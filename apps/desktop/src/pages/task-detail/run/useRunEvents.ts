// 执行事件订阅：整个应用订阅一次 RUN_EVENT，把内核推送的执行变化同步到 React Query 缓存，
// 并为任务详情提供该次执行的进度增量。
//
// 约定（与内核一致）：
// - `run_changed`：替换缓存中的该 run，并让任务详情、看板、团队概览与岗位工作台查询失效；
// - `run_updated`：按 update.id 合并进进度列表，按 seq 排序；
// - `permission_requested`：执行状态已是 waiting_permission，由 run_changed 携带的
//   `pendingPermission` 驱动弹窗，这里不重复处理。
//
// 事件不是唯一通道（D23）：capability 未放行事件权限时 listen 会被 Tauri 拒绝，
// 订阅失败必须记录日志并交给轮询兜底（useRunPolling / useRunMonitor），不能静默吞掉。
import { useQueryClient } from "@tanstack/react-query";
import { useEffect } from "react";

import { onRunEvent } from "../../../api/commands.js";
import { queryKeys } from "../../../api/queryKeys.js";
import type { Run } from "../../../bindings/Run.js";
import type { RunEvent } from "../../../bindings/RunEvent.js";

/** 活跃的执行状态（仍在进行）：执行区据此显示取消、接管等操作。 */
export function isRunActive(run: Run | null): boolean {
  return (
    run !== null &&
    (run.status === "queued" || run.status === "running" || run.status === "waiting_permission")
  );
}

/** 执行已结束（含失败、取消、中断）。 */
export function isRunFinished(run: Run | null): boolean {
  return run !== null && !isRunActive(run);
}

/**
 * 订阅执行事件，返回取消订阅的函数。
 * 订阅失败（权限被拒、通道不可用）时记录日志并回调 onFailure，由调用方切到轮询兜底；
 * 组件卸载后才拿到的 unlisten 会立刻取消，StrictMode 的重复挂载也不会泄漏订阅。
 */
export function subscribeRunEvents(
  handle: (event: RunEvent) => void,
  onFailure: (error: unknown) => void = () => undefined,
): () => void {
  let dispose: (() => void) | undefined;
  let cancelled = false;

  void onRunEvent(handle)
    .then((unlisten) => {
      if (cancelled) {
        unlisten();
        return;
      }
      dispose = unlisten;
    })
    .catch((error: unknown) => {
      console.error("订阅执行事件失败，已改用定时刷新兜底：", error);
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
 * 应用级订阅：任何执行状态变化都让相关查询失效，让看板、任务列表、岗位工作台跟随更新。
 * 放在 AppShell 里调用一次即可。
 */
export function useRunEventInvalidation(): void {
  const queryClient = useQueryClient();

  useEffect(() => {
    const handle = (event: RunEvent) => {
      if (event.type !== "run_changed") {
        return;
      }
      const { teamId, taskId, id } = event.run;
      const known = (queryClient.getQueryData<Run[]>(queryKeys.runs(teamId, taskId)) ?? []).some(
        (run) => run.id === id,
      );
      if (known) {
        // 缓存里已有的该 run 直接替换，详情页立即拿到新状态（pendingPermission 等）。
        queryClient.setQueryData(queryKeys.runs(teamId, taskId), (previous: Run[] | undefined) =>
          previous === undefined
            ? previous
            : previous.map((run) => (run.id === id ? event.run : run)),
        );
      } else {
        // 刚创建的执行还不在列表里（setQueryData 只会替换已有项），让 list_runs 重新取。
        void queryClient.invalidateQueries({ queryKey: queryKeys.runs(teamId, taskId) });
      }
      // 任务详情、执行列表与待验收入口都挂在这个前缀下，一起失效。
      void queryClient.invalidateQueries({ queryKey: queryKeys.task(teamId, taskId) });
      void queryClient.invalidateQueries({ queryKey: queryKeys.tasks(teamId) });
      void queryClient.invalidateQueries({ queryKey: queryKeys.teamOverview(teamId) });
      void queryClient.invalidateQueries({ queryKey: queryKeys.roles(teamId) });
    };

    return subscribeRunEvents(handle);
  }, [queryClient]);
}
