//! 验收（M3-01）的服务层规则：通过 / 退回的 fencing、意见必填、REVIEW → DONE 收口、普通文件夹直接完成、
//! Git 的合并成功 / 冲突 / 受阻 / 清理失败（假 Git 层），以及验收页数据。
//! 执行记录直接写入数据库，不启动 Agent（退回后续接的执行会因 Mock 宿主不存在而启动失败，不影响断言）；
//! 真实子进程的全流程见 `tests/reviews.rs`。

use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use super::*;
use crate::run::review_seam::{Cleanup, DiffEntry, Merge, PatchText, ReviewGit};

/// 假 Git 层：按配置返回结果并记下调用。
#[derive(Default)]
struct FakeGit {
    merge: Mutex<Option<Merge>>,
    commit_err: Option<String>,
    remove_err: Option<String>,
    diff_err: Option<String>,
    /// `is_merged_into` 的结果（默认未合并）。
    merged_into: Option<bool>,
    merge_delay: Duration,
    calls: Mutex<Vec<String>>,
}

impl FakeGit {
    fn with_merge(merge: Merge) -> Self {
        Self {
            merge: Mutex::new(Some(merge)),
            ..Self::default()
        }
    }

    fn calls(&self) -> Vec<String> {
        self.calls.lock().unwrap().clone()
    }

    fn log(&self, call: String) {
        self.calls.lock().unwrap().push(call);
    }
}

impl ReviewGit for FakeGit {
    fn commit_all(
        &self,
        dir: &Path,
        message: &str,
        _: Option<&std::ffi::OsStr>,
    ) -> std::result::Result<Option<String>, String> {
        self.log(format!("commit {} {message}", dir.display()));
        match &self.commit_err {
            Some(err) => Err(err.clone()),
            None => Ok(Some("c0ffee".into())),
        }
    }

    fn diff_files(
        &self,
        _: &Path,
        base: &str,
        head: &str,
        _: Option<&std::ffi::OsStr>,
    ) -> std::result::Result<Vec<DiffEntry>, String> {
        self.log(format!("diff {base}..{head}"));
        if let Some(err) = &self.diff_err {
            return Err(err.clone());
        }
        Ok(vec![
            DiffEntry {
                path: "src/app.ts".into(),
                change: FileChangeKind::Modified,
                binary: false,
                added: 3,
                removed: 1,
            },
            DiffEntry {
                path: "logo.png".into(),
                change: FileChangeKind::Added,
                binary: true,
                added: 0,
                removed: 0,
            },
        ])
    }

    fn file_patch(
        &self,
        _: &Path,
        base: &str,
        head: &str,
        path: &str,
        _: Option<&std::ffi::OsStr>,
    ) -> std::result::Result<PatchText, String> {
        self.log(format!("patch {base}..{head} {path}"));
        if path != "src/app.ts" {
            return Err(format!("{path} 不在改动文件中"));
        }
        Ok(PatchText {
            binary: false,
            truncated: false,
            patch: "@@ -1 +1 @@\n-a\n+b\n".into(),
        })
    }

    fn merge_into_base(
        &self,
        _: &Path,
        base: &str,
        branch: &str,
        message: &str,
        _: Option<&std::ffi::OsStr>,
    ) -> std::result::Result<Merge, String> {
        self.log(format!("merge {branch} -> {base}: {message}"));
        std::thread::sleep(self.merge_delay);
        self.merge
            .lock()
            .unwrap()
            .clone()
            .ok_or_else(|| "没有配置合并结果".to_owned())
    }

    fn remove_run_worktree(
        &self,
        _: &Path,
        root: &Path,
        branch: &str,
        cleanup: Cleanup,
        _: Option<&std::ffi::OsStr>,
    ) -> std::result::Result<(), String> {
        let how = match cleanup {
            Cleanup::Merged => "",
            Cleanup::Force => " --force",
        };
        self.log(format!("remove {} {branch}{how}", root.display()));
        match &self.remove_err {
            Some(err) => Err(err.clone()),
            None => Ok(()),
        }
    }

    fn is_merged_into(
        &self,
        _: &Path,
        branch: &str,
        base: &str,
        _: Option<&std::ffi::OsStr>,
    ) -> std::result::Result<bool, String> {
        self.log(format!("is-merged {branch} {base}"));
        Ok(self.merged_into.unwrap_or(false))
    }

    fn branch_head(&self, _: &Path, branch: &str, _: Option<&std::ffi::OsStr>) -> Option<String> {
        Some(format!("head-of-{branch}"))
    }

    fn base_branch(&self, _: &crate::run::worktree::Worktree) -> Option<String> {
        Some("main".into())
    }
}

struct Seeded {
    team: Team,
    task: Task,
    harness: HarnessInstance,
    /// 待验收的执行。
    run_id: String,
    /// worktree 执行的执行目录（worktree 根下的子目录 `pkg`）。
    exec_dir: PathBuf,
}

/// 一个处于 REVIEW 的任务与一次成功的执行（`git` 为真时是 worktree 执行，否则原地执行）。
fn seed(f: &Fixture, git: bool) -> Seeded {
    let team = f.team("T");
    let fe = f.role(&team.id, "role_frontend");
    let h = f.harness(&team.id, "Mock", AcpPreset::Mock);
    f.ts.bind_role_harness(&fe.id, &h.id).unwrap();
    let task = f.readyable(&team.id, &fe, "登录页").task;
    let exec_dir = if git {
        f.dir
            .path()
            .join("worktrees")
            .join(&team.id)
            .join(format!("{}-r1", task.key))
            .join("pkg")
    } else {
        f.ws.clone()
    };
    std::fs::create_dir_all(&exec_dir).unwrap();
    let run_id = add_run(
        f,
        &task,
        &h,
        &exec_dir,
        git,
        "'[{\"path\":\"a.md\",\"change\":\"added\"}]'",
    );
    f.raw_sql(&format!(
        "UPDATE tasks SET status = 'review' WHERE id = '{}'",
        task.id
    ));
    let task = f.ts.get_task(&task.id).unwrap().task;
    Seeded {
        team,
        task,
        harness: h,
        run_id,
        exec_dir,
    }
}

/// 追加一次成功的执行（序号递增），返回 ID。
fn add_run(
    f: &Fixture,
    task: &Task,
    h: &HarnessInstance,
    exec_dir: &Path,
    git: bool,
    changed: &str,
) -> String {
    let id = crate::store::new_id();
    let (isolation, branch, base, base_branch) = if git {
        (
            "worktree",
            format!("'teamspace/{}-r1'", task.key),
            "'abc123'",
            "'main'",
        )
    } else {
        ("in_place", "NULL".to_owned(), "NULL", "NULL")
    };
    f.raw_sql(&format!(
        "INSERT INTO runs (id, team_id, task_id, role_id, harness_id, harness_name, harness_preset,
                           harness_command, attempt, status, session_id, workspace_path, exec_dir,
                           isolation, branch, base_commit, base_branch, capabilities, started_at,
                           ended_at, stop_reason, summary, changed_files)
         VALUES ('{id}', '{team}', '{task_id}', '{role}', '{h}', 'Mock', 'mock', 'x',
                 (SELECT COALESCE(MAX(attempt), 0) + 1 FROM runs WHERE task_id = '{task_id}'),
                 'succeeded', 's-1', '{ws}', '{exec}', '{isolation}', {branch}, {base}, {base_branch},
                 '{{\"loadSession\":true,\"image\":false,\"audio\":false,\"embeddedContext\":false,\"mcpHttp\":false,\"mcpSse\":false}}',
                 '2026-10-13T00:00:00.000Z', '2026-10-13T00:01:00.000Z', 'end_turn', '做完了登录页',
                 {changed});",
        team = task.team_id,
        task_id = task.id,
        role = task.owner_role_id,
        h = h.id,
        ws = f.ws.display(),
        exec = exec_dir.display(),
    ));
    id
}

fn fake(f: &Fixture, git: FakeGit) -> Arc<FakeGit> {
    let git = Arc::new(git);
    f.ts.set_review_git(git.clone());
    git
}

fn approve(f: &Fixture, run_id: &str) -> Result<ApproveReviewResult> {
    f.ts.approve_review(ApproveReviewInput {
        run_id: run_id.into(),
    })
}

fn reject(f: &Fixture, run_id: &str, comment: &str) -> Result<Run> {
    f.ts.reject_review(RejectReviewInput {
        run_id: run_id.into(),
        comment: comment.into(),
    })
}

fn kinds(f: &Fixture, task_id: &str) -> Vec<EventKind> {
    f.ts.get_task(task_id)
        .unwrap()
        .events
        .iter()
        .map(|e| e.kind)
        .collect()
}

fn wait_end(f: &Fixture, run_id: &str) -> Run {
    let deadline = Instant::now() + Duration::from_secs(20);
    loop {
        let run = f.ts.get_run(run_id).unwrap();
        if !run.status.is_active() {
            return run;
        }
        assert!(Instant::now() < deadline, "run did not end: {run:#?}");
        std::thread::sleep(Duration::from_millis(20));
    }
}

#[test]
fn in_place_review_shows_files_and_approve_completes_the_task() {
    let f = plain_fixture();
    let s = seed(&f, false);
    let view = f.ts.get_review(&s.task.id).unwrap();
    assert_eq!(view.run.as_ref().unwrap().id, s.run_id);
    assert_eq!(view.summary.as_deref(), Some("做完了登录页"));
    assert_eq!(view.isolation, Some(RunIsolation::InPlace));
    assert_eq!(view.approve_action, Some(ApproveAction::Complete));
    assert!(view.can_approve && view.can_reject, "{:?}", view.problems);
    assert!(view.problems.is_empty());
    assert_eq!(view.files.len(), 1);
    assert_eq!(view.files[0].path, "a.md");
    assert!(!view.files[0].has_patch);
    assert_eq!(view.files[0].added, None);
    assert!(view.rejections.is_empty());

    let result = approve(&f, &s.run_id).unwrap();
    assert_eq!(result.outcome, ApproveOutcome::Completed);
    assert_eq!(result.task.status, TaskStatus::Done);
    assert_eq!(
        result.run.review.as_ref().map(|r| r.outcome),
        Some(ReviewOutcome::Approved)
    );
    assert!(kinds(&f, &s.task.id).contains(&EventKind::ReviewApproved));
    // 岗位工作日志里有验收通过的条目（D14）。
    f.ts.get_team_overview(&s.team.id).unwrap();
    let folder = f.ts.get_role_folder(&s.task.owner_role_id, None).unwrap();
    assert!(
        folder.journal.iter().any(|l| l.contains("验收通过")),
        "{:?}",
        folder.journal
    );

    // 已完成后：没有待验收的执行，不能再通过或退回。
    let view = f.ts.get_review(&s.task.id).unwrap();
    assert!(view.run.is_none());
    assert!(!view.can_approve && !view.can_reject);
    assert!(
        view.problems[0].contains("不在待验收"),
        "{:?}",
        view.problems
    );
    let err = assert_kind(approve(&f, &s.run_id), ErrorKind::Conflict);
    assert!(err.contains("不在待验收"), "{err}");
}

#[test]
fn approve_and_reject_only_accept_the_latest_run_of_a_review_task() {
    let f = plain_fixture();
    let s = seed(&f, false);
    // 第 2 次执行出现后，第 1 次就是旧执行。
    let newer = add_run(&f, &s.task, &s.harness, &s.exec_dir, false, "'[]'");
    let err = assert_kind(approve(&f, &s.run_id), ErrorKind::Conflict);
    assert!(
        err.contains("不是任务") && err.contains("最近一次是第 2 次"),
        "{err}"
    );
    let err = assert_kind(reject(&f, &s.run_id, "再改改"), ErrorKind::Conflict);
    assert!(err.contains("最近一次是第 2 次"), "{err}");

    // 任务不在 REVIEW 时都拒绝。
    f.move_to(&s.task.id, TaskStatus::Ready).unwrap();
    let err = assert_kind(approve(&f, &newer), ErrorKind::Conflict);
    assert!(err.contains("当前是「READY」"), "{err}");
    assert_kind(reject(&f, &newer, "再改改"), ErrorKind::Conflict);
    assert_eq!(
        f.ts.get_task(&s.task.id).unwrap().task.status,
        TaskStatus::Ready
    );
    assert!(f.ts.get_run(&newer).unwrap().review.is_none());

    // 不存在的执行。
    assert_kind(approve(&f, "missing"), ErrorKind::NotFound);
}

#[test]
fn reject_requires_a_comment() {
    let f = plain_fixture();
    let s = seed(&f, false);
    for blank in ["", "   ", "\n\t "] {
        let err = assert_kind(reject(&f, &s.run_id, blank), ErrorKind::Invalid);
        assert_eq!(err, "退回意见不能为空");
    }
    let long = "改".repeat(2001);
    assert_kind(reject(&f, &s.run_id, &long), ErrorKind::Invalid);
    // 被拒绝时什么也没变。
    assert_eq!(f.ts.list_runs(&s.task.id).unwrap().len(), 1);
    assert_eq!(
        f.ts.get_task(&s.task.id).unwrap().task.status,
        TaskStatus::Review
    );
}

#[test]
fn reject_continues_in_the_same_directory_and_keeps_the_comment() {
    let f = plain_fixture();
    let s = seed(&f, false);
    let revision = reject(&f, &s.run_id, "  按钮颜色不对，改成主色  ").unwrap();
    assert_eq!(revision.status, RunStatus::Queued);
    assert_eq!(revision.attempt, 2);
    assert_eq!(revision.revision_of.as_deref(), Some(s.run_id.as_str()));
    assert_eq!(revision.exec_dir, s.exec_dir.display().to_string());
    assert_eq!(revision.isolation, RunIsolation::InPlace);
    // 续接原会话：沿用会话 ID。
    assert_eq!(revision.session_id.as_deref(), Some("s-1"));

    let rejected = f.ts.get_run(&s.run_id).unwrap();
    let review = rejected.review.unwrap();
    assert_eq!(review.outcome, ReviewOutcome::Rejected);
    assert_eq!(review.comment.as_deref(), Some("按钮颜色不对，改成主色"));

    let detail = f.ts.get_task(&s.task.id).unwrap();
    assert!(
        detail
            .events
            .iter()
            .any(|e| e.kind == EventKind::ReviewRejected
                && e.summary.contains("按钮颜色不对，改成主色")),
        "{:?}",
        detail.events
    );
    // Mock 宿主不存在，续接的执行启动失败，任务转为受阻；退回意见仍然保留。
    wait_end(&f, &revision.id);
    let view = f.ts.get_review(&s.task.id).unwrap();
    assert_eq!(view.rejections.len(), 1);
    assert_eq!(view.rejections[0].attempt, 1);
    assert_eq!(view.rejections[0].comment, "按钮颜色不对，改成主色");
    // 退回的执行不能再被验收。
    assert_kind(approve(&f, &s.run_id), ErrorKind::Conflict);
}

#[test]
fn reject_of_a_git_run_reuses_the_worktree_and_branch() {
    let f = fixture();
    let s = seed(&f, true);
    let git = fake(&f, FakeGit::default());
    let revision = reject(&f, &s.run_id, "补上单元测试").unwrap();
    assert_eq!(revision.exec_dir, s.exec_dir.display().to_string());
    assert_eq!(revision.isolation, RunIsolation::Worktree);
    assert_eq!(revision.branch.as_deref(), Some("teamspace/FE-001-r1"));
    assert_eq!(revision.base_commit.as_deref(), Some("abc123"));
    assert_eq!(revision.base_branch.as_deref(), Some("main"));
    wait_end(&f, &revision.id);
    // 没有新建 worktree：worktrees/<团队> 下仍只有第 1 次执行的目录。
    let dirs: Vec<_> = std::fs::read_dir(f.dir.path().join("worktrees").join(&s.team.id))
        .unwrap()
        .collect();
    assert_eq!(dirs.len(), 1);
    assert!(git.calls().is_empty(), "{:?}", git.calls());
}

#[test]
fn reject_refuses_when_the_exec_dir_is_gone() {
    let f = fixture();
    let s = seed(&f, true);
    std::fs::remove_dir_all(&s.exec_dir).unwrap();
    let view = f.ts.get_review(&s.task.id).unwrap();
    assert!(view.can_approve);
    assert!(!view.can_reject);
    assert!(
        view.problems.iter().any(|p| p.contains("执行目录已不存在")),
        "{:?}",
        view.problems
    );
    let err = assert_kind(reject(&f, &s.run_id, "再改"), ErrorKind::Conflict);
    assert!(err.contains("执行目录已不存在"), "{err}");
}

#[test]
fn git_review_cannot_be_completed_through_change_task_status() {
    let f = fixture();
    let s = seed(&f, true);
    let detail = f.ts.get_task(&s.task.id).unwrap();
    assert!(!detail.allowed_transitions.contains(&TaskStatus::Done));
    assert!(detail.allowed_transitions.contains(&TaskStatus::Ready));
    let err = assert_kind(f.move_to(&s.task.id, TaskStatus::Done), ErrorKind::Conflict);
    assert!(err.contains("请在验收页「通过」"), "{err}");
    // READY / CANCELLED 维持原规则。
    f.move_to(&s.task.id, TaskStatus::Ready).unwrap();

    // 普通文件夹保留原行为：可以直接改为已完成。
    let p = plain_fixture();
    let s = seed(&p, false);
    assert!(
        p.ts.get_task(&s.task.id)
            .unwrap()
            .allowed_transitions
            .contains(&TaskStatus::Done)
    );
    p.move_to(&s.task.id, TaskStatus::Done).unwrap();
}

#[test]
fn git_review_lists_branch_diff_and_patches() {
    let f = fixture();
    let s = seed(&f, true);
    let git = fake(&f, FakeGit::default());
    let view = f.ts.get_review(&s.task.id).unwrap();
    assert_eq!(view.approve_action, Some(ApproveAction::Merge));
    assert_eq!(view.base_branch.as_deref(), Some("main"));
    assert_eq!(view.branch.as_deref(), Some("teamspace/FE-001-r1"));
    assert!(view.files_problem.is_none());
    let paths: Vec<_> = view.files.iter().map(|f| f.path.as_str()).collect();
    assert_eq!(paths, ["src/app.ts", "logo.png"]);
    assert!(view.files[0].has_patch);
    assert_eq!(
        (view.files[0].added, view.files[0].removed),
        (Some(3), Some(1))
    );
    assert!(view.files[1].binary && !view.files[1].has_patch);
    assert_eq!(git.calls(), ["diff abc123..teamspace/FE-001-r1"]);

    let patch = f.ts.get_review_file_patch(&s.run_id, "src/app.ts").unwrap();
    assert_eq!(patch.run_id, s.run_id);
    assert!(patch.patch.contains("+b"));
    let err = assert_kind(
        f.ts.get_review_file_patch(&s.run_id, "nope.txt"),
        ErrorKind::Conflict,
    );
    assert!(err.contains("nope.txt"), "{err}");

    // diff 读不到时退回执行记录里的改动文件（没有 diff）。
    fake(
        &f,
        FakeGit {
            diff_err: Some("分支不存在".into()),
            ..FakeGit::default()
        },
    );
    let view = f.ts.get_review(&s.task.id).unwrap();
    assert!(view.files_problem.unwrap().contains("分支不存在"));
    assert_eq!(view.files.len(), 1);
    assert!(!view.files[0].has_patch);
}

#[test]
fn in_place_runs_have_no_patch() {
    let f = plain_fixture();
    let s = seed(&f, false);
    let err = assert_kind(
        f.ts.get_review_file_patch(&s.run_id, "a.md"),
        ErrorKind::Invalid,
    );
    assert!(err.contains("普通文件夹"), "{err}");
}

#[test]
fn git_approve_merges_completes_and_cleans_up() {
    let f = fixture();
    let s = seed(&f, true);
    let git = fake(
        &f,
        FakeGit::with_merge(Merge::Merged {
            commit: "1234567890abcdef".into(),
        }),
    );
    let result = approve(&f, &s.run_id).unwrap();
    assert_eq!(result.outcome, ApproveOutcome::Merged);
    assert_eq!(result.merged_commit.as_deref(), Some("1234567890abcdef"));
    assert!(result.cleanup_problem.is_none());
    assert_eq!(result.task.status, TaskStatus::Done);
    assert_eq!(
        result.run.merged_commit.as_deref(),
        Some("1234567890abcdef")
    );
    let root = s.exec_dir.parent().unwrap();
    assert_eq!(
        git.calls(),
        [
            format!(
                "commit {} teamspace: FE-001 attempt 1 (review)",
                s.exec_dir.display()
            ),
            "merge teamspace/FE-001-r1 -> main: merge: FE-001 登录页".to_owned(),
            format!("remove {} teamspace/FE-001-r1", root.display()),
        ]
    );
    let events = f.ts.get_task(&s.task.id).unwrap().events;
    let approved = events
        .iter()
        .find(|e| e.kind == EventKind::ReviewApproved)
        .unwrap();
    assert!(
        approved.summary.contains("合并进 main"),
        "{}",
        approved.summary
    );
}

#[test]
fn cleanup_failure_after_merge_is_reported_but_not_rolled_back() {
    let f = fixture();
    let s = seed(&f, true);
    fake(
        &f,
        FakeGit {
            remove_err: Some("worktree 被占用".into()),
            ..FakeGit::with_merge(Merge::Merged {
                commit: "beef".into(),
            })
        },
    );
    let result = approve(&f, &s.run_id).unwrap();
    assert_eq!(result.outcome, ApproveOutcome::Merged);
    assert_eq!(result.task.status, TaskStatus::Done);
    assert!(result.cleanup_problem.unwrap().contains("worktree 被占用"));
    assert!(kinds(&f, &s.task.id).contains(&EventKind::ReviewCleanupFailed));
}

#[test]
fn merge_conflict_blocks_the_task_and_keeps_the_branch() {
    let f = fixture();
    let s = seed(&f, true);
    let git = fake(
        &f,
        FakeGit::with_merge(Merge::Conflict {
            files: vec!["src/app.ts".into(), "README.md".into()],
        }),
    );
    let result = approve(&f, &s.run_id).unwrap();
    assert_eq!(result.outcome, ApproveOutcome::Conflict);
    assert_eq!(result.conflict_files, ["src/app.ts", "README.md"]);
    assert_eq!(result.task.status, TaskStatus::Blocked);
    let reason = result.task.blocked_reason.unwrap();
    assert!(reason.contains("src/app.ts、README.md"), "{reason}");
    assert!(reason.contains("基线分支 main 未改变"), "{reason}");
    assert_eq!(
        result.run.review.as_ref().map(|r| r.outcome),
        Some(ReviewOutcome::Conflict)
    );
    assert!(git.calls().iter().all(|c| !c.starts_with("remove")));
    assert!(kinds(&f, &s.task.id).contains(&EventKind::ReviewConflict));
}

#[test]
fn blocked_merge_keeps_the_task_in_review() {
    let f = fixture();
    let s = seed(&f, true);
    fake(
        &f,
        FakeGit::with_merge(Merge::Blocked {
            reason: "基线分支 main 有未提交的改动".into(),
        }),
    );
    let result = approve(&f, &s.run_id).unwrap();
    assert_eq!(result.outcome, ApproveOutcome::Blocked);
    assert!(result.reason.unwrap().contains("未提交的改动"));
    assert_eq!(result.task.status, TaskStatus::Review);
    assert!(result.run.review.is_none());
    assert!(kinds(&f, &s.task.id).contains(&EventKind::ReviewMergeBlocked));

    // 提交执行目录失败、合并层报错，同样保持待验收。
    let git = fake(
        &f,
        FakeGit {
            commit_err: Some("索引被锁".into()),
            ..FakeGit::default()
        },
    );
    let result = approve(&f, &s.run_id).unwrap();
    assert_eq!(result.outcome, ApproveOutcome::Blocked);
    assert!(result.reason.unwrap().contains("索引被锁"));
    assert!(git.calls().iter().all(|c| !c.starts_with("merge")));
    fake(&f, FakeGit::default());
    let result = approve(&f, &s.run_id).unwrap();
    assert_eq!(result.outcome, ApproveOutcome::Blocked);
    assert!(result.reason.unwrap().starts_with("合并失败"));
    assert_eq!(result.task.status, TaskStatus::Review);
}

#[test]
fn concurrent_approvals_merge_only_once() {
    let f = fixture();
    let s = seed(&f, true);
    let git = fake(
        &f,
        FakeGit {
            merge_delay: Duration::from_millis(300),
            ..FakeGit::with_merge(Merge::Merged {
                commit: "cafe".into(),
            })
        },
    );
    let results: Vec<Result<ApproveReviewResult>> = std::thread::scope(|scope| {
        let handles: Vec<_> = (0..2)
            .map(|_| scope.spawn(|| approve(&f, &s.run_id)))
            .collect();
        handles.into_iter().map(|h| h.join().unwrap()).collect()
    });
    let ok = results.iter().filter(|r| r.is_ok()).count();
    assert_eq!(ok, 1, "{results:?}");
    let err = results.into_iter().find_map(|r| r.err()).unwrap();
    assert_eq!(err.kind(), ErrorKind::Conflict);
    let merges = git
        .calls()
        .iter()
        .filter(|c| c.starts_with("merge"))
        .count();
    assert_eq!(merges, 1);

    // 普通文件夹：并发通过只有一个成功。
    let p = plain_fixture();
    let s = seed(&p, false);
    let results: Vec<Result<ApproveReviewResult>> = std::thread::scope(|scope| {
        let handles: Vec<_> = (0..4)
            .map(|_| scope.spawn(|| approve(&p, &s.run_id)))
            .collect();
        handles.into_iter().map(|h| h.join().unwrap()).collect()
    });
    assert_eq!(results.iter().filter(|r| r.is_ok()).count(), 1);
    let approved = kinds(&p, &s.task.id)
        .into_iter()
        .filter(|k| *k == EventKind::ReviewApproved)
        .count();
    assert_eq!(approved, 1);
}

#[test]
fn in_place_review_merges_files_across_rejected_rounds() {
    let f = plain_fixture();
    let s = seed(&f, false);
    // 第 1 次执行被退回，第 2 次执行续接（revision_of），各自改了一些文件。
    let second = add_run(
        &f,
        &s.task,
        &s.harness,
        &s.exec_dir,
        false,
        "'[{\"path\":\"a.md\",\"change\":\"modified\"},{\"path\":\"b.md\",\"change\":\"modified\"}]'",
    );
    f.raw_sql(&format!(
        "UPDATE runs SET review_outcome = 'rejected', review_comment = '再改改',
                         reviewed_at = '2026-10-13T00:02:00.000Z' WHERE id = '{first}';
         UPDATE runs SET revision_of = '{first}' WHERE id = '{second}';",
        first = s.run_id
    ));
    let view = f.ts.get_review(&s.task.id).unwrap();
    assert_eq!(view.run.unwrap().id, second);
    let files: Vec<_> = view
        .files
        .iter()
        .map(|f| (f.path.as_str(), f.change))
        .collect();
    // a.md 先新增后修改，仍算新增。
    assert_eq!(
        files,
        [
            ("a.md", FileChangeKind::Added),
            ("b.md", FileChangeKind::Modified)
        ]
    );
    assert_eq!(view.rejections.len(), 1);
    assert_eq!(view.rejections[0].comment, "再改改");
}

#[test]
fn manual_review_without_runs_cannot_be_approved_here() {
    let f = plain_fixture();
    let team = f.team("T");
    let fe = f.role(&team.id, "role_frontend");
    let t = f.readyable(&team.id, &fe, "手动");
    f.drive(
        &t.task.id,
        &[TaskStatus::Ready, TaskStatus::Doing, TaskStatus::Review],
    );
    let view = f.ts.get_review(&t.task.id).unwrap();
    assert!(view.run.is_none());
    assert!(!view.can_approve && !view.can_reject);
    assert!(view.problems[0].contains("没有可验收的执行记录"));
    // 仍可用状态流转完成。
    f.move_to(&t.task.id, TaskStatus::Done).unwrap();
}

/// 让任务因合并冲突受阻（经 `approve_review`）。
fn conflicted(f: &Fixture, s: &Seeded) {
    fake(
        f,
        FakeGit::with_merge(Merge::Conflict {
            files: vec!["src/app.ts".into()],
        }),
    );
    let result = approve(f, &s.run_id).unwrap();
    assert_eq!(result.outcome, ApproveOutcome::Conflict);
}

#[test]
fn conflict_view_offers_manual_resolve_and_rerun() {
    let f = fixture();
    let s = seed(&f, true);
    // 没有冲突时不能用这两个命令。
    let err = assert_kind(
        f.ts.resolve_merge_manually(ResolveMergeManuallyInput {
            run_id: s.run_id.clone(),
        }),
        ErrorKind::Conflict,
    );
    assert!(err.contains("合并冲突"), "{err}");
    assert_kind(
        f.ts.rerun_after_conflict(RerunAfterConflictInput {
            run_id: s.run_id.clone(),
        }),
        ErrorKind::Conflict,
    );

    conflicted(&f, &s);
    let view = f.ts.get_review(&s.task.id).unwrap();
    assert!(view.conflict);
    assert_eq!(view.run.as_ref().unwrap().id, s.run_id);
    assert!(!view.can_approve && !view.can_reject);
    assert!(
        view.can_resolve_manually && view.can_rerun,
        "{:?}",
        view.problems
    );
    // 冲突时也不能通过或退回。
    assert_kind(approve(&f, &s.run_id), ErrorKind::Conflict);
    assert_kind(reject(&f, &s.run_id, "改"), ErrorKind::Conflict);
}

#[test]
fn manual_resolve_requires_the_branch_to_be_merged() {
    let f = fixture();
    let s = seed(&f, true);
    conflicted(&f, &s);
    let input = || ResolveMergeManuallyInput {
        run_id: s.run_id.clone(),
    };
    fake(&f, FakeGit::default());
    let err = assert_kind(f.ts.resolve_merge_manually(input()), ErrorKind::Conflict);
    assert!(err.contains("还没有并入 main"), "{err}");
    assert_eq!(
        f.ts.get_task(&s.task.id).unwrap().task.status,
        TaskStatus::Blocked
    );

    let git = fake(
        &f,
        FakeGit {
            merged_into: Some(true),
            ..FakeGit::default()
        },
    );
    let result = f.ts.resolve_merge_manually(input()).unwrap();
    assert_eq!(result.outcome, ApproveOutcome::Merged);
    assert_eq!(result.task.status, TaskStatus::Done);
    assert_eq!(result.merged_commit.as_deref(), Some("head-of-main"));
    assert_eq!(
        result.run.review.as_ref().map(|r| r.outcome),
        Some(ReviewOutcome::Approved)
    );
    let root = s.exec_dir.parent().unwrap();
    assert_eq!(
        git.calls(),
        [
            "is-merged teamspace/FE-001-r1 main".to_owned(),
            format!("remove {} teamspace/FE-001-r1", root.display()),
        ]
    );
    let events = f.ts.get_task(&s.task.id).unwrap().events;
    assert!(
        events
            .iter()
            .any(|e| e.kind == EventKind::ReviewApproved && e.summary.contains("已手动解决"))
    );
    // 再来一次：任务已完成，fencing 拒绝。
    assert_kind(f.ts.resolve_merge_manually(input()), ErrorKind::Conflict);
}

#[test]
fn rerun_after_conflict_records_the_branch_then_force_removes_it() {
    let f = fixture();
    let s = seed(&f, true);
    conflicted(&f, &s);
    let git = fake(&f, FakeGit::default());
    let detail =
        f.ts.rerun_after_conflict(RerunAfterConflictInput {
            run_id: s.run_id.clone(),
        })
        .unwrap();
    assert_eq!(detail.task.status, TaskStatus::Ready);
    assert!(detail.task.blocked_reason.is_none());
    let root = s.exec_dir.parent().unwrap();
    assert_eq!(
        git.calls(),
        [format!(
            "remove {} teamspace/FE-001-r1 --force",
            root.display()
        )]
    );
    let rerun = detail
        .events
        .iter()
        .find(|e| e.kind == EventKind::ReviewRerun)
        .unwrap();
    assert!(
        rerun.summary.contains("teamspace/FE-001-r1")
            && rerun.summary.contains("head-of-teamspace/FE-001-r1"),
        "{}",
        rerun.summary
    );
    // 之后可以正常开始执行。
    assert!(detail.run_problems.is_empty(), "{:?}", detail.run_problems);
}

#[test]
fn rerun_after_conflict_requires_readiness() {
    let f = fixture();
    let s = seed(&f, true);
    conflicted(&f, &s);
    std::fs::remove_file(f.ws.join("docs/spec.md")).unwrap();
    let view = f.ts.get_review(&s.task.id).unwrap();
    assert!(view.can_resolve_manually && !view.can_rerun);
    let git = fake(&f, FakeGit::default());
    let err = assert_kind(
        f.ts.rerun_after_conflict(RerunAfterConflictInput {
            run_id: s.run_id.clone(),
        }),
        ErrorKind::Conflict,
    );
    assert!(err.starts_with("不能重新执行："), "{err}");
    assert!(git.calls().is_empty());
}

#[test]
fn open_run_file_opens_files_inside_the_exec_dir_only() {
    let f = plain_fixture();
    let s = seed(&f, false);
    std::fs::create_dir_all(f.ws.join("docs")).unwrap();
    std::fs::write(f.ws.join("docs/a.md"), "x").unwrap();
    std::fs::write(f.dir.path().join("secret.md"), "x").unwrap();
    std::os::unix::fs::symlink(f.dir.path().join("secret.md"), f.ws.join("link.md")).unwrap();

    let before = REVEALED.with(|r| r.borrow().len());
    f.ts.open_run_file(&s.run_id, "docs/a.md").unwrap();
    let opened = REVEALED.with(|r| r.borrow().last().cloned()).unwrap();
    assert_eq!(
        opened.canonicalize().unwrap(),
        f.ws.join("docs/a.md").canonicalize().unwrap()
    );
    for bad in ["", "../secret.md", "/etc/hosts", "link.md", "docs"] {
        assert_kind(f.ts.open_run_file(&s.run_id, bad), ErrorKind::Invalid);
    }
    assert_kind(
        f.ts.open_run_file(&s.run_id, "nope.md"),
        ErrorKind::NotFound,
    );
    assert_kind(f.ts.open_run_file("nope", "docs/a.md"), ErrorKind::NotFound);
    assert_eq!(
        REVEALED.with(|r| r.borrow().len()),
        before + 1,
        "被拒绝的路径不应打开"
    );
}
