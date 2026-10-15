//! 验收（M3-01，D7 / D20）：查看待验收执行的摘要、改动文件与逐文件 diff；通过（Git 工作空间合并执行分支，
//! 普通文件夹直接完成）或退回（附意见，在同一执行目录续接会话重新执行）。

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use super::{FileChangeKind, Run, RunIsolation, Task};

/// 一次执行的验收结果。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum ReviewOutcome {
    /// 通过（Git：已合并进基线分支；普通文件夹：任务直接完成）。
    Approved,
    /// 退回，附意见，已在同一执行目录续接重新执行（见 `Run.revisionOf`）。
    Rejected,
    /// 通过时合并有冲突，已中止合并，任务转为受阻，等 Human 处理。
    Conflict,
}

/// 执行记录上的验收结果。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct RunReview {
    pub outcome: ReviewOutcome,
    /// 退回意见（`rejected`）或冲突说明（`conflict`）；通过时为 `null`。
    pub comment: Option<String>,
    pub reviewed_at: String,
}

/// 一次退回。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ReviewRejection {
    /// 被退回的执行。
    pub run_id: String,
    /// 被退回的是第几次执行。
    pub attempt: u32,
    pub comment: String,
    pub rejected_at: String,
}

/// 验收页中的一个改动文件。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ReviewFile {
    /// Git：相对仓库根；普通文件夹：相对工作空间（工作空间之外的为绝对路径）。`/` 分隔。
    pub path: String,
    pub change: FileChangeKind,
    /// 二进制文件（没有逐行 diff）。普通文件夹总是 `false`。
    pub binary: bool,
    /// 新增 / 删除的行数；没有统计（普通文件夹、二进制文件）时为 `null`。
    pub added: Option<u32>,
    pub removed: Option<u32>,
    /// 可以用 `get_review_file_patch` 查看逐文件 diff（只有 Git 工作空间提供，D20）。
    pub has_patch: bool,
}

/// 一个文件的补丁（unified diff，`git diff` 格式）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ReviewFilePatch {
    pub run_id: String,
    pub path: String,
    /// 二进制文件：`patch` 为空。
    pub binary: bool,
    /// 补丁过大，已截断。
    pub truncated: bool,
    pub patch: String,
}

/// 「通过」会做什么。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum ApproveAction {
    /// Git 工作空间：以 `--no-ff` 把执行分支合并进基线分支，成功后删除 worktree 与执行分支，不推送。
    Merge,
    /// 普通文件夹：改动已在原地发生，只把任务置为完成。
    Complete,
}

/// 验收页数据。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ReviewView {
    pub task: Task,
    /// 待验收的执行：任务处于待验收（REVIEW）时它的最近一次执行；任务不在待验收、或待验收但从未执行过
    /// （手动提交验收）时为 `null`。
    pub run: Option<Run>,
    /// 执行摘要（同 `run.summary`）。
    pub summary: Option<String>,
    /// 隔离方式；没有待验收的执行时为 `null`。
    pub isolation: Option<RunIsolation>,
    /// Git：合并的目标分支（基线分支）；没有记录时为 `null`（通过时退回到工作空间当前分支）。
    pub base_branch: Option<String>,
    /// Git：执行分支；普通文件夹为 `null`。
    pub branch: Option<String>,
    /// Git：预览对应的执行分支 head（读取验收页时已把执行目录里未提交的改动提交到执行分支）；通过时原样
    /// 传回 `approve_review` 的 `expectedHead`。普通文件夹或读取失败时为 `null`。
    pub head_commit: Option<String>,
    /// 这次执行有一次合并正在进行（或上次合并没有落库、尚未对账）：不能退回，稍后刷新。
    pub merging: bool,
    /// 改动文件。Git：执行分支相对基线提交的 diff；普通文件夹：执行记录里的改动文件（退回续接的多轮执行
    /// 合在一起），没有 diff。按路径排序。
    pub files: Vec<ReviewFile>,
    /// 改动文件读取失败的原因（如 worktree 已被删除）；读取成功为 `null`。
    pub files_problem: Option<String>,
    /// 「通过」会做什么；没有待验收的执行时为 `null`。
    pub approve_action: Option<ApproveAction>,
    /// 现在能否「通过」（`approve_review`）。
    pub can_approve: bool,
    /// 能否「退回」（`reject_review`）。
    pub can_reject: bool,
    /// 不能通过或退回的原因（中文）；为空表示都可以。合并时才能发现的问题（基线分支有未提交改动、
    /// 冲突）不在这里，由 `approve_review` 的结果给出。
    pub problems: Vec<String>,
    /// 历次退回意见，最早的在前。
    pub rejections: Vec<ReviewRejection>,
    /// `run` 是一次合并冲突的执行（任务因冲突转为受阻，D22）：此时不能通过或退回，可以「已手动合并，
    /// 标记完成」（`resolve_merge_manually`）或「重新执行」（`rerun_after_conflict`）。
    pub conflict: bool,
    /// 合并冲突的文件（`conflict` 时取自执行记录，刷新后仍可列出）；其他情况为空。
    pub conflict_files: Vec<String>,
    /// 现在能否「已手动合并，标记完成」（是否真的已并入基线在调用时校验）。
    pub can_resolve_manually: bool,
    /// 现在能否「重新执行」（需满足就绪条件）。
    pub can_rerun: bool,
}

/// 合并冲突后「已手动合并，标记完成」（D22）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ResolveMergeManuallyInput {
    /// 合并冲突的执行（任务最近一次执行）。
    pub run_id: String,
}

/// 合并冲突后「重新执行」（D22）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct RerunAfterConflictInput {
    /// 合并冲突的执行（任务最近一次执行）。
    pub run_id: String,
}

/// 通过验收。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ApproveReviewInput {
    /// 待验收的执行（任务最近一次执行）；不是时拒绝（conflict），防止对旧页面上的旧执行操作。
    pub run_id: String,
    /// Git：验收页显示的 `ReviewView.headCommit`；与执行分支当前 head 不一致时拒绝（conflict，「改动已变化，
    /// 请刷新验收页」），保证合并的就是看过的内容。Git 执行必填；普通文件夹忽略。
    #[ts(optional = nullable)]
    pub expected_head: Option<String>,
}

/// 任务完成后自动转为 READY 的下游任务。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct AdvancedTask {
    pub id: String,
    pub key: String,
    pub title: String,
}

/// 退回验收。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct RejectReviewInput {
    /// 待验收的执行（任务最近一次执行）。
    pub run_id: String,
    /// 退回意见，必填（去掉首尾空白后不能为空，最多 2000 字）。
    pub comment: String,
}

/// 通过验收的结果。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum ApproveOutcome {
    /// 普通文件夹：任务已完成。
    Completed,
    /// Git：已合并进基线分支，任务已完成。
    Merged,
    /// Git：合并有冲突，已中止合并、基线不变；任务转为受阻（原因写明冲突文件），执行分支与 worktree 保留。
    Conflict,
    /// Git：现在不能合并（如基线分支有未提交的改动、基线分支不存在）；任务保持待验收，`reason` 说明原因。
    Blocked,
}

/// `approve_review` 的返回。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ApproveReviewResult {
    pub outcome: ApproveOutcome,
    /// 合并提交（`merged`）。
    pub merged_commit: Option<String>,
    /// 冲突文件（`conflict`）。
    pub conflict_files: Vec<String>,
    /// 不能合并的原因（`blocked`）。
    pub reason: Option<String>,
    /// 合并成功后清理 worktree 或执行分支失败的说明（不影响合并结果）；没有问题为 `null`。
    pub cleanup_problem: Option<String>,
    /// 任务完成后自动转为 READY 的下游任务（验收页「合并成功 · 下游自动就绪」提示）。
    pub advanced_tasks: Vec<AdvancedTask>,
    /// 最新的任务。
    pub task: Task,
    /// 最新的执行记录。
    pub run: Run,
}
