//! 各工具的参数结构。
//!
//! 字段上的说明就是 MCP 工具的 JSON Schema 里的 `description`（中文），
//! Leader 由此知道该怎么填。

use rmcp::schemars::JsonSchema;
use serde::Deserialize;

/// 只要一个团队的工具。
#[derive(Debug, Clone, Default, Deserialize, JsonSchema)]
#[schemars(crate = "rmcp::schemars")]
pub struct TeamArgs {
    /// 团队名称或 id；只有一个团队时可省略。
    #[serde(default)]
    pub team: Option<String>,
}

/// 只要一个任务的工具。
#[derive(Debug, Clone, Default, Deserialize, JsonSchema)]
#[schemars(crate = "rmcp::schemars")]
pub struct TaskArgs {
    /// 任务编号（如 FE-001）、任务标题或任务 id。
    pub task: String,
    /// 团队名称或 id；任务编号在多个团队重复时用它消歧。
    #[serde(default)]
    pub team: Option<String>,
}

/// 查执行状态的参数。
#[derive(Debug, Clone, Default, Deserialize, JsonSchema)]
#[schemars(crate = "rmcp::schemars")]
pub struct RunStatusArgs {
    /// 任务编号或标题（取该任务最近一次执行）。
    #[serde(default)]
    pub task: Option<String>,
    /// 执行 id；与 task 二选一，给了 run 就用它。
    #[serde(default)]
    pub run: Option<String>,
    /// 团队名称或 id；只有一个团队时可省略。
    #[serde(default)]
    pub team: Option<String>,
}

/// 读执行进度的参数。
#[derive(Debug, Clone, Default, Deserialize, JsonSchema)]
#[schemars(crate = "rmcp::schemars")]
pub struct RunProgressArgs {
    /// 执行 id；与 task 二选一。
    #[serde(default)]
    pub run: Option<String>,
    /// 任务编号或标题（取该任务最近一次执行）。
    #[serde(default)]
    pub task: Option<String>,
    /// 续读游标：上一次返回的 nextCursor 原样传回。
    #[serde(default)]
    pub cursor: Option<String>,
    /// 每页条数（默认 200，最多 500）。
    #[serde(default)]
    pub limit: Option<u32>,
    /// 团队名称或 id；只有一个团队时可省略。
    #[serde(default)]
    pub team: Option<String>,
}

/// 查活动记录的参数。
#[derive(Debug, Clone, Default, Deserialize, JsonSchema)]
#[schemars(crate = "rmcp::schemars")]
pub struct ActivityArgs {
    /// 团队名称或 id；只有一个团队时可省略。
    #[serde(default)]
    pub team: Option<String>,
    /// 只看某个任务：任务编号或标题。
    #[serde(default)]
    pub task: Option<String>,
    /// 只看某个岗位：岗位名称、岗位标识或前缀。
    #[serde(default)]
    pub role: Option<String>,
    /// 翻页游标：上一次返回的 nextCursor 原样传回。
    #[serde(default)]
    pub cursor: Option<String>,
    /// 每页条数（默认 50，最多 200）。
    #[serde(default)]
    pub limit: Option<u32>,
}

/// 建任务的参数。
#[derive(Debug, Clone, Default, Deserialize, JsonSchema)]
#[schemars(crate = "rmcp::schemars")]
pub struct CreateTaskArgs {
    /// 任务标题（一句话说清要做什么）。
    pub title: String,
    /// 负责岗位：岗位名称、岗位标识（如 role_frontend）、前缀（如 FE）或岗位 id。
    pub role: String,
    /// 团队名称或 id；只有一个团队时可省略。
    #[serde(default)]
    pub team: Option<String>,
    /// 任务说明：背景、要求与验收标准。
    #[serde(default)]
    pub description: Option<String>,
    /// 初始状态：todo（默认）或 ready（依赖已满足时）。
    #[serde(default)]
    pub status: Option<String>,
    /// 依赖的任务（编号或标题），这些任务完成后本任务才能开始。
    #[serde(default)]
    pub depends_on: Vec<String>,
    /// 引用的工作空间文件（相对工作空间的路径）。
    /// 可以写 `路径#锚点`（如 `docs/spec.md#验收标准`）：`#` 之后是锚点，会一起存下来。
    #[serde(default)]
    pub file_refs: Vec<String>,
}

/// 编辑任务的参数：没给的字段保持原样。
#[derive(Debug, Clone, Default, Deserialize, JsonSchema)]
#[schemars(crate = "rmcp::schemars")]
pub struct UpdateTaskArgs {
    /// 任务编号、标题或 id。
    pub task: String,
    /// 团队名称或 id；只有一个团队时可省略。
    #[serde(default)]
    pub team: Option<String>,
    /// 新标题；不给就不改。
    #[serde(default)]
    pub title: Option<String>,
    /// 新说明；不给就不改。
    #[serde(default)]
    pub description: Option<String>,
    /// 新的负责岗位；不给就不改。
    #[serde(default)]
    pub role: Option<String>,
    /// 新的依赖列表；不给就不改，给空数组表示清空依赖。
    #[serde(default)]
    pub depends_on: Option<Vec<String>>,
    /// 新的文件引用；不给就不改，给空数组表示清空引用。
    /// 每一项是 `路径` 或 `路径#锚点`（如 `docs/spec.md#验收标准`）：锚点会一起写回。
    #[serde(default)]
    pub file_refs: Option<Vec<String>>,
}

/// 设置依赖的参数。
#[derive(Debug, Clone, Default, Deserialize, JsonSchema)]
#[schemars(crate = "rmcp::schemars")]
pub struct SetDependenciesArgs {
    /// 任务编号、标题或 id。
    pub task: String,
    /// 新的依赖列表（任务编号或标题）；空数组表示清空依赖。
    pub depends_on: Vec<String>,
    /// 团队名称或 id；只有一个团队时可省略。
    #[serde(default)]
    pub team: Option<String>,
}

/// 变更任务状态的参数。
#[derive(Debug, Clone, Default, Deserialize, JsonSchema)]
#[schemars(crate = "rmcp::schemars")]
pub struct ChangeStatusArgs {
    /// 任务编号、标题或 id。
    pub task: String,
    /// 目标状态：todo、ready、doing、blocked、review 或 cancelled（不能是 done）。
    pub to: String,
    /// 受阻原因；改成 blocked 时必填。
    #[serde(default)]
    pub reason: Option<String>,
    /// 团队名称或 id；只有一个团队时可省略。
    #[serde(default)]
    pub team: Option<String>,
}

/// 取消执行的参数。
#[derive(Debug, Clone, Default, Deserialize, JsonSchema)]
#[schemars(crate = "rmcp::schemars")]
pub struct CancelRunArgs {
    /// 执行 id；与 task 二选一。
    #[serde(default)]
    pub run: Option<String>,
    /// 任务编号或标题（取该任务最近一次执行）。
    #[serde(default)]
    pub task: Option<String>,
    /// 团队名称或 id；只有一个团队时可省略。
    #[serde(default)]
    pub team: Option<String>,
}

/// 退回验收的参数。
#[derive(Debug, Clone, Default, Deserialize, JsonSchema)]
#[schemars(crate = "rmcp::schemars")]
pub struct RejectReviewArgs {
    /// 任务编号或标题（取该任务待验收的执行）。
    #[serde(default)]
    pub task: Option<String>,
    /// 执行 id；与 task 二选一。
    #[serde(default)]
    pub run: Option<String>,
    /// 退回意见（必填）：要改什么、怎么改；会写进执行记录和岗位工作日志。
    pub comment: String,
    /// 团队名称或 id；只有一个团队时可省略。
    #[serde(default)]
    pub team: Option<String>,
}

/// 查看板的参数。
#[derive(Debug, Clone, Default, Deserialize, JsonSchema)]
#[schemars(crate = "rmcp::schemars")]
pub struct BoardArgs {
    /// 团队名称或 id；只有一个团队时可省略。
    #[serde(default)]
    pub team: Option<String>,
    /// 是否包含「已取消」这一列，默认不包含。
    #[serde(default)]
    pub include_cancelled: Option<bool>,
}
