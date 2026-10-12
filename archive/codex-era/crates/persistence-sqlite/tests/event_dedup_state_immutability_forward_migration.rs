//! DEV-02-07: forward migration `0016` closes the UPDATE path on delivery
//! deduplication and requires a ROLE_BINDING Event snapshot to name a Binding
//! that still holds execution capability.
//!
//! 0015 stopped `INSERT OR REPLACE` from resetting Outbox and consumption
//! state, but an `UPDATE` restored the same state.  These tests pin both halves
//! of the guarantee: the illegal rewrites are rejected with zero side effects,
//! and the legal forward transitions the DEV-02-14 dispatcher needs still work.

use std::{borrow::Cow, error::Error, future::Future, pin::Pin, str::FromStr};

use persistence_sqlite::migrate;
use sqlx_core::{
    error::BoxDynError,
    migrate::{Migration, MigrationSource, MigrationType, Migrator},
    query::query,
    query_scalar::query_scalar,
};
use sqlx_sqlite::{SqliteConnectOptions, SqlitePool, SqlitePoolOptions};

const V15: [(&str, &str); 15] = [
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
];

#[derive(Debug)]
struct V15Migrations;

impl MigrationSource<'static> for V15Migrations {
    fn resolve(
        self,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<Migration>, BoxDynError>> + Send + 'static>> {
        Box::pin(async {
            Ok(V15
                .iter()
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

async fn v15_database() -> Result<SqlitePool, Box<dyn Error>> {
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(SqliteConnectOptions::from_str("sqlite::memory:")?.foreign_keys(true))
        .await?;
    Migrator::new(V15Migrations).await?.run(&pool).await?;
    Ok(pool)
}

const TEAM: &str = "01921389d31174628f80cb5f79aa1901";
const ROLE: &str = "01921389d31174628f80cb5f79aa1902";
const HARNESS: &str = "01921389d31174628f80cb5f79aa1903";
const ADAPTER: &str = "01921389d31174628f80cb5f79aa1904";
const BINDING: &str = "01921389d31174628f80cb5f79aa1905";
const TASK: &str = "01921389d31174628f80cb5f79aa1906";
const EVENT: &str = "01921389d31174628f80cb5f79aa1916";
const OTHER_EVENT: &str = "01921389d31174628f80cb5f79aa1918";
const HASH: &str = "E3B0C44298FC1C149AFBF4C8996FB92427AE41E4649B934CA495991B7852B855";

async fn seed_team(pool: &SqlitePool) -> Result<(), Box<dyn Error>> {
    for statement in [
        format!("INSERT INTO teams VALUES(X'{TEAM}','team-a','ACTIVE',1,1,1,NULL)"),
        format!(
            "INSERT INTO roles VALUES(X'{ROLE}',X'{TEAM}','role-a','role','ACTIVE',1,1,1,NULL)"
        ),
        format!(
            "INSERT INTO harness_instances VALUES(X'{HARNESS}',X'{TEAM}','mock','harness','ACTIVE','[]',NULL,1,1,1,NULL)"
        ),
        format!(
            "INSERT INTO adapters VALUES(X'{ADAPTER}',X'{TEAM}','adapter-a','1','/adapter','{{}}',1,1,1)"
        ),
        format!(
            "INSERT INTO bindings VALUES(X'{BINDING}',X'{TEAM}',X'{ROLE}',X'{HARNESS}',X'{ADAPTER}','ACTIVE',1,1,'{{}}',1,1,1,NULL)"
        ),
        format!(
            "INSERT INTO tasks VALUES(X'{TASK}',X'{TEAM}',X'{ROLE}','task-a','task','','DOING',1,1,1,NULL,NULL)"
        ),
    ] {
        query(&statement).execute(pool).await?;
    }
    Ok(())
}

fn system_event(id: &str) -> String {
    format!(
        "INSERT INTO events (id,team_id,event_type,actor_type,actor_id,subject_type,subject_id,request_id,object_version,payload,metadata,occurred_at,previous_hash,hash) VALUES(X'{id}',X'{TEAM}','audit.recorded','SYSTEM',NULL,'TASK',X'{TASK}',NULL,1,'{{}}','{{}}',1,NULL,'{HASH}')"
    )
}

fn outbox_row(id: &str, event: &str, status: &str, retry_count: i64, published_at: &str) -> String {
    format!(
        "INSERT INTO outbox (id,team_id,event_id,destination,payload,status,retry_count,max_retries,last_error,idempotency_key,created_at,scheduled_at,published_at) VALUES(X'{id}',X'{TEAM}',X'{event}','READ_MODEL_PROJECTION','{{}}','{status}',{retry_count},5,NULL,'k1',1,1,{published_at})"
    )
}

#[tokio::test]
async fn consumption_deduplication_survives_every_update_path() -> Result<(), Box<dyn Error>> {
    let pool = database().await?;
    assert_eq!(
        query_scalar::<_, i64>("PRAGMA recursive_triggers")
            .fetch_one(&pool)
            .await?,
        0
    );
    seed_team(&pool).await?;
    query(&system_event(EVENT)).execute(&pool).await?;
    query(&outbox_row(
        "01921389d31174628f80cb5f79aa1917",
        EVENT,
        "PUBLISHED",
        3,
        "500",
    ))
    .execute(&pool)
    .await?;
    query(&format!(
        "INSERT INTO event_consumptions VALUES('projector',X'{EVENT}',X'{TEAM}',7)"
    ))
    .execute(&pool)
    .await?;

    for (name, sql) in [
        (
            "reset consumed_at",
            "UPDATE event_consumptions SET consumed_at=0".to_owned(),
        ),
        (
            "rewrite consumer_id",
            "UPDATE event_consumptions SET consumer_id='attacker'".to_owned(),
        ),
        (
            "rewrite event_id",
            format!("UPDATE event_consumptions SET event_id=X'{OTHER_EVENT}'"),
        ),
    ] {
        assert!(
            query(&sql).execute(&pool).await.is_err(),
            "{name} unexpectedly succeeded"
        );
    }

    assert_eq!(
        query_scalar::<_, i64>("SELECT consumed_at FROM event_consumptions")
            .fetch_one(&pool)
            .await?,
        7
    );
    assert_eq!(
        query_scalar::<_, String>("SELECT consumer_id FROM event_consumptions")
            .fetch_one(&pool)
            .await?,
        "projector"
    );
    Ok(())
}

#[tokio::test]
async fn published_outbox_cannot_roll_back_or_lose_its_evidence() -> Result<(), Box<dyn Error>> {
    let pool = database().await?;
    seed_team(&pool).await?;
    query(&system_event(EVENT)).execute(&pool).await?;
    query(&outbox_row(
        "01921389d31174628f80cb5f79aa1917",
        EVENT,
        "PUBLISHED",
        3,
        "500",
    ))
    .execute(&pool)
    .await?;

    for (name, sql) in [
        (
            "roll back to PENDING",
            "UPDATE outbox SET status='PENDING'".to_owned(),
        ),
        (
            "switch to DEAD_LETTER",
            "UPDATE outbox SET status='DEAD_LETTER'".to_owned(),
        ),
        (
            "clear published_at",
            "UPDATE outbox SET published_at=NULL".to_owned(),
        ),
        (
            "lower retry_count",
            "UPDATE outbox SET retry_count=0".to_owned(),
        ),
        (
            "rewrite idempotency_key",
            "UPDATE outbox SET idempotency_key='forged'".to_owned(),
        ),
        (
            "rewrite event_id",
            format!("UPDATE outbox SET event_id=X'{OTHER_EVENT}'"),
        ),
    ] {
        assert!(
            query(&sql).execute(&pool).await.is_err(),
            "{name} unexpectedly succeeded"
        );
    }

    let row: (String, i64, Option<i64>, String) = sqlx_core::query_as::query_as(
        "SELECT status, retry_count, published_at, idempotency_key FROM outbox",
    )
    .fetch_one(&pool)
    .await?;
    assert_eq!(row, ("PUBLISHED".to_owned(), 3, Some(500), "k1".to_owned()));
    Ok(())
}

#[tokio::test]
async fn dispatcher_forward_transitions_remain_available() -> Result<(), Box<dyn Error>> {
    let pool = database().await?;
    seed_team(&pool).await?;
    query(&system_event(EVENT)).execute(&pool).await?;
    query(&outbox_row(
        "01921389d31174628f80cb5f79aa1917",
        EVENT,
        "PENDING",
        0,
        "NULL",
    ))
    .execute(&pool)
    .await?;

    for sql in [
        "UPDATE outbox SET status='PROCESSING', retry_count=1",
        "UPDATE outbox SET status='FAILED', last_error='boom'",
        "UPDATE outbox SET status='PENDING', retry_count=2, last_error=NULL",
        "UPDATE outbox SET status='PUBLISHED', published_at=500",
        "UPDATE outbox SET retry_count=3",
        "UPDATE outbox SET status='PUBLISHED'",
        "UPDATE outbox SET payload='{\"v\":2}'",
    ] {
        query(sql).execute(&pool).await?;
    }

    let row: (String, i64, Option<i64>, String) = sqlx_core::query_as::query_as(
        "SELECT status, retry_count, published_at, idempotency_key FROM outbox",
    )
    .fetch_one(&pool)
    .await?;
    assert_eq!(row, ("PUBLISHED".to_owned(), 3, Some(500), "k1".to_owned()));
    Ok(())
}

#[tokio::test]
async fn role_event_snapshot_requires_a_binding_that_can_execute() -> Result<(), Box<dyn Error>> {
    let pool = database().await?;
    seed_team(&pool).await?;

    for (name, status, effective_from, effective_to) in [
        ("revoked", "REVOKED", "1", "2"),
        ("failed", "FAILED", "NULL", "NULL"),
    ] {
        query(&format!(
            "UPDATE bindings SET status='{status}', effective_from={effective_from}, effective_to={effective_to} WHERE id=X'{BINDING}'"
        ))
        .execute(&pool)
        .await?;
        let sql = format!(
            "INSERT INTO events (id,team_id,event_type,actor_type,actor_id,actor_kind,actor_role_id,actor_binding_id,actor_binding_version,actor_harness_instance_id,subject_type,subject_id,request_id,object_version,payload,metadata,occurred_at,previous_hash,hash) VALUES(X'01921389d31174628f80cb5f79aa1940',X'{TEAM}','audit.recorded','ROLE',X'{ROLE}','ROLE_BINDING',X'{ROLE}',X'{BINDING}',1,X'{HARNESS}','TASK',X'{TASK}',NULL,1,'{{}}','{{}}',1,NULL,'{HASH}')"
        );
        assert!(
            query(&sql).execute(&pool).await.is_err(),
            "{name} binding unexpectedly recorded as execution actor"
        );
        assert_eq!(
            query_scalar::<_, i64>("SELECT COUNT(*) FROM events")
                .fetch_one(&pool)
                .await?,
            0
        );
    }
    Ok(())
}

#[tokio::test]
async fn existing_snapshots_survive_later_binding_revocation() -> Result<(), Box<dyn Error>> {
    let pool = database().await?;
    seed_team(&pool).await?;
    let sql = format!(
        "INSERT INTO events (id,team_id,event_type,actor_type,actor_id,actor_kind,actor_role_id,actor_binding_id,actor_binding_version,actor_harness_instance_id,subject_type,subject_id,request_id,object_version,payload,metadata,occurred_at,previous_hash,hash) VALUES(X'{EVENT}',X'{TEAM}','audit.recorded','ROLE',X'{ROLE}','ROLE_BINDING',X'{ROLE}',X'{BINDING}',1,X'{HARNESS}','TASK',X'{TASK}',NULL,1,'{{}}','{{}}',1,NULL,'{HASH}')"
    );
    query(&sql).execute(&pool).await?;
    query("UPDATE bindings SET status='REVOKED', effective_from=1, effective_to=2 WHERE id=X'01921389d31174628f80cb5f79aa1905'")
        .execute(&pool)
        .await?;

    let row: (String, i64, Vec<u8>) = sqlx_core::query_as::query_as(
        "SELECT actor_kind, actor_binding_version, actor_harness_instance_id FROM events",
    )
    .fetch_one(&pool)
    .await?;
    assert_eq!(row.0, "ROLE_BINDING");
    assert_eq!(row.1, 1);
    assert_eq!(row.2, hex_to_bytes(HARNESS));
    Ok(())
}

fn hex_to_bytes(hex: &str) -> Vec<u8> {
    (0..hex.len())
        .step_by(2)
        .map(|index| u8::from_str_radix(&hex[index..index + 2], 16).unwrap_or(0))
        .collect()
}

#[tokio::test]
async fn dirty_delivery_history_is_rejected_atomically() -> Result<(), Box<dyn Error>> {
    let pool = v15_database().await?;
    seed_team(&pool).await?;
    query(&system_event(EVENT)).execute(&pool).await?;
    // A published fact with no publication timestamp is audit distortion that
    // 0016 refuses to inherit.
    query(&format!(
        "INSERT INTO outbox (id,team_id,event_id,destination,payload,status,retry_count,max_retries,last_error,idempotency_key,created_at,scheduled_at,published_at) VALUES(X'01921389d31174628f80cb5f79aa1917',X'{TEAM}',X'{EVENT}','READ_MODEL_PROJECTION','{{}}','PUBLISHED',0,5,NULL,NULL,1,1,NULL)"
    ))
    .execute(&pool)
    .await?;

    let result = migrate(&pool).await;
    assert!(result.is_err(), "dirty v15 history upgraded silently");

    let status: String = query_scalar("SELECT status FROM outbox")
        .fetch_one(&pool)
        .await?;
    assert_eq!(status, "PUBLISHED");
    let guards: i64 = query_scalar(
        "SELECT COUNT(*) FROM sqlite_master WHERE type='trigger' AND name='outbox_deduplication_state_immutable_update'",
    )
    .fetch_one(&pool)
    .await?;
    assert_eq!(guards, 0, "0016 guards installed despite rejected upgrade");
    Ok(())
}
