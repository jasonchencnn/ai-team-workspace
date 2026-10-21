//! 改动统计兜底（S-1b）与重启后的改动文件（G-6）：真实子进程（cargo 构建的 `teamspace-mock-agent`，
//! 用 `[mock:shellwrite=<相对路径>]` 模拟 Agent 用 shell 直接写文件，不经 ACP、不发权限请求）、
//! 真实 Git、临时目录；不调用任何真实 Agent。

mod common;

use std::path::Path;
use std::time::Duration;

use common::*;
use teamspace_core::RunSettings;
use teamspace_core::model::*;

fn paths(run: &Run) -> Vec<(String, FileChangeKind)> {
    run.changed_files
        .iter()
        .map(|f| (f.path.clone(), f.change))
        .collect()
}

fn has_change(run: &Run, path: &str, kind: FileChangeKind) -> bool {
    run.changed_files
        .iter()
        .any(|f| f.path == path && f.change == kind)
}

fn warnings(e: &Env, run: &Run) -> Vec<(String, Vec<String>)> {
    e.updates(&run.id)
        .into_iter()
        .filter_map(|u| match u.body {
            RunUpdateBody::Warning { text, files } => Some((text, files)),
            _ => None,
        })
        .collect()
}

/// 普通文件夹：Agent 用 shell 在岗位文件夹之外写文件，没有任何权限请求。文件计入改动，
/// 执行记录标出醒目的警告，执行虽然成功但任务受阻而不是待验收，活动与工作日志都有记录。
#[test]
fn shell_write_outside_scope_in_a_plain_folder_blocks_the_task() {
    let e = env(false);
    let task = e.task("写文件 [mock:shellwrite=output.txt]");
    let run = e.ts.start_run(&task.id).unwrap();
    let run = e.wait_end(&run.id);

    assert_eq!(run.status, RunStatus::Succeeded, "{run:#?}");
    assert!(e.ws.join("output.txt").is_file());
    assert!(
        summary(&run).contains("[shellwrite:ok]"),
        "{}",
        summary(&run)
    );
    // 没有任何权限请求（Agent 没有申请，Teamspace 也没有被问）。
    assert!(!e.updates(&run.id).iter().any(
        |u| matches!(&u.body, RunUpdateBody::Permission { paths, .. }
                if paths.iter().any(|p| p.ends_with("output.txt")))
    ));

    // 改动 = ACP 报告的过程文件 ∪ 快照发现的 shell 写入；范围外只有 output.txt。
    let session = run.session_id.clone().unwrap();
    assert_eq!(
        paths(&run),
        vec![
            (
                "Teamspace/前端/过程文件/mock-".to_owned() + &session + ".md",
                FileChangeKind::Added
            ),
            ("output.txt".to_owned(), FileChangeKind::Added),
        ]
    );
    assert_eq!(run.out_of_scope, vec!["output.txt".to_owned()]);
    assert_eq!(run.change_note, None);

    // 任务受阻，原因写明范围外的文件。
    let detail = e.ts.get_task(&task.id).unwrap();
    assert_eq!(detail.task.status, TaskStatus::Blocked);
    assert_eq!(
        detail.task.blocked_reason.as_deref(),
        Some("Agent 未经确认改动了权限范围外的文件：output.txt，请检查后处理")
    );

    // 执行记录里有醒目的警告，状态行排在它后面。
    let warned = warnings(&e, &run);
    assert_eq!(warned.len(), 1, "{warned:?}");
    assert!(
        warned[0].0.contains("Agent 未经确认改动了范围外的文件"),
        "{}",
        warned[0].0
    );
    assert!(warned[0].0.contains("output.txt"));
    assert_eq!(warned[0].1, vec!["output.txt".to_owned()]);
    let updates = e.updates(&run.id);
    let warning_at = updates
        .iter()
        .position(|u| matches!(u.body, RunUpdateBody::Warning { .. }))
        .unwrap();
    assert!(matches!(
        updates.last().unwrap().body,
        RunUpdateBody::Status {
            status: RunStatus::Succeeded,
            ..
        }
    ));
    assert!(warning_at < updates.len() - 1);

    // 活动记录与岗位工作日志。
    let journal = e.wait_journal("未经确认改动了范围外的文件");
    assert!(
        journal.contains("Agent 未经确认改动了范围外的文件：output.txt"),
        "{journal}"
    );
    assert!(
        journal.contains("受阻") && journal.contains("Agent 未经确认改动了权限范围外的文件"),
        "{journal}"
    );
    // 事件推送里的最终记录也带着范围外清单（最终的 run_changed 在岗位文件夹同步之后推送，稍等）。
    e.wait_event("run_changed(out_of_scope)", |events| {
        events.iter().any(|ev| {
            matches!(
                ev,
                RunEvent::RunChanged { run: r } if r.id == run.id && r.out_of_scope == ["output.txt"]
            )
        })
    });
    // 结束后快照文件已清理。
    assert!(
        !e.dir
            .path()
            .join(format!("data/run-snapshots/{}.json", run.id))
            .exists()
    );
}

/// 普通文件夹：shell 写入落在本岗位文件夹内，计入改动，任务正常进入待验收。
#[test]
fn shell_write_inside_the_role_folder_counts_and_goes_to_review() {
    let e = env(false);
    let task = e.task("记笔记 [mock:shellwrite=Teamspace/前端/过程文件/shell-note.md]");
    let run = e.ts.start_run(&task.id).unwrap();
    let run = e.wait_end(&run.id);

    assert_eq!(run.status, RunStatus::Succeeded, "{run:#?}");
    assert!(has_change(
        &run,
        "Teamspace/前端/过程文件/shell-note.md",
        FileChangeKind::Added
    ));
    assert_eq!(run.changed_files.len(), 2, "{:?}", paths(&run));
    assert!(run.out_of_scope.is_empty(), "{:?}", run.out_of_scope);
    assert!(warnings(&e, &run).is_empty());
    let detail = e.ts.get_task(&task.id).unwrap();
    assert_eq!(detail.task.status, TaskStatus::Review);
    assert_eq!(detail.task.blocked_reason, None);
}

/// shell 写入落在任务引用的文件夹里同样在范围内。
#[test]
fn shell_write_over_a_referenced_file_is_in_scope() {
    let e = env(false);
    let task = e.task("改规格 [mock:shellwrite=docs/spec.md]");
    let run = e.ts.start_run(&task.id).unwrap();
    let run = e.wait_end(&run.id);
    assert_eq!(run.status, RunStatus::Succeeded, "{run:#?}");
    assert!(has_change(&run, "docs/spec.md", FileChangeKind::Modified));
    assert!(run.out_of_scope.is_empty());
    assert_eq!(
        e.ts.get_task(&task.id).unwrap().task.status,
        TaskStatus::Review
    );
}

/// Human 明确允许过对某路径的编辑后，该路径上的改动不算未经确认。
#[test]
fn a_path_the_human_allowed_is_not_out_of_scope() {
    let e = env(false);
    let task = e.task("越界允许 [mock:outside]");
    let run = e.ts.start_run(&task.id).unwrap();
    let waiting = e.wait(&run.id, "waiting_permission", |r| {
        r.status == RunStatus::WaitingPermission
    });
    let prompt = waiting.pending_permission.unwrap();
    e.respond(&run, &prompt, "allow");
    let run = e.wait_end(&run.id);
    assert_eq!(run.status, RunStatus::Succeeded, "{run:#?}");
    assert!(has_change(&run, "out.md", FileChangeKind::Added));
    assert!(run.out_of_scope.is_empty(), "{:?}", run.out_of_scope);
    assert_eq!(
        e.ts.get_task(&task.id).unwrap().task.status,
        TaskStatus::Review
    );
}

/// 同时有范围内与范围外的 shell 写入（这里一条标记只能写一个文件，所以分两次执行）：
/// 第二次执行不会把第一次留下的范围外文件算进来。
#[test]
fn a_leftover_file_from_an_earlier_run_is_not_blamed_on_the_next_run() {
    let e = env(false);
    let first = e.task("第一次 [mock:shellwrite=left-over.txt]");
    let run = e.ts.start_run(&first.id).unwrap();
    let run = e.wait_end(&run.id);
    assert_eq!(run.out_of_scope, vec!["left-over.txt".to_owned()]);

    let second = e.task("第二次");
    let run = e.ts.start_run(&second.id).unwrap();
    let run = e.wait_end(&run.id);
    assert_eq!(run.status, RunStatus::Succeeded, "{run:#?}");
    assert!(!has_change(&run, "left-over.txt", FileChangeKind::Added));
    assert!(run.out_of_scope.is_empty());
    assert_eq!(
        e.ts.get_task(&second.id).unwrap().task.status,
        TaskStatus::Review
    );
}

/// Git worktree：shell 写入只落在 worktree 里，计入改动；主工作区不受影响，任务正常进入待验收。
#[test]
fn shell_write_in_a_git_worktree_counts_without_blocking() {
    let e = env(true);
    let task = e.task("写文件 [mock:shellwrite=output.txt]");
    let run = e.ts.start_run(&task.id).unwrap();
    let run = e.wait_end(&run.id);

    assert_eq!(run.status, RunStatus::Succeeded, "{run:#?}");
    assert_eq!(run.isolation, RunIsolation::Worktree);
    assert!(has_change(&run, "output.txt", FileChangeKind::Added));
    assert!(Path::new(&run.exec_dir).join("output.txt").is_file());
    assert!(!e.ws.join("output.txt").exists(), "主工作区不应被改动");
    assert!(run.out_of_scope.is_empty());
    assert!(warnings(&e, &run).is_empty());
    assert_eq!(
        e.ts.get_task(&task.id).unwrap().task.status,
        TaskStatus::Review
    );
}

/// 取消（Human 停止）的执行同样用快照算出真实改动，范围外改动在执行记录里标出，任务仍回到 READY。
#[test]
fn cancelled_run_reports_shell_writes_too() {
    let e = env(false);
    let task = e.task("写了就等 [mock:shellwrite=output.txt] [mock:linger]");
    let run = e.ts.start_run(&task.id).unwrap();
    e.wait(&run.id, "shell write done", |_| {
        e.ws.join("output.txt").is_file()
    });
    e.wait(&run.id, "linger message", |_| {
        e.updates(&run.id).iter().any(|u| {
            matches!(&u.body, RunUpdateBody::Message { text, .. } if text.contains("步骤已处理完"))
        })
    });
    let run = e.ts.cancel_run(&run.id).unwrap();
    assert_eq!(run.status, RunStatus::Cancelled);
    assert!(has_change(&run, "output.txt", FileChangeKind::Added));
    assert_eq!(run.out_of_scope, vec!["output.txt".to_owned()]);
    assert_eq!(warnings(&e, &run).len(), 1);
    assert_eq!(
        e.ts.get_task(&task.id).unwrap().task.status,
        TaskStatus::Ready
    );
}

/// G-6：应用退出时被中断的执行，改动文件列表包含中断前已写入的文件（含 shell 写入）。
#[test]
fn interrupted_run_keeps_the_files_written_before_the_restart() {
    let e = env(false);
    let task = e.task("中断前已改文件 [mock:shellwrite=output.txt] [mock:linger]");
    let run = e.ts.start_run(&task.id).unwrap();
    e.wait(&run.id, "linger message", |_| {
        e.updates(&run.id).iter().any(|u| {
            matches!(&u.body, RunUpdateBody::Message { text, .. } if text.contains("步骤已处理完"))
        })
    });
    e.ts.shutdown();
    let run = e.wait(&run.id, "interrupted", |r| {
        r.status == RunStatus::Interrupted
    });

    let session = run.session_id.clone().unwrap();
    assert_eq!(
        paths(&run),
        vec![
            (
                "Teamspace/前端/过程文件/mock-".to_owned() + &session + ".md",
                FileChangeKind::Added
            ),
            ("output.txt".to_owned(), FileChangeKind::Added),
        ]
    );
    // 任务保持执行中；可续接时快照留着。
    assert_eq!(
        e.ts.get_task(&task.id).unwrap().task.status,
        TaskStatus::Doing
    );
    assert!(
        e.dir
            .path()
            .join(format!("data/run-snapshots/{}.json", run.id))
            .is_file()
    );
}

/// G-6：应用被强制结束后重新打开（残留的活动执行被恢复为已中断）时，同样算出改动文件；
/// 没有快照时退回到执行进度里的 ACP 写入记录并说明。
#[test]
fn recovery_after_a_crash_computes_changed_files_from_the_saved_snapshot() {
    let e = env(false);
    let task = e.task("崩溃前已改文件 [mock:shellwrite=output.txt] [mock:linger]");
    let run = e.ts.start_run(&task.id).unwrap();
    e.wait(&run.id, "linger message", |_| {
        e.updates(&run.id).iter().any(|u| {
            matches!(&u.body, RunUpdateBody::Message { text, .. } if text.contains("步骤已处理完"))
        })
    });
    e.ts.shutdown();
    e.wait(&run.id, "interrupted", |r| {
        r.status == RunStatus::Interrupted
    });
    let db = e.dir.path().join("data/t.db");
    let snapshot = e
        .dir
        .path()
        .join(format!("data/run-snapshots/{}.json", run.id));

    // 模拟强制结束：数据库里还是活动状态、改动文件为空。
    let reset = || {
        let conn = rusqlite::Connection::open(&db).unwrap();
        conn.execute(
            "UPDATE runs SET status = 'running', ended_at = NULL, failure = NULL,
                             changed_files = '[]', change_note = NULL WHERE id = ?1",
            [&run.id],
        )
        .unwrap();
    };
    let reopen = || {
        teamspace_core::Teamspace::open(
            &db,
            Box::new(teamspace_core::system_probe::LocalProbe::new()),
        )
        .unwrap()
    };

    // 强制结束 = 旧进程已经不在了：放开数据库的独占权，新实例才打不开——才能重开。
    e.ts.release_instance();
    reset();
    let ts = reopen();
    let recovered = ts.get_run(&run.id).unwrap();
    assert_eq!(recovered.status, RunStatus::Interrupted);
    assert!(has_change(&recovered, "output.txt", FileChangeKind::Added));
    assert_eq!(recovered.changed_files.len(), 2, "{:?}", paths(&recovered));
    assert_eq!(recovered.change_note, None);
    drop(ts);

    // 快照丢失：只剩 ACP 写入记录，并在记录里说明。
    std::fs::remove_file(&snapshot).unwrap();
    reset();
    let ts = reopen();
    let recovered = ts.get_run(&run.id).unwrap();
    assert_eq!(recovered.status, RunStatus::Interrupted);
    assert_eq!(recovered.changed_files.len(), 1, "{:?}", paths(&recovered));
    assert!(
        recovered.changed_files[0]
            .path
            .starts_with("Teamspace/前端/过程文件/mock-")
    );
    assert!(
        recovered
            .change_note
            .as_deref()
            .is_some_and(|n| n.contains("没有执行前的快照")),
        "{:?}",
        recovered.change_note
    );
}

/// Git worktree 的执行被中断：用 Git 统计出改动。
#[test]
fn interrupted_worktree_run_reports_git_changes() {
    let e = env(true);
    let task = e.task("中断前已改文件 [mock:shellwrite=output.txt] [mock:linger]");
    let run = e.ts.start_run(&task.id).unwrap();
    e.wait(&run.id, "linger message", |_| {
        e.updates(&run.id).iter().any(|u| {
            matches!(&u.body, RunUpdateBody::Message { text, .. } if text.contains("步骤已处理完"))
        })
    });
    e.ts.shutdown();
    let run = e.wait(&run.id, "interrupted", |r| {
        r.status == RunStatus::Interrupted
    });
    assert!(
        has_change(&run, "output.txt", FileChangeKind::Added),
        "{:?}",
        paths(&run)
    );
    assert_eq!(run.changed_files.len(), 2, "{:?}", paths(&run));
}

/// 快照文件数超过上限时降级：先扫本岗位文件夹和任务引用所在的目录，其余只扫靠前的一部分；
/// 不会误报，也不会崩，执行记录和任务卡都标出「改动统计不完整」。
#[test]
fn snapshot_over_the_file_limit_degrades_and_says_so() {
    let e = env(false);
    // 一大堆无关文件，排在优先目录之后。
    for i in 0..30 {
        let dir = e.ws.join("zz");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join(format!("f{i:02}.txt")), "x").unwrap();
    }
    e.ts.set_run_settings(RunSettings {
        snapshot_max_entries: 14,
        idle_timeout: Duration::from_secs(30),
        ..RunSettings::default()
    });
    // docs/ 是任务引用所在的目录（优先扫描），里面的范围外写入照样发现；
    // 根目录下靠后的 zzz-late.txt 在覆盖范围之外，不会被发现，也不会被误报成别的。
    let task = e.task("写文件 [mock:shellwrite=docs/extra.txt]");
    let run = e.ts.start_run(&task.id).unwrap();
    let run = e.wait_end(&run.id);

    assert_eq!(run.status, RunStatus::Succeeded, "{run:#?}");
    assert!(run.change_incomplete);
    let note = run.change_note.clone().expect("降级时必须有说明");
    assert!(note.contains("改动统计不完整"), "{note}");
    assert!(note.contains("超过 14 个"), "{note}");
    assert!(note.contains("没有统计"), "{note}");
    assert!(has_change(&run, "docs/extra.txt", FileChangeKind::Added));
    assert_eq!(run.out_of_scope, vec!["docs/extra.txt".to_owned()]);
    assert_eq!(
        e.ts.get_task(&task.id).unwrap().task.status,
        TaskStatus::Blocked
    );
    // 看板上的任务卡也带着「改动统计不完整」。
    let board =
        e.ts.get_task_board(TaskBoardQuery {
            team_id: e.team.id.clone(),
            include_cancelled: None,
            limit: None,
        })
        .unwrap();
    let card = board
        .columns
        .iter()
        .flat_map(|c| &c.tasks)
        .find(|t| t.id == task.id)
        .unwrap();
    assert!(card.change_incomplete);

    // 另一个任务：靠后位置的写入看不到，但执行记录说明了统计不完整；任务照常进入待验收。
    let late = e.task("写文件 [mock:shellwrite=zzz-late.txt]");
    let run = e.ts.start_run(&late.id).unwrap();
    let run = e.wait_end(&run.id);
    assert_eq!(run.status, RunStatus::Succeeded, "{run:#?}");
    assert!(run.change_incomplete);
    assert!(!has_change(&run, "zzz-late.txt", FileChangeKind::Added));
    assert!(run.out_of_scope.is_empty());
    assert_eq!(
        e.ts.get_task(&late.id).unwrap().task.status,
        TaskStatus::Review
    );
}

/// 上限内的普通执行没有统计说明。
#[test]
fn complete_snapshots_leave_no_change_note() {
    let e = env(false);
    let task = e.task("普通任务");
    let run = e.ts.start_run(&task.id).unwrap();
    let run = e.wait_end(&run.id);
    assert_eq!(run.change_note, None);
    assert!(!run.change_incomplete);
    assert!(run.out_of_scope.is_empty());
}

// ------------------------------------------------------------------ 交叉审查 06b 的修复

/// 一般 #1：隐藏文件（如 `.env`）也在检查范围内：shell 写入它算范围外改动。
#[test]
fn a_shell_write_to_a_hidden_file_is_checked() {
    let e = env(false);
    let task = e.task("改环境变量 [mock:shellwrite=.env]");
    let run = e.ts.start_run(&task.id).unwrap();
    let run = e.wait_end(&run.id);
    assert_eq!(run.status, RunStatus::Succeeded, "{run:#?}");
    assert!(has_change(&run, ".env", FileChangeKind::Added));
    assert_eq!(run.out_of_scope, vec![".env".to_owned()]);
    assert_eq!(
        e.ts.get_task(&task.id).unwrap().task.status,
        TaskStatus::Blocked
    );
}

/// 一般 #1：`.git`、`node_modules` 等目录不检查（界面也这么写），里面的写入不算改动。
#[test]
fn writes_into_skipped_directories_are_not_checked() {
    let e = env(false);
    for rel in [
        "node_modules/pkg/index.js",
        "dist/bundle.js",
        ".git/hooks-note",
    ] {
        let task = e.task(&format!("写 {rel} [mock:shellwrite={rel}]"));
        let run = e.ts.start_run(&task.id).unwrap();
        let run = e.wait_end(&run.id);
        assert_eq!(run.status, RunStatus::Succeeded, "{rel}: {run:#?}");
        assert!(run.out_of_scope.is_empty(), "{rel}");
        assert!(!run.changed_files.iter().any(|f| f.path == rel), "{rel}");
        assert_eq!(
            e.ts.get_task(&task.id).unwrap().task.status,
            TaskStatus::Review
        );
    }
}

/// 一般 #4：本岗位的岗位说明由 Teamspace 维护，不算改动；别的岗位的同名文件按普通文件处理。
#[test]
fn only_the_own_role_brief_is_exempt_other_roles_are_ordinary_files() {
    let e = env(false);
    let own = e.task("改本岗位说明 [mock:shellwrite=Teamspace/前端/岗位说明.md]");
    let run = e.ts.start_run(&own.id).unwrap();
    let run = e.wait_end(&run.id);
    assert!(run.out_of_scope.is_empty(), "{:?}", run.out_of_scope);
    assert!(
        !run.changed_files
            .iter()
            .any(|f| f.path == "Teamspace/前端/岗位说明.md")
    );

    let other = e.task("改别的岗位 [mock:shellwrite=Teamspace/测试/岗位说明.md]");
    let run = e.ts.start_run(&other.id).unwrap();
    let run = e.wait_end(&run.id);
    assert_eq!(
        run.out_of_scope,
        vec!["Teamspace/测试/岗位说明.md".to_owned()]
    );
    assert_eq!(
        e.ts.get_task(&other.id).unwrap().task.status,
        TaskStatus::Blocked
    );

    let fake = e.task("编造岗位 [mock:shellwrite=Teamspace/编造/工作日志.md]");
    let run = e.ts.start_run(&fake.id).unwrap();
    let run = e.wait_end(&run.id);
    assert_eq!(
        run.out_of_scope,
        vec!["Teamspace/编造/工作日志.md".to_owned()]
    );
}

/// 一般 #5：被删除的文件，只有 Human 允许过针对该路径的删除才算范围内；
/// 任务引用文件里的文件也一样（允许编辑不等于允许删除）。
#[test]
fn deleting_a_file_needs_an_approved_delete() {
    let e = env(false);
    std::fs::write(e.ws.join("docs/other.md"), "keep me").unwrap();
    std::fs::write(e.ws.join("docs/third.md"), "keep me too").unwrap();

    // shell 直接删除任务引用文件：没有批准，算范围外。
    let task = e.task("删规格 [mock:shellrm=docs/spec.md]");
    let run = e.ts.start_run(&task.id).unwrap();
    let run = e.wait_end(&run.id);
    assert_eq!(run.status, RunStatus::Succeeded, "{run:#?}");
    assert!(has_change(&run, "docs/spec.md", FileChangeKind::Deleted));
    assert_eq!(run.out_of_scope, vec!["docs/spec.md".to_owned()]);
    assert_eq!(
        e.ts.get_task(&task.id).unwrap().task.status,
        TaskStatus::Blocked
    );

    std::fs::write(e.ws.join("docs/spec.md"), "# 规格\n").unwrap();

    // Human 允许了针对该路径的删除：范围内。
    let task = e.task("删文件 [mock:rmgrant=docs/other.md]");
    let run = e.ts.start_run(&task.id).unwrap();
    let waiting = e.wait(&run.id, "waiting_permission", |r| {
        r.status == RunStatus::WaitingPermission
    });
    let prompt = waiting.pending_permission.unwrap();
    assert_eq!(prompt.tool_kind.as_deref(), Some("delete"));
    e.respond(&run, &prompt, "allow");
    let run = e.wait_end(&run.id);
    assert_eq!(run.status, RunStatus::Succeeded, "{run:#?}");
    assert!(!e.ws.join("docs/other.md").exists());
    assert!(has_change(&run, "docs/other.md", FileChangeKind::Deleted));
    assert!(run.out_of_scope.is_empty(), "{:?}", run.out_of_scope);
    assert_eq!(
        e.ts.get_task(&task.id).unwrap().task.status,
        TaskStatus::Review
    );

    // Human 拒绝：文件还在，没有改动。
    let task = e.task("删文件被拒 [mock:rmgrant=docs/third.md]");
    let run = e.ts.start_run(&task.id).unwrap();
    let waiting = e.wait(&run.id, "waiting_permission", |r| {
        r.status == RunStatus::WaitingPermission
    });
    let prompt = waiting.pending_permission.unwrap();
    e.respond(&run, &prompt, "reject");
    let run = e.wait_end(&run.id);
    assert!(e.ws.join("docs/third.md").is_file());
    assert!(!has_change(&run, "docs/third.md", FileChangeKind::Deleted));
    assert!(run.out_of_scope.is_empty());
}

/// 一般 #3：续接时，中断到续接之间别人（Human）改的文件不算本次执行的，也不判范围外，
/// 只在统计说明里写明；中断前 Agent 自己写的文件仍然计入。
#[test]
fn resume_does_not_blame_the_agent_for_changes_made_while_interrupted() {
    let e = env(false);
    let note = "Teamspace/前端/过程文件/before-interrupt.md";
    let task = e.task(&format!("写完等着 [mock:shellwrite={note}] [mock:linger]"));
    let run = e.ts.start_run(&task.id).unwrap();
    e.wait(&run.id, "linger message", |_| {
        e.updates(&run.id).iter().any(|u| {
            matches!(&u.body, RunUpdateBody::Message { text, .. } if text.contains("步骤已处理完"))
        })
    });
    e.ts.shutdown();
    let interrupted = e.wait(&run.id, "interrupted", |r| {
        r.status == RunStatus::Interrupted
    });
    assert!(has_change(&interrupted, note, FileChangeKind::Added));
    assert!(
        e.dir
            .path()
            .join(format!("data/run-snapshots/{}.interrupted.json", run.id))
            .is_file(),
        "中断时的快照要存下来"
    );

    // 中断期间 Human 在范围外手动改了文件。
    std::fs::write(e.ws.join("cleanup-notes.txt"), "by human").unwrap();
    e.ts.resume_run(&run.id).unwrap();
    let run = e.wait_end(&run.id);
    assert_eq!(run.status, RunStatus::Succeeded, "{run:#?}");
    assert!(
        has_change(&run, note, FileChangeKind::Added),
        "{:?}",
        paths(&run)
    );
    assert!(!has_change(
        &run,
        "cleanup-notes.txt",
        FileChangeKind::Added
    ));
    assert!(run.out_of_scope.is_empty(), "{:?}", run.out_of_scope);
    let change_note = run.change_note.clone().expect("要写明间隙里的改动");
    assert!(
        change_note.contains("中断到续接期间另有 1 个文件被改动"),
        "{change_note}"
    );
    assert!(change_note.contains("cleanup-notes.txt"), "{change_note}");
    assert_eq!(
        e.ts.get_task(&task.id).unwrap().task.status,
        TaskStatus::Review
    );
}

/// 一般 #3 的另一面：没有中断时的快照时无法分清，宁可保守——按现状计入，并在说明里写明。
#[test]
fn resume_without_the_interrupt_snapshot_says_it_cannot_tell() {
    let e = env(false);
    let task = e.task("写完等着 [mock:shellwrite=Teamspace/前端/过程文件/a.md] [mock:linger]");
    let run = e.ts.start_run(&task.id).unwrap();
    e.wait(&run.id, "linger message", |_| {
        e.updates(&run.id).iter().any(|u| {
            matches!(&u.body, RunUpdateBody::Message { text, .. } if text.contains("步骤已处理完"))
        })
    });
    e.ts.shutdown();
    e.wait(&run.id, "interrupted", |r| {
        r.status == RunStatus::Interrupted
    });
    std::fs::remove_file(
        e.dir
            .path()
            .join(format!("data/run-snapshots/{}.interrupted.json", run.id)),
    )
    .unwrap();
    std::fs::write(e.ws.join("cleanup-notes.txt"), "by human").unwrap();
    e.ts.resume_run(&run.id).unwrap();
    let run = e.wait_end(&run.id);
    let change_note = run.change_note.clone().unwrap_or_default();
    assert!(change_note.contains("没有中断时的快照"), "{change_note}");
    assert_eq!(run.out_of_scope, vec!["cleanup-notes.txt".to_owned()]);
}

/// 一般 #7：快照存不下来时，改动统计说明里写明，界面看得到；本次执行仍然照常比对。
#[test]
fn a_snapshot_that_cannot_be_saved_is_reported_in_the_change_note() {
    let e = env(false);
    // 让保存快照的目录位置被一个普通文件占住。
    std::fs::write(e.dir.path().join("data/run-snapshots"), "not a directory").unwrap();
    let task = e.task("写文件 [mock:shellwrite=output.txt]");
    let run = e.ts.start_run(&task.id).unwrap();
    let run = e.wait_end(&run.id);
    assert_eq!(run.status, RunStatus::Succeeded, "{run:#?}");
    let note = run.change_note.clone().expect("保存失败必须在说明里写明");
    assert!(note.contains("执行前的快照没有保存成功"), "{note}");
    // 内存里的基线仍然有效：范围外改动照样发现。
    assert_eq!(run.out_of_scope, vec!["output.txt".to_owned()]);
    assert!(!run.change_incomplete);
}
