//! DEV-02-07: narrow, internal System expiry command; never an IPC actor claim.
//! PENDING attempts fail closed until the frozen attempt/expiry discrepancy is
//! resolved. No dispatch, notification policy, or arbitrary mutation API.

use std::{fmt, future::Future};
use teamspace_domain::{
    Clock, EventId, ExpectedVersion, MessageCommand, MessageId, MessageStatus, RequestId, TeamId,
    UtcTimestamp, UuidV7, UuidV7Source, Version, idempotency::IdempotencyKeyString,
    message_transition,
};

/// Internal scheduler input. The host supplies clock and ID capabilities.
#[derive(Debug, Clone, Copy)]
pub struct ExpiryRequest {
    pub team_id: TeamId,
    pub message_id: MessageId,
    pub request_id: RequestId,
    pub expected_version: ExpectedVersion,
    pub idempotency_key: IdempotencyKeyString,
}
/// The immutable first successful command result.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ExpiryResult {
    pub message_id: MessageId,
    pub event_id: EventId,
    pub version: Version,
}
/// Stable, sanitized errors; infrastructure text never crosses this boundary.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExpiryError {
    InvalidField,
    ObjectNotFound,
    TeamArchived,
    InvalidTransition,
    PreconditionsFailed,
    VersionConflict { current_version: Version },
    IdempotencyConflict,
    Busy,
    StorageUnavailable,
    IntegrityFailed,
}
impl ExpiryError {
    /// Frozen error catalogue spelling.
    pub const fn code(self) -> &'static str {
        match self {
            Self::InvalidField => "VALIDATION_INVALID_FIELD",
            Self::ObjectNotFound => "DOMAIN_OBJECT_NOT_FOUND",
            Self::TeamArchived => "DOMAIN_TEAM_ARCHIVED",
            Self::InvalidTransition => "DOMAIN_INVALID_TRANSITION",
            Self::PreconditionsFailed => "DOMAIN_PRECONDITION_FAILED",
            Self::VersionConflict { .. } => "CONCURRENCY_VERSION_CONFLICT",
            Self::IdempotencyConflict => "CONCURRENCY_IDEMPOTENCY_CONFLICT",
            Self::Busy => "CONCURRENCY_RESOURCE_BUSY",
            Self::StorageUnavailable => "STORAGE_UNAVAILABLE",
            Self::IntegrityFailed => "STORAGE_INTEGRITY_FAILED",
        }
    }
}
impl fmt::Display for ExpiryError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.code())
    }
}
impl std::error::Error for ExpiryError {}

/// Current aggregate facts, loaded within the write transaction.
pub struct ExpirySnapshot {
    pub status: MessageStatus,
    pub version: Version,
    pub expires_at: Option<UtcTimestamp>,
    pub has_pending_attempt: bool,
}
/// Persistence port. Starting/committing/rolling back is decided by Application.
pub trait ExpiryStore {
    type Transaction: ExpiryTransaction;
    fn begin(&self) -> impl Future<Output = Result<Self::Transaction, ExpiryError>> + Send;
}
/// Operations for this command only; no Event update/delete or arbitrary SQL.
pub trait ExpiryTransaction {
    fn team_state(
        &mut self,
        team: TeamId,
    ) -> impl Future<Output = Result<Option<String>, ExpiryError>> + Send;
    fn replay(
        &mut self,
        request: &ExpiryRequest,
    ) -> impl Future<Output = Result<Option<Result<ExpiryResult, ExpiryError>>, ExpiryError>> + Send;
    fn message(
        &mut self,
        request: &ExpiryRequest,
    ) -> impl Future<Output = Result<Option<ExpirySnapshot>, ExpiryError>> + Send;
    fn claim(
        &mut self,
        request: &ExpiryRequest,
        now: UtcTimestamp,
    ) -> impl Future<Output = Result<(), ExpiryError>> + Send;
    fn write_message(
        &mut self,
        request: &ExpiryRequest,
        version: Version,
        now: UtcTimestamp,
    ) -> impl Future<Output = Result<(), ExpiryError>> + Send;
    fn abandon_in_flight(
        &mut self,
        request: &ExpiryRequest,
        now: UtcTimestamp,
    ) -> impl Future<Output = Result<(), ExpiryError>> + Send;
    fn append_event(
        &mut self,
        request: &ExpiryRequest,
        result: ExpiryResult,
        previous_version: Version,
        now: UtcTimestamp,
    ) -> impl Future<Output = Result<(), ExpiryError>> + Send;
    fn enqueue_audit(
        &mut self,
        request: &ExpiryRequest,
        result: ExpiryResult,
        outbox_id: UuidV7,
        now: UtcTimestamp,
    ) -> impl Future<Output = Result<(), ExpiryError>> + Send;
    fn finish(
        &mut self,
        request: &ExpiryRequest,
        outcome: Result<ExpiryResult, ExpiryError>,
        now: UtcTimestamp,
    ) -> impl Future<Output = Result<(), ExpiryError>> + Send;
    fn commit(self) -> impl Future<Output = Result<(), ExpiryError>> + Send;
    fn rollback(self) -> impl Future<Output = Result<(), ExpiryError>> + Send;
}

/// Executes the scheduler's fixed System action with one atomic commit.
/// Replay precedes current-state checks; request IDs/time/generated IDs are
/// not payload. Stable business failures retain their first details. Storage
/// failures roll back the idempotency claim together with all business writes.
pub async fn expire_message<S: ExpiryStore, C: Clock, I: UuidV7Source>(
    store: &S,
    clock: &C,
    ids: &mut I,
    request: ExpiryRequest,
) -> Result<ExpiryResult, ExpiryError> {
    if request.idempotency_key.as_str().trim().is_empty() {
        return Err(ExpiryError::InvalidField);
    }
    let mut tx = store.begin().await?;
    match execute(&mut tx, clock, ids, &request).await {
        Ok(outcome) => {
            tx.commit().await?;
            outcome
        }
        Err(error) => {
            tx.rollback().await?;
            Err(error)
        }
    }
}

async fn execute<T: ExpiryTransaction, C: Clock, I: UuidV7Source>(
    tx: &mut T,
    clock: &C,
    ids: &mut I,
    request: &ExpiryRequest,
) -> Result<Result<ExpiryResult, ExpiryError>, ExpiryError> {
    let Some(team_state) = tx.team_state(request.team_id).await? else {
        return Ok(Err(ExpiryError::ObjectNotFound));
    };
    if let Some(outcome) = tx.replay(request).await? {
        return Ok(outcome);
    }
    let now = clock.now();
    let snapshot = tx.message(request).await?;
    tx.claim(request, now).await?;
    let decision = decide(&team_state, snapshot.as_ref(), request, now);
    let version = match decision {
        Ok(version) => version,
        Err(ExpiryError::IntegrityFailed) => return Err(ExpiryError::IntegrityFailed),
        Err(error) => {
            tx.finish(request, Err(error), now).await?;
            return Ok(Err(error));
        }
    };
    let event_id = EventId::from_uuid(
        ids.next_uuid_v7()
            .map_err(|_| ExpiryError::StorageUnavailable)?,
    );
    let outbox_id = ids
        .next_uuid_v7()
        .map_err(|_| ExpiryError::StorageUnavailable)?;
    let result = ExpiryResult {
        message_id: request.message_id,
        event_id,
        version,
    };
    tx.write_message(request, version, now).await?;
    tx.abandon_in_flight(request, now).await?;
    tx.append_event(request, result, request.expected_version.get(), now)
        .await?;
    tx.enqueue_audit(request, result, outbox_id, now).await?;
    tx.finish(request, Ok(result), now).await?;
    Ok(Ok(result))
}

fn decide(
    team_state: &str,
    snapshot: Option<&ExpirySnapshot>,
    request: &ExpiryRequest,
    now: UtcTimestamp,
) -> Result<Version, ExpiryError> {
    match team_state {
        "ARCHIVED" => return Err(ExpiryError::TeamArchived),
        "ACTIVE" | "PAUSED" => (),
        _ => return Err(ExpiryError::IntegrityFailed),
    }
    let snapshot = snapshot.ok_or(ExpiryError::ObjectNotFound)?;
    if !request.expected_version.matches(snapshot.version) {
        return Err(ExpiryError::VersionConflict {
            current_version: snapshot.version,
        });
    }
    message_transition(snapshot.status, MessageCommand::Expire)
        .map_err(|_| ExpiryError::InvalidTransition)?;
    if snapshot.has_pending_attempt || !snapshot.expires_at.is_some_and(|expires| now >= expires) {
        return Err(ExpiryError::PreconditionsFailed);
    }
    let next = snapshot
        .version
        .next()
        .map_err(|_| ExpiryError::IntegrityFailed)?;
    if next.get() > i64::MAX as u64 {
        return Err(ExpiryError::IntegrityFailed);
    }
    Ok(next)
}
