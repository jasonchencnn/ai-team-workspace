-- DEV-02-09: stable, same-Team source references (DOMAIN_MODEL sections 2/5).
-- 0019 validates Attention writes, but not parent moves, and its upgrade scan
-- checks existence without Team equality. Reject incompatible old references
-- atomically; never infer historical resolution validity from current status.
-- SYSTEM has no frozen backing table: keep its mapping decision in ADR-0007.
CREATE TABLE attention_source_reference_upgrade_guard (
    valid INTEGER NOT NULL CHECK (valid = 1)
);
INSERT INTO attention_source_reference_upgrade_guard(valid)
SELECT 0 WHERE EXISTS (
    SELECT 1 FROM attention_items AS a
    WHERE (a.source_type = 'TASK' AND NOT EXISTS (
        SELECT 1 FROM tasks WHERE id = a.source_id AND team_id = a.team_id
    )) OR (a.source_type = 'RUN' AND NOT EXISTS (
        SELECT 1 FROM runs WHERE id = a.source_id AND team_id = a.team_id
    )) OR (a.source_type = 'MESSAGE' AND NOT EXISTS (
        SELECT 1 FROM messages WHERE id = a.source_id AND team_id = a.team_id
    )) OR (a.source_type = 'APPROVAL' AND NOT EXISTS (
        SELECT 1 FROM approvals WHERE id = a.source_id AND team_id = a.team_id
    ))
);
DROP TABLE attention_source_reference_upgrade_guard;

-- Source IDs and deletion are already protected by the parent history guards.
-- Run Team is already immutable (0012/0026); Approval Team is immutable (0020).
-- Message Team is already immutable under 0026, including DRAFT.
-- Task can otherwise move when its dependent tuple is valid.
-- Only a Team change that would break an actual Attention reference is new.
-- Keep normal same-Team business edits and unreferenced cases unchanged.
CREATE TRIGGER tasks_attention_source_team_update
BEFORE UPDATE OF team_id ON tasks
WHEN NEW.team_id IS NOT OLD.team_id AND EXISTS (
    SELECT 1 FROM attention_items
    WHERE source_type = 'TASK' AND source_id = OLD.id AND team_id IS NOT NEW.team_id
)
BEGIN SELECT RAISE(ABORT, 'Attention source Team is retained'); END;


