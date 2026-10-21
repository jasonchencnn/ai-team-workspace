//! 岗位通信的消息与线程（M5-01）。
//!
//! 团队通信经 Teamspace 中转并持久化：消息由 Human、Leader 或岗位发出，投递引擎把消息交给
//! 接收岗位绑定的 Harness 处理，Agent 的最终文字作为回复写回同一线程。
//!
//! 约定：
//! - 字段在 JSON / TS 中为 camelCase，枚举值为 snake_case 字符串。
//! - ID 为 UUID v7 字符串；时间为 UTC RFC3339 字符串，界面按本地时间显示。
//! - 消息不硬删除：失败与过期都保留原消息、失败原因与时间，便于重发与追溯。

use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// 消息的发送方。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum MessageSender {
    /// 在 Teamspace 应用里操作的人。
    Human,
    /// Leader：外部会话（Claude Code / Codex 等）经本机控制 socket 发起（D8）。
    Leader,
    /// 岗位：以某个岗位的身份发出的消息（发信岗位）。
    Role,
}

impl MessageSender {
    /// 展示用的中文名。
    pub fn label(self) -> &'static str {
        match self {
            MessageSender::Human => "Human",
            MessageSender::Leader => "Leader",
            MessageSender::Role => "岗位",
        }
    }
}

/// 消息的投递状态。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum MessageStatus {
    /// 排队等待投递。
    Queued,
    /// 投递中：已交给接收岗位绑定的 Harness。
    Delivering,
    /// 已回复：Agent 的回复已写回消息。
    Replied,
    /// 失败：岗位未绑定 Harness、握手失败或 Agent 出错。
    Failed,
    /// 过期：超过 7 天仍未回复。
    Expired,
}

impl MessageStatus {
    /// 展示用的中文名。
    pub fn label(self) -> &'static str {
        match self {
            MessageStatus::Queued => "排队中",
            MessageStatus::Delivering => "投递中",
            MessageStatus::Replied => "已回复",
            MessageStatus::Failed => "失败",
            MessageStatus::Expired => "已过期",
        }
    }

    /// 是否是终态（不再变化，除非重发）。
    pub fn is_terminal(self) -> bool {
        matches!(
            self,
            MessageStatus::Replied | MessageStatus::Failed | MessageStatus::Expired
        )
    }
}

/// 线程参与方：发送方（Human / Leader / 岗位）与接收岗位。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ThreadParticipant {
    /// 岗位 id；Human 与 Leader 没有岗位，为 `null`。
    pub role_id: Option<String>,
    /// 参与方的身份。
    pub kind: MessageSender,
    /// 展示名：岗位名，或「Human」/「Leader」。
    pub name: String,
}

/// 消息线程：围绕一个主题的往来。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct MessageThread {
    pub id: String,
    pub team_id: String,
    /// 关联任务；可缺省。
    pub task_id: Option<String>,
    /// 主题。
    pub subject: String,
    /// 参与方。
    pub participants: Vec<ThreadParticipant>,
    pub created_at: String,
    /// 最近一条消息的时间。
    pub updated_at: String,
    /// 最近一条消息的投递状态。
    pub last_status: MessageStatus,
    /// 最近一条消息的发送时间。
    pub last_message_at: String,
    /// 最近一条消息的摘要；可缺省。
    #[ts(optional = nullable)]
    pub last_snippet: Option<String>,
}

/// 一条消息。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct Message {
    pub id: String,
    pub thread_id: String,
    pub team_id: String,
    /// 关联任务；可缺省。
    pub task_id: Option<String>,
    /// 发送方。
    pub sender: MessageSender,
    /// 发送方是岗位时的岗位 id；Human 与 Leader 为 `null`。
    pub sender_role_id: Option<String>,
    /// 接收岗位。
    pub to_role_id: String,
    /// 消息正文。
    pub body: String,
    /// 投递状态。
    pub status: MessageStatus,
    /// 投递失败的原因；未失败时为 `null`。
    pub failure_reason: Option<String>,
    /// Agent 的回复正文；已回复时才有。
    pub reply_body: Option<String>,
    pub created_at: String,
    /// 进入「投递中」的时间。
    pub delivered_at: Option<String>,
    /// 写回回复的时间。
    pub replied_at: Option<String>,
    /// 入队时接收岗位绑定的 Harness 实例 id（快照，仅留痕）。
    ///
    /// 换绑后仍能看出这条消息是按哪个绑定入队的；实际投递按岗位「当前有效」绑定解析（D33）。
    pub binding_harness_id: Option<String>,
}

/// 发往某个岗位的消息：换绑界面要看「换绑期间还有哪些消息在排队」（M6-04）。
///
/// 一条查询取回线程主题与入队时的绑定实例名，界面不必再逐个取线程详情、
/// 也不用另查实例列表去翻译绑定 id。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct RoleMessage {
    pub id: String,
    pub thread_id: String,
    /// 线程主题。
    pub thread_subject: String,
    pub team_id: String,
    /// 关联任务；可缺省。
    pub task_id: Option<String>,
    /// 接收岗位。
    pub to_role_id: String,
    /// 消息正文。
    pub body: String,
    /// 投递状态。
    pub status: MessageStatus,
    /// 状态显示名。
    pub status_label: String,
    /// 原始发送时间。
    pub created_at: String,
    /// 入队时间（重发时更新）；过期判定与排序用它。
    pub queued_at: String,
    /// 入队时接收岗位绑定的 Harness 实例 id（快照，仅留痕）。
    pub binding_harness_id: Option<String>,
    /// 入队时接收岗位绑定的 Harness 实例名；实例已删除或当时没有绑定时为 `null`。
    pub binding_harness_name: Option<String>,
}

/// 发往某岗位的消息的筛选条件。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ListRoleMessagesQuery {
    /// 接收岗位；必填。
    pub role_id: String,
    /// 只看这些投递状态；缺省或为空时为「排队中 + 投递中」。
    #[ts(optional = nullable)]
    pub statuses: Option<Vec<MessageStatus>>,
    /// 最多返回条数，默认与上限都是 200。
    #[ts(optional = nullable)]
    pub limit: Option<u32>,
}

/// 发送消息的入参。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct SendMessageInput {
    /// 消息所属团队。
    pub team_id: String,
    /// 接收岗位；须是本团队未归档的岗位。
    pub to_role_id: String,
    /// 消息正文。
    pub body: String,
    /// 关联任务；可缺省。给了时须是本团队的任务。
    #[ts(optional = nullable)]
    pub task_id: Option<String>,
    /// 续用已有线程时给出线程 id；缺省时按「接收岗位 + 关联任务」找最近的线程，没有则新建。
    #[ts(optional = nullable)]
    pub thread_id: Option<String>,
    /// 主题；新建线程时用，缺省取正文第一行。
    #[ts(optional = nullable)]
    pub subject: Option<String>,
    /// 以某个岗位的身份发消息时给出该岗位 id（发信岗位）；缺省时按操作者判定发送方。
    #[ts(optional = nullable)]
    pub from_role_id: Option<String>,
}

/// 线程列表的筛选条件。各项之间是「且」的关系，缺省表示不按该项过滤。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ListThreadsQuery {
    /// 团队；必填。
    pub team_id: String,
    /// 只看关联某个任务的线程。
    #[ts(optional = nullable)]
    pub task_id: Option<String>,
    /// 只看某岗位参与（发送或接收）的线程。
    #[ts(optional = nullable)]
    pub role_id: Option<String>,
}

/// 线程详情：线程与其中的消息（按时间正序）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ThreadDetail {
    pub thread: MessageThread,
    pub messages: Vec<Message>,
}
