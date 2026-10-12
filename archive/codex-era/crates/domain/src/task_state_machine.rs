//! Task lifecycle state machine.
//!
//! Frozen source: `docs/specs/STATE_MACHINES.md` §2 and the transition table in
//! §2.1; `docs/development/TECH_STACK_AND_STANDARDS.md` §5 requires every state
//! machine to be an explicit, testable, **pure** function.
//!
//! # What "pure" buys, and where the line sits
//!
//! This module decides **only** whether an edge exists in the §2 graph and which
//! frozen rejection code that edge carries. It performs no I/O, holds no state,
//! and cannot emit an event.
//!
//! Everything §2.1 lists under 前置条件 — hard dependencies being `DONE`, the
//! owner Role being `ACTIVE`, an active Run being cancelled or fenced, a result
//! Artifact existing, a reviewer not being the requester — needs to read the
//! database. `TECH_STACK_AND_STANDARDS.md` §5 already places that work in the
//! transaction: "Task 领取在同一事务中校验状态、依赖、Binding、权限、并发上限".
//! So the boundary is:
//!
//! * `domain` — is this edge legal, and what does it require? (this module)
//! * `application` — are the requirements met, and what does it emit?
//!
//! # Zero side effect on rejection
//!
//! A rejected transition returns [`TaskRejection`] and nothing else. There is no
//! `Partial` value, no "applied but flagged" path, and no way to observe a
//! mutation. `from` is never rewritten, no event is described, and no version
//! bump is implied — satisfying §5's "非法迁移零状态副作用" at the type level
//! rather than by convention.

use std::fmt;

use serde::{Deserialize, Serialize};

/// A Task's position in the §2 lifecycle.
///
/// The seven values are exactly the states named in §2. Notably absent is
/// `FAILED`: §2.2 states "任意状态 → FAILED：Task 无 FAILED 状态，失败属于 Run".
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum TaskStatus {
    /// Created without complete information, or with dependencies still unmet.
    Todo,
    /// Eligible to be claimed.
    Ready,
    /// Claimed; a Run is active.
    Doing,
    /// Blocked; an Attention item is open.
    Blocked,
    /// Awaiting review.
    Review,
    /// Accepted. Re-openable to `Ready`.
    Done,
    /// P0 terminal state. §2.2 forbids leaving it.
    Cancelled,
}

impl TaskStatus {
    /// Every state in the machine, in a stable order.
    pub const ALL: [TaskStatus; 7] = [
        TaskStatus::Todo,
        TaskStatus::Ready,
        TaskStatus::Doing,
        TaskStatus::Blocked,
        TaskStatus::Review,
        TaskStatus::Done,
        TaskStatus::Cancelled,
    ];

    /// The wire name used in `TASK_*` events and the UI.
    pub const fn as_str(self) -> &'static str {
        match self {
            TaskStatus::Todo => "TODO",
            TaskStatus::Ready => "READY",
            TaskStatus::Doing => "DOING",
            TaskStatus::Blocked => "BLOCKED",
            TaskStatus::Review => "REVIEW",
            TaskStatus::Done => "DONE",
            TaskStatus::Cancelled => "CANCELLED",
        }
    }
}

impl fmt::Display for TaskStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// A command that may drive a Task.
///
/// Creation is handled separately by [`creation_target`]: §2.1 lists "创建 → TODO"
/// and "创建 → READY" as two distinct creation edges, so a freshly created Task
/// does not start from an existing state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum TaskCommand {
    MarkReady,
    Cancel,
    Claim,
    Block,
    SubmitReview,
    Complete,
    ResolveBlock,
    ApproveReview,
    RequestChanges,
    Reopen,
}

impl TaskCommand {
    /// The command name used in `task.*` action strings.
    pub const fn as_str(self) -> &'static str {
        match self {
            TaskCommand::MarkReady => "task.mark_ready",
            TaskCommand::Cancel => "task.cancel",
            TaskCommand::Claim => "task.claim",
            TaskCommand::Block => "task.block",
            TaskCommand::SubmitReview => "task.submit_review",
            TaskCommand::Complete => "task.complete",
            TaskCommand::ResolveBlock => "task.resolve_block",
            TaskCommand::ApproveReview => "review.approve",
            TaskCommand::RequestChanges => "review.request_changes",
            TaskCommand::Reopen => "task.reopen",
        }
    }
}

impl fmt::Display for TaskCommand {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Stable rejection codes, frozen in `docs/specs/ERROR_CATALOG.md`.
///
/// The set is deliberately **not** exhaustive over the catalogue: it holds only
/// the codes §2.1 attaches to Task lifecycle edges. Every value here is copied
/// verbatim from the frozen catalogue; no code was invented, and `as_str` is
/// the protocol representation.
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
pub enum TaskErrorCode {
    /// Not a legal edge in the §2 graph at all.
    #[serde(rename = "DOMAIN_INVALID_TRANSITION")]
    InvalidTransition,
    /// A legal edge, but a §2.1 precondition is unmet.
    #[serde(rename = "DOMAIN_PRECONDITION_FAILED")]
    PreconditionFailed,
    #[serde(rename = "AUTH_PERMISSION_DENIED")]
    PermissionDenied,
    #[serde(rename = "TASK_DEPENDENCY_UNMET")]
    DependencyUnmet,
    #[serde(rename = "TASK_ALREADY_CLAIMED")]
    AlreadyClaimed,
    #[serde(rename = "TASK_BLOCK_REASON_REQUIRED")]
    BlockReasonRequired,
    #[serde(rename = "TASK_RESULT_REQUIRED")]
    ResultRequired,
    #[serde(rename = "TASK_REVIEW_REQUIRED")]
    ReviewRequired,
    #[serde(rename = "REVIEW_STALE_RESULT")]
    StaleReviewResult,
    #[serde(rename = "REVIEW_COMMENT_REQUIRED")]
    ReviewCommentRequired,
    #[serde(rename = "RUN_CANCEL_NOT_CONFIRMED")]
    RunCancelNotConfirmed,
    #[serde(rename = "RUN_FENCED")]
    RunFenced,
    #[serde(rename = "VALIDATION_INVALID_FIELD")]
    ValidationInvalidField,
}

impl TaskErrorCode {
    /// The frozen wire name.
    pub const fn as_str(self) -> &'static str {
        match self {
            TaskErrorCode::InvalidTransition => "DOMAIN_INVALID_TRANSITION",
            TaskErrorCode::PreconditionFailed => "DOMAIN_PRECONDITION_FAILED",
            TaskErrorCode::PermissionDenied => "AUTH_PERMISSION_DENIED",
            TaskErrorCode::DependencyUnmet => "TASK_DEPENDENCY_UNMET",
            TaskErrorCode::AlreadyClaimed => "TASK_ALREADY_CLAIMED",
            TaskErrorCode::BlockReasonRequired => "TASK_BLOCK_REASON_REQUIRED",
            TaskErrorCode::ResultRequired => "TASK_RESULT_REQUIRED",
            TaskErrorCode::ReviewRequired => "TASK_REVIEW_REQUIRED",
            TaskErrorCode::StaleReviewResult => "REVIEW_STALE_RESULT",
            TaskErrorCode::ReviewCommentRequired => "REVIEW_COMMENT_REQUIRED",
            TaskErrorCode::RunCancelNotConfirmed => "RUN_CANCEL_NOT_CONFIRMED",
            TaskErrorCode::RunFenced => "RUN_FENCED",
            TaskErrorCode::ValidationInvalidField => "VALIDATION_INVALID_FIELD",
        }
    }

    /// `true` for codes the catalogue marks retryable.
    ///
    /// Only `TASK_ALREADY_CLAIMED` and `RUN_CANCEL_NOT_CONFIRMED` carry
    /// `retryable=true` in §3 of the catalogue; the catalogue's rule is that
    /// retryability means "the same business intent may be retried once the
    /// suggested condition holds", never "the client should retry forever".
    pub const fn is_retryable(self) -> bool {
        matches!(
            self,
            TaskErrorCode::AlreadyClaimed | TaskErrorCode::RunCancelNotConfirmed
        )
    }
}

impl fmt::Display for TaskErrorCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// A precondition §2.1 attaches to an edge.
///
/// These are *descriptions*, not checks. The application layer evaluates them
/// inside the transaction and must reject with [`TaskErrorCode`] if one is unmet;
/// this module never learns whether they held.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum TaskPrecondition {
    /// Owner is an `ACTIVE` Role, references are valid, all hard dependencies are
    /// `DONE`, and the Role can receive work.
    InformationComplete,
    /// No active Run.
    NoActiveRun,
    /// A valid `Binding` and no effective Run/lease, dependencies satisfied,
    /// version matches, permission passes.
    Claimable,
    /// A short reason or a valid question reference was supplied, and the active
    /// Run entered its pause/failure policy.
    BlockReasonSupplied,
    /// At least one accessible result Artifact exists, and the active Run has
    /// `SUCCEEDED` or the submission is atomic.
    ResultAvailable,
    /// The Task may skip review and an acceptance record exists.
    ReviewExempt,
    /// Run cancel/fence confirmed; the Task alone may not be changed.
    RunCancelConfirmed,
    /// The block is resolved with a reference; owner/dependency/references valid.
    BlockResolved,
    /// No write lease is still effective.
    LeaseFenced,
    /// The Review matches the current Task version and its result; acceptance items
    /// pass.
    ReviewCurrent,
    /// A comment was supplied and the previous Run is terminal.
    ChangesCommented,
    /// A reopen reason was supplied; owner and references are still valid.
    ReopenReasonSupplied,
}

impl TaskPrecondition {
    /// The stable condition key, matching the catalogue's `condition_keys`
    /// detail convention.
    pub const fn condition_key(self) -> &'static str {
        match self {
            TaskPrecondition::InformationComplete => "information_complete",
            TaskPrecondition::NoActiveRun => "no_active_run",
            TaskPrecondition::Claimable => "claimable",
            TaskPrecondition::BlockReasonSupplied => "block_reason_supplied",
            TaskPrecondition::ResultAvailable => "result_available",
            TaskPrecondition::ReviewExempt => "review_exempt",
            TaskPrecondition::RunCancelConfirmed => "run_cancel_confirmed",
            TaskPrecondition::BlockResolved => "block_resolved",
            TaskPrecondition::LeaseFenced => "lease_fenced",
            TaskPrecondition::ReviewCurrent => "review_current",
            TaskPrecondition::ChangesCommented => "changes_commented",
            TaskPrecondition::ReopenReasonSupplied => "reopen_reason_supplied",
        }
    }
}

/// A domain event §2.1 requires on a successful edge.
///
/// Described, never emitted: writing the `TASK_*` row is the application layer's
/// job, in the same transaction as the state change.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum TaskEvent {
    TaskCreated,
    TaskReady,
    TaskCancelled,
    TaskClaimed,
    RunQueued,
    TaskBlocked,
    TaskReviewRequested,
    TaskCompleted,
    TaskUnblocked,
    TaskReviewApproved,
    TaskReviewChangesRequested,
    TaskReopened,
}

impl TaskEvent {
    /// The frozen event type string.
    pub const fn as_str(self) -> &'static str {
        match self {
            TaskEvent::TaskCreated => "TASK_CREATED",
            TaskEvent::TaskReady => "TASK_READY",
            TaskEvent::TaskCancelled => "TASK_CANCELLED",
            TaskEvent::TaskClaimed => "TASK_CLAIMED",
            TaskEvent::RunQueued => "RUN_QUEUED",
            TaskEvent::TaskBlocked => "TASK_BLOCKED",
            TaskEvent::TaskReviewRequested => "TASK_REVIEW_REQUESTED",
            TaskEvent::TaskCompleted => "TASK_COMPLETED",
            TaskEvent::TaskUnblocked => "TASK_UNBLOCKED",
            TaskEvent::TaskReviewApproved => "TASK_REVIEW_APPROVED",
            TaskEvent::TaskReviewChangesRequested => "TASK_REVIEW_CHANGES_REQUESTED",
            TaskEvent::TaskReopened => "TASK_REOPENED",
        }
    }
}

/// A legal edge, fully described but not applied.
///
/// Returning this is a *decision*, not a mutation. The caller validates
/// [`TaskTransition::preconditions`] first and only then conditionally writes the
/// new status together with [`TaskTransition::events`], never the other way round.
///
/// The fields are private on purpose. With public fields this compiles:
///
/// ```ignore
/// let mut t = transition(TaskStatus::Ready, TaskCommand::Claim)?;
/// t.to = TaskStatus::Done;                 // never approved by the machine
/// persist(t);                              // bypasses §2 entirely
/// ```
///
/// so the caller could rewrite the decision the state machine just made. The
/// machine's output is a *value*, per `TECH_STACK_AND_STANDARDS.md` §5 ("输出新状态"),
/// and a value the caller can edit is not one. Read access stays open through the
/// accessors below, so nothing that legitimately consumes a transition is blocked.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct TaskTransition {
    from: TaskStatus,
    to: TaskStatus,
    command: TaskCommand,
    preconditions: &'static [TaskPrecondition],
    events: &'static [TaskEvent],
}

impl TaskTransition {
    /// The state the Task was in; it is not changed by the decision.
    pub const fn from(&self) -> TaskStatus {
        self.from
    }
    /// The state the Task moves to, as decided by the §2 graph.
    pub const fn to(&self) -> TaskStatus {
        self.to
    }
    /// The command that produced this edge.
    pub const fn command(&self) -> TaskCommand {
        self.command
    }
    /// §2.1 preconditions the application layer must verify in-transaction.
    pub const fn preconditions(&self) -> &'static [TaskPrecondition] {
        self.preconditions
    }
    /// §2.1 events to write in the same transaction.
    pub const fn events(&self) -> &'static [TaskEvent] {
        self.events
    }
}

/// A rejected transition.
///
/// Carries no state, so there is nothing an illegal command could have already
/// changed. The catalogue detail fields (`current_state`, `requested_state`,
/// `condition_keys`) are named here so the application layer can fill them
/// without inventing new envelope keys.
///
/// Fields are private for the same reason as [`TaskTransition`]: a rejection that
/// a caller can edit is not evidence that the machine refused anything.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct TaskRejection {
    current: TaskStatus,
    command: TaskCommand,
    code: TaskErrorCode,
    condition_keys: &'static [&'static str],
}

impl TaskRejection {
    /// The state the Task was in; unchanged by this rejection.
    pub const fn current(&self) -> TaskStatus {
        self.current
    }
    /// The command that was refused.
    pub const fn command(&self) -> TaskCommand {
        self.command
    }
    /// The frozen stable code.
    pub const fn code(&self) -> TaskErrorCode {
        self.code
    }
    /// Precondition keys the caller may surface, when the rejection is
    /// "edge exists but unmet" rather than "no such edge".
    pub const fn condition_keys(&self) -> &'static [&'static str] {
        self.condition_keys
    }
}

impl fmt::Display for TaskRejection {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{} rejected `{}` in state {} ({})",
            self.code, self.command, self.current, self.code
        )
    }
}

impl std::error::Error for TaskRejection {}

/// One row of the §2.1 transition table.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct TaskEdge {
    /// Source state.
    pub from: TaskStatus,
    /// Command that drives it.
    pub command: TaskCommand,
    /// Destination state.
    pub to: TaskStatus,
    /// §2.1 preconditions.
    pub preconditions: &'static [TaskPrecondition],
    /// §2.1 events.
    pub events: &'static [TaskEvent],
    /// The §2.1 主要拒绝码 for this edge.
    pub rejection: TaskErrorCode,
}

const NO_PRECONDITIONS: &[TaskPrecondition] = &[];

/// The complete §2.1 transition table, in table order.
///
/// This is the single source of truth for the machine: [`transition`] looks up
/// here and nowhere else, so a table edit is a behaviour edit and the table-driven
/// test walks the same data the implementation does.
pub const TASK_EDGES: &[TaskEdge] = &[
    // 创建 → TODO | 创建 → READY are creation edges and are deliberately absent:
    // they have no source state. See `creation_target`.
    TaskEdge {
        from: TaskStatus::Todo,
        command: TaskCommand::MarkReady,
        to: TaskStatus::Ready,
        preconditions: &[TaskPrecondition::InformationComplete],
        events: &[TaskEvent::TaskReady],
        rejection: TaskErrorCode::PreconditionFailed,
    },
    TaskEdge {
        from: TaskStatus::Todo,
        command: TaskCommand::Cancel,
        to: TaskStatus::Cancelled,
        preconditions: &[TaskPrecondition::NoActiveRun],
        events: &[TaskEvent::TaskCancelled],
        rejection: TaskErrorCode::PermissionDenied,
    },
    TaskEdge {
        from: TaskStatus::Ready,
        command: TaskCommand::Claim,
        to: TaskStatus::Doing,
        preconditions: &[TaskPrecondition::Claimable],
        events: &[TaskEvent::TaskClaimed, TaskEvent::RunQueued],
        rejection: TaskErrorCode::AlreadyClaimed,
    },
    TaskEdge {
        from: TaskStatus::Ready,
        command: TaskCommand::Cancel,
        to: TaskStatus::Cancelled,
        preconditions: &[TaskPrecondition::NoActiveRun],
        events: &[TaskEvent::TaskCancelled],
        rejection: TaskErrorCode::PreconditionFailed,
    },
    TaskEdge {
        from: TaskStatus::Doing,
        command: TaskCommand::Block,
        to: TaskStatus::Blocked,
        preconditions: &[
            TaskPrecondition::BlockReasonSupplied,
            TaskPrecondition::NoActiveRun,
        ],
        events: &[TaskEvent::TaskBlocked],
        rejection: TaskErrorCode::BlockReasonRequired,
    },
    TaskEdge {
        from: TaskStatus::Doing,
        command: TaskCommand::SubmitReview,
        to: TaskStatus::Review,
        preconditions: &[TaskPrecondition::ResultAvailable],
        events: &[TaskEvent::TaskReviewRequested],
        rejection: TaskErrorCode::ResultRequired,
    },
    TaskEdge {
        from: TaskStatus::Doing,
        command: TaskCommand::Complete,
        to: TaskStatus::Done,
        preconditions: &[TaskPrecondition::ReviewExempt],
        events: &[TaskEvent::TaskCompleted],
        rejection: TaskErrorCode::ReviewRequired,
    },
    TaskEdge {
        from: TaskStatus::Doing,
        command: TaskCommand::Cancel,
        to: TaskStatus::Cancelled,
        preconditions: &[TaskPrecondition::RunCancelConfirmed],
        events: &[TaskEvent::TaskCancelled],
        rejection: TaskErrorCode::RunCancelNotConfirmed,
    },
    TaskEdge {
        from: TaskStatus::Blocked,
        command: TaskCommand::ResolveBlock,
        to: TaskStatus::Ready,
        preconditions: &[TaskPrecondition::BlockResolved],
        events: &[TaskEvent::TaskUnblocked, TaskEvent::TaskReady],
        rejection: TaskErrorCode::PreconditionFailed,
    },
    TaskEdge {
        from: TaskStatus::Blocked,
        command: TaskCommand::Cancel,
        to: TaskStatus::Cancelled,
        preconditions: &[TaskPrecondition::LeaseFenced],
        events: &[TaskEvent::TaskCancelled],
        rejection: TaskErrorCode::RunFenced,
    },
    TaskEdge {
        from: TaskStatus::Review,
        command: TaskCommand::ApproveReview,
        to: TaskStatus::Done,
        preconditions: &[TaskPrecondition::ReviewCurrent],
        events: &[TaskEvent::TaskReviewApproved, TaskEvent::TaskCompleted],
        rejection: TaskErrorCode::StaleReviewResult,
    },
    TaskEdge {
        from: TaskStatus::Review,
        command: TaskCommand::RequestChanges,
        to: TaskStatus::Ready,
        preconditions: &[TaskPrecondition::ChangesCommented],
        events: &[TaskEvent::TaskReviewChangesRequested, TaskEvent::TaskReady],
        rejection: TaskErrorCode::ReviewCommentRequired,
    },
    TaskEdge {
        from: TaskStatus::Review,
        command: TaskCommand::Cancel,
        to: TaskStatus::Cancelled,
        preconditions: &[TaskPrecondition::NoActiveRun],
        events: &[TaskEvent::TaskCancelled],
        rejection: TaskErrorCode::PermissionDenied,
    },
    TaskEdge {
        from: TaskStatus::Done,
        command: TaskCommand::Reopen,
        to: TaskStatus::Ready,
        preconditions: &[TaskPrecondition::ReopenReasonSupplied],
        events: &[TaskEvent::TaskReopened, TaskEvent::TaskReady],
        rejection: TaskErrorCode::PreconditionFailed,
    },
];

/// The two states §2.1 allows a Task to be *created* into.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct TaskCreation {
    initial: TaskStatus,
    preconditions: &'static [TaskPrecondition],
    events: &'static [TaskEvent],
    rejection: TaskErrorCode,
}

impl TaskCreation {
    /// The state the new Task starts in.
    pub const fn initial(&self) -> TaskStatus {
        self.initial
    }
    /// Preconditions the application layer must verify before persisting.
    pub const fn preconditions(&self) -> &'static [TaskPrecondition] {
        self.preconditions
    }
    /// Events to write for the creation itself.
    pub const fn events(&self) -> &'static [TaskEvent] {
        self.events
    }
    /// The frozen rejection code when the creation preconditions are unmet.
    pub const fn rejection(&self) -> TaskErrorCode {
        self.rejection
    }
}

/// `创建 → TODO`.
pub const CREATE_TODO: TaskCreation = TaskCreation {
    initial: TaskStatus::Todo,
    preconditions: NO_PRECONDITIONS,
    events: &[TaskEvent::TaskCreated],
    rejection: TaskErrorCode::ValidationInvalidField,
};

/// `创建 → READY`.
///
/// §2.1 requires an `ACTIVE` owner Role, valid references, all hard dependencies
/// `DONE`, and a Role able to receive work; unmet dependencies are the specific
/// `TASK_DEPENDENCY_UNMET` case.
pub const CREATE_READY: TaskCreation = TaskCreation {
    initial: TaskStatus::Ready,
    preconditions: &[TaskPrecondition::InformationComplete],
    events: &[TaskEvent::TaskCreated, TaskEvent::TaskReady],
    rejection: TaskErrorCode::DependencyUnmet,
};

/// Both legal creation targets, keyed by the status the caller wants.
pub const TASK_CREATIONS: &[TaskCreation] = &[CREATE_TODO, CREATE_READY];

/// Resolve the creation target for a desired initial state.
pub fn creation_target(initial: TaskStatus) -> Option<&'static TaskCreation> {
    TASK_CREATIONS.iter().find(|c| c.initial == initial)
}

/// Decide whether `command` is legal in `current`.
///
/// Pure: no I/O, no mutation, no clock. On success it returns the fully
/// described edge; on failure it returns a [`TaskRejection`] that still names the
/// untouched `current` state.
///
/// The distinction the catalogue cares about:
///
/// * **no such edge** — `DOMAIN_INVALID_TRANSITION`, the command is not in the
///   graph at all (for example `TODO → DOING`, forbidden by §2.2);
/// * **edge exists, precondition unmet** — the edge's own §2.1 rejection code,
///   so the caller learns *which* condition to surface.
pub fn transition(
    current: TaskStatus,
    command: TaskCommand,
) -> Result<TaskTransition, TaskRejection> {
    match TASK_EDGES
        .iter()
        .find(|edge| edge.from == current && edge.command == command)
    {
        Some(edge) => Ok(TaskTransition {
            from: edge.from,
            to: edge.to,
            command: edge.command,
            preconditions: edge.preconditions,
            events: edge.events,
        }),
        None => Err(TaskRejection {
            current,
            command,
            code: TaskErrorCode::InvalidTransition,
            condition_keys: &[],
        }),
    }
}

/// The rejection code for a legal-but-unmet edge, or `None` when the command is
/// not a legal edge at all.
///
/// The application layer calls this after its own precondition checks fail, so
/// it can report the catalogue's specific code instead of the generic one.
pub fn rejection_code(current: TaskStatus, command: TaskCommand) -> Option<TaskErrorCode> {
    TASK_EDGES
        .iter()
        .find(|edge| edge.from == current && edge.command == command)
        .map(|edge| edge.rejection)
}
