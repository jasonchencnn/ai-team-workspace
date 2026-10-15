//! 系统探测接口（M1-04 实现，M1.3 改为 ACP 握手）。
//!
//! 内核通过 [`SystemProbe`] 访问 Git、工作空间文件、本机命令和 ACP Agent 进程，便于在测试中替换为
//! 假实现。实现不得读取或保存任何密钥，也不得让 Harness 执行任务；对 Agent 只允许 ACP 握手
//! （`initialize`），不发送 prompt。

use std::ffi::OsString;
use std::path::Path;

use crate::acp::AgentLaunch;
use crate::error::Result;
use crate::model::{Handshake, WorkspaceInfo};

/// 工作空间文件搜索的返回上限。
pub const FILE_SEARCH_LIMIT: usize = 50;

/// 普通文件夹（非 Git）文件搜索时最多扫描的文件数，超过后停止遍历。
pub const MAX_SCANNED_FILES: usize = 20_000;

/// 共享文件页一次最多列出多少个 Markdown 文件，超过后前端显示「已截断」。
pub const MARKDOWN_FILE_LIMIT: usize = 5_000;

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

    /// 列出工作空间里的全部 Markdown 文件（共享文件页用），返回相对工作空间目录、`/` 分隔的路径，
    /// 按路径排序，最多 `limit` 个；列出 `limit` 个即视为可能还有更多（调用方据此标记截断）。
    ///
    /// 清单来源与 [`SystemProbe::search_files`] 相同：Git 目录问 `git ls-files`（已跟踪与未跟踪但未忽略），
    /// 普通文件夹遍历目录树。之后两套清单都过同一道筛：只要 `.md`，路径里不含以 `.` 开头的目录段
    /// （`.git` 等）也不含 `node_modules`、`target`、`dist`、`build`。
    ///
    /// **不跟随指向工作空间之外的符号链接**：链接解析后在工作空间外或读不到的一律不列。
    fn list_markdown_files(&self, workspace: &Path, limit: usize) -> Result<Vec<String>>;

    /// 在补全后的 PATH（进程 PATH + 登录 shell 的 PATH + 常见安装目录）中查找命令，返回可执行文件的
    /// 绝对路径；`command` 含 `/` 时按路径判断是否可执行。不运行任何程序。
    fn locate_command(&self, command: &str) -> Option<String>;

    /// 对 ACP Agent 做握手检查（见 [`crate::acp`]）：用补全后的 PATH 启动 `launch` 描述的进程
    /// （工作目录为临时目录，不注入额外环境变量），发送 `initialize`，解析响应后整组终止进程，
    /// 超时 [`crate::acp::HANDSHAKE_TIMEOUT`]。配置了 `env_profile` 时只检查文件存在，不读取内容。
    /// 失败体现在返回值中；`checked_at` 由内核写入。
    fn handshake(&self, launch: &AgentLaunch) -> Handshake;

    /// 在系统文件管理器（macOS 为 Finder）中打开已存在的路径（内核已校验）：目录直接打开，
    /// 文件则打开所在目录并选中该文件。打开失败时返回 internal 错误。
    fn reveal_path(&self, path: &Path) -> Result<()>;

    /// 用系统默认程序打开已存在的文件（内核已校验路径在执行目录内）。默认实现退回到
    /// [`SystemProbe::reveal_path`]；[`crate::system_probe::LocalProbe`] 改用系统的「打开」命令。
    fn open_path(&self, path: &Path) -> Result<()> {
        self.reveal_path(path)
    }

    /// 启动执行用的 Agent 进程和 Git worktree 操作时使用的 PATH（补全后的）。默认为进程继承的 PATH；
    /// [`crate::system_probe::LocalProbe`] 返回与 [`SystemProbe::locate_command`] 一致的补全 PATH。
    fn command_path_env(&self) -> OsString {
        std::env::var_os("PATH").unwrap_or_default()
    }
}
