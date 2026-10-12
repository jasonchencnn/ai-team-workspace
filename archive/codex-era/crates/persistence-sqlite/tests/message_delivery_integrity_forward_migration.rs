//! DEV-02-07: forward migration `0017` closes the Team, immutability and state
//! gaps that an independent counterexample review reproduced against `0007`.
//!
//! MM-R01 extends this file with the upgrade path of `0025`, which gives the
//! Message Team guard its missing UPDATE arm (deep audit DA-01).  `0025` is
//! verified against a 0001..0024 baseline rather than a 0001..0016 one, because
//! the defect it repairs is only reachable through the schema 0017 already
//! installed.

use std::{borrow::Cow, error::Error, future::Future, pin::Pin, str::FromStr};

use persistence_sqlite::migrate;
use sqlx_core::{
    error::BoxDynError,
    migrate::{Migration, MigrationSource, MigrationType, Migrator},
    query::query,
    query_scalar::query_scalar,
};
use sqlx_sqlite::{SqliteConnectOptions, SqlitePool, SqlitePoolOptions};

const V16: [(&str, &str); 16] = [
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
];

#[derive(Debug)]
struct V16Migrations;

impl MigrationSource<'static> for V16Migrations {
    fn resolve(
        self,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<Migration>, BoxDynError>> + Send + 'static>> {
        Box::pin(async {
            Ok(V16
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

async fn v16_database() -> Result<SqlitePool, Box<dyn Error>> {
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(SqliteConnectOptions::from_str("sqlite::memory:")?.foreign_keys(true))
        .await?;
    Migrator::new(V16Migrations).await?.run(&pool).await?;
    Ok(pool)
}

const TEAM_A: &str = "01921389d31174628f80cb5f79aa1901";
const ROLE_A: &str = "01921389d31174628f80cb5f79aa1902";
const HARNESS_A: &str = "01921389d31174628f80cb5f79aa1903";
const ADAPTER_A: &str = "01921389d31174628f80cb5f79aa1904";
const BINDING_A: &str = "01921389d31174628f80cb5f79aa1905";
const TEAM_B: &str = "01921389d31174628f80cb5f79aa1a01";
const ROLE_B: &str = "01921389d31174628f80cb5f79aa1a02";
const HARNESS_B: &str = "01921389d31174628f80cb5f79aa1a03";
const ADAPTER_B: &str = "01921389d31174628f80cb5f79aa1a04";
const BINDING_B: &str = "01921389d31174628f80cb5f79aa1a05";
const THREAD: &str = "01921389d31174628f80cb5f79aa1b01";
const MESSAGE: &str = "01921389d31174628f80cb5f79aa1b02";
const ATTEMPT: &str = "01921389d31174628f80cb5f79aa1b03";

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
            "INSERT INTO harness_instances VALUES(X'{HARNESS_A}',X'{TEAM_A}','mock','harness','ACTIVE','[]',NULL,1,1,1,NULL)"
        ),
        format!(
            "INSERT INTO harness_instances VALUES(X'{HARNESS_B}',X'{TEAM_B}','mock','harness','ACTIVE','[]',NULL,1,1,1,NULL)"
        ),
        format!(
            "INSERT INTO adapters VALUES(X'{ADAPTER_A}',X'{TEAM_A}','adapter-a','1','/adapter','{{}}',1,1,1)"
        ),
        format!(
            "INSERT INTO adapters VALUES(X'{ADAPTER_B}',X'{TEAM_B}','adapter-b','1','/adapter','{{}}',1,1,1)"
        ),
        format!(
            "INSERT INTO bindings VALUES(X'{BINDING_A}',X'{TEAM_A}',X'{ROLE_A}',X'{HARNESS_A}',X'{ADAPTER_A}','ACTIVE',1,1,'{{}}',1,1,1,NULL)"
        ),
        format!(
            "INSERT INTO bindings VALUES(X'{BINDING_B}',X'{TEAM_B}',X'{ROLE_B}',X'{HARNESS_B}',X'{ADAPTER_B}','ACTIVE',1,1,'{{}}',1,1,1,NULL)"
        ),
        format!("INSERT INTO threads VALUES(X'{THREAD}',X'{TEAM_A}','thread-a','OPEN',1,1,1)"),
    ] {
        query(&statement).execute(pool).await?;
    }
    Ok(())
}

fn message(
    id: &str,
    team: &str,
    thread: &str,
    to_role: &str,
    status: &str,
    idempotency_key: &str,
) -> String {
    // `NULL` stays a bare keyword; every other key becomes a quoted SQL literal.
    let key = if idempotency_key == "NULL" {
        "NULL".to_owned()
    } else {
        format!("'{idempotency_key}'")
    };
    format!(
        "INSERT INTO messages (id,team_id,thread_id,sender_actor_type,from_role_id,from_human_id,to_role_id,sender_binding_snapshot_id,sender_binding_generation,reply_to_message_id,message_type,body,status,version,idempotency_key,expires_at,created_at,updated_at,delivered_at,acknowledged_at,replied_at) VALUES(X'{id}',X'{team}',X'{thread}','ROLE',X'{ROLE_A}',NULL,X'{to_role}',X'{BINDING_A}',1,NULL,'DIRECT','body','{status}',1,{key},NULL,1,1,NULL,NULL,NULL)"
    )
}

fn attempt(id: &str, message_id: &str, target_role: &str, status: &str) -> String {
    format!(
        "INSERT INTO delivery_attempts (id,team_id,message_id,attempt_number,target_role_id,target_binding_id,target_binding_generation,status,error_code,error_message,created_at,started_at,completed_at) VALUES(X'{id}',X'{TEAM_A}',X'{message_id}',1,X'{target_role}',X'{BINDING_A}',1,'{status}',NULL,NULL,1,NULL,NULL)"
    )
}

/// 0018 closes the creation axis: a Message may only be born as `DRAFT` and a
/// DeliveryAttempt only as `PENDING`, per `STATE_MACHINES.md` section 5
/// ("创建 → DRAFT" and "每次 attempt 单独记录 PENDING → IN_FLIGHT → ...").
///
/// These helpers build a target state by walking the legal chain instead of
/// inserting the terminal state directly.  The direct inserts these tests used
/// before 0018 were themselves a specification violation that 0017's
/// UPDATE-only state machine could not see.
async fn seeded_message(
    pool: &SqlitePool,
    id: &str,
    to_role: &str,
    status: &str,
    idempotency_key: &str,
) -> Result<(), Box<dyn Error>> {
    query(&message(
        id,
        TEAM_A,
        THREAD,
        to_role,
        "DRAFT",
        idempotency_key,
    ))
    .execute(pool)
    .await?;
    // The first step is skipped: the row is already DRAFT, and a no-op
    // `DRAFT -> DRAFT` update is itself an illegal transition.  Each step is
    // executed *before* the stop check, otherwise the row would be left one
    // state short of the requested one.
    if status == "DRAFT" {
        return Ok(());
    }
    for step in [
        "QUEUED",
        "ROUTED",
        "DELIVERED",
        "ACKNOWLEDGED",
        "PROCESSING",
        "REPLIED",
    ] {
        query(&format!(
            "UPDATE messages SET status='{step}' WHERE id=X'{id}'"
        ))
        .execute(pool)
        .await?;
        if step == status {
            return Ok(());
        }
    }
    Ok(())
}

async fn seeded_attempt(
    pool: &SqlitePool,
    id: &str,
    message_id: &str,
    status: &str,
) -> Result<(), Box<dyn Error>> {
    query(&attempt(id, message_id, ROLE_A, "PENDING"))
        .execute(pool)
        .await?;
    // The row is already PENDING, so the first step is skipped.  As above,
    // each step runs before the stop check.
    if status == "PENDING" {
        return Ok(());
    }
    for step in ["IN_FLIGHT", "SUCCEEDED", "FAILED", "ABANDONED"] {
        query(&format!(
            "UPDATE delivery_attempts SET status='{step}' WHERE id=X'{id}'"
        ))
        .execute(pool)
        .await?;
        if step == status {
            return Ok(());
        }
    }
    Ok(())
}

#[tokio::test]
async fn cross_team_message_addressing_is_rejected() -> Result<(), Box<dyn Error>> {
    let pool = database().await?;
    assert_eq!(
        query_scalar::<_, i64>("PRAGMA recursive_triggers")
            .fetch_one(&pool)
            .await?,
        0
    );
    seed(&pool).await?;

    for (name, sql) in [
        (
            "recipient from another Team",
            message(
                "01921389d31174628f80cb5f79aa1b10",
                TEAM_A,
                THREAD,
                ROLE_B,
                "QUEUED",
                "NULL",
            ),
        ),
        (
            "sender Binding snapshot from another Team",
            message(
                "01921389d31174628f80cb5f79aa1b13",
                TEAM_A,
                THREAD,
                ROLE_A,
                "QUEUED",
                "NULL",
            )
            .replace(BINDING_A, BINDING_B),
        ),
    ] {
        assert!(
            query(&sql).execute(&pool).await.is_err(),
            "{name} unexpectedly succeeded"
        );
    }

    // A message hanging on another Team's Thread.
    query(&format!(
        "INSERT INTO threads VALUES(X'01921389d31174628f80cb5f79aa1b11',X'{TEAM_B}','thread-b','OPEN',1,1,1)"
    ))
    .execute(&pool)
    .await?;
    assert!(
        query(&message(
            "01921389d31174628f80cb5f79aa1b12",
            TEAM_A,
            "01921389d31174628f80cb5f79aa1b11",
            ROLE_A,
            "QUEUED",
            "NULL",
        ))
        .execute(&pool)
        .await
        .is_err(),
        "cross-Team thread unexpectedly accepted"
    );

    assert_eq!(
        query_scalar::<_, i64>("SELECT COUNT(*) FROM messages")
            .fetch_one(&pool)
            .await?,
        0
    );
    Ok(())
}

#[tokio::test]
async fn delivery_attempt_cannot_be_redirected() -> Result<(), Box<dyn Error>> {
    let pool = database().await?;
    seed(&pool).await?;
    seeded_message(&pool, MESSAGE, ROLE_A, "QUEUED", "idem-1").await?;

    // Target a Role that is not the message's intended recipient.
    query(&format!(
        "INSERT INTO roles VALUES(X'01921389d31174628f80cb5f79aa1c02',X'{TEAM_A}','role-a2','role','ACTIVE',1,1,1,NULL)"
    ))
    .execute(&pool)
    .await?;
    assert!(
        query(&attempt(
            ATTEMPT,
            MESSAGE,
            "01921389d31174628f80cb5f79aa1c02",
            "PENDING"
        ))
        .execute(&pool)
        .await
        .is_err(),
        "delivery redirected to a different Role"
    );

    // A cross-Team target is equally rejected.
    assert!(
        query(&format!(
            "INSERT INTO delivery_attempts (id,team_id,message_id,attempt_number,target_role_id,target_binding_id,target_binding_generation,status,error_code,error_message,created_at,started_at,completed_at) VALUES(X'01921389d31174628f80cb5f79aa1b14',X'{TEAM_A}',X'{MESSAGE}',2,X'{ROLE_B}',NULL,NULL,'PENDING',NULL,NULL,1,NULL,NULL)"
        ))
        .execute(&pool)
        .await
        .is_err(),
        "cross-Team delivery target unexpectedly accepted"
    );

    // The intended recipient still works.
    query(&attempt(
        "01921389d31174628f80cb5f79aa1b15",
        MESSAGE,
        ROLE_A,
        "PENDING",
    ))
    .execute(&pool)
    .await?;
    assert_eq!(
        query_scalar::<_, i64>("SELECT COUNT(*) FROM delivery_attempts")
            .fetch_one(&pool)
            .await?,
        1
    );
    Ok(())
}

#[tokio::test]
async fn queued_role_message_requires_its_sender_binding_snapshot() -> Result<(), Box<dyn Error>> {
    let pool = database().await?;
    seed(&pool).await?;

    // `0018` requires a Message to be created as DRAFT, so the snapshot rule
    // has to be exercised on the DRAFT -> QUEUED transition rather than on
    // the insert.  Inserting straight to QUEUED would now be short-circuited by
    // the creation guard and the assertions below would pass for the wrong
    // reason.
    for (name, draft_sql, transition) in [
        (
            "missing snapshot once queued",
            "INSERT INTO messages (id,team_id,thread_id,sender_actor_type,from_role_id,from_human_id,to_role_id,sender_binding_snapshot_id,sender_binding_generation,reply_to_message_id,message_type,body,status,version,idempotency_key,expires_at,created_at,updated_at,delivered_at,acknowledged_at,replied_at) VALUES(X'01921389d31174628f80cb5f79aa1b20',X'{TEAM_A}',X'{THREAD}','ROLE',X'{ROLE_A}',NULL,X'{ROLE_A}',NULL,NULL,NULL,'DIRECT','x','DRAFT',1,NULL,NULL,1,1,NULL,NULL,NULL)",
            "UPDATE messages SET status='QUEUED' WHERE id=X'01921389d31174628f80cb5f79aa1b20'",
        ),
        (
            "snapshot generation mismatch",
            "INSERT INTO messages (id,team_id,thread_id,sender_actor_type,from_role_id,from_human_id,to_role_id,sender_binding_snapshot_id,sender_binding_generation,reply_to_message_id,message_type,body,status,version,idempotency_key,expires_at,created_at,updated_at,delivered_at,acknowledged_at,replied_at) VALUES(X'01921389d31174628f80cb5f79aa1b21',X'{TEAM_A}',X'{THREAD}','ROLE',X'{ROLE_A}',NULL,X'{ROLE_A}',X'{BINDING_A}',7,NULL,'DIRECT','x','DRAFT',1,NULL,NULL,1,1,NULL,NULL,NULL)",
            "UPDATE messages SET status='QUEUED' WHERE id=X'01921389d31174628f80cb5f79aa1b21'",
        ),
    ] {
        let draft = draft_sql
            .replace("{TEAM_A}", TEAM_A)
            .replace("{THREAD}", THREAD)
            .replace("{ROLE_A}", ROLE_A)
            .replace("{BINDING_A}", BINDING_A);
        query(&draft).execute(&pool).await?;
        assert!(
            query(transition).execute(&pool).await.is_err(),
            "{name} unexpectedly succeeded"
        );
    }

    // A DRAFT Role message legitimately has no snapshot yet.
    query(&format!(
        "INSERT INTO messages (id,team_id,thread_id,sender_actor_type,from_role_id,from_human_id,to_role_id,sender_binding_snapshot_id,sender_binding_generation,reply_to_message_id,message_type,body,status,version,idempotency_key,expires_at,created_at,updated_at,delivered_at,acknowledged_at,replied_at) VALUES(X'01921389d31174628f80cb5f79aa1b22',X'{TEAM_A}',X'{THREAD}','ROLE',X'{ROLE_A}',NULL,X'{ROLE_A}',NULL,NULL,NULL,'DIRECT','draft','DRAFT',1,NULL,NULL,1,1,NULL,NULL,NULL)"
    ))
    .execute(&pool)
    .await?;

    // 0015 froze the matching rule for Events; a Message must not be looser.
    for (name, actor) in [("SYSTEM", "SYSTEM"), ("HUMAN", "HUMAN")] {
        assert!(
            query(&format!(
                "INSERT INTO messages (id,team_id,thread_id,sender_actor_type,from_role_id,from_human_id,to_role_id,sender_binding_snapshot_id,sender_binding_generation,reply_to_message_id,message_type,body,status,version,idempotency_key,expires_at,created_at,updated_at,delivered_at,acknowledged_at,replied_at) VALUES(X'01921389d31174628f80cb5f79aa1b40',X'{TEAM_A}',X'{THREAD}','{actor}',NULL,NULL,X'{ROLE_A}',X'{BINDING_A}',1,NULL,'DIRECT','x','DRAFT',1,NULL,NULL,1,1,NULL,NULL,NULL)"
            ))
            .execute(&pool)
            .await
            .is_err(),
            "{name} sender carried a Role-Binding snapshot"
        );
    }

    // A SYSTEM sender with no snapshot remains legal: create it as DRAFT and
    // take the legal DRAFT -> QUEUED transition.
    query(&format!(
        "INSERT INTO messages (id,team_id,thread_id,sender_actor_type,from_role_id,from_human_id,to_role_id,sender_binding_snapshot_id,sender_binding_generation,reply_to_message_id,message_type,body,status,version,idempotency_key,expires_at,created_at,updated_at,delivered_at,acknowledged_at,replied_at) VALUES(X'01921389d31174628f80cb5f79aa1b41',X'{TEAM_A}',X'{THREAD}','SYSTEM',NULL,NULL,X'{ROLE_A}',NULL,NULL,NULL,'DIRECT','sys','DRAFT',1,NULL,NULL,1,1,NULL,NULL,NULL)"
    ))
    .execute(&pool)
    .await?;
    query("UPDATE messages SET status='QUEUED' WHERE id=X'01921389d31174628f80cb5f79aa1b41'")
        .execute(&pool)
        .await?;
    Ok(())
}

#[tokio::test]
async fn message_addressing_and_idempotency_survive_every_update_path() -> Result<(), Box<dyn Error>>
{
    let pool = database().await?;
    seed(&pool).await?;
    seeded_message(&pool, MESSAGE, ROLE_A, "DELIVERED", "idem-a").await?;

    for (name, sql) in [
        (
            "retarget a delivered message",
            format!("UPDATE messages SET to_role_id=X'{ROLE_B}' WHERE id=X'{MESSAGE}'"),
        ),
        (
            "rewrite idempotency_key",
            "UPDATE messages SET idempotency_key='idem-forged'".to_owned(),
        ),
        (
            "rewrite the sender Binding snapshot",
            format!(
                "UPDATE messages SET sender_binding_snapshot_id=X'{BINDING_B}' WHERE id=X'{MESSAGE}'"
            ),
        ),
        (
            "rewrite message_type",
            "UPDATE messages SET message_type='REPORT'".to_owned(),
        ),
        (
            "move the message to another Thread",
            format!("UPDATE messages SET thread_id=X'{TEAM_B}' WHERE id=X'{MESSAGE}'"),
        ),
    ] {
        assert!(
            query(&sql).execute(&pool).await.is_err(),
            "{name} unexpectedly succeeded"
        );
    }

    let row: (String, String) =
        sqlx_core::query_as::query_as("SELECT hex(to_role_id), idempotency_key FROM messages")
            .fetch_one(&pool)
            .await?;
    assert_eq!(row.0.to_uppercase(), ROLE_A.to_uppercase());
    assert_eq!(row.1, "idem-a");
    Ok(())
}

#[tokio::test]
async fn message_idempotency_survives_insert_or_replace() -> Result<(), Box<dyn Error>> {
    let pool = database().await?;
    seed(&pool).await?;
    seeded_message(&pool, MESSAGE, ROLE_A, "QUEUED", "idem-a").await?;

    let forged = message(
        "01921389d31174628f80cb5f79aa1b30",
        TEAM_A,
        THREAD,
        ROLE_A,
        "QUEUED",
        "idem-a",
    )
    .replace("INSERT INTO messages", "INSERT OR REPLACE INTO messages");
    assert!(
        query(&forged).execute(&pool).await.is_err(),
        "OR REPLACE reused an idempotency key"
    );

    let (body,): (String,) =
        sqlx_core::query_as::query_as("SELECT body FROM messages WHERE idempotency_key='idem-a'")
            .fetch_one(&pool)
            .await?;
    assert_eq!(body, "body");
    assert_eq!(
        query_scalar::<_, i64>("SELECT COUNT(*) FROM messages")
            .fetch_one(&pool)
            .await?,
        1
    );
    Ok(())
}

#[tokio::test]
async fn message_state_machine_matches_the_frozen_transition_table() -> Result<(), Box<dyn Error>> {
    let pool = database().await?;
    seed(&pool).await?;
    seeded_message(&pool, MESSAGE, ROLE_A, "DELIVERED", "idem-a").await?;

    for to in [
        "DRAFT",
        "QUEUED",
        "ROUTED",
        "FAILED",
        "EXPIRED",
        "REPLIED",
        "NEEDS_HUMAN",
    ] {
        assert!(
            query(&format!(
                "UPDATE messages SET status='{to}' WHERE id=X'{MESSAGE}'"
            ))
            .execute(&pool)
            .await
            .is_err(),
            "DELIVERED -> {to} unexpectedly succeeded"
        );
    }
    query("UPDATE messages SET status='ACKNOWLEDGED'")
        .execute(&pool)
        .await?;
    assert_eq!(
        query_scalar::<_, String>("SELECT status FROM messages")
            .fetch_one(&pool)
            .await?,
        "ACKNOWLEDGED"
    );
    Ok(())
}

#[tokio::test]
async fn expired_message_is_never_reopened() -> Result<(), Box<dyn Error>> {
    let pool = database().await?;
    seed(&pool).await?;
    seeded_message(&pool, MESSAGE, ROLE_A, "QUEUED", "idem-a").await?;
    query("UPDATE messages SET status='EXPIRED'")
        .execute(&pool)
        .await?;
    for to in ["QUEUED", "DELIVERED", "DRAFT", "REPLIED"] {
        assert!(
            query(&format!(
                "UPDATE messages SET status='{to}' WHERE id=X'{MESSAGE}'"
            ))
            .execute(&pool)
            .await
            .is_err(),
            "EXPIRED -> {to} unexpectedly succeeded"
        );
    }
    Ok(())
}

#[tokio::test]
async fn delivery_attempt_history_and_outcome_are_immutable() -> Result<(), Box<dyn Error>> {
    let pool = database().await?;
    seed(&pool).await?;
    seeded_message(&pool, MESSAGE, ROLE_A, "ROUTED", "idem-a").await?;
    seeded_attempt(&pool, ATTEMPT, MESSAGE, "SUCCEEDED").await?;

    for (name, sql) in [
        (
            "flip a completed outcome",
            format!("UPDATE delivery_attempts SET status='FAILED' WHERE id=X'{ATTEMPT}'"),
        ),
        (
            "rewrite attempt_number",
            "UPDATE delivery_attempts SET attempt_number=99".to_owned(),
        ),
        (
            "redirect target_role_id",
            format!(
                "UPDATE delivery_attempts SET target_role_id=X'{ROLE_B}' WHERE id=X'{ATTEMPT}'"
            ),
        ),
        (
            "rewrite the target Binding snapshot",
            format!(
                "UPDATE delivery_attempts SET target_binding_id=X'{BINDING_B}' WHERE id=X'{ATTEMPT}'"
            ),
        ),
        (
            "rewrite the recorded completion time",
            "UPDATE delivery_attempts SET completed_at=999".to_owned(),
        ),
    ] {
        assert!(
            query(&sql).execute(&pool).await.is_err(),
            "{name} unexpectedly succeeded"
        );
    }

    let row: (String, i64, String) = sqlx_core::query_as::query_as(
        "SELECT status, attempt_number, hex(target_role_id) FROM delivery_attempts",
    )
    .fetch_one(&pool)
    .await?;
    assert_eq!(row.0, "SUCCEEDED");
    assert_eq!(row.1, 1);
    assert_eq!(row.2.to_uppercase(), ROLE_A.to_uppercase());
    Ok(())
}

#[tokio::test]
async fn dispatcher_attempt_transitions_remain_available() -> Result<(), Box<dyn Error>> {
    let pool = database().await?;
    seed(&pool).await?;
    seeded_message(&pool, MESSAGE, ROLE_A, "ROUTED", "idem-a").await?;
    seeded_attempt(&pool, ATTEMPT, MESSAGE, "PENDING").await?;

    for status in ["IN_FLIGHT", "SUCCEEDED"] {
        query(&format!("UPDATE delivery_attempts SET status='{status}'"))
            .execute(&pool)
            .await?;
    }
    assert_eq!(
        query_scalar::<_, String>("SELECT status FROM delivery_attempts")
            .fetch_one(&pool)
            .await?,
        "SUCCEEDED"
    );

    // A failed cycle is retryable on the Message, but only through a NEW attempt.
    query("UPDATE messages SET status='FAILED'")
        .execute(&pool)
        .await?;
    query("UPDATE messages SET status='QUEUED'")
        .execute(&pool)
        .await?;
    Ok(())
}

#[tokio::test]
async fn dirty_message_history_is_rejected_atomically() -> Result<(), Box<dyn Error>> {
    let pool = v16_database().await?;
    seed(&pool).await?;
    query(&format!(
        "INSERT INTO threads VALUES(X'01921389d31174628f80cb5f79aa1b11',X'{TEAM_B}','thread-b','OPEN',1,1,1)"
    ))
    .execute(&pool)
    .await?;
    // A cross-Team message that 0007 accepted and 0017 must refuse to inherit.
    query(&format!(
        "INSERT INTO messages (id,team_id,thread_id,sender_actor_type,from_role_id,from_human_id,to_role_id,sender_binding_snapshot_id,sender_binding_generation,reply_to_message_id,message_type,body,status,version,idempotency_key,expires_at,created_at,updated_at,delivered_at,acknowledged_at,replied_at) VALUES(X'01921389d31174628f80cb5f79aa1b12',X'{TEAM_A}',X'01921389d31174628f80cb5f79aa1b11','ROLE',X'{ROLE_A}',NULL,X'{ROLE_B}',NULL,NULL,NULL,'DIRECT','x','DRAFT',1,NULL,NULL,1,1,NULL,NULL,NULL)"
    ))
    .execute(&pool)
    .await?;

    assert!(
        migrate(&pool).await.is_err(),
        "dirty v16 history upgraded silently"
    );

    let guards: i64 = query_scalar(
        "SELECT COUNT(*) FROM sqlite_master WHERE type='trigger' AND name='messages_team_consistency_insert'",
    )
    .fetch_one(&pool)
    .await?;
    assert_eq!(guards, 0, "0017 guards installed despite rejected upgrade");
    Ok(())
}

// ---------------------------------------------------------------------------
// MM-R01 (DA-01): upgrade path of `0025_message_routing_team_integrity`.
// ---------------------------------------------------------------------------

/// The published chain 0001..0024, i.e. everything `0025` may build on.  The
/// defect `0025` repairs is only reachable through this schema, so a 0001..0016
/// baseline would not exercise it at all.
const V24: [(&str, &str); 24] = [
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
    (
        "command idempotency",
        include_str!("../migrations/0022_command_idempotency.sql"),
    ),
    (
        "notification execution integrity",
        include_str!("../migrations/0023_notification_execution_integrity.sql"),
    ),
    (
        "outbox retry default",
        include_str!("../migrations/0024_outbox_retry_default.sql"),
    ),
];

#[derive(Debug)]
struct V24Migrations;

impl MigrationSource<'static> for V24Migrations {
    fn resolve(
        self,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<Migration>, BoxDynError>> + Send + 'static>> {
        Box::pin(async {
            Ok(V24
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

async fn v24_database() -> Result<SqlitePool, Box<dyn Error>> {
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(SqliteConnectOptions::from_str("sqlite::memory:")?.foreign_keys(true))
        .await?;
    Migrator::new(V24Migrations).await?.run(&pool).await?;
    seed(&pool).await?;
    Ok(pool)
}

const NEW_MESSAGES: [&str; 6] = [
    "messages_team_consistency_update",
    "messages_non_role_snapshot_rejected_update",
    "messages_sender_binding_snapshot_update",
    "delivery_attempts_target_role_team_insert",
    "delivery_attempts_target_role_team_update",
    "delivery_attempts_target_binding_update",
];

/// Proves the baseline round really lacks the new guards and the full round has
/// them.  Without this assertion the tests below could pass because nothing was
/// ever installed -- the exact failure mode the adversarial playbook records for
/// a device that does not declare which schema it loaded.
#[tokio::test]
async fn the_0025_guards_exist_only_after_the_migration() -> Result<(), Box<dyn Error>> {
    let baseline = v24_database().await?;
    for name in NEW_MESSAGES {
        assert_eq!(
            query_scalar::<_, i64>(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='trigger' AND name=?",
            )
            .bind(name)
            .fetch_one(&baseline)
            .await?,
            0,
            "the 0001..0024 baseline already contained {name}"
        );
    }

    let upgraded = v24_database().await?;
    migrate(&upgraded).await?;
    for name in NEW_MESSAGES {
        assert_eq!(
            query_scalar::<_, i64>(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='trigger' AND name=?",
            )
            .bind(name)
            .fetch_one(&upgraded)
            .await?,
            1,
            "{name} missing after the upgrade"
        );
    }
    Ok(())
}

/// The core of the defect, shown on the pre-`0025` schema: the DRAFT hole is
/// genuinely open there, and the device that reports it is a real one.
#[tokio::test]
async fn the_pre_0025_baseline_really_admits_the_cross_team_draft_retarget()
-> Result<(), Box<dyn Error>> {
    let pool = v24_database().await?;
    seeded_message(&pool, MESSAGE, ROLE_A, "DRAFT", "baseline").await?;
    let foreign_role = "01921389d31174628f80cb5f79aa1c03";
    for statement in [format!(
        "INSERT INTO roles VALUES(X'{foreign_role}',X'{TEAM_B}','role-b2','role','ACTIVE',1,1,1,NULL)"
    )] {
        query(&statement).execute(&pool).await?;
    }
    assert!(
        query(&format!("UPDATE messages SET to_role_id=X'{foreign_role}'"))
            .execute(&pool)
            .await
            .is_ok(),
        "the 0001..0024 baseline unexpectedly rejected the cross-Team retarget; \
         the upgrade test below would then prove nothing"
    );
    // And the misroute really does complete.  The attempt carries no Binding
    // snapshot on purpose: `0017` already refuses a Binding that does not name
    // the target Role, so a Binding here would be rejected for an unrelated
    // reason and the test would pass without proving anything about Team
    // consistency.  A routing-only attempt is exactly what the router writes
    // before it resolves the target's ACTIVE Binding.
    query("UPDATE messages SET status='QUEUED'")
        .execute(&pool)
        .await?;
    query("UPDATE messages SET status='ROUTED'")
        .execute(&pool)
        .await?;
    query(&format!(
        "INSERT INTO delivery_attempts (id,team_id,message_id,attempt_number,target_role_id,target_binding_id,target_binding_generation,status,created_at) VALUES(X'01921389d31174628f80cb5f79aa1c04',X'{TEAM_A}',X'{MESSAGE}',1,X'{foreign_role}',NULL,NULL,'PENDING',1)"
    ))
    .execute(&pool)
    .await?;
    assert_eq!(
        query_scalar::<_, i64>("SELECT COUNT(*) FROM delivery_attempts")
            .fetch_one(&pool)
            .await?,
        1,
        "the baseline did not actually produce the misrouted attempt"
    );
    Ok(())
}

/// A database carrying the DA-01 damage is rejected atomically, no guard is
/// installed, and the diagnostic survives the rejection.
#[tokio::test]
async fn a_misrouted_delivery_attempt_is_rejected_before_0025_installs_guards()
-> Result<(), Box<dyn Error>> {
    let pool = v24_database().await?;
    let foreign_role = "01921389d31174628f80cb5f79aa1c05";
    query(&format!(
        "INSERT INTO roles VALUES(X'{foreign_role}',X'{TEAM_B}','role-b3','role','ACTIVE',1,1,1,NULL)"
    ))
    .execute(&pool)
    .await?;
    seeded_message(&pool, MESSAGE, ROLE_A, "DRAFT", "dirty-a").await?;

    // The completed DA-01 damage: the DRAFT was retargeted to Team B's Role and
    // then committed, so both the message and its attempt sit on a foreign
    // route.  `0017` accepts this because the attempt matches the message.
    //
    // Note on the attempt clause of the 0025 upgrade guard: it is defence in
    // depth.  `0017` already requires
    //   message.team_id = attempt.team_id AND message.to_role_id = attempt.target_role_id,
    // so on a 0001..0024 baseline a cross-Team attempt target is reachable only
    // together with the cross-Team message that produced it, and the message
    // clause of the guard is what fires.  The attempt clause cannot be shown to
    // fire *independently* from that schema, and this test does not claim it
    // does; the live INSERT/UPDATE behaviour of the attempt guards is asserted
    // in `message_routing_team_integrity.rs`.
    query(&format!("UPDATE messages SET to_role_id=X'{foreign_role}'"))
        .execute(&pool)
        .await?;
    query("UPDATE messages SET status='QUEUED'")
        .execute(&pool)
        .await?;
    query("UPDATE messages SET status='ROUTED'")
        .execute(&pool)
        .await?;
    query(&format!(
        "INSERT INTO delivery_attempts (id,team_id,message_id,attempt_number,target_role_id,target_binding_id,target_binding_generation,status,created_at) VALUES(X'01921389d31174628f80cb5f79aa1c06',X'{TEAM_A}',X'{MESSAGE}',1,X'{foreign_role}',NULL,NULL,'PENDING',1)"
    ))
    .execute(&pool)
    .await?;
    assert_eq!(
        query_scalar::<_, i64>("SELECT COUNT(*) FROM delivery_attempts")
            .fetch_one(&pool)
            .await?,
        1
    );

    let error = migrate(&pool)
        .await
        .expect_err("a misrouted delivery attempt upgraded silently");
    // The diagnostic has to remain visible: an operator must be able to tell
    // *why* the upgrade stopped, not just that it did.
    let rendered = format!("{error}");
    assert!(
        rendered.contains("message_routing_team_upgrade_guard"),
        "the rejection carried no identifying diagnostic: {rendered}"
    );

    for name in NEW_MESSAGES {
        assert_eq!(
            query_scalar::<_, i64>(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='trigger' AND name=?",
            )
            .bind(name)
            .fetch_one(&pool)
            .await?,
            0,
            "{name} installed despite the rejected upgrade"
        );
    }
    // No Team was invented and no row was silently repaired.
    assert_eq!(
        query_scalar::<_, String>("SELECT hex(target_role_id) FROM delivery_attempts")
            .fetch_one(&pool)
            .await?
            .to_uppercase(),
        foreign_role.to_uppercase()
    );
    Ok(())
}

/// A HUMAN or SYSTEM sender holding a Role-Binding snapshot is the second row
/// shape `0025` refuses to inherit.  `0017` rejected it on INSERT only, and a
/// DRAFT could still acquire one.
#[tokio::test]
async fn a_non_role_sender_snapshot_is_rejected_before_0025_installs_guards()
-> Result<(), Box<dyn Error>> {
    let pool = v24_database().await?;
    seeded_message(&pool, MESSAGE, ROLE_A, "DRAFT", "dirty-b").await?;
    assert!(
        query(&format!(
            "UPDATE messages SET sender_actor_type='HUMAN', sender_binding_snapshot_id=X'{BINDING_A}', sender_binding_generation=1"
        ))
        .execute(&pool)
        .await
        .is_ok(),
        "the baseline did not admit the snapshot on a HUMAN sender; \
         this upgrade test would then prove nothing"
    );

    let error = migrate(&pool)
        .await
        .expect_err("a HUMAN sender carrying a Role-Binding snapshot upgraded silently");
    assert!(
        format!("{error}").contains("message_routing_team_upgrade_guard"),
        "the rejection carried no identifying diagnostic: {error}"
    );
    assert_eq!(
        query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM sqlite_master WHERE type='trigger' AND name='messages_non_role_snapshot_rejected_update'",
        )
        .fetch_one(&pool)
        .await?,
        0,
        "0025 guards installed despite the rejected upgrade"
    );
    Ok(())
}

/// The legal upgrade: a clean 0001..0024 database moves to the full chain and
/// keeps its rows.  This is the positive control for the three rejections above.
#[tokio::test]
async fn a_clean_v24_history_upgrades_without_rewriting_anything() -> Result<(), Box<dyn Error>> {
    let pool = v24_database().await?;
    seeded_message(&pool, MESSAGE, ROLE_A, "DELIVERED", "clean-a").await?;
    // The existing attempt is finished, so `delivery_attempts_single_active`
    // (one PENDING/IN_FLIGHT attempt per message) still leaves room for the
    // second one this test creates after the upgrade.
    seeded_attempt(&pool, ATTEMPT, MESSAGE, "SUCCEEDED").await?;
    let before: (String, String, i64) = sqlx_core::query_as::query_as(
        "SELECT status, idempotency_key, version FROM messages WHERE id=X'01921389d31174628f80cb5f79aa1b02'",
    )
    .fetch_one(&pool)
    .await?;

    migrate(&pool).await?;

    let after: (String, String, i64) = sqlx_core::query_as::query_as(
        "SELECT status, idempotency_key, version FROM messages WHERE id=X'01921389d31174628f80cb5f79aa1b02'",
    )
    .fetch_one(&pool)
    .await?;
    assert_eq!(before, after, "the upgrade rewrote an existing message");
    assert_eq!(
        query_scalar::<_, i64>("SELECT COUNT(*) FROM delivery_attempts")
            .fetch_one(&pool)
            .await?,
        1,
        "the upgrade dropped the existing attempt"
    );
    assert_eq!(
        query_scalar::<_, i64>("SELECT COUNT(*) FROM pragma_foreign_key_check")
            .fetch_one(&pool)
            .await?,
        0
    );
    // The upgraded database still routes normally.
    query(&format!(
        "INSERT INTO delivery_attempts (id,team_id,message_id,attempt_number,target_role_id,target_binding_id,target_binding_generation,status,created_at) VALUES(X'01921389d31174628f80cb5f79aa1c07',X'{TEAM_A}',X'{MESSAGE}',2,X'{ROLE_A}',X'{BINDING_A}',1,'PENDING',1)"
    ))
    .execute(&pool)
    .await?;
    Ok(())
}
