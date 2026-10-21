//! 待我处理：按类别聚合、处理后按真实状态消失、Leader 越权记录与权限边界。
//!
//! 不存「已读」：每个用例改完真实状态（任务流转、执行状态、消息状态、决策）后重新聚合，
//! 事项应当自然消失。

use serde_json::{Value, json};

use super::*;
use crate::dispatch::{Caller, dispatch};
use crate::model::{InboxCategory, InboxTarget};
use crate::service::message as message_svc;

/// 团队与前端岗位。
fn setup(f: &Fixture) -> (Team, Role) {
    let team = f.team("T");
    let fe = f.role(&team.id, "role_frontend");
    (team, fe)
}

/// 待我处理。
fn collect_inbox(f: &Fixture, team_id: &str) -> Inbox {
    f.ts.get_inbox(GetInboxInput {
        team_id: team_id.into(),
    })
    .unwrap()
}

/// 某类别的条目。
fn of_category(inbox: &Inbox, category: InboxCategory) -> Vec<&InboxItem> {
    inbox
        .items
        .iter()
        .filter(|item| item.category == category)
        .collect()
}

/// 插入一条「等待权限确认」的执行（模拟 Agent 发来权限请求）。
fn insert_waiting_run(
    f: &Fixture,
    team: &Team,
    role: &Role,
    task_id: &str,
    attempt: i64,
) -> String {
    let harness = f.harness(&team.id, "Mock", AcpPreset::Mock);
    let id = format!("run-perm-{attempt}");
    let prompt = json!({
        "requestId": format!("req-{attempt}"),
        "toolCallId": format!("call-{attempt}"),
        "title": "运行 cargo test",
        "toolKind": "execute",
        "paths": [],
        "command": "cargo test",
        "reason": "执行命令需要确认",
        "options": [{ "optionId": "allow", "name": "允许一次", "kind": "allow_once" }],
        "requestedAt": "2026-10-21T04:00:00.000Z",
    });
    f.raw_sql(&format!(
        "INSERT INTO runs (id, team_id, task_id, role_id, harness_id, harness_name, harness_preset,
                           harness_command, attempt, status, workspace_path, exec_dir, isolation,
                           started_at, pending_permission)
         VALUES ('{id}', '{team_id}', '{task_id}', '{role_id}', '{harness_id}', 'Mock', 'mock',
                 '/opt/teamspace/host', {attempt}, 'waiting_permission', '{ws}', '{ws}/exec',
                 'worktree', '2026-10-21T04:00:00.000Z', '{prompt}')",
        id = id,
        team_id = team.id,
        task_id = task_id,
        role_id = role.id,
        harness_id = harness.id,
        ws = f.ws.to_string_lossy(),
        prompt = prompt,
    ));
    id
}

// ---------------------------------------------------------------- 待验收

/// 待验收的任务出现在待我处理里；验收通过后自动消失。
#[test]
fn review_tasks_appear_and_disappear_after_approval() {
    let f = plain_fixture();
    let (team, fe) = setup(&f);
    let task = f.readyable(&team.id, &fe, "登录页");
    f.drive(
        &task.task.id,
        &[TaskStatus::Ready, TaskStatus::Doing, TaskStatus::Review],
    );

    let inbox = collect_inbox(&f, &team.id);
    assert_eq!(inbox.counts.review, 1);
    assert_eq!(inbox.counts.total, 1);
    let item = of_category(&inbox, InboxCategory::Review)[0];
    assert_eq!(item.task_key.as_deref(), Some(task.task.key.as_str()));
    assert_eq!(item.task_id.as_deref(), Some(task.task.id.as_str()));
    assert!(item.impact.contains("通过后"), "{}", item.impact);
    assert!(!item.suggestion.is_empty());
    assert!(matches!(item.target, InboxTarget::Task { .. }));
    assert!(!item.no_changes);

    // 验收通过：任务不再是待验收，事项按真实状态自动消失。
    f.move_to(&task.task.id, TaskStatus::Done).unwrap();
    let inbox = collect_inbox(&f, &team.id);
    assert_eq!(inbox.counts.review, 0);
    assert_eq!(inbox.counts.total, 0);
    assert!(inbox.items.is_empty());
}

/// 待验收但本次执行没有任何改动时，条目里带「无改动」提示。
#[test]
fn review_items_flag_runs_without_changes() {
    let f = plain_fixture();
    let (team, fe) = setup(&f);
    let task = f.readyable(&team.id, &fe, "登录页");
    f.drive(
        &task.task.id,
        &[TaskStatus::Ready, TaskStatus::Doing, TaskStatus::Review],
    );
    let harness = f.harness(&team.id, "Mock", AcpPreset::Mock);
    f.raw_sql(&format!(
        "INSERT INTO runs (id, team_id, task_id, role_id, harness_id, harness_name, harness_preset,
                           harness_command, attempt, status, workspace_path, exec_dir, isolation,
                           started_at, ended_at, no_changes, base_branch)
         VALUES ('run-none', '{team_id}', '{task_id}', '{role_id}', '{harness_id}', 'Mock', 'mock',
                 '/opt/teamspace/host', 1, 'succeeded', '{ws}', '{ws}/exec', 'worktree',
                 '2026-10-21T04:00:00.000Z', '2026-10-21T04:10:00.000Z', 1, 'claude/mvp-reset')",
        team_id = team.id,
        task_id = task.task.id,
        role_id = fe.id,
        harness_id = harness.id,
        ws = f.ws.to_string_lossy(),
    ));

    let inbox = collect_inbox(&f, &team.id);
    let item = of_category(&inbox, InboxCategory::Review)[0];
    assert!(item.no_changes, "{item:?}");
    assert!(item.impact.contains("没有任何改动"), "{}", item.impact);
    // 影响说明里带上合并目标分支。
    assert!(item.impact.contains("claude/mvp-reset"), "{}", item.impact);
}

// ---------------------------------------------------------------- 执行失败与受阻

/// 受阻的任务出现在待我处理里；恢复后自动消失。
#[test]
fn blocked_tasks_appear_and_disappear_after_resuming() {
    let f = plain_fixture();
    let (team, fe) = setup(&f);
    let task = f.readyable(&team.id, &fe, "登录页");
    f.drive(
        &task.task.id,
        &[TaskStatus::Ready, TaskStatus::Doing, TaskStatus::Blocked],
    );

    let inbox = collect_inbox(&f, &team.id);
    assert_eq!(inbox.counts.blocked, 1);
    let item = of_category(&inbox, InboxCategory::Blocked)[0];
    assert_eq!(item.task_key.as_deref(), Some(task.task.key.as_str()));
    assert_eq!(item.impact, "等待接口");

    // 恢复为 READY：不再是受阻，事项自动消失。
    f.move_to(&task.task.id, TaskStatus::Ready).unwrap();
    let inbox = collect_inbox(&f, &team.id);
    assert_eq!(inbox.counts.blocked, 0);
    assert!(inbox.items.is_empty());
}

/// 合并冲突单独提示：建议给出「已手动合并」或「重新执行」两个动作。
#[test]
fn merge_conflicts_are_reported_with_both_actions() {
    let f = plain_fixture();
    let (team, fe) = setup(&f);
    let task = f.readyable(&team.id, &fe, "登录页");
    f.raw_sql(&format!(
        "UPDATE tasks SET status = 'blocked', blocked_reason = '合并冲突：src/app.ts 两侧都改过'
         WHERE id = '{}'",
        task.task.id
    ));

    let inbox = collect_inbox(&f, &team.id);
    let item = of_category(&inbox, InboxCategory::Blocked)[0];
    assert!(item.title.contains("合并冲突"), "{}", item.title);
    assert!(item.impact.contains("冲突"), "{}", item.impact);
    assert!(
        item.suggestion.contains("已手动合并"),
        "{}",
        item.suggestion
    );
    assert!(item.suggestion.contains("重新执行"), "{}", item.suggestion);
}

/// 执行失败（受阻原因以「执行失败：」开头）标成失败而不是普通受阻。
#[test]
fn failed_runs_are_reported_as_failures() {
    let f = plain_fixture();
    let (team, fe) = setup(&f);
    let task = f.readyable(&team.id, &fe, "登录页");
    f.raw_sql(&format!(
        "UPDATE tasks SET status = 'blocked', blocked_reason = '执行失败：cargo test 退出码 1'
         WHERE id = '{}'",
        task.task.id
    ));

    let inbox = collect_inbox(&f, &team.id);
    let item = of_category(&inbox, InboxCategory::Blocked)[0];
    assert!(item.title.contains("执行失败"), "{}", item.title);
    assert!(item.impact.contains("cargo test"), "{}", item.impact);
}

// ---------------------------------------------------------------- 权限请求

/// 等待答复的权限请求出现在待我处理里；执行不再等待权限后自动消失。
#[test]
fn permission_requests_appear_until_answered() {
    let f = plain_fixture();
    let (team, fe) = setup(&f);
    let task = f.readyable(&team.id, &fe, "登录页");
    let run_id = insert_waiting_run(&f, &team, &fe, &task.task.id, 1);

    let inbox = collect_inbox(&f, &team.id);
    assert_eq!(inbox.counts.permission, 1);
    let item = of_category(&inbox, InboxCategory::Permission)[0];
    assert_eq!(item.run_id.as_deref(), Some(run_id.as_str()));
    assert_eq!(item.task_key.as_deref(), Some(task.task.key.as_str()));
    assert!(item.title.contains("运行 cargo test"), "{}", item.title);
    assert_eq!(item.impact, "执行命令需要确认");
    assert!(matches!(item.target, InboxTarget::Permission { .. }));

    // 权限已答复：执行不再等待权限，事项自动消失。
    f.raw_sql(&format!(
        "UPDATE runs SET status = 'running', pending_permission = NULL WHERE id = '{run_id}'"
    ));
    let inbox = collect_inbox(&f, &team.id);
    assert_eq!(inbox.counts.permission, 0);
    assert!(inbox.items.is_empty());
}

// ---------------------------------------------------------------- 需要拍板的消息

/// 岗位发来还没答复的消息要你拍板；记了决策之后自动消失。
#[test]
fn role_messages_wait_for_a_decision_and_disappear_after_it() {
    let f = plain_fixture();
    f.ts.core_handle().deliveries.stop();
    let (team, fe) = setup(&f);
    let be = f.role(&team.id, "role_backend");
    let message =
        f.ts.send_message_as(
            Actor::System,
            SendMessageInput {
                team_id: team.id.clone(),
                to_role_id: be.id.clone(),
                body: "接口字段改成了驼峰，请确认要不要同步改前端。".into(),
                task_id: None,
                thread_id: None,
                subject: None,
                from_role_id: Some(fe.id.clone()),
            },
        )
        .unwrap();
    assert_eq!(message.sender, MessageSender::Role);
    // 发信岗位记进消息里，待拍板的事项报的是发信岗位的名字。
    assert_eq!(message.sender_role_id.as_deref(), Some(fe.id.as_str()));

    let inbox = collect_inbox(&f, &team.id);
    assert_eq!(inbox.counts.decision, 1);
    let item = of_category(&inbox, InboxCategory::Decision)[0];
    assert_eq!(item.message_id.as_deref(), Some(message.id.as_str()));
    assert_eq!(item.thread_id.as_deref(), Some(message.thread_id.as_str()));
    assert!(item.title.contains("前端开发"), "{}", item.title);
    assert!(matches!(item.target, InboxTarget::Message { .. }));

    // 记了决策：消息不再要你拍板。
    f.ts.record_decision(RecordDecisionInput {
        team_id: team.id.clone(),
        task_id: None,
        message_id: Some(message.id.clone()),
        text: "改成驼峰，前端同步改".into(),
        resume_task: None,
    })
    .unwrap();
    let inbox = collect_inbox(&f, &team.id);
    assert_eq!(inbox.counts.decision, 0);
    assert!(inbox.items.is_empty());
}

/// 投递失败的消息要你拍板；重发之后回到排队，事项自动消失。
#[test]
fn failed_messages_need_a_decision_and_disappear_when_retried() {
    let f = plain_fixture();
    f.ts.core_handle().deliveries.stop();
    let (team, fe) = setup(&f);
    let message =
        f.ts.send_message(SendMessageInput {
            team_id: team.id.clone(),
            to_role_id: fe.id.clone(),
            body: "进度如何".into(),
            task_id: None,
            thread_id: None,
            subject: None,
            from_role_id: None,
        })
        .unwrap();
    message_svc::mark_failed(&f.ts.lock(), &message.id, "岗位未绑定 Harness").unwrap();

    let inbox = collect_inbox(&f, &team.id);
    assert_eq!(inbox.counts.decision, 1);
    let item = of_category(&inbox, InboxCategory::Decision)[0];
    assert_eq!(item.message_id.as_deref(), Some(message.id.as_str()));
    assert!(item.impact.contains("未绑定 Harness"), "{}", item.impact);

    // 重发：消息回到排队，不再需要拍板。
    f.ts.retry_message(&message.id).unwrap();
    let inbox = collect_inbox(&f, &team.id);
    assert_eq!(inbox.counts.decision, 0);
    assert!(inbox.items.is_empty());
}

/// 记过决策的消息又投递失败：重新出现，等着再拍一次板。
#[test]
fn a_message_that_fails_again_comes_back_to_the_inbox() {
    let f = plain_fixture();
    f.ts.core_handle().deliveries.stop();
    let (team, fe) = setup(&f);
    let message =
        f.ts.send_message(SendMessageInput {
            team_id: team.id.clone(),
            to_role_id: fe.id.clone(),
            body: "进度如何".into(),
            task_id: None,
            thread_id: None,
            subject: None,
            from_role_id: None,
        })
        .unwrap();
    message_svc::mark_failed(&f.ts.lock(), &message.id, "岗位未绑定 Harness").unwrap();
    assert_eq!(collect_inbox(&f, &team.id).counts.decision, 1);

    // 记了决策：消息不再要你拍板。
    f.ts.record_decision(RecordDecisionInput {
        team_id: team.id.clone(),
        task_id: None,
        message_id: Some(message.id.clone()),
        text: "换个 Harness 再试".into(),
        resume_task: None,
    })
    .unwrap();
    let inbox = collect_inbox(&f, &team.id);
    assert_eq!(inbox.counts.decision, 0);

    // 重发后又被投递引擎判失败：又需要拍板了。
    f.ts.retry_message(&message.id).unwrap();
    message_svc::mark_failed(&f.ts.lock(), &message.id, "岗位仍未绑定 Harness").unwrap();
    let inbox = collect_inbox(&f, &team.id);
    assert_eq!(inbox.counts.decision, 1, "决策之后又失败，消息该重新出现");
    let item = of_category(&inbox, InboxCategory::Decision)[0];
    assert_eq!(item.message_id.as_deref(), Some(message.id.as_str()));
    assert!(item.impact.contains("仍未绑定 Harness"), "{}", item.impact);
}

// ---------------------------------------------------------------- Leader 越权记录

/// Leader 越权记录出现在待我处理里；超过 7 天后自动消失。
#[test]
fn leader_denied_entries_are_listed_for_the_last_seven_days() {
    let f = plain_fixture();
    let (team, fe) = setup(&f);
    let err = dispatch(
        &f.ts,
        Caller::Leader,
        "record_decision",
        &json!({ "input": { "teamId": team.id, "taskId": "" } }),
    )
    .unwrap_err();
    assert_eq!(err.kind, crate::error::ErrorKind::Conflict);
    let _ = fe;

    let inbox = collect_inbox(&f, &team.id);
    assert_eq!(inbox.counts.leader_denied, 1);
    let item = of_category(&inbox, InboxCategory::LeaderDenied)[0];
    assert!(item.title.contains("记录决策"), "{}", item.title);
    assert!(item.impact.contains("Leader"), "{}", item.impact);
    assert!(matches!(item.target, InboxTarget::Activity { .. }));

    // 超过 7 天：不再出现在待我处理里。
    let old = (time::OffsetDateTime::now_utc() - time::Duration::days(8))
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap();
    f.raw_sql(&format!(
        "UPDATE events SET created_at = '{old}' WHERE kind = 'leader_denied'"
    ));
    let inbox = collect_inbox(&f, &team.id);
    assert_eq!(inbox.counts.leader_denied, 0);
    assert!(inbox.items.is_empty());
}

// ---------------------------------------------------------------- 计数与权限边界

/// 计数汇总覆盖每个类别，总数等于条目数。
#[test]
fn counts_summarise_every_category() {
    let f = plain_fixture();
    let (team, fe) = setup(&f);
    let task = f.readyable(&team.id, &fe, "登录页");
    f.drive(
        &task.task.id,
        &[TaskStatus::Ready, TaskStatus::Doing, TaskStatus::Review],
    );
    let blocked = f.readyable(&team.id, &fe, "支付页");
    f.drive(
        &blocked.task.id,
        &[TaskStatus::Ready, TaskStatus::Doing, TaskStatus::Blocked],
    );
    insert_waiting_run(&f, &team, &fe, &task.task.id, 1);

    let inbox = collect_inbox(&f, &team.id);
    assert_eq!(inbox.counts.review, 1);
    assert_eq!(inbox.counts.blocked, 1);
    assert_eq!(inbox.counts.permission, 1);
    assert_eq!(inbox.counts.decision, 0);
    assert_eq!(inbox.counts.leader_denied, 0);
    assert_eq!(inbox.counts.total, 3);
    assert_eq!(inbox.counts.total as usize, inbox.items.len());
    // 按发生时间倒序。
    let times: Vec<&str> = inbox.items.iter().map(|i| i.occurred_at.as_str()).collect();
    let mut sorted = times.clone();
    sorted.sort_by(|a, b| b.cmp(a));
    assert_eq!(times, sorted);
}

/// Leader 可以看待我处理（只读），但不能记录决策、也不能改通知订阅。
#[test]
fn leader_may_read_the_inbox_but_not_record_decisions() {
    let f = plain_fixture();
    let (team, fe) = setup(&f);
    let task = f.readyable(&team.id, &fe, "登录页");

    // 只读命令在白名单里。
    let value = dispatch(
        &f.ts,
        Caller::Leader,
        "get_inbox",
        &json!({ "input": { "teamId": team.id } }),
    )
    .unwrap();
    let parsed: Value = serde_json::to_value(value).unwrap();
    assert_eq!(parsed["counts"]["total"], 0);

    // 记决策、改订阅设置不在白名单里。
    for cmd in [
        "record_decision",
        "list_decisions",
        "get_notification_settings",
        "update_notification_settings",
    ] {
        let err = dispatch(
            &f.ts,
            Caller::Leader,
            cmd,
            &json!({ "input": { "teamId": team.id, "taskId": task.task.id } }),
        )
        .unwrap_err();
        assert_eq!(err.kind, crate::error::ErrorKind::Conflict, "{cmd}");
        assert!(
            err.message.starts_with("Leader 无权执行此操作："),
            "{cmd}: {}",
            err.message
        );
    }
}
