-- DEV-02-07 remediation: ERROR_CATALOG section 11 freezes default retries at 3.
-- SQLite cannot alter a column default directly. Rebuild only Outbox; preserve
-- every stored column, configured retry ceiling, status, dedup marker and time.
-- No other published migration is changed. SQLx runs this DDL transactionally.
-- Outbox has no incoming foreign keys in the frozen schema. Its three named
-- indexes and four guards from 0014..0016 are recreated verbatim below.

CREATE TABLE outbox_default_rebuild (
    id BLOB PRIMARY KEY NOT NULL
        CHECK (
            typeof(id) = 'blob'
            AND length(id) = 16
            AND substr(hex(id), 13, 1) = '7'
            AND substr(hex(id), 17, 1) IN ('8', '9', 'A', 'B')
        ),
    team_id BLOB NOT NULL
        CHECK (
            typeof(team_id) = 'blob'
            AND length(team_id) = 16
            AND substr(hex(team_id), 13, 1) = '7'
            AND substr(hex(team_id), 17, 1) IN ('8', '9', 'A', 'B')
        ),
    event_id BLOB NOT NULL
        CHECK (
            typeof(event_id) = 'blob'
            AND length(event_id) = 16
            AND substr(hex(event_id), 13, 1) = '7'
            AND substr(hex(event_id), 17, 1) IN ('8', '9', 'A', 'B')
        ),
    destination TEXT NOT NULL CHECK (length(trim(destination)) > 0),
    payload TEXT NOT NULL CHECK (json_valid(payload)),
    status TEXT NOT NULL CHECK (status IN ('PENDING', 'PROCESSING', 'PUBLISHED', 'FAILED', 'DEAD_LETTER')),
    retry_count INTEGER NOT NULL DEFAULT 0 CHECK (typeof(retry_count) = 'integer' AND retry_count >= 0),
    max_retries INTEGER NOT NULL DEFAULT 3 CHECK (typeof(max_retries) = 'integer' AND max_retries >= 0),
    last_error TEXT,
    idempotency_key TEXT,
    created_at INTEGER NOT NULL CHECK (typeof(created_at) = 'integer'),
    scheduled_at INTEGER NOT NULL CHECK (typeof(scheduled_at) = 'integer'),
    published_at INTEGER CHECK (published_at IS NULL OR typeof(published_at) = 'integer'),
    CONSTRAINT outbox_team_id_foreign_key
        FOREIGN KEY (team_id) REFERENCES teams (id)
        ON UPDATE RESTRICT ON DELETE RESTRICT,
    CONSTRAINT outbox_event_id_foreign_key
        FOREIGN KEY (event_id) REFERENCES events (id)
        ON UPDATE RESTRICT ON DELETE RESTRICT
);

INSERT INTO outbox_default_rebuild (
    id, team_id, event_id, destination, payload, status, retry_count,
    max_retries, last_error, idempotency_key, created_at, scheduled_at, published_at
)
SELECT id, team_id, event_id, destination, payload, status, retry_count,
       max_retries, last_error, idempotency_key, created_at, scheduled_at, published_at
FROM outbox;

DROP TABLE outbox;
ALTER TABLE outbox_default_rebuild RENAME TO outbox;

CREATE UNIQUE INDEX outbox_event_destination_unique ON outbox (event_id, destination);
CREATE INDEX outbox_team_id ON outbox (team_id);
CREATE INDEX outbox_pending_dispatch ON outbox (status, scheduled_at) WHERE status IN ('PENDING', 'FAILED');

CREATE TRIGGER outbox_event_team_consistency_insert
BEFORE INSERT ON outbox
FOR EACH ROW
WHEN NOT EXISTS (
    SELECT 1 FROM events
    WHERE id = NEW.event_id AND team_id = NEW.team_id
)
BEGIN
    SELECT RAISE(ABORT, 'Outbox Event Team consistency violation');
END;

CREATE TRIGGER outbox_event_team_consistency_update
BEFORE UPDATE ON outbox
FOR EACH ROW
WHEN NOT EXISTS (
    SELECT 1 FROM events
    WHERE id = NEW.event_id AND team_id = NEW.team_id
)
BEGIN
    SELECT RAISE(ABORT, 'Outbox Event Team consistency violation');
END;

CREATE TRIGGER outbox_deduplication_replace_guard
BEFORE INSERT ON outbox
FOR EACH ROW
WHEN EXISTS (
    SELECT 1 FROM outbox
    WHERE event_id = NEW.event_id AND destination = NEW.destination
)
BEGIN
    SELECT RAISE(ABORT, 'Outbox deduplication is immutable');
END;

CREATE TRIGGER outbox_deduplication_state_immutable_update
BEFORE UPDATE ON outbox
FOR EACH ROW
WHEN NEW.event_id IS NOT OLD.event_id
   OR NEW.destination IS NOT OLD.destination
   OR NEW.idempotency_key IS NOT OLD.idempotency_key
   OR (OLD.published_at IS NOT NULL AND NEW.published_at IS NULL)
   OR (OLD.status = 'PUBLISHED' AND NEW.status IS NOT 'PUBLISHED')
   OR (OLD.status = 'DEAD_LETTER' AND NEW.status IS NOT 'DEAD_LETTER')
   OR NEW.retry_count < OLD.retry_count
BEGIN
    SELECT RAISE(ABORT, 'Outbox deduplication state is immutable');
END;
