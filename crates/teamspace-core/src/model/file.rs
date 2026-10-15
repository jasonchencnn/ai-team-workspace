use serde::{Deserialize, Serialize};
use ts_rs::TS;

use super::{RoleRef, TaskStatus};

/// 共享文件列表查询。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct MarkdownListQuery {
    pub team_id: String,
}

/// 共享文件列表：工作空间内全部 Markdown 文件的相对路径（`/` 分隔，按路径排序）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct MarkdownList {
    pub team_id: String,
    pub files: Vec<String>,
    /// 文件数达到返回上限、可能还有更多时为 `true`（见 [`crate::probe::MARKDOWN_FILE_LIMIT`]）。
    pub truncated: bool,
}

/// 读取单个 Markdown 文件。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct MarkdownReadQuery {
    pub team_id: String,
    /// 相对工作空间的路径（`/` 分隔）；可带 `#锚点`，锚点会被忽略。
    pub path: String,
}

/// 一个 Markdown 文件的内容与元信息。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct MarkdownFile {
    pub team_id: String,
    /// 规范化后的相对路径。
    pub path: String,
    pub content: String,
    /// 超过大小上限、内容被截断时为 `true`。
    pub truncated: bool,
    /// 文件大小（字节），截断时是截断前的大小。
    pub size: u64,
    /// 最后修改时间，UTC RFC3339。
    pub updated_at: String,
}

/// 查询引用了某个文件的任务。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct FileTasksQuery {
    pub team_id: String,
    /// 相对工作空间的路径（`/` 分隔），与任务文件引用里的 `path` 完全一致才算引用。
    pub path: String,
}

/// 引用了某个文件的任务（共享文件页右侧的「引用此文件的任务」）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct FileTaskRef {
    pub id: String,
    /// 任务编号，如 `FE-023`。
    pub key: String,
    pub title: String,
    pub status: TaskStatus,
    pub owner: RoleRef,
    /// 引用里带的锚点（`#` 之后的部分），没有则为 `null`。
    pub anchor: Option<String>,
    pub updated_at: String,
}
