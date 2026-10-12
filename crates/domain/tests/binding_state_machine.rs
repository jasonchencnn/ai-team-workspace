//! Table-driven conformance tests for the Binding state machine.
//!
//! Same two-layer shape as the Task and Run suites: `EXPECTED_ROWS` is a
//! hand-transcription of `STATE_MACHINES.md` §4's table, and the tests assert it
//! against [`BINDING_EDGES`] in both directions, so a row dropped from the table
//! fails and a row invented into the table fails too.
//!
//! Two §4 facts get dedicated tests because a purely mechanical transcription
//! would miss both:
//!
//! * "不创建 `UNBOUND` 伪记录" — the negative space is the requirement.
//! * `ROLE_BOUND`/`ROLE_REBOUND` is one printed row with two events, split into
//!   `Activate` and `Rebind` so the graph never has to pick.

use teamspace_domain::{
    BINDING_CREATE_TARGETS, BINDING_EDGES, BindingCommand, BindingErrorCode, BindingEvent,
    BindingPrecondition, BindingStatus, CREATE_PENDING_CHECK, binding_creation_target,
    binding_rejection_code as rejection_code, binding_transition as transition,
};

/// Every non-creation row of §4's table: (from, command, to, events, rejection).
///
/// §4 prints seven rows. Six transcribe one-for-one; `PENDING_CHECK → ACTIVE`
/// appears twice because that row's Event cell is `ROLE_BOUND`/`ROLE_REBOUND`
/// and each event needs its own command for `transition` to stay total.
type Row = (
    BindingStatus,
    BindingCommand,
    BindingStatus,
    &'static [BindingEvent],
    BindingErrorCode,
);

const EXPECTED_ROWS: &[Row] = &[
    (
        BindingStatus::PendingCheck,
        BindingCommand::Activate,
        BindingStatus::Active,
        &[BindingEvent::RoleBound],
        BindingErrorCode::AdapterIncompatible,
    ),
    (
        BindingStatus::PendingCheck,
        BindingCommand::Rebind,
        BindingStatus::Active,
        &[BindingEvent::RoleRebound],
        BindingErrorCode::AdapterIncompatible,
    ),
    (
        BindingStatus::PendingCheck,
        BindingCommand::FailCheck,
        BindingStatus::Failed,
        &[BindingEvent::BindingChangeFailed],
        BindingErrorCode::PreconditionFailed,
    ),
    (
        BindingStatus::Active,
        BindingCommand::Drain,
        BindingStatus::Draining,
        &[BindingEvent::BindingDraining],
        BindingErrorCode::HumanRequired,
    ),
    (
        BindingStatus::Draining,
        BindingCommand::Revoke,
        BindingStatus::Revoked,
        &[BindingEvent::RoleUnbound],
        BindingErrorCode::RunActiveExists,
    ),
    (
        BindingStatus::Active,
        BindingCommand::Revoke,
        BindingStatus::Revoked,
        &[BindingEvent::RoleUnbound],
        BindingErrorCode::RunCancelNotConfirmed,
    ),
    (
        BindingStatus::Draining,
        BindingCommand::UndoDrain,
        BindingStatus::Active,
        &[BindingEvent::BindingChangeCancelled],
        BindingErrorCode::BindingStaleVersion,
    ),
];

const ALL_COMMANDS: [BindingCommand; 6] = BindingCommand::ALL;

#[test]
fn the_table_holds_exactly_the_specified_rows() {
    assert_eq!(
        BINDING_EDGES.len(),
        EXPECTED_ROWS.len(),
        "§4 has 6 unambiguous lifecycle rows plus the split \
         PENDING_CHECK→ACTIVE row, so 7 edges; the table holds {}",
        BINDING_EDGES.len()
    );
    for (from, command, to, events, rejection) in EXPECTED_ROWS {
        let decided = transition(*from, *command);
        let t = decided.unwrap_or_else(|r| {
            panic!("§4 requires {from} --{command}--> {to}, but rejected: {r}")
        });
        assert_eq!(t.from(), *from);
        assert_eq!(t.to(), *to, "§4 destination for {from} --{command}-->");
        assert_eq!(t.command(), *command);
        assert_eq!(
            t.events(),
            *events,
            "§4 Event cell for {from} --{command}-->"
        );
        assert_eq!(
            rejection_code(*from, *command),
            Some(*rejection),
            "{from} --{command}--> lost its §4 拒绝码"
        );
    }
}

#[test]
fn the_table_holds_no_edge_the_specification_omits() {
    for status in BindingStatus::ALL {
        for command in ALL_COMMANDS {
            let in_table = BINDING_EDGES
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
    // `transition` depends on this. `Revoke` appears on two rows, but from
    // different states, so no (from, command) pair repeats.
    for (i, a) in BINDING_EDGES.iter().enumerate() {
        for b in &BINDING_EDGES[i + 1..] {
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
    for status in BindingStatus::ALL {
        for command in ALL_COMMANDS {
            let matches = BINDING_EDGES
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
fn there_is_no_unbound_state() {
    // §4: "未绑定由'无 ACTIVE Binding'表达，不创建 `UNBOUND` 伪记录".
    // The enum cannot be constructed with that name, so the negative space is
    // what gets asserted. A sixth state would also break the "no ACTIVE" query
    // the application layer uses to detect an unbound Role.
    let names: Vec<&str> = BindingStatus::ALL.iter().map(|s| s.as_str()).collect();
    assert!(
        !names.contains(&"UNBOUND"),
        "§4 forbids an UNBOUND pseudo-record, found {names:?}"
    );
    assert_eq!(names.len(), 5);
    // And "unbound" is expressible: a Role with no ACTIVE version, which is the
    // absence of a row rather than a state of one.
    let has_active: fn(&[&BindingStatus]) -> bool =
        |versions| versions.contains(&&BindingStatus::Active);
    assert!(!has_active(&[&BindingStatus::Revoked]));
    assert!(has_active(&[
        &BindingStatus::Revoked,
        &BindingStatus::Active
    ]));
}

#[test]
fn every_state_is_reachable_from_pending_check() {
    let mut reached = vec![BindingStatus::PendingCheck];
    let mut changed = true;
    while changed {
        changed = false;
        for edge in BINDING_EDGES {
            if reached.contains(&edge.from) && !reached.contains(&edge.to) {
                reached.push(edge.to);
                changed = true;
            }
        }
    }
    for status in BindingStatus::ALL {
        assert!(
            reached.contains(&status),
            "{status} is unreachable from PENDING_CHECK — it has no inbound edge"
        );
    }
    // §4 strategy 3: "Role 可暂时无 ACTIVE Binding" — reachable states that are
    // not ACTIVE must exist, or that sentence could not hold.
    assert!(reached.contains(&BindingStatus::Revoked));
    assert!(reached.contains(&BindingStatus::Failed));
}

#[test]
fn revoked_and_failed_absorb() {
    // §4 gives neither state an out-edge. Derived from the table rather than a
    // 终态 sentence, because §4 does not use that word.
    for status in [BindingStatus::Revoked, BindingStatus::Failed] {
        assert!(status.is_terminal(), "{status} must be terminal");
        for command in ALL_COMMANDS {
            assert!(
                transition(status, command).is_err(),
                "§4 gives {status} no out-edge, but {command} was accepted"
            );
        }
    }
    assert_eq!(
        BINDING_EDGES
            .iter()
            .filter(|e| e.from.is_terminal())
            .count(),
        0
    );
}

#[test]
fn draining_is_not_terminal() {
    // The asymmetry is the point of the drain: a DRAINING version can still go
    // back to ACTIVE if the Human calls the swap off. Treating DRAINING as
    // terminal would strand every Role mid-swap with no way back.
    assert!(!BindingStatus::Draining.is_terminal());
    let out: Vec<BindingStatus> = BINDING_EDGES
        .iter()
        .filter(|e| e.from == BindingStatus::Draining)
        .map(|e| e.to)
        .collect();
    assert_eq!(
        out,
        vec![BindingStatus::Revoked, BindingStatus::Active],
        "§4 gives DRAINING exactly two destinations"
    );
    assert!(transition(BindingStatus::Draining, BindingCommand::UndoDrain).is_ok());
    assert!(transition(BindingStatus::Draining, BindingCommand::Revoke).is_ok());
}

#[test]
fn activate_and_rebind_share_a_destination_but_not_an_event() {
    // §4's PENDING_CHECK → ACTIVE row prints `ROLE_BOUND`/`ROLE_REBOUND`. If the
    // two collapsed into one edge the event would be a caller-side choice, and
    // the wrong pick would tell every subscriber the Role was bound for the
    // first time when it was actually a re-bind.
    let a = transition(BindingStatus::PendingCheck, BindingCommand::Activate)
        .expect("§4 PENDING_CHECK → ACTIVE");
    let r = transition(BindingStatus::PendingCheck, BindingCommand::Rebind)
        .expect("§4 PENDING_CHECK → ACTIVE");
    assert_eq!(a.to(), r.to());
    assert_eq!(a.to(), BindingStatus::Active);
    assert_eq!(a.events(), &[BindingEvent::RoleBound]);
    assert_eq!(r.events(), &[BindingEvent::RoleRebound]);
    // Same preconditions and same rejection code: only the event differs.
    assert_eq!(a.preconditions(), r.preconditions());
    assert_eq!(
        rejection_code(BindingStatus::PendingCheck, BindingCommand::Activate),
        rejection_code(BindingStatus::PendingCheck, BindingCommand::Rebind)
    );
    // The events must actually differ — that is the whole reason these are two
    // edges rather than one edge with a conditional event.
    assert_ne!(a.events(), r.events());
}

#[test]
fn revoke_carries_a_different_code_from_each_source() {
    // §4 rejects the two revocations for genuinely different reasons: a drained
    // version still has live Runs, while an ACTIVE one needs cancel+fence to
    // have completed. Collapsing them would report the wrong remediation.
    assert_eq!(
        rejection_code(BindingStatus::Draining, BindingCommand::Revoke),
        Some(BindingErrorCode::RunActiveExists)
    );
    assert_eq!(
        rejection_code(BindingStatus::Active, BindingCommand::Revoke),
        Some(BindingErrorCode::RunCancelNotConfirmed)
    );
    assert!(!BindingErrorCode::RunActiveExists.is_retryable());
    assert!(BindingErrorCode::RunCancelNotConfirmed.is_retryable());
    // Both emit ROLE_UNBOUND.
    for from in [BindingStatus::Active, BindingStatus::Draining] {
        let t = transition(from, BindingCommand::Revoke).expect("§4 revokes");
        assert_eq!(t.events(), &[BindingEvent::RoleUnbound]);
    }
}

#[test]
fn an_illegal_transition_reports_the_catalogue_code() {
    for status in BindingStatus::ALL {
        for command in ALL_COMMANDS {
            if let Err(rejection) = transition(status, command) {
                assert_eq!(
                    rejection.code(),
                    BindingErrorCode::InvalidTransition,
                    "{status} --{command}--> has no edge, so it must be \
                     DOMAIN_INVALID_TRANSITION"
                );
                assert_eq!(rejection.current(), status);
                assert_eq!(rejection.command(), command);
            }
        }
    }
}

#[test]
fn a_legal_edge_exposes_its_own_rejection_code() {
    for edge in BINDING_EDGES {
        assert_eq!(
            rejection_code(edge.from, edge.command),
            Some(edge.rejection),
            "{} --{}--> lost its §4 rejection code",
            edge.from,
            edge.command
        );
        assert_ne!(
            edge.rejection,
            BindingErrorCode::InvalidTransition,
            "{} --{}--> is a real edge, so it must not carry the generic code",
            edge.from,
            edge.command
        );
    }
    for status in BindingStatus::ALL {
        for command in ALL_COMMANDS {
            if transition(status, command).is_err() {
                assert_eq!(rejection_code(status, command), None);
            }
        }
    }
}

#[test]
fn every_edge_declares_preconditions_and_events() {
    for edge in BINDING_EDGES {
        assert!(
            !edge.preconditions.is_empty(),
            "{} --{}--> has no preconditions; §4 lists at least one",
            edge.from,
            edge.command
        );
        assert!(
            !edge.events.is_empty(),
            "{} --{}--> has no events; §4 lists at least one",
            edge.from,
            edge.command
        );
        for precondition in edge.preconditions {
            assert!(!precondition.condition_key().is_empty());
        }
        for event in edge.events {
            let name = event.as_str();
            assert!(
                name.starts_with("BINDING_") || name.starts_with("ROLE_"),
                "{name} is neither a BINDING_* nor a ROLE_* event name"
            );
        }
    }
    // Every §4 row is transcribed with its own preconditions. Two of them are
    // shared, by specification: `Activate` and `Rebind` are the same check, and
    // only their event differs. The other two live on the creation row, which is
    // not in BINDING_EDGES. So 10 across 7 edges, 12 counting creation.
    let mut keys: Vec<&str> = BINDING_EDGES
        .iter()
        .flat_map(|e| e.preconditions.iter())
        .map(|p| p.condition_key())
        .collect();
    keys.sort_unstable();
    keys.dedup();
    assert_eq!(
        keys.len(),
        10,
        "§4's 7 edges name 10 distinct preconditions; got {keys:?}"
    );
    let mut with_creation = CREATE_PENDING_CHECK
        .preconditions()
        .iter()
        .map(|p| p.condition_key())
        .collect::<Vec<_>>();
    with_creation.extend(keys.iter().copied());
    with_creation.sort_unstable();
    with_creation.dedup();
    assert_eq!(
        with_creation.len(),
        12,
        "counting the creation row, §4 names 12 distinct preconditions"
    );
}

#[test]
fn no_edge_is_a_self_loop() {
    for edge in BINDING_EDGES {
        assert_ne!(
            edge.to, edge.from,
            "{} --{}--> is a self-loop",
            edge.from, edge.command
        );
        let t = transition(edge.from, edge.command).expect("table row must apply");
        assert!(BindingStatus::ALL.contains(&t.to()));
    }
}

#[test]
fn creation_supports_exactly_pending_check() {
    assert_eq!(BINDING_CREATE_TARGETS.len(), 1);
    assert_eq!(BINDING_CREATE_TARGETS[0], BindingStatus::PendingCheck);
    let creation = binding_creation_target(BindingStatus::PendingCheck)
        .expect("创建 → PENDING_CHECK is legal");
    assert_eq!(creation.initial(), BindingStatus::PendingCheck);
    assert_eq!(
        creation.rejection(),
        BindingErrorCode::BindingChangeInProgress
    );
    assert_eq!(creation.events(), &[BindingEvent::BindingChangeRequested]);
    assert_eq!(
        creation.preconditions(),
        &[
            BindingPrecondition::RoleAndHarnessActive,
            BindingPrecondition::NoConflictingChange,
        ],
        "§4: Role/Harness ACTIVE；无冲突变更"
    );
    for status in [
        BindingStatus::Active,
        BindingStatus::Draining,
        BindingStatus::Revoked,
        BindingStatus::Failed,
    ] {
        assert!(
            binding_creation_target(status).is_none(),
            "§4 has no 创建 --> {status} row"
        );
    }
    // No edge may land in PENDING_CHECK either: a version is never pushed back
    // into a check, it is superseded by a new version.
    assert_eq!(
        BINDING_EDGES
            .iter()
            .filter(|e| e.to == BindingStatus::PendingCheck)
            .count(),
        0
    );
    assert_eq!(
        binding_creation_target(BindingStatus::PendingCheck),
        Some(&CREATE_PENDING_CHECK)
    );
}

#[test]
fn the_swap_strategies_are_all_representable() {
    // §4's three 换绑策略 are narrative, not table rows, but each one is only
    // executable if the edges it needs exist. If a strategy's path has a hole,
    // the strategy is unimplementable and no table-level test would say so.
    let path = |from: BindingStatus, cmd: BindingCommand| transition(from, cmd).map(|t| t.to());
    // 1. 完成后换绑: ACTIVE --drain--> DRAINING --revoke--> REVOKED, while the
    //    replacement goes PENDING_CHECK --activate--> ACTIVE.
    assert_eq!(
        path(BindingStatus::Active, BindingCommand::Drain),
        Ok(BindingStatus::Draining)
    );
    assert_eq!(
        path(BindingStatus::Draining, BindingCommand::Revoke),
        Ok(BindingStatus::Revoked)
    );
    assert_eq!(
        path(BindingStatus::PendingCheck, BindingCommand::Activate),
        Ok(BindingStatus::Active)
    );
    // 2. 取消并重开: ACTIVE --revoke--> REVOKED directly, no drain.
    assert_eq!(
        path(BindingStatus::Active, BindingCommand::Revoke),
        Ok(BindingStatus::Revoked)
    );
    // 3. 人工接管: same direct revoke, then the Role legitimately has no ACTIVE
    //    version until a later binding is checked.
    assert_eq!(
        path(BindingStatus::Active, BindingCommand::Revoke),
        Ok(BindingStatus::Revoked)
    );
    assert!(binding_creation_target(BindingStatus::PendingCheck).is_some());
    // And the escape hatch: a swap called off before it takes effect.
    assert_eq!(
        path(BindingStatus::Draining, BindingCommand::UndoDrain),
        Ok(BindingStatus::Active)
    );
}

#[test]
fn error_codes_match_the_frozen_catalogue() {
    let expected = [
        (
            BindingErrorCode::InvalidTransition,
            "DOMAIN_INVALID_TRANSITION",
            false,
        ),
        (
            BindingErrorCode::BindingChangeInProgress,
            "BINDING_CHANGE_IN_PROGRESS",
            true,
        ),
        (
            BindingErrorCode::AdapterIncompatible,
            "ADAPTER_INCOMPATIBLE",
            false,
        ),
        (
            BindingErrorCode::HumanRequired,
            "AUTH_HUMAN_REQUIRED",
            false,
        ),
        (
            BindingErrorCode::RunActiveExists,
            "RUN_ACTIVE_EXISTS",
            false,
        ),
        (
            BindingErrorCode::RunCancelNotConfirmed,
            "RUN_CANCEL_NOT_CONFIRMED",
            true,
        ),
        (
            BindingErrorCode::BindingStaleVersion,
            "BINDING_STALE_VERSION",
            false,
        ),
        (
            BindingErrorCode::PreconditionFailed,
            "DOMAIN_PRECONDITION_FAILED",
            false,
        ),
    ];
    for (code, name, retryable) in expected {
        assert_eq!(code.as_str(), name);
        assert_eq!(
            code.is_retryable(),
            retryable,
            "{name} retryable flag drifted from the catalogue"
        );
    }
    // serde must produce the same string as as_str(): the catalogue spelling is
    // the protocol one, and `rename_all` alone would have emitted
    // `INVALID_TRANSITION` / `HUMAN_REQUIRED` / `PRECONDITION_FAILED` for three
    // of these eight. See the Q-07 note on BindingErrorCode.
    for (code, name, _) in expected {
        assert_eq!(
            serde_json::to_string(&code).unwrap(),
            format!("\"{name}\""),
            "serde and as_str disagree for {code:?}"
        );
    }
    for status in BindingStatus::ALL {
        assert_eq!(
            serde_json::to_string(&status).unwrap(),
            format!("\"{}\"", status.as_str()),
            "serde and as_str disagree for {status:?}"
        );
    }
    // `BINDING_ACTIVE_RUNS_EXIST` also exists in the catalogue, and it is
    // *closer* to §4's wording than `RUN_ACTIVE_EXISTS`. §4 prints the latter, so
    // that is the one used. Asserting the near-miss is absent stops a future
    // reader from "correcting" it to the more plausible-looking code.
    let wires: Vec<&str> = expected.iter().map(|(_, n, _)| *n).collect();
    assert!(!wires.contains(&"BINDING_ACTIVE_RUNS_EXIST"));
    assert!(wires.contains(&"RUN_ACTIVE_EXISTS"));
}

#[test]
fn event_names_are_transcribed_verbatim_from_the_specification() {
    // Each spelling is printed in §4's Event column. Drift here changes what
    // subscribers see, so the exact strings are asserted.
    let expected = [
        (
            BindingEvent::BindingChangeRequested,
            "BINDING_CHANGE_REQUESTED",
        ),
        (BindingEvent::RoleBound, "ROLE_BOUND"),
        (BindingEvent::RoleRebound, "ROLE_REBOUND"),
        (BindingEvent::BindingChangeFailed, "BINDING_CHANGE_FAILED"),
        (BindingEvent::BindingDraining, "BINDING_DRAINING"),
        (BindingEvent::RoleUnbound, "ROLE_UNBOUND"),
        (
            BindingEvent::BindingChangeCancelled,
            "BINDING_CHANGE_CANCELLED",
        ),
    ];
    for (event, name) in expected {
        assert_eq!(event.as_str(), name);
        assert_eq!(
            serde_json::to_string(&event).unwrap(),
            format!("\"{name}\"")
        );
    }
    for status in BindingStatus::ALL {
        assert_eq!(status.as_str(), status.as_str().to_uppercase());
    }
    assert_eq!(
        serde_json::to_string(&BindingStatus::PendingCheck).unwrap(),
        "\"PENDING_CHECK\""
    );
    assert_eq!(
        serde_json::to_string(&BindingErrorCode::RunActiveExists).unwrap(),
        "\"RUN_ACTIVE_EXISTS\""
    );
}

#[test]
fn serde_agrees_with_as_str_for_every_event() {
    // The Q-07 tripwire, and the check the other two suites still owe. `as_str()`
    // is the frozen §4 spelling; serde is what actually reaches the wire. When
    // they disagree the event has two names depending on which path serialises
    // it, and nothing else in the build notices.
    //
    // The three `BINDING_*` variants were once spelled `RunBinding*`, so serde
    // said `RUN_BINDING_CHANGE_REQUESTED` while `as_str()` said
    // `BINDING_CHANGE_REQUESTED`. Renaming the variants made the
    // `SCREAMING_SNAKE_CASE` derive reproduce §4 exactly.
    for edge in BINDING_EDGES {
        for event in edge.events {
            assert_eq!(
                serde_json::to_string(event).unwrap(),
                format!("\"{}\"", event.as_str()),
                "serde and as_str disagree for {event:?}"
            );
        }
    }
    for event in CREATE_PENDING_CHECK.events() {
        assert_eq!(
            serde_json::to_string(event).unwrap(),
            format!("\"{}\"", event.as_str()),
            "serde and as_str disagree for creation event {event:?}"
        );
    }
    // Every event name must be a plausible §4 name, not a serde artefact.
    for edge in BINDING_EDGES {
        for event in edge.events {
            let name = event.as_str();
            assert!(
                name.starts_with("BINDING_") || name.starts_with("ROLE_"),
                "{name} must be a BINDING_* or ROLE_* §4 spelling, \
                 not a serde expansion of the Rust variant name"
            );
        }
    }
}

#[test]
fn the_machine_is_pure_and_deterministic() {
    for status in BindingStatus::ALL {
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
