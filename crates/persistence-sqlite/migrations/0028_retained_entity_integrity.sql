-- MM-R02 follow-up, FC-02 (audit MR-03): six retained objects could still be
-- destroyed at storage level, outside the five tables `0026` was given.
--
-- The complete retention census in
-- `docs/reviews/evidence/minimax-return-audit-2026-10-08/storage/RETENTION_INVENTORY.md`
-- measured, against `0001..0027` with foreign keys on and a clean seed:
--
--   table              DELETE          INSERT OR REPLACE
--   -----------------  --------------  --------------------------------------
--   teams              1 -> 0          rewrote `name`
--   roles              1 -> 0          rewrote `name`
--   tasks              1 -> 0          rewrote `title`
--   bindings           1 -> 0          rewrote `configuration_snapshot`
--   "references"       1 -> 0          rewrote `target_uri`
--   delivery_attempts  1 -> 0          already refused by `0017` (see below)
--
-- These are **not** unprotected tables, and saying so would be wrong twice over.
-- `0013` and `0014` do guard `teams`, `roles`, `tasks` and `bindings` -- but every
-- one of those arms is conditional on a referencing row existing:
--
--   0013 `tasks_referenced_subject_delete`        WHEN a "references" row names it
--   0014 `<table>_event_subject_delete`            WHEN an events row names it
--   0015 `<table>_event_actor_snapshot_delete`     WHEN a ROLE_BINDING Event names it
--
-- `"references"` and `delivery_attempts` have no `BEFORE DELETE` trigger at all;
-- a Reference is a leaf (nothing points at one) and an attempt is a child of a
-- Message, so neither met an obstacle.
--
-- **`OR REPLACE` is not decorative.** SQLite implements it as a hidden delete of
-- the conflicting row followed by an insert, and with the shipped
-- `PRAGMA recursive_triggers = 0` that hidden delete does **not** fire the table's
-- `BEFORE DELETE` triggers. DEV-02-14 had to answer this for
-- `command_idempotency`, and `0013`, `0014` and `0020` already carry
-- `*_replace_guard` triggers for the same reason.
--
-- Two arms the census did not spell out, because they need the schema rather
-- than a census row:
--
--   - `roles` and `tasks` also carry `UNIQUE (team_id, display_id)`. A
--     replacement under a **different** primary key still deletes the existing
--     row through that index, so a same-id-only guard would have left the real
--     hole open. Both are guarded below.
--   - `delivery_attempts` carries the partial unique index
--     `delivery_attempts_single_active ON (message_id) WHERE status IN
--     ('PENDING','IN_FLIGHT')`. `0017`'s replace guard covers only `id` and
--     `(message_id, attempt_number)`, so a **different** id carrying a different
--     attempt number still deleted the live attempt. One extra arm covers it.
--
-- **`delivery_attempts` keeps `0017`'s original replace guard.** It is not
-- duplicated, restated or weakened, and the new active-slot arm explicitly
-- stands aside whenever `0017`'s own condition holds, so `0017` keeps firing on
-- its own message. A legitimate retry -- previous attempt terminal, new attempt
-- `PENDING` with the next attempt number -- does not collide with the partial
-- index and is still accepted.
--
-- **One path is deliberately left to an existing guard.** `bindings` also carries
-- `bindings_single_active_per_role ON (role_id) WHERE status = 'ACTIVE'`. Two
-- `ACTIVE` Bindings for one Role always have overlapping effective ranges, so
-- `0002`'s `bindings_no_overlapping_ranges_insert` refuses the second one first.
-- A guard for that key would be unproven code asserting a guarantee another
-- trigger already provides; `0026` recorded the same judgement for `runs`,
-- `approvals` and `messages`.
--
-- **Shadowing is avoided by yielding, not by reordering.** In SQLite the first
-- `BEFORE` trigger to fire wins, and the order between triggers on the same
-- event is not defined. Every new guard below therefore carries a `WHEN` clause
-- that is false whenever a pre-existing guard already refuses that exact row,
-- so the established guard keeps its own diagnostic and `0028` only covers what
-- nothing else did. `run_lease_consistency_forward_migration.rs` asserts
-- `0012`'s exact message for a `roles.team_id` move, which is why `team_id` is
-- not restated anywhere below.
--
-- **Identity, checked per table and stated per table.** DATA_CONVENTIONS
-- section 1 and DOMAIN_MODEL section 2 make these stable identities, and DOMAIN
-- section 5 says core objects are linked by stable ID without cascading hard
-- deletes, so renumbering a row is the `UPDATE`-side form of the same hole.
-- What is **not** frozen, and is therefore not guarded:
--
--   - `name`, `title`, `description`, `configuration_snapshot`, `version`,
--     `updated_at` and every state column stay editable. Freezing them would
--     weld shut ordinary business edits, which is a different defect.
--   - `roles.team_id`, `tasks.team_id`/`owner_role_id`, `bindings.team_id`,
--     `bindings.role_id`/`harness_instance_id`/`adapter_id` are already refused
--     by `0010`..`0015` whenever any historical association exists. The residual
--     case is a row with no history at all, which has nothing to re-point, so
--     no new rule was invented for it.
--   - A same-Team executor swap on a Binding -- `harness_instance_id` and
--     `adapter_id` moved to another Harness/Adapter of the *same* Team -- is
--     legitimate business behaviour and stays legal. This was measured, not
--     assumed: an earlier draft froze those columns and
--     `binding_team_consistency_forward_migration.rs` failed on it. See the
--     `bindings_retained_entity_identity_immutable_update` header.
--
-- A `"references"` row keeps the target it recorded. DATA_CONVENTIONS section 5
-- says a reference that stops being valid has **only** its `validation_status`
-- updated and does not delete the historical URI/path/hash, and DOMAIN_MODEL
-- section 5 says a broken Workspace reference records its validation state and
-- guides repair. Rewriting `target_uri` in place is the `UPDATE`-side form of
-- the rewrite the census found through `INSERT OR REPLACE`.
--
-- Frozen basis, none of which this migration extends:
--   - DATA_CONVENTIONS.md section 5: "P0 核心对象无通用 DELETE 命令"; Team uses
--     `ACTIVE/PAUSED/ARCHIVED`, Role uses `ACTIVE/DEACTIVATED`, Task uses
--     `CANCELLED`; "Event、Run、Binding、Review decision、DeliveryAttempt、
--     Approval decision 不更新历史语义，只追加后续事实"; "引用失效只更新
--     `validation_status`；不删除历史 URI/path/hash".
--   - DATA_CONVENTIONS.md section 6: Team/Role/Task/Binding are "随 Team 永久保留";
--     Message/Delivery are "随 Team 保留".
--   - DOMAIN_MODEL.md section 5: "核心对象通过稳定 ID 关联，不级联硬删除".
--
-- Scope notes, so this file is not read as claiming more than it does:
--   - Attention items are deliberately **not** protected here. Deep audit
--     section 4 records that Attention is a rebuildable derived index outside the
--     DATA retention table. Nothing in this file touches `attention_items`.
--   - This migration guards storage-level `DELETE`/`REPLACE`. It makes no claim
--     about a production `DELETE` command existing, and none about any loss that
--     has already occurred.
--
-- No new state, status, error code, permission or protocol meaning is added.
-- Every trigger raises `ABORT` with a message only, the same shape `0013`,
-- `0014`, `0016`, `0017` and `0020` already use.
--
-- No upgrade guard table is created. `0025` and `0020` need one because they
-- constrain rows that already exist; this migration constrains only statements
-- issued *after* it is applied, so every existing row already satisfies it and a
-- `valid = 1` probe would be the tautology ADVERSARIAL_REVIEW_PLAYBOOK rule 5
-- warns about. What the upgrade must guarantee instead is that it changes
-- nothing, which `upgrading_to_0028_preserves_every_existing_row` and
-- `a_failed_upgrade_has_no_side_effects` check directly.
--
-- Forward migration: `0001..0027` are never modified and no checksum drifts.
-- SQLx applies this file in one transaction, so a failure leaves neither the
-- schema nor the data changed.

-- ===========================================================================
-- Team
--
-- `teams` has exactly one unique key, the primary key, so the same-id arm is
-- the whole story here; a replacement under a *new* id is a create, not a
-- replacement, and `ordinary_business_edits_still_work` pins that.
-- ===========================================================================

CREATE TRIGGER teams_retained_entity_retention_delete
BEFORE DELETE ON teams
FOR EACH ROW
WHEN NOT EXISTS (
    SELECT 1 FROM events WHERE subject_type = 'TEAM' AND subject_id = OLD.id
)
BEGIN
    SELECT RAISE(ABORT, 'Team history is retained with the Team');
END;

CREATE TRIGGER teams_retained_entity_replace_guard
BEFORE INSERT ON teams
FOR EACH ROW
WHEN EXISTS (SELECT 1 FROM teams AS existing WHERE existing.id = NEW.id)
 AND NOT EXISTS (
    SELECT 1 FROM teams AS existing
    JOIN events AS event
      ON event.subject_type = 'TEAM'
     AND event.subject_id = existing.id
    WHERE existing.id = NEW.id
 )
BEGIN
    SELECT RAISE(ABORT, 'Team history is retained with the Team');
END;

CREATE TRIGGER teams_retained_entity_identity_immutable_update
BEFORE UPDATE OF id ON teams
FOR EACH ROW
WHEN NOT EXISTS (
    SELECT 1 FROM events WHERE subject_type = 'TEAM' AND subject_id = OLD.id
)
BEGIN
    SELECT RAISE(ABORT, 'Team identity is immutable');
END;

-- ===========================================================================
-- Role
--
-- Two unique keys: the primary key and `roles_team_display_id_unique
-- (team_id, display_id)`. The second arm is the one that survives a different
-- primary key, and it is what `0014`/`0015` were reaching for only through an
-- Event join.
-- ===========================================================================

CREATE TRIGGER roles_retained_entity_retention_delete
BEFORE DELETE ON roles
FOR EACH ROW
WHEN NOT EXISTS (
    SELECT 1 FROM events WHERE subject_type = 'ROLE' AND subject_id = OLD.id
) AND NOT EXISTS (
    SELECT 1 FROM events WHERE actor_type = 'ROLE' AND actor_id = OLD.id
) AND NOT EXISTS (
    SELECT 1 FROM events WHERE actor_kind = 'ROLE_BINDING' AND actor_role_id = OLD.id
)
BEGIN
    SELECT RAISE(ABORT, 'Role history is retained with the Team');
END;

CREATE TRIGGER roles_retained_entity_replace_guard
BEFORE INSERT ON roles
FOR EACH ROW
WHEN EXISTS (
    SELECT 1 FROM roles AS existing
    WHERE existing.id = NEW.id
       OR (existing.team_id = NEW.team_id AND existing.display_id = NEW.display_id)
)
 AND NOT EXISTS (
    SELECT 1 FROM roles AS existing
    JOIN events AS event
      ON (event.subject_type = 'ROLE' AND event.subject_id = existing.id)
      OR (event.actor_type = 'ROLE' AND event.actor_id = existing.id)
    WHERE existing.id = NEW.id
       OR (existing.team_id = NEW.team_id AND existing.display_id = NEW.display_id)
 )
 AND NOT EXISTS (
    SELECT 1 FROM roles AS existing
    JOIN events AS event
      ON event.actor_kind = 'ROLE_BINDING'
     AND event.actor_role_id = existing.id
    WHERE existing.id = NEW.id
       OR (existing.team_id = NEW.team_id AND existing.display_id = NEW.display_id)
 )
BEGIN
    SELECT RAISE(ABORT, 'Role history is retained with the Team');
END;

-- Only `id`. `team_id` is left to `0010`/`0011`/`0012`/`0013`, which refuse it
-- whenever a Binding, Task, Run/Review or Reference points at the Role, and
-- `run_lease_consistency_forward_migration.rs` asserts `0012`'s own message for
-- that case. Restating it here would shadow a diagnostic another package proved.
CREATE TRIGGER roles_retained_entity_identity_immutable_update
BEFORE UPDATE OF id ON roles
FOR EACH ROW
WHEN NOT EXISTS (
    SELECT 1 FROM events WHERE subject_type = 'ROLE' AND subject_id = OLD.id
) AND NOT EXISTS (
    SELECT 1 FROM events WHERE actor_type = 'ROLE' AND actor_id = OLD.id
) AND NOT EXISTS (
    SELECT 1 FROM events WHERE actor_kind = 'ROLE_BINDING' AND actor_role_id = OLD.id
) AND NOT EXISTS (
    SELECT 1 FROM notifications WHERE recipient_human_id = NEW.id
)
BEGIN
    SELECT RAISE(ABORT, 'Role identity is immutable');
END;

-- ===========================================================================
-- Task
--
-- Same two unique keys as Role: the primary key and
-- `tasks_team_display_id_unique (team_id, display_id)`.
-- ===========================================================================

CREATE TRIGGER tasks_retained_entity_retention_delete
BEFORE DELETE ON tasks
FOR EACH ROW
WHEN NOT EXISTS (
    SELECT 1 FROM "references" WHERE subject_type = 'TASK' AND subject_id = OLD.id
) AND NOT EXISTS (
    SELECT 1 FROM events WHERE subject_type = 'TASK' AND subject_id = OLD.id
)
BEGIN
    SELECT RAISE(ABORT, 'Task history is retained with the Team');
END;

CREATE TRIGGER tasks_retained_entity_replace_guard
BEFORE INSERT ON tasks
FOR EACH ROW
WHEN EXISTS (
    SELECT 1 FROM tasks AS existing
    WHERE existing.id = NEW.id
       OR (existing.team_id = NEW.team_id AND existing.display_id = NEW.display_id)
)
 AND NOT EXISTS (
    SELECT 1 FROM tasks AS existing
    JOIN "references" AS reference
      ON reference.subject_type = 'TASK'
     AND reference.subject_id = existing.id
    WHERE existing.id = NEW.id
       OR (existing.team_id = NEW.team_id AND existing.display_id = NEW.display_id)
 )
 AND NOT EXISTS (
    SELECT 1 FROM tasks AS existing
    JOIN events AS event
      ON event.subject_type = 'TASK'
     AND event.subject_id = existing.id
    WHERE existing.id = NEW.id
       OR (existing.team_id = NEW.team_id AND existing.display_id = NEW.display_id)
 )
BEGIN
    SELECT RAISE(ABORT, 'Task history is retained with the Team');
END;

CREATE TRIGGER tasks_retained_entity_identity_immutable_update
BEFORE UPDATE OF id ON tasks
FOR EACH ROW
WHEN NOT EXISTS (
    SELECT 1 FROM "references" WHERE subject_type = 'TASK' AND subject_id = OLD.id
) AND NOT EXISTS (
    SELECT 1 FROM events WHERE subject_type = 'TASK' AND subject_id = OLD.id
)
BEGIN
    SELECT RAISE(ABORT, 'Task identity is immutable');
END;

-- ===========================================================================
-- Binding
--
-- DATA_CONVENTIONS section 5 puts a Binding in the "does not rewrite its
-- historical semantics, only appends" list, and DOMAIN_MODEL section 2 calls it
-- "版本化且追加历史". The row is what is retained here: its status, version and
-- snapshot stay editable, because a Binding that cannot advance is not a
-- retained Binding. Only removal and silent replacement are closed.
-- ===========================================================================

CREATE TRIGGER bindings_retained_entity_retention_delete
BEFORE DELETE ON bindings
FOR EACH ROW
WHEN NOT EXISTS (
    SELECT 1 FROM events WHERE subject_type = 'BINDING' AND subject_id = OLD.id
) AND NOT EXISTS (
    SELECT 1 FROM events WHERE actor_kind = 'ROLE_BINDING' AND actor_binding_id = OLD.id
)
BEGIN
    SELECT RAISE(ABORT, 'Binding history is retained with the Team');
END;

-- The single-ACTIVE-per-Role arm is deliberately absent: `0002`'s overlap guard
-- already refuses a second ACTIVE Binding for one Role. See the header.
CREATE TRIGGER bindings_retained_entity_replace_guard
BEFORE INSERT ON bindings
FOR EACH ROW
WHEN EXISTS (
    SELECT 1 FROM bindings AS existing
    WHERE existing.id = NEW.id
       OR (existing.role_id = NEW.role_id
           AND existing.binding_generation = NEW.binding_generation)
)
 AND NOT EXISTS (
    SELECT 1 FROM bindings AS existing
    JOIN events AS event
      ON event.subject_type = 'BINDING'
     AND event.subject_id = existing.id
    WHERE existing.id = NEW.id
       OR (existing.role_id = NEW.role_id
           AND existing.binding_generation = NEW.binding_generation)
 )
 AND NOT EXISTS (
    SELECT 1 FROM bindings AS existing
    JOIN events AS event
      ON event.actor_kind = 'ROLE_BINDING'
     AND event.actor_binding_id = existing.id
    WHERE existing.id = NEW.id
       OR (existing.role_id = NEW.role_id
           AND existing.binding_generation = NEW.binding_generation)
 )
BEGIN
    SELECT RAISE(ABORT, 'Binding history is retained with the Team');
END;

-- `id` only, and that is a measured result rather than an omission.
--
-- An earlier draft of this file also froze `role_id` and `harness_instance_id`,
-- on the argument that `0015` names exactly those columns as snapshot-critical
-- for a `ROLE_BINDING` Event. It broke
-- `binding_team_consistency_forward_migration.rs`, which re-points an `ACTIVE`
-- Binding at a different Harness and Adapter **of the same Team** after a
-- forward upgrade and expects that to succeed: re-pointing a Binding at another
-- executor is ordinary business behaviour, not history tampering, and freezing
-- it would have welded shut a legitimate edit.
--
-- `0015` covers those columns when a `ROLE_BINDING` Event has actually
-- snapshotted them; `0010` refuses a cross-Team move; `0012` refuses a move that
-- breaks the Run/Review tuple. Nothing frozen says a same-Team executor swap is
-- forbidden, so nothing here says it either.
CREATE TRIGGER bindings_retained_entity_identity_immutable_update
BEFORE UPDATE OF id ON bindings
FOR EACH ROW
WHEN NOT EXISTS (
    SELECT 1 FROM events WHERE actor_kind = 'ROLE_BINDING' AND actor_binding_id = OLD.id
) AND NOT EXISTS (
    SELECT 1 FROM events WHERE subject_type = 'BINDING' AND subject_id = OLD.id
)
BEGIN
    SELECT RAISE(ABORT, 'Binding identity is immutable');
END;

-- ===========================================================================
-- Reference
--
-- The table name is quoted: `references` is an SQL reserved word. It is a leaf
-- (nothing points at one) and it had no `BEFORE DELETE` trigger in `0001..0027`,
-- so nothing stood between it and a `DELETE` at all.
--
-- Only the primary key is unique here, so -- as with `teams` -- the same-id arm
-- is the whole story.
-- ===========================================================================

CREATE TRIGGER references_retained_entity_retention_delete
BEFORE DELETE ON "references"
FOR EACH ROW
BEGIN
    SELECT RAISE(ABORT, 'Reference history is retained with the Team');
END;

CREATE TRIGGER references_retained_entity_replace_guard
BEFORE INSERT ON "references"
FOR EACH ROW
WHEN EXISTS (SELECT 1 FROM "references" AS existing WHERE existing.id = NEW.id)
BEGIN
    SELECT RAISE(ABORT, 'Reference history is retained with the Team');
END;

-- `validation_status`, `validated_at` and `version` stay editable -- that is
-- how a broken reference is recorded, per DATA_CONVENTIONS section 5 and
-- DOMAIN_MODEL section 5. The recorded target does not move.
CREATE TRIGGER references_retained_entity_identity_immutable_update
BEFORE UPDATE OF id, target_uri, checksum_sha256 ON "references"
FOR EACH ROW
BEGIN
    SELECT RAISE(ABORT, 'Reference target is immutable');
END;

-- ===========================================================================
-- Delivery Attempt
--
-- `0017` already refuses a same-id or same-`(message_id, attempt_number)`
-- replacement with its own message, and that trigger is left exactly as it is.
-- The two additions here are the missing arm and the missing `id` guard.
-- ===========================================================================

CREATE TRIGGER delivery_attempts_retained_entity_retention_delete
BEFORE DELETE ON delivery_attempts
FOR EACH ROW
BEGIN
    SELECT RAISE(ABORT, 'Delivery attempt history is retained with the Team');
END;

-- `delivery_attempts_single_active ON (message_id) WHERE status IN
-- ('PENDING','IN_FLIGHT')`. A retry that follows a terminal attempt does not
-- collide with this index and is still accepted; what is refused is replacing a
-- live attempt with a differently numbered one, which deletes the live row.
CREATE TRIGGER delivery_attempts_retained_entity_active_slot_replace_guard
BEFORE INSERT ON delivery_attempts
FOR EACH ROW
WHEN EXISTS (
    SELECT 1 FROM delivery_attempts AS existing
    WHERE existing.message_id = NEW.message_id
      AND existing.status IN ('PENDING', 'IN_FLIGHT')
      AND NEW.status IN ('PENDING', 'IN_FLIGHT')
)
 AND NOT EXISTS (
    SELECT 1 FROM delivery_attempts AS existing
    WHERE existing.id = NEW.id
       OR (existing.message_id = NEW.message_id
           AND existing.attempt_number = NEW.attempt_number)
 )
BEGIN
    SELECT RAISE(ABORT, 'Delivery attempt history is retained with the Team');
END;

-- `0017`'s identity trigger covers message_id, team_id, attempt_number,
-- target_role_id, target_binding_id, target_binding_generation and created_at.
-- It does not cover `id`, so an attempt could be renumbered in place.
CREATE TRIGGER delivery_attempts_retained_entity_identity_immutable_update
BEFORE UPDATE OF id ON delivery_attempts
FOR EACH ROW
BEGIN
    SELECT RAISE(ABORT, 'Delivery attempt identity is immutable');
END;