use std::{borrow::Cow, error::Error, future::Future, pin::Pin, str::FromStr};

use persistence_sqlite::migrate;
use sqlx_core::{
    error::BoxDynError,
    migrate::{Migration, MigrationSource, MigrationType, Migrator},
    query::query,
    query_scalar::query_scalar,
};
use sqlx_sqlite::{SqliteConnectOptions, SqlitePool, SqlitePoolOptions};

const V14: [(&str, &str); 14] = [
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
    (
        "artifact reference consistency",
        include_str!("../migrations/0013_artifact_reference_consistency.sql"),
    ),
    (
        "event outbox consistency",
        include_str!("../migrations/0014_event_outbox_consistency.sql"),
    ),
];

#[derive(Debug)]
struct V14Migrations;

impl MigrationSource<'static> for V14Migrations {
    fn resolve(
        self,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<Migration>, BoxDynError>> + Send + 'static>> {
        Box::pin(async {
            Ok(V14
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
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(SqliteConnectOptions::from_str("sqlite::memory:")?.foreign_keys(true))
        .await?;
    migrate(&pool).await?;
    Ok(pool)
}

async fn v14_database() -> Result<SqlitePool, Box<dyn Error>> {
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(SqliteConnectOptions::from_str("sqlite::memory:")?.foreign_keys(true))
        .await?;
    Migrator::new(V14Migrations).await?.run(&pool).await?;
    Ok(pool)
}

struct Graph {
    team: &'static str,
    role: &'static str,
    harness: &'static str,
    adapter: &'static str,
    binding: &'static str,
    task: &'static str,
}

fn graph(seed: char) -> Graph {
    match seed {
        'a' => Graph {
            team: "01921389d31174628f80cb5f79aa1901",
            role: "01921389d31174628f80cb5f79aa1902",
            harness: "01921389d31174628f80cb5f79aa1903",
            adapter: "01921389d31174628f80cb5f79aa1904",
            binding: "01921389d31174628f80cb5f79aa1905",
            task: "01921389d31174628f80cb5f79aa1906",
        },
        'b' => Graph {
            team: "01921389d31174628f80cb5f79aa1a01",
            role: "01921389d31174628f80cb5f79aa1a02",
            harness: "01921389d31174628f80cb5f79aa1a03",
            adapter: "01921389d31174628f80cb5f79aa1a04",
            binding: "01921389d31174628f80cb5f79aa1a05",
            task: "01921389d31174628f80cb5f79aa1a06",
        },
        _ => unreachable!(),
    }
}

async fn insert_graph(pool: &SqlitePool, graph: &Graph, label: char) -> Result<(), Box<dyn Error>> {
    for statement in [
        format!(
            "INSERT INTO teams VALUES(X'{}','team-{label}','ACTIVE',1,1,1,NULL)",
            graph.team
        ),
        format!(
            "INSERT INTO roles VALUES(X'{}',X'{}','role-{label}','role','ACTIVE',1,1,1,NULL)",
            graph.role, graph.team
        ),
        format!(
            "INSERT INTO harness_instances VALUES(X'{}',X'{}','mock','harness','ACTIVE','[]',NULL,1,1,1,NULL)",
            graph.harness, graph.team
        ),
        format!(
            "INSERT INTO adapters VALUES(X'{}',X'{}','adapter-{label}','1','/adapter','{{}}',1,1,1)",
            graph.adapter, graph.team
        ),
        format!(
            "INSERT INTO bindings VALUES(X'{}',X'{}',X'{}',X'{}',X'{}','ACTIVE',1,1,'{{}}',1,1,1,NULL)",
            graph.binding, graph.team, graph.role, graph.harness, graph.adapter
        ),
        format!(
            "INSERT INTO tasks VALUES(X'{}',X'{}',X'{}','task-{label}','task','','DOING',1,1,1,NULL,NULL)",
            graph.task, graph.team, graph.role
        ),
    ] {
        query(&statement).execute(pool).await?;
    }
    Ok(())
}

const HASH: &str = "E3B0C44298FC1C149AFBF4C8996FB92427AE41E4649B934CA495991B7852B855";

fn legacy_system_event(id: &str, team: &str, subject: &str) -> String {
    format!(
        "INSERT INTO events (id, team_id, event_type, actor_type, actor_id, subject_type, subject_id, request_id, object_version, payload, metadata, occurred_at, previous_hash, hash) VALUES (X'{id}',X'{team}','audit.recorded','SYSTEM',NULL,'TASK',X'{subject}',NULL,1,'{{}}','{{}}',1,NULL,'{HASH}')"
    )
}

/// Frozen Role-Binding execution actor snapshot required by `PERMISSION_MATRIX.md`.
#[derive(Clone, Copy)]
struct RoleActor<'a> {
    actor_type: &'a str,
    actor_id: &'a str,
    actor_kind: &'a str,
    role_id: &'a str,
    binding_id: &'a str,
    binding_version: i64,
    harness_instance_id: &'a str,
}

#[derive(Clone, Copy)]
struct RoleEventSpec<'a> {
    id: &'a str,
    team: &'a str,
    subject: &'a str,
    actor: RoleActor<'a>,
}

fn role_event(spec: &RoleEventSpec<'_>) -> String {
    let actor = spec.actor;
    format!(
        "INSERT INTO events (id, team_id, event_type, actor_type, actor_id, actor_kind, actor_role_id, actor_binding_id, actor_binding_version, actor_harness_instance_id, subject_type, subject_id, request_id, object_version, payload, metadata, occurred_at, previous_hash, hash) VALUES(X'{id}',X'{team}','audit.recorded','{actor_type}',X'{actor_id}','{actor_kind}',X'{role_id}',X'{binding_id}',{binding_version},X'{harness_instance_id}','TASK',X'{subject}',NULL,1,'{{}}','{{}}',1,NULL,'{HASH}')",
        id = spec.id,
        team = spec.team,
        subject = spec.subject,
        actor_type = actor.actor_type,
        actor_id = actor.actor_id,
        actor_kind = actor.actor_kind,
        role_id = actor.role_id,
        binding_id = actor.binding_id,
        binding_version = actor.binding_version,
        harness_instance_id = actor.harness_instance_id,
    )
}

fn outbox(id: &str, team: &str, event: &str, status: &str) -> String {
    format!(
        "INSERT INTO outbox VALUES(X'{id}',X'{team}',X'{event}','READ_MODEL_PROJECTION','{{}}','{status}',0,5,NULL,NULL,1,1,NULL)"
    )
}

#[tokio::test]
async fn replace_cannot_reset_outbox_or_consumption_deduplication_state()
-> Result<(), Box<dyn Error>> {
    let pool = database().await?;
    assert_eq!(
        query_scalar::<_, i64>("PRAGMA recursive_triggers")
            .fetch_one(&pool)
            .await?,
        0
    );
    let a = graph('a');
    insert_graph(&pool, &a, 'a').await?;
    let event = "01921389d31174628f80cb5f79aa1911";
    let outbox_id = "01921389d31174628f80cb5f79aa1912";
    query(&legacy_system_event(event, a.team, a.task))
        .execute(&pool)
        .await?;
    query(&outbox(outbox_id, a.team, event, "PUBLISHED"))
        .execute(&pool)
        .await?;
    query(&format!(
        "INSERT INTO event_consumptions VALUES('projector',X'{event}',X'{}',7)",
        a.team
    ))
    .execute(&pool)
    .await?;

    let outbox_replace = outbox("01921389d31174628f80cb5f79aa1913", a.team, event, "PENDING")
        .replace("INSERT INTO outbox", "INSERT OR REPLACE INTO outbox");
    let consumption_replace = format!(
        "INSERT OR REPLACE INTO event_consumptions VALUES('projector',X'{event}',X'{}',9)",
        a.team
    );
    assert!(
        query(&outbox_replace).execute(&pool).await.is_err(),
        "OR REPLACE reset Outbox state"
    );
    assert!(
        query(&consumption_replace).execute(&pool).await.is_err(),
        "OR REPLACE reset consumption state"
    );
    assert_eq!(
        query_scalar::<_, String>("SELECT status FROM outbox")
            .fetch_one(&pool)
            .await?,
        "PUBLISHED"
    );
    assert_eq!(
        query_scalar::<_, i64>("SELECT consumed_at FROM event_consumptions")
            .fetch_one(&pool)
            .await?,
        7
    );
    Ok(())
}

#[tokio::test]
async fn normal_outbox_update_and_explicit_consumption_replay_remain_available()
-> Result<(), Box<dyn Error>> {
    let pool = database().await?;
    let a = graph('a');
    insert_graph(&pool, &a, 'a').await?;
    let event = "01921389d31174628f80cb5f79aa1914";
    query(&legacy_system_event(event, a.team, a.task))
        .execute(&pool)
        .await?;
    query(&outbox(
        "01921389d31174628f80cb5f79aa1915",
        a.team,
        event,
        "PENDING",
    ))
    .execute(&pool)
    .await?;
    query(&format!(
        "INSERT INTO event_consumptions VALUES('projector',X'{event}',X'{}',1)",
        a.team
    ))
    .execute(&pool)
    .await?;
    query("UPDATE outbox SET status='PROCESSING', retry_count=1")
        .execute(&pool)
        .await?;
    query(&format!(
        "DELETE FROM event_consumptions WHERE consumer_id='projector' AND event_id=X'{event}'"
    ))
    .execute(&pool)
    .await?;
    query(&format!(
        "INSERT INTO event_consumptions VALUES('projector',X'{event}',X'{}',2)",
        a.team
    ))
    .execute(&pool)
    .await?;
    assert_eq!(
        query_scalar::<_, String>("SELECT status FROM outbox")
            .fetch_one(&pool)
            .await?,
        "PROCESSING"
    );
    assert_eq!(
        query_scalar::<_, i64>("SELECT consumed_at FROM event_consumptions")
            .fetch_one(&pool)
            .await?,
        2
    );
    Ok(())
}

#[tokio::test]
async fn role_event_requires_a_consistent_role_binding_harness_snapshot()
-> Result<(), Box<dyn Error>> {
    let pool = database().await?;
    let a = graph('a');
    let b = graph('b');
    insert_graph(&pool, &a, 'a').await?;
    insert_graph(&pool, &b, 'b').await?;
    let valid = RoleEventSpec {
        id: "01921389d31174628f80cb5f79aa1916",
        team: a.team,
        subject: a.task,
        actor: RoleActor {
            actor_type: "ROLE",
            actor_id: a.role,
            actor_kind: "ROLE_BINDING",
            role_id: a.role,
            binding_id: a.binding,
            binding_version: 1,
            harness_instance_id: a.harness,
        },
    };
    query(&role_event(&valid)).execute(&pool).await?;
    for (name, sql) in [
        (
            "missing actor kind",
            role_event(&valid)
                .replacen("'ROLE_BINDING'", "NULL", 1)
                .replace("1916", "1917"),
        ),
        (
            "cross-Team role",
            role_event(&RoleEventSpec {
                id: "01921389d31174628f80cb5f79aa1918",
                actor: RoleActor {
                    role_id: b.role,
                    actor_id: b.role,
                    ..valid.actor
                },
                ..valid
            }),
        ),
        (
            "cross-Team binding",
            role_event(&RoleEventSpec {
                id: "01921389d31174628f80cb5f79aa1919",
                actor: RoleActor {
                    binding_id: b.binding,
                    ..valid.actor
                },
                ..valid
            }),
        ),
        (
            "binding version mismatch",
            role_event(&RoleEventSpec {
                id: "01921389d31174628f80cb5f79aa1921",
                actor: RoleActor {
                    binding_version: 2,
                    ..valid.actor
                },
                ..valid
            }),
        ),
        (
            "binding harness mismatch",
            role_event(&RoleEventSpec {
                id: "01921389d31174628f80cb5f79aa1922",
                actor: RoleActor {
                    harness_instance_id: b.harness,
                    ..valid.actor
                },
                ..valid
            }),
        ),
        (
            "actor identity mismatch",
            role_event(&RoleEventSpec {
                id: "01921389d31174628f80cb5f79aa1923",
                actor: RoleActor {
                    actor_id: b.role,
                    ..valid.actor
                },
                ..valid
            }),
        ),
        (
            "adapter actor",
            role_event(&RoleEventSpec {
                id: "01921389d31174628f80cb5f79aa1924",
                actor: RoleActor {
                    actor_type: "ADAPTER",
                    ..valid.actor
                },
                ..valid
            }),
        ),
    ] {
        assert!(
            query(&sql).execute(&pool).await.is_err(),
            "{name} unexpectedly succeeded"
        );
    }
    Ok(())
}

#[tokio::test]
async fn v14_events_upgrade_without_fabricating_actor_snapshot_history()
-> Result<(), Box<dyn Error>> {
    let pool = v14_database().await?;
    let a = graph('a');
    insert_graph(&pool, &a, 'a').await?;
    let event = "01921389d31174628f80cb5f79aa1925";
    query(&legacy_system_event(event, a.team, a.task))
        .execute(&pool)
        .await?;
    migrate(&pool).await?;
    assert_eq!(
        query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM _sqlx_migrations WHERE version=15 AND success=TRUE"
        )
        .fetch_one(&pool)
        .await?,
        1
    );
    assert_eq!(
        query_scalar::<_, String>(&format!("SELECT hash FROM events WHERE id=X'{event}'"))
            .fetch_one(&pool)
            .await?,
        HASH
    );
    assert_eq!(query_scalar::<_, i64>(&format!("SELECT actor_kind IS NULL AND actor_role_id IS NULL AND actor_binding_id IS NULL AND actor_binding_version IS NULL AND actor_harness_instance_id IS NULL FROM events WHERE id=X'{event}'")).fetch_one(&pool).await?, 1);
    Ok(())
}
