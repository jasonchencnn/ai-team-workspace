use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// Leader 接入配置（M4-03）：把 Teamspace 接到外部会话（Claude Code / Codex）所需的一切。
///
/// 三段配置文本由内核生成（见 [`crate::leader`]），路径里的空格与特殊字符已按各自格式转义，
/// 可以直接复制使用。权限边界不变（D26）：Leader 能创建与派发任务、查进度、退回验收，
/// 但合并、答复权限、换绑等操作只能由你在应用里做。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct LeaderIntegration {
    pub team_id: String,
    /// `teamspace-mcp` 可执行文件的绝对路径（找不到时是「应该在哪」的路径）。
    pub binary_path: String,
    /// 该可执行文件当前是否存在。
    pub binary_found: bool,
    /// 找不到可执行文件时的提示（怎么构建）。
    pub binary_hint: Option<String>,
    /// 本机控制 socket 的绝对路径。
    pub control_socket: String,
    /// 控制 socket 现在能不能连上（宿主应用监听失败时为 `false`）。
    pub control_socket_ready: bool,
    /// 控制 socket 不可用的原因（界面「接入 Leader」处提示 Leader 连不上）。
    pub control_socket_hint: Option<String>,
    /// 工作空间目录的绝对路径。
    pub workspace_path: String,
    /// 工作空间是不是 Git 仓库（决定要不要提示「会出现在 Git 改动中」）。
    pub git_workspace: bool,
    /// Claude Code 的登记命令（`claude mcp add teamspace …`）。
    pub claude_command: String,
    /// 工作空间 `.mcp.json` 片段（Claude Code 项目级配置）。
    pub mcp_json: String,
    /// Codex `~/.codex/config.toml` 片段。
    pub codex_toml: String,
}

/// 写入工作空间 `.mcp.json` 的结果。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct McpConfigWrite {
    /// 写入的文件路径。
    pub path: String,
    /// 之前没有这个文件，是新建的（`false` 表示合并进了已有文件）。
    pub created: bool,
    /// 工作空间是不是 Git 仓库（文件会出现在 Git 改动里）。
    pub git_workspace: bool,
}
