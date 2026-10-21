//! 应用内通知：订阅设置（默认全开、按类别与岗位开关）、通知推送与消息状态事件推送。

use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use super::*;

/// 团队与前端岗位。
fn setup(f: &Fixture) -> (Team, Role) {
    let team = f.team("T");
    let fe = f.role(&team.id, "role_frontend");
    (team, fe)
}

/// 注册推送接收者，收集推来的事件。
fn recorder(f: &Fixture) -> Arc<Mutex<Vec<NotifyEvent>>> {
    let events: Arc<Mutex<Vec<NotifyEvent>>> = Arc::default();
    let sink = Arc::clone(&events);
    f.ts.set_notify_listener(move |event| {
        sink.lock()
            .unwrap_or_else(|p| p.into_inner())
            .push(event.clone());
    });
    events
}

/// 等到出现满足条件的事件（最多 10 秒）。
fn wait_for<F>(events: &Arc<Mutex<Vec<NotifyEvent>>>, mut predicate: F) -> Option<NotifyEvent>
where
    F: FnMut(&NotifyEvent) -> bool,
{
    let deadline = Instant::now() + Duration::from_secs(10);
    while Instant::now() < deadline {
        let found = events
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .iter()
            .find(|event| predicate(event))
            .cloned();
        if found.is_some() {
            return found;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    None
}

/// 某类别的通知是否推过。
fn pushed_category(events: &Arc<Mutex<Vec<NotifyEvent>>>, category: NotificationCategory) -> bool {
    events
        .lock()
        .unwrap_or_else(|p| p.into_inner())
        .iter()
        .any(|event| {
            matches!(event, NotifyEvent::Notification { notification } if notification.category == category)
        })
}

/// 读取订阅设置。
fn settings(f: &Fixture, team_id: &str) -> NotificationSettings {
    f.ts.get_notification_settings(GetNotificationSettingsInput {
        team_id: team_id.into(),
    })
    .unwrap()
}

// ---------------------------------------------------------------- 订阅设置

/// 默认全部开启：每个类别、每个岗位都是开的。
#[test]
fn notification_settings_default_to_everything_on() {
    let f = plain_fixture();
    let (team, _) = setup(&f);
    let settings = settings(&f, &team.id);
    assert_eq!(settings.categories.len(), NotificationCategory::ALL.len());
    for category in &settings.categories {
        assert!(category.enabled, "{category:?}");
        assert!(!category.category_label.is_empty());
        for role in &category.roles {
            assert!(role.enabled, "{category:?} {role:?}");
        }
    }
    // 岗位开关列出了团队内的岗位。
    let codes: Vec<_> = settings.categories[0]
        .roles
        .iter()
        .map(|r| r.role_code.as_str())
        .collect();
    assert!(codes.contains(&"role_frontend"), "{codes:?}");
}

/// 关掉一个类别后，该类别不再推送；设置持久化（重新读取仍是关的）。
#[test]
fn turning_a_category_off_stops_its_notifications() {
    let f = plain_fixture();
    let (team, fe) = setup(&f);
    let events = recorder(&f);
    f.ts.update_notification_settings(UpdateNotificationSettingsInput {
        team_id: team.id.clone(),
        categories: vec![NotificationCategoryUpdate {
            category: NotificationCategory::TaskBlocked,
            enabled: Some(false),
            roles: vec![],
        }],
    })
    .unwrap();

    let task = f.readyable(&team.id, &fe, "登录页");
    f.drive(
        &task.task.id,
        &[TaskStatus::Ready, TaskStatus::Doing, TaskStatus::Blocked],
    );
    // 稍等推送线程（这里其实是同步推送，等一小会儿保险）。
    std::thread::sleep(Duration::from_millis(200));
    assert!(
        !pushed_category(&events, NotificationCategory::TaskBlocked),
        "关掉的类别不该推送"
    );

    // 设置持久化：重新读取仍是关的。
    let settings = settings(&f, &team.id);
    let blocked = settings
        .categories
        .iter()
        .find(|c| c.category == NotificationCategory::TaskBlocked)
        .unwrap();
    assert!(!blocked.enabled);
}

/// 按岗位关掉：别的岗位不受影响。
#[test]
fn turning_off_one_role_keeps_the_others() {
    let f = plain_fixture();
    let (team, fe) = setup(&f);
    let be = f.role(&team.id, "role_backend");
    f.ts.update_notification_settings(UpdateNotificationSettingsInput {
        team_id: team.id.clone(),
        categories: vec![NotificationCategoryUpdate {
            category: NotificationCategory::TaskBlocked,
            enabled: None,
            roles: vec![NotificationRoleUpdate {
                role_id: fe.id.clone(),
                enabled: false,
            }],
        }],
    })
    .unwrap();

    let settings = settings(&f, &team.id);
    let blocked = settings
        .categories
        .iter()
        .find(|c| c.category == NotificationCategory::TaskBlocked)
        .unwrap();
    // 类别总开关没动。
    assert!(blocked.enabled);
    let fe_switch = blocked.roles.iter().find(|r| r.role_id == fe.id).unwrap();
    let be_switch = blocked.roles.iter().find(|r| r.role_id == be.id).unwrap();
    assert!(!fe_switch.enabled, "前端岗位应被关掉");
    assert!(be_switch.enabled, "后端岗位不应受影响");
}

/// 岗位开关按岗位生效：关掉前端岗位后，前端任务受阻不推送；后端任务仍推送。
#[test]
fn role_settings_filter_the_notifications() {
    let f = plain_fixture();
    let (team, fe) = setup(&f);
    let be = f.role(&team.id, "role_backend");
    let events = recorder(&f);
    f.ts.update_notification_settings(UpdateNotificationSettingsInput {
        team_id: team.id.clone(),
        categories: vec![NotificationCategoryUpdate {
            category: NotificationCategory::TaskBlocked,
            enabled: None,
            roles: vec![NotificationRoleUpdate {
                role_id: fe.id.clone(),
                enabled: false,
            }],
        }],
    })
    .unwrap();

    // 前端岗位的任务受阻：不推送。
    let fe_task = f.readyable(&team.id, &fe, "登录页");
    f.drive(
        &fe_task.task.id,
        &[TaskStatus::Ready, TaskStatus::Doing, TaskStatus::Blocked],
    );
    std::thread::sleep(Duration::from_millis(200));
    assert!(!pushed_category(&events, NotificationCategory::TaskBlocked));

    // 后端岗位的任务受阻：照常推送。
    let be_task = f.readyable(&team.id, &be, "接口");
    f.drive(
        &be_task.task.id,
        &[TaskStatus::Ready, TaskStatus::Doing, TaskStatus::Blocked],
    );
    assert!(
        wait_for(&events, |event| {
            matches!(event, NotifyEvent::Notification { notification }
                if notification.category == NotificationCategory::TaskBlocked
                    && notification.role_id.as_deref() == Some(be.id.as_str()))
        })
        .is_some(),
        "后端岗位的受阻通知应推送"
    );
}

/// 类别总开关关着时，岗位开关回显自己的存储值：岗位设置不会被总开关冲掉。
#[test]
fn role_switches_keep_their_own_value_when_the_category_is_off() {
    let f = plain_fixture();
    let (team, fe) = setup(&f);
    let be = f.role(&team.id, "role_backend");

    // 关掉前端岗位。
    f.ts.update_notification_settings(UpdateNotificationSettingsInput {
        team_id: team.id.clone(),
        categories: vec![NotificationCategoryUpdate {
            category: NotificationCategory::TaskBlocked,
            enabled: None,
            roles: vec![NotificationRoleUpdate {
                role_id: fe.id.clone(),
                enabled: false,
            }],
        }],
    })
    .unwrap();

    // 关掉类别总开关。
    f.ts.update_notification_settings(UpdateNotificationSettingsInput {
        team_id: team.id.clone(),
        categories: vec![NotificationCategoryUpdate {
            category: NotificationCategory::TaskBlocked,
            enabled: Some(false),
            roles: vec![],
        }],
    })
    .unwrap();

    // 回显的是岗位自己的存储值：前端岗位仍是关，后端岗位仍是开。
    let current = settings(&f, &team.id);
    let blocked = current
        .categories
        .iter()
        .find(|c| c.category == NotificationCategory::TaskBlocked)
        .unwrap();
    assert!(!blocked.enabled, "类别总开关应为关");
    let fe_switch = blocked.roles.iter().find(|r| r.role_id == fe.id).unwrap();
    let be_switch = blocked.roles.iter().find(|r| r.role_id == be.id).unwrap();
    assert!(!fe_switch.enabled, "前端岗位应保持关");
    assert!(be_switch.enabled, "后端岗位应保持开");

    // 重新打开总开关：岗位设置原样还在。
    f.ts.update_notification_settings(UpdateNotificationSettingsInput {
        team_id: team.id.clone(),
        categories: vec![NotificationCategoryUpdate {
            category: NotificationCategory::TaskBlocked,
            enabled: Some(true),
            roles: vec![],
        }],
    })
    .unwrap();

    let current = settings(&f, &team.id);
    let blocked = current
        .categories
        .iter()
        .find(|c| c.category == NotificationCategory::TaskBlocked)
        .unwrap();
    assert!(blocked.enabled, "类别总开关应为开");
    let fe_switch = blocked.roles.iter().find(|r| r.role_id == fe.id).unwrap();
    let be_switch = blocked.roles.iter().find(|r| r.role_id == be.id).unwrap();
    assert!(!fe_switch.enabled, "前端岗位应仍是关");
    assert!(be_switch.enabled, "后端岗位应仍是开");
}

/// 岗位不属于该团队时拒绝修改。
#[test]
fn settings_reject_roles_from_another_team() {
    let f = plain_fixture();
    let (team, _) = setup(&f);
    let other = f.team("另一个团队");
    let other_role = f.role(&other.id, "role_frontend");
    let err = assert_kind(
        f.ts.update_notification_settings(UpdateNotificationSettingsInput {
            team_id: team.id.clone(),
            categories: vec![NotificationCategoryUpdate {
                category: NotificationCategory::TaskBlocked,
                enabled: None,
                roles: vec![NotificationRoleUpdate {
                    role_id: other_role.id.clone(),
                    enabled: false,
                }],
            }],
        })
        .map(|s| s.team_id),
        ErrorKind::Invalid,
    );
    assert!(err.contains("岗位不在该团队中"), "{err}");
}

// ---------------------------------------------------------------- 通知推送

/// 任务受阻会推送通知，标题与影响说明都带上任务信息。
#[test]
fn blocked_tasks_push_a_notification() {
    let f = plain_fixture();
    let (team, fe) = setup(&f);
    let events = recorder(&f);
    let task = f.readyable(&team.id, &fe, "登录页");
    f.drive(
        &task.task.id,
        &[TaskStatus::Ready, TaskStatus::Doing, TaskStatus::Blocked],
    );

    let event = wait_for(&events, |event| {
        matches!(event, NotifyEvent::Notification { notification }
            if notification.category == NotificationCategory::TaskBlocked)
    })
    .expect("任务受阻应推送通知");
    let NotifyEvent::Notification { notification } = event else {
        unreachable!()
    };
    assert_eq!(notification.team_id, team.id);
    assert_eq!(notification.task_id.as_deref(), Some(task.task.id.as_str()));
    assert_eq!(
        notification.task_key.as_deref(),
        Some(task.task.key.as_str())
    );
    assert_eq!(notification.role_id.as_deref(), Some(fe.id.as_str()));
    assert!(
        notification.title.contains("受阻"),
        "{}",
        notification.title
    );
    assert_eq!(notification.body, "等待接口");
    assert!(!notification.occurred_at.is_empty());
}

// ---------------------------------------------------------------- 消息状态事件

/// 消息状态变化（排队 → 失败）会推送事件：页面不靠轮询也能看到。
///
/// 走真实的投递引擎：岗位没绑定 Harness，消息投递失败。
#[test]
fn message_status_changes_are_pushed_to_the_frontend() {
    let f = plain_fixture();
    let (team, fe) = setup(&f);
    let events = recorder(&f);
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

    // 排队：发送时就推送（没有前一个状态）。
    let queued = wait_for(&events, |event| {
        matches!(event, NotifyEvent::MessageStatus { message: m, previous_status: None }
            if m.id == message.id && m.status == MessageStatus::Queued)
    })
    .expect("排队状态应推送");
    let NotifyEvent::MessageStatus { message: m, .. } = queued else {
        unreachable!()
    };
    assert_eq!(m.id, message.id);

    // 投递引擎接手后失败：状态变化与通知都推。
    let failed = wait_for(&events, |event| {
        matches!(event, NotifyEvent::MessageStatus { message: m, .. }
            if m.id == message.id && m.status == MessageStatus::Failed)
    })
    .expect("失败状态应推送");
    let NotifyEvent::MessageStatus {
        message: m,
        previous_status,
    } = failed
    else {
        unreachable!()
    };
    assert_eq!(previous_status, Some(MessageStatus::Queued));
    assert!(m.failure_reason.is_some());

    // 失败的消息要 Human 处理：对应通知也推了。
    assert!(
        wait_for(&events, |event| {
            matches!(event, NotifyEvent::Notification { notification }
                if notification.category == NotificationCategory::MessageAction
                    && notification.message_id.as_deref() == Some(message.id.as_str()))
        })
        .is_some(),
        "失败消息应推送「消息需要处理」通知"
    );
}

/// 岗位回复后推送新的状态与通知（投递引擎的回复路径）。
#[test]
fn a_reply_pushes_the_new_status_and_a_notification() {
    let f = plain_fixture();
    let (team, fe) = setup(&f);
    let events = recorder(&f);
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

    // 投递引擎把状态转「投递中」再转「已回复」。
    let core = f.ts.core_handle();
    core.update_message_status(MessageStatus::Queued, |tx| {
        crate::service::message::mark_delivering(tx, &message.id)
    })
    .unwrap();
    core.update_message_status(MessageStatus::Delivering, |tx| {
        crate::service::message::mark_replied(tx, &message.id, "已完成一半")
    })
    .unwrap();

    let replied = wait_for(&events, |event| {
        matches!(event, NotifyEvent::MessageStatus { message: m, .. }
            if m.id == message.id && m.status == MessageStatus::Replied)
    })
    .expect("已回复状态应推送");
    let NotifyEvent::MessageStatus { message: m, .. } = replied else {
        unreachable!()
    };
    assert_eq!(m.reply_body.as_deref(), Some("已完成一半"));

    // 消息已回复的通知也推了。
    assert!(
        wait_for(&events, |event| {
            matches!(event, NotifyEvent::Notification { notification }
                if notification.category == NotificationCategory::MessageReplied
                    && notification.message_id.as_deref() == Some(message.id.as_str()))
        })
        .is_some(),
        "应推送「消息已回复」通知"
    );
}
