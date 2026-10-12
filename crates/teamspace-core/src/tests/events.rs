use super::*;

fn page(f: &Fixture, team_id: &str, cursor: Option<String>, limit: Option<u32>) -> EventPage {
    f.ts.list_events(EventQuery {
        team_id: team_id.into(),
        cursor,
        limit,
    })
    .unwrap()
}

#[test]
fn every_change_writes_an_event_with_chinese_summary() {
    let f = fixture();
    let team = f.team("T");
    let fe = f.role(&team.id, "role_frontend");
    let h = f.harness(&team.id, "frontend-local", HarnessKind::Mock);
    f.ts.bind_role_harness(&fe.id, &h.id).unwrap();
    let t = f.readyable(&team.id, &fe, "登录页");
    f.move_to(&t.task.id, TaskStatus::Ready).unwrap();

    let events = page(&f, &team.id, None, None).events;
    let rows: Vec<_> = events
        .iter()
        .map(|e| (e.kind, e.summary.as_str()))
        .collect();
    assert_eq!(
        rows,
        [
            (EventKind::TaskStatusChanged, "FE-001 状态 TODO → READY"),
            (
                EventKind::TaskCreated,
                "创建任务 FE-001「登录页」，负责岗位「前端开发」"
            ),
            (
                EventKind::RoleHarnessBound,
                "岗位「前端开发」绑定 Harness「frontend-local」"
            ),
            (EventKind::HarnessCreated, "注册 Harness「frontend-local」"),
            (EventKind::TeamCreated, "创建团队「T」，含 5 个岗位"),
        ]
    );
    assert!(
        events
            .iter()
            .all(|e| e.actor == Actor::Human && e.team_id == team.id)
    );
    assert_eq!(events[0].task_id.as_deref(), Some(t.task.id.as_str()));
    assert_eq!(events[0].role_id.as_deref(), Some(fe.id.as_str()));
    assert_eq!(events[0].data["from"], "todo");
    assert_eq!(events[0].data["to"], "ready");
    assert_eq!(events[2].role_id.as_deref(), Some(fe.id.as_str()));
}

#[test]
fn failed_commands_write_no_event() {
    let f = fixture();
    let team = f.team("T");
    let fe = f.role(&team.id, "role_frontend");
    let t = f.task(&team.id, &fe, "a");
    let before = page(&f, &team.id, None, None).events.len();
    assert!(f.move_to(&t.task.id, TaskStatus::Done).is_err());
    assert!(f.ts.archive_role(&fe.id).is_err());
    assert_eq!(page(&f, &team.id, None, None).events.len(), before);
}

#[test]
fn events_are_paginated_newest_first_with_cursor() {
    let f = fixture();
    let team = f.team("T");
    for i in 0..6 {
        f.harness(&team.id, &format!("h{i}"), HarnessKind::Mock);
    }
    // 共 7 条：1 条建团队 + 6 条注册 Harness。
    let all = page(&f, &team.id, None, None);
    assert_eq!(all.events.len(), 7);
    assert!(all.next_cursor.is_none());

    let mut seen = Vec::new();
    let mut cursor = None;
    loop {
        let p = page(&f, &team.id, cursor, Some(3));
        assert!(p.events.len() <= 3);
        seen.extend(p.events.iter().map(|e| e.id.clone()));
        match p.next_cursor {
            Some(next) => cursor = Some(next),
            None => break,
        }
    }
    let expected: Vec<_> = all.events.iter().map(|e| e.id.clone()).collect();
    assert_eq!(seen, expected);
    assert!(
        all.events
            .windows(2)
            .all(|w| w[0].created_at >= w[1].created_at)
    );

    // 恰好整页时没有下一页。
    let p = page(&f, &team.id, None, Some(7));
    assert!(p.next_cursor.is_none());
    // limit 被限制在 1..=200。
    assert_eq!(page(&f, &team.id, None, Some(0)).events.len(), 1);
    assert_eq!(page(&f, &team.id, None, Some(10_000)).events.len(), 7);
}

#[test]
fn events_are_scoped_to_team() {
    let f = fixture();
    let a = f.team("A");
    let b = f.team("B");
    f.harness(&b.id, "h", HarnessKind::Mock);
    assert_eq!(page(&f, &a.id, None, None).events.len(), 1);
    assert_eq!(page(&f, &b.id, None, None).events.len(), 2);
    assert_kind(
        f.ts.list_events(EventQuery {
            team_id: "nope".into(),
            cursor: None,
            limit: None,
        }),
        ErrorKind::NotFound,
    );
}
