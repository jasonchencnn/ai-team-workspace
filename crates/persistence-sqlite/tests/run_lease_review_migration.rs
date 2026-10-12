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

fn insert_task(team_id: &str, role_id: &str, task_id: &str, display_id: &str) -> String {
    format!(
        "INSERT INTO tasks (id, team_id, owner_role_id, display_id, title, description, status, version, created_at, updated_at, cancelled_at, completed_at) VALUES (X'{task_id}', X'{team_id}', X'{role_id}', '{display_id}', 'Title', 'Desc', 'DOING', 1, 1726300123456, 1726300123456, NULL, NULL);"
    )
}

struct RunRow<'a> {
    id: &'a str,
    team_id: &'a str,
    task_id: &'a str,
    role_id: &'a str,
    binding_id: &'a str,
    harness_id: &'a str,
    attempt: i64,
    fencing_token: i64,
    status: &'a str,
    started_at: &'a str,
    completed_at: &'a str,
}

fn insert_run(row: RunRow<'_>) -> String {
    format!(
        "INSERT INTO runs (id, team_id, task_id, role_id, binding_id, harness_instance_id, attempt, fencing_token, status, exit_code, failure_reason, version, created_at, started_at, completed_at) VALUES (X'{}', X'{}', X'{}', X'{}', X'{}', X'{}', {}, {}, '{}', NULL, NULL, 1, 1726300123456, {}, {});",
        row.id,
        row.team_id,
        row.task_id,
        row.role_id,
        row.binding_id,
        row.harness_id,
        row.attempt,
        row.fencing_token,
        row.status,
        row.started_at,
        row.completed_at
    )
}

fn insert_lease(
    task_id: &str,
    run_id: &str,
    fencing_token: i64,
    acquired_at: i64,
    expires_at: i64,
) -> String {
    format!(
        "INSERT INTO leases (task_id, run_id, fencing_token, acquired_at, expires_at, version) VALUES (X'{task_id}', X'{run_id}', {fencing_token}, {acquired_at}, {expires_at}, 1);"
    )
}

fn insert_review(
    review_id: &str,
    team_id: &str,
    task_id: &str,
    run_id: &str,
    reviewer_role_id: &str,
    decision: &str,
    comments: &str,
) -> String {
    format!(
        "INSERT INTO reviews (id, team_id, task_id, run_id, reviewer_role_id, decision, comments, version, created_at) VALUES (X'{review_id}', X'{team_id}', X'{task_id}', X'{run_id}', X'{reviewer_role_id}', '{decision}', {comments}, 1, 1726300123456);"
    )
}

#[tokio::test]
async fn empty_database_upgrades_to_run_lease_review_schema() -> Result<(), Box<dyn Error>> {
    let pool = database().await?;

    // Migration replay is idempotent
    migrate(&pool).await?;

    let tables: String = query_scalar(
        "SELECT group_concat(name, ',') FROM (SELECT name FROM sqlite_master WHERE type = 'table' AND name IN ('runs', 'leases', 'reviews') ORDER BY name);",
    )
    .fetch_one(&pool)
    .await?;

    assert_eq!(tables, "leases,reviews,runs");
    Ok(())
}

#[tokio::test]
async fn migration_enforces_single_active_run_per_task() -> Result<(), Box<dyn Error>> {
    let pool = database().await?;

    let team_id = "01921389d31174628f80cb5f79aa8001";
    let role_id = "01921389d31174628f80cb5f79aa8002";
    let harness_id = "01921389d31174628f80cb5f79aa8003";
    let adapter_id = "01921389d31174628f80cb5f79aa8004";
    let binding_id = "01921389d31174628f80cb5f79aa8005";
    let task_id = "01921389d31174628f80cb5f79aa8006";
    let run1_id = "01921389d31174628f80cb5f79aa8007";
    let run2_id = "01921389d31174628f80cb5f79aa8008";

    query(&insert_team(team_id)).execute(&pool).await?;
    query(&insert_role(team_id, role_id, "ROLE-01"))
        .execute(&pool)
        .await?;
    query(&insert_harness(team_id, harness_id))
        .execute(&pool)
        .await?;
    query(&insert_adapter(team_id, adapter_id))
        .execute(&pool)
        .await?;
    query(&insert_binding(
        team_id, role_id, harness_id, adapter_id, binding_id,
    ))
    .execute(&pool)
    .await?;
    query(&insert_task(team_id, role_id, task_id, "TASK-01"))
        .execute(&pool)
        .await?;

    // First RUNNING run succeeds
    query(&insert_run(RunRow {
        id: run1_id,
        team_id,
        task_id,
        role_id,
        binding_id,
        harness_id,
        attempt: 1,
        fencing_token: 1,
        status: "RUNNING",
        started_at: "1726300123456",
        completed_at: "NULL",
    }))
    .execute(&pool)
    .await?;

    // Second active run (STARTING) on same task MUST fail
    execute_expect_error(
        &pool,
        &insert_run(RunRow {
            id: run2_id,
            team_id,
            task_id,
            role_id,
            binding_id,
            harness_id,
            attempt: 2,
            fencing_token: 2,
            status: "STARTING",
            started_at: "NULL",
            completed_at: "NULL",
        }),
    )
    .await;

    // Duplicate attempt or fencing_token MUST fail
    execute_expect_error(
        &pool,
        &insert_run(RunRow {
            id: run2_id,
            team_id,
            task_id,
            role_id,
            binding_id,
            harness_id,
            attempt: 1, // duplicate attempt
            fencing_token: 3,
            status: "SUCCEEDED",
            started_at: "1726300123456",
            completed_at: "1726300123456",
        }),
    )
    .await;

    Ok(())
}

#[tokio::test]
async fn migration_enforces_lease_and_review_invariants() -> Result<(), Box<dyn Error>> {
    let pool = database().await?;

    let team_id = "01921389d31174628f80cb5f79aa8001";
    let role_id = "01921389d31174628f80cb5f79aa8002";
    let reviewer_id = "01921389d31174628f80cb5f79aa8022";
    let harness_id = "01921389d31174628f80cb5f79aa8003";
    let adapter_id = "01921389d31174628f80cb5f79aa8004";
    let binding_id = "01921389d31174628f80cb5f79aa8005";
    let task_id = "01921389d31174628f80cb5f79aa8006";
    let run1_id = "01921389d31174628f80cb5f79aa8007";
    let review1_id = "01921389d31174628f80cb5f79aa8009";

    query(&insert_team(team_id)).execute(&pool).await?;
    query(&insert_role(team_id, role_id, "ROLE-01"))
        .execute(&pool)
        .await?;
    query(&insert_role(team_id, reviewer_id, "ROLE-02"))
        .execute(&pool)
        .await?;
    query(&insert_harness(team_id, harness_id))
        .execute(&pool)
        .await?;
    query(&insert_adapter(team_id, adapter_id))
        .execute(&pool)
        .await?;
    query(&insert_binding(
        team_id, role_id, harness_id, adapter_id, binding_id,
    ))
    .execute(&pool)
    .await?;
    query(&insert_task(team_id, role_id, task_id, "TASK-01"))
        .execute(&pool)
        .await?;

    query(&insert_run(RunRow {
        id: run1_id,
        team_id,
        task_id,
        role_id,
        binding_id,
        harness_id,
        attempt: 1,
        fencing_token: 1,
        status: "RUNNING",
        started_at: "1726300123456",
        completed_at: "NULL",
    }))
    .execute(&pool)
    .await?;

    // Valid lease succeeds
    query(&insert_lease(task_id, run1_id, 1, 1000, 2000))
        .execute(&pool)
        .await?;

    // Invalid lease: expires_at < acquired_at MUST fail
    execute_expect_error(&pool, &insert_lease(task_id, run1_id, 1, 2000, 1000)).await;

    query(&format!("DELETE FROM leases WHERE task_id = X'{task_id}'"))
        .execute(&pool)
        .await?;
    query(&format!(
        "UPDATE runs SET status = 'SUCCEEDED', completed_at = 1726300123457 WHERE id = X'{run1_id}'"
    ))
    .execute(&pool)
    .await?;

    // Valid approved review succeeds
    query(&insert_review(
        review1_id,
        team_id,
        task_id,
        run1_id,
        reviewer_id,
        "APPROVED",
        "NULL",
    ))
    .execute(&pool)
    .await?;

    // Review with CHANGES_REQUESTED but NULL comments MUST fail
    let review2_id = "01921389d31174628f80cb5f79aa8010";
    execute_expect_error(
        &pool,
        &insert_review(
            review2_id,
            team_id,
            task_id,
            run1_id,
            reviewer_id,
            "CHANGES_REQUESTED",
            "NULL",
        ),
    )
    .await;

    // Foreign key RESTRICT: cannot delete Task when Run or Review exists
    execute_expect_error(
        &pool,
        &format!("DELETE FROM tasks WHERE id = X'{task_id}';"),
    )
    .await;

    Ok(())
}
