use super::*;

fn page(f: &Fixture, team_id: &str, cursor: Option<String>, limit: Option<u32>) -> EventPage {
    f.ts.list_events(EventQuery {
        team_id: team_id.into(),
        cursor,
        limit,
        ..Default::default()
    })
    .unwrap()
}

/// 一次查询的基准条件：单页取回全部，筛选条件由各用例按需覆盖。
fn base(team_id: &str) -> EventQuery {
    EventQuery {
        team_id: team_id.into(),
        limit: Some(200),
        ..Default::default()
    }
}

/// 执行筛选查询并取出活动。
fn filter(f: &Fixture, query: EventQuery) -> Vec<Event> {
    f.ts.list_events(query).unwrap().events
}

/// 查询失败时返回错误说明。
fn filter_err(f: &Fixture, query: EventQuery) -> String {
    match f.ts.list_events(query) {
        Ok(page) => panic!("expected an error, got {} events", page.events.len()),
        Err(err) => err.to_string(),
    }
}

#[test]
fn every_change_writes_an_event_with_chinese_summary() {
    let f = fixture();
    let team = f.team("T");
    let fe = f.role(&team.id, "role_frontend");
    let h = f.harness(&team.id, "frontend-local", AcpPreset::Mock);
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
        f.harness(&team.id, &format!("h{i}"), AcpPreset::Mock);
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
    f.harness(&b.id, "h", AcpPreset::Mock);
    assert_eq!(page(&f, &a.id, None, None).events.len(), 1);
    assert_eq!(page(&f, &b.id, None, None).events.len(), 2);
    assert_kind(
        f.ts.list_events(EventQuery {
            team_id: "nope".into(),
            cursor: None,
            limit: None,
            ..Default::default()
        }),
        ErrorKind::NotFound,
    );
}

#[test]
fn invalid_cursor_is_rejected() {
    let f = fixture();
    let team = f.team("T");
    assert_kind(
        f.ts.list_events(EventQuery {
            team_id: team.id,
            cursor: Some("not-a-cursor".into()),
            limit: None,
            ..Default::default()
        }),
        ErrorKind::Invalid,
    );
}

#[test]
fn events_keep_write_order_within_the_same_millisecond() {
    let f = fixture();
    let team = f.team("T");
    for i in 0..30 {
        f.harness(&team.id, &format!("h{i:02}"), AcpPreset::Mock);
    }
    let summaries: Vec<_> = page(&f, &team.id, None, Some(30))
        .events
        .into_iter()
        .map(|e| e.summary)
        .collect();
    let expected: Vec<_> = (0..30)
        .rev()
        .map(|i| format!("注册 Harness「h{i:02}」"))
        .collect();
    assert_eq!(summaries, expected);
}

#[test]
fn filters_by_task_and_by_role() {
    let f = fixture();
    let team = f.team("T");
    let fe = f.role(&team.id, "role_frontend");
    let be = f.role(&team.id, "role_backend");
    let a = f.readyable(&team.id, &fe, "登录页");
    f.move_to(&a.task.id, TaskStatus::Ready).unwrap();
    let b = f.readyable(&team.id, &be, "登录接口");

    // 按任务：只有该任务的「创建」和「状态变化」。
    let by_task = filter(
        &f,
        EventQuery {
            task_id: Some(a.task.id.clone()),
            ..base(&team.id)
        },
    );
    assert_eq!(by_task.len(), 2);
    assert!(
        by_task
            .iter()
            .all(|e| e.task_id.as_deref() == Some(a.task.id.as_str()))
    );
    let kinds: Vec<_> = by_task.iter().map(|e| e.kind).collect();
    assert!(
        kinds.contains(&EventKind::TaskCreated) && kinds.contains(&EventKind::TaskStatusChanged),
        "{kinds:?}"
    );
    // 与「角色 + 种类」取交集得到同一条：筛选之间是「且」。
    assert_eq!(
        filter(
            &f,
            EventQuery {
                task_id: Some(a.task.id.clone()),
                role_id: Some(fe.id.clone()),
                kinds: Some(vec![EventKind::TaskStatusChanged]),
                ..base(&team.id)
            },
        )
        .len(),
        1
    );

    // 按岗位：岗位自身的创建记录，加上归属它的任务记录。
    let by_role = filter(
        &f,
        EventQuery {
            role_id: Some(fe.id.clone()),
            ..base(&team.id)
        },
    );
    // 建团队只写一条团队级活动，不按岗位拆开；这里因此只有任务 A 的两条。
    assert_eq!(by_role.len(), 2, "任务 A 的创建与状态变化");
    assert!(
        by_role
            .iter()
            .all(|e| e.role_id.as_deref() == Some(fe.id.as_str()))
    );
    let kinds: Vec<_> = by_role.iter().map(|e| e.kind).collect();
    assert_eq!(
        kinds,
        [EventKind::TaskStatusChanged, EventKind::TaskCreated],
        "岗位筛选只命中归属该岗位的任务活动（时间倒序）"
    );
    // 两个岗位互不相交，且各自的条数加上团队级记录等于总数。
    let by_backend = filter(
        &f,
        EventQuery {
            role_id: Some(be.id.clone()),
            ..base(&team.id)
        },
    );
    assert_eq!(by_backend.len(), 1, "只有任务 B 的创建");
    let with_role = filter(
        &f,
        EventQuery {
            task_id: Some(b.task.id.clone()),
            role_id: Some(fe.id.clone()),
            ..base(&team.id)
        },
    );
    assert!(
        with_role.is_empty(),
        "任务属于后端岗位，按前端岗位筛选应为空"
    );
}

#[test]
fn filters_by_kind() {
    let f = fixture();
    let team = f.team("T");
    let fe = f.role(&team.id, "role_frontend");
    f.task(&team.id, &fe, "任务一");
    f.task(&team.id, &fe, "任务二");

    let created = filter(
        &f,
        EventQuery {
            kinds: Some(vec![EventKind::TaskCreated]),
            ..base(&team.id)
        },
    );
    assert_eq!(created.len(), 2);
    assert!(created.iter().all(|e| e.kind == EventKind::TaskCreated));

    // 多个种类之间是「或」：两个任务创建加上建团队那条。
    let two_kinds = filter(
        &f,
        EventQuery {
            kinds: Some(vec![EventKind::TaskCreated, EventKind::TeamCreated]),
            ..base(&team.id)
        },
    );
    assert_eq!(two_kinds.len(), 2 + 1, "两个任务创建 + 建团队");
    assert!(
        two_kinds.iter().any(|e| e.kind == EventKind::TeamCreated)
            && two_kinds
                .iter()
                .all(|e| matches!(e.kind, EventKind::TaskCreated | EventKind::TeamCreated))
    );

    let all = page(&f, &team.id, None, None);
    // 空数组与 null 一样表示不限种类。
    assert_eq!(
        filter(
            &f,
            EventQuery {
                kinds: Some(vec![]),
                ..base(&team.id)
            },
        ),
        all.events
    );
}

#[test]
fn filters_by_time_range_are_inclusive_at_since_and_exclusive_at_until() {
    let f = fixture();
    let team = f.team("T");
    let fe = f.role(&team.id, "role_frontend");
    let old_task = f.readyable(&team.id, &fe, "旧任务");
    // 把已写入的记录固定到早的一批时间，再造一条更晚的；断言不依赖运行时的真实时钟。
    f.raw_sql("UPDATE events SET created_at = '2026-03-01T00:00:00.000Z'");
    let new_task = f.readyable(&team.id, &fe, "新任务");
    f.raw_sql(&format!(
        "UPDATE events SET created_at = '2026-03-08T12:00:00.000Z' WHERE task_id = '{}'",
        new_task.task.id
    ));

    let ids_of = |query: EventQuery| -> Vec<String> {
        filter(&f, query).iter().map(|e| e.id.clone()).collect()
    };
    let of_task = |task_id: &str| {
        ids_of(EventQuery {
            task_id: Some(task_id.to_owned()),
            ..base(&team.id)
        })
    };
    let range = |since: Option<&str>, until: Option<&str>| {
        ids_of(EventQuery {
            since: since.map(str::to_owned),
            until: until.map(str::to_owned),
            ..base(&team.id)
        })
    };

    let late = of_task(&new_task.task.id);
    let old_task_ids = of_task(&old_task.task.id);
    let all = ids_of(base(&team.id));
    // 建团队那条不挂在任何任务上，因此「较早的一批」按集合差取，而不是按任务取。
    let early: Vec<String> = all
        .iter()
        .filter(|id| !late.contains(id))
        .cloned()
        .collect();
    assert!(
        !early.is_empty() && !late.is_empty(),
        "两批都要有记录才有意义"
    );
    assert!(
        old_task_ids.iter().all(|id| early.contains(id)),
        "旧任务的活动应落在较早的那一批里"
    );

    // since 含边界：起点恰好等于较早那批的时刻，它们必须被算进来。
    assert_eq!(range(Some("2026-03-01T00:00:00.000Z"), None), all);
    // until 不含边界：终点恰好等于较晚那批的时刻，那批被排除，只剩较早那批。
    assert_eq!(range(None, Some("2026-03-08T12:00:00.000Z")), early);
    // 同一时刻作为起点则相反：含边界，较晚那批被算进来。
    assert_eq!(range(Some("2026-03-08T12:00:00.000Z"), None), late);
    // 终点早于两批时为空。
    assert!(range(None, Some("2026-03-01T00:00:00.000Z")).is_empty());
    // 开区间取中间：起点含、终点不含。
    assert_eq!(
        range(
            Some("2026-03-01T00:00:00.000Z"),
            Some("2026-03-08T12:00:00.000Z")
        ),
        early
    );
    // 区间落在两批之间时为空。
    assert!(range(Some("2026-03-02T00:00:00Z"), Some("2026-03-05T00:00:00Z")).is_empty());
    // 不给范围就是全部，且顺序与其他查询一致。
    assert_eq!(range(None, None), all);
}
#[test]
fn time_filter_accepts_rfc3339_forms_that_name_the_same_instant() {
    let f = fixture();
    let team = f.team("T");
    f.role(&team.id, "role_frontend");
    f.raw_sql("UPDATE events SET created_at = '2026-03-01T00:00:00.000Z'");
    let at = |since: &str| {
        filter(
            &f,
            EventQuery {
                since: Some(since.into()),
                ..base(&team.id)
            },
        )
        .len()
    };
    // 同一时刻的三种写法都要命中；缺小数位或带时区偏移时若按原始字符串比较就会漏掉这一批。
    // 建团队只写一条活动，这里固定成 1 让边界断言一目了然。
    assert_eq!(page(&f, &team.id, None, None).events.len(), 1);
    assert_eq!(at("2026-03-01T00:00:00.000Z"), 1);
    assert_eq!(at("2026-03-01T00:00:00Z"), 1);
    assert_eq!(at("2026-03-01T08:00:00+08:00"), 1);
    assert_eq!(at("2026-02-28T19:00:00-05:00"), 1);
    assert_eq!(at("2026-03-01T00:00:00.001Z"), 0);
}

#[test]
fn malformed_time_filters_are_rejected_with_a_chinese_message() {
    let f = fixture();
    let team = f.team("T");
    let cases = [
        ("2026-03-01", "since"),
        ("2026-03-01 00:00:00", "since"),
        ("2026-13-01T00:00:00Z", "since"),
        ("", "since"),
        ("昨天", "since"),
        ("2026-03-01", "until"),
        ("not-a-time", "until"),
    ];
    for (raw, field) in cases {
        let since = if field == "since" { Some(raw) } else { None };
        let until = if field == "until" { Some(raw) } else { None };
        let err = filter_err(
            &f,
            EventQuery {
                since: since.map(str::to_owned),
                until: until.map(str::to_owned),
                ..base(&team.id)
            },
        );
        assert!(err.contains(field), "{field} 应在说明里出现: {err}");
        assert!(err.contains("RFC3339"), "{err}");
        assert!(err.contains(raw), "说明里应带上原始输入: {err}");
    }
    // 时间不合法优先于其他筛选条件被报告。
    assert!(
        filter_err(
            &f,
            EventQuery {
                task_id: Some("whatever".into()),
                since: Some("2026-03-01".into()),
                ..base(&team.id)
            },
        )
        .contains("since")
    );
}

#[test]
fn filters_of_another_team_or_of_unknown_ids_return_empty_pages() {
    let f = fixture();
    let a = f.team("A");
    let b = f.team("B");
    let fe_b = f.role(&b.id, "role_frontend");
    let task_b = f.task(&b.id, &fe_b, "B 的任务");

    for query in [
        EventQuery {
            task_id: Some(task_b.task.id.clone()),
            ..base(&a.id)
        },
        EventQuery {
            role_id: Some(fe_b.id.clone()),
            ..base(&a.id)
        },
        EventQuery {
            task_id: Some("task-does-not-exist".into()),
            ..base(&a.id)
        },
        EventQuery {
            role_id: Some("role-does-not-exist".into()),
            ..base(&a.id)
        },
        EventQuery {
            task_id: Some(task_b.task.id.clone()),
            role_id: Some(fe_b.id.clone()),
            ..base(&a.id)
        },
    ] {
        let page = f.ts.list_events(query).unwrap();
        assert!(page.events.is_empty());
        assert!(page.next_cursor.is_none());
    }
}

#[test]
fn filters_survive_cursor_paging_without_gaps_or_duplicates() {
    let f = fixture();
    let team = f.team("T");
    let fe = f.role(&team.id, "role_frontend");
    let be = f.role(&team.id, "role_backend");
    for i in 0..5 {
        f.task(&team.id, &fe, &format!("前端任务{i}"));
    }
    for i in 0..3 {
        f.task(&team.id, &be, &format!("后端任务{i}"));
    }

    // 只看前端岗位且只看任务创建，预期 5 条（岗位创建那条被 kinds 排除）。
    let filtered = EventQuery {
        role_id: Some(fe.id.clone()),
        kinds: Some(vec![EventKind::TaskCreated]),
        limit: Some(200),
        ..base(&team.id)
    };
    let all = filter(&f, filtered.clone());
    assert_eq!(all.len(), 5);

    let mut seen: Vec<String> = Vec::new();
    let mut cursor: Option<String> = None;
    let mut pages = 0;
    loop {
        let p =
            f.ts.list_events(EventQuery {
                cursor: cursor.clone(),
                limit: Some(2),
                ..filtered.clone()
            })
            .unwrap();
        pages += 1;
        assert!(p.events.len() <= 2);
        assert!(
            p.events
                .iter()
                .all(|e| e.role_id.as_deref() == Some(fe.id.as_str())
                    && e.kind == EventKind::TaskCreated),
            "翻页后筛选条件不能丢"
        );
        seen.extend(p.events.iter().map(|e| e.id.clone()));
        match p.next_cursor {
            Some(next) => cursor = Some(next),
            None => break,
        }
    }
    assert_eq!(pages, 3, "5 条按每页 2 条应为 3 页");
    assert_eq!(seen, all.iter().map(|e| e.id.clone()).collect::<Vec<_>>());
    let mut unique = seen.clone();
    unique.sort();
    unique.dedup();
    assert_eq!(unique.len(), 5, "跨页不能重复");
    // 翻页顺序与单页一致（按时间倒序）。
    assert!(all.windows(2).all(|w| w[0].created_at >= w[1].created_at));
    // 同一组筛选从头再查一遍，结果稳定（游标只推进、不改筛选）。
    assert_eq!(
        seen,
        filter(&f, filtered.clone())
            .iter()
            .map(|e| e.id.clone())
            .collect::<Vec<_>>()
    );
}
