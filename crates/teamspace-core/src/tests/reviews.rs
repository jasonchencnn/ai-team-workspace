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
pub(crate) struct FakeGit {
    merge: Mutex<Option<Merge>>,
    commit_err: Option<String>,
    remove_err: Option<String>,
    diff_err: Option<String>,
    /// `is_merged_into` 的结果（默认未合并）。
    merged_into: Option<bool>,
    merge_delay: Duration,
    /// 设置时 `check_on_branch` 报告执行目录不在执行分支上。
    wrong_branch: Option<String>,
    /// 同时在合并中的数量与观察到的最大值（串行化测试）。
    in_flight: std::sync::atomic::AtomicUsize,
    max_in_flight: std::sync::atomic::AtomicUsize,
    calls: Mutex<Vec<String>>,
}

impl FakeGit {
    fn with_merge(merge: Merge) -> Self {
        Self {
            merge: Mutex::new(Some(merge)),
            ..Self::default()
        }
    }

    pub(crate) fn calls(&self) -> Vec<String> {
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
        use std::sync::atomic::Ordering::SeqCst;
        self.log(format!("merge {branch} -> {base}: {message}"));
        let now = self.in_flight.fetch_add(1, SeqCst) + 1;
        self.max_in_flight.fetch_max(now, SeqCst);
        std::thread::sleep(self.merge_delay);
        self.in_flight.fetch_sub(1, SeqCst);
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
            Cleanup::Merged(_) => "",
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

    fn check_on_branch(
        &self,
        _: &Path,
        branch: &str,
        _: Option<&std::ffi::OsStr>,
    ) -> std::result::Result<(), String> {
        match &self.wrong_branch {
            Some(other) => Err(format!("执行目录不在执行分支 {branch} 上（当前：{other}）")),
            None => Ok(()),
        }
    }

    fn repo_root(&self, _: &Path, _: Option<&std::ffi::OsStr>) -> Option<PathBuf> {
        None
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
    // Git 执行按假 Git 层的 head 传回 expectedHead（验收页上看到的那个）。
    let expected_head =
        f.ts.get_run(run_id)
            .ok()
            .and_then(|r| r.branch)
            .map(|b| format!("head-of-{b}"));
    f.ts.approve_review(ApproveReviewInput {
        run_id: run_id.into(),
        expected_head,
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
    fake(&f, FakeGit::default());
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
    // 读验收页时先把执行目录里未提交的改动提交到执行分支，预览即分支 head。
    assert_eq!(
        git.calls(),
        [
            format!(
                "commit {} teamspace: FE-001 attempt 1 (review)",
                s.exec_dir.display()
            ),
            "diff abc123..teamspace/FE-001-r1".to_owned()
        ]
    );
    assert_eq!(
        view.head_commit.as_deref(),
        Some("head-of-teamspace/FE-001-r1")
    );

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
            // 通过时不再提交（预览时已提交，expectedHead 保证合并的就是预览的内容）。
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

    // 执行目录不在执行分支上、合并层报错，同样保持待验收。
    let git = fake(
        &f,
        FakeGit {
            wrong_branch: Some("main".into()),
            ..FakeGit::default()
        },
    );
    let result = approve(&f, &s.run_id).unwrap();
    assert_eq!(result.outcome, ApproveOutcome::Blocked);
    assert!(
        result
            .reason
            .unwrap()
            .contains("执行目录不在执行分支 teamspace/FE-001-r1 上")
    );
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
    assert!(view.problems[0].contains("没有待验收的执行"));
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

    let before = OPENED.with(|r| r.borrow().len());
    f.ts.open_run_file(&s.run_id, "docs/a.md").unwrap();
    let opened = OPENED.with(|r| r.borrow().last().cloned()).unwrap();
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
        OPENED.with(|r| r.borrow().len()),
        before + 1,
        "被拒绝的路径不应打开"
    );
}

#[test]
fn open_run_file_reveals_runnable_files_instead_of_opening_them() {
    use std::os::unix::fs::PermissionsExt;
    let f = plain_fixture();
    let s = seed(&f, false);
    std::fs::write(f.ws.join("run.command"), "echo hi").unwrap();
    std::fs::write(f.ws.join("Setup.PKG"), "x").unwrap();
    std::fs::write(f.ws.join("tool"), "#!/bin/sh").unwrap();
    std::fs::set_permissions(f.ws.join("tool"), std::fs::Permissions::from_mode(0o755)).unwrap();
    std::fs::write(f.ws.join("notes.md"), "x").unwrap();
    let opened_before = OPENED.with(|r| r.borrow().len());
    for name in ["run.command", "Setup.PKG", "tool"] {
        let before = REVEALED.with(|r| r.borrow().len());
        f.ts.open_run_file(&s.run_id, name).unwrap();
        let shown = REVEALED.with(|r| r.borrow().last().cloned()).unwrap();
        assert_eq!(REVEALED.with(|r| r.borrow().len()), before + 1, "{name}");
        assert!(shown.ends_with(name), "{name}: {shown:?}");
    }
    assert_eq!(OPENED.with(|r| r.borrow().len()), opened_before);
    f.ts.open_run_file(&s.run_id, "notes.md").unwrap();
    assert_eq!(OPENED.with(|r| r.borrow().len()), opened_before + 1);

    // 原地执行里工作空间之外的改动以绝对路径记录，不能打开。
    let outside = f.dir.path().join("outside.md");
    std::fs::write(&outside, "x").unwrap();
    let err = assert_kind(
        f.ts.open_run_file(&s.run_id, &outside.display().to_string()),
        ErrorKind::Invalid,
    );
    assert!(err.starts_with("该文件不在执行目录内，无法打开"), "{err}");
}

// ---------------------------------------------------------------- 修复轮（M3-08 交叉审查）

fn set_marker(f: &Fixture, run_id: &str) {
    f.raw_sql(&format!(
        "UPDATE runs SET merge_base_branch = 'main', merge_head = 'h', merging_at = 'x'
         WHERE id = '{run_id}'"
    ));
}

/// #1：合并之后、落库之前中断（留下「合并中」标记）：分支已并入就按 Git 事实补记通过并推进下游。
#[test]
fn leftover_merge_marker_is_reconciled_as_approved_when_merged() {
    let f = fixture();
    let s = seed(&f, true);
    let fe = f.role(&s.team.id, "role_frontend");
    let downstream = f
        .try_task(
            &s.team.id,
            &fe,
            "下游",
            TaskStatus::Todo,
            vec!["docs/spec.md"],
            vec![s.task.id.clone()],
        )
        .unwrap();
    set_marker(&f, &s.run_id);
    let git = fake(
        &f,
        FakeGit {
            merged_into: Some(true),
            ..FakeGit::default()
        },
    );
    let view = f.ts.get_review(&s.task.id).unwrap();
    assert!(view.run.is_none(), "已补记通过，没有待验收的执行");
    assert_eq!(view.task.status, TaskStatus::Done);
    let run = f.ts.get_run(&s.run_id).unwrap();
    assert_eq!(run.review.unwrap().outcome, ReviewOutcome::Approved);
    assert_eq!(run.merged_commit.as_deref(), Some("head-of-main"));
    assert!(git.calls().iter().any(|c| c.starts_with("remove")));
    assert_eq!(
        f.ts.get_task(&downstream.task.id).unwrap().task.status,
        TaskStatus::Ready
    );
    let events = f.ts.get_task(&s.task.id).unwrap().events;
    assert!(
        events
            .iter()
            .any(|e| e.kind == EventKind::ReviewApproved && e.summary.contains("按 Git 事实补记"))
    );
}

/// #1：标记留下但分支没有并入：清除标记，回到待验收；Git 读不到结果时保留标记，不能退回。
#[test]
fn leftover_merge_marker_is_cleared_when_not_merged() {
    let f = fixture();
    let s = seed(&f, true);
    set_marker(&f, &s.run_id);
    fake(&f, FakeGit::default());
    let view = f.ts.get_review(&s.task.id).unwrap();
    assert!(!view.merging);
    assert!(view.can_approve, "{:?}", view.problems);
    assert_eq!(view.task.status, TaskStatus::Review);

    set_marker(&f, &s.run_id);
    // 模拟 Git 读不到合并结果：直接让 is_merged_into 失败。
    f.ts.set_review_git(Arc::new(MergedErr(FakeGit::default())));
    let view = f.ts.get_review(&s.task.id).unwrap();
    assert!(view.merging);
    assert!(!view.can_approve && !view.can_reject);
    let err = assert_kind(reject(&f, &s.run_id, "再改"), ErrorKind::Conflict);
    assert!(err.contains("无法确认上次合并的结果"), "{err}");
}

/// `is_merged_into` 总是失败的假 Git 层。
struct MergedErr(FakeGit);

impl ReviewGit for MergedErr {
    fn commit_all(
        &self,
        d: &Path,
        m: &str,
        p: Option<&std::ffi::OsStr>,
    ) -> std::result::Result<Option<String>, String> {
        self.0.commit_all(d, m, p)
    }
    fn diff_files(
        &self,
        r: &Path,
        b: &str,
        h: &str,
        p: Option<&std::ffi::OsStr>,
    ) -> std::result::Result<Vec<DiffEntry>, String> {
        self.0.diff_files(r, b, h, p)
    }
    fn file_patch(
        &self,
        r: &Path,
        b: &str,
        h: &str,
        x: &str,
        p: Option<&std::ffi::OsStr>,
    ) -> std::result::Result<PatchText, String> {
        self.0.file_patch(r, b, h, x, p)
    }
    fn merge_into_base(
        &self,
        r: &Path,
        b: &str,
        h: &str,
        m: &str,
        p: Option<&std::ffi::OsStr>,
    ) -> std::result::Result<Merge, String> {
        self.0.merge_into_base(r, b, h, m, p)
    }
    fn remove_run_worktree(
        &self,
        r: &Path,
        w: &Path,
        b: &str,
        c: Cleanup,
        p: Option<&std::ffi::OsStr>,
    ) -> std::result::Result<(), String> {
        self.0.remove_run_worktree(r, w, b, c, p)
    }
    fn is_merged_into(
        &self,
        _: &Path,
        _: &str,
        _: &str,
        _: Option<&std::ffi::OsStr>,
    ) -> std::result::Result<bool, String> {
        Err("仓库被锁".into())
    }
    fn branch_head(&self, r: &Path, b: &str, p: Option<&std::ffi::OsStr>) -> Option<String> {
        self.0.branch_head(r, b, p)
    }
    fn base_branch(&self, w: &crate::run::worktree::Worktree) -> Option<String> {
        self.0.base_branch(w)
    }
    fn check_on_branch(
        &self,
        d: &Path,
        b: &str,
        p: Option<&std::ffi::OsStr>,
    ) -> std::result::Result<(), String> {
        self.0.check_on_branch(d, b, p)
    }
    fn repo_root(&self, d: &Path, p: Option<&std::ffi::OsStr>) -> Option<PathBuf> {
        self.0.repo_root(d, p)
    }
}

/// #1：启动时对账（真实 Git：执行分支早已是 main 的祖先，即「已并入」）。
#[test]
fn startup_reconciles_leftover_merges_with_real_git() {
    let f = fixture();
    let run_git = |args: &[&str]| {
        let ok = std::process::Command::new("git")
            .arg("-C")
            .arg(&f.ws)
            .args(args)
            .output()
            .unwrap()
            .status
            .success();
        assert!(ok, "git {args:?}");
    };
    std::fs::remove_dir_all(f.ws.join(".git")).unwrap();
    run_git(&["init", "-q", "-b", "main"]);
    run_git(&["config", "user.email", "t@example.com"]);
    run_git(&["config", "user.name", "t"]);
    run_git(&["config", "commit.gpgsign", "false"]);
    std::fs::write(f.ws.join("a.txt"), "a").unwrap();
    run_git(&["add", "a.txt"]);
    run_git(&["commit", "-q", "-m", "init"]);
    let s = seed(&f, true);
    run_git(&["branch", "teamspace/FE-001-r1"]);
    set_marker(&f, &s.run_id);

    let reopened =
        Teamspace::open_with(&f.db_path(), Box::new(FakeProbe), HostConfig::default()).unwrap();
    assert_eq!(
        reopened.get_task(&s.task.id).unwrap().task.status,
        TaskStatus::Done
    );
    let run = reopened.get_run(&s.run_id).unwrap();
    assert_eq!(run.review.unwrap().outcome, ReviewOutcome::Approved);
    assert!(
        crate::service::review::merging(&reopened.lock(), &s.run_id)
            .unwrap()
            .is_none()
    );
}

/// #2：验收合并进行中，对该任务的其他入口一律拒绝；合并期间任务被改动时，结果不改任务状态。
#[test]
fn entries_are_refused_during_a_merge_and_results_respect_concurrent_changes() {
    let f = fixture();
    let s = seed(&f, true);
    fake(
        &f,
        FakeGit {
            merge_delay: Duration::from_millis(400),
            ..FakeGit::with_merge(Merge::Conflict {
                files: vec!["a.md".into()],
            })
        },
    );
    std::thread::scope(|scope| {
        let handle = scope.spawn(|| approve(&f, &s.run_id));
        std::thread::sleep(Duration::from_millis(150));
        for err in [
            f.move_to(&s.task.id, TaskStatus::Ready).map(|_| ()),
            f.ts.start_run(&s.task.id).map(|_| ()),
            reject(&f, &s.run_id, "改").map(|_| ()),
            approve(&f, &s.run_id).map(|_| ()),
        ] {
            let msg = assert_kind(err, ErrorKind::Conflict);
            assert_eq!(msg, "正在验收合并，请稍候");
        }
        // 别的途径（直接改库）在合并期间取消了任务。
        f.raw_sql(&format!(
            "UPDATE tasks SET status = 'cancelled' WHERE id = '{}'",
            s.task.id
        ));
        let result = handle.join().unwrap().unwrap();
        assert_eq!(result.outcome, ApproveOutcome::Conflict);
        // 冲突结果不能把已取消的任务改回受阻。
        assert_eq!(result.task.status, TaskStatus::Cancelled);
        assert!(result.run.review.is_none());
    });
    let events = f.ts.get_task(&s.task.id).unwrap().events;
    assert!(
        events
            .iter()
            .any(|e| e.kind == EventKind::ReviewConflict && e.summary.contains("任务状态未变"))
    );
}

/// #2：合并成功但合并期间任务被改动：记下合并事实，不改任务状态。
#[test]
fn merged_result_does_not_overwrite_a_task_changed_during_the_merge() {
    let f = fixture();
    let s = seed(&f, true);
    fake(
        &f,
        FakeGit {
            merge_delay: Duration::from_millis(300),
            ..FakeGit::with_merge(Merge::Merged {
                commit: "feed".into(),
            })
        },
    );
    std::thread::scope(|scope| {
        let handle = scope.spawn(|| approve(&f, &s.run_id));
        std::thread::sleep(Duration::from_millis(100));
        f.raw_sql(&format!(
            "UPDATE tasks SET status = 'ready' WHERE id = '{}'",
            s.task.id
        ));
        let result = handle.join().unwrap().unwrap();
        assert_eq!(result.outcome, ApproveOutcome::Merged);
        assert_eq!(result.task.status, TaskStatus::Ready);
        assert_eq!(result.run.merged_commit.as_deref(), Some("feed"));
        assert!(result.advanced_tasks.is_empty());
    });
}

/// #3：同一仓库里不同任务的合并串行进行。
#[test]
fn merges_in_the_same_repository_are_serialized() {
    let f = fixture();
    let s1 = seed(&f, true);
    let fe = f.role(&s1.team.id, "role_frontend");
    let t2 = f.readyable(&s1.team.id, &fe, "第二个").task;
    let exec2 = f
        .dir
        .path()
        .join("worktrees")
        .join(&s1.team.id)
        .join("FE-002-r1");
    std::fs::create_dir_all(&exec2).unwrap();
    let run2 = add_run(&f, &t2, &s1.harness, &exec2, true, "'[]'");
    f.raw_sql(&format!(
        "UPDATE tasks SET status = 'review' WHERE id = '{}';
         UPDATE runs SET branch = 'teamspace/FE-002-r1' WHERE id = '{run2}';",
        t2.id
    ));
    let git = fake(
        &f,
        FakeGit {
            merge_delay: Duration::from_millis(200),
            ..FakeGit::with_merge(Merge::Merged {
                commit: "abcdef".into(),
            })
        },
    );
    let results: Vec<Result<ApproveReviewResult>> = std::thread::scope(|scope| {
        let a = scope.spawn(|| approve(&f, &s1.run_id));
        let b = scope.spawn(|| approve(&f, &run2));
        vec![a.join().unwrap(), b.join().unwrap()]
    });
    for r in &results {
        assert_eq!(r.as_ref().unwrap().outcome, ApproveOutcome::Merged, "{r:?}");
    }
    assert_eq!(
        git.max_in_flight.load(std::sync::atomic::Ordering::SeqCst),
        1
    );
}

/// #4：执行目录不在执行分支上：验收页不提交并说明，不能通过。
#[test]
fn review_does_not_commit_when_the_exec_dir_left_the_run_branch() {
    let f = fixture();
    let s = seed(&f, true);
    let git = fake(
        &f,
        FakeGit {
            wrong_branch: Some("main".into()),
            ..FakeGit::default()
        },
    );
    let view = f.ts.get_review(&s.task.id).unwrap();
    assert!(!view.can_approve);
    assert!(
        view.files_problem
            .unwrap()
            .contains("执行目录不在执行分支 teamspace/FE-001-r1 上"),
    );
    assert!(git.calls().iter().all(|c| !c.starts_with("commit")));
}

/// #6：expectedHead 与执行分支当前 head 不一致（或缺少）时拒绝。
#[test]
fn approve_requires_the_previewed_head() {
    let f = fixture();
    let s = seed(&f, true);
    let git = fake(
        &f,
        FakeGit::with_merge(Merge::Merged { commit: "c".into() }),
    );
    let with = |head: Option<&str>| {
        f.ts.approve_review(ApproveReviewInput {
            run_id: s.run_id.clone(),
            expected_head: head.map(str::to_owned),
        })
    };
    let err = assert_kind(with(Some("stale")), ErrorKind::Conflict);
    assert_eq!(err, "改动已变化，请刷新验收页");
    assert_kind(with(None), ErrorKind::Invalid);
    assert!(git.calls().iter().all(|c| !c.starts_with("merge")));
    assert!(
        crate::service::review::merging(&f.ts.lock(), &s.run_id)
            .unwrap()
            .is_none()
    );
    let ok = with(Some("head-of-teamspace/FE-001-r1")).unwrap();
    assert_eq!(ok.outcome, ApproveOutcome::Merged);
}

/// #7：已通过的执行不会因为任务重新打开、未经新执行回到 REVIEW 而再次待验收。
#[test]
fn an_approved_run_is_not_pending_again_after_reopening() {
    let f = plain_fixture();
    let s = seed(&f, false);
    approve(&f, &s.run_id).unwrap();
    f.drive(
        &s.task.id,
        &[TaskStatus::Ready, TaskStatus::Doing, TaskStatus::Review],
    );
    let view = f.ts.get_review(&s.task.id).unwrap();
    assert!(view.run.is_none());
    assert!(!view.can_approve && !view.can_reject);
    assert!(
        view.problems[0].contains("没有待验收的执行"),
        "{:?}",
        view.problems
    );
    let err = assert_kind(approve(&f, &s.run_id), ErrorKind::Conflict);
    assert!(err.contains("已有验收结果（已通过）"), "{err}");
    assert_kind(reject(&f, &s.run_id, "改"), ErrorKind::Conflict);

    // Git：已通过的执行不再拦手动完成。
    let g = fixture();
    let s = seed(&g, true);
    fake(
        &g,
        FakeGit::with_merge(Merge::Merged { commit: "c".into() }),
    );
    approve(&g, &s.run_id).unwrap();
    g.drive(
        &s.task.id,
        &[TaskStatus::Ready, TaskStatus::Doing, TaskStatus::Review],
    );
    g.move_to(&s.task.id, TaskStatus::Done).unwrap();
}

/// #8：没有记录基线分支的旧执行，首次通过时把实际使用的基线分支写回；之后的恢复命令只用它。
#[test]
fn legacy_runs_record_the_base_branch_used_on_first_approve() {
    let f = fixture();
    let s = seed(&f, true);
    f.raw_sql(&format!(
        "UPDATE runs SET base_branch = NULL WHERE id = '{}'",
        s.run_id
    ));
    let view = f.ts.get_review(&s.task.id).unwrap();
    assert!(view.base_branch.is_none());
    fake(
        &f,
        FakeGit::with_merge(Merge::Conflict {
            files: vec!["a.md".into(), "b.md".into()],
        }),
    );
    let result = approve(&f, &s.run_id).unwrap();
    assert_eq!(result.outcome, ApproveOutcome::Conflict);
    // FakeProbe 报告工作空间在 main 上。
    assert_eq!(result.run.base_branch.as_deref(), Some("main"));

    // #9 / #10：冲突文件刷新后仍可列出；冲突说明不再重复出现在 problems 里。
    let view = f.ts.get_review(&s.task.id).unwrap();
    assert!(view.conflict);
    assert_eq!(view.conflict_files, ["a.md", "b.md"]);
    assert!(view.problems.is_empty(), "{:?}", view.problems);
    assert!(view.can_resolve_manually);

    // 基线分支仍未知的冲突执行：不能手动标记完成。
    f.raw_sql(&format!(
        "UPDATE runs SET base_branch = NULL WHERE id = '{}'",
        s.run_id
    ));
    let view = f.ts.get_review(&s.task.id).unwrap();
    assert!(!view.can_resolve_manually);
    fake(
        &f,
        FakeGit {
            merged_into: Some(true),
            ..FakeGit::default()
        },
    );
    let err = assert_kind(
        f.ts.resolve_merge_manually(ResolveMergeManuallyInput {
            run_id: s.run_id.clone(),
        }),
        ErrorKind::Conflict,
    );
    assert!(err.contains("没有记录基线分支"), "{err}");
}

/// 通过后返回自动就绪的下游任务。
#[test]
fn approve_reports_advanced_dependents() {
    let f = plain_fixture();
    let s = seed(&f, false);
    let fe = f.role(&s.team.id, "role_frontend");
    let downstream = f
        .try_task(
            &s.team.id,
            &fe,
            "下游页面",
            TaskStatus::Todo,
            vec!["docs/spec.md"],
            vec![s.task.id.clone()],
        )
        .unwrap();
    let result = approve(&f, &s.run_id).unwrap();
    assert_eq!(
        result.advanced_tasks,
        [AdvancedTask {
            id: downstream.task.id.clone(),
            key: downstream.task.key.clone(),
            title: "下游页面".into(),
        }]
    );
}
