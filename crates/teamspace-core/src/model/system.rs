use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// 工作空间信息（实时检测）。工作空间可以是任意本地文件夹，不要求是 Git 仓库。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct WorkspaceInfo {
    /// 工作空间目录的绝对路径（即用户所选目录本身，规范化后不带末尾斜杠）。
    pub path: String,
    /// 目录位于 Git 仓库中时的分支与提交；不是 Git 仓库时为 `null`。
    pub git: Option<GitInfo>,
}

/// 工作空间所在 Git 仓库的状态。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct GitInfo {
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
