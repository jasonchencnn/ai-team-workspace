use std::{borrow::Cow, error::Error, future::Future, pin::Pin, str::FromStr};

use persistence_sqlite::migrate;
use sqlx_core::{
    error::BoxDynError,
    migrate::{Migration, MigrationSource, MigrationType, Migrator},
    query::query,
    query_scalar::query_scalar,
};
use sqlx_sqlite::{SqliteConnectOptions, SqlitePool, SqlitePoolOptions};

const V12: [(&str, &str); 12] = [
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
    (
        "run lease consistency",
        include_str!("../migrations/0012_run_lease_consistency.sql"),
    ),
];

#[derive(Debug)]
struct V12Migrations;
impl MigrationSource<'static> for V12Migrations {
    fn resolve(
        self,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<Migration>, BoxDynError>> + Send + 'static>> {
        Box::pin(async {
            Ok(V12
                .iter()
                .enumerate()
                .map(|(i, (name, sql))| {
                    Migration::new(
                        (i + 1) as i64,
                        Cow::Borrowed(*name),
                        MigrationType::Simple,
                        Cow::Borrowed(*sql),
                        false,
                    )
                })
                .collect())
        })
    }
}
async fn v12_database() -> Result<SqlitePool, Box<dyn Error>> {
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(SqliteConnectOptions::from_str("sqlite::memory:")?.foreign_keys(true))
        .await?;
    Migrator::new(V12Migrations).await?.run(&pool).await?;
    Ok(pool)
}

async fn history(pool: &SqlitePool) -> Result<(), Box<dyn Error>> {
    for sql in [
        "INSERT INTO teams VALUES(X'01921389d31174628f80cb5f79aa1501','team','ACTIVE',1,1,1,NULL)",
        "INSERT INTO roles VALUES(X'01921389d31174628f80cb5f79aa1502',X'01921389d31174628f80cb5f79aa1501','owner','owner','ACTIVE',1,1,1,NULL)",
        "INSERT INTO harness_instances VALUES(X'01921389d31174628f80cb5f79aa1503',X'01921389d31174628f80cb5f79aa1501','mock','h','ACTIVE','[]',NULL,1,1,1,NULL)",
        "INSERT INTO adapters VALUES(X'01921389d31174628f80cb5f79aa1504',X'01921389d31174628f80cb5f79aa1501','adapter','1','/adapter','{}',1,1,1)",
        "INSERT INTO bindings VALUES(X'01921389d31174628f80cb5f79aa1505',X'01921389d31174628f80cb5f79aa1501',X'01921389d31174628f80cb5f79aa1502',X'01921389d31174628f80cb5f79aa1503',X'01921389d31174628f80cb5f79aa1504','ACTIVE',1,1,'{}',1,1,1,NULL)",
        "INSERT INTO tasks VALUES(X'01921389d31174628f80cb5f79aa1506',X'01921389d31174628f80cb5f79aa1501',X'01921389d31174628f80cb5f79aa1502','task','task','','DOING',1,1,1,NULL,NULL)",
        "INSERT INTO tasks VALUES(X'01921389d31174628f80cb5f79aa1512',X'01921389d31174628f80cb5f79aa1501',X'01921389d31174628f80cb5f79aa1502','other-task','other task','','DOING',1,1,1,NULL,NULL)",
        "INSERT INTO runs VALUES(X'01921389d31174628f80cb5f79aa1507',X'01921389d31174628f80cb5f79aa1501',X'01921389d31174628f80cb5f79aa1506',X'01921389d31174628f80cb5f79aa1502',X'01921389d31174628f80cb5f79aa1505',X'01921389d31174628f80cb5f79aa1503',1,1,'SUCCEEDED',0,NULL,1,1,1,2)",
        "INSERT INTO runs VALUES(X'01921389d31174628f80cb5f79aa1508',X'01921389d31174628f80cb5f79aa1501',X'01921389d31174628f80cb5f79aa1506',X'01921389d31174628f80cb5f79aa1502',X'01921389d31174628f80cb5f79aa1505',X'01921389d31174628f80cb5f79aa1503',2,2,'SUCCEEDED',0,NULL,1,1,1,2)",
        "INSERT INTO artifacts VALUES(X'01921389d31174628f80cb5f79aa1509',X'01921389d31174628f80cb5f79aa1501',X'01921389d31174628f80cb5f79aa1506',X'01921389d31174628f80cb5f79aa1507',X'01921389d31174628f80cb5f79aa1502','DOCUMENT','artifact://one',NULL,NULL,'TEAM_INTERNAL',1,1)",
        "INSERT INTO artifacts VALUES(X'01921389d31174628f80cb5f79aa1510',X'01921389d31174628f80cb5f79aa1501',X'01921389d31174628f80cb5f79aa1506',X'01921389d31174628f80cb5f79aa1508',X'01921389d31174628f80cb5f79aa1502','TEST_REPORT','artifact://two',NULL,NULL,'TEAM_INTERNAL',1,1)",
        "INSERT INTO threads VALUES(X'01921389d31174628f80cb5f79aa1513',X'01921389d31174628f80cb5f79aa1501','thread','OPEN',1,1,1)",
        "INSERT INTO messages VALUES(X'01921389d31174628f80cb5f79aa1514',X'01921389d31174628f80cb5f79aa1501',X'01921389d31174628f80cb5f79aa1513','ROLE',X'01921389d31174628f80cb5f79aa1502',NULL,X'01921389d31174628f80cb5f79aa1502',X'01921389d31174628f80cb5f79aa1505',1,NULL,'DIRECT','message','DRAFT',1,NULL,1,1,1,NULL,NULL,NULL)",
        "INSERT INTO \"references\" VALUES(X'01921389d31174628f80cb5f79aa1511',X'01921389d31174628f80cb5f79aa1501','TASK',X'01921389d31174628f80cb5f79aa1506','WORKSPACE_PATH','docs/a.md','VALID',NULL,1,1,NULL)",
    ] {
        query(sql).execute(pool).await?;
    }
    Ok(())
}

#[tokio::test]
async fn valid_v12_artifact_history_with_multiple_runs_upgrades_to_0013()
-> Result<(), Box<dyn Error>> {
    let pool = v12_database().await?;
    history(&pool).await?;
    migrate(&pool).await?;
    assert_eq!(
        query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM _sqlx_migrations WHERE version = 13 AND success = TRUE"
        )
        .fetch_one(&pool)
        .await?,
        1
    );
    assert_eq!(
        query_scalar::<_, i64>("SELECT COUNT(*) FROM artifacts")
            .fetch_one(&pool)
            .await?,
        2
    );
    assert_eq!(
        query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM sqlite_master WHERE name = 'artifact_reference_consistency_guard'"
        )
        .fetch_one(&pool)
        .await?,
        0
    );
    Ok(())
}

#[tokio::test]
async fn dirty_v12_artifact_or_reference_history_rejects_upgrade_atomically()
-> Result<(), Box<dyn Error>> {
    for (name, dirty) in [
        (
            "artifact task/run",
            "UPDATE artifacts SET task_id=X'01921389d31174628f80cb5f79aa1512' WHERE id=X'01921389d31174628f80cb5f79aa1509'",
        ),
        (
            "missing RUN reference subject",
            "UPDATE \"references\" SET subject_type='RUN', subject_id=X'01921389d31174628f80cb5f79aa1599'",
        ),
        (
            "missing ARTIFACT reference subject",
            "UPDATE \"references\" SET subject_type='ARTIFACT', subject_id=X'01921389d31174628f80cb5f79aa1599'",
        ),
        (
            "missing MESSAGE reference subject",
            "UPDATE \"references\" SET subject_type='MESSAGE', subject_id=X'01921389d31174628f80cb5f79aa1599'",
        ),
    ] {
        let pool = v12_database().await?;
        history(&pool).await?;
        query(dirty).execute(&pool).await?;
        let artifacts: i64 = query_scalar("SELECT COUNT(*) FROM artifacts")
            .fetch_one(&pool)
            .await?;
        let references: i64 = query_scalar("SELECT COUNT(*) FROM \"references\"")
            .fetch_one(&pool)
            .await?;
        assert!(
            migrate(&pool).await.is_err(),
            "dirty {name} history upgraded"
        );
        assert_eq!(
            query_scalar::<_, i64>("SELECT COUNT(*) FROM artifacts")
                .fetch_one(&pool)
                .await?,
            artifacts
        );
        assert_eq!(
            query_scalar::<_, i64>("SELECT COUNT(*) FROM \"references\"")
                .fetch_one(&pool)
                .await?,
            references
        );
        assert_eq!(
            query_scalar::<_, i64>("SELECT COUNT(*) FROM _sqlx_migrations WHERE version = 13")
                .fetch_one(&pool)
                .await?,
            0
        );
        assert_eq!(query_scalar::<_, i64>("SELECT COUNT(*) FROM sqlite_master WHERE name = 'artifact_reference_consistency_guard'").fetch_one(&pool).await?, 0);
    }
    Ok(())
}
