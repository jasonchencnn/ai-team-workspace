//! 执行（Run）：把一个任务派给负责岗位绑定的 ACP Agent 执行一次（M2，D13 / D15）。
//!
//! 一个任务可以有多次执行（`attempt` 递增），同一任务同时最多一个活动执行（queued / running /
//! waiting_permission）；普通文件夹工作空间同时最多一个活动执行（原地执行，D15）。

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use super::{AcpCapabilities, AcpPreset};

/// 执行状态。
///
/// - `queued`：已创建，正在准备执行目录、启动 Agent、建立会话；
/// - `running`：任务说明已发出，Agent 正在工作；
/// - `waiting_permission`：Agent 请求了需要 Human 确认的权限（见 `pendingPermission`）；
/// - `succeeded`：Agent 以 `end_turn` 结束，任务转为待验收（REVIEW）；
/// - `failed`：失败（原因见 `failure` / `failureKind`），任务转为受阻（BLOCKED，原因「执行失败：…」），
///   可「重试」（`start_run`）；
/// - `cancelled`：Human 取消，任务回到 READY；人工接管（`takenOver`）时任务保持执行中（DOING），
///   之后可再次 `start_run` 或直接提交验收（`change_task_status` → review）；
/// - `interrupted`：应用退出或重启时执行仍在进行，进程已不在；任务保持执行中，可续接（`resume_run`）、
///   重新执行（`start_run`）或标记失败（`mark_run_failed`，任务转为受阻）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum RunStatus {
    Queued,
    Running,
    WaitingPermission,
    Succeeded,
    Failed,
    Cancelled,
    Interrupted,
}

impl RunStatus {
    pub const ACTIVE: [RunStatus; 3] = [Self::Queued, Self::Running, Self::WaitingPermission];

    /// 活动状态：有 Agent 进程在为它工作（或正在启动）。
    pub fn is_active(self) -> bool {
        Self::ACTIVE.contains(&self)
    }

    /// 界面显示名。
    pub fn label(self) -> &'static str {
        match self {
            Self::Queued => "准备中",
            Self::Running => "执行中",
            Self::WaitingPermission => "等待确认",
            Self::Succeeded => "已完成",
            Self::Failed => "失败",
            Self::Cancelled => "已取消",
            Self::Interrupted => "已中断",
        }
    }
}

/// 执行目录的隔离方式（D15）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum RunIsolation {
    /// Git 工作空间：在应用数据目录下新建 worktree 与分支 `teamspace/<任务编号>-r<序号>`，完成后保留，
    /// 不自动合并。
    Worktree,
    /// 普通文件夹：原地执行，执行目录就是工作空间；写入本岗位文件夹与任务引用文件自动批准，
    /// 其他写入与命令执行转交 Human 确认。
    InPlace,
}

/// 执行开始时 Harness 实例的快照（之后实例被编辑或换绑不影响这条记录）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct HarnessSnapshot {
    pub harness_id: String,
    pub name: String,
    pub preset: AcpPreset,
    pub command: String,
    pub args: Vec<String>,
}

/// 失败原因的类别（界面据此显示提示与操作，如「需要登录」时提示在终端完成登录后重试）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum FailureKind {
    /// Agent 要求登录（ACP auth_required）；登录由 Agent / CLI 自己管理，平台不保存凭据。
    NeedsAuth,
    /// Agent 进程退出或关闭了输出。
    AgentExited,
    /// 连续一段时间（默认 30 分钟）没有任何输出，或启动阶段没有响应。
    Timeout,
    /// 协议错误：Agent 返回 JSON-RPC 错误、输出无法解析、协议版本不兼容等。
    Protocol,
    /// Agent 拒绝了任务（stopReason = refusal）。
    Refused,
    /// 达到 Agent 的输出或轮次上限（max_tokens / max_turn_requests）。
    Limit,
    /// 无法启动：命令不存在、环境配置档案不可用、执行目录（worktree）无法创建等。
    Launch,
    /// 执行中断后由 Human 标记为失败。
    Interrupted,
}

/// 改动的类型。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum FileChangeKind {
    Added,
    Modified,
    Deleted,
}

/// 一个改动的文件。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ChangedFile {
    /// worktree 执行：相对仓库根；原地执行：相对工作空间（工作空间之外的为绝对路径）。`/` 分隔。
    pub path: String,
    pub change: FileChangeKind,
}

/// 一次编辑的行数统计（来自 ACP 工具调用内容中的 diff）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct DiffStat {
    pub path: String,
    pub added: u32,
    pub removed: u32,
}

/// 权限选项（ACP `PermissionOption`）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct PermissionOption {
    pub option_id: String,
    /// Agent 提供的显示文字。
    pub name: String,
    /// `allow_once` / `allow_always` / `reject_once` / `reject_always`（其他值原样保留）。
    pub kind: String,
}

/// 等待 Human 确认的权限请求。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct PermissionPrompt {
    /// 传给 `respond_permission` 的请求 ID（Teamspace 生成）。
    pub request_id: String,
    pub tool_call_id: String,
    /// 工具调用标题，如「运行 cargo test」。
    pub title: String,
    /// ACP 工具类型：read / edit / delete / move / search / execute / think / fetch / switch_mode / other；
    /// Agent 未提供时为 `null`。
    pub tool_kind: Option<String>,
    /// 涉及的路径（Agent 给出的原样）。
    pub paths: Vec<String>,
    /// 要执行的命令（取自工具调用的 rawInput.command 等字段）；没有时为 `null`。
    pub command: Option<String>,
    /// 为什么需要 Human 确认（中文），如「执行命令需要确认」「写入本岗位文件夹与任务引用文件之外需要确认：/x/y」。
    pub reason: String,
    pub options: Vec<PermissionOption>,
    pub requested_at: String,
}

/// 执行的完成条目写入岗位工作日志的真实结果（界面按它显示，不自己拼装）。
///
/// 判据是数据库里的事实：完成条目对应的活动序号已被 `roles.journaled_seq` 覆盖即视为已写入；
/// 未覆盖说明岗位文件夹不可写等原因导致写入失败（或尚未同步），条目会在恢复后补写。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct RunJournal {
    /// 完成条目是否已写入岗位工作日志。
    pub written: bool,
    /// 工作日志里真实的那一行（不含行首的 `- `）；没有对应条目时为 `null`。
    pub line: Option<String>,
    /// 未写入时的原因（如「无法写入工作日志…」）；已写入时为 `null`。
    pub problem: Option<String>,
}

/// 一次执行。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct Run {
    pub id: String,
    pub team_id: String,
    pub task_id: String,
    /// 任务编号，如 `FE-001`。
    pub task_key: String,
    pub role_id: String,
    pub harness: HarnessSnapshot,
    /// 该任务的第几次执行，从 1 开始。
    pub attempt: u32,
    pub status: RunStatus,
    /// 状态显示名。
    pub status_label: String,
    /// ACP 会话 ID；会话建立前为 `null`。
    pub session_id: Option<String>,
    /// 执行目录（Agent 的 cwd）绝对路径。
    pub exec_dir: String,
    pub isolation: RunIsolation,
    /// worktree 分支，如 `teamspace/FE-001-r1`；原地执行为 `null`。
    pub branch: Option<String>,
    /// worktree 的基线提交（完整哈希）；原地执行为 `null`。
    pub base_commit: Option<String>,
    /// Agent 在 `initialize` 中声明的能力；Agent 尚未握手时为 `null`。
    pub capabilities: Option<AcpCapabilities>,
    pub started_at: String,
    pub ended_at: Option<String>,
    /// ACP 结束原因：end_turn / max_tokens / max_turn_requests / refusal / cancelled；未正常结束时为 `null`。
    pub stop_reason: Option<String>,
    /// 失败、取消或中断的中文原因，如「需要登录：…」「Agent 进程退出（退出码 3）」「超时：30 分钟无输出」。
    pub failure: Option<String>,
    /// 失败类别；未失败时为 `null`。
    pub failure_kind: Option<FailureKind>,
    /// 执行摘要：取自 Agent 最后的消息（截断）；没有时为 `null`。
    pub summary: Option<String>,
    /// 改动的文件：worktree 用 Git 统计（相对基线提交，含未跟踪文件）；原地执行取「ACP 报告的写入与编辑类
    /// 工具调用」与「执行前后目录快照的比对结果」的并集（shell 直接写的文件也在内）。执行结束或中断
    /// （含应用重启后恢复）时写入，按路径排序。
    pub changed_files: Vec<ChangedFile>,
    /// 普通文件夹原地执行时，快照比对发现的「范围外改动」：不在本岗位文件夹、不在任务引用文件、
    /// 也没有被 Human 明确允许写入的文件（相对工作空间，已计入 `changedFiles`）。非空表示 Agent
    /// 未经确认改动了权限范围外的文件：执行即使成功，任务也会转为受阻而不是待验收。Git worktree 执行
    /// 与没有此类改动时为空。
    pub out_of_scope: Vec<String>,
    /// 改动统计的补充说明（中文），如文件太多、快照降级后统计不完整；没有时为 `null`。
    pub change_note: Option<String>,
    /// 改动统计不完整：执行目录里的文件太多，快照降级，部分位置的改动没有统计（说明见 `changeNote`）。
    pub change_incomplete: bool,
    /// 本次执行中的工具调用次数。
    pub tool_call_count: u32,
    /// 执行成功但相对基线没有任何改动：worktree 执行是执行分支相对基线提交的 diff 为空，原地执行是快照
    /// 比对没有发现改动。这类执行仍进入待验收，界面据此提示 Agent 可能提前结束（例如权限被拒）。
    /// 失败、取消、中断、人工接管的执行恒为 `false`。
    pub no_changes: bool,
    /// 中断时把执行目录里的未提交改动提交到执行分支得到的提交（完整哈希）；没有保全或不是 Git 执行时为 `null`。
    pub interrupted_commit: Option<String>,
    /// 中断时没能把改动保全到执行分支的原因（中文），如「执行目录不在执行分支 X 上（当前：main）」；
    /// 保全成功或不需要保全时为 `null`。
    pub interrupted_problem: Option<String>,
    /// 是否为人工接管而停止。
    pub taken_over: bool,
    /// 等待 Human 确认的权限请求（status 为 waiting_permission 时有值）。
    pub pending_permission: Option<PermissionPrompt>,
    /// 当前可否续接（interrupted、已有会话且 Agent 支持 loadSession）。
    pub can_resume: bool,
    /// 完成条目写入岗位工作日志的真实结果（成功面板据此显示）。
    pub journal: RunJournal,
    /// worktree 执行开始时工作空间所在的分支（验收通过时合并进这个分支，D20）；原地执行、
    /// 处于分离 HEAD 或 M3 之前的执行为 `null`。
    pub base_branch: Option<String>,
    /// 验收通过、合并成功后基线分支上的合并提交（完整哈希）；未合并为 `null`。
    pub merged_commit: Option<String>,
    /// 退回后续接的执行：被退回的那次执行的 ID（沿用它的执行目录与分支）；其他执行为 `null`。
    pub revision_of: Option<String>,
    /// 这次执行的验收结果；尚未验收为 `null`。
    pub review: Option<super::RunReview>,
}

/// 消息的来源。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum MessageRole {
    /// Agent 的回复（`agent_message_chunk`）。
    Agent,
    /// Agent 的思考过程（`agent_thought_chunk`）。
    Thought,
    /// 发给 Agent 的内容（任务说明、续接提示，或 Agent 回放的用户消息）。
    User,
}

/// 计划中的一项（ACP `PlanEntry`）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct PlanEntry {
    pub content: String,
    /// high / medium / low
    pub priority: String,
    /// pending / in_progress / completed
    pub status: String,
}

/// 权限请求的决定。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct PermissionDecision {
    /// 选中的选项；取消时为 `null`。
    pub option_id: Option<String>,
    /// allow_once / allow_always / reject_once / reject_always / cancelled
    pub kind: String,
    /// `rule`：按权限规则自动批准；`human`：Human 选择；`cancelled`：执行取消或结束时撤销。
    pub by: String,
    /// 中文说明，如「编辑本岗位文件夹内的文件，自动允许」。
    pub reason: String,
    pub decided_at: String,
}

/// 执行进度条目的内容。同一条目会被后续消息更新（消息块合并、工具调用状态、计划替换、权限决定），
/// 前端按 `seq` 排序、按 `id` 替换。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(
    tag = "type",
    rename_all = "snake_case",
    rename_all_fields = "camelCase"
)]
#[ts(export)]
pub enum RunUpdateBody {
    /// 连续的同来源消息块合并为一条。
    Message { role: MessageRole, text: String },
    /// 当前计划（整体替换，每次执行只有一条）。
    Plan { entries: Vec<PlanEntry> },
    /// 工具调用及其最新状态。
    ToolCall {
        tool_call_id: String,
        title: String,
        /// ACP 工具类型；未提供时为 `null`。
        kind: Option<String>,
        /// pending / in_progress / completed / failed；未提供时为 `null`。
        status: Option<String>,
        /// 涉及的路径。
        paths: Vec<String>,
        /// 要执行的命令（execute 类）；没有时为 `null`。
        command: Option<String>,
        /// 编辑内容的行数统计（ACP diff 内容）。
        diffs: Vec<DiffStat>,
    },
    /// 权限请求；`decision` 为 `null` 表示仍在等待 Human。
    Permission {
        request_id: String,
        tool_call_id: String,
        title: String,
        tool_kind: Option<String>,
        paths: Vec<String>,
        command: Option<String>,
        reason: String,
        options: Vec<PermissionOption>,
        decision: Option<PermissionDecision>,
    },
    /// Agent 通过 `fs/write_text_file` 写文件的结果。
    FileWrite {
        path: String,
        allowed: bool,
        reason: String,
    },
    /// 执行状态变化（含失败原因）。
    Status {
        status: RunStatus,
        detail: Option<String>,
    },
    /// 需要 Human 留意的醒目提示，如 Agent 未经确认改动了权限范围外的文件（`files` 列出这些文件）。
    Warning { text: String, files: Vec<String> },
}

/// 一条执行进度。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct RunUpdate {
    pub id: String,
    pub run_id: String,
    /// 在本次执行中的位置（时间顺序），显示时按它排序。
    #[ts(type = "number")]
    pub seq: i64,
    pub body: RunUpdateBody,
    pub created_at: String,
    pub updated_at: String,
}

/// 执行进度分页查询。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct RunUpdateQuery {
    pub run_id: String,
    /// 上次返回的 `nextCursor`（不透明字符串）；缺省从头读取。
    #[ts(optional = nullable)]
    pub cursor: Option<String>,
    /// 每页条数，默认 200，最多 500。
    #[ts(optional = nullable)]
    pub limit: Option<u32>,
}

/// 执行进度分页结果：自 `cursor` 之后**新增或被更新**的条目（按变更顺序）。前端按 `id` 合并到
/// 已有列表并按 `seq` 排序；之后用 `nextCursor` 继续读取增量。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct RunUpdatePage {
    pub run_id: String,
    pub updates: Vec<RunUpdate>,
    /// 下一次增量读取的游标（总是有值；没有新内容时与传入的相同）。
    pub next_cursor: String,
    /// 本页已满、还有更多变更时为 `true`，应立即再读一次。
    pub has_more: bool,
    /// 执行当前状态（便于轮询时判断是否结束）。
    pub status: RunStatus,
}

/// 回应权限请求。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct RespondPermissionInput {
    pub run_id: String,
    /// `PermissionPrompt.requestId`。
    pub request_id: String,
    /// 选中的 `PermissionOption.optionId`。
    pub option_id: String,
}

/// 内核推送给前端的执行事件（Tauri 事件名 [`crate::api::RUN_EVENT`]）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(
    tag = "type",
    rename_all = "snake_case",
    rename_all_fields = "camelCase"
)]
#[ts(export)]
pub enum RunEvent {
    /// 执行记录变化（状态、会话、摘要、待确认的权限等），载荷为最新的完整记录。任务状态可能随之变化，
    /// 前端应刷新相关任务与看板。
    RunChanged { run: Box<Run> },
    /// 新增或更新了一条执行进度。
    RunUpdated {
        run_id: String,
        task_id: String,
        update: RunUpdate,
    },
    /// Agent 请求了需要 Human 确认的权限（同时会有 `run_changed`，状态为 waiting_permission）。
    PermissionRequested {
        run_id: String,
        task_id: String,
        team_id: String,
        prompt: PermissionPrompt,
    },
}
