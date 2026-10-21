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
    /// Harness 握手检查（ACP `initialize`）。
    HarnessChecked,
    TaskCreated,
    TaskUpdated,
    TaskStatusChanged,
    /// 岗位文件夹（`Teamspace/<岗位>/`）写入失败；同一问题只记录一次。
    RoleFolderError,
    /// 开始执行任务（含重新执行）。
    RunStarted,
    /// 执行结束：成功、失败、取消或人工接管（`data.status`、`data.summary`、`data.changedFiles`、`data.failure`）。
    /// 写入岗位工作日志时带上执行摘要与改动文件。
    RunFinished,
    /// 应用退出或重启时执行仍在进行，已标记为中断。
    RunInterrupted,
    /// 续接已中断的执行。
    RunResumed,
    /// 验收通过（`data.runId`、`data.mergedCommit`；普通文件夹没有合并提交）。
    ReviewApproved,
    /// 验收退回（`data.runId`、`data.comment`、`data.revisionRunId`）。
    ReviewRejected,
    /// 验收通过时合并有冲突，已中止合并、基线不变，任务转为受阻（`data.conflictFiles`）。
    ReviewConflict,
    /// 验收通过时无法合并（如基线分支有未提交的改动），任务保持待验收（`data.reason`）。
    ReviewMergeBlocked,
    /// 合并成功后清理 worktree 或执行分支失败（不影响合并结果，`data.problem`）。
    ReviewCleanupFailed,
    /// 合并冲突后选择重新执行：任务回到 READY，旧执行分支与 worktree 已删除（`data.branch`、`data.commit`
    /// 记下被删除分支的名称与提交，便于找回）。
    ReviewRerun,
    /// Leader 越权：外部会话经本机控制 socket 请求了白名单之外的操作，被拒绝并留痕
    /// （`data.command` 为被拒的命令名，`data.message` 为拒绝原因）。
    LeaderDenied,
    /// 消息已发出（`data.messageId`、`data.threadId`、`data.toRoleId`）。
    MessageSent,
    /// 消息已收到回复（`data.messageId`、`data.threadId`）。
    MessageReplied,
    /// 消息投递失败（`data.messageId`、`data.threadId`、`data.reason`）。
    MessageDeliveryFailed,
    /// 消息过期（超过 7 天仍未回复；`data.messageId`、`data.threadId`）。
    MessageExpired,
}

/// 操作者：Human、Leader，或 Teamspace 自己（执行引擎根据 Agent 的结果做的状态变化）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum Actor {
    /// 在 Teamspace 应用里操作的人，以及端到端测试桥接。
    Human,
    /// Leader：外部会话（Claude Code / Codex 等）经本机控制 socket 操作（D7、D8、D26）。
    Leader,
    /// Teamspace 执行引擎（执行成功转待验收、失败回到 READY、重启时标记中断等）。
    System,
}

impl Actor {
    /// 操作者的稳定编号，用于在不引入全局可变状态的地方传递身份（如执行的停止请求）。
    pub(crate) fn code(self) -> u8 {
        match self {
            Actor::Human => 0,
            Actor::Leader => 1,
            Actor::System => 2,
        }
    }

    /// 由 [`Actor::code`] 还原操作者；未知编号按 Human 处理。
    pub(crate) fn from_code(code: u8) -> Actor {
        match code {
            1 => Actor::Leader,
            2 => Actor::System,
            _ => Actor::Human,
        }
    }
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

/// 活动分页查询。筛选条件全部可选，缺省表示不按该项过滤；各项之间是「且」的关系。
///
/// 派生 `Default` 是为了让既有调用点可以用 `..Default::default()` 补齐新增的筛选字段。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
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
    /// 只看某个任务的记录。任务不存在或不属于该团队时返回空结果，不报错。
    #[ts(optional = nullable)]
    pub task_id: Option<String>,
    /// 只看某个岗位的记录。岗位不存在或不属于该团队时返回空结果，不报错。
    #[ts(optional = nullable)]
    pub role_id: Option<String>,
    /// 只看这些种类的记录；`null` 或空数组表示不限种类。
    #[ts(optional = nullable)]
    pub kinds: Option<Vec<EventKind>>,
    /// 起始时间，**含**该时刻，UTC RFC3339（如 `2026-10-15T02:00:00.000Z`）。
    #[ts(optional = nullable)]
    pub since: Option<String>,
    /// 结束时间，**不含**该时刻，UTC RFC3339。格式与 `since` 相同。
    #[ts(optional = nullable)]
    pub until: Option<String>,
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
