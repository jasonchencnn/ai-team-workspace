// 任务详情的执行区：执行信息条、实时进度流 / 结束状态区、右栏摘要，以及权限弹窗与接管确认。
// 页面顶部的「开始执行 / 取消执行 / 人工接管」按钮由 TaskDetailPage 用同一个 useTaskRuns 渲染，
// 这里只负责执行区本体，避免两处各管一份状态。
import { useEffect, useState } from "react";
import { useNavigate } from "react-router";

import { Button, ErrorBanner, Panel, Tag } from "../../../components/index.js";
import type { FileRef } from "../../../bindings/FileRef.js";
import type { TaskStatus } from "../../../bindings/TaskStatus.js";
import { roleBindingPath } from "../../roles/rolesNav.js";
import { PermissionDialog } from "./PermissionDialog.js";
import { RunOutcome } from "./RunOutcome.js";
import { RunProgressStream } from "./RunProgressStream.js";
import { RunSidePanel } from "./RunSidePanel.js";
import {
  execDirTitle,
  formatDuration,
  formatLocal,
  hasEnded,
  runDisplayStatus,
  runElapsedSeconds,
} from "./runUi.js";
import { isRunActive, useRunUpdates } from "./useRunEvents.js";
import type { TaskRunsApi } from "./useTaskRuns.js";
import "./run.css";

interface RunExecutionAreaProps {
  teamId: string;
  taskKey: string;
  taskTitle: string;
  roleName: string;
  roleFolder: string | null;
  fileRefs: FileRef[];
  api: TaskRunsApi;
  /** 任务当前可执行的状态流转（人工接管时判断能否直接提交验收）。 */
  allowedTransitions: TaskStatus[];
  /** 任务当前状态（后端值）。 */
  taskStatus: TaskStatus;
  /** 请求人工接管：由页面统一弹出确认框。 */
  onRequestTakeover: () => void;
}

export function RunExecutionArea({
  teamId,
  taskKey,
  taskTitle,
  roleName,
  roleFolder,
  fileRefs,
  api,
  allowedTransitions,
  taskStatus,
  onRequestTakeover,
}: RunExecutionAreaProps) {
  const run = api.run;
  const navigate = useNavigate();
  const { updates, loading } = useRunUpdates(run?.id ?? null);
  const [permissionOpen, setPermissionOpen] = useState(false);
  const [dismissed, setDismissed] = useState<string[]>([]);
  const [changedOpen, setChangedOpen] = useState(false);
  const [now, setNow] = useState(() => Date.now());

  const pending = run?.pendingPermission ?? null;
  const active = isRunActive(run);

  // 执行进行中时每秒刷新一次，让「已运行」计时走动；结束后停。
  useEffect(() => {
    if (!active) {
      return;
    }
    const timer = window.setInterval(() => {
      setNow(Date.now());
    }, 1000);
    return () => {
      window.clearInterval(timer);
    };
  }, [active]);

  // 新的权限请求自动弹出；已经手动关掉的那次不再自动打扰，可在执行区重新打开。
  useEffect(() => {
    if (pending !== null && !dismissed.includes(pending.requestId)) {
      setPermissionOpen(true);
    }
  }, [pending, dismissed]);

  if (run === null) {
    return null;
  }

  const elapsed = runElapsedSeconds(run, now);
  const ended = hasEnded(run);
  const status = runDisplayStatus(run);

  // 「去验收」进入验收页（M3-06）；手动流转仍在任务详情的「状态操作」区。
  const goReview = () => {
    void navigate(`/teams/${teamId}/tasks/${run.taskId}/review`);
  };

  return (
    <>
      <Panel className="run-info">
        <div className="run-info__cell">
          <span className="run-info__label">执行次数</span>
          <span className="run-info__value">第 {String(run.attempt)} 次执行</span>
        </div>
        <div className="run-info__cell">
          <span className="run-info__label">Harness</span>
          <span className="run-info__value">{run.harness.name}</span>
        </div>
        <div className="run-info__cell">
          <span className="run-info__label">执行目录</span>
          <span className="run-info__value run-info__value--path" title={run.execDir}>
            {execDirTitle(run)}
            <span className="run-info__sub">{run.execDir}</span>
          </span>
        </div>
        <div className="run-info__cell">
          <span className="run-info__label">开始时间</span>
          <span className="run-info__value">
            {formatLocal(run.startedAt)}
            <span className="run-info__sub">
              {ended ? `总时长 ${formatDuration(elapsed)}` : `已运行 ${formatDuration(elapsed)}`}
            </span>
          </span>
        </div>
        <div className="run-info__cell run-info__cell--status">
          <span className="run-info__label">状态</span>
          <Tag tone={status.tone}>
            <span className="run-info__dot" aria-hidden="true" />
            {status.label}
          </Tag>
        </div>
      </Panel>

      <div className="run-layout">
        <div className="run-main">
          {pending !== null && !permissionOpen ? (
            <div className="run-pending">
              <span>有一个权限请求在等你确认。</span>
              <Button
                variant="neutral"
                onClick={() => {
                  setPermissionOpen(true);
                }}
              >
                重新打开权限请求
              </Button>
            </div>
          ) : null}

          {api.error === null ? null : <ErrorBanner error={api.error} />}

          {active ? (
            <Panel className="run-progress">
              <div className="run-progress__head">
                <h2 className="run-progress__title">执行进度</h2>
                <span className="run-progress__meta">
                  实时更新 · 已收到 {String(updates.length)} 条事件
                </span>
              </div>
              <RunProgressStream
                run={run}
                updates={updates}
                loading={loading}
                roleName={roleName}
              />
            </Panel>
          ) : (
            <RunOutcome
              run={run}
              roleName={roleName}
              taskKey={taskKey}
              taskTitle={taskTitle}
              updates={updates}
              changedOpen={changedOpen}
              onToggleChanged={() => {
                setChangedOpen(!changedOpen);
              }}
              onGoReview={goReview}
              busy={api.busy}
              onStartNewRun={api.startRun}
              onResume={api.resumeRun}
              onMarkFailed={api.markRunFailed}
              onTakeover={onRequestTakeover}
              onRebind={() => {
                void navigate(roleBindingPath(teamId, run.roleId));
              }}
              onSubmitReview={api.submitReview}
              onOpenExecDir={api.openRunDir}
              execDirBusy={api.execDirPending}
              canSubmitReview={allowedTransitions.includes("review")}
              taskStatus={taskStatus}
            />
          )}
        </div>

        <RunSidePanel
          run={run}
          runs={api.runs}
          selectedRunId={run.id}
          roleFolder={roleFolder}
          fileRefs={fileRefs}
          onSelectRun={api.selectRun}
        />
      </div>

      {permissionOpen && pending !== null ? (
        <PermissionDialog
          run={run}
          prompt={pending}
          roleName={roleName}
          taskKey={taskKey}
          taskTitle={taskTitle}
          submitting={false}
          error={null}
          onRespond={(optionId) => {
            setDismissed((previous) => [...previous, pending.requestId]);
            setPermissionOpen(false);
            api.respondPermission(pending.requestId, optionId);
          }}
          onClose={() => {
            setDismissed((previous) => [...previous, pending.requestId]);
            setPermissionOpen(false);
          }}
        />
      ) : null}
    </>
  );
}
