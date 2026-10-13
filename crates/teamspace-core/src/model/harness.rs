use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// ACP Agent 预设（D13）。Harness 实例都是 ACP Agent，预设只决定默认启动命令和本机检测方式，
/// 平台不为单一厂商写专门的执行代码。`mock` 由宿主程序内置，用于测试和演示，不消耗额度。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum AcpPreset {
    /// Claude Code，经开源适配器 `claude-code-acp` 接入。
    ClaudeCode,
    /// Codex，经开源适配器 `codex-acp` 接入。
    Codex,
    /// MiniMax Code，原生 ACP：`mcode acp`。
    MinimaxCode,
    /// Gemini CLI，原生 ACP：`gemini --experimental-acp`。
    GeminiCli,
    /// CodeBuddy（腾讯），原生 ACP：`codebuddy --acp`。
    #[serde(rename = "codebuddy")]
    CodeBuddy,
    /// Qoder CLI（阿里），原生 ACP：`qodercli --acp`。
    QoderCli,
    /// Grok Build（xAI），原生 ACP（测试版）：`grok agent stdio`，需要 SuperGrok Heavy 订阅。
    GrokBuild,
    /// Qwen Code（阿里），原生 ACP：`qwen --experimental-acp`。
    QwenCode,
    /// Kimi CLI（月之暗面），原生 ACP：`kimi --acp`。
    KimiCli,
    /// OpenCode，原生 ACP：`opencode acp`。
    #[serde(rename = "opencode")]
    OpenCode,
    /// 任意支持 ACP 的 Agent，启动命令由用户填写。
    Custom,
    /// 内置 Mock Agent（测试用）。
    Mock,
}

impl AcpPreset {
    pub const ALL: [AcpPreset; 12] = [
        AcpPreset::ClaudeCode,
        AcpPreset::Codex,
        AcpPreset::MinimaxCode,
        AcpPreset::GeminiCli,
        AcpPreset::CodeBuddy,
        AcpPreset::QoderCli,
        AcpPreset::GrokBuild,
        AcpPreset::QwenCode,
        AcpPreset::KimiCli,
        AcpPreset::OpenCode,
        AcpPreset::Custom,
        AcpPreset::Mock,
    ];

    /// 界面显示名。
    pub fn label(self) -> &'static str {
        match self {
            AcpPreset::ClaudeCode => "Claude Code",
            AcpPreset::Codex => "Codex",
            AcpPreset::MinimaxCode => "MiniMax Code",
            AcpPreset::GeminiCli => "Gemini CLI",
            AcpPreset::CodeBuddy => "CodeBuddy（腾讯）",
            AcpPreset::QoderCli => "Qoder CLI（阿里）",
            AcpPreset::GrokBuild => "Grok Build（xAI）",
            AcpPreset::QwenCode => "Qwen Code（阿里）",
            AcpPreset::KimiCli => "Kimi CLI（月之暗面）",
            AcpPreset::OpenCode => "OpenCode",
            AcpPreset::Custom => "自定义 ACP Agent",
            AcpPreset::Mock => "Mock",
        }
    }
}

/// ACP 握手状态。从未检查过（或配置修改后）为 `unknown`。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum HandshakeStatus {
    /// `initialize` 成功且协议版本兼容，可以派工。
    Connected,
    /// Agent 要求先登录（`initialize` 返回 ACP `auth_required` 错误）；请在对应工具中完成登录后重新检查。
    NeedsAuth,
    /// 启动或握手失败：命令不存在、进程退出、超时、输出不是 JSON、协议版本不兼容等，原因见 `detail`。
    Error,
    /// 尚未做过握手检查。
    Unknown,
}

/// Agent 自报的实现信息（ACP `agentInfo`）；Agent 未提供时整体为 `null`。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct AgentInfo {
    /// 程序名，如 `claude-code-acp`。
    pub name: String,
    /// 显示名（可选）。
    pub title: Option<String>,
    /// 版本，如 `0.9.2`。
    pub version: Option<String>,
}

/// Agent 在 `initialize` 中声明的能力（ACP `agentCapabilities`），未声明的项为 `false`。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct AcpCapabilities {
    /// 支持 `session/load`（加载已有会话，续接用）。
    pub load_session: bool,
    /// 提示中可以带图片。
    pub image: bool,
    /// 提示中可以带音频。
    pub audio: bool,
    /// 提示中可以带嵌入的上下文（资源内容）。
    pub embedded_context: bool,
    /// 支持 HTTP 传输的 MCP 服务器（stdio MCP 是 ACP 基线能力，不单列）。
    pub mcp_http: bool,
    /// 支持 SSE 传输的 MCP 服务器。
    pub mcp_sse: bool,
}

/// 最近一次 ACP 握手的结果，随 Harness 实例持久化。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct Handshake {
    pub status: HandshakeStatus,
    /// Agent 返回的 ACP 协议版本；未握手成功时为 `null`。
    pub protocol_version: Option<u32>,
    /// Agent 自报的名称与版本；Agent 未提供或未握手成功时为 `null`。
    pub agent: Option<AgentInfo>,
    /// Agent 声明的能力；未握手成功时为 `null`。
    pub capabilities: Option<AcpCapabilities>,
    /// Agent 提供的登录方式名称（ACP `authMethods[].name`），没有时为空数组。平台不代为登录。
    pub auth_methods: Vec<String>,
    /// 中文说明：失败原因、需要登录时的提示，或成功时实际启动的命令。
    pub detail: Option<String>,
    /// 检查时间；从未检查过为 `null`。
    pub checked_at: Option<String>,
}

impl Handshake {
    /// 从未检查过的状态。
    pub fn unknown() -> Self {
        Self {
            status: HandshakeStatus::Unknown,
            protocol_version: None,
            agent: None,
            capabilities: None,
            auth_methods: Vec::new(),
            detail: None,
            checked_at: None,
        }
    }

    /// 启动或握手失败。
    pub fn error(detail: impl Into<String>) -> Self {
        Self {
            status: HandshakeStatus::Error,
            detail: Some(detail.into()),
            ..Self::unknown()
        }
    }
}

/// Harness 实例 = 一个 ACP Agent 的启动配置，属于一个团队；岗位通过 `harnessId` 绑定。
/// 平台不保存任何密钥，登录由各 Agent / CLI 自己管理。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct HarnessInstance {
    pub id: String,
    pub team_id: String,
    /// 团队内唯一（未归档实例中），如 `Claude Code（默认）`。
    pub name: String,
    /// 创建后不可修改。
    pub preset: AcpPreset,
    /// 启动命令：PATH 中的命令名（如 `claude-code-acp`）或绝对路径。
    /// `mock` 预设固定为 `teamspace-mock-agent`，实际由宿主程序以内置参数启动。
    pub command: String,
    /// 启动参数，如 `["acp"]`。
    pub args: Vec<String>,
    /// 环境配置档案文件的绝对路径（可选）。平台只记路径，不读取、不保存其中的内容。
    pub env_profile: Option<String>,
    /// 最近一次握手结果。
    pub handshake: Handshake,
    pub created_at: String,
    pub updated_at: String,
    pub archived_at: Option<String>,
}

/// 注册 Harness 实例（ACP Agent）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct CreateHarnessInput {
    pub team_id: String,
    pub name: String,
    pub preset: AcpPreset,
    /// 缺省或空白时使用预设的默认命令；`custom` 预设必填；`mock` 预设忽略此字段。
    #[serde(default)]
    #[ts(optional = nullable)]
    pub command: Option<String>,
    /// 缺省（`null`）时使用预设的默认参数；传空数组表示不带参数；`mock` 预设忽略此字段。
    #[serde(default)]
    #[ts(optional = nullable)]
    pub args: Option<Vec<String>>,
    #[serde(default)]
    #[ts(optional = nullable)]
    pub env_profile: Option<String>,
}

/// 编辑 Harness 实例（整体替换可编辑字段；预设不可改）。修改后握手状态重置为 `unknown`。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct UpdateHarnessInput {
    pub harness_id: String,
    pub name: String,
    /// 规则同 [`CreateHarnessInput::command`]。
    #[serde(default)]
    #[ts(optional = nullable)]
    pub command: Option<String>,
    /// 规则同 [`CreateHarnessInput::args`]。
    #[serde(default)]
    #[ts(optional = nullable)]
    pub args: Option<Vec<String>>,
    #[serde(default)]
    #[ts(optional = nullable)]
    pub env_profile: Option<String>,
}

/// 预设与本机检测结果（`list_acp_presets`，不涉及任何团队、不写数据）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct AcpPresetInfo {
    pub preset: AcpPreset,
    /// 显示名，如 `Claude Code`。
    pub label: String,
    /// 一句话说明，如 `经适配器 claude-code-acp`。
    pub description: String,
    /// 默认启动命令；`custom` 为 `null`（需用户填写）。
    pub default_command: Option<String>,
    /// 默认参数。
    pub default_args: Vec<String>,
    /// 默认命令在本机 PATH（含登录 shell 的 PATH）中的绝对路径；未找到或 `custom` 时为 `null`。
    /// `mock` 为宿主程序路径。
    pub command_path: Option<String>,
    /// 默认命令是否可用（`custom` 恒为 `false`）。
    pub command_found: bool,
    /// 适配器所依赖的底层 CLI 命令名（如 Claude Code 适配器依赖 `claude`）；没有单独依赖时为 `null`。
    pub requires_cli: Option<String>,
    /// 底层 CLI 是否在本机找到；`requiresCli` 为 `null` 时也为 `null`。
    pub cli_found: Option<bool>,
    /// 缺少什么时的中文提示（只说明需要安装什么）；都已就绪、`custom`、`mock` 时为 `null`。
    pub hint: Option<String>,
}
