//! 岗位换绑的规则（M6-01）：三种方式、待生效与撤销、立即生效、Leader 被拒、消息绑定快照。
//!
//! 这里不启动任何 Agent（Mock 预设指向不存在的宿主程序）；执行真跑起来的换绑与投递见
//! `tests/m6_01_rebinds.rs`。

use super::*;
use crate::dispatch::{Caller, dispatch};
use serde_json::json;

/// 团队 T、岗位「前端」（绑定 Mock A）、Harness B。
fn setup(f: &Fixture) -> (Team, Role, HarnessInstance, HarnessInstance) {
    let team = f.team("T");
    let fe = f.role(&team.id, "role_frontend");
    let a = f.harness(&team.id, "Mock A", AcpPreset::Mock);
    let b = f.harness(&team.id, "Mock B", AcpPreset::Mock);
    f.ts.bind_role_harness(&fe.id, &a.id).unwrap();
    (team, fe, a, b)
}

/// 岗位卡片（含待生效换绑）。
fn view(f: &Fixture, role_id: &str) -> RoleView {
    f.ts.list_roles(&team_of(f, role_id))
        .unwrap()
        .into_iter()
        .find(|view| view.role.id == role_id)
        .expect("岗位卡片")
}

/// 岗位所属团队。
fn team_of(f: &Fixture, role_id: &str) -> String {
    f.ts.get_role_workbench(role_id).unwrap().view.role.team_id
}

/// 直接写一条「进行中」的执行（绕过执行引擎，模拟 Agent 正在跑）。
fn insert_active_run(f: &Fixture, task: &Task, harness: &HarnessInstance) -> String {
    let id = format!("run-{}", task.key);
    f.raw_sql(&format!(
        "UPDATE tasks SET status = 'doing' WHERE id = '{task_id}';
         INSERT INTO runs (id, team_id, task_id, role_id, harness_id, harness_name, harness_preset,
                           harness_command, attempt, status, session_id, workspace_path, exec_dir,
                           isolation, capabilities, started_at)
         VALUES ('{id}', '{team}', '{task_id}', '{role}', '{h}', 'Mock', 'mock', 'x', 1, 'running',
                 's-1', '{ws}', '{ws}', 'in_place', '{{\"loadSession\":true,\"image\":false,\"audio\":false,\"embeddedContext\":false,\"mcpHttp\":false,\"mcpSse\":false}}',
                 '2026-10-13T00:00:00.000Z');",
        task_id = task.id,
        team = task.team_id,
        role = task.owner_role_id,
        h = harness.id,
        ws = f.ws.display(),
    ));
    id
}

/// 没有进行中的执行时换绑立即生效：绑定换过去，没有待生效换绑。
#[test]
fn after_current_without_a_running_task_takes_effect_immediately() {
    let f = plain_fixture();
    let (_, fe, a, b) = setup(&f);

    let view =
        f.ts.rebind_role(&fe.id, &b.id, RebindMode::AfterCurrent)
            .unwrap();
    assert_eq!(view.role.harness_id.as_deref(), Some(b.id.as_str()));
    assert!(view.pending_binding.is_none(), "{view:?}");
    assert_eq!(view.harness.unwrap().id, b.id);

    // 活动留痕：换绑生效。
    let events =
        f.ts.list_events(EventQuery {
            team_id: view.role.team_id.clone(),
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
    assert!(
        !events
            .events
            .iter()
            .any(|event| event.kind == EventKind::RoleRebindRequested),
        "{events:?}"
    );
    let _ = a;
}

/// 有进行中的执行时登记「换绑待生效」：当前绑定不变，新的派工仍走旧绑定。
#[test]
fn after_current_with_a_running_task_records_a_pending_binding() {
    let f = plain_fixture();
    let (_, fe, a, b) = setup(&f);
    let task = f.readyable(&fe.team_id, &fe, "登录页");
    let run_id = insert_active_run(&f, &task.task, &a);

    let view =
        f.ts.rebind_role(&fe.id, &b.id, RebindMode::AfterCurrent)
            .unwrap();
    // 当前绑定没动：新的派工因此仍走旧绑定。
    assert_eq!(view.role.harness_id.as_deref(), Some(a.id.as_str()));
    assert_eq!(
        view.harness.as_ref().map(|harness| harness.id.as_str()),
        Some(a.id.as_str())
    );
    let pending = view.pending_binding.expect("应登记待生效换绑");
    assert_eq!(pending.harness_id, b.id);
    assert_eq!(pending.mode, RebindMode::AfterCurrent);
    assert_eq!(pending.active_run_id.as_deref(), Some(run_id.as_str()));
    assert_eq!(pending.role_id, fe.id);

    // 活动留痕：换绑待生效。
    let events =
        f.ts.list_events(EventQuery {
            team_id: fe.team_id.clone(),
            ..Default::default()
        })
        .unwrap();
    let requested = events
        .events
        .iter()
        .find(|event| event.kind == EventKind::RoleRebindRequested)
        .expect("应记录换绑待生效");
    assert_eq!(requested.data["harnessId"], b.id);
    assert_eq!(requested.data["mode"], "after_current");
}

/// 待生效换绑可以撤销：当前绑定不变，之后执行结束也不会换过去。
#[test]
fn a_pending_binding_can_be_cancelled() {
    let f = plain_fixture();
    let (_, fe, a, b) = setup(&f);
    let task = f.readyable(&fe.team_id, &fe, "登录页");
    insert_active_run(&f, &task.task, &a);

    f.ts.rebind_role(&fe.id, &b.id, RebindMode::AfterCurrent)
        .unwrap();
    let view = f.ts.cancel_pending_rebind(&fe.id).unwrap();
    assert!(view.pending_binding.is_none(), "{view:?}");
    assert_eq!(view.role.harness_id.as_deref(), Some(a.id.as_str()));

    // 撤销也要留痕。
    let events =
        f.ts.list_events(EventQuery {
            team_id: fe.team_id.clone(),
            ..Default::default()
        })
        .unwrap();
    assert!(
        events
            .events
            .iter()
            .any(|event| event.kind == EventKind::RoleRebindCancelled),
        "{events:?}"
    );

    // 没有待生效换绑时再撤销是冲突。
    assert_kind(f.ts.cancel_pending_rebind(&fe.id), ErrorKind::Conflict);
}

/// 换绑到同一个 Harness 是无效操作。
#[test]
fn rebinding_to_the_same_harness_is_refused() {
    let f = plain_fixture();
    let (_, fe, a, _) = setup(&f);
    let err = assert_kind(
        f.ts.rebind_role(&fe.id, &a.id, RebindMode::AfterCurrent),
        ErrorKind::Invalid,
    );
    assert!(err.contains("无需更换"), "{err}");
}

/// 不能换绑到别的团队的 Harness。
#[test]
fn rebinding_to_another_teams_harness_is_refused() {
    let f = plain_fixture();
    let (_, fe, _, _) = setup(&f);
    let other = f.team("另一个团队");
    let foreign = f.harness(&other.id, "别家的", AcpPreset::Mock);
    let err = assert_kind(
        f.ts.rebind_role(&fe.id, &foreign.id, RebindMode::AfterCurrent),
        ErrorKind::Invalid,
    );
    assert!(err.contains("不属于该岗位所在的团队"), "{err}");
}

/// Leader 不能换绑（D8、D32）：被拒绝并留一条 `leader_denied` 痕迹。
#[test]
fn leader_cannot_rebind_a_role() {
    let f = plain_fixture();
    let (_, fe, a, b) = setup(&f);

    let err = dispatch(
        &f.ts,
        Caller::Leader,
        "rebind_role",
        &json!({ "roleId": fe.id, "harnessInstanceId": b.id, "mode": "after_current" }),
    )
    .unwrap_err();
    assert_eq!(err.kind, crate::error::ErrorKind::Conflict);
    assert!(
        err.message.starts_with("Leader 无权执行此操作："),
        "{}",
        err.message
    );

    // 绑定没变，也没有待生效换绑。
    let view = view(&f, &fe.id);
    assert_eq!(view.role.harness_id.as_deref(), Some(a.id.as_str()));
    assert!(view.pending_binding.is_none(), "{view:?}");

    // 越权留痕。
    let events =
        f.ts.list_events(EventQuery {
            team_id: fe.team_id.clone(),
            ..Default::default()
        })
        .unwrap();
    let denied = events
        .events
        .into_iter()
        .find(|event| event.kind == EventKind::LeaderDenied)
        .expect("越权操作应留下活动");
    assert_eq!(denied.actor, Actor::Leader);
    assert_eq!(denied.data["command"], "rebind_role");
}

/// Leader 也不能撤销待生效换绑。
#[test]
fn leader_cannot_cancel_a_pending_rebind() {
    let f = plain_fixture();
    let (_, fe, _, b) = setup(&f);
    let err = dispatch(
        &f.ts,
        Caller::Leader,
        "cancel_pending_rebind",
        &json!({ "roleId": fe.id }),
    )
    .unwrap_err();
    assert_eq!(err.kind, crate::error::ErrorKind::Conflict);

    let events =
        f.ts.list_events(EventQuery {
            team_id: fe.team_id.clone(),
            ..Default::default()
        })
        .unwrap();
    assert!(
        events
            .events
            .iter()
            .any(|event| event.kind == EventKind::LeaderDenied),
        "{events:?}"
    );
    let _ = b;
}

/// 消息入队时记下当时绑定的 Harness（快照，仅留痕）：换绑后快照不变（D33）。
#[test]
fn a_message_keeps_the_binding_snapshot_from_enqueue_time() {
    let f = plain_fixture();
    let (_, fe, a, b) = setup(&f);

    let message =
        f.ts.send_message(SendMessageInput {
            team_id: fe.team_id.clone(),
            to_role_id: fe.id.clone(),
            body: "请处理登录页".into(),
            task_id: None,
            thread_id: None,
            subject: None,
            from_role_id: None,
        })
        .unwrap();
    assert_eq!(message.binding_harness_id.as_deref(), Some(a.id.as_str()));

    // 换绑后快照仍是入队时的那个绑定。
    f.ts.rebind_role(&fe.id, &b.id, RebindMode::AfterCurrent)
        .unwrap();
    let after =
        f.ts.get_thread(&message.thread_id)
            .unwrap()
            .messages
            .into_iter()
            .find(|m| m.id == message.id)
            .expect("消息还在");
    assert_eq!(after.binding_harness_id.as_deref(), Some(a.id.as_str()));
}
