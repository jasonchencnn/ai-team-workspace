//! Storage-side identity exclusion; the Owner itself comes from the native port.
use application::owner_identity::{OwnerIdentityError, OwnerRoleReferences};
use sqlx_core::query_scalar::query_scalar;
use sqlx_sqlite::SqlitePool;
use teamspace_domain::HumanId;

/// Host-owned lookup capability. No client controls the pool or Owner identity.
pub struct SqliteOwnerRoleReferences {
    pool: SqlitePool,
}
impl SqliteOwnerRoleReferences {
    /// Uses the host's migrated database with foreign keys enabled.
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }
}
impl OwnerRoleReferences for SqliteOwnerRoleReferences {
    async fn is_role(&self, id: HumanId) -> Result<bool, OwnerIdentityError> {
        query_scalar("SELECT EXISTS(SELECT 1 FROM roles WHERE id=?)")
            .bind(id.database_bytes().to_vec())
            .fetch_one(&self.pool)
            .await
            .map_err(|_| OwnerIdentityError::StorageUnavailable)
    }
}
