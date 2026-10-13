//! 系统探测接口（M1-04 实现）。
//!
//! 内核通过 [`SystemProbe`] 访问 Git、工作空间文件和本机 CLI，便于在测试中替换为假实现。
//! 实现不得读取或保存任何密钥，也不得启动 Harness 执行任务；只允许 `--version` 一类只读调用。

use std::path::Path;

use crate::error::Result;
use crate::model::{HarnessDetection, HarnessInstance, HarnessKind, HealthCheck, WorkspaceInfo};

/// 工作空间文件搜索的返回上限。
pub const FILE_SEARCH_LIMIT: usize = 50;

/// 普通文件夹（非 Git）文件搜索时最多扫描的文件数，超过后停止遍历。
pub const MAX_SCANNED_FILES: usize = 20_000;

pub trait SystemProbe: Send + Sync {
    /// 探测工作空间目录。`path` 由内核规范化为已存在目录的绝对路径后传入，内核会以它覆盖
    /// 返回值的 `path`（工作空间就是所选目录本身，不提升为仓库根）。
    ///
    /// 目录位于 Git 仓库中（含仓库子目录）时 `git` 为 `Some`：当前分支（detached 时为 `None`）
    /// 和 7 位短提交（无提交时为 `None`）；不是 Git 仓库时 `git` 为 `None`，**不是错误**。
    /// 只有目录不存在、不是目录或 Git 本身无法运行等情况才返回错误。
    fn inspect_workspace(&self, path: &Path) -> Result<WorkspaceInfo>;

    /// 在工作空间中按不区分大小写的子串搜索文件，返回相对工作空间目录、`/` 分隔的路径，
    /// 最多 `limit` 条，排序稳定（建议：文件名命中优先，其次路径短者优先）。搜索范围：
    ///
    /// - Git 目录：`git ls-files --cached --others --exclude-standard`（相对所选目录）；
    /// - 普通文件夹：遍历目录树，跳过隐藏文件与目录（以 `.` 开头）以及 `node_modules`、`target`、
    ///   `dist`、`build` 等常见大目录，扫描 [`MAX_SCANNED_FILES`] 个文件后停止。
    fn search_files(&self, workspace: &Path, query: &str, limit: usize) -> Result<Vec<String>>;

    /// 在本机查找 `kind` 对应的 CLI（`claude` / `codex`）并读取版本。
    /// PATH 需补充登录 shell 的 PATH 和常见安装目录。内核不会对 `mock` 调用此方法。
    fn detect_harness(&self, kind: HarnessKind) -> HarnessDetection;

    /// 对已注册的实例做健康检查：使用 `executable` 覆盖路径（否则自动查找）运行版本命令；
    /// Claude Code 若配置了 `profile`（settings 文件）需检查文件存在。
    /// 返回的 `checked_at` 可为 `None`，由内核写入检查时间。内核不会对 `mock` 调用此方法。
    fn check_harness(&self, harness: &HarnessInstance) -> HealthCheck;
}
