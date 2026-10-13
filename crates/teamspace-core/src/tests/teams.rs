use super::*;

#[test]
fn create_team_with_template_roles() {
    let f = fixture();
    let team = f.team("  AI Team  ");
    assert_eq!(team.name, "AI Team");
    assert_eq!(team.workspace_path, f.ws.to_string_lossy());
    assert!(team.archived_at.is_none());

    let roles = f.ts.list_roles(&team.id).unwrap();
    let codes: Vec<_> = roles.iter().map(|v| v.role.code.as_str()).collect();
    assert_eq!(
        codes,
        [
            "role_leader",
            "role_product",
            "role_frontend",
            "role_backend",
            "role_qa"
        ]
    );
    assert!(roles[0].role.is_leader);
    assert!(roles.iter().all(|v| v.harness.is_none()));

    assert_eq!(f.ts.list_teams().unwrap(), vec![team]);
}

#[test]
fn team_name_is_required_and_unique_among_active_teams() {
    let f = fixture();
    let input = |name: &str| CreateTeamInput {
        name: name.into(),
        description: String::new(),
        workspace_path: f.ws.to_string_lossy().into_owned(),
        roles: vec![],
    };
    assert_kind(f.ts.create_team(input("  ")), ErrorKind::Invalid);
    let first = f.ts.create_team(input("Alpha")).unwrap();
    let msg = assert_kind(f.ts.create_team(input("Alpha")), ErrorKind::Conflict);
    assert!(msg.contains("Alpha"), "{msg}");

    // 归档后名称可以复用，且归档团队不可见。
    f.raw_sql(&format!(
        "UPDATE teams SET archived_at = '2026-01-01T00:00:00.000Z' WHERE id = '{}'",
        first.id
    ));
    assert!(f.ts.list_teams().unwrap().is_empty());
    assert_kind(f.ts.get_team_overview(&first.id), ErrorKind::NotFound);
    f.ts.create_team(input("Alpha")).unwrap();
}

fn team_at(f: &Fixture, name: &str, path: &str) -> Result<Team> {
    f.ts.create_team(CreateTeamInput {
        name: name.into(),
        description: String::new(),
        workspace_path: path.into(),
        roles: vec![],
    })
}

#[test]
fn workspace_must_be_an_existing_directory() {
    let f = fixture();
    let missing = f.ws.join("missing");
    let msg = assert_kind(
        team_at(&f, "X", &missing.to_string_lossy()),
        ErrorKind::Invalid,
    );
    assert!(msg.contains("工作空间目录不存在"), "{msg}");
    f.touch("notes.txt");
    let msg = assert_kind(
        team_at(&f, "X", &f.ws.join("notes.txt").to_string_lossy()),
        ErrorKind::Invalid,
    );
    assert!(msg.contains("工作空间目录不存在"), "{msg}");
    assert_kind(team_at(&f, "X", "  "), ErrorKind::Invalid);
    assert!(f.ts.list_teams().unwrap().is_empty());
}

#[test]
fn plain_folder_can_be_a_workspace() {
    let f = plain_fixture();
    let path = format!("{}/", f.ws.to_string_lossy());
    let team = team_at(&f, "剪辑组", &path).unwrap();
    assert_eq!(team.workspace_path, f.ws.to_string_lossy(), "去掉末尾斜杠");

    let info = f.ts.inspect_workspace(&path).unwrap();
    assert_eq!(info.path, team.workspace_path);
    assert!(info.git.is_none());

    let o = f.ts.get_team_overview(&team.id).unwrap();
    assert!(o.workspace_problem.is_none());
    assert!(o.workspace.unwrap().git.is_none());
}

#[test]
fn git_subdirectory_stays_the_workspace() {
    let f = fixture();
    let sub = f.ws.join("videos/raw");
    fs::create_dir_all(&sub).unwrap();
    let team = team_at(&f, "T", &format!("{}/./", sub.to_string_lossy())).unwrap();
    assert_eq!(team.workspace_path, sub.to_string_lossy(), "不提升为仓库根");

    let o = f.ts.get_team_overview(&team.id).unwrap();
    let ws = o.workspace.unwrap();
    assert_eq!(ws.path, team.workspace_path);
    assert_eq!(ws.git.unwrap().branch.as_deref(), Some("main"));
}

#[test]
fn team_can_start_without_roles() {
    let f = plain_fixture();
    let team = team_at(&f, "空团队", &f.ws.to_string_lossy()).unwrap();
    assert!(f.ts.list_roles(&team.id).unwrap().is_empty());
    let o = f.ts.get_team_overview(&team.id).unwrap();
    assert!(o.roles.is_empty());
    assert_eq!(o.recent_events[0].summary, "创建团队「空团队」");
}

#[test]
fn invalid_initial_roles_roll_back_the_whole_team() {
    let f = fixture();
    let err = f.ts.create_team(CreateTeamInput {
        name: "X".into(),
        description: String::new(),
        workspace_path: f.ws.to_string_lossy().into_owned(),
        roles: vec![
            new_role("A", "role_a", "AA", true),
            new_role("B", "role_b", "BB", true),
        ],
    });
    assert_kind(err, ErrorKind::Conflict);
    assert!(f.ts.list_teams().unwrap().is_empty());
}

#[test]
fn overview_combines_workspace_roles_counts_tasks_and_events() {
    let f = fixture();
    let team = f.team("T");
    let fe = f.role(&team.id, "role_frontend");
    let doing = f.readyable(&team.id, &fe, "进行中");
    f.drive(&doing.task.id, &[TaskStatus::Ready, TaskStatus::Doing]);
    f.task(&team.id, &fe, "待办");

    let o = f.ts.get_team_overview(&team.id).unwrap();
    assert_eq!(o.team.id, team.id);
    let ws = o.workspace.unwrap();
    assert_eq!(ws.path, team.workspace_path);
    let git = ws.git.unwrap();
    assert_eq!(git.branch.as_deref(), Some("main"));
    assert_eq!(git.commit.as_deref(), Some("a31f8c2"));
    assert!(o.workspace_problem.is_none());
    assert_eq!(o.roles.len(), 5);
    let fe_card = o.roles.iter().find(|v| v.role.id == fe.id).unwrap();
    assert_eq!((fe_card.open_task_count, fe_card.doing_task_count), (2, 1));

    let counts: Vec<_> = o.task_counts.iter().map(|c| (c.status, c.count)).collect();
    assert_eq!(counts.len(), 7);
    assert_eq!(counts[0], (TaskStatus::Todo, 1));
    assert_eq!(counts[2], (TaskStatus::Doing, 1));
    assert_eq!(o.task_counts[2].label, "执行中");

    assert_eq!(o.active_tasks.len(), 1);
    assert_eq!(o.active_tasks[0].id, doing.task.id);
    assert_eq!(o.recent_events.len(), 5);
    assert_eq!(o.recent_events[0].kind, EventKind::TaskCreated);
}

#[test]
fn overview_reports_unavailable_workspace_without_failing() {
    let f = fixture();
    let team = f.team("T");
    fs::remove_dir_all(&f.ws).unwrap();
    let o = f.ts.get_team_overview(&team.id).unwrap();
    assert!(o.workspace.is_none());
    let problem = o.workspace_problem.unwrap();
    assert!(problem.contains("工作空间目录不存在"), "{problem}");
}

#[test]
fn inspect_workspace_and_detect_harnesses_delegate_to_probe() {
    let f = fixture();
    assert_eq!(
        f.ts.inspect_workspace(&f.ws.to_string_lossy())
            .unwrap()
            .git
            .unwrap()
            .commit
            .as_deref(),
        Some("a31f8c2")
    );
    let kinds: Vec<_> =
        f.ts.detect_harnesses()
            .into_iter()
            .map(|d| d.kind)
            .collect();
    assert_eq!(kinds, [HarnessKind::ClaudeCode, HarnessKind::Codex]);
}

#[test]
fn search_workspace_files_returns_relative_paths_capped_at_50() {
    let f = fixture();
    let team = f.team("T");
    for i in 0..60 {
        f.touch(&format!("src/file{i:02}.ts"));
    }
    f.touch("docs/PLAN.md");
    let hits =
        f.ts.search_workspace_files(FileSearchQuery {
            team_id: team.id.clone(),
            query: " plan ".into(),
        })
        .unwrap();
    assert_eq!(hits, ["docs/PLAN.md"]);
    let all =
        f.ts.search_workspace_files(FileSearchQuery {
            team_id: team.id,
            query: String::new(),
        })
        .unwrap();
    assert_eq!(all.len(), crate::probe::FILE_SEARCH_LIMIT);
}

#[test]
fn reopening_the_database_keeps_data() {
    let f = fixture();
    let team = f.team("T");
    let path = f.db_path();
    drop(f.ts);
    let ts = Teamspace::open(&path, Box::new(FakeProbe)).unwrap();
    assert_eq!(ts.list_teams().unwrap(), vec![team]);
}

#[test]
fn team_text_lengths_are_checked() {
    let f = fixture();
    let input = |name: String, description: String| CreateTeamInput {
        name,
        description,
        workspace_path: f.ws.to_string_lossy().into_owned(),
        roles: vec![],
    };
    assert_kind(
        f.ts.create_team(input("名".repeat(101), String::new())),
        ErrorKind::Invalid,
    );
    assert_kind(
        f.ts.create_team(input("A\u{7}".into(), String::new())),
        ErrorKind::Invalid,
    );
    assert_kind(
        f.ts.create_team(input("A".into(), "x".repeat(20_001))),
        ErrorKind::Invalid,
    );
    f.ts.create_team(input("名".repeat(100), "x".repeat(20_000)))
        .unwrap();
}
