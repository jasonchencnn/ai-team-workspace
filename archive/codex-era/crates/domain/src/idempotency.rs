//! §4 idempotency rules, frozen by `DATA_CONVENTIONS.md` §4.
//!
//! ```text
//! 幂等记录键为 (team_id, actor_fingerprint, command_name, idempotency_key)，
//! 保存 payload hash、状态和首次结果引用。
//! ```
//!
//! | 情况 | 行为 |
//! |---|---|
//! | 同 key + 同 payload，首次仍处理中 | 返回处理中/等待同一结果，不启动第二次执行 |
//! | 同 key + 同 payload，首次成功 | 返回首次结果，不重复 Event/Run/Artifact |
//! | 同 key + 同 payload，首次稳定业务失败 | 返回同一稳定失败；可重试基础设施错误按命令策略处理 |
//! | 同 key + 不同 payload | `CONCURRENCY_IDEMPOTENCY_CONFLICT` |
//! | key 已超保留期 | 关键命令仍通过唯一约束/业务引用防重复；客户端不应复用 |
//!
//! # The five rows are the whole contract
//!
//! [`crate::idempotency::evaluate_replay`] is a total function over
//! `(stored record, incoming request)` — the four columns are enumerated, not
//! matched by string comparison against the table above. Each row has a test
//! named for it (`row_1_in_progress_…` through `row_4_a_different_payload_…` in
//! `tests/idempotency.rs`), so a row added to the specification is visible as a
//! missing test in review rather than as a silent gap.
//!
//! That is a review-time check, not an automated one: nothing in this crate
//! re-reads `DATA_CONVENTIONS.md` at test time. An earlier version of this
//! comment named a device (`every_spec_row_has_a_case`) as though such a check
//! existed. It did not, and a comment that claims a safety net which is not
//! there is worse than no comment — it stops the next reader from building one.
//!
//! The fifth row is deliberately *not* a case: it describes a key past its
//! retention window, which `DATA_CONVENTIONS` §6 says is still prevented by the
//! unique constraint rather than by this decision. Modelling it as "allow" would
//! make a replay of an expired key look safe, which is the opposite of the row.
//!
//! # A replay never starts a second execution
//!
//! [`crate::idempotency::IdempotencyOutcome`] is the *only* thing a caller may branch on. Every
//! variant that is not [`crate::idempotency::IdempotencyOutcome::Proceed`] carries either the first
//! result or the reason the first attempt is still running, so a caller cannot
//! accidentally treat "already succeeded" as "go do it again". The one variant
//! that authorises work, [`crate::idempotency::IdempotencyOutcome::Proceed`], is reachable only when
//! no record exists at all — and
//! `no_record_is_the_only_way_to_authorise_work` plus
//! `a_finished_record_without_its_outcome_never_authorises_a_second_run` hold
//! that line.
//!
//! # The payload hash is injected, never computed here
//!
//! [`crate::idempotency::PayloadHash`] is an opaque 32-byte digest. §4 says "保存 payload hash" and
//! nothing about how to produce one, so this module compares digests and leaves
//! their construction to the caller. Inventing a hash here would freeze an
//! algorithm the specification never chose, and the storage format is where a
//! mismatch would be discovered.

use std::fmt;

use crate::{Actor, ExpectedVersion, RequestId, TeamId, ValueObjectError, Version};

/// The record key, verbatim from §4's first sentence.
///
/// The four parts are stored in §4's printed order, and the docstring quotes the
/// specification so a reader can check the shape without opening the file.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct IdempotencyKey {
    team_id: TeamId,
    actor_fingerprint: ActorFingerprint,
    command_name: CommandName,
    idempotency_key: IdempotencyKeyString,
}

impl IdempotencyKey {
    /// Composes the four parts §4 names. The key is a value: two requests with
    /// equal parts are the same logical operation, which is what makes
    /// "同一逻辑重试必须复用" (`DATA_CONVENTIONS` §1) a type-level fact.
    pub const fn new(
        team_id: TeamId,
        actor_fingerprint: ActorFingerprint,
        command_name: CommandName,
        idempotency_key: IdempotencyKeyString,
    ) -> Self {
        Self {
            team_id,
            actor_fingerprint,
            command_name,
            idempotency_key,
        }
    }

    /// The `team_id` part.
    pub const fn team_id(self) -> TeamId {
        self.team_id
    }

    /// The `actor_fingerprint` part.
    pub const fn actor_fingerprint(self) -> ActorFingerprint {
        self.actor_fingerprint
    }

    /// The `command_name` part.
    pub const fn command_name(self) -> CommandName {
        self.command_name
    }

    /// The `idempotency_key` part.
    pub const fn idempotency_key(self) -> IdempotencyKeyString {
        self.idempotency_key
    }
}

/// §4's second key part: a stable fingerprint of the calling actor.
///
/// §4 names the part but not how to compute it, so this is an opaque value
/// object. [`ActorFingerprint::of_actor`] gives a deterministic derivation for
/// the four [`Actor`] variants, which is enough for the rules to be testable
/// without claiming the specification prescribed it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ActorFingerprint([u8; 32]);

impl ActorFingerprint {
    /// Wraps a digest produced by the application layer.
    pub const fn from_bytes(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }

    /// A deterministic fingerprint of an [`Actor`].
    ///
    /// This is a *derivation*, not a specification: §4 requires the key to
    /// contain an actor fingerprint and does not say how one is produced. Two
    /// requests from the same actor must produce equal fingerprints or replay
    /// detection silently stops working, so the mapping is fixed here rather
    /// than left to each call site.
    ///
    /// It is deliberately simple and carries **no security claim**: §4 wants
    /// stability, not unguessability, and the fingerprint never leaves the
    /// unique key. A cryptographic digest here would be a claim the
    /// specification does not make and an algorithm frozen by nothing.
    pub fn of_actor(actor: Actor) -> Self {
        let mut bytes = [0_u8; 32];
        // Domain-separated prefix so a fingerprint can never be confused with a
        // payload digest that happens to hash the same bytes.
        bytes[..8].copy_from_slice(b"actorfp1");
        let mut body = [0_u8; 17];
        match actor {
            Actor::Human(id) => {
                body[0] = 1;
                body[1..].copy_from_slice(&id.database_bytes());
            }
            Actor::Role(id) => {
                body[0] = 2;
                body[1..].copy_from_slice(&id.database_bytes());
            }
            Actor::System => body[0] = 3,
            Actor::Adapter(id) => {
                body[0] = 4;
                body[1..].copy_from_slice(&id.database_bytes());
            }
        }
        // 8 prefix bytes + 16 spread bytes = 24 of 32; the remaining 8 stay zero
        // so the fingerprint has a fixed width regardless of the actor.
        let spread = spread128(&body);
        bytes[8..8 + 16].copy_from_slice(&spread);
        Self(bytes)
    }

    /// The 32 fingerprint bytes, for storage and comparison.
    pub fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }
}

/// §4's third key part: the command's stable name.
///
/// `DATA_CONVENTIONS` §4 names five commands that must be idempotent
/// (Run retry, Message retry, Approval decide, Binding confirm, Artifact
/// publish) but does not enumerate the whole command vocabulary, so this stays a
/// validated string rather than an invented enum. The five named ones are
/// pinned by `the_five_commands_the_spec_names_are_exact`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct CommandName(CompactString);

impl CommandName {
    /// Validates a non-empty command name.
    pub fn parse(value: &str) -> Result<Self, ValueObjectError> {
        if value.is_empty() || value.len() > 64 {
            return Err(ValueObjectError::EmptyField);
        }
        if !value.bytes().all(|byte| {
            byte.is_ascii_alphanumeric() || byte == b'.' || byte == b'_' || byte == b'-'
        }) {
            return Err(ValueObjectError::EmptyField);
        }
        Ok(Self(CompactString::new(value)))
    }

    /// The command name as written.
    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }
}

/// §4's fourth key part, and `DATA_CONVENTIONS` §1's `idempotency_key`:
/// "调用者按逻辑操作生成的稳定字符串/UUID".
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct IdempotencyKeyString(CompactString);

impl IdempotencyKeyString {
    /// Validates a non-empty caller-supplied key.
    pub fn parse(value: &str) -> Result<Self, ValueObjectError> {
        if value.is_empty() || value.len() > 128 {
            return Err(ValueObjectError::EmptyField);
        }
        Ok(Self(CompactString::new(value)))
    }

    /// The key as supplied by the caller.
    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }
}

/// §4's "保存 payload hash": an opaque digest of the command payload.
///
/// The comparison below is over these bytes. Which digest algorithm produces
/// them is the application's choice — the specification records that a hash is
/// kept, not which one.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct PayloadHash([u8; 32]);

impl PayloadHash {
    /// Wraps a digest produced by the application layer.
    pub const fn from_bytes(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }

    /// The 32 digest bytes, for storage and comparison.
    pub fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }
}

/// §4's "状态": whether the first attempt is still running, succeeded, or
/// failed stably.
///
/// These three are the record states the four actionable rows refer to. The
/// fifth row — a key past its retention window — is not a state: the
/// specification keeps that case enforced by the unique constraint instead.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum IdempotencyState {
    /// §4 row 1: 首次仍处理中.
    InProgress,
    /// §4 row 2: 首次成功.
    Succeeded,
    /// §4 row 3: 首次稳定业务失败.
    FailedStably,
}

impl IdempotencyState {
    /// The storage spelling. These three strings are this module's choice: the
    /// specification writes the states in prose, so any stable spelling will
    /// do as long as the migration and this agree.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::InProgress => "IN_PROGRESS",
            Self::Succeeded => "SUCCEEDED",
            Self::FailedStably => "FAILED",
        }
    }
}

/// §4's "首次结果引用": what a replay must return instead of re-executing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum FirstOutcome {
    /// §4 row 2: the first attempt's successful result.
    Succeeded {
        /// A reference to the first result, opaque to this module.
        result_ref: ResultRef,
    },
    /// §4 row 3: the first attempt's stable business failure.
    FailedStably {
        /// The stable error code the first attempt produced.
        error_code: &'static str,
    },
}

/// An opaque reference to a first result. The specification says the record
/// stores a reference and does not say what kind, so the domain layer only
/// carries and compares it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ResultRef(CompactString);

impl ResultRef {
    /// Validates a non-empty result reference.
    pub fn parse(value: &str) -> Result<Self, ValueObjectError> {
        if value.is_empty() || value.len() > 128 {
            return Err(ValueObjectError::EmptyField);
        }
        Ok(Self(CompactString::new(value)))
    }

    /// The reference as stored.
    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }
}

/// The stored record, i.e. one row of the `command_idempotency` table.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct IdempotencyRecord {
    /// The §4 key.
    pub key: IdempotencyKey,
    /// §4's "payload hash".
    pub payload_hash: PayloadHash,
    /// §4's "状态".
    pub state: IdempotencyState,
    /// §4's "首次结果引用", present once the first attempt finished.
    pub first_outcome: Option<FirstOutcome>,
}

/// The incoming request, i.e. the second caller's arguments.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct IdempotentCall {
    /// The §4 key the caller used.
    pub key: IdempotencyKey,
    /// The digest of this call's payload.
    pub payload_hash: PayloadHash,
}

/// The frozen §4 codes this module can report.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum IdempotencyErrorCode {
    /// §4 row 4: 同 key + 不同 payload.
    Conflict,
}

impl IdempotencyErrorCode {
    /// The frozen catalogue code, per `ERROR_CATALOG.md` §4.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Conflict => "CONCURRENCY_IDEMPOTENCY_CONFLICT",
        }
    }
}

/// The decision a caller may branch on. Nothing else authorises execution.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IdempotencyOutcome {
    /// No record exists: this is a first attempt and the caller may run it.
    Proceed,
    /// §4 row 1: 首次仍处理中 → 返回处理中/等待同一结果，不启动第二次执行.
    ///
    /// `CONCURRENCY_RESOURCE_BUSY` is the catalogued code here because
    /// `ERROR_CATALOG.md` §4 marks it retryable, and §4's row says the client
    /// waits for the same result.
    InProgress {
        /// The stable catalogue code for "still running".
        error_code: IdempotencyBusyCode,
    },
    /// §4 row 2: 首次成功 → 返回首次结果，不重复 Event/Run/Artifact.
    ReturnFirstResult {
        /// The first attempt's result reference.
        result_ref: ResultRef,
    },
    /// §4 row 3: 首次稳定业务失败 → 返回同一稳定失败.
    ReturnFirstFailure {
        /// The first attempt's stable error code.
        error_code: &'static str,
    },
    /// §4 row 4: 同 key + 不同 payload.
    Conflict {
        /// The frozen catalogue code.
        error_code: IdempotencyErrorCode,
    },
}

/// `ERROR_CATALOG.md` §4's `CONCURRENCY_RESOURCE_BUSY`, kept as its own type so
/// the retryable flag is a property of the type rather than a comment.
///
/// # Why this code, stated as a fact rather than an argument
///
/// §4's first row says 同 key + 同 payload，首次仍处理中 →
/// 返回处理中/等待同一结果. A client that is told to wait must be holding a
/// code it may retry, so the row needs a `retryable=是` code.
///
/// §4 lists **three** codes marked 是 — `CONCURRENCY_RESOURCE_BUSY`,
/// `CONCURRENCY_LIMIT_REACHED` and `OUTBOX_DELIVERY_PENDING` — so "it is the
/// only retryable one" would be a false statement about the catalogue. An
/// earlier version of this comment made exactly that claim; it was wrong, and a
/// wrong citation of a frozen document is worse than no citation because it
/// reads as though the question had already been settled.
///
/// `CONCURRENCY_RESOURCE_BUSY` is chosen on **semantics**, not on exclusivity:
/// its catalogued remedy is 有限退避；`retry_after_ms`, and its hint is
/// 资源正在处理中，请稍后重试 — which is precisely §4 row 1's situation.
/// `CONCURRENCY_LIMIT_REACHED` means a concurrency ceiling was hit and
/// `OUTBOX_DELIVERY_PENDING` refers to a notification still in flight, neither
/// of which is "your request is being processed under its own key".
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum IdempotencyBusyCode {
    /// 资源正在处理中，请稍后重试。
    ResourceBusy,
}

impl IdempotencyBusyCode {
    /// The frozen catalogue code.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::ResourceBusy => "CONCURRENCY_RESOURCE_BUSY",
        }
    }
}

impl IdempotencyOutcome {
    /// `true` only for [`IdempotencyOutcome::Proceed`].
    ///
    /// Every other variant is a replay that must not start work, so a caller
    /// that wants "should I execute?" has exactly one branch to write.
    pub fn authorises_execution(self) -> bool {
        matches!(self, Self::Proceed)
    }

    /// The catalogue code this decision reports, if it is a refusal.
    ///
    /// A caller handing this to the `ERROR_CATALOG` cannot pick a code the
    /// specification does not pair with this situation.
    pub fn error_code(self) -> Option<IdempotencyReportedCode> {
        match self {
            Self::Proceed | Self::ReturnFirstResult { .. } => None,
            Self::ReturnFirstFailure { error_code } => {
                Some(IdempotencyReportedCode::FirstFailure(error_code))
            }
            Self::InProgress { error_code } => Some(IdempotencyReportedCode::Busy(error_code)),
            Self::Conflict { error_code } => Some(IdempotencyReportedCode::Conflict(error_code)),
        }
    }
}

/// A reported code, keeping the three sources distinguishable.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum IdempotencyReportedCode {
    /// `CONCURRENCY_RESOURCE_BUSY`, retryable per the catalogue.
    Busy(IdempotencyBusyCode),
    /// `CONCURRENCY_IDEMPOTENCY_CONFLICT`, not retryable.
    Conflict(IdempotencyErrorCode),
    /// The first attempt's stable business failure, replayed verbatim.
    FirstFailure(&'static str),
}

impl fmt::Display for IdempotencyReportedCode {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Busy(code) => formatter.write_str(code.as_str()),
            Self::Conflict(code) => formatter.write_str(code.as_str()),
            Self::FirstFailure(code) => formatter.write_str(code),
        }
    }
}

/// §4's decision function. Total, pure, and the only place a replay is decided.
///
/// # The order of the two comparisons
///
/// Payload first, state second. §4's rows all distinguish "同 payload" from
/// "不同 payload", and a different payload is a conflict whether the first
/// attempt finished or not. Checking state first would let a `SUCCEEDED` record
/// answer a different payload with a stale success.
pub fn evaluate_replay(
    stored: Option<IdempotencyRecord>,
    call: IdempotentCall,
) -> IdempotencyOutcome {
    let Some(record) = stored else {
        return IdempotencyOutcome::Proceed;
    };

    // Row 4 applies to any state: a different payload is a different logical
    // operation that reused a key, which the specification calls a conflict.
    if record.payload_hash != call.payload_hash {
        return IdempotencyOutcome::Conflict {
            error_code: IdempotencyErrorCode::Conflict,
        };
    }

    // A record under a different key is a caller bug rather than a replay; the
    // unique constraint is what prevents it, and this keeps the function total
    // instead of returning a wrong answer for a state the store cannot hold.
    if record.key != call.key {
        return IdempotencyOutcome::Conflict {
            error_code: IdempotencyErrorCode::Conflict,
        };
    }

    match record.state {
        IdempotencyState::InProgress => IdempotencyOutcome::InProgress {
            error_code: IdempotencyBusyCode::ResourceBusy,
        },
        IdempotencyState::Succeeded => match record.first_outcome {
            Some(FirstOutcome::Succeeded { result_ref }) => {
                IdempotencyOutcome::ReturnFirstResult { result_ref }
            }
            // A succeeded record with no result reference cannot be answered
            // honestly, and returning `Proceed` here would run the command a
            // second time. Denying is the only safe reading.
            _ => IdempotencyOutcome::InProgress {
                error_code: IdempotencyBusyCode::ResourceBusy,
            },
        },
        IdempotencyState::FailedStably => match record.first_outcome {
            Some(FirstOutcome::FailedStably { error_code }) => {
                IdempotencyOutcome::ReturnFirstFailure { error_code }
            }
            _ => IdempotencyOutcome::InProgress {
                error_code: IdempotencyBusyCode::ResourceBusy,
            },
        },
    }
}

/// `STATE_MACHINES.md`'s opening sentence: every mutating command carries
/// `request_id`, and `DATA_CONVENTIONS` §3 requires `expected_version`.
///
/// The two preconditions are gathered here so a caller cannot apply one and
/// forget the other, which is the failure mode a struct of independent
/// parameters invites.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MutationPreconditions {
    /// §3: 每次外部命令一次 UUIDv7, and a retry may carry a new one.
    pub request_id: RequestId,
    /// §3: Command 必须传 expected_version, 创建命令除外.
    pub expected_version: ExpectedVersion,
}

/// §3's optimistic-lock decision, frozen by `DATA_CONVENTIONS.md` §3:
///
/// > Command 必须传 `expected_version`，创建命令除外；不匹配返回
/// > `CONCURRENCY_VERSION_CONFLICT` 并附当前版本，不自动覆盖。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VersionDecision {
    /// The caller's `expected_version` equals the stored one; the mutation may
    /// proceed and the new version is `current.next()`.
    Proceed {
        /// The version the aggregate will hold after this change.
        next: Version,
    },
    /// §3: 不匹配 → `CONCURRENCY_VERSION_CONFLICT` 并附当前版本，不自动覆盖.
    Conflict {
        /// The frozen catalogue code.
        error_code: VersionConflictCode,
        /// The version actually stored, which §3 requires the error to carry.
        current_version: Version,
    },
}

/// `ERROR_CATALOG.md` §4's `CONCURRENCY_VERSION_CONFLICT`.
///
/// A distinct type from [`IdempotencyErrorCode`] because the catalogue marks it
/// not retryable while `CONCURRENCY_RESOURCE_BUSY` is retryable; collapsing
/// them would lose the one fact a client branches on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum VersionConflictCode {
    /// 这项内容已被更新，请刷新后比较再提交。
    VersionConflict,
}

impl VersionConflictCode {
    /// The frozen catalogue code.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::VersionConflict => "CONCURRENCY_VERSION_CONFLICT",
        }
    }
}

/// §3's check, as a pure function.
///
/// `current` is the version read inside the same transaction that will write the
/// change; comparing against a value read earlier is the race this function
/// exists to make harmless. The caller must therefore re-read inside the
/// transaction — passing a stale `current` here reproduces the lost update that
/// §3 forbids, and nothing in this signature can detect that.
pub fn check_expected_version(expected: ExpectedVersion, current: Version) -> VersionDecision {
    if expected.matches(current) {
        // `next` can only fail at u64::MAX, where there is no representable
        // successor. Treating that as a conflict keeps the function total.
        match current.next() {
            Ok(next) => VersionDecision::Proceed { next },
            Err(_) => VersionDecision::Conflict {
                error_code: VersionConflictCode::VersionConflict,
                current_version: current,
            },
        }
    } else {
        VersionDecision::Conflict {
            error_code: VersionConflictCode::VersionConflict,
            current_version: current,
        }
    }
}

/// A fixed, non-cryptographic spreading function over 128 bits.
///
/// It exists only so [`ActorFingerprint::of_actor`] is a pure derivation with no
/// allocation and no dependency. It is **not** a hash with a security claim, and
/// it is **not** the payload hash — §4 leaves that to the application.
fn spread128(bytes: &[u8]) -> [u8; 16] {
    let mut hash = [0x6a_u8; 16];
    let mut index = 0;
    while index < bytes.len() {
        let lane = index % 16;
        // u64 arithmetic to avoid the u8 overflow that a byte-wise FNV needs to
        // spell out; the widths here are small and fixed, so wrapping is exact.
        let product = u64::from(hash[lane]).wrapping_mul(0x0000_0100_0000_01b3);
        let mixed =
            product ^ (product >> 29) ^ u64::from(bytes[index]).wrapping_mul(0x9e37_79b9_7f4a_7c15);
        hash[lane] = (mixed >> 7) as u8;
        hash[(lane + 7) % 16] = hash[(lane + 7) % 16].wrapping_add(mixed as u8);
        index += 1;
    }
    hash
}

/// An inline string with no heap allocation, so every key type above is `Copy`.
///
/// Bounded by the `parse` limits (64 / 128 bytes), which is what lets the record
/// key be compared and stored by value with no allocation on the hot path.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct CompactString {
    bytes: [u8; CompactString::CAPACITY],
    len: u8,
}

impl CompactString {
    const CAPACITY: usize = 128;

    /// # Panics
    ///
    /// Panics if `value` is longer than [`CompactString::CAPACITY`]. Every call
    /// site validates against a smaller bound first.
    fn new(value: &str) -> Self {
        let bytes = value.as_bytes();
        assert!(
            bytes.len() <= Self::CAPACITY,
            "CompactString capacity exceeded by construction"
        );
        let mut buffer = [0_u8; Self::CAPACITY];
        buffer[..bytes.len()].copy_from_slice(bytes);
        Self {
            bytes: buffer,
            len: bytes.len() as u8,
        }
    }

    /// The stored string.
    pub fn as_str(&self) -> &str {
        // Only ASCII is accepted by every `parse` above, so this cannot fail.
        core::str::from_utf8(&self.bytes[..usize::from(self.len)])
            .expect("CompactString only stores ASCII")
    }
}
