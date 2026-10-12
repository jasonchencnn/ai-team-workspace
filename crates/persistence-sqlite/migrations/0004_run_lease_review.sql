-- DEV-02-05: Run, Lease, and Review tables.
-- IDs use the 16-byte UUIDv7 BLOB representation frozen in DATA_CONVENTIONS.md;
-- timestamps are UTC Unix milliseconds.

CREATE TABLE runs (
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
    task_id BLOB NOT NULL
        CHECK (
            typeof(task_id) = 'blob'
            AND length(task_id) = 16
            AND substr(hex(task_id), 13, 1) = '7'
            AND substr(hex(task_id), 17, 1) IN ('8', '9', 'A', 'B')
        ),
    role_id BLOB NOT NULL
        CHECK (
            typeof(role_id) = 'blob'
            AND length(role_id) = 16
            AND substr(hex(role_id), 13, 1) = '7'
            AND substr(hex(role_id), 17, 1) IN ('8', '9', 'A', 'B')
        ),
    binding_id BLOB NOT NULL
        CHECK (
            typeof(binding_id) = 'blob'
            AND length(binding_id) = 16
            AND substr(hex(binding_id), 13, 1) = '7'
            AND substr(hex(binding_id), 17, 1) IN ('8', '9', 'A', 'B')
        ),
    harness_instance_id BLOB NOT NULL
        CHECK (
            typeof(harness_instance_id) = 'blob'
            AND length(harness_instance_id) = 16
            AND substr(hex(harness_instance_id), 13, 1) = '7'
            AND substr(hex(harness_instance_id), 17, 1) IN ('8', '9', 'A', 'B')
        ),
    attempt INTEGER NOT NULL CHECK (typeof(attempt) = 'integer' AND attempt >= 1),
    fencing_token INTEGER NOT NULL CHECK (typeof(fencing_token) = 'integer' AND fencing_token >= 1),
    status TEXT NOT NULL CHECK (status IN ('QUEUED', 'STARTING', 'RUNNING', 'SUCCEEDED', 'FAILED', 'TIMED_OUT', 'CANCELLED', 'UNKNOWN')),
    exit_code INTEGER CHECK (exit_code IS NULL OR typeof(exit_code) = 'integer'),
    failure_reason TEXT CHECK (failure_reason IS NULL OR length(trim(failure_reason)) > 0),
    version INTEGER NOT NULL CHECK (typeof(version) = 'integer' AND version >= 1),
    created_at INTEGER NOT NULL CHECK (typeof(created_at) = 'integer'),
    started_at INTEGER CHECK (started_at IS NULL OR typeof(started_at) = 'integer'),
    completed_at INTEGER CHECK (completed_at IS NULL OR typeof(completed_at) = 'integer'),
    CONSTRAINT runs_team_id_foreign_key
        FOREIGN KEY (team_id) REFERENCES teams (id)
        ON UPDATE RESTRICT ON DELETE RESTRICT,
    CONSTRAINT runs_task_id_foreign_key
        FOREIGN KEY (task_id) REFERENCES tasks (id)
        ON UPDATE RESTRICT ON DELETE RESTRICT,
    CONSTRAINT runs_role_id_foreign_key
        FOREIGN KEY (role_id) REFERENCES roles (id)
        ON UPDATE RESTRICT ON DELETE RESTRICT,
    CONSTRAINT runs_binding_id_foreign_key
        FOREIGN KEY (binding_id) REFERENCES bindings (id)
        ON UPDATE RESTRICT ON DELETE RESTRICT,
    CONSTRAINT runs_harness_instance_id_foreign_key
        FOREIGN KEY (harness_instance_id) REFERENCES harness_instances (id)
        ON UPDATE RESTRICT ON DELETE RESTRICT,
    CONSTRAINT runs_task_attempt_unique UNIQUE (task_id, attempt),
    CONSTRAINT runs_task_fencing_token_unique UNIQUE (task_id, fencing_token),
    CHECK (
        (status IN ('SUCCEEDED', 'FAILED', 'TIMED_OUT', 'CANCELLED') AND completed_at IS NOT NULL)
        OR (status NOT IN ('SUCCEEDED', 'FAILED', 'TIMED_OUT', 'CANCELLED') AND completed_at IS NULL)
    ),
    CHECK (
        (status IN ('RUNNING', 'SUCCEEDED', 'FAILED', 'TIMED_OUT', 'CANCELLED') AND started_at IS NOT NULL)
        OR (status IN ('QUEUED', 'STARTING') AND started_at IS NULL)
        OR status = 'UNKNOWN'
    )
);

-- At most one active Run per Task
CREATE UNIQUE INDEX runs_single_active_run_per_task ON runs (task_id) WHERE status IN ('QUEUED', 'STARTING', 'RUNNING', 'UNKNOWN');
CREATE INDEX runs_team_id ON runs (team_id);
CREATE INDEX runs_task_id ON runs (task_id);
CREATE INDEX runs_role_id ON runs (role_id);
CREATE INDEX runs_binding_id ON runs (binding_id);
CREATE INDEX runs_harness_instance_id ON runs (harness_instance_id);

CREATE TABLE leases (
    task_id BLOB PRIMARY KEY NOT NULL
        CHECK (
            typeof(task_id) = 'blob'
            AND length(task_id) = 16
            AND substr(hex(task_id), 13, 1) = '7'
            AND substr(hex(task_id), 17, 1) IN ('8', '9', 'A', 'B')
        ),
    run_id BLOB NOT NULL
        CHECK (
            typeof(run_id) = 'blob'
            AND length(run_id) = 16
            AND substr(hex(run_id), 13, 1) = '7'
            AND substr(hex(run_id), 17, 1) IN ('8', '9', 'A', 'B')
        ),
    fencing_token INTEGER NOT NULL CHECK (typeof(fencing_token) = 'integer' AND fencing_token >= 1),
    acquired_at INTEGER NOT NULL CHECK (typeof(acquired_at) = 'integer'),
    expires_at INTEGER NOT NULL CHECK (typeof(expires_at) = 'integer' AND expires_at >= acquired_at),
    version INTEGER NOT NULL CHECK (typeof(version) = 'integer' AND version >= 1),
    CONSTRAINT leases_task_id_foreign_key
        FOREIGN KEY (task_id) REFERENCES tasks (id)
        ON UPDATE RESTRICT ON DELETE RESTRICT,
    CONSTRAINT leases_run_id_foreign_key
        FOREIGN KEY (run_id) REFERENCES runs (id)
        ON UPDATE RESTRICT ON DELETE RESTRICT,
    CONSTRAINT leases_run_id_unique UNIQUE (run_id)
);

CREATE TABLE reviews (
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
    task_id BLOB NOT NULL
        CHECK (
            typeof(task_id) = 'blob'
            AND length(task_id) = 16
            AND substr(hex(task_id), 13, 1) = '7'
            AND substr(hex(task_id), 17, 1) IN ('8', '9', 'A', 'B')
        ),
    run_id BLOB NOT NULL
        CHECK (
            typeof(run_id) = 'blob'
            AND length(run_id) = 16
            AND substr(hex(run_id), 13, 1) = '7'
            AND substr(hex(run_id), 17, 1) IN ('8', '9', 'A', 'B')
        ),
    reviewer_role_id BLOB NOT NULL
        CHECK (
            typeof(reviewer_role_id) = 'blob'
            AND length(reviewer_role_id) = 16
            AND substr(hex(reviewer_role_id), 13, 1) = '7'
            AND substr(hex(reviewer_role_id), 17, 1) IN ('8', '9', 'A', 'B')
        ),
    decision TEXT NOT NULL CHECK (decision IN ('APPROVED', 'CHANGES_REQUESTED')),
    comments TEXT CHECK (comments IS NULL OR length(trim(comments)) > 0),
    version INTEGER NOT NULL CHECK (typeof(version) = 'integer' AND version >= 1),
    created_at INTEGER NOT NULL CHECK (typeof(created_at) = 'integer'),
    CONSTRAINT reviews_team_id_foreign_key
        FOREIGN KEY (team_id) REFERENCES teams (id)
        ON UPDATE RESTRICT ON DELETE RESTRICT,
    CONSTRAINT reviews_task_id_foreign_key
        FOREIGN KEY (task_id) REFERENCES tasks (id)
        ON UPDATE RESTRICT ON DELETE RESTRICT,
    CONSTRAINT reviews_run_id_foreign_key
        FOREIGN KEY (run_id) REFERENCES runs (id)
        ON UPDATE RESTRICT ON DELETE RESTRICT,
    CONSTRAINT reviews_reviewer_role_id_foreign_key
        FOREIGN KEY (reviewer_role_id) REFERENCES roles (id)
        ON UPDATE RESTRICT ON DELETE RESTRICT,
    CHECK (
        (decision = 'CHANGES_REQUESTED' AND comments IS NOT NULL)
        OR (decision = 'APPROVED')
    )
);

CREATE INDEX reviews_task_id ON reviews (task_id);
CREATE INDEX reviews_run_id ON reviews (run_id);
