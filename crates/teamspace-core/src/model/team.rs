use serde::{Deserialize, Serialize};
use ts_rs::TS;

use super::{Event, RoleView, StatusCount, TaskSummary, WorkspaceInfo};

/// 团队。名称在未归档团队中唯一；`workspacePath` 是 Git 仓库顶层目录的绝对路径。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct Team {
    pub id: String,
    pub name: String,
    pub description: String,
    pub workspace_path: String,
    pub created_at: String,
    pub updated_at: String,
    pub archived_at: Option<String>,
}

/// 创建团队的输入。`roles` 为初始岗位（通常来自岗位模板，可修改），可以为空。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct CreateTeamInput {
    pub name: String,
    pub description: String,
    /// 用户选择的文件夹；后端会规范化为所在 Git 仓库的顶层目录。
    pub workspace_path: String,
    pub roles: Vec<NewRole>,
}

/// 新岗位的字段（创建团队时的初始岗位，或单独创建岗位时使用）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct NewRole {
    pub name: String,
    /// 稳定标识，创建后不可修改，团队内唯一（含已归档岗位），如 `role_frontend`。
    /// 规则：小写字母开头，只含小写字母、数字和下划线，最长 40 位。
    pub code: String,
    /// 任务编号前缀，2–4 位大写字母，在团队的未归档岗位中唯一。
    pub task_prefix: String,
    /// 职责。
    pub description: String,
    /// 工作约定。
    pub guidelines: String,
    /// 每个团队至多一个 Leader。
    pub is_leader: bool,
}

/// 团队概览（团队空间首页）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct TeamOverview {
    pub team: Team,
    /// 实时检测的工作区状态；检测失败时为 `null`，原因见 `workspaceProblem`。
    pub workspace: Option<WorkspaceInfo>,
    /// 工作区不可用时的中文说明（如目录不存在、不再是 Git 仓库）。
    pub workspace_problem: Option<String>,
    /// 未归档岗位卡片，Leader 在前，其余按创建时间。
    pub roles: Vec<RoleView>,
    /// 各状态任务数，按状态标准顺序，包含全部 7 种状态（含已取消）。
    pub task_counts: Vec<StatusCount>,
    /// 进行中的任务（执行中、受阻、待验收），按更新时间倒序。
    pub active_tasks: Vec<TaskSummary>,
    /// 最近活动，按时间倒序，最多 10 条。
    pub recent_events: Vec<Event>,
}
