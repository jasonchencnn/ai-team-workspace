import { useMutation, useQueryClient } from "@tanstack/react-query";

import {
  archiveRole,
  bindRoleHarness,
  cancelPendingRebind,
  rebindRole,
  unbindRoleHarness,
  updateRole,
} from "../../api/commands.js";
import { queryKeys } from "../../api/queryKeys.js";
import type { RebindMode } from "../../bindings/RebindMode.js";

/** 岗位相关的写操作；成功后失效岗位、团队概览与实例列表。 */
export function useRoleMutations(teamId: string) {
  const queryClient = useQueryClient();

  const invalidate = () => {
    void queryClient.invalidateQueries({ queryKey: queryKeys.roles(teamId) });
    void queryClient.invalidateQueries({ queryKey: queryKeys.teamOverview(teamId) });
    void queryClient.invalidateQueries({ queryKey: queryKeys.harnesses(teamId) });
  };

  const updateRoleMutation = useMutation({
    mutationFn: updateRole,
    onSuccess: invalidate,
  });

  const archiveRoleMutation = useMutation({
    mutationFn: (roleId: string) => archiveRole(roleId),
    onSuccess: invalidate,
  });

  const bindRoleHarnessMutation = useMutation({
    mutationFn: ({ roleId, harnessId }: { roleId: string; harnessId: string }) =>
      bindRoleHarness(roleId, harnessId),
    onSuccess: invalidate,
  });

  const unbindRoleHarnessMutation = useMutation({
    mutationFn: (roleId: string) => unbindRoleHarness(roleId),
    onSuccess: invalidate,
  });

  /** 更换 Harness：方式（after_current / cancel_and_rerun / manual_takeover）由 Human 选。 */
  const rebindRoleMutation = useMutation({
    mutationFn: ({
      roleId,
      harnessId,
      mode,
    }: {
      roleId: string;
      harnessId: string;
      mode: RebindMode;
    }) => rebindRole(roleId, harnessId, mode),
    onSuccess: invalidate,
  });

  /** 撤销待生效的换绑：当前绑定不变。 */
  const cancelPendingRebindMutation = useMutation({
    mutationFn: (roleId: string) => cancelPendingRebind(roleId),
    onSuccess: invalidate,
  });

  return {
    updateRole: updateRoleMutation,
    archiveRole: archiveRoleMutation,
    bindRoleHarness: bindRoleHarnessMutation,
    unbindRoleHarness: unbindRoleHarnessMutation,
    rebindRole: rebindRoleMutation,
    cancelPendingRebind: cancelPendingRebindMutation,
  };
}
