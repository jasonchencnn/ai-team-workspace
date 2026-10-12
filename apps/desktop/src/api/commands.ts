// 前端调用后端的唯一入口：每个 Tauri 命令一个函数，命令名与 crates/teamspace-core/src/api.rs 一致。
// 类型全部来自 ts-rs 生成的 bindings；失败时 Promise 以 ErrorPayload（{ kind, message }）拒绝。
// 业务规则（流转、就绪条件等）以后端返回为准，这里不做任何校验。
import { invoke } from "@tauri-apps/api/core";

import type { ChangeTaskStatusInput } from "../bindings/ChangeTaskStatusInput.js";
import type { CreateHarnessInput } from "../bindings/CreateHarnessInput.js";
import type { CreateRoleInput } from "../bindings/CreateRoleInput.js";
import type { CreateTaskInput } from "../bindings/CreateTaskInput.js";
import type { CreateTeamInput } from "../bindings/CreateTeamInput.js";
import type { ErrorPayload } from "../bindings/ErrorPayload.js";
import type { EventPage } from "../bindings/EventPage.js";
import type { EventQuery } from "../bindings/EventQuery.js";
import type { FileSearchQuery } from "../bindings/FileSearchQuery.js";
import type { HarnessDetection } from "../bindings/HarnessDetection.js";
import type { HarnessInstance } from "../bindings/HarnessInstance.js";
import type { Role } from "../bindings/Role.js";
import type { RoleTemplate } from "../bindings/RoleTemplate.js";
import type { RoleView } from "../bindings/RoleView.js";
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

/** 检测某个路径的 Git 工作区。 */
export const inspectWorkspace = (path: string) =>
  invoke<WorkspaceInfo>("inspect_workspace", { path });

/** 打开文件夹对话框并检测所选目录；用户取消时为 null。 */
export const pickWorkspaceFolder = () => invoke<WorkspaceInfo | null>("pick_workspace_folder");

// ---------------------------------------------------------------- 岗位

/** 内置岗位模板。 */
export const listRoleTemplates = () => invoke<RoleTemplate[]>("list_role_templates");

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

// ---------------------------------------------------------------- Harness

/** 检测本机 Claude Code 与 Codex CLI。 */
export const detectHarnesses = () => invoke<HarnessDetection[]>("detect_harnesses");

/** 团队的未归档 Harness 实例。 */
export const listHarnesses = (teamId: string) =>
  invoke<HarnessInstance[]>("list_harnesses", { teamId });

/** 注册 Harness 实例。 */
export const createHarness = (input: CreateHarnessInput) =>
  invoke<HarnessInstance>("create_harness", { input });

/** 编辑 Harness 实例（健康状态重置为 unknown）。 */
export const updateHarness = (input: UpdateHarnessInput) =>
  invoke<HarnessInstance>("update_harness", { input });

/** 归档 Harness 实例；仍被岗位绑定时被拒绝。 */
export const archiveHarness = (harnessId: string) =>
  invoke<HarnessInstance>("archive_harness", { harnessId });

/** 健康检查并保存结果。 */
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

/** 变更任务状态；进入 blocked 需提供原因。 */
export const changeTaskStatus = (input: ChangeTaskStatusInput) =>
  invoke<TaskDetail>("change_task_status", { input });

/** 在团队工作区中搜索文件，返回相对路径（最多 50 条）。 */
export const searchWorkspaceFiles = (input: FileSearchQuery) =>
  invoke<string[]>("search_workspace_files", { input });

// ---------------------------------------------------------------- 活动

/** 按团队分页列出活动。 */
export const listEvents = (input: EventQuery) => invoke<EventPage>("list_events", { input });
