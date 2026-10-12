-- DEV-02-04: preserve Team isolation and acyclicity for the Task graph.
CREATE TABLE task_dependency_consistency_upgrade_guard (
    id INTEGER PRIMARY KEY CHECK (id = 1),
    valid INTEGER NOT NULL CHECK (valid = 1)
);

INSERT INTO task_dependency_consistency_upgrade_guard (id, valid)
SELECT 1, 0
WHERE EXISTS (
    SELECT 1 FROM tasks AS task
    LEFT JOIN roles AS role ON role.id = task.owner_role_id
    WHERE role.team_id IS NOT task.team_id
) OR EXISTS (
    SELECT 1 FROM task_dependencies AS dependency
    LEFT JOIN tasks AS task ON task.id = dependency.task_id
    LEFT JOIN tasks AS prerequisite ON prerequisite.id = dependency.depends_on_task_id
    WHERE task.team_id IS NOT dependency.team_id
       OR prerequisite.team_id IS NOT dependency.team_id
) OR EXISTS (
    WITH RECURSIVE reachable(origin, node) AS (
        SELECT task_id, depends_on_task_id FROM task_dependencies
        UNION
        SELECT reachable.origin, dependency.depends_on_task_id
        FROM reachable
        JOIN task_dependencies AS dependency ON dependency.task_id = reachable.node
    )
    SELECT 1 FROM reachable WHERE origin = node
);

DROP TABLE task_dependency_consistency_upgrade_guard;

CREATE TRIGGER tasks_team_consistency_insert
BEFORE INSERT ON tasks
FOR EACH ROW
WHEN NOT EXISTS (SELECT 1 FROM roles WHERE id = NEW.owner_role_id AND team_id = NEW.team_id)
BEGIN SELECT RAISE(ABORT, 'Task Team consistency violation'); END;

CREATE TRIGGER tasks_team_consistency_update
BEFORE UPDATE ON tasks
FOR EACH ROW
WHEN NOT EXISTS (SELECT 1 FROM roles WHERE id = NEW.owner_role_id AND team_id = NEW.team_id)
 OR EXISTS (
    SELECT 1 FROM task_dependencies AS dependency
    JOIN tasks AS other ON other.id = CASE WHEN dependency.task_id = NEW.id THEN dependency.depends_on_task_id ELSE dependency.task_id END
    WHERE (dependency.task_id = NEW.id OR dependency.depends_on_task_id = NEW.id)
      AND other.team_id != NEW.team_id
 )
BEGIN SELECT RAISE(ABORT, 'Task Team consistency violation'); END;

CREATE TRIGGER roles_task_team_consistency_update
BEFORE UPDATE OF team_id ON roles
FOR EACH ROW
WHEN EXISTS (SELECT 1 FROM tasks WHERE owner_role_id = NEW.id AND team_id != NEW.team_id)
BEGIN SELECT RAISE(ABORT, 'Task Team consistency violation'); END;

CREATE TRIGGER task_dependencies_consistency_insert
BEFORE INSERT ON task_dependencies
FOR EACH ROW
WHEN NOT (
    EXISTS (SELECT 1 FROM tasks WHERE id = NEW.task_id AND team_id = NEW.team_id)
    AND EXISTS (SELECT 1 FROM tasks WHERE id = NEW.depends_on_task_id AND team_id = NEW.team_id)
)
 OR EXISTS (
    WITH RECURSIVE reachable(node) AS (
        SELECT NEW.depends_on_task_id
        UNION
        SELECT dependency.depends_on_task_id FROM task_dependencies AS dependency
        JOIN reachable ON dependency.task_id = reachable.node
    )
    SELECT 1 FROM reachable WHERE node = NEW.task_id
 )
BEGIN SELECT RAISE(ABORT, 'Task dependency consistency violation'); END;

CREATE TRIGGER task_dependencies_consistency_update
BEFORE UPDATE ON task_dependencies
FOR EACH ROW
WHEN NOT (
    EXISTS (SELECT 1 FROM tasks WHERE id = NEW.task_id AND team_id = NEW.team_id)
    AND EXISTS (SELECT 1 FROM tasks WHERE id = NEW.depends_on_task_id AND team_id = NEW.team_id)
)
 OR EXISTS (
    WITH RECURSIVE reachable(node) AS (
        SELECT NEW.depends_on_task_id
        UNION
        SELECT dependency.depends_on_task_id FROM task_dependencies AS dependency
        JOIN reachable ON dependency.task_id = reachable.node
        WHERE dependency.task_id != OLD.task_id
           OR dependency.depends_on_task_id != OLD.depends_on_task_id
    )
    SELECT 1 FROM reachable WHERE node = NEW.task_id
 )
BEGIN SELECT RAISE(ABORT, 'Task dependency consistency violation'); END;
