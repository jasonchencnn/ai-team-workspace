#![forbid(unsafe_code)]

mod commands;

use std::sync::Arc;

use tauri::{Emitter, Manager};
use teamspace_core::control::ControlServer;
use teamspace_core::system_probe::LocalProbe;
use teamspace_core::{HostConfig, NOTIFY_EVENT, RUN_EVENT, Teamspace};

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
    rebind_role,
    cancel_pending_rebind,
    get_role_folder,
    reveal_role_folder,
    get_role_workbench,
    get_role_handoff_preview,
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
    list_board_column,
    list_task_options,
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
    list_active_runs,
    list_run_updates,
    respond_permission,
    escalate_permission,
    get_review,
    get_review_file_patch,
    approve_review,
    reject_review,
    resolve_merge_manually,
    rerun_after_conflict,
    list_events,
    leader_integration,
    write_workspace_mcp_config,
    pick_workspace_folder,
    send_message,
    list_threads,
    get_thread,
    list_role_messages,
    retry_message,
    get_inbox,
    record_decision,
    list_decisions,
    get_notification_settings,
    update_notification_settings,
    get_team_automation,
    update_team_automation,
    export_backup,
    inspect_backup,
    restore_backup,
    pick_backup_dest_dir,
);

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
            // 应用内通知（待验收、执行失败、受阻、消息已回复、消息需要处理、权限请求）
            // 与消息状态变化推送给前端：页面不靠轮询也能实时看到。
            let notify_handle = app.handle().clone();
            teamspace.set_notify_listener(move |event| {
                let _ = notify_handle.emit(NOTIFY_EVENT, event);
            });
            // 本机控制 socket：外部会话（Leader）经它操作 Teamspace，权限边界在内核里把守。
            let socket_path = teamspace_core::leader::control_socket_path(&dir);
            // 控制 socket 起不来时应用照常运行：只是 Leader 连不上，界面「接入 Leader」处提示。
            let control = match teamspace_core::control::serve(Arc::clone(&teamspace), &socket_path)
            {
                Ok(control) => Some(control),
                Err(err) => {
                    let message = format!(
                        "控制 socket 不可用（{}）：{err}；Leader 暂时连不上本应用",
                        socket_path.display()
                    );
                    eprintln!("teamspace: {message}");
                    teamspace.set_control_socket_error(Some(message));
                    None
                }
            };
            app.manage(Arc::clone(&teamspace));
            if let Some(control) = control {
                app.manage(control);
            }
            Ok(())
        })
        .invoke_handler(handler())
        .build(tauri::generate_context!())?
        .run(|app, event| {
            // 退出时先停控制 socket（Leader 连不上了），再终止执行中的 Agent 进程，
            // 执行标记为已中断（下次打开可续接或重新执行）。
            if let tauri::RunEvent::Exit = event {
                let control = app.try_state::<ControlServer>();
                teamspace_core::control::shutdown_on_exit(
                    control.as_ref().map(|state| state.inner()),
                    &app.state::<Arc<Teamspace>>(),
                );
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
