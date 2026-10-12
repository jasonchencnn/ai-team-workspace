//! Approval lifecycle state machine.
//!
//! Frozen source: `docs/specs/STATE_MACHINES.md` §7 and its transition table;
//! `docs/specs/ERROR_CATALOG.md`; `docs/specs/PERMISSION_MATRIX.md` §6.
//!
//! Same layering as the Task / Run / Binding / Message machines: this module
//! decides whether an edge exists in the §7 graph and which frozen rejection code
//! it carries. Expiry arithmetic, requester/approver identity, Team scope
//! ceilings and grant validity all need to read the database, so they are
//! described here and evaluated by the application layer in the same
//! transaction.
//!
//! # Every terminal state absorbs, permanently
//!
//! §7: "全部终态不可重开；新请求必须新建 Approval." `APPROVED`, `REJECTED`,
//! `EXPIRED` and `CANCELLED` all have no out-edge, so the rule is structural
//! rather than a rule someone has to remember to enforce. `every_terminal_state_absorbs`
//! in the test suite pins it.
//!
//! # "已批准" is not durable permission
//!
//! §7's closing line: "Approval grant 每次执行时再次校验 action/resource/scope/
//! expiry/actor/context；'已批准'不是永久 Role 权限." That is why the
//! `PENDING → APPROVED` edge declares *no* postcondition beyond the grant's
//! existence, and why its preconditions are re-evaluated on every use rather than
//! baked into the transition. Modelling APPROVED as "grants X" would misstate the
//! specification as a capability grant; the graph only says a restricted grant
//! was *issued*.

use std::fmt;

use serde::{Deserialize, Serialize};

/// An Approval request's position in the §7 lifecycle.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ApprovalStatus {
    /// Requested and awaiting a decision. The only non-terminal state.
    Pending,
    /// A Human decided in favour. Terminal; never reopened.
    Approved,
    /// A Human decided against. Terminal; never reopened.
    Rejected,
    /// The deadline passed. Terminal; never reopened.
    Expired,
    /// The requester or a Human withdrew it. Terminal; never reopened.
    Cancelled,
}

impl ApprovalStatus {
    /// Every state, in §7's declaration order.
    pub const ALL: [ApprovalStatus; 5] = [
        ApprovalStatus::Pending,
        ApprovalStatus::Approved,
        ApprovalStatus::Rejected,
        ApprovalStatus::Expired,
        ApprovalStatus::Cancelled,
    ];

    /// The wire name used in `APPROVAL_*` events and the UI.
    pub const fn as_str(self) -> &'static str {
        match self {
            ApprovalStatus::Pending => "PENDING",
            ApprovalStatus::Approved => "APPROVED",
            ApprovalStatus::Rejected => "REJECTED",
            ApprovalStatus::Expired => "EXPIRED",
            ApprovalStatus::Cancelled => "CANCELLED",
        }
    }

    /// `true` for the four states §7 forbids reopening.
    pub const fn is_terminal(self) -> bool {
        matches!(
            self,
            ApprovalStatus::Approved
                | ApprovalStatus::Rejected
                | ApprovalStatus::Expired
                | ApprovalStatus::Cancelled
        )
    }
}

impl fmt::Display for ApprovalStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// A command that may drive an Approval request.
///
/// Deliberately no `as_str()` and no serde derives: §7's Actor column names
/// roles ("Human Owner/有权 Admin", "requester/Human"), not a frozen command
/// vocabulary. Same stance as [`crate::run_state_machine::RunCommand`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ApprovalCommand {
    /// A Human decided in favour.
    Approve,
    /// A Human decided against.
    Reject,
    /// The deadline passed.
    Expire,
    /// The requester or a Human withdrew the request.
    Cancel,
}

impl ApprovalCommand {
    /// Every command, in §7 table order.
    pub const ALL: [ApprovalCommand; 4] = [
        ApprovalCommand::Approve,
        ApprovalCommand::Reject,
        ApprovalCommand::Expire,
        ApprovalCommand::Cancel,
    ];
}

impl fmt::Display for ApprovalCommand {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}

/// Stable rejection codes, frozen in `docs/specs/ERROR_CATALOG.md`.
///
/// # Why `PreconditionFailed` exists
///
/// §7's table has five printed rows. Four print a concrete 主要拒绝码
/// (`APPROVAL_SCOPE_INVALID`, `AUTH_HUMAN_REQUIRED`,
/// `CONCURRENCY_VERSION_CONFLICT`, `DOMAIN_INVALID_TRANSITION`). The fifth —
/// `PENDING → EXPIRED` — prints `—`, because expiry is observed by the
/// Scheduler rather than requested by a caller. `DOMAIN_PRECONDITION_FAILED`, the
/// catalogue's generic precondition failure, is what the specification points at.
/// Attaching it there is a **derived** choice, recorded rather than assumed.
///
/// # `DOMAIN_INVALID_TRANSITION` as a *legal* edge's rejection code
///
/// Unusual but specified: `PENDING → CANCELLED` is a real edge whose 主要拒绝码
/// is `DOMAIN_INVALID_TRANSITION`. §7 means it as "the cancel was refused because
/// a decision already exists" — a state-level refusal attached to a legal edge,
/// not "this edge does not exist". The machine keeps the two apart: the code
/// appears only on the edge, and a genuinely absent edge still reports the same
/// wire string, which is the specification's choice and not something to "fix".
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ApprovalErrorCode {
    /// Not a legal edge in the §7 graph, and also §7's code for a refused cancel.
    #[serde(rename = "DOMAIN_INVALID_TRANSITION")]
    InvalidTransition,
    /// The code §7 attaches to a legal edge.
    #[serde(rename = "APPROVAL_SCOPE_INVALID")]
    ScopeInvalid,
    #[serde(rename = "AUTH_HUMAN_REQUIRED")]
    HumanRequired,
    #[serde(rename = "CONCURRENCY_VERSION_CONFLICT")]
    ConcurrencyVersionConflict,
    /// Derived: the one §7 row whose 拒绝码 cell is `—`.
    #[serde(rename = "DOMAIN_PRECONDITION_FAILED")]
    PreconditionFailed,
}

impl ApprovalErrorCode {
    /// The frozen wire name.
    pub const fn as_str(self) -> &'static str {
        match self {
            ApprovalErrorCode::InvalidTransition => "DOMAIN_INVALID_TRANSITION",
            ApprovalErrorCode::ScopeInvalid => "APPROVAL_SCOPE_INVALID",
            ApprovalErrorCode::HumanRequired => "AUTH_HUMAN_REQUIRED",
            ApprovalErrorCode::ConcurrencyVersionConflict => "CONCURRENCY_VERSION_CONFLICT",
            ApprovalErrorCode::PreconditionFailed => "DOMAIN_PRECONDITION_FAILED",
        }
    }

    /// `true` for codes `ERROR_CATALOG.md` marks 是.
    ///
    /// `false` for all five. Read out of the catalogue's own table, not assumed.
    pub const fn is_retryable(self) -> bool {
        false
    }
}

impl fmt::Display for ApprovalErrorCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// A precondition §7 attaches to an edge. Described, never checked here.
///
/// Variant names and [`ApprovalPrecondition::condition_key`] spellings are **derived**: §7 states each
/// precondition in prose and freezes no identifiers for them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ApprovalPrecondition {
    /// §7 创建 → PENDING: action、resource scope、理由、期限完整.
    RequestFieldsComplete,
    /// §7 创建 → PENDING: 请求者不能预授权.
    RequesterCannotSelfAuthorize,
    /// §7 PENDING → APPROVED: 未过期.
    NotExpired,
    /// §7 PENDING → APPROVED: decider 与 requester 不构成自批.
    DeciderIsNotRequester,
    /// §7 PENDING → APPROVED: scope 在 Team 上限内. `PERMISSION_MATRIX` §6.
    ScopeWithinTeamCeiling,
    /// §7 PENDING → REJECTED: 提供决定/可选原因.
    DecisionRecorded,
    /// §7 PENDING → EXPIRED: expires_at 到达.
    ExpiresAtReached,
    /// §7 PENDING → CANCELLED: 尚未决定.
    NotYetDecided,
}

impl ApprovalPrecondition {
    /// The stable condition key for the catalogue's `condition_keys` detail.
    pub const fn condition_key(self) -> &'static str {
        match self {
            ApprovalPrecondition::RequestFieldsComplete => "request_fields_complete",
            ApprovalPrecondition::RequesterCannotSelfAuthorize => "requester_cannot_self_authorize",
            ApprovalPrecondition::NotExpired => "not_expired",
            ApprovalPrecondition::DeciderIsNotRequester => "decider_is_not_requester",
            ApprovalPrecondition::ScopeWithinTeamCeiling => "scope_within_team_ceiling",
            ApprovalPrecondition::DecisionRecorded => "decision_recorded",
            ApprovalPrecondition::ExpiresAtReached => "expires_at_reached",
            ApprovalPrecondition::NotYetDecided => "not_yet_decided",
        }
    }
}

/// An `APPROVAL_*` event §7 requires on a successful edge. Described, never
/// emitted. All five spellings are transcribed verbatim from §7, and the variant
/// names are the §7 spellings in Rust casing so the `SCREAMING_SNAKE_CASE` serde
/// rename reproduces [`Self::as_str`] exactly.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ApprovalEvent {
    ApprovalRequested,
    ApprovalApproved,
    ApprovalRejected,
    ApprovalExpired,
    ApprovalCancelled,
}

impl ApprovalEvent {
    /// The frozen event type string.
    pub const fn as_str(self) -> &'static str {
        match self {
            ApprovalEvent::ApprovalRequested => "APPROVAL_REQUESTED",
            ApprovalEvent::ApprovalApproved => "APPROVAL_APPROVED",
            ApprovalEvent::ApprovalRejected => "APPROVAL_REJECTED",
            ApprovalEvent::ApprovalExpired => "APPROVAL_EXPIRED",
            ApprovalEvent::ApprovalCancelled => "APPROVAL_CANCELLED",
        }
    }
}

impl fmt::Display for ApprovalEvent {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// A legal Approval edge, fully described but not applied.
///
/// Fields are private for the reason given on
/// [`crate::task_state_machine::TaskTransition`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ApprovalTransition {
    from: ApprovalStatus,
    to: ApprovalStatus,
    command: ApprovalCommand,
    preconditions: &'static [ApprovalPrecondition],
    events: &'static [ApprovalEvent],
}

impl ApprovalTransition {
    /// The state the request was in; not changed by the decision.
    pub const fn from(&self) -> ApprovalStatus {
        self.from
    }
    /// The state the request moves to, as decided by the §7 graph.
    pub const fn to(&self) -> ApprovalStatus {
        self.to
    }
    /// The command that produced this edge.
    pub const fn command(&self) -> ApprovalCommand {
        self.command
    }
    /// §7 preconditions the application layer must verify in-transaction.
    pub const fn preconditions(&self) -> &'static [ApprovalPrecondition] {
        self.preconditions
    }
    /// §7 events to write in the same transaction.
    pub const fn events(&self) -> &'static [ApprovalEvent] {
        self.events
    }
}

/// A rejected Approval transition. Carries no new state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ApprovalRejection {
    current: ApprovalStatus,
    command: ApprovalCommand,
    code: ApprovalErrorCode,
}

impl ApprovalRejection {
    /// The state the request was in; unchanged by this rejection.
    pub const fn current(&self) -> ApprovalStatus {
        self.current
    }
    /// The command that was refused.
    pub const fn command(&self) -> ApprovalCommand {
        self.command
    }
    /// The frozen stable code.
    pub const fn code(&self) -> ApprovalErrorCode {
        self.code
    }
}

impl fmt::Display for ApprovalRejection {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{} rejected `{:?}` in state {}",
            self.code, self.command, self.current
        )
    }
}

impl std::error::Error for ApprovalRejection {}

/// One row of the §7 transition table.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ApprovalEdge {
    /// Source state.
    pub from: ApprovalStatus,
    /// Command that drives it.
    pub command: ApprovalCommand,
    /// Destination state.
    pub to: ApprovalStatus,
    /// §7 preconditions.
    pub preconditions: &'static [ApprovalPrecondition],
    /// §7 events.
    pub events: &'static [ApprovalEvent],
    /// The §7 主要拒绝码, or the derived `DOMAIN_PRECONDITION_FAILED` where §7
    /// prints `—`.
    pub rejection: ApprovalErrorCode,
}

/// The complete §7 transition table, in table order: four edges, no fan-out.
pub const APPROVAL_EDGES: &[ApprovalEdge] = &[
    // 创建 → PENDING is a creation edge; see `CREATE_PENDING`.
    ApprovalEdge {
        from: ApprovalStatus::Pending,
        command: ApprovalCommand::Approve,
        to: ApprovalStatus::Approved,
        preconditions: &[
            ApprovalPrecondition::NotExpired,
            ApprovalPrecondition::DeciderIsNotRequester,
            ApprovalPrecondition::ScopeWithinTeamCeiling,
        ],
        events: &[ApprovalEvent::ApprovalApproved],
        rejection: ApprovalErrorCode::HumanRequired,
    },
    ApprovalEdge {
        from: ApprovalStatus::Pending,
        command: ApprovalCommand::Reject,
        to: ApprovalStatus::Rejected,
        preconditions: &[ApprovalPrecondition::DecisionRecorded],
        events: &[ApprovalEvent::ApprovalRejected],
        rejection: ApprovalErrorCode::ConcurrencyVersionConflict,
    },
    ApprovalEdge {
        from: ApprovalStatus::Pending,
        command: ApprovalCommand::Expire,
        to: ApprovalStatus::Expired,
        preconditions: &[ApprovalPrecondition::ExpiresAtReached],
        events: &[ApprovalEvent::ApprovalExpired],
        rejection: ApprovalErrorCode::PreconditionFailed,
    },
    ApprovalEdge {
        from: ApprovalStatus::Pending,
        command: ApprovalCommand::Cancel,
        to: ApprovalStatus::Cancelled,
        preconditions: &[ApprovalPrecondition::NotYetDecided],
        events: &[ApprovalEvent::ApprovalCancelled],
        rejection: ApprovalErrorCode::InvalidTransition,
    },
];

/// The single legal creation target: `创建 → PENDING`.
pub const CREATE_PENDING_APPROVAL: ApprovalCreation = ApprovalCreation {
    initial: ApprovalStatus::Pending,
    preconditions: &[
        ApprovalPrecondition::RequestFieldsComplete,
        ApprovalPrecondition::RequesterCannotSelfAuthorize,
    ],
    events: &[ApprovalEvent::ApprovalRequested],
    rejection: ApprovalErrorCode::ScopeInvalid,
};

/// The only state §7 allows an Approval request to be created into.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ApprovalCreation {
    initial: ApprovalStatus,
    preconditions: &'static [ApprovalPrecondition],
    events: &'static [ApprovalEvent],
    rejection: ApprovalErrorCode,
}

impl ApprovalCreation {
    /// The state the new request starts in.
    pub const fn initial(&self) -> ApprovalStatus {
        self.initial
    }
    /// Preconditions the application layer must verify before persisting.
    pub const fn preconditions(&self) -> &'static [ApprovalPrecondition] {
        self.preconditions
    }
    /// Events to write for the creation itself.
    pub const fn events(&self) -> &'static [ApprovalEvent] {
        self.events
    }
    /// The frozen rejection code when the creation preconditions are unmet.
    pub const fn rejection(&self) -> ApprovalErrorCode {
        self.rejection
    }
}

/// The §7 states a creation may target. §7 has exactly one.
pub const APPROVAL_CREATE_TARGETS: &[ApprovalStatus] = &[ApprovalStatus::Pending];

/// The §7 creation row, when `status` is a legal target, else `None`.
pub fn creation_target(status: ApprovalStatus) -> Option<&'static ApprovalCreation> {
    if APPROVAL_CREATE_TARGETS.contains(&status) {
        Some(&CREATE_PENDING_APPROVAL)
    } else {
        None
    }
}

/// Decide whether `command` is legal in `current`.
///
/// Pure: no I/O, no mutation, no clock. A missing edge yields
/// [`ApprovalErrorCode::InvalidTransition`]; a legal edge carries its own §7 code,
/// which for `PENDING → CANCELLED` is that same wire string — see
/// [`ApprovalErrorCode`].
pub fn transition(
    current: ApprovalStatus,
    command: ApprovalCommand,
) -> Result<ApprovalTransition, ApprovalRejection> {
    match APPROVAL_EDGES
        .iter()
        .find(|edge| edge.from == current && edge.command == command)
    {
        Some(edge) => Ok(ApprovalTransition {
            from: edge.from,
            to: edge.to,
            command: edge.command,
            preconditions: edge.preconditions,
            events: edge.events,
        }),
        None => Err(ApprovalRejection {
            current,
            command,
            code: ApprovalErrorCode::InvalidTransition,
        }),
    }
}

/// The rejection code for a legal-but-unmet edge, or `None` when the command is
/// not a legal edge at all.
pub fn rejection_code(
    current: ApprovalStatus,
    command: ApprovalCommand,
) -> Option<ApprovalErrorCode> {
    APPROVAL_EDGES
        .iter()
        .find(|edge| edge.from == current && edge.command == command)
        .map(|edge| edge.rejection)
}
