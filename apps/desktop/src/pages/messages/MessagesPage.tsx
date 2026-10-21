import { useMutation, useQueries, useQuery, useQueryClient } from "@tanstack/react-query";
import { useMemo, useState } from "react";
import { Link, useNavigate, useParams, useSearchParams } from "react-router";

import {
  getTaskBoard,
  getThread,
  listRoles,
  listThreads,
  retryMessage,
} from "../../api/commands.js";
import { queryKeys } from "../../api/queryKeys.js";
import {
  Button,
  Card,
  EmptyState,
  ErrorBanner,
  PageHeader,
  Panel,
  SelectField,
  Spinner,
  Tag,
} from "../../components/index.js";
import type { MessageThread } from "../../bindings/MessageThread.js";
import type { ThreadDetail } from "../../bindings/ThreadDetail.js";
import { healthLabel } from "../harness-detail/harnessFormat.js";
import { MessageBubble } from "./MessageBubble.js";
import { SendMessageDialog } from "./SendMessageDialog.js";
import {
  MAX_LISTED_THREADS,
  MESSAGE_STATUS_LABELS,
  MESSAGE_STATUS_TONES,
  ROLE_PARAM,
  TASK_PARAM,
  THREAD_PARAM,
  byUpdatedDesc,
  formatMessageTime,
  latestMessage,
  parseThreadFilters,
  participantsLabel,
  shouldPollMessages,
  threadPreview,
} from "./messagesUi.js";
import { useMessageEventInvalidation } from "./useMessageEvents.js";
import { useMessagePolling } from "./useMessagePolling.js";
import "./messages.css";

/** 消息相关的活动种类：活动与审计页按这个组合过滤出线程的往来记录。 */
const MESSAGE_EVENT_KINDS = [
  "message_sent",
  "message_replied",
  "message_delivery_failed",
  "message_expired",
].join(",");

export function MessagesPage() {
  const { teamId = "" } = useParams();
  const [params, setParams] = useSearchParams();
  const navigate = useNavigate();
  const queryClient = useQueryClient();
  const [sendOpen, setSendOpen] = useState(false);
  // 列表里的时间用同一个「现在」判断归属日，避免每次渲染都变。
  const clock = useMemo(() => new Date(), []);

  const filters = parseThreadFilters(params);
  const threadParam = params.get(THREAD_PARAM) ?? "";

  const threads = useQuery({
    queryKey: queryKeys.messageThreadList(teamId, {
      teamId,
      taskId: filters.taskId === "" ? null : filters.taskId,
      roleId: filters.roleId === "" ? null : filters.roleId,
    }),
    queryFn: () =>
      listThreads({
        teamId,
        taskId: filters.taskId === "" ? null : filters.taskId,
        roleId: filters.roleId === "" ? null : filters.roleId,
      }),
    enabled: teamId !== "",
  });

  const roles = useQuery({
    queryKey: queryKeys.roles(teamId),
    queryFn: () => listRoles(teamId),
    enabled: teamId !== "",
  });

  // 关联任务的编号与标题来自看板接口。
  const board = useQuery({
    queryKey: queryKeys.taskBoard(teamId, { teamId, includeCancelled: true }),
    queryFn: () => getTaskBoard({ teamId, includeCancelled: true }),
    enabled: teamId !== "",
  });

  const listed = useMemo(
    () => byUpdatedDesc(threads.data ?? []).slice(0, MAX_LISTED_THREADS),
    [threads.data],
  );

  // 当前线程：地址栏指定了就用指定的（可以带筛选深链进来），否则取最近更新的那个。
  const selectedThreadId = threadParam !== "" ? threadParam : (listed[0]?.id ?? "");

  // 列表里其余线程的详情：只为拿到「最近一条的状态与时间」，所以按需取，不缓存成长期数据。
  const otherThreadIds = useMemo(
    () => listed.filter((thread) => thread.id !== selectedThreadId).map((thread) => thread.id),
    [listed, selectedThreadId],
  );
  const otherDetails = useQueries({
    queries: otherThreadIds.map((threadId) => ({
      queryKey: queryKeys.messageThread(teamId, threadId),
      queryFn: () => getThread(threadId),
      enabled: teamId !== "" && threadId !== "",
      staleTime: 2000,
    })),
  });

  const detail = useQuery({
    queryKey: queryKeys.messageThread(teamId, selectedThreadId),
    queryFn: () => getThread(selectedThreadId),
    enabled: teamId !== "" && selectedThreadId !== "",
  });

  // 状态更新有两条通道：事件订阅（useMessageEvents，内核还没开始推送消息事件）
  // 与轮询兜底（useMessagePolling）。消息进入终态后轮询就停。
  const pending = useMemo(() => {
    const messages = [
      ...(detail.data?.messages ?? []),
      ...otherDetails.flatMap((result) => result.data?.messages ?? []),
    ];
    return shouldPollMessages(messages);
  }, [detail.data, otherDetails]);
  useMessagePolling(teamId, pending);
  useMessageEventInvalidation(teamId);

  const roleViews = roles.data ?? [];
  const roleNames = useMemo(
    () => new Map(roleViews.map((view) => [view.role.id, view.role.name])),
    [roleViews],
  );
  const taskKeys = useMemo(() => {
    const map = new Map<string, string>();
    for (const column of board.data?.columns ?? []) {
      for (const task of column.tasks) {
        map.set(task.id, `${task.key} ${task.title}`);
      }
    }
    return map;
  }, [board.data]);

  const retry = useMutation({
    mutationFn: (messageId: string) => retryMessage(messageId),
    onSuccess: async () => {
      await queryClient.invalidateQueries({ queryKey: queryKeys.messageThreads(teamId) });
    },
  });

  const updateParam = (key: string, value: string, replace: boolean) => {
    const next = new URLSearchParams(params);
    if (value === "") {
      next.delete(key);
    } else {
      next.set(key, value);
    }
    setParams(next, { replace });
  };

  const selectThread = (threadId: string) => {
    updateParam(THREAD_PARAM, threadId, false);
  };

  const selectedThread: MessageThread | undefined =
    detail.data?.thread ?? listed.find((thread) => thread.id === selectedThreadId);

  // 详情之外的线程详情，按线程 id 归拢，列表里用来显示最近一条的状态与时间。
  // 当前线程的详情也放进来：列表里选中项同样要显示状态。
  const detailByThread = useMemo(() => {
    const map = new Map<string, ThreadDetail>();
    if (detail.data !== undefined) {
      map.set(detail.data.thread.id, detail.data);
    }
    for (const result of otherDetails) {
      if (result.data !== undefined) {
        map.set(result.data.thread.id, result.data);
      }
    }
    return map;
  }, [detail.data, otherDetails]);

  // 有失败或过期消息的线程：列表里的「处理未投递消息」入口用它。
  const undelivered = useMemo(
    () =>
      listed.find((thread) => {
        const messages = detailByThread.get(thread.id)?.messages ?? [];
        return messages.some(
          (message) => message.status === "failed" || message.status === "expired",
        );
      }) ?? null,
    [listed, detailByThread],
  );

  const header = (
    <PageHeader
      title="岗位通信"
      description="消息由 Teamspace 中转：发给岗位的消息进入排队，交给该岗位当前绑定的 Harness 处理，回复写回同一线程。"
      actions={
        <Button
          onClick={() => {
            setSendOpen(true);
          }}
        >
          发送消息
        </Button>
      }
    />
  );

  const filtersPanel = (
    <Panel className="msg-filters">
      <SelectField
        label="岗位"
        value={filters.roleId}
        onChange={(event) => {
          updateParam(ROLE_PARAM, event.target.value, true);
        }}
      >
        <option value="">全部岗位</option>
        {roleViews.map((view) => (
          <option key={view.role.id} value={view.role.id}>
            {view.role.name}
          </option>
        ))}
      </SelectField>
      <SelectField
        label="任务"
        value={filters.taskId}
        onChange={(event) => {
          updateParam(TASK_PARAM, event.target.value, true);
        }}
      >
        <option value="">全部任务</option>
        {(board.data?.columns ?? [])
          .flatMap((column) => column.tasks)
          .sort((a, b) => a.key.localeCompare(b.key, "zh-CN"))
          .map((task) => (
            <option key={task.id} value={task.id}>
              {task.key} {task.title}
            </option>
          ))}
      </SelectField>
      {filters.roleId !== "" || filters.taskId !== "" ? (
        <Button
          variant="subtle"
          onClick={() => {
            const next = new URLSearchParams(params);
            next.delete(ROLE_PARAM);
            next.delete(TASK_PARAM);
            setParams(next, { replace: true });
          }}
        >
          清除筛选
        </Button>
      ) : null}
    </Panel>
  );

  if (threads.isPending) {
    return (
      <>
        {header}
        {filtersPanel}
        <div className="msg-state">
          <Spinner label="正在加载线程" />
        </div>
      </>
    );
  }

  if (threads.isError) {
    return (
      <>
        {header}
        {filtersPanel}
        <ErrorBanner
          error={threads.error}
          action={
            <Button variant="neutral" onClick={() => void threads.refetch()}>
              重试
            </Button>
          }
        />
      </>
    );
  }

  const listPanel = (
    <Panel className="msg-list">
      <h2 className="msg-list__title">线程</h2>
      {listed.length === 0 ? (
        <EmptyState
          bare
          title="还没有线程"
          description="给岗位发第一条消息，这里会记录往来与投递状态。"
        />
      ) : (
        <ul className="msg-list__items">
          {listed.map((thread) => {
            const messages = detailByThread.get(thread.id)?.messages ?? [];
            const last = latestMessage(messages);
            const active = thread.id === selectedThreadId;
            return (
              <li key={thread.id}>
                <button
                  type="button"
                  className={active ? "msg-thread msg-thread--active" : "msg-thread"}
                  aria-current={active ? "true" : undefined}
                  onClick={() => {
                    selectThread(thread.id);
                  }}
                >
                  <span className="msg-thread__subject">{thread.subject}</span>
                  <span className="msg-thread__meta">
                    {/* 状态标签要等线程详情取回来才有；还没取回来时只显示时间，不装作没有消息。 */}
                    {last === null ? null : (
                      <Tag tone={MESSAGE_STATUS_TONES[last.status]}>
                        {MESSAGE_STATUS_LABELS[last.status]}
                      </Tag>
                    )}
                    <span className="msg-thread__time">
                      {formatMessageTime(thread.updatedAt, clock)}
                    </span>
                  </span>
                  <span className="msg-thread__preview">{threadPreview(messages)}</span>
                  <span className="msg-thread__participants">
                    参与方：{participantsLabel(thread.participants)}
                  </span>
                  {thread.taskId === null ? null : (
                    <span className="msg-thread__task">
                      关联任务：{taskKeys.get(thread.taskId) ?? "（已删除的任务）"}
                    </span>
                  )}
                </button>
              </li>
            );
          })}
        </ul>
      )}
      <div className="msg-list__actions">
        <Button
          onClick={() => {
            setSendOpen(true);
          }}
        >
          发送消息
        </Button>
        <Button
          variant="neutral"
          disabled={undelivered === null}
          title={undelivered === null ? "没有失败或过期的消息" : undefined}
          onClick={() => {
            if (undelivered !== null) {
              selectThread(undelivered.id);
            }
          }}
        >
          处理未投递消息
        </Button>
      </div>
    </Panel>
  );

  const messages = detail.data?.messages ?? [];

  const detailPanel = (
    <Panel className="msg-detail">
      {selectedThread === undefined ? (
        <EmptyState
          bare
          title="还没有选中线程"
          description="左侧选一个线程，或发送新消息开始一段往来。"
        />
      ) : (
        <>
          <header className="msg-detail__head">
            <h2 className="msg-detail__subject">{selectedThread.subject}</h2>
            <p className="msg-detail__meta">
              <span>参与方：{participantsLabel(selectedThread.participants)}</span>
              {selectedThread.taskId === null ? null : (
                <span>
                  关联任务：
                  <Link to={`/teams/${teamId}/tasks/${selectedThread.taskId}`}>
                    {taskKeys.get(selectedThread.taskId) ?? "（已删除的任务）"}
                  </Link>
                </span>
              )}
              <span>
                最近更新：
                <time dateTime={selectedThread.updatedAt}>
                  {formatMessageTime(selectedThread.updatedAt, clock)}
                </time>
              </span>
            </p>
          </header>

          {detail.isPending ? (
            <div className="msg-state">
              <Spinner label="正在加载消息" />
            </div>
          ) : detail.isError ? (
            <ErrorBanner
              error={detail.error}
              action={
                <Button variant="neutral" onClick={() => void detail.refetch()}>
                  重试
                </Button>
              }
            />
          ) : messages.length === 0 ? (
            <EmptyState bare title="这个线程还没有消息" />
          ) : (
            <div className="msg-detail__bubbles">
              {messages.map((message) => (
                <MessageBubble
                  key={message.id}
                  message={message}
                  roleNames={roleNames}
                  now={clock}
                  retrying={retry.isPending && retry.variables === message.id}
                  onRetry={(target) => {
                    retry.mutate(target.id);
                  }}
                />
              ))}
            </div>
          )}
        </>
      )}
    </Panel>
  );

  // 路由与审计：能显示的只有后端给的信息（目标岗位、当前绑定、关联任务）。
  const lastMessage =
    selectedThread === undefined ? null : latestMessage(detail.data?.messages ?? []);
  const targetRole =
    lastMessage === null
      ? undefined
      : roleViews.find((view) => view.role.id === lastMessage.toRoleId);
  const auditLink = `/teams/${teamId}/activity?kind=${MESSAGE_EVENT_KINDS}${
    targetRole === undefined ? "" : `&role=${targetRole.role.id}`
  }`;

  const sidePanel = (
    <div className="msg-side">
      <Card title="投递快照">
        <dl className="msg-side__list">
          <dt>目标岗位</dt>
          <dd>{targetRole?.role.name ?? "—"}</dd>
          <dt>当前绑定</dt>
          <dd>
            {targetRole?.harness === null || targetRole?.harness === undefined
              ? "未绑定"
              : `${targetRole.harness.name}（${healthLabel(targetRole.harness.handshake.status)}）`}
          </dd>
          <dt>关联任务</dt>
          <dd>
            {selectedThread?.taskId === null || selectedThread?.taskId === undefined
              ? "—"
              : (taskKeys.get(selectedThread.taskId) ?? "（已删除的任务）")}
          </dd>
        </dl>
      </Card>
      <Card title="投递状态" tone="info">
        <p>排队中 → 投递中 → 已回复；失败或过期保留原消息与原因，可重新发送。</p>
      </Card>
      <Link className="msg-side__link" to={auditLink}>
        查看线程审计
      </Link>
    </div>
  );

  return (
    <>
      {header}
      {filtersPanel}
      <div className="msg-layout">
        {listPanel}
        {detailPanel}
        {sidePanel}
      </div>

      {sendOpen ? (
        <SendMessageDialog
          teamId={teamId}
          onClose={() => {
            setSendOpen(false);
          }}
          onSent={(message) => {
            // 发送后进入线程：消息先显示「排队中」，投递结果由轮询跟进。
            void navigate(`/teams/${teamId}/messages?thread=${message.threadId}`);
          }}
        />
      ) : null}
    </>
  );
}
