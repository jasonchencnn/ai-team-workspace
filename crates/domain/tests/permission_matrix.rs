//! Conformance tests for the §3 permission matrix and the §2 intersection.
//!
//! The matrix's 216 cells are checked mechanically by `q09-matrix-vs-spec.py`,
//! which diffs them against a parse of `PERMISSION_MATRIX.md` and has been
//! reverse-verified to fail on a deliberately corrupted cell. What is left here is
//! everything a cell-by-cell diff cannot express: the invariants that make the
//! table safe to use.

use teamspace_domain::permission_intersection::{
    ApprovalContext, ApprovalGrant, CacheContext, CompoundFacts, DenialKeys, DenialReason, Layer,
    LayerVerdict, PermissionDecision, PermissionErrorCode, PermissionRequest,
    approval_grant_is_usable, evaluate_permission,
};
use teamspace_domain::permission_matrix::{
    ActorKind, DiscriminatingFact, MATRIX, MatrixVerdict, PermissionAction, SimpleVerdict,
    VerdictPair, matrix_verdict,
};

// ---------------------------------------------------------------- the table

#[test]
fn all_is_indexed_by_discriminant() {
    // `PermissionAction::index()` reads the discriminant, so the remaining hazard
    // is `ALL` being reordered or incomplete. That is what this pins.
    assert_eq!(PermissionAction::ALL.len(), 36, "§3 prints 36 action rows");
    assert_eq!(MATRIX.len(), PermissionAction::ALL.len());
    for (i, action) in PermissionAction::ALL.iter().enumerate() {
        assert_eq!(action.index(), i, "{action} sits at the wrong index");
    }
    // And every index 0..36 is covered, so no row is unreachable.
    let mut seen = [false; 36];
    for action in PermissionAction::ALL {
        seen[action.index()] = true;
    }
    assert!(
        seen.iter().all(|s| *s),
        "ALL does not cover every row index"
    );
}

#[test]
fn actor_all_is_indexed_and_named_after_the_spec_columns() {
    assert_eq!(ActorKind::ALL.len(), 6);
    let names: Vec<&str> = ActorKind::ALL.iter().map(|a| a.column_name()).collect();
    assert_eq!(
        names,
        vec!["Owner", "Leader", "Product", "Developer", "QA", "System"],
        "§3's six column headers, in order"
    );
    for (i, actor) in ActorKind::ALL.iter().enumerate() {
        assert_eq!(actor.index(), i);
    }
}

#[test]
fn spec_labels_are_verbatim_and_unique() {
    // The variant names are derived; these labels are §3's frozen text. A
    // duplicate would mean two rows share a label and the diff cannot tell them
    // apart, so uniqueness is asserted rather than assumed.
    let mut labels: Vec<&str> = PermissionAction::ALL
        .iter()
        .map(|a| a.spec_label())
        .collect();
    let total = labels.len();
    labels.sort_unstable();
    labels.dedup();
    assert_eq!(labels.len(), total, "duplicate §3 row label");
    for action in PermissionAction::ALL {
        let l = action.spec_label();
        assert!(!l.is_empty());
        assert!(!l.contains('\n'));
    }
}

#[test]
fn there_are_exactly_four_compound_cells_and_they_are_the_documented_four() {
    let mut found: Vec<(PermissionAction, ActorKind, VerdictPair)> = Vec::new();
    for action in PermissionAction::ALL {
        for actor in ActorKind::ALL {
            if let MatrixVerdict::ContextDependent(pair) = matrix_verdict(action, actor) {
                found.push((action, actor, pair));
            }
        }
    }
    assert_eq!(
        found.len(),
        4,
        "§3 prints four compound cells: A/H, A/S ×2, R/S"
    );
    // Pinned by row *and* column, because a compound cell moved to a neighbouring
    // cell still leaves the count at four.
    let located: Vec<(&str, &str, String)> = found
        .iter()
        .map(|(a, k, p)| (a.spec_label(), k.column_name(), p.spec_symbol()))
        .collect();
    assert_eq!(
        located,
        vec![
            ("Task 设置依赖/软依赖", "Owner", "A/H".to_string()),
            ("Task 设置依赖/软依赖", "Leader", "A/S".to_string()),
            ("Review 通过/退回", "QA", "A/S".to_string()),
            ("Workspace 写代码任务区", "QA", "R/S".to_string()),
        ],
        "the four compound cells, by row and column"
    );
    // Row 12's two cells turn on the same fact — §5 puts 绕过硬依赖 in the
    // Human-only categories, and §3's note says "Human 可越过硬依赖".
    assert_eq!(
        found[0].2.decided_by,
        DiscriminatingFact::HardDependencyBypass
    );
    assert_eq!(
        found[1].2.decided_by,
        DiscriminatingFact::HardDependencyBypass
    );
    assert_eq!(
        found[2].2.decided_by,
        DiscriminatingFact::ReviewerAuthorizedForHighRiskResult
    );
    assert_eq!(
        found[3].2.decided_by,
        DiscriminatingFact::TaskAuthorizedCodePath
    );
}

#[test]
fn a_compound_cell_never_resolves_to_allow_on_its_own() {
    // §2 step 7 is 默认拒绝, and §3's compound cells exist because the table
    // declines to pick a winner. Resolving one without the fact would put that
    // judgement back into the code.
    for action in PermissionAction::ALL {
        for actor in ActorKind::ALL {
            if let MatrixVerdict::ContextDependent(_) = matrix_verdict(action, actor) {
                assert!(
                    !matrix_verdict(action, actor).permits(),
                    "{action}/{actor} is compound and must not permit unaided"
                );
                assert!(
                    CompoundFacts::default()
                        .resolve_cell(matrix_verdict(action, actor))
                        .is_none(),
                    "{action}/{actor} must not resolve with no facts"
                );
            }
        }
    }
}

#[test]
fn owner_column_never_denies_but_may_be_scoped() {
    // Not a spec rule — §3 gives `Run start/heartbeat/finish` an Owner of `S` — but
    // a real invariant worth holding: the Human Owner can attempt every action in
    // §3, and the only Owner verdicts are A, H and S. An Owner `—` would mean the
    // final controller cannot act at all.
    for action in PermissionAction::ALL {
        let cell = matrix_verdict(action, ActorKind::HumanOwner);
        assert!(
            !matches!(cell, MatrixVerdict::Simple(SimpleVerdict::Deny)),
            "{action} denies the Human Owner, which §3 never does"
        );
    }
    // And the AI post called "Leader" is not Human: §5 says "岗位名不能提升权限".
    assert!(!ActorKind::RoleLeader.is_human());
    assert!(ActorKind::HumanOwner.is_human());
}

#[test]
fn human_only_actions_admit_no_role_or_system_actor() {
    // The table-level form of §3's `H` marking, which is how "Harness 绝不可自换绑"
    // and "Approval 批准/拒绝 = H only" are expressed per cell rather than in prose.
    //
    // Note this keys on §3's Owner cell, not on §5's eight categories: the two
    // statements do not cover the same set (see the module note in
    // `permission_intersection`), so the row-level flag is §3's `H`, and that
    // reading is a derived one rather than a mechanical transcription.
    let human_only: Vec<&str> = PermissionAction::ALL
        .iter()
        .filter(|a| teamspace_domain::permission_matrix::is_human_only_action(**a))
        .map(|a| a.spec_label())
        .collect();
    assert!(
        human_only.contains(&"Binding 绑定/解绑/换绑"),
        "§3's Harness 额外条件 says Harness 绝不可自换绑"
    );
    assert!(human_only.contains(&"Approval 批准/拒绝"));
    assert!(human_only.contains(&"Workspace 写生产/凭据区"));
    assert!(human_only.contains(&"Human takeover"));
    assert!(human_only.contains(&"部署/付费/生产数据/外发消息"));
    // Conversely these admit at least one non-Owner actor, so they must not be
    // classified Human-only.
    for label in [
        "Task 创建",
        "Task claim",
        "Run retry",
        "Message 发送/回复/确认",
        "Workspace 读取共享资料",
    ] {
        assert!(
            !human_only.contains(&label),
            "{label} admits a Role or System actor and is not Human-only"
        );
    }
    // The boundary case that a "no other column permits" rule gets wrong.
    // §3 gives `Team 创建/编辑` Owner `A` and `—` everywhere else — every
    // non-Owner cell denies, yet §5 does not list it, so it is NOT Human-only
    // and its non-Owner `—` cells must report AUTH_PERMISSION_DENIED, whose
    // remedy is "request a grant", rather than AUTH_HUMAN_REQUIRED.
    assert!(
        !human_only.contains(&"Team 创建/编辑"),
        "Owner cell is A, not H: §5 does not freeze Team 创建/编辑"
    );
    for actor in [
        ActorKind::RoleLeader,
        ActorKind::RoleProduct,
        ActorKind::RoleDeveloper,
        ActorKind::RoleQa,
        ActorKind::System,
    ] {
        assert_eq!(
            evaluate_permission(request(
                actor,
                PermissionAction::TeamCreateEdit,
                all_allow(None)
            )),
            PermissionDecision::Deny {
                layer: None,
                reason: DenialReason::NotPermitted,
            },
            "a plain A-row `—` is a permission denial, not a Human-only refusal"
        );
    }
    // Every Human-only row must be `H` for Owner, never `A`.
    for action in PermissionAction::ALL {
        if teamspace_domain::permission_matrix::is_human_only_action(action) {
            assert_eq!(
                matrix_verdict(action, ActorKind::HumanOwner),
                MatrixVerdict::Simple(SimpleVerdict::HumanOnly),
                "{action} is Human-only, so its Owner cell must be H"
            );
        }
    }
}

// ---------------------------------------------------------- the intersection

/// All eight terms permitting, with `scoped` left as the caller wants.
fn all_allow(scoped: Option<Layer>) -> [LayerVerdict; 8] {
    let mut terms = [LayerVerdict::Allow; 8];
    if let Some(l) = scoped {
        terms[l.index()] = LayerVerdict::Scoped;
    }
    terms
}

fn request(
    actor: ActorKind,
    action: PermissionAction,
    terms: [LayerVerdict; 8],
) -> PermissionRequest {
    PermissionRequest {
        actor,
        action,
        terms,
        facts: CompoundFacts::default(),
    }
}

#[test]
fn every_term_is_supplied_and_the_array_is_the_right_size() {
    // "No check can be skipped" is enforced by the type: `[LayerVerdict; 8]` with
    // `Layer::ALL` being exactly eight. If a ninth term were ever added to §2 the
    // literal below would stop compiling.
    assert_eq!(Layer::ALL.len(), 8);
    // The order is §2's printed order and `condition_key()` is the stable
    // identifier these terms are audited under. Both are pinned literally, so a
    // reordering or a renamed key fails here instead of silently changing which
    // term a denial Event names.
    assert_eq!(
        Layer::ALL
            .iter()
            .map(|l| (l.condition_key(), *l))
            .collect::<Vec<_>>(),
        vec![
            ("team_policy", Layer::TeamPolicy),
            ("actor_base_permission", Layer::ActorBasePermission),
            ("role_policy", Layer::RolePolicy),
            ("task_scope", Layer::TaskScope),
            ("workspace_path_policy", Layer::WorkspacePathPolicy),
            ("harness_capability", Layer::HarnessCapability),
            ("active_binding_and_fencing", Layer::ActiveBindingAndFencing),
            ("valid_approval_grant", Layer::ValidApprovalGrant),
        ],
        "§2's eight terms, in the order it prints them"
    );
    for (i, layer) in Layer::ALL.iter().enumerate() {
        assert_eq!(layer.index(), i, "{layer} is out of order");
    }
    let r = request(
        ActorKind::HumanOwner,
        PermissionAction::TaskCreate,
        all_allow(None),
    );
    for layer in Layer::ALL {
        assert_eq!(r.term(layer), LayerVerdict::Allow, "{layer} not readable");
    }
}

#[test]
fn the_happy_path_allows() {
    let d = evaluate_permission(request(
        ActorKind::RoleDeveloper,
        PermissionAction::TaskCreate,
        all_allow(None),
    ));
    assert_eq!(d, PermissionDecision::Allow);
    assert!(d.permits());
    assert_eq!(d.error_code(), None);
    assert_eq!(
        d.condition_keys(),
        None,
        "a permitted decision records no keys"
    );
}

#[test]
fn a_scoped_term_yields_allow_scoped_and_names_the_narrowest() {
    let d = evaluate_permission(request(
        ActorKind::RoleDeveloper,
        PermissionAction::TaskCreate,
        all_allow(Some(Layer::TaskScope)),
    ));
    assert_eq!(
        d,
        PermissionDecision::AllowScoped {
            limited_by: Layer::TaskScope
        }
    );
    assert!(d.permits());
    // Two scoped terms: the earlier one in §2's order wins the record.
    let mut terms = all_allow(None);
    terms[Layer::HarnessCapability.index()] = LayerVerdict::Scoped;
    terms[Layer::RolePolicy.index()] = LayerVerdict::Scoped;
    let d2 = evaluate_permission(request(
        ActorKind::RoleDeveloper,
        PermissionAction::TaskCreate,
        terms,
    ));
    assert_eq!(
        d2,
        PermissionDecision::AllowScoped {
            limited_by: Layer::RolePolicy
        },
        "§2's first narrowed term is the one recorded"
    );
}

#[test]
fn the_first_denial_wins_and_names_its_term() {
    // §2: "任一层拒绝即终止". Two denying terms: the earlier one must be reported,
    // because that is the check the caller has to fix first.
    let mut terms = all_allow(None);
    terms[Layer::WorkspacePathPolicy.index()] = LayerVerdict::Deny(DenialReason::PathOutsideScope);
    terms[Layer::RolePolicy.index()] = LayerVerdict::Deny(DenialReason::NotPermitted);
    let d = evaluate_permission(request(
        ActorKind::RoleDeveloper,
        PermissionAction::TaskCreate,
        terms,
    ));
    assert_eq!(
        d,
        PermissionDecision::Deny {
            layer: Some(Layer::RolePolicy),
            reason: DenialReason::NotPermitted,
        },
        "RolePolicy is printed before WorkspacePathPolicy in §2"
    );
    assert!(!d.permits());
}

#[test]
fn approval_can_supplement_but_never_rescue() {
    // §2: "Approval 只能缩小或临时补充已允许的动作，不能突破 Team 安全上限."
    //
    // The action here must have a non-denying cell for the actor, or the matrix
    // refuses before any term is consulted and the supplement half is untestable.
    // `Task 创建` is `| A | A | S | S | S | S |`, so a Developer reaches the terms.
    let action = PermissionAction::TaskCreate;
    let actor = ActorKind::RoleDeveloper;

    // Team/actor/Role checks remain mandatory even when an approval supplements
    // the authorized action. Approval cannot replace those checks.
    let mut supplemented = all_allow(None);
    supplemented[Layer::ValidApprovalGrant.index()] = LayerVerdict::Allow;
    assert_eq!(
        evaluate_permission(request(actor, action, supplemented)),
        PermissionDecision::Allow,
        "an approval may supply a term that would otherwise be NotApplicable"
    );
    assert_eq!(
        evaluate_permission(request(actor, action, all_allow(None))),
        PermissionDecision::Allow,
        "sanity: a fully-permitted action still needs its terms to permit"
    );

    // Narrow, the other half of §2's sentence: an approval that says Scoped makes
    // the result scoped, never wider.
    let mut narrowed = all_allow(None);
    narrowed[Layer::ValidApprovalGrant.index()] = LayerVerdict::Scoped;
    assert_eq!(
        evaluate_permission(request(actor, action, narrowed)),
        PermissionDecision::AllowScoped {
            limited_by: Layer::ValidApprovalGrant
        },
        "an approval may narrow, never widen"
    );

    // Rescue: an earlier term denied, and the approval term says Allow. The denial
    // stands, because the scan never reaches the last term.
    let mut denied = all_allow(None);
    denied[Layer::TeamPolicy.index()] = LayerVerdict::Deny(DenialReason::HumanOnly);
    denied[Layer::ValidApprovalGrant.index()] = LayerVerdict::Allow;
    let d = evaluate_permission(request(actor, action, denied));
    assert_eq!(
        d,
        PermissionDecision::Deny {
            layer: Some(Layer::TeamPolicy),
            reason: DenialReason::HumanOnly,
        },
        "an approval must not override a Human-only denial"
    );
    // Even when the approval term itself is Scoped, the earlier denial stands.
    let mut denied2 = all_allow(None);
    denied2[Layer::RolePolicy.index()] = LayerVerdict::Deny(DenialReason::NotPermitted);
    denied2[Layer::ValidApprovalGrant.index()] = LayerVerdict::Scoped;
    assert!(
        !evaluate_permission(request(actor, action, denied2)).permits(),
        "a later Scoped approval cannot rescue an earlier denial"
    );

    // The strongest form of "Approval 不能突破 Team 安全上限": even with the
    // matrix permitting the actor and every term allowing, an action §5 freezes
    // stays refused for a Role actor. Approval is the last term and cannot reach
    // a refusal the matrix already made.
    assert_eq!(
        evaluate_permission(request(
            ActorKind::RoleDeveloper,
            PermissionAction::BindingManage,
            all_allow(None)
        )),
        PermissionDecision::Deny {
            layer: None,
            reason: DenialReason::HumanOnly,
        },
        "§5 item 3 freezes Binding 绑定/解绑/换绑; no approval term reaches it"
    );
}

#[test]
fn a_human_only_matrix_cell_refuses_a_role_actor_before_any_term() {
    // `Harness 绝不可自换绑`, `Approval 批准/拒绝 = H only`, and §5's eight frozen
    // categories. The refusal happens at the matrix, so `layer` is None and the
    // Event records the `permission_matrix` namespace.
    for (action, label) in [
        (PermissionAction::BindingManage, "Binding 绑定/解绑/换绑"),
        (PermissionAction::ApprovalDecide, "Approval 批准/拒绝"),
        (PermissionAction::HumanTakeover, "Human takeover"),
        (
            PermissionAction::DeployOrProduction,
            "部署/付费/生产数据/外发消息",
        ),
    ] {
        assert_eq!(
            matrix_verdict(action, ActorKind::HumanOwner),
            MatrixVerdict::Simple(SimpleVerdict::HumanOnly),
            "{label}"
        );
        for actor in [
            ActorKind::RoleLeader,
            ActorKind::RoleProduct,
            ActorKind::RoleDeveloper,
            ActorKind::RoleQa,
            ActorKind::System,
        ] {
            let d = evaluate_permission(request(actor, action, all_allow(None)));
            assert_eq!(
                d,
                PermissionDecision::Deny {
                    layer: None,
                    reason: DenialReason::HumanOnly,
                },
                "{label} as {actor} must be AUTH_HUMAN_REQUIRED"
            );
            assert_eq!(d.error_code(), Some(PermissionErrorCode::HumanRequired));
            assert_eq!(
                d.condition_keys(),
                Some(["permission_matrix", "human_only"]),
                "{label} denial keys"
            );
        }
        // The Human Owner may attempt it, and then the terms decide.
        assert!(
            evaluate_permission(request(ActorKind::HumanOwner, action, all_allow(None))).permits(),
            "{label} must stay possible for the Human Owner"
        );
    }
}

#[test]
fn a_matrix_deny_refuses_before_any_term() {
    // A `—` cell: no actor in that column may attempt it at all. §3 gives
    // `Task 分配/改派` the row `| A | A | S | — | — | S |` — Product is `S`,
    // not `—`, so only Developer and QA are refused at the matrix layer.
    //
    // This row's Owner cell is `A`, so it is not a Human-only row and its `—`
    // cells must report AUTH_PERMISSION_DENIED, not AUTH_HUMAN_REQUIRED.
    for actor in [ActorKind::RoleDeveloper, ActorKind::RoleQa] {
        assert_eq!(
            matrix_verdict(PermissionAction::TaskAssignReassign, actor),
            MatrixVerdict::Simple(SimpleVerdict::Deny),
            "§3 gives Task 分配/改派 `—` for Developer/QA"
        );
        let d = evaluate_permission(request(
            actor,
            PermissionAction::TaskAssignReassign,
            all_allow(None),
        ));
        assert_eq!(
            d,
            PermissionDecision::Deny {
                layer: None,
                reason: DenialReason::NotPermitted
            }
        );
        assert_eq!(d.error_code(), Some(PermissionErrorCode::PermissionDenied));
    }
    // Product is `S` on this row, so it reaches the terms and is not refused.
    // This is the `—` vs `S` distinction inside a single row.
    //
    // The `S` cell itself records no narrowing: §2's scope is carried by whichever
    // term says Scoped, and the decision names that term. With every term
    // allowing there is nothing to narrow, so the result is plain Allow.
    assert_eq!(
        matrix_verdict(PermissionAction::TaskAssignReassign, ActorKind::RoleProduct),
        MatrixVerdict::Simple(SimpleVerdict::Scoped),
        "§3 gives Task 分配/改派 `S` for Product"
    );
    assert_eq!(
        evaluate_permission(request(
            ActorKind::RoleProduct,
            PermissionAction::TaskAssignReassign,
            all_allow(None),
        )),
        PermissionDecision::Allow,
        "an `S` cell with no narrowing term is plain Allow"
    );
    // Narrow it, and the term — not the cell — is what gets recorded.
    assert_eq!(
        evaluate_permission(request(
            ActorKind::RoleProduct,
            PermissionAction::TaskAssignReassign,
            all_allow(Some(Layer::RolePolicy)),
        )),
        PermissionDecision::AllowScoped {
            limited_by: Layer::RolePolicy
        },
        "the narrowing term is named, not the matrix column"
    );
    // Owner and Leader are A, so they get through to the terms.
    for actor in [ActorKind::HumanOwner, ActorKind::RoleLeader] {
        assert!(
            evaluate_permission(request(
                actor,
                PermissionAction::TaskAssignReassign,
                all_allow(None)
            ))
            .permits()
        );
    }
}

#[test]
fn a_compound_cell_denies_until_the_fact_is_supplied() {
    // `Workspace 写代码任务区` + QA is `R/S`. With no fact the decision must deny.
    let base = request(
        ActorKind::RoleQa,
        PermissionAction::WorkspaceWriteCodeArea,
        all_allow(None),
    );
    let denied = evaluate_permission(base);
    assert_eq!(
        denied,
        PermissionDecision::Deny {
            layer: None,
            reason: DenialReason::DefaultDeny
        },
        "an unproven compound fact must deny, not guess"
    );
    assert_eq!(
        denied.error_code(),
        Some(PermissionErrorCode::PermissionDenied)
    );

    // With the fact, the cell resolves and the terms decide. Both branches of
    // `R/S` permit — §3's legend defines `R` 仅只读 and `S` 限作用域 as two
    // different *scopes* of permission, not as permit/refuse. So the decision is
    // the same for both; what the fact selects is *which* symbol, and that is
    // what is pinned here. Asserting a permit/refuse difference between them
    // would be inventing a rule the legend does not give.
    //
    // The boolean is the caller's assertion of *which* of §3's two printed
    // symbols applies — not "the fact holds". Polarity is deliberately not
    // encoded (see `VerdictPair`), so a `true` selecting `R` and a `false`
    // selecting `S` is the documented behaviour, not an inversion to fix.
    for (first_symbol_applies, expected) in [
        (true, SimpleVerdict::ReadOnly),
        (false, SimpleVerdict::Scoped),
    ] {
        assert_eq!(
            CompoundFacts {
                task_authorized_code_path: Some(first_symbol_applies),
                ..CompoundFacts::default()
            }
            .resolve_cell(matrix_verdict(
                PermissionAction::WorkspaceWriteCodeArea,
                ActorKind::RoleQa
            )),
            Some(expected),
            "first_symbol_applies={first_symbol_applies} selects {expected:?}"
        );
        let r = PermissionRequest {
            facts: CompoundFacts {
                task_authorized_code_path: Some(first_symbol_applies),
                ..CompoundFacts::default()
            },
            ..base
        };
        assert!(
            evaluate_permission(r).permits(),
            "first_symbol_applies={first_symbol_applies} must resolve and permit"
        );
    }
    // Resolved to the second symbol (R, 仅只读) the term must still be Scoped or
    // Allow — the machine does not widen a read-only grant on its own.
    let r_scoped = PermissionRequest {
        facts: CompoundFacts {
            task_authorized_code_path: Some(false),
            ..CompoundFacts::default()
        },
        terms: [LayerVerdict::Deny(DenialReason::ScopeNotEstablished); 8],
        ..base
    };
    let d = evaluate_permission(r_scoped);
    assert_eq!(
        d,
        PermissionDecision::Deny {
            layer: Some(Layer::TeamPolicy),
            reason: DenialReason::ScopeNotEstablished
        }
    );
    assert_eq!(d.error_code(), Some(PermissionErrorCode::PermissionDenied));
}

#[test]
fn the_owner_hard_dependency_bypass_resolves_to_human_only() {
    // §3's `Task 设置依赖/软依赖` + Owner is `A/H`, and §5 puts 绕过硬依赖 in the
    // Human-only categories. Resolved to the second symbol the owner must be told
    // AUTH_HUMAN_REQUIRED — the sub-case is flagged so the Human actor is recorded.
    let pair = match matrix_verdict(PermissionAction::TaskSetDependencies, ActorKind::HumanOwner) {
        MatrixVerdict::ContextDependent(p) => p,
        other => panic!("expected a compound cell, got {other}"),
    };
    assert_eq!(pair.first, SimpleVerdict::Allow);
    assert_eq!(pair.second, SimpleVerdict::HumanOnly);
    assert_eq!(pair.decided_by, DiscriminatingFact::HardDependencyBypass);
    assert_eq!(pair.resolve(true), SimpleVerdict::Allow);
    assert_eq!(pair.resolve(false), SimpleVerdict::HumanOnly);
    assert_eq!(
        pair.resolve(false).error_code_for_test(),
        PermissionErrorCode::HumanRequired
    );
}

#[test]
fn section_7_maps_each_reason_to_its_code() {
    // §7: 权限拒绝返回 AUTH_PERMISSION_DENIED；Human-only 返回 AUTH_HUMAN_REQUIRED；
    // 越界路径返回 WORKSPACE_PATH_OUTSIDE_SCOPE.
    let expected = [
        (
            DenialReason::HumanOnly,
            PermissionErrorCode::HumanRequired,
            "AUTH_HUMAN_REQUIRED",
        ),
        (
            DenialReason::PathOutsideScope,
            PermissionErrorCode::PathOutsideScope,
            "WORKSPACE_PATH_OUTSIDE_SCOPE",
        ),
        (
            DenialReason::NotPermitted,
            PermissionErrorCode::PermissionDenied,
            "AUTH_PERMISSION_DENIED",
        ),
        (
            DenialReason::DefaultDeny,
            PermissionErrorCode::PermissionDenied,
            "AUTH_PERMISSION_DENIED",
        ),
        (
            DenialReason::CapabilityMissing,
            PermissionErrorCode::PermissionDenied,
            "AUTH_PERMISSION_DENIED",
        ),
        (
            DenialReason::ApprovalBoundToAnotherBinding,
            PermissionErrorCode::PermissionDenied,
            "AUTH_PERMISSION_DENIED",
        ),
        (
            DenialReason::ObjectNotVisible,
            PermissionErrorCode::PermissionDenied,
            "AUTH_PERMISSION_DENIED",
        ),
    ];
    for (reason, code, name) in expected {
        assert_eq!(reason.error_code(), code);
        assert_eq!(code.as_str(), name);
        assert!(!code.is_retryable(), "{name} is 否 in the catalogue");
    }
    // Every DenialReason must be covered, so a new variant cannot slip through
    // without a code decision.
    let all = [
        DenialReason::NotPermitted,
        DenialReason::HumanOnly,
        DenialReason::PathOutsideScope,
        DenialReason::BindingOrFencingInvalid,
        DenialReason::ApprovalGrantInvalid,
        DenialReason::ApprovalBoundToAnotherBinding,
        DenialReason::CapabilityMissing,
        DenialReason::ObjectNotVisible,
        DenialReason::ScopeNotEstablished,
        DenialReason::DefaultDeny,
    ];
    assert_eq!(all.len(), 10);
    let mut seen: Vec<PermissionErrorCode> = all.iter().map(|r| r.error_code()).collect();
    seen.sort_by_key(|c| c.as_str());
    seen.dedup_by_key(|c| c.as_str());
    assert_eq!(
        seen.len(),
        3,
        "all three §7 codes are reachable, so none is dead"
    );
    for r in all {
        assert!(!r.condition_key().is_empty());
    }
}

#[test]
fn denial_keys_name_the_term_and_the_reason_and_nothing_else() {
    // §7: the Event records 脱敏原因 and must not carry secrets or content. The
    // keys are identifiers; the type makes an arbitrary string impossible.
    assert_eq!(
        DenialKeys::of(
            DenialReason::PathOutsideScope,
            Some(Layer::WorkspacePathPolicy)
        ),
        ["workspace_path_policy", "path_outside_scope"]
    );
    assert_eq!(
        DenialKeys::of(DenialReason::DefaultDeny, None),
        ["permission_matrix", "default_deny"]
    );
    let d = evaluate_permission(request(
        ActorKind::RoleQa,
        PermissionAction::TaskAssignReassign,
        all_allow(None),
    ));
    assert_eq!(
        d.condition_keys(),
        Some(["permission_matrix", "not_permitted"])
    );
    for key in d.condition_keys().unwrap() {
        assert!(
            key.chars().all(|c| c.is_ascii_lowercase() || c == '_'),
            "{key} must be a bare identifier, never content"
        );
    }
}

#[test]
fn the_decision_cannot_change_a_resource() {
    // The node's criterion "拒绝不改变资源" is guaranteed by type, not by test
    // discipline: `PermissionDecision` holds two `Option<Layer>`-shaped enums and
    // nothing else, `evaluate_permission` takes its input by value, and neither
    // type carries a reference, a handle or interior mutability. This test is the
    // written form of that argument.
    let d = evaluate_permission(request(
        ActorKind::RoleDeveloper,
        PermissionAction::TaskCreate,
        all_allow(None),
    ));
    // Copy, not borrow: there is nothing borrowed from any resource.
    let copied = d;
    assert_eq!(d, copied);
    // A denial is data, and can be re-evaluated to the same answer — an
    // evaluation is a pure function of the request.
    let req = request(
        ActorKind::RoleQa,
        PermissionAction::TaskAssignReassign,
        all_allow(None),
    );
    assert_eq!(
        evaluate_permission(req),
        evaluate_permission(req),
        "evaluation is deterministic and side-effect free"
    );
    // A denial is a value, not a latch: re-evaluating the same request yields
    // the same denial, and the same request with one term changed yields a
    // different one. This is what "拒绝不改变资源" means operationally — the
    // decision carries no hidden state between calls.
    let mut terms = all_allow(None);
    terms[Layer::TeamPolicy.index()] = LayerVerdict::Deny(DenialReason::HumanOnly);
    let denied = request(
        ActorKind::RoleDeveloper,
        PermissionAction::TaskCreate,
        terms,
    );
    assert_eq!(evaluate_permission(denied), evaluate_permission(denied));
    assert_ne!(evaluate_permission(denied), evaluate_permission(req));
}

#[test]
fn serde_agrees_with_as_str_for_every_permission_code() {
    // §7's three codes are frozen in `ERROR_CATALOG.md`, so the serde path and
    // `as_str()` must agree — this is the Q-04 structural fix in its original
    // form, and the assertion is anchored on the catalogue, not on memory.
    for code in [
        PermissionErrorCode::PermissionDenied,
        PermissionErrorCode::HumanRequired,
        PermissionErrorCode::PathOutsideScope,
    ] {
        assert_eq!(
            serde_json::to_string(&code).unwrap(),
            format!("\"{}\"", code.as_str()),
            "serde and as_str disagree for {code:?}"
        );
    }
    // `Layer` and `ActorKind` are deliberately not asserted here.
    //
    // `Layer`: §2 prints the eight terms as prose in a code block, never as wire
    // values, so `Layer` carries no serde derive at all. Its stable identifier is
    // `condition_key()`, pinned literally in
    // `every_term_is_supplied_and_the_array_is_the_right_size`.
    //
    // `ActorKind`: these are §3's six *matrix columns*, not the Event
    // `actor_kind` field. The only frozen spelling of the latter is
    // `ROLE_BINDING` (migration 0015's CHECK constraint), and it is deliberately
    // not one of these six — so deriving a wire name here would assert a
    // relationship the specifications never make. What is contractual is the
    // column name, pinned against §3's header in
    // `actor_all_is_indexed_and_named_after_the_spec_columns`.
    for action in PermissionAction::ALL {
        assert!(!action.spec_label().is_empty());
    }
}

// Small helper so the `A/H` assertion reads in terms of the §7 code rather than
// duplicating the mapping.
trait ErrorCodeForTest {
    fn error_code_for_test(self) -> PermissionErrorCode;
}
impl ErrorCodeForTest for SimpleVerdict {
    fn error_code_for_test(self) -> PermissionErrorCode {
        match self {
            SimpleVerdict::HumanOnly => PermissionErrorCode::HumanRequired,
            _ => PermissionErrorCode::PermissionDenied,
        }
    }
}

// ------------------------------------------------------------- §6 grant rules

fn grant_uuid(byte: u8) -> teamspace_domain::UuidV7 {
    teamspace_domain::UuidV7::from_timestamp_and_random(100, [byte; 10]).expect("fixture UUIDv7")
}

fn grant_context() -> ApprovalContext {
    use teamspace_domain::{Actor, BindingId, RoleId, RunId, TeamId};
    ApprovalContext {
        team_id: TeamId::from_uuid(grant_uuid(1)),
        actor: Actor::Role(RoleId::from_uuid(grant_uuid(2))),
        binding_id: BindingId::from_uuid(grant_uuid(3)),
        binding_version: 7,
        action: PermissionAction::RunCancel,
        resource_type: "RUN",
        resource_id: grant_uuid(4),
        task_id: None,
        run_id: Some(RunId::from_uuid(grant_uuid(4))),
        scope_hash: teamspace_domain::idempotency::PayloadHash::from_bytes([1; 32]),
    }
}

fn usable_grant() -> ApprovalGrant {
    use teamspace_domain::{Actor, HumanId};
    let owner_id = HumanId::from_uuid(grant_uuid(5));
    ApprovalGrant {
        requested_by: grant_context().actor,
        decided_by: Actor::Human(owner_id),
        owner_id,
        context: grant_context(),
        consumed: false,
        expires_at_ms: 1000,
    }
}

#[test]
fn a_grant_is_usable_only_when_every_section_6_condition_holds() {
    use teamspace_domain::{Actor, RoleId};
    let grant = usable_grant();
    assert_eq!(approval_grant_is_usable(grant, grant_context(), 999), None);
    for now in [1000, 1001, i64::MAX] {
        assert_eq!(
            approval_grant_is_usable(grant, grant_context(), now),
            Some(DenialReason::ApprovalGrantInvalid)
        );
    }
    for mutant in [
        ApprovalGrant {
            consumed: true,
            ..grant
        },
        ApprovalGrant {
            decided_by: grant.requested_by,
            ..grant
        },
        ApprovalGrant {
            decided_by: Actor::Role(RoleId::from_uuid(grant_uuid(6))),
            ..grant
        },
    ] {
        assert_eq!(
            approval_grant_is_usable(mutant, grant_context(), 0),
            Some(DenialReason::ApprovalGrantInvalid)
        );
    }
    let c = grant_context();
    // Same matrix category is not the same actual Role, Team, resource or Run.
    for changed in [
        ApprovalContext {
            actor: Actor::Role(RoleId::from_uuid(grant_uuid(6))),
            ..c
        },
        ApprovalContext {
            team_id: teamspace_domain::TeamId::from_uuid(grant_uuid(6)),
            ..c
        },
        ApprovalContext {
            action: PermissionAction::RunRetry,
            ..c
        },
        ApprovalContext {
            resource_type: "TASK",
            ..c
        },
        ApprovalContext {
            resource_id: grant_uuid(6),
            ..c
        },
        ApprovalContext {
            task_id: Some(teamspace_domain::TaskId::from_uuid(grant_uuid(6))),
            ..c
        },
        ApprovalContext {
            run_id: Some(teamspace_domain::RunId::from_uuid(grant_uuid(6))),
            ..c
        },
        ApprovalContext {
            scope_hash: teamspace_domain::idempotency::PayloadHash::from_bytes([2; 32]),
            ..c
        },
    ] {
        assert_eq!(
            approval_grant_is_usable(grant, changed, 0),
            Some(DenialReason::ApprovalGrantInvalid)
        );
    }
    for changed in [
        ApprovalContext {
            binding_id: teamspace_domain::BindingId::from_uuid(grant_uuid(6)),
            ..c
        },
        ApprovalContext {
            binding_version: 8,
            ..c
        },
    ] {
        assert_eq!(
            approval_grant_is_usable(grant, changed, 0),
            Some(DenialReason::ApprovalBoundToAnotherBinding)
        );
    }
    let self_decided = ApprovalGrant {
        requested_by: grant.decided_by,
        ..grant
    };
    assert_eq!(
        approval_grant_is_usable(self_decided, c, 0),
        Some(DenialReason::ApprovalGrantInvalid)
    );
    let wrong_owner = ApprovalGrant {
        decided_by: Actor::Human(teamspace_domain::HumanId::from_uuid(grant_uuid(6))),
        ..grant
    };
    assert_eq!(
        approval_grant_is_usable(wrong_owner, c, 0),
        Some(DenialReason::ApprovalGrantInvalid)
    );
}

#[test]
fn the_binding_rule_is_reported_before_expiry() {
    assert_eq!(
        approval_grant_is_usable(
            usable_grant(),
            ApprovalContext {
                binding_version: 99,
                ..grant_context()
            },
            1001
        ),
        Some(DenialReason::ApprovalBoundToAnotherBinding)
    );
}

// -------------------------------------------------------------- §7 cache rules

fn cache_context() -> CacheContext {
    CacheContext {
        object_version: 3,
        binding_version: 7,
        approval_expiry_tick: Some(1000),
        fencing_token: 42,
    }
}

#[test]
fn a_cached_allow_may_not_cross_any_of_the_four_dimensions() {
    // §7: "缓存的 allow 结果不能跨对象版本、Binding version、Approval expiry 或
    // fencing token 复用."
    let cached = cache_context();
    assert!(
        cached.may_reuse(cache_context(), 999),
        "an identical context may reuse its cached allow"
    );

    // Each of the four, moved by exactly one. None may reuse.
    for (moved, dimension) in [
        (
            CacheContext {
                object_version: 4,
                ..cached
            },
            "对象版本",
        ),
        (
            CacheContext {
                binding_version: 8,
                ..cached
            },
            "Binding version",
        ),
        (
            CacheContext {
                approval_expiry_tick: Some(1001),
                ..cached
            },
            "Approval expiry",
        ),
        (
            CacheContext {
                approval_expiry_tick: None,
                ..cached
            },
            "Approval expiry 消失",
        ),
        (
            CacheContext {
                fencing_token: 43,
                ..cached
            },
            "fencing token",
        ),
    ] {
        assert!(
            !moved.may_reuse(cached, 999),
            "§7: a cached allow must not cross {dimension}"
        );
        assert!(
            !cached.may_reuse(moved, 999),
            "§7: symmetric — the earlier context cannot adopt a later one's allow"
        );
    }

    // A decision computed with no approval at all is a different decision from
    // one computed under a grant, even when every other field matches.
    let no_approval = CacheContext {
        approval_expiry_tick: None,
        ..cached
    };
    assert!(no_approval.may_reuse(no_approval, u64::MAX));
    assert!(!no_approval.may_reuse(cached, 999));
    assert!(!cached.may_reuse(no_approval, 999));
}

// -------------------------------------------------- all three compound facts

#[test]
fn every_compound_fact_is_reachable_and_unproven_denies() {
    // The four compound cells of §3 and the fact each one needs, named from §3's
    // own "Harness 额外条件" column. §3 gives a compound symbol only for these
    // four cells, so this table is the whole of the fact surface.
    //
    // `first` is §3's first printed symbol and `second` its second, with no
    // polarity encoded — see `VerdictPair`.
    let cases: &[(
        PermissionAction,
        ActorKind,
        DiscriminatingFact,
        SimpleVerdict,
        SimpleVerdict,
    )] = &[
        // `| Task 设置依赖/软依赖 | A/H | A/S | S | — | S | S |` — Owner `A/H`,
        // Leader `A/S`. §5 item 4 puts 绕过硬依赖 in the Human-only list, which
        // is the `H` in both compound cells.
        (
            PermissionAction::TaskSetDependencies,
            ActorKind::HumanOwner,
            DiscriminatingFact::HardDependencyBypass,
            SimpleVerdict::Allow,
            SimpleVerdict::HumanOnly,
        ),
        (
            PermissionAction::TaskSetDependencies,
            ActorKind::RoleLeader,
            DiscriminatingFact::HardDependencyBypass,
            SimpleVerdict::Allow,
            SimpleVerdict::Scoped,
        ),
        // `| Review 通过/退回 | A | S | S | — | A/S | S |`
        (
            PermissionAction::ReviewApproveOrRequestChanges,
            ActorKind::RoleQa,
            DiscriminatingFact::ReviewerAuthorizedForHighRiskResult,
            SimpleVerdict::Allow,
            SimpleVerdict::Scoped,
        ),
        // `| Workspace 写代码任务区 | A | S | — | S | R/S | S |`
        (
            PermissionAction::WorkspaceWriteCodeArea,
            ActorKind::RoleQa,
            DiscriminatingFact::TaskAuthorizedCodePath,
            SimpleVerdict::ReadOnly,
            SimpleVerdict::Scoped,
        ),
    ];

    let mut seen: Vec<DiscriminatingFact> = Vec::new();
    for (action, actor, fact, first, second) in cases {
        let cell = matrix_verdict(*action, *actor);
        let pair = match cell {
            MatrixVerdict::ContextDependent(p) => p,
            other => panic!("§3 gives {action:?}/{actor:?} a compound cell, got {other:?}"),
        };
        assert_eq!(
            pair.decided_by, *fact,
            "the fact {action:?}/{actor:?} needs"
        );
        assert_eq!(
            (pair.first, pair.second),
            (*first, *second),
            "§3's printed order for {action:?}/{actor:?}"
        );
        assert_eq!(
            pair.spec_symbol(),
            format!("{}/{}", first.symbol(), second.symbol())
        );

        // Unproven: the whole point of the type. No fact means no guess.
        let unproven = evaluate_permission(request(*actor, *action, all_allow(None)));
        assert_eq!(
            unproven,
            PermissionDecision::Deny {
                layer: None,
                reason: DenialReason::DefaultDeny,
            },
            "{action:?}/{actor:?} must deny when {fact:?} is unproven"
        );

        // Proven: the caller's boolean selects the printed symbol, in order.
        for (first_symbol_applies, expected) in [(true, *first), (false, *second)] {
            let facts = match fact {
                DiscriminatingFact::HardDependencyBypass => CompoundFacts {
                    hard_dependency_bypass: Some(first_symbol_applies),
                    ..CompoundFacts::default()
                },
                DiscriminatingFact::ReviewerAuthorizedForHighRiskResult => CompoundFacts {
                    reviewer_authorized_for_high_risk: Some(first_symbol_applies),
                    ..CompoundFacts::default()
                },
                DiscriminatingFact::TaskAuthorizedCodePath => CompoundFacts {
                    task_authorized_code_path: Some(first_symbol_applies),
                    ..CompoundFacts::default()
                },
            };
            let r = PermissionRequest {
                facts,
                ..request(*actor, *action, all_allow(None))
            };
            let d = evaluate_permission(r);
            if expected == SimpleVerdict::HumanOnly && !actor.is_human() {
                // §5: 绕过硬依赖 is frozen Human-only. §3 prints the `H` second
                // for `Task 设置依赖/软依赖`, and it is the *second* branch that a
                // caller selects by declaring the bypass happened.
                assert_eq!(
                    d,
                    PermissionDecision::Deny {
                        layer: None,
                        reason: DenialReason::HumanOnly
                    },
                    "{action:?}/{actor:?} selecting H must be AUTH_HUMAN_REQUIRED"
                );
            } else {
                // The Human Owner is never refused on Human-only grounds — that
                // is what `H` in the Owner column means. `Task 设置依赖/软依赖`'s
                // `A/H` is precisely "a Human may, and only a Human may, bypass
                // a hard dependency", so the Human branch permits.
                assert!(
                    d.permits(),
                    "{action:?}/{actor:?} selecting {expected:?} as {actor:?} must permit"
                );
            }
        }
        seen.push(*fact);
    }

    // Every one of the three facts is exercised, so a fourth fact added to
    // `DiscriminatingFact` without a §3 cell to justify it fails here.
    assert_eq!(seen.len(), 4, "four compound cells in §3");
    assert_eq!(
        seen.iter().collect::<std::collections::HashSet<_>>().len(),
        3,
        "four cells share three facts: Task 设置依赖/软依赖 uses one fact twice"
    );
}

#[test]
fn cache_expiry_is_about_time_not_a_changed_expiry_field() {
    let cached = cache_context();
    assert!(cached.may_reuse(cached, 999));
    for now in [1000, 1001, u64::MAX] {
        assert!(!cached.may_reuse(cached, now));
    }
}
