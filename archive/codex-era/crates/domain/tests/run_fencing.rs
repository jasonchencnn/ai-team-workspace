//! `TECH_STACK_AND_STANDARDS.md` §5: fencing generation, lease renewal, write
//! admission. Node standard: 旧 token 的心跳、输出和完成都被拒；并发与恢复
//! 测试无双主 Run.
//!
//! The sweep that carries the node standard is
//! `every_stale_write_is_refused` — it walks every `RunWrite` against every
//! non-current generation, so a variant added without a rule fails loudly.

use teamspace_domain::{
    Generation, HeartbeatDecision, LEASE_BEARING_STATUSES, LeaseRefusal, LeaseSnapshot,
    RenewalDecision, RunErrorCode, RunStatus, RunWrite, RunWriteRefusal, WriteDecision,
    decide_heartbeat, decide_renewal, decide_write,
};

/// A lease that is current and unexpired at `NOW`.
const NOW: i64 = 1_000_000;
const TTL: i64 = 30_000;

fn live_lease() -> LeaseSnapshot {
    LeaseSnapshot {
        fencing_token: 7,
        acquired_at_ms: NOW - 5_000,
        expires_at_ms: NOW + TTL,
    }
}

/// The node standard's first clause: 旧 token 的心跳、输出和完成都被拒.
#[test]
fn every_stale_write_is_refused() {
    for write in RunWrite::ALL {
        for current_token in 1..=3_u64 {
            for presented in 0..current_token {
                assert_eq!(
                    decide_write(current_token, presented, write, RunStatus::Running),
                    WriteDecision::Refuse {
                        error_code: RunWriteRefusal::Fenced
                    },
                    "current={current_token} presented={presented} write={write:?}"
                );
            }
        }
    }
}

/// The same sweep with the roles of the executor and the platform swapped: a
/// token *ahead* of the Task's current one is impossible under `DATA_CONVENTIONS`
/// §2, so it is a data fault, and it must not be admitted as if it were current.
#[test]
fn a_token_ahead_of_the_current_generation_is_not_current() {
    for write in RunWrite::ALL {
        assert_eq!(
            decide_write(7, 9, write, RunStatus::Running),
            WriteDecision::Refuse {
                error_code: RunWriteRefusal::Fenced
            },
            "{write:?}"
        );
        assert_eq!(Generation::classify(9, 7), Generation::Ahead);
    }
}

/// Only the current generation is admitted. One row, because the node standard
/// puts the burden on the refusals above.
#[test]
fn the_current_generation_is_admitted() {
    for write in RunWrite::ALL {
        assert_eq!(
            decide_write(7, 7, write, RunStatus::Running),
            WriteDecision::Accept { write },
            "{write:?}"
        );
    }
    assert!(Generation::classify(7, 7).is_current());
    assert!(!Generation::classify(6, 7).is_current());
    // The `Ahead` arm was the one `is_current` never pinned: without this the
    // method could widen to `Current | Ahead` and all 16 tests still pass.
    // §2 makes "ahead" impossible for a real Task, so the only thing left to
    // catch the widening is a test that states the case.
    assert!(!Generation::classify(8, 7).is_current());
}

/// The three cases stay distinct. `!=` would collapse the first and third into
/// one boolean, and the two mean different things.
#[test]
fn generation_keeps_three_cases_apart() {
    assert_eq!(Generation::classify(7, 7), Generation::Current);
    assert_eq!(Generation::classify(6, 7), Generation::Stale);
    assert_eq!(Generation::classify(8, 7), Generation::Ahead);
    // Generations start at 1, so 0 is "nothing was ever minted" and is Stale
    // like any other past value — never Current. The floor is `0004`'s
    // `fencing_token >= 1` CHECK, not `DATA_CONVENTIONS.md` §2: §2 freezes the
    // counter as monotonic and forbids substituting a random Session ID, and
    // says nothing about where it starts.
    assert_eq!(Generation::classify(0, 1), Generation::Stale);
    assert_eq!(Generation::ALL.len(), 3);
}

/// §5: 心跳只能延长匹配当前 token 的租约.
#[test]
fn only_the_current_token_renews_the_lease() {
    for presented in [0, 6, 9] {
        assert_eq!(
            decide_renewal(live_lease(), 7, presented, NOW, TTL),
            RenewalDecision::Refused {
                error_code: LeaseRefusal::Fenced
            },
            "presented={presented}"
        );
    }
    assert_eq!(
        decide_renewal(live_lease(), 7, 7, NOW, TTL),
        RenewalDecision::Renewed {
            expires_at_ms: NOW + TTL
        }
    );
}

/// The current executor whose window has closed is **not** fenced. Calling it
/// `RUN_FENCED` would be exactly the false diagnosis the two codes were split
/// to avoid, and it would point the caller at a takeover that never happened.
#[test]
fn an_expired_lease_is_not_fenced() {
    let expired = LeaseSnapshot {
        fencing_token: 7,
        acquired_at_ms: NOW - 60_000,
        expires_at_ms: NOW - 1_000,
    };
    assert!(expired.is_expired_at(NOW));
    let decision = decide_renewal(expired, 7, 7, NOW, TTL);
    assert_eq!(
        decision,
        RenewalDecision::Refused {
            error_code: LeaseRefusal::Expired
        }
    );
    assert_ne!(
        decision.refusal(),
        Some(LeaseRefusal::Fenced),
        "an expired lease is a time-window problem, not an identity problem"
    );
}

/// If a heartbeat could revive an expired lease, leases would never expire and
/// §3's `RUNNING → TIMED_OUT` would be unreachable for a live process. This is
/// the whole reason the two codes are separate.
#[test]
fn an_expired_lease_is_never_revived() {
    let expired = LeaseSnapshot {
        fencing_token: 7,
        acquired_at_ms: NOW - 60_000,
        expires_at_ms: NOW - 1_000,
    };
    for presented in [6, 7, 9] {
        let decision = decide_renewal(expired, 7, presented, NOW, TTL);
        assert!(
            !decision.renewed(),
            "presented={presented} revived an expired lease: {decision}"
        );
    }
}

/// Renewal moves the window forward from *now*, not from the old expiry, so a
/// slow heartbeat cannot shorten a lease.
#[test]
fn renewal_advances_from_now_not_from_the_old_expiry() {
    let nearly_done = LeaseSnapshot {
        fencing_token: 7,
        acquired_at_ms: NOW - 29_000,
        expires_at_ms: NOW + 1_000,
    };
    assert_eq!(
        decide_renewal(nearly_done, 7, 7, NOW, TTL),
        RenewalDecision::Renewed {
            expires_at_ms: NOW + TTL
        }
    );
}

/// A non-positive TTL would let a heartbeat pull the expiry back before the
/// heartbeat that produced it. `0004` does not rule this out; the domain does.
#[test]
fn a_non_positive_ttl_cannot_pull_the_expiry_backwards() {
    for ttl in [0, -1, i64::MIN] {
        let decision = decide_renewal(live_lease(), 7, 7, NOW, ttl);
        let RenewalDecision::Renewed { expires_at_ms } = decision else {
            panic!("ttl={ttl} was not renewed: {decision}");
        };
        assert!(
            expires_at_ms >= NOW,
            "ttl={ttl} produced {expires_at_ms}, before the heartbeat at {NOW}"
        );
    }
    // And a TTL that would overflow saturates instead of wrapping.
    let decision = decide_renewal(live_lease(), 7, 7, NOW, i64::MAX);
    let RenewalDecision::Renewed { expires_at_ms } = decision else {
        panic!("{decision}");
    };
    assert_eq!(expires_at_ms, i64::MAX);
}

/// The composed entry point, which is the order §5's sentence reads in.
#[test]
fn the_heartbeat_composes_identity_before_the_window() {
    // Current and live: renewed.
    assert_eq!(
        decide_heartbeat(live_lease(), 7, 7, NOW, TTL, RunStatus::Running),
        HeartbeatDecision::Renewed {
            expires_at_ms: NOW + TTL
        }
    );
    // Current but expired: admitted as the current generation, window closed.
    let expired = LeaseSnapshot {
        fencing_token: 7,
        acquired_at_ms: NOW - 60_000,
        expires_at_ms: NOW - 1_000,
    };
    assert_eq!(
        decide_heartbeat(expired, 7, 7, NOW, TTL, RunStatus::Running),
        HeartbeatDecision::LeaseExpired
    );
    // Stale but unexpired: refused. Identity is checked first, so a live lease
    // does not rescue a dead executor.
    assert_eq!(
        decide_heartbeat(live_lease(), 7, 6, NOW, TTL, RunStatus::Running),
        HeartbeatDecision::Refused {
            error_code: RunWriteRefusal::Fenced
        }
    );
    // Stale *and* expired: still fenced, not `Expired` — the identity answer is
    // the definitive one and `Expired` would misdescribe it.
    assert_eq!(
        decide_heartbeat(expired, 7, 6, NOW, TTL, RunStatus::Running),
        HeartbeatDecision::Refused {
            error_code: RunWriteRefusal::Fenced
        }
    );
    assert_eq!(
        decide_heartbeat(expired, 7, 6, NOW, TTL, RunStatus::Running).refusal(),
        Some(RunWriteRefusal::Fenced)
    );
    assert_eq!(HeartbeatDecision::LeaseExpired.refusal(), None);
}

/// `decide_heartbeat` must agree with the two functions it composes, on every
/// combination, rather than drifting into a third answer.
#[test]
fn the_composed_heartbeat_agrees_with_its_parts() {
    for current_token in [1_u64, 7, 9] {
        for presented in [0_u64, 1, 6, 7, 8, 9] {
            for expires_at_ms in [NOW - 1, NOW, NOW + 1, NOW + TTL] {
                for ttl in [0, TTL] {
                    let lease = LeaseSnapshot {
                        fencing_token: current_token,
                        acquired_at_ms: 0,
                        expires_at_ms,
                    };
                    let write = decide_write(
                        current_token,
                        presented,
                        RunWrite::Heartbeat,
                        RunStatus::Running,
                    );
                    let renewal = decide_renewal(lease, current_token, presented, NOW, ttl);
                    let composed = decide_heartbeat(
                        lease,
                        current_token,
                        presented,
                        NOW,
                        ttl,
                        RunStatus::Running,
                    );
                    let expected = match (write, renewal) {
                        (WriteDecision::Refuse { error_code }, _) => {
                            HeartbeatDecision::Refused { error_code }
                        }
                        (_, RenewalDecision::Refused { error_code }) => match error_code {
                            LeaseRefusal::Expired => HeartbeatDecision::LeaseExpired,
                            LeaseRefusal::Fenced => HeartbeatDecision::Refused {
                                error_code: RunWriteRefusal::Fenced,
                            },
                        },
                        (_, RenewalDecision::Renewed { expires_at_ms }) => {
                            HeartbeatDecision::Renewed { expires_at_ms }
                        }
                    };
                    assert_eq!(
                        composed, expected,
                        "current={current_token} presented={presented} \
                         expires={expires_at_ms} ttl={ttl}"
                    );
                }
            }
        }
    }
}

/// The lease window is valid *through* `expires_at`, so a zero-length window is
/// not simultaneously valid and expired.
#[test]
fn a_lease_is_valid_through_its_expiry_instant() {
    let lease = LeaseSnapshot {
        fencing_token: 1,
        acquired_at_ms: NOW,
        expires_at_ms: NOW,
    };
    assert!(!lease.is_expired_at(NOW), "valid through expires_at");
    assert!(lease.is_expired_at(NOW + 1));
    assert!(decide_renewal(lease, 1, 1, NOW, TTL).renewed());
}

/// `RUN_FENCED` is a §3 rejection code; `RUN_LEASE_EXPIRED` is not, and saying so
/// honestly matters more than forcing a mapping.
#[test]
fn the_two_refusals_map_to_the_catalogue_as_written() {
    assert_eq!(LeaseRefusal::Fenced.as_str(), "RUN_FENCED");
    assert_eq!(LeaseRefusal::Expired.as_str(), "RUN_LEASE_EXPIRED");
    assert_eq!(
        LeaseRefusal::Fenced.as_run_error_code(),
        Some(RunErrorCode::RunFenced)
    );
    assert_eq!(
        LeaseRefusal::Expired.as_run_error_code(),
        None,
        "§3 has no row that raises RUN_LEASE_EXPIRED, so there is nothing to map it to"
    );
    assert_eq!(LeaseRefusal::Fenced.to_string(), "RUN_FENCED");
    assert_eq!(LeaseRefusal::Expired.to_string(), "RUN_LEASE_EXPIRED");
}

/// The statuses a lease may exist for: every non-terminal Run status, and
/// nothing else. That much is a domain fact and this test can prove it.
///
/// **It deliberately does not read `0012`.** Two earlier versions did, parsing
/// the trigger text out of the migration to check the constant against it. That
/// was the wrong shape of check, and independent review walked it down six
/// one-line bypasses in two rounds — `/* … */` and trailing `--` decoys, a
/// `DROP TRIGGER` appended at the end of the file, a `CASE` decoy reading a
/// different table, `AND 0 AND status IN (…)`, literal case changes. Each of
/// them made the domain suite report 16/16 green. Reading a file is not the same
/// question as running the database, and a text check can always be satisfied by
/// text that enforces nothing.
///
/// The database half of the claim is verified where a database exists:
/// `persistence-sqlite/tests/run_lease_consistency_forward_migration.rs`
/// builds the real schema and, against live SQLite, asserts that all eleven
/// `0012` guards are present in `sqlite_master` by name, that the insert guard
/// admits exactly these four statuses and refuses the other four, and that the
/// update guard refuses to re-point a live lease onto a terminal Run while
/// still permitting renewal while the Run is active. Four separate claims,
/// four separate places — rather than one test stretching a domain fact over a
/// storage fact.
#[test]
fn lease_bearing_statuses_are_exactly_the_non_terminal_statuses() {
    assert_eq!(LEASE_BEARING_STATUSES.len(), 4);
    for status in LEASE_BEARING_STATUSES {
        assert!(
            !status.is_terminal(),
            "{status} is a terminal state and cannot hold a lease"
        );
    }
    let listed: i32 = LEASE_BEARING_STATUSES.len() as i32;
    let non_terminal = RunStatus::ALL
        .iter()
        .filter(|status| !status.is_terminal())
        .count() as i32;
    assert_eq!(
        listed, non_terminal,
        "the list is every non-terminal status"
    );
}

/// The enumeration constants must have no duplicates. Not a completeness claim:
/// nothing without a derive can compare an enum's variant count against a
/// hand-written list, and claiming otherwise is what independent review caught
/// in DEV-03-06.
#[test]
fn the_enumerations_are_distinct() {
    assert_eq!(RunWrite::ALL.len(), 3);
    assert_eq!(Generation::ALL.len(), 3);
    for write in RunWrite::ALL {
        assert_eq!(
            RunWrite::ALL
                .iter()
                .filter(|other| **other == write)
                .count(),
            1,
            "{write:?} appears twice in RunWrite::ALL"
        );
    }
    for generation in Generation::ALL {
        assert_eq!(
            Generation::ALL
                .iter()
                .filter(|other| **other == generation)
                .count(),
            1,
            "{generation:?} appears twice in Generation::ALL"
        );
    }
}

/// `Display` reaches the diagnostic, so it must name the case and the code.
#[test]
fn display_names_the_case_and_the_code() {
    assert_eq!(
        decide_write(7, 6, RunWrite::Output, RunStatus::Running).to_string(),
        "write refused: RUN_FENCED"
    );
    assert_eq!(
        decide_write(7, 7, RunWrite::Completion, RunStatus::Running).to_string(),
        "completion accepted for the current generation"
    );
    assert_eq!(
        decide_renewal(live_lease(), 7, 7, NOW, TTL).to_string(),
        format!("lease renewed to {} ms", NOW + TTL)
    );
    assert_eq!(
        decide_renewal(live_lease(), 7, 6, NOW, TTL).to_string(),
        "lease not renewed: RUN_FENCED"
    );
    assert_eq!(
        decide_heartbeat(live_lease(), 7, 7, NOW, TTL, RunStatus::Running).to_string(),
        format!("heartbeat renewed the lease to {} ms", NOW + TTL)
    );
    assert_eq!(
        decide_heartbeat(
            LeaseSnapshot {
                fencing_token: 7,
                acquired_at_ms: 0,
                expires_at_ms: NOW - 1,
            },
            7,
            7,
            NOW,
            TTL,
            RunStatus::Running
        )
        .to_string(),
        "heartbeat from the current generation, but the lease window has closed"
    );
    assert_eq!(
        decide_heartbeat(live_lease(), 7, 6, NOW, TTL, RunStatus::Running).to_string(),
        "heartbeat refused: RUN_FENCED"
    );
    assert_eq!(Generation::Current.to_string(), "current generation");
    assert_eq!(Generation::Stale.to_string(), "previous generation");
    assert_eq!(
        Generation::Ahead.to_string(),
        "generation ahead of the Task's current one"
    );
}

/// Clamping a non-positive TTL lands on `now`, not merely near it.
///
/// A zero or negative TTL cannot shorten a live lease. The previous
/// expectation of expiry=now contradicted the frozen renewal invariant.
#[test]
fn a_zero_ttl_preserves_the_existing_expiry() {
    let now = 1_000_000_000i64;
    let lease = LeaseSnapshot {
        fencing_token: 7,
        acquired_at_ms: now - 5_000,
        expires_at_ms: now + 30_000,
    };
    match decide_renewal(lease, 7, 7, now, 0) {
        RenewalDecision::Renewed { expires_at_ms } => assert_eq!(
            expires_at_ms, lease.expires_at_ms,
            "zero TTL must preserve the live window"
        ),
        other => panic!("a zero TTL on a live lease returned {other:?}"),
    }
    // And a negative TTL preserves the same live expiry, rather than to a different
    // floor that would be just as untested as the original.
    for ttl in [-1i64, i64::MIN, -9_000_000_000] {
        match decide_renewal(lease, 7, 7, now, ttl) {
            RenewalDecision::Renewed { expires_at_ms } => assert_eq!(
                expires_at_ms, lease.expires_at_ms,
                "a TTL of {ttl} must preserve the existing expiry, got {expires_at_ms}"
            ),
            other => panic!("a TTL of {ttl} on a live lease returned {other:?}"),
        }
    }
}

/// Every `refusal()` arm is reachable and says which refusal it was.
///
/// `RenewalDecision::refusal` and `HeartbeatDecision::refusal` both have a
/// "no refusal" arm, and neither was asserted. Rewriting `Renewed => Some(Fenced)`
/// kept the whole suite green, which means the only thing those two methods were
/// for — reporting *which* refusal, and reporting *none* — had no test at all.
#[test]
fn refusal_is_none_exactly_when_the_lease_was_renewed() {
    let now = 1_000_000i64;
    let lease = LeaseSnapshot {
        fencing_token: 7,
        acquired_at_ms: now - 5_000,
        expires_at_ms: now + 30_000,
    };
    // A closed window, same token: the case that must come back `Expired`
    // rather than `Fenced`. (The first version of this used struct update
    // syntax with `fencing_token: 7` — the field's existing value — so `stale`
    // was byte-identical to `lease` and the expired case never ran.)
    let stale = LeaseSnapshot {
        expires_at_ms: now - 1,
        ..lease
    };
    let live = decide_renewal(lease, 7, 7, now, 30_000);
    assert_eq!(live.refusal(), None, "a renewed lease has no refusal");
    assert!(live.renewed());

    for (label, decision) in [
        (
            "a stale generation",
            decide_renewal(lease, 7, 6, now, 30_000),
        ),
        (
            "a generation from the future",
            decide_renewal(lease, 7, 8, now, 30_000),
        ),
    ] {
        assert_eq!(
            decision.refusal(),
            Some(LeaseRefusal::Fenced),
            "{label} should be fenced"
        );
        assert!(!decision.renewed(), "{label} must not read as renewed");
    }

    // Deliberately not in the loop above: an expired lease is `Expired`, not
    // `Fenced`. Grouping it with the two generation cases would assert the very
    // conflation the two codes were split to avoid — and the first version of
    // this test did exactly that, which is why it failed.
    let expired = decide_renewal(stale, 7, 7, now, 30_000);
    assert_eq!(expired.refusal(), Some(LeaseRefusal::Expired));
    assert!(!expired.renewed());

    let beat = decide_heartbeat(lease, 7, 7, now, 30_000, RunStatus::Running);
    assert_eq!(beat.refusal(), None, "a clean heartbeat has no refusal");
    assert_eq!(
        decide_heartbeat(lease, 7, 6, now, 30_000, RunStatus::Running).refusal(),
        Some(RunWriteRefusal::Fenced)
    );
    // And the time-based half, which is deliberately *not* a refusal at all.
    // `decide_renewal` reports an expired lease as `Expired`; the heartbeat wraps
    // the same fact in its own `LeaseExpired` variant, because §3's deadline is
    // not a takeover and the caller must not read it as one. So the heartbeat's
    // `refusal()` is `None` here — which is the whole reason the two decisions
    // have different shapes, and the reason this assertion belongs next to it
    // rather than in the loop above.
    assert_eq!(
        decide_heartbeat(stale, 7, 7, now, 30_000, RunStatus::Running),
        HeartbeatDecision::LeaseExpired,
        "an expired lease must surface as the deadline variant, not as a refusal"
    );
    assert_eq!(
        decide_heartbeat(stale, 7, 7, now, 30_000, RunStatus::Running).refusal(),
        None,
        "a deadline is not a refusal: the executor still holds the current token"
    );
    // `HeartbeatDecision` carries the expiry, so its "renewed" answer is a
    // value, not a flag; assert it equals the renewal's rather than leaving the
    // two functions free to disagree.
    match (beat, live) {
        (
            HeartbeatDecision::Renewed {
                expires_at_ms: a, ..
            },
            RenewalDecision::Renewed { expires_at_ms: b },
        ) => {
            assert_eq!(
                a, b,
                "the heartbeat and the plain renewal disagree on the expiry"
            )
        }
        (a, b) => panic!("heartbeat {a:?} and renewal {b:?} took different shapes"),
    }
}

/// The frozen spellings of the three write kinds are asserted, not implied.
///
/// `RunWrite`'s `Display` is what a caller would log or put on the wire, and
/// changing `"output"` to `"OUTPUT"` left the suite green. §5 names the three
/// kinds in lowercase in the frozen spec, so that is what these assert — and
/// if the spec ever freezes a different spelling, this test is where the change
/// becomes a deliberate act rather than a silent one.
#[test]
fn the_three_write_kinds_spell_themselves_the_way_the_spec_does() {
    assert_eq!(RunWrite::Heartbeat.to_string(), "heartbeat");
    assert_eq!(RunWrite::Output.to_string(), "output");
    assert_eq!(RunWrite::Completion.to_string(), "completion");
    // A `match` on the display text, so a new variant without a spelling fails
    // to compile here rather than printing something nobody looked at.
    assert_eq!(
        [RunWrite::Heartbeat, RunWrite::Output, RunWrite::Completion].map(|w| w.to_string()),
        ["heartbeat", "output", "completion"]
    );
}

/// `Generation::ALL` order is part of what the type means, so it is pinned.
///
/// The ten reviews never found a consumer that depends on the order, so a
/// reorder is harmless today — which is exactly why it should be a deliberate
/// edit. `classify` is a tri-state over the same three arms, and the order here
/// is the order it reads in.
#[test]
fn generation_all_is_stale_current_ahead() {
    // The order that is actually in the source. Written down after this test
    // failed on its first run: I assumed the enumeration order and asserted
    // `Stale, Current, Ahead`. The type says `Current, Stale, Ahead` — which is
    // also the order `classify`'s documentation reads in, so it is the better
    // one to keep. The failure was mine, not the code's.
    assert_eq!(
        Generation::ALL,
        [Generation::Current, Generation::Stale, Generation::Ahead]
    );
}

/// A generation of zero has never been minted (0004 requires token >= 1).
#[test]
fn a_zero_generation_never_authorizes_writes_or_renewal() {
    let lease = LeaseSnapshot {
        fencing_token: 0,
        acquired_at_ms: NOW - 1,
        expires_at_ms: NOW + TTL,
    };
    for write in RunWrite::ALL {
        assert_eq!(
            decide_write(0, 0, write, RunStatus::Running),
            WriteDecision::Refuse {
                error_code: RunWriteRefusal::Fenced
            }
        );
    }
    assert_eq!(
        decide_renewal(lease, 0, 0, NOW, TTL),
        RenewalDecision::Refused {
            error_code: LeaseRefusal::Fenced
        }
    );
    assert_eq!(
        decide_heartbeat(lease, 0, 0, NOW, TTL, RunStatus::Running),
        HeartbeatDecision::Refused {
            error_code: RunWriteRefusal::Fenced
        }
    );
}

#[test]
fn ordinary_callbacks_cannot_extend_terminal_or_unknown_run_history() {
    for status in RunStatus::ALL {
        for write in RunWrite::ALL {
            let expected = if status.is_terminal() {
                WriteDecision::Refuse {
                    error_code: RunWriteRefusal::Terminal,
                }
            } else if status == RunStatus::Unknown {
                WriteDecision::Refuse {
                    error_code: RunWriteRefusal::ReconciliationRequired,
                }
            } else {
                // This gate cannot authorize a concrete transition. STARTING
                // can receive run.failed; the state machine still decides which
                // specific callback/command is legal for each active state.
                WriteDecision::Accept { write }
            };
            assert_eq!(
                decide_write(7, 7, write, status),
                expected,
                "status={status} write={write}"
            );
        }
    }
}

#[test]
fn stopped_and_unknown_runs_cannot_renew_even_a_matching_live_lease() {
    for status in RunStatus::ALL {
        let decision = decide_heartbeat(live_lease(), 7, 7, NOW, TTL, status);
        if status.is_terminal() {
            assert_eq!(
                decision,
                HeartbeatDecision::Refused {
                    error_code: RunWriteRefusal::Terminal
                }
            );
        } else if status == RunStatus::Unknown {
            assert_eq!(
                decision,
                HeartbeatDecision::Refused {
                    error_code: RunWriteRefusal::ReconciliationRequired
                }
            );
        } else {
            assert_eq!(
                decision,
                HeartbeatDecision::Renewed {
                    expires_at_ms: NOW + TTL
                }
            );
        }
    }
}

#[test]
fn stale_generation_refusal_precedes_run_status_refusal() {
    for status in RunStatus::ALL {
        for presented in [0, 6, 8] {
            for write in RunWrite::ALL {
                assert_eq!(
                    decide_write(7, presented, write, status),
                    WriteDecision::Refuse {
                        error_code: RunWriteRefusal::Fenced
                    }
                );
            }
            assert_eq!(
                decide_heartbeat(live_lease(), 7, presented, NOW, TTL, status),
                HeartbeatDecision::Refused {
                    error_code: RunWriteRefusal::Fenced
                }
            );
        }
    }
}

#[test]
fn callback_refusals_use_existing_catalogue_names() {
    for (reason, name) in [
        (RunWriteRefusal::Fenced, "RUN_FENCED"),
        (
            RunWriteRefusal::ReconciliationRequired,
            "RUN_STATUS_UNKNOWN",
        ),
        (RunWriteRefusal::Terminal, "DOMAIN_INVALID_TRANSITION"),
    ] {
        assert_eq!(reason.as_str(), name);
        assert_eq!(reason.to_string(), name);
    }
}
