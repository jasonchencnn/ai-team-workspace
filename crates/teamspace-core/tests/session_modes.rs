//! M2.1-02（S-1a）：会话建立后，Teamspace 把 Agent 切到「向 Client 请求权限」的模式。
//! 真实子进程（Mock Agent，工作目录里的 `.mock-modes` 决定它公布什么模式）；不调用任何真实 Agent。

mod common;

use common::*;
use teamspace_core::model::*;

/// 让 Mock 在这个工作空间里公布指定的权限模式，返回任务总结。
fn run_with_modes(variant: Option<&str>) -> (Env, Run, Vec<RunUpdate>) {
    let e = env(false);
    if let Some(variant) = variant {
        std::fs::write(e.ws.join(".mock-modes"), variant).unwrap();
    }
    let task = e.task("看看权限模式 [mock:modes]");
    let run = e.ts.start_run(&task.id).unwrap();
    let run = e.wait_end(&run.id);
    assert_eq!(run.status, RunStatus::Succeeded, "{run:#?}");
    let updates = e.updates(&run.id);
    (e, run, updates)
}

/// 进度里的状态提示文字。
fn notices(updates: &[RunUpdate]) -> Vec<String> {
    updates
        .iter()
        .filter_map(|u| match &u.body {
            RunUpdateBody::Status {
                detail: Some(text), ..
            } if text.contains("可能不经确认就执行操作") => Some(text.clone()),
            _ => None,
        })
        .collect()
}

#[test]
fn permission_option_agent_is_switched_from_full_access_to_ask() {
    // 仿 MiniMax Code：configOptions.permissionMode 起始 bypassPermissions。
    let (_e, run, updates) = run_with_modes(Some("config"));
    assert!(
        summary(&run).contains("[modes:default]"),
        "{}",
        summary(&run)
    );
    assert!(notices(&updates).is_empty(), "{:?}", notices(&updates));
}

#[test]
fn session_mode_agent_is_switched_to_the_strictest_asking_mode() {
    // 仿 Claude Code / Gemini：可选里有 plan、acceptEdits、dontAsk，只能选 default。
    let (_e, run, updates) = run_with_modes(Some("modes"));
    assert!(
        summary(&run).contains("[modes:default]"),
        "{}",
        summary(&run)
    );
    assert!(notices(&updates).is_empty());
}

#[test]
fn agent_without_published_modes_is_left_alone_without_noise() {
    let (_e, run, updates) = run_with_modes(None);
    assert!(summary(&run).contains("[modes:none]"), "{}", summary(&run));
    assert!(notices(&updates).is_empty());
}

#[test]
fn no_asking_mode_available_is_reported_in_the_run_record() {
    // 只有放行类和不执行类的模式：不切换（绝不选 acceptEdits / plan / dontAsk），执行记录里提示风险。
    let (_e, run, updates) = run_with_modes(Some("locked"));
    assert!(summary(&run).contains("[modes:yolo]"), "{}", summary(&run));
    let notices = notices(&updates);
    assert_eq!(notices.len(), 1, "{notices:?}");
    assert!(notices[0].contains("yolo"), "{}", notices[0]);
}

#[test]
fn failed_switch_does_not_fail_the_run_but_is_reported() {
    let (_e, run, updates) = run_with_modes(Some("refuse"));
    assert!(
        summary(&run).contains("[modes:bypassPermissions]"),
        "{}",
        summary(&run)
    );
    let notices = notices(&updates);
    assert_eq!(notices.len(), 1, "{notices:?}");
    assert!(notices[0].contains("无法把 Agent 切换"), "{}", notices[0]);
}
