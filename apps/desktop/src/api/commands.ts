// 前端调用后端的唯一入口：每个 Tauri 命令一个函数，命令名与 crates/teamspace-core/src/api.rs 一致。
// 类型全部来自 ts-rs 生成的 bindings；失败时 Promise 以 ErrorPayload（{ kind, message }）拒绝。
// 业务规则（流转、就绪条件等）以后端返回为准，这里不做任何校验。
import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

import type { BackupExportResult } from "../bindings/BackupExportResult.js";
import type { BackupRestoreResult } from "../bindings/BackupRestoreResult.js";
import type { BackupSummary } from "../bindings/BackupSummary.js";
import type { ExportBackupInput } from "../bindings/ExportBackupInput.js";
import type { InspectBackupInput } from "../bindings/InspectBackupInput.js";
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
import type { Decision } from "../bindings/Decision.js";
import type { ErrorPayload } from "../bindings/ErrorPayload.js";
import type { EventPage } from "../bindings/EventPage.js";
import type { EventQuery } from "../bindings/EventQuery.js";
import type { FileSearchQuery } from "../bindings/FileSearchQuery.js";
import type { FileTasksQuery } from "../bindings/FileTasksQuery.js";
import type { FileTaskRef } from "../bindings/FileTaskRef.js";
import type { GetInboxInput } from "../bindings/GetInboxInput.js";
import type { GetNotificationSettingsInput } from "../bindings/GetNotificationSettingsInput.js";
import type { HarnessInstance } from "../bindings/HarnessInstance.js";
import type { Inbox } from "../bindings/Inbox.js";
import type { ListDecisionsQuery } from "../bindings/ListDecisionsQuery.js";
import type { Message } from "../bindings/Message.js";
import type { MessageThread } from "../bindings/MessageThread.js";
import type { ListThreadsQuery } from "../bindings/ListThreadsQuery.js";
import type { LeaderIntegration } from "../bindings/LeaderIntegration.js";
import type { MarkdownFile } from "../bindings/MarkdownFile.js";
import type { MarkdownList } from "../bindings/MarkdownList.js";
import type { MarkdownListQuery } from "../bindings/MarkdownListQuery.js";
import type { MarkdownReadQuery } from "../bindings/MarkdownReadQuery.js";
import type { McpConfigWrite } from "../bindings/McpConfigWrite.js";
import type { NotificationSettings } from "../bindings/NotificationSettings.js";
import type { NotifyEvent } from "../bindings/NotifyEvent.js";
import type { RecordDecisionInput } from "../bindings/RecordDecisionInput.js";
import type { RebindMode } from "../bindings/RebindMode.js";
import type { RejectReviewInput } from "../bindings/RejectReviewInput.js";
import type { RerunAfterConflictInput } from "../bindings/RerunAfterConflictInput.js";
import type { ResolveMergeManuallyInput } from "../bindings/ResolveMergeManuallyInput.js";
import type { RespondPermissionInput } from "../bindings/RespondPermissionInput.js";
import type { ReviewFilePatch } from "../bindings/ReviewFilePatch.js";
import type { ReviewView } from "../bindings/ReviewView.js";
import type { Role } from "../bindings/Role.js";
import type { RestoreBackupInput } from "../bindings/RestoreBackupInput.js";
import type { RoleFolder } from "../bindings/RoleFolder.js";
import type { RoleTemplate } from "../bindings/RoleTemplate.js";
import type { RoleView } from "../bindings/RoleView.js";
import type { RoleWorkbench } from "../bindings/RoleWorkbench.js";
import type { Run } from "../bindings/Run.js";
import type { RunEvent } from "../bindings/RunEvent.js";
import type { RunUpdatePage } from "../bindings/RunUpdatePage.js";
import type { RunUpdateQuery } from "../bindings/RunUpdateQuery.js";
import type { SuggestTaskPrefixInput } from "../bindings/SuggestTaskPrefixInput.js";
import type { SendMessageInput } from "../bindings/SendMessageInput.js";
import type { TaskBoard } from "../bindings/TaskBoard.js";
import type { TaskBoardQuery } from "../bindings/TaskBoardQuery.js";
import type { TaskDetail } from "../bindings/TaskDetail.js";
import type { TaskStatusInfo } from "../bindings/TaskStatusInfo.js";
import type { Team } from "../bindings/Team.js";
import type { ThreadDetail } from "../bindings/ThreadDetail.js";
import type { TeamOverview } from "../bindings/TeamOverview.js";
import type { UpdateHarnessInput } from "../bindings/UpdateHarnessInput.js";
import type { UpdateNotificationSettingsInput } from "../bindings/UpdateNotificationSettingsInput.js";
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

/**
 * 更换岗位的 Harness：三种方式。
 * - `after_current`：当前执行结束后再生效（默认）。
 * - `cancel_and_rerun`：取消当前执行，立即换绑并在新 Harness 上重新执行该任务。
 * - `manual_takeover`：取消当前执行，任务转为受阻（人工接管），立即换绑。
 */
export const rebindRole = (roleId: string, harnessInstanceId: string, mode: RebindMode) =>
  invoke<RoleView>("rebind_role", { roleId, harnessInstanceId, mode });

/** 撤销待生效的换绑：当前绑定不变。 */
export const cancelPendingRebind = (roleId: string) =>
  invoke<RoleView>("cancel_pending_rebind", { roleId });

/** 岗位文件夹：绝对路径、是否存在、最近的工作日志（默认 20 条，最新在前）。 */
export const getRoleFolder = (roleId: string, limit?: number) =>
  invoke<RoleFolder>("get_role_folder", { roleId, limit: limit ?? null });

/** 在 Finder 中打开岗位文件夹；文件夹不存在时被拒绝。 */
export const revealRoleFolder = (roleId: string) => invoke<null>("reveal_role_folder", { roleId });

/** 岗位工作台：岗位卡片（含绑定与握手结果）、文件夹与工作日志、最近 8 个任务、最近 10 个过程文件。 */
export const getRoleWorkbench = (roleId: string) =>
  invoke<RoleWorkbench>("get_role_workbench", { roleId });

/**
 * 岗位启动包预览：该岗位下一次执行会附上的交接内容（换了 Harness 后新环境第一次执行时带上）。
 * 不会附加时返回空字符串。
 */
export const getRoleHandoffPreview = (roleId: string) =>
  invoke<string>("get_role_handoff_preview", { roleId });

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

// ---------------------------------------------------------------- 共享文件

/** 列出工作空间内的全部 Markdown 文件（相对路径、已排序；truncated 表示可能还有更多）。 */
export const listWorkspaceMarkdown = (input: MarkdownListQuery) =>
  invoke<MarkdownList>("list_workspace_markdown", { input });

/** 读取工作空间内的一个 Markdown 文件（只读；越界或非 .md 由内核拒绝）。 */
export const readWorkspaceMarkdown = (input: MarkdownReadQuery) =>
  invoke<MarkdownFile>("read_workspace_markdown", { input });

/** 引用了某个文件的任务（编号、标题、状态、负责岗位）。 */
export const listTasksReferencingFile = (input: FileTasksQuery) =>
  invoke<FileTaskRef[]>("list_tasks_referencing_file", { input });

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

// ---------------------------------------------------------------- 岗位消息

/** 发送一条消息给岗位：消息进入排队，由投递引擎交给该岗位绑定的 Harness 处理。 */
export const sendMessage = (input: SendMessageInput) => invoke<Message>("send_message", { input });

/** 列出消息线程：按团队（必填）、任务与岗位筛选。 */
export const listThreads = (input: ListThreadsQuery) =>
  invoke<MessageThread[]>("list_threads", { input });

/** 取一个线程及其中的消息。 */
export const getThread = (threadId: string) => invoke<ThreadDetail>("get_thread", { threadId });

/** 重发一条失败或过期的消息。 */
export const retryMessage = (messageId: string) => invoke<Message>("retry_message", { messageId });

// ---------------------------------------------------------------- Leader 接入

/** Leader 接入配置：teamspace-mcp 路径、控制 socket 路径与三段可复制的配置文本。 */
export const leaderIntegration = (teamId: string) =>
  invoke<LeaderIntegration>("leader_integration", { teamId });

/** 把 mcpServers.teamspace 合并进工作空间的 .mcp.json（只在用户确认后调用）。 */
export const writeWorkspaceMcpConfig = (teamId: string) =>
  invoke<McpConfigWrite>("write_workspace_mcp_config", { teamId });

// ---------------------------------------------------------------- 待我处理、决策、通知

/** 通知事件的名称（与 crates/teamspace-core 的 NOTIFY_EVENT 一致）。 */
export const NOTIFY_EVENT = "teamspace:notify";

/**
 * 订阅通知事件（待验收、执行失败、受阻、消息已回复、消息需要处理、权限请求）与消息状态变化。
 * 页面据此实时更新，不必轮询；返回取消订阅的函数。
 */
export const onNotifyEvent = (handler: (event: NotifyEvent) => void): Promise<UnlistenFn> =>
  listen<NotifyEvent>(NOTIFY_EVENT, (event) => {
    handler(event.payload);
  });

/** 待我处理：按类别聚合需要 Human 处理的事项；已处理的项按真实状态自动消失。 */
export const getInbox = (input: GetInboxInput) => invoke<Inbox>("get_inbox", { input });

/** 记录一条决策：写入决策，并在关联岗位的工作日志追加一行「决策：…」。 */
export const recordDecision = (input: RecordDecisionInput) =>
  invoke<Decision>("record_decision", { input });

/** 决策列表：按团队（必填）、任务与消息筛选。 */
export const listDecisions = (input: ListDecisionsQuery) =>
  invoke<Decision[]>("list_decisions", { input });

/** 通知订阅设置：按类别与岗位开关，默认全部开启。 */
export const getNotificationSettings = (input: GetNotificationSettingsInput) =>
  invoke<NotificationSettings>("get_notification_settings", { input });

/** 修改通知订阅设置，返回修改后的设置。 */
export const updateNotificationSettings = (input: UpdateNotificationSettingsInput) =>
  invoke<NotificationSettings>("update_notification_settings", { input });

// ---------------------------------------------------------------- 数据备份

/** 导出数据备份：数据库一致性快照 + 各团队工作空间的岗位文件夹副本 + 清单。 */
export const exportBackup = (input: ExportBackupInput) =>
  invoke<BackupExportResult>("export_backup", { input });

/** 查看备份摘要：恢复前给 Human 看的那一份（清单里的版本、团队与文件清单）。 */
export const inspectBackup = (input: InspectBackupInput) =>
  invoke<BackupSummary>("inspect_backup", { input });

/** 从备份恢复：恢复前自动导出一份「恢复前备份」，然后恢复数据库与岗位文件夹。 */
export const restoreBackup = (input: RestoreBackupInput) =>
  invoke<BackupRestoreResult>("restore_backup", { input });

/** 打开文件夹对话框选择备份存放目录；用户取消时为 null。 */
export const pickBackupDestDir = () => invoke<string | null>("pick_backup_dest_dir");
