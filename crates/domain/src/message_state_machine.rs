//! Message lifecycle state machine.
//!
//! Frozen source: `docs/specs/STATE_MACHINES.md` §5 and its transition table;
//! `docs/specs/ERROR_CATALOG.md`.
//!
//! Same layering as the Task / Run / Binding machines: this module decides
//! whether an edge exists in the §5 graph and which frozen rejection code it
//! carries. TTL arithmetic, Adapter acknowledgement, permission and subscription
//! checks, and retry limits all need to read the database, so they are described
//! here and evaluated by the application layer in the same transaction.
//!
//! # `RETRYING` is deliberately not a state
//!
//! §5: "`RETRYING` 是"已安排下一次 attempt"的派生瞬态，不持久化为 Message 终态".
//! The enum cannot spell it, and `there_is_no_retrying_state` asserts the
//! negative space. The retry edge itself exists — `ROUTED → QUEUED` emits
//! `MESSAGE_RETRY_SCHEDULED` — so a scheduled retry is a **return to QUEUED**,
//! not a distinct resting place. `RETRYING` is a query over "there is a pending
//! attempt with a future `next_attempt_at`", not a column value.
//!
//! # `MESSAGE_REASSIGNED` / `MESSAGE_REQUEUED` are two edges — a *derived* reading
//!
//! §5 prints this Event cell with `/` where §5's own `DRAFT → QUEUED` row uses
//! `、`. Reading across the whole frozen file: `、` occurs on seven rows
//! (six in §2, one in §5) and `/` on exactly two (§4's `ROLE_BOUND`/`ROLE_REBOUND`
//! and this one).
//!
//! **The specification never states this convention.** It is an inference from
//! n = 2, and one of those two rows is the already-shipped §4 precedent the reading
//! is generalised *from* — so it is not independent confirmation. It is recorded as
//! a derived judgement, not as a mechanical fact.
//!
//! What makes it safe to act on anyway: the split **loses nothing frozen**. Both
//! resulting edges keep the printed row's preconditions and rejection code
//! verbatim, and differ only in which event each emits. If a Human later reads
//! `/` as "both", collapsing the two edges back into one edge with two events
//! touches nothing else. `reassign_and_requeue_are_distinguishable` pins the
//! current reading so any change to it has to be deliberate.
//!
//! # Two printed rows fan out on the source state
//!
//! `QUEUED/ROUTED → EXPIRED` and `PROCESSING/ACKNOWLEDGED → REPLIED` are one
//! printed line each and two edges each. Same `/`, different cell, different
//! meaning: here it names the sources, not the event.

use std::fmt;

use serde::{Deserialize, Serialize};

/// A Message's position in the §5 lifecycle.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum MessageStatus {
    /// Saved but never sent. §5: "保存草稿，不投递".
    Draft,
    /// Content frozen, waiting for a Router. §5: "冻结内容，记录 sender Binding snapshot".
    Queued,
    /// An attempt exists against a resolved target Binding.
    Routed,
    /// The Adapter accepted the attempt.
    Delivered,
    /// The target confirmed `message_id`.
    Acknowledged,
    /// The Harness explicitly claimed processing. Never implied.
    Processing,
    /// A reply was posted into the same thread. Absorbing.
    Replied,
    /// A delivery cycle ended unsuccessfully. One explicit retry edge back.
    Failed,
    /// TTL elapsed. Absorbing: §5 never reopens the same Message.
    Expired,
    /// No usable Binding, Role disabled, or reassignment needs authorisation.
    NeedsHuman,
}

impl MessageStatus {
    /// Every state, in §5's declaration order.
    pub const ALL: [MessageStatus; 10] = [
        MessageStatus::Draft,
        MessageStatus::Queued,
        MessageStatus::Routed,
        MessageStatus::Delivered,
        MessageStatus::Acknowledged,
        MessageStatus::Processing,
        MessageStatus::Replied,
        MessageStatus::Failed,
        MessageStatus::Expired,
        MessageStatus::NeedsHuman,
    ];

    /// The wire name used in `MESSAGE_*` events and the UI.
    pub const fn as_str(self) -> &'static str {
        match self {
            MessageStatus::Draft => "DRAFT",
            MessageStatus::Queued => "QUEUED",
            MessageStatus::Routed => "ROUTED",
            MessageStatus::Delivered => "DELIVERED",
            MessageStatus::Acknowledged => "ACKNOWLEDGED",
            MessageStatus::Processing => "PROCESSING",
            MessageStatus::Replied => "REPLIED",
            MessageStatus::Failed => "FAILED",
            MessageStatus::Expired => "EXPIRED",
            MessageStatus::NeedsHuman => "NEEDS_HUMAN",
        }
    }

    /// `true` for the states §5 gives no out-edge at all.
    ///
    /// §5 does not use the word 终态 for these, so this is derived from the
    /// table. `REPLIED` and `EXPIRED` absorb. `FAILED` does **not**: §5 calls it
    /// "某投递周期终态" but also gives it the explicit `FAILED → QUEUED` retry
    /// edge, so it is terminal for one delivery cycle and re-enterable on demand.
    /// Collapsing that distinction would either strand failed Messages or let
    /// expiry reopen, and §5 forbids the latter explicitly.
    pub const fn is_terminal(self) -> bool {
        matches!(self, MessageStatus::Replied | MessageStatus::Expired)
    }
}

impl fmt::Display for MessageStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// A command that may drive a Message.
///
/// Deliberately no `as_str()` and no serde derives: §5's Actor/命令 column names
/// roles and commands (`message.send`, `message.retry`) as *inputs to the
/// permission matrix*, not as a wire vocabulary this machine owns. Any further
/// `message.*` string here would be invented protocol — same stance as
/// [`crate::run_state_machine::RunCommand`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum MessageCommand {
    /// §5: `message.send`；sender. The only edge out of `DRAFT`.
    Send,
    /// Router resolved a target and created an attempt.
    Route,
    /// Router could not resolve; a Human has to intervene.
    NeedsHuman,
    /// Scheduler; legal from `QUEUED` and from `ROUTED`.
    Expire,
    /// Adapter accepted the attempt.
    Deliver,
    /// Retry scheduler rescheduled a retryable failure. Produces §5's derived
    /// `RETRYING` transient as a *return to* `QUEUED`.
    ScheduleRetry,
    /// Router gave up: non-retryable error, or the retry ceiling was reached.
    Fail,
    /// Target Adapter / Harness confirmed `message_id`.
    Acknowledge,
    /// Target explicitly declared processing.
    Process,
    /// Target Role posted a reply. Legal from `PROCESSING` and `ACKNOWLEDGED`.
    Reply,
    /// §5: `message.retry`；Human/授权 Role, out of `FAILED` only.
    RetryFailed,
    /// Human moved the Message to a *different* target. Emits
    /// `MESSAGE_REASSIGNED`.
    Reassign,
    /// Human re-queued the *same* target after the Binding was repaired. Emits
    /// `MESSAGE_REQUEUED`.
    RequeueNeedsHuman,
}

impl MessageCommand {
    /// Every command, in §5 table order.
    pub const ALL: [MessageCommand; 13] = [
        MessageCommand::Send,
        MessageCommand::Route,
        MessageCommand::NeedsHuman,
        MessageCommand::Expire,
        MessageCommand::Deliver,
        MessageCommand::ScheduleRetry,
        MessageCommand::Fail,
        MessageCommand::Acknowledge,
        MessageCommand::Process,
        MessageCommand::Reply,
        MessageCommand::RetryFailed,
        MessageCommand::Reassign,
        MessageCommand::RequeueNeedsHuman,
    ];
}

impl fmt::Display for MessageCommand {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}

/// Stable rejection codes, frozen in `docs/specs/ERROR_CATALOG.md`.
///
/// # Why `PreconditionFailed` exists
///
/// §5's table has thirteen printed rows. Nine print a concrete 主要拒绝码
/// (`AUTH_PERMISSION_DENIED`, `VALIDATION_INVALID_FIELD`, `BINDING_NOT_FOUND`,
/// `BINDING_STALE_VERSION`, `MESSAGE_THREAD_MISMATCH`, `MESSAGE_TTL_EXPIRED`,
/// `AUTH_HUMAN_REQUIRED` — seven distinct). The other four print `—`:
///
/// | §5 row | cell |
/// |---|---|
/// | `QUEUED → NEEDS_HUMAN` | `—` |
/// | `QUEUED/ROUTED → EXPIRED` | `—` |
/// | `ROUTED → QUEUED` (retry) | `—` |
/// | `ROUTED → FAILED` | `—` |
///
/// (four printed rows, five edges once the `QUEUED/ROUTED → EXPIRED` fan-out is
/// expanded)
/// Those edges are reached by *observing* a missing Binding, a lapsed TTL, a
/// retryable failure or a retry ceiling — not by a caller failing a check — so
/// §5 names no code. `DOMAIN_PRECONDITION_FAILED`, the catalogue's generic
/// precondition failure, is what the specification points at. Attaching it there
/// is a **derived** choice, recorded rather than silently assumed.
///
/// # No §5 code is retryable
///
/// Read out of the catalogue's own table, not assumed: all nine codes this
/// machine can raise are marked 否. That is a fact about the frozen catalogue,
/// not an oversight, and `error_codes_match_the_frozen_catalogue` asserts it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum MessageErrorCode {
    /// Not a legal edge in the §5 graph at all.
    #[serde(rename = "DOMAIN_INVALID_TRANSITION")]
    InvalidTransition,
    /// The code §5 attaches to a legal edge.
    #[serde(rename = "AUTH_PERMISSION_DENIED")]
    PermissionDenied,
    /// §5 `NEEDS_HUMAN → QUEUED`: the row is Human-triggered, so a refused
    /// reassign or requeue is answered with "open the approval or takeover
    /// entry point", not with "you lack permission". Reading it as a permission
    /// failure sends the caller to the wrong remediation.
    #[serde(rename = "AUTH_HUMAN_REQUIRED")]
    HumanRequired,
    #[serde(rename = "VALIDATION_INVALID_FIELD")]
    ValidationInvalidField,
    #[serde(rename = "BINDING_NOT_FOUND")]
    BindingNotFound,
    #[serde(rename = "BINDING_STALE_VERSION")]
    BindingStaleVersion,
    #[serde(rename = "MESSAGE_TTL_EXPIRED")]
    TtlExpired,
    #[serde(rename = "MESSAGE_THREAD_MISMATCH")]
    ThreadMismatch,
    /// Derived: the §5 rows whose 拒绝码 cell is `—`.
    #[serde(rename = "DOMAIN_PRECONDITION_FAILED")]
    PreconditionFailed,
}

impl MessageErrorCode {
    /// The frozen wire name.
    pub const fn as_str(self) -> &'static str {
        match self {
            MessageErrorCode::InvalidTransition => "DOMAIN_INVALID_TRANSITION",
            MessageErrorCode::PermissionDenied => "AUTH_PERMISSION_DENIED",
            MessageErrorCode::HumanRequired => "AUTH_HUMAN_REQUIRED",
            MessageErrorCode::ValidationInvalidField => "VALIDATION_INVALID_FIELD",
            MessageErrorCode::BindingNotFound => "BINDING_NOT_FOUND",
            MessageErrorCode::BindingStaleVersion => "BINDING_STALE_VERSION",
            MessageErrorCode::TtlExpired => "MESSAGE_TTL_EXPIRED",
            MessageErrorCode::ThreadMismatch => "MESSAGE_THREAD_MISMATCH",
            MessageErrorCode::PreconditionFailed => "DOMAIN_PRECONDITION_FAILED",
        }
    }

    /// `true` for codes `ERROR_CATALOG.md` marks 是.
    ///
    /// `false` for all eight. The catalogue marks every code §5 can raise as 否,
    /// so this machine has no retryable code of its own. §5's *edges* are still
    /// retryable — `ROUTED → QUEUED` exists — but the client-facing code for a
    /// refused retry is not.
    pub const fn is_retryable(self) -> bool {
        false
    }
}

impl fmt::Display for MessageErrorCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// A precondition §5 attaches to an edge. Described, never checked here.
///
/// Variant names and [`MessagePrecondition::condition_key`] spellings are **derived**: §5 states each
/// precondition in prose and freezes no identifiers for them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum MessagePrecondition {
    /// §5 创建 → DRAFT: sender 有权向目标 Role 发送.
    SenderMaySendToTargetRole,
    /// §5 DRAFT → QUEUED: to_role、type、body/引用、TTL、幂等键有效.
    RecipientTypeBodyTtlIdempotencyValid,
    /// §5 QUEUED → ROUTED: TTL 有效.
    TtlValid,
    /// §5 QUEUED → ROUTED: 目标 Role ACTIVE；解析当前 ACTIVE Binding.
    TargetRoleActiveAndBindingResolved,
    /// §5 QUEUED → ROUTED: 权限/订阅允许.
    PermissionOrSubscriptionAllows,
    /// §5 QUEUED → NEEDS_HUMAN: 无有效 Binding、Role 停用、改派需授权.
    NoValidBindingOrRoleDisabledOrReassignNeedsAuthorization,
    /// §5 → EXPIRED: `now >= expires_at`.
    ExpiresAtReached,
    /// §5 ROUTED → DELIVERED: attempt、binding、sequence 匹配，Adapter 已接收.
    AttemptBindingSequenceMatchAndAdapterAccepted,
    /// §5 ROUTED → QUEUED: retryable 失败、未达上限、TTL 有效.
    RetryableFailureWithinLimitAndTtlValid,
    /// §5 ROUTED → FAILED: 非重试错误或达到上限.
    NonRetryableErrorOrLimitReached,
    /// §5 DELIVERED → ACKNOWLEDGED: 目标确认 message_id，Binding snapshot 匹配.
    MessageIdConfirmedAndBindingSnapshotMatches,
    /// §5 ACKNOWLEDGED → PROCESSING: 显式声明处理；不自动 claim Task.
    ExplicitlyDeclaredProcessing,
    /// §5 → REPLIED: 新 Reply Message 同 thread，`reply_to` 指向原消息.
    ReplyInSameThreadPointsToOriginal,
    /// §5 FAILED → QUEUED: 失败原因可重试；幂等键新且内容不改.
    FailureRetryableAndFreshIdempotencyKeyWithUnchangedContent,
    /// §5 NEEDS_HUMAN → QUEUED: 新目标/Binding 有效，权限通过，TTL 有效.
    NewTargetOrBindingValidAndPermissionPassesAndTtlValid,
}

impl MessagePrecondition {
    /// The stable condition key for the catalogue's `condition_keys` detail.
    pub const fn condition_key(self) -> &'static str {
        match self {
            MessagePrecondition::SenderMaySendToTargetRole => "sender_may_send_to_target_role",
            MessagePrecondition::RecipientTypeBodyTtlIdempotencyValid => {
                "recipient_type_body_ttl_idempotency_valid"
            }
            MessagePrecondition::TtlValid => "ttl_valid",
            MessagePrecondition::TargetRoleActiveAndBindingResolved => {
                "target_role_active_and_binding_resolved"
            }
            MessagePrecondition::PermissionOrSubscriptionAllows => {
                "permission_or_subscription_allows"
            }
            MessagePrecondition::NoValidBindingOrRoleDisabledOrReassignNeedsAuthorization => {
                "no_valid_binding_or_role_disabled_or_reassign_needs_authorization"
            }
            MessagePrecondition::ExpiresAtReached => "expires_at_reached",
            MessagePrecondition::AttemptBindingSequenceMatchAndAdapterAccepted => {
                "attempt_binding_sequence_match_and_adapter_accepted"
            }
            MessagePrecondition::RetryableFailureWithinLimitAndTtlValid => {
                "retryable_failure_within_limit_and_ttl_valid"
            }
            MessagePrecondition::NonRetryableErrorOrLimitReached => {
                "non_retryable_error_or_limit_reached"
            }
            MessagePrecondition::MessageIdConfirmedAndBindingSnapshotMatches => {
                "message_id_confirmed_and_binding_snapshot_matches"
            }
            MessagePrecondition::ExplicitlyDeclaredProcessing => "explicitly_declared_processing",
            MessagePrecondition::ReplyInSameThreadPointsToOriginal => {
                "reply_in_same_thread_points_to_original"
            }
            MessagePrecondition::FailureRetryableAndFreshIdempotencyKeyWithUnchangedContent => {
                "failure_retryable_and_fresh_idempotency_key_with_unchanged_content"
            }
            MessagePrecondition::NewTargetOrBindingValidAndPermissionPassesAndTtlValid => {
                "new_target_or_binding_valid_and_permission_passes_and_ttl_valid"
            }
        }
    }
}

/// A `MESSAGE_*` event §5 requires on a successful edge. Described, never
/// emitted. All fourteen spellings are transcribed verbatim from §5, and the
/// variant names are the §5 spellings in Rust casing so the
/// `SCREAMING_SNAKE_CASE` serde rename reproduces [`Self::as_str`] exactly.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum MessageEvent {
    MessageDrafted,
    MessageSent,
    MessageQueued,
    MessageRouted,
    MessageNeedsHuman,
    MessageExpired,
    MessageDelivered,
    MessageRetryScheduled,
    MessageFailed,
    MessageAcknowledged,
    MessageProcessing,
    MessageReplied,
    MessageRequeued,
    MessageReassigned,
}

impl MessageEvent {
    /// The frozen event type string.
    pub const fn as_str(self) -> &'static str {
        match self {
            MessageEvent::MessageDrafted => "MESSAGE_DRAFTED",
            MessageEvent::MessageSent => "MESSAGE_SENT",
            MessageEvent::MessageQueued => "MESSAGE_QUEUED",
            MessageEvent::MessageRouted => "MESSAGE_ROUTED",
            MessageEvent::MessageNeedsHuman => "MESSAGE_NEEDS_HUMAN",
            MessageEvent::MessageExpired => "MESSAGE_EXPIRED",
            MessageEvent::MessageDelivered => "MESSAGE_DELIVERED",
            MessageEvent::MessageRetryScheduled => "MESSAGE_RETRY_SCHEDULED",
            MessageEvent::MessageFailed => "MESSAGE_FAILED",
            MessageEvent::MessageAcknowledged => "MESSAGE_ACKNOWLEDGED",
            MessageEvent::MessageProcessing => "MESSAGE_PROCESSING",
            MessageEvent::MessageReplied => "MESSAGE_REPLIED",
            MessageEvent::MessageRequeued => "MESSAGE_REQUEUED",
            MessageEvent::MessageReassigned => "MESSAGE_REASSIGNED",
        }
    }
}

impl fmt::Display for MessageEvent {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// A legal Message edge, fully described but not applied.
///
/// Fields are private for the reason given on
/// [`crate::task_state_machine::TaskTransition`]: with public fields a caller
/// could rewrite the decision and bypass the machine.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct MessageTransition {
    from: MessageStatus,
    to: MessageStatus,
    command: MessageCommand,
    preconditions: &'static [MessagePrecondition],
    events: &'static [MessageEvent],
}

impl MessageTransition {
    /// The state the Message was in; not changed by the decision.
    pub const fn from(&self) -> MessageStatus {
        self.from
    }
    /// The state the Message moves to, as decided by the §5 graph.
    pub const fn to(&self) -> MessageStatus {
        self.to
    }
    /// The command that produced this edge.
    pub const fn command(&self) -> MessageCommand {
        self.command
    }
    /// §5 preconditions the application layer must verify in-transaction.
    pub const fn preconditions(&self) -> &'static [MessagePrecondition] {
        self.preconditions
    }
    /// §5 events to write in the same transaction.
    pub const fn events(&self) -> &'static [MessageEvent] {
        self.events
    }
}

/// A rejected Message transition. Carries no new state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct MessageRejection {
    current: MessageStatus,
    command: MessageCommand,
    code: MessageErrorCode,
}

impl MessageRejection {
    /// The state the Message was in; unchanged by this rejection.
    pub const fn current(&self) -> MessageStatus {
        self.current
    }
    /// The command that was refused.
    pub const fn command(&self) -> MessageCommand {
        self.command
    }
    /// The frozen stable code.
    pub const fn code(&self) -> MessageErrorCode {
        self.code
    }
}

impl fmt::Display for MessageRejection {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{} rejected `{:?}` in state {}",
            self.code, self.command, self.current
        )
    }
}

impl std::error::Error for MessageRejection {}

/// One row of the §5 transition table.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct MessageEdge {
    /// Source state.
    pub from: MessageStatus,
    /// Command that drives it.
    pub command: MessageCommand,
    /// Destination state.
    pub to: MessageStatus,
    /// §5 preconditions.
    pub preconditions: &'static [MessagePrecondition],
    /// §5 events.
    pub events: &'static [MessageEvent],
    /// The §5 主要拒绝码, or the derived `DOMAIN_PRECONDITION_FAILED` where §5
    /// prints `—`.
    pub rejection: MessageErrorCode,
}

/// The complete §5 transition table, in table order.
///
/// Thirteen printed rows become fifteen edges: `QUEUED/ROUTED → EXPIRED` and
/// `PROCESSING/ACKNOWLEDGED → REPLIED` each fan out on the source state, and
/// `NEEDS_HUMAN → QUEUED` splits on its mutually exclusive event cell.
pub const MESSAGE_EDGES: &[MessageEdge] = &[
    // 创建 → DRAFT is a creation edge; see `CREATE_DRAFT`.
    MessageEdge {
        from: MessageStatus::Draft,
        command: MessageCommand::Send,
        to: MessageStatus::Queued,
        preconditions: &[MessagePrecondition::RecipientTypeBodyTtlIdempotencyValid],
        events: &[MessageEvent::MessageSent, MessageEvent::MessageQueued],
        rejection: MessageErrorCode::ValidationInvalidField,
    },
    MessageEdge {
        from: MessageStatus::Queued,
        command: MessageCommand::Route,
        to: MessageStatus::Routed,
        preconditions: &[
            MessagePrecondition::TtlValid,
            MessagePrecondition::TargetRoleActiveAndBindingResolved,
            MessagePrecondition::PermissionOrSubscriptionAllows,
        ],
        events: &[MessageEvent::MessageRouted],
        rejection: MessageErrorCode::BindingNotFound,
    },
    MessageEdge {
        from: MessageStatus::Queued,
        command: MessageCommand::NeedsHuman,
        to: MessageStatus::NeedsHuman,
        preconditions: &[
            MessagePrecondition::NoValidBindingOrRoleDisabledOrReassignNeedsAuthorization,
        ],
        events: &[MessageEvent::MessageNeedsHuman],
        rejection: MessageErrorCode::PreconditionFailed,
    },
    // §5 prints `QUEUED/ROUTED → EXPIRED` as one line; it is two edges.
    MessageEdge {
        from: MessageStatus::Queued,
        command: MessageCommand::Expire,
        to: MessageStatus::Expired,
        preconditions: &[MessagePrecondition::ExpiresAtReached],
        events: &[MessageEvent::MessageExpired],
        rejection: MessageErrorCode::PreconditionFailed,
    },
    MessageEdge {
        from: MessageStatus::Routed,
        command: MessageCommand::Expire,
        to: MessageStatus::Expired,
        preconditions: &[MessagePrecondition::ExpiresAtReached],
        events: &[MessageEvent::MessageExpired],
        rejection: MessageErrorCode::PreconditionFailed,
    },
    MessageEdge {
        from: MessageStatus::Routed,
        command: MessageCommand::Deliver,
        to: MessageStatus::Delivered,
        preconditions: &[MessagePrecondition::AttemptBindingSequenceMatchAndAdapterAccepted],
        events: &[MessageEvent::MessageDelivered],
        rejection: MessageErrorCode::BindingStaleVersion,
    },
    MessageEdge {
        from: MessageStatus::Routed,
        command: MessageCommand::ScheduleRetry,
        to: MessageStatus::Queued,
        preconditions: &[MessagePrecondition::RetryableFailureWithinLimitAndTtlValid],
        events: &[MessageEvent::MessageRetryScheduled],
        rejection: MessageErrorCode::PreconditionFailed,
    },
    MessageEdge {
        from: MessageStatus::Routed,
        command: MessageCommand::Fail,
        to: MessageStatus::Failed,
        preconditions: &[MessagePrecondition::NonRetryableErrorOrLimitReached],
        events: &[MessageEvent::MessageFailed],
        rejection: MessageErrorCode::PreconditionFailed,
    },
    MessageEdge {
        from: MessageStatus::Delivered,
        command: MessageCommand::Acknowledge,
        to: MessageStatus::Acknowledged,
        preconditions: &[MessagePrecondition::MessageIdConfirmedAndBindingSnapshotMatches],
        events: &[MessageEvent::MessageAcknowledged],
        rejection: MessageErrorCode::BindingStaleVersion,
    },
    MessageEdge {
        from: MessageStatus::Acknowledged,
        command: MessageCommand::Process,
        to: MessageStatus::Processing,
        preconditions: &[MessagePrecondition::ExplicitlyDeclaredProcessing],
        events: &[MessageEvent::MessageProcessing],
        rejection: MessageErrorCode::PermissionDenied,
    },
    // §5 prints `PROCESSING/ACKNOWLEDGED → REPLIED` as one line; two edges.
    MessageEdge {
        from: MessageStatus::Processing,
        command: MessageCommand::Reply,
        to: MessageStatus::Replied,
        preconditions: &[MessagePrecondition::ReplyInSameThreadPointsToOriginal],
        events: &[MessageEvent::MessageReplied],
        rejection: MessageErrorCode::ThreadMismatch,
    },
    MessageEdge {
        from: MessageStatus::Acknowledged,
        command: MessageCommand::Reply,
        to: MessageStatus::Replied,
        preconditions: &[MessagePrecondition::ReplyInSameThreadPointsToOriginal],
        events: &[MessageEvent::MessageReplied],
        rejection: MessageErrorCode::ThreadMismatch,
    },
    MessageEdge {
        from: MessageStatus::Failed,
        command: MessageCommand::RetryFailed,
        to: MessageStatus::Queued,
        preconditions: &[
            MessagePrecondition::TtlValid,
            MessagePrecondition::FailureRetryableAndFreshIdempotencyKeyWithUnchangedContent,
        ],
        events: &[MessageEvent::MessageRequeued],
        rejection: MessageErrorCode::TtlExpired,
    },
    // §5's Event cell here is `MESSAGE_REASSIGNED`/`MESSAGE_REQUEUED` — `/`,
    // not `、` — so the two are mutually exclusive and become two edges. The
    // Human moved the target versus the Human repaired the Binding and re-ran
    // the same target; conflating them would lose the route-override record.
    MessageEdge {
        from: MessageStatus::NeedsHuman,
        command: MessageCommand::Reassign,
        to: MessageStatus::Queued,
        preconditions: &[
            MessagePrecondition::NewTargetOrBindingValidAndPermissionPassesAndTtlValid,
        ],
        events: &[MessageEvent::MessageReassigned],
        rejection: MessageErrorCode::HumanRequired,
    },
    MessageEdge {
        from: MessageStatus::NeedsHuman,
        command: MessageCommand::RequeueNeedsHuman,
        to: MessageStatus::Queued,
        preconditions: &[
            MessagePrecondition::NewTargetOrBindingValidAndPermissionPassesAndTtlValid,
        ],
        events: &[MessageEvent::MessageRequeued],
        rejection: MessageErrorCode::HumanRequired,
    },
];

/// The single legal creation target: `创建 → DRAFT`.
pub const CREATE_DRAFT: MessageCreation = MessageCreation {
    initial: MessageStatus::Draft,
    preconditions: &[MessagePrecondition::SenderMaySendToTargetRole],
    events: &[MessageEvent::MessageDrafted],
    rejection: MessageErrorCode::PermissionDenied,
};

/// The only state §5 allows a Message to be created into.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct MessageCreation {
    initial: MessageStatus,
    preconditions: &'static [MessagePrecondition],
    events: &'static [MessageEvent],
    rejection: MessageErrorCode,
}

impl MessageCreation {
    /// The state the new Message starts in.
    pub const fn initial(&self) -> MessageStatus {
        self.initial
    }
    /// Preconditions the application layer must verify before persisting.
    pub const fn preconditions(&self) -> &'static [MessagePrecondition] {
        self.preconditions
    }
    /// Events to write for the creation itself.
    pub const fn events(&self) -> &'static [MessageEvent] {
        self.events
    }
    /// The frozen rejection code when the creation preconditions are unmet.
    pub const fn rejection(&self) -> MessageErrorCode {
        self.rejection
    }
}

/// The §5 states a creation may target. §5 has exactly one.
pub const CREATE_TARGETS: &[MessageStatus] = &[MessageStatus::Draft];

/// The §5 creation row, when `status` is a legal target, else `None`.
pub fn creation_target(status: MessageStatus) -> Option<&'static MessageCreation> {
    if CREATE_TARGETS.contains(&status) {
        Some(&CREATE_DRAFT)
    } else {
        None
    }
}

/// Decide whether `command` is legal in `current`.
///
/// Pure: no I/O, no mutation, no clock. A missing edge yields
/// [`MessageErrorCode::InvalidTransition`]; a legal edge carries its own §5 code.
///
/// Total on its domain: every `(from, command)` pair in [`MESSAGE_EDGES`] appears
/// exactly once. `Expire` and `Reply` each appear twice, from different states.
pub fn transition(
    current: MessageStatus,
    command: MessageCommand,
) -> Result<MessageTransition, MessageRejection> {
    match MESSAGE_EDGES
        .iter()
        .find(|edge| edge.from == current && edge.command == command)
    {
        Some(edge) => Ok(MessageTransition {
            from: edge.from,
            to: edge.to,
            command: edge.command,
            preconditions: edge.preconditions,
            events: edge.events,
        }),
        None => Err(MessageRejection {
            current,
            command,
            code: MessageErrorCode::InvalidTransition,
        }),
    }
}

/// The rejection code for a legal-but-unmet edge, or `None` when the command is
/// not a legal edge at all.
pub fn rejection_code(current: MessageStatus, command: MessageCommand) -> Option<MessageErrorCode> {
    MESSAGE_EDGES
        .iter()
        .find(|edge| edge.from == current && edge.command == command)
        .map(|edge| edge.rejection)
}
