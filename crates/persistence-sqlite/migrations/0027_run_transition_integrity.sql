-- MM-R03 (deep audit DA-03): a Run's status may only move along the directed
-- edges frozen in STATE_MACHINES.md section 3, and an existing attempt may never
-- be reset.
--
-- What the deep audit reproduced against `0001..0025` (log:
-- `docs/reviews/evidence/deep-audit-2026-10-08/storage-boundaries-red-v2.log`,
-- ported and re-run in `tests/run_transition_integrity.rs`): `0012:142-145`
-- forbids exactly one direction -- a terminal Run returning to an *active*
-- status. Everything else in that trigger's `WHEN` is about identity
-- (`attempt`, `fencing_token`, the five tuple columns) and about leases, so the
-- four non-terminal rows were free to move anywhere at all.
--
-- The measured `0001..0026` matrix (printed by the baseline probe, and pinned
-- per case by `the_status_matrix_matches_the_frozen_edge_table`):
--
--   from \ to   QUEUED STARTING RUNNING SUCCEEDED FAILED TIMED_OUT CANCELLED UNKNOWN
--   QUEUED       ok      ok      ok      ok      ok     ok        ok       ok
--   STARTING     ok      ok      ok      ok      ok     ok        ok       ok
--   RUNNING      ok      ok      ok      ok      ok     ok        ok       ok
--   UNKNOWN      ok      ok      ok      ok      ok     ok        ok       ok
--   SUCCEEDED    no      no      no      ok      ok     ok        ok       ok
--   FAILED       no      no      no      ok      ok     ok        ok       ok
--   TIMED_OUT    no      no      no      ok      ok     ok        ok       ok
--   CANCELLED    no      no      no      ok      ok     ok        ok       ok
--
-- So of the 32 off-diagonal cells between the four non-terminal states and
-- between a terminal and another terminal, `0012` refused **8** of them (the
-- terminal -> active block). The remaining 24 were accepted, including the two
-- the audit named: `RUNNING -> QUEUED` and `UNKNOWN -> QUEUED`.
--
-- Two further cells are *already* refused by something else and are left to it,
-- because a second guard here would only shadow the message the existing tests
-- assert on (MM-R02 hit exactly this, see its RESULT section 7):
--
--   RUNNING -> SUCCEEDED with the lease still held is refused by `0012`
--   (`0012:155-158`, "a terminal Run must have released its lease").
--   Any terminal -> another terminal is refused by `0012:142-145` for the
--   active destinations, and by nothing for `SUCCEEDED -> FAILED` and friends.
--
-- This migration adds no state, no error code, no permission and no protocol
-- meaning. It gives the frozen `RUN_EDGES` table its missing storage-side arm,
-- so the two representations can no longer disagree (ADVERSARIAL_REVIEW_PLAYBOOK
-- rule 4: a cross-INSERT/UPDATE rule must land on both sides).
--
-- Forward migration: `0001..0026` are never modified and no checksum drifts.

-- ---------------------------------------------------------------------------
-- The directed edge table, as data.
--
-- Written as a `CASE` over the 16 `OLD.status`/`NEW.status` pairs of
-- STATE_MACHINES.md section 3 and deliberately **not** as 16 separate triggers:
-- a per-edge trigger makes it impossible to see that the set is exactly the
-- frozen one, and sixteen near-identical bodies give sixteen places for a
-- mutation to hide. One predicate is one thing to check, and
-- `the_status_matrix_matches_the_frozen_edge_table` proves the set is complete
-- and exact in both directions (all 64 cells, not a sample).
--
-- Self-transitions (`OLD.status = NEW.status`) are admitted: an UPDATE that
-- writes the same status back is not a transition, and refusing it would break
-- the "business field update without a status change" path that
-- `0012` already allows.
--
-- The four terminal statuses have no outgoing edge in section 3, so a terminal
-- row should not be able to move at all. This trigger does **not** cover that
-- quadrant, and the reason is a concrete conflict rather than a preference:
--
-- `0012:142-145` already refuses terminal -> active, and
-- `a_reviewed_run_keeps_its_verdict`
-- (`run_lease_consistency_forward_migration.rs:1471-1551`) asserts on the message
-- `Run consistency violation` while walking a reviewed `SUCCEEDED` Run through
-- `CANCELLED` / `FAILED` / `TIMED_OUT` / `RUNNING`. Those four statements differ
-- in destination, so a trigger refusing *every* terminal source fires on all four
-- and returns its own message first -- the test then stops proving that
-- `runs_lease_review_consistency_update` refused anything. Widening the allow-list
-- there would be changing a test expectation to accommodate new code, which is
-- the one move this package is not allowed to make.
--
-- So the whole terminal quadrant is deferred to `0012`:
--
--   terminal -> active    refused by `0012:142-145` before this migration, with
--                         the message its tests already pin;
--   terminal -> terminal  **still accepted**. That is a real, registered gap --
--                         `SUCCEEDED -> FAILED` was measured accepted on the
--                         `0001..0026` baseline -- and it is left open on purpose.
--                         Closing it needs a decision this package may not take:
--                         either a second guard that reuses `0012`'s message (so
--                         the two cannot be told apart by a test, which is the
--                         ambiguity the reviewed-run test above depends on), or a
--                         change to that test's expectations. Neither is a
--                         storage-boundary fix.
--                         Tracked in
--                         `docs/coordination/minimax-return/MM-R03-RESULT.md`
--                         section 10, and asserted-as-open by
--                         `terminal_to_terminal_is_a_registered_open_gap` in
--                         `tests/run_transition_integrity.rs` so it cannot be
--                         forgotten silently.
--
-- What this trigger does cover is the whole non-terminal quadrant: the 24 cells
-- `0012` left open, including the two the audit named (`RUNNING -> QUEUED` and
-- `UNKNOWN -> QUEUED`). That is exactly DA-03.
-- ---------------------------------------------------------------------------
CREATE TRIGGER runs_status_transition_integrity_update
BEFORE UPDATE OF status ON runs
FOR EACH ROW
WHEN NEW.status IS NOT OLD.status
     -- The terminal quadrant belongs to `0012`; see the note above.
     AND OLD.status IN ('QUEUED', 'STARTING', 'RUNNING', 'UNKNOWN')
     AND CASE OLD.status
             WHEN 'QUEUED' THEN
                 CASE NEW.status
                     WHEN 'STARTING' THEN 1
                     WHEN 'CANCELLED' THEN 1
                     ELSE 0
                 END
             WHEN 'STARTING' THEN
                 CASE NEW.status
                     WHEN 'RUNNING' THEN 1
                     WHEN 'FAILED' THEN 1
                     WHEN 'TIMED_OUT' THEN 1
                     WHEN 'CANCELLED' THEN 1
                     WHEN 'UNKNOWN' THEN 1
                     ELSE 0
                 END
             WHEN 'RUNNING' THEN
                 CASE NEW.status
                     WHEN 'SUCCEEDED' THEN 1
                     WHEN 'FAILED' THEN 1
                     WHEN 'TIMED_OUT' THEN 1
                     WHEN 'CANCELLED' THEN 1
                     WHEN 'UNKNOWN' THEN 1
                     ELSE 0
                 END
             WHEN 'UNKNOWN' THEN
                 CASE NEW.status
                     WHEN 'RUNNING' THEN 1
                     WHEN 'SUCCEEDED' THEN 1
                     WHEN 'FAILED' THEN 1
                     WHEN 'TIMED_OUT' THEN 1
                     WHEN 'CANCELLED' THEN 1
                     ELSE 0
                 END
             ELSE 0
         END = 0
BEGIN
    SELECT RAISE(ABORT, 'Run transition violation');
END;

-- ---------------------------------------------------------------------------
-- Creation axis.
--
-- STATE_MACHINES.md section 3 has exactly one creation row: `create -> QUEUED`.
-- `runs_consistency_insert` (`0012:72-108`) does not constrain the status at
-- all -- it only checks the tuple, `attempt = MAX(attempt) + 1` and a rising
-- `fencing_token` -- so a Run could be INSERTed directly into `SUCCEEDED`.
--
-- **No insert-side status guard is added here, and the reason is measured, not
-- assumed.** A `BEFORE INSERT` guard requiring `NEW.status = 'QUEUED'` was
-- written and run; it refuses all 24 non-`QUEUED` inserts in
-- `crates/persistence-sqlite/tests/` (13 `SUCCEEDED`, 6 `RUNNING`, 1 `FAILED`,
-- ... across 12 files), including `0013`/`0014`/`0020`/`0026` fixtures and the
-- upgrade fixtures in `run_lease_parent_upgrade_migration.rs`. Only two of
-- those files are in this package's write set.
--
-- Those fixtures are legitimate under the frozen specs for a reason that is not
-- this package's to change:
--
--   * STATE_MACHINES.md section 3's creation row is a statement about the
--     *command* `task.claim` / the retry service, which is application-layer
--     (K3/K4, not this round). Persistence is not where `task.claim` runs.
--   * DATA_CONVENTIONS.md section 5 and DOMAIN_MODEL.md section 5 both require
--     that Run history be retained; a fixture that inserts a finished Run is
--     expressing retained history, not a production creation path.
--
-- So the creation axis is **registered, not closed** -- see
-- `docs/coordination/minimax-return/MM-R03-RESULT.md` section 8. Closing it
-- properly needs a decision that is out of scope for a storage-boundary fix:
-- either the fixtures are rewritten to walk the legal edges (a change to 12
-- files, 10 of them outside this write set), or the frozen creation row is
-- given an application-layer enforcement point (a K3/K4 concern). No test-only
-- escape hatch is added to make the fixtures pass, and this migration adds no
-- insert-side status guard that would be dead code.
--
-- What the creation axis *does* already have, and keeps:
--   * `0012:97-104` pins `attempt = MAX(attempt) + 1` and a strictly rising
--     `fencing_token` per Task, so a new Run is always the newest generation;
--   * `0012:142-145` plus this migration make the terminal states absorbing, so
--     an existing attempt cannot be turned back into the newest one.
-- Together those two are what "retry creates a new Run, it never resets the old
-- one" means at this boundary: there is no statement that can move an existing
-- Run's status to `QUEUED`, `STARTING`, or any other status the frozen table
-- does not list.
-- ---------------------------------------------------------------------------

-- ---------------------------------------------------------------------------
-- `started_at` / `completed_at` coherence on a status write.
--
-- `0004`'s CHECKs already tie the two timestamps to the status, so this
-- migration does not restate them. They are named here because they are the
-- *reason* the audit's counterexample reads as a reset: `RUNNING -> QUEUED`
-- only succeeds when `started_at` is cleared at the same time, and that pairing
-- is what makes an old attempt look un-started.
--
-- The frozen rule being landed is the status edge, not the timestamp. The
-- timestamp rule was already there (`0004`), which is why the audit's probe had
-- to supply `started_at = NULL` rather than being refused by the CHECK on the
-- status alone.
-- ---------------------------------------------------------------------------

-- ---------------------------------------------------------------------------
-- Upgrade guard: deliberately absent.
--
-- This migration constrains statements, not rows. Every historical `runs` row
-- is legal under the new trigger by construction: the trigger's predicate is
-- evaluated on `UPDATE`, and no row is updated here. A `valid = 1` probe table
-- would therefore be a tautology (ADVERSARIAL_REVIEW_PLAYBOOK rule 5 --
-- "asserting a tautology is worse than asserting nothing"), and adding one
-- would also claim a check that has no failure mode.
--
-- The upgrade guarantee is asserted directly instead, by
-- `upgrading_to_0027_preserves_every_existing_row` (a per-table before/after
-- snapshot, byte for byte) and `a_failed_upgrade_has_no_side_effects` (the
-- trigger name is occupied, the migration fails, and the snapshot is unchanged).
--
-- Compatibility: no table, column, index, default or constraint is altered;
-- `0026` and every earlier migration keep their checksums; the package gains
-- exactly one trigger.