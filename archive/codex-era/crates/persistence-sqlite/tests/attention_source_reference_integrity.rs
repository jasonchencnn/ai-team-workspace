//! DEV-02-09: protect both ends of a polymorphic same-Team reference.
use persistence_sqlite::migrate;
use sqlx_core::{
    migrate::{Migration, MigrationType, Migrator},
    query::query,
    query_scalar::query_scalar,
    row::Row,
};
use sqlx_sqlite::{SqliteConnectOptions, SqlitePool, SqlitePoolOptions};
use std::{borrow::Cow, error::Error, path::PathBuf, str::FromStr};

type TestResult = Result<(), Box<dyn Error>>;
const A: &str = "01921389d31174628f80cb5f79aad001";
const B: &str = "01921389d31174628f80cb5f79aad002";
const ROLE_A: &str = "01921389d31174628f80cb5f79aad003";
const ROLE_B: &str = "01921389d31174628f80cb5f79aad004";
const TASK: &str = "01921389d31174628f80cb5f79aad005";
const MESSAGE: &str = "01921389d31174628f80cb5f79aad006";
const RUN: &str = "01921389d31174628f80cb5f79aad007";
const APPROVAL: &str = "01921389d31174628f80cb5f79aad008";
const HUMAN: &str = "01921389d31174628f80cb5f79aad009";
const THREAD_A: &str = "01921389d31174628f80cb5f79aad00a";
const THREAD_B: &str = "01921389d31174628f80cb5f79aad00b";
const ATTENTION: &str = "01921389d31174628f80cb5f79aad00c";
const HARNESS: &str = "01921389d31174628f80cb5f79aad00d";
const ADAPTER: &str = "01921389d31174628f80cb5f79aad00e";
const BINDING: &str = "01921389d31174628f80cb5f79aad00f";

async fn disk() -> Result<Migrator, Box<dyn Error>> {
    Ok(Migrator::new(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("migrations")).await?)
}
async fn upgrade(pool: &SqlitePool, version: i64) -> TestResult {
    let mut m = disk().await?;
    m.migrations.to_mut().retain(|m| m.version <= version);
    m.run(pool).await?;
    Ok(())
}
async fn database(version: i64) -> Result<SqlitePool, Box<dyn Error>> {
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(SqliteConnectOptions::from_str("sqlite::memory:")?.foreign_keys(true))
        .await?;
    upgrade(&pool, version).await?;
    for (team, role, thread) in [(A, ROLE_A, THREAD_A), (B, ROLE_B, THREAD_B)] {
        for sql in [
            format!("INSERT INTO teams VALUES(X'{team}','team','ACTIVE',1,1,1,NULL)"),
            format!(
                "INSERT INTO roles VALUES(X'{role}',X'{team}','role','role','ACTIVE',1,1,1,NULL)"
            ),
            format!("INSERT INTO threads VALUES(X'{thread}',X'{team}','thread','OPEN',1,1,1)"),
        ] {
            query(&sql).execute(&pool).await?;
        }
    }
    for sql in [
        format!(
            "INSERT INTO tasks VALUES(X'{TASK}',X'{A}',X'{ROLE_A}','T1','task','reason','BLOCKED',1,1,1,NULL,NULL)"
        ),
        format!(
            "INSERT INTO messages(id,team_id,thread_id,sender_actor_type,from_human_id,to_role_id,message_type,body,status,version,created_at,updated_at) VALUES(X'{MESSAGE}',X'{A}',X'{THREAD_A}','HUMAN',X'{HUMAN}',X'{ROLE_A}','DIRECT','body','DRAFT',1,1,1)"
        ),
        format!(
            "INSERT INTO harness_instances VALUES(X'{HARNESS}',X'{A}','mock','harness','ACTIVE','[]',NULL,1,1,1,NULL)"
        ),
        format!(
            "INSERT INTO adapters VALUES(X'{ADAPTER}',X'{A}','adapter','1','/adapter','{{}}',1,1,1)"
        ),
        format!(
            "INSERT INTO bindings VALUES(X'{BINDING}',X'{A}',X'{ROLE_A}',X'{HARNESS}',X'{ADAPTER}','ACTIVE',1,1,'{{}}',1,1,1,NULL)"
        ),
        format!(
            "INSERT INTO runs(id,team_id,task_id,role_id,binding_id,harness_instance_id,attempt,fencing_token,status,version,created_at,started_at,completed_at) VALUES(X'{RUN}',X'{A}',X'{TASK}',X'{ROLE_A}',X'{BINDING}',X'{HARNESS}',1,1,'FAILED',1,1,1,2)"
        ),
        format!(
            "INSERT INTO approvals(id,team_id,requester_actor_type,requester_role_id,action,resource_type,resource_scope,reason,status,version,expires_at,created_at) VALUES(X'{APPROVAL}',X'{A}','ROLE',X'{ROLE_A}','UPDATE','TASK_MUTATION','task','reason','PENDING',1,3,1)"
        ),
    ] {
        query(&sql).execute(&pool).await?;
    }
    assert_eq!(
        query_scalar::<_, i64>("PRAGMA foreign_keys")
            .fetch_one(&pool)
            .await?,
        1
    );
    for (table, count) in [
        ("teams", 2),
        ("roles", 2),
        ("threads", 2),
        ("tasks", 1),
        ("messages", 1),
        ("runs", 1),
        ("approvals", 1),
    ] {
        assert_eq!(
            query_scalar::<_, i64>(&format!("SELECT count(*) FROM {table}"))
                .fetch_one(&pool)
                .await?,
            count
        );
    }
    Ok(pool)
}
async fn attention(pool: &SqlitePool, kind: &str, source: &str, team: &str) -> TestResult {
    query(&format!("INSERT INTO attention_items(id,team_id,attention_type,source_type,source_id,title,summary,status,version,created_at,updated_at) VALUES(X'{ATTENTION}',X'{team}','SECURITY_ALERT','{kind}',X'{source}','title','summary','OPEN',1,1,1)"))
        .execute(pool).await?;
    assert_eq!(
        query_scalar::<_, i64>("SELECT count(*) FROM attention_items")
            .fetch_one(pool)
            .await?,
        1
    );
    Ok(())
}
async fn snapshot(pool: &SqlitePool) -> Result<Vec<(String, Vec<String>)>, Box<dyn Error>> {
    let tables = query_scalar::<_, String>("SELECT name FROM sqlite_master WHERE type='table' AND name NOT LIKE 'sqlite_%' ORDER BY name").fetch_all(pool).await?;
    let mut result = Vec::new();
    for table in tables {
        let columns = query(&format!("PRAGMA table_info(\"{table}\")"))
            .fetch_all(pool)
            .await?;
        let expression = columns
            .iter()
            .map(|r| format!("quote(\"{}\")", r.get::<String, _>("name")))
            .collect::<Vec<_>>()
            .join(" || '|' || ");
        result.push((
            table.clone(),
            query_scalar::<_, String>(&format!(
                "SELECT {expression} AS snapshot FROM \"{table}\" ORDER BY snapshot"
            ))
            .fetch_all(pool)
            .await?,
        ));
    }
    Ok(result)
}
fn message_move() -> String {
    format!(
        "UPDATE messages SET team_id=X'{B}',thread_id=X'{THREAD_B}',to_role_id=X'{ROLE_B}',version=2 WHERE id=X'{MESSAGE}'"
    )
}
async fn movable_task(pool: &SqlitePool) -> TestResult {
    // A separate source without Run/Event history exposes the Attention-only edge.
    query(&format!("INSERT INTO tasks VALUES(X'01921389d31174628f80cb5f79aad010',X'{A}',X'{ROLE_A}','T2','task','reason','BLOCKED',1,1,1,NULL,NULL)"))
        .execute(pool).await?;
    Ok(())
}
const MOVABLE_TASK: &str = "01921389d31174628f80cb5f79aad010";
fn task_move() -> String {
    format!(
        "UPDATE tasks SET team_id=X'{B}',owner_role_id=X'{ROLE_B}',version=2,updated_at=2 WHERE id=X'{MOVABLE_TASK}'"
    )
}

#[tokio::test]
async fn v34_parent_moves_really_leave_cross_team_attention() -> TestResult {
    {
        let (kind, id, sql) = ("TASK", MOVABLE_TASK, task_move());
        let pool = database(34).await?;
        movable_task(&pool).await?;
        attention(&pool, kind, id, A).await?;
        assert_eq!(query_scalar::<_,i64>("SELECT count(*) FROM sqlite_master WHERE name IN ('tasks_attention_source_team_update','messages_attention_source_team_update')").fetch_one(&pool).await?,0);
        assert_eq!(query(&sql).execute(&pool).await?.rows_affected(), 1);
        let table = if kind == "TASK" { "tasks" } else { "messages" };
        assert_eq!(query_scalar::<_,i64>(&format!("SELECT count(*) FROM attention_items a JOIN {table} s ON s.id=a.source_id WHERE s.team_id IS NOT a.team_id")).fetch_one(&pool).await?,1);
    }
    Ok(())
}

#[tokio::test]
async fn current_parent_moves_preserve_open_and_dismissed_references() -> TestResult {
    {
        let (kind, id, sql) = ("TASK", MOVABLE_TASK, task_move());
        for dismissed in [false, true] {
            let pool = database(35).await?;
            movable_task(&pool).await?;
            attention(&pool, kind, id, A).await?;
            if dismissed {
                query("UPDATE attention_items SET status='DISMISSED',dismiss_reason='reason',dismissed_at=2,version=2").execute(&pool).await?;
            }
            let before = snapshot(&pool).await?;
            let error = query(&sql)
                .execute(&pool)
                .await
                .expect_err("parent Team move must not corrupt reference");
            assert!(
                error
                    .to_string()
                    .contains("Attention source Team is retained"),
                "{error}"
            );
            assert_eq!(snapshot(&pool).await?, before);
        }
    }
    Ok(())
}

#[tokio::test]
async fn v18_cross_team_history_survives_v34_but_v35_rejects_it_atomically() -> TestResult {
    for (kind, id) in [
        ("TASK", TASK),
        ("RUN", RUN),
        ("MESSAGE", MESSAGE),
        ("APPROVAL", APPROVAL),
    ] {
        let pool = database(18).await?;
        attention(&pool, kind, id, B).await?;
        upgrade(&pool, 34).await?;
        let before = snapshot(&pool).await?;
        let schema_sql = "SELECT name || ':' || coalesce(sql,'') FROM sqlite_master ORDER BY name";
        let schema = query_scalar::<_, String>(schema_sql)
            .fetch_all(&pool)
            .await?;
        let error = migrate(&pool)
            .await
            .expect_err("legacy cross-Team reference must block upgrade");
        assert!(error.to_string().contains("valid = 1"), "{error}");
        assert_eq!(snapshot(&pool).await?, before);
        assert_eq!(
            query_scalar::<_, String>(schema_sql)
                .fetch_all(&pool)
                .await?,
            schema
        );
    }
    Ok(())
}

#[tokio::test]
async fn same_team_business_edits_and_unreferenced_moves_remain_legal() -> TestResult {
    let pool = database(35).await?;
    movable_task(&pool).await?;
    attention(&pool, "TASK", MOVABLE_TASK, A).await?;
    query(&format!(
        "UPDATE tasks SET title='edited',version=2,updated_at=2 WHERE id=X'{MOVABLE_TASK}'"
    ))
    .execute(&pool)
    .await?;
    query("UPDATE messages SET body='edited',version=2")
        .execute(&pool)
        .await?;
    assert_eq!(
        query_scalar::<_, String>("SELECT body FROM messages")
            .fetch_one(&pool)
            .await?,
        "edited"
    );
    let before = snapshot(&pool).await?;
    assert!(query(&message_move()).execute(&pool).await.is_err());
    assert_eq!(snapshot(&pool).await?, before);
    query(&format!("INSERT INTO tasks VALUES(X'01921389d31174628f80cb5f79aad011',X'{A}',X'{ROLE_A}','T3','unreferenced','reason','BLOCKED',1,1,1,NULL,NULL)")).execute(&pool).await?;
    assert_eq!(query(&format!("UPDATE tasks SET team_id=X'{B}',owner_role_id=X'{ROLE_B}',version=2 WHERE id=X'01921389d31174628f80cb5f79aad011'")).execute(&pool).await?.rows_affected(),1);
    Ok(())
}

#[tokio::test]
async fn later_source_state_changes_do_not_invalidate_historical_resolution() -> TestResult {
    let pool = database(34).await?;
    movable_task(&pool).await?;
    attention(&pool, "TASK", MOVABLE_TASK, A).await?;
    query(&format!(
        "UPDATE tasks SET status='READY',version=2 WHERE id=X'{MOVABLE_TASK}'"
    ))
    .execute(&pool)
    .await?;
    query("UPDATE attention_items SET status='RESOLVED',resolved_at=2,version=2")
        .execute(&pool)
        .await?;
    query(&format!(
        "UPDATE tasks SET status='DOING',version=3 WHERE id=X'{MOVABLE_TASK}'"
    ))
    .execute(&pool)
    .await?;
    query(&format!(
        "UPDATE tasks SET status='BLOCKED',version=4 WHERE id=X'{MOVABLE_TASK}'"
    ))
    .execute(&pool)
    .await?;
    let mut before = snapshot(&pool).await?;
    before.retain(|(t, _)| t != "_sqlx_migrations");
    migrate(&pool).await?;
    migrate(&pool).await?;
    let mut after = snapshot(&pool).await?;
    after.retain(|(t, _)| t != "_sqlx_migrations");
    assert_eq!(after, before);
    assert_eq!(
        query_scalar::<_, i64>(
            "SELECT count(*) FROM _sqlx_migrations WHERE version=35 AND success=1"
        )
        .fetch_one(&pool)
        .await?,
        1
    );
    assert!(query(&task_move()).execute(&pool).await.is_err());
    Ok(())
}

#[tokio::test]
async fn post_create_failure_rolls_back_the_entire_source_upgrade() -> TestResult {
    let pool = database(34).await?;
    attention(&pool, "TASK", TASK, A).await?;
    let before = snapshot(&pool).await?;
    let schema_sql = "SELECT name || ':' || coalesce(sql,'') FROM sqlite_master ORDER BY name";
    let schema = query_scalar::<_, String>(schema_sql)
        .fetch_all(&pool)
        .await?;
    let mut m = disk().await?;
    let sql = m
        .migrations
        .iter()
        .find(|m| m.version == 35)
        .ok_or("migration 35 missing")?
        .sql
        .to_string();
    m.migrations.to_mut().retain(|m| m.version <= 34);
    m.migrations.to_mut().push(Migration::new(
        35,
        Cow::Borrowed("injected failure"),
        MigrationType::Simple,
        Cow::Owned(format!(
            "{sql}\nINSERT INTO deliberate_missing_failure_table VALUES(1);"
        )),
        false,
    ));
    let error = m
        .run(&pool)
        .await
        .expect_err("injected failure must roll back");
    assert!(
        error
            .to_string()
            .contains("deliberate_missing_failure_table"),
        "{error}"
    );
    assert_eq!(snapshot(&pool).await?, before);
    assert_eq!(
        query_scalar::<_, String>(schema_sql)
            .fetch_all(&pool)
            .await?,
        schema
    );
    migrate(&pool).await?;
    Ok(())
}
