//! M4-04 的三条使用记录：中断时把改动保全到执行分支（1）、成功但没有任何改动（2）、
//! 人工接管的文案（5）。
//!
//! 真实子进程（cargo 构建的 `teamspace-mock-agent`）、真实 Git、临时目录；不调用任何真实 Agent。

mod common;

use std::path::Path;
use std::process::Command;

use common::*;
use teamspace_core::model::*;

/// 等到 Mock 走完写入、正停在「步骤已处理完，等待收尾」（执行仍在进行）。
fn wait_linger(e: &Env, run_id: &str) {
    e.wait(run_id, "linger message", |_| {
        e.updates(run_id).iter().any(|u| {
            matches!(&u.body, RunUpdateBody::Message { text, .. } if text.contains("步骤已处理完"))
        })
    });
}

/// 分支上最新一条提交的信息。
fn head_subject(repo: &Path, branch: &str) -> String {
    let out = Command::new("git")
        .arg("-C")
        .arg(repo)
        .args(["log", "-1", "--format=%s", branch])
        .output()
        .unwrap();
    String::from_utf8_lossy(&out.stdout).trim().to_owned()
}

/// 分支上最新一条提交的哈希。
fn head_commit(repo: &Path, branch: &str) -> String {
    let out = Command::new("git")
        .arg("-C")
        .arg(repo)
        .args(["log", "-1", "--format=%H", branch])
        .output()
        .unwrap();
    String::from_utf8_lossy(&out.stdout).trim().to_owned()
}

/// 分支上最新一条提交里的文件列表。
fn head_files(repo: &Path, branch: &str) -> Vec<String> {
    let out = Command::new("git")
        .arg("-C")
        .arg(repo)
        .args(["ls-tree", "-r", "--name-only", branch])
        .output()
        .unwrap();
    String::from_utf8_lossy(&out.stdout)
        .lines()
        .map(str::to_owned)
        .collect()
}

/// 任务说明包（发给 Agent 的那条用户消息）。
fn brief(e: &Env, run_id: &str) -> String {
    e.updates(run_id)
        .into_iter()
        .find_map(|u| match u.body {
            RunUpdateBody::Message {
                role: MessageRole::User,
                text,
            } => Some(text),
            _ => None,
        })
        .expect("说明包")
}

/// 使用记录 1：Git worktree 的执行被中断（应用退出）时，执行目录里未提交的改动被提交到执行分支，
/// 提交信息是英文的；执行记录里给出这次提交。
#[test]
fn interrupted_worktree_run_preserves_the_changes_on_the_run_branch() {
    let e = env(true);
    let task = e.task("中断前已改文件 [mock:shellwrite=output.txt] [mock:linger]");
    let started = e.ts.start_run(&task.id).unwrap();
    wait_linger(&e, &started.id);
    // worktree 与分支是 driver 起来之后才有的。
    let run = e.ts.get_run(&started.id).unwrap();
    assert_eq!(run.isolation, RunIsolation::Worktree);
    let branch = run.branch.clone().unwrap();
    let exec = Path::new(&run.exec_dir).to_path_buf();
    assert!(
        exec.join("output.txt").is_file(),
        "中断前改动留在执行目录里"
    );

    e.ts.shutdown();
    let run = e.wait(&run.id, "interrupted", |r| {
        r.status == RunStatus::Interrupted
    });
    assert!(run.interrupted_problem.is_none(), "{run:#?}");
    let commit = run
        .interrupted_commit
        .clone()
        .unwrap_or_else(|| panic!("中断提交：{run:#?}"));
    assert_eq!(commit, head_commit(&e.ws, &branch));
    assert_eq!(
        head_subject(&e.ws, &branch),
        "teamspace: FE-001 attempt 1 (interrupted)"
    );
    assert!(
        head_files(&e.ws, &branch).contains(&"output.txt".to_owned()),
        "{:?}",
        head_files(&e.ws, &branch)
    );
}

/// 使用记录 1：执行目录已经不在执行分支上时不提交，只记一条警告，执行照常标记为已中断。
#[test]
fn interrupted_run_off_the_run_branch_only_warns() {
    let e = env(true);
    let task = e.task("改到别的分支 [mock:shellwrite=output.txt] [mock:linger]");
    let started = e.ts.start_run(&task.id).unwrap();
    wait_linger(&e, &started.id);
    let run = e.ts.get_run(&started.id).unwrap();
    let branch = run.branch.clone().unwrap();
    // 执行目录被切到了别的分支：保全时不能提交。
    let exec = Path::new(&run.exec_dir).to_path_buf();
    git(&exec, &["checkout", "-q", "-b", "elsewhere"]);

    e.ts.shutdown();
    let run = e.wait(&run.id, "interrupted", |r| {
        r.status == RunStatus::Interrupted
    });
    assert!(run.interrupted_commit.is_none(), "{run:#?}");
    let problem = run
        .interrupted_problem
        .clone()
        .unwrap_or_else(|| panic!("保全失败的原因：{run:#?}"));
    assert!(problem.contains("不在执行分支"), "{problem}");
    assert!(
        !head_subject(&e.ws, &branch).contains("interrupted"),
        "{}",
        head_subject(&e.ws, &branch)
    );
    let warns: Vec<String> = e
        .updates(&run.id)
        .into_iter()
        .filter_map(|u| match u.body {
            RunUpdateBody::Warning { text, .. } => Some(text),
            _ => None,
        })
        .collect();
    assert!(
        warns
            .iter()
            .any(|text| text.contains("没能把改动提交到执行分支")),
        "{warns:?}"
    );
}

/// 使用记录 1：重新执行（新的 worktree、新的分支）时，任务说明包里带上一次执行的分支，
/// Agent 可以 `git cherry-pick` 取回。
#[test]
fn a_new_run_brief_mentions_the_previous_run_branch() {
    let e = env(true);
    let task = e.task("中断后重做 [mock:shellwrite=output.txt] [mock:linger]");
    let first = e.ts.start_run(&task.id).unwrap();
    wait_linger(&e, &first.id);
    let first_branch = e.ts.get_run(&first.id).unwrap().branch.clone().unwrap();
    e.ts.shutdown();
    e.wait(&first.id, "interrupted", |r| {
        r.status == RunStatus::Interrupted
    });

    let second = e.ts.start_run(&task.id).unwrap();
    assert_eq!(second.attempt, 2);
    assert_ne!(second.branch.as_deref(), Some(first_branch.as_str()));
    e.wait(&second.id, "brief", |_| {
        e.updates(&second.id).iter().any(|u| {
            matches!(
                &u.body,
                RunUpdateBody::Message {
                    role: MessageRole::User,
                    ..
                }
            )
        })
    });
    let brief = brief(&e, &second.id);
    assert!(
        brief.contains(&format!(
            "上一次执行的改动保存在分支 `{first_branch}`，需要时可以 `git cherry-pick` 取回其中有用的部分。"
        )),
        "{brief}"
    );
}

/// 使用记录 2：Git 执行成功但相对基线没有任何改动时，执行记录标记 `noChanges`，任务仍进入待验收，
/// 验收页据此提示。
#[test]
fn a_successful_worktree_run_without_changes_is_flagged() {
    let e = env(true);
    let task = e.task("没有改动 [mock:nochange]");
    let run = e.ts.start_run(&task.id).unwrap();
    let run = e.wait_end(&run.id);
    assert_eq!(run.status, RunStatus::Succeeded, "{run:#?}");
    assert!(run.no_changes, "{run:#?}");
    assert!(run.changed_files.is_empty(), "{:?}", run.changed_files);
    assert_eq!(
        e.ts.get_task(&task.id).unwrap().task.status,
        TaskStatus::Review
    );
    let view = e.ts.get_review(&task.id).unwrap();
    assert!(view.run.as_ref().unwrap().no_changes, "{view:#?}");
}

/// 使用记录 2：普通文件夹原地执行成功但快照比对没有发现改动时，同样标记 `noChanges`。
#[test]
fn a_successful_plain_folder_run_without_changes_is_flagged() {
    let e = env(false);
    let task = e.task("没有改动 [mock:nochange]");
    let run = e.ts.start_run(&task.id).unwrap();
    let run = e.wait_end(&run.id);
    assert_eq!(run.status, RunStatus::Succeeded, "{run:#?}");
    assert!(run.no_changes, "{run:#?}");
    assert_eq!(
        e.ts.get_task(&task.id).unwrap().task.status,
        TaskStatus::Review
    );
    let view = e.ts.get_review(&task.id).unwrap();
    assert!(view.run.as_ref().unwrap().no_changes, "{view:#?}");
}

/// 使用记录 5：人工接管后的执行状态显示「已被接管」；接管后再送验收，验收页也是同一份文案。
#[test]
fn a_taken_over_run_is_labelled_as_taken_over() {
    let e = env(false);
    let task = e.task("[mock:hang]");
    let run = e.ts.start_run(&task.id).unwrap();
    e.wait(&run.id, "running", |r| r.status == RunStatus::Running);
    let taken = e.ts.takeover_run(&run.id).unwrap();
    assert_eq!(taken.status, RunStatus::Cancelled);
    assert!(taken.taken_over);
    assert_eq!(taken.status_label, "已被接管", "{taken:#?}");

    // 接管后再送验收：验收页的执行结果也是「已被接管」。
    e.ts.change_task_status(ChangeTaskStatusInput {
        task_id: task.id.clone(),
        to: TaskStatus::Review,
        reason: None,
    })
    .unwrap();
    let view = e.ts.get_review(&task.id).unwrap();
    let reviewed = view.run.as_ref().unwrap();
    assert!(reviewed.taken_over);
    assert_eq!(reviewed.status_label, "已被接管", "{reviewed:#?}");
}
