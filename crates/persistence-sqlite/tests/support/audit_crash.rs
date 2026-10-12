//! Test-only pause adapter around the production SQLite ports. Readiness is
//! derived from actual transaction state, then the parent kills this process.
use application::audit_consumer::{
    AuditDelivery, AuditError, AuditEvent, AuditReceipt, AuditState, AuditStore, AuditTransaction,
};
use persistence_sqlite::audit_consumer::{SqliteAuditStore, SqliteAuditTransaction};
use sqlx_core::query_scalar::query_scalar;
use sqlx_sqlite::SqlitePool;
use std::path::PathBuf;

pub struct PausedStore {
    pub pool: SqlitePool,
    pub phase: String,
    pub ready: PathBuf,
}
pub struct PausedTransaction {
    inner: SqliteAuditTransaction,
    pool: SqlitePool,
    phase: String,
    ready: PathBuf,
}
async fn wait_for_kill(ready: &std::path::Path, state: String) -> Result<(), AuditError> {
    std::fs::write(ready, state).map_err(|_| AuditError::StorageUnavailable)?;
    std::future::pending::<()>().await;
    Ok(())
}
impl AuditStore for PausedStore {
    type Transaction = PausedTransaction;
    async fn begin(&self) -> Result<Self::Transaction, AuditError> {
        Ok(PausedTransaction {
            inner: SqliteAuditStore::new(self.pool.clone()).begin().await?,
            pool: self.pool.clone(),
            phase: self.phase.clone(),
            ready: self.ready.clone(),
        })
    }
}
impl PausedTransaction {
    async fn pause_after_write(&mut self, receipt: AuditReceipt) -> Result<(), AuditError> {
        let state = self.inner.state(receipt.event).await?;
        let evidence = match state {
            AuditState::MissingProjection { consumed_at } => format!(
                "marker=1;projection=0;consumed_at={}",
                consumed_at.unix_millis()
            ),
            AuditState::Consumed(stored) => format!(
                "marker=1;projection=1;consumed_at={}",
                stored.consumed_at.unix_millis()
            ),
            AuditState::Unconsumed => return Err(AuditError::IntegrityFailed),
        };
        wait_for_kill(&self.ready, evidence).await
    }
}
impl AuditTransaction for PausedTransaction {
    async fn load_event(&mut self, d: AuditDelivery) -> Result<Option<AuditEvent>, AuditError> {
        self.inner.load_event(d).await
    }
    async fn state(&mut self, e: AuditEvent) -> Result<AuditState, AuditError> {
        self.inner.state(e).await
    }
    async fn record_consumption(&mut self, r: AuditReceipt) -> Result<(), AuditError> {
        self.inner.record_consumption(r).await?;
        if self.phase == "marker" {
            self.pause_after_write(r).await?;
        }
        Ok(())
    }
    async fn write_projection(&mut self, r: AuditReceipt) -> Result<(), AuditError> {
        self.inner.write_projection(r).await?;
        if self.phase == "projection" {
            self.pause_after_write(r).await?;
        }
        Ok(())
    }
    async fn commit(self) -> Result<(), AuditError> {
        self.inner.commit().await?;
        if self.phase == "commit" {
            let markers: i64 = query_scalar(
                "SELECT COUNT(*) FROM event_consumptions WHERE consumer_id='audit-projection-v1'",
            )
            .fetch_one(&self.pool)
            .await
            .map_err(|_| AuditError::StorageUnavailable)?;
            let projections: i64 = query_scalar("SELECT COUNT(*) FROM audit_expiry_projection")
                .fetch_one(&self.pool)
                .await
                .map_err(|_| AuditError::StorageUnavailable)?;
            let time:i64=query_scalar("SELECT consumed_at FROM event_consumptions WHERE consumer_id='audit-projection-v1'").fetch_one(&self.pool).await.map_err(|_|AuditError::StorageUnavailable)?;
            wait_for_kill(
                &self.ready,
                format!("marker={markers};projection={projections};consumed_at={time}"),
            )
            .await?;
        }
        Ok(())
    }
    async fn rollback(self) -> Result<(), AuditError> {
        self.inner.rollback().await
    }
}
