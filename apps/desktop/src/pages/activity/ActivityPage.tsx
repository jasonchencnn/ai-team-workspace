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
  SelectField,
  Spinner,
  Tag,
} from "../../components/index.js";
import type { EventKind } from "../../bindings/EventKind.js";
import type { EventQuery } from "../../bindings/EventQuery.js";
import type { TaskSummary } from "../../bindings/TaskSummary.js";
import { formatTime } from "../tasks/taskUi.js";
import {
  ACTOR_LABELS,
  EVENT_KINDS,
  KIND_LABELS,
  TIME_RANGES,
  parseRange,
  rangeBounds,
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

function EventRow({
  event,
  teamId,
  tasks,
  roles,
}: {
  event: {
    id: string;
    kind: EventKind;
    actor: string;
    summary: string;
    createdAt: string;
    taskId: string | null;
    roleId: string | null;
  };
  teamId: string;
  tasks: Map<string, TaskSummary>;
  roles: Map<string, string>;
}) {
  const task = event.taskId === null ? undefined : tasks.get(event.taskId);
  const roleName = event.roleId === null ? undefined : roles.get(event.roleId);
  return (
    <li className="act-event">
      <div className="act-event__head">
        <time className="act-event__time" dateTime={event.createdAt}>
          {formatTime(event.createdAt)}
        </time>
        <Tag tone={event.actor === "system" ? "info" : "neutral"}>{KIND_LABELS[event.kind]}</Tag>
      </div>
      <p className="act-event__actor">操作者：{ACTOR_LABELS[event.actor] ?? event.actor}</p>
      <p className="act-event__summary">{event.summary}</p>
      {task === undefined && roleName === undefined ? null : (
        <p className="act-event__links">
          {task === undefined ? null : (
            <Link className="act-event__link" to={`/teams/${teamId}/tasks/${task.id}`}>
              关联任务 {task.key} {task.title}
            </Link>
          )}
          {task === undefined || roleName === undefined ? null : <span aria-hidden="true">·</span>}
          {roleName === undefined ? null : (
            <Link
              className="act-event__link"
              to={`/teams/${teamId}/roles/${event.roleId ?? ""}/workbench`}
            >
              关联岗位 {roleName}
            </Link>
          )}
        </p>
      )}
    </li>
  );
}

export function ActivityPage() {
  const { teamId = "" } = useParams();
  const [params, setParams] = useSearchParams();

  const taskId = params.get(TASK_PARAM) ?? "";
  const roleId = params.get(ROLE_PARAM) ?? "";
  const kindParam = params.get(KIND_PARAM) ?? "";
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

  const kinds = EVENT_KINDS.includes(kindParam as EventKind) ? [kindParam as EventKind] : undefined;
  // 零点对齐的时间条件在会话内是常量，据此组成的查询 key 不会每次渲染都变。
  const bounds = useMemo(() => rangeBounds(range, new Date()), [range]);

  const filterQuery: EventQuery = {
    teamId,
    taskId: taskId === "" ? null : taskId,
    roleId: roleId === "" ? null : roleId,
    kinds: kinds ?? null,
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

  // 任务或岗位下拉读不到时只影响名称显示，不影响列表本身。
  const taskOptions = useMemo(
    () => [...tasks.values()].sort((a, b) => a.key.localeCompare(b.key, "zh-CN")),
    [tasks],
  );
  const filtered = events.data?.pages.flatMap((page) => page.events) ?? [];
  const hasFilters = taskId !== "" || roleId !== "" || kindParam !== "" || range !== "all";

  const header = (
    <PageHeader
      title="活动与审计"
      description="系统事实与可回放的操作记录；不保存 Harness 完整私有对话。"
    />
  );

  const toolbar = (
    <div className="act-toolbar">
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
      <SelectField
        label="时间范围"
        value={range}
        onChange={(event) => {
          // 「全部」是默认值，选回它时删掉参数而不是写 range=all，保持地址栏干净。
          const value = event.target.value;
          update(RANGE_PARAM, value === "all" ? "" : value);
        }}
      >
        {TIME_RANGES.map((entry) => (
          <option key={entry.value} value={entry.value}>
            {entry.label}
          </option>
        ))}
      </SelectField>
      <SelectField
        label="活动种类"
        value={kindParam}
        onChange={(event) => {
          update(KIND_PARAM, event.target.value);
        }}
      >
        <option value="">全部种类</option>
        {EVENT_KINDS.map((value) => (
          <option key={value} value={value}>
            {KIND_LABELS[value]}
          </option>
        ))}
      </SelectField>
      {hasFilters ? (
        <Button
          variant="neutral"
          onClick={() => {
            setParams(new URLSearchParams(), { replace: true });
          }}
        >
          清除筛选
        </Button>
      ) : null}
    </div>
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

  return (
    <>
      {header}
      {toolbar}
      {filtered.length === 0 ? (
        <EmptyState
          title={hasFilters ? "当前筛选下没有活动记录" : "还没有活动记录"}
          description={
            hasFilters
              ? "换一个任务、岗位或时间范围，或清除筛选查看全部记录。"
              : "创建任务、调整任务状态或绑定 Harness 之后，这里会留下可回放的操作记录。"
          }
          action={
            hasFilters ? (
              <Button
                variant="neutral"
                onClick={() => {
                  setParams(new URLSearchParams(), { replace: true });
                }}
              >
                清除筛选
              </Button>
            ) : undefined
          }
        />
      ) : (
        <>
          <ul className="act-list">
            {filtered.map((event) => (
              <EventRow key={event.id} event={event} teamId={teamId} tasks={tasks} roles={roles} />
            ))}
          </ul>
          <div className="act-more">
            {events.hasNextPage ? (
              <Button
                variant="neutral"
                onClick={() => {
                  void events.fetchNextPage();
                }}
              >
                加载更多
              </Button>
            ) : (
              <p className="act-note">没有更早的记录了。</p>
            )}
          </div>
        </>
      )}
    </>
  );
}
