#![forbid(unsafe_code)]

mod commands;

use tauri::Manager;
use teamspace_core::system_probe::LocalProbe;
use teamspace_core::{HostConfig, Teamspace};

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
    change_task_status,
    search_workspace_files,
    list_events,
    pick_workspace_folder,
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
            let teamspace = Teamspace::open_with(&db_path, Box::new(LocalProbe::new()), config)
                .map_err(|err| format!("无法打开数据库 {}：{err}", db_path.display()))?;
            app.manage(teamspace);
            Ok(())
        })
        .invoke_handler(handler())
        .run(tauri::generate_context!())
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
