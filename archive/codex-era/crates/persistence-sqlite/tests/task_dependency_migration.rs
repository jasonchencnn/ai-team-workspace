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

struct TaskRow<'a> {
    id: &'a str,
    team_id: &'a str,
    owner_role_id: &'a str,
    display_id: &'a str,
    title: &'a str,
    status: &'a str,
    cancelled_at: &'a str,
    completed_at: &'a str,
}

fn insert_task(row: TaskRow<'_>) -> String {
    format!(
        "INSERT INTO tasks (id, team_id, owner_role_id, display_id, title, description, status, version, created_at, updated_at, cancelled_at, completed_at) VALUES (X'{}', X'{}', X'{}', '{}', '{}', 'Task details', '{}', 1, 1726300123456, 1726300123456, {}, {});",
        row.id,
        row.team_id,
        row.owner_role_id,
        row.display_id,
        row.title,
        row.status,
        row.cancelled_at,
        row.completed_at
    )
}

fn insert_dependency(team_id: &str, task_id: &str, depends_on: &str, dep_type: &str) -> String {
    format!(
        "INSERT INTO task_dependencies (team_id, task_id, depends_on_task_id, dependency_type, created_at) VALUES (X'{team_id}', X'{task_id}', X'{depends_on}', '{dep_type}', 1726300123456);"
    )
}

#[tokio::test]
async fn empty_database_upgrades_to_task_and_dependency_schema() -> Result<(), Box<dyn Error>> {
    let pool = database().await?;

    // Migration replay is idempotent
    migrate(&pool).await?;

    let tables: String = query_scalar(
        "SELECT group_concat(name, ',') FROM (SELECT name FROM sqlite_master WHERE type = 'table' AND name IN ('tasks', 'task_dependencies') ORDER BY name);",
    )
    .fetch_one(&pool)
    .await?;

    assert_eq!(tables, "task_dependencies,tasks");
    Ok(())
}

#[tokio::test]
async fn migration_enforces_task_invariants_and_unique_display_id() -> Result<(), Box<dyn Error>> {
    let pool = database().await?;

    let team_id = "01921389d31174628f80cb5f79aa8001";
    let role_id = "01921389d31174628f80cb5f79aa8002";
    let task1_id = "01921389d31174628f80cb5f79aa8011";
    let task2_id = "01921389d31174628f80cb5f79aa8012";

    query(&insert_team(team_id)).execute(&pool).await?;
    query(&insert_role(team_id, role_id, "ROLE-01"))
        .execute(&pool)
        .await?;

    // Valid task succeeds
    query(&insert_task(TaskRow {
        id: task1_id,
        team_id,
        owner_role_id: role_id,
        display_id: "TASK-01",
        title: "Setup Auth",
        status: "TODO",
        cancelled_at: "NULL",
        completed_at: "NULL",
    }))
    .execute(&pool)
    .await?;

    // Duplicate display_id in same team MUST fail
    execute_expect_error(
        &pool,
        &insert_task(TaskRow {
            id: task2_id,
            team_id,
            owner_role_id: role_id,
            display_id: "TASK-01", // duplicate
            title: "Setup Payments",
            status: "TODO",
            cancelled_at: "NULL",
            completed_at: "NULL",
        }),
    )
    .await;

    // Disallowed status (e.g. FAILED is not a Task status, only Run status) MUST fail
    execute_expect_error(
        &pool,
        &insert_task(TaskRow {
            id: task2_id,
            team_id,
            owner_role_id: role_id,
            display_id: "TASK-02",
            title: "Setup Payments",
            status: "FAILED", // invalid status
            cancelled_at: "NULL",
            completed_at: "NULL",
        }),
    )
    .await;

    // DONE status without completed_at MUST fail
    execute_expect_error(
        &pool,
        &insert_task(TaskRow {
            id: task2_id,
            team_id,
            owner_role_id: role_id,
            display_id: "TASK-02",
            title: "Setup Payments",
            status: "DONE",
            cancelled_at: "NULL",
            completed_at: "NULL",
        }),
    )
    .await;

    // CANCELLED status without cancelled_at MUST fail
    execute_expect_error(
        &pool,
        &insert_task(TaskRow {
            id: task2_id,
            team_id,
            owner_role_id: role_id,
            display_id: "TASK-02",
            title: "Setup Payments",
            status: "CANCELLED",
            cancelled_at: "NULL",
            completed_at: "NULL",
        }),
    )
    .await;

    Ok(())
}

#[tokio::test]
async fn migration_enforces_dependency_integrity_and_prevents_cycles() -> Result<(), Box<dyn Error>>
{
    let pool = database().await?;

    let team_id = "01921389d31174628f80cb5f79aa8001";
    let role_id = "01921389d31174628f80cb5f79aa8002";
    let task1_id = "01921389d31174628f80cb5f79aa8011";
    let task2_id = "01921389d31174628f80cb5f79aa8012";

    query(&insert_team(team_id)).execute(&pool).await?;
    query(&insert_role(team_id, role_id, "ROLE-01"))
        .execute(&pool)
        .await?;

    query(&insert_task(TaskRow {
        id: task1_id,
        team_id,
        owner_role_id: role_id,
        display_id: "TASK-01",
        title: "Task 1",
        status: "TODO",
        cancelled_at: "NULL",
        completed_at: "NULL",
    }))
    .execute(&pool)
    .await?;

    query(&insert_task(TaskRow {
        id: task2_id,
        team_id,
        owner_role_id: role_id,
        display_id: "TASK-02",
        title: "Task 2",
        status: "READY",
        cancelled_at: "NULL",
        completed_at: "NULL",
    }))
    .execute(&pool)
    .await?;

    // Self dependency (Task 1 depends on Task 1) MUST fail
    execute_expect_error(
        &pool,
        &insert_dependency(team_id, task1_id, task1_id, "BLOCKING"),
    )
    .await;

    // Task 1 depends on Task 2 succeeds
    query(&insert_dependency(team_id, task1_id, task2_id, "BLOCKING"))
        .execute(&pool)
        .await?;

    // Direct cycle: Task 2 depends on Task 1 MUST fail
    execute_expect_error(
        &pool,
        &insert_dependency(team_id, task2_id, task1_id, "BLOCKING"),
    )
    .await;

    // Foreign key RESTRICT: cannot delete Task 2 while Task 1 depends on it
    execute_expect_error(
        &pool,
        &format!("DELETE FROM tasks WHERE id = X'{task2_id}';"),
    )
    .await;

    Ok(())
}
