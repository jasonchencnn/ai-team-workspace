//! M4-07 交叉审查的修复：中断保全的分支提示、Leader 的只读验收视图、执行目录没了时的保全提交。
//!
//! 真实子进程（cargo 构建的 `teamspace-mock-agent`）、真实 Git、临时目录；不调用任何真实 Agent。

mod common;

use std::path::Path;
use std::process::Command;

use common::*;
use serde_json::json;
use teamspace_core::dispatch::{Caller, dispatch};
use teamspace_core::model::*;

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

/// 等到说明包发出。
fn wait_brief(e: &Env, run_id: &str) -> String {
    e.wait(run_id, "brief", |_| {
        e.updates(run_id).iter().any(|u| {
            matches!(
                &u.body,
                RunUpdateBody::Message {
                    role: MessageRole::User,
                    ..
                }
            )
        })
    });
    brief(e, run_id)
}

/// 上一次执行成功待验收（不是中断保全）时，说明包里不再提示它的分支：
/// 那份成果还在待验收里，Agent 不该去 cherry-pick 一份重复的改动。
#[test]
fn a_new_run_does_not_mention_a_previous_run_that_was_not_interrupted() {
    let e = env(true);
    let task = e.task("先成功一次 [mock:shellwrite=output.txt]");
    let first = e.ts.start_run(&task.id).unwrap();
    let first = e.wait_end(&first.id);
    assert_eq!(first.status, RunStatus::Succeeded, "{first:#?}");
    let first_branch = first.branch.clone().unwrap();

    // 验收页退回前先把任务改回 READY，重新执行（新的 worktree、新的分支）。
    e.ts.change_task_status(ChangeTaskStatusInput {
        task_id: task.id.clone(),
        to: TaskStatus::Ready,
        reason: None,
    })
    .unwrap();
    let second = e.ts.start_run(&task.id).unwrap();
    assert_eq!(second.attempt, 2);

    let brief = wait_brief(&e, &second.id);
    assert!(
        !brief.contains("上一次执行的改动保存在分支"),
        "上一次执行不是中断保全，不该提示它的分支：{brief}"
    );
    assert!(!brief.contains(&first_branch), "{brief}");
}

/// Leader 调 `get_review` 是只读的：不在执行分支上留下提交，也不跟 Human 的验收抢仓库锁。
#[test]
fn the_leader_get_review_does_not_commit_the_exec_dir() {
    let e = env(true);
    let task = e.task("写文件 [mock:shellwrite=output.txt]");
    let started = e.ts.start_run(&task.id).unwrap();
    let run = e.wait_end(&started.id);
    assert_eq!(run.status, RunStatus::Succeeded, "{run:#?}");
    let branch = run.branch.clone().unwrap();
    let head_before = head_commit(&e.ws, &branch);

    // 再往执行目录里放一个没提交的文件。
    let exec = Path::new(&run.exec_dir).to_path_buf();
    std::fs::write(exec.join("extra.txt"), "extra\n").unwrap();

    // Leader 查询验收视图：只读。
    let reply = dispatch(
        &e.ts,
        Caller::Leader,
        "get_review",
        &json!({ "taskId": task.id }),
    )
    .unwrap();
    assert!(reply.get("err").is_none(), "{reply}");
    assert_eq!(
        head_commit(&e.ws, &branch),
        head_before,
        "Leader 调用 get_review 不应在执行分支上留下提交"
    );

    // Human 在验收页打开：这时才把执行目录里的改动提交到执行分支。
    e.ts.get_review(&task.id).unwrap();
    assert_ne!(
        head_commit(&e.ws, &branch),
        head_before,
        "Human 打开验收页时应把执行目录里的改动提交到执行分支"
    );
}

/// 执行目录已经被删掉时，中断保全仍能从仓库里找到执行分支上的保全提交。
#[test]
fn the_preserved_commit_is_found_even_when_the_exec_dir_is_gone() {
    let e = env(true);
    let task = e.task("先成功再退回 [mock:shellwrite=output.txt]");
    let first = e.ts.start_run(&task.id).unwrap();
    let first = e.wait_end(&first.id);
    assert_eq!(first.status, RunStatus::Succeeded, "{first:#?}");
    let branch = first.branch.clone().unwrap();
    let head = head_commit(&e.ws, &branch);

    // 验收退回：续接在同一条分支、同一个执行目录上。
    let revision =
        e.ts.reject_review(RejectReviewInput {
            run_id: first.id.clone(),
            comment: "再改改 [mock:shellwrite=output2.txt] [mock:linger]".into(),
        })
        .unwrap();
    assert_eq!(revision.branch.as_deref(), Some(branch.as_str()));
    e.wait(&revision.id, "linger message", |_| {
        e.updates(&revision.id).iter().any(|u| {
            matches!(&u.body, RunUpdateBody::Message { text, .. } if text.contains("步骤已处理完"))
        })
    });

    // 执行目录被删掉（用户清理了 worktree）。
    let exec = Path::new(&revision.exec_dir).to_path_buf();
    std::fs::remove_dir_all(&exec).unwrap();

    e.ts.shutdown();
    let revision = e.wait(&revision.id, "interrupted", |r| {
        r.status == RunStatus::Interrupted
    });
    // 分支上的保全提交还在仓库里：中断记录里应当能拿到它。
    assert_eq!(
        revision.interrupted_commit.as_deref(),
        Some(head.as_str()),
        "{revision:#?}"
    );
}
