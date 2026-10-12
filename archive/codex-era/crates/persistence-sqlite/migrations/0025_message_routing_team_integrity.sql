-- MM-R01 (deep audit DA-01): the Message Team guard had no UPDATE arm.
--
-- Reproduced against 0001..0024: `0017` installed
--   messages_team_consistency_insert  (BEFORE INSERT)  -- the only Team arm
--   messages_addressing_immutable_update (BEFORE UPDATE) -- WHEN OLD.status != 'DRAFT'
-- so the addressing columns were mutable for exactly one status: DRAFT.
-- A DRAFT could therefore be retargeted to another Team's Role, committed
-- through the legal `DRAFT -> QUEUED` edge, routed, and given a delivery
-- attempt on the foreign route.  `delivery_attempts_team_consistency_*` only
-- compared an attempt to the message it belonged to, so it inherited the hole.
--
-- Frozen basis, none of which this migration extends:
--   - DOMAIN_MODEL.md section 1: Team is the isolation boundary for
--     collaboration, permission, Task, Workspace and Event.
--   - PERMISSION_MATRIX.md section 3, `Message 发送/回复/确认`: the guard
--     condition is `to_role_id` / Team / visibility validation.
--   - STATE_MACHINES.md section 5: `DRAFT -> QUEUED` freezes the content and
--     records the sender Binding snapshot; `QUEUED -> NEEDS_HUMAN` is legal
--     when the target has no usable Binding.
--   - DATA_CONVENTIONS.md section 5: a user may revise a DRAFT, and the body
--     freezes at QUEUED -- which is why DRAFT stays editable and this
--     migration constrains only the *Team* of the addressing, not its value.
--
-- This adds no state, no error code, no permission and no protocol meaning.  It
-- only gives the existing Team invariant its missing UPDATE side, so the two
-- sides can no longer disagree (ADVERSARIAL_REVIEW_PLAYBOOK rule 4).
--
-- Forward migration: 0001..0024 are never modified and no checksum drifts.
-- Dirty history is rejected atomically below; no Team is ever back-filled.

-- ---------------------------------------------------------------------------
-- Upgrade guard.
--
-- `0017` already rejected cross-Team message addressing, so a database that
-- passed it cannot contain these rows.  The scan is still performed because the
-- new guards constrain rows that `0017` never examined -- notably an attempt
-- whose target Role exists but belongs to another Team, which `0017` accepted
-- whenever the (already corrupted) message agreed with it.
--
-- The CHECK on `valid` is what rejects the upgrade: inserting 0 violates it and
-- aborts the migration transaction.  Nothing is repaired and no Team is invented.
-- ---------------------------------------------------------------------------
CREATE TABLE message_routing_team_upgrade_guard (
    id INTEGER PRIMARY KEY CHECK (id = 1),
    -- Named on purpose: SQLite reports `CHECK constraint failed: <name>`, so the
    -- operator sees which migration refused the upgrade instead of a bare
    -- `valid = 1`.  0017 and 0023 leave the constraint unnamed, which makes
    -- their rejection indistinguishable from any other failed check.
    valid INTEGER NOT NULL
        CONSTRAINT message_routing_team_upgrade_guard_valid CHECK (valid = 1)
);

INSERT INTO message_routing_team_upgrade_guard (id, valid)
SELECT 1, 0
WHERE EXISTS (
    -- A message whose addressing leaves its Team.
    SELECT 1
    FROM messages AS message
    WHERE (message.to_role_id IS NOT NULL AND NOT EXISTS (
               SELECT 1 FROM roles
               WHERE id = message.to_role_id AND team_id = message.team_id
           ))
       OR (message.from_role_id IS NOT NULL AND NOT EXISTS (
               SELECT 1 FROM roles
               WHERE id = message.from_role_id AND team_id = message.team_id
           ))
       OR NOT EXISTS (
               SELECT 1 FROM threads
               WHERE id = message.thread_id AND team_id = message.team_id
           )
       OR (message.sender_binding_snapshot_id IS NOT NULL AND NOT EXISTS (
               SELECT 1 FROM bindings
               WHERE id = message.sender_binding_snapshot_id AND team_id = message.team_id
           ))
)
OR EXISTS (
    -- A HUMAN or SYSTEM sender may not hold a Role-Binding snapshot.
    SELECT 1
    FROM messages AS message
    WHERE message.sender_actor_type IN ('HUMAN', 'SYSTEM')
      AND (
          message.sender_binding_snapshot_id IS NOT NULL
          OR message.sender_binding_generation IS NOT NULL
      )
)
OR EXISTS (
    -- A delivery attempt must target a Role of its own Team, not merely a Role
    -- that agrees with a message that `0017` already accepted.
    SELECT 1
    FROM delivery_attempts AS attempt
    WHERE NOT EXISTS (
              SELECT 1 FROM roles AS role
              WHERE id = attempt.target_role_id AND team_id = attempt.team_id
          )
       OR (attempt.target_binding_id IS NOT NULL AND NOT EXISTS (
               SELECT 1 FROM bindings AS binding
               WHERE id = attempt.target_binding_id
                 AND team_id = attempt.team_id
                 AND role_id = attempt.target_role_id
           ))
);

DROP TABLE message_routing_team_upgrade_guard;

-- ---------------------------------------------------------------------------
-- Message: the missing UPDATE arm of `messages_team_consistency_insert`.
--
-- Same predicate, same message, on UPDATE.  It is deliberately not restricted to
-- DRAFT: after DRAFT the addressing is already immutable, so on every other
-- status this trigger simply re-asserts a value `0017` froze, and it would still
-- catch a row whose addressing was wrong before 0025 ran.
--
-- DRAFT editing stays legal because only the *Team* of each address is checked.
-- Retargeting inside the Team, and revising the body, remain permitted exactly
-- as DATA_CONVENTIONS section 5 requires.
-- ---------------------------------------------------------------------------
CREATE TRIGGER messages_team_consistency_update
BEFORE UPDATE ON messages
FOR EACH ROW
WHEN (NEW.to_role_id IS NOT NULL AND NOT EXISTS (
          SELECT 1 FROM roles WHERE id = NEW.to_role_id AND team_id = NEW.team_id
      ))
  OR (NEW.from_role_id IS NOT NULL AND NOT EXISTS (
          SELECT 1 FROM roles WHERE id = NEW.from_role_id AND team_id = NEW.team_id
      ))
  OR NOT EXISTS (
          SELECT 1 FROM threads WHERE id = NEW.thread_id AND team_id = NEW.team_id
      )
  OR (NEW.sender_binding_snapshot_id IS NOT NULL AND NOT EXISTS (
          SELECT 1 FROM bindings
          WHERE id = NEW.sender_binding_snapshot_id AND team_id = NEW.team_id
      ))
BEGIN
    SELECT RAISE(ABORT, 'Message Team consistency violation');
END;

-- ---------------------------------------------------------------------------
-- Message: the UPDATE arm of `messages_non_role_snapshot_rejected_insert`.
--
-- 0017 wrote that rule for INSERT only.  A DRAFT may set the snapshot columns,
-- so a HUMAN or SYSTEM message could acquire a Role-Binding snapshot that no
-- INSERT would have accepted.  0015 froze the same rule for Events; a Message
-- must not be looser than the Event recording it.
-- ---------------------------------------------------------------------------
CREATE TRIGGER messages_non_role_snapshot_rejected_update
BEFORE UPDATE ON messages
FOR EACH ROW
WHEN NEW.sender_actor_type IN ('HUMAN', 'SYSTEM')
 AND (
     NEW.sender_binding_snapshot_id IS NOT NULL
     OR NEW.sender_binding_generation IS NOT NULL
 )
BEGIN
    SELECT RAISE(ABORT, 'Message sender Binding snapshot violation');
END;

-- ---------------------------------------------------------------------------
-- Message: the UPDATE arm of `messages_sender_binding_snapshot_insert`.
--
-- STATE_MACHINES section 5 makes the snapshot a product of `DRAFT -> QUEUED`
-- for a ROLE sender.  0018 already guards that edge; this covers the remaining
-- way to reach a non-DRAFT ROLE message, namely changing `from_role_id` or the
-- actor type after the snapshot was taken.
-- ---------------------------------------------------------------------------
CREATE TRIGGER messages_sender_binding_snapshot_update
BEFORE UPDATE ON messages
FOR EACH ROW
WHEN NEW.sender_actor_type = 'ROLE'
 AND NEW.status != 'DRAFT'
 AND (
     NEW.sender_binding_snapshot_id IS NULL
     OR NEW.sender_binding_generation IS NULL
     OR NOT EXISTS (
         SELECT 1 FROM bindings
         WHERE id = NEW.sender_binding_snapshot_id
           AND team_id = NEW.team_id
           AND role_id = NEW.from_role_id
           AND binding_generation = NEW.sender_binding_generation
     )
 )
BEGIN
    SELECT RAISE(ABORT, 'Message sender Binding snapshot violation');
END;

-- ---------------------------------------------------------------------------
-- Delivery Attempt: the target Role must be a Role of the attempt's own Team.
--
-- `delivery_attempts_team_consistency_*` checks the attempt against its
-- message.  That is necessary but not sufficient: it is only as sound as the
-- message, and the message could name a foreign Role through the DRAFT hole.
-- This states the rule directly against the real Role table, so a misrouted
-- attempt is refused even if some future path produced a corrupt message.
--
-- No attempt is required to have a Binding: `QUEUED -> NEEDS_HUMAN` remains the
-- legal route for a target that has none, and this guard does not invent one.
-- ---------------------------------------------------------------------------
CREATE TRIGGER delivery_attempts_target_role_team_insert
BEFORE INSERT ON delivery_attempts
FOR EACH ROW
WHEN NOT EXISTS (
    SELECT 1 FROM roles
    WHERE id = NEW.target_role_id AND team_id = NEW.team_id
)
BEGIN
    SELECT RAISE(ABORT, 'Delivery attempt Team consistency violation');
END;

CREATE TRIGGER delivery_attempts_target_role_team_update
BEFORE UPDATE ON delivery_attempts
FOR EACH ROW
WHEN NOT EXISTS (
    SELECT 1 FROM roles
    WHERE id = NEW.target_role_id AND team_id = NEW.team_id
)
BEGIN
    SELECT RAISE(ABORT, 'Delivery attempt Team consistency violation');
END;

-- ---------------------------------------------------------------------------
-- Delivery Attempt: the UPDATE arm of
-- `delivery_attempts_target_binding_insert`.
--
-- The target Binding snapshot must name the Role this attempt delivers to, in
-- the attempt's Team.  `0017` stated this on INSERT only; here it is stated on
-- UPDATE as well, so the two sides cannot disagree.
-- ---------------------------------------------------------------------------
CREATE TRIGGER delivery_attempts_target_binding_update
BEFORE UPDATE ON delivery_attempts
FOR EACH ROW
WHEN NEW.target_binding_id IS NOT NULL
 AND NOT EXISTS (
     SELECT 1 FROM bindings
     WHERE id = NEW.target_binding_id
       AND team_id = NEW.team_id
       AND role_id = NEW.target_role_id
       AND binding_generation = NEW.target_binding_generation
 )
BEGIN
    SELECT RAISE(ABORT, 'Delivery target Binding snapshot violation');
END;