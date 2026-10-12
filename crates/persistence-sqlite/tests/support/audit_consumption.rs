//! Synthetic isolated databases and deterministic capabilities for consumer tests.
use application::message_expiry::{ExpiryRequest, expire_message};
use persistence_sqlite::{message_expiry::SqliteExpiryStore, migrate};
use sqlx_core::{query::query, query_scalar::query_scalar, raw_sql::raw_sql, row::Row};
use sqlx_sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePool, SqlitePoolOptions};
use std::{error::Error, str::FromStr};
use teamspace_domain::{
    Clock, ExpectedVersion, MessageId, RequestId, TeamId, UtcTimestamp, UuidV7, UuidV7Source,
    Version, idempotency::IdempotencyKeyString,
};

pub type TestResult = Result<(), Box<dyn Error>>;
pub fn uuid(tag: u16) -> UuidV7 {
    UuidV7::parse_protocol(&format!("01921389-d311-7462-8f80-cb5f79ab{tag:04x}")).unwrap()
}
pub struct Time(pub i64);
impl Clock for Time {
    fn now(&self) -> UtcTimestamp {
        UtcTimestamp::from_unix_millis(self.0).unwrap()
    }
}
pub struct Ids(pub u16);
impl UuidV7Source for Ids {
    type Error = std::convert::Infallible;
    fn next_uuid_v7(&mut self) -> Result<UuidV7, Self::Error> {
        self.0 += 1;
        Ok(uuid(self.0))
    }
}
pub async fn seed(pool: &SqlitePool) -> TestResult {
    migrate(pool).await?;
    emit_fixture(pool).await
}
pub async fn emit_fixture(pool: &SqlitePool) -> TestResult {
    raw_sql(include_str!("../fixtures/atomic_expiry.sql"))
        .execute(pool)
        .await?;
    let request = ExpiryRequest {
        team_id: TeamId::from_uuid(uuid(1)),
        message_id: MessageId::from_uuid(uuid(0x80)),
        request_id: RequestId::from_uuid(uuid(0x90)),
        expected_version: ExpectedVersion::new(Version::new(2)?),
        idempotency_key: IdempotencyKeyString::parse("expire")?,
    };
    expire_message(
        &SqliteExpiryStore::new(pool.clone()),
        &Time(100),
        &mut Ids(0x100),
        request,
    )
    .await?;
    for table in ["events", "outbox", "messages"] {
        assert_eq!(
            query_scalar::<_, i64>(&format!("SELECT COUNT(*) FROM {table}"))
                .fetch_one(pool)
                .await?,
            1,
            "{table}"
        );
    }
    assert_eq!(
        query_scalar::<_, i64>("SELECT COUNT(*) FROM event_consumptions")
            .fetch_one(pool)
            .await?,
        0
    );
    Ok(())
}
pub async fn database() -> Result<SqlitePool, Box<dyn Error>> {
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(SqliteConnectOptions::from_str("sqlite::memory:")?.foreign_keys(true))
        .await?;
    seed(&pool).await?;
    Ok(pool)
}
pub async fn rows(pool: &SqlitePool) -> Result<Vec<(String, Vec<String>)>, Box<dyn Error>> {
    let tables:Vec<String>=query_scalar("SELECT name FROM sqlite_master WHERE type='table' AND name NOT LIKE 'sqlite_%' ORDER BY name").fetch_all(pool).await?;
    let mut result = Vec::new();
    for table in tables {
        let columns: Vec<String> = query_scalar("SELECT name FROM pragma_table_info(?)")
            .bind(&table)
            .fetch_all(pool)
            .await?;
        let expression = columns
            .iter()
            .map(|c| format!("quote(\"{}\")", c.replace('"', "\"\"")))
            .collect::<Vec<_>>()
            .join(",");
        let values = query(&format!(
            "SELECT {expression} FROM \"{}\"",
            table.replace('"', "\"\"")
        ))
        .fetch_all(pool)
        .await?;
        let mut values = values
            .iter()
            .map(|r| {
                (0..columns.len())
                    .map(|i| r.try_get::<String, _>(i))
                    .collect::<Result<Vec<_>, _>>()
                    .map(|v| v.join("|"))
            })
            .collect::<Result<Vec<_>, _>>()?;
        values.sort();
        result.push((table, values));
    }
    Ok(result)
}
pub struct Scratch(pub std::path::PathBuf);
impl Scratch {
    pub fn new() -> Result<Self, std::io::Error> {
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let dir = std::env::temp_dir().join(format!(
            "audit-consumer-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        ));
        std::fs::create_dir(&dir)?;
        Ok(Self(dir))
    }
}
impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
pub async fn open_file(path: &std::path::Path) -> Result<SqlitePool, Box<dyn Error>> {
    Ok(SqlitePoolOptions::new()
        .max_connections(2)
        .connect_with(
            SqliteConnectOptions::new()
                .filename(path)
                .create_if_missing(true)
                .foreign_keys(true)
                .journal_mode(SqliteJournalMode::Wal)
                .busy_timeout(std::time::Duration::from_secs(5)),
        )
        .await?)
}
