// 换绑流程要用的数据：岗位进行中的执行、换绑期间待投递的消息、启动包预览，
// 以及换绑事件到达时的刷新。界面只消费这里的结果，不自己拼后端调用。
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { useEffect } from "react";

import {
  getRoleHandoffPreview,
  listActiveRuns,
  listRoleMessages,
  onNotifyEvent,
} from "../../api/commands.js";
import { queryKeys } from "../../api/queryKeys.js";
import type { ActiveRunSummary } from "../../bindings/ActiveRunSummary.js";
import type { NotifyEvent } from "../../bindings/NotifyEvent.js";
import type { RoleMessage } from "../../bindings/RoleMessage.js";

/** 换绑相关的轮询兜底间隔（毫秒）。事件通道不可用时靠它跟上后端。 */
export const REBIND_POLL_INTERVAL_MS = 5000;

/**
 * 待生效的换绑会自己变化（当前执行结束就自动生效），
 * 事件通道不可用时靠轮询跟上：只有存在待生效换绑时才轮询，平时不打扰。
 */
export function hasPendingBinding(views: { pendingBinding: unknown }[]): boolean {
  return views.some((view) => view.pendingBinding !== null && view.pendingBinding !== undefined);
}

/**
 * 岗位进行中的执行。
 *
 * 直接问内核「这个岗位现在有没有在跑的活」：进行中的判定、任务编号与标题、次数与
 * Harness 实例都由后端给，前端不再自己列状态白名单，也不用逐个任务去查执行。
 */
export function useRoleActiveRuns(teamId: string, roleId: string) {
  return useQuery<ActiveRunSummary[]>({
    queryKey: queryKeys.roleActiveRuns(teamId, roleId),
    enabled: teamId !== "" && roleId !== "",
    refetchInterval: REBIND_POLL_INTERVAL_MS,
    queryFn: () => listActiveRuns(roleId),
  });
}

/**
 * 换绑期间待投递的消息：发往该岗位、状态还是排队 / 投递中的消息。
 *
 * 后端一条查询取回，带上线程主题与入队那一刻的绑定实例名（`bindingHarnessName`）。
 * 消息上的绑定快照说明这条消息是按哪个绑定入队的，换绑生效后才会按新绑定投递。
 */
export function useRolePendingMessages(teamId: string, roleId: string, enabled: boolean) {
  return useQuery<RoleMessage[]>({
    queryKey: queryKeys.rolePendingMessages(teamId, roleId),
    enabled: enabled && teamId !== "" && roleId !== "",
    refetchInterval: REBIND_POLL_INTERVAL_MS,
    queryFn: () => listRoleMessages({ roleId }),
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
