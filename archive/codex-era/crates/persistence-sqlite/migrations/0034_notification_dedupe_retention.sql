-- DEV-02-09: DATA_CONVENTIONS section 6 retains Notification facts with Team.
-- Deduplication merges presentation, not history (STATE_MACHINES section 6).
--
-- 0019 guards replacement only by primary key. 0023's DELETE guard does not
-- fire for SQLite's implicit REPLACE deletion when recursive_triggers is OFF.
-- A fresh ID colliding with 0008's partial active dedupe index could therefore
-- delete an UNREAD or READ fact. Cover that secondary key before insertion,
-- independent of connection recursive_triggers. Same-ID conflicts remain
-- owned by 0019/0014. NULL keys and terminal rows do not occupy the index.
--
-- This raises ABORT for every insert conflict mode, including OR IGNORE:
-- callers must explicitly find/reuse the retained presentation instead of
-- relying on silent conflict handling. No production Notification writer
-- currently uses either IGNORE or REPLACE. No dedupe window is interpreted.
-- UPDATE cannot create this collision: identity/recipient/key are immutable
-- and no transition enters UNREAD/READ from an inactive state (0019).
--
-- Forward-only schema change, no DML or historical-state inference. Existing
-- rows already obey the unique index; no upgrade data guard is necessary.
-- SQLx applies this single migration in a transaction. Preserve 0001..0033.
CREATE TRIGGER notifications_active_dedupe_retention_insert
BEFORE INSERT ON notifications
FOR EACH ROW
WHEN NEW.dedupe_key IS NOT NULL
 AND NEW.status IN ('UNREAD', 'READ')
 AND NOT EXISTS (SELECT 1 FROM notifications WHERE id = NEW.id)
 AND EXISTS (
     SELECT 1 FROM notifications AS existing
     WHERE existing.team_id = NEW.team_id
       AND existing.recipient_human_id = NEW.recipient_human_id
       AND existing.dedupe_key = NEW.dedupe_key
       AND existing.status IN ('UNREAD', 'READ')
 )
BEGIN
    SELECT RAISE(ABORT, 'Notification dedupe conflict retains history');
END;
