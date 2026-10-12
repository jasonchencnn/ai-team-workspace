//! DEV-02-09: dedupe may merge presentation, never delete Notification facts.
use std::{borrow::Cow, error::Error, path::PathBuf, str::FromStr};

use persistence_sqlite::migrate;
use sqlx_core::{
    migrate::{Migration, MigrationType, Migrator},
    query::query,
    query_scalar::query_scalar,
    row::Row,
};
use sqlx_sqlite::{SqliteConnectOptions, SqlitePool, SqlitePoolOptions};

type TestResult = Result<(), Box<dyn Error>>;
const TEAM: &str = "01921389d31174628f80cb5f79aac001";
const OTHER_TEAM: &str = "01921389d31174628f80cb5f79aac002";
const HUMAN: &str = "01921389d31174628f80cb5f79aac003";
const OTHER_HUMAN: &str = "01921389d31174628f80cb5f79aac004";
const ORIGINAL: &str = "01921389d31174628f80cb5f79aac005";
const REPLACEMENT: &str = "01921389d31174628f80cb5f79aac006";

async fn disk_migrations() -> Result<Migrator, Box<dyn Error>> {
    Ok(Migrator::new(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("migrations")).await?)
}

async fn database(v33: bool, recursive: bool) -> Result<SqlitePool, Box<dyn Error>> {
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(SqliteConnectOptions::from_str("sqlite::memory:")?.foreign_keys(true))
        .await?;
    query(if recursive {
        "PRAGMA recursive_triggers=ON"
    } else {
        "PRAGMA recursive_triggers=OFF"
    })
    .execute(&pool)
    .await?;
    if v33 {
        let mut migrator = disk_migrations().await?;
        migrator.migrations.to_mut().retain(|m| m.version <= 33);
        migrator.run(&pool).await?;
    } else {
        migrate(&pool).await?;
    }
    assert_eq!(
        query_scalar::<_, i64>("PRAGMA foreign_keys")
            .fetch_one(&pool)
            .await?,
        1
    );
    for team in [TEAM, OTHER_TEAM] {
        query(&format!(
            "INSERT INTO teams VALUES(X'{team}','team','ACTIVE',1,1,1,NULL)"
        ))
        .execute(&pool)
        .await?;
    }
    assert_eq!(
        query_scalar::<_, i64>("SELECT count(*) FROM teams")
            .fetch_one(&pool)
            .await?,
        2
    );
    Ok(pool)
}

fn insert(mode: &str, id: &str, team: &str, human: &str, key: Option<&str>) -> String {
    // All keys in this fixture are local constants, without SQL delimiters.
    let key = key.map_or_else(|| "NULL".to_owned(), |key| format!("'{key}'"));
    format!(
        "{mode} INTO notifications(id,team_id,recipient_human_id,severity,title,body,status,dedupe_key,version,created_at) VALUES(X'{id}',X'{team}',X'{human}','INFO','title','body','UNREAD',{key},1,1)"
    )
}

async fn seed_notification(pool: &SqlitePool, status: &str) -> TestResult {
    query(&insert("INSERT", ORIGINAL, TEAM, HUMAN, Some("same")))
        .execute(pool)
        .await?;
    if status == "READ" {
        assert_eq!(
            query("UPDATE notifications SET status='READ',read_at=2,version=2")
                .execute(pool)
                .await?
                .rows_affected(),
            1
        );
    }
    assert_eq!(
        query_scalar::<_, String>("SELECT status FROM notifications")
            .fetch_one(pool)
            .await?,
        status
    );
    Ok(())
}

/// SQLite quote() includes storage types and every column, including nullable refs.
async fn rows(pool: &SqlitePool, table: &str) -> Result<Vec<String>, Box<dyn Error>> {
    let columns = query(&format!("PRAGMA table_info(\"{table}\")"))
        .fetch_all(pool)
        .await?;
    assert!(!columns.is_empty());
    let expression = columns
        .iter()
        .map(|row| {
            let name: String = row.get("name");
            format!("quote(\"{name}\")")
        })
        .collect::<Vec<_>>()
        .join(" || '|' || ");
    Ok(query_scalar(&format!(
        "SELECT {expression} AS snapshot FROM \"{table}\" ORDER BY snapshot"
    ))
    .fetch_all(pool)
    .await?)
}

async fn business_rows(pool: &SqlitePool) -> Result<Vec<(String, Vec<String>)>, Box<dyn Error>> {
    let tables = query_scalar::<_, String>("SELECT name FROM sqlite_master WHERE type='table' AND name NOT LIKE 'sqlite_%' AND name != '_sqlx_migrations' ORDER BY name")
        .fetch_all(pool).await?;
    let mut snapshot = Vec::new();
    for table in tables {
        snapshot.push((table.clone(), rows(pool, &table).await?));
    }
    Ok(snapshot)
}

#[tokio::test]
async fn v33_replacement_actually_deletes_an_unread_or_read_fact() -> TestResult {
    for status in ["UNREAD", "READ"] {
        let pool = database(true, false).await?;
        assert_eq!(query_scalar::<_, i64>("SELECT count(*) FROM sqlite_master WHERE name='notifications_active_dedupe_retention_insert'").fetch_one(&pool).await?, 0);
        seed_notification(&pool, status).await?;
        assert_eq!(
            query(&insert(
                "INSERT OR REPLACE",
                REPLACEMENT,
                TEAM,
                HUMAN,
                Some("same")
            ))
            .execute(&pool)
            .await?
            .rows_affected(),
            1
        );
        assert_eq!(
            query_scalar::<_, i64>(&format!(
                "SELECT count(*) FROM notifications WHERE id=X'{ORIGINAL}'"
            ))
            .fetch_one(&pool)
            .await?,
            0,
            "old retained fact must actually disappear on v33"
        );
        assert_eq!(
            query_scalar::<_, String>("SELECT lower(hex(id)) FROM notifications")
                .fetch_one(&pool)
                .await?,
            REPLACEMENT
        );
    }
    Ok(())
}

#[tokio::test]
async fn an_active_dedupe_collision_cannot_replace_any_notification_fact() -> TestResult {
    for recursive in [false, true] {
        for status in ["UNREAD", "READ"] {
            let pool = database(false, recursive).await?;
            seed_notification(&pool, status).await?;
            let before = rows(&pool, "notifications").await?;
            for mode in ["INSERT OR REPLACE", "REPLACE", "INSERT", "INSERT OR IGNORE"] {
                let error = query(&insert(mode, REPLACEMENT, TEAM, HUMAN, Some("same")))
                    .execute(&pool)
                    .await
                    .expect_err("dedupe collision must retain original fact");
                assert!(
                    error
                        .to_string()
                        .contains("Notification dedupe conflict retains history"),
                    "{error}"
                );
                assert_eq!(rows(&pool, "notifications").await?, before);
            }
            assert_eq!(query_scalar::<_, i64>("SELECT count(*) FROM sqlite_master WHERE name='notifications_active_dedupe_retention_insert'").fetch_one(&pool).await?, 1);
        }
    }
    Ok(())
}

#[tokio::test]
async fn distinct_recipient_team_key_and_null_key_still_create_notifications() -> TestResult {
    let pool = database(false, false).await?;
    seed_notification(&pool, "UNREAD").await?;
    for (id, team, human, key) in [
        (
            "01921389d31174628f80cb5f79aac010",
            TEAM,
            OTHER_HUMAN,
            Some("same"),
        ),
        (
            "01921389d31174628f80cb5f79aac011",
            OTHER_TEAM,
            HUMAN,
            Some("same"),
        ),
        (
            "01921389d31174628f80cb5f79aac012",
            TEAM,
            HUMAN,
            Some("different"),
        ),
        ("01921389d31174628f80cb5f79aac013", TEAM, HUMAN, None),
        ("01921389d31174628f80cb5f79aac014", TEAM, HUMAN, None),
    ] {
        assert_eq!(
            query(&insert("INSERT OR REPLACE", id, team, human, key))
                .execute(&pool)
                .await?
                .rows_affected(),
            1
        );
    }
    assert_eq!(rows(&pool, "notifications").await?.len(), 6);
    Ok(())
}

#[tokio::test]
async fn a_terminal_notification_releases_dedupe_without_losing_history() -> TestResult {
    for status in ["DISMISSED", "EXPIRED"] {
        let pool = database(false, false).await?;
        seed_notification(&pool, "READ").await?;
        query(&format!(
            "UPDATE notifications SET status='{status}',version=3"
        ))
        .execute(&pool)
        .await?;
        let original = rows(&pool, "notifications").await?;
        query(&insert(
            "INSERT OR REPLACE",
            REPLACEMENT,
            TEAM,
            HUMAN,
            Some("same"),
        ))
        .execute(&pool)
        .await?;
        let after = rows(&pool, "notifications").await?;
        assert_eq!(after.len(), 2);
        assert!(after.contains(&original[0]));
        assert!(
            query(&insert(
                "INSERT OR REPLACE",
                ORIGINAL,
                TEAM,
                HUMAN,
                Some("different")
            ))
            .execute(&pool)
            .await
            .is_err()
        );
        assert_eq!(rows(&pool, "notifications").await?, after);
    }
    Ok(())
}

#[tokio::test]
async fn a_multirow_collision_aborts_the_whole_statement_and_transaction_can_rollback() -> TestResult
{
    let pool = database(false, false).await?;
    seed_notification(&pool, "UNREAD").await?;
    let before = business_rows(&pool).await?;
    let sql = insert("INSERT OR REPLACE", REPLACEMENT, TEAM, HUMAN, Some("free"));
    let collision = format!(
        "{sql},(X'01921389d31174628f80cb5f79aac015',X'{TEAM}',X'{HUMAN}','INFO','title','body','UNREAD','same',1,1)"
    );
    assert!(query(&collision).execute(&pool).await.is_err());
    assert_eq!(
        business_rows(&pool).await?,
        before,
        "ABORT must undo preceding values in this statement"
    );
    let mut tx = pool.begin().await?;
    query("UPDATE teams SET name='changed',version=2,updated_at=2")
        .execute(&mut *tx)
        .await?;
    assert!(
        query(&insert("REPLACE", REPLACEMENT, TEAM, HUMAN, Some("same")))
            .execute(&mut *tx)
            .await
            .is_err()
    );
    tx.rollback().await?;
    assert_eq!(
        business_rows(&pool).await?,
        before,
        "application rollback must undo preceding transaction writes"
    );
    Ok(())
}

#[tokio::test]
async fn upgrading_v33_preserves_all_business_rows_and_is_idempotent() -> TestResult {
    let pool = database(true, false).await?;
    seed_notification(&pool, "READ").await?;
    let before = business_rows(&pool).await?;
    migrate(&pool).await?;
    migrate(&pool).await?;
    assert_eq!(business_rows(&pool).await?, before);
    assert_eq!(
        query_scalar::<_, i64>(
            "SELECT count(*) FROM _sqlx_migrations WHERE version=34 AND success=1"
        )
        .fetch_one(&pool)
        .await?,
        1
    );
    assert!(
        query(&insert("REPLACE", REPLACEMENT, TEAM, HUMAN, Some("same")))
            .execute(&pool)
            .await
            .is_err()
    );
    assert_eq!(business_rows(&pool).await?, before);
    Ok(())
}

#[tokio::test]
async fn a_failure_after_installing_the_guard_rolls_back_schema_rows_and_ledger() -> TestResult {
    let pool = database(true, false).await?;
    seed_notification(&pool, "UNREAD").await?;
    let before = business_rows(&pool).await?;
    let ledger = rows(&pool, "_sqlx_migrations").await?;
    let schema_sql = "SELECT name || ':' || coalesce(sql,'') FROM sqlite_master ORDER BY name";
    let schema = query_scalar::<_, String>(schema_sql)
        .fetch_all(&pool)
        .await?;
    let mut migrator = disk_migrations().await?;
    let sql = migrator
        .migrations
        .iter()
        .find(|m| m.version == 34)
        .ok_or("migration 34 missing")?
        .sql
        .to_string();
    migrator.migrations.to_mut().retain(|m| m.version <= 33);
    migrator.migrations.to_mut().push(Migration::new(
        34,
        Cow::Borrowed("injected failure"),
        MigrationType::Simple,
        Cow::Owned(format!(
            "{sql}\nINSERT INTO deliberate_missing_failure_table VALUES(1);"
        )),
        false,
    ));
    let error = migrator
        .run(&pool)
        .await
        .expect_err("post-CREATE failure must abort migration");
    assert!(
        error
            .to_string()
            .contains("deliberate_missing_failure_table"),
        "{error}"
    );
    assert_eq!(
        query_scalar::<_, String>(schema_sql)
            .fetch_all(&pool)
            .await?,
        schema
    );
    assert_eq!(business_rows(&pool).await?, before);
    assert_eq!(rows(&pool, "_sqlx_migrations").await?, ledger);
    migrate(&pool).await?;
    assert_eq!(business_rows(&pool).await?, before);
    Ok(())
}
