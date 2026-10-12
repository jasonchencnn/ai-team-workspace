//! MM-R02 follow-up FC-02 (audit MR-03): six retained objects could still be
//! destroyed by a storage-level `DELETE` or `INSERT OR REPLACE`.
//!
//! The counterexample this file answers was reproduced by the complete retention
//! census in
//! `docs/reviews/evidence/minimax-return-audit-2026-10-08/storage/RETENTION_INVENTORY.md`:
//! with a row that no `events` row and no `"references"` row names, `DELETE`
//! succeeded and each really went `1 -> 0`; and `INSERT OR REPLACE` rewrote the
//! recorded value. Those rows are outside the five tables `0026` was given, so
//! `0026` closed its own write set and left these six.
//!
//! | table             | census result                                        |
//! |-------------------|------------------------------------------------------|
//! | `teams`           | `1 -> 0`; replace changed `name`                     |
//! | `roles`           | `1 -> 0`; replace changed `name`                     |
//! | `tasks`           | `1 -> 0`; replace changed `title`                    |
//! | `bindings`        | `1 -> 0`; replace changed `configuration_snapshot`   |
//! | `"references"`    | `1 -> 0`; replace changed `target_uri`               |
//! | `delivery_attempts` | `1 -> 0`; `0017` already refuses same-id and same `(message_id, attempt_number)` replacement |
//!
//! Why the existing guards did not stop it, and why "it was deletable" is the
//! wrong summary: `0013` and `0014` do guard these tables, but every one of
//! those arms is **conditional on a referencing row existing**. A populated
//! fixture therefore looks protected while a valid leaf row is still removable.
//! [`existing_reference_and_event_guards_still_apply`] and
//! [`those_guards_block_on_the_pre_0028_schema_too`] pin that both halves stay.
//!
//! Scoring rules taken from ADVERSARIAL_REVIEW_PLAYBOOK, because a passing
//! assertion here is worthless unless it can fail:
//!
//! - rule 2, "failure disguised as a pass": every negative assertion checks that
//!   the statement returned an error **and** that the row survives with every
//!   column unchanged ([`row_snapshot`] renders the whole row) **and**, where a
//!   new guard is the thing under test, that the refusal carries that guard's
//!   own message. A foreign key, an unrelated CHECK or a zero-row no-op never
//!   counts as a pass here.
//! - rule 1-c, the rig declares its own schema: [`the_new_guards_are_installed`]
//!   asserts the `0028` triggers exist on the hardened schema **and** are absent
//!   on the `v27` baseline, and [`the_row_snapshot_covers_every_column`] pins the
//!   snapshot column lists against `pragma_table_info`, so a rig that silently
//!   loaded the wrong migration -- or that snapshotted fewer columns than the
//!   table has -- cannot produce a result.
//! - rule 3, every tested row is seeded: [`seed`] ends with per-table counts and
//!   a `pragma_foreign_key_check`, and no test creates the row it then attacks.
//!
//! Frozen basis, none of which this file extends:
//!   - DATA_CONVENTIONS.md section 5: "P0 核心对象无通用 DELETE 命令";
//!     Team uses `ACTIVE/PAUSED/ARCHIVED`, Role uses `ACTIVE/DEACTIVATED`, Task
//!     uses `CANCELLED`; "Event、Run、Binding、Review decision、DeliveryAttempt、
//!     Approval decision 不更新历史语义，只追加后续事实"; and "引用失效只更新
//!     `validation_status`；不删除历史 URI/path/hash".
//!   - DATA_CONVENTIONS.md section 6: Team/Role/Task/Binding are "随 Team 永久
//!     保留"; Message/Delivery are "随 Team 保留".
//!   - DOMAIN_MODEL.md section 5: "核心对象通过稳定 ID 关联，不级联硬删除";
//!     a Role that is deactivated keeps its Task/Thread/Artifact/Binding
//!     history; a cancelled Task deletes no Run/Artifact/Event; "Workspace 引用
//!     失效不删除 Reference；记录验证状态并引导修复"; "Message 到期/失败不删除
//!     原文或投递尝试".
//!   - DOMAIN_MODEL.md section 2: Team, Role and Task are stable identities and
//!     a Binding is "版本化且追加历史".
//!
//! Two deliberate non-claims, so this file is not read as promising more:
//!   - **Attention gets no permanent DELETE guard.** Deep audit section 4 records
//!     that Attention is a rebuildable derived index outside the DATA retention
//!     table. Nothing here touches `attention_items`.
//!   - **Legitimate editing is not welded shut.** Team/Role/Task/Binding/
//!     Reference/Delivery all keep their normal state transitions; see
//!     [`ordinary_business_edits_still_work`]. Retention here means the row is
//!     not destroyed or silently replaced, not that the object is frozen.

use std::{borrow::Cow, error::Error, future::Future, pin::Pin, str::FromStr};

use persistence_sqlite::migrate;
use sqlx_core::{
    error::BoxDynError,
    migrate::{Migration, MigrationSource, MigrationType, Migrator},
    query::query,
    query_scalar::query_scalar,
    raw_sql::raw_sql,
};
use sqlx_sqlite::{SqliteConnectOptions, SqlitePool, SqlitePoolOptions};

// ---------------------------------------------------------------------------
// Identities.
//
// Every ID is a 16-byte UUIDv7 literal: the `7` version nibble sits at hex index
// 12 and an RFC 4122 variant nibble at index 16, because the CHECK constraints
// reject anything else and a rejected INSERT would silently turn an attack into
// a device fault.
// ---------------------------------------------------------------------------

/// The Team everything else hangs from. It is deliberately *not* the leaf probe.
const TEAM: &str = "01921389d31174628f80cb5f79ab0001";
const ROLE: &str = "01921389d31174628f80cb5f79ab0002";
const HARNESS: &str = "01921389d31174628f80cb5f79ab0003";
const ADAPTER: &str = "01921389d31174628f80cb5f79ab0004";
/// Generation 1 of `ROLE`'s bindings, `PENDING_CHECK`.
const BINDING: &str = "01921389d31174628f80cb5f79ab0005";
/// The Task a `"references"` row names, so `0013` already arms it.
const REFERENCED_TASK: &str = "01921389d31174628f80cb5f79ab0006";
const REFERENCE: &str = "01921389d31174628f80cb5f79ab0007";

const THREAD: &str = "01921389d31174628f80cb5f79ab0010";
const MESSAGE: &str = "01921389d31174628f80cb5f79ab0011";
/// The one `PENDING` attempt on `MESSAGE`. Nothing points at an attempt row.
const ATTEMPT: &str = "01921389d31174628f80cb5f79ab0012";

/// An empty Team: no Role, no Workspace, no Event, so no foreign key and no
/// `0014` guard stands between it and a `DELETE`.
const LONE_TEAM: &str = "01921389d31174628f80cb5f79ab0020";
/// A Role that owns no Task, holds no Binding, receives no Message and is named
/// by no Event.
const LONE_ROLE: &str = "01921389d31174628f80cb5f79ab0021";
/// A Task no `"references"` row and no `events` row names.
const LONE_TASK: &str = "01921389d31174628f80cb5f79ab0022";
/// `ROLE`'s second Binding, generation 2. No Run points at it.
const LONE_BINDING: &str = "01921389d31174628f80cb5f79ab0023";

/// A Task an `events` row names as its subject, so `0014` already arms it.
const EVENTED_TASK: &str = "01921389d31174628f80cb5f79ab0030";
const EVENT_TASK: &str = "01921389d31174628f80cb5f79ab0031";
/// A Role an `events` row names as its subject, so `0014` already arms it.
const EVENTED_ROLE: &str = "01921389d31174628f80cb5f79ab0032";
const EVENT_ROLE: &str = "01921389d31174628f80cb5f79ab0033";

/// A second, `ACTIVE` Binding for `ACTIVE_ROLE`, which holds no other Binding.
///
/// Exists only so the single-ACTIVE-per-Role path has a real victim without
/// colliding with the legitimate status advance in
/// [`ordinary_business_edits_still_work`]; see
/// [`the_single_active_binding_slot_is_already_refused`].
const ACTIVE_ROLE: &str = "01921389d31174628f80cb5f79ab0035";
const ACTIVE_BINDING: &str = "01921389d31174628f80cb5f79ab0034";

/// Attacker identities. Every one of these differs from the row it collides
/// with, so a rejection cannot be explained by "it was the same primary key".
const ATTACK_TEAM: &str = "01921389d31174628f80cb5f79ab0101";
const ATTACK_ROLE: &str = "01921389d31174628f80cb5f79ab0102";
const ATTACK_TASK: &str = "01921389d31174628f80cb5f79ab0103";
const ATTACK_BINDING: &str = "01921389d31174628f80cb5f79ab0104";
const ATTACK_REFERENCE: &str = "01921389d31174628f80cb5f79ab0105";
const ATTACK_ATTEMPT: &str = "01921389d31174628f80cb5f79ab0106";

/// The `0028` triggers, in the order `0028` creates them.
///
/// Kept as one list so the schema assertion, the failed-upgrade test and the
/// mutation log cannot drift apart. The last entry matters: the failure-path
/// test occupies that name, so the seventeen triggers created before it inside
/// the same transaction have to roll back with it.
const NEW_TRIGGERS: [&str; 18] = [
    "teams_retained_entity_retention_delete",
    "teams_retained_entity_replace_guard",
    "teams_retained_entity_identity_immutable_update",
    "roles_retained_entity_retention_delete",
    "roles_retained_entity_replace_guard",
    "roles_retained_entity_identity_immutable_update",
    "tasks_retained_entity_retention_delete",
    "tasks_retained_entity_replace_guard",
    "tasks_retained_entity_identity_immutable_update",
    "bindings_retained_entity_retention_delete",
    "bindings_retained_entity_replace_guard",
    "bindings_retained_entity_identity_immutable_update",
    "references_retained_entity_retention_delete",
    "references_retained_entity_replace_guard",
    "references_retained_entity_identity_immutable_update",
    "delivery_attempts_retained_entity_retention_delete",
    "delivery_attempts_retained_entity_active_slot_replace_guard",
    "delivery_attempts_retained_entity_identity_immutable_update",
];

/// The message each new guard raises. Asserted, not assumed: a negative test
/// that only checked `is_err()` could be green because of a foreign key.
const RETAINED: &str = "history is retained with the Team";
const IMMUTABLE: &str = "identity is immutable";

/// Every table this package reads or seeds.
///
/// The snapshot columns are **not** written down here: they are read back from
/// `pragma_table_info` at run time, so a column can never quietly fall out of
/// the "row unchanged" proof. [`the_row_snapshot_covers_every_column`] then
/// checks that each real column really did appear in the rendered row.
const SNAPSHOT_TABLES: [&str; 10] = [
    "teams",
    "roles",
    "tasks",
    "bindings",
    "references",
    "delivery_attempts",
    "harness_instances",
    "adapters",
    "threads",
    "messages",
];

const SEED: &str = "\
INSERT INTO teams VALUES(X'01921389d31174628f80cb5f79ab0001','t','ACTIVE',1,1,1,NULL);
INSERT INTO roles VALUES(X'01921389d31174628f80cb5f79ab0002',X'01921389d31174628f80cb5f79ab0001','R-1','builder','ACTIVE',1,1,1,NULL);
INSERT INTO harness_instances VALUES(X'01921389d31174628f80cb5f79ab0003',X'01921389d31174628f80cb5f79ab0001','h','h','ACTIVE','[]',NULL,1,1,1,NULL);
INSERT INTO adapters VALUES(X'01921389d31174628f80cb5f79ab0004',X'01921389d31174628f80cb5f79ab0001','a','1','/a','{}',1,1,1);
INSERT INTO bindings VALUES(X'01921389d31174628f80cb5f79ab0005',X'01921389d31174628f80cb5f79ab0001',X'01921389d31174628f80cb5f79ab0002',X'01921389d31174628f80cb5f79ab0003',X'01921389d31174628f80cb5f79ab0004','PENDING_CHECK',1,1,'{\"gen\":1}',1,1,NULL,NULL);
INSERT INTO tasks VALUES(X'01921389d31174628f80cb5f79ab0006',X'01921389d31174628f80cb5f79ab0001',X'01921389d31174628f80cb5f79ab0002','T-1','ship it','','DOING',1,1,1,NULL,NULL);
INSERT INTO threads VALUES(X'01921389d31174628f80cb5f79ab0010',X'01921389d31174628f80cb5f79ab0001','thread','OPEN',1,1,1);
INSERT INTO messages(id,team_id,thread_id,sender_actor_type,from_human_id,to_role_id,message_type,body,status,version,created_at,updated_at) VALUES(X'01921389d31174628f80cb5f79ab0011',X'01921389d31174628f80cb5f79ab0001',X'01921389d31174628f80cb5f79ab0010','HUMAN',X'01921389d31174628f80cb5f79ab0039',X'01921389d31174628f80cb5f79ab0002','DIRECT','body','DRAFT',1,1,1);
UPDATE messages SET status='QUEUED', version=2 WHERE id=X'01921389d31174628f80cb5f79ab0011';
";

/// The leaf rows: one per table, none named by an Event, a Reference or a child.
///
/// `"references"` is seeded after the Task it points at so `0013` accepts it.
const LEAF_SEED: &str = "\
INSERT INTO \"references\" VALUES(X'01921389d31174628f80cb5f79ab0007',X'01921389d31174628f80cb5f79ab0001','TASK',X'01921389d31174628f80cb5f79ab0006','WORKSPACE_PATH','docs/spec.md','VALID',NULL,1,7,7);
INSERT INTO teams VALUES(X'01921389d31174628f80cb5f79ab0020','lone','ACTIVE',1,1,1,NULL);
INSERT INTO roles VALUES(X'01921389d31174628f80cb5f79ab0021',X'01921389d31174628f80cb5f79ab0001','L-1','lone role','ACTIVE',1,1,1,NULL);
INSERT INTO tasks VALUES(X'01921389d31174628f80cb5f79ab0022',X'01921389d31174628f80cb5f79ab0001',X'01921389d31174628f80cb5f79ab0002','T-L','lone task','','TODO',1,1,1,NULL,NULL);
INSERT INTO bindings VALUES(X'01921389d31174628f80cb5f79ab0023',X'01921389d31174628f80cb5f79ab0001',X'01921389d31174628f80cb5f79ab0002',X'01921389d31174628f80cb5f79ab0003',X'01921389d31174628f80cb5f79ab0004','PENDING_CHECK',2,1,'{\"gen\":2}',1,1,NULL,NULL);
INSERT INTO delivery_attempts(id,team_id,message_id,attempt_number,target_role_id,target_binding_id,target_binding_generation,status,error_code,error_message,created_at,started_at,completed_at) VALUES(X'01921389d31174628f80cb5f79ab0012',X'01921389d31174628f80cb5f79ab0001',X'01921389d31174628f80cb5f79ab0011',1,X'01921389d31174628f80cb5f79ab0002',NULL,NULL,'PENDING',NULL,NULL,4,NULL,NULL);
INSERT INTO roles VALUES(X'01921389d31174628f80cb5f79ab0035',X'01921389d31174628f80cb5f79ab0001','R-A','active role','ACTIVE',1,1,1,NULL);
INSERT INTO bindings VALUES(X'01921389d31174628f80cb5f79ab0034',X'01921389d31174628f80cb5f79ab0001',X'01921389d31174628f80cb5f79ab0035',X'01921389d31174628f80cb5f79ab0003',X'01921389d31174628f80cb5f79ab0004','ACTIVE',1,1,'{\"gen\":1}',1,1,5,NULL);
";

/// Rows that a pre-existing guard already protects, plus the Event that arms it.
///
/// These are what makes the new guards *additional* rather than a replacement:
/// each of these rows was already refused before `0028` existed.
const EVENTED_SEED: &str = "\
INSERT INTO tasks VALUES(X'01921389d31174628f80cb5f79ab0030',X'01921389d31174628f80cb5f79ab0001',X'01921389d31174628f80cb5f79ab0002','T-E','evented task','','TODO',1,1,1,NULL,NULL);
INSERT INTO events(id,team_id,event_type,actor_type,subject_type,subject_id,object_version,occurred_at,hash) VALUES(X'01921389d31174628f80cb5f79ab0031',X'01921389d31174628f80cb5f79ab0001','audit.recorded','SYSTEM','TASK',X'01921389d31174628f80cb5f79ab0030',1,1,'E3B0C44298FC1C149AFBF4C8996FB92427AE41E4649B934CA495991B7852B855');
INSERT INTO roles VALUES(X'01921389d31174628f80cb5f79ab0032',X'01921389d31174628f80cb5f79ab0001','R-E','evented role','ACTIVE',1,1,1,NULL);
INSERT INTO events(id,team_id,event_type,actor_type,subject_type,subject_id,object_version,occurred_at,hash) VALUES(X'01921389d31174628f80cb5f79ab0033',X'01921389d31174628f80cb5f79ab0001','audit.recorded','SYSTEM','ROLE',X'01921389d31174628f80cb5f79ab0032',1,1,'E3B0C44298FC1C149AFBF4C8996FB92427AE41E4649B934CA495991B7852B855');
";

/// Every leaf row, paired with the table it lives in.
///
/// Each of these must survive a `DELETE` on its own. `LONE_TEAM` exists because a
/// Team that owns a Role is refused by a foreign key, and a foreign key is not
/// the guarantee under test.
const LEAF_ROWS: [(&str, &str); 6] = [
    ("teams", LONE_TEAM),
    ("roles", LONE_ROLE),
    ("tasks", LONE_TASK),
    ("bindings", LONE_BINDING),
    ("references", REFERENCE),
    ("delivery_attempts", ATTEMPT),
];

async fn count(pool: &SqlitePool, table: &str) -> Result<i64, Box<dyn Error>> {
    Ok(
        query_scalar::<_, i64>(&format!("SELECT COUNT(*) FROM \"{table}\""))
            .fetch_one(pool)
            .await?,
    )
}

/// A fresh database with every migration applied and the whole fixture seeded.
async fn database() -> Result<SqlitePool, Box<dyn Error>> {
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(SqliteConnectOptions::from_str("sqlite::memory:")?.foreign_keys(true))
        .await?;
    migrate(&pool).await?;
    seed(&pool).await?;
    Ok(pool)
}

/// A migrated database with the shared graph seeded but **no** leaf rows.
///
/// Used as the control for a business-key vector: the same statement that is
/// refused against a leaf row must be accepted here, or the refusal could be
/// explained by the seed rather than by retention.
async fn base_database() -> Result<SqlitePool, Box<dyn Error>> {
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(SqliteConnectOptions::from_str("sqlite::memory:")?.foreign_keys(true))
        .await?;
    migrate(&pool).await?;
    raw_sql(SEED).execute(&pool).await?;
    Ok(pool)
}

/// The seed plus its own self-checks. A rig fault must fail here rather than
/// look like an attack result.
async fn seed(pool: &SqlitePool) -> Result<(), Box<dyn Error>> {
    raw_sql(SEED).execute(pool).await?;
    raw_sql(LEAF_SEED).execute(pool).await?;
    raw_sql(EVENTED_SEED).execute(pool).await?;
    for (table, expected) in [
        ("teams", 2),
        ("roles", 4),
        ("tasks", 3),
        ("bindings", 3),
        ("references", 1),
        ("delivery_attempts", 1),
        ("messages", 1),
        ("events", 2),
    ] {
        assert_eq!(
            count(pool, table).await?,
            expected,
            "the base seed for {table} did not land the expected number of rows"
        );
    }
    assert_eq!(
        query_scalar::<_, i64>("SELECT COUNT(*) FROM pragma_foreign_key_check")
            .fetch_one(pool)
            .await?,
        0,
        "the base seed is not foreign-key clean"
    );
    // Every leaf row must really be a leaf, or the DELETE result below would be
    // explained by a foreign key rather than by the guard under test.
    for (table, id) in LEAF_ROWS {
        assert!(
            row_snapshot(pool, table, id).await?.is_some(),
            "the leaf probe row for {table} is not present"
        );
    }
    assert_eq!(
        query_scalar::<_, i64>(&format!(
            "SELECT COUNT(*) FROM \"references\" WHERE subject_id=X'{LONE_TASK}'"
        ))
        .fetch_one(pool)
        .await?,
        0,
        "LONE_TASK is named by a Reference, so it is not a leaf probe"
    );
    assert_eq!(
        query_scalar::<_, i64>(&format!(
            "SELECT COUNT(*) FROM events WHERE subject_id=X'{LONE_TASK}'"
        ))
        .fetch_one(pool)
        .await?,
        0,
        "LONE_TASK is named by an Event, so it is not a leaf probe"
    );
    // The fixture wiring the rest of the file depends on, stated here so a
    // changed constant cannot silently un-arm the pre-existing-guard probes.
    for (probe, expected, why) in [
        (
            format!("SELECT lower(hex(subject_id)) FROM \"references\" WHERE id=X'{REFERENCE}'"),
            REFERENCED_TASK,
            "REFERENCE must name REFERENCED_TASK for the 0013 probe",
        ),
        (
            format!("SELECT lower(hex(subject_id)) FROM events WHERE id=X'{EVENT_TASK}'"),
            EVENTED_TASK,
            "EVENT_TASK must name EVENTED_TASK for the 0014 probe",
        ),
        (
            format!("SELECT lower(hex(subject_id)) FROM events WHERE id=X'{EVENT_ROLE}'"),
            EVENTED_ROLE,
            "EVENT_ROLE must name EVENTED_ROLE for the 0014 probe",
        ),
        (
            format!("SELECT lower(hex(thread_id)) FROM messages WHERE id=X'{MESSAGE}'"),
            THREAD,
            "MESSAGE must hang off THREAD for the delivery probe",
        ),
    ] {
        assert_eq!(
            query_scalar::<_, String>(&probe).fetch_one(pool).await?,
            expected.to_lowercase(),
            "{why}"
        );
    }
    // Columns that actually carry a foreign key or a source pointer to `tasks`.
    // `tasks` itself is not one of them: counting its own row here would report
    // the probe as its own child.
    for (table, column) in [
        ("runs", "task_id"),
        ("reviews", "task_id"),
        ("artifacts", "task_id"),
        ("task_dependencies", "task_id"),
        ("attention_items", "source_id"),
    ] {
        assert_eq!(
            query_scalar::<_, i64>(&format!(
                "SELECT COUNT(*) FROM \"{table}\" WHERE {column}=X'{LONE_TASK}'"
            ))
            .fetch_one(pool)
            .await?,
            0,
            "LONE_TASK still has a child row in {table}"
        );
    }
    for binding in [LONE_BINDING, BINDING, ACTIVE_BINDING] {
        assert_eq!(
            query_scalar::<_, i64>(&format!(
                "SELECT COUNT(*) FROM runs WHERE binding_id=X'{binding}'"
            ))
            .fetch_one(pool)
            .await?,
            0,
            "a Binding probe row is named by a Run"
        );
    }
    Ok(())
}

/// The real column names of `table`, in declaration order.
///
/// Read from the schema rather than transcribed, which is what makes the
/// "every column" claim in [`the_row_snapshot_covers_every_column`] hold by
/// construction rather than by review.
async fn columns_of(pool: &SqlitePool, table: &str) -> Result<Vec<String>, Box<dyn Error>> {
    Ok(query_scalar::<_, String>(&format!(
        "SELECT name FROM pragma_table_info('{table}') ORDER BY cid"
    ))
    .fetch_all(pool)
    .await?)
}

/// `quote(col) || '=' || quote(col)` for every column, joined by newlines.
///
/// The column is quoted on **both** sides on purpose: a bare `col || '='` would
/// make SQLite coerce the 16 raw UUID bytes to text, which fails on the first
/// non-UTF-8 byte. `quote()` renders a BLOB as `X'...'`, so every column becomes
/// TEXT before the concatenation happens.
fn projection(columns: &[String]) -> String {
    columns
        .iter()
        .map(|column| format!("'{column}=' || quote(\"{column}\")"))
        .collect::<Vec<_>>()
        .join(" || char(10) || ")
}

/// The full contents of one row, rendered column by column.
///
/// `quote()` is used so a `NULL`, an empty string, a zero and a BLOB all render
/// distinctly, and every column is named in the output so a single changed
/// column shows up as a diff rather than as a silently equal row.
async fn row_snapshot(
    pool: &SqlitePool,
    table: &str,
    id: &str,
) -> Result<Option<String>, Box<dyn Error>> {
    let columns = columns_of(pool, table).await?;
    assert!(
        !columns.is_empty(),
        "{table} has no columns, so the rig cannot be reading the right schema"
    );
    Ok(query_scalar::<_, String>(&format!(
        "SELECT {} FROM \"{table}\" WHERE id = X'{id}'",
        projection(&columns)
    ))
    .fetch_optional(pool)
    .await?)
}

/// Every seeded row of every snapshotted table, for the upgrade proof.
async fn graph_snapshot(pool: &SqlitePool) -> Result<String, Box<dyn Error>> {
    let mut out = String::new();
    for table in SNAPSHOT_TABLES {
        let columns = columns_of(pool, table).await?;
        let rows: Vec<String> = query_scalar::<_, String>(&format!(
            "SELECT {} FROM \"{table}\" ORDER BY rowid",
            projection(&columns)
        ))
        .fetch_all(pool)
        .await?;
        out.push_str(&format!("### {table} ({})\n", rows.len()));
        for row in rows {
            out.push_str(&row);
            out.push('\n');
        }
    }
    Ok(out)
}

/// Run `sql`, require it to be refused, and return the refusal message.
async fn refusal_of(pool: &SqlitePool, sql: &str) -> Result<String, Box<dyn Error>> {
    match query(sql).execute(pool).await {
        Ok(result) => {
            let changed = result.rows_affected();
            panic!("the write was accepted and changed {changed} row(s): {sql}")
        }
        Err(error) => Ok(format!("{error}")),
    }
}

/// A rejected write must leave its victim byte-identical, and must be refused
/// by the named guard rather than by a foreign key that happens to be in the way.
async fn assert_refused_and_intact(
    pool: &SqlitePool,
    table: &str,
    id: &str,
    sql: &str,
    expected_message: &str,
) -> Result<(), Box<dyn Error>> {
    let before = row_snapshot(pool, table, id)
        .await?
        .unwrap_or_else(|| panic!("{table} row {id} vanished before {sql} was attempted"));
    let message = refusal_of(pool, sql).await?;
    assert!(
        message.contains(expected_message),
        "{table}: refused for '{message}' rather than at '{expected_message}' -- {sql}"
    );
    assert_eq!(
        row_snapshot(pool, table, id).await?,
        Some(before),
        "{table}: the refused write changed the row -- {sql}"
    );
    Ok(())
}

// ---------------------------------------------------------------------------
// Migrations up to `0027`, used to prove the pre-`0028` schema behaves
// differently from the hardened one (ADVERSARIAL_REVIEW_PLAYBOOK rule 1-b).
// ---------------------------------------------------------------------------
const V27: [(&str, &str); 27] = [
    (
        "team role workspace",
        include_str!("../migrations/0001_team_role_workspace.sql"),
    ),
    (
        "harness adapter binding",
        include_str!("../migrations/0002_harness_adapter_binding.sql"),
    ),
    (
        "task dependency",
        include_str!("../migrations/0003_task_dependency.sql"),
    ),
    (
        "run lease review",
        include_str!("../migrations/0004_run_lease_review.sql"),
    ),
    (
        "artifact reference",
        include_str!("../migrations/0005_artifact_reference.sql"),
    ),
    (
        "event outbox",
        include_str!("../migrations/0006_event_outbox.sql"),
    ),
    (
        "thread message delivery",
        include_str!("../migrations/0007_thread_message_delivery.sql"),
    ),
    (
        "notification attention",
        include_str!("../migrations/0008_notification_attention.sql"),
    ),
    (
        "permission approval",
        include_str!("../migrations/0009_permission_approval.sql"),
    ),
    (
        "binding team consistency",
        include_str!("../migrations/0010_binding_team_consistency.sql"),
    ),
    (
        "task dependency consistency",
        include_str!("../migrations/0011_task_dependency_consistency.sql"),
    ),
    (
        "run lease consistency",
        include_str!("../migrations/0012_run_lease_consistency.sql"),
    ),
    (
        "artifact reference consistency",
        include_str!("../migrations/0013_artifact_reference_consistency.sql"),
    ),
    (
        "event outbox consistency",
        include_str!("../migrations/0014_event_outbox_consistency.sql"),
    ),
    (
        "event actor outbox integrity",
        include_str!("../migrations/0015_event_actor_outbox_integrity.sql"),
    ),
    (
        "event dedup state immutability",
        include_str!("../migrations/0016_event_dedup_state_immutability.sql"),
    ),
    (
        "message delivery integrity",
        include_str!("../migrations/0017_message_delivery_integrity.sql"),
    ),
    (
        "message delivery insert state guard",
        include_str!("../migrations/0018_message_delivery_insert_state_guard.sql"),
    ),
    (
        "notification attention integrity",
        include_str!("../migrations/0019_notification_attention_integrity.sql"),
    ),
    (
        "permission approval integrity",
        include_str!("../migrations/0020_permission_approval_integrity.sql"),
    ),
    (
        "approval policy linkage",
        include_str!("../migrations/0021_approval_policy_linkage.sql"),
    ),
    (
        "command idempotency",
        include_str!("../migrations/0022_command_idempotency.sql"),
    ),
    (
        "notification execution integrity",
        include_str!("../migrations/0023_notification_execution_integrity.sql"),
    ),
    (
        "outbox retry default",
        include_str!("../migrations/0024_outbox_retry_default.sql"),
    ),
    (
        "message routing team integrity",
        include_str!("../migrations/0025_message_routing_team_integrity.sql"),
    ),
    (
        "core history retention",
        include_str!("../migrations/0026_core_history_retention.sql"),
    ),
    (
        "run transition integrity",
        include_str!("../migrations/0027_run_transition_integrity.sql"),
    ),
];

#[derive(Debug)]
struct V27OnlyMigrations;

impl MigrationSource<'static> for V27OnlyMigrations {
    fn resolve(
        self,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<Migration>, BoxDynError>> + Send + 'static>> {
        Box::pin(async {
            Ok(V27
                .iter()
                .enumerate()
                .map(|(index, (description, sql))| {
                    Migration::new(
                        (index + 1) as i64,
                        Cow::Borrowed(*description),
                        MigrationType::Simple,
                        Cow::Borrowed(*sql),
                        false,
                    )
                })
                .collect())
        })
    }
}

/// A database at `0027`: everything except this package's migration.
async fn v27_database() -> Result<SqlitePool, Box<dyn Error>> {
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(SqliteConnectOptions::from_str("sqlite::memory:")?.foreign_keys(true))
        .await?;
    Migrator::new(V27OnlyMigrations).await?.run(&pool).await?;
    Ok(pool)
}

async fn trigger_exists(pool: &SqlitePool, name: &str) -> Result<bool, Box<dyn Error>> {
    Ok(
        query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM sqlite_master WHERE type='trigger' AND name=?",
        )
        .bind(name)
        .fetch_one(pool)
        .await?
            == 1,
    )
}

// ---------------------------------------------------------------------------
// Axis 1 -- DELETE with no Event, no Reference and no child row.
// ---------------------------------------------------------------------------

/// The census counterexample, kept in the shape the inventory logged: for each
/// of the six objects, `DELETE` must be refused and every column must survive.
///
/// The assertion is the guard's own message **plus** a full row comparison. On
/// the `v27` baseline the statement returned `Ok` and the row really was gone,
/// so neither half could have been passing for an unrelated reason.
#[tokio::test]
async fn leaf_rows_of_all_six_entities_cannot_be_deleted() -> Result<(), Box<dyn Error>> {
    let pool = database().await?;
    for (table, id) in LEAF_ROWS {
        assert_refused_and_intact(
            &pool,
            table,
            id,
            &format!("DELETE FROM \"{table}\" WHERE id=X'{id}'"),
            RETAINED,
        )
        .await?;
    }
    Ok(())
}

/// A `DELETE` that matches nothing is not an error and not a bypass.
///
/// Without this, a guard implemented as a bare `RAISE` on a statement that never
/// matched a row could look identical to a correct one.
#[tokio::test]
async fn a_zero_row_delete_is_not_a_bypass_and_is_not_an_error() -> Result<(), Box<dyn Error>> {
    let pool = database().await?;
    let result = query(&format!("DELETE FROM tasks WHERE id=X'{ATTACK_TASK}'"))
        .execute(&pool)
        .await?;
    assert_eq!(
        result.rows_affected(),
        0,
        "the zero-row DELETE matched something"
    );
    assert_eq!(count(&pool, "tasks").await?, 3);
    Ok(())
}

// ---------------------------------------------------------------------------
// Axis 2 -- INSERT OR REPLACE, on the primary key *and* on each real UNIQUE key.
//
// `teams` and `"references"` have exactly one unique key, the primary key, so
// for those two the different-id arm does not exist and the same-id arm is the
// whole story. Saying so is the point; inventing a second unique key for them
// would be testing a constraint the schema does not have.
// ---------------------------------------------------------------------------

/// Same primary key, for all six.
#[tokio::test]
async fn a_replacement_on_the_same_primary_key_is_refused() -> Result<(), Box<dyn Error>> {
    let pool = database().await?;
    for (table, id) in LEAF_ROWS {
        // The row must be present and readable before the attack, so a refusal
        // cannot be reported against a row the rig had already lost.
        assert!(
            row_snapshot(&pool, table, id).await?.is_some(),
            "{table}: the probe row {id} is missing before the attack"
        );
        // Rebuild the same row through INSERT OR REPLACE with one visible value
        // changed, so a silent overwrite would have to show up as a diff.
        //
        // `delivery_attempts` is the one table where the refusal belongs to a
        // pre-existing guard: `0017`'s replace guard already covers a same-id
        // collision and keeps its own message, and `0028` deliberately stands
        // aside. Asserting that here is what makes "not duplicated, not
        // shadowed" a tested claim rather than a comment.
        let expected = if table == "delivery_attempts" {
            "Delivery attempt deduplication is immutable"
        } else {
            RETAINED
        };
        let sql = replacement_of(table, id, id);
        assert_refused_and_intact(&pool, table, id, &sql, expected).await?;
    }
    Ok(())
}

/// A **different** primary key colliding with a **real** business unique key.
///
/// * `roles`: `UNIQUE (team_id, display_id)`.
/// * `tasks`: `UNIQUE (team_id, display_id)`.
/// * `bindings`: `UNIQUE INDEX (role_id, binding_generation)`.
/// * `delivery_attempts`: `delivery_attempts_single_active`, the partial unique
///   index `ON (message_id) WHERE status IN ('PENDING','IN_FLIGHT')` -- the one
///   business key `0017`'s replace guard does not cover.
/// * `teams` / `"references"`: no second unique key exists, so they are covered
///   by the same-id arm above and are deliberately absent here.
#[tokio::test]
async fn a_replacement_under_a_different_id_colliding_on_a_business_key_is_refused()
-> Result<(), Box<dyn Error>> {
    let pool = database().await?;
    for (table, victim, sql) in business_key_vectors() {
        assert_refused_and_intact(&pool, table, victim, &sql, RETAINED).await?;
    }
    Ok(())
}

/// Every business-key vector really does change a different column, and really
/// does carry a different primary key, so the arm above cannot be passing
/// because the row happened to be the same one.
#[tokio::test]
async fn each_business_key_vector_changes_a_different_value_and_a_different_id()
-> Result<(), Box<dyn Error>> {
    for (table, victim, sql) in business_key_vectors() {
        let id_column = if table == "references" {
            "\"references\""
        } else {
            table
        };
        let candidate = extract_attack_id(&sql);
        assert_ne!(
            candidate.as_str(),
            victim,
            "{table}: the vector uses the victim's own primary key"
        );
        // Prove the INSERT would otherwise be legal: against the same schema
        // with no conflicting row the very same statement is accepted.
        let probe = base_database().await?;
        let rows = query(&sql).execute(&probe).await;
        assert!(
            rows.is_ok(),
            "{table}: the vector is refused by an unrelated constraint, so it \
             proves nothing about retention -- {}",
            rows.err()
                .map(|error| error.to_string())
                .unwrap_or_default()
        );
        assert!(
            query_scalar::<_, String>(&format!(
                "SELECT quote(id) FROM {id_column} WHERE id=X'{candidate}'"
            ))
            .fetch_optional(&probe)
            .await?
            .is_some(),
            "{table}: the control insert did not land"
        );
    }
    Ok(())
}

/// The single-ACTIVE-per-Role slot stays refused.
///
/// Measured, not assumed: `0002`'s overlap trigger already refuses a second
/// `ACTIVE` Binding for the same Role, so `0028` deliberately adds **no** arm for
/// this key. A guard for it would be unproven code asserting a guarantee another
/// trigger already provides -- the same judgement `0026` recorded for `runs`,
/// `approvals` and `messages`.
#[tokio::test]
async fn the_single_active_binding_slot_is_already_refused() -> Result<(), Box<dyn Error>> {
    let pool = database().await?;
    let sql = format!(
        "INSERT OR REPLACE INTO bindings VALUES(X'{ATTACK_BINDING}',X'{TEAM}',X'{ACTIVE_ROLE}',\
         X'{HARNESS}',X'{ADAPTER}','ACTIVE',9,1,'{{\"gen\":9}}',1,1,6,NULL)"
    );
    let before = row_snapshot(&pool, "bindings", ACTIVE_BINDING).await?;
    let message = refusal_of(&pool, &sql).await?;
    assert!(
        message.contains("OVERLAPPING_BINDING_EFFECTIVE_RANGE"),
        "the ACTIVE slot was refused for '{message}' rather than at the overlap guard"
    );
    assert_eq!(
        row_snapshot(&pool, "bindings", ACTIVE_BINDING).await?,
        before
    );
    Ok(())
}

// ---------------------------------------------------------------------------
// Axis 3 -- pre-existing guards keep working, and keep their own message.
// ---------------------------------------------------------------------------

/// `0013` and `0014` already refused these rows before `0028`; they must still
/// do so afterwards, with the same message, so this package is provably additive.
#[tokio::test]
async fn existing_reference_and_event_guards_still_apply() -> Result<(), Box<dyn Error>> {
    let pool = database().await?;
    for (table, id, sql, expected) in [
        (
            "tasks",
            REFERENCED_TASK,
            format!("DELETE FROM tasks WHERE id=X'{REFERENCED_TASK}'"),
            "Reference consistency violation",
        ),
        (
            "tasks",
            EVENTED_TASK,
            format!("DELETE FROM tasks WHERE id=X'{EVENTED_TASK}'"),
            "Event subject consistency violation",
        ),
        (
            "roles",
            EVENTED_ROLE,
            format!("DELETE FROM roles WHERE id=X'{EVENTED_ROLE}'"),
            "Event subject consistency violation",
        ),
    ] {
        assert_refused_and_intact(&pool, table, id, &sql, expected).await?;
    }
    Ok(())
}

/// The same two guards, on a database that has **only** migrations `0001..0027`.
///
/// Without this half a green result on the hardened schema could be the new
/// guard doing all the work, and the claim "the pre-existing guard still carries
/// these rows" would be untested.
#[tokio::test]
async fn those_guards_block_on_the_pre_0028_schema_too() -> Result<(), Box<dyn Error>> {
    let pool = v27_database().await?;
    seed(&pool).await?;
    for (table, id, sql, expected) in [
        (
            "tasks",
            REFERENCED_TASK,
            format!("DELETE FROM tasks WHERE id=X'{REFERENCED_TASK}'"),
            "Reference consistency violation",
        ),
        (
            "tasks",
            EVENTED_TASK,
            format!("DELETE FROM tasks WHERE id=X'{EVENTED_TASK}'"),
            "Event subject consistency violation",
        ),
    ] {
        assert_refused_and_intact(&pool, table, id, &sql, expected).await?;
    }
    Ok(())
}

/// `0017`'s replace guard keeps its original effect, with its own message.
///
/// FC-02 must not duplicate it and must not let a new trigger shadow it, so this
/// asserts the exact string `0017` raises for both of its arms.
#[tokio::test]
async fn delivery_attempts_keep_their_original_replace_guard() -> Result<(), Box<dyn Error>> {
    let pool = database().await?;
    for sql in [
        // Same primary key.
        format!(
            "INSERT OR REPLACE INTO delivery_attempts(id,team_id,message_id,attempt_number,target_role_id,target_binding_id,target_binding_generation,status,error_code,error_message,created_at,started_at,completed_at) VALUES(X'{ATTEMPT}',X'{TEAM}',\
             X'{MESSAGE}',1,X'{ROLE}',NULL,NULL,'PENDING',NULL,NULL,9,NULL,NULL)"
        ),
        // Different primary key, colliding on (message_id, attempt_number).
        format!(
            "INSERT OR REPLACE INTO delivery_attempts(id,team_id,message_id,attempt_number,target_role_id,target_binding_id,target_binding_generation,status,error_code,error_message,created_at,started_at,completed_at) VALUES(X'{ATTACK_ATTEMPT}',X'{TEAM}',\
             X'{MESSAGE}',1,X'{ROLE}',NULL,NULL,'PENDING',NULL,NULL,9,NULL,NULL)"
        ),
    ] {
        assert_refused_and_intact(
            &pool,
            "delivery_attempts",
            ATTEMPT,
            &sql,
            "Delivery attempt deduplication is immutable",
        )
        .await?;
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Axis 4 -- identity is not repointed.
// ---------------------------------------------------------------------------

/// An `UPDATE` that renumbers or re-parents a retained row is refused.
///
/// Only columns with a clear frozen basis appear here. `roles.team_id`,
/// `tasks.team_id`, `bindings.team_id`, `role_id` and `harness_instance_id`
/// already have unconditional or child-conditioned guards in `0010`..`0015`, and
/// `run_lease_consistency_forward_migration.rs` asserts `0012`'s own message for
/// a `roles.team_id` move, so restating them here would shadow a message another
/// package proved.
#[tokio::test]
async fn identity_columns_are_not_repointed() -> Result<(), Box<dyn Error>> {
    let pool = database().await?;
    for (table, id, sql, expected) in [
        (
            "teams",
            LONE_TEAM,
            format!("UPDATE teams SET id=X'{ATTACK_TEAM}' WHERE id=X'{LONE_TEAM}'"),
            IMMUTABLE,
        ),
        (
            "roles",
            LONE_ROLE,
            format!("UPDATE roles SET id=X'{ATTACK_ROLE}' WHERE id=X'{LONE_ROLE}'"),
            IMMUTABLE,
        ),
        (
            "tasks",
            LONE_TASK,
            format!("UPDATE tasks SET id=X'{ATTACK_TASK}' WHERE id=X'{LONE_TASK}'"),
            IMMUTABLE,
        ),
        (
            "bindings",
            LONE_BINDING,
            format!("UPDATE bindings SET id=X'{ATTACK_BINDING}' WHERE id=X'{LONE_BINDING}'"),
            IMMUTABLE,
        ),
        (
            // One trigger covers `id`, `target_uri` and `checksum_sha256`, so
            // the Reference row reports the same message for all three.
            "references",
            REFERENCE,
            format!("UPDATE \"references\" SET id=X'{ATTACK_REFERENCE}' WHERE id=X'{REFERENCE}'"),
            "Reference target is immutable",
        ),
        (
            "delivery_attempts",
            ATTEMPT,
            format!("UPDATE delivery_attempts SET id=X'{ATTACK_ATTEMPT}' WHERE id=X'{ATTEMPT}'"),
            IMMUTABLE,
        ),
    ] {
        assert_refused_and_intact(&pool, table, id, &sql, expected).await?;
    }
    Ok(())
}

/// A Reference keeps the URI and hash it recorded.
///
/// DATA_CONVENTIONS section 5: a reference that stops being valid has **only**
/// its `validation_status` updated, and "不删除历史 URI/path/hash". Editing the
/// recorded target in place is the `UPDATE`-side form of the very rewrite the
/// census found through `INSERT OR REPLACE`.
#[tokio::test]
async fn a_reference_cannot_be_repointed_at_a_new_target() -> Result<(), Box<dyn Error>> {
    let pool = database().await?;
    for sql in [
        format!("UPDATE \"references\" SET target_uri='docs/other.md' WHERE id=X'{REFERENCE}'"),
        format!(
            "UPDATE \"references\" SET checksum_sha256='{}' WHERE id=X'{REFERENCE}'",
            "a".repeat(64)
        ),
    ] {
        assert_refused_and_intact(
            &pool,
            "references",
            REFERENCE,
            &sql,
            "Reference target is immutable",
        )
        .await?;
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Axis 5 -- the guards are not a shutdown. Every normal edit still works.
// ---------------------------------------------------------------------------

/// The positive control for the whole package.
///
/// If any of these were welded shut the six objects would be retained but
/// unusable, which is a different defect and not an acceptable price for
/// retention. Mirrors the frozen rules: Team `ACTIVE/PAUSED/ARCHIVED`, Role
/// `ACTIVE/DEACTIVATED`, Task `CANCELLED`, Binding status advance, Reference
/// `validation_status`, Delivery append-a-retry.
#[tokio::test]
async fn ordinary_business_edits_still_work() -> Result<(), Box<dyn Error>> {
    let pool = database().await?;

    // Team: rename, pause, archive.
    query(&format!(
        "UPDATE teams SET name='renamed',version=2,updated_at=2 WHERE id=X'{TEAM}'"
    ))
    .execute(&pool)
    .await?;
    query(&format!(
        "UPDATE teams SET status='PAUSED',version=3,updated_at=3 WHERE id=X'{TEAM}'"
    ))
    .execute(&pool)
    .await?;
    query(&format!(
        "UPDATE teams SET status='ARCHIVED',archived_at=4,version=4,updated_at=4 WHERE id=X'{TEAM}'"
    ))
    .execute(&pool)
    .await?;
    assert_eq!(
        query_scalar::<_, String>(&format!("SELECT status FROM teams WHERE id=X'{TEAM}'"))
            .fetch_one(&pool)
            .await?,
        "ARCHIVED"
    );

    // Role: deactivate, keeping its display ID stable.
    query(&format!(
        "UPDATE roles SET status='DEACTIVATED',deactivated_at=5,version=2,updated_at=5 WHERE id=X'{LONE_ROLE}'"
    ))
    .execute(&pool)
    .await?;
    assert_eq!(
        query_scalar::<_, String>(&format!("SELECT status FROM roles WHERE id=X'{LONE_ROLE}'"))
            .fetch_one(&pool)
            .await?,
        "DEACTIVATED"
    );
    // A deactivated Role keeps its display ID and its name.
    query(&format!(
        "UPDATE roles SET name='renamed role',version=3 WHERE id=X'{LONE_ROLE}'"
    ))
    .execute(&pool)
    .await?;

    // Task: edit the title, then cancel.
    query(&format!(
        "UPDATE tasks SET title='edited',version=2,updated_at=2 WHERE id=X'{LONE_TASK}'"
    ))
    .execute(&pool)
    .await?;
    query(&format!(
        "UPDATE tasks SET status='CANCELLED',cancelled_at=6,version=3,updated_at=6 WHERE id=X'{LONE_TASK}'"
    ))
    .execute(&pool)
    .await?;
    assert_eq!(
        query_scalar::<_, String>(&format!("SELECT status FROM tasks WHERE id=X'{LONE_TASK}'"))
            .fetch_one(&pool)
            .await?,
        "CANCELLED"
    );

    // Binding: a legitimate status advance keeps its snapshot editable.
    query(&format!(
        "UPDATE bindings SET status='ACTIVE',effective_from=7,version=2,updated_at=7 WHERE id=X'{LONE_BINDING}'"
    ))
    .execute(&pool)
    .await?;
    query(&format!(
        "UPDATE bindings SET configuration_snapshot='{{\"gen\":2,\"model\":\"gpt\"}}',version=3,updated_at=8 WHERE id=X'{LONE_BINDING}'"
    ))
    .execute(&pool)
    .await?;
    assert_eq!(
        query_scalar::<_, String>(&format!(
            "SELECT status || '/' || configuration_snapshot FROM bindings WHERE id=X'{LONE_BINDING}'"
        ))
        .fetch_one(&pool)
        .await?,
        "ACTIVE/{\"gen\":2,\"model\":\"gpt\"}"
    );

    // Reference: only the validation status moves.
    query(&format!(
        "UPDATE \"references\" SET validation_status='STALE',validated_at=9,version=2 WHERE id=X'{REFERENCE}'"
    ))
    .execute(&pool)
    .await?;
    assert_eq!(
        query_scalar::<_, String>(&format!(
            "SELECT validation_status || '/' || target_uri FROM \"references\" WHERE id=X'{REFERENCE}'"
        ))
        .fetch_one(&pool)
        .await?,
        "STALE/docs/spec.md"
    );

    // Delivery: advance the attempt, then append the retry that follows it.
    query(&format!(
        "UPDATE delivery_attempts SET status='IN_FLIGHT',started_at=10 WHERE id=X'{ATTEMPT}'"
    ))
    .execute(&pool)
    .await?;
    query(&format!(
        "UPDATE delivery_attempts SET status='FAILED',error_code='ADAPTER_UNAVAILABLE',completed_at=11 WHERE id=X'{ATTEMPT}'"
    ))
    .execute(&pool)
    .await?;
    query(&format!(
        "INSERT INTO delivery_attempts(id,team_id,message_id,attempt_number,target_role_id,target_binding_id,target_binding_generation,status,error_code,error_message,created_at,started_at,completed_at) VALUES(X'01921389d31174628f80cb5f79ab0013',X'{TEAM}',\
         X'{MESSAGE}',2,X'{ROLE}',NULL,NULL,'PENDING',NULL,NULL,12,NULL,NULL)"
    ))
    .execute(&pool)
    .await?;
    assert_eq!(
        query_scalar::<_, i64>(&format!(
            "SELECT COUNT(*) FROM delivery_attempts WHERE message_id=X'{MESSAGE}'"
        ))
        .fetch_one(&pool)
        .await?,
        2,
        "the retry was not appended beside the failed attempt"
    );

    // And a genuinely new Team may still be created, including while the old
    // one is archived: `INSERT OR REPLACE` on a *new* primary key is a create,
    // not a replacement, and 0028 must not confuse the two.
    query(&format!(
        "INSERT INTO teams VALUES(X'{ATTACK_TEAM}','second','ACTIVE',1,1,1,NULL)"
    ))
    .execute(&pool)
    .await?;
    assert_eq!(count(&pool, "teams").await?, 3);
    Ok(())
}

// ---------------------------------------------------------------------------
// Upgrade paths.
// ---------------------------------------------------------------------------

/// A `v27` database carrying data upgrades to `v28` without changing a row.
///
/// What the upgrade has to guarantee is that it changes *nothing*: every row
/// present before it is still present with the same values afterwards, and the
/// guards are live on the upgraded database, not just on a fresh one.
#[tokio::test]
async fn upgrading_to_0028_preserves_every_existing_row() -> Result<(), Box<dyn Error>> {
    let pool = v27_database().await?;
    seed(&pool).await?;
    let before = graph_snapshot(&pool).await?;

    // This test owns the v27 -> v28 boundary. A later migration may legitimately
    // append nullable columns; the production v29 -> v30 history test covers
    // that upgrade with every old field and both new NULL fields checked.
    let mut migrator =
        Migrator::new(std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("migrations"))
            .await?;
    migrator.migrations.to_mut().retain(|row| row.version <= 28);
    migrator.run(&pool).await?;

    assert_eq!(
        graph_snapshot(&pool).await?,
        before,
        "the upgrade changed a retained row instead of only adding guards"
    );
    assert_eq!(
        query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM _sqlx_migrations WHERE version=28 AND success=TRUE"
        )
        .fetch_one(&pool)
        .await?,
        1,
        "0028 did not record itself as applied"
    );
    for name in NEW_TRIGGERS {
        assert!(trigger_exists(&pool, name).await?, "{name} is missing");
    }
    for (table, id) in LEAF_ROWS {
        assert_refused_and_intact(
            &pool,
            table,
            id,
            &format!("DELETE FROM \"{table}\" WHERE id=X'{id}'"),
            RETAINED,
        )
        .await?;
    }
    Ok(())
}

/// A failed upgrade leaves the schema and the data exactly as they were.
///
/// The failure is produced by occupying the name of the **last** trigger `0028`
/// installs, so the seventeen created before it inside the same transaction have
/// to roll back with it.
#[tokio::test]
async fn a_failed_upgrade_has_no_side_effects() -> Result<(), Box<dyn Error>> {
    let pool = v27_database().await?;
    seed(&pool).await?;

    let occupied = *NEW_TRIGGERS.last().expect("NEW_TRIGGERS must not be empty");
    query(&format!(
        "CREATE TRIGGER {occupied} BEFORE INSERT ON messages BEGIN SELECT 1; END;"
    ))
    .execute(&pool)
    .await?;
    assert!(trigger_exists(&pool, occupied).await?);

    let before = graph_snapshot(&pool).await?;

    assert!(
        migrate(&pool).await.is_err(),
        "the occupied trigger name did not stop the migration, so this failure path proves nothing"
    );

    assert_eq!(
        graph_snapshot(&pool).await?,
        before,
        "the failed upgrade changed data"
    );
    for name in NEW_TRIGGERS {
        if name == occupied {
            continue;
        }
        assert!(
            !trigger_exists(&pool, name).await?,
            "{name} survived the failed migration, so 0028 does not run atomically"
        );
    }
    assert_eq!(
        query_scalar::<_, i64>("SELECT COUNT(*) FROM _sqlx_migrations WHERE version=28")
            .fetch_one(&pool)
            .await?,
        0,
        "a failed 0028 was recorded as applied"
    );
    Ok(())
}

// ---------------------------------------------------------------------------
// Rig self-checks (ADVERSARIAL_REVIEW_PLAYBOOK rule 1-c).
// ---------------------------------------------------------------------------

/// Bidirectional: the eighteen `0028` triggers exist on the hardened schema and
/// are absent on the `v27` baseline.
#[tokio::test]
async fn the_new_guards_are_installed() -> Result<(), Box<dyn Error>> {
    let hardened = database().await?;
    for name in NEW_TRIGGERS {
        assert!(
            trigger_exists(&hardened, name).await?,
            "{name} is missing from the hardened schema"
        );
    }
    let baseline = v27_database().await?;
    for name in NEW_TRIGGERS {
        assert!(
            !trigger_exists(&baseline, name).await?,
            "{name} already existed on the v27 baseline, so the two schemas are not distinguishable"
        );
    }
    Ok(())
}

/// This package owns version `28`, recorded exactly once and successfully.
#[tokio::test]
async fn this_package_owns_exactly_version_28() -> Result<(), Box<dyn Error>> {
    let pool = database().await?;
    assert_eq!(
        query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM _sqlx_migrations WHERE version=28 AND success=TRUE"
        )
        .fetch_one(&pool)
        .await?,
        1,
        "0028 did not record exactly one successful application"
    );
    assert_eq!(
        query_scalar::<_, String>("SELECT description FROM _sqlx_migrations WHERE version=28")
            .fetch_one(&pool)
            .await?,
        "retained entity integrity"
    );
    Ok(())
}

/// Foreign keys really are on, and the shipped `recursive_triggers` setting is
/// the `0` that makes the `OR REPLACE` arm necessary at all.
#[tokio::test]
async fn foreign_keys_are_on_and_the_seed_is_clean() -> Result<(), Box<dyn Error>> {
    let pool = database().await?;
    assert_eq!(
        query_scalar::<_, i64>("PRAGMA foreign_keys")
            .fetch_one(&pool)
            .await?,
        1
    );
    assert_eq!(
        query_scalar::<_, i64>("PRAGMA recursive_triggers")
            .fetch_one(&pool)
            .await?,
        0
    );
    assert_eq!(
        query_scalar::<_, i64>("SELECT COUNT(*) FROM pragma_foreign_key_check")
            .fetch_one(&pool)
            .await?,
        0
    );
    Ok(())
}

/// The snapshot column lists match the real tables.
///
/// Otherwise "the row is unchanged" could quietly mean "the eleven columns this
/// file happens to name are unchanged" while a twelfth was rewritten.
#[tokio::test]
async fn the_row_snapshot_covers_every_column() -> Result<(), Box<dyn Error>> {
    let pool = database().await?;
    for table in SNAPSHOT_TABLES {
        let columns = columns_of(&pool, table).await?;
        assert!(
            columns.iter().any(|column| column == "id"),
            "{table} has no id column, so this file cannot probe it"
        );
        let rendered: String = query_scalar::<_, String>(&format!(
            "SELECT {} FROM \"{table}\" ORDER BY rowid LIMIT 1",
            projection(&columns)
        ))
        .fetch_one(&pool)
        .await?;
        let keys: Vec<&str> = rendered
            .split('\n')
            .map(|line| line.split_once('=').map_or(line, |(key, _)| key))
            .collect();
        for column in &columns {
            assert!(
                keys.contains(&column.as_str()),
                "{table}: the snapshot never rendered column {column} (saw {keys:?})"
            );
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Vector builders.
// ---------------------------------------------------------------------------

/// `INSERT OR REPLACE` of the *same* row, with one visible value changed.
///
/// The value that changes differs per table so no two vectors are the same
/// statement wearing a different label.
fn replacement_of(table: &str, victim: &str, attacker: &str) -> String {
    match table {
        "teams" => format!(
            "INSERT OR REPLACE INTO teams VALUES(X'{attacker}','hijacked','ARCHIVED',9,1,1,9)"
        ),
        "roles" => format!(
            "INSERT OR REPLACE INTO roles VALUES(X'{attacker}',X'{TEAM}','L-1','hijacked','ACTIVE',9,1,1,NULL)"
        ),
        "tasks" => format!(
            "INSERT OR REPLACE INTO tasks VALUES(X'{attacker}',X'{TEAM}',X'{ROLE}','T-L','hijacked','','DONE',9,1,1,NULL,9)"
        ),
        "bindings" => format!(
            "INSERT OR REPLACE INTO bindings VALUES(X'{attacker}',X'{TEAM}',X'{ROLE}',X'{HARNESS}',X'{ADAPTER}','PENDING_CHECK',2,9,'{{\"hijacked\":true}}',1,1,NULL,NULL)"
        ),
        "references" => format!(
            "INSERT OR REPLACE INTO \"references\" VALUES(X'{attacker}',X'{TEAM}','TASK',X'{REFERENCED_TASK}','WORKSPACE_PATH','docs/hijacked.md','VALID',NULL,9,7,7)"
        ),
        "delivery_attempts" => format!(
            "INSERT OR REPLACE INTO delivery_attempts(id,team_id,message_id,attempt_number,target_role_id,target_binding_id,target_binding_generation,status,error_code,error_message,created_at,started_at,completed_at) VALUES(X'{attacker}',X'{TEAM}',X'{MESSAGE}',1,X'{ROLE}',NULL,NULL,'PENDING',NULL,NULL,9,NULL,NULL)"
        ),
        other => panic!("no replacement vector is declared for {other} ({victim})"),
    }
}

/// One replacement per **real** business unique key, each with a different id.
fn business_key_vectors() -> Vec<(&'static str, &'static str, String)> {
    vec![
        // roles_team_display_id_unique (team_id, display_id)
        (
            "roles",
            LONE_ROLE,
            format!(
                "INSERT OR REPLACE INTO roles VALUES(X'{ATTACK_ROLE}',X'{TEAM}','L-1','hijacked','ACTIVE',9,1,1,NULL)"
            ),
        ),
        // tasks_team_display_id_unique (team_id, display_id)
        (
            "tasks",
            LONE_TASK,
            format!(
                "INSERT OR REPLACE INTO tasks VALUES(X'{ATTACK_TASK}',X'{TEAM}',X'{ROLE}','T-L','hijacked','','DONE',9,1,1,NULL,9)"
            ),
        ),
        // bindings_role_generation_unique (role_id, binding_generation)
        (
            "bindings",
            LONE_BINDING,
            format!(
                "INSERT OR REPLACE INTO bindings VALUES(X'{ATTACK_BINDING}',X'{TEAM}',X'{ROLE}',X'{HARNESS}',X'{ADAPTER}','PENDING_CHECK',2,9,'{{\"hijacked\":true}}',1,1,NULL,NULL)"
            ),
        ),
        // delivery_attempts_single_active (message_id) WHERE status IN (PENDING, IN_FLIGHT)
        (
            "delivery_attempts",
            ATTEMPT,
            format!(
                "INSERT OR REPLACE INTO delivery_attempts(id,team_id,message_id,attempt_number,target_role_id,target_binding_id,target_binding_generation,status,error_code,error_message,created_at,started_at,completed_at) VALUES(X'{ATTACK_ATTEMPT}',X'{TEAM}',X'{MESSAGE}',2,X'{ROLE}',NULL,NULL,'PENDING',NULL,NULL,9,NULL,NULL)"
            ),
        ),
    ]
}

/// The attacker's primary key, read back out of the vector SQL.
fn extract_attack_id(sql: &str) -> String {
    let start = sql.find("X'").expect("the vector has no blob literal") + 2;
    let end = start + 32;
    sql[start..end].to_string()
}
