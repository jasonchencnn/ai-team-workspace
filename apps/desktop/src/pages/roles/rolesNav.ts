// 岗位与绑定页的跳转路径：岗位工作台的「换绑 Harness」要带着岗位 id 回到岗位详情并展开绑定选择。
/** 「Harness 接入」页签的查询参数。 */
export const HARNESS_TAB_PARAM = "harness";

/** 岗位页内切换到「Harness 接入」页签。 */
export function harnessListPath(teamId: string): string {
  return `/teams/${teamId}/roles?tab=${HARNESS_TAB_PARAM}`;
}

/** 岗位工作台路径。 */
export function roleWorkbenchPath(teamId: string, roleId: string): string {
  return `/teams/${teamId}/roles/${roleId}/workbench`;
}

/**
 * 岗位工作台「换绑 Harness」的落点：回到岗位页签、选中该岗位并展开绑定选择，
 * 直接改选即可，不需要用户再点一次「更换 Harness」。
 */
export function roleBindingPath(teamId: string, roleId: string): string {
  return `/teams/${teamId}/roles?tab=roles&role=${roleId}`;
}
