//! Table-driven conformance tests for the Message state machine.
//!
//! Same two-layer shape as the Task / Run / Binding suites: `EXPECTED_ROWS` is a
//! hand-transcription of `STATE_MACHINES.md` §5's table, asserted against
//! [`MESSAGE_EDGES`] in both directions.
//!
//! Three §5 facts get dedicated tests because a mechanical transcription would
//! miss all of them:
//!
//! * `RETRYING` is a derived transient and must not be a state.
//! * The Event cell `MESSAGE_REASSIGNED`/`MESSAGE_REQUEUED` is mutually
//!   exclusive (`/`), unlike §5's `MESSAGE_SENT`、`MESSAGE_QUEUED` (`、`), so it
//!   is two commands and two edges.
//! * `QUEUED/ROUTED → EXPIRED` and `PROCESSING/ACKNOWLEDGED → REPLIED` are one
//!   printed line each and two edges each.

use teamspace_domain::{
    CREATE_DRAFT, MESSAGE_CREATE_TARGETS, MESSAGE_EDGES, MessageCommand, MessageErrorCode,
    MessageEvent, MessagePrecondition, MessageStatus, message_creation_target,
    message_rejection_code as rejection_code, message_transition as transition,
};

/// Every non-creation row of §5's table: (from, command, to, events, rejection).
type Row = (
    MessageStatus,
    MessageCommand,
    MessageStatus,
    &'static [MessageEvent],
    MessageErrorCode,
);

const EXPECTED_ROWS: &[Row] = &[
    (
        MessageStatus::Draft,
        MessageCommand::Send,
        MessageStatus::Queued,
        &[MessageEvent::MessageSent, MessageEvent::MessageQueued],
        MessageErrorCode::ValidationInvalidField,
    ),
    (
        MessageStatus::Queued,
        MessageCommand::Route,
        MessageStatus::Routed,
        &[MessageEvent::MessageRouted],
        MessageErrorCode::BindingNotFound,
    ),
    (
        MessageStatus::Queued,
        MessageCommand::NeedsHuman,
        MessageStatus::NeedsHuman,
        &[MessageEvent::MessageNeedsHuman],
        MessageErrorCode::PreconditionFailed,
    ),
    // §5 prints `QUEUED/ROUTED → EXPIRED` as one line; two edges.
    (
        MessageStatus::Queued,
        MessageCommand::Expire,
        MessageStatus::Expired,
        &[MessageEvent::MessageExpired],
        MessageErrorCode::PreconditionFailed,
    ),
    (
        MessageStatus::Routed,
        MessageCommand::Expire,
        MessageStatus::Expired,
        &[MessageEvent::MessageExpired],
        MessageErrorCode::PreconditionFailed,
    ),
    (
        MessageStatus::Routed,
        MessageCommand::Deliver,
        MessageStatus::Delivered,
        &[MessageEvent::MessageDelivered],
        MessageErrorCode::BindingStaleVersion,
    ),
    (
        MessageStatus::Routed,
        MessageCommand::ScheduleRetry,
        MessageStatus::Queued,
        &[MessageEvent::MessageRetryScheduled],
        MessageErrorCode::PreconditionFailed,
    ),
    (
        MessageStatus::Routed,
        MessageCommand::Fail,
        MessageStatus::Failed,
        &[MessageEvent::MessageFailed],
        MessageErrorCode::PreconditionFailed,
    ),
    (
        MessageStatus::Delivered,
        MessageCommand::Acknowledge,
        MessageStatus::Acknowledged,
        &[MessageEvent::MessageAcknowledged],
        MessageErrorCode::BindingStaleVersion,
    ),
    (
        MessageStatus::Acknowledged,
        MessageCommand::Process,
        MessageStatus::Processing,
        &[MessageEvent::MessageProcessing],
        MessageErrorCode::PermissionDenied,
    ),
    // §5 prints `PROCESSING/ACKNOWLEDGED → REPLIED` as one line; two edges.
    (
        MessageStatus::Processing,
        MessageCommand::Reply,
        MessageStatus::Replied,
        &[MessageEvent::MessageReplied],
        MessageErrorCode::ThreadMismatch,
    ),
    (
        MessageStatus::Acknowledged,
        MessageCommand::Reply,
        MessageStatus::Replied,
        &[MessageEvent::MessageReplied],
        MessageErrorCode::ThreadMismatch,
    ),
    (
        MessageStatus::Failed,
        MessageCommand::RetryFailed,
        MessageStatus::Queued,
        &[MessageEvent::MessageRequeued],
        MessageErrorCode::TtlExpired,
    ),
    // §5's Event cell here is `MESSAGE_REASSIGNED`/`MESSAGE_REQUEUED`.
    (
        MessageStatus::NeedsHuman,
        MessageCommand::Reassign,
        MessageStatus::Queued,
        &[MessageEvent::MessageReassigned],
        MessageErrorCode::HumanRequired,
    ),
    (
        MessageStatus::NeedsHuman,
        MessageCommand::RequeueNeedsHuman,
        MessageStatus::Queued,
        &[MessageEvent::MessageRequeued],
        MessageErrorCode::HumanRequired,
    ),
];

const ALL_COMMANDS: [MessageCommand; 13] = MessageCommand::ALL;

#[test]
fn the_table_holds_exactly_the_specified_rows() {
    assert_eq!(
        MESSAGE_EDGES.len(),
        EXPECTED_ROWS.len(),
        "§5 has 13 printed rows → 15 edges (two source fan-outs, one event split); \
         the table holds {}",
        MESSAGE_EDGES.len()
    );
    for (from, command, to, events, rejection) in EXPECTED_ROWS {
        let decided = transition(*from, *command);
        let t = decided.unwrap_or_else(|r| {
            panic!("§5 requires {from} --{command}--> {to}, but rejected: {r}")
        });
        assert_eq!(t.from(), *from);
        assert_eq!(t.to(), *to, "§5 destination for {from} --{command}-->");
        assert_eq!(t.command(), *command);
        assert_eq!(
            t.events(),
            *events,
            "§5 Event cell for {from} --{command}-->"
        );
        assert_eq!(
            rejection_code(*from, *command),
            Some(*rejection),
            "{from} --{command}--> lost its §5 拒绝码"
        );
    }
}

#[test]
fn the_table_holds_no_edge_the_specification_omits() {
    for status in MessageStatus::ALL {
        for command in ALL_COMMANDS {
            let in_table = MESSAGE_EDGES
                .iter()
                .any(|e| e.from == status && e.command == command);
            let in_spec = EXPECTED_ROWS
                .iter()
                .any(|(from, cmd, ..)| *from == status && *cmd == command);
            assert_eq!(
                in_table, in_spec,
                "{status} --{command}--> table={in_table} spec={in_spec}"
            );
        }
    }
}

#[test]
fn each_pair_resolves_to_exactly_one_row() {
    for (i, a) in MESSAGE_EDGES.iter().enumerate() {
        for b in &MESSAGE_EDGES[i + 1..] {
            assert!(
                !(a.from == b.from && a.command == b.command),
                "{} --{}--> is claimed by two rows ({:?} and {:?})",
                a.from,
                a.command,
                a.to,
                b.to
            );
        }
    }
    for status in MessageStatus::ALL {
        for command in ALL_COMMANDS {
            let matches = MESSAGE_EDGES
                .iter()
                .filter(|e| e.from == status && e.command == command)
                .count();
            assert!(
                matches <= 1,
                "{status} --{command}--> matched {matches} rows"
            );
            match transition(status, command) {
                Ok(_) => assert!(matches == 1, "{status} --{command}--> not in table"),
                Err(_) => assert!(
                    matches == 0,
                    "{status} --{command}--> rejected but in table"
                ),
            }
        }
    }
}

#[test]
fn there_is_no_retrying_state() {
    // §5: "RETRYING 是"已安排下一次 attempt"的派生瞬态，不持久化为 Message 终态".
    // A RETRYING column would make the transient persistable, and then two
    // questions become unanswerable: is a RETRYING Message claimable, and does it
    // expire? §5 answers neither, because it is not a state.
    let names: Vec<&str> = MessageStatus::ALL.iter().map(|s| s.as_str()).collect();
    assert!(
        !names.contains(&"RETRYING"),
        "§5 forbids persisting RETRYING as a Message state, found {names:?}"
    );
    assert_eq!(names.len(), 10);
    // The retry itself is a real edge back to QUEUED, not a resting place.
    let t = transition(MessageStatus::Routed, MessageCommand::ScheduleRetry)
        .expect("§5 has ROUTED → QUEUED for a scheduled retry");
    assert_eq!(t.to(), MessageStatus::Queued);
    assert_eq!(t.events(), &[MessageEvent::MessageRetryScheduled]);
}

#[test]
fn reassign_and_requeue_are_distinguishable() {
    // §5 prints this Event cell with `/` (mutually exclusive), not `、` (both).
    // The convention holds across the whole frozen file: `、` appears on six §2
    // rows and one §5 row, and means both events; `/` appears on exactly two rows
    // in the entire spec — this one and §4's `ROLE_BOUND`/`ROLE_REBOUND`.
    //
    // Collapsing them would lose the route-override record: §5's 副作用 for this
    // row is "新 attempt，记录 route override", and a Human who repaired the
    // Binding and re-ran the *same* target is a different event from a Human who
    // moved the Message elsewhere.
    let reassign = transition(MessageStatus::NeedsHuman, MessageCommand::Reassign)
        .expect("§5 NEEDS_HUMAN → QUEUED (reassign)");
    let requeue = transition(MessageStatus::NeedsHuman, MessageCommand::RequeueNeedsHuman)
        .expect("§5 NEEDS_HUMAN → QUEUED (requeue)");
    assert_eq!(reassign.to(), requeue.to());
    assert_eq!(reassign.to(), MessageStatus::Queued);
    assert_eq!(reassign.events(), &[MessageEvent::MessageReassigned]);
    assert_eq!(requeue.events(), &[MessageEvent::MessageRequeued]);
    assert_ne!(reassign.events(), requeue.events());
    // Same preconditions and code: only the proven fact differs.
    assert_eq!(reassign.preconditions(), requeue.preconditions());
    assert_eq!(
        rejection_code(MessageStatus::NeedsHuman, MessageCommand::Reassign),
        rejection_code(MessageStatus::NeedsHuman, MessageCommand::RequeueNeedsHuman)
    );
}

#[test]
fn send_emits_both_events_because_the_spec_joins_them_with_a_comma() {
    // The counterweight to the test above: `DRAFT → QUEUED` prints
    // `MESSAGE_SENT`、`MESSAGE_QUEUED` with `、`, which the same convention reads
    // as BOTH. If `、` and `/` were interchangeable this edge would be wrong too.
    let t = transition(MessageStatus::Draft, MessageCommand::Send).expect("§5 DRAFT → QUEUED");
    assert_eq!(t.events().len(), 2);
    assert!(t.events().contains(&MessageEvent::MessageSent));
    assert!(t.events().contains(&MessageEvent::MessageQueued));
    // Only the send edge carries two events; every other edge carries one.
    let multi: Vec<(&MessageStatus, MessageCommand)> = MESSAGE_EDGES
        .iter()
        .filter(|e| e.events.len() > 1)
        .map(|e| (&e.from, e.command))
        .collect();
    assert_eq!(multi, vec![(&MessageStatus::Draft, MessageCommand::Send)]);
}

#[test]
fn the_two_source_fan_outs_really_have_two_edges_each() {
    // `QUEUED/ROUTED → EXPIRED` and `PROCESSING/ACKNOWLEDGED → REPLIED` are one
    // printed line each. Both commands must be legal from both sources.
    for command in [MessageCommand::Expire, MessageCommand::Reply] {
        let sources: Vec<MessageStatus> = MESSAGE_EDGES
            .iter()
            .filter(|e| e.command == command)
            .map(|e| e.from)
            .collect();
        assert_eq!(
            sources.len(),
            2,
            "{command} must be reachable from exactly the two sources §5 prints"
        );
        for s in sources {
            assert!(
                transition(s, command).is_ok(),
                "{s} --{command}--> must be legal"
            );
        }
    }
    assert_eq!(
        sources_of(MessageCommand::Expire),
        vec![MessageStatus::Queued, MessageStatus::Routed]
    );
    assert_eq!(
        sources_of(MessageCommand::Reply),
        vec![MessageStatus::Processing, MessageStatus::Acknowledged]
    );
}

fn sources_of(command: MessageCommand) -> Vec<MessageStatus> {
    MESSAGE_EDGES
        .iter()
        .filter(|e| e.command == command)
        .map(|e| e.from)
        .collect()
}

#[test]
fn replied_and_expired_absorb_and_failed_does_not() {
    // §5: "`EXPIRED` 不在原 Message 上重开" — a follow-up Message referencing the
    // old ID is a NEW Message. And "`FAILED` 是某投递周期终态，只有显式 retry
    // 才能回到 QUEUED" — FAILED is terminal for one delivery cycle but has exactly
    // one out-edge.
    for status in [MessageStatus::Replied, MessageStatus::Expired] {
        assert!(status.is_terminal(), "{status} must be terminal");
        for command in ALL_COMMANDS {
            assert!(
                transition(status, command).is_err(),
                "§5 gives {status} no out-edge, but {command} was accepted"
            );
        }
    }
    assert!(!MessageStatus::Failed.is_terminal());
    let failed_out: Vec<MessageCommand> = ALL_COMMANDS
        .iter()
        .copied()
        .filter(|c| transition(MessageStatus::Failed, *c).is_ok())
        .collect();
    assert_eq!(
        failed_out,
        vec![MessageCommand::RetryFailed],
        "§5 allows FAILED --retry--> QUEUED only"
    );
    assert_eq!(
        MESSAGE_EDGES
            .iter()
            .filter(|e| e.from.is_terminal())
            .count(),
        0
    );
}

#[test]
fn every_state_is_reachable_from_draft() {
    let mut reached = vec![MessageStatus::Draft];
    let mut changed = true;
    while changed {
        changed = false;
        for edge in MESSAGE_EDGES {
            if reached.contains(&edge.from) && !reached.contains(&edge.to) {
                reached.push(edge.to);
                changed = true;
            }
        }
    }
    for status in MessageStatus::ALL {
        assert!(
            reached.contains(&status),
            "{status} is unreachable from DRAFT — it has no inbound edge"
        );
    }
    // NEEDS_HUMAN must be enterable and leaveable, or the Router's dead end is
    // unreachable in practice.
    assert!(reached.contains(&MessageStatus::NeedsHuman));
    assert_eq!(
        sources_of(MessageCommand::Reassign),
        vec![MessageStatus::NeedsHuman]
    );
    assert_eq!(
        sources_of(MessageCommand::RequeueNeedsHuman),
        vec![MessageStatus::NeedsHuman]
    );
}

#[test]
fn the_delivery_path_is_ordered_and_gated() {
    // §5's happy path, walked edge by edge. The order matters: PROCESSING is only
    // reachable from ACKNOWLEDGED, and REPLIED from ACKNOWLEDGED or PROCESSING,
    // so a Message can never be marked replied before it was delivered.
    let path = [
        (
            MessageStatus::Draft,
            MessageCommand::Send,
            MessageStatus::Queued,
        ),
        (
            MessageStatus::Queued,
            MessageCommand::Route,
            MessageStatus::Routed,
        ),
        (
            MessageStatus::Routed,
            MessageCommand::Deliver,
            MessageStatus::Delivered,
        ),
        (
            MessageStatus::Delivered,
            MessageCommand::Acknowledge,
            MessageStatus::Acknowledged,
        ),
        (
            MessageStatus::Acknowledged,
            MessageCommand::Process,
            MessageStatus::Processing,
        ),
        (
            MessageStatus::Processing,
            MessageCommand::Reply,
            MessageStatus::Replied,
        ),
    ];
    for (from, command, to) in path {
        let t =
            transition(from, command).unwrap_or_else(|r| panic!("{from} --{command}--> {to}: {r}"));
        assert_eq!(t.to(), to);
    }
    // Skipping delivery is impossible: DRAFT cannot reach DELIVERED, and
    // ACKNOWLEDGED cannot be reached from anything but DELIVERED.
    assert!(transition(MessageStatus::Draft, MessageCommand::Deliver).is_err());
    assert!(transition(MessageStatus::Queued, MessageCommand::Acknowledge).is_err());
    assert!(transition(MessageStatus::Routed, MessageCommand::Acknowledge).is_err());
    // §5: ACKNOWLEDGED → PROCESSING is "显式声明处理；不自动 claim Task". DELIVERED
    // cannot jump straight to PROCESSING.
    assert_eq!(
        transition(MessageStatus::Delivered, MessageCommand::Process)
            .unwrap_err()
            .code(),
        MessageErrorCode::InvalidTransition
    );
    assert_eq!(
        transition(MessageStatus::Acknowledged, MessageCommand::Process)
            .expect("§5 has ACKNOWLEDGED → PROCESSING")
            .preconditions(),
        &[MessagePrecondition::ExplicitlyDeclaredProcessing]
    );
}

#[test]
fn failed_needs_human_and_expired_preserve_history() {
    // The node's acceptance criterion: "FAILED/EXPIRED/NEEDS_HUMAN 不丢历史".
    // At the graph level "不丢历史" means: each is enterable from the states §5
    // names, and the two that may be re-entered leave through an explicit command
    // rather than silently. EXPIRED may not be re-entered at all.
    for (from, command) in [
        (MessageStatus::Routed, MessageCommand::Fail),
        (MessageStatus::Queued, MessageCommand::NeedsHuman),
        (MessageStatus::Queued, MessageCommand::Expire),
        (MessageStatus::Routed, MessageCommand::Expire),
    ] {
        assert!(
            transition(from, command).is_ok(),
            "{from} --{command}--> must exist"
        );
    }
    // Each emits its own distinct event, so a subscriber can tell the three apart.
    let events = [
        (MessageCommand::Fail, MessageEvent::MessageFailed),
        (MessageCommand::NeedsHuman, MessageEvent::MessageNeedsHuman),
        (MessageCommand::Expire, MessageEvent::MessageExpired),
    ];
    let mut seen: Vec<MessageEvent> = Vec::new();
    for (command, event) in events {
        let edge = MESSAGE_EDGES
            .iter()
            .find(|e| e.command == command)
            .expect("edge must exist");
        assert_eq!(
            edge.events,
            &[event],
            "{command} must emit only its own event"
        );
        assert!(!seen.contains(&event), "{event} reused across commands");
        seen.push(event);
    }
    // Each out-degree is §5's own: EXPIRED 0, FAILED 1 (retry only), NEEDS_HUMAN
    // 2 (reassign and requeue are separate commands). Lumping FAILED in with
    // NEEDS_HUMAN would hide that FAILED has exactly one way out.
    for (status, expected) in [
        (MessageStatus::Expired, 0usize),
        (MessageStatus::Failed, 1),
        (MessageStatus::NeedsHuman, 2),
    ] {
        let out: Vec<MessageCommand> = ALL_COMMANDS
            .iter()
            .copied()
            .filter(|c| transition(status, *c).is_ok())
            .collect();
        assert_eq!(
            out.len(),
            expected,
            "{status} out-degree must be {expected}, got {out:?}"
        );
    }
}

#[test]
fn an_illegal_transition_reports_the_catalogue_code() {
    for status in MessageStatus::ALL {
        for command in ALL_COMMANDS {
            if let Err(rejection) = transition(status, command) {
                assert_eq!(
                    rejection.code(),
                    MessageErrorCode::InvalidTransition,
                    "{status} --{command}--> has no edge"
                );
                assert_eq!(rejection.current(), status);
                assert_eq!(rejection.command(), command);
            }
        }
    }
}

#[test]
fn a_legal_edge_exposes_its_own_rejection_code() {
    for edge in MESSAGE_EDGES {
        assert_eq!(
            rejection_code(edge.from, edge.command),
            Some(edge.rejection),
            "{} --{}--> lost its §5 rejection code",
            edge.from,
            edge.command
        );
    }
    for status in MessageStatus::ALL {
        for command in ALL_COMMANDS {
            if transition(status, command).is_err() {
                assert_eq!(rejection_code(status, command), None);
            }
        }
    }
}

#[test]
fn every_edge_declares_preconditions_and_events() {
    for edge in MESSAGE_EDGES {
        assert!(
            !edge.preconditions.is_empty(),
            "{} --{}--> has no preconditions; §5 lists at least one",
            edge.from,
            edge.command
        );
        assert!(
            !edge.events.is_empty(),
            "{} --{}--> has no events; §5 lists at least one",
            edge.from,
            edge.command
        );
        for precondition in edge.preconditions {
            assert!(!precondition.condition_key().is_empty());
        }
        for event in edge.events {
            assert!(
                event.as_str().starts_with("MESSAGE_"),
                "{} is not a MESSAGE_* event name",
                event.as_str()
            );
        }
    }
    // The only precondition §5 shares across rows is `TTL 有效`, which appears on
    // three different edges. Everything else is edge-specific.
    // Preconditions, counted rather than guessed: 15 edges carry 18
    // precondition slots, of which 14 are distinct. Exactly four keys are shared,
    // and each has a different reason — asserting only the total would hide a
    // fifth accidental duplicate.
    let slots: Vec<&str> = MESSAGE_EDGES
        .iter()
        .flat_map(|e| e.preconditions.iter())
        .map(|p| p.condition_key())
        .collect();
    assert_eq!(
        slots.len(),
        18,
        "15 edges, three of them with two preconditions"
    );
    let mut keys: Vec<&str> = slots.clone();
    keys.sort_unstable();
    keys.dedup();
    assert_eq!(
        keys.len(),
        14,
        "§5's edges name 14 distinct preconditions; got {keys:?}"
    );

    // Three shares are structural: the same printed §5 row became two edges, so
    // the two copies must carry the same precondition.
    for (command, key) in [
        (MessageCommand::Expire, "expires_at_reached"),
        (
            MessageCommand::Reply,
            "reply_in_same_thread_points_to_original",
        ),
    ] {
        let owners: Vec<&str> = MESSAGE_EDGES
            .iter()
            .filter(|e| e.command == command)
            .map(|e| e.preconditions[0].condition_key())
            .collect();
        assert_eq!(
            owners,
            vec![key; 2],
            "{command} fans out on the source state"
        );
    }
    // The third share is the `NEEDS_HUMAN -> QUEUED` row: Reassign and
    // RequeueNeedsHuman are two commands over one printed row, so they carry the
    // same 前置条件. The share is *between* the commands, not within one.
    let needs_human_keys: Vec<&str> = MESSAGE_EDGES
        .iter()
        .filter(|e| e.from == MessageStatus::NeedsHuman)
        .map(|e| e.preconditions[0].condition_key())
        .collect();
    assert_eq!(
        needs_human_keys,
        vec!["new_target_or_binding_valid_and_permission_passes_and_ttl_valid"; 2],
        "§5's NEEDS_HUMAN -> QUEUED row splits on its event cell, not its \
         前置条件, so both commands share one condition"
    );

    // The fourth share is §5's own reuse: "TTL 有效" appears in two different rows.
    // `ScheduleRetry` does not use this key — §5 bundles TTL into
    // "retryable 失败、未达上限、TTL 有效" as one condition.
    let ttl_owners: Vec<String> = MESSAGE_EDGES
        .iter()
        .filter(|e| e.preconditions.contains(&MessagePrecondition::TtlValid))
        .map(|e| format!("{:?}", e.command))
        .collect();
    assert_eq!(ttl_owners, vec!["Route", "RetryFailed"]);
    assert!(
        MESSAGE_EDGES.iter().any(|e| e
            .preconditions
            .contains(&MessagePrecondition::RetryableFailureWithinLimitAndTtlValid)),
        "§5's retry row carries TTL inside its own bundled condition"
    );

    // Counting the creation row adds exactly one more distinct key.
    let with_creation: std::collections::BTreeSet<&str> = keys
        .iter()
        .copied()
        .chain(
            CREATE_DRAFT
                .preconditions()
                .iter()
                .map(|p| p.condition_key()),
        )
        .collect();
    assert_eq!(
        with_creation.len(),
        15,
        "counting creation, 15 distinct keys"
    );
}

#[test]
fn no_edge_is_a_self_loop() {
    for edge in MESSAGE_EDGES {
        assert_ne!(
            edge.to, edge.from,
            "{} --{}--> is a self-loop",
            edge.from, edge.command
        );
        let t = transition(edge.from, edge.command).expect("table row must apply");
        assert!(MessageStatus::ALL.contains(&t.to()));
    }
}

#[test]
fn creation_supports_exactly_draft() {
    assert_eq!(MESSAGE_CREATE_TARGETS.len(), 1);
    assert_eq!(MESSAGE_CREATE_TARGETS[0], MessageStatus::Draft);
    let creation = message_creation_target(MessageStatus::Draft).expect("创建 → DRAFT is legal");
    assert_eq!(creation.initial(), MessageStatus::Draft);
    assert_eq!(creation.rejection(), MessageErrorCode::PermissionDenied);
    assert_eq!(creation.events(), &[MessageEvent::MessageDrafted]);
    assert_eq!(
        creation.preconditions(),
        &[MessagePrecondition::SenderMaySendToTargetRole],
        "§5: sender 有权向目标 Role 发送"
    );
    for status in [
        MessageStatus::Queued,
        MessageStatus::Routed,
        MessageStatus::Delivered,
        MessageStatus::Acknowledged,
        MessageStatus::Processing,
        MessageStatus::Replied,
        MessageStatus::Failed,
        MessageStatus::Expired,
        MessageStatus::NeedsHuman,
    ] {
        assert!(
            message_creation_target(status).is_none(),
            "§5 has no 创建 --> {status} row"
        );
    }
    // No edge lands in DRAFT either: content is never un-sent into a draft.
    assert_eq!(
        MESSAGE_EDGES
            .iter()
            .filter(|e| e.to == MessageStatus::Draft)
            .count(),
        0
    );
    assert_eq!(
        message_creation_target(MessageStatus::Draft),
        Some(&CREATE_DRAFT)
    );
}

#[test]
fn error_codes_match_the_frozen_catalogue() {
    let expected = [
        (
            MessageErrorCode::InvalidTransition,
            "DOMAIN_INVALID_TRANSITION",
            false,
        ),
        (
            MessageErrorCode::PermissionDenied,
            "AUTH_PERMISSION_DENIED",
            false,
        ),
        (
            MessageErrorCode::HumanRequired,
            "AUTH_HUMAN_REQUIRED",
            false,
        ),
        (
            MessageErrorCode::ValidationInvalidField,
            "VALIDATION_INVALID_FIELD",
            false,
        ),
        (
            MessageErrorCode::BindingNotFound,
            "BINDING_NOT_FOUND",
            false,
        ),
        (
            MessageErrorCode::BindingStaleVersion,
            "BINDING_STALE_VERSION",
            false,
        ),
        (MessageErrorCode::TtlExpired, "MESSAGE_TTL_EXPIRED", false),
        (
            MessageErrorCode::ThreadMismatch,
            "MESSAGE_THREAD_MISMATCH",
            false,
        ),
        (
            MessageErrorCode::PreconditionFailed,
            "DOMAIN_PRECONDITION_FAILED",
            false,
        ),
    ];
    for (code, name, retryable) in expected {
        assert_eq!(code.as_str(), name);
        assert_eq!(
            code.is_retryable(),
            retryable,
            "{name} retryable flag drifted"
        );
    }
    // Not one §5 code is retryable in the frozen catalogue. That is a fact about
    // the catalogue, not an omission: §5's *edges* are still retryable.
    assert!(
        expected.iter().all(|(_, _, r)| !*r),
        "the catalogue marks every code §5 can raise as 否"
    );
    // The catalogue also holds MESSAGE_DELIVERY_FAILED and MESSAGE_REASSIGN_DENIED,
    // which §5's table does not reference. Do not "use them up".
    let wires: Vec<&str> = expected.iter().map(|(_, n, _)| *n).collect();
    assert!(!wires.contains(&"MESSAGE_DELIVERY_FAILED"));
    assert!(!wires.contains(&"MESSAGE_REASSIGN_DENIED"));
}

#[test]
fn serde_agrees_with_as_str_for_every_code_and_event() {
    let codes = [
        (
            MessageErrorCode::InvalidTransition,
            "DOMAIN_INVALID_TRANSITION",
        ),
        (MessageErrorCode::PermissionDenied, "AUTH_PERMISSION_DENIED"),
        (MessageErrorCode::HumanRequired, "AUTH_HUMAN_REQUIRED"),
        (
            MessageErrorCode::ValidationInvalidField,
            "VALIDATION_INVALID_FIELD",
        ),
        (MessageErrorCode::BindingNotFound, "BINDING_NOT_FOUND"),
        (
            MessageErrorCode::BindingStaleVersion,
            "BINDING_STALE_VERSION",
        ),
        (MessageErrorCode::TtlExpired, "MESSAGE_TTL_EXPIRED"),
        (MessageErrorCode::ThreadMismatch, "MESSAGE_THREAD_MISMATCH"),
        (
            MessageErrorCode::PreconditionFailed,
            "DOMAIN_PRECONDITION_FAILED",
        ),
    ];
    for (code, name) in codes {
        assert_eq!(code.as_str(), name);
        assert_eq!(
            serde_json::to_string(&code).unwrap(),
            format!("\"{name}\""),
            "serde and as_str disagree for {code:?}"
        );
    }
    for status in MessageStatus::ALL {
        assert_eq!(
            serde_json::to_string(&status).unwrap(),
            format!("\"{}\"", status.as_str()),
            "serde and as_str disagree for {status:?}"
        );
    }
    for edge in MESSAGE_EDGES {
        for event in edge.events {
            assert_eq!(
                serde_json::to_string(event).unwrap(),
                format!("\"{}\"", event.as_str()),
                "serde and as_str disagree for {event:?}"
            );
        }
    }
    for event in CREATE_DRAFT.events() {
        assert_eq!(
            serde_json::to_string(event).unwrap(),
            format!("\"{}\"", event.as_str()),
            "serde and as_str disagree for creation event {event:?}"
        );
    }
    // All fourteen §5 event names are covered: 13 edges carry one each, the send
    // edge carries two, and MESSAGE_REQUEUED is shared by two edges.
    let mut names: Vec<&str> = MESSAGE_EDGES
        .iter()
        .flat_map(|e| e.events.iter())
        .map(|e| e.as_str())
        .collect();
    names.push(CREATE_DRAFT.events()[0].as_str());
    names.sort_unstable();
    names.dedup();
    assert_eq!(
        names.len(),
        14,
        "§5 freezes exactly 14 MESSAGE_* event names"
    );
    for name in names {
        assert!(
            name.starts_with("MESSAGE_"),
            "{name} is not a MESSAGE_* name"
        );
    }
}

#[test]
fn the_machine_is_pure_and_deterministic() {
    for status in MessageStatus::ALL {
        for command in ALL_COMMANDS {
            let a = transition(status, command);
            let b = transition(status, command);
            match (a, b) {
                (Ok(x), Ok(y)) => assert_eq!(x, y, "{status} --{command}--> unstable"),
                (Err(x), Err(y)) => assert_eq!(x, y, "{status} --{command}--> unstable"),
                _ => unreachable!("same input produced Ok once and Err once"),
            }
        }
    }
}
