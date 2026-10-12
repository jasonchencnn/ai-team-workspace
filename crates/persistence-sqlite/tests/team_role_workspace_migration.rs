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

fn insert_team(id: &str, name: &str, status: &str, archived_at: &str) -> String {
    format!(
        "INSERT INTO teams (id, name, status, version, created_at, updated_at, archived_at) VALUES (X'{id}', '{name}', '{status}', 1, 1726300123456, 1726300123456, {archived_at});"
    )
}

fn insert_role(
    team_id: &str,
    id: &str,
    display_id: &str,
    status: &str,
    deactivated_at: &str,
) -> String {
    format!(
        "INSERT INTO roles (id, team_id, display_id, name, status, version, created_at, updated_at, deactivated_at) VALUES (X'{id}', X'{team_id}', '{display_id}', 'Developer', '{status}', 1, 1726300123456, 1726300123456, {deactivated_at});"
    )
}

fn insert_workspace(team_id: &str, id: &str) -> String {
    format!(
        "INSERT INTO workspaces (id, team_id, kind, root_reference, validation_status, version, created_at, updated_at) VALUES (X'{id}', X'{team_id}', 'GIT', 'workspace://example', 'UNVERIFIED', 1, 1726300123456, 1726300123456);"
    )
}

#[tokio::test]
async fn empty_database_upgrades_to_team_role_and_workspace_schema() -> Result<(), Box<dyn Error>> {
    let pool = database().await?;

    migrate(&pool).await?;

    let tables: String = query_scalar(
        "SELECT group_concat(name, ',') FROM sqlite_master WHERE type = 'table' ORDER BY name;",
    )
    .fetch_one(&pool)
    .await?;

    assert!(tables.contains("teams"));
    assert!(tables.contains("roles"));
    assert!(tables.contains("workspaces"));
    Ok(())
}

#[tokio::test]
async fn migration_enforces_stable_uuidv7_ids_versions_and_foreign_keys()
-> Result<(), Box<dyn Error>> {
    let pool = database().await?;
    let team_id = "0191EF8171407000A5A5A5A5A5A5A5A5";
    let role_id = "0191EF8171417000A5A5A5A5A5A5A5A5";
    let workspace_id = "0191EF8171427000A5A5A5A5A5A5A5A5";

    query(&insert_team(team_id, "Platform", "ACTIVE", "NULL"))
        .execute(&pool)
        .await?;
    query(&insert_role(team_id, role_id, "DEV-001", "ACTIVE", "NULL"))
        .execute(&pool)
        .await?;
    query(&insert_workspace(team_id, workspace_id))
        .execute(&pool)
        .await?;

    execute_expect_error(
        &pool,
        &insert_team(
            "0191EF8171406000A5A5A5A5A5A5A5A5",
            "Invalid UUID version",
            "ACTIVE",
            "NULL",
        ),
    )
    .await;
    execute_expect_error(
        &pool,
        "INSERT INTO teams (id, name, status, version, created_at, updated_at, archived_at) VALUES (X'0191EF8171437000A5A5A5A5A5A5A5A5', 'Zero version', 'ACTIVE', 0, 1726300123456, 1726300123456, NULL);",
    )
    .await;
    execute_expect_error(
        &pool,
        &insert_role(
            "0191EF8171447000A5A5A5A5A5A5A5A5",
            "0191EF8171457000A5A5A5A5A5A5A5A5",
            "DEV-002",
            "ACTIVE",
            "NULL",
        ),
    )
    .await;

    query("PRAGMA foreign_keys = OFF;").execute(&pool).await?;
    execute_expect_error(
        &pool,
        &insert_role(
            "0191EF8171446000A5A5A5A5A5A5A5A5",
            "0191EF8171467000A5A5A5A5A5A5A5A5",
            "DEV-003",
            "ACTIVE",
            "NULL",
        ),
    )
    .await;
    execute_expect_error(
        &pool,
        &insert_workspace(
            "0191EF8171447000C5A5A5A5A5A5A5A5",
            "0191EF8171477000A5A5A5A5A5A5A5A5",
        ),
    )
    .await;
    query("PRAGMA foreign_keys = ON;").execute(&pool).await?;
    Ok(())
}

#[tokio::test]
async fn migration_enforces_team_scoped_role_and_workspace_uniqueness() -> Result<(), Box<dyn Error>>
{
    let pool = database().await?;
    let team_id = "0191EF8171507000A5A5A5A5A5A5A5A5";
    query(&insert_team(team_id, "Platform", "ACTIVE", "NULL"))
        .execute(&pool)
        .await?;
    query(&insert_role(
        team_id,
        "0191EF8171517000A5A5A5A5A5A5A5A5",
        "DEV-001",
        "ACTIVE",
        "NULL",
    ))
    .execute(&pool)
    .await?;
    execute_expect_error(
        &pool,
        &insert_role(
            team_id,
            "0191EF8171527000A5A5A5A5A5A5A5A5",
            "DEV-001",
            "ACTIVE",
            "NULL",
        ),
    )
    .await;
    query(&insert_workspace(
        team_id,
        "0191EF8171537000A5A5A5A5A5A5A5A5",
    ))
    .execute(&pool)
    .await?;
    execute_expect_error(
        &pool,
        &insert_workspace(team_id, "0191EF8171547000A5A5A5A5A5A5A5A5"),
    )
    .await;
    Ok(())
}

#[tokio::test]
async fn migration_models_team_archival_and_role_deactivation_without_deleting_history()
-> Result<(), Box<dyn Error>> {
    let pool = database().await?;
    let team_id = "0191EF8171607000A5A5A5A5A5A5A5A5";
    query(&insert_team(
        team_id,
        "Archived platform",
        "ARCHIVED",
        "1726300123456",
    ))
    .execute(&pool)
    .await?;
    query(&insert_role(
        team_id,
        "0191EF8171617000A5A5A5A5A5A5A5A5",
        "DEV-002",
        "DEACTIVATED",
        "1726300123456",
    ))
    .execute(&pool)
    .await?;

    execute_expect_error(
        &pool,
        &insert_team(
            "0191EF8171627000A5A5A5A5A5A5A5A5",
            "Missing archive time",
            "ARCHIVED",
            "NULL",
        ),
    )
    .await;
    execute_expect_error(
        &pool,
        &insert_role(
            team_id,
            "0191EF8171637000A5A5A5A5A5A5A5A5",
            "DEV-003",
            "DEACTIVATED",
            "NULL",
        ),
    )
    .await;
    execute_expect_error(
        &pool,
        "DELETE FROM teams WHERE id = X'0191EF8171607000A5A5A5A5A5A5A5A5';",
    )
    .await;
    Ok(())
}
