import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useState } from "react";
import { Link, useLocation, useNavigate, useParams } from "react-router";

import {
  changeTaskStatus,
  getReview,
  getTask,
  listRoles,
  listTaskStatuses,
  listThreads,
} from "../../api/commands.js";
import { errorMessage } from "../../api/errors.js";
import { queryKeys } from "../../api/queryKeys.js";
import {
  Button,
  Card,
  ErrorBanner,
  Modal,
  PageHeader,
  Panel,
  Spinner,
  Tag,
  TextareaField,
} from "../../components/index.js";
import type { Message } from "../../bindings/Message.js";
import type { TaskStatus } from "../../bindings/TaskStatus.js";
import type { TaskSummary } from "../../bindings/TaskSummary.js";
import { RunExecutionArea } from "./run/RunExecutionArea.js";
import { TaskDependencies } from "./TaskDependencies.js";
import { canGoReview, showStartRunButton } from "./run/runUi.js";
import { isRunActive } from "./run/useRunEvents.js";
import { useRunRecordPolling } from "./run/useRunPolling.js";
import { useTaskRuns } from "./run/useTaskRuns.js";
import { SendMessageDialog } from "../messages/SendMessageDialog.js";
import { isMarkdownRef } from "../files/filesUi.js";
import { formatTime, statusTone, taskPanelSearch } from "../tasks/taskUi.js";
import "./task-detail.css";

export function TaskDetailPage() {
  const { teamId = "", taskId = "" } = useParams();
  const navigate = useNavigate();
  const location = useLocation();
  const queryClient = useQueryClient();
  const [blockingOpen, setBlockingOpen] = useState(false);
  const [reason, setReason] = useState("");
  const [takeoverOpen, setTakeoverOpen] = useState(false);
  // 岗位通信：给负责岗位发消息的弹窗，以及刚发出的消息所在的线程。
  const [messageOpen, setMessageOpen] = useState(false);
  const [sentThreadId, setSentThreadId] = useState<string | null>(null);

  const detail = useQuery({
    queryKey: queryKeys.task(teamId, taskId),
    queryFn: () => getTask(taskId),
    enabled: teamId !== "" && taskId !== "",
  });
  const statuses = useQuery({
    queryKey: queryKeys.taskStatuses,
    queryFn: listTaskStatuses,
    staleTime: Infinity,
  });
  const roles = useQuery({
    queryKey: queryKeys.roles(teamId),
    queryFn: () => listRoles(teamId),
    enabled: teamId !== "",
  });
  // 与本任务关联的岗位通信线程：详情页给出去线程的入口，往来正文在岗位通信页看。
  const threads = useQuery({
    queryKey: queryKeys.messageThreadList(teamId, { teamId, taskId, roleId: null }),
    queryFn: () => listThreads({ teamId, taskId, roleId: null }),
    enabled: teamId !== "" && taskId !== "",
  });
  const runApi = useTaskRuns(teamId, taskId);
  // 事件之外的第二条通道（D23）：执行进行中定时刷新执行列表、任务详情与待验收入口，
  // 执行结束后停止。事件正常时也保留，保证不依赖单一通道。
  useRunRecordPolling(teamId, taskId, runApi.run);
  // 「去验收」入口：待验收，或因合并冲突受阻（是否因冲突以验收数据的 conflict 为准）。
  const taskStatus = detail.data?.task.status;
  const reviewEntry = useQuery({
    queryKey: queryKeys.review(teamId, taskId),
    queryFn: () => getReview(taskId),
    enabled:
      teamId !== "" && taskId !== "" && (taskStatus === "review" || taskStatus === "blocked"),
  });
  const reviewVisible =
    taskStatus === "review" || (taskStatus === "blocked" && reviewEntry.data?.conflict === true);

  const statusLabel = (status: TaskStatus) =>
    statuses.data?.find((info) => info.status === status)?.label ?? status;

  const transition = useMutation({
    mutationFn: (input: { to: TaskStatus; reason?: string }) =>
      changeTaskStatus({ taskId, to: input.to, reason: input.reason ?? null }),
    onSuccess: async () => {
      setBlockingOpen(false);
      setReason("");
      await queryClient.invalidateQueries({ queryKey: queryKeys.tasks(teamId) });
      await queryClient.invalidateQueries({ queryKey: queryKeys.teamOverview(teamId) });
    },
  });

  const boardPath = `/teams/${teamId}/tasks`;
  // 面板的视图与筛选存在地址栏里：带着参数返回，返回后仍是之前的视图和筛选；
  // 从别处直接进详情时地址栏没有这些参数，返回就是默认面板。
  const backPath = `${boardPath}${taskPanelSearch(location.search)}`;
  const backButton = (
    <Button variant="neutral" onClick={() => void navigate(backPath)}>
      返回任务面板
    </Button>
  );

  if (detail.isPending) {
    return (
      <>
        <PageHeader title="任务详情" actions={backButton} />
        <div className="task-detail__state">
          <Spinner label="正在加载任务" />
        </div>
      </>
    );
  }
  if (detail.isError) {
    return (
      <>
        <PageHeader title="任务详情" actions={backButton} />
        <ErrorBanner
          error={detail.error}
          action={
            <Button variant="neutral" onClick={() => void detail.refetch()}>
              重试
            </Button>
          }
        />
      </>
    );
  }

  const { task, owner, fileRefs, dependencies, dependents, allowedTransitions, readiness, events } =
    detail.data;
  const roleProblems = detail.data.runProblems;
  // runProblems 与 readiness 有重叠（同一句话可能两边都有）。这里只补充 readiness 没覆盖的
  // 「不能开始执行」原因（如未绑定 Harness），避免同一条原因在页面上出现两次。
  const extraRunProblems = roleProblems.filter((problem) => !readiness.includes(problem));
  const ownerView = roles.data?.find((view) => view.role.id === owner.id);
  const harness = ownerView?.harness ?? null;
  const run = runApi.run;
  const active = isRunActive(run);

  // 页头按钮随执行状态切换：执行中是「人工接管」「取消执行」。
  // 失败、已中断、已接管、成功这四种状态，执行区面板里已经带了主操作（重试 / 续接 / 重新执行），
  // 页头再放一次会出现两个主按钮，所以这里只留「返回任务面板」和「编辑」。
  // 任务验收通过（done）后不再显示开始 / 重新执行：内核此时不接受开始执行，
  // 要重做得先在「状态操作」里重新打开为 READY（判定见 runUi.showStartRunButton）。
  // 其余状态能否开始以后端的 runProblems 为准，不可用时把原因显示在下面。
  const outcomeHasActions =
    run !== null &&
    !active &&
    ((run.status === "succeeded" && canGoReview(run, task.status)) ||
      run.status === "failed" ||
      run.status === "interrupted" ||
      run.takenOver);
  const runHeaderActions = active ? (
    <>
      <Button
        variant="neutral"
        disabled={runApi.busy}
        onClick={() => {
          setTakeoverOpen(true);
        }}
      >
        人工接管
      </Button>
      <Button
        variant="neutral"
        loading={runApi.busy}
        disabled={runApi.busy}
        onClick={runApi.cancelRun}
      >
        取消执行
      </Button>
    </>
  ) : outcomeHasActions || !showStartRunButton(task.status) ? null : (
    <Button
      loading={runApi.busy}
      disabled={runApi.busy || roleProblems.length > 0}
      title={roleProblems.length === 0 ? undefined : `还不能开始执行：${roleProblems.join("；")}`}
      onClick={runApi.startRun}
    >
      {run === null ? "开始执行" : "重新执行"}
    </Button>
  );

  const requestTransition = (to: TaskStatus) => {
    transition.reset();
    if (to === "blocked") {
      setBlockingOpen(true);
      return;
    }
    setBlockingOpen(false);
    transition.mutate({ to });
  };

  const submitBlocked = () => {
    transition.mutate({ to: "blocked", reason: reason.trim() });
  };

  const renderDependencies = (items: TaskSummary[]) =>
    items.length === 0 ? (
      <p className="task-detail__muted">暂无</p>
    ) : (
      <ul className="task-detail__dep-list">
        {items.map((item) => (
          <li key={item.id}>
            <Link to={`/teams/${teamId}/tasks/${item.id}`}>
              {item.key} {item.title}
            </Link>{" "}
            <Tag tone={statusTone[item.status]}>{statusLabel(item.status)}</Tag>
          </li>
        ))}
      </ul>
    );

  return (
    <>
      <PageHeader
        title={task.title}
        description={`${task.key} / ${owner.name}`}
        actions={
          <>
            {backButton}
            {runHeaderActions}
            {active ? null : (
              <Button
                variant="neutral"
                onClick={() => void navigate(`${boardPath}/${task.id}/edit`)}
              >
                编辑
              </Button>
            )}
          </>
        }
      />

      {reviewVisible ? (
        <div className="task-detail__review-entry" role="region" aria-label="验收入口">
          <div>
            <strong>
              {taskStatus === "review" ? "这个任务在等你验收" : "合并冲突，需要你处理"}
            </strong>
            <p>
              {taskStatus === "review"
                ? "查看摘要与改动，通过或退回。"
                : "查看冲突说明，手动合并后标记完成，或重新执行。"}
            </p>
          </div>
          <Button
            onClick={() => {
              void navigate(`${boardPath}/${task.id}/review`);
            }}
          >
            去验收
          </Button>
        </div>
      ) : null}

      {run === null && roleProblems.length === 0 ? (
        <p className="task-detail__hint">准备就绪后即可派给「{owner.name}」执行。</p>
      ) : null}
      {roleProblems.length > 0 && !active && extraRunProblems.length > 0 ? (
        <Card title="还不能开始执行" tone="warning">
          <ul className="task-detail__readiness">
            {extraRunProblems.map((problem) => (
              <li key={problem}>{problem}</li>
            ))}
          </ul>
        </Card>
      ) : null}

      <RunExecutionArea
        teamId={teamId}
        taskKey={task.key}
        taskTitle={task.title}
        roleName={owner.name}
        roleFolder={ownerView?.folder ?? null}
        fileRefs={task.fileRefs}
        api={runApi}
        allowedTransitions={allowedTransitions}
        taskStatus={task.status}
        onRequestTakeover={() => {
          setTakeoverOpen(true);
        }}
      />

      <div className="task-detail">
        <Panel className="task-detail__main">
          <p className={`task-detail__status task-detail__status--${task.status}`}>
            <span className="task-detail__dot" aria-hidden="true" />
            {statusLabel(task.status)} / {task.key}
          </p>

          <section className="task-detail__section">
            <TaskDependencies detail={detail.data} statusLabel={statusLabel} />
          </section>

          <section className="task-detail__section">
            <h2>任务说明</h2>
            {fileRefs.length > 0 ? (
              <ul className="task-detail__files">
                {fileRefs.map((ref) => (
                  <li
                    key={`${ref.path}#${ref.anchor ?? ""}`}
                    className={ref.exists ? "file-chip" : "file-chip file-chip--missing"}
                  >
                    {/* 存在的 .md 引用是去共享文件页看正文的入口；其余保持文字。 */}
                    {ref.exists && isMarkdownRef(ref.path) ? (
                      <Link
                        className="file-chip__link"
                        to={`/teams/${teamId}/files?path=${encodeURIComponent(ref.path)}`}
                      >
                        {ref.path}
                        {ref.anchor === null ? "" : ` #${ref.anchor}`}
                      </Link>
                    ) : (
                      <>
                        {ref.path}
                        {ref.anchor === null ? "" : ` #${ref.anchor}`}
                      </>
                    )}
                    {ref.exists ? null : (
                      <span className="file-chip__note">文件已被移动或删除</span>
                    )}
                  </li>
                ))}
              </ul>
            ) : null}
            {task.description === "" ? (
              <p className="task-detail__muted">暂无说明。</p>
            ) : (
              <p className="task-detail__description">{task.description}</p>
            )}
          </section>

          <section className="task-detail__section" id="task-status-actions">
            <h2>状态操作</h2>
            {task.status === "blocked" && task.blockedReason !== null ? (
              <p className="task-detail__muted">受阻原因：{task.blockedReason}</p>
            ) : null}
            {readiness.length > 0 ? (
              <Card title="进入 READY 前还需要" tone="warning">
                <ul className="task-detail__readiness">
                  {readiness.map((problem) => (
                    <li key={problem}>{problem}</li>
                  ))}
                </ul>
              </Card>
            ) : null}
            {allowedTransitions.length === 0 ? (
              <p className="task-detail__muted">
                {active
                  ? "执行进行中，暂不能变更任务状态；可以「取消执行」或「人工接管」后再改。"
                  : "当前没有可执行的状态操作。"}
              </p>
            ) : (
              <div className="task-detail__actions">
                {allowedTransitions.map((to) => (
                  <Button
                    key={to}
                    variant="neutral"
                    disabled={transition.isPending}
                    onClick={() => {
                      requestTransition(to);
                    }}
                  >
                    {statusLabel(to)}
                  </Button>
                ))}
              </div>
            )}
            {blockingOpen ? (
              <div className="task-detail__block-form">
                <TextareaField
                  label="受阻原因"
                  required
                  rows={3}
                  value={reason}
                  onChange={(event) => {
                    setReason(event.target.value);
                  }}
                />
                <div className="task-detail__actions">
                  <Button
                    loading={transition.isPending}
                    disabled={reason.trim() === ""}
                    onClick={submitBlocked}
                  >
                    确认受阻
                  </Button>
                  <Button
                    variant="subtle"
                    onClick={() => {
                      setBlockingOpen(false);
                    }}
                  >
                    取消
                  </Button>
                </div>
              </div>
            ) : null}
            {transition.isError ? (
              <p className="task-detail__error" role="alert">
                {errorMessage(transition.error)}
              </p>
            ) : null}
          </section>

          <section className="task-detail__section">
            <h2>活动记录</h2>
            {events.length === 0 ? (
              <p className="task-detail__muted">暂无活动。</p>
            ) : (
              <>
                <ul className="task-detail__events">
                  {events.map((event) => (
                    <li key={event.id}>
                      <time dateTime={event.createdAt}>{formatTime(event.createdAt)}</time>
                      <span>{event.summary}</span>
                    </li>
                  ))}
                </ul>
                {/* 带上任务筛选跳过去，活动与审计页按这个任务列出全部记录。 */}
                <p className="task-detail__muted">
                  <Link to={`/teams/${teamId}/activity?task=${taskId}`}>查看全部活动</Link>
                </p>
              </>
            )}
          </section>
        </Panel>

        <Panel className="task-detail__side">
          <p className="task-detail__caption">负责人岗位</p>
          <h2 className="task-detail__owner">
            {owner.name}
            {owner.archived ? <Tag tone="neutral">已归档</Tag> : null}
          </h2>
          <div>
            <p className="task-detail__caption">当前绑定</p>
            <p>{harness === null ? "未绑定" : harness.name}</p>
          </div>
          <div className="task-detail__message">
            <p className="task-detail__caption">岗位通信</p>
            <Button
              variant="neutral"
              onClick={() => {
                setSentThreadId(null);
                setMessageOpen(true);
              }}
            >
              给负责岗位发消息
            </Button>
            {sentThreadId === null ? null : (
              <p className="task-detail__message-sent">
                消息已排队。
                <Link to={`/teams/${teamId}/messages?thread=${sentThreadId}`}>查看线程</Link>
              </p>
            )}
            {(threads.data ?? []).length === 0 ? (
              <p className="task-detail__muted">还没有与本任务相关的通信线程。</p>
            ) : (
              <ul className="task-detail__thread-list">
                {(threads.data ?? []).map((thread) => (
                  <li key={thread.id}>
                    <Link to={`/teams/${teamId}/messages?thread=${thread.id}`}>
                      {thread.subject}
                    </Link>
                  </li>
                ))}
              </ul>
            )}
          </div>
          <Button variant="neutral" onClick={() => void navigate(`/teams/${teamId}/roles`)}>
            查看岗位与绑定
          </Button>
          <div>
            <p className="task-detail__caption">前置依赖</p>
            {renderDependencies(dependencies)}
          </div>
          <div>
            <p className="task-detail__caption">后续依赖</p>
            {renderDependencies(dependents)}
          </div>
          {task.status === "doing" || task.status === "review" ? (
            <p className="task-detail__notice">
              本次执行结束后提交验收。
              <br />
              执行完成不代表任务已验收。
            </p>
          ) : null}
        </Panel>
      </div>

      {takeoverOpen && run !== null ? (
        <Modal
          title="人工接管"
          description={`${task.key} ${task.title} · 第 ${String(run.attempt)} 次执行 · ${run.harness.name}`}
          onClose={() => {
            setTakeoverOpen(false);
          }}
          actions={
            <>
              <Button
                loading={runApi.busy}
                disabled={runApi.busy}
                onClick={() => {
                  setTakeoverOpen(false);
                  runApi.takeoverRun();
                }}
              >
                确认接管
              </Button>
              <Button
                variant="subtle"
                disabled={runApi.busy}
                onClick={() => {
                  setTakeoverOpen(false);
                }}
              >
                取消
              </Button>
            </>
          }
        >
          <p className="run-takeover__text">
            接管会停止这条执行：Agent 进程结束，待确认的权限请求被撤销，Agent
            不再改动任何文件。任务保持执行中， 由你在执行目录里自己继续处理。
          </p>
          <p className="run-takeover__text run-takeover__text--muted">
            已经产生的改动会保留。接管后可以重新执行第 {String(run.attempt + 1)}{" "}
            次，也可以直接把任务提交验收。
          </p>
          <p className="run-takeover__path" title={run.execDir}>
            执行目录：{run.execDir}
          </p>
        </Modal>
      ) : null}

      {messageOpen ? (
        <SendMessageDialog
          teamId={teamId}
          defaultRoleId={owner.id}
          defaultTaskId={task.id}
          onClose={() => {
            setMessageOpen(false);
          }}
          onSent={(message: Message) => {
            setSentThreadId(message.threadId);
          }}
        />
      ) : null}
    </>
  );
}
