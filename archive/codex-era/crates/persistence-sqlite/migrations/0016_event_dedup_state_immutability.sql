-- DEV-02-07: close the UPDATE path on delivery deduplication and validate the
-- execution Binding state recorded by a ROLE_BINDING Event snapshot.
--
-- 0015 installed BEFORE INSERT guards, which stop `INSERT OR REPLACE` from
-- resetting Outbox and consumption state, and 0014 guards delivery Team on
-- UPDATE.  Neither protects the deduplication keys themselves: an UPDATE can
-- still change `consumer_id`/`consumed_at`, roll a PUBLISHED row back to
-- PENDING, clear `published_at`, rewrite `idempotency_key`, or lower
-- `retry_count`.  Those paths restore exactly the state 0015 was written to
-- make immutable, so the deduplication guarantee is still bypassable.
--
-- 0015 also validated that the referenced Binding exists and matches Role,
-- Harness, version and Team, but not that the Binding can still execute.  The
-- frozen bindings schema already groups `ACTIVE` and `DRAINING` as its live
-- states (the only ones requiring `effective_from` with a NULL `effective_to`),
-- so a snapshot citing `PENDING_CHECK`, `REVOKED` or `FAILED` names an actor
-- that never held execution capability.
--
-- This is a forward migration.  0001-0015 are never modified, historical Event
-- rows are never rewritten or back-filled, and a database whose existing
-- delivery state already contradicts the invariants below is rejected
-- atomically instead of being silently repaired.

CREATE TABLE event_dedup_state_upgrade_guard (
    id INTEGER PRIMARY KEY CHECK (id = 1),
    valid INTEGER NOT NULL CHECK (valid = 1)
);

INSERT INTO event_dedup_state_upgrade_guard (id, valid)
SELECT 1, 0
WHERE EXISTS (
    -- A published fact without its timestamp is audit distortion.
    SELECT 1 FROM outbox WHERE status = 'PUBLISHED' AND published_at IS NULL
) OR EXISTS (
    -- A consumption row without its dedup marker cannot prove delivery.
    SELECT 1 FROM event_consumptions WHERE consumed_at IS NULL
) OR EXISTS (
    -- A terminal dead letter must not claim a successful publication.
    SELECT 1 FROM outbox WHERE status = 'DEAD_LETTER' AND published_at IS NOT NULL
);

DROP TABLE event_dedup_state_upgrade_guard;

-- A consumption row is the deduplication record for one (consumer, event) pair.
-- Every column that defines or witnesses that record is immutable; only the
-- Team remains updatable, and 0014 already constrains it.
CREATE TRIGGER event_consumptions_deduplication_immutable_update
BEFORE UPDATE ON event_consumptions
FOR EACH ROW
WHEN NEW.consumer_id IS NOT OLD.consumer_id
   OR NEW.event_id IS NOT OLD.event_id
   OR NEW.consumed_at IS NOT OLD.consumed_at
BEGIN
    SELECT RAISE(ABORT, 'Event consumption deduplication is immutable');
END;

-- Outbox deduplication is keyed by (event_id, destination) and witnessed by
-- status, retry_count, published_at and idempotency_key.  This migration does
-- not define the DEV-02-14 dispatcher's legal transitions; it only removes the
-- ability to move a row backwards out of a terminal state, clear evidence that
-- publication happened, rewrite the idempotency key, or lower the retry count.
CREATE TRIGGER outbox_deduplication_state_immutable_update
BEFORE UPDATE ON outbox
FOR EACH ROW
WHEN NEW.event_id IS NOT OLD.event_id
   OR NEW.destination IS NOT OLD.destination
   OR NEW.idempotency_key IS NOT OLD.idempotency_key
   OR (OLD.published_at IS NOT NULL AND NEW.published_at IS NULL)
   OR (OLD.status = 'PUBLISHED' AND NEW.status IS NOT 'PUBLISHED')
   OR (OLD.status = 'DEAD_LETTER' AND NEW.status IS NOT 'DEAD_LETTER')
   OR NEW.retry_count < OLD.retry_count
BEGIN
    SELECT RAISE(ABORT, 'Outbox deduplication state is immutable');
END;

-- Rebuild the 0015 insert guard with the execution-capability check added.
-- Dropping and recreating a trigger is a forward schema change; the 0015 file
-- itself stays byte-identical so its SQLx checksum does not drift.
DROP TRIGGER events_actor_snapshot_consistency_insert;

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
                 AND status IN ('ACTIVE', 'DRAINING')
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

-- An existing ROLE_BINDING Event keeps its historical snapshot after its
-- Binding is later revoked or version-incremented; only NEW Events are
-- constrained.  0015 already freezes the referenced Role, Binding and Harness
-- identity, so this migration adds no further parent-table guards.
