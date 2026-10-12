//! Cancellation and timeout **decisions** for the §3 Run lifecycle.
//!
//! Frozen source: `STATE_MACHINES.md` §3, with `TECH_STACK_AND_STANDARDS.md` §5.
//!
//! # This module decides; it does not act
//!
//! `adapter-supervisor` already owns every process-level action — `CancellationToken`,
//! `Termination`, wait deadlines and shutdown grace periods. What it does not own
//! is the question of **whether** a Run may be declared cancelled or timed out.
//! That is a domain judgement over observed facts, and `STATE_MACHINES` §3 puts
//! the conditions in the transition table, so it belongs here.
//!
//! Nothing in this file reads a clock, touches a process, or mutates a Run. The
//! caller supplies the observations and acts on the verdict.
//!
//! # A cancel request is not a cancellation
//!
//! §3's two rows differ from the third in wording, and the difference is the
//! whole point:
//!
//! ```text
//! QUEUED   → CANCELLED │ 尚未开始或已安全停止
//! STARTING → CANCELLED │ cancel 确认或达到强制 fence 条件
//! RUNNING  → CANCELLED │ cancel 确认；或 Human takeover 先 fence
//! ```
//!
//! `STARTING` and `RUNNING` both put **确认** (confirmation) in the precondition,
//! and `RUN_CANCEL_NOT_CONFIRMED` is the code that says the distinction was
//! ignored. A request that has been sent but not acknowledged is not a
//! cancellation; declaring one would drop a Run that is still writing.
//!
//! # `UNKNOWN` is not a cancel or a timeout — it is a reconcile
//!
//! §3 **does** have an `UNKNOWN → FAILED/TIMED_OUT/CANCELLED` row, so it would be
//! easy to read this module as covering it. It does not, and the reason is the
//! 触发者 column: that row belongs to `Reconcile service/Human`, under
//! `TerminalProvenOrDecided` — 「Adapter 证明终态，或无法恢复并按策略/Human 决定」.
//! There is no cancel-command row and no timeout row out of `UNKNOWN`, which is
//! why [`RunCommand::Reconcile`](crate::RunCommand::Reconcile) owns those three
//! targets and [`RunCommand::Cancel`] / [`RunCommand::Timeout`] do not.
//!
//! Treating "§3's table has no `→ CANCELLED` row for `UNKNOWN`" as the reason to
//! refuse would be a false statement about the specification. The accurate reason
//! is that the row that exists is not a cancel row, so both verdicts answer
//! [`CancelDecision::ReconcileOnly`] / [`TimeoutDecision::ReconcileOnly`] and hand
//! the decision to DEV-03-08.
//!
//! # A timeout is not a deadline either, once the Run is RUNNING
//!
//! ```text
//! STARTING → TIMED_OUT │ 超过 start timeout
//! RUNNING  → TIMED_OUT │ 心跳/总时长超限**且对账仍无法证明运行**
//! ```
//!
//! The second row carries an extra condition the first does not. It exists because
//! the platform may have lost contact with a process that is still running:
//! `UNKNOWN` is the state for "cannot confirm", and reconcile is what resolves
//! it (DEV-03-08). So a `RUNNING` Run that has passed its deadline may **not**
//! be declared timed out while reconciliation could still prove it is running.
//! [`ReconcileEvidence`] carries that fact, and only
//! [`ReconcileEvidence::CouldNotProve`] lets the transition through.
//!
//! Modelling that gate here — rather than leaving it to the caller — is what
//! stops "the heartbeat stopped, therefore it timed out" from becoming the
//! default. §3's `RUNNING → UNKNOWN` row exists precisely for the case where
//! contact was lost without proof of death.

use std::fmt;

use crate::run_state_machine::{RunErrorCode, RunStatus};

/// What a reconcile attempt established about a `RUNNING` Run's liveness.
///
/// §3 asks two different questions and an earlier version of this module
/// collapsed them into one boolean, which is the kind of compression that
/// hides a decision:
///
/// * `RUNNING → TIMED_OUT` requires 心跳/总时长超限**且对账仍无法证明运行**
///   — a reconcile has run and failed to prove the Run alive.
/// * `RUNNING → UNKNOWN` requires 平台重启/连接丢失**且事实不明**
///   — the fact is unknown, which is the state *before* any reconcile result.
///
/// Those are complementary, and only the first one is satisfied by a negative
/// result. A single flag cannot carry both, and a flag named for one of them
/// reads as the other: `reconciliation_proves_running` sounds like "reconcile
/// has proved it is running", which is the exact opposite of the fact §3's
/// timeout gate needs. So the flag became a three-state value.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ReconcileEvidence {
    /// No reconcile has produced a result, so 事实不明 still holds.
    ///
    /// §3's timeout gate is not satisfied — nothing has *yet* failed to prove
    /// the Run alive — and `RUNNING → UNKNOWN` is available.
    Indeterminate,
    /// A reconcile proved the Run is still running under the same session/token.
    ///
    /// §3's timeout gate is not satisfied, and neither is `RUNNING → UNKNOWN`:
    /// the fact is now known, so 事实不明 is false. No transition is available.
    ProvedRunning,
    /// A reconcile ran and could not prove the Run is running.
    ///
    /// The only state in which §3's timeout gate holds.
    CouldNotProve,
}

impl ReconcileEvidence {
    /// Every evidence state, in a stable order.
    ///
    /// The duplicate check is folded into this initializer rather than living in
    /// a separate constant — see [`CancelObservation::ALL`] for why.
    ///
    /// # This one fires earlier than the other two
    ///
    /// [`CancelObservation::ALL`] and [`crate::RunErrorCode::ALL`] are read only
    /// by tests and the example, so a duplicate entry there surfaces at *use*
    /// time and a bare `cargo build` still succeeds. **This constant is the
    /// exception**: [`ExecutionObservation::ALL`] reads it while the library
    /// itself is being compiled, so a duplicate entry here fails `cargo build`.
    ///
    /// The three timings differ, which is why this doc does not defer to
    /// another's for "the exact timing" — doing so would inherit a statement
    /// that is false here, in the same way the earlier `"fails the build"`
    /// wording inherited a claim the code did not deliver.
    pub const ALL: [ReconcileEvidence; 3] = {
        let all = [
            ReconcileEvidence::Indeterminate,
            ReconcileEvidence::ProvedRunning,
            ReconcileEvidence::CouldNotProve,
        ];
        let mut seen = [false; 3];
        let mut entry = 0;
        while entry < all.len() {
            let index = all[entry].variant_index();
            assert!(!seen[index], "ReconcileEvidence::ALL lists a variant twice");
            seen[index] = true;
            entry += 1;
        }
        all
    };

    /// This variant's position in [`ReconcileEvidence::ALL`].
    ///
    /// Exhaustive with no wildcard arm, so a new variant breaks the build here
    /// before it can be forgotten in `ALL`.
    const fn variant_index(self) -> usize {
        match self {
            Self::Indeterminate => 0,
            Self::ProvedRunning => 1,
            Self::CouldNotProve => 2,
        }
    }

    /// `true` only in the state §3's `RUNNING → TIMED_OUT` precondition requires.
    pub const fn blocks_timeout(self) -> bool {
        !matches!(self, Self::CouldNotProve)
    }
}

/// What the caller has observed about a Run's execution.
///
/// Filled in by the caller from whatever source it trusts — supervisor events,
/// a clock, a reconcile result — so this module never has to reach for one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExecutionObservation {
    /// The Run never left `STARTING`.
    Starting {
        /// Whether the start deadline has passed.
        start_deadline_passed: bool,
    },
    /// The Run is executing and heartbeating.
    Running {
        /// Whether the heartbeat deadline has passed.
        heartbeat_deadline_passed: bool,
        /// Whether the total-duration deadline has passed.
        total_deadline_passed: bool,
        /// What a reconcile attempt established about this Run's liveness.
        ///
        /// §3: 心跳/总时长超限**且对账仍无法证明运行** — the timeout needs
        /// [`ReconcileEvidence::CouldNotProve`] and nothing else. See that type
        /// for why this is three states and not the boolean it used to be.
        evidence: ReconcileEvidence,
    },
}

impl ExecutionObservation {
    /// Every observation, in a stable order: `2` for `Starting`, `12` for
    /// `Running` — two deadline flags times the three reconcile-evidence states.
    ///
    /// # The length is a third manual sync point
    ///
    /// The loop below is driven by `ReconcileEvidence::ALL.len()`, so a fourth
    /// evidence state is caught there; the array's own length is not derived
    /// from it. Rust array lengths in a `const` initializer cannot call a
    /// `const fn` on another type's constant here without a generic const
    /// expression, so the two have to agree by hand. The failure is fail-closed
    /// — an out-of-bounds const-evaluation error at the assignment — not a
    /// silently shorter sweep, but the diagnostic points at the array
    /// declaration rather than at the evidence enum.
    pub const ALL: [ExecutionObservation; 14] = {
        // Seeded with a repeated value because `ExecutionObservation` is `Copy`;
        // every slot is overwritten below before the array is returned.
        let mut all = [ExecutionObservation::Starting {
            start_deadline_passed: false,
        }; 14];
        all[0] = ExecutionObservation::Starting {
            start_deadline_passed: false,
        };
        all[1] = ExecutionObservation::Starting {
            start_deadline_passed: true,
        };
        let mut index = 0;
        while index < ReconcileEvidence::ALL.len() {
            let evidence = ReconcileEvidence::ALL[index];
            all[2 + index * 4] = ExecutionObservation::Running {
                heartbeat_deadline_passed: false,
                total_deadline_passed: false,
                evidence,
            };
            all[3 + index * 4] = ExecutionObservation::Running {
                heartbeat_deadline_passed: false,
                total_deadline_passed: true,
                evidence,
            };
            all[4 + index * 4] = ExecutionObservation::Running {
                heartbeat_deadline_passed: true,
                total_deadline_passed: false,
                evidence,
            };
            all[5 + index * 4] = ExecutionObservation::Running {
                heartbeat_deadline_passed: true,
                total_deadline_passed: true,
                evidence,
            };
            index += 1;
        }
        all
    };

    /// `true` when at least one applicable deadline has passed.
    pub const fn deadline_passed(self) -> bool {
        match self {
            Self::Starting {
                start_deadline_passed,
            } => start_deadline_passed,
            Self::Running {
                heartbeat_deadline_passed,
                total_deadline_passed,
                ..
            } => heartbeat_deadline_passed || total_deadline_passed,
        }
    }
}

/// What the caller has observed about a cancellation request.
///
/// §3 words its three cancel rows differently, and each wording is a different
/// precondition — so each gets its own variant rather than one
/// "cancellation was forced" flag:
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CancelObservation {
    /// The request was sent; the Adapter has not acknowledged it.
    Requested,
    /// The Adapter acknowledged, or the process is known to have stopped.
    ///
    /// Covers 「已安全停止」 in §3's `QUEUED` row and 「cancel 确认」 in the other
    /// two, so it is the only observation all three rows accept.
    Confirmed,
    /// A forced fence condition was reached on its own.
    ///
    /// §3's `STARTING` row: 「cancel 确认**或达到强制 fence 条件**」. No takeover
    /// is named there, so this variant is `STARTING`-only — a fence alone is not
    /// a `RUNNING` precondition, and §3's `QUEUED` row never mentions fencing.
    ForceFenceReached,
    /// A Human takeover fenced this Run first.
    ///
    /// §3's `RUNNING` row: 「cancel 确认；**或 Human takeover 先 fence**」. The
    /// 「先」 is load-bearing — an ordinary force-fence is not this row's second
    /// alternative, so it is a separate variant from [`Self::ForceFenceReached`]
    /// and `RUNNING`-only.
    TakeoverFenced,
    /// The Run never started, so §3's `QUEUED` row applies: 「尚未开始或已安全停止」.
    NeverStarted,
}

impl CancelObservation {
    /// Every observation, in a stable order.
    ///
    /// Duplicate entries are rejected as this constant is evaluated; see below
    /// for why an `ALL.len()` assertion in a test cannot stand in for it.
    ///
    /// The duplicate check is folded into this initializer rather than living in
    /// a separate constant, because a plain associated constant is evaluated
    /// lazily and nothing referenced the one it replaced — so its docstring
    /// claimed "fails the build" for a check that no build ever ran. See
    /// [`crate::RunErrorCode::ALL`] for the exact timing: the check fires where
    /// `ALL` is used (tests and examples), not in a bare `cargo build`.
    pub const ALL: [CancelObservation; 5] = {
        let all = [
            CancelObservation::Requested,
            CancelObservation::Confirmed,
            CancelObservation::ForceFenceReached,
            CancelObservation::TakeoverFenced,
            CancelObservation::NeverStarted,
        ];
        let mut seen = [false; 5];
        let mut entry = 0;
        while entry < all.len() {
            let index = all[entry].variant_index();
            assert!(!seen[index], "CancelObservation::ALL lists a variant twice");
            seen[index] = true;
            entry += 1;
        }
        all
    };

    /// This variant's position in [`CancelObservation::ALL`].
    ///
    /// Exhaustive with no wildcard arm, so a new observation breaks the build
    /// here before it can be forgotten in `ALL`.
    const fn variant_index(self) -> usize {
        match self {
            Self::Requested => 0,
            Self::Confirmed => 1,
            Self::ForceFenceReached => 2,
            Self::TakeoverFenced => 3,
            Self::NeverStarted => 4,
        }
    }

    /// `true` when this observation satisfies §3's precondition for the given
    /// status.
    ///
    /// Every cell of this table is a word in the row it claims, and nothing
    /// else:
    ///
    /// | §3 row | 尚未开始 | 已安全停止（QUEUED）／cancel 确认（其余两行） | 强制 fence | takeover 先 fence |
    /// |---|---|:---:|:---:|:---:|
    /// | `QUEUED → CANCELLED` ｜尚未开始或已安全停止 | ✅ | ✅ | — | — |
    /// | `STARTING → CANCELLED` ｜cancel 确认或达到强制 fence 条件 | — | ✅ | ✅ | — |
    /// | `RUNNING → CANCELLED` ｜cancel 确认；或 Human takeover 先 fence | — | ✅ | — | ✅ |
    ///
    /// An earlier version of this table had `QUEUED × 强制 fence` and
    /// `RUNNING × 强制 fence` ticked as well. Neither is supported: 「强制 fence」
    /// appears only in §3's `STARTING` row, and the `RUNNING` row's alternative
    /// is qualified by a Human takeover that 「先」 must precede it. Those two
    /// cells would have let a bare forced fence cancel a `QUEUED` Run and cancel
    /// a `RUNNING` Run without any takeover evidence.
    pub const fn satisfies(self, status: RunStatus) -> bool {
        match self {
            // Sent but unacknowledged. §3's `STARTING` and `RUNNING` rows both
            // require 确认, and nothing here satisfies `QUEUED`'s row either.
            Self::Requested => false,
            // The only precondition word shared by all three rows.
            Self::Confirmed => matches!(
                status,
                RunStatus::Queued | RunStatus::Starting | RunStatus::Running
            ),
            Self::ForceFenceReached => matches!(status, RunStatus::Starting),
            Self::TakeoverFenced => matches!(status, RunStatus::Running),
            // 尚未开始 — a `RUNNING` Run is by definition not one.
            Self::NeverStarted => matches!(status, RunStatus::Queued),
        }
    }
}

/// The verdict for a cancellation request.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CancelDecision {
    /// §3's transition applies. The caller must fence the token and release the
    /// lease before writing `RUN_CANCELLED` — §3 lists both as the row's effects.
    Confirm {
        /// The status to write.
        target: RunStatus,
    },
    /// The request has not been confirmed and no force-fence condition holds.
    ///
    /// §3 puts 确认 in the precondition, so the Run stays where it is. The
    /// refusal names the code the specification froze for exactly this mistake.
    Refuse {
        /// The frozen catalogue code.
        error_code: RunErrorCode,
    },
    /// §3's only route to `CANCELLED` from this status is a **reconcile** row,
    /// not a cancel command.
    ///
    /// Distinct from [`Self::NotCancellable`]: the specification does have a
    /// `→ CANCELLED` row here, it is just not a cancel-command row. Refusing it
    /// as a permission or confirmation failure would be a false code, and
    /// answering `NotCancellable` would be a false claim about the table.
    ReconcileOnly {
        /// The status observed.
        status: RunStatus,
    },
    /// This status has no `→ CANCELLED` row in §3 at all.
    ///
    /// Only the four terminal statuses reach this: §3 closes them with
    /// 「`SUCCEEDED/FAILED/TIMED_OUT/CANCELLED` 为单个 Run 终态」.
    NotCancellable {
        /// The status observed.
        status: RunStatus,
    },
}

/// Decide whether a Run may be declared cancelled right now.
///
/// Pure: the verdict is a function of `(status, observation)`, so every
/// combination is table-testable without a process.
///
/// # Consistency with `RUN_EDGES`
///
/// Every [`CancelDecision::Confirm`] here must be a transition
/// `run_transition(status, RunCommand::Cancel)` also accepts. This module adds
/// §3's precondition text; it never adds an edge.
pub fn decide_cancel(status: RunStatus, observation: CancelObservation) -> CancelDecision {
    match status {
        // §3's `UNKNOWN → FAILED/TIMED_OUT/CANCELLED` row is triggered by
        // `Reconcile service/Human`, not by a cancel command. DEV-03-08 owns it.
        RunStatus::Unknown => return CancelDecision::ReconcileOnly { status },
        // §3 closes the four terminal states: `SUCCEEDED/FAILED/TIMED_OUT/CANCELLED`
        // 为单个 Run 终态. No `→ CANCELLED` row exists from any of them.
        RunStatus::Succeeded | RunStatus::Failed | RunStatus::TimedOut | RunStatus::Cancelled => {
            return CancelDecision::NotCancellable { status };
        }
        RunStatus::Queued | RunStatus::Starting | RunStatus::Running => {}
    }

    if observation.satisfies(status) {
        return CancelDecision::Confirm {
            target: RunStatus::Cancelled,
        };
    }

    // §3's `STARTING → CANCELLED` row is the one whose 主要拒绝码 is
    // `RUN_CANCEL_NOT_CONFIRMED`; `QUEUED`'s and `RUNNING`'s are both
    // `AUTH_PERMISSION_DENIED`. Only `STARTING` names the confirmation code, so
    // only that status reports it.
    let error_code = match status {
        RunStatus::Starting => RunErrorCode::RunCancelNotConfirmed,
        _ => RunErrorCode::PermissionDenied,
    };
    CancelDecision::Refuse { error_code }
}

/// The verdict for a deadline that has passed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TimeoutDecision {
    /// §3's transition applies: terminate or quarantine the process and fence
    /// the token.
    TimeOut {
        /// The status to write.
        target: RunStatus,
    },
    /// Nothing to do: the applicable deadline has not passed.
    ///
    /// Also covers a status with no `→ TIMED_OUT` row that is not `UNKNOWN`
    /// (`QUEUED` and the four terminal states), and an observation that does not
    /// match the status. Both mean §3's precondition cannot even be evaluated,
    /// which is why they share this verdict with a deadline that has not
    /// arrived. An earlier docstring listed only the deadline case, which was
    /// true of the code and not of this doc.
    NotYet,
    /// The deadline has passed and no reconcile has produced a result, so
    /// §3's 事实不明 still holds and `RUNNING → UNKNOWN` is the row.
    ///
    /// Freezing the lease and not dispatching a new Run is DEV-03-08's action;
    /// this module only names the transition.
    NeedsReconcile {
        /// The status to write: §3's `RUNNING → UNKNOWN`.
        target: RunStatus,
    },
    /// The deadline has passed and a reconcile **proved the Run is still
    /// running**, so no §3 transition is available.
    ///
    /// # A derived reading, not a spec sentence
    ///
    /// Neither row of §3 covers this, and the exclusion is the point:
    /// `RUNNING → TIMED_OUT` needs 对账仍无法证明运行, which a proof of liveness
    /// contradicts; `RUNNING → UNKNOWN` needs 事实不明, which a proof of liveness
    /// also contradicts. §3 does not say what happens next, so this variant
    /// asserts the *absence* of a transition and nothing more. The caller keeps
    /// the Run in `RUNNING`; refreshing the heartbeat deadline is
    /// `UNKNOWN → RUNNING`'s side effect in §3, applied here to a Run that never
    /// left `RUNNING`.
    KeepRunning,
    /// The deadline has passed, a reconcile could not prove the Run is alive,
    /// and §3's timeout precondition holds.
    TimeOutFromReconcile {
        /// The status to write: `TIMED_OUT`.
        target: RunStatus,
    },
    /// The Run is already `UNKNOWN`, and §3's only route to `TIMED_OUT` from
    /// there is the `Reconcile service/Human` row under `TerminalProvenOrDecided`.
    ///
    /// Separate from [`Self::NotYet`]: no deadline is involved, the Run is
    /// waiting on DEV-03-08's reconcile rather than on a clock. Answering
    /// `NotYet` would tell the caller to keep waiting for a deadline that
    /// nothing will ever satisfy.
    ReconcileOnly {
        /// The status observed.
        status: RunStatus,
    },
}

/// Decide whether a Run that has passed a deadline may be declared timed out.
///
/// Every [`TimeoutDecision::TimeOut`] here must be a transition
/// `run_transition(status, RunCommand::Timeout)` also accepts. This module adds
/// §3's precondition text; it never adds an edge.
pub fn decide_timeout(status: RunStatus, observation: ExecutionObservation) -> TimeoutDecision {
    match (status, observation) {
        // §3: STARTING → TIMED_OUT │ 超过 start timeout. No reconcile clause:
        // a Run that never started has no external process whose liveness is in
        // question.
        (RunStatus::Starting, ExecutionObservation::Starting { .. }) => {
            if observation.deadline_passed() {
                TimeoutDecision::TimeOut {
                    target: RunStatus::TimedOut,
                }
            } else {
                TimeoutDecision::NotYet
            }
        }

        // §3: RUNNING → TIMED_OUT │ 心跳/总时长超限且对账仍无法证明运行.
        (RunStatus::Running, ExecutionObservation::Running { evidence, .. }) => {
            if !observation.deadline_passed() {
                TimeoutDecision::NotYet
            } else {
                match evidence {
                    // 对账仍无法证明运行 — the only state that satisfies §3.
                    ReconcileEvidence::CouldNotProve => TimeoutDecision::TimeOutFromReconcile {
                        target: RunStatus::TimedOut,
                    },
                    // 事实不明 — §3's `RUNNING → UNKNOWN` precondition.
                    ReconcileEvidence::Indeterminate => TimeoutDecision::NeedsReconcile {
                        target: RunStatus::Unknown,
                    },
                    // Liveness proved, so neither row applies.
                    ReconcileEvidence::ProvedRunning => TimeoutDecision::KeepRunning,
                }
            }
        }

        // §3's `UNKNOWN → TIMED_OUT` is the `Reconcile service/Human` row, not a
        // deadline. The status is already frozen, so reconcile is the only
        // thing left to do — and DEV-03-08 owns it.
        (RunStatus::Unknown, _) => TimeoutDecision::ReconcileOnly { status },

        // §3 has three rows reaching `TIMED_OUT`: `STARTING → TIMED_OUT` and
        // `RUNNING → TIMED_OUT` above, plus `UNKNOWN → FAILED/TIMED_OUT/CANCELLED`
        // handled by the arm above. What is left is `QUEUED`, the four terminal
        // states (closed by 「为单个 Run 终态」), and an observation that does not
        // match the status — e.g. a `RUNNING` Run reported as `Starting`.
        _ => TimeoutDecision::NotYet,
    }
}

impl fmt::Display for CancelDecision {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Confirm { target } => write!(formatter, "cancel confirmed → {target}"),
            Self::Refuse { error_code } => write!(formatter, "cancel refused: {error_code}"),
            Self::ReconcileOnly { status } => {
                write!(formatter, "{status} reaches CANCELLED only via reconcile")
            }
            Self::NotCancellable { status } => {
                write!(formatter, "{status} has no §3 → CANCELLED row")
            }
        }
    }
}

impl fmt::Display for TimeoutDecision {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::TimeOut { target } => write!(formatter, "deadline passed → {target}"),
            Self::NotYet => formatter.write_str("no timeout"),
            Self::NeedsReconcile { target } => {
                write!(
                    formatter,
                    "no reconcile result yet, fact 事实不明 → {target}"
                )
            }
            Self::KeepRunning => {
                formatter.write_str("reconcile proved it still running; no §3 row applies")
            }
            Self::TimeOutFromReconcile { target } => {
                write!(formatter, "reconcile could not prove it running → {target}")
            }
            Self::ReconcileOnly { status } => {
                write!(formatter, "{status} reaches TIMED_OUT only via reconcile")
            }
        }
    }
}
