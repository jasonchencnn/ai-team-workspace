use super::*;
use TaskStatus::*;

/// 从新建（TODO）到达各状态的流转路径。
fn path_to(status: TaskStatus) -> &'static [TaskStatus] {
    match status {
        Todo => &[],
        Ready => &[Ready],
        Doing => &[Ready, Doing],
        Blocked => &[Ready, Doing, Blocked],
        Review => &[Ready, Doing, Review],
        Done => &[Ready, Doing, Review, Done],
        Cancelled => &[Cancelled],
    }
}

fn edit(
    f: &Fixture,
    detail: &TaskDetail,
    refs: Vec<&str>,
    deps: Vec<String>,
) -> Result<TaskDetail> {
    f.ts.update_task(UpdateTaskInput {
        task_id: detail.task.id.clone(),
        title: detail.task.title.clone(),
        description: detail.task.description.clone(),
        owner_role_id: detail.task.owner_role_id.clone(),
        file_refs: refs.into_iter().map(file_ref).collect(),
        depends_on: deps,
    })
}

#[test]
fn keys_use_owner_prefix_and_per_prefix_sequence() {
    let f = fixture();
    let team = f.team("T");
    let fe = f.role(&team.id, "role_frontend");
    let be = f.role(&team.id, "role_backend");
    assert_eq!(f.task(&team.id, &fe, "a").task.key, "FE-001");
    assert_eq!(f.task(&team.id, &fe, "b").task.key, "FE-002");
    assert_eq!(f.task(&team.id, &be, "c").task.key, "BE-001");

    // 其他团队独立编号。
    let other = f.team("U");
    let other_fe = f.role(&other.id, "role_frontend");
    assert_eq!(f.task(&other.id, &other_fe, "d").task.key, "FE-001");
}

#[test]
fn key_is_stable_when_owner_changes() {
    let f = fixture();
    let team = f.team("T");
    let qa = f.role(&team.id, "role_qa");
    let leader = f.role(&team.id, "role_leader");
    let t = f.task(&team.id, &qa, "回归");
    let moved =
        f.ts.update_task(UpdateTaskInput {
            task_id: t.task.id.clone(),
            title: "回归".into(),
            description: String::new(),
            owner_role_id: leader.id.clone(),
            file_refs: vec![],
            depends_on: vec![],
        })
        .unwrap();
    assert_eq!(moved.task.key, "QA-001");
    assert_eq!(moved.owner.id, leader.id);
    assert!(
        moved.events[0].summary.contains("Leader"),
        "{}",
        moved.events[0].summary
    );
}

#[test]
fn prefix_sequence_continues_after_prefix_changes_and_reuse() {
    let f = fixture();
    let team = f.team("T");
    let fe = f.role(&team.id, "role_frontend");
    f.task(&team.id, &fe, "a");
    // 前缀改为 WEB：新任务用 WEB，旧任务编号不变。
    f.ts.update_role(UpdateRoleInput {
        role_id: fe.id.clone(),
        name: fe.name.clone(),
        task_prefix: "WEB".into(),
        description: String::new(),
        guidelines: String::new(),
        is_leader: false,
    })
    .unwrap();
    let fe = f.role(&team.id, "role_frontend");
    assert_eq!(f.task(&team.id, &fe, "b").task.key, "WEB-001");
    // 另一个岗位接手 FE 前缀，序号接着已有的 FE-001。
    let pm = f.role(&team.id, "role_product");
    f.ts.update_role(UpdateRoleInput {
        role_id: pm.id.clone(),
        name: pm.name.clone(),
        task_prefix: "FE".into(),
        description: String::new(),
        guidelines: String::new(),
        is_leader: false,
    })
    .unwrap();
    let pm = f.role(&team.id, "role_product");
    assert_eq!(f.task(&team.id, &pm, "c").task.key, "FE-002");
}

#[test]
fn create_validates_title_owner_status_and_refs() {
    let f = fixture();
    let team = f.team("T");
    let fe = f.role(&team.id, "role_frontend");
    assert_kind(
        f.try_task(&team.id, &fe, "  ", Todo, vec![], vec![]),
        ErrorKind::Invalid,
    );
    for status in [Doing, Blocked, Review, Done, Cancelled] {
        assert_kind(
            f.try_task(&team.id, &fe, "x", status, vec![], vec![]),
            ErrorKind::Invalid,
        );
    }
    for bad in ["/etc/hosts", "../x.md", "a/../../b"] {
        assert_kind(
            f.try_task(&team.id, &fe, "x", Todo, vec![bad], vec![]),
            ErrorKind::Invalid,
        );
    }
    let other = f.team("U");
    let foreign = f.role(&other.id, "role_frontend");
    assert_kind(
        f.try_task(&team.id, &foreign, "x", Todo, vec![], vec![]),
        ErrorKind::NotFound,
    );
    // 失败的创建不消耗编号。
    assert_eq!(f.task(&team.id, &fe, "ok").task.key, "FE-001");
}

#[test]
fn create_stores_normalized_refs_and_deps() {
    let f = fixture();
    let team = f.team("T");
    let fe = f.role(&team.id, "role_frontend");
    let dep = f.task(&team.id, &fe, "dep");
    let t = f
        .try_task(
            &team.id,
            &fe,
            " 页面 ",
            Todo,
            vec![
                "./docs/PLAN.md#当前状态",
                "docs/PLAN.md#当前状态",
                "src/App.tsx",
            ],
            vec![dep.task.id.clone(), dep.task.id.clone()],
        )
        .unwrap();
    assert_eq!(t.task.title, "页面");
    assert_eq!(t.task.status, Todo);
    assert_eq!(
        t.task.file_refs,
        vec![
            FileRef {
                path: "docs/PLAN.md".into(),
                anchor: Some("当前状态".into())
            },
            file_ref("src/App.tsx"),
        ]
    );
    assert_eq!(t.task.depends_on, vec![dep.task.id.clone()]);
    assert_eq!(t.dependencies[0].key, "FE-001");
    assert!(t.file_refs.iter().all(|r| !r.exists));
    assert_eq!(t.events.len(), 1);
    assert_eq!(t.events[0].kind, EventKind::TaskCreated);

    let upstream = f.ts.get_task(&dep.task.id).unwrap();
    assert_eq!(upstream.dependents.len(), 1);
    assert_eq!(upstream.dependents[0].id, t.task.id);
}

#[test]
fn ready_requires_refs_existing_files_and_done_dependencies() {
    let f = fixture();
    let team = f.team("T");
    let fe = f.role(&team.id, "role_frontend");

    // 没有引用。
    let t = f.task(&team.id, &fe, "a");
    assert_eq!(t.readiness, ["至少需要一个文件引用"]);
    assert!(!t.allowed_transitions.contains(&Ready));
    assert_kind(f.move_to(&t.task.id, Ready), ErrorKind::Conflict);

    // 引用的文件不存在。
    let t = edit(&f, &t, vec!["docs/spec.md"], vec![]).unwrap();
    assert_eq!(t.readiness.len(), 1);
    assert!(t.readiness[0].contains("docs/spec.md"));
    let msg = assert_kind(f.move_to(&t.task.id, Ready), ErrorKind::Conflict);
    assert!(msg.contains("docs/spec.md"), "{msg}");

    // 文件存在但依赖未完成。
    f.touch("docs/spec.md");
    let dep = f.readyable(&team.id, &fe, "dep");
    let t = edit(&f, &t, vec!["docs/spec.md"], vec![dep.task.id.clone()]).unwrap();
    assert_eq!(t.readiness.len(), 1);
    assert!(t.readiness[0].contains(&dep.task.key));
    assert_kind(f.move_to(&t.task.id, Ready), ErrorKind::Conflict);

    // 依赖完成后就绪。
    f.drive(&dep.task.id, &[Ready, Doing, Review, Done]);
    let t = f.ts.get_task(&t.task.id).unwrap();
    assert!(t.readiness.is_empty());
    assert_eq!(t.allowed_transitions, [Ready, Cancelled]);
    let t = f.move_to(&t.task.id, Ready).unwrap();
    assert_eq!(t.task.status, Ready);
    assert!(t.file_refs[0].exists);
}

#[test]
fn create_as_ready_checks_readiness() {
    let f = fixture();
    let team = f.team("T");
    let fe = f.role(&team.id, "role_frontend");
    assert_kind(
        f.try_task(&team.id, &fe, "x", Ready, vec!["missing.md"], vec![]),
        ErrorKind::Conflict,
    );
    f.touch("README.md");
    let t = f
        .try_task(&team.id, &fe, "x", Ready, vec!["README.md#用法"], vec![])
        .unwrap();
    assert_eq!(t.task.status, Ready);
}

#[test]
fn editing_a_ready_task_must_keep_it_ready() {
    let f = fixture();
    let team = f.team("T");
    let fe = f.role(&team.id, "role_frontend");
    let t = f.readyable(&team.id, &fe, "a");
    let t = f.move_to(&t.task.id, Ready).unwrap();
    assert_kind(edit(&f, &t, vec![], vec![]), ErrorKind::Conflict);
    assert_kind(edit(&f, &t, vec!["nope.md"], vec![]), ErrorKind::Conflict);
    f.touch("other.md");
    let t = edit(&f, &t, vec!["other.md"], vec![]).unwrap();
    assert_eq!(t.task.status, Ready);
    // TODO 任务可以随意编辑。
    let todo = f.task(&team.id, &fe, "b");
    edit(&f, &todo, vec!["nope.md"], vec![]).unwrap();
}

#[test]
fn every_transition_follows_the_table() {
    let f = fixture();
    let team = f.team("T");
    let fe = f.role(&team.id, "role_frontend");
    for from in TaskStatus::ALL {
        for to in TaskStatus::ALL {
            let t = f.readyable(&team.id, &fe, &format!("{from:?}->{to:?}"));
            let t = f.drive(&t.task.id, path_to(from));
            assert_eq!(t.task.status, from);
            assert_eq!(t.allowed_transitions, from.next(), "{from:?}");
            let result = f.move_to(&t.task.id, to);
            if from.can_transition_to(to) {
                assert_eq!(result.unwrap().task.status, to, "{from:?} -> {to:?}");
            } else {
                assert_kind(result, ErrorKind::Conflict);
            }
        }
    }
}

#[test]
fn blocked_requires_reason_which_is_cleared_on_leave() {
    let f = fixture();
    let team = f.team("T");
    let fe = f.role(&team.id, "role_frontend");
    let t = f.readyable(&team.id, &fe, "a");
    f.drive(&t.task.id, &[Ready, Doing]);
    for reason in [None, Some("  ".to_owned())] {
        assert_kind(
            f.ts.change_task_status(ChangeTaskStatusInput {
                task_id: t.task.id.clone(),
                to: Blocked,
                reason,
            }),
            ErrorKind::Invalid,
        );
    }
    let blocked =
        f.ts.change_task_status(ChangeTaskStatusInput {
            task_id: t.task.id.clone(),
            to: Blocked,
            reason: Some(" 等待后端接口 ".into()),
        })
        .unwrap();
    assert_eq!(blocked.task.blocked_reason.as_deref(), Some("等待后端接口"));
    assert_eq!(
        blocked.events[0].summary,
        format!("{} 状态 执行中 → 受阻：等待后端接口", t.task.key)
    );
    let board =
        f.ts.get_task_board(TaskBoardQuery {
            team_id: team.id.clone(),
            include_cancelled: None,
        })
        .unwrap();
    let col = board.columns.iter().find(|c| c.status == Blocked).unwrap();
    assert_eq!(col.tasks[0].blocked_reason.as_deref(), Some("等待后端接口"));

    let resumed = f.move_to(&t.task.id, Doing).unwrap();
    assert!(resumed.task.blocked_reason.is_none());
}

#[test]
fn dependency_cycles_are_rejected_with_the_path() {
    let f = fixture();
    let team = f.team("T");
    let fe = f.role(&team.id, "role_frontend");
    let a = f.task(&team.id, &fe, "a");
    let b = f
        .try_task(&team.id, &fe, "b", Todo, vec![], vec![a.task.id.clone()])
        .unwrap();
    let c = f
        .try_task(&team.id, &fe, "c", Todo, vec![], vec![b.task.id.clone()])
        .unwrap();
    let msg = assert_kind(
        edit(&f, &a, vec![], vec![c.task.id.clone()]),
        ErrorKind::Conflict,
    );
    assert_eq!(msg, "依赖形成环路：FE-001 → FE-003 → FE-002 → FE-001");
    // 失败的编辑不改变依赖。
    assert!(
        f.ts.get_task(&a.task.id)
            .unwrap()
            .task
            .depends_on
            .is_empty()
    );

    assert_kind(
        edit(&f, &a, vec![], vec![a.task.id.clone()]),
        ErrorKind::Invalid,
    );
    let other = f.team("U");
    let other_fe = f.role(&other.id, "role_frontend");
    let foreign = f.task(&other.id, &other_fe, "x");
    assert_kind(
        edit(&f, &a, vec![], vec![foreign.task.id]),
        ErrorKind::NotFound,
    );
    assert_kind(
        edit(&f, &a, vec![], vec!["nope".into()]),
        ErrorKind::NotFound,
    );

    // 无环的菱形依赖允许。
    edit(&f, &c, vec![], vec![a.task.id.clone(), b.task.id.clone()]).unwrap();
}

#[test]
fn board_groups_by_status_and_hides_cancelled_by_default() {
    let f = fixture();
    let team = f.team("T");
    let fe = f.role(&team.id, "role_frontend");
    let done = f.readyable(&team.id, &fe, "done");
    f.drive(&done.task.id, path_to(Done));
    let cancelled = f.task(&team.id, &fe, "cancelled");
    f.drive(&cancelled.task.id, &[Cancelled]);
    f.task(&team.id, &fe, "todo");

    let board =
        f.ts.get_task_board(TaskBoardQuery {
            team_id: team.id.clone(),
            include_cancelled: None,
        })
        .unwrap();
    let statuses: Vec<_> = board.columns.iter().map(|c| c.status).collect();
    assert_eq!(statuses, [Todo, Ready, Doing, Blocked, Review, Done]);
    assert_eq!(board.columns[2].label, "执行中");
    let counts: Vec<_> = board.columns.iter().map(|c| c.tasks.len()).collect();
    assert_eq!(counts, [1, 0, 0, 0, 0, 1]);
    assert_eq!(board.columns[5].tasks[0].file_ref_count, 1);

    let board =
        f.ts.get_task_board(TaskBoardQuery {
            team_id: team.id.clone(),
            include_cancelled: Some(true),
        })
        .unwrap();
    assert_eq!(board.columns.len(), 7);
    assert_eq!(board.columns[6].tasks[0].title, "cancelled");
}

#[test]
fn summaries_count_open_dependencies() {
    let f = fixture();
    let team = f.team("T");
    let fe = f.role(&team.id, "role_frontend");
    let a = f.readyable(&team.id, &fe, "a");
    let b = f.task(&team.id, &fe, "b");
    let c = f
        .try_task(
            &team.id,
            &fe,
            "c",
            Todo,
            vec![],
            vec![a.task.id.clone(), b.task.id.clone()],
        )
        .unwrap();
    f.drive(&a.task.id, path_to(Done));
    let board =
        f.ts.get_task_board(TaskBoardQuery {
            team_id: team.id,
            include_cancelled: None,
        })
        .unwrap();
    let card = board.columns[0]
        .tasks
        .iter()
        .find(|t| t.id == c.task.id)
        .unwrap();
    assert_eq!((card.dependency_count, card.open_dependency_count), (2, 1));
}

#[test]
fn task_statuses_have_backend_labels() {
    let f = fixture();
    let labels: Vec<_> =
        f.ts.list_task_statuses()
            .into_iter()
            .map(|s| (s.label, s.closed))
            .collect();
    let expected = [
        ("TODO", false),
        ("READY", false),
        ("执行中", false),
        ("受阻", false),
        ("待验收", false),
        ("已完成", true),
        ("已取消", true),
    ];
    assert_eq!(labels, expected.map(|(l, c)| (l.to_owned(), c)).to_vec());
}

#[test]
fn unknown_task_is_not_found() {
    let f = fixture();
    assert_kind(f.ts.get_task("nope"), ErrorKind::NotFound);
    assert_kind(f.move_to("nope", Ready), ErrorKind::NotFound);
}
