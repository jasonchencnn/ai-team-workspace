-- DEV-02-04: Task and Task Dependency tables.
-- IDs use the 16-byte UUIDv7 BLOB representation frozen in DATA_CONVENTIONS.md;
-- timestamps are UTC Unix milliseconds.

CREATE TABLE tasks (
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
    owner_role_id BLOB NOT NULL
        CHECK (
            typeof(owner_role_id) = 'blob'
            AND length(owner_role_id) = 16
            AND substr(hex(owner_role_id), 13, 1) = '7'
            AND substr(hex(owner_role_id), 17, 1) IN ('8', '9', 'A', 'B')
        ),
    display_id TEXT NOT NULL CHECK (length(trim(display_id)) > 0),
    title TEXT NOT NULL CHECK (length(trim(title)) > 0),
    description TEXT NOT NULL,
    status TEXT NOT NULL CHECK (status IN ('TODO', 'READY', 'DOING', 'BLOCKED', 'REVIEW', 'DONE', 'CANCELLED')),
    version INTEGER NOT NULL CHECK (typeof(version) = 'integer' AND version >= 1),
    created_at INTEGER NOT NULL CHECK (typeof(created_at) = 'integer'),
    updated_at INTEGER NOT NULL CHECK (typeof(updated_at) = 'integer'),
    cancelled_at INTEGER CHECK (cancelled_at IS NULL OR typeof(cancelled_at) = 'integer'),
    completed_at INTEGER CHECK (completed_at IS NULL OR typeof(completed_at) = 'integer'),
    CONSTRAINT tasks_team_id_foreign_key
        FOREIGN KEY (team_id) REFERENCES teams (id)
        ON UPDATE RESTRICT ON DELETE RESTRICT,
    CONSTRAINT tasks_owner_role_id_foreign_key
        FOREIGN KEY (owner_role_id) REFERENCES roles (id)
        ON UPDATE RESTRICT ON DELETE RESTRICT,
    CONSTRAINT tasks_team_display_id_unique UNIQUE (team_id, display_id),
    CHECK (
        (status = 'CANCELLED' AND cancelled_at IS NOT NULL)
        OR (status != 'CANCELLED' AND cancelled_at IS NULL)
    ),
    CHECK (
        (status = 'DONE' AND completed_at IS NOT NULL)
        OR (status != 'DONE' AND completed_at IS NULL)
    )
);

CREATE INDEX tasks_team_id_status ON tasks (team_id, status);
CREATE INDEX tasks_owner_role_id ON tasks (owner_role_id);

CREATE TABLE task_dependencies (
    team_id BLOB NOT NULL
        CHECK (
            typeof(team_id) = 'blob'
            AND length(team_id) = 16
            AND substr(hex(team_id), 13, 1) = '7'
            AND substr(hex(team_id), 17, 1) IN ('8', '9', 'A', 'B')
        ),
    task_id BLOB NOT NULL
        CHECK (
            typeof(task_id) = 'blob'
            AND length(task_id) = 16
            AND substr(hex(task_id), 13, 1) = '7'
            AND substr(hex(task_id), 17, 1) IN ('8', '9', 'A', 'B')
        ),
    depends_on_task_id BLOB NOT NULL
        CHECK (
            typeof(depends_on_task_id) = 'blob'
            AND length(depends_on_task_id) = 16
            AND substr(hex(depends_on_task_id), 13, 1) = '7'
            AND substr(hex(depends_on_task_id), 17, 1) IN ('8', '9', 'A', 'B')
        ),
    dependency_type TEXT NOT NULL CHECK (dependency_type IN ('BLOCKING', 'NON_BLOCKING')),
    created_at INTEGER NOT NULL CHECK (typeof(created_at) = 'integer'),
    PRIMARY KEY (task_id, depends_on_task_id),
    CONSTRAINT task_dependencies_team_id_foreign_key
        FOREIGN KEY (team_id) REFERENCES teams (id)
        ON UPDATE RESTRICT ON DELETE RESTRICT,
    CONSTRAINT task_dependencies_task_id_foreign_key
        FOREIGN KEY (task_id) REFERENCES tasks (id)
        ON UPDATE RESTRICT ON DELETE RESTRICT,
    CONSTRAINT task_dependencies_depends_on_task_id_foreign_key
        FOREIGN KEY (depends_on_task_id) REFERENCES tasks (id)
        ON UPDATE RESTRICT ON DELETE RESTRICT,
    CHECK (task_id != depends_on_task_id)
);

CREATE INDEX task_dependencies_team_id ON task_dependencies (team_id);
CREATE INDEX task_dependencies_depends_on ON task_dependencies (depends_on_task_id);

-- Enforce no immediate 2-node cycle (A depends on B and B depends on A)
CREATE TRIGGER task_dependencies_no_direct_cycle_insert
BEFORE INSERT ON task_dependencies
FOR EACH ROW
BEGIN
    SELECT RAISE(ABORT, 'CIRCULAR_TASK_DEPENDENCY')
    FROM task_dependencies
    WHERE task_id = NEW.depends_on_task_id
      AND depends_on_task_id = NEW.task_id;
END;
