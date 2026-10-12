//! DEV-02-08 schema tests; SQL transactions here are fixtures, not production
//! authorization or a Messaging service. Actor/TTL/dispatch checks remain at
//! the application boundary. Published migrations and historical AF evidence
//! are read-only.

use std::{error::Error, path::PathBuf, str::FromStr};

use persistence_sqlite::migrate;
use sqlx_core::{
    migrate::{Migration, MigrationType, Migrator},
    query::query,
    query_scalar::query_scalar,
    raw_sql::raw_sql,
    row::Row,
};
use sqlx_sqlite::{SqliteConnectOptions, SqlitePool, SqlitePoolOptions};

type TestResult = Result<(), Box<dyn Error>>;
const SEED: &str = include_str!("fixtures/route_history.sql");
const CONCEPT_BASELINE: &str = include_str!("fixtures/route_concept_baseline.sql");
const FINGERPRINT: &str = "4f574e4552000000000000000000000000000000000000000000000000000000";

fn id(tag: u16) -> String {
    format!("X'01921389d31174628f80cb5f79ab{tag:04x}'")
}

async fn loader() -> Result<Migrator, sqlx_core::migrate::MigrateError> {
    Migrator::new(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("migrations")).await
}

async fn historical_database() -> Result<SqlitePool, Box<dyn Error>> {
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(SqliteConnectOptions::from_str("sqlite::memory:")?.foreign_keys(true))
        .await?;
    let mut legacy = loader().await?;
    legacy.migrations.to_mut().retain(|row| row.version <= 29);
    legacy.run(&pool).await?;
    raw_sql(SEED).execute(&pool).await?;
    assert_eq!(
        query_scalar::<_, i64>("SELECT COUNT(*) FROM delivery_attempts")
            .fetch_one(&pool)
            .await?,
        5,
        "seed must actually contain the retained attempts"
    );
    Ok(pool)
}

async fn schema(pool: &SqlitePool) -> Result<Vec<String>, sqlx_core::Error> {
    query_scalar("SELECT type || '|' || name || '|' || tbl_name || '|' || coalesce(sql,'') FROM sqlite_master ORDER BY type,name")
        .fetch_all(pool).await
}

async fn all_rows(pool: &SqlitePool) -> Result<Vec<(String, Vec<String>)>, sqlx_core::Error> {
    let tables: Vec<String> = query_scalar(
        "SELECT name FROM sqlite_master WHERE type='table' AND name NOT LIKE 'sqlite_%' ORDER BY name",
    ).fetch_all(pool).await?;
    let mut snapshot = Vec::new();
    for table in tables {
        let columns: Vec<String> = query_scalar("SELECT name FROM pragma_table_info(?)")
            .bind(&table)
            .fetch_all(pool)
            .await?;
        let quoted = columns
            .iter()
            .map(|c| format!("quote(\"{}\")", c.replace('"', "\"\"")))
            .collect::<Vec<_>>()
            .join(",");
        let rows = query(&format!(
            "SELECT {quoted} FROM \"{}\"",
            table.replace('"', "\"\"")
        ))
        .fetch_all(pool)
        .await?;
        let mut values = rows
            .iter()
            .map(|row| {
                (0..columns.len())
                    .map(|i| row.try_get::<String, _>(i))
                    .collect::<Result<Vec<_>, _>>()
                    .map(|v| v.join("|"))
            })
            .collect::<Result<Vec<_>, _>>()?;
        values.sort();
        snapshot.push((table, values));
    }
    Ok(snapshot)
}

async fn old_attempts(pool: &SqlitePool) -> Result<Vec<String>, sqlx_core::Error> {
    query_scalar("SELECT hex(id) || '|' || hex(target_role_id) || '|' || hex(target_binding_id) || '|' || target_binding_generation || '|' || status || '|' || coalesce(error_code,'') || '|' || created_at || '|' || coalesce(started_at,'') || '|' || coalesce(completed_at,'') FROM delivery_attempts ORDER BY id")
        .fetch_all(pool).await
}

async fn refuse(pool: &SqlitePool, sql: &str, marker: &str) {
    let before = all_rows(pool).await.expect("snapshot before refusal");
    let error = query(sql)
        .execute(pool)
        .await
        .expect_err("operation must be refused");
    assert!(
        error.to_string().contains(marker),
        "unrelated refusal: {error}"
    );
    assert_eq!(
        all_rows(pool).await.expect("snapshot after refusal"),
        before
    );
}

fn event(tag: u16, team: u16, subject: u16, version: i64) -> String {
    format!(
        "INSERT INTO events(id,team_id,event_type,actor_type,actor_id,subject_type,subject_id,request_id,object_version,payload,metadata,occurred_at,previous_hash,hash) VALUES({}, {}, 'MESSAGE_REASSIGNED','HUMAN',NULL,'MESSAGE',{}, {},{version},'{{}}','{{}}',100,NULL,'{tag:064X}')",
        id(tag),
        id(team),
        id(subject),
        id(tag)
    )
}

fn override_row(
    number: i64,
    from: u16,
    to: u16,
    binding: u16,
    event_tag: u16,
    key: &str,
) -> String {
    format!(
        "INSERT INTO message_route_overrides(id,team_id,message_id,override_number,from_role_id,to_role_id,target_binding_id,target_binding_generation,actor_fingerprint,command_name,idempotency_key,event_id,request_id,created_at) VALUES({}, {}, {},{number},{},{},{},1,X'{FINGERPRINT}','message.reassign','{key}',{},{},100)",
        id(0x90 + number as u16),
        id(1),
        id(0x41),
        id(from),
        id(to),
        id(binding),
        id(event_tag),
        id(event_tag)
    )
}

fn attempt(number: i64, to: u16, binding: u16, override_id: &str, override_number: &str) -> String {
    format!(
        "INSERT INTO delivery_attempts(id,team_id,message_id,attempt_number,target_role_id,target_binding_id,target_binding_generation,status,created_at,route_override_id,route_override_number) VALUES({}, {}, {},{number},{},{},1,'PENDING',200,{override_id},{override_number})",
        id(0xb0 + number as u16),
        id(1),
        id(0x41),
        id(to),
        id(binding)
    )
}

async fn reassignment(
    pool: &SqlitePool,
    number: i64,
    from: u16,
    to: u16,
    binding: u16,
) -> TestResult {
    let mut tx = pool.begin().await?;
    let key = format!("route-test-{number}");
    let event_tag = 0x70 + number as u16;
    let version: i64 = query_scalar("SELECT version FROM messages WHERE id=?")
        .bind(hex_id(0x41))
        .fetch_one(&mut *tx)
        .await?;
    query(&format!("INSERT INTO command_idempotency(team_id,actor_fingerprint,command_name,idempotency_key,payload_hash,state,created_at,updated_at) VALUES({},X'{FINGERPRINT}','message.reassign','{key}',zeroblob(32),'IN_PROGRESS',100,100)",id(1)))
        .execute(&mut *tx).await?;
    query(&event(event_tag, 1, 0x41, version + 1))
        .execute(&mut *tx)
        .await?;
    query(&override_row(number, from, to, binding, event_tag, &key))
        .execute(&mut *tx)
        .await?;
    query(&attempt(
        number + 2,
        to,
        binding,
        &id(0x90 + number as u16),
        &number.to_string(),
    ))
    .execute(&mut *tx)
    .await?;
    let result = query(&format!("UPDATE messages SET status='QUEUED',version=version+1,updated_at=100 WHERE id={} AND version={version}",id(0x41)))
        .execute(&mut *tx).await?;
    assert_eq!(result.rows_affected(), 1);
    query(&format!("INSERT INTO outbox(id,team_id,event_id,destination,payload,status,created_at,scheduled_at) VALUES({}, {}, {},'notification','{{}}','PENDING',100,100)",id(0x170+number as u16),id(1),id(event_tag)))
        .execute(&mut *tx).await?;
    query(&format!("UPDATE command_idempotency SET state='SUCCEEDED',result_ref={},updated_at=101 WHERE team_id={} AND actor_fingerprint=X'{FINGERPRINT}' AND command_name='message.reassign' AND idempotency_key='{key}'",id(0x90+number as u16),id(1)))
        .execute(&mut *tx).await?;
    tx.commit().await?;
    Ok(())
}

fn hex_id(tag: u16) -> Vec<u8> {
    let text = format!("01921389d31174628f80cb5f79ab{tag:04x}");
    (0..text.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&text[i..i + 2], 16).expect("synthetic UUID"))
        .collect()
}

async fn fail_attempt(pool: &SqlitePool, tag: u16) -> TestResult {
    // Follow the frozen PENDING -> IN_FLIGHT -> FAILED chain in fixtures.
    for statement in [
        format!(
            "UPDATE delivery_attempts SET status='IN_FLIGHT',started_at=201 WHERE id={} AND status='PENDING'",
            id(tag)
        ),
        format!(
            "UPDATE delivery_attempts SET status='FAILED',error_code='DELIVERY_TIMEOUT',error_message='synthetic failure',completed_at=250 WHERE id={} AND status='IN_FLIGHT'",
            id(tag)
        ),
    ] {
        assert_eq!(query(&statement).execute(pool).await?.rows_affected(), 1);
    }
    Ok(())
}

#[tokio::test]
async fn registered_upgrade_preserves_all_existing_history() -> TestResult {
    let pool = historical_database().await?;
    let before = all_rows(&pool).await?;
    let attempts = old_attempts(&pool).await?;
    migrate(&pool).await?;
    assert_eq!(query_scalar::<_,i64>("SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='message_route_overrides'").fetch_one(&pool).await?,1);
    assert_eq!(old_attempts(&pool).await?, attempts);
    let after = all_rows(&pool).await?;
    for (table, rows) in before {
        let actual = &after
            .iter()
            .find(|(name, _)| *name == table)
            .expect("old table retained")
            .1;
        if table == "_sqlx_migrations" {
            assert!(
                rows.iter().all(|row| actual.contains(row)),
                "published migration ledger changed"
            );
            assert_eq!(actual.len(), loader().await?.migrations.len());
        } else if table == "delivery_attempts" {
            assert_eq!(
                actual
                    .iter()
                    .map(|row| row
                        .strip_suffix("|NULL|NULL")
                        .expect("legacy attempts adopt no override")
                        .to_owned())
                    .collect::<Vec<_>>(),
                rows,
                "old attempt fields changed"
            );
        } else if table == "command_idempotency" {
            // 0031 only appends nullable first-error details. Every original
            // key/hash/outcome/timestamp remains byte-for-byte preserved.
            assert_eq!(
                actual
                    .iter()
                    .map(|row| row
                        .strip_suffix("|NULL")
                        .expect("legacy error details remain NULL")
                        .to_owned())
                    .collect::<Vec<_>>(),
                rows,
                "old command outcomes changed"
            );
        } else {
            assert_eq!(*actual, rows, "old {table} rows changed");
        }
    }
    assert_eq!(query_scalar::<_,i64>("SELECT COUNT(*) FROM delivery_attempts WHERE route_override_id IS NOT NULL OR route_override_number IS NOT NULL").fetch_one(&pool).await?,0);
    assert_eq!(
        query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM _sqlx_migrations WHERE version=30 AND success=TRUE"
        )
        .fetch_one(&pool)
        .await?,
        1
    );
    migrate(&pool).await?;
    assert_eq!(
        all_rows(&pool).await?,
        after,
        "rerunning completed migrations must be inert"
    );
    Ok(())
}

#[tokio::test]
async fn three_reassignments_keep_each_attempts_real_recipient() -> TestResult {
    let pool = historical_database().await?;
    let original = old_attempts(&pool).await?;
    migrate(&pool).await?;
    for (index, from, to, binding) in [
        (1, 0x11, 0x12, 0x32),
        (2, 0x12, 0x13, 0x33),
        (3, 0x13, 0x11, 0x31),
    ] {
        reassignment(&pool, index, from, to, binding).await?;
        if index < 3 {
            fail_attempt(&pool, 0xb0 + (index + 2) as u16).await?;
            query(&format!("UPDATE messages SET status='NEEDS_HUMAN',version=version+1,updated_at=101 WHERE id={}",id(0x41))).execute(&pool).await?;
        }
    }
    let final_attempts = old_attempts(&pool).await?;
    assert!(
        original.iter().all(|row| final_attempts.contains(row)),
        "old attempts changed"
    );
    let recipients: Vec<Vec<u8>> = query_scalar(
        "SELECT target_role_id FROM delivery_attempts WHERE message_id=? ORDER BY attempt_number",
    )
    .bind(hex_id(0x41))
    .fetch_all(&pool)
    .await?;
    assert_eq!(
        recipients,
        vec![
            hex_id(0x11),
            hex_id(0x11),
            hex_id(0x12),
            hex_id(0x13),
            hex_id(0x11)
        ]
    );
    assert_eq!(
        query_scalar::<_, Vec<u8>>("SELECT to_role_id FROM messages WHERE id=?")
            .bind(hex_id(0x41))
            .fetch_one(&pool)
            .await?,
        hex_id(0x11)
    );
    Ok(())
}

#[tokio::test]
async fn route_linkage_and_history_mutations_are_refused_without_side_effects() -> TestResult {
    let pool = historical_database().await?;
    migrate(&pool).await?;
    reassignment(&pool, 1, 0x11, 0x12, 0x32).await?;
    fail_attempt(&pool, 0xb3).await?;
    let valid = attempt(4, 0x12, 0x32, &id(0x91), "1");
    // Under the candidate migration two diagnostics in this loop are produced
    // by a different guard than under version 30, because the route trigger no
    // longer compares the attempt's Binding against the override's decision
    // Binding. In both cases the write is still refused and every row is still
    // unchanged; only the enforcing guard moved. No case may be removed or
    // relaxed: each one still has to be refused.
    for (statement, marker) in [
        // The attempt targets a Role the override did not authorize. The route
        // trigger now refuses it first; the Team consistency trigger would also
        // refuse it.
        (
            attempt(4, 0x13, 0x33, &id(0x91), "1"),
            "Delivery attempt does not match the override",
        ),
        // A Binding that belongs to another Role. 0017's target Binding snapshot
        // guard refuses it now that the route trigger stops comparing Binding
        // identity. Version 30 reported this through the route trigger instead.
        (
            attempt(4, 0x12, 0x31, &id(0x91), "1"),
            "Delivery target Binding snapshot violation",
        ),
        (attempt(4, 0x12, 0x32, &id(0x91), "NULL"), "does not match"),
        (attempt(4, 0x11, 0x31, "NULL", "1"), "does not match"),
    ] {
        refuse(&pool, &statement, marker).await;
    }
    // The slot is free throughout the negatives and the legal insert control;
    // a busy-slot rejection cannot masquerade as a linkage check.
    query(&valid).execute(&pool).await?;
    refuse(&pool,&format!("UPDATE delivery_attempts SET route_override_id=NULL,route_override_number=NULL WHERE id={}",id(0xb4)),"immutable").await;
    refuse(
        &pool,
        &format!(
            "UPDATE message_route_overrides SET to_role_id={} WHERE id={}",
            id(0x13),
            id(0x91)
        ),
        "immutable",
    )
    .await;
    refuse(
        &pool,
        &format!("DELETE FROM message_route_overrides WHERE id={}", id(0x91)),
        "retained",
    )
    .await;
    refuse(&pool,&format!("INSERT OR REPLACE INTO message_route_overrides SELECT * FROM message_route_overrides WHERE id={}",id(0x91)),"written once").await;
    Ok(())
}

#[tokio::test]
async fn every_drop_create_gap_rolls_back_the_entire_migration_on_history() -> TestResult {
    for target in [
        "delivery_attempts_team_consistency_insert",
        "delivery_attempts_team_consistency_update",
        "delivery_attempts_identity_immutable_update",
    ] {
        let pool = historical_database().await?;
        let old_schema = schema(&pool).await?;
        let old_rows = all_rows(&pool).await?;
        let mut migrator = loader().await?;
        let row = migrator
            .migrations
            .to_mut()
            .iter_mut()
            .find(|m| m.version == 30)
            .expect("production 0030 must be registered");
        assert!(
            !row.no_tx,
            "replacement guards must share one SQLx transaction"
        );
        let marker = format!("DROP TRIGGER {target};");
        assert_eq!(row.sql.matches(&marker).count(), 1);
        let injected = row.sql.replace(
            &marker,
            &format!("{marker}\nSELECT * FROM migration_0030_injected_failure;"),
        );
        *row = Migration::new(
            30,
            row.description.clone(),
            MigrationType::Simple,
            injected.into(),
            false,
        );
        let error = migrator
            .run(&pool)
            .await
            .expect_err("injected migration must fail");
        assert!(
            error
                .to_string()
                .contains("migration_0030_injected_failure"),
            "unrelated failure: {error}"
        );
        assert_eq!(schema(&pool).await?, old_schema, "{target}: schema drift");
        assert_eq!(
            all_rows(&pool).await?,
            old_rows,
            "{target}: historical/ledger drift"
        );
        migrate(&pool).await?;
        assert_eq!(
            query_scalar::<_, i64>(
                "SELECT COUNT(*) FROM _sqlx_migrations WHERE version=30 AND success=TRUE"
            )
            .fetch_one(&pool)
            .await?,
            1
        );
    }
    Ok(())
}

#[tokio::test]
async fn old_loader_refuses_new_schema_instead_of_claiming_a_safe_binary_rollback() -> TestResult {
    let pool = historical_database().await?;
    migrate(&pool).await?;
    let mut legacy = loader().await?;
    legacy.migrations.to_mut().retain(|m| m.version <= 29);
    let error = legacy
        .run(&pool)
        .await
        .expect_err("old loader must reject unknown migration");
    assert!(
        matches!(error, sqlx_core::migrate::MigrateError::VersionMissing(30)),
        "unexpected old-loader result: {error}"
    );
    Ok(())
}

#[tokio::test]
async fn a_route_cannot_adopt_an_unrelated_event_even_when_all_ids_exist() -> TestResult {
    // This is a real RED baseline: the audited concept accepts a valid Event
    // of another Message. The production forward migration must reject it.
    for baseline in [true, false] {
        let pool = historical_database().await?;
        if baseline {
            let mut migrator = loader().await?;
            let row = migrator
                .migrations
                .to_mut()
                .iter_mut()
                .find(|m| m.version == 30)
                .expect("production version 30");
            *row = Migration::new(
                30,
                row.description.clone(),
                MigrationType::Simple,
                CONCEPT_BASELINE.into(),
                false,
            );
            migrator.run(&pool).await?;
        } else {
            migrate(&pool).await?;
        }
        let before = all_rows(&pool).await?;
        let mut tx = pool.begin().await?;
        query(&format!("INSERT INTO command_idempotency(team_id,actor_fingerprint,command_name,idempotency_key,payload_hash,state,created_at,updated_at) VALUES({},X'{FINGERPRINT}','message.reassign','unrelated-event',zeroblob(32),'IN_PROGRESS',100,100)",id(1)))
            .execute(&mut *tx).await?;
        query(&event(0x7f, 1, 0x42, 3)).execute(&mut *tx).await?;
        let result = query(&override_row(1, 0x11, 0x12, 0x32, 0x7f, "unrelated-event"))
            .execute(&mut *tx)
            .await;
        if baseline {
            assert_eq!(
                result
                    .expect("concept baseline accepts mismatched event")
                    .rows_affected(),
                1
            );
            tx.commit().await?;
            assert_ne!(all_rows(&pool).await?, before);
        } else {
            let error = result.expect_err("production guard must reject mismatched Event");
            assert!(
                error.to_string().contains("Route override target invalid"),
                "unrelated refusal: {error}"
            );
            tx.rollback().await?;
            assert_eq!(
                all_rows(&pool).await?,
                before,
                "Event and idempotency must roll back with the failed fixture transaction"
            );
            reassignment(&pool, 1, 0x11, 0x12, 0x32).await?;
        }
    }
    Ok(())
}

#[tokio::test]
async fn route_request_must_match_the_request_of_its_event() -> TestResult {
    let pool = historical_database().await?;
    migrate(&pool).await?;
    let before = all_rows(&pool).await?;
    let mut tx = pool.begin().await?;
    query(&format!("INSERT INTO command_idempotency(team_id,actor_fingerprint,command_name,idempotency_key,payload_hash,state,created_at,updated_at) VALUES({},X'{FINGERPRINT}','message.reassign','request-link',zeroblob(32),'IN_PROGRESS',100,100)",id(1)))
        .execute(&mut *tx).await?;
    query(&event(0x7e, 1, 0x41, 6)).execute(&mut *tx).await?;
    let valid = override_row(1, 0x11, 0x12, 0x32, 0x7e, "request-link");
    let old_suffix = format!(",{},100)", id(0x7e));
    assert_eq!(valid.matches(&old_suffix).count(), 1);
    let invalid = valid.replace(&old_suffix, &format!(",{},100)", id(0x7d)));
    let error = query(&invalid)
        .execute(&mut *tx)
        .await
        .expect_err("different request must be refused");
    assert!(
        error.to_string().contains("Route override target invalid"),
        "unrelated refusal: {error}"
    );
    query(&valid).execute(&mut *tx).await?;
    tx.rollback().await?;
    assert_eq!(all_rows(&pool).await?, before);
    Ok(())
}

#[tokio::test]
async fn published_retention_guards_keep_the_same_refusals_after_upgrade() -> TestResult {
    let pool = historical_database().await?;
    // The seed has real rows for all six 0028 objects. Read each identity,
    // prove the target is non-empty, and compare the actual rejection twice.
    let tables = [
        "teams",
        "roles",
        "tasks",
        "bindings",
        "references",
        "delivery_attempts",
    ];
    let mut probes = Vec::new();
    for table in tables {
        let target: Vec<u8> =
            query_scalar(&format!("SELECT id FROM \"{table}\" ORDER BY id LIMIT 1"))
                .fetch_one(&pool)
                .await?;
        let hex = target
            .iter()
            .map(|b| format!("{b:02X}"))
            .collect::<String>();
        probes.extend([
            format!("DELETE FROM \"{table}\" WHERE id=X'{hex}'"),
            format!(
                "INSERT OR REPLACE INTO \"{table}\" SELECT * FROM \"{table}\" WHERE id=X'{hex}'"
            ),
            format!("UPDATE \"{table}\" SET id={} WHERE id=X'{hex}'", id(0xff00)),
        ]);
    }
    let mut before_errors = Vec::new();
    let before_rows = all_rows(&pool).await?;
    for statement in &probes {
        let error = query(statement)
            .execute(&pool)
            .await
            .expect_err("existing retention must refuse");
        assert_eq!(
            all_rows(&pool).await?,
            before_rows,
            "old guard mutated rows"
        );
        before_errors.push(error.to_string());
    }
    migrate(&pool).await?;
    let after_rows = all_rows(&pool).await?;
    for (statement, expected) in probes.iter().zip(before_errors) {
        let error = query(statement)
            .execute(&pool)
            .await
            .expect_err("upgraded retention must refuse");
        assert_eq!(error.to_string(), expected, "retention diagnostic changed");
        assert_eq!(all_rows(&pool).await?, after_rows, "new guard mutated rows");
    }
    Ok(())
}
