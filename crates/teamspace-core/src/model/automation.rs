use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// 团队的自动化设置（D43）：权限请求的自动答复规则、Leader 自治级别与合并闸门、任务动态推送给 Leader。
///
/// 没改过时为默认值：自治级别 0（Leader 不能合并）、不推送任务动态、不额外放行或拒绝命令。
/// 只由 Human 在应用里修改；Leader 可以读取。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct TeamAutomation {
    pub team_id: String,
    /// Leader 自治级别：0 = 合并只能由 Human 点；1 = 只合并文档或测试类改动；2 = 闸门全过就合并。
    pub autonomy_level: u8,
    /// 合并前的检查命令（在工作空间根目录以 `sh -c` 运行，如 `scripts/check.sh`）；空表示不检查。
    /// 内核先把执行分支合进基线、在临时 worktree 里跑一遍，退出码为 0 才允许 Leader 合并。
    pub check_command: Option<String>,
    /// 受保护路径（相对工作空间，`dir/` 结尾表示整个目录，如 `docs/DECISIONS.md`、`.github/`）：
    /// 改动碰到它们时 Leader 不能合并，退回给 Human。
    pub protected_paths: Vec<String>,
    /// 额外自动允许的命令前缀（如 `make test`），在内置白名单之外。
    pub allow_commands: Vec<String>,
    /// 额外自动拒绝的命令前缀，在内置黑名单之外。
    pub deny_commands: Vec<String>,
    /// 任务进入待验收、受阻或执行失败时，自动给 Leader 岗位发一条消息（会唤起 Leader 绑定的 Harness，消耗额度）。
    pub notify_leader: bool,
    /// 最近一次修改时间；没改过为 `null`。
    pub updated_at: Option<String>,
}

/// 修改团队的自动化设置（整体替换）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct UpdateTeamAutomationInput {
    pub team_id: String,
    pub autonomy_level: u8,
    #[serde(default)]
    #[ts(optional = nullable)]
    pub check_command: Option<String>,
    #[serde(default)]
    pub protected_paths: Vec<String>,
    #[serde(default)]
    pub allow_commands: Vec<String>,
    #[serde(default)]
    pub deny_commands: Vec<String>,
    #[serde(default)]
    pub notify_leader: bool,
}
