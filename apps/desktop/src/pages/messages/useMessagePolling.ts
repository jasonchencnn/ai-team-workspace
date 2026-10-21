// 岗位消息的轮询兜底。
//
// 消息事件由内核推送（事件通道正在实现，见 useMessageEvents.ts 的预留位置）。在那之前，
// 页面在有「排队中」「投递中」消息时按固定间隔刷新线程，等消息进入终态就停下来——
// 与执行详情的轮询兜底（D23）做法一致：事件不是唯一通道。
import { useQueryClient } from "@tanstack/react-query";
import { useEffect } from "react";

import { queryKeys } from "../../api/queryKeys.js";

/** 轮询间隔：3 秒一次（与执行详情的轮询兜底一致）。 */
export const MESSAGE_POLL_INTERVAL_MS = 3000;

/**
 * 定时触发器。抽成不依赖 React 的小对象，开合的判定可以在单元测试里直接验证，
 * React 侧只负责把生命周期接上来。
 */
export function createMessagePoller(
  tick: () => void,
  intervalMs: number = MESSAGE_POLL_INTERVAL_MS,
): { start: () => void; stop: () => void } {
  let timer: ReturnType<typeof setInterval> | null = null;
  return {
    start: () => {
      if (timer !== null) {
        return;
      }
      timer = setInterval(tick, intervalMs);
    },
    stop: () => {
      if (timer === null) {
        return;
      }
      clearInterval(timer);
      timer = null;
    },
  };
}

/**
 * 消息轮询：有排队中或投递中的消息时，定期让线程列表与线程详情失效并重新取。
 * 线程列表与详情都挂在 `queryKeys.messageThreads(teamId)` 前缀下，一次失效全部刷新。
 */
export function useMessagePolling(teamId: string, pending: boolean): void {
  const queryClient = useQueryClient();

  useEffect(() => {
    if (!pending) {
      return;
    }
    const poller = createMessagePoller(() => {
      void queryClient.invalidateQueries({ queryKey: queryKeys.messageThreads(teamId) });
    });
    poller.start();
    return () => {
      poller.stop();
    };
  }, [pending, queryClient, teamId]);
}
