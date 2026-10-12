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

fn insert_run(
    team_id: &str,
    task_id: &str,
    role_id: &str,
    binding_id: &str,
    harness_id: &str,
    run_id: &str,
) -> String {
    format!(
        "INSERT INTO runs (id, team_id, task_id, role_id, binding_id, harness_instance_id, attempt, fencing_token, status, exit_code, failure_reason, version, created_at, started_at, completed_at) VALUES (X'{run_id}', X'{team_id}', X'{task_id}', X'{role_id}', X'{binding_id}', X'{harness_id}', 1, 1, 'RUNNING', NULL, NULL, 1, 1726300123456, 1726300123456, NULL);"
    )
}

struct ArtifactRow<'a> {
    id: &'a str,
    team_id: &'a str,
    task_id: &'a str,
    run_id: &'a str,
    creator_role_id: &'a str,
    artifact_type: &'a str,
    uri: &'a str,
    checksum_sha256: &'a str,
    git_commit_sha: &'a str,
    visibility: &'a str,
}

fn insert_artifact(row: ArtifactRow<'_>) -> String {
    format!(
        "INSERT INTO artifacts (id, team_id, task_id, run_id, creator_role_id, artifact_type, uri, checksum_sha256, git_commit_sha, visibility, version, created_at) VALUES (X'{}', X'{}', X'{}', X'{}', X'{}', '{}', '{}', {}, {}, '{}', 1, 1726300123456);",
        row.id,
        row.team_id,
        row.task_id,
        row.run_id,
        row.creator_role_id,
        row.artifact_type,
        row.uri,
        row.checksum_sha256,
        row.git_commit_sha,
        row.visibility
    )
}

fn insert_reference(
    id: &str,
    team_id: &str,
    subject_type: &str,
    subject_id: &str,
    ref_kind: &str,
    target_uri: &str,
    status: &str,
) -> String {
    format!(
        "INSERT INTO \"references\" (id, team_id, subject_type, subject_id, reference_kind, target_uri, validation_status, checksum_sha256, version, created_at, validated_at) VALUES (X'{id}', X'{team_id}', '{subject_type}', X'{subject_id}', '{ref_kind}', '{target_uri}', '{status}', NULL, 1, 1726300123456, 1726300123456);"
    )
}

#[tokio::test]
async fn empty_database_upgrades_to_artifact_and_reference_schema() -> Result<(), Box<dyn Error>> {
    let pool = database().await?;

    // Migration replay is idempotent
    migrate(&pool).await?;

    let tables: String = query_scalar(
        "SELECT group_concat(name, ',') FROM (SELECT name FROM sqlite_master WHERE type = 'table' AND name IN ('artifacts', 'references') ORDER BY name);",
    )
    .fetch_one(&pool)
    .await?;

    assert_eq!(tables, "artifacts,references");
    Ok(())
}

#[tokio::test]
async fn migration_enforces_artifact_traceability_and_visibility() -> Result<(), Box<dyn Error>> {
    let pool = database().await?;

    let team_id = "01921389d31174628f80cb5f79aa8001";
    let role_id = "01921389d31174628f80cb5f79aa8002";
    let harness_id = "01921389d31174628f80cb5f79aa8003";
    let adapter_id = "01921389d31174628f80cb5f79aa8004";
    let binding_id = "01921389d31174628f80cb5f79aa8005";
    let task_id = "01921389d31174628f80cb5f79aa8006";
    let run_id = "01921389d31174628f80cb5f79aa8007";
    let artifact1_id = "01921389d31174628f80cb5f79aa8031";

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
    query(&insert_run(
        team_id, task_id, role_id, binding_id, harness_id, run_id,
    ))
    .execute(&pool)
    .await?;

    let valid_sha = "'e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855'";
    let valid_commit = "'1a14b8795e76fc373bef326eb5df96f9ac173f71'";

    // Valid artifact succeeds
    query(&insert_artifact(ArtifactRow {
        id: artifact1_id,
        team_id,
        task_id,
        run_id,
        creator_role_id: role_id,
        artifact_type: "DOCUMENT",
        uri: "artifact://reports/summary.md",
        checksum_sha256: valid_sha,
        git_commit_sha: valid_commit,
        visibility: "TEAM_INTERNAL",
    }))
    .execute(&pool)
    .await?;

    // Invalid artifact_type MUST fail
    let artifact2_id = "01921389d31174628f80cb5f79aa8032";
    execute_expect_error(
        &pool,
        &insert_artifact(ArtifactRow {
            id: artifact2_id,
            team_id,
            task_id,
            run_id,
            creator_role_id: role_id,
            artifact_type: "INVALID_TYPE",
            uri: "artifact://reports/invalid.md",
            checksum_sha256: "NULL",
            git_commit_sha: "NULL",
            visibility: "TEAM_INTERNAL",
        }),
    )
    .await;

    // Invalid checksum (not 64 hex) MUST fail
    execute_expect_error(
        &pool,
        &insert_artifact(ArtifactRow {
            id: artifact2_id,
            team_id,
            task_id,
            run_id,
            creator_role_id: role_id,
            artifact_type: "DOCUMENT",
            uri: "artifact://reports/invalid.md",
            checksum_sha256: "'not-a-valid-sha256'",
            git_commit_sha: "NULL",
            visibility: "TEAM_INTERNAL",
        }),
    )
    .await;

    // Foreign key RESTRICT: cannot delete Run when Artifact exists
    execute_expect_error(&pool, &format!("DELETE FROM runs WHERE id = X'{run_id}';")).await;

    Ok(())
}

#[tokio::test]
async fn migration_enforces_reference_integrity() -> Result<(), Box<dyn Error>> {
    let pool = database().await?;

    let team_id = "01921389d31174628f80cb5f79aa8001";
    let role_id = "01921389d31174628f80cb5f79aa8002";
    let task_id = "01921389d31174628f80cb5f79aa8006";
    let ref1_id = "01921389d31174628f80cb5f79aa8041";

    query(&insert_team(team_id)).execute(&pool).await?;
    query(&insert_role(team_id, role_id, "ROLE-01"))
        .execute(&pool)
        .await?;
    query(&insert_task(team_id, role_id, task_id, "TASK-01"))
        .execute(&pool)
        .await?;

    // Valid workspace reference succeeds
    query(&insert_reference(
        ref1_id,
        team_id,
        "TASK",
        task_id,
        "WORKSPACE_PATH",
        "docs/specs/DOMAIN_MODEL.md",
        "VALID",
    ))
    .execute(&pool)
    .await?;

    // Invalid subject_type MUST fail
    let ref2_id = "01921389d31174628f80cb5f79aa8042";
    execute_expect_error(
        &pool,
        &insert_reference(
            ref2_id,
            team_id,
            "INVALID_SUBJECT",
            task_id,
            "WORKSPACE_PATH",
            "docs/specs/DOMAIN_MODEL.md",
            "VALID",
        ),
    )
    .await;

    // Invalid validation_status MUST fail
    execute_expect_error(
        &pool,
        &insert_reference(
            ref2_id,
            team_id,
            "TASK",
            task_id,
            "WORKSPACE_PATH",
            "docs/specs/DOMAIN_MODEL.md",
            "NON_EXISTENT_STATUS",
        ),
    )
    .await;

    Ok(())
}
