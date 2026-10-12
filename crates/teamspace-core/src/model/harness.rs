use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// Harness 种类。`mock` 用于测试和演示，不调用任何外部程序。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum HarnessKind {
    ClaudeCode,
    Codex,
    Mock,
}

/// 健康状态。从未检查过为 `unknown`。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum HealthStatus {
    Ok,
    Error,
    Unknown,
}

/// 一次健康检查的结果，最近一次随 Harness 实例持久化。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct HealthCheck {
    pub status: HealthStatus,
    /// CLI 报告的版本，如 `2.1.288 (Claude Code)`。
    pub version: Option<String>,
    /// 中文说明：失败原因，或成功时实际使用的可执行文件等。
    pub detail: Option<String>,
    /// 从未检查过时为 `null`。
    pub checked_at: Option<String>,
}

/// Harness 实例，属于一个团队；岗位通过 `harnessId` 绑定。不保存任何密钥，登录由 CLI 自己管理。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct HarnessInstance {
    pub id: String,
    pub team_id: String,
    /// 团队内唯一（未归档实例中），如 `frontend-local`。
    pub name: String,
    /// 创建后不可修改。
    pub kind: HarnessKind,
    /// 可执行文件覆盖路径（绝对路径）；为 `null` 时使用本机检测到的路径。
    pub executable: Option<String>,
    /// 模型名，原样传给 CLI。
    pub model: Option<String>,
    /// Claude Code 为 settings 文件路径，Codex 为 profile 名。
    pub profile: Option<String>,
    pub health: HealthCheck,
    pub created_at: String,
    pub updated_at: String,
    pub archived_at: Option<String>,
}

/// 注册 Harness 实例。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct CreateHarnessInput {
    pub team_id: String,
    pub name: String,
    pub kind: HarnessKind,
    #[ts(optional = nullable)]
    pub executable: Option<String>,
    #[ts(optional = nullable)]
    pub model: Option<String>,
    #[ts(optional = nullable)]
    pub profile: Option<String>,
}

/// 编辑 Harness 实例（整体替换可编辑字段，缺省即清空）。修改后健康状态重置为 `unknown`。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct UpdateHarnessInput {
    pub harness_id: String,
    pub name: String,
    #[ts(optional = nullable)]
    pub executable: Option<String>,
    #[ts(optional = nullable)]
    pub model: Option<String>,
    #[ts(optional = nullable)]
    pub profile: Option<String>,
}

/// 本机 CLI 检测结果（不涉及任何团队）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct HarnessDetection {
    pub kind: HarnessKind,
    pub found: bool,
    /// 找到的可执行文件绝对路径。
    pub executable: Option<String>,
    pub version: Option<String>,
    /// 未找到或运行失败时的中文说明。
    pub error: Option<String>,
}
