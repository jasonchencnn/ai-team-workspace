//! 岗位启动包（交接节）：岗位换绑到新的 Harness 后，新环境第一次执行时把接手前的上下文附在
//! 任务说明包里；同一个 Harness 再次执行时不重复附上。
//!
//! 这里不启动任何 Agent（Mock 预设指向不存在的宿主程序），只验证交接的判定与内容；
//! 说明包里是否真的带上交接节，见 `tests/runs.rs` 的端到端测试。

use std::time::{Duration, Instant};

use super::*;

/// 等执行结束（启动失败的执行也会很快结束）。
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

/// 团队 T、岗位「前端」（绑定 Mock Harness）与一个 Mock Harness。
fn setup(f: &Fixture) -> (Team, Role, HarnessInstance) {
    let team = f.team("T");
    let fe = f.role(&team.id, "role_frontend");
    let h = f.harness(&team.id, "Mock", AcpPreset::Mock);
    f.ts.bind_role_harness(&fe.id, &h.id).unwrap();
    (team, fe, h)
}

/// 在岗位上跑一次（Mock 预设指向不存在的宿主程序，执行会启动失败，但执行记录已经落库）。
fn run_once(f: &Fixture, team: &Team, role: &Role, title: &str) -> Run {
    let task = f.readyable(&team.id, role, title);
    let task = f.move_to(&task.task.id, TaskStatus::Ready).unwrap();
    let run = f.ts.start_run(&task.task.id).unwrap();
    wait_end(f, &run.id)
}

#[test]
fn empty_role_still_gets_a_handoff_section() {
    let f = plain_fixture();
    let (_, fe, _) = setup(&f);
    let text = f.ts.get_role_handoff_preview(&fe.id).unwrap();
    assert!(text.contains("## 岗位交接"), "{text}");
    assert!(text.contains("### 岗位说明"), "{text}");
    assert!(text.contains("### 未完成任务\n\n（无）"), "{text}");
    assert!(text.contains("### 近期执行结果\n\n（无）"), "{text}");
    assert!(text.contains("### 工作日志（最近部分）"), "{text}");
    assert!(text.contains("### 相关文件引用\n\n（无）"), "{text}");
}

/// 取「未完成任务」一节的正文（到下一节为止）。
fn open_tasks_section(text: &str) -> String {
    let start = text.find("### 未完成任务").expect("未完成任务一节");
    let rest = &text[start..];
    let end = rest[1..]
        .find("\n### ")
        .map(|i| i + 1)
        .unwrap_or(rest.len());
    rest[..end].to_owned()
}

#[test]
fn handoff_lists_open_tasks_and_their_files() {
    let f = plain_fixture();
    let (team, fe, _) = setup(&f);
    let task = f.readyable(&team.id, &fe, "登录页");
    let text = f.ts.get_role_handoff_preview(&fe.id).unwrap();
    assert!(
        text.contains(&format!("- {} 登录页（TODO）", task.task.key)),
        "{text}"
    );
    assert!(
        text.contains("### 相关文件引用\n\n- `docs/spec.md`"),
        "{text}"
    );
    // 已完成的任务不在未完成任务列表里。
    let done = f.task(&team.id, &fe, "收尾的任务");
    f.move_to(&done.task.id, TaskStatus::Ready).unwrap();
    f.move_to(&done.task.id, TaskStatus::Doing).unwrap();
    f.move_to(&done.task.id, TaskStatus::Review).unwrap();
    f.move_to(&done.task.id, TaskStatus::Done).unwrap();
    let text = f.ts.get_role_handoff_preview(&fe.id).unwrap();
    let open = open_tasks_section(&text);
    assert!(open.contains(&task.task.key), "{open}");
    assert!(!open.contains(&done.task.key), "{open}");
    // 已完成任务引用的文件也不在文件引用里。
    assert!(
        !text.contains("### 相关文件引用\n\n- `docs/spec.md`\n- `收尾"),
        "{text}"
    );
}

#[test]
fn the_same_harness_does_not_get_the_handoff_again() {
    let f = plain_fixture();
    let (team, fe, _) = setup(&f);
    run_once(&f, &team, &fe, "登录页");
    // 同一个 Harness 已经在这个岗位上执行过：不再交底。
    let text = f.ts.get_role_handoff_preview(&fe.id).unwrap();
    assert_eq!(text, "", "{text}");
}

#[test]
fn a_new_harness_gets_the_handoff_content() {
    let f = plain_fixture();
    let (team, fe, _) = setup(&f);
    let run = run_once(&f, &team, &fe, "登录页");

    // 换绑到另一个 Harness：新环境第一次执行前要交底。
    let other = f.harness(&team.id, "Mock 二号", AcpPreset::Mock);
    f.ts.bind_role_harness(&fe.id, &other.id).unwrap();
    let text = f.ts.get_role_handoff_preview(&fe.id).unwrap();
    assert!(text.contains("## 岗位交接"), "{text}");
    assert!(text.contains("### 近期执行结果"), "{text}");
    assert!(text.contains(&run.task_key), "{text}");
    assert!(text.contains("失败"), "{text}");

    // 新 Harness 上执行过一次之后就不再交底。
    run_once(&f, &team, &fe, "注册页");
    let text = f.ts.get_role_handoff_preview(&fe.id).unwrap();
    assert_eq!(text, "", "{text}");
}

#[test]
fn preview_of_a_role_without_harness_is_empty() {
    let f = plain_fixture();
    let team = f.team("T");
    let fe = f.role(&team.id, "role_frontend");
    assert_eq!(f.ts.get_role_handoff_preview(&fe.id).unwrap(), "");
    let _ = team;
}

#[test]
fn preview_of_an_unknown_role_is_not_found() {
    let f = plain_fixture();
    let err = assert_kind(
        f.ts.get_role_handoff_preview("没有这个岗位"),
        ErrorKind::NotFound,
    );
    assert!(err.contains("岗位不存在"), "{err}");
}
