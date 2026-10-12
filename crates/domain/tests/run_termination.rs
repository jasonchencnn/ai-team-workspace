//! `STATE_MACHINES.md` §3 cancel and timeout decisions.
//!
//! The load-bearing tests here are the cross-checks against `RUN_EDGES`
//! (`every_confirmed_cancel_is_an_edge_run_state_machine_accepts` and
//! `every_time_out_verdict_is_an_edge_run_state_machine_accepts`): this module
//! is supposed to add §3's *precondition text* to the transition graph, never a
//! new edge. The other two that matter are
//! `each_cancel_observation_binds_to_exactly_the_rows_that_name_it`, which pins
//! the whole cancel matrix cell by cell, and
//! `proved_running_offers_no_section_3_transition`, which pins the one verdict
//! that is a derived reading rather than a spec row.

use std::collections::HashSet;

use teamspace_domain::{
    CancelDecision, CancelObservation, ExecutionObservation, ReconcileEvidence, RunCommand,
    RunErrorCode, RunStatus, TimeoutDecision, decide_cancel, decide_timeout, run_rejection_code,
    run_transition,
};

/// Every verdict that authorises a write must correspond to a §3 edge that
/// `RUN_EDGES` already accepts, for the same `(from, command)` pair.
#[test]
fn every_confirmed_cancel_is_an_edge_run_state_machine_accepts() {
    for status in RunStatus::ALL {
        for observation in CancelObservation::ALL {
            if let CancelDecision::Confirm { target } = decide_cancel(status, observation) {
                let transition =
                    run_transition(status, RunCommand::Cancel).unwrap_or_else(|error| {
                        panic!("{status} + {observation:?} confirmed {target} but {error:?}")
                    });
                assert_eq!(transition.to(), target, "{status} + {observation:?}");
            }
        }
    }
}

/// The timeout twin of the same invariant.
#[test]
fn every_time_out_verdict_is_an_edge_run_state_machine_accepts() {
    for status in RunStatus::ALL {
        for observation in ExecutionObservation::ALL {
            let verdict = decide_timeout(status, observation);
            if let TimeoutDecision::TimeOut { target }
            | TimeoutDecision::TimeOutFromReconcile { target } = verdict
            {
                let transition =
                    run_transition(status, RunCommand::Timeout).unwrap_or_else(|error| {
                        panic!("{status} + {observation:?} timed out as {target} but {error:?}")
                    });
                assert_eq!(transition.to(), target, "{status} + {observation:?}");
            }
        }
    }
}

/// The reverse direction: a refusal must never name a code §3 froze for a
/// different row. `run_rejection_code` is the derived reading, so the codes this
/// module invents are checked against it rather than against prose.
#[test]
fn every_refusal_code_matches_the_derived_edge_rejection() {
    for status in RunStatus::ALL {
        for observation in CancelObservation::ALL {
            if let CancelDecision::Refuse { error_code } = decide_cancel(status, observation) {
                let expected =
                    run_rejection_code(status, RunCommand::Cancel).unwrap_or_else(|| {
                        panic!("{status} refused with {error_code} but §3 has no cancel row at all")
                    });
                assert_eq!(error_code, expected, "{status} + {observation:?}");
            }
        }
    }
}

/// §3 words its three cancel rows differently, and every cell of the matrix
/// follows from the words in that row and nothing else:
///
/// ```text
/// QUEUED   → CANCELLED │ 尚未开始或已安全停止
/// STARTING → CANCELLED │ cancel 确认或达到强制 fence 条件
/// RUNNING  → CANCELLED │ cancel 确认；或 Human takeover 先 fence
/// ```
///
/// The two cells an earlier version of this matrix got wrong are the ones this
/// test exists for: 「强制 fence」 appears only in `STARTING`'s row, and
/// `RUNNING`'s alternative is qualified by a Human takeover that 「先」 must
/// precede it. A bare forced fence satisfies neither.
#[test]
fn each_cancel_observation_binds_to_exactly_the_rows_that_name_it() {
    let cancellable = [RunStatus::Queued, RunStatus::Starting, RunStatus::Running];
    // Written as the specification words it, not as the code accepts, so a change
    // to either side shows up as a diff against the spec rather than a rewrite.
    let expected: HashSet<(CancelObservation, RunStatus)> = [
        // cancel 确认 — the word shared by all three rows.
        (CancelObservation::Confirmed, RunStatus::Queued),
        (CancelObservation::Confirmed, RunStatus::Starting),
        (CancelObservation::Confirmed, RunStatus::Running),
        // 尚未开始 — QUEUED's row only.
        (CancelObservation::NeverStarted, RunStatus::Queued),
        // 达到强制 fence 条件 — STARTING's row only.
        (CancelObservation::ForceFenceReached, RunStatus::Starting),
        // Human takeover 先 fence — RUNNING's row only.
        (CancelObservation::TakeoverFenced, RunStatus::Running),
    ]
    .into_iter()
    .collect();

    let mut actual = HashSet::new();
    for status in RunStatus::ALL {
        for observation in CancelObservation::ALL {
            if observation.satisfies(status) {
                assert!(
                    cancellable.contains(&status),
                    "{status} is not one of §3's three cancel rows, yet {observation:?} satisfies it"
                );
                actual.insert((observation, status));
            }
        }
    }
    assert_eq!(
        actual, expected,
        "the cancel matrix drifted from §3's wording"
    );

    // The three refutations, spelled out so a regression names the cell.
    assert!(!CancelObservation::Requested.satisfies(RunStatus::Queued));
    assert!(!CancelObservation::ForceFenceReached.satisfies(RunStatus::Queued));
    assert!(!CancelObservation::ForceFenceReached.satisfies(RunStatus::Running));
    assert!(!CancelObservation::TakeoverFenced.satisfies(RunStatus::Starting));
    assert!(!CancelObservation::TakeoverFenced.satisfies(RunStatus::Queued));
    assert!(!CancelObservation::NeverStarted.satisfies(RunStatus::Running));
    assert!(!CancelObservation::NeverStarted.satisfies(RunStatus::Starting));
}

/// `NeverStarted` is 尚未开始, and §3 gives that word to one row.
#[test]
fn never_started_only_satisfies_queued() {
    assert!(CancelObservation::NeverStarted.satisfies(RunStatus::Queued));
    for status in RunStatus::ALL {
        if status != RunStatus::Queued {
            assert!(
                !CancelObservation::NeverStarted.satisfies(status),
                "{status} is by definition not 尚未开始"
            );
        }
    }
}

/// An unacknowledged request is not a cancellation, and each row reports the
/// code §3 froze for it.
#[test]
fn requested_is_refused_with_each_rows_own_code() {
    assert_eq!(
        decide_cancel(RunStatus::Queued, CancelObservation::Requested),
        CancelDecision::Refuse {
            error_code: RunErrorCode::PermissionDenied
        }
    );
    assert_eq!(
        decide_cancel(RunStatus::Starting, CancelObservation::Requested),
        CancelDecision::Refuse {
            error_code: RunErrorCode::RunCancelNotConfirmed
        }
    );
    assert_eq!(
        decide_cancel(RunStatus::Running, CancelObservation::Requested),
        CancelDecision::Refuse {
            error_code: RunErrorCode::PermissionDenied
        }
    );
}

/// `UNKNOWN` is the case this module got wrong first. §3 **does** have an
/// `UNKNOWN → CANCELLED` row, so refusing it as "no such row" is a false claim
/// about the specification; but that row belongs to the reconcile service, so
/// neither a cancel nor a timeout command may produce it.
#[test]
fn unknown_is_reconcile_only_not_missing_row() {
    for observation in CancelObservation::ALL {
        assert_eq!(
            decide_cancel(RunStatus::Unknown, observation),
            CancelDecision::ReconcileOnly {
                status: RunStatus::Unknown
            },
            "{observation:?}"
        );
    }
    for observation in ExecutionObservation::ALL {
        assert_eq!(
            decide_timeout(RunStatus::Unknown, observation),
            TimeoutDecision::ReconcileOnly {
                status: RunStatus::Unknown
            },
            "{observation:?}"
        );
    }

    // The reconcile command is the legal route, and it is the one DEV-03-08 owns.
    // Note the two codes: `Cancel` from `UNKNOWN` has no edge at all, so
    // `transition` reports `DOMAIN_INVALID_TRANSITION` — you used the wrong
    // command. The reconcile edge itself carries §3's
    // `DOMAIN_PRECONDITION_FAILED`, which is what a caller gets once it has
    // correctly routed through reconcile and the evidence is missing.
    assert_eq!(
        run_transition(RunStatus::Unknown, RunCommand::Cancel)
            .expect_err("UNKNOWN has no cancel-command row in RUN_EDGES")
            .code(),
        RunErrorCode::InvalidTransition
    );
    assert_eq!(
        run_rejection_code(RunStatus::Unknown, RunCommand::Cancel),
        None
    );
    assert_eq!(
        run_transition(
            RunStatus::Unknown,
            RunCommand::Reconcile(teamspace_domain::ReconcileOutcome::Cancelled)
        )
        .expect("§3's UNKNOWN → FAILED/TIMED_OUT/CANCELLED row is the reconcile row")
        .to(),
        RunStatus::Cancelled
    );
    assert_eq!(
        run_rejection_code(
            RunStatus::Unknown,
            RunCommand::Reconcile(teamspace_domain::ReconcileOutcome::Cancelled)
        ),
        Some(RunErrorCode::PreconditionFailed)
    );
}

/// The four terminal states are closed: §3 calls them 终态.
#[test]
fn terminal_states_are_not_cancellable() {
    for status in RunStatus::ALL {
        if !status.is_terminal() {
            continue;
        }
        for observation in CancelObservation::ALL {
            assert_eq!(
                decide_cancel(status, observation),
                CancelDecision::NotCancellable { status },
                "{status} + {observation:?}"
            );
        }
    }
    assert_eq!(
        RunStatus::ALL
            .iter()
            .filter(|status| status.is_terminal())
            .count(),
        4,
        "§3: SUCCEEDED/FAILED/TIMED_OUT/CANCELLED 为单个 Run 终态"
    );
}

/// `STARTING → TIMED_OUT │ 超过 start timeout` carries no reconcile clause, so a
/// `STARTING` Run times out on the clock alone.
#[test]
fn starting_times_out_on_the_deadline_alone() {
    assert_eq!(
        decide_timeout(
            RunStatus::Starting,
            ExecutionObservation::Starting {
                start_deadline_passed: false
            }
        ),
        TimeoutDecision::NotYet
    );
    assert_eq!(
        decide_timeout(
            RunStatus::Starting,
            ExecutionObservation::Starting {
                start_deadline_passed: true
            }
        ),
        TimeoutDecision::TimeOut {
            target: RunStatus::TimedOut
        }
    );
}

/// §3's two timeout rows ask two different questions, and only the second
/// one's answer is the timeout gate:
///
/// * `RUNNING → TIMED_OUT` ｜ 心跳/总时长超限**且对账仍无法证明运行**
/// * `RUNNING → UNKNOWN` ｜ 平台重启/连接丢失**且事实不明**
///
/// The second is the state *before* any reconcile result. Collapsing them into
/// one boolean named after the first is how a completed-tense reading of that
/// name — "reconcile has proved it is running", the exact opposite of what §3
/// needs — would slip in and let a live Run be timed out.
#[test]
fn each_reconcile_evidence_state_gets_its_own_verdict() {
    let expired = |evidence| ExecutionObservation::Running {
        heartbeat_deadline_passed: true,
        total_deadline_passed: false,
        evidence,
    };
    // 对账仍无法证明运行 — the only state §3's timeout gate accepts.
    assert_eq!(
        decide_timeout(
            RunStatus::Running,
            expired(ReconcileEvidence::CouldNotProve)
        ),
        TimeoutDecision::TimeOutFromReconcile {
            target: RunStatus::TimedOut
        }
    );
    // 事实不明 — `RUNNING → UNKNOWN`.
    assert_eq!(
        decide_timeout(
            RunStatus::Running,
            expired(ReconcileEvidence::Indeterminate)
        ),
        TimeoutDecision::NeedsReconcile {
            target: RunStatus::Unknown
        }
    );
    // Liveness proved: neither row applies.
    assert_eq!(
        decide_timeout(
            RunStatus::Running,
            expired(ReconcileEvidence::ProvedRunning)
        ),
        TimeoutDecision::KeepRunning
    );

    // Before the deadline, evidence is irrelevant — §3's gate is a conjunction.
    for evidence in ReconcileEvidence::ALL {
        assert_eq!(
            decide_timeout(
                RunStatus::Running,
                ExecutionObservation::Running {
                    heartbeat_deadline_passed: false,
                    total_deadline_passed: false,
                    evidence,
                }
            ),
            TimeoutDecision::NotYet,
            "{evidence:?}"
        );
    }
}

/// `blocks_timeout` is the one-line restatement of §3's gate, kept next to the
/// three-state test so the two cannot disagree.
#[test]
fn only_could_not_prove_unblocks_the_timeout() {
    for evidence in ReconcileEvidence::ALL {
        let expected = !matches!(evidence, ReconcileEvidence::CouldNotProve);
        assert_eq!(evidence.blocks_timeout(), expected, "{evidence:?}");
    }
    assert!(!ReconcileEvidence::CouldNotProve.blocks_timeout());
    assert!(ReconcileEvidence::Indeterminate.blocks_timeout());
    assert!(ReconcileEvidence::ProvedRunning.blocks_timeout());
}

/// A proven-live Run must be offered no §3 transition, and the reason is the
/// exclusion of both rows rather than a rule the specification states.
#[test]
fn proved_running_offers_no_section_3_transition() {
    let verdict = decide_timeout(
        RunStatus::Running,
        ExecutionObservation::Running {
            heartbeat_deadline_passed: true,
            total_deadline_passed: true,
            evidence: ReconcileEvidence::ProvedRunning,
        },
    );
    assert_eq!(verdict, TimeoutDecision::KeepRunning);
    // `RUNNING → TIMED_OUT` needs 对账仍无法证明运行; a proof of liveness
    // contradicts it, so `run_transition` was never going to accept it anyway.
    assert!(run_transition(RunStatus::Running, RunCommand::Timeout).is_ok());
    assert!(run_transition(RunStatus::Running, RunCommand::MarkUnknown).is_ok());
    // The verdict therefore commits to neither, which is what makes it a
    // derived reading: it asserts the absence of a transition, and the tests
    // above are the ones that hold that assertion in place.
    assert!(!matches!(
        verdict,
        TimeoutDecision::TimeOut { .. } | TimeoutDecision::TimeOutFromReconcile { .. }
    ));
    assert!(!matches!(verdict, TimeoutDecision::NeedsReconcile { .. }));
}

/// `NeedsReconcile` writes `UNKNOWN`, so that too must be a real §3 edge.
#[test]
fn the_needs_reconcile_target_is_a_real_edge() {
    let transition = run_transition(RunStatus::Running, RunCommand::MarkUnknown)
        .expect("§3 gives RUNNING → UNKNOWN to the recovery service");
    assert_eq!(transition.to(), RunStatus::Unknown);
    assert_eq!(
        decide_timeout(
            RunStatus::Running,
            ExecutionObservation::Running {
                heartbeat_deadline_passed: true,
                total_deadline_passed: false,
                evidence: ReconcileEvidence::Indeterminate,
            }
        ),
        TimeoutDecision::NeedsReconcile {
            target: RunStatus::Unknown
        }
    );
}

/// Either deadline alone is enough; §3 says 心跳/总时长, disjunctively.
#[test]
fn either_deadline_alone_makes_running_time_out() {
    for (heartbeat, total) in [(true, false), (false, true), (true, true)] {
        assert_eq!(
            decide_timeout(
                RunStatus::Running,
                ExecutionObservation::Running {
                    heartbeat_deadline_passed: heartbeat,
                    total_deadline_passed: total,
                    evidence: ReconcileEvidence::CouldNotProve,
                }
            ),
            TimeoutDecision::TimeOutFromReconcile {
                target: RunStatus::TimedOut
            },
            "heartbeat={heartbeat} total={total}"
        );
    }
    assert_eq!(
        decide_timeout(
            RunStatus::Running,
            ExecutionObservation::Running {
                heartbeat_deadline_passed: false,
                total_deadline_passed: false,
                evidence: ReconcileEvidence::CouldNotProve,
            }
        ),
        TimeoutDecision::NotYet
    );
}

/// `deadline_passed()` is the only shared helper, so it is checked on its own
/// rather than only through the verdicts that use it.
#[test]
fn deadline_passed_is_a_disjunction_over_the_applicable_deadlines() {
    for observation in ExecutionObservation::ALL {
        let expected = match observation {
            ExecutionObservation::Starting {
                start_deadline_passed,
            } => start_deadline_passed,
            ExecutionObservation::Running {
                heartbeat_deadline_passed,
                total_deadline_passed,
                ..
            } => heartbeat_deadline_passed || total_deadline_passed,
        };
        assert_eq!(observation.deadline_passed(), expected, "{observation:?}");
    }
    // 2 `Starting` plus 3 evidence states × 4 deadline combinations.
    assert_eq!(ExecutionObservation::ALL.len(), 14);
}

/// An observation that does not match the status is not a timeout. Reporting
/// `STARTING`'s deadline for a `RUNNING` Run would be a caller bug this module
/// should not paper over.
#[test]
fn mismatched_observation_is_not_a_timeout() {
    let running_observation = ExecutionObservation::Running {
        heartbeat_deadline_passed: true,
        total_deadline_passed: true,
        evidence: ReconcileEvidence::CouldNotProve,
    };
    let starting_observation = ExecutionObservation::Starting {
        start_deadline_passed: true,
    };
    assert_eq!(
        decide_timeout(RunStatus::Starting, running_observation),
        TimeoutDecision::NotYet
    );
    assert_eq!(
        decide_timeout(RunStatus::Running, starting_observation),
        TimeoutDecision::NotYet
    );
}

/// Terminal states are closed, and `QUEUED` has no timeout row.
#[test]
fn statuses_without_a_timeout_row_are_not_yet() {
    for status in [
        RunStatus::Queued,
        RunStatus::Succeeded,
        RunStatus::Failed,
        RunStatus::TimedOut,
        RunStatus::Cancelled,
    ] {
        for observation in ExecutionObservation::ALL {
            assert_eq!(
                decide_timeout(status, observation),
                TimeoutDecision::NotYet,
                "{status} + {observation:?}"
            );
        }
    }
}

/// The enumeration constants must enumerate without duplicates, checked twice:
/// a `const` assertion folded into each `ALL` initializer, which reports a
/// duplicate at test-compile time by name, and the runtime assertions below.
///
/// This is deliberately **not** a completeness claim. Nothing available without
/// a derive can compare an enum's variant count against a hand-written list, and
/// claiming otherwise is what independent review caught in the first draft. See
/// `RunErrorCode::ALL` for what stands in its place and for the exact timing of
/// the compile-time half.
#[test]
fn the_enumerations_are_distinct_and_cover_every_index() {
    assert_eq!(RunStatus::ALL.len(), 8);
    assert_eq!(CancelObservation::ALL.len(), 5);
    assert_eq!(ReconcileEvidence::ALL.len(), 3);
    assert_eq!(ExecutionObservation::ALL.len(), 14);

    for status in RunStatus::ALL {
        assert_eq!(
            RunStatus::ALL
                .iter()
                .filter(|candidate| **candidate == status)
                .count(),
            1,
            "{status} appears twice in RunStatus::ALL"
        );
    }
    for observation in CancelObservation::ALL {
        assert_eq!(
            CancelObservation::ALL
                .iter()
                .filter(|candidate| **candidate == observation)
                .count(),
            1,
            "{observation:?} appears twice in CancelObservation::ALL"
        );
    }
    for evidence in ReconcileEvidence::ALL {
        assert_eq!(
            ReconcileEvidence::ALL
                .iter()
                .filter(|candidate| **candidate == evidence)
                .count(),
            1,
            "{evidence:?} appears twice in ReconcileEvidence::ALL"
        );
    }
    for observation in ExecutionObservation::ALL {
        assert_eq!(
            ExecutionObservation::ALL
                .iter()
                .filter(|candidate| **candidate == observation)
                .count(),
            1,
            "{observation:?} appears twice in ExecutionObservation::ALL"
        );
    }
}

/// `Display` is what reaches the diagnostic, so it must name the status, the
/// target and the frozen code — and must not read as "no such row" for
/// `UNKNOWN`.
#[test]
fn display_names_the_status_target_and_code() {
    assert_eq!(
        decide_cancel(RunStatus::Starting, CancelObservation::Requested).to_string(),
        format!("cancel refused: {}", RunErrorCode::RunCancelNotConfirmed)
    );
    assert_eq!(
        decide_cancel(RunStatus::Queued, CancelObservation::Confirmed).to_string(),
        "cancel confirmed → CANCELLED"
    );
    assert_eq!(
        decide_cancel(RunStatus::Unknown, CancelObservation::Confirmed).to_string(),
        "UNKNOWN reaches CANCELLED only via reconcile"
    );
    assert_eq!(
        decide_cancel(RunStatus::Succeeded, CancelObservation::Confirmed).to_string(),
        "SUCCEEDED has no §3 → CANCELLED row"
    );
    assert_eq!(
        decide_timeout(
            RunStatus::Starting,
            ExecutionObservation::Starting {
                start_deadline_passed: true
            }
        )
        .to_string(),
        "deadline passed → TIMED_OUT"
    );
    let running = |evidence| ExecutionObservation::Running {
        heartbeat_deadline_passed: true,
        total_deadline_passed: false,
        evidence,
    };
    assert_eq!(
        decide_timeout(
            RunStatus::Running,
            running(ReconcileEvidence::Indeterminate)
        )
        .to_string(),
        "no reconcile result yet, fact 事实不明 → UNKNOWN"
    );
    assert_eq!(
        decide_timeout(
            RunStatus::Running,
            running(ReconcileEvidence::ProvedRunning)
        )
        .to_string(),
        "reconcile proved it still running; no §3 row applies"
    );
    assert_eq!(
        decide_timeout(
            RunStatus::Running,
            running(ReconcileEvidence::CouldNotProve)
        )
        .to_string(),
        "reconcile could not prove it running → TIMED_OUT"
    );
    assert_eq!(TimeoutDecision::NotYet.to_string(), "no timeout");
}
