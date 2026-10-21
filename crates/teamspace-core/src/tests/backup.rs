//! 数据备份与恢复（M7-04）：导出、清单、恢复、跳过、拒绝与 Leader 越权留痕。
//!
//! 全部用临时目录：备份写到临时目录，工作空间也在临时目录里，不碰真实数据。

use super::*;
use crate::backup::{BACKUP_PREFIX, MANIFEST_FILE, PRE_RESTORE_PREFIX};
use crate::dispatch::Caller;
use crate::model::{EventKind, EventQuery, TaskBoardQuery};
use serde_json::json;

/// 导出备份到指定目录。
fn export(f: &Fixture, dest: &Path) -> BackupExportResult {
    f.ts.export_backup(ExportBackupInput {
        dest_dir: dest.to_string_lossy().into_owned(),
        app_version: Some("9.9.9".into()),
    })
    .unwrap()
}

/// 备份目录里的清单。
fn manifest_of(backup_dir: &str) -> BackupManifest {
    let path = Path::new(backup_dir).join(MANIFEST_FILE);
    let text = fs::read_to_string(&path).unwrap();
    serde_json::from_str(&text).unwrap()
}

#[test]
fn export_writes_manifest_snapshot_and_role_folder_copies() {
    let f = fixture();
    let team = f.team("T");
    let dest = f.dir.path().join("backups");
    let result = export(&f, &dest);

    // 备份目录建在目标目录下，名字带 UTC 时间戳。
    let backup_dir = Path::new(&result.backup_dir);
    assert!(backup_dir.starts_with(&dest), "{}", result.backup_dir);
    let name = backup_dir.file_name().unwrap().to_string_lossy();
    assert!(name.starts_with(BACKUP_PREFIX), "{name}");
    assert!(Path::new(&result.manifest_path).is_file());
    assert!(Path::new(&result.database_path).is_file());
    assert_eq!(result.team_count, 1);
    assert!(result.total_bytes > 0);

    // 每个岗位的文件夹都有副本。
    let entry = result
        .teams
        .iter()
        .find(|entry| entry.team_id == team.id)
        .expect("清单里有这个团队");
    assert_eq!(entry.workspace_path, f.ws.to_string_lossy());
    assert_eq!(entry.role_folders.len(), 5);
    for folder in &entry.role_folders {
        assert!(folder.problem.is_none(), "{folder:?}");
        assert!(folder.file_count > 0, "{folder:?}");
        let copy = backup_dir.join(&folder.backup_path);
        assert!(copy.is_dir(), "{}", copy.display());
        assert!(copy.join("岗位说明.md").is_file(), "{}", copy.display());
        assert!(copy.join("工作日志.md").is_file(), "{}", copy.display());
    }

    // 清单本身能读出来，版本与团队都对得上。
    let manifest = manifest_of(&result.backup_dir);
    assert_eq!(
        manifest.format_version,
        crate::backup::BACKUP_FORMAT_VERSION
    );
    assert_eq!(manifest.app_version, "9.9.9");
    assert_eq!(manifest.schema_version, crate::store::SCHEMA_VERSION);
    assert_eq!(manifest.teams.len(), 1);
    assert!(
        manifest
            .files
            .iter()
            .any(|file| file.path == "database/teamspace.db")
    );
}

#[test]
fn export_snapshot_is_a_readable_database() {
    let f = fixture();
    f.team("T");
    let dest = f.dir.path().join("backups");
    let result = export(&f, &dest);

    // 快照是独立可读的数据库（不是正在写的文件被复制了一半）。
    let conn = rusqlite::Connection::open(&result.database_path).unwrap();
    let teams: i64 = conn
        .query_row("SELECT count(*) FROM teams", [], |row| row.get(0))
        .unwrap();
    assert_eq!(teams, 1);
    let version: i64 = conn
        .query_row("PRAGMA user_version", [], |row| row.get(0))
        .unwrap();
    assert_eq!(version, crate::store::SCHEMA_VERSION);
}

#[test]
fn restore_brings_back_the_state_at_export_time() {
    let f = fixture();
    let team = f.team("T");
    let role = f.role(&team.id, "role_frontend");
    f.task(&team.id, &role, "导出前的任务");
    let dest = f.dir.path().join("backups");
    let backup = export(&f, &dest);

    // 导出之后又改了数据：新增一个任务、一个团队。
    f.task(&team.id, &role, "导出后的任务");
    f.ts.create_team(CreateTeamInput {
        name: "导出后创建的团队".into(),
        description: String::new(),
        workspace_path: f.ws.to_string_lossy().into_owned(),
        roles: vec![],
    })
    .unwrap();

    let result =
        f.ts.restore_backup(RestoreBackupInput {
            backup_dir: backup.backup_dir.clone(),
        })
        .unwrap();
    assert!(result.database_restored);
    assert!(result.requires_restart);
    assert_eq!(result.backup_dir, backup.backup_dir);

    // 团队回到导出时的样子。
    let teams = f.ts.list_teams().unwrap();
    assert_eq!(teams.len(), 1, "导出后创建的团队应当消失");
    assert_eq!(teams[0].id, team.id);

    // 任务也回到导出时的样子。
    let board =
        f.ts.get_task_board(TaskBoardQuery {
            team_id: team.id.clone(),
            include_cancelled: None,
        })
        .unwrap();
    let titles: Vec<String> = board
        .columns
        .iter()
        .flat_map(|column| column.tasks.iter().map(|task| task.title.clone()))
        .collect();
    assert!(titles.contains(&"导出前的任务".to_owned()), "{titles:?}");
    assert!(!titles.contains(&"导出后的任务".to_owned()), "{titles:?}");
}

#[test]
fn restore_saves_a_pre_restore_backup_first() {
    let f = fixture();
    f.team("T");
    let dest = f.dir.path().join("backups");
    let backup = export(&f, &dest);

    let result =
        f.ts.restore_backup(RestoreBackupInput {
            backup_dir: backup.backup_dir.clone(),
        })
        .unwrap();

    // 恢复前自动导出了一份当前数据。
    let pre = Path::new(&result.pre_restore_backup_dir);
    assert!(pre.is_dir(), "{}", result.pre_restore_backup_dir);
    let name = pre.file_name().unwrap().to_string_lossy();
    assert!(name.starts_with(PRE_RESTORE_PREFIX), "{name}");
    assert!(pre.join(MANIFEST_FILE).is_file());
    assert!(
        result.notes.iter().any(|note| note.contains("恢复前备份")),
        "{:?}",
        result.notes
    );
}

#[test]
fn restore_writes_role_folders_back_to_the_workspace() {
    let f = fixture();
    let team = f.team("T");
    let dest = f.dir.path().join("backups");
    let backup = export(&f, &dest);

    // 导出后在岗位文件夹里放一个文件，恢复后应当回来。
    let folder = f.ws.join("Teamspace/前端开发/过程文件/临时笔记.md");
    fs::create_dir_all(folder.parent().unwrap()).unwrap();
    fs::write(&folder, "导出后的内容").unwrap();
    assert!(
        !f.ts
            .inspect_backup(InspectBackupInput {
                backup_dir: backup.backup_dir.clone(),
            })
            .unwrap()
            .teams
            .iter()
            .any(|entry| entry.team_id == team.id
                && entry
                    .role_folders
                    .iter()
                    .any(|role| role.backup_path.contains("过程文件")))
    );

    let result =
        f.ts.restore_backup(RestoreBackupInput {
            backup_dir: backup.backup_dir.clone(),
        })
        .unwrap();
    assert_eq!(
        result.skipped_folders.len(),
        0,
        "{:?}",
        result.skipped_folders
    );
    assert!(
        result
            .restored_folders
            .iter()
            .any(|path| path.contains("前端开发")),
        "{:?}",
        result.restored_folders
    );
    // 导出前就在岗位文件夹里的文件恢复后还在。
    assert!(f.ws.join("Teamspace/前端开发/岗位说明.md").is_file());
}

#[test]
fn restore_skips_missing_workspaces_and_lists_them() {
    let f = fixture();
    let team = f.team("T");
    let dest = f.dir.path().join("backups");
    let backup = export(&f, &dest);

    // 工作空间没了：岗位文件夹写不回去，但数据库照样恢复。
    fs::remove_dir_all(&f.ws).unwrap();

    let result =
        f.ts.restore_backup(RestoreBackupInput {
            backup_dir: backup.backup_dir.clone(),
        })
        .unwrap();
    assert!(result.database_restored);
    assert_eq!(result.restored_folders.len(), 0);
    assert_eq!(
        result.skipped_folders.len(),
        5,
        "{:?}",
        result.skipped_folders
    );
    for skipped in &result.skipped_folders {
        assert!(skipped.reason.contains("工作空间不存在"), "{skipped:?}");
        assert_eq!(skipped.workspace_path, f.ws.to_string_lossy());
    }
    // 团队本身还在（数据库恢复了）。
    let teams = f.ts.list_teams().unwrap();
    assert_eq!(teams.len(), 1);
    assert_eq!(teams[0].id, team.id);
}

#[test]
fn restore_refuses_a_backup_from_a_newer_schema() {
    let f = fixture();
    f.team("T");
    let dest = f.dir.path().join("backups");
    let backup = export(&f, &dest);

    // 把清单里的 schema 版本改成比当前程序新。
    let manifest_path = Path::new(&backup.backup_dir).join(MANIFEST_FILE);
    let mut manifest = manifest_of(&backup.backup_dir);
    manifest.schema_version = crate::store::SCHEMA_VERSION + 1;
    fs::write(
        &manifest_path,
        serde_json::to_string_pretty(&manifest).unwrap(),
    )
    .unwrap();

    let err =
        f.ts.restore_backup(RestoreBackupInput {
            backup_dir: backup.backup_dir.clone(),
        })
        .unwrap_err();
    assert_eq!(err.kind(), crate::error::ErrorKind::Conflict);
    assert!(err.to_string().contains("更新版本"), "{err}");
}

#[test]
fn restore_falls_back_to_the_current_database_when_the_backup_cannot_be_opened() {
    let f = fixture();
    let team = f.team("T");
    let dest = f.dir.path().join("backups");
    let backup = export(&f, &dest);

    // 把备份里的数据库换成内核打不开的版本（user_version 高于当前支持版本）。
    // 清单里的版本号没动，所以校验能过——问题只在打开数据库时才暴露。
    let broken = f.dir.path().join("broken.db");
    {
        let conn = rusqlite::Connection::open(&broken).unwrap();
        conn.pragma_update(None, "user_version", crate::store::SCHEMA_VERSION + 1)
            .unwrap();
    }
    fs::copy(&broken, Path::new(&backup.database_path)).unwrap();

    let err =
        f.ts.restore_backup(RestoreBackupInput {
            backup_dir: backup.backup_dir.clone(),
        })
        .unwrap_err();
    assert!(err.to_string().contains("退回原数据库"), "{err}");

    // 当前数据没丢：应用还能正常读到原来的团队。
    let teams = f.ts.list_teams().unwrap();
    assert_eq!(teams.len(), 1);
    assert_eq!(teams[0].id, team.id);
}

#[test]
fn inspect_reports_the_backup_summary() {
    let f = fixture();
    f.team("T");
    let dest = f.dir.path().join("backups");
    let backup = export(&f, &dest);

    let summary =
        f.ts.inspect_backup(InspectBackupInput {
            backup_dir: backup.backup_dir.clone(),
        })
        .unwrap();
    assert_eq!(summary.team_count, 1);
    assert_eq!(summary.app_version, "9.9.9");
    assert_eq!(summary.schema_version, crate::store::SCHEMA_VERSION);
    assert!(!summary.needs_migration);
    assert!(summary.total_bytes > 0);
    assert_eq!(summary.teams[0].role_folders.len(), 5);
}

#[test]
fn leader_cannot_export_or_restore_and_it_leaves_a_trace() {
    let f = plain_fixture();
    let team = f.team("团队");
    let dest = f.dir.path().join("backups");

    for cmd in ["export_backup", "inspect_backup", "restore_backup"] {
        let err = crate::dispatch::dispatch(
            &f.ts,
            Caller::Leader,
            cmd,
            &json!({ "input": { "destDir": dest.to_string_lossy(), "backupDir": dest.to_string_lossy() } }),
        )
        .unwrap_err();
        assert_eq!(err.kind, crate::error::ErrorKind::Conflict, "{cmd}");
        assert!(
            err.message.starts_with("Leader 无权执行此操作："),
            "{cmd}: {}",
            err.message
        );
    }

    // 越权尝试在活动页里留痕。
    let events =
        f.ts.list_events(EventQuery {
            team_id: team.id.clone(),
            ..Default::default()
        })
        .unwrap();
    let denied: Vec<_> = events
        .events
        .into_iter()
        .filter(|event| event.kind == EventKind::LeaderDenied)
        .collect();
    assert_eq!(denied.len(), 3, "三次越权应当留三条痕");
    assert_eq!(denied[0].actor, Actor::Leader);
    assert!(
        denied
            .iter()
            .any(|event| event.data["command"] == "export_backup")
    );
    assert!(
        denied
            .iter()
            .any(|event| event.data["command"] == "restore_backup")
    );
}
