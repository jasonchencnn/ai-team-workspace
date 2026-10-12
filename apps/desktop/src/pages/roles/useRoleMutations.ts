import { useMutation, useQueryClient } from "@tanstack/react-query";

import { archiveRole, bindRoleHarness, unbindRoleHarness, updateRole } from "../../api/commands.js";
import { queryKeys } from "../../api/queryKeys.js";

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

  return {
    updateRole: updateRoleMutation,
    archiveRole: archiveRoleMutation,
    bindRoleHarness: bindRoleHarnessMutation,
    unbindRoleHarness: unbindRoleHarnessMutation,
  };
}
