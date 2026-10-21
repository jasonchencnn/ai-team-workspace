// 待答复权限的就地答复：取执行记录里的待确认提示，提交选中的选项。
// 与任务详情共用同一个答复接口（`respond_permission`），内核按既有规则判定范围。
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";

import { getRun, respondPermission } from "../../api/commands.js";
import { queryKeys } from "../../api/queryKeys.js";
import type { PermissionPrompt } from "../../bindings/PermissionPrompt.js";
import type { Run } from "../../bindings/Run.js";

export interface PermissionDecision {
  /** 执行记录：取回后用于显示发起岗位。 */
  run: Run | null;
  /** 待确认的权限请求；执行记录还没取回或没有待确认的请求时为 null。 */
  prompt: PermissionPrompt | null;
  /** 执行记录还在取。 */
  loading: boolean;
  /** 提交中。 */
  submitting: boolean;
  /** 提交出错。 */
  error: unknown;
  /** 提交选中的选项：只传 optionId。 */
  respond: (optionId: string) => void;
}

/**
 * 就地答复一个待答复权限。
 *
 * `runId` 为 null 时不取数据（待我处理里没有权限类事项时保持空）。
 */
export function usePermissionDecision(teamId: string, runId: string | null): PermissionDecision {
  const queryClient = useQueryClient();

  const run = useQuery({
    queryKey: queryKeys.permissionRun(teamId, runId ?? ""),
    queryFn: () => {
      if (runId === null) {
        throw new Error("缺少执行 ID");
      }
      return getRun(runId);
    },
    enabled: runId !== null,
  });

  const respond = useMutation({
    mutationFn: async (optionId: string) => {
      const prompt = run.data?.pendingPermission ?? null;
      if (runId === null || prompt === null) {
        throw new Error("这个权限请求已经不在待确认状态");
      }
      return respondPermission({ runId, requestId: prompt.requestId, optionId });
    },
    onSuccess: async () => {
      // 答复后执行继续跑，待我处理与执行记录都要跟上。
      await queryClient.invalidateQueries({ queryKey: queryKeys.inbox(teamId) });
      await queryClient.invalidateQueries({
        queryKey: queryKeys.permissionRun(teamId, runId ?? ""),
      });
    },
  });

  return {
    run: run.data ?? null,
    prompt: run.data?.pendingPermission ?? null,
    loading: run.isPending,
    submitting: respond.isPending,
    error: respond.error,
    respond: (optionId: string) => {
      respond.mutate(optionId);
    },
  };
}
