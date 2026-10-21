#![forbid(unsafe_code)]

mod commands;

use std::sync::Arc;

use tauri::{Emitter, Manager};
use teamspace_core::control::ControlServer;
use teamspace_core::system_probe::LocalProbe;
use teamspace_core::{HostConfig, RUN_EVENT, Teamspace};

/// 同时生成命令名常量与 `generate_handler!`，保证二者不会漂移。
macro_rules! register_commands {
    ($($name:ident),+ $(,)?) => {
        /// 已注册到 Tauri 的全部命令名。
        pub const REGISTERED_COMMANDS: &[&str] = &[$(stringify!($name)),+];

        fn handler() -> impl Fn(tauri::ipc::Invoke<tauri::Wry>) -> bool + Send + Sync + 'static {
            tauri::generate_handler![$(commands::$name),+]
        }
    };
}

register_commands!(
    list_teams,
    create_team,
    get_team_overview,
    inspect_workspace,
    list_role_templates,
    suggest_task_prefix,
    list_roles,
    create_role,
    update_role,
    archive_role,
    bind_role_harness,
    unbind_role_harness,
    get_role_folder,
    reveal_role_folder,
    get_role_workbench,
    open_role_path,
    list_acp_presets,
    list_harnesses,
    get_harness,
    create_harness,
    update_harness,
    archive_harness,
    check_harness,
    list_task_statuses,
    get_task_board,
    get_task,
    create_task,
    update_task,
    check_task_dependencies,
    change_task_status,
    search_workspace_files,
    list_workspace_markdown,
    read_workspace_markdown,
    list_tasks_referencing_file,
    start_run,
    cancel_run,
    resume_run,
    mark_run_failed,
    takeover_run,
    open_run_dir,
    open_run_file,
    get_run,
    list_runs,
    list_run_updates,
    respond_permission,
    get_review,
    get_review_file_patch,
    approve_review,
    reject_review,
    resolve_merge_manually,
    rerun_after_conflict,
    list_events,
    pick_workspace_folder,
);

/// 本机控制 socket 的文件名（放在应用数据目录里）。
const CONTROL_SOCKET_NAME: &str = "teamspace-control.sock";

/// Starts the native host.
pub fn run() -> tauri::Result<()> {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            let dir = app.path().app_data_dir()?;
            std::fs::create_dir_all(&dir)
                .map_err(|err| format!("无法创建数据目录 {}：{err}", dir.display()))?;
            let db_path = dir.join("teamspace.db");
            // 内置 Mock Agent 由本程序以 `--teamspace-mock-agent` 参数启动（见 main.rs）。
            let config = HostConfig::with_current_exe_mock_agent();
            let teamspace = Arc::new(
                Teamspace::open_with(&db_path, Box::new(LocalProbe::new()), config)
                    .map_err(|err| format!("无法打开数据库 {}：{err}", db_path.display()))?,
            );
            // 执行事件（状态变化、新进度、权限请求）推送给前端。
            let handle = app.handle().clone();
            teamspace.set_run_listener(move |event| {
                let _ = handle.emit(RUN_EVENT, event);
            });
            // 本机控制 socket：外部会话（Leader）经它操作 Teamspace，权限边界在内核里把守。
            let socket_path = dir.join(CONTROL_SOCKET_NAME);
            let control = teamspace_core::control::serve(Arc::clone(&teamspace), &socket_path)
                .map_err(|err| format!("无法监听控制 socket {}：{err}", socket_path.display()))?;
            app.manage(Arc::clone(&teamspace));
            app.manage(control);
            Ok(())
        })
        .invoke_handler(handler())
        .build(tauri::generate_context!())?
        .run(|app, event| {
            // 退出时终止执行中的 Agent 进程，执行标记为已中断（下次打开可续接或重新执行），
            // 并停止控制 socket（关闭监听、删除 socket 文件）。
            if let tauri::RunEvent::Exit = event {
                app.state::<Arc<Teamspace>>().shutdown();
                app.state::<ControlServer>().shutdown();
            }
        });
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::REGISTERED_COMMANDS;
    use teamspace_core::api::{COMMANDS, TAURI_ONLY_COMMANDS};

    #[test]
    fn registered_commands_match_core_manifest() {
        let mut registered: Vec<&str> = REGISTERED_COMMANDS.to_vec();
        let mut expected: Vec<&str> = COMMANDS
            .iter()
            .chain(TAURI_ONLY_COMMANDS)
            .copied()
            .collect();
        registered.sort_unstable();
        expected.sort_unstable();
        assert_eq!(registered, expected);
    }
}
