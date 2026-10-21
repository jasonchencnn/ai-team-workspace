//! 岗位换绑的执行级验证（M6-01）：真实子进程（cargo 构建的 `teamspace-mock-agent`）、临时目录。
//!
//! 覆盖：待生效换绑在执行成功 / 失败 / 取消后自动生效；「取消并重新执行」与「人工接管」
//! 两种立即换绑方式；消息投递按岗位「当前有效」绑定解析（快照只留痕）。
//!
//! 不调用任何真实 Agent：Mock 预设指向本仓库构建的 `teamspace-mock-agent`。

mod common;

use std::time::{Duration, Instant};

use common::*;
use teamspace_core::model::*;

/// 团队里的第二个 Harness 实例（Mock）。
fn second_harness(e: &Env, name: &str) -> HarnessInstance {
    e.ts.create_harness(CreateHarnessInput {
        team_id: e.team.id.clone(),
        name: name.into(),
        preset: AcpPreset::Mock,
        command: None,
        args: None,
        env_profile: None,
    })
    .unwrap()
}

/// 等消息到达某个状态（最多 60 秒）。
fn wait_message(
    e: &Env,
    thread_id: &str,
    message_id: &str,
    what: &str,
    done: impl Fn(&Message) -> bool,
) -> Message {
    let deadline = Instant::now() + Duration::from_secs(60);
    loop {
        let detail = e.ts.get_thread(thread_id).unwrap();
        let message = detail
            .messages
            .into_iter()
            .find(|message| message.id == message_id)
            .expect("消息还在");
        if done(&message) {
            return message;
        }
        assert!(
            Instant::now() < deadline,
            "timed out waiting for {what}: {message:#?}"
        );
        std::thread::sleep(Duration::from_millis(30));
    }
}

/// 等到岗位的绑定换成目标 Harness（最多 30 秒）。
fn wait_binding(e: &Env, harness_id: &str) -> RoleView {
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        let view = e.ts.get_role_workbench(&e.role.id).unwrap().view;
        if view.role.harness_id.as_deref() == Some(harness_id) {
            return view;
        }
        assert!(
            Instant::now() < deadline,
            "timed out waiting for binding: {view:#?}"
        );
        std::thread::sleep(Duration::from_millis(30));
    }
}

/// 等到执行进入「等待权限」：Agent 已经在跑，并且会一直等 Human 决定。
fn wait_permission(e: &Env, run_id: &str) -> Run {
    e.wait(run_id, "waiting_permission", |run| {
        run.status == RunStatus::WaitingPermission
    })
}

/// 待生效换绑在执行「成功」后自动生效。
#[test]
fn a_pending_rebind_takes_effect_after_the_run_succeeds() {
    let e = env(false);
    let b = second_harness(&e, "Mock B");
    let task = e.task("构建 [mock:perm]");
    let run = e.ts.start_run(&task.id).unwrap();
    let waiting = wait_permission(&e, &run.id);

    // 登记待生效换绑：当前绑定不变。
    let view =
        e.ts.rebind_role(&e.role.id, &b.id, RebindMode::AfterCurrent)
            .unwrap();
    assert_eq!(
        view.role.harness_id.as_deref(),
        Some(e.harness.id.as_str()),
        "待生效期间当前绑定不应变化"
    );
    let pending = view.pending_binding.expect("应登记待生效换绑");
    assert_eq!(pending.harness_id, b.id);
    assert_eq!(pending.mode, RebindMode::AfterCurrent);

    // 允许这次权限：执行成功结束。
    e.respond(&run, &waiting.pending_permission.unwrap(), "allow");
    let run = e.wait_end(&run.id);
    assert_eq!(run.status, RunStatus::Succeeded, "{run:#?}");

    // 执行结束后待生效换绑自动生效。
    let view = wait_binding(&e, &b.id);
    assert!(view.pending_binding.is_none(), "{view:?}");
    let events =
        e.ts.list_events(EventQuery {
            team_id: e.team.id.clone(),
            ..Default::default()
        })
        .unwrap();
    assert!(
        events
            .events
            .iter()
            .any(|event| event.kind == EventKind::RoleRebindApplied),
        "{events:?}"
    );
}

/// 待生效换绑在执行「失败」后自动生效。
#[test]
fn a_pending_rebind_takes_effect_after_the_run_fails() {
    let e = env(false);
    let b = second_harness(&e, "Mock B");
    let task = e.task("构建 [mock:perm] [mock:failafter]");
    let run = e.ts.start_run(&task.id).unwrap();
    let waiting = wait_permission(&e, &run.id);

    e.ts.rebind_role(&e.role.id, &b.id, RebindMode::AfterCurrent)
        .unwrap();
    e.respond(&run, &waiting.pending_permission.unwrap(), "allow");
    let run = e.wait_end(&run.id);
    assert_eq!(run.status, RunStatus::Failed, "{run:#?}");

    let view = wait_binding(&e, &b.id);
    assert!(view.pending_binding.is_none(), "{view:?}");
}

/// 待生效换绑在执行「取消」后自动生效。
#[test]
fn a_pending_rebind_takes_effect_after_the_run_is_cancelled() {
    let e = env(false);
    let b = second_harness(&e, "Mock B");
    let task = e.task("构建 [mock:perm]");
    let run = e.ts.start_run(&task.id).unwrap();
    wait_permission(&e, &run.id);

    e.ts.rebind_role(&e.role.id, &b.id, RebindMode::AfterCurrent)
        .unwrap();
    let cancelled = e.ts.cancel_run(&run.id).unwrap();
    assert_eq!(cancelled.status, RunStatus::Cancelled, "{cancelled:#?}");

    let view = wait_binding(&e, &b.id);
    assert!(view.pending_binding.is_none(), "{view:?}");
}

/// 撤销待生效换绑：执行结束后也不会换过去。
#[test]
fn a_cancelled_pending_rebind_never_takes_effect() {
    let e = env(false);
    let b = second_harness(&e, "Mock B");
    let task = e.task("构建 [mock:perm]");
    let run = e.ts.start_run(&task.id).unwrap();
    let waiting = wait_permission(&e, &run.id);

    e.ts.rebind_role(&e.role.id, &b.id, RebindMode::AfterCurrent)
        .unwrap();
    let view = e.ts.cancel_pending_rebind(&e.role.id).unwrap();
    assert!(view.pending_binding.is_none(), "{view:?}");
    assert_eq!(view.role.harness_id.as_deref(), Some(e.harness.id.as_str()));

    // 执行结束后绑定仍然没变。
    e.respond(&run, &waiting.pending_permission.unwrap(), "allow");
    e.wait_end(&run.id);
    std::thread::sleep(Duration::from_millis(200));
    let view = e.ts.get_role_workbench(&e.role.id).unwrap().view;
    assert_eq!(
        view.role.harness_id.as_deref(),
        Some(e.harness.id.as_str()),
        "撤销后不应换绑"
    );
    assert!(view.pending_binding.is_none(), "{view:?}");
}

/// 「取消并重新执行」：取消进行中的执行，立即换绑，并在新 Harness 上重新执行该任务。
#[test]
fn cancel_and_rerun_cancels_the_run_and_reruns_on_the_new_harness() {
    let e = env(false);
    let b = second_harness(&e, "Mock B");
    let task = e.task("构建 [mock:perm]");
    let run = e.ts.start_run(&task.id).unwrap();
    wait_permission(&e, &run.id);

    let view =
        e.ts.rebind_role(&e.role.id, &b.id, RebindMode::CancelAndRerun)
            .unwrap();
    // 立即换绑，没有待生效换绑。
    assert_eq!(view.role.harness_id.as_deref(), Some(b.id.as_str()));
    assert!(view.pending_binding.is_none(), "{view:?}");

    // 旧执行被取消。
    let old = e.ts.get_run(&run.id).unwrap();
    assert_eq!(old.status, RunStatus::Cancelled, "{old:#?}");

    // 同一个任务在新 Harness 上重新执行。
    let rerun = e.ts.get_task(&task.id).unwrap().latest_run.unwrap();
    assert_ne!(rerun.id, run.id);
    assert_eq!(rerun.harness.harness_id, b.id);
    assert_eq!(rerun.attempt, 2);
}

/// 「人工接管」：取消进行中的执行，任务转为受阻并注明「人工接管」，立即换绑。
#[test]
fn manual_takeover_blocks_the_task_and_binds_immediately() {
    let e = env(false);
    let b = second_harness(&e, "Mock B");
    let task = e.task("构建 [mock:perm]");
    let run = e.ts.start_run(&task.id).unwrap();
    wait_permission(&e, &run.id);

    let view =
        e.ts.rebind_role(&e.role.id, &b.id, RebindMode::ManualTakeover)
            .unwrap();
    assert_eq!(view.role.harness_id.as_deref(), Some(b.id.as_str()));
    assert!(view.pending_binding.is_none(), "{view:?}");

    // 旧执行被取消，任务转为受阻并注明「人工接管」。
    let old = e.ts.get_run(&run.id).unwrap();
    assert_eq!(old.status, RunStatus::Cancelled, "{old:#?}");
    let detail = e.ts.get_task(&task.id).unwrap();
    assert_eq!(detail.task.status, TaskStatus::Blocked, "{detail:#?}");
    assert!(
        detail
            .task
            .blocked_reason
            .as_deref()
            .is_some_and(|reason| reason.contains("人工接管")),
        "{:?}",
        detail.task.blocked_reason
    );
}

/// 投递按岗位「当前有效」绑定解析：消息入队时记的快照只是留痕。
///
/// 这里的做法：先绑定一个坏掉的 Harness（消息投递必然失败），再换绑到能用的 Mock，
/// 重发消息——投递用的是换绑后的绑定，而不是入队时的快照。
#[test]
fn delivery_resolves_the_current_binding_not_the_enqueue_snapshot() {
    let e = env(false);
    // 坏掉的 Harness：启动命令不存在。
    let broken =
        e.ts.create_harness(CreateHarnessInput {
            team_id: e.team.id.clone(),
            name: "坏掉的".into(),
            preset: AcpPreset::Custom,
            command: Some("/nonexistent/agent".into()),
            args: None,
            env_profile: None,
        })
        .unwrap();
    e.ts.bind_role_harness(&e.role.id, &broken.id).unwrap();

    let message =
        e.ts.send_message(SendMessageInput {
            team_id: e.team.id.clone(),
            to_role_id: e.role.id.clone(),
            body: "请处理登录页".into(),
            task_id: None,
            thread_id: None,
            subject: None,
            from_role_id: None,
        })
        .unwrap();
    // 入队时记的是当时的绑定（坏掉的 Harness）。
    assert_eq!(
        message.binding_harness_id.as_deref(),
        Some(broken.id.as_str())
    );

    // 投递失败：当前绑定是坏掉的 Harness。
    let failed = wait_message(&e, &message.thread_id, &message.id, "message failed", |m| {
        m.status == MessageStatus::Failed
    });
    assert!(
        failed
            .failure_reason
            .as_deref()
            .is_some_and(|reason| reason.contains("启动命令不存在")),
        "{failed:#?}"
    );

    // 换绑到能用的 Mock，重发：投递用的是当前绑定。
    let good = second_harness(&e, "Mock B");
    e.ts.rebind_role(&e.role.id, &good.id, RebindMode::AfterCurrent)
        .unwrap();
    let retried = e.ts.retry_message(&message.id).unwrap();
    assert_eq!(
        retried.binding_harness_id.as_deref(),
        Some(broken.id.as_str())
    );

    let replied = wait_message(
        &e,
        &message.thread_id,
        &message.id,
        "message replied",
        |m| m.status == MessageStatus::Replied,
    );
    assert!(
        replied
            .reply_body
            .as_deref()
            .is_some_and(|body| !body.is_empty()),
        "{replied:#?}"
    );
}
