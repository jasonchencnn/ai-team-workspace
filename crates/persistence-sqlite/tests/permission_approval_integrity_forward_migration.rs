//! DEV-02-10: forward migration `0020` closes the Team, state-machine and
//! immutability gaps that a four-axis counterexample review reproduced against
//! `0009`.
//!
//! `0009` defines two tables (137 lines, 3 indexes, 29 CHECKs) but installs no
//! trigger.  Its own table-level CHECKs are meaningful — no self-approval, a
//! human decider is required, a rejection must carry notes — but there is no
//! trigger-level Team consistency, state machine, creation entry, immutability
//! or replace/delete guard.
//!
//! Every status set asserted here is transcribed from the frozen
//! `docs/specs/STATE_MACHINES.md` section 7 transition table, including its
//! explicit rule that all four terminal states are final: "全部终态不可重开；
//! 新请求必须新建 Approval".
//!
//! `permissions` deliberately gets **no** state machine: the table has no
//! `status` column and the frozen specification defines no Permission state.

use std::{borrow::Cow, error::Error, future::Future, pin::Pin, str::FromStr};

use persistence_sqlite::migrate;
use sqlx_core::{
    error::BoxDynError,
    migrate::{Migration, MigrationSource, MigrationType, Migrator},
    query::query,
    query_scalar::query_scalar,
};
use sqlx_sqlite::{SqliteConnectOptions, SqlitePool, SqlitePoolOptions};

const V21: [(&str, &str); 21] = [
    (
        "team role workspace",
        include_str!("../migrations/0001_team_role_workspace.sql"),
    ),
    (
        "harness adapter binding",
        include_str!("../migrations/0002_harness_adapter_binding.sql"),
    ),
    (
        "task dependency",
        include_str!("../migrations/0003_task_dependency.sql"),
    ),
    (
        "run lease review",
        include_str!("../migrations/0004_run_lease_review.sql"),
    ),
    (
        "artifact reference",
        include_str!("../migrations/0005_artifact_reference.sql"),
    ),
    (
        "event outbox",
        include_str!("../migrations/0006_event_outbox.sql"),
    ),
    (
        "thread message delivery",
        include_str!("../migrations/0007_thread_message_delivery.sql"),
    ),
    (
        "notification attention",
        include_str!("../migrations/0008_notification_attention.sql"),
    ),
    (
        "permission approval",
        include_str!("../migrations/0009_permission_approval.sql"),
    ),
    (
        "binding team consistency",
        include_str!("../migrations/0010_binding_team_consistency.sql"),
    ),
    (
        "task dependency consistency",
        include_str!("../migrations/0011_task_dependency_consistency.sql"),
    ),
    (
        "run lease consistency",
        include_str!("../migrations/0012_run_lease_consistency.sql"),
    ),
    (
        "artifact reference consistency",
        include_str!("../migrations/0013_artifact_reference_consistency.sql"),
    ),
    (
        "event outbox consistency",
        include_str!("../migrations/0014_event_outbox_consistency.sql"),
    ),
    (
        "event actor outbox integrity",
        include_str!("../migrations/0015_event_actor_outbox_integrity.sql"),
    ),
    (
        "event dedup state immutability",
        include_str!("../migrations/0016_event_dedup_state_immutability.sql"),
    ),
    (
        "message delivery integrity",
        include_str!("../migrations/0017_message_delivery_integrity.sql"),
    ),
    (
        "message delivery insert state guard",
        include_str!("../migrations/0018_message_delivery_insert_state_guard.sql"),
    ),
    (
        "notification attention integrity",
        include_str!("../migrations/0019_notification_attention_integrity.sql"),
    ),
    (
        "permission approval integrity",
        include_str!("../migrations/0020_permission_approval_integrity.sql"),
    ),
    (
        "approval policy linkage",
        include_str!("../migrations/0021_approval_policy_linkage.sql"),
    ),
];

/// Stops at `0020` so the regression test can prove the pre-`0021` schema
/// really accepted a forged policy link and a forged cancellation actor.
#[derive(Debug)]
struct V20OnlyMigrations;

impl MigrationSource<'static> for V20OnlyMigrations {
    fn resolve(
        self,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<Migration>, BoxDynError>> + Send + 'static>> {
        Box::pin(async {
            Ok(V21
                .iter()
                .take(20)
                .enumerate()
                .map(|(index, (description, sql))| {
                    Migration::new(
                        (index + 1) as i64,
                        Cow::Borrowed(*description),
                        MigrationType::Simple,
                        Cow::Borrowed(*sql),
                        false,
                    )
                })
                .collect())
        })
    }
}

/// Stops at `0019` so the regression test can prove the pre-`0020` schema
/// really accepted the forged rows.
#[derive(Debug)]
struct V19OnlyMigrations;

impl MigrationSource<'static> for V19OnlyMigrations {
    fn resolve(
        self,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<Migration>, BoxDynError>> + Send + 'static>> {
        Box::pin(async {
            Ok(V21
                .iter()
                .take(19)
                .enumerate()
                .map(|(index, (description, sql))| {
                    Migration::new(
                        (index + 1) as i64,
                        Cow::Borrowed(*description),
                        MigrationType::Simple,
                        Cow::Borrowed(*sql),
                        false,
                    )
                })
                .collect())
        })
    }
}

async fn database() -> Result<SqlitePool, Box<dyn Error>> {
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(SqliteConnectOptions::from_str("sqlite::memory:")?.foreign_keys(true))
        .await?;
    migrate(&pool).await?;
    Ok(pool)
}

async fn v20_database() -> Result<SqlitePool, Box<dyn Error>> {
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(SqliteConnectOptions::from_str("sqlite::memory:")?.foreign_keys(true))
        .await?;
    Migrator::new(V20OnlyMigrations).await?.run(&pool).await?;
    Ok(pool)
}

async fn v19_database() -> Result<SqlitePool, Box<dyn Error>> {
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(SqliteConnectOptions::from_str("sqlite::memory:")?.foreign_keys(true))
        .await?;
    Migrator::new(V19OnlyMigrations).await?.run(&pool).await?;
    Ok(pool)
}

const TEAM_A: &str = "01921389d31174628f80cb5f79aa1901";
const TEAM_B: &str = "01921389d31174628f80cb5f79aa1a01";
const ROLE_A: &str = "01921389d31174628f80cb5f79aa1902";
const ROLE_B: &str = "01921389d31174628f80cb5f79aa1a02";
const HUMAN: &str = "01921389d31174628f80cb5f79aa1b90";
const HUMAN_B: &str = "01921389d31174628f80cb5f79aa1b96";
const APPROVAL: &str = "01921389d31174628f80cb5f79aa1b91";
const DECIDED: &str = "01921389d31174628f80cb5f79aa1b92";
const POLICY: &str = "01921389d31174628f80cb5f79aa1b93";
/// Stays PENDING after seeding so the "no premature decision trace" guard has a
/// real target; `APPROVAL` and `DECIDED` both leave PENDING inside `seed`.
const PENDING: &str = "01921389d31174628f80cb5f79aa1b94";
/// A `CANCELLED` approval that already carries a real cancellation actor.
/// Attacking this row is only meaningful: without a value to erase or swap,
/// "set the canceller to NULL" is a no-op and would be scored as a bypass
/// for the wrong reason (the J12 trap).
const CANCELLED_BY_ROLE: &str = "01921389d31174628f80cb5f79aa1b95";
/// A Team B policy — the cross-Team target for `permission_id` and for a
/// ROLE cancellation actor.
const POLICY_B: &str = "01921389d31174628f80cb5f79aa1b96";
/// A same-Team `DENY` policy. `PERMISSION_MATRIX` §2 forbids an approval from
/// overriding a Team deny, so pointing at one must be rejected.
const POLICY_DENY: &str = "01921389d31174628f80cb5f79aa1b97";

async fn seed(pool: &SqlitePool) -> Result<(), Box<dyn Error>> {
    for statement in [
        format!("INSERT INTO teams VALUES(X'{TEAM_A}','team-a','ACTIVE',1,1,1,NULL)"),
        format!("INSERT INTO teams VALUES(X'{TEAM_B}','team-b','ACTIVE',1,1,1,NULL)"),
        format!(
            "INSERT INTO roles VALUES(X'{ROLE_A}',X'{TEAM_A}','role-a','role','ACTIVE',1,1,1,NULL)"
        ),
        format!(
            "INSERT INTO roles VALUES(X'{ROLE_B}',X'{TEAM_B}','role-b','role','ACTIVE',1,1,1,NULL)"
        ),
        format!(
            "INSERT INTO permissions VALUES(X'{POLICY}',X'{TEAM_A}',NULL,'WORKSPACE','READ','docs/*','ALLOW',0,1,1,1)"
        ),
        format!(
            "INSERT INTO permissions VALUES(X'{POLICY_B}',X'{TEAM_B}',NULL,'CREDENTIAL','READ','vault/*','ALLOW',0,1,1,1)"
        ),
        // A same-Team DENY policy. It has to be same-Team on purpose: if it
        // were cross-Team, the "another Team" guard would fire first and the
        // test could not tell which rule is actually doing the rejecting.
        format!(
            "INSERT INTO permissions VALUES(X'{POLICY_DENY}',X'{TEAM_A}',NULL,'CREDENTIAL','UPDATE','vault/prod','DENY',0,1,1,1)"
        ),
        format!(
            "INSERT INTO approvals (id,team_id,requester_actor_type,requester_role_id,requester_human_id,action,resource_type,resource_scope,reason,status,decided_by_human_id,decision_notes,version,expires_at,created_at,decided_at) VALUES(X'{APPROVAL}',X'{TEAM_A}','ROLE',X'{ROLE_A}',NULL,'EXECUTE','CREDENTIAL_ACCESS','vault/prod','needs credential','PENDING',NULL,NULL,1,9000,1,NULL)"
        ),
        format!(
            "INSERT INTO approvals (id,team_id,requester_actor_type,requester_role_id,requester_human_id,action,resource_type,resource_scope,reason,status,decided_by_human_id,decision_notes,version,expires_at,created_at,decided_at) VALUES(X'{PENDING}',X'{TEAM_A}','HUMAN',NULL,X'{HUMAN}','EXECUTE','CREDENTIAL_ACCESS','vault/prod','needs credential','PENDING',NULL,NULL,1,9000,1,NULL)"
        ),
    ] {
        query(&statement).execute(pool).await?;
    }
    // The decided approval must be reached through the legal path, never born
    // in a terminal state — `0020` forbids the latter.
    query(&format!(
        "UPDATE approvals SET status='APPROVED', decided_by_human_id=X'{HUMAN}', decision_notes='ok', decided_at=2, version=2 WHERE id=X'{APPROVAL}'"
    ))
    .execute(pool)
    .await?;
    query(&format!(
        "INSERT INTO approvals (id,team_id,requester_actor_type,requester_role_id,requester_human_id,action,resource_type,resource_scope,reason,status,decided_by_human_id,decision_notes,version,expires_at,created_at,decided_at) VALUES(X'{DECIDED}',X'{TEAM_A}','HUMAN',NULL,X'{HUMAN_B}','EXECUTE','TEAM_ARCHIVE','team','archive','PENDING',NULL,NULL,1,9000,1,NULL)"
    ))
    .execute(pool)
    .await?;
    query(&format!(
        "UPDATE approvals SET status='APPROVED', decided_by_human_id=X'{HUMAN}', decision_notes='ok', decided_at=3 WHERE id=X'{DECIDED}'"
    ))
    .execute(pool)
    .await?;
    Ok(())
}

fn approval_sql(id: &str, status: &str) -> String {
    format!(
        "INSERT INTO approvals (id,team_id,requester_actor_type,requester_role_id,requester_human_id,action,resource_type,resource_scope,reason,status,decided_by_human_id,decision_notes,version,expires_at,created_at,decided_at) VALUES(X'{id}',X'{TEAM_A}','ROLE',X'{ROLE_A}',NULL,'EXECUTE','CREDENTIAL_ACCESS','s','r','{status}',X'{HUMAN_B}','ok',1,9000,1,1)"
    )
}

#[tokio::test]
async fn an_approval_can_only_be_created_as_pending() -> Result<(), Box<dyn Error>> {
    let pool = database().await?;
    seed(&pool).await?;

    for (index, status) in ["APPROVED", "REJECTED", "EXPIRED", "CANCELLED"]
        .iter()
        .enumerate()
    {
        let id = format!("01921389d31174628f80cb5f79aa1c{index:02}");
        let sql = approval_sql(&id, status);
        assert!(
            query(&sql).execute(&pool).await.is_err(),
            "approval created directly as {status} unexpectedly succeeded"
        );
    }
    Ok(())
}

#[tokio::test]
async fn the_requester_identity_must_match_its_actor_type() -> Result<(), Box<dyn Error>> {
    let pool = database().await?;
    seed(&pool).await?;

    // ROLE requester must carry a Role, must not carry a Human id, and that
    // Role must belong to the same Team.
    for (name, sql) in [
        (
            "ROLE requester without a Role",
            format!(
                "INSERT INTO approvals (id,team_id,requester_actor_type,requester_role_id,requester_human_id,action,resource_type,resource_scope,reason,status,decided_by_human_id,decision_notes,version,expires_at,created_at,decided_at) VALUES(X'01921389d31174628f80cb5f79aa1d01',X'{TEAM_A}','ROLE',NULL,NULL,'EXECUTE','CREDENTIAL_ACCESS','s','r','PENDING',NULL,NULL,1,9000,1,NULL)"
            ),
        ),
        (
            "ROLE requester carrying a Human id",
            format!(
                "INSERT INTO approvals (id,team_id,requester_actor_type,requester_role_id,requester_human_id,action,resource_type,resource_scope,reason,status,decided_by_human_id,decision_notes,version,expires_at,created_at,decided_at) VALUES(X'01921389d31174628f80cb5f79aa1d02',X'{TEAM_A}','ROLE',X'{ROLE_A}',X'{HUMAN}','EXECUTE','CREDENTIAL_ACCESS','s','r','PENDING',NULL,NULL,1,9000,1,NULL)"
            ),
        ),
        (
            "HUMAN requester carrying a Role",
            format!(
                "INSERT INTO approvals (id,team_id,requester_actor_type,requester_role_id,requester_human_id,action,resource_type,resource_scope,reason,status,decided_by_human_id,decision_notes,version,expires_at,created_at,decided_at) VALUES(X'01921389d31174628f80cb5f79aa1d03',X'{TEAM_A}','HUMAN',X'{ROLE_A}',X'{HUMAN}','EXECUTE','CREDENTIAL_ACCESS','s','r','PENDING',NULL,NULL,1,9000,1,NULL)"
            ),
        ),
        (
            "ROLE requester from another Team",
            format!(
                "INSERT INTO approvals (id,team_id,requester_actor_type,requester_role_id,requester_human_id,action,resource_type,resource_scope,reason,status,decided_by_human_id,decision_notes,version,expires_at,created_at,decided_at) VALUES(X'01921389d31174628f80cb5f79aa1d04',X'{TEAM_A}','ROLE',X'{ROLE_B}',NULL,'EXECUTE','CREDENTIAL_ACCESS','s','r','PENDING',NULL,NULL,1,9000,1,NULL)"
            ),
        ),
    ] {
        assert!(
            query(&sql).execute(&pool).await.is_err(),
            "{name} unexpectedly succeeded"
        );
    }
    Ok(())
}

#[tokio::test]
async fn a_terminal_approval_is_never_reopened_or_redecided() -> Result<(), Box<dyn Error>> {
    let pool = database().await?;
    seed(&pool).await?;

    for to in ["PENDING", "REJECTED", "CANCELLED", "APPROVED"] {
        assert!(
            query(&format!(
                "UPDATE approvals SET status='{to}' WHERE id=X'{APPROVAL}'"
            ))
            .execute(&pool)
            .await
            .is_err(),
            "APPROVED -> {to} unexpectedly succeeded"
        );
    }
    Ok(())
}

#[tokio::test]
async fn a_decided_approval_cannot_be_rewritten() -> Result<(), Box<dyn Error>> {
    let pool = database().await?;
    seed(&pool).await?;

    for (name, sql) in [
        (
            "rewrite the reason",
            format!("UPDATE approvals SET reason='forged' WHERE id=X'{APPROVAL}'"),
        ),
        (
            "widen the resource scope",
            format!("UPDATE approvals SET resource_scope='*' WHERE id=X'{APPROVAL}'"),
        ),
        (
            "extend the expiry",
            format!("UPDATE approvals SET expires_at=999999 WHERE id=X'{APPROVAL}'"),
        ),
        (
            "clear the decider",
            format!("UPDATE approvals SET decided_by_human_id=NULL WHERE id=X'{APPROVAL}'"),
        ),
        (
            "clear the decision time",
            format!("UPDATE approvals SET decided_at=NULL WHERE id=X'{APPROVAL}'"),
        ),
        (
            "move to another Team",
            format!("UPDATE approvals SET team_id=X'{TEAM_B}' WHERE id=X'{APPROVAL}'"),
        ),
        (
            "roll the version back",
            format!("UPDATE approvals SET version=1 WHERE id=X'{APPROVAL}'"),
        ),
        (
            "erase the requester identity",
            format!("UPDATE approvals SET requester_human_id=NULL WHERE id=X'{DECIDED}'"),
        ),
        // T-01 (second pass): swapping the recorded decider for another Human
        // is not "clearing" the trace, so `0020` never blocked it. The decision
        // was already made; swapping who made it forges the audit record.
        (
            "swap the decider for another Human",
            format!("UPDATE approvals SET decided_by_human_id=X'{HUMAN_B}' WHERE id=X'{APPROVAL}'"),
        ),
        // T-02 (second pass): same forgery, aimed at the decision timestamp.
        (
            "backdate the decision time",
            format!("UPDATE approvals SET decided_at=1 WHERE id=X'{APPROVAL}'"),
        ),
    ] {
        assert!(
            query(&sql).execute(&pool).await.is_err(),
            "{name} unexpectedly succeeded"
        );
    }
    Ok(())
}

#[tokio::test]
async fn a_pending_approval_may_not_already_carry_a_decision() -> Result<(), Box<dyn Error>> {
    let pool = database().await?;
    seed(&pool).await?;

    for (name, sql) in [
        (
            "a decider on a pending approval",
            format!("UPDATE approvals SET decided_by_human_id=X'{HUMAN_B}' WHERE id=X'{PENDING}'"),
        ),
        (
            "a decision time on a pending approval",
            format!("UPDATE approvals SET decided_at=5 WHERE id=X'{PENDING}'"),
        ),
    ] {
        assert!(
            query(&sql).execute(&pool).await.is_err(),
            "{name} unexpectedly succeeded"
        );
    }
    Ok(())
}

#[tokio::test]
async fn a_permission_policy_stays_inside_its_team() -> Result<(), Box<dyn Error>> {
    let pool = database().await?;
    seed(&pool).await?;

    for (name, sql) in [
        (
            "Role from another Team",
            format!(
                "INSERT INTO permissions VALUES(X'01921389d31174628f80cb5f79aa1e01',X'{TEAM_A}',X'{ROLE_B}','TASK','READ','*','ALLOW',0,1,1,1)"
            ),
        ),
        (
            "policy moved to another Team",
            format!("UPDATE permissions SET team_id=X'{TEAM_B}' WHERE id=X'{POLICY}'"),
        ),
        (
            "role_id repointed across Teams",
            format!("UPDATE permissions SET role_id=X'{ROLE_B}' WHERE id=X'{POLICY}'"),
        ),
        (
            "version rolled back",
            format!("UPDATE permissions SET version=0 WHERE id=X'{POLICY}'"),
        ),
        (
            "scope widened without a version bump",
            format!("UPDATE permissions SET scope_pattern='*' WHERE id=X'{POLICY}'"),
        ),
        (
            "effect flipped without a version bump",
            format!("UPDATE permissions SET effect='DENY' WHERE id=X'{POLICY}'"),
        ),
        (
            "policy deleted",
            format!("DELETE FROM permissions WHERE id=X'{POLICY}'"),
        ),
    ] {
        assert!(
            query(&sql).execute(&pool).await.is_err(),
            "{name} unexpectedly succeeded"
        );
    }
    Ok(())
}

/// Editing a policy is legitimate — `PERMISSION_MATRIX.md` grants that to the
/// Leader.  What is not legitimate is editing it without leaving a version
/// trace, so a proper edit must bump `version`.
#[tokio::test]
async fn a_policy_edit_is_allowed_when_it_bumps_the_version() -> Result<(), Box<dyn Error>> {
    let pool = database().await?;
    seed(&pool).await?;

    query(&format!(
        "UPDATE permissions SET scope_pattern='docs/limited/*', effect='DENY', version=2 WHERE id=X'{POLICY}'"
    ))
    .execute(&pool)
    .await?;

    let sql = format!("SELECT version FROM permissions WHERE id=X'{POLICY}'");
    assert_eq!(
        query_scalar::<_, i64>(&sql).fetch_one(&pool).await?,
        2,
        "the legitimate policy edit did not take effect"
    );
    Ok(())
}

#[tokio::test]
async fn replace_and_delete_cannot_destroy_audit_records() -> Result<(), Box<dyn Error>> {
    let pool = database().await?;
    seed(&pool).await?;

    let approvals_before = query_scalar::<_, i64>("SELECT COUNT(*) FROM approvals")
        .fetch_one(&pool)
        .await?;
    let pending_before =
        query_scalar::<_, i64>("SELECT COUNT(*) FROM approvals WHERE status='PENDING'")
            .fetch_one(&pool)
            .await?;
    assert!(
        pending_before > 0,
        "the seed carried no pending approval, so this proves nothing"
    );

    for (name, sql) in [
        (
            "replace a decided approval",
            format!(
                "INSERT OR REPLACE INTO approvals VALUES(X'{APPROVAL}',X'{TEAM_A}','ROLE',X'{ROLE_A}',NULL,'EXECUTE','TASK_MUTATION','task-9','overwritten','PENDING',NULL,NULL,1,9000,2,NULL)"
            ),
        ),
        (
            "replace a permission policy",
            format!(
                "INSERT OR REPLACE INTO permissions VALUES(X'{POLICY}',X'{TEAM_A}',NULL,'WORKSPACE','READ','*','ALLOW',0,99,2,2)"
            ),
        ),
        (
            "delete a decided approval",
            format!("DELETE FROM approvals WHERE id=X'{APPROVAL}'"),
        ),
        (
            "delete a still-pending approval",
            "DELETE FROM approvals WHERE status='PENDING'".to_string(),
        ),
    ] {
        assert!(
            query(&sql).execute(&pool).await.is_err(),
            "{name} unexpectedly succeeded"
        );
    }

    // Corrected by MM-R02 (deep audit DA-02). This used to assert that a
    // `PENDING` approval *may* be removed by a direct `DELETE`, which
    // contradicts DATA_CONVENTIONS section 6 -- an Approval is "随 Team 保留"
    // in every state, not only once decided -- and section 5's "P0 核心对象无
    // 通用 DELETE 命令".
    //
    // What replaces it is not merely a flipped polarity. `0020`'s own
    // `approvals_terminal_delete_guard` covers `OLD.status != 'PENDING'`, so on
    // this schema the new refusal comes from `0026`
    // `approvals_core_history_retention_delete`, which is unconditional. Saying
    // so keeps the distinction visible: the old assertion was passing because
    // `0020` deliberately let this one state through.
    //
    // A pending request is withdrawn through the state machine, not by removing
    // it: STATE_MACHINES section 7 gives `PENDING -> CANCELLED`, and the row
    // keeps carrying who cancelled it.
    assert_eq!(
        query_scalar::<_, i64>("SELECT COUNT(*) FROM approvals")
            .fetch_one(&pool)
            .await?,
        approvals_before,
        "a refused DELETE changed the approval count"
    );
    assert_eq!(
        query_scalar::<_, i64>("SELECT COUNT(*) FROM approvals WHERE status='PENDING'")
            .fetch_one(&pool)
            .await?,
        pending_before,
        "a refused DELETE removed a still-pending request"
    );
    Ok(())
}

/// Pins the regression itself: on a `v19` schema the forged rows are accepted.
#[tokio::test]
async fn v19_really_did_accept_terminal_state_at_creation() -> Result<(), Box<dyn Error>> {
    let pool = v19_database().await?;
    seed(&pool).await?;

    assert!(
        query(&format!(
            "INSERT INTO approvals (id,team_id,requester_actor_type,requester_role_id,requester_human_id,action,resource_type,resource_scope,reason,status,decided_by_human_id,decision_notes,version,expires_at,created_at,decided_at) VALUES(X'01921389d31174628f80cb5f79aa1f01',X'{TEAM_A}','ROLE',X'{ROLE_A}',NULL,'EXECUTE','CREDENTIAL_ACCESS','s','r','APPROVED',X'{HUMAN}','ok',1,9000,1,1)"
        ))
        .execute(&pool)
        .await
        .is_ok(),
        "v19 unexpectedly rejected the forged approval, so this regression test proves nothing"
    );
    assert!(
        query(&format!("DELETE FROM approvals WHERE id=X'{APPROVAL}'"))
            .execute(&pool)
            .await
            .is_ok(),
        "v19 unexpectedly rejected the delete, so this regression test proves nothing"
    );
    Ok(())
}

/// A `CANCELLED` approval that already carries a real cancellation actor.
///
/// Kept out of [`seed`] because the columns it writes only exist from `0021`
/// on, and the `v19` regression test still has to run against the old schema.
async fn seed_cancellation_actor(pool: &SqlitePool) -> Result<(), Box<dyn Error>> {
    query(&format!(
        "INSERT INTO approvals (id,team_id,requester_actor_type,requester_role_id,requester_human_id,action,resource_type,resource_scope,reason,status,decided_by_human_id,decision_notes,version,expires_at,created_at,decided_at) VALUES(X'{CANCELLED_BY_ROLE}',X'{TEAM_A}','ROLE',X'{ROLE_A}',NULL,'EXECUTE','TASK_MUTATION','task-cxl2','withdrawn','PENDING',NULL,NULL,1,9000,1,NULL)"
    ))
    .execute(pool)
    .await?;
    query(&format!(
        "UPDATE approvals SET status='CANCELLED', cancelled_by_actor_type='ROLE', cancelled_by_actor_id=X'{ROLE_A}', version=2 WHERE id=X'{CANCELLED_BY_ROLE}'"
    ))
    .execute(pool)
    .await?;
    Ok(())
}

/// Columns added by `0021`, applied by hand so the pre-`0021` schema can be
/// exercised with the columns *present but unguarded* — exactly the state the
/// bash harness calls the "shadow" build. Without this, every statement below
/// would fail with "no such column" and the regression test would prove nothing
/// about the guards.
async fn shadow_0021_columns(pool: &SqlitePool) -> Result<(), Box<dyn Error>> {
    for statement in [
        "ALTER TABLE approvals ADD COLUMN permission_id BLOB",
        "ALTER TABLE approvals ADD COLUMN cancelled_by_actor_type TEXT",
        "ALTER TABLE approvals ADD COLUMN cancelled_by_actor_id BLOB",
    ] {
        query(statement).execute(pool).await?;
    }
    Ok(())
}

#[tokio::test]
async fn v20_really_did_accept_a_forged_link_and_cancellation_actor() -> Result<(), Box<dyn Error>>
{
    let pool = v20_database().await?;
    shadow_0021_columns(&pool).await?;
    seed(&pool).await?;
    seed_cancellation_actor(&pool).await?;

    assert!(
        query(&format!(
            "UPDATE approvals SET permission_id=X'{POLICY_B}' WHERE id=X'{PENDING}'"
        ))
        .execute(&pool)
        .await
        .is_ok(),
        "pre-0021 unexpectedly rejected the cross-Team policy link, so this regression test proves nothing"
    );
    assert!(
        query(&format!(
            "UPDATE approvals SET cancelled_by_actor_type='ROLE', cancelled_by_actor_id=X'{ROLE_B}' WHERE id=X'{CANCELLED_BY_ROLE}'"
        ))
        .execute(&pool)
        .await
        .is_ok(),
        "pre-0021 unexpectedly rejected the forged cancellation actor, so this regression test proves nothing"
    );
    Ok(())
}

#[tokio::test]
async fn an_approval_may_only_link_a_policy_in_its_own_team() -> Result<(), Box<dyn Error>> {
    let pool = database().await?;
    seed(&pool).await?;
    seed_cancellation_actor(&pool).await?;

    for (name, sql) in [
        (
            "a policy that does not exist",
            format!(
                "UPDATE approvals SET permission_id=X'01921389d31174628f80cb5f79aa1fff' WHERE id=X'{PENDING}'"
            ),
        ),
        (
            "a policy from another Team",
            format!("UPDATE approvals SET permission_id=X'{POLICY_B}' WHERE id=X'{PENDING}'"),
        ),
        (
            "a decided approval repointed at another Team's policy",
            format!("UPDATE approvals SET permission_id=X'{POLICY_B}' WHERE id=X'{APPROVAL}'"),
        ),
    ] {
        assert!(
            query(&sql).execute(&pool).await.is_err(),
            "{name} unexpectedly succeeded"
        );
    }
    Ok(())
}

#[tokio::test]
async fn a_policy_link_cannot_be_swapped_once_it_is_set() -> Result<(), Box<dyn Error>> {
    let pool = database().await?;
    seed(&pool).await?;
    query(&format!(
        "UPDATE approvals SET permission_id=X'{POLICY}' WHERE id=X'{PENDING}'"
    ))
    .execute(&pool)
    .await?;

    // Same Team this time — the swap itself is the violation, not the Team.
    let other = "01921389d31174628f80cb5f79aa1e02";
    query(&format!(
        "INSERT INTO permissions VALUES(X'{other}',X'{TEAM_A}',NULL,'TASK','UPDATE','task/*','DENY',0,1,1,1)"
    ))
    .execute(&pool)
    .await?;
    assert!(
        query(&format!(
            "UPDATE approvals SET permission_id=X'{other}' WHERE id=X'{PENDING}'"
        ))
        .execute(&pool)
        .await
        .is_err(),
        "swapping an established policy link unexpectedly succeeded"
    );
    Ok(())
}

#[tokio::test]
async fn a_cancellation_actor_only_exists_on_a_cancelled_approval() -> Result<(), Box<dyn Error>> {
    let pool = database().await?;
    seed(&pool).await?;
    seed_cancellation_actor(&pool).await?;

    for (name, sql) in [
        (
            "a pending approval claiming it was cancelled by a Role",
            format!(
                "UPDATE approvals SET cancelled_by_actor_type='ROLE', cancelled_by_actor_id=X'{ROLE_A}' WHERE id=X'{PENDING}'"
            ),
        ),
        (
            "an approved approval claiming it was cancelled",
            format!(
                "UPDATE approvals SET cancelled_by_actor_type='HUMAN', cancelled_by_actor_id=X'{HUMAN}' WHERE id=X'{APPROVAL}'"
            ),
        ),
        (
            "a Role canceller pointing at another Team's Role",
            format!(
                "UPDATE approvals SET cancelled_by_actor_type='ROLE', cancelled_by_actor_id=X'{ROLE_B}' WHERE id=X'{CANCELLED_BY_ROLE}'"
            ),
        ),
        (
            "a type without an id",
            format!(
                "UPDATE approvals SET cancelled_by_actor_type='HUMAN', cancelled_by_actor_id=NULL WHERE id=X'{CANCELLED_BY_ROLE}'"
            ),
        ),
    ] {
        assert!(
            query(&sql).execute(&pool).await.is_err(),
            "{name} unexpectedly succeeded"
        );
    }
    Ok(())
}

#[tokio::test]
async fn a_cancellation_actor_cannot_be_erased_or_swapped() -> Result<(), Box<dyn Error>> {
    let pool = database().await?;
    seed(&pool).await?;
    seed_cancellation_actor(&pool).await?;

    assert!(
        query(&format!(
            "UPDATE approvals SET cancelled_by_actor_type=NULL, cancelled_by_actor_id=NULL WHERE id=X'{CANCELLED_BY_ROLE}'"
        ))
        .execute(&pool)
        .await
        .is_err(),
        "erasing the recorded canceller unexpectedly succeeded"
    );
    assert!(
        query(&format!(
            "UPDATE approvals SET cancelled_by_actor_id=X'{HUMAN}' WHERE id=X'{CANCELLED_BY_ROLE}'"
        ))
        .execute(&pool)
        .await
        .is_err(),
        "swapping the recorded canceller unexpectedly succeeded"
    );
    Ok(())
}

#[tokio::test]
async fn the_legal_cancellation_and_link_paths_stay_open() -> Result<(), Box<dyn Error>> {
    let pool = database().await?;
    seed(&pool).await?;

    for sql in [
        // Link to a same-Team policy, both at creation and afterwards.
        format!(
            "INSERT INTO permissions VALUES(X'01921389d31174628f80cb5f79aa1e03',X'{TEAM_A}',NULL,'TASK','READ','task/*','ALLOW',0,1,1,1)"
        ),
        format!(
            "INSERT INTO approvals (id,team_id,requester_actor_type,requester_role_id,requester_human_id,action,resource_type,resource_scope,reason,status,decided_by_human_id,decision_notes,version,expires_at,created_at,decided_at) VALUES(X'01921389d31174628f80cb5f79aa1b97',X'{TEAM_A}','ROLE',X'{ROLE_A}',NULL,'EXECUTE','CREDENTIAL_ACCESS','s','r','PENDING',NULL,NULL,1,9000,1,NULL)"
        ),
        "UPDATE approvals SET permission_id=X'01921389d31174628f80cb5f79aa1e03' WHERE id=X'01921389d31174628f80cb5f79aa1b97'".to_string(),
        // Cancel with a Role actor, with a Human actor, and with none at all —
        // `STATE_MACHINES` §7 does not make the actor mandatory.
        format!(
            "INSERT INTO approvals (id,team_id,requester_actor_type,requester_role_id,requester_human_id,action,resource_type,resource_scope,reason,status,decided_by_human_id,decision_notes,version,expires_at,created_at,decided_at) VALUES(X'01921389d31174628f80cb5f79aa1b98',X'{TEAM_A}','ROLE',X'{ROLE_A}',NULL,'EXECUTE','CREDENTIAL_ACCESS','s','r','PENDING',NULL,NULL,1,9000,1,NULL)"
        ),
        format!(
            "UPDATE approvals SET status='CANCELLED', cancelled_by_actor_type='ROLE', cancelled_by_actor_id=X'{ROLE_A}' WHERE id=X'01921389d31174628f80cb5f79aa1b98'"
        ),
        format!(
            "INSERT INTO approvals (id,team_id,requester_actor_type,requester_role_id,requester_human_id,action,resource_type,resource_scope,reason,status,decided_by_human_id,decision_notes,version,expires_at,created_at,decided_at) VALUES(X'01921389d31174628f80cb5f79aa1b99',X'{TEAM_A}','HUMAN',NULL,X'{HUMAN}','EXECUTE','CREDENTIAL_ACCESS','s','r','PENDING',NULL,NULL,1,9000,1,NULL)"
        ),
        format!(
            "UPDATE approvals SET status='CANCELLED', cancelled_by_actor_type='HUMAN', cancelled_by_actor_id=X'{HUMAN_B}' WHERE id=X'01921389d31174628f80cb5f79aa1b99'"
        ),
        format!(
            "INSERT INTO approvals (id,team_id,requester_actor_type,requester_role_id,requester_human_id,action,resource_type,resource_scope,reason,status,decided_by_human_id,decision_notes,version,expires_at,created_at,decided_at) VALUES(X'01921389d31174628f80cb5f79aa1b9a',X'{TEAM_A}','ROLE',X'{ROLE_A}',NULL,'EXECUTE','CREDENTIAL_ACCESS','s','r','PENDING',NULL,NULL,1,9000,1,NULL)"
        ),
        "UPDATE approvals SET status='CANCELLED' WHERE id=X'01921389d31174628f80cb5f79aa1b9a'".to_string(),
    ] {
        assert!(
            query(&sql).execute(&pool).await.is_ok(),
            "legal path was wrongly rejected: {sql}"
        );
    }
    Ok(())
}

#[tokio::test]
async fn an_approval_cannot_be_granted_against_a_denied_policy() -> Result<(), Box<dyn Error>> {
    let pool = database().await?;
    seed(&pool).await?;

    // `PERMISSION_MATRIX` §2: an approval may only narrow or temporarily
    // supplement actions the Team already allows; it must never breach the
    // Team ceiling. Pointing at an `ALLOW` row is the only legitimate case.
    for (name, sql) in [
        (
            "UPDATE path: link to a same-Team DENY policy",
            format!("UPDATE approvals SET permission_id=X'{POLICY_DENY}' WHERE id=X'{PENDING}'"),
        ),
        (
            "INSERT path: created already linked to a same-Team DENY policy",
            format!(
                "INSERT INTO approvals (id,team_id,requester_actor_type,requester_role_id,requester_human_id,action,resource_type,resource_scope,reason,status,version,expires_at,created_at,permission_id) VALUES(X'01921389d31174628f80cb5f79aa1ca1',X'{TEAM_A}','ROLE',X'{ROLE_A}',NULL,'EXECUTE','CREDENTIAL_ACCESS','s','r','PENDING',1,9000,1000,X'{POLICY_DENY}')"
            ),
        ),
    ] {
        assert!(
            query(&sql).execute(&pool).await.is_err(),
            "{name} unexpectedly succeeded"
        );
    }

    // The legitimate counterpart must stay open.
    assert!(
        query(&format!(
            "UPDATE approvals SET permission_id=X'{POLICY}' WHERE id=X'{PENDING}'"
        ))
        .execute(&pool)
        .await
        .is_ok(),
        "linking to a same-Team ALLOW policy was wrongly rejected"
    );
    Ok(())
}
