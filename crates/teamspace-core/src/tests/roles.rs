use super::*;

fn create(f: &Fixture, team_id: &str, role: NewRole) -> Result<RoleView> {
    f.ts.create_role(CreateRoleInput {
        team_id: team_id.into(),
        role,
    })
}

fn update(f: &Fixture, role: &Role, prefix: &str, is_leader: bool) -> Result<RoleView> {
    f.ts.update_role(UpdateRoleInput {
        role_id: role.id.clone(),
        name: role.name.clone(),
        task_prefix: prefix.into(),
        description: role.description.clone(),
        guidelines: role.guidelines.clone(),
        is_leader,
    })
}

#[test]
fn templates_come_from_the_backend() {
    let f = fixture();
    let t = f.ts.list_role_templates();
    let rows: Vec<_> = t
        .iter()
        .map(|t| {
            (
                t.name.as_str(),
                t.code.as_str(),
                t.task_prefix.as_str(),
                t.is_leader,
            )
        })
        .collect();
    assert_eq!(
        rows,
        [
            ("Leader", "role_leader", "LD", true),
            ("产品经理", "role_product", "PM", false),
            ("前端开发", "role_frontend", "FE", false),
            ("后端开发", "role_backend", "BE", false),
            ("QA 测试", "role_qa", "QA", false),
        ]
    );
    assert_eq!(
        t[0].description,
        "拆解、路由、验收；读取团队计划；无生产权限。"
    );
    assert_eq!(t[2].description, "负责前端界面、交互实现及前端验证。");
    assert_eq!(t[4].description, "验收与回归");
}

#[test]
fn role_fields_are_validated() {
    let f = fixture();
    let team = f.team("T");
    assert_kind(
        create(&f, &team.id, new_role(" ", "role_x", "XX", false)),
        ErrorKind::Invalid,
    );
    assert_kind(
        create(&f, &team.id, new_role("X", "Role X", "XX", false)),
        ErrorKind::Invalid,
    );
    assert_kind(
        create(&f, &team.id, new_role("X", "role_x", "X", false)),
        ErrorKind::Invalid,
    );
    assert_kind(
        create(&f, &team.id, new_role("X", "role_x", "xx", false)),
        ErrorKind::Invalid,
    );
    let ok = create(
        &f,
        &team.id,
        new_role(" 设计 ", "role_design", "DSGN", false),
    )
    .unwrap();
    assert_eq!(ok.role.name, "设计");
    assert_kind(
        create(&f, "nope", new_role("X", "role_x", "XX", false)),
        ErrorKind::NotFound,
    );
}

#[test]
fn leader_is_unique_per_team() {
    let f = fixture();
    let team = f.team("T");
    let msg = assert_kind(
        create(&f, &team.id, new_role("L2", "role_l2", "LL", true)),
        ErrorKind::Conflict,
    );
    assert!(msg.contains("Leader"), "{msg}");
    let qa = f.role(&team.id, "role_qa");
    assert_kind(update(&f, &qa, "QA", true), ErrorKind::Conflict);

    // 原 Leader 取消后可以指定新的 Leader；编辑 Leader 自身不冲突。
    let leader = f.role(&team.id, "role_leader");
    update(&f, &leader, "LD", true).unwrap();
    update(&f, &leader, "LD", false).unwrap();
    assert!(update(&f, &qa, "QA", true).unwrap().role.is_leader);

    // 归档的 Leader 不占名额。
    let other = f.team("U");
    let old = f.role(&other.id, "role_leader");
    f.ts.archive_role(&old.id).unwrap();
    create(
        &f,
        &other.id,
        new_role("新 Leader", "role_new_leader", "NL", true),
    )
    .unwrap();
}

#[test]
fn code_is_unique_including_archived_and_prefix_unique_among_active() {
    let f = fixture();
    let team = f.team("T");
    assert_kind(
        create(&f, &team.id, new_role("X", "role_frontend", "XX", false)),
        ErrorKind::Conflict,
    );
    let msg = assert_kind(
        create(&f, &team.id, new_role("X", "role_x", "FE", false)),
        ErrorKind::Conflict,
    );
    assert!(msg.contains("前端开发"), "{msg}");
    let qa = f.role(&team.id, "role_qa");
    assert_kind(update(&f, &qa, "FE", false), ErrorKind::Conflict);

    let fe = f.role(&team.id, "role_frontend");
    f.ts.archive_role(&fe.id).unwrap();
    // code 仍被占用，前缀已释放。
    assert_kind(
        create(&f, &team.id, new_role("X", "role_frontend", "XX", false)),
        ErrorKind::Conflict,
    );
    create(
        &f,
        &team.id,
        new_role("新前端", "role_frontend2", "FE", false),
    )
    .unwrap();

    // 其他团队不受影响。
    let other = f.team("U");
    assert_eq!(f.role(&other.id, "role_frontend").task_prefix, "FE");
}

#[test]
fn update_changes_editable_fields_and_records_event() {
    let f = fixture();
    let team = f.team("T");
    let fe = f.role(&team.id, "role_frontend");
    let view =
        f.ts.update_role(UpdateRoleInput {
            role_id: fe.id.clone(),
            name: "Web 前端".into(),
            task_prefix: "WEB".into(),
            description: "职责".into(),
            guidelines: "约定".into(),
            is_leader: false,
        })
        .unwrap();
    assert_eq!(view.role.code, "role_frontend");
    assert_eq!(
        (view.role.name.as_str(), view.role.task_prefix.as_str()),
        ("Web 前端", "WEB")
    );
    assert_eq!(view.role.guidelines, "约定");
    let events = f.ts.get_team_overview(&team.id).unwrap().recent_events;
    assert_eq!(events[0].kind, EventKind::RoleUpdated);
    assert_eq!(events[0].role_id.as_deref(), Some(fe.id.as_str()));
}

#[test]
fn role_with_open_tasks_cannot_be_archived() {
    let f = fixture();
    let team = f.team("T");
    let fe = f.role(&team.id, "role_frontend");
    let t1 = f.readyable(&team.id, &fe, "A");
    let t2 = f.task(&team.id, &fe, "B");
    let msg = assert_kind(f.ts.archive_role(&fe.id), ErrorKind::Conflict);
    assert!(msg.contains('2'), "{msg}");

    f.drive(
        &t1.task.id,
        &[
            TaskStatus::Ready,
            TaskStatus::Doing,
            TaskStatus::Review,
            TaskStatus::Done,
        ],
    );
    f.drive(&t2.task.id, &[TaskStatus::Cancelled]);
    let archived = f.ts.archive_role(&fe.id).unwrap();
    assert!(archived.archived_at.is_some());

    // 归档后不可见、不能再操作，但历史任务仍能看到负责人。
    assert!(
        f.ts.list_roles(&team.id)
            .unwrap()
            .iter()
            .all(|v| v.role.id != fe.id)
    );
    assert_kind(f.ts.archive_role(&fe.id), ErrorKind::NotFound);
    assert_kind(update(&f, &fe, "FE", false), ErrorKind::NotFound);
    let detail = f.ts.get_task(&t1.task.id).unwrap();
    assert!(detail.owner.archived);
    // 已归档岗位不能成为新任务的负责人。
    assert_kind(
        f.try_task(&team.id, &fe, "C", TaskStatus::Todo, vec![], vec![]),
        ErrorKind::NotFound,
    );
}

#[test]
fn bind_and_unbind_harness() {
    let f = fixture();
    let team = f.team("T");
    let fe = f.role(&team.id, "role_frontend");
    let h1 = f.harness(&team.id, "frontend-local", HarnessKind::ClaudeCode);
    let h2 = f.harness(&team.id, "codex-local", HarnessKind::Codex);

    let view = f.ts.bind_role_harness(&fe.id, &h1.id).unwrap();
    assert_eq!(view.role.harness_id.as_deref(), Some(h1.id.as_str()));
    assert_eq!(view.harness.unwrap().name, "frontend-local");
    let view = f.ts.bind_role_harness(&fe.id, &h2.id).unwrap();
    assert_eq!(view.role.harness_id.as_deref(), Some(h2.id.as_str()));
    let view = f.ts.unbind_role_harness(&fe.id).unwrap();
    assert!(view.role.harness_id.is_none() && view.harness.is_none());
    // 重复解绑不报错也不写活动。
    f.ts.unbind_role_harness(&fe.id).unwrap();

    let kinds: Vec<_> =
        f.ts.list_events(EventQuery {
            team_id: team.id.clone(),
            cursor: None,
            limit: Some(3),
        })
        .unwrap()
        .events
        .into_iter()
        .map(|e| e.kind)
        .collect();
    assert_eq!(
        kinds,
        [
            EventKind::RoleHarnessUnbound,
            EventKind::RoleHarnessBound,
            EventKind::RoleHarnessBound
        ]
    );

    // 其他团队或已归档的 Harness 不能绑定。
    let other = f.team("U");
    let foreign = f.harness(&other.id, "x", HarnessKind::Mock);
    assert_kind(
        f.ts.bind_role_harness(&fe.id, &foreign.id),
        ErrorKind::NotFound,
    );
    f.ts.archive_harness(&h1.id).unwrap();
    assert_kind(f.ts.bind_role_harness(&fe.id, &h1.id), ErrorKind::NotFound);
}

#[test]
fn role_text_lengths_are_checked() {
    let f = fixture();
    let team = f.team("T");
    assert_kind(
        create(
            &f,
            &team.id,
            new_role(&"岗".repeat(101), "role_x", "XX", false),
        ),
        ErrorKind::Invalid,
    );
    let mut long = new_role("X", "role_x", "XX", false);
    long.guidelines = "x".repeat(20_001);
    assert_kind(create(&f, &team.id, long), ErrorKind::Invalid);
    let mut ctrl = new_role("X", "role_x", "XX", false);
    ctrl.description = "a\u{1b}b".into();
    assert_kind(create(&f, &team.id, ctrl), ErrorKind::Invalid);
    let qa = f.role(&team.id, "role_qa");
    let err = f.ts.update_role(UpdateRoleInput {
        role_id: qa.id.clone(),
        name: "QA".into(),
        task_prefix: "QA".into(),
        description: "x".repeat(20_001),
        guidelines: String::new(),
        is_leader: false,
    });
    assert_kind(err, ErrorKind::Invalid);
}

#[test]
fn archiving_a_role_clears_its_harness_binding() {
    let f = fixture();
    let team = f.team("T");
    let pm = f.role(&team.id, "role_product");
    let h = f.harness(&team.id, "h", HarnessKind::Mock);
    f.ts.bind_role_harness(&pm.id, &h.id).unwrap();
    let archived = f.ts.archive_role(&pm.id).unwrap();
    assert!(archived.harness_id.is_none());
    let event = &f.ts.get_team_overview(&team.id).unwrap().recent_events[0];
    assert_eq!(event.kind, EventKind::RoleArchived);
    assert_eq!(event.data["previousHarnessId"], h.id.as_str());
    // 已不被任何岗位引用，可以直接归档。
    f.ts.archive_harness(&h.id).unwrap();
}

#[test]
fn code_is_generated_when_omitted_and_stays_stable() {
    let f = plain_fixture();
    let team =
        f.ts.create_team(CreateTeamInput {
            name: "剪辑组".into(),
            description: String::new(),
            workspace_path: f.ws.to_string_lossy().into_owned(),
            roles: vec![custom_role("剪辑师", "R1"), custom_role("调色师", "R2")],
        })
        .unwrap();
    let codes: Vec<_> =
        f.ts.list_roles(&team.id)
            .unwrap()
            .into_iter()
            .map(|v| v.role.code)
            .collect();
    assert_eq!(codes, ["role_1", "role_2"]);

    // 空白 code 视为未填写；与已有（含用户填写的）code 冲突时序号继续递增。
    create(&f, &team.id, new_role("占位", "role_4", "ZW", false)).unwrap();
    let blank = create(&f, &team.id, new_role("配音", "  ", "PY", false)).unwrap();
    assert_eq!(blank.role.code, "role_5");

    // 归档后的 code 不复用；改名不改变 code。
    f.ts.archive_role(&blank.role.id).unwrap();
    let next = create(&f, &team.id, custom_role("字幕", "ZM")).unwrap();
    assert_eq!(next.role.code, "role_6");
    let renamed =
        f.ts.update_role(UpdateRoleInput {
            role_id: next.role.id.clone(),
            name: "字幕与包装".into(),
            task_prefix: "ZM".into(),
            description: String::new(),
            guidelines: String::new(),
            is_leader: true,
        })
        .unwrap();
    assert_eq!(renamed.role.code, "role_6");
    assert!(renamed.role.is_leader);
}

#[test]
fn generated_codes_are_unique_per_team() {
    let f = fixture();
    let a = f.team("A");
    let b =
        f.ts.create_team(CreateTeamInput {
            name: "B".into(),
            description: String::new(),
            workspace_path: f.ws.to_string_lossy().into_owned(),
            roles: vec![],
        })
        .unwrap();
    // 团队 A 已有 5 个示例岗位，新岗位从 role_6 开始；团队 B 独立编号。
    assert_eq!(
        create(&f, &a.id, custom_role("X", "XX")).unwrap().role.code,
        "role_6"
    );
    assert_eq!(
        create(&f, &b.id, custom_role("X", "XX")).unwrap().role.code,
        "role_1"
    );
}

#[test]
fn prefix_may_contain_digits_after_a_letter() {
    let f = plain_fixture();
    let team = f.team("T");
    let role = create(&f, &team.id, custom_role("剪辑师", "R1"))
        .unwrap()
        .role;
    let task = f.task(&team.id, &role, "粗剪");
    assert_eq!(task.task.key, "R1-001");
    for bad in ["1R", "R", "r1", "R_1", "ABCDE"] {
        let msg = assert_kind(
            create(&f, &team.id, custom_role("X", bad)),
            ErrorKind::Invalid,
        );
        assert!(msg.contains("以字母开头"), "{bad}: {msg}");
    }
}

#[test]
fn suggest_task_prefix_avoids_team_and_form_prefixes() {
    let f = fixture();
    let suggest = |team_id: Option<&str>, name: &str, taken: Option<Vec<&str>>| {
        f.ts.suggest_task_prefix(SuggestTaskPrefixInput {
            team_id: team_id.map(Into::into),
            name: name.into(),
            taken: taken.map(|t| t.into_iter().map(Into::into).collect()),
        })
    };
    assert_eq!(suggest(None, "Video Editor", None).unwrap(), "VE");
    assert_eq!(suggest(None, "Leader", None).unwrap(), "LE");
    assert_eq!(suggest(None, "剪辑师", None).unwrap(), "R1");
    assert_eq!(
        suggest(None, "调色师", Some(vec!["R1"])).unwrap(),
        "R2",
        "创建团队表单中尚未保存的前缀也要避开"
    );

    let team = f.team("T");
    // 示例岗位占用了 LD / PM / FE / BE / QA。
    assert_eq!(suggest(Some(&team.id), "QA", None).unwrap(), "QA2");
    assert_eq!(suggest(Some(&team.id), "Ld", None).unwrap(), "LD2");
    create(&f, &team.id, custom_role("剪辑师", "R1")).unwrap();
    assert_eq!(
        suggest(Some(&team.id), "调色师", Some(vec!["R2"])).unwrap(),
        "R3"
    );
    // 已归档岗位的前缀可以再用。
    let qa = f.role(&team.id, "role_qa");
    f.ts.archive_role(&qa.id).unwrap();
    assert_eq!(suggest(Some(&team.id), "QA", None).unwrap(), "QA");

    assert_kind(suggest(Some("nope"), "X", None), ErrorKind::NotFound);
}
