use teamspace_domain::outbox_dispatch::{
    DeliveryFailure, OutboxEntry, OutboxStatus, OutcomeDecision, RetryPolicy, decide_outcome,
};
use teamspace_domain::permission_intersection::{
    ApprovalContext, ApprovalGrant, CacheContext, approval_grant_is_usable,
};
use teamspace_domain::{
    ActorKind, CompoundFacts, LayerVerdict, LeaseSnapshot, PermissionAction, PermissionRequest,
    RenewalDecision, decide_renewal, evaluate_permission,
};

#[test]
fn review_missing_required_permissions_denies() {
    let request = PermissionRequest {
        actor: ActorKind::HumanOwner,
        action: PermissionAction::TeamCreateEdit,
        terms: [LayerVerdict::NotApplicable; 8],
        facts: CompoundFacts::default(),
    };
    assert!(!evaluate_permission(request).permits());
}

#[test]
fn review_non_human_decider_denies() {
    use teamspace_domain::{Actor, BindingId, HumanId, RoleId, TeamId, UuidV7};
    let id = UuidV7::from_timestamp_and_random(100, [0; 10]).expect("fixture");
    let context = ApprovalContext {
        team_id: TeamId::from_uuid(id),
        actor: Actor::Role(RoleId::from_uuid(id)),
        binding_id: BindingId::from_uuid(id),
        binding_version: 7,
        action: PermissionAction::RunCancel,
        resource_type: "RUN",
        resource_id: id,
        task_id: None,
        run_id: None,
        scope_hash: teamspace_domain::idempotency::PayloadHash::from_bytes([1; 32]),
    };
    let grant = ApprovalGrant {
        requested_by: context.actor,
        decided_by: Actor::System,
        owner_id: HumanId::from_uuid(id),
        context,
        consumed: false,
        expires_at_ms: 1000,
    };
    assert!(approval_grant_is_usable(grant, context, 0).is_some());
}

#[test]
fn review_mismatched_lease_generation_denies() {
    let lease = LeaseSnapshot {
        fencing_token: 7,
        acquired_at_ms: 0,
        expires_at_ms: 1000,
    };
    assert!(matches!(
        decide_renewal(lease, 8, 8, 100, 100),
        RenewalDecision::Refused { .. }
    ));
}

#[test]
fn review_renewal_never_shortens_the_existing_window() {
    let lease = LeaseSnapshot {
        fencing_token: 7,
        acquired_at_ms: 0,
        expires_at_ms: 1000,
    };
    assert_eq!(
        decide_renewal(lease, 7, 7, 100, 100),
        RenewalDecision::Renewed {
            expires_at_ms: 1000
        }
    );
}

#[test]
fn review_exhausted_retry_budget_stops() {
    let row = OutboxEntry {
        id: [0; 16],
        status: OutboxStatus::Processing,
        retry_count: 0,
        max_retries: 3,
        scheduled_at: 0,
    };
    let policy = RetryPolicy {
        max_retries: 3,
        backoff_base_millis: 200,
        total_budget_millis: 100,
        jitter_millis: 0,
    };
    assert!(matches!(
        decide_outcome(
            &row,
            DeliveryFailure::Retryable {
                code: "STORAGE_UNAVAILABLE"
            },
            policy,
            101,
            101
        ),
        OutcomeDecision::DeadLetter { .. }
    ));
}

#[test]
fn review_time_advancing_expires_an_unchanged_cache() {
    let context = CacheContext {
        object_version: 1,
        binding_version: 7,
        approval_expiry_tick: Some(100),
        fencing_token: 8,
    };
    assert!(context.may_reuse(context, 99));
    assert!(!context.may_reuse(context, 100));
    assert!(!context.may_reuse(context, 101));
}

#[test]
fn review_absolute_deadline_includes_delivery_and_queue_time() {
    use teamspace_domain::outbox_dispatch::{
        DispatchDecision, decide_dispatch_before_deadline, decide_outcome_before_deadline,
    };
    let mut row = OutboxEntry {
        id: [0; 16],
        status: OutboxStatus::Processing,
        retry_count: 0,
        max_retries: 3,
        scheduled_at: 0,
    };
    let policy = RetryPolicy {
        max_retries: 3,
        backoff_base_millis: 10,
        total_budget_millis: 1000,
        jitter_millis: 0,
    };
    for now in [90, 100, 101] {
        assert!(matches!(
            decide_outcome_before_deadline(
                &row,
                DeliveryFailure::Retryable {
                    code: "STORAGE_UNAVAILABLE"
                },
                policy,
                now,
                0,
                100
            ),
            OutcomeDecision::DeadLetter { .. }
        ));
    }
    assert!(matches!(
        decide_outcome_before_deadline(
            &row,
            DeliveryFailure::Retryable {
                code: "STORAGE_UNAVAILABLE"
            },
            policy,
            89,
            0,
            100
        ),
        OutcomeDecision::Retry {
            scheduled_at: 99,
            ..
        }
    ));
    row.status = OutboxStatus::Pending;
    assert!(matches!(
        decide_dispatch_before_deadline(&row, 99, 100),
        DispatchDecision::Claim { .. }
    ));
    assert!(matches!(
        decide_dispatch_before_deadline(&row, 100, 100),
        DispatchDecision::NotClaimable { .. }
    ));
}

#[test]
fn review_base_and_role_permission_layers_cannot_be_skipped() {
    use teamspace_domain::{DenialReason, Layer, PermissionDecision};
    for layer in [
        Layer::TeamPolicy,
        Layer::ActorBasePermission,
        Layer::RolePolicy,
    ] {
        let mut terms = [LayerVerdict::Allow; 8];
        terms[layer.index()] = LayerVerdict::NotApplicable;
        let r = PermissionRequest {
            actor: ActorKind::RoleDeveloper,
            action: PermissionAction::TaskCreate,
            terms,
            facts: CompoundFacts::default(),
        };
        assert_eq!(
            evaluate_permission(r),
            PermissionDecision::Deny {
                layer: Some(layer),
                reason: DenialReason::DefaultDeny
            }
        );
    }
}
