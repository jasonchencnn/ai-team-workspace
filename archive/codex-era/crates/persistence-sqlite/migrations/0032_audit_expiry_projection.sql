-- DEV-02-07: reconstructible read model for the fixed audit-projection-v1
-- consumer. No business aggregate, Event, Outbox or existing marker is changed.
-- Event identity is the projection key, not a new domain object's identity.
CREATE TABLE audit_expiry_projection (
    event_id BLOB PRIMARY KEY NOT NULL CHECK(typeof(event_id)='blob' AND length(event_id)=16),
    consumer_id TEXT NOT NULL CHECK(consumer_id='audit-projection-v1'),
    team_id BLOB NOT NULL CHECK(typeof(team_id)='blob' AND length(team_id)=16),
    message_id BLOB NOT NULL CHECK(typeof(message_id)='blob' AND length(message_id)=16),
    request_id BLOB NOT NULL CHECK(typeof(request_id)='blob' AND length(request_id)=16),
    object_version INTEGER NOT NULL CHECK(typeof(object_version)='integer' AND object_version>=1),
    occurred_at INTEGER NOT NULL CHECK(typeof(occurred_at)='integer'),
    event_hash TEXT NOT NULL CHECK(length(event_hash)=64 AND hex(unhex(event_hash))=upper(event_hash)),
    consumed_at INTEGER NOT NULL CHECK(typeof(consumed_at)='integer'),
    FOREIGN KEY(event_id) REFERENCES events(id) ON UPDATE RESTRICT ON DELETE RESTRICT,
    FOREIGN KEY(consumer_id,event_id) REFERENCES event_consumptions(consumer_id,event_id) ON UPDATE RESTRICT ON DELETE RESTRICT,
    FOREIGN KEY(team_id) REFERENCES teams(id) ON UPDATE RESTRICT ON DELETE RESTRICT,
    FOREIGN KEY(message_id) REFERENCES messages(id) ON UPDATE RESTRICT ON DELETE RESTRICT
);

-- Projection values must be copies of their protected source and first receipt.
-- Cryptographic verification and scope/format admission remain in the reader.
CREATE TRIGGER audit_expiry_projection_source_insert
BEFORE INSERT ON audit_expiry_projection
WHEN NOT EXISTS (
    SELECT 1 FROM events AS e JOIN event_consumptions AS c ON c.event_id=e.id
    WHERE e.id=NEW.event_id AND e.team_id IS NEW.team_id
      AND e.subject_type='MESSAGE' AND e.subject_id IS NEW.message_id
      AND e.event_type='MESSAGE_EXPIRED' AND e.actor_type='SYSTEM'
      AND e.request_id IS NEW.request_id AND e.object_version IS NEW.object_version
      AND e.occurred_at IS NEW.occurred_at AND e.hash IS NEW.event_hash
      AND c.consumer_id=NEW.consumer_id AND c.team_id IS NEW.team_id
      AND c.consumed_at IS NEW.consumed_at
)
BEGIN SELECT RAISE(ABORT,'audit projection must agree with source and first receipt'); END;

-- A reader must never turn replay into an overwrite of the first receipt.
CREATE TRIGGER audit_expiry_projection_no_replace
BEFORE INSERT ON audit_expiry_projection
WHEN EXISTS(SELECT 1 FROM audit_expiry_projection WHERE event_id=NEW.event_id)
BEGIN SELECT RAISE(ABORT,'audit projection replay must not replace a receipt'); END;

CREATE TRIGGER audit_expiry_projection_no_update
BEFORE UPDATE ON audit_expiry_projection
BEGIN SELECT RAISE(ABORT,'audit projection rebuilds from its source instead of mutating a receipt'); END;

-- The derived table may be reconstructed without deleting the dedup ledger.
-- Narrow to this newly implemented consumer; historical other consumers are
-- not assigned a retention policy by this migration.
CREATE TRIGGER audit_consumption_no_delete
BEFORE DELETE ON event_consumptions
WHEN OLD.consumer_id='audit-projection-v1'
BEGIN SELECT RAISE(ABORT,'audit first consumption is retained'); END;

CREATE INDEX audit_expiry_projection_team_time ON audit_expiry_projection(team_id,occurred_at,event_id);
