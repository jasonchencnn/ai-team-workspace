//! 决策记录：写入与列表、恢复受阻任务（走就绪校验）、岗位工作日志追加、Leader 权限边界。

use serde_json::json;

use super::*;
use crate::dispatch::{Caller, dispatch};
use crate::role_folder::JOURNAL_FILE;

/// 团队与前端岗位。
fn setup(f: &Fixture) -> (Team, Role) {
    let team = f.team("T");
    let fe = f.role(&team.id, "role_frontend");
    (team, fe)
}

/// 岗位工作日志里的条目文本（去掉时间与任务编号）。
fn journal(f: &Fixture, rel: &str) -> Vec<String> {
    fs::read_to_string(f.ws.join(rel).join(JOURNAL_FILE))
        .unwrap_or_else(|err| panic!("read journal {rel}: {err}"))
        .lines()
        .filter_map(|l| l.strip_prefix("- "))
        .map(|l| l.split_once(" · ").unwrap().1.to_owned())
        .collect()
}

/// 记一条决策。
fn record(f: &Fixture, input: RecordDecisionInput) -> Result<Decision> {
    f.ts.record_decision(input)
}

// ---------------------------------------------------------------- 写入与列表

/// 决策写入后可按团队列出，最近的在前。
#[test]
fn decisions_are_listed_for_the_team() {
    let f = plain_fixture();
    let (team, fe) = setup(&f);
    let task = f.task(&team.id, &fe, "登录页");

    let decision = record(
        &f,
        RecordDecisionInput {
            team_id: team.id.clone(),
            task_id: Some(task.task.id.clone()),
            message_id: None,
            text: "先做静态页，接口等后端确认".into(),
            resume_task: None,
        },
    )
    .unwrap();
    assert_eq!(decision.team_id, team.id);
    assert_eq!(decision.task_id.as_deref(), Some(task.task.id.as_str()));
    assert_eq!(decision.task_key.as_deref(), Some(task.task.key.as_str()));
    assert_eq!(decision.role_id.as_deref(), Some(fe.id.as_str()));
    assert_eq!(decision.role_name.as_deref(), Some("前端开发"));
    assert_eq!(decision.text, "先做静态页，接口等后端确认");
    assert!(!decision.resumed_task);

    let list =
        f.ts.list_decisions(ListDecisionsQuery {
            team_id: team.id.clone(),
            task_id: None,
            message_id: None,
        })
        .unwrap();
    assert_eq!(list.len(), 1);
    assert_eq!(list[0].id, decision.id);

    // 按任务筛选。
    let filtered =
        f.ts.list_decisions(ListDecisionsQuery {
            team_id: team.id.clone(),
            task_id: Some(task.task.id.clone()),
            message_id: None,
        })
        .unwrap();
    assert_eq!(filtered.len(), 1);
    let none =
        f.ts.list_decisions(ListDecisionsQuery {
            team_id: team.id.clone(),
            task_id: Some("not-a-task".into()),
            message_id: None,
        })
        .unwrap();
    assert!(none.is_empty());
}

/// 决策内容必填；空内容报错。
#[test]
fn decision_text_is_required() {
    let f = plain_fixture();
    let (team, fe) = setup(&f);
    let task = f.task(&team.id, &fe, "登录页");
    let message = assert_kind(
        record(
            &f,
            RecordDecisionInput {
                team_id: team.id.clone(),
                task_id: Some(task.task.id.clone()),
                message_id: None,
                text: "   ".into(),
                resume_task: None,
            },
        )
        .map(|d| d.id),
        ErrorKind::Invalid,
    );
    assert!(message.contains("决策内容"), "{message}");
}

/// 关联对象必须属于本团队。
#[test]
fn decisions_reject_objects_from_another_team() {
    let f = plain_fixture();
    let (team, _) = setup(&f);
    let other = f.team("另一个团队");
    let other_task = f.task(&other.id, &f.role(&other.id, "role_frontend"), "别的任务");

    let message = assert_kind(
        record(
            &f,
            RecordDecisionInput {
                team_id: team.id.clone(),
                task_id: Some(other_task.task.id.clone()),
                message_id: None,
                text: "先这样".into(),
                resume_task: None,
            },
        )
        .map(|d| d.id),
        ErrorKind::Invalid,
    );
    assert!(message.contains("不在该团队中"), "{message}");
}

// ---------------------------------------------------------------- 恢复受阻任务

/// 记录决策时可以把受阻任务恢复到 READY。
#[test]
fn recording_a_decision_resumes_a_blocked_task() {
    let f = plain_fixture();
    let (team, fe) = setup(&f);
    let task = f.readyable(&team.id, &fe, "登录页");
    f.drive(
        &task.task.id,
        &[TaskStatus::Ready, TaskStatus::Doing, TaskStatus::Blocked],
    );

    let decision = record(
        &f,
        RecordDecisionInput {
            team_id: team.id.clone(),
            task_id: Some(task.task.id.clone()),
            message_id: None,
            text: "接口字段先按驼峰来".into(),
            resume_task: Some(true),
        },
    )
    .unwrap();
    assert!(decision.resumed_task);

    let after = f.ts.get_task(&task.task.id).unwrap();
    assert_eq!(after.task.status, TaskStatus::Ready);
    // 离开受阻后原因清空。
    assert_eq!(after.task.blocked_reason, None);
}

/// 恢复任务仍走就绪条件校验：依赖没完成时恢复失败，决策也一起回滚。
#[test]
fn resuming_a_task_still_checks_readiness() {
    let f = plain_fixture();
    let (team, fe) = setup(&f);
    let upstream = f.task(&team.id, &fe, "接口");
    f.drive(
        &upstream.task.id,
        &[
            TaskStatus::Ready,
            TaskStatus::Doing,
            TaskStatus::Review,
            TaskStatus::Done,
        ],
    );
    let task = f
        .try_task(
            &team.id,
            &fe,
            "登录页",
            TaskStatus::Todo,
            vec![],
            vec![upstream.task.id.clone()],
        )
        .unwrap();
    f.drive(
        &task.task.id,
        &[TaskStatus::Ready, TaskStatus::Doing, TaskStatus::Blocked],
    );
    // 依赖回到未完成：就绪条件不再满足。
    f.raw_sql(&format!(
        "UPDATE tasks SET status = 'todo' WHERE id = '{}'",
        upstream.task.id
    ));

    let message = assert_kind(
        record(
            &f,
            RecordDecisionInput {
                team_id: team.id.clone(),
                task_id: Some(task.task.id.clone()),
                message_id: None,
                text: "先做".into(),
                resume_task: Some(true),
            },
        )
        .map(|d| d.id),
        ErrorKind::Conflict,
    );
    assert!(message.contains("尚未就绪"), "{message}");

    // 事务回滚：任务仍是受阻，决策也没有留下。
    let after = f.ts.get_task(&task.task.id).unwrap();
    assert_eq!(after.task.status, TaskStatus::Blocked);
    let list =
        f.ts.list_decisions(ListDecisionsQuery {
            team_id: team.id.clone(),
            task_id: None,
            message_id: None,
        })
        .unwrap();
    assert!(list.is_empty());
}

/// 任务不是「受阻」时，恢复没有意义，直接报错。
#[test]
fn resuming_a_task_that_is_not_blocked_is_rejected() {
    let f = plain_fixture();
    let (team, fe) = setup(&f);
    let task = f.task(&team.id, &fe, "登录页");

    let message = assert_kind(
        record(
            &f,
            RecordDecisionInput {
                team_id: team.id.clone(),
                task_id: Some(task.task.id.clone()),
                message_id: None,
                text: "先这样".into(),
                resume_task: Some(true),
            },
        )
        .map(|d| d.id),
        ErrorKind::Conflict,
    );
    assert!(message.contains("无需恢复"), "{message}");
}

// ---------------------------------------------------------------- 工作日志追加

/// 决策在关联岗位的工作日志里追加一行「决策：…」（D14：只追加）。
#[test]
fn decisions_are_appended_to_the_role_work_log() {
    let f = plain_fixture();
    let (team, fe) = setup(&f);
    let task = f.task(&team.id, &fe, "登录页");
    let rel = "Teamspace/前端开发";
    let before = journal(&f, rel);

    record(
        &f,
        RecordDecisionInput {
            team_id: team.id.clone(),
            task_id: Some(task.task.id.clone()),
            message_id: None,
            text: "先做静态页，接口等后端确认".into(),
            resume_task: None,
        },
    )
    .unwrap();

    let after = journal(&f, rel);
    assert_eq!(after.len(), before.len() + 1);
    let added = after.last().unwrap();
    assert!(added.contains("决策："), "{added}");
    assert!(added.contains("先做静态页，接口等后端确认"), "{added}");
    // 旧条目没有被改写（只追加）。
    assert_eq!(&after[..before.len()], &before[..]);
}

/// 别的岗位不写这条日志：决策只写进关联岗位。
#[test]
fn decisions_only_reach_the_related_role() {
    let f = plain_fixture();
    let (team, _) = setup(&f);
    let be = f.role(&team.id, "role_backend");
    let task = f.task(&team.id, &be, "接口");
    let before_fe = journal(&f, "Teamspace/前端开发");
    let before_be = journal(&f, "Teamspace/后端开发");

    record(
        &f,
        RecordDecisionInput {
            team_id: team.id.clone(),
            task_id: Some(task.task.id.clone()),
            message_id: None,
            text: "接口先只读".into(),
            resume_task: None,
        },
    )
    .unwrap();

    assert_eq!(journal(&f, "Teamspace/前端开发"), before_fe);
    let after_be = journal(&f, "Teamspace/后端开发");
    assert_eq!(after_be.len(), before_be.len() + 1);
    assert!(after_be.last().unwrap().contains("决策：接口先只读"));
}

// ---------------------------------------------------------------- 权限边界

/// Leader 不能记录决策：命令被拒并留下越权活动。
#[test]
fn leader_cannot_record_decisions() {
    let f = plain_fixture();
    let (team, _) = setup(&f);
    let task = f.task(&team.id, &f.role(&team.id, "role_frontend"), "登录页");

    let err = dispatch(
        &f.ts,
        Caller::Leader,
        "record_decision",
        &json!({
            "input": {
                "teamId": team.id,
                "taskId": task.task.id,
                "text": "我来定",
            }
        }),
    )
    .unwrap_err();
    assert_eq!(err.kind, ErrorKind::Conflict);
    assert!(
        err.message.starts_with("Leader 无权执行此操作："),
        "{}",
        err.message
    );
    assert!(err.message.contains("记录决策"), "{}", err.message);

    // 决策没有写进去。
    let list =
        f.ts.list_decisions(ListDecisionsQuery {
            team_id: team.id.clone(),
            task_id: None,
            message_id: None,
        })
        .unwrap();
    assert!(list.is_empty());

    // 越权留痕：活动里有 LeaderDenied。
    let events =
        f.ts.list_events(EventQuery {
            team_id: team.id.clone(),
            limit: Some(100),
            ..Default::default()
        })
        .unwrap();
    assert!(
        events
            .events
            .iter()
            .any(|event| event.kind == EventKind::LeaderDenied)
    );
}
