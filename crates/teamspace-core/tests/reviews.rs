//! 验收（M3-01）的端到端测试：真实子进程（cargo 构建的 `teamspace-mock-agent`）、临时目录。
//! 普通文件夹：执行 → 待验收 → 退回（续接原会话 / 不支持 loadSession 时新开会话）→ 再次待验收 → 通过。
//! Git 工作空间的合并路径在 Git 层（M3-02）接入后补充。

mod common;

use std::path::Path;

use common::*;
use teamspace_core::model::*;

fn run_to_review(e: &Env, task: &Task) -> Run {
    let run = e.ts.start_run(&task.id).unwrap();
    let run = e.wait_end(&run.id);
    assert_eq!(run.status, RunStatus::Succeeded, "{run:#?}");
    assert_eq!(
        e.ts.get_task(&task.id).unwrap().task.status,
        TaskStatus::Review
    );
    run
}

fn user_messages(updates: &[RunUpdate]) -> Vec<String> {
    updates
        .iter()
        .filter_map(|u| match &u.body {
            RunUpdateBody::Message {
                role: MessageRole::User,
                text,
            } => Some(text.clone()),
            _ => None,
        })
        .collect()
}

#[test]
fn plain_folder_reject_resumes_the_session_then_approve_completes() {
    let e = env(false);
    let task = e.task("登录页");
    let first = run_to_review(&e, &task);

    let revision =
        e.ts.reject_review(RejectReviewInput {
            run_id: first.id.clone(),
            comment: "按钮改成主色 [mock:shellwrite=docs/spec.md]".into(),
        })
        .unwrap();
    assert_eq!(revision.attempt, 2);
    assert_eq!(revision.revision_of.as_deref(), Some(first.id.as_str()));
    assert_eq!(
        e.ts.get_task(&task.id).unwrap().task.status,
        TaskStatus::Doing
    );
    let revision = e.wait_end(&revision.id);
    assert_eq!(revision.status, RunStatus::Succeeded, "{revision:#?}");
    // 同一执行目录、续接原会话。
    assert_eq!(revision.exec_dir, first.exec_dir);
    assert_eq!(revision.session_id, first.session_id);
    let summary = summary(&revision);
    assert!(summary.contains("[revise:按钮改成主色"), "{summary}");
    assert!(summary.contains("[shellwrite:ok]"), "{summary}");
    let prompts = user_messages(&e.updates(&revision.id));
    assert_eq!(prompts.len(), 1, "{prompts:?}");
    assert!(prompts[0].contains("## 退回意见"), "{}", prompts[0]);
    assert!(!prompts[0].contains("## 任务\n"), "{}", prompts[0]);
    assert!(
        std::fs::read_to_string(e.ws.join("docs/spec.md"))
            .unwrap()
            .contains("written by shell")
    );

    let view = e.ts.get_review(&task.id).unwrap();
    assert_eq!(view.run.as_ref().unwrap().id, revision.id);
    assert_eq!(view.rejections.len(), 1);
    assert!(view.rejections[0].comment.starts_with("按钮改成主色"));
    assert!(
        view.files.iter().any(|f| f.path == "docs/spec.md"),
        "{:?}",
        view.files
    );
    assert!(view.can_approve, "{:?}", view.problems);

    // 旧执行不能再验收。
    let err =
        e.ts.approve_review(ApproveReviewInput {
            run_id: first.id.clone(),
            expected_head: None,
        })
        .unwrap_err();
    assert_eq!(err.kind(), teamspace_core::ErrorKind::Conflict);

    let result =
        e.ts.approve_review(ApproveReviewInput {
            run_id: revision.id.clone(),
            expected_head: None,
        })
        .unwrap();
    assert_eq!(result.outcome, ApproveOutcome::Completed);
    assert_eq!(result.task.status, TaskStatus::Done);
    let journal = e.wait_journal("验收通过");
    assert!(
        journal.contains("验收退回，意见：按钮改成主色"),
        "{journal}"
    );
}

#[test]
fn reject_opens_a_new_session_in_the_same_folder_without_load_session() {
    let e = env(false);
    std::fs::write(e.ws.join(".mock-noload"), "").unwrap();
    let task = e.task("登录页");
    let first = run_to_review(&e, &task);
    assert!(!first.capabilities.as_ref().unwrap().load_session);

    let revision =
        e.ts.reject_review(RejectReviewInput {
            run_id: first.id.clone(),
            comment: "补上空状态".into(),
        })
        .unwrap();
    let revision = e.wait_end(&revision.id);
    assert_eq!(revision.status, RunStatus::Succeeded, "{revision:#?}");
    assert_eq!(Path::new(&revision.exec_dir), Path::new(&first.exec_dir));
    let summary = summary(&revision);
    assert!(summary.contains("[revise:补上空状态]"), "{summary}");
    // 新会话：完整任务说明 + 上次摘要 + 退回意见。
    let prompts = user_messages(&e.updates(&revision.id));
    assert!(prompts[0].contains("## 任务"), "{}", prompts[0]);
    assert!(prompts[0].contains("## 退回意见"), "{}", prompts[0]);
    assert!(prompts[0].contains("上次执行的摘要"), "{}", prompts[0]);
    assert_eq!(
        e.ts.get_task(&task.id).unwrap().task.status,
        TaskStatus::Review
    );
}

// ---------------------------------------------------------------- Git 工作空间（真实 Git）

fn git_out(dir: &Path, args: &[&str]) -> String {
    let out = std::process::Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(args)
        .output()
        .unwrap();
    String::from_utf8_lossy(&out.stdout).trim().to_owned()
}

fn branch_exists(e: &Env, branch: &str) -> bool {
    !git_out(&e.ws, &["branch", "--list", branch]).is_empty()
}

fn approve(e: &Env, run: &Run) -> ApproveReviewResult {
    // 先看验收页（会把执行目录里未提交的改动提交到执行分支），再按看到的 head 通过。
    let view = e.ts.get_review(&run.task_id).unwrap();
    e.ts.approve_review(ApproveReviewInput {
        run_id: run.id.clone(),
        expected_head: view.head_commit,
    })
    .unwrap()
}

/// 执行成功后改动已提交到执行分支；验收页列出分支 diff；通过后合并进 main 并清理。
#[test]
fn git_run_is_committed_reviewed_merged_and_cleaned_up() {
    let e = env(true);
    let task = e.task("登录页");
    let run = run_to_review(&e, &task);
    assert_eq!(run.base_branch.as_deref(), Some("main"));
    let branch = run.branch.clone().unwrap();
    // 执行结束时已提交，没有提交失败的提示。
    assert!(run.change_note.is_none(), "{:?}", run.change_note);
    assert_eq!(
        git_out(Path::new(&run.exec_dir), &["status", "--porcelain"]),
        ""
    );
    let log = git_out(&e.ws, &["log", "-1", "--format=%s", &branch]);
    assert_eq!(log, "teamspace: FE-001 attempt 1");

    let view = e.ts.get_review(&task.id).unwrap();
    assert_eq!(view.approve_action, Some(ApproveAction::Merge));
    assert!(view.files_problem.is_none(), "{:?}", view.files_problem);
    assert!(!view.files.is_empty());
    let file = view.files.iter().find(|f| f.has_patch).unwrap();
    let patch = e.ts.get_review_file_patch(&run.id, &file.path).unwrap();
    assert!(patch.patch.contains("Mock 工作记录"), "{}", patch.patch);

    let main_before = git_out(&e.ws, &["rev-parse", "main"]);
    let result = approve(&e, &run);
    assert_eq!(result.outcome, ApproveOutcome::Merged, "{result:#?}");
    assert!(
        result.cleanup_problem.is_none(),
        "{:?}",
        result.cleanup_problem
    );
    assert_eq!(result.task.status, TaskStatus::Done);
    let merged = result.merged_commit.unwrap();
    assert_eq!(git_out(&e.ws, &["rev-parse", "main"]), merged);
    assert_eq!(
        git_out(&e.ws, &["rev-parse", &format!("{merged}^1")]),
        main_before
    );
    assert!(e.ws.join(&file.path).is_file());
    assert!(!Path::new(&run.exec_dir).exists());
    assert!(!branch_exists(&e, &branch));
    assert_eq!(
        git_out(&e.ws, &["log", "-1", "--format=%s", "main"]),
        "merge: FE-001 登录页"
    );
}

/// 基线有未提交改动：不能合并，任务保持待验收。
#[test]
fn dirty_base_blocks_the_merge() {
    let e = env(true);
    let task = e.task("登录页");
    let run = run_to_review(&e, &task);
    std::fs::write(e.ws.join("docs/spec.md"), "# 本地改动\n").unwrap();
    let result = approve(&e, &run);
    assert_eq!(result.outcome, ApproveOutcome::Blocked);
    assert!(result.reason.unwrap().contains("未提交"));
    assert_eq!(result.task.status, TaskStatus::Review);
    assert!(branch_exists(&e, run.branch.as_deref().unwrap()));
}

/// 在 main 上提交与执行分支同路径、不同内容的文件，制造冲突。
fn conflict_on_main(e: &Env, run: &Run) -> String {
    let path = run
        .changed_files
        .iter()
        .find(|f| f.path.ends_with(".md"))
        .unwrap()
        .path
        .clone();
    let target = e.ws.join(&path);
    std::fs::create_dir_all(target.parent().unwrap()).unwrap();
    std::fs::write(&target, "main 上的版本\n").unwrap();
    git(&e.ws, &["add", &path]);
    git(&e.ws, &["commit", "-q", "-m", "conflict"]);
    path
}

#[test]
fn merge_conflict_then_manual_resolve() {
    let e = env(true);
    let task = e.task("登录页");
    let run = run_to_review(&e, &task);
    let path = conflict_on_main(&e, &run);
    let main_before = git_out(&e.ws, &["rev-parse", "main"]);
    let result = approve(&e, &run);
    assert_eq!(result.outcome, ApproveOutcome::Conflict, "{result:#?}");
    assert!(result.conflict_files.contains(&path), "{result:#?}");
    assert_eq!(result.task.status, TaskStatus::Blocked);
    assert_eq!(git_out(&e.ws, &["rev-parse", "main"]), main_before);
    let branch = run.branch.clone().unwrap();
    assert!(branch_exists(&e, &branch));
    assert!(Path::new(&run.exec_dir).is_dir());

    // 未合并时拒绝。
    let input = || ResolveMergeManuallyInput {
        run_id: run.id.clone(),
    };
    let err = e.ts.resolve_merge_manually(input()).unwrap_err();
    assert!(err.to_string().contains("还没有并入"), "{err}");

    // Human 在仓库里手动合并（保留 main 的版本）。
    git(
        &e.ws,
        &["merge", "--no-ff", "-s", "ours", "-m", "manual", &branch],
    );
    let result = e.ts.resolve_merge_manually(input()).unwrap();
    assert_eq!(result.task.status, TaskStatus::Done);
    assert!(
        result.cleanup_problem.is_none(),
        "{:?}",
        result.cleanup_problem
    );
    assert!(!branch_exists(&e, &branch));
    assert!(!Path::new(&run.exec_dir).exists());
}

#[test]
fn merge_conflict_then_rerun_from_latest_base() {
    let e = env(true);
    let task = e.task("登录页");
    let run = run_to_review(&e, &task);
    conflict_on_main(&e, &run);
    assert_eq!(approve(&e, &run).outcome, ApproveOutcome::Conflict);
    let branch = run.branch.clone().unwrap();
    let head = git_out(&e.ws, &["rev-parse", &branch]);

    let detail =
        e.ts.rerun_after_conflict(RerunAfterConflictInput {
            run_id: run.id.clone(),
        })
        .unwrap();
    assert_eq!(detail.task.status, TaskStatus::Ready);
    assert!(!branch_exists(&e, &branch));
    assert!(!Path::new(&run.exec_dir).exists());
    let rerun = detail
        .events
        .iter()
        .find(|ev| ev.kind == EventKind::ReviewRerun)
        .unwrap();
    assert!(rerun.summary.contains(&head), "{}", rerun.summary);

    // 重新执行基于最新的 main。
    let next = e.ts.start_run(&task.id).unwrap();
    let next = e.wait_end(&next.id);
    assert_eq!(next.status, RunStatus::Succeeded, "{next:#?}");
    assert_eq!(
        next.base_commit.as_deref().unwrap(),
        git_out(&e.ws, &["rev-parse", "main"])
    );
}

/// Git：退回后在同一 worktree 续接，再次通过时两轮的改动一起合并。
#[test]
fn git_reject_continues_in_the_worktree_and_merges_both_rounds() {
    let e = env(true);
    let task = e.task("登录页");
    let first = run_to_review(&e, &task);
    let revision =
        e.ts.reject_review(RejectReviewInput {
            run_id: first.id.clone(),
            comment: "规格也要更新 [mock:shellwrite=docs/spec.md]".into(),
        })
        .unwrap();
    let revision = e.wait_end(&revision.id);
    assert_eq!(revision.status, RunStatus::Succeeded, "{revision:#?}");
    assert_eq!(revision.exec_dir, first.exec_dir);
    assert_eq!(revision.branch, first.branch);
    let branch = revision.branch.clone().unwrap();
    assert_eq!(
        git_out(&e.ws, &["log", "-1", "--format=%s", &branch]),
        "teamspace: FE-001 attempt 2"
    );
    let view = e.ts.get_review(&task.id).unwrap();
    assert!(
        view.files.iter().any(|f| f.path == "docs/spec.md"),
        "{:?}",
        view.files
    );
    let result = approve(&e, &revision);
    assert_eq!(result.outcome, ApproveOutcome::Merged, "{result:#?}");
    assert!(
        std::fs::read_to_string(e.ws.join("docs/spec.md"))
            .unwrap()
            .contains("written by shell")
    );
}

/// #4 / #6：执行目录被切到别的分支时，验收页不提交、不能通过；在执行目录里改了文件但没提交时，验收页
/// 先把它们提交到执行分支，预览与合并都包含它们。
#[test]
fn review_commits_pending_changes_and_checks_the_branch() {
    let e = env(true);
    let task = e.task("登录页");
    let run = run_to_review(&e, &task);
    let exec = Path::new(&run.exec_dir);
    let before = git_out(&e.ws, &["rev-parse", run.branch.as_deref().unwrap()]);
    std::fs::write(exec.join("docs/spec.md"), "# 人工补充\n").unwrap();
    let view = e.ts.get_review(&task.id).unwrap();
    let head = view.head_commit.clone().unwrap();
    assert_ne!(head, before);
    assert!(
        view.files.iter().any(|f| f.path == "docs/spec.md"),
        "{:?}",
        view.files
    );

    git(exec, &["checkout", "-q", "-b", "elsewhere"]);
    let view = e.ts.get_review(&task.id).unwrap();
    assert!(!view.can_approve);
    assert!(
        view.files_problem
            .as_deref()
            .unwrap_or_default()
            .contains("执行目录不在执行分支"),
        "{view:#?}"
    );
    let result =
        e.ts.approve_review(ApproveReviewInput {
            run_id: run.id.clone(),
            expected_head: Some(head.clone()),
        })
        .unwrap();
    assert_eq!(result.outcome, ApproveOutcome::Blocked);
    assert!(result.reason.unwrap().contains("执行目录不在执行分支"));

    git(exec, &["checkout", "-q", run.branch.as_deref().unwrap()]);
    let result = approve(&e, &run);
    assert_eq!(result.outcome, ApproveOutcome::Merged, "{result:#?}");
    assert_eq!(
        std::fs::read_to_string(e.ws.join("docs/spec.md")).unwrap(),
        "# 人工补充\n"
    );
}
