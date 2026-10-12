-- DEV-02-10: Permission Policy and Human Approval tables.
-- IDs use the 16-byte UUIDv7 BLOB representation frozen in DATA_CONVENTIONS.md;
-- timestamps are UTC Unix milliseconds.

CREATE TABLE permissions (
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
    role_id BLOB
        CHECK (
            role_id IS NULL
            OR (
                typeof(role_id) = 'blob'
                AND length(role_id) = 16
                AND substr(hex(role_id), 13, 1) = '7'
                AND substr(hex(role_id), 17, 1) IN ('8', '9', 'A', 'B')
            )
        ),
    resource_type TEXT NOT NULL CHECK (
        resource_type IN (
            'TEAM', 'ROLE', 'WORKSPACE', 'TASK', 'RUN', 'ARTIFACT',
            'MESSAGE', 'NOTIFICATION', 'APPROVAL', 'CREDENTIAL', 'SYSTEM'
        )
    ),
    action TEXT NOT NULL CHECK (
        action IN (
            'CREATE', 'READ', 'UPDATE', 'DELETE', 'EXECUTE',
            'APPROVE', 'CLAIM', 'CANCEL', 'RETRY'
        )
    ),
    scope_pattern TEXT NOT NULL DEFAULT '*' CHECK (length(trim(scope_pattern)) > 0),
    effect TEXT NOT NULL CHECK (effect IN ('ALLOW', 'DENY')),
    is_human_only INTEGER NOT NULL DEFAULT 0 CHECK (is_human_only IN (0, 1)),
    version INTEGER NOT NULL CHECK (typeof(version) = 'integer' AND version >= 1),
    created_at INTEGER NOT NULL CHECK (typeof(created_at) = 'integer'),
    updated_at INTEGER NOT NULL CHECK (typeof(updated_at) = 'integer'),
    CONSTRAINT permissions_team_id_foreign_key
        FOREIGN KEY (team_id) REFERENCES teams (id)
        ON UPDATE RESTRICT ON DELETE RESTRICT,
    CONSTRAINT permissions_role_id_foreign_key
        FOREIGN KEY (role_id) REFERENCES roles (id)
        ON UPDATE RESTRICT ON DELETE RESTRICT
);

CREATE INDEX permissions_team_role ON permissions (team_id, role_id, resource_type, action);

CREATE TABLE approvals (
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
    requester_actor_type TEXT NOT NULL CHECK (requester_actor_type IN ('ROLE', 'HUMAN', 'SYSTEM')),
    requester_role_id BLOB
        CHECK (
            requester_role_id IS NULL
            OR (
                typeof(requester_role_id) = 'blob'
                AND length(requester_role_id) = 16
                AND substr(hex(requester_role_id), 13, 1) = '7'
                AND substr(hex(requester_role_id), 17, 1) IN ('8', '9', 'A', 'B')
            )
        ),
    requester_human_id BLOB
        CHECK (
            requester_human_id IS NULL
            OR (
                typeof(requester_human_id) = 'blob'
                AND length(requester_human_id) = 16
                AND substr(hex(requester_human_id), 13, 1) = '7'
                AND substr(hex(requester_human_id), 17, 1) IN ('8', '9', 'A', 'B')
            )
        ),
    action TEXT NOT NULL CHECK (length(trim(action)) > 0),
    resource_type TEXT NOT NULL CHECK (
        resource_type IN (
            'WORKSPACE_PATH', 'TASK_MUTATION', 'ROLE_MUTATION',
            'CREDENTIAL_ACCESS', 'EXTERNAL_DISPATCH', 'TEAM_ARCHIVE'
        )
    ),
    resource_scope TEXT NOT NULL CHECK (length(trim(resource_scope)) > 0),
    reason TEXT NOT NULL CHECK (length(trim(reason)) > 0),
    status TEXT NOT NULL CHECK (status IN ('PENDING', 'APPROVED', 'REJECTED', 'EXPIRED', 'CANCELLED')),
    decided_by_human_id BLOB
        CHECK (
            decided_by_human_id IS NULL
            OR (
                typeof(decided_by_human_id) = 'blob'
                AND length(decided_by_human_id) = 16
                AND substr(hex(decided_by_human_id), 13, 1) = '7'
                AND substr(hex(decided_by_human_id), 17, 1) IN ('8', '9', 'A', 'B')
            )
        ),
    decision_notes TEXT,
    version INTEGER NOT NULL CHECK (typeof(version) = 'integer' AND version >= 1),
    expires_at INTEGER NOT NULL CHECK (typeof(expires_at) = 'integer'),
    created_at INTEGER NOT NULL CHECK (typeof(created_at) = 'integer'),
    decided_at INTEGER CHECK (decided_at IS NULL OR typeof(decided_at) = 'integer'),
    CONSTRAINT approvals_team_id_foreign_key
        FOREIGN KEY (team_id) REFERENCES teams (id)
        ON UPDATE RESTRICT ON DELETE RESTRICT,
    CONSTRAINT approvals_requester_role_id_foreign_key
        FOREIGN KEY (requester_role_id) REFERENCES roles (id)
        ON UPDATE RESTRICT ON DELETE RESTRICT,
    -- Self-approval is strictly prohibited: Human requester cannot approve their own request
    CONSTRAINT approvals_no_self_approval
        CHECK (decided_by_human_id IS NULL OR requester_human_id IS NULL OR decided_by_human_id != requester_human_id),
    -- Human decision requires decided_by_human_id and decided_at
    CONSTRAINT approvals_decision_requires_human
        CHECK (status NOT IN ('APPROVED', 'REJECTED') OR (decided_by_human_id IS NOT NULL AND decided_at IS NOT NULL)),
    -- Rejection requires decision_notes
    CONSTRAINT approvals_rejection_requires_notes
        CHECK (status != 'REJECTED' OR (decision_notes IS NOT NULL AND length(trim(decision_notes)) > 0))
);

CREATE INDEX approvals_team_status ON approvals (team_id, status, created_at);
CREATE INDEX approvals_pending_expiry ON approvals (status, expires_at) WHERE status = 'PENDING';
