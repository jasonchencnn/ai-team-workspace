//! Fencing generation, lease renewal, and write admission for §3 Runs.
//!
//! Frozen source: `TECH_STACK_AND_STANDARDS.md` §5, with `DATA_CONVENTIONS.md`
//! §2 and `STATE_MACHINES.md` §3. The load-bearing sentence is §5's second:
//!
//! ```text
//! 心跳只能延长匹配当前 token 的租约；接管/超时生成新 token，旧执行者所有写入返回 RUN_FENCED。
//! ```
//!
//! # What the supervisor already does, and what it structurally cannot do
//!
//! `adapter-supervisor` validates the `fencing_token` on every notification —
//! but against the token **it was configured with at spawn time**:
//!
//! ```text
//! crates/adapter-supervisor/src/protocol.rs:1089 → pub fn new(expected_context: RunContext) -> Self {
//! crates/adapter-supervisor/src/protocol.rs:1223 → if fencing_token != self.expected_context.fencing_token {
//! ```
//!
//! It never re-reads the current generation. So this sequence passes every check
//! it makes:
//!
//! A timeline, not a quotation, so it is written as an **indented** block. Two
//! constraints meet here:
//!
//!   * a bare ``` fence is a doctest, and rustdoc tries to compile this as Rust
//!     (`error: unknown start of token`) — a ```text fence avoids that but is
//!     exactly what the quote-fidelity device reads as "this claims to be
//!     verbatim from a frozen document", which it does not;
//!   * an indented block is neither. It is not compiled and it is not a ```text
//!     fence, so the device skips it without needing an exemption.
//!
//!     t0  Run A takes Task T, token = 7; supervisor_A starts with expected = 7
//!     t1  A wedges; §3 mints token = 8; Run B takes T; supervisor_A still runs
//!     t2  A wakes and reports a heartbeat: presented 7 == expected 7 → accepted
//!
//! `0012`'s guards do not close this. They check that a Task never has two
//! *active Run records*, and at `t2` A has already been fenced to a terminal
//! state, so the record layer genuinely has one active Run. `0012` stops the
//! duplicate record; nothing stops the process that record used to belong to.
//!
//! **The comparison has to be against the current generation, not against a
//! snapshot of it.** That is the entire reason this module exists.
//!
//! # Compare generations; never use `!=`
//!
//! `DATA_CONVENTIONS.md` §2 freezes `fencing_token` as 「每 Task 执行世代单调递增整数」
//! and explicitly forbids substituting a random Session ID. Monotonicity makes
//! three states distinguishable, and they are not the same problem:
//!
//! | presented vs current | Meaning | Verdict |
//! |---|---|---|
//! | `<` | a previous executor, normal after a takeover or timeout | `RUN_FENCED` |
//! | `==` | the current executor | admitted, subject to the lease window |
//! | `>` | impossible under §2; a data fault, not a takeover | `RUN_FENCED`, loudly |
//!
//! `!=` collapses the first and third rows. That is not merely imprecise: an
//! executor presenting a *future* token means the generation allocator or the
//! caller's read is wrong, and that deserves a different signal than a routine
//! takeover. [`Generation::classify`] keeps them apart.
//!
//! # Fenced and lease-expired are two different refusals
//!
//! §5 says 旧执行者所有写入返回 `RUN_FENCED` — that is about *identity*.
//! `RUN_LEASE_EXPIRED` is about the *time window*: the catalogue gives it the
//! details field `expired_at` and a remedy of 「新 Run」.
//!
//! An executor holding the current token whose lease has already expired is
//! still the current executor, so it is not fenced. But its heartbeat must not
//! renew the lease: if it could, leases would never expire. Hence two codes, and
//! hence [`decide_renewal`] refusing [`LeaseRefusal::Expired`] rather than
//! extending.

use std::fmt;

use crate::run_state_machine::{RunErrorCode, RunStatus};

/// One of the three write kinds the node standard names.
///
/// 输出 is not a §3 state transition, so it has no row of its own; it is listed
/// here because the node standard requires 旧 token 的心跳、输出和完成都被拒.
/// Modelling it as a peer of the other two rather than inventing a transition
/// is a **derived reading** of that sentence, not a §3 row.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RunWrite {
    /// 心跳 — also the lease-renewal trigger (§5: 心跳只能延长匹配当前 token 的租约).
    Heartbeat,
    /// 输出 — a `RunOutput` notification. Not a state transition.
    Output,
    /// 完成 — the `RUNNING → SUCCEEDED` / `→ FAILED` path.
    Completion,
}

impl RunWrite {
    /// Every write kind, in a stable order.
    pub const ALL: [RunWrite; 3] = [RunWrite::Heartbeat, RunWrite::Output, RunWrite::Completion];
}

/// The one frozen refusal a lease or fencing check can return.
///
/// `RUN_FENCED` is [`RunErrorCode::RunFenced`], already a §3 rejection code on
/// three rows. `RUN_LEASE_EXPIRED` is **not** in [`RunErrorCode`] and is
/// deliberately not added there: §3 has no row that raises it, and the test
/// `every_code_the_run_machine_can_raise_is_enumerated` requires every code in
/// `RunErrorCode::ALL` to be raisable by `RUN_EDGES` or `CREATE_QUEUED`.
/// Adding it would have broken an invariant DEV-03-06 established — so it
/// travels here as its own shape with its frozen wire name, the same way
/// `INTERNAL_UNEXPECTED` does in `run_retry`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LeaseRefusal {
    /// `RUN_FENCED` — 「此执行已失去写入权限，结果不会被接收」.
    Fenced,
    /// `RUN_LEASE_EXPIRED` — 「执行租约已过期，需要重新领取任务」.
    Expired,
}

impl LeaseRefusal {
    /// The frozen catalogue wire name.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Fenced => "RUN_FENCED",
            Self::Expired => "RUN_LEASE_EXPIRED",
        }
    }

    /// The `RunErrorCode` for refusals that §3 already names.
    ///
    /// `None` for [`Self::Expired`], which is the honest answer rather than a
    /// fallback: no §3 row produces it, so there is no code to map it to.
    pub const fn as_run_error_code(self) -> Option<RunErrorCode> {
        match self {
            Self::Fenced => Some(RunErrorCode::RunFenced),
            Self::Expired => None,
        }
    }
}

impl fmt::Display for LeaseRefusal {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

/// How a presented `fencing_token` relates to the Task's current one.
///
/// A total order on the three cases rather than a bool, because the first and
/// third need different signals and `!=` erases the difference.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Generation {
    /// `presented == current`: this is the current executor.
    Current,
    /// `presented < current`: a previous executor, after a takeover or timeout.
    Stale,
    /// `presented > current`: impossible under `DATA_CONVENTIONS` §2's
    /// monotonic generations, so it indicates a data fault rather than a
    /// takeover. Refused like a stale one, but never described as one.
    Ahead,
}

impl Generation {
    /// Every case, in a stable order.
    pub const ALL: [Generation; 3] = [Generation::Current, Generation::Stale, Generation::Ahead];

    /// Classify a presented token against the Task's current generation.
    ///
    /// §2 freezes generations as monotonically increasing integers starting at
    /// 1 (`0004` enforces `fencing_token >= 1`), so `0` can only be "nothing was
    /// ever minted" and is [`Self::Stale`] like any other past value.
    pub const fn classify(presented: u64, current: u64) -> Self {
        if presented == current {
            Self::Current
        } else if presented < current {
            Self::Stale
        } else {
            Self::Ahead
        }
    }

    /// `true` only for the current executor.
    pub const fn is_current(self) -> bool {
        matches!(self, Self::Current)
    }
}

/// The lease as the database currently holds it.
///
/// Ids are omitted deliberately: the caller has already resolved the row, and
/// every decision here is a function of `(token, now)`. Carrying them would add
/// construction noise to tests without testing anything.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LeaseSnapshot {
    /// The Run's `fencing_token`; §2 makes it the execution generation.
    pub fencing_token: u64,
    /// `acquired_at`, Unix milliseconds.
    pub acquired_at_ms: i64,
    /// `expires_at`, Unix milliseconds. `0004` enforces `expires_at >= acquired_at`.
    pub expires_at_ms: i64,
}

impl LeaseSnapshot {
    /// `true` once the lease's window has closed at `now_ms`.
    ///
    /// Boundary is inclusive of `expires_at`: a lease is valid *through* the
    /// instant it expires, and `0004` stores `expires_at >= acquired_at` as a
    /// minimum-duration contract, so treating the endpoint as still-valid keeps
    /// a zero-length window from being both valid and expired.
    pub const fn is_expired_at(self, now_ms: i64) -> bool {
        now_ms > self.expires_at_ms
    }
}

/// Refusal of an ordinary Adapter callback. Existing catalogue codes only.
///
/// UNKNOWN callbacks require the separate reconcile use case. Terminal Run
/// history cannot be extended by an ordinary callback.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RunWriteRefusal {
    /// The token is unminted, stale or ahead of the current generation.
    Fenced,
    /// Ordinary callbacks are frozen until the UNKNOWN Run is reconciled.
    ReconciliationRequired,
    /// Terminal history must not be extended by callbacks.
    Terminal,
}

impl RunWriteRefusal {
    /// Stable catalogue wire name; this adds no protocol error code.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Fenced => "RUN_FENCED",
            Self::ReconciliationRequired => "RUN_STATUS_UNKNOWN",
            Self::Terminal => "DOMAIN_INVALID_TRANSITION",
        }
    }
}

impl fmt::Display for RunWriteRefusal {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

/// The verdict on a write from an executor.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WriteDecision {
    /// Generation and Run-state checks passed, not complete write authorization.
    /// The application must still verify the lease, Binding, scope, permissions
    /// and sequence in the transaction applying the effect.
    Accept {
        /// The write kind that was admitted, echoed for diagnostics.
        write: RunWrite,
    },
    /// §5: 旧执行者所有写入返回 `RUN_FENCED`.
    Refuse {
        /// The frozen catalogue code.
        error_code: RunWriteRefusal,
    },
}

/// Decide whether a write from an executor may be accepted.
///
/// `current_token` is the Task's **current** generation, which the caller must
/// read from the database on every call together with `run_status`, inside the
/// transaction applying the effect. A spawn snapshot is insufficient. Ordinary
/// callbacks cannot extend terminal history; UNKNOWN is handled only by
/// reconcile (§3). The application must additionally validate each notification
/// kind/transition: STARTING can legitimately receive a failure, for example.
/// This pure rule does not load authority, check permissions, or commit a write.
///
/// This is only the generation/state layer. `DOMAIN_MODEL.md` §2 defines a
/// lease as time-limited execution authority: a current token alone does not
/// establish a valid lease. The application must check the matching Run/Task
/// lease window before applying any effect. Heartbeat renewal composes this
/// layer with the lease check through [`decide_heartbeat`].
pub fn decide_write(
    current_token: u64,
    presented_token: u64,
    write: RunWrite,
    run_status: RunStatus,
) -> WriteDecision {
    // The persisted counter starts at one (0004). Equal unminted tokens do not
    // establish authority. Generation refusal takes priority over Run state.
    if current_token == 0
        || presented_token == 0
        || !Generation::classify(presented_token, current_token).is_current()
    {
        return WriteDecision::Refuse {
            error_code: RunWriteRefusal::Fenced,
        };
    }
    match run_status {
        RunStatus::Queued | RunStatus::Starting | RunStatus::Running => {
            WriteDecision::Accept { write }
        }
        RunStatus::Unknown => WriteDecision::Refuse {
            error_code: RunWriteRefusal::ReconciliationRequired,
        },
        _ => WriteDecision::Refuse {
            error_code: RunWriteRefusal::Terminal,
        },
    }
}

/// The verdict on a heartbeat that tries to extend a lease.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RenewalDecision {
    /// §5: 心跳只能延长匹配当前 token 的租约.
    Renewed {
        /// The `expires_at` to write.
        expires_at_ms: i64,
    },
    /// The lease was not extended. See [`RenewalDecision::refusal`].
    Refused {
        /// Why the lease was not extended.
        error_code: LeaseRefusal,
    },
}

impl RenewalDecision {
    /// The refusal code, if this is a refusal.
    pub const fn refusal(self) -> Option<LeaseRefusal> {
        match self {
            Self::Renewed { .. } => None,
            Self::Refused { error_code } => Some(error_code),
        }
    }

    /// `true` when renewal was admitted; a short TTL may preserve the expiry.
    pub const fn renewed(self) -> bool {
        matches!(self, Self::Renewed { .. })
    }
}

/// Decide whether a heartbeat may extend a lease, and to when.
///
/// `ttl_ms` is the caller's lease duration: `TECH_STACK_AND_STANDARDS.md` §5
/// requires the lease to be created at claim time but **freezes no number**, and
/// neither does `0004` — `expires_at >= acquired_at` is a shape constraint, not a
/// value. So the duration is an input rather than a constant here.
///
/// # Why an expired lease is refused rather than extended
///
/// If a heartbeat could revive an expired lease, leases would never expire and
/// §3's `RUNNING → TIMED_OUT` would become unreachable for a live process. The
/// refusal is [`LeaseRefusal::Expired`], not [`LeaseRefusal::Fenced`]: the
/// executor still holds the current token, so calling it fenced would be the
/// false diagnosis the two codes were split to avoid.
pub fn decide_renewal(
    lease: LeaseSnapshot,
    current_token: u64,
    presented_token: u64,
    now_ms: i64,
    ttl_ms: i64,
) -> RenewalDecision {
    // §5's 「匹配当前 token 的」 — identity before time.
    if current_token == 0 || presented_token == 0 || lease.fencing_token != current_token {
        return RenewalDecision::Refused {
            error_code: LeaseRefusal::Fenced,
        };
    }
    match Generation::classify(presented_token, current_token) {
        Generation::Current => {}
        Generation::Stale | Generation::Ahead => {
            return RenewalDecision::Refused {
                error_code: LeaseRefusal::Fenced,
            };
        }
    }

    if lease.is_expired_at(now_ms) {
        return RenewalDecision::Refused {
            error_code: LeaseRefusal::Expired,
        };
    }

    // Renewal cannot shorten an existing window, even with short, zero or
    // negative TTL. Expired leases were rejected above; this is not revival.
    let ttl_ms = ttl_ms.max(0);
    RenewalDecision::Renewed {
        expires_at_ms: lease.expires_at_ms.max(now_ms.saturating_add(ttl_ms)),
    }
}

/// The verdict on a heartbeat, which is the one write that also renews.
///
/// §5's sentence has two halves and a heartbeat is subject to both, so the two
/// checks are composed here rather than left as glue for every caller to
/// reassemble — and reassembling them in the wrong order is easy, because the
/// answer differs: a stale executor with an unexpired lease is fenced, while the
/// current executor with an expired lease is merely not renewed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HeartbeatDecision {
    /// Admitted; the lease window was preserved or extended.
    Renewed {
        /// The `expires_at` to write.
        expires_at_ms: i64,
    },
    /// Admitted as the current generation, but the lease window had closed.
    ///
    /// The executor is **not** fenced — it still holds the current token — so
    /// the caller should treat this as §3's deadline, not as a takeover.
    LeaseExpired,
    /// The heartbeat itself is refused; nothing was written.
    Refused {
        /// The frozen catalogue code.
        error_code: RunWriteRefusal,
    },
}

impl HeartbeatDecision {
    /// The refusal code, if the heartbeat was refused outright.
    pub const fn refusal(self) -> Option<RunWriteRefusal> {
        match self {
            Self::Refused { error_code } => Some(error_code),
            _ => None,
        }
    }
}

/// Apply both of §5's halves to a heartbeat.
///
/// Both Run status and generation must be read in the same transaction that
/// applies the heartbeat. This composes callback admission and lease renewal;
/// reconciliation uses a different path and cannot use an ordinary heartbeat.
pub fn decide_heartbeat(
    lease: LeaseSnapshot,
    current_token: u64,
    presented_token: u64,
    now_ms: i64,
    ttl_ms: i64,
    run_status: RunStatus,
) -> HeartbeatDecision {
    if let WriteDecision::Refuse { error_code } = decide_write(
        current_token,
        presented_token,
        RunWrite::Heartbeat,
        run_status,
    ) {
        return HeartbeatDecision::Refused { error_code };
    }
    match decide_renewal(lease, current_token, presented_token, now_ms, ttl_ms) {
        RenewalDecision::Renewed { expires_at_ms } => HeartbeatDecision::Renewed { expires_at_ms },
        RenewalDecision::Refused {
            error_code: LeaseRefusal::Expired,
        } => HeartbeatDecision::LeaseExpired,
        RenewalDecision::Refused {
            error_code: LeaseRefusal::Fenced,
        } => HeartbeatDecision::Refused {
            error_code: RunWriteRefusal::Fenced,
        },
    }
}

impl fmt::Display for HeartbeatDecision {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Renewed { expires_at_ms } => {
                write!(
                    formatter,
                    "heartbeat renewed the lease to {expires_at_ms} ms"
                )
            }
            Self::LeaseExpired => formatter.write_str(
                "heartbeat from the current generation, but the lease window has closed",
            ),
            Self::Refused { error_code } => {
                write!(formatter, "heartbeat refused: {error_code}")
            }
        }
    }
}

impl fmt::Display for WriteDecision {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Accept { write } => {
                write!(formatter, "{write} accepted for the current generation")
            }
            Self::Refuse { error_code } => write!(formatter, "write refused: {error_code}"),
        }
    }
}

impl fmt::Display for RenewalDecision {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Renewed { expires_at_ms } => {
                write!(formatter, "lease renewed to {expires_at_ms} ms")
            }
            Self::Refused { error_code } => write!(formatter, "lease not renewed: {error_code}"),
        }
    }
}

impl fmt::Display for RunWrite {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Heartbeat => "heartbeat",
            Self::Output => "output",
            Self::Completion => "completion",
        })
    }
}

impl fmt::Display for Generation {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Current => "current generation",
            Self::Stale => "previous generation",
            Self::Ahead => "generation ahead of the Task's current one",
        })
    }
}

/// The statuses from which a Run may still hold a lease.
///
/// `0012`'s `leases_consistency_insert` and `leases_consistency_update`
/// triggers both require a lease's Run to be in exactly this set, so this list
/// is the same one the database enforces — restated here so a caller does not
/// have to grep the migration to know when renewal is even worth attempting.
/// (`0004` creates the `leases` table but has no trigger at all; naming it here
/// pointed at nothing.)
///
/// The agreement between the two is not asserted by this file, and it is not
/// asserted by reading the migration: the domain suite proves only that these
/// four are every non-terminal status, and
/// `persistence-sqlite/tests/run_lease_consistency_forward_migration.rs`
/// proves against a live database that the guards admit the same four. A
/// domain test that parsed the SQL looked like it closed the gap and left it
/// open instead — review walked six one-line bypasses through it.
pub const LEASE_BEARING_STATUSES: [RunStatus; 4] = [
    RunStatus::Queued,
    RunStatus::Starting,
    RunStatus::Running,
    RunStatus::Unknown,
];
