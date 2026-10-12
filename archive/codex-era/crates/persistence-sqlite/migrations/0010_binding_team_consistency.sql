-- DEV-02-03: Binding references must remain inside their Team isolation boundary.
-- SQLite foreign keys validate existence, but cannot express that all referenced
-- rows have the same team_id. The guard makes an upgrade fail atomically when
-- an older database already contains an invalid relationship; it never deletes
-- or rewrites historical Binding data.

CREATE TABLE binding_team_consistency_upgrade_guard (
    id INTEGER PRIMARY KEY CHECK (id = 1),
    valid INTEGER NOT NULL CHECK (valid = 1)
);

INSERT INTO binding_team_consistency_upgrade_guard (id, valid)
SELECT 1, 0
WHERE EXISTS (
    SELECT 1
    FROM bindings AS binding
    LEFT JOIN roles AS role ON role.id = binding.role_id
    LEFT JOIN harness_instances AS harness ON harness.id = binding.harness_instance_id
    LEFT JOIN adapters AS adapter ON adapter.id = binding.adapter_id
    WHERE role.team_id IS NOT binding.team_id
       OR harness.team_id IS NOT binding.team_id
       OR adapter.team_id IS NOT binding.team_id
);

DROP TABLE binding_team_consistency_upgrade_guard;

CREATE TRIGGER bindings_team_consistency_insert
BEFORE INSERT ON bindings
FOR EACH ROW
WHEN NOT (
    EXISTS (SELECT 1 FROM roles WHERE id = NEW.role_id AND team_id = NEW.team_id)
    AND EXISTS (SELECT 1 FROM harness_instances WHERE id = NEW.harness_instance_id AND team_id = NEW.team_id)
    AND EXISTS (SELECT 1 FROM adapters WHERE id = NEW.adapter_id AND team_id = NEW.team_id)
)
BEGIN
    SELECT RAISE(ABORT, 'binding Team consistency violation');
END;

CREATE TRIGGER bindings_team_consistency_update
BEFORE UPDATE ON bindings
FOR EACH ROW
WHEN NOT (
    EXISTS (SELECT 1 FROM roles WHERE id = NEW.role_id AND team_id = NEW.team_id)
    AND EXISTS (SELECT 1 FROM harness_instances WHERE id = NEW.harness_instance_id AND team_id = NEW.team_id)
    AND EXISTS (SELECT 1 FROM adapters WHERE id = NEW.adapter_id AND team_id = NEW.team_id)
)
BEGIN
    SELECT RAISE(ABORT, 'binding Team consistency violation');
END;

CREATE TRIGGER roles_binding_team_consistency_update
BEFORE UPDATE OF team_id ON roles
FOR EACH ROW
WHEN EXISTS (
    SELECT 1 FROM bindings WHERE role_id = NEW.id AND team_id != NEW.team_id
)
BEGIN
    SELECT RAISE(ABORT, 'binding Team consistency violation');
END;

CREATE TRIGGER harness_instances_binding_team_consistency_update
BEFORE UPDATE OF team_id ON harness_instances
FOR EACH ROW
WHEN EXISTS (
    SELECT 1 FROM bindings WHERE harness_instance_id = NEW.id AND team_id != NEW.team_id
)
BEGIN
    SELECT RAISE(ABORT, 'binding Team consistency violation');
END;

CREATE TRIGGER adapters_binding_team_consistency_update
BEFORE UPDATE OF team_id ON adapters
FOR EACH ROW
WHEN EXISTS (
    SELECT 1 FROM bindings WHERE adapter_id = NEW.id AND team_id != NEW.team_id
)
BEGIN
    SELECT RAISE(ABORT, 'binding Team consistency violation');
END;
