-- ===========================================================================
-- CONCEPT-ONLY forward migration.  NOT a production migration.
-- ===========================================================================
--
-- This file lives in the FC-07 evidence directory and is applied ONLY to a
-- throwaway concept database built by `concept_route_override_poc.py`.
--
--   * It is NOT registered in `crates/persistence-sqlite/migrations/`.
--   * It occupies NO production migration number.  There is deliberately no
--     `0028_` prefix here; the name carries no ordinal at all, so nothing in
--     this file can ever be mistaken for, or collide with, the migration number
--     that Codex will sign when it issues the implementation write set.
--   * It writes nothing into any production crate.
--   * It modifies no file under `crates/`.
--
-- Frozen basis (none of it is extended by this file):
--   - STATE_MACHINES.md section 5, transition table row
--     `NEEDS_HUMAN -> QUEUED`: side effect is "新 attempt，记录 route override",
--     event is `MESSAGE_REASSIGNED`/`MESSAGE_REQUEUED`, main rejection code is
--     `AUTH_HUMAN_REQUIRED`.
--   - STATE_MACHINES.md section 5, `QUEUED -> ROUTED`: side effect is "新建
--     attempt 与 target Binding snapshot".
--   - STATE_MACHINES.md section 5: "历史 attempt 和 Event 不可改写".
--   - DATA_CONVENTIONS.md section 3: every successful business change bumps the
--     aggregate version by one; `expected_version` mismatch is
--     `CONCURRENCY_VERSION_CONFLICT` and never auto-overwrites.
--   - DATA_CONVENTIONS.md section 4: the idempotency record key is the FOUR-TUPLE
--     `(team_id, actor_fingerprint, command_name, idempotency_key)` and it stores
--     a payload hash.
--   - DATA_CONVENTIONS.md section 5/6: Message, DeliveryAttempt and their
--     routing history are retained with the Team and are append-only.
--   - DATA_CONVENTIONS.md section 8: Event/Outbox commit in the same transaction
--     as the business change; a failed delivery never rolls back the Event.
--   - DOMAIN_MODEL.md section 4: `Message/Thread` is written only by the
--     Messaging service.
--   - DOMAIN_MODEL.md section 5: no cascading hard deletes.
--   - PERMISSION_MATRIX.md section 3, `Message reassign`: Owner `H`, Leader `S`
--     (only an explicitly authorised Leader), everyone else `-`.
--   - TECH_STACK_AND_STANDARDS.md section 4.4: schema changes only through
--     forward migrations; a published migration is never modified.
--
-- ---------------------------------------------------------------------------
-- WHAT THIS CONCEPT REPLACES, AND WHY IT IS NOT OPTIONAL
-- ---------------------------------------------------------------------------
-- `0017` installs `delivery_attempts_team_consistency_insert/update` whose
-- predicate is
--
--     message.to_role_id = attempt.target_role_id
--
-- That single equality is what currently makes the frozen
-- `NEEDS_HUMAN -> QUEUED` edge inexpressible: a new attempt aimed at a
-- reassignment target cannot satisfy it, because the Message addressing is
-- frozen by `0017: messages_addressing_immutable_update`.
--
-- The rejected R07 option (b) left that equality standing and redefined
-- `attempt.target_role_id` as "the Message's original addressing", which made a
-- historical attempt's real recipient change meaning every time a later
-- reassignment happened, and left the real recipient's Binding snapshot off the
-- attempt row entirely.  Codex rejected it for exactly that reason
-- (`docs/reviews/MINIMAX-RETURN-AUDIT-2026-10-08.md` MR-05).  This concept
-- therefore DROPs and rebuilds the guard, replacing ONE implicit equality with
-- TWO explicit, separately provable paths:
--
--   path 1  the attempt follows the Message's frozen addressing
--            (route_override_id IS NULL)
--   path 2  the attempt adopts a named, append-only route override, and the
--            override's own to_role_id / Binding / generation are what the
--            attempt must carry (route_override_id IS NOT NULL)
--
-- Path 2 is what makes an attempt's recipient a permanent fact of that attempt.
-- ===========================================================================

-- ---------------------------------------------------------------------------
-- 1. The append-only reassignment carrier.
--
-- `attempt` here is the name this document uses for "which route this attempt
-- took"; it is deliberately NOT stored as a status column, because
-- DATA_CONVENTIONS section 5 requires routing history to be append-only and a
-- mutable column would need a "who invalidated it" race that the frozen
-- specification never describes.
-- ---------------------------------------------------------------------------
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

    -- Which reassignment this is, counting from 1 for this Message.
    override_number INTEGER NOT NULL
        CHECK (typeof(override_number) = 'integer' AND override_number >= 1),

    -- The route leaves `from_role_id` and enters `to_role_id`.  `from_role_id`
    -- of the first reassignment is the Message's frozen to_role_id; every later
    -- one is the previous reassignment's to_role_id.  That is the "predecessor
    -- chain", enforced mechanically in section 3 rather than trusted.
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

    -- The Binding actually resolved for the reassignment target.  NOT NULL, and
    -- not nullable-on-purpose: STATE_MACHINES section 5 row
    -- `NEEDS_HUMAN -> QUEUED` says the precondition is "新目标/Binding 有效",
    -- so a reassignment without a resolved Binding is not a legal reassignment.
    -- This is the correction of R07 option (b), which left this information
    -- stranded on the override while the attempt carried nothing.
    target_binding_id BLOB NOT NULL
        CHECK (
            typeof(target_binding_id) = 'blob' AND length(target_binding_id) = 16
            AND substr(hex(target_binding_id), 13, 1) = '7'
            AND substr(hex(target_binding_id), 17, 1) IN ('8', '9', 'A', 'B')
        ),
    target_binding_generation INTEGER NOT NULL
        CHECK (typeof(target_binding_generation) = 'integer'
               AND target_binding_generation >= 1),

    -- DATA_CONVENTIONS section 4 four-tuple.  The fingerprint and payload
    -- comparison are NOT re-implemented here: this table carries the same four
    -- columns and points at the `0022 command_idempotency` row that owns the
    -- payload hash, so there is exactly one place where DATA section 4 lives.
    -- R07's `UNIQUE (team_id, idempotency_key)` was both the wrong arity (two
    -- parts instead of four) and over-broad (one Team's unrelated commands
    -- colliding on a shared key); both defects are impossible with this shape.
    actor_fingerprint BLOB NOT NULL
        CHECK (typeof(actor_fingerprint) = 'blob' AND length(actor_fingerprint) = 32),
    command_name TEXT NOT NULL
        CHECK (length(trim(command_name)) > 0 AND length(command_name) <= 64),
    idempotency_key TEXT NOT NULL
        CHECK (length(trim(idempotency_key)) > 0 AND length(idempotency_key) <= 128),

    -- Every FK below is IMMEDIATE: none of the 27 shipped migrations declares
    -- DEFERRABLE anywhere, so the Event row has to exist BEFORE this row is
    -- inserted.  `concept_route_override_poc.py` proves both halves of that
    -- claim on a real database instead of asserting it.
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

    -- A row that does not actually change the route is not an override; that
    -- case is `MESSAGE_REQUEUED` and belongs to no override at all.  This makes
    -- the `/` in the frozen Event column mechanical.
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
    -- Points at the 0022 row that owns the DATA section 4 payload hash.
    CONSTRAINT message_route_overrides_idempotency_fk
        FOREIGN KEY (team_id, actor_fingerprint, command_name, idempotency_key)
        REFERENCES command_idempotency (team_id, actor_fingerprint, command_name, idempotency_key)
        ON UPDATE RESTRICT ON DELETE RESTRICT
);

-- `UNIQUE(message_id, override_number)` on its own guarantees UNIQUENESS ONLY.
-- It does NOT guarantee contiguity: rows {1, 3} satisfy it perfectly.  This
-- index is kept because uniqueness is still required, but it is deliberately NOT
-- relied on for the no-hole property -- see `..._contiguous_insert` below.
CREATE UNIQUE INDEX message_route_overrides_message_number
    ON message_route_overrides (message_id, override_number);

CREATE INDEX message_route_overrides_message
    ON message_route_overrides (message_id, created_at);
CREATE INDEX message_route_overrides_team
    ON message_route_overrides (team_id);

-- ---------------------------------------------------------------------------
-- 2. The attempt permanently records the override it adopted.
--
-- Both columns are nullable precisely because an attempt on the original route
-- has no override.  They are added by ALTER TABLE so no existing attempt row is
-- rewritten; SQLite fills the pre-existing rows with NULL, which is the correct
-- value for "this attempt followed the Message's own addressing".
-- ---------------------------------------------------------------------------
ALTER TABLE delivery_attempts ADD COLUMN route_override_id BLOB
    REFERENCES message_route_overrides (id)
    ON UPDATE RESTRICT ON DELETE RESTRICT;

ALTER TABLE delivery_attempts ADD COLUMN route_override_number INTEGER;

-- ---------------------------------------------------------------------------
-- 3. Contiguity, and the predecessor chain.
--
-- `..._contiguous_insert` is the constraint R07 was missing.  It requires the
-- new number to equal one more than the number of overrides that already exist
-- for this Message.  Starting from zero rows that forces 1, then 2, then 3 ...
--
--   UNIQUE(message_id, override_number) alone would still have accepted the
--   sequence 1, 2, 4 -- this one refuses the 4.
--
-- `..._predecessor_insert` then pins what the reassignment leaves FROM: the
-- first one leaves the Message's frozen to_role_id, every later one leaves the
-- previous reassignment's to_role_id.  `IS NOT` is used rather than `!=` so a
-- missing predecessor (subquery yields NULL) is also a refusal instead of a
-- silent pass.
-- ---------------------------------------------------------------------------
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

-- Same Team for the Message and both Roles, and an ACTIVE Binding that really
-- belongs to the new target at the recorded generation.
--
-- `status = 'ACTIVE'` is derived, not quoted: STATE_MACHINES section 4 puts
-- DRAINING bindings into "existing Runs may finish, queued messages re-resolve
-- at delivery time", so a NEW delivery attempt must resolve an ACTIVE binding.
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
BEGIN
    SELECT RAISE(ABORT, 'Route override target invalid');
END;

-- ---------------------------------------------------------------------------
-- 4. Route override history is append-only and retained.
--
-- `INSERT OR REPLACE` is SQLite's hidden DELETE + INSERT; with the shipped
-- `PRAGMA recursive_triggers = 0` that hidden delete does NOT fire the DELETE
-- trigger.  DEV-02-14 had to answer this for `command_idempotency` and
-- `0013`/`0014`/`0016` already carry the same `*_replace_guard` pattern, so the
-- guard below follows the established shape rather than inventing one.
-- ---------------------------------------------------------------------------
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
    -- The four-tuple may be claimed by at most one override row.  The uniqueness
    -- that `0022` already enforces on `command_idempotency` cannot be relied on
    -- here on its own, because OR REPLACE deletes the conflicting override row
    -- before inserting; without this arm the new row would simply take over the
    -- key and the first attempt's evidence would be rewritten.
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

-- ---------------------------------------------------------------------------
-- 5. The attempt -> override link.
--
-- This is the whole point of the concept.  An attempt that adopted an override
-- must carry that override's real recipient and real Binding, not merely point
-- at a row somewhere else, and an attempt that adopted nothing must still match
-- the Message's frozen addressing.
-- ---------------------------------------------------------------------------
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

-- ---------------------------------------------------------------------------
-- 6. The rebuilt Team/recipient guard.
--
-- R07 option (b) left `0017`'s equality untouched; this DROPs and rebuilds it as
-- two named paths.  `0017` and `0018` themselves are never modified -- only
-- forward.
-- ---------------------------------------------------------------------------
DROP TRIGGER delivery_attempts_team_consistency_insert;

CREATE TRIGGER delivery_attempts_team_consistency_insert
BEFORE INSERT ON delivery_attempts
FOR EACH ROW
WHEN NOT (
        -- path 1: the attempt follows the Message's frozen addressing
        (NEW.route_override_id IS NULL AND EXISTS (
            SELECT 1 FROM messages AS message
            WHERE message.id = NEW.message_id
              AND message.team_id = NEW.team_id
              AND message.to_role_id = NEW.target_role_id))
        -- path 2: the attempt adopted a named override of this Message
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

-- The adopted override is part of the attempt's identity evidence, so it
-- freezes with the rest of it.  Without this arm an UPDATE could swap the
-- adopted override while leaving the row looking consistent.
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