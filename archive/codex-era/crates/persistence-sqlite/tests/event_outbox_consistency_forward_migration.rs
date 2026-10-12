use std::{borrow::Cow, error::Error, future::Future, pin::Pin, str::FromStr};

use persistence_sqlite::migrate;
use sqlx_core::{
    error::BoxDynError,
    migrate::{Migration, MigrationSource, MigrationType, Migrator},
    query::query,
    query_scalar::query_scalar,
};
use sqlx_sqlite::{SqliteConnectOptions, SqlitePool, SqlitePoolOptions};

const V13: [(&str, &str); 13] = [
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
];

#[derive(Debug)]
struct V13Migrations;

impl MigrationSource<'static> for V13Migrations {
    fn resolve(
        self,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<Migration>, BoxDynError>> + Send + 'static>> {
        Box::pin(async {
            Ok(V13
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

async fn v13_database() -> Result<SqlitePool, Box<dyn Error>> {
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(SqliteConnectOptions::from_str("sqlite::memory:")?.foreign_keys(true))
        .await?;
    Migrator::new(V13Migrations).await?.run(&pool).await?;
    Ok(pool)
}

struct Graph {
    team: &'static str,
    role: &'static str,
    harness: &'static str,
    adapter: &'static str,
    binding: &'static str,
    task: &'static str,
    run: &'static str,
    artifact: &'static str,
    thread: &'static str,
    message: &'static str,
}

fn graph(seed: char) -> Graph {
    match seed {
        'a' => Graph {
            team: "01921389d31174628f80cb5f79aa1701",
            role: "01921389d31174628f80cb5f79aa1702",
            harness: "01921389d31174628f80cb5f79aa1703",
            adapter: "01921389d31174628f80cb5f79aa1704",
            binding: "01921389d31174628f80cb5f79aa1705",
            task: "01921389d31174628f80cb5f79aa1706",
            run: "01921389d31174628f80cb5f79aa1707",
            artifact: "01921389d31174628f80cb5f79aa1708",
            thread: "01921389d31174628f80cb5f79aa1709",
            message: "01921389d31174628f80cb5f79aa1710",
        },
        'b' => Graph {
            team: "01921389d31174628f80cb5f79aa1801",
            role: "01921389d31174628f80cb5f79aa1802",
            harness: "01921389d31174628f80cb5f79aa1803",
            adapter: "01921389d31174628f80cb5f79aa1804",
            binding: "01921389d31174628f80cb5f79aa1805",
            task: "01921389d31174628f80cb5f79aa1806",
            run: "01921389d31174628f80cb5f79aa1807",
            artifact: "01921389d31174628f80cb5f79aa1808",
            thread: "01921389d31174628f80cb5f79aa1809",
            message: "01921389d31174628f80cb5f79aa1810",
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
        format!(
            "INSERT INTO runs VALUES(X'{}',X'{}',X'{}',X'{}',X'{}',X'{}',1,1,'SUCCEEDED',0,NULL,1,1,1,2)",
            graph.run, graph.team, graph.task, graph.role, graph.binding, graph.harness
        ),
        format!(
            "INSERT INTO artifacts VALUES(X'{}',X'{}',X'{}',X'{}',X'{}','DOCUMENT','artifact://{label}',NULL,NULL,'TEAM_INTERNAL',1,1)",
            graph.artifact, graph.team, graph.task, graph.run, graph.role
        ),
        format!(
            "INSERT INTO threads VALUES(X'{}',X'{}','thread','OPEN',1,1,1)",
            graph.thread, graph.team
        ),
        format!(
            "INSERT INTO messages VALUES(X'{}',X'{}',X'{}','ROLE',X'{}',NULL,X'{}',X'{}',1,NULL,'DIRECT','message','DRAFT',1,NULL,1,1,1,NULL,NULL,NULL)",
            graph.message, graph.team, graph.thread, graph.role, graph.role, graph.binding
        ),
    ] {
        query(&statement).execute(pool).await?;
    }
    Ok(())
}

fn event(id: &str, graph: &Graph, actor: &str, subject_type: &str, subject: &str) -> String {
    format!(
        "INSERT INTO events (id, team_id, event_type, actor_type, actor_id, actor_kind, actor_role_id, actor_binding_id, actor_binding_version, actor_harness_instance_id, subject_type, subject_id, request_id, object_version, payload, metadata, occurred_at, previous_hash, hash) VALUES (X'{id}',X'{}','audit.recorded','ROLE',X'{actor}','ROLE_BINDING',X'{actor}',X'{}',1,X'{}','{subject_type}',X'{subject}',NULL,1,'{{}}','{{}}',1,NULL,'E3B0C44298FC1C149AFBF4C8996FB92427AE41E4649B934CA495991B7852B855')",
        graph.team, graph.binding, graph.harness
    )
}

fn legacy_event(id: &str, team: &str, actor: &str, subject_type: &str, subject: &str) -> String {
    format!(
        "INSERT INTO events (id, team_id, event_type, actor_type, actor_id, subject_type, subject_id, request_id, object_version, payload, metadata, occurred_at, previous_hash, hash) VALUES (X'{id}',X'{team}','audit.recorded','ROLE',X'{actor}','{subject_type}',X'{subject}',NULL,1,'{{}}','{{}}',1,NULL,'E3B0C44298FC1C149AFBF4C8996FB92427AE41E4649B934CA495991B7852B855')"
    )
}

fn outbox(id: &str, team: &str, event: &str) -> String {
    format!(
        "INSERT INTO outbox VALUES(X'{id}',X'{team}',X'{event}','READ_MODEL_PROJECTION','{{}}','PENDING',0,5,NULL,NULL,1,1,NULL)"
    )
}

fn consumption(consumer: &str, team: &str, event: &str) -> String {
    format!("INSERT INTO event_consumptions VALUES('{consumer}',X'{event}',X'{team}',1)")
}

#[tokio::test]
async fn replace_cannot_bypass_event_append_only_when_recursive_triggers_are_disabled()
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
    let event_id = "01921389d31174628f80cb5f79aa1721";
    query(&event(event_id, &a, a.role, "TASK", a.task))
        .execute(&pool)
        .await?;

    let replacement = event(event_id, &a, a.role, "ROLE", a.role)
        .replace("INSERT INTO events", "INSERT OR REPLACE INTO events");
    assert!(
        query(&replacement).execute(&pool).await.is_err(),
        "REPLACE rewrote an Event"
    );
    let stored: (String, String, String) = query_as_event(&pool, event_id).await?;
    assert_eq!(
        stored,
        (
            "TASK".to_owned(),
            a.task.to_uppercase(),
            "audit.recorded".to_owned()
        )
    );
    Ok(())
}

async fn query_as_event(
    pool: &SqlitePool,
    id: &str,
) -> Result<(String, String, String), Box<dyn Error>> {
    Ok(sqlx_core::query_as::query_as(&format!(
        "SELECT subject_type, hex(subject_id), event_type FROM events WHERE id=X'{id}'"
    ))
    .fetch_one(pool)
    .await?)
}

#[tokio::test]
async fn outbox_and_consumptions_must_match_their_event_team_on_insert_and_update()
-> Result<(), Box<dyn Error>> {
    let pool = database().await?;
    let a = graph('a');
    let b = graph('b');
    insert_graph(&pool, &a, 'a').await?;
    insert_graph(&pool, &b, 'b').await?;
    let event_id = "01921389d31174628f80cb5f79aa1722";
    query(&event(event_id, &a, a.role, "TASK", a.task))
        .execute(&pool)
        .await?;
    query(&outbox(
        "01921389d31174628f80cb5f79aa1723",
        a.team,
        event_id,
    ))
    .execute(&pool)
    .await?;
    query(&consumption("projector", a.team, event_id))
        .execute(&pool)
        .await?;

    for statement in [
        outbox("01921389d31174628f80cb5f79aa1724", b.team, event_id),
        consumption("foreign-projector", b.team, event_id),
        format!(
            "UPDATE outbox SET team_id=X'{}' WHERE id=X'01921389d31174628f80cb5f79aa1723'",
            b.team
        ),
        format!(
            "UPDATE event_consumptions SET team_id=X'{}' WHERE consumer_id='projector' AND event_id=X'{event_id}'",
            b.team
        ),
        format!(
            "UPDATE events SET team_id=X'{}' WHERE id=X'{event_id}'",
            b.team
        ),
    ] {
        assert!(
            query(&statement).execute(&pool).await.is_err(),
            "cross-Team mutation succeeded: {statement}"
        );
    }
    assert_eq!(
        query_scalar::<_, String>("SELECT hex(team_id) FROM outbox")
            .fetch_one(&pool)
            .await?,
        a.team.to_uppercase()
    );
    assert_eq!(
        query_scalar::<_, String>("SELECT hex(team_id) FROM event_consumptions")
            .fetch_one(&pool)
            .await?,
        a.team.to_uppercase()
    );
    Ok(())
}

#[tokio::test]
async fn event_subjects_and_role_actor_must_exist_in_the_same_team() -> Result<(), Box<dyn Error>> {
    let pool = database().await?;
    let a = graph('a');
    let b = graph('b');
    insert_graph(&pool, &a, 'a').await?;
    insert_graph(&pool, &b, 'b').await?;
    let subjects = [
        ("TASK", a.task, b.task),
        ("ROLE", a.role, b.role),
        ("RUN", a.run, b.run),
        ("ARTIFACT", a.artifact, b.artifact),
        ("MESSAGE", a.message, b.message),
    ];
    for (offset, (subject_type, local, foreign)) in subjects.into_iter().enumerate() {
        let valid = format!("01921389d31174628f80cb5f79aa173{offset:x}");
        query(&event(&valid, &a, a.role, subject_type, local))
            .execute(&pool)
            .await?;
        let missing = format!("01921389d31174628f80cb5f79aa174{offset:x}");
        assert!(
            query(&event(
                &missing,
                &a,
                a.role,
                subject_type,
                "01921389d31174628f80cb5f79aa1799"
            ))
            .execute(&pool)
            .await
            .is_err(),
            "missing {subject_type} subject succeeded"
        );
        let cross_team = format!("01921389d31174628f80cb5f79aa175{offset:x}");
        assert!(
            query(&event(&cross_team, &a, a.role, subject_type, foreign))
                .execute(&pool)
                .await
                .is_err(),
            "cross-Team {subject_type} subject succeeded"
        );
    }
    assert!(
        query(&event(
            "01921389d31174628f80cb5f79aa1761",
            &a,
            b.role,
            "TASK",
            a.task
        ))
        .execute(&pool)
        .await
        .is_err(),
        "ROLE actor from another Team succeeded"
    );
    Ok(())
}

#[tokio::test]
async fn valid_v13_event_history_upgrades_and_dirty_history_is_left_intact()
-> Result<(), Box<dyn Error>> {
    let clean = v13_database().await?;
    let a = graph('a');
    insert_graph(&clean, &a, 'a').await?;
    query(&legacy_event(
        "01921389d31174628f80cb5f79aa1762",
        a.team,
        a.role,
        "TASK",
        a.task,
    ))
    .execute(&clean)
    .await?;
    migrate(&clean).await?;
    assert_eq!(
        query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM _sqlx_migrations WHERE version=14 AND success=TRUE"
        )
        .fetch_one(&clean)
        .await?,
        1
    );

    let dirty = v13_database().await?;
    let b = graph('b');
    insert_graph(&dirty, &a, 'a').await?;
    insert_graph(&dirty, &b, 'b').await?;
    query(&legacy_event(
        "01921389d31174628f80cb5f79aa1763",
        a.team,
        a.role,
        "TASK",
        b.task,
    ))
    .execute(&dirty)
    .await?;
    assert!(
        migrate(&dirty).await.is_err(),
        "polluted v13 history upgraded"
    );
    assert_eq!(
        query_scalar::<_, i64>("SELECT COUNT(*) FROM _sqlx_migrations WHERE version=14")
            .fetch_one(&dirty)
            .await?,
        0
    );
    assert_eq!(
        query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM events WHERE id=X'01921389d31174628f80cb5f79aa1763'"
        )
        .fetch_one(&dirty)
        .await?,
        1
    );
    Ok(())
}

#[tokio::test]
async fn failed_event_or_outbox_write_rolls_back_the_business_change() -> Result<(), Box<dyn Error>>
{
    let pool = database().await?;
    let a = graph('a');
    let b = graph('b');
    insert_graph(&pool, &a, 'a').await?;
    insert_graph(&pool, &b, 'b').await?;
    let event_id = "01921389d31174628f80cb5f79aa1764";
    let mut transaction = pool.begin().await?;
    query(&format!(
        "UPDATE tasks SET status='DONE', completed_at=2, version=2 WHERE id=X'{}'",
        a.task
    ))
    .execute(&mut *transaction)
    .await?;
    query(&event(event_id, &a, a.role, "TASK", a.task))
        .execute(&mut *transaction)
        .await?;
    assert!(
        query(&outbox(
            "01921389d31174628f80cb5f79aa1765",
            b.team,
            event_id
        ))
        .execute(&mut *transaction)
        .await
        .is_err(),
        "invalid Outbox write succeeded"
    );
    transaction.rollback().await?;
    assert_eq!(
        query_scalar::<_, String>(&format!("SELECT status FROM tasks WHERE id=X'{}'", a.task))
            .fetch_one(&pool)
            .await?,
        "DOING"
    );
    assert_eq!(
        query_scalar::<_, i64>("SELECT COUNT(*) FROM events")
            .fetch_one(&pool)
            .await?,
        0
    );
    assert_eq!(
        query_scalar::<_, i64>("SELECT COUNT(*) FROM outbox")
            .fetch_one(&pool)
            .await?,
        0
    );
    Ok(())
}
