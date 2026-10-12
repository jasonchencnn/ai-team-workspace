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
    /// 稳定标识，不可修改，团队内唯一，如 `role_frontend`。
    pub code: String,
    /// 任务编号前缀，2–4 位大写字母，如 `FE`。修改前缀不影响已有任务编号。
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

/// 后端提供的岗位模板，用于创建团队和新增岗位时预填。
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

/// 内置岗位模板，顺序即界面展示顺序。
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
