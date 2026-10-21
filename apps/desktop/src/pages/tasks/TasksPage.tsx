import { useQueries, useQuery } from "@tanstack/react-query";
import { useEffect, useMemo, useState, type CSSProperties } from "react";
import { Link, useNavigate, useParams, useSearchParams } from "react-router";

import {
  getRoleWorkbench,
  getTaskBoard,
  getTeamOverview,
  listBoardColumn,
  listRoles,
} from "../../api/commands.js";
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
import type { RoleView } from "../../bindings/RoleView.js";
import type { Run } from "../../bindings/Run.js";
import type { TaskStatus } from "../../bindings/TaskStatus.js";
import type { TaskSummary } from "../../bindings/TaskSummary.js";
import { runBadge } from "../task-detail/run/runUi.js";
import { columnHasHiddenTasks, mergeBoardTasks } from "./boardPaging.js";
import type { FirstColumnFilter, TaskPanelState } from "./taskUi.js";
import {
  formatTime,
  parseTaskPanel,
  serializeTaskPanel,
  statusTone,
  taskPanelSearch,
} from "./taskUi.js";
import "./tasks.css";

/**
 * 看板每列一次渲染的任务数。
 *
 * 任务多到上万时一次渲染上千张卡片会拖慢页面，所以每列先取这一页，剩下的用「加载更多」
 * 按列往后取（内核接口 `list_board_column`，游标分页）。
 */
const COLUMN_PAGE = 200;

/** 看板上展示的列：TODO 列合并 todo 与 ready。 */
interface DisplayColumn {
  key: string;
  title: string;
  tone: "todo" | "doing" | "blocked" | "review" | "done" | "cancelled";
  tasks: TaskSummary[];
  /** 这个展示列由哪些后端状态组成（TODO 列由 todo 与 ready 合并）。 */
  statuses: TaskStatus[];
  /** 展示列的任务总数（含还没加载的部分）。 */
  total: number;
  /** 已经取回的任务数（可能比显示的多，还没展开）。 */
  pool: number;
  /** 还有更多任务没取回。 */
  hasMore: boolean;
}

/** 某个后端状态列在页面上的状态。 */
interface StatusColumn {
  label: string;
  tasks: TaskSummary[];
  total: number;
  /** 已经取回的任务数（第一页 + 加载回来的）。 */
  pool: number;
  hasMore: boolean;
}

/** 某个状态列已加载的后续页。 */
interface ColumnPage {
  /** 已经加载回来的任务（不含看板第一页）。 */
  tasks: TaskSummary[];
  /** 下一页游标；`undefined` 表示没有更多了。 */
  cursor: string | undefined;
}

function buildColumns(
  byStatus: Map<TaskStatus, StatusColumn>,
  filter: FirstColumnFilter,
  showDone: boolean,
  showCancelled: boolean,
  visible: Record<string, number>,
): DisplayColumn[] {
  const result: DisplayColumn[] = [];

  const todoStatuses: TaskStatus[] =
    filter === "ready" ? ["ready"] : filter === "todo" ? ["todo"] : ["todo", "ready"];
  const todoTasks = todoStatuses
    .flatMap((status) => byStatus.get(status)?.tasks ?? [])
    .sort((a, b) => b.updatedAt.localeCompare(a.updatedAt));
  result.push({
    key: "todo",
    title: "TODO",
    tone: "todo",
    tasks: todoTasks.slice(0, visible["todo"] ?? COLUMN_PAGE),
    statuses: todoStatuses,
    total: todoStatuses.reduce((sum, status) => sum + (byStatus.get(status)?.total ?? 0), 0),
    pool: todoStatuses.reduce((sum, status) => sum + (byStatus.get(status)?.pool ?? 0), 0),
    hasMore: todoStatuses.some((status) => byStatus.get(status)?.hasMore ?? false),
  });

  for (const status of ["doing", "blocked", "review"] as const) {
    const column = byStatus.get(status);
    result.push({
      key: status,
      title: column?.label ?? status,
      tone: status,
      tasks: (column?.tasks ?? []).slice(0, visible[status] ?? COLUMN_PAGE),
      statuses: [status],
      total: column?.total ?? 0,
      pool: column?.pool ?? 0,
      hasMore: column?.hasMore ?? false,
    });
  }
  if (showDone) {
    const column = byStatus.get("done");
    result.push({
      key: "done",
      title: column?.label ?? "done",
      tone: "done",
      tasks: (column?.tasks ?? []).slice(0, visible["done"] ?? COLUMN_PAGE),
      statuses: ["done"],
      total: column?.total ?? 0,
      pool: column?.pool ?? 0,
      hasMore: column?.hasMore ?? false,
    });
  }
  if (showCancelled) {
    const column = byStatus.get("cancelled");
    result.push({
      key: "cancelled",
      title: column?.label ?? "cancelled",
      tone: "cancelled",
      tasks: (column?.tasks ?? []).slice(0, visible["cancelled"] ?? COLUMN_PAGE),
      statuses: ["cancelled"],
      total: column?.total ?? 0,
      pool: column?.pool ?? 0,
      hasMore: column?.hasMore ?? false,
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
  search,
  roleViews,
  badge,
}: {
  task: TaskSummary;
  teamId: string;
  /** 带上当前地址栏参数，返回任务面板时还是这个视图和筛选。 */
  search: string;
  roleViews: RoleView[] | undefined;
  badge: ReturnType<typeof runBadge>;
}) {
  const harness = roleViews?.find((view) => view.role.id === task.owner.id)?.harness ?? null;
  const dependencyOrBinding =
    secondLine(task) ??
    (task.status === "doing" && harness !== null ? `当前绑定 ${harness.name}` : null);
  const last = lastLine(task);
  const card = (
    <Link
      className="task-card-link"
      to={`/teams/${teamId}/tasks/${task.id}${search}`}
      aria-label={`${task.key} ${task.title}`}
    >
      <Card
        tone={task.status === "blocked" ? "warning" : "default"}
        title={
          <span className="task-card__title">
            {badge === null &&
            task.status !== "todo" &&
            task.status !== "ready" &&
            !task.changeIncomplete ? null : (
              <span className="task-card__badges">
                {badge === null ? null : <Tag tone={badge.tone}>{badge.label}</Tag>}
                {task.status === "todo" || task.status === "ready" ? (
                  <Tag tone={statusTone[task.status]}>
                    {task.status === "todo" ? "TODO" : "READY"}
                  </Tag>
                ) : null}
                {task.changeIncomplete ? <Tag tone="warning">改动统计不完整</Tag> : null}
              </span>
            )}
            <span className="task-card__name">
              {task.key} {task.title}
            </span>
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
  if (task.status !== "review") {
    return card;
  }
  return (
    <div className="task-card-wrap">
      {card}
      <Link className="task-card__review" to={`/teams/${teamId}/tasks/${task.id}/review${search}`}>
        去验收
      </Link>
    </div>
  );
}

export function TasksPage() {
  const { teamId = "" } = useParams();
  const navigate = useNavigate();
  const [params, setParams] = useSearchParams();
  // 视图与筛选全部存在地址栏（与活动与审计页同一做法）：刷新、复制链接、从任务详情返回
  // 都保持一致，面板自己不做记忆，也不需要额外状态。
  const panel = parseTaskPanel(params);
  const { view, filter, showDone, showCancelled } = panel;
  const update = (patch: Partial<TaskPanelState>) => {
    // 用 replace：调整筛选不应该在历史里堆一串记录，但当前地址栏仍是筛选后的状态。
    setParams(serializeTaskPanel({ ...panel, ...patch }), { replace: true });
  };
  // 进任务详情时把面板参数一起带上，返回时才能恢复；别的页面的参数不带过去。
  const detailSearch = taskPanelSearch(params.toString());

  // 看板每列先取一页（COLUMN_PAGE 条），更多用「加载更多」按列往后取。
  const query = { teamId, includeCancelled: showCancelled, limit: COLUMN_PAGE };
  const board = useQuery({
    queryKey: queryKeys.taskBoard(teamId, query),
    queryFn: () => getTaskBoard(query),
    enabled: teamId !== "",
  });

  // 「加载更多」取回来的任务与下一页游标，以及每个展示列已经显示多少条。
  const [extraByStatus, setExtraByStatus] = useState<Record<string, ColumnPage>>({});
  const [visibleByColumn, setVisibleByColumn] = useState<Record<string, number>>({});
  const [loadingMore, setLoadingMore] = useState(false);
  // 换团队或换「显示已取消」后看板数据变了，已加载的后续页不再对应当前看板。
  useEffect(() => {
    setExtraByStatus({});
    setVisibleByColumn({});
  }, [teamId, showCancelled]);
  // 看板每列自带下一页游标：「加载更多」从那里接着往后取。
  // 已经加载过后续页的列不再退回第一页的游标，否则会取到重复的任务。
  useEffect(() => {
    setExtraByStatus((previous) => {
      const next = { ...previous };
      for (const column of board.data?.columns ?? []) {
        if (next[column.status] === undefined) {
          next[column.status] = { tasks: [], cursor: column.nextCursor ?? undefined };
        }
      }
      return next;
    });
  }, [board.data]);

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

  // 每个后端状态列在页面上的状态：第一页 + 已加载的后续页。
  const byStatus = useMemo(() => {
    const map = new Map<TaskStatus, StatusColumn>();
    for (const column of board.data?.columns ?? []) {
      const extra = extraByStatus[column.status];
      // 第一页与后续页可能有重叠（看板刷新后排序基准变了），按 id 去重，第一页优先。
      const tasks = mergeBoardTasks(column.tasks, extra?.tasks ?? []);
      map.set(column.status, {
        label: column.label,
        tasks,
        total: column.total,
        pool: tasks.length,
        // 还没加载过时听内核的；加载过之后看游标还在不在。
        hasMore: extra === undefined ? column.hasMore : extra.cursor !== undefined,
      });
    }
    return map;
  }, [board.data, extraByStatus]);

  /**
   * 取某个展示列的后续页，并把这一列的可见条数加一页。
   *
   * 取回后如果已经全部取完，就只是把已取回但还没显示的任务展开出来。
   */
  const loadMore = async (column: DisplayColumn) => {
    const targets = column.statuses.filter((status) => byStatus.get(status)?.hasMore ?? false);
    setLoadingMore(true);
    try {
      if (targets.length > 0) {
        const pages = await Promise.all(
          targets.map((status) =>
            listBoardColumn({
              teamId,
              status,
              limit: COLUMN_PAGE,
              cursor: cursorOf(status),
            }),
          ),
        );
        setExtraByStatus((previous) => {
          const next = { ...previous };
          pages.forEach((page, index) => {
            const status = targets[index];
            if (status === undefined) {
              return;
            }
            const entry = next[status] ?? { tasks: [], cursor: undefined };
            next[status] = {
              tasks: [...entry.tasks, ...page.tasks],
              cursor: page.nextCursor ?? undefined,
            };
          });
          return next;
        });
      }
      setVisibleByColumn((previous) => {
        const next = { ...previous };
        next[column.key] = (next[column.key] ?? COLUMN_PAGE) + COLUMN_PAGE;
        return next;
      });
    } finally {
      setLoadingMore(false);
    }
  };

  /** 某个状态列的下一页游标：已加载过的用加载后的游标，没加载过的用看板带来的。 */
  const cursorOf = (status: TaskStatus): string | null => {
    const entry = extraByStatus[status];
    if (entry !== undefined) {
      return entry.cursor ?? null;
    }
    const column = board.data?.columns.find((item) => item.status === status);
    return column?.nextCursor ?? null;
  };

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
  // 「显示已完成」后面是这一列的真实总数（分页后也准），不是当前这一页的条数。
  const doneCount = byStatus.get("done")?.total ?? 0;
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

  const displayColumns = buildColumns(byStatus, filter, showDone, showCancelled, visibleByColumn);
  const listTasks = displayColumns.flatMap((column) => column.tasks);
  const moreColumns = displayColumns.filter((column) =>
    columnHasHiddenTasks(column.hasMore, column.pool, column.tasks.length),
  );

  return (
    <>
      {header}
      <div className="tasks-toolbar">
        <label className="tasks-toolbar__select">
          <span className="tasks-toolbar__label">TODO / READY</span>
          <select
            value={filter}
            onChange={(event) => {
              update({ filter: event.target.value as FirstColumnFilter });
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
              update({ view: "board" });
            }}
          >
            看板
          </Button>
          <Button
            variant={view === "list" ? "primary" : "neutral"}
            aria-pressed={view === "list"}
            onClick={() => {
              update({ view: "list" });
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
              update({ showDone: event.target.checked });
            }}
          />
          显示已完成（{doneCount}）
        </label>
        <label className="tasks-toolbar__toggle">
          <input
            type="checkbox"
            checked={showCancelled}
            onChange={(event) => {
              update({ showCancelled: event.target.checked });
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
          style={
            {
              "--board-column-count": String(displayColumns.length),
            } as CSSProperties
          }
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
                  search={detailSearch}
                  roleViews={roles.data}
                  badge={runBadge(runByTask.get(task.id))}
                />
              ))}
              {columnHasHiddenTasks(column.hasMore, column.pool, column.tasks.length) ? (
                <div className="task-board__more">
                  {/* 后端翻完了但本地还有没展开的卡片时也要能点开：
                      「加载更多」在这种情况下只把已取回的卡片展开出来。 */}
                  <Button
                    variant="neutral"
                    disabled={loadingMore}
                    onClick={() => {
                      void loadMore(column);
                    }}
                  >
                    加载更多
                  </Button>
                  <span className="task-board__more-note">
                    已显示 {column.tasks.length} / {column.total}
                  </span>
                </div>
              ) : null}
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
                    onClick={() =>
                      void navigate(`/teams/${teamId}/tasks/${task.id}${detailSearch}`)
                    }
                  >
                    <td>
                      <Link
                        className="task-table__link"
                        to={`/teams/${teamId}/tasks/${task.id}${detailSearch}`}
                        onClick={(event) => {
                          event.stopPropagation();
                        }}
                      >
                        {task.key}
                      </Link>
                    </td>
                    <td className="task-table__title" title={task.title}>
                      {/* 标题也做成链接：不只靠整行点击，键盘也能直接打开任务。 */}
                      <Link
                        className="task-table__title-link"
                        to={`/teams/${teamId}/tasks/${task.id}${detailSearch}`}
                        onClick={(event) => {
                          event.stopPropagation();
                        }}
                      >
                        {task.title}
                      </Link>
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
          {moreColumns.length === 0 ? null : (
            <div className="task-table__more">
              <Button
                variant="neutral"
                disabled={loadingMore}
                onClick={() => {
                  void Promise.all(moreColumns.map((column) => loadMore(column)));
                }}
              >
                加载更多
              </Button>
              <span className="task-board__more-note">
                已显示 {listTasks.length} 条，共{" "}
                {displayColumns.reduce((sum, column) => sum + column.total, 0)} 条
              </span>
            </div>
          )}
        </div>
      )}
    </>
  );
}
