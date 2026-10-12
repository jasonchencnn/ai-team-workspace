-- DEV-02-02: immutable identities and lifecycle metadata for the first
-- Team-scoped aggregates. IDs use the 16-byte UUIDv7 representation frozen in
-- DATA_CONVENTIONS.md; timestamps are UTC Unix milliseconds.

CREATE TABLE teams (
    id BLOB PRIMARY KEY NOT NULL
        CHECK (
            typeof(id) = 'blob'
            AND length(id) = 16
            AND substr(hex(id), 13, 1) = '7'
            AND substr(hex(id), 17, 1) IN ('8', '9', 'A', 'B')
        ),
    name TEXT NOT NULL CHECK (length(trim(name)) > 0),
    status TEXT NOT NULL CHECK (status IN ('ACTIVE', 'PAUSED', 'ARCHIVED')),
    version INTEGER NOT NULL CHECK (typeof(version) = 'integer' AND version >= 1),
    created_at INTEGER NOT NULL CHECK (typeof(created_at) = 'integer'),
    updated_at INTEGER NOT NULL CHECK (typeof(updated_at) = 'integer'),
    archived_at INTEGER CHECK (archived_at IS NULL OR typeof(archived_at) = 'integer'),
    CHECK (
        (status = 'ARCHIVED' AND archived_at IS NOT NULL)
        OR (status IN ('ACTIVE', 'PAUSED') AND archived_at IS NULL)
    )
);

CREATE TABLE roles (
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
    display_id TEXT NOT NULL CHECK (length(trim(display_id)) > 0),
    name TEXT NOT NULL CHECK (length(trim(name)) > 0),
    status TEXT NOT NULL CHECK (status IN ('ACTIVE', 'DEACTIVATED')),
    version INTEGER NOT NULL CHECK (typeof(version) = 'integer' AND version >= 1),
    created_at INTEGER NOT NULL CHECK (typeof(created_at) = 'integer'),
    updated_at INTEGER NOT NULL CHECK (typeof(updated_at) = 'integer'),
    deactivated_at INTEGER CHECK (deactivated_at IS NULL OR typeof(deactivated_at) = 'integer'),
    CONSTRAINT roles_team_display_id_unique UNIQUE (team_id, display_id),
    CONSTRAINT roles_team_id_foreign_key
        FOREIGN KEY (team_id) REFERENCES teams (id)
        ON UPDATE RESTRICT ON DELETE RESTRICT,
    CHECK (
        (status = 'DEACTIVATED' AND deactivated_at IS NOT NULL)
        OR (status = 'ACTIVE' AND deactivated_at IS NULL)
    )
);

CREATE INDEX roles_by_team_and_status ON roles (team_id, status);

CREATE TABLE workspaces (
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
    kind TEXT NOT NULL CHECK (length(trim(kind)) > 0),
    root_reference TEXT NOT NULL CHECK (length(trim(root_reference)) > 0),
    validation_status TEXT NOT NULL CHECK (length(trim(validation_status)) > 0),
    version INTEGER NOT NULL CHECK (typeof(version) = 'integer' AND version >= 1),
    created_at INTEGER NOT NULL CHECK (typeof(created_at) = 'integer'),
    updated_at INTEGER NOT NULL CHECK (typeof(updated_at) = 'integer'),
    CONSTRAINT workspaces_team_id_unique UNIQUE (team_id),
    CONSTRAINT workspaces_team_id_foreign_key
        FOREIGN KEY (team_id) REFERENCES teams (id)
        ON UPDATE RESTRICT ON DELETE RESTRICT
);
