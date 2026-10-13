//! 岗位工作台（`get_role_workbench`）与在 Finder 中打开岗位文件夹内的路径（`open_role_path`）。

use super::*;
use crate::role_folder::{JOURNAL_FILE, PROCESS_DIR};

fn role_dir(f: &Fixture, name: &str) -> PathBuf {
    f.ws.join("Teamspace").join(name)
}

fn set_mtime(path: &Path, secs: u64) {
    fs::File::options()
        .write(true)
        .open(path)
        .unwrap()
        .set_modified(std::time::UNIX_EPOCH + std::time::Duration::from_secs(secs))
        .unwrap();
}

fn revealed() -> Vec<PathBuf> {
    REVEALED.with(|r| r.borrow().clone())
}

#[test]
fn workbench_collects_binding_tasks_journal_and_files() {
    let f = fixture();
    let team = f.team("T");
    let fe = f.role(&team.id, "role_frontend");
    let be = f.role(&team.id, "role_backend");
    let h = f.harness(&team.id, "Claude Code（默认）", AcpPreset::ClaudeCode);
    f.ts.bind_role_harness(&fe.id, &h.id).unwrap();
    f.ts.check_harness(&h.id).unwrap();

    let mut ids = Vec::new();
    for i in 0..10 {
        ids.push(f.task(&team.id, &fe, &format!("前端任务 {i}")).task.id);
    }
    f.task(&team.id, &be, "后端任务");
    // 让最早的任务成为最近更新的。
    std::thread::sleep(std::time::Duration::from_millis(5));
    let first = f.ts.get_task(&ids[0]).unwrap();
    f.ts.update_task(UpdateTaskInput {
        task_id: first.task.id.clone(),
        title: "前端任务 0（改）".into(),
        description: String::new(),
        owner_role_id: fe.id.clone(),
        file_refs: vec![],
        depends_on: vec![],
    })
    .unwrap();

    let process = role_dir(&f, "前端开发").join(PROCESS_DIR);
    fs::create_dir_all(process.join("草稿")).unwrap();
    for (rel, secs) in [("a.md", 100), ("草稿/b.md", 200), (".hidden.md", 300)] {
        fs::write(process.join(rel), rel).unwrap();
        set_mtime(&process.join(rel), secs);
    }

    let wb = f.ts.get_role_workbench(&fe.id).unwrap();
    assert_eq!(wb.view.role.id, fe.id);
    let bound = wb.view.harness.as_ref().unwrap();
    assert_eq!(bound.id, h.id);
    assert_eq!(bound.handshake.status, HandshakeStatus::Connected);
    assert_eq!(
        bound.handshake.agent.as_ref().unwrap().name,
        "claude-code-acp"
    );

    assert_eq!(wb.task_count, 10);
    assert_eq!(wb.recent_tasks.len(), 8);
    assert_eq!(wb.recent_tasks[0].title, "前端任务 0（改）");
    assert_eq!(wb.recent_tasks[0].key, "FE-001");
    assert!(wb.recent_tasks.iter().all(|t| t.owner.id == fe.id));
    assert!(
        wb.recent_tasks
            .windows(2)
            .all(|w| w[0].updated_at >= w[1].updated_at)
    );

    assert!(wb.folder.exists);
    assert_eq!(
        wb.folder.relative_path.as_deref(),
        Some("Teamspace/前端开发")
    );
    assert!(wb.folder.problem.is_none());
    assert!(!wb.folder.journal.is_empty());
    assert!(wb.folder.journal.len() <= 20);
    assert!(
        wb.folder.journal[0].contains(" · FE-001 · "),
        "最新的日志在前：{:?}",
        wb.folder.journal
    );

    let files: Vec<_> = wb
        .recent_files
        .iter()
        .map(|e| e.relative_path.as_str())
        .collect();
    assert_eq!(files, ["过程文件/草稿/b.md", "过程文件/a.md"]);
    assert_eq!(wb.recent_files[1].size, 4);
}

#[test]
fn workbench_for_role_without_tasks_or_files_is_empty() {
    let f = fixture();
    let team = f.team("T");
    let qa = f.role(&team.id, "role_qa");
    let wb = f.ts.get_role_workbench(&qa.id).unwrap();
    assert!(wb.view.harness.is_none());
    assert!(wb.recent_tasks.is_empty());
    assert_eq!(wb.task_count, 0);
    assert!(wb.recent_files.is_empty(), "过程文件/README.md 不算");
    assert!(wb.folder.exists);
}

#[test]
fn workbench_reports_missing_folder_without_failing() {
    let f = fixture();
    let team = f.team("T");
    let qa = f.role(&team.id, "role_qa");
    fs::remove_dir_all(&f.ws).unwrap();
    let wb = f.ts.get_role_workbench(&qa.id).unwrap();
    assert!(!wb.folder.exists);
    assert!(wb.folder.problem.is_some());
    assert!(wb.recent_files.is_empty());
    assert!(wb.view.folder_problem.is_some());
}

#[test]
fn workbench_rejects_archived_or_unknown_role() {
    let f = fixture();
    let team = f.team("T");
    let pm = f.role(&team.id, "role_product");
    f.ts.archive_role(&pm.id).unwrap();
    assert_kind(f.ts.get_role_workbench(&pm.id), ErrorKind::NotFound);
    assert_kind(f.ts.get_role_workbench("nope"), ErrorKind::NotFound);
}

#[test]
fn open_role_path_reveals_folder_and_files_inside_only() {
    let f = fixture();
    let team = f.team("T");
    let fe = f.role(&team.id, "role_frontend");
    let dir = role_dir(&f, "前端开发").canonicalize().unwrap();
    fs::write(dir.join(PROCESS_DIR).join("素材清单.md"), "x").unwrap();
    fs::write(f.ws.join("secret.md"), "x").unwrap();
    std::os::unix::fs::symlink(
        f.ws.join("secret.md"),
        dir.join(PROCESS_DIR).join("link.md"),
    )
    .unwrap();

    f.ts.open_role_path(&fe.id, None).unwrap();
    f.ts.open_role_path(&fe.id, Some(JOURNAL_FILE)).unwrap();
    f.ts.open_role_path(&fe.id, Some("过程文件/素材清单.md"))
        .unwrap();
    let before = revealed();
    assert_eq!(
        before[before.len() - 3..],
        [
            dir.clone(),
            dir.join(JOURNAL_FILE),
            dir.join("过程文件/素材清单.md")
        ]
    );

    for bad in [
        "../后端开发/岗位说明.md",
        "../../secret.md",
        "/etc/hosts",
        "过程文件/link.md",
    ] {
        assert_kind(f.ts.open_role_path(&fe.id, Some(bad)), ErrorKind::Invalid);
    }
    assert_kind(
        f.ts.open_role_path(&fe.id, Some("过程文件/没有.md")),
        ErrorKind::NotFound,
    );
    assert_eq!(revealed(), before, "被拒绝的路径不应打开");

    fs::remove_dir_all(&f.ws).unwrap();
    assert_kind(f.ts.open_role_path(&fe.id, None), ErrorKind::NotFound);
}
