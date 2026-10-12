//! DEV-02-07: fixed internal System consumer; no actor or destination input.
//! Only validated expiry Events enter this projection. Dispatch claims and
//! publication acknowledgement belong to DEV-02-14, outside this transaction.
use std::{fmt, future::Future};
use teamspace_domain::{
    Clock, EventId, MessageId, RequestId, TeamId, UtcTimestamp, UuidV7, Version,
};

/// Host-owned delivery reference, never a caller-provided Event body.
#[derive(Clone, Copy, Debug)]
pub struct AuditDelivery {
    pub team_id: TeamId,
    pub outbox_id: UuidV7,
}
/// Whitelisted, hash-verified Event facts. No arbitrary payload or metadata.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AuditEvent {
    pub event_id: EventId,
    pub team_id: TeamId,
    pub message_id: MessageId,
    pub request_id: RequestId,
    pub object_version: Version,
    pub occurred_at: UtcTimestamp,
    pub hash: [u8; 32],
}
/// Durable first receipt, also used when rebuilding a missing read model.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AuditReceipt {
    pub event: AuditEvent,
    pub consumed_at: UtcTimestamp,
}
/// Projection effect; every variant preserves the same first receipt.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AuditOutcome {
    Applied(AuditReceipt),
    Duplicate(AuditReceipt),
    Rebuilt(AuditReceipt),
}
impl AuditOutcome {
    /// First persisted facts, independent of delivery attempts or replay time.
    pub const fn receipt(self) -> AuditReceipt {
        match self {
            Self::Applied(r) | Self::Duplicate(r) | Self::Rebuilt(r) => r,
        }
    }
}
/// Sanitized frozen catalogue errors; no DB/JSON text reaches the host caller.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AuditError {
    ObjectNotFound,
    UnsupportedEvent,
    IntegrityFailed,
    StorageUnavailable,
}
impl AuditError {
    /// Stable code; a valid but unsupported historical format is not corruption.
    pub const fn code(self) -> &'static str {
        match self {
            Self::ObjectNotFound => "DOMAIN_OBJECT_NOT_FOUND",
            Self::UnsupportedEvent => "DOMAIN_PRECONDITION_FAILED",
            Self::IntegrityFailed => "STORAGE_INTEGRITY_FAILED",
            Self::StorageUnavailable => "STORAGE_UNAVAILABLE",
        }
    }
}
impl fmt::Display for AuditError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.code())
    }
}
impl std::error::Error for AuditError {}
/// State of this one consumer/event pair, loaded within the write transaction.
pub enum AuditState {
    Unconsumed,
    Consumed(AuditReceipt),
    MissingProjection { consumed_at: UtcTimestamp },
}
/// Infrastructure supplies a transaction; Application decides its lifecycle.
pub trait AuditStore {
    type Transaction: AuditTransaction;
    fn begin(&self) -> impl Future<Output = Result<Self::Transaction, AuditError>> + Send;
}
/// Fixed projection ports. load_event verifies source hash and association;
/// state verifies the consumer's durable key, scope and first consumed_at.
pub trait AuditTransaction {
    fn load_event(
        &mut self,
        delivery: AuditDelivery,
    ) -> impl Future<Output = Result<Option<AuditEvent>, AuditError>> + Send;
    fn state(
        &mut self,
        event: AuditEvent,
    ) -> impl Future<Output = Result<AuditState, AuditError>> + Send;
    fn record_consumption(
        &mut self,
        receipt: AuditReceipt,
    ) -> impl Future<Output = Result<(), AuditError>> + Send;
    fn write_projection(
        &mut self,
        receipt: AuditReceipt,
    ) -> impl Future<Output = Result<(), AuditError>> + Send;
    fn commit(self) -> impl Future<Output = Result<(), AuditError>> + Send;
    fn rollback(self) -> impl Future<Output = Result<(), AuditError>> + Send;
}
/// Atomically projects a validated source and records its first consumption.
/// A missing projection rebuild never deletes or resets the consumer ledger.
pub async fn consume_audit<S: AuditStore, C: Clock>(
    store: &S,
    clock: &C,
    delivery: AuditDelivery,
) -> Result<AuditOutcome, AuditError> {
    let mut tx = store.begin().await?;
    match consume(&mut tx, clock, delivery).await {
        Ok(result) => {
            tx.commit().await?;
            Ok(result)
        }
        Err(error) => {
            tx.rollback().await?;
            Err(error)
        }
    }
}
async fn consume<T: AuditTransaction, C: Clock>(
    tx: &mut T,
    clock: &C,
    delivery: AuditDelivery,
) -> Result<AuditOutcome, AuditError> {
    let event = tx
        .load_event(delivery)
        .await?
        .ok_or(AuditError::ObjectNotFound)?;
    if event.team_id != delivery.team_id {
        return Err(AuditError::IntegrityFailed);
    }
    match tx.state(event).await? {
        AuditState::Consumed(receipt) => {
            if receipt.event != event {
                return Err(AuditError::IntegrityFailed);
            }
            Ok(AuditOutcome::Duplicate(receipt))
        }
        AuditState::MissingProjection { consumed_at } => {
            let receipt = AuditReceipt { event, consumed_at };
            tx.write_projection(receipt).await?;
            Ok(AuditOutcome::Rebuilt(receipt))
        }
        AuditState::Unconsumed => {
            let receipt = AuditReceipt {
                event,
                consumed_at: clock.now(),
            };
            tx.record_consumption(receipt).await?;
            tx.write_projection(receipt).await?;
            Ok(AuditOutcome::Applied(receipt))
        }
    }
}
