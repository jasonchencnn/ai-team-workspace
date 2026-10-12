//! Binding lifecycle state machine.
//!
//! Frozen source: `docs/specs/STATE_MACHINES.md` §4 and its transition table;
//! `docs/development/TECH_STACK_AND_STANDARDS.md` §5.
//!
//! Same layering as [`crate::task_state_machine`] and [`crate::run_state_machine`]:
//! this module decides whether an edge exists in the §4 graph and which frozen
//! rejection code it carries. Adapter health, capability/version/credential
//! checks, active-Run counts and effective-dating all need to read the database,
//! so they are described here and evaluated by the application layer in the same
//! transaction.
//!
//! # There is no `UNBOUND` state, on purpose
//!
//! §4: "未绑定由'无 ACTIVE Binding'表达，不创建 `UNBOUND` 伪记录". So "not bound"
//! is the *absence* of an `ACTIVE` row, not a state a row can be in. The enum
//! cannot spell `UNBOUND`, and `there_is_no_unbound_state` in the test suite
//! asserts the negative space.
//!
//! # `ROLE_BOUND` vs `ROLE_REBOUND` is two edges, not one conditional
//!
//! §4's `PENDING_CHECK → ACTIVE` row prints the Event cell as
//! "`ROLE_BOUND`/`ROLE_REBOUND`". A single edge would leave `events()` ambiguous
//! and push the choice onto the caller — the same shape that made Run's
//! unqualified `Reconcile` silently resolve to the wrong destination. So the
//! distinction is carried in the command: `Activate` emits `ROLE_BOUND`,
//! `Rebind` emits `ROLE_REBOUND`. Both land on `ACTIVE` with the same §3-style
//! preconditions; they differ only in which fact the application layer proved.
//!
//! # `DRAINING → ACTIVE` is not a second `Activate`
//!
//! §4 gives it its own event (`BINDING_CHANGE_CANCELLED`) and its own rejection
//! code (`BINDING_STALE_VERSION`), so it is `UndoDrain`. Merging it into
//! `Activate` would make one command mean "the check passed" in one state and
//! "the Human called off the swap" in another.

use std::fmt;

use serde::{Deserialize, Serialize};

/// A Binding version's position in the §4 lifecycle.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum BindingStatus {
    /// A new version exists and is being checked. Not yet effective.
    PendingCheck,
    /// The effective Binding for its Role. At most one per Role.
    Active,
    /// Swapped out at the caller's request; existing Runs may finish, no new
    /// Task may be claimed against it.
    Draining,
    /// `effective_to` written. Terminal.
    Revoked,
    /// The check failed, or a Human cancelled it. Terminal. Does not affect the
    /// Role's previous `ACTIVE` version.
    Failed,
}

impl BindingStatus {
    /// Every state, in §4's declaration order.
    pub const ALL: [BindingStatus; 5] = [
        BindingStatus::PendingCheck,
        BindingStatus::Active,
        BindingStatus::Draining,
        BindingStatus::Revoked,
        BindingStatus::Failed,
    ];

    /// The wire name used in `BINDING_*` / `ROLE_*` events and the UI.
    pub const fn as_str(self) -> &'static str {
        match self {
            BindingStatus::PendingCheck => "PENDING_CHECK",
            BindingStatus::Active => "ACTIVE",
            BindingStatus::Draining => "DRAINING",
            BindingStatus::Revoked => "REVOKED",
            BindingStatus::Failed => "FAILED",
        }
    }

    /// `true` for the states §4 gives no out-edge.
    ///
    /// §4 does not use the word 终态 here the way §3 does, so this is derived
    /// from the table: `REVOKED` and `FAILED` are the only states with no row
    /// leaving them. `DRAINING` has two and is therefore *not* terminal — that
    /// asymmetry is the whole point of the drain, so it is asserted directly in
    /// `draining_is_not_terminal`.
    pub const fn is_terminal(self) -> bool {
        matches!(self, BindingStatus::Revoked | BindingStatus::Failed)
    }
}

impl fmt::Display for BindingStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// A command that may drive a Binding version.
///
/// Deliberately no `as_str()` and no serde derives: §4's Actor/场景 column names
/// roles and services ("Human Owner/Admin", "Binding service"), not a frozen
/// action vocabulary, so any `binding.*` wire string here would be invented
/// protocol. See the same note on [`crate::run_state_machine::RunCommand`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum BindingCommand {
    /// A check passed for a Role that had no previous `ACTIVE` version.
    Activate,
    /// A check passed for a Role that is being re-bound. Same destination,
    /// different event: see the module note on `ROLE_BOUND` / `ROLE_REBOUND`.
    Rebind,
    /// The check failed, or a Human cancelled the pending change.
    FailCheck,
    /// Human chose "完成后换绑". §4's first swap strategy.
    Drain,
    /// Write `effective_to`. Legal from both `ACTIVE` and `DRAINING`, with a
    /// different precondition and rejection code on each.
    Revoke,
    /// Human cancelled a swap that had not taken effect. §4's `DRAINING → ACTIVE`.
    UndoDrain,
}

impl BindingCommand {
    /// Every command, in §4 table order.
    pub const ALL: [BindingCommand; 6] = [
        BindingCommand::Activate,
        BindingCommand::Rebind,
        BindingCommand::FailCheck,
        BindingCommand::Drain,
        BindingCommand::Revoke,
        BindingCommand::UndoDrain,
    ];
}

impl fmt::Display for BindingCommand {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}

/// Stable rejection codes, frozen in `docs/specs/ERROR_CATALOG.md`.
///
/// # Why `PreconditionFailed` exists
///
/// §4's table has seven printed rows and one of them leaves 主要拒绝码 as `—`:
/// `PENDING_CHECK → FAILED`. That row is reached by the check failing or a Human
/// cancelling, both observed rather than requested, so §4 names no code. The
/// catalogue's generic `DOMAIN_PRECONDITION_FAILED` is what the specification
/// points at, and attaching it there is a **derived** choice, recorded rather
/// than assumed.
///
/// # `RUN_ACTIVE_EXISTS`, not `BINDING_ACTIVE_RUNS_EXIST`
///
/// Both strings exist in the catalogue. §4's `DRAINING → REVOKED` row prints
/// `RUN_ACTIVE_EXISTS`, and that is the one used here. The
/// `BINDING_ACTIVE_RUNS_EXIST` near-miss is left alone: the catalogue is a
/// protocol contract, and a "more obviously Binding-flavoured" code is still a
/// code the specification did not choose for this row.
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
pub enum BindingErrorCode {
    /// Not a legal edge in the §4 graph at all.
    #[serde(rename = "DOMAIN_INVALID_TRANSITION")]
    InvalidTransition,
    /// The code §4 attaches to a legal edge.
    #[serde(rename = "BINDING_CHANGE_IN_PROGRESS")]
    BindingChangeInProgress,
    #[serde(rename = "ADAPTER_INCOMPATIBLE")]
    AdapterIncompatible,
    #[serde(rename = "AUTH_HUMAN_REQUIRED")]
    HumanRequired,
    #[serde(rename = "RUN_ACTIVE_EXISTS")]
    RunActiveExists,
    #[serde(rename = "RUN_CANCEL_NOT_CONFIRMED")]
    RunCancelNotConfirmed,
    #[serde(rename = "BINDING_STALE_VERSION")]
    BindingStaleVersion,
    /// Derived: the one §4 row whose 拒绝码 cell is `—`.
    #[serde(rename = "DOMAIN_PRECONDITION_FAILED")]
    PreconditionFailed,
}

impl BindingErrorCode {
    /// The frozen wire name.
    pub const fn as_str(self) -> &'static str {
        match self {
            BindingErrorCode::InvalidTransition => "DOMAIN_INVALID_TRANSITION",
            BindingErrorCode::BindingChangeInProgress => "BINDING_CHANGE_IN_PROGRESS",
            BindingErrorCode::AdapterIncompatible => "ADAPTER_INCOMPATIBLE",
            BindingErrorCode::HumanRequired => "AUTH_HUMAN_REQUIRED",
            BindingErrorCode::RunActiveExists => "RUN_ACTIVE_EXISTS",
            BindingErrorCode::RunCancelNotConfirmed => "RUN_CANCEL_NOT_CONFIRMED",
            BindingErrorCode::BindingStaleVersion => "BINDING_STALE_VERSION",
            BindingErrorCode::PreconditionFailed => "DOMAIN_PRECONDITION_FAILED",
        }
    }

    /// `true` for codes `ERROR_CATALOG.md` marks 是.
    ///
    /// Read out of the catalogue's own table, not from the pair §4 happens to
    /// quote most: `BINDING_CHANGE_IN_PROGRESS` is also 是. Missing it would
    /// present a lock-contention retry as a hard failure.
    pub const fn is_retryable(self) -> bool {
        matches!(
            self,
            BindingErrorCode::BindingChangeInProgress | BindingErrorCode::RunCancelNotConfirmed
        )
    }
}

impl fmt::Display for BindingErrorCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// A precondition §4 attaches to an edge. Described, never checked here.
///
/// The variant names and [`BindingPrecondition::condition_key`] spellings are **derived**: §4 states
/// each precondition in prose and nowhere gives them identifiers, so these are
/// this implementation's names for facts the application layer must verify.
/// `ERROR_CATALOG.md`'s `condition_keys` detail field is a whitelist of field
/// names, not a frozen vocabulary of conditions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum BindingPrecondition {
    /// §4: Role/Harness ACTIVE.
    RoleAndHarnessActive,
    /// §4: 无冲突变更.
    NoConflictingChange,
    /// §4: Adapter 健康.
    AdapterHealthy,
    /// §4: 能力/版本/凭据通过.
    CapabilityVersionCredentialsPass,
    /// §4: Role 无其他 ACTIVE.
    NoOtherActiveBinding,
    /// §4: 检查失败或 Human 取消.
    CheckFailedOrHumanCancelled,
    /// §4 `ACTIVE → DRAINING`: 存在活动 Run.
    ActiveRunsExist,
    /// §4 `DRAINING → REVOKED`: 活动 Run 全部终结.
    AllActiveRunsTerminal,
    /// §4 `DRAINING → REVOKED`: 新 Binding 检查通过.
    ReplacementCheckPassed,
    /// §4 `ACTIVE → REVOKED`: 活动 Run 已完成 cancel+fence，或无活动 Run.
    ActiveRunsCancelledAndFencedOrNone,
    /// §4 `DRAINING → ACTIVE`: 新 Binding 尚未 ACTIVE.
    ReplacementNotYetActive,
    /// §4 `DRAINING → ACTIVE`: 旧 Run 仍合法.
    OldRunsStillLegal,
}

impl BindingPrecondition {
    /// The stable condition key for the catalogue's `condition_keys` detail.
    pub const fn condition_key(self) -> &'static str {
        match self {
            BindingPrecondition::RoleAndHarnessActive => "role_and_harness_active",
            BindingPrecondition::NoConflictingChange => "no_conflicting_change",
            BindingPrecondition::AdapterHealthy => "adapter_healthy",
            BindingPrecondition::CapabilityVersionCredentialsPass => {
                "capability_version_credentials_pass"
            }
            BindingPrecondition::NoOtherActiveBinding => "no_other_active_binding",
            BindingPrecondition::CheckFailedOrHumanCancelled => "check_failed_or_human_cancelled",
            BindingPrecondition::ActiveRunsExist => "active_runs_exist",
            BindingPrecondition::AllActiveRunsTerminal => "all_active_runs_terminal",
            BindingPrecondition::ReplacementCheckPassed => "replacement_check_passed",
            BindingPrecondition::ActiveRunsCancelledAndFencedOrNone => {
                "active_runs_cancelled_and_fenced_or_none"
            }
            BindingPrecondition::ReplacementNotYetActive => "replacement_not_yet_active",
            BindingPrecondition::OldRunsStillLegal => "old_runs_still_legal",
        }
    }
}

/// A `BINDING_*` / `ROLE_*` event §4 requires on a successful edge. Described,
/// never emitted. All seven spellings are transcribed verbatim from §4.
///
/// The variant names are the §4 spellings in Rust casing, so the
/// `SCREAMING_SNAKE_CASE` serde rename reproduces `as_str()` exactly. That
/// agreement is structural, not a coincidence to be re-checked: these variants
/// were once named `RunBindingChangeRequested` (copied from [`crate::run_state_machine::RunEvent`]),
/// which serde expanded to `RUN_BINDING_CHANGE_REQUESTED` while `as_str()`
/// returned §4's `BINDING_CHANGE_REQUESTED` — two different wire names for one
/// event, and the kind of split that only shows up on the wire.
/// `serde_agrees_with_as_str_for_every_event` in the test suite is the tripwire.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum BindingEvent {
    BindingChangeRequested,
    RoleBound,
    RoleRebound,
    BindingChangeFailed,
    BindingDraining,
    RoleUnbound,
    BindingChangeCancelled,
}

impl BindingEvent {
    /// The frozen event type string.
    pub const fn as_str(self) -> &'static str {
        match self {
            BindingEvent::BindingChangeRequested => "BINDING_CHANGE_REQUESTED",
            BindingEvent::RoleBound => "ROLE_BOUND",
            BindingEvent::RoleRebound => "ROLE_REBOUND",
            BindingEvent::BindingChangeFailed => "BINDING_CHANGE_FAILED",
            BindingEvent::BindingDraining => "BINDING_DRAINING",
            BindingEvent::RoleUnbound => "ROLE_UNBOUND",
            BindingEvent::BindingChangeCancelled => "BINDING_CHANGE_CANCELLED",
        }
    }
}

impl fmt::Display for BindingEvent {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// A legal Binding edge, fully described but not applied.
///
/// Fields are private for the reason given on
/// [`crate::task_state_machine::TaskTransition`]: with public fields a caller
/// could rewrite the decision and bypass the machine.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct BindingTransition {
    from: BindingStatus,
    to: BindingStatus,
    command: BindingCommand,
    preconditions: &'static [BindingPrecondition],
    events: &'static [BindingEvent],
}

impl BindingTransition {
    /// The state the version was in; not changed by the decision.
    pub const fn from(&self) -> BindingStatus {
        self.from
    }
    /// The state the version moves to, as decided by the §4 graph.
    pub const fn to(&self) -> BindingStatus {
        self.to
    }
    /// The command that produced this edge.
    pub const fn command(&self) -> BindingCommand {
        self.command
    }
    /// §4 preconditions the application layer must verify in-transaction.
    pub const fn preconditions(&self) -> &'static [BindingPrecondition] {
        self.preconditions
    }
    /// §4 events to write in the same transaction.
    pub const fn events(&self) -> &'static [BindingEvent] {
        self.events
    }
}

/// A rejected Binding transition. Carries no new state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct BindingRejection {
    current: BindingStatus,
    command: BindingCommand,
    code: BindingErrorCode,
}

impl BindingRejection {
    /// The state the version was in; unchanged by this rejection.
    pub const fn current(&self) -> BindingStatus {
        self.current
    }
    /// The command that was refused.
    pub const fn command(&self) -> BindingCommand {
        self.command
    }
    /// The frozen stable code.
    pub const fn code(&self) -> BindingErrorCode {
        self.code
    }
}

impl fmt::Display for BindingRejection {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{} rejected `{:?}` in state {}",
            self.code, self.command, self.current
        )
    }
}

impl std::error::Error for BindingRejection {}

/// One row of the §4 transition table.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct BindingEdge {
    /// Source state.
    pub from: BindingStatus,
    /// Command that drives it.
    pub command: BindingCommand,
    /// Destination state.
    pub to: BindingStatus,
    /// §4 preconditions.
    pub preconditions: &'static [BindingPrecondition],
    /// §4 events.
    pub events: &'static [BindingEvent],
    /// The §4 主要拒绝码, or the derived `DOMAIN_PRECONDITION_FAILED` where §4
    /// prints `—`.
    pub rejection: BindingErrorCode,
}

/// The complete §4 transition table, in table order.
///
/// Seven printed rows become eight edges: `PENDING_CHECK → ACTIVE` is split into
/// `Activate` and `Rebind` because §4 gives that row two possible events.
pub const BINDING_EDGES: &[BindingEdge] = &[
    // 创建 → PENDING_CHECK is a creation edge; see `creation_target`.
    BindingEdge {
        from: BindingStatus::PendingCheck,
        command: BindingCommand::Activate,
        to: BindingStatus::Active,
        preconditions: &[
            BindingPrecondition::AdapterHealthy,
            BindingPrecondition::CapabilityVersionCredentialsPass,
            BindingPrecondition::NoOtherActiveBinding,
        ],
        events: &[BindingEvent::RoleBound],
        rejection: BindingErrorCode::AdapterIncompatible,
    },
    BindingEdge {
        from: BindingStatus::PendingCheck,
        command: BindingCommand::Rebind,
        to: BindingStatus::Active,
        preconditions: &[
            BindingPrecondition::AdapterHealthy,
            BindingPrecondition::CapabilityVersionCredentialsPass,
            BindingPrecondition::NoOtherActiveBinding,
        ],
        events: &[BindingEvent::RoleRebound],
        rejection: BindingErrorCode::AdapterIncompatible,
    },
    BindingEdge {
        from: BindingStatus::PendingCheck,
        command: BindingCommand::FailCheck,
        to: BindingStatus::Failed,
        preconditions: &[BindingPrecondition::CheckFailedOrHumanCancelled],
        events: &[BindingEvent::BindingChangeFailed],
        rejection: BindingErrorCode::PreconditionFailed,
    },
    BindingEdge {
        from: BindingStatus::Active,
        command: BindingCommand::Drain,
        to: BindingStatus::Draining,
        preconditions: &[BindingPrecondition::ActiveRunsExist],
        events: &[BindingEvent::BindingDraining],
        rejection: BindingErrorCode::HumanRequired,
    },
    BindingEdge {
        from: BindingStatus::Draining,
        command: BindingCommand::Revoke,
        to: BindingStatus::Revoked,
        preconditions: &[
            BindingPrecondition::AllActiveRunsTerminal,
            BindingPrecondition::ReplacementCheckPassed,
        ],
        events: &[BindingEvent::RoleUnbound],
        rejection: BindingErrorCode::RunActiveExists,
    },
    BindingEdge {
        from: BindingStatus::Active,
        command: BindingCommand::Revoke,
        to: BindingStatus::Revoked,
        preconditions: &[BindingPrecondition::ActiveRunsCancelledAndFencedOrNone],
        events: &[BindingEvent::RoleUnbound],
        rejection: BindingErrorCode::RunCancelNotConfirmed,
    },
    BindingEdge {
        from: BindingStatus::Draining,
        command: BindingCommand::UndoDrain,
        to: BindingStatus::Active,
        preconditions: &[
            BindingPrecondition::ReplacementNotYetActive,
            BindingPrecondition::OldRunsStillLegal,
        ],
        events: &[BindingEvent::BindingChangeCancelled],
        rejection: BindingErrorCode::BindingStaleVersion,
    },
];

/// The single legal creation target: `创建 → PENDING_CHECK`.
pub const CREATE_PENDING_CHECK: BindingCreation = BindingCreation {
    initial: BindingStatus::PendingCheck,
    preconditions: &[
        BindingPrecondition::RoleAndHarnessActive,
        BindingPrecondition::NoConflictingChange,
    ],
    events: &[BindingEvent::BindingChangeRequested],
    rejection: BindingErrorCode::BindingChangeInProgress,
};

/// The only state §4 allows a Binding version to be created into.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct BindingCreation {
    initial: BindingStatus,
    preconditions: &'static [BindingPrecondition],
    events: &'static [BindingEvent],
    rejection: BindingErrorCode,
}

impl BindingCreation {
    /// The state the new version starts in.
    pub const fn initial(&self) -> BindingStatus {
        self.initial
    }
    /// Preconditions the application layer must verify before persisting.
    pub const fn preconditions(&self) -> &'static [BindingPrecondition] {
        self.preconditions
    }
    /// Events to write for the creation itself.
    pub const fn events(&self) -> &'static [BindingEvent] {
        self.events
    }
    /// The frozen rejection code when the creation preconditions are unmet.
    pub const fn rejection(&self) -> BindingErrorCode {
        self.rejection
    }
}

/// The §4 states a creation may target. §4 has exactly one.
pub const CREATE_TARGETS: &[BindingStatus] = &[BindingStatus::PendingCheck];

/// The §4 creation row, when `status` is a legal target, else `None`.
pub fn creation_target(status: BindingStatus) -> Option<&'static BindingCreation> {
    if CREATE_TARGETS.contains(&status) {
        Some(&CREATE_PENDING_CHECK)
    } else {
        None
    }
}

/// Decide whether `command` is legal in `current`.
///
/// Pure: no I/O, no mutation, no clock. A missing edge yields
/// [`BindingErrorCode::InvalidTransition`]; a legal edge carries its own §4 code.
///
/// Total on its domain: every `(from, command)` pair in [`BINDING_EDGES`] appears
/// exactly once. `Revoke` appears twice, but from different states, so no pair
/// repeats — the invariant is asserted in the test suite.
pub fn transition(
    current: BindingStatus,
    command: BindingCommand,
) -> Result<BindingTransition, BindingRejection> {
    match BINDING_EDGES
        .iter()
        .find(|edge| edge.from == current && edge.command == command)
    {
        Some(edge) => Ok(BindingTransition {
            from: edge.from,
            to: edge.to,
            command: edge.command,
            preconditions: edge.preconditions,
            events: edge.events,
        }),
        None => Err(BindingRejection {
            current,
            command,
            code: BindingErrorCode::InvalidTransition,
        }),
    }
}

/// The rejection code for a legal-but-unmet edge, or `None` when the command is
/// not a legal edge at all.
pub fn rejection_code(current: BindingStatus, command: BindingCommand) -> Option<BindingErrorCode> {
    BINDING_EDGES
        .iter()
        .find(|edge| edge.from == current && edge.command == command)
        .map(|edge| edge.rejection)
}
