//! DEV-02-07: production consumer, not handwritten SQL claiming consumption.
#[path = "support/audit_crash.rs"]
mod crash;
#[path = "support/audit_port_faults.rs"]
mod port_faults;
#[path = "support/audit_consumption.rs"]
mod support;
use application::audit_consumer::{AuditDelivery, AuditError, AuditOutcome, consume_audit};
use persistence_sqlite::audit_consumer::SqliteAuditStore;
use sqlx_core::{query::query, query_scalar::query_scalar, raw_sql::raw_sql};
use support::*;
use teamspace_domain::TeamId;

fn delivery() -> AuditDelivery {
    AuditDelivery {
        team_id: TeamId::from_uuid(uuid(1)),
        outbox_id: uuid(0x102),
    }
}

#[tokio::test]
async fn application_refuses_cross_team_port_event_before_querying_or_writing_receipt() -> TestResult
{
    use port_faults::{Fault, FaultyStore};
    let pool = database().await?;
    let before = rows(&pool).await?;
    let store = FaultyStore::new(
        SqliteAuditStore::new(pool.clone()),
        Fault::CrossTeam(TeamId::from_uuid(uuid(2))),
    );
    let error = consume_audit(&store, &Time(200), delivery())
        .await
        .unwrap_err();
    assert_eq!(error, AuditError::IntegrityFailed);
    assert_eq!(error.to_string(), "STORAGE_INTEGRITY_FAILED");
    assert_eq!(store.calls(), ["begin", "load_event", "rollback"]);
    assert_eq!(rows(&pool).await?, before);
    let normal = FaultyStore::new(SqliteAuditStore::new(pool.clone()), Fault::None);
    let outcome = consume_audit(&normal, &Time(200), delivery()).await?;
    assert!(matches!(outcome, AuditOutcome::Applied(_)));
    assert_eq!(
        normal.calls(),
        [
            "begin",
            "load_event",
            "state",
            "record_consumption",
            "write_projection",
            "commit"
        ]
    );
    Ok(())
}

#[tokio::test]
async fn application_refuses_mismatched_port_receipt_without_acknowledging_or_rewriting_it()
-> TestResult {
    use port_faults::{Fault, FaultyStore};
    let pool = database().await?;
    let real = SqliteAuditStore::new(pool.clone());
    let first = consume_audit(&real, &Time(200), delivery()).await?;
    assert!(matches!(first, AuditOutcome::Applied(_)));
    let before = rows(&pool).await?;
    let store = FaultyStore::new(SqliteAuditStore::new(pool.clone()), Fault::ReceiptHash);
    let error = consume_audit(&store, &Time(300), delivery())
        .await
        .unwrap_err();
    assert_eq!(error, AuditError::IntegrityFailed);
    assert_eq!(error.to_string(), "STORAGE_INTEGRITY_FAILED");
    assert_eq!(store.calls(), ["begin", "load_event", "state", "rollback"]);
    assert_eq!(rows(&pool).await?, before);
    let normal = FaultyStore::new(SqliteAuditStore::new(pool.clone()), Fault::None);
    let duplicate = consume_audit(&normal, &Time(400), delivery()).await?;
    assert!(matches!(duplicate, AuditOutcome::Duplicate(_)));
    assert_eq!(normal.calls(), ["begin", "load_event", "state", "commit"]);
    assert_eq!(duplicate.receipt(), first.receipt());
    assert_eq!(duplicate.receipt().consumed_at.unix_millis(), 200);
    assert_eq!(rows(&pool).await?, before);
    Ok(())
}
#[tokio::test]
async fn duplicate_delivery_returns_first_receipt_without_repeating_any_fact() -> TestResult {
    let pool = database().await?;
    let store = SqliteAuditStore::new(pool.clone());
    let before = rows(&pool)
        .await?
        .into_iter()
        .filter(|(t, _)| t != "event_consumptions" && t != "audit_expiry_projection")
        .collect::<Vec<_>>();
    let first = consume_audit(&store, &Time(200), delivery()).await?;
    assert!(matches!(first, AuditOutcome::Applied(_)));
    let second = consume_audit(&store, &Time(300), delivery()).await?;
    assert!(matches!(second, AuditOutcome::Duplicate(_)));
    assert_eq!(first.receipt(), second.receipt());
    assert_eq!(first.receipt().consumed_at.unix_millis(), 200);
    assert_eq!(
        query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM event_consumptions WHERE consumer_id='audit-projection-v1'"
        )
        .fetch_one(&pool)
        .await?,
        1
    );
    assert_eq!(
        query_scalar::<_, i64>("SELECT COUNT(*) FROM audit_expiry_projection")
            .fetch_one(&pool)
            .await?,
        1
    );
    assert_eq!(
        rows(&pool)
            .await?
            .into_iter()
            .filter(|(t, _)| t != "event_consumptions" && t != "audit_expiry_projection")
            .collect::<Vec<_>>(),
        before
    );
    Ok(())
}

#[tokio::test]
async fn every_consumer_write_failure_rolls_back_without_reverting_source_facts() -> TestResult {
    for (name, on) in [
        ("marker", "BEFORE INSERT ON event_consumptions"),
        ("projection", "BEFORE INSERT ON audit_expiry_projection"),
        (
            "after_projection",
            "AFTER INSERT ON audit_expiry_projection",
        ),
    ] {
        let pool = database().await?;
        let before = rows(&pool).await?;
        raw_sql(&format!("CREATE TRIGGER inject_failure {on} BEGIN SELECT RAISE(ABORT,'synthetic private diagnostic'); END")).execute(&pool).await?;
        let store = SqliteAuditStore::new(pool.clone());
        let error = consume_audit(&store, &Time(200), delivery())
            .await
            .unwrap_err();
        assert_eq!(error, AuditError::StorageUnavailable, "{name}");
        assert_eq!(error.to_string(), "STORAGE_UNAVAILABLE");
        assert_eq!(rows(&pool).await?, before, "{name}");
        query("DROP TRIGGER inject_failure").execute(&pool).await?;
        assert!(matches!(
            consume_audit(&store, &Time(200), delivery()).await?,
            AuditOutcome::Applied(_)
        ));
    }
    Ok(())
}
#[tokio::test]
async fn commit_constraint_failure_rolls_back_receipt_and_projection() -> TestResult {
    let pool = database().await?;
    raw_sql("CREATE TABLE commit_probe(team_id BLOB REFERENCES teams(id) DEFERRABLE INITIALLY DEFERRED); CREATE TRIGGER inject_commit AFTER INSERT ON audit_expiry_projection BEGIN INSERT INTO commit_probe(team_id) VALUES(X'01921389d31174628f80cb5f79ab00ff'); END").execute(&pool).await?;
    let before = rows(&pool).await?;
    let store = SqliteAuditStore::new(pool.clone());
    assert_eq!(
        consume_audit(&store, &Time(200), delivery())
            .await
            .unwrap_err(),
        AuditError::StorageUnavailable
    );
    assert_eq!(rows(&pool).await?, before);
    query("DROP TRIGGER inject_commit").execute(&pool).await?;
    consume_audit(&store, &Time(200), delivery()).await?;
    Ok(())
}
#[tokio::test]
async fn silently_ignored_inserts_never_report_applied_or_persist_a_partial_receipt() -> TestResult
{
    for table in ["event_consumptions", "audit_expiry_projection"] {
        let pool = database().await?;
        raw_sql(&format!(
            "CREATE TRIGGER inject_ignore BEFORE INSERT ON {table} BEGIN SELECT RAISE(IGNORE); END"
        ))
        .execute(&pool)
        .await?;
        let before = rows(&pool).await?;
        assert_eq!(
            consume_audit(&SqliteAuditStore::new(pool.clone()), &Time(200), delivery())
                .await
                .unwrap_err(),
            AuditError::IntegrityFailed,
            "{table}"
        );
        assert_eq!(rows(&pool).await?, before);
    }
    Ok(())
}
#[tokio::test]
async fn lost_projection_rebuilds_with_first_time_without_resetting_dedup() -> TestResult {
    let pool = database().await?;
    let store = SqliteAuditStore::new(pool.clone());
    let first = consume_audit(&store, &Time(200), delivery()).await?;
    assert_eq!(
        query("DELETE FROM audit_expiry_projection")
            .execute(&pool)
            .await?
            .rows_affected(),
        1
    );
    let before = rows(&pool).await?;
    let error = query("DELETE FROM event_consumptions WHERE consumer_id='audit-projection-v1'")
        .execute(&pool)
        .await
        .unwrap_err();
    assert!(
        error
            .to_string()
            .contains("audit first consumption is retained")
    );
    assert_eq!(rows(&pool).await?, before);
    let rebuilt = consume_audit(&store, &Time(900), delivery()).await?;
    assert!(matches!(rebuilt, AuditOutcome::Rebuilt(_)));
    assert_eq!(rebuilt.receipt(), first.receipt());
    assert_eq!(
        query_scalar::<_, i64>(
            "SELECT consumed_at FROM event_consumptions WHERE consumer_id='audit-projection-v1'"
        )
        .fetch_one(&pool)
        .await?,
        200
    );
    assert!(matches!(
        consume_audit(&store, &Time(999), delivery()).await?,
        AuditOutcome::Duplicate(_)
    ));
    Ok(())
}
#[tokio::test]
async fn other_consumers_are_not_used_as_this_consumer_receipt() -> TestResult {
    let pool = database().await?;
    query("INSERT INTO event_consumptions(consumer_id,event_id,team_id,consumed_at) VALUES('different-consumer',?,?,11)").bind(uuid(0x101).database_bytes().to_vec()).bind(uuid(1).database_bytes().to_vec()).execute(&pool).await?;
    assert!(matches!(
        consume_audit(&SqliteAuditStore::new(pool.clone()), &Time(200), delivery()).await?,
        AuditOutcome::Applied(_)
    ));
    assert_eq!(
        query_scalar::<_, i64>("SELECT COUNT(*) FROM event_consumptions")
            .fetch_one(&pool)
            .await?,
        2
    );
    assert_eq!(
        query_scalar::<_, i64>(
            "SELECT consumed_at FROM event_consumptions WHERE consumer_id='different-consumer'"
        )
        .fetch_one(&pool)
        .await?,
        11
    );
    Ok(())
}
#[tokio::test]
async fn source_scope_destination_and_reference_are_revalidated_before_replay() -> TestResult {
    for case in ["team", "missing", "destination", "payload"] {
        let pool = database().await?;
        let store = SqliteAuditStore::new(pool.clone());
        consume_audit(&store, &Time(200), delivery()).await?;
        let mut incoming = delivery();
        let expected = match case {
            "team" => {
                incoming.team_id = TeamId::from_uuid(uuid(2));
                AuditError::ObjectNotFound
            }
            "missing" => {
                incoming.outbox_id = uuid(0x199);
                AuditError::ObjectNotFound
            }
            "destination" => {
                query("INSERT INTO outbox(id,team_id,event_id,destination,payload,status,created_at,scheduled_at) SELECT ?,team_id,event_id,'notification',payload,'PENDING',created_at,scheduled_at FROM outbox WHERE id=?").bind(uuid(0x199).database_bytes().to_vec()).bind(uuid(0x102).database_bytes().to_vec()).execute(&pool).await?;
                incoming.outbox_id = uuid(0x199);
                AuditError::UnsupportedEvent
            }
            "payload" => {
                query("UPDATE outbox SET payload='{\"event_id\":\"SYNTHETIC_PRIVATE_CONTENT\"}' WHERE id=?").bind(uuid(0x102).database_bytes().to_vec()).execute(&pool).await?;
                AuditError::IntegrityFailed
            }
            _ => unreachable!(),
        };
        let before = rows(&pool).await?;
        assert_eq!(
            consume_audit(&store, &Time(300), incoming)
                .await
                .unwrap_err(),
            expected,
            "{case}"
        );
        assert_eq!(rows(&pool).await?, before);
    }
    Ok(())
}
// A deliberately malformed new source, accepted by SQLite's structural guards,
// tests the runtime verifier. It never replaces a retained Event.
async fn malformed_source(pool: &sqlx_sqlite::SqlitePool, case: &str) -> TestResult {
    let (kind, actor, payload, metadata, previous, hash) = match case {
        "format" => (
            "MESSAGE_EXPIRED",
            "SYSTEM",
            "payload",
            "'{}'",
            "hash",
            "hash",
        ),
        "type" => (
            "MESSAGE_QUEUED",
            "SYSTEM",
            "payload",
            "metadata",
            "hash",
            "hash",
        ),
        "actor" => (
            "MESSAGE_EXPIRED",
            "HUMAN",
            "payload",
            "metadata",
            "hash",
            "hash",
        ),
        "metadata" => (
            "MESSAGE_EXPIRED",
            "SYSTEM",
            "payload",
            "json_set(metadata,'$.private','SYNTHETIC_PRIVATE_CONTENT')",
            "hash",
            "hash",
        ),
        "payload" => (
            "MESSAGE_EXPIRED",
            "SYSTEM",
            "json_set(payload,'$.private','SYNTHETIC_PRIVATE_CONTENT')",
            "metadata",
            "hash",
            "hash",
        ),
        "predecessor" => (
            "MESSAGE_EXPIRED",
            "SYSTEM",
            "payload",
            "metadata",
            "NULL",
            "hash",
        ),
        "hash" => (
            "MESSAGE_EXPIRED",
            "SYSTEM",
            "payload",
            "metadata",
            "hash",
            "'BBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBB'",
        ),
        _ => unreachable!(),
    };
    query(&format!("INSERT INTO events(id,team_id,event_type,actor_type,subject_type,subject_id,request_id,object_version,payload,metadata,occurred_at,previous_hash,hash) SELECT ?,team_id,'{kind}','{actor}',subject_type,subject_id,request_id,object_version,{payload},{metadata},occurred_at,{previous},{hash} FROM events WHERE id=?")).bind(uuid(0x301).database_bytes().to_vec()).bind(uuid(0x101).database_bytes().to_vec()).execute(pool).await?;
    query("INSERT INTO outbox(id,team_id,event_id,destination,payload,status,created_at,scheduled_at) VALUES(?, ?,?,'audit',?,'PENDING',100,100)").bind(uuid(0x302).database_bytes().to_vec()).bind(uuid(1).database_bytes().to_vec()).bind(uuid(0x301).database_bytes().to_vec()).bind(format!("{{\"event_id\":\"{}\"}}",uuid(0x301))).execute(pool).await?;
    assert_eq!(
        query_scalar::<_, i64>("SELECT COUNT(*) FROM events")
            .fetch_one(pool)
            .await?,
        2
    );
    assert_eq!(
        query_scalar::<_, i64>("SELECT COUNT(*) FROM outbox")
            .fetch_one(pool)
            .await?,
        2
    );
    Ok(())
}
#[tokio::test]
async fn unsupported_history_and_corrupt_known_format_remain_unconsumed() -> TestResult {
    for case in [
        "format",
        "type",
        "actor",
        "metadata",
        "payload",
        "predecessor",
        "hash",
    ] {
        let pool = database().await?;
        malformed_source(&pool, case).await?;
        let before = rows(&pool).await?;
        let expected = if matches!(case, "format" | "type") {
            AuditError::UnsupportedEvent
        } else {
            AuditError::IntegrityFailed
        };
        let error = consume_audit(
            &SqliteAuditStore::new(pool.clone()),
            &Time(200),
            AuditDelivery {
                team_id: delivery().team_id,
                outbox_id: uuid(0x302),
            },
        )
        .await
        .unwrap_err();
        assert_eq!(error, expected, "{case}");
        assert!(!error.to_string().contains("SYNTHETIC_PRIVATE_CONTENT"));
        assert_eq!(rows(&pool).await?, before);
    }
    Ok(())
}
#[tokio::test]
async fn two_wal_connections_project_once_and_return_the_same_receipt() -> TestResult {
    let scratch = Scratch::new()?;
    let pool = open_file(&scratch.0.join("test.sqlite")).await?;
    seed(&pool).await?;
    let (a, b) = tokio::join!(pool.acquire(), pool.acquire());
    let a = a?;
    let b = b?;
    assert_ne!((&*a) as *const _, (&*b) as *const _);
    drop((a, b));
    let store = SqliteAuditStore::new(pool.clone());
    let (a, b) = tokio::join!(
        consume_audit(&store, &Time(200), delivery()),
        consume_audit(&store, &Time(300), delivery())
    );
    let a = a?;
    let b = b?;
    assert_eq!(a.receipt(), b.receipt());
    assert_eq!(
        [a, b]
            .iter()
            .filter(|o| matches!(o, AuditOutcome::Applied(_)))
            .count(),
        1
    );
    assert_eq!(
        [a, b]
            .iter()
            .filter(|o| matches!(o, AuditOutcome::Duplicate(_)))
            .count(),
        1
    );
    assert_eq!(
        query_scalar::<_, i64>("SELECT COUNT(*) FROM audit_expiry_projection")
            .fetch_one(&pool)
            .await?,
        1
    );
    pool.close().await;
    Ok(())
}
#[tokio::test]
async fn reverse_delivery_order_does_not_require_a_cursor_or_duplicate_old_effects() -> TestResult {
    use application::message_expiry::{ExpiryRequest, expire_message};
    use persistence_sqlite::message_expiry::SqliteExpiryStore;
    use teamspace_domain::{
        ExpectedVersion, MessageId, RequestId, Version, idempotency::IdempotencyKeyString,
    };
    let pool = database().await?;
    query("INSERT INTO messages(id,team_id,thread_id,sender_actor_type,to_role_id,message_type,body,status,version,expires_at,created_at,updated_at) SELECT ?,team_id,thread_id,sender_actor_type,to_role_id,message_type,body,'DRAFT',1,100,1,1 FROM messages WHERE id=?").bind(uuid(0x81).database_bytes().to_vec()).bind(uuid(0x80).database_bytes().to_vec()).execute(&pool).await?;
    query("UPDATE messages SET status='QUEUED',version=2,updated_at=2 WHERE id=?")
        .bind(uuid(0x81).database_bytes().to_vec())
        .execute(&pool)
        .await?;
    expire_message(
        &SqliteExpiryStore::new(pool.clone()),
        &Time(100),
        &mut Ids(0x200),
        ExpiryRequest {
            team_id: delivery().team_id,
            message_id: MessageId::from_uuid(uuid(0x81)),
            request_id: RequestId::from_uuid(uuid(0x91)),
            expected_version: ExpectedVersion::new(Version::new(2)?),
            idempotency_key: IdempotencyKeyString::parse("second")?,
        },
    )
    .await?;
    let store = SqliteAuditStore::new(pool.clone());
    let newer = AuditDelivery {
        outbox_id: uuid(0x202),
        ..delivery()
    };
    consume_audit(&store, &Time(200), newer).await?;
    consume_audit(&store, &Time(201), delivery()).await?;
    assert!(matches!(
        consume_audit(&store, &Time(300), newer).await?,
        AuditOutcome::Duplicate(_)
    ));
    assert_eq!(
        query_scalar::<_, i64>("SELECT COUNT(*) FROM audit_expiry_projection")
            .fetch_one(&pool)
            .await?,
        2
    );
    assert_eq!(
        query_scalar::<_, i64>("SELECT COUNT(*) FROM outbox WHERE status='PENDING'")
            .fetch_one(&pool)
            .await?,
        2
    );
    Ok(())
}

#[tokio::test]
async fn failed_rebuild_keeps_the_first_receipt_and_can_retry() -> TestResult {
    let pool = database().await?;
    let store = SqliteAuditStore::new(pool.clone());
    let first = consume_audit(&store, &Time(200), delivery()).await?;
    query("DELETE FROM audit_expiry_projection")
        .execute(&pool)
        .await?;
    raw_sql("CREATE TRIGGER fail_rebuild BEFORE INSERT ON audit_expiry_projection BEGIN SELECT RAISE(ABORT,'synthetic'); END").execute(&pool).await?;
    let before = rows(&pool).await?;
    assert_eq!(
        consume_audit(&store, &Time(900), delivery())
            .await
            .unwrap_err(),
        AuditError::StorageUnavailable
    );
    assert_eq!(rows(&pool).await?, before);
    query("DROP TRIGGER fail_rebuild").execute(&pool).await?;
    assert_eq!(
        consume_audit(&store, &Time(999), delivery())
            .await?
            .receipt(),
        first.receipt()
    );
    Ok(())
}
#[tokio::test]
async fn a_corrupt_derived_receipt_is_not_replayed_as_a_first_success() -> TestResult {
    let pool = database().await?;
    let store = SqliteAuditStore::new(pool.clone());
    consume_audit(&store, &Time(200), delivery()).await?;
    // Test-owned DB only: bypass the derived-table guard to verify the reader's
    // independent receipt/ledger comparison. Core Event and marker remain intact.
    query("DROP TRIGGER audit_expiry_projection_no_update")
        .execute(&pool)
        .await?;
    assert_eq!(
        query("UPDATE audit_expiry_projection SET consumed_at=900")
            .execute(&pool)
            .await?
            .rows_affected(),
        1
    );
    let before = rows(&pool).await?;
    assert_eq!(
        consume_audit(&store, &Time(999), delivery())
            .await
            .unwrap_err(),
        AuditError::IntegrityFailed
    );
    assert_eq!(rows(&pool).await?, before);
    Ok(())
}
#[tokio::test]
async fn projection_source_and_first_receipt_guards_reject_mutation_and_replacement() -> TestResult
{
    let pool = database().await?;
    consume_audit(&SqliteAuditStore::new(pool.clone()), &Time(200), delivery()).await?;
    for sql in [
        "UPDATE audit_expiry_projection SET consumed_at=900",
        "UPDATE audit_expiry_projection SET message_id=X'01921389d31174628f80cb5f79ab0081'",
        "INSERT OR REPLACE INTO audit_expiry_projection(event_id,consumer_id,team_id,message_id,request_id,object_version,occurred_at,event_hash,consumed_at) SELECT event_id,consumer_id,team_id,message_id,request_id,object_version,occurred_at,event_hash,consumed_at FROM audit_expiry_projection",
    ] {
        let before = rows(&pool).await?;
        assert!(query(sql).execute(&pool).await.is_err());
        assert_eq!(rows(&pool).await?, before);
    }
    // DELETE only the reconstructible read model, then try a source mismatch.
    query("DELETE FROM audit_expiry_projection")
        .execute(&pool)
        .await?;
    let before = rows(&pool).await?;
    let error=query("INSERT INTO audit_expiry_projection(event_id,consumer_id,team_id,message_id,request_id,object_version,occurred_at,event_hash,consumed_at) SELECT e.id,'audit-projection-v1',e.team_id,e.subject_id,e.request_id,e.object_version,e.occurred_at,e.hash,900 FROM events e").execute(&pool).await.unwrap_err();
    assert!(
        error
            .to_string()
            .contains("must agree with source and first receipt")
    );
    assert_eq!(rows(&pool).await?, before);
    Ok(())
}
#[tokio::test]
async fn forward_0032_preserves_all_old_rows_and_ledger_then_reruns_inertly() -> TestResult {
    use sqlx_core::migrate::Migrator;
    use sqlx_sqlite::{SqliteConnectOptions, SqlitePoolOptions};
    use std::str::FromStr;
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(SqliteConnectOptions::from_str("sqlite::memory:")?.foreign_keys(true))
        .await?;
    let mut old =
        Migrator::new(std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("migrations"))
            .await?;
    old.migrations.to_mut().retain(|m| m.version <= 31);
    old.run(&pool).await?;
    // Use the real writer on 0031; the projection schema is still absent.
    assert_eq!(
        query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM sqlite_master WHERE name='audit_expiry_projection'"
        )
        .fetch_one(&pool)
        .await?,
        0
    );
    emit_fixture(&pool).await?;
    query("INSERT INTO event_consumptions(consumer_id,event_id,team_id,consumed_at) VALUES('legacy',?,?,3)").bind(uuid(0x101).database_bytes().to_vec()).bind(uuid(1).database_bytes().to_vec()).execute(&pool).await?;
    let before = rows(&pool).await?;
    // This test owns 0031 -> 0032. Later upgrades have their own full-row tests.
    let mut current =
        Migrator::new(std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("migrations"))
            .await?;
    current.migrations.to_mut().retain(|m| m.version <= 32);
    current.run(&pool).await?;
    let after = rows(&pool).await?;
    for (name, values) in before {
        let actual = &after.iter().find(|(n, _)| n == &name).unwrap().1;
        if name == "_sqlx_migrations" {
            assert!(values.iter().all(|v| actual.contains(v)));
            assert_eq!(actual.len(), values.len() + 1);
        } else {
            assert_eq!(*actual, values, "{name}");
        }
    }
    assert_eq!(
        query_scalar::<_, i64>("SELECT COUNT(*) FROM audit_expiry_projection")
            .fetch_one(&pool)
            .await?,
        0
    );
    assert!(matches!(
        old.run(&pool).await.unwrap_err(),
        sqlx_core::migrate::MigrateError::VersionMissing(32)
    ));
    let after = rows(&pool).await?;
    current.run(&pool).await?;
    assert_eq!(rows(&pool).await?, after);
    Ok(())
}

#[tokio::test]
async fn failed_0032_upgrade_rolls_back_early_and_late_schema_changes() -> TestResult {
    use sqlx_core::migrate::{Migration, MigrationType, Migrator};
    use sqlx_sqlite::{SqliteConnectOptions, SqlitePoolOptions};
    use std::{borrow::Cow, str::FromStr};
    for marker in [
        "CREATE TRIGGER audit_expiry_projection_source_insert",
        "CREATE INDEX audit_expiry_projection_team_time",
    ] {
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect_with(SqliteConnectOptions::from_str("sqlite::memory:")?.foreign_keys(true))
            .await?;
        let mut loader =
            Migrator::new(std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("migrations"))
                .await?;
        let migration = loader
            .migrations
            .iter()
            .find(|m| m.version == 32)
            .unwrap()
            .clone();
        loader.migrations.to_mut().retain(|m| m.version <= 31);
        loader.run(&pool).await?;
        emit_fixture(&pool).await?;
        let before = rows(&pool).await?;
        let schema: Vec<String> = query_scalar(
            "SELECT type||'|'||name||'|'||coalesce(sql,'') FROM sqlite_master ORDER BY type,name",
        )
        .fetch_all(&pool)
        .await?;
        let broken = migration.sql.replacen(
            marker,
            &format!("SELECT * FROM injected_0032_failure;\n{marker}"),
            1,
        );
        assert_ne!(broken, migration.sql);
        loader.migrations.to_mut().push(Migration::new(
            32,
            Cow::Borrowed("audit expiry projection"),
            MigrationType::Simple,
            Cow::Owned(broken),
            false,
        ));
        let error = loader.run(&pool).await.unwrap_err();
        assert!(error.to_string().contains("injected_0032_failure"));
        assert_eq!(rows(&pool).await?, before);
        assert_eq!(query_scalar::<_,String>("SELECT type||'|'||name||'|'||coalesce(sql,'') FROM sqlite_master ORDER BY type,name").fetch_all(&pool).await?,schema);
        persistence_sqlite::migrate(&pool).await?;
        assert!(matches!(
            consume_audit(&SqliteAuditStore::new(pool.clone()), &Time(200), delivery()).await?,
            AuditOutcome::Applied(_)
        ));
    }
    Ok(())
}

#[tokio::test]
async fn oversized_json_is_not_materialized_or_marked_consumed() -> TestResult {
    let pool = database().await?;
    let payload = format!("{}{{\"event_id\":\"{}\"}}", " ".repeat(10_000), uuid(0x101));
    query("UPDATE outbox SET payload=?")
        .bind(payload)
        .execute(&pool)
        .await?;
    let before = rows(&pool).await?;
    assert_eq!(
        consume_audit(&SqliteAuditStore::new(pool.clone()), &Time(200), delivery())
            .await
            .unwrap_err(),
        AuditError::IntegrityFailed
    );
    assert_eq!(rows(&pool).await?, before);
    Ok(())
}

// Fixture entry point. In the normal suite it performs no assertions; the
// following parent test invokes it in separate OS processes for three phases.
#[tokio::test]
async fn crash_child_helper() -> TestResult {
    let Some(db) = std::env::var_os("TEAMSPACE_AUDIT_CRASH_DB") else {
        return Ok(());
    };
    let pool = open_file(std::path::Path::new(&db)).await?;
    let phase = std::env::var("TEAMSPACE_AUDIT_CRASH_PHASE")?;
    let ready = std::path::PathBuf::from(
        std::env::var_os("TEAMSPACE_AUDIT_CRASH_READY").ok_or("missing readiness path")?,
    );
    consume_audit(
        &crash::PausedStore { pool, phase, ready },
        &Time(200),
        delivery(),
    )
    .await?;
    Err("child should remain paused until the parent kills it".into())
}
struct ChildGuard(std::process::Child);
impl Drop for ChildGuard {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}
#[tokio::test]
async fn actual_process_crashes_before_commit_and_after_commit_replay_correctly() -> TestResult {
    for phase in ["marker", "projection", "commit"] {
        let scratch = Scratch::new()?;
        let db = scratch.0.join("test.sqlite");
        let pool = open_file(&db).await?;
        seed(&pool).await?;
        let before = rows(&pool).await?;
        pool.close().await;
        let ready = scratch.0.join("ready.txt");
        let log = std::fs::File::create(scratch.0.join("child.log"))?;
        let mut child = ChildGuard(
            std::process::Command::new(std::env::current_exe()?)
                .args(["crash_child_helper", "--exact", "--nocapture"])
                .env("TEAMSPACE_AUDIT_CRASH_DB", &db)
                .env("TEAMSPACE_AUDIT_CRASH_PHASE", phase)
                .env("TEAMSPACE_AUDIT_CRASH_READY", &ready)
                .stdout(std::process::Stdio::from(log.try_clone()?))
                .stderr(std::process::Stdio::from(log))
                .spawn()?,
        );
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(15);
        loop {
            if ready.exists() {
                break;
            }
            if let Some(status) = child.0.try_wait()? {
                return Err(format!(
                    "child exited before {phase} readiness: {status}; {}",
                    std::fs::read_to_string(scratch.0.join("child.log"))?
                )
                .into());
            }
            if std::time::Instant::now() > deadline {
                return Err(format!("no real {phase} readiness").into());
            }
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
        let witness = std::fs::read_to_string(&ready)?;
        assert_eq!(
            witness,
            if phase == "marker" {
                "marker=1;projection=0;consumed_at=200"
            } else {
                "marker=1;projection=1;consumed_at=200"
            }
        );
        assert!(child.0.try_wait()?.is_none(), "child must still be live");
        child.0.kill()?;
        let exit = child.0.wait()?;
        assert!(!exit.success());
        let pool = open_file(&db).await?;
        if phase != "commit" {
            assert_eq!(rows(&pool).await?, before, "{phase} rollback");
        }
        let result =
            consume_audit(&SqliteAuditStore::new(pool.clone()), &Time(999), delivery()).await?;
        if phase == "commit" {
            assert!(matches!(result, AuditOutcome::Duplicate(_)));
            assert_eq!(result.receipt().consumed_at.unix_millis(), 200);
        } else {
            assert!(matches!(result, AuditOutcome::Applied(_)));
            assert_eq!(result.receipt().consumed_at.unix_millis(), 999);
        }
        assert_eq!(
            query_scalar::<_, i64>("SELECT COUNT(*) FROM audit_expiry_projection")
                .fetch_one(&pool)
                .await?,
            1
        );
        assert_eq!(
            query_scalar::<_, i64>("SELECT COUNT(*) FROM outbox WHERE status='PENDING'")
                .fetch_one(&pool)
                .await?,
            1
        );
        let core = rows(&pool)
            .await?
            .into_iter()
            .filter(|(t, _)| t != "event_consumptions" && t != "audit_expiry_projection")
            .collect::<Vec<_>>();
        assert_eq!(
            core,
            before
                .into_iter()
                .filter(|(t, _)| t != "event_consumptions" && t != "audit_expiry_projection")
                .collect::<Vec<_>>()
        );
        eprintln!("crash {phase}: verified {witness}; killed live process; replay {result:?}");
        pool.close().await;
    }
    Ok(())
}
