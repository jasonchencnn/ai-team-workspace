use std::error::Error;
use std::str::FromStr;

use persistence_sqlite::migrate;
use sqlx_core::{query::query, query_scalar::query_scalar};
use sqlx_sqlite::{SqliteConnectOptions, SqlitePool, SqlitePoolOptions};

async fn database() -> Result<SqlitePool, Box<dyn Error>> {
    let options = SqliteConnectOptions::from_str("sqlite::memory:")?.foreign_keys(true);
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(options)
        .await?;
    migrate(&pool).await?;
    Ok(pool)
}

async fn execute_expect_error(pool: &SqlitePool, sql: &str) {
    assert!(
        query(sql).execute(pool).await.is_err(),
        "statement unexpectedly succeeded: {sql}"
    );
}

fn insert_team(id: &str) -> String {
    format!(
        "INSERT INTO teams (id, name, status, version, created_at, updated_at, archived_at) VALUES (X'{id}', 'Core Team', 'ACTIVE', 1, 1726300123456, 1726300123456, NULL);"
    )
}

fn insert_role(team_id: &str, role_id: &str, display_id: &str) -> String {
    format!(
        "INSERT INTO roles (id, team_id, display_id, name, status, version, created_at, updated_at, deactivated_at) VALUES (X'{role_id}', X'{team_id}', '{display_id}', 'Developer', 'ACTIVE', 1, 1726300123456, 1726300123456, NULL);"
    )
}

fn insert_task(team_id: &str, role_id: &str, task_id: &str, display_id: &str) -> String {
    format!(
        "INSERT INTO tasks (id, team_id, owner_role_id, display_id, title, description, status, version, created_at, updated_at, cancelled_at, completed_at) VALUES (X'{task_id}', X'{team_id}', X'{role_id}', '{display_id}', 'Title', 'Desc', 'DOING', 1, 1726300123456, 1726300123456, NULL, NULL);"
    )
}

const ACTOR_HARNESS_ID: &str = "01921389d31174628f80cb5f79aa8003";
const ACTOR_ADAPTER_ID: &str = "01921389d31174628f80cb5f79aa8004";
const ACTOR_BINDING_ID: &str = "01921389d31174628f80cb5f79aa8005";

async fn insert_role_actor_binding(
    pool: &SqlitePool,
    team_id: &str,
    role_id: &str,
) -> Result<(), Box<dyn Error>> {
    for statement in [
        format!(
            "INSERT INTO harness_instances VALUES(X'{ACTOR_HARNESS_ID}',X'{team_id}','mock','Harness','ACTIVE','[]',NULL,1,1,1,NULL)"
        ),
        format!(
            "INSERT INTO adapters VALUES(X'{ACTOR_ADAPTER_ID}',X'{team_id}','adapter','1','/adapter','{{}}',1,1,1)"
        ),
        format!(
            "INSERT INTO bindings VALUES(X'{ACTOR_BINDING_ID}',X'{team_id}',X'{role_id}',X'{ACTOR_HARNESS_ID}',X'{ACTOR_ADAPTER_ID}','ACTIVE',1,1,'{{}}',1,1,1,NULL)"
        ),
    ] {
        query(&statement).execute(pool).await?;
    }
    Ok(())
}

struct EventRow<'a> {
    id: &'a str,
    team_id: &'a str,
    event_type: &'a str,
    actor_type: &'a str,
    actor_id: Option<&'a str>,
    subject_type: &'a str,
    subject_id: &'a str,
    request_id: Option<&'a str>,
    object_version: u64,
    payload: &'a str,
    metadata: &'a str,
    occurred_at: i64,
    previous_hash: Option<&'a str>,
    hash: &'a str,
}

fn insert_event(row: EventRow<'_>) -> String {
    let actor_str = match row.actor_id {
        Some(id) => format!("X'{id}'"),
        None => "NULL".to_string(),
    };
    let req_str = match row.request_id {
        Some(id) => format!("X'{id}'"),
        None => "NULL".to_string(),
    };
    let prev_hash_str = match row.previous_hash {
        Some(h) => format!("'{h}'"),
        None => "NULL".to_string(),
    };

    let actor_snapshot = if row.actor_type == "ROLE" {
        format!(
            "'ROLE_BINDING', X'{actor}', X'{ACTOR_BINDING_ID}', 1, X'{ACTOR_HARNESS_ID}'",
            actor = row.actor_id.expect("ROLE Event fixtures require actor_id")
        )
    } else {
        "NULL, NULL, NULL, NULL, NULL".to_owned()
    };

    format!(
        "INSERT INTO events (id, team_id, event_type, actor_type, actor_id, actor_kind, actor_role_id, actor_binding_id, actor_binding_version, actor_harness_instance_id, subject_type, subject_id, request_id, object_version, payload, metadata, occurred_at, previous_hash, hash) VALUES (X'{}', X'{}', '{}', '{}', {}, {}, '{}', X'{}', {}, {}, '{}', '{}', {}, {}, '{}');",
        row.id,
        row.team_id,
        row.event_type,
        row.actor_type,
        actor_str,
        actor_snapshot,
        row.subject_type,
        row.subject_id,
        req_str,
        row.object_version,
        row.payload,
        row.metadata,
        row.occurred_at,
        prev_hash_str,
        row.hash
    )
}

struct OutboxRow<'a> {
    id: &'a str,
    team_id: &'a str,
    event_id: &'a str,
    destination: &'a str,
    payload: &'a str,
    status: &'a str,
    retry_count: i64,
    max_retries: i64,
    last_error: Option<&'a str>,
    idempotency_key: Option<&'a str>,
    created_at: i64,
    scheduled_at: i64,
    published_at: Option<i64>,
}

fn insert_outbox(row: OutboxRow<'_>) -> String {
    let err_str = match row.last_error {
        Some(e) => format!("'{e}'"),
        None => "NULL".to_string(),
    };
    let ikey_str = match row.idempotency_key {
        Some(k) => format!("'{k}'"),
        None => "NULL".to_string(),
    };
    let pub_str = match row.published_at {
        Some(p) => format!("{p}"),
        None => "NULL".to_string(),
    };

    format!(
        "INSERT INTO outbox (id, team_id, event_id, destination, payload, status, retry_count, max_retries, last_error, idempotency_key, created_at, scheduled_at, published_at) VALUES (X'{}', X'{}', X'{}', '{}', '{}', '{}', {}, {}, {}, {}, {}, {}, {});",
        row.id,
        row.team_id,
        row.event_id,
        row.destination,
        row.payload,
        row.status,
        row.retry_count,
        row.max_retries,
        err_str,
        ikey_str,
        row.created_at,
        row.scheduled_at,
        pub_str
    )
}

#[tokio::test]
async fn empty_database_upgrades_to_event_and_outbox_schema() -> Result<(), Box<dyn Error>> {
    let pool = database().await?;

    // Migration replay is idempotent
    migrate(&pool).await?;

    let tables: String = query_scalar(
        "SELECT group_concat(name, ',') FROM (SELECT name FROM sqlite_master WHERE type = 'table' AND name IN ('events', 'outbox', 'event_consumptions') ORDER BY name);",
    )
    .fetch_one(&pool)
    .await?;

    assert_eq!(tables, "event_consumptions,events,outbox");
    Ok(())
}

#[tokio::test]
async fn migration_enforces_event_append_only_and_invariants() -> Result<(), Box<dyn Error>> {
    let pool = database().await?;

    let team_id = "01921389d31174628f80cb5f79aa8001";
    let role_id = "01921389d31174628f80cb5f79aa8002";
    let task_id = "01921389d31174628f80cb5f79aa8006";
    let event1_id = "01921389d31174628f80cb5f79aa8051";

    query(&insert_team(team_id)).execute(&pool).await?;
    query(&insert_role(team_id, role_id, "ROLE-01"))
        .execute(&pool)
        .await?;
    query(&insert_task(team_id, role_id, task_id, "TASK-01"))
        .execute(&pool)
        .await?;
    insert_role_actor_binding(&pool, team_id, role_id).await?;

    let valid_hash = "E3B0C44298FC1C149AFBF4C8996FB92427AE41E4649B934CA495991B7852B855";

    // Valid event insert succeeds
    query(&insert_event(EventRow {
        id: event1_id,
        team_id,
        event_type: "task.status_changed",
        actor_type: "ROLE",
        actor_id: Some(role_id),
        subject_type: "TASK",
        subject_id: task_id,
        request_id: None,
        object_version: 1,
        payload: r#"{"status":"DOING"}"#,
        metadata: "{}",
        occurred_at: 1726300123456,
        previous_hash: None,
        hash: valid_hash,
    }))
    .execute(&pool)
    .await?;

    // UPDATE on events MUST FAIL: append-only guarantee
    execute_expect_error(
        &pool,
        &format!("UPDATE events SET event_type = 'tampered' WHERE id = X'{event1_id}';"),
    )
    .await;

    // DELETE on events MUST FAIL: append-only guarantee
    execute_expect_error(
        &pool,
        &format!("DELETE FROM events WHERE id = X'{event1_id}';"),
    )
    .await;

    // Invalid actor_type MUST fail
    let event2_id = "01921389d31174628f80cb5f79aa8052";
    execute_expect_error(
        &pool,
        &insert_event(EventRow {
            id: event2_id,
            team_id,
            event_type: "task.created",
            actor_type: "INVALID_ACTOR",
            actor_id: Some(role_id),
            subject_type: "TASK",
            subject_id: task_id,
            request_id: None,
            object_version: 1,
            payload: "{}",
            metadata: "{}",
            occurred_at: 1726300123456,
            previous_hash: None,
            hash: valid_hash,
        }),
    )
    .await;

    Ok(())
}

#[tokio::test]
async fn migration_enforces_outbox_atomic_commit_and_deduplication() -> Result<(), Box<dyn Error>> {
    let pool = database().await?;

    let team_id = "01921389d31174628f80cb5f79aa8001";
    let role_id = "01921389d31174628f80cb5f79aa8002";
    let task_id = "01921389d31174628f80cb5f79aa8006";
    let event1_id = "01921389d31174628f80cb5f79aa8051";
    let outbox1_id = "01921389d31174628f80cb5f79aa8061";
    let valid_hash = "E3B0C44298FC1C149AFBF4C8996FB92427AE41E4649B934CA495991B7852B855";

    query(&insert_team(team_id)).execute(&pool).await?;
    query(&insert_role(team_id, role_id, "ROLE-01"))
        .execute(&pool)
        .await?;
    query(&insert_task(team_id, role_id, task_id, "TASK-01"))
        .execute(&pool)
        .await?;
    insert_role_actor_binding(&pool, team_id, role_id).await?;

    // Atomic transaction: task status update + event + outbox
    let mut tx = pool.begin().await?;

    query(&format!(
        "UPDATE tasks SET status = 'DONE', completed_at = 1726300200000, version = 2 WHERE id = X'{task_id}';"
    ))
    .execute(&mut *tx)
    .await?;

    query(&insert_event(EventRow {
        id: event1_id,
        team_id,
        event_type: "task.completed",
        actor_type: "ROLE",
        actor_id: Some(role_id),
        subject_type: "TASK",
        subject_id: task_id,
        request_id: None,
        object_version: 2,
        payload: r#"{"status":"DONE"}"#,
        metadata: "{}",
        occurred_at: 1726300200000,
        previous_hash: None,
        hash: valid_hash,
    }))
    .execute(&mut *tx)
    .await?;

    query(&insert_outbox(OutboxRow {
        id: outbox1_id,
        team_id,
        event_id: event1_id,
        destination: "READ_MODEL_PROJECTION",
        payload: r#"{"event":"task.completed","task_id":"TASK-01"}"#,
        status: "PENDING",
        retry_count: 0,
        max_retries: 5,
        last_error: None,
        idempotency_key: Some("idem-task-01-complete"),
        created_at: 1726300200000,
        scheduled_at: 1726300200000,
        published_at: None,
    }))
    .execute(&mut *tx)
    .await?;

    tx.commit().await?;

    // Verify task, event, and outbox all committed together
    let task_status: String = query_scalar(&format!(
        "SELECT status FROM tasks WHERE id = X'{task_id}';"
    ))
    .fetch_one(&pool)
    .await?;
    assert_eq!(task_status, "DONE");

    let event_count: i64 = query_scalar("SELECT COUNT(*) FROM events;")
        .fetch_one(&pool)
        .await?;
    assert_eq!(event_count, 1);

    let outbox_count: i64 = query_scalar("SELECT COUNT(*) FROM outbox;")
        .fetch_one(&pool)
        .await?;
    assert_eq!(outbox_count, 1);

    // Duplicate outbox for same event and same destination MUST fail (unique index)
    let outbox2_id = "01921389d31174628f80cb5f79aa8062";
    execute_expect_error(
        &pool,
        &insert_outbox(OutboxRow {
            id: outbox2_id,
            team_id,
            event_id: event1_id,
            destination: "READ_MODEL_PROJECTION",
            payload: r#"{"duplicate":true}"#,
            status: "PENDING",
            retry_count: 0,
            max_retries: 5,
            last_error: None,
            idempotency_key: None,
            created_at: 1726300200000,
            scheduled_at: 1726300200000,
            published_at: None,
        }),
    )
    .await;

    Ok(())
}

#[tokio::test]
async fn migration_enforces_consumption_deduplication_and_replay() -> Result<(), Box<dyn Error>> {
    let pool = database().await?;

    let team_id = "01921389d31174628f80cb5f79aa8001";
    let role_id = "01921389d31174628f80cb5f79aa8002";
    let task_id = "01921389d31174628f80cb5f79aa8006";
    let event1_id = "01921389d31174628f80cb5f79aa8051";
    let valid_hash = "E3B0C44298FC1C149AFBF4C8996FB92427AE41E4649B934CA495991B7852B855";

    query(&insert_team(team_id)).execute(&pool).await?;
    query(&insert_role(team_id, role_id, "ROLE-01"))
        .execute(&pool)
        .await?;
    query(&insert_task(team_id, role_id, task_id, "TASK-01"))
        .execute(&pool)
        .await?;
    insert_role_actor_binding(&pool, team_id, role_id).await?;

    query(&insert_event(EventRow {
        id: event1_id,
        team_id,
        event_type: "task.created",
        actor_type: "ROLE",
        actor_id: Some(role_id),
        subject_type: "TASK",
        subject_id: task_id,
        request_id: None,
        object_version: 1,
        payload: "{}",
        metadata: "{}",
        occurred_at: 1726300123456,
        previous_hash: None,
        hash: valid_hash,
    }))
    .execute(&pool)
    .await?;

    // First consumption succeeds
    query(&format!(
        "INSERT INTO event_consumptions (consumer_id, event_id, team_id, consumed_at) VALUES ('activity_feed_projector', X'{event1_id}', X'{team_id}', 1726300125000);"
    ))
    .execute(&pool)
    .await?;

    // Duplicate consumption for same consumer and event MUST fail (deduplication)
    execute_expect_error(
        &pool,
        &format!(
            "INSERT INTO event_consumptions (consumer_id, event_id, team_id, consumed_at) VALUES ('activity_feed_projector', X'{event1_id}', X'{team_id}', 1726300126000);"
        ),
    )
    .await;

    // Different consumer on same event succeeds
    query(&format!(
        "INSERT INTO event_consumptions (consumer_id, event_id, team_id, consumed_at) VALUES ('notification_engine', X'{event1_id}', X'{team_id}', 1726300127000);"
    ))
    .execute(&pool)
    .await?;

    // Deleting consumer consumption record allows replay
    query(&format!(
        "DELETE FROM event_consumptions WHERE consumer_id = 'activity_feed_projector' AND event_id = X'{event1_id}';"
    ))
    .execute(&pool)
    .await?;

    // Replay consumption now succeeds
    query(&format!(
        "INSERT INTO event_consumptions (consumer_id, event_id, team_id, consumed_at) VALUES ('activity_feed_projector', X'{event1_id}', X'{team_id}', 1726300130000);"
    ))
    .execute(&pool)
    .await?;

    Ok(())
}
