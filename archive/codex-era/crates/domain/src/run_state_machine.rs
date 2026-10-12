//! Run lifecycle state machine.
//!
//! Frozen source: `docs/specs/STATE_MACHINES.md` §3 and its transition table;
//! `docs/development/TECH_STACK_AND_STANDARDS.md` §5.
//!
//! Same layering as [`crate::task_state_machine`]: this module decides whether an
//! edge exists in the §3 graph and which frozen rejection code it carries. Lease,
//! fencing-token, sequence and Adapter-state preconditions need to read the
//! database, so they are described here and evaluated by the application layer in
//! the same transaction.
//!
//! # `UNKNOWN` is not a terminal state
//!
//! §3 says so explicitly: "`UNKNOWN` 是平台重启或 Adapter 失联后的恢复态，
//! 不是正常执行路径". It therefore has five out-edges, all driven by the reconcile
//! service. Only `SUCCEEDED` / `FAILED` / `TIMED_OUT` / `CANCELLED` absorb.
//!
//! # No edge returns to `QUEUED`
//!
//! §3 ends with "禁止把旧 Run 改回 QUEUED", and §3's table has no such row.
//! Retries read a terminal Run and create a *new* one. The absence is structural:
//! `QUEUED` has exactly one in-edge, from creation.
//!
//! # `(from, command)` is unique across the table
//!
//! Unlike §2, §3 has a `(from, command)` pair that fans out: the reconcile
//! service drives `UNKNOWN` to five destinations. Because the five rows differ
//! only in the evidence proven, that evidence is carried *in* the command — see
//! [`ReconcileOutcome`]. With it, every `(from, command)` pair in [`RUN_EDGES`]
//! resolves to exactly one row, so [`crate::run_state_machine::transition`] never has to break a tie by
//! table order. `each_pair_resolves_to_exactly_one_row` in the test suite pins
//! that invariant; reintroducing an unqualified multi-destination command would
//! make `transition` silently return the first row.

use std::fmt;

use serde::{Deserialize, Serialize};

/// A Run's position in the §3 lifecycle.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum RunStatus {
    /// Created with a fresh attempt number and fencing token.
    Queued,
    /// The Supervisor is preparing the process, credentials and workspace.
    Starting,
    /// The Adapter confirmed start and is heartbeating.
    Running,
    /// Platform cannot confirm the external process; frozen pending reconcile.
    Unknown,
    /// Completed successfully. Terminal.
    Succeeded,
    /// Failed with a stable error class. Terminal.
    Failed,
    /// Exceeded a start, heartbeat or total deadline. Terminal.
    TimedOut,
    /// Cancelled. Terminal.
    Cancelled,
}

impl RunStatus {
    /// Every state, in a stable order.
    pub const ALL: [RunStatus; 8] = [
        RunStatus::Queued,
        RunStatus::Starting,
        RunStatus::Running,
        RunStatus::Unknown,
        RunStatus::Succeeded,
        RunStatus::Failed,
        RunStatus::TimedOut,
        RunStatus::Cancelled,
    ];

    /// The wire name used in `RUN_*` events and the UI.
    pub const fn as_str(self) -> &'static str {
        match self {
            RunStatus::Queued => "QUEUED",
            RunStatus::Starting => "STARTING",
            RunStatus::Running => "RUNNING",
            RunStatus::Unknown => "UNKNOWN",
            RunStatus::Succeeded => "SUCCEEDED",
            RunStatus::Failed => "FAILED",
            RunStatus::TimedOut => "TIMED_OUT",
            RunStatus::Cancelled => "CANCELLED",
        }
    }

    /// The four states §3 calls 终态.
    pub const fn is_terminal(self) -> bool {
        matches!(
            self,
            RunStatus::Succeeded | RunStatus::Failed | RunStatus::TimedOut | RunStatus::Cancelled
        )
    }
}

impl fmt::Display for RunStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// The outcome a reconcile pass proved from Adapter evidence.
///
/// §3's 触发者 column sends the "Reconcile service" down five different
/// destinations from `UNKNOWN`, and the rows differ *only* in the evidence that
/// was proven — not in the command. Modelling that as one command with no payload
/// would make `transition(UNKNOWN, Reconcile)` ambiguous, and a linear search
/// returns whichever row happens to come first, which is `UNKNOWN → RUNNING`.
/// That failure is silent and dangerous: a Run the Adapter reported as dead gets
/// recorded as still running. So the evidence is part of the command, and the
/// ambiguity is unrepresentable rather than resolved by table order.
///
/// The variant names track §3's destinations; they are not error codes, and §3
/// freezes no wire spelling for them, so there is deliberately no serde derive and
/// no `as_str()`. Same stance as [`RunCommand`]: an encoding here would be
/// invented protocol.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ReconcileOutcome {
    /// §3: Adapter 证明同 session/token 仍运行；未产生更新世代.
    StillRunning,
    /// §3: 可验证完成结果及 sequence/token.
    Succeeded,
    /// §3: Adapter 证明终态，或无法恢复并按策略/Human 决定.
    Failed,
    /// As `Failed`; §3 folds the three terminal outcomes into one row.
    TimedOut,
    /// As `Failed`; §3 folds the three terminal outcomes into one row.
    Cancelled,
}

impl ReconcileOutcome {
    /// Every outcome, in §3's order.
    pub const ALL: [ReconcileOutcome; 5] = [
        ReconcileOutcome::StillRunning,
        ReconcileOutcome::Succeeded,
        ReconcileOutcome::Failed,
        ReconcileOutcome::TimedOut,
        ReconcileOutcome::Cancelled,
    ];
}

impl fmt::Display for ReconcileOutcome {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}

/// A command that may drive a Run.
///
/// `Reconcile` carries its outcome rather than standing alone; see
/// [`ReconcileOutcome`] for why.
///
/// No `Serialize`/`Deserialize`: unlike `RunStatus` and `RunErrorCode`, §3 gives
/// Run no frozen command spelling, so any encoding here would be invented
/// protocol. `TaskCommand` keeps its derives only because §2 names its actions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum RunCommand {
    Start,
    ConfirmRunning,
    Succeed,
    Fail,
    Timeout,
    Cancel,
    MarkUnknown,
    /// The §3 "Reconcile service" trigger, qualified by proven evidence.
    Reconcile(ReconcileOutcome),
}

impl RunCommand {
    /// Every command except the five reconcile variants, which need an outcome.
    pub const PLAIN: [RunCommand; 7] = [
        RunCommand::Start,
        RunCommand::ConfirmRunning,
        RunCommand::Succeed,
        RunCommand::Fail,
        RunCommand::Timeout,
        RunCommand::Cancel,
        RunCommand::MarkUnknown,
    ];

    /// Every command, including each reconcile outcome: 7 plain + 5 reconcile.
    pub const fn all() -> [RunCommand; 12] {
        [
            RunCommand::Start,
            RunCommand::ConfirmRunning,
            RunCommand::Succeed,
            RunCommand::Fail,
            RunCommand::Timeout,
            RunCommand::Cancel,
            RunCommand::MarkUnknown,
            RunCommand::Reconcile(ReconcileOutcome::StillRunning),
            RunCommand::Reconcile(ReconcileOutcome::Succeeded),
            RunCommand::Reconcile(ReconcileOutcome::Failed),
            RunCommand::Reconcile(ReconcileOutcome::TimedOut),
            RunCommand::Reconcile(ReconcileOutcome::Cancelled),
        ]
    }
}

// There is deliberately no `RunCommand::as_str()`. `TaskCommand` has one because
// §2 names its actions and they double as frozen permission-action strings
// (`task.claim`, `review.approve`, ...). §3 gives Run a 触发者 column
// ("Supervisor", "Reconcile service", ...) and no action vocabulary, so any
// `run.*` string here would be invented protocol. The variant name stays the only
// spelling until a frozen Run action vocabulary exists.
impl fmt::Display for RunCommand {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}

/// Stable rejection codes, frozen in `docs/specs/ERROR_CATALOG.md`.
///
/// # Why `PreconditionFailed` exists
///
/// §3's table has fifteen rows. Ten print a concrete 主要拒绝码. Of the other five
/// the cell is `—`:
///
/// | §3 row | cell |
/// |---|---|
/// | `STARTING → FAILED` | `—` |
/// | `STARTING → TIMED_OUT` | `—` |
/// | `STARTING → UNKNOWN` | `—` |
/// | `RUNNING → TIMED_OUT` | `—` |
/// | `RUNNING → UNKNOWN` | `—` |
///
/// Those edges are reached by *observing* a timeout, a lost connection or an
/// unclassifiable failure, not by a caller failing a check, so §3 has no code to
/// name. They still need one, and `DOMAIN_PRECONDITION_FAILED` is the catalogue's
/// generic precondition failure. Attaching it to those five is a **derived**
/// choice, made here and recorded rather than silently assumed.
///
/// Separately, the last §3 row (`UNKNOWN → FAILED/TIMED_OUT/CANCELLED`) prints
/// `DOMAIN_PRECONDITION_FAILED` **verbatim**, so its use of this variant is
/// spec-mandated and not derived. Both cases land on one variant because §3
/// points them at the same wire string.
///
/// # Wire names
///
/// Every variant carries an explicit `#[serde(rename)]` equal to its `as_str()`.
/// The enum-level `rename_all` is therefore a no-op and is kept only so a
/// forgotten rename is still a compile-clean *detectable* mistake rather than a
/// bare name: the Q-05 device compares the serde expansion against `as_str()` and
/// fails on any divergence. Relying on `rename_all` alone produced
/// `INVALID_TRANSITION` for a catalogue code frozen as `DOMAIN_INVALID_TRANSITION`
/// — a wire name that exists nowhere in `ERROR_CATALOG.md`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum RunErrorCode {
    /// Not a legal edge in the §3 graph at all.
    #[serde(rename = "DOMAIN_INVALID_TRANSITION")]
    InvalidTransition,
    /// The code §3 attaches to a legal edge.
    #[serde(rename = "TASK_ALREADY_CLAIMED")]
    AlreadyClaimed,
    #[serde(rename = "ADAPTER_UNAVAILABLE")]
    AdapterUnavailable,
    #[serde(rename = "AUTH_PERMISSION_DENIED")]
    PermissionDenied,
    #[serde(rename = "BINDING_STALE_VERSION")]
    BindingStaleVersion,
    #[serde(rename = "RUN_CANCEL_NOT_CONFIRMED")]
    RunCancelNotConfirmed,
    #[serde(rename = "RUN_FENCED")]
    RunFenced,
    #[serde(rename = "ARTIFACT_INVALID")]
    ArtifactInvalid,
    /// Spec-frozen on `UNKNOWN → FAILED/TIMED_OUT/CANCELLED`; derived on the five
    /// rows whose §3 cell is `—`. See the type-level note.
    #[serde(rename = "DOMAIN_PRECONDITION_FAILED")]
    PreconditionFailed,
}

impl RunErrorCode {
    /// Every code §3 can raise, in declaration order.
    ///
    /// Added so downstream sweeps do not hand-keep this list. DEV-03-06 wrote
    /// an eight-entry copy in a test and a device, and missed
    /// `DOMAIN_PRECONDITION_FAILED` — the one code that is derived rather than
    /// read off a row, so it was the one a hand-written list drops first. An
    /// exhaustive check over a stale list is worse than no check: it reports
    /// coverage it does not have.
    ///
    /// Putting the list behind a constant did not by itself fix that, and
    /// neither does the duplicate check folded into its initializer. What does
    /// is checking the list against something outside itself — see the test
    /// `every_code_the_run_machine_can_raise_is_enumerated`, which requires
    /// every rejection code `RUN_EDGES` and `CREATE_QUEUED` can produce to
    /// appear here.
    ///
    /// # The duplicate check, and exactly when it fires
    ///
    /// The `assert!` lives inside this initializer rather than in a separate
    /// constant, because a plain associated constant is evaluated lazily and
    /// **nothing referenced the one it replaced** — so its docstring claimed
    /// "fails the build" for a check no build ever ran. Independent review caught
    /// that: a duplicate entry left `cargo build` exiting 0.
    ///
    /// Folding it in fixes the dead-code half, but **not** the timing half, and
    /// the difference is worth being exact about. A `pub const` in a library is
    /// evaluated at each *use*, not at build time, and nothing in the library
    /// itself reads `ALL` — only the tests and the example do. So a duplicate
    /// produces:
    ///
    /// ```text
    /// error[E0080]: evaluation panicked: RunErrorCode::ALL lists a variant twice
    ///   --> crates/domain/tests/run_retry.rs:20:38
    /// ```
    ///
    /// at **test/example compile time**, naming the enum and the fault. A bare
    /// `cargo build` still succeeds. The runtime assertion in
    /// `the_code_enumeration_is_complete_and_distinct` catches the same
    /// duplicate independently, so there are two layers — but only the outer one
    /// is compile-time, and saying otherwise was the defect.
    pub const ALL: [RunErrorCode; 9] = {
        let all = [
            RunErrorCode::InvalidTransition,
            RunErrorCode::AlreadyClaimed,
            RunErrorCode::AdapterUnavailable,
            RunErrorCode::PermissionDenied,
            RunErrorCode::BindingStaleVersion,
            RunErrorCode::RunCancelNotConfirmed,
            RunErrorCode::RunFenced,
            RunErrorCode::ArtifactInvalid,
            RunErrorCode::PreconditionFailed,
        ];
        let mut seen = [false; 9];
        let mut entry = 0;
        while entry < all.len() {
            let index = all[entry].variant_index();
            assert!(!seen[index], "RunErrorCode::ALL lists a variant twice");
            seen[index] = true;
            entry += 1;
        }
        all
    };

    /// This variant's position in [`RunErrorCode::ALL`].
    ///
    /// Exhaustive with no wildcard arm. This is one of the two places a new
    /// variant breaks the build; `as_str` is the other.
    const fn variant_index(self) -> usize {
        match self {
            Self::InvalidTransition => 0,
            Self::AlreadyClaimed => 1,
            Self::AdapterUnavailable => 2,
            Self::PermissionDenied => 3,
            Self::BindingStaleVersion => 4,
            Self::RunCancelNotConfirmed => 5,
            Self::RunFenced => 6,
            Self::ArtifactInvalid => 7,
            Self::PreconditionFailed => 8,
        }
    }

    /// The frozen wire name.
    pub const fn as_str(self) -> &'static str {
        match self {
            RunErrorCode::InvalidTransition => "DOMAIN_INVALID_TRANSITION",
            RunErrorCode::AlreadyClaimed => "TASK_ALREADY_CLAIMED",
            RunErrorCode::AdapterUnavailable => "ADAPTER_UNAVAILABLE",
            RunErrorCode::PermissionDenied => "AUTH_PERMISSION_DENIED",
            RunErrorCode::BindingStaleVersion => "BINDING_STALE_VERSION",
            RunErrorCode::RunCancelNotConfirmed => "RUN_CANCEL_NOT_CONFIRMED",
            RunErrorCode::RunFenced => "RUN_FENCED",
            RunErrorCode::ArtifactInvalid => "ARTIFACT_INVALID",
            RunErrorCode::PreconditionFailed => "DOMAIN_PRECONDITION_FAILED",
        }
    }

    /// `true` for codes `ERROR_CATALOG.md` marks retryable.
    ///
    /// Only the codes this machine can actually raise are considered. Read out of
    /// the catalogue's own tables rather than assumed: `ADAPTER_UNAVAILABLE` is
    /// `是`, which is easy to miss because §3's own `TASK_ALREADY_CLAIMED` and
    /// `RUN_CANCEL_NOT_CONFIRMED` are the pair most often quoted.
    pub const fn is_retryable(self) -> bool {
        matches!(
            self,
            RunErrorCode::AlreadyClaimed
                | RunErrorCode::RunCancelNotConfirmed
                | RunErrorCode::AdapterUnavailable
        )
    }
}

impl fmt::Display for RunErrorCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// A precondition §3 attaches to an edge. Described, never checked here.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum RunPrecondition {
    /// Task is `READY`, Binding valid, lease obtainable.
    LeaseObtainable,
    /// Adapter compatible and healthy; credentials and workspace preparable.
    AdapterPrepared,
    /// The Run has not begun, or has stopped safely.
    NotYetStartedOrStopped,
    /// `initialize`/`start` confirmed; token and Binding version match.
    StartConfirmed,
    /// A stable, non-timeout startup error occurred.
    StableStartupError,
    /// The start timeout elapsed.
    StartTimeoutElapsed,
    /// Cancel confirmed, or the forced-fence condition was reached.
    CancelConfirmed,
    /// The platform cannot determine the external process state.
    ProcessStateIndeterminate,
    /// Token, sequence and Binding snapshot valid; result references check out.
    ResultsVerifiable,
    /// A structured failure classification with a valid token.
    FailureClassified,
    /// Heartbeat or total duration exceeded and reconciliation cannot prove it ran.
    DeadlineExceeded,
    /// Platform restart or lost connection with the fact unknown.
    ConnectionLost,
    /// The Adapter proves the same session/token is still running, with no newer
    /// generation produced.
    SameSessionStillRunning,
    /// A verifiable completion result with matching sequence and token.
    CompletionVerifiable,
    /// The Adapter proves a terminal state, or recovery failed and policy or a
    /// Human decided.
    TerminalProvenOrDecided,
}

impl RunPrecondition {
    /// The stable condition key for the catalogue's `condition_keys` detail.
    pub const fn condition_key(self) -> &'static str {
        match self {
            RunPrecondition::LeaseObtainable => "lease_obtainable",
            RunPrecondition::AdapterPrepared => "adapter_prepared",
            RunPrecondition::NotYetStartedOrStopped => "not_yet_started_or_stopped",
            RunPrecondition::StartConfirmed => "start_confirmed",
            RunPrecondition::StableStartupError => "stable_startup_error",
            RunPrecondition::StartTimeoutElapsed => "start_timeout_elapsed",
            RunPrecondition::CancelConfirmed => "cancel_confirmed",
            RunPrecondition::ProcessStateIndeterminate => "process_state_indeterminate",
            RunPrecondition::ResultsVerifiable => "results_verifiable",
            RunPrecondition::FailureClassified => "failure_classified",
            RunPrecondition::DeadlineExceeded => "deadline_exceeded",
            RunPrecondition::ConnectionLost => "connection_lost",
            RunPrecondition::SameSessionStillRunning => "same_session_still_running",
            RunPrecondition::CompletionVerifiable => "completion_verifiable",
            RunPrecondition::TerminalProvenOrDecided => "terminal_proven_or_decided",
        }
    }
}

/// A `RUN_*` event §3 requires on a successful edge. Described, never emitted.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum RunEvent {
    RunQueued,
    RunStarting,
    RunCancelled,
    RunStarted,
    RunFailed,
    RunTimedOut,
    RunStatusUnknown,
    RunSucceeded,
    RunReconciledRunning,
    RunReconciledSucceeded,
    /// `RUN_RECONCILED_*` covers the FAILED / TIMED_OUT / CANCELLED outcomes of
    /// reconcile; the concrete suffix is chosen by the application layer from the
    /// destination state.
    RunReconciledTerminal,
}

impl RunEvent {
    /// The frozen event type string.
    pub const fn as_str(self) -> &'static str {
        match self {
            RunEvent::RunQueued => "RUN_QUEUED",
            RunEvent::RunStarting => "RUN_STARTING",
            RunEvent::RunCancelled => "RUN_CANCELLED",
            RunEvent::RunStarted => "RUN_STARTED",
            RunEvent::RunFailed => "RUN_FAILED",
            RunEvent::RunTimedOut => "RUN_TIMED_OUT",
            RunEvent::RunStatusUnknown => "RUN_STATUS_UNKNOWN",
            RunEvent::RunSucceeded => "RUN_SUCCEEDED",
            RunEvent::RunReconciledRunning => "RUN_RECONCILED_RUNNING",
            RunEvent::RunReconciledSucceeded => "RUN_RECONCILED_SUCCEEDED",
            RunEvent::RunReconciledTerminal => "RUN_RECONCILED_TERMINAL",
        }
    }
}

impl fmt::Display for RunEvent {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// A legal Run edge, fully described but not applied.
///
/// Fields are private for the reason given on [`crate::task_state_machine::TaskTransition`]: with public
/// fields, `t.to = RunStatus::Succeeded` compiles and the whole machine is
/// bypassable. See `crate::task_state_machine` for the full write-up.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct RunTransition {
    from: RunStatus,
    to: RunStatus,
    command: RunCommand,
    preconditions: &'static [RunPrecondition],
    events: &'static [RunEvent],
}

impl RunTransition {
    /// The state the Run was in; not changed by the decision.
    pub const fn from(&self) -> RunStatus {
        self.from
    }
    /// The state the Run moves to, as decided by the §3 graph.
    pub const fn to(&self) -> RunStatus {
        self.to
    }
    /// The command that produced this edge.
    pub const fn command(&self) -> RunCommand {
        self.command
    }
    /// §3 preconditions the application layer must verify in-transaction.
    pub const fn preconditions(&self) -> &'static [RunPrecondition] {
        self.preconditions
    }
    /// §3 events to write in the same transaction.
    pub const fn events(&self) -> &'static [RunEvent] {
        self.events
    }
}

/// A rejected Run transition. Carries no new state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct RunRejection {
    current: RunStatus,
    command: RunCommand,
    code: RunErrorCode,
}

impl RunRejection {
    /// The state the Run was in; unchanged by this rejection.
    pub const fn current(&self) -> RunStatus {
        self.current
    }
    /// The command that was refused.
    pub const fn command(&self) -> RunCommand {
        self.command
    }
    /// The frozen stable code.
    pub const fn code(&self) -> RunErrorCode {
        self.code
    }
}

impl fmt::Display for RunRejection {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{} rejected `{:?}` in state {}",
            self.code, self.command, self.current
        )
    }
}

impl std::error::Error for RunRejection {}

/// One row of the §3 transition table.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct RunEdge {
    /// Source state.
    pub from: RunStatus,
    /// Command that drives it.
    pub command: RunCommand,
    /// Destination state.
    pub to: RunStatus,
    /// §3 preconditions.
    pub preconditions: &'static [RunPrecondition],
    /// §3 events.
    pub events: &'static [RunEvent],
    /// The §3 主要拒绝码, or the derived `DOMAIN_PRECONDITION_FAILED` where §3
    /// prints `—` (five rows — see [`RunErrorCode`]).
    pub rejection: RunErrorCode,
}

/// The complete §3 transition table, in table order.
pub const RUN_EDGES: &[RunEdge] = &[
    // 创建 → QUEUED is a creation edge and is handled by `creation_target`.
    RunEdge {
        from: RunStatus::Queued,
        command: RunCommand::Start,
        to: RunStatus::Starting,
        preconditions: &[RunPrecondition::AdapterPrepared],
        events: &[RunEvent::RunStarting],
        rejection: RunErrorCode::AdapterUnavailable,
    },
    RunEdge {
        from: RunStatus::Queued,
        command: RunCommand::Cancel,
        to: RunStatus::Cancelled,
        preconditions: &[RunPrecondition::NotYetStartedOrStopped],
        events: &[RunEvent::RunCancelled],
        rejection: RunErrorCode::PermissionDenied,
    },
    RunEdge {
        from: RunStatus::Starting,
        command: RunCommand::ConfirmRunning,
        to: RunStatus::Running,
        preconditions: &[RunPrecondition::StartConfirmed],
        events: &[RunEvent::RunStarted],
        rejection: RunErrorCode::BindingStaleVersion,
    },
    RunEdge {
        from: RunStatus::Starting,
        command: RunCommand::Fail,
        to: RunStatus::Failed,
        preconditions: &[RunPrecondition::StableStartupError],
        events: &[RunEvent::RunFailed],
        rejection: RunErrorCode::PreconditionFailed,
    },
    RunEdge {
        from: RunStatus::Starting,
        command: RunCommand::Timeout,
        to: RunStatus::TimedOut,
        preconditions: &[RunPrecondition::StartTimeoutElapsed],
        events: &[RunEvent::RunTimedOut],
        rejection: RunErrorCode::PreconditionFailed,
    },
    RunEdge {
        from: RunStatus::Starting,
        command: RunCommand::Cancel,
        to: RunStatus::Cancelled,
        preconditions: &[RunPrecondition::CancelConfirmed],
        events: &[RunEvent::RunCancelled],
        rejection: RunErrorCode::RunCancelNotConfirmed,
    },
    RunEdge {
        from: RunStatus::Starting,
        command: RunCommand::MarkUnknown,
        to: RunStatus::Unknown,
        preconditions: &[RunPrecondition::ProcessStateIndeterminate],
        events: &[RunEvent::RunStatusUnknown],
        rejection: RunErrorCode::PreconditionFailed,
    },
    RunEdge {
        from: RunStatus::Running,
        command: RunCommand::Succeed,
        to: RunStatus::Succeeded,
        preconditions: &[RunPrecondition::ResultsVerifiable],
        events: &[RunEvent::RunSucceeded],
        rejection: RunErrorCode::RunFenced,
    },
    RunEdge {
        from: RunStatus::Running,
        command: RunCommand::Fail,
        to: RunStatus::Failed,
        preconditions: &[RunPrecondition::FailureClassified],
        events: &[RunEvent::RunFailed],
        rejection: RunErrorCode::RunFenced,
    },
    RunEdge {
        from: RunStatus::Running,
        command: RunCommand::Timeout,
        to: RunStatus::TimedOut,
        preconditions: &[RunPrecondition::DeadlineExceeded],
        events: &[RunEvent::RunTimedOut],
        rejection: RunErrorCode::PreconditionFailed,
    },
    RunEdge {
        from: RunStatus::Running,
        command: RunCommand::Cancel,
        to: RunStatus::Cancelled,
        preconditions: &[RunPrecondition::CancelConfirmed],
        events: &[RunEvent::RunCancelled],
        rejection: RunErrorCode::PermissionDenied,
    },
    RunEdge {
        from: RunStatus::Running,
        command: RunCommand::MarkUnknown,
        to: RunStatus::Unknown,
        preconditions: &[RunPrecondition::ConnectionLost],
        events: &[RunEvent::RunStatusUnknown],
        rejection: RunErrorCode::PreconditionFailed,
    },
    RunEdge {
        from: RunStatus::Unknown,
        command: RunCommand::Reconcile(ReconcileOutcome::StillRunning),
        to: RunStatus::Running,
        preconditions: &[RunPrecondition::SameSessionStillRunning],
        events: &[RunEvent::RunReconciledRunning],
        rejection: RunErrorCode::RunFenced,
    },
    RunEdge {
        from: RunStatus::Unknown,
        command: RunCommand::Reconcile(ReconcileOutcome::Succeeded),
        to: RunStatus::Succeeded,
        preconditions: &[RunPrecondition::CompletionVerifiable],
        events: &[RunEvent::RunReconciledSucceeded],
        rejection: RunErrorCode::ArtifactInvalid,
    },
    RunEdge {
        from: RunStatus::Unknown,
        command: RunCommand::Reconcile(ReconcileOutcome::Failed),
        to: RunStatus::Failed,
        preconditions: &[RunPrecondition::TerminalProvenOrDecided],
        events: &[RunEvent::RunReconciledTerminal],
        rejection: RunErrorCode::PreconditionFailed,
    },
    RunEdge {
        from: RunStatus::Unknown,
        command: RunCommand::Reconcile(ReconcileOutcome::TimedOut),
        to: RunStatus::TimedOut,
        preconditions: &[RunPrecondition::TerminalProvenOrDecided],
        events: &[RunEvent::RunReconciledTerminal],
        rejection: RunErrorCode::PreconditionFailed,
    },
    RunEdge {
        from: RunStatus::Unknown,
        command: RunCommand::Reconcile(ReconcileOutcome::Cancelled),
        to: RunStatus::Cancelled,
        preconditions: &[RunPrecondition::TerminalProvenOrDecided],
        events: &[RunEvent::RunReconciledTerminal],
        rejection: RunErrorCode::PreconditionFailed,
    },
];

/// The single legal creation target: `创建 → QUEUED`.
pub const CREATE_QUEUED: RunCreation = RunCreation {
    initial: RunStatus::Queued,
    preconditions: &[RunPrecondition::LeaseObtainable],
    events: &[RunEvent::RunQueued],
    rejection: RunErrorCode::AlreadyClaimed,
};

/// The only state §3 allows a Run to be created into.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct RunCreation {
    initial: RunStatus,
    preconditions: &'static [RunPrecondition],
    events: &'static [RunEvent],
    rejection: RunErrorCode,
}

impl RunCreation {
    /// The state the new Run starts in.
    pub const fn initial(&self) -> RunStatus {
        self.initial
    }
    /// Preconditions the application layer must verify before persisting.
    pub const fn preconditions(&self) -> &'static [RunPrecondition] {
        self.preconditions
    }
    /// Events to write for the creation itself.
    pub const fn events(&self) -> &'static [RunEvent] {
        self.events
    }
    /// The frozen rejection code when the creation preconditions are unmet.
    pub const fn rejection(&self) -> RunErrorCode {
        self.rejection
    }
}

/// Decide whether `command` is legal in `current`.
///
/// Pure: no I/O, no mutation, no clock. A missing edge yields
/// [`RunErrorCode::InvalidTransition`]; a legal edge carries its own §3 code.
///
/// Total on its domain: every `(from, command)` pair in [`RUN_EDGES`] appears
/// exactly once, so there is never a tie to break. An ambiguous command is not
/// representable — see [`ReconcileOutcome`].
pub fn transition(current: RunStatus, command: RunCommand) -> Result<RunTransition, RunRejection> {
    match RUN_EDGES
        .iter()
        .find(|edge| edge.from == current && edge.command == command)
    {
        Some(edge) => Ok(RunTransition {
            from: edge.from,
            to: edge.to,
            command: edge.command,
            preconditions: edge.preconditions,
            events: edge.events,
        }),
        None => Err(RunRejection {
            current,
            command,
            code: RunErrorCode::InvalidTransition,
        }),
    }
}

/// The rejection code for a legal-but-unmet edge, or `None` when the command is
/// not a legal edge at all.
pub fn rejection_code(current: RunStatus, command: RunCommand) -> Option<RunErrorCode> {
    RUN_EDGES
        .iter()
        .find(|edge| edge.from == current && edge.command == command)
        .map(|edge| edge.rejection)
}
