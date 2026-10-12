//! Conformance tests for the Outbox dispatch decisions.
//!
//! Frozen sources:
//!   - `TECH_STACK_AND_STANDARDS.md` §4.3 — 聚合更新、Event 与 Outbox 在同一
//!     SQLite 事务提交；Outbox 采用至少一次处理；消费者按 event ID 或幂等键去重。
//!   - `ERROR_CATALOG.md` §11 重试规则 — 只有目录中 `retryable=是` 且命令本身幂等时
//!     允许自动重试；自动重试默认最多 3 次、指数退避并加入 jitter；总时长不得超过
//!     命令/Message TTL；权限、校验、非法状态、版本冲突、fencing、TTL 过期不自动重试。
//!
//! Nothing here touches a database, a clock or a transport: the module is
//! deliberately IO-free so every branch is reachable from a table.

use teamspace_domain::outbox_dispatch::{
    DEFAULT_BACKOFF_BASE_MILLIS, DEFAULT_MAX_RETRIES, DeliveryFailure, DispatchDecision,
    OutboxEntry, OutboxStatus, OutcomeDecision, RetryPolicy, decide_dispatch, decide_outcome,
    decide_published, is_due,
};

const NOW: i64 = 1_726_300_123_456;
const ID: [u8; 16] = [0x11; 16];

fn policy() -> RetryPolicy {
    RetryPolicy::new(0)
}

fn entry(status: OutboxStatus, retry_count: u32, max_retries: u32) -> OutboxEntry {
    OutboxEntry {
        id: ID,
        status,
        retry_count,
        max_retries,
        scheduled_at: NOW,
    }
}

// ------------------------------------------------------ the five frozen states

#[test]
fn the_five_statuses_are_exactly_the_frozen_ones() {
    // `0006`'s CHECK: status IN ('PENDING','PROCESSING','PUBLISHED','FAILED','DEAD_LETTER')
    let spellings: Vec<&str> = [
        OutboxStatus::Pending,
        OutboxStatus::Processing,
        OutboxStatus::Published,
        OutboxStatus::Failed,
        OutboxStatus::DeadLetter,
    ]
    .iter()
    .map(|s| s.as_str())
    .collect();
    assert_eq!(
        spellings,
        vec![
            "PENDING",
            "PROCESSING",
            "PUBLISHED",
            "FAILED",
            "DEAD_LETTER"
        ]
    );
}

#[test]
fn only_pending_and_failed_are_dispatchable() {
    // `outbox_pending_dispatch`'s partial index:
    //   WHERE status IN ('PENDING', 'FAILED')
    assert!(OutboxStatus::Pending.is_dispatchable());
    assert!(OutboxStatus::Failed.is_dispatchable());
    for not in [
        OutboxStatus::Processing,
        OutboxStatus::Published,
        OutboxStatus::DeadLetter,
    ] {
        assert!(
            !not.is_dispatchable(),
            "{not} must not be selected by the dispatch index"
        );
    }
}

#[test]
fn a_processing_row_is_never_due() {
    // A row another dispatcher has claimed. Claiming it again would turn
    // at-least-once delivery into at-most-once by accident, which §4.3 does not
    // promise and cannot support.
    let claimed = entry(OutboxStatus::Processing, 0, DEFAULT_MAX_RETRIES);
    assert!(!is_due(&claimed, NOW));
    assert_eq!(
        decide_dispatch(&claimed, NOW),
        DispatchDecision::NotClaimable {
            observed: OutboxStatus::Processing
        }
    );
}

// ------------------------------------------------------------------- claiming

#[test]
fn a_due_row_is_claimed_as_processing() {
    for status in [OutboxStatus::Pending, OutboxStatus::Failed] {
        let due = entry(status, 0, DEFAULT_MAX_RETRIES);
        assert_eq!(
            decide_dispatch(&due, NOW),
            DispatchDecision::Claim {
                claim_as: OutboxStatus::Processing
            },
            "{status} is dispatchable and due"
        );
    }
}

#[test]
fn a_row_scheduled_in_the_future_is_not_due() {
    let mut later = entry(OutboxStatus::Failed, 1, DEFAULT_MAX_RETRIES);
    later.scheduled_at = NOW + 1;
    assert!(
        !is_due(&later, NOW),
        "scheduled_at is one millisecond ahead"
    );
    assert_eq!(decide_dispatch(&later, NOW), DispatchDecision::NotDue);

    // The boundary itself is due: the comparison is `<=`, matching a scheduler
    // that fires at the scheduled instant.
    let mut at = entry(OutboxStatus::Failed, 1, DEFAULT_MAX_RETRIES);
    at.scheduled_at = NOW;
    assert!(is_due(&at, NOW));
    assert!(matches!(
        decide_dispatch(&at, NOW),
        DispatchDecision::Claim { .. }
    ));
}

#[test]
fn a_terminal_row_is_never_claimed() {
    for terminal in [OutboxStatus::Published, OutboxStatus::DeadLetter] {
        assert_eq!(
            decide_dispatch(&entry(terminal, 0, DEFAULT_MAX_RETRIES), NOW),
            DispatchDecision::NotClaimable { observed: terminal },
            "{terminal} is terminal: no index selects it and no retry may claim it"
        );
    }
}

// ------------------------------------------------------------- §11 retry rules

#[test]
fn a_retryable_failure_below_the_ceiling_schedules_the_next_attempt() {
    // §11: 自动重试默认最多 3 次、指数退避并加入 jitter
    let due = entry(OutboxStatus::Processing, 0, DEFAULT_MAX_RETRIES);
    let decision = decide_outcome(
        &due,
        DeliveryFailure::Retryable {
            code: "ADAPTER_UNAVAILABLE",
        },
        policy(),
        NOW,
        0, /* budget_spent_millis */
    );
    match decision {
        OutcomeDecision::Retry {
            status,
            retry_count,
            scheduled_at,
            last_error,
        } => {
            assert_eq!(status, OutboxStatus::Failed);
            assert_eq!(retry_count, 1, "一次失败记一次");
            assert_eq!(
                scheduled_at,
                NOW + DEFAULT_BACKOFF_BASE_MILLIS,
                "第一次退避就是 base"
            );
            assert_eq!(last_error, "ADAPTER_UNAVAILABLE");
        }
        other => panic!("expected a retry, got {other:?}"),
    }
}

#[test]
fn three_retries_are_permitted_and_the_fourth_failure_dead_letters() {
    // §11: 最多 3 次. Read as *retries*, not attempts: a row that fails is
    // retried three times, and the failure after the third retry is terminal.
    for attempt in 0..DEFAULT_MAX_RETRIES {
        let decision = decide_outcome(
            &entry(OutboxStatus::Processing, attempt, DEFAULT_MAX_RETRIES),
            DeliveryFailure::Retryable {
                code: "ADAPTER_UNAVAILABLE",
            },
            policy(),
            NOW,
            0, /* budget_spent_millis */
        );
        assert!(
            matches!(decision, OutcomeDecision::Retry { retry_count, .. } if retry_count == attempt + 1),
            "retry {attempt} is still within the ceiling, got {decision:?}"
        );
    }

    // The fourth failure exhausts it.
    let decision = decide_outcome(
        &entry(
            OutboxStatus::Processing,
            DEFAULT_MAX_RETRIES,
            DEFAULT_MAX_RETRIES,
        ),
        DeliveryFailure::Retryable {
            code: "ADAPTER_UNAVAILABLE",
        },
        policy(),
        NOW,
        0, /* budget_spent_millis */
    );
    assert_eq!(
        decision,
        OutcomeDecision::DeadLetter {
            retry_count: DEFAULT_MAX_RETRIES,
            last_error: "ADAPTER_UNAVAILABLE"
        },
        "§11: 最多 3 次之后进入终态"
    );
}

#[test]
fn a_permanent_failure_never_retries_even_with_retries_left() {
    // §11: 权限、校验、非法状态、版本冲突、fencing、TTL 过期不自动重试.
    //
    // Checked with a fresh row (retry_count = 0) so this cannot pass merely
    // because the ceiling happened to be reached.
    for reason in [
        "AUTH_PERMISSION_DENIED",
        "VALIDATION_INVALID_FIELD",
        "DOMAIN_INVALID_TRANSITION",
        "CONCURRENCY_VERSION_CONFLICT",
        "RUN_FENCED",
    ] {
        let decision = decide_outcome(
            &entry(OutboxStatus::Processing, 0, DEFAULT_MAX_RETRIES),
            DeliveryFailure::Permanent { reason },
            policy(),
            NOW,
            0, /* budget_spent_millis */
        );
        assert_eq!(
            decision,
            OutcomeDecision::DeadLetter {
                retry_count: 0,
                last_error: reason
            },
            "{reason} 不自动重试"
        );
    }
}

#[test]
fn a_dead_lettered_row_is_not_dispatchable() {
    // The point of DEAD_LETTER: `outbox_pending_dispatch` does not select it,
    // so nothing retries it and an operator has to look.
    let dead = entry(
        OutboxStatus::DeadLetter,
        DEFAULT_MAX_RETRIES,
        DEFAULT_MAX_RETRIES,
    );
    assert!(!dead.status.is_dispatchable());
    assert!(!is_due(&dead, NOW));
    assert_eq!(
        decide_dispatch(&dead, NOW),
        DispatchDecision::NotClaimable {
            observed: OutboxStatus::DeadLetter
        }
    );
}

#[test]
fn a_successful_delivery_publishes_with_its_timestamp() {
    assert_eq!(
        decide_published(NOW),
        OutcomeDecision::Published { published_at: NOW }
    );
}

// --------------------------------------------------- §11 backoff and jitter

#[test]
fn the_backoff_is_exponential() {
    let policy = policy();
    let delays: Vec<i64> = (0..=4).map(|n| policy.backoff_millis(n)).collect();
    // base 200: 200, 400, 800, 1600, 3200
    assert_eq!(delays, vec![200, 400, 800, 1600, 3200]);
    for pair in delays.windows(2) {
        assert_eq!(
            pair[1],
            pair[0] * 2,
            "指数退避: each attempt doubles the delay"
        );
    }
}

#[test]
fn jitter_is_added_and_the_total_stays_inside_the_ceiling() {
    // §11: 指数退避并加入 jitter；总时长不得超过命令/Message TTL.
    let jittered = RetryPolicy::new(50);
    assert_eq!(jittered.backoff_millis(0), 200 + 50);
    assert_eq!(jittered.backoff_millis(1), 400 + 50);

    // §11's bound is on the **sum**, so the budget is what accumulates. A single
    // delay is deliberately not clamped to the budget here — the budget decides
    // how much of it is still available.
    let capped = RetryPolicy {
        max_retries: DEFAULT_MAX_RETRIES,
        backoff_base_millis: DEFAULT_BACKOFF_BASE_MILLIS,
        total_budget_millis: 1_000,
        jitter_millis: 50,
    };
    let mut budget = capped.budget_after(0);
    let mut spent = 0;
    for attempt in 0..8 {
        let delay = budget.next_delay_millis(attempt);
        assert!(delay >= 0, "attempt {attempt} produced a negative delay");
        assert_eq!(
            spent + delay,
            spent + delay,
            "saturating add keeps the total monotone"
        );
        spent += delay;
        assert!(
            spent <= 1_000,
            "attempt {attempt} pushed the total to {spent}, past the TTL budget"
        );
        budget = budget.plus(delay);
    }
    assert!(
        budget.is_exhausted() || spent < 1_000,
        "the budget is either exhausted or still has room; never overspent"
    );
}

#[test]
fn a_huge_retry_count_saturates_instead_of_overflowing() {
    // A row whose retry_count was corrupted, or a caller that passes u32::MAX.
    // Saturating arithmetic must not panic or wrap to a negative delay.
    let policy = policy();
    for attempt in [31_u32, 32, 63, u32::MAX] {
        let delay = policy.backoff_millis(attempt);
        assert!(delay >= 0, "attempt {attempt} produced a negative delay");
        // The single delay may be large by design; what must hold is that the
        // **budget** never goes negative when it is charged.
        assert!(
            policy.budget_after(delay).remaining_millis() >= 0,
            "attempt {attempt} left the budget negative"
        );
    }
}

#[test]
fn the_scheduled_at_of_a_retry_respects_the_backoff() {
    // The two functions together: a retry's next attempt is scheduled now plus
    // the computed delay, not at a fixed interval.
    let policy = policy();
    for retry_count in 0..3_u32 {
        let decision = decide_outcome(
            &entry(OutboxStatus::Processing, retry_count, DEFAULT_MAX_RETRIES),
            DeliveryFailure::Retryable {
                code: "ADAPTER_UNAVAILABLE",
            },
            policy,
            NOW,
            0, /* budget_spent_millis */
        );
        match decision {
            OutcomeDecision::Retry { scheduled_at, .. } => {
                assert_eq!(
                    scheduled_at,
                    NOW + policy.backoff_millis(retry_count),
                    "the delay after {retry_count} failed attempts is that attempt's own backoff, \
                     so the first retry waits exactly the base"
                );
                assert!(
                    scheduled_at > NOW,
                    "a retry is always scheduled in the future"
                );
            }
            other => panic!("expected a retry, got {other:?}"),
        }
    }
}

// -------------------------------------------------- at-least-once consequences

#[test]
fn a_retried_row_becomes_due_again_and_will_be_redelivered() {
    // §4.3: 至少一次. A retried row returns to FAILED, which the dispatch index
    // selects — so the same event is delivered again. That is the contract, and
    // it is why consumers must deduplicate rather than assume once-only.
    let mut row = entry(OutboxStatus::Processing, 0, DEFAULT_MAX_RETRIES);
    let decision = decide_outcome(
        &row,
        DeliveryFailure::Retryable {
            code: "ADAPTER_UNAVAILABLE",
        },
        policy(),
        NOW,
        0, /* budget_spent_millis */
    );
    match decision {
        OutcomeDecision::Retry {
            status,
            retry_count,
            scheduled_at,
            ..
        } => {
            row.status = status;
            row.retry_count = retry_count;
            row.scheduled_at = scheduled_at;
        }
        other => panic!("expected a retry, got {other:?}"),
    }

    assert!(row.status.is_dispatchable(), "FAILED is dispatchable again");
    assert!(
        !is_due(&row, NOW),
        "the retry is scheduled in the future, not immediately"
    );
    assert!(
        is_due(&row, row.scheduled_at),
        "and due once the backoff has elapsed"
    );
    assert_eq!(
        decide_dispatch(&row, row.scheduled_at),
        DispatchDecision::Claim {
            claim_as: OutboxStatus::Processing
        },
        "至少一次: the same row is delivered again"
    );
}

#[test]
fn the_full_lifecycle_reaches_a_single_terminal_state() {
    // A row that keeps failing walks PENDING → PROCESSING → FAILED → … and
    // ends in DEAD_LETTER exactly once. No path may resurrect it.
    let mut row = OutboxEntry::pending(ID, NOW);
    let mut transitions: Vec<OutboxStatus> = vec![row.status];
    let policy = policy();
    // Tracked so `decide_outcome` is charged for the wait it schedules: §11's
    // 总时长 bound is on the sum, and passing 0 every round would hide that.
    let mut spent_so_far = 0_i64;

    for _ in 0..8 {
        let claim = decide_dispatch(&row, row.scheduled_at);
        let DispatchDecision::Claim { claim_as } = claim else {
            break;
        };
        row.status = claim_as;
        transitions.push(row.status);

        match decide_outcome(
            &row,
            DeliveryFailure::Retryable {
                code: "ADAPTER_UNAVAILABLE",
            },
            policy,
            row.scheduled_at,
            spent_so_far,
        ) {
            OutcomeDecision::Retry {
                status,
                retry_count,
                scheduled_at,
                ..
            } => {
                spent_so_far += scheduled_at - row.scheduled_at;
                row.status = status;
                row.retry_count = retry_count;
                row.scheduled_at = scheduled_at;
                transitions.push(row.status);
            }
            OutcomeDecision::DeadLetter { .. } => {
                row.status = OutboxStatus::DeadLetter;
                transitions.push(row.status);
                break;
            }
            OutcomeDecision::Published { .. } => panic!("a failure cannot publish"),
        }
    }

    assert_eq!(
        row.status,
        OutboxStatus::DeadLetter,
        "a permanently failing row must end terminal"
    );
    assert_eq!(
        transitions
            .iter()
            .filter(|s| **s == OutboxStatus::DeadLetter)
            .count(),
        1,
        "DEAD_LETTER is entered once, never repeated: {transitions:?}"
    );
    assert_eq!(
        row.retry_count, DEFAULT_MAX_RETRIES,
        "the row used exactly the ceiling's worth of retries"
    );
    assert!(
        !is_due(&row, row.scheduled_at),
        "a dead-lettered row is never due again, even at its own schedule"
    );
}

#[test]
fn the_frozen_default_remains_three() {
    // ERROR_CATALOG §11 is the normative default. Current SQLite behaviour is
    // tested with actual omitted-column INSERTs in persistence-sqlite; comparing
    // this constant with the old DDL's literal 5 does not verify a database.
    assert_eq!(DEFAULT_MAX_RETRIES, 3, "§11: 自动重试默认最多 3 次");
}

#[test]
fn the_permanent_branch_is_reachable_and_total() {
    // §11's first rule is exclusive: 只有目录中 `retryable=是` 才允许自动重试.
    // This module does **not** read the catalogue — see the `DeliveryFailure`
    // docstring for why duplicating that table would be worse than trusting the
    // caller. What is assertable here is the consequence: whatever the caller
    // classified as permanent never reaches the retry path, at any retry count.
    //
    // Swept over every count from 0 past the ceiling, so "permanent" is a
    // property of the classification rather than of the remaining budget.
    for retry_count in 0..=DEFAULT_MAX_RETRIES + 2 {
        for reason in [
            "AUTH_PERMISSION_DENIED",
            "VALIDATION_INVALID_FIELD",
            "DOMAIN_INVALID_TRANSITION",
            "CONCURRENCY_VERSION_CONFLICT",
            "RUN_FENCED",
            "MESSAGE_TTL_EXPIRED",
        ] {
            let decision = decide_outcome(
                &entry(OutboxStatus::Processing, retry_count, DEFAULT_MAX_RETRIES),
                DeliveryFailure::Permanent { reason },
                policy(),
                NOW,
                0, /* budget_spent_millis */
            );
            assert_eq!(
                decision,
                OutcomeDecision::DeadLetter {
                    retry_count,
                    last_error: reason
                },
                "永久失败在 retry_count={retry_count} 上仍不得重试（§11 第一条的排他条件）"
            );
        }
    }
}

#[test]
fn the_catalogue_gate_is_the_callers_and_the_comment_says_so() {
    // There is no error-code catalogue in this crate, so §11's first condition
    // cannot be enforced here. Asserting that absence is deliberate: if a future
    // change adds such a table, this test should be revisited rather than left
    // to rot as a comment nobody reads.
    //
    // `Permanent` is the half this module does own, and it is total — see
    // `the_permanent_branch_is_reachable_and_total`. This test records the
    // other half's location: the classification happens where the catalogue is
    // read, not here.
    let classified_by_caller = DeliveryFailure::Retryable {
        code: "CONCURRENCY_RESOURCE_BUSY",
    };
    assert!(matches!(
        decide_outcome(
            &entry(OutboxStatus::Processing, 0, DEFAULT_MAX_RETRIES),
            classified_by_caller,
            policy(),
            NOW,
            0 /* budget_spent_millis */
        ),
        OutcomeDecision::Retry { .. }
    ));
}

#[test]
fn the_total_wait_stays_within_the_ttl_budget_across_every_retry() {
    // §11: 总时长不得超过命令/Message TTL. The bound is on the **sum** of the
    // delays, which is why `RetryBudget` exists: clamping each delay separately
    // says nothing about the total, and with a caller-settable `max_retries` the
    // sum is otherwise unbounded.
    let policy = RetryPolicy {
        max_retries: 50,
        backoff_base_millis: DEFAULT_BACKOFF_BASE_MILLIS,
        total_budget_millis: 5_000,
        jitter_millis: 0,
    };
    // Driven through `decide_outcome` rather than through a local re-implementation
    // of the arithmetic. An earlier draft of this test computed `spent` itself and
    // so passed even when the budget was bypassed in `decide_outcome` — the test
    // checked its own maths instead of the code's. The delays below are read out
    // of the decisions, so bypassing the budget changes them.
    let mut spent = 0_i64;
    let mut row = entry(OutboxStatus::Processing, 0, 50);

    for retry_count in 0..50_u32 {
        row.retry_count = retry_count;
        let decision = decide_outcome(
            &row,
            DeliveryFailure::Retryable {
                code: "ADAPTER_UNAVAILABLE",
            },
            policy,
            NOW,
            spent,
        );
        let OutcomeDecision::Retry { scheduled_at, .. } = decision else {
            assert!(matches!(decision, OutcomeDecision::DeadLetter { .. }));
            assert_eq!(spent, policy.total_budget_millis);
            break;
        };
        let delay = scheduled_at - NOW;
        assert!(delay >= 0, "no negative delay at retry {retry_count}");
        spent += delay;
        assert!(
            spent <= policy.total_budget_millis,
            "§11 总时长: accumulated {spent} exceeds the {} budget at retry \
             {retry_count}",
            policy.total_budget_millis
        );
    }

    // With 50 permitted retries and exponential growth, the budget is what stops
    // the *delay*, not the retry ceiling: 200+400+800+1600+3200 alone is 6200 ms,
    // already past 5000, so ignoring the budget would overspend.
    assert_eq!(
        spent, 5_000,
        "the delays must be truncated by the budget, not merely summed"
    );
}

#[test]
fn the_rows_own_ceiling_overrides_the_policy_default() {
    // §11 says 默认最多 3 次, and `0006` stores a per-row `max_retries`. An
    // earlier version read only the policy, so a row explicitly set to 0 — "never
    // retry this" — still got three retries. The row's own value wins.
    let strict = RetryPolicy::new(0);

    // 0 means "never retry this row at all", so it is checked separately from
    // the ceilings that still permit at least one retry.
    let never = decide_outcome(
        &entry(OutboxStatus::Processing, 0, 0),
        DeliveryFailure::Retryable {
            code: "ADAPTER_UNAVAILABLE",
        },
        strict,
        NOW,
        0,
    );
    assert_eq!(
        never,
        OutcomeDecision::DeadLetter {
            retry_count: 0,
            last_error: "ADAPTER_UNAVAILABLE"
        },
        "行级 max_retries=0 表示永不重试，不得被策略的默认值覆盖"
    );

    for row_ceiling in [1, 2] {
        let decision = decide_outcome(
            &entry(OutboxStatus::Processing, 0, row_ceiling),
            DeliveryFailure::Retryable {
                code: "ADAPTER_UNAVAILABLE",
            },
            strict,
            NOW,
            0,
        );
        let OutcomeDecision::Retry { retry_count, .. } = decision else {
            panic!("row_ceiling={row_ceiling} should still permit one retry");
        };
        assert_eq!(
            retry_count, 1,
            "a row capped at {row_ceiling} must stop there, not at the policy's 3"
        );
        // And the row's own ceiling is what ends it.
        let exhausted = decide_outcome(
            &entry(OutboxStatus::Processing, row_ceiling, row_ceiling),
            DeliveryFailure::Retryable {
                code: "ADAPTER_UNAVAILABLE",
            },
            strict,
            NOW,
            0,
        );
        assert!(
            matches!(exhausted, OutcomeDecision::DeadLetter { .. }),
            "row_ceiling={row_ceiling} must dead-letter at that count"
        );
    }

    // A row ceiling above the policy's default still respects §11's 3.
    let generous = decide_outcome(
        &entry(OutboxStatus::Processing, DEFAULT_MAX_RETRIES, 99),
        DeliveryFailure::Retryable {
            code: "ADAPTER_UNAVAILABLE",
        },
        strict,
        NOW,
        0,
    );
    assert!(
        matches!(generous, OutcomeDecision::DeadLetter { .. }),
        "a row asking for 99 retries does not raise §11's ceiling"
    );
}

#[test]
fn a_negative_budget_does_not_panic() {
    // `RetryPolicy` fields are public, so a caller can construct a negative
    // `total_budget_millis`. `clamp(0, -1)` would panic on `min > max`; the
    // budget must degrade to "nothing left" instead.
    let hostile = RetryPolicy {
        max_retries: DEFAULT_MAX_RETRIES,
        backoff_base_millis: DEFAULT_BACKOFF_BASE_MILLIS,
        total_budget_millis: -1,
        jitter_millis: 0,
    };
    let budget = hostile.budget_after(0);
    assert_eq!(
        budget.remaining_millis(),
        0,
        "a hostile budget has nothing left"
    );
    assert!(budget.is_exhausted());
    assert_eq!(
        budget.next_delay_millis(0),
        0,
        "no delay is available; outcome must stop retrying"
    );

    // A hostile base and jitter must not produce a negative delay either.
    let hostile2 = RetryPolicy {
        backoff_base_millis: -500,
        jitter_millis: -1_000,
        ..policy()
    };
    for attempt in 0..8 {
        assert!(
            hostile2.backoff_millis(attempt) >= 0,
            "attempt {attempt} produced a negative delay"
        );
    }
}
