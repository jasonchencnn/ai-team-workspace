-- DEV-02-09: Notification, Policy, and Attention Item tables.
-- IDs use the 16-byte UUIDv7 BLOB representation frozen in DATA_CONVENTIONS.md;
-- timestamps are UTC Unix milliseconds.

CREATE TABLE notification_policies (
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
    name TEXT NOT NULL CHECK (length(trim(name)) > 0),
    event_type TEXT NOT NULL CHECK (length(trim(event_type)) > 0),
    channel TEXT NOT NULL CHECK (channel IN ('IN_APP', 'OS_SYSTEM', 'EMAIL', 'WEBHOOK')),
    severity TEXT NOT NULL CHECK (severity IN ('INFO', 'WARNING', 'CRITICAL')),
    dedupe_window_seconds INTEGER NOT NULL DEFAULT 0 CHECK (dedupe_window_seconds >= 0),
    ttl_seconds INTEGER NOT NULL DEFAULT 86400 CHECK (ttl_seconds >= 0),
    status TEXT NOT NULL CHECK (status IN ('ACTIVE', 'DISABLED')),
    version INTEGER NOT NULL CHECK (typeof(version) = 'integer' AND version >= 1),
    created_at INTEGER NOT NULL CHECK (typeof(created_at) = 'integer'),
    updated_at INTEGER NOT NULL CHECK (typeof(updated_at) = 'integer'),
    CONSTRAINT notification_policies_team_id_foreign_key
        FOREIGN KEY (team_id) REFERENCES teams (id)
        ON UPDATE RESTRICT ON DELETE RESTRICT
);

CREATE UNIQUE INDEX notification_policies_team_name ON notification_policies (team_id, name);
CREATE INDEX notification_policies_event_type ON notification_policies (team_id, event_type, status);

CREATE TABLE notifications (
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
    recipient_human_id BLOB NOT NULL
        CHECK (
            typeof(recipient_human_id) = 'blob'
            AND length(recipient_human_id) = 16
            AND substr(hex(recipient_human_id), 13, 1) = '7'
            AND substr(hex(recipient_human_id), 17, 1) IN ('8', '9', 'A', 'B')
        ),
    source_event_id BLOB
        CHECK (
            source_event_id IS NULL
            OR (
                typeof(source_event_id) = 'blob'
                AND length(source_event_id) = 16
                AND substr(hex(source_event_id), 13, 1) = '7'
                AND substr(hex(source_event_id), 17, 1) IN ('8', '9', 'A', 'B')
            )
        ),
    source_message_id BLOB
        CHECK (
            source_message_id IS NULL
            OR (
                typeof(source_message_id) = 'blob'
                AND length(source_message_id) = 16
                AND substr(hex(source_message_id), 13, 1) = '7'
                AND substr(hex(source_message_id), 17, 1) IN ('8', '9', 'A', 'B')
            )
        ),
    policy_id BLOB
        CHECK (
            policy_id IS NULL
            OR (
                typeof(policy_id) = 'blob'
                AND length(policy_id) = 16
                AND substr(hex(policy_id), 13, 1) = '7'
                AND substr(hex(policy_id), 17, 1) IN ('8', '9', 'A', 'B')
            )
        ),
    severity TEXT NOT NULL CHECK (severity IN ('INFO', 'WARNING', 'CRITICAL')),
    title TEXT NOT NULL CHECK (length(trim(title)) > 0),
    body TEXT NOT NULL,
    status TEXT NOT NULL CHECK (status IN ('UNREAD', 'READ', 'DISMISSED', 'EXPIRED')),
    dedupe_key TEXT,
    version INTEGER NOT NULL CHECK (typeof(version) = 'integer' AND version >= 1),
    expires_at INTEGER CHECK (expires_at IS NULL OR typeof(expires_at) = 'integer'),
    created_at INTEGER NOT NULL CHECK (typeof(created_at) = 'integer'),
    read_at INTEGER CHECK (read_at IS NULL OR typeof(read_at) = 'integer'),
    dismissed_at INTEGER CHECK (dismissed_at IS NULL OR typeof(dismissed_at) = 'integer'),
    CONSTRAINT notifications_team_id_foreign_key
        FOREIGN KEY (team_id) REFERENCES teams (id)
        ON UPDATE RESTRICT ON DELETE RESTRICT,
    CONSTRAINT notifications_source_event_id_foreign_key
        FOREIGN KEY (source_event_id) REFERENCES events (id)
        ON UPDATE RESTRICT ON DELETE RESTRICT,
    CONSTRAINT notifications_source_message_id_foreign_key
        FOREIGN KEY (source_message_id) REFERENCES messages (id)
        ON UPDATE RESTRICT ON DELETE RESTRICT,
    CONSTRAINT notifications_policy_id_foreign_key
        FOREIGN KEY (policy_id) REFERENCES notification_policies (id)
        ON UPDATE RESTRICT ON DELETE RESTRICT
);

-- Deduplication index: Prevents duplicate active unread/read notifications for the same dedupe key per human recipient
CREATE UNIQUE INDEX notifications_active_dedupe
ON notifications (team_id, recipient_human_id, dedupe_key)
WHERE dedupe_key IS NOT NULL AND status IN ('UNREAD', 'READ');

CREATE INDEX notifications_recipient_status ON notifications (recipient_human_id, status, created_at);
CREATE INDEX notifications_team_id ON notifications (team_id);

CREATE TABLE attention_items (
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
    attention_type TEXT NOT NULL CHECK (
        attention_type IN (
            'APPROVAL_REQUIRED', 'TASK_BLOCKED', 'RUN_FAILED',
            'RUN_TIMED_OUT', 'MESSAGE_NEEDS_HUMAN', 'SECURITY_ALERT'
        )
    ),
    source_type TEXT NOT NULL CHECK (source_type IN ('TASK', 'RUN', 'MESSAGE', 'APPROVAL', 'SYSTEM')),
    source_id BLOB NOT NULL
        CHECK (
            typeof(source_id) = 'blob'
            AND length(source_id) = 16
            AND substr(hex(source_id), 13, 1) = '7'
            AND substr(hex(source_id), 17, 1) IN ('8', '9', 'A', 'B')
        ),
    title TEXT NOT NULL CHECK (length(trim(title)) > 0),
    summary TEXT NOT NULL,
    status TEXT NOT NULL CHECK (status IN ('OPEN', 'RESOLVED', 'DISMISSED')),
    dismiss_reason TEXT,
    version INTEGER NOT NULL CHECK (typeof(version) = 'integer' AND version >= 1),
    created_at INTEGER NOT NULL CHECK (typeof(created_at) = 'integer'),
    updated_at INTEGER NOT NULL CHECK (typeof(updated_at) = 'integer'),
    resolved_at INTEGER CHECK (resolved_at IS NULL OR typeof(resolved_at) = 'integer'),
    dismissed_at INTEGER CHECK (dismissed_at IS NULL OR typeof(dismissed_at) = 'integer'),
    CONSTRAINT attention_items_team_id_foreign_key
        FOREIGN KEY (team_id) REFERENCES teams (id)
        ON UPDATE RESTRICT ON DELETE RESTRICT,
    CONSTRAINT attention_dismiss_reason_check
        CHECK (status != 'DISMISSED' OR (dismiss_reason IS NOT NULL AND length(trim(dismiss_reason)) > 0)),
    CONSTRAINT attention_resolved_timestamp_check
        CHECK (status != 'RESOLVED' OR resolved_at IS NOT NULL),
    CONSTRAINT attention_dismissed_timestamp_check
        CHECK (status != 'DISMISSED' OR dismissed_at IS NOT NULL)
);

-- Attention deduplication: Only one OPEN AttentionItem per source object and attention type
CREATE UNIQUE INDEX attention_items_unique_open
ON attention_items (team_id, source_type, source_id, attention_type)
WHERE status = 'OPEN';

CREATE INDEX attention_items_team_status ON attention_items (team_id, status, created_at);
