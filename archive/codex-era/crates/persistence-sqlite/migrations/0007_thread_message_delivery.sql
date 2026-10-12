-- DEV-02-08: Thread, Message, and Delivery Attempt tables.
-- IDs use the 16-byte UUIDv7 BLOB representation frozen in DATA_CONVENTIONS.md;
-- timestamps are UTC Unix milliseconds.

CREATE TABLE threads (
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
    subject TEXT NOT NULL CHECK (length(trim(subject)) > 0),
    status TEXT NOT NULL CHECK (status IN ('OPEN', 'CLOSED', 'ARCHIVED')),
    version INTEGER NOT NULL CHECK (typeof(version) = 'integer' AND version >= 1),
    created_at INTEGER NOT NULL CHECK (typeof(created_at) = 'integer'),
    updated_at INTEGER NOT NULL CHECK (typeof(updated_at) = 'integer'),
    CONSTRAINT threads_team_id_foreign_key
        FOREIGN KEY (team_id) REFERENCES teams (id)
        ON UPDATE RESTRICT ON DELETE RESTRICT
);

CREATE INDEX threads_team_id ON threads (team_id);

CREATE TABLE messages (
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
    thread_id BLOB NOT NULL
        CHECK (
            typeof(thread_id) = 'blob'
            AND length(thread_id) = 16
            AND substr(hex(thread_id), 13, 1) = '7'
            AND substr(hex(thread_id), 17, 1) IN ('8', '9', 'A', 'B')
        ),
    sender_actor_type TEXT NOT NULL CHECK (sender_actor_type IN ('ROLE', 'HUMAN', 'SYSTEM')),
    from_role_id BLOB
        CHECK (
            from_role_id IS NULL
            OR (
                typeof(from_role_id) = 'blob'
                AND length(from_role_id) = 16
                AND substr(hex(from_role_id), 13, 1) = '7'
                AND substr(hex(from_role_id), 17, 1) IN ('8', '9', 'A', 'B')
            )
        ),
    from_human_id BLOB
        CHECK (
            from_human_id IS NULL
            OR (
                typeof(from_human_id) = 'blob'
                AND length(from_human_id) = 16
                AND substr(hex(from_human_id), 13, 1) = '7'
                AND substr(hex(from_human_id), 17, 1) IN ('8', '9', 'A', 'B')
            )
        ),
    -- to_role_id is mandatory: messages are sent to Roles (Human notifications belong to notification service)
    to_role_id BLOB NOT NULL
        CHECK (
            typeof(to_role_id) = 'blob'
            AND length(to_role_id) = 16
            AND substr(hex(to_role_id), 13, 1) = '7'
            AND substr(hex(to_role_id), 17, 1) IN ('8', '9', 'A', 'B')
        ),
    sender_binding_snapshot_id BLOB
        CHECK (
            sender_binding_snapshot_id IS NULL
            OR (
                typeof(sender_binding_snapshot_id) = 'blob'
                AND length(sender_binding_snapshot_id) = 16
                AND substr(hex(sender_binding_snapshot_id), 13, 1) = '7'
                AND substr(hex(sender_binding_snapshot_id), 17, 1) IN ('8', '9', 'A', 'B')
            )
        ),
    sender_binding_generation INTEGER CHECK (sender_binding_generation IS NULL OR sender_binding_generation >= 1),
    reply_to_message_id BLOB
        CHECK (
            reply_to_message_id IS NULL
            OR (
                typeof(reply_to_message_id) = 'blob'
                AND length(reply_to_message_id) = 16
                AND substr(hex(reply_to_message_id), 13, 1) = '7'
                AND substr(hex(reply_to_message_id), 17, 1) IN ('8', '9', 'A', 'B')
                AND reply_to_message_id != id
            )
        ),
    message_type TEXT NOT NULL CHECK (message_type IN ('DIRECT', 'TASK_CONTEXT', 'SYSTEM_DISPATCH', 'QUESTION', 'REPORT')),
    body TEXT NOT NULL,
    status TEXT NOT NULL CHECK (
        status IN (
            'DRAFT', 'QUEUED', 'ROUTED', 'DELIVERED', 'ACKNOWLEDGED',
            'PROCESSING', 'REPLIED', 'FAILED', 'EXPIRED', 'NEEDS_HUMAN'
        )
    ),
    version INTEGER NOT NULL CHECK (typeof(version) = 'integer' AND version >= 1),
    idempotency_key TEXT,
    expires_at INTEGER CHECK (expires_at IS NULL OR typeof(expires_at) = 'integer'),
    created_at INTEGER NOT NULL CHECK (typeof(created_at) = 'integer'),
    updated_at INTEGER NOT NULL CHECK (typeof(updated_at) = 'integer'),
    delivered_at INTEGER CHECK (delivered_at IS NULL OR typeof(delivered_at) = 'integer'),
    acknowledged_at INTEGER CHECK (acknowledged_at IS NULL OR typeof(acknowledged_at) = 'integer'),
    replied_at INTEGER CHECK (replied_at IS NULL OR typeof(replied_at) = 'integer'),
    CONSTRAINT messages_team_id_foreign_key
        FOREIGN KEY (team_id) REFERENCES teams (id)
        ON UPDATE RESTRICT ON DELETE RESTRICT,
    CONSTRAINT messages_thread_id_foreign_key
        FOREIGN KEY (thread_id) REFERENCES threads (id)
        ON UPDATE RESTRICT ON DELETE RESTRICT,
    CONSTRAINT messages_from_role_id_foreign_key
        FOREIGN KEY (from_role_id) REFERENCES roles (id)
        ON UPDATE RESTRICT ON DELETE RESTRICT,
    CONSTRAINT messages_to_role_id_foreign_key
        FOREIGN KEY (to_role_id) REFERENCES roles (id)
        ON UPDATE RESTRICT ON DELETE RESTRICT,
    CONSTRAINT messages_sender_binding_foreign_key
        FOREIGN KEY (sender_binding_snapshot_id) REFERENCES bindings (id)
        ON UPDATE RESTRICT ON DELETE RESTRICT,
    CONSTRAINT messages_reply_to_foreign_key
        FOREIGN KEY (reply_to_message_id) REFERENCES messages (id)
        ON UPDATE RESTRICT ON DELETE RESTRICT
);

-- Trigger: Once QUEUED (or past DRAFT), message body is frozen.
CREATE TRIGGER messages_freeze_body_after_queued
BEFORE UPDATE OF body ON messages
WHEN OLD.status != 'DRAFT'
BEGIN
    SELECT RAISE(ABORT, 'MESSAGE_BODY_FROZEN_AFTER_QUEUED');
END;

-- Trigger: reply_to message must belong to the same thread.
CREATE TRIGGER messages_reply_to_same_thread
BEFORE INSERT ON messages
WHEN NEW.reply_to_message_id IS NOT NULL
BEGIN
    SELECT RAISE(ABORT, 'MESSAGE_THREAD_MISMATCH')
    FROM messages target
    WHERE target.id = NEW.reply_to_message_id AND target.thread_id != NEW.thread_id;
END;

CREATE UNIQUE INDEX messages_team_idempotency ON messages (team_id, idempotency_key) WHERE idempotency_key IS NOT NULL;
CREATE INDEX messages_thread_id ON messages (thread_id, created_at);
CREATE INDEX messages_to_role_status ON messages (to_role_id, status);
CREATE INDEX messages_pending_expiry ON messages (status, expires_at) WHERE status IN ('QUEUED', 'ROUTED') AND expires_at IS NOT NULL;

CREATE TABLE delivery_attempts (
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
    message_id BLOB NOT NULL
        CHECK (
            typeof(message_id) = 'blob'
            AND length(message_id) = 16
            AND substr(hex(message_id), 13, 1) = '7'
            AND substr(hex(message_id), 17, 1) IN ('8', '9', 'A', 'B')
        ),
    attempt_number INTEGER NOT NULL CHECK (typeof(attempt_number) = 'integer' AND attempt_number >= 1),
    target_role_id BLOB NOT NULL
        CHECK (
            typeof(target_role_id) = 'blob'
            AND length(target_role_id) = 16
            AND substr(hex(target_role_id), 13, 1) = '7'
            AND substr(hex(target_role_id), 17, 1) IN ('8', '9', 'A', 'B')
        ),
    target_binding_id BLOB
        CHECK (
            target_binding_id IS NULL
            OR (
                typeof(target_binding_id) = 'blob'
                AND length(target_binding_id) = 16
                AND substr(hex(target_binding_id), 13, 1) = '7'
                AND substr(hex(target_binding_id), 17, 1) IN ('8', '9', 'A', 'B')
            )
        ),
    target_binding_generation INTEGER CHECK (target_binding_generation IS NULL OR target_binding_generation >= 1),
    status TEXT NOT NULL CHECK (status IN ('PENDING', 'IN_FLIGHT', 'SUCCEEDED', 'FAILED', 'ABANDONED')),
    error_code TEXT,
    error_message TEXT,
    created_at INTEGER NOT NULL CHECK (typeof(created_at) = 'integer'),
    started_at INTEGER CHECK (started_at IS NULL OR typeof(started_at) = 'integer'),
    completed_at INTEGER CHECK (completed_at IS NULL OR typeof(completed_at) = 'integer'),
    CONSTRAINT delivery_attempts_team_id_foreign_key
        FOREIGN KEY (team_id) REFERENCES teams (id)
        ON UPDATE RESTRICT ON DELETE RESTRICT,
    CONSTRAINT delivery_attempts_message_id_foreign_key
        FOREIGN KEY (message_id) REFERENCES messages (id)
        ON UPDATE RESTRICT ON DELETE RESTRICT,
    CONSTRAINT delivery_attempts_target_role_id_foreign_key
        FOREIGN KEY (target_role_id) REFERENCES roles (id)
        ON UPDATE RESTRICT ON DELETE RESTRICT,
    CONSTRAINT delivery_attempts_target_binding_id_foreign_key
        FOREIGN KEY (target_binding_id) REFERENCES bindings (id)
        ON UPDATE RESTRICT ON DELETE RESTRICT
);

-- Monotonic attempt number per message
CREATE UNIQUE INDEX delivery_attempts_message_attempt ON delivery_attempts (message_id, attempt_number);

-- At most one active delivery attempt per message in flight
CREATE UNIQUE INDEX delivery_attempts_single_active ON delivery_attempts (message_id) WHERE status IN ('PENDING', 'IN_FLIGHT');

CREATE INDEX delivery_attempts_team_id ON delivery_attempts (team_id);
CREATE INDEX delivery_attempts_target_role ON delivery_attempts (target_role_id, status);
