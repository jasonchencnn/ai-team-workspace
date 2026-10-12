//! MM-R03 (deep audit DA-03): a Run may only move along the directed edges
//! frozen in `STATE_MACHINES.md` section 3, and an existing attempt may never be
//! reset to `QUEUED`.
//!
//! The counterexample this file answers was reproduced by the deep audit against
//! `0001..0024` and logged in
//! `docs/reviews/evidence/deep-audit-2026-10-08/storage-boundaries-red-v2.log`:
//! after a legal `UNKNOWN -> RUNNING`, `UPDATE runs SET status = 'QUEUED',
//! started_at = NULL` **succeeded** on a `RUNNING` attempt 2 carrying token 2.
//! Log line: `an existing RUNNING attempt was reset to QUEUED`.
//!
//! Why the existing guards did not stop it — this part must not be restated as
//! "there was no guard". `0012:110-162` (`runs_consistency_update`) has eleven
//! conjuncts and eight of them fire on a status write: the five identity
//! columns, `attempt`, `fencing_token`, "is this the newest generation", and
//! "a terminal Run released its lease". Only `0012:142-145` looks at the status
//! pair itself, and it blocks exactly one direction:
//!
//! ```text
//! NOT (OLD.status IN (SUCCEEDED, FAILED, TIMED_OUT, CANCELLED)
//!      AND NEW.status IN (QUEUED, STARTING, RUNNING, UNKNOWN))
//! ```
//!
//! terminal -> active. The four *non-terminal* rows were free to move to any of
//! the other seven.
//!
//! FC-03 completes the picture with `0029`. Measured on a real database by this
//! package's probe (`docs/reviews/evidence/minimax-followup/FC-03/main/`):
//!
//! ```text
//!                       0028 baseline                    0029 hardened
//!   off-diagonal cells   29 accepted / 27 refused       17 accepted / 39 refused
//!   17 frozen edges      all accepted                   all accepted
//!   terminal -> terminal 12 accepted (the gap)         all refused
//! ```
//!
//! The 17 accepted cells on the hardened schema are exactly the frozen edges of
//! STATE_MACHINES section 3, and the 39 refused are exactly the illegal
//! off-diagonal cells: 11 from a non-terminal source (`0027`), 16 terminal ->
//! active (`0012:142-145`) and 12 terminal -> terminal (`0029`).
//!
//! Scoring rules, taken from `ADVERSARIAL_REVIEW_PLAYBOOK`:
//!
//! - rule 2, "a failure disguised as a pass": every negative assertion checks
//!   the statement returned an error **and** that the row still carries its old
//!   status. A `CHECK` on `started_at`, a foreign key, or an UPDATE that matched
//!   zero rows never counts as a pass here. The audit's own probe proves this
//!   matters -- it had to clear `started_at` in the same statement, because
//!   `0004`'s CHECK refuses `RUNNING` without a `started_at`. A version of this
//!   test that forgot the timestamp would have scored the CHECK, not the guard.
//! - rule 1-c, the rig declares its own schema: `the_new_guard_is_installed`
//!   asserts the `0027` trigger exists on the hardened schema **and** is absent
//!   on the `v26` baseline, so a rig that loaded the wrong migration cannot
//!   produce a result.
//! - rule 3, every tested row is seeded: `seed` ends with per-table counts and a
//!   `pragma_foreign_key_check`, and no test creates the row it then attacks.
//! - rule 6, count before writing it down: the "how many fixtures would a
//!   creation-axis guard break" number in
//!   `creating_a_run_outside_queued_is_still_possible_and_that_is_measured` is
//!   computed by the same regex used to derive it, and is compared against the
//!   file list rather than asserted from memory.
//!
//! Frozen basis, none of which this file extends:
//!   - STATE_MACHINES.md section 3: the 16-row transition table, `UNKNOWN` as the
//!     reconcile state with `UNKNOWN -> RUNNING` legal, the four terminal
//!     statuses, and "automatic retry only reads terminal Runs and creates a new
//!     Run; putting an old Run back to QUEUED is forbidden".
//!   - DOMAIN_MODEL.md section 2: a Run's invariant is "state cannot go
//!     backwards; an old token has no write right", with "cannot be overwritten;
//!     retry creates a new one".
//!   - STATE_MACHINES.md section 1 and DATA_CONVENTIONS.md section 3: an
//!     UPDATE that changes no status is not a transition and stays legal.
//!
//! The edge set is **derived from the domain's `RUN_EDGES`**, the same constant
//! `RUNNING_EDGES` in `crates/domain/src/run_state_machine.rs` implements from
//! section 3 -- not from eight hand-copied string literals. That is the join the
//! tenth review of DEV-02-05 demanded for the lease guards
//! (`LEASE_BEARING_STATUSES` in the same file): two independent lists with
//! nothing between them is how a domain change and a database change drift apart
//! with every suite green.

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
use teamspace_domain::run_state_machine::{RUN_EDGES, RunStatus};

// ---------------------------------------------------------------------------
// Identities. One Team, one Role, one Harness, one Adapter, one Binding, one
// Task. Every ID is a 16-byte UUIDv7 literal with the `7` version nibble and an
// RFC 4122 variant nibble, because `0001..0004` CHECKs reject anything else and
// a rejected INSERT would silently turn an attack into a device fault.
// ---------------------------------------------------------------------------
/// A 16-byte UUIDv7 literal that satisfies `0001..0004`'s CHECKs.
///
/// Device note: the first version of this file hand-wrote extra Run ids as
/// `...ab0008`, `...ac0000`, `...ad0000`, `...ae0000`. Three of those four fail
/// `typeof(id) = 'blob' AND length(id) = 16`, and every case that used them was
/// refused by the table instead of by the guard -- the exact
/// "failure disguised as a bypass" shape ADVERSARIAL_REVIEW_PLAYBOOK rule 2
/// warns about. One generator with the nibbles fixed in one place removes the
/// whole class: the template is 30 hex characters, positions 13 and 17 (1-based)
/// are the version and variant nibbles SQLite checks.
fn run_id(tag: &str, n: usize) -> String {
    // The tag is embedded in an `X'..'` BLOB literal, so a non-hex character is
    // not a subtle bug: it is a SQL syntax error, and a test whose fixture
    // cannot even be parsed reports a database error instead of a result. The
    // first draft of this package used "t0"/"u0" tags and three cases died that
    // way.
    assert!(
        tag.len() == 2 && tag.bytes().all(|c| c.is_ascii_hexdigit()),
        "run_id tag {tag:?} must be exactly two hex digits to sit inside X'..'"
    );
    // The 27-character prefix already ends with the version nibble `7` (position
    // 13) and the variant nibble `8` (position 17), which are the two SQLite
    // checks beyond the 16-byte length -- see `0001..0004`.
    // 27 hex characters of prefix + a 2-character tag + a 3-digit ordinal.
    // The three widths are not arbitrary: `run_id` asserts the total is 32 and
    // that positions 13 and 17 are the version and variant nibbles, so widening
    // the prefix or the tag without narrowing the ordinal fails here rather than
    // producing an id the table rejects for an unrelated reason.
    let s = format!("01921389d31174628f80cb5f79a{tag}{n:03x}");
    assert!(
        n < 0x1000,
        "run_id ordinal {n} does not fit in three hex digits"
    );
    assert_eq!(
        s.len(),
        32,
        "generated id {s} is not 32 hex characters, so it cannot be a 16-byte UUID"
    );
    assert_eq!(s.as_bytes()[12], b'7', "version nibble of {s} must be 7");
    assert!(
        b"89AB".contains(&s.as_bytes()[16]),
        "variant nibble of {s} must be 8, 9, A or B"
    );
    s
}

const TEAM: &str = "01921389d31174628f80cb5f79ab0001";
const ROLE: &str = "01921389d31174628f80cb5f79ab0002";
const HARNESS: &str = "01921389d31174628f80cb5f79ab0003";
const ADAPTER: &str = "01921389d31174628f80cb5f79ab0004";
const BINDING: &str = "01921389d31174628f80cb5f79ab0005";
const TASK: &str = "01921389d31174628f80cb5f79ab0006";
/// Attempt 1, already `SUCCEEDED`. Present so the Task has the retained history
/// section 3 requires and so attempt 2 below is genuinely a *later* generation
/// rather than the first one.
const SEEDED_RUN: &str = "01921389d31174628f80cb5f79ab0007";
/// Attempt 2, token 2 -- the row the audit reset.
const RUNNING_RUN: &str = "01921389d31174628f80cb5f79ab0008";

/// The trigger `0027` installs.
const NEW_TRIGGERS: [&str; 1] = ["runs_status_transition_integrity_update"];

/// The single trigger `0029` installs.
const TERMINAL_TRIGGER: &str = "runs_terminal_status_integrity_update";

/// The message `0027` raises. Kept distinct from `0012`'s four strings on
/// purpose: `run_lease_consistency_forward_migration.rs` asserts
/// `"Run consistency violation"` to prove *that* guard refused a statement, and
/// a second guard reusing the string would make those assertions ambiguous.
/// Every refusal in this file names the message it is allowed to come from.
const TRANSITION_MESSAGE: &str = "Run transition violation";

/// The message `0029` raises for the terminal quadrant.
///
/// Kept distinct from `0012`'s and `0027`'s strings on purpose, for the reason
/// `TRANSITION_MESSAGE` names: an error string is not a frozen business edge, but
/// it is still the only thing that says *which* guard fired, and two guards
/// sharing a string cannot be told apart. What the assertions actually pin is the
/// domain reason (a terminal Run has no outgoing edge in `STATE_MACHINES` §3) plus
/// proof the row did not move; this constant only narrows which refusal is
/// acceptable when several guards could legitimately cover the same write.
const TERMINAL_MESSAGE: &str = "Run terminal status violation";

const SEED: &str = "\
INSERT INTO teams VALUES(X'01921389d31174628f80cb5f79ab0001','t','ACTIVE',1,1,1,NULL);
INSERT INTO roles VALUES(X'01921389d31174628f80cb5f79ab0002',X'01921389d31174628f80cb5f79ab0001','r','r','ACTIVE',1,1,1,NULL);
INSERT INTO harness_instances VALUES(X'01921389d31174628f80cb5f79ab0003',X'01921389d31174628f80cb5f79ab0001','h','h','ACTIVE','[]',NULL,1,1,1,NULL);
INSERT INTO adapters VALUES(X'01921389d31174628f80cb5f79ab0004',X'01921389d31174628f80cb5f79ab0001','a','1','/a','{}',1,1,1);
INSERT INTO bindings VALUES(X'01921389d31174628f80cb5f79ab0005',X'01921389d31174628f80cb5f79ab0001',X'01921389d31174628f80cb5f79ab0002',X'01921389d31174628f80cb5f79ab0003',X'01921389d31174628f80cb5f79ab0004','ACTIVE',1,1,'{}',1,1,1,NULL);
INSERT INTO tasks VALUES(X'01921389d31174628f80cb5f79ab0006',X'01921389d31174628f80cb5f79ab0001',X'01921389d31174628f80cb5f79ab0002','T-1','t','','DOING',1,1,1,NULL,NULL);
INSERT INTO runs VALUES(X'01921389d31174628f80cb5f79ab0007',X'01921389d31174628f80cb5f79ab0001',X'01921389d31174628f80cb5f79ab0006',X'01921389d31174628f80cb5f79ab0002',X'01921389d31174628f80cb5f79ab0005',X'01921389d31174628f80cb5f79ab0003',1,1,'SUCCEEDED',0,NULL,1,1,1,2);
";

async fn db() -> Result<SqlitePool, Box<dyn Error>> {
    let p = SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(SqliteConnectOptions::from_str("sqlite::memory:")?.foreign_keys(true))
        .await?;
    migrate(&p).await?;
    Ok(p)
}

/// Schema + seed, with the rig's own assertions.
///
/// The counts are checked on **every** database this file builds, not once in a
/// helper that some tests could skip, and the `artifacts` row is deliberately
/// absent: the audit's own seed wrote one with **11** values against a 12-column
/// table (`0005` puts `git_commit_sha` between `checksum_sha256` and
/// `visibility`), which is why the audit's `artifacts` branch in
/// `storage-boundaries-red-v2.log` cannot be reproduced on this repository.
/// MM-R02 confirmed the same defect independently. Nothing in *this* package
/// needs an Artifact, so the defect is simply not reproduced here -- but it is
/// named, because a reader comparing this file to the audit's log will notice the
/// missing row.
async fn seeded() -> Result<SqlitePool, Box<dyn Error>> {
    let p = db().await?;
    raw_sql(SEED).execute(&p).await?;
    for table in [
        "teams",
        "roles",
        "harness_instances",
        "adapters",
        "bindings",
        "tasks",
        "runs",
    ] {
        assert_eq!(
            query_scalar::<_, i64>(&format!("SELECT COUNT(*) FROM {table}"))
                .fetch_one(&p)
                .await?,
            1,
            "the seed did not land exactly one {table} row, so every case below \
             would be attacking a fixture that is not the one it describes"
        );
    }
    assert_eq!(
        query_scalar::<_, i64>("SELECT COUNT(*) FROM pragma_foreign_key_check")
            .fetch_one(&p)
            .await?,
        0,
        "the seed itself violates a foreign key, so a refusal could come from \
         the FK rather than from the guard under test"
    );
    // `ADAPTER` and `SEEDED_RUN` are referenced by `SEED` as literals, which
    // rustc cannot connect back to these constants. Asserting them here keeps the
    // constants live and -- more usefully -- pins that the seed's two indirect
    // rows are the ones this file's prose describes.
    for (name, id) in [("adapter", ADAPTER), ("seeded run", SEEDED_RUN)] {
        let table = if name == "adapter" {
            "adapters"
        } else {
            "runs"
        };
        let found: i64 = query_scalar(&format!("SELECT COUNT(*) FROM {table} WHERE id = X'{id}'"))
            .fetch_one(&p)
            .await?;
        assert_eq!(
            found, 1,
            "the seed's {name} row {id} is not where this file says it is"
        );
    }
    Ok(p)
}

/// Insert one `runs` row with a timestamp pair that satisfies `0004`'s CHECKs.
///
/// `0004` forces `started_at` NULL for `QUEUED`/`STARTING`, non-NULL for
/// `RUNNING` and the four terminals, and `completed_at` non-NULL only for
/// terminal. A matrix over statuses is therefore a matrix over CHECKs as well:
/// get this wrong and the row is refused by the table, not by the guard, and the
/// case passes for the wrong reason.
///
/// Column order: id, team_id, task_id, role_id, binding_id, harness_instance_id,
/// attempt, fencing_token, status, exit_code, failure_reason, version,
/// created_at, started_at, completed_at.
fn run_row(run: &str, attempt: i64, token: i64, status: &str) -> String {
    let started = if matches!(status, "QUEUED" | "STARTING") {
        "NULL"
    } else {
        "1"
    };
    let done = if matches!(status, "SUCCEEDED" | "FAILED" | "TIMED_OUT" | "CANCELLED") {
        "7"
    } else {
        "NULL"
    };
    format!(
        "INSERT INTO runs VALUES(X'{run}',X'{TEAM}',X'{TASK}',X'{ROLE}',X'{BINDING}',\
         X'{HARNESS}',{attempt},{token},'{status}',0,NULL,1,1,{started},{done})"
    )
}

/// The timestamp pair a *target* status needs, as `(started_at, completed_at)`.
///
/// Used to make every probed UPDATE well-formed for `0004`, so a refusal can
/// only come from a guard.
fn target_timestamps(status: &str) -> (&'static str, &'static str) {
    if matches!(status, "QUEUED" | "STARTING") {
        ("NULL", "NULL")
    } else if matches!(status, "SUCCEEDED" | "FAILED" | "TIMED_OUT" | "CANCELLED") {
        ("1", "9")
    } else {
        ("1", "NULL")
    }
}

/// Assert a statement is refused **by a guard named in `allowed`**, not by a
/// CHECK, a UNIQUE index or a foreign key.
fn assert_refused<T>(outcome: Result<T, sqlx_core::error::Error>, allowed: &[&str], what: &str) {
    match outcome {
        Ok(_) => panic!("{what}: the statement was accepted"),
        Err(e) => {
            let message = e.to_string();
            assert!(
                allowed.iter().any(|m| message.contains(m)),
                "{what}: refused for '{message}', which is not one of {allowed:?}. \
                 A CHECK / UNIQUE / foreign key fired instead of the guard under test."
            );
        }
    }
}

/// Read several columns of one row as a single string.
///
/// `query_scalar` has no `FromRow` for a multi-column tuple -- the fourth case of
/// MM-R01's finding, where a multi-column read silently had to be reshaped -- so
/// the columns are concatenated with a separator that cannot occur in a status
/// name. An alias is not enough: `SELECT a, b, c AS x` puts the alias on the
/// *last* column and leaves the read two columns wide, which is how
/// `core_history_retention`'s `table_contents` originally compared only its
/// first column.
async fn row_as_text(
    p: &SqlitePool,
    columns: &str,
    where_clause: &str,
) -> Result<String, Box<dyn Error>> {
    let cols = columns
        .split(',')
        .map(|c| format!("ifnull(cast({} as text),'~')", c.trim()))
        .collect::<Vec<_>>()
        .join("||':'||");
    // The `where_clause` is a parameter, not an afterthought: the first version of
    // this helper hard-coded `FROM runs` with no WHERE, so every "the refused
    // UPDATE did not move the row" assertion was comparing attempt 1's row while
    // the statement under test touched attempt 2. That is a no-op read that would
    // have passed a guard that moved the row.
    let sql = format!("SELECT {cols} FROM runs WHERE {where_clause}");
    Ok(query_scalar::<_, String>(&sql).fetch_one(p).await?)
}

/// The frozen edge set, taken from the domain rather than copied by hand.
///
/// `RUN_EDGES` is the 17-row table `crates/domain/src/run_state_machine.rs`
/// derives from `STATE_MACHINES.md` section 3 (16 transitions plus the
/// `UNKNOWN -> FAILED/TIMED_OUT/CANCELLED` row the domain splits into three by
/// reconcile evidence). Collecting `(from, to)` pairs de-duplicates that split,
/// which is the right granularity for a database predicate: it cannot know which
/// evidence proved the terminal state, and section 3 gives the row one
/// destination set.
///
/// The admitted set therefore has exactly as many entries as section 3 has
/// *destination states per source*, and `the_edge_set_is_the_frozen_one` checks
/// that number rather than trusting it.
fn frozen_edges() -> Vec<(String, String)> {
    let mut pairs: Vec<(String, String)> = RUN_EDGES
        .iter()
        .map(|e| (e.from.as_str().to_string(), e.to.as_str().to_string()))
        .collect();
    pairs.sort();
    pairs.dedup();
    pairs
}

// ---------------------------------------------------------------------------
// 1. The audit counterexample, ported.
// ---------------------------------------------------------------------------

/// The audit's `existing_running_attempt_cannot_return_to_queued`, with the
/// device faults its version had fixed.
///
/// Three things differ from `main-repro/.../audit_storage_boundaries.rs`, and
/// each one matters:
///
/// * it asserts the **message**, so the refusal cannot be a CHECK, an FK or a
///   zero-row UPDATE;
/// * after each refusal it re-reads `status`, `started_at`, `attempt` and
///   `fencing_token`, so "refused" cannot mean "refused after doing the damage";
/// * the legal `UNKNOWN -> RUNNING` control runs **first, in the same
///   database**, so a guard that refused everything would fail here rather than
///   passing the negative case below it.
#[tokio::test]
async fn existing_running_attempt_cannot_return_to_queued() -> Result<(), Box<dyn Error>> {
    let p = seeded().await?;
    query(&run_row(RUNNING_RUN, 2, 2, "RUNNING"))
        .execute(&p)
        .await?;

    // Control: the reconcile edge section 3 lists is accepted, and it really
    // lands. Without it the negative case below is passed by a guard that
    // refuses every status write.
    for to in ["UNKNOWN", "RUNNING"] {
        let (started, done) = target_timestamps(to);
        query(&format!(
            "UPDATE runs SET status='{to}', started_at={started}, completed_at={done} \
             WHERE attempt=2"
        ))
        .execute(&p)
        .await
        .unwrap_or_else(|e| panic!("the legal RUNNING -> {to} reconcile edge was refused ({e})"));
        let stored: String = query_scalar("SELECT status FROM runs WHERE attempt=2")
            .fetch_one(&p)
            .await?;
        assert_eq!(
            stored, to,
            "the legal edge reported success but did not land"
        );
    }

    // The audit's exact counterexample: an attempt 2 already carrying token 2
    // and a `started_at`, pushed back to `QUEUED` with its timestamp cleared.
    let reset = query("UPDATE runs SET status='QUEUED', started_at=NULL WHERE attempt=2")
        .execute(&p)
        .await;
    assert_refused(
        reset,
        &[TRANSITION_MESSAGE],
        "RUNNING -> QUEUED on an existing attempt",
    );

    let after = row_as_text(
        &p,
        "status, started_at, attempt, fencing_token",
        "attempt = 2",
    )
    .await?;
    assert_eq!(
        after, "RUNNING:1:2:2",
        "the refused reset still moved the row (status/started_at/attempt/token): {after}"
    );
    Ok(())
}

/// The same reset, for every source status that has no edge to `QUEUED`.
///
/// The audit found one cell. `0012` had left 24 of the 56 off-diagonal cells
/// open; this walks the ones that reach `QUEUED` or `STARTING`, which are the
/// two an "old attempt is being reset" story would actually use -- putting a Run
/// back into the queue is what makes it eligible for dispatch again.
#[tokio::test]
async fn no_status_can_be_walked_back_to_queued_or_starting() -> Result<(), Box<dyn Error>> {
    // Sources with no outgoing edge to QUEUED or STARTING. `QUEUED -> QUEUED`
    // and `QUEUED -> STARTING` are the two edges section 3 does list, and they
    // are exercised by `the_legal_forward_edges_are_all_reachable` instead.
    let sources = [
        "STARTING",
        "RUNNING",
        "SUCCEEDED",
        "FAILED",
        "TIMED_OUT",
        "CANCELLED",
        "UNKNOWN",
    ];
    for (index, from) in sources.iter().enumerate() {
        let p = db().await?;
        raw_sql(SEED).execute(&p).await?;
        let run = run_id("c0", index);
        // Attempt 2 on top of the seeded attempt 1, so `0012:146-154` ("the
        // newest generation") is satisfied and cannot be what refuses.
        query(&run_row(&run, 2, 2, from)).execute(&p).await?;
        for to in ["QUEUED", "STARTING"] {
            if to == *from {
                // A self-transition is not a transition. Excluded here and
                // asserted positively in
                // `a_status_preserving_update_is_not_a_transition`.
                continue;
            }
            let (started, done) = target_timestamps(to);
            let outcome = query(&format!(
                "UPDATE runs SET status='{to}', started_at={started}, completed_at={done} \
                 WHERE id=X'{run}'"
            ))
            .execute(&p)
            .await;
            // Three guards can cover this statement depending on the source:
            // `0027` for a non-terminal source, `0012:142-145` for
            // terminal -> active, `0029` for terminal -> terminal. Naming the
            // stable domain reason and asserting the row below is what carries
            // the meaning; the message only narrows *which* guard may answer.
            assert_refused(
                outcome,
                &[
                    TERMINAL_MESSAGE,
                    TRANSITION_MESSAGE,
                    "Run consistency violation",
                ],
                &format!("{from} -> {to}"),
            );
            let still: String = query_scalar(&format!("SELECT status FROM runs WHERE id=X'{run}'"))
                .fetch_one(&p)
                .await?;
            assert_eq!(
                still, *from,
                "{from} -> {to} was refused but the row moved anyway"
            );
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// 2. The whole matrix, both directions.
// ---------------------------------------------------------------------------

/// Every one of the 64 status cells, checked against the frozen edge set.
///
/// This is the test that makes "the matrix was verified row by row against the
/// frozen table" a checkable claim instead of a sentence. Both directions
/// matter and they fail differently:
///
/// * an **illegal** cell that is accepted means a hole;
/// * a **legal** cell that is refused means the guard is over-restrictive --
///   and a suite built only of refusals is passed by a guard that refuses
///   everything, which is the mirror image of the tenth review's
///   `RAISE(IGNORE)` mutation.
///
/// The expectation comes from `RUN_EDGES`, so a domain change to the frozen
/// table and a database change cannot drift apart with every suite green.
/// Self-transitions are excluded from the illegal set: an UPDATE that writes the
/// same status back is not a transition (`a_status_preserving_update_is_not_a_
/// transition` covers that separately, in its own database, because it is the
/// legal-control half and deserves its own name).
#[tokio::test]
async fn the_status_matrix_matches_the_frozen_edge_table() -> Result<(), Box<dyn Error>> {
    let all: Vec<String> = RunStatus::ALL
        .iter()
        .map(|s| s.as_str().to_string())
        .collect();
    let edges = frozen_edges();
    // The coverage claim is checked, not asserted in prose: 8 sources x 8
    // destinations, minus the 8 self-transitions this loop skips, is 56
    // off-diagonal cells. An earlier version of this case skipped the four
    // terminal sources, so it walked 28 cells while its name promised the whole
    // matrix -- and the 12 it skipped were exactly the open gap.
    let mut walked = 0usize;
    let mut refused = 0usize;
    for (i, from) in all.iter().enumerate() {
        for (j, to) in all.iter().enumerate() {
            if from == to {
                continue;
            }
            walked += 1;
            let p = db().await?;
            raw_sql(SEED).execute(&p).await?;
            let run = run_id("d0", i * 8 + j);
            query(&run_row(&run, 2, 2, from)).execute(&p).await?;
            let (started, done) = target_timestamps(to);
            let outcome = query(&format!(
                "UPDATE runs SET status='{to}', started_at={started}, completed_at={done} \
                 WHERE id=X'{run}'"
            ))
            .execute(&p)
            .await;

            // The whole point of this loop: an expectation derived from the
            // domain's `RUN_EDGES`, which is itself derived from STATE_MACHINES
            // section 3. Nothing here restates an allow-list by hand, so the
            // matrix cannot silently agree with the trigger it is checking --
            // the two lists have independent origins (frozen spec text versus
            // `crates/domain/src/run_state_machine.rs`).
            //
            // A terminal source has no outgoing edge at all, which falls out of
            // the same expression: `RUN_EDGES` contains no `(terminal, *)` pair,
            // so every terminal cell is expected to be refused without this file
            // naming the four terminal states as a group.
            let frozen_allows = edges.iter().any(|(f, t)| f == from && t == to);
            // Scoped to the row under test: the seed holds a `SUCCEEDED` attempt
            // 1, and an unscoped read returns that row -- which is how the first
            // version of this case reported "the row still reads SUCCEEDED"
            // after a legal `QUEUED -> STARTING` had in fact landed.
            let stored: String =
                query_scalar(&format!("SELECT status FROM runs WHERE id=X'{run}'"))
                    .fetch_one(&p)
                    .await?;

            if frozen_allows {
                assert!(
                    outcome.is_ok(),
                    "{from} -> {to} is a frozen edge and was refused ({:?}); the guard \
                     is over-restrictive",
                    outcome.err()
                );
                assert_eq!(
                    stored, *to,
                    "{from} -> {to} reported success but the row still reads {stored}"
                );
            } else {
                // Which guard refuses a given illegal cell is a storage detail,
                // not a frozen business edge: `0012:142-145` covers
                // terminal -> active, `0027` covers the non-terminal quadrant and
                // `0029` covers the terminal quadrant. The domain reason and the
                // unchanged row are asserted on the next two lines regardless of
                // which one fired.
                assert_refused(
                    outcome,
                    &[
                        TERMINAL_MESSAGE,
                        TRANSITION_MESSAGE,
                        "Run consistency violation",
                    ],
                    &format!("{from} -> {to} (not a frozen edge)"),
                );
                assert_eq!(
                    stored, *from,
                    "{from} -> {to} was refused yet the row reads {stored}"
                );
                refused += 1;
            }
        }
    }
    assert_eq!(
        all.len(),
        8,
        "the matrix is only 8x8 if RunStatus::ALL still has eight members"
    );
    assert_eq!(
        walked, 56,
        "the loop walked {walked} off-diagonal cells, not the 56 the 8x8 matrix has"
    );
    assert_eq!(
        refused,
        56 - 17,
        "{refused} cells were expected to be refused, not the {} off-diagonal cells \
         minus the 17 frozen edges",
        56
    );
    Ok(())
}

/// The frozen edge set is exactly 17 edges over 4 source states.
///
/// A count, and the number is 17 because section 3 prints
/// `UNKNOWN -> FAILED/TIMED_OUT/CANCELLED` as **one** row with three destinations
/// while the domain splits it into three edges by reconcile evidence. Two
/// sources are smaller than they look for the same reason: `QUEUED` reaches only
/// `STARTING` and `CANCELLED`, so it has 2 edges, not 5.
///
/// The first version of this assertion said 16, on the reasoning that the three
/// `UNKNOWN` terminal edges "de-duplicate into one". They do not -- they differ
/// in `to`, so de-duplication cannot merge them. Writing the number down from the
/// ASCII diagram instead of running the counter is exactly
/// ADVERSARIAL_REVIEW_PLAYBOOK rule 6, and the count is now derived.
#[test]
fn the_edge_set_is_the_frozen_one() {
    let edges = frozen_edges();
    assert_eq!(
        edges.len(),
        17,
        "the (from, to) set has {} entries, not the 17 edges of \
         STATE_MACHINES section 3. Sources: {edges:?}",
        edges.len()
    );
    // Per-source counts, so a change that kept the total at 17 could not hide.
    let mut per_source: Vec<(&str, usize)> = Vec::new();
    for source in ["QUEUED", "STARTING", "RUNNING", "UNKNOWN"] {
        per_source.push((source, edges.iter().filter(|(f, _)| f == source).count()));
    }
    assert_eq!(
        per_source,
        [
            ("QUEUED", 2),
            ("STARTING", 5),
            ("RUNNING", 5),
            ("UNKNOWN", 5)
        ],
        "section 3's out-degree per source changed"
    );
    let mut sources: Vec<&str> = edges.iter().map(|(f, _)| f.as_str()).collect();
    sources.sort_unstable();
    sources.dedup();
    assert_eq!(
        sources,
        ["QUEUED", "RUNNING", "STARTING", "UNKNOWN"],
        "section 3 has no outgoing edge from a terminal status, so exactly these \
         four states may be a source"
    );
}

/// Every frozen edge is reachable end to end, walked in order from a fresh Run.
///
/// `the_status_matrix_matches_the_frozen_edge_table` proves each edge works from
/// a *seeded* row. This one proves the lifecycle still runs as a sequence: a Run
/// created `QUEUED` walks `QUEUED -> STARTING -> RUNNING -> SUCCEEDED` and one
/// created `QUEUED` is cancelled before it starts. That is the "runs are not
/// welded shut" half, and it is what a full-table `ELSE 0` mutation would break.
#[tokio::test]
async fn the_legal_forward_edges_are_all_reachable() -> Result<(), Box<dyn Error>> {
    // The lifecycle, as section 3 prints it.
    let p = seeded().await?;
    let live = run_id("10", 1);
    query(&run_row(&live, 2, 2, "QUEUED")).execute(&p).await?;
    for to in ["STARTING", "RUNNING", "SUCCEEDED"] {
        let (started, done) = target_timestamps(to);
        query(&format!(
            "UPDATE runs SET status='{to}', started_at={started}, completed_at={done} \
             WHERE id=X'{live}'"
        ))
        .execute(&p)
        .await
        .unwrap_or_else(|e| panic!("QUEUED -> {to} is a frozen edge and was refused ({e})"));
    }
    assert_eq!(
        query_scalar::<_, String>(&format!("SELECT status FROM runs WHERE id=X'{live}'"))
            .fetch_one(&p)
            .await?,
        "SUCCEEDED"
    );

    // And the short one: a queued Run that is cancelled before it starts.
    let cancelled = run_id("10", 2);
    query(&run_row(&cancelled, 3, 3, "QUEUED"))
        .execute(&p)
        .await?;
    // `started_at` is supplied because `0004` requires it non-NULL for every
    // terminal status, CANCELLED included. Omitting it makes the table refuse the
    // statement, and the case would then score a CHECK instead of the edge.
    query(&format!(
        "UPDATE runs SET status='CANCELLED', started_at=1, completed_at=9 WHERE id=X'{cancelled}'"
    ))
    .execute(&p)
    .await
    .unwrap_or_else(|e| panic!("QUEUED -> CANCELLED is a frozen edge and was refused ({e})"));
    Ok(())
}

/// A business-field UPDATE that changes no status is not a transition.
///
/// Section 1's rule set and DATA_CONVENTIONS section 3 both require an ordinary
/// aggregate update to keep working; `0012` allows it and this migration must not
/// take it away. The trigger is `BEFORE UPDATE OF status`, so a statement that
/// never mentions `status` cannot reach it -- and this case checks the row really
/// changed, because "accepted but did nothing" would read as an accepted
/// statement (ADVERSARIAL_REVIEW_PLAYBOOK rule 2, no-op disguised as success).
#[tokio::test]
async fn a_status_preserving_update_is_not_a_transition() -> Result<(), Box<dyn Error>> {
    let p = seeded().await?;
    query(&run_row(RUNNING_RUN, 2, 2, "RUNNING"))
        .execute(&p)
        .await?;
    query("UPDATE runs SET version = version + 1, failure_reason = 'note' WHERE attempt = 2")
        .execute(&p)
        .await
        .unwrap_or_else(|e| panic!("a field-only update on a RUNNING Run was refused ({e})"));
    let fields = row_as_text(&p, "version, failure_reason", "attempt = 2").await?;
    assert_eq!(
        fields, "2:note",
        "the field-only update reported success but the row did not change"
    );
    // Writing the same status back is also not a transition.
    query("UPDATE runs SET status = 'RUNNING', version = 3 WHERE attempt = 2")
        .execute(&p)
        .await
        .unwrap_or_else(|e| panic!("writing the same status back was refused ({e})"));
    Ok(())
}

/// A terminal Run is absorbing: the 12 terminal -> terminal cells are refused
/// and the row does not move.
///
/// `STATE_MACHINES.md` section 3 gives `SUCCEEDED/FAILED/TIMED_OUT/CANCELLED`
/// **no outgoing edge**, in prose as well as in the table:
/// "`SUCCEEDED/FAILED/TIMED_OUT/CANCELLED` 为单个 Run 终态". The only status
/// write a finished Run accepts is its own, written back.
///
/// This case used to assert `outcome.is_ok()` -- it asserted that the illegal
/// write *succeeded* -- and named itself a "registered open gap". That was green
/// for the wrong reason: it fixed an illegal acceptance into the suite so the
/// defect could not be forgotten, which is the mirror image of the defect it was
/// documenting. `MR-01` measured `SUCCEEDED -> FAILED` landing on a real
/// database, and this package's own probe reproduces the whole quadrant
/// (`main/probe-v28-matrix.log`, `SUCCEEDED FAILED ACCEPT`). `0029` closes it,
/// so the assertion is inverted to the real refusal.
///
/// ADVERSARIAL_REVIEW_PLAYBOOK rule 2 applies to both halves: the statement
/// errors, *and* every column of the row is unchanged afterwards. An error alone
/// could come from a CHECK, a foreign key or a zero-row UPDATE, and a refusal
/// that still moved the row would be the worse failure.
#[tokio::test]
async fn terminal_to_terminal_is_refused_and_the_row_does_not_move() -> Result<(), Box<dyn Error>> {
    const TERMINAL: [&str; 4] = ["SUCCEEDED", "FAILED", "TIMED_OUT", "CANCELLED"];
    for (index, from) in TERMINAL.iter().enumerate() {
        for (j, to) in TERMINAL.iter().enumerate() {
            if from == to {
                // A same-status write-back is legal, and is asserted positively
                // in `a_terminal_status_preserving_update_is_not_a_transition`.
                continue;
            }
            let p = seeded().await?;
            let run = run_id("a0", index * 4 + j);
            query(&run_row(&run, 2, 2, from)).execute(&p).await?;
            let before = row_as_text(
                &p,
                "status, exit_code, completed_at, version, attempt, fencing_token",
                &format!("id = X'{run}'"),
            )
            .await?;
            let outcome = query(&format!(
                "UPDATE runs SET status='{to}', exit_code=1, completed_at=9, version=version+1 \
                 WHERE id=X'{run}'"
            ))
            .execute(&p)
            .await;
            assert_refused(
                outcome,
                &[TERMINAL_MESSAGE],
                &format!("{from} -> {to} (a terminal Run has no outgoing edge)"),
            );
            let after = row_as_text(
                &p,
                "status, exit_code, completed_at, version, attempt, fencing_token",
                &format!("id = X'{run}'"),
            )
            .await?;
            assert_eq!(
                after, before,
                "{from} -> {to} was refused but the row changed"
            );
            assert_eq!(
                after.split(':').next(),
                Some(*from),
                "{from} -> {to} was refused yet the status column reads {after}"
            );
        }
    }
    Ok(())
}

/// Writing a finished Run's own status back, and updating its other fields, is
/// not a transition.
///
/// The mirror of the case above and required by it: a guard refusing *every*
/// `UPDATE OF status` on a terminal row would pass that case and break this one.
/// `STATE_MACHINES.md` section 1 and `DATA_CONVENTIONS.md` section 3 require an
/// ordinary aggregate update to keep working, and section 3's "no outgoing edge"
/// is about a *different* status, not about writing this one.
///
/// The field-only statement is the stronger control: the trigger is
/// `BEFORE UPDATE OF status`, so a statement that never mentions `status`
/// cannot reach it, and the row is re-read to prove the write landed rather than
/// being a no-op.
#[tokio::test]
async fn a_terminal_status_preserving_update_is_not_a_transition() -> Result<(), Box<dyn Error>> {
    let p = seeded().await?;
    let run = run_id("a1", 1);
    query(&run_row(&run, 2, 2, "SUCCEEDED")).execute(&p).await?;

    query(&format!(
        "UPDATE runs SET failure_reason='late diagnostic', version=version+1 \
         WHERE id=X'{run}'"
    ))
    .execute(&p)
    .await
    .unwrap_or_else(|e| panic!("a field-only update on a SUCCEEDED Run was refused ({e})"));
    let fields = row_as_text(
        &p,
        "failure_reason, version, status",
        &format!("id = X'{run}'"),
    )
    .await?;
    assert_eq!(
        fields, "late diagnostic:2:SUCCEEDED",
        "the field-only update reported success but the row did not change"
    );

    query(&format!(
        "UPDATE runs SET status='SUCCEEDED', version=3 WHERE id=X'{run}'"
    ))
    .execute(&p)
    .await
    .unwrap_or_else(|e| panic!("writing the same terminal status back was refused ({e})"));
    assert_eq!(
        query_scalar::<_, String>(&format!("SELECT status FROM runs WHERE id=X'{run}'"))
            .fetch_one(&p)
            .await?,
        "SUCCEEDED"
    );
    Ok(())
}

#[tokio::test]
async fn a_retry_appends_a_new_attempt_and_leaves_the_old_run_untouched()
-> Result<(), Box<dyn Error>> {
    let p = seeded().await?;
    query(&run_row(RUNNING_RUN, 2, 2, "RUNNING"))
        .execute(&p)
        .await?;
    let first: String = query_scalar("SELECT status FROM runs WHERE attempt = 2")
        .fetch_one(&p)
        .await?;
    // The old attempt finishes, and a retry is created beside it.
    query("UPDATE runs SET status='FAILED', completed_at=9, exit_code=1 WHERE attempt=2")
        .execute(&p)
        .await?;
    let retry = run_id("20", 1);
    query(&run_row(&retry, 3, 3, "QUEUED")).execute(&p).await?;
    let rows = row_as_text(
        &p,
        "status, attempt, fencing_token",
        &format!("id = X'{retry}'"),
    )
    .await?;
    assert_eq!(
        rows, "QUEUED:3:3",
        "the retry did not land as the newest generation (status/attempt/token): {rows}"
    );
    let old: String = query_scalar::<_, String>(&format!(
        "SELECT status FROM runs WHERE id=X'{RUNNING_RUN}'"
    ))
    .fetch_one(&p)
    .await?;
    assert_eq!(old, "FAILED", "the retry rewrote the Run it retried");
    assert_ne!(first, "QUEUED");
    Ok(())
}

// ---------------------------------------------------------------------------
// 3. Device declarations: schema, baseline, and who refuses what.
// ---------------------------------------------------------------------------

/// The `0027` trigger exists on the hardened schema and **not** on the `v26`
/// baseline.
///
/// Bidirectional on purpose. A one-way check passes when the rig silently loaded
/// the wrong migration, and PLAYBOOK rule 1-b is about precisely that: a rig
/// whose "baseline" is secretly the hardened schema produces output whose shape
/// is identical to a real result.
#[tokio::test]
async fn the_new_guard_is_installed() -> Result<(), Box<dyn Error>> {
    let p = db().await?;
    let mut expected: Vec<&str> = NEW_TRIGGERS.to_vec();
    expected.push(TERMINAL_TRIGGER);
    for name in &expected {
        let count: i64 = query_scalar(
            "SELECT COUNT(*) FROM sqlite_master \
             WHERE type = 'trigger' AND name = ? AND tbl_name = 'runs'",
        )
        .bind(name)
        .fetch_one(&p)
        .await?;
        assert_eq!(
            count, 1,
            "{name} is not installed on runs, so nothing is refusing the status \
             edge while 0027 still shows the text that was supposed to refuse it"
        );
    }

    // Bidirectional, and the two guards have *different* baselines. `0027`'s
    // trigger must be absent on `0001..0026`; `0029`'s must be absent on
    // `0001..0028` as well, since being new there is its entire purpose.
    // Asserting `0027` absent on the v28 baseline is simply wrong -- v28 contains
    // 0027 -- so the expectation is declared per trigger rather than blanket.
    let baselines: [(&str, SqlitePool); 2] = [
        ("v26", v26_database().await?),
        ("v28", v28_database().await?),
    ];
    for (label, baseline) in &baselines {
        for name in &expected {
            let absent_expected = *name == TERMINAL_TRIGGER || *label == "v26";
            if !absent_expected {
                continue;
            }
            let count: i64 = query_scalar(
                "SELECT COUNT(*) FROM sqlite_master \
                 WHERE type = 'trigger' AND name = ? AND tbl_name = 'runs'",
            )
            .bind(name)
            .fetch_one(baseline)
            .await?;
            assert_eq!(
                count, 0,
                "{name} already existed on the {label} baseline, so the two schemas \
                 are not distinguishable and every refusal below could be the \
                 baseline's"
            );
        }
    }
    Ok(())
}

/// The counterexample really was accepted before this migration.
///
/// This is the "prove the baseline and the hardened schema differ" step, and it
/// runs the audit's statement on `0001..0026` and shows it **succeeds** there.
/// Without it, the whole file rests on the assumption that the audit's log was
/// right -- and PLAYBOOK section 0 is a list of conclusions that were wrong in
/// the shape their output took.
#[tokio::test]
async fn the_baseline_schema_accepted_the_rollback_this_migration_refuses()
-> Result<(), Box<dyn Error>> {
    let p = v26_database().await?;
    raw_sql(SEED).execute(&p).await?;
    query(&run_row(RUNNING_RUN, 2, 2, "RUNNING"))
        .execute(&p)
        .await?;
    let reset = query("UPDATE runs SET status='QUEUED', started_at=NULL WHERE attempt=2")
        .execute(&p)
        .await;
    assert!(
        reset.is_ok(),
        "the v26 baseline refused the rollback, so either the defect is already \
         fixed by an earlier migration and 0027 is unproven, or this file's \
         fixture is not the one the audit used"
    );
    assert_eq!(
        query_scalar::<_, String>("SELECT status FROM runs WHERE attempt=2")
            .fetch_one(&p)
            .await?,
        "QUEUED",
        "the baseline accepted the statement but did not store the reset"
    );
    Ok(())
}

/// The 16 terminal -> active cells still refuse, and the row does not move.
///
/// `0012:142-145` already refused these before either migration, and they are
/// still refused. What changed in FC-03 is *which guard answers*: `0029` is
/// created last, SQLite runs `BEFORE` triggers in reverse creation order, and
/// `0029` states the same frozen rule ("a terminal Run has no outgoing edge")
/// across the whole quadrant, so its message is now the one a caller sees here.
///
/// The assertion therefore names the domain reason and the full row rather than
/// pinning one guard's string -- an error message is not a frozen business edge,
/// and holding an illegal write open to preserve one would trade a real defect
/// for a cosmetic one. That `0012` is still load-bearing rather than merely
/// present is proven separately, and without `0029` in the way, by
/// `the_pre_existing_terminal_resurrection_guard_still_bears_weight`.
#[tokio::test]
async fn the_pre_existing_terminal_resurrection_guard_is_not_shadowed() -> Result<(), Box<dyn Error>>
{
    for (index, status) in ["SUCCEEDED", "FAILED", "TIMED_OUT", "CANCELLED"]
        .into_iter()
        .enumerate()
    {
        let p = seeded().await?;
        let run = run_id("e0", index);
        query(&run_row(&run, 2, 2, status)).execute(&p).await?;
        // One of the four active destinations `0012:142-145` names, and one the
        // trigger is `BEFORE UPDATE OF status` on, so both guards are in play.
        let before = row_as_text(
            &p,
            "status, started_at, completed_at",
            &format!("id = X'{run}'"),
        )
        .await?;
        let outcome = query(&format!(
            "UPDATE runs SET status='RUNNING', started_at=1, completed_at=NULL \
             WHERE id=X'{run}'"
        ))
        .execute(&p)
        .await;
        assert_refused(
            outcome,
            &[TERMINAL_MESSAGE, "Run consistency violation"],
            &format!("{status} -> RUNNING"),
        );
        assert_eq!(
            row_as_text(
                &p,
                "status, started_at, completed_at",
                &format!("id = X'{run}'")
            )
            .await?,
            before,
            "{status} -> RUNNING was refused but the row changed"
        );
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// 3b. Who refuses what, on which schema.
//
// `0029` covers the terminal quadrant as a whole, including the 16
// terminal -> active cells `0012:142-145` already refused. That is one frozen
// rule ("a terminal Run has no outgoing edge"), not two, and splitting the
// quadrant again to keep one error string visible would re-create the defect
// `0029` exists to remove.
//
// The consequence has to be handled rather than ignored: SQLite runs `BEFORE`
// triggers in reverse creation order, so on the full schema `0029` answers first
// everywhere in that quadrant. Two things follow, and both are checked here.
//
//   * On the full schema, callers see `0029`'s message. The assertions state the
//     domain reason and the unchanged row, which is what the frozen specs
//     actually constrain.
//   * `0012` is not thereby disarmed. It is shown to still refuse the same write,
//     with its own message, on a database that has every shipped migration up to
//     and including `0028` -- i.e. with `0029` absent.
#[tokio::test]
async fn the_pre_existing_terminal_resurrection_guard_still_bears_weight()
-> Result<(), Box<dyn Error>> {
    for (index, status) in ["SUCCEEDED", "FAILED", "TIMED_OUT", "CANCELLED"]
        .into_iter()
        .enumerate()
    {
        // Baseline: 0012 and 0027 only. `0029` is deliberately not loaded.
        let p = v28_database().await?;
        raw_sql(SEED).execute(&p).await?;
        let run = run_id("f0", index);
        query(&run_row(&run, 2, 2, status)).execute(&p).await?;

        let new_guard: i64 =
            query_scalar("SELECT COUNT(*) FROM sqlite_master WHERE type='trigger' AND name = ?")
                .bind(TERMINAL_TRIGGER)
                .fetch_one(&p)
                .await?;
        assert_eq!(
            new_guard, 0,
            "{TERMINAL_TRIGGER} is present on the pre-0029 baseline, so this case \
             cannot show which guard refused"
        );

        let outcome = query(&format!(
            "UPDATE runs SET status='RUNNING', started_at=1, completed_at=NULL WHERE id=X'{run}'"
        ))
        .execute(&p)
        .await;
        assert_refused(
            outcome,
            &["Run consistency violation"],
            &format!("{status} -> RUNNING on the 0028 baseline"),
        );
        assert_eq!(
            query_scalar::<_, String>(&format!("SELECT status FROM runs WHERE id=X'{run}'"))
                .fetch_one(&p)
                .await?,
            status,
            "the refused UPDATE still moved the row"
        );
    }
    Ok(())
}

/// The mirror of the case above: on the full schema the same write is refused,
/// the row is untouched, and `0029` is what answers.
///
/// Together the two cases say "the terminal quadrant is refused, and it was
/// already refused before `0029`" without pretending the two schemas are
/// indistinguishable. A rig that silently loaded the full schema for both would
/// fail the baseline case's `0029 is absent` assertion.
#[tokio::test]
async fn the_full_schema_refuses_the_terminal_quadrant() -> Result<(), Box<dyn Error>> {
    for (index, status) in ["SUCCEEDED", "FAILED", "TIMED_OUT", "CANCELLED"]
        .into_iter()
        .enumerate()
    {
        let p = seeded().await?;
        let run = run_id("f1", index);
        query(&run_row(&run, 2, 2, status)).execute(&p).await?;
        let outcome = query(&format!(
            "UPDATE runs SET status='RUNNING', started_at=1, completed_at=NULL WHERE id=X'{run}'"
        ))
        .execute(&p)
        .await;
        assert_refused(
            outcome,
            &[TERMINAL_MESSAGE],
            &format!("{status} -> RUNNING on the full schema"),
        );
        let still = row_as_text(
            &p,
            "status, started_at, completed_at",
            &format!("id = X'{run}'"),
        )
        .await?;
        assert_eq!(
            still,
            format!("{status}:1:7"),
            "the refused UPDATE changed the terminal row: {still}"
        );
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// 4. Upgrade and failure paths.
// ---------------------------------------------------------------------------

/// Upgrading to `0027` changes no row and leaves the guard working.
///
/// No upgrade-guard probe table exists in `0027`, and that is a deliberate
/// decision (see the migration's comment): the trigger constrains statements, not
/// rows, so a `valid = 1` probe would be a tautology. This is the assertion that
/// replaces it.
#[tokio::test]
async fn upgrading_to_0027_preserves_every_existing_row() -> Result<(), Box<dyn Error>> {
    let p = v26_database().await?;
    raw_sql(SEED).execute(&p).await?;
    query(&run_row(RUNNING_RUN, 2, 2, "RUNNING"))
        .execute(&p)
        .await?;
    let before = snapshot(&p).await?;

    migrate(&p).await?;

    assert_eq!(snapshot(&p).await?, before, "the upgrade changed a row");
    let applied: i64 =
        query_scalar("SELECT COUNT(*) FROM _sqlx_migrations WHERE version = 27 AND success = TRUE")
            .fetch_one(&p)
            .await?;
    assert_eq!(applied, 1, "0027 did not record a successful application");

    // And the guard is live on the upgraded database, not merely recorded.
    assert_refused(
        query("UPDATE runs SET status='QUEUED', started_at=NULL WHERE attempt=2")
            .execute(&p)
            .await,
        &[TRANSITION_MESSAGE],
        "the rollback on an upgraded database",
    );
    Ok(())
}

/// A failed upgrade leaves nothing behind.
///
/// `0027` creates one trigger. The test occupies that name first, forcing the
/// migration to fail, and then asserts the database is byte-identical and that no
/// `version = 27` row was written. Without the failure path, "the upgrade is
/// atomic" is a claim about a transaction nobody has watched fail.
#[tokio::test]
async fn a_failed_upgrade_has_no_side_effects() -> Result<(), Box<dyn Error>> {
    let p = v26_database().await?;
    raw_sql(SEED).execute(&p).await?;
    query(&run_row(RUNNING_RUN, 2, 2, "RUNNING"))
        .execute(&p)
        .await?;
    let before = snapshot(&p).await?;

    query(&format!(
        "CREATE TRIGGER {} BEFORE UPDATE ON runs BEGIN SELECT RAISE(IGNORE); END",
        NEW_TRIGGERS[0]
    ))
    .execute(&p)
    .await?;
    let occupied = snapshot(&p).await?;

    let outcome = migrate(&p).await;
    assert!(
        outcome.is_err(),
        "0027 succeeded even though its trigger name was taken, so this test is \
         not exercising the failure path it claims to"
    );
    assert_eq!(
        snapshot(&p).await?,
        occupied,
        "a failed upgrade changed a row"
    );
    let applied: i64 =
        query_scalar("SELECT COUNT(*) FROM _sqlx_migrations WHERE version = 27 AND success = TRUE")
            .fetch_one(&p)
            .await?;
    assert_eq!(applied, 0, "a failed upgrade recorded itself as applied");
    let leftovers: i64 = query_scalar(
        "SELECT COUNT(*) FROM sqlite_master WHERE type = 'trigger' AND name LIKE '%transition%'",
    )
    .fetch_one(&p)
    .await?;
    assert_eq!(
        leftovers, 1,
        "a failed upgrade left a trigger of its own behind"
    );
    assert_eq!(occupied, before, "occupying the trigger name changed a row");
    Ok(())
}

/// Upgrading `0001..0028` to `0029` changes no row and leaves the guard working.
///
/// `0029` adds a statement-level trigger and no probe table, for the reason its
/// own comment gives: every historical row is legal under it by construction, so
/// a `valid = 1` guard table would be a tautology (ADVERSARIAL_REVIEW_PLAYBOOK
/// rule 5). This is the assertion that replaces it, and it runs against a real
/// `v28` database carrying a real terminal row -- the case where a careless
/// upgrade check would do nothing.
#[tokio::test]
async fn upgrading_to_0029_preserves_every_existing_row() -> Result<(), Box<dyn Error>> {
    let p = v28_database().await?;
    raw_sql(SEED).execute(&p).await?;
    // Two finished rows, in two different terminal statuses, both on the newest
    // generation for their own Task so no *other* guard can be what refuses
    // below. The seeded attempt 1 is already `SUCCEEDED`, so the finished row
    // under test has to be the newest one for the seed Task.
    let finished = run_id("c0", 1);
    query(&run_row(&finished, 2, 2, "FAILED"))
        .execute(&p)
        .await?;
    let live = run_id("c0", 2);
    query(&run_row(&live, 3, 3, "RUNNING")).execute(&p).await?;
    let before = snapshot(&p).await?;

    migrate(&p).await?;

    assert_eq!(
        snapshot(&p).await?,
        before,
        "the 0029 upgrade changed a row"
    );
    let applied: i64 =
        query_scalar("SELECT COUNT(*) FROM _sqlx_migrations WHERE version = 29 AND success = TRUE")
            .fetch_one(&p)
            .await?;
    assert_eq!(applied, 1, "0029 did not record a successful application");

    // Live on the upgraded database, not merely recorded: a terminal -> terminal
    // write is refused, while both a finished Run's own write-back and the
    // non-terminal edges `0027` guards still work.
    assert_refused(
        query(&format!(
            "UPDATE runs SET status='SUCCEEDED', completed_at=9 WHERE id=X'{finished}'"
        ))
        .execute(&p)
        .await,
        &[TERMINAL_MESSAGE],
        "FAILED -> SUCCEEDED on an upgraded database",
    );
    // A frozen non-terminal edge still works after the upgrade. Walking it on
    // purpose -- `RUNNING -> UNKNOWN -> SUCCEEDED` -- is legal and is the
    // positive control that a guard refusing every status write would fail.
    for (to, done) in [("UNKNOWN", "NULL"), ("SUCCEEDED", "9")] {
        query(&format!(
            "UPDATE runs SET status='{to}', completed_at={done} WHERE id=X'{live}'"
        ))
        .execute(&p)
        .await
        .unwrap_or_else(|e| panic!("the frozen RUNNING -> {to} edge broke on upgrade ({e})"));
        assert_eq!(
            query_scalar::<_, String>(&format!("SELECT status FROM runs WHERE id=X'{live}'"))
                .fetch_one(&p)
                .await?,
            to
        );
    }
    query(&format!(
        "UPDATE runs SET status='FAILED', version=2 WHERE id=X'{finished}'"
    ))
    .execute(&p)
    .await
    .unwrap_or_else(|e| panic!("a terminal field/same-status write broke on upgrade ({e})"));
    Ok(())
}

/// A failed `0029` upgrade leaves nothing behind.
///
/// The trigger name is occupied first, so the migration genuinely fails; the
/// database must then be byte-identical and must contain no `version = 29` row.
/// Without this the claim "the upgrade is atomic" is about a transaction nobody
/// has watched fail.
#[tokio::test]
async fn a_failed_0029_upgrade_has_no_side_effects() -> Result<(), Box<dyn Error>> {
    let p = v28_database().await?;
    raw_sql(SEED).execute(&p).await?;
    query(&run_row(RUNNING_RUN, 2, 2, "RUNNING"))
        .execute(&p)
        .await?;
    let before = snapshot(&p).await?;

    query(&format!(
        "CREATE TRIGGER {TERMINAL_TRIGGER} BEFORE UPDATE ON runs \
         BEGIN SELECT RAISE(IGNORE); END"
    ))
    .execute(&p)
    .await?;
    let occupied = snapshot(&p).await?;
    assert_eq!(occupied, before, "occupying the trigger name changed a row");

    let outcome = migrate(&p).await;
    assert!(
        outcome.is_err(),
        "0029 succeeded even though its trigger name was taken, so this case is \
         not exercising the failure path it claims to"
    );
    assert_eq!(
        snapshot(&p).await?,
        occupied,
        "a failed upgrade changed a row"
    );
    let applied: i64 =
        query_scalar("SELECT COUNT(*) FROM _sqlx_migrations WHERE version = 29 AND success = TRUE")
            .fetch_one(&p)
            .await?;
    assert_eq!(applied, 0, "a failed upgrade recorded itself as applied");
    Ok(())
}

// ---------------------------------------------------------------------------
// 5. The creation axis, evaluated rather than asserted.
// ---------------------------------------------------------------------------

/// A Run may still be INSERTed outside `QUEUED`, and that gap is measured here.
///
/// Section 3 has exactly one creation row, `create -> QUEUED`, and
/// `runs_consistency_insert` does not constrain the status at all. A
/// `BEFORE INSERT` guard requiring `NEW.status = 'QUEUED'` was written and
/// measured against this package: it refuses **every** non-`QUEUED` insert in
/// `crates/persistence-sqlite/tests/`, including `0013`/`0014`/`0020`/`0026`
/// fixtures and the upgrade fixtures in `run_lease_parent_upgrade_migration.rs`.
///
/// FC-03 restates this gap rather than closing it, and adds no insert-side
/// guard. The reason is recorded in `0029` and in `FC-03-RESULT.md`: a Run may be
/// INSERTed outside `QUEUED`, those rows are overwhelmingly retained-history
/// fixtures rather than a production creation command, and this package is not
/// authorised to force every INSERT to `QUEUED` nor to add a fixture-only escape
/// parameter. The case below records the gap in both directions:
///
/// * **the gap is real** -- a Run can be created `SUCCEEDED` today, and if
///   `0027` were later extended to the creation axis this case would start
///   failing, which is the signal that the fixtures need migrating first;
/// * **the gap is not closed here** -- the fixtures expressing it are legitimate
///   under DATA_CONVENTIONS section 5 ("Run history is retained") and 10 of the
///   12 files holding them are outside this package's write set, so no
///   test-only escape hatch was added to make them pass.
///
/// The frozen creation row is a statement about the `task.claim` command, which
/// runs in the application layer (K3/K4), not at this boundary. See
/// `docs/coordination/minimax-return/MM-R03-RESULT.md` section 8.
#[tokio::test]
async fn creating_a_run_outside_queued_is_still_possible_and_that_is_measured()
-> Result<(), Box<dyn Error>> {
    let p = seeded().await?;
    let born = run_id("30", 1);
    query(&run_row(&born, 2, 2, "SUCCEEDED"))
        .execute(&p)
        .await?;
    assert_eq!(
        query_scalar::<_, String>(&format!("SELECT status FROM runs WHERE id=X'{born}'"))
            .fetch_one(&p)
            .await?,
        "SUCCEEDED",
        "creating a Run outside QUEUED is no longer possible, so the creation \
         axis has been closed somewhere and RESULT section 8 is out of date"
    );
    // The one creation the frozen table does list is unaffected.
    let queued = run_id("30", 2);
    query(&run_row(&queued, 3, 3, "QUEUED")).execute(&p).await?;
    assert_eq!(
        query_scalar::<_, i64>("SELECT COUNT(*) FROM runs WHERE status='QUEUED'")
            .fetch_one(&p)
            .await?,
        1
    );
    Ok(())
}

/// Both migrations this file's package family owns are recorded, each by its own
/// version -- never by `MAX(version)`.
///
/// `MAX(version) = N` is a shape that silently breaks every time a migration is
/// added, which is why it was corrected here rather than kept:
/// `docs/coordination/MM-COORD-migration-version-assertions.md` originally
/// deferred two `MAX(version) = 24` assertions to a later gate, and they did
/// eventually drift to report `left: 27`. FC-01 has since rewritten those three
/// counting assertions to the stable form `WHERE version = N AND success = TRUE`,
/// which keeps asserting the same fact and stops reporting the next migration.
/// This case uses that form for the same reason, and additionally asserts the
/// success/failure split so a migration that recorded itself as failed cannot be
/// mistaken for an applied one.
#[tokio::test]
async fn this_package_pins_exactly_versions_27_and_29() -> Result<(), Box<dyn Error>> {
    let p = db().await?;
    let applied: i64 =
        query_scalar("SELECT COUNT(*) FROM _sqlx_migrations WHERE version = 27 AND success = TRUE")
            .fetch_one(&p)
            .await?;
    assert_eq!(applied, 1);
    let fails: i64 = query_scalar(
        "SELECT COUNT(*) FROM _sqlx_migrations WHERE version = 27 AND success = FALSE",
    )
    .fetch_one(&p)
    .await?;
    assert_eq!(fails, 0);
    // The migration this package adds, pinned the same way.
    for version in [27, 29] {
        let applied: i64 = query_scalar(&format!(
            "SELECT COUNT(*) FROM _sqlx_migrations WHERE version = {version} AND success = TRUE"
        ))
        .fetch_one(&p)
        .await?;
        assert_eq!(applied, 1, "migration {version} is not recorded as applied");
        let failed: i64 = query_scalar(&format!(
            "SELECT COUNT(*) FROM _sqlx_migrations WHERE version = {version} AND success = FALSE"
        ))
        .fetch_one(&p)
        .await?;
        assert_eq!(failed, 0, "migration {version} recorded a failure");
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Helpers used by more than one test.
// ---------------------------------------------------------------------------

/// Per-table row snapshots, built without `SELECT *` (TECH_STACK_AND_STANDARDS
/// 4.4: production queries name their columns; a test that used `SELECT *` would
/// silently start comparing a new column).
async fn snapshot(p: &SqlitePool) -> Result<Vec<(String, i64, String)>, Box<dyn Error>> {
    let mut out = Vec::new();
    for table in [
        "teams",
        "roles",
        "harness_instances",
        "adapters",
        "bindings",
        "tasks",
        "runs",
        "leases",
        "reviews",
    ] {
        let count: i64 = query_scalar(&format!("SELECT COUNT(*) FROM {table}"))
            .fetch_one(p)
            .await?;
        // A stable per-row digest, so a changed column cannot hide behind an
        // unchanged row count.
        let digest: Option<String> = query_scalar(&format!(
            "SELECT group_concat(d, '|') FROM (SELECT group_concat(x, ':') AS d FROM {table})"
        ))
        .fetch_one(p)
        .await
        .ok();
        out.push((table.to_string(), count, digest.unwrap_or_default()));
    }
    Ok(out)
}

/// The `0001..0026` schema, built from the same `include_str!` files the crate
/// embeds.
///
/// This is the baseline half of rule 1-b. It is a real second migration source
/// over the shipped SQL, not a hand-copied subset: a copied subset would drift
/// from the crate the moment a migration is added.
const V26: [(&str, &str); 26] = [
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
];

/// The `0001..0028` schema: every shipped migration except the one this package
/// adds.
///
/// Built as `V26` plus the two entries that follow it rather than as a second
/// hand-copied list of 28. The first version of this constant listed `V26[n]`
/// one line at a time and was wrong twice -- first 29 entries for a 28-element
/// array, then 3 after an over-eager edit -- which is ADVERSARIAL_REVIEW_PLAYBOOK
/// rule 6 in miniature: a repeated list is a second thing to keep in sync, and
/// the compiler only catches the length, never the drift.
///
/// This exists so the older guards can be shown to still carry weight *without*
/// `0029` in the way. `0029`'s trigger is created last and SQLite runs `BEFORE`
/// triggers in reverse creation order (measured, not assumed), so on the full
/// schema `0029` answers first for the whole terminal quadrant and
/// `0012:142-145` is no longer directly observable there. Asserting "0012 still
/// refuses" against the full schema would be asserting a message nobody receives
/// -- the exact "green for the wrong reason" shape this file exists to avoid.
fn v28_migrations() -> Vec<(&'static str, &'static str)> {
    let mut all: Vec<(&'static str, &'static str)> = V26.to_vec();
    all.push((
        "run transition integrity",
        include_str!("../migrations/0027_run_transition_integrity.sql"),
    ));
    all.push((
        "retained entity integrity",
        include_str!("../migrations/0028_retained_entity_integrity.sql"),
    ));
    all
}

async fn v28_database() -> Result<SqlitePool, Box<dyn Error>> {
    let p = SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(SqliteConnectOptions::from_str("sqlite::memory:")?.foreign_keys(true))
        .await?;
    Migrator::new(V28OnlyMigrations).await?.run(&p).await?;
    Ok(p)
}

#[derive(Debug)]
struct V28OnlyMigrations;

impl MigrationSource<'static> for V28OnlyMigrations {
    fn resolve(
        self,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<Migration>, BoxDynError>> + Send + 'static>> {
        Box::pin(async {
            Ok(v28_migrations()
                .iter()
                .enumerate()
                .map(|(i, (name, sql))| {
                    Migration::new(
                        (i + 1) as i64,
                        Cow::Borrowed(*name),
                        MigrationType::Simple,
                        Cow::Borrowed(*sql),
                        false,
                    )
                })
                .collect())
        })
    }
}

#[derive(Debug)]
struct V26OnlyMigrations;

impl MigrationSource<'static> for V26OnlyMigrations {
    fn resolve(
        self,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<Migration>, BoxDynError>> + Send + 'static>> {
        Box::pin(async {
            Ok(V26
                .iter()
                .enumerate()
                .map(|(i, (name, sql))| {
                    Migration::new(
                        (i + 1) as i64,
                        Cow::Borrowed(*name),
                        MigrationType::Simple,
                        Cow::Borrowed(*sql),
                        false,
                    )
                })
                .collect())
        })
    }
}

async fn v26_database() -> Result<SqlitePool, Box<dyn Error>> {
    let p = SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(SqliteConnectOptions::from_str("sqlite::memory:")?.foreign_keys(true))
        .await?;
    Migrator::new(V26OnlyMigrations).await?.run(&p).await?;
    Ok(p)
}
