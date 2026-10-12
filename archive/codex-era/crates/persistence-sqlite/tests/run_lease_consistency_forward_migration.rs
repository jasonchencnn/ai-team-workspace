use persistence_sqlite::migrate;
use sqlx_core::{
    error::BoxDynError,
    migrate::{Migration, MigrationSource, MigrationType, Migrator},
    query::query,
    query_scalar::query_scalar,
};
use sqlx_sqlite::{SqliteConnectOptions, SqlitePool, SqlitePoolOptions};
use std::{borrow::Cow, error::Error, future::Future, pin::Pin, str::FromStr};
use teamspace_domain::run_fencing::LEASE_BEARING_STATUSES;
use teamspace_domain::run_state_machine::RunStatus;
async fn db() -> Result<SqlitePool, Box<dyn Error>> {
    let p = SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(SqliteConnectOptions::from_str("sqlite::memory:")?.foreign_keys(true))
        .await?;
    migrate(&p).await?;
    Ok(p)
}
/// Build a `runs` row with a correct `started_at`/`completed_at` pair.
///
/// `runs` CHECKs force `started_at` NULL for `QUEUED`/`STARTING`, non-NULL for
/// `RUNNING` and the four terminal statuses, and `completed_at` non-NULL only
/// for terminal. A matrix over statuses is therefore a matrix over CHECKs as
/// well — get this wrong and the row is refused by the table, not by the guard
/// under test, and the test passes for the wrong reason.
///
/// Column order: id, team_id, task_id, role_id, binding_id, harness_instance_id,
/// attempt, fencing_token, status, exit_code, failure_reason, version,
/// created_at, started_at, completed_at.
///
/// The argument list mirrors that order on purpose, and mirrors `graph()`'s, so
/// a reader can check a generated statement against the schema by eye. Bundling
/// the ids into a struct would shorten the signature and hide exactly that.
#[allow(clippy::too_many_arguments)]
fn run_row(
    run: &str,
    team: &str,
    task: &str,
    role: &str,
    binding: &str,
    harness: &str,
    attempt: i64,
    fencing_token: i64,
    status: &str,
) -> String {
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
        "INSERT INTO runs VALUES(X'{run}',X'{team}',X'{task}',X'{role}',X'{binding}',\
         X'{harness}',{attempt},{fencing_token},'{status}',0,NULL,1,1,{started},{done})"
    )
}

/// Assert that a statement is refused **by a consistency guard**, not by a
/// CHECK, a UNIQUE index or a foreign key.
///
/// The tenth review's point about the older assertions was right: a bare
/// `is_err()` cannot tell "the guard I am testing refused this" from "some
/// unrelated constraint happened to fire first", and eight of the holes it
/// found were exactly that kind of test. Every refusal in this file now goes
/// through here, with the message it is allowed to come from.
///
/// The allow-list is per call site, not global: `0012` uses four distinct
/// strings, and a test that only ever says "is_err" is a test that will stay
/// green after the guard it names has been removed.
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

async fn graph(
    p: &SqlitePool,
    team: &str,
    role: &str,
    harness: &str,
    adapter: &str,
    binding: &str,
    task: &str,
) -> Result<(), Box<dyn Error>> {
    for s in [
        format!("INSERT INTO teams VALUES(X'{team}','t','ACTIVE',1,1,1,NULL)"),
        format!("INSERT INTO roles VALUES(X'{role}',X'{team}','r{role}','r','ACTIVE',1,1,1,NULL)"),
        format!(
            "INSERT INTO harness_instances VALUES(X'{harness}',X'{team}','h','h','ACTIVE','[]',NULL,1,1,1,NULL)"
        ),
        format!("INSERT INTO adapters VALUES(X'{adapter}',X'{team}','a','1','/a','{{}}',1,1,1)"),
        format!(
            "INSERT INTO bindings VALUES(X'{binding}',X'{team}',X'{role}',X'{harness}',X'{adapter}','ACTIVE',1,1,'{{}}',1,1,1,NULL)"
        ),
        format!(
            "INSERT INTO tasks VALUES(X'{task}',X'{team}',X'{role}','d{task}','t','','DOING',1,1,1,NULL,NULL)"
        ),
    ] {
        query(&s).execute(p).await?;
    }
    Ok(())
}

#[tokio::test]
async fn run_role_must_match_the_task_owner_role() -> Result<(), Box<dyn Error>> {
    let p = db().await?;
    let (team, owner, harness, adapter, binding, task) = (
        "01921389d31174628f80cb5f79aa9201",
        "01921389d31174628f80cb5f79aa9202",
        "01921389d31174628f80cb5f79aa9203",
        "01921389d31174628f80cb5f79aa9204",
        "01921389d31174628f80cb5f79aa9205",
        "01921389d31174628f80cb5f79aa9206",
    );
    graph(&p, team, owner, harness, adapter, binding, task).await?;
    let (other, other_harness, other_adapter, other_binding) = (
        "01921389d31174628f80cb5f79aa9207",
        "01921389d31174628f80cb5f79aa9208",
        "01921389d31174628f80cb5f79aa9209",
        "01921389d31174628f80cb5f79aa9210",
    );
    query(&format!(
        "INSERT INTO roles VALUES(X'{other}',X'{team}','other','Other','ACTIVE',1,1,1,NULL)"
    ))
    .execute(&p)
    .await?;
    query(&format!("INSERT INTO harness_instances VALUES(X'{other_harness}',X'{team}','h','h','ACTIVE','[]',NULL,1,1,1,NULL)")).execute(&p).await?;
    query(&format!(
        "INSERT INTO adapters VALUES(X'{other_adapter}',X'{team}','a','1','/a','{{}}',1,1,1)"
    ))
    .execute(&p)
    .await?;
    query(&format!("INSERT INTO bindings VALUES(X'{other_binding}',X'{team}',X'{other}',X'{other_harness}',X'{other_adapter}','ACTIVE',1,1,'{{}}',1,1,1,NULL)")).execute(&p).await?;
    let bad = format!(
        "INSERT INTO runs VALUES(X'01921389d31174628f80cb5f79aa9211',X'{team}',X'{task}',X'{other}',X'{other_binding}',X'{other_harness}',1,1,'RUNNING',NULL,NULL,1,1,1,NULL)"
    );
    assert_refused(
        query(&bad).execute(&p).await,
        &["Run consistency violation"],
        "a Run whose role is not the Task owner's",
    );
    Ok(())
}
#[tokio::test]
async fn run_lease_and_review_tuple_mismatches_are_rejected_without_rows()
-> Result<(), Box<dyn Error>> {
    let p = db().await?;
    let (ta, ra, ha, aa, ba, ka) = (
        "01921389d31174628f80cb5f79aa9001",
        "01921389d31174628f80cb5f79aa9002",
        "01921389d31174628f80cb5f79aa9003",
        "01921389d31174628f80cb5f79aa9004",
        "01921389d31174628f80cb5f79aa9005",
        "01921389d31174628f80cb5f79aa9006",
    );
    let (tb, rb, hb, ab, bb, kb) = (
        "01921389d31174628f80cb5f79aa9011",
        "01921389d31174628f80cb5f79aa9012",
        "01921389d31174628f80cb5f79aa9013",
        "01921389d31174628f80cb5f79aa9014",
        "01921389d31174628f80cb5f79aa9015",
        "01921389d31174628f80cb5f79aa9016",
    );
    graph(&p, ta, ra, ha, aa, ba, ka).await?;
    graph(&p, tb, rb, hb, ab, bb, kb).await?;
    let bad = format!(
        "INSERT INTO runs VALUES(X'01921389d31174628f80cb5f79aa9021',X'{ta}',X'{ka}',X'{rb}',X'{bb}',X'{hb}',1,1,'RUNNING',NULL,NULL,1,1,1,NULL)"
    );
    assert_refused(
        query(&bad).execute(&p).await,
        &["Run consistency violation"],
        "a Run borrowing another Task's role, binding and harness",
    );
    assert_eq!(
        query_scalar::<_, i64>("SELECT COUNT(*) FROM runs")
            .fetch_one(&p)
            .await?,
        0
    );
    Ok(())
}

#[tokio::test]
async fn lease_and_review_insert_update_mismatches_leave_rows_unchanged()
-> Result<(), Box<dyn Error>> {
    let p = db().await?;
    let (ta, ra, ha, aa, ba, ka) = (
        "01921389d31174628f80cb5f79aa9101",
        "01921389d31174628f80cb5f79aa9102",
        "01921389d31174628f80cb5f79aa9103",
        "01921389d31174628f80cb5f79aa9104",
        "01921389d31174628f80cb5f79aa9105",
        "01921389d31174628f80cb5f79aa9106",
    );
    let (tb, rb, hb, ab, bb, kb) = (
        "01921389d31174628f80cb5f79aa9111",
        "01921389d31174628f80cb5f79aa9112",
        "01921389d31174628f80cb5f79aa9113",
        "01921389d31174628f80cb5f79aa9114",
        "01921389d31174628f80cb5f79aa9115",
        "01921389d31174628f80cb5f79aa9116",
    );
    graph(&p, ta, ra, ha, aa, ba, ka).await?;
    graph(&p, tb, rb, hb, ab, bb, kb).await?;
    query(&format!("INSERT INTO runs VALUES(X'01921389d31174628f80cb5f79aa9121',X'{ta}',X'{ka}',X'{ra}',X'{ba}',X'{ha}',1,1,'RUNNING',NULL,NULL,1,1,1,NULL)")).execute(&p).await?;
    query(&format!(
        "INSERT INTO leases VALUES(X'{ka}',X'01921389d31174628f80cb5f79aa9121',1,1,2,1)"
    ))
    .execute(&p)
    .await?;
    for statement in [
        format!("UPDATE leases SET task_id=X'{kb}'"),
        "UPDATE leases SET fencing_token=2".to_string(),
    ] {
        assert_refused(
            query(&statement).execute(&p).await,
            &["Lease consistency violation"],
            &statement,
        );
    }
    assert_eq!(
        query_scalar::<_, i64>("SELECT COUNT(*) FROM leases")
            .fetch_one(&p)
            .await?,
        1
    );
    let bad_review = format!(
        "INSERT INTO reviews VALUES(X'01921389d31174628f80cb5f79aa9122',X'{ta}',X'{kb}',X'01921389d31174628f80cb5f79aa9121',X'{rb}','APPROVED',NULL,1,1)"
    );
    assert_refused(
        query(&bad_review).execute(&p).await,
        &["Review consistency violation"],
        "a Review whose run_id and task_id name different rows",
    );
    assert_eq!(
        query_scalar::<_, i64>("SELECT COUNT(*) FROM reviews")
            .fetch_one(&p)
            .await?,
        0
    );
    Ok(())
}

#[tokio::test]
async fn new_run_attempt_and_fencing_token_must_exceed_all_task_history()
-> Result<(), Box<dyn Error>> {
    let p = db().await?;
    let (team, role, harness, adapter, binding, task) = (
        "01921389d31174628f80cb5f79aa9301",
        "01921389d31174628f80cb5f79aa9302",
        "01921389d31174628f80cb5f79aa9303",
        "01921389d31174628f80cb5f79aa9304",
        "01921389d31174628f80cb5f79aa9305",
        "01921389d31174628f80cb5f79aa9306",
    );
    graph(&p, team, role, harness, adapter, binding, task).await?;
    query(&format!(
        "INSERT INTO runs VALUES(X'01921389d31174628f80cb5f79aa9307',X'{team}',X'{task}',X'{role}',X'{binding}',X'{harness}',1,4,'SUCCEEDED',0,NULL,1,1,1,2)"
    ))
    .execute(&p)
    .await?;

    for statement in [
        format!(
            "INSERT INTO runs VALUES(X'01921389d31174628f80cb5f79aa9308',X'{team}',X'{task}',X'{role}',X'{binding}',X'{harness}',3,5,'QUEUED',NULL,NULL,1,2,NULL,NULL)"
        ),
        format!(
            "INSERT INTO runs VALUES(X'01921389d31174628f80cb5f79aa9309',X'{team}',X'{task}',X'{role}',X'{binding}',X'{harness}',2,3,'QUEUED',NULL,NULL,1,2,NULL,NULL)"
        ),
    ] {
        assert!(
            query(&statement).execute(&p).await.is_err(),
            "historical generation order accepted: {statement}"
        );
    }
    assert_eq!(
        query_scalar::<_, i64>("SELECT COUNT(*) FROM runs")
            .fetch_one(&p)
            .await?,
        1
    );
    Ok(())
}

#[tokio::test]
async fn terminal_run_requires_its_lease_to_be_released_and_cannot_receive_one()
-> Result<(), Box<dyn Error>> {
    let p = db().await?;
    let (team, role, harness, adapter, binding, task) = (
        "01921389d31174628f80cb5f79aa9401",
        "01921389d31174628f80cb5f79aa9402",
        "01921389d31174628f80cb5f79aa9403",
        "01921389d31174628f80cb5f79aa9404",
        "01921389d31174628f80cb5f79aa9405",
        "01921389d31174628f80cb5f79aa9406",
    );
    graph(&p, team, role, harness, adapter, binding, task).await?;
    query(&format!(
        "INSERT INTO runs VALUES(X'01921389d31174628f80cb5f79aa9407',X'{team}',X'{task}',X'{role}',X'{binding}',X'{harness}',1,1,'SUCCEEDED',0,NULL,1,1,1,2)"
    ))
    .execute(&p)
    .await?;
    assert_refused(
        query(&format!(
            "INSERT INTO leases VALUES(X'{task}',X'01921389d31174628f80cb5f79aa9407',1,1,2,1)"
        ))
        .execute(&p)
        .await,
        &["Lease consistency violation"],
        "a lease on a SUCCEEDED Run",
    );
    query(&format!("DELETE FROM leases WHERE task_id = X'{task}'"))
        .execute(&p)
        .await?;

    query(&format!(
        "INSERT INTO runs VALUES(X'01921389d31174628f80cb5f79aa9408',X'{team}',X'{task}',X'{role}',X'{binding}',X'{harness}',2,2,'RUNNING',NULL,NULL,1,2,2,NULL)"
    ))
    .execute(&p)
    .await?;
    query(&format!(
        "INSERT INTO leases VALUES(X'{task}',X'01921389d31174628f80cb5f79aa9408',2,2,3,1)"
    ))
    .execute(&p)
    .await?;
    assert_refused(
        query("UPDATE runs SET status = 'SUCCEEDED', completed_at = 3 WHERE attempt = 2")
            .execute(&p)
            .await,
        &["Run consistency violation"],
        "finishing a Run that still holds a lease",
    );
    assert_eq!(
        query_scalar::<_, String>("SELECT status FROM runs WHERE attempt = 2")
            .fetch_one(&p)
            .await?,
        "RUNNING"
    );
    Ok(())
}

#[tokio::test]
async fn review_must_reference_a_succeeded_run() -> Result<(), Box<dyn Error>> {
    let p = db().await?;
    let (team, owner, harness, adapter, binding, task, reviewer) = (
        "01921389d31174628f80cb5f79aa9501",
        "01921389d31174628f80cb5f79aa9502",
        "01921389d31174628f80cb5f79aa9503",
        "01921389d31174628f80cb5f79aa9504",
        "01921389d31174628f80cb5f79aa9505",
        "01921389d31174628f80cb5f79aa9506",
        "01921389d31174628f80cb5f79aa9507",
    );
    graph(&p, team, owner, harness, adapter, binding, task).await?;
    query(&format!(
        "INSERT INTO roles VALUES(X'{reviewer}',X'{team}','reviewer','Reviewer','ACTIVE',1,1,1,NULL)"
    ))
    .execute(&p)
    .await?;
    query(&format!(
        "INSERT INTO runs VALUES(X'01921389d31174628f80cb5f79aa9508',X'{team}',X'{task}',X'{owner}',X'{binding}',X'{harness}',1,1,'FAILED',1,NULL,1,1,1,2)"
    ))
    .execute(&p)
    .await?;
    assert_refused(
        query(&format!(
            "INSERT INTO reviews VALUES(X'01921389d31174628f80cb5f79aa9509',X'{team}',X'{task}',X'01921389d31174628f80cb5f79aa9508',X'{reviewer}','APPROVED',NULL,1,3)"
        ))
        .execute(&p)
        .await,
        &["Review consistency violation"],
        "a Review of a FAILED Run",
    );
    assert_eq!(
        query_scalar::<_, i64>("SELECT COUNT(*) FROM reviews")
            .fetch_one(&p)
            .await?,
        0
    );
    Ok(())
}

/// Every guard `0012` installs must actually be installed, on the table it
/// belongs to.
///
/// A text check reading `0012` cannot see this: appending
/// `DROP TRIGGER leases_consistency_insert;` at the end of the migration leaves
/// every one of those four status literals in the file, and the domain suite
/// still reports 16/16 green — while the database no longer refuses a terminal
/// Run's lease. `sqlite_master` is the only place where "the trigger is
/// installed" is an answerable question.
///
/// All eleven are named, not counted, and each is named **together with the
/// table it fires on**. Name alone answers "does a trigger by this name
/// exist"; a trigger of the same name re-hung on `runs` instead of `leases`
/// still answers yes. The behaviour suite catches that too, but this is the
/// check whose stated claim is "installed", so it should be the one that
/// constrains both halves.
///
/// The list is deliberately literal — if `0012` later gains a guard, it is
/// absent here, and that is a gap to close by adding a name, not a reason to
/// weaken this one.
#[tokio::test]
async fn every_0012_guard_is_installed_in_the_live_schema() -> Result<(), Box<dyn Error>> {
    let p = db().await?;
    for (trigger, table) in [
        ("runs_consistency_insert", "runs"),
        ("runs_consistency_update", "runs"),
        ("leases_consistency_insert", "leases"),
        ("leases_consistency_update", "leases"),
        ("reviews_consistency_insert", "reviews"),
        ("reviews_consistency_update", "reviews"),
        ("tasks_run_consistency_update", "tasks"),
        ("bindings_run_consistency_update", "bindings"),
        ("roles_run_review_consistency_update", "roles"),
        (
            "harness_instances_run_consistency_update",
            "harness_instances",
        ),
        ("runs_lease_review_consistency_update", "runs"),
    ] {
        let count: i64 = query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM sqlite_master \
             WHERE type = 'trigger' AND name = ? AND tbl_name = ?",
        )
        .bind(trigger)
        .bind(table)
        .fetch_one(&p)
        .await?;
        assert_eq!(
            count, 1,
            "{trigger} is not installed on {table}, so whatever it refused is \
             now allowed while 0012 still shows the text that was supposed to \
             refuse it"
        );
    }
    Ok(())
}

/// The insert guard must accept exactly the four non-terminal statuses.
///
/// Two gaps showed up here, and both are invisible to a suite that checks one
/// side of the rule:
///
/// * only refusal was checked, and a guard that refuses everything passes it —
///   rewriting the literals as `'Queued'` instead of `'QUEUED'` is exactly that,
///   and SQLite compares strings case-sensitively;
/// * only `SUCCEEDED` was tried as a terminal status, so *adding another*
///   terminal status to the allowed list went unnoticed.
///
/// So: all eight, both directions. `domain::run_fencing::LEASE_BEARING_STATUSES`
/// names the four that must be accepted; the rest must be refused.
#[tokio::test]
async fn the_insert_guard_admits_exactly_the_non_terminal_statuses() -> Result<(), Box<dyn Error>> {
    const PREFIX: &str = "01921389d31174628f80cb5f79ae";
    let id = |n: usize| -> String { format!("{PREFIX}{n:04x}") };
    // Derived, not written out. The tenth review found that this test used to
    // hard-code the same eight spellings the migration uses, while the
    // constant it claimed to be checking (`LEASE_BEARING_STATUSES`) appeared
    // nowhere in this file — two independent lists with nothing between them.
    // Swapping `RunStatus::Queued` for a second `RunStatus::Unknown` in the
    // domain left this suite green while the database kept enforcing the old
    // set. Now the two ends are joined: the admitted set comes from the domain
    // constant, and the universe comes from `RunStatus::ALL`, so adding a ninth
    // status to the domain makes this test try it against a database that does
    // not know it — and the test says so by name.
    let all: Vec<String> = RunStatus::ALL
        .iter()
        .map(|s| s.as_str().to_string())
        .collect();
    let admitted: Vec<String> = LEASE_BEARING_STATUSES
        .iter()
        .map(|s| s.as_str().to_string())
        .collect();
    for status in &admitted {
        assert!(
            all.contains(status),
            "LEASE_BEARING_STATUSES names {status}, which is not one of RunStatus::ALL"
        );
    }
    let cases: Vec<(String, bool)> = all
        .iter()
        .map(|s| {
            let terminal = RunStatus::ALL
                .iter()
                .find(|r| r.as_str() == s)
                .is_some_and(|r| r.is_terminal());
            (s.clone(), admitted.contains(s) && !terminal)
        })
        .collect();

    for (index, (status, must_be_admitted)) in cases.into_iter().enumerate() {
        let p = db().await?;
        let base = 0x3000 + index * 0x0100;
        let (team, role, harness, adapter, binding, task) = (
            id(base),
            id(base + 1),
            id(base + 2),
            id(base + 3),
            id(base + 4),
            id(base + 5),
        );
        let run = id(base + 6);
        graph(&p, &team, &role, &harness, &adapter, &binding, &task).await?;
        query(&run_row(
            &run, &team, &task, &role, &binding, &harness, 1, 1, &status,
        ))
        .execute(&p)
        .await?;
        let acquired = 100_000 + index;
        let accepted = query(&format!(
            "INSERT INTO leases VALUES(X'{task}',X'{run}',1,{acquired},{},1)",
            acquired + 1
        ))
        .execute(&p)
        .await;

        if !must_be_admitted {
            assert_refused(
                accepted,
                &["Lease consistency violation"],
                &format!(
                    "a lease on a {status} Run. `LEASE_BEARING_STATUSES` excludes it, so \
                     the constant and the database now disagree."
                ),
            );
        } else {
            assert!(
                accepted.is_ok(),
                "a lease on a {status} Run was refused. The guard is over-restrictive: \
                 `LEASE_BEARING_STATUSES` says this status may hold a lease."
            );
        }
    }
    Ok(())
}

/// The update guard must refuse to move a live lease onto a terminal Run.
///
/// This is the half `the_insert_guard_admits_exactly_the_non_terminal_statuses`
/// cannot reach, and it is where the one-line bypass lives: adding a fifth
/// status to the update trigger's `IN (...)` list is invisible to any suite
/// that only inserts leases. The two triggers share a rule but not a code path
/// — one is not a weaker copy of the other.
///
/// Two facts shape the test:
///
/// * re-pointing onto an *active* Run is unconstructible, because
///   `runs_single_active_run_per_task` (0004) allows only one. So the positive
///   half of this pair is a renewal that keeps `run_id`, not a re-point.
/// * so the refusal is isolated by construction: the re-point names the target
///   Run's real `fencing_token` and its own `task_id`, which makes `id`,
///   `task_id` and `fencing_token` all match. The only sub-predicate left that
///   can fail is `status IN (...)`. A `NOT NULL`/FK/`UNIQUE (run_id)` refusal
///   would then have to be reported as one, and the message check rejects it.
#[tokio::test]
async fn a_lease_cannot_be_repointed_onto_a_terminal_run() -> Result<(), Box<dyn Error>> {
    const PREFIX: &str = "01921389d31174628f80cb5f79ae";
    let id = |n: usize| -> String { format!("{PREFIX}{n:04x}") };
    for (index, status) in ["SUCCEEDED", "FAILED", "TIMED_OUT", "CANCELLED"]
        .into_iter()
        .enumerate()
    {
        let p = db().await?;
        let base = 0x4000 + index * 0x0100;
        let (team, role, harness, adapter, binding, task) = (
            id(base),
            id(base + 1),
            id(base + 2),
            id(base + 3),
            id(base + 4),
            id(base + 5),
        );
        let (holder, terminal) = (id(base + 6), id(base + 7));
        graph(&p, &team, &role, &harness, &adapter, &binding, &task).await?;
        query(&run_row(
            &holder, &team, &task, &role, &binding, &harness, 1, 1, "RUNNING",
        ))
        .execute(&p)
        .await?;
        query(&format!(
            "INSERT INTO leases VALUES(X'{task}',X'{holder}',1,100,200,1)"
        ))
        .execute(&p)
        .await?;

        // Control: this very row, renewed in place, is accepted. If it were not,
        // the refusal below would say nothing about the target Run's status.
        query(&format!(
            "UPDATE leases SET expires_at = 210 WHERE task_id = X'{task}'"
        ))
        .execute(&p)
        .await?;

        // A terminal Run on the same Task, no lease of its own, higher attempt
        // and higher token — so every guard except the status one is satisfied.
        query(&run_row(
            &terminal, &team, &task, &role, &binding, &harness, 2, 2, status,
        ))
        .execute(&p)
        .await?;
        let moved = query(&format!(
            "UPDATE leases SET run_id = X'{terminal}', fencing_token = 2, version = 2, \
             expires_at = 210 WHERE task_id = X'{task}'"
        ))
        .execute(&p)
        .await;
        let message = match &moved {
            Ok(_) => panic!("a live lease was re-pointed onto a {status} Run"),
            Err(e) => format!("{e}"),
        };
        assert!(
            message.contains("Lease consistency violation"),
            "the re-point onto a {status} Run failed for '{message}' rather than at \
             `status IN (...)`. Run {terminal} has task_id {task} and fencing_token 2, \
             so the status predicate is the only clause left that can refuse — if this \
             message changes, the test is no longer measuring what it claims to. \
             The string is raised by 0012:175 (insert) and 0012:189 (update) and \
             nothing else in the 22 migrations, so it pins both lease guards together \
             — it cannot tell the two apart. Here it does not need to: the row already \
             exists, so the insert guard cannot be the one that fired."
        );
        // And the refused statement left the lease where it was.
        assert_eq!(
            query_scalar::<_, String>("SELECT hex(run_id) FROM leases")
                .fetch_one(&p)
                .await?
                .to_lowercase(),
            holder,
            "the rejected UPDATE still moved the lease"
        );
    }
    Ok(())
}

/// The update guard must keep admitting a lease whose Run is still active.
///
/// Without this, a guard that refuses every update passes
/// `a_lease_cannot_be_repointed_onto_a_terminal_run` outright: lowercasing the
/// four literals in `leases_consistency_update` turns that suite green while
/// heartbeats and lease renewal stop working. Admission has to be asserted on
/// the same path as refusal, or "it refuses" is all the guard is ever checked
/// for.
#[tokio::test]
async fn a_lease_stays_updatable_while_its_run_is_active() -> Result<(), Box<dyn Error>> {
    const PREFIX: &str = "01921389d31174628f80cb5f79ae";
    let id = |n: usize| -> String { format!("{PREFIX}{n:04x}") };
    for (index, status) in ["QUEUED", "STARTING", "RUNNING", "UNKNOWN"]
        .into_iter()
        .enumerate()
    {
        let p = db().await?;
        let base = 0x5000 + index * 0x0100;
        let (team, role, harness, adapter, binding, task) = (
            id(base),
            id(base + 1),
            id(base + 2),
            id(base + 3),
            id(base + 4),
            id(base + 5),
        );
        let run = id(base + 6);
        graph(&p, &team, &role, &harness, &adapter, &binding, &task).await?;
        query(&run_row(
            &run, &team, &task, &role, &binding, &harness, 1, 1, status,
        ))
        .execute(&p)
        .await?;
        query(&format!(
            "INSERT INTO leases VALUES(X'{task}',X'{run}',1,100,200,1)"
        ))
        .execute(&p)
        .await?;
        query(&format!(
            "UPDATE leases SET expires_at = 240, version = 2 WHERE task_id = X'{task}'"
        ))
        .execute(&p)
        .await
        .unwrap_or_else(|e| panic!("a lease on a {status} Run could not be renewed: {e}"));
        // Not "did not error" — the row must actually carry the new values, or a
        // silently ignored UPDATE would read as an accepted one.
        let expires_at: i64 = query_scalar("SELECT expires_at FROM leases")
            .fetch_one(&p)
            .await?;
        let version: i64 = query_scalar("SELECT version FROM leases")
            .fetch_one(&p)
            .await?;
        assert_eq!(
            (expires_at, version),
            (240, 2),
            "the renewal on a {status} Run reported success but changed nothing"
        );
    }
    Ok(())
}

/// A lease must name the Run's own Task and the Run's own generation.
///
/// The status half of `leases_consistency_insert` has a bidirectional matrix
/// (`the_insert_guard_admits_exactly_the_non_terminal_statuses`) and the update
/// guard has one too. The **identity** half had nothing at all.
///
/// That is not a coverage nicety. `0012:50-55` — the upgrade-time backfill —
/// treats `lease.task_id IS NOT run.task_id` and
/// `lease.fencing_token IS NOT run.fencing_token` as integrity violations for
/// historical rows. So the migration asserts the invariant for the past and
/// does not enforce it going forward: one guard says this is not allowed, the
/// other says anything goes. Deleting the two lines below leaves the domain
/// suite 16/16, this suite 10/10, and the whole `persistence-sqlite` package
/// 137/137 — while a `RUNNING` Run accepts a lease carrying a stale token, a
/// token from the future, or a Task that is not its own.
///
/// `id = NEW.run_id` is the third identity clause, and it is not exercised
/// either, but a missing Run is already refused by
/// `leases_run_id_foreign_key`, so removing it is not exploitable. Only the two
/// that are.
#[tokio::test]
async fn a_lease_must_name_its_own_task_and_generation() -> Result<(), Box<dyn Error>> {
    const PREFIX: &str = "01921389d31174628f80cb5f79ae";
    let id = |n: usize| -> String { format!("{PREFIX}{n:04x}") };

    // (description, lease task, lease token, must be refused)
    let cases = [
        ("the Task the Run does not belong to", true, 2usize, true),
        ("a stale generation", false, 1, true),
        ("a generation from the future", false, 3, true),
        ("the correct Task and generation", false, 2, false),
    ];

    for (index, (desc, foreign_task, token, refused)) in cases.into_iter().enumerate() {
        let p = db().await?;
        let base = 0x6000 + index * 0x0100;
        // Two Tasks on one team: the Run belongs to the first, and the second
        // exists so that naming it is not merely a dangling foreign key.
        let (team, role_a, role_b) = (id(base), id(base + 1), id(base + 2));
        let (harness, adapter, binding) = (id(base + 3), id(base + 4), id(base + 5));
        let (task_a, task_b, run_a) = (id(base + 6), id(base + 7), id(base + 8));
        graph(&p, &team, &role_a, &harness, &adapter, &binding, &task_a).await?;
        // The second Task needs its own owner Role but shares the team, the
        // harness and the binding — `graph()` would re-insert the team row.
        query(&format!(
            "INSERT INTO roles VALUES(X'{role_b}',X'{team}','r{role_b}','r','ACTIVE',1,1,1,NULL)"
        ))
        .execute(&p)
        .await?;
        query(&format!(
            "INSERT INTO tasks VALUES(X'{task_b}',X'{team}',X'{role_b}','d{task_b}','t','','DOING',1,1,1,NULL,NULL)"
        ))
        .execute(&p)
        .await?;
        // `0012:97-100` pins `attempt` to exactly `MAX(attempt) + 1`, so with no
        // history the first Run is attempt 1. Its token is free, and 2 leaves a
        // token on each side for a lease to disagree with.
        query(&run_row(
            &run_a, &team, &task_a, &role_a, &binding, &harness, 1, 2, "RUNNING",
        ))
        .execute(&p)
        .await?;
        let lease_task = if foreign_task { &task_b } else { &task_a };

        // Control, same database and same table: the well-formed lease is
        // accepted. The ninth review showed this block is *not* what stops a
        // blanket-refusal guard from passing — the fourth case below, which
        // demands acceptance, does that on its own. What the control buys is
        // narrower: it proves the other three databases are in a state where a
        // lease *can* be inserted, so their refusals are about the identity
        // value and not about the fixture. The docstring claim it replaced
        // ("without it a refusal could be anything at all") was too strong;
        // the reviewer's measurement was right and the claim is gone.
        let correct = query(&format!(
            "INSERT INTO leases VALUES(X'{task_a}',X'{run_a}',2,100,200,1)"
        ))
        .execute(&p)
        .await;
        assert!(
            correct.is_ok(),
            "{desc}: the well-formed lease was refused, so the guard is \
             over-restrictive and this case would pass for the wrong reason"
        );
        query(&format!("DELETE FROM leases WHERE task_id = X'{task_a}'"))
            .execute(&p)
            .await?;

        let outcome = query(&format!(
            "INSERT INTO leases VALUES(X'{lease_task}',X'{run_a}',{token},100,200,1)"
        ))
        .execute(&p)
        .await;
        if refused {
            let message = match &outcome {
                Ok(_) => panic!(
                    "a lease naming {desc} was accepted. Run {run_a} has task_id \
                     {task_a} and fencing_token 2; the guard admits neither \
                     {lease_task} nor token {token}."
                ),
                Err(e) => format!("{e}"),
            };
            assert!(
                message.contains("Lease consistency violation"),
                "naming {desc} failed for '{message}' rather than at the identity \
                 sub-clause. That string is raised by {}[2] and by nothing else in \
                 the 22 migrations, so if it changes this test is no longer \
                 measuring what it claims to.",
                "0012:175,0012:189"
            );
            assert_eq!(
                query_scalar::<_, i64>("SELECT COUNT(*) FROM leases")
                    .fetch_one(&p)
                    .await?,
                0,
                "the refused INSERT left a row behind"
            );
        } else {
            assert!(
                outcome.is_ok(),
                "the correct lease was refused: a guard that refuses the truth \
                 passes every case above, so this row is what makes them mean \
                 something"
            );
        }
    }
    Ok(())
}

/// A lease may not be rewritten to a generation the Run never had.
///
/// `leases_consistency_update` requires `fencing_token = NEW.fencing_token`.
/// Weakened to `>=`, the guard still refuses a token from the future — it reads
/// as "the Run's generation is at least the lease's" — and **a stale token sails
/// straight through**, which is the one thing fencing exists to stop. The whole
/// suite stayed green under that one-character change: every test that inserts
/// a stale token uses the INSERT guard, and no test updated a lease's token at
/// all.
///
/// A lease's token is written once, at acquisition. The guard is not asserting
/// that a caller is well-behaved; it is asserting that a mis-written or replayed
/// UPDATE cannot move the record out from under the fence.
#[tokio::test]
async fn a_lease_may_not_be_rewritten_to_a_generation_the_run_never_had()
-> Result<(), Box<dyn Error>> {
    const PREFIX: &str = "01921389d31174628f80cb5f79ae";
    let id = |n: usize| -> String { format!("{PREFIX}{n:04x}") };
    for (index, token) in [1i64, 3].into_iter().enumerate() {
        let p = db().await?;
        let base = 0x7000 + index * 0x0100;
        let (team, role, harness, adapter, binding, task) = (
            id(base),
            id(base + 1),
            id(base + 2),
            id(base + 3),
            id(base + 4),
            id(base + 5),
        );
        let run = id(base + 6);
        graph(&p, &team, &role, &harness, &adapter, &binding, &task).await?;
        query(&run_row(
            &run, &team, &task, &role, &binding, &harness, 1, 2, "RUNNING",
        ))
        .execute(&p)
        .await?;
        query(&format!(
            "INSERT INTO leases VALUES(X'{task}',X'{run}',2,100,200,1)"
        ))
        .execute(&p)
        .await?;
        // Control: this lease, renewed in place, is accepted. Without it, the
        // refusal below could be "updates are refused here" and nothing more.
        query(&format!(
            "UPDATE leases SET expires_at = 240 WHERE task_id = X'{task}'"
        ))
        .execute(&p)
        .await?;

        let moved = query(&format!(
            "UPDATE leases SET fencing_token = {token}, version = 2 WHERE task_id = X'{task}'"
        ))
        .execute(&p)
        .await;
        let message = match &moved {
            Ok(_) => panic!(
                "a lease on a Run at generation 2 was rewritten to generation {token} \
                 and stored"
            ),
            Err(e) => format!("{e}"),
        };
        assert!(
            message.contains("Lease consistency violation"),
            "the rewrite to generation {token} failed for '{message}' rather than at \
             `fencing_token = NEW.fencing_token`"
        );
        let stored: i64 = query_scalar("SELECT fencing_token FROM leases")
            .fetch_one(&p)
            .await?;
        assert_eq!(
            stored, 2,
            "the refused UPDATE still moved the lease's generation"
        );
    }
    Ok(())
}

/// A Run's attempt and generation are written once, at insertion.
///
/// `runs_single_active_run_per_task` (`0004:86`) is keyed on `attempt`, and
/// `0012:146-154` decides "is this the latest attempt" from the same column, so
/// a Run that can rewrite its own `attempt` can present itself as the newest
/// generation of its Task. Nothing in the suite noticed: no test updated those
/// two columns, and a `runs` UPDATE that touched neither was never attempted.
#[tokio::test]
async fn a_run_may_not_rewrite_its_own_attempt_or_generation() -> Result<(), Box<dyn Error>> {
    const PREFIX: &str = "01921389d31174628f80cb5f79ae";
    let id = |n: usize| -> String { format!("{PREFIX}{n:04x}") };
    for (index, (column, value)) in [("attempt", 7), ("fencing_token", 9)]
        .into_iter()
        .enumerate()
    {
        let p = db().await?;
        let base = 0x8000 + index * 0x0100;
        let (team, role, harness, adapter, binding, task) = (
            id(base),
            id(base + 1),
            id(base + 2),
            id(base + 3),
            id(base + 4),
            id(base + 5),
        );
        let run = id(base + 6);
        graph(&p, &team, &role, &harness, &adapter, &binding, &task).await?;
        query(&run_row(
            &run, &team, &task, &role, &binding, &harness, 1, 1, "RUNNING",
        ))
        .execute(&p)
        .await?;
        // Control: this Run, updated without touching its identity, is accepted.
        query("UPDATE runs SET version = 2 WHERE attempt = 1")
            .execute(&p)
            .await?;

        let moved = query(&format!(
            "UPDATE runs SET {column} = {value} WHERE attempt = 1"
        ))
        .execute(&p)
        .await;
        let message = match &moved {
            Ok(_) => panic!("a Run rewrote its own {column} to {value} and was accepted"),
            Err(e) => format!("{e}"),
        };
        assert!(
            message.contains("Run consistency violation"),
            "rewriting {column} to {value} failed for '{message}' rather than at the \
             immutability sub-clause"
        );
        let stored: i64 = query_scalar(&format!("SELECT {column} FROM runs"))
            .fetch_one(&p)
            .await?;
        assert_eq!(stored, 1, "the refused UPDATE still moved {column}");
    }
    Ok(())
}

/// A Run that has finished does not start again.
///
/// `0012:142-145` is the only thing in the schema standing between a `SUCCEEDED`
/// Run and a second execution of the same Task. §5's whole table rests on it:
/// a terminal Run is a fence, and a fence that can be lifted is not one. With
/// that block deleted, a finished Run walks straight back to `RUNNING` and the
/// suite reports 11/11 green.
#[tokio::test]
async fn a_terminal_run_cannot_be_brought_back_to_life() -> Result<(), Box<dyn Error>> {
    const PREFIX: &str = "01921389d31174628f80cb5f79ae";
    let id = |n: usize| -> String { format!("{PREFIX}{n:04x}") };
    for (index, status) in ["SUCCEEDED", "FAILED", "TIMED_OUT", "CANCELLED"]
        .into_iter()
        .enumerate()
    {
        let p = db().await?;
        let base = 0x9000 + index * 0x0100;
        let (team, role, harness, adapter, binding, task) = (
            id(base),
            id(base + 1),
            id(base + 2),
            id(base + 3),
            id(base + 4),
            id(base + 5),
        );
        let run = id(base + 6);
        graph(&p, &team, &role, &harness, &adapter, &binding, &task).await?;
        query(&run_row(
            &run, &team, &task, &role, &binding, &harness, 1, 1, status,
        ))
        .execute(&p)
        .await?;
        // Control: a terminal Run is not a frozen row. Touching `version` is
        // accepted, so the refusal below is about the status transition and not
        // about "you may not update a finished Run".
        query("UPDATE runs SET version = 2 WHERE attempt = 1")
            .execute(&p)
            .await?;

        // `started_at` is set and `completed_at` cleared so the row satisfies
        // the table CHECK for RUNNING. If that were the refusal, the control
        // above would not have passed — but the CHECKs are not the guard under
        // test, so the statement has to be well-formed or the test is vacuous.
        let revived = query(
            "UPDATE runs SET status = 'RUNNING', started_at = 1, completed_at = NULL \
             WHERE attempt = 1",
        )
        .execute(&p)
        .await;
        // FC-03 note. This used to pin `0012`'s exact string. `0029` now states
        // the same frozen rule ("a terminal Run has no outgoing edge", STATE_MACHINES
        // section 3) across the whole quadrant, and SQLite runs `BEFORE` triggers in
        // reverse creation order, so `0029` answers first and `0012`'s message is no
        // longer the one a caller sees here.
        //
        // What is asserted is therefore the stable domain reason plus the proof the
        // row did not move -- not which of two correct guards spoke. An error
        // message is not a frozen business edge, and preserving it by leaving an
        // illegal write open would trade a real defect for a cosmetic one.
        // `0012` is shown to still bear the load directly, on a database without
        // `0029`, in `the_pre_existing_terminal_guard_still_bears_weight_without_0029`.
        let message = match &revived {
            Ok(_) => panic!("a {status} Run was put back to RUNNING and the guard allowed it"),
            Err(e) => format!("{e}"),
        };
        assert!(
            message.contains("Run terminal status violation")
                || message.contains("Run consistency violation"),
            "reviving a {status} Run failed for '{message}', which is neither the \
             0029 terminal guard nor the 0012 consistency guard"
        );
        let still: String = query_scalar("SELECT status FROM runs")
            .fetch_one(&p)
            .await?;
        assert_eq!(still, status, "the refused UPDATE still revived the Run");
    }
    Ok(())
}

/// A review is filed against the Task its Run belongs to.
///
/// `reviews_consistency_insert` requires `task_id = NEW.task_id`, i.e. the
/// review's Task and its Run's Task must be the same row. Without it, a review
/// lands on Task B pointing at Task A's finished Run — and `0012:66` treats
/// exactly that mismatch as a violation when validating historical rows, so
/// once again the migration asserts the invariant for the past and leaves the
/// forward path open.
#[tokio::test]
async fn a_review_must_name_the_task_its_run_belongs_to() -> Result<(), Box<dyn Error>> {
    const PREFIX: &str = "01921389d31174628f80cb5f79ae";
    let id = |n: usize| -> String { format!("{PREFIX}{n:04x}") };
    let p = db().await?;
    let base = 0xa000;
    let (team, role, harness, adapter, binding, task_a) = (
        id(base),
        id(base + 1),
        id(base + 2),
        id(base + 3),
        id(base + 4),
        id(base + 5),
    );
    let (role_b, task_b, reviewer, run_a) =
        (id(base + 6), id(base + 7), id(base + 8), id(base + 9));
    graph(&p, &team, &role, &harness, &adapter, &binding, &task_a).await?;
    for (r, n) in [(&role_b, "r"), (&reviewer, "reviewer")] {
        query(&format!(
            "INSERT INTO roles VALUES(X'{r}',X'{team}','{n}{r}','r','ACTIVE',1,1,1,NULL)"
        ))
        .execute(&p)
        .await?;
    }
    // `graph()` already created task_a; only the decoy is missing. It shares
    // task_a's owner, which is irrelevant here — what matters is that it exists
    // and is not the Run's, so the only thing that can refuse is the binding.
    query(&format!(
        "INSERT INTO tasks VALUES(X'{task_b}',X'{team}',X'{role}','d{task_b}','t','','DOING',1,1,1,NULL,NULL)"
    ))
    .execute(&p)
    .await?;
    query(&run_row(
        &run_a,
        &team,
        &task_a,
        &role,
        &binding,
        &harness,
        1,
        1,
        "SUCCEEDED",
    ))
    .execute(&p)
    .await?;

    // Control: the well-formed review is accepted.
    query(&format!(
        "INSERT INTO reviews VALUES(X'{reviewer}',X'{team}',X'{task_a}',X'{run_a}',\
         X'{reviewer}','APPROVED',NULL,1,3)"
    ))
    .execute(&p)
    .await?;

    // Same Run, same reviewer, a Task that exists — and is not the Run's.
    let stray = id(base + 10);
    let mismatched = query(&format!(
        "INSERT INTO reviews VALUES(X'{stray}',X'{team}',X'{task_b}',X'{run_a}',\
         X'{reviewer}','APPROVED',NULL,1,3)"
    ))
    .execute(&p)
    .await;
    let message = match &mismatched {
        Ok(_) => panic!(
            "a review was filed on Task {task_b} against Run {run_a}, which belongs to \
             Task {task_a}. Both rows exist, so only the tuple guard can be refusing this."
        ),
        Err(e) => format!("{e}"),
    };
    assert!(
        message.contains("Review consistency violation"),
        "the mismatched review failed for '{message}' rather than at the Task binding"
    );
    assert_eq!(
        query_scalar::<_, i64>("SELECT COUNT(*) FROM reviews")
            .fetch_one(&p)
            .await?,
        1,
        "the refused INSERT left a row behind, or the control did not land"
    );
    Ok(())
}

/// The fixture `runs_consistency_insert`'s five EXISTS clauses are tested
/// against: one good team, a second good team, and inside the first team a
/// second role, a second harness, and the bindings that make each mismatch
/// expressible.
///
/// The second team exists so "wrong team" is not merely a dangling foreign key.
/// The second role and second harness exist so a binding can be wrong in
/// *exactly one* way — same team, same role, different harness — instead of
/// failing several clauses at once and proving nothing about any of them.
struct Teams {
    t1: String,
    t2: String,
    ad1: String,
    ad2: String,
    ra: String,
    ha: String,
    ba: String,
    ka: String,
    kd: String,
    ra2: String,
    ba2: String,
    h2: String,
    ba3: String,
    kc: String,
    rb: String,
    hb: String,
    bb: String,
    kb: String,
}

async fn two_teams(p: &SqlitePool, base: usize) -> Result<Teams, Box<dyn Error>> {
    let id = |n: usize| -> String { format!("01921389d31174628f80cb5f79ae{n:04x}") };
    let t = Teams {
        t1: id(base),
        t2: id(base + 30),
        ad1: id(base + 15),
        ad2: id(base + 31),
        ra: id(base + 1),
        ha: id(base + 2),
        ba: id(base + 3),
        ka: id(base + 4),
        kd: id(base + 5),
        ra2: id(base + 6),
        ba2: id(base + 7),
        h2: id(base + 8),
        ba3: id(base + 9),
        kc: id(base + 10),
        rb: id(base + 11),
        hb: id(base + 12),
        bb: id(base + 13),
        kb: id(base + 14),
    };
    for s in [
        format!(
            "INSERT INTO teams VALUES(X'{}','t1','ACTIVE',1,1,1,NULL)",
            t.t1
        ),
        format!(
            "INSERT INTO teams VALUES(X'{}','t2','ACTIVE',1,1,1,NULL)",
            t.t2
        ),
        format!(
            "INSERT INTO roles VALUES(X'{}',X'{}','ra','r','ACTIVE',1,1,1,NULL)",
            t.ra, t.t1
        ),
        format!(
            "INSERT INTO roles VALUES(X'{}',X'{}','ra2','r','ACTIVE',1,1,1,NULL)",
            t.ra2, t.t1
        ),
        format!(
            "INSERT INTO roles VALUES(X'{}',X'{}','rb','r','ACTIVE',1,1,1,NULL)",
            t.rb, t.t2
        ),
        format!(
            "INSERT INTO harness_instances VALUES(X'{}',X'{}','h','h','ACTIVE','[]',NULL,1,1,1,NULL)",
            t.ha, t.t1
        ),
        format!(
            "INSERT INTO harness_instances VALUES(X'{}',X'{}','h','h','ACTIVE','[]',NULL,1,1,1,NULL)",
            t.h2, t.t1
        ),
        format!(
            "INSERT INTO harness_instances VALUES(X'{}',X'{}','h','h','ACTIVE','[]',NULL,1,1,1,NULL)",
            t.hb, t.t2
        ),
        format!(
            "INSERT INTO adapters VALUES(X'{}',X'{}','a','1','/a','{{}}',1,1,1)",
            t.ad1, t.t1
        ),
        format!(
            "INSERT INTO adapters VALUES(X'{}',X'{}','a','1','/a','{{}}',1,1,1)",
            t.ad2, t.t2
        ),
    ] {
        query(&s).execute(p).await?;
    }
    // `0002` refuses two bindings for the same Role whose effective ranges
    // overlap, and the status CHECK makes a closed range exclusive to
    // `REVOKED` — so two live bindings cannot share a Role at all. `ba3` is
    // `UNIQUE (role_id, binding_generation)` adds a second reason: two live
    // bindings for one Role need two generations, but `0012` does not read the
    // generation either, so that is not what makes the fourth case work.
    // `ba3` is therefore `PENDING_CHECK` with a null range, which the overlap trigger
    // skips entirely (`WHEN NEW.effective_from IS NOT NULL`) and which
    // `0012` does not look at: its binding clause checks id, team, role and
    // harness, nothing else. That is exactly what the fourth case needs — a
    // binding for the right Role on the right Team, pointing at the wrong
    // Harness.
    for (b, team, role, harness, adapter, status, generation, from) in [
        (&t.ba, &t.t1, &t.ra, &t.ha, &t.ad1, "ACTIVE", 1, "1"),
        (&t.ba2, &t.t1, &t.ra2, &t.ha, &t.ad1, "ACTIVE", 1, "1"),
        (
            &t.ba3,
            &t.t1,
            &t.ra,
            &t.h2,
            &t.ad1,
            "PENDING_CHECK",
            2,
            "NULL",
        ),
        (&t.bb, &t.t2, &t.rb, &t.hb, &t.ad2, "ACTIVE", 1, "1"),
    ] {
        query(&format!(
            "INSERT INTO bindings VALUES(X'{b}',X'{team}',X'{role}',X'{harness}',X'{adapter}','{status}',{generation},1,'{{}}',1,1,{from},NULL)"
        ))
        .execute(p)
        .await?;
    }
    // `kb` is the only Task on the second Team — without it, "wrong Team" is
    // not expressible and the first case silently degenerates into the second.
    for (k, team, owner) in [
        (&t.ka, &t.t1, &t.ra),
        (&t.kd, &t.t1, &t.ra),
        (&t.kc, &t.t1, &t.ra2),
        (&t.kb, &t.t2, &t.rb),
    ] {
        query(&format!(
            "INSERT INTO tasks VALUES(X'{k}',X'{team}',X'{owner}','d{k}','t','','DOING',1,1,1,NULL,NULL)"
        ))
        .execute(p)
        .await?;
    }
    Ok(t)
}

/// Every tuple `runs_consistency_insert` requires, broken one way at a time.
///
/// Five EXISTS clauses, and before this test **not one of them had a test**.
/// The tenth review deleted `AND team_id = NEW.team_id` (`:79`),
/// `AND role_id = NEW.role_id` (`:90`) and `AND harness_instance_id =
/// NEW.harness_instance_id` (`:91`), and weakened `WHERE id = NEW.task_id`
/// (`:78`) to `IS NOT NULL`; all four left the 15-test suite *and* the whole
/// 142-test `persistence-sqlite` package green, while a Run that names another
/// Team's Task, or a Role and a Harness its Binding does not carry, was
/// accepted. `0012:18`, `:25` and `:26` treat exactly those three mismatches as
/// violations when validating historical rows.
///
/// Each case breaks **one** clause. That is the whole point: a fixture where
/// three clauses fail at once proves that *something* is refused, not that
/// *this* clause is the reason. The harness, role and Team rows exist so a
/// mismatch can be expressed without dragging a second clause down with it.
///
/// The one case that cannot be isolated is the Task's team, and it is worth
/// saying why rather than leaving it as an unexplained gap. `NEW.team_id`
/// appears in all four EXISTS clauses, so a cross-Team Run is refused by three
/// of them whatever the fourth does — and conversely, with the other three
/// intact, deleting the Task clause's `team_id` conjunct changes nothing that
/// any statement can reach: `id = NEW.task_id` still pins the Task, and that
/// Task's team is then the Run's team by the other three. **That conjunct is
/// redundant, not unguarded** — verified by running the deletion: the suite
/// stays green, and so does the pristine probe. The reviewer's B5 probe ("a
/// T2 Run can take a T1 Task") is, in fact, the `id = NEW.task_id` hole (B6),
/// which is reachable and which the second case here closes.
///
/// The case is kept anyway, because the invariant it states — a Run and its
/// Task share a Team — is worth having a test name for even when no single
/// conjunct is what enforces it.
#[tokio::test]
async fn a_run_must_name_a_task_role_and_harness_its_graph_agrees_on() -> Result<(), Box<dyn Error>>
{
    // Indices into `Teams` field order:
    //   0 t1 · 1 ra · 2 ha · 3 ba · 4 ka · 5 kd · 6 ra2 · 7 ba2 · 8 h2
    //   9 ba3 · 10 kc · 11 rb · 12 hb · 13 bb · 14 kb
    // Only the field under test differs from the control Run; everything else
    // is the well-formed tuple. That is the point: a case that gets three
    // columns wrong proves that *something* is refused, not that *this* clause
    // is the reason. The first version of this table got the Role, Binding and
    // Harness columns wrong, so every case was refused by an unrelated foreign
    // key and the suite stayed green through the exact mutations it was written
    // to catch — **the bypass matrix found that, not review**, which is the
    // argument for running the mutations on a test you just wrote.
    // (what is wrong, team, task, role, binding, harness)
    let cases: [(&str, usize, usize, usize, usize, usize); 4] = [
        ("the Run's Task belongs to another Team", 0, 14, 1, 3, 2),
        (
            "the Run's Task is not the one its Role owns",
            0,
            10,
            1,
            3,
            2,
        ),
        (
            "the Run's Binding belongs to a different Role",
            0,
            4,
            1,
            7,
            2,
        ),
        (
            "the Run's Binding points at a different Harness",
            0,
            4,
            1,
            9,
            2,
        ),
    ];
    for (index, (desc, ti, ki, ri, bi, hi)) in cases.into_iter().enumerate() {
        let p = db().await?;
        let base = 0xb000 + index * 0x0100;
        let t = two_teams(&p, base).await?;
        let pick = |n: usize, i: usize| -> &String {
            [
                &t.t1, &t.ra, &t.ha, &t.ba, &t.ka, &t.kd, &t.ra2, &t.ba2, &t.h2, &t.ba3, &t.kc,
                &t.rb, &t.hb, &t.bb, &t.kb,
            ][n + i]
        };
        let run = format!("01921389d31174628f80cb5f79ae{:04x}", base + 20);
        // Control: a fully correct Run, on its own Task so the partial unique
        // index does not collide. If this is refused, every case below would
        // pass for a reason that has nothing to do with the clause under test.
        let control = format!("01921389d31174628f80cb5f79ae{:04x}", base + 21);
        query(&run_row(&control, &t.t1, &t.kd, &t.ra, &t.ba, &t.ha, 1, 1, "RUNNING"))
            .execute(&p)
            .await
            .unwrap_or_else(|e| panic!("{desc}: the control Run was refused ({e}), so the fixture is not admitting Runs at all"));

        let bad = query(&run_row(
            &run,
            pick(ti, 0),
            pick(ki, 0),
            pick(ri, 0),
            pick(bi, 0),
            pick(hi, 0),
            1,
            1,
            "RUNNING",
        ))
        .execute(&p)
        .await;
        let message = match &bad {
            Ok(_) => panic!("{desc}: the Run was accepted"),
            Err(e) => format!("{e}"),
        };
        assert!(
            message.contains("Run consistency violation"),
            "{desc}: refused for '{message}' rather than at the tuple clauses"
        );
        assert_eq!(
            query_scalar::<_, i64>("SELECT COUNT(*) FROM runs")
                .fetch_one(&p)
                .await?,
            1,
            "{desc}: the refused INSERT left a row behind, or the control did not land"
        );
    }
    Ok(())
}

/// A Role or Harness that a Run points at cannot be moved to another Team.
///
/// Two guards, and before this test **neither had a test and neither had a row
/// in the bypass matrix**. The tenth review replaced each of them wholesale with
/// `SELECT RAISE(IGNORE);` — SQLite's only legal `RAISE` with no message — and
/// the 15-test suite *and* the whole 142-test `persistence-sqlite` package
/// stayed green while `UPDATE roles SET team_id = <other team>` and the same for
/// a harness were accepted.
///
/// `0012:25` and `0012:26` treat `role.team_id IS NOT run.team_id` and
/// `harness.team_id IS NOT run.team_id` as violations when validating historical
/// rows, so the migration asserts the invariant for the past and leaves the
/// forward path open — the same shape the eighth and ninth reviews found three
/// times each.
///
/// Both are covered here by behaviour, not by a name in `sqlite_master`:
/// a guard that exists under its own name and refuses nothing is installed.
#[tokio::test]
async fn a_role_or_harness_a_run_points_at_cannot_change_team() -> Result<(), Box<dyn Error>> {
    for (index, (table, target)) in [("roles", 1usize), ("harness_instances", 2)]
        .into_iter()
        .enumerate()
    {
        let p = db().await?;
        let base = 0xc000 + index * 0x0100;
        let t = two_teams(&p, base).await?;
        let run = format!("01921389d31174628f80cb5f79ae{:04x}", base + 20);
        query(&run_row(
            &run, &t.t1, &t.ka, &t.ra, &t.ba, &t.ha, 1, 1, "RUNNING",
        ))
        .execute(&p)
        .await?;
        let other = &t.t2;
        let self_id = if target == 1 { &t.ra } else { &t.ha };
        let moving = format!("UPDATE {table} SET team_id = X'{other}' WHERE id = X'{self_id}'");
        let moved = query(&moving).execute(&p).await;
        let message = match &moved {
            Ok(_) => panic!(
                "{table}.team_id was moved to another Team while a Run still points \
                 at it"
            ),
            Err(e) => format!("{e}"),
        };
        assert!(
            message.contains("Run consistency violation"),
            "moving {table}.team_id failed for '{message}' rather than at the \
             cross-Team guard"
        );
        let still: String = query_scalar::<_, String>(&format!(
            "SELECT hex(team_id) FROM {table} WHERE id = X'{self_id}'"
        ))
        .fetch_one(&p)
        .await?
        .to_lowercase();
        assert_eq!(still, t.t1, "the refused UPDATE still moved the row");
    }
    Ok(())
}

/// A Run that has been reviewed keeps its verdict.
///
/// `runs_lease_review_consistency_update` fires `BEFORE UPDATE OF team_id,
/// task_id, fencing_token, status` and requires, among other things, that a Run
/// carrying a Review is `SUCCEEDED`. Two different edits open the same hole and
/// the tenth review found both: deleting the `OR NEW.status IS NOT 'SUCCEEDED'`
/// disjunct, and dropping `status` from the `UPDATE OF` column list so the
/// trigger never fires for a status-only write. One behaviour test closes both —
/// what matters is that the statement is refused, not which clause refused it.
///
/// The statement keeps `version` and `completed_at` in step, because a Run that
/// reads as half-terminal would be refused by the table rather than by the
/// guard under test.
#[tokio::test]
async fn a_reviewed_run_keeps_its_verdict() -> Result<(), Box<dyn Error>> {
    const PREFIX: &str = "01921389d31174628f80cb5f79ae";
    let id = |n: usize| -> String { format!("{PREFIX}{n:04x}") };
    for (index, status) in ["CANCELLED", "FAILED", "TIMED_OUT", "RUNNING"]
        .into_iter()
        .enumerate()
    {
        let p = db().await?;
        let base = 0xd000 + index * 0x0100;
        let (team, role, harness, adapter, binding, task) = (
            id(base),
            id(base + 1),
            id(base + 2),
            id(base + 3),
            id(base + 4),
            id(base + 5),
        );
        let (run, reviewer, review) = (id(base + 6), id(base + 7), id(base + 8));
        graph(&p, &team, &role, &harness, &adapter, &binding, &task).await?;
        query(&format!(
            "INSERT INTO roles VALUES(X'{reviewer}',X'{team}','rv','r','ACTIVE',1,1,1,NULL)"
        ))
        .execute(&p)
        .await?;
        query(&run_row(
            &run,
            &team,
            &task,
            &role,
            &binding,
            &harness,
            1,
            1,
            "SUCCEEDED",
        ))
        .execute(&p)
        .await?;
        query(&format!(
            "INSERT INTO reviews VALUES(X'{review}',X'{team}',X'{task}',X'{run}',\
             X'{reviewer}','APPROVED',NULL,1,3)"
        ))
        .execute(&p)
        .await?;

        // Control: a reviewed Run is not a frozen row — moving a column the
        // guard does not watch is accepted, so the refusal below is about the
        // status and not about "you may not touch a reviewed Run".
        query("UPDATE runs SET failure_reason = 'x' WHERE attempt = 1")
            .execute(&p)
            .await?;

        let (started, done) = match status {
            "RUNNING" => ("1", "NULL"),
            _ => ("1", "3"),
        };
        let changed = query(&format!(
            "UPDATE runs SET status = '{status}', started_at = {started}, \
             completed_at = {done} WHERE attempt = 1"
        ))
        .execute(&p)
        .await;
        // FC-03 note: the same shadowing as above. Three of the four destinations
        // here are terminal, so `0029` now answers first; `RUNNING` is terminal ->
        // active, which `0029` also covers. The assertion names the domain reason
        // (a reviewed Run's verdict is a fact, and a terminal Run has no outgoing
        // edge) and the unchanged row, and the pre-0029 baseline below proves
        // `runs_lease_review_consistency_update` still refuses on its own.
        let message = match &changed {
            Ok(_) => panic!("a Run carrying an APPROVED review was moved to {status}"),
            Err(e) => format!("{e}"),
        };
        assert!(
            message.contains("Run terminal status violation")
                || message.contains("Run consistency violation"),
            "moving a reviewed Run to {status} failed for '{message}', which is \
             neither the 0029 terminal guard nor the 0012 reviewed-run rule"
        );
        let still: String = query_scalar("SELECT status FROM runs")
            .fetch_one(&p)
            .await?;
        assert_eq!(
            still, "SUCCEEDED",
            "the refused UPDATE still changed the status"
        );
    }
    Ok(())
}

/// A filed review cannot be moved off the tuple it was filed against.
///
/// `reviews_consistency_update` (`0012:213`, `RAISE` at `:231`) had **no test and
/// no row in the bypass matrix**. The eleventh review replaced the whole body
/// with `SELECT RAISE(IGNORE);` and the entire `persistence-sqlite` package
/// stayed green. The reason is worth stating, because it is the reason the row
/// was missing: the only thing that had ever mentioned this trigger by name was
/// an existence check against `sqlite_master`, and **a neutered guard is still
/// installed under its own name**. `RAISE(IGNORE)` is invisible to every check
/// that asks *whether* a guard is there.
///
/// Its sibling `reviews_consistency_insert` has a test, and it is easy to
/// assume the update path comes free with it. It does not: `0012` spells the
/// same three EXISTS clauses out twice, and a review can be filed correctly and
/// then walked sideways onto another Task, another Team, or another Run.
///
/// `0012:63-67` treats all five mismatches below as violations when validating
/// historical rows, so as everywhere else in this migration the invariant is
/// asserted for the past and left open on the forward path.
///
/// The control is load-bearing in the other direction. A suite made only of
/// refusals is passed by a guard that refuses *everything*, which is the mirror
/// of the tenth review's `RAISE(IGNORE)`: there, a guard that refuses nothing
/// went unnoticed; here, one that refuses everything would go unnoticed by the
/// same tests. So the first thing every case does is prove a `version` bump on
/// the very row it is about to move is accepted.
#[tokio::test]
async fn a_review_cannot_be_moved_off_the_tuple_it_was_filed_against() -> Result<(), Box<dyn Error>>
{
    const PREFIX: &str = "01921389d31174628f80cb5f79ae";
    let id = |n: usize| -> String { format!("{PREFIX}{n:04x}") };
    // (what is being done, the column it moves, which fixture row it is moved
    //  to). Pure strings, so the case table is written once and read as the
    //  enumeration of the guard's three EXISTS clauses.
    //
    //  `kd` is a second Task on the *same* Team, so moving `task_id` there
    //  breaks only the Run's `task_id = NEW.task_id` conjunct. `rb` is a role on
    //  the *other* Team, so moving `reviewer_role_id` there breaks only the
    //  reviewer's own `team_id` conjunct. The decoys exist so that no case is
    //  refused by two clauses at once and therefore proves nothing about
    //  either. The `team_id` case is the one that cannot be isolated: the
    //  review's Team appears in all three clauses, so moving it breaks three at
    //  once. It is kept anyway, and `0012` is one rule rather than three.
    let cases: [(&str, &str, Option<&str>); 6] = [
        (
            "control: a version bump on the same review",
            "version",
            None,
        ),
        (
            "the review's Task is moved to another Task of the same Team",
            "task_id",
            Some("kd"),
        ),
        (
            "the review's Team is moved to another Team",
            "team_id",
            Some("t2"),
        ),
        (
            "the review is pointed at a Run that never succeeded",
            "run_id",
            Some("run_running"),
        ),
        (
            "the review is pointed at a SUCCEEDED Run of another Task",
            "run_id",
            Some("run_other"),
        ),
        (
            "the review is pointed at a reviewer Role of another Team",
            "reviewer_role_id",
            Some("rb"),
        ),
    ];
    for (index, (desc, column, target_name)) in cases.into_iter().enumerate() {
        let p = db().await?;
        let base = 0xe000 + index * 0x0100;
        let t = two_teams(&p, base).await?;
        // The reviewer is a role of its own rather than a borrowed one, so that
        // "point the review at another Team's reviewer" is a statement about
        // this guard and not about whether the owner role may review its own
        // Task. `roles` is UNIQUE on (team_id, display_id), and `two_teams` has
        // already spent 'ra' and 'rb' on the other two.
        let reviewer = id(base + 41);
        query(&format!(
            "INSERT INTO roles VALUES(X'{reviewer}',X'{}','rv','Reviewer','ACTIVE',1,1,1,NULL)",
            t.t1
        ))
        .execute(&p)
        .await?;

        // Three Runs. `runs_consistency_insert` (`:96-102`) demands
        // `attempt = MAX(attempt) + 1` and a strictly rising `fencing_token` per
        // Task, which is why the two Runs on `ka` are attempts 1 and 2 and not
        // both attempt 1. Between them they separate the Run clause into its
        // two halves: a Run of the right Task that did not finish, and a Run
        // that finished but belongs to another Task.
        let (run_ok, run_running, run_other) = (id(base + 20), id(base + 21), id(base + 22));
        for (run, task, attempt, token, status) in [
            (&run_ok, &t.ka, 1, 1, "SUCCEEDED"),
            (&run_running, &t.ka, 2, 2, "RUNNING"),
            (&run_other, &t.kd, 1, 1, "SUCCEEDED"),
        ] {
            query(&run_row(
                run, &t.t1, task, &t.ra, &t.ba, &t.ha, attempt, token, status,
            ))
            .execute(&p)
            .await
            .unwrap_or_else(|e| panic!("{desc}: the fixture Run {status} was refused ({e})"));
        }
        let review = id(base + 9);
        query(&format!(
            "INSERT INTO reviews VALUES(X'{review}',X'{}',X'{}',X'{run_ok}',X'{reviewer}',\
             'APPROVED',NULL,1,3)",
            t.t1, t.ka
        ))
        .execute(&p)
        .await
        .unwrap_or_else(|e| {
            panic!("{desc}: the well-formed review was refused ({e}), so this case proves nothing")
        });

        // The four identity columns as one string: sqlx has no `FromRow` for
        // a four-tuple, and "did the refused UPDATE move anything" is a single
        // yes-or-no question anyway.
        const TUPLE: &str = "SELECT hex(team_id) || hex(task_id) || hex(run_id) \
                             || hex(reviewer_role_id) FROM reviews";
        let before: String = query_scalar(TUPLE).fetch_one(&p).await?;

        let sql = match target_name {
            None => format!("UPDATE reviews SET version = 2 WHERE id = X'{review}'"),
            Some(name) => {
                // The row a column is moved to, addressed by the name the case
                // table used. A name the fixture does not have is a bug in the
                // table, and is told apart from a refused statement here rather
                // than turning into a NULL that the database refuses.
                let hex = [
                    ("kd", &t.kd),
                    ("t2", &t.t2),
                    ("run_running", &run_running),
                    ("run_other", &run_other),
                    ("rb", &t.rb),
                ]
                .iter()
                .find(|(n, _)| *n == name)
                .unwrap_or_else(|| {
                    panic!("{desc}: the case names a row {name}, which the fixture does not have")
                })
                .1;
                format!("UPDATE reviews SET {column} = X'{hex}' WHERE id = X'{review}'")
            }
        };
        let outcome = query(&sql).execute(&p).await;

        if target_name.is_none() {
            outcome.unwrap_or_else(|e| {
                panic!(
                    "{desc}: refused for '{e}'. A filed review must be updatable, so \
                         a guard that refuses every write would pass every case below."
                )
            });
            let after: i64 = query_scalar("SELECT version FROM reviews")
                .fetch_one(&p)
                .await?;
            assert_eq!(after, 2, "{desc}: accepted but did not land");
            continue;
        }
        assert_refused(outcome, &["Review consistency violation"], desc);
        let after: String = query_scalar(TUPLE).fetch_one(&p).await?;
        assert_eq!(
            before, after,
            "{desc}: the review was refused and yet its tuple moved anyway"
        );
    }
    Ok(())
}

/// The `0001..0028` schema, built from the same `include_str!` files the crate
/// embeds -- a real second migration source over the shipped SQL, not a
/// hand-copied subset that would drift the moment a migration is added.
///
/// Composed as `V12` plus everything that follows it, rather than as a second
/// literal list of 28 entries. An earlier draft of the equivalent rig in
/// `run_transition_integrity.rs` listed the entries one line at a time and was
/// wrong twice (29 entries for a 28-element array, then 3 after a partial edit);
/// the compiler catches the length and never the drift, which is
/// ADVERSARIAL_REVIEW_PLAYBOOK rule 6 in miniature.
const PRE_0029: [(&str, &str); 28] = [
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
    (
        "retained entity integrity",
        include_str!("../migrations/0028_retained_entity_integrity.sql"),
    ),
];

#[derive(Debug)]
struct Pre0029Migrations;

impl MigrationSource<'static> for Pre0029Migrations {
    fn resolve(
        self,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<Migration>, BoxDynError>> + Send + 'static>> {
        Box::pin(async {
            Ok(PRE_0029
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

async fn pre_0029_database() -> Result<SqlitePool, Box<dyn Error>> {
    let p = SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(SqliteConnectOptions::from_str("sqlite::memory:")?.foreign_keys(true))
        .await?;
    Migrator::new(Pre0029Migrations).await?.run(&p).await?;
    Ok(p)
}

/// The `0012` guards this file pins still carry the load **without** `0029`.
///
/// FC-03 added `runs_terminal_status_integrity_update`, which states the same
/// frozen rule as two `0012` sub-clauses: a terminal Run has no outgoing edge
/// (STATE_MACHINES section 3). SQLite runs `BEFORE` triggers in reverse creation
/// order, so on the full schema `0029` answers first for the whole terminal
/// quadrant and the two assertions in this file no longer observe `0012`'s own
/// message. Broadening them to accept either guard keeps them meaningful, but on
/// its own it would leave `0012` unproven -- a guard that had been replaced
/// wholesale would pass those same broadened assertions.
///
/// So the claim is checked the only way it can be: against a database built from
/// `0001..0028`, where `0029` does not exist and `0012` is the only thing that
/// can refuse. The rig asserts `0029`'s absence first, because a baseline that
/// silently loaded the full schema would produce output identical to a real
/// result (ADVERSARIAL_REVIEW_PLAYBOOK rule 1-b).
#[tokio::test]
async fn the_pre_existing_terminal_guard_still_bears_weight_without_0029()
-> Result<(), Box<dyn Error>> {
    for (index, status) in ["SUCCEEDED", "FAILED", "TIMED_OUT", "CANCELLED"]
        .into_iter()
        .enumerate()
    {
        let p = pre_0029_database().await?;
        let present: i64 = query_scalar(
            "SELECT COUNT(*) FROM sqlite_master WHERE type = 'trigger' \
             AND name = 'runs_terminal_status_integrity_update'",
        )
        .fetch_one(&p)
        .await?;
        assert_eq!(
            present, 0,
            "the pre-0029 baseline already carries the 0029 trigger, so this case \\
             cannot show which guard refused"
        );

        let p = pre_0029_database().await?;
        const PREFIX: &str = "01921389d31174628f80cb5f79ae";
        let id = |n: usize| -> String { format!("{PREFIX}{n:04x}") };
        let base = 0xf000 + index * 0x0100;
        let (team, role, harness, adapter, binding, task) = (
            id(base),
            id(base + 1),
            id(base + 2),
            id(base + 3),
            id(base + 4),
            id(base + 5),
        );
        let (run, reviewer, review) = (id(base + 6), id(base + 7), id(base + 8));
        graph(&p, &team, &role, &harness, &adapter, &binding, &task).await?;
        query(&format!(
            "INSERT INTO roles VALUES(X'{reviewer}',X'{team}','rv','r','ACTIVE',1,1,1,NULL)"
        ))
        .execute(&p)
        .await?;
        query(&run_row(
            &run,
            &team,
            &task,
            &role,
            &binding,
            &harness,
            1,
            1,
            "SUCCEEDED",
        ))
        .execute(&p)
        .await?;
        query(&format!(
            "INSERT INTO reviews VALUES(X'{review}',X'{team}',X'{task}',X'{run}',\
             X'{reviewer}','APPROVED',NULL,1,3)"
        ))
        .execute(&p)
        .await?;

        // Both of `0012`'s terminal rules at once, on a reviewed Run: the
        // reviewed-run rule and the terminal-status sub-clause.
        for destination in ["RUNNING", status] {
            // Writing `SUCCEEDED` back onto a `SUCCEEDED` Run is not a
            // transition and is legal everywhere, including before `0029`. It is
            // excluded here because "the reviewed Run may be touched at all" is
            // already covered by the field-only control in
            // `a_reviewed_run_keeps_its_verdict`, and including it here would
            // make this case assert that a legal write is refused.
            if destination == "SUCCEEDED" {
                continue;
            }
            let (started, done) = if destination == "RUNNING" {
                ("1", "NULL")
            } else {
                ("1", "3")
            };
            let outcome = query(&format!(
                "UPDATE runs SET status = '{destination}', started_at = {started}, \
                 completed_at = {done} WHERE attempt = 1"
            ))
            .execute(&p)
            .await;
            match outcome {
                Ok(_) => panic!(
                    "a reviewed {status} Run was moved to {destination} on the \
                     pre-0029 schema, so 0012 is no longer refusing"
                ),
                Err(e) => assert!(
                    format!("{e}").contains("Run consistency violation"),
                    "moving a reviewed Run to {destination} failed for '{e}' rather \
                     than at the 0012 reviewed-run rule"
                ),
            }
            let still: String = query_scalar("SELECT status FROM runs")
                .fetch_one(&p)
                .await?;
            assert_eq!(
                still, "SUCCEEDED",
                "the refused UPDATE still changed the status"
            );
        }
    }
    Ok(())
}
