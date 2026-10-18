// 某次执行的监控：进度列表（事件增量 + 轮询兜底）、实时通道是否可用、
// 以及最近一次出现新动静的时间——心跳指示（「监控中」「最近更新 N 秒前」）都用它。
//
// 两条通道并行（D23）：
// - 事件：`run_updated` 到达就合并，按 update.id 去重；
// - 轮询：执行进行中每 3 秒从游标继续增量读一次，事件通道不可用时由它顶上。
// 任何一次「新内容」出现都会刷新 lastActivityAt；一轮轮询什么都没读到不算有新动静，
// 否则每 3 秒被轮询自己抹平，长时间没有进度的提示就永远不会出现。
import { useEffect, useRef, useState } from "react";

import { listRunUpdates } from "../../../api/commands.js";
import type { Run } from "../../../bindings/Run.js";
import type { RunEvent } from "../../../bindings/RunEvent.js";
import type { RunUpdate } from "../../../bindings/RunUpdate.js";
import { subscribeRunEvents } from "./useRunEvents.js";
import { createRunPoller, shouldPollRun } from "./useRunPolling.js";

export interface RunMonitorState {
  /** 进度条目，按 seq 排序。 */
  updates: RunUpdate[];
  /** 首次读取或补漏中。轮询的静默刷新不置位，免得进度流每 3 秒闪一次「补齐中」。 */
  loading: boolean;
  error: unknown;
  /** 实时事件通道是否可用：订阅失败为 false，界面据此提示「已改为定时刷新」。 */
  realtime: boolean;
  /** 最近一次收到事件或出现新进度的时间（毫秒时间戳）；还没有过为 null。 */
  lastActivityAt: number | null;
}

const EMPTY: RunMonitorState = {
  updates: [],
  loading: false,
  error: null,
  realtime: true,
  lastActivityAt: null,
};

export function useRunMonitor(run: Run | null): RunMonitorState {
  const runId = run?.id ?? null;
  const active = shouldPollRun(run);
  const [state, setState] = useState<RunMonitorState>(EMPTY);
  const [lastActivityAt, setLastActivityAt] = useState<number | null>(null);
  const cursorRef = useRef<string | null>(null);
  const entriesRef = useRef<Map<string, RunUpdate>>(new Map());
  const pullRef = useRef<(() => Promise<void>) | null>(null);

  // 执行记录变化的指纹：状态、结束时间、待确认请求任一变化都算有新动静
  // （轮询模式下看不到事件，只能从刷新回来的记录上判断）。
  const signature =
    run === null
      ? ""
      : `${run.id}:${run.status}:${run.endedAt ?? ""}:${run.pendingPermission?.requestId ?? ""}`;
  const signatureRef = useRef(signature);

  // 切换执行时清空上一次的内容。只依赖 runId：执行状态变化必须留住已有的进度条目。
  useEffect(() => {
    entriesRef.current = new Map();
    cursorRef.current = null;
    signatureRef.current = signature;
    setLastActivityAt(null);
    setState(EMPTY);
  }, [runId]);
  useEffect(() => {
    if (runId === null) {
      return;
    }
    let cancelled = false;
    let firstLoad = true;

    const publish = () => {
      const updates = [...entriesRef.current.values()].sort((a, b) => a.seq - b.seq);
      setState((previous) => ({ ...previous, updates, loading: false, error: null }));
    };

    /** 增量补漏：从上次游标继续，hasMore 时立刻再取一页。 */
    const pull = async (initial: boolean): Promise<void> => {
      if (initial) {
        setState((previous) => ({ ...previous, loading: true }));
      }
      let added = false;
      try {
        for (;;) {
          const page = await listRunUpdates({ runId, cursor: cursorRef.current, limit: 200 });
          if (cancelled) {
            return;
          }
          for (const update of page.updates) {
            if (!entriesRef.current.has(update.id)) {
              added = true;
            }
            entriesRef.current.set(update.id, update);
          }
          cursorRef.current = page.nextCursor;
          if (!page.hasMore) {
            break;
          }
        }
        publish();
        if (added || firstLoad) {
          firstLoad = false;
          setLastActivityAt(Date.now());
        }
      } catch (error) {
        if (!cancelled) {
          setState((previous) => ({ ...previous, loading: false, error }));
        }
      }
    };
    pullRef.current = () => pull(false);

    const handle = (event: RunEvent) => {
      const mine = event.type === "run_changed" ? event.run.id === runId : event.runId === runId;
      if (!mine) {
        return;
      }
      if (event.type === "run_updated") {
        entriesRef.current.set(event.update.id, event.update);
        publish();
      }
      setLastActivityAt(Date.now());
    };

    void pull(true);
    const unsubscribe = subscribeRunEvents(handle, () => {
      // 订阅失败不静默：记录已由 subscribeRunEvents 打出，这里把状态告诉界面。
      setState((previous) => ({ ...previous, realtime: false }));
    });

    return () => {
      cancelled = true;
      pullRef.current = null;
      unsubscribe();
    };
  }, [runId]);

  // 执行记录变了（新状态、新的待确认请求）也算有新动静。
  useEffect(() => {
    if (signature === "" || signatureRef.current === signature) {
      return;
    }
    signatureRef.current = signature;
    setLastActivityAt(Date.now());
  }, [signature]);

  // 轮询兜底：执行进行中每 3 秒增量读一次进度，执行结束后停止。
  useEffect(() => {
    if (runId === null || !active) {
      return;
    }
    const poller = createRunPoller(() => {
      void pullRef.current?.();
    });
    poller.start();
    return () => {
      poller.stop();
    };
  }, [runId, active]);

  if (runId === null) {
    return EMPTY;
  }
  return { ...state, lastActivityAt };
}
