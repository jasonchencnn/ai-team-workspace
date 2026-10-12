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

fn insert_role(team_id: &str, role_id: &str) -> String {
    format!(
        "INSERT INTO roles (id, team_id, display_id, name, status, version, created_at, updated_at, deactivated_at) VALUES (X'{role_id}', X'{team_id}', 'DEV-01', 'Developer', 'ACTIVE', 1, 1726300123456, 1726300123456, NULL);"
    )
}

fn insert_harness(team_id: &str, harness_id: &str, status: &str, disabled_at: &str) -> String {
    format!(
        "INSERT INTO harness_instances (id, team_id, harness_type, display_name, status, capabilities, credential_reference, version, created_at, updated_at, disabled_at) VALUES (X'{harness_id}', X'{team_id}', 'codex', 'Codex Primary', '{status}', '[\"run.start\",\"run.cancel\"]', 'keychain://codex-token', 1, 1726300123456, 1726300123456, {disabled_at});"
    )
}

fn insert_adapter(team_id: &str, adapter_id: &str) -> String {
    format!(
        "INSERT INTO adapters (id, team_id, name, protocol_version, executable_path, manifest_json, version, created_at, updated_at) VALUES (X'{adapter_id}', X'{team_id}', 'codex-adapter', '0.1.0', '/usr/local/bin/codex-adapter', '{{\"name\":\"codex-adapter\",\"protocol_version\":\"0.1.0\"}}', 1, 1726300123456, 1726300123456);"
    )
}

struct BindingRow<'a> {
    id: &'a str,
    team_id: &'a str,
    role_id: &'a str,
    harness_id: &'a str,
    adapter_id: &'a str,
    status: &'a str,
    generation: i64,
    effective_from: &'a str,
    effective_to: &'a str,
}

fn insert_binding(row: BindingRow<'_>) -> String {
    format!(
        "INSERT INTO bindings (id, team_id, role_id, harness_instance_id, adapter_id, status, binding_generation, version, configuration_snapshot, created_at, updated_at, effective_from, effective_to) VALUES (X'{}', X'{}', X'{}', X'{}', X'{}', '{}', {}, 1, '{{\"model\":\"gpt-5\"}}', 1726300123456, 1726300123456, {}, {});",
        row.id,
        row.team_id,
        row.role_id,
        row.harness_id,
        row.adapter_id,
        row.status,
        row.generation,
        row.effective_from,
        row.effective_to
    )
}

#[tokio::test]
async fn empty_database_upgrades_to_harness_adapter_binding_schema() -> Result<(), Box<dyn Error>> {
    let pool = database().await?;

    // Migration replay is idempotent
    migrate(&pool).await?;

    let tables: String = query_scalar(
        "SELECT group_concat(name, ',') FROM (SELECT name FROM sqlite_master WHERE type = 'table' AND name IN ('harness_instances', 'adapters', 'bindings') ORDER BY name);",
    )
    .fetch_one(&pool)
    .await?;

    assert_eq!(tables, "adapters,bindings,harness_instances");
    Ok(())
}

#[tokio::test]
async fn migration_enforces_single_active_binding_per_role() -> Result<(), Box<dyn Error>> {
    let pool = database().await?;

    let team_id = "01921389d31174628f80cb5f79aa8001";
    let role_id = "01921389d31174628f80cb5f79aa8002";
    let harness_id = "01921389d31174628f80cb5f79aa8003";
    let adapter_id = "01921389d31174628f80cb5f79aa8004";
    let binding1_id = "01921389d31174628f80cb5f79aa8005";
    let binding2_id = "01921389d31174628f80cb5f79aa8006";

    query(&insert_team(team_id)).execute(&pool).await?;
    query(&insert_role(team_id, role_id)).execute(&pool).await?;
    query(&insert_harness(team_id, harness_id, "ACTIVE", "NULL"))
        .execute(&pool)
        .await?;
    query(&insert_adapter(team_id, adapter_id))
        .execute(&pool)
        .await?;

    // First ACTIVE binding succeeds
    query(&insert_binding(BindingRow {
        id: binding1_id,
        team_id,
        role_id,
        harness_id,
        adapter_id,
        status: "ACTIVE",
        generation: 1,
        effective_from: "1726300123456",
        effective_to: "NULL",
    }))
    .execute(&pool)
    .await?;

    // Second ACTIVE binding for the same role MUST fail
    execute_expect_error(
        &pool,
        &insert_binding(BindingRow {
            id: binding2_id,
            team_id,
            role_id,
            harness_id,
            adapter_id,
            status: "ACTIVE",
            generation: 2,
            effective_from: "1726300123457",
            effective_to: "NULL",
        }),
    )
    .await;

    // Duplicate binding generation for the same role MUST fail
    execute_expect_error(
        &pool,
        &insert_binding(BindingRow {
            id: binding2_id,
            team_id,
            role_id,
            harness_id,
            adapter_id,
            status: "REVOKED",
            generation: 1, // duplicate generation 1
            effective_from: "1726300123456",
            effective_to: "1726300123456",
        }),
    )
    .await;

    Ok(())
}

#[tokio::test]
async fn migration_enforces_non_overlapping_historical_effective_ranges()
-> Result<(), Box<dyn Error>> {
    let pool = database().await?;

    let team_id = "01921389d31174628f80cb5f79aa8001";
    let role_id = "01921389d31174628f80cb5f79aa8002";
    let harness_id = "01921389d31174628f80cb5f79aa8003";
    let adapter_id = "01921389d31174628f80cb5f79aa8004";
    let binding1_id = "01921389d31174628f80cb5f79aa8005";
    let binding2_id = "01921389d31174628f80cb5f79aa8006";

    query(&insert_team(team_id)).execute(&pool).await?;
    query(&insert_role(team_id, role_id)).execute(&pool).await?;
    query(&insert_harness(team_id, harness_id, "ACTIVE", "NULL"))
        .execute(&pool)
        .await?;
    query(&insert_adapter(team_id, adapter_id))
        .execute(&pool)
        .await?;

    // Historical revoked binding covering [1000, 2000]
    query(&insert_binding(BindingRow {
        id: binding1_id,
        team_id,
        role_id,
        harness_id,
        adapter_id,
        status: "REVOKED",
        generation: 1,
        effective_from: "1000",
        effective_to: "2000",
    }))
    .execute(&pool)
    .await?;

    // Attempting to insert another binding covering [1500, 2500] (overlaps with [1000, 2000]) MUST fail
    execute_expect_error(
        &pool,
        &insert_binding(BindingRow {
            id: binding2_id,
            team_id,
            role_id,
            harness_id,
            adapter_id,
            status: "REVOKED",
            generation: 2,
            effective_from: "1500",
            effective_to: "2500",
        }),
    )
    .await;

    // Non-overlapping historical binding covering [2001, 3000] succeeds
    query(&insert_binding(BindingRow {
        id: binding2_id,
        team_id,
        role_id,
        harness_id,
        adapter_id,
        status: "REVOKED",
        generation: 2,
        effective_from: "2001",
        effective_to: "3000",
    }))
    .execute(&pool)
    .await?;

    Ok(())
}

#[tokio::test]
async fn migration_enforces_foreign_keys_and_uuidv7_constraints() -> Result<(), Box<dyn Error>> {
    let pool = database().await?;

    let valid_team_id = "01921389d31174628f80cb5f79aa8001";
    let valid_role_id = "01921389d31174628f80cb5f79aa8002";
    let valid_harness_id = "01921389d31174628f80cb5f79aa8003";
    let valid_adapter_id = "01921389d31174628f80cb5f79aa8004";
    let valid_binding_id = "01921389d31174628f80cb5f79aa8005";

    // Non-UUIDv7 ID check (version nibble is '4' instead of '7')
    let invalid_uuid_id = "01921389d31144628f80cb5f79aa8005";

    query(&insert_team(valid_team_id)).execute(&pool).await?;
    query(&insert_role(valid_team_id, valid_role_id))
        .execute(&pool)
        .await?;
    query(&insert_harness(
        valid_team_id,
        valid_harness_id,
        "ACTIVE",
        "NULL",
    ))
    .execute(&pool)
    .await?;
    query(&insert_adapter(valid_team_id, valid_adapter_id))
        .execute(&pool)
        .await?;

    // Reject non-UUIDv7 binding ID
    execute_expect_error(
        &pool,
        &insert_binding(BindingRow {
            id: invalid_uuid_id,
            team_id: valid_team_id,
            role_id: valid_role_id,
            harness_id: valid_harness_id,
            adapter_id: valid_adapter_id,
            status: "ACTIVE",
            generation: 1,
            effective_from: "1726300123456",
            effective_to: "NULL",
        }),
    )
    .await;

    // Foreign key RESTRICT: cannot delete role when binding exists
    query(&insert_binding(BindingRow {
        id: valid_binding_id,
        team_id: valid_team_id,
        role_id: valid_role_id,
        harness_id: valid_harness_id,
        adapter_id: valid_adapter_id,
        status: "ACTIVE",
        generation: 1,
        effective_from: "1726300123456",
        effective_to: "NULL",
    }))
    .execute(&pool)
    .await?;

    execute_expect_error(
        &pool,
        &format!("DELETE FROM roles WHERE id = X'{valid_role_id}';"),
    )
    .await;

    // Foreign key RESTRICT: cannot delete harness when binding exists
    execute_expect_error(
        &pool,
        &format!("DELETE FROM harness_instances WHERE id = X'{valid_harness_id}';"),
    )
    .await;

    Ok(())
}
