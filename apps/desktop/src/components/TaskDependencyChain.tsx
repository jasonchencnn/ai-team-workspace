import { Link } from "react-router";

import type { TaskStatus } from "../bindings/TaskStatus.js";
import type { TaskSummary } from "../bindings/TaskSummary.js";
import { statusTone } from "../pages/tasks/taskUi.js";
import { Tag } from "./Tag.js";
import "./task-dependency-chain.css";

type ChainTask = Pick<TaskSummary, "id" | "key" | "title" | "status">;

interface TaskDependencyChainProps {
  teamId: string;
  current: ChainTask;
  dependencies: ChainTask[];
  dependents: ChainTask[];
  statusLabel: (status: TaskStatus) => string;
  preview?: boolean;
}

export function TaskDependencyChain({
  teamId,
  current,
  dependencies,
  dependents,
  statusLabel,
  preview = false,
}: TaskDependencyChainProps) {
  const node = (task: ChainTask, isCurrent = false) => (
    <div className={`dependency-chain__node${isCurrent ? " dependency-chain__node--current" : ""}`}>
      {preview || isCurrent ? (
        <span>
          {task.key} {task.title}
        </span>
      ) : (
        <Link to={`/teams/${teamId}/tasks/${task.id}`}>
          <span>{task.key}</span> {task.title}
        </Link>
      )}
      <div className="dependency-chain__status">
        <Tag tone={task.status === "review" ? "warning" : statusTone[task.status]}>
          {statusLabel(task.status)}
        </Tag>
        {isCurrent ? <span>当前任务</span> : null}
      </div>
    </div>
  );
  const arrow = (
    <span className="dependency-chain__arrow" aria-label="指向">
      →
    </span>
  );

  return (
    <section
      className={`dependency-chain${preview ? " dependency-chain--preview" : ""}`}
      aria-label={preview ? "依赖链预览" : "依赖链"}
    >
      <div className="dependency-chain__heading">
        <h2>{preview ? "依赖链预览" : "依赖链"}</h2>
        <span>
          前置 {dependencies.length} 个 · 下游 {dependents.length} 个
        </span>
      </div>
      {preview ? (
        <div className="dependency-chain__rows">
          {dependencies.map((task) => (
            <div className="dependency-chain__row" key={task.id}>
              {node(task)}
              {arrow}
              {node(current, true)}
            </div>
          ))}
          {dependents.map((task) => (
            <div className="dependency-chain__row" key={task.id}>
              {node(current, true)}
              {arrow}
              {node(task)}
              <span className="dependency-chain__empty">下游</span>
            </div>
          ))}
          {dependencies.length === 0 && dependents.length === 0 ? (
            <p className="dependency-chain__empty">暂无依赖关系。</p>
          ) : null}
        </div>
      ) : (
        <>
          <p className="dependency-chain__description">
            左边是当前任务的前置任务，右边是依赖当前任务的直接下游任务。
          </p>
          <div className="dependency-chain__graph">
            <div className="dependency-chain__column" aria-label="前置任务">
              {dependencies.length === 0 ? (
                <span className="dependency-chain__empty">无前置任务</span>
              ) : (
                dependencies.map((task) => <div key={task.id}>{node(task)}</div>)
              )}
            </div>
            {arrow}
            {node(current, true)}
            {arrow}
            <div className="dependency-chain__column" aria-label="直接下游任务">
              {dependents.length === 0 ? (
                <span className="dependency-chain__empty">无下游任务</span>
              ) : (
                dependents.map((task) => <div key={task.id}>{node(task)}</div>)
              )}
            </div>
          </div>
        </>
      )}
    </section>
  );
}
