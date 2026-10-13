//! Tauri 命令包装：每个 `Teamspace` 方法一个同名命令，业务规则全部在 teamspace-core。
//!
//! 命令使用 `#[tauri::command(async)]` 在工作线程执行，避免阻塞主线程。

use tauri::{AppHandle, State};
use tauri_plugin_dialog::DialogExt;
use teamspace_core::model::*;
use teamspace_core::{Error, Teamspace};

#[tauri::command(async)]
pub fn list_teams(ts: State<'_, Teamspace>) -> Result<Vec<Team>, Error> {
    ts.list_teams()
}

#[tauri::command(async)]
pub fn create_team(ts: State<'_, Teamspace>, input: CreateTeamInput) -> Result<Team, Error> {
    ts.create_team(input)
}

#[tauri::command(async)]
pub fn get_team_overview(ts: State<'_, Teamspace>, team_id: &str) -> Result<TeamOverview, Error> {
    ts.get_team_overview(team_id)
}

#[tauri::command(async)]
pub fn inspect_workspace(ts: State<'_, Teamspace>, path: &str) -> Result<WorkspaceInfo, Error> {
    ts.inspect_workspace(path)
}

#[tauri::command(async)]
pub fn list_role_templates(ts: State<'_, Teamspace>) -> Result<Vec<RoleTemplate>, Error> {
    Ok(ts.list_role_templates())
}

#[tauri::command(async)]
pub fn suggest_task_prefix(
    ts: State<'_, Teamspace>,
    input: SuggestTaskPrefixInput,
) -> Result<String, Error> {
    ts.suggest_task_prefix(input)
}

#[tauri::command(async)]
pub fn list_roles(ts: State<'_, Teamspace>, team_id: &str) -> Result<Vec<RoleView>, Error> {
    ts.list_roles(team_id)
}

#[tauri::command(async)]
pub fn create_role(ts: State<'_, Teamspace>, input: CreateRoleInput) -> Result<RoleView, Error> {
    ts.create_role(input)
}

#[tauri::command(async)]
pub fn update_role(ts: State<'_, Teamspace>, input: UpdateRoleInput) -> Result<RoleView, Error> {
    ts.update_role(input)
}

#[tauri::command(async)]
pub fn archive_role(ts: State<'_, Teamspace>, role_id: &str) -> Result<Role, Error> {
    ts.archive_role(role_id)
}

#[tauri::command(async)]
pub fn bind_role_harness(
    ts: State<'_, Teamspace>,
    role_id: &str,
    harness_id: &str,
) -> Result<RoleView, Error> {
    ts.bind_role_harness(role_id, harness_id)
}

#[tauri::command(async)]
pub fn unbind_role_harness(ts: State<'_, Teamspace>, role_id: &str) -> Result<RoleView, Error> {
    ts.unbind_role_harness(role_id)
}

#[tauri::command(async)]
pub fn detect_harnesses(ts: State<'_, Teamspace>) -> Result<Vec<HarnessDetection>, Error> {
    Ok(ts.detect_harnesses())
}

#[tauri::command(async)]
pub fn list_harnesses(
    ts: State<'_, Teamspace>,
    team_id: &str,
) -> Result<Vec<HarnessInstance>, Error> {
    ts.list_harnesses(team_id)
}

#[tauri::command(async)]
pub fn get_harness(ts: State<'_, Teamspace>, harness_id: &str) -> Result<HarnessInstance, Error> {
    ts.get_harness(harness_id)
}

#[tauri::command(async)]
pub fn create_harness(
    ts: State<'_, Teamspace>,
    input: CreateHarnessInput,
) -> Result<HarnessInstance, Error> {
    ts.create_harness(input)
}

#[tauri::command(async)]
pub fn update_harness(
    ts: State<'_, Teamspace>,
    input: UpdateHarnessInput,
) -> Result<HarnessInstance, Error> {
    ts.update_harness(input)
}

#[tauri::command(async)]
pub fn archive_harness(
    ts: State<'_, Teamspace>,
    harness_id: &str,
) -> Result<HarnessInstance, Error> {
    ts.archive_harness(harness_id)
}

#[tauri::command(async)]
pub fn check_harness(ts: State<'_, Teamspace>, harness_id: &str) -> Result<HarnessInstance, Error> {
    ts.check_harness(harness_id)
}

#[tauri::command(async)]
pub fn list_task_statuses(ts: State<'_, Teamspace>) -> Result<Vec<TaskStatusInfo>, Error> {
    Ok(ts.list_task_statuses())
}

#[tauri::command(async)]
pub fn get_task_board(ts: State<'_, Teamspace>, input: TaskBoardQuery) -> Result<TaskBoard, Error> {
    ts.get_task_board(input)
}

#[tauri::command(async)]
pub fn get_task(ts: State<'_, Teamspace>, task_id: &str) -> Result<TaskDetail, Error> {
    ts.get_task(task_id)
}

#[tauri::command(async)]
pub fn create_task(ts: State<'_, Teamspace>, input: CreateTaskInput) -> Result<TaskDetail, Error> {
    ts.create_task(input)
}

#[tauri::command(async)]
pub fn update_task(ts: State<'_, Teamspace>, input: UpdateTaskInput) -> Result<TaskDetail, Error> {
    ts.update_task(input)
}

#[tauri::command(async)]
pub fn change_task_status(
    ts: State<'_, Teamspace>,
    input: ChangeTaskStatusInput,
) -> Result<TaskDetail, Error> {
    ts.change_task_status(input)
}

#[tauri::command(async)]
pub fn search_workspace_files(
    ts: State<'_, Teamspace>,
    input: FileSearchQuery,
) -> Result<Vec<String>, Error> {
    ts.search_workspace_files(input)
}

#[tauri::command(async)]
pub fn list_events(ts: State<'_, Teamspace>, input: EventQuery) -> Result<EventPage, Error> {
    ts.list_events(input)
}

/// 打开系统文件夹对话框；取消返回 `None`，否则返回所选目录的工作区信息。
#[tauri::command(async)]
pub fn pick_workspace_folder(
    app: AppHandle,
    ts: State<'_, Teamspace>,
) -> Result<Option<WorkspaceInfo>, Error> {
    let Some(picked) = app.dialog().file().blocking_pick_folder() else {
        return Ok(None);
    };
    let path = picked
        .into_path()
        .map_err(|err| Error::invalid(format!("无法读取所选目录：{err}")))?;
    ts.inspect_workspace(&path.to_string_lossy()).map(Some)
}
