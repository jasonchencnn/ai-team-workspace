-- DEV-02-03: Harness, Adapter, and Binding tables.
-- IDs use the 16-byte UUIDv7 BLOB representation frozen in DATA_CONVENTIONS.md;
-- timestamps are UTC Unix milliseconds.

CREATE TABLE harness_instances (
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
    harness_type TEXT NOT NULL CHECK (length(trim(harness_type)) > 0),
    display_name TEXT NOT NULL CHECK (length(trim(display_name)) > 0),
    status TEXT NOT NULL CHECK (status IN ('ACTIVE', 'DISABLED')),
    capabilities TEXT NOT NULL CHECK (length(trim(capabilities)) > 0),
    credential_reference TEXT CHECK (credential_reference IS NULL OR length(trim(credential_reference)) > 0),
    version INTEGER NOT NULL CHECK (typeof(version) = 'integer' AND version >= 1),
    created_at INTEGER NOT NULL CHECK (typeof(created_at) = 'integer'),
    updated_at INTEGER NOT NULL CHECK (typeof(updated_at) = 'integer'),
    disabled_at INTEGER CHECK (disabled_at IS NULL OR typeof(disabled_at) = 'integer'),
    CONSTRAINT harness_instances_team_id_foreign_key
        FOREIGN KEY (team_id) REFERENCES teams (id)
        ON UPDATE RESTRICT ON DELETE RESTRICT,
    CHECK (
        (status = 'DISABLED' AND disabled_at IS NOT NULL)
        OR (status = 'ACTIVE' AND disabled_at IS NULL)
    )
);

CREATE INDEX harness_instances_team_id_status ON harness_instances (team_id, status);

CREATE TABLE adapters (
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
    protocol_version TEXT NOT NULL CHECK (length(trim(protocol_version)) > 0),
    executable_path TEXT NOT NULL CHECK (length(trim(executable_path)) > 0),
    manifest_json TEXT NOT NULL CHECK (length(trim(manifest_json)) > 0),
    version INTEGER NOT NULL CHECK (typeof(version) = 'integer' AND version >= 1),
    created_at INTEGER NOT NULL CHECK (typeof(created_at) = 'integer'),
    updated_at INTEGER NOT NULL CHECK (typeof(updated_at) = 'integer'),
    CONSTRAINT adapters_team_id_foreign_key
        FOREIGN KEY (team_id) REFERENCES teams (id)
        ON UPDATE RESTRICT ON DELETE RESTRICT
);

CREATE INDEX adapters_team_id ON adapters (team_id);

CREATE TABLE bindings (
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
    role_id BLOB NOT NULL
        CHECK (
            typeof(role_id) = 'blob'
            AND length(role_id) = 16
            AND substr(hex(role_id), 13, 1) = '7'
            AND substr(hex(role_id), 17, 1) IN ('8', '9', 'A', 'B')
        ),
    harness_instance_id BLOB NOT NULL
        CHECK (
            typeof(harness_instance_id) = 'blob'
            AND length(harness_instance_id) = 16
            AND substr(hex(harness_instance_id), 13, 1) = '7'
            AND substr(hex(harness_instance_id), 17, 1) IN ('8', '9', 'A', 'B')
        ),
    adapter_id BLOB NOT NULL
        CHECK (
            typeof(adapter_id) = 'blob'
            AND length(adapter_id) = 16
            AND substr(hex(adapter_id), 13, 1) = '7'
            AND substr(hex(adapter_id), 17, 1) IN ('8', '9', 'A', 'B')
        ),
    status TEXT NOT NULL CHECK (status IN ('PENDING_CHECK', 'ACTIVE', 'DRAINING', 'REVOKED', 'FAILED')),
    binding_generation INTEGER NOT NULL CHECK (typeof(binding_generation) = 'integer' AND binding_generation >= 1),
    version INTEGER NOT NULL CHECK (typeof(version) = 'integer' AND version >= 1),
    configuration_snapshot TEXT NOT NULL CHECK (length(trim(configuration_snapshot)) > 0),
    created_at INTEGER NOT NULL CHECK (typeof(created_at) = 'integer'),
    updated_at INTEGER NOT NULL CHECK (typeof(updated_at) = 'integer'),
    effective_from INTEGER CHECK (effective_from IS NULL OR typeof(effective_from) = 'integer'),
    effective_to INTEGER CHECK (effective_to IS NULL OR typeof(effective_to) = 'integer'),
    CONSTRAINT bindings_team_id_foreign_key
        FOREIGN KEY (team_id) REFERENCES teams (id)
        ON UPDATE RESTRICT ON DELETE RESTRICT,
    CONSTRAINT bindings_role_id_foreign_key
        FOREIGN KEY (role_id) REFERENCES roles (id)
        ON UPDATE RESTRICT ON DELETE RESTRICT,
    CONSTRAINT bindings_harness_instance_id_foreign_key
        FOREIGN KEY (harness_instance_id) REFERENCES harness_instances (id)
        ON UPDATE RESTRICT ON DELETE RESTRICT,
    CONSTRAINT bindings_adapter_id_foreign_key
        FOREIGN KEY (adapter_id) REFERENCES adapters (id)
        ON UPDATE RESTRICT ON DELETE RESTRICT,
    CHECK (
        (status IN ('ACTIVE', 'DRAINING') AND effective_from IS NOT NULL AND effective_to IS NULL)
        OR (status = 'REVOKED' AND (
            (effective_from IS NOT NULL AND effective_to IS NOT NULL AND effective_to >= effective_from)
            OR (effective_from IS NULL AND effective_to IS NULL)
        ))
        OR (status IN ('PENDING_CHECK', 'FAILED') AND effective_from IS NULL AND effective_to IS NULL)
    )
);

-- At most one ACTIVE binding per role at any moment
CREATE UNIQUE INDEX bindings_single_active_per_role ON bindings (role_id) WHERE status = 'ACTIVE';

-- Generation numbers are strictly unique and monotonic per role
CREATE UNIQUE INDEX bindings_role_generation_unique ON bindings (role_id, binding_generation);

CREATE INDEX bindings_team_id ON bindings (team_id);
CREATE INDEX bindings_role_id ON bindings (role_id);
CREATE INDEX bindings_harness_instance_id ON bindings (harness_instance_id);
CREATE INDEX bindings_adapter_id ON bindings (adapter_id);

-- Enforce non-overlapping effective ranges [effective_from, effective_to] per role
CREATE TRIGGER bindings_no_overlapping_ranges_insert
BEFORE INSERT ON bindings
FOR EACH ROW
WHEN NEW.effective_from IS NOT NULL
BEGIN
    SELECT RAISE(ABORT, 'OVERLAPPING_BINDING_EFFECTIVE_RANGE')
    FROM bindings
    WHERE role_id = NEW.role_id
      AND id != NEW.id
      AND effective_from IS NOT NULL
      AND (
          (effective_to IS NULL AND (NEW.effective_to IS NULL OR NEW.effective_to >= effective_from))
          OR
          (NEW.effective_to IS NULL AND effective_to >= NEW.effective_from)
          OR
          (effective_to IS NOT NULL AND NEW.effective_to IS NOT NULL
           AND max(effective_from, NEW.effective_from) <= min(effective_to, NEW.effective_to))
      );
END;

CREATE TRIGGER bindings_no_overlapping_ranges_update
BEFORE UPDATE ON bindings
FOR EACH ROW
WHEN NEW.effective_from IS NOT NULL
BEGIN
    SELECT RAISE(ABORT, 'OVERLAPPING_BINDING_EFFECTIVE_RANGE')
    FROM bindings
    WHERE role_id = NEW.role_id
      AND id != NEW.id
      AND effective_from IS NOT NULL
      AND (
          (effective_to IS NULL AND (NEW.effective_to IS NULL OR NEW.effective_to >= effective_from))
          OR
          (NEW.effective_to IS NULL AND effective_to >= NEW.effective_from)
          OR
          (effective_to IS NOT NULL AND NEW.effective_to IS NOT NULL
           AND max(effective_from, NEW.effective_from) <= min(effective_to, NEW.effective_to))
      );
END;
