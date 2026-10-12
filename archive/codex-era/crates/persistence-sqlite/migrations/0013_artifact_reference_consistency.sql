-- DEV-02-06: Artifact and Reference links must preserve their subject tuples.
CREATE TABLE artifact_reference_consistency_guard (
    id INTEGER PRIMARY KEY CHECK (id = 1),
    valid INTEGER NOT NULL CHECK (valid = 1)
);

INSERT INTO artifact_reference_consistency_guard (id, valid)
SELECT 1, 0
WHERE EXISTS (
    SELECT 1
    FROM artifacts AS artifact
    LEFT JOIN tasks AS task ON task.id = artifact.task_id
    LEFT JOIN runs AS run ON run.id = artifact.run_id
    LEFT JOIN roles AS creator ON creator.id = artifact.creator_role_id
    WHERE artifact.team_id IS NOT task.team_id
       OR artifact.team_id IS NOT run.team_id
       OR artifact.team_id IS NOT creator.team_id
       OR artifact.task_id IS NOT run.task_id
       OR artifact.creator_role_id IS NOT run.role_id
) OR EXISTS (
    SELECT 1
    FROM "references" AS reference
    WHERE (reference.subject_type = 'TASK' AND NOT EXISTS (
        SELECT 1 FROM tasks
        WHERE id = reference.subject_id AND team_id = reference.team_id
    )) OR (reference.subject_type = 'RUN' AND NOT EXISTS (
        SELECT 1 FROM runs
        WHERE id = reference.subject_id AND team_id = reference.team_id
    )) OR (reference.subject_type = 'ARTIFACT' AND NOT EXISTS (
        SELECT 1 FROM artifacts
        WHERE id = reference.subject_id AND team_id = reference.team_id
    )) OR (reference.subject_type = 'MESSAGE' AND NOT EXISTS (
        SELECT 1 FROM messages
        WHERE id = reference.subject_id AND team_id = reference.team_id
    ))
);

DROP TABLE artifact_reference_consistency_guard;

CREATE TRIGGER artifacts_consistency_insert
BEFORE INSERT ON artifacts
FOR EACH ROW
WHEN NOT (
    EXISTS (SELECT 1 FROM tasks WHERE id = NEW.task_id AND team_id = NEW.team_id)
    AND EXISTS (
        SELECT 1 FROM runs
        WHERE id = NEW.run_id
          AND team_id = NEW.team_id
          AND task_id = NEW.task_id
          AND role_id = NEW.creator_role_id
    )
    AND EXISTS (
        SELECT 1 FROM roles
        WHERE id = NEW.creator_role_id AND team_id = NEW.team_id
    )
)
BEGIN
    SELECT RAISE(ABORT, 'Artifact consistency violation');
END;

CREATE TRIGGER artifacts_consistency_update
BEFORE UPDATE ON artifacts
FOR EACH ROW
WHEN NOT (
    EXISTS (SELECT 1 FROM tasks WHERE id = NEW.task_id AND team_id = NEW.team_id)
    AND EXISTS (
        SELECT 1 FROM runs
        WHERE id = NEW.run_id
          AND team_id = NEW.team_id
          AND task_id = NEW.task_id
          AND role_id = NEW.creator_role_id
    )
    AND EXISTS (
        SELECT 1 FROM roles
        WHERE id = NEW.creator_role_id AND team_id = NEW.team_id
    )
)
BEGIN
    SELECT RAISE(ABORT, 'Artifact consistency violation');
END;

CREATE TRIGGER references_consistency_insert
BEFORE INSERT ON "references"
FOR EACH ROW
WHEN (NEW.subject_type = 'TASK' AND NOT EXISTS (
    SELECT 1 FROM tasks WHERE id = NEW.subject_id AND team_id = NEW.team_id
)) OR (NEW.subject_type = 'RUN' AND NOT EXISTS (
    SELECT 1 FROM runs WHERE id = NEW.subject_id AND team_id = NEW.team_id
)) OR (NEW.subject_type = 'ARTIFACT' AND NOT EXISTS (
    SELECT 1 FROM artifacts WHERE id = NEW.subject_id AND team_id = NEW.team_id
)) OR (NEW.subject_type = 'MESSAGE' AND NOT EXISTS (
    SELECT 1 FROM messages WHERE id = NEW.subject_id AND team_id = NEW.team_id
))
BEGIN
    SELECT RAISE(ABORT, 'Reference consistency violation');
END;

CREATE TRIGGER references_consistency_update
BEFORE UPDATE ON "references"
FOR EACH ROW
WHEN (NEW.subject_type = 'TASK' AND NOT EXISTS (
    SELECT 1 FROM tasks WHERE id = NEW.subject_id AND team_id = NEW.team_id
)) OR (NEW.subject_type = 'RUN' AND NOT EXISTS (
    SELECT 1 FROM runs WHERE id = NEW.subject_id AND team_id = NEW.team_id
)) OR (NEW.subject_type = 'ARTIFACT' AND NOT EXISTS (
    SELECT 1 FROM artifacts WHERE id = NEW.subject_id AND team_id = NEW.team_id
)) OR (NEW.subject_type = 'MESSAGE' AND NOT EXISTS (
    SELECT 1 FROM messages WHERE id = NEW.subject_id AND team_id = NEW.team_id
))
BEGIN
    SELECT RAISE(ABORT, 'Reference consistency violation');
END;

CREATE TRIGGER tasks_artifact_reference_consistency_update
BEFORE UPDATE OF team_id ON tasks
FOR EACH ROW
WHEN EXISTS (
    SELECT 1 FROM artifacts
    WHERE task_id = NEW.id AND team_id IS NOT NEW.team_id
) OR EXISTS (
    SELECT 1 FROM "references"
    WHERE subject_type = 'TASK'
      AND subject_id = NEW.id
      AND team_id IS NOT NEW.team_id
)
BEGIN
    SELECT RAISE(ABORT, 'Artifact consistency violation');
END;

CREATE TRIGGER runs_artifact_reference_consistency_update
BEFORE UPDATE OF team_id, task_id, role_id ON runs
FOR EACH ROW
WHEN EXISTS (
    SELECT 1 FROM artifacts
    WHERE run_id = NEW.id
      AND (
          team_id IS NOT NEW.team_id
          OR task_id IS NOT NEW.task_id
          OR creator_role_id IS NOT NEW.role_id
      )
) OR EXISTS (
    SELECT 1 FROM "references"
    WHERE subject_type = 'RUN'
      AND subject_id = NEW.id
      AND team_id IS NOT NEW.team_id
)
BEGIN
    SELECT RAISE(ABORT, 'Artifact consistency violation');
END;

CREATE TRIGGER roles_artifact_consistency_update
BEFORE UPDATE OF team_id ON roles
FOR EACH ROW
WHEN EXISTS (
    SELECT 1 FROM artifacts
    WHERE creator_role_id = NEW.id AND team_id IS NOT NEW.team_id
)
BEGIN
    SELECT RAISE(ABORT, 'Artifact consistency violation');
END;

CREATE TRIGGER artifacts_reference_consistency_update
BEFORE UPDATE OF team_id ON artifacts
FOR EACH ROW
WHEN EXISTS (
    SELECT 1 FROM "references"
    WHERE subject_type = 'ARTIFACT'
      AND subject_id = NEW.id
      AND team_id IS NOT NEW.team_id
)
BEGIN
    SELECT RAISE(ABORT, 'Reference consistency violation');
END;

CREATE TRIGGER messages_reference_consistency_update
BEFORE UPDATE OF team_id ON messages
FOR EACH ROW
WHEN EXISTS (
    SELECT 1 FROM "references"
    WHERE subject_type = 'MESSAGE'
      AND subject_id = NEW.id
      AND team_id IS NOT NEW.team_id
)
BEGIN
    SELECT RAISE(ABORT, 'Reference consistency violation');
END;

CREATE TRIGGER tasks_referenced_subject_id_update
BEFORE UPDATE OF id ON tasks
FOR EACH ROW
WHEN EXISTS (
    SELECT 1 FROM "references"
    WHERE subject_type = 'TASK' AND subject_id = OLD.id
)
BEGIN
    SELECT RAISE(ABORT, 'Reference consistency violation');
END;

CREATE TRIGGER tasks_referenced_subject_delete
BEFORE DELETE ON tasks
FOR EACH ROW
WHEN EXISTS (
    SELECT 1 FROM "references"
    WHERE subject_type = 'TASK' AND subject_id = OLD.id
)
BEGIN
    SELECT RAISE(ABORT, 'Reference consistency violation');
END;

CREATE TRIGGER runs_referenced_subject_id_update
BEFORE UPDATE OF id ON runs
FOR EACH ROW
WHEN EXISTS (
    SELECT 1 FROM "references"
    WHERE subject_type = 'RUN' AND subject_id = OLD.id
)
BEGIN
    SELECT RAISE(ABORT, 'Reference consistency violation');
END;

CREATE TRIGGER runs_referenced_subject_delete
BEFORE DELETE ON runs
FOR EACH ROW
WHEN EXISTS (
    SELECT 1 FROM "references"
    WHERE subject_type = 'RUN' AND subject_id = OLD.id
)
BEGIN
    SELECT RAISE(ABORT, 'Reference consistency violation');
END;

CREATE TRIGGER artifacts_referenced_subject_id_update
BEFORE UPDATE OF id ON artifacts
FOR EACH ROW
WHEN EXISTS (
    SELECT 1 FROM "references"
    WHERE subject_type = 'ARTIFACT' AND subject_id = OLD.id
)
BEGIN
    SELECT RAISE(ABORT, 'Reference consistency violation');
END;

CREATE TRIGGER artifacts_referenced_subject_delete
BEFORE DELETE ON artifacts
FOR EACH ROW
WHEN EXISTS (
    SELECT 1 FROM "references"
    WHERE subject_type = 'ARTIFACT' AND subject_id = OLD.id
)
BEGIN
    SELECT RAISE(ABORT, 'Reference consistency violation');
END;

CREATE TRIGGER messages_referenced_subject_id_update
BEFORE UPDATE OF id ON messages
FOR EACH ROW
WHEN EXISTS (
    SELECT 1 FROM "references"
    WHERE subject_type = 'MESSAGE' AND subject_id = OLD.id
)
BEGIN
    SELECT RAISE(ABORT, 'Reference consistency violation');
END;

CREATE TRIGGER messages_referenced_subject_delete
BEFORE DELETE ON messages
FOR EACH ROW
WHEN EXISTS (
    SELECT 1 FROM "references"
    WHERE subject_type = 'MESSAGE' AND subject_id = OLD.id
)
BEGIN
    SELECT RAISE(ABORT, 'Reference consistency violation');
END;

CREATE TRIGGER tasks_referenced_subject_replace_guard
BEFORE INSERT ON tasks
FOR EACH ROW
WHEN EXISTS (
    SELECT 1
    FROM tasks AS existing
    JOIN "references" AS reference
      ON reference.subject_type = 'TASK'
     AND reference.subject_id = existing.id
    WHERE existing.id = NEW.id
       OR (existing.team_id = NEW.team_id AND existing.display_id = NEW.display_id)
)
BEGIN
    SELECT RAISE(ABORT, 'Reference consistency violation');
END;

CREATE TRIGGER runs_referenced_subject_replace_guard
BEFORE INSERT ON runs
FOR EACH ROW
WHEN EXISTS (
    SELECT 1
    FROM runs AS existing
    JOIN "references" AS reference
      ON reference.subject_type = 'RUN'
     AND reference.subject_id = existing.id
    WHERE existing.id = NEW.id
       OR (existing.task_id = NEW.task_id AND existing.attempt = NEW.attempt)
       OR (existing.task_id = NEW.task_id AND existing.fencing_token = NEW.fencing_token)
       OR (
           NEW.status IN ('QUEUED', 'STARTING', 'RUNNING', 'UNKNOWN')
           AND existing.task_id = NEW.task_id
           AND existing.status IN ('QUEUED', 'STARTING', 'RUNNING', 'UNKNOWN')
       )
)
BEGIN
    SELECT RAISE(ABORT, 'Reference consistency violation');
END;

CREATE TRIGGER artifacts_referenced_subject_replace_guard
BEFORE INSERT ON artifacts
FOR EACH ROW
WHEN EXISTS (
    SELECT 1
    FROM artifacts AS existing
    JOIN "references" AS reference
      ON reference.subject_type = 'ARTIFACT'
     AND reference.subject_id = existing.id
    WHERE existing.id = NEW.id
)
BEGIN
    SELECT RAISE(ABORT, 'Reference consistency violation');
END;

CREATE TRIGGER messages_referenced_subject_replace_guard
BEFORE INSERT ON messages
FOR EACH ROW
WHEN EXISTS (
    SELECT 1
    FROM messages AS existing
    JOIN "references" AS reference
      ON reference.subject_type = 'MESSAGE'
     AND reference.subject_id = existing.id
    WHERE existing.id = NEW.id
       OR (
           NEW.idempotency_key IS NOT NULL
           AND existing.team_id = NEW.team_id
           AND existing.idempotency_key = NEW.idempotency_key
       )
)
BEGIN
    SELECT RAISE(ABORT, 'Reference consistency violation');
END;
