use persistence_sqlite::migrate;
use sqlx_core::{query::query, query_scalar::query_scalar};
use sqlx_sqlite::{SqliteConnectOptions, SqlitePool, SqlitePoolOptions};
use std::error::Error;
use std::str::FromStr;

async fn database() -> Result<SqlitePool, Box<dyn Error>> {
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(SqliteConnectOptions::from_str("sqlite::memory:")?.foreign_keys(true))
        .await?;
    migrate(&pool).await?;
    Ok(pool)
}

struct Graph {
    team: &'static str,
    role: &'static str,
    other_role: &'static str,
    task: &'static str,
    other_task: &'static str,
    run: &'static str,
    other_run: &'static str,
    artifact: &'static str,
    message: &'static str,
}

async fn graph(pool: &SqlitePool, base: &'static str) -> Result<Graph, Box<dyn Error>> {
    let graph = if base == "a" {
        Graph {
            team: "01921389d31174628f80cb5f79aa1301",
            role: "01921389d31174628f80cb5f79aa1302",
            other_role: "01921389d31174628f80cb5f79aa1303",
            task: "01921389d31174628f80cb5f79aa1308",
            other_task: "01921389d31174628f80cb5f79aa1309",
            run: "01921389d31174628f80cb5f79aa1310",
            other_run: "01921389d31174628f80cb5f79aa1311",
            artifact: "01921389d31174628f80cb5f79aa1312",
            message: "01921389d31174628f80cb5f79aa1314",
        }
    } else {
        Graph {
            team: "01921389d31174628f80cb5f79aa1401",
            role: "01921389d31174628f80cb5f79aa1402",
            other_role: "01921389d31174628f80cb5f79aa1403",
            task: "01921389d31174628f80cb5f79aa1408",
            other_task: "01921389d31174628f80cb5f79aa1409",
            run: "01921389d31174628f80cb5f79aa1410",
            other_run: "01921389d31174628f80cb5f79aa1411",
            artifact: "01921389d31174628f80cb5f79aa1412",
            message: "01921389d31174628f80cb5f79aa1414",
        }
    };
    let harness = if base == "a" {
        "01921389d31174628f80cb5f79aa1304"
    } else {
        "01921389d31174628f80cb5f79aa1404"
    };
    let adapter = if base == "a" {
        "01921389d31174628f80cb5f79aa1305"
    } else {
        "01921389d31174628f80cb5f79aa1405"
    };
    let binding = if base == "a" {
        "01921389d31174628f80cb5f79aa1306"
    } else {
        "01921389d31174628f80cb5f79aa1406"
    };
    let thread = if base == "a" {
        "01921389d31174628f80cb5f79aa1313"
    } else {
        "01921389d31174628f80cb5f79aa1413"
    };
    for statement in [
        format!(
            "INSERT INTO teams VALUES(X'{}','team','ACTIVE',1,1,1,NULL)",
            graph.team
        ),
        format!(
            "INSERT INTO roles VALUES(X'{}',X'{}','owner-{base}','owner','ACTIVE',1,1,1,NULL)",
            graph.role, graph.team
        ),
        format!(
            "INSERT INTO roles VALUES(X'{}',X'{}','other-{base}','other','ACTIVE',1,1,1,NULL)",
            graph.other_role, graph.team
        ),
        format!(
            "INSERT INTO harness_instances VALUES(X'{harness}',X'{}','mock','h','ACTIVE','[]',NULL,1,1,1,NULL)",
            graph.team
        ),
        format!(
            "INSERT INTO adapters VALUES(X'{adapter}',X'{}','adapter-{base}','1','/adapter','{{}}',1,1,1)",
            graph.team
        ),
        format!(
            "INSERT INTO bindings VALUES(X'{binding}',X'{}',X'{}',X'{harness}',X'{adapter}','ACTIVE',1,1,'{{}}',1,1,1,NULL)",
            graph.team, graph.role
        ),
        format!(
            "INSERT INTO tasks VALUES(X'{}',X'{}',X'{}','task-{base}','task','','DOING',1,1,1,NULL,NULL)",
            graph.task, graph.team, graph.role
        ),
        format!(
            "INSERT INTO tasks VALUES(X'{}',X'{}',X'{}','other-task-{base}','other task','','DOING',1,1,1,NULL,NULL)",
            graph.other_task, graph.team, graph.role
        ),
        format!(
            "INSERT INTO runs VALUES(X'{}',X'{}',X'{}',X'{}',X'{binding}',X'{harness}',1,1,'SUCCEEDED',0,NULL,1,1,1,2)",
            graph.run, graph.team, graph.task, graph.role
        ),
        format!(
            "INSERT INTO runs VALUES(X'{}',X'{}',X'{}',X'{}',X'{binding}',X'{harness}',1,1,'SUCCEEDED',0,NULL,1,1,1,2)",
            graph.other_run, graph.team, graph.other_task, graph.role
        ),
        format!(
            "INSERT INTO threads VALUES(X'{thread}',X'{}','thread','OPEN',1,1,1)",
            graph.team
        ),
        format!(
            "INSERT INTO messages VALUES(X'{}',X'{}',X'{thread}','ROLE',X'{}',NULL,X'{}',X'{binding}',1,NULL,'DIRECT','message','DRAFT',1,NULL,1,1,1,NULL,NULL,NULL)",
            graph.message, graph.team, graph.role, graph.other_role
        ),
        format!(
            "INSERT INTO artifacts VALUES(X'{}',X'{}',X'{}',X'{}',X'{}','DOCUMENT','artifact://ok',NULL,NULL,'TEAM_INTERNAL',1,1)",
            graph.artifact, graph.team, graph.task, graph.run, graph.role
        ),
    ] {
        query(&statement).execute(pool).await?;
    }
    Ok(graph)
}

fn artifact(id: &str, team: &str, task: &str, run: &str, role: &str) -> String {
    format!(
        "INSERT INTO artifacts VALUES(X'{id}',X'{team}',X'{task}',X'{run}',X'{role}','DOCUMENT','artifact://test',NULL,NULL,'TEAM_INTERNAL',1,1)"
    )
}

fn reference(id: &str, team: &str, subject_type: &str, subject: &str) -> String {
    format!(
        "INSERT INTO \"references\" VALUES(X'{id}',X'{team}','{subject_type}',X'{subject}','WORKSPACE_PATH','docs/a.md','VALID',NULL,1,1,NULL)"
    )
}

async fn minimally_referenced_subject(
    pool: &SqlitePool,
    subject_type: &str,
) -> Result<(&'static str, &'static str), Box<dyn Error>> {
    let (subject, replacement, statements): (&str, &str, Vec<String>) = match subject_type {
        "TASK" => (
            "01921389d31174628f80cb5f79aa1606",
            "01921389d31174628f80cb5f79aa1616",
            vec![],
        ),
        "RUN" => (
            "01921389d31174628f80cb5f79aa1607",
            "01921389d31174628f80cb5f79aa1617",
            vec!["INSERT INTO runs VALUES(X'01921389d31174628f80cb5f79aa1607',X'01921389d31174628f80cb5f79aa1601',X'01921389d31174628f80cb5f79aa1606',X'01921389d31174628f80cb5f79aa1602',X'01921389d31174628f80cb5f79aa1605',X'01921389d31174628f80cb5f79aa1603',1,1,'SUCCEEDED',0,NULL,1,1,1,2)".to_owned()],
        ),
        "ARTIFACT" => (
            "01921389d31174628f80cb5f79aa1608",
            "01921389d31174628f80cb5f79aa1618",
            vec![
                "INSERT INTO runs VALUES(X'01921389d31174628f80cb5f79aa1607',X'01921389d31174628f80cb5f79aa1601',X'01921389d31174628f80cb5f79aa1606',X'01921389d31174628f80cb5f79aa1602',X'01921389d31174628f80cb5f79aa1605',X'01921389d31174628f80cb5f79aa1603',1,1,'SUCCEEDED',0,NULL,1,1,1,2)".to_owned(),
                "INSERT INTO artifacts VALUES(X'01921389d31174628f80cb5f79aa1608',X'01921389d31174628f80cb5f79aa1601',X'01921389d31174628f80cb5f79aa1606',X'01921389d31174628f80cb5f79aa1607',X'01921389d31174628f80cb5f79aa1602','DOCUMENT','artifact://minimal',NULL,NULL,'TEAM_INTERNAL',1,1)".to_owned(),
            ],
        ),
        "MESSAGE" => (
            "01921389d31174628f80cb5f79aa1610",
            "01921389d31174628f80cb5f79aa1620",
            vec![
                "INSERT INTO threads VALUES(X'01921389d31174628f80cb5f79aa1609',X'01921389d31174628f80cb5f79aa1601','thread','OPEN',1,1,1)".to_owned(),
                "INSERT INTO messages VALUES(X'01921389d31174628f80cb5f79aa1610',X'01921389d31174628f80cb5f79aa1601',X'01921389d31174628f80cb5f79aa1609','ROLE',X'01921389d31174628f80cb5f79aa1602',NULL,X'01921389d31174628f80cb5f79aa1602',X'01921389d31174628f80cb5f79aa1605',1,NULL,'DIRECT','message','DRAFT',1,NULL,1,1,1,NULL,NULL,NULL)".to_owned(),
            ],
        ),
        _ => unreachable!(),
    };
    for statement in [
        "INSERT INTO teams VALUES(X'01921389d31174628f80cb5f79aa1601','team','ACTIVE',1,1,1,NULL)".to_owned(),
        "INSERT INTO roles VALUES(X'01921389d31174628f80cb5f79aa1602',X'01921389d31174628f80cb5f79aa1601','role','role','ACTIVE',1,1,1,NULL)".to_owned(),
        "INSERT INTO harness_instances VALUES(X'01921389d31174628f80cb5f79aa1603',X'01921389d31174628f80cb5f79aa1601','mock','h','ACTIVE','[]',NULL,1,1,1,NULL)".to_owned(),
        "INSERT INTO adapters VALUES(X'01921389d31174628f80cb5f79aa1604',X'01921389d31174628f80cb5f79aa1601','adapter','1','/adapter','{}',1,1,1)".to_owned(),
        "INSERT INTO bindings VALUES(X'01921389d31174628f80cb5f79aa1605',X'01921389d31174628f80cb5f79aa1601',X'01921389d31174628f80cb5f79aa1602',X'01921389d31174628f80cb5f79aa1603',X'01921389d31174628f80cb5f79aa1604','ACTIVE',1,1,'{}',1,1,1,NULL)".to_owned(),
        "INSERT INTO tasks VALUES(X'01921389d31174628f80cb5f79aa1606',X'01921389d31174628f80cb5f79aa1601',X'01921389d31174628f80cb5f79aa1602','task','task','','DOING',1,1,1,NULL,NULL)".to_owned(),
    ].into_iter().chain(statements) {
        query(&statement).execute(pool).await?;
    }
    query(&reference(
        "01921389d31174628f80cb5f79aa1611",
        "01921389d31174628f80cb5f79aa1601",
        subject_type,
        subject,
    ))
    .execute(pool)
    .await?;
    Ok((subject, replacement))
}

macro_rules! referenced_parent_id_is_immutable {
    ($update:ident, $delete:ident, $subject_type:literal, $table:literal) => {
        #[tokio::test]
        async fn $update() -> Result<(), Box<dyn Error>> {
            let pool = database().await?;
            let (subject, replacement) = minimally_referenced_subject(&pool, $subject_type).await?;
            assert!(
                query(&format!(
                    "UPDATE {} SET id=X'{}' WHERE id=X'{}'",
                    $table, replacement, subject
                ))
                .execute(&pool)
                .await
                .is_err(),
                "{} parent id update unexpectedly succeeded",
                $subject_type
            );
            assert_eq!(
                query_scalar::<_, i64>(&format!(
                    "SELECT COUNT(*) FROM {} WHERE id=X'{}'",
                    $table, subject
                ))
                .fetch_one(&pool)
                .await?,
                1
            );
            assert_eq!(
                query_scalar::<_, i64>(&format!(
                    "SELECT COUNT(*) FROM \"references\" WHERE subject_id=X'{}'",
                    subject
                ))
                .fetch_one(&pool)
                .await?,
                1
            );
            assert_eq!(
                query_scalar::<_, i64>("SELECT COUNT(*) FROM \"references\"")
                    .fetch_one(&pool)
                    .await?,
                1
            );
            Ok(())
        }
        #[tokio::test]
        async fn $delete() -> Result<(), Box<dyn Error>> {
            let pool = database().await?;
            let (subject, _) = minimally_referenced_subject(&pool, $subject_type).await?;
            assert!(
                query(&format!("DELETE FROM {} WHERE id=X'{}'", $table, subject))
                    .execute(&pool)
                    .await
                    .is_err(),
                "{} parent delete unexpectedly succeeded",
                $subject_type
            );
            assert_eq!(
                query_scalar::<_, i64>(&format!(
                    "SELECT COUNT(*) FROM {} WHERE id=X'{}'",
                    $table, subject
                ))
                .fetch_one(&pool)
                .await?,
                1
            );
            assert_eq!(
                query_scalar::<_, i64>(&format!(
                    "SELECT COUNT(*) FROM \"references\" WHERE subject_id=X'{}'",
                    subject
                ))
                .fetch_one(&pool)
                .await?,
                1
            );
            Ok(())
        }
    };
}

referenced_parent_id_is_immutable!(
    task_reference_parent_id_update_rejected,
    task_reference_parent_delete_rejected,
    "TASK",
    "tasks"
);
referenced_parent_id_is_immutable!(
    run_reference_parent_id_update_rejected,
    run_reference_parent_delete_rejected,
    "RUN",
    "runs"
);
referenced_parent_id_is_immutable!(
    artifact_reference_parent_id_update_rejected,
    artifact_reference_parent_delete_rejected,
    "ARTIFACT",
    "artifacts"
);
referenced_parent_id_is_immutable!(
    message_reference_parent_id_update_rejected,
    message_reference_parent_delete_rejected,
    "MESSAGE",
    "messages"
);

#[tokio::test]
async fn artifact_requires_one_same_team_task_run_creator_tuple() -> Result<(), Box<dyn Error>> {
    let pool = database().await?;
    let a = graph(&pool, "a").await?;
    let b = graph(&pool, "b").await?;
    for (name, statement) in [
        (
            "run belongs to a different task",
            artifact(
                "01921389d31174628f80cb5f79aa1321",
                a.team,
                a.task,
                a.other_run,
                a.role,
            ),
        ),
        (
            "creator differs from Run history",
            artifact(
                "01921389d31174628f80cb5f79aa1322",
                a.team,
                a.task,
                a.run,
                a.other_role,
            ),
        ),
        (
            "Task is from another Team",
            artifact(
                "01921389d31174628f80cb5f79aa1323",
                a.team,
                b.task,
                a.run,
                a.role,
            ),
        ),
        (
            "Run is from another Team",
            artifact(
                "01921389d31174628f80cb5f79aa1324",
                a.team,
                a.task,
                b.run,
                a.role,
            ),
        ),
        (
            "creator Role is from another Team",
            artifact(
                "01921389d31174628f80cb5f79aa1325",
                a.team,
                a.task,
                a.run,
                b.role,
            ),
        ),
    ] {
        assert!(
            query(&statement).execute(&pool).await.is_err(),
            "{name} unexpectedly succeeded"
        );
    }
    assert_eq!(
        query_scalar::<_, i64>("SELECT COUNT(*) FROM artifacts")
            .fetch_one(&pool)
            .await?,
        2
    );
    Ok(())
}

#[tokio::test]
async fn every_reference_subject_type_requires_an_existing_same_team_subject()
-> Result<(), Box<dyn Error>> {
    let pool = database().await?;
    let a = graph(&pool, "a").await?;
    let b = graph(&pool, "b").await?;
    for (offset, subject_type, local, foreign) in [
        (31, "TASK", a.task, b.task),
        (32, "RUN", a.run, b.run),
        (33, "ARTIFACT", a.artifact, b.artifact),
        (34, "MESSAGE", a.message, b.message),
    ] {
        let missing = format!("01921389d31174628f80cb5f79aa13{offset:02x}");
        assert!(
            query(&reference(
                &missing,
                a.team,
                subject_type,
                "01921389d31174628f80cb5f79aa1399"
            ))
            .execute(&pool)
            .await
            .is_err(),
            "missing {subject_type} subject succeeded"
        );
        let cross_team = format!("01921389d31174628f80cb5f79aa14{offset:02x}");
        assert!(
            query(&reference(&cross_team, a.team, subject_type, foreign))
                .execute(&pool)
                .await
                .is_err(),
            "cross-Team {subject_type} subject succeeded"
        );
        query(&reference(local, a.team, subject_type, local))
            .execute(&pool)
            .await?;
    }
    assert_eq!(
        query_scalar::<_, i64>("SELECT COUNT(*) FROM \"references\"")
            .fetch_one(&pool)
            .await?,
        4
    );
    Ok(())
}

#[tokio::test]
async fn parent_updates_cannot_break_existing_artifact_or_reference_relationships()
-> Result<(), Box<dyn Error>> {
    let pool = database().await?;
    let a = graph(&pool, "a").await?;
    let b = graph(&pool, "b").await?;
    query(&reference(
        "01921389d31174628f80cb5f79aa1331",
        a.team,
        "TASK",
        a.task,
    ))
    .execute(&pool)
    .await?;
    for (name, statement) in [
        (
            "Artifact task",
            format!(
                "UPDATE artifacts SET task_id=X'{}' WHERE id=X'{}'",
                a.other_task, a.artifact
            ),
        ),
        (
            "Artifact run",
            format!(
                "UPDATE artifacts SET run_id=X'{}' WHERE id=X'{}'",
                a.other_run, a.artifact
            ),
        ),
        (
            "Artifact role",
            format!(
                "UPDATE artifacts SET creator_role_id=X'{}' WHERE id=X'{}'",
                a.other_role, a.artifact
            ),
        ),
        (
            "Artifact Team",
            format!(
                "UPDATE artifacts SET team_id=X'{}' WHERE id=X'{}'",
                b.team, a.artifact
            ),
        ),
        (
            "Reference Team",
            format!("UPDATE \"references\" SET team_id=X'{}'", b.team),
        ),
    ] {
        assert!(
            query(&statement).execute(&pool).await.is_err(),
            "{name} update unexpectedly succeeded"
        );
    }
    Ok(())
}

#[tokio::test]
async fn replace_cannot_swap_a_referenced_artifact_into_another_team() -> Result<(), Box<dyn Error>>
{
    let pool = database().await?;
    assert_eq!(
        query_scalar::<_, i64>("PRAGMA recursive_triggers")
            .fetch_one(&pool)
            .await?,
        0
    );
    let a = graph(&pool, "a").await?;
    let b = graph(&pool, "b").await?;
    query(&reference(
        "01921389d31174628f80cb5f79aa1332",
        a.team,
        "ARTIFACT",
        a.artifact,
    ))
    .execute(&pool)
    .await?;
    let replace = artifact(a.artifact, b.team, b.task, b.run, b.role)
        .replace("INSERT INTO artifacts", "INSERT OR REPLACE INTO artifacts");
    assert!(
        query(&replace).execute(&pool).await.is_err(),
        "OR REPLACE swapped a referenced Artifact into Team B"
    );
    assert_eq!(
        query_scalar::<_, String>(&format!(
            "SELECT hex(team_id) FROM artifacts WHERE id=X'{}'",
            a.artifact
        ))
        .fetch_one(&pool)
        .await?,
        a.team.to_uppercase()
    );
    assert_eq!(
        query_scalar::<_, String>(
            "SELECT hex(team_id) FROM \"references\" WHERE id=X'01921389d31174628f80cb5f79aa1332'"
        )
        .fetch_one(&pool)
        .await?,
        a.team.to_uppercase()
    );
    Ok(())
}

#[tokio::test]
async fn replace_cannot_delete_a_referenced_task_through_its_display_id_conflict()
-> Result<(), Box<dyn Error>> {
    let pool = database().await?;
    assert_eq!(
        query_scalar::<_, i64>("PRAGMA recursive_triggers")
            .fetch_one(&pool)
            .await?,
        0
    );
    let (task, replacement) = minimally_referenced_subject(&pool, "TASK").await?;
    let replace = format!(
        "INSERT OR REPLACE INTO tasks VALUES(X'{replacement}',X'01921389d31174628f80cb5f79aa1601',X'01921389d31174628f80cb5f79aa1602','task','replacement','','DOING',1,1,1,NULL,NULL)"
    );
    assert!(
        query(&replace).execute(&pool).await.is_err(),
        "OR REPLACE deleted a referenced Task through its Team display_id conflict"
    );
    assert_eq!(
        query_scalar::<_, i64>(&format!("SELECT COUNT(*) FROM tasks WHERE id=X'{task}'"))
            .fetch_one(&pool)
            .await?,
        1
    );
    assert_eq!(
        query_scalar::<_, i64>(&format!(
            "SELECT COUNT(*) FROM \"references\" WHERE subject_type='TASK' AND subject_id=X'{task}'"
        ))
        .fetch_one(&pool)
        .await?,
        1
    );
    Ok(())
}
