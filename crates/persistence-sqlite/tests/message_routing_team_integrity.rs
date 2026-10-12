//! MM-R01 (DA-01): the Message Team guard was INSERT-only, so a `DRAFT` message
//! could be retargeted to another Team's Role and then committed through the
//! legal `DRAFT -> QUEUED` edge.
//!
//! The two counterexamples below (`draft_recipient_update_cannot_cross_team`
//! and `valid_same_team_draft_can_queue_and_history_passes_foreign_keys`) are
//! the audit regression carried over verbatim in behaviour from
//! `docs/reviews/evidence/deep-audit-2026-10-08/main-repro/`.
//!
//! Frozen basis:
//!   - `DOMAIN_MODEL.md` section 1: Team is the isolation boundary.
//!   - `PERMISSION_MATRIX.md` section 3, `Message 发送/回复/确认`: the guard
//!     condition is `to_role_id`/Team validation.
//!   - `STATE_MACHINES.md` section 5: `DRAFT -> QUEUED` freezes the content, and
//!     `QUEUED -> NEEDS_HUMAN` is a legal edge when the target has no Binding.
//!
//! Every assertion below pins both halves of a guard: the cross-Team write is
//! rejected *and* the row is proven unchanged, and the legal same-Team path is
//! proven to still work.

use std::{error::Error, str::FromStr};

use persistence_sqlite::migrate;
use sqlx_core::{query::query, query_scalar::query_scalar};
use sqlx_sqlite::{SqliteConnectOptions, SqlitePool, SqlitePoolOptions};

/// Same seed as the audit reproduction. `foreign_keys` is connection-local in
/// SQLite, so every connection sets it explicitly.
const SEED: &str = "\
INSERT INTO teams VALUES(X'01921389d31174628f80cb5f79ab0001','t','ACTIVE',1,1,1,NULL);
INSERT INTO roles VALUES(X'01921389d31174628f80cb5f79ab0002',X'01921389d31174628f80cb5f79ab0001','r','r','ACTIVE',1,1,1,NULL);
INSERT INTO harness_instances VALUES(X'01921389d31174628f80cb5f79ab0003',X'01921389d31174628f80cb5f79ab0001','h','h','ACTIVE','[]',NULL,1,1,1,NULL);
INSERT INTO adapters VALUES(X'01921389d31174628f80cb5f79ab0004',X'01921389d31174628f80cb5f79ab0001','a','1','/a','{}',1,1,1);
INSERT INTO bindings VALUES(X'01921389d31174628f80cb5f79ab0005',X'01921389d31174628f80cb5f79ab0001',X'01921389d31174628f80cb5f79ab0002',X'01921389d31174628f80cb5f79ab0003',X'01921389d31174628f80cb5f79ab0004','ACTIVE',1,1,'{}',1,1,1,NULL);
INSERT INTO tasks VALUES(X'01921389d31174628f80cb5f79ab0006',X'01921389d31174628f80cb5f79ab0001',X'01921389d31174628f80cb5f79ab0002','T-1','t','','DOING',1,1,1,NULL,NULL);
INSERT INTO runs VALUES(X'01921389d31174628f80cb5f79ab0007',X'01921389d31174628f80cb5f79ab0001',X'01921389d31174628f80cb5f79ab0006',X'01921389d31174628f80cb5f79ab0002',X'01921389d31174628f80cb5f79ab0005',X'01921389d31174628f80cb5f79ab0003',1,1,'SUCCEEDED',0,NULL,1,1,1,2);
";

/// Team A and its single bound Role, plus a second Role in Team A that has no
/// Binding at all (the `NEEDS_HUMAN` path).
const TEAM_A: &str = "01921389d31174628f80cb5f79ab0001";
const ROLE_A: &str = "01921389d31174628f80cb5f79ab0002";
const BINDING_A: &str = "01921389d31174628f80cb5f79ab0005";
const ROLE_A_UNBOUND: &str = "01921389d31174628f80cb5f79ab0012";
const MESSAGE: &str = "01921389d31174628f80cb5f79ab0021";

/// Team B: a foreign Team used only by the negative probes.
const TEAM_B: &str = "01921389d31174628f80cb5f79ab0030";
const ROLE_B: &str = "01921389d31174628f80cb5f79ab0031";
const BINDING_B: &str = "01921389d31174628f80cb5f79ab0034";
const THREAD_B: &str = "01921389d31174628f80cb5f79ab0035";

const FOREIGN_TEAM_FIXTURE: &str = "\
INSERT INTO teams VALUES(X'01921389d31174628f80cb5f79ab0030','other','ACTIVE',1,1,1,NULL);
INSERT INTO roles VALUES(X'01921389d31174628f80cb5f79ab0031',X'01921389d31174628f80cb5f79ab0030','other','other','ACTIVE',1,1,1,NULL);
INSERT INTO harness_instances VALUES(X'01921389d31174628f80cb5f79ab0032',X'01921389d31174628f80cb5f79ab0030','oh','oh','ACTIVE','[]',NULL,1,1,1,NULL);
INSERT INTO adapters VALUES(X'01921389d31174628f80cb5f79ab0033',X'01921389d31174628f80cb5f79ab0030','oa','1','/oa','{}',1,1,1);
INSERT INTO bindings VALUES(X'01921389d31174628f80cb5f79ab0034',X'01921389d31174628f80cb5f79ab0030',X'01921389d31174628f80cb5f79ab0031',X'01921389d31174628f80cb5f79ab0032',X'01921389d31174628f80cb5f79ab0033','ACTIVE',1,1,'{}',1,1,1,NULL);
INSERT INTO threads VALUES(X'01921389d31174628f80cb5f79ab0035',X'01921389d31174628f80cb5f79ab0030','othread','OPEN',1,1,1);
";

const MESSAGE_DRAFT: &str = "INSERT INTO threads VALUES(X'01921389d31174628f80cb5f79ab0020',X'01921389d31174628f80cb5f79ab0001','thread','OPEN',1,1,1); INSERT INTO messages(id,team_id,thread_id,sender_actor_type,from_human_id,to_role_id,message_type,body,status,version,created_at,updated_at) VALUES(X'01921389d31174628f80cb5f79ab0021',X'01921389d31174628f80cb5f79ab0001',X'01921389d31174628f80cb5f79ab0020','HUMAN',X'01921389d31174628f80cb5f79ab0022',X'01921389d31174628f80cb5f79ab0002','DIRECT','body','DRAFT',1,1,1);";

/// Every connection runs the full migration chain, the seed, and the device's
/// own self-checks.  The row-count and `pragma_foreign_key_check` assertions are
/// the anti-`no-op` guard: they prove the seed really produced the rows the
/// probes are about to attack.
async fn database() -> Result<SqlitePool, Box<dyn Error>> {
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(SqliteConnectOptions::from_str("sqlite::memory:")?.foreign_keys(true))
        .await?;
    migrate(&pool).await?;
    for statement in SEED.lines().filter(|line| !line.trim().is_empty()) {
        query(statement).execute(&pool).await?;
    }
    for (table, expected) in [
        ("teams", 1),
        ("roles", 1),
        ("harness_instances", 1),
        ("adapters", 1),
        ("bindings", 1),
        ("tasks", 1),
        ("runs", 1),
        ("messages", 0),
        ("delivery_attempts", 0),
    ] {
        assert_eq!(
            query_scalar::<_, i64>(&format!("SELECT COUNT(*) FROM {table}"))
                .fetch_one(&pool)
                .await?,
            expected,
            "seed row count for {table}"
        );
    }
    assert_eq!(
        query_scalar::<_, i64>("SELECT COUNT(*) FROM pragma_foreign_key_check")
            .fetch_one(&pool)
            .await?,
        0,
        "seed violated a foreign key"
    );
    Ok(pool)
}

/// `team_id`, `thread_id`, `to_role_id`, `from_role_id`, `body` — the mutable
/// routing surface of a `DRAFT` message, read back as uppercase hex so a BLOB
/// and its textual form compare identically.
async fn draft_routing_row(pool: &SqlitePool) -> Result<String, Box<dyn Error>> {
    Ok(query_scalar::<_, String>(
        "SELECT hex(team_id)||'|'||hex(thread_id)||'|'||hex(to_role_id)||'|'||COALESCE(hex(from_role_id),'NULL')||'|'||COALESCE(hex(sender_binding_snapshot_id),'NULL')||'|'||COALESCE(sender_binding_generation,'NULL')||'|'||body FROM messages WHERE id=X'01921389d31174628f80cb5f79ab0021'",
    )
    .fetch_one(pool)
    .await?)
}

/// The audit counterexample, carried over unchanged.
#[tokio::test]
async fn draft_recipient_update_cannot_cross_team() -> Result<(), Box<dyn Error>> {
    let pool = database().await?;
    for statement in MESSAGE_DRAFT.split("; ").filter(|s| !s.trim().is_empty()) {
        query(statement).execute(&pool).await?;
    }
    for statement in FOREIGN_TEAM_FIXTURE
        .lines()
        .filter(|line| !line.trim().is_empty())
    {
        query(statement).execute(&pool).await?;
    }
    assert_eq!(
        query_scalar::<_, i64>("SELECT COUNT(*) FROM roles")
            .fetch_one(&pool)
            .await?,
        2
    );

    let before = draft_routing_row(&pool).await?;
    let result = query("UPDATE messages SET to_role_id=X'01921389d31174628f80cb5f79ab0031'")
        .execute(&pool)
        .await;
    assert!(
        result.is_err(),
        "DRAFT accepted a recipient from another Team"
    );
    assert_eq!(
        draft_routing_row(&pool).await?,
        before,
        "rejected cross-Team write still mutated the row"
    );
    Ok(())
}

/// The audit's legal control, carried over unchanged.  It must stay green: the
/// fix may not turn a same-Team draft edit or send into an error.
#[tokio::test]
async fn valid_same_team_draft_can_queue_and_history_passes_foreign_keys()
-> Result<(), Box<dyn Error>> {
    let pool = database().await?;
    for statement in MESSAGE_DRAFT.split("; ").filter(|s| !s.trim().is_empty()) {
        query(statement).execute(&pool).await?;
    }
    query("UPDATE messages SET body='edited'")
        .execute(&pool)
        .await?;
    query("UPDATE messages SET status='QUEUED'")
        .execute(&pool)
        .await?;
    assert_eq!(
        query_scalar::<_, String>("SELECT status FROM messages")
            .fetch_one(&pool)
            .await?,
        "QUEUED"
    );
    assert_eq!(
        query_scalar::<_, i64>("SELECT COUNT(*) FROM pragma_foreign_key_check")
            .fetch_one(&pool)
            .await?,
        0
    );
    Ok(())
}

/// Every mutable routing column of a `DRAFT` must keep its Team.  Each probe is
/// checked twice: the statement fails, and the row still holds the seed values.
#[tokio::test]
async fn every_mutable_draft_routing_field_rejects_cross_team_and_keeps_the_row()
-> Result<(), Box<dyn Error>> {
    let pool = database().await?;
    for statement in MESSAGE_DRAFT.split("; ").filter(|s| !s.trim().is_empty()) {
        query(statement).execute(&pool).await?;
    }
    for statement in FOREIGN_TEAM_FIXTURE
        .lines()
        .filter(|line| !line.trim().is_empty())
    {
        query(statement).execute(&pool).await?;
    }
    // A second Role inside Team A, so the "same Team, different Role" edit is
    // available as a positive control below.
    query(&format!(
        "INSERT INTO roles VALUES(X'{ROLE_A_UNBOUND}',X'{TEAM_A}','r2','role','ACTIVE',1,1,1,NULL)"
    ))
    .execute(&pool)
    .await?;

    let before = draft_routing_row(&pool).await?;
    for (name, sql) in [
        (
            "recipient Role from another Team",
            format!("UPDATE messages SET to_role_id=X'{ROLE_B}'"),
        ),
        (
            "Thread from another Team",
            format!("UPDATE messages SET thread_id=X'{THREAD_B}'"),
        ),
        (
            "message moved to another Team",
            format!("UPDATE messages SET team_id=X'{TEAM_B}'"),
        ),
        (
            "sender Binding snapshot from another Team",
            format!(
                "UPDATE messages SET sender_binding_snapshot_id=X'{BINDING_B}', sender_binding_generation=1"
            ),
        ),
    ] {
        assert!(
            query(&sql).execute(&pool).await.is_err(),
            "DRAFT accepted {name}"
        );
        assert_eq!(
            draft_routing_row(&pool).await?,
            before,
            "rejected write to {name} still mutated the row"
        );
    }

    // The same-Team positive control: retargeting inside Team A is a legal
    // draft edit and must not be collateral damage of the new guard.
    query(&format!(
        "UPDATE messages SET to_role_id=X'{ROLE_A_UNBOUND}'"
    ))
    .execute(&pool)
    .await?;
    assert_eq!(
        query_scalar::<_, String>("SELECT hex(to_role_id) FROM messages")
            .fetch_one(&pool)
            .await?
            .to_uppercase(),
        ROLE_A_UNBOUND.to_uppercase()
    );

    // A same-Team draft remains queueable after editing.
    query("UPDATE messages SET status='QUEUED'")
        .execute(&pool)
        .await?;
    assert_eq!(
        query_scalar::<_, String>("SELECT status FROM messages")
            .fetch_one(&pool)
            .await?,
        "QUEUED"
    );
    Ok(())
}

/// A `HUMAN`/`SYSTEM` sender has no Role-Binding mapping, so it must never carry
/// a snapshot — on INSERT (0017) or on UPDATE.  Only DRAFT left that seam open.
#[tokio::test]
async fn a_draft_human_message_cannot_gain_a_role_binding_snapshot() -> Result<(), Box<dyn Error>> {
    let pool = database().await?;
    for statement in MESSAGE_DRAFT.split("; ").filter(|s| !s.trim().is_empty()) {
        query(statement).execute(&pool).await?;
    }
    let before = draft_routing_row(&pool).await?;
    assert!(
        query(&format!(
            "UPDATE messages SET sender_binding_snapshot_id=X'{BINDING_A}', sender_binding_generation=1"
        ))
        .execute(&pool)
        .await
        .is_err(),
        "DRAFT HUMAN message acquired a Role-Binding snapshot"
    );
    assert_eq!(draft_routing_row(&pool).await?, before);
    Ok(())
}

/// The end-to-end harm DA-01 describes: retarget a DRAFT across Teams, send it,
/// and let the router create a delivery attempt on the foreign route.
///
/// Each stage below is a legal state transition in isolation.  Against `0017`
/// the chain runs to completion, which is the whole defect: the message guard
/// had no UPDATE arm, and `delivery_attempts_team_consistency_*` only compares
/// the attempt to the (now already corrupted) message.
#[tokio::test]
async fn a_draft_cannot_be_misrouted_into_a_foreign_team_attempt() -> Result<(), Box<dyn Error>> {
    let pool = database().await?;
    for statement in MESSAGE_DRAFT.split("; ").filter(|s| !s.trim().is_empty()) {
        query(statement).execute(&pool).await?;
    }
    for statement in FOREIGN_TEAM_FIXTURE
        .lines()
        .filter(|line| !line.trim().is_empty())
    {
        query(statement).execute(&pool).await?;
    }

    // Stage 1: redirect the recipient while the message is still a DRAFT.
    assert!(
        query(&format!("UPDATE messages SET to_role_id=X'{ROLE_B}'"))
            .execute(&pool)
            .await
            .is_err(),
        "DRAFT was retargeted to another Team's Role"
    );

    // Stage 2: the sender sends it.  Still the intended recipient, so the send
    // is legal and must succeed.
    query("UPDATE messages SET status='QUEUED'")
        .execute(&pool)
        .await?;
    query("UPDATE messages SET status='ROUTED'")
        .execute(&pool)
        .await?;

    // Stage 3: the router resolves the foreign Role's Binding and records it.
    // No attempt may exist on a foreign route, and the correct same-Team route
    // must still be available.
    assert!(
        query(&format!(
            "INSERT INTO delivery_attempts (id,team_id,message_id,attempt_number,target_role_id,target_binding_id,target_binding_generation,status,created_at) VALUES(X'01921389d31174628f80cb5f79ab0042',X'{TEAM_A}',X'{MESSAGE}',1,X'{ROLE_B}',X'{BINDING_B}',1,'PENDING',1)"
        ))
        .execute(&pool)
        .await
        .is_err(),
        "a delivery attempt was created on a foreign Team's route"
    );
    assert_eq!(
        query_scalar::<_, i64>("SELECT COUNT(*) FROM delivery_attempts")
            .fetch_one(&pool)
            .await?,
        0,
        "a rejected attempt left a row behind"
    );

    // The legitimate route is untouched by the fix.
    query(&format!(
        "INSERT INTO delivery_attempts (id,team_id,message_id,attempt_number,target_role_id,target_binding_id,target_binding_generation,status,created_at) VALUES(X'01921389d31174628f80cb5f79ab0041',X'{TEAM_A}',X'{MESSAGE}',1,X'{ROLE_A}',X'{BINDING_A}',1,'PENDING',1)"
    ))
    .execute(&pool)
    .await?;
    assert_eq!(
        query_scalar::<_, i64>("SELECT COUNT(*) FROM delivery_attempts")
            .fetch_one(&pool)
            .await?,
        1
    );

    // An existing attempt may not be re-pointed at another Team either.
    assert!(
        query(&format!(
            "UPDATE delivery_attempts SET target_role_id=X'{ROLE_B}', target_binding_id=X'{BINDING_B}', target_binding_generation=1"
        ))
        .execute(&pool)
        .await
        .is_err(),
        "an existing delivery attempt was retargeted to another Team"
    );
    assert_eq!(
        query_scalar::<_, String>(
            "SELECT hex(target_role_id)||'|'||hex(target_binding_id) FROM delivery_attempts"
        )
        .fetch_one(&pool)
        .await?
        .to_uppercase(),
        format!("{}|{}", ROLE_A.to_uppercase(), BINDING_A.to_uppercase())
    );
    Ok(())
}

/// `STATE_MACHINES.md` section 5 keeps `QUEUED -> NEEDS_HUMAN` legal when the
/// target Role has no usable Binding.  0025 must not require every target to be
/// pre-bound, and must not invent a state.
#[tokio::test]
async fn an_unbound_target_may_still_be_queued_and_escalated() -> Result<(), Box<dyn Error>> {
    let pool = database().await?;
    query(&format!(
        "INSERT INTO roles VALUES(X'{ROLE_A_UNBOUND}',X'{TEAM_A}','r2','role','ACTIVE',1,1,1,NULL)"
    ))
    .execute(&pool)
    .await?;
    for statement in MESSAGE_DRAFT.split("; ").filter(|s| !s.trim().is_empty()) {
        query(statement).execute(&pool).await?;
    }
    // Retarget inside Team A to the Role that has no Binding.
    query(&format!(
        "UPDATE messages SET to_role_id=X'{ROLE_A_UNBOUND}'"
    ))
    .execute(&pool)
    .await?;
    for status in ["QUEUED", "NEEDS_HUMAN"] {
        query(&format!("UPDATE messages SET status='{status}'"))
            .execute(&pool)
            .await?;
    }
    assert_eq!(
        query_scalar::<_, String>("SELECT status FROM messages")
            .fetch_one(&pool)
            .await?,
        "NEEDS_HUMAN"
    );
    assert_eq!(
        query_scalar::<_, i64>("SELECT COUNT(*) FROM delivery_attempts")
            .fetch_one(&pool)
            .await?,
        0,
        "an unbound target must not be forced into a delivery attempt"
    );
    Ok(())
}
