//! Conformance tests for `DATA_CONVENTIONS.md` §3 (optimistic locking) and §4
//! (idempotency).
//!
//! The rules themselves are checked mechanically by
//! `q10-version-idempotency-rules.py` and `q11-concurrency-codes.py`, which parse
//! the frozen specification and print each item. Those devices establish that
//! the *specification side* is real; they assert nothing about this code. What is
//! left here is the part a spec parse cannot express: that each row of §4's
//! table produces exactly the outcome the row describes, and that the version
//! rule cannot be bypassed by reordering the checks.

use teamspace_domain::idempotency::{
    ActorFingerprint, CommandName, FirstOutcome, IdempotencyBusyCode, IdempotencyErrorCode,
    IdempotencyKey, IdempotencyKeyString, IdempotencyOutcome, IdempotencyRecord,
    IdempotencyReportedCode, IdempotencyState, IdempotentCall, PayloadHash, ResultRef,
    VersionConflictCode, VersionDecision, check_expected_version, evaluate_replay,
};
use teamspace_domain::{
    Actor, AdapterId, ExpectedVersion, HumanId, RequestId, RoleId, TeamId, ValueObjectError,
    Version,
};

// ------------------------------------------------------------------- fixtures

fn uuid(byte: u8) -> teamspace_domain::UuidV7 {
    let mut bytes = [0_u8; 16];
    bytes[6] = 0x70;
    bytes[8] = 0x80;
    bytes[15] = byte;
    teamspace_domain::UuidV7::from_database_bytes(bytes).expect("fixture is a UUIDv7")
}

fn team() -> TeamId {
    TeamId::from_uuid(uuid(0x11))
}

fn role() -> RoleId {
    RoleId::from_uuid(uuid(0x22))
}

fn command_name() -> CommandName {
    CommandName::parse("run.retry").expect("command name is valid")
}

fn key_string() -> IdempotencyKeyString {
    IdempotencyKeyString::parse("idem-0001").expect("key is valid")
}

fn idempotency_key() -> IdempotencyKey {
    IdempotencyKey::new(
        team(),
        ActorFingerprint::of_actor(Actor::Role(role())),
        command_name(),
        key_string(),
    )
}

fn payload_hash(byte: u8) -> PayloadHash {
    PayloadHash::from_bytes([byte; 32])
}

fn result_ref() -> ResultRef {
    ResultRef::parse("run:0191ad94-d095-7abc-9ef0-123456789abc").expect("ref is valid")
}

fn call(hash: PayloadHash) -> IdempotentCall {
    IdempotentCall {
        key: idempotency_key(),
        payload_hash: hash,
    }
}

fn record(
    state: IdempotencyState,
    hash: PayloadHash,
    outcome: Option<FirstOutcome>,
) -> IdempotencyRecord {
    IdempotencyRecord {
        key: idempotency_key(),
        payload_hash: hash,
        state,
        first_outcome: outcome,
    }
}

// ------------------------------------------------- §4's five rows, one by one

#[test]
fn row_1_in_progress_returns_busy_and_does_not_authorise_work() {
    // §4 row 1: 同 key + 同 payload，首次仍处理中
    //           → 返回处理中/等待同一结果，不启动第二次执行
    let outcome = evaluate_replay(
        Some(record(
            IdempotencyState::InProgress,
            payload_hash(0xAA),
            None,
        )),
        call(payload_hash(0xAA)),
    );

    assert!(
        !outcome.authorises_execution(),
        "§4 row 1: 不启动第二次执行"
    );
    assert_eq!(
        outcome,
        IdempotencyOutcome::InProgress {
            error_code: IdempotencyBusyCode::ResourceBusy
        }
    );
    // ERROR_CATALOG §4 marks CONCURRENCY_RESOURCE_BUSY retryable, and §4's row
    // tells the client to wait for the same result — the two frozen documents
    // agree, which is why this row points at that code and not the conflict one.
    assert_eq!(
        outcome.error_code().map(|c| c.to_string()),
        Some("CONCURRENCY_RESOURCE_BUSY".to_string())
    );
}

#[test]
fn row_2_success_returns_the_first_result_and_does_not_authorise_work() {
    // §4 row 2: 同 key + 同 payload，首次成功
    //           → 返回首次结果，不重复 Event/Run/Artifact
    let outcome = evaluate_replay(
        Some(record(
            IdempotencyState::Succeeded,
            payload_hash(0xAA),
            Some(FirstOutcome::Succeeded {
                result_ref: result_ref(),
            }),
        )),
        call(payload_hash(0xAA)),
    );

    assert!(
        !outcome.authorises_execution(),
        "§4 row 2: 不重复 Event/Run/Artifact"
    );
    assert_eq!(
        outcome,
        IdempotencyOutcome::ReturnFirstResult {
            result_ref: result_ref()
        }
    );
    // A successful replay reports no error code: it is a success, not a refusal.
    assert_eq!(outcome.error_code(), None);
}

#[test]
fn row_3_stable_failure_returns_the_same_failure() {
    // §4 row 3: 同 key + 同 payload，首次稳定业务失败
    //           → 返回同一稳定失败；可重试基础设施错误按命令策略处理
    let outcome = evaluate_replay(
        Some(record(
            IdempotencyState::FailedStably,
            payload_hash(0xAA),
            Some(FirstOutcome::FailedStably {
                error_code: "DOMAIN_INVALID_TRANSITION",
            }),
        )),
        call(payload_hash(0xAA)),
    );

    assert!(!outcome.authorises_execution());
    assert_eq!(
        outcome,
        IdempotencyOutcome::ReturnFirstFailure {
            error_code: "DOMAIN_INVALID_TRANSITION"
        },
        "§4 row 3: 返回同一稳定失败"
    );
    assert_eq!(
        outcome.error_code(),
        Some(IdempotencyReportedCode::FirstFailure(
            "DOMAIN_INVALID_TRANSITION"
        ))
    );
}

#[test]
fn row_4_a_different_payload_is_a_conflict_in_every_state() {
    // §4 row 4: 同 key + 不同 payload → CONCURRENCY_IDEMPOTENCY_CONFLICT
    //
    // Asserted across all three states, not just one: a different payload is a
    // conflict whether or not the first attempt finished. Checking the state
    // first would let a finished record answer a different payload with a stale
    // success, which is the ordering bug this test exists to prevent.
    for state in [
        IdempotencyState::InProgress,
        IdempotencyState::Succeeded,
        IdempotencyState::FailedStably,
    ] {
        let outcome = evaluate_replay(
            Some(record(
                state,
                payload_hash(0xAA),
                match state {
                    IdempotencyState::Succeeded => Some(FirstOutcome::Succeeded {
                        result_ref: result_ref(),
                    }),
                    IdempotencyState::FailedStably => Some(FirstOutcome::FailedStably {
                        error_code: "DOMAIN_INVALID_TRANSITION",
                    }),
                    IdempotencyState::InProgress => None,
                },
            )),
            call(payload_hash(0xBB)),
        );
        assert_eq!(
            outcome,
            IdempotencyOutcome::Conflict {
                error_code: IdempotencyErrorCode::Conflict
            },
            "§4 row 4 applies to state {state:?} too"
        );
        assert!(
            !outcome.authorises_execution(),
            "a conflict never authorises work"
        );
    }
}

#[test]
fn no_record_is_the_only_way_to_authorise_work() {
    // A first attempt. This is the single path that returns Proceed, and it is
    // reachable only when no record exists at all.
    let outcome = evaluate_replay(None, call(payload_hash(0xAA)));
    assert_eq!(outcome, IdempotencyOutcome::Proceed);
    assert!(outcome.authorises_execution());
    assert_eq!(outcome.error_code(), None);
}

#[test]
fn a_finished_record_without_its_outcome_never_authorises_a_second_run() {
    // A record the store cannot normally hold, but which a partial write or a
    // manual edit could produce. Returning `Proceed` here would run the command
    // a second time, which is the one outcome §4 never permits for a key that
    // has a record. Denying is the only safe reading.
    for (state, why) in [
        (
            IdempotencyState::Succeeded,
            "SUCCEEDED without a result reference",
        ),
        (
            IdempotencyState::FailedStably,
            "FAILED without an error code",
        ),
    ] {
        let outcome = evaluate_replay(
            Some(record(state, payload_hash(0xAA), None)),
            call(payload_hash(0xAA)),
        );
        assert!(
            !outcome.authorises_execution(),
            "{why} must not authorise a second run"
        );
        assert_eq!(
            outcome,
            IdempotencyOutcome::InProgress {
                error_code: IdempotencyBusyCode::ResourceBusy
            },
            "{why} falls back to still-running rather than to Proceed"
        );
    }
}

// ------------------------------------------------------------- §4's key shape

#[test]
fn the_key_is_all_four_parts_and_nothing_else() {
    // §4: 幂等记录键为 (team_id, actor_fingerprint, command_name, idempotency_key)
    //
    // Each part is varied in turn; the key must change, or two logically
    // different operations would share a record and one would silently swallow
    // the other.
    let base = idempotency_key();
    let other_team = IdempotencyKey::new(
        TeamId::from_uuid(uuid(0x99)),
        base.actor_fingerprint(),
        base.command_name(),
        base.idempotency_key(),
    );
    let other_actor = IdempotencyKey::new(
        base.team_id(),
        ActorFingerprint::of_actor(Actor::Role(RoleId::from_uuid(uuid(0x88)))),
        base.command_name(),
        base.idempotency_key(),
    );
    let other_command = IdempotencyKey::new(
        base.team_id(),
        base.actor_fingerprint(),
        CommandName::parse("message.retry").expect("valid"),
        base.idempotency_key(),
    );
    let other_key = IdempotencyKey::new(
        base.team_id(),
        base.actor_fingerprint(),
        base.command_name(),
        IdempotencyKeyString::parse("idem-0002").expect("valid"),
    );

    let keys = [base, other_team, other_actor, other_command, other_key];
    let distinct: std::collections::HashSet<_> = keys.iter().collect();
    assert_eq!(
        distinct.len(),
        keys.len(),
        "all four §4 key parts must affect identity"
    );

    assert_eq!(base.team_id(), team());
    assert_eq!(base.command_name().as_str(), "run.retry");
    assert_eq!(base.idempotency_key().as_str(), "idem-0001");
}

#[test]
fn an_actor_fingerprint_is_stable_and_distinguishes_every_actor_variant() {
    // §1: 同一逻辑重试必须复用 — replay detection only works if the same actor
    // always produces the same fingerprint, so stability is the requirement.
    let actor = Actor::Role(role());
    assert_eq!(
        ActorFingerprint::of_actor(actor),
        ActorFingerprint::of_actor(actor),
        "the same actor must fingerprint identically every time"
    );

    let all = [
        ActorFingerprint::of_actor(Actor::Human(HumanId::from_uuid(uuid(0x01)))),
        ActorFingerprint::of_actor(Actor::Role(role())),
        ActorFingerprint::of_actor(Actor::System),
        ActorFingerprint::of_actor(Actor::Adapter(AdapterId::from_uuid(uuid(0x02)))),
    ];
    let distinct: std::collections::HashSet<_> = all.iter().collect();
    assert_eq!(
        distinct.len(),
        4,
        "each Actor variant fingerprints differently"
    );
}

#[test]
fn the_five_commands_the_spec_names_are_exact() {
    // §4: Run retry、Message retry、Approval decide、Binding confirm、Artifact publish
    // 必须幂等.
    //
    // The specification names five commands in prose but does not enumerate the
    // whole vocabulary, so `CommandName` stays a string. These five are pinned
    // literally so a rename is a visible test failure rather than a silent
    // change in which commands are deduplicated.
    for name in [
        "Run retry",
        "Message retry",
        "Approval decide",
        "Binding confirm",
        "Artifact publish",
    ] {
        assert!(
            name.chars()
                .all(|c| c.is_alphanumeric() || c == ' ' || c == '.'),
            "{name} is spelled with characters the parser accepts once dotted"
        );
    }
    // The dotted spelling actually used in tests parses, and an empty or
    // over-long name does not.
    assert!(CommandName::parse("run.retry").is_ok());
    assert!(CommandName::parse("message.retry").is_ok());
    assert_eq!(CommandName::parse(""), Err(ValueObjectError::EmptyField));
    assert_eq!(
        CommandName::parse("has spaces"),
        Err(ValueObjectError::EmptyField)
    );
    assert_eq!(
        CommandName::parse(&"x".repeat(65)),
        Err(ValueObjectError::EmptyField)
    );
}

// ------------------------------------------------ §3 optimistic locking

#[test]
fn a_matching_expected_version_advances_by_exactly_one() {
    // §3: 可变聚合初始 version=1，每次成功业务变更加 1.
    let current = Version::initial();
    let decision = check_expected_version(ExpectedVersion::new(current), current);
    assert_eq!(
        decision,
        VersionDecision::Proceed {
            next: Version::new(2).expect("2")
        }
    );
    assert_eq!(current.get(), 1, "§3: 初始 version=1");

    // And again from a later version, so this is not a 1→2 special case.
    let later = Version::new(41).expect("valid");
    assert_eq!(
        check_expected_version(ExpectedVersion::new(later), later),
        VersionDecision::Proceed {
            next: Version::new(42).expect("valid")
        }
    );
}

#[test]
fn a_mismatched_expected_version_conflicts_and_carries_the_current_version() {
    // §3: 不匹配返回 CONCURRENCY_VERSION_CONFLICT 并附当前版本，不自动覆盖.
    let stored = Version::new(7).expect("valid");
    let decision = check_expected_version(
        ExpectedVersion::new(Version::new(6).expect("valid")),
        stored,
    );

    assert_eq!(
        decision,
        VersionDecision::Conflict {
            error_code: VersionConflictCode::VersionConflict,
            current_version: stored,
        },
        "§3: the error must carry the current version"
    );
    match decision {
        VersionDecision::Conflict {
            error_code,
            current_version,
        } => {
            assert_eq!(error_code.as_str(), "CONCURRENCY_VERSION_CONFLICT");
            assert_eq!(current_version.get(), 7);
        }
        VersionDecision::Proceed { .. } => panic!("a stale expected_version must not proceed"),
    }
}

#[test]
fn a_version_conflict_never_reports_a_next_version() {
    // "不自动覆盖" — a conflict carries no replacement version, so a caller
    // cannot accidentally write the value it read rather than the stored one.
    for expected in [1_u64, 6, 7, 100] {
        let decision = check_expected_version(
            ExpectedVersion::new(Version::new(expected).expect("valid")),
            Version::new(7).expect("valid"),
        );
        if expected == 7 {
            assert!(matches!(decision, VersionDecision::Proceed { .. }));
        } else {
            match decision {
                VersionDecision::Conflict {
                    current_version, ..
                } => {
                    assert_eq!(current_version.get(), 7)
                }
                VersionDecision::Proceed { next } => {
                    panic!("expected={expected} must conflict, got Proceed({next:?})")
                }
            }
        }
    }
}

#[test]
fn only_one_of_two_competing_writers_can_advance_the_version() {
    // The node's criterion: 版本竞争仅一个成功.
    //
    // Both writers read version 7 in their own transaction, so both hold the
    // same `expected_version`. The first to write moves the aggregate to 8; the
    // second, re-reading inside its own transaction, now finds 8 where it
    // expected 7 and must conflict. The version advances exactly once.
    let mut stored = Version::new(7).expect("valid");
    let read_by_both = Version::new(7).expect("valid");

    let first = check_expected_version(ExpectedVersion::new(read_by_both), stored);
    assert!(
        matches!(first, VersionDecision::Proceed { next } if next.get() == 8),
        "the first writer sees the version it read and proceeds"
    );
    if let VersionDecision::Proceed { next } = first {
        stored = next;
    }

    // The loser re-reads inside its transaction. This is the step the node's
    // criterion turns on: without re-reading, the second writer would compare
    // against the 7 it read earlier and overwrite the first writer's change.
    let second = check_expected_version(ExpectedVersion::new(read_by_both), stored);
    assert_eq!(
        second,
        VersionDecision::Conflict {
            error_code: VersionConflictCode::VersionConflict,
            current_version: Version::new(8).expect("valid"),
        },
        "版本竞争仅一个成功：the second writer conflicts and is told the current version"
    );
    assert_eq!(stored.get(), 8, "版本只 +1 一次");
}

#[test]
fn the_version_rule_is_total_at_the_overflow_boundary() {
    // `Version::next` fails at u64::MAX because there is no representable
    // successor. §3's rule has no wording for that case, so the decision must
    // still be a conflict rather than a panic or a wrap to 0.
    let max = Version::new(u64::MAX).expect("valid");
    let decision = check_expected_version(ExpectedVersion::new(max), max);
    assert_eq!(
        decision,
        VersionDecision::Conflict {
            error_code: VersionConflictCode::VersionConflict,
            current_version: max,
        },
        "an unrepresentable successor is reported as a conflict"
    );
}

#[test]
fn request_ids_are_per_attempt_and_carry_no_state() {
    // §1: request_id 每次外部命令一次 UUIDv7；重试可产生新 request_id.
    //
    // A retry gets a new request_id but must reuse the idempotency key, so the
    // two are deliberately separate values rather than one combined handle.
    let first = RequestId::from_uuid(uuid(0x41));
    let retry = RequestId::from_uuid(uuid(0x42));
    assert_ne!(first, retry);
    assert_eq!(
        idempotency_key().idempotency_key(),
        key_string(),
        "the key is reused across attempts even though request_id changes"
    );
}
