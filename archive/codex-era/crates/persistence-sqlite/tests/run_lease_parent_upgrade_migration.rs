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

const V11: [(&str, &str); 11] = [
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
    (
        "binding team consistency",
        include_str!("../migrations/0010_binding_team_consistency.sql"),
    ),
    (
        "task dependency consistency",
        include_str!("../migrations/0011_task_dependency_consistency.sql"),
    ),
];

#[derive(Debug)]
struct V11Migrations;

impl MigrationSource<'static> for V11Migrations {
    fn resolve(
        self,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<Migration>, BoxDynError>> + Send + 'static>> {
        Box::pin(async {
            Ok(V11
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

async fn v11_database() -> Result<SqlitePool, Box<dyn Error>> {
    let options = SqliteConnectOptions::from_str("sqlite::memory:")?.foreign_keys(true);
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(options)
        .await?;
    Migrator::new(V11Migrations).await?.run(&pool).await?;
    Ok(pool)
}

fn id(seed: u16, offset: u8) -> String {
    format!("01921389d31174628f80cb5f79aa{:02x}{offset:02x}", seed)
}

struct Graph {
    team: String,
    role: String,
    reviewer: String,
    harness: String,
    alternate_harness: String,
    adapter: String,
    binding: String,
    task: String,
    alternate_task: String,
    active_run: String,
    reviewed_run: String,
}

async fn insert_graph(pool: &SqlitePool, seed: u16) -> Result<Graph, Box<dyn Error>> {
    let graph = Graph {
        team: id(seed, 1),
        role: id(seed, 2),
        reviewer: id(seed, 3),
        harness: id(seed, 4),
        alternate_harness: id(seed, 5),
        adapter: id(seed, 6),
        binding: id(seed, 7),
        task: id(seed, 8),
        alternate_task: id(seed, 9),
        active_run: id(seed, 10),
        reviewed_run: id(seed, 11),
    };
    for statement in [
        format!(
            "INSERT INTO teams VALUES(X'{}', 'Team {seed}', 'ACTIVE', 1, 1, 1, NULL)",
            graph.team
        ),
        format!(
            "INSERT INTO roles VALUES(X'{}', X'{}', 'OWNER-{seed}', 'Owner', 'ACTIVE', 1, 1, 1, NULL)",
            graph.role, graph.team
        ),
        format!(
            "INSERT INTO roles VALUES(X'{}', X'{}', 'REVIEW-{seed}', 'Reviewer', 'ACTIVE', 1, 1, 1, NULL)",
            graph.reviewer, graph.team
        ),
        format!(
            "INSERT INTO harness_instances VALUES(X'{}', X'{}', 'mock', 'Harness', 'ACTIVE', '[]', NULL, 1, 1, 1, NULL)",
            graph.harness, graph.team
        ),
        format!(
            "INSERT INTO harness_instances VALUES(X'{}', X'{}', 'mock', 'Alternate harness', 'ACTIVE', '[]', NULL, 1, 1, 1, NULL)",
            graph.alternate_harness, graph.team
        ),
        format!(
            "INSERT INTO adapters VALUES(X'{}', X'{}', 'adapter-{seed}', '1', '/adapter', '{{}}', 1, 1, 1)",
            graph.adapter, graph.team
        ),
        format!(
            "INSERT INTO bindings VALUES(X'{}', X'{}', X'{}', X'{}', X'{}', 'ACTIVE', 1, 1, '{{}}', 1, 1, 1, NULL)",
            graph.binding, graph.team, graph.role, graph.harness, graph.adapter
        ),
        format!(
            "INSERT INTO tasks VALUES(X'{}', X'{}', X'{}', 'TASK-{seed}', 'Task', '', 'DOING', 1, 1, 1, NULL, NULL)",
            graph.task, graph.team, graph.role
        ),
        format!(
            "INSERT INTO tasks VALUES(X'{}', X'{}', X'{}', 'ALT-{seed}', 'Alternate task', '', 'DOING', 1, 1, 1, NULL, NULL)",
            graph.alternate_task, graph.team, graph.role
        ),
        format!(
            "INSERT INTO runs VALUES(X'{}', X'{}', X'{}', X'{}', X'{}', X'{}', 1, 1, 'SUCCEEDED', 0, NULL, 1, 1, 1, 2)",
            graph.reviewed_run, graph.team, graph.task, graph.role, graph.binding, graph.harness
        ),
        format!(
            "INSERT INTO reviews VALUES(X'{}', X'{}', X'{}', X'{}', X'{}', 'APPROVED', NULL, 1, 2)",
            id(seed, 12),
            graph.team,
            graph.task,
            graph.reviewed_run,
            graph.reviewer
        ),
        format!(
            "INSERT INTO runs VALUES(X'{}', X'{}', X'{}', X'{}', X'{}', X'{}', 2, 2, 'RUNNING', NULL, NULL, 1, 1, 1, NULL)",
            graph.active_run, graph.team, graph.task, graph.role, graph.binding, graph.harness
        ),
        format!(
            "INSERT INTO leases VALUES(X'{}', X'{}', 2, 1, 2, 1)",
            graph.task, graph.active_run
        ),
    ] {
        query(&statement).execute(pool).await?;
    }
    Ok(graph)
}

async fn count(pool: &SqlitePool, table: &str) -> Result<i64, Box<dyn Error>> {
    Ok(query_scalar(&format!("SELECT COUNT(*) FROM {table}"))
        .fetch_one(pool)
        .await?)
}

#[tokio::test]
async fn valid_v11_run_lease_review_history_upgrades_to_0012_and_installs_guards()
-> Result<(), Box<dyn Error>> {
    let pool = v11_database().await?;
    let graph = insert_graph(&pool, 0x91).await?;

    migrate(&pool).await?;

    assert_eq!(
        query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM _sqlx_migrations WHERE version = 12 AND success = TRUE",
        )
        .fetch_one(&pool)
        .await?,
        1
    );
    assert_eq!(count(&pool, "runs").await?, 2);
    assert_eq!(count(&pool, "leases").await?, 1);
    assert_eq!(count(&pool, "reviews").await?, 1);
    assert_eq!(
        query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM sqlite_master WHERE type = 'trigger' AND name IN (\
                'runs_consistency_insert', 'runs_consistency_update', \
                'leases_consistency_insert', 'leases_consistency_update', \
                'reviews_consistency_insert', 'reviews_consistency_update'\
            )",
        )
        .fetch_one(&pool)
        .await?,
        6
    );
    assert!(
        query(&format!(
            "INSERT INTO leases VALUES(X'{}', X'{}', 2, 1, 2, 1)",
            graph.alternate_task, graph.reviewed_run
        ))
        .execute(&pool)
        .await
        .is_err()
    );
    Ok(())
}

#[tokio::test]
async fn terminal_run_history_allows_same_team_task_reassignment_after_lease_release()
-> Result<(), Box<dyn Error>> {
    let pool = v11_database().await?;
    let graph = insert_graph(&pool, 0xa1).await?;
    query(&format!(
        "DELETE FROM leases WHERE task_id = X'{}'",
        graph.task
    ))
    .execute(&pool)
    .await?;
    query(&format!(
        "UPDATE runs SET status = 'SUCCEEDED', completed_at = 2 WHERE id = X'{}'",
        graph.active_run
    ))
    .execute(&pool)
    .await?;
    migrate(&pool).await?;
    query(&format!(
        "UPDATE tasks SET owner_role_id = X'{}' WHERE id = X'{}'",
        graph.reviewer, graph.task
    ))
    .execute(&pool)
    .await?;
    let historical_role: String = query_scalar(&format!(
        "SELECT hex(role_id) FROM runs WHERE id = X'{}'",
        graph.active_run
    ))
    .fetch_one(&pool)
    .await?;
    assert_eq!(historical_role, graph.role.to_uppercase());
    Ok(())
}

#[tokio::test]
async fn dirty_v11_run_lease_and_review_histories_fail_upgrade_atomically()
-> Result<(), Box<dyn Error>> {
    for case in [
        "run",
        "lease-task",
        "lease-token",
        "review-team",
        "review-task",
        "generation-order",
        "review-status",
    ] {
        let pool = v11_database().await?;
        let a = insert_graph(&pool, 0xa1).await?;
        let b = insert_graph(&pool, 0xb1).await?;
        match case {
            "run" => {
                query(&format!(
                    "UPDATE runs SET role_id = X'{}' WHERE id = X'{}'",
                    b.role, a.active_run
                ))
                .execute(&pool)
                .await?
            }
            "lease-task" => {
                query(&format!(
                    "UPDATE leases SET task_id = X'{}' WHERE run_id = X'{}'",
                    a.alternate_task, a.active_run
                ))
                .execute(&pool)
                .await?
            }
            "lease-token" => {
                query(&format!(
                    "UPDATE leases SET fencing_token = 9 WHERE run_id = X'{}'",
                    a.active_run
                ))
                .execute(&pool)
                .await?
            }
            "review-team" => {
                query(&format!(
                    "UPDATE reviews SET team_id = X'{}' WHERE run_id = X'{}'",
                    b.team, a.reviewed_run
                ))
                .execute(&pool)
                .await?
            }
            "review-task" => {
                query(&format!(
                    "UPDATE reviews SET task_id = X'{}' WHERE run_id = X'{}'",
                    a.alternate_task, a.reviewed_run
                ))
                .execute(&pool)
                .await?
            }
            "generation-order" => {
                query(&format!(
                    "UPDATE runs SET attempt = 3, fencing_token = 3 WHERE id = X'{}'",
                    a.reviewed_run
                ))
                .execute(&pool)
                .await?
            }
            _ => {
                query(&format!(
                    "UPDATE runs SET status = 'FAILED' WHERE id = X'{}'",
                    a.reviewed_run
                ))
                .execute(&pool)
                .await?
            }
        };
        let rows = (
            count(&pool, "runs").await?,
            count(&pool, "leases").await?,
            count(&pool, "reviews").await?,
        );
        assert!(
            migrate(&pool).await.is_err(),
            "dirty {case} history upgraded"
        );
        assert_eq!(
            (
                count(&pool, "runs").await?,
                count(&pool, "leases").await?,
                count(&pool, "reviews").await?
            ),
            rows,
            "failed {case} upgrade changed historical rows"
        );
        assert_eq!(
            query_scalar::<_, i64>("SELECT COUNT(*) FROM _sqlx_migrations WHERE version = 12")
                .fetch_one(&pool)
                .await?,
            0
        );
        assert_eq!(
            query_scalar::<_, i64>(
                "SELECT COUNT(*) FROM sqlite_master WHERE name = 'run_lease_consistency_guard'"
            )
            .fetch_one(&pool)
            .await?,
            0
        );
    }
    Ok(())
}

#[tokio::test]
async fn parent_updates_cannot_break_existing_run_lease_or_review_relationships()
-> Result<(), Box<dyn Error>> {
    let parent_updates = [
        (
            "task owner",
            "UPDATE tasks SET owner_role_id = X'{reviewer}' WHERE id = X'{task}'",
        ),
        (
            "binding role",
            "UPDATE bindings SET role_id = X'{reviewer}' WHERE id = X'{binding}'",
        ),
        (
            "binding harness",
            "UPDATE bindings SET harness_instance_id = X'{alternate_harness}' WHERE id = X'{binding}'",
        ),
        (
            "run fencing token",
            "UPDATE runs SET fencing_token = 9 WHERE id = X'{active_run}'",
        ),
        (
            "run task",
            "UPDATE runs SET task_id = X'{alternate_task}' WHERE id = X'{reviewed_run}'",
        ),
    ];
    for (name, template) in parent_updates {
        let pool = v11_database().await?;
        let graph = insert_graph(&pool, 0xc1).await?;
        migrate(&pool).await?;
        let statement = template
            .replace("{reviewer}", &graph.reviewer)
            .replace("{task}", &graph.task)
            .replace("{binding}", &graph.binding)
            .replace("{alternate_harness}", &graph.alternate_harness)
            .replace("{active_run}", &graph.active_run)
            .replace("{alternate_task}", &graph.alternate_task)
            .replace("{reviewed_run}", &graph.reviewed_run);
        assert!(
            query(&statement).execute(&pool).await.is_err(),
            "{name} update broke existing Run/Lease/Review rows"
        );
        assert_eq!(
            count(&pool, "runs").await?,
            2,
            "{name} update changed Run rows"
        );
        assert_eq!(
            count(&pool, "leases").await?,
            1,
            "{name} update changed Lease rows"
        );
        assert_eq!(
            count(&pool, "reviews").await?,
            1,
            "{name} update changed Review rows"
        );
    }
    Ok(())
}
