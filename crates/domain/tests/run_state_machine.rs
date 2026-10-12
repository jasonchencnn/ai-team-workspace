//! Table-driven conformance tests for the Run state machine.
//!
//! Same two-layer shape as [`task_state_machine`]: `EXPECTED_ROWS` is a
//! hand-transcription of `STATE_MACHINES.md` §3's table, and the tests assert it
//! against [`RUN_EDGES`] in both directions, so a row dropped from the table
//! fails and a row invented into the table fails too.
//!
//! The transcription deliberately carries the **rejection code** and the **event**
//! of every row, not just the endpoints. A behavioural test that only checked
//! `from -> to` would pass against a table whose codes had silently drifted from
//! the frozen catalogue.
//!
//! [`task_state_machine`]: crate::task_state_machine

use teamspace_domain::{
    CREATE_QUEUED, RUN_EDGES, ReconcileOutcome, RunCommand, RunErrorCode, RunEvent,
    RunPrecondition, RunStatus, run_rejection_code as rejection_code, run_transition as transition,
};

/// Every non-creation row of §3's table, transcribed: (from, command, to, event,
/// rejection).
///
/// §3's last row is one line for three destinations, so it is expanded into
/// three rows here. §3's 拒绝码 column is `—` on five rows; the transcription
/// records the derived `PreconditionFailed` for those, and
/// `the_five_dash_rows_and_the_frozen_one_share_one_code` pins that down.
type Row = (RunStatus, RunCommand, RunStatus, RunEvent, RunErrorCode);

const EXPECTED_ROWS: &[Row] = &[
    // QUEUED has exactly two out-edges: STARTING and CANCELLED.
    (
        RunStatus::Queued,
        RunCommand::Start,
        RunStatus::Starting,
        RunEvent::RunStarting,
        RunErrorCode::AdapterUnavailable,
    ),
    (
        RunStatus::Queued,
        RunCommand::Cancel,
        RunStatus::Cancelled,
        RunEvent::RunCancelled,
        RunErrorCode::PermissionDenied,
    ),
    // STARTING has five.
    (
        RunStatus::Starting,
        RunCommand::ConfirmRunning,
        RunStatus::Running,
        RunEvent::RunStarted,
        RunErrorCode::BindingStaleVersion,
    ),
    (
        RunStatus::Starting,
        RunCommand::Fail,
        RunStatus::Failed,
        RunEvent::RunFailed,
        RunErrorCode::PreconditionFailed,
    ),
    (
        RunStatus::Starting,
        RunCommand::Timeout,
        RunStatus::TimedOut,
        RunEvent::RunTimedOut,
        RunErrorCode::PreconditionFailed,
    ),
    (
        RunStatus::Starting,
        RunCommand::Cancel,
        RunStatus::Cancelled,
        RunEvent::RunCancelled,
        RunErrorCode::RunCancelNotConfirmed,
    ),
    (
        RunStatus::Starting,
        RunCommand::MarkUnknown,
        RunStatus::Unknown,
        RunEvent::RunStatusUnknown,
        RunErrorCode::PreconditionFailed,
    ),
    // RUNNING has five.
    (
        RunStatus::Running,
        RunCommand::Succeed,
        RunStatus::Succeeded,
        RunEvent::RunSucceeded,
        RunErrorCode::RunFenced,
    ),
    (
        RunStatus::Running,
        RunCommand::Fail,
        RunStatus::Failed,
        RunEvent::RunFailed,
        RunErrorCode::RunFenced,
    ),
    (
        RunStatus::Running,
        RunCommand::Timeout,
        RunStatus::TimedOut,
        RunEvent::RunTimedOut,
        RunErrorCode::PreconditionFailed,
    ),
    (
        RunStatus::Running,
        RunCommand::Cancel,
        RunStatus::Cancelled,
        RunEvent::RunCancelled,
        RunErrorCode::PermissionDenied,
    ),
    (
        RunStatus::Running,
        RunCommand::MarkUnknown,
        RunStatus::Unknown,
        RunEvent::RunStatusUnknown,
        RunErrorCode::PreconditionFailed,
    ),
    // UNKNOWN has five, all driven by reconcile. The last three are §3's single
    // combined line and share the `RUN_RECONCILED_*` wildcard event.
    (
        RunStatus::Unknown,
        RunCommand::Reconcile(ReconcileOutcome::StillRunning),
        RunStatus::Running,
        RunEvent::RunReconciledRunning,
        RunErrorCode::RunFenced,
    ),
    (
        RunStatus::Unknown,
        RunCommand::Reconcile(ReconcileOutcome::Succeeded),
        RunStatus::Succeeded,
        RunEvent::RunReconciledSucceeded,
        RunErrorCode::ArtifactInvalid,
    ),
    (
        RunStatus::Unknown,
        RunCommand::Reconcile(ReconcileOutcome::Failed),
        RunStatus::Failed,
        RunEvent::RunReconciledTerminal,
        RunErrorCode::PreconditionFailed,
    ),
    (
        RunStatus::Unknown,
        RunCommand::Reconcile(ReconcileOutcome::TimedOut),
        RunStatus::TimedOut,
        RunEvent::RunReconciledTerminal,
        RunErrorCode::PreconditionFailed,
    ),
    (
        RunStatus::Unknown,
        RunCommand::Reconcile(ReconcileOutcome::Cancelled),
        RunStatus::Cancelled,
        RunEvent::RunReconciledTerminal,
        RunErrorCode::PreconditionFailed,
    ),
];

/// Every command, including each reconcile outcome: 7 plain + 5 reconcile.
const ALL_COMMANDS: [RunCommand; 12] = RunCommand::all();

/// The four states §3 calls 终态.
const TERMINAL: [RunStatus; 4] = [
    RunStatus::Succeeded,
    RunStatus::Failed,
    RunStatus::TimedOut,
    RunStatus::Cancelled,
];

#[test]
fn the_table_holds_exactly_the_specified_rows() {
    assert_eq!(
        RUN_EDGES.len(),
        EXPECTED_ROWS.len(),
        "§3 has 17 lifecycle edges (its 15 printed lines, last one covering three \
         destinations); the table holds {}",
        RUN_EDGES.len()
    );
    for (from, command, to, event, rejection) in EXPECTED_ROWS {
        let decided = transition(*from, *command);
        let t = decided.unwrap_or_else(|r| {
            panic!("§3 requires {from} --{command}--> {to}, but rejected: {r}")
        });
        assert_eq!(t.from(), *from);
        assert_eq!(t.to(), *to, "§3 destination for {from} --{command}-->");
        assert_eq!(t.command(), *command);
        assert!(
            t.events().contains(event),
            "{from} --{command}--> must emit {event}, table says {:?}",
            t.events()
        );
        assert_eq!(
            rejection_code(*from, *command),
            Some(*rejection),
            "{from} --{command}--> lost its §3 拒绝码"
        );
    }
}

#[test]
fn the_table_holds_no_edge_the_specification_omits() {
    // The mirror image. Without it, an invented edge passes the positive test.
    for status in RunStatus::ALL {
        for command in ALL_COMMANDS {
            let in_table = RUN_EDGES
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
fn every_state_is_reachable_from_queued() {
    let mut reached = vec![RunStatus::Queued];
    let mut changed = true;
    while changed {
        changed = false;
        for edge in RUN_EDGES {
            if reached.contains(&edge.from) && !reached.contains(&edge.to) {
                reached.push(edge.to);
                changed = true;
            }
        }
    }
    for status in RunStatus::ALL {
        assert!(
            reached.contains(&status),
            "{status} is unreachable from QUEUED — it has no inbound edge"
        );
    }
}

#[test]
fn the_four_terminal_states_absorb() {
    // §3: "SUCCEEDED/FAILED/TIMED_OUT/CANCELLED 为单个 Run 终态".
    for status in TERMINAL {
        assert!(status.is_terminal(), "{status} must be terminal");
        for command in ALL_COMMANDS {
            assert!(
                transition(status, command).is_err(),
                "§3 makes {status} absorbing, but {command} was accepted"
            );
        }
    }
    assert_eq!(
        RUN_EDGES
            .iter()
            .filter(|e| TERMINAL.contains(&e.from))
            .count(),
        0
    );
}

#[test]
fn unknown_is_not_terminal() {
    // §3: "UNKNOWN 是平台重启或 Adapter 失联后的恢复态，不是正常执行路径".
    // The consequence is that it has five out-edges. Treating it as terminal
    // would strand every Run whose Adapter vanished.
    assert!(!RunStatus::Unknown.is_terminal());
    let out: Vec<RunStatus> = RUN_EDGES
        .iter()
        .filter(|e| e.from == RunStatus::Unknown)
        .map(|e| e.to)
        .collect();
    assert_eq!(out.len(), 5, "§3 lists five reconcile destinations");
    for target in [
        RunStatus::Running,
        RunStatus::Succeeded,
        RunStatus::Failed,
        RunStatus::TimedOut,
        RunStatus::Cancelled,
    ] {
        assert!(out.contains(&target), "§3 allows UNKNOWN --> {target}");
    }
}

#[test]
fn no_edge_returns_to_queued() {
    // §3: "自动重试只读取终态 Run，创建新 Run；禁止把旧 Run 改回 QUEUED".
    // Enforced structurally: QUEUED's only in-edge is creation.
    for edge in RUN_EDGES {
        assert_ne!(
            edge.to,
            RunStatus::Queued,
            "{} --{}--> returns to QUEUED, which §3 forbids",
            edge.from,
            edge.command
        );
    }
    let into_queued = RUN_EDGES
        .iter()
        .filter(|e| e.to == RunStatus::Queued)
        .count();
    assert_eq!(into_queued, 0);
    assert_eq!(
        CREATE_QUEUED.initial(),
        RunStatus::Queued,
        "a retry creates a *new* Run in QUEUED, it does not revive an old one"
    );
}

#[test]
fn each_pair_resolves_to_exactly_one_row() {
    // The invariant `transition` depends on. If a second row ever shared a
    // (from, command) pair, `transition`'s linear search would return the first
    // and quietly mis-route the Run — for reconcile that meant recording a dead
    // Run as still running. This test is the tripwire for reintroducing an
    // unqualified multi-destination command.
    for (i, a) in RUN_EDGES.iter().enumerate() {
        for b in &RUN_EDGES[i + 1..] {
            assert!(
                !(a.from == b.from && a.command == b.command),
                "{} --{}--> is claimed by two rows ({:?} and {:?}); \
                 `transition` would silently pick one",
                a.from,
                a.command,
                a.to,
                b.to
            );
        }
    }
    // And the sweep must be exhaustive: for every pair, the table is either
    // silent or answers, and the answer is the one row.
    for status in RunStatus::ALL {
        for command in ALL_COMMANDS {
            let matches: Vec<&teamspace_domain::RunEdge> = RUN_EDGES
                .iter()
                .filter(|e| e.from == status && e.command == command)
                .collect();
            assert!(
                matches.len() <= 1,
                "{status} --{command}--> matched {} rows",
                matches.len()
            );
            match transition(status, command) {
                Ok(t) => assert_eq!(
                    Some(t.to()),
                    matches.first().map(|e| e.to),
                    "{status} --{command}--> returned a destination the table does not hold"
                ),
                Err(_) => assert!(
                    matches.is_empty(),
                    "{status} --{command}--> is in the table but was rejected"
                ),
            }
        }
    }
}

#[test]
fn reconcile_is_reachable_only_from_unknown() {
    // `Reconcile` exists to recover an `UNKNOWN` Run. Letting it fire from a live
    // Run would let a recovery pass overwrite a Run the Adapter is still driving.
    let sources: Vec<RunStatus> = RUN_EDGES
        .iter()
        .filter(|e| matches!(e.command, RunCommand::Reconcile(_)))
        .map(|e| e.from)
        .collect();
    assert_eq!(sources, vec![RunStatus::Unknown; 5]);
    for status in RunStatus::ALL {
        for outcome in ReconcileOutcome::ALL {
            let accepted = transition(status, RunCommand::Reconcile(outcome)).is_ok();
            assert_eq!(
                accepted,
                status == RunStatus::Unknown,
                "reconcile({outcome}) from {status} accepted={accepted}"
            );
        }
    }
}

#[test]
fn each_reconcile_outcome_lands_on_its_own_destination() {
    // The bug this exists to prevent: a bare `Reconcile` made the graph 5-way
    // ambiguous, and the linear search returned `UNKNOWN → RUNNING` for every
    // outcome. Asserting the mapping per outcome is the regression guard.
    let expected = [
        (ReconcileOutcome::StillRunning, RunStatus::Running),
        (ReconcileOutcome::Succeeded, RunStatus::Succeeded),
        (ReconcileOutcome::Failed, RunStatus::Failed),
        (ReconcileOutcome::TimedOut, RunStatus::TimedOut),
        (ReconcileOutcome::Cancelled, RunStatus::Cancelled),
    ];
    for (outcome, target) in expected {
        let t = transition(RunStatus::Unknown, RunCommand::Reconcile(outcome))
            .unwrap_or_else(|r| panic!("§3 allows UNKNOWN --reconcile--> {target}: {r}"));
        assert_eq!(
            t.to(),
            target,
            "reconcile({outcome}) must not fall through to another destination"
        );
    }
    // Each outcome must also carry its own §3 rejection code, so a failed
    // proof reports why *that* proof failed.
    assert_eq!(
        rejection_code(
            RunStatus::Unknown,
            RunCommand::Reconcile(ReconcileOutcome::StillRunning)
        ),
        Some(RunErrorCode::RunFenced)
    );
    assert_eq!(
        rejection_code(
            RunStatus::Unknown,
            RunCommand::Reconcile(ReconcileOutcome::Succeeded)
        ),
        Some(RunErrorCode::ArtifactInvalid)
    );
    for outcome in [
        ReconcileOutcome::Failed,
        ReconcileOutcome::TimedOut,
        ReconcileOutcome::Cancelled,
    ] {
        assert_eq!(
            rejection_code(RunStatus::Unknown, RunCommand::Reconcile(outcome)),
            Some(RunErrorCode::PreconditionFailed),
            "§3 prints DOMAIN_PRECONDITION_FAILED verbatim for UNKNOWN --> {outcome}"
        );
    }
}

#[test]
fn the_five_dash_rows_and_the_frozen_one_share_one_code() {
    // §3's 拒绝码 column is `—` on exactly five rows. The catalogue's generic
    // `DOMAIN_PRECONDITION_FAILED` is the only code the specification points at
    // for them, so the implementation derives that choice. §3's *last* row
    // prints `DOMAIN_PRECONDITION_FAILED` verbatim, so it reaches the same
    // variant by a spec-mandated route rather than a derived one.
    let dash_rows = [
        (RunStatus::Starting, RunCommand::Fail),
        (RunStatus::Starting, RunCommand::Timeout),
        (RunStatus::Starting, RunCommand::MarkUnknown),
        (RunStatus::Running, RunCommand::Timeout),
        (RunStatus::Running, RunCommand::MarkUnknown),
    ];
    assert_eq!(dash_rows.len(), 5);
    for (from, command) in dash_rows {
        assert_eq!(
            rejection_code(from, command),
            Some(RunErrorCode::PreconditionFailed),
            "{from} --{command}--> is a §3 `—` row"
        );
    }
    for outcome in [
        ReconcileOutcome::Failed,
        ReconcileOutcome::TimedOut,
        ReconcileOutcome::Cancelled,
    ] {
        assert_eq!(
            rejection_code(RunStatus::Unknown, RunCommand::Reconcile(outcome)),
            Some(RunErrorCode::PreconditionFailed),
            "UNKNOWN --> {outcome} prints DOMAIN_PRECONDITION_FAILED verbatim"
        );
    }
    // The ten rows with a printed code must not be collapsed onto the generic one.
    let printed = [
        (RunStatus::Queued, RunCommand::Start),
        (RunStatus::Queued, RunCommand::Cancel),
        (RunStatus::Starting, RunCommand::ConfirmRunning),
        (RunStatus::Starting, RunCommand::Cancel),
        (RunStatus::Running, RunCommand::Succeed),
        (RunStatus::Running, RunCommand::Fail),
        (RunStatus::Running, RunCommand::Cancel),
    ];
    for (from, command) in printed {
        assert_ne!(
            rejection_code(from, command),
            Some(RunErrorCode::PreconditionFailed),
            "{from} --{command}--> has a printed §3 code and must keep it"
        );
    }
}

#[test]
fn an_illegal_transition_reports_the_catalogue_code() {
    for status in RunStatus::ALL {
        for command in ALL_COMMANDS {
            if let Err(rejection) = transition(status, command) {
                assert_eq!(
                    rejection.code(),
                    RunErrorCode::InvalidTransition,
                    "{status} --{command}--> has no edge, so it must be \
                     DOMAIN_INVALID_TRANSITION"
                );
                assert_eq!(
                    rejection.current(),
                    status,
                    "a rejection must report the untouched state"
                );
                assert_eq!(
                    rejection.command(),
                    command,
                    "a rejection must report the refused command"
                );
            }
        }
    }
}

#[test]
fn a_legal_edge_exposes_its_own_rejection_code() {
    for edge in RUN_EDGES {
        assert_eq!(
            rejection_code(edge.from, edge.command),
            Some(edge.rejection),
            "{} --{}--> lost its §3 rejection code",
            edge.from,
            edge.command
        );
        assert_ne!(
            edge.rejection,
            RunErrorCode::InvalidTransition,
            "{} --{}--> is a real edge, so it must not carry the generic code",
            edge.from,
            edge.command
        );
    }
    for status in RunStatus::ALL {
        for command in ALL_COMMANDS {
            if transition(status, command).is_err() {
                assert_eq!(
                    rejection_code(status, command),
                    None,
                    "{status} --{command}--> has no edge, so no specific code exists"
                );
            }
        }
    }
}

#[test]
fn every_edge_declares_preconditions_and_events() {
    // §3 gives both columns for all fifteen printed lines. An empty slice would
    // mean the row was transcribed with its details dropped, which no behavioural
    // test could detect.
    for edge in RUN_EDGES {
        assert!(
            !edge.preconditions.is_empty(),
            "{} --{}--> has no preconditions; §3 lists at least one",
            edge.from,
            edge.command
        );
        assert!(
            !edge.events.is_empty(),
            "{} --{}--> has no events; §3 lists at least one",
            edge.from,
            edge.command
        );
        for precondition in edge.preconditions {
            assert!(
                !precondition.condition_key().is_empty(),
                "precondition must expose a stable condition_key"
            );
        }
        for event in edge.events {
            assert!(
                event.as_str().starts_with("RUN_"),
                "{} is not a RUN_* event name",
                event.as_str()
            );
        }
    }
    // §3's table has 17 edges but only 14 distinct preconditions: its last line
    // covers three destinations under one 前置条件 ("Adapter 证明终态，或无法恢复
    // 并按策略/Human 决定"). So 14 is the specification's number, not a shortfall.
    let mut keys: Vec<&str> = RUN_EDGES
        .iter()
        .flat_map(|e| e.preconditions.iter())
        .map(|p| p.condition_key())
        .collect();
    keys.sort_unstable();
    keys.dedup();
    assert_eq!(
        keys.len(),
        14,
        "§3 names 14 distinct preconditions across 17 edges; got {keys:?}"
    );
    let shared = RUN_EDGES
        .iter()
        .filter(|e| {
            e.preconditions
                .contains(&RunPrecondition::TerminalProvenOrDecided)
        })
        .map(|e| e.to)
        .collect::<Vec<_>>();
    assert_eq!(
        shared,
        vec![RunStatus::Failed, RunStatus::TimedOut, RunStatus::Cancelled],
        "the only precondition §3 shares across rows is the combined terminal one"
    );
}

#[test]
fn every_transition_ends_in_a_state_of_the_machine() {
    for edge in RUN_EDGES {
        let t = transition(edge.from, edge.command).expect("table row must apply");
        assert!(RunStatus::ALL.contains(&t.to()));
        assert_ne!(t.to(), edge.from, "§3 has no self-loops");
    }
}

#[test]
fn cancelling_always_reaches_cancelled() {
    for edge in RUN_EDGES {
        if edge.command == RunCommand::Cancel {
            assert_eq!(edge.to, RunStatus::Cancelled, "§3 cancel rows");
            assert!(
                edge.events.contains(&RunEvent::RunCancelled),
                "{} --cancel--> must emit RUN_CANCELLED",
                edge.from
            );
        }
    }
    let cancellable: Vec<RunStatus> = RUN_EDGES
        .iter()
        .filter(|e| e.command == RunCommand::Cancel)
        .map(|e| e.from)
        .collect();
    for status in [RunStatus::Queued, RunStatus::Starting, RunStatus::Running] {
        assert!(
            cancellable.contains(&status),
            "§3 allows cancelling from {status}"
        );
    }
    // UNKNOWN cannot be cancelled directly: §3 routes it through reconcile.
    assert!(!cancellable.contains(&RunStatus::Unknown));
}

#[test]
fn cancelling_from_starting_and_running_carry_different_codes() {
    // §3 distinguishes them: STARTING → CANCELLED is `RUN_CANCEL_NOT_CONFIRMED`
    // (retryable), RUNNING → CANCELLED is `AUTH_PERMISSION_DENIED`. A
    // Human-takeover fence has already been decided by the time a Run is
    // RUNNING, so the remaining failure is a permission problem, not a
    // confirmation one.
    assert_eq!(
        rejection_code(RunStatus::Starting, RunCommand::Cancel),
        Some(RunErrorCode::RunCancelNotConfirmed)
    );
    assert!(RunErrorCode::RunCancelNotConfirmed.is_retryable());
    assert_eq!(
        rejection_code(RunStatus::Running, RunCommand::Cancel),
        Some(RunErrorCode::PermissionDenied)
    );
    assert!(!RunErrorCode::PermissionDenied.is_retryable());
    // QUEUED → CANCELLED is a Human/Leader action, so it is also permission-gated.
    assert_eq!(
        rejection_code(RunStatus::Queued, RunCommand::Cancel),
        Some(RunErrorCode::PermissionDenied)
    );
}

#[test]
fn creation_supports_exactly_queued() {
    // §3 has exactly one creation row: 创建 → QUEUED, claimed by the Task
    // claim/retry service, rejected with TASK_ALREADY_CLAIMED.
    assert_eq!(CREATE_QUEUED.initial(), RunStatus::Queued);
    assert_eq!(CREATE_QUEUED.rejection(), RunErrorCode::AlreadyClaimed);
    assert!(CREATE_QUEUED.events().contains(&RunEvent::RunQueued));
    assert_eq!(
        CREATE_QUEUED.preconditions(),
        &[RunPrecondition::LeaseObtainable],
        "§3: Task READY、Binding 有效、租约可获得"
    );
    for status in [
        RunStatus::Starting,
        RunStatus::Running,
        RunStatus::Unknown,
        RunStatus::Succeeded,
        RunStatus::Failed,
        RunStatus::TimedOut,
        RunStatus::Cancelled,
    ] {
        assert_ne!(
            CREATE_QUEUED.initial(),
            status,
            "§3 has no 创建 --> {status} row"
        );
    }
    // Every state other than QUEUED must be enterable by a command edge. If a
    // state had no in-edge, the only way to reach it would be construction, and
    // §3 allows exactly one construction target.
    for status in [
        RunStatus::Starting,
        RunStatus::Running,
        RunStatus::Unknown,
        RunStatus::Succeeded,
        RunStatus::Failed,
        RunStatus::TimedOut,
        RunStatus::Cancelled,
    ] {
        let inbound = RUN_EDGES.iter().filter(|e| e.to == status).count();
        assert!(
            inbound > 0,
            "{status} has no in-edge, so it could only be entered by construction \
             — which §3 does not allow"
        );
    }
    // And QUEUED, the sole construction target, has no in-edge either: §3
    // forbids reviving an old Run by pushing it back to QUEUED.
    assert_eq!(
        RUN_EDGES
            .iter()
            .filter(|e| e.to == RunStatus::Queued)
            .count(),
        0
    );
}

#[test]
fn error_codes_match_the_frozen_catalogue() {
    // `ERROR_CATALOG.md` is a protocol contract. Spelling drift would be a
    // breaking change discovered at runtime.
    let expected = [
        (
            RunErrorCode::InvalidTransition,
            "DOMAIN_INVALID_TRANSITION",
            false,
        ),
        (RunErrorCode::AlreadyClaimed, "TASK_ALREADY_CLAIMED", true),
        (
            RunErrorCode::AdapterUnavailable,
            "ADAPTER_UNAVAILABLE",
            true,
        ),
        (
            RunErrorCode::PermissionDenied,
            "AUTH_PERMISSION_DENIED",
            false,
        ),
        (
            RunErrorCode::BindingStaleVersion,
            "BINDING_STALE_VERSION",
            false,
        ),
        (
            RunErrorCode::RunCancelNotConfirmed,
            "RUN_CANCEL_NOT_CONFIRMED",
            true,
        ),
        (RunErrorCode::RunFenced, "RUN_FENCED", false),
        (RunErrorCode::ArtifactInvalid, "ARTIFACT_INVALID", false),
        (
            RunErrorCode::PreconditionFailed,
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
    // Exactly three of this machine's nine codes are retryable. ADAPTER_UNAVAILABLE
    // being the third is easy to miss: §3 quotes TASK_ALREADY_CLAIMED and
    // RUN_CANCEL_NOT_CONFIRMED as its retryable pair, but the catalogue marks
    // ADAPTER_UNAVAILABLE 是 as well, and §3 does use it on QUEUED → STARTING.
    let retryable: Vec<&str> = expected
        .iter()
        .filter(|(_, _, r)| *r)
        .map(|(_, n, _)| *n)
        .collect();
    assert_eq!(
        retryable,
        vec![
            "TASK_ALREADY_CLAIMED",
            "ADAPTER_UNAVAILABLE",
            "RUN_CANCEL_NOT_CONFIRMED",
        ]
    );
}

#[test]
fn wire_names_are_screaming_snake_case() {
    // The catalogue and the event bus both spell these in SCREAMING_SNAKE_CASE;
    // serde must agree or the stored event names drift from the protocol.
    assert_eq!(
        serde_json::to_string(&RunStatus::TimedOut).unwrap(),
        "\"TIMED_OUT\""
    );
    assert_eq!(
        serde_json::to_string(&RunStatus::Queued).unwrap(),
        "\"QUEUED\""
    );
    assert_eq!(
        serde_json::to_string(&RunErrorCode::RunFenced).unwrap(),
        "\"RUN_FENCED\""
    );
    assert_eq!(
        serde_json::to_string(&RunEvent::RunStatusUnknown).unwrap(),
        "\"RUN_STATUS_UNKNOWN\""
    );
    for status in RunStatus::ALL {
        assert_eq!(
            status.as_str(),
            status.as_str().to_uppercase(),
            "{} is not upper case",
            status.as_str()
        );
    }
}

#[test]
fn the_machine_is_pure_and_deterministic() {
    // `transition` takes no context and performs no I/O, so the same question
    // must always get the same answer. Reconcile used to be the case that broke
    // this in a worse way: one unqualified command with five answers, resolved by
    // table order. The sweep below now covers all 8 × 12 pairs, five of which are
    // the reconcile outcomes.
    for status in RunStatus::ALL {
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
    let reconciled: Vec<RunStatus> = RUN_EDGES
        .iter()
        .filter(|e| matches!(e.command, RunCommand::Reconcile(_)))
        .map(|e| e.to)
        .collect();
    assert_eq!(
        reconciled.len(),
        5,
        "§3 lists five reconcile destinations, one per outcome"
    );
    for outcome in ReconcileOutcome::ALL {
        assert!(transition(RunStatus::Unknown, RunCommand::Reconcile(outcome)).is_ok());
    }
}

#[test]
fn serde_agrees_with_as_str_for_every_code_and_event() {
    // The Q-07/Q-08 tripwire, Run's half. `as_str()` is the frozen
    // `ERROR_CATALOG.md` spelling; serde is what reaches the wire. Four of Run's
    // nine codes were wrong on the serde path before every variant got an
    // explicit `#[serde(rename)]`: `INVALID_TRANSITION`, `ALREADY_CLAIMED`,
    // `PERMISSION_DENIED` and `PRECONDITION_FAILED` all lost their domain
    // prefix. RunErrorCode covers three of the same codes Task raises, so the
    // same drift existed on both sides of the crate boundary.
    let expected = [
        (RunErrorCode::InvalidTransition, "DOMAIN_INVALID_TRANSITION"),
        (RunErrorCode::AlreadyClaimed, "TASK_ALREADY_CLAIMED"),
        (RunErrorCode::AdapterUnavailable, "ADAPTER_UNAVAILABLE"),
        (RunErrorCode::PermissionDenied, "AUTH_PERMISSION_DENIED"),
        (RunErrorCode::BindingStaleVersion, "BINDING_STALE_VERSION"),
        (
            RunErrorCode::RunCancelNotConfirmed,
            "RUN_CANCEL_NOT_CONFIRMED",
        ),
        (RunErrorCode::RunFenced, "RUN_FENCED"),
        (RunErrorCode::ArtifactInvalid, "ARTIFACT_INVALID"),
        (
            RunErrorCode::PreconditionFailed,
            "DOMAIN_PRECONDITION_FAILED",
        ),
    ];
    for (code, name) in expected {
        assert_eq!(code.as_str(), name);
        assert_eq!(
            serde_json::to_string(&code).unwrap(),
            format!("\"{name}\""),
            "serde and as_str disagree for {code:?}"
        );
    }
    for status in RunStatus::ALL {
        assert_eq!(
            serde_json::to_string(&status).unwrap(),
            format!("\"{}\"", status.as_str()),
            "serde and as_str disagree for {status:?}"
        );
    }
    for edge in RUN_EDGES {
        for event in edge.events {
            assert_eq!(
                serde_json::to_string(event).unwrap(),
                format!("\"{}\"", event.as_str()),
                "serde and as_str disagree for {event:?}"
            );
        }
    }
    for event in CREATE_QUEUED.events() {
        assert_eq!(
            serde_json::to_string(event).unwrap(),
            format!("\"{}\"", event.as_str()),
            "serde and as_str disagree for creation event {event:?}"
        );
    }
}
