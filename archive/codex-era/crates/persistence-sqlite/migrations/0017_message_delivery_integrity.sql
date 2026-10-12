-- DEV-02-08: Team consistency, immutability and state-machine guards for
-- Thread, Message and Delivery Attempt.
--
-- 0007 defines the three tables but installs only two triggers (body freeze and
-- reply_to same-thread).  It has no Team guard, no state guard and no delivery
-- immutability, while 0010-0016 already established that pattern for Binding,
-- Task, Run, Artifact, Event, Outbox and Consumption.  An independent
-- counterexample review reproduced cross-Team addressing, recipient
-- redirection, terminal-state escape, idempotency-key rewrite, delivery-result
-- flip and attempt-number reuse against 0007.
--
-- Every status set below is taken verbatim from the frozen
-- docs/specs/STATE_MACHINES.md section 5 transition table.  This migration adds
-- no state, no error code and no permission that the specification does not
-- already define.
--
-- Forward migration: 0001-0016 are never modified.  Existing history is never
-- rewritten or back-filled, and a database that already contradicts these
-- invariants is rejected atomically instead of being silently repaired.

CREATE TABLE message_delivery_integrity_upgrade_guard (
    id INTEGER PRIMARY KEY CHECK (id = 1),
    valid INTEGER NOT NULL CHECK (valid = 1)
);

INSERT INTO message_delivery_integrity_upgrade_guard (id, valid)
SELECT 1, 0
WHERE EXISTS (
    -- A message whose recipient, thread or sender snapshot belongs to another
    -- Team is a cross-Team addressing record.
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
) OR EXISTS (
    -- A delivery attempt must share its message's Team and intended recipient.
    SELECT 1
    FROM delivery_attempts AS attempt
    WHERE NOT EXISTS (
              SELECT 1 FROM messages AS message
              WHERE message.id = attempt.message_id
                AND message.team_id = attempt.team_id
                AND message.to_role_id = attempt.target_role_id
          )
       OR (attempt.target_binding_id IS NOT NULL AND NOT EXISTS (
               SELECT 1 FROM bindings
               WHERE id = attempt.target_binding_id AND team_id = attempt.team_id
           ))
) OR EXISTS (
    -- A Role sender that has left DRAFT must carry its Binding snapshot, and the
    -- snapshot must name that Role at the recorded generation.
    SELECT 1
    FROM messages AS message
    WHERE message.sender_actor_type = 'ROLE'
      AND message.status != 'DRAFT'
      AND (
          message.sender_binding_snapshot_id IS NULL
          OR message.sender_binding_generation IS NULL
          OR NOT EXISTS (
              SELECT 1 FROM bindings
              WHERE id = message.sender_binding_snapshot_id
                AND team_id = message.team_id
                AND role_id = message.from_role_id
                AND binding_generation = message.sender_binding_generation
          )
      )
);

DROP TABLE message_delivery_integrity_upgrade_guard;

-- ---------------------------------------------------------------------------
-- Message: Team consistency on insert.
-- ---------------------------------------------------------------------------
CREATE TRIGGER messages_team_consistency_insert
BEFORE INSERT ON messages
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

-- STATE_MACHINES.md section 5 records the sender Binding snapshot as a side
-- effect of DRAFT -> QUEUED.  A Role sender that has left DRAFT therefore must
-- carry a snapshot naming that Role, in the same Team, at the recorded
-- generation.  HUMAN and SYSTEM senders legitimately have none.
CREATE TRIGGER messages_sender_binding_snapshot_insert
BEFORE INSERT ON messages
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

-- 0015 already froze the matching rule for Events: HUMAN and SYSTEM actors
-- have no invented persistence mapping and may not carry a Role-Binding
-- snapshot.  A Message must not be looser than the Event that records it.
CREATE TRIGGER messages_non_role_snapshot_rejected_insert
BEFORE INSERT ON messages
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
-- Message: state machine.
--
-- Legal edges, taken from the frozen transition table:
--   DRAFT        -> QUEUED
--   QUEUED       -> ROUTED | NEEDS_HUMAN | EXPIRED
--   ROUTED       -> EXPIRED | DELIVERED | QUEUED | FAILED
--   DELIVERED    -> ACKNOWLEDGED
--   ACKNOWLEDGED -> PROCESSING | REPLIED
--   PROCESSING   -> REPLIED
--   FAILED       -> QUEUED
--   NEEDS_HUMAN  -> QUEUED
-- EXPIRED and REPLIED have no outgoing edge; the specification states EXPIRED
-- is never reopened on the original Message.
-- ---------------------------------------------------------------------------
CREATE TRIGGER messages_state_machine_update
BEFORE UPDATE OF status ON messages
FOR EACH ROW
WHEN NOT (
       (OLD.status = 'DRAFT'        AND NEW.status IN ('QUEUED'))
    OR (OLD.status = 'QUEUED'       AND NEW.status IN ('ROUTED', 'NEEDS_HUMAN', 'EXPIRED'))
    OR (OLD.status = 'ROUTED'       AND NEW.status IN ('EXPIRED', 'DELIVERED', 'QUEUED', 'FAILED'))
    OR (OLD.status = 'DELIVERED'    AND NEW.status IN ('ACKNOWLEDGED'))
    OR (OLD.status = 'ACKNOWLEDGED' AND NEW.status IN ('PROCESSING', 'REPLIED'))
    OR (OLD.status = 'PROCESSING'   AND NEW.status IN ('REPLIED'))
    OR (OLD.status = 'FAILED'       AND NEW.status IN ('QUEUED'))
    OR (OLD.status = 'NEEDS_HUMAN'  AND NEW.status IN ('QUEUED'))
)
BEGIN
    SELECT RAISE(ABORT, 'DOMAIN_INVALID_TRANSITION');
END;

-- ---------------------------------------------------------------------------
-- Message: addressing, snapshot and idempotency are frozen once the sender has
-- committed by leaving DRAFT.  The specification makes DRAFT -> QUEUED the
-- point at which content is frozen; these columns decide who receives it and
-- under which identity, so they freeze with the body.
-- ---------------------------------------------------------------------------
CREATE TRIGGER messages_addressing_immutable_update
BEFORE UPDATE ON messages
FOR EACH ROW
WHEN OLD.status != 'DRAFT'
 AND (
     NEW.team_id IS NOT OLD.team_id
     OR NEW.thread_id IS NOT OLD.thread_id
     OR NEW.to_role_id IS NOT OLD.to_role_id
     OR NEW.from_role_id IS NOT OLD.from_role_id
     OR NEW.sender_actor_type IS NOT OLD.sender_actor_type
     OR NEW.message_type IS NOT OLD.message_type
     OR NEW.idempotency_key IS NOT OLD.idempotency_key
     OR NEW.reply_to_message_id IS NOT OLD.reply_to_message_id
     OR NEW.sender_binding_snapshot_id IS NOT OLD.sender_binding_snapshot_id
     OR NEW.sender_binding_generation IS NOT OLD.sender_binding_generation
 )
BEGIN
    SELECT RAISE(ABORT, 'Message addressing is immutable');
END;

-- A sender Binding snapshot may only appear on the DRAFT -> QUEUED edge, never
-- later, so a queued message can never gain a snapshot that was not recorded
-- when the sender committed it.
CREATE TRIGGER messages_sender_binding_locked_after_queued
BEFORE UPDATE OF sender_binding_snapshot_id, sender_binding_generation ON messages
FOR EACH ROW
WHEN OLD.status != 'DRAFT'
BEGIN
    SELECT RAISE(ABORT, 'Message sender Binding snapshot is immutable');
END;

-- ---------------------------------------------------------------------------
-- Message: idempotency must survive INSERT OR REPLACE.  0007 relies solely on
-- the (team_id, idempotency_key) unique index, which OR REPLACE bypasses by
-- deleting the conflicting row before inserting -- the same path closed for
-- Outbox and Consumption in 0016.
-- ---------------------------------------------------------------------------
CREATE TRIGGER messages_idempotency_replace_guard
BEFORE INSERT ON messages
FOR EACH ROW
WHEN NEW.idempotency_key IS NOT NULL
 AND EXISTS (
     SELECT 1 FROM messages AS existing
     WHERE existing.team_id = NEW.team_id
       AND existing.idempotency_key = NEW.idempotency_key
 )
BEGIN
    SELECT RAISE(ABORT, 'Message idempotency key is immutable');
END;

-- ---------------------------------------------------------------------------
-- Message: primary key replacement would silently drop every delivery attempt
-- and reply that referenced it.
-- ---------------------------------------------------------------------------
CREATE TRIGGER messages_identity_replace_guard
BEFORE INSERT ON messages
FOR EACH ROW
WHEN EXISTS (
    SELECT 1 FROM messages AS existing
    WHERE existing.id = NEW.id
)
 OR EXISTS (
    SELECT 1 FROM delivery_attempts AS attempt
    WHERE attempt.message_id = NEW.id
)
BEGIN
    SELECT RAISE(ABORT, 'MESSAGE_IDENTITY_IMMUTABLE');
END;

-- ---------------------------------------------------------------------------
-- Delivery Attempt: Team, recipient and Binding snapshot must match the
-- message being delivered.  A target that differs from messages.to_role_id is
-- a redirection of someone else's message.
-- ---------------------------------------------------------------------------
CREATE TRIGGER delivery_attempts_team_consistency_insert
BEFORE INSERT ON delivery_attempts
FOR EACH ROW
WHEN NOT EXISTS (
          SELECT 1 FROM messages AS message
          WHERE message.id = NEW.message_id
            AND message.team_id = NEW.team_id
            AND message.to_role_id = NEW.target_role_id
      )
  OR (NEW.target_binding_id IS NOT NULL AND NOT EXISTS (
          SELECT 1 FROM bindings
          WHERE id = NEW.target_binding_id AND team_id = NEW.team_id
      ))
BEGIN
    SELECT RAISE(ABORT, 'Delivery attempt Team consistency violation');
END;

CREATE TRIGGER delivery_attempts_team_consistency_update
BEFORE UPDATE ON delivery_attempts
FOR EACH ROW
WHEN NOT EXISTS (
          SELECT 1 FROM messages AS message
          WHERE message.id = NEW.message_id
            AND message.team_id = NEW.team_id
            AND message.to_role_id = NEW.target_role_id
      )
  OR (NEW.target_binding_id IS NOT NULL AND NOT EXISTS (
          SELECT 1 FROM bindings
          WHERE id = NEW.target_binding_id AND team_id = NEW.team_id
      ))
BEGIN
    SELECT RAISE(ABORT, 'Delivery attempt Team consistency violation');
END;

-- The target Binding snapshot must name the Role this attempt delivers to.
CREATE TRIGGER delivery_attempts_target_binding_insert
BEFORE INSERT ON delivery_attempts
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

-- ---------------------------------------------------------------------------
-- Delivery Attempt: attempt history is evidence.  STATE_MACHINES.md section 5
-- states that a FAILED attempt is the terminal state of one delivery cycle and
-- that historical attempts may not be rewritten; a retry creates a new attempt.
-- The identity, recipient, Binding snapshot and attempt number are therefore
-- immutable, and the outcome may not be flipped.
-- ---------------------------------------------------------------------------
CREATE TRIGGER delivery_attempts_identity_immutable_update
BEFORE UPDATE ON delivery_attempts
FOR EACH ROW
WHEN NEW.message_id IS NOT OLD.message_id
  OR NEW.team_id IS NOT OLD.team_id
  OR NEW.attempt_number IS NOT OLD.attempt_number
  OR NEW.target_role_id IS NOT OLD.target_role_id
  OR NEW.target_binding_id IS NOT OLD.target_binding_id
  OR NEW.target_binding_generation IS NOT OLD.target_binding_generation
  OR NEW.created_at IS NOT OLD.created_at
BEGIN
    SELECT RAISE(ABORT, 'Delivery attempt history is immutable');
END;

-- PENDING -> IN_FLIGHT -> SUCCEEDED | FAILED | ABANDONED.  All three outcomes
-- are absorbing, so a FAILED delivery can never be relabelled as delivered.
CREATE TRIGGER delivery_attempts_state_machine_update
BEFORE UPDATE OF status ON delivery_attempts
FOR EACH ROW
WHEN NOT (
       (OLD.status = 'PENDING'  AND NEW.status IN ('IN_FLIGHT', 'FAILED', 'ABANDONED'))
    OR (OLD.status = 'IN_FLIGHT' AND NEW.status IN ('SUCCEEDED', 'FAILED', 'ABANDONED'))
)
BEGIN
    SELECT RAISE(ABORT, 'DOMAIN_INVALID_TRANSITION');
END;

-- A completed attempt's timestamps and failure detail are evidence of what
-- happened and may not be rewritten afterwards.
CREATE TRIGGER delivery_attempts_outcome_immutable_update
BEFORE UPDATE ON delivery_attempts
FOR EACH ROW
WHEN OLD.status IN ('SUCCEEDED', 'FAILED', 'ABANDONED')
 AND (
     NEW.completed_at IS NOT OLD.completed_at
     OR NEW.error_code IS NOT OLD.error_code
     OR NEW.error_message IS NOT OLD.error_message
     OR NEW.started_at IS NOT OLD.started_at
 )
BEGIN
    SELECT RAISE(ABORT, 'Delivery attempt outcome is immutable');
END;

-- Replacement would delete the attempt row without running its DELETE
-- triggers under SQLite's default recursive_triggers=OFF, and a unique
-- attempt_number could then be reused.
CREATE TRIGGER delivery_attempts_replace_guard
BEFORE INSERT ON delivery_attempts
FOR EACH ROW
WHEN EXISTS (
    SELECT 1 FROM delivery_attempts AS existing
    WHERE existing.id = NEW.id
     OR (existing.message_id = NEW.message_id
         AND existing.attempt_number = NEW.attempt_number)
)
BEGIN
    SELECT RAISE(ABORT, 'Delivery attempt deduplication is immutable');
END;

-- ---------------------------------------------------------------------------
-- Thread: a Thread referenced by messages or by an Event must keep its Team and
-- identity, matching the pattern 0014 applied to every other polymorphic Event
-- subject.
-- ---------------------------------------------------------------------------
CREATE TRIGGER threads_message_reference_update
BEFORE UPDATE OF id, team_id ON threads
FOR EACH ROW
WHEN EXISTS (SELECT 1 FROM messages WHERE thread_id = OLD.id)
BEGIN
    SELECT RAISE(ABORT, 'Thread identity is referenced by messages');
END;

CREATE TRIGGER threads_message_reference_delete
BEFORE DELETE ON threads
FOR EACH ROW
WHEN EXISTS (SELECT 1 FROM messages WHERE thread_id = OLD.id)
BEGIN
    SELECT RAISE(ABORT, 'Thread identity is referenced by messages');
END;

CREATE TRIGGER threads_replace_guard
BEFORE INSERT ON threads
FOR EACH ROW
WHEN EXISTS (SELECT 1 FROM threads AS existing WHERE existing.id = NEW.id)
BEGIN
    SELECT RAISE(ABORT, 'Thread identity is immutable');
END;
