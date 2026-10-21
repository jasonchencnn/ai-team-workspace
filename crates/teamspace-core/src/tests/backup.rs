//! 数据备份与恢复（M7-04）：导出、清单、恢复、跳过、拒绝与 Leader 越权留痕。
//!
//! 全部用临时目录：备份写到临时目录，工作空间也在临时目录里，不碰真实数据。

use super::*;
use crate::backup::{BACKUP_PREFIX, MANIFEST_FILE, PRE_RESTORE_PREFIX, RestoreFailPoint};
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

/// 改写备份目录里的清单（模拟被改过的备份）。
fn write_manifest(backup_dir: &str, manifest: &BackupManifest) {
    let path = Path::new(backup_dir).join(MANIFEST_FILE);
    fs::write(&path, serde_json::to_string_pretty(manifest).unwrap()).unwrap();
}

/// 备份目录里除清单外的文件（相对路径）。
fn backup_files(backup_dir: &str) -> Vec<String> {
    fn walk(root: &Path, current: &Path, out: &mut Vec<String>) {
        let mut entries: Vec<PathBuf> = fs::read_dir(current)
            .unwrap()
            .filter_map(|entry| entry.ok().map(|entry| entry.path()))
            .collect();
        entries.sort();
        for path in entries {
            let meta = fs::symlink_metadata(&path).unwrap();
            if meta.is_dir() {
                walk(root, &path, out);
            } else if meta.is_file() {
                let relative = path.strip_prefix(root).unwrap();
                out.push(
                    relative
                        .components()
                        .map(|part| part.as_os_str().to_string_lossy().into_owned())
                        .collect::<Vec<String>>()
                        .join("/"),
                );
            }
        }
    }
    let mut out = Vec::new();
    walk(Path::new(backup_dir), Path::new(backup_dir), &mut out);
    out
}

/// 造一条「还在进行中」的执行记录（不启动 Agent，只占住数据库里的状态）。
fn insert_running_run(f: &Fixture, team_id: &str, task_id: &str, role_id: &str, harness_id: &str) {
    let conn = f.ts.lock();
    conn.execute(
        "INSERT INTO runs (id, team_id, task_id, role_id, harness_id, harness_name, harness_preset,
                           harness_command, harness_args, attempt, status, workspace_path, exec_dir,
                           isolation, started_at)
         VALUES ('run_in_progress', ?1, ?2, ?3, ?4, 'Mock', 'mock', 'mock', '[]', 1, 'running',
                 ?5, ?5, 'in_place', ?6)",
        rusqlite::params![
            team_id,
            task_id,
            role_id,
            harness_id,
            f.ws.to_string_lossy(),
            crate::store::now()
        ],
    )
    .unwrap();
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
            limit: None,
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

// ---------------------------------------------------------------- 路径穿越

#[test]
fn restore_skips_role_folders_whose_paths_escape_the_backup_directory() {
    let f = fixture();
    f.team("T");
    let dest = f.dir.path().join("backups");
    let backup = export(&f, &dest);

    // 备份目录与工作空间之外各放一个「哨兵」目录：恢复不该往里写任何东西。
    let outside = f.dir.path().join("escaped");
    fs::create_dir_all(&outside).unwrap();

    // 篡改清单：岗位文件夹的相对路径里带上 `..`，指向备份目录与工作空间之外。
    let mut manifest = manifest_of(&backup.backup_dir);
    let folder = &mut manifest.teams[0].role_folders[0];
    folder.relative_path = "../escaped".to_owned();
    folder.backup_path = "../../escaped".to_owned();
    write_manifest(&backup.backup_dir, &manifest);

    let result =
        f.ts.restore_backup(RestoreBackupInput {
            backup_dir: backup.backup_dir.clone(),
        })
        .unwrap();

    // 逃逸的岗位文件夹被跳过，原因写进结果。
    assert!(
        result
            .skipped_folders
            .iter()
            .any(|skipped| skipped.reason.contains("..")),
        "{:?}",
        result.skipped_folders
    );
    // 其余岗位文件夹照常写回工作空间。
    assert!(
        result.restored_folders.len() >= 4,
        "{:?}",
        result.restored_folders
    );

    // 工作空间与备份目录之外没有落下任何文件。
    assert_eq!(
        fs::read_dir(&outside).unwrap().count(),
        0,
        "工作空间之外不该被创建文件"
    );
    assert!(!f.ws.join("escaped").exists(), "工作空间里不该多出逃逸目录");
}

#[test]
fn restore_refuses_a_manifest_whose_database_path_is_not_relative() {
    for bad in [
        "/etc/passwd",
        "../escaped.db",
        "C:\\Windows\\win.ini",
        "\\\\server\\share\\db",
    ] {
        let f = fixture();
        let team = f.team("T");
        let dest = f.dir.path().join("backups");
        let backup = export(&f, &dest);

        let mut manifest = manifest_of(&backup.backup_dir);
        manifest.database_file = bad.to_owned();
        write_manifest(&backup.backup_dir, &manifest);

        let err =
            f.ts.restore_backup(RestoreBackupInput {
                backup_dir: backup.backup_dir.clone(),
            })
            .unwrap_err();
        assert!(
            err.to_string().contains("相对路径")
                || err.to_string().contains("盘符")
                || err.to_string().contains(".."),
            "{bad}: {err}"
        );
        // 当前数据没被动过。
        let teams = f.ts.list_teams().unwrap();
        assert_eq!(teams.len(), 1, "{bad}");
        assert_eq!(teams[0].id, team.id, "{bad}");
    }
}

#[test]
fn restore_refuses_a_manifest_whose_file_list_points_outside_the_backup_directory() {
    let f = fixture();
    f.team("T");
    let dest = f.dir.path().join("backups");
    let backup = export(&f, &dest);

    let mut manifest = manifest_of(&backup.backup_dir);
    manifest.files.push(BackupFile {
        path: "../../escaped.txt".to_owned(),
        size: 12,
    });
    write_manifest(&backup.backup_dir, &manifest);

    let err =
        f.ts.restore_backup(RestoreBackupInput {
            backup_dir: backup.backup_dir.clone(),
        })
        .unwrap_err();
    assert!(err.to_string().contains("校验"), "{err}");
    // 当前数据没被动过。
    assert_eq!(f.ts.list_teams().unwrap().len(), 1);
}

#[test]
fn restore_skips_a_workspace_path_that_is_not_in_the_backup_database() {
    let f = fixture();
    f.team("T");
    let dest = f.dir.path().join("backups");
    let backup = export(&f, &dest);

    // 篡改清单：把工作空间指向备份数据库里没有的目录。
    let attacker = f.dir.path().join("attacker");
    fs::create_dir_all(&attacker).unwrap();
    let mut manifest = manifest_of(&backup.backup_dir);
    manifest.teams[0].workspace_path = attacker.to_string_lossy().into_owned();
    write_manifest(&backup.backup_dir, &manifest);

    let result =
        f.ts.restore_backup(RestoreBackupInput {
            backup_dir: backup.backup_dir.clone(),
        })
        .unwrap();

    // 工作空间与备份数据库对不上 → 这个团队的岗位文件夹全部跳过并列出。
    assert_eq!(
        result.restored_folders.len(),
        0,
        "{:?}",
        result.restored_folders
    );
    assert_eq!(
        result.skipped_folders.len(),
        5,
        "{:?}",
        result.skipped_folders
    );
    for skipped in &result.skipped_folders {
        assert!(skipped.reason.contains("不一致"), "{skipped:?}");
    }
    // 篡改出来的目录里没有落下任何文件。
    assert_eq!(fs::read_dir(&attacker).unwrap().count(), 0);
}

// ---------------------------------------------------------------- 覆盖失败后的回滚

#[test]
fn restore_puts_the_original_database_back_when_the_overwrite_fails() {
    for point in [
        RestoreFailPoint::SaveOriginal,
        RestoreFailPoint::CopySnapshot,
        RestoreFailPoint::ReplaceMain,
    ] {
        let f = fixture();
        let team = f.team("T");
        let dest = f.dir.path().join("backups");
        let backup = export(&f, &dest);

        // 导出之后再加一个团队：恢复本该把它抹掉，但覆盖失败了。
        f.ts.create_team(CreateTeamInput {
            name: "导出后创建的团队".into(),
            description: String::new(),
            workspace_path: f.ws.to_string_lossy().into_owned(),
            roles: vec![],
        })
        .unwrap();

        crate::backup::fail_restore_at(point);
        let err =
            f.ts.restore_backup(RestoreBackupInput {
                backup_dir: backup.backup_dir.clone(),
            })
            .unwrap_err();
        crate::backup::clear_restore_fail_point();

        assert!(
            err.to_string().contains("已退回原数据库"),
            "{point:?}: {err}"
        );
        // 关键：连接没有停在空库上——原来的数据还在，导出后加的团队也还在。
        let teams = f.ts.list_teams().unwrap();
        assert_eq!(teams.len(), 2, "{point:?}: {teams:?}");
        assert!(teams.iter().any(|entry| entry.id == team.id), "{point:?}");
        // 磁盘上的库文件也没坏：直接打开它还能读到数据。
        let conn = rusqlite::Connection::open(f.db_path()).unwrap();
        let count: i64 = conn
            .query_row("SELECT count(*) FROM teams", [], |row| row.get(0))
            .unwrap();
        assert_eq!(count, 2, "{point:?}");
        // 没有留下半成品文件。
        let leftover = f.db_path().with_extension("db.restore-orig");
        assert!(!leftover.exists(), "{point:?}: {}", leftover.display());
    }
}

#[test]
fn restore_puts_the_original_database_back_when_the_new_database_cannot_be_opened() {
    let f = fixture();
    let team = f.team("T");
    let dest = f.dir.path().join("backups");
    let backup = export(&f, &dest);

    // 把备份里的数据库换成「能通过校验但打不开」的版本：user_version 说自己是 1，
    // 里面还留着一张 events_v2 表——迁移到当前版本时会撞上「table already exists」。
    let broken = f.dir.path().join("broken.db");
    {
        let conn = rusqlite::Connection::open(&broken).unwrap();
        conn.execute_batch("CREATE TABLE events_v2 (id TEXT);")
            .unwrap();
        conn.pragma_update(None, "user_version", 1).unwrap();
    }
    let db_path = Path::new(&backup.database_path);
    fs::copy(&broken, db_path).unwrap();

    // 清单里的版本与大小跟着改，骗过前面的内容校验。
    let mut manifest = manifest_of(&backup.backup_dir);
    manifest.schema_version = 1;
    let size = fs::metadata(db_path).unwrap().len();
    for file in &mut manifest.files {
        if file.path == "database/teamspace.db" {
            file.size = size;
        }
    }
    write_manifest(&backup.backup_dir, &manifest);

    let err =
        f.ts.restore_backup(RestoreBackupInput {
            backup_dir: backup.backup_dir.clone(),
        })
        .unwrap_err();
    assert!(err.to_string().contains("已退回原数据库"), "{err}");

    // 当前数据没丢：应用还能正常读到原来的团队。
    let teams = f.ts.list_teams().unwrap();
    assert_eq!(teams.len(), 1);
    assert_eq!(teams[0].id, team.id);
}

// ---------------------------------------------------------------- 内容校验

#[test]
fn inspect_reports_the_validation_results() {
    let f = fixture();
    f.team("T");
    let dest = f.dir.path().join("backups");
    let backup = export(&f, &dest);

    let summary =
        f.ts.inspect_backup(InspectBackupInput {
            backup_dir: backup.backup_dir.clone(),
        })
        .unwrap();
    let validation = &summary.validation;
    assert!(validation.database_readable, "{validation:?}");
    assert_eq!(validation.database_integrity, "ok");
    assert_eq!(
        validation.database_schema_version,
        crate::store::SCHEMA_VERSION
    );
    assert!(validation.schema_matches, "{validation:?}");
    assert!(validation.files_match, "{validation:?}");
    assert_eq!(validation.skipped_folders, 0);
    assert!(validation.restorable, "{validation:?}");
    assert!(validation.problems.is_empty(), "{:?}", validation.problems);
}

#[test]
fn restore_refuses_a_backup_whose_snapshot_is_not_a_database() {
    let f = fixture();
    let team = f.team("T");
    let dest = f.dir.path().join("backups");
    let backup = export(&f, &dest);

    // 把数据库快照换成不是数据库的文件。
    fs::write(Path::new(&backup.database_path), b"not a database").unwrap();

    let err =
        f.ts.restore_backup(RestoreBackupInput {
            backup_dir: backup.backup_dir.clone(),
        })
        .unwrap_err();
    assert!(err.to_string().contains("校验"), "{err}");
    // 当前数据没被动过。
    let teams = f.ts.list_teams().unwrap();
    assert_eq!(teams.len(), 1);
    assert_eq!(teams[0].id, team.id);
}

#[test]
fn restore_refuses_a_backup_whose_file_list_does_not_match_the_directory() {
    let f = fixture();
    f.team("T");
    let dest = f.dir.path().join("backups");
    let backup = export(&f, &dest);

    // 备份目录里少了一个文件（清单里还写着它）。
    let victim = backup_files(&backup.backup_dir)
        .into_iter()
        .find(|path| path.starts_with("workspaces/"))
        .expect("备份里有岗位文件夹的文件");
    fs::remove_file(Path::new(&backup.backup_dir).join(&victim)).unwrap();

    let err =
        f.ts.restore_backup(RestoreBackupInput {
            backup_dir: backup.backup_dir.clone(),
        })
        .unwrap_err();
    assert!(err.to_string().contains("对不上"), "{err}");
    // 当前数据没被动过。
    assert_eq!(f.ts.list_teams().unwrap().len(), 1);
}

// ---------------------------------------------------------------- 有活儿时不恢复

#[test]
fn restore_refuses_while_a_run_is_in_progress() {
    let f = fixture();
    let team = f.team("T");
    let role = f.role(&team.id, "role_frontend");
    let task = f.task(&team.id, &role, "正在跑的任务");
    let harness = f.harness(&team.id, "Mock", AcpPreset::Mock);
    insert_running_run(&f, &team.id, &task.task.id, &role.id, &harness.id);

    let dest = f.dir.path().join("backups");
    let backup = export(&f, &dest);

    let err =
        f.ts.restore_backup(RestoreBackupInput {
            backup_dir: backup.backup_dir.clone(),
        })
        .unwrap_err();
    assert_eq!(err.kind(), crate::error::ErrorKind::Conflict);
    assert!(err.to_string().contains("执行正在进行"), "{err}");
    assert!(err.to_string().contains("取消"), "{err}");
    // 数据没被动过。
    assert_eq!(f.ts.list_teams().unwrap().len(), 1);
}

#[test]
fn restore_refuses_while_a_message_is_waiting_to_be_delivered() {
    let f = plain_fixture_without_delivery();
    let team = f.team("T");
    let role = f.role(&team.id, "role_frontend");
    f.ts.send_message(SendMessageInput {
        team_id: team.id.clone(),
        to_role_id: role.id.clone(),
        body: "在吗".into(),
        task_id: None,
        thread_id: None,
        subject: None,
        from_role_id: None,
    })
    .unwrap();

    let dest = f.dir.path().join("backups");
    let backup = export(&f, &dest);

    let err =
        f.ts.restore_backup(RestoreBackupInput {
            backup_dir: backup.backup_dir.clone(),
        })
        .unwrap_err();
    assert_eq!(err.kind(), crate::error::ErrorKind::Conflict);
    assert!(err.to_string().contains("消息正在投递"), "{err}");
    // 数据没被动过。
    assert_eq!(f.ts.list_teams().unwrap().len(), 1);
}

// ---------------------------------------------------------------- Leader 越权

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
