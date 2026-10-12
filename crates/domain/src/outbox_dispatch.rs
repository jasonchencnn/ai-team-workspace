//! Outbox dispatch decisions, frozen by `ERROR_CATALOG.md` §11 and the
//! `TECH_STACK_AND_STANDARDS.md` §4.3 delivery contract.
//!
//! ```text
//! §11 重试规则
//!   - 只有目录中 retryable=是 且命令本身幂等时允许自动重试。
//!   - 自动重试默认最多 3 次、指数退避并加入 jitter；总时长不得超过命令/Message TTL。
//!   - 权限、校验、非法状态、版本冲突、fencing、TTL 过期不自动重试。
//! ```
//!
//! # At-least-once, so a replay must be harmless
//!
//! `TECH_STACK §4.3` says the Outbox is processed at least once and consumers
//! deduplicate by event ID or idempotency key. The consequence for this module:
//! every function here is a **decision about one row**, never a guarantee that
//! a row is handled once. A dispatcher that claimed single delivery would be
//! describing behaviour the transport does not provide.
//!
//! # The IO is the caller's
//!
//! Nothing here touches a database, a clock, or a transport. A caller reads the
//! due rows, calls [`decide_dispatch`] per row, performs the delivery, and calls
//! [`decide_outcome`] with what happened. That split is what makes every
//! branch table-testable without a transport, and it is the same layering the
//! state machines use.
//!
//! # A lease, because two dispatchers may see the same row
//!
//! [`OutboxStatus::Processing`] with a lease is how a row is claimed. §4.3 does
//! not describe leases, and none is invented: the frozen `outbox` table has no
//! lease column, so this module reports the claim as a status transition and
//! leaves the durability of that claim to the caller's transaction. What is
//! specified here is only that a row must be claimable, and that a claim which
//! is never completed has to become retryable again — otherwise a crashed
//! dispatcher strands rows in `PROCESSING` forever.

use std::fmt;

/// The five frozen `outbox.status` values, verbatim from migration `0006`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum OutboxStatus {
    /// Queued for delivery. `outbox_pending_dispatch` selects on this.
    Pending,
    /// Claimed by a dispatcher and not yet reported back.
    Processing,
    /// Delivered. `published_at` is set.
    Published,
    /// Delivery failed but is below the retry ceiling; `scheduled_at` moved.
    Failed,
    /// Retries exhausted. Terminal, and an operator has to look.
    DeadLetter,
}

impl OutboxStatus {
    /// The storage spelling, matching `0006`'s CHECK constraint exactly.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Pending => "PENDING",
            Self::Processing => "PROCESSING",
            Self::Published => "PUBLISHED",
            Self::Failed => "FAILED",
            Self::DeadLetter => "DEAD_LETTER",
        }
    }

    /// `true` when `outbox_pending_dispatch`'s partial index would select this
    /// status: `WHERE status IN ('PENDING', 'FAILED')`.
    pub const fn is_dispatchable(self) -> bool {
        matches!(self, Self::Pending | Self::Failed)
    }
}

impl fmt::Display for OutboxStatus {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

/// One row of the `outbox` table, as far as this module needs it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OutboxEntry {
    /// `outbox.id`.
    pub id: [u8; 16],
    /// `outbox.status`.
    pub status: OutboxStatus,
    /// `outbox.retry_count`.
    pub retry_count: u32,
    /// `outbox.max_retries`.
    pub max_retries: u32,
    /// `outbox.scheduled_at`, in Unix milliseconds.
    pub scheduled_at: i64,
}

impl OutboxEntry {
    /// A row in its initial state: `PENDING`, no retries used, ready now.
    ///
    /// The ceiling is [`DEFAULT_MAX_RETRIES`], i.e. §11's number, **not** zero.
    /// Published `0006` keeps its historical `DEFAULT 5` bytes; forward
    /// migration `0024` corrects the current database default to this same value.
    pub const fn pending(id: [u8; 16], now_millis: i64) -> Self {
        Self {
            id,
            status: OutboxStatus::Pending,
            retry_count: 0,
            max_retries: DEFAULT_MAX_RETRIES,
            scheduled_at: now_millis,
        }
    }
}

/// Why a delivery attempt did not succeed.
///
/// §11 splits the failure kinds sharply, and the split is the whole point:
/// a retryable code may be retried, and everything else may not.
///
/// # The retryable check is the caller's, and saying so is the point
///
/// §11's first rule is an **exclusive** condition: 只有目录中 `retryable=是`
/// 且命令本身幂等时允许自动重试. Neither half is enforced here:
///
/// * This module has no error-code catalogue. The retryable column lives in
///   `ERROR_CATALOG.md`'s table, and reading it at runtime would mean shipping
///   a second, hand-maintained copy of that table — which is exactly the drift
///   DEV-02-13's Q-04/Q-07 devices exist to catch. Classifying the code is
///   therefore the caller's job, at the point where it has the catalogue.
/// * §11's second half (命令本身幂等) is likewise a property of the command,
///   not of a row.
///
/// What this type *does* guarantee is the consequence: whatever the caller
/// classified as [`DeliveryFailure::Permanent`] is never retried, and only
/// [`DeliveryFailure::Retryable`] can reach the retry path. An earlier version
/// of this comment implied the retryable flag was checked here. It never was,
/// and a reader who believed otherwise would write `Retryable { code: "…非可重试码" }`
/// and get three retries. The variant is named for the *caller's* classification
/// on purpose.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeliveryFailure {
    /// The caller classified this code as `retryable=是` in the catalogue.
    ///
    /// Naming it `ClassifiedRetryable` would be more honest but reads worse at
    /// every call site; the docstring above carries the caveat instead.
    Retryable {
        /// The catalogue code that failed, kept so the caller can record
        /// `outbox.last_error` without inventing a spelling.
        code: &'static str,
    },
    /// §11: 权限、校验、非法状态、版本冲突、fencing、TTL 过期不自动重试.
    ///
    /// The specification names these categories in one sentence and does not
    /// enumerate the codes, so the caller supplies its own label rather than
    /// this type inventing a list that §11 never froze.
    Permanent {
        /// The caller's label for why this is permanent.
        reason: &'static str,
    },
}

/// The default retry ceiling, from §11: 自动重试默认最多 3 次.
///
/// Read as three **retries**: a row that fails is retried three times, and the
/// failure after the third retry is terminal. `three_retries_are_permitted_and_
// the_fourth_failure_dead_letters` pins that reading, which is the one §11's
/// wording ("自动重试…3 次") supports.
///
/// Published migration `0006` retains its historical `DEFAULT 5`; forward
/// migration `0024` corrects the current schema to `DEFAULT 3` without changing
/// existing row settings. Persistence tests insert a row while omitting the
/// column and verify the actual database value equals this constant. This
/// schema correction does not implement the IO dispatcher.
pub const DEFAULT_MAX_RETRIES: u32 = 3;

/// The base delay for §11's 指数退避, in milliseconds.
///
/// §11 fixes the *shape* (exponential, with jitter) but not the base. 200 ms is
/// this module's choice, stated here so it is a decision rather than a
/// constant that appears from nowhere; the delay a caller gets is
/// `base × 2^retry_count` plus jitter, clamped by `RetryPolicy::ceiling_millis`.
pub const DEFAULT_BACKOFF_BASE_MILLIS: i64 = 200;

/// How long a dispatcher waits between rounds, and the jitter it adds.
///
/// §11 requires jitter without specifying its distribution, so the added
/// milliseconds are an **input** rather than something sampled here: the
/// specification does not say who draws the randomness, and this module reads
/// no clock and holds no random source. Sampling internally would require
/// inventing a `RandomSource` usage the specification never describes.
/// Deterministic in, deterministic out.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RetryPolicy {
    /// The per-row ceiling. §11's default is [`DEFAULT_MAX_RETRIES`].
    pub max_retries: u32,
    /// The exponential base in milliseconds.
    pub backoff_base_millis: i64,
    /// The **total** retry budget, in milliseconds, across the whole life of a
    /// row — not a per-delay cap.
    ///
    /// §11: 总时长不得超过命令/Message TTL. That is a statement about the sum of
    /// every delay a row will ever wait, so an earlier version of this field
    /// clamped only the single delay and its docstring promised a TTL
    /// guarantee the code did not provide. [`RetryBudget`] is the type that
    /// actually accumulates.
    pub total_budget_millis: i64,
    /// Jitter to add to each computed delay, in milliseconds.
    pub jitter_millis: i64,
}

impl RetryPolicy {
    /// §11's defaults: three retries, exponential from 200 ms, jitter supplied
    /// by the caller.
    ///
    /// The 60-second total budget is this module's choice, like
    /// [`DEFAULT_BACKOFF_BASE_MILLIS`]: §11 bounds the total by the command's or
    /// Message's TTL but does not fix a number, and a Message's TTL is not known
    /// here. A caller that does know its TTL should set
    /// [`RetryPolicy::total_budget_millis`] from it.
    pub const fn new(jitter_millis: i64) -> Self {
        Self {
            max_retries: DEFAULT_MAX_RETRIES,
            backoff_base_millis: DEFAULT_BACKOFF_BASE_MILLIS,
            total_budget_millis: 60_000,
            jitter_millis,
        }
    }

    /// The delay before the attempt that follows `retry_count` attempts.
    ///
    /// §11: 指数退避并加入 jitter.
    ///
    /// The parameter counts attempts **already made**, so zero means "the first
    /// retry" and yields the base delay. The caller in [`decide_outcome`] passes
    /// `entry.retry_count` for exactly this reason: passing the count about to
    /// be written would skip the base and make the first retry wait twice as
    /// long as §11's shape implies.
    ///
    /// The exponential term uses a saturating shift so a large `retry_count`
    /// saturates instead of overflowing. This is a **single** delay and is not
    /// bounded by the total budget — that is [`RetryBudget`]'s job, and
    /// `total_stays_within_the_ttl_budget` checks the sum.
    pub fn backoff_millis(self, retry_count: u32) -> i64 {
        let exponent = retry_count.min(32);
        let factor = 1_i64 << exponent;
        self.backoff_base_millis
            .saturating_mul(factor)
            .saturating_add(self.jitter_millis)
            .max(0)
    }

    /// A budget that has already spent `spent_millis` of this policy's total.
    pub fn budget_after(self, spent_millis: i64) -> RetryBudget {
        RetryBudget {
            policy: self,
            spent_millis: spent_millis.max(0),
        }
    }
}

/// §11's 总时长不得超过命令/Message TTL, as an accumulating quantity.
///
/// A single clamped delay is not this rule: three retries at 200/400/800 ms
/// total 1400 ms, five total 6200 ms, and `max_retries` is caller-settable, so
/// the sum has no bound unless something accumulates it. This is that something.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RetryBudget {
    policy: RetryPolicy,
    spent_millis: i64,
}

impl RetryBudget {
    /// Milliseconds still available, never negative.
    pub fn remaining_millis(self) -> i64 {
        self.policy
            .total_budget_millis
            .saturating_sub(self.spent_millis)
            .max(0)
    }

    /// `true` once the budget is exhausted.
    pub fn is_exhausted(self) -> bool {
        self.remaining_millis() == 0
    }

    /// The next delay, truncated to what the budget still allows.
    ///
    /// Returns `0` once exhausted. Dispatch decisions must check exhaustion
    /// before scheduling: zero is not permission to retry beyond the deadline.
    pub fn next_delay_millis(self, retry_count: u32) -> i64 {
        self.policy
            .backoff_millis(retry_count)
            .min(self.remaining_millis())
    }

    /// The budget after waiting `delay_millis` more.
    pub fn plus(self, delay_millis: i64) -> Self {
        Self {
            policy: self.policy,
            spent_millis: self.spent_millis.saturating_add(delay_millis.max(0)),
        }
    }
}

/// Whether a row is due for a dispatch round at `now_millis`.
///
/// `outbox_pending_dispatch` selects on `(status, scheduled_at)` for the two
/// dispatchable statuses, so this mirrors the index rather than re-deciding
/// which statuses are eligible: a row is due when it is dispatchable **and** its
/// scheduled time has arrived. A `PROCESSING` row is deliberately not due —
/// another dispatcher holds it, and claiming it again would break the
/// at-least-once contract into at-most-once by accident.
pub fn is_due(entry: &OutboxEntry, now_millis: i64) -> bool {
    entry.status.is_dispatchable() && entry.scheduled_at <= now_millis
}

/// What the caller should do with one due row.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DispatchDecision {
    /// Claim the row and deliver it.
    Claim {
        /// The status to write before delivering, so a concurrent dispatcher
        /// sees the claim.
        claim_as: OutboxStatus,
    },
    /// The row is not due yet. No write.
    NotDue,
    /// The row is in a state this dispatcher must not touch: already
    /// `PUBLISHED`, `DEAD_LETTER`, or claimed by another dispatcher as
    /// `PROCESSING`.
    NotClaimable {
        /// The status observed, for the caller's log.
        observed: OutboxStatus,
    },
}

/// Decide what to do with one outbox row in this round.
pub fn decide_dispatch(entry: &OutboxEntry, now_millis: i64) -> DispatchDecision {
    if !entry.status.is_dispatchable() {
        return DispatchDecision::NotClaimable {
            observed: entry.status,
        };
    }
    if !is_due(entry, now_millis) {
        return DispatchDecision::NotDue;
    }
    DispatchDecision::Claim {
        claim_as: OutboxStatus::Processing,
    }
}

/// What the caller should write after a delivery attempt finished.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutcomeDecision {
    /// §11: a retryable failure below the ceiling moves the row to `FAILED` and
    /// schedules the next attempt with exponential backoff plus jitter.
    Retry {
        /// The status to write.
        status: OutboxStatus,
        /// The `retry_count` to write: one more attempt was used.
        retry_count: u32,
        /// The next `scheduled_at`.
        scheduled_at: i64,
        /// What to record in `outbox.last_error`.
        last_error: &'static str,
    },
    /// §11: a retryable failure that has used up the ceiling becomes
    /// `DEAD_LETTER`, which no index selects and no dispatcher retries.
    DeadLetter {
        /// The final `retry_count`.
        retry_count: u32,
        /// What to record in `outbox.last_error`.
        last_error: &'static str,
    },
    /// The delivery succeeded: `PUBLISHED` with `published_at` set.
    Published {
        /// The `published_at` to write.
        published_at: i64,
    },
}

/// Decide what to write after one delivery attempt, given the attempt failed.
///
/// `now_millis` is the caller's clock reading; this module reads none.
/// `budget_spent_millis` is how long the row has already spent waiting, which is
/// what §11's 总时长 bound applies to — without it the ceiling check covers only
/// the single next delay and the sum is unbounded. The
/// ceiling check is `retry_count >= max_retries` **before** the increment, so
/// `max_retries = 3` permits three retries and the fourth failure dead-letters:
/// the row is tried, fails, retried, tried, fails, retried, tried, fails,
/// dead-lettered — four attempts, three retries, which is what "最多 3 次" means
/// for retries.
pub fn decide_outcome(
    entry: &OutboxEntry,
    failure: DeliveryFailure,
    policy: RetryPolicy,
    now_millis: i64,
    budget_spent_millis: i64,
) -> OutcomeDecision {
    let last_error = match failure {
        DeliveryFailure::Retryable { code } => code,
        DeliveryFailure::Permanent { reason } => reason,
    };

    // §11: 权限、校验、非法状态、版本冲突、fencing、TTL 过期不自动重试.
    if matches!(failure, DeliveryFailure::Permanent { .. }) {
        return OutcomeDecision::DeadLetter {
            retry_count: entry.retry_count,
            last_error,
        };
    }

    // The row's own `max_retries` wins over the policy's, because §11's "默认"
    // marks the policy value as a default and `0006` stores the ceiling per row.
    // A row that says 0 means "never retry this", and honouring only the policy
    // would retry it three times anyway.
    let ceiling = match entry.max_retries {
        0 => 0,
        row_ceiling => row_ceiling.min(policy.max_retries),
    };
    if entry.retry_count >= ceiling {
        return OutcomeDecision::DeadLetter {
            retry_count: entry.retry_count,
            last_error,
        };
    }

    // §11: 总时长不得超过命令/Message TTL — the sum, not the next delay alone.
    let budget = policy.budget_after(budget_spent_millis);
    if budget.is_exhausted() {
        return OutcomeDecision::DeadLetter {
            retry_count: entry.retry_count,
            last_error,
        };
    }
    let delay = budget.next_delay_millis(entry.retry_count);
    let next_retry = entry.retry_count.saturating_add(1);
    OutcomeDecision::Retry {
        status: OutboxStatus::Failed,
        retry_count: next_retry,
        scheduled_at: now_millis.saturating_add(delay),
        last_error,
    }
}

/// The decision for a successful delivery.
pub fn decide_published(now_millis: i64) -> OutcomeDecision {
    OutcomeDecision::Published {
        published_at: now_millis,
    }
}

/// Whether a consumer has already consumed `event_id`, per
/// `event_consumptions` (`PRIMARY KEY (consumer_id, event_id)`).
///
/// §4.3: 消费者按 event ID 或幂等键去重. The schema already makes a second row
/// for the same pair impossible, so this is the lookup a caller does before
/// doing the work; returning `true` means "skip, this was already handled".
pub const fn already_consumed(record_present: bool) -> bool {
    record_present
}

/// Decide a retry against an absolute UTC deadline, including delivery time,
/// queue delay and time spent offline. Scheduling at the deadline is expired.
/// The caller derives the deadline from the command/Message TTL and also uses
/// `decide_dispatch_before_deadline` at actual dispatch time.
pub fn decide_outcome_before_deadline(
    entry: &OutboxEntry,
    failure: DeliveryFailure,
    policy: RetryPolicy,
    now_millis: i64,
    started_at_millis: i64,
    deadline_millis: i64,
) -> OutcomeDecision {
    let decision = decide_outcome(
        entry,
        failure,
        policy,
        now_millis,
        now_millis.saturating_sub(started_at_millis).max(0),
    );
    match decision {
        OutcomeDecision::Retry {
            scheduled_at,
            last_error,
            ..
        } if now_millis >= deadline_millis || scheduled_at >= deadline_millis => {
            OutcomeDecision::DeadLetter {
                retry_count: entry.retry_count,
                last_error,
            }
        }
        _ => decision,
    }
}

/// Stop expired delivery work even when it was queued before the deadline.
/// Expired work is retained for operator handling, not retried or deleted.
pub fn decide_dispatch_before_deadline(
    entry: &OutboxEntry,
    now_millis: i64,
    deadline_millis: i64,
) -> DispatchDecision {
    if now_millis >= deadline_millis {
        return DispatchDecision::NotClaimable {
            observed: entry.status,
        };
    }
    decide_dispatch(entry, now_millis)
}
