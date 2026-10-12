//! Return corrupted DTOs after real SQLite validation, without corrupting stored facts.
//! The call trail distinguishes an Application rejection from a later SQL refusal.
use application::audit_consumer::{
    AuditDelivery, AuditError, AuditEvent, AuditReceipt, AuditState, AuditStore, AuditTransaction,
};
use persistence_sqlite::audit_consumer::{SqliteAuditStore, SqliteAuditTransaction};
use std::sync::{Arc, Mutex};
use teamspace_domain::TeamId;

#[derive(Clone, Copy)]
pub enum Fault {
    None,
    CrossTeam(TeamId),
    ReceiptHash,
}
pub struct FaultyStore {
    inner: SqliteAuditStore,
    fault: Fault,
    calls: Arc<Mutex<Vec<&'static str>>>,
}
impl FaultyStore {
    pub fn new(inner: SqliteAuditStore, fault: Fault) -> Self {
        Self {
            inner,
            fault,
            calls: Arc::new(Mutex::new(Vec::new())),
        }
    }
    pub fn calls(&self) -> Vec<&'static str> {
        self.calls.lock().unwrap().clone()
    }
}
pub struct FaultyTransaction {
    inner: SqliteAuditTransaction,
    fault: Fault,
    calls: Arc<Mutex<Vec<&'static str>>>,
}
impl FaultyTransaction {
    fn record(&self, name: &'static str) {
        self.calls.lock().unwrap().push(name);
    }
}
impl AuditStore for FaultyStore {
    type Transaction = FaultyTransaction;
    async fn begin(&self) -> Result<Self::Transaction, AuditError> {
        let inner = self.inner.begin().await?;
        self.calls.lock().unwrap().push("begin");
        Ok(FaultyTransaction {
            inner,
            fault: self.fault,
            calls: self.calls.clone(),
        })
    }
}
impl AuditTransaction for FaultyTransaction {
    async fn load_event(
        &mut self,
        delivery: AuditDelivery,
    ) -> Result<Option<AuditEvent>, AuditError> {
        self.record("load_event");
        let mut event = self.inner.load_event(delivery).await?;
        if let (Fault::CrossTeam(team), Some(event)) = (self.fault, event.as_mut()) {
            assert_ne!(event.team_id, team, "fault must actually alter Team");
            event.team_id = team;
        }
        Ok(event)
    }
    async fn state(&mut self, event: AuditEvent) -> Result<AuditState, AuditError> {
        self.record("state");
        let state = self.inner.state(event).await?;
        if let Fault::ReceiptHash = self.fault {
            let AuditState::Consumed(mut receipt) = state else {
                panic!("receipt fault requires a real previously consumed Event");
            };
            assert_eq!(receipt.event, event, "valid receipt before injection");
            receipt.event.hash[0] ^= 1;
            assert_ne!(receipt.event, event, "fault must actually change receipt");
            return Ok(AuditState::Consumed(receipt));
        }
        Ok(state)
    }
    async fn record_consumption(&mut self, receipt: AuditReceipt) -> Result<(), AuditError> {
        self.record("record_consumption");
        self.inner.record_consumption(receipt).await
    }
    async fn write_projection(&mut self, receipt: AuditReceipt) -> Result<(), AuditError> {
        self.record("write_projection");
        self.inner.write_projection(receipt).await
    }
    async fn commit(self) -> Result<(), AuditError> {
        self.record("commit");
        self.inner.commit().await
    }
    async fn rollback(self) -> Result<(), AuditError> {
        self.record("rollback");
        self.inner.rollback().await
    }
}
