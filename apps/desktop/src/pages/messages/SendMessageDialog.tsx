import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useMemo, useState } from "react";

import { getTaskBoard, listRoles, sendMessage } from "../../api/commands.js";
import { queryKeys } from "../../api/queryKeys.js";
import {
  Button,
  ErrorBanner,
  InputField,
  Modal,
  SelectField,
  TextareaField,
} from "../../components/index.js";
import type { Message } from "../../bindings/Message.js";
import type { RoleView } from "../../bindings/RoleView.js";
import { healthLabel } from "../harness-detail/harnessFormat.js";

export interface SendMessageDialogProps {
  teamId: string;
  /** 关闭弹窗。 */
  onClose: () => void;
  /** 预选的接收岗位。 */
  defaultRoleId?: string;
  /** 预选的关联任务。 */
  defaultTaskId?: string;
  /** 发送成功：拿到新消息（页面据此跳到线程）。 */
  onSent?: (message: Message) => void;
}

/**
 * 发送岗位消息弹窗（Figma 13:364）。
 *
 * 字段与后端 `SendMessageInput` 一一对应：接收岗位必填，关联任务与主题可选；
 * 能不能发、发出去是什么状态都由后端决定，页面只做「不能为空」这类前置检查。
 */
export function SendMessageDialog({
  teamId,
  onClose,
  defaultRoleId = "",
  defaultTaskId = "",
  onSent,
}: SendMessageDialogProps) {
  const queryClient = useQueryClient();
  const [roleId, setRoleId] = useState(defaultRoleId);
  const [taskId, setTaskId] = useState(defaultTaskId);
  const [subject, setSubject] = useState("");
  const [body, setBody] = useState("");

  const roles = useQuery({
    queryKey: queryKeys.roles(teamId),
    queryFn: () => listRoles(teamId),
    enabled: teamId !== "",
  });
  // 关联任务的可选项来自看板接口（含已取消，保证预选的任务一定在列表里）。
  const board = useQuery({
    queryKey: queryKeys.taskBoard(teamId, { teamId, includeCancelled: true }),
    queryFn: () => getTaskBoard({ teamId, includeCancelled: true }),
    enabled: teamId !== "",
  });

  const roleViews = roles.data ?? [];
  const selectedRole: RoleView | undefined = roleViews.find((view) => view.role.id === roleId);

  const tasks = useMemo(
    () =>
      (board.data?.columns ?? [])
        .flatMap((column) => column.tasks)
        .sort((a, b) => a.key.localeCompare(b.key, "zh-CN")),
    [board.data],
  );

  const send = useMutation({
    mutationFn: () =>
      sendMessage({
        teamId,
        toRoleId: roleId,
        body: body.trim(),
        taskId: taskId === "" ? null : taskId,
        subject: subject.trim() === "" ? null : subject.trim(),
      }),
    onSuccess: async (message) => {
      // 线程列表与详情都挂在同一个前缀下，一起失效。
      await queryClient.invalidateQueries({ queryKey: queryKeys.messageThreads(teamId) });
      onSent?.(message);
      onClose();
    },
  });

  const canSend = roleId !== "" && body.trim() !== "" && !send.isPending;

  const bindingHint =
    selectedRole === undefined
      ? null
      : selectedRole.harness === null
        ? "该岗位未绑定 Harness：消息会排队，交付时会失败并写明原因。"
        : `当前绑定：${selectedRole.harness.name}（${healthLabel(selectedRole.harness.handshake.status)}）`;

  return (
    <Modal
      title="发送岗位消息"
      description="消息由 Teamspace 中转和路由：进入排队后交给接收岗位当前绑定的 Harness 处理，回复写回线程。"
      onClose={onClose}
      actions={
        <>
          <Button
            loading={send.isPending}
            disabled={!canSend}
            onClick={() => {
              send.mutate();
            }}
          >
            排队发送
          </Button>
          <Button variant="subtle" disabled={send.isPending} onClick={onClose}>
            取消
          </Button>
        </>
      }
    >
      {send.isError ? <ErrorBanner error={send.error} /> : null}

      <SelectField
        label="接收岗位"
        required
        value={roleId}
        onChange={(event) => {
          setRoleId(event.target.value);
        }}
      >
        <option value="">请选择接收岗位</option>
        {roleViews.map((view) => (
          <option key={view.role.id} value={view.role.id}>
            {view.role.name}（{view.role.code}）
          </option>
        ))}
      </SelectField>
      {bindingHint === null ? null : <p className="msg-dialog__hint">{bindingHint}</p>}

      <SelectField
        label="关联任务"
        value={taskId}
        onChange={(event) => {
          setTaskId(event.target.value);
        }}
      >
        <option value="">不关联任务</option>
        {tasks.map((task) => (
          <option key={task.id} value={task.id}>
            {task.key} {task.title}
          </option>
        ))}
      </SelectField>

      <InputField
        label="主题"
        value={subject}
        placeholder="留空则取正文第一行"
        hint="新建线程时使用；同一岗位与任务的消息会续用同一个线程。"
        onChange={(event) => {
          setSubject(event.target.value);
        }}
      />

      <TextareaField
        label="消息内容"
        required
        rows={6}
        value={body}
        onChange={(event) => {
          setBody(event.target.value);
        }}
      />
    </Modal>
  );
}
