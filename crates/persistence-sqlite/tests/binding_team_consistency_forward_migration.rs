use std::borrow::Cow;
use std::error::Error;
use std::future::Future;
use std::pin::Pin;
use std::str::FromStr;

use persistence_sqlite::migrate;
use sqlx_core::error::BoxDynError;
use sqlx_core::migrate::{Migration, MigrationSource, MigrationType, Migrator};
use sqlx_core::query::query;
use sqlx_core::query_scalar::query_scalar;
use sqlx_sqlite::{SqliteConnectOptions, SqlitePool, SqlitePoolOptions};

const LEGACY_MIGRATIONS: [(&str, &str); 9] = [
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
];

#[derive(Debug)]
struct LegacyMigrations;

impl MigrationSource<'static> for LegacyMigrations {
    fn resolve(
        self,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<Migration>, BoxDynError>> + Send + 'static>> {
        Box::pin(async {
            Ok(LEGACY_MIGRATIONS
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
    let options = SqliteConnectOptions::from_str("sqlite::memory:")?.foreign_keys(true);
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(options)
        .await?;
    migrate(&pool).await?;
    Ok(pool)
}

async fn database_before_binding_team_consistency() -> Result<SqlitePool, Box<dyn Error>> {
    let options = SqliteConnectOptions::from_str("sqlite::memory:")?.foreign_keys(true);
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(options)
        .await?;
    Migrator::new(LegacyMigrations).await?.run(&pool).await?;
    Ok(pool)
}

async fn insert_team_graph(
    pool: &SqlitePool,
    team_id: &str,
    suffix: &str,
) -> Result<(), Box<dyn Error>> {
    let role_id = match suffix {
        "a" => "01921389d31174628f80cb5f79aa8002",
        _ => "01921389d31174628f80cb5f79aa8012",
    };
    let harness_id = match suffix {
        "a" => "01921389d31174628f80cb5f79aa8003",
        _ => "01921389d31174628f80cb5f79aa8013",
    };
    let adapter_id = match suffix {
        "a" => "01921389d31174628f80cb5f79aa8004",
        _ => "01921389d31174628f80cb5f79aa8014",
    };

    query(&format!(
        "INSERT INTO teams (id, name, status, version, created_at, updated_at, archived_at) VALUES (X'{team_id}', 'Team {suffix}', 'ACTIVE', 1, 1726300123456, 1726300123456, NULL);"
    ))
    .execute(pool)
    .await?;
    query(&format!(
        "INSERT INTO roles (id, team_id, display_id, name, status, version, created_at, updated_at, deactivated_at) VALUES (X'{role_id}', X'{team_id}', 'DEV-{suffix}', 'Developer', 'ACTIVE', 1, 1726300123456, 1726300123456, NULL);"
    ))
    .execute(pool)
    .await?;
    query(&format!(
        "INSERT INTO harness_instances (id, team_id, harness_type, display_name, status, capabilities, credential_reference, version, created_at, updated_at, disabled_at) VALUES (X'{harness_id}', X'{team_id}', 'codex', 'Codex {suffix}', 'ACTIVE', '[]', NULL, 1, 1726300123456, 1726300123456, NULL);"
    ))
    .execute(pool)
    .await?;
    query(&format!(
        "INSERT INTO adapters (id, team_id, name, protocol_version, executable_path, manifest_json, version, created_at, updated_at) VALUES (X'{adapter_id}', X'{team_id}', 'adapter-{suffix}', '0.1.0', '/bin/adapter', '{{}}', 1, 1726300123456, 1726300123456);"
    ))
    .execute(pool)
    .await?;
    Ok(())
}

struct BindingRow<'a> {
    binding_id: &'a str,
    team_id: &'a str,
    role_id: &'a str,
    harness_id: &'a str,
    adapter_id: &'a str,
    status: &'a str,
    generation: i64,
    effective_from: &'a str,
    effective_to: &'a str,
}

fn binding_insert(row: BindingRow<'_>) -> String {
    format!(
        "INSERT INTO bindings (id, team_id, role_id, harness_instance_id, adapter_id, status, binding_generation, version, configuration_snapshot, created_at, updated_at, effective_from, effective_to) VALUES (X'{}', X'{}', X'{}', X'{}', X'{}', '{}', {}, 1, '{{}}', 1726300123456, 1726300123456, {}, {});",
        row.binding_id,
        row.team_id,
        row.role_id,
        row.harness_id,
        row.adapter_id,
        row.status,
        row.generation,
        row.effective_from,
        row.effective_to,
    )
}

async fn binding_count(pool: &SqlitePool) -> Result<i64, Box<dyn Error>> {
    Ok(query_scalar("SELECT COUNT(*) FROM bindings")
        .fetch_one(pool)
        .await?)
}

#[tokio::test]
async fn cross_team_binding_insert_is_rejected() -> Result<(), Box<dyn Error>> {
    let pool = database().await?;
    let team_a = "01921389d31174628f80cb5f79aa8001";
    let team_b = "01921389d31174628f80cb5f79aa8011";
    insert_team_graph(&pool, team_a, "a").await?;
    insert_team_graph(&pool, team_b, "b").await?;

    for (role_id, harness_id, adapter_id, binding_team_id) in [
        (
            "01921389d31174628f80cb5f79aa8012",
            "01921389d31174628f80cb5f79aa8003",
            "01921389d31174628f80cb5f79aa8004",
            team_a,
        ),
        (
            "01921389d31174628f80cb5f79aa8002",
            "01921389d31174628f80cb5f79aa8013",
            "01921389d31174628f80cb5f79aa8004",
            team_a,
        ),
        (
            "01921389d31174628f80cb5f79aa8002",
            "01921389d31174628f80cb5f79aa8003",
            "01921389d31174628f80cb5f79aa8014",
            team_a,
        ),
        (
            "01921389d31174628f80cb5f79aa8002",
            "01921389d31174628f80cb5f79aa8003",
            "01921389d31174628f80cb5f79aa8004",
            team_b,
        ),
    ] {
        let result = query(&binding_insert(BindingRow {
            binding_id: "01921389d31174628f80cb5f79aa8005",
            team_id: binding_team_id,
            role_id,
            harness_id,
            adapter_id,
            status: "ACTIVE",
            generation: 1,
            effective_from: "1726300123456",
            effective_to: "NULL",
        }))
        .execute(&pool)
        .await;
        assert!(
            result.is_err(),
            "cross-Team binding insert unexpectedly succeeded"
        );
        assert_eq!(
            binding_count(&pool).await?,
            0,
            "failed insert wrote a Binding"
        );
    }
    Ok(())
}

#[tokio::test]
async fn cross_team_binding_update_is_rejected_without_side_effects() -> Result<(), Box<dyn Error>>
{
    let pool = database().await?;
    let team_a = "01921389d31174628f80cb5f79aa8001";
    let team_b = "01921389d31174628f80cb5f79aa8011";
    let binding_id = "01921389d31174628f80cb5f79aa8005";
    insert_team_graph(&pool, team_a, "a").await?;
    insert_team_graph(&pool, team_b, "b").await?;
    query(&binding_insert(BindingRow {
        binding_id,
        team_id: team_a,
        role_id: "01921389d31174628f80cb5f79aa8002",
        harness_id: "01921389d31174628f80cb5f79aa8003",
        adapter_id: "01921389d31174628f80cb5f79aa8004",
        status: "ACTIVE",
        generation: 1,
        effective_from: "1726300123456",
        effective_to: "NULL",
    }))
    .execute(&pool)
    .await?;

    for update in [
        format!("UPDATE bindings SET team_id = X'{team_b}' WHERE id = X'{binding_id}'"),
        format!(
            "UPDATE bindings SET role_id = X'01921389d31174628f80cb5f79aa8012' WHERE id = X'{binding_id}'"
        ),
        format!(
            "UPDATE bindings SET harness_instance_id = X'01921389d31174628f80cb5f79aa8013' WHERE id = X'{binding_id}'"
        ),
        format!(
            "UPDATE bindings SET adapter_id = X'01921389d31174628f80cb5f79aa8014' WHERE id = X'{binding_id}'"
        ),
        format!(
            "UPDATE roles SET team_id = X'{team_b}' WHERE id = X'01921389d31174628f80cb5f79aa8002'"
        ),
        format!(
            "UPDATE harness_instances SET team_id = X'{team_b}' WHERE id = X'01921389d31174628f80cb5f79aa8003'"
        ),
        format!(
            "UPDATE adapters SET team_id = X'{team_b}' WHERE id = X'01921389d31174628f80cb5f79aa8004'"
        ),
    ] {
        assert!(
            query(&update).execute(&pool).await.is_err(),
            "cross-Team update unexpectedly succeeded: {update}"
        );
        let stored_team: String = query_scalar(&format!(
            "SELECT hex(team_id) FROM bindings WHERE id = X'{binding_id}'"
        ))
        .fetch_one(&pool)
        .await?;
        assert_eq!(stored_team, team_a.to_uppercase());
    }
    Ok(())
}

#[tokio::test]
async fn same_team_binding_insert_update_and_historical_ranges_remain_valid()
-> Result<(), Box<dyn Error>> {
    let pool = database().await?;
    let team_id = "01921389d31174628f80cb5f79aa8001";
    let role_id = "01921389d31174628f80cb5f79aa8002";
    let harness_id = "01921389d31174628f80cb5f79aa8003";
    let adapter_id = "01921389d31174628f80cb5f79aa8004";
    let binding_id = "01921389d31174628f80cb5f79aa8005";
    let next_binding_id = "01921389d31174628f80cb5f79aa8008";
    insert_team_graph(&pool, team_id, "a").await?;
    query(&binding_insert(BindingRow {
        binding_id,
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
    query(&binding_insert(BindingRow {
        binding_id: next_binding_id,
        team_id,
        role_id,
        harness_id,
        adapter_id,
        status: "ACTIVE",
        generation: 2,
        effective_from: "2001",
        effective_to: "NULL",
    }))
    .execute(&pool)
    .await?;
    query(&format!(
        "UPDATE bindings SET configuration_snapshot = '{{\"model\":\"updated\"}}', version = 2, updated_at = 1726300123457 WHERE id = X'{next_binding_id}'"
    ))
    .execute(&pool)
    .await?;
    assert_eq!(binding_count(&pool).await?, 2);
    Ok(())
}

#[tokio::test]
async fn upgrade_fails_without_deleting_an_existing_cross_team_binding()
-> Result<(), Box<dyn Error>> {
    let pool = database_before_binding_team_consistency().await?;
    let team_a = "01921389d31174628f80cb5f79aa8001";
    let team_b = "01921389d31174628f80cb5f79aa8011";
    insert_team_graph(&pool, team_a, "a").await?;
    insert_team_graph(&pool, team_b, "b").await?;
    query(&binding_insert(BindingRow {
        binding_id: "01921389d31174628f80cb5f79aa8005",
        team_id: team_a,
        role_id: "01921389d31174628f80cb5f79aa8012",
        harness_id: "01921389d31174628f80cb5f79aa8003",
        adapter_id: "01921389d31174628f80cb5f79aa8004",
        status: "ACTIVE",
        generation: 1,
        effective_from: "1726300123456",
        effective_to: "NULL",
    }))
    .execute(&pool)
    .await?;

    assert!(
        migrate(&pool).await.is_err(),
        "dirty legacy database unexpectedly upgraded"
    );
    assert_eq!(
        binding_count(&pool).await?,
        1,
        "failed upgrade deleted historical data"
    );
    let upgrade_artifacts: i64 = query_scalar(
        "SELECT COUNT(*) FROM sqlite_master WHERE name = 'binding_team_consistency_upgrade_guard'",
    )
    .fetch_one(&pool)
    .await?;
    assert_eq!(
        upgrade_artifacts, 0,
        "failed upgrade left partial schema behind"
    );
    Ok(())
}

#[tokio::test]
async fn valid_v9_history_upgrades_and_enforces_team_consistency_afterward()
-> Result<(), Box<dyn Error>> {
    // database_before_binding_team_consistency configures this connection with
    // foreign_keys=ON, which is required because SQLite foreign-key enforcement
    // is connection-local.
    let pool = database_before_binding_team_consistency().await?;
    let foreign_keys: i64 = query_scalar("PRAGMA foreign_keys").fetch_one(&pool).await?;
    assert_eq!(foreign_keys, 1, "test requires SQLite foreign_keys=ON");

    let team_a = "01921389d31174628f80cb5f79aa8001";
    let team_b = "01921389d31174628f80cb5f79aa8011";
    let role_a = "01921389d31174628f80cb5f79aa8002";
    let harness_a = "01921389d31174628f80cb5f79aa8003";
    let adapter_a = "01921389d31174628f80cb5f79aa8004";
    let historical_binding = "01921389d31174628f80cb5f79aa8005";
    let active_binding = "01921389d31174628f80cb5f79aa8008";
    let replacement_harness = "01921389d31174628f80cb5f79aa8006";
    let replacement_adapter = "01921389d31174628f80cb5f79aa8007";

    insert_team_graph(&pool, team_a, "a").await?;
    insert_team_graph(&pool, team_b, "b").await?;
    query(&binding_insert(BindingRow {
        binding_id: historical_binding,
        team_id: team_a,
        role_id: role_a,
        harness_id: harness_a,
        adapter_id: adapter_a,
        status: "REVOKED",
        generation: 1,
        effective_from: "1000",
        effective_to: "2000",
    }))
    .execute(&pool)
    .await?;
    query(&binding_insert(BindingRow {
        binding_id: active_binding,
        team_id: team_a,
        role_id: role_a,
        harness_id: harness_a,
        adapter_id: adapter_a,
        status: "ACTIVE",
        generation: 2,
        effective_from: "2001",
        effective_to: "NULL",
    }))
    .execute(&pool)
    .await?;

    migrate(&pool).await?;

    let applied_migration: i64 =
        query_scalar("SELECT COUNT(*) FROM _sqlx_migrations WHERE version = 10 AND success = TRUE")
            .fetch_one(&pool)
            .await?;
    assert_eq!(applied_migration, 1, "migration 0010 was not recorded");
    let triggers: i64 = query_scalar(
        "SELECT COUNT(*) FROM sqlite_master WHERE type = 'trigger' AND name IN (\
            'bindings_team_consistency_insert', \
            'bindings_team_consistency_update', \
            'roles_binding_team_consistency_update', \
            'harness_instances_binding_team_consistency_update', \
            'adapters_binding_team_consistency_update'\
        )",
    )
    .fetch_one(&pool)
    .await?;
    assert_eq!(triggers, 5, "migration 0010 did not install every trigger");

    query(&format!(
        "INSERT INTO harness_instances (id, team_id, harness_type, display_name, status, capabilities, credential_reference, version, created_at, updated_at, disabled_at) VALUES (X'{replacement_harness}', X'{team_a}', 'codex', 'Codex replacement', 'ACTIVE', '[]', NULL, 1, 1726300123456, 1726300123456, NULL);"
    ))
    .execute(&pool)
    .await?;
    query(&format!(
        "INSERT INTO adapters (id, team_id, name, protocol_version, executable_path, manifest_json, version, created_at, updated_at) VALUES (X'{replacement_adapter}', X'{team_a}', 'adapter-replacement', '0.1.0', '/bin/adapter', '{{}}', 1, 1726300123456, 1726300123456);"
    ))
    .execute(&pool)
    .await?;
    query(&format!(
        "UPDATE bindings SET harness_instance_id = X'{replacement_harness}', adapter_id = X'{replacement_adapter}', version = 2, updated_at = 1726300123457 WHERE id = X'{active_binding}'"
    ))
    .execute(&pool)
    .await?;

    let illegal_update = query(&format!(
        "UPDATE bindings SET adapter_id = X'01921389d31174628f80cb5f79aa8014' WHERE id = X'{active_binding}'"
    ))
    .execute(&pool)
    .await;
    assert!(
        illegal_update.is_err(),
        "cross-Team update after a successful forward upgrade unexpectedly succeeded"
    );
    assert_eq!(
        binding_count(&pool).await?,
        2,
        "failed update changed row count"
    );
    let stored_adapter: String = query_scalar(&format!(
        "SELECT hex(adapter_id) FROM bindings WHERE id = X'{active_binding}'"
    ))
    .fetch_one(&pool)
    .await?;
    assert_eq!(stored_adapter, replacement_adapter.to_uppercase());
    Ok(())
}
