// 执行事件订阅：整个应用订阅一次 RUN_EVENT，把内核推送的执行变化同步到 React Query 缓存，
// 并为任务详情提供该次执行的进度增量。
//
// 约定（与内核一致）：
// - `run_changed`：替换缓存中的该 run，并让任务详情、看板、团队概览与岗位工作台查询失效；
// - `run_updated`：按 update.id 合并进进度列表，按 seq 排序；
// - `permission_requested`：执行状态已是 waiting_permission，由 run_changed 携带的
//   `pendingPermission` 驱动弹窗，这里不重复处理。
import { useQueryClient } from "@tanstack/react-query";
import { useEffect, useRef, useState } from "react";

import { listRunUpdates, onRunEvent } from "../../../api/commands.js";
import { queryKeys } from "../../../api/queryKeys.js";
import type { Run } from "../../../bindings/Run.js";
import type { RunEvent } from "../../../bindings/RunEvent.js";
import type { RunUpdate } from "../../../bindings/RunUpdate.js";

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
 * 应用级订阅：任何执行状态变化都让相关查询失效，让看板、任务列表、岗位工作台跟随更新。
 * 放在 AppShell 里调用一次即可。
 */
export function useRunEventInvalidation(): void {
  const queryClient = useQueryClient();

  useEffect(() => {
    let dispose: (() => void) | undefined;
    let cancelled = false;

    const handle = (event: RunEvent) => {
      if (event.type !== "run_changed") {
        return;
      }
      const { teamId, taskId, id } = event.run;
      // 缓存里已有的该 run 直接替换，详情页立即拿到新状态（pendingPermission 等）。
      queryClient.setQueryData(queryKeys.runs(teamId, taskId), (previous: Run[] | undefined) =>
        previous === undefined
          ? previous
          : previous.map((run) => (run.id === id ? event.run : run)),
      );
      void queryClient.invalidateQueries({ queryKey: queryKeys.task(teamId, taskId) });
      void queryClient.invalidateQueries({ queryKey: queryKeys.tasks(teamId) });
      void queryClient.invalidateQueries({ queryKey: queryKeys.teamOverview(teamId) });
      void queryClient.invalidateQueries({ queryKey: queryKeys.roles(teamId) });
    };

    void onRunEvent(handle).then((unlisten) => {
      if (cancelled) {
        unlisten();
        return;
      }
      dispose = unlisten;
    });

    return () => {
      cancelled = true;
      dispose?.();
    };
  }, [queryClient]);
}

interface RunUpdatesState {
  updates: RunUpdate[];
  /** 仍在补漏（hasMore 时继续翻页）。 */
  loading: boolean;
  error: unknown;
}

const EMPTY: RunUpdatesState = { updates: [], loading: false, error: null };

/**
 * 某次执行的进度列表：初次打开从 listRunUpdates 从头读取，之后由 run_updated 增量合并；
 * 同一次打开期间 hasMore 为 true 时立即再取一页，直到读完。
 */
export function useRunUpdates(runId: string | null): RunUpdatesState {
  const [state, setState] = useState<RunUpdatesState>(EMPTY);
  const cursorRef = useRef<string | null>(null);
  const entriesRef = useRef<Map<string, RunUpdate>>(new Map());

  // 切换执行时清空上一次的内容。
  useEffect(() => {
    entriesRef.current = new Map();
    cursorRef.current = null;
    setState(EMPTY);
  }, [runId]);

  useEffect(() => {
    if (runId === null) {
      return;
    }
    let dispose: (() => void) | undefined;
    let cancelled = false;

    const publish = () => {
      const updates = [...entriesRef.current.values()].sort((a, b) => a.seq - b.seq);
      setState({ updates, loading: false, error: null });
    };

    /** 增量补漏：从上次游标继续，hasMore 时立刻再取一页。 */
    const pull = async () => {
      setState((prev) => ({ ...prev, loading: true }));
      try {
        for (;;) {
          const page = await listRunUpdates({ runId, cursor: cursorRef.current, limit: 200 });
          if (cancelled) {
            return;
          }
          for (const update of page.updates) {
            entriesRef.current.set(update.id, update);
          }
          cursorRef.current = page.nextCursor;
          if (!page.hasMore) {
            break;
          }
        }
        publish();
      } catch (error) {
        if (!cancelled) {
          setState((prev) => ({ ...prev, loading: false, error }));
        }
      }
    };

    const handle = (event: RunEvent) => {
      if (event.type !== "run_updated" || event.runId !== runId) {
        return;
      }
      entriesRef.current.set(event.update.id, event.update);
      publish();
    };

    void pull();
    void onRunEvent(handle).then((unlisten) => {
      if (cancelled) {
        unlisten();
        return;
      }
      dispose = unlisten;
    });

    return () => {
      cancelled = true;
      dispose?.();
    };
  }, [runId]);

  return runId === null ? EMPTY : state;
}
