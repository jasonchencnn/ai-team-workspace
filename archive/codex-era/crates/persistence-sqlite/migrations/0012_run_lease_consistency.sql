-- DEV-02-05: keep Run, Lease, and Review relationships internally consistent.
-- The guard rejects an upgrade atomically when historical rows cannot satisfy
-- the same tuple invariants enforced by the forward-write triggers below.
CREATE TABLE run_lease_consistency_guard (
    id INTEGER PRIMARY KEY CHECK (id = 1),
    valid INTEGER NOT NULL CHECK (valid = 1)
);

INSERT INTO run_lease_consistency_guard (id, valid)
SELECT 1, 0
WHERE EXISTS (
    SELECT 1
    FROM runs AS run
    LEFT JOIN tasks AS task ON task.id = run.task_id
    LEFT JOIN roles AS role ON role.id = run.role_id
    LEFT JOIN bindings AS binding ON binding.id = run.binding_id
    LEFT JOIN harness_instances AS harness ON harness.id = run.harness_instance_id
    WHERE task.team_id IS NOT run.team_id
       OR (
           run.status IN ('QUEUED', 'STARTING', 'RUNNING', 'UNKNOWN')
           AND task.owner_role_id IS NOT run.role_id
       )
       OR role.team_id IS NOT run.team_id
       OR binding.team_id IS NOT run.team_id
       OR binding.role_id IS NOT run.role_id
       OR binding.harness_instance_id IS NOT run.harness_instance_id
       OR harness.team_id IS NOT run.team_id
) OR EXISTS (
    SELECT 1
    FROM runs
    GROUP BY task_id
    HAVING MIN(attempt) != 1 OR MAX(attempt) != COUNT(*)
) OR EXISTS (
    SELECT 1
    FROM runs AS later
    JOIN runs AS earlier
      ON earlier.task_id = later.task_id
     AND earlier.attempt < later.attempt
    WHERE later.fencing_token <= earlier.fencing_token
) OR EXISTS (
    SELECT 1
    FROM runs AS active
    WHERE active.status IN ('QUEUED', 'STARTING', 'RUNNING', 'UNKNOWN')
      AND EXISTS (
          SELECT 1
          FROM runs AS later
          WHERE later.task_id = active.task_id
            AND later.attempt > active.attempt
      )
) OR EXISTS (
    SELECT 1
    FROM leases AS lease
    LEFT JOIN runs AS run ON run.id = lease.run_id
    WHERE lease.task_id IS NOT run.task_id
       OR lease.fencing_token IS NOT run.fencing_token
       OR run.status NOT IN ('QUEUED', 'STARTING', 'RUNNING', 'UNKNOWN')
) OR EXISTS (
    SELECT 1
    FROM reviews AS review
    LEFT JOIN tasks AS task ON task.id = review.task_id
    LEFT JOIN runs AS run ON run.id = review.run_id
    LEFT JOIN roles AS reviewer ON reviewer.id = review.reviewer_role_id
    WHERE review.team_id IS NOT task.team_id
       OR review.team_id IS NOT run.team_id
       OR review.team_id IS NOT reviewer.team_id
       OR review.task_id IS NOT run.task_id
       OR run.status IS NOT 'SUCCEEDED'
);

DROP TABLE run_lease_consistency_guard;

CREATE TRIGGER runs_consistency_insert
BEFORE INSERT ON runs
FOR EACH ROW
WHEN NOT (
    EXISTS (
        SELECT 1 FROM tasks
        WHERE id = NEW.task_id
          AND team_id = NEW.team_id
          AND (
              NEW.status NOT IN ('QUEUED', 'STARTING', 'RUNNING', 'UNKNOWN')
              OR owner_role_id = NEW.role_id
          )
    )
    AND EXISTS (SELECT 1 FROM roles WHERE id = NEW.role_id AND team_id = NEW.team_id)
    AND EXISTS (
        SELECT 1 FROM bindings
        WHERE id = NEW.binding_id
          AND team_id = NEW.team_id
          AND role_id = NEW.role_id
          AND harness_instance_id = NEW.harness_instance_id
    )
    AND EXISTS (
        SELECT 1 FROM harness_instances
        WHERE id = NEW.harness_instance_id AND team_id = NEW.team_id
    )
    AND NEW.attempt = COALESCE(
        (SELECT MAX(attempt) FROM runs WHERE task_id = NEW.task_id),
        0
    ) + 1
    AND NEW.fencing_token > COALESCE(
        (SELECT MAX(fencing_token) FROM runs WHERE task_id = NEW.task_id),
        0
    )
)
BEGIN
    SELECT RAISE(ABORT, 'Run consistency violation');
END;

CREATE TRIGGER runs_consistency_update
BEFORE UPDATE ON runs
FOR EACH ROW
WHEN NOT (
    EXISTS (
        SELECT 1 FROM tasks
        WHERE id = NEW.task_id
          AND team_id = NEW.team_id
          AND (
              NEW.status NOT IN ('QUEUED', 'STARTING', 'RUNNING', 'UNKNOWN')
              OR owner_role_id = NEW.role_id
          )
    )
    AND EXISTS (SELECT 1 FROM roles WHERE id = NEW.role_id AND team_id = NEW.team_id)
    AND EXISTS (
        SELECT 1 FROM bindings
        WHERE id = NEW.binding_id
          AND team_id = NEW.team_id
          AND role_id = NEW.role_id
          AND harness_instance_id = NEW.harness_instance_id
    )
    AND EXISTS (
        SELECT 1 FROM harness_instances
        WHERE id = NEW.harness_instance_id AND team_id = NEW.team_id
    )
    AND NEW.attempt = OLD.attempt
    AND NEW.fencing_token = OLD.fencing_token
    AND NEW.team_id IS OLD.team_id
    AND NEW.task_id IS OLD.task_id
    AND NEW.role_id IS OLD.role_id
    AND NEW.binding_id IS OLD.binding_id
    AND NEW.harness_instance_id IS OLD.harness_instance_id
    AND NOT (
        OLD.status IN ('SUCCEEDED', 'FAILED', 'TIMED_OUT', 'CANCELLED')
        AND NEW.status IN ('QUEUED', 'STARTING', 'RUNNING', 'UNKNOWN')
    )
    AND (
        NEW.status NOT IN ('QUEUED', 'STARTING', 'RUNNING', 'UNKNOWN')
        OR NOT EXISTS (
            SELECT 1
            FROM runs AS later
            WHERE later.task_id = NEW.task_id
              AND later.attempt > NEW.attempt
        )
    )
    AND (
        NEW.status NOT IN ('SUCCEEDED', 'FAILED', 'TIMED_OUT', 'CANCELLED')
        OR NOT EXISTS (SELECT 1 FROM leases WHERE run_id = NEW.id)
    )
)
BEGIN
    SELECT RAISE(ABORT, 'Run consistency violation');
END;

CREATE TRIGGER leases_consistency_insert
BEFORE INSERT ON leases
FOR EACH ROW
WHEN NOT EXISTS (
    SELECT 1 FROM runs
    WHERE id = NEW.run_id
      AND task_id = NEW.task_id
      AND fencing_token = NEW.fencing_token
      AND status IN ('QUEUED', 'STARTING', 'RUNNING', 'UNKNOWN')
)
BEGIN
    SELECT RAISE(ABORT, 'Lease consistency violation');
END;

CREATE TRIGGER leases_consistency_update
BEFORE UPDATE ON leases
FOR EACH ROW
WHEN NOT EXISTS (
    SELECT 1 FROM runs
    WHERE id = NEW.run_id
      AND task_id = NEW.task_id
      AND fencing_token = NEW.fencing_token
      AND status IN ('QUEUED', 'STARTING', 'RUNNING', 'UNKNOWN')
)
BEGIN
    SELECT RAISE(ABORT, 'Lease consistency violation');
END;

CREATE TRIGGER reviews_consistency_insert
BEFORE INSERT ON reviews
FOR EACH ROW
WHEN NOT (
    EXISTS (SELECT 1 FROM tasks WHERE id = NEW.task_id AND team_id = NEW.team_id)
    AND EXISTS (
        SELECT 1 FROM runs
        WHERE id = NEW.run_id
          AND team_id = NEW.team_id
          AND task_id = NEW.task_id
          AND status = 'SUCCEEDED'
    )
    AND EXISTS (
        SELECT 1 FROM roles
        WHERE id = NEW.reviewer_role_id AND team_id = NEW.team_id
    )
)
BEGIN
    SELECT RAISE(ABORT, 'Review consistency violation');
END;

CREATE TRIGGER reviews_consistency_update
BEFORE UPDATE ON reviews
FOR EACH ROW
WHEN NOT (
    EXISTS (SELECT 1 FROM tasks WHERE id = NEW.task_id AND team_id = NEW.team_id)
    AND EXISTS (
        SELECT 1 FROM runs
        WHERE id = NEW.run_id
          AND team_id = NEW.team_id
          AND task_id = NEW.task_id
          AND status = 'SUCCEEDED'
    )
    AND EXISTS (
        SELECT 1 FROM roles
        WHERE id = NEW.reviewer_role_id AND team_id = NEW.team_id
    )
)
BEGIN
    SELECT RAISE(ABORT, 'Review consistency violation');
END;

CREATE TRIGGER tasks_run_consistency_update
BEFORE UPDATE OF team_id, owner_role_id ON tasks
FOR EACH ROW
WHEN EXISTS (
    SELECT 1
    FROM runs
    WHERE task_id = NEW.id
      AND (
          team_id IS NOT NEW.team_id
          OR (
              status IN ('QUEUED', 'STARTING', 'RUNNING', 'UNKNOWN')
              AND role_id IS NOT NEW.owner_role_id
          )
      )
) OR EXISTS (
    SELECT 1
    FROM reviews
    WHERE task_id = NEW.id AND team_id IS NOT NEW.team_id
)
BEGIN
    SELECT RAISE(ABORT, 'Run consistency violation');
END;

CREATE TRIGGER bindings_run_consistency_update
BEFORE UPDATE OF team_id, role_id, harness_instance_id ON bindings
FOR EACH ROW
WHEN EXISTS (
    SELECT 1
    FROM runs
    WHERE binding_id = NEW.id
      AND (
          team_id IS NOT NEW.team_id
          OR role_id IS NOT NEW.role_id
          OR harness_instance_id IS NOT NEW.harness_instance_id
      )
)
BEGIN
    SELECT RAISE(ABORT, 'Run consistency violation');
END;

CREATE TRIGGER roles_run_review_consistency_update
BEFORE UPDATE OF team_id ON roles
FOR EACH ROW
WHEN EXISTS (
    SELECT 1 FROM runs WHERE role_id = NEW.id AND team_id IS NOT NEW.team_id
) OR EXISTS (
    SELECT 1 FROM reviews WHERE reviewer_role_id = NEW.id AND team_id IS NOT NEW.team_id
)
BEGIN
    SELECT RAISE(ABORT, 'Run consistency violation');
END;

CREATE TRIGGER harness_instances_run_consistency_update
BEFORE UPDATE OF team_id ON harness_instances
FOR EACH ROW
WHEN EXISTS (
    SELECT 1 FROM runs
    WHERE harness_instance_id = NEW.id AND team_id IS NOT NEW.team_id
)
BEGIN
    SELECT RAISE(ABORT, 'Run consistency violation');
END;

CREATE TRIGGER runs_lease_review_consistency_update
BEFORE UPDATE OF team_id, task_id, fencing_token, status ON runs
FOR EACH ROW
WHEN EXISTS (
    SELECT 1
    FROM leases
    WHERE run_id = NEW.id
      AND (task_id IS NOT NEW.task_id OR fencing_token IS NOT NEW.fencing_token)
) OR EXISTS (
    SELECT 1
    FROM reviews
    WHERE run_id = NEW.id
      AND (
          team_id IS NOT NEW.team_id
          OR task_id IS NOT NEW.task_id
          OR NEW.status IS NOT 'SUCCEEDED'
      )
)
BEGIN
    SELECT RAISE(ABORT, 'Run consistency violation');
END;
