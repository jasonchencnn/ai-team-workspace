-- DEV-02-07: preserve Event subject and delivery Team consistency.
-- SQLite foreign keys validate individual rows, but cannot express the
-- polymorphic Event subject relationship or require an Outbox/consumption to
-- share its Event's Team.  Reject a historical violation atomically before
-- installing the forward-write guards; migration never rewrites audit data.
CREATE TABLE event_outbox_consistency_upgrade_guard (
    id INTEGER PRIMARY KEY CHECK (id = 1),
    valid INTEGER NOT NULL CHECK (valid = 1)
);

INSERT INTO event_outbox_consistency_upgrade_guard (id, valid)
SELECT 1, 0
WHERE EXISTS (
    SELECT 1
    FROM events AS event
    WHERE (event.actor_type = 'ROLE' AND NOT EXISTS (
        SELECT 1 FROM roles
        WHERE id = event.actor_id AND team_id = event.team_id
    )) OR (event.subject_type = 'TEAM' AND NOT EXISTS (
        SELECT 1 FROM teams
        WHERE id = event.subject_id AND id = event.team_id
    )) OR (event.subject_type = 'ROLE' AND NOT EXISTS (
        SELECT 1 FROM roles
        WHERE id = event.subject_id AND team_id = event.team_id
    )) OR (event.subject_type = 'WORKSPACE' AND NOT EXISTS (
        SELECT 1 FROM workspaces
        WHERE id = event.subject_id AND team_id = event.team_id
    )) OR (event.subject_type = 'HARNESS' AND NOT EXISTS (
        SELECT 1 FROM harness_instances
        WHERE id = event.subject_id AND team_id = event.team_id
    )) OR (event.subject_type = 'ADAPTER' AND NOT EXISTS (
        SELECT 1 FROM adapters
        WHERE id = event.subject_id AND team_id = event.team_id
    )) OR (event.subject_type = 'BINDING' AND NOT EXISTS (
        SELECT 1 FROM bindings
        WHERE id = event.subject_id AND team_id = event.team_id
    )) OR (event.subject_type = 'TASK' AND NOT EXISTS (
        SELECT 1 FROM tasks
        WHERE id = event.subject_id AND team_id = event.team_id
    )) OR (event.subject_type = 'RUN' AND NOT EXISTS (
        SELECT 1 FROM runs
        WHERE id = event.subject_id AND team_id = event.team_id
    )) OR (event.subject_type = 'ARTIFACT' AND NOT EXISTS (
        SELECT 1 FROM artifacts
        WHERE id = event.subject_id AND team_id = event.team_id
    )) OR (event.subject_type = 'THREAD' AND NOT EXISTS (
        SELECT 1 FROM threads
        WHERE id = event.subject_id AND team_id = event.team_id
    )) OR (event.subject_type = 'MESSAGE' AND NOT EXISTS (
        SELECT 1 FROM messages
        WHERE id = event.subject_id AND team_id = event.team_id
    )) OR (event.subject_type = 'NOTIFICATION' AND NOT EXISTS (
        SELECT 1 FROM notifications
        WHERE id = event.subject_id AND team_id = event.team_id
    )) OR (event.subject_type = 'APPROVAL' AND NOT EXISTS (
        SELECT 1 FROM approvals
        WHERE id = event.subject_id AND team_id = event.team_id
    ))
) OR EXISTS (
    SELECT 1
    FROM outbox
    JOIN events AS event ON event.id = outbox.event_id
    WHERE outbox.team_id IS NOT event.team_id
) OR EXISTS (
    SELECT 1
    FROM event_consumptions AS consumption
    JOIN events AS event ON event.id = consumption.event_id
    WHERE consumption.team_id IS NOT event.team_id
);

DROP TABLE event_outbox_consistency_upgrade_guard;

-- A DELETE caused by INSERT OR REPLACE does not run DELETE triggers while
-- recursive_triggers is OFF.  Detect the conflicting existing Event before
-- SQLite can replace it.
CREATE TRIGGER events_append_only_replace_guard
BEFORE INSERT ON events
FOR EACH ROW
WHEN EXISTS (SELECT 1 FROM events WHERE id = NEW.id)
BEGIN
    SELECT RAISE(ABORT, 'EVENTS_ARE_APPEND_ONLY');
END;

CREATE TRIGGER events_subject_consistency_insert
BEFORE INSERT ON events
FOR EACH ROW
WHEN (NEW.actor_type = 'ROLE' AND NOT EXISTS (
    SELECT 1 FROM roles WHERE id = NEW.actor_id AND team_id = NEW.team_id
)) OR (NEW.subject_type = 'TEAM' AND NOT EXISTS (
    SELECT 1 FROM teams WHERE id = NEW.subject_id AND id = NEW.team_id
)) OR (NEW.subject_type = 'ROLE' AND NOT EXISTS (
    SELECT 1 FROM roles WHERE id = NEW.subject_id AND team_id = NEW.team_id
)) OR (NEW.subject_type = 'WORKSPACE' AND NOT EXISTS (
    SELECT 1 FROM workspaces WHERE id = NEW.subject_id AND team_id = NEW.team_id
)) OR (NEW.subject_type = 'HARNESS' AND NOT EXISTS (
    SELECT 1 FROM harness_instances WHERE id = NEW.subject_id AND team_id = NEW.team_id
)) OR (NEW.subject_type = 'ADAPTER' AND NOT EXISTS (
    SELECT 1 FROM adapters WHERE id = NEW.subject_id AND team_id = NEW.team_id
)) OR (NEW.subject_type = 'BINDING' AND NOT EXISTS (
    SELECT 1 FROM bindings WHERE id = NEW.subject_id AND team_id = NEW.team_id
)) OR (NEW.subject_type = 'TASK' AND NOT EXISTS (
    SELECT 1 FROM tasks WHERE id = NEW.subject_id AND team_id = NEW.team_id
)) OR (NEW.subject_type = 'RUN' AND NOT EXISTS (
    SELECT 1 FROM runs WHERE id = NEW.subject_id AND team_id = NEW.team_id
)) OR (NEW.subject_type = 'ARTIFACT' AND NOT EXISTS (
    SELECT 1 FROM artifacts WHERE id = NEW.subject_id AND team_id = NEW.team_id
)) OR (NEW.subject_type = 'THREAD' AND NOT EXISTS (
    SELECT 1 FROM threads WHERE id = NEW.subject_id AND team_id = NEW.team_id
)) OR (NEW.subject_type = 'MESSAGE' AND NOT EXISTS (
    SELECT 1 FROM messages WHERE id = NEW.subject_id AND team_id = NEW.team_id
)) OR (NEW.subject_type = 'NOTIFICATION' AND NOT EXISTS (
    SELECT 1 FROM notifications WHERE id = NEW.subject_id AND team_id = NEW.team_id
)) OR (NEW.subject_type = 'APPROVAL' AND NOT EXISTS (
    SELECT 1 FROM approvals WHERE id = NEW.subject_id AND team_id = NEW.team_id
))
BEGIN
    SELECT RAISE(ABORT, 'Event consistency violation');
END;

CREATE TRIGGER outbox_event_team_consistency_insert
BEFORE INSERT ON outbox
FOR EACH ROW
WHEN NOT EXISTS (
    SELECT 1 FROM events
    WHERE id = NEW.event_id AND team_id = NEW.team_id
)
BEGIN
    SELECT RAISE(ABORT, 'Outbox Event Team consistency violation');
END;

CREATE TRIGGER outbox_event_team_consistency_update
BEFORE UPDATE ON outbox
FOR EACH ROW
WHEN NOT EXISTS (
    SELECT 1 FROM events
    WHERE id = NEW.event_id AND team_id = NEW.team_id
)
BEGIN
    SELECT RAISE(ABORT, 'Outbox Event Team consistency violation');
END;

CREATE TRIGGER event_consumptions_event_team_consistency_insert
BEFORE INSERT ON event_consumptions
FOR EACH ROW
WHEN NOT EXISTS (
    SELECT 1 FROM events
    WHERE id = NEW.event_id AND team_id = NEW.team_id
)
BEGIN
    SELECT RAISE(ABORT, 'Event consumption Team consistency violation');
END;

CREATE TRIGGER event_consumptions_event_team_consistency_update
BEFORE UPDATE ON event_consumptions
FOR EACH ROW
WHEN NOT EXISTS (
    SELECT 1 FROM events
    WHERE id = NEW.event_id AND team_id = NEW.team_id
)
BEGIN
    SELECT RAISE(ABORT, 'Event consumption Team consistency violation');
END;

-- Event subjects are polymorphic, so the database cannot use a foreign key.
-- These guards keep a referenced subject's identity and Team stable and also
-- close every OR REPLACE path that could silently delete such a subject.
CREATE TRIGGER teams_event_subject_update
BEFORE UPDATE OF id ON teams
FOR EACH ROW
WHEN EXISTS (SELECT 1 FROM events WHERE subject_type = 'TEAM' AND subject_id = OLD.id)
BEGIN SELECT RAISE(ABORT, 'Event subject consistency violation'); END;

CREATE TRIGGER teams_event_subject_delete
BEFORE DELETE ON teams
FOR EACH ROW
WHEN EXISTS (SELECT 1 FROM events WHERE subject_type = 'TEAM' AND subject_id = OLD.id)
BEGIN SELECT RAISE(ABORT, 'Event subject consistency violation'); END;

CREATE TRIGGER teams_event_subject_replace_guard
BEFORE INSERT ON teams
FOR EACH ROW
WHEN EXISTS (
    SELECT 1 FROM teams AS existing
    JOIN events AS event ON event.subject_type = 'TEAM' AND event.subject_id = existing.id
    WHERE existing.id = NEW.id
)
BEGIN SELECT RAISE(ABORT, 'Event subject consistency violation'); END;

CREATE TRIGGER roles_event_subject_update
BEFORE UPDATE OF id, team_id ON roles
FOR EACH ROW
WHEN EXISTS (SELECT 1 FROM events WHERE subject_type = 'ROLE' AND subject_id = OLD.id)
   OR EXISTS (SELECT 1 FROM events WHERE actor_type = 'ROLE' AND actor_id = OLD.id)
BEGIN SELECT RAISE(ABORT, 'Event subject consistency violation'); END;

CREATE TRIGGER roles_event_subject_delete
BEFORE DELETE ON roles
FOR EACH ROW
WHEN EXISTS (SELECT 1 FROM events WHERE subject_type = 'ROLE' AND subject_id = OLD.id)
   OR EXISTS (SELECT 1 FROM events WHERE actor_type = 'ROLE' AND actor_id = OLD.id)
BEGIN SELECT RAISE(ABORT, 'Event subject consistency violation'); END;

CREATE TRIGGER roles_event_subject_replace_guard
BEFORE INSERT ON roles
FOR EACH ROW
WHEN EXISTS (
    SELECT 1 FROM roles AS existing
    JOIN events AS event ON (event.subject_type = 'ROLE' AND event.subject_id = existing.id)
                       OR (event.actor_type = 'ROLE' AND event.actor_id = existing.id)
    WHERE existing.id = NEW.id
       OR (existing.team_id = NEW.team_id AND existing.display_id = NEW.display_id)
)
BEGIN SELECT RAISE(ABORT, 'Event subject consistency violation'); END;

CREATE TRIGGER workspaces_event_subject_update
BEFORE UPDATE OF id, team_id ON workspaces
FOR EACH ROW
WHEN EXISTS (SELECT 1 FROM events WHERE subject_type = 'WORKSPACE' AND subject_id = OLD.id)
BEGIN SELECT RAISE(ABORT, 'Event subject consistency violation'); END;

CREATE TRIGGER workspaces_event_subject_delete
BEFORE DELETE ON workspaces
FOR EACH ROW
WHEN EXISTS (SELECT 1 FROM events WHERE subject_type = 'WORKSPACE' AND subject_id = OLD.id)
BEGIN SELECT RAISE(ABORT, 'Event subject consistency violation'); END;

CREATE TRIGGER workspaces_event_subject_replace_guard
BEFORE INSERT ON workspaces
FOR EACH ROW
WHEN EXISTS (
    SELECT 1 FROM workspaces AS existing
    JOIN events AS event ON event.subject_type = 'WORKSPACE' AND event.subject_id = existing.id
    WHERE existing.id = NEW.id OR existing.team_id = NEW.team_id
)
BEGIN SELECT RAISE(ABORT, 'Event subject consistency violation'); END;

CREATE TRIGGER harness_instances_event_subject_update
BEFORE UPDATE OF id, team_id ON harness_instances
FOR EACH ROW
WHEN EXISTS (SELECT 1 FROM events WHERE subject_type = 'HARNESS' AND subject_id = OLD.id)
BEGIN SELECT RAISE(ABORT, 'Event subject consistency violation'); END;

CREATE TRIGGER harness_instances_event_subject_delete
BEFORE DELETE ON harness_instances
FOR EACH ROW
WHEN EXISTS (SELECT 1 FROM events WHERE subject_type = 'HARNESS' AND subject_id = OLD.id)
BEGIN SELECT RAISE(ABORT, 'Event subject consistency violation'); END;

CREATE TRIGGER harness_instances_event_subject_replace_guard
BEFORE INSERT ON harness_instances
FOR EACH ROW
WHEN EXISTS (
    SELECT 1 FROM harness_instances AS existing
    JOIN events AS event ON event.subject_type = 'HARNESS' AND event.subject_id = existing.id
    WHERE existing.id = NEW.id
)
BEGIN SELECT RAISE(ABORT, 'Event subject consistency violation'); END;

CREATE TRIGGER adapters_event_subject_update
BEFORE UPDATE OF id, team_id ON adapters
FOR EACH ROW
WHEN EXISTS (SELECT 1 FROM events WHERE subject_type = 'ADAPTER' AND subject_id = OLD.id)
BEGIN SELECT RAISE(ABORT, 'Event subject consistency violation'); END;

CREATE TRIGGER adapters_event_subject_delete
BEFORE DELETE ON adapters
FOR EACH ROW
WHEN EXISTS (SELECT 1 FROM events WHERE subject_type = 'ADAPTER' AND subject_id = OLD.id)
BEGIN SELECT RAISE(ABORT, 'Event subject consistency violation'); END;

CREATE TRIGGER adapters_event_subject_replace_guard
BEFORE INSERT ON adapters
FOR EACH ROW
WHEN EXISTS (
    SELECT 1 FROM adapters AS existing
    JOIN events AS event ON event.subject_type = 'ADAPTER' AND event.subject_id = existing.id
    WHERE existing.id = NEW.id
)
BEGIN SELECT RAISE(ABORT, 'Event subject consistency violation'); END;

CREATE TRIGGER bindings_event_subject_update
BEFORE UPDATE OF id, team_id ON bindings
FOR EACH ROW
WHEN EXISTS (SELECT 1 FROM events WHERE subject_type = 'BINDING' AND subject_id = OLD.id)
BEGIN SELECT RAISE(ABORT, 'Event subject consistency violation'); END;

CREATE TRIGGER bindings_event_subject_delete
BEFORE DELETE ON bindings
FOR EACH ROW
WHEN EXISTS (SELECT 1 FROM events WHERE subject_type = 'BINDING' AND subject_id = OLD.id)
BEGIN SELECT RAISE(ABORT, 'Event subject consistency violation'); END;

CREATE TRIGGER bindings_event_subject_replace_guard
BEFORE INSERT ON bindings
FOR EACH ROW
WHEN EXISTS (
    SELECT 1 FROM bindings AS existing
    JOIN events AS event ON event.subject_type = 'BINDING' AND event.subject_id = existing.id
    WHERE existing.id = NEW.id
       OR (NEW.status = 'ACTIVE' AND existing.role_id = NEW.role_id AND existing.status = 'ACTIVE')
       OR (existing.role_id = NEW.role_id AND existing.binding_generation = NEW.binding_generation)
)
BEGIN SELECT RAISE(ABORT, 'Event subject consistency violation'); END;

CREATE TRIGGER tasks_event_subject_update
BEFORE UPDATE OF id, team_id ON tasks
FOR EACH ROW
WHEN EXISTS (SELECT 1 FROM events WHERE subject_type = 'TASK' AND subject_id = OLD.id)
BEGIN SELECT RAISE(ABORT, 'Event subject consistency violation'); END;

CREATE TRIGGER tasks_event_subject_delete
BEFORE DELETE ON tasks
FOR EACH ROW
WHEN EXISTS (SELECT 1 FROM events WHERE subject_type = 'TASK' AND subject_id = OLD.id)
BEGIN SELECT RAISE(ABORT, 'Event subject consistency violation'); END;

CREATE TRIGGER tasks_event_subject_replace_guard
BEFORE INSERT ON tasks
FOR EACH ROW
WHEN EXISTS (
    SELECT 1 FROM tasks AS existing
    JOIN events AS event ON event.subject_type = 'TASK' AND event.subject_id = existing.id
    WHERE existing.id = NEW.id
       OR (existing.team_id = NEW.team_id AND existing.display_id = NEW.display_id)
)
BEGIN SELECT RAISE(ABORT, 'Event subject consistency violation'); END;

CREATE TRIGGER runs_event_subject_update
BEFORE UPDATE OF id, team_id ON runs
FOR EACH ROW
WHEN EXISTS (SELECT 1 FROM events WHERE subject_type = 'RUN' AND subject_id = OLD.id)
BEGIN SELECT RAISE(ABORT, 'Event subject consistency violation'); END;

CREATE TRIGGER runs_event_subject_delete
BEFORE DELETE ON runs
FOR EACH ROW
WHEN EXISTS (SELECT 1 FROM events WHERE subject_type = 'RUN' AND subject_id = OLD.id)
BEGIN SELECT RAISE(ABORT, 'Event subject consistency violation'); END;

CREATE TRIGGER runs_event_subject_replace_guard
BEFORE INSERT ON runs
FOR EACH ROW
WHEN EXISTS (
    SELECT 1 FROM runs AS existing
    JOIN events AS event ON event.subject_type = 'RUN' AND event.subject_id = existing.id
    WHERE existing.id = NEW.id
       OR (existing.task_id = NEW.task_id AND existing.attempt = NEW.attempt)
       OR (existing.task_id = NEW.task_id AND existing.fencing_token = NEW.fencing_token)
       OR (
           NEW.status IN ('QUEUED', 'STARTING', 'RUNNING', 'UNKNOWN')
           AND existing.task_id = NEW.task_id
           AND existing.status IN ('QUEUED', 'STARTING', 'RUNNING', 'UNKNOWN')
       )
)
BEGIN SELECT RAISE(ABORT, 'Event subject consistency violation'); END;

CREATE TRIGGER artifacts_event_subject_update
BEFORE UPDATE OF id, team_id ON artifacts
FOR EACH ROW
WHEN EXISTS (SELECT 1 FROM events WHERE subject_type = 'ARTIFACT' AND subject_id = OLD.id)
BEGIN SELECT RAISE(ABORT, 'Event subject consistency violation'); END;

CREATE TRIGGER artifacts_event_subject_delete
BEFORE DELETE ON artifacts
FOR EACH ROW
WHEN EXISTS (SELECT 1 FROM events WHERE subject_type = 'ARTIFACT' AND subject_id = OLD.id)
BEGIN SELECT RAISE(ABORT, 'Event subject consistency violation'); END;

CREATE TRIGGER artifacts_event_subject_replace_guard
BEFORE INSERT ON artifacts
FOR EACH ROW
WHEN EXISTS (
    SELECT 1 FROM artifacts AS existing
    JOIN events AS event ON event.subject_type = 'ARTIFACT' AND event.subject_id = existing.id
    WHERE existing.id = NEW.id
)
BEGIN SELECT RAISE(ABORT, 'Event subject consistency violation'); END;

CREATE TRIGGER threads_event_subject_update
BEFORE UPDATE OF id, team_id ON threads
FOR EACH ROW
WHEN EXISTS (SELECT 1 FROM events WHERE subject_type = 'THREAD' AND subject_id = OLD.id)
BEGIN SELECT RAISE(ABORT, 'Event subject consistency violation'); END;

CREATE TRIGGER threads_event_subject_delete
BEFORE DELETE ON threads
FOR EACH ROW
WHEN EXISTS (SELECT 1 FROM events WHERE subject_type = 'THREAD' AND subject_id = OLD.id)
BEGIN SELECT RAISE(ABORT, 'Event subject consistency violation'); END;

CREATE TRIGGER threads_event_subject_replace_guard
BEFORE INSERT ON threads
FOR EACH ROW
WHEN EXISTS (
    SELECT 1 FROM threads AS existing
    JOIN events AS event ON event.subject_type = 'THREAD' AND event.subject_id = existing.id
    WHERE existing.id = NEW.id
)
BEGIN SELECT RAISE(ABORT, 'Event subject consistency violation'); END;

CREATE TRIGGER messages_event_subject_update
BEFORE UPDATE OF id, team_id ON messages
FOR EACH ROW
WHEN EXISTS (SELECT 1 FROM events WHERE subject_type = 'MESSAGE' AND subject_id = OLD.id)
BEGIN SELECT RAISE(ABORT, 'Event subject consistency violation'); END;

CREATE TRIGGER messages_event_subject_delete
BEFORE DELETE ON messages
FOR EACH ROW
WHEN EXISTS (SELECT 1 FROM events WHERE subject_type = 'MESSAGE' AND subject_id = OLD.id)
BEGIN SELECT RAISE(ABORT, 'Event subject consistency violation'); END;

CREATE TRIGGER messages_event_subject_replace_guard
BEFORE INSERT ON messages
FOR EACH ROW
WHEN EXISTS (
    SELECT 1 FROM messages AS existing
    JOIN events AS event ON event.subject_type = 'MESSAGE' AND event.subject_id = existing.id
    WHERE existing.id = NEW.id
       OR (
           NEW.idempotency_key IS NOT NULL
           AND existing.team_id = NEW.team_id
           AND existing.idempotency_key = NEW.idempotency_key
       )
)
BEGIN SELECT RAISE(ABORT, 'Event subject consistency violation'); END;

CREATE TRIGGER notifications_event_subject_update
BEFORE UPDATE OF id, team_id ON notifications
FOR EACH ROW
WHEN EXISTS (SELECT 1 FROM events WHERE subject_type = 'NOTIFICATION' AND subject_id = OLD.id)
BEGIN SELECT RAISE(ABORT, 'Event subject consistency violation'); END;

CREATE TRIGGER notifications_event_subject_delete
BEFORE DELETE ON notifications
FOR EACH ROW
WHEN EXISTS (SELECT 1 FROM events WHERE subject_type = 'NOTIFICATION' AND subject_id = OLD.id)
BEGIN SELECT RAISE(ABORT, 'Event subject consistency violation'); END;

CREATE TRIGGER notifications_event_subject_replace_guard
BEFORE INSERT ON notifications
FOR EACH ROW
WHEN EXISTS (
    SELECT 1 FROM notifications AS existing
    JOIN events AS event ON event.subject_type = 'NOTIFICATION' AND event.subject_id = existing.id
    WHERE existing.id = NEW.id
       OR (
           NEW.dedupe_key IS NOT NULL
           AND NEW.status IN ('UNREAD', 'READ')
           AND existing.team_id = NEW.team_id
           AND existing.recipient_human_id = NEW.recipient_human_id
           AND existing.dedupe_key = NEW.dedupe_key
           AND existing.status IN ('UNREAD', 'READ')
       )
)
BEGIN SELECT RAISE(ABORT, 'Event subject consistency violation'); END;

CREATE TRIGGER approvals_event_subject_update
BEFORE UPDATE OF id, team_id ON approvals
FOR EACH ROW
WHEN EXISTS (SELECT 1 FROM events WHERE subject_type = 'APPROVAL' AND subject_id = OLD.id)
BEGIN SELECT RAISE(ABORT, 'Event subject consistency violation'); END;

CREATE TRIGGER approvals_event_subject_delete
BEFORE DELETE ON approvals
FOR EACH ROW
WHEN EXISTS (SELECT 1 FROM events WHERE subject_type = 'APPROVAL' AND subject_id = OLD.id)
BEGIN SELECT RAISE(ABORT, 'Event subject consistency violation'); END;

CREATE TRIGGER approvals_event_subject_replace_guard
BEFORE INSERT ON approvals
FOR EACH ROW
WHEN EXISTS (
    SELECT 1 FROM approvals AS existing
    JOIN events AS event ON event.subject_type = 'APPROVAL' AND event.subject_id = existing.id
    WHERE existing.id = NEW.id
)
BEGIN SELECT RAISE(ABORT, 'Event subject consistency violation'); END;
