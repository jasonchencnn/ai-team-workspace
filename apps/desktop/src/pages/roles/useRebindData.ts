// 换绑流程要用的数据：岗位进行中的执行、换绑期间待投递的消息、启动包预览，
// 以及换绑事件到达时的刷新。界面只消费这里的结果，不自己拼后端调用。
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { useEffect } from "react";

import {
  getRoleHandoffPreview,
  getTaskBoard,
  getThread,
  listRuns,
  listThreads,
  onNotifyEvent,
} from "../../api/commands.js";
import { queryKeys } from "../../api/queryKeys.js";
import type { Message } from "../../bindings/Message.js";
import type { MessageThread } from "../../bindings/MessageThread.js";
import type { NotifyEvent } from "../../bindings/NotifyEvent.js";
import type { Run } from "../../bindings/Run.js";
import { isPendingStatus } from "../messages/messagesUi.js";
import { isActiveRunStatus } from "./rebindUi.js";

/** 换绑相关的轮询兜底间隔（毫秒）。事件通道不可用时靠它跟上后端。 */
export const REBIND_POLL_INTERVAL_MS = 5000;

/**
 * 待生效的换绑会自己变化（当前执行结束就自动生效），
 * 事件通道不可用时靠轮询跟上：只有存在待生效换绑时才轮询，平时不打扰。
 */
export function hasPendingBinding(views: { pendingBinding: unknown }[]): boolean {
  return views.some((view) => view.pendingBinding !== null && view.pendingBinding !== undefined);
}

/** 岗位进行中的执行：执行本身 + 它在做的任务编号与标题。 */
export interface ActiveRunEntry {
  run: Run;
  taskKey: string;
  taskTitle: string;
}

/**
 * 岗位进行中的执行。
 *
 * 走「看板 → 该岗位 doing 中的任务 → 任务执行记录」这条路，
 * 而不是岗位工作台的 recentRuns：那里只保留最近 5 条，活动执行可能被挤掉，
 * 而「有没有在跑的活」直接决定换绑方式，必须算准。
 */
export function useRoleActiveRuns(teamId: string, roleId: string) {
  return useQuery<ActiveRunEntry[]>({
    queryKey: queryKeys.roleActiveRuns(teamId, roleId),
    enabled: teamId !== "" && roleId !== "",
    refetchInterval: REBIND_POLL_INTERVAL_MS,
    queryFn: async () => {
      const board = await getTaskBoard({ teamId });
      const doing = board.columns
        .filter((column) => column.status === "doing")
        .flatMap((column) => column.tasks)
        .filter((task) => task.owner.id === roleId);
      const entries: ActiveRunEntry[] = [];
      for (const task of doing) {
        const runs = await listRuns(task.id);
        for (const run of runs) {
          if (isActiveRunStatus(run.status)) {
            entries.push({ run, taskKey: task.key, taskTitle: task.title });
          }
        }
      }
      return entries.sort((a, b) => a.run.startedAt.localeCompare(b.run.startedAt));
    },
  });
}

/** 待投递消息：一条待投递消息与它所在的线程。 */
export interface PendingMessage {
  message: Message;
  thread: MessageThread;
}

/**
 * 换绑期间待投递的消息：发往该岗位、状态还是排队 / 投递中的消息。
 *
 * 用现成的消息 API 按岗位筛：先列出该岗位参与的线程，再取每个线程里的消息。
 * 消息上带 `bindingHarnessId`——那是入队那一刻的绑定快照，
 * 换绑生效后这些消息才会按新绑定投递。
 */
export function useRolePendingMessages(teamId: string, roleId: string, enabled: boolean) {
  return useQuery<PendingMessage[]>({
    queryKey: queryKeys.rolePendingMessages(teamId, roleId),
    enabled: enabled && teamId !== "" && roleId !== "",
    refetchInterval: REBIND_POLL_INTERVAL_MS,
    queryFn: async () => {
      const threads = await listThreads({ teamId, roleId });
      const details = await Promise.all(threads.map((thread) => getThread(thread.id)));
      const pending: PendingMessage[] = [];
      for (const detail of details) {
        for (const message of detail.messages) {
          if (message.toRoleId === roleId && isPendingStatus(message.status)) {
            pending.push({ message, thread: detail.thread });
          }
        }
      }
      return pending.sort((a, b) => a.message.createdAt.localeCompare(b.message.createdAt));
    },
  });
}

/**
 * 启动包预览：岗位下一次执行会附上的交接内容。
 * 为空字符串表示不会附加交接（当前 Harness 已在此岗位执行过）。
 */
export function useRoleHandoffPreview(teamId: string, roleId: string) {
  return useQuery<string>({
    queryKey: queryKeys.roleHandoffPreview(teamId, roleId),
    enabled: teamId !== "" && roleId !== "",
    queryFn: () => getRoleHandoffPreview(roleId),
  });
}

/**
 * 换绑相关的刷新：订阅 `teamspace:notify`，收到岗位换绑通知（待生效 / 已生效 / 已撤销）
 * 就让岗位与团队概览重新取一次；消息状态变化时刷新待投递列表（投递完就从列表里消失）。
 */
export function useRebindRefresh(teamId: string): void {
  const queryClient = useQueryClient();

  useEffect(() => {
    if (teamId === "") {
      return;
    }
    let dispose: (() => void) | undefined;
    let cancelled = false;

    const handle = (event: NotifyEvent) => {
      if (event.type === "notification") {
        if (event.notification.teamId !== teamId) {
          return;
        }
        if (event.notification.category !== "role_rebind") {
          return;
        }
        void queryClient.invalidateQueries({ queryKey: queryKeys.roles(teamId) });
        void queryClient.invalidateQueries({ queryKey: queryKeys.teamOverview(teamId) });
      }
      if (event.type === "message_status" && event.message.teamId === teamId) {
        void queryClient.invalidateQueries({ queryKey: queryKeys.messageThreads(teamId) });
      }
    };

    void onNotifyEvent(handle)
      .then((unlisten) => {
        if (cancelled) {
          unlisten();
          return;
        }
        dispose = unlisten;
      })
      .catch((error: unknown) => {
        console.error("订阅通知事件失败，已改用轮询兜底：", error);
      });

    return () => {
      cancelled = true;
      dispose?.();
    };
  }, [queryClient, teamId]);
}
