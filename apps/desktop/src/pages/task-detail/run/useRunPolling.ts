// 执行进行中的轮询兜底（D23）。
//
// 事件是首选通道，但不是唯一通道：capability 未放行事件权限时 listen 会被 Tauri 拒绝，
// 界面就会一直停在旧状态。所以任务详情在有活动执行时按固定间隔刷新记录与进度，
// 事件正常时也保留（间隔很短，开销可以忽略），任何一条通道失效都不会卡住界面。
import { useQueryClient } from "@tanstack/react-query";
import { useEffect } from "react";

import { queryKeys } from "../../../api/queryKeys.js";
import type { Run } from "../../../bindings/Run.js";
import { isRunActive } from "./useRunEvents.js";

/** 轮询间隔：执行进行中每 3 秒一次，执行结束后停止。 */
export const RUN_POLL_INTERVAL_MS = 3000;

/** 是否该轮询：只有活动执行（queued / running / waiting_permission）才需要。 */
export function shouldPollRun(run: Run | null): boolean {
  return isRunActive(run);
}

export interface RunPoller {
  /** 开始按间隔触发 tick；重复调用不会叠加定时器。 */
  start: () => void;
  /** 停止并清掉定时器。 */
  stop: () => void;
}

/**
 * 定时触发器。抽成不依赖 React 的小对象，执行开合的判定（活动时开、结束时关）
 * 可以在单元测试里直接验证，React 侧只负责把生命周期接上来。
 */
export function createRunPoller(
  tick: () => void,
  intervalMs: number = RUN_POLL_INTERVAL_MS,
): RunPoller {
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
 * 任务详情的记录轮询：执行进行中定时失效执行列表、任务详情与待验收入口查询。
 * 这三个 key 都挂在 queryKeys.task 的前缀下，一次失效全部刷新；执行结束后停止。
 */
export function useRunRecordPolling(teamId: string, taskId: string, run: Run | null): void {
  const queryClient = useQueryClient();
  const polling = shouldPollRun(run);

  useEffect(() => {
    if (!polling) {
      return;
    }
    const poller = createRunPoller(() => {
      void queryClient.invalidateQueries({ queryKey: queryKeys.task(teamId, taskId) });
    });
    poller.start();
    return () => {
      poller.stop();
    };
  }, [polling, queryClient, teamId, taskId]);
}
