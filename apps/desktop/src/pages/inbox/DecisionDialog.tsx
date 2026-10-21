// 「记录决策」弹窗：受阻任务与需要拍板的消息上可用。
// 决策写入后由后端记活动；勾选「同时把任务恢复到 READY」时后端会走就绪条件校验。
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useEffect, useState } from "react";

import { getTask, recordDecision } from "../../api/commands.js";
import { ErrorBanner } from "../../components/index.js";
import { Button, Modal, TextareaField } from "../../components/index.js";
import { queryKeys } from "../../api/queryKeys.js";
import type { InboxItem } from "../../bindings/InboxItem.js";
import {
  canSubmitDecision,
  decisionSubmitLabel,
  MAX_DECISION_TEXT,
  validateDecisionText,
} from "./inboxUi.js";
import "./inbox.css";

export interface DecisionDialogProps {
  teamId: string;
  /** 触发决策的待办事项（受阻任务或需要拍板的消息）。 */
  item: InboxItem;
  onClose: () => void;
  /** 决策记录成功（带上后端返回的决策）。 */
  onRecorded: (decision: {
    text: string;
    taskKey: string | null;
    roleName: string | null;
    resumedTask: boolean;
  }) => void;
}

/** 记录决策弹窗。 */
export function DecisionDialog({ teamId, item, onClose, onRecorded }: DecisionDialogProps) {
  const queryClient = useQueryClient();
  const [text, setText] = useState("");
  const [resumeTask, setResumeTask] = useState(false);
  const [touched, setTouched] = useState(false);

  // 受阻任务一定是 Blocked；消息类事项要问后端这个任务现在是什么状态。
  const blockedByCategory = item.category === "blocked";
  const taskQuery = useQuery({
    queryKey: queryKeys.task(teamId, item.taskId ?? ""),
    queryFn: () => getTask(item.taskId ?? ""),
    enabled: !blockedByCategory && item.taskId !== null,
  });
  const taskStatus = blockedByCategory ? "blocked" : taskQuery.data?.task.status;
  const taskKey = item.taskKey ?? taskQuery.data?.task.key ?? null;
  const canResume = taskStatus === "blocked";

  // 任务确实受阻时默认勾选恢复。
  useEffect(() => {
    if (canResume) {
      setResumeTask(true);
    }
  }, [canResume]);

  const mutation = useMutation({
    mutationFn: () =>
      recordDecision({
        teamId,
        taskId: item.taskId,
        messageId: item.messageId,
        text: text.trim(),
        resumeTask: resumeTask && canResume,
      }),
    onSuccess: (decision) => {
      void queryClient.invalidateQueries({ queryKey: queryKeys.inbox(teamId) });
      void queryClient.invalidateQueries({ queryKey: queryKeys.tasks(teamId) });
      if (decision.taskId !== null) {
        void queryClient.invalidateQueries({ queryKey: queryKeys.task(teamId, decision.taskId) });
      }
      onRecorded({
        text: decision.text,
        taskKey: decision.taskKey,
        roleName: decision.roleName,
        resumedTask: decision.resumedTask,
      });
    },
  });

  const error = touched ? validateDecisionText(text) : null;
  const canSubmit = canSubmitDecision(text, mutation.isPending);

  return (
    <Modal
      title="记录决策"
      description={
        item.taskKey === null && item.messageId === null
          ? "记录一条团队决策，写入团队活动。"
          : `关联：${item.taskKey ?? "消息"}${
              item.category === "decision" ? " · 需要拍板的消息" : ""
            }`
      }
      onClose={onClose}
      actions={
        <>
          <Button
            variant="primary"
            disabled={!canSubmit}
            loading={mutation.isPending}
            onClick={() => {
              setTouched(true);
              if (!canSubmit) {
                return;
              }
              mutation.mutate();
            }}
          >
            {decisionSubmitLabel(resumeTask && canResume)}
          </Button>
          <Button variant="neutral" onClick={onClose} disabled={mutation.isPending}>
            取消
          </Button>
        </>
      }
    >
      {mutation.isError ? <ErrorBanner error={mutation.error} /> : null}
      <TextareaField
        label="决策内容"
        required
        rows={5}
        maxLength={MAX_DECISION_TEXT}
        value={text}
        error={error ?? undefined}
        hint="写清结论与边界，后续岗位按这条决策继续推进。"
        placeholder="例如：保持登录有效期：7 天"
        onChange={(event) => {
          setText(event.target.value);
        }}
        onBlur={() => {
          setTouched(true);
        }}
      />
      {item.taskId === null ? null : (
        <label className="decision__resume">
          <input
            type="checkbox"
            checked={resumeTask && canResume}
            disabled={!canResume || mutation.isPending}
            onChange={(event) => {
              setResumeTask(event.target.checked);
            }}
          />
          <span>同时把任务恢复到 READY</span>
        </label>
      )}
      {item.taskId !== null && !canResume ? (
        <p className="decision__hint">
          {taskQuery.isError
            ? "没能确认任务状态，只记录决策。"
            : taskStatus === undefined
              ? "正在确认任务状态…"
              : "任务当前不是「受阻」，无需恢复；只记录决策。"}
        </p>
      ) : (
        <p className="decision__hint">
          {resumeTask && canResume
            ? `决策会写入团队活动记录，随后把 ${taskKey ?? "该任务"} 恢复为可执行。`
            : "决策会写入团队活动记录。"}
        </p>
      )}
    </Modal>
  );
}
