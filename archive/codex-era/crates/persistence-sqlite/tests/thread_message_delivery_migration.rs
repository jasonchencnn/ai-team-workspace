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

fn insert_harness(team_id: &str, harness_id: &str) -> String {
    format!(
        "INSERT INTO harness_instances (id, team_id, harness_type, display_name, status, capabilities, credential_reference, version, created_at, updated_at, disabled_at) VALUES (X'{harness_id}', X'{team_id}', 'codex', 'Codex Primary', 'ACTIVE', '[\"run.start\"]', NULL, 1, 1726300123456, 1726300123456, NULL);"
    )
}

fn insert_adapter(team_id: &str, adapter_id: &str) -> String {
    format!(
        "INSERT INTO adapters (id, team_id, name, protocol_version, executable_path, manifest_json, version, created_at, updated_at) VALUES (X'{adapter_id}', X'{team_id}', 'codex-adapter', '0.1.0', '/usr/local/bin/codex-adapter', '{{\"name\":\"codex-adapter\"}}', 1, 1726300123456, 1726300123456);"
    )
}

fn insert_binding(
    team_id: &str,
    role_id: &str,
    harness_id: &str,
    adapter_id: &str,
    binding_id: &str,
) -> String {
    format!(
        "INSERT INTO bindings (id, team_id, role_id, harness_instance_id, adapter_id, status, binding_generation, version, configuration_snapshot, created_at, updated_at, effective_from, effective_to) VALUES (X'{binding_id}', X'{team_id}', X'{role_id}', X'{harness_id}', X'{adapter_id}', 'ACTIVE', 1, 1, '{{}}', 1726300123456, 1726300123456, 1726300123456, NULL);"
    )
}

fn insert_thread(team_id: &str, thread_id: &str, subject: &str) -> String {
    format!(
        "INSERT INTO threads (id, team_id, subject, status, version, created_at, updated_at) VALUES (X'{thread_id}', X'{team_id}', '{subject}', 'OPEN', 1, 1726300123456, 1726300123456);"
    )
}

struct MessageRow<'a> {
    id: &'a str,
    team_id: &'a str,
    thread_id: &'a str,
    sender_actor_type: &'a str,
    from_role_id: Option<&'a str>,
    to_role_id: &'a str,
    reply_to_message_id: Option<&'a str>,
    body: &'a str,
    status: &'a str,
    idempotency_key: Option<&'a str>,
    /// STATE_MACHINES.md section 5 records the sender Binding snapshot as a side
    /// effect of DRAFT -> QUEUED; required for a Role sender that has left DRAFT.
    sender_binding_id: Option<&'a str>,
}

fn insert_message(row: MessageRow<'_>) -> String {
    let from_str = match row.from_role_id {
        Some(id) => format!("X'{id}'"),
        None => "NULL".to_string(),
    };
    let reply_str = match row.reply_to_message_id {
        Some(id) => format!("X'{id}'"),
        None => "NULL".to_string(),
    };
    let ikey_str = match row.idempotency_key {
        Some(k) => format!("'{k}'"),
        None => "NULL".to_string(),
    };
    // STATE_MACHINES.md section 5 records the sender Binding snapshot as a side
    // effect of DRAFT -> QUEUED, so a Role sender that has left DRAFT must carry
    // one.  A DRAFT draft legitimately has none yet.
    let snapshot_str = match row.sender_binding_id {
        Some(id) => format!("X'{id}', 1"),
        None => "NULL, NULL".to_string(),
    };

    format!(
        "INSERT INTO messages (id, team_id, thread_id, sender_actor_type, from_role_id, from_human_id, to_role_id, sender_binding_snapshot_id, sender_binding_generation, reply_to_message_id, message_type, body, status, version, idempotency_key, expires_at, created_at, updated_at, delivered_at, acknowledged_at, replied_at) VALUES (X'{}', X'{}', X'{}', '{}', {}, NULL, X'{}', {}, {}, 'DIRECT', '{}', '{}', 1, {}, NULL, 1726300123456, 1726300123456, NULL, NULL, NULL);",
        row.id,
        row.team_id,
        row.thread_id,
        row.sender_actor_type,
        from_str,
        row.to_role_id,
        snapshot_str,
        reply_str,
        row.body,
        row.status,
        ikey_str
    )
}

#[tokio::test]
async fn empty_database_upgrades_to_thread_message_delivery_schema() -> Result<(), Box<dyn Error>> {
    let pool = database().await?;

    // Migration replay is idempotent
    migrate(&pool).await?;

    let tables: String = query_scalar(
        "SELECT group_concat(name, ',') FROM (SELECT name FROM sqlite_master WHERE type = 'table' AND name IN ('threads', 'messages', 'delivery_attempts') ORDER BY name);",
    )
    .fetch_one(&pool)
    .await?;

    assert_eq!(tables, "delivery_attempts,messages,threads");
    Ok(())
}

#[tokio::test]
async fn migration_enforces_mandatory_to_role_and_body_freezing() -> Result<(), Box<dyn Error>> {
    let pool = database().await?;

    let team_id = "01921389d31174628f80cb5f79aa8001";
    let role1_id = "01921389d31174628f80cb5f79aa8002";
    let role2_id = "01921389d31174628f80cb5f79aa8003";
    let harness_id = "01921389d31174628f80cb5f79aa8004";
    let adapter_id = "01921389d31174628f80cb5f79aa8005";
    let binding_id = "01921389d31174628f80cb5f79aa8006";
    let thread1_id = "01921389d31174628f80cb5f79aa8071";
    let msg1_id = "01921389d31174628f80cb5f79aa8081";

    query(&insert_team(team_id)).execute(&pool).await?;
    query(&insert_role(team_id, role1_id, "ROLE-01"))
        .execute(&pool)
        .await?;
    query(&insert_role(team_id, role2_id, "ROLE-02"))
        .execute(&pool)
        .await?;
    query(&insert_harness(team_id, harness_id))
        .execute(&pool)
        .await?;
    query(&insert_adapter(team_id, adapter_id))
        .execute(&pool)
        .await?;
    query(&insert_binding(
        team_id, role1_id, harness_id, adapter_id, binding_id,
    ))
    .execute(&pool)
    .await?;
    query(&insert_thread(team_id, thread1_id, "Task Discussion"))
        .execute(&pool)
        .await?;

    // Insert draft message
    query(&insert_message(MessageRow {
        id: msg1_id,
        team_id,
        thread_id: thread1_id,
        sender_actor_type: "ROLE",
        from_role_id: Some(role1_id),
        to_role_id: role2_id,
        reply_to_message_id: None,
        body: "Initial draft proposal",
        status: "DRAFT",
        idempotency_key: None,
        sender_binding_id: None,
    }))
    .execute(&pool)
    .await?;

    // DRAFT body can be edited
    query(&format!(
        "UPDATE messages SET body = 'Revised draft proposal' WHERE id = X'{msg1_id}';"
    ))
    .execute(&pool)
    .await?;

    // Transition to QUEUED, recording the sender Binding snapshot as the frozen
    // transition table requires.
    query(&format!(
        "UPDATE messages SET status = 'QUEUED', sender_binding_snapshot_id = X'{binding_id}', sender_binding_generation = 1 WHERE id = X'{msg1_id}';"
    ))
    .execute(&pool)
    .await?;

    // The snapshot is frozen once the sender has left DRAFT.
    execute_expect_error(
        &pool,
        &format!("UPDATE messages SET sender_binding_snapshot_id = X'{binding_id}', sender_binding_generation = 9 WHERE id = X'{msg1_id}';"),
    )
    .await;

    // Once QUEUED, body is frozen! UPDATE body MUST FAIL
    execute_expect_error(
        &pool,
        &format!("UPDATE messages SET body = 'Tampered content' WHERE id = X'{msg1_id}';"),
    )
    .await;

    // STATE_MACHINES.md section 5 has no QUEUED -> DELIVERED edge; ROUTED is
    // mandatory, so the shortcut is rejected.
    execute_expect_error(
        &pool,
        &format!("UPDATE messages SET status = 'DELIVERED' WHERE id = X'{msg1_id}';"),
    )
    .await;

    // Non-body updates along the legal path still succeed.
    query(&format!(
        "UPDATE messages SET status = 'ROUTED' WHERE id = X'{msg1_id}';"
    ))
    .execute(&pool)
    .await?;
    query(&format!(
        "UPDATE messages SET status = 'DELIVERED', delivered_at = 1726300125000 WHERE id = X'{msg1_id}';"
    ))
    .execute(&pool)
    .await?;

    Ok(())
}

#[tokio::test]
async fn migration_enforces_reply_to_same_thread_and_self_ref() -> Result<(), Box<dyn Error>> {
    let pool = database().await?;

    let team_id = "01921389d31174628f80cb5f79aa8001";
    let role1_id = "01921389d31174628f80cb5f79aa8002";
    let role2_id = "01921389d31174628f80cb5f79aa8003";
    let thread1_id = "01921389d31174628f80cb5f79aa8071";
    let harness_id = "01921389d31174628f80cb5f79aa8004";
    let adapter_id = "01921389d31174628f80cb5f79aa8005";
    let binding1_id = "01921389d31174628f80cb5f79aa8006";
    let binding2_id = "01921389d31174628f80cb5f79aa8007";
    let thread2_id = "01921389d31174628f80cb5f79aa8072";
    let msg1_id = "01921389d31174628f80cb5f79aa8081";
    let msg2_id = "01921389d31174628f80cb5f79aa8082";
    let msg3_id = "01921389d31174628f80cb5f79aa8083";

    query(&insert_team(team_id)).execute(&pool).await?;
    query(&insert_role(team_id, role1_id, "ROLE-01"))
        .execute(&pool)
        .await?;
    query(&insert_role(team_id, role2_id, "ROLE-02"))
        .execute(&pool)
        .await?;
    query(&insert_harness(team_id, harness_id))
        .execute(&pool)
        .await?;
    query(&insert_adapter(team_id, adapter_id))
        .execute(&pool)
        .await?;
    query(&insert_binding(
        team_id,
        role1_id,
        harness_id,
        adapter_id,
        binding1_id,
    ))
    .execute(&pool)
    .await?;
    query(&insert_binding(
        team_id,
        role2_id,
        harness_id,
        adapter_id,
        binding2_id,
    ))
    .execute(&pool)
    .await?;
    query(&insert_thread(team_id, thread1_id, "Thread 1"))
        .execute(&pool)
        .await?;
    query(&insert_thread(team_id, thread2_id, "Thread 2"))
        .execute(&pool)
        .await?;

    // Original message in Thread 1
    query(&insert_message(MessageRow {
        id: msg1_id,
        team_id,
        thread_id: thread1_id,
        sender_actor_type: "ROLE",
        from_role_id: Some(role1_id),
        to_role_id: role2_id,
        reply_to_message_id: None,
        body: "Hello in Thread 1",
        status: "DRAFT",
        idempotency_key: None,
        sender_binding_id: Some(binding1_id),
    }))
    .execute(&pool)
    .await?;

    // Reply in same Thread 1 succeeds
    query(&insert_message(MessageRow {
        id: msg2_id,
        team_id,
        thread_id: thread1_id,
        sender_actor_type: "ROLE",
        from_role_id: Some(role2_id),
        to_role_id: role1_id,
        reply_to_message_id: Some(msg1_id),
        body: "Reply in Thread 1",
        status: "DRAFT",
        idempotency_key: None,
        sender_binding_id: Some(binding2_id),
    }))
    .execute(&pool)
    .await?;

    // Reply across different Thread (Thread 2 replying to Thread 1 msg) MUST FAIL
    execute_expect_error(
        &pool,
        &insert_message(MessageRow {
            id: msg3_id,
            team_id,
            thread_id: thread2_id,
            sender_actor_type: "ROLE",
            from_role_id: Some(role1_id),
            to_role_id: role2_id,
            reply_to_message_id: Some(msg1_id),
            body: "Mismatched reply across threads",
            status: "DRAFT",
            idempotency_key: None,
            sender_binding_id: Some(binding1_id),
        }),
    )
    .await;

    // Self-reply (reply_to == id) MUST FAIL
    let msg4_id = "01921389d31174628f80cb5f79aa8084";
    execute_expect_error(
        &pool,
        &insert_message(MessageRow {
            id: msg4_id,
            team_id,
            thread_id: thread1_id,
            sender_actor_type: "ROLE",
            from_role_id: Some(role1_id),
            to_role_id: role2_id,
            reply_to_message_id: Some(msg4_id),
            body: "Self-referencing reply",
            status: "DRAFT",
            idempotency_key: None,
            sender_binding_id: Some(binding1_id),
        }),
    )
    .await;

    Ok(())
}

#[tokio::test]
async fn migration_enforces_delivery_attempt_invariants() -> Result<(), Box<dyn Error>> {
    let pool = database().await?;

    let team_id = "01921389d31174628f80cb5f79aa8001";
    let role1_id = "01921389d31174628f80cb5f79aa8002";
    let role2_id = "01921389d31174628f80cb5f79aa8003";
    let harness_id = "01921389d31174628f80cb5f79aa8004";
    let adapter_id = "01921389d31174628f80cb5f79aa8005";
    let binding_id = "01921389d31174628f80cb5f79aa8006";
    let thread_id = "01921389d31174628f80cb5f79aa8071";
    let msg_id = "01921389d31174628f80cb5f79aa8081";
    let att1_id = "01921389d31174628f80cb5f79aa8091";
    let att2_id = "01921389d31174628f80cb5f79aa8092";

    query(&insert_team(team_id)).execute(&pool).await?;
    query(&insert_role(team_id, role1_id, "ROLE-01"))
        .execute(&pool)
        .await?;
    query(&insert_role(team_id, role2_id, "ROLE-02"))
        .execute(&pool)
        .await?;
    query(&insert_harness(team_id, harness_id))
        .execute(&pool)
        .await?;
    query(&insert_adapter(team_id, adapter_id))
        .execute(&pool)
        .await?;
    query(&insert_binding(
        team_id, role2_id, harness_id, adapter_id, binding_id,
    ))
    .execute(&pool)
    .await?;
    // The message is sent by Role 1, so its snapshot needs Role 1's own Binding.
    let sender_binding_id = "01921389d31174628f80cb5f79aa8007";
    query(&insert_binding(
        team_id,
        role1_id,
        harness_id,
        adapter_id,
        sender_binding_id,
    ))
    .execute(&pool)
    .await?;
    query(&insert_thread(team_id, thread_id, "Thread"))
        .execute(&pool)
        .await?;

    query(&insert_message(MessageRow {
        id: msg_id,
        team_id,
        thread_id,
        sender_actor_type: "ROLE",
        from_role_id: Some(role1_id),
        to_role_id: role2_id,
        reply_to_message_id: None,
        body: "Instruction to Role 2",
        status: "DRAFT",
        idempotency_key: Some("idem-msg-01"),
        sender_binding_id: Some(sender_binding_id),
    }))
    .execute(&pool)
    .await?;
    // `0018`: a Message is born as DRAFT; `0018` also requires the sender
    // Binding snapshot to be frozen as part of DRAFT -> QUEUED.
    for step in ["QUEUED", "ROUTED"] {
        query(&format!(
            "UPDATE messages SET status = '{step}' WHERE id = X'{msg_id}';"
        ))
        .execute(&pool)
        .await?;
    }

    // Attempt 1: IN_FLIGHT
    query(&format!(
        "INSERT INTO delivery_attempts (id, team_id, message_id, attempt_number, target_role_id, target_binding_id, target_binding_generation, status, error_code, error_message, created_at, started_at, completed_at) VALUES (X'{att1_id}', X'{team_id}', X'{msg_id}', 1, X'{role2_id}', X'{binding_id}', 1, 'PENDING', NULL, NULL, 1726300124000, NULL, NULL);"
    ))
    .execute(&pool)
    .await?;

    // Attempt 2 while Attempt 1 is still IN_FLIGHT MUST FAIL (single active in-flight check)
    execute_expect_error(
        &pool,
        &format!(
            "INSERT INTO delivery_attempts (id, team_id, message_id, attempt_number, target_role_id, target_binding_id, target_binding_generation, status, error_code, error_message, created_at, started_at, completed_at) VALUES (X'{att2_id}', X'{team_id}', X'{msg_id}', 2, X'{role2_id}', X'{binding_id}', 1, 'PENDING', NULL, NULL, 1726300125000, NULL, NULL);"
        ),
    )
    .await;

    // Fail Attempt 1 (terminal state)
    query(&format!(
        "UPDATE delivery_attempts SET status = 'FAILED', error_code = 'ADAPTER_UNAVAILABLE', completed_at = 1726300126000 WHERE id = X'{att1_id}';"
    ))
    .execute(&pool)
    .await?;

    // Now Attempt 2 succeeds
    query(&format!(
        "INSERT INTO delivery_attempts (id, team_id, message_id, attempt_number, target_role_id, target_binding_id, target_binding_generation, status, error_code, error_message, created_at, started_at, completed_at) VALUES (X'{att2_id}', X'{team_id}', X'{msg_id}', 2, X'{role2_id}', X'{binding_id}', 1, 'PENDING', NULL, NULL, 1726300127000, NULL, NULL);"
    ))
    .execute(&pool)
    .await?;

    // Succeeded Attempt 2
    query(&format!(
        "UPDATE delivery_attempts SET status = 'IN_FLIGHT', started_at = 1726300127100 WHERE id = X'{att2_id}';"
    ))
    .execute(&pool)
    .await?;
    query(&format!(
        "UPDATE delivery_attempts SET status = 'SUCCEEDED', completed_at = 1726300128000 WHERE id = X'{att2_id}';"
    ))
    .execute(&pool)
    .await?;

    // Verify both attempts preserved in history
    let attempt_count: i64 = query_scalar(&format!(
        "SELECT COUNT(*) FROM delivery_attempts WHERE message_id = X'{msg_id}';"
    ))
    .fetch_one(&pool)
    .await?;
    assert_eq!(attempt_count, 2);

    Ok(())
}
