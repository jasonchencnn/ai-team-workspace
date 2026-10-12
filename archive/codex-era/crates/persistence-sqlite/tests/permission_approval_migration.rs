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

struct PermissionRow<'a> {
    id: &'a str,
    team_id: &'a str,
    role_id: Option<&'a str>,
    resource_type: &'a str,
    action: &'a str,
    scope_pattern: &'a str,
    effect: &'a str,
    is_human_only: i32,
}

fn insert_permission(row: PermissionRow<'_>) -> String {
    let role_str = match row.role_id {
        Some(r) => format!("X'{r}'"),
        None => "NULL".to_string(),
    };

    format!(
        "INSERT INTO permissions (id, team_id, role_id, resource_type, action, scope_pattern, effect, is_human_only, version, created_at, updated_at) VALUES (X'{}', X'{}', {}, '{}', '{}', '{}', '{}', {}, 1, 1726300123456, 1726300123456);",
        row.id,
        row.team_id,
        role_str,
        row.resource_type,
        row.action,
        row.scope_pattern,
        row.effect,
        row.is_human_only
    )
}

struct ApprovalRow<'a> {
    id: &'a str,
    team_id: &'a str,
    requester_actor_type: &'a str,
    requester_role_id: Option<&'a str>,
    requester_human_id: Option<&'a str>,
    action: &'a str,
    resource_type: &'a str,
    resource_scope: &'a str,
    reason: &'a str,
    status: &'a str,
    decided_by_human_id: Option<&'a str>,
    decision_notes: Option<&'a str>,
    decided_at: Option<i64>,
}

fn insert_approval(row: ApprovalRow<'_>) -> String {
    let role_str = match row.requester_role_id {
        Some(r) => format!("X'{r}'"),
        None => "NULL".to_string(),
    };
    let human_str = match row.requester_human_id {
        Some(h) => format!("X'{h}'"),
        None => "NULL".to_string(),
    };
    let dec_human_str = match row.decided_by_human_id {
        Some(d) => format!("X'{d}'"),
        None => "NULL".to_string(),
    };
    let notes_str = match row.decision_notes {
        Some(n) => format!("'{n}'"),
        None => "NULL".to_string(),
    };
    let dec_at_str = match row.decided_at {
        Some(t) => format!("{t}"),
        None => "NULL".to_string(),
    };

    format!(
        "INSERT INTO approvals (id, team_id, requester_actor_type, requester_role_id, requester_human_id, action, resource_type, resource_scope, reason, status, decided_by_human_id, decision_notes, version, expires_at, created_at, decided_at) VALUES (X'{}', X'{}', '{}', {}, {}, '{}', '{}', '{}', '{}', '{}', {}, {}, 1, 1726300200000, 1726300123456, {});",
        row.id,
        row.team_id,
        row.requester_actor_type,
        role_str,
        human_str,
        row.action,
        row.resource_type,
        row.resource_scope,
        row.reason,
        row.status,
        dec_human_str,
        notes_str,
        dec_at_str
    )
}

#[tokio::test]
async fn empty_database_upgrades_to_permission_approval_schema() -> Result<(), Box<dyn Error>> {
    let pool = database().await?;

    // Migration replay is idempotent
    migrate(&pool).await?;

    let tables: String = query_scalar(
        "SELECT group_concat(name, ',') FROM (SELECT name FROM sqlite_master WHERE type = 'table' AND name IN ('permissions', 'approvals') ORDER BY name);",
    )
    .fetch_one(&pool)
    .await?;

    assert_eq!(tables, "approvals,permissions");
    Ok(())
}

#[tokio::test]
async fn migration_enforces_permission_rules_and_default_deny() -> Result<(), Box<dyn Error>> {
    let pool = database().await?;

    let team_id = "01921389d31174628f80cb5f79aa8001";
    let role_id = "01921389d31174628f80cb5f79aa8002";
    let perm1_id = "01921389d31174628f80cb5f79aa80c1";

    query(&insert_team(team_id)).execute(&pool).await?;
    query(&insert_role(team_id, role_id, "ROLE-01"))
        .execute(&pool)
        .await?;

    // Valid permission succeeds
    query(&insert_permission(PermissionRow {
        id: perm1_id,
        team_id,
        role_id: Some(role_id),
        resource_type: "WORKSPACE",
        action: "UPDATE",
        scope_pattern: "src/**",
        effect: "ALLOW",
        is_human_only: 0,
    }))
    .execute(&pool)
    .await?;

    // Invalid action MUST fail
    let perm2_id = "01921389d31174628f80cb5f79aa80c2";
    execute_expect_error(
        &pool,
        &insert_permission(PermissionRow {
            id: perm2_id,
            team_id,
            role_id: Some(role_id),
            resource_type: "WORKSPACE",
            action: "INVALID_ACTION",
            scope_pattern: "src/**",
            effect: "ALLOW",
            is_human_only: 0,
        }),
    )
    .await;

    // Invalid effect MUST fail
    execute_expect_error(
        &pool,
        &insert_permission(PermissionRow {
            id: perm2_id,
            team_id,
            role_id: Some(role_id),
            resource_type: "WORKSPACE",
            action: "UPDATE",
            scope_pattern: "src/**",
            effect: "MAYBE",
            is_human_only: 0,
        }),
    )
    .await;

    Ok(())
}

#[tokio::test]
async fn migration_enforces_approval_lifecycle_and_self_approval_ban() -> Result<(), Box<dyn Error>>
{
    let pool = database().await?;

    let team_id = "01921389d31174628f80cb5f79aa8001";
    let role_id = "01921389d31174628f80cb5f79aa8002";
    let human1_id = "01921389d31174628f80cb5f79aa8003";
    let human2_id = "01921389d31174628f80cb5f79aa8004";
    let app1_id = "01921389d31174628f80cb5f79aa80d1";
    let app2_id = "01921389d31174628f80cb5f79aa80d2";

    query(&insert_team(team_id)).execute(&pool).await?;
    query(&insert_role(team_id, role_id, "ROLE-01"))
        .execute(&pool)
        .await?;

    // Valid PENDING approval request
    query(&insert_approval(ApprovalRow {
        id: app1_id,
        team_id,
        requester_actor_type: "ROLE",
        requester_role_id: Some(role_id),
        requester_human_id: None,
        action: "write_production_config",
        resource_type: "WORKSPACE_PATH",
        resource_scope: "config/prod.json",
        reason: "Need to update API endpoint",
        status: "PENDING",
        decided_by_human_id: None,
        decision_notes: None,
        decided_at: None,
    }))
    .execute(&pool)
    .await?;

    // Human approves with valid decision metadata
    query(&format!(
        "UPDATE approvals SET status = 'APPROVED', decided_by_human_id = X'{human1_id}', decision_notes = 'Approved for deploy', decided_at = 1726300130000 WHERE id = X'{app1_id}';"
    ))
    .execute(&pool)
    .await?;

    // Rejection without decision_notes MUST FAIL
    let app3_id = "01921389d31174628f80cb5f79aa80d3";
    execute_expect_error(
        &pool,
        &insert_approval(ApprovalRow {
            id: app3_id,
            team_id,
            requester_actor_type: "ROLE",
            requester_role_id: Some(role_id),
            requester_human_id: None,
            action: "delete_database",
            resource_type: "TASK_MUTATION",
            resource_scope: "db/*",
            reason: "Cleanup",
            status: "REJECTED",
            decided_by_human_id: Some(human1_id),
            decision_notes: None,
            decided_at: Some(1726300130000),
        }),
    )
    .await;

    // Self-approval BAN: Human requester cannot approve own request!
    execute_expect_error(
        &pool,
        &insert_approval(ApprovalRow {
            id: app2_id,
            team_id,
            requester_actor_type: "HUMAN",
            requester_role_id: None,
            requester_human_id: Some(human2_id),
            action: "bypass_security_check",
            resource_type: "WORKSPACE_PATH",
            resource_scope: "secrets/*",
            reason: "Emergency fix",
            status: "APPROVED",
            decided_by_human_id: Some(human2_id), // SAME HUMAN!
            decision_notes: Some("Self approved"),
            decided_at: Some(1726300130000),
        }),
    )
    .await;

    Ok(())
}
