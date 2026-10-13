import { useQueries, useQuery } from "@tanstack/react-query";
import { useMemo, useState } from "react";
import { Link, useNavigate, useParams } from "react-router";

import { getRoleWorkbench, getTaskBoard, getTeamOverview, listRoles } from "../../api/commands.js";
import { queryKeys } from "../../api/queryKeys.js";
import {
  Button,
  Card,
  EmptyState,
  ErrorBanner,
  PageHeader,
  Spinner,
  Tag,
} from "../../components/index.js";
import type { BoardColumn } from "../../bindings/BoardColumn.js";
import type { RoleView } from "../../bindings/RoleView.js";
import type { Run } from "../../bindings/Run.js";
import type { TaskStatus } from "../../bindings/TaskStatus.js";
import type { TaskSummary } from "../../bindings/TaskSummary.js";
import { runBadge } from "../task-detail/run/runUi.js";
import { formatTime, statusTone } from "./taskUi.js";
import "./tasks.css";

type ViewMode = "board" | "list";
type FirstColumnFilter = "all" | "todo" | "ready";

/** 看板上展示的列：TODO 列合并 todo 与 ready。 */
interface DisplayColumn {
  key: string;
  title: string;
  tone: "todo" | "doing" | "blocked" | "review" | "done" | "cancelled";
  tasks: TaskSummary[];
}

function buildColumns(
  columns: BoardColumn[],
  filter: FirstColumnFilter,
  showDone: boolean,
  showCancelled: boolean,
): DisplayColumn[] {
  const byStatus = (status: TaskStatus) => columns.find((column) => column.status === status);
  const result: DisplayColumn[] = [];

  const todoTasks = [
    ...(filter === "ready" ? [] : (byStatus("todo")?.tasks ?? [])),
    ...(filter === "todo" ? [] : (byStatus("ready")?.tasks ?? [])),
  ].sort((a, b) => b.updatedAt.localeCompare(a.updatedAt));
  result.push({ key: "todo", title: "TODO", tone: "todo", tasks: todoTasks });

  for (const status of ["doing", "blocked", "review"] as const) {
    const column = byStatus(status);
    result.push({
      key: status,
      title: column?.label ?? status,
      tone: status,
      tasks: column?.tasks ?? [],
    });
  }
  if (showDone) {
    const column = byStatus("done");
    result.push({
      key: "done",
      title: column?.label ?? "done",
      tone: "done",
      tasks: column?.tasks ?? [],
    });
  }
  if (showCancelled) {
    const column = byStatus("cancelled");
    result.push({
      key: "cancelled",
      title: column?.label ?? "cancelled",
      tone: "cancelled",
      tasks: column?.tasks ?? [],
    });
  }
  return result;
}

function secondLine(task: TaskSummary): string | null {
  if (task.status === "blocked" && task.blockedReason !== null) {
    return task.blockedReason;
  }
  if (task.openDependencyCount > 0) {
    return `等待 ${String(task.openDependencyCount)} 项依赖完成`;
  }
  return null;
}

function lastLine(task: TaskSummary): string | null {
  if (task.status === "ready") {
    return "已就绪，可派工";
  }
  if (task.status === "todo") {
    if (task.fileRefCount === 0) {
      return "尚未引用文件，暂不能标记为 READY";
    }
    if (task.openDependencyCount > 0) {
      return null; // 第二行已写明在等待依赖，不重复。
    }
    return "可尝试标记为 READY";
  }
  return null;
}

function TaskCard({
  task,
  teamId,
  roleViews,
  badge,
}: {
  task: TaskSummary;
  teamId: string;
  roleViews: RoleView[] | undefined;
  badge: ReturnType<typeof runBadge>;
}) {
  const harness = roleViews?.find((view) => view.role.id === task.owner.id)?.harness ?? null;
  const dependencyOrBinding =
    secondLine(task) ??
    (task.status === "doing" && harness !== null ? `当前绑定 ${harness.name}` : null);
  const last = lastLine(task);
  return (
    <Link
      className="task-card-link"
      to={`/teams/${teamId}/tasks/${task.id}`}
      aria-label={`${task.key} ${task.title}`}
    >
      <Card
        tone={task.status === "blocked" ? "warning" : "default"}
        title={
          <span className="task-card__title">
            <span className="task-card__name">
              {task.key} {task.title}
            </span>
            {badge === null ? null : <Tag tone={badge.tone}>{badge.label}</Tag>}
            {task.status === "todo" || task.status === "ready" ? (
              <Tag tone={statusTone[task.status]}>{task.status === "todo" ? "TODO" : "READY"}</Tag>
            ) : null}
          </span>
        }
      >
        <div className="task-card__lines">
          <p>负责人 {task.owner.name}</p>
          {dependencyOrBinding === null ? null : <p>{dependencyOrBinding}</p>}
          {task.fileRefCount > 0 ? <p>引用 {task.fileRefCount} 个文件</p> : null}
          {last === null ? null : <p className="task-card__last">{last}</p>}
        </div>
      </Card>
    </Link>
  );
}

export function TasksPage() {
  const { teamId = "" } = useParams();
  const navigate = useNavigate();
  const [view, setView] = useState<ViewMode>("board");
  const [filter, setFilter] = useState<FirstColumnFilter>("all");
  const [showDone, setShowDone] = useState(false);
  const [showCancelled, setShowCancelled] = useState(false);

  const query = { teamId, includeCancelled: showCancelled };
  const board = useQuery({
    queryKey: queryKeys.taskBoard(teamId, query),
    queryFn: () => getTaskBoard(query),
    enabled: teamId !== "",
  });
  // 卡片上的「当前绑定」来自岗位卡片；失败时只是不显示该行。
  const roles = useQuery({
    queryKey: queryKeys.roles(teamId),
    queryFn: () => listRoles(teamId),
    enabled: teamId !== "",
  });

  // 看板接口默认不含已取消任务，用概览的状态计数（共享缓存）判断团队是否有已取消任务。
  const overview = useQuery({
    queryKey: queryKeys.teamOverview(teamId),
    queryFn: () => getTeamOverview(teamId),
    enabled: teamId !== "",
  });

  // 卡片上的「执行中 / 等待你确认」来自岗位工作台里的最近执行（同一岗位的执行都带 taskId）。
  // 只有执行中的任务需要，所以只查这些任务所属岗位的工作台。
  const doingOwnerIds = useMemo(() => {
    const ids = new Set<string>();
    for (const column of board.data?.columns ?? []) {
      if (column.status !== "doing") {
        continue;
      }
      for (const task of column.tasks) {
        ids.add(task.owner.id);
      }
    }
    return [...ids];
  }, [board.data]);
  const workbenches = useQueries({
    queries: doingOwnerIds.map((roleId) => ({
      queryKey: [...queryKeys.roles(teamId), "workbench", roleId] as const,
      queryFn: () => getRoleWorkbench(roleId),
      enabled: teamId !== "",
    })),
  });
  const runByTask = useMemo(() => {
    const map = new Map<string, Run>();
    for (const result of workbenches) {
      for (const run of result.data?.recentRuns ?? []) {
        const previous = map.get(run.taskId);
        if (previous === undefined || previous.attempt < run.attempt) {
          map.set(run.taskId, run);
        }
      }
    }
    return map;
  }, [workbenches]);

  const createPath = `/teams/${teamId}/tasks/new`;
  const header = (
    <PageHeader title="任务面板" description="任务分派给岗位，详细要求保存在共享文件。" />
  );

  if (board.isPending) {
    return (
      <>
        {header}
        <div className="tasks-state">
          <Spinner label="正在加载任务" />
        </div>
      </>
    );
  }
  if (board.isError) {
    return (
      <>
        {header}
        <ErrorBanner
          error={board.error}
          action={
            <Button variant="neutral" onClick={() => void board.refetch()}>
              重试
            </Button>
          }
        />
      </>
    );
  }

  const columns = board.data.columns;
  const doneCount = columns.find((column) => column.status === "done")?.tasks.length ?? 0;
  const total = columns.reduce((sum, column) => sum + column.tasks.length, 0);

  const cancelledTotal =
    overview.data?.taskCounts.find((entry) => entry.status === "cancelled")?.count ?? 0;
  const onlyCancelled = total === 0 && !showCancelled && cancelledTotal > 0;

  if (total === 0 && !showCancelled && overview.isPending) {
    return (
      <>
        {header}
        <div className="tasks-state">
          <Spinner label="正在加载任务" />
        </div>
      </>
    );
  }
  if (total === 0 && !showCancelled && !onlyCancelled) {
    return (
      <>
        {header}
        <EmptyState
          title="团队已就绪，创建第一项任务"
          description="将任务分配给岗位，并关联已有需求文件。岗位当前绑定的 Harness 会在满足依赖后领取任务。"
          action={<Button onClick={() => void navigate(createPath)}>创建任务</Button>}
        />
      </>
    );
  }

  const displayColumns = buildColumns(columns, filter, showDone, showCancelled);
  const listTasks = displayColumns.flatMap((column) => column.tasks);

  return (
    <>
      {header}
      <div className="tasks-toolbar">
        <label className="tasks-toolbar__select">
          <span className="tasks-toolbar__label">TODO / READY</span>
          <select
            value={filter}
            onChange={(event) => {
              setFilter(event.target.value as FirstColumnFilter);
            }}
          >
            <option value="all">全部</option>
            <option value="todo">仅 TODO</option>
            <option value="ready">仅 READY</option>
          </select>
        </label>
        <div className="tasks-toolbar__segment" role="group" aria-label="视图">
          <Button
            variant={view === "board" ? "primary" : "neutral"}
            aria-pressed={view === "board"}
            onClick={() => {
              setView("board");
            }}
          >
            看板
          </Button>
          <Button
            variant={view === "list" ? "primary" : "neutral"}
            aria-pressed={view === "list"}
            onClick={() => {
              setView("list");
            }}
          >
            列表
          </Button>
        </div>
        <label className="tasks-toolbar__toggle">
          <input
            type="checkbox"
            checked={showDone}
            onChange={(event) => {
              setShowDone(event.target.checked);
            }}
          />
          显示已完成（{doneCount}）
        </label>
        <label className="tasks-toolbar__toggle">
          <input
            type="checkbox"
            checked={showCancelled}
            onChange={(event) => {
              setShowCancelled(event.target.checked);
            }}
          />
          显示已取消
        </label>
        <Button onClick={() => void navigate(createPath)}>创建任务</Button>
      </div>

      {onlyCancelled ? (
        <p className="tasks-state">当前只有已取消的任务，勾选「显示已取消」查看。</p>
      ) : view === "board" ? (
        <div
          className="task-board"
          style={{
            gridTemplateColumns: `repeat(${String(displayColumns.length)}, minmax(220px, 1fr))`,
          }}
          // 列数多时在看板区域内横向滚动，让键盘用户也能滚动。
          tabIndex={0}
          aria-label="任务看板"
          role="region"
        >
          {displayColumns.map((column) => (
            <section key={column.key} className="task-board__column">
              <h2 className={`task-board__heading task-board__heading--${column.tone}`}>
                {column.title} {column.tasks.length}
              </h2>
              {column.tasks.map((task) => (
                <TaskCard
                  key={task.id}
                  task={task}
                  teamId={teamId}
                  roleViews={roles.data}
                  badge={runBadge(runByTask.get(task.id))}
                />
              ))}
            </section>
          ))}
        </div>
      ) : listTasks.length === 0 ? (
        <p className="tasks-state">当前筛选下没有任务。</p>
      ) : (
        <div className="task-table-wrap">
          <table className="task-table">
            <thead>
              <tr>
                <th>KEY</th>
                <th>标题</th>
                <th>负责人</th>
                <th>状态</th>
                <th>更新时间</th>
              </tr>
            </thead>
            <tbody>
              {listTasks.map((task) => {
                const label =
                  columns.find((column) => column.status === task.status)?.label ?? task.status;
                return (
                  <tr
                    key={task.id}
                    onClick={() => void navigate(`/teams/${teamId}/tasks/${task.id}`)}
                  >
                    <td>
                      <Link
                        className="task-table__link"
                        to={`/teams/${teamId}/tasks/${task.id}`}
                        onClick={(event) => {
                          event.stopPropagation();
                        }}
                      >
                        {task.key}
                      </Link>
                    </td>
                    <td className="task-table__title" title={task.title}>
                      {task.title}
                    </td>
                    <td>{task.owner.name}</td>
                    <td>
                      <Tag tone={statusTone[task.status]}>{label}</Tag>
                    </td>
                    <td>{formatTime(task.updatedAt)}</td>
                  </tr>
                );
              })}
            </tbody>
          </table>
        </div>
      )}
    </>
  );
}
