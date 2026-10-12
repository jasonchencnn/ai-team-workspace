-- DEV-02-09 remediation. Preserve 0001..0022 bytes/checksums.
-- Human and Role are distinct globally unique identities (DOMAIN_MODEL).
-- A UUID shape alone is not proof of Human identity; the application must also
-- compare the recipient with its trusted installation Owner at execution time.

-- Only permanent identity conflicts can be established from old rows. A resolved
-- Attention may legitimately refer to a Task that was reopened and blocked again;
-- current source state cannot prove that a historical resolution was invalid.
CREATE TABLE notification_execution_upgrade_guard (
    id INTEGER PRIMARY KEY CHECK (id = 1),
    valid INTEGER NOT NULL CHECK (valid = 1)
);
INSERT INTO notification_execution_upgrade_guard(id, valid)
SELECT 1, 0 WHERE EXISTS (
    SELECT 1 FROM notifications n JOIN roles r ON r.id = n.recipient_human_id
);
DROP TABLE notification_execution_upgrade_guard;

CREATE TRIGGER notifications_recipient_not_role_insert
BEFORE INSERT ON notifications
WHEN EXISTS (SELECT 1 FROM roles WHERE id = NEW.recipient_human_id)
BEGIN SELECT RAISE(ABORT, 'Notification recipient cannot be a Role'); END;

CREATE TRIGGER notifications_recipient_not_role_update
BEFORE UPDATE OF recipient_human_id ON notifications
WHEN EXISTS (SELECT 1 FROM roles WHERE id = NEW.recipient_human_id)
BEGIN SELECT RAISE(ABORT, 'Notification recipient cannot be a Role'); END;

-- Guard the reverse insertion/update too: identities cannot be repurposed.
CREATE TRIGGER roles_notification_human_identity_insert
BEFORE INSERT ON roles
WHEN EXISTS (SELECT 1 FROM notifications WHERE recipient_human_id = NEW.id)
BEGIN SELECT RAISE(ABORT, 'Role identity is used by a Human notification'); END;

CREATE TRIGGER roles_notification_human_identity_update
BEFORE UPDATE OF id ON roles
WHEN EXISTS (SELECT 1 FROM notifications WHERE recipient_human_id = NEW.id)
BEGIN SELECT RAISE(ABORT, 'Role identity is used by a Human notification'); END;

-- Task resolve_block -> READY closes the corresponding Attention (§2.1).
-- The state of the *source* must change first inside the application transaction.
CREATE TRIGGER attention_items_source_resolved_update
BEFORE UPDATE OF status ON attention_items
WHEN NEW.status = 'RESOLVED' AND (
    (NEW.source_type = 'TASK' AND NOT EXISTS (
        SELECT 1 FROM tasks WHERE id = NEW.source_id AND team_id = NEW.team_id
          AND status != 'BLOCKED'
    )) OR
    (NEW.source_type = 'APPROVAL' AND NOT EXISTS (
        SELECT 1 FROM approvals WHERE id = NEW.source_id AND team_id = NEW.team_id
          AND status IN ('APPROVED', 'REJECTED', 'EXPIRED', 'CANCELLED')
    )) OR
    (NEW.source_type = 'MESSAGE' AND NOT EXISTS (
        SELECT 1 FROM messages WHERE id = NEW.source_id AND team_id = NEW.team_id
          AND status NOT IN ('FAILED', 'NEEDS_HUMAN')
    )) OR
    (NEW.source_type = 'RUN' AND NOT EXISTS (
        SELECT 1 FROM runs r JOIN tasks t ON t.id = r.task_id
        WHERE r.id = NEW.source_id AND r.team_id = NEW.team_id
          AND r.status != 'UNKNOWN'
          AND (
              r.status IN ('SUCCEEDED', 'CANCELLED')
              OR t.status IN ('DONE', 'CANCELLED')
          )
    )) OR
    -- SYSTEM has no frozen backing state table. No proof means no automatic
    -- resolution. Human dismissal with reason remains the separate §6 path.
    NEW.source_type = 'SYSTEM'
)
BEGIN SELECT RAISE(ABORT, 'Attention source is unresolved'); END;

-- Store the already specified filters/escalation without replacing the published
-- table layout. Options are owned by the Policy, not a new aggregate. No complex
-- filter/strategy language is interpreted by this storage migration.
CREATE TABLE notification_policy_options (
    policy_id BLOB PRIMARY KEY NOT NULL REFERENCES notification_policies(id)
        ON UPDATE RESTRICT ON DELETE RESTRICT,
    filters TEXT NOT NULL DEFAULT '{}'
        CHECK (json_valid(filters) AND json_type(filters) = 'object'),
    escalation TEXT NOT NULL DEFAULT '{}'
        CHECK (json_valid(escalation) AND json_type(escalation) = 'object'),
    is_mandatory INTEGER NOT NULL DEFAULT 0
        CHECK (typeof(is_mandatory) = 'integer' AND is_mandatory IN (0, 1)),
    policy_version INTEGER NOT NULL CHECK (typeof(policy_version) = 'integer' AND policy_version >= 1),
    CHECK (is_mandatory = 0 OR json(filters) = '{}')
);

CREATE TRIGGER notification_policy_options_insert
BEFORE INSERT ON notification_policy_options
WHEN NOT EXISTS (
    SELECT 1 FROM notification_policies p WHERE p.id = NEW.policy_id
      AND p.version = NEW.policy_version
      AND (NEW.is_mandatory = 0 OR p.status = 'ACTIVE')
) OR EXISTS (SELECT 1 FROM notification_policy_options WHERE policy_id = NEW.policy_id)
BEGIN SELECT RAISE(ABORT, 'Notification policy options invalid'); END;

CREATE TRIGGER notification_policy_options_update
BEFORE UPDATE ON notification_policy_options
WHEN NEW.policy_id IS NOT OLD.policy_id
  OR NEW.policy_version <= OLD.policy_version
  OR (OLD.is_mandatory = 1 AND NEW.is_mandatory != 1)
  OR NOT EXISTS (
      SELECT 1 FROM notification_policies p WHERE p.id = NEW.policy_id
        AND p.version = NEW.policy_version
        AND (NEW.is_mandatory = 0 OR p.status = 'ACTIVE')
  )
BEGIN SELECT RAISE(ABORT, 'Notification policy options invalid'); END;

CREATE TRIGGER notification_policy_options_delete
BEFORE DELETE ON notification_policy_options
BEGIN SELECT RAISE(ABORT, 'Notification policy options history is retained'); END;

CREATE TRIGGER notification_policy_mandatory_active_update
BEFORE UPDATE ON notification_policies
WHEN EXISTS (SELECT 1 FROM notification_policy_options o WHERE o.policy_id = OLD.id AND o.is_mandatory = 1)
  AND (NEW.status != 'ACTIVE' OR NEW.event_type IS NOT OLD.event_type)
BEGIN SELECT RAISE(ABORT, 'Mandatory notification policy cannot be disabled'); END;

-- 0019 lists source columns in UPDATE OF but omits them from its comparison.
-- Freeze the actual references too; redirecting a resolved item would fabricate
-- which issue was handled, and detaching a notification would sever its evidence.
CREATE TRIGGER attention_items_source_immutable_update
BEFORE UPDATE OF source_type, source_id ON attention_items
WHEN NEW.source_type IS NOT OLD.source_type OR NEW.source_id IS NOT OLD.source_id
BEGIN SELECT RAISE(ABORT, 'Attention source is immutable'); END;

CREATE TRIGGER notifications_source_immutable_update
BEFORE UPDATE OF source_event_id, source_message_id, policy_id ON notifications
WHEN NEW.source_event_id IS NOT OLD.source_event_id
  OR NEW.source_message_id IS NOT OLD.source_message_id
  OR NEW.policy_id IS NOT OLD.policy_id
BEGIN SELECT RAISE(ABORT, 'Notification source is immutable'); END;

-- DATA_CONVENTIONS §6: Notification presentation may be hidden, facts retained.
-- 0014 only protects Notifications already referenced as Event subjects.
CREATE TRIGGER notifications_history_retained_delete
BEFORE DELETE ON notifications
BEGIN SELECT RAISE(ABORT, 'Notification history is retained'); END;
