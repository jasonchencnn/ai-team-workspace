//! 测试轮次 2026-10-14（Sonnet B）：执行（RUN-*）中属于内核层的用例。
//! 真实子进程（Mock Agent）、真实 Git、临时目录；不调用任何真实 Agent。
//!
//! 对应用例：RUN-ACP-002、RUN-ACP-004、RUN-ACP-005、RUN-PRE-004、RUN-DIR-002、RUN-PKG-002、RUN-EVT-002、
//! RUN-FAIL-005、RUN-LOG-001、RUN-LOG-004。

mod common;

use std::ffi::OsString;
use std::path::{Path, PathBuf};

use common::*;
use teamspace_core::acp::AgentLaunch;
use teamspace_core::model::*;
use teamspace_core::role_folder;
use teamspace_core::run::brief::{self, BriefInput};
use teamspace_core::run::worktree;
use teamspace_core::system_probe::LocalProbe;
use teamspace_core::{ErrorKind, SystemProbe};

/// 包一层真实探测：指定的命令「找不到」、指定的命令握手视为成功，其余照旧。
struct Probe {
    inner: LocalProbe,
    /// 全部命令都找不到。
    nothing_installed: bool,
    missing: Vec<&'static str>,
    handshake_ok: Vec<&'static str>,
}

impl Probe {
    fn new() -> Self {
        Self {
            inner: LocalProbe::new(),
            nothing_installed: false,
            missing: Vec::new(),
            handshake_ok: Vec::new(),
        }
    }
}

impl SystemProbe for Probe {
    fn inspect_workspace(&self, path: &Path) -> teamspace_core::Result<WorkspaceInfo> {
        self.inner.inspect_workspace(path)
    }

    fn search_files(
        &self,
        workspace: &Path,
        query: &str,
        limit: usize,
    ) -> teamspace_core::Result<Vec<String>> {
        self.inner.search_files(workspace, query, limit)
    }

    fn list_markdown_files(
        &self,
        workspace: &Path,
        limit: usize,
    ) -> teamspace_core::Result<Vec<String>> {
        self.inner.list_markdown_files(workspace, limit)
    }

    fn locate_command(&self, command: &str) -> Option<String> {
        if self.nothing_installed || self.missing.contains(&command) {
            return None;
        }
        self.inner.locate_command(command)
    }

    fn handshake(&self, launch: &AgentLaunch) -> Handshake {
        if self.handshake_ok.contains(&launch.command.as_str()) {
            return Handshake {
                status: HandshakeStatus::Connected,
                protocol_version: Some(1),
                detail: Some("握手成功（测试替身）".into()),
                ..Handshake::unknown()
            };
        }
        self.inner.handshake(launch)
    }

    fn reveal_path(&self, path: &Path) -> teamspace_core::Result<()> {
        self.inner.reveal_path(path)
    }

    fn command_path_env(&self) -> OsString {
        self.inner.command_path_env()
    }
}

// ------------------------------------------------------------------ RUN-ACP-002

#[test]
fn run_acp_002_claude_code_and_codex_report_required_cli_and_npm_packages() {
    let e = env_with(
        false,
        Box::new(Probe {
            nothing_installed: true,
            ..Probe::new()
        }),
    );
    let presets = e.ts.list_acp_presets();
    let get = |preset| presets.iter().find(|p| p.preset == preset).unwrap();

    let claude = get(AcpPreset::ClaudeCode);
    assert_eq!(claude.default_command.as_deref(), Some("claude-code-acp"));
    assert_eq!(claude.requires_cli.as_deref(), Some("claude"));
    assert!(!claude.command_found);
    assert_eq!(claude.cli_found, Some(false));
    let hint = claude.hint.as_deref().unwrap();
    assert!(hint.contains("@zed-industries/claude-code-acp"), "{hint}");

    let codex = get(AcpPreset::Codex);
    assert_eq!(codex.default_command.as_deref(), Some("codex-acp"));
    assert_eq!(codex.requires_cli.as_deref(), Some("codex"));
    assert!(!codex.command_found);
    assert_eq!(codex.cli_found, Some(false));
    let hint = codex.hint.as_deref().unwrap();
    assert!(hint.contains("@zed-industries/codex-acp"), "{hint}");
}

// ------------------------------------------------------------------ RUN-ACP-005

#[test]
fn run_acp_005_mock_preset_is_builtin_and_needs_no_install() {
    // 本机什么都没装：Mock 预设仍然可用，没有安装提示，也不依赖任何 CLI
    let e = env_with(
        false,
        Box::new(Probe {
            nothing_installed: true,
            ..Probe::new()
        }),
    );
    let presets = e.ts.list_acp_presets();
    let mock = presets
        .iter()
        .find(|p| p.preset == AcpPreset::Mock)
        .unwrap();
    assert!(mock.command_found);
    assert_eq!(mock.requires_cli, None);
    assert_eq!(mock.hint, None);
    // 握手检查（真实子进程）：协议版本 1、名称 teamspace-mock-agent、支持 loadSession
    let checked = e.ts.check_harness(&e.harness.id).unwrap();
    assert_eq!(checked.handshake.status, HandshakeStatus::Connected);
    assert_eq!(checked.handshake.protocol_version, Some(1));
    assert_eq!(
        checked.handshake.agent.as_ref().map(|a| a.name.as_str()),
        Some("teamspace-mock-agent")
    );
    assert!(
        checked
            .handshake
            .capabilities
            .as_ref()
            .unwrap()
            .load_session
    );
    assert!(checked.handshake.auth_methods.is_empty());
    // 默认流程：会话 ID 以 mock-session- 开头，以 end_turn 结束
    let task = e.task("Mock 全流程");
    let run = e.wait_end(&e.ts.start_run(&task.id).unwrap().id);
    assert!(
        run.session_id
            .as_deref()
            .unwrap()
            .starts_with("mock-session-")
    );
    assert_eq!(run.stop_reason.as_deref(), Some("end_turn"));
}

// ------------------------------------------------------------------ RUN-FAIL-005

#[test]
fn run_fail_005_missing_command_fails_with_launch_and_path_hint() {
    let e = env_with(
        false,
        Box::new(Probe {
            handshake_ok: vec!["non_existing_cmd_xyz"],
            missing: vec!["non_existing_cmd_xyz"],
            ..Probe::new()
        }),
    );
    let harness =
        e.ts.create_harness(CreateHarnessInput {
            team_id: e.team.id.clone(),
            name: "不存在的命令".into(),
            preset: AcpPreset::Custom,
            command: Some("non_existing_cmd_xyz".into()),
            args: Some(Vec::new()),
            env_profile: None,
        })
        .unwrap();
    e.ts.bind_role_harness(&e.role.id, &harness.id).unwrap();
    let task = e.task("启动不了");
    let run = e.ts.start_run(&task.id).unwrap();
    let run = e.wait_end(&run.id);
    assert_eq!(run.status, RunStatus::Failed, "{run:#?}");
    assert_eq!(run.failure_kind, Some(FailureKind::Launch));
    assert_eq!(
        run.failure.as_deref(),
        Some("未找到命令 non_existing_cmd_xyz：请确认已安装并加入 PATH")
    );
    let task = e.ts.get_task(&task.id).unwrap().task;
    assert_eq!(task.status, TaskStatus::Blocked);
    assert!(
        task.blocked_reason
            .unwrap()
            .contains("未找到命令 non_existing_cmd_xyz：请确认已安装并加入 PATH")
    );
    // 没有会话、没有可续接的东西，执行已经结束
    assert!(run.session_id.is_none());
    assert!(!run.can_resume);
    assert!(run.ended_at.is_some());
}

// ------------------------------------------------------------------ RUN-PRE-004

#[test]
fn run_pre_004_archived_or_missing_owner_blocks_starting() {
    let e = env(false);
    // A) 岗位已归档：任务必须先结束（岗位归档要求没有未结束任务）
    let task = e.task("归档前的任务");
    let run = e.ts.start_run(&task.id).unwrap();
    assert_eq!(e.wait_end(&run.id).status, RunStatus::Succeeded);
    e.ts.change_task_status(ChangeTaskStatusInput {
        task_id: task.id.clone(),
        to: TaskStatus::Done,
        reason: None,
    })
    .unwrap();
    e.ts.archive_role(&e.role.id).unwrap();
    let problems = e.ts.get_task(&task.id).unwrap().run_problems;
    assert!(
        problems.iter().any(|p| p == "负责岗位「前端」已归档"),
        "{problems:?}"
    );
    let err = e.ts.start_run(&task.id).unwrap_err();
    assert_eq!(err.kind(), ErrorKind::Conflict);
    assert!(err.to_string().contains("负责岗位「前端」已归档"), "{err}");

    // B) 负责岗位记录不存在（数据损坏）：「未指定负责岗位」的问题清单由内核单元用例
    // service::run::tests::start_problems_reports_a_missing_owner_role 断言；这里确认
    // 对外接口不会开始执行。
    let other = env(false);
    let task = other.task("没有负责人的任务");
    let db = other.dir.path().join("data/t.db");
    let conn = rusqlite::Connection::open(&db).unwrap();
    conn.execute_batch(&format!(
        "PRAGMA foreign_keys = OFF; UPDATE tasks SET owner_role_id = 'role-gone' WHERE id = '{}';",
        task.id
    ))
    .unwrap();
    drop(conn);
    assert!(other.ts.start_run(&task.id).is_err());
    assert!(other.ts.list_runs(&task.id).unwrap().is_empty());
}

// ------------------------------------------------------------------ RUN-DIR-002

fn git_out(dir: &Path, args: &[&str]) -> String {
    let out = std::process::Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(args)
        .output()
        .unwrap();
    assert!(out.status.success(), "git {args:?}");
    String::from_utf8_lossy(&out.stdout).trim().to_owned()
}

#[test]
fn run_dir_002_branch_and_directory_collisions_get_numeric_suffixes() {
    let e = env(true);
    let target = e.dir.path().join("wt/DEV-001-r1");
    let path_env = LocalProbe::new().command_path_env();

    let first = worktree::create(&e.ws, &target, "teamspace/DEV-001-r1", Some(&path_env)).unwrap();
    assert_eq!(first.branch, "teamspace/DEV-001-r1");
    let second = worktree::create(&e.ws, &target, "teamspace/DEV-001-r1", Some(&path_env)).unwrap();
    assert_eq!(second.branch, "teamspace/DEV-001-r1-2");
    assert!(
        second.exec_dir.ends_with("DEV-001-r1-2"),
        "{:?}",
        second.exec_dir
    );
    assert!(second.exec_dir.is_dir());
    // 目录已存在但分支空着：同样递增
    let only_dir = e.dir.path().join("wt/DEV-002-r1");
    std::fs::create_dir_all(&only_dir).unwrap();
    let third =
        worktree::create(&e.ws, &only_dir, "teamspace/DEV-002-r1", Some(&path_env)).unwrap();
    assert_eq!(third.branch, "teamspace/DEV-002-r1-2");

    // 后缀用尽（-1 即原名，到 -20 共 20 个）：报错
    for n in 1..=20 {
        let name = if n == 1 {
            "teamspace/DEV-003-r1".to_owned()
        } else {
            format!("teamspace/DEV-003-r1-{n}")
        };
        git_out(&e.ws, &["branch", &name]);
    }
    let err = worktree::create(
        &e.ws,
        &e.dir.path().join("wt/DEV-003-r1"),
        "teamspace/DEV-003-r1",
        Some(&path_env),
    )
    .unwrap_err();
    assert!(
        err.contains("分支 teamspace/DEV-003-r1 及其后缀都已存在"),
        "{err}"
    );
}

// ------------------------------------------------------------------ RUN-PKG-002

fn brief_input(journal: Vec<String>) -> BriefInput {
    BriefInput {
        role_name: "前端".into(),
        role_description: "负责页面".into(),
        role_guidelines: String::new(),
        journal,
        task_key: "FE-001".into(),
        task_title: "登录页".into(),
        task_description: String::new(),
        refs: Vec::new(),
        dependencies: Vec::new(),
        exec_dir: "/tmp/ws".into(),
        isolation: RunIsolation::InPlace,
        branch: None,
        role_folder: Some("Teamspace/前端".into()),
        journal_path: None,
        previous_branch: None,
        handoff: None,
    }
}

#[test]
fn run_pkg_002_brief_carries_the_latest_ten_journal_lines_newest_first() {
    // 磁盘上的日志 15 条：读出来最新的在前，只取 10 条
    let dir = tempfile::tempdir().unwrap();
    let text: String = std::iter::once("# 工作日志\n\n> 说明\n\n".to_owned())
        .chain((1..=15).map(|i| format!("- 日志 {i}\n")))
        .collect();
    std::fs::write(dir.path().join("工作日志.md"), text).unwrap();
    let recent = role_folder::read_recent_journal(dir.path(), brief::JOURNAL_LINES).unwrap();
    let expected: Vec<String> = (6..=15).rev().map(|i| format!("日志 {i}")).collect();
    assert_eq!(recent, expected);

    // 说明包：小标题、10 条、顺序、被截掉的旧条目
    let body = brief::render(&brief_input(recent));
    assert!(
        body.contains("## 岗位工作日志（最近 10 条，最新的在前）"),
        "{body}"
    );
    let lines: Vec<&str> = body
        .lines()
        .skip_while(|l| !l.starts_with("## 岗位工作日志"))
        .skip(1)
        .take_while(|l| !l.starts_with("## "))
        .filter(|l| !l.is_empty())
        .collect();
    let expected: Vec<String> = (6..=15).rev().map(|i| format!("- 日志 {i}")).collect();
    assert_eq!(lines, expected, "{body}");
    assert!(!body.contains("- 日志 5\n"));
    // 即使调用方传了 15 条，说明包也只取前 10 条
    let all: Vec<String> = (1..=15).rev().map(|i| format!("日志 {i}")).collect();
    let body = brief::render(&brief_input(all));
    assert!(
        body.contains("- 日志 6\n") && !body.contains("- 日志 5\n"),
        "{body}"
    );
    // 无记录
    let body = brief::render(&brief_input(Vec::new()));
    assert!(body.contains("（暂无记录）"), "{body}");
}

// ------------------------------------------------------------------ RUN-EVT-002

#[test]
fn run_evt_002_agent_message_chunks_merge_into_one_growing_update() {
    let e = env(false);
    let task = e.task("消息合并");
    let run = e.ts.start_run(&task.id).unwrap();
    let run = e.wait_end(&run.id);
    assert_eq!(run.status, RunStatus::Succeeded, "{run:#?}");

    // 事件流：同一条进度被反复更新，文字一次比一次长。
    // 事件异步推送，等最终的 run_changed 到齐再取快照。
    let events = e.wait_event("run_changed(succeeded)", |events| {
        events.iter().any(|ev| {
            matches!(ev, RunEvent::RunChanged { run: r } if r.id == run.id && r.status == RunStatus::Succeeded)
        })
    });
    let updates: Vec<&RunUpdate> = events
        .iter()
        .filter_map(|ev| match ev {
            RunEvent::RunUpdated { update, .. } => Some(update),
            _ => None,
        })
        .collect();
    let agent_text = |u: &RunUpdate| match &u.body {
        RunUpdateBody::Message {
            role: MessageRole::Agent,
            text,
        } => Some(text.clone()),
        _ => None,
    };
    let first_id = updates
        .iter()
        .find(|u| agent_text(u).is_some())
        .map(|u| u.id.clone())
        .unwrap();
    let growth: Vec<String> = updates
        .iter()
        .filter(|u| u.id == first_id)
        .copied()
        .filter_map(agent_text)
        .collect();
    assert_eq!(
        growth,
        vec!["收到任务，".to_owned(), "收到任务，开始处理。".to_owned()],
        "两个分块应累加到同一条更新里"
    );

    // 数据库：只有一条这样的消息，没有分散的卡片
    let stored = e.updates(&run.id);
    let chunks: Vec<&RunUpdate> = stored
        .iter()
        .filter(|u| matches!(&u.body, RunUpdateBody::Message { text, .. } if text.starts_with("收到任务")))
        .collect();
    assert_eq!(chunks.len(), 1, "{stored:#?}");
    assert_eq!(chunks[0].id, first_id);
    assert_eq!(
        agent_text(chunks[0]).as_deref(),
        Some("收到任务，开始处理。")
    );

    // 结束摘要取自最后一条 Agent 消息
    let last = stored
        .iter()
        .rev()
        .find_map(agent_text)
        .expect("agent message");
    assert_eq!(run.summary.as_deref(), Some(last.trim()));
}

// ------------------------------------------------------------------ RUN-LOG-001

#[test]
fn run_log_001_summary_is_the_last_agent_message_and_persists() {
    let e = env(false);
    let task = e.task("摘要落库");
    let run = e.ts.start_run(&task.id).unwrap();
    let run = e.wait_end(&run.id);
    assert_eq!(run.status, RunStatus::Succeeded);
    let last = e
        .updates(&run.id)
        .iter()
        .rev()
        .find_map(|u| match &u.body {
            RunUpdateBody::Message {
                role: MessageRole::Agent,
                text,
            } => Some(text.clone()),
            _ => None,
        })
        .unwrap();
    assert_eq!(run.stop_reason.as_deref(), Some("end_turn"));
    assert_eq!(run.summary.as_deref(), Some(last.trim()));
    assert!(
        run.summary
            .as_deref()
            .unwrap()
            .starts_with("已完成：在 Teamspace/前端/过程文件/ 写入了")
    );
    // 落库：重新打开同一个数据库，摘要仍在，任务详情的最新执行也带着它
    let Env { dir, ts, .. } = e;
    drop(ts);
    let config = teamspace_core::HostConfig::default();
    let reopened = teamspace_core::Teamspace::open_with(
        &dir.path().join("data/t.db"),
        Box::new(LocalProbe::new()),
        config,
    )
    .unwrap();
    assert_eq!(
        reopened.get_run(&run.id).unwrap().summary,
        run.summary,
        "摘要应写入数据库"
    );
    assert_eq!(
        reopened
            .get_task(&task.id)
            .unwrap()
            .latest_run
            .unwrap()
            .summary,
        run.summary
    );
}

// ------------------------------------------------------------------ RUN-LOG-004

fn assert_local_timestamps(journal: &str) {
    let stamp = regex_like_timestamp;
    for line in journal.lines().filter(|l| l.starts_with("- ")) {
        assert!(stamp(&line[2..]), "时间戳格式不对：{line}");
    }
}

/// `YYYY-MM-DD HH:MM:SS UTC±HH:MM` 开头。
fn regex_like_timestamp(text: &str) -> bool {
    let b = text.as_bytes();
    let digit = |i: usize| b.get(i).is_some_and(u8::is_ascii_digit);
    let at = |i: usize, c: u8| b.get(i) == Some(&c);
    [0, 1, 2, 3, 5, 6, 8, 9, 11, 12, 14, 15, 17, 18]
        .iter()
        .all(|&i| digit(i))
        && at(4, b'-')
        && at(7, b'-')
        && at(10, b' ')
        && at(13, b':')
        && at(16, b':')
        && text[19..].starts_with(" UTC")
        && matches!(b.get(23), Some(b'+' | b'-'))
        && digit(24)
        && digit(25)
        && at(26, b':')
        && digit(27)
        && digit(28)
}

#[test]
fn run_log_004_cancel_failure_and_takeover_are_journaled() {
    let e = env(false);

    // 取消
    let cancelled = e.task("取消的任务 [mock:hang]");
    let run = e.ts.start_run(&cancelled.id).unwrap();
    e.wait(&run.id, "running", |r| r.status == RunStatus::Running);
    e.ts.cancel_run(&run.id).unwrap();
    let journal = e.wait_journal("第 1 次执行已取消（Mock）");
    assert!(
        journal.contains("FE-001 · 第 1 次执行已取消（Mock）；原因：已取消"),
        "{journal}"
    );
    assert!(
        journal.contains("FE-001 · 状态 执行中 → READY（执行已取消）"),
        "{journal}"
    );

    // 失败
    let failed = e.task("失败的任务 [mock:fail]");
    let run = e.ts.start_run(&failed.id).unwrap();
    assert_eq!(e.wait_end(&run.id).status, RunStatus::Failed);
    let journal = e.wait_journal("FE-002 · 第 1 次执行失败（Mock）；原因：");
    assert!(
        journal.contains("FE-002 · 状态 执行中 → 受阻：执行失败："),
        "{journal}"
    );

    // 人工接管：有「已停止，人工接管」，且那一行没有「；原因：」
    let taken = e.task("接管的任务 [mock:hang]");
    let run = e.ts.start_run(&taken.id).unwrap();
    e.wait(&run.id, "running", |r| r.status == RunStatus::Running);
    e.ts.takeover_run(&run.id).unwrap();
    let journal = e.wait_journal("FE-003 · 第 1 次执行已停止，人工接管（Mock）");
    let line = journal
        .lines()
        .find(|l| l.contains("FE-003 · 第 1 次执行已停止，人工接管"))
        .unwrap();
    assert!(!line.contains("；原因："), "{line}");
    assert_local_timestamps(&journal);
}

// ------------------------------------------------------------------ RUN-ACP-004

/// `export -p` 输出里 PATH 变量的那一行（不同 shell 的前缀不同）。
fn path_var_line(dump: &str) -> &str {
    dump.lines()
        .find(|l| {
            let l = l.trim_start();
            let l = l.strip_prefix("export ").unwrap_or(l);
            let l = l.strip_prefix("declare -x ").unwrap_or(l);
            l.starts_with("PATH=")
        })
        .unwrap_or_else(|| panic!("没有 PATH：{dump}"))
}

#[test]
fn run_acp_004_custom_agent_gets_env_profile_and_platform_path_without_leaking_values() {
    use std::os::unix::fs::PermissionsExt;

    let secret = "s3cr3t-value-xyz";
    let e = env(false);
    let work = tempfile::tempdir().unwrap();
    let dump = work.path().join("env.dump");
    let script = work.path().join("agent.sh");
    // 先记下子进程的环境与参数，再接力给内置 Mock Agent。
    std::fs::write(
        &script,
        format!(
            "#!/bin/sh\nexport -p > '{dump}'\nprintf '%s\\n' \"$@\" > '{dump}.args'\nexec '{mock}'\n",
            dump = dump.display(),
            mock = mock_agent().display()
        ),
    )
    .unwrap();
    std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
    let plain_profile = work.path().join("plain.env");
    std::fs::write(
        &plain_profile,
        format!("FOO={secret}\nexport BAR=\"two words\"\n"),
    )
    .unwrap();
    let path_profile = work.path().join("path.env");
    std::fs::write(&path_profile, format!("FOO={secret}\nPATH=/custom\n")).unwrap();

    let make = |name: &str, profile: &Path| {
        let h =
            e.ts.create_harness(CreateHarnessInput {
                team_id: e.team.id.clone(),
                name: name.into(),
                preset: AcpPreset::Custom,
                command: Some(script.display().to_string()),
                args: Some(vec!["--stdio".into(), "--verbose".into()]),
                env_profile: Some(profile.display().to_string()),
            })
            .unwrap();
        assert_eq!(h.preset, AcpPreset::Custom);
        assert_eq!(h.env_profile.as_deref(), Some(profile.to_str().unwrap()));
        assert_eq!(h.args, ["--stdio", "--verbose"]);
        h
    };

    // 档案没写 PATH：拿到平台补全的 PATH，加上档案里的变量
    let h = make("custom-env", &plain_profile);
    e.ts.bind_role_harness(&e.role.id, &h.id).unwrap();
    let task = e.task("环境注入");
    let run = e.ts.start_run(&task.id).unwrap();
    let run = e.wait_end(&run.id);
    assert_eq!(run.status, RunStatus::Succeeded, "{run:#?}");
    assert_eq!(run.harness.preset, AcpPreset::Custom);
    let env_dump = std::fs::read_to_string(&dump).unwrap();
    assert!(env_dump.contains(secret), "{env_dump}");
    assert!(env_dump.contains("two words"), "{env_dump}");
    let platform_path = LocalProbe::new().command_path_env();
    let path_line = path_var_line(&env_dump);
    assert!(
        path_line.contains(&platform_path.to_string_lossy().into_owned()),
        "{path_line}"
    );
    let args = std::fs::read_to_string(format!("{}.args", dump.display())).unwrap();
    assert_eq!(args.lines().collect::<Vec<_>>(), ["--stdio", "--verbose"]);

    // 档案写了 PATH：以档案为准
    let h = make("custom-path", &path_profile);
    e.ts.bind_role_harness(&e.role.id, &h.id).unwrap();
    let task = e.task("PATH 覆盖");
    let run = e.ts.start_run(&task.id).unwrap();
    let run = e.wait_end(&run.id);
    assert_eq!(run.status, RunStatus::Succeeded, "{run:#?}");
    let env_dump = std::fs::read_to_string(&dump).unwrap();
    let path_line = path_var_line(&env_dump);
    assert!(path_line.contains("/custom"), "{path_line}");
    assert!(!path_line.contains("/usr/bin"), "{path_line}");

    // 档案里的值不落入数据库、进度与日志
    let mut haystack = String::new();
    for run in e.ts.list_runs(&task.id).unwrap() {
        haystack.push_str(&format!("{run:?}"));
        haystack.push_str(&format!("{:?}", e.updates(&run.id)));
    }
    for entry in std::fs::read_dir(e.dir.path().join("data")).unwrap() {
        let path = entry.unwrap().path();
        if path.is_file() {
            haystack.push_str(&String::from_utf8_lossy(&std::fs::read(&path).unwrap()));
        }
    }
    haystack.push_str(&e.wait_journal("PATH 覆盖"));
    haystack.push_str(&std::fs::read_to_string(e.role_file("岗位说明.md")).unwrap());
    assert!(!haystack.contains(secret), "环境档案里的值泄露了");
    let _ = PathBuf::new();
}
