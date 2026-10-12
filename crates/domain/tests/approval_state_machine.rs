//! Table-driven conformance tests for the Approval state machine.
//!
//! Same two-layer shape as the other five suites: `EXPECTED_ROWS` is a
//! hand-transcription of `STATE_MACHINES.md` §7's table, asserted against
//! [`APPROVAL_EDGES`] in both directions.
//!
//! §7 is the shortest table in the spec, so the interesting part is not
//! transcription but the two things a table cannot state on its own:
//!
//! * "全部终态不可重开；新请求必须新建 Approval" — all four terminal states absorb.
//! * "`PENDING → CANCELLED` carries `DOMAIN_INVALID_TRANSITION`" — a *legal* edge
//!   whose rejection code is the same string a *missing* edge reports. The two
//!   meanings must stay distinguishable.

use teamspace_domain::{
    APPROVAL_CREATE_TARGETS, APPROVAL_EDGES, ApprovalCommand, ApprovalErrorCode, ApprovalEvent,
    ApprovalPrecondition, ApprovalStatus, CREATE_PENDING_APPROVAL,
    approval_creation_target as creation_target, approval_rejection_code as rejection_code,
    approval_transition as transition,
};

/// Every non-creation row of §7's table.
type Row = (
    ApprovalStatus,
    ApprovalCommand,
    ApprovalStatus,
    &'static [ApprovalEvent],
    ApprovalErrorCode,
);

const EXPECTED_ROWS: &[Row] = &[
    (
        ApprovalStatus::Pending,
        ApprovalCommand::Approve,
        ApprovalStatus::Approved,
        &[ApprovalEvent::ApprovalApproved],
        ApprovalErrorCode::HumanRequired,
    ),
    (
        ApprovalStatus::Pending,
        ApprovalCommand::Reject,
        ApprovalStatus::Rejected,
        &[ApprovalEvent::ApprovalRejected],
        ApprovalErrorCode::ConcurrencyVersionConflict,
    ),
    (
        ApprovalStatus::Pending,
        ApprovalCommand::Expire,
        ApprovalStatus::Expired,
        &[ApprovalEvent::ApprovalExpired],
        ApprovalErrorCode::PreconditionFailed,
    ),
    (
        ApprovalStatus::Pending,
        ApprovalCommand::Cancel,
        ApprovalStatus::Cancelled,
        &[ApprovalEvent::ApprovalCancelled],
        ApprovalErrorCode::InvalidTransition,
    ),
];

const ALL_COMMANDS: [ApprovalCommand; 4] = ApprovalCommand::ALL;

#[test]
fn the_table_holds_exactly_the_specified_rows() {
    assert_eq!(
        APPROVAL_EDGES.len(),
        EXPECTED_ROWS.len(),
        "§7 has 4 lifecycle edges and no fan-out; the table holds {}",
        APPROVAL_EDGES.len()
    );
    for (from, command, to, events, rejection) in EXPECTED_ROWS {
        let t = transition(*from, *command)
            .unwrap_or_else(|r| panic!("§7 requires {from} --{command}--> {to}: {r}"));
        assert_eq!(t.from(), *from);
        assert_eq!(t.to(), *to, "§7 destination for {from} --{command}-->");
        assert_eq!(t.command(), *command);
        assert_eq!(
            t.events(),
            *events,
            "§7 Event cell for {from} --{command}-->"
        );
        assert_eq!(
            rejection_code(*from, *command),
            Some(*rejection),
            "{from} --{command}--> lost its §7 拒绝码"
        );
    }
}

#[test]
fn the_table_holds_no_edge_the_specification_omits() {
    for status in ApprovalStatus::ALL {
        for command in ALL_COMMANDS {
            let in_table = APPROVAL_EDGES
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
fn pending_is_the_only_state_with_outgoing_edges() {
    // §7's whole table hangs off PENDING. A row from any other state would let a
    // decided request change its mind, which §7 forbids.
    for status in ApprovalStatus::ALL {
        let out: Vec<ApprovalCommand> = ALL_COMMANDS
            .iter()
            .copied()
            .filter(|c| transition(status, *c).is_ok())
            .collect();
        if status == ApprovalStatus::Pending {
            assert_eq!(out.len(), 4, "§7's four rows all start at PENDING");
        } else {
            assert!(
                out.is_empty(),
                "§7 gives {status} no out-edge, but {out:?} was accepted"
            );
        }
    }
    assert_eq!(
        APPROVAL_EDGES
            .iter()
            .filter(|e| e.from != ApprovalStatus::Pending)
            .count(),
        0
    );
}

#[test]
fn every_terminal_state_absorbs_forever() {
    // §7: "全部终态不可重开；新请求必须新建 Approval."
    for status in [
        ApprovalStatus::Approved,
        ApprovalStatus::Rejected,
        ApprovalStatus::Expired,
        ApprovalStatus::Cancelled,
    ] {
        assert!(status.is_terminal(), "{status} must be terminal");
        for command in ALL_COMMANDS {
            assert!(
                transition(status, command).is_err(),
                "§7 forbids reopening a {status}, but {command} was accepted"
            );
        }
    }
    assert_eq!(
        APPROVAL_EDGES
            .iter()
            .filter(|e| e.from.is_terminal())
            .count(),
        0
    );
    // Not one of the four decisions can follow another, so a request can be
    // decided exactly once.
    for status in ApprovalStatus::ALL {
        for first in ALL_COMMANDS {
            for second in ALL_COMMANDS {
                if let Ok(t) = transition(status, first)
                    && t.to().is_terminal()
                {
                    assert!(
                        transition(t.to(), second).is_err(),
                        "{status} --{first}--> {} then --{second}--> would \
                         re-decide a settled request",
                        t.to()
                    );
                }
            }
        }
    }
}

#[test]
fn the_four_decisions_are_never_confusable() {
    // Each decision has its own event and its own rejection code, so a caller
    // cannot read "the request was refused" without learning which way it went.
    let mut events = Vec::new();
    let mut codes = Vec::new();
    for (command, to) in [
        (ApprovalCommand::Approve, ApprovalStatus::Approved),
        (ApprovalCommand::Reject, ApprovalStatus::Rejected),
        (ApprovalCommand::Expire, ApprovalStatus::Expired),
        (ApprovalCommand::Cancel, ApprovalStatus::Cancelled),
    ] {
        let t = transition(ApprovalStatus::Pending, command)
            .unwrap_or_else(|r| panic!("§7 PENDING --{command}--> {to}: {r}"));
        assert_eq!(t.to(), to);
        assert_eq!(t.events().len(), 1);
        assert!(
            !events.contains(&t.events()[0]),
            "{command} reuses an event name"
        );
        events.push(t.events()[0]);
        let code = rejection_code(ApprovalStatus::Pending, command).expect("edge has a code");
        codes.push(code);
    }
    assert_eq!(
        events.len(),
        4,
        "§7 has four distinct APPROVAL_* decision events"
    );
    // All four decision codes are distinct. CANCELLED's is
    // DOMAIN_INVALID_TRANSITION, which is §7's own choice even though it is also
    // what a genuinely absent edge reports.
    let mut unique = codes.clone();
    unique.sort_by_key(|c| c.as_str());
    unique.dedup_by_key(|c| c.as_str());
    assert_eq!(unique.len(), 4, "§7's four decision codes are all distinct");
}

#[test]
fn a_refused_cancel_keeps_the_invalid_transition_wire_string() {
    // §7 puts `DOMAIN_INVALID_TRANSITION` on `PENDING → CANCELLED`, meaning "the
    // cancel was refused because a decision already exists" — a state-level
    // refusal on a *legal* edge. The same string also reports a genuinely absent
    // edge. Both are §7's choice; the machine must not "fix" the first into
    // something else, and must not confuse the two.
    assert_eq!(
        rejection_code(ApprovalStatus::Pending, ApprovalCommand::Cancel),
        Some(ApprovalErrorCode::InvalidTransition)
    );
    let legal = transition(ApprovalStatus::Pending, ApprovalCommand::Cancel)
        .expect("PENDING --cancel--> CANCELLED is a legal edge");
    assert_eq!(legal.to(), ApprovalStatus::Cancelled);
    // A genuinely absent edge: same wire string, but no edge in the table.
    let absent = transition(ApprovalStatus::Approved, ApprovalCommand::Approve)
        .expect_err("§7 forbids re-approving a decided request");
    assert_eq!(absent.code().as_str(), legal_rejection_code_string());
    assert_eq!(absent.code(), ApprovalErrorCode::InvalidTransition);
    assert_eq!(absent.current(), ApprovalStatus::Approved);
    assert_eq!(absent.command(), ApprovalCommand::Approve);
}

fn legal_rejection_code_string() -> &'static str {
    "DOMAIN_INVALID_TRANSITION"
}

#[test]
fn the_catalogue_codes_spec_section_7_uses_but_we_do_not_invent() {
    // The catalogue also holds `APPROVAL_ALREADY_DECIDED` and
    // `APPROVAL_SELF_DECISION_DENIED`, which read like they belong on these rows.
    // §7 does not reference them, and inventing an "improvement" would be
    // changing a frozen protocol contract.
    let used = [
        "APPROVAL_SCOPE_INVALID",
        "AUTH_HUMAN_REQUIRED",
        "CONCURRENCY_VERSION_CONFLICT",
        "DOMAIN_INVALID_TRANSITION",
        "DOMAIN_PRECONDITION_FAILED",
    ];
    assert!(!used.contains(&"APPROVAL_ALREADY_DECIDED"));
    assert!(!used.contains(&"APPROVAL_SELF_DECISION_DENIED"));
    // §7's "decider 与 requester 不构成自批" is a *precondition*, not the code.
    assert_eq!(
        transition(ApprovalStatus::Pending, ApprovalCommand::Approve)
            .expect("§7 PENDING → APPROVED")
            .preconditions(),
        &[
            ApprovalPrecondition::NotExpired,
            ApprovalPrecondition::DeciderIsNotRequester,
            ApprovalPrecondition::ScopeWithinTeamCeiling,
        ]
    );
}

#[test]
fn an_illegal_transition_reports_the_catalogue_code() {
    for status in ApprovalStatus::ALL {
        for command in ALL_COMMANDS {
            if let Err(rejection) = transition(status, command) {
                assert_eq!(
                    rejection.code(),
                    ApprovalErrorCode::InvalidTransition,
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
    for edge in APPROVAL_EDGES {
        assert_eq!(
            rejection_code(edge.from, edge.command),
            Some(edge.rejection),
            "{} --{}--> lost its §7 rejection code",
            edge.from,
            edge.command
        );
    }
    for status in ApprovalStatus::ALL {
        for command in ALL_COMMANDS {
            if transition(status, command).is_err() {
                assert_eq!(rejection_code(status, command), None);
            }
        }
    }
}

#[test]
fn every_edge_declares_preconditions_and_events() {
    for edge in APPROVAL_EDGES {
        assert!(
            !edge.preconditions.is_empty(),
            "{} --{}--> has no preconditions; §7 lists at least one",
            edge.from,
            edge.command
        );
        assert_eq!(
            edge.events.len(),
            1,
            "{} --{}--> §7 gives exactly one event per row",
            edge.from,
            edge.command
        );
        for precondition in edge.preconditions {
            assert!(!precondition.condition_key().is_empty());
        }
        for event in edge.events {
            assert!(
                event.as_str().starts_with("APPROVAL_"),
                "{} is not an APPROVAL_* event name",
                event.as_str()
            );
        }
    }
    // Eight distinct preconditions across four edges, plus two on the creation row.
    let mut keys: Vec<&str> = APPROVAL_EDGES
        .iter()
        .flat_map(|e| e.preconditions.iter())
        .map(|p| p.condition_key())
        .collect();
    keys.sort_unstable();
    keys.dedup();
    assert_eq!(
        keys.len(),
        6,
        "§7's 4 edges name 6 distinct preconditions; got {keys:?}"
    );
    let with_creation: std::collections::BTreeSet<&str> = keys
        .iter()
        .copied()
        .chain(
            CREATE_PENDING_APPROVAL
                .preconditions()
                .iter()
                .map(|p| p.condition_key()),
        )
        .collect();
    assert_eq!(
        with_creation.len(),
        8,
        "counting creation, 8 distinct preconditions"
    );
}

#[test]
fn no_edge_is_a_self_loop() {
    for edge in APPROVAL_EDGES {
        assert_ne!(
            edge.to, edge.from,
            "{} --{}--> is a self-loop",
            edge.from, edge.command
        );
        let t = transition(edge.from, edge.command).expect("table row must apply");
        assert!(ApprovalStatus::ALL.contains(&t.to()));
    }
}

#[test]
fn creation_supports_exactly_pending() {
    assert_eq!(APPROVAL_CREATE_TARGETS.len(), 1);
    assert_eq!(APPROVAL_CREATE_TARGETS[0], ApprovalStatus::Pending);
    let creation = creation_target(ApprovalStatus::Pending).expect("创建 → PENDING is legal");
    assert_eq!(creation.initial(), ApprovalStatus::Pending);
    assert_eq!(creation.rejection(), ApprovalErrorCode::ScopeInvalid);
    assert_eq!(creation.events(), &[ApprovalEvent::ApprovalRequested]);
    assert_eq!(
        creation.preconditions(),
        &[
            ApprovalPrecondition::RequestFieldsComplete,
            ApprovalPrecondition::RequesterCannotSelfAuthorize,
        ],
        "§7: action、resource scope、理由、期限完整；请求者不能预授权"
    );
    for status in [
        ApprovalStatus::Approved,
        ApprovalStatus::Rejected,
        ApprovalStatus::Expired,
        ApprovalStatus::Cancelled,
    ] {
        assert!(
            creation_target(status).is_none(),
            "§7 has no 创建 --> {status} row; a new request starts a new Approval"
        );
    }
    // No edge lands in PENDING either — a decided request is never un-decided.
    assert_eq!(
        APPROVAL_EDGES
            .iter()
            .filter(|e| e.to == ApprovalStatus::Pending)
            .count(),
        0
    );
    assert_eq!(
        creation_target(ApprovalStatus::Pending),
        Some(&CREATE_PENDING_APPROVAL)
    );
}

#[test]
fn error_codes_match_the_frozen_catalogue() {
    let expected = [
        (
            ApprovalErrorCode::InvalidTransition,
            "DOMAIN_INVALID_TRANSITION",
            false,
        ),
        (
            ApprovalErrorCode::ScopeInvalid,
            "APPROVAL_SCOPE_INVALID",
            false,
        ),
        (
            ApprovalErrorCode::HumanRequired,
            "AUTH_HUMAN_REQUIRED",
            false,
        ),
        (
            ApprovalErrorCode::ConcurrencyVersionConflict,
            "CONCURRENCY_VERSION_CONFLICT",
            false,
        ),
        (
            ApprovalErrorCode::PreconditionFailed,
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
    // Not one §7 code is retryable in the frozen catalogue.
    assert!(expected.iter().all(|(_, _, r)| !*r));
}

#[test]
fn serde_agrees_with_as_str_for_every_code_and_event() {
    let codes = [
        (
            ApprovalErrorCode::InvalidTransition,
            "DOMAIN_INVALID_TRANSITION",
        ),
        (ApprovalErrorCode::ScopeInvalid, "APPROVAL_SCOPE_INVALID"),
        (ApprovalErrorCode::HumanRequired, "AUTH_HUMAN_REQUIRED"),
        (
            ApprovalErrorCode::ConcurrencyVersionConflict,
            "CONCURRENCY_VERSION_CONFLICT",
        ),
        (
            ApprovalErrorCode::PreconditionFailed,
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
    for status in ApprovalStatus::ALL {
        assert_eq!(
            serde_json::to_string(&status).unwrap(),
            format!("\"{}\"", status.as_str()),
            "serde and as_str disagree for {status:?}"
        );
    }
    let mut names: Vec<&str> = APPROVAL_EDGES
        .iter()
        .flat_map(|e| e.events.iter())
        .map(|e| e.as_str())
        .collect();
    names.push(CREATE_PENDING_APPROVAL.events()[0].as_str());
    names.sort_unstable();
    names.dedup();
    assert_eq!(
        names.len(),
        5,
        "§7 freezes exactly 5 APPROVAL_* event names"
    );
    for edge in APPROVAL_EDGES {
        for event in edge.events {
            assert_eq!(
                serde_json::to_string(event).unwrap(),
                format!("\"{}\"", event.as_str()),
                "serde and as_str disagree for {event:?}"
            );
        }
    }
}

#[test]
fn the_machine_is_pure_and_deterministic() {
    for status in ApprovalStatus::ALL {
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
