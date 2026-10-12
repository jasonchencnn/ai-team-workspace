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

fn insert_policy(id: &str, team_id: &str, name: &str, event_type: &str) -> String {
    format!(
        "INSERT INTO notification_policies (id, team_id, name, event_type, channel, severity, dedupe_window_seconds, ttl_seconds, status, version, created_at, updated_at) VALUES (X'{id}', X'{team_id}', '{name}', '{event_type}', 'IN_APP', 'INFO', 300, 86400, 'ACTIVE', 1, 1726300123456, 1726300123456);"
    )
}

struct NotificationRow<'a> {
    id: &'a str,
    team_id: &'a str,
    recipient_human_id: &'a str,
    title: &'a str,
    body: &'a str,
    status: &'a str,
    dedupe_key: Option<&'a str>,
}

fn insert_notification(row: NotificationRow<'_>) -> String {
    let key_str = match row.dedupe_key {
        Some(k) => format!("'{k}'"),
        None => "NULL".to_string(),
    };

    format!(
        "INSERT INTO notifications (id, team_id, recipient_human_id, source_event_id, source_message_id, policy_id, severity, title, body, status, dedupe_key, version, expires_at, created_at, read_at, dismissed_at) VALUES (X'{}', X'{}', X'{}', NULL, NULL, NULL, 'INFO', '{}', '{}', '{}', {}, 1, NULL, 1726300123456, NULL, NULL);",
        row.id, row.team_id, row.recipient_human_id, row.title, row.body, row.status, key_str
    )
}

struct AttentionRow<'a> {
    id: &'a str,
    team_id: &'a str,
    attention_type: &'a str,
    source_type: &'a str,
    source_id: &'a str,
    title: &'a str,
    summary: &'a str,
    status: &'a str,
    dismiss_reason: Option<&'a str>,
    resolved_at: Option<i64>,
    dismissed_at: Option<i64>,
}

fn insert_attention(row: AttentionRow<'_>) -> String {
    let reason_str = match row.dismiss_reason {
        Some(r) => format!("'{r}'"),
        None => "NULL".to_string(),
    };
    let res_str = match row.resolved_at {
        Some(t) => format!("{t}"),
        None => "NULL".to_string(),
    };
    let dis_str = match row.dismissed_at {
        Some(t) => format!("{t}"),
        None => "NULL".to_string(),
    };

    format!(
        "INSERT INTO attention_items (id, team_id, attention_type, source_type, source_id, title, summary, status, dismiss_reason, version, created_at, updated_at, resolved_at, dismissed_at) VALUES (X'{}', X'{}', '{}', '{}', X'{}', '{}', '{}', '{}', {}, 1, 1726300123456, 1726300123456, {}, {});",
        row.id,
        row.team_id,
        row.attention_type,
        row.source_type,
        row.source_id,
        row.title,
        row.summary,
        row.status,
        reason_str,
        res_str,
        dis_str
    )
}

#[tokio::test]
async fn empty_database_upgrades_to_notification_attention_schema() -> Result<(), Box<dyn Error>> {
    let pool = database().await?;

    // Migration replay is idempotent
    migrate(&pool).await?;

    let tables: String = query_scalar(
        "SELECT group_concat(name, ',') FROM (SELECT name FROM sqlite_master WHERE type = 'table' AND name IN ('notification_policies', 'notifications', 'attention_items') ORDER BY name);",
    )
    .fetch_one(&pool)
    .await?;

    assert_eq!(
        tables,
        "attention_items,notification_policies,notifications"
    );
    Ok(())
}

#[tokio::test]
async fn migration_enforces_notification_human_recipient_and_deduplication()
-> Result<(), Box<dyn Error>> {
    let pool = database().await?;

    let team_id = "01921389d31174628f80cb5f79aa8001";
    let policy_id = "01921389d31174628f80cb5f79aa8002";
    let human_id = "01921389d31174628f80cb5f79aa8003";
    let notif1_id = "01921389d31174628f80cb5f79aa80a1";
    let notif2_id = "01921389d31174628f80cb5f79aa80a2";
    let notif3_id = "01921389d31174628f80cb5f79aa80a3";

    query(&insert_team(team_id)).execute(&pool).await?;
    query(&insert_policy(
        policy_id,
        team_id,
        "Run Failures",
        "run.failed",
    ))
    .execute(&pool)
    .await?;

    // First notification insert succeeds
    query(&insert_notification(NotificationRow {
        id: notif1_id,
        team_id,
        recipient_human_id: human_id,
        title: "Task Alert",
        body: "Run failed on task 1",
        status: "UNREAD",
        dedupe_key: Some("run-failed-task-1"),
    }))
    .execute(&pool)
    .await?;

    // Second notification with same dedupe_key while first is UNREAD MUST FAIL
    execute_expect_error(
        &pool,
        &insert_notification(NotificationRow {
            id: notif2_id,
            team_id,
            recipient_human_id: human_id,
            title: "Task Alert Duplicate",
            body: "Run failed on task 1 duplicate",
            status: "UNREAD",
            dedupe_key: Some("run-failed-task-1"),
        }),
    )
    .await;

    // Mark first notification as DISMISSED
    query(&format!(
        "UPDATE notifications SET status = 'DISMISSED', dismissed_at = 1726300125000 WHERE id = X'{notif1_id}';"
    ))
    .execute(&pool)
    .await?;

    // Now a new notification with the same dedupe_key succeeds
    query(&insert_notification(NotificationRow {
        id: notif3_id,
        team_id,
        recipient_human_id: human_id,
        title: "Task Alert Re-alert",
        body: "Run failed on task 1 new attempt",
        status: "UNREAD",
        dedupe_key: Some("run-failed-task-1"),
    }))
    .execute(&pool)
    .await?;

    Ok(())
}

#[tokio::test]
async fn migration_enforces_attention_item_deduplication_and_lifecycle()
-> Result<(), Box<dyn Error>> {
    let pool = database().await?;

    let team_id = "01921389d31174628f80cb5f79aa8001";
    let task_id = "01921389d31174628f80cb5f79aa8006";
    let att1_id = "01921389d31174628f80cb5f79aa80b1";
    let att2_id = "01921389d31174628f80cb5f79aa80b2";
    let att3_id = "01921389d31174628f80cb5f79aa80b3";

    query(&insert_team(team_id)).execute(&pool).await?;

    // `0019` validates that an AttentionItem's polymorphic `source_id` really
    // exists inside the same Team, so the Task these items point at must be a
    // real row rather than a bare id.
    query(&format!(
        "INSERT INTO roles VALUES(X'01921389d31174628f80cb5f79aa8005', X'{team_id}', 'role-a', 'ROLE-A', 'ACTIVE', 1, 1726300123456, 1726300123456, NULL)"
    ))
    .execute(&pool)
    .await?;
    query(&format!(
        "INSERT INTO tasks VALUES(X'{task_id}', X'{team_id}', X'01921389d31174628f80cb5f79aa8005', 'TASK-1', 'task', 'description', 'BLOCKED', 1, 1726300123456, 1726300123456, NULL, NULL)"
    ))
    .execute(&pool)
    .await?;

    // Insert OPEN attention item
    query(&insert_attention(AttentionRow {
        id: att1_id,
        team_id,
        attention_type: "TASK_BLOCKED",
        source_type: "TASK",
        source_id: task_id,
        title: "Task Blocked",
        summary: "Task requires external API key",
        status: "OPEN",
        dismiss_reason: None,
        resolved_at: None,
        dismissed_at: None,
    }))
    .execute(&pool)
    .await?;

    // Duplicate OPEN attention item for same source & attention type MUST FAIL
    execute_expect_error(
        &pool,
        &insert_attention(AttentionRow {
            id: att2_id,
            team_id,
            attention_type: "TASK_BLOCKED",
            source_type: "TASK",
            source_id: task_id,
            title: "Task Blocked Again",
            summary: "Duplicate block message",
            status: "OPEN",
            dismiss_reason: None,
            resolved_at: None,
            dismissed_at: None,
        }),
    )
    .await;

    // Transitioning to RESOLVED without resolved_at MUST FAIL
    execute_expect_error(
        &pool,
        &format!("UPDATE attention_items SET status = 'RESOLVED' WHERE id = X'{att1_id}';"),
    )
    .await;

    // Transitioning to DISMISSED without dismiss_reason MUST FAIL
    execute_expect_error(
        &pool,
        &format!(
            "UPDATE attention_items SET status = 'DISMISSED', dismissed_at = 1726300125000 WHERE id = X'{att1_id}';"
        ),
    )
    .await;

    // A timestamp cannot resolve the source. Unblock the Task first.
    query(&format!(
        "UPDATE tasks SET status='READY',version=2,updated_at=1726300125000 WHERE id=X'{task_id}'"
    ))
    .execute(&pool)
    .await?;

    // Transitioning to RESOLVED with resolved_at succeeds
    query(&format!(
        "UPDATE attention_items SET status = 'RESOLVED', resolved_at = 1726300125000 WHERE id = X'{att1_id}';"
    ))
    .execute(&pool)
    .await?;

    // A later execution can block again; retain the old resolved Attention.
    query(&format!(
        "UPDATE tasks SET status='DOING',version=3,updated_at=1726300126000 WHERE id=X'{task_id}'"
    ))
    .execute(&pool)
    .await?;
    query(&format!(
        "UPDATE tasks SET status='BLOCKED',version=4,updated_at=1726300127000 WHERE id=X'{task_id}'"
    ))
    .execute(&pool)
    .await?;

    // Once the previous one is RESOLVED, a new OPEN attention item for the same source succeeds
    query(&insert_attention(AttentionRow {
        id: att3_id,
        team_id,
        attention_type: "TASK_BLOCKED",
        source_type: "TASK",
        source_id: task_id,
        title: "Task Blocked Anew",
        summary: "New block occurred later",
        status: "OPEN",
        dismiss_reason: None,
        resolved_at: None,
        dismissed_at: None,
    }))
    .execute(&pool)
    .await?;

    Ok(())
}
