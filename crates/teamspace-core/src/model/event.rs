use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// 活动种类。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum EventKind {
    TeamCreated,
    RoleCreated,
    RoleUpdated,
    RoleArchived,
    /// 岗位绑定或更换 Harness。
    RoleHarnessBound,
    RoleHarnessUnbound,
    HarnessCreated,
    HarnessUpdated,
    HarnessArchived,
    HarnessChecked,
    TaskCreated,
    TaskUpdated,
    TaskStatusChanged,
}

/// 操作者。M1 只有 Human；后续里程碑会加入岗位 / Harness。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum Actor {
    Human,
}

/// 活动记录。每次变更写一条，只追加不修改。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct Event {
    pub id: String,
    pub team_id: String,
    pub task_id: Option<String>,
    pub role_id: Option<String>,
    pub kind: EventKind,
    pub actor: Actor,
    /// 由后端生成的中文摘要，如「FE-023 状态 READY → 执行中」。
    pub summary: String,
    /// 结构化细节（如 `{ "from": "ready", "to": "doing" }`），各种类的字段由后端决定，前端只用于展示。
    #[ts(type = "unknown")]
    pub data: serde_json::Value,
    pub created_at: String,
}

/// 活动分页查询。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct EventQuery {
    pub team_id: String,
    /// 上一页返回的 `nextCursor`（不透明字符串，原样传回）；缺省从最新开始。
    #[ts(optional = nullable)]
    pub cursor: Option<String>,
    /// 每页条数，默认 50，最大 200。
    #[ts(optional = nullable)]
    pub limit: Option<u32>,
}

/// 活动分页结果，按时间倒序。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct EventPage {
    pub events: Vec<Event>,
    /// 还有更早的记录时为下一页游标，否则为 `null`。
    pub next_cursor: Option<String>,
}
