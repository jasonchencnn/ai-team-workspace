//! SQLite ports for the fixed audit consumer. Transactions are orchestrated by
//! Application; this module never dispatches, acknowledges or mutates Events.
use crate::expiry_hash::{ExpiryHashFields, FORMAT, digest};
use application::audit_consumer::{
    AuditDelivery, AuditError, AuditEvent, AuditReceipt, AuditState, AuditStore, AuditTransaction,
};
use serde_json::{Value, json};
use sqlx_core::{
    query::query, query_scalar::query_scalar, row::Row, transaction::Transaction, value::ValueRef,
};
use sqlx_sqlite::{Sqlite, SqlitePool, SqliteRow};
use teamspace_domain::{EventId, MessageId, RequestId, TeamId, UtcTimestamp, Version};

const CONSUMER: &str = "audit-projection-v1";
/// Host-owned pool capability; consumers and destinations are fixed in code.
pub struct SqliteAuditStore {
    pool: SqlitePool,
}
impl SqliteAuditStore {
    /// Uses an already migrated, foreign-key-enabled pool.
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }
}
/// SQLite transaction implementing only the fixed projection ports.
pub struct SqliteAuditTransaction {
    tx: Transaction<'static, Sqlite>,
}
fn storage(_: sqlx_core::Error) -> AuditError {
    AuditError::StorageUnavailable
}
fn blob(row: &SqliteRow, key: &str) -> Result<[u8; 16], AuditError> {
    row.try_get::<Vec<u8>, _>(key)
        .map_err(storage)?
        .try_into()
        .map_err(|_| AuditError::IntegrityFailed)
}
fn timestamp(value: i64) -> Result<UtcTimestamp, AuditError> {
    UtcTimestamp::from_unix_millis(value).map_err(|_| AuditError::IntegrityFailed)
}
fn hash_bytes(value: &str) -> Result<[u8; 32], AuditError> {
    if value.len() != 64 {
        return Err(AuditError::IntegrityFailed);
    }
    let mut bytes = [0; 32];
    for (i, pair) in value.as_bytes().as_chunks::<2>().0.iter().enumerate() {
        let digit = |b: u8| match b {
            b'0'..=b'9' => Ok(b - b'0'),
            b'A'..=b'F' => Ok(b - b'A' + 10),
            _ => Err(AuditError::IntegrityFailed),
        };
        bytes[i] = digit(pair[0])? * 16 + digit(pair[1])?;
    }
    Ok(bytes)
}
fn event_fields(row: &SqliteRow) -> Result<AuditEvent, AuditError> {
    let stored_version: i64 = row.try_get("object_version").map_err(storage)?;
    Ok(AuditEvent {
        event_id: EventId::from_database_bytes(blob(row, "event_id")?)
            .map_err(|_| AuditError::IntegrityFailed)?,
        team_id: TeamId::from_database_bytes(blob(row, "team_id")?)
            .map_err(|_| AuditError::IntegrityFailed)?,
        message_id: MessageId::from_database_bytes(blob(row, "message_id")?)
            .map_err(|_| AuditError::IntegrityFailed)?,
        request_id: RequestId::from_database_bytes(blob(row, "request_id")?)
            .map_err(|_| AuditError::IntegrityFailed)?,
        object_version: Version::new(
            u64::try_from(stored_version).map_err(|_| AuditError::IntegrityFailed)?,
        )
        .map_err(|_| AuditError::IntegrityFailed)?,
        occurred_at: timestamp(row.try_get("occurred_at").map_err(storage)?)?,
        hash: hash_bytes(&row.try_get::<String, _>("event_hash").map_err(storage)?)?,
    })
}
impl AuditStore for SqliteAuditStore {
    type Transaction = SqliteAuditTransaction;
    async fn begin(&self) -> Result<Self::Transaction, AuditError> {
        Ok(SqliteAuditTransaction {
            tx: self
                .pool
                .begin_with("BEGIN IMMEDIATE")
                .await
                .map_err(storage)?,
        })
    }
}
impl AuditTransaction for SqliteAuditTransaction {
    async fn load_event(
        &mut self,
        delivery: AuditDelivery,
    ) -> Result<Option<AuditEvent>, AuditError> {
        let outbox =
            query("SELECT event_id,destination='audit' AS is_audit,CASE WHEN length(CAST(payload AS BLOB))<=512 THEN payload END AS payload FROM outbox WHERE id=? AND team_id=?")
                .bind(delivery.outbox_id.database_bytes().to_vec())
                .bind(delivery.team_id.database_bytes().to_vec())
                .fetch_optional(&mut *self.tx)
                .await
                .map_err(storage)?;
        let Some(outbox) = outbox else {
            return Ok(None);
        };
        if outbox.try_get::<i64, _>("is_audit").map_err(storage)? != 1 {
            return Err(AuditError::UnsupportedEvent);
        }
        let id = EventId::from_database_bytes(blob(&outbox, "event_id")?)
            .map_err(|_| AuditError::IntegrityFailed)?;
        let reference: Value = serde_json::from_str(
            &outbox
                .try_get::<Option<String>, _>("payload")
                .map_err(storage)?
                .ok_or(AuditError::IntegrityFailed)?,
        )
        .map_err(|_| AuditError::IntegrityFailed)?;
        if reference != json!({"event_id":id.to_protocol()}) {
            return Err(AuditError::IntegrityFailed);
        }
        let row=query("SELECT rowid AS insertion_sequence,id AS event_id,team_id,event_type='MESSAGE_EXPIRED' AS is_expiry,actor_type,actor_id,actor_kind,actor_role_id,actor_binding_id,actor_binding_version,actor_harness_instance_id,subject_type,subject_id AS message_id,request_id,object_version,CASE WHEN length(CAST(payload AS BLOB))<=512 THEN payload END AS payload,CASE WHEN length(CAST(metadata AS BLOB))<=512 THEN metadata END AS metadata,occurred_at,previous_hash,hash AS event_hash FROM events WHERE id=?")
            .bind(id.database_bytes().to_vec()).fetch_optional(&mut *self.tx).await.map_err(storage)?.ok_or(AuditError::IntegrityFailed)?;
        let metadata: Value = serde_json::from_str(
            &row.try_get::<Option<String>, _>("metadata")
                .map_err(storage)?
                .ok_or(AuditError::UnsupportedEvent)?,
        )
        .map_err(|_| AuditError::IntegrityFailed)?;
        if row.try_get::<i64, _>("is_expiry").map_err(storage)? != 1
            || metadata["hash_format"].as_str() != Some(FORMAT)
        {
            return Err(AuditError::UnsupportedEvent);
        }
        if row.try_get::<String, _>("actor_type").map_err(storage)? != "SYSTEM"
            || row.try_get::<String, _>("subject_type").map_err(storage)? != "MESSAGE"
        {
            return Err(AuditError::IntegrityFailed);
        }
        for key in [
            "actor_id",
            "actor_kind",
            "actor_role_id",
            "actor_binding_id",
            "actor_binding_version",
            "actor_harness_instance_id",
        ] {
            if !row.try_get_raw(key).map_err(storage)?.is_null() {
                return Err(AuditError::IntegrityFailed);
            }
        }
        let event = event_fields(&row)?;
        if event.team_id != delivery.team_id {
            return Err(AuditError::IntegrityFailed);
        }
        if metadata != json!({"correlation_id":event.request_id.to_protocol(),"hash_format":FORMAT})
        {
            return Err(AuditError::IntegrityFailed);
        }
        let payload: Value = serde_json::from_str(
            &row.try_get::<Option<String>, _>("payload")
                .map_err(storage)?
                .ok_or(AuditError::IntegrityFailed)?,
        )
        .map_err(|_| AuditError::IntegrityFailed)?;
        let previous_version = payload["previous_version"]
            .as_u64()
            .and_then(|v| Version::new(v).ok())
            .ok_or(AuditError::IntegrityFailed)?;
        if previous_version
            .next()
            .map_err(|_| AuditError::IntegrityFailed)?
            != event.object_version
            || payload != json!({"previous_version":previous_version.get(),"status":"EXPIRED"})
        {
            return Err(AuditError::IntegrityFailed);
        }
        let previous_hash: Option<String> = row.try_get("previous_hash").map_err(storage)?;
        let predecessor: Option<String> = query_scalar(
            "SELECT hash FROM events WHERE team_id=? AND rowid<? ORDER BY rowid DESC LIMIT 1",
        )
        .bind(event.team_id.database_bytes().to_vec())
        .bind(
            row.try_get::<i64, _>("insertion_sequence")
                .map_err(storage)?,
        )
        .fetch_optional(&mut *self.tx)
        .await
        .map_err(storage)?;
        if previous_hash != predecessor {
            return Err(AuditError::IntegrityFailed);
        }
        let expected = digest(ExpiryHashFields {
            event_id: event.event_id,
            team_id: event.team_id,
            message_id: event.message_id,
            request_id: event.request_id,
            version: event.object_version,
            occurred_at: event.occurred_at,
            previous_hash: previous_hash.as_deref(),
            payload: &payload,
            metadata: &metadata,
        });
        if row.try_get::<String, _>("event_hash").map_err(storage)? != expected {
            return Err(AuditError::IntegrityFailed);
        }
        Ok(Some(event))
    }
    async fn state(&mut self, event: AuditEvent) -> Result<AuditState, AuditError> {
        let marker = query(
            "SELECT team_id,consumed_at FROM event_consumptions WHERE consumer_id=? AND event_id=?",
        )
        .bind(CONSUMER)
        .bind(event.event_id.database_bytes().to_vec())
        .fetch_optional(&mut *self.tx)
        .await
        .map_err(storage)?;
        let projection=query("SELECT event_id,team_id,message_id,request_id,object_version,occurred_at,event_hash,consumed_at FROM audit_expiry_projection WHERE event_id=?")
            .bind(event.event_id.database_bytes().to_vec()).fetch_optional(&mut *self.tx).await.map_err(storage)?;
        let Some(marker) = marker else {
            return if projection.is_none() {
                Ok(AuditState::Unconsumed)
            } else {
                Err(AuditError::IntegrityFailed)
            };
        };
        if blob(&marker, "team_id")? != event.team_id.database_bytes() {
            return Err(AuditError::IntegrityFailed);
        }
        let consumed_at = timestamp(marker.try_get("consumed_at").map_err(storage)?)?;
        let Some(projection) = projection else {
            return Ok(AuditState::MissingProjection { consumed_at });
        };
        let receipt = AuditReceipt {
            event: event_fields(&projection)?,
            consumed_at: timestamp(projection.try_get("consumed_at").map_err(storage)?)?,
        };
        if receipt.consumed_at != consumed_at || receipt.event != event {
            return Err(AuditError::IntegrityFailed);
        }
        Ok(AuditState::Consumed(receipt))
    }
    async fn record_consumption(&mut self, receipt: AuditReceipt) -> Result<(), AuditError> {
        let changed=query("INSERT INTO event_consumptions(consumer_id,event_id,team_id,consumed_at) VALUES(?,?,?,?)")
            .bind(CONSUMER).bind(receipt.event.event_id.database_bytes().to_vec()).bind(receipt.event.team_id.database_bytes().to_vec()).bind(receipt.consumed_at.unix_millis()).execute(&mut *self.tx).await.map_err(storage)?;
        if changed.rows_affected() != 1 {
            return Err(AuditError::IntegrityFailed);
        }
        Ok(())
    }
    async fn write_projection(&mut self, receipt: AuditReceipt) -> Result<(), AuditError> {
        let hash = receipt
            .event
            .hash
            .iter()
            .map(|b| format!("{b:02X}"))
            .collect::<String>();
        let changed=query("INSERT INTO audit_expiry_projection(event_id,consumer_id,team_id,message_id,request_id,object_version,occurred_at,event_hash,consumed_at) VALUES(?,?,?,?,?,?,?,?,?)")
            .bind(receipt.event.event_id.database_bytes().to_vec()).bind(CONSUMER).bind(receipt.event.team_id.database_bytes().to_vec()).bind(receipt.event.message_id.database_bytes().to_vec()).bind(receipt.event.request_id.database_bytes().to_vec()).bind(i64::try_from(receipt.event.object_version.get()).map_err(|_|AuditError::IntegrityFailed)?).bind(receipt.event.occurred_at.unix_millis()).bind(hash).bind(receipt.consumed_at.unix_millis()).execute(&mut *self.tx).await.map_err(storage)?;
        if changed.rows_affected() != 1 {
            return Err(AuditError::IntegrityFailed);
        }
        Ok(())
    }
    async fn commit(self) -> Result<(), AuditError> {
        self.tx.commit().await.map_err(storage)
    }
    async fn rollback(self) -> Result<(), AuditError> {
        self.tx.rollback().await.map_err(storage)
    }
}
