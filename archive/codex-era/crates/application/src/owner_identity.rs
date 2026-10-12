//! ADR-0007 clause 1: trusted installation identity, never an IPC actor claim.
use std::{fmt, future::Future};
use teamspace_domain::{Actor, HumanId};

/// Sanitized failures using the frozen catalogue.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OwnerIdentityError {
    /// No usable installation identity is available.
    Unauthenticated,
    /// A declared recipient differs from the trusted installation identity.
    PermissionDenied,
    /// A Role, Adapter or System claim cannot stand in for a Human.
    HumanRequired,
    /// Local identity storage is inaccessible.
    StorageUnavailable,
    /// The persisted identity is inconsistent or has been replaced.
    IntegrityFailed,
}
impl OwnerIdentityError {
    /// Stable spelling; filesystem diagnostics never cross this boundary.
    pub const fn code(self) -> &'static str {
        match self {
            Self::Unauthenticated => "AUTH_UNAUTHENTICATED",
            Self::PermissionDenied => "AUTH_PERMISSION_DENIED",
            Self::HumanRequired => "AUTH_HUMAN_REQUIRED",
            Self::StorageUnavailable => "STORAGE_UNAVAILABLE",
            Self::IntegrityFailed => "STORAGE_INTEGRITY_FAILED",
        }
    }
}
impl fmt::Display for OwnerIdentityError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.code())
    }
}
impl std::error::Error for OwnerIdentityError {}

/// Implemented only by trusted host infrastructure. Do not select this port
/// or its backing path from IPC/Adapter payloads.
pub trait OwnerIdentity {
    fn current_owner(&self) -> impl Future<Output = Result<HumanId, OwnerIdentityError>> + Send;
}

/// Trusted storage lookup, performed at use time to exclude identity reuse.
pub trait OwnerRoleReferences {
    fn is_role(&self, id: HumanId)
    -> impl Future<Output = Result<bool, OwnerIdentityError>> + Send;
}

/// One identity for both notification and policy references. No Deserialize
/// implementation or constructor accepting an untrusted UUID is exposed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OwnerRecipients(HumanId);
impl OwnerRecipients {
    /// Recipient selected by the host, independent of client payload.
    pub const fn notification_recipient(self) -> HumanId {
        self.0
    }
    /// Policy uses exactly the same installation identity.
    pub const fn policy_recipient(self) -> HumanId {
        self.0
    }
    /// Attribution consistency only, not actor authentication/authorization.
    /// Actual command boundaries must authenticate origin and audit denial.
    /// This does not forbid an authorized Role editing a Policy for its Human
    /// recipient; Policy actor permissions remain a separate frozen rule.
    pub fn check_human_attribution(self, actor: Actor) -> Result<(), OwnerIdentityError> {
        match actor {
            Actor::Human(id) if id == self.0 => Ok(()),
            Actor::Human(_) => Err(OwnerIdentityError::PermissionDenied),
            _ => Err(OwnerIdentityError::HumanRequired),
        }
    }
    /// Validate a stored reference without adopting it as identity evidence.
    pub fn check_recipient(self, id: HumanId) -> Result<(), OwnerIdentityError> {
        if id == self.0 {
            Ok(())
        } else {
            Err(OwnerIdentityError::PermissionDenied)
        }
    }
}

/// Resolve again at use time; callers cannot supply a recipient/allow flag.
pub async fn resolve_owner_recipients<I: OwnerIdentity, R: OwnerRoleReferences>(
    identity: &I,
    references: &R,
) -> Result<OwnerRecipients, OwnerIdentityError> {
    let owner = identity.current_owner().await?;
    if references.is_role(owner).await? {
        return Err(OwnerIdentityError::IntegrityFailed);
    }
    Ok(OwnerRecipients(owner))
}
