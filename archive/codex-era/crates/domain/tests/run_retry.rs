//! `ERROR_CATALOG.md` §11 retry admission, with `STATE_MACHINES.md` §3's
//! 自动重试只读取终态 Run，创建新 Run.
//!
//! Two things carry the weight here. `every_retryable_code_may_create_a_new_run`
//! pins §11's 默认最多 3 次 by walking the counter to exhaustion. And
//! `no_decision_can_mutate_the_source_run` walks every `(status, failure)`
//! combination and asserts that the only status any verdict ever carries is the
//! one it was given or `QUEUED` — so 禁止把旧 Run 改回 QUEUED is checked, not
//! just documented.

use std::collections::HashSet;

use teamspace_domain::{
    CREATE_QUEUED, DEFAULT_MAX_RETRIES, RUN_EDGES, RetryDecision, RetryLimit, RetryPolicy,
    RunErrorCode, RunEvent, RunFailure, RunPrecondition, RunStatus, decide_retry,
};

/// Every §3 code, taken from the source of truth rather than re-listed
/// here — a hand-kept copy already missed one code once.
const ALL_CODES: [RunErrorCode; 9] = RunErrorCode::ALL;

const RETRYABLE: [RunErrorCode; 3] = [
    RunErrorCode::AlreadyClaimed,
    RunErrorCode::RunCancelNotConfirmed,
    RunErrorCode::AdapterUnavailable,
];

fn policy() -> RetryPolicy {
    RetryPolicy::new(50)
}

fn decide(
    source: RunStatus,
    failure: RunFailure,
    idempotent: bool,
    retries_so_far: u32,
    spent_millis: i64,
) -> RetryDecision {
    decide_retry(
        source,
        failure,
        idempotent,
        policy(),
        retries_so_far,
        spent_millis,
    )
}

/// §3: 自动重试只读取终态 Run. A Run that has not finished is not retry
/// material, whatever else is true of it.
#[test]
fn a_non_terminal_run_is_never_retry_material() {
    for status in RunStatus::ALL {
        if status.is_terminal() {
            continue;
        }
        for code in ALL_CODES {
            assert_eq!(
                decide(status, RunFailure::Code(code), true, 0, 0),
                RetryDecision::NotTerminal { status },
                "{status} + {code}"
            );
        }
        assert_eq!(
            decide(status, RunFailure::InternalUnexpected, true, 0, 0),
            RetryDecision::NotTerminal { status },
            "{status} + INTERNAL_UNEXPECTED"
        );
    }
    assert_eq!(
        RunStatus::ALL
            .iter()
            .filter(|status| !status.is_terminal())
            .count(),
        4
    );
}

/// §11 rule 1's first half, checked against the catalogue's `否` column.
#[test]
fn a_non_retryable_code_is_never_replayed() {
    for code in ALL_CODES {
        if RETRYABLE.contains(&code) {
            continue;
        }
        // Both idempotency answers, so the refusal cannot be blamed on it.
        for idempotent in [true, false] {
            assert_eq!(
                decide(RunStatus::Failed, RunFailure::Code(code), idempotent, 0, 0),
                RetryDecision::DoNotRetry { code },
                "{code} idempotent={idempotent}"
            );
        }
    }
}

/// §11 rule 3 is a restatement of rule 1's negative side for this machine, so
/// it is checked as the mapping rather than as a second list.
///
/// Only three of rule 3's six concepts have a §3 Run code — 权限, 非法状态 and
/// fencing. 版本冲突, 校验 and TTL 过期 cannot reach this module at all, so what
/// this test asserts about them is the weaker and only true thing: that the
/// catalogue marks them `否`. An earlier version of this test claimed
/// `BINDING_STALE_VERSION` and `DOMAIN_PRECONDITION_FAILED` stood in for
/// 版本冲突 and 校验/TTL 过期; neither code means that, and naming them as if it
/// did was an invention presented as a lookup.
#[test]
fn section_11_rule_three_lands_on_do_not_retry() {
    let rule_three_with_a_run_code = [
        RunErrorCode::PermissionDenied,
        RunErrorCode::InvalidTransition,
        RunErrorCode::RunFenced,
    ];
    for code in rule_three_with_a_run_code {
        assert!(!code.is_retryable(), "{code} must not be retryable");
        assert_eq!(
            decide(RunStatus::Failed, RunFailure::Code(code), true, 0, 0),
            RetryDecision::DoNotRetry { code }
        );
    }

    // The other three concepts live outside §3. What can be checked from here is
    // that none of the codes standing in for them is retryable, so a caller
    // reaching this module with one of them still lands on `DoNotRetry`.
    let near_misses = [
        // 版本冲突 的真码是 CONCURRENCY_VERSION_CONFLICT，不在 §3；这里是 Binding 变更。
        RunErrorCode::BindingStaleVersion,
        // §3 从未给 `QUEUED` 或终态列出过 `→ TIMED_OUT` 行，也没有校验或 TTL 码。
        RunErrorCode::PreconditionFailed,
    ];
    for code in near_misses {
        assert!(!code.is_retryable(), "{code} must not be retryable");
        assert_eq!(
            decide(RunStatus::Failed, RunFailure::Code(code), true, 0, 0),
            RetryDecision::DoNotRetry { code }
        );
    }
}

/// §11 rule 1's second half: 且命令本身幂等.
#[test]
fn a_non_idempotent_command_is_not_replayed_even_when_the_code_says_yes() {
    for code in RETRYABLE {
        assert_eq!(
            decide(RunStatus::Failed, RunFailure::Code(code), false, 0, 0),
            RetryDecision::NotIdempotent {
                source: RunStatus::Failed
            },
            "{code}"
        );
    }
}

/// §11 rule 4: 禁止盲目重放可能产生外部副作用的命令 — and the catalogue marks
/// `INTERNAL_UNEXPECTED` `是`, so rule 1 alone would admit it. Rule 4 overrides.
#[test]
fn internal_unexpected_queries_the_first_result_before_anything_else() {
    // Even an idempotent command must query first: the point of rule 4 is that
    // a lost response is indistinguishable from a lost side effect.
    for idempotent in [true, false] {
        assert_eq!(
            decide(
                RunStatus::Failed,
                RunFailure::InternalUnexpected,
                idempotent,
                0,
                0
            ),
            RetryDecision::QueryFirstResult {
                source: RunStatus::Failed
            },
            "idempotent={idempotent}"
        );
    }
    assert_eq!(
        RunFailure::InternalUnexpected.as_str(),
        "INTERNAL_UNEXPECTED"
    );
}

/// Rule 4 also outranks the budget: a query is not a retry, so an exhausted
/// budget must not turn "look it up first" into "look it up later, maybe".
#[test]
fn rule_four_outranks_the_budget() {
    assert_eq!(
        decide(
            RunStatus::Failed,
            RunFailure::InternalUnexpected,
            true,
            u32::MAX,
            i64::MAX
        ),
        RetryDecision::QueryFirstResult {
            source: RunStatus::Failed
        }
    );
}

/// §11 rule 2: 自动重试默认最多 3 次. Walked to exhaustion rather than asserted
/// at one point, so an off-by-one in either direction shows up.
#[test]
fn every_retryable_code_may_create_a_new_run_until_the_ceiling() {
    assert_eq!(DEFAULT_MAX_RETRIES, 3, "§11: 默认最多 3 次");
    for code in RETRYABLE {
        for retries_so_far in 0..DEFAULT_MAX_RETRIES {
            let decision = decide(
                RunStatus::Failed,
                RunFailure::Code(code),
                true,
                retries_so_far,
                0,
            );
            assert_eq!(
                decision,
                RetryDecision::CreateNewRun {
                    source: RunStatus::Failed,
                    creation: CREATE_QUEUED,
                },
                "{code} after {retries_so_far} retries"
            );
        }
        assert_eq!(
            decide(
                RunStatus::Failed,
                RunFailure::Code(code),
                true,
                DEFAULT_MAX_RETRIES,
                0
            ),
            // The ceiling is reached with budget to spare, so the computed
            // delay is still non-zero: 200 ms base x 2^3, plus 50 ms jitter.
            RetryDecision::BudgetExhausted {
                limit: RetryLimit::Count,
                delay_millis: 1_650,
            },
            "{code} at the ceiling"
        );
    }
}

/// §11 rule 2's second clause: 总时长不得超过命令/Message TTL. The ceiling is
/// necessary but not sufficient — three retries that each wait longer than the
/// TTL are still over it, and that is the budget's job.
#[test]
fn the_total_budget_stops_a_run_before_the_count_ceiling() {
    let over_ttl = policy().total_budget_millis;
    let decision = decide(
        RunStatus::Failed,
        RunFailure::Code(RunErrorCode::AdapterUnavailable),
        true,
        0,
        over_ttl,
    );
    assert_eq!(
        decision,
        RetryDecision::BudgetExhausted {
            limit: RetryLimit::Duration,
            delay_millis: 0,
        },
        "an exhausted budget schedules no delay, matching RetryBudget"
    );
    // One millisecond short of the TTL is still inside it.
    assert!(
        decide(
            RunStatus::Failed,
            RunFailure::Code(RunErrorCode::AdapterUnavailable),
            true,
            0,
            over_ttl - 1
        )
        .creates_new_run()
    );
}

/// §3: 创建新 Run, 禁止把旧 Run 改回 QUEUED. Checked mechanically over the
/// whole input space: the only status a verdict may carry is the one it was
/// handed, or `QUEUED` as the new Run's creation target.
#[test]
fn no_decision_can_mutate_the_source_run() {
    for source in RunStatus::ALL {
        for failure in ALL_CODES
            .map(RunFailure::Code)
            .into_iter()
            .chain([RunFailure::InternalUnexpected])
        {
            for idempotent in [true, false] {
                for retries_so_far in [0, 1, 2, 3, u32::MAX] {
                    for spent in [0, 1, 60_000, i64::MAX] {
                        let decision = decide(source, failure, idempotent, retries_so_far, spent);
                        for carried in statuses_carried_by(decision) {
                            assert!(
                                carried == source || carried == RunStatus::Queued,
                                "{source} + {failure:?} produced {decision:?}, which carries {carried}"
                            );
                        }
                    }
                }
            }
        }
    }
}

/// Every `RunStatus` a verdict mentions. Kept next to the assertion that
/// constrains it, so adding a field to a variant forces a decision here.
fn statuses_carried_by(decision: RetryDecision) -> Vec<RunStatus> {
    match decision {
        RetryDecision::CreateNewRun { source, creation } => vec![source, creation.initial()],
        RetryDecision::QueryFirstResult { source } => vec![source],
        RetryDecision::NotIdempotent { source } => vec![source],
        RetryDecision::NotTerminal { status } => vec![status],
        RetryDecision::DoNotRetry { .. } | RetryDecision::BudgetExhausted { .. } => Vec::new(),
    }
}

/// The creation carried by the one writing verdict must be §3's frozen
/// `创建 → QUEUED`, with the precondition and event the catalogue froze for it.
#[test]
fn the_new_run_is_created_through_the_frozen_creation_contract() {
    let decision = decide(
        RunStatus::TimedOut,
        RunFailure::Code(RunErrorCode::AdapterUnavailable),
        true,
        0,
        0,
    );
    let creation = decision.created_run().expect("a positive verdict");
    assert_eq!(creation.initial(), RunStatus::Queued);
    assert_eq!(
        creation.preconditions(),
        &[RunPrecondition::LeaseObtainable]
    );
    assert_eq!(creation.events(), &[RunEvent::RunQueued]);
    assert_eq!(creation.rejection(), RunErrorCode::AlreadyClaimed);
    assert!(decision.creates_new_run());

    // Only that one variant creates a Run.
    for negative in [
        RetryDecision::QueryFirstResult {
            source: RunStatus::Failed,
        },
        RetryDecision::DoNotRetry {
            code: RunErrorCode::RunFenced,
        },
        RetryDecision::NotIdempotent {
            source: RunStatus::Failed,
        },
        RetryDecision::NotTerminal {
            status: RunStatus::Running,
        },
        RetryDecision::BudgetExhausted {
            limit: RetryLimit::Count,
            delay_millis: 0,
        },
    ] {
        assert!(!negative.creates_new_run(), "{negative:?}");
        assert_eq!(negative.created_run(), None, "{negative:?}");
    }
}

/// Every terminal status is retry material on the same terms — §3 closes all
/// four, and none of them is privileged by a status argument.
#[test]
fn all_four_terminal_statuses_behave_identically() {
    for source in RunStatus::ALL {
        if !source.is_terminal() {
            continue;
        }
        assert_eq!(
            decide(
                source,
                RunFailure::Code(RunErrorCode::AdapterUnavailable),
                true,
                0,
                0
            ),
            RetryDecision::CreateNewRun {
                source,
                creation: CREATE_QUEUED,
            },
            "{source}"
        );
        assert_eq!(
            decide(
                source,
                RunFailure::Code(RunErrorCode::RunFenced),
                true,
                0,
                0
            ),
            RetryDecision::DoNotRetry {
                code: RunErrorCode::RunFenced
            },
            "{source}"
        );
    }
}

/// The precedence chosen in `decide_retry`, pinned so it cannot drift silently.
#[test]
fn precedence_is_terminal_then_code_then_rule_four_then_idempotent_then_budget() {
    // A non-terminal Run outranks even a definitive code refusal.
    assert_eq!(
        decide(
            RunStatus::Running,
            RunFailure::Code(RunErrorCode::RunFenced),
            true,
            0,
            0
        ),
        RetryDecision::NotTerminal {
            status: RunStatus::Running
        }
    );
    // A definitive code refusal outranks the contingent idempotency question.
    assert_eq!(
        decide(
            RunStatus::Failed,
            RunFailure::Code(RunErrorCode::RunFenced),
            false,
            0,
            0
        ),
        RetryDecision::DoNotRetry {
            code: RunErrorCode::RunFenced
        }
    );
    // Idempotency outranks the budget: a non-idempotent command is refused
    // whether or not there is budget left, so the caller is told the real reason.
    assert_eq!(
        decide(
            RunStatus::Failed,
            RunFailure::Code(RunErrorCode::AdapterUnavailable),
            false,
            0,
            0
        ),
        RetryDecision::NotIdempotent {
            source: RunStatus::Failed
        }
    );
}

/// The enumeration the sweeps above depend on. DEV-03-06 shipped an eight-entry
/// hand-kept copy and silently dropped `DOMAIN_PRECONDITION_FAILED`, so an
/// exhaustive check over a stale list is worse than none: it claims coverage it
/// does not have. Pinned here so a tenth variant cannot slip through.
#[test]
fn the_code_enumeration_is_complete_and_distinct() {
    // This is a non-duplication check, not a completeness check. Completeness
    // is `every_code_the_run_machine_can_raise_is_enumerated`, which has an
    // external reference to check against.
    assert_eq!(ALL_CODES.len(), 9);
    for code in ALL_CODES {
        assert_eq!(
            ALL_CODES.iter().filter(|other| **other == code).count(),
            1,
            "{code} appears twice in RunErrorCode::ALL"
        );
        assert!(!code.as_str().is_empty(), "{code:?} has no wire name");
    }
    // The catalogue names must be unique, since the device keys on them.
    for (index, code) in ALL_CODES.iter().enumerate() {
        for other in &ALL_CODES[index + 1..] {
            assert_ne!(code.as_str(), other.as_str(), "wire name collision");
        }
    }
    // The code that a hand-kept list drops first is checked by name, because it
    // is the one §3 derives rather than reads off a row.
    assert!(
        ALL_CODES.contains(&RunErrorCode::PreconditionFailed),
        "DOMAIN_PRECONDITION_FAILED must be inside every sweep"
    );
}

/// The completeness claim that can actually be made without a derive.
///
/// `assert_eq!(ALL_CODES.len(), 9)` is a tautology — the `9` is the array's own
/// type annotation. Independent review killed a stronger version of this test:
/// adding a tenth `RunErrorCode` variant, giving it an index, and leaving `ALL`
/// at nine entries kept both this test and the compile-time duplicate check
/// green, because every completeness check available here derives the universe
/// of variants *from* `ALL`.
///
/// So completeness is checked against something outside the enum: every
/// rejection code the §3 graph can actually produce. A new code that no edge
/// raises cannot affect §11 admission, and a new code that some edge does raise
/// is now forced into `ALL` — which is the case that would have mattered.
#[test]
fn every_code_the_run_machine_can_raise_is_enumerated() {
    let mut raisable: Vec<RunErrorCode> = RUN_EDGES.iter().map(|edge| edge.rejection).collect();
    raisable.push(CREATE_QUEUED.rejection());
    // `transition` also reports this one for a command with no edge at all, and
    // §11 admission must be able to reason about it.
    raisable.push(RunErrorCode::InvalidTransition);
    raisable.sort_unstable();
    raisable.dedup();

    for code in &raisable {
        assert!(
            ALL_CODES.contains(code),
            "{code} is raised by the §3 graph but missing from RunErrorCode::ALL, \
             so the §11 sweeps above silently skip it"
        );
    }

    // And the reverse direction is what makes the list usable: nothing in `ALL`
    // may be a code §3 never raises, or the sweep would test a fiction.
    let raisable_set: HashSet<RunErrorCode> = raisable.iter().copied().collect();
    for code in ALL_CODES {
        assert!(
            raisable_set.contains(&code),
            "{code} is in RunErrorCode::ALL but the §3 graph never raises it"
        );
    }
}

/// `Display` reaches the diagnostic, so it must carry the code or the status.
#[test]
fn display_names_the_code_or_the_status() {
    assert_eq!(
        decide(
            RunStatus::Failed,
            RunFailure::Code(RunErrorCode::AdapterUnavailable),
            true,
            0,
            0
        )
        .to_string(),
        "create a new Run from terminal FAILED"
    );
    assert_eq!(
        decide(
            RunStatus::Failed,
            RunFailure::Code(RunErrorCode::RunFenced),
            true,
            0,
            0
        )
        .to_string(),
        "not auto-retryable: RUN_FENCED"
    );
    assert_eq!(
        decide(
            RunStatus::Failed,
            RunFailure::Code(RunErrorCode::AdapterUnavailable),
            false,
            0,
            0
        )
        .to_string(),
        "command is not idempotent, so FAILED is not replayed"
    );
    assert_eq!(
        decide(
            RunStatus::Queued,
            RunFailure::InternalUnexpected,
            true,
            0,
            0
        )
        .to_string(),
        "§3: 自动重试只读取终态 Run, and QUEUED is not one"
    );
    assert_eq!(
        decide(
            RunStatus::Failed,
            RunFailure::InternalUnexpected,
            true,
            0,
            0
        )
        .to_string(),
        "§11 rule 4: query the first result by idempotency key before replaying FAILED"
    );
    assert_eq!(
        decide(
            RunStatus::Failed,
            RunFailure::Code(RunErrorCode::AdapterUnavailable),
            true,
            3,
            0
        )
        .to_string(),
        "§11 retry limit spent (Count, next delay would be 1650 ms)"
    );
    assert_eq!(
        decide(
            RunStatus::Failed,
            RunFailure::Code(RunErrorCode::AdapterUnavailable),
            true,
            0,
            0
        )
        .to_string(),
        "create a new Run from terminal FAILED"
    );
}
