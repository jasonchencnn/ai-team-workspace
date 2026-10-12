use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// Git 工作区信息（实时检测）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct WorkspaceInfo {
    /// Git 仓库顶层目录的绝对路径。
    pub path: String,
    /// 当前分支；处于 detached HEAD 时为 `null`。
    pub branch: Option<String>,
    /// 当前提交的短哈希（7 位）；仓库还没有提交时为 `null`。
    pub commit: Option<String>,
}

/// 工作区文件搜索。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct FileSearchQuery {
    pub team_id: String,
    /// 不区分大小写的子串匹配；为空时返回最前面的文件。
    pub query: String,
}
