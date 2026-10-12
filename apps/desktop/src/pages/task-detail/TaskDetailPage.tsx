import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useState } from "react";
import { Link, useNavigate, useParams } from "react-router";

import { changeTaskStatus, getTask, listRoles, listTaskStatuses } from "../../api/commands.js";
import { errorMessage } from "../../api/errors.js";
import { queryKeys } from "../../api/queryKeys.js";
import {
  Button,
  Card,
  ErrorBanner,
  PageHeader,
  Panel,
  Spinner,
  Tag,
  TextareaField,
} from "../../components/index.js";
import type { TaskStatus } from "../../bindings/TaskStatus.js";
import type { TaskSummary } from "../../bindings/TaskSummary.js";
import { formatTime, statusTone } from "../tasks/taskUi.js";
import "./task-detail.css";

export function TaskDetailPage() {
  const { teamId = "", taskId = "" } = useParams();
  const navigate = useNavigate();
  const queryClient = useQueryClient();
  const [blockingOpen, setBlockingOpen] = useState(false);
  const [reason, setReason] = useState("");

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
  const backButton = (
    <Button variant="neutral" onClick={() => void navigate(boardPath)}>
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
  const harness = roles.data?.find((view) => view.role.id === owner.id)?.harness ?? null;

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
            <Button onClick={() => void navigate(`${boardPath}/${task.id}/edit`)}>编辑</Button>
          </>
        }
      />

      <div className="task-detail">
        <Panel className="task-detail__main">
          <p className={`task-detail__status task-detail__status--${task.status}`}>
            <span className="task-detail__dot" aria-hidden="true" />
            {statusLabel(task.status)} / {task.key}
          </p>

          <section className="task-detail__section">
            <h2>任务说明</h2>
            {fileRefs.length > 0 ? (
              <ul className="task-detail__files">
                {fileRefs.map((ref) => (
                  <li
                    key={`${ref.path}#${ref.anchor ?? ""}`}
                    className={ref.exists ? "file-chip" : "file-chip file-chip--missing"}
                  >
                    {ref.path}
                    {ref.anchor === null ? "" : ` #${ref.anchor}`}
                    {ref.exists ? null : <span className="file-chip__note">文件不存在</span>}
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

          <section className="task-detail__section">
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
              <p className="task-detail__muted">当前没有可执行的状态操作。</p>
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
              <ul className="task-detail__events">
                {events.map((event) => (
                  <li key={event.id}>
                    <time dateTime={event.createdAt}>{formatTime(event.createdAt)}</time>
                    <span>{event.summary}</span>
                  </li>
                ))}
              </ul>
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
    </>
  );
}
