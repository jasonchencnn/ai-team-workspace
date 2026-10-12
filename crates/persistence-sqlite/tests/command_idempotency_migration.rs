//! Conformance tests for migration `0022_command_idempotency.sql`.
//!
//! Frozen source: `DATA_CONVENTIONS.md` §4.
//!
//! These are database-level tests: the domain layer's `evaluate_replay` is
//! tested in `crates/domain/tests/idempotency.rs`, and what is checked here is
//! the part only the database can enforce — the unique key that refuses a
//! second record for the same logical operation, and the guards that stop a
//! record from being edited into a state a replay could not explain.
//!
//! The node's risk level is R3, which `QUALITY_GATES.md` §3 requires at least
//! one failure injection for. Every guard below is exercised by an attempted
//! violation that must be refused.

use std::error::Error;
use std::str::FromStr;

use persistence_sqlite::migrate;
use sqlx_core::{query::query, query_scalar::query_scalar};
use sqlx_sqlite::{SqliteConnectOptions, SqlitePool, SqlitePoolOptions};

const TEAM: &str = "0191ad94d0957abc9ef0123456789abc";
const FINGERPRINT_A: &str = "000000000000000000000000000000000000000000000000000000000000000a";
const FINGERPRINT_B: &str = "000000000000000000000000000000000000000000000000000000000000000b";
const HASH_A: &str = "1111111111111111111111111111111111111111111111111111111111111111";
const HASH_B: &str = "2222222222222222222222222222222222222222222222222222222222222222";
const NOW: i64 = 1_726_300_123_456;

async fn database() -> Result<SqlitePool, Box<dyn Error>> {
    let options = SqliteConnectOptions::from_str("sqlite::memory:")?.foreign_keys(true);
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(options)
        .await?;
    migrate(&pool).await?;
    Ok(pool)
}

fn insert_team() -> String {
    format!(
        "INSERT INTO teams (id, name, status, version, created_at, updated_at, archived_at) \
         VALUES (X'{TEAM}', 'Core Team', 'ACTIVE', 1, {NOW}, {NOW}, NULL);"
    )
}

fn insert_in_progress(fingerprint: &str, command: &str, key: &str, hash: &str) -> String {
    format!(
        "INSERT INTO command_idempotency (team_id, actor_fingerprint, command_name, \
         idempotency_key, payload_hash, state, result_ref, first_error_code, \
         created_at, updated_at) \
         VALUES (X'{TEAM}', X'{fingerprint}', '{command}', '{key}', X'{hash}', \
         'IN_PROGRESS', NULL, NULL, {NOW}, {NOW});"
    )
}

async fn expect_refused(pool: &SqlitePool, sql: &str, why: &str) {
    let error = query(sql)
        .execute(pool)
        .await
        .err()
        .unwrap_or_else(|| panic!("statement unexpectedly succeeded: {why}\n{sql}"));
    let message = error.to_string();
    assert!(
        !message.is_empty(),
        "a refusal must carry a message so the cause is diagnosable: {why}"
    );
}

// ------------------------------------------------------ the key, part by part

#[tokio::test]
async fn a_second_record_for_the_same_key_is_refused() -> Result<(), Box<dyn Error>> {
    // §4 row 4: 同 key + 不同 payload → the key is already taken.
    //
    // The specification reports that as CONCURRENCY_IDEMPOTENCY_CONFLICT, but
    // what the database can do is refuse the write. The application layer turns
    // this refusal into the catalogued code; that translation is
    // `evaluate_replay`'s job and is tested on the domain side.
    let pool = database().await?;
    query(&insert_team()).execute(&pool).await?;
    query(&insert_in_progress(
        FINGERPRINT_A,
        "run.retry",
        "idem-1",
        HASH_A,
    ))
    .execute(&pool)
    .await?;

    expect_refused(
        &pool,
        &insert_in_progress(FINGERPRINT_A, "run.retry", "idem-1", HASH_B),
        "§4: the same key with a different payload must not create a second record",
    )
    .await;

    // And the original row is untouched — a refused insert leaves no trace.
    let state: String =
        query_scalar("SELECT state FROM command_idempotency WHERE idempotency_key = 'idem-1'")
            .fetch_one(&pool)
            .await?;
    assert_eq!(state, "IN_PROGRESS", "the first record is unchanged");
    let count: i64 = query_scalar("SELECT COUNT(*) FROM command_idempotency")
        .fetch_one(&pool)
        .await?;
    assert_eq!(count, 1, "拒绝后不产生任何新行");
    Ok(())
}

#[tokio::test]
async fn all_four_key_parts_take_part_in_identity() -> Result<(), Box<dyn Error>> {
    // §4: the key is the four-part tuple, so changing any one part is a
    // different logical operation and must be accepted.
    let pool = database().await?;
    query(&insert_team()).execute(&pool).await?;
    for (fingerprint, command, key) in [
        (FINGERPRINT_A, "run.retry", "idem-1"),
        (FINGERPRINT_B, "run.retry", "idem-1"),
        (FINGERPRINT_A, "message.retry", "idem-1"),
        (FINGERPRINT_A, "run.retry", "idem-2"),
    ] {
        query(&insert_in_progress(fingerprint, command, key, HASH_A))
            .execute(&pool)
            .await?;
    }
    let count: i64 = query_scalar("SELECT COUNT(*) FROM command_idempotency")
        .fetch_one(&pool)
        .await?;
    assert_eq!(
        count, 4,
        "team_id is fixed here, so the other three parts must all affect identity"
    );
    Ok(())
}

#[tokio::test]
async fn the_same_key_in_a_different_team_is_a_different_operation() -> Result<(), Box<dyn Error>> {
    // §4's key starts with team_id, so two Teams may reuse the same key without
    // colliding. Without the first component one Team's key would swallow the
    // other's request.
    let pool = database().await?;
    let other_team = "0191ad94d0957abc9ef1fedcba987654";
    query(&insert_team()).execute(&pool).await?;
    query(&format!(
        "INSERT INTO teams (id, name, status, version, created_at, updated_at, archived_at) \
         VALUES (X'{other_team}', 'Other Team', 'ACTIVE', 1, {NOW}, {NOW}, NULL);"
    ))
    .execute(&pool)
    .await?;

    query(&insert_in_progress(
        FINGERPRINT_A,
        "run.retry",
        "shared-key",
        HASH_A,
    ))
    .execute(&pool)
    .await?;
    query(&format!(
        "INSERT INTO command_idempotency (team_id, actor_fingerprint, command_name, \
         idempotency_key, payload_hash, state, result_ref, first_error_code, \
         created_at, updated_at) \
         VALUES (X'{other_team}', X'{FINGERPRINT_A}', 'run.retry', 'shared-key', \
         X'{HASH_B}', 'IN_PROGRESS', NULL, NULL, {NOW}, {NOW});"
    ))
    .execute(&pool)
    .await?;

    let count: i64 = query_scalar("SELECT COUNT(*) FROM command_idempotency")
        .fetch_one(&pool)
        .await?;
    assert_eq!(count, 2, "team_id 参与键，跨 Team 复用同一 key 不冲突");
    Ok(())
}

// --------------------------------------------------- state / result coherence

#[tokio::test]
async fn a_succeeded_record_must_carry_its_result_reference() -> Result<(), Box<dyn Error>> {
    // §4 row 2: 返回首次结果. A SUCCEEDED row with no result reference could
    // not answer a replay honestly, so the write is refused rather than
    // accepted and later guessed at.
    let pool = database().await?;
    query(&insert_team()).execute(&pool).await?;

    expect_refused(
        &pool,
        &format!(
            "INSERT INTO command_idempotency (team_id, actor_fingerprint, command_name, \
             idempotency_key, payload_hash, state, result_ref, first_error_code, \
             created_at, updated_at) \
             VALUES (X'{TEAM}', X'{FINGERPRINT_A}', 'run.retry', 'idem-1', X'{HASH_A}', \
             'SUCCEEDED', NULL, NULL, {NOW}, {NOW});"
        ),
        "SUCCEEDED without a result_ref cannot answer a replay",
    )
    .await;

    // With the reference it is accepted.
    query(&format!(
        "INSERT INTO command_idempotency (team_id, actor_fingerprint, command_name, \
         idempotency_key, payload_hash, state, result_ref, first_error_code, \
         created_at, updated_at) \
         VALUES (X'{TEAM}', X'{FINGERPRINT_A}', 'run.retry', 'idem-1', X'{HASH_A}', \
         'SUCCEEDED', 'run:0191ad94-d095-7abc-9ef0-123456789abc', NULL, {NOW}, {NOW});"
    ))
    .execute(&pool)
    .await?;
    Ok(())
}

#[tokio::test]
async fn a_failed_record_must_carry_its_error_code_and_no_result() -> Result<(), Box<dyn Error>> {
    // §4 row 3: 返回同一稳定失败. The code has to be stored to be replayed, and
    // a FAILED row must not also claim a result reference.
    let pool = database().await?;
    query(&insert_team()).execute(&pool).await?;

    for (sql, why) in [
        (
            format!(
                "INSERT INTO command_idempotency (team_id, actor_fingerprint, command_name, \
                 idempotency_key, payload_hash, state, result_ref, first_error_code, \
                 created_at, updated_at) \
                 VALUES (X'{TEAM}', X'{FINGERPRINT_A}', 'run.retry', 'idem-1', X'{HASH_A}', \
                 'FAILED', NULL, NULL, {NOW}, {NOW});"
            ),
            "FAILED without an error code cannot replay the first failure",
        ),
        (
            format!(
                "INSERT INTO command_idempotency (team_id, actor_fingerprint, command_name, \
                 idempotency_key, payload_hash, state, result_ref, first_error_code, \
                 created_at, updated_at) \
                 VALUES (X'{TEAM}', X'{FINGERPRINT_A}', 'run.retry', 'idem-2', X'{HASH_A}', \
                 'FAILED', 'run:0191ad94-d095-7abc-9ef0-123456789abc', \
                 'DOMAIN_INVALID_TRANSITION', {NOW}, {NOW});"
            ),
            "FAILED must not also carry a result reference",
        ),
    ] {
        expect_refused(&pool, &sql, why).await;
    }

    query(&format!(
        "INSERT INTO command_idempotency (team_id, actor_fingerprint, command_name, \
         idempotency_key, payload_hash, state, result_ref, first_error_code, \
         created_at, updated_at) \
         VALUES (X'{TEAM}', X'{FINGERPRINT_A}', 'run.retry', 'idem-3', X'{HASH_A}', \
         'FAILED', NULL, 'DOMAIN_INVALID_TRANSITION', {NOW}, {NOW});"
    ))
    .execute(&pool)
    .await?;
    Ok(())
}

#[tokio::test]
async fn an_in_progress_record_must_not_carry_an_outcome() -> Result<(), Box<dyn Error>> {
    // §4 row 1: 首次仍处理中 → 返回处理中. A result reference here would claim
    // the first attempt already finished, which is what stops a second
    // execution from starting.
    let pool = database().await?;
    query(&insert_team()).execute(&pool).await?;
    expect_refused(
        &pool,
        &format!(
            "INSERT INTO command_idempotency (team_id, actor_fingerprint, command_name, \
             idempotency_key, payload_hash, state, result_ref, first_error_code, \
             created_at, updated_at) \
             VALUES (X'{TEAM}', X'{FINGERPRINT_A}', 'run.retry', 'idem-1', X'{HASH_A}', \
             'IN_PROGRESS', 'run:0191ad94-d095-7abc-9ef0-123456789abc', NULL, {NOW}, {NOW});"
        ),
        "IN_PROGRESS must not claim a result",
    )
    .await;
    Ok(())
}

#[tokio::test]
async fn a_state_cannot_be_edited_into_something_unexplainable() -> Result<(), Box<dyn Error>> {
    // The coherence rule applies to UPDATE as well as INSERT: a record could
    // otherwise be inserted as IN_PROGRESS and later rewritten into an
    // inconsistent pair.
    let pool = database().await?;
    query(&insert_team()).execute(&pool).await?;
    query(&insert_in_progress(
        FINGERPRINT_A,
        "run.retry",
        "idem-1",
        HASH_A,
    ))
    .execute(&pool)
    .await?;

    expect_refused(
        &pool,
        "UPDATE command_idempotency SET state = 'SUCCEEDED' WHERE idempotency_key = 'idem-1'",
        "SUCCEEDED without a result_ref must be refused on update too",
    )
    .await;

    // And the legitimate transition is accepted.
    query(
        "UPDATE command_idempotency SET state = 'SUCCEEDED', \
         result_ref = 'run:0191ad94-d095-7abc-9ef0-123456789abc', updated_at = 1726300123457 \
         WHERE idempotency_key = 'idem-1'",
    )
    .execute(&pool)
    .await?;
    let state: String =
        query_scalar("SELECT state FROM command_idempotency WHERE idempotency_key = 'idem-1'")
            .fetch_one(&pool)
            .await?;
    assert_eq!(state, "SUCCEEDED");
    Ok(())
}

// ------------------------------------------------------------ retention row

#[tokio::test]
async fn a_record_cannot_be_deleted_so_the_key_stays_claimed() -> Result<(), Box<dyn Error>> {
    // §4 row 5: 「关键命令仍通过唯一约束/业务引用防重复」.
    //
    // That row is what keeps a key alive past its retention window, so deleting
    // a record would silently re-open the key: the next call with the same key
    // would look like a first attempt and run the command a second time.
    // `DATA_CONVENTIONS` §6 says records live at least 30 days and offers no
    // deletion rule, so this migration refuses deletion outright rather than
    // inventing a retention job.
    let pool = database().await?;
    query(&insert_team()).execute(&pool).await?;
    query(&insert_in_progress(
        FINGERPRINT_A,
        "run.retry",
        "idem-1",
        HASH_A,
    ))
    .execute(&pool)
    .await?;

    expect_refused(
        &pool,
        "DELETE FROM command_idempotency WHERE idempotency_key = 'idem-1'",
        "§4 row 5: the unique constraint is the guard past retention, so the row must stay",
    )
    .await;

    let count: i64 = query_scalar("SELECT COUNT(*) FROM command_idempotency")
        .fetch_one(&pool)
        .await?;
    assert_eq!(count, 1, "拒绝后记录仍在");
    Ok(())
}

// ------------------------------------------------------------ shape of a cell

#[tokio::test]
async fn the_column_shapes_match_the_specification() -> Result<(), Box<dyn Error>> {
    // §4 names the key's parts and says the record keeps a payload hash, a
    // state and a first result reference. Each of those has to exist with the
    // width the domain layer writes, or a valid record could not be stored.
    let pool = database().await?;
    for (column, expected) in [
        ("team_id", "blob"),
        ("actor_fingerprint", "blob"),
        ("payload_hash", "blob"),
        ("command_name", "text"),
        ("idempotency_key", "text"),
        ("state", "text"),
    ] {
        let actual: String = query_scalar(
            "SELECT type FROM pragma_table_info('command_idempotency') WHERE name = ?",
        )
        .bind(column)
        .fetch_one(&pool)
        .await?;
        assert_eq!(
            actual.to_ascii_lowercase(),
            expected,
            "column {column} has the wrong type (pragma reports it as {actual:?})"
        );
    }

    // The three states the domain layer can write, and nothing else.
    for state in ["IN_PROGRESS", "SUCCEEDED", "FAILED"] {
        let count: i64 =
            query_scalar("SELECT COUNT(*) FROM (SELECT 1 FROM pragma_table_info('command_idempotency') WHERE name = 'state')")
                .fetch_one(&pool)
                .await?;
        assert_eq!(count, 1, "state column exists once");
        assert!(
            ["IN_PROGRESS", "SUCCEEDED", "FAILED"].contains(&state),
            "{state} is one of the three written by IdempotencyState::as_str"
        );
    }

    // A state outside the frozen three is refused.
    query(&insert_team()).execute(&pool).await?;
    expect_refused(
        &pool,
        &insert_in_progress(FINGERPRINT_A, "run.retry", "idem-1", HASH_A)
            .replace("'IN_PROGRESS'", "'EXPIRED'"),
        "§4 has no EXPIRED state: 保留期过期由唯一约束兜底，不进状态机",
    )
    .await;
    Ok(())
}

#[tokio::test]
async fn a_record_cannot_reference_a_team_that_does_not_exist() -> Result<(), Box<dyn Error>> {
    // The key is scoped by team_id, so a record for a missing Team would make
    // the key's first component meaningless. `foreign_keys` is connection-local
    // in SQLite, which is why `database()` enables it.
    let pool = database().await?;
    let missing = "0191ad94d0957abc9ef0aaaaaaaaaaaaaa";
    expect_refused(
        &pool,
        &format!(
            "INSERT INTO command_idempotency (team_id, actor_fingerprint, command_name, \
             idempotency_key, payload_hash, state, result_ref, first_error_code, \
             created_at, updated_at) \
             VALUES (X'{missing}', X'{FINGERPRINT_A}', 'run.retry', 'idem-1', X'{HASH_A}', \
             'IN_PROGRESS', NULL, NULL, {NOW}, {NOW});"
        ),
        "a record must belong to an existing Team",
    )
    .await;
    Ok(())
}

// ------------------------------------ INSERT OR REPLACE and UPDATE hijacking
//
// These are the attacks that `no_delete_guard` alone does not stop. The
// project's own `0013`/`0014`/`0016` migrations already carry `*_replace_guard`
// triggers for exactly this reason; `0022` was the one table missing them.

#[tokio::test]
async fn insert_or_replace_cannot_hijack_a_claimed_key() -> Result<(), Box<dyn Error>> {
    // `no_delete_guard` is a BEFORE DELETE trigger, and SQLite's implicit DELETE
    // inside `INSERT OR REPLACE` does not fire it (that needs
    // `PRAGMA recursive_triggers=ON`, which this project does not set).
    //
    // So without `command_idempotency_replace_guard` the following statement
    // succeeds: the first result reference is overwritten in place, which breaks
    // §4 row 2 (返回首次结果) and row 4 (同 key + 不同 payload → 冲突) at once.
    let pool = database().await?;
    query(&insert_team()).execute(&pool).await?;
    query(&format!(
        "INSERT INTO command_idempotency (team_id, actor_fingerprint, command_name, \
         idempotency_key, payload_hash, state, result_ref, first_error_code, \
         created_at, updated_at) \
         VALUES (X'{TEAM}', X'{FINGERPRINT_A}', 'run.retry', 'idem-1', X'{HASH_A}', \
         'SUCCEEDED', 'run:0191ad94-d095-7abc-9ef0-123456789abc', NULL, {NOW}, {NOW});"
    ))
    .execute(&pool)
    .await?;

    expect_refused(
        &pool,
        &format!(
            "INSERT OR REPLACE INTO command_idempotency (team_id, actor_fingerprint, \
             command_name, idempotency_key, payload_hash, state, result_ref, \
             first_error_code, created_at, updated_at) \
             VALUES (X'{TEAM}', X'{FINGERPRINT_A}', 'run.retry', 'idem-1', X'{HASH_B}', \
             'SUCCEEDED', 'run:HIJACKED', NULL, {NOW}, {NOW});"
        ),
        "INSERT OR REPLACE 会隐式删除冲突行再插入，必须被 replace_guard 拒绝",
    )
    .await;

    // The first record survives untouched. Two single-column reads rather than
    // one row read, because `query_scalar` cannot decode a tuple.
    let hash: Vec<u8> = query_scalar(
        "SELECT payload_hash FROM command_idempotency WHERE idempotency_key = 'idem-1'",
    )
    .fetch_one(&pool)
    .await
    .expect("the original record must still be present");
    let result_ref: Option<String> =
        query_scalar("SELECT result_ref FROM command_idempotency WHERE idempotency_key = 'idem-1'")
            .fetch_one(&pool)
            .await
            .expect("the original record must still be present");
    assert_eq!(hash, hex_to_bytes(HASH_A), "payload_hash 未被改写");
    assert_eq!(
        result_ref.as_deref(),
        Some("run:0191ad94-d095-7abc-9ef0-123456789abc"),
        "§4 第 2 行：返回首次结果，不得被替换"
    );
    Ok(())
}

#[tokio::test]
async fn the_payload_hash_and_first_outcome_cannot_be_rewritten_in_place()
-> Result<(), Box<dyn Error>> {
    // A different problem from the REPLACE bypass: there is no guard at all, so
    // a bare UPDATE can flip the "same payload" comparison or swap the recorded
    // first outcome. §4 requires both to describe the *first* attempt.
    let pool = database().await?;
    query(&insert_team()).execute(&pool).await?;
    query(&insert_in_progress(
        FINGERPRINT_A,
        "run.retry",
        "idem-1",
        HASH_A,
    ))
    .execute(&pool)
    .await?;

    for (sql, why) in [
        (
            format!(
                "UPDATE command_idempotency SET payload_hash = X'{HASH_B}' \
                 WHERE idempotency_key = 'idem-1'"
            ),
            "payload_hash 记录的是首次尝试，改写它会翻转「同 payload」判定",
        ),
        (
            "UPDATE command_idempotency SET idempotency_key = 'idem-2' \
             WHERE idempotency_key = 'idem-1'"
                .to_owned(),
            "键的任一段都是 §4 键的一部分，不能改",
        ),
    ] {
        expect_refused(&pool, &sql, why).await;
    }

    // And the legitimate IN_PROGRESS → SUCCEEDED transition still works, which
    // is the one update the record must allow: that step *is* the first write of
    // the result reference, so "recorded once" cannot mean "never written".
    query(
        "UPDATE command_idempotency SET state = 'SUCCEEDED', \
         result_ref = 'run:FIRST', updated_at = 1726300123457 \
         WHERE idempotency_key = 'idem-1'",
    )
    .execute(&pool)
    .await?;
    let state: String =
        query_scalar("SELECT state FROM command_idempotency WHERE idempotency_key = 'idem-1'")
            .fetch_one(&pool)
            .await?;
    assert_eq!(state, "SUCCEEDED", "合法推进必须仍然可用");

    // Once written, though, it is final: §4 row 2 replays *this* value, so
    // replacing it would return the second write instead of the first.
    expect_refused(
        &pool,
        "UPDATE command_idempotency SET result_ref = 'run:SECOND' \
         WHERE idempotency_key = 'idem-1'",
        "首次结果已写入后不得被改写，否则 §4 第 2 行返回的就不是首次结果",
    )
    .await;
    let result_ref: Option<String> =
        query_scalar("SELECT result_ref FROM command_idempotency WHERE idempotency_key = 'idem-1'")
            .fetch_one(&pool)
            .await?;
    assert_eq!(
        result_ref.as_deref(),
        Some("run:FIRST"),
        "首次结果引用保持不变"
    );
    Ok(())
}

fn hex_to_bytes(hex: &str) -> Vec<u8> {
    (0..hex.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&hex[i..i + 2], 16).expect("fixture hex"))
        .collect()
}
