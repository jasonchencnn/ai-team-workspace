//! Native installation identity; never take backing paths or IDs from IPC.
use application::owner_identity::{OwnerIdentity, OwnerIdentityError};
use std::path::{Path, PathBuf};
use teamspace_domain::HumanId;

const MARKER: &str = "installation.identity-marker";
const RECORD: &str = "installation.owner-v1";
const MARKER_BYTES: &[u8; 8] = b"TSINIT1\n";
const HEADER: &[u8; 8] = b"TSOWN01\n";

/// Private native source, revalidated at use. Not an IPC credential or DTO.
#[derive(Debug, Clone)]
pub struct InstallationOwner {
    root: PathBuf,
    owner: HumanId,
    stamp: Stamp,
}
type FileStamp = (u64, u64, u32);
type FileRecord = (Vec<u8>, FileStamp);
#[derive(Debug, Clone, PartialEq, Eq)]
struct Stamp(Vec<FileStamp>);
impl OwnerIdentity for InstallationOwner {
    async fn current_owner(&self) -> Result<HumanId, OwnerIdentityError> {
        let source = self.clone();
        tokio::task::spawn_blocking(move || {
            let (owner, stamp) = load(&source.root)?;
            if owner != source.owner || stamp != source.stamp {
                return Err(OwnerIdentityError::IntegrityFailed);
            }
            Ok(owner)
        })
        .await
        .map_err(|_| OwnerIdentityError::StorageUnavailable)?
    }
}
/// Native startup only: first initialize an empty installation, then load.
/// Existing data without identity, interrupted bootstrap or record loss fails
/// closed. No adoption of historical notification IDs, Role IDs or usernames.
pub fn open_installation(root: &Path) -> Result<InstallationOwner, OwnerIdentityError> {
    let new_root = platform::ensure_root(root)?;
    if !root.join(MARKER).try_exists().map_err(io_error)? {
        if !new_root {
            return Err(OwnerIdentityError::Unauthenticated);
        }
        if std::fs::read_dir(root).map_err(io_error)?.next().is_some() {
            return Err(OwnerIdentityError::Unauthenticated);
        }
        let owner = platform::new_owner()?;
        // Exclusive durable marker distinguishes first launch from identity loss.
        // A racing launch either loads the winner or fails on incomplete data.
        match platform::create_private(&root.join(MARKER), MARKER_BYTES) {
            Ok(()) => {
                let mut bytes = HEADER.to_vec();
                bytes.extend_from_slice(&owner.database_bytes());
                platform::create_private(&root.join(RECORD), &bytes).map_err(io_error)?;
                std::fs::File::open(root)
                    .and_then(|dir| dir.sync_all())
                    .map_err(io_error)?;
            }
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {}
            Err(e) => return Err(io_error(e)),
        }
    }
    let (owner, stamp) = load(root)?;
    Ok(InstallationOwner {
        root: root.to_owned(),
        owner,
        stamp,
    })
}
fn io_error(error: std::io::Error) -> OwnerIdentityError {
    match error.kind() {
        std::io::ErrorKind::NotFound => OwnerIdentityError::Unauthenticated,
        std::io::ErrorKind::InvalidData => OwnerIdentityError::IntegrityFailed,
        _ => OwnerIdentityError::StorageUnavailable,
    }
}
fn load(root: &Path) -> Result<(HumanId, Stamp), OwnerIdentityError> {
    let root_stamp = platform::root_stamp(root)?;
    let (marker, marker_stamp) = platform::read_private(&root.join(MARKER), 8, root_stamp.2)?;
    let (record, record_stamp) = platform::read_private(&root.join(RECORD), 24, root_stamp.2)?;
    if marker.as_slice() != MARKER_BYTES
        || record[..8] != HEADER[..]
        || platform::root_stamp(root)? != root_stamp
    {
        return Err(OwnerIdentityError::IntegrityFailed);
    }
    let bytes: [u8; 16] = record[8..]
        .try_into()
        .map_err(|_| OwnerIdentityError::IntegrityFailed)?;
    let owner =
        HumanId::from_database_bytes(bytes).map_err(|_| OwnerIdentityError::IntegrityFailed)?;
    Ok((owner, Stamp(vec![root_stamp, marker_stamp, record_stamp])))
}
#[cfg(unix)]
mod platform {
    use super::*;
    use std::{
        fs::{self, DirBuilder, File, OpenOptions},
        io::{Read, Write},
        os::unix::fs::{DirBuilderExt, MetadataExt, OpenOptionsExt},
        time::{SystemTime, UNIX_EPOCH},
    };
    use teamspace_domain::{Clock, RandomSource, UtcTimestamp, UuidV7Generator, UuidV7Source};
    pub(super) fn ensure_root(root: &Path) -> Result<bool, OwnerIdentityError> {
        let created = match fs::symlink_metadata(root) {
            Ok(_) => false,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                match DirBuilder::new().recursive(true).mode(0o700).create(root) {
                    Ok(()) => {}
                    Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {}
                    Err(e) => return Err(io_error(e)),
                }
                true
            }
            Err(e) => return Err(io_error(e)),
        };
        root_stamp(root).map(|_| created)
    }
    pub(super) fn root_stamp(root: &Path) -> Result<FileStamp, OwnerIdentityError> {
        let meta = fs::symlink_metadata(root).map_err(io_error)?;
        if !meta.file_type().is_dir() || meta.mode() & 0o077 != 0 {
            return Err(OwnerIdentityError::IntegrityFailed);
        }
        Ok((meta.dev(), meta.ino(), meta.uid()))
    }
    pub(super) fn create_private(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
            .open(path)?;
        file.write_all(bytes)?;
        file.sync_all()
    }
    pub(super) fn read_private(
        path: &Path,
        length: usize,
        uid: u32,
    ) -> Result<FileRecord, OwnerIdentityError> {
        let before = fs::symlink_metadata(path).map_err(io_error)?;
        if !before.file_type().is_file()
            || before.mode() & 0o077 != 0
            || before.uid() != uid
            || before.len() != length as u64
        {
            return Err(OwnerIdentityError::IntegrityFailed);
        }
        let mut file = OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
            .open(path)
            .map_err(io_error)?;
        let meta = file.metadata().map_err(io_error)?;
        if !meta.file_type().is_file()
            || meta.dev() != before.dev()
            || meta.ino() != before.ino()
            || meta.uid() != uid
            || meta.mode() & 0o077 != 0
        {
            return Err(OwnerIdentityError::IntegrityFailed);
        }
        let mut bytes = vec![0; length];
        file.read_exact(&mut bytes)
            .map_err(|_| OwnerIdentityError::IntegrityFailed)?;
        let mut extra = [0];
        if file.read(&mut extra).map_err(io_error)? != 0 {
            return Err(OwnerIdentityError::IntegrityFailed);
        }
        let after = fs::symlink_metadata(path).map_err(io_error)?;
        if meta.dev() != after.dev()
            || meta.ino() != after.ino()
            || after.uid() != uid
            || after.mode() & 0o077 != 0
        {
            return Err(OwnerIdentityError::IntegrityFailed);
        }
        Ok((bytes, (meta.dev(), meta.ino(), meta.uid())))
    }
    struct NativeClock(UtcTimestamp);
    impl Clock for NativeClock {
        fn now(&self) -> UtcTimestamp {
            self.0
        }
    }
    struct OsRandom(File);
    impl RandomSource for OsRandom {
        type Error = std::io::Error;
        fn fill_bytes(&mut self, bytes: &mut [u8]) -> Result<(), Self::Error> {
            self.0.read_exact(bytes)
        }
    }
    pub(super) fn new_owner() -> Result<HumanId, OwnerIdentityError> {
        let millis = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|_| OwnerIdentityError::StorageUnavailable)?
            .as_millis();
        let clock = NativeClock(
            UtcTimestamp::from_unix_millis(
                i64::try_from(millis).map_err(|_| OwnerIdentityError::IntegrityFailed)?,
            )
            .map_err(|_| OwnerIdentityError::IntegrityFailed)?,
        );
        let mut random = OsRandom(File::open("/dev/urandom").map_err(io_error)?);
        let id = UuidV7Generator::new(&clock, &mut random)
            .next_uuid_v7()
            .map_err(|_| OwnerIdentityError::StorageUnavailable)?;
        Ok(HumanId::from_uuid(id))
    }
}
#[cfg(not(unix))]
mod platform {
    use super::*;
    pub(super) fn ensure_root(_: &Path) -> Result<bool, OwnerIdentityError> {
        Err(OwnerIdentityError::StorageUnavailable)
    }
    pub(super) fn root_stamp(_: &Path) -> Result<FileStamp, OwnerIdentityError> {
        Err(OwnerIdentityError::StorageUnavailable)
    }
    pub(super) fn read_private(
        _: &Path,
        _: usize,
        _: u32,
    ) -> Result<FileRecord, OwnerIdentityError> {
        Err(OwnerIdentityError::StorageUnavailable)
    }
    pub(super) fn create_private(_: &Path, _: &[u8]) -> std::io::Result<()> {
        Err(std::io::Error::from(std::io::ErrorKind::Unsupported))
    }
    pub(super) fn new_owner() -> Result<HumanId, OwnerIdentityError> {
        Err(OwnerIdentityError::StorageUnavailable)
    }
}
