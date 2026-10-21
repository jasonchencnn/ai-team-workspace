//! Tauri 命令包装：每个 `Teamspace` 方法一个同名命令，业务规则全部在 teamspace-core。
//!
//! 命令使用 `#[tauri::command(async)]` 在工作线程执行，避免阻塞主线程。

use std::sync::Arc;

use tauri::{AppHandle, State};
use tauri_plugin_dialog::DialogExt;
use teamspace_core::model::*;
use teamspace_core::{Error, Teamspace};

#[tauri::command(async)]
pub fn list_teams(ts: State<'_, Arc<Teamspace>>) -> Result<Vec<Team>, Error> {
    ts.list_teams()
}

#[tauri::command(async)]
pub fn create_team(ts: State<'_, Arc<Teamspace>>, input: CreateTeamInput) -> Result<Team, Error> {
    ts.create_team(input)
}

#[tauri::command(async)]
pub fn get_team_overview(
    ts: State<'_, Arc<Teamspace>>,
    team_id: &str,
) -> Result<TeamOverview, Error> {
    ts.get_team_overview(team_id)
}

#[tauri::command(async)]
pub fn inspect_workspace(
    ts: State<'_, Arc<Teamspace>>,
    path: &str,
) -> Result<WorkspaceInfo, Error> {
    ts.inspect_workspace(path)
}

#[tauri::command(async)]
pub fn list_role_templates(ts: State<'_, Arc<Teamspace>>) -> Result<Vec<RoleTemplate>, Error> {
    Ok(ts.list_role_templates())
}

#[tauri::command(async)]
pub fn suggest_task_prefix(
    ts: State<'_, Arc<Teamspace>>,
    input: SuggestTaskPrefixInput,
) -> Result<String, Error> {
    ts.suggest_task_prefix(input)
}

#[tauri::command(async)]
pub fn list_roles(ts: State<'_, Arc<Teamspace>>, team_id: &str) -> Result<Vec<RoleView>, Error> {
    ts.list_roles(team_id)
}

#[tauri::command(async)]
pub fn create_role(
    ts: State<'_, Arc<Teamspace>>,
    input: CreateRoleInput,
) -> Result<RoleView, Error> {
    ts.create_role(input)
}

#[tauri::command(async)]
pub fn update_role(
    ts: State<'_, Arc<Teamspace>>,
    input: UpdateRoleInput,
) -> Result<RoleView, Error> {
    ts.update_role(input)
}

#[tauri::command(async)]
pub fn archive_role(ts: State<'_, Arc<Teamspace>>, role_id: &str) -> Result<Role, Error> {
    ts.archive_role(role_id)
}

#[tauri::command(async)]
pub fn bind_role_harness(
    ts: State<'_, Arc<Teamspace>>,
    role_id: &str,
    harness_id: &str,
) -> Result<RoleView, Error> {
    ts.bind_role_harness(role_id, harness_id)
}

#[tauri::command(async)]
pub fn unbind_role_harness(
    ts: State<'_, Arc<Teamspace>>,
    role_id: &str,
) -> Result<RoleView, Error> {
    ts.unbind_role_harness(role_id)
}

#[tauri::command(async)]
pub fn get_role_folder(
    ts: State<'_, Arc<Teamspace>>,
    role_id: &str,
    limit: Option<u32>,
) -> Result<RoleFolder, Error> {
    ts.get_role_folder(role_id, limit)
}

#[tauri::command(async)]
pub fn reveal_role_folder(ts: State<'_, Arc<Teamspace>>, role_id: &str) -> Result<(), Error> {
    ts.reveal_role_folder(role_id)
}

#[tauri::command(async)]
pub fn get_role_workbench(
    ts: State<'_, Arc<Teamspace>>,
    role_id: &str,
) -> Result<RoleWorkbench, Error> {
    ts.get_role_workbench(role_id)
}

/// 岗位启动包预览：该岗位下一次执行会附上的交接内容；不会附加时为空字符串。
#[tauri::command(async)]
pub fn get_role_handoff_preview(
    ts: State<'_, Arc<Teamspace>>,
    role_id: &str,
) -> Result<String, Error> {
    ts.get_role_handoff_preview(role_id)
}

#[tauri::command(async)]
pub fn open_role_path(
    ts: State<'_, Arc<Teamspace>>,
    role_id: &str,
    relative_path: Option<String>,
) -> Result<(), Error> {
    ts.open_role_path(role_id, relative_path.as_deref())
}

#[tauri::command(async)]
pub fn list_acp_presets(ts: State<'_, Arc<Teamspace>>) -> Result<Vec<AcpPresetInfo>, Error> {
    Ok(ts.list_acp_presets())
}

#[tauri::command(async)]
pub fn list_harnesses(
    ts: State<'_, Arc<Teamspace>>,
    team_id: &str,
) -> Result<Vec<HarnessInstance>, Error> {
    ts.list_harnesses(team_id)
}

#[tauri::command(async)]
pub fn get_harness(
    ts: State<'_, Arc<Teamspace>>,
    harness_id: &str,
) -> Result<HarnessInstance, Error> {
    ts.get_harness(harness_id)
}

#[tauri::command(async)]
pub fn create_harness(
    ts: State<'_, Arc<Teamspace>>,
    input: CreateHarnessInput,
) -> Result<HarnessInstance, Error> {
    ts.create_harness(input)
}

#[tauri::command(async)]
pub fn update_harness(
    ts: State<'_, Arc<Teamspace>>,
    input: UpdateHarnessInput,
) -> Result<HarnessInstance, Error> {
    ts.update_harness(input)
}

#[tauri::command(async)]
pub fn archive_harness(
    ts: State<'_, Arc<Teamspace>>,
    harness_id: &str,
) -> Result<HarnessInstance, Error> {
    ts.archive_harness(harness_id)
}

#[tauri::command(async)]
pub fn check_harness(
    ts: State<'_, Arc<Teamspace>>,
    harness_id: &str,
) -> Result<HarnessInstance, Error> {
    ts.check_harness(harness_id)
}

#[tauri::command(async)]
pub fn list_task_statuses(ts: State<'_, Arc<Teamspace>>) -> Result<Vec<TaskStatusInfo>, Error> {
    Ok(ts.list_task_statuses())
}

#[tauri::command(async)]
pub fn get_task_board(
    ts: State<'_, Arc<Teamspace>>,
    input: TaskBoardQuery,
) -> Result<TaskBoard, Error> {
    ts.get_task_board(input)
}

#[tauri::command(async)]
pub fn get_task(ts: State<'_, Arc<Teamspace>>, task_id: &str) -> Result<TaskDetail, Error> {
    ts.get_task(task_id)
}

#[tauri::command]
pub fn check_task_dependencies(
    ts: State<'_, Arc<Teamspace>>,
    input: CheckTaskDependenciesInput,
) -> Result<TaskDependencyCheck, Error> {
    ts.check_task_dependencies(input)
}

#[tauri::command(async)]
pub fn create_task(
    ts: State<'_, Arc<Teamspace>>,
    input: CreateTaskInput,
) -> Result<TaskDetail, Error> {
    ts.create_task(input)
}

#[tauri::command(async)]
pub fn update_task(
    ts: State<'_, Arc<Teamspace>>,
    input: UpdateTaskInput,
) -> Result<TaskDetail, Error> {
    ts.update_task(input)
}

#[tauri::command(async)]
pub fn change_task_status(
    ts: State<'_, Arc<Teamspace>>,
    input: ChangeTaskStatusInput,
) -> Result<TaskDetail, Error> {
    ts.change_task_status(input)
}

#[tauri::command(async)]
pub fn search_workspace_files(
    ts: State<'_, Arc<Teamspace>>,
    input: FileSearchQuery,
) -> Result<Vec<String>, Error> {
    ts.search_workspace_files(input)
}

#[tauri::command(async)]
pub fn list_workspace_markdown(
    ts: State<'_, Arc<Teamspace>>,
    input: MarkdownListQuery,
) -> Result<MarkdownList, Error> {
    ts.list_workspace_markdown(input)
}

#[tauri::command(async)]
pub fn read_workspace_markdown(
    ts: State<'_, Arc<Teamspace>>,
    input: MarkdownReadQuery,
) -> Result<MarkdownFile, Error> {
    ts.read_workspace_markdown(input)
}

#[tauri::command(async)]
pub fn list_tasks_referencing_file(
    ts: State<'_, Arc<Teamspace>>,
    input: FileTasksQuery,
) -> Result<Vec<FileTaskRef>, Error> {
    ts.list_tasks_referencing_file(input)
}

#[tauri::command(async)]
pub fn list_events(ts: State<'_, Arc<Teamspace>>, input: EventQuery) -> Result<EventPage, Error> {
    ts.list_events(input)
}

/// Leader 接入配置：`teamspace-mcp` 路径、控制 socket 路径与三段可复制的配置文本。
#[tauri::command(async)]
pub fn leader_integration(
    ts: State<'_, Arc<Teamspace>>,
    team_id: &str,
) -> Result<LeaderIntegration, Error> {
    ts.leader_integration(team_id)
}

/// 把 `mcpServers.teamspace` 合并进工作空间的 `.mcp.json`（只在用户确认后调用）。
#[tauri::command(async)]
pub fn write_workspace_mcp_config(
    ts: State<'_, Arc<Teamspace>>,
    team_id: &str,
) -> Result<McpConfigWrite, Error> {
    ts.write_workspace_mcp_config(team_id)
}

#[tauri::command(async)]
pub fn start_run(ts: State<'_, Arc<Teamspace>>, task_id: &str) -> Result<Run, Error> {
    ts.start_run(task_id)
}

#[tauri::command(async)]
pub fn cancel_run(ts: State<'_, Arc<Teamspace>>, run_id: &str) -> Result<Run, Error> {
    ts.cancel_run(run_id)
}

#[tauri::command(async)]
pub fn resume_run(ts: State<'_, Arc<Teamspace>>, run_id: &str) -> Result<Run, Error> {
    ts.resume_run(run_id)
}

#[tauri::command(async)]
pub fn mark_run_failed(ts: State<'_, Arc<Teamspace>>, run_id: &str) -> Result<Run, Error> {
    ts.mark_run_failed(run_id)
}

#[tauri::command(async)]
pub fn takeover_run(ts: State<'_, Arc<Teamspace>>, run_id: &str) -> Result<Run, Error> {
    ts.takeover_run(run_id)
}

#[tauri::command(async)]
pub fn open_run_dir(ts: State<'_, Arc<Teamspace>>, run_id: &str) -> Result<(), Error> {
    ts.open_run_dir(run_id)
}

#[tauri::command(async)]
pub fn open_run_file(ts: State<'_, Arc<Teamspace>>, run_id: &str, path: &str) -> Result<(), Error> {
    ts.open_run_file(run_id, path)
}

#[tauri::command(async)]
pub fn get_run(ts: State<'_, Arc<Teamspace>>, run_id: &str) -> Result<Run, Error> {
    ts.get_run(run_id)
}

#[tauri::command(async)]
pub fn list_runs(ts: State<'_, Arc<Teamspace>>, task_id: &str) -> Result<Vec<Run>, Error> {
    ts.list_runs(task_id)
}

#[tauri::command(async)]
pub fn list_run_updates(
    ts: State<'_, Arc<Teamspace>>,
    input: RunUpdateQuery,
) -> Result<RunUpdatePage, Error> {
    ts.list_run_updates(input)
}

#[tauri::command(async)]
pub fn respond_permission(
    ts: State<'_, Arc<Teamspace>>,
    input: RespondPermissionInput,
) -> Result<Run, Error> {
    ts.respond_permission(input)
}

#[tauri::command(async)]
pub fn get_review(ts: State<'_, Arc<Teamspace>>, task_id: &str) -> Result<ReviewView, Error> {
    ts.get_review(task_id)
}

#[tauri::command(async)]
pub fn get_review_file_patch(
    ts: State<'_, Arc<Teamspace>>,
    run_id: &str,
    path: &str,
) -> Result<ReviewFilePatch, Error> {
    ts.get_review_file_patch(run_id, path)
}

#[tauri::command(async)]
pub fn approve_review(
    ts: State<'_, Arc<Teamspace>>,
    input: ApproveReviewInput,
) -> Result<ApproveReviewResult, Error> {
    ts.approve_review(input)
}

#[tauri::command(async)]
pub fn reject_review(
    ts: State<'_, Arc<Teamspace>>,
    input: RejectReviewInput,
) -> Result<Run, Error> {
    ts.reject_review(input)
}

#[tauri::command(async)]
pub fn resolve_merge_manually(
    ts: State<'_, Arc<Teamspace>>,
    input: ResolveMergeManuallyInput,
) -> Result<ApproveReviewResult, Error> {
    ts.resolve_merge_manually(input)
}

#[tauri::command(async)]
pub fn rerun_after_conflict(
    ts: State<'_, Arc<Teamspace>>,
    input: RerunAfterConflictInput,
) -> Result<TaskDetail, Error> {
    ts.rerun_after_conflict(input)
}

/// 打开系统文件夹对话框；取消返回 `None`，否则返回所选目录的工作区信息。
#[tauri::command(async)]
pub fn pick_workspace_folder(
    app: AppHandle,
    ts: State<'_, Arc<Teamspace>>,
) -> Result<Option<WorkspaceInfo>, Error> {
    let Some(picked) = app.dialog().file().blocking_pick_folder() else {
        return Ok(None);
    };
    let path = picked
        .into_path()
        .map_err(|err| Error::invalid(format!("无法读取所选目录：{err}")))?;
    ts.inspect_workspace(&path.to_string_lossy()).map(Some)
}

// ---------------------------------------------------------------- 岗位消息

#[tauri::command(async)]
pub fn send_message(
    ts: State<'_, Arc<Teamspace>>,
    input: SendMessageInput,
) -> Result<Message, Error> {
    ts.send_message(input)
}

#[tauri::command(async)]
pub fn list_threads(
    ts: State<'_, Arc<Teamspace>>,
    input: ListThreadsQuery,
) -> Result<Vec<MessageThread>, Error> {
    ts.list_threads(input)
}

#[tauri::command(async)]
pub fn get_thread(ts: State<'_, Arc<Teamspace>>, thread_id: &str) -> Result<ThreadDetail, Error> {
    ts.get_thread(thread_id)
}

#[tauri::command(async)]
pub fn retry_message(ts: State<'_, Arc<Teamspace>>, message_id: &str) -> Result<Message, Error> {
    ts.retry_message(message_id)
}

// ---------------------------------------------------------------- 待我处理、决策、通知

#[tauri::command(async)]
pub fn get_inbox(ts: State<'_, Arc<Teamspace>>, input: GetInboxInput) -> Result<Inbox, Error> {
    ts.get_inbox(input)
}

#[tauri::command(async)]
pub fn record_decision(
    ts: State<'_, Arc<Teamspace>>,
    input: RecordDecisionInput,
) -> Result<Decision, Error> {
    ts.record_decision(input)
}

#[tauri::command(async)]
pub fn list_decisions(
    ts: State<'_, Arc<Teamspace>>,
    input: ListDecisionsQuery,
) -> Result<Vec<Decision>, Error> {
    ts.list_decisions(input)
}

#[tauri::command(async)]
pub fn get_notification_settings(
    ts: State<'_, Arc<Teamspace>>,
    input: GetNotificationSettingsInput,
) -> Result<NotificationSettings, Error> {
    ts.get_notification_settings(input)
}

#[tauri::command(async)]
pub fn update_notification_settings(
    ts: State<'_, Arc<Teamspace>>,
    input: UpdateNotificationSettingsInput,
) -> Result<NotificationSettings, Error> {
    ts.update_notification_settings(input)
}
