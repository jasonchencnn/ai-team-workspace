//! 待我处理、决策记录与应用内通知（M5-02）。
//!
//! 约定：
//! - 字段在 JSON / TS 中为 camelCase，枚举值为 snake_case 字符串。
//! - ID 为 UUID v7 字符串；时间为 UTC RFC3339 字符串，界面按本地时间显示。
//! - 待我处理不存「已读」：每次按真实状态（任务、执行、消息、决策记录）重算，处理完自然消失。

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use super::{Message, MessageStatus, PermissionPrompt, Run, Task};

/// 通知类别（应用内通知按它订阅）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum NotificationCategory {
    /// 任务进入待验收。
    TaskReview,
    /// 执行失败。
    TaskFailed,
    /// 任务受阻。
    TaskBlocked,
    /// 消息已回复。
    MessageReplied,
    /// 消息需要处理（投递失败、过期，或岗位发来等你答复）。
    MessageAction,
    /// 权限请求待答复。
    Permission,
}

impl NotificationCategory {
    /// 全部类别（顺序同界面分组顺序）。
    pub const ALL: [NotificationCategory; 6] = [
        Self::TaskReview,
        Self::TaskFailed,
        Self::TaskBlocked,
        Self::MessageReplied,
        Self::MessageAction,
        Self::Permission,
    ];

    /// 界面显示名。
    pub fn label(self) -> &'static str {
        match self {
            Self::TaskReview => "任务待验收",
            Self::TaskFailed => "执行失败",
            Self::TaskBlocked => "任务受阻",
            Self::MessageReplied => "消息已回复",
            Self::MessageAction => "消息需要处理",
            Self::Permission => "权限请求",
        }
    }
}

/// 查询待我处理。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct GetInboxInput {
    /// 团队；必填。
    pub team_id: String,
}

/// 查询通知订阅设置。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct GetNotificationSettingsInput {
    /// 团队；必填。
    pub team_id: String,
}

/// 待我处理事项的类别。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum InboxCategory {
    /// 待验收：任务已进入待验收，等你通过或退回。
    Review,
    /// 执行失败或受阻（含合并冲突）。
    Blocked,
    /// 待答复的权限请求。
    Permission,
    /// 需要你拍板的消息。
    Decision,
    /// Leader 越权记录（最近 7 天）。
    LeaderDenied,
}

impl InboxCategory {
    /// 全部类别（顺序同界面分组顺序）。
    pub const ALL: [InboxCategory; 5] = [
        Self::Review,
        Self::Blocked,
        Self::Permission,
        Self::Decision,
        Self::LeaderDenied,
    ];

    /// 界面显示名。
    pub fn label(self) -> &'static str {
        match self {
            Self::Review => "待验收",
            Self::Blocked => "执行失败或受阻",
            Self::Permission => "待答复权限",
            Self::Decision => "需要拍板",
            Self::LeaderDenied => "Leader 越权",
        }
    }
}

/// 待我处理条目的跳转目标。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(
    tag = "type",
    rename_all = "snake_case",
    rename_all_fields = "camelCase"
)]
#[ts(export)]
pub enum InboxTarget {
    /// 跳到任务（任务详情或看板卡片）。
    Task { task_id: String, task_key: String },
    /// 跳到消息线程。
    Message {
        message_id: String,
        thread_id: String,
    },
    /// 跳到执行中的权限请求。
    Permission { run_id: String, request_id: String },
    /// 跳到活动记录。
    Activity { event_id: String },
}

/// 一条待我处理的事项。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct InboxItem {
    /// 条目标识：同一类别内稳定（由关联对象的 id 拼出），界面用它做 key 与变更检测。
    pub id: String,
    pub category: InboxCategory,
    /// 类别显示名（后端给出，界面直接用）。
    pub category_label: String,
    /// 标题。
    pub title: String,
    /// 关联任务；没有时为 `null`。
    pub task_id: Option<String>,
    /// 关联任务编号（如 `FE-001`）；没有时为 `null`。
    pub task_key: Option<String>,
    /// 关联消息；没有时为 `null`。
    pub message_id: Option<String>,
    /// 关联消息线程；没有时为 `null`。
    pub thread_id: Option<String>,
    /// 关联执行；没有时为 `null`。
    pub run_id: Option<String>,
    /// 发生时间（UTC RFC3339）。
    pub occurred_at: String,
    /// 影响说明，如「通过后将合并到 claude/mvp-reset」。
    pub impact: String,
    /// 建议操作。
    pub suggestion: String,
    /// 跳转目标。
    pub target: InboxTarget,
    /// 待验收时：本次执行没有任何改动（提示 Agent 可能提前结束）。
    pub no_changes: bool,
}

/// 待我处理的计数汇总。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct InboxCounts {
    /// 全部事项的总数。
    pub total: u32,
    /// 待验收。
    pub review: u32,
    /// 执行失败或受阻。
    pub blocked: u32,
    /// 待答复的权限请求。
    pub permission: u32,
    /// 需要拍板的消息。
    pub decision: u32,
    /// Leader 越权记录。
    pub leader_denied: u32,
}

/// 待我处理。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct Inbox {
    pub team_id: String,
    /// 计数汇总（各类别的条数）。
    pub counts: InboxCounts,
    /// 事项列表（按发生时间倒序，同一时间按类别顺序）。
    pub items: Vec<InboxItem>,
    /// 生成时间（UTC RFC3339）。
    pub generated_at: String,
}

/// 记录一条决策。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct RecordDecisionInput {
    /// 决策所属团队。
    pub team_id: String,
    /// 关联任务；可缺省。给了时须是本团队的任务。
    #[ts(optional = nullable)]
    pub task_id: Option<String>,
    /// 关联消息；可缺省。给了时须是本团队的消息。
    #[ts(optional = nullable)]
    pub message_id: Option<String>,
    /// 决策内容（必填）。
    pub text: String,
    /// 是否把受阻任务恢复到 READY（走现有就绪条件校验）。缺省为不恢复。
    #[ts(optional = nullable)]
    pub resume_task: Option<bool>,
}

/// 一条决策记录。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct Decision {
    pub id: String,
    pub team_id: String,
    /// 关联任务；没有时为 `null`。
    pub task_id: Option<String>,
    /// 关联任务编号；没有时为 `null`。
    pub task_key: Option<String>,
    /// 关联消息；没有时为 `null`。
    pub message_id: Option<String>,
    /// 关联消息线程；没有时为 `null`。
    pub thread_id: Option<String>,
    /// 关联岗位；没有关联对象时为 `null`。
    pub role_id: Option<String>,
    /// 关联岗位名；没有时为 `null`。
    pub role_name: Option<String>,
    /// 决策内容。
    pub text: String,
    /// 是否同时把受阻任务恢复到了 READY。
    pub resumed_task: bool,
    pub created_at: String,
}

/// 决策列表的筛选条件。各项之间是「且」的关系，缺省表示不按该项过滤。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ListDecisionsQuery {
    /// 团队；必填。
    pub team_id: String,
    /// 只看某个任务的决策。
    #[ts(optional = nullable)]
    pub task_id: Option<String>,
    /// 只看某条消息的决策。
    #[ts(optional = nullable)]
    pub message_id: Option<String>,
}

/// 一条应用内通知。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct Notification {
    pub id: String,
    pub team_id: String,
    pub category: NotificationCategory,
    /// 类别显示名。
    pub category_label: String,
    /// 标题。
    pub title: String,
    /// 说明正文。
    pub body: String,
    /// 关联任务；没有时为 `null`。
    pub task_id: Option<String>,
    /// 关联任务编号；没有时为 `null`。
    pub task_key: Option<String>,
    /// 关联消息；没有时为 `null`。
    pub message_id: Option<String>,
    /// 关联消息线程；没有时为 `null`。
    pub thread_id: Option<String>,
    /// 关联执行；没有时为 `null`。
    pub run_id: Option<String>,
    /// 关联岗位（订阅按岗位过滤时用它）。
    pub role_id: Option<String>,
    /// 发生时间（UTC RFC3339）。
    pub occurred_at: String,
}

/// 通知订阅设置：按类别与岗位开关，默认全部开启。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct NotificationSettings {
    pub team_id: String,
    /// 按类别的设置（顺序同 [`NotificationCategory::ALL`]）。
    pub categories: Vec<NotificationCategorySetting>,
}

/// 一个通知类别的设置。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct NotificationCategorySetting {
    pub category: NotificationCategory,
    /// 类别显示名。
    pub category_label: String,
    /// 该类别的总开关。
    pub enabled: bool,
    /// 各岗位的开关（团队内未归档的岗位）。
    pub roles: Vec<NotificationRoleSetting>,
}

/// 一个岗位的通知开关。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct NotificationRoleSetting {
    pub role_id: String,
    pub role_name: String,
    pub role_code: String,
    /// 该岗位是否接收这个类别的通知。
    pub enabled: bool,
}

/// 修改通知订阅设置。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct UpdateNotificationSettingsInput {
    pub team_id: String,
    /// 要修改的类别；未列出的类别保持不变。
    pub categories: Vec<NotificationCategoryUpdate>,
}

/// 一个类别的开关改动。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct NotificationCategoryUpdate {
    pub category: NotificationCategory,
    /// 类别总开关；缺省表示不改。
    #[ts(optional = nullable)]
    pub enabled: Option<bool>,
    /// 岗位开关；缺省表示不改。
    pub roles: Vec<NotificationRoleUpdate>,
}

/// 一个岗位的开关改动。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct NotificationRoleUpdate {
    pub role_id: String,
    pub enabled: bool,
}

/// 推送给前端的通知事件（Tauri 事件名 [`crate::api::NOTIFY_EVENT`]）。
///
/// 与执行事件（[`crate::model::RunEvent`]）分开：执行事件只讲「执行」，
/// 这里讲「需要你知道的事」——通知与消息状态变化。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(
    tag = "type",
    rename_all = "snake_case",
    rename_all_fields = "camelCase"
)]
#[ts(export)]
pub enum NotifyEvent {
    /// 一条应用内通知（已按订阅设置过滤：关掉的类别不推）。
    Notification { notification: Box<Notification> },
    /// 消息状态变化（排队 → 投递中 → 已回复 / 失败 / 过期）。
    ///
    /// 页面据此实时更新消息状态，不必轮询。
    MessageStatus {
        message: Box<Message>,
        /// 变化前的状态；`null` 表示这条消息刚创建（排队）。
        previous_status: Option<MessageStatus>,
    },
}

/// 通知的素材：调用方给，内核补齐 id、时间并按订阅设置决定是否推送。
pub(crate) struct NotificationSpec {
    pub team_id: String,
    pub category: NotificationCategory,
    pub title: String,
    pub body: String,
    pub task_id: Option<String>,
    pub task_key: Option<String>,
    pub message_id: Option<String>,
    pub thread_id: Option<String>,
    pub run_id: Option<String>,
    pub role_id: Option<String>,
    pub occurred_at: Option<String>,
}

/// 任务进入待验收的通知素材。
pub(crate) fn task_review_spec(task: &Task, run: Option<&Run>) -> NotificationSpec {
    let impact = match run {
        Some(run) => match run.base_branch.as_deref() {
            Some(branch) => format!("通过后将合并到 {branch}"),
            None => "通过后任务标记完成".to_owned(),
        },
        None => "通过后任务标记完成".to_owned(),
    };
    let no_changes = run.is_some_and(|r| r.no_changes);
    let body = if no_changes {
        format!("{impact}；本次执行没有任何改动，Agent 可能提前结束")
    } else {
        impact
    };
    NotificationSpec {
        team_id: task.team_id.clone(),
        category: NotificationCategory::TaskReview,
        title: format!("任务 {} 待验收", task.key),
        body,
        task_id: Some(task.id.clone()),
        task_key: Some(task.key.clone()),
        message_id: None,
        thread_id: None,
        run_id: run.map(|r| r.id.clone()),
        role_id: Some(task.owner_role_id.clone()),
        occurred_at: run
            .and_then(|r| r.ended_at.clone())
            .or_else(|| Some(task.updated_at.clone())),
    }
}

/// 执行失败（`failed` 为真）或受阻的通知素材。
pub(crate) fn task_blocked_spec(task: &Task, reason: &str, failed: bool) -> NotificationSpec {
    let category = if failed {
        NotificationCategory::TaskFailed
    } else {
        NotificationCategory::TaskBlocked
    };
    let title = if failed {
        format!("任务 {} 执行失败", task.key)
    } else {
        format!("任务 {} 受阻", task.key)
    };
    NotificationSpec {
        team_id: task.team_id.clone(),
        category,
        title,
        body: reason.to_owned(),
        task_id: Some(task.id.clone()),
        task_key: Some(task.key.clone()),
        message_id: None,
        thread_id: None,
        run_id: None,
        role_id: Some(task.owner_role_id.clone()),
        occurred_at: Some(task.updated_at.clone()),
    }
}

/// 权限请求的通知素材。
pub(crate) fn permission_spec(run: &Run, prompt: &PermissionPrompt) -> NotificationSpec {
    NotificationSpec {
        team_id: run.team_id.clone(),
        category: NotificationCategory::Permission,
        title: format!("{} 请求确认：{}", run.task_key, prompt.title),
        body: prompt.reason.clone(),
        task_id: Some(run.task_id.clone()),
        task_key: Some(run.task_key.clone()),
        message_id: None,
        thread_id: None,
        run_id: Some(run.id.clone()),
        role_id: Some(run.role_id.clone()),
        occurred_at: Some(prompt.requested_at.clone()),
    }
}

/// 消息的通知素材（已回复 / 需要处理）。
pub(crate) fn message_spec(message: &Message, category: NotificationCategory) -> NotificationSpec {
    let title = match category {
        NotificationCategory::MessageReplied => "岗位已回复消息".to_owned(),
        _ => "有一条消息需要你处理".to_owned(),
    };
    let body = match category {
        NotificationCategory::MessageReplied => message
            .reply_body
            .clone()
            .unwrap_or_else(|| "岗位已回复这条消息。".to_owned()),
        _ => match message.failure_reason.as_deref() {
            Some(reason) => reason.to_owned(),
            None => message.body.clone(),
        },
    };
    NotificationSpec {
        team_id: message.team_id.clone(),
        category,
        title,
        body,
        task_id: message.task_id.clone(),
        task_key: None,
        message_id: Some(message.id.clone()),
        thread_id: Some(message.thread_id.clone()),
        run_id: None,
        role_id: Some(message.to_role_id.clone()),
        occurred_at: Some(message.created_at.clone()),
    }
}
