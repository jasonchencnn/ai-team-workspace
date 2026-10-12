//! DEV-02-08 HC-01: the delivery attempt records the Binding it
//! actually used, not the Binding the override decided on.
//!
//! Frozen basis: STATE_MACHINES.md section 4 (a queued message re-resolves its
//! Binding at delivery time) and section 5 (`QUEUED -> ROUTED` resolves the
//! current ACTIVE Binding and creates the attempt with that snapshot;
//! `NEEDS_HUMAN -> QUEUED` creates a new attempt and records the route
//! override); DOMAIN_MODEL.md sections 3/5; DATA_CONVENTIONS sections 5/6.
//!
//! Every statement below is a synthetic SQLite fixture. This file is not a
//! production Messaging or Binding service: actor authentication, permission
//! layers, TTL evaluation and choosing which binding is ACTIVE at dispatch stay
//! at the application execution point. Published migrations 0001-0032 and the
//! historical AF evidence are read-only inputs.

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
const FINGERPRINT: &str = "4f574e4552000000000000000000000000000000000000000000000000000000";

/// Official forward migration version under test.
const CANDIDATE: i64 = 33;

fn id(tag: u16) -> String {
    format!("X'01921389d31174628f80cb5f79ab{tag:04x}'")
}

fn hex_id(tag: u16) -> Vec<u8> {
    let text = format!("01921389d31174628f80cb5f79ab{tag:04x}");
    (0..text.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&text[i..i + 2], 16).expect("synthetic UUID"))
        .collect()
}

async fn loader() -> Result<Migrator, sqlx_core::migrate::MigrateError> {
    Migrator::new(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("migrations")).await
}

async fn loader_through(limit: i64) -> Result<Migrator, sqlx_core::migrate::MigrateError> {
    let mut migrator = loader().await?;
    migrator
        .migrations
        .to_mut()
        .retain(|row| row.version <= limit);
    Ok(migrator)
}

async fn pool() -> Result<SqlitePool, Box<dyn Error>> {
    Ok(SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(SqliteConnectOptions::from_str("sqlite::memory:")?.foreign_keys(true))
        .await?)
}

/// A 29-era database with the retained AF-02 history already present.
async fn historical_database() -> Result<SqlitePool, Box<dyn Error>> {
    let pool = pool().await?;
    loader_through(29).await?.run(&pool).await?;
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

async fn all_rows(pool: &SqlitePool) -> Result<Vec<(String, Vec<String>)>, sqlx_core::Error> {
    let tables: Vec<String> = query_scalar(
        "SELECT name FROM sqlite_master WHERE type='table' AND name NOT LIKE 'sqlite_%' ORDER BY name",
    )
    .fetch_all(pool)
    .await?;
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

/// Full delivery history, including which override each attempt adopted.
async fn attempt_history(pool: &SqlitePool) -> Result<Vec<String>, sqlx_core::Error> {
    query_scalar(
        "SELECT hex(id) || '|' || hex(message_id) || '|' || attempt_number || '|'
            || hex(target_role_id) || '|' || coalesce(hex(target_binding_id),'NULL') || '|'
            || coalesce(target_binding_generation,'NULL') || '|' || status || '|'
            || coalesce(hex(route_override_id),'NULL') || '|'
            || coalesce(route_override_number,'NULL') || '|' || created_at
            || '|' || hex(team_id) || '|' || quote(error_code) || '|' || quote(error_message)
            || '|' || quote(started_at) || '|' || quote(completed_at)
         FROM delivery_attempts ORDER BY id",
    )
    .fetch_all(pool)
    .await
}

/// The override decision snapshot, as stored.
async fn override_snapshot(pool: &SqlitePool) -> Result<Vec<String>, sqlx_core::Error> {
    query_scalar(
        "SELECT hex(id) || '|' || hex(message_id) || '|' || override_number || '|'
            || hex(from_role_id) || '|' || hex(to_role_id) || '|'
            || hex(target_binding_id) || '|' || target_binding_generation || '|'
            || hex(event_id) || '|' || hex(request_id) || '|' || created_at
            || '|' || hex(team_id) || '|' || hex(actor_fingerprint)
            || '|' || quote(command_name) || '|' || quote(idempotency_key)
         FROM message_route_overrides ORDER BY id",
    )
    .fetch_all(pool)
    .await
}

async fn message_row(pool: &SqlitePool) -> Result<String, sqlx_core::Error> {
    query_scalar(
        "SELECT hex(id) || '|' || hex(to_role_id) || '|' || message_type || '|' || body || '|'
            || status || '|' || version FROM messages WHERE id=?",
    )
    .bind(hex_id(0x41))
    .fetch_one(pool)
    .await
}

async fn refuse(pool: &SqlitePool, sql: &str, marker: &str) {
    refuse_named(pool, sql, marker, "operation").await
}

/// Same check, naming the case so a failure identifies which rule broke.
async fn refuse_named(pool: &SqlitePool, sql: &str, marker: &str, case: &str) {
    let before = all_rows(pool).await.expect("snapshot before refusal");
    let error = query(sql)
        .execute(pool)
        .await
        .expect_err("operation must be refused");
    assert!(
        error.to_string().contains(marker),
        "unrelated refusal in [{case}]: {error}"
    );
    assert_eq!(
        all_rows(pool).await.expect("snapshot after refusal"),
        before,
        "[{case}] a refused write must leave every row byte-identical"
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
    generation: &str,
    event_tag: u16,
    key: &str,
) -> String {
    format!(
        "INSERT INTO message_route_overrides(id,team_id,message_id,override_number,from_role_id,to_role_id,target_binding_id,target_binding_generation,actor_fingerprint,command_name,idempotency_key,event_id,request_id,created_at) VALUES({}, {}, {},{number},{},{},{},{generation},X'{FINGERPRINT}','message.reassign','{key}',{},{},100)",
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

/// An attempt insert against message 0x41. `generation` is the generation the
/// attempt claims for the Binding it actually used.
fn attempt_on_41(
    number: i64,
    to: u16,
    binding: u16,
    generation: &str,
    override_id: &str,
    override_number: &str,
) -> String {
    format!(
        "INSERT INTO delivery_attempts(id,team_id,message_id,attempt_number,target_role_id,target_binding_id,target_binding_generation,status,created_at,route_override_id,route_override_number) VALUES({}, {}, {},{number},{},{},{generation},'PENDING',200,{override_id},{override_number})",
        id(0xb0 + number as u16),
        id(1),
        id(0x41),
        id(to),
        id(binding)
    )
}

/// One authorized reassignment: idempotency row, Event, override, attempt, the
/// Message going back to QUEUED, the outbox row and the command outcome.
async fn reassignment(
    pool: &SqlitePool,
    number: i64,
    from: u16,
    to: u16,
    binding: u16,
) -> TestResult {
    let mut tx = pool.begin().await?;
    let key = format!("hc01-route-{number}");
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
    query(&override_row(
        number, from, to, binding, "1", event_tag, &key,
    ))
    .execute(&mut *tx)
    .await?;
    query(&attempt_on_41(
        number + 2,
        to,
        binding,
        "1",
        &id(0x90 + number as u16),
        &number.to_string(),
    ))
    .execute(&mut *tx)
    .await?;
    // The reassignment leaves the Message QUEUED. A Message that is already
    // QUEUED must not be driven QUEUED -> QUEUED again, so only move a Message
    // that is waiting on a human. This mirrors the frozen NEEDS_HUMAN -> QUEUED
    // edge; it is not a relaxation of any guard.
    let waiting: i64 =
        query_scalar("SELECT COUNT(*) FROM messages WHERE id=? AND status='NEEDS_HUMAN'")
            .bind(hex_id(0x41))
            .fetch_one(&mut *tx)
            .await?;
    if waiting == 1 {
        let result = query(&format!("UPDATE messages SET status='QUEUED',version=version+1,updated_at=100 WHERE id={} AND version={version}",id(0x41)))
            .execute(&mut *tx).await?;
        assert_eq!(result.rows_affected(), 1);
    }
    query(&format!("INSERT INTO outbox(id,team_id,event_id,destination,payload,status,created_at,scheduled_at) VALUES({}, {}, {},'notification','{{}}','PENDING',100,100)",id(0x170+number as u16),id(1),id(event_tag)))
        .execute(&mut *tx).await?;
    query(&format!("UPDATE command_idempotency SET state='SUCCEEDED',result_ref={},updated_at=101 WHERE team_id={} AND actor_fingerprint=X'{FINGERPRINT}' AND command_name='message.reassign' AND idempotency_key='{key}'",id(0x90+number as u16),id(1)))
        .execute(&mut *tx).await?;
    tx.commit().await?;
    Ok(())
}

async fn fail_attempt(pool: &SqlitePool, tag: u16) -> TestResult {
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

/// A legal rebind of `role`: revoke the current ACTIVE binding, then move a new
/// one through PENDING_CHECK -> ACTIVE with a higher generation. 0002 requires
/// non-overlapping effective ranges, so `effective_to` precedes `effective_from`.
async fn rebind_role(
    pool: &SqlitePool,
    old_binding: u16,
    role: u16,
    new_binding: u16,
) -> TestResult {
    assert_eq!(
        query(&format!(
            "UPDATE bindings SET status='REVOKED',effective_to=300,version=version+1,updated_at=300 WHERE id={}",
            id(old_binding)
        ))
        .execute(pool)
        .await?
        .rows_affected(),
        1
    );
    assert_eq!(
        query(&format!("INSERT INTO bindings(id,team_id,role_id,harness_instance_id,adapter_id,status,binding_generation,version,configuration_snapshot,created_at,updated_at,effective_from,effective_to) VALUES({},{},{},{},{},'PENDING_CHECK',2,1,'{{}}',301,301,NULL,NULL)",id(new_binding),id(1),id(role),id(0x20),id(0x21)))
            .execute(pool)
            .await?
            .rows_affected(),
        1
    );
    assert_eq!(
        query(&format!(
            "UPDATE bindings SET status='ACTIVE',effective_from=301,version=version+1,updated_at=302 WHERE id={}",
            id(new_binding)
        ))
        .execute(pool)
        .await?
        .rows_affected(),
        1
    );
    Ok(())
}

/// A reassigned Role whose Binding has since been rebound: override 1 pinned
/// role B / binding generation 1, that attempt failed, and role B now has a
/// different ACTIVE binding at generation 2.
async fn reassigned_then_rebound() -> Result<SqlitePool, Box<dyn Error>> {
    let pool = historical_database().await?;
    migrate(&pool).await?;
    assert_eq!(
        query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM _sqlx_migrations WHERE version=33 AND success=TRUE"
        )
        .fetch_one(&pool)
        .await?,
        1
    );
    reassignment(&pool, 1, 0x11, 0x12, 0x32).await?;
    fail_attempt(&pool, 0xb3).await?;
    // Seed attempt a2 is already IN_FLIGHT; do not replay its PENDING step.
    assert_eq!(
        query(&format!("UPDATE delivery_attempts SET status='FAILED',error_code='DELIVERY_TIMEOUT',error_message='synthetic failure',completed_at=250 WHERE id={} AND status='IN_FLIGHT'", id(0xa2)))
            .execute(&pool)
            .await?
            .rows_affected(),
        1
    );
    rebind_role(&pool, 0x32, 0x12, 0x34).await?;
    Ok(pool)
}

/// The retry Frozen routing requires: adopt the existing override and record the
/// Binding the Role actually has now.
fn rebound_retry() -> String {
    attempt_on_41(4, 0x12, 0x34, "2", &id(0x91), "1")
}

// ---------------------------------------------------------------------------
// 1. Frozen positive behaviour: the current Binding is accepted, and no history
//    is reinterpreted.
// ---------------------------------------------------------------------------

#[tokio::test]
async fn reassigned_role_retry_adopts_the_current_binding_without_rewriting_history() -> TestResult
{
    let pool = reassigned_then_rebound().await?;
    let attempts_before = attempt_history(&pool).await?;
    let overrides_before = override_snapshot(&pool).await?;
    let message_before = message_row(&pool).await?;

    // The scenario is real, not a busy slot or a foreign id: exactly one
    // ACTIVE binding for role B, at generation 2, and the override still names
    // role B.
    assert_eq!(
        query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM bindings WHERE role_id=? AND status='ACTIVE' AND binding_generation=2"
        )
        .bind(hex_id(0x12))
        .fetch_one(&pool)
        .await?,
        1
    );
    assert_eq!(
        query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM message_route_overrides WHERE id=? AND to_role_id=?",
        )
        .bind(hex_id(0x91))
        .bind(hex_id(0x12))
        .fetch_one(&pool)
        .await?,
        1
    );

    assert_eq!(
        query(&rebound_retry())
            .execute(&pool)
            .await?
            .rows_affected(),
        1,
        "the current Binding must be accepted for the adopted override"
    );

    // Message addressing and body are untouched by the retry.
    assert_eq!(message_row(&pool).await?, message_before);
    // The override decision snapshot is untouched.
    assert_eq!(override_snapshot(&pool).await?, overrides_before);
    // Every old attempt row survives verbatim; exactly one row was appended and
    // it records the Binding actually used, not the one decided on.
    let attempts_after = attempt_history(&pool).await?;
    assert!(
        attempts_before
            .iter()
            .all(|row| attempts_after.contains(row)),
        "old attempt snapshots must be retained"
    );
    assert_eq!(attempts_after.len(), attempts_before.len() + 1);
    let appended = attempts_after
        .iter()
        .find(|row| !attempts_before.contains(row))
        .expect("the new attempt");
    let columns: Vec<&str> = appended.split('|').collect();
    assert_eq!(
        columns[1],
        hex_id(0x41)
            .iter()
            .map(|b| format!("{b:02X}"))
            .collect::<String>(),
        "attempt must stay on the same Message"
    );
    assert_eq!(
        columns[3],
        hex_id(0x12)
            .iter()
            .map(|b| format!("{b:02X}"))
            .collect::<String>(),
        "attempt must stay on the override's target Role"
    );
    assert_eq!(
        columns[4],
        hex_id(0x34)
            .iter()
            .map(|b| format!("{b:02X}"))
            .collect::<String>(),
        "attempt must record the CURRENT binding"
    );
    assert_eq!(
        columns[5], "2",
        "attempt must record the CURRENT generation"
    );
    assert_eq!(
        columns[7],
        hex_id(0x91)
            .iter()
            .map(|b| format!("{b:02X}"))
            .collect::<String>(),
        "attempt must still adopt the original override"
    );
    assert_eq!(
        columns[8], "1",
        "attempt must still cite the original override number"
    );
    // The decision snapshot is still readable and still says the binding the
    // override itself chose, at generation 1, even though the retry used
    // generation 2 of a different row.
    let decision = override_snapshot(&pool).await?;
    assert_eq!(
        decision, overrides_before,
        "override decision snapshot changed"
    );
    let binding32 = hex_id(0x32)
        .iter()
        .map(|b| format!("{b:02X}"))
        .collect::<String>();
    let binding34 = hex_id(0x34)
        .iter()
        .map(|b| format!("{b:02X}"))
        .collect::<String>();
    assert!(
        decision[0].contains(&format!("|{binding32}|1|")),
        "the override must still record its own decision binding: {}",
        decision[0]
    );
    assert!(
        !decision[0].contains(&binding34),
        "the retry's binding must not have leaked into the decision snapshot"
    );
    Ok(())
}

#[tokio::test]
async fn original_addressing_retry_after_rebinding_also_uses_the_current_binding() -> TestResult {
    // The same rebind, reached without any reassignment: message 0x41 still
    // addresses role A, and role A's binding moved to generation 2. The frozen
    // `QUEUED -> ROUTED` rule resolves the current binding there too, so this
    // path must accept the new binding and keep the NULL-override path intact.
    let pool = historical_database().await?;
    migrate(&pool).await?;
    let attempts_before = attempt_history(&pool).await?;
    rebind_role(&pool, 0x31, 0x11, 0x35).await?;
    let statement = attempt_on_41(9, 0x11, 0x35, "2", "NULL", "NULL");
    assert_eq!(query(&statement).execute(&pool).await?.rows_affected(), 1);
    let after = attempt_history(&pool).await?;
    assert!(attempts_before.iter().all(|row| after.contains(row)));
    // A NULL override still may not be combined with a number.
    refuse(
        &pool,
        &attempt_on_41(10, 0x11, 0x35, "2", "NULL", "1"),
        "does not match the override",
    )
    .await;
    // ...and an attempt on the original path still must address the Message's
    // own Role.
    refuse(
        &pool,
        &attempt_on_41(10, 0x12, 0x35, "2", "NULL", "NULL"),
        "Team consistency",
    )
    .await;
    Ok(())
}

// ---------------------------------------------------------------------------
// 2. The route association stays mandatory. Only the decision-snapshot identity
//    was released; everything else still refuses with no side effects.
// ---------------------------------------------------------------------------

#[tokio::test]
async fn every_route_linkage_error_is_still_refused() -> TestResult {
    let pool = reassigned_then_rebound().await?;

    for (label, statement, marker) in [
        // Wrong override number for the same override row.
        (
            "wrong override number",
            attempt_on_41(4, 0x12, 0x34, "2", &id(0x91), "2"),
            "does not match the override",
        ),
        // Half-written override: id without a number.
        (
            "half override id only",
            attempt_on_41(4, 0x12, 0x34, "2", &id(0x91), "NULL"),
            "does not match the override",
        ),
        // Half-written override: number without an id.
        (
            "half override number only",
            attempt_on_41(4, 0x12, 0x34, "2", "NULL", "1"),
            "does not match the override",
        ),
        // Target Role is not the Role the override authorized. The route
        // trigger fires first and refuses it; the Team consistency trigger would
        // also refuse it. Either diagnostic proves the attempt is rejected.
        (
            "wrong target role",
            attempt_on_41(4, 0x13, 0x33, "1", &id(0x91), "1"),
            "override",
        ),
        // A Binding that belongs to a different Role than the attempt's.
        (
            "binding of another role",
            attempt_on_41(4, 0x12, 0x31, "1", &id(0x91), "1"),
            "Delivery target Binding snapshot violation",
        ),
        // Right Binding, generation the attempt never actually saw.
        (
            "wrong generation",
            attempt_on_41(4, 0x12, 0x34, "7", &id(0x91), "1"),
            "Delivery target Binding snapshot violation",
        ),
        (
            "stale lower generation",
            attempt_on_41(4, 0x12, 0x34, "1", &id(0x91), "1"),
            "Delivery target Binding snapshot violation",
        ),
        // A Binding that does not exist in this Team at all. The Team
        // consistency trigger may refuse it before the 0017 snapshot trigger
        // does; either way the write is rejected.
        (
            "unknown binding",
            attempt_on_41(4, 0x12, 0x99, "2", &id(0x91), "1"),
            "Delivery",
        ),
        // Adopting an override while recording no Binding snapshot.
        (
            "missing binding snapshot",
            format!(
                "INSERT INTO delivery_attempts(id,team_id,message_id,attempt_number,target_role_id,target_binding_id,target_binding_generation,status,created_at,route_override_id,route_override_number) VALUES({}, {}, {},4,{},NULL,NULL,'PENDING',200,{},1)",
                id(0xb4),
                id(1),
                id(0x41),
                id(0x12),
                id(0x91)
            ),
            "Delivery",
        ),
        // Adopting an override while recording a Binding id but no generation.
        (
            "half binding snapshot",
            format!(
                "INSERT INTO delivery_attempts(id,team_id,message_id,attempt_number,target_role_id,target_binding_id,target_binding_generation,status,created_at,route_override_id,route_override_number) VALUES({}, {}, {},4,{},{},NULL,'PENDING',200,{},1)",
                id(0xb4),
                id(1),
                id(0x41),
                id(0x12),
                id(0x34),
                id(0x91)
            ),
            "does not match the override",
        ),
    ] {
        refuse_named(&pool, &statement, marker, label).await;
    }

    // Cross-Team: an attempt in the foreign Team adopting our Team's override.
    // The route trigger refuses it on team_id before the Team consistency
    // trigger can; either diagnostic proves the adoption is rejected.
    let cross_team = attempt_on_41(4, 0x12, 0x34, "2", &id(0x91), "1").replacen(&id(1), &id(2), 1);
    refuse_named(&pool, &cross_team, "Delivery", "cross team").await;

    // The four-tuple pre-emption guard is still in force: the same actor may not
    // reuse a key, and an override may not be replaced.
    refuse(
        &pool,
        &override_row(2, 0x11, 0x12, 0x34, "2", 0x75, "hc01-route-1"),
        "written once",
    )
    .await;
    refuse(
        &pool,
        &format!(
            "INSERT OR REPLACE INTO message_route_overrides SELECT * FROM message_route_overrides WHERE id={}",
            id(0x91)
        ),
        "written once",
    )
    .await;
    Ok(())
}

#[tokio::test]
async fn a_cross_message_override_cannot_be_adopted() -> TestResult {
    let pool = reassigned_then_rebound().await?;
    // A legal override for a different Message, committed for real. Message
    // 0x42 addresses role B, so its override continues from there. Its id and
    // idempotency key must differ from ours: both are globally write-once.
    let mut tx = pool.begin().await?;
    query(&format!("INSERT INTO command_idempotency(team_id,actor_fingerprint,command_name,idempotency_key,payload_hash,state,created_at,updated_at) VALUES({},X'{FINGERPRINT}','message.reassign','other-message',zeroblob(32),'IN_PROGRESS',400,400)",id(1)))
        .execute(&mut *tx).await?;
    query(&event(0x75, 1, 0x42, 4)).execute(&mut *tx).await?;
    let other = override_row(1, 0x12, 0x13, 0x33, "1", 0x75, "other-message")
        .replace(&id(0x41), &id(0x42))
        .replace(&id(0x91), &id(0x96));
    query(&other).execute(&mut *tx).await?;
    tx.commit().await?;

    // It exists, it is valid, and it still may not be adopted by an attempt on
    // message 0x41.
    assert_eq!(
        query_scalar::<_, i64>("SELECT COUNT(*) FROM message_route_overrides WHERE id=?",)
            .bind(hex_id(0x96))
            .fetch_one(&pool)
            .await?,
        1
    );
    refuse_named(
        &pool,
        &attempt_on_41(4, 0x13, 0x33, "1", &id(0x96), "1"),
        "does not match the override",
        "override belongs to another Message",
    )
    .await;
    // Adopting our own Message's override under the OTHER override's number is
    // equally refused.
    refuse_named(
        &pool,
        &attempt_on_41(4, 0x12, 0x34, "2", &id(0x91), "2"),
        "does not match the override",
        "override number of another Message",
    )
    .await;
    Ok(())
}

// ---------------------------------------------------------------------------
// 3. Repeated reassignment followed by a rebind still reads real history.
// ---------------------------------------------------------------------------

#[tokio::test]
async fn repeated_reassignment_then_rebinding_keeps_each_attempt_real() -> TestResult {
    let pool = historical_database().await?;
    migrate(&pool).await?;
    let original = attempt_history(&pool).await?;
    for (index, from, to, binding) in [
        (1, 0x11, 0x12, 0x32),
        (2, 0x12, 0x13, 0x33),
        (3, 0x13, 0x11, 0x31),
    ] {
        reassignment(&pool, index, from, to, binding).await?;
        if index < 3 {
            fail_attempt(&pool, 0xb0 + (index + 2) as u16).await?;
            query(&format!("UPDATE messages SET status='NEEDS_HUMAN',version=version+1,updated_at=101 WHERE id={}", id(0x41)))
                .execute(&pool)
                .await?;
        }
    }
    let before_rebind = attempt_history(&pool).await?;

    // The third override's attempt is still PENDING and holds the Message's only
    // active delivery slot, so it has to reach a terminal state before any retry
    // is legal. This is the frozen ROUTED -> QUEUED retry precondition, not a
    // fixture convenience. The reassignment helper already left the Message
    // QUEUED, which is where a retry starts, so no status write is needed here.
    fail_attempt(&pool, 0xb5).await?;

    // Role A is where the route came back to; rebind it and retry.
    rebind_role(&pool, 0x31, 0x11, 0x36).await?;
    assert_eq!(
        query(&attempt_on_41(6, 0x11, 0x36, "2", &id(0x93), "3"))
            .execute(&pool)
            .await?
            .rows_affected(),
        1
    );

    let after = attempt_history(&pool).await?;
    assert!(
        original.iter().all(|row| after.contains(row)),
        "seed history changed"
    );
    // Route history means the frozen snapshot columns: the Message, the Role,
    // the Binding, the generation and the override each attempt cites. Only
    // attempt 5's status/outcome may have advanced, which is the frozen
    // PENDING -> IN_FLIGHT -> FAILED chain, never a rewrite of the snapshot.
    let snapshot_of = |row: &str| {
        let parts: Vec<&str> = row.split('|').collect();
        [0, 1, 2, 3, 4, 5, 7, 8, 9, 10]
            .map(|index| parts[index])
            .join("|")
    };
    for before_row in &before_rebind {
        let fingerprint = snapshot_of(before_row);
        assert!(
            after.iter().any(|row| snapshot_of(row) == fingerprint),
            "an attempt's frozen snapshot changed: {before_row}"
        );
    }
    // The retry appended exactly one row and still cites override 3.
    assert_eq!(after.len(), before_rebind.len() + 1);

    // Each attempt still names the Role it really targeted, in order.
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
            hex_id(0x11),
            hex_id(0x11),
        ]
    );
    // The Message itself never followed the overrides.
    assert_eq!(
        query_scalar::<_, Vec<u8>>("SELECT to_role_id FROM messages WHERE id=?")
            .bind(hex_id(0x41))
            .fetch_one(&pool)
            .await?,
        hex_id(0x11)
    );

    // Overrides keep their own decision snapshots.
    let decisions = override_snapshot(&pool).await?;
    assert_eq!(decisions.len(), 3);
    assert!(
        decisions[0].contains(
            &hex_id(0x32)
                .iter()
                .map(|b| format!("{b:02X}"))
                .collect::<String>()
        )
    );
    assert!(
        decisions[1].contains(
            &hex_id(0x33)
                .iter()
                .map(|b| format!("{b:02X}"))
                .collect::<String>()
        )
    );
    assert!(
        decisions[2].contains(
            &hex_id(0x31)
                .iter()
                .map(|b| format!("{b:02X}"))
                .collect::<String>()
        )
    );

    // History may still not be rewritten, replaced or deleted. Each probe is
    // named, and the accepted marker names the refusal family rather than
    // pinning one guard: the point under test is that no history mutation lands,
    // not which of the 0017/0028 guards happened to fire first.
    refuse_named(
        &pool,
        &format!(
            "UPDATE delivery_attempts SET target_binding_id={} WHERE id={}",
            id(0x36),
            id(0xb3)
        ),
        "immutable",
        "rewrite the binding snapshot",
    )
    .await;
    refuse_named(
        &pool,
        &format!("DELETE FROM delivery_attempts WHERE id={}", id(0xb3)),
        "is retained with the Team",
        "delete an attempt",
    )
    .await;
    // Replacing a terminal attempt row is refused by 0017's write-once
    // insertion guard, which fires before the deduplication message can. Close
    // the live slot first so the refusal cannot be mistaken for the slot guard.
    fail_attempt(&pool, 0xb6).await?;
    refuse_named(
        &pool,
        &format!(
            "INSERT OR REPLACE INTO delivery_attempts SELECT * FROM delivery_attempts WHERE id={}",
            id(0xb3)
        ),
        "DOMAIN_PRECONDITION_FAILED",
        "replace a terminal attempt with no live attempt",
    )
    .await;
    refuse_named(
        &pool,
        &format!(
            "UPDATE delivery_attempts SET id={} WHERE id={}",
            id(0xffe0),
            id(0xb3)
        ),
        "immutable",
        "renumber an attempt in place",
    )
    .await;
    // Every probe refused. The only permitted difference is attempt 6, whose
    // live attempt this test deliberately drove to FAILED so the write-once
    // rule could be reached; no refused mutation changed anything.
    let final_history = attempt_history(&pool).await?;
    for row in &after {
        let is_live_attempt = row.contains(
            &hex_id(0xb6)
                .iter()
                .map(|b| format!("{b:02X}"))
                .collect::<String>(),
        );
        assert!(
            final_history.contains(row) || is_live_attempt,
            "history changed under a refused mutation: {row}"
        );
    }
    // And the deliberate transition itself is the frozen terminal chain.
    assert_eq!(
        final_history
            .iter()
            .filter(|row| row.contains(
                &hex_id(0xb6)
                    .iter()
                    .map(|b| format!("{b:02X}"))
                    .collect::<String>()
            ))
            .count(),
        1
    );
    Ok(())
}

#[tokio::test]
async fn override_numbering_gaps_and_wrong_predecessors_are_refused() -> TestResult {
    let pool = reassigned_then_rebound().await?;

    // Number 2 is the correct next number and every other field is valid, but
    // its declared predecessor role is not where the chain actually is:
    // override 1 ended at role B (0x12), so number 2 must start from 0x12.
    // Everything else -- Team, target Role, ACTIVE Binding at generation 2,
    // Event and request -- is genuinely valid, so only the chain rule can fire.
    let mut tx = pool.begin().await?;
    query(&format!("INSERT INTO command_idempotency(team_id,actor_fingerprint,command_name,idempotency_key,payload_hash,state,created_at,updated_at) VALUES({},X'{FINGERPRINT}','message.reassign','wrong-predecessor',zeroblob(32),'IN_PROGRESS',500,500)",id(1)))
        .execute(&mut *tx).await?;
    query(&event(0x76, 1, 0x41, 9)).execute(&mut *tx).await?;
    let wrong_predecessor = override_row(2, 0x13, 0x12, 0x34, "2", 0x76, "wrong-predecessor");
    let error = query(&wrong_predecessor)
        .execute(&mut *tx)
        .await
        .expect_err("a wrong predecessor must be refused");
    assert!(
        error.to_string().contains("predecessor"),
        "unrelated refusal: {error}"
    );
    tx.rollback().await?;

    // Number 3 while only 1 exists: the sequence must be contiguous from 1.
    // The predecessor role is correct here (0x12) and the target is a real
    // ACTIVE binding, so only the numbering rule can fire.
    let mut tx = pool.begin().await?;
    query(&format!("INSERT INTO command_idempotency(team_id,actor_fingerprint,command_name,idempotency_key,payload_hash,state,created_at,updated_at) VALUES({},X'{FINGERPRINT}','message.reassign','gap',zeroblob(32),'IN_PROGRESS',500,500)",id(1)))
        .execute(&mut *tx).await?;
    query(&event(0x77, 1, 0x41, 9)).execute(&mut *tx).await?;
    let gap = override_row(3, 0x12, 0x11, 0x31, "1", 0x77, "gap");
    let error = query(&gap)
        .execute(&mut *tx)
        .await
        .expect_err("a sequence gap must be refused");
    // SQLite evaluates the two chain guards in reverse creation order, so a
    // gap is reported by the predecessor guard first. Both are the same rule
    // family and both refuse the write; the sequence is not silently accepted.
    assert!(
        error.to_string().contains("contiguous") || error.to_string().contains("predecessor"),
        "unrelated refusal: {error}"
    );
    tx.rollback().await?;

    // Both refusals left nothing behind.
    assert_eq!(
        query_scalar::<_, i64>("SELECT COUNT(*) FROM message_route_overrides")
            .fetch_one(&pool)
            .await?,
        1
    );

    // Numbering and chain are checked on a Message that has no override yet, so
    // the first-override rule is isolated from any existing chain. Message
    // 0x43 addresses role C and has no override at all.
    let mut tx = pool.begin().await?;
    query(&format!("INSERT INTO command_idempotency(team_id,actor_fingerprint,command_name,idempotency_key,payload_hash,state,created_at,updated_at) VALUES({},X'{FINGERPRINT}','message.reassign','first-is-two',zeroblob(32),'IN_PROGRESS',500,500)",id(1)))
        .execute(&mut *tx).await?;
    query(&event(0x78, 1, 0x43, 4)).execute(&mut *tx).await?;
    let first_is_two = override_row(2, 0x13, 0x11, 0x31, "1", 0x78, "first-is-two")
        .replace(&id(0x41), &id(0x43))
        .replace(&id(0x92), &id(0x98));
    let error = query(&first_is_two)
        .execute(&mut *tx)
        .await
        .expect_err("the first override must be numbered 1");
    assert!(
        error.to_string().contains("contiguous") || error.to_string().contains("predecessor"),
        "unrelated refusal: {error}"
    );
    tx.rollback().await?;

    // Positive control: the correct continuation still works.
    reassignment(&pool, 2, 0x12, 0x13, 0x33).await?;
    assert_eq!(
        query_scalar::<_, i64>("SELECT COUNT(*) FROM message_route_overrides WHERE message_id=?",)
            .bind(hex_id(0x41))
            .fetch_one(&pool)
            .await?,
        2
    );
    Ok(())
}

// ---------------------------------------------------------------------------
// 4. Forward upgrade from 29 and from 32 preserves every column value.
// ---------------------------------------------------------------------------

#[tokio::test]
async fn upgrade_from_29_preserves_attempts_and_keeps_new_columns_null() -> TestResult {
    let pool = historical_database().await?;
    let before = all_rows(&pool).await?;
    // The 29-era attempt rows, read without the columns 0030 later appends.
    let attempts = query_scalar::<_, String>(
        "SELECT hex(id) || '|' || hex(message_id) || '|' || attempt_number || '|'
            || hex(target_role_id) || '|' || hex(target_binding_id) || '|'
            || target_binding_generation || '|' || status || '|' || created_at
         FROM delivery_attempts ORDER BY id",
    )
    .fetch_all(&pool)
    .await?;
    migrate(&pool).await?;

    assert_eq!(
        query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM _sqlx_migrations WHERE version=33 AND success=TRUE"
        )
        .fetch_one(&pool)
        .await?,
        1
    );
    // Attempts from before 0030 adopted no override; that reading is unchanged.
    assert_eq!(
        query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM delivery_attempts WHERE route_override_id IS NOT NULL OR route_override_number IS NOT NULL"
        )
        .fetch_one(&pool)
        .await?,
        0
    );
    // Every legacy attempt still names the same Role, Binding and generation.
    let after_attempts = query_scalar::<_, String>(
        "SELECT hex(id) || '|' || hex(message_id) || '|' || attempt_number || '|'
            || hex(target_role_id) || '|' || hex(target_binding_id) || '|'
            || target_binding_generation || '|' || status || '|' || created_at
         FROM delivery_attempts ORDER BY id",
    )
    .fetch_all(&pool)
    .await?;
    assert_eq!(after_attempts, attempts, "legacy attempt columns changed");

    let after = all_rows(&pool).await?;
    for (table, rows) in &before {
        if table == "_sqlx_migrations" {
            continue;
        }
        let actual = &after
            .iter()
            .find(|(name, _)| name == table)
            .expect("old table retained")
            .1;
        assert_eq!(actual.len(), rows.len(), "row count of old {table} changed");
        for (after_row, before_row) in actual.iter().zip(rows.iter()) {
            // Every migration 0030-0033 only appended nullable columns, so the
            // 29-era row must remain an exact prefix of the upgraded row and
            // everything appended must read NULL.
            assert!(
                after_row.starts_with(before_row),
                "old {table} row changed:\n  before: {before_row}\n  after:  {after_row}"
            );
            let appended = after_row.len() - before_row.len();
            assert!(
                appended == 0 || after_row[before_row.len()..] == "|NULL".repeat(appended / 5),
                "appended {table} columns must be NULL: {after_row}"
            );
        }
    }

    // Re-running is inert.
    migrate(&pool).await?;
    assert_eq!(
        all_rows(&pool).await?,
        after,
        "re-running 0033 must not change anything"
    );
    Ok(())
}

#[tokio::test]
async fn upgrade_from_32_preserves_the_existing_override_decision_snapshot() -> TestResult {
    let pool = historical_database().await?;
    loader_through(32).await?.run(&pool).await?;
    assert_eq!(
        query_scalar::<_, i64>("SELECT MAX(version) FROM _sqlx_migrations")
            .fetch_one(&pool)
            .await?,
        32
    );
    reassignment(&pool, 1, 0x11, 0x12, 0x32).await?;
    fail_attempt(&pool, 0xb3).await?;
    let attempts = attempt_history(&pool).await?;
    let decisions = override_snapshot(&pool).await?;
    let message = message_row(&pool).await?;
    let full_before = all_rows(&pool).await?;

    loader_through(CANDIDATE).await?.run(&pool).await?;
    assert_eq!(
        query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM _sqlx_migrations WHERE version=33 AND success=TRUE"
        )
        .fetch_one(&pool)
        .await?,
        1
    );
    assert_eq!(
        override_snapshot(&pool).await?,
        decisions,
        "override decision snapshot changed"
    );
    assert_eq!(
        attempt_history(&pool).await?,
        attempts,
        "existing attempts changed"
    );
    assert_eq!(message_row(&pool).await?, message);
    let full_after = all_rows(&pool).await?;
    for (table, rows) in &full_before {
        let actual = &full_after.iter().find(|(name, _)| name == table).unwrap().1;
        if table == "_sqlx_migrations" {
            assert!(rows.iter().all(|row| actual.contains(row)));
            assert_eq!(actual.len(), rows.len() + 1);
        } else {
            assert_eq!(actual, rows, "32 -> 33 changed a complete old row: {table}");
        }
    }
    loader_through(CANDIDATE).await?.run(&pool).await?;
    assert_eq!(all_rows(&pool).await?, full_after);
    assert_eq!(
        override_snapshot(&pool).await?,
        decisions,
        "re-run changed history"
    );
    Ok(())
}

// ---------------------------------------------------------------------------
// 5. The DROP/CREATE window is atomic under a real SQLx failure.
// ---------------------------------------------------------------------------

#[tokio::test]
async fn a_failure_between_the_drop_and_the_create_rolls_the_whole_migration_back() -> TestResult {
    // Both a 29-era and a 32-era database reach the 32-era schema through the
    // published chain, so the only difference between the runs is how much of
    // the published history had to be walked to get there.
    for start in [29, 32] {
        let pool = historical_database().await?;
        loader_through(start).await?.run(&pool).await?;
        loader_through(32).await?.run(&pool).await?;
        assert_eq!(
            query_scalar::<_, i64>("SELECT MAX(version) FROM _sqlx_migrations")
                .fetch_one(&pool)
                .await?,
            32,
            "the rollback baseline must be the 32-era schema"
        );

        // Hold the pre-candidate schema and rows, then present the candidate as
        // a real SQLx migration that fails between its DROP and its CREATE.
        let old_schema: Vec<String> =
            query_scalar("SELECT type || '|' || name || '|' || coalesce(sql,'') FROM sqlite_master ORDER BY type,name")
                .fetch_all(&pool)
                .await?;
        let old_rows = all_rows(&pool).await?;

        let mut migrator = loader().await?;
        let row = migrator
            .migrations
            .to_mut()
            .iter_mut()
            .find(|m| m.version == CANDIDATE)
            .expect("candidate 0033 must be registered");
        assert!(
            !row.no_tx,
            "the replacement trigger must swap in one transaction"
        );
        let marker = "DROP TRIGGER delivery_attempts_route_override_insert;";
        assert_eq!(row.sql.matches(marker).count(), 1);
        let injected = row.sql.replace(
            marker,
            &format!("{marker}\nSELECT * FROM migration_0033_injected_failure;"),
        );
        *row = Migration::new(
            CANDIDATE,
            row.description.clone(),
            MigrationType::Simple,
            injected.into(),
            false,
        );
        let error = migrator
            .run(&pool)
            .await
            .expect_err("the injected failure must abort the migration");
        assert!(
            error
                .to_string()
                .contains("migration_0033_injected_failure"),
            "unrelated failure: {error}"
        );

        // The trigger that 0033 removes is still present, the replacement is
        // not, the ledger gained no row, and nothing else moved.
        let new_schema: Vec<String> =
            query_scalar("SELECT type || '|' || name || '|' || coalesce(sql,'') FROM sqlite_master ORDER BY type,name")
                .fetch_all(&pool)
                .await?;
        assert_eq!(new_schema, old_schema, "from {start}: schema drift");
        assert_eq!(
            query_scalar::<_, i64>(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='trigger' AND name='delivery_attempts_route_override_insert'"
            )
            .fetch_one(&pool)
            .await?,
            1,
            "the original guard must still be in place"
        );
        assert_eq!(
            all_rows(&pool).await?,
            old_rows,
            "from {start}: history/ledger drift"
        );

        // The unchanged migration then succeeds and the guard is the new one.
        migrate(&pool).await?;
        assert_eq!(
            query_scalar::<_, i64>(
                "SELECT COUNT(*) FROM _sqlx_migrations WHERE version=33 AND success=TRUE"
            )
            .fetch_one(&pool)
            .await?,
            1
        );
        assert!(
            query_scalar::<_, String>(
                "SELECT sql FROM sqlite_master WHERE type='trigger' AND name='delivery_attempts_route_override_insert'"
            )
            .fetch_one(&pool)
            .await?
            .contains("NEW.target_binding_generation IS NULL"),
            "the replaced guard must be the candidate one"
        );
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// 6. Loader compatibility boundary. These say what a migration set does with a
//    database; they do not claim any old binary can downgrade.
// ---------------------------------------------------------------------------

#[tokio::test]
async fn loaders_report_their_first_missing_version_instead_of_downgrading() -> TestResult {
    let pool = reassigned_then_rebound().await?;

    // A 29-only set against the candidate schema.
    let error = loader_through(29)
        .await?
        .run(&pool)
        .await
        .expect_err("a 29-only set must refuse the candidate schema");
    assert!(
        matches!(error, sqlx_core::migrate::MigrateError::VersionMissing(30)),
        "unexpected 29-only result: {error}"
    );

    // 30-only and 31-only sets against the candidate schema.
    for (limit, missing) in [(30, 31), (31, 32)] {
        let error = loader_through(limit)
            .await?
            .run(&pool)
            .await
            .expect_err("a truncated set must refuse the candidate schema");
        match error {
            sqlx_core::migrate::MigrateError::VersionMissing(version) => {
                assert_eq!(
                    version, missing,
                    "unexpected {limit}-only first missing version"
                )
            }
            other => panic!("unexpected {limit}-only result: {other}"),
        }
    }

    // A 32-only set sees exactly the candidate version as missing, which is the
    // forward step a released binary would have to make.
    let error = loader_through(32)
        .await?
        .run(&pool)
        .await
        .expect_err("a 32-only set must refuse the candidate schema");
    assert!(
        matches!(
            error,
            sqlx_core::migrate::MigrateError::VersionMissing(CANDIDATE)
        ),
        "unexpected 32-only result: {error}"
    );
    Ok(())
}

#[tokio::test]
async fn idempotency_key_cannot_be_reused_by_an_otherwise_valid_route() -> TestResult {
    let pool = reassigned_then_rebound().await?;
    let before = all_rows(&pool).await?;
    let mut tx = pool.begin().await?;
    // New override ID/number, correct B -> C predecessor, ACTIVE C Binding,
    // and a committed-in-this-transaction Event with matching Message/request.
    // Only the already used four-tuple key is invalid.
    query(&event(0x79, 1, 0x41, 8)).execute(&mut *tx).await?;
    let invalid = override_row(2, 0x12, 0x13, 0x33, "1", 0x79, "hc01-route-1");
    let error = query(&invalid)
        .execute(&mut *tx)
        .await
        .expect_err("HC01_IDEMPOTENCY_PREEMPTION_ACCEPTED");
    assert!(
        error.to_string().contains("written once"),
        "unrelated refusal: {error}"
    );
    tx.rollback().await?;
    assert_eq!(all_rows(&pool).await?, before);
    // Otherwise identical route under a fresh four-tuple succeeds.
    let mut tx = pool.begin().await?;
    query(&format!("INSERT INTO command_idempotency(team_id,actor_fingerprint,command_name,idempotency_key,payload_hash,state,created_at,updated_at) VALUES({},X'{FINGERPRINT}','message.reassign','fresh-control',zeroblob(32),'IN_PROGRESS',500,500)", id(1)))
        .execute(&mut *tx).await?;
    query(&event(0x7a, 1, 0x41, 8)).execute(&mut *tx).await?;
    let valid = override_row(2, 0x12, 0x13, 0x33, "1", 0x7a, "fresh-control");
    assert_eq!(query(&valid).execute(&mut *tx).await?.rows_affected(), 1);
    tx.rollback().await?;
    assert_eq!(all_rows(&pool).await?, before);
    Ok(())
}
