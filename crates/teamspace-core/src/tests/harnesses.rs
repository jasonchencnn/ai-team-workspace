use super::*;

fn input(team_id: &str, name: &str, preset: AcpPreset) -> CreateHarnessInput {
    CreateHarnessInput {
        team_id: team_id.into(),
        name: name.into(),
        preset,
        command: None,
        args: None,
        env_profile: None,
    }
}

fn create_with(
    f: &Fixture,
    team_id: &str,
    name: &str,
    command: Option<&str>,
) -> Result<HarnessInstance> {
    f.ts.create_harness(CreateHarnessInput {
        command: command.map(Into::into),
        ..input(team_id, name, AcpPreset::Custom)
    })
}

fn update(f: &Fixture, h: &HarnessInstance, name: &str, command: Option<&str>) -> HarnessInstance {
    f.ts.update_harness(UpdateHarnessInput {
        harness_id: h.id.clone(),
        name: name.into(),
        command: command.map(Into::into),
        args: None,
        env_profile: None,
    })
    .unwrap()
}

#[test]
fn presets_fill_default_command_and_args() {
    let f = fixture();
    let team = f.team("T");
    let cases = [
        (AcpPreset::ClaudeCode, "claude-code-acp", vec![]),
        (AcpPreset::Codex, "codex-acp", vec![]),
        (AcpPreset::MinimaxCode, "mcode", vec!["acp"]),
        (AcpPreset::GeminiCli, "gemini", vec!["--experimental-acp"]),
        (AcpPreset::Mock, "teamspace-mock-agent", vec![]),
    ];
    for (i, (preset, command, args)) in cases.into_iter().enumerate() {
        let h =
            f.ts.create_harness(input(&team.id, &format!("h{i}"), preset))
                .unwrap();
        assert_eq!(h.preset, preset);
        assert_eq!(h.command, command);
        assert_eq!(h.args, args);
        assert_eq!(h.env_profile, None);
        assert_eq!(h.handshake, Handshake::unknown());
    }
}

#[test]
fn create_normalizes_fields_and_keeps_explicit_args() {
    let f = fixture();
    let team = f.team("T");
    let h =
        f.ts.create_harness(CreateHarnessInput {
            command: Some("  /opt/homebrew/bin/mcode ".into()),
            args: Some(vec![]),
            env_profile: Some(" /Users/me/.config/mcode.env ".into()),
            ..input(&team.id, " MiniMax Code ", AcpPreset::MinimaxCode)
        })
        .unwrap();
    assert_eq!(h.name, "MiniMax Code");
    assert_eq!(h.command, "/opt/homebrew/bin/mcode");
    assert!(h.args.is_empty(), "显式传空数组表示不带参数");
    assert_eq!(
        h.env_profile.as_deref(),
        Some("/Users/me/.config/mcode.env")
    );
    assert_eq!(f.ts.list_harnesses(&team.id).unwrap(), vec![h]);
}

#[test]
fn mock_command_is_fixed() {
    let f = fixture();
    let team = f.team("T");
    let h =
        f.ts.create_harness(CreateHarnessInput {
            command: Some("/bin/sh".into()),
            args: Some(vec!["-c".into(), "rm -rf /".into()]),
            ..input(&team.id, "mock", AcpPreset::Mock)
        })
        .unwrap();
    assert_eq!(h.command, "teamspace-mock-agent");
    assert!(h.args.is_empty());
    let h = update(&f, &h, "mock", Some("/bin/sh"));
    assert_eq!(h.command, "teamspace-mock-agent");
}

#[test]
fn custom_requires_valid_command() {
    let f = fixture();
    let team = f.team("T");
    let msg = assert_kind(create_with(&f, &team.id, "a", None), ErrorKind::Invalid);
    assert!(msg.contains("需要填写启动命令"), "{msg}");
    assert_kind(
        create_with(&f, &team.id, "a", Some("  ")),
        ErrorKind::Invalid,
    );
    assert_kind(
        create_with(&f, &team.id, "a", Some("bin/agent")),
        ErrorKind::Invalid,
    );
    let msg = assert_kind(
        create_with(&f, &team.id, "a", Some("mcode acp")),
        ErrorKind::Invalid,
    );
    assert!(msg.contains("参数请填在「参数」里"), "{msg}");
    assert_kind(
        create_with(&f, &team.id, "a", Some("agent\u{7}")),
        ErrorKind::Invalid,
    );
    let h = create_with(&f, &team.id, "a", Some("/Applications/My Agent/agent")).unwrap();
    assert_eq!(h.command, "/Applications/My Agent/agent");
    let h = create_with(&f, &team.id, "b", Some("my-agent")).unwrap();
    assert_eq!(h.command, "my-agent");
}

#[test]
fn args_and_env_profile_are_validated() {
    let f = fixture();
    let team = f.team("T");
    let with = |args: Vec<String>, env: Option<&str>| {
        f.ts.create_harness(CreateHarnessInput {
            args: Some(args),
            env_profile: env.map(Into::into),
            ..input(&team.id, "x", AcpPreset::GeminiCli)
        })
    };
    assert_kind(with(vec!["a\nb".into()], None), ErrorKind::Invalid);
    assert_kind(with(vec!["a".into(); 65], None), ErrorKind::Invalid);
    let msg = assert_kind(
        with(vec![], Some("~/.config/agent.env")),
        ErrorKind::Invalid,
    );
    assert!(msg.contains("绝对路径"), "{msg}");
    with(vec!["a".into(); 64], Some("/etc/agent.env")).unwrap();
}

#[test]
fn name_is_required_and_unique_per_team() {
    let f = fixture();
    let team = f.team("T");
    assert_kind(
        f.ts.create_harness(input(&team.id, " ", AcpPreset::Mock)),
        ErrorKind::Invalid,
    );
    f.harness(&team.id, "local", AcpPreset::Mock);
    assert_kind(
        f.ts.create_harness(input(&team.id, "local", AcpPreset::Codex)),
        ErrorKind::Conflict,
    );
    let other = f.team("U");
    f.harness(&other.id, "local", AcpPreset::Mock);
}

#[test]
fn check_runs_handshake_persists_result_and_update_resets_it() {
    let f = fixture();
    let team = f.team("T");
    let h = f.harness(&team.id, "a", AcpPreset::MinimaxCode);
    let checked = f.ts.check_harness(&h.id).unwrap();
    let launched = LAUNCHED.with(|l| l.borrow().last().cloned()).unwrap();
    assert_eq!(launched.command, "mcode");
    assert_eq!(launched.args, ["acp"]);
    assert_eq!(checked.handshake.status, HandshakeStatus::Connected);
    assert_eq!(checked.handshake.protocol_version, Some(1));
    assert_eq!(checked.handshake.agent.as_ref().unwrap().name, "mcode");
    assert!(
        checked
            .handshake
            .capabilities
            .as_ref()
            .unwrap()
            .load_session
    );
    assert!(checked.handshake.checked_at.is_some());
    assert_eq!(checked.updated_at, h.updated_at);
    assert_eq!(f.ts.list_harnesses(&team.id).unwrap()[0], checked);
    assert_eq!(f.ts.get_harness(&h.id).unwrap(), checked);
    let events = f.ts.get_team_overview(&team.id).unwrap().recent_events;
    assert_eq!(events[0].kind, EventKind::HarnessChecked);
    assert!(
        events[0].summary.contains("已连接（mcode 0.9.2）"),
        "{}",
        events[0].summary
    );

    let updated = update(&f, &h, "b", Some(BROKEN_COMMAND));
    assert_eq!(updated.name, "b");
    assert_eq!(updated.preset, AcpPreset::MinimaxCode);
    assert_eq!(updated.args, ["acp"], "参数缺省时回到预设默认");
    assert_eq!(updated.handshake, Handshake::unknown());

    // 握手失败不是命令错误。
    let failed = f.ts.check_harness(&h.id).unwrap();
    assert_eq!(failed.handshake.status, HandshakeStatus::Error);
    assert!(failed.handshake.agent.is_none());
    assert!(failed.handshake.detail.is_some());
    let events = f.ts.get_team_overview(&team.id).unwrap().recent_events;
    assert!(events[0].summary.contains("异常"), "{}", events[0].summary);
}

#[test]
fn check_reports_needs_auth() {
    let f = fixture();
    let team = f.team("T");
    let h = create_with(&f, &team.id, "codex", Some(NEEDS_AUTH_COMMAND)).unwrap();
    let checked = f.ts.check_harness(&h.id).unwrap();
    assert_eq!(checked.handshake.status, HandshakeStatus::NeedsAuth);
    assert_eq!(checked.handshake.auth_methods, ["ChatGPT 登录"]);
    let events = f.ts.get_team_overview(&team.id).unwrap().recent_events;
    assert!(
        events[0].summary.contains("需要登录"),
        "{}",
        events[0].summary
    );
}

#[test]
fn check_passes_env_profile_to_probe() {
    let f = fixture();
    let team = f.team("T");
    let h =
        f.ts.create_harness(CreateHarnessInput {
            env_profile: Some("/etc/agent.env".into()),
            ..input(&team.id, "g", AcpPreset::GeminiCli)
        })
        .unwrap();
    f.ts.check_harness(&h.id).unwrap();
    let launched = LAUNCHED.with(|l| l.borrow().last().cloned()).unwrap();
    assert_eq!(launched.env_profile.as_deref(), Some("/etc/agent.env"));
    assert_eq!(launched.args, ["--experimental-acp"]);
}

#[test]
fn mock_check_launches_host_injected_agent() {
    let f = fixture();
    let team = f.team("T");
    let h = f.harness(&team.id, "mock", AcpPreset::Mock);
    let checked = f.ts.check_harness(&h.id).unwrap();
    let launched = LAUNCHED.with(|l| l.borrow().last().cloned()).unwrap();
    assert_eq!(launched.command, MOCK_HOST);
    assert_eq!(launched.args, [crate::mock_agent::MOCK_AGENT_FLAG]);
    assert_eq!(checked.handshake.status, HandshakeStatus::Connected);

    // 宿主没有提供 Mock Agent 时握手报错，不启动任何程序。
    let path = f.db_path();
    drop(f.ts);
    let ts = Teamspace::open(&path, Box::new(FakeProbe)).unwrap();
    let before = LAUNCHED.with(|l| l.borrow().len());
    let checked = ts.check_harness(&h.id).unwrap();
    assert_eq!(checked.handshake.status, HandshakeStatus::Error);
    assert!(checked.handshake.detail.unwrap().contains("Mock Agent"));
    assert_eq!(LAUNCHED.with(|l| l.borrow().len()), before);
}

#[test]
fn presets_report_local_detection_and_hints() {
    let f = fixture();
    let presets = f.ts.list_acp_presets();
    let order: Vec<_> = presets.iter().map(|p| p.preset).collect();
    assert_eq!(order, AcpPreset::ALL);
    let get = |preset| presets.iter().find(|p| p.preset == preset).unwrap();

    // 假探测：claude-code-acp 在，claude 不在。
    let claude = get(AcpPreset::ClaudeCode);
    assert_eq!(claude.default_command.as_deref(), Some("claude-code-acp"));
    assert_eq!(
        claude.command_path.as_deref(),
        Some("/usr/local/bin/claude-code-acp")
    );
    assert!(claude.command_found);
    assert_eq!(claude.requires_cli.as_deref(), Some("claude"));
    assert_eq!(claude.cli_found, Some(false));
    let hint = claude.hint.as_deref().unwrap();
    assert!(
        hint.contains("需要安装 Claude Code（claude 命令）"),
        "{hint}"
    );
    assert!(!hint.contains("适配器"), "{hint}");

    // codex-acp 不在，codex 在。
    let codex = get(AcpPreset::Codex);
    assert!(!codex.command_found);
    assert_eq!(codex.command_path, None);
    assert_eq!(codex.cli_found, Some(true));
    let hint = codex.hint.as_deref().unwrap();
    assert!(hint.contains("@zed-industries/codex-acp"), "{hint}");

    let minimax = get(AcpPreset::MinimaxCode);
    assert_eq!(minimax.default_args, ["acp"]);
    assert!(minimax.command_found);
    assert_eq!(minimax.requires_cli, None);
    assert_eq!(minimax.cli_found, None);
    assert_eq!(minimax.hint, None);

    let gemini = get(AcpPreset::GeminiCli);
    assert_eq!(gemini.default_args, ["--experimental-acp"]);
    assert!(!gemini.command_found);
    assert!(gemini.hint.as_deref().unwrap().contains("Gemini CLI"));

    let custom = get(AcpPreset::Custom);
    assert_eq!(custom.default_command, None);
    assert!(!custom.command_found);
    assert_eq!(custom.hint, None);

    let mock = get(AcpPreset::Mock);
    assert_eq!(
        mock.default_command.as_deref(),
        Some("teamspace-mock-agent")
    );
    assert_eq!(mock.command_path.as_deref(), Some(MOCK_HOST));
    assert!(mock.command_found);
    assert_eq!(mock.hint, None);
}

#[test]
fn bound_harness_cannot_be_archived() {
    let f = fixture();
    let team = f.team("T");
    let h = f.harness(&team.id, "frontend-local", AcpPreset::ClaudeCode);
    let fe = f.role(&team.id, "role_frontend");
    let qa = f.role(&team.id, "role_qa");
    f.ts.bind_role_harness(&fe.id, &h.id).unwrap();
    f.ts.bind_role_harness(&qa.id, &h.id).unwrap();
    let msg = assert_kind(f.ts.archive_harness(&h.id), ErrorKind::Conflict);
    assert!(msg.contains("前端开发") && msg.contains("QA 测试"), "{msg}");

    f.ts.unbind_role_harness(&fe.id).unwrap();
    f.ts.unbind_role_harness(&qa.id).unwrap();
    let archived = f.ts.archive_harness(&h.id).unwrap();
    assert!(archived.archived_at.is_some());

    // 归档后不可见、不可操作，名称可以复用。
    assert!(f.ts.list_harnesses(&team.id).unwrap().is_empty());
    assert_kind(f.ts.check_harness(&h.id), ErrorKind::NotFound);
    assert_kind(f.ts.get_harness(&h.id), ErrorKind::NotFound);
    assert_kind(f.ts.archive_harness(&h.id), ErrorKind::NotFound);
    f.harness(&team.id, "frontend-local", AcpPreset::Codex);
}

#[test]
fn harness_bound_only_to_archived_role_can_be_archived() {
    let f = fixture();
    let team = f.team("T");
    let h = f.harness(&team.id, "h", AcpPreset::Mock);
    let pm = f.role(&team.id, "role_product");
    f.ts.bind_role_harness(&pm.id, &h.id).unwrap();
    f.ts.archive_role(&pm.id).unwrap();
    f.ts.archive_harness(&h.id).unwrap();
}

#[test]
fn harness_name_length_is_checked() {
    let f = fixture();
    let team = f.team("T");
    assert_kind(
        f.ts.create_harness(input(&team.id, &"h".repeat(101), AcpPreset::Mock)),
        ErrorKind::Invalid,
    );
    assert_kind(
        f.ts.create_harness(input(&team.id, "a\tb\u{0}", AcpPreset::Mock)),
        ErrorKind::Invalid,
    );
    f.harness(&team.id, &"h".repeat(100), AcpPreset::Mock);
}
