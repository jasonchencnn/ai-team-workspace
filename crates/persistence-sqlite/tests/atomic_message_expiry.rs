//! DEV-02-07: exercise the production Application command against real SQLite.
use application::message_expiry::{ExpiryError, ExpiryRequest, expire_message};
use persistence_sqlite::{message_expiry::SqliteExpiryStore, migrate};
use sqlx_core::{
    query::query, query_scalar::query_scalar, raw_sql::raw_sql, row::Row, value::ValueRef,
};
use sqlx_sqlite::{SqliteConnectOptions, SqlitePool, SqlitePoolOptions};
use std::{error::Error, str::FromStr};
use teamspace_domain::{
    Clock, ExpectedVersion, MessageId, RequestId, TeamId, UtcTimestamp, UuidV7, UuidV7Source,
    Version, idempotency::IdempotencyKeyString,
};

type TestResult = Result<(), Box<dyn Error>>;
fn uuid(tag: u16) -> UuidV7 {
    UuidV7::parse_protocol(&format!("01921389-d311-7462-8f80-cb5f79ab{tag:04x}")).unwrap()
}
struct Time(i64);
impl Clock for Time {
    fn now(&self) -> UtcTimestamp {
        UtcTimestamp::from_unix_millis(self.0).unwrap()
    }
}
struct Ids(u16);
impl UuidV7Source for Ids {
    type Error = std::convert::Infallible;
    fn next_uuid_v7(&mut self) -> Result<UuidV7, Self::Error> {
        self.0 += 1;
        Ok(uuid(self.0))
    }
}
fn request(key: &str, version: u64) -> ExpiryRequest {
    ExpiryRequest {
        team_id: TeamId::from_uuid(uuid(1)),
        message_id: MessageId::from_uuid(uuid(0x80)),
        request_id: RequestId::from_uuid(uuid(0x90)),
        expected_version: ExpectedVersion::new(Version::new(version).unwrap()),
        idempotency_key: IdempotencyKeyString::parse(key).unwrap(),
    }
}
async fn seed(pool: &SqlitePool) -> TestResult {
    migrate(pool).await?;
    raw_sql(include_str!("fixtures/atomic_expiry.sql"))
        .execute(pool)
        .await?;
    assert_eq!(
        query_scalar::<_, i64>("SELECT COUNT(*) FROM messages")
            .fetch_one(pool)
            .await?,
        1
    );
    Ok(())
}
async fn database() -> Result<SqlitePool, Box<dyn Error>> {
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(SqliteConnectOptions::from_str("sqlite::memory:")?.foreign_keys(true))
        .await?;
    seed(&pool).await?;
    Ok(pool)
}

#[tokio::test]
async fn missing_team_expiry_returns_not_found_without_claim_or_generated_ids() -> TestResult {
    let pool = database().await?;
    let before = all_rows(&pool).await?;
    let mut req = request("missing-team", 2);
    req.team_id = TeamId::from_uuid(uuid(2));
    assert_eq!(
        query_scalar::<_, i64>("SELECT COUNT(*) FROM teams WHERE id=?")
            .bind(req.team_id.database_bytes().to_vec())
            .fetch_one(&pool)
            .await?,
        0
    );
    let store = SqliteExpiryStore::new(pool.clone());
    let mut ids = Ids(0x100);
    let error = expire_message(&store, &Time(100), &mut ids, req)
        .await
        .unwrap_err();
    assert_eq!(error, ExpiryError::ObjectNotFound);
    assert_eq!(error.to_string(), "DOMAIN_OBJECT_NOT_FOUND");
    assert_eq!(ids.0, 0x100);
    assert_eq!(all_rows(&pool).await?, before);
    let result = expire_message(&store, &Time(100), &mut ids, request("present-team", 2)).await?;
    assert_eq!(result.version.get(), 3);
    assert_eq!(ids.0, 0x102);
    Ok(())
}

#[tokio::test]
async fn sqlite_version_ceiling_refuses_overflow_before_ids_and_rolls_back_claim() -> TestResult {
    // Persisted boundary snapshots are synthetic test data. All production
    // migrations, foreign keys and triggers remain enabled throughout.
    for current in [i64::MAX, i64::MAX - 1] {
        let pool = database().await?;
        assert_eq!(
            query("UPDATE messages SET version=? WHERE id=?")
                .bind(current)
                .bind(uuid(0x80).database_bytes().to_vec())
                .execute(&pool)
                .await?
                .rows_affected(),
            1
        );
        assert_eq!(
            query_scalar::<_, i64>("SELECT version FROM messages WHERE id=?")
                .bind(uuid(0x80).database_bytes().to_vec())
                .fetch_one(&pool)
                .await?,
            current
        );
        let before = all_rows(&pool).await?;
        let store = SqliteExpiryStore::new(pool.clone());
        let mut ids = Ids(0x100);
        let result = expire_message(
            &store,
            &Time(100),
            &mut ids,
            request("ceiling", current as u64),
        )
        .await;
        if current == i64::MAX {
            let error = result.unwrap_err();
            assert_eq!(error, ExpiryError::IntegrityFailed);
            assert_eq!(error.to_string(), "STORAGE_INTEGRITY_FAILED");
            assert_eq!(
                ids.0, 0x100,
                "Application must reject before infrastructure conversion"
            );
            assert_eq!(
                all_rows(&pool).await?,
                before,
                "including idempotency claim"
            );
        } else {
            assert_eq!(result?.version.get(), i64::MAX as u64);
            assert_eq!(ids.0, 0x102);
            assert_eq!(
                query_scalar::<_, i64>("SELECT COUNT(*) FROM events")
                    .fetch_one(&pool)
                    .await?,
                1
            );
            assert_eq!(
                query_scalar::<_, i64>("SELECT COUNT(*) FROM outbox")
                    .fetch_one(&pool)
                    .await?,
                1
            );
        }
    }
    Ok(())
}
#[tokio::test]
async fn expires_at_equality_and_replays_first_result_without_repeating_effects() -> TestResult {
    let pool = database().await?;
    let store = SqliteExpiryStore::new(pool.clone());
    let first = expire_message(&store, &Time(100), &mut Ids(0x100), request("equal", 2)).await?;
    let mut retry = request("equal", 2);
    retry.request_id = RequestId::from_uuid(uuid(0x99));
    assert_eq!(
        expire_message(&store, &Time(200), &mut Ids(0x200), retry).await?,
        first
    );
    let row = query("SELECT status,version,body,updated_at FROM messages")
        .fetch_one(&pool)
        .await?;
    assert_eq!(row.try_get::<String, _>("status")?, "EXPIRED");
    assert_eq!(row.try_get::<i64, _>("version")?, 3);
    assert_eq!(row.try_get::<String, _>("body")?, "SYNTHETIC_PRIVATE_BODY");
    assert_eq!(row.try_get::<i64, _>("updated_at")?, 100);
    assert_eq!(
        query_scalar::<_, i64>("SELECT COUNT(*) FROM events")
            .fetch_one(&pool)
            .await?,
        1
    );
    assert_eq!(
        query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM outbox WHERE max_retries=3 AND status='PENDING'"
        )
        .fetch_one(&pool)
        .await?,
        1
    );
    let audit: String = query_scalar("SELECT payload || metadata FROM events")
        .fetch_one(&pool)
        .await?;
    assert!(!audit.contains("SYNTHETIC_PRIVATE_BODY"));
    assert_eq!(
        expire_message(&store, &Time(200), &mut Ids(0x300), request("equal", 3))
            .await
            .unwrap_err(),
        ExpiryError::IdempotencyConflict
    );
    pool.close().await;
    Ok(())
}

async fn inflight(pool: &SqlitePool) -> TestResult {
    raw_sql(include_str!("fixtures/atomic_expiry_inflight.sql"))
        .execute(pool)
        .await?;
    assert_eq!(
        query_scalar::<_, i64>("SELECT COUNT(*) FROM delivery_attempts WHERE status='IN_FLIGHT'")
            .fetch_one(pool)
            .await?,
        1
    );
    assert_eq!(
        query_scalar::<_, i64>("SELECT COUNT(*) FROM delivery_attempts WHERE status='FAILED'")
            .fetch_one(pool)
            .await?,
        1
    );
    Ok(())
}
async fn all_rows(pool: &SqlitePool) -> Result<Vec<(String, Vec<String>)>, Box<dyn Error>> {
    let tables: Vec<String> = query_scalar("SELECT name FROM sqlite_master WHERE type='table' AND name NOT LIKE 'sqlite_%' ORDER BY name").fetch_all(pool).await?;
    let mut result = Vec::new();
    for table in tables {
        let columns: Vec<String> = query_scalar("SELECT name FROM pragma_table_info(?)")
            .bind(&table)
            .fetch_all(pool)
            .await?;
        let expressions = columns
            .iter()
            .map(|c| format!("quote(\"{}\")", c.replace('"', "\"\"")))
            .collect::<Vec<_>>()
            .join(",");
        let rows = query(&format!(
            "SELECT {expressions} FROM \"{}\"",
            table.replace('"', "\"\"")
        ))
        .fetch_all(pool)
        .await?;
        let mut values = rows
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
#[tokio::test]
async fn inflight_is_abandoned_and_terminal_history_is_unchanged() -> TestResult {
    let pool = database().await?;
    inflight(&pool).await?;
    let before: String=query_scalar("SELECT hex(id)||'|'||status||'|'||error_code||'|'||error_message||'|'||started_at||'|'||completed_at FROM delivery_attempts WHERE attempt_number=1").fetch_one(&pool).await?;
    expire_message(
        &SqliteExpiryStore::new(pool.clone()),
        &Time(100),
        &mut Ids(0x100),
        request("inflight", 3),
    )
    .await?;
    assert_eq!(query_scalar::<_,String>("SELECT status||'|'||started_at||'|'||completed_at||'|'||error_code FROM delivery_attempts WHERE attempt_number=2").fetch_one(&pool).await?,"ABANDONED|5|100|MESSAGE_TTL_EXPIRED");
    assert_eq!(query_scalar::<_,String>("SELECT hex(id)||'|'||status||'|'||error_code||'|'||error_message||'|'||started_at||'|'||completed_at FROM delivery_attempts WHERE attempt_number=1").fetch_one(&pool).await?,before);
    Ok(())
}
#[tokio::test]
async fn every_write_failure_rolls_back_all_tables_and_same_key_can_retry() -> TestResult {
    let boundaries = [
        ("claim", "BEFORE INSERT ON command_idempotency"),
        ("message", "BEFORE UPDATE ON messages"),
        ("attempt", "BEFORE UPDATE ON delivery_attempts"),
        ("event", "BEFORE INSERT ON events"),
        ("outbox", "BEFORE INSERT ON outbox"),
        ("result", "BEFORE UPDATE ON command_idempotency"),
    ];
    for (name, on) in boundaries {
        let pool = database().await?;
        inflight(&pool).await?;
        let before = all_rows(&pool).await?;
        raw_sql(&format!("CREATE TRIGGER injected_failure {on} BEGIN SELECT RAISE(ABORT,'synthetic failure'); END")).execute(&pool).await?;
        let store = SqliteExpiryStore::new(pool.clone());
        assert_eq!(
            expire_message(&store, &Time(100), &mut Ids(0x100), request(name, 3))
                .await
                .unwrap_err(),
            ExpiryError::StorageUnavailable,
            "{name}"
        );
        assert_eq!(all_rows(&pool).await?, before, "rollback at {name}");
        raw_sql("DROP TRIGGER injected_failure")
            .execute(&pool)
            .await?;
        expire_message(&store, &Time(100), &mut Ids(0x100), request(name, 3)).await?;
    }
    Ok(())
}
#[tokio::test]
async fn stable_failure_keeps_first_code_and_version_after_resource_changes() -> TestResult {
    let pool = database().await?;
    let store = SqliteExpiryStore::new(pool.clone());
    let conflict = ExpiryError::VersionConflict {
        current_version: Version::new(2)?,
    };
    assert_eq!(
        expire_message(&store, &Time(100), &mut Ids(0x100), request("stale", 1))
            .await
            .unwrap_err(),
        conflict
    );
    expire_message(&store, &Time(100), &mut Ids(0x100), request("new", 2)).await?;
    assert_eq!(
        expire_message(&store, &Time(200), &mut Ids(0x200), request("stale", 1))
            .await
            .unwrap_err(),
        conflict
    );
    assert_eq!(
        query_scalar::<_, String>(
            "SELECT first_error_details FROM command_idempotency WHERE idempotency_key='stale'"
        )
        .fetch_one(&pool)
        .await?,
        "{\"current_version\":2}"
    );
    let before = all_rows(&pool).await?;
    assert!(
        query(
            "UPDATE command_idempotency SET first_error_details='{}' WHERE idempotency_key='stale'"
        )
        .execute(&pool)
        .await
        .is_err()
    );
    assert_eq!(all_rows(&pool).await?, before);
    Ok(())
}
#[tokio::test]
async fn not_due_missing_ttl_wrong_team_archived_and_illegal_state_do_not_change_business_rows()
-> TestResult {
    for mode in [
        "not_due",
        "no_ttl",
        "wrong_team",
        "archived",
        "delivered",
        "pending",
    ] {
        let pool = database().await?;
        let mut req = request(mode, 2);
        let expected = match mode {
            "not_due" => ExpiryError::PreconditionsFailed,
            "no_ttl" => {
                query("UPDATE messages SET expires_at=NULL")
                    .execute(&pool)
                    .await?;
                ExpiryError::PreconditionsFailed
            }
            "wrong_team" => {
                query("INSERT INTO teams(id,name,status,version,created_at,updated_at) VALUES(?,'other','ACTIVE',1,1,1)").bind(uuid(2).database_bytes().to_vec()).execute(&pool).await?;
                req.team_id = TeamId::from_uuid(uuid(2));
                ExpiryError::ObjectNotFound
            }
            "archived" => {
                query("UPDATE teams SET status='ARCHIVED',archived_at=9,version=2,updated_at=9")
                    .execute(&pool)
                    .await?;
                ExpiryError::TeamArchived
            }
            "delivered" => {
                inflight(&pool).await?;
                query(
                    "UPDATE messages SET status='DELIVERED',version=4,updated_at=6,delivered_at=6",
                )
                .execute(&pool)
                .await?;
                req.expected_version = ExpectedVersion::new(Version::new(4)?);
                ExpiryError::InvalidTransition
            }
            "pending" => {
                inflight(&pool).await?;
                query("UPDATE delivery_attempts SET status='FAILED',completed_at=6,error_code='MESSAGE_DELIVERY_FAILED' WHERE attempt_number=2").execute(&pool).await?;
                query("INSERT INTO delivery_attempts(id,team_id,message_id,attempt_number,target_role_id,target_binding_id,target_binding_generation,status,created_at) VALUES(?,?,?,?,?,?,1,'PENDING',6)").bind(uuid(0xa2).database_bytes().to_vec()).bind(uuid(1).database_bytes().to_vec()).bind(uuid(0x80).database_bytes().to_vec()).bind(3_i64).bind(uuid(0x11).database_bytes().to_vec()).bind(uuid(0x31).database_bytes().to_vec()).execute(&pool).await?;
                req.expected_version = ExpectedVersion::new(Version::new(3)?);
                ExpiryError::PreconditionsFailed
            }
            _ => unreachable!(),
        };
        let before = all_rows(&pool)
            .await?
            .into_iter()
            .filter(|(t, _)| t != "command_idempotency")
            .collect::<Vec<_>>();
        let store = SqliteExpiryStore::new(pool.clone());
        let time = if mode == "not_due" { 99 } else { 100 };
        assert_eq!(
            expire_message(&store, &Time(time), &mut Ids(0x100), req)
                .await
                .unwrap_err(),
            expected,
            "{mode}"
        );
        assert_eq!(
            all_rows(&pool)
                .await?
                .into_iter()
                .filter(|(t, _)| t != "command_idempotency")
                .collect::<Vec<_>>(),
            before,
            "{mode}"
        );
        assert_eq!(
            expire_message(&store, &Time(200), &mut Ids(0x200), req)
                .await
                .unwrap_err(),
            expected,
            "replay {mode}"
        );
    }
    Ok(())
}

struct Scratch(std::path::PathBuf);
impl Scratch {
    fn new() -> Result<Self, std::io::Error> {
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let dir = std::env::temp_dir().join(format!(
            "atomic-expiry-{}-{}",
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
async fn file_database(scratch: &Scratch) -> Result<SqlitePool, Box<dyn Error>> {
    let options = SqliteConnectOptions::new()
        .filename(scratch.0.join("test.sqlite"))
        .create_if_missing(true)
        .foreign_keys(true)
        .journal_mode(sqlx_sqlite::SqliteJournalMode::Wal)
        .busy_timeout(std::time::Duration::from_secs(5));
    let pool = SqlitePoolOptions::new()
        .max_connections(2)
        .connect_with(options)
        .await?;
    seed(&pool).await?;
    Ok(pool)
}
#[tokio::test]
async fn two_connections_same_key_return_one_first_result() -> TestResult {
    let scratch = Scratch::new()?;
    let pool = file_database(&scratch).await?;
    let connections = tokio::join!(pool.acquire(), pool.acquire());
    drop(connections);
    let store = SqliteExpiryStore::new(pool.clone());
    let mut a = Ids(0x100);
    let mut b = Ids(0x200);
    let (a, b) = tokio::join!(
        expire_message(&store, &Time(100), &mut a, request("shared", 2)),
        expire_message(&store, &Time(100), &mut b, request("shared", 2))
    );
    assert_eq!(a?, b?);
    assert_eq!(
        query_scalar::<_, i64>("SELECT COUNT(*) FROM events")
            .fetch_one(&pool)
            .await?,
        1
    );
    assert_eq!(
        query_scalar::<_, i64>("SELECT COUNT(*) FROM command_idempotency")
            .fetch_one(&pool)
            .await?,
        1
    );
    pool.close().await;
    Ok(())
}
#[tokio::test]
async fn two_connections_same_version_have_one_success_and_one_stable_conflict() -> TestResult {
    let scratch = Scratch::new()?;
    let pool = file_database(&scratch).await?;
    let store = SqliteExpiryStore::new(pool.clone());
    let mut a = Ids(0x100);
    let mut b = Ids(0x200);
    let (a, b) = tokio::join!(
        expire_message(&store, &Time(100), &mut a, request("a", 2)),
        expire_message(&store, &Time(100), &mut b, request("b", 2))
    );
    assert_eq!([&a, &b].iter().filter(|r| r.is_ok()).count(), 1);
    let loser = match (a, b) {
        (Err(error), _) | (_, Err(error)) => error,
        _ => panic!("exactly one request must lose"),
    };
    assert_eq!(
        loser,
        ExpiryError::VersionConflict {
            current_version: Version::new(3)?
        }
    );
    assert_eq!(
        query_scalar::<_, i64>("SELECT COUNT(*) FROM events")
            .fetch_one(&pool)
            .await?,
        1
    );
    assert_eq!(
        query_scalar::<_, i64>("SELECT COUNT(*) FROM outbox")
            .fetch_one(&pool)
            .await?,
        1
    );
    assert_eq!(
        query_scalar::<_, i64>("SELECT COUNT(*) FROM command_idempotency WHERE state='FAILED'")
            .fetch_one(&pool)
            .await?,
        1
    );
    pool.close().await;
    Ok(())
}
#[tokio::test]
async fn commit_then_reopen_preserves_first_result_and_pending_outbox() -> TestResult {
    let scratch = Scratch::new()?;
    let pool = file_database(&scratch).await?;
    let store = SqliteExpiryStore::new(pool.clone());
    let result = expire_message(&store, &Time(100), &mut Ids(0x100), request("restart", 2)).await?;
    drop(store);
    pool.close().await;
    let pool = SqlitePoolOptions::new()
        .connect_with(
            SqliteConnectOptions::new()
                .filename(scratch.0.join("test.sqlite"))
                .foreign_keys(true),
        )
        .await?;
    assert_eq!(
        expire_message(
            &SqliteExpiryStore::new(pool.clone()),
            &Time(300),
            &mut Ids(0x300),
            request("restart", 2)
        )
        .await?,
        result
    );
    assert_eq!(
        query_scalar::<_, i64>("SELECT COUNT(*) FROM outbox WHERE status='PENDING'")
            .fetch_one(&pool)
            .await?,
        1
    );
    pool.close().await;
    Ok(())
}
#[tokio::test]
async fn generated_id_collision_rolls_back_and_does_not_leak_storage_text() -> TestResult {
    let pool = database().await?;
    let store = SqliteExpiryStore::new(pool.clone());
    // A syntactically valid but colliding Outbox ID exercises an actual write failure.
    query("INSERT INTO events(id,team_id,event_type,actor_type,subject_type,subject_id,object_version,occurred_at,hash) VALUES(? ,?,'MESSAGE_QUEUED','SYSTEM','MESSAGE',?,2,2,?)").bind(uuid(0x102).database_bytes().to_vec()).bind(uuid(1).database_bytes().to_vec()).bind(uuid(0x80).database_bytes().to_vec()).bind("A".repeat(64)).execute(&pool).await?;
    query("INSERT INTO outbox(id,team_id,event_id,destination,payload,status,created_at,scheduled_at) VALUES(?,?,?,'audit','{}','PENDING',2,2)").bind(uuid(0x102).database_bytes().to_vec()).bind(uuid(1).database_bytes().to_vec()).bind(uuid(0x102).database_bytes().to_vec()).execute(&pool).await?;
    let before = all_rows(&pool).await?;
    let error = expire_message(&store, &Time(100), &mut Ids(0x100), request("collision", 2))
        .await
        .unwrap_err();
    assert_eq!(error.to_string(), "STORAGE_UNAVAILABLE");
    assert_eq!(all_rows(&pool).await?, before);
    expire_message(&store, &Time(100), &mut Ids(0x200), request("collision", 2)).await?;
    Ok(())
}
#[tokio::test]
async fn forward_details_migration_preserves_legacy_results_and_refuses_rollback_loader()
-> TestResult {
    use sqlx_core::migrate::Migrator;
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(SqliteConnectOptions::from_str("sqlite::memory:")?.foreign_keys(true))
        .await?;
    let mut old =
        Migrator::new(std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("migrations"))
            .await?;
    old.migrations.to_mut().retain(|m| m.version <= 30);
    old.run(&pool).await?;
    raw_sql(include_str!("fixtures/atomic_expiry.sql"))
        .execute(&pool)
        .await?;
    query("INSERT INTO command_idempotency(team_id,actor_fingerprint,command_name,idempotency_key,payload_hash,state,first_error_code,created_at,updated_at) VALUES(?,?,'legacy','legacy',?,'FAILED','DOMAIN_PRECONDITION_FAILED',1,1)").bind(uuid(1).database_bytes().to_vec()).bind(vec![1_u8;32]).bind(vec![2_u8;32]).execute(&pool).await?;
    let before = all_rows(&pool).await?;
    let mut through_0031 =
        Migrator::new(std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("migrations"))
            .await?;
    through_0031.migrations.to_mut().retain(|m| m.version <= 31);
    through_0031.run(&pool).await?;
    let after = all_rows(&pool).await?;
    for (table, rows) in before.into_iter().filter(|(t, _)| t != "_sqlx_migrations") {
        let new = &after.iter().find(|(t, _)| t == &table).unwrap().1;
        let expected = if table == "command_idempotency" {
            rows.iter().map(|r| format!("{r}|NULL")).collect::<Vec<_>>()
        } else {
            rows
        };
        assert_eq!(*new, expected, "{table}");
    }
    assert!(matches!(
        old.run(&pool).await.unwrap_err(),
        sqlx_core::migrate::MigrateError::VersionMissing(31)
    ));
    let snapshot = all_rows(&pool).await?;
    through_0031.run(&pool).await?;
    assert_eq!(all_rows(&pool).await?, snapshot);
    assert!(
        query(
            "UPDATE command_idempotency SET first_error_details='{}' WHERE command_name='legacy'"
        )
        .execute(&pool)
        .await
        .is_err()
    );
    Ok(())
}

#[tokio::test]
async fn audit_chain_uses_insert_order_and_hashes_every_protected_field() -> TestResult {
    use serde_json::{Value, json};
    use sha2::{Digest, Sha256};
    let pool = database().await?;
    // Another legal aggregate; creation and send occur before expiry.
    query("INSERT INTO messages(id,team_id,thread_id,sender_actor_type,to_role_id,message_type,body,status,version,expires_at,created_at,updated_at) SELECT ?,team_id,thread_id,sender_actor_type,to_role_id,message_type,body,'DRAFT',1,100,1,1 FROM messages WHERE id=?").bind(uuid(0x81).database_bytes().to_vec()).bind(uuid(0x80).database_bytes().to_vec()).execute(&pool).await?;
    query("UPDATE messages SET status='QUEUED',version=2,updated_at=2 WHERE id=?")
        .bind(uuid(0x81).database_bytes().to_vec())
        .execute(&pool)
        .await?;
    // PAUSED is not ARCHIVED; the frozen baseline only makes ARCHIVED read-only.
    query("UPDATE teams SET status='PAUSED',version=2,updated_at=9")
        .execute(&pool)
        .await?;
    let store = SqliteExpiryStore::new(pool.clone());
    expire_message(&store, &Time(200), &mut Ids(0x200), request("first", 2)).await?;
    let mut second = request("second", 2);
    second.message_id = MessageId::from_uuid(uuid(0x81));
    expire_message(&store, &Time(100), &mut Ids(0x100), second).await?;
    let rows=query("SELECT id,team_id,event_type,actor_type,actor_id,actor_kind,actor_role_id,actor_binding_id,actor_binding_version,actor_harness_instance_id,subject_type,subject_id,request_id,object_version,payload,metadata,occurred_at,previous_hash,hash FROM events ORDER BY rowid").fetch_all(&pool).await?;
    assert_eq!(rows.len(), 2);
    let mut tail: Option<String> = None;
    for row in rows {
        let protocol = |column: &str| -> Result<String, Box<dyn Error>> {
            let bytes: Vec<u8> = row.try_get(column)?;
            Ok(
                UuidV7::from_database_bytes(bytes.try_into().map_err(|_| "wrong ID size")?)?
                    .to_protocol(),
            )
        };
        let previous: Option<String> = row.try_get("previous_hash")?;
        assert_eq!(previous, tail);
        for column in [
            "actor_id",
            "actor_kind",
            "actor_role_id",
            "actor_binding_id",
            "actor_binding_version",
            "actor_harness_instance_id",
        ] {
            assert!(row.try_get_raw(column)?.is_null(), "{column}");
        }
        let payload: Value = serde_json::from_str(&row.try_get::<String, _>("payload")?)?;
        let metadata: Value = serde_json::from_str(&row.try_get::<String, _>("metadata")?)?;
        let material = json!({"actor_binding_id":null,"actor_binding_version":null,"actor_harness_instance_id":null,"actor_id":null,"actor_kind":null,"actor_role_id":null,"actor_type":row.try_get::<String,_>("actor_type")?,"event_id":protocol("id")?,"event_type":row.try_get::<String,_>("event_type")?,"metadata":metadata,"object_version":row.try_get::<i64,_>("object_version")?,"occurred_at":row.try_get::<i64,_>("occurred_at")?,"payload":payload,"previous_hash":previous,"request_id":protocol("request_id")?,"subject_id":protocol("subject_id")?,"subject_type":row.try_get::<String,_>("subject_type")?,"team_id":protocol("team_id")?});
        let hash: String = row.try_get("hash")?;
        assert_eq!(hash, format!("{:X}", Sha256::digest(material.to_string())));
        for field in material.as_object().unwrap().keys() {
            let mut changed = material.clone();
            changed[field] = json!("SYNTHETIC_TAMPER");
            assert_ne!(
                hash,
                format!("{:X}", Sha256::digest(changed.to_string())),
                "unprotected {field}"
            );
        }
        tail = Some(hash);
    }
    Ok(())
}
#[tokio::test]
async fn id_source_failure_and_invalid_key_leave_all_tables_unchanged() -> TestResult {
    struct FailedIds;
    impl UuidV7Source for FailedIds {
        type Error = ();
        fn next_uuid_v7(&mut self) -> Result<UuidV7, ()> {
            Err(())
        }
    }
    let pool = database().await?;
    let store = SqliteExpiryStore::new(pool.clone());
    let before = all_rows(&pool).await?;
    assert_eq!(
        expire_message(&store, &Time(100), &mut FailedIds, request("ids", 2))
            .await
            .unwrap_err(),
        ExpiryError::StorageUnavailable
    );
    assert_eq!(all_rows(&pool).await?, before);
    assert_eq!(
        expire_message(&store, &Time(100), &mut Ids(0x100), request("   ", 2))
            .await
            .unwrap_err(),
        ExpiryError::InvalidField
    );
    assert_eq!(all_rows(&pool).await?, before);
    expire_message(&store, &Time(100), &mut Ids(0x100), request("ids", 2)).await?;
    Ok(())
}

#[tokio::test]
async fn real_commit_constraint_failure_rolls_back_every_write() -> TestResult {
    let pool = database().await?;
    inflight(&pool).await?;
    raw_sql("CREATE TABLE commit_probe(team_id BLOB REFERENCES teams(id) DEFERRABLE INITIALLY DEFERRED); CREATE TRIGGER fail_at_commit AFTER INSERT ON events BEGIN INSERT INTO commit_probe(team_id) VALUES(X'01921389d31174628f80cb5f79ab00ff'); END;").execute(&pool).await?;
    let before = all_rows(&pool).await?;
    let store = SqliteExpiryStore::new(pool.clone());
    assert_eq!(
        expire_message(&store, &Time(100), &mut Ids(0x100), request("commit", 3))
            .await
            .unwrap_err(),
        ExpiryError::StorageUnavailable
    );
    assert_eq!(all_rows(&pool).await?, before);
    query("DROP TRIGGER fail_at_commit").execute(&pool).await?;
    expire_message(&store, &Time(100), &mut Ids(0x100), request("commit", 3)).await?;
    Ok(())
}
