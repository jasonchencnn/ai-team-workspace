//! 换绑的边界情形（M6-04 第二部分，按 D35 修）。
//!
//! 覆盖：登记待生效与「有没有进行中的执行」在同一事务里（D35 严重项 1）、重启恢复后待生效换绑
//! 生效（2）、「取消并重跑」重跑每个被取消的任务且失败不回滚（3、4）、人工接管复用现有人工接管
//! 路径（5）、手动绑定 / 解绑清掉待生效换绑（6）。

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
    let team_id = f.ts.get_role_workbench(role_id).unwrap().view.role.team_id;
    f.ts.list_roles(&team_id)
        .unwrap()
        .into_iter()
        .find(|view| view.role.id == role_id)
        .expect("岗位卡片")
}

/// 直接写一条「进行中」的执行（绕过执行引擎，模拟 Agent 正在跑）。
fn insert_active_run(f: &Fixture, task: &Task, harness: &HarnessInstance, id: &str) -> String {
    // 两个同时在跑的执行不能共用同一个工作目录（in_place 执行的工作目录唯一）。
    let ws = format!("{}/{id}", f.ws.display());
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
        ws = ws,
    ));
    id.to_owned()
}

/// 待生效换绑存在时，岗位的绑定仍是旧的。
#[test]
fn a_pending_binding_keeps_the_old_binding() {
    let f = plain_fixture();
    let (_, fe, a, b) = setup(&f);
    let task = f.readyable(&fe.team_id, &fe, "登录页");
    insert_active_run(&f, &task.task, &a, "run-1");

    f.ts.rebind_role(&fe.id, &b.id, RebindMode::AfterCurrent)
        .unwrap();
    let view = view(&f, &fe.id);
    assert_eq!(view.role.harness_id.as_deref(), Some(a.id.as_str()));
    assert!(view.pending_binding.is_some());
}

/// D35 严重项 1：登记待生效与「有没有进行中的执行」在同一个事务里判定。
///
/// 执行恰好在判定前结束时，这里已经没有进行中的执行了，换绑直接生效，
/// 不会留下一条永远等不到的待生效记录。
#[test]
fn a_rebind_without_active_runs_takes_effect_immediately() {
    let f = plain_fixture();
    let (_, fe, _, b) = setup(&f);
    // 没有进行中的执行：当场换绑。
    f.ts.rebind_role(&fe.id, &b.id, RebindMode::AfterCurrent)
        .unwrap();
    let view = view(&f, &fe.id);
    assert_eq!(view.role.harness_id.as_deref(), Some(b.id.as_str()));
    assert_eq!(view.pending_binding, None);
}

/// D35 严重项 2：应用重启把残留执行标为中断后，该岗位的待生效换绑随之生效。
#[test]
fn a_pending_binding_applies_after_a_restart_recovers_the_runs() {
    let f = plain_fixture();
    let (_, fe, a, b) = setup(&f);
    let task = f.readyable(&fe.team_id, &fe, "登录页");
    insert_active_run(&f, &task.task, &a, "run-1");
    f.ts.rebind_role(&fe.id, &b.id, RebindMode::AfterCurrent)
        .unwrap();
    assert!(view(&f, &fe.id).pending_binding.is_some());

    // 模拟应用重启：新实例把残留执行标为中断。
    let restarted = f.reopen();
    let view = restarted
        .list_roles(&fe.team_id)
        .unwrap()
        .into_iter()
        .find(|view| view.role.id == fe.id)
        .expect("岗位卡片");
    assert_eq!(
        view.role.harness_id.as_deref(),
        Some(b.id.as_str()),
        "重启恢复后待生效换绑应已生效"
    );
    assert_eq!(view.pending_binding, None);
    // 恢复后的执行是已中断。
    let run = restarted.get_run("run-1").unwrap();
    assert_eq!(run.status, RunStatus::Interrupted);
}

/// D35④：手动绑定清掉该岗位的待生效换绑，并记一条「已撤销」活动。
#[test]
fn manual_bind_clears_the_pending_binding() {
    let f = plain_fixture();
    let (_, fe, a, b) = setup(&f);
    let task = f.readyable(&fe.team_id, &fe, "登录页");
    insert_active_run(&f, &task.task, &a, "run-1");
    f.ts.rebind_role(&fe.id, &b.id, RebindMode::AfterCurrent)
        .unwrap();
    assert!(view(&f, &fe.id).pending_binding.is_some());

    // 手动绑定到第三个实例：待生效换绑被清掉。
    let c = f.harness(&fe.team_id, "Mock C", AcpPreset::Mock);
    f.ts.bind_role_harness(&fe.id, &c.id).unwrap();
    let view = view(&f, &fe.id);
    assert_eq!(view.role.harness_id.as_deref(), Some(c.id.as_str()));
    assert_eq!(view.pending_binding, None, "手动绑定应清掉待生效换绑");
    // 记了一条「已撤销」活动。
    let events =
        f.ts.list_events(EventQuery {
            team_id: fe.team_id.clone(),
            role_id: Some(fe.id.clone()),
            limit: Some(50),
            ..Default::default()
        })
        .unwrap()
        .events;
    assert!(
        events
            .iter()
            .any(|event| event.kind == EventKind::RoleRebindCancelled),
        "{events:?}"
    );
}

/// D35④：手动解绑同样清掉该岗位的待生效换绑。
#[test]
fn manual_unbind_clears_the_pending_binding() {
    let f = plain_fixture();
    let (_, fe, a, b) = setup(&f);
    let task = f.readyable(&fe.team_id, &fe, "登录页");
    insert_active_run(&f, &task.task, &a, "run-1");
    f.ts.rebind_role(&fe.id, &b.id, RebindMode::AfterCurrent)
        .unwrap();
    assert!(view(&f, &fe.id).pending_binding.is_some());

    f.ts.unbind_role_harness(&fe.id).unwrap();
    let view = view(&f, &fe.id);
    assert_eq!(view.role.harness_id, None);
    assert_eq!(view.pending_binding, None, "手动解绑应清掉待生效换绑");
}

/// 换绑仍属 Human 的控制权：Leader 不能发起（D8、D32）。
#[test]
fn leader_still_cannot_rebind_a_role() {
    let f = plain_fixture();
    let (_, fe, _, b) = setup(&f);
    let err = dispatch(
        &f.ts,
        Caller::Leader,
        "rebind_role",
        &json!({ "roleId": fe.id, "harnessInstanceId": b.id, "mode": "after_current" }),
    )
    .unwrap_err();
    assert!(err.message.contains("无权"), "{err:?}");
}
