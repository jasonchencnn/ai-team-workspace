-- MM-R02 (deep audit DA-02): the persisted history of a Run, a Review
-- decision, an Artifact index row, a Message and an Approval is retained for
-- the life of its Team.
--
-- What the deep audit reproduced against `0001..0025` (log:
-- `docs/reviews/evidence/deep-audit-2026-10-08/storage-boundaries-red-v2.log`,
-- ported and re-run in `tests/core_history_retention.rs`): with a row that no
-- `events` row and no `"references"` row names, `DELETE` succeeded on all five
-- tables and each really fell 1 -> 0.
--
-- This is **not** "the five tables had no protection". Two of the four gaps
-- that produced it are conditional by construction, and two tables had no
-- guard of the relevant kind at all:
--
--   table       `0013` Reference guard        `0014` Event subject guard
--   ----------  ----------------------------  -----------------------------
--   runs        yes, only when a `"references"` row names it   yes, same
--   artifacts   yes, same                    yes, same
--   messages    yes, same                    yes, same
--   approvals   **none** -- `0005` does not    yes, same
--               allow `APPROVAL` as a
--               `"references".subject_type`
--   reviews     **none** -- `0005` has no      **none** -- `0006` has no
--               `REVIEW` subject_type         `REVIEW` subject_type
--
-- So `reviews` was reachable by `DELETE` and by `INSERT OR REPLACE` with no
-- obstacle but its own foreign keys, and `approvals` was reachable whenever it
-- had no Event yet. The pre-existing guards are kept exactly as they are; the
-- cases they already covered keep their own message and keep firing. This
-- migration only removes the dependency on a referencing row existing.
--
-- The `INSERT OR REPLACE` arm is not decorative. SQLite implements `OR REPLACE`
-- as a hidden delete of the conflicting row followed by an insert, and with the
-- shipped `PRAGMA recursive_triggers = 0` that hidden delete does **not** fire
-- the table's `BEFORE DELETE` triggers. DEV-02-14 already had to answer this
-- for `command_idempotency`; `0013`, `0014` and `0020` already carry
-- `*_replace_guard` triggers for the same reason.
--
-- Only **two** tables get a replace guard here, and that is a measured result
-- rather than an omission. Making every guard permanently false and re-running
-- `tests/core_history_retention.rs` showed that `runs`, `approvals` and
-- `messages` were already refused before this migration ran, so a guard for
-- them would have been unproven code asserting a guarantee somebody else
-- already provides:
--
--   table       who already refuses a same-id `INSERT OR REPLACE`
--   ----------  --------------------------------------------------------
--   runs        `0012` `runs_consistency_insert` -- an `OR REPLACE` colliding
--               on `id` still presents the conflicting row when `BEFORE INSERT`
--               runs, so `attempt = MAX(attempt) + 1` cannot hold. Its
--               `(task_id, attempt)` and `(task_id, fencing_token)` collision
--               arms fail the same clause for the same reason.
--   approvals   `0020` `approvals_replace_guard`, an identical predicate.
--   messages    `0017` `messages_identity_replace_guard` for `id` and
--               `messages_idempotency_replace_guard` for
--               `(team_id, idempotency_key)`.
--   reviews     **nothing** -- see below.
--   artifacts   only `0013`/`0014`, and only when a Reference or an Event
--               names the row.
--
-- The third axis is identity. `0013` and `0014` guard `UPDATE OF id` and
-- `UPDATE OF id, team_id` on the rows that a Reference or an Event happens to
-- name; an orphan row could be renumbered into another Team. The same mutation
-- run found that `approvals` is already covered by `0020`
-- `approvals_request_immutable_update`, so only four tables get a guard here.
-- No further columns are restated: `runs.role_id`, `task_id`, `binding_id`,
-- `harness_instance_id`, `attempt` and `fencing_token` already have their own
-- guards in `0012`, and duplicating them here would be a second source of truth
-- for rules this package did not write.
--
-- Frozen basis, none of which this migration extends:
--   - DATA_CONVENTIONS.md section 5: "P0 核心对象无通用 DELETE 命令", and
--     "Event、Run、Binding、Review decision、DeliveryAttempt、Approval decision
--     不更新历史语义，只追加后续事实".
--   - DATA_CONVENTIONS.md section 6: Team/Role/Task/Binding/Run/Event/Review
--     are "随 Team 永久保留"; Message/Delivery/Notification/Approval are
--     "随 Team 保留"; the Artifact index is "随 Team 保留" and keeps its
--     metadata when the external file is gone.
--   - DOMAIN_MODEL.md section 5: "核心对象通过稳定 ID 关联，不级联硬删除".
--   - DOMAIN_MODEL.md section 2: every one of these five is a stable identity.
--
-- Scope notes, so this file is not read as claiming more than it does:
--   - Attention items are deliberately **not** protected here. Deep audit
--     section 4 records that Attention is a rebuildable derived index, that the
--     DATA retention table does not list it separately, and that it is
--     "暂不计 DA-02 永久核心历史集合". No claim is made that this migration
--     fixes Attention deletion.
--   - The `"references"` table itself still has no `BEFORE DELETE` trigger in
--     `0001..0025`. DATA_CONVENTIONS section 5 says a reference that stops
--     being valid only has its `validation_status` updated, so that gap is
--     real, but it is a separate finding rather than part of the five tables
--     this package was given. It does not reopen DA-02: with this migration a
--     Run, Review, Artifact, Message or Approval is retained whether or not a
--     `"references"` row survives.
--   - `team_id` is guarded on four tables but is not probed for a fifth
--     (`reviews`), because `0012` already refuses a Review's Team move and
--     restating it there would shadow that guard's message for no gain.
--
-- No new state, status, error code, permission or protocol meaning is added.
-- Every trigger raises `ABORT` with a message only, which is the same shape
-- `0013`, `0014`, `0016`, `0017` and `0020` already use.
--
-- No upgrade guard table is created. `0025` and `0020` need one because they
-- constrain rows that already exist; this migration constrains only statements
-- issued *after* it is applied, so every existing row already satisfies it and
-- a `valid = 1` probe would be the tautology ADVERSARIAL_REVIEW_PLAYBOOK rule 5
-- warns about. What the upgrade must guarantee instead is that it changes
-- nothing, which `upgrading_to_0026_preserves_every_existing_row` and
-- `a_failed_upgrade_has_no_side_effects` check directly.
--
-- Forward migration: `0001..0025` are never modified and no checksum drifts.
-- SQLx applies this file in one transaction, so a failure leaves neither the
-- schema nor the data changed.

-- ===========================================================================
-- Run
-- ===========================================================================

CREATE TRIGGER runs_core_history_retention_delete
BEFORE DELETE ON runs
FOR EACH ROW
BEGIN
    SELECT RAISE(ABORT, 'Run history is retained with the Team');
END;

CREATE TRIGGER runs_core_history_identity_immutable_update
BEFORE UPDATE OF id, team_id ON runs
FOR EACH ROW
BEGIN
    SELECT RAISE(ABORT, 'Run identity is immutable');
END;

-- ===========================================================================
-- Review decision
--
-- `0012` guards a Review's Team/Task/Run/Reviewer consistency, but a Review has
-- no Reference and no Event subject type, so nothing here is load-bearing
-- elsewhere. This is the whole of its retention protection.
-- ===========================================================================

CREATE TRIGGER reviews_core_history_retention_delete
BEFORE DELETE ON reviews
FOR EACH ROW
BEGIN
    SELECT RAISE(ABORT, 'Review decision history is retained with the Team');
END;

CREATE TRIGGER reviews_core_history_retention_replace_guard
BEFORE INSERT ON reviews
FOR EACH ROW
WHEN EXISTS (SELECT 1 FROM reviews AS existing WHERE existing.id = NEW.id)
BEGIN
    SELECT RAISE(ABORT, 'Review decision history is retained with the Team');
END;

-- Only `id`, deliberately. `0012`'s `reviews_consistency_update` already refuses
-- a `team_id` / `task_id` / `run_id` / `reviewer_role_id` move unconditionally,
-- with its own message, and restating those columns here would shadow it:
-- `tests/run_lease_consistency_forward_migration.rs` asserts that a Review's
-- Team move is refused *by that guard*, and it is right to. Nothing references
-- a Review, so `id` is the only identity column with no other guard behind it.
CREATE TRIGGER reviews_core_history_identity_immutable_update
BEFORE UPDATE OF id ON reviews
FOR EACH ROW
BEGIN
    SELECT RAISE(ABORT, 'Review decision identity is immutable');
END;

-- ===========================================================================
-- Artifact index
--
-- The index row is retained, not the external file: DATA_CONVENTIONS section 6
-- keeps metadata and checksum even when the file is gone. So this guards the
-- index row only and says nothing about the bytes it points at.
-- ===========================================================================

CREATE TRIGGER artifacts_core_history_retention_delete
BEFORE DELETE ON artifacts
FOR EACH ROW
BEGIN
    SELECT RAISE(ABORT, 'Artifact index is retained with the Team');
END;

CREATE TRIGGER artifacts_core_history_retention_replace_guard
BEFORE INSERT ON artifacts
FOR EACH ROW
WHEN EXISTS (SELECT 1 FROM artifacts AS existing WHERE existing.id = NEW.id)
BEGIN
    SELECT RAISE(ABORT, 'Artifact index is retained with the Team');
END;

CREATE TRIGGER artifacts_core_history_identity_immutable_update
BEFORE UPDATE OF id, team_id ON artifacts
FOR EACH ROW
BEGIN
    SELECT RAISE(ABORT, 'Artifact index identity is immutable');
END;

-- ===========================================================================
-- Approval
--
-- `0020`'s `approvals_terminal_delete_guard` covers `OLD.status != 'PENDING'`,
-- which left the request row itself removable while it was still open. That
-- gap is the one the `0020` forward-migration test asserted as correct; a
-- cancellation is `PENDING -> CANCELLED` plus its actor (STATE_MACHINES
-- section 7), so the open request is evidence too.
--
-- This trigger is unconditional, so it also covers the decided states `0020`
-- already refused -- with `0020` still in place and still firing on its own
-- message for those rows.
-- ===========================================================================

CREATE TRIGGER approvals_core_history_retention_delete
BEFORE DELETE ON approvals
FOR EACH ROW
BEGIN
    SELECT RAISE(ABORT, 'Approval request history is retained with the Team');
END;

-- ===========================================================================
-- Message
--
-- DATA_CONVENTIONS section 5 lets a Human revise a `DRAFT` body, and freezes
-- it at `QUEUED`; `0017` and `0018` already enforce that. Nothing about a
-- draft's revisability is touched here: this guards removal of the row, not
-- editing of its content, and the editable columns stay editable.
-- ===========================================================================

CREATE TRIGGER messages_core_history_retention_delete
BEFORE DELETE ON messages
FOR EACH ROW
BEGIN
    SELECT RAISE(ABORT, 'Message history is retained with the Team');
END;

CREATE TRIGGER messages_core_history_identity_immutable_update
BEFORE UPDATE OF id, team_id ON messages
FOR EACH ROW
BEGIN
    SELECT RAISE(ABORT, 'Message identity is immutable');
END;