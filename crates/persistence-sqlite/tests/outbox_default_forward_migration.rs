//! DEV-02-07: preserve Outbox integrity while correcting its frozen default.
use persistence_sqlite::migrate;
use sqlx_core::{migrate::Migrator, query::query, query_scalar::query_scalar};
use sqlx_sqlite::{SqliteConnectOptions, SqlitePool, SqlitePoolOptions};
use std::{error::Error, path::PathBuf, str::FromStr};
use teamspace_domain::DEFAULT_MAX_RETRIES;

fn id(offset: u16) -> String {
    let value = format!("01921389d31174628f80cb5f79abd{offset:03x}");
    assert_eq!(value.len(), 32);
    value
}
async fn db(version: Option<i64>) -> Result<SqlitePool, Box<dyn Error>> {
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(SqliteConnectOptions::from_str("sqlite::memory:")?.foreign_keys(true))
        .await?;
    if let Some(version) = version {
        let mut migrator =
            Migrator::new(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("migrations")).await?;
        migrator
            .migrations
            .to_mut()
            .retain(|m| m.version <= version);
        migrator.run(&pool).await?;
    } else {
        migrate(&pool).await?;
    }
    Ok(pool)
}
async fn seed(pool: &SqlitePool) -> Result<(), Box<dyn Error>> {
    query(&format!(
        "INSERT INTO teams VALUES(X'{}','team','ACTIVE',1,1,1,NULL)",
        id(1)
    ))
    .execute(pool)
    .await?;
    query(&format!("INSERT INTO events(id,team_id,event_type,actor_type,subject_type,subject_id,object_version,occurred_at,hash) VALUES(X'{}',X'{}','audit.recorded','SYSTEM','TEAM',X'{}',1,1,?)", id(2),id(1),id(1)))
        .bind("0".repeat(64)).execute(pool).await?;
    for table in ["teams", "events"] {
        assert_eq!(
            query_scalar::<_, i64>(&format!("SELECT COUNT(*) FROM {table}"))
                .fetch_one(pool)
                .await?,
            1,
            "seed must create exactly one {table} row"
        );
    }
    Ok(())
}
fn insert_default(offset: u16) -> String {
    format!(
        "INSERT INTO outbox(id,team_id,event_id,destination,payload,status,idempotency_key,created_at,scheduled_at) VALUES(X'{}',X'{}',X'{}','consumer-{offset}','{{}}','PENDING','key-{offset}',1,1)",
        id(offset),
        id(1),
        id(2)
    )
}
#[tokio::test]
async fn omitted_max_retries_uses_the_frozen_default() -> Result<(), Box<dyn Error>> {
    let pool = db(None).await?;
    seed(&pool).await?;
    query(&insert_default(10)).execute(&pool).await?;
    assert_eq!(
        query_scalar::<_, i64>("SELECT max_retries FROM outbox")
            .fetch_one(&pool)
            .await?,
        i64::from(DEFAULT_MAX_RETRIES)
    );
    Ok(())
}

async fn data(pool: &SqlitePool) -> Result<String, Box<dyn Error>> {
    Ok(query_scalar("SELECT json_group_array(json_object('id',hex(id),'team',hex(team_id),'event',hex(event_id),'destination',destination,'payload',payload,'status',status,'retry_count',retry_count,'max_retries',max_retries,'last_error',last_error,'key',idempotency_key,'created_at',created_at,'scheduled_at',scheduled_at,'published_at',published_at)) FROM (SELECT id,team_id,event_id,destination,payload,status,retry_count,max_retries,last_error,idempotency_key,created_at,scheduled_at,published_at FROM outbox ORDER BY id)").fetch_one(pool).await?)
}
async fn objects(pool: &SqlitePool) -> Result<String, Box<dyn Error>> {
    Ok(query_scalar("SELECT json_group_array(json_object('type',type,'name',name,'sql',sql)) FROM (SELECT type,name,sql FROM sqlite_master WHERE tbl_name='outbox' AND type IN ('trigger','index') AND name NOT LIKE 'sqlite_%' ORDER BY type,name)").fetch_one(pool).await?)
}
async fn table(pool: &SqlitePool) -> Result<String, Box<dyn Error>> {
    Ok(
        query_scalar("SELECT sql FROM sqlite_master WHERE type='table' AND name='outbox'")
            .fetch_one(pool)
            .await?,
    )
}

#[tokio::test]
async fn upgrade_preserves_all_rows_and_changes_only_the_column_default()
-> Result<(), Box<dyn Error>> {
    let pool = db(Some(23)).await?;
    seed(&pool).await?;
    assert_eq!(
        query_scalar::<_, String>(
            "SELECT dflt_value FROM pragma_table_info('outbox') WHERE name='max_retries'"
        )
        .fetch_one(&pool)
        .await?,
        "5"
    );
    for (offset, status, ceiling) in [
        (10, "PENDING", 0),
        (11, "PROCESSING", 1),
        (12, "PUBLISHED", 5),
        (13, "FAILED", 17),
        (14, "DEAD_LETTER", 3),
    ] {
        let published = if status == "PUBLISHED" { "9" } else { "NULL" };
        query(&format!("INSERT INTO outbox(id,team_id,event_id,destination,payload,status,retry_count,max_retries,last_error,idempotency_key,created_at,scheduled_at,published_at) VALUES(X'{}',X'{}',X'{}','consumer-{offset}',?,'{status}',0,{ceiling},'historical error','key-{offset}',2,7,{published})",id(offset),id(1),id(2)))
            .bind(r#"{ "retained": [1, null, "内容"] }"#).execute(&pool).await?;
    }
    let rows = data(&pool).await?;
    let guards = objects(&pool).await?;
    let ddl = table(&pool).await?;
    migrate(&pool).await?;
    assert_eq!(
        data(&pool).await?,
        rows,
        "every persisted value must survive"
    );
    assert_eq!(
        objects(&pool).await?,
        guards,
        "all four guards and three named indexes must survive"
    );
    assert_eq!(
        table(&pool).await?.replace("\"outbox\"", "outbox"),
        ddl.replace("DEFAULT 5", "DEFAULT 3")
    );
    assert_eq!(
        query_scalar::<_, i64>("SELECT COUNT(*) FROM pragma_foreign_key_check")
            .fetch_one(&pool)
            .await?,
        0
    );
    query(&insert_default(15)).execute(&pool).await?;
    assert_eq!(
        query_scalar::<_, i64>(&format!(
            "SELECT max_retries FROM outbox WHERE id=X'{}'",
            id(15)
        ))
        .fetch_one(&pool)
        .await?,
        3
    );
    migrate(&pool).await?;
    // Stable form, not `MAX(version)`: this asserts that 0024 (the migration
    // under test) recorded exactly one successful application. `MAX(version)`
    // tracks the length of the whole migration chain, so it breaks every time
    // an unrelated migration is registered. See MR-04 in
    // `docs/reviews/MINIMAX-RETURN-AUDIT-2026-10-08.md`.
    assert_eq!(
        query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM _sqlx_migrations WHERE version = 24 AND success = TRUE"
        )
        .fetch_one(&pool)
        .await?,
        1,
        "0024 did not record exactly one successful application"
    );
    Ok(())
}

#[tokio::test]
async fn rebuild_retains_team_dedup_and_terminal_evidence_guards() -> Result<(), Box<dyn Error>> {
    let pool = db(None).await?;
    seed(&pool).await?;
    query(&insert_default(10)).execute(&pool).await?;
    query("UPDATE outbox SET status='PROCESSING',retry_count=1")
        .execute(&pool)
        .await?;
    assert_eq!(
        query_scalar::<_, i64>("SELECT COUNT(*) FROM outbox WHERE status='PROCESSING' AND retry_count=1 AND idempotency_key='key-10'")
            .fetch_one(&pool)
            .await?,
        1,
        "the REPLACE probe must actually reset a persisted attempt"
    );
    query(&format!(
        "INSERT INTO teams VALUES(X'{}','other','ACTIVE',1,1,1,NULL)",
        id(3)
    ))
    .execute(&pool)
    .await?;
    let before = data(&pool).await?;
    for sql in [
        insert_default(10).replacen("INSERT INTO", "INSERT OR REPLACE INTO", 1),
        insert_default(11).replace("consumer-11", "consumer-10"),
        format!("UPDATE outbox SET team_id=X'{}'", id(3)),
        "UPDATE outbox SET idempotency_key='new-key'".to_owned(),
        "UPDATE outbox SET destination='new-destination'".to_owned(),
        "UPDATE outbox SET max_retries=-1".to_owned(),
        "UPDATE outbox SET retry_count=-1".to_owned(),
        "UPDATE outbox SET payload='not-json'".to_owned(),
        "UPDATE outbox SET id=X'00000000000000000000000000000000'".to_owned(),
        "UPDATE outbox SET id=X'01921389d31164628f80cb5f79abd999'".to_owned(),
        "UPDATE outbox SET id=X'01921389d31174624f80cb5f79abd999'".to_owned(),
    ] {
        assert!(
            query(&sql).execute(&pool).await.is_err(),
            "forbidden write accepted: {sql}"
        );
        assert_eq!(data(&pool).await?, before);
    }
    // Positive publication and terminal evidence stay protected afterwards.
    query("UPDATE outbox SET status='PUBLISHED',published_at=10")
        .execute(&pool)
        .await?;
    let published = data(&pool).await?;
    for sql in [
        "UPDATE outbox SET status='PENDING'",
        "UPDATE outbox SET published_at=NULL",
    ] {
        let error = query(sql)
            .execute(&pool)
            .await
            .expect_err("terminal evidence cannot be cleared");
        assert!(
            error
                .to_string()
                .contains("Outbox deduplication state is immutable")
        );
        assert_eq!(data(&pool).await?, published);
    }
    Ok(())
}

#[tokio::test]
async fn failed_rebuild_rolls_back_schema_and_keeps_corrupt_history_for_diagnosis()
-> Result<(), Box<dyn Error>> {
    let pool = db(Some(23)).await?;
    seed(&pool).await?;
    query(&insert_default(10)).execute(&pool).await?;
    query("PRAGMA ignore_check_constraints=ON")
        .execute(&pool)
        .await?;
    query("UPDATE outbox SET max_retries=-1")
        .execute(&pool)
        .await?;
    query("PRAGMA ignore_check_constraints=OFF")
        .execute(&pool)
        .await?;
    let rows = data(&pool).await?;
    let guards = objects(&pool).await?;
    let ddl = table(&pool).await?;
    let error = migrate(&pool)
        .await
        .expect_err("invalid history cannot be silently repaired");
    assert!(
        error.to_string().contains("max_retries"),
        "unexpected migration failure: {error}"
    );
    assert_eq!(
        query_scalar::<_, i64>("SELECT MAX(version) FROM _sqlx_migrations")
            .fetch_one(&pool)
            .await?,
        23
    );
    assert_eq!(
        query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM sqlite_master WHERE name='outbox_default_rebuild'"
        )
        .fetch_one(&pool)
        .await?,
        0
    );
    assert_eq!(data(&pool).await?, rows);
    assert_eq!(objects(&pool).await?, guards);
    assert_eq!(table(&pool).await?, ddl);
    Ok(())
}
