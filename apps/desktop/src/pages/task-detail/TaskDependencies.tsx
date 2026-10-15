import type { TaskDetail } from "../../bindings/TaskDetail.js";
import type { TaskStatus } from "../../bindings/TaskStatus.js";
import { Card } from "../../components/index.js";
import { TaskDependencyChain } from "../../components/TaskDependencyChain.js";

export function TaskDependencies({
  detail,
  statusLabel,
}: {
  detail: TaskDetail;
  statusLabel: (status: TaskStatus) => string;
}) {
  return (
    <>
      <TaskDependencyChain
        teamId={detail.task.teamId}
        current={detail.task}
        dependencies={detail.dependencies}
        dependents={detail.dependents}
        statusLabel={statusLabel}
      />
      {detail.dependencyNotice === null ? null : (
        <Card title="自动就绪" tone="info">
          <p>{detail.dependencyNotice}</p>
        </Card>
      )}
    </>
  );
}
