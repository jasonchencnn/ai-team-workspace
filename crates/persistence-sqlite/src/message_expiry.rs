//! SQLx implementation of the Application-owned expiry transaction ports.
use application::message_expiry::{
    ExpiryError, ExpiryRequest, ExpiryResult, ExpirySnapshot, ExpiryStore, ExpiryTransaction,
};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use sqlx_core::{query::query, query_scalar::query_scalar, row::Row, transaction::Transaction};
use sqlx_sqlite::{Sqlite, SqlitePool};
use teamspace_domain::{
    Actor, EventId, MessageStatus, TeamId, UtcTimestamp, UuidV7, Version,
    idempotency::ActorFingerprint,
};

const COMMAND: &str = "internal.message.expire";
const DESTINATION: &str = "audit";
/// Host-owned SQLite capability; no externally supplied actor or allow flag.
pub struct SqliteExpiryStore {
    pool: SqlitePool,
}
impl SqliteExpiryStore {
    /// Uses an already migrated, foreign-key-enabled host pool.
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }
}
/// Transaction handle; its lifecycle is orchestrated by Application.
pub struct SqliteExpiryTransaction {
    tx: Transaction<'static, Sqlite>,
}

fn storage(_: sqlx_core::Error) -> ExpiryError {
    ExpiryError::StorageUnavailable
}
fn fingerprint() -> Vec<u8> {
    ActorFingerprint::of_actor(Actor::System)
        .as_bytes()
        .to_vec()
}
fn payload_hash(request: &ExpiryRequest) -> Vec<u8> {
    let mut hash = Sha256::new();
    hash.update(b"internal.message.expire/v1\0");
    hash.update(request.team_id.database_bytes());
    hash.update(request.message_id.database_bytes());
    hash.update(request.expected_version.get().get().to_be_bytes());
    hash.finalize().to_vec()
}
fn version(value: i64) -> Result<Version, ExpiryError> {
    Version::new(u64::try_from(value).map_err(|_| ExpiryError::IntegrityFailed)?)
        .map_err(|_| ExpiryError::IntegrityFailed)
}
fn stored_error(code: &str, details: &str) -> Result<ExpiryError, ExpiryError> {
    let details: Value = serde_json::from_str(details).map_err(|_| ExpiryError::IntegrityFailed)?;
    Ok(match code {
        "DOMAIN_OBJECT_NOT_FOUND" => ExpiryError::ObjectNotFound,
        "DOMAIN_TEAM_ARCHIVED" => ExpiryError::TeamArchived,
        "DOMAIN_INVALID_TRANSITION" => ExpiryError::InvalidTransition,
        "DOMAIN_PRECONDITION_FAILED" => ExpiryError::PreconditionsFailed,
        "CONCURRENCY_VERSION_CONFLICT" => ExpiryError::VersionConflict {
            current_version: Version::new(
                details["current_version"]
                    .as_u64()
                    .ok_or(ExpiryError::IntegrityFailed)?,
            )
            .map_err(|_| ExpiryError::IntegrityFailed)?,
        },
        _ => return Err(ExpiryError::IntegrityFailed),
    })
}
impl ExpiryStore for SqliteExpiryStore {
    type Transaction = SqliteExpiryTransaction;
    async fn begin(&self) -> Result<Self::Transaction, ExpiryError> {
        let tx = self
            .pool
            .begin_with("BEGIN IMMEDIATE")
            .await
            .map_err(storage)?;
        Ok(SqliteExpiryTransaction { tx })
    }
}
impl ExpiryTransaction for SqliteExpiryTransaction {
    async fn team_state(&mut self, team: TeamId) -> Result<Option<String>, ExpiryError> {
        query_scalar("SELECT status FROM teams WHERE id=?")
            .bind(team.database_bytes().to_vec())
            .fetch_optional(&mut *self.tx)
            .await
            .map_err(storage)
    }
    async fn replay(
        &mut self,
        request: &ExpiryRequest,
    ) -> Result<Option<Result<ExpiryResult, ExpiryError>>, ExpiryError> {
        let row = query("SELECT payload_hash,state,result_ref,first_error_code,first_error_details FROM command_idempotency WHERE team_id=? AND actor_fingerprint=? AND command_name=? AND idempotency_key=?")
            .bind(request.team_id.database_bytes().to_vec()).bind(fingerprint()).bind(COMMAND).bind(request.idempotency_key.as_str()).fetch_optional(&mut *self.tx).await.map_err(storage)?;
        let Some(row) = row else {
            return Ok(None);
        };
        if row.try_get::<Vec<u8>, _>("payload_hash").map_err(storage)? != payload_hash(request) {
            return Ok(Some(Err(ExpiryError::IdempotencyConflict)));
        }
        match row.try_get::<String, _>("state").map_err(storage)?.as_str() {
            "IN_PROGRESS" => Ok(Some(Err(ExpiryError::Busy))),
            "FAILED" => {
                let code: String = row.try_get("first_error_code").map_err(storage)?;
                let details: Option<String> =
                    row.try_get("first_error_details").map_err(storage)?;
                Ok(Some(Err(stored_error(
                    &code,
                    details.as_deref().unwrap_or("{}"),
                )?)))
            }
            "SUCCEEDED" => {
                let reference: String = row.try_get("result_ref").map_err(storage)?;
                let id = EventId::parse_protocol(&reference)
                    .map_err(|_| ExpiryError::IntegrityFailed)?;
                let stored_version: i64 = query_scalar("SELECT object_version FROM events WHERE id=? AND team_id=? AND subject_id=? AND subject_type='MESSAGE' AND event_type='MESSAGE_EXPIRED' AND actor_type='SYSTEM'")
                    .bind(id.database_bytes().to_vec()).bind(request.team_id.database_bytes().to_vec()).bind(request.message_id.database_bytes().to_vec()).fetch_one(&mut *self.tx).await.map_err(storage)?;
                Ok(Some(Ok(ExpiryResult {
                    message_id: request.message_id,
                    event_id: id,
                    version: version(stored_version)?,
                })))
            }
            _ => Err(ExpiryError::IntegrityFailed),
        }
    }
    async fn message(
        &mut self,
        request: &ExpiryRequest,
    ) -> Result<Option<ExpirySnapshot>, ExpiryError> {
        let row = query("SELECT status,version,expires_at,EXISTS(SELECT 1 FROM delivery_attempts WHERE message_id=messages.id AND status='PENDING') AS has_pending FROM messages WHERE id=? AND team_id=?")
            .bind(request.message_id.database_bytes().to_vec()).bind(request.team_id.database_bytes().to_vec()).fetch_optional(&mut *self.tx).await.map_err(storage)?;
        let Some(row) = row else {
            return Ok(None);
        };
        let state: String = row.try_get("status").map_err(storage)?;
        let status = MessageStatus::ALL
            .into_iter()
            .find(|v| v.as_str() == state)
            .ok_or(ExpiryError::IntegrityFailed)?;
        let expires: Option<i64> = row.try_get("expires_at").map_err(storage)?;
        Ok(Some(ExpirySnapshot {
            status,
            version: version(row.try_get("version").map_err(storage)?)?,
            expires_at: expires
                .map(UtcTimestamp::from_unix_millis)
                .transpose()
                .map_err(|_| ExpiryError::IntegrityFailed)?,
            has_pending_attempt: row.try_get::<i64, _>("has_pending").map_err(storage)? != 0,
        }))
    }
    async fn claim(
        &mut self,
        request: &ExpiryRequest,
        now: UtcTimestamp,
    ) -> Result<(), ExpiryError> {
        query("INSERT INTO command_idempotency(team_id,actor_fingerprint,command_name,idempotency_key,payload_hash,state,created_at,updated_at) VALUES(?,?,?,?,?,'IN_PROGRESS',?,?)")
            .bind(request.team_id.database_bytes().to_vec()).bind(fingerprint()).bind(COMMAND).bind(request.idempotency_key.as_str()).bind(payload_hash(request)).bind(now.unix_millis()).bind(now.unix_millis()).execute(&mut *self.tx).await.map_err(storage)?;
        Ok(())
    }
    async fn write_message(
        &mut self,
        request: &ExpiryRequest,
        next: Version,
        now: UtcTimestamp,
    ) -> Result<(), ExpiryError> {
        let changed = query("UPDATE messages SET status='EXPIRED',version=?,updated_at=? WHERE id=? AND team_id=? AND version=? AND status IN ('QUEUED','ROUTED') AND expires_at<=?")
            .bind(i64::try_from(next.get()).map_err(|_| ExpiryError::IntegrityFailed)?).bind(now.unix_millis()).bind(request.message_id.database_bytes().to_vec()).bind(request.team_id.database_bytes().to_vec()).bind(i64::try_from(request.expected_version.get().get()).map_err(|_| ExpiryError::IntegrityFailed)?).bind(now.unix_millis()).execute(&mut *self.tx).await.map_err(storage)?;
        if changed.rows_affected() != 1 {
            return Err(ExpiryError::IntegrityFailed);
        }
        Ok(())
    }
    async fn abandon_in_flight(
        &mut self,
        request: &ExpiryRequest,
        now: UtcTimestamp,
    ) -> Result<(), ExpiryError> {
        query("UPDATE delivery_attempts SET status='ABANDONED',completed_at=?,error_code='MESSAGE_TTL_EXPIRED' WHERE message_id=? AND team_id=? AND status='IN_FLIGHT'")
            .bind(now.unix_millis()).bind(request.message_id.database_bytes().to_vec()).bind(request.team_id.database_bytes().to_vec()).execute(&mut *self.tx).await.map_err(storage)?;
        Ok(())
    }
    async fn append_event(
        &mut self,
        request: &ExpiryRequest,
        result: ExpiryResult,
        previous_version: Version,
        now: UtcTimestamp,
    ) -> Result<(), ExpiryError> {
        // Insertion order is the chain order, independent of wall-clock rollback.
        // BEGIN IMMEDIATE serializes tail selection with the following insert.
        let previous_hash: Option<String> =
            query_scalar("SELECT hash FROM events WHERE team_id=? ORDER BY rowid DESC LIMIT 1")
                .bind(request.team_id.database_bytes().to_vec())
                .fetch_optional(&mut *self.tx)
                .await
                .map_err(storage)?;
        let payload =
            json!({"previous_version": previous_version.get(), "status": "EXPIRED"}).to_string();
        let metadata = json!({"correlation_id": request.request_id.to_protocol(), "hash_format": "message-expiry-v1"}).to_string();
        // Byte-compatible shared format; all protected nullable actor fields participate.
        let payload_value =
            serde_json::from_str::<Value>(&payload).map_err(|_| ExpiryError::IntegrityFailed)?;
        let metadata_value =
            serde_json::from_str::<Value>(&metadata).map_err(|_| ExpiryError::IntegrityFailed)?;
        let hash = crate::expiry_hash::digest(crate::expiry_hash::ExpiryHashFields {
            event_id: result.event_id,
            team_id: request.team_id,
            message_id: request.message_id,
            request_id: request.request_id,
            version: result.version,
            occurred_at: now,
            previous_hash: previous_hash.as_deref(),
            payload: &payload_value,
            metadata: &metadata_value,
        });
        query("INSERT INTO events(id,team_id,event_type,actor_type,actor_id,subject_type,subject_id,request_id,object_version,payload,metadata,occurred_at,previous_hash,hash) VALUES(?,?,'MESSAGE_EXPIRED','SYSTEM',NULL,'MESSAGE',?,?,?,?,?,?,?,?)")
            .bind(result.event_id.database_bytes().to_vec()).bind(request.team_id.database_bytes().to_vec()).bind(request.message_id.database_bytes().to_vec()).bind(request.request_id.database_bytes().to_vec()).bind(result.version.get() as i64).bind(payload).bind(metadata).bind(now.unix_millis()).bind(previous_hash).bind(hash).execute(&mut *self.tx).await.map_err(storage)?;
        Ok(())
    }
    async fn enqueue_audit(
        &mut self,
        request: &ExpiryRequest,
        result: ExpiryResult,
        outbox_id: UuidV7,
        now: UtcTimestamp,
    ) -> Result<(), ExpiryError> {
        query("INSERT INTO outbox(id,team_id,event_id,destination,payload,status,retry_count,idempotency_key,created_at,scheduled_at) VALUES(?,?,?,? ,?,'PENDING',0,?,?,?)")
            .bind(outbox_id.database_bytes().to_vec()).bind(request.team_id.database_bytes().to_vec()).bind(result.event_id.database_bytes().to_vec()).bind(DESTINATION).bind(json!({"event_id": result.event_id.to_protocol()}).to_string()).bind(result.event_id.to_protocol()).bind(now.unix_millis()).bind(now.unix_millis()).execute(&mut *self.tx).await.map_err(storage)?;
        Ok(())
    }
    async fn finish(
        &mut self,
        request: &ExpiryRequest,
        outcome: Result<ExpiryResult, ExpiryError>,
        now: UtcTimestamp,
    ) -> Result<(), ExpiryError> {
        let (state, reference, code, details) = match outcome {
            Ok(result) => ("SUCCEEDED", Some(result.event_id.to_protocol()), None, None),
            Err(error) => {
                let details = match error {
                    ExpiryError::VersionConflict { current_version } => {
                        json!({"current_version": current_version.get()})
                    }
                    _ => json!({}),
                };
                (
                    "FAILED",
                    None,
                    Some(error.code()),
                    Some(details.to_string()),
                )
            }
        };
        let changed = query("UPDATE command_idempotency SET state=?,result_ref=?,first_error_code=?,first_error_details=?,updated_at=? WHERE team_id=? AND actor_fingerprint=? AND command_name=? AND idempotency_key=? AND state='IN_PROGRESS'")
            .bind(state).bind(reference).bind(code).bind(details).bind(now.unix_millis()).bind(request.team_id.database_bytes().to_vec()).bind(fingerprint()).bind(COMMAND).bind(request.idempotency_key.as_str()).execute(&mut *self.tx).await.map_err(storage)?;
        if changed.rows_affected() != 1 {
            return Err(ExpiryError::IntegrityFailed);
        }
        Ok(())
    }
    async fn commit(self) -> Result<(), ExpiryError> {
        self.tx.commit().await.map_err(storage)
    }
    async fn rollback(self) -> Result<(), ExpiryError> {
        self.tx.rollback().await.map_err(storage)
    }
}
