//! Real native identity files + actual SQLite identity references, no IPC claims.
#![cfg(unix)]
use application::owner_identity::{OwnerIdentity, OwnerIdentityError, resolve_owner_recipients};
use persistence_sqlite::{migrate, owner_identity::SqliteOwnerRoleReferences};
use security::installation_owner::open_installation;
use sqlx_core::{query::query, query_scalar::query_scalar};
use sqlx_sqlite::{SqliteConnectOptions, SqlitePool, SqlitePoolOptions};
use std::{error::Error, path::PathBuf, str::FromStr};
use teamspace_domain::{Actor, HumanId, RoleId};
struct TempRoot(PathBuf);
impl TempRoot {
    fn new() -> Self {
        use std::sync::atomic::{AtomicUsize, Ordering};
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        Self(std::env::temp_dir().join(format!(
            "teamspace-owner-sql-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        )))
    }
}
impl Drop for TempRoot {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
async fn db() -> Result<SqlitePool, Box<dyn Error>> {
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(SqliteConnectOptions::from_str("sqlite::memory:")?.foreign_keys(true))
        .await?;
    migrate(&pool).await?;
    query(
        "INSERT INTO teams VALUES(X'01921389d31174628f80cb5f79aaee01','team','ACTIVE',1,1,1,NULL)",
    )
    .execute(&pool)
    .await?;
    Ok(pool)
}
fn other() -> HumanId {
    HumanId::parse_protocol("01921389-d311-7462-8f80-cb5f79aaee02").expect("fixture UUID")
}

#[tokio::test]
async fn one_native_owner_is_used_for_notification_and_policy_references()
-> Result<(), Box<dyn Error>> {
    let root = TempRoot::new();
    let source = open_installation(&root.0)?;
    let pool = db().await?;
    let references = SqliteOwnerRoleReferences::new(pool.clone());
    let recipients = resolve_owner_recipients(&source, &references).await?;
    assert_eq!(
        recipients.notification_recipient(),
        recipients.policy_recipient()
    );
    query("INSERT INTO notification_policies VALUES(X'01921389d31174628f80cb5f79aaee03',X'01921389d31174628f80cb5f79aaee01','policy','TASK_READY','IN_APP','INFO',0,86400,'ACTIVE',1,1,1)").execute(&pool).await?;
    query("INSERT INTO notifications(id,team_id,recipient_human_id,policy_id,severity,title,body,status,version,created_at) VALUES(X'01921389d31174628f80cb5f79aaee04',X'01921389d31174628f80cb5f79aaee01',?,X'01921389d31174628f80cb5f79aaee03','INFO','title','body','UNREAD',1,1)").bind(recipients.notification_recipient().database_bytes().to_vec()).execute(&pool).await?;
    let bytes: Vec<u8> = query_scalar("SELECT recipient_human_id FROM notifications")
        .fetch_one(&pool)
        .await?;
    assert_eq!(bytes, recipients.policy_recipient().database_bytes());
    assert_eq!(
        recipients.check_recipient(other()),
        Err(OwnerIdentityError::PermissionDenied)
    );
    let actor = Actor::Human(other());
    assert_eq!(
        recipients.check_human_attribution(actor),
        Err(OwnerIdentityError::PermissionDenied)
    );
    assert_eq!(
        recipients.check_recipient(recipients.notification_recipient()),
        Ok(())
    );
    assert_eq!(
        recipients.check_human_attribution(Actor::Human(recipients.notification_recipient())),
        Ok(())
    );
    for actor in [
        Actor::Role(RoleId::from_uuid(other().uuid())),
        Actor::System,
        Actor::Adapter(teamspace_domain::AdapterId::from_uuid(other().uuid())),
    ] {
        assert_eq!(
            recipients.check_human_attribution(actor),
            Err(OwnerIdentityError::HumanRequired)
        );
    }
    assert_eq!(
        query_scalar::<_, i64>("SELECT count(*) FROM notifications")
            .fetch_one(&pool)
            .await?,
        1
    );
    Ok(())
}

#[tokio::test]
async fn a_native_id_colliding_with_a_role_cannot_mint_recipient_capability()
-> Result<(), Box<dyn Error>> {
    let root = TempRoot::new();
    let source = open_installation(&root.0)?;
    let pool = db().await?;
    let owner = source.current_owner().await?;
    query("INSERT INTO roles VALUES(?,X'01921389d31174628f80cb5f79aaee01','role','role','ACTIVE',1,1,1,NULL)").bind(owner.database_bytes().to_vec()).execute(&pool).await?;
    assert_eq!(
        resolve_owner_recipients(&source, &SqliteOwnerRoleReferences::new(pool.clone())).await,
        Err(OwnerIdentityError::IntegrityFailed)
    );
    assert_eq!(
        query_scalar::<_, i64>("SELECT count(*) FROM notifications")
            .fetch_one(&pool)
            .await?,
        0
    );
    Ok(())
}

#[tokio::test]
async fn identity_loss_and_unavailable_storage_fail_without_business_writes()
-> Result<(), Box<dyn Error>> {
    let root = TempRoot::new();
    let source = open_installation(&root.0)?;
    let pool = db().await?;
    let references = SqliteOwnerRoleReferences::new(pool.clone());
    pool.close().await;
    assert_eq!(
        resolve_owner_recipients(&source, &references).await,
        Err(OwnerIdentityError::StorageUnavailable)
    );
    std::fs::remove_file(root.0.join("installation.owner-v1"))?;
    assert_eq!(
        resolve_owner_recipients(&source, &references).await,
        Err(OwnerIdentityError::Unauthenticated)
    );
    Ok(())
}

#[test]
fn identity_errors_are_sanitized_and_use_only_frozen_codes() {
    for (error, code) in [
        (OwnerIdentityError::Unauthenticated, "AUTH_UNAUTHENTICATED"),
        (
            OwnerIdentityError::PermissionDenied,
            "AUTH_PERMISSION_DENIED",
        ),
        (OwnerIdentityError::HumanRequired, "AUTH_HUMAN_REQUIRED"),
        (
            OwnerIdentityError::StorageUnavailable,
            "STORAGE_UNAVAILABLE",
        ),
        (
            OwnerIdentityError::IntegrityFailed,
            "STORAGE_INTEGRITY_FAILED",
        ),
    ] {
        assert_eq!(error.to_string(), code);
        assert_eq!(error.code(), code);
    }
}
