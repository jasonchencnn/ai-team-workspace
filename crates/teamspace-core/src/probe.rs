//! 系统探测接口（M1-04 实现）。
//!
//! 内核通过 [`SystemProbe`] 访问 Git、工作区文件和本机 CLI，便于在测试中替换为假实现。
//! 实现不得读取或保存任何密钥，也不得启动 Harness 执行任务；只允许 `--version` 一类只读调用。

use std::path::Path;

use crate::error::Result;
use crate::model::{HarnessDetection, HarnessInstance, HarnessKind, HealthCheck, WorkspaceInfo};

/// 工作区文件搜索的返回上限。
pub const FILE_SEARCH_LIMIT: usize = 50;

pub trait SystemProbe: Send + Sync {
    /// 解析 `path` 所在的 Git 仓库，返回顶层目录绝对路径、当前分支（detached 时为 `None`）
    /// 和 7 位短提交（无提交时为 `None`）。路径不存在或不在 Git 仓库中返回 `Error::Invalid`。
    fn inspect_workspace(&self, path: &Path) -> Result<WorkspaceInfo>;

    /// 在工作区中按不区分大小写的子串搜索文件，范围为
    /// `git ls-files --cached --others --exclude-standard`；返回 `/` 分隔的相对路径，
    /// 最多 `limit` 条，排序稳定（建议：文件名命中优先，其次路径短者优先）。
    fn search_files(&self, workspace: &Path, query: &str, limit: usize) -> Result<Vec<String>>;

    /// 在本机查找 `kind` 对应的 CLI（`claude` / `codex`）并读取版本。
    /// PATH 需补充登录 shell 的 PATH 和常见安装目录。内核不会对 `mock` 调用此方法。
    fn detect_harness(&self, kind: HarnessKind) -> HarnessDetection;

    /// 对已注册的实例做健康检查：使用 `executable` 覆盖路径（否则自动查找）运行版本命令；
    /// Claude Code 若配置了 `profile`（settings 文件）需检查文件存在。
    /// 返回的 `checked_at` 可为 `None`，由内核写入检查时间。内核不会对 `mock` 调用此方法。
    fn check_harness(&self, harness: &HarnessInstance) -> HealthCheck;
}
