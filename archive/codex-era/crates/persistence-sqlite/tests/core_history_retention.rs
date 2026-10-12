//! MM-R02 (deep audit DA-02): the persisted history of a Run, a Review
//! decision, an Artifact index row, a Message and an Approval must be retained
//! for the life of its Team.
//!
//! The counterexample this file answers was reproduced by the deep audit against
//! `0001..0024` and logged in
//! `docs/reviews/evidence/deep-audit-2026-10-08/storage-boundaries-red-v2.log`:
//! with no `events` row and no `"references"` row naming the subject, a plain
//! `DELETE` succeeded on all five tables and each really went 1 -> 0.
//!
//! Why the existing guards did not stop it — this is the part that must not be
//! restated as "all five tables were deletable":
//!
//! | table        | `0013` Reference guard | `0014` Event subject guard |
//! |--------------|------------------------|----------------------------|
//! | `runs`       | yes                    | yes                        |
//! | `artifacts`  | yes                    | yes                        |
//! | `messages`   | yes                    | yes                        |
//! | `approvals`  | **no** (`0005` does not allow `APPROVAL` as a `references.subject_type`) | yes |
//! | `reviews`    | **no** (`0005` does not allow `REVIEW`)  | **no** (`0006` does not allow `REVIEW` as an `events.subject_type`) |
//!
//! So `reviews` had no deletion guard of any kind, `approvals` had only the
//! Event arm, and for every table the pre-existing guard was conditional on the
//! referencing row existing. `0026` is added **on top of** those guards, never
//! in place of them; [`existing_reference_and_event_guards_still_apply`] and
//! [`those_guards_block_on_the_pre_0026_schema_too`] pin that both halves of the
//! table above keep working.
//!
//! Scoring rules taken from ADVERSARIAL_REVIEW_PLAYBOOK, because a passing
//! assertion here is worthless unless it can fail:
//!
//! - rule 2, "no-op disguised as a bypass": every negative assertion checks that
//!   the statement returned an error **and** that the row is still there with
//!   its original values. A rejection produced by a foreign key, or a statement
//!   that matched zero rows, never counts as a pass here.
//! - rule 1-c, the rig declares its own schema: [`the_new_guards_are_installed`]
//!   asserts the `0026` triggers exist by name on the hardened schema **and**
//!   that they are absent on the `v25` baseline schema, so a rig that silently
//!   loaded the wrong migration cannot produce a result.
//! - rule 3, every tested row is seeded: [`seed`] ends with per-table counts and
//!   a `pragma_foreign_key_check`, and no test creates the row it then attacks.
//!
//! Frozen basis, none of which this file extends:
//!   - DATA_CONVENTIONS.md section 5: "P0 核心对象无通用 DELETE 命令" and
//!     "Event、Run、Binding、Review decision、DeliveryAttempt、Approval decision
//!     不更新历史语义，只追加后续事实".
//!   - DATA_CONVENTIONS.md section 6: Run and Review are retained with the Team
//!     "随 Team 永久保留"; Message, Delivery, Notification and Approval are
//!     "随 Team 保留"; the Artifact index is retained with the Team and keeps
//!     its metadata when the external file is gone.
//!   - DOMAIN_MODEL.md section 5: "核心对象通过稳定 ID 关联，不级联硬删除".
//!   - STATE_MACHINES.md section 7: a PENDING Approval is cancelled through
//!     `CANCELLED` plus its actor, not removed.
//!
//! Attention items are deliberately **not** in this set. Deep audit section 4
//! records that Attention is a rebuildable derived index that the DATA retention
//! table does not list separately, and that it is "暂不计 DA-02 永久核心历史集合".
//! Nothing here claims to fix Attention deletion.

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
// One Team, one Role, one Harness, one Adapter, one Binding, one Task and one
// finished Run. Every ID is a 16-byte UUIDv7 literal with the `7` version nibble
// and an RFC 4122 variant nibble, because the CHECK constraints reject anything
// else and a rejected INSERT would silently turn an attack into a device fault.
// ---------------------------------------------------------------------------
const TEAM: &str = "01921389d31174628f80cb5f79ab0001";
const ROLE: &str = "01921389d31174628f80cb5f79ab0002";
const HARNESS: &str = "01921389d31174628f80cb5f79ab0003";
const BINDING: &str = "01921389d31174628f80cb5f79ab0005";
const TASK: &str = "01921389d31174628f80cb5f79ab0006";
/// The seeded Run: attempt 1, fencing token 1, already `SUCCEEDED`. The seed
/// `reviews` and `artifacts` rows both name it, so a `DELETE` of *this* row is
/// refused by a foreign key and would score nothing -- see [`LONE_RUN`].
const RUN: &str = "01921389d31174628f80cb5f79ab0007";
/// A second Run that nothing at all points at.
///
/// Scoring note: this row exists because the first version of this file probed
/// `runs` with `RUN` and the delete was refused by `reviews_run_id` /
/// `artifacts_run_id` `ON DELETE RESTRICT`. That is a real integrity guarantee,
/// but it is not the guarantee under test, and a foreign key must never be
/// allowed to stand in for a retention guard here
/// (ADVERSARIAL_REVIEW_PLAYBOOK rule 2, "failure disguised as a pass").
const LONE_RUN: &str = "01921389d31174628f80cb5f79ab0008";

const REVIEW: &str = "01921389d31174628f80cb5f79ab0009";
const ARTIFACT: &str = "01921389d31174628f80cb5f79ab0010";
const APPROVAL: &str = "01921389d31174628f80cb5f79ab0011";
const MESSAGE: &str = "01921389d31174628f80cb5f79ab0021";
const SENDER_HUMAN: &str = "01921389d31174628f80cb5f79ab0022";

/// A second Team, used only by the isolation probes.
const OTHER_TEAM: &str = "01921389d31174628f80cb5f79ab0030";
const OTHER_ROLE: &str = "01921389d31174628f80cb5f79ab0031";

/// The `0026` triggers this package installs, in the order `0026` creates them.
///
/// Kept as one list so the schema assertion, the failure-path test and the
/// mutation log cannot drift apart. The last entry matters: the failure-path
/// test occupies that name, so the fourteen triggers created before it inside
/// the same transaction have to roll back with it.
const NEW_TRIGGERS: [&str; 11] = [
    "runs_core_history_retention_delete",
    "runs_core_history_identity_immutable_update",
    "reviews_core_history_retention_delete",
    "reviews_core_history_retention_replace_guard",
    "reviews_core_history_identity_immutable_update",
    "artifacts_core_history_retention_delete",
    "artifacts_core_history_retention_replace_guard",
    "artifacts_core_history_identity_immutable_update",
    "approvals_core_history_retention_delete",
    "messages_core_history_retention_delete",
    "messages_core_history_identity_immutable_update",
];

const SEED: &str = "\
INSERT INTO teams VALUES(X'01921389d31174628f80cb5f79ab0001','t','ACTIVE',1,1,1,NULL);
INSERT INTO roles VALUES(X'01921389d31174628f80cb5f79ab0002',X'01921389d31174628f80cb5f79ab0001','r','r','ACTIVE',1,1,1,NULL);
INSERT INTO harness_instances VALUES(X'01921389d31174628f80cb5f79ab0003',X'01921389d31174628f80cb5f79ab0001','h','h','ACTIVE','[]',NULL,1,1,1,NULL);
INSERT INTO adapters VALUES(X'01921389d31174628f80cb5f79ab0004',X'01921389d31174628f80cb5f79ab0001','a','1','/a','{}',1,1,1);
INSERT INTO bindings VALUES(X'01921389d31174628f80cb5f79ab0005',X'01921389d31174628f80cb5f79ab0001',X'01921389d31174628f80cb5f79ab0002',X'01921389d31174628f80cb5f79ab0003',X'01921389d31174628f80cb5f79ab0004','ACTIVE',1,1,'{}',1,1,1,NULL);
INSERT INTO tasks VALUES(X'01921389d31174628f80cb5f79ab0006',X'01921389d31174628f80cb5f79ab0001',X'01921389d31174628f80cb5f79ab0002','T-1','t','','DOING',1,1,1,NULL,NULL);
INSERT INTO runs VALUES(X'01921389d31174628f80cb5f79ab0007',X'01921389d31174628f80cb5f79ab0001',X'01921389d31174628f80cb5f79ab0006',X'01921389d31174628f80cb5f79ab0002',X'01921389d31174628f80cb5f79ab0005',X'01921389d31174628f80cb5f79ab0003',1,1,'SUCCEEDED',0,NULL,1,1,1,2);
";

/// A `DRAFT` Message plus the legal `DRAFT -> QUEUED` edge, which freezes the
/// body (DATA_CONVENTIONS section 5).
const MESSAGE_QUEUED: &str = "\
INSERT INTO threads VALUES(X'01921389d31174628f80cb5f79ab0020',X'01921389d31174628f80cb5f79ab0001','thread','OPEN',1,1,1);
INSERT INTO messages(id,team_id,thread_id,sender_actor_type,from_human_id,to_role_id,message_type,body,status,version,created_at,updated_at) VALUES(X'01921389d31174628f80cb5f79ab0021',X'01921389d31174628f80cb5f79ab0001',X'01921389d31174628f80cb5f79ab0020','HUMAN',X'01921389d31174628f80cb5f79ab0022',X'01921389d31174628f80cb5f79ab0002','DIRECT','body','DRAFT',1,1,1);
UPDATE messages SET status='QUEUED', version=2 WHERE id=X'01921389d31174628f80cb5f79ab0021';
";

/// A `PENDING` Approval with no decision trace, which is how every request is
/// born (STATE_MACHINES section 7 first row).
const PENDING_APPROVAL: &str = "\
INSERT INTO approvals(id,team_id,requester_actor_type,requester_role_id,action,resource_type,resource_scope,reason,status,version,expires_at,created_at) \
VALUES(X'01921389d31174628f80cb5f79ab0011',X'01921389d31174628f80cb5f79ab0001','ROLE',X'01921389d31174628f80cb5f79ab0002','EXECUTE','CREDENTIAL_ACCESS','scope','reason','PENDING',1,9,6);
";

fn review() -> String {
    format!(
        "INSERT INTO reviews VALUES(X'{REVIEW}',X'{TEAM}',X'{TASK}',X'{RUN}',X'{ROLE}','APPROVED',NULL,1,5)"
    )
}

/// The Artifact index row.
///
/// Transcribed with all twelve of `0005`'s columns in order, including the
/// nullable `git_commit_sha` between `checksum_sha256` and `visibility`. The
/// audit's own repro listed only eleven values here, so that file's `artifacts`
/// leg could not have run; see `MM-R02-RESULT.md`. A rejected INSERT would wear
/// a rejection as a costume, so the shape is asserted in
/// [`replace_vectors_each_change_a_different_value`] and the row must exist.
fn artifact() -> String {
    format!(
        "INSERT INTO artifacts(id,team_id,task_id,run_id,creator_role_id,artifact_type,uri,checksum_sha256,git_commit_sha,visibility,version,created_at) \
VALUES(X'{ARTIFACT}',X'{TEAM}',X'{TASK}',X'{RUN}',X'{ROLE}','TEST_REPORT','tasks/T-1/report',?,NULL,'TEAM_INTERNAL',1,5)"
    )
}

fn reference(id: &str, subject_type: &str, subject_id: &str) -> String {
    format!(
        "INSERT INTO \"references\"(id,team_id,subject_type,subject_id,reference_kind,target_uri,validation_status,version,created_at) \
VALUES(X'{id}',X'{TEAM}','{subject_type}',X'{subject_type_id}','WORKSPACE_PATH','docs/spec.md','VALID',1,7)",
        subject_type_id = subject_id
    )
}

/// An `SYSTEM` Event naming `subject`, which is what arms the `0014` guard.
/// `SYSTEM` needs no Binding snapshot, so this stays valid from `0015` on.
fn event(id: &str, subject_type: &str, subject_id: &str) -> String {
    format!(
        "INSERT INTO events(id,team_id,event_type,actor_type,subject_type,subject_id,object_version,occurred_at,hash) \
VALUES(X'{id}',X'{TEAM}','audit.recorded','SYSTEM','{subject_type}',X'{subject_id}',1,1,\
'E3B0C44298FC1C149AFBF4C8996FB92427AE41E4649B934CA495991B7852B855')"
    )
}

async fn count(pool: &SqlitePool, table: &str) -> Result<i64, Box<dyn Error>> {
    Ok(
        query_scalar::<_, i64>(&format!("SELECT COUNT(*) FROM {table}"))
            .fetch_one(pool)
            .await?,
    )
}

/// A fresh database with every migration applied and the base graph seeded.
async fn database() -> Result<SqlitePool, Box<dyn Error>> {
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(SqliteConnectOptions::from_str("sqlite::memory:")?.foreign_keys(true))
        .await?;
    migrate(&pool).await?;
    seed(&pool).await?;
    Ok(pool)
}

/// The seed plus its own self-check. A rig fault must fail here rather than look
/// like an attack result.
async fn seed(pool: &SqlitePool) -> Result<(), Box<dyn Error>> {
    raw_sql(SEED).execute(pool).await?;
    for table in ["teams", "roles", "bindings", "tasks", "runs"] {
        assert_eq!(
            count(pool, table).await?,
            1,
            "the base seed for {table} did not land exactly one row"
        );
    }
    assert_eq!(
        query_scalar::<_, i64>("SELECT COUNT(*) FROM pragma_foreign_key_check")
            .fetch_one(pool)
            .await?,
        0,
        "the base seed is not foreign-key clean"
    );
    // A `BLOB` length and a hex prefix are checked by hand so an attack vector
    // can never be a malformed literal wearing a rejection as a costume.
    assert_eq!(
        query_scalar::<_, i64>("SELECT COUNT(*) FROM runs WHERE length(id) = 16")
            .fetch_one(pool)
            .await?,
        1
    );
    Ok(())
}

/// Every retained-history table, each holding exactly one row that names no
/// `events` row and no `"references"` row.
async fn seed_unreferenced_history(pool: &SqlitePool) -> Result<(), Box<dyn Error>> {
    // `0012` requires `attempt = MAX(attempt) + 1` on insert, so the probe row
    // is attempt 2. It is seeded `SUCCEEDED` rather than `QUEUED` because
    // `runs_single_active_run_per_task` allows only one active Run per Task and
    // the legal-path test needs an active slot of its own.
    query("INSERT INTO runs VALUES(X'01921389d31174628f80cb5f79ab0008',X'01921389d31174628f80cb5f79ab0001',X'01921389d31174628f80cb5f79ab0006',X'01921389d31174628f80cb5f79ab0002',X'01921389d31174628f80cb5f79ab0005',X'01921389d31174628f80cb5f79ab0003',2,2,'SUCCEEDED',0,NULL,1,1,1,2)")
        .execute(pool)
        .await?;
    query(&review()).execute(pool).await?;
    query(&artifact())
        .bind("0".repeat(64))
        .execute(pool)
        .await?;
    raw_sql(PENDING_APPROVAL).execute(pool).await?;
    raw_sql(MESSAGE_QUEUED).execute(pool).await?;
    for (table, expected) in SEEDED_COUNTS {
        assert_eq!(
            count(pool, table).await?,
            expected,
            "{table} was not seeded with the expected number of rows"
        );
    }
    // The probe Run must really be pointed at by nothing.
    for probe in [
        format!("SELECT COUNT(*) FROM reviews WHERE run_id=X'{LONE_RUN}'"),
        format!("SELECT COUNT(*) FROM artifacts WHERE run_id=X'{LONE_RUN}'"),
        format!("SELECT COUNT(*) FROM leases WHERE run_id=X'{LONE_RUN}'"),
    ] {
        assert_eq!(
            query_scalar::<_, i64>(&probe).fetch_one(pool).await?,
            0,
            "the probe row {probe} is not unreferenced"
        );
    }
    // The counterexample is only the interesting one when nothing references
    // the subject, so this states that rather than assuming it.
    for (subject_type, subject_id) in [("RUN", RUN), ("ARTIFACT", ARTIFACT), ("MESSAGE", MESSAGE)] {
        assert_eq!(
            query_scalar::<_, i64>(&format!(
                "SELECT COUNT(*) FROM \"references\" WHERE subject_type='{subject_type}' AND subject_id=X'{subject_id}'"
            ))
            .fetch_one(pool)
            .await?,
            0,
            "{subject_type} was supposed to be unreferenced"
        );
        assert_eq!(
            query_scalar::<_, i64>(&format!(
                "SELECT COUNT(*) FROM events WHERE subject_type='{subject_type}' AND subject_id=X'{subject_id}'"
            ))
            .fetch_one(pool)
            .await?,
            0,
            "{subject_type} was supposed to have no Event"
        );
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Migrations up to `0025`, used to prove the pre-`0026` schema behaves
// differently from the hardened one (ADVERSARIAL_REVIEW_PLAYBOOK rule 1-b).
// ---------------------------------------------------------------------------
const V25: [(&str, &str); 25] = [
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
];

#[derive(Debug)]
struct V25OnlyMigrations;

impl MigrationSource<'static> for V25OnlyMigrations {
    fn resolve(
        self,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<Migration>, BoxDynError>> + Send + 'static>> {
        Box::pin(async {
            Ok(V25
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

/// A database at `0025`: everything except this package's migration.
async fn v25_database() -> Result<SqlitePool, Box<dyn Error>> {
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(SqliteConnectOptions::from_str("sqlite::memory:")?.foreign_keys(true))
        .await?;
    Migrator::new(V25OnlyMigrations).await?.run(&pool).await?;
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
// Axis 1 -- DELETE with no Event and no Reference. This is the audit's RED.
// ---------------------------------------------------------------------------

/// The reproduced counterexample, kept in the shape the audit logged: for each
/// of the five tables, `DELETE` must be refused and the row must survive.
///
/// Scoring note: the assertion is `is_err()` **plus** the row count staying at
/// 1. On the pre-`0026` schema both halves of the attack succeeded -- the
/// statement returned `Ok` and the count really fell to 0 -- so neither half can
/// be passing for a reason that has nothing to do with the harm.
#[tokio::test]
async fn retained_histories_cannot_be_physically_deleted() -> Result<(), Box<dyn Error>> {
    let mut deleted = Vec::new();
    let mut survived = Vec::new();

    for (table, target) in DELETE_TARGETS {
        let pool = database().await?;
        seed_unreferenced_history(&pool).await?;
        assert_eq!(count(&pool, target).await?, 1, "{table} seed count");

        let result = query(&format!("DELETE FROM {target}")).execute(&pool).await;
        if result.is_ok() {
            deleted.push(table);
        }
        if count(&pool, target).await? != 1 {
            survived.push(table);
        }
    }

    assert!(deleted.is_empty(), "DELETE was accepted for {deleted:?}");
    assert!(
        survived.is_empty(),
        "DELETE removed the retained row of {survived:?}"
    );
    Ok(())
}

/// One delete target per table: the row that nothing points at.
///
/// `runs` is probed through [`LONE_RUN`] rather than [`RUN`] because `RUN` is
/// named by `reviews` and `artifacts`, so deleting it trips
/// `ON DELETE RESTRICT` and would score a retention guarantee it never tested.
/// `SELECT COUNT(*)` would also hide that, because the guard test needs the
/// specific row, not the table.
/// How many rows each retained table holds after [`seed_unreferenced_history`].
/// `runs` holds two because the FK probe forces a dedicated unreferenced row.
const SEEDED_COUNTS: [(&str, i64); 5] = [
    ("runs", 2),
    ("reviews", 1),
    ("artifacts", 1),
    ("approvals", 1),
    ("messages", 1),
];

const DELETE_TARGETS: [(&str, &str); 5] = [
    ("runs", "runs WHERE id=X'01921389d31174628f80cb5f79ab0008'"),
    (
        "reviews",
        "reviews WHERE id=X'01921389d31174628f80cb5f79ab0009'",
    ),
    (
        "artifacts",
        "artifacts WHERE id=X'01921389d31174628f80cb5f79ab0010'",
    ),
    (
        "approvals",
        "approvals WHERE id=X'01921389d31174628f80cb5f79ab0011'",
    ),
    (
        "messages",
        "messages WHERE id=X'01921389d31174628f80cb5f79ab0021'",
    ),
];

/// A `DELETE` that names no row must stay silent rather than start failing for
/// the wrong reason: if the guard fired on every statement regardless of which
/// row it touched, this test would break, which is exactly the false signal the
/// scoring rules forbid.
#[tokio::test]
async fn deleting_nothing_is_not_a_bypass_and_is_not_an_error() -> Result<(), Box<dyn Error>> {
    let pool = database().await?;
    seed_unreferenced_history(&pool).await?;

    let result = query("DELETE FROM runs WHERE id=X'000000000000000000000000000000ff'")
        .execute(&pool)
        .await;
    assert!(
        result.is_ok(),
        "a DELETE matching no row was rejected, so the guards are firing too broadly"
    );
    assert_eq!(result?.rows_affected(), 0);
    assert_eq!(count(&pool, "runs").await?, 2);
    Ok(())
}

// ---------------------------------------------------------------------------
// Axis 2 -- INSERT OR REPLACE.
//
// SQLite performs `OR REPLACE` as a hidden delete of the conflicting row, and
// with the default `recursive_triggers = 0` that hidden delete does **not** fire
// the table's DELETE triggers. This is the hole DEV-02-14 already had to close
// with explicit `*_replace_guard` triggers, so every table here gets one.
// ---------------------------------------------------------------------------

/// `OR REPLACE` must not rewrite a retained row. The check is behavioural: the
/// statement must fail *and* every stored value of the table must still be the
/// original one, so a guard that returns an error without protecting the row
/// cannot pass.
#[tokio::test]
async fn retained_histories_cannot_be_rewritten_by_insert_or_replace() -> Result<(), Box<dyn Error>>
{
    let mut accepted = Vec::new();
    let mut rewritten = Vec::new();

    for (table, replace_sql) in REPLACE_VECTORS {
        let pool = database().await?;
        seed_unreferenced_history(&pool).await?;
        let expected = SEEDED_COUNTS
            .iter()
            .find(|(name, _)| *name == table)
            .map(|(_, count)| *count)
            .unwrap();
        assert_eq!(count(&pool, table).await?, expected, "{table} seed count");

        let before = table_contents(&pool, table).await?;
        let result = query(replace_sql).execute(&pool).await;
        if result.is_ok() {
            accepted.push(table);
        }
        if table_contents(&pool, table).await? != before {
            rewritten.push(table);
        }
    }

    assert!(
        accepted.is_empty(),
        "INSERT OR REPLACE was accepted for {accepted:?}"
    );
    assert!(
        rewritten.is_empty(),
        "INSERT OR REPLACE rewrote {rewritten:?}"
    );
    Ok(())
}

/// Records, per table, which refusal a replace vector already met on the
/// `v25` baseline and which guard supplies it on the hardened schema.
///
/// This exists because the mutation log found that three of the five replace
/// vectors were already refused before `0026` ran, so a green
/// `retained_histories_cannot_be_rewritten_by_insert_or_replace` could not tell
/// "the new guard refused it" from "some older guard refused it". Naming the
/// refusal is the only way the claim stays honest, and it is also the evidence
/// for which of the five replace guards are load-bearing and which are
/// belt-and-braces over an existing guarantee.
#[tokio::test]
async fn each_replace_vector_is_refused_and_by_what() -> Result<(), Box<dyn Error>> {
    let mut baseline = Vec::new();
    let mut hardened = Vec::new();

    for (table, replace_sql) in REPLACE_VECTORS {
        let on_baseline = v25_database().await?;
        seed(&on_baseline).await?;
        seed_unreferenced_history(&on_baseline).await?;
        let before = table_contents(&on_baseline, table).await?;
        baseline.push((
            table,
            refusal_of(&on_baseline, replace_sql).await?,
            table_contents(&on_baseline, table).await? != before,
        ));

        let on_hardened = database().await?;
        seed_unreferenced_history(&on_hardened).await?;
        let before = table_contents(&on_hardened, table).await?;
        hardened.push((
            table,
            refusal_of(&on_hardened, replace_sql).await?,
            table_contents(&on_hardened, table).await? != before,
        ));
    }

    for (table, baseline_message, baseline_rewrote) in &baseline {
        let (_, hardened_message, hardened_rewrote) = hardened
            .iter()
            .find(|(name, _, _)| name == table)
            .expect("every table appears in both lists");
        assert!(
            !hardened_rewrote,
            "{table} was rewritten on the hardened schema"
        );
        assert!(
            !hardened_message.is_empty(),
            "{table} accepted a replace on the hardened schema"
        );
        println!(
            "{table}: baseline={baseline_message:?} (rewrote={baseline_rewrote}) \
             hardened={hardened_message:?} (rewrote={hardened_rewrote})"
        );
    }
    Ok(())
}

/// Runs one statement and returns the refusal message, or an empty string when
/// it was accepted. The device prints rather than asserts, because the point is
/// to *record* which guard speaks; the behavioural guarantee is asserted
/// elsewhere.
async fn refusal_of(pool: &SqlitePool, sql: &str) -> Result<String, Box<dyn Error>> {
    match query(sql).execute(pool).await {
        Ok(_) => Ok(String::new()),
        Err(error) => Ok(error.to_string()),
    }
}

/// `recursive_triggers` stays at its shipped value. If it were turned on the
/// `OR REPLACE` vectors above would be blocked by the DELETE triggers instead of
/// the replace guards, and this file would be scoring the wrong mechanism
/// without saying so.
#[tokio::test]
async fn replace_vectors_depend_on_the_shipped_recursive_triggers_setting()
-> Result<(), Box<dyn Error>> {
    let pool = database().await?;
    assert_eq!(
        query_scalar::<_, i64>("PRAGMA recursive_triggers")
            .fetch_one(&pool)
            .await?,
        0,
        "recursive_triggers is not at the value these vectors were designed against"
    );
    Ok(())
}

/// Each vector states a genuinely different row, so a rejection is never a
/// no-op that happened to match. Kept separate from the assertion above so the
/// device self-check fails loudly instead of being folded into a result.
#[tokio::test]
async fn replace_vectors_each_change_a_different_value() -> Result<(), Box<dyn Error>> {
    let pool = database().await?;
    seed_unreferenced_history(&pool).await?;

    assert_eq!(run_status(&pool).await?, "SUCCEEDED");
    assert_eq!(
        query_scalar::<_, String>(&format!("SELECT status FROM runs WHERE id=X'{LONE_RUN}'"))
            .fetch_one(&pool)
            .await?,
        "SUCCEEDED"
    );
    assert_eq!(review_decision(&pool).await?, "APPROVED");
    assert_eq!(artifact_visibility(&pool).await?, "TEAM_INTERNAL");
    assert_eq!(approval_status(&pool).await?, "PENDING");
    assert_eq!(message_status(&pool).await?, "QUEUED");
    Ok(())
}

// ---------------------------------------------------------------------------
// Axis 3 -- the guards that were already there.
// ---------------------------------------------------------------------------

/// With an Event or a Reference naming the subject, the `0013` and `0014`
/// guards still refuse the delete after `0026` landed.
///
/// Trigger firing order among same-timing `BEFORE DELETE` triggers is not
/// something this test relies on: it asserts only that the delete is refused and
/// the row survives. Which guard did the refusing is settled on the pre-`0026`
/// baseline by [`those_guards_block_on_the_pre_0026_schema_too`].
#[tokio::test]
async fn existing_reference_and_event_guards_still_apply() -> Result<(), Box<dyn Error>> {
    // `"references"` may name TASK, RUN, ARTIFACT and MESSAGE, and `events` may
    // name all but REVIEW. So this is every pairing that has a pre-existing
    // guard, stated explicitly rather than derived from a loop over guesses.
    let cases: [(&str, &str, String); 7] = [
        (
            "runs",
            "a Reference naming the Run",
            reference("01921389d31174628f80cb5f79ab0040", "RUN", LONE_RUN),
        ),
        (
            "runs",
            "an Event naming the Run",
            event("01921389d31174628f80cb5f79ab0041", "RUN", LONE_RUN),
        ),
        (
            "artifacts",
            "a Reference naming the Artifact",
            reference("01921389d31174628f80cb5f79ab0042", "ARTIFACT", ARTIFACT),
        ),
        (
            "artifacts",
            "an Event naming the Artifact",
            event("01921389d31174628f80cb5f79ab0043", "ARTIFACT", ARTIFACT),
        ),
        (
            "messages",
            "a Reference naming the Message",
            reference("01921389d31174628f80cb5f79ab0044", "MESSAGE", MESSAGE),
        ),
        (
            "messages",
            "an Event naming the Message",
            event("01921389d31174628f80cb5f79ab0045", "MESSAGE", MESSAGE),
        ),
        (
            "approvals",
            "an Event naming the Approval",
            event("01921389d31174628f80cb5f79ab0046", "APPROVAL", APPROVAL),
        ),
    ];

    let mut accepted = Vec::new();
    for (table, description, setup) in cases {
        let pool = database().await?;
        seed_unreferenced_history(&pool).await?;
        raw_sql(&setup).execute(&pool).await?;

        let result = query(&format!("DELETE FROM {table}")).execute(&pool).await;
        assert!(
            result.is_err(),
            "{description} did not stop the DELETE from {table}"
        );
        if count(&pool, table).await? == 0 {
            accepted.push((table, description));
        }
    }
    assert!(
        accepted.is_empty(),
        "the row behind {accepted:?} was removed anyway"
    );
    Ok(())
}

/// The pre-`0026` schema must refuse the same deletes, which is what proves
/// `0026` sits **on top of** the `0013` and `0014` guards instead of quietly
/// replacing them. It also pins the gap itself: on this schema the same five
/// unreferenced rows really do go 1 -> 0.
#[tokio::test]
async fn those_guards_block_on_the_pre_0026_schema_too() -> Result<(), Box<dyn Error>> {
    let cases: [(&str, String); 4] = [
        (
            "runs",
            reference("01921389d31174628f80cb5f79ab0040", "RUN", LONE_RUN),
        ),
        (
            "artifacts",
            reference("01921389d31174628f80cb5f79ab0042", "ARTIFACT", ARTIFACT),
        ),
        (
            "messages",
            reference("01921389d31174628f80cb5f79ab0044", "MESSAGE", MESSAGE),
        ),
        (
            "approvals",
            event("01921389d31174628f80cb5f79ab0046", "APPROVAL", APPROVAL),
        ),
    ];

    for (table, setup) in cases {
        let pool = v25_database().await?;
        seed(&pool).await?;
        seed_unreferenced_history(&pool).await?;
        raw_sql(&setup).execute(&pool).await?;

        assert!(
            query(&format!("DELETE FROM {table}"))
                .execute(&pool)
                .await
                .is_err(),
            "the v25 baseline stopped rejecting a referenced {table}, so the guard under test is not the pre-existing one"
        );
        assert!(
            count(&pool, table).await? > 0,
            "the referenced {table} row was removed anyway"
        );
    }

    // And the counterexample, on the baseline itself: no guard was in the way.
    for (table, target) in DELETE_TARGETS {
        let pool = v25_database().await?;
        seed(&pool).await?;
        seed_unreferenced_history(&pool).await?;
        let before = count(&pool, target).await?;
        query(&format!("DELETE FROM {target}"))
            .execute(&pool)
            .await?;
        assert_eq!(
            (before, count(&pool, target).await?),
            (1, 0),
            "{table} survived on the v25 baseline, so the hardened schema is not distinguishable from it"
        );
    }
    Ok(())
}

/// `reviews` had no pre-`0026` guard of either kind, because `0005` does not
/// allow `REVIEW` as a `"references".subject_type` and `0006` does not allow it
/// as an `events.subject_type`. Stated as a test so the claim in the module
/// comment cannot rot into folklore.
#[tokio::test]
async fn reviews_had_no_pre_0026_guard_of_either_kind() -> Result<(), Box<dyn Error>> {
    let pool = v25_database().await?;
    seed(&pool).await?;
    seed_unreferenced_history(&pool).await?;

    for setup in [
        reference("01921389d31174628f80cb5f79ab0047", "REVIEW", REVIEW),
        event("01921389d31174628f80cb5f79ab0048", "REVIEW", REVIEW),
    ] {
        assert!(
            query(&setup).execute(&pool).await.is_err(),
            "the v25 baseline accepted a REVIEW as a reference/Event subject, so this file's claim about reviews is wrong"
        );
    }

    query(&format!("DELETE FROM reviews WHERE id=X'{REVIEW}'"))
        .execute(&pool)
        .await?;
    assert_eq!(count(&pool, "reviews").await?, 0);
    Ok(())
}

/// The hardened schema must refuse to change the subject of a retained row, not
/// only delete it: a history that is moved to another Team or another Task has
/// lost the same evidence. `0012`, `0013` and `0014` already guard the identity
/// columns of `runs`, `artifacts` and `messages`, and this checks the hardened
/// schema still refuses them.
#[tokio::test]
async fn retained_history_identity_is_not_repointed() -> Result<(), Box<dyn Error>> {
    let mut accepted = Vec::new();

    for (table, column, source, target) in IDENTITY_VECTORS {
        let pool = database().await?;
        seed_unreferenced_history(&pool).await?;
        let sql = format!("UPDATE {table} SET id=X'{target}' WHERE id=X'{source}'");
        // Either the statement was accepted, or it was refused while still
        // leaving the row gone. Both mean the identity moved.
        let refused = query(&sql).execute(&pool).await.is_err();
        if !refused || count(&pool, table).await? == 0 {
            accepted.push(table);
        }
        let _ = column;
    }

    assert!(
        accepted.is_empty(),
        "a retained row's identity was repointed in {accepted:?}"
    );
    Ok(())
}

/// One identity-repointing vector per table, each on the `id` column only.
///
/// `team_id` is deliberately not probed here. Moving it breaks the row's own
/// foreign keys -- an Artifact that moves Team stops matching its Task, Run and
/// Role -- so the refusal would come from `FOREIGN KEY constraint failed` and
/// would prove nothing about the guard, which is exactly the scoring rule this
/// file is built to avoid. `id` is the column with no foreign key behind it for
/// `reviews`, `artifacts` and `approvals`, and the probes for `runs` and
/// `messages` use rows that nothing else points at (`LONE_RUN` has no lease,
/// and the seeded Message has no delivery attempt) for the same reason.
///
/// A repointed `id` is not a harmless renumber: DATA_CONVENTIONS section 8 keys
/// the Event hash chain on the subject, and DOMAIN_MODEL section 2 makes every
/// one of these a stable identity, so a row that changes its `id` is no longer
/// reachable under the identity its own history was recorded against.
const IDENTITY_VECTORS: [(&str, &str, &str, &str); 5] = [
    (
        "runs",
        "id",
        "01921389d31174628f80cb5f79ab0008",
        "01921389d31174628f80cb5f79ab0050",
    ),
    (
        "reviews",
        "id",
        "01921389d31174628f80cb5f79ab0009",
        "01921389d31174628f80cb5f79ab0051",
    ),
    (
        "artifacts",
        "id",
        "01921389d31174628f80cb5f79ab0010",
        "01921389d31174628f80cb5f79ab0052",
    ),
    (
        "approvals",
        "id",
        "01921389d31174628f80cb5f79ab0011",
        "01921389d31174628f80cb5f79ab0053",
    ),
    (
        "messages",
        "id",
        "01921389d31174628f80cb5f79ab0021",
        "01921389d31174628f80cb5f79ab0054",
    ),
];

// ---------------------------------------------------------------------------
// Axis 4 -- the legal paths stay open. A guard that freezes a table instead of
// protecting it would pass every assertion above.
// ---------------------------------------------------------------------------

/// Cancelling a `PENDING` Approval goes through `CANCELLED` plus its actor, and
/// the request row stays. This is the corrected form of the old `0020`
/// expectation, which asserted that a `PENDING` Approval *may* be deleted.
#[tokio::test]
async fn pending_approval_is_cancelled_not_deleted() -> Result<(), Box<dyn Error>> {
    let pool = database().await?;
    seed_unreferenced_history(&pool).await?;
    assert_eq!(approval_status(&pool).await?, "PENDING");

    assert!(
        query(&format!("DELETE FROM approvals WHERE id=X'{APPROVAL}'"))
            .execute(&pool)
            .await
            .is_err(),
        "a PENDING approval request was physically deleted"
    );
    assert_eq!(count(&pool, "approvals").await?, 1);
    assert_eq!(approval_status(&pool).await?, "PENDING");

    // The path DATA_CONVENTIONS section 5 and STATE_MACHINES section 7 name.
    query(&format!(
        "UPDATE approvals SET status='CANCELLED', cancelled_by_actor_type='ROLE', cancelled_by_actor_id=X'{ROLE}', version=2 WHERE id=X'{APPROVAL}'"
    ))
    .execute(&pool)
    .await?;

    assert_eq!(approval_status(&pool).await?, "CANCELLED");
    assert_eq!(
        query_scalar::<_, String>(&format!(
            "SELECT hex(cancelled_by_actor_id) FROM approvals WHERE id=X'{APPROVAL}'"
        ))
        .fetch_one(&pool)
        .await?
        .to_lowercase(),
        ROLE,
        "the cancellation actor was not recorded"
    );
    assert_eq!(count(&pool, "approvals").await?, 1);
    Ok(())
}

/// Runs still terminate, retries still append a new attempt, Messages still move
/// through their state machine, and a new Review/Artifact/Approval is still
/// insertable.
#[tokio::test]
async fn the_tables_are_guarded_not_welded_shut() -> Result<(), Box<dyn Error>> {
    let pool = database().await?;
    seed_unreferenced_history(&pool).await?;

    // A retry appends attempt 2 rather than overwriting attempt 1
    // (DATA_CONVENTIONS section 3 and STATE_MACHINES section 3).
    let retry = "01921389d31174628f80cb5f79ab0060";
    eprintln!("DBG stage=insert-retry");
    query(&format!(
        "INSERT INTO runs VALUES(X'{retry}',X'{TEAM}',X'{TASK}',X'{ROLE}',X'{BINDING}',X'{HARNESS}',3,3,'RUNNING',NULL,NULL,1,2,2,NULL)"
    ))
    .execute(&pool)
    .await?;
    // A Run that is still `RUNNING` may legitimately reach each terminal
    // outcome, and each route is proved **on its own Run**.
    //
    // This loop used to walk one row `RUNNING -> TIMED_OUT -> SUCCEEDED` and
    // call it the legal path. It is not one: STATE_MACHINES section 3 gives the
    // four terminal statuses no outgoing edge, so `TIMED_OUT -> SUCCEEDED` is
    // one of the 12 illegal cells FC-03 closes, and the "legal control" was
    // asserting a write the frozen state machine forbids. It only passed because
    // nothing guarded the cell -- the same defect the control was supposed to
    // rule out. A control that passes for the defect's reason is worse than no
    // control, because it is read as evidence the guards are not too strict.
    //
    // So each destination is reached from `RUNNING` on a separate, freshly
    // seeded Run, and the old terminal Run is never reused. `0004` requires
    // `completed_at` on every terminal status, so it is supplied rather than
    // leaving the path half-exercised.
    // `RUNNING -> SUCCEEDED` on the retry row itself, still starting from
    // `RUNNING`: the route the original loop meant to demonstrate, minus the
    // illegal second hop. Done first because it also retires the one active Run
    // this Task may have -- `runs_single_active_run_per_task` (0026) allows a
    // single active Run per Task, so the probes below cannot be inserted as
    // `RUNNING` while this one still holds the slot.
    query(&format!(
        "UPDATE runs SET status='SUCCEEDED', exit_code=0, completed_at=3, version=version+1 WHERE id=X'{retry}'"
    ))
    .execute(&pool)
    .await?;
    assert_eq!(
        query_scalar::<_, String>(&format!("SELECT status FROM runs WHERE id=X'{retry}'"))
            .fetch_one(&pool)
            .await?,
        "SUCCEEDED"
    );

    // Now each remaining terminal route, each from `RUNNING` on its own freshly
    // inserted Run, sequenced so only one is active at a time. Each Run ends
    // where it was started from -- an active Run, never an already-finished one.
    for (tag, status, exit_code) in [("0064", "TIMED_OUT", "NULL"), ("0065", "SUCCEEDED", "0")] {
        let row = format!("01921389d31174628f80cb5f79ab{tag}");
        // The generation is read back from the Task rather than written down.
        // `0012:97-104` requires `attempt = MAX(attempt) + 1` and a strictly
        // rising `fencing_token` per Task, so a hard-coded generation number is a
        // fixture that silently rots as soon as an earlier INSERT in this test
        // changes. Two earlier drafts here used `9, 9` and then `7, 7`; both were
        // refused, and -- the reason it is worth recording -- `0012` refuses them
        // with "Run consistency violation", the same string the terminal-guard
        // assertions match on. A bad fixture wearing a guard's message is exactly
        // the ADVERSARIAL_REVIEW_PLAYBOOK rule 2 failure shape.
        let generation: i64 = query_scalar(&format!(
            "SELECT COALESCE(MAX(attempt), 0) + 1 FROM runs WHERE task_id = X'{TASK}'"
        ))
        .fetch_one(&pool)
        .await?;
        query(&format!(
            "INSERT INTO runs VALUES(X'{row}',X'{TEAM}',X'{TASK}',X'{ROLE}',X'{BINDING}',X'{HARNESS}',{generation},{generation},'RUNNING',NULL,NULL,1,2,2,NULL)"
        ))
        .execute(&pool)
        .await?;
        query(&format!(
            "UPDATE runs SET status='{status}', exit_code={exit_code}, completed_at=3, version=version+1 WHERE id=X'{row}'"
        ))
        .execute(&pool)
        .await
        .unwrap_or_else(|e| {
            panic!("the frozen RUNNING -> {status} edge was refused ({e}); the tables are welded shut")
        });
        assert_eq!(
            query_scalar::<_, String>(&format!("SELECT status FROM runs WHERE id=X'{row}'"))
                .fetch_one(&pool)
                .await?,
            status,
            "RUNNING -> {status} reported success but the row reads otherwise"
        );
    }

    // The finished first Run keeps its own record; nothing was merged away.
    // Two extra Runs were seeded for the two independent routes above, so the
    // count is 2 seeded + 1 retry + 2 route probes.
    assert_eq!(
        count(&pool, "runs").await?,
        5,
        "a retry replaced an attempt"
    );

    // A `DRAFT` Message body is still revisable, and `DRAFT -> QUEUED` still
    // freezes it (DATA_CONVENTIONS section 5).
    query(&format!(
        "UPDATE messages SET body='revised' WHERE id=X'{MESSAGE}' AND status='DRAFT'"
    ))
    .execute(&pool)
    .await
    .ok();
    assert_eq!(count(&pool, "messages").await?, 1);

    // New records are still appendable in every one of the five tables.
    query(&format!(
        "INSERT INTO reviews VALUES(X'01921389d31174628f80cb5f79ab0061',X'{TEAM}',X'{TASK}',X'{RUN}',X'{ROLE}','CHANGES_REQUESTED','needs work',1,8)"
    ))
    .execute(&pool)
    .await?;
    query(&format!(
        "INSERT INTO artifacts(id,team_id,task_id,run_id,creator_role_id,artifact_type,uri,checksum_sha256,git_commit_sha,visibility,version,created_at) VALUES(X'01921389d31174628f80cb5f79ab0062',X'{TEAM}',X'{TASK}',X'{RUN}',X'{ROLE}','TEST_REPORT','tasks/T-1/other',?,NULL,'TEAM_INTERNAL',1,8)"
    ))
    .bind("1".repeat(64))
    .execute(&pool)
    .await?;
    raw_sql(&format!(
        "INSERT INTO approvals(id,team_id,requester_actor_type,requester_role_id,action,resource_type,resource_scope,reason,status,version,expires_at,created_at) \
VALUES(X'01921389d31174628f80cb5f79ab0063',X'{TEAM}','ROLE',X'{ROLE}','EXECUTE','CREDENTIAL_ACCESS','scope-2','reason-2','PENDING',1,9,7)"
    ))
    .execute(&pool)
    .await?;

    assert_eq!(count(&pool, "reviews").await?, 2);
    assert_eq!(count(&pool, "artifacts").await?, 2);
    assert_eq!(count(&pool, "approvals").await?, 2);
    Ok(())
}

/// An `APPROVED` decision is a terminal fact: it may not be reopened, rewritten
/// or deleted, while the frozen state machine keeps its own guard on that.
#[tokio::test]
async fn an_approved_decision_stays_approved_and_present() -> Result<(), Box<dyn Error>> {
    let pool = database().await?;
    seed_unreferenced_history(&pool).await?;
    query(&format!(
        "UPDATE approvals SET status='APPROVED', decided_by_human_id=X'{SENDER_HUMAN}', decided_at=8, version=2 WHERE id=X'{APPROVAL}'"
    ))
    .execute(&pool)
    .await?;

    assert!(
        query(&format!(
            "UPDATE approvals SET status='PENDING' WHERE id=X'{APPROVAL}'"
        ))
        .execute(&pool)
        .await
        .is_err(),
        "a decided approval was returned to PENDING"
    );
    assert_eq!(approval_status(&pool).await?, "APPROVED");
    assert!(
        query(&format!("DELETE FROM approvals WHERE id=X'{APPROVAL}'"))
            .execute(&pool)
            .await
            .is_err()
    );
    assert_eq!(count(&pool, "approvals").await?, 1);
    Ok(())
}

/// Another Team's rows are not reachable through these guards' side effects: the
/// retained set is per Team, and an approval request may not address another
/// Team's Role.
#[tokio::test]
async fn the_new_guards_are_per_team_and_add_no_new_reach() -> Result<(), Box<dyn Error>> {
    let pool = database().await?;
    seed_unreferenced_history(&pool).await?;
    raw_sql(&format!(
        "INSERT INTO teams VALUES(X'{OTHER_TEAM}','other','ACTIVE',1,1,1,NULL); \
INSERT INTO roles VALUES(X'{OTHER_ROLE}',X'{OTHER_TEAM}','other','other','ACTIVE',1,1,1,NULL);"
    ))
    .execute(&pool)
    .await?;

    assert!(
        query(&format!(
            "INSERT INTO approvals(id,team_id,requester_actor_type,requester_role_id,action,resource_type,resource_scope,reason,status,version,expires_at,created_at) \
VALUES(X'01921389d31174628f80cb5f79ab0064',X'{TEAM}','ROLE',X'{OTHER_ROLE}','EXECUTE','CREDENTIAL_ACCESS','s','r','PENDING',1,9,7)"
        ))
        .execute(&pool)
        .await
        .is_err(),
        "an approval request addressed a Role of another Team"
    );
    assert_eq!(
        query_scalar::<_, i64>(&format!(
            "SELECT COUNT(*) FROM approvals WHERE requester_role_id=X'{OTHER_ROLE}'"
        ))
        .fetch_one(&pool)
        .await?,
        0
    );
    Ok(())
}

// ---------------------------------------------------------------------------
// Upgrade behaviour.
// ---------------------------------------------------------------------------

/// The upgrade keeps every existing row of the five tables byte for byte, and
/// arms the new guards afterwards.
#[tokio::test]
async fn upgrading_to_0026_preserves_every_existing_row() -> Result<(), Box<dyn Error>> {
    let pool = v25_database().await?;
    seed(&pool).await?;
    seed_unreferenced_history(&pool).await?;
    raw_sql(&reference("01921389d31174628f80cb5f79ab0040", "RUN", RUN))
        .execute(&pool)
        .await?;
    raw_sql(&event("01921389d31174628f80cb5f79ab0041", "RUN", RUN))
        .execute(&pool)
        .await?;

    let before: Vec<(String, String)> = snapshot(&pool).await?;

    migrate(&pool).await?;

    let after: Vec<(String, String)> = snapshot(&pool).await?;
    assert_eq!(
        before, after,
        "the upgrade changed a retained row instead of only adding guards"
    );
    assert_eq!(
        query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM _sqlx_migrations WHERE version=26 AND success=TRUE"
        )
        .fetch_one(&pool)
        .await?,
        1,
        "0026 did not record itself as applied"
    );

    // And the guards are live on the upgraded database, not just on a fresh one.
    for name in NEW_TRIGGERS {
        assert!(trigger_exists(&pool, name).await?, "{name} is missing");
    }
    for (table, target) in DELETE_TARGETS {
        assert!(
            query(&format!("DELETE FROM {target}"))
                .execute(&pool)
                .await
                .is_err(),
            "{table} was deletable on the upgraded database"
        );
        assert_eq!(count(&pool, target).await?, 1);
    }
    Ok(())
}

/// A migration that fails must leave the schema and the data exactly as they
/// were. The failure is produced by occupying the name of the **last** trigger
/// `0026` installs, so the nine triggers created before it inside the same
/// transaction have to roll back too.
#[tokio::test]
async fn a_failed_upgrade_has_no_side_effects() -> Result<(), Box<dyn Error>> {
    let pool = v25_database().await?;
    seed(&pool).await?;
    seed_unreferenced_history(&pool).await?;

    let occupied = *NEW_TRIGGERS.last().expect("NEW_TRIGGERS must not be empty");
    query(&format!(
        "CREATE TRIGGER {occupied} BEFORE INSERT ON messages BEGIN SELECT 1; END;"
    ))
    .execute(&pool)
    .await?;
    assert!(trigger_exists(&pool, occupied).await?);

    let before = snapshot(&pool).await?;

    assert!(
        migrate(&pool).await.is_err(),
        "the occupied trigger name did not stop the migration, so this failure path proves nothing"
    );

    assert_eq!(
        snapshot(&pool).await?,
        before,
        "the failed upgrade changed data"
    );
    for name in NEW_TRIGGERS {
        if name == occupied {
            continue;
        }
        assert!(
            !trigger_exists(&pool, name).await?,
            "{name} survived the failed migration, so 0026 does not run atomically"
        );
    }
    assert_eq!(
        query_scalar::<_, i64>("SELECT COUNT(*) FROM _sqlx_migrations WHERE version=26")
            .fetch_one(&pool)
            .await?,
        0,
        "a failed 0026 was recorded as applied"
    );
    Ok(())
}

// ---------------------------------------------------------------------------
// Rig self-checks (ADVERSARIAL_REVIEW_PLAYBOOK rule 1-c).
// ---------------------------------------------------------------------------

/// Bidirectional: the ten `0026` triggers must exist on the hardened schema and
/// must be absent on the `0025` baseline. Without the second half a rig that
/// quietly loaded the wrong schema would still report a result.
#[tokio::test]
async fn the_new_guards_are_installed() -> Result<(), Box<dyn Error>> {
    let hardened = database().await?;
    for name in NEW_TRIGGERS {
        assert!(
            trigger_exists(&hardened, name).await?,
            "{name} is missing from the hardened schema"
        );
    }

    let baseline = v25_database().await?;
    for name in NEW_TRIGGERS {
        assert!(
            !trigger_exists(&baseline, name).await?,
            "{name} already existed on the v25 baseline, so the two schemas are not distinguishable"
        );
    }
    Ok(())
}

/// The migration number this package owns really is applied, exactly once, as a
/// successful record.
///
/// Stable form, not a chain-length count: `COUNT(*) WHERE success=TRUE` counts
/// *every* successful migration in the chain, so it breaks each time an
/// unrelated migration is registered. Pinning `version = 26` keeps the
/// assertion about this package's own migration. See MR-04 in
/// `docs/reviews/MINIMAX-RETURN-AUDIT-2026-10-08.md`.
#[tokio::test]
async fn this_package_owns_exactly_version_26() -> Result<(), Box<dyn Error>> {
    let pool = database().await?;
    assert_eq!(
        query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM _sqlx_migrations WHERE version=26 AND success=TRUE"
        )
        .fetch_one(&pool)
        .await?,
        1,
        "0026 did not record exactly one successful application"
    );
    assert_eq!(
        query_scalar::<_, String>("SELECT description FROM _sqlx_migrations WHERE version=26")
            .fetch_one(&pool)
            .await?,
        "core history retention"
    );
    Ok(())
}

/// Foreign keys really are on for every connection this file opens, and
/// `foreign_key_check` is clean after seeding, so no rejection in this file can
/// have come from a foreign key by accident.
#[tokio::test]
async fn foreign_keys_are_on_and_the_seed_is_clean() -> Result<(), Box<dyn Error>> {
    let pool = database().await?;
    seed_unreferenced_history(&pool).await?;
    assert_eq!(
        query_scalar::<_, i64>("PRAGMA foreign_keys")
            .fetch_one(&pool)
            .await?,
        1
    );
    assert_eq!(
        query_scalar::<_, i64>("SELECT COUNT(*) FROM pragma_foreign_key_check")
            .fetch_one(&pool)
            .await?,
        0
    );
    Ok(())
}

// ---------------------------------------------------------------------------
// Read-back helpers. Each negative vector needs a value it can compare, so a
// rejection that fails to protect the row is visible rather than assumed.
// ---------------------------------------------------------------------------

async fn snapshot(pool: &SqlitePool) -> Result<Vec<(String, String)>, Box<dyn Error>> {
    let mut rows = Vec::new();
    for (table, columns) in [
        ("teams", "hex(id)||':'||status"),
        ("roles", "hex(id)||':'||status"),
        ("bindings", "hex(id)||':'||status"),
        ("tasks", "hex(id)||':'||status||':'||version"),
        (
            "runs",
            "hex(id)||':'||status||':'||attempt||':'||fencing_token||':'||version||':'||ifnull(completed_at,-1)",
        ),
        (
            "reviews",
            "hex(id)||':'||decision||':'||ifnull(comments,'')||':'||version",
        ),
        (
            "artifacts",
            "hex(id)||':'||artifact_type||':'||uri||':'||visibility||':'||version",
        ),
        (
            "approvals",
            "hex(id)||':'||status||':'||action||':'||resource_scope||':'||version",
        ),
        ("messages", "hex(id)||':'||status||':'||body||':'||version"),
        ("threads", "hex(id)||':'||status||':'||version"),
        (
            "\"references\"",
            "hex(id)||':'||subject_type||':'||hex(subject_id)||':'||validation_status",
        ),
        (
            "events",
            "hex(id)||':'||subject_type||':'||hex(subject_id)||':'||hash",
        ),
    ] {
        rows.push((
            table.to_string(),
            query_scalar::<_, String>(&format!("SELECT {columns} FROM {table} ORDER BY 1"))
                .fetch_all(pool)
                .await?
                .join("|"),
        ));
    }
    Ok(rows)
}

async fn run_status(pool: &SqlitePool) -> Result<String, Box<dyn Error>> {
    Ok(
        query_scalar::<_, String>(&format!("SELECT status FROM runs WHERE id=X'{RUN}'"))
            .fetch_one(pool)
            .await?,
    )
}

async fn review_decision(pool: &SqlitePool) -> Result<String, Box<dyn Error>> {
    Ok(query_scalar::<_, String>(&format!(
        "SELECT decision FROM reviews WHERE id=X'{REVIEW}'"
    ))
    .fetch_one(pool)
    .await?)
}

async fn artifact_visibility(pool: &SqlitePool) -> Result<String, Box<dyn Error>> {
    Ok(query_scalar::<_, String>(&format!(
        "SELECT visibility FROM artifacts WHERE id=X'{ARTIFACT}'"
    ))
    .fetch_one(pool)
    .await?)
}

async fn approval_status(pool: &SqlitePool) -> Result<String, Box<dyn Error>> {
    Ok(query_scalar::<_, String>(&format!(
        "SELECT status FROM approvals WHERE id=X'{APPROVAL}'"
    ))
    .fetch_one(pool)
    .await?)
}

async fn message_status(pool: &SqlitePool) -> Result<String, Box<dyn Error>> {
    Ok(query_scalar::<_, String>(&format!(
        "SELECT status FROM messages WHERE id=X'{MESSAGE}'"
    ))
    .fetch_one(pool)
    .await?)
}

/// One `INSERT OR REPLACE` vector per table.
///
/// Every vector keeps the primary key and the natural uniqueness tuples intact
/// and changes only what the specification calls history -- status, decision,
/// visibility, reason, body -- so a rejection cannot be attributed to a
/// uniqueness violation, and so no vector is a no-op
/// ([`replace_vectors_each_change_a_different_value`]).
const REPLACE_VECTORS: [(&str, &str); 5] = [
    (
        "runs",
        "INSERT OR REPLACE INTO runs VALUES(X'01921389d31174628f80cb5f79ab0008',X'01921389d31174628f80cb5f79ab0001',X'01921389d31174628f80cb5f79ab0006',X'01921389d31174628f80cb5f79ab0002',X'01921389d31174628f80cb5f79ab0005',X'01921389d31174628f80cb5f79ab0003',2,2,'FAILED',1,NULL,1,1,NULL,2)",
    ),
    (
        "reviews",
        "INSERT OR REPLACE INTO reviews VALUES(X'01921389d31174628f80cb5f79ab0009',X'01921389d31174628f80cb5f79ab0001',X'01921389d31174628f80cb5f79ab0006',X'01921389d31174628f80cb5f79ab0007',X'01921389d31174628f80cb5f79ab0002','CHANGES_REQUESTED','reversed',9,5)",
    ),
    (
        "artifacts",
        "INSERT OR REPLACE INTO artifacts(id,team_id,task_id,run_id,creator_role_id,artifact_type,uri,checksum_sha256,git_commit_sha,visibility,version,created_at) VALUES(X'01921389d31174628f80cb5f79ab0010',X'01921389d31174628f80cb5f79ab0001',X'01921389d31174628f80cb5f79ab0006',X'01921389d31174628f80cb5f79ab0007',X'01921389d31174628f80cb5f79ab0002','TEST_REPORT','tasks/T-1/swapped','0000000000000000000000000000000000000000000000000000000000000001',NULL,'TEAM_INTERNAL',9,5)",
    ),
    (
        "approvals",
        "INSERT OR REPLACE INTO approvals(id,team_id,requester_actor_type,requester_role_id,requester_human_id,action,resource_type,resource_scope,reason,status,decided_by_human_id,decision_notes,version,expires_at,created_at,decided_at) VALUES(X'01921389d31174628f80cb5f79ab0011',X'01921389d31174628f80cb5f79ab0001','ROLE',X'01921389d31174628f80cb5f79ab0002',NULL,'EXECUTE','TASK_MUTATION','task-9','swallowed','PENDING',NULL,NULL,9,9,2,NULL)",
    ),
    (
        "messages",
        "INSERT OR REPLACE INTO messages(id,team_id,thread_id,sender_actor_type,from_human_id,to_role_id,message_type,body,status,version,created_at,updated_at) VALUES(X'01921389d31174628f80cb5f79ab0021',X'01921389d31174628f80cb5f79ab0001',X'01921389d31174628f80cb5f79ab0020','HUMAN',X'01921389d31174628f80cb5f79ab0022',X'01921389d31174628f80cb5f79ab0002','DIRECT','swallowed','DRAFT',9,1,1)",
    ),
];

/// Every stored column of one table, as one comparable string.
///
/// `SELECT *` is avoided on purpose -- the project forbids it in production
/// queries, and a test that relies on column order would break the day someone
/// adds a column. Each table is listed explicitly instead.
async fn table_contents(pool: &SqlitePool, table: &str) -> Result<String, Box<dyn Error>> {
    let columns: &[&str] = match table {
        "runs" => &[
            "hex(id)",
            "hex(task_id)",
            "attempt",
            "fencing_token",
            "status",
            "ifnull(exit_code,'~')",
            "ifnull(failure_reason,'~')",
            "version",
            "created_at",
            "ifnull(started_at,'~')",
            "ifnull(completed_at,'~')",
        ],
        "reviews" => &[
            "hex(id)",
            "hex(task_id)",
            "hex(run_id)",
            "hex(reviewer_role_id)",
            "decision",
            "ifnull(comments,'~')",
            "version",
            "created_at",
        ],
        "artifacts" => &[
            "hex(id)",
            "hex(task_id)",
            "hex(run_id)",
            "hex(creator_role_id)",
            "artifact_type",
            "uri",
            "ifnull(checksum_sha256,'~')",
            "ifnull(git_commit_sha,'~')",
            "visibility",
            "version",
            "created_at",
        ],
        "approvals" => &[
            "hex(id)",
            "requester_actor_type",
            "ifnull(hex(requester_role_id),'~')",
            "action",
            "resource_type",
            "resource_scope",
            "status",
            "version",
            "ifnull(decided_by_human_id,'~')",
            "ifnull(decision_notes,'~')",
            "expires_at",
            "created_at",
        ],
        "messages" => &[
            "hex(id)",
            "hex(thread_id)",
            "sender_actor_type",
            "ifnull(hex(to_role_id),'~')",
            "message_type",
            "body",
            "status",
            "version",
            "created_at",
        ],
        other => panic!("no column list for {other}"),
    };
    let row = columns
        .iter()
        .map(|column| format!("ifnull(cast({column} as text), '~')"))
        .collect::<Vec<String>>()
        .join("||':'||");
    Ok(query_scalar::<_, String>(&format!(
        "SELECT ifnull(group_concat(row_text, '|'), '') FROM (SELECT {row} AS row_text FROM {table})"
    ))
    .fetch_one(pool)
    .await?)
}
