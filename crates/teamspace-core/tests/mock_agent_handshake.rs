//! Mock Agent 的真实子进程握手：启动 cargo 构建的 `teamspace-mock-agent`，走完整的 ACP 客户端与
//! 本机探测（PATH、进程组、临时工作目录），以及内核 `check_harness` 的 Mock 预设路径。

use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use std::time::Duration;

use teamspace_core::acp::{self, AgentLaunch};
use teamspace_core::model::{AcpPreset, CreateHarnessInput, CreateTeamInput, HandshakeStatus};
use teamspace_core::system_probe::LocalProbe;
use teamspace_core::{AgentCommand, HostConfig, SystemProbe, Teamspace};

fn mock_agent() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_teamspace-mock-agent"))
}

#[test]
fn acp_client_handshakes_with_mock_agent_process() {
    let dir = tempfile::tempdir().unwrap();
    let hs = acp::handshake_process(
        &mock_agent(),
        &[],
        dir.path(),
        OsStr::new("/usr/bin:/bin"),
        Duration::from_secs(20),
    );
    assert_eq!(hs.status, HandshakeStatus::Connected, "{:?}", hs.detail);
    assert_eq!(hs.protocol_version, Some(1));
    let agent = hs.agent.unwrap();
    assert_eq!(agent.name, "teamspace-mock-agent");
    assert_eq!(agent.version.as_deref(), Some(env!("CARGO_PKG_VERSION")));
    assert!(hs.auth_methods.is_empty());
    assert!(!hs.capabilities.unwrap().load_session);
}

#[test]
fn local_probe_handshakes_with_mock_agent_by_absolute_path() {
    let probe = LocalProbe::new();
    let hs = probe.handshake(&AgentLaunch {
        command: mock_agent().display().to_string(),
        args: vec![],
        env_profile: None,
    });
    assert_eq!(hs.status, HandshakeStatus::Connected, "{:?}", hs.detail);
}

#[test]
fn check_harness_with_mock_preset_launches_host_agent() {
    let dir = tempfile::tempdir().unwrap();
    let ws = dir.path().join("ws");
    std::fs::create_dir_all(&ws).unwrap();
    let config = HostConfig {
        mock_agent: Some(AgentCommand {
            program: mock_agent(),
            args: vec![],
        }),
    };
    let ts = Teamspace::open_with(
        &dir.path().join("t.db"),
        Box::new(LocalProbe::new()),
        config,
    )
    .unwrap();
    let team = ts
        .create_team(CreateTeamInput {
            name: "T".into(),
            description: String::new(),
            workspace_path: ws.display().to_string(),
            roles: vec![],
        })
        .unwrap();
    let h = ts
        .create_harness(CreateHarnessInput {
            team_id: team.id,
            name: "Mock（测试用）".into(),
            preset: AcpPreset::Mock,
            command: None,
            args: None,
            env_profile: None,
        })
        .unwrap();
    let checked = ts.check_harness(&h.id).unwrap();
    assert_eq!(
        checked.handshake.status,
        HandshakeStatus::Connected,
        "{:?}",
        checked.handshake.detail
    );
    assert_eq!(
        checked.handshake.agent.unwrap().name,
        "teamspace-mock-agent"
    );
    assert!(checked.handshake.checked_at.is_some());
    assert_eq!(checked.command, "teamspace-mock-agent");

    let presets = ts.list_acp_presets();
    let mock = presets
        .iter()
        .find(|p| p.preset == AcpPreset::Mock)
        .unwrap();
    assert!(mock.command_found);
    assert_eq!(
        mock.command_path.as_deref().map(Path::new),
        Some(mock_agent().as_path())
    );
}
