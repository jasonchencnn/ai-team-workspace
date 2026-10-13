use serde::{Deserialize, Serialize};
use ts_rs::TS;

use super::HarnessInstance;

/// 岗位，属于一个团队。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct Role {
    pub id: String,
    pub team_id: String,
    pub name: String,
    /// 稳定标识，不可修改，团队内唯一，如 `role_3`（自动生成）或 `role_frontend`。
    pub code: String,
    /// 任务编号前缀，2–4 位大写字母或数字、以字母开头，如 `FE`、`R1`。修改前缀不影响已有任务编号。
    pub task_prefix: String,
    /// 职责。
    pub description: String,
    /// 工作约定。
    pub guidelines: String,
    pub is_leader: bool,
    /// 绑定的 Harness 实例；为 `null` 时界面显示「未绑定 Harness · 暂不派工」。
    pub harness_id: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    pub archived_at: Option<String>,
}

/// 岗位卡片：岗位 + 绑定的 Harness + 任务统计。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct RoleView {
    pub role: Role,
    /// 绑定的 Harness 实例（含最近健康检查）；未绑定时为 `null`。
    pub harness: Option<HarnessInstance>,
    /// 该岗位负责的未结束任务数（状态不是 done / cancelled）。
    pub open_task_count: u32,
    /// 该岗位负责的执行中任务数。
    pub doing_task_count: u32,
    /// 岗位文件夹相对工作空间的路径（`/` 分隔），如 `Teamspace/前端开发`；尚未建立时为 `null`。
    pub folder: Option<String>,
    /// 岗位文件夹最近一次同步失败的中文说明（如工作空间不存在或不可写）；正常时为 `null`。
    pub folder_problem: Option<String>,
}

/// 岗位文件夹（`get_role_folder` 的结果）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct RoleFolder {
    pub role_id: String,
    /// 相对工作空间的路径（`/` 分隔），如 `Teamspace/前端开发`；尚未建立时为 `null`。
    pub relative_path: Option<String>,
    /// 绝对路径；尚未建立时为 `null`。
    pub path: Option<String>,
    /// 文件夹当前是否存在于磁盘上。
    pub exists: bool,
    /// 工作日志最近的条目（默认 20 条），最新的在前；每条形如
    /// `2026-10-13 14:05:01 UTC+08:00 · FE-001 · 状态 执行中 → 受阻：等待接口`。
    pub journal: Vec<String>,
    /// 文件夹不可用或读写失败时的中文说明；正常时为 `null`。
    pub problem: Option<String>,
}

/// 岗位文件夹 `过程文件/` 下的一个文件（岗位工作台「最近文件」）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct RoleFileEntry {
    /// 相对岗位文件夹的路径（`/` 分隔），如 `过程文件/素材清单.md`；可直接传给 `open_role_path`。
    pub relative_path: String,
    /// 文件大小（字节）。
    pub size: u64,
    /// 最后修改时间（UTC RFC3339）。
    pub modified_at: String,
}

/// 岗位工作台（`get_role_workbench` 的结果）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct RoleWorkbench {
    /// 岗位卡片：岗位、绑定的 Harness 实例（含最近握手结果）、任务统计、文件夹登记信息。
    pub view: RoleView,
    /// 岗位文件夹：路径、是否存在、问题、工作日志最近 20 条（最新的在前）。
    pub folder: RoleFolder,
    /// 该岗位负责的最近任务（任何状态），按更新时间倒序，最多 8 条。
    pub recent_tasks: Vec<super::TaskSummary>,
    /// 该岗位负责的任务总数（用于「查看全部」）。
    pub task_count: u32,
    /// `过程文件/` 下最近修改的文件（递归，跳过隐藏文件与目录、符号链接和 Teamspace 生成的
    /// `过程文件/README.md`），按修改时间倒序，最多 10 条；文件夹不存在时为空。
    pub recent_files: Vec<RoleFileEntry>,
    /// 该岗位最近的执行（任何状态），按开始时间倒序，最多 5 条；活动执行在其中。
    pub recent_runs: Vec<super::Run>,
}

/// 任务等处引用岗位时使用的简要信息（岗位可能已归档）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct RoleRef {
    pub id: String,
    pub name: String,
    pub code: String,
    pub task_prefix: String,
    pub archived: bool,
}

/// 岗位模板：可选的「快速添加示例」，用户主动选择时才用来预填，默认不加载到任何团队。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct RoleTemplate {
    pub name: String,
    pub code: String,
    pub task_prefix: String,
    pub description: String,
    pub guidelines: String,
    pub is_leader: bool,
}

/// 在已有团队中新增岗位。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct CreateRoleInput {
    pub team_id: String,
    pub role: super::NewRole,
}

/// 请求任务前缀建议。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct SuggestTaskPrefixInput {
    /// 已有团队时传入，建议会避开该团队未归档岗位已用的前缀；创建团队前可缺省。
    #[serde(default)]
    #[ts(optional = nullable)]
    pub team_id: Option<String>,
    /// 岗位名称（可为空）。
    pub name: String,
    /// 额外需要避开的前缀，如创建团队表单里其他尚未保存的岗位前缀。
    #[serde(default)]
    #[ts(optional = nullable)]
    pub taken: Option<Vec<String>>,
}

/// 编辑岗位（整体替换可编辑字段）。`code` 不可修改；绑定 Harness 走单独命令。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct UpdateRoleInput {
    pub role_id: String,
    pub name: String,
    pub task_prefix: String,
    pub description: String,
    pub guidelines: String,
    pub is_leader: bool,
}

/// 内置岗位模板（快速添加示例，偏软件开发场景），顺序即界面展示顺序。
pub fn role_templates() -> Vec<RoleTemplate> {
    let t =
        |name: &str, code: &str, prefix: &str, description: &str, is_leader: bool| RoleTemplate {
            name: name.to_owned(),
            code: code.to_owned(),
            task_prefix: prefix.to_owned(),
            description: description.to_owned(),
            guidelines: String::new(),
            is_leader,
        };
    vec![
        t(
            "Leader",
            "role_leader",
            "LD",
            "拆解、路由、验收；读取团队计划；无生产权限。",
            true,
        ),
        t(
            "产品经理",
            "role_product",
            "PM",
            "需求澄清、验收标准与优先级。",
            false,
        ),
        t(
            "前端开发",
            "role_frontend",
            "FE",
            "负责前端界面、交互实现及前端验证。",
            false,
        ),
        t(
            "后端开发",
            "role_backend",
            "BE",
            "负责后端服务、数据与接口实现及后端验证。",
            false,
        ),
        t("QA 测试", "role_qa", "QA", "验收与回归", false),
    ]
}
