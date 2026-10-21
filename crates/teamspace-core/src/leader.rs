//! Leader 接入（M4-03）：为外部会话（Claude Code / Codex）生成 MCP 配置，并按需写入工作空间的 `.mcp.json`。
//!
//! 三段配置都由内核生成，路径里的空格与特殊字符按各自格式转义：
//! - Claude Code：`claude mcp add teamspace -- <二进制路径>`（shell 规则）；
//! - 工作空间 `.mcp.json`：项目级配置（JSON 规则）；
//! - Codex `~/.codex/config.toml`：`[mcp_servers.teamspace]` 片段（TOML 规则）。
//!
//! 可执行文件的位置：开发模式是 cargo 的 `target/debug/teamspace-mcp` 或 `target/release/teamspace-mcp`
//! （存在的那个），打包后是应用包内的路径；都找不到时给出构建提示。
//!
//! 权限边界不变（D26）：这里只是生成配置与写一个工作空间文件，不扩大 Leader 的能力。

use std::path::{Path, PathBuf};

use serde_json::{Value, json};

use crate::Teamspace;
use crate::error::{Error, Result};
use crate::model::{LeaderIntegration, McpConfigWrite, Team};
use crate::service::team;

/// MCP 服务器名（三段配置里都用它）。
pub const MCP_SERVER_NAME: &str = "teamspace";
/// `teamspace-mcp` 可执行文件名。
pub const MCP_BINARY_NAME: &str = "teamspace-mcp";
/// 工作空间里的 MCP 配置文件名（Claude Code 项目级配置）。
pub const MCP_CONFIG_FILE: &str = ".mcp.json";
/// 控制 socket 的文件名（放在应用数据目录下的 `control/` 子目录里）。
pub const SOCKET_FILE_NAME: &str = "teamspace-control.sock";
/// 控制 socket 所在子目录（放在应用数据目录下，权限 0700：只有当前用户能进）。
pub const SOCKET_DIR_NAME: &str = "control";
/// 环境变量名：覆盖控制 socket 的路径。
pub const SOCKET_ENV: &str = "TEAMSPACE_CONTROL_SOCKET";
/// 找不到可执行文件时的提示。
pub const BUILD_HINT: &str = "请先构建 teamspace-mcp：cargo build -p teamspace-mcp";
/// 桌面应用标识（macOS 上是应用数据目录名）。
pub const APP_IDENTIFIER: &str = "com.ai-team-workspace.desktop";

/// 应用数据目录：macOS 是 `~/Library/Application Support/<应用标识>`，
/// Linux 是 `$XDG_DATA_HOME/<应用标识>`（缺省 `~/.local/share/<应用标识>`）。
///
/// 与 `crates/teamspace-mcp` 里的同名逻辑保持一致（那个 crate 不能依赖本 crate，只能各写一份）。
pub fn app_data_dir() -> PathBuf {
    #[cfg(target_os = "macos")]
    {
        if let Some(home) = std::env::var_os("HOME").map(PathBuf::from) {
            return home
                .join("Library")
                .join("Application Support")
                .join(APP_IDENTIFIER);
        }
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        if let Some(dir) = std::env::var_os("XDG_DATA_HOME").map(PathBuf::from) {
            return dir.join(APP_IDENTIFIER);
        }
        if let Some(home) = std::env::var_os("HOME").map(PathBuf::from) {
            return home.join(".local").join("share").join(APP_IDENTIFIER);
        }
    }
    PathBuf::from(APP_IDENTIFIER)
}

/// 控制 socket 的默认路径（应用数据目录下的 `teamspace-control.sock`）。
pub fn default_socket_path() -> PathBuf {
    app_data_dir().join(SOCKET_DIR_NAME).join(SOCKET_FILE_NAME)
}

/// `teamspace-mcp` 可执行文件的位置。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BinaryLocation {
    /// 报告给用户的绝对路径（文件不存在时是「应该在哪」的路径）。
    pub path: PathBuf,
    /// 文件当前是否存在。
    pub found: bool,
}

/// 找出 `teamspace-mcp` 可执行文件。
///
/// `exe` 是宿主程序的可执行文件路径（一般是 `target/<profile>/` 下的程序）：
/// 开发模式时它的同级目录就是 cargo 的产物目录，打包后是应用包内的可执行文件目录。
pub fn locate_binary(exe: Option<&Path>) -> BinaryLocation {
    let candidates = candidate_paths(exe);
    for path in &candidates {
        if path.is_file() {
            return BinaryLocation {
                path: path.clone(),
                found: true,
            };
        }
    }
    BinaryLocation {
        path: candidates
            .first()
            .cloned()
            .unwrap_or_else(|| PathBuf::from(MCP_BINARY_NAME)),
        found: false,
    }
}

/// 候选路径：打包后是应用包内的可执行文件，开发模式是 cargo 的 `target/<profile>/` 产物。
fn candidate_paths(exe: Option<&Path>) -> Vec<PathBuf> {
    let mut paths = Vec::new();
    let Some(dir) = exe.and_then(Path::parent) else {
        return paths;
    };
    // 打包后：可执行文件旁边（macOS 是 Contents/MacOS/）。
    paths.push(dir.join(MCP_BINARY_NAME));
    let Some(parent) = dir.parent() else {
        return paths;
    };
    // 打包后：应用包的 Resources 目录里。
    paths.push(parent.join("Resources").join(MCP_BINARY_NAME));
    paths.push(parent.join("Resources/bin").join(MCP_BINARY_NAME));
    // 开发模式：cargo 的 target/<profile>/ 目录（debug 与 release，debug 在前）。
    for profile in ["debug", "release"] {
        paths.push(parent.join(profile).join(MCP_BINARY_NAME));
    }
    paths
}

/// shell 单引号转义：只含安全字符时原样返回，否则用单引号包起来（内部单引号写成 `'\''`）。
pub fn shell_quote(value: &str) -> String {
    let safe = !value.is_empty()
        && value
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || "-_./:=@%+,".contains(ch));
    if safe {
        return value.to_owned();
    }
    format!("'{}'", value.replace('\'', "'\\''"))
}

/// TOML 基本字符串转义（`"..."` 里的引号、反斜杠与控制字符）。
pub fn toml_quote(value: &str) -> String {
    let mut out = String::with_capacity(value.len() + 2);
    out.push('"');
    for ch in value.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '\u{8}' => out.push_str("\\b"),
            '\u{c}' => out.push_str("\\f"),
            ch if (ch as u32) < 0x20 || ch == '\u{7f}' => {
                out.push_str(&format!("\\u{:04X}", ch as u32));
            }
            ch => out.push(ch),
        }
    }
    out.push('"');
    out
}

/// MCP 服务器条目（`.mcp.json` 与 `config.toml` 共用）。
///
/// `socket` 为 `None` 表示控制 socket 在默认位置，配置里不需要带环境变量。
fn server_entry(binary: &str, socket: Option<&str>) -> Value {
    let mut entry = json!({ "command": binary });
    if let Some(socket) = socket {
        entry["env"] = json!({ SOCKET_ENV: socket });
    }
    entry
}

/// Claude Code 的登记命令。
pub fn claude_command(binary: &str, socket: Option<&str>) -> String {
    let mut command = format!("claude mcp add {MCP_SERVER_NAME}");
    if let Some(socket) = socket {
        let assignment = format!("{SOCKET_ENV}={socket}");
        command.push_str(&format!(" -e {}", shell_quote(&assignment)));
    }
    command.push_str(&format!(" -- {}", shell_quote(binary)));
    command
}

/// 工作空间 `.mcp.json` 片段（Claude Code 项目级配置）。
pub fn mcp_json_fragment(binary: &str, socket: Option<&str>) -> String {
    let document = json!({
        "mcpServers": { MCP_SERVER_NAME: server_entry(binary, socket) },
    });
    serde_json::to_string_pretty(&document).unwrap_or_default()
}

/// Codex `~/.codex/config.toml` 片段。
pub fn codex_toml_fragment(binary: &str, socket: Option<&str>) -> String {
    let mut out = format!(
        "[mcp_servers.{MCP_SERVER_NAME}]\ncommand = {}\n",
        toml_quote(binary)
    );
    if let Some(socket) = socket {
        out.push_str(&format!(
            "\n[mcp_servers.{MCP_SERVER_NAME}.env]\n{SOCKET_ENV} = {}\n",
            toml_quote(socket)
        ));
    }
    out
}

/// 把 `mcpServers.teamspace` 一项合并进已有的 `.mcp.json` 文本。
///
/// `existing` 为 `None` 表示新建。已有内容解析不了（或顶层不是对象、`mcpServers` 不是对象）时
/// 返回错误，**不覆盖原文件**。其余内容原样保留（文件会被重新按规范缩进写出）。
pub fn merge_mcp_config(existing: Option<&str>, entry: &Value) -> Result<String> {
    let mut document = match existing {
        None => json!({}),
        Some(text) => {
            let value: Value = serde_json::from_str(text).map_err(|err| {
                Error::invalid(format!(
                    "工作空间的 {MCP_CONFIG_FILE} 不是合法 JSON，没有修改它：{err}"
                ))
            })?;
            if !value.is_object() {
                return Err(Error::invalid(format!(
                    "工作空间的 {MCP_CONFIG_FILE} 顶层不是 JSON 对象，没有修改它"
                )));
            }
            value
        }
    };
    let servers = document
        .as_object_mut()
        .expect("顶层已确认为对象")
        .entry("mcpServers")
        .or_insert_with(|| json!({}));
    if !servers.is_object() {
        return Err(Error::invalid(format!(
            "工作空间的 {MCP_CONFIG_FILE} 里的 mcpServers 不是对象，没有修改它"
        )));
    }
    servers
        .as_object_mut()
        .expect("mcpServers 已确认为对象")
        .insert(MCP_SERVER_NAME.to_owned(), entry.clone());
    let mut text = serde_json::to_string_pretty(&document)
        .map_err(|err| Error::internal(format!("生成 {MCP_CONFIG_FILE} 失败：{err}")))?;
    text.push('\n');
    Ok(text)
}

/// Leader 接入需要的内部信息。
struct LeaderContext {
    team: Team,
    /// 控制 socket 的绝对路径。
    socket: PathBuf,
    /// socket 不在默认位置时为 `Some`（配置里要带环境变量）。
    socket_override: Option<String>,
    binary: BinaryLocation,
}

impl Teamspace {
    fn leader_context(&self, team_id: &str) -> Result<LeaderContext> {
        let team = team::get_active(&self.lock(), team_id)?;
        let socket = self.data_dir.join(SOCKET_DIR_NAME).join(SOCKET_FILE_NAME);
        let socket_override =
            (socket != default_socket_path()).then(|| socket.display().to_string());
        let binary = locate_binary(std::env::current_exe().ok().as_deref());
        Ok(LeaderContext {
            team,
            socket,
            socket_override,
            binary,
        })
    }

    /// Leader 接入配置：`teamspace-mcp` 可执行文件的绝对路径、控制 socket 路径、工作空间路径，
    /// 以及三段可直接复制的配置文本（Claude Code 命令、工作空间 `.mcp.json` 片段、
    /// Codex `config.toml` 片段）。
    ///
    /// 找不到可执行文件时 `binaryHint` 给出构建命令，配置文本仍按「应该在哪」的路径生成。
    /// 工作空间是 Git 仓库时 `gitWorkspace` 为真，界面据此提示文件会出现在 Git 改动里。
    pub fn leader_integration(&self, team_id: &str) -> Result<LeaderIntegration> {
        let context = self.leader_context(team_id)?;
        let binary_path = context.binary.path.display().to_string();
        let socket = context.socket_override.as_deref();
        // 控制 socket 起不来时（宿主应用监听失败）：应用照常运行，只是 Leader 连不上。
        let socket_error = self
            .control_socket_error
            .read()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clone();
        Ok(LeaderIntegration {
            team_id: context.team.id,
            binary_found: context.binary.found,
            binary_hint: (!context.binary.found).then(|| BUILD_HINT.to_owned()),
            binary_path: binary_path.clone(),
            control_socket: context.socket.display().to_string(),
            control_socket_ready: socket_error.is_none(),
            control_socket_hint: socket_error,
            workspace_path: context.team.workspace_path.clone(),
            git_workspace: self.is_git_workspace(&context.team.workspace_path),
            claude_command: claude_command(&binary_path, socket),
            mcp_json: mcp_json_fragment(&binary_path, socket),
            codex_toml: codex_toml_fragment(&binary_path, socket),
        })
    }

    /// 把 `mcpServers.teamspace` 合并进工作空间的 `.mcp.json`（只在你点确认后调用）。
    ///
    /// 文件不存在时新建；已存在时只替换 `mcpServers.teamspace` 一项，其余内容保留。
    /// 原文件不是合法 JSON 时报错，不覆盖它。
    pub fn write_workspace_mcp_config(&self, team_id: &str) -> Result<McpConfigWrite> {
        let context = self.leader_context(team_id)?;
        let target = PathBuf::from(&context.team.workspace_path).join(MCP_CONFIG_FILE);
        let entry = server_entry(
            &context.binary.path.display().to_string(),
            context.socket_override.as_deref(),
        );
        let existing = match std::fs::read_to_string(&target) {
            Ok(text) => Some(text),
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => None,
            Err(err) => {
                return Err(Error::internal(format!(
                    "读取 {} 失败：{err}",
                    target.display()
                )));
            }
        };
        let text = merge_mcp_config(existing.as_deref(), &entry)?;
        std::fs::write(&target, text.as_bytes())
            .map_err(|err| Error::internal(format!("写入 {} 失败：{err}", target.display())))?;
        Ok(McpConfigWrite {
            path: target.display().to_string(),
            created: existing.is_none(),
            git_workspace: self.is_git_workspace(&context.team.workspace_path),
        })
    }

    /// 工作空间是不是 Git 仓库（检测失败时按「不是」处理）。
    fn is_git_workspace(&self, workspace_path: &str) -> bool {
        self.inspect(workspace_path)
            .ok()
            .and_then(|info| info.git)
            .is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests::plain_fixture;

    /// shell 转义：普通路径原样返回，含空格与引号的路径被正确包起来。
    #[test]
    fn shell_quote_only_quotes_when_needed() {
        assert_eq!(shell_quote("/opt/teamspace-mcp"), "/opt/teamspace-mcp");
        assert_eq!(
            shell_quote("/Users/me/My Projects/teamspace-mcp"),
            "'/Users/me/My Projects/teamspace-mcp'"
        );
        assert_eq!(shell_quote("it's"), "'it'\\''s'");
        assert_eq!(shell_quote(""), "''");
        assert_eq!(shell_quote("/a b;rm -rf /"), "'/a b;rm -rf /'");
    }

    /// TOML 转义：引号、反斜杠与控制字符都有合法写法。
    #[test]
    fn toml_quote_escapes_special_characters() {
        assert_eq!(toml_quote("/opt/teamspace-mcp"), "\"/opt/teamspace-mcp\"");
        assert_eq!(toml_quote("a\"b"), "\"a\\\"b\"");
        assert_eq!(toml_quote("a\\b"), "\"a\\\\b\"");
        assert_eq!(toml_quote("a\nb"), "\"a\\nb\"");
        assert_eq!(toml_quote("a\u{1}b"), "\"a\\u0001b\"");
    }

    /// Claude Code 命令：默认 socket 不带环境变量，非默认 socket 带 `-e`。
    #[test]
    fn claude_command_adds_env_only_for_a_custom_socket() {
        assert_eq!(
            claude_command("/opt/teamspace-mcp", None),
            "claude mcp add teamspace -- /opt/teamspace-mcp"
        );
        assert_eq!(
            claude_command(
                "/opt/teamspace-mcp",
                Some("/tmp/my sock/teamspace-control.sock")
            ),
            "claude mcp add teamspace -e 'TEAMSPACE_CONTROL_SOCKET=/tmp/my sock/teamspace-control.sock' \
             -- /opt/teamspace-mcp"
        );
    }

    /// `.mcp.json` 片段是合法 JSON，含命令与（需要时的）环境变量。
    #[test]
    fn mcp_json_fragment_is_valid_json() {
        let text = mcp_json_fragment("/opt/teamspace-mcp", Some("/tmp/s.sock"));
        let value: Value = serde_json::from_str(&text).unwrap();
        let server = &value["mcpServers"]["teamspace"];
        assert_eq!(server["command"], "/opt/teamspace-mcp");
        assert_eq!(server["env"]["TEAMSPACE_CONTROL_SOCKET"], "/tmp/s.sock");

        let plain: Value =
            serde_json::from_str(&mcp_json_fragment("/opt/teamspace-mcp", None)).unwrap();
        assert!(plain["mcpServers"]["teamspace"].get("env").is_none());
    }

    /// Codex 片段是 TOML 表格，含命令与（需要时的）环境变量表。
    #[test]
    fn codex_toml_fragment_has_server_and_env_tables() {
        let text = codex_toml_fragment("/opt/teamspace-mcp", Some("/tmp/s.sock"));
        assert!(text.contains("[mcp_servers.teamspace]"), "{text}");
        assert!(text.contains("command = \"/opt/teamspace-mcp\""), "{text}");
        assert!(text.contains("[mcp_servers.teamspace.env]"), "{text}");
        assert!(
            text.contains("TEAMSPACE_CONTROL_SOCKET = \"/tmp/s.sock\""),
            "{text}"
        );

        let plain = codex_toml_fragment("/opt/teamspace-mcp", None);
        assert!(!plain.contains("env"), "{plain}");
    }

    /// 合并：保留其他内容，只替换 `mcpServers.teamspace`。
    #[test]
    fn merge_keeps_other_content() {
        let existing = r#"{
  "mcpServers": {
    "other": { "command": "/bin/other" }
  },
  "extra": { "keep": true }
}"#;
        let entry = server_entry("/opt/teamspace-mcp", Some("/tmp/s.sock"));
        let merged: Value =
            serde_json::from_str(&merge_mcp_config(Some(existing), &entry).unwrap()).unwrap();
        assert_eq!(merged["mcpServers"]["other"]["command"], "/bin/other");
        assert_eq!(merged["extra"]["keep"], true);
        assert_eq!(
            merged["mcpServers"]["teamspace"]["command"],
            "/opt/teamspace-mcp"
        );
        assert_eq!(
            merged["mcpServers"]["teamspace"]["env"]["TEAMSPACE_CONTROL_SOCKET"],
            "/tmp/s.sock"
        );
    }

    /// 新建：只有 `mcpServers.teamspace` 一项。
    #[test]
    fn merge_creates_a_fresh_document() {
        let entry = server_entry("/opt/teamspace-mcp", None);
        let merged: Value = serde_json::from_str(&merge_mcp_config(None, &entry).unwrap()).unwrap();
        assert_eq!(
            merged["mcpServers"]["teamspace"]["command"],
            "/opt/teamspace-mcp"
        );
        assert_eq!(merged.as_object().unwrap().len(), 1);
    }

    /// 原文件不是合法 JSON 时报错，不覆盖。
    #[test]
    fn merge_rejects_unparsable_json() {
        let entry = server_entry("/opt/teamspace-mcp", None);
        let err = merge_mcp_config(Some("{不是 JSON"), &entry).unwrap_err();
        assert_eq!(err.kind(), crate::error::ErrorKind::Invalid);
        assert!(err.to_string().contains("没有修改它"), "{err}");

        let err = merge_mcp_config(Some("[1, 2]"), &entry).unwrap_err();
        assert!(err.to_string().contains("顶层不是 JSON 对象"), "{err}");

        let err = merge_mcp_config(Some(r#"{"mcpServers": 3}"#), &entry).unwrap_err();
        assert!(err.to_string().contains("mcpServers 不是对象"), "{err}");
    }

    /// 可执行文件位置：开发模式取 `target/<profile>/` 下的产物，找到就用它。
    #[test]
    fn locate_binary_prefers_an_existing_candidate() {
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("target/debug");
        std::fs::create_dir_all(&target).unwrap();
        let exe = target.join("desktop");

        // 什么都没构建时：报告应该在哪，并说明没找到。
        let missing = locate_binary(Some(&exe));
        assert!(!missing.found);
        assert_eq!(missing.path, target.join("teamspace-mcp"));

        // 构建了 debug 产物时：找到它。
        std::fs::write(target.join("teamspace-mcp"), b"#!/bin/sh\n").unwrap();
        let found = locate_binary(Some(&exe));
        assert!(found.found);
        assert_eq!(found.path, target.join("teamspace-mcp"));

        // 只有 release 产物时：用 release 的那个。
        let release = dir.path().join("target/release");
        std::fs::create_dir_all(&release).unwrap();
        std::fs::remove_file(target.join("teamspace-mcp")).unwrap();
        std::fs::write(release.join("teamspace-mcp"), b"#!/bin/sh\n").unwrap();
        let release_only = locate_binary(Some(&exe));
        assert!(release_only.found);
        assert_eq!(release_only.path, release.join("teamspace-mcp"));
    }

    /// 接入配置：三段文本齐备，socket 路径与工作空间路径正确。
    #[test]
    fn leader_integration_returns_the_three_configurations() {
        let fixture = plain_fixture();
        let team = fixture.team("接入团队");
        let integration = fixture.ts.leader_integration(&team.id).unwrap();

        assert_eq!(integration.team_id, team.id);
        assert_eq!(integration.workspace_path, fixture.ws.to_string_lossy());
        assert!(integration.control_socket.ends_with(SOCKET_FILE_NAME));
        assert!(
            integration
                .claude_command
                .contains("claude mcp add teamspace")
        );
        assert!(integration.mcp_json.contains("mcpServers"));
        assert!(integration.codex_toml.contains("[mcp_servers.teamspace]"));
        // 测试用的数据目录不是默认位置，所以配置里要带 socket 环境变量。
        assert!(integration.claude_command.contains(SOCKET_ENV));
    }

    /// 控制 socket 不可用时（宿主应用监听失败），接入配置里带上原因：
    /// 界面「接入 Leader」处据此提示 Leader 连不上（应用本身照常运行）。
    #[test]
    fn leader_integration_reports_an_unavailable_control_socket() {
        let fixture = plain_fixture();
        let team = fixture.team("接入团队");
        let ok = fixture.ts.leader_integration(&team.id).unwrap();
        assert!(ok.control_socket_ready, "{ok:?}");
        assert!(ok.control_socket_hint.is_none(), "{ok:?}");
        // socket 在应用数据目录下的专用子目录里。
        assert!(
            Path::new(&ok.control_socket)
                .parent()
                .unwrap()
                .ends_with(SOCKET_DIR_NAME),
            "{}",
            ok.control_socket
        );

        fixture
            .ts
            .set_control_socket_error(Some("控制 socket 不可用（路径被普通文件占用）".into()));
        let bad = fixture.ts.leader_integration(&team.id).unwrap();
        assert!(!bad.control_socket_ready, "{bad:?}");
        assert!(
            bad.control_socket_hint
                .as_deref()
                .unwrap_or_default()
                .contains("不可用"),
            "{bad:?}"
        );
    }

    /// 写入 `.mcp.json`：新建后再合并，其他内容不丢。
    #[test]
    fn write_workspace_mcp_config_creates_then_merges() {
        let fixture = plain_fixture();
        let team = fixture.team("接入团队");
        let target = fixture.ws.join(MCP_CONFIG_FILE);

        let created = fixture.ts.write_workspace_mcp_config(&team.id).unwrap();
        assert!(created.created);
        assert_eq!(created.path, target.display().to_string());
        let value: Value =
            serde_json::from_str(&std::fs::read_to_string(&target).unwrap()).unwrap();
        assert!(value["mcpServers"]["teamspace"]["command"].is_string());

        // 已有其他内容时合并，不丢。
        std::fs::write(
            &target,
            r#"{"mcpServers":{"other":{"command":"/bin/other"}},"keep":1}"#,
        )
        .unwrap();
        let merged = fixture.ts.write_workspace_mcp_config(&team.id).unwrap();
        assert!(!merged.created);
        let value: Value =
            serde_json::from_str(&std::fs::read_to_string(&target).unwrap()).unwrap();
        assert_eq!(value["mcpServers"]["other"]["command"], "/bin/other");
        assert_eq!(value["keep"], 1);
        assert!(value["mcpServers"]["teamspace"]["command"].is_string());
    }

    /// 已有 `.mcp.json` 不是合法 JSON 时报错，文件内容不变。
    #[test]
    fn write_workspace_mcp_config_keeps_unparsable_file() {
        let fixture = plain_fixture();
        let team = fixture.team("接入团队");
        let target = fixture.ws.join(MCP_CONFIG_FILE);
        std::fs::write(&target, "{不是 JSON").unwrap();

        let err = fixture.ts.write_workspace_mcp_config(&team.id).unwrap_err();
        assert_eq!(err.kind(), crate::error::ErrorKind::Invalid);
        assert_eq!(std::fs::read_to_string(&target).unwrap(), "{不是 JSON");
    }

    /// Git 工作空间会被标记出来，界面据此提示「会出现在 Git 改动中」。
    #[test]
    fn git_workspace_is_detected() {
        let fixture = plain_fixture();
        std::fs::create_dir_all(fixture.ws.join(".git")).unwrap();
        let team = fixture.team("接入团队");

        let integration = fixture.ts.leader_integration(&team.id).unwrap();
        assert!(integration.git_workspace);
        let written = fixture.ts.write_workspace_mcp_config(&team.id).unwrap();
        assert!(written.git_workspace);
    }
}
