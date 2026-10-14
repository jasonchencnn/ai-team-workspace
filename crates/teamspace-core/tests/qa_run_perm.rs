//! 测试轮次 2026-10-14（Sonnet B）：权限与岗位文件夹边界的 driver 层用例。
//! 真实子进程（cargo 构建的 `teamspace-mock-agent`，用 `[mock:outside]` 等标记）、真实 Git、临时目录；
//! 不调用任何真实 Agent。
//!
//! 对应用例：K-PERM-002、K-PERM-006、K-PERM-008（参数化）、K-FLD-011（D14：工作日志允许 Agent 纯追加）。

mod common;

use std::path::PathBuf;

use common::*;
use teamspace_core::model::*;

// ------------------------------------------------------------------ K-PERM-002

#[test]
fn k_perm_002_outside_write_waits_for_human_then_reject_blocks_and_allow_writes() {
    let e = env(false);
    let out = e.ws.join("out.md");

    // 拒绝：等待决定，随后写入被拦下，执行继续并结束。
    let rejected = e.task("越界 [mock:outside]");
    let run = e.ts.start_run(&rejected.id).unwrap();
    let waiting = e.wait(&run.id, "waiting_permission", |r| {
        r.status == RunStatus::WaitingPermission
    });
    let prompt = waiting.pending_permission.clone().unwrap();
    assert_eq!(prompt.tool_kind.as_deref(), Some("edit"));
    assert!(
        prompt
            .reason
            .contains("写入本岗位文件夹与任务引用文件之外需要确认"),
        "{}",
        prompt.reason
    );
    assert!(
        prompt.paths.iter().any(|p| p.ends_with("/out.md")),
        "{:?}",
        prompt.paths
    );
    assert!(e.events.lock().unwrap().iter().any(|ev| matches!(
        ev,
        RunEvent::PermissionRequested { prompt: p, .. } if p.request_id == prompt.request_id
    )));
    assert!(!out.exists());
    e.respond(&run, &prompt, "reject");
    let run = e.wait_end(&run.id);
    assert_eq!(run.status, RunStatus::Succeeded, "{run:#?}");
    assert!(!out.exists(), "拒绝后范围外文件不应被创建");
    let writes = file_writes(&e.updates(&run.id));
    let blocked: Vec<_> = writes
        .iter()
        .filter(|(p, ..)| p.ends_with("/out.md"))
        .collect();
    assert_eq!(blocked.len(), 1, "{writes:?}");
    assert!(!blocked[0].1, "{blocked:?}");
    assert!(
        summary(&run).contains("[outside:err:Teamspace 未允许这次写入"),
        "{}",
        summary(&run)
    );

    // 允许：该路径随后的写入放行，原因为「Human 已允许对应的权限请求」。
    let allowed = e.task("越界允许 [mock:outside]");
    let run = e.ts.start_run(&allowed.id).unwrap();
    let waiting = e.wait(&run.id, "waiting_permission", |r| {
        r.status == RunStatus::WaitingPermission
    });
    let prompt = waiting.pending_permission.unwrap();
    assert!(
        prompt
            .reason
            .contains("写入本岗位文件夹与任务引用文件之外需要确认")
    );
    e.respond(&run, &prompt, "allow");
    let run = e.wait_end(&run.id);
    assert_eq!(run.status, RunStatus::Succeeded, "{run:#?}");
    assert!(out.is_file(), "允许后应写入 out.md");
    let writes = file_writes(&e.updates(&run.id));
    let granted = writes
        .iter()
        .find(|(p, ..)| p.ends_with("/out.md"))
        .unwrap();
    assert!(granted.1);
    assert_eq!(granted.2, "Human 已允许对应的权限请求");
    assert!(
        run.changed_files.iter().any(|c| c.path == "out.md"),
        "{:?}",
        run.changed_files
    );
}

// ------------------------------------------------------------------ K-PERM-006

#[test]
fn k_perm_006_agent_fs_calls_follow_the_same_scope() {
    let e = env(false);
    let outside = e.dir.path().join("outside.txt");
    std::fs::write(&outside, "机密\n").unwrap();
    let journal_before = hash(&e.role_file("工作日志.md")).unwrap();
    let task = e.task(&format!("探测 [mock:fsprobe={}]", outside.display()));
    let run = e.ts.start_run(&task.id).unwrap();
    let run = e.wait_end(&run.id);
    assert_eq!(run.status, RunStatus::Succeeded, "{run:#?}");

    // 1) 读取范围外文件：JSON-RPC 错误，Agent 收到「读取执行目录与岗位文件夹之外的文件」。
    let text = summary(&run);
    assert!(
        text.contains("[fsprobe-read:err:读取执行目录与岗位文件夹之外的文件"),
        "{text}"
    );
    // 2) 岗位文件夹内：成功并记 FileWrite allowed=true，changedFiles 含该文件。
    let rel = "Teamspace/前端/过程文件/fsprobe-ok.md";
    assert!(text.contains("[fsprobe-write-in:ok]"), "{text}");
    assert!(e.ws.join(rel).is_file());
    assert!(
        run.changed_files.iter().any(|c| c.path == rel),
        "{:?}",
        run.changed_files
    );
    // 3)4) 范围外文件、工作日志：未被允许，文件未被改动。
    assert!(
        text.contains("[fsprobe-write-out:err:Teamspace 未允许这次写入"),
        "{text}"
    );
    assert!(
        text.contains("[fsprobe-write-journal:err:Teamspace 未允许这次写入"),
        "{text}"
    );
    assert_eq!(std::fs::read_to_string(&outside).unwrap(), "机密\n");
    let journal = std::fs::read_to_string(e.role_file("工作日志.md")).unwrap();
    assert!(!journal.contains("# Mock fsprobe"), "{journal}");
    assert!(
        journal.len() > journal_before.len(),
        "Teamspace 自己的条目仍会写入"
    );

    let writes = file_writes(&e.updates(&run.id));
    let find = |suffix: &str| {
        writes
            .iter()
            .filter(|(p, ..)| p.ends_with(suffix))
            .collect::<Vec<_>>()
    };
    assert!(find("/fsprobe-ok.md").iter().all(|w| w.1));
    assert!(!find("/outside.txt").is_empty() && find("/outside.txt").iter().all(|w| !w.1));
    assert!(!find("/工作日志.md").is_empty() && find("/工作日志.md").iter().all(|w| !w.1));
    // D14：工作日志只允许纯追加，整体覆盖被拒。
    assert!(
        find("/工作日志.md")[0]
            .2
            .contains("工作日志只允许在末尾追加"),
        "{writes:?}"
    );
    assert!(run.changed_files.iter().all(|c| c.path != "工作日志.md"));
}

// ------------------------------------------------------------------ K-PERM-008

#[derive(Clone, Copy)]
enum Target {
    /// 工作空间之外的文件（不存在）。
    Outside,
    /// 岗位文件夹里的 `工作日志.md`。
    Journal,
    /// 岗位文件夹里的 `岗位说明.md`。
    Brief,
    /// worktree 根的 `.git` 指针文件（仅 Worktree）。
    GitPointer,
}

/// K-PERM-008：read/search 请求被规则自动允许后，对同一路径的 `fs/write_text_file` 仍不得放行。
fn readgrant_case(git_repo: bool, kind: &str, target: Target) {
    let e = env(git_repo);
    let outside = e.dir.path().join("outside.txt");
    let (param, watched): (String, Option<PathBuf>) = match target {
        Target::Outside => (outside.display().to_string(), Some(outside.clone())),
        Target::Journal => ("Teamspace/前端/工作日志.md".into(), None),
        Target::Brief => ("Teamspace/前端/岗位说明.md".into(), None),
        Target::GitPointer => (".git".into(), None),
    };
    let brief_before = hash(&e.role_file("岗位说明.md"));
    let task = e.task(&format!("授权 [mock:readgrant={kind}|{param}]"));
    let run = e.ts.start_run(&task.id).unwrap();
    let run = e.wait_end(&run.id);
    assert_eq!(run.status, RunStatus::Succeeded, "{run:#?}");
    let exec = PathBuf::from(&run.exec_dir);
    let p = match target {
        Target::Outside => outside.clone(),
        Target::GitPointer => exec.join(".git"),
        Target::Journal => exec.join("Teamspace/前端/工作日志.md"),
        Target::Brief => exec.join("Teamspace/前端/岗位说明.md"),
    };
    if matches!(target, Target::GitPointer) {
        assert!(p.is_file(), "worktree 的 .git 应是指针文件");
    }
    // 1) 请求被规则自动允许。
    let updates = e.updates(&run.id);
    let auto = updates.iter().find_map(|u| match &u.body {
        RunUpdateBody::Permission {
            tool_call_id,
            decision: Some(d),
            ..
        } if tool_call_id == "mock-readgrant" => Some(d.clone()),
        _ => None,
    });
    assert_eq!(
        auto.map(|d| d.by),
        Some("rule".to_owned()),
        "read/search 应由规则自动允许"
    );

    // 2) 写入被拒：JSON-RPC 错误「Teamspace 未允许这次写入」，进度含 FileWrite allowed=false。
    let text = summary(&run);
    assert!(
        text.contains("[readgrant:err:Teamspace 未允许这次写入"),
        "Q-01：自动允许 read/search 后写入被放行。总结：{text}"
    );
    let writes = file_writes(&updates);
    let name = p.file_name().unwrap().to_string_lossy().into_owned();
    let hits: Vec<_> = writes
        .iter()
        .filter(|(path, ..)| path.ends_with(&format!("/{name}")) && !path.contains("过程文件"))
        .collect();
    assert!(!hits.is_empty(), "{writes:?}");
    assert!(hits.iter().all(|w| !w.1), "Q-01：{hits:?}");

    // 3) 内容保持不变。
    if let Some(path) = watched {
        assert!(!path.exists(), "范围外文件不应被创建");
    }
    match target {
        Target::Journal => {
            let journal = std::fs::read_to_string(&p).unwrap_or_default();
            assert!(!journal.contains("# Mock 越权写入"), "{journal}");
        }
        Target::Brief => {
            assert_eq!(hash(&e.role_file("岗位说明.md")), brief_before);
            let text = std::fs::read_to_string(&p).unwrap_or_default();
            assert!(!text.contains("# Mock 越权写入"), "{text}");
        }
        Target::GitPointer => {
            let pointer = std::fs::read_to_string(&p).unwrap();
            assert!(pointer.starts_with("gitdir:"), "指针文件被覆盖：{pointer}");
            // 失联检测：worktree 仍可被 Git 识别。
            git(&exec, &["rev-parse", "--git-dir"]);
        }
        Target::Outside => {}
    }
}

/// 工作日志的追加判断（D14）对每次写入都生效，不因 read/search 被规则自动允许（`granted`）而跳过，
/// 因此这几种组合不受 Q-01 影响，不再忽略。
macro_rules! readgrant_journal {
    ($name:ident, $repo:expr, $kind:expr) => {
        #[test]
        fn $name() {
            readgrant_case($repo, $kind, Target::Journal);
        }
    };
}

macro_rules! readgrant {
    ($name:ident, $git:expr, $kind:expr, $target:expr) => {
        #[test]
        #[ignore = "缺陷 K-PERM-008 (Q-01): read/search 自动允许会调用 grant()，之后对同一路径的 fs/write_text_file 被放行"]
        fn $name() {
            readgrant_case($git, $kind, $target);
        }
    };
}

readgrant!(
    k_perm_008_in_place_read_outside,
    false,
    "read",
    Target::Outside
);
readgrant!(
    k_perm_008_in_place_search_outside,
    false,
    "search",
    Target::Outside
);
readgrant_journal!(k_perm_008_in_place_read_journal, false, "read");
readgrant_journal!(k_perm_008_in_place_search_journal, false, "search");
readgrant!(k_perm_008_in_place_read_brief, false, "read", Target::Brief);
readgrant!(
    k_perm_008_in_place_search_brief,
    false,
    "search",
    Target::Brief
);
readgrant!(
    k_perm_008_worktree_read_outside,
    true,
    "read",
    Target::Outside
);
readgrant!(
    k_perm_008_worktree_search_outside,
    true,
    "search",
    Target::Outside
);
readgrant_journal!(k_perm_008_worktree_read_journal, true, "read");
readgrant_journal!(k_perm_008_worktree_search_journal, true, "search");
readgrant!(k_perm_008_worktree_read_brief, true, "read", Target::Brief);
readgrant!(
    k_perm_008_worktree_search_brief,
    true,
    "search",
    Target::Brief
);
readgrant!(
    k_perm_008_worktree_read_git_pointer,
    true,
    "read",
    Target::GitPointer
);
readgrant!(
    k_perm_008_worktree_search_git_pointer,
    true,
    "search",
    Target::GitPointer
);

// ------------------------------------------------------------------ K-FLD-011

#[test]
fn k_fld_011_progress_stays_in_run_updates_and_agent_can_only_append_to_the_journal() {
    let e = env(false);
    let journal_path = e.role_file("工作日志.md");
    let before = std::fs::read_to_string(&journal_path).unwrap();

    // 1)-3) 一次成功执行：进度进 run_updates，日志只有 Teamspace 写入的条目。
    let ok = e.task("普通任务");
    let run = e.wait_end(&e.ts.start_run(&ok.id).unwrap().id);
    assert_eq!(run.status, RunStatus::Succeeded, "{run:#?}");
    let updates = e.updates(&run.id);
    let has = |f: &dyn Fn(&RunUpdateBody) -> bool| updates.iter().any(|u| f(&u.body));
    assert!(has(&|b| matches!(
        b,
        RunUpdateBody::Message {
            role: MessageRole::Agent,
            ..
        }
    )));
    assert!(has(&|b| matches!(b, RunUpdateBody::Plan { .. })));
    assert!(has(&|b| matches!(b, RunUpdateBody::ToolCall { .. })));
    assert!(has(&|b| matches!(b, RunUpdateBody::Permission { .. })));
    assert!(has(&|b| matches!(b, RunUpdateBody::FileWrite { .. })));
    let after = e.wait_journal("改动文件：");
    let added = after.strip_prefix(&before).unwrap_or_else(|| {
        panic!("日志只应追加。之前：{before}\n之后：{after}");
    });
    assert!(added.contains("开始第 1 次执行"), "{added}");
    assert!(added.contains("第 1 次执行完成（Mock）；摘要："), "{added}");
    assert!(added.contains("改动文件："), "{added}");
    for progress in [
        "收到任务",
        "读取任务说明与引用文件",
        "读取引用文件",
        "写入过程文件",
    ] {
        assert!(
            !added.contains(progress),
            "日志里不应有逐条进展「{progress}」：{added}"
        );
    }

    // 4) Agent 改写日志（整体覆盖）：编辑权限请求转 Human，拒绝后 fs 写入仍被拒，文件未被改动。
    let before_attempt = std::fs::read_to_string(&journal_path).unwrap();
    let editing = e.task("改日志 [mock:editjournal]");
    let run = e.ts.start_run(&editing.id).unwrap();
    let waiting = e.wait(&run.id, "waiting_permission", |r| {
        r.status == RunStatus::WaitingPermission
    });
    let prompt = waiting.pending_permission.unwrap();
    assert_eq!(prompt.tool_kind.as_deref(), Some("edit"));
    assert!(
        prompt.reason.contains("工作日志只允许在末尾追加"),
        "{}",
        prompt.reason
    );
    e.respond(&run, &prompt, "reject");
    let run = e.wait_end(&run.id);
    assert_eq!(run.status, RunStatus::Succeeded, "{run:#?}");
    let writes = file_writes(&e.updates(&run.id));
    let journal_write = writes
        .iter()
        .find(|(p, ..)| p.ends_with("/工作日志.md"))
        .unwrap_or_else(|| panic!("{writes:?}"));
    assert!(!journal_write.1, "{journal_write:?}");
    assert!(
        summary(&run).contains("[editjournal:err:Teamspace 未允许这次写入"),
        "{}",
        summary(&run)
    );
    let now = std::fs::read_to_string(&journal_path).unwrap();
    assert!(!now.contains("# Mock 越权写入"), "{now}");
    assert!(
        now.starts_with(&before_attempt),
        "日志只应被 Teamspace 追加"
    );

    // 5) D14：Agent 在末尾追加进展——权限请求与 fs 写入都由规则自动允许，
    //    Agent 的行与 Teamspace 的开始、结束条目都在，已有内容不变，日志不算改动文件。
    let before_append = std::fs::read_to_string(&journal_path).unwrap();
    let appending = e.task("记进展 [mock:appendjournal]");
    let run = e.wait_end(&e.ts.start_run(&appending.id).unwrap().id);
    assert_eq!(run.status, RunStatus::Succeeded, "{run:#?}");
    let updates = e.updates(&run.id);
    let decision = updates.iter().find_map(|u| match &u.body {
        RunUpdateBody::Permission {
            tool_call_id,
            decision: Some(d),
            ..
        } if tool_call_id == "mock-appendjournal" => Some(d.clone()),
        _ => None,
    });
    let decision = decision.unwrap_or_else(|| panic!("{updates:#?}"));
    assert_eq!(decision.by, "rule");
    assert!(decision.reason.contains("追加"), "{}", decision.reason);
    let writes = file_writes(&updates);
    let journal_write = writes
        .iter()
        .find(|(p, ..)| p.ends_with("/工作日志.md"))
        .unwrap_or_else(|| panic!("{writes:?}"));
    assert!(journal_write.1, "{journal_write:?}");
    assert!(journal_write.2.contains("追加"), "{journal_write:?}");
    assert!(
        summary(&run).contains("[appendjournal:ok]"),
        "{}",
        summary(&run)
    );
    let after = e.wait_journal(&format!("{} · 第 1 次执行完成", appending.key));
    let added = after
        .strip_prefix(&before_append)
        .unwrap_or_else(|| panic!("已有内容不应被改动：{after}"));
    let lines: Vec<&str> = added.lines().collect();
    let pos = |needle: &str| {
        lines
            .iter()
            .position(|l| l.contains(needle))
            .unwrap_or_else(|| panic!("缺少「{needle}」：{added}"))
    };
    let started = pos("开始第 1 次执行");
    let progress = pos(&format!("{} · Mock 进展：已写入过程文件", appending.key));
    let finished = pos("第 1 次执行完成");
    // 「开始」由 Teamspace 在启动 Agent 之后同步写入，与 Agent 的追加先后不定；「完成」一定在最后。
    assert!(started != progress && progress < finished, "{added}");
    assert!(lines.iter().all(|l| l.starts_with("- ")), "{added}");
    assert!(
        run.changed_files
            .iter()
            .all(|c| !c.path.ends_with("工作日志.md")),
        "{:?}",
        run.changed_files
    );
}
