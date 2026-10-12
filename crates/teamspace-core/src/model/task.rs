use serde::{Deserialize, Serialize};
use ts_rs::TS;

use super::{Event, RoleRef};

/// 任务状态。显示名由后端提供（见 [`TaskStatus::label`] 与 `list_task_statuses`）。
///
/// 流转规则（[`TaskStatus::next`]）：
/// - todo → ready | cancelled
/// - ready → todo | doing | cancelled
/// - doing → blocked | review | cancelled
/// - blocked → ready | doing | cancelled
/// - review → done | ready | cancelled
/// - done → ready
/// - cancelled → todo
///
/// 进入 ready 或 doing（含 blocked → doing）需满足就绪条件；进入 blocked 必须给出原因，
/// 离开 blocked 时清空原因。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum TaskStatus {
    Todo,
    Ready,
    Doing,
    Blocked,
    Review,
    Done,
    Cancelled,
}

impl TaskStatus {
    /// 标准顺序（看板列顺序、计数顺序）。
    pub const ALL: [TaskStatus; 7] = [
        Self::Todo,
        Self::Ready,
        Self::Doing,
        Self::Blocked,
        Self::Review,
        Self::Done,
        Self::Cancelled,
    ];

    /// 界面显示名。
    pub fn label(self) -> &'static str {
        match self {
            Self::Todo => "TODO",
            Self::Ready => "READY",
            Self::Doing => "执行中",
            Self::Blocked => "受阻",
            Self::Review => "待验收",
            Self::Done => "已完成",
            Self::Cancelled => "已取消",
        }
    }

    /// 已结束（done / cancelled）。未结束任务会阻止岗位归档。
    pub fn is_closed(self) -> bool {
        matches!(self, Self::Done | Self::Cancelled)
    }

    /// 创建任务时允许的初始状态。
    pub fn is_initial(self) -> bool {
        matches!(self, Self::Todo | Self::Ready)
    }

    /// 按流转规则可以到达的下一状态（不考虑就绪条件）。
    pub fn next(self) -> &'static [TaskStatus] {
        use TaskStatus::*;
        match self {
            Todo => &[Ready, Cancelled],
            Ready => &[Todo, Doing, Cancelled],
            Doing => &[Blocked, Review, Cancelled],
            Blocked => &[Ready, Doing, Cancelled],
            Review => &[Done, Ready, Cancelled],
            Done => &[Ready],
            Cancelled => &[Todo],
        }
    }

    pub fn can_transition_to(self, to: TaskStatus) -> bool {
        self.next().contains(&to)
    }
}

/// 状态及其显示名。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct TaskStatusInfo {
    pub status: TaskStatus,
    pub label: String,
    /// 是否为结束状态（done / cancelled）。
    pub closed: bool,
}

impl From<TaskStatus> for TaskStatusInfo {
    fn from(status: TaskStatus) -> Self {
        Self {
            status,
            label: status.label().to_owned(),
            closed: status.is_closed(),
        }
    }
}

/// 某状态的任务数。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct StatusCount {
    pub status: TaskStatus,
    pub label: String,
    pub count: u32,
}

/// 文件引用。`path` 相对工作区根目录，使用 `/` 分隔，禁止绝对路径和 `..`；
/// `anchor` 为 `#` 之后的部分（如标题或行号），不含 `#`。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct FileRef {
    pub path: String,
    #[ts(optional = nullable)]
    pub anchor: Option<String>,
}

/// 文件引用及其在工作区中的实时存在情况。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct FileRefView {
    pub path: String,
    pub anchor: Option<String>,
    pub exists: bool,
}

/// 任务。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct Task {
    pub id: String,
    pub team_id: String,
    /// 创建时负责岗位的前缀 + 团队内该前缀的序号，补零到 3 位，如 `FE-023`。负责人改变时不变。
    pub key: String,
    pub title: String,
    pub description: String,
    pub status: TaskStatus,
    pub owner_role_id: String,
    pub file_refs: Vec<FileRef>,
    /// 依赖的任务 ID（同团队）。
    pub depends_on: Vec<String>,
    /// 仅在 blocked 时有值。
    pub blocked_reason: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

/// 看板卡片、列表行、依赖列表使用的任务摘要。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct TaskSummary {
    pub id: String,
    pub key: String,
    pub title: String,
    pub status: TaskStatus,
    pub owner: RoleRef,
    pub file_ref_count: u32,
    pub dependency_count: u32,
    /// 尚未 done 的依赖数。
    pub open_dependency_count: u32,
    pub blocked_reason: Option<String>,
    pub updated_at: String,
}

/// 看板的一列。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct BoardColumn {
    pub status: TaskStatus,
    pub label: String,
    /// 按更新时间倒序。
    pub tasks: Vec<TaskSummary>,
}

/// 看板查询。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct TaskBoardQuery {
    pub team_id: String,
    /// 是否包含「已取消」列，默认不包含。
    #[ts(optional = nullable)]
    pub include_cancelled: Option<bool>,
}

/// 看板数据：按标准顺序的状态列，始终含 done；cancelled 仅在请求时包含。空列也返回。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct TaskBoard {
    pub team_id: String,
    pub columns: Vec<BoardColumn>,
}

/// 任务详情。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct TaskDetail {
    pub task: Task,
    pub owner: RoleRef,
    /// 带存在情况的文件引用，顺序同 `task.fileRefs`。
    pub file_refs: Vec<FileRefView>,
    /// 本任务依赖的任务（上游）。
    pub dependencies: Vec<TaskSummary>,
    /// 依赖本任务的任务（下游）。
    pub dependents: Vec<TaskSummary>,
    /// 当前可执行的状态流转（不满足就绪条件时已排除 ready 和 doing）。
    pub allowed_transitions: Vec<TaskStatus>,
    /// 进入 ready / doing 的未满足条件（中文）；为空表示满足。与当前状态无关，始终计算。
    /// 工作区目录不存在时只有一条「工作区目录不存在或已移动」。
    pub readiness: Vec<String>,
    /// 与本任务相关的活动，按时间倒序。
    pub events: Vec<Event>,
}

/// 创建任务。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct CreateTaskInput {
    pub team_id: String,
    pub title: String,
    pub description: String,
    /// 未归档的同团队岗位；决定任务编号前缀。
    pub owner_role_id: String,
    /// 只能是 todo 或 ready；ready 需满足就绪条件。
    pub status: TaskStatus,
    /// 最多 50 个。
    pub file_refs: Vec<FileRef>,
    /// 同团队任务 ID，最多 20 个。
    pub depends_on: Vec<String>,
}

/// 编辑任务（整体替换可编辑字段）。状态走 `change_task_status`。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct UpdateTaskInput {
    pub task_id: String,
    pub title: String,
    pub description: String,
    pub owner_role_id: String,
    /// 最多 50 个。
    pub file_refs: Vec<FileRef>,
    /// 同团队任务 ID，最多 20 个。
    pub depends_on: Vec<String>,
}

/// 变更任务状态。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ChangeTaskStatusInput {
    pub task_id: String,
    pub to: TaskStatus,
    /// 进入 blocked 时必填（受阻原因，最多 500 字），其他状态忽略。
    #[ts(optional = nullable)]
    pub reason: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::TaskStatus::{self, *};

    #[test]
    fn transitions_match_the_design() {
        let table: [(TaskStatus, &[TaskStatus]); 7] = [
            (Todo, &[Ready, Cancelled]),
            (Ready, &[Todo, Doing, Cancelled]),
            (Doing, &[Blocked, Review, Cancelled]),
            (Blocked, &[Ready, Doing, Cancelled]),
            (Review, &[Done, Ready, Cancelled]),
            (Done, &[Ready]),
            (Cancelled, &[Todo]),
        ];
        for (from, allowed) in table {
            for to in TaskStatus::ALL {
                assert_eq!(
                    from.can_transition_to(to),
                    allowed.contains(&to),
                    "{from:?} -> {to:?}"
                );
            }
        }
    }

    #[test]
    fn only_todo_and_ready_are_initial() {
        let initial: Vec<_> = TaskStatus::ALL
            .into_iter()
            .filter(|s| s.is_initial())
            .collect();
        assert_eq!(initial, [Todo, Ready]);
    }
}
