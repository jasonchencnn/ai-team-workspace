//! 换绑界面的后端查询（M6-04）：岗位进行中的执行、发往岗位的消息、待生效换绑的旧绑定。
//!
//! 这些判定以前散在前端（自己列状态白名单、逐个查任务与线程、靠岗位当前绑定反推旧实例名）；
//! 现在都在内核里。这里的测试把「哪些算进行中」「哪些消息算待投递」和
//! 「旧绑定记的是发起那一刻的哪个实例」锁住。

use super::*;
use crate::dispatch::{Caller, dispatch};
use serde_json::json;

/// 打开不启动投递线程的夹具：消息停在「排队」，投递时机由用例自己控制。
fn fixture() -> Fixture {
    let f = plain_fixture();
    f.ts.core_handle().deliveries.stop();
    f
}

/// 团队 T、岗位「前端」（绑定 Mock A）、Harness B。
fn setup(f: &Fixture) -> (Team, Role, HarnessInstance, HarnessInstance) {
    let team = f.team("T");
    let fe = f.role(&team.id, "role_frontend");
    let a = f.harness(&team.id, "Mock A", AcpPreset::Mock);
    let b = f.harness(&team.id, "Mock B", AcpPreset::Mock);
    f.ts.bind_role_harness(&fe.id, &a.id).unwrap();
    (team, fe, a, b)
}

/// 直接写一条执行（绕过执行引擎，模拟 Agent 正在跑）。
fn insert_run(f: &Fixture, task: &Task, harness: &HarnessInstance, status: &str, id: &str) {
    f.raw_sql(&format!(
        "UPDATE tasks SET status = 'doing' WHERE id = '{task_id}';
         INSERT INTO runs (id, team_id, task_id, role_id, harness_id, harness_name, harness_preset,
                           harness_command, attempt, status, session_id, workspace_path, exec_dir,
                           isolation, capabilities, started_at)
         VALUES ('{id}', '{team}', '{task_id}', '{role}', '{h}', '{harness_name}', 'mock', 'x', 1, '{status}',
                 's-1', '{ws}', '{ws}', 'in_place', '{{\"loadSession\":true,\"image\":false,\"audio\":false,\"embeddedContext\":false,\"mcpHttp\":false,\"mcpSse\":false}}',
                 '2026-10-13T00:00:00.000Z');",
        task_id = task.id,
        team = task.team_id,
        role = task.owner_role_id,
        h = harness.id,
        harness_name = harness.name,
        ws = f.ws.display(),
    ));
}

/// 发一条消息给岗位。
fn send(f: &Fixture, team_id: &str, to_role_id: &str, body: &str) -> Message {
    f.ts.send_message(SendMessageInput {
        team_id: team_id.to_owned(),
        to_role_id: to_role_id.to_owned(),
        body: body.to_owned(),
        task_id: None,
        thread_id: None,
        subject: None,
        from_role_id: None,
    })
    .unwrap()
}

/// 岗位进行中的执行由内核给出：任务编号与标题、次数、状态与 Harness 实例都在结果里。
#[test]
fn active_runs_carry_task_and_harness_facts() {
    let f = fixture();
    let (_, fe, a, _) = setup(&f);
    let task = f.readyable(&fe.team_id, &fe, "登录页");
    insert_run(&f, &task.task, &a, "running", "run-1");

    let runs = f.ts.list_active_runs(&fe.id).unwrap();
    assert_eq!(runs.len(), 1, "{runs:?}");
    let run = &runs[0];
    assert_eq!(run.run_id, "run-1");
    assert_eq!(run.task_id, task.task.id);
    assert_eq!(run.task_key, task.task.key);
    assert_eq!(run.task_title, "登录页");
    assert_eq!(run.attempt, 1);
    assert_eq!(run.status, RunStatus::Running);
    assert_eq!(run.status_label, "执行中");
    assert_eq!(run.harness.harness_id, a.id);
    assert_eq!(run.harness.name, "Mock A");
}

/// 只有还在跑的算「进行中」：已结束的执行不算。
#[test]
fn only_running_executions_count_as_active() {
    let f = fixture();
    let (_, fe, a, _) = setup(&f);
    let task = f.readyable(&fe.team_id, &fe, "登录页");
    insert_run(&f, &task.task, &a, "succeeded", "run-done");

    assert!(f.ts.list_active_runs(&fe.id).unwrap().is_empty());
}

/// 别人的执行不算这个岗位的进行中执行。
#[test]
fn active_runs_are_scoped_to_the_role() {
    let f = fixture();
    let (_, fe, a, _) = setup(&f);
    let other = f.role(&fe.team_id, "role_backend");
    let task = f
        .try_task(
            &fe.team_id,
            &other,
            "后端接口",
            TaskStatus::Todo,
            vec![],
            vec![],
        )
        .unwrap();
    insert_run(&f, &task.task, &a, "running", "run-other");

    assert!(f.ts.list_active_runs(&fe.id).unwrap().is_empty());
    let other_runs = f.ts.list_active_runs(&other.id).unwrap();
    assert_eq!(other_runs.len(), 1, "{other_runs:?}");
    assert_eq!(other_runs[0].run_id, "run-other");
}

/// 发往岗位的消息由内核给出：线程主题、入队时的绑定实例名都在结果里。
#[test]
fn role_messages_carry_thread_and_binding_facts() {
    let f = fixture();
    let (_, fe, a, _) = setup(&f);
    let message = send(&f, &fe.team_id, &fe.id, "请处理登录页");

    let messages =
        f.ts.list_role_messages(ListRoleMessagesQuery {
            role_id: fe.id.clone(),
            statuses: None,
            limit: None,
        })
        .unwrap();
    assert_eq!(messages.len(), 1, "{messages:?}");
    let item = &messages[0];
    assert_eq!(item.id, message.id);
    assert_eq!(item.thread_id, message.thread_id);
    assert_eq!(item.thread_subject, "请处理登录页");
    assert_eq!(item.body, "请处理登录页");
    assert_eq!(item.status, MessageStatus::Queued);
    assert_eq!(item.status_label, "排队中");
    // 入队那一刻的绑定快照，界面直接用名字显示，不用再查实例列表。
    assert_eq!(item.binding_harness_id.as_deref(), Some(a.id.as_str()));
    assert_eq!(item.binding_harness_name.as_deref(), Some("Mock A"));
}

/// 终态消息不再算「待投递」，别的岗位的消息也不在这个列表里。
#[test]
fn only_pending_messages_of_that_role_are_listed() {
    let f = fixture();
    let (_, fe, _, _) = setup(&f);
    let other = f.role(&fe.team_id, "role_backend");

    let replied = send(&f, &fe.team_id, &fe.id, "已经回复的");
    crate::service::message::mark_replied(&f.ts.lock(), &replied.id, "收到").unwrap();
    let queued = send(&f, &fe.team_id, &fe.id, "还在排队的");
    send(&f, &fe.team_id, &other.id, "发给别人的");

    let messages =
        f.ts.list_role_messages(ListRoleMessagesQuery {
            role_id: fe.id.clone(),
            statuses: None,
            limit: None,
        })
        .unwrap();
    assert_eq!(messages.len(), 1, "{messages:?}");
    assert_eq!(messages[0].id, queued.id);
}

/// 按入队时间倒序：最近入队的排在前面。
#[test]
fn role_messages_are_ordered_by_queue_time() {
    let f = fixture();
    let (_, fe, _, _) = setup(&f);
    let older = send(&f, &fe.team_id, &fe.id, "先入队的");
    let newer = send(&f, &fe.team_id, &fe.id, "后入队的");
    f.raw_sql(&format!(
        "UPDATE messages SET queued_at = '2026-10-01T00:00:00.000Z' WHERE id = '{}'",
        older.id
    ));

    let messages =
        f.ts.list_role_messages(ListRoleMessagesQuery {
            role_id: fe.id.clone(),
            statuses: None,
            limit: None,
        })
        .unwrap();
    assert_eq!(messages.len(), 2, "{messages:?}");
    assert_eq!(messages[0].id, newer.id);
    assert_eq!(messages[1].id, older.id);
}

/// 一次最多返回 200 条：更多时只给最新的 200 条。
#[test]
fn role_messages_are_capped_at_two_hundred() {
    let f = fixture();
    let (_, fe, _, _) = setup(&f);
    // 直接批量写入 205 条排队消息。
    f.raw_sql(&format!(
        "INSERT INTO message_threads (id, team_id, task_id, subject, participants, created_at, updated_at)
         VALUES ('th-bulk', '{team}', NULL, '批量', '[]', '2026-10-01T00:00:00.000Z', '2026-10-01T00:00:00.000Z');
         WITH RECURSIVE seq(n) AS (SELECT 1 UNION ALL SELECT n + 1 FROM seq WHERE n < 205)
         INSERT INTO messages (id, thread_id, team_id, task_id, sender_kind, sender_role_id,
                               to_role_id, body, status, created_at, queued_at, delivered_at, replied_at)
         SELECT 'bulk-' || n, 'th-bulk', '{team}', NULL, 'human', NULL, '{role}', '正文', 'queued',
                '2026-10-01T00:00:00.000Z', '2026-10-01T00:00:00.000Z', NULL, NULL
         FROM seq;",
        team = fe.team_id,
        role = fe.id,
    ));

    let messages =
        f.ts.list_role_messages(ListRoleMessagesQuery {
            role_id: fe.id.clone(),
            statuses: None,
            limit: None,
        })
        .unwrap();
    assert_eq!(messages.len(), 200, "待投递消息上限是 200 条");

    // 显式要更少时按要求给。
    let few =
        f.ts.list_role_messages(ListRoleMessagesQuery {
            role_id: fe.id.clone(),
            statuses: None,
            limit: Some(5),
        })
        .unwrap();
    assert_eq!(few.len(), 5);
}

/// 待生效换绑记下发起时岗位的旧绑定：界面显示的旧实例名不用靠岗位现在的绑定反推。
#[test]
fn a_pending_binding_records_the_harness_bound_at_request_time() {
    let f = fixture();
    let (_, fe, a, b) = setup(&f);
    let task = f.readyable(&fe.team_id, &fe, "登录页");
    insert_run(&f, &task.task, &a, "running", "run-1");

    let view =
        f.ts.rebind_role(&fe.id, &b.id, RebindMode::AfterCurrent)
            .unwrap();
    let pending = view.pending_binding.expect("应登记待生效换绑");
    assert_eq!(pending.previous_harness_id.as_deref(), Some(a.id.as_str()));
    assert_eq!(pending.previous_harness_name.as_deref(), Some("Mock A"));
    // 目标仍是新实例，旧实例名不会串。
    assert_eq!(pending.harness_id, b.id);
    assert_eq!(pending.harness_name, "Mock B");
}

/// 发起换绑时岗位没有绑定过实例：旧绑定记为「无」，界面显示「未绑定」。
#[test]
fn a_pending_binding_without_a_previous_harness_records_none() {
    let f = fixture();
    let team = f.team("T");
    let fe = f.role(&team.id, "role_frontend");
    let a = f.harness(&team.id, "Mock A", AcpPreset::Mock);
    let b = f.harness(&team.id, "Mock B", AcpPreset::Mock);
    let task = f.readyable(&fe.team_id, &fe, "登录页");
    insert_run(&f, &task.task, &a, "running", "run-1");

    let view =
        f.ts.rebind_role(&fe.id, &b.id, RebindMode::AfterCurrent)
            .unwrap();
    let pending = view.pending_binding.expect("应登记待生效换绑");
    assert_eq!(pending.previous_harness_id, None);
    assert_eq!(pending.previous_harness_name, None);
}

/// Leader 可以读这两项（只读查询：不改任何状态）。
#[test]
fn leader_can_read_active_runs_and_role_messages() {
    let f = fixture();
    let (_, fe, a, _) = setup(&f);
    let task = f.readyable(&fe.team_id, &fe, "登录页");
    insert_run(&f, &task.task, &a, "running", "run-1");
    send(&f, &fe.team_id, &fe.id, "请处理登录页");

    let runs = dispatch(
        &f.ts,
        Caller::Leader,
        "list_active_runs",
        &json!({ "roleId": fe.id }),
    )
    .unwrap();
    let runs = runs.as_array().unwrap();
    assert_eq!(runs.len(), 1, "{runs:?}");
    assert_eq!(runs[0]["runId"], "run-1");
    assert_eq!(runs[0]["taskTitle"], "登录页");

    let messages = dispatch(
        &f.ts,
        Caller::Leader,
        "list_role_messages",
        &json!({ "input": { "roleId": fe.id } }),
    )
    .unwrap();
    let messages = messages.as_array().unwrap();
    assert_eq!(messages.len(), 1, "{messages:?}");
    assert_eq!(messages[0]["bindingHarnessName"], "Mock A");
}
