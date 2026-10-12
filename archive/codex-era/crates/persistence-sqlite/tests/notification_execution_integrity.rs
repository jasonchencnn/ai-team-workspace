use std::{error::Error, path::PathBuf, str::FromStr};

use persistence_sqlite::migrate;
use sqlx_core::{migrate::Migrator, query::query, query_scalar::query_scalar};
use sqlx_sqlite::{SqliteConnectOptions, SqlitePool, SqlitePoolOptions};

const TEAM: &str = "01921389d31174628f80cb5f79aab001";
const ROLE: &str = "01921389d31174628f80cb5f79aab002";
const HUMAN: &str = "01921389d31174628f80cb5f79aab003";
const TASK: &str = "01921389d31174628f80cb5f79aab004";
const ATTENTION: &str = "01921389d31174628f80cb5f79aab005";
const NOTIFICATION: &str = "01921389d31174628f80cb5f79aab006";

async fn db_at(version: Option<i64>) -> Result<SqlitePool, Box<dyn Error>> {
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(SqliteConnectOptions::from_str("sqlite::memory:")?.foreign_keys(true))
        .await?;
    if let Some(version) = version {
        let mut migrator =
            Migrator::new(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("migrations")).await?;
        migrator
            .migrations
            .to_mut()
            .retain(|migration| migration.version <= version);
        migrator.run(&pool).await?;
    } else {
        migrate(&pool).await?;
    }
    query(&format!(
        "INSERT INTO teams VALUES(X'{TEAM}','team','ACTIVE',1,1,1,NULL)"
    ))
    .execute(&pool)
    .await?;
    query(&format!(
        "INSERT INTO roles VALUES(X'{ROLE}',X'{TEAM}','role','role','ACTIVE',1,1,1,NULL)"
    ))
    .execute(&pool)
    .await?;
    query(&format!("INSERT INTO tasks VALUES(X'{TASK}',X'{TEAM}',X'{ROLE}','TASK-1','task','reason','BLOCKED',1,1,1,NULL,NULL)"))
        .execute(&pool).await?;
    Ok(pool)
}

async fn db() -> Result<SqlitePool, Box<dyn Error>> {
    db_at(None).await
}

fn notification(recipient: &str) -> String {
    format!(
        "INSERT INTO notifications(id,team_id,recipient_human_id,severity,title,body,status,version,created_at) VALUES(X'{NOTIFICATION}',X'{TEAM}',X'{recipient}','INFO','title','body','UNREAD',1,1)"
    )
}

async fn expect_guard(pool: &SqlitePool, statement: &str, expected: &str) {
    let error = query(statement)
        .execute(pool)
        .await
        .expect_err("guard must reject");
    assert!(
        error.to_string().contains(expected),
        "wrong refusal: {error}"
    );
}

#[tokio::test]
async fn recipient_cannot_be_a_role_and_a_human_id_cannot_be_repurposed_as_role()
-> Result<(), Box<dyn Error>> {
    let pool = db().await?;
    expect_guard(
        &pool,
        &notification(ROLE),
        "Notification recipient cannot be a Role",
    )
    .await;
    assert_eq!(
        query_scalar::<_, i64>("SELECT COUNT(*) FROM notifications")
            .fetch_one(&pool)
            .await?,
        0
    );
    query(&notification(HUMAN)).execute(&pool).await?;
    expect_guard(
        &pool,
        &format!(
            "INSERT INTO roles VALUES(X'{HUMAN}',X'{TEAM}','other','other','ACTIVE',1,1,1,NULL)"
        ),
        "Role identity is used by a Human notification",
    )
    .await;
    assert_eq!(
        query_scalar::<_, i64>("SELECT COUNT(*) FROM roles")
            .fetch_one(&pool)
            .await?,
        1
    );
    Ok(())
}

#[tokio::test]
async fn an_attention_item_cannot_hide_an_unresolved_task() -> Result<(), Box<dyn Error>> {
    let pool = db().await?;
    query(&format!("INSERT INTO attention_items(id,team_id,attention_type,source_type,source_id,title,summary,status,version,created_at,updated_at) VALUES(X'{ATTENTION}',X'{TEAM}','TASK_BLOCKED','TASK',X'{TASK}','blocked','reason','OPEN',1,1,1)"))
        .execute(&pool).await?;
    let resolve = format!(
        "UPDATE attention_items SET status='RESOLVED',resolved_at=2,updated_at=2,version=2 WHERE id=X'{ATTENTION}'"
    );
    expect_guard(&pool, &resolve, "Attention source is unresolved").await;
    assert_eq!(
        query_scalar::<_, String>("SELECT status FROM attention_items")
            .fetch_one(&pool)
            .await?,
        "OPEN"
    );
    query(&format!(
        "UPDATE tasks SET status='READY',version=2,updated_at=2 WHERE id=X'{TASK}'"
    ))
    .execute(&pool)
    .await?;
    query(&resolve).execute(&pool).await?;
    assert_eq!(
        query_scalar::<_, String>("SELECT status FROM attention_items")
            .fetch_one(&pool)
            .await?,
        "RESOLVED"
    );
    Ok(())
}

const POLICY: &str = "01921389d31174628f80cb5f79aab007";
const OTHER_TASK: &str = "01921389d31174628f80cb5f79aab008";
const APPROVAL: &str = "01921389d31174628f80cb5f79aab009";
const HARNESS: &str = "01921389d31174628f80cb5f79aab00a";
const ADAPTER: &str = "01921389d31174628f80cb5f79aab00b";
const BINDING: &str = "01921389d31174628f80cb5f79aab00c";
const RUN: &str = "01921389d31174628f80cb5f79aab00d";
const EVENT: &str = "01921389d31174628f80cb5f79aab010";
const THREAD: &str = "01921389d31174628f80cb5f79aab00e";
const MESSAGE: &str = "01921389d31174628f80cb5f79aab00f";

fn attention(source_type: &str, source_id: &str, kind: &str) -> String {
    format!(
        "INSERT INTO attention_items(id,team_id,attention_type,source_type,source_id,title,summary,status,version,created_at,updated_at) VALUES(X'{ATTENTION}',X'{TEAM}','{kind}','{source_type}',X'{source_id}','title','summary','OPEN',1,1,1)"
    )
}

fn resolve() -> String {
    format!(
        "UPDATE attention_items SET status='RESOLVED',resolved_at=4,updated_at=4,version=2 WHERE id=X'{ATTENTION}'"
    )
}

async fn policy(pool: &SqlitePool) -> Result<(), Box<dyn Error>> {
    query(&format!("INSERT INTO notification_policies VALUES(X'{POLICY}',X'{TEAM}','policy','SECURITY_ALERT','IN_APP','CRITICAL',0,86400,'ACTIVE',1,1,1)"))
        .execute(pool).await?;
    Ok(())
}

#[tokio::test]
async fn source_references_cannot_be_redirected_even_inside_one_team() -> Result<(), Box<dyn Error>>
{
    let pool = db().await?;
    policy(&pool).await?;
    query(&notification(HUMAN)).execute(&pool).await?;
    query(&format!("INSERT INTO events(id,team_id,event_type,actor_type,subject_type,subject_id,object_version,occurred_at,hash) VALUES(X'{EVENT}',X'{TEAM}','TASK_BLOCKED','SYSTEM','TASK',X'{TASK}',1,1,?)"))
        .bind("0".repeat(64)).execute(&pool).await?;
    query(&format!(
        "INSERT INTO threads VALUES(X'{THREAD}',X'{TEAM}','thread','OPEN',1,1,1)"
    ))
    .execute(&pool)
    .await?;
    query(&format!("INSERT INTO messages(id,team_id,thread_id,sender_actor_type,from_human_id,to_role_id,message_type,body,status,version,created_at,updated_at) VALUES(X'{MESSAGE}',X'{TEAM}',X'{THREAD}','HUMAN',X'{HUMAN}',X'{ROLE}','DIRECT','body','DRAFT',1,1,1)"))
        .execute(&pool).await?;
    for (column, target) in [("source_event_id", EVENT), ("source_message_id", MESSAGE)] {
        expect_guard(
            &pool,
            &format!("UPDATE notifications SET {column}=X'{target}'"),
            "Notification source is immutable",
        )
        .await;
    }
    expect_guard(
        &pool,
        &format!("UPDATE notifications SET policy_id=X'{POLICY}'"),
        "Notification source is immutable",
    )
    .await;
    query(&attention("TASK", TASK, "TASK_BLOCKED"))
        .execute(&pool)
        .await?;
    query(&format!("INSERT INTO tasks VALUES(X'{OTHER_TASK}',X'{TEAM}',X'{ROLE}','TASK-2','other','','READY',1,1,1,NULL,NULL)"))
        .execute(&pool).await?;
    expect_guard(
        &pool,
        &format!("UPDATE attention_items SET source_id=X'{OTHER_TASK}'"),
        "Attention source is immutable",
    )
    .await;
    expect_guard(
        &pool,
        "UPDATE attention_items SET source_type='SYSTEM'",
        "Attention source is immutable",
    )
    .await;
    query(&format!(
        "UPDATE tasks SET status='READY',version=2,updated_at=2 WHERE id=X'{TASK}'"
    ))
    .execute(&pool)
    .await?;
    query(&resolve()).execute(&pool).await?;
    expect_guard(
        &pool,
        &format!("UPDATE attention_items SET source_id=X'{OTHER_TASK}'"),
        "Attention source is immutable",
    )
    .await;
    assert_eq!(
        query_scalar::<_, String>("SELECT lower(hex(source_id)) FROM attention_items")
            .fetch_one(&pool)
            .await?,
        TASK
    );
    Ok(())
}

#[tokio::test]
async fn notification_role_collision_update_and_replace_paths_are_rejected()
-> Result<(), Box<dyn Error>> {
    let pool = db().await?;
    query(&notification(HUMAN)).execute(&pool).await?;
    // Several independent immutable guards may reject these writes; assert the
    // preserved recipient and row count rather than SQLite trigger ordering.
    for sql in [
        format!("UPDATE notifications SET recipient_human_id=X'{ROLE}'"),
        notification(ROLE).replacen("INSERT INTO", "INSERT OR REPLACE INTO", 1),
        format!("UPDATE roles SET id=X'{HUMAN}' WHERE id=X'{ROLE}'"),
        format!(
            "INSERT OR REPLACE INTO roles VALUES(X'{HUMAN}',X'{TEAM}','other','other','ACTIVE',1,1,1,NULL)"
        ),
    ] {
        assert!(
            query(&sql).execute(&pool).await.is_err(),
            "forbidden write accepted: {sql}"
        );
    }
    assert_eq!(
        query_scalar::<_, String>("SELECT lower(hex(recipient_human_id)) FROM notifications")
            .fetch_one(&pool)
            .await?,
        HUMAN
    );
    assert_eq!(
        query_scalar::<_, i64>("SELECT COUNT(*) FROM roles")
            .fetch_one(&pool)
            .await?,
        1
    );
    Ok(())
}

#[tokio::test]
async fn pending_approval_cannot_be_hidden_but_expiry_resolves_it() -> Result<(), Box<dyn Error>> {
    let pool = db().await?;
    query(&format!("INSERT INTO approvals(id,team_id,requester_actor_type,requester_role_id,action,resource_type,resource_scope,reason,status,version,expires_at,created_at) VALUES(X'{APPROVAL}',X'{TEAM}','ROLE',X'{ROLE}','UPDATE','TASK_MUTATION','task','reason','PENDING',1,3,1)"))
        .execute(&pool).await?;
    query(&attention("APPROVAL", APPROVAL, "APPROVAL_REQUIRED"))
        .execute(&pool)
        .await?;
    expect_guard(&pool, &resolve(), "Attention source is unresolved").await;
    query("UPDATE approvals SET status='EXPIRED',version=2")
        .execute(&pool)
        .await?;
    query(&resolve()).execute(&pool).await?;
    Ok(())
}

async fn run(pool: &SqlitePool, status: &str) -> Result<(), Box<dyn Error>> {
    for sql in [
        format!(
            "INSERT INTO harness_instances VALUES(X'{HARNESS}',X'{TEAM}','mock','harness','ACTIVE','[]',NULL,1,1,1,NULL)"
        ),
        format!(
            "INSERT INTO adapters VALUES(X'{ADAPTER}',X'{TEAM}','adapter','1','/adapter','{{}}',1,1,1)"
        ),
        format!(
            "INSERT INTO bindings VALUES(X'{BINDING}',X'{TEAM}',X'{ROLE}',X'{HARNESS}',X'{ADAPTER}','ACTIVE',1,1,'{{}}',1,1,1,NULL)"
        ),
        format!(
            "INSERT INTO runs(id,team_id,task_id,role_id,binding_id,harness_instance_id,attempt,fencing_token,status,version,created_at,started_at,completed_at) VALUES(X'{RUN}',X'{TEAM}',X'{TASK}',X'{ROLE}',X'{BINDING}',X'{HARNESS}',1,1,'{status}',1,1,1,{})",
            if status == "UNKNOWN" { "NULL" } else { "2" }
        ),
    ] {
        query(&sql).execute(pool).await?;
    }
    Ok(())
}

#[tokio::test]
async fn run_issues_require_resolution_and_unknown_never_counts_as_resolved()
-> Result<(), Box<dyn Error>> {
    for status in ["FAILED", "TIMED_OUT", "UNKNOWN", "CANCELLED", "SUCCEEDED"] {
        let pool = db().await?;
        run(&pool, status).await?;
        query(&attention("RUN", RUN, "RUN_FAILED"))
            .execute(&pool)
            .await?;
        if matches!(status, "SUCCEEDED" | "CANCELLED") {
            query(&resolve()).execute(&pool).await?;
        } else {
            expect_guard(&pool, &resolve(), "Attention source is unresolved").await;
            // Task cancellation is an explicit handling outcome. UNKNOWN still
            // requires reconciliation even if a Task row says CANCELLED.
            query(&format!(
                "UPDATE tasks SET status='CANCELLED',cancelled_at=3,version=2,updated_at=3 WHERE id=X'{TASK}'"
            ))
            .execute(&pool)
            .await?;
            if status == "UNKNOWN" {
                expect_guard(&pool, &resolve(), "Attention source is unresolved").await;
            } else {
                query(&resolve()).execute(&pool).await?;
            }
        }
    }
    Ok(())
}

#[tokio::test]
async fn message_needing_human_cannot_be_hidden_until_routing_is_fixed()
-> Result<(), Box<dyn Error>> {
    let pool = db().await?;
    query(&format!(
        "INSERT INTO threads VALUES(X'{THREAD}',X'{TEAM}','thread','OPEN',1,1,1)"
    ))
    .execute(&pool)
    .await?;
    query(&format!("INSERT INTO messages(id,team_id,thread_id,sender_actor_type,from_human_id,to_role_id,message_type,body,status,version,expires_at,created_at,updated_at) VALUES(X'{MESSAGE}',X'{TEAM}',X'{THREAD}','HUMAN',X'{HUMAN}',X'{ROLE}','DIRECT','body','DRAFT',1,100,1,1)"))
        .execute(&pool).await?;
    query("UPDATE messages SET status='QUEUED',idempotency_key='send-1',version=2")
        .execute(&pool)
        .await?;
    query("UPDATE messages SET status='NEEDS_HUMAN',version=3")
        .execute(&pool)
        .await?;
    query(&attention("MESSAGE", MESSAGE, "MESSAGE_NEEDS_HUMAN"))
        .execute(&pool)
        .await?;
    expect_guard(&pool, &resolve(), "Attention source is unresolved").await;
    query("UPDATE messages SET status='QUEUED',version=4")
        .execute(&pool)
        .await?;
    query(&resolve()).execute(&pool).await?;
    Ok(())
}

#[tokio::test]
async fn system_item_without_backing_resolution_is_not_automatically_resolved()
-> Result<(), Box<dyn Error>> {
    let pool = db().await?;
    query(&attention("SYSTEM", HUMAN, "SECURITY_ALERT"))
        .execute(&pool)
        .await?;
    expect_guard(&pool, &resolve(), "Attention source is unresolved").await;
    Ok(())
}

#[tokio::test]
async fn policy_options_require_a_versioned_parent_and_preserve_mandatory_marker()
-> Result<(), Box<dyn Error>> {
    let pool = db().await?;
    policy(&pool).await?;
    let insert = format!(
        "INSERT INTO notification_policy_options(policy_id,filters,escalation,is_mandatory,policy_version) VALUES(X'{POLICY}','{{ }}','{{}}',1,1)"
    );
    // Bind JSON instead of constructing SQL literals with embedded quotes.
    query("INSERT INTO notification_policy_options(policy_id,filters,escalation,is_mandatory,policy_version) SELECT id,?, ?,1,1 FROM notification_policies")
        .bind("{ }").bind(r#"{"after_seconds":30}"#).execute(&pool).await?;
    expect_guard(
        &pool,
        "UPDATE notification_policy_options SET policy_version=2",
        "Notification policy options invalid",
    )
    .await;
    query("UPDATE notification_policies SET version=2,updated_at=2")
        .execute(&pool)
        .await?;
    for sql in [
        "UPDATE notification_policy_options SET is_mandatory=0,policy_version=2".to_owned(),
        r#"UPDATE notification_policy_options SET filters='{"ignored":true}',policy_version=2"#
            .to_owned(),
        "UPDATE notification_policy_options SET escalation='[]',policy_version=2".to_owned(),
        "UPDATE notification_policy_options SET policy_version=1".to_owned(),
        "UPDATE notification_policies SET status='DISABLED',version=2".to_owned(),
        "UPDATE notification_policies SET event_type='TASK_READY',version=2".to_owned(),
        "DELETE FROM notification_policy_options".to_owned(),
        insert.replacen("INSERT INTO", "INSERT OR REPLACE INTO", 1),
    ] {
        assert!(
            query(&sql).execute(&pool).await.is_err(),
            "forbidden policy write accepted: {sql}"
        );
    }
    query("UPDATE notification_policy_options SET escalation=?,policy_version=2")
        .bind(r#"{"after_seconds":60}"#)
        .execute(&pool)
        .await?;
    assert_eq!(
        query_scalar::<_, String>("SELECT escalation FROM notification_policy_options")
            .fetch_one(&pool)
            .await?,
        r#"{"after_seconds":60}"#
    );
    Ok(())
}

#[tokio::test]
async fn incompatible_identity_upgrade_rolls_back_and_keeps_old_rows() -> Result<(), Box<dyn Error>>
{
    let pool = db_at(Some(22)).await?;
    // Prove that the old schema actually accepts the forbidden identity.
    query(&notification(ROLE)).execute(&pool).await?;
    assert!(migrate(&pool).await.is_err());
    assert_eq!(
        query_scalar::<_, i64>("SELECT MAX(version) FROM _sqlx_migrations")
            .fetch_one(&pool)
            .await?,
        22
    );
    assert_eq!(query_scalar::<_, i64>("SELECT COUNT(*) FROM sqlite_master WHERE name IN ('notification_execution_upgrade_guard','notification_policy_options','notifications_recipient_not_role_insert')").fetch_one(&pool).await?, 0);
    assert_eq!(
        query_scalar::<_, String>("SELECT lower(hex(recipient_human_id)) FROM notifications")
            .fetch_one(&pool)
            .await?,
        ROLE
    );
    Ok(())
}

#[tokio::test]
async fn a_later_task_block_does_not_invalidate_historical_attention_resolution()
-> Result<(), Box<dyn Error>> {
    let pool = db_at(Some(22)).await?;
    query(&attention("TASK", TASK, "TASK_BLOCKED"))
        .execute(&pool)
        .await?;
    query(&format!(
        "UPDATE tasks SET status='READY',version=2,updated_at=2 WHERE id=X'{TASK}'"
    ))
    .execute(&pool)
    .await?;
    query(&resolve()).execute(&pool).await?;
    query(&format!(
        "UPDATE tasks SET status='DOING',version=3,updated_at=3 WHERE id=X'{TASK}'"
    ))
    .execute(&pool)
    .await?;
    query(&format!(
        "UPDATE tasks SET status='BLOCKED',version=4,updated_at=4 WHERE id=X'{TASK}'"
    ))
    .execute(&pool)
    .await?;
    migrate(&pool).await?;
    migrate(&pool).await?;
    assert_eq!(
        query_scalar::<_, String>("SELECT status FROM attention_items")
            .fetch_one(&pool)
            .await?,
        "RESOLVED"
    );
    // Stable form, not `MAX(version)`: this asserts that 0024 (the migration
    // under test) recorded exactly one successful application. `MAX(version)`
    // tracks the length of the whole migration chain, so it breaks every time
    // an unrelated migration is registered. See MR-04 in
    // `docs/reviews/MINIMAX-RETURN-AUDIT-2026-10-08.md`.
    assert_eq!(
        query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM _sqlx_migrations WHERE version = 24 AND success = TRUE"
        )
        .fetch_one(&pool)
        .await?,
        1,
        "0024 did not record exactly one successful application"
    );
    Ok(())
}

#[tokio::test]
async fn v22_accepts_source_redirection_that_v23_rejects() -> Result<(), Box<dyn Error>> {
    let pool = db_at(Some(22)).await?;
    policy(&pool).await?;
    query(&notification(HUMAN)).execute(&pool).await?;
    let changed = query(&format!("UPDATE notifications SET policy_id=X'{POLICY}'"))
        .execute(&pool)
        .await?;
    assert_eq!(changed.rows_affected(), 1);
    assert_eq!(
        query_scalar::<_, String>("SELECT lower(hex(policy_id)) FROM notifications")
            .fetch_one(&pool)
            .await?,
        POLICY
    );
    query(&attention("TASK", TASK, "TASK_BLOCKED"))
        .execute(&pool)
        .await?;
    query(&format!("INSERT INTO tasks VALUES(X'{OTHER_TASK}',X'{TEAM}',X'{ROLE}','TASK-2','other','','READY',1,1,1,NULL,NULL)"))
        .execute(&pool).await?;
    let changed = query(&format!(
        "UPDATE attention_items SET source_id=X'{OTHER_TASK}'"
    ))
    .execute(&pool)
    .await?;
    assert_eq!(changed.rows_affected(), 1);
    assert_eq!(
        query_scalar::<_, String>("SELECT lower(hex(source_id)) FROM attention_items")
            .fetch_one(&pool)
            .await?,
        OTHER_TASK
    );
    assert_eq!(query_scalar::<_, i64>("SELECT COUNT(*) FROM sqlite_master WHERE name IN ('attention_items_source_immutable_update','notifications_source_immutable_update')").fetch_one(&pool).await?, 0);
    Ok(())
}

#[tokio::test]
async fn failed_resolution_rolls_back_prior_source_changes_in_the_transaction()
-> Result<(), Box<dyn Error>> {
    let pool = db().await?;
    query(&attention("TASK", TASK, "TASK_BLOCKED"))
        .execute(&pool)
        .await?;
    let mut tx = pool.begin().await?;
    query(&format!(
        "UPDATE tasks SET title='changed',version=2,updated_at=2 WHERE id=X'{TASK}'"
    ))
    .execute(&mut *tx)
    .await?;
    let error = query(&resolve())
        .execute(&mut *tx)
        .await
        .expect_err("BLOCKED source must reject resolution");
    assert!(error.to_string().contains("Attention source is unresolved"));
    tx.rollback().await?;
    assert_eq!(
        query_scalar::<_, String>("SELECT title FROM tasks")
            .fetch_one(&pool)
            .await?,
        "task"
    );
    assert_eq!(
        query_scalar::<_, i64>("SELECT version FROM tasks")
            .fetch_one(&pool)
            .await?,
        1
    );
    assert_eq!(
        query_scalar::<_, String>("SELECT status FROM attention_items")
            .fetch_one(&pool)
            .await?,
        "OPEN"
    );
    Ok(())
}

#[tokio::test]
async fn notification_facts_are_retained_even_without_an_event_subject_reference()
-> Result<(), Box<dyn Error>> {
    let old = db_at(Some(22)).await?;
    query(&notification(HUMAN)).execute(&old).await?;
    assert_eq!(
        query("DELETE FROM notifications")
            .execute(&old)
            .await?
            .rows_affected(),
        1
    );
    assert_eq!(
        query_scalar::<_, i64>("SELECT COUNT(*) FROM notifications")
            .fetch_one(&old)
            .await?,
        0
    );
    let pool = db().await?;
    query(&notification(HUMAN)).execute(&pool).await?;
    expect_guard(
        &pool,
        "DELETE FROM notifications",
        "Notification history is retained",
    )
    .await;
    query("UPDATE notifications SET status='DISMISSED',dismissed_at=2,version=2")
        .execute(&pool)
        .await?;
    expect_guard(
        &pool,
        "DELETE FROM notifications",
        "Notification history is retained",
    )
    .await;
    assert_eq!(
        query_scalar::<_, i64>("SELECT COUNT(*) FROM notifications")
            .fetch_one(&pool)
            .await?,
        1
    );
    Ok(())
}
