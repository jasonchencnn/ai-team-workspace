//! Table-driven conformance tests for the Task state machine.
//!
//! Every test walks [`TASK_EDGES`] / [`TASK_CREATIONS`] — the same constants the
//! implementation reads — so a table edit is a test edit and the two can never
//! drift apart. Tests assert the *frozen specification* shape (which edges exist,
//! which codes they carry) separately from the *behaviour* shape (what
//! `transition` returns), because a table that quietly drops a row would still
//! make a purely behavioural test pass.

use teamspace_domain::{
    CREATE_READY, CREATE_TODO, TASK_CREATIONS, TASK_EDGES, TaskCommand, TaskErrorCode, TaskEvent,
    TaskPrecondition, TaskStatus, task_creation_target as creation_target,
    task_rejection_code as rejection_code, task_transition as transition,
};

/// The 14 lifecycle edges of `STATE_MACHINES.md` §2.1, excluding the two
/// creation edges (which have no source state and are covered separately).
const EXPECTED_EDGES: &[(TaskStatus, TaskCommand, TaskStatus)] = &[
    (TaskStatus::Todo, TaskCommand::MarkReady, TaskStatus::Ready),
    (TaskStatus::Todo, TaskCommand::Cancel, TaskStatus::Cancelled),
    (TaskStatus::Ready, TaskCommand::Claim, TaskStatus::Doing),
    (
        TaskStatus::Ready,
        TaskCommand::Cancel,
        TaskStatus::Cancelled,
    ),
    (TaskStatus::Doing, TaskCommand::Block, TaskStatus::Blocked),
    (
        TaskStatus::Doing,
        TaskCommand::SubmitReview,
        TaskStatus::Review,
    ),
    (TaskStatus::Doing, TaskCommand::Complete, TaskStatus::Done),
    (
        TaskStatus::Doing,
        TaskCommand::Cancel,
        TaskStatus::Cancelled,
    ),
    (
        TaskStatus::Blocked,
        TaskCommand::ResolveBlock,
        TaskStatus::Ready,
    ),
    (
        TaskStatus::Blocked,
        TaskCommand::Cancel,
        TaskStatus::Cancelled,
    ),
    (
        TaskStatus::Review,
        TaskCommand::ApproveReview,
        TaskStatus::Done,
    ),
    (
        TaskStatus::Review,
        TaskCommand::RequestChanges,
        TaskStatus::Ready,
    ),
    (
        TaskStatus::Review,
        TaskCommand::Cancel,
        TaskStatus::Cancelled,
    ),
    (TaskStatus::Done, TaskCommand::Reopen, TaskStatus::Ready),
];

const ALL_COMMANDS: [TaskCommand; 10] = [
    TaskCommand::MarkReady,
    TaskCommand::Cancel,
    TaskCommand::Claim,
    TaskCommand::Block,
    TaskCommand::SubmitReview,
    TaskCommand::Complete,
    TaskCommand::ResolveBlock,
    TaskCommand::ApproveReview,
    TaskCommand::RequestChanges,
    TaskCommand::Reopen,
];

#[test]
fn the_table_holds_exactly_the_specified_edges() {
    assert_eq!(
        TASK_EDGES.len(),
        EXPECTED_EDGES.len(),
        "§2.1 lists 14 lifecycle edges; the table holds {}",
        TASK_EDGES.len()
    );
    for (from, command, to) in EXPECTED_EDGES {
        let found = transition(*from, *command);
        let transition = found.unwrap_or_else(|rejection| {
            panic!(
                "§2.1 requires {from} --{}--> {to}, but the machine rejected it: {rejection}",
                command
            )
        });
        assert_eq!(transition.from(), *from);
        assert_eq!(
            transition.to(),
            *to,
            "§2.1 destination for {from} --{command}-->"
        );
    }
}

#[test]
fn the_table_holds_no_edge_the_specification_omits() {
    // The mirror image of the test above: walk every (state, command) pair and
    // assert the table is silent exactly where §2 is silent. Without this, an
    // invented edge would pass the positive test unnoticed.
    for status in TaskStatus::ALL {
        for command in ALL_COMMANDS {
            let in_table = TASK_EDGES
                .iter()
                .any(|edge| edge.from == status && edge.command == command);
            let in_spec = EXPECTED_EDGES
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
fn every_non_creation_state_reachable_from_todo() {
    // Reachability, computed by walking the table rather than hard-coded. A new
    // state added without an inbound edge fails here rather than at runtime.
    let mut reached = vec![TaskStatus::Todo];
    let mut changed = true;
    while changed {
        changed = false;
        for edge in TASK_EDGES {
            if reached.contains(&edge.from) && !reached.contains(&edge.to) {
                reached.push(edge.to);
                changed = true;
            }
        }
    }
    for status in TaskStatus::ALL {
        assert!(
            reached.contains(&status),
            "{status} is unreachable from TODO — it has no inbound edge"
        );
    }
}

#[test]
fn cancelled_has_no_outgoing_edge() {
    // §2.2: "CANCELLED → *: P0 不恢复已取消 Task".
    for command in ALL_COMMANDS {
        assert!(
            transition(TaskStatus::Cancelled, command).is_err(),
            "§2.2 forbids leaving CANCELLED, but {command} was accepted"
        );
    }
}

#[test]
fn todo_cannot_jump_straight_to_doing() {
    // §2.2: "TODO → DOING: 必须先通过 readiness 校验进入 READY".
    let rejection =
        transition(TaskStatus::Todo, TaskCommand::Claim).expect_err("§2.2 forbids TODO → DOING");
    assert_eq!(rejection.code(), TaskErrorCode::InvalidTransition);
    assert_eq!(rejection.current(), TaskStatus::Todo);
}

#[test]
fn there_is_no_failed_state() {
    // §2.2: "任意状态 → FAILED: Task 无 FAILED 状态，失败属于 Run".
    // Enforced structurally: the enum cannot be constructed with such a name, so
    // the test asserts the negative space instead.
    let names: Vec<&str> = TaskStatus::ALL.iter().map(|s| s.as_str()).collect();
    assert!(
        !names.contains(&"FAILED"),
        "Task must not have a FAILED state, found {names:?}"
    );
    assert_eq!(names.len(), 7);
}

#[test]
fn an_illegal_transition_reports_the_catalogue_code() {
    for status in TaskStatus::ALL {
        for command in ALL_COMMANDS {
            if let Err(rejection) = transition(status, command) {
                assert_eq!(
                    rejection.code(),
                    TaskErrorCode::InvalidTransition,
                    "{} --{command}--> has no edge, so it must be DOMAIN_INVALID_TRANSITION",
                    status
                );
                assert_eq!(
                    rejection.current(),
                    status,
                    "a rejection must report the untouched state"
                );
                assert!(
                    rejection.condition_keys().is_empty(),
                    "a missing edge has no condition to surface"
                );
            }
        }
    }
}

#[test]
fn a_legal_edge_exposes_its_own_rejection_code() {
    // The catalogue distinguishes "no such edge" from "edge exists, precondition
    // unmet". The application layer relies on this to report the specific code.
    for edge in TASK_EDGES {
        assert_eq!(
            rejection_code(edge.from, edge.command),
            Some(edge.rejection),
            "{} --{}--> lost its §2.1 rejection code",
            edge.from,
            edge.command
        );
        assert_ne!(
            edge.rejection,
            TaskErrorCode::InvalidTransition,
            "{} --{}--> is a real edge, so it must not carry the generic code",
            edge.from,
            edge.command
        );
    }
    for status in TaskStatus::ALL {
        for command in ALL_COMMANDS {
            if transition(status, command).is_err() {
                assert_eq!(
                    rejection_code(status, command),
                    None,
                    "{} --{command}--> has no edge, so no specific code exists",
                    status
                );
            }
        }
    }
}

#[test]
fn every_transition_ends_in_a_state_of_the_machine() {
    for edge in TASK_EDGES {
        let transition = transition(edge.from, edge.command).expect("table row must apply");
        assert!(TaskStatus::ALL.contains(&transition.to()));
        assert_ne!(transition.to(), edge.from, "§2 has no self-loops");
    }
}

#[test]
fn cancelling_always_reaches_cancelled() {
    for edge in TASK_EDGES {
        if edge.command == TaskCommand::Cancel {
            assert_eq!(edge.to, TaskStatus::Cancelled, "§2.1 cancel rows");
            assert!(
                edge.events.contains(&TaskEvent::TaskCancelled),
                "{} --cancel--> must emit TASK_CANCELLED",
                edge.from
            );
        }
    }
    // §2.1 cancels from exactly TODO, READY, DOING, BLOCKED, REVIEW.
    let cancellable: Vec<TaskStatus> = TASK_EDGES
        .iter()
        .filter(|e| e.command == TaskCommand::Cancel)
        .map(|e| e.from)
        .collect();
    for status in [
        TaskStatus::Todo,
        TaskStatus::Ready,
        TaskStatus::Doing,
        TaskStatus::Blocked,
        TaskStatus::Review,
    ] {
        assert!(
            cancellable.contains(&status),
            "§2.1 allows cancelling from {status}"
        );
    }
}

#[test]
fn every_edge_declares_preconditions_and_events() {
    // §2.1 gives both columns for all 14 rows. An empty slice would mean the row
    // was transcribed with the details dropped, which no behavioural test above
    // could detect.
    for edge in TASK_EDGES {
        assert!(
            !edge.preconditions.is_empty(),
            "{} --{}--> has no preconditions; §2.1 lists at least one",
            edge.from,
            edge.command
        );
        assert!(
            !edge.events.is_empty(),
            "{} --{}--> has no events; §2.1 lists at least one",
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
                event.as_str().starts_with("TASK_") || event.as_str().starts_with("RUN_"),
                "{} is not a TASK_*/RUN_* event name",
                event.as_str()
            );
        }
    }
}

#[test]
fn only_two_states_are_terminal_in_the_ordinary_sense() {
    // §2: "CANCELLED 为 P0 终态" and "DONE 可由 Human/授权 Leader 重开为 READY".
    // So CANCELLED has no out-edge at all, while DONE has exactly one.
    let cancelled_out: usize = ALL_COMMANDS
        .iter()
        .filter(|c| transition(TaskStatus::Cancelled, **c).is_ok())
        .count();
    assert_eq!(cancelled_out, 0);

    let done_out: usize = ALL_COMMANDS
        .iter()
        .filter(|c| transition(TaskStatus::Done, **c).is_ok())
        .count();
    assert_eq!(done_out, 1, "§2 allows DONE --reopen--> READY only");
    assert!(transition(TaskStatus::Done, TaskCommand::Reopen).is_ok());
}

#[test]
fn creation_supports_exactly_the_two_documented_targets() {
    assert_eq!(TASK_CREATIONS.len(), 2);
    let todo = creation_target(TaskStatus::Todo).expect("创建 → TODO is legal");
    assert_eq!(todo.initial(), TaskStatus::Todo);
    assert_eq!(todo.rejection(), TaskErrorCode::ValidationInvalidField);
    assert!(todo.events().contains(&TaskEvent::TaskCreated));

    let ready = creation_target(TaskStatus::Ready).expect("创建 → READY is legal");
    assert_eq!(ready.initial(), TaskStatus::Ready);
    assert_eq!(ready.rejection(), TaskErrorCode::DependencyUnmet);
    assert!(ready.events().contains(&TaskEvent::TaskCreated));
    assert!(ready.events().contains(&TaskEvent::TaskReady));
    assert!(!ready.preconditions().is_empty());

    // §2.1 has no other creation edge.
    for status in [
        TaskStatus::Doing,
        TaskStatus::Blocked,
        TaskStatus::Review,
        TaskStatus::Done,
        TaskStatus::Cancelled,
    ] {
        assert!(
            creation_target(status).is_none(),
            "§2.1 has no 创建 → {status} row"
        );
    }
    assert_eq!(creation_target(TaskStatus::Todo), Some(&CREATE_TODO));
    assert_eq!(creation_target(TaskStatus::Ready), Some(&CREATE_READY));
}

#[test]
fn creation_preconditions_carry_a_stable_condition_key() {
    for creation in TASK_CREATIONS {
        for precondition in creation.preconditions() {
            assert!(!precondition.condition_key().is_empty());
        }
    }
    // 创建 → TODO is legal with an incomplete Task, so it declares no readiness
    // precondition; 创建 → READY must declare one.
    assert!(CREATE_TODO.preconditions().is_empty());
    assert_eq!(
        CREATE_READY.preconditions(),
        &[TaskPrecondition::InformationComplete]
    );
}

#[test]
fn error_codes_match_the_frozen_catalogue() {
    // `ERROR_CATALOG.md` is a protocol contract. Spelling drift here would be a
    // breaking change discovered at runtime, so the exact strings are asserted.
    let expected = [
        (
            TaskErrorCode::InvalidTransition,
            "DOMAIN_INVALID_TRANSITION",
            false,
        ),
        (
            TaskErrorCode::PreconditionFailed,
            "DOMAIN_PRECONDITION_FAILED",
            false,
        ),
        (
            TaskErrorCode::PermissionDenied,
            "AUTH_PERMISSION_DENIED",
            false,
        ),
        (
            TaskErrorCode::DependencyUnmet,
            "TASK_DEPENDENCY_UNMET",
            false,
        ),
        (TaskErrorCode::AlreadyClaimed, "TASK_ALREADY_CLAIMED", true),
        (
            TaskErrorCode::BlockReasonRequired,
            "TASK_BLOCK_REASON_REQUIRED",
            false,
        ),
        (TaskErrorCode::ResultRequired, "TASK_RESULT_REQUIRED", false),
        (TaskErrorCode::ReviewRequired, "TASK_REVIEW_REQUIRED", false),
        (
            TaskErrorCode::StaleReviewResult,
            "REVIEW_STALE_RESULT",
            false,
        ),
        (
            TaskErrorCode::ReviewCommentRequired,
            "REVIEW_COMMENT_REQUIRED",
            false,
        ),
        (
            TaskErrorCode::RunCancelNotConfirmed,
            "RUN_CANCEL_NOT_CONFIRMED",
            true,
        ),
        (TaskErrorCode::RunFenced, "RUN_FENCED", false),
        (
            TaskErrorCode::ValidationInvalidField,
            "VALIDATION_INVALID_FIELD",
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
    // The catalogue spells these with SCREAMING_SNAKE_CASE; serde must agree.
    assert_eq!(
        serde_json::to_string(&TaskStatus::Doing).unwrap(),
        "\"DOING\""
    );
    assert_eq!(
        serde_json::to_string(&TaskStatus::Cancelled).unwrap(),
        "\"CANCELLED\""
    );
    assert_eq!(
        serde_json::to_string(&TaskErrorCode::RunFenced).unwrap(),
        "\"RUN_FENCED\""
    );
}

#[test]
fn serde_agrees_with_as_str_for_every_code_and_event() {
    // The Q-07/Q-08 tripwire. `as_str()` is the frozen `ERROR_CATALOG.md`
    // spelling; serde is what actually reaches the wire. Relying on
    // `rename_all = "SCREAMING_SNAKE_CASE"` alone silently produced
    // `INVALID_TRANSITION` for a code frozen as `DOMAIN_INVALID_TRANSITION`,
    // and `PERMISSION_DENIED` for `AUTH_PERMISSION_DENIED` — wire names that
    // exist nowhere in the catalogue. Every variant now carries an explicit
    // `#[serde(rename)]`; this asserts the two paths cannot drift again.
    let codes = [
        (
            TaskErrorCode::InvalidTransition,
            "DOMAIN_INVALID_TRANSITION",
        ),
        (
            TaskErrorCode::PreconditionFailed,
            "DOMAIN_PRECONDITION_FAILED",
        ),
        (TaskErrorCode::PermissionDenied, "AUTH_PERMISSION_DENIED"),
        (TaskErrorCode::DependencyUnmet, "TASK_DEPENDENCY_UNMET"),
        (TaskErrorCode::AlreadyClaimed, "TASK_ALREADY_CLAIMED"),
        (
            TaskErrorCode::BlockReasonRequired,
            "TASK_BLOCK_REASON_REQUIRED",
        ),
        (TaskErrorCode::ResultRequired, "TASK_RESULT_REQUIRED"),
        (TaskErrorCode::ReviewRequired, "TASK_REVIEW_REQUIRED"),
        (TaskErrorCode::StaleReviewResult, "REVIEW_STALE_RESULT"),
        (
            TaskErrorCode::RunCancelNotConfirmed,
            "RUN_CANCEL_NOT_CONFIRMED",
        ),
        (TaskErrorCode::RunFenced, "RUN_FENCED"),
        (
            TaskErrorCode::ValidationInvalidField,
            "VALIDATION_INVALID_FIELD",
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
    for status in TaskStatus::ALL {
        assert_eq!(
            serde_json::to_string(&status).unwrap(),
            format!("\"{}\"", status.as_str()),
            "serde and as_str disagree for {status:?}"
        );
    }
    for event in [
        TaskEvent::TaskCreated,
        TaskEvent::TaskReady,
        TaskEvent::TaskClaimed,
        TaskEvent::TaskCancelled,
        TaskEvent::TaskReviewRequested,
        TaskEvent::TaskReviewApproved,
        TaskEvent::TaskReviewChangesRequested,
        TaskEvent::TaskCompleted,
        TaskEvent::TaskBlocked,
        TaskEvent::TaskUnblocked,
        TaskEvent::TaskReopened,
        TaskEvent::RunQueued,
    ] {
        assert_eq!(
            serde_json::to_string(&event).unwrap(),
            format!("\"{}\"", event.as_str()),
            "serde and as_str disagree for {event:?}"
        );
    }
}
