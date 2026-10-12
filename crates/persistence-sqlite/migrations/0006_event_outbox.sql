-- DEV-02-07: Event, Outbox, and Consumption Deduplication tables.
-- IDs use the 16-byte UUIDv7 BLOB representation frozen in DATA_CONVENTIONS.md;
-- timestamps are UTC Unix milliseconds.

CREATE TABLE events (
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
    event_type TEXT NOT NULL CHECK (length(trim(event_type)) > 0),
    actor_type TEXT NOT NULL CHECK (actor_type IN ('HUMAN', 'ROLE', 'SYSTEM', 'ADAPTER')),
    actor_id BLOB
        CHECK (
            actor_id IS NULL
            OR (
                typeof(actor_id) = 'blob'
                AND length(actor_id) = 16
                AND substr(hex(actor_id), 13, 1) = '7'
                AND substr(hex(actor_id), 17, 1) IN ('8', '9', 'A', 'B')
            )
        ),
    subject_type TEXT NOT NULL CHECK (
        subject_type IN (
            'TEAM', 'ROLE', 'WORKSPACE', 'HARNESS', 'ADAPTER',
            'BINDING', 'TASK', 'RUN', 'ARTIFACT', 'THREAD',
            'MESSAGE', 'NOTIFICATION', 'APPROVAL'
        )
    ),
    subject_id BLOB NOT NULL
        CHECK (
            typeof(subject_id) = 'blob'
            AND length(subject_id) = 16
            AND substr(hex(subject_id), 13, 1) = '7'
            AND substr(hex(subject_id), 17, 1) IN ('8', '9', 'A', 'B')
        ),
    request_id BLOB
        CHECK (
            request_id IS NULL
            OR (
                typeof(request_id) = 'blob'
                AND length(request_id) = 16
                AND substr(hex(request_id), 13, 1) = '7'
                AND substr(hex(request_id), 17, 1) IN ('8', '9', 'A', 'B')
            )
        ),
    object_version INTEGER NOT NULL CHECK (typeof(object_version) = 'integer' AND object_version >= 1),
    payload TEXT NOT NULL DEFAULT '{}' CHECK (json_valid(payload)),
    metadata TEXT NOT NULL DEFAULT '{}' CHECK (json_valid(metadata)),
    occurred_at INTEGER NOT NULL CHECK (typeof(occurred_at) = 'integer'),
    previous_hash TEXT CHECK (previous_hash IS NULL OR (length(previous_hash) = 64 AND hex(unhex(previous_hash)) = upper(previous_hash))),
    hash TEXT NOT NULL CHECK (length(hash) = 64 AND hex(unhex(hash)) = upper(hash)),
    CONSTRAINT events_team_id_foreign_key
        FOREIGN KEY (team_id) REFERENCES teams (id)
        ON UPDATE RESTRICT ON DELETE RESTRICT
);

-- Events are strictly append-only: updates and deletes are prohibited at database engine level.
CREATE TRIGGER events_prohibit_update
BEFORE UPDATE ON events
BEGIN
    SELECT RAISE(ABORT, 'EVENTS_ARE_APPEND_ONLY');
END;

CREATE TRIGGER events_prohibit_delete
BEFORE DELETE ON events
BEGIN
    SELECT RAISE(ABORT, 'EVENTS_ARE_APPEND_ONLY');
END;

CREATE INDEX events_team_occurred ON events (team_id, occurred_at, id);
CREATE INDEX events_subject ON events (subject_type, subject_id, occurred_at);

CREATE TABLE outbox (
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
    max_retries INTEGER NOT NULL DEFAULT 5 CHECK (typeof(max_retries) = 'integer' AND max_retries >= 0),
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

-- Unique index to prevent duplicate outbox entries for the same event and destination
CREATE UNIQUE INDEX outbox_event_destination_unique ON outbox (event_id, destination);

CREATE INDEX outbox_team_id ON outbox (team_id);
CREATE INDEX outbox_pending_dispatch ON outbox (status, scheduled_at) WHERE status IN ('PENDING', 'FAILED');

-- Consumer tracking table for consumption deduplication and deterministic replay
CREATE TABLE event_consumptions (
    consumer_id TEXT NOT NULL CHECK (length(trim(consumer_id)) > 0),
    event_id BLOB NOT NULL
        CHECK (
            typeof(event_id) = 'blob'
            AND length(event_id) = 16
            AND substr(hex(event_id), 13, 1) = '7'
            AND substr(hex(event_id), 17, 1) IN ('8', '9', 'A', 'B')
        ),
    team_id BLOB NOT NULL
        CHECK (
            typeof(team_id) = 'blob'
            AND length(team_id) = 16
            AND substr(hex(team_id), 13, 1) = '7'
            AND substr(hex(team_id), 17, 1) IN ('8', '9', 'A', 'B')
        ),
    consumed_at INTEGER NOT NULL CHECK (typeof(consumed_at) = 'integer'),
    PRIMARY KEY (consumer_id, event_id),
    CONSTRAINT event_consumptions_team_id_foreign_key
        FOREIGN KEY (team_id) REFERENCES teams (id)
        ON UPDATE RESTRICT ON DELETE RESTRICT,
    CONSTRAINT event_consumptions_event_id_foreign_key
        FOREIGN KEY (event_id) REFERENCES events (id)
        ON UPDATE RESTRICT ON DELETE RESTRICT
);

CREATE INDEX event_consumptions_team_consumer ON event_consumptions (team_id, consumer_id, consumed_at);
