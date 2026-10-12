//! DEV-02-08 遗留修复：forward migration `0018` closes the creation axis that
//! `0017` left open.
//!
//! `0017` installed `messages_state_machine_update` and
//! `delivery_attempts_state_machine_update`, both `BEFORE UPDATE OF status`.
//! The 35-attack counterexample review behind `0017` used UPDATE semantics
//! throughout, so the whole creation axis was never attempted.  A message could
//! be born as `DELIVERED` or `REPLIED` and an attempt as `SUCCEEDED` with a
//! `NULL` `completed_at`, skipping the frozen transition table entirely.
//!
//! `STATE_MACHINES.md` section 5 is unambiguous: a Message is created as
//! `DRAFT` and an attempt begins at `PENDING`.  `0018` adds those entry
//! constraints plus the matching "no terminal timestamp before the state"
//! rules.  No state, error code or permission is introduced.
//!
//! Every status asserted here is transcribed from that frozen table.

use std::{borrow::Cow, error::Error, future::Future, pin::Pin, str::FromStr};

use persistence_sqlite::migrate;
use sqlx_core::{
    error::BoxDynError,
    migrate::{Migration, MigrationSource, MigrationType, Migrator},
    query::query,
    query_scalar::query_scalar,
};
use sqlx_sqlite::{SqliteConnectOptions, SqlitePool, SqlitePoolOptions};

const V18: [(&str, &str); 18] = [
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
];

/// Applies every migration the crate knows, including `0018` and `0019`.
async fn database() -> Result<SqlitePool, Box<dyn Error>> {
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(SqliteConnectOptions::from_str("sqlite::memory:")?.foreign_keys(true))
        .await?;
    migrate(&pool).await?;
    Ok(pool)
}

/// Stops at `0017` so the regression test can prove the pre-`0018` schema
/// really accepted a message born in a terminal state.
#[derive(Debug)]
struct V17Migrations;

impl MigrationSource<'static> for V17Migrations {
    fn resolve(
        self,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<Migration>, BoxDynError>> + Send + 'static>> {
        Box::pin(async {
            Ok(V18
                .iter()
                .take(17)
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

async fn v17_database() -> Result<SqlitePool, Box<dyn Error>> {
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(SqliteConnectOptions::from_str("sqlite::memory:")?.foreign_keys(true))
        .await?;
    Migrator::new(V17Migrations).await?.run(&pool).await?;
    Ok(pool)
}

const TEAM_A: &str = "01921389d31174628f80cb5f79aa1901";
const ROLE_A: &str = "01921389d31174628f80cb5f79aa1902";
const HARNESS_A: &str = "01921389d31174628f80cb5f79aa1903";
const ADAPTER_A: &str = "01921389d31174628f80cb5f79aa1904";
const BINDING_A: &str = "01921389d31174628f80cb5f79aa1905";
const THREAD: &str = "01921389d31174628f80cb5f79aa1b01";

async fn seed(pool: &SqlitePool) -> Result<(), Box<dyn Error>> {
    for statement in [
        format!("INSERT INTO teams VALUES(X'{TEAM_A}','team-a','ACTIVE',1,1,1,NULL)"),
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
        format!("INSERT INTO threads VALUES(X'{THREAD}',X'{TEAM_A}','thread-a','OPEN',1,1,1)"),
    ] {
        query(&statement).execute(pool).await?;
    }
    Ok(())
}

fn message(id: &str, status: &str, delivered_at: &str) -> String {
    let ts = if delivered_at == "NULL" {
        "NULL".to_owned()
    } else {
        delivered_at.to_string()
    };
    format!(
        "INSERT INTO messages (id,team_id,thread_id,sender_actor_type,from_role_id,from_human_id,to_role_id,sender_binding_snapshot_id,sender_binding_generation,reply_to_message_id,message_type,body,status,version,idempotency_key,expires_at,created_at,updated_at,delivered_at,acknowledged_at,replied_at) VALUES(X'{id}',X'{TEAM_A}',X'{THREAD}','ROLE',X'{ROLE_A}',NULL,X'{ROLE_A}',X'{BINDING_A}',1,NULL,'DIRECT','body','{status}',1,NULL,NULL,1,1,{ts},NULL,NULL)"
    )
}

fn attempt(
    id: &str,
    message_id: &str,
    status: &str,
    started_at: &str,
    completed_at: &str,
) -> String {
    let started = if started_at == "NULL" {
        "NULL".to_owned()
    } else {
        started_at.to_string()
    };
    let completed = if completed_at == "NULL" {
        "NULL".to_owned()
    } else {
        completed_at.to_string()
    };
    format!(
        "INSERT INTO delivery_attempts (id,team_id,message_id,attempt_number,target_role_id,target_binding_id,target_binding_generation,status,error_code,error_message,created_at,started_at,completed_at) VALUES(X'{id}',X'{TEAM_A}',X'{message_id}',1,X'{ROLE_A}',X'{BINDING_A}',1,'{status}',NULL,NULL,1,{started},{completed})"
    )
}

/// The frozen section 5 table lists no row whose From is `REPLIED` or `EXPIRED`,
/// and no row whose From is `SUCCEEDED`, `FAILED` or `ABANDONED` on an attempt.
/// Every one of those must therefore be unreachable at creation time.
#[tokio::test]
async fn message_can_only_be_created_as_draft() -> Result<(), Box<dyn Error>> {
    let pool = database().await?;
    seed(&pool).await?;

    for (index, status) in [
        "QUEUED",
        "ROUTED",
        "NEEDS_HUMAN",
        "EXPIRED",
        "DELIVERED",
        "FAILED",
        "ACKNOWLEDGED",
        "PROCESSING",
        "REPLIED",
    ]
    .iter()
    .enumerate()
    {
        let id = format!("01921389d31174628f80cb5f79aa1c{index:02}");
        assert!(
            query(&message(&id, status, "NULL"))
                .execute(&pool)
                .await
                .is_err(),
            "message created directly as {status} unexpectedly succeeded"
        );
    }

    assert_eq!(
        query_scalar::<_, i64>("SELECT COUNT(*) FROM messages")
            .fetch_one(&pool)
            .await?,
        0
    );

    query(&message(
        "01921389d31174628f80cb5f79aa1c90",
        "DRAFT",
        "NULL",
    ))
    .execute(&pool)
    .await?;
    assert_eq!(
        query_scalar::<_, String>("SELECT status FROM messages")
            .fetch_one(&pool)
            .await?,
        "DRAFT"
    );
    Ok(())
}

#[tokio::test]
async fn delivery_attempt_can_only_be_created_as_pending() -> Result<(), Box<dyn Error>> {
    let pool = database().await?;
    seed(&pool).await?;
    query(&message(
        "01921389d31174628f80cb5f79aa1d00",
        "DRAFT",
        "NULL",
    ))
    .execute(&pool)
    .await?;
    for step in ["QUEUED", "ROUTED"] {
        query(&format!(
            "UPDATE messages SET status='{step}' WHERE id=X'01921389d31174628f80cb5f79aa1d00'"
        ))
        .execute(&pool)
        .await?;
    }

    for (index, status) in ["IN_FLIGHT", "SUCCEEDED", "FAILED", "ABANDONED"]
        .iter()
        .enumerate()
    {
        let id = format!("01921389d31174628f80cb5f79aa1d{index:02}");
        assert!(
            query(&attempt(
                &id,
                "01921389d31174628f80cb5f79aa1d00",
                status,
                "NULL",
                "NULL"
            ))
            .execute(&pool)
            .await
            .is_err(),
            "attempt created directly as {status} unexpectedly succeeded"
        );
    }

    query(&attempt(
        "01921389d31174628f80cb5f79aa1d90",
        "01921389d31174628f80cb5f79aa1d00",
        "PENDING",
        "NULL",
        "NULL",
    ))
    .execute(&pool)
    .await?;
    assert_eq!(
        query_scalar::<_, String>("SELECT status FROM delivery_attempts")
            .fetch_one(&pool)
            .await?,
        "PENDING"
    );
    Ok(())
}

#[tokio::test]
async fn terminal_timestamps_cannot_precede_their_state() -> Result<(), Box<dyn Error>> {
    let pool = database().await?;
    seed(&pool).await?;

    // A DRAFT message that already claims to be delivered.
    assert!(
        query(&message("01921389d31174628f80cb5f79aa1e00", "DRAFT", "99"))
            .execute(&pool)
            .await
            .is_err(),
        "DRAFT message carrying delivered_at unexpectedly succeeded"
    );

    // A PENDING attempt that already claims to have started and completed.
    query(&message(
        "01921389d31174628f80cb5f79aa1e01",
        "DRAFT",
        "NULL",
    ))
    .execute(&pool)
    .await?;
    for step in ["QUEUED", "ROUTED"] {
        query(&format!(
            "UPDATE messages SET status='{step}' WHERE id=X'01921389d31174628f80cb5f79aa1e01'"
        ))
        .execute(&pool)
        .await?;
    }
    for (label, started, completed) in [("started_at", "99", "NULL"), ("completed_at", "99", "99")]
    {
        assert!(
            query(&attempt(
                "01921389d31174628f80cb5f79aa1e02",
                "01921389d31174628f80cb5f79aa1e01",
                "PENDING",
                started,
                completed
            ))
            .execute(&pool)
            .await
            .is_err(),
            "PENDING attempt carrying {label} unexpectedly succeeded"
        );
    }
    Ok(())
}

#[tokio::test]
async fn the_whole_legal_delivery_path_still_works() -> Result<(), Box<dyn Error>> {
    let pool = database().await?;
    seed(&pool).await?;

    query(&message(
        "01921389d31174628f80cb5f79aa1f00",
        "DRAFT",
        "NULL",
    ))
    .execute(&pool)
    .await?;
    for step in ["QUEUED", "ROUTED"] {
        query(&format!(
            "UPDATE messages SET status='{step}' WHERE id=X'01921389d31174628f80cb5f79aa1f00'"
        ))
        .execute(&pool)
        .await?;
    }
    query(&attempt(
        "01921389d31174628f80cb5f79aa1f01",
        "01921389d31174628f80cb5f79aa1f00",
        "PENDING",
        "NULL",
        "NULL",
    ))
    .execute(&pool)
    .await?;
    for (attempt_status, message_status) in
        [("IN_FLIGHT", "DELIVERED"), ("SUCCEEDED", "ACKNOWLEDGED")]
    {
        query(&format!(
            "UPDATE delivery_attempts SET status='{attempt_status}', started_at=2, completed_at=3 WHERE id=X'01921389d31174628f80cb5f79aa1f01'"
        ))
        .execute(&pool)
        .await?;
        query(&format!(
            "UPDATE messages SET status='{message_status}' WHERE id=X'01921389d31174628f80cb5f79aa1f00'"
        ))
        .execute(&pool)
        .await?;
    }

    assert_eq!(
        query_scalar::<_, String>("SELECT status FROM messages")
            .fetch_one(&pool)
            .await?,
        "ACKNOWLEDGED"
    );
    assert_eq!(
        query_scalar::<_, String>("SELECT status FROM delivery_attempts")
            .fetch_one(&pool)
            .await?,
        "SUCCEEDED"
    );
    Ok(())
}

/// Pins the regression itself: on a `v17` schema the same rows are accepted.
/// Without this test the new guard could be silently dropped and the suite
/// would still pass, because every other test only exercises `v18`.
#[tokio::test]
async fn v17_really_did_accept_terminal_state_at_creation() -> Result<(), Box<dyn Error>> {
    let pool = v17_database().await?;
    seed(&pool).await?;

    let forged = query(
        "INSERT INTO messages (id,team_id,thread_id,sender_actor_type,from_role_id,from_human_id,to_role_id,sender_binding_snapshot_id,sender_binding_generation,reply_to_message_id,message_type,body,status,version,idempotency_key,expires_at,created_at,updated_at,delivered_at,acknowledged_at,replied_at) VALUES(X'01921389d31174628f80cb5f79aa1f90',X'01921389d31174628f80cb5f79aa1901',X'01921389d31174628f80cb5f79aa1b01','ROLE',X'01921389d31174628f80cb5f79aa1902',NULL,X'01921389d31174628f80cb5f79aa1902',X'01921389d31174628f80cb5f79aa1905',1,NULL,'DIRECT','body','DELIVERED',1,NULL,NULL,1,1,NULL,NULL,NULL)",
    )
    .execute(&pool)
    .await;

    assert!(
        forged.is_ok(),
        "v17 unexpectedly rejected the forged row, so this regression test would not prove anything"
    );
    assert_eq!(
        query_scalar::<_, String>("SELECT status FROM messages")
            .fetch_one(&pool)
            .await?,
        "DELIVERED"
    );
    Ok(())
}

/// `0018` constrains the creation action, never existing history.  Reaching an
/// advanced state through the legal chain must still work under `0018`,
/// which is why `0018` installs no upgrade guard — unlike `0017`.
#[tokio::test]
async fn advanced_history_remains_reachable_under_0018() -> Result<(), Box<dyn Error>> {
    let pool = database().await?;
    seed(&pool).await?;

    for (id, target) in [
        ("01921389d31174628f80cb5f79aa1fb1", "DELIVERED"),
        ("01921389d31174628f80cb5f79aa1fb2", "ACKNOWLEDGED"),
    ] {
        query(&message(id, "DRAFT", "NULL")).execute(&pool).await?;
        for step in ["QUEUED", "ROUTED", "DELIVERED", "ACKNOWLEDGED"] {
            query(&format!(
                "UPDATE messages SET status='{step}' WHERE id=X'{id}'"
            ))
            .execute(&pool)
            .await?;
            if step == target {
                break;
            }
        }
        let sql = format!("SELECT status FROM messages WHERE id=X'{id}'");
        assert_eq!(
            query_scalar::<_, String>(&sql).fetch_one(&pool).await?,
            target
        );
    }
    Ok(())
}
