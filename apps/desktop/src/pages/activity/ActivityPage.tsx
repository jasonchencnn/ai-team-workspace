import { useInfiniteQuery, useQuery } from "@tanstack/react-query";
import { useMemo } from "react";
import { Link, useParams, useSearchParams } from "react-router";

import { getTaskBoard, listEvents, listRoles } from "../../api/commands.js";
import { queryKeys } from "../../api/queryKeys.js";
import {
  Button,
  EmptyState,
  ErrorBanner,
  PageHeader,
  Panel,
  SelectField,
  Spinner,
  Tag,
} from "../../components/index.js";
import type { EventQuery } from "../../bindings/EventQuery.js";
import type { TaskSummary } from "../../bindings/TaskSummary.js";
import {
  ACTOR_LABELS,
  EVENT_KINDS,
  KIND_LABELS,
  TIME_RANGES,
  formatEventTime,
  kindsTriggerLabel,
  parseKinds,
  parseRange,
  rangeBounds,
  serializeKinds,
} from "./activityUi.js";
import "./activity.css";

/** 每页条数；与后端默认值一致，页面本身不做分页规则。 */
const PAGE_SIZE = 50;

const RANGE_PARAM = "range";
const TASK_PARAM = "task";
const ROLE_PARAM = "role";
const KIND_PARAM = "kind";

/** 任务下拉的数据来自任务面板接口，避免另开一个接口只为填下拉框。 */
const INCLUDE_CANCELLED = true;

/** 没有关联对象时表格里显示的占位。 */
const DASH = "—";

export function ActivityPage() {
  const { teamId = "" } = useParams();
  const [params, setParams] = useSearchParams();

  const taskId = params.get(TASK_PARAM) ?? "";
  const roleId = params.get(ROLE_PARAM) ?? "";
  const kinds = parseKinds(params.get(KIND_PARAM));
  const range = parseRange(params.get(RANGE_PARAM));

  // 筛选条件全部存在地址栏：刷新、复制链接、从任务详情跳回来都能保持一致。
  const update = (key: string, value: string) => {
    const next = new URLSearchParams(params);
    if (value === "") {
      next.delete(key);
    } else {
      next.set(key, value);
    }
    // 用 replace：调整筛选不应该在历史里堆一串记录，但当前地址栏仍是筛选后的状态。
    setParams(next, { replace: true });
  };
  const clearAll = () => {
    setParams(new URLSearchParams(), { replace: true });
  };
  const hasFilters = taskId !== "" || roleId !== "" || kinds.length > 0 || range !== "all";

  // 零点对齐的时间条件在会话内是常量，据此组成的查询 key 不会每次渲染都变。
  const bounds = useMemo(() => rangeBounds(range, new Date()), [range]);
  // 列表里的「今天 / 昨天」也要一个稳定的参照时间。
  const clock = useMemo(() => new Date(), []);

  const filterQuery: EventQuery = {
    teamId,
    taskId: taskId === "" ? null : taskId,
    roleId: roleId === "" ? null : roleId,
    kinds: kinds.length === 0 ? null : kinds,
    since: bounds?.since ?? null,
    until: bounds?.until ?? null,
  };

  const events = useInfiniteQuery({
    queryKey: queryKeys.events(teamId, filterQuery),
    queryFn: ({ pageParam }) => listEvents({ ...filterQuery, cursor: pageParam, limit: PAGE_SIZE }),
    initialPageParam: null as string | null,
    getNextPageParam: (last) => last.nextCursor,
    enabled: teamId !== "",
  });

  const boardQuery = { teamId, includeCancelled: INCLUDE_CANCELLED };
  const board = useQuery({
    queryKey: queryKeys.taskBoard(teamId, boardQuery),
    queryFn: () => getTaskBoard(boardQuery),
    enabled: teamId !== "",
  });
  const roleViews = useQuery({
    queryKey: queryKeys.roles(teamId),
    queryFn: () => listRoles(teamId),
    enabled: teamId !== "",
  });

  const tasks = useMemo(() => {
    const map = new Map<string, TaskSummary>();
    for (const column of board.data?.columns ?? []) {
      for (const task of column.tasks) {
        map.set(task.id, task);
      }
    }
    return map;
  }, [board.data]);
  const roles = useMemo(() => {
    const map = new Map<string, string>();
    for (const view of roleViews.data ?? []) {
      map.set(view.role.id, view.role.name);
    }
    return map;
  }, [roleViews.data]);
  const taskOptions = useMemo(
    () => [...tasks.values()].sort((a, b) => a.key.localeCompare(b.key, "zh-CN")),
    [tasks],
  );

  const header = (
    <PageHeader
      title="活动与审计"
      description="系统事实与可回放的操作记录，只追加不修改；不保存 Harness 的完整私有对话。"
    />
  );

  const toolbar = (
    <Panel className="act-filters">
      <div className="act-filters__row">
        <SelectField
          label="任务"
          value={taskId}
          onChange={(event) => {
            update(TASK_PARAM, event.target.value);
          }}
        >
          <option value="">全部任务</option>
          {taskOptions.map((task) => (
            <option key={task.id} value={task.id}>
              {task.key} {task.title}
            </option>
          ))}
        </SelectField>
        <SelectField
          label="岗位"
          value={roleId}
          onChange={(event) => {
            update(ROLE_PARAM, event.target.value);
          }}
        >
          <option value="">全部岗位</option>
          {(roleViews.data ?? []).map((view) => (
            <option key={view.role.id} value={view.role.id}>
              {view.role.name}
            </option>
          ))}
        </SelectField>
        <div className="act-segment" role="group" aria-label="时间范围">
          {TIME_RANGES.map((entry) => (
            <Button
              key={entry.value}
              variant={range === entry.value ? "primary" : "neutral"}
              aria-pressed={range === entry.value}
              onClick={() => {
                // 「全部」是默认值，选回它时删掉参数而不是写 range=all，保持地址栏干净。
                update(RANGE_PARAM, entry.value === "all" ? "" : entry.value);
              }}
            >
              {entry.label}
            </Button>
          ))}
        </div>
        {/* 原生 details 做多选：展开与收起不需要脚本，键盘也能用。 */}
        <details className="act-multi">
          <summary className="act-multi__trigger">{kindsTriggerLabel(kinds)}</summary>
          <div className="act-multi__panel" role="group" aria-label="活动种类">
            {EVENT_KINDS.map((kind) => (
              <label key={kind} className="act-multi__option">
                <input
                  type="checkbox"
                  checked={kinds.includes(kind)}
                  onChange={(event) => {
                    const next = event.target.checked
                      ? [...kinds, kind]
                      : kinds.filter((value) => value !== kind);
                    update(KIND_PARAM, serializeKinds(next) ?? "");
                  }}
                />
                <span>{KIND_LABELS[kind]}</span>
              </label>
            ))}
          </div>
        </details>
        {hasFilters ? (
          <Button variant="subtle" onClick={clearAll}>
            清除筛选
          </Button>
        ) : null}
      </div>
      <p className="act-filters__note">
        筛选条件写进地址栏，刷新或返回后保留。时间按本地时区显示；「今天」「近 7 天」「近 30
        天」按本地时间换算。
      </p>
    </Panel>
  );

  if (events.isPending) {
    return (
      <>
        {header}
        {toolbar}
        <div className="act-state">
          <Spinner label="正在加载活动" />
        </div>
      </>
    );
  }
  if (events.isError) {
    return (
      <>
        {header}
        {toolbar}
        <ErrorBanner
          error={events.error}
          action={
            <Button variant="neutral" onClick={() => void events.refetch()}>
              重试
            </Button>
          }
        />
      </>
    );
  }

  const rows = events.data.pages.flatMap((page) => page.events);

  return (
    <>
      {header}
      {toolbar}
      {rows.length === 0 ? (
        <EmptyState
          title="没有符合条件的活动"
          description={
            hasFilters
              ? "换一个任务、岗位、时间范围或活动种类再看看。"
              : "创建任务、调整任务状态或绑定 Harness 之后，这里会留下可回放的操作记录。"
          }
          action={
            hasFilters ? (
              <Button variant="neutral" onClick={clearAll}>
                清除筛选
              </Button>
            ) : undefined
          }
        />
      ) : (
        <>
          <div className="act-table-wrap">
            <table className="act-table">
              <thead>
                <tr>
                  <th scope="col">时间</th>
                  <th scope="col">操作者</th>
                  <th scope="col">摘要</th>
                  <th scope="col">关联任务</th>
                  <th scope="col">关联岗位</th>
                </tr>
              </thead>
              <tbody>
                {rows.map((event) => {
                  const task = event.taskId === null ? undefined : tasks.get(event.taskId);
                  const roleName = event.roleId === null ? undefined : roles.get(event.roleId);
                  return (
                    <tr key={event.id}>
                      <td className="act-table__time">
                        <time dateTime={event.createdAt}>
                          {formatEventTime(event.createdAt, clock)}
                        </time>
                      </td>
                      <td className="act-table__actor">
                        {ACTOR_LABELS[event.actor] ?? event.actor}
                      </td>
                      <td className="act-table__summary">
                        <span className="act-table__text">{event.summary}</span>
                        <Tag
                          tone={
                            event.actor === "system"
                              ? "info"
                              : event.actor === "leader"
                                ? "warning"
                                : "neutral"
                          }
                        >
                          {KIND_LABELS[event.kind]}
                        </Tag>
                      </td>
                      <td>
                        {task === undefined ? (
                          DASH
                        ) : (
                          <Link
                            className="act-table__link"
                            to={`/teams/${teamId}/tasks/${task.id}`}
                          >
                            {task.key}
                          </Link>
                        )}
                      </td>
                      <td>
                        {event.roleId === null || roleName === undefined ? (
                          DASH
                        ) : (
                          <Link
                            className="act-table__link"
                            to={`/teams/${teamId}/roles/${event.roleId}/workbench`}
                          >
                            {roleName}
                          </Link>
                        )}
                      </td>
                    </tr>
                  );
                })}
              </tbody>
            </table>
          </div>
          <div className="act-more">
            <p className="act-note">已显示 {String(rows.length)} 条，按时间从新到旧</p>
            {events.hasNextPage ? (
              <Button
                variant="neutral"
                onClick={() => {
                  void events.fetchNextPage();
                }}
              >
                加载更多
              </Button>
            ) : null}
          </div>
        </>
      )}
    </>
  );
}
