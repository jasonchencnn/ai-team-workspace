//! DEV-02-09: forward migration `0019` closes the Team, immutability and
//! state-machine gaps that a four-axis counterexample review reproduced
//! against `0008`.
//!
//! `0008` defines three tables (176 lines, 7 indexes) but installs no trigger
//! of its own.  `0014` added three guards for `notifications`, yet all three
//! are conditioned on the row being referenced by an Event, and
//! `attention_items` received nothing at all.
//!
//! Every status set asserted here is transcribed from the frozen
//! `docs/specs/STATE_MACHINES.md` section 6 transition table.  The tests pin
//! both halves of each guard: the illegal write is rejected, and the legal
//! transition a human recipient or the corresponding domain use case still
//! needs still succeeds.

use std::{borrow::Cow, error::Error, future::Future, pin::Pin, str::FromStr};

use persistence_sqlite::migrate;
use sqlx_core::{
    error::BoxDynError,
    migrate::{Migration, MigrationSource, MigrationType, Migrator},
    query::query,
    query_scalar::query_scalar,
};
use sqlx_sqlite::{SqliteConnectOptions, SqlitePool, SqlitePoolOptions};

const V18: [(&str, &str); 19] = [
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
];

/// Stops at `0018` so the regression test can prove the pre-`0019` schema
/// really accepted the forged rows.
#[derive(Debug)]
struct V18OnlyMigrations;

impl MigrationSource<'static> for V18OnlyMigrations {
    fn resolve(
        self,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<Migration>, BoxDynError>> + Send + 'static>> {
        Box::pin(async {
            Ok(V18
                .iter()
                .take(18)
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

async fn v18_database() -> Result<SqlitePool, Box<dyn Error>> {
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(SqliteConnectOptions::from_str("sqlite::memory:")?.foreign_keys(true))
        .await?;
    Migrator::new(V18OnlyMigrations).await?.run(&pool).await?;
    Ok(pool)
}

const TEAM_A: &str = "01921389d31174628f80cb5f79aa1901";
const ROLE_A: &str = "01921389d31174628f80cb5f79aa1902";
const HARNESS_A: &str = "01921389d31174628f80cb5f79aa1903";
const ADAPTER_A: &str = "01921389d31174628f80cb5f79aa1904";
const BINDING_A: &str = "01921389d31174628f80cb5f79aa1905";
const TEAM_B: &str = "01921389d31174628f80cb5f79aa1a01";
const HUMAN: &str = "01921389d31174628f80cb5f79aa1b90";
const POLICY_A: &str = "01921389d31174628f80cb5f79aa1b91";
const POLICY_B: &str = "01921389d31174628f80cb5f79aa1b92";
const TASK_A: &str = "01921389d31174628f80cb5f79aa1b93";
const NOTIFICATION: &str = "01921389d31174628f80cb5f79aa1b94";
const ATTENTION: &str = "01921389d31174628f80cb5f79aa1b95";

async fn seed(pool: &SqlitePool) -> Result<(), Box<dyn Error>> {
    for statement in [
        format!("INSERT INTO teams VALUES(X'{TEAM_A}','team-a','ACTIVE',1,1,1,NULL)"),
        format!("INSERT INTO teams VALUES(X'{TEAM_B}','team-b','ACTIVE',1,1,1,NULL)"),
        format!(
            "INSERT INTO roles VALUES(X'{ROLE_A}',X'{TEAM_A}','role-a','role','ACTIVE',1,1,1,NULL)"
        ),
        format!(
            "INSERT INTO harness_instances VALUES(X'{HARNESS_A}',X'{TEAM_A}','mock','harness','ACTIVE','[]',NULL,1,1,1,NULL)"
        ),
        format!(
            "INSERT INTO adapters VALUES(X'{ADAPTER_A}',X'{TEAM_A}','adapter-a','1','/adapter','{{}}',1,1,1)"
        ),
        format!(
            "INSERT INTO bindings VALUES(X'{BINDING_A}',X'{TEAM_A}',X'{ROLE_A}',X'{HARNESS_A}',X'{ADAPTER_A}','ACTIVE',1,1,'{{}}',1,1,1,NULL)"
        ),
        format!(
            "INSERT INTO tasks VALUES(X'{TASK_A}',X'{TEAM_A}',X'{ROLE_A}','TASK-1','t','d','BLOCKED',1,1,1,NULL,NULL)"
        ),
        format!(
            "INSERT INTO notification_policies VALUES(X'{POLICY_A}',X'{TEAM_A}','pol-a','TASK_BLOCKED','IN_APP','INFO',0,86400,'ACTIVE',1,1,1)"
        ),
        format!(
            "INSERT INTO notification_policies VALUES(X'{POLICY_B}',X'{TEAM_B}','pol-b','TASK_BLOCKED','IN_APP','INFO',0,86400,'ACTIVE',1,1,1)"
        ),
    ] {
        query(&statement).execute(pool).await?;
    }
    Ok(())
}

fn notification(id: &str, team: &str, status: &str) -> String {
    format!(
        "INSERT INTO notifications (id,team_id,recipient_human_id,severity,title,body,status,dedupe_key,version,created_at) VALUES(X'{id}',X'{team}',X'{HUMAN}','INFO','t','b','{status}','K1',1,1)"
    )
}

fn notification_with_policy(id: &str, team: &str, status: &str, policy: &str) -> String {
    let policy_id = if policy == "NULL" {
        "NULL".to_owned()
    } else {
        format!("X'{policy}'")
    };
    format!(
        "INSERT INTO notifications (id,team_id,recipient_human_id,policy_id,severity,title,body,status,dedupe_key,version,created_at) VALUES(X'{id}',X'{team}',X'{HUMAN}',{policy_id},'INFO','t','b','{status}','K1',1,1)"
    )
}

fn attention(id: &str, status: &str) -> String {
    format!(
        "INSERT INTO attention_items (id,team_id,attention_type,source_type,source_id,title,summary,status,version,created_at,updated_at) VALUES(X'{id}',X'{TEAM_A}','TASK_BLOCKED','TASK',X'{TASK_A}','t','s','{status}',1,1,1)"
    )
}

async fn seed_notification(pool: &SqlitePool) -> Result<(), Box<dyn Error>> {
    query(&notification_with_policy(
        NOTIFICATION,
        TEAM_A,
        "UNREAD",
        "NULL",
    ))
    .execute(pool)
    .await?;
    Ok(())
}

#[tokio::test]
async fn a_notification_can_only_be_created_as_unread() -> Result<(), Box<dyn Error>> {
    let pool = database().await?;
    seed(&pool).await?;

    for (index, status) in ["READ", "DISMISSED", "EXPIRED"].iter().enumerate() {
        let id = format!("01921389d31174628f80cb5f79aa1c{index:02}");
        assert!(
            query(&notification(&id, TEAM_A, status))
                .execute(&pool)
                .await
                .is_err(),
            "notification created directly as {status} unexpectedly succeeded"
        );
    }
    // A notification may not claim it was read at the moment of creation.
    assert!(
        query("INSERT INTO notifications (id,team_id,recipient_human_id,severity,title,body,status,dedupe_key,version,created_at,read_at) VALUES(X'01921389d31174628f80cb5f79aa1c90',X'01921389d31174628f80cb5f79aa1901',X'01921389d31174628f80cb5f79aa1b90','INFO','t','b','UNREAD','K9',1,1,1)")
            .execute(&pool)
            .await
            .is_err(),
        "UNREAD notification carrying read_at unexpectedly succeeded"
    );
    Ok(())
}

#[tokio::test]
async fn a_notification_may_not_be_resurrected_or_take_an_undefined_edge()
-> Result<(), Box<dyn Error>> {
    let pool = database().await?;
    seed(&pool).await?;
    seed_notification(&pool).await?;

    query(&format!(
        "UPDATE notifications SET status='DISMISSED', dismissed_at=2 WHERE id=X'{NOTIFICATION}'"
    ))
    .execute(&pool)
    .await?;

    for to in ["UNREAD", "READ", "EXPIRED"] {
        assert!(
            query(&format!(
                "UPDATE notifications SET status='{to}' WHERE id=X'{NOTIFICATION}'"
            ))
            .execute(&pool)
            .await
            .is_err(),
            "DISMISSED -> {to} unexpectedly succeeded"
        );
    }
    Ok(())
}

#[tokio::test]
async fn a_notification_may_not_leave_its_team_or_have_its_content_rewritten()
-> Result<(), Box<dyn Error>> {
    let pool = database().await?;
    seed(&pool).await?;
    seed_notification(&pool).await?;

    for (name, sql) in [
        (
            "move to another Team",
            format!("UPDATE notifications SET team_id=X'{TEAM_B}' WHERE id=X'{NOTIFICATION}'"),
        ),
        (
            "rewrite the dedupe key",
            format!("UPDATE notifications SET dedupe_key='K-forged' WHERE id=X'{NOTIFICATION}'"),
        ),
        (
            "rewrite the title",
            format!("UPDATE notifications SET title='forged' WHERE id=X'{NOTIFICATION}'"),
        ),
        (
            "rewrite the body",
            format!("UPDATE notifications SET body='forged' WHERE id=X'{NOTIFICATION}'"),
        ),
        (
            "rewrite created_at",
            format!("UPDATE notifications SET created_at=0 WHERE id=X'{NOTIFICATION}'"),
        ),
        (
            "roll the version back",
            format!("UPDATE notifications SET version=0 WHERE id=X'{NOTIFICATION}'"),
        ),
        (
            "point at another Team's policy",
            format!("UPDATE notifications SET policy_id=X'{POLICY_B}' WHERE id=X'{NOTIFICATION}'"),
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
async fn a_notification_may_not_reference_another_teams_objects() -> Result<(), Box<dyn Error>> {
    let pool = database().await?;
    seed(&pool).await?;

    assert!(
        query(&notification_with_policy(
            "01921389d31174628f80cb5f79aa1d01",
            TEAM_A,
            "UNREAD",
            POLICY_B
        ))
        .execute(&pool)
        .await
        .is_err(),
        "notification referencing another Team's policy unexpectedly succeeded"
    );
    Ok(())
}

#[tokio::test]
async fn an_attention_item_can_only_be_created_as_open() -> Result<(), Box<dyn Error>> {
    let pool = database().await?;
    seed(&pool).await?;

    for (index, status) in ["RESOLVED", "DISMISSED"].iter().enumerate() {
        let id = format!("01921389d31174628f80cb5f79aa1e{index:02}");
        assert!(
            query(&attention(&id, status)).execute(&pool).await.is_err(),
            "attention item created directly as {status} unexpectedly succeeded"
        );
    }
    // Its source must exist inside the same Team.
    assert!(
        query("INSERT INTO attention_items (id,team_id,attention_type,source_type,source_id,title,summary,status,version,created_at,updated_at) VALUES(X'01921389d31174628f80cb5f79aa1e90',X'01921389d31174628f80cb5f79aa1901','TASK_BLOCKED','TASK',X'01921389d31174628f80cb5f79aa1fff','t','s','OPEN',1,1,1)")
            .execute(&pool)
            .await
            .is_err(),
        "attention item pointing at a non-existent Task unexpectedly succeeded"
    );
    Ok(())
}

#[tokio::test]
async fn an_attention_item_may_not_be_reopened_or_carry_both_terminal_states()
-> Result<(), Box<dyn Error>> {
    let pool = database().await?;
    seed(&pool).await?;
    query(&attention(ATTENTION, "OPEN")).execute(&pool).await?;

    for (name, sql) in [
        (
            "claim resolved while still OPEN",
            format!("UPDATE attention_items SET resolved_at=2 WHERE id=X'{ATTENTION}'"),
        ),
        (
            "point at an object from another Team",
            format!(
                "UPDATE attention_items SET source_id=X'01921389d31174628f80cb5f79aa1fff' WHERE id=X'{ATTENTION}'"
            ),
        ),
        (
            "move to another Team",
            format!("UPDATE attention_items SET team_id=X'{TEAM_B}' WHERE id=X'{ATTENTION}'"),
        ),
        (
            "rewrite the summary",
            format!("UPDATE attention_items SET summary='forged' WHERE id=X'{ATTENTION}'"),
        ),
    ] {
        assert!(
            query(&sql).execute(&pool).await.is_err(),
            "{name} unexpectedly succeeded"
        );
    }

    // Resolve the real source before closing its derived Attention (0023).
    query(&format!(
        "UPDATE tasks SET status='READY',version=2,updated_at=2 WHERE id=X'{TASK_A}'"
    ))
    .execute(&pool)
    .await?;

    query(&format!(
        "UPDATE attention_items SET status='RESOLVED', resolved_at=2 WHERE id=X'{ATTENTION}'"
    ))
    .execute(&pool)
    .await?;

    for (name, sql) in [
        (
            "reopen a resolved item",
            format!("UPDATE attention_items SET status='OPEN' WHERE id=X'{ATTENTION}'"),
        ),
        (
            "add a dismissal reason to a resolved item",
            format!("UPDATE attention_items SET dismiss_reason='x' WHERE id=X'{ATTENTION}'"),
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
async fn the_whole_legal_notification_and_attention_path_still_works() -> Result<(), Box<dyn Error>>
{
    let pool = database().await?;
    seed(&pool).await?;
    query(&notification_with_policy(
        NOTIFICATION,
        TEAM_A,
        "UNREAD",
        POLICY_A,
    ))
    .execute(&pool)
    .await?;
    query(&format!(
        "UPDATE notifications SET status='READ', read_at=2 WHERE id=X'{NOTIFICATION}'"
    ))
    .execute(&pool)
    .await?;
    query(&format!(
        "UPDATE notifications SET status='DISMISSED', dismissed_at=3 WHERE id=X'{NOTIFICATION}'"
    ))
    .execute(&pool)
    .await?;

    let sql = format!("SELECT status FROM notifications WHERE id=X'{NOTIFICATION}'");
    assert_eq!(
        query_scalar::<_, String>(&sql).fetch_one(&pool).await?,
        "DISMISSED"
    );

    query(&attention(ATTENTION, "OPEN")).execute(&pool).await?;
    query(&format!(
        "UPDATE attention_items SET status='DISMISSED', dismiss_reason='not applicable', dismissed_at=2 WHERE id=X'{ATTENTION}'"
    ))
    .execute(&pool)
    .await?;
    let sql = format!("SELECT status FROM attention_items WHERE id=X'{ATTENTION}'");
    assert_eq!(
        query_scalar::<_, String>(&sql).fetch_one(&pool).await?,
        "DISMISSED"
    );
    Ok(())
}

/// Pins the regression itself: on a `v18` schema the forged rows are accepted.
#[tokio::test]
async fn v18_really_did_accept_terminal_state_at_creation() -> Result<(), Box<dyn Error>> {
    let pool = v18_database().await?;
    seed(&pool).await?;

    assert!(
        query(&notification(
            "01921389d31174628f80cb5f79aa1f90",
            TEAM_A,
            "DISMISSED"
        ))
        .execute(&pool)
        .await
        .is_ok(),
        "v18 unexpectedly rejected the forged notification"
    );
    // `0008`'s own CHECK already rejects a RESOLVED item without `resolved_at`,
    // so the real bypass is a RESOLVED item born *with* one.
    assert!(
        query("INSERT INTO attention_items (id,team_id,attention_type,source_type,source_id,title,summary,status,version,created_at,updated_at,resolved_at) VALUES(X'01921389d31174628f80cb5f79aa1f91',X'01921389d31174628f80cb5f79aa1901','TASK_BLOCKED','TASK',X'01921389d31174628f80cb5f79aa1b93','t','s','RESOLVED',1,1,1,1)")
            .execute(&pool)
            .await
            .is_ok(),
        "v18 unexpectedly rejected the forged attention item"
    );
    Ok(())
}
