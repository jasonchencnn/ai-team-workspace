// 消息事件订阅。
//
// 内核的消息事件推送由另一个任务实现（事件名与这里约定一致）。订阅已经接好：内核一旦开始
// 推送消息状态变化，页面就会立刻失效线程查询；在这之前没有事件到达，页面靠轮询兜底
// （useMessagePolling）跟随状态——与执行事件（useRunEvents.ts）的处理方式一致：
// 事件不是唯一通道，订阅失败要记日志并由轮询顶上，不能静默吞掉。
import { useQueryClient } from "@tanstack/react-query";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { useEffect } from "react";

import { queryKeys } from "../../api/queryKeys.js";

/** 消息事件名。 */
export const MESSAGE_EVENT = "teamspace:message";

/**
 * 订阅消息事件，返回取消订阅的函数。
 * 订阅失败（权限被拒、通道不可用）时记录日志并回调 onFailure，由调用方切到轮询兜底；
 * 组件卸载后才拿到的 unlisten 会立刻取消，StrictMode 的重复挂载也不会泄漏订阅。
 */
export function subscribeMessageEvents(
  handle: (event: unknown) => void,
  onFailure: (error: unknown) => void = () => undefined,
): () => void {
  let dispose: UnlistenFn | undefined;
  let cancelled = false;

  void listen(MESSAGE_EVENT, (event) => {
    handle(event.payload);
  })
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
 * 应用级订阅：消息状态变化时让线程列表与线程详情失效。
 * 放在岗位通信页里调用一次即可。
 */
export function useMessageEventInvalidation(teamId: string): void {
  const queryClient = useQueryClient();

  useEffect(() => {
    return subscribeMessageEvents(() => {
      void queryClient.invalidateQueries({ queryKey: queryKeys.messageThreads(teamId) });
    });
  }, [queryClient, teamId]);
}
