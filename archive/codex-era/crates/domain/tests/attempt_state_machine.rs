//! Conformance tests for the DeliveryAttempt state machine.
//!
//! §5 gives the attempt chain one sentence and nothing else: no Event column, no
//! 拒绝码 column, no Actor, no 前置条件. Most of this file is therefore about
//! **absence** — that nothing was invented to fill those columns, because filling
//! them would be inventing protocol.

use teamspace_domain::{
    ATTEMPT_CREATE_TARGETS, ATTEMPT_EDGES, AttemptCommand, AttemptStatus,
    CREATE_PENDING_ATTEMPT as CREATE_PENDING, MessageErrorCode, attempt_creation_target,
    attempt_transition as transition,
};

/// Every row of the §5 chain: `PENDING → IN_FLIGHT → SUCCEEDED/FAILED/ABANDONED`.
const EXPECTED_ROWS: &[(AttemptStatus, AttemptCommand, AttemptStatus)] = &[
    (
        AttemptStatus::Pending,
        AttemptCommand::Start,
        AttemptStatus::InFlight,
    ),
    (
        AttemptStatus::InFlight,
        AttemptCommand::Succeed,
        AttemptStatus::Succeeded,
    ),
    (
        AttemptStatus::InFlight,
        AttemptCommand::Fail,
        AttemptStatus::Failed,
    ),
    (
        AttemptStatus::InFlight,
        AttemptCommand::Abandon,
        AttemptStatus::Abandoned,
    ),
];

#[test]
fn the_chain_holds_exactly_the_specified_rows() {
    assert_eq!(
        ATTEMPT_EDGES.len(),
        EXPECTED_ROWS.len(),
        "§5's chain is 4 edges; the table holds {}",
        ATTEMPT_EDGES.len()
    );
    for (from, command, to) in EXPECTED_ROWS {
        let t = transition(*from, *command)
            .unwrap_or_else(|r| panic!("§5 requires {from} --{command}--> {to}: {r}"));
        assert_eq!(t.from(), *from);
        assert_eq!(t.to(), *to);
        assert_eq!(t.command(), *command);
    }
}

#[test]
fn the_chain_holds_no_edge_the_specification_omits() {
    for status in AttemptStatus::ALL {
        for command in AttemptCommand::ALL {
            let in_table = ATTEMPT_EDGES
                .iter()
                .any(|e| e.from == status && e.command == command);
            let in_spec = EXPECTED_ROWS
                .iter()
                .any(|(from, cmd, _)| *from == status && *cmd == command);
            assert_eq!(
                in_table, in_spec,
                "{status} --{command}--> table={in_table} spec={in_spec}"
            );
        }
    }
}

#[test]
fn every_edge_declares_no_events_and_no_preconditions() {
    // §5 gives the attempt chain neither column, and records every attempt
    // observation on the **Message** event instead: "attempt SUCCEEDED" is part of
    // `MESSAGE_DELIVERED`, "当前 attempt FAILED" is part of
    // `MESSAGE_RETRY_SCHEDULED` and `MESSAGE_FAILED`, "终止待处理 attempt" is part
    // of `MESSAGE_EXPIRED`.
    //
    // Inventing `ATTEMPT_STARTED`-style names, or copying Message preconditions
    // onto attempt edges, would be inventing protocol. This test is the tripwire:
    // a future reader who "helpfully" fills them in sees it fail.
    for edge in ATTEMPT_EDGES {
        assert!(
            edge.events.is_empty(),
            "{} --{}--> invented events {:?}; §5 records attempts on the Message event",
            edge.from,
            edge.command,
            edge.events
        );
        assert!(
            edge.preconditions.is_empty(),
            "{} --{}--> invented preconditions {:?}; §5 gives the attempt chain no \
             前置条件 column",
            edge.from,
            edge.command,
            edge.preconditions
        );
    }
    // And the decided transition reports the same emptiness through its accessors.
    for (from, command, _) in EXPECTED_ROWS {
        let t = transition(*from, *command).expect("chain row must apply");
        assert!(t.events().is_empty());
        assert!(t.preconditions().is_empty());
    }
}

#[test]
fn every_rejection_is_the_invalid_transition_code() {
    // §5 attaches no code to any attempt edge, so the only code an attempt
    // transition can produce is "there is no such edge". That needs no derivation:
    // the catalogue's DOMAIN_INVALID_TRANSITION applies unchanged, and it is the
    // same string the Message machine uses, so the crate has one spelling.
    for edge in ATTEMPT_EDGES {
        assert_eq!(edge.rejection, MessageErrorCode::InvalidTransition);
        assert_eq!(edge.rejection.as_str(), "DOMAIN_INVALID_TRANSITION");
    }
    for status in AttemptStatus::ALL {
        for command in AttemptCommand::ALL {
            if let Err(r) = transition(status, command) {
                assert_eq!(r.code(), MessageErrorCode::InvalidTransition);
                assert_eq!(r.current(), status);
                assert_eq!(r.command(), command);
            }
        }
    }
}

#[test]
fn a_failed_attempt_is_never_reopened() {
    // §5: a retry is "新 attempt，保留旧 attempts", and "当前 attempt FAILED" is a
    // side effect of scheduling. So `FAILED` has no out-edge — retrying creates
    // another attempt rather than rewinding this one. That is what makes the
    // attempt history immutable, which §5 requires.
    for status in [
        AttemptStatus::Succeeded,
        AttemptStatus::Failed,
        AttemptStatus::Abandoned,
    ] {
        assert!(status.is_terminal(), "{status} must be terminal");
        for command in AttemptCommand::ALL {
            assert!(
                transition(status, command).is_err(),
                "§5 never rewinds a {status}, but {command} was accepted"
            );
        }
    }
    assert_eq!(
        ATTEMPT_EDGES
            .iter()
            .filter(|e| e.from.is_terminal())
            .count(),
        0
    );
    // In particular there is no FAILED → PENDING edge, and no PENDING → anything
    // other than IN_FLIGHT.
    assert!(transition(AttemptStatus::Failed, AttemptCommand::Start).is_err());
    assert!(transition(AttemptStatus::Failed, AttemptCommand::Fail).is_err());
    let pending_out: Vec<AttemptCommand> = AttemptCommand::ALL
        .iter()
        .copied()
        .filter(|c| transition(AttemptStatus::Pending, *c).is_ok())
        .collect();
    assert_eq!(pending_out, vec![AttemptCommand::Start]);
}

#[test]
fn the_three_terminal_outcomes_are_distinguishable() {
    // `SUCCEEDED/FAILED/ABANDONED` is one printed chain step and three edges.
    // Collapsing them would lose the distinction §5's Message rows rely on:
    // "attempt SUCCEEDED" continues the Message, "attempt FAILED" schedules a
    // retry or fails the Message, and "终止待处理 attempt" expires it.
    let mut seen = Vec::new();
    for (command, target) in [
        (AttemptCommand::Succeed, AttemptStatus::Succeeded),
        (AttemptCommand::Fail, AttemptStatus::Failed),
        (AttemptCommand::Abandon, AttemptStatus::Abandoned),
    ] {
        let t = transition(AttemptStatus::InFlight, command)
            .unwrap_or_else(|r| panic!("§5 IN_FLIGHT --{command}--> {target}: {r}"));
        assert_eq!(t.to(), target);
        assert!(!seen.contains(&target), "{target} reachable twice");
        seen.push(target);
    }
    assert_eq!(seen.len(), 3);
    // All three leave IN_FLIGHT, and it has exactly three out-edges.
    let in_flight_out: Vec<AttemptCommand> = AttemptCommand::ALL
        .iter()
        .copied()
        .filter(|c| transition(AttemptStatus::InFlight, *c).is_ok())
        .collect();
    assert_eq!(
        in_flight_out,
        vec![
            AttemptCommand::Succeed,
            AttemptCommand::Fail,
            AttemptCommand::Abandon
        ]
    );
}

#[test]
fn creation_supports_exactly_pending() {
    // §5 states the chain in one sentence and gives no creation row, so this is a
    // **derived** target: an attempt must be recorded before it can start, and
    // `PENDING` is the chain's only zero-in-degree state.
    assert_eq!(ATTEMPT_CREATE_TARGETS.len(), 1);
    assert_eq!(ATTEMPT_CREATE_TARGETS[0], AttemptStatus::Pending);
    let creation = attempt_creation_target(AttemptStatus::Pending).expect("创建 → PENDING");
    assert_eq!(creation.initial(), AttemptStatus::Pending);
    assert_eq!(creation.rejection(), MessageErrorCode::InvalidTransition);
    for status in [
        AttemptStatus::InFlight,
        AttemptStatus::Succeeded,
        AttemptStatus::Failed,
        AttemptStatus::Abandoned,
    ] {
        assert!(
            attempt_creation_target(status).is_none(),
            "§5 has no 创建 --> {status}"
        );
    }
    assert_eq!(
        ATTEMPT_EDGES
            .iter()
            .filter(|e| e.to == AttemptStatus::Pending)
            .count(),
        0,
        "no edge lands in PENDING; a retry is a new attempt"
    );
    assert_eq!(
        attempt_creation_target(AttemptStatus::Pending),
        Some(&CREATE_PENDING)
    );
}

#[test]
fn the_chain_is_reachable_from_creation() {
    let mut reached = vec![AttemptStatus::Pending];
    let mut changed = true;
    while changed {
        changed = false;
        for edge in ATTEMPT_EDGES {
            if reached.contains(&edge.from) && !reached.contains(&edge.to) {
                reached.push(edge.to);
                changed = true;
            }
        }
    }
    for status in AttemptStatus::ALL {
        assert!(
            reached.contains(&status),
            "{status} is unreachable from PENDING"
        );
    }
}

#[test]
fn no_edge_is_a_self_loop_and_wire_names_agree_with_serde() {
    for edge in ATTEMPT_EDGES {
        assert_ne!(
            edge.to, edge.from,
            "{} --{}--> is a self-loop",
            edge.from, edge.command
        );
        let t = transition(edge.from, edge.command).expect("chain row must apply");
        assert!(AttemptStatus::ALL.contains(&t.to()));
    }
    for status in AttemptStatus::ALL {
        assert_eq!(
            serde_json::to_string(&status).unwrap(),
            format!("\"{}\"", status.as_str()),
            "serde and as_str disagree for {status:?}"
        );
    }
}

#[test]
fn the_machine_is_pure_and_deterministic() {
    for status in AttemptStatus::ALL {
        for command in AttemptCommand::ALL {
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
