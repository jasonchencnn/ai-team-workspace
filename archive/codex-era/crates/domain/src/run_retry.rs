//! Automatic **retry admission** for the §3 Run lifecycle.
//!
//! Frozen source: `ERROR_CATALOG.md` §11, with `STATE_MACHINES.md` §3's last
//! line and `DATA_CONVENTIONS.md` §4's idempotency rules.
//!
//! # This module admits or refuses; it does not schedule
//!
//! The backoff arithmetic already exists and is frozen by DEV-02-14 in
//! [`RetryPolicy`] / [`RetryBudget`](crate::outbox_dispatch::RetryBudget).
//! Re-deriving it here would be a second
//! answer to the same question, and the two would drift — which is exactly how
//! `0006`'s `max_retries DEFAULT 5` came to contradict §11's 默认最多 3 次. So
//! this module reuses that type verbatim and only answers the question §11 puts
//! *before* arithmetic matters: **is this failure eligible for an automatic
//! retry at all?**
//!
//! # §11's four rules, and the one that is not §11
//!
//! ```text
//! 1. 只有目录中 retryable=是 且命令本身幂等时允许自动重试。
//! 2. 自动重试默认最多 3 次、指数退避并加入 jitter；总时长不得超过命令/Message TTL。
//! 3. 权限、校验、非法状态、版本冲突、fencing、TTL 过期不自动重试。
//! 4. INTERNAL_UNEXPECTED 后先按 idempotency key 查询首次结果，
//!    禁止盲目重放可能产生外部副作用的命令。
//! ```
//!
//! Quoted whole. An earlier version of this comment truncated rule 4 right
//! after 禁止盲目重放 — dropping the half that says *why* the rule exists,
//! which is the part a reader needs in order to judge whether a given replay is
//! safe. Q-13 checks every quoted fragment in this file against the frozen
//! specifications, which is what caught it.
//!
//! Rule 3 reads as a restatement of rule 1's negative side, and for the Run
//! machine it is. It is kept as a test rather than a second branch, because a
//! separate rule-3 list would be a hand-kept copy of the catalogue's
//! `Retryable` column — the same duplication that produced `is_retryable()` in
//! the first place.
//!
//! How much of rule 3 is actually reachable is worth being exact about, because
//! an earlier version of this comment over-claimed. Of rule 3's six concepts,
//! only three have a §3 Run code at all:
//!
//! | §11 rule 3 concept | §3 Run code | Catalogue |
//! |---|---|---|
//! | 权限 | `AUTH_PERMISSION_DENIED` | `否` |
//! | 非法状态 | `DOMAIN_INVALID_TRANSITION` | `否` |
//! | fencing | `RUN_FENCED` | `否` |
//! | 版本冲突 | — | `CONCURRENCY_VERSION_CONFLICT` (非 §3 码) |
//! | 校验 | — | `VALIDATION_*` (§2, 非 §3 码) |
//! | TTL 过期 | — | `MESSAGE_TTL_EXPIRED` (§9, 非 §3 码) |
//!
//! 版本冲突 is worth calling out: the obvious candidate `BINDING_STALE_VERSION`
//! is *not* a version-conflict code. It means the Binding moved on, and it is
//! §3's `STARTING → RUNNING` rejection. Mapping rule 3's 版本冲突 onto it would
//! be an invention dressed as a lookup. The last three concepts never reach this
//! module at all, because the Run machine cannot raise them — so "they are
//! never retried" is a property of the catalogue, not something decided here.
//!
//! # 每次新 Run, by construction
//!
//! §3 closes with 自动重试只读取终态 Run，创建新 Run；禁止把旧 Run 改回 QUEUED.
//! No variant of [`RetryDecision`] can express a transition **of the source
//! Run**: the only positive outcome names a [`RunCreation`], whose initial state
//! is the frozen `CREATE_QUEUED`. "把旧 Run 改回 QUEUED" is unrepresentable, not
//! merely discouraged — the same trick DEV-02-14 used for §3's conflict result,
//! which has no `next` field for exactly this reason.

use std::fmt;

use crate::outbox_dispatch::{RetryBudget, RetryPolicy};
use crate::run_state_machine::{CREATE_QUEUED, RunCreation, RunErrorCode, RunStatus};

/// Why a Run ended, as far as retry admission is concerned.
///
/// Kept separate from [`RunErrorCode`] because `INTERNAL_UNEXPECTED` is a system
/// code in the catalogue and is deliberately not one of §3's eight: adding it
/// there would claim it is a legal edge in the §3 graph, which it is not.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RunFailure {
    /// A §3 rejection code. Admitted only if the catalogue marks it `是`.
    Code(RunErrorCode),
    /// `INTERNAL_UNEXPECTED` — `ERROR_CATALOG.md` marks it `是`, but §11's
    /// fourth rule overrides that column: query the first result by idempotency
    /// key before anything else. 禁止盲目重放可能产生外部副作用的命令.
    InternalUnexpected,
}

impl RunFailure {
    /// The catalogue wire name, for diagnostics.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Code(code) => code.as_str(),
            Self::InternalUnexpected => "INTERNAL_UNEXPECTED",
        }
    }
}

/// The verdict on whether an automatic retry may be created.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RetryDecision {
    /// §3: 创建新 Run. The previous Run is read-only from here.
    CreateNewRun {
        /// The terminal Run being retried, echoed for diagnostics. It is an
        /// input, never a mutation target.
        source: RunStatus,
        /// The creation contract for the new Run. Always `CREATE_QUEUED`:
        /// §3 gives exactly one creation target and forbids reusing the old Run.
        creation: RunCreation,
    },
    /// §11's fourth rule: the first result must be queried by idempotency key
    /// before any replay is considered.
    ///
    /// Not a refusal and not an admission — a third thing. DEV-02-14's
    /// [`IdempotencyOutcome`](crate::idempotency::IdempotencyOutcome) answers the query;
    /// only its `Proceed` may become a `CreateNewRun`.
    QueryFirstResult {
        /// The terminal Run being retried.
        source: RunStatus,
    },
    /// The catalogue marks this code `否`, so §11's first rule withholds it.
    ///
    /// This is also where §11's third rule lands, for the three of its six
    /// concepts the Run machine can actually raise: 权限, 非法状态 and fencing.
    /// 校验, 版本冲突 and TTL 过期 have no §3 Run code, so they cannot reach this
    /// variant — see the module docs for why `BINDING_STALE_VERSION` is not
    /// standing in for 版本冲突.
    DoNotRetry {
        /// The frozen code, so the caller can log why.
        code: RunErrorCode,
    },
    /// §11's first rule's other half: the catalogue says `是`, but 命令本身
    /// 非幂等, so an automatic replay is not allowed.
    NotIdempotent {
        /// The terminal Run being considered.
        source: RunStatus,
    },
    /// §3: 自动重试只读取终态 Run. A Run that has not reached a terminal state
    /// is not retry material at all.
    NotTerminal {
        /// The non-terminal status observed.
        status: RunStatus,
    },
    /// §11's second rule: one of its two bounds on automatic retry is spent.
    ///
    /// [`RetryLimit`] rather than a flag because a caller cannot recompute which
    /// bound fired without repeating this function, and the two have different
    /// remedies: the count is a policy decision, the duration is the command's
    /// or Message's TTL arriving.
    BudgetExhausted {
        /// Which of §11 rule 2's two bounds stopped the retry.
        limit: RetryLimit,
        /// The delay the refused attempt would have waited, already truncated by
        /// the budget. Non-zero under [`RetryLimit::Count`] — the ceiling is
        /// reached with budget to spare — so it is not a usable "nothing left
        /// to wait for" signal.
        delay_millis: i64,
    },
}

/// §11 rule 2's two bounds on automatic retry.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RetryLimit {
    /// 默认最多 3 次 — [`RetryPolicy::max_retries`] reached.
    Count,
    /// 总时长不得超过命令/Message TTL — the budget's total is spent.
    Duration,
}

impl RetryDecision {
    /// The new Run's creation contract, if this decision creates one.
    pub const fn created_run(self) -> Option<RunCreation> {
        match self {
            Self::CreateNewRun { creation, .. } => Some(creation),
            _ => None,
        }
    }

    /// `true` only for the one outcome that writes a new Run.
    pub const fn creates_new_run(self) -> bool {
        matches!(self, Self::CreateNewRun { .. })
    }
}

/// Decide whether an automatic retry may be created for a finished Run.
///
/// `retries_so_far` counts retries **already made**, matching
/// [`RetryPolicy::backoff_millis`]'s convention, and `spent_millis` is what the
/// [`RetryBudget`] has accumulated so far.
///
/// # Precedence is this module's choice, not §11's
///
/// §11 lists four rules without ordering them, so where two apply the order had
/// to be picked. A definitive, checkable fact is reported before a contingent
/// one: the catalogue's `Retryable` column is frozen text, whereas
/// `command_is_idempotent` is a property the caller asserts. That is why
/// [`RetryDecision::DoNotRetry`] precedes [`RetryDecision::NotIdempotent`] even
/// though rule 1 mentions the catalogue first — a non-retryable code refuses
/// regardless of idempotency, and naming it is the more useful diagnostic.
pub fn decide_retry(
    source: RunStatus,
    failure: RunFailure,
    command_is_idempotent: bool,
    policy: RetryPolicy,
    retries_so_far: u32,
    spent_millis: i64,
) -> RetryDecision {
    // §3: 自动重试只读取终态 Run. Checked first because every other question is
    // about a Run that is allowed to exist as retry material.
    if !source.is_terminal() {
        return RetryDecision::NotTerminal { status: source };
    }

    // §11 rule 1, first half: 目录中 retryable=是.
    if let RunFailure::Code(code) = failure
        && !code.is_retryable()
    {
        return RetryDecision::DoNotRetry { code };
    }

    // §11 rule 4. Checked before idempotency because 禁止盲目重放 is absolute:
    // it does not become permissible just because the command happens to be
    // idempotent. The catalogue marks `INTERNAL_UNEXPECTED` `是`, so rule 1
    // admits it — rule 4 is what makes the answer a query rather than a replay.
    if failure == RunFailure::InternalUnexpected {
        return RetryDecision::QueryFirstResult { source };
    }

    // §11 rule 1, second half: 且命令本身幂等.
    if !command_is_idempotent {
        return RetryDecision::NotIdempotent { source };
    }

    // §11 rule 2: 默认最多 3 次, plus the total-duration budget. Counted first so
    // the reported limit is the one that actually applies when both are spent.
    let budget: RetryBudget = policy.budget_after(spent_millis);
    let delay_millis = budget.next_delay_millis(retries_so_far);
    if retries_so_far >= policy.max_retries {
        return RetryDecision::BudgetExhausted {
            limit: RetryLimit::Count,
            delay_millis,
        };
    }
    if budget.is_exhausted() {
        return RetryDecision::BudgetExhausted {
            limit: RetryLimit::Duration,
            delay_millis,
        };
    }

    RetryDecision::CreateNewRun {
        source,
        creation: CREATE_QUEUED,
    }
}

impl fmt::Display for RetryDecision {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::CreateNewRun { source, .. } => {
                write!(formatter, "create a new Run from terminal {source}")
            }
            Self::QueryFirstResult { source } => write!(
                formatter,
                "§11 rule 4: query the first result by idempotency key before replaying {source}"
            ),
            Self::DoNotRetry { code } => {
                write!(formatter, "not auto-retryable: {code}")
            }
            Self::NotIdempotent { source } => {
                write!(
                    formatter,
                    "command is not idempotent, so {source} is not replayed"
                )
            }
            Self::NotTerminal { status } => write!(
                formatter,
                "§3: 自动重试只读取终态 Run, and {status} is not one"
            ),
            Self::BudgetExhausted {
                limit,
                delay_millis,
            } => write!(
                formatter,
                "§11 retry limit spent ({limit:?}, next delay would be {delay_millis} ms)"
            ),
        }
    }
}
