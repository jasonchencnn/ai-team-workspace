use std::borrow::Cow;
use std::error::Error;
use std::future::Future;
use std::pin::Pin;
use std::str::FromStr;

use persistence_sqlite::migrate;
use sqlx_core::query::query;
use sqlx_core::query_scalar::query_scalar;
use sqlx_core::{
    error::BoxDynError,
    migrate::{Migration, MigrationSource, MigrationType, Migrator},
};
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

const V10: [(&str, &str); 10] = [
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
];
#[derive(Debug)]
struct V10Migrations;
impl MigrationSource<'static> for V10Migrations {
    fn resolve(
        self,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<Migration>, BoxDynError>> + Send + 'static>> {
        Box::pin(async {
            Ok(V10
                .iter()
                .enumerate()
                .map(|(i, (d, s))| {
                    Migration::new(
                        (i + 1) as i64,
                        Cow::Borrowed(*d),
                        MigrationType::Simple,
                        Cow::Borrowed(*s),
                        false,
                    )
                })
                .collect())
        })
    }
}
async fn v10_database() -> Result<SqlitePool, Box<dyn Error>> {
    let options = SqliteConnectOptions::from_str("sqlite::memory:")?.foreign_keys(true);
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(options)
        .await?;
    Migrator::new(V10Migrations).await?.run(&pool).await?;
    Ok(pool)
}

fn task(id: &str, team: &str, owner: &str, display_id: &str) -> String {
    format!(
        "INSERT INTO tasks (id, team_id, owner_role_id, display_id, title, description, status, version, created_at, updated_at, cancelled_at, completed_at) VALUES (X'{id}', X'{team}', X'{owner}', '{display_id}', 'Task', '', 'TODO', 1, 1, 1, NULL, NULL);"
    )
}

fn dependency(team: &str, task_id: &str, prerequisite: &str) -> String {
    format!(
        "INSERT INTO task_dependencies (team_id, task_id, depends_on_task_id, dependency_type, created_at) VALUES (X'{team}', X'{task_id}', X'{prerequisite}', 'BLOCKING', 1);"
    )
}

async fn graph(
    pool: &SqlitePool,
    team: &str,
    role: &str,
    start: u8,
) -> Result<[String; 4], Box<dyn Error>> {
    query(&format!("INSERT INTO teams (id, name, status, version, created_at, updated_at, archived_at) VALUES (X'{team}', 'Team', 'ACTIVE', 1, 1, 1, NULL);"))
        .execute(pool).await?;
    query(&format!("INSERT INTO roles (id, team_id, display_id, name, status, version, created_at, updated_at, deactivated_at) VALUES (X'{role}', X'{team}', 'ROLE-{start}', 'Role', 'ACTIVE', 1, 1, 1, NULL);"))
        .execute(pool).await?;
    let ids =
        [1_u8, 2, 3, 4].map(|offset| format!("01921389d31174628f80cb5f79aa{:04x}", start + offset));
    for (index, id) in ids.iter().enumerate() {
        query(&task(id, team, role, &format!("T-{start}-{index}")))
            .execute(pool)
            .await?;
    }
    Ok(ids)
}

#[tokio::test]
async fn cross_team_task_owner_is_rejected() -> Result<(), Box<dyn Error>> {
    let pool = database().await?;
    for (team_id, role_id) in [
        (
            "01921389d31174628f80cb5f79aa8101",
            "01921389d31174628f80cb5f79aa8102",
        ),
        (
            "01921389d31174628f80cb5f79aa8111",
            "01921389d31174628f80cb5f79aa8112",
        ),
    ] {
        query(&format!("INSERT INTO teams (id, name, status, version, created_at, updated_at, archived_at) VALUES (X'{team_id}', 'Team', 'ACTIVE', 1, 1, 1, NULL);"))
            .execute(&pool)
            .await?;
        query(&format!("INSERT INTO roles (id, team_id, display_id, name, status, version, created_at, updated_at, deactivated_at) VALUES (X'{role_id}', X'{team_id}', 'ROLE-{role_id}', 'Role', 'ACTIVE', 1, 1, 1, NULL);"))
            .execute(&pool)
            .await?;
    }
    let result = query("INSERT INTO tasks (id, team_id, owner_role_id, display_id, title, description, status, version, created_at, updated_at, cancelled_at, completed_at) VALUES (X'01921389d31174628f80cb5f79aa8121', X'01921389d31174628f80cb5f79aa8101', X'01921389d31174628f80cb5f79aa8112', 'TASK-1', 'Task', '', 'TODO', 1, 1, 1, NULL, NULL);")
        .execute(&pool)
        .await;
    assert!(
        result.is_err(),
        "cross-Team Task owner unexpectedly succeeded"
    );
    Ok(())
}

#[tokio::test]
async fn task_dependencies_reject_cross_team_cycles_and_update_cycles_without_side_effects()
-> Result<(), Box<dyn Error>> {
    let pool = database().await?;
    let a = graph(
        &pool,
        "01921389d31174628f80cb5f79aa8201",
        "01921389d31174628f80cb5f79aa8202",
        0x21,
    )
    .await?;
    let b = graph(
        &pool,
        "01921389d31174628f80cb5f79aa8301",
        "01921389d31174628f80cb5f79aa8302",
        0x31,
    )
    .await?;
    let team = "01921389d31174628f80cb5f79aa8201";
    assert!(
        query(&dependency(
            "01921389d31174628f80cb5f79aa8301",
            &a[0],
            &b[0]
        ))
        .execute(&pool)
        .await
        .is_err()
    );
    query(&dependency(team, &a[0], &a[1]))
        .execute(&pool)
        .await?;
    query(&dependency(team, &a[1], &a[2]))
        .execute(&pool)
        .await?;
    assert!(
        query(&dependency(team, &a[2], &a[0]))
            .execute(&pool)
            .await
            .is_err(),
        "three-hop cycle succeeded"
    );
    query(&dependency(team, &a[2], &a[3]))
        .execute(&pool)
        .await?;
    assert!(
        query(&dependency(team, &a[3], &a[0]))
            .execute(&pool)
            .await
            .is_err(),
        "four-hop cycle succeeded"
    );
    let count: i64 = query_scalar("SELECT COUNT(*) FROM task_dependencies")
        .fetch_one(&pool)
        .await?;
    assert_eq!(count, 3);
    let update = format!(
        "UPDATE task_dependencies SET depends_on_task_id = X'{}' WHERE task_id = X'{}' AND depends_on_task_id = X'{}'",
        a[0], a[1], a[2]
    );
    assert!(
        query(&update).execute(&pool).await.is_err(),
        "cycle-forming update succeeded"
    );
    let old: String = query_scalar(&format!(
        "SELECT hex(depends_on_task_id) FROM task_dependencies WHERE task_id = X'{}'",
        a[1]
    ))
    .fetch_one(&pool)
    .await?;
    assert_eq!(old, a[2].to_uppercase());
    Ok(())
}

#[tokio::test]
async fn valid_v10_graph_upgrades_to_0011_without_changing_existing_edges()
-> Result<(), Box<dyn Error>> {
    let pool = v10_database().await?;
    let team = "01921389d31174628f80cb5f79aa8401";
    let role = "01921389d31174628f80cb5f79aa8402";
    let tasks = graph(&pool, team, role, 0x41).await?;
    query(&dependency(team, &tasks[0], &tasks[1]))
        .execute(&pool)
        .await?;
    query(&dependency(team, &tasks[1], &tasks[2]))
        .execute(&pool)
        .await?;
    migrate(&pool).await?;
    let migration: i64 =
        query_scalar("SELECT COUNT(*) FROM _sqlx_migrations WHERE version = 11 AND success = TRUE")
            .fetch_one(&pool)
            .await?;
    let edges: i64 = query_scalar("SELECT COUNT(*) FROM task_dependencies")
        .fetch_one(&pool)
        .await?;
    assert_eq!(migration, 1);
    assert_eq!(edges, 2);
    let triggers: i64 = query_scalar("SELECT COUNT(*) FROM sqlite_master WHERE type = 'trigger' AND name IN ('tasks_team_consistency_insert', 'tasks_team_consistency_update', 'roles_task_team_consistency_update', 'task_dependencies_consistency_insert', 'task_dependencies_consistency_update')").fetch_one(&pool).await?;
    assert_eq!(triggers, 5);
    let other_team = "01921389d31174628f80cb5f79aa8901";
    let other_role = "01921389d31174628f80cb5f79aa8902";
    let other = graph(&pool, other_team, other_role, 0x91).await?;
    assert!(
        query(&format!(
            "UPDATE task_dependencies SET depends_on_task_id = X'{}' WHERE task_id = X'{}'",
            other[0], tasks[0]
        ))
        .execute(&pool)
        .await
        .is_err()
    );
    assert!(
        query(&format!(
            "UPDATE tasks SET owner_role_id = X'{other_role}' WHERE id = X'{}'",
            tasks[0]
        ))
        .execute(&pool)
        .await
        .is_err()
    );
    assert!(
        query(&format!(
            "UPDATE roles SET team_id = X'{other_team}' WHERE id = X'{role}'"
        ))
        .execute(&pool)
        .await
        .is_err()
    );
    let dependency_before: String = query_scalar(&format!(
        "SELECT hex(depends_on_task_id) FROM task_dependencies WHERE task_id = X'{}'",
        tasks[0]
    ))
    .fetch_one(&pool)
    .await?;
    let owner_before: String = query_scalar(&format!(
        "SELECT hex(owner_role_id) FROM tasks WHERE id = X'{}'",
        tasks[0]
    ))
    .fetch_one(&pool)
    .await?;
    let role_team_before: String = query_scalar(&format!(
        "SELECT hex(team_id) FROM roles WHERE id = X'{role}'"
    ))
    .fetch_one(&pool)
    .await?;
    assert_eq!(dependency_before, tasks[1].to_uppercase());
    assert_eq!(owner_before, role.to_uppercase());
    assert_eq!(role_team_before, team.to_uppercase());
    assert_eq!(
        query_scalar::<_, i64>("SELECT COUNT(*) FROM task_dependencies")
            .fetch_one(&pool)
            .await?,
        2
    );
    Ok(())
}

#[tokio::test]
async fn team_consistency_updates_reject_every_mutated_reference_without_side_effects()
-> Result<(), Box<dyn Error>> {
    let pool = database().await?;
    let team_a = "01921389d31174628f80cb5f79aa8501";
    let role_a = "01921389d31174628f80cb5f79aa8502";
    let team_b = "01921389d31174628f80cb5f79aa8601";
    let role_b = "01921389d31174628f80cb5f79aa8602";
    let a = graph(&pool, team_a, role_a, 0x51).await?;
    let b = graph(&pool, team_b, role_b, 0x61).await?;
    query(&dependency(team_a, &a[0], &a[1]))
        .execute(&pool)
        .await?;
    for statement in [
        format!(
            "UPDATE task_dependencies SET team_id = X'{team_b}' WHERE task_id = X'{}'",
            a[0]
        ),
        format!(
            "UPDATE task_dependencies SET task_id = X'{}' WHERE task_id = X'{}'",
            b[0], a[0]
        ),
        format!(
            "UPDATE task_dependencies SET depends_on_task_id = X'{}' WHERE task_id = X'{}'",
            b[0], a[0]
        ),
        format!(
            "UPDATE tasks SET owner_role_id = X'{role_b}' WHERE id = X'{}'",
            a[0]
        ),
        format!(
            "UPDATE tasks SET team_id = X'{team_b}' WHERE id = X'{}'",
            a[0]
        ),
        format!("UPDATE roles SET team_id = X'{team_b}' WHERE id = X'{role_a}'"),
    ] {
        assert!(
            query(&statement).execute(&pool).await.is_err(),
            "illegal update succeeded: {statement}"
        );
    }
    let values: (String, String, String) = sqlx_core::query_as::query_as(&format!(
        "SELECT hex(team_id), hex(task_id), hex(depends_on_task_id) FROM task_dependencies WHERE task_id = X'{}'", a[0]
    )).fetch_one(&pool).await?;
    assert_eq!(
        values,
        (
            team_a.to_uppercase(),
            a[0].to_uppercase(),
            a[1].to_uppercase()
        )
    );
    let task_owner: String = query_scalar(&format!(
        "SELECT hex(owner_role_id) FROM tasks WHERE id = X'{}'",
        a[0]
    ))
    .fetch_one(&pool)
    .await?;
    let task_team: String = query_scalar(&format!(
        "SELECT hex(team_id) FROM tasks WHERE id = X'{}'",
        a[0]
    ))
    .fetch_one(&pool)
    .await?;
    let role_team: String = query_scalar(&format!(
        "SELECT hex(team_id) FROM roles WHERE id = X'{role_a}'"
    ))
    .fetch_one(&pool)
    .await?;
    assert_eq!(task_owner, role_a.to_uppercase());
    assert_eq!(task_team, team_a.to_uppercase());
    assert_eq!(role_team, team_a.to_uppercase());
    let rows: i64 = query_scalar("SELECT COUNT(*) FROM task_dependencies")
        .fetch_one(&pool)
        .await?;
    assert_eq!(rows, 1);
    Ok(())
}

#[tokio::test]
async fn dirty_v10_owner_edge_and_cycles_fail_upgrade_without_partial_schema()
-> Result<(), Box<dyn Error>> {
    for case in ["owner", "edge", "three", "four"] {
        let pool = v10_database().await?;
        let team_a = "01921389d31174628f80cb5f79aa8701";
        let role_a = "01921389d31174628f80cb5f79aa8702";
        let team_b = "01921389d31174628f80cb5f79aa8801";
        let role_b = "01921389d31174628f80cb5f79aa8802";
        let a = graph(&pool, team_a, role_a, 0x71).await?;
        let b = graph(&pool, team_b, role_b, 0x81).await?;
        match case {
            "owner" => {
                query(&format!(
                    "UPDATE tasks SET owner_role_id = X'{role_b}' WHERE id = X'{}'",
                    a[0]
                ))
                .execute(&pool)
                .await?;
            }
            "edge" => {
                query(&dependency(team_b, &a[0], &b[0]))
                    .execute(&pool)
                    .await?;
            }
            "three" => {
                query(&dependency(team_a, &a[0], &a[1]))
                    .execute(&pool)
                    .await?;
                query(&dependency(team_a, &a[1], &a[2]))
                    .execute(&pool)
                    .await?;
                query(&dependency(team_a, &a[2], &a[0]))
                    .execute(&pool)
                    .await?;
            }
            _ => {
                query(&dependency(team_a, &a[0], &a[1]))
                    .execute(&pool)
                    .await?;
                query(&dependency(team_a, &a[1], &a[2]))
                    .execute(&pool)
                    .await?;
                query(&dependency(team_a, &a[2], &a[3]))
                    .execute(&pool)
                    .await?;
                query(&dependency(team_a, &a[3], &a[0]))
                    .execute(&pool)
                    .await?;
            }
        }
        let task_count: i64 = query_scalar("SELECT COUNT(*) FROM tasks")
            .fetch_one(&pool)
            .await?;
        let edge_count: i64 = query_scalar("SELECT COUNT(*) FROM task_dependencies")
            .fetch_one(&pool)
            .await?;
        assert!(migrate(&pool).await.is_err(), "dirty {case} upgraded");
        assert_eq!(
            query_scalar::<_, i64>("SELECT COUNT(*) FROM tasks")
                .fetch_one(&pool)
                .await?,
            task_count
        );
        assert_eq!(
            query_scalar::<_, i64>("SELECT COUNT(*) FROM task_dependencies")
                .fetch_one(&pool)
                .await?,
            edge_count
        );
        assert_eq!(
            query_scalar::<_, i64>("SELECT COUNT(*) FROM _sqlx_migrations WHERE version = 11")
                .fetch_one(&pool)
                .await?,
            0
        );
        assert_eq!(query_scalar::<_, i64>("SELECT COUNT(*) FROM sqlite_master WHERE name = 'task_dependency_consistency_upgrade_guard'").fetch_one(&pool).await?, 0);
    }
    Ok(())
}
