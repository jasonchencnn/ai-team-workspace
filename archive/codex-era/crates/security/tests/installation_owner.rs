#![cfg(unix)]
use application::owner_identity::{
    OwnerIdentity, OwnerIdentityError, OwnerRoleReferences, resolve_owner_recipients,
};
use security::installation_owner::open_installation;
use std::{error::Error, path::PathBuf};

struct TempRoot(PathBuf);
struct NoRoles;
impl OwnerRoleReferences for NoRoles {
    async fn is_role(&self, _: teamspace_domain::HumanId) -> Result<bool, OwnerIdentityError> {
        Ok(false)
    }
}
impl TempRoot {
    fn new() -> Self {
        use std::sync::atomic::{AtomicUsize, Ordering};
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        Self(std::env::temp_dir().join(format!(
            "teamspace-owner-{}-{}",
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

#[tokio::test]
async fn missing_record_never_regenerates_the_installation_identity() -> Result<(), Box<dyn Error>>
{
    let root = TempRoot::new();
    let source = open_installation(&root.0)?;
    let marker = std::fs::read(root.0.join("installation.identity-marker"))?;
    std::fs::remove_file(root.0.join("installation.owner-v1"))?;
    assert_eq!(
        source.current_owner().await,
        Err(OwnerIdentityError::Unauthenticated)
    );
    assert!(open_installation(&root.0).is_err());
    assert!(!root.0.join("installation.owner-v1").exists());
    assert_eq!(
        std::fs::read(root.0.join("installation.identity-marker"))?,
        marker
    );
    Ok(())
}

#[tokio::test]
async fn invalid_record_shapes_and_headers_are_rejected_without_repair()
-> Result<(), Box<dyn Error>> {
    let root = TempRoot::new();
    let source = open_installation(&root.0)?;
    let path = root.0.join("installation.owner-v1");
    let original = std::fs::read(&path)?;
    let mut wrong_header = original.clone();
    wrong_header[0] = b'X';
    let mut wrong_uuid = original.clone();
    wrong_uuid[14] = 0;
    for bytes in [
        vec![],
        original[..23].to_vec(),
        [original.clone(), vec![0]].concat(),
        wrong_header,
        wrong_uuid,
    ] {
        std::fs::write(&path, &bytes)?;
        assert_eq!(
            source.current_owner().await,
            Err(OwnerIdentityError::IntegrityFailed)
        );
        assert!(open_installation(&root.0).is_err());
        assert_eq!(std::fs::read(&path)?, bytes);
    }
    Ok(())
}

#[tokio::test]
async fn live_replacement_is_rejected_even_when_bytes_are_identical() -> Result<(), Box<dyn Error>>
{
    use std::os::unix::fs::PermissionsExt;
    let root = TempRoot::new();
    let source = open_installation(&root.0)?;
    let path = root.0.join("installation.owner-v1");
    let original = std::fs::read(&path)?;
    let replacement = root.0.join("replacement");
    std::fs::write(&replacement, original)?;
    std::fs::set_permissions(&replacement, std::fs::Permissions::from_mode(0o600))?;
    std::fs::rename(&replacement, &path)?;
    assert_eq!(
        source.current_owner().await,
        Err(OwnerIdentityError::IntegrityFailed)
    );
    Ok(())
}

#[tokio::test]
async fn different_valid_uuid_cannot_replace_a_live_owner() -> Result<(), Box<dyn Error>> {
    let root = TempRoot::new();
    let source = open_installation(&root.0)?;
    let path = root.0.join("installation.owner-v1");
    let mut bytes = std::fs::read(&path)?;
    bytes[23] ^= 1;
    assert!(teamspace_domain::HumanId::from_database_bytes(bytes[8..].try_into()?).is_ok());
    std::fs::write(&path, bytes)?;
    assert_eq!(
        source.current_owner().await,
        Err(OwnerIdentityError::IntegrityFailed)
    );
    Ok(())
}

#[tokio::test]
async fn symlink_and_group_readable_records_are_not_trusted() -> Result<(), Box<dyn Error>> {
    use std::os::unix::fs::{PermissionsExt, symlink};
    let root = TempRoot::new();
    let source = open_installation(&root.0)?;
    let path = root.0.join("installation.owner-v1");
    let target = root.0.join("original");
    std::fs::rename(&path, &target)?;
    symlink(&target, &path)?;
    assert_eq!(
        source.current_owner().await,
        Err(OwnerIdentityError::IntegrityFailed)
    );
    assert!(open_installation(&root.0).is_err());
    std::fs::remove_file(&path)?;
    std::fs::rename(&target, &path)?;
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o640))?;
    assert_eq!(
        source.current_owner().await,
        Err(OwnerIdentityError::IntegrityFailed)
    );
    Ok(())
}

#[test]
fn existing_data_and_incomplete_bootstrap_are_not_adopted() -> Result<(), Box<dyn Error>> {
    use std::os::unix::fs::{DirBuilderExt, PermissionsExt};
    let root = TempRoot::new();
    std::fs::DirBuilder::new().mode(0o700).create(&root.0)?;
    let database = root.0.join("existing.sqlite");
    std::fs::write(&database, b"legacy")?;
    assert!(open_installation(&root.0).is_err());
    assert!(!root.0.join("installation.identity-marker").exists());
    std::fs::remove_file(&database)?;
    let marker = root.0.join("installation.identity-marker");
    std::fs::write(&marker, b"TSINIT1\n")?;
    std::fs::set_permissions(&marker, std::fs::Permissions::from_mode(0o600))?;
    assert!(open_installation(&root.0).is_err());
    assert!(!root.0.join("installation.owner-v1").exists());
    Ok(())
}

#[tokio::test]
async fn marker_tampering_and_insecure_or_symlink_root_fail_closed() -> Result<(), Box<dyn Error>> {
    use std::os::unix::fs::{PermissionsExt, symlink};
    let root = TempRoot::new();
    let source = open_installation(&root.0)?;
    let marker = root.0.join("installation.identity-marker");
    std::fs::write(&marker, b"INVALID\n")?;
    assert_eq!(
        source.current_owner().await,
        Err(OwnerIdentityError::IntegrityFailed)
    );
    std::fs::write(&marker, b"TSINIT1\n")?;
    std::fs::set_permissions(&root.0, std::fs::Permissions::from_mode(0o750))?;
    assert!(open_installation(&root.0).is_err());
    std::fs::set_permissions(&root.0, std::fs::Permissions::from_mode(0o700))?;
    let alias = TempRoot::new();
    symlink(&root.0, &alias.0)?;
    assert!(open_installation(&alias.0).is_err());
    std::fs::remove_file(&alias.0)?;
    Ok(())
}

#[test]
fn racing_startups_never_return_two_different_owners() -> Result<(), Box<dyn Error>> {
    use std::{
        sync::{Arc, Barrier},
        thread,
    };
    let root = TempRoot::new();
    let barrier = Arc::new(Barrier::new(2));
    let mut threads = Vec::new();
    for _ in 0..2 {
        let path = root.0.clone();
        let barrier = barrier.clone();
        threads.push(thread::spawn(move || {
            barrier.wait();
            open_installation(&path)
        }));
    }
    let sources = threads
        .into_iter()
        .map(|t| t.join().expect("startup thread"))
        .filter_map(Result::ok)
        .collect::<Vec<_>>();
    assert!(!sources.is_empty(), "at least one initializer must succeed");
    let runtime = tokio::runtime::Runtime::new()?;
    let winner = runtime.block_on(open_installation(&root.0)?.current_owner())?;
    for source in sources {
        assert_eq!(runtime.block_on(source.current_owner())?, winner);
    }
    Ok(())
}

#[tokio::test]
async fn native_initialization_survives_reopening_with_one_shared_recipient()
-> Result<(), Box<dyn Error>> {
    let root = TempRoot::new();
    let first = open_installation(&root.0)?;
    let owner = first.current_owner().await?;
    let reopened = open_installation(&root.0)?;
    assert_eq!(reopened.current_owner().await?, owner);
    let recipients = resolve_owner_recipients(&reopened, &NoRoles).await?;
    assert_eq!(recipients.notification_recipient(), owner);
    assert_eq!(recipients.policy_recipient(), owner);
    Ok(())
}

#[tokio::test]
async fn losing_both_records_cannot_rebootstrap_an_existing_empty_directory()
-> Result<(), Box<dyn Error>> {
    let root = TempRoot::new();
    let source = open_installation(&root.0)?;
    std::fs::remove_file(root.0.join("installation.owner-v1"))?;
    std::fs::remove_file(root.0.join("installation.identity-marker"))?;
    assert_eq!(
        source.current_owner().await,
        Err(OwnerIdentityError::Unauthenticated)
    );
    assert!(open_installation(&root.0).is_err());
    assert_eq!(
        std::fs::read_dir(&root.0)?.count(),
        0,
        "no replacement identity may appear"
    );
    Ok(())
}
