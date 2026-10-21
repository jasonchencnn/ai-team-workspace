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
use teamspace_core::system_probe::LocalProbe;
use teamspace_core::{AgentCommand, HostConfig, Teamspace};

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
    wait_binding_of(&e.ts, &e.role.id, harness_id)
}

/// 等到（任意实例里的）岗位绑定换成目标 Harness（最多 30 秒）。
fn wait_binding_of(ts: &Teamspace, role_id: &str, harness_id: &str) -> RoleView {
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        let view = ts.get_role_workbench(role_id).unwrap().view;
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

/// 「人工接管」：复用现有人工接管——执行记为已被接管，任务留在执行中，立即换绑（D35①）。
///
/// 与现有人工接管一致：Human 改完可以直接提交验收，任务不转受阻。
#[test]
fn manual_takeover_marks_the_run_as_taken_over_and_keeps_the_task_doing() {
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

    // 旧执行记为已被接管（已取消 + takenOver）。
    let old = e.ts.get_run(&run.id).unwrap();
    assert_eq!(old.status, RunStatus::Cancelled, "{old:#?}");
    assert!(old.taken_over, "{old:#?}");

    // 任务留在执行中，不转受阻。
    let detail = e.ts.get_task(&task.id).unwrap();
    assert_eq!(detail.task.status, TaskStatus::Doing, "{detail:#?}");
    assert_eq!(detail.task.blocked_reason, None, "{detail:#?}");
}

/// 「取消并重新执行」：两个执行同时在跑时，两个任务都在新 Harness 上重跑（D35 严重项 3）。
#[test]
fn cancel_and_rerun_reruns_every_cancelled_task() {
    // Git 仓库：两个执行各自一个 worktree，可以同时在跑。
    let e = env(true);
    let b = second_harness(&e, "Mock B");
    let first = e.task("构建一 [mock:perm]");
    let second = e.task("构建二 [mock:perm]");
    let run1 = e.ts.start_run(&first.id).unwrap();
    let run2 = e.ts.start_run(&second.id).unwrap();
    wait_permission(&e, &run1.id);
    wait_permission(&e, &run2.id);

    let view =
        e.ts.rebind_role(&e.role.id, &b.id, RebindMode::CancelAndRerun)
            .unwrap();
    assert_eq!(view.role.harness_id.as_deref(), Some(b.id.as_str()));

    // 两个旧执行都被取消。
    let old1 = e.ts.get_run(&run1.id).unwrap();
    let old2 = e.ts.get_run(&run2.id).unwrap();
    assert_eq!(old1.status, RunStatus::Cancelled, "{old1:#?}");
    assert_eq!(old2.status, RunStatus::Cancelled, "{old2:#?}");

    // 两个任务都在新 Harness 上重跑了。
    let rerun1 = e.ts.get_task(&first.id).unwrap().latest_run.unwrap();
    assert_ne!(rerun1.id, run1.id);
    assert_eq!(rerun1.harness.harness_id, b.id);
    let rerun2 = e.ts.get_task(&second.id).unwrap().latest_run.unwrap();
    assert_ne!(rerun2.id, run2.id);
    assert_eq!(rerun2.harness.harness_id, b.id);
}

/// 「取消并重新执行」重跑失败：换绑不回滚，命令成功返回，失败的任务留在待执行（D35⑤）。
#[test]
fn a_failed_rerun_keeps_the_rebind_and_reports_it() {
    let e = env(false);
    let b = second_harness(&e, "Mock B");
    let task = e.task("构建 [mock:perm]");
    let run = e.ts.start_run(&task.id).unwrap();
    wait_permission(&e, &run.id);

    // 引用的文件没了：重跑必然失败（就绪条件不满足）。
    std::fs::remove_file(e.ws.join("docs/spec.md")).unwrap();

    // 命令成功返回，不报冲突。
    let view =
        e.ts.rebind_role(&e.role.id, &b.id, RebindMode::CancelAndRerun)
            .unwrap();
    // 换绑生效了，没有回滚。
    assert_eq!(view.role.harness_id.as_deref(), Some(b.id.as_str()));
    assert!(view.pending_binding.is_none(), "{view:?}");

    // 失败的任务留在待执行。
    let detail = e.ts.get_task(&task.id).unwrap();
    assert_eq!(detail.task.status, TaskStatus::Ready, "{detail:#?}");

    // 记了一条「重跑失败」活动。
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
            .any(|event| event.kind == EventKind::RoleRebindRerunFailed),
        "{events:?}"
    );
}

/// 应用重启把残留执行标为中断后，该岗位的待生效换绑随之生效（D35 严重项 2）。
#[test]
fn a_pending_rebind_applies_after_a_restart_recovers_the_runs() {
    let e = env(false);
    let b = second_harness(&e, "Mock B");
    let task = e.task("构建 [mock:perm]");
    let run = e.ts.start_run(&task.id).unwrap();
    wait_permission(&e, &run.id);

    // 登记待生效换绑：当前绑定不变。
    e.ts.rebind_role(&e.role.id, &b.id, RebindMode::AfterCurrent)
        .unwrap();
    assert_eq!(
        e.ts.get_role_workbench(&e.role.id)
            .unwrap()
            .view
            .role
            .harness_id
            .as_deref(),
        Some(e.harness.id.as_str())
    );

    // 模拟应用重启：新实例把残留执行标为中断，待生效换绑随之生效。
    e.ts.release_instance();
    let config = HostConfig {
        mock_agent: Some(AgentCommand {
            program: mock_agent(),
            args: vec![],
        }),
    };
    let restarted = Teamspace::open_with(
        &e.dir.path().join("data/t.db"),
        Box::new(LocalProbe::new()),
        config,
    )
    .unwrap();

    let view = wait_binding_of(&restarted, &e.role.id, &b.id);
    assert!(view.pending_binding.is_none(), "{view:?}");
    // 恢复后的执行是已中断。
    let recovered = restarted.get_run(&run.id).unwrap();
    assert_eq!(recovered.status, RunStatus::Interrupted, "{recovered:#?}");
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
