// 消息事件订阅。
//
// 内核的通知通道（teamspace:notify）推两类事件：应用内通知与消息状态变化。
// 岗位通信页只关心后者（type = "message_status"）：消息送出、进入投递中、已回复、失败或过期时，
// 内核都会推一条事件，页面据此立刻更新线程里的状态。
//
// 事件不是唯一通道：订阅失败（权限被拒、通道不可用）时记录日志，页面靠轮询兜底
// （useMessagePolling）跟随状态——与执行事件（useRunEvents.ts）的处理方式一致。
import { useQueryClient } from "@tanstack/react-query";
import { useEffect } from "react";

import { onNotifyEvent } from "../../api/commands.js";
import { queryKeys } from "../../api/queryKeys.js";
import type { NotifyEvent } from "../../bindings/NotifyEvent.js";
import { messageStatusChange } from "./messageEvents.js";

/**
 * 订阅通知事件，返回取消订阅的函数。
 * 订阅失败（权限被拒、通道不可用）时记录日志并回调 onFailure，由调用方切到轮询兜底；
 * 组件卸载后才拿到的 unlisten 会立刻取消，StrictMode 的重复挂载也不会泄漏订阅。
 */
export function subscribeMessageEvents(
  handle: (event: NotifyEvent) => void,
  onFailure: (error: unknown) => void = () => undefined,
): () => void {
  let dispose: (() => void) | undefined;
  let cancelled = false;

  void onNotifyEvent(handle)
    .then((unlisten) => {
      if (cancelled) {
        unlisten();
        return;
      }
      dispose = unlisten;
    })
    .catch((error: unknown) => {
      console.error("订阅消息事件失败，已改用定时刷新兜底：", error);
      if (!cancelled) {
        onFailure(error);
      }
    });

  return () => {
    cancelled = true;
    dispose?.();
  };
}

/**
 * 页面级订阅：消息状态变化时让线程列表与该线程的详情失效，界面立刻跟上。
 * 其他团队的消息与通知类事件不关这里的事，直接忽略。
 */
export function useMessageEventInvalidation(teamId: string): void {
  const queryClient = useQueryClient();

  useEffect(() => {
    const handle = (event: NotifyEvent) => {
      const change = messageStatusChange(event, teamId);
      if (change === null) {
        return;
      }
      // 该线程的详情与线程列表（最近状态与时间）一起刷新。
      void queryClient.invalidateQueries({
        queryKey: queryKeys.messageThread(teamId, change.threadId),
      });
      void queryClient.invalidateQueries({
        queryKey: queryKeys.messageThreadLists(teamId),
      });
    };

    return subscribeMessageEvents(handle);
  }, [queryClient, teamId]);
}
