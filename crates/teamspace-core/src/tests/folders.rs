//! 岗位文件夹（D14）：建立、命名、改名、归档、工作日志、锁外写入失败与迁移补建。

use super::*;
use crate::role_folder::{BRIEF_FILE, JOURNAL_FILE, PROCESS_DIR, README_FILE};

fn folder_dir(f: &Fixture, rel: &str) -> PathBuf {
    rel.split('/').fold(f.ws.clone(), |p, part| p.join(part))
}

fn view(f: &Fixture, team_id: &str, code: &str) -> RoleView {
    f.ts.list_roles(team_id)
        .unwrap()
        .into_iter()
        .find(|v| v.role.code == code)
        .unwrap()
}

fn read(path: &Path) -> String {
    fs::read_to_string(path).unwrap_or_else(|err| panic!("read {}: {err}", path.display()))
}

/// 工作日志中的条目行（去掉 `- ` 与时间），按文件顺序。
fn journal(f: &Fixture, rel: &str) -> Vec<String> {
    read(&folder_dir(f, rel).join(JOURNAL_FILE))
        .lines()
        .filter_map(|l| l.strip_prefix("- "))
        .map(|l| l.split_once(" · ").unwrap().1.to_owned())
        .collect()
}

fn rename(f: &Fixture, role: &Role, name: &str) -> Result<RoleView> {
    f.ts.update_role(UpdateRoleInput {
        role_id: role.id.clone(),
        name: name.into(),
        task_prefix: role.task_prefix.clone(),
        description: role.description.clone(),
        guidelines: role.guidelines.clone(),
        is_leader: role.is_leader,
    })
}

fn add_role(f: &Fixture, team_id: &str, role: NewRole) -> RoleView {
    f.ts.create_role(CreateRoleInput {
        team_id: team_id.into(),
        role,
    })
    .unwrap()
}

#[test]
fn creating_a_team_builds_a_folder_per_role() {
    let f = fixture();
    let team = f.team("T");
    let root = f.ws.join("Teamspace");
    assert!(read(&root.join(README_FILE)).contains("Teamspace 岗位工作区"));
    let names: Vec<_> =
        f.ts.list_roles(&team.id)
            .unwrap()
            .into_iter()
            .map(|v| (v.folder.unwrap(), v.folder_problem))
            .collect();
    assert_eq!(names.len(), 5);
    for (rel, problem) in &names {
        assert!(problem.is_none(), "{rel}: {problem:?}");
        let dir = folder_dir(&f, rel);
        assert!(dir.join(BRIEF_FILE).is_file(), "{rel}");
        assert!(dir.join(JOURNAL_FILE).is_file(), "{rel}");
        assert!(dir.join(PROCESS_DIR).join(README_FILE).is_file(), "{rel}");
    }

    let fe = view(&f, &team.id, "role_frontend");
    assert_eq!(fe.folder.as_deref(), Some("Teamspace/前端开发"));
    let brief = read(&f.ws.join("Teamspace/前端开发").join(BRIEF_FILE));
    for needle in [
        "由 Teamspace 自动生成，请勿手改；手写内容请放到 `过程文件/`",
        "# 岗位说明：前端开发",
        "- 岗位名：前端开发",
        "- 岗位标识：`role_frontend`",
        "- 任务前缀：FE",
        "- 是否 Leader：否",
        "- 所属团队：T",
        "- 状态：在岗",
        "- 当前绑定 Harness：未绑定（暂不派工）",
        "- 更新时间：",
        "UTC+",
        "负责前端界面、交互实现及前端验证。",
        &format!("teamspace-role-id: {} ", fe.role.id),
    ] {
        assert!(brief.contains(needle), "missing {needle:?} in\n{brief}");
    }
    assert!(read(&f.ws.join("Teamspace/Leader").join(BRIEF_FILE)).contains("- 是否 Leader：是"));
    assert_eq!(
        journal(&f, "Teamspace/前端开发"),
        ["创建岗位「前端开发」（任务前缀 FE）"]
    );
    // 同步是幂等的：再次打开概览不会重复写日志。
    f.ts.get_team_overview(&team.id).unwrap();
    assert_eq!(journal(&f, "Teamspace/前端开发").len(), 1);
}

#[test]
fn folder_names_are_sanitized_and_conflicts_get_the_role_code() {
    let f = plain_fixture();
    let team = f.team("T");
    let slash = add_role(&f, &team.id, new_role(" a/b:c? ", "role_ab", "AB", false));
    assert_eq!(slash.folder.as_deref(), Some("Teamspace/a_b_c_"));
    let dots = add_role(&f, &team.id, new_role("...", "role_dots", "DT", false));
    assert_eq!(dots.folder.as_deref(), Some("Teamspace/role_dots"));

    // 磁盘上已有用户自己的同名文件夹：不动它，新岗位加「（岗位标识）」。
    f.touch("Teamspace/设计/我的笔记.md");
    let design = add_role(&f, &team.id, new_role("设计", "role_design", "DS", false));
    assert_eq!(
        design.folder.as_deref(),
        Some("Teamspace/设计（role_design）")
    );
    assert_eq!(read(&f.ws.join("Teamspace/设计/我的笔记.md")), "x");
    assert!(!f.ws.join("Teamspace/设计").join(BRIEF_FILE).exists());

    // 与本团队已有岗位同名。
    let twin = add_role(&f, &team.id, new_role("前端开发", "role_fe2", "FX", false));
    assert_eq!(
        twin.folder.as_deref(),
        Some("Teamspace/前端开发（role_fe2）")
    );
    // 用户事先放在 Teamspace/ 下的 README 不会被覆盖。
    let g = plain_fixture();
    g.touch("Teamspace/README.md");
    g.team("T");
    assert_eq!(read(&g.ws.join("Teamspace/README.md")), "x");
}

#[test]
fn renaming_a_role_moves_its_folder() {
    let f = fixture();
    let team = f.team("T");
    let fe = f.role(&team.id, "role_frontend");
    f.touch("Teamspace/前端开发/过程文件/草稿.md");
    let renamed = rename(&f, &fe, "Web 前端").unwrap();
    assert_eq!(renamed.folder.as_deref(), Some("Teamspace/Web 前端"));
    assert!(renamed.folder_problem.is_none());
    assert!(!f.ws.join("Teamspace/前端开发").exists());
    assert_eq!(read(&f.ws.join("Teamspace/Web 前端/过程文件/草稿.md")), "x");
    assert!(read(&f.ws.join("Teamspace/Web 前端").join(BRIEF_FILE)).contains("- 岗位名：Web 前端"));
    assert_eq!(
        journal(&f, "Teamspace/Web 前端").last().unwrap(),
        "编辑岗位：岗位改名：「前端开发」→「Web 前端」"
    );

    // 目标已存在（用户文件夹或其他岗位的文件夹）：冲突，不改名。
    f.touch("Teamspace/占用/a.md");
    let fe = f.role(&team.id, "role_frontend");
    let msg = assert_kind(rename(&f, &fe, "占用"), ErrorKind::Conflict);
    assert!(msg.contains("Teamspace/占用"), "{msg}");
    assert_kind(rename(&f, &fe, "后端开发"), ErrorKind::Conflict);
    assert_eq!(f.role(&team.id, "role_frontend").name, "Web 前端");
    assert!(f.ws.join("Teamspace/Web 前端").is_dir());

    // 只改职责不动文件夹；日志记录修改。
    let mut input = UpdateRoleInput {
        role_id: fe.id.clone(),
        name: fe.name.clone(),
        task_prefix: fe.task_prefix.clone(),
        description: "新的职责".into(),
        guidelines: "新的约定".into(),
        is_leader: false,
    };
    f.ts.update_role(input.clone()).unwrap();
    assert!(read(&f.ws.join("Teamspace/Web 前端").join(BRIEF_FILE)).contains("新的职责"));
    assert_eq!(
        journal(&f, "Teamspace/Web 前端").last().unwrap(),
        "编辑岗位：修改职责；修改工作约定"
    );
    // 名称只差大小写也能改。
    input.name = "web 前端".into();
    let lower = f.ts.update_role(input).unwrap();
    assert_eq!(lower.folder.as_deref(), Some("Teamspace/web 前端"));
    assert!(lower.folder_problem.is_none());
}

#[test]
fn archiving_keeps_the_folder() {
    let f = fixture();
    let team = f.team("T");
    let qa = f.role(&team.id, "role_qa");
    let mock = f.harness(&team.id, "Mock", AcpPreset::Mock);
    f.ts.bind_role_harness(&qa.id, &mock.id).unwrap();
    f.ts.archive_role(&qa.id).unwrap();
    let dir = f.ws.join("Teamspace/QA 测试");
    let brief = read(&dir.join(BRIEF_FILE));
    assert!(brief.contains("- 状态：已归档（"), "{brief}");
    assert!(brief.contains("- 当前绑定 Harness：未绑定"), "{brief}");
    assert_eq!(
        journal(&f, "Teamspace/QA 测试").last().unwrap(),
        "归档岗位「QA 测试」"
    );
    // 归档岗位的文件夹名仍被占用：新岗位重名时加后缀。
    let again = add_role(&f, &team.id, new_role("QA 测试", "role_qa2", "QA", false));
    assert_eq!(
        again.folder.as_deref(),
        Some("Teamspace/QA 测试（role_qa2）")
    );
    let folder = f.ts.get_role_folder(&qa.id, None).unwrap();
    assert!(folder.exists);
}

#[test]
fn journal_records_bindings_dispatch_and_status_changes_in_order() {
    let f = fixture();
    let team = f.team("T");
    let fe = f.role(&team.id, "role_frontend");
    let be = f.role(&team.id, "role_backend");
    let a = f.harness(&team.id, "Claude A", AcpPreset::ClaudeCode);
    let b = f.harness(&team.id, "Codex B", AcpPreset::Codex);
    f.ts.bind_role_harness(&fe.id, &a.id).unwrap();
    assert!(
        read(&f.ws.join("Teamspace/前端开发").join(BRIEF_FILE)).contains(
            "- 当前绑定 Harness：Claude A（Claude Code · ACP，启动命令 `claude-code-acp`）"
        )
    );
    f.ts.bind_role_harness(&fe.id, &b.id).unwrap();
    // 改 Harness 名称后岗位说明随之更新。
    f.ts.update_harness(UpdateHarnessInput {
        harness_id: b.id.clone(),
        name: "Codex 二号".into(),
        command: Some("/opt/bin/codex-acp".into()),
        args: Some(vec!["--profile".into(), "work dir".into()]),
        env_profile: None,
    })
    .unwrap();
    assert!(
        read(&f.ws.join("Teamspace/前端开发").join(BRIEF_FILE))
            .contains(
                "- 当前绑定 Harness：Codex 二号（Codex · ACP，启动命令 `/opt/bin/codex-acp --profile \"work dir\"`）"
            )
    );
    f.ts.unbind_role_harness(&fe.id).unwrap();

    let t = f.readyable(&team.id, &fe, "登录页");
    f.drive(
        &t.task.id,
        &[TaskStatus::Ready, TaskStatus::Doing, TaskStatus::Blocked],
    );
    f.ts.update_task(UpdateTaskInput {
        task_id: t.task.id.clone(),
        title: "登录页".into(),
        description: String::new(),
        owner_role_id: be.id.clone(),
        file_refs: vec![file_ref("docs/spec.md")],
        depends_on: vec![],
    })
    .unwrap();

    assert_eq!(
        journal(&f, "Teamspace/前端开发"),
        [
            "创建岗位「前端开发」（任务前缀 FE）",
            "绑定 Harness「Claude A」",
            "更换 Harness：「Claude A」→「Codex B」",
            "岗位「前端开发」解除绑定 Harness「Codex 二号」",
            "FE-001 · 创建任务 FE-001「登录页」，负责岗位「前端开发」",
            "FE-001 · 状态 TODO → READY",
            "FE-001 · 状态 READY → 执行中",
            "FE-001 · 状态 执行中 → 受阻：等待接口",
            "FE-001 · 编辑任务 FE-001「登录页」，负责岗位改为「后端开发」（已从本岗位改派走）",
        ]
        .map(|s| s.to_owned())
        .to_vec(),
        "{:#?}",
        journal(&f, "Teamspace/前端开发")
    );
    assert_eq!(
        journal(&f, "Teamspace/后端开发").last().unwrap(),
        "FE-001 · 编辑任务 FE-001「登录页」，负责岗位改为「后端开发」（改派到本岗位）"
    );
    // 其他岗位的日志不受影响。
    assert_eq!(journal(&f, "Teamspace/QA 测试").len(), 1);

    let folder = f.ts.get_role_folder(&fe.id, Some(2)).unwrap();
    assert!(folder.exists);
    assert!(folder.problem.is_none());
    assert_eq!(folder.relative_path.as_deref(), Some("Teamspace/前端开发"));
    assert_eq!(
        Path::new(folder.path.as_deref().unwrap()),
        f.ws.join("Teamspace/前端开发")
    );
    assert_eq!(folder.journal.len(), 2);
    assert!(
        folder.journal[0].ends_with("（已从本岗位改派走）"),
        "{:?}",
        folder.journal
    );
    assert!(folder.journal[1].ends_with("执行中 → 受阻：等待接口"));
    assert_eq!(f.ts.get_role_folder(&fe.id, None).unwrap().journal.len(), 9);
}

#[test]
fn reveal_opens_the_folder_through_the_probe() {
    let f = fixture();
    let team = f.team("T");
    let fe = f.role(&team.id, "role_frontend");
    f.ts.reveal_role_folder(&fe.id).unwrap();
    REVEALED.with(|r| assert_eq!(*r.borrow(), [f.ws.join("Teamspace/前端开发")]));
    assert_kind(f.ts.reveal_role_folder("missing"), ErrorKind::NotFound);
    assert_kind(f.ts.get_role_folder("missing", None), ErrorKind::NotFound);
}

#[cfg(unix)]
#[test]
fn folder_write_failures_do_not_roll_back_and_are_reported_once() {
    use std::os::unix::fs::PermissionsExt;
    let f = fixture();
    let team = f.team("T");
    let root = f.ws.join("Teamspace");
    fs::set_permissions(&root, fs::Permissions::from_mode(0o555)).unwrap();

    let design = add_role(&f, &team.id, new_role("设计", "role_design", "DS", false));
    assert!(design.folder.is_none());
    let problem = design.folder_problem.clone().unwrap();
    assert!(problem.contains("无法写入岗位文件夹"), "{problem}");
    // 数据库已提交：岗位存在。
    assert_eq!(f.role(&team.id, "role_design").name, "设计");
    // 再次写操作不会重复记录同一问题。
    f.ts.get_team_overview(&team.id).unwrap();
    let errors = |f: &Fixture| {
        f.ts.list_events(EventQuery {
            team_id: team.id.clone(),
            cursor: None,
            limit: Some(200),
        })
        .unwrap()
        .events
        .into_iter()
        .filter(|e| e.kind == EventKind::RoleFolderError)
        .collect::<Vec<_>>()
    };
    let recorded = errors(&f);
    assert_eq!(recorded.len(), 1);
    assert_eq!(
        recorded[0].role_id.as_deref(),
        Some(design.role.id.as_str())
    );
    assert!(
        recorded[0]
            .summary
            .starts_with("岗位「设计」的文件夹写入失败：")
    );
    let shown = f.ts.get_role_folder(&design.role.id, None).unwrap();
    assert!(!shown.exists && shown.path.is_none());
    assert_eq!(shown.problem.as_deref(), Some(problem.as_str()));
    assert_kind(
        f.ts.reveal_role_folder(&design.role.id),
        ErrorKind::NotFound,
    );

    // 恢复后下一次同步补建文件夹，并补写日志（含失败记录）。
    fs::set_permissions(&root, fs::Permissions::from_mode(0o755)).unwrap();
    let overview = f.ts.get_team_overview(&team.id).unwrap();
    let design = overview
        .roles
        .iter()
        .find(|v| v.role.code == "role_design")
        .unwrap();
    assert_eq!(design.folder.as_deref(), Some("Teamspace/设计"));
    assert!(design.folder_problem.is_none());
    let lines = journal(&f, "Teamspace/设计");
    assert_eq!(lines[0], "创建岗位「设计」（任务前缀 DS）");
    assert!(
        lines[1].starts_with("岗位「设计」的文件夹写入失败"),
        "{lines:?}"
    );
}

#[test]
fn missing_workspace_is_reported_without_failing_commands() {
    let f = fixture();
    let team = f.team("T");
    let fe = f.role(&team.id, "role_frontend");
    let mock = f.harness(&team.id, "Mock", AcpPreset::Mock);
    fs::remove_dir_all(&f.ws).unwrap();
    let bound = f.ts.bind_role_harness(&fe.id, &mock.id).unwrap();
    assert_eq!(bound.role.harness_id.as_deref(), Some(mock.id.as_str()));
    assert_eq!(bound.folder.as_deref(), Some("Teamspace/前端开发"));
    let problem = bound.folder_problem.unwrap();
    assert!(problem.contains("工作空间目录不存在"), "{problem}");
    let shown = f.ts.get_role_folder(&fe.id, None).unwrap();
    assert!(!shown.exists && shown.journal.is_empty());
    assert_eq!(shown.problem.as_deref(), Some(problem.as_str()));
    let msg = assert_kind(f.ts.reveal_role_folder(&fe.id), ErrorKind::NotFound);
    assert!(msg.contains("工作空间目录不存在"), "{msg}");
    // 改名不因工作空间缺失而被拒绝。
    let fe = f.role(&team.id, "role_frontend");
    rename(&f, &fe, "前端").unwrap();

    // 工作空间恢复后：文件夹按新名称重建，绑定写入日志。
    fs::create_dir_all(&f.ws).unwrap();
    f.ts.get_team_overview(&team.id).unwrap();
    let fe = view(&f, &team.id, "role_frontend");
    assert_eq!(fe.folder.as_deref(), Some("Teamspace/前端"));
    assert!(fe.folder_problem.is_none());
    let lines = journal(&f, "Teamspace/前端");
    assert!(
        lines.contains(&"绑定 Harness「Mock」".to_owned()),
        "{lines:?}"
    );
}

#[test]
fn roles_from_before_v3_get_folders_and_history_on_next_sync() {
    let f = fixture();
    let team = f.team("T");
    let fe = f.role(&team.id, "role_frontend");
    f.task(&team.id, &fe, "旧任务");
    // 模拟 v3 之前的数据：没有文件夹登记，日志序号为 0，磁盘上也没有 Teamspace/。
    fs::remove_dir_all(f.ws.join("Teamspace")).unwrap();
    f.raw_sql("UPDATE roles SET folder = NULL, folder_problem = NULL, journaled_seq = 0;");
    assert!(view(&f, &team.id, "role_frontend").folder.is_none());

    f.ts.get_team_overview(&team.id).unwrap();
    assert_eq!(
        view(&f, &team.id, "role_frontend").folder.as_deref(),
        Some("Teamspace/前端开发")
    );
    assert_eq!(
        journal(&f, "Teamspace/前端开发"),
        [
            "创建岗位「前端开发」（任务前缀 FE）",
            "FE-001 · 创建任务 FE-001「旧任务」，负责岗位「前端开发」",
        ]
    );
}
