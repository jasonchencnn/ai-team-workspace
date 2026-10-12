-- DEV-02-08: immutable per-attempt route history for authorized reassignment.
-- Frozen basis: STATE_MACHINES section 5; DOMAIN_MODEL Team/Message/Attempt;
-- DATA_CONVENTIONS sections 3-6/8 and PERMISSION_MATRIX section 3.
-- Forward-only 0030; SQLx registers no_tx=false so all replacements are atomic.
-- Existing attempts adopt NULL/NULL (original Message addressing); their old
-- fields remain intact. Each override preserves its actual Role/Binding/Event.
-- Derived from the independently audited AF-02 concept. Event/Team/subject/
-- request association and INTEGER route ordinals are enforced at this boundary.
-- Application actor/permission/TTL/first-result checks remain application work.
-- After application, a 29-only loader rejects version 30; recovery requires an
-- audited forward repair or a consistent backup, not dropped guards/ledger rows.

CREATE TABLE message_route_overrides (
    id BLOB PRIMARY KEY NOT NULL
        CHECK (
            typeof(id) = 'blob' AND length(id) = 16
            AND substr(hex(id), 13, 1) = '7'
            AND substr(hex(id), 17, 1) IN ('8', '9', 'A', 'B')
        ),
    team_id BLOB NOT NULL
        CHECK (
            typeof(team_id) = 'blob' AND length(team_id) = 16
            AND substr(hex(team_id), 13, 1) = '7'
            AND substr(hex(team_id), 17, 1) IN ('8', '9', 'A', 'B')
        ),
    message_id BLOB NOT NULL
        CHECK (
            typeof(message_id) = 'blob' AND length(message_id) = 16
            AND substr(hex(message_id), 13, 1) = '7'
            AND substr(hex(message_id), 17, 1) IN ('8', '9', 'A', 'B')
        ),

    override_number INTEGER NOT NULL
        CHECK (typeof(override_number) = 'integer' AND override_number >= 1),

    from_role_id BLOB NOT NULL
        CHECK (
            typeof(from_role_id) = 'blob' AND length(from_role_id) = 16
            AND substr(hex(from_role_id), 13, 1) = '7'
            AND substr(hex(from_role_id), 17, 1) IN ('8', '9', 'A', 'B')
        ),
    to_role_id BLOB NOT NULL
        CHECK (
            typeof(to_role_id) = 'blob' AND length(to_role_id) = 16
            AND substr(hex(to_role_id), 13, 1) = '7'
            AND substr(hex(to_role_id), 17, 1) IN ('8', '9', 'A', 'B')
        ),

    target_binding_id BLOB NOT NULL
        CHECK (
            typeof(target_binding_id) = 'blob' AND length(target_binding_id) = 16
            AND substr(hex(target_binding_id), 13, 1) = '7'
            AND substr(hex(target_binding_id), 17, 1) IN ('8', '9', 'A', 'B')
        ),
    target_binding_generation INTEGER NOT NULL
        CHECK (typeof(target_binding_generation) = 'integer'
               AND target_binding_generation >= 1),

    actor_fingerprint BLOB NOT NULL
        CHECK (typeof(actor_fingerprint) = 'blob' AND length(actor_fingerprint) = 32),
    command_name TEXT NOT NULL
        CHECK (length(trim(command_name)) > 0 AND length(command_name) <= 64),
    idempotency_key TEXT NOT NULL
        CHECK (length(trim(idempotency_key)) > 0 AND length(idempotency_key) <= 128),

    event_id BLOB NOT NULL
        CHECK (
            typeof(event_id) = 'blob' AND length(event_id) = 16
            AND substr(hex(event_id), 13, 1) = '7'
            AND substr(hex(event_id), 17, 1) IN ('8', '9', 'A', 'B')
        ),
    request_id BLOB NOT NULL
        CHECK (
            typeof(request_id) = 'blob' AND length(request_id) = 16
            AND substr(hex(request_id), 13, 1) = '7'
            AND substr(hex(request_id), 17, 1) IN ('8', '9', 'A', 'B')
        ),

    created_at INTEGER NOT NULL CHECK (typeof(created_at) = 'integer'),

    CHECK (from_role_id IS NOT to_role_id),

    CONSTRAINT message_route_overrides_message_fk
        FOREIGN KEY (message_id) REFERENCES messages (id)
        ON UPDATE RESTRICT ON DELETE RESTRICT,
    CONSTRAINT message_route_overrides_team_fk
        FOREIGN KEY (team_id) REFERENCES teams (id)
        ON UPDATE RESTRICT ON DELETE RESTRICT,
    CONSTRAINT message_route_overrides_from_role_fk
        FOREIGN KEY (from_role_id) REFERENCES roles (id)
        ON UPDATE RESTRICT ON DELETE RESTRICT,
    CONSTRAINT message_route_overrides_to_role_fk
        FOREIGN KEY (to_role_id) REFERENCES roles (id)
        ON UPDATE RESTRICT ON DELETE RESTRICT,
    CONSTRAINT message_route_overrides_target_binding_fk
        FOREIGN KEY (target_binding_id) REFERENCES bindings (id)
        ON UPDATE RESTRICT ON DELETE RESTRICT,
    CONSTRAINT message_route_overrides_event_fk
        FOREIGN KEY (event_id) REFERENCES events (id)
        ON UPDATE RESTRICT ON DELETE RESTRICT,
    CONSTRAINT message_route_overrides_idempotency_fk
        FOREIGN KEY (team_id, actor_fingerprint, command_name, idempotency_key)
        REFERENCES command_idempotency (team_id, actor_fingerprint, command_name, idempotency_key)
        ON UPDATE RESTRICT ON DELETE RESTRICT
);

CREATE UNIQUE INDEX message_route_overrides_message_number
    ON message_route_overrides (message_id, override_number);

CREATE INDEX message_route_overrides_message
    ON message_route_overrides (message_id, created_at);
CREATE INDEX message_route_overrides_team
    ON message_route_overrides (team_id);

ALTER TABLE delivery_attempts ADD COLUMN route_override_id BLOB
    REFERENCES message_route_overrides (id)
    ON UPDATE RESTRICT ON DELETE RESTRICT;

ALTER TABLE delivery_attempts ADD COLUMN route_override_number INTEGER
    CHECK (route_override_number IS NULL OR (typeof(route_override_number) = 'integer' AND route_override_number >= 1));

CREATE TRIGGER message_route_overrides_contiguous_insert
BEFORE INSERT ON message_route_overrides
FOR EACH ROW
WHEN NEW.override_number != 1 + (
        SELECT COUNT(*)
        FROM message_route_overrides AS existing
        WHERE existing.message_id = NEW.message_id)
BEGIN
    SELECT RAISE(ABORT, 'Route override sequence must be contiguous from 1');
END;

CREATE TRIGGER message_route_overrides_predecessor_insert
BEFORE INSERT ON message_route_overrides
FOR EACH ROW
WHEN (NEW.override_number = 1 AND NEW.from_role_id IS NOT (
          SELECT message.to_role_id FROM messages AS message
          WHERE message.id = NEW.message_id
      ))
  OR (NEW.override_number > 1 AND NEW.from_role_id IS NOT (
          SELECT previous.to_role_id FROM message_route_overrides AS previous
          WHERE previous.message_id = NEW.message_id
            AND previous.override_number = NEW.override_number - 1
      ))
BEGIN
    SELECT RAISE(ABORT, 'Route override does not continue the predecessor chain');
END;

CREATE TRIGGER message_route_overrides_target_insert
BEFORE INSERT ON message_route_overrides
FOR EACH ROW
WHEN NOT EXISTS (
        SELECT 1 FROM messages AS message
        WHERE message.id = NEW.message_id AND message.team_id = NEW.team_id)
  OR NOT EXISTS (
        SELECT 1 FROM roles AS role
        WHERE role.id = NEW.from_role_id AND role.team_id = NEW.team_id)
  OR NOT EXISTS (
        SELECT 1 FROM roles AS role
        WHERE role.id = NEW.to_role_id AND role.team_id = NEW.team_id)
  OR NOT EXISTS (
        SELECT 1 FROM bindings AS binding
        WHERE binding.id = NEW.target_binding_id
          AND binding.team_id = NEW.team_id
          AND binding.role_id = NEW.to_role_id
          AND binding.binding_generation = NEW.target_binding_generation
          AND binding.status = 'ACTIVE')
  OR NOT EXISTS (
        SELECT 1 FROM events AS event
        WHERE event.id = NEW.event_id
          AND event.team_id = NEW.team_id
          AND event.subject_type = 'MESSAGE'
          AND event.subject_id = NEW.message_id
          AND event.request_id = NEW.request_id)
BEGIN
    SELECT RAISE(ABORT, 'Route override target invalid');
END;

CREATE TRIGGER message_route_overrides_no_delete_guard
BEFORE DELETE ON message_route_overrides
FOR EACH ROW
BEGIN
    SELECT RAISE(ABORT, 'Route override history is retained with the Team');
END;

CREATE TRIGGER message_route_overrides_replace_guard
BEFORE INSERT ON message_route_overrides
FOR EACH ROW
WHEN EXISTS (
    SELECT 1 FROM message_route_overrides AS existing
    WHERE existing.id = NEW.id
)
 OR EXISTS (
    SELECT 1 FROM message_route_overrides AS existing
    WHERE existing.team_id = NEW.team_id
      AND existing.actor_fingerprint = NEW.actor_fingerprint
      AND existing.command_name = NEW.command_name
      AND existing.idempotency_key = NEW.idempotency_key
)
BEGIN
    SELECT RAISE(ABORT, 'Route override and its idempotency key are written once');
END;

CREATE TRIGGER message_route_overrides_identity_immutable_update
BEFORE UPDATE ON message_route_overrides
FOR EACH ROW
WHEN NEW.id IS NOT OLD.id
  OR NEW.team_id IS NOT OLD.team_id
  OR NEW.message_id IS NOT OLD.message_id
  OR NEW.override_number IS NOT OLD.override_number
  OR NEW.from_role_id IS NOT OLD.from_role_id
  OR NEW.to_role_id IS NOT OLD.to_role_id
  OR NEW.target_binding_id IS NOT OLD.target_binding_id
  OR NEW.target_binding_generation IS NOT OLD.target_binding_generation
  OR NEW.actor_fingerprint IS NOT OLD.actor_fingerprint
  OR NEW.command_name IS NOT OLD.command_name
  OR NEW.idempotency_key IS NOT OLD.idempotency_key
  OR NEW.event_id IS NOT OLD.event_id
  OR NEW.request_id IS NOT OLD.request_id
  OR NEW.created_at IS NOT OLD.created_at
BEGIN
    SELECT RAISE(ABORT, 'Route override history is immutable');
END;

CREATE TRIGGER delivery_attempts_route_override_insert
BEFORE INSERT ON delivery_attempts
FOR EACH ROW
WHEN (NEW.route_override_id IS NULL) != (NEW.route_override_number IS NULL)
  OR (NEW.route_override_id IS NOT NULL AND NOT EXISTS (
        SELECT 1 FROM message_route_overrides AS override
        WHERE override.id = NEW.route_override_id
          AND override.message_id = NEW.message_id
          AND override.team_id = NEW.team_id
          AND override.override_number = NEW.route_override_number
          AND override.to_role_id = NEW.target_role_id
          AND override.target_binding_id IS NEW.target_binding_id
          AND override.target_binding_generation IS NEW.target_binding_generation
      ))
BEGIN
    SELECT RAISE(ABORT, 'Delivery attempt does not match the override it adopted');
END;

DROP TRIGGER delivery_attempts_team_consistency_insert;

CREATE TRIGGER delivery_attempts_team_consistency_insert
BEFORE INSERT ON delivery_attempts
FOR EACH ROW
WHEN NOT (
        (NEW.route_override_id IS NULL AND EXISTS (
            SELECT 1 FROM messages AS message
            WHERE message.id = NEW.message_id
              AND message.team_id = NEW.team_id
              AND message.to_role_id = NEW.target_role_id))
     OR (NEW.route_override_id IS NOT NULL AND EXISTS (
            SELECT 1
            FROM message_route_overrides AS override
            JOIN messages AS message ON message.id = override.message_id
            WHERE override.id = NEW.route_override_id
              AND override.message_id = NEW.message_id
              AND override.team_id = NEW.team_id
              AND message.team_id = NEW.team_id
              AND override.to_role_id = NEW.target_role_id))
      )
  OR (NEW.target_binding_id IS NOT NULL AND NOT EXISTS (
          SELECT 1 FROM bindings AS binding
          WHERE binding.id = NEW.target_binding_id AND binding.team_id = NEW.team_id
      ))
BEGIN
    SELECT RAISE(ABORT, 'Delivery attempt Team consistency violation');
END;

DROP TRIGGER delivery_attempts_team_consistency_update;

CREATE TRIGGER delivery_attempts_team_consistency_update
BEFORE UPDATE ON delivery_attempts
FOR EACH ROW
WHEN NOT (
        (NEW.route_override_id IS NULL AND EXISTS (
            SELECT 1 FROM messages AS message
            WHERE message.id = NEW.message_id
              AND message.team_id = NEW.team_id
              AND message.to_role_id = NEW.target_role_id))
     OR (NEW.route_override_id IS NOT NULL AND EXISTS (
            SELECT 1
            FROM message_route_overrides AS override
            JOIN messages AS message ON message.id = override.message_id
            WHERE override.id = NEW.route_override_id
              AND override.message_id = NEW.message_id
              AND override.team_id = NEW.team_id
              AND message.team_id = NEW.team_id
              AND override.to_role_id = NEW.target_role_id))
      )
  OR (NEW.target_binding_id IS NOT NULL AND NOT EXISTS (
          SELECT 1 FROM bindings AS binding
          WHERE binding.id = NEW.target_binding_id AND binding.team_id = NEW.team_id
      ))
BEGIN
    SELECT RAISE(ABORT, 'Delivery attempt Team consistency violation');
END;

DROP TRIGGER delivery_attempts_identity_immutable_update;

CREATE TRIGGER delivery_attempts_identity_immutable_update
BEFORE UPDATE ON delivery_attempts
FOR EACH ROW
WHEN NEW.message_id IS NOT OLD.message_id
  OR NEW.team_id IS NOT OLD.team_id
  OR NEW.attempt_number IS NOT OLD.attempt_number
  OR NEW.target_role_id IS NOT OLD.target_role_id
  OR NEW.target_binding_id IS NOT OLD.target_binding_id
  OR NEW.target_binding_generation IS NOT OLD.target_binding_generation
  OR NEW.route_override_id IS NOT OLD.route_override_id
  OR NEW.route_override_number IS NOT OLD.route_override_number
  OR NEW.created_at IS NOT OLD.created_at
BEGIN
    SELECT RAISE(ABORT, 'Delivery attempt history is immutable');
END;
