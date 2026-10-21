//! 数据备份与恢复（M7-04）的对外类型（IPC 契约）。
//!
//! 约定：
//! - 字段在 JSON / TS 中为 camelCase；时间为 UTC RFC3339 字符串。
//! - 备份目录结构：`manifest.json`（清单）、`database/teamspace.db`（数据库一致性快照）、
//!   `workspaces/<团队标识>/<岗位文件夹>/`（各团队工作空间下的岗位文件夹副本）。
//! - 备份与恢复都只能由 Human 在应用里操作（Leader 无权，见 [`crate::dispatch`]）。

use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// 导出备份的请求。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ExportBackupInput {
    /// 备份的存放目录（不存在会创建）；备份目录本身建在它下面。
    pub dest_dir: String,
    /// 应用版本号（由宿主应用提供；缺省时用内核版本）。
    #[serde(default)]
    pub app_version: Option<String>,
}

/// 读取备份摘要的请求。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct InspectBackupInput {
    /// 备份目录（含 `manifest.json` 的那一层）。
    pub backup_dir: String,
}

/// 恢复备份的请求。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct RestoreBackupInput {
    /// 备份目录（含 `manifest.json` 的那一层）。
    pub backup_dir: String,
}

/// 备份清单里的一个文件。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct BackupFile {
    /// 相对备份目录的路径（`/` 分隔）。
    pub path: String,
    /// 文件字节数。
    pub size: u64,
}

/// 备份清单里的一个岗位文件夹。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct BackupRoleFolder {
    /// 岗位标识。
    pub role_id: String,
    /// 岗位名。
    pub role_name: String,
    /// 相对工作空间的路径（`Teamspace/<岗位文件夹>`）。
    pub relative_path: String,
    /// 相对备份目录的路径（`workspaces/<团队标识>/<岗位文件夹>`）。
    pub backup_path: String,
    /// 复制的文件数。
    pub file_count: u32,
    /// 复制文件的总字节数。
    pub total_bytes: u64,
    /// 没复制成功时的原因（工作空间不可用、文件夹不存在等）。
    pub problem: Option<String>,
}

/// 备份清单里的一个团队（及其工作空间与岗位文件夹）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct BackupTeam {
    /// 团队标识。
    pub team_id: String,
    /// 团队名。
    pub team_name: String,
    /// 团队工作空间的绝对路径。
    pub workspace_path: String,
    /// 该团队各岗位的文件夹。
    pub role_folders: Vec<BackupRoleFolder>,
}

/// 备份清单（`manifest.json` 的内容）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct BackupManifest {
    /// 备份格式版本（比当前程序新时拒绝恢复）。
    pub format_version: u32,
    /// 导出时的应用版本号。
    pub app_version: String,
    /// 导出时的数据库 schema 版本（比当前程序新时拒绝恢复）。
    pub schema_version: i64,
    /// 导出时间（UTC RFC3339）。
    pub exported_at: String,
    /// 数据库快照相对备份目录的路径。
    pub database_file: String,
    /// 团队与工作空间路径列表。
    pub teams: Vec<BackupTeam>,
    /// 备份内所有文件的清单与大小。
    pub files: Vec<BackupFile>,
}

/// 导出备份的结果。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct BackupExportResult {
    /// 备份目录的绝对路径。
    pub backup_dir: String,
    /// 清单文件的绝对路径。
    pub manifest_path: String,
    /// 数据库快照的绝对路径。
    pub database_path: String,
    /// 导出时间（UTC RFC3339）。
    pub exported_at: String,
    /// 导出时的 schema 版本。
    pub schema_version: i64,
    /// 团队数。
    pub team_count: u32,
    /// 文件总数（含数据库快照）。
    pub file_count: u32,
    /// 文件总字节数。
    pub total_bytes: u64,
    /// 团队与工作空间路径列表。
    pub teams: Vec<BackupTeam>,
}

/// 备份摘要（恢复前给 Human 看的那一份）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct BackupSummary {
    /// 备份目录的绝对路径。
    pub backup_dir: String,
    /// 备份格式版本。
    pub format_version: u32,
    /// 导出时的应用版本号。
    pub app_version: String,
    /// 导出时的 schema 版本。
    pub schema_version: i64,
    /// 导出时间（UTC RFC3339）。
    pub exported_at: String,
    /// 团队数。
    pub team_count: u32,
    /// 文件总数。
    pub file_count: u32,
    /// 文件总字节数。
    pub total_bytes: u64,
    /// 团队与工作空间路径列表。
    pub teams: Vec<BackupTeam>,
    /// 恢复后需要跑数据库迁移（备份比当前程序旧）。
    pub needs_migration: bool,
    /// 数据库快照相对备份目录的路径。
    pub database_file: String,
}

/// 恢复时跳过的岗位文件夹。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct SkippedRoleFolder {
    /// 岗位标识。
    pub role_id: String,
    /// 岗位名。
    pub role_name: String,
    /// 工作空间路径。
    pub workspace_path: String,
    /// 跳过的原因。
    pub reason: String,
}

/// 恢复备份的结果。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct BackupRestoreResult {
    /// 被恢复的备份目录。
    pub backup_dir: String,
    /// 备份里的 schema 版本。
    pub schema_version: i64,
    /// 恢复前自动导出的备份目录（回退用）。
    pub pre_restore_backup_dir: String,
    /// 已写回原工作空间的岗位文件夹（相对工作空间的路径）。
    pub restored_folders: Vec<String>,
    /// 没能写回的岗位文件夹。
    pub skipped_folders: Vec<SkippedRoleFolder>,
    /// 数据库是否已恢复。
    pub database_restored: bool,
    /// 是否需要重启应用才完全生效。
    pub requires_restart: bool,
    /// 补充说明（迁移、跳过原因等）。
    pub notes: Vec<String>,
}
