-- DEV-02-07: preserve the frozen ROLE_BINDING Event actor snapshot without
-- rewriting legacy Events or their hash-chain material.
ALTER TABLE events ADD COLUMN actor_kind TEXT
    CHECK (actor_kind IS NULL OR actor_kind = 'ROLE_BINDING');
ALTER TABLE events ADD COLUMN actor_role_id BLOB
    CHECK (
        actor_role_id IS NULL
        OR (
            typeof(actor_role_id) = 'blob'
            AND length(actor_role_id) = 16
            AND substr(hex(actor_role_id), 13, 1) = '7'
            AND substr(hex(actor_role_id), 17, 1) IN ('8', '9', 'A', 'B')
        )
    );
ALTER TABLE events ADD COLUMN actor_binding_id BLOB
    CHECK (
        actor_binding_id IS NULL
        OR (
            typeof(actor_binding_id) = 'blob'
            AND length(actor_binding_id) = 16
            AND substr(hex(actor_binding_id), 13, 1) = '7'
            AND substr(hex(actor_binding_id), 17, 1) IN ('8', '9', 'A', 'B')
        )
    );
ALTER TABLE events ADD COLUMN actor_binding_version INTEGER
    CHECK (
        actor_binding_version IS NULL
        OR (typeof(actor_binding_version) = 'integer' AND actor_binding_version >= 1)
    );
ALTER TABLE events ADD COLUMN actor_harness_instance_id BLOB
    CHECK (
        actor_harness_instance_id IS NULL
        OR (
            typeof(actor_harness_instance_id) = 'blob'
            AND length(actor_harness_instance_id) = 16
            AND substr(hex(actor_harness_instance_id), 13, 1) = '7'
            AND substr(hex(actor_harness_instance_id), 17, 1) IN ('8', '9', 'A', 'B')
        )
    );

-- New ROLE Events must record the stable Role and the exact Binding/Harness
-- snapshot which represented it.  HUMAN and SYSTEM deliberately have no
-- invented persistence mapping; ADAPTER is forbidden as an Event actor.
CREATE TRIGGER events_actor_snapshot_consistency_insert
BEFORE INSERT ON events
FOR EACH ROW
WHEN NEW.actor_type = 'ADAPTER'
   OR (
       NEW.actor_type IN ('HUMAN', 'SYSTEM')
       AND (
           NEW.actor_kind IS NOT NULL
           OR NEW.actor_role_id IS NOT NULL
           OR NEW.actor_binding_id IS NOT NULL
           OR NEW.actor_binding_version IS NOT NULL
           OR NEW.actor_harness_instance_id IS NOT NULL
       )
   )
   OR (
       NEW.actor_type = 'ROLE'
       AND (
           NEW.actor_kind IS NOT 'ROLE_BINDING'
           OR NEW.actor_id IS NOT NEW.actor_role_id
           OR NOT EXISTS (
               SELECT 1 FROM roles
               WHERE id = NEW.actor_role_id AND team_id = NEW.team_id
           )
           OR NOT EXISTS (
               SELECT 1 FROM bindings
               WHERE id = NEW.actor_binding_id
                 AND team_id = NEW.team_id
                 AND role_id = NEW.actor_role_id
                 AND harness_instance_id = NEW.actor_harness_instance_id
                 AND version = NEW.actor_binding_version
           )
           OR NOT EXISTS (
               SELECT 1 FROM harness_instances
               WHERE id = NEW.actor_harness_instance_id AND team_id = NEW.team_id
           )
       )
   )
BEGIN
    SELECT RAISE(ABORT, 'Event actor snapshot consistency violation');
END;

-- INSERT OR REPLACE deletes the conflicting row without running its DELETE
-- trigger when recursive_triggers is OFF.  Preserve the deduplication state
-- by rejecting the conflict before SQLite replaces either record.
CREATE TRIGGER outbox_deduplication_replace_guard
BEFORE INSERT ON outbox
FOR EACH ROW
WHEN EXISTS (
    SELECT 1 FROM outbox
    WHERE event_id = NEW.event_id AND destination = NEW.destination
)
BEGIN
    SELECT RAISE(ABORT, 'Outbox deduplication is immutable');
END;

CREATE TRIGGER event_consumptions_deduplication_replace_guard
BEFORE INSERT ON event_consumptions
FOR EACH ROW
WHEN EXISTS (
    SELECT 1 FROM event_consumptions
    WHERE consumer_id = NEW.consumer_id AND event_id = NEW.event_id
)
BEGIN
    SELECT RAISE(ABORT, 'Event consumption deduplication is immutable');
END;

-- A ROLE_BINDING Event snapshot is historical evidence.  It must keep its
-- referenced identity, Team, and Binding relationship, but an ordinary
-- Binding version increment remains valid because the Event stores its
-- original version rather than requiring the current version to stay fixed.
CREATE TRIGGER roles_event_actor_snapshot_update
BEFORE UPDATE OF id, team_id ON roles
FOR EACH ROW
WHEN EXISTS (
    SELECT 1 FROM events
    WHERE actor_kind = 'ROLE_BINDING' AND actor_role_id = OLD.id
)
BEGIN
    SELECT RAISE(ABORT, 'Event actor snapshot consistency violation');
END;

CREATE TRIGGER roles_event_actor_snapshot_delete
BEFORE DELETE ON roles
FOR EACH ROW
WHEN EXISTS (
    SELECT 1 FROM events
    WHERE actor_kind = 'ROLE_BINDING' AND actor_role_id = OLD.id
)
BEGIN
    SELECT RAISE(ABORT, 'Event actor snapshot consistency violation');
END;

CREATE TRIGGER roles_event_actor_snapshot_replace_guard
BEFORE INSERT ON roles
FOR EACH ROW
WHEN EXISTS (
    SELECT 1
    FROM roles AS existing
    JOIN events AS event
      ON event.actor_kind = 'ROLE_BINDING'
     AND event.actor_role_id = existing.id
    WHERE existing.id = NEW.id
       OR (existing.team_id = NEW.team_id AND existing.display_id = NEW.display_id)
)
BEGIN
    SELECT RAISE(ABORT, 'Event actor snapshot consistency violation');
END;

CREATE TRIGGER bindings_event_actor_snapshot_update
BEFORE UPDATE OF id, team_id, role_id, harness_instance_id ON bindings
FOR EACH ROW
WHEN EXISTS (
    SELECT 1 FROM events
    WHERE actor_kind = 'ROLE_BINDING' AND actor_binding_id = OLD.id
)
BEGIN
    SELECT RAISE(ABORT, 'Event actor snapshot consistency violation');
END;

CREATE TRIGGER bindings_event_actor_snapshot_delete
BEFORE DELETE ON bindings
FOR EACH ROW
WHEN EXISTS (
    SELECT 1 FROM events
    WHERE actor_kind = 'ROLE_BINDING' AND actor_binding_id = OLD.id
)
BEGIN
    SELECT RAISE(ABORT, 'Event actor snapshot consistency violation');
END;

CREATE TRIGGER bindings_event_actor_snapshot_replace_guard
BEFORE INSERT ON bindings
FOR EACH ROW
WHEN EXISTS (
    SELECT 1
    FROM bindings AS existing
    JOIN events AS event
      ON event.actor_kind = 'ROLE_BINDING'
     AND event.actor_binding_id = existing.id
    WHERE existing.id = NEW.id
       OR (NEW.status = 'ACTIVE' AND existing.role_id = NEW.role_id AND existing.status = 'ACTIVE')
       OR (existing.role_id = NEW.role_id AND existing.binding_generation = NEW.binding_generation)
)
BEGIN
    SELECT RAISE(ABORT, 'Event actor snapshot consistency violation');
END;

CREATE TRIGGER harness_instances_event_actor_snapshot_update
BEFORE UPDATE OF id, team_id ON harness_instances
FOR EACH ROW
WHEN EXISTS (
    SELECT 1 FROM events
    WHERE actor_kind = 'ROLE_BINDING' AND actor_harness_instance_id = OLD.id
)
BEGIN
    SELECT RAISE(ABORT, 'Event actor snapshot consistency violation');
END;

CREATE TRIGGER harness_instances_event_actor_snapshot_delete
BEFORE DELETE ON harness_instances
FOR EACH ROW
WHEN EXISTS (
    SELECT 1 FROM events
    WHERE actor_kind = 'ROLE_BINDING' AND actor_harness_instance_id = OLD.id
)
BEGIN
    SELECT RAISE(ABORT, 'Event actor snapshot consistency violation');
END;

CREATE TRIGGER harness_instances_event_actor_snapshot_replace_guard
BEFORE INSERT ON harness_instances
FOR EACH ROW
WHEN EXISTS (
    SELECT 1
    FROM harness_instances AS existing
    JOIN events AS event
      ON event.actor_kind = 'ROLE_BINDING'
     AND event.actor_harness_instance_id = existing.id
    WHERE existing.id = NEW.id
)
BEGIN
    SELECT RAISE(ABORT, 'Event actor snapshot consistency violation');
END;
