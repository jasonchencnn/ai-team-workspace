// 前端调用后端的唯一入口：每个 Tauri 命令一个函数，命令名与 crates/teamspace-core/src/api.rs 一致。
// 类型全部来自 ts-rs 生成的 bindings；失败时 Promise 以 ErrorPayload（{ kind, message }）拒绝。
// 业务规则（流转、就绪条件等）以后端返回为准，这里不做任何校验。
import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

import type { AcpPresetInfo } from "../bindings/AcpPresetInfo.js";
import type { ApproveReviewInput } from "../bindings/ApproveReviewInput.js";
import type { ApproveReviewResult } from "../bindings/ApproveReviewResult.js";
import type { ChangeTaskStatusInput } from "../bindings/ChangeTaskStatusInput.js";
import type { CheckTaskDependenciesInput } from "../bindings/CheckTaskDependenciesInput.js";
import type { TaskDependencyCheck } from "../bindings/TaskDependencyCheck.js";
import type { CreateHarnessInput } from "../bindings/CreateHarnessInput.js";
import type { CreateRoleInput } from "../bindings/CreateRoleInput.js";
import type { CreateTaskInput } from "../bindings/CreateTaskInput.js";
import type { CreateTeamInput } from "../bindings/CreateTeamInput.js";
import type { ErrorPayload } from "../bindings/ErrorPayload.js";
import type { EventPage } from "../bindings/EventPage.js";
import type { EventQuery } from "../bindings/EventQuery.js";
import type { FileSearchQuery } from "../bindings/FileSearchQuery.js";
import type { HarnessInstance } from "../bindings/HarnessInstance.js";
import type { RejectReviewInput } from "../bindings/RejectReviewInput.js";
import type { RerunAfterConflictInput } from "../bindings/RerunAfterConflictInput.js";
import type { ResolveMergeManuallyInput } from "../bindings/ResolveMergeManuallyInput.js";
import type { RespondPermissionInput } from "../bindings/RespondPermissionInput.js";
import type { ReviewFilePatch } from "../bindings/ReviewFilePatch.js";
import type { ReviewView } from "../bindings/ReviewView.js";
import type { Role } from "../bindings/Role.js";
import type { RoleFolder } from "../bindings/RoleFolder.js";
import type { RoleTemplate } from "../bindings/RoleTemplate.js";
import type { RoleView } from "../bindings/RoleView.js";
import type { RoleWorkbench } from "../bindings/RoleWorkbench.js";
import type { Run } from "../bindings/Run.js";
import type { RunEvent } from "../bindings/RunEvent.js";
import type { RunUpdatePage } from "../bindings/RunUpdatePage.js";
import type { RunUpdateQuery } from "../bindings/RunUpdateQuery.js";
import type { SuggestTaskPrefixInput } from "../bindings/SuggestTaskPrefixInput.js";
import type { TaskBoard } from "../bindings/TaskBoard.js";
import type { TaskBoardQuery } from "../bindings/TaskBoardQuery.js";
import type { TaskDetail } from "../bindings/TaskDetail.js";
import type { TaskStatusInfo } from "../bindings/TaskStatusInfo.js";
import type { Team } from "../bindings/Team.js";
import type { TeamOverview } from "../bindings/TeamOverview.js";
import type { UpdateHarnessInput } from "../bindings/UpdateHarnessInput.js";
import type { UpdateRoleInput } from "../bindings/UpdateRoleInput.js";
import type { UpdateTaskInput } from "../bindings/UpdateTaskInput.js";
import type { WorkspaceInfo } from "../bindings/WorkspaceInfo.js";

export type { ErrorPayload };

/** 判断 invoke 拒绝的值是否为后端错误体。 */
export function isErrorPayload(value: unknown): value is ErrorPayload {
  return (
    typeof value === "object" &&
    value !== null &&
    "kind" in value &&
    "message" in value &&
    typeof value.message === "string"
  );
}

// ---------------------------------------------------------------- 团队

/** 列出未归档团队。 */
export const listTeams = () => invoke<Team[]>("list_teams");

/** 创建团队及初始岗位。 */
export const createTeam = (input: CreateTeamInput) => invoke<Team>("create_team", { input });

/** 团队概览：工作区状态、岗位卡片、任务计数、进行中的任务、最近活动。 */
export const getTeamOverview = (teamId: string) =>
  invoke<TeamOverview>("get_team_overview", { teamId });

/** 检测某个目录作为工作空间的信息（任意文件夹；是 Git 仓库时 git 含分支与提交）。 */
export const inspectWorkspace = (path: string) =>
  invoke<WorkspaceInfo>("inspect_workspace", { path });

/** 打开文件夹对话框并检测所选目录；用户取消时为 null。 */
export const pickWorkspaceFolder = () => invoke<WorkspaceInfo | null>("pick_workspace_folder");

// ---------------------------------------------------------------- 岗位

/** 可选的「快速添加示例」岗位（默认不加载）。 */
export const listRoleTemplates = () => invoke<RoleTemplate[]>("list_role_templates");

/** 根据岗位名称建议任务前缀（用于表单预填）。 */
export const suggestTaskPrefix = (input: SuggestTaskPrefixInput) =>
  invoke<string>("suggest_task_prefix", { input });

/** 团队的未归档岗位卡片。 */
export const listRoles = (teamId: string) => invoke<RoleView[]>("list_roles", { teamId });

/** 新增岗位。 */
export const createRole = (input: CreateRoleInput) => invoke<RoleView>("create_role", { input });

/** 编辑岗位（code 不可改）。 */
export const updateRole = (input: UpdateRoleInput) => invoke<RoleView>("update_role", { input });

/** 归档岗位；有未结束任务时被拒绝。 */
export const archiveRole = (roleId: string) => invoke<Role>("archive_role", { roleId });

/** 绑定或更换岗位的 Harness。 */
export const bindRoleHarness = (roleId: string, harnessId: string) =>
  invoke<RoleView>("bind_role_harness", { roleId, harnessId });

/** 解除岗位的 Harness 绑定。 */
export const unbindRoleHarness = (roleId: string) =>
  invoke<RoleView>("unbind_role_harness", { roleId });

/** 岗位文件夹：绝对路径、是否存在、最近的工作日志（默认 20 条，最新在前）。 */
export const getRoleFolder = (roleId: string, limit?: number) =>
  invoke<RoleFolder>("get_role_folder", { roleId, limit: limit ?? null });

/** 在 Finder 中打开岗位文件夹；文件夹不存在时被拒绝。 */
export const revealRoleFolder = (roleId: string) => invoke<null>("reveal_role_folder", { roleId });

/** 岗位工作台：岗位卡片（含绑定与握手结果）、文件夹与工作日志、最近 8 个任务、最近 10 个过程文件。 */
export const getRoleWorkbench = (roleId: string) =>
  invoke<RoleWorkbench>("get_role_workbench", { roleId });

/** 在 Finder 中打开岗位文件夹内的文件或目录（相对岗位文件夹；缺省为文件夹本身）；越界路径被拒绝。 */
export const openRolePath = (roleId: string, relativePath?: string) =>
  invoke<null>("open_role_path", { roleId, relativePath: relativePath ?? null });

// ---------------------------------------------------------------- Harness

/** ACP 预设与本机检测结果（默认命令、底层 CLI 是否存在、缺少时的提示）。 */
export const listAcpPresets = () => invoke<AcpPresetInfo[]>("list_acp_presets");

/** 团队的未归档 Harness 实例。 */
export const listHarnesses = (teamId: string) =>
  invoke<HarnessInstance[]>("list_harnesses", { teamId });

/** 单个 Harness 实例（含最近握手结果）。 */
export const getHarness = (harnessId: string) =>
  invoke<HarnessInstance>("get_harness", { harnessId });

/** 注册 Harness 实例。 */
export const createHarness = (input: CreateHarnessInput) =>
  invoke<HarnessInstance>("create_harness", { input });

/** 编辑 Harness 实例（握手状态重置为 unknown）。 */
export const updateHarness = (input: UpdateHarnessInput) =>
  invoke<HarnessInstance>("update_harness", { input });

/** 归档 Harness 实例；仍被岗位绑定时被拒绝。 */
export const archiveHarness = (harnessId: string) =>
  invoke<HarnessInstance>("archive_harness", { harnessId });

/** ACP 握手检查（initialize，不发送 prompt）并保存结果。 */
export const checkHarness = (harnessId: string) =>
  invoke<HarnessInstance>("check_harness", { harnessId });

// ---------------------------------------------------------------- 任务

/** 全部任务状态及显示名。 */
export const listTaskStatuses = () => invoke<TaskStatusInfo[]>("list_task_statuses");

/** 看板数据（按状态分组）。 */
export const getTaskBoard = (input: TaskBoardQuery) =>
  invoke<TaskBoard>("get_task_board", { input });

/** 任务详情。 */
export const getTask = (taskId: string) => invoke<TaskDetail>("get_task", { taskId });

/** 创建任务（todo 或 ready）。 */
export const createTask = (input: CreateTaskInput) => invoke<TaskDetail>("create_task", { input });

/** 编辑任务内容（不改状态）。 */
export const updateTask = (input: UpdateTaskInput) => invoke<TaskDetail>("update_task", { input });

export const checkTaskDependencies = (input: CheckTaskDependenciesInput) =>
  invoke<TaskDependencyCheck>("check_task_dependencies", { input });

/** 变更任务状态；进入 blocked 需提供原因。 */
export const changeTaskStatus = (input: ChangeTaskStatusInput) =>
  invoke<TaskDetail>("change_task_status", { input });

/** 在团队工作区中搜索文件，返回相对路径（最多 50 条）。 */
export const searchWorkspaceFiles = (input: FileSearchQuery) =>
  invoke<string[]>("search_workspace_files", { input });

// ---------------------------------------------------------------- 执行

/** 执行事件的名称（与 crates/teamspace-core 的 RUN_EVENT 一致）。 */
export const RUN_EVENT = "teamspace:run";

/** 订阅执行事件（状态变化、新进度、权限请求）；返回取消订阅的函数。 */
export const onRunEvent = (handler: (event: RunEvent) => void): Promise<UnlistenFn> =>
  listen<RunEvent>(RUN_EVENT, (event) => {
    handler(event.payload);
  });

/** 开始执行任务（READY、受阻，或中断 / 人工接管后重新执行）；返回 queued 状态的执行。 */
export const startRun = (taskId: string) => invoke<Run>("start_run", { taskId });

/** 取消活动执行，等执行结束后返回（任务回到 READY）。 */
export const cancelRun = (runId: string) => invoke<Run>("cancel_run", { runId });

/** 续接已中断的执行（Agent 支持 loadSession 时）。 */
export const resumeRun = (runId: string) => invoke<Run>("resume_run", { runId });

/** 把已中断的执行标记为失败（任务转为受阻）。 */
export const markRunFailed = (runId: string) => invoke<Run>("mark_run_failed", { runId });

/** 人工接管：停止执行，任务保持执行中，由你在执行目录中处理。 */
export const takeoverRun = (runId: string) => invoke<Run>("takeover_run", { runId });

/** 在 Finder 中打开执行目录。 */
export const openRunDir = (runId: string) => invoke<null>("open_run_dir", { runId });

/** 用系统默认程序打开执行目录内的文件（path 相对执行目录，越界被拒绝）。 */
export const openRunFile = (runId: string, path: string) =>
  invoke<null>("open_run_file", { runId, path });

/** 单个执行记录。 */
export const getRun = (runId: string) => invoke<Run>("get_run", { runId });

/** 任务的全部执行，最近的在前。 */
export const listRuns = (taskId: string) => invoke<Run[]>("list_runs", { taskId });

/** 执行进度增量读取：按 id 合并、按 seq 排序；用 nextCursor 继续读取。 */
export const listRunUpdates = (input: RunUpdateQuery) =>
  invoke<RunUpdatePage>("list_run_updates", { input });

/** 回应等待确认的权限请求。 */
export const respondPermission = (input: RespondPermissionInput) =>
  invoke<Run>("respond_permission", { input });

// ---------------------------------------------------------------- 验收

/** 验收页数据：待验收的执行、摘要、改动文件、能否通过 / 退回及原因、历次退回意见。 */
export const getReview = (taskId: string) => invoke<ReviewView>("get_review", { taskId });

/** 一个改动文件的 diff（只有 Git 工作空间提供）。 */
export const getReviewFilePatch = (runId: string, path: string) =>
  invoke<ReviewFilePatch>("get_review_file_patch", { runId, path });

/** 通过验收：Git 合并执行分支（结果可能是 merged / conflict / blocked），普通文件夹直接完成。 */
export const approveReview = (input: ApproveReviewInput) =>
  invoke<ApproveReviewResult>("approve_review", { input });

/** 退回验收（意见必填），在同一执行目录续接执行；返回新的执行。 */
export const rejectReview = (input: RejectReviewInput) => invoke<Run>("reject_review", { input });

/** 合并冲突后「已手动合并，标记完成」：校验执行分支已并入基线，任务完成并清理执行分支。 */
export const resolveMergeManually = (input: ResolveMergeManuallyInput) =>
  invoke<ApproveReviewResult>("resolve_merge_manually", { input });

/** 合并冲突后「重新执行」：删除旧执行分支与 worktree，任务回到 READY。 */
export const rerunAfterConflict = (input: RerunAfterConflictInput) =>
  invoke<TaskDetail>("rerun_after_conflict", { input });

// ---------------------------------------------------------------- 活动

/** 按团队分页列出活动。 */
export const listEvents = (input: EventQuery) => invoke<EventPage>("list_events", { input });
