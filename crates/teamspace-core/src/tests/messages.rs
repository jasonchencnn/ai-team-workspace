//! 岗位消息：模型与状态流转、迁移、线程归组、排队、过期、重启恢复与 Leader 权限边界。
//!
//! 这些用例不启动消息投递线程（`open_without_delivery`），投递时机由用例自己控制；
//! 真实的 ACP 投递（回复、失败、只读策略）见 `tests/messages_delivery.rs`。

use serde_json::json;

use super::*;
use crate::dispatch::{Caller, dispatch};
use crate::service::message as message_svc;

/// 打开不启动投递线程的夹具：消息停在「排队」，状态流转由用例自己驱动。
fn fixture() -> Fixture {
    let f = plain_fixture();
    f.ts.core_handle().deliveries.stop();
    f
}

/// 发送一条消息（缺省字段都省掉）。
fn send(f: &Fixture, team_id: &str, to_role_id: &str, body: &str) -> Message {
    f.ts.send_message(SendMessageInput {
        team_id: team_id.into(),
        to_role_id: to_role_id.into(),
        body: body.into(),
        task_id: None,
        thread_id: None,
        subject: None,
    })
    .unwrap()
}

/// 团队、接收岗位与一个可关联的任务。
fn setup(f: &Fixture) -> (Team, Role, TaskDetail) {
    let team = f.team("T");
    let fe = f.role(&team.id, "role_frontend");
    let task = f.task(&team.id, &fe, "登录页");
    (team, fe, task)
}

/// 活动列表。
fn events(f: &Fixture, team_id: &str) -> Vec<Event> {
    f.ts.list_events(EventQuery {
        team_id: team_id.into(),
        limit: Some(200),
        ..Default::default()
    })
    .unwrap()
    .events
}

// ---------------------------------------------------------------- 模型与状态流转

/// 发送后消息进入「排队」，线程带上发送方与接收岗位。
#[test]
fn sending_a_message_creates_a_queued_message_in_a_thread() {
    let f = fixture();
    let (team, fe, _) = setup(&f);

    let message = send(&f, &team.id, &fe.id, "请同步一下进度");

    assert_eq!(message.status, MessageStatus::Queued);
    assert_eq!(message.sender, MessageSender::Human);
    assert_eq!(message.sender_role_id, None);
    assert_eq!(message.to_role_id, fe.id);
    assert_eq!(message.failure_reason, None);
    assert_eq!(message.reply_body, None);
    assert_eq!(message.delivered_at, None);
    assert_eq!(message.replied_at, None);

    let detail = f.ts.get_thread(&message.thread_id).unwrap();
    assert_eq!(detail.thread.team_id, team.id);
    assert_eq!(detail.thread.task_id, None);
    assert_eq!(detail.thread.subject, "请同步一下进度");
    assert_eq!(detail.messages.len(), 1);
    let kinds: Vec<_> = detail
        .thread
        .participants
        .iter()
        .map(|p| (p.kind, p.role_id.clone()))
        .collect();
    assert_eq!(
        kinds,
        [
            (MessageSender::Role, Some(fe.id.clone())),
            (MessageSender::Human, None)
        ]
    );
}

/// 状态流转：排队 → 投递中 → 已回复，回复正文与时间都记下。
#[test]
fn message_status_flows_from_queued_to_replied() {
    let f = fixture();
    let (team, fe, _) = setup(&f);
    let message = send(&f, &team.id, &fe.id, "进度如何");

    let delivering = message_svc::mark_delivering(&f.ts.lock(), &message.id).unwrap();
    assert_eq!(delivering.status, MessageStatus::Delivering);
    assert!(delivering.delivered_at.is_some());

    let replied = message_svc::mark_replied(&f.ts.lock(), &message.id, "已完成一半").unwrap();
    assert_eq!(replied.status, MessageStatus::Replied);
    assert_eq!(replied.reply_body.as_deref(), Some("已完成一半"));
    assert!(replied.replied_at.is_some());
    assert_eq!(replied.failure_reason, None);

    let kinds: Vec<_> = events(&f, &team.id).iter().map(|e| e.kind).collect();
    assert!(kinds.contains(&EventKind::MessageSent));
    assert!(kinds.contains(&EventKind::MessageReplied));
}

/// 投递失败记下原因；失败的消息可以重发，重发后回到「排队」。
#[test]
fn failed_messages_keep_the_reason_and_can_be_retried() {
    let f = fixture();
    let (team, fe, _) = setup(&f);
    let message = send(&f, &team.id, &fe.id, "进度如何");

    let failed = message_svc::mark_failed(&f.ts.lock(), &message.id, "岗位未绑定 Harness").unwrap();
    assert_eq!(failed.status, MessageStatus::Failed);
    assert_eq!(failed.failure_reason.as_deref(), Some("岗位未绑定 Harness"));

    let retried = f.ts.retry_message(&message.id).unwrap();
    assert_eq!(retried.status, MessageStatus::Queued);
    assert_eq!(retried.failure_reason, None);

    // 只有失败或过期的消息可以重发。
    let err = assert_kind(f.ts.retry_message(&message.id), ErrorKind::Conflict);
    assert!(err.contains("重发"), "{err}");
}

/// 终态判定：已回复、失败、过期都是终态。
#[test]
fn terminal_statuses_are_recognised() {
    assert!(MessageStatus::Replied.is_terminal());
    assert!(MessageStatus::Failed.is_terminal());
    assert!(MessageStatus::Expired.is_terminal());
    assert!(!MessageStatus::Queued.is_terminal());
    assert!(!MessageStatus::Delivering.is_terminal());
}

// ---------------------------------------------------------------- 线程

/// 同一岗位、同一关联任务的往来进同一个线程；换了岗位或任务就换一个线程。
#[test]
fn messages_are_grouped_into_threads_by_role_and_task() {
    let f = fixture();
    let (team, fe, task) = setup(&f);
    let be = f.role(&team.id, "role_backend");

    let first = send(&f, &team.id, &fe.id, "第一条");
    let second = send(&f, &team.id, &fe.id, "第二条");
    let with_task =
        f.ts.send_message(SendMessageInput {
            team_id: team.id.clone(),
            to_role_id: fe.id.clone(),
            body: "跟任务有关".into(),
            task_id: Some(task.task.id.clone()),
            thread_id: None,
            subject: None,
        })
        .unwrap();
    let other_role = send(&f, &team.id, &be.id, "给后端");

    assert_eq!(first.thread_id, second.thread_id);
    assert_ne!(first.thread_id, with_task.thread_id);
    assert_ne!(first.thread_id, other_role.thread_id);

    // 线程列表按团队、任务与岗位筛选。
    let all =
        f.ts.list_threads(ListThreadsQuery {
            team_id: team.id.clone(),
            task_id: None,
            role_id: None,
        })
        .unwrap();
    assert_eq!(all.len(), 3);

    let by_role =
        f.ts.list_threads(ListThreadsQuery {
            team_id: team.id.clone(),
            task_id: None,
            role_id: Some(be.id.clone()),
        })
        .unwrap();
    assert_eq!(by_role.len(), 1);
    assert_eq!(by_role[0].id, other_role.thread_id);

    let by_task =
        f.ts.list_threads(ListThreadsQuery {
            team_id: team.id.clone(),
            task_id: Some(task.task.id.clone()),
            role_id: None,
        })
        .unwrap();
    assert_eq!(by_task.len(), 1);
    assert_eq!(by_task[0].id, with_task.thread_id);
}

/// 线程详情里的消息按时间正序。
#[test]
fn thread_detail_lists_messages_in_order() {
    let f = fixture();
    let (team, fe, _) = setup(&f);
    let first = send(&f, &team.id, &fe.id, "第一条");
    let second = send(&f, &team.id, &fe.id, "第二条");

    let detail = f.ts.get_thread(&first.thread_id).unwrap();
    let ids: Vec<_> = detail.messages.iter().map(|m| m.id.clone()).collect();
    assert_eq!(ids, [first.id, second.id]);
}

// ---------------------------------------------------------------- 排队

/// 同一岗位同时只投递一条：目标岗位正在投递时，它的消息不再出队。
#[test]
fn the_queue_serves_one_message_per_role_at_a_time() {
    let f = fixture();
    let (team, fe, _) = setup(&f);
    let be = f.role(&team.id, "role_backend");
    let first = send(&f, &team.id, &fe.id, "给前端第一条");
    let second = send(&f, &team.id, &fe.id, "给前端第二条");
    let other = send(&f, &team.id, &be.id, "给后端");

    // 前端空闲：先出前端最早的一条。
    let next = message_svc::next_queued(&f.ts.lock(), &[])
        .unwrap()
        .unwrap();
    assert_eq!(next.id, first.id);

    // 前端正在投递：它的消息都让着，后端的可以走。
    let busy = [fe.id.clone()];
    let next = message_svc::next_queued(&f.ts.lock(), &busy)
        .unwrap()
        .unwrap();
    assert_eq!(next.id, other.id);

    // 两个岗位都在投递：队列里没有可出队的消息。
    let busy = [fe.id.clone(), be.id.clone()];
    let next = message_svc::next_queued(&f.ts.lock(), &busy).unwrap();
    assert!(next.is_none());
    assert_ne!(second.id, other.id);
}

// ---------------------------------------------------------------- 过期

/// 超过 7 天仍未回复的消息转「过期」，并留下活动。
#[test]
fn stale_messages_expire_after_seven_days() {
    let f = fixture();
    let (team, fe, _) = setup(&f);
    let message = send(&f, &team.id, &fe.id, "很久以前的消息");
    // 把创建时间挪到 8 天前。
    let old = (time::OffsetDateTime::now_utc() - time::Duration::days(8))
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap();
    f.raw_sql(&format!(
        "UPDATE messages SET created_at = '{old}' WHERE id = '{}'",
        message.id
    ));

    // 过期检查的期限是 7 天前：8 天前的消息已经过期。
    let cutoff = (time::OffsetDateTime::now_utc() - time::Duration::days(7))
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap();
    let expired = message_svc::expire_stale(&f.ts.lock(), &cutoff).unwrap();
    assert_eq!(expired.len(), 1);
    assert_eq!(expired[0].0.id, message.id);
    assert_eq!(expired[0].0.status, MessageStatus::Expired);
    // 原来的状态也带回来（推送时告诉前端状态是从哪儿变过来的）。
    assert_eq!(expired[0].1, MessageStatus::Queued);

    let kinds: Vec<_> = events(&f, &team.id).iter().map(|e| e.kind).collect();
    assert!(kinds.contains(&EventKind::MessageExpired));
}

/// 没到期��消息不受过期检查影响。
#[test]
fn fresh_messages_survive_the_expiry_check() {
    let f = fixture();
    let (team, fe, _) = setup(&f);
    let message = send(&f, &team.id, &fe.id, "刚发的消息");
    let cutoff = (time::OffsetDateTime::now_utc() - time::Duration::days(7))
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap();

    let expired = message_svc::expire_stale(&f.ts.lock(), &cutoff).unwrap();
    assert!(expired.is_empty());
    let again = f.ts.get_thread(&message.thread_id).unwrap();
    assert_eq!(again.messages[0].status, MessageStatus::Queued);
}

// ---------------------------------------------------------------- 重启恢复

/// 应用重启时未完成的投递回到「排队」。
#[test]
fn delivering_messages_return_to_queued_after_restart() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("teamspace.db");
    let ws = dir.path().join("ws");
    std::fs::create_dir_all(&ws).unwrap();
    let config = HostConfig::default();
    let message_id = {
        let ts =
            Teamspace::open_without_delivery(&db, Box::new(FakeProbe), config.clone()).unwrap();
        let team = ts
            .create_team(CreateTeamInput {
                name: "T".into(),
                description: String::new(),
                workspace_path: ws.to_string_lossy().into_owned(),
                roles: templates(),
            })
            .unwrap();
        let fe = ts
            .list_roles(&team.id)
            .unwrap()
            .into_iter()
            .map(|v| v.role)
            .find(|r| r.code == "role_frontend")
            .unwrap();
        let message = ts
            .send_message(SendMessageInput {
                team_id: team.id.clone(),
                to_role_id: fe.id.clone(),
                body: "重启前发的".into(),
                task_id: None,
                thread_id: None,
                subject: None,
            })
            .unwrap();
        message_svc::mark_delivering(&ts.lock(), &message.id).unwrap();
        message.id
    };

    // 重新打开：残留的「投递中」回到「排队」。
    let ts = Teamspace::open_without_delivery(&db, Box::new(FakeProbe), config).unwrap();
    let conn = ts.lock();
    let status: String = conn
        .query_row(
            "SELECT status FROM messages WHERE id = ?1",
            rusqlite::params![message_id],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(status, "queued");
}

// ---------------------------------------------------------------- Leader 边界

/// Leader 可以发消息、可以看线程（D8：消息由 Leader 发起属正常编排）。
#[test]
fn leader_can_send_messages_and_read_threads() {
    let f = fixture();
    let (team, fe, _) = setup(&f);

    let sent = dispatch(
        &f.ts,
        Caller::Leader,
        "send_message",
        &json!({ "input": {
            "teamId": team.id,
            "toRoleId": fe.id,
            "body": "Leader 发来的消息",
        } }),
    )
    .unwrap();
    assert_eq!(sent["sender"], "leader");
    assert_eq!(sent["status"], "queued");

    let thread_id = sent["threadId"].as_str().unwrap().to_owned();
    let threads = dispatch(
        &f.ts,
        Caller::Leader,
        "list_threads",
        &json!({ "input": { "teamId": team.id } }),
    )
    .unwrap();
    assert_eq!(threads.as_array().unwrap().len(), 1);

    let detail = dispatch(
        &f.ts,
        Caller::Leader,
        "get_thread",
        &json!({ "threadId": thread_id }),
    )
    .unwrap();
    assert_eq!(detail["messages"].as_array().unwrap().len(), 1);
}

/// Leader 不能重发消息：重发是 Human 的操作。
#[test]
fn leader_cannot_retry_messages() {
    let f = fixture();
    let (team, fe, _) = setup(&f);
    let message = send(&f, &team.id, &fe.id, "先发一条");
    message_svc::mark_failed(&f.ts.lock(), &message.id, "投递失败").unwrap();

    let err = dispatch(
        &f.ts,
        Caller::Leader,
        "retry_message",
        &json!({ "messageId": message.id }),
    )
    .unwrap_err();
    assert_eq!(err.kind, ErrorKind::Conflict);
    assert!(
        err.message.contains("Leader 无权执行此操作"),
        "{}",
        err.message
    );

    let denied = events(&f, &team.id)
        .into_iter()
        .find(|e| e.kind == EventKind::LeaderDenied)
        .expect("越权操作应留下活动");
    assert_eq!(denied.actor, Actor::Leader);
    assert_eq!(denied.data["command"], "retry_message");
}

/// Leader 发给不存在的岗位时报错，不是静默成功。
#[test]
fn leader_cannot_send_to_a_role_that_does_not_exist() {
    let f = fixture();
    let (team, _, _) = setup(&f);

    let err = dispatch(
        &f.ts,
        Caller::Leader,
        "send_message",
        &json!({ "input": {
            "teamId": team.id,
            "toRoleId": "not-a-role",
            "body": "发给不存在的岗位",
        } }),
    )
    .unwrap_err();
    assert_eq!(err.kind, ErrorKind::NotFound);
    assert!(err.message.contains("岗位"), "{}", err.message);
}

/// 发给别的团队的岗位会被拒绝。
#[test]
fn sending_to_a_role_in_another_team_is_rejected() {
    let f = fixture();
    let (team, _, _) = setup(&f);
    let other = f.team("另一个团队");
    let other_role = f.role(&other.id, "role_frontend");

    let err = assert_kind(
        f.ts.send_message(SendMessageInput {
            team_id: team.id.clone(),
            to_role_id: other_role.id.clone(),
            body: "跨团队".into(),
            task_id: None,
            thread_id: None,
            subject: None,
        }),
        ErrorKind::Invalid,
    );
    assert!(err.contains("不在该团队"), "{err}");
}

/// 正文为空会被拒绝。
#[test]
fn empty_message_body_is_rejected() {
    let f = fixture();
    let (team, fe, _) = setup(&f);
    let err = assert_kind(
        f.ts.send_message(SendMessageInput {
            team_id: team.id.clone(),
            to_role_id: fe.id.clone(),
            body: "   ".into(),
            task_id: None,
            thread_id: None,
            subject: None,
        }),
        ErrorKind::Invalid,
    );
    assert!(err.contains("消息正文"), "{err}");
}

/// 关联别的团队的任务会被拒绝。
#[test]
fn linking_a_task_from_another_team_is_rejected() {
    let f = fixture();
    let (team, fe, _) = setup(&f);
    let other = f.team("另一个团队");
    let other_role = f.role(&other.id, "role_frontend");
    let other_task = f.task(&other.id, &other_role, "别的任务");

    let err = assert_kind(
        f.ts.send_message(SendMessageInput {
            team_id: team.id.clone(),
            to_role_id: fe.id.clone(),
            body: "关联别的任务".into(),
            task_id: Some(other_task.task.id.clone()),
            thread_id: None,
            subject: None,
        }),
        ErrorKind::Invalid,
    );
    assert!(err.contains("关联任务"), "{err}");
}
