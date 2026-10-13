// 任务详情的执行状态：读取历次执行、选择当前查看的那次，并统一发起执行操作
// （开始、取消、续接、标记失败、人工接管、打开执行目录、回应权限、提交验收）。
// 状态变化后由 useRunEventInvalidation 触发的查询失效来刷新，这里不重复轮询。
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useEffect, useMemo, useState } from "react";

import {
  cancelRun,
  changeTaskStatus,
  listRuns,
  markRunFailed,
  openRunDir,
  respondPermission,
  resumeRun,
  startRun,
  takeoverRun,
} from "../../../api/commands.js";
import { queryKeys } from "../../../api/queryKeys.js";
import type { Run } from "../../../bindings/Run.js";
import type { TaskStatus } from "../../../bindings/TaskStatus.js";
import { isRunActive } from "./useRunEvents.js";

export interface TaskRunsApi {
  runs: Run[];
  /** 当前查看的执行；从未执行过时为 null。 */
  run: Run | null;
  loading: boolean;
  /** 用户点了历史记录里的某次执行后固定查看它；null 表示跟随当前执行。 */
  selectRun: (runId: string) => void;
  /** 是否有任何执行操作在进行中。 */
  busy: boolean;
  error: unknown;
  startRun: () => void;
  cancelRun: () => void;
  resumeRun: () => void;
  markRunFailed: () => void;
  takeoverRun: () => void;
  openRunDir: () => void;
  respondPermission: (requestId: string, optionId: string) => void;
  submitReview: () => void;
  execDirPending: boolean;
}

export function useTaskRuns(teamId: string, taskId: string): TaskRunsApi {
  const queryClient = useQueryClient();
  const [pinnedRunId, setPinnedRunId] = useState<string | null>(null);

  const runsQuery = useQuery({
    queryKey: queryKeys.runs(teamId, taskId),
    queryFn: () => listRuns(taskId),
    enabled: teamId !== "" && taskId !== "",
  });
  const runs = useMemo(() => runsQuery.data ?? [], [runsQuery.data]);

  const activeRun = runs.find((item) => isRunActive(item)) ?? null;

  // 任务切换时取消固定，回到「跟随当前执行」。
  useEffect(() => {
    setPinnedRunId(null);
  }, [teamId, taskId]);

  const run = useMemo(() => {
    if (pinnedRunId !== null) {
      const pinned = runs.find((item) => item.id === pinnedRunId);
      if (pinned !== undefined) {
        return pinned;
      }
    }
    return activeRun ?? runs[0] ?? null;
  }, [pinnedRunId, runs, activeRun]);

  /** 执行操作成功后刷新执行列表；事件推送到达前先拿到一次新状态。 */
  const refresh = async () => {
    await queryClient.invalidateQueries({ queryKey: queryKeys.runs(teamId, taskId) });
    await queryClient.invalidateQueries({ queryKey: queryKeys.task(teamId, taskId) });
  };

  const action = useMutation({
    mutationFn: async (input: {
      kind: "cancel" | "resume" | "markFailed" | "takeover";
      run: Run;
    }) => {
      switch (input.kind) {
        case "cancel":
          return cancelRun(input.run.id);
        case "resume":
          return resumeRun(input.run.id);
        case "markFailed":
          return markRunFailed(input.run.id);
        case "takeover":
          return takeoverRun(input.run.id);
      }
    },
    onSuccess: () => {
      void refresh();
    },
  });

  const start = useMutation({
    mutationFn: () => startRun(taskId),
    onSuccess: async () => {
      // 新的一次执行自动成为当前查看对象。
      setPinnedRunId(null);
      await refresh();
    },
  });

  const permission = useMutation({
    mutationFn: (input: { requestId: string; optionId: string }) =>
      respondPermission({
        runId: run?.id ?? "",
        requestId: input.requestId,
        optionId: input.optionId,
      }),
    onSuccess: () => {
      void refresh();
    },
  });

  const review = useMutation({
    mutationFn: (to: TaskStatus) => changeTaskStatus({ taskId, to, reason: null }),
    onSuccess: () => {
      void refresh();
    },
  });

  const execDir = useMutation({
    mutationFn: () => openRunDir(run?.id ?? ""),
  });

  const selectRun = (runId: string) => {
    // 点当前正在执行的那次等于取消固定，继续跟随。
    setPinnedRunId(runId === activeRun?.id ? null : runId);
  };

  return {
    runs,
    run,
    loading: runsQuery.isPending,
    selectRun,
    busy: action.isPending || start.isPending || review.isPending,
    error: action.error ?? start.error ?? review.error ?? null,
    startRun: () => {
      action.reset();
      start.mutate();
    },
    cancelRun: () => {
      action.reset();
      if (run !== null) {
        action.mutate({ kind: "cancel", run });
      }
    },
    resumeRun: () => {
      action.reset();
      if (run !== null) {
        action.mutate({ kind: "resume", run });
      }
    },
    markRunFailed: () => {
      action.reset();
      if (run !== null) {
        action.mutate({ kind: "markFailed", run });
      }
    },
    takeoverRun: () => {
      action.reset();
      if (run !== null) {
        action.mutate({ kind: "takeover", run });
      }
    },
    openRunDir: () => {
      execDir.mutate();
    },
    respondPermission: (requestId: string, optionId: string) => {
      permission.mutate({ requestId, optionId });
    },
    submitReview: () => {
      review.mutate("review");
    },
    execDirPending: execDir.isPending,
  };
}
