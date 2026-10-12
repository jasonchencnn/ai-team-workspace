//! DeliveryAttempt lifecycle state machine.
//!
//! Frozen source: `docs/specs/STATE_MACHINES.md` §5, the sentence "每次 attempt
//! 单独记录 `PENDING → IN_FLIGHT → SUCCEEDED/FAILED/ABANDONED`"; and
//! `docs/specs/ERROR_CATALOG.md`.
//!
//! §5: "Message 是持久内容；DeliveryAttempt 是可重试的投递尝试."
//!
//! # This machine has no events and no preconditions, on purpose
//!
//! §5 gives DeliveryAttempt a state chain and nothing else — no Event column, no
//! 拒绝码 column, no Actor column, no 前置条件 column. Every observation the
//! Message table makes about an attempt is recorded on the **Message** event:
//! "attempt SUCCEEDED" is part of `MESSAGE_DELIVERED`, "当前 attempt FAILED" is
//! part of `MESSAGE_RETRY_SCHEDULED` and `MESSAGE_FAILED`, "终止待处理 attempt"
//! is part of `MESSAGE_EXPIRED`.
//!
//! So an attempt edge carries an empty `events` slice and an empty `preconditions`
//! slice. Inventing `ATTEMPT_STARTED`-style names, or copying Message preconditions
//! onto attempt edges, would be inventing protocol — the very thing this node
//! exists to avoid. `every_edge_declares_no_events_and_no_preconditions` asserts
//! the absence, so a future reader who "helpfully" fills them in sees it fail.
//!
//! # Rejection codes are all `DOMAIN_INVALID_TRANSITION`
//!
//! §5 attaches no code to any attempt edge, so the only code an attempt
//! transition can produce is "there is no such edge". That is the one case where
//! no derivation is needed: the frozen catalogue's
//! [`crate::message_state_machine::MessageErrorCode::InvalidTransition`] wire
//! string applies unchanged.
//!
//! # A retry is a new attempt, never a rewind
//!
//! §5's retry edges say "新 attempt，保留旧 attempts" and "当前 attempt FAILED；
//! 计算退避". So `FAILED` has **no** out-edge: retrying means creating another
//! `PENDING` attempt, not moving the old one. That is what makes the attempt
//! history immutable, which §5 requires ("历史 attempt 和 Event 不可改写").

use std::fmt;

use serde::{Deserialize, Serialize};

use crate::message_state_machine::MessageErrorCode;

/// One delivery attempt's position in the §5 chain.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum AttemptStatus {
    /// Created and waiting. A retry is a fresh `Pending`, never a rewind.
    Pending,
    /// Handed to the Adapter; the attempt is in flight.
    InFlight,
    /// The Adapter accepted the payload.
    Succeeded,
    /// The attempt failed. One delivery cycle ends here; a retry creates a new
    /// attempt rather than reopening this one.
    Failed,
    /// The attempt was given up on — §5's "终止待处理 attempt" on expiry.
    Abandoned,
}

impl AttemptStatus {
    /// Every state, in §5's order.
    pub const ALL: [AttemptStatus; 5] = [
        AttemptStatus::Pending,
        AttemptStatus::InFlight,
        AttemptStatus::Succeeded,
        AttemptStatus::Failed,
        AttemptStatus::Abandoned,
    ];

    /// The wire name.
    pub const fn as_str(self) -> &'static str {
        match self {
            AttemptStatus::Pending => "PENDING",
            AttemptStatus::InFlight => "IN_FLIGHT",
            AttemptStatus::Succeeded => "SUCCEEDED",
            AttemptStatus::Failed => "FAILED",
            AttemptStatus::Abandoned => "ABANDONED",
        }
    }

    /// `true` for the three states §5's chain ends on.
    ///
    /// Derived from the chain, which has no return edges. `FAILED` is terminal
    /// here even though the *Message* can be requeued — the attempt is not
    /// reopened, a new one is created.
    pub const fn is_terminal(self) -> bool {
        matches!(
            self,
            AttemptStatus::Succeeded | AttemptStatus::Failed | AttemptStatus::Abandoned
        )
    }
}

impl fmt::Display for AttemptStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// A command that may drive a delivery attempt.
///
/// No `as_str()` and no serde derives: §5 names no command vocabulary for
/// attempts, so any spelling would be invented protocol.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum AttemptCommand {
    /// Hand the attempt to the Adapter.
    Start,
    /// The Adapter accepted it.
    Succeed,
    /// The attempt failed.
    Fail,
    /// The attempt was abandoned before finishing.
    Abandon,
}

impl AttemptCommand {
    /// Every command, in §5 chain order.
    pub const ALL: [AttemptCommand; 4] = [
        AttemptCommand::Start,
        AttemptCommand::Succeed,
        AttemptCommand::Fail,
        AttemptCommand::Abandon,
    ];
}

impl fmt::Display for AttemptCommand {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}

/// One row of the §5 attempt chain. Preconditions and events are intentionally
/// empty; see the module note.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct AttemptEdge {
    /// Source state.
    pub from: AttemptStatus,
    /// Command that drives it.
    pub command: AttemptCommand,
    /// Destination state.
    pub to: AttemptStatus,
    /// Always empty. §5 gives the attempt chain no 前置条件 column.
    pub preconditions: &'static [()],
    /// Always empty. §5 records every attempt observation on the Message event.
    pub events: &'static [()],
    /// §5 attaches no code to an attempt edge, so this is always
    /// `DOMAIN_INVALID_TRANSITION`, meaning "this is not a legal attempt edge".
    pub rejection: MessageErrorCode,
}

/// The complete §5 attempt chain: `PENDING → IN_FLIGHT → SUCCEEDED/FAILED/ABANDONED`.
pub const ATTEMPT_EDGES: &[AttemptEdge] = &[
    AttemptEdge {
        from: AttemptStatus::Pending,
        command: AttemptCommand::Start,
        to: AttemptStatus::InFlight,
        preconditions: &[],
        events: &[],
        rejection: MessageErrorCode::InvalidTransition,
    },
    AttemptEdge {
        from: AttemptStatus::InFlight,
        command: AttemptCommand::Succeed,
        to: AttemptStatus::Succeeded,
        preconditions: &[],
        events: &[],
        rejection: MessageErrorCode::InvalidTransition,
    },
    AttemptEdge {
        from: AttemptStatus::InFlight,
        command: AttemptCommand::Fail,
        to: AttemptStatus::Failed,
        preconditions: &[],
        events: &[],
        rejection: MessageErrorCode::InvalidTransition,
    },
    AttemptEdge {
        from: AttemptStatus::InFlight,
        command: AttemptCommand::Abandon,
        to: AttemptStatus::Abandoned,
        preconditions: &[],
        events: &[],
        rejection: MessageErrorCode::InvalidTransition,
    },
];

/// The only state a new attempt starts in: `创建 → PENDING`.
///
/// §5 states the chain in one sentence and gives no creation row, so this is a
/// **derived** target: an attempt has to be recorded before it can be started, and
/// the chain's only zero-in-degree state is `PENDING`.
pub const CREATE_PENDING: AttemptCreation = AttemptCreation {
    initial: AttemptStatus::Pending,
    rejection: MessageErrorCode::InvalidTransition,
};

/// The only state §5 allows an attempt to be created into.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct AttemptCreation {
    initial: AttemptStatus,
    rejection: MessageErrorCode,
}

impl AttemptCreation {
    /// The state the new attempt starts in.
    pub const fn initial(&self) -> AttemptStatus {
        self.initial
    }
    /// The frozen rejection code when creation is not allowed. §5 names none, so
    /// this is always `DOMAIN_INVALID_TRANSITION`.
    pub const fn rejection(&self) -> MessageErrorCode {
        self.rejection
    }
}

/// The §5 states a creation may target. §5's chain has exactly one.
pub const ATTEMPT_CREATE_TARGETS: &[AttemptStatus] = &[AttemptStatus::Pending];

/// The creation row, when `status` is a legal target, else `None`.
pub fn creation_target(status: AttemptStatus) -> Option<&'static AttemptCreation> {
    if ATTEMPT_CREATE_TARGETS.contains(&status) {
        Some(&CREATE_PENDING)
    } else {
        None
    }
}

/// A decided attempt transition. No preconditions, no events — see module note.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct AttemptTransition {
    from: AttemptStatus,
    to: AttemptStatus,
    command: AttemptCommand,
    // Carried from the edge rather than hardcoded, so the accessors below report
    // what the table says. The values are empty on every §5 attempt row; keeping
    // them as real fields means a future row that did declare a precondition would
    // be visible instead of silently dropped.
    preconditions: &'static [()],
    events: &'static [()],
}

impl AttemptTransition {
    /// The state the attempt was in; not changed by the decision.
    pub const fn from(&self) -> AttemptStatus {
        self.from
    }
    /// The state the attempt moves to, as decided by the §5 chain.
    pub const fn to(&self) -> AttemptStatus {
        self.to
    }
    /// The command that produced this edge.
    pub const fn command(&self) -> AttemptCommand {
        self.command
    }
    /// §5's attempt chain has no 前置条件 column, so this is empty on every row.
    pub const fn preconditions(&self) -> &'static [()] {
        self.preconditions
    }
    /// §5 records every attempt observation on the Message event, so this is empty
    /// on every row.
    pub const fn events(&self) -> &'static [()] {
        self.events
    }
}

/// A rejected attempt transition. Carries no new state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct AttemptRejection {
    current: AttemptStatus,
    command: AttemptCommand,
    code: MessageErrorCode,
}

impl AttemptRejection {
    /// The state the attempt was in; unchanged by this rejection.
    pub const fn current(&self) -> AttemptStatus {
        self.current
    }
    /// The command that was refused.
    pub const fn command(&self) -> AttemptCommand {
        self.command
    }
    /// The frozen stable code — always `DOMAIN_INVALID_TRANSITION`.
    pub const fn code(&self) -> MessageErrorCode {
        self.code
    }
}

impl fmt::Display for AttemptRejection {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{} rejected `{:?}` in state {}",
            self.code, self.command, self.current
        )
    }
}

impl std::error::Error for AttemptRejection {}

/// Decide whether `command` is legal in `current`.
///
/// Pure: no I/O, no mutation, no clock. `ATTEMPT_EDGES` has no duplicate
/// `(from, command)` pair, so the answer is never ambiguous.
pub fn transition(
    current: AttemptStatus,
    command: AttemptCommand,
) -> Result<AttemptTransition, AttemptRejection> {
    match ATTEMPT_EDGES
        .iter()
        .find(|edge| edge.from == current && edge.command == command)
    {
        Some(edge) => Ok(AttemptTransition {
            from: edge.from,
            to: edge.to,
            command: edge.command,
            preconditions: edge.preconditions,
            events: edge.events,
        }),
        None => Err(AttemptRejection {
            current,
            command,
            code: MessageErrorCode::InvalidTransition,
        }),
    }
}
