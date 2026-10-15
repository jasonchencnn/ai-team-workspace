//! QA 第 2 轮（2026-10-14）补充的内核单元测试，断言按 `cases/用例总表.md` 的预期写。
//!
//! 每个测试在函数文档里标明对应的用例编号。确认是产品缺陷的测试保留并标为 `#[ignore]`，
//! 忽略原因以「缺陷 <用例编号>」开头，断言保持用例的原始预期。

use std::os::unix::fs::symlink;

use super::*;
use crate::role_folder::{BRIEF_FILE, JOURNAL_FILE, PROCESS_DIR};
use crate::system_probe::LocalProbe;

// ---------------------------------------------------------------- 公共辅助

fn read(path: &Path) -> String {
    fs::read_to_string(path).unwrap_or_else(|err| panic!("read {}: {err}", path.display()))
}

fn team_at(f: &Fixture, name: &str, path: &str, roles: Vec<NewRole>) -> Result<Team> {
    f.ts.create_team(CreateTeamInput {
        name: name.into(),
        description: String::new(),
        workspace_path: path.into(),
        roles,
    })
}

fn add_role(f: &Fixture, team_id: &str, role: NewRole) -> Result<RoleView> {
    f.ts.create_role(CreateRoleInput {
        team_id: team_id.into(),
        role,
    })
}

fn view(f: &Fixture, team_id: &str, code: &str) -> RoleView {
    f.ts.list_roles(team_id)
        .unwrap()
        .into_iter()
        .find(|v| v.role.code == code)
        .unwrap()
}

fn events(f: &Fixture, team_id: &str) -> Vec<Event> {
    f.ts.list_events(EventQuery {
        team_id: team_id.into(),
        cursor: None,
        limit: Some(200),
    })
    .unwrap()
    .events
}

fn count_events(f: &Fixture) -> i64 {
    rusqlite::Connection::open(f.db_path())
        .unwrap()
        .query_row("SELECT COUNT(*) FROM events", [], |row| row.get(0))
        .unwrap()
}

fn rel_dir(f: &Fixture, rel: &str) -> PathBuf {
    rel.split('/').fold(f.ws.clone(), |p, part| p.join(part))
}

/// 工作日志里的条目文字（去掉行首的 `- 时间 · `）。
fn journal(f: &Fixture, rel: &str) -> Vec<String> {
    read(&rel_dir(f, rel).join(JOURNAL_FILE))
        .lines()
        .filter_map(|l| l.strip_prefix("- "))
        .map(|l| l.split_once(" · ").unwrap().1.to_owned())
        .collect()
}

/// 用真实的 `LocalProbe` 打开同一个数据库（文件搜索、真实 Git 目录探测用）。
fn local_ts(f: &Fixture) -> Teamspace {
    Teamspace::open_with(
        &f.dir.path().join("local.db"),
        Box::new(LocalProbe::new()),
        HostConfig::default(),
    )
    .unwrap()
}

fn git(dir: &Path, args: &[&str]) {
    let ok = std::process::Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(args)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .unwrap()
        .success();
    assert!(ok, "git {args:?}");
}

fn git_init(dir: &Path) {
    git(dir, &["init", "-q", "-b", "main"]);
    git(dir, &["config", "user.email", "t@example.com"]);
    git(dir, &["config", "user.name", "t"]);
    git(dir, &["config", "commit.gpgsign", "false"]);
}

fn write(dir: &Path, rel: &str, text: &str) {
    let path = dir.join(rel);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, text).unwrap();
}

fn search(ts: &Teamspace, team_id: &str, query: &str) -> Result<Vec<String>> {
    ts.search_workspace_files(FileSearchQuery {
        team_id: team_id.into(),
        query: query.into(),
    })
}

// ---------------------------------------------------------------- K-TEAM

/// K-TEAM-003：五类无效工作空间路径都被拒绝，不留团队、不生成 `Teamspace/`。
#[test]
fn k_team_003_invalid_workspace_paths_are_rejected_without_residue() {
    let f = plain_fixture();
    f.touch("notes.txt");
    let nul = format!("{}/a\0b", f.ws.display());
    let cases: Vec<(String, &str)> = vec![
        (
            f.ws.join("missing").display().to_string(),
            "工作空间目录不存在",
        ),
        (
            f.ws.join("notes.txt").display().to_string(),
            "工作空间目录不存在",
        ),
        ("  ".to_owned(), "请选择工作空间目录"),
        // 不做 `~` 展开：按相对路径处理，当前目录下没有名为 `~` 的目录。
        ("~/Documents".to_owned(), "工作空间目录不存在"),
        (nul, "工作空间目录不存在"),
    ];
    for (path, needle) in cases {
        let msg = assert_kind(
            team_at(&f, "X", &path, vec![new_role("A", "role_a", "AA", false)]),
            ErrorKind::Invalid,
        );
        assert!(msg.contains(needle), "{path:?} -> {msg}");
    }
    assert!(f.ts.list_teams().unwrap().is_empty());
    assert!(!f.ws.join("Teamspace").exists(), "不应生成 Teamspace/");
}

/// K-TEAM-004：路径只做词法规范化，符号链接保持所选路径本身。
#[test]
fn k_team_004_lexical_normalization_keeps_symlinks() {
    let f = plain_fixture();
    let base = f.dir.path().join("base");
    fs::create_dir_all(base.join("a/b")).unwrap();
    symlink(base.join("a"), base.join("link")).unwrap();
    let a = base.join("a").display().to_string();

    for (i, input) in [
        format!("{}/a/", base.display()),
        format!("  {}/./a/b/.. ", base.display()),
    ]
    .into_iter()
    .enumerate()
    {
        let info = f.ts.inspect_workspace(&input).unwrap();
        assert_eq!(info.path, a, "{input:?}");
        let team = team_at(&f, &format!("规范{i}"), &input, vec![]).unwrap();
        assert_eq!(team.workspace_path, a);
        assert!(!team.workspace_path.ends_with('/'));
    }
    let cwd = std::env::current_dir().unwrap().display().to_string();
    assert_eq!(f.ts.inspect_workspace(".").unwrap().path, cwd);

    let link = base.join("link").display().to_string();
    assert_eq!(f.ts.inspect_workspace(&link).unwrap().path, link);
    let team = team_at(&f, "链接", &link, vec![]).unwrap();
    assert_eq!(team.workspace_path, link, "符号链接不被解析成 base/a");
    assert_ne!(team.workspace_path, a);
}

/// K-TEAM-005：重名、归档后复用、整体回滚且磁盘无残留、名称超长。
#[test]
fn k_team_005_duplicate_names_and_rollback_leave_no_residue() {
    let f = plain_fixture();
    let ws = f.ws.display().to_string();
    let first = team_at(&f, "Alpha", &ws, vec![]).unwrap();
    let msg = assert_kind(team_at(&f, " Alpha ", &ws, vec![]), ErrorKind::Conflict);
    assert!(msg.contains("Alpha"), "{msg}");

    f.raw_sql(&format!(
        "UPDATE teams SET archived_at = '2026-01-01T00:00:00.000Z' WHERE id = '{}'",
        first.id
    ));
    let again = team_at(&f, "Alpha", &ws, vec![]).unwrap();
    assert_ne!(again.id, first.id);

    // 两个 Leader：整个团队回滚，磁盘上没有为「X」的岗位生成文件夹。
    assert!(!f.ws.join("Teamspace").exists(), "前面的团队没有岗位");
    assert_kind(
        team_at(
            &f,
            "X",
            &ws,
            vec![
                new_role("甲岗", "role_a", "AA", true),
                new_role("乙岗", "role_b", "BB", true),
            ],
        ),
        ErrorKind::Conflict,
    );
    assert!(f.ts.list_teams().unwrap().iter().all(|t| t.name != "X"));
    assert!(
        !f.ws.join("Teamspace").exists(),
        "回滚后不应留下 Teamspace/ 或岗位文件夹"
    );

    assert_kind(
        team_at(&f, &"名".repeat(101), &ws, vec![]),
        ErrorKind::Invalid,
    );
}

/// K-TEAM-006：同一工作空间路径的第二、第三个团队，岗位文件夹互不覆盖。
#[test]
fn k_team_006_second_team_on_same_workspace_gets_distinct_folders() {
    let f = plain_fixture();
    let a = f.team("A");
    let a_dir = f.ws.join("Teamspace/前端开发");
    let a_brief = read(&a_dir.join(BRIEF_FILE));
    let a_journal = read(&a_dir.join(JOURNAL_FILE));
    assert_eq!(
        view(&f, &a.id, "role_frontend").folder.as_deref(),
        Some("Teamspace/前端开发")
    );

    let b = f.team("B");
    let c = f.team("C");
    assert_eq!(
        view(&f, &b.id, "role_frontend").folder.as_deref(),
        Some("Teamspace/前端开发（role_frontend）")
    );
    assert_eq!(
        view(&f, &c.id, "role_frontend").folder.as_deref(),
        Some("Teamspace/前端开发（role_frontend）2")
    );
    // A 的内容未被改动。
    assert_eq!(read(&a_dir.join(BRIEF_FILE)), a_brief);
    assert_eq!(read(&a_dir.join(JOURNAL_FILE)), a_journal);
    assert!(
        read(
            &f.ws
                .join("Teamspace/前端开发（role_frontend）")
                .join(BRIEF_FILE)
        )
        .contains("- 所属团队：B")
    );
    assert!(
        read(
            &f.ws
                .join("Teamspace/前端开发（role_frontend）2")
                .join(BRIEF_FILE)
        )
        .contains("- 所属团队：C")
    );
    // 重新打开 A、B 的概览（再次同步）后三个文件夹仍各归各的。
    f.ts.get_team_overview(&a.id).unwrap();
    f.ts.get_team_overview(&b.id).unwrap();
    assert_eq!(
        view(&f, &a.id, "role_frontend").folder.as_deref(),
        Some("Teamspace/前端开发")
    );
    assert_eq!(read(&a_dir.join(BRIEF_FILE)), a_brief);
}

/// K-TEAM-007 A)：工作空间根目录只读时团队仍创建成功，恢复后补建文件夹。
#[test]
fn k_team_007_readonly_workspace_root_does_not_fail_creation() {
    use std::os::unix::fs::PermissionsExt;
    let f = plain_fixture();
    fs::set_permissions(&f.ws, fs::Permissions::from_mode(0o555)).unwrap();
    let team = team_at(
        &f,
        "T",
        &f.ws.display().to_string(),
        vec![new_role("设计", "role_design", "DS", false)],
    );
    // 先读出结果再恢复权限，避免断言失败时留下无法清理的只读目录。
    let snapshot = team.as_ref().ok().map(|team| {
        (
            view(&f, &team.id, "role_design"),
            events(&f, &team.id)
                .into_iter()
                .filter(|e| e.kind == EventKind::RoleFolderError)
                .count(),
        )
    });
    fs::set_permissions(&f.ws, fs::Permissions::from_mode(0o755)).unwrap();
    let team = team.unwrap();
    let (card, errors) = snapshot.unwrap();
    assert!(card.folder.is_none());
    let problem = card.folder_problem.unwrap();
    assert!(problem.contains("无法写入岗位工作区目录"), "{problem}");
    assert_eq!(errors, 1, "恰有 1 条 role_folder_error");

    // 恢复后打开概览：文件夹补建、问题清除。
    let overview = f.ts.get_team_overview(&team.id).unwrap();
    let card = overview
        .roles
        .iter()
        .find(|v| v.role.code == "role_design")
        .unwrap();
    assert_eq!(card.folder.as_deref(), Some("Teamspace/设计"));
    assert!(card.folder_problem.is_none());
    assert!(f.ws.join("Teamspace/设计").join(BRIEF_FILE).is_file());
}

/// K-TEAM-007 B)：团队创建后工作空间被删除，命令不失败，卡片带同样的说明。
#[test]
fn k_team_007_deleted_workspace_keeps_commands_working() {
    let f = plain_fixture();
    let team = f.team("T");
    let fe = f.role(&team.id, "role_frontend");
    let mock = f.harness(&team.id, "Mock", AcpPreset::Mock);
    fs::remove_dir_all(&f.ws).unwrap();

    let overview = f.ts.get_team_overview(&team.id).unwrap();
    assert!(overview.workspace.is_none());
    assert!(
        overview
            .workspace_problem
            .unwrap()
            .contains("工作空间目录不存在")
    );
    let bound = f.ts.bind_role_harness(&fe.id, &mock.id).unwrap();
    assert_eq!(bound.role.harness_id.as_deref(), Some(mock.id.as_str()));
    assert!(bound.folder_problem.unwrap().contains("工作空间目录不存在"));
    let renamed =
        f.ts.update_role(UpdateRoleInput {
            role_id: fe.id.clone(),
            name: "Web 前端".into(),
            task_prefix: fe.task_prefix.clone(),
            description: fe.description.clone(),
            guidelines: fe.guidelines.clone(),
            is_leader: false,
        })
        .unwrap();
    assert_eq!(renamed.role.name, "Web 前端");
    assert!(
        renamed
            .folder_problem
            .unwrap()
            .contains("工作空间目录不存在")
    );
    let card = view(&f, &team.id, "role_frontend");
    assert!(card.folder_problem.unwrap().contains("工作空间目录不存在"));
}

// ---------------------------------------------------------------- K-SRCH

/// K-SRCH-001：普通文件夹与 Git 仓库在服务层同一用例里对照（真实 LocalProbe 与真实 Git）。
#[test]
fn k_srch_001_plain_folder_and_git_repo_both_searchable() {
    let f = plain_fixture();
    let ts = local_ts(&f);
    let make_team = |name: &str, path: &Path| {
        ts.create_team(CreateTeamInput {
            name: name.into(),
            description: String::new(),
            workspace_path: path.display().to_string(),
            roles: vec![],
        })
        .unwrap()
    };

    // A) 普通文件夹。
    let plain = f.dir.path().join("plain");
    write(&plain, "docs/PLAN.md", "p");
    write(&plain, "src/a.ts", "a");
    let team_a = make_team("A", &plain);
    assert_eq!(search(&ts, &team_a.id, " plan ").unwrap(), ["docs/PLAN.md"]);
    assert_eq!(search(&ts, &team_a.id, "PLAN").unwrap(), ["docs/PLAN.md"]);
    let all = search(&ts, &team_a.id, "").unwrap();
    assert!(
        all.contains(&"docs/PLAN.md".to_owned()) && all.contains(&"src/a.ts".to_owned()),
        "{all:?}"
    );
    assert!(all.iter().all(|p| !p.starts_with('/') && !p.contains('\\')));

    // B) Git 仓库：已跟踪、未跟踪未忽略、被忽略。
    let repo = f.dir.path().join("repo");
    fs::create_dir_all(&repo).unwrap();
    git_init(&repo);
    write(&repo, "docs/PLAN.md", "p");
    write(&repo, "src/tracked.ts", "t");
    write(&repo, ".gitignore", "ignored.log\nbuild-out/\n");
    git(
        &repo,
        &["add", "docs/PLAN.md", "src/tracked.ts", ".gitignore"],
    );
    git(&repo, &["commit", "-q", "-m", "init"]);
    write(&repo, "src/untracked.ts", "u");
    write(&repo, "ignored.log", "i");
    write(&repo, "build-out/x.js", "x");
    write(&repo, "videos/raw/clip.txt", "c");
    let team_b = make_team("B", &repo);
    assert_eq!(search(&ts, &team_b.id, " plan ").unwrap(), ["docs/PLAN.md"]);
    let all = search(&ts, &team_b.id, "").unwrap();
    for want in [
        "docs/PLAN.md",
        "src/tracked.ts",
        "src/untracked.ts",
        "videos/raw/clip.txt",
    ] {
        assert!(all.contains(&want.to_owned()), "缺少 {want}: {all:?}");
    }
    for unwanted in ["ignored.log", "build-out/x.js"] {
        assert!(
            !all.contains(&unwanted.to_owned()),
            "不应包含被忽略的 {unwanted}: {all:?}"
        );
    }

    // Git 子目录作为工作空间：只返回子目录内文件，路径相对子目录。
    let team_sub = make_team("Sub", &repo.join("videos/raw"));
    assert_eq!(search(&ts, &team_sub.id, "").unwrap(), ["clip.txt"]);
    assert!(search(&ts, &team_sub.id, "PLAN").unwrap().is_empty());
}

/// K-SRCH-003：异常输入与越界。
#[test]
fn k_srch_003_odd_queries_missing_workspace_and_outside_files() {
    let f = plain_fixture();
    let ts = local_ts(&f);
    let ws = f.dir.path().join("work");
    for name in [
        "a%b.md",
        "x*y.md",
        "[z].md",
        "q'uote.md",
        "say\"hi\".md",
        "文档.md",
        "😀.md",
        "r\\s.md",
    ] {
        write(&ws, name, "x");
    }
    write(&ws, "plan.md", "x");
    write(f.dir.path(), "outside/plan.md", "outside");
    write(f.dir.path(), "outside/outside-only.md", "outside");
    let team = ts
        .create_team(CreateTeamInput {
            name: "T".into(),
            description: String::new(),
            workspace_path: ws.display().to_string(),
            roles: vec![],
        })
        .unwrap();

    // 1) 特殊字符按子串匹配，没有通配含义。
    for (query, expected) in [
        ("%", vec!["a%b.md"]),
        ("*", vec!["x*y.md"]),
        ("[", vec!["[z].md"]),
        ("\\", vec!["r\\s.md"]),
        ("'", vec!["q'uote.md"]),
        ("\"", vec!["say\"hi\".md"]),
        ("文", vec!["文档.md"]),
        ("😀", vec!["😀.md"]),
        ("a%", vec!["a%b.md"]),
        (".*", vec![]),
        ("?", vec![]),
        ("_", vec![]),
        ("%b", vec!["a%b.md"]),
    ] {
        assert_eq!(
            search(&ts, &team.id, query).unwrap(),
            expected,
            "query {query:?}"
        );
    }
    assert!(
        search(&ts, &team.id, &"x".repeat(10_000))
            .unwrap()
            .is_empty()
    );

    // 5) 工作空间外的同名文件不会出现。
    assert_eq!(search(&ts, &team.id, "plan").unwrap(), ["plan.md"]);
    assert!(search(&ts, &team.id, "outside-only").unwrap().is_empty());

    // 4) 不存在的团队。
    assert_kind(search(&ts, "nope", "plan"), ErrorKind::NotFound);

    // 3) 工作空间变成文件。
    fs::remove_dir_all(&ws).unwrap();
    fs::write(&ws, "now a file").unwrap();
    let msg = assert_kind(search(&ts, &team.id, "plan"), ErrorKind::Invalid);
    assert!(msg.contains("不是目录"), "{msg}");
    // 2) 工作空间被删除。
    fs::remove_file(&ws).unwrap();
    let msg = assert_kind(search(&ts, &team.id, "plan"), ErrorKind::Invalid);
    assert!(msg.contains("工作区目录不存在或已移动"), "{msg}");
}

// ---------------------------------------------------------------- K-ROLE

/// K-ROLE-006：TODO / READY / DOING / BLOCKED / REVIEW 各状态都阻止岗位归档，全部结束后成功。
#[test]
fn k_role_006_every_open_status_blocks_archiving() {
    use TaskStatus::*;
    let f = plain_fixture();
    let team = f.team("T");
    let cases: [(TaskStatus, &[TaskStatus]); 5] = [
        (Todo, &[]),
        (Ready, &[Ready]),
        (Doing, &[Ready, Doing]),
        (Blocked, &[Ready, Doing, Blocked]),
        (Review, &[Ready, Doing, Review]),
    ];
    for (i, (status, path)) in cases.into_iter().enumerate() {
        let role = add_role(
            &f,
            &team.id,
            new_role(
                &format!("岗{i}"),
                &format!("role_x{i}"),
                &format!("X{i}"),
                false,
            ),
        )
        .unwrap()
        .role;
        let task = f.readyable(&team.id, &role, &format!("任务{i}"));
        f.drive(&task.task.id, path);
        assert_eq!(f.ts.get_task(&task.task.id).unwrap().task.status, status);
        let msg = assert_kind(f.ts.archive_role(&role.id), ErrorKind::Conflict);
        assert!(msg.contains("还负责 1 个未结束的任务"), "{status:?}: {msg}");
        assert!(msg.contains("请先转交或结束"), "{status:?}: {msg}");

        // 结束任务后可归档。
        match status {
            Review => {
                f.drive(&task.task.id, &[Done]);
            }
            _ => {
                f.drive(&task.task.id, &[Cancelled]);
            }
        }
        let archived = f.ts.archive_role(&role.id).unwrap();
        assert!(archived.archived_at.is_some(), "{status:?}");
    }
    // 多个未结束任务时给出总数。
    let fe = f.role(&team.id, "role_frontend");
    f.task(&team.id, &fe, "a");
    f.task(&team.id, &fe, "b");
    f.task(&team.id, &fe, "c");
    let msg = assert_kind(f.ts.archive_role(&fe.id), ErrorKind::Conflict);
    assert!(msg.contains("还负责 3 个未结束的任务"), "{msg}");
}

/// K-ROLE-007：归档后解绑 Harness、记录原绑定、不可再操作、历史保留。
#[test]
fn k_role_007_archived_role_is_unbound_and_inert_but_history_remains() {
    let f = plain_fixture();
    let team = f.team("T");
    let fe = f.role(&team.id, "role_frontend");
    let h = f.harness(&team.id, "h1", AcpPreset::Mock);
    let other = f.harness(&team.id, "h2", AcpPreset::Mock);
    f.ts.bind_role_harness(&fe.id, &h.id).unwrap();
    let done = f.readyable(&team.id, &fe, "已完成任务");
    f.drive(
        &done.task.id,
        &[
            TaskStatus::Ready,
            TaskStatus::Doing,
            TaskStatus::Review,
            TaskStatus::Done,
        ],
    );

    let archived = f.ts.archive_role(&fe.id).unwrap();
    assert!(archived.harness_id.is_none());
    let event = &events(&f, &team.id)[0];
    assert_eq!(event.kind, EventKind::RoleArchived);
    assert_eq!(event.data["previousHarnessId"], h.id.as_str());
    f.ts.archive_harness(&h.id).unwrap();
    let baseline = count_events(&f);

    // 再次归档、编辑、绑定、工作台、作为负责人：均 not_found。
    assert_kind(f.ts.archive_role(&fe.id), ErrorKind::NotFound);
    assert_kind(
        f.ts.update_role(UpdateRoleInput {
            role_id: fe.id.clone(),
            name: "x".into(),
            task_prefix: "FE".into(),
            description: String::new(),
            guidelines: String::new(),
            is_leader: false,
        }),
        ErrorKind::NotFound,
    );
    assert_kind(
        f.ts.bind_role_harness(&fe.id, &other.id),
        ErrorKind::NotFound,
    );
    assert_kind(f.ts.unbind_role_harness(&fe.id), ErrorKind::NotFound);
    assert_kind(f.ts.get_role_workbench(&fe.id), ErrorKind::NotFound);
    assert_kind(
        f.try_task(&team.id, &fe, "新任务", TaskStatus::Todo, vec![], vec![]),
        ErrorKind::NotFound,
    );
    assert_eq!(count_events(&f), baseline, "失败的操作不写活动");

    // 历史任务仍可见，负责人标注已归档；岗位文件夹保留且说明标注已归档。
    let detail = f.ts.get_task(&done.task.id).unwrap();
    assert!(detail.owner.archived);
    let brief = read(&f.ws.join("Teamspace/前端开发").join(BRIEF_FILE));
    assert!(brief.contains("- 状态：已归档（"), "{brief}");
    assert!(f.ts.get_role_folder(&fe.id, None).unwrap().exists);
}

/// K-ROLE-008：绑定、换绑、重复绑定幂等、解绑、重复解绑，活动与 previousHarnessName。
#[test]
fn k_role_008_bind_rebind_unbind_are_idempotent_and_logged() {
    let f = plain_fixture();
    let team = f.team("T");
    let fe = f.role(&team.id, "role_frontend");
    let h1 = f.harness(&team.id, "h1", AcpPreset::Mock);
    let h2 = f.harness(&team.id, "h2", AcpPreset::Mock);
    let kinds = |f: &Fixture| -> Vec<EventKind> {
        events(f, &team.id)
            .into_iter()
            .map(|e| e.kind)
            .filter(|k| {
                matches!(
                    k,
                    EventKind::RoleHarnessBound | EventKind::RoleHarnessUnbound
                )
            })
            .collect()
    };

    let v = f.ts.bind_role_harness(&fe.id, &h1.id).unwrap();
    assert_eq!(v.harness.unwrap().name, "h1");
    let v = f.ts.bind_role_harness(&fe.id, &h2.id).unwrap();
    assert_eq!(v.role.harness_id.as_deref(), Some(h2.id.as_str()));
    assert_eq!(kinds(&f).len(), 2);
    // 3) 重复绑定 h2：幂等、不写活动。
    let v = f.ts.bind_role_harness(&fe.id, &h2.id).unwrap();
    assert_eq!(v.role.harness_id.as_deref(), Some(h2.id.as_str()));
    assert_eq!(kinds(&f).len(), 2, "重复绑定不写活动");
    // 4) 解绑。
    let v = f.ts.unbind_role_harness(&fe.id).unwrap();
    assert!(v.harness.is_none() && v.role.harness_id.is_none());
    assert_eq!(kinds(&f).len(), 3);
    // 5) 重复解绑不写活动。
    f.ts.unbind_role_harness(&fe.id).unwrap();
    assert_eq!(kinds(&f).len(), 3);

    let bound: Vec<Event> = events(&f, &team.id)
        .into_iter()
        .filter(|e| {
            matches!(
                e.kind,
                EventKind::RoleHarnessBound | EventKind::RoleHarnessUnbound
            )
        })
        .collect();
    // 最新在前：unbound、bound(h2，换绑)、bound(h1)。
    assert_eq!(bound[0].kind, EventKind::RoleHarnessUnbound);
    assert_eq!(bound[1].kind, EventKind::RoleHarnessBound);
    assert_eq!(bound[1].data["previousHarnessName"], "h1");
    assert_eq!(bound[1].data["harnessName"], "h2");
    assert_eq!(bound[2].kind, EventKind::RoleHarnessBound);
    assert!(bound[2].data["previousHarnessName"].is_null());
}

/// K-ROLE-009：跨团队、已归档的 Harness 或岗位、不存在的 ID 都是 not_found，且无变化。
#[test]
fn k_role_009_binding_boundaries_change_nothing() {
    let f = plain_fixture();
    let t = f.team("T");
    let u = f.team("U");
    let fe = f.role(&t.id, "role_frontend");
    let qa = f.role(&t.id, "role_qa");
    let own = f.harness(&t.id, "own", AcpPreset::Mock);
    let foreign = f.harness(&u.id, "foreign", AcpPreset::Mock);
    let gone = f.harness(&t.id, "gone", AcpPreset::Mock);
    f.ts.archive_harness(&gone.id).unwrap();
    f.ts.archive_role(&qa.id).unwrap();
    f.ts.bind_role_harness(&fe.id, &own.id).unwrap();
    let before = count_events(&f);

    assert_kind(
        f.ts.bind_role_harness(&fe.id, &foreign.id),
        ErrorKind::NotFound,
    );
    assert_kind(
        f.ts.bind_role_harness(&fe.id, &gone.id),
        ErrorKind::NotFound,
    );
    assert_kind(f.ts.bind_role_harness(&qa.id, &own.id), ErrorKind::NotFound);
    assert_kind(
        f.ts.bind_role_harness("no-such-role", &own.id),
        ErrorKind::NotFound,
    );
    assert_kind(
        f.ts.bind_role_harness(&fe.id, "no-such-harness"),
        ErrorKind::NotFound,
    );
    assert_kind(
        f.ts.bind_role_harness("no-such-role", "no-such-harness"),
        ErrorKind::NotFound,
    );

    assert_eq!(
        f.role(&t.id, "role_frontend").harness_id.as_deref(),
        Some(own.id.as_str())
    );
    assert_eq!(count_events(&f), before, "不写活动");
}

// ---------------------------------------------------------------- K-FLD

/// K-FLD-002：岗位名转成安全文件夹名，永远不会逃出 `Teamspace/`。
#[test]
fn k_fld_002_hostile_role_names_stay_inside_teamspace() {
    let f = plain_fixture();
    let team = f.team("T");
    let long = "汉".repeat(100);
    let cases: Vec<(String, &str, Option<String>)> = vec![
        (
            " a/b:c? ".into(),
            "role_n1",
            Some("Teamspace/a_b_c_".into()),
        ),
        ("...".into(), "role_n2", Some("Teamspace/role_n2".into())),
        (
            "../../etc".into(),
            "role_n3",
            Some("Teamspace/_.._etc".into()),
        ),
        ("..".into(), "role_n4", Some("Teamspace/role_n4".into())),
        ("/abs".into(), "role_n5", Some("Teamspace/_abs".into())),
        (".hidden".into(), "role_n6", Some("Teamspace/hidden".into())),
        (long, "role_n7", None),
        (
            "含\t制表符".into(),
            "role_n8",
            Some("Teamspace/含_制表符".into()),
        ),
    ];
    let root = f.ws.join("Teamspace").canonicalize().unwrap();
    for (i, (name, code, expected)) in cases.into_iter().enumerate() {
        let created = add_role(&f, &team.id, new_role(&name, code, &format!("Z{i}"), false))
            .unwrap_or_else(|err| panic!("{name:?}: {err}"));
        let rel = created
            .folder
            .clone()
            .unwrap_or_else(|| panic!("{name:?}: {:?}", created.folder_problem));
        match expected {
            Some(expected) => assert_eq!(rel, expected, "{name:?}"),
            None => {
                let leaf = rel.strip_prefix("Teamspace/").unwrap();
                assert!(leaf.len() <= 150, "{} 字节", leaf.len());
                assert!(leaf.chars().all(|c| c == '汉'), "整字符截断: {leaf}");
            }
        }
        // 路径中不存在独立的 `..` 段、没有多余层级，规范化后位于 Teamspace/ 之内。
        assert!(
            rel.split('/').all(|part| part != ".." && part != "."),
            "{rel}"
        );
        assert_eq!(rel.split('/').count(), 2, "{rel}");
        let dir = rel_dir(&f, &rel);
        assert!(dir.join(BRIEF_FILE).is_file(), "{rel}");
        assert!(dir.canonicalize().unwrap().starts_with(&root), "{rel}");
    }
    // 工作空间之外没有被写入任何东西（`../../etc` 没有逃逸）。
    let parent = f.ws.parent().unwrap();
    let entries: Vec<_> = fs::read_dir(parent)
        .unwrap()
        .flatten()
        .map(|e| e.file_name())
        .collect();
    assert!(
        entries.iter().all(|n| n == "ws"
            || n == "teamspace.db"
            || n.to_string_lossy().starts_with("teamspace.db")),
        "工作空间之外出现了多余条目: {entries:?}"
    );
}

/// K-FLD-003：文件夹重名冲突（用户目录、同名岗位、大小写、符号链接占位）。
#[test]
fn k_fld_003_folder_name_conflicts() {
    let f = plain_fixture();
    // 用户事先放好的目录与符号链接占位。
    f.touch("Teamspace/设计/我的笔记.md");
    let outside = f.dir.path().join("link-target");
    fs::create_dir_all(&outside).unwrap();
    fs::create_dir_all(f.ws.join("Teamspace")).unwrap();
    symlink(&outside, f.ws.join("Teamspace/测试")).unwrap();

    let team = f.team("T"); // 含「前端开发」「QA 测试」等
    let design = add_role(&f, &team.id, new_role("设计", "role_design", "DS", false)).unwrap();
    assert_eq!(
        design.folder.as_deref(),
        Some("Teamspace/设计（role_design）")
    );
    assert_eq!(read(&f.ws.join("Teamspace/设计/我的笔记.md")), "x");
    assert!(!f.ws.join("Teamspace/设计").join(BRIEF_FILE).exists());

    // 同名岗位：第二个加（code），第三个同样加自己的（code）。
    let fe2 = add_role(&f, &team.id, new_role("前端开发", "role_fe2", "F2", false)).unwrap();
    let fe3 = add_role(&f, &team.id, new_role("前端开发", "role_fe3", "F3", false)).unwrap();
    assert_eq!(
        fe2.folder.as_deref(),
        Some("Teamspace/前端开发（role_fe2）")
    );
    assert_eq!(
        fe3.folder.as_deref(),
        Some("Teamspace/前端开发（role_fe3）")
    );

    // 大小写不同仍视为冲突。
    let qa_lower = add_role(&f, &team.id, new_role("qa 测试", "role_x", "QX", false)).unwrap();
    assert_eq!(
        qa_lower.folder.as_deref(),
        Some("Teamspace/qa 测试（role_x）")
    );

    // 二次后缀：同一 code 的候选已被占用时加序号。
    let again = add_role(&f, &team.id, new_role("前端开发", "role_fe2x", "F4", false)).unwrap();
    assert_eq!(
        again.folder.as_deref(),
        Some("Teamspace/前端开发（role_fe2x）")
    );
    // 另一个团队使用 role_frontend 同名同 code：占用 `（role_frontend）`，第三个得到后缀 2。
    let u = f.team("U");
    let u_fe = view(&f, &u.id, "role_frontend");
    assert_eq!(
        u_fe.folder.as_deref(),
        Some("Teamspace/前端开发（role_frontend）")
    );
    let w = f.team("W");
    let w_fe = view(&f, &w.id, "role_frontend");
    assert_eq!(
        w_fe.folder.as_deref(),
        Some("Teamspace/前端开发（role_frontend）2")
    );

    // 符号链接占位的「测试」：不跟随链接写入链接目标，选用 `测试（code）`。
    let test_role = add_role(&f, &team.id, new_role("测试", "role_test", "TS", false)).unwrap();
    assert_eq!(
        test_role.folder.as_deref(),
        Some("Teamspace/测试（role_test）")
    );
    assert!(
        fs::read_dir(&outside).unwrap().next().is_none(),
        "链接目标不应被写入"
    );
    assert!(
        f.ws.join("Teamspace/测试")
            .symlink_metadata()
            .unwrap()
            .file_type()
            .is_symlink()
    );
}

/// K-FLD-005：归档岗位保留文件夹；被用户移走后不重建、不记 role_folder_error。
#[test]
fn k_fld_005_archived_role_folder_is_not_recreated_after_user_moves_it() {
    let f = plain_fixture();
    let team = f.team("T");
    let qa = f.role(&team.id, "role_qa");
    let h = f.harness(&team.id, "h", AcpPreset::Mock);
    f.ts.bind_role_harness(&qa.id, &h.id).unwrap();
    f.ts.archive_role(&qa.id).unwrap();
    let dir = f.ws.join("Teamspace/QA 测试");
    let brief = read(&dir.join(BRIEF_FILE));
    assert!(brief.contains("- 状态：已归档（"), "{brief}");
    assert!(brief.contains("- 当前绑定 Harness：未绑定"), "{brief}");
    assert_eq!(
        journal(&f, "Teamspace/QA 测试").last().unwrap(),
        "归档岗位「QA 测试」"
    );
    let again = add_role(&f, &team.id, new_role("QA 测试", "role_qa2", "QA", false)).unwrap();
    assert_eq!(
        again.folder.as_deref(),
        Some("Teamspace/QA 测试（role_qa2）")
    );

    // 用户把归档岗位的文件夹移出工作空间。
    let moved = f.dir.path().join("moved-QA");
    fs::rename(&dir, &moved).unwrap();
    let errors_before = events(&f, &team.id)
        .iter()
        .filter(|e| e.kind == EventKind::RoleFolderError)
        .count();
    let folder = f.ts.get_role_folder(&qa.id, None).unwrap();
    assert!(!folder.exists);
    f.ts.get_team_overview(&team.id).unwrap();
    assert!(!dir.exists(), "归档岗位的文件夹不会被重新创建");
    let errors_after = events(&f, &team.id)
        .iter()
        .filter(|e| e.kind == EventKind::RoleFolderError)
        .count();
    assert_eq!(errors_before, errors_after, "不记 role_folder_error");
    assert!(moved.join(BRIEF_FILE).is_file());
}

/// K-FLD-007：工作日志只追加；手写行保留；删除后重建。
#[test]
fn k_fld_007_journal_is_append_only_and_rebuilt_when_deleted() {
    let f = plain_fixture();
    let team = f.team("T");
    let fe = f.role(&team.id, "role_frontend");
    let path = f.ws.join("Teamspace/前端开发").join(JOURNAL_FILE);

    // 1) 含换行、回车、制表符的日志条目保持单行。
    let entry = crate::role_folder::JournalEntry {
        at: "2026-10-14T01:02:03.000Z".into(),
        task_key: Some("FE-009".into()),
        text: "第一行\n第二行\r第三行\t第四行".into(),
    };
    crate::role_folder::append_journal(&f.ws.join("Teamspace/前端开发"), &[entry]).unwrap();
    let text = read(&path);
    let last = text.lines().last().unwrap();
    assert!(
        last.contains("FE-009 · 第一行 第二行 第三行 第四行"),
        "{last:?}"
    );
    assert!(!last.contains(['\r', '\t']), "{last:?}");
    assert_eq!(text.lines().filter(|l| l.contains("第一行")).count(), 1);

    // 2) 用户手写一行；新的活动同步后手写行与原有内容保持不变，新条目接在其后。
    let mut handwritten = read(&path);
    handwritten.push_str("- 我手写的一行\n");
    fs::write(&path, &handwritten).unwrap();
    f.task(&team.id, &fe, "新任务");
    f.ts.get_team_overview(&team.id).unwrap();
    let now = read(&path);
    assert!(
        now.starts_with(&handwritten),
        "原有内容与手写行保持不变:\n{now}"
    );
    let tail = &now[handwritten.len()..];
    assert!(tail.contains("创建任务 FE-001「新任务」"), "{tail}");

    // 3) 删除整个文件后再同步：重建为表头加「创建岗位…」行，不报错。
    fs::remove_file(&path).unwrap();
    f.task(&team.id, &fe, "再来一个");
    let overview = f.ts.get_team_overview(&team.id).unwrap();
    let card = overview.roles.iter().find(|v| v.role.id == fe.id).unwrap();
    assert!(card.folder_problem.is_none(), "{:?}", card.folder_problem);
    let rebuilt = read(&path);
    assert!(rebuilt.starts_with("# 工作日志"), "{rebuilt}");
    assert!(
        rebuilt.contains("创建岗位「前端开发」（任务前缀 FE）"),
        "{rebuilt}"
    );
    assert!(rebuilt.contains("创建任务 FE-002「再来一个」"), "{rebuilt}");
}

/// K-FLD-009：读取日志条数的钳制与「最近文件」的筛选规则。
#[test]
fn k_fld_009_journal_limit_clamping_and_recent_files() {
    let f = plain_fixture();
    let team = f.team("T");
    let fe = f.role(&team.id, "role_frontend");
    let dir = f.ws.join("Teamspace/前端开发");
    // 工作日志 >= 250 条（直接追加，之后再同步）。
    let entries: Vec<crate::role_folder::JournalEntry> = (0..260)
        .map(|i| crate::role_folder::JournalEntry {
            at: "2026-10-14T01:02:03.000Z".into(),
            task_key: None,
            text: format!("条目 {i:03}"),
        })
        .collect();
    crate::role_folder::append_journal(&dir, &entries).unwrap();

    assert_eq!(
        f.ts.get_role_folder(&fe.id, None).unwrap().journal.len(),
        20
    );
    let one = f.ts.get_role_folder(&fe.id, Some(0)).unwrap().journal;
    assert_eq!(one.len(), 1, "0 被钳制为 1");
    assert!(one[0].ends_with("条目 259"), "最新的在前: {one:?}");
    assert_eq!(
        f.ts.get_role_folder(&fe.id, Some(500))
            .unwrap()
            .journal
            .len(),
        200
    );
    assert_eq!(
        f.ts.get_role_folder(&fe.id, Some(200))
            .unwrap()
            .journal
            .len(),
        200
    );
    let top = f.ts.get_role_folder(&fe.id, None).unwrap().journal;
    assert!(
        top[0].ends_with("条目 259") && top[19].ends_with("条目 240"),
        "{top:?}"
    );

    // 最近文件：隐藏项、子目录、符号链接、README、最多 10 个、按修改时间倒序且同时间按路径升序。
    let process = dir.join(PROCESS_DIR);
    let set_mtime = |p: &Path, secs: u64| {
        fs::File::options()
            .write(true)
            .open(p)
            .unwrap()
            .set_modified(std::time::UNIX_EPOCH + std::time::Duration::from_secs(secs))
            .unwrap();
    };
    fs::create_dir_all(process.join("子目录")).unwrap();
    fs::create_dir_all(process.join(".隐藏目录")).unwrap();
    for i in 0..15u64 {
        let p = process.join(format!("f{i:02}.md"));
        fs::write(&p, "x").unwrap();
        set_mtime(&p, 1_000 + i);
    }
    // 两个同时间的文件按路径升序。
    for name in ["子目录/b.md", "子目录/a.md"] {
        let p = process.join(name);
        fs::write(&p, "x").unwrap();
        set_mtime(&p, 5_000);
    }
    fs::write(process.join(".hidden.md"), "x").unwrap();
    set_mtime(&process.join(".hidden.md"), 9_000);
    fs::write(process.join(".隐藏目录/x.md"), "x").unwrap();
    set_mtime(&process.join(".隐藏目录/x.md"), 9_000);
    set_mtime(&process.join("README.md"), 9_000);
    symlink(process.join("f00.md"), process.join("link.md")).unwrap();
    let wb = f.ts.get_role_workbench(&fe.id).unwrap();
    let names: Vec<_> = wb
        .recent_files
        .iter()
        .map(|e| e.relative_path.as_str())
        .collect();
    assert_eq!(names.len(), 10, "{names:?}");
    assert_eq!(
        &names[..2],
        ["过程文件/子目录/a.md", "过程文件/子目录/b.md"],
        "{names:?}"
    );
    assert_eq!(names[2], "过程文件/f14.md");
    assert!(
        names.iter().all(|n| !n.contains("hidden")
            && !n.contains("隐藏")
            && !n.ends_with("link.md")
            && *n != "过程文件/README.md"),
        "{names:?}"
    );
}

// ---------------------------------------------------------------- K-HAR

fn harness_input(team_id: &str, name: &str, command: Option<&str>) -> CreateHarnessInput {
    CreateHarnessInput {
        team_id: team_id.into(),
        name: name.into(),
        preset: AcpPreset::Custom,
        command: command.map(Into::into),
        args: None,
        env_profile: None,
    }
}

/// 假探测的包装：`claude` / `claude-code-acp` 是否存在由测试决定，其余沿用 [`FakeProbe`]。
struct ClaudeProbe {
    adapter: bool,
    cli: bool,
}

impl SystemProbe for ClaudeProbe {
    fn inspect_workspace(&self, path: &Path) -> Result<WorkspaceInfo> {
        FakeProbe.inspect_workspace(path)
    }
    fn search_files(&self, workspace: &Path, query: &str, limit: usize) -> Result<Vec<String>> {
        FakeProbe.search_files(workspace, query, limit)
    }
    fn locate_command(&self, command: &str) -> Option<String> {
        match command {
            "claude-code-acp" => self
                .adapter
                .then(|| "/usr/local/bin/claude-code-acp".to_owned()),
            "claude" => self.cli.then(|| "/usr/local/bin/claude".to_owned()),
            other => FakeProbe.locate_command(other),
        }
    }
    fn handshake(&self, launch: &AgentLaunch) -> Handshake {
        FakeProbe.handshake(launch)
    }
    fn reveal_path(&self, path: &Path) -> Result<()> {
        FakeProbe.reveal_path(path)
    }
}

/// K-HAR-002：自定义实例命令、参数与环境配置档案的全部校验（补 1001 字符边界）。
#[test]
fn k_har_002_custom_command_args_and_env_profile_validation() {
    let f = plain_fixture();
    let team = f.team("T");
    let make = |name: &str, command: Option<&str>| {
        f.ts.create_harness(harness_input(&team.id, name, command))
    };

    let msg = assert_kind(make("a", None), ErrorKind::Invalid);
    assert!(msg.contains("需要填写启动命令"), "{msg}");
    assert_kind(make("a", Some("   ")), ErrorKind::Invalid);
    assert_kind(make("a", Some("bin/agent")), ErrorKind::Invalid);
    let msg = assert_kind(make("a", Some("mcode acp")), ErrorKind::Invalid);
    assert!(msg.contains("参数请填在「参数」里"), "{msg}");
    assert_kind(make("a", Some("agent\u{7}")), ErrorKind::Invalid);
    // 命令长度：1000 合法、1001 非法。
    assert_kind(make("a", Some(&"c".repeat(1001))), ErrorKind::Invalid);
    assert_eq!(
        make("long", Some(&"c".repeat(1000))).unwrap().command.len(),
        1000
    );
    assert_eq!(
        make("abs", Some("/Applications/My Agent/agent"))
            .unwrap()
            .command,
        "/Applications/My Agent/agent"
    );
    assert_eq!(make("plain", Some("my-agent")).unwrap().command, "my-agent");

    let with_args = |name: &str, args: Vec<String>, env: Option<&str>| {
        f.ts.create_harness(CreateHarnessInput {
            args: Some(args),
            env_profile: env.map(Into::into),
            ..harness_input(&team.id, name, Some("my-agent"))
        })
    };
    assert_kind(
        with_args("b1", vec!["a".into(); 65], None),
        ErrorKind::Invalid,
    );
    assert_kind(
        with_args("b2", vec!["a\nb".into()], None),
        ErrorKind::Invalid,
    );
    assert_kind(
        with_args("b3", vec!["x".repeat(1001)], None),
        ErrorKind::Invalid,
    );
    with_args("b4", vec!["x".repeat(1000)], None).unwrap();
    with_args("b5", vec!["a".into(); 64], None).unwrap();
    let msg = assert_kind(with_args("b6", vec![], Some("~/x.env")), ErrorKind::Invalid);
    assert!(msg.contains("须为绝对路径"), "{msg}");
    with_args("b7", vec![], Some("/etc/agent.env")).unwrap();
}

/// K-HAR-004：握手成功持久化，`updatedAt` 不变，活动摘要正确。
#[test]
fn k_har_004_check_persists_handshake_and_logs_activity() {
    let f = plain_fixture();
    let team = f.team("T");
    let h = f.harness(&team.id, "MiniMax", AcpPreset::MinimaxCode);
    let checked = f.ts.check_harness(&h.id).unwrap();
    let hs = &checked.handshake;
    assert_eq!(hs.status, HandshakeStatus::Connected);
    assert_eq!(hs.protocol_version, Some(1));
    let agent = hs.agent.as_ref().unwrap();
    assert!(!agent.name.is_empty());
    assert_eq!(agent.version.as_deref(), Some("0.9.2"));
    assert!(hs.capabilities.as_ref().unwrap().load_session);
    assert!(hs.checked_at.is_some());
    assert_eq!(checked.updated_at, h.updated_at);
    assert_eq!(f.ts.get_harness(&h.id).unwrap().handshake, *hs);
    assert_eq!(f.ts.list_harnesses(&team.id).unwrap()[0].handshake, *hs);
    let top = &events(&f, &team.id)[0];
    assert_eq!(top.kind, EventKind::HarnessChecked);
    assert!(
        top.summary.contains("已连接（mcode 0.9.2）"),
        "{}",
        top.summary
    );
}

/// K-HAR-006：编辑实例重置握手、预设不可改、活动 `harness_updated`、参数回到预设默认。
#[test]
fn k_har_006_editing_resets_handshake_and_logs_update() {
    let f = plain_fixture();
    let team = f.team("T");
    let h =
        f.ts.create_harness(CreateHarnessInput {
            args: Some(vec!["--custom".into()]),
            ..harness_input(&team.id, "a", None).with_preset(AcpPreset::MinimaxCode)
        })
        .unwrap();
    assert_eq!(h.args, ["--custom"]);
    f.ts.check_harness(&h.id).unwrap();
    assert_eq!(
        f.ts.get_harness(&h.id).unwrap().handshake.status,
        HandshakeStatus::Connected
    );

    let before = count_events(&f);
    let input = UpdateHarnessInput {
        harness_id: h.id.clone(),
        name: "改名后".into(),
        command: Some("/opt/bin/mcode".into()),
        args: None,
        env_profile: None,
    };
    // 输入类型没有 preset 字段（编译期保证：下面的解构穷尽全部字段）。
    let UpdateHarnessInput {
        harness_id: _,
        name: _,
        command: _,
        args: _,
        env_profile: _,
    } = input.clone();
    let updated = f.ts.update_harness(input).unwrap();
    assert_eq!(updated.preset, AcpPreset::MinimaxCode);
    assert_eq!(updated.name, "改名后");
    assert_eq!(updated.command, "/opt/bin/mcode");
    assert_eq!(updated.args, ["acp"], "不传参数时回到预设默认");
    let hs = &updated.handshake;
    assert_eq!(
        *hs,
        Handshake::unknown(),
        "握手恢复为 unknown 并清空 detail、agent、capabilities"
    );
    assert!(hs.detail.is_none() && hs.agent.is_none() && hs.capabilities.is_none());
    assert_eq!(count_events(&f), before + 1);
    let top = &events(&f, &team.id)[0];
    assert_eq!(top.kind, EventKind::HarnessUpdated);
    assert_eq!(top.summary, "编辑 Harness「改名后」");
}

trait WithPreset {
    fn with_preset(self, preset: AcpPreset) -> Self;
}

impl WithPreset for CreateHarnessInput {
    fn with_preset(self, preset: AcpPreset) -> Self {
        Self { preset, ..self }
    }
}

/// K-HAR-007：被绑定的实例不能归档；没有删除入口；历史执行保留快照。
#[test]
fn k_har_007_bound_harness_is_protected_and_history_keeps_snapshot() {
    let f = plain_fixture();
    let team = f.team("T");
    let fe = f.role(&team.id, "role_frontend");
    let qa = f.role(&team.id, "role_qa");
    let h =
        f.ts.create_harness(CreateHarnessInput {
            args: Some(vec!["--flag".into()]),
            ..harness_input(&team.id, "历史实例", Some("/opt/bin/agent-x"))
        })
        .unwrap();
    f.ts.bind_role_harness(&fe.id, &h.id).unwrap();
    f.ts.bind_role_harness(&qa.id, &h.id).unwrap();

    let msg = assert_kind(f.ts.archive_harness(&h.id), ErrorKind::Conflict);
    assert!(
        msg.contains("「前端开发」") && msg.contains("「QA 测试」"),
        "{msg}"
    );

    // 产生一次历史执行（启动命令不存在，立即失败，但执行记录与快照已写入）。
    let task = f.readyable(&team.id, &fe, "历史任务");
    f.move_to(&task.task.id, TaskStatus::Ready).unwrap();
    let run = f.ts.start_run(&task.task.id).unwrap();
    let run = {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(20);
        loop {
            let r = f.ts.get_run(&run.id).unwrap();
            if !r.status.is_active() || std::time::Instant::now() > deadline {
                break r;
            }
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
    };

    f.ts.unbind_role_harness(&fe.id).unwrap();
    f.ts.unbind_role_harness(&qa.id).unwrap();
    let archived = f.ts.archive_harness(&h.id).unwrap();
    assert!(archived.archived_at.is_some());
    assert_kind(f.ts.get_harness(&h.id), ErrorKind::NotFound);
    assert_kind(f.ts.check_harness(&h.id), ErrorKind::NotFound);
    assert_kind(f.ts.archive_harness(&h.id), ErrorKind::NotFound);

    let again = f.ts.get_run(&run.id).unwrap();
    assert_eq!(again.harness.name, "历史实例");
    assert_eq!(again.harness.command, "/opt/bin/agent-x");
    assert_eq!(again.harness.args, ["--flag"]);
    assert_eq!(again.harness.harness_id, h.id);
    let listed = f.ts.list_runs(&task.task.id).unwrap();
    assert_eq!(listed[0].harness.name, "历史实例");

    // 只被已归档岗位绑定的实例可以直接归档。
    let pm = f.role(&team.id, "role_product");
    let h2 = f.harness(&team.id, "仅归档岗位", AcpPreset::Mock);
    f.ts.bind_role_harness(&pm.id, &h2.id).unwrap();
    f.ts.archive_role(&pm.id).unwrap();
    f.ts.archive_harness(&h2.id).unwrap();

    // 没有删除 Harness 的命令（核心对象只归档）。
    assert!(crate::api::COMMANDS.contains(&"archive_harness"));
    assert!(
        crate::api::COMMANDS
            .iter()
            .all(|c| !c.starts_with("delete_") && !c.starts_with("remove_")),
        "{:?}",
        crate::api::COMMANDS
    );
}

/// K-HAR-011：Claude Code 预设对适配器与 claude CLI 缺失的三种组合给出正确提示。
#[test]
fn k_har_011_claude_preset_hints_for_missing_adapter_and_cli() {
    let hint_for = |adapter: bool, cli: bool| {
        let dir = tempfile::tempdir().unwrap();
        let ts = Teamspace::open_with(
            &dir.path().join("t.db"),
            Box::new(ClaudeProbe { adapter, cli }),
            HostConfig::default(),
        )
        .unwrap();
        let presets = ts.list_acp_presets();
        presets
            .into_iter()
            .find(|p| p.preset == AcpPreset::ClaudeCode)
            .unwrap()
    };
    const CLI: &str = "需要安装 Claude Code（claude 命令）并完成登录";
    const ADAPTER: &str = "需要安装 Claude Code ACP 适配器（npm 包 @zed-industries/claude-code-acp，提供 claude-code-acp 命令）";

    // A) 适配器在、claude 不在。
    let a = hint_for(true, false);
    assert!(a.command_found);
    assert_eq!(a.cli_found, Some(false));
    assert_eq!(a.hint.as_deref(), Some(CLI));
    assert!(!a.hint.as_deref().unwrap().contains("适配器"));
    // B) 适配器不在、claude 在。
    let b = hint_for(false, true);
    assert!(!b.command_found);
    assert_eq!(b.cli_found, Some(true));
    assert_eq!(b.hint.as_deref(), Some(ADAPTER));
    // C) 两者都不在：两条提示用「；」连接，适配器在前。
    let c = hint_for(false, false);
    assert!(!c.command_found);
    assert_eq!(c.cli_found, Some(false));
    assert_eq!(c.hint, Some(format!("{ADAPTER}；{CLI}")));
    // 两者都在：无提示。
    let d = hint_for(true, true);
    assert_eq!(d.hint, None);
}

// ---------------------------------------------------------------- K-TASK / K-DEP

fn edit_task(
    f: &Fixture,
    detail: &TaskDetail,
    title: &str,
    owner: &str,
    refs: Vec<&str>,
    deps: Vec<String>,
) -> Result<TaskDetail> {
    f.ts.update_task(UpdateTaskInput {
        task_id: detail.task.id.clone(),
        title: title.into(),
        description: detail.task.description.clone(),
        owner_role_id: owner.into(),
        file_refs: refs.into_iter().map(file_ref).collect(),
        depends_on: deps,
    })
}

/// K-TASK-001：序号超过 999 时不截断、不重复；团队间独立。
#[test]
fn k_task_001_key_sequence_grows_past_999() {
    let f = plain_fixture();
    let team = f.team("T");
    let fe = f.role(&team.id, "role_frontend");
    let be = f.role(&team.id, "role_backend");
    assert_eq!(f.task(&team.id, &fe, "a").task.key, "FE-001");
    assert_eq!(f.task(&team.id, &fe, "b").task.key, "FE-002");
    assert_eq!(f.task(&team.id, &be, "c").task.key, "BE-001");
    let other = f.team("U");
    let ofe = f.role(&other.id, "role_frontend");
    assert_eq!(f.task(&other.id, &ofe, "d").task.key, "FE-001");

    f.raw_sql(&format!(
        "UPDATE task_sequences SET last = 998 WHERE team_id = '{}' AND prefix = 'FE'",
        team.id
    ));
    assert_eq!(f.task(&team.id, &fe, "e").task.key, "FE-999");
    assert_eq!(f.task(&team.id, &fe, "f").task.key, "FE-1000");
    assert_eq!(f.task(&team.id, &fe, "g").task.key, "FE-1001");
    // 其他团队、其他前缀不受影响。
    assert_eq!(f.task(&other.id, &ofe, "h").task.key, "FE-002");
    assert_eq!(f.task(&team.id, &be, "i").task.key, "BE-002");
}

/// K-TASK-005：非法流转的文案，且不写活动、不改状态。
#[test]
fn k_task_005_illegal_transitions_have_exact_message_and_leave_no_trace() {
    let f = plain_fixture();
    let team = f.team("T");
    let fe = f.role(&team.id, "role_frontend");
    let t = f.task(&team.id, &fe, "a");
    let before = count_events(&f);

    let msg = assert_kind(f.move_to(&t.task.id, TaskStatus::Done), ErrorKind::Conflict);
    assert_eq!(msg, "任务 FE-001 不能从「TODO」变为「已完成」");
    assert_eq!(
        f.ts.get_task(&t.task.id).unwrap().task.status,
        TaskStatus::Todo
    );
    assert_eq!(count_events(&f), before);

    f.drive(
        &t.task.id,
        &[
            TaskStatus::Ready,
            TaskStatus::Doing,
            TaskStatus::Review,
            TaskStatus::Done,
        ],
    );
    let before = count_events(&f);
    let msg = assert_kind(f.move_to(&t.task.id, TaskStatus::Todo), ErrorKind::Conflict);
    assert_eq!(msg, "任务 FE-001 不能从「已完成」变为「TODO」");
    assert_eq!(
        f.ts.get_task(&t.task.id).unwrap().task.status,
        TaskStatus::Done
    );
    assert_eq!(count_events(&f), before);
}

/// K-TASK-008：引用必须存在（文件或目录），锚点不参与判断。
#[test]
fn k_task_008_refs_must_exist_files_or_directories() {
    let f = plain_fixture();
    let team = f.team("T");
    let fe = f.role(&team.id, "role_frontend");
    let t = f
        .try_task(
            &team.id,
            &fe,
            "引用",
            TaskStatus::Todo,
            vec!["docs/spec.md"],
            vec![],
        )
        .unwrap();
    let msg = assert_kind(
        f.move_to(&t.task.id, TaskStatus::Ready),
        ErrorKind::Conflict,
    );
    assert!(msg.starts_with("任务尚未就绪："), "{msg}");
    assert!(
        msg.contains("引用的文件在工作区中不存在：docs/spec.md"),
        "{msg}"
    );
    assert_eq!(
        f.ts.get_task(&t.task.id).unwrap().task.status,
        TaskStatus::Todo
    );

    f.touch("docs/spec.md");
    let ready = f.move_to(&t.task.id, TaskStatus::Ready).unwrap();
    assert!(ready.file_refs[0].exists);

    // 引用目录视为存在。
    fs::create_dir_all(f.ws.join("assets")).unwrap();
    let dir = f
        .try_task(
            &team.id,
            &fe,
            "目录",
            TaskStatus::Todo,
            vec!["assets"],
            vec![],
        )
        .unwrap();
    assert!(dir.file_refs[0].exists, "目录存在");
    assert!(dir.readiness.is_empty(), "{:?}", dir.readiness);
    let ready = f.move_to(&dir.task.id, TaskStatus::Ready).unwrap();
    assert!(ready.file_refs[0].exists);

    // 锚点不参与存在性判断。
    let anchor = f
        .try_task(
            &team.id,
            &fe,
            "锚点",
            TaskStatus::Todo,
            vec!["docs/spec.md#标题"],
            vec![],
        )
        .unwrap();
    assert_eq!(anchor.task.file_refs[0].path, "docs/spec.md");
    assert_eq!(anchor.task.file_refs[0].anchor.as_deref(), Some("标题"));
    assert!(anchor.file_refs[0].exists);
    f.move_to(&anchor.task.id, TaskStatus::Ready).unwrap();
    let missing = f
        .try_task(
            &team.id,
            &fe,
            "缺",
            TaskStatus::Todo,
            vec!["docs/none.md#标题"],
            vec![],
        )
        .unwrap();
    assert!(missing.readiness[0].contains("docs/none.md"));
}

/// K-TASK-009：已取消的依赖同样不满足 READY；`openDependencyCount` 随依赖状态变化。
#[test]
fn k_task_009_cancelled_dependency_does_not_satisfy_ready() {
    use TaskStatus::*;
    let f = plain_fixture();
    let team = f.team("T");
    let fe = f.role(&team.id, "role_frontend");
    let a = f.readyable(&team.id, &fe, "A");
    let c = f.readyable(&team.id, &fe, "C");
    let b = f
        .try_task(
            &team.id,
            &fe,
            "B",
            Todo,
            vec![],
            vec![a.task.id.clone(), c.task.id.clone()],
        )
        .unwrap();
    let open = |f: &Fixture| -> u32 {
        f.ts.get_task_board(TaskBoardQuery {
            team_id: team.id.clone(),
            include_cancelled: Some(true),
        })
        .unwrap()
        .columns
        .into_iter()
        .flat_map(|c| c.tasks)
        .find(|t| t.id == b.task.id)
        .unwrap()
        .open_dependency_count
    };
    assert_eq!(open(&f), 2);
    let msg = assert_kind(f.move_to(&b.task.id, Ready), ErrorKind::Conflict);
    assert!(msg.contains("依赖任务 FE-001 尚未完成（TODO）"), "{msg}");
    assert!(msg.contains("依赖任务 FE-002 尚未完成（TODO）"), "{msg}");

    f.drive(&a.task.id, &[Ready, Doing, Review, Done]);
    assert_eq!(open(&f), 1);
    f.drive(&c.task.id, &[Cancelled]);
    assert_eq!(open(&f), 1, "已取消的依赖仍算未完成");
    let msg = assert_kind(f.move_to(&b.task.id, Ready), ErrorKind::Conflict);
    assert!(msg.contains("依赖任务 FE-002 尚未完成（已取消）"), "{msg}");
    assert!(!msg.contains("FE-001"), "已完成的依赖不应出现: {msg}");

    // 重开并完成后可以就绪。
    f.drive(&c.task.id, &[Todo, Ready, Doing, Review, Done]);
    assert_eq!(open(&f), 0);
    assert_eq!(f.ts.get_task(&b.task.id).unwrap().task.status, Ready);
}

/// K-TASK-010：READY 之后环境变化（引用被删、工作空间缺失、负责岗位归档）。
#[test]
fn k_task_010_environment_changes_after_ready() {
    use TaskStatus::*;
    let f = plain_fixture();
    let team = f.team("T");
    let fe = f.role(&team.id, "role_frontend");
    let t = f.readyable(&team.id, &fe, "a");
    f.move_to(&t.task.id, Ready).unwrap();

    // 1) 引用被删：readiness 含缺失路径，DOING 不在可选流转中，被拒。
    fs::remove_file(f.ws.join("docs/spec.md")).unwrap();
    let d = f.ts.get_task(&t.task.id).unwrap();
    assert_eq!(d.task.status, Ready);
    assert!(
        d.readiness.iter().any(|r| r.contains("docs/spec.md")),
        "{:?}",
        d.readiness
    );
    assert!(
        !d.allowed_transitions.contains(&Doing),
        "{:?}",
        d.allowed_transitions
    );
    assert_kind(f.move_to(&t.task.id, Doing), ErrorKind::Conflict);

    // 2) 工作空间整个被删：readiness 恰一条，所有引用 exists=false。
    f.touch("docs/spec.md");
    assert!(f.ts.get_task(&t.task.id).unwrap().readiness.is_empty());
    fs::remove_dir_all(&f.ws).unwrap();
    let d = f.ts.get_task(&t.task.id).unwrap();
    assert_eq!(d.readiness, ["工作区目录不存在或已移动"]);
    assert!(d.file_refs.iter().all(|r| !r.exists));
    fs::create_dir_all(&f.ws).unwrap();

    // 3) DONE 任务的负责岗位已归档：DONE→READY 被拒。
    let other = f.readyable(&team.id, &fe, "b");
    f.drive(&other.task.id, &[Ready, Doing, Review, Done]);
    f.drive(&t.task.id, &[Cancelled]);
    f.ts.archive_role(&fe.id).unwrap();
    let msg = assert_kind(f.move_to(&other.task.id, Ready), ErrorKind::Conflict);
    assert!(msg.contains("负责岗位「前端开发」已归档"), "{msg}");
}

/// K-TASK-013：看板列顺序、CANCELLED 默认不返回、列内按更新时间倒序、摘要字段。
#[test]
fn k_task_013_board_order_and_per_column_recency() {
    use TaskStatus::*;
    let f = plain_fixture();
    let team = f.team("T");
    let fe = f.role(&team.id, "role_frontend");
    let pause = || std::thread::sleep(std::time::Duration::from_millis(5));
    let t1 = f
        .try_task(
            &team.id,
            &fe,
            "T1",
            Todo,
            vec!["docs/a.md", "docs/b.md"],
            vec![],
        )
        .unwrap();
    pause();
    let t2 = f.task(&team.id, &fe, "T2");
    pause();
    let t3 = f.task(&team.id, &fe, "T3");
    pause();
    // 每一列至少一个任务。
    let ready = f.readyable(&team.id, &fe, "R1");
    f.move_to(&ready.task.id, Ready).unwrap();
    let doing = f.readyable(&team.id, &fe, "D1");
    f.drive(&doing.task.id, &[Ready, Doing]);
    let blocked = f.readyable(&team.id, &fe, "B1");
    f.drive(&blocked.task.id, &[Ready, Doing, Blocked]);
    let review = f.readyable(&team.id, &fe, "V1");
    f.drive(&review.task.id, &[Ready, Doing, Review]);
    let done = f.readyable(&team.id, &fe, "N1");
    f.drive(&done.task.id, &[Ready, Doing, Review, Done]);
    let cancelled = f.task(&team.id, &fe, "X1");
    f.drive(&cancelled.task.id, &[Cancelled]);
    pause();
    // 最早创建的 T1 被编辑后成为 TODO 列里最近更新的。
    edit_task(
        &f,
        &t1,
        "T1（改）",
        &fe.id,
        vec!["docs/a.md", "docs/b.md"],
        vec![],
    )
    .unwrap();

    let board =
        f.ts.get_task_board(TaskBoardQuery {
            team_id: team.id.clone(),
            include_cancelled: None,
        })
        .unwrap();
    let order: Vec<_> = board.columns.iter().map(|c| c.status).collect();
    assert_eq!(
        order,
        [Todo, Ready, Doing, Blocked, Review, Done],
        "默认没有已取消列"
    );
    let todo: Vec<_> = board.columns[0]
        .tasks
        .iter()
        .map(|t| t.title.as_str())
        .collect();
    assert_eq!(todo, ["T1（改）", "T3", "T2"], "列内按更新时间倒序");
    assert_eq!(board.columns[0].tasks[0].file_ref_count, 2);
    assert_eq!(board.columns[0].tasks[0].dependency_count, 0);
    assert_eq!(board.columns[2].label, "执行中");
    let _ = t2;
    let _ = t3;

    // 全部状态都返回（空列也返回）。
    let empty = f.team("空");
    let empty_board =
        f.ts.get_task_board(TaskBoardQuery {
            team_id: empty.id.clone(),
            include_cancelled: None,
        })
        .unwrap();
    assert_eq!(empty_board.columns.len(), 6);
    assert!(empty_board.columns.iter().all(|c| c.tasks.is_empty()));

    let full =
        f.ts.get_task_board(TaskBoardQuery {
            team_id: team.id.clone(),
            include_cancelled: Some(true),
        })
        .unwrap();
    let order: Vec<_> = full.columns.iter().map(|c| c.status).collect();
    assert_eq!(
        order,
        [Todo, Ready, Doing, Blocked, Review, Done, Cancelled]
    );
    assert_eq!(full.columns[6].label, "已取消");
    assert_eq!(full.columns[6].tasks[0].title, "X1");
    assert_eq!(
        full.columns[3].tasks[0].blocked_reason.as_deref(),
        Some("等待接口")
    );
}

fn insert_run_row(f: &Fixture, task: &Task, harness: &HarnessInstance, status: &str) {
    f.raw_sql(&format!(
        "UPDATE tasks SET status = 'doing' WHERE id = '{task_id}';
         INSERT INTO runs (id, team_id, task_id, role_id, harness_id, harness_name, harness_preset,
                           harness_command, attempt, status, session_id, workspace_path, exec_dir,
                           isolation, capabilities, started_at)
         VALUES ('run-{task_id}', '{team}', '{task_id}', '{role}', '{h}', 'Mock', 'mock', 'x', 1,
                 '{status}', 's-1', '{ws}', '{ws}', 'in_place',
                 '{{\"loadSession\":true,\"image\":false,\"audio\":false,\"embeddedContext\":false,\"mcpHttp\":false,\"mcpSse\":false}}',
                 '2026-10-13T00:00:00.000Z');",
        task_id = task.id,
        team = task.team_id,
        role = task.owner_role_id,
        h = harness.id,
        ws = f.ws.display(),
    ));
}

/// K-TASK-015：queued / running / waiting_permission 三种活动执行都禁止编辑。
#[test]
fn k_task_015_editing_is_refused_for_every_active_run_status() {
    let f = plain_fixture();
    let team = f.team("T");
    let fe = f.role(&team.id, "role_frontend");
    let be = f.role(&team.id, "role_backend");
    let h = f.harness(&team.id, "Mock", AcpPreset::Mock);
    f.ts.bind_role_harness(&fe.id, &h.id).unwrap();
    let dep = f.task(&team.id, &fe, "上游");
    let task = f.readyable(&team.id, &fe, "目标");
    insert_run_row(&f, &task.task, &h, "queued");
    let before = count_events(&f);

    for status in ["queued", "running", "waiting_permission"] {
        f.raw_sql(&format!(
            "UPDATE runs SET status = '{status}' WHERE task_id = '{}'",
            task.task.id
        ));
        let attempts = [
            edit_task(&f, &task, "改标题", &fe.id, vec!["docs/spec.md"], vec![]),
            edit_task(&f, &task, "目标", &be.id, vec!["docs/spec.md"], vec![]),
            edit_task(
                &f,
                &task,
                "目标",
                &fe.id,
                vec!["docs/spec.md"],
                vec![dep.task.id.clone()],
            ),
        ];
        for attempt in attempts {
            let msg = assert_kind(attempt, ErrorKind::Conflict);
            assert_eq!(msg, "任务正在执行中，结束或取消执行后才能编辑", "{status}");
        }
        let now = f.ts.get_task(&task.task.id).unwrap().task;
        assert_eq!(now.title, "目标", "{status}");
        assert_eq!(now.owner_role_id, fe.id, "{status}");
        assert!(now.depends_on.is_empty(), "{status}");
    }
    assert_eq!(count_events(&f), before, "不写 task_updated 活动");
}

/// K-DEP-002：两个任务互相依赖被拒绝并指出环路。
#[test]
fn k_dep_002_two_task_cycle_is_rejected_with_path() {
    let f = plain_fixture();
    let team = f.team("T");
    let fe = f.role(&team.id, "role_frontend");
    let a = f.task(&team.id, &fe, "A");
    let b = f
        .try_task(
            &team.id,
            &fe,
            "B",
            TaskStatus::Todo,
            vec![],
            vec![a.task.id.clone()],
        )
        .unwrap();
    let before = count_events(&f);
    let msg = assert_kind(
        edit_task(&f, &a, "A", &fe.id, vec![], vec![b.task.id.clone()]),
        ErrorKind::Conflict,
    );
    assert_eq!(msg, "依赖形成环路：FE-001 → FE-002 → FE-001");
    assert!(
        f.ts.get_task(&a.task.id)
            .unwrap()
            .task
            .depends_on
            .is_empty()
    );
    assert_eq!(count_events(&f), before);
}

/// K-DEP-004：长链与分叉图中完整指出环路，不含无关分叉；跨团队依赖被拒。
#[test]
fn k_dep_004_long_chain_cycle_lists_every_node_and_skips_the_branch() {
    let f = plain_fixture();
    let team = f.team("T");
    let fe = f.role(&team.id, "role_frontend");
    let mut chain: Vec<TaskDetail> = Vec::new();
    for i in 1..=6 {
        let deps = chain
            .last()
            .map(|t| vec![t.task.id.clone()])
            .unwrap_or_default();
        chain.push(
            f.try_task(
                &team.id,
                &fe,
                &format!("T{i}"),
                TaskStatus::Todo,
                vec![],
                deps,
            )
            .unwrap(),
        );
    }
    // T3 另有一条无环的分叉依赖 X（放在 T2 之前，检验回溯时 X 不会残留在路径里）。
    let x = f.task(&team.id, &fe, "X");
    let t3 = &chain[2];
    edit_task(
        &f,
        t3,
        "T3",
        &fe.id,
        vec![],
        vec![x.task.id.clone(), chain[1].task.id.clone()],
    )
    .unwrap();
    assert_eq!(x.task.key, "FE-007");

    let msg = assert_kind(
        edit_task(
            &f,
            &chain[0],
            "T1",
            &fe.id,
            vec![],
            vec![chain[5].task.id.clone()],
        ),
        ErrorKind::Conflict,
    );
    assert_eq!(
        msg,
        "依赖形成环路：FE-001 → FE-006 → FE-005 → FE-004 → FE-003 → FE-002 → FE-001"
    );
    assert!(!msg.contains("FE-007"), "{msg}");

    // 环检测只看同团队任务：另一个团队的任务不能作为依赖。
    let other = f.team("U");
    let ofe = f.role(&other.id, "role_frontend");
    let foreign = f.task(&other.id, &ofe, "外部");
    assert_kind(
        edit_task(
            &f,
            &chain[0],
            "T1",
            &fe.id,
            vec![],
            vec![foreign.task.id.clone()],
        ),
        ErrorKind::NotFound,
    );
}

/// K-DEP-006：上游完成后下游自动变 READY，并产生一条系统活动（M3）。
#[test]
fn k_dep_006_downstream_is_promoted_automatically() {
    use TaskStatus::*;
    let f = plain_fixture();
    let team = f.team("T");
    let fe = f.role(&team.id, "role_frontend");
    let a = f.readyable(&team.id, &fe, "A");
    let b = f
        .try_task(&team.id, &fe, "B", Todo, vec![], vec![a.task.id.clone()])
        .unwrap();
    f.drive(&a.task.id, &[Ready, Doing, Review, Done]);
    let before = count_events(&f);
    let b_now = f.ts.get_task(&b.task.id).unwrap();
    assert_eq!(b_now.task.status, Ready);
    assert!(b_now.readiness.is_empty(), "{:?}", b_now.readiness);
    assert_eq!(count_events(&f), before);
    let automatic: Vec<_> = events(&f, &team.id)
        .into_iter()
        .filter(|e| e.actor == Actor::System)
        .collect();
    assert_eq!(automatic.len(), 1);
    assert_eq!(automatic[0].task_id.as_deref(), Some(b.task.id.as_str()));
}

// ---------------------------------------------------------------- K-EVT

/// K-EVT-001：每类变更恰写一条带中文摘要的活动。
#[test]
fn k_evt_001_each_command_writes_exactly_one_event_of_the_right_kind() {
    let f = plain_fixture();
    let mut last = count_events(&f);
    let mut step = |f: &Fixture, team_id: &str, kind: EventKind, label: &str| -> Event {
        let now = count_events(f);
        assert_eq!(now, last + 1, "{label}: 应恰写 1 条活动");
        last = now;
        let top = events(f, team_id).remove(0);
        assert_eq!(top.kind, kind, "{label}: {}", top.summary);
        assert_eq!(top.actor, Actor::Human, "{label}");
        assert_eq!(top.team_id, team_id, "{label}");
        assert!(
            top.summary
                .chars()
                .any(|c| ('\u{4e00}'..='\u{9fff}').contains(&c)),
            "{label}: 摘要应为中文 {}",
            top.summary
        );
        top
    };

    let team = f.team("T");
    let e = step(&f, &team.id, EventKind::TeamCreated, "创建团队");
    assert_eq!(e.summary, "创建团队「T」，含 5 个岗位");

    let created = add_role(&f, &team.id, new_role("设计师", "role_design", "DS", false))
        .unwrap()
        .role;
    let e = step(&f, &team.id, EventKind::RoleCreated, "创建岗位");
    assert_eq!(e.summary, "新增岗位「设计师」（DS）");
    assert_eq!(e.role_id.as_deref(), Some(created.id.as_str()));

    f.ts.update_role(UpdateRoleInput {
        role_id: created.id.clone(),
        name: "设计师".into(),
        task_prefix: "DS".into(),
        description: "做设计".into(),
        guidelines: String::new(),
        is_leader: false,
    })
    .unwrap();
    let e = step(&f, &team.id, EventKind::RoleUpdated, "编辑岗位");
    assert_eq!(e.summary, "编辑岗位「设计师」");
    assert_eq!(e.role_id.as_deref(), Some(created.id.as_str()));

    let h = f.harness(&team.id, "h", AcpPreset::Mock);
    let e = step(&f, &team.id, EventKind::HarnessCreated, "注册 Harness");
    assert_eq!(e.summary, "注册 Harness「h」");

    f.ts.bind_role_harness(&created.id, &h.id).unwrap();
    let e = step(&f, &team.id, EventKind::RoleHarnessBound, "绑定");
    assert_eq!(e.summary, "岗位「设计师」绑定 Harness「h」");
    assert_eq!(e.role_id.as_deref(), Some(created.id.as_str()));

    f.ts.unbind_role_harness(&created.id).unwrap();
    let e = step(&f, &team.id, EventKind::RoleHarnessUnbound, "解绑");
    assert_eq!(e.summary, "岗位「设计师」解除绑定 Harness「h」");

    f.ts.update_harness(UpdateHarnessInput {
        harness_id: h.id.clone(),
        name: "h2".into(),
        command: None,
        args: None,
        env_profile: None,
    })
    .unwrap();
    let e = step(&f, &team.id, EventKind::HarnessUpdated, "编辑 Harness");
    assert_eq!(e.summary, "编辑 Harness「h2」");

    f.ts.check_harness(&h.id).unwrap();
    let e = step(&f, &team.id, EventKind::HarnessChecked, "检查 Harness");
    assert!(
        e.summary.starts_with("Harness「h2」握手检查："),
        "{}",
        e.summary
    );

    f.ts.archive_harness(&h.id).unwrap();
    let e = step(&f, &team.id, EventKind::HarnessArchived, "归档 Harness");
    assert_eq!(e.summary, "归档 Harness「h2」");

    let task = f.task(&team.id, &created, "出图");
    let e = step(&f, &team.id, EventKind::TaskCreated, "创建任务");
    assert_eq!(e.summary, "创建任务 DS-001「出图」，负责岗位「设计师」");
    assert_eq!(e.task_id.as_deref(), Some(task.task.id.as_str()));
    assert_eq!(e.role_id.as_deref(), Some(created.id.as_str()));

    edit_task(&f, &task, "出图（改）", &created.id, vec![], vec![]).unwrap();
    let e = step(&f, &team.id, EventKind::TaskUpdated, "编辑任务");
    assert_eq!(e.summary, "编辑任务 DS-001「出图（改）」");
    assert_eq!(e.task_id.as_deref(), Some(task.task.id.as_str()));

    f.move_to(&task.task.id, TaskStatus::Ready).unwrap();
    let e = step(&f, &team.id, EventKind::TaskStatusChanged, "变更状态");
    assert_eq!(e.summary, "DS-001 状态 TODO → READY");
    assert_eq!(e.task_id.as_deref(), Some(task.task.id.as_str()));

    f.drive(&task.task.id, &[TaskStatus::Cancelled]);
    step(&f, &team.id, EventKind::TaskStatusChanged, "取消任务");
    f.ts.archive_role(&created.id).unwrap();
    let e = step(&f, &team.id, EventKind::RoleArchived, "归档岗位");
    assert_eq!(e.summary, "归档岗位「设计师」");
    assert_eq!(e.role_id.as_deref(), Some(created.id.as_str()));
}

/// K-EVT-002：失败的命令不写活动，活动只追加且序号单调。
#[test]
fn k_evt_002_failed_commands_write_nothing_and_events_are_append_only() {
    let f = plain_fixture();
    let team = f.team("T");
    let fe = f.role(&team.id, "role_frontend");
    let t = f.task(&team.id, &fe, "a");
    let before = count_events(&f);
    assert!(f.move_to(&t.task.id, TaskStatus::Done).is_err());
    assert!(f.ts.archive_role(&fe.id).is_err());
    assert!(
        team_at(&f, "T", &f.ws.display().to_string(), vec![]).is_err(),
        "重名团队"
    );
    assert!(add_role(&f, &team.id, new_role("X", "role_frontend", "XX", false)).is_err());
    assert_eq!(count_events(&f), before);

    // 序号单调递增、与写入顺序一致；活动列表按序号倒序。
    let conn = rusqlite::Connection::open(f.db_path()).unwrap();
    let seqs: Vec<i64> = conn
        .prepare("SELECT seq FROM events ORDER BY rowid")
        .unwrap()
        .query_map([], |row| row.get(0))
        .unwrap()
        .collect::<rusqlite::Result<_>>()
        .unwrap();
    assert!(seqs.windows(2).all(|w| w[0] < w[1]), "{seqs:?}");
    // 同一毫秒内连续写入的活动，ID 也按写入顺序递增。
    for i in 0..30 {
        f.harness(&team.id, &format!("h{i:02}"), AcpPreset::Mock);
    }
    let ids_in_write_order: Vec<String> = conn
        .prepare("SELECT id FROM events ORDER BY seq")
        .unwrap()
        .query_map([], |row| row.get(0))
        .unwrap()
        .collect::<rusqlite::Result<_>>()
        .unwrap();
    assert!(
        ids_in_write_order.windows(2).all(|w| w[0] < w[1]),
        "活动 ID 应随写入顺序单调递增: {ids_in_write_order:?}"
    );
    let listed = events(&f, &team.id);
    let ids: std::collections::HashSet<_> = listed.iter().map(|e| e.id.clone()).collect();
    assert_eq!(ids.len(), listed.len(), "活动 ID 唯一");
    let top = listed[0].id.clone();
    f.move_to(&t.task.id, TaskStatus::Ready).unwrap();
    let newer = events(&f, &team.id);
    assert_eq!(newer[1].id, top, "旧活动保持原位置，新活动只在最前追加");

    // 不存在更新或删除活动的入口。
    assert!(
        crate::api::COMMANDS
            .iter()
            .filter(|c| c.contains("event"))
            .all(|c| *c == "list_events"),
        "{:?}",
        crate::api::COMMANDS
    );
}

/// K-EVT-004（1～3）：Harness 编辑 / 归档与续接执行的活动。
#[test]
fn k_evt_004_harness_and_resume_activities() {
    let f = plain_fixture();
    let team = f.team("T");
    let fe = f.role(&team.id, "role_frontend");
    let h = f.harness(&team.id, "实例", AcpPreset::Mock);
    f.ts.bind_role_harness(&fe.id, &h.id).unwrap();
    let task = f.readyable(&team.id, &fe, "任务");
    insert_run_row(&f, &task.task, &h, "running");

    f.ts.update_harness(UpdateHarnessInput {
        harness_id: h.id.clone(),
        name: "实例B".into(),
        command: None,
        args: None,
        env_profile: None,
    })
    .unwrap();
    assert_eq!(events(&f, &team.id)[0].kind, EventKind::HarnessUpdated);
    assert_eq!(events(&f, &team.id)[0].summary, "编辑 Harness「实例B」");

    // 重新打开数据库：残留的执行被标为 interrupted，再续接。
    let ts =
        Teamspace::open_with(&f.db_path(), Box::new(FakeProbe), HostConfig::default()).unwrap();
    let run_id = format!("run-{}", task.task.id);
    assert_eq!(ts.get_run(&run_id).unwrap().status, RunStatus::Interrupted);
    ts.resume_run(&run_id).unwrap();
    let resumed: Vec<Event> = ts
        .list_events(EventQuery {
            team_id: team.id.clone(),
            cursor: None,
            limit: Some(50),
        })
        .unwrap()
        .events
        .into_iter()
        .filter(|e| e.kind == EventKind::RunResumed)
        .collect();
    assert_eq!(resumed.len(), 1);
    assert_eq!(
        resumed[0].summary,
        format!("{} 续接第 1 次执行", task.task.key)
    );
    assert_eq!(resumed[0].actor, Actor::Human);
    // 等续接的 driver 结束（宿主没有 Mock Agent，启动失败），避免线程悬挂。
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(20);
    while ts.get_run(&run_id).unwrap().status.is_active() && std::time::Instant::now() < deadline {
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    ts.unbind_role_harness(&fe.id).unwrap();
    ts.archive_harness(&h.id).unwrap();
    let top = ts
        .list_events(EventQuery {
            team_id: team.id.clone(),
            cursor: None,
            limit: Some(1),
        })
        .unwrap()
        .events
        .remove(0);
    assert_eq!(top.kind, EventKind::HarnessArchived);
    assert_eq!(top.summary, "归档 Harness「实例B」");
}

/// K-EVT-004（4）/ K-RUN-012（2）：重新执行时旧的中断执行被置为失败，应留下可追溯的活动。
#[test]
#[ignore = "缺陷 K-EVT-004/K-RUN-012（Q-12）: start_run 把中断的旧执行置为 failed 但没有写活动记录，工作日志里看不到"]
fn k_evt_004_rerun_of_interrupted_run_leaves_an_activity_for_the_old_run() {
    let f = plain_fixture();
    let team = f.team("T");
    let fe = f.role(&team.id, "role_frontend");
    let h = f.harness(&team.id, "实例", AcpPreset::Mock);
    f.ts.bind_role_harness(&fe.id, &h.id).unwrap();
    let task = f.readyable(&team.id, &fe, "任务");
    insert_run_row(&f, &task.task, &h, "running");
    let ts =
        Teamspace::open_with(&f.db_path(), Box::new(FakeProbe), HostConfig::default()).unwrap();
    let old = format!("run-{}", task.task.id);
    assert_eq!(ts.get_run(&old).unwrap().status, RunStatus::Interrupted);
    let list = |ts: &Teamspace| {
        ts.list_events(EventQuery {
            team_id: team.id.clone(),
            cursor: None,
            limit: Some(200),
        })
        .unwrap()
        .events
    };
    let known: std::collections::HashSet<String> = list(&ts).into_iter().map(|e| e.id).collect();
    let new_run = ts.start_run(&task.task.id).unwrap();
    assert_eq!(new_run.attempt, 2);
    let old_run = ts.get_run(&old).unwrap();
    assert_eq!(old_run.status, RunStatus::Failed);
    assert_eq!(old_run.failure.as_deref(), Some("执行中断后已重新执行"));
    // 重新执行之后新增的活动里，要有一条以旧执行为对象（把它置为失败）。
    let traceable = list(&ts)
        .into_iter()
        .filter(|e| !known.contains(&e.id))
        .any(|e| e.data["runId"] == old.as_str());
    assert!(traceable, "旧执行被置为失败应有一条以它为对象的活动");
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(20);
    while ts.get_run(&new_run.id).unwrap().status.is_active()
        && std::time::Instant::now() < deadline
    {
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
}

// ---------------------------------------------------------------- K-SEC

/// K-SEC-001（有效部分）：路径穿越、绝对路径、盘符、UNC、空路径被拒。
#[test]
fn k_sec_001_hostile_ref_paths_are_rejected() {
    let f = plain_fixture();
    let team = f.team("T");
    let fe = f.role(&team.id, "role_frontend");
    for bad in [
        "/etc/hosts",
        "C:/x",
        "C:\\x",
        "\\\\server\\share\\a.md",
        "../x.md",
        "a/../../b",
        "..",
        "",
        "   ",
        "#x",
    ] {
        let created = f.try_task(&team.id, &fe, "x", TaskStatus::Todo, vec![bad], vec![]);
        assert_kind(created, ErrorKind::Invalid);
        let t = f.task(&team.id, &fe, "ok");
        assert_kind(
            edit_task(&f, &t, "ok", &fe.id, vec![bad], vec![]),
            ErrorKind::Invalid,
        );
    }
}

/// K-SEC-001（控制字符与长度）：期望 NUL、换行、超长路径与超长锚点被拒绝。
#[test]
#[ignore = "缺陷 K-SEC-001（Q-05）: 文件引用路径与锚点没有控制字符和长度校验，含 NUL、换行、5000 字符的值被接受并写入数据库"]
fn k_sec_001_control_characters_and_overlong_refs_are_rejected() {
    let f = plain_fixture();
    let team = f.team("T");
    let fe = f.role(&team.id, "role_frontend");
    let long_path = "a".repeat(5000);
    let long_anchor = format!("x.md#{}", "b".repeat(5000));
    for bad in [
        "a\u{0}b.md",
        "a\nb.md",
        long_path.as_str(),
        long_anchor.as_str(),
    ] {
        let created = f.try_task(&team.id, &fe, "x", TaskStatus::Todo, vec![bad], vec![]);
        assert_kind(created, ErrorKind::Invalid);
    }
}

/// K-SEC-002：引用指向工作空间外的符号链接。期望：越界链接不就绪；或至少不嵌入说明包、不获得写权限。
#[test]
fn k_sec_002_symlink_outside_workspace_is_not_embedded_or_writable() {
    use crate::model::RunIsolation;
    use crate::run::{brief, policy::Scope};
    let f = plain_fixture();
    let team = f.team("T");
    let fe = f.role(&team.id, "role_frontend");
    let outside = f.dir.path().join("outside");
    fs::create_dir_all(&outside).unwrap();
    let secret = outside.join("secret.txt");
    fs::write(&secret, "TOP-SECRET-CONTENT").unwrap();
    symlink(&secret, f.ws.join("notes.md")).unwrap();

    let task = f
        .try_task(
            &team.id,
            &fe,
            "链接",
            TaskStatus::Todo,
            vec!["notes.md"],
            vec![],
        )
        .unwrap();
    // 越界链接不满足就绪条件，原因为「引用文件不在工作空间内」。
    let err = f
        .move_to(&task.task.id, TaskStatus::Ready)
        .expect_err("越界链接不应就绪");
    assert!(
        err.to_string().contains("引用文件不在工作空间内：notes.md"),
        "{err}"
    );
    // 说明包也不能嵌入链接目标的内容（纵深防御）。
    let link = f.ws.join("notes.md");
    let blocks = brief::prompt_blocks(
        "正文",
        &[brief::BriefRef {
            path: "notes.md".into(),
            anchor: None,
            absolute: link.clone(),
            unavailable: None,
        }],
        true,
        &[f.ws.as_path()],
    );
    let embedded = serde_json::to_string(&blocks).unwrap();
    assert!(
        !embedded.contains("TOP-SECRET-CONTENT"),
        "链接目标内容被嵌入说明包: {embedded}"
    );
    // 且不会获得写权限：链接目标不在自动写入范围内。
    let scope = Scope::new(&f.ws, RunIsolation::InPlace, &[], &[link]);
    assert!(
        scope.can_write(&secret.display().to_string()).is_err(),
        "工作空间外的链接目标不应被自动允许写入"
    );
}

/// K-SEC-003：岗位名里的 `../`、`/`、`..`、NUL 与换行。
#[test]
fn k_sec_003_hostile_role_names_stay_inside_and_nul_is_rejected() {
    let f = plain_fixture();
    let team = f.team("T");
    let root = f.ws.join("Teamspace").canonicalize().unwrap();
    for (i, name) in ["../../x", "a/b", ".."].into_iter().enumerate() {
        let created = add_role(
            &f,
            &team.id,
            new_role(name, &format!("role_s{i}"), &format!("S{i}"), false),
        )
        .unwrap();
        let rel = created.folder.unwrap();
        let dir = rel_dir(&f, &rel);
        assert!(
            dir.canonicalize().unwrap().starts_with(&root),
            "{name}: {rel}"
        );
        let brief = read(&dir.join(BRIEF_FILE));
        assert!(
            brief.lines().next().unwrap().contains("teamspace-role-id:"),
            "{brief}"
        );
        assert!(brief.contains(&format!("# 岗位说明：{name}\n")), "{brief}");
    }
    assert!(!f.ws.parent().unwrap().join("x").exists());
    assert_kind(
        add_role(&f, &team.id, new_role("a\u{0}b", "role_nul", "NU", false)),
        ErrorKind::Invalid,
    );
}

/// K-SEC-003（换行）：期望岗位名含换行被拒，或至少不破坏岗位说明的标题。
#[test]
#[ignore = "缺陷 K-SEC-003（Q-09）: 岗位名允许换行，岗位说明的标题行与「岗位名」行被换行打断"]
fn k_sec_003_newline_in_role_name_does_not_break_the_brief() {
    let f = plain_fixture();
    let team = f.team("T");
    match add_role(&f, &team.id, new_role("甲\n乙", "role_nl", "NL", false)) {
        Err(err) => assert_eq!(err.kind(), ErrorKind::Invalid),
        Ok(created) => {
            let brief = read(&rel_dir(&f, &created.folder.unwrap()).join(BRIEF_FILE));
            let lines: Vec<&str> = brief.lines().collect();
            assert!(lines[0].contains("teamspace-role-id:"), "{brief}");
            assert!(lines[1].starts_with("# 岗位说明：甲"), "{brief}");
            assert!(lines[2].trim().is_empty(), "标题应保持单行: {brief}");
        }
    }
}

/// K-SEC-004：SQL 与 Unicode 特殊字符原样保存。
#[test]
fn k_sec_004_special_characters_are_stored_verbatim() {
    let f = plain_fixture();
    let values = [
        "'; DROP TABLE tasks;--",
        "\"双引号\"",
        "%_",
        "带 emoji 😀🎉 的文字",
        "שלום עולם مرحبا بالعالم",
        "e\u{301}a\u{308}n\u{303}",
    ];
    for (i, value) in values.into_iter().enumerate() {
        let padded = format!("  {value}  ");
        let team = team_at(
            &f,
            &padded,
            &f.ws.display().to_string(),
            vec![new_role(
                &padded,
                &format!("role_u{i}"),
                &format!("U{i}"),
                false,
            )],
        )
        .unwrap();
        assert_eq!(team.name, value);
        let role = f.role(&team.id, &format!("role_u{i}"));
        assert_eq!(role.name, value);
        let h =
            f.ts.create_harness(harness_input(&team.id, &padded, Some("my-agent")))
                .unwrap();
        assert_eq!(h.name, value);
        let task =
            f.ts.create_task(CreateTaskInput {
                team_id: team.id.clone(),
                title: padded.clone(),
                description: padded.clone(),
                owner_role_id: role.id.clone(),
                status: TaskStatus::Todo,
                file_refs: vec![],
                depends_on: vec![],
            })
            .unwrap();
        assert_eq!(task.task.title, value);
        assert_eq!(task.task.description, value);
        f.ts.change_task_status(ChangeTaskStatusInput {
            task_id: task.task.id.clone(),
            to: TaskStatus::Ready,
            reason: None,
        })
        .unwrap();
        f.drive(&task.task.id, &[TaskStatus::Doing]);
        let blocked =
            f.ts.change_task_status(ChangeTaskStatusInput {
                task_id: task.task.id.clone(),
                to: TaskStatus::Blocked,
                reason: Some(padded.clone()),
            })
            .unwrap();
        assert_eq!(blocked.task.blocked_reason.as_deref(), Some(value));

        // 读取与保存逐字一致，活动摘要与工作日志逐字包含该文本。
        let again = f.ts.get_task(&task.task.id).unwrap();
        assert_eq!(again.task.title, value);
        let summaries: Vec<String> = events(&f, &team.id)
            .into_iter()
            .map(|e| e.summary)
            .collect();
        assert!(
            summaries
                .iter()
                .any(|s| s.contains(&format!("「{value}」"))),
            "{summaries:?}"
        );
        assert!(
            summaries
                .iter()
                .any(|s| s.contains("受阻") && s.contains(value)),
            "{summaries:?}"
        );
        let folder = f.ts.get_role_folder(&role.id, Some(50)).unwrap();
        assert!(folder.exists, "{value}: {:?}", folder.problem);
        let joined = folder.journal.join("\n");
        assert!(joined.contains(&format!("「{value}」")), "{joined}");
        assert!(
            joined.contains(&format!("：{value}")),
            "受阻原因逐字进入日志: {joined}"
        );
        let rel = folder.relative_path.unwrap();
        assert_eq!(rel.split('/').count(), 2, "{rel}");
    }
    // 各表仍在、任务数据完整。
    let conn = rusqlite::Connection::open(f.db_path()).unwrap();
    let tables: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name IN
             ('teams','roles','tasks','harnesses','events','runs')",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(tables, 6);
    let tasks: i64 = conn
        .query_row("SELECT COUNT(*) FROM tasks", [], |row| row.get(0))
        .unwrap();
    assert_eq!(tasks, 6);
}

/// K-SEC-005：各字段的长度上限与上限+1。
#[test]
fn k_sec_005_length_limits_for_every_field() {
    let f = plain_fixture();
    let ws = f.ws.display().to_string();
    let n = |c: char, count: usize| c.to_string().repeat(count);

    // 团队：名称 100、描述 20000。
    let team_input = |name: String, description: String| CreateTeamInput {
        name,
        description,
        workspace_path: ws.clone(),
        roles: vec![],
    };
    let events_before = count_events(&f);
    let msg = assert_kind(
        f.ts.create_team(team_input(n('名', 101), String::new())),
        ErrorKind::Invalid,
    );
    assert!(msg.contains("100"), "{msg}");
    let msg = assert_kind(
        f.ts.create_team(team_input("T".into(), n('x', 20_001))),
        ErrorKind::Invalid,
    );
    assert!(msg.contains("20000"), "{msg}");
    assert_eq!(count_events(&f), events_before);
    assert!(f.ts.list_teams().unwrap().is_empty());
    let team =
        f.ts.create_team(team_input(n('名', 100), n('x', 20_000)))
            .unwrap();

    // 岗位：名称 100、描述与约定 20000。
    let mk = |name: String, description: String, guidelines: String, code: &str, prefix: &str| {
        add_role(
            &f,
            &team.id,
            NewRole {
                name,
                code: Some(code.into()),
                task_prefix: prefix.into(),
                description,
                guidelines,
                is_leader: false,
            },
        )
    };
    let before = count_events(&f);
    let msg = assert_kind(
        mk(n('岗', 101), String::new(), String::new(), "role_a1", "AA"),
        ErrorKind::Invalid,
    );
    assert!(msg.contains("100"), "{msg}");
    let msg = assert_kind(
        mk("a".into(), n('x', 20_001), String::new(), "role_a2", "AB"),
        ErrorKind::Invalid,
    );
    assert!(msg.contains("20000"), "{msg}");
    let msg = assert_kind(
        mk("a".into(), String::new(), n('x', 20_001), "role_a3", "AC"),
        ErrorKind::Invalid,
    );
    assert!(msg.contains("20000"), "{msg}");
    assert_eq!(count_events(&f), before);
    assert_eq!(
        f.ts.list_roles(&team.id).unwrap().len(),
        0,
        "失败的创建不留半成品"
    );
    let role = mk(
        n('岗', 100),
        n('x', 20_000),
        n('x', 20_000),
        "role_ok",
        "OK",
    )
    .unwrap()
    .role;

    // Harness：名称 100、命令 1000、参数 1000 与 64 个。
    let before = count_events(&f);
    let msg = assert_kind(
        f.ts.create_harness(harness_input(&team.id, &n('h', 101), Some("a"))),
        ErrorKind::Invalid,
    );
    assert!(msg.contains("100"), "{msg}");
    let msg = assert_kind(
        f.ts.create_harness(harness_input(&team.id, "c", Some(&n('c', 1001)))),
        ErrorKind::Invalid,
    );
    assert!(msg.contains("1000"), "{msg}");
    let msg = assert_kind(
        f.ts.create_harness(CreateHarnessInput {
            args: Some(vec![n('p', 1001)]),
            ..harness_input(&team.id, "p", Some("a"))
        }),
        ErrorKind::Invalid,
    );
    assert!(msg.contains("1000"), "{msg}");
    let msg = assert_kind(
        f.ts.create_harness(CreateHarnessInput {
            args: Some(vec!["a".into(); 65]),
            ..harness_input(&team.id, "q", Some("a"))
        }),
        ErrorKind::Invalid,
    );
    assert!(msg.contains("64"), "{msg}");
    assert_eq!(count_events(&f), before);
    assert!(f.ts.list_harnesses(&team.id).unwrap().is_empty());
    f.ts.create_harness(harness_input(&team.id, &n('h', 100), Some(&n('c', 1000))))
        .unwrap();
    f.ts.create_harness(CreateHarnessInput {
        args: Some(vec![n('p', 1000); 64]),
        ..harness_input(&team.id, "最大参数", Some("a"))
    })
    .unwrap();

    // 任务：标题 200、描述 20000、受阻原因 500、文件引用 50、依赖 20。
    let before = count_events(&f);
    let task_with = |title: String, description: String, refs: Vec<FileRef>, deps: Vec<String>| {
        f.ts.create_task(CreateTaskInput {
            team_id: team.id.clone(),
            title,
            description,
            owner_role_id: role.id.clone(),
            status: TaskStatus::Todo,
            file_refs: refs,
            depends_on: deps,
        })
    };
    let msg = assert_kind(
        task_with(n('题', 201), String::new(), vec![], vec![]),
        ErrorKind::Invalid,
    );
    assert!(msg.contains("200"), "{msg}");
    let msg = assert_kind(
        task_with("t".into(), n('x', 20_001), vec![], vec![]),
        ErrorKind::Invalid,
    );
    assert!(msg.contains("20000"), "{msg}");
    let refs = |count: usize| {
        (0..count)
            .map(|i| file_ref(&format!("docs/f{i}.md")))
            .collect::<Vec<_>>()
    };
    let msg = assert_kind(
        task_with("t".into(), String::new(), refs(51), vec![]),
        ErrorKind::Invalid,
    );
    assert!(msg.contains("50"), "{msg}");
    assert_eq!(count_events(&f), before, "失败的创建不写活动");
    let ok_title = task_with(n('题', 200), n('x', 20_000), refs(50), vec![]).unwrap();
    assert_eq!(ok_title.task.key, "OK-001", "失败的创建不消耗编号");

    let deps: Vec<String> = (0..21)
        .map(|i| f.task(&team.id, &role, &format!("dep{i}")).task.id)
        .collect();
    let msg = assert_kind(
        task_with("t".into(), String::new(), vec![], deps.clone()),
        ErrorKind::Invalid,
    );
    assert!(msg.contains("20"), "{msg}");
    task_with("t".into(), String::new(), vec![], deps[..20].to_vec()).unwrap();

    let blocker = f.readyable(&team.id, &role, "受阻");
    f.drive(&blocker.task.id, &[TaskStatus::Ready, TaskStatus::Doing]);
    let before = count_events(&f);
    let too_long = f.ts.change_task_status(ChangeTaskStatusInput {
        task_id: blocker.task.id.clone(),
        to: TaskStatus::Blocked,
        reason: Some(n('因', 501)),
    });
    let msg = assert_kind(too_long, ErrorKind::Invalid);
    assert!(msg.contains("500"), "{msg}");
    assert_eq!(count_events(&f), before);
    assert_eq!(
        f.ts.get_task(&blocker.task.id).unwrap().task.status,
        TaskStatus::Doing
    );
    let blocked =
        f.ts.change_task_status(ChangeTaskStatusInput {
            task_id: blocker.task.id.clone(),
            to: TaskStatus::Blocked,
            reason: Some(n('因', 500)),
        })
        .unwrap();
    assert_eq!(blocked.task.blocked_reason.unwrap().chars().count(), 500);
}
