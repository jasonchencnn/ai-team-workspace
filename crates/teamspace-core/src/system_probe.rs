//! 本机系统探测实现（M1-04）。
//!
//! [`LocalProbe`] 是 [`SystemProbe`] 的真实实现：只调用 Git 的只读查询和各 Harness CLI 的
//! `--version`，不读取也不保存任何密钥，也不启动 Harness 执行任务。
//!
//! macOS 上以 GUI 方式启动的应用继承不到登录 shell 的 PATH（Homebrew、NVM、Volta、Cargo
//! 装的 CLI 都会找不到），因此这里先用 `$SHELL -ilc` 打印一段带标记的 PATH，再补上常见安装
//! 目录，去重后缓存，所有子进程共用这一份 PATH。
//!
//! 依赖只用标准库：子进程超时用 `try_wait` 轮询实现，不引入新依赖。

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::OnceLock;
use std::thread;
use std::time::{Duration, Instant};

use crate::error::{Error, Result};
use crate::model::{
    HarnessDetection, HarnessInstance, HarnessKind, HealthCheck, HealthStatus, WorkspaceInfo,
};
use crate::probe::SystemProbe;

/// 读取登录 shell PATH 的超时。
const SHELL_PATH_TIMEOUT: Duration = Duration::from_secs(5);
/// 运行 `--version` 的超时。
const VERSION_TIMEOUT: Duration = Duration::from_secs(10);
/// Git 查询的超时（正常是毫秒级，给慢盘留余量）。
const GIT_TIMEOUT: Duration = Duration::from_secs(30);
/// 等待子进程退出时的轮询间隔。
const POLL_INTERVAL: Duration = Duration::from_millis(20);

/// 登录 shell 输出中 PATH 的起始标记。
const PATH_START_MARKER: &str = "__TS_PATH_START__";
/// 登录 shell 输出中 PATH 的结束标记。
const PATH_END_MARKER: &str = "__TS_PATH_END__";
/// 打印登录 shell PATH 的命令。用 `printf` 而不是 `echo -n`，避开不同 shell 的差异。
const SHELL_PATH_COMMAND: &str = "printf '__TS_PATH_START__%s__TS_PATH_END__' \"$PATH\"";

/// 除登录 shell 外还要补上的固定安装目录。
const EXTRA_BIN_DIRS: [&str; 2] = ["/opt/homebrew/bin", "/usr/local/bin"];
/// 除登录 shell 外还要补上的、相对用户主目录的安装目录。
const EXTRA_HOME_BIN_DIRS: [&str; 5] = [
    ".local/bin",
    ".npm-global/bin",
    ".bun/bin",
    ".volta/bin",
    ".cargo/bin",
];

/// [`SystemProbe`] 的真实实现，探测本机的 Git 工作区和 Harness CLI。
///
/// PATH 只解析一次并缓存在实例里（[`LocalProbe::new`] 之后不变），因此同一个实例的多次
/// 探测看到的是同一份环境。
#[derive(Debug, Default)]
pub struct LocalProbe {
    path: OnceLock<Vec<PathBuf>>,
}

impl LocalProbe {
    /// 创建探测器，PATH 在首次使用时解析。
    pub fn new() -> Self {
        Self::default()
    }

    /// 当前使用的 PATH 目录（去重后保序）。
    fn path_dirs(&self) -> &[PathBuf] {
        self.path.get_or_init(resolve_path_dirs)
    }

    /// 在给定 PATH 中查找 `kind` 对应的 CLI 并读取版本。
    fn detect_with(&self, kind: HarnessKind, dirs: &[PathBuf]) -> HarnessDetection {
        let command = match kind {
            HarnessKind::ClaudeCode => "claude",
            HarnessKind::Codex => "codex",
            // 内核不会对 mock 调用探测；这里不启动任何外部程序。
            HarnessKind::Mock => {
                return HarnessDetection {
                    kind,
                    found: false,
                    executable: None,
                    version: None,
                    error: Some("mock 不需要检测本机 CLI".to_string()),
                };
            }
        };

        let Some(executable) = find_in_path(command, dirs) else {
            return HarnessDetection {
                kind,
                found: false,
                executable: None,
                version: None,
                error: Some(format!("未找到 {command} 命令：请确认已安装并加入 PATH")),
            };
        };

        match self.version_of(&executable, dirs, VERSION_TIMEOUT) {
            Ok(version) => HarnessDetection {
                kind,
                found: true,
                executable: Some(executable.display().to_string()),
                version: Some(version),
                error: None,
            },
            Err(message) => HarnessDetection {
                kind,
                found: false,
                executable: None,
                version: None,
                error: Some(message),
            },
        }
    }

    /// 用给定 PATH 和 Codex 配置目录做健康检查（拆出来是为了能传临时目录进测试）。
    fn check_with(
        &self,
        harness: &HarnessInstance,
        dirs: &[PathBuf],
        codex_home: &Path,
    ) -> HealthCheck {
        if harness.kind == HarnessKind::Mock {
            // 内核不会对 mock 调用检查；不启动任何外部程序。
            return HealthCheck {
                status: HealthStatus::Unknown,
                version: None,
                detail: Some("mock 不做本机检查".to_string()),
                checked_at: None,
            };
        }

        let executable = match resolve_executable(harness, dirs) {
            Ok(path) => path,
            Err(message) => return health_error(None, message),
        };

        let version = match self.version_of(&executable, dirs, VERSION_TIMEOUT) {
            Ok(version) => version,
            Err(message) => return health_error(None, message),
        };

        let detail = format!("使用 {}（版本 {version}）", executable.display());
        match check_profile(harness, codex_home) {
            Ok(None) => HealthCheck {
                status: HealthStatus::Ok,
                version: Some(version),
                detail: Some(detail),
                checked_at: None,
            },
            Ok(Some(note)) => HealthCheck {
                status: HealthStatus::Ok,
                version: Some(version),
                detail: Some(format!("{detail}；{note}")),
                checked_at: None,
            },
            Err(message) => health_error(Some(version), message),
        }
    }

    /// 运行 `<exe> --version` 并解析版本号，失败时给出中文原因。
    fn version_of(
        &self,
        executable: &Path,
        dirs: &[PathBuf],
        timeout: Duration,
    ) -> std::result::Result<String, String> {
        let mut command = Command::new(executable);
        command.arg("--version");
        command.env("PATH", path_env(dirs));
        command.stdin(Stdio::null());

        let output = run_command(&mut command, timeout)
            .map_err(|err| format!("无法运行 {}：{err}", executable.display()))?;
        if output.timed_out {
            return Err(format!(
                "{} --version 超时（超过 {} 秒）",
                executable.display(),
                timeout.as_secs()
            ));
        }
        if !output.success {
            let reason = first_non_empty_line(&output.stderr)
                .or_else(|| first_non_empty_line(&output.stdout))
                .unwrap_or_else(|| "无输出".to_string());
            return Err(format!("{} --version 失败：{reason}", executable.display()));
        }
        parse_version(&output.stdout)
            .ok_or_else(|| format!("{} 未返回可解析的版本号", executable.display()))
    }

    /// 执行一条 Git 查询。`Ok(Some(text))` 为成功，`Ok(None)` 为 Git 正常拒绝（退出码非 0），
    /// `Err` 为 Git 本身跑不起来。
    fn git_text(&self, path: &Path, args: &[&str]) -> Result<Option<String>> {
        let mut command = Command::new("git");
        command.arg("-C").arg(path);
        command.args(args);
        command.stdin(Stdio::null());
        command.env("PATH", path_env(self.path_dirs()));
        // 不做交互式凭据提问，避免远程仓库查询把界面卡住。
        command.env("GIT_TERMINAL_PROMPT", "0");

        let output = run_command(&mut command, GIT_TIMEOUT)
            .map_err(|err| Error::internal(format!("无法运行 git：{err}")))?;
        if output.timed_out {
            return Err(Error::internal(format!(
                "git {} 超时，请稍后重试",
                args.first().copied().unwrap_or_default()
            )));
        }
        if !output.success {
            return Ok(None);
        }
        Ok(first_non_empty_line(&output.stdout))
    }
}

impl SystemProbe for LocalProbe {
    fn inspect_workspace(&self, path: &Path) -> Result<WorkspaceInfo> {
        if !path.exists() {
            return Err(Error::invalid(format!(
                "所选目录不存在：{}",
                path.display()
            )));
        }
        if !path.is_dir() {
            return Err(Error::invalid(format!(
                "所选路径不是目录：{}",
                path.display()
            )));
        }

        let root = self
            .git_text(path, &["rev-parse", "--show-toplevel"])?
            .ok_or_else(|| Error::invalid(format!("所选目录不是 Git 仓库：{}", path.display())))?;

        // detached HEAD 时 symbolic-ref 退出码非 0，即分支为 None。
        let branch = self.git_text(path, &["symbolic-ref", "--short", "-q", "HEAD"])?;
        // 还没有任何提交时 rev-parse 失败，即提交为 None。
        let commit = self.git_text(path, &["rev-parse", "--short=7", "HEAD"])?;

        Ok(WorkspaceInfo {
            path: root,
            branch,
            commit,
        })
    }

    fn search_files(&self, workspace: &Path, query: &str, limit: usize) -> Result<Vec<String>> {
        let mut command = Command::new("git");
        command.arg("-C").arg(workspace);
        command.args([
            "ls-files",
            "--cached",
            "--others",
            "--exclude-standard",
            "-z",
        ]);
        command.stdin(Stdio::null());
        command.env("PATH", path_env(self.path_dirs()));
        command.env("GIT_TERMINAL_PROMPT", "0");

        let output = run_command(&mut command, GIT_TIMEOUT)
            .map_err(|err| Error::internal(format!("无法运行 git：{err}")))?;
        if output.timed_out {
            return Err(Error::internal("搜索文件超时，请稍后重试"));
        }
        if !output.success {
            return Err(Error::invalid(format!(
                "所选目录不是 Git 仓库：{}",
                workspace.display()
            )));
        }

        Ok(select_files(&output.stdout, query, limit))
    }

    fn detect_harness(&self, kind: HarnessKind) -> HarnessDetection {
        self.detect_with(kind, self.path_dirs())
    }

    fn check_harness(&self, harness: &HarnessInstance) -> HealthCheck {
        let codex_home = codex_home_dir();
        self.check_with(harness, self.path_dirs(), &codex_home)
    }
}

/// 一条子进程的执行结果。
struct CommandOutput {
    success: bool,
    stdout: String,
    stderr: String,
    timed_out: bool,
}

/// 运行子进程并在超时后杀掉它。标准库没有跨平台超时，用 `try_wait` 轮询实现。
///
/// stdout / stderr 各自在一个线程里读，避免管道写满后互相死锁。
fn run_command(command: &mut Command, timeout: Duration) -> std::io::Result<CommandOutput> {
    let mut child = command
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;

    let out_handle = child
        .stdout
        .take()
        .map(|stream| thread::spawn(move || read_stream(stream)));
    let err_handle = child
        .stderr
        .take()
        .map(|stream| thread::spawn(move || read_stream(stream)));

    let deadline = Instant::now() + timeout;
    let mut timed_out = false;
    let status = loop {
        match child.try_wait()? {
            Some(status) => break status,
            None => {
                if Instant::now() >= deadline {
                    timed_out = true;
                    let _ = child.kill();
                    break child.wait()?;
                }
                thread::sleep(POLL_INTERVAL);
            }
        }
    };

    let join = |handle: Option<thread::JoinHandle<String>>| {
        handle.map_or_else(String::new, |h| h.join().unwrap_or_default())
    };
    Ok(CommandOutput {
        success: status.success(),
        stdout: join(out_handle),
        stderr: join(err_handle),
        timed_out,
    })
}

/// 读完一个管道并按 UTF-8 宽松解码（文件路径理论上都是 UTF-8，但不因为个别坏字节丢掉整条命令的输出）。
fn read_stream<R: std::io::Read>(mut stream: R) -> String {
    let mut buffer = Vec::new();
    let _ = stream.read_to_end(&mut buffer);
    String::from_utf8_lossy(&buffer).into_owned()
}

/// 解析 `--version` 的输出：取第一个非空行里的版本号，解析不到就保留原始首行。
///
/// `2.1.296 (Claude Code)` → `2.1.296`，`codex-cli 0.145.0` → `0.145.0`，`hello` → `hello`。
fn parse_version(output: &str) -> Option<String> {
    let line = first_non_empty_line(output)?;
    if let Some(token) = line
        .split_whitespace()
        .find(|token| looks_like_version(token))
    {
        return Some(token.trim_start_matches('v').to_string());
    }
    Some(line)
}

/// 是否是 `2.1.296` / `v0.145.0` 这样的版本号。
fn looks_like_version(token: &str) -> bool {
    let body = token.strip_prefix('v').unwrap_or(token);
    !body.is_empty() && body.contains('.') && body.chars().all(|c| c.is_ascii_digit() || c == '.')
}

/// 取出 `git ls-files -z` 的输出并按规则筛选排序。
///
/// 命中判断不区分大小写；排序为「文件名命中 → 路径更短 → 字典序」，最多 `limit` 条。
fn select_files(stdout: &str, query: &str, limit: usize) -> Vec<String> {
    if limit == 0 {
        return Vec::new();
    }
    let needle = query.trim().to_lowercase();

    let mut files: Vec<String> = stdout
        .split('\0')
        .filter(|path| !path.is_empty())
        .filter(|path| needle.is_empty() || path.to_lowercase().contains(&needle))
        .map(str::to_string)
        .collect();

    files.sort_by(|a, b| {
        match_rank(a, &needle)
            .cmp(&match_rank(b, &needle))
            .then_with(|| a.len().cmp(&b.len()))
            .then_with(|| a.cmp(b))
    });
    files.truncate(limit);
    files
}

/// 命中层级：0 = 文件名命中，1 = 只在目录里命中，2 = 没命中。
fn match_rank(path: &str, needle: &str) -> u8 {
    if needle.is_empty() {
        return 0;
    }
    let lower = path.to_lowercase();
    if !lower.contains(needle) {
        return 2;
    }
    let name = lower.rsplit('/').next().unwrap_or(lower.as_str());
    if name.contains(needle) { 0 } else { 1 }
}

/// 从登录 shell 的输出里取出两个标记之间的 PATH；取不到返回 `None`。
fn extract_shell_path(output: &str) -> Option<String> {
    let start = output.find(PATH_START_MARKER)? + PATH_START_MARKER.len();
    let end = output.find(PATH_END_MARKER).filter(|end| *end >= start)?;
    let path = output[start..end].trim();
    (!path.is_empty()).then(|| path.to_string())
}

/// 解析本机 PATH：进程继承的 PATH + 登录 shell 的 PATH + 常见安装目录，去重后保序。
fn resolve_path_dirs() -> Vec<PathBuf> {
    let mut dirs: Vec<PathBuf> = Vec::new();

    let mut push = |dir: PathBuf| {
        if !dir.as_os_str().is_empty() && !dirs.contains(&dir) {
            dirs.push(dir);
        }
    };

    if let Some(path) = std::env::var_os("PATH") {
        for dir in std::env::split_paths(&path) {
            push(dir);
        }
    }

    if let Some(shell_path) = login_shell_path() {
        let shell_path = std::ffi::OsString::from(shell_path);
        for dir in std::env::split_paths(&shell_path) {
            push(dir);
        }
    }

    if let Some(home) = home_dir() {
        for relative in EXTRA_HOME_BIN_DIRS {
            push(home.join(relative));
        }
    }
    for absolute in EXTRA_BIN_DIRS {
        push(PathBuf::from(absolute));
    }

    dirs
}

/// 让登录 shell 打印自己的 PATH。任何失败（没有 SHELL、shell 报错、超时）都返回 `None`。
fn login_shell_path() -> Option<String> {
    let shell = std::env::var_os("SHELL")
        .filter(|shell| !shell.is_empty())
        .or_else(shell_fallback)?;

    let mut command = Command::new(shell);
    command.arg("-ilc").arg(SHELL_PATH_COMMAND);
    command.env("DISABLE_AUTO_UPDATE", "true");
    command.stdin(Stdio::null());
    // 解析本身不该依赖 PATH，只求 shell 能起来；这里仍用进程继承的 PATH 作为初值。
    command.env("PATH", path_env(&inherited_path_dirs()));

    let output = run_command(&mut command, SHELL_PATH_TIMEOUT).ok()?;
    if output.timed_out || !output.success {
        return None;
    }
    extract_shell_path(&output.stdout)
}

fn shell_fallback() -> Option<std::ffi::OsString> {
    ["/bin/zsh", "/bin/bash", "/bin/sh"]
        .into_iter()
        .map(std::ffi::OsString::from)
        .find(|shell| Path::new(shell).is_file())
}

fn inherited_path_dirs() -> Vec<PathBuf> {
    match std::env::var_os("PATH") {
        Some(path) => std::env::split_paths(&path).collect(),
        None => Vec::new(),
    }
}

/// 在 `dirs` 中按名字查找可执行文件；名字本身含分隔符时按路径处理。
fn find_in_path(command: &str, dirs: &[PathBuf]) -> Option<PathBuf> {
    if command.contains('/') {
        let direct = PathBuf::from(command);
        return is_executable(&direct).then_some(direct);
    }
    dirs.iter()
        .map(|dir| dir.join(command))
        .find(|candidate| is_executable(candidate))
}

#[cfg(unix)]
fn is_executable(path: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    path.is_file()
        && path
            .metadata()
            .is_ok_and(|meta| meta.permissions().mode() & 0o111 != 0)
}

#[cfg(not(unix))]
fn is_executable(path: &Path) -> bool {
    path.is_file()
}

/// 实例要用的可执行文件：优先 `executable` 覆盖路径，否则按 kind 自动查找。
fn resolve_executable(
    harness: &HarnessInstance,
    dirs: &[PathBuf],
) -> std::result::Result<PathBuf, String> {
    if let Some(configured) = harness
        .executable
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        let path = PathBuf::from(configured);
        if is_executable(&path) {
            return Ok(path);
        }
        return Err(format!("配置的可执行文件不存在或不可执行：{configured}"));
    }

    let command = match harness.kind {
        HarnessKind::ClaudeCode => "claude",
        HarnessKind::Codex => "codex",
        HarnessKind::Mock => return Err("mock 不做本机检查".to_string()),
    };
    find_in_path(command, dirs)
        .ok_or_else(|| format!("未找到 {command} 命令：请确认已安装并加入 PATH"))
}

/// 校验配置的 profile：Claude Code 是 settings 文件路径，Codex 是 `~/.codex` 里的 profile 名。
///
/// `Ok(None)` 表示没有配置 profile；`Ok(Some(note))` 表示已确认存在。
fn check_profile(
    harness: &HarnessInstance,
    codex_home: &Path,
) -> std::result::Result<Option<String>, String> {
    let Some(profile) = harness
        .profile
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    else {
        return Ok(None);
    };

    match harness.kind {
        HarnessKind::ClaudeCode => {
            let settings = Path::new(profile);
            if settings.is_file() {
                Ok(Some(format!("配置文件 {profile} 存在")))
            } else {
                Err(format!("未找到 Claude Code 配置文件：{profile}"))
            }
        }
        HarnessKind::Codex => {
            let standalone = codex_home.join(format!("{profile}.config.toml"));
            if standalone.is_file() {
                return Ok(Some(format!("profile 文件 {profile}.config.toml 存在")));
            }
            let config = codex_home.join("config.toml");
            let matched = std::fs::read_to_string(&config)
                .is_ok_and(|content| config_toml_has_profile(&content, profile));
            if matched {
                Ok(Some(format!("config.toml 中有 profile {profile}")))
            } else {
                Err(format!("未找到 Codex profile：{profile}"))
            }
        }
        HarnessKind::Mock => Ok(None),
    }
}

/// 简单文本匹配：`config.toml` 里是否有该 profile 的配置段。
fn config_toml_has_profile(content: &str, profile: &str) -> bool {
    if profile.is_empty() {
        return false;
    }
    content.lines().any(|line| {
        let Some(section) = line
            .trim()
            .strip_prefix('[')
            .and_then(|rest| rest.split_once(']').map(|(head, _)| head))
        else {
            return false;
        };
        let section = unquote(section.trim());
        section == profile
            || section
                .strip_prefix("profiles.")
                .is_some_and(|rest| unquote(rest) == profile)
    })
}

fn unquote(value: &str) -> &str {
    value.trim_matches('"').trim_matches('\'')
}

/// 进程当前的用户主目录。
fn home_dir() -> Option<PathBuf> {
    std::env::var_os("HOME")
        .filter(|home| !home.is_empty())
        .or_else(|| std::env::var_os("USERPROFILE").filter(|home| !home.is_empty()))
        .map(PathBuf::from)
}

/// Codex 配置目录，优先 `$CODEX_HOME`，否则 `~/.codex`。
fn codex_home_dir() -> PathBuf {
    if let Some(configured) = std::env::var_os("CODEX_HOME").filter(|home| !home.is_empty()) {
        return PathBuf::from(configured);
    }
    home_dir().map_or_else(|| PathBuf::from(".codex"), |home| home.join(".codex"))
}

/// 把目录列表拼成给子进程用的 `PATH`。
fn path_env(dirs: &[PathBuf]) -> std::ffi::OsString {
    std::env::join_paths(dirs).unwrap_or_default()
}

/// 第一个非空行（去掉首尾空白），用于「查询无输出」的判断。
fn first_non_empty_line(text: &str) -> Option<String> {
    text.lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .map(str::to_string)
}

fn health_error(version: Option<String>, message: String) -> HealthCheck {
    HealthCheck {
        status: HealthStatus::Error,
        version,
        detail: Some(message),
        checked_at: None,
    }
}

#[cfg(test)]
mod tests {
    use std::os::unix::fs::PermissionsExt;

    use super::*;

    fn git(dir: &Path, args: &[&str]) {
        let status = Command::new("git")
            .arg("-C")
            .arg(dir)
            .args(args)
            .stdin(Stdio::null())
            .status()
            .unwrap_or_else(|err| panic!("git {args:?} 运行失败：{err}"));
        assert!(status.success(), "git {args:?} 退出码 {status}");
    }

    /// 建一个临时 Git 仓库，`commit` 为真时提交一次。分支固定为 `probe-branch`。
    fn temp_repo(commit: bool) -> tempfile::TempDir {
        let dir = tempfile::tempdir().expect("创建临时目录");
        git(dir.path(), &["init", "--quiet"]);
        git(
            dir.path(),
            &["symbolic-ref", "HEAD", "refs/heads/probe-branch"],
        );
        std::fs::write(dir.path().join("README.md"), "probe\n").unwrap();
        if commit {
            git(dir.path(), &["add", "-A"]);
            git(
                dir.path(),
                &[
                    "-c",
                    "user.name=probe",
                    "-c",
                    "user.email=probe@example.com",
                    "commit",
                    "--quiet",
                    "--no-gpg-sign",
                    "-m",
                    "init",
                ],
            );
        }
        dir
    }

    fn canonical(path: &Path) -> String {
        std::fs::canonicalize(path)
            .expect("解析真实路径")
            .display()
            .to_string()
    }

    fn harness(kind: HarnessKind) -> HarnessInstance {
        HarnessInstance {
            id: "harness-1".to_string(),
            team_id: "team-1".to_string(),
            name: "local".to_string(),
            kind,
            executable: None,
            model: None,
            profile: None,
            health: HealthCheck {
                status: HealthStatus::Unknown,
                version: None,
                detail: None,
                checked_at: None,
            },
            created_at: "2026-01-01T00:00:00Z".to_string(),
            updated_at: "2026-01-01T00:00:00Z".to_string(),
            archived_at: None,
        }
    }

    /// 在 `dir` 里放一个假可执行脚本并返回它的路径。
    fn fake_executable(dir: &Path, name: &str, body: &str) -> PathBuf {
        std::fs::create_dir_all(dir).unwrap();
        let path = dir.join(name);
        std::fs::write(&path, format!("#!/bin/sh\n{body}\n")).unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
        path
    }

    /// 测试用 PATH：只放临时目录，**不含**进程继承的 PATH。
    ///
    /// 故意不追加继承的 PATH，否则本机真的装了 claude/codex 时，测试会找到并运行真实 CLI。
    /// 假脚本只使用 `sh` 内建命令，外部命令一律写绝对路径，所以这个最小 PATH 足够用。
    fn test_dirs(bin: &Path) -> Vec<PathBuf> {
        vec![bin.to_path_buf()]
    }

    #[test]
    fn inspect_reports_top_level_branch_and_commit() {
        let dir = temp_repo(true);
        let probe = LocalProbe::new();

        let info = probe
            .inspect_workspace(dir.path())
            .expect("应识别为 Git 仓库");
        assert_eq!(info.path, canonical(dir.path()));
        assert_eq!(info.branch.as_deref(), Some("probe-branch"));
        let commit = info.commit.expect("有提交时应返回短哈希");
        assert_eq!(commit.len(), 7);
        assert!(commit.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn inspect_resolves_top_level_from_subdirectory() {
        let dir = temp_repo(true);
        let nested = dir.path().join("crates");
        std::fs::create_dir_all(&nested).unwrap();
        let probe = LocalProbe::new();

        let info = probe
            .inspect_workspace(&nested)
            .expect("子目录也应解析到顶层");
        assert_eq!(info.path, canonical(dir.path()));
    }

    #[test]
    fn inspect_reports_none_commit_for_empty_repository() {
        let dir = temp_repo(false);
        let probe = LocalProbe::new();

        let info = probe.inspect_workspace(dir.path()).expect("空仓库也是仓库");
        assert_eq!(info.path, canonical(dir.path()));
        assert_eq!(info.commit, None);
        assert_eq!(info.branch.as_deref(), Some("probe-branch"));
    }

    #[test]
    fn inspect_reports_none_branch_when_detached() {
        let dir = temp_repo(true);
        git(
            dir.path(),
            &[
                "-c",
                "advice.detachedHead=false",
                "checkout",
                "--quiet",
                "--detach",
                "HEAD",
            ],
        );
        let probe = LocalProbe::new();

        let info = probe
            .inspect_workspace(dir.path())
            .expect("detached HEAD 仍是仓库");
        assert_eq!(info.branch, None);
        assert!(info.commit.is_some());
    }

    #[test]
    fn inspect_rejects_missing_directory() {
        // 先建一个唯一的临时目录再删掉，保证这个路径确实不存在。
        let dir = tempfile::tempdir().unwrap();
        let gone = dir.path().to_path_buf();
        drop(dir);
        assert!(!gone.exists());

        let probe = LocalProbe::new();
        let err = probe
            .inspect_workspace(&gone.join("not-here"))
            .expect_err("目录不存在应报错");
        assert!(matches!(err, Error::Invalid(_)));
        assert!(err.to_string().contains("不存在"));
    }

    #[test]
    fn inspect_rejects_directory_that_is_not_a_repository() {
        let dir = tempfile::tempdir().expect("创建临时目录");
        let probe = LocalProbe::new();

        let err = probe
            .inspect_workspace(dir.path())
            .expect_err("非仓库目录应报错");
        assert!(matches!(err, Error::Invalid(_)));
        assert!(err.to_string().contains("不是 Git 仓库"));
    }

    #[test]
    fn inspect_rejects_file_path() {
        let dir = tempfile::tempdir().expect("创建临时目录");
        let file = dir.path().join("notes.txt");
        std::fs::write(&file, "x").unwrap();
        let probe = LocalProbe::new();

        let err = probe.inspect_workspace(&file).expect_err("文件路径应报错");
        assert!(matches!(err, Error::Invalid(_)));
        assert!(err.to_string().contains("不是目录"));
    }

    #[test]
    fn search_lists_tracked_and_untracked_but_not_ignored() {
        let dir = temp_repo(false);
        std::fs::write(dir.path().join(".gitignore"), "ignored.txt\nbuild/\n").unwrap();
        std::fs::create_dir_all(dir.path().join("build")).unwrap();
        std::fs::write(dir.path().join("ignored.txt"), "x").unwrap();
        std::fs::write(dir.path().join("build/out.bin"), "x").unwrap();
        std::fs::write(dir.path().join("target.rs"), "fn main() {}").unwrap();
        std::fs::create_dir_all(dir.path().join("src/deep/nested")).unwrap();
        std::fs::write(dir.path().join("src/deep/nested/target.rs"), "fn main() {}").unwrap();
        std::fs::create_dir_all(dir.path().join("docs/target")).unwrap();
        std::fs::write(dir.path().join("docs/target/notes.md"), "# notes").unwrap();
        git(dir.path(), &["add", "-A"]);

        let probe = LocalProbe::new();
        let files = probe
            .search_files(dir.path(), "target", 50)
            .expect("搜索应成功");

        // 文件名命中优先，其次路径短者优先；ignored 与 build/ 不在结果里。
        assert_eq!(
            files,
            vec![
                "target.rs".to_string(),
                "src/deep/nested/target.rs".to_string(),
                "docs/target/notes.md".to_string(),
            ]
        );
    }

    #[test]
    fn search_is_case_insensitive_and_respects_limit() {
        let dir = temp_repo(false);
        std::fs::create_dir_all(dir.path().join("src")).unwrap();
        std::fs::write(dir.path().join("src/Probe.TS"), "x").unwrap();
        std::fs::write(dir.path().join("probe.md"), "x").unwrap();
        std::fs::write(dir.path().join("other.md"), "x").unwrap();

        let probe = LocalProbe::new();
        let all = probe
            .search_files(dir.path(), "PROBE", 50)
            .expect("搜索应成功");
        assert_eq!(
            all,
            vec!["probe.md".to_string(), "src/Probe.TS".to_string()]
        );

        let limited = probe
            .search_files(dir.path(), "probe", 1)
            .expect("搜索应成功");
        assert_eq!(limited, vec!["probe.md".to_string()]);
        assert!(
            probe
                .search_files(dir.path(), "probe", 0)
                .expect("搜索应成功")
                .is_empty()
        );
    }

    #[test]
    fn search_without_query_returns_leading_files() {
        let dir = temp_repo(false);
        std::fs::write(dir.path().join("a.md"), "x").unwrap();
        std::fs::create_dir_all(dir.path().join("z")).unwrap();
        std::fs::write(dir.path().join("z/b.md"), "x").unwrap();

        let probe = LocalProbe::new();
        let files = probe
            .search_files(dir.path(), "  ", 50)
            .expect("搜索应成功");
        assert_eq!(
            files,
            vec![
                "a.md".to_string(),
                "z/b.md".to_string(),
                "README.md".to_string()
            ]
        );

        let limited = probe.search_files(dir.path(), "", 1).expect("搜索应成功");
        assert_eq!(limited, vec!["a.md".to_string()]);
    }

    #[test]
    fn search_rejects_directory_that_is_not_a_repository() {
        let dir = tempfile::tempdir().expect("创建临时目录");
        let probe = LocalProbe::new();

        let err = probe
            .search_files(dir.path(), "", 10)
            .expect_err("非仓库目录应报错");
        assert!(matches!(err, Error::Invalid(_)));
        assert!(err.to_string().contains("不是 Git 仓库"));
    }

    #[test]
    fn parse_version_reads_claude_and_codex_output() {
        assert_eq!(
            parse_version("2.1.296 (Claude Code)\n").as_deref(),
            Some("2.1.296")
        );
        assert_eq!(
            parse_version("codex-cli 0.145.0\r\n").as_deref(),
            Some("0.145.0")
        );
        assert_eq!(parse_version("v1.0.22\n").as_deref(), Some("1.0.22"));
        assert_eq!(
            parse_version("\n  0.4.1  \n1.0.0\n").as_deref(),
            Some("0.4.1")
        );
    }

    #[test]
    fn parse_version_falls_back_to_raw_line_or_none() {
        assert_eq!(
            parse_version("totally unknown\n").as_deref(),
            Some("totally unknown")
        );
        assert_eq!(parse_version("   \n\n"), None);
    }

    #[test]
    fn extract_shell_path_reads_only_marked_segment() {
        let output =
            format!("zsh: noise\n{PATH_START_MARKER}/usr/bin:/bin{PATH_END_MARKER}\nbye\n");
        assert_eq!(
            extract_shell_path(&output).as_deref(),
            Some("/usr/bin:/bin")
        );
        assert_eq!(extract_shell_path("nothing here"), None);
        assert_eq!(
            extract_shell_path(&format!("{PATH_START_MARKER}{PATH_END_MARKER}")),
            None
        );
        assert_eq!(
            extract_shell_path(&format!("{PATH_END_MARKER}/bin{PATH_START_MARKER}")),
            None
        );
    }

    #[test]
    fn config_toml_matches_profile_sections() {
        let content = "model = \"gpt-5\"\n\n[profiles.work]\nmodel = \"gpt-5-codex\"\n";
        assert!(config_toml_has_profile(content, "work"));
        assert!(!config_toml_has_profile(content, "home"));

        let quoted = "[profiles.\"review\"]\nmodel = \"gpt-5\"\n";
        assert!(config_toml_has_profile(quoted, "review"));
        assert!(config_toml_has_profile("[solo]\n", "solo"));
        assert!(!config_toml_has_profile("[profiles.work.env]\n", "work"));
        assert!(!config_toml_has_profile("", "work"));
        assert!(!config_toml_has_profile("[profiles.work]\n", ""));
    }

    #[test]
    fn detect_reads_version_from_fake_cli() {
        let dir = tempfile::tempdir().unwrap();
        let executable = fake_executable(dir.path(), "claude", "echo '2.1.296 (Claude Code)'");
        let probe = LocalProbe::new();
        let dirs = test_dirs(dir.path());

        let detected = probe.detect_with(HarnessKind::ClaudeCode, &dirs);
        assert!(detected.found);
        assert_eq!(detected.version.as_deref(), Some("2.1.296"));
        assert_eq!(
            detected.executable.as_deref(),
            Some(executable.to_str().unwrap())
        );
        assert_eq!(detected.error, None);
    }

    #[test]
    fn detect_reads_codex_version_through_path() {
        let dir = tempfile::tempdir().unwrap();
        fake_executable(dir.path(), "codex", "echo 'codex-cli 0.145.0'");
        let probe = LocalProbe::new();

        let detected = probe.detect_with(HarnessKind::Codex, &test_dirs(dir.path()));
        assert!(detected.found);
        assert_eq!(detected.version.as_deref(), Some("0.145.0"));
    }

    #[test]
    fn detect_reports_missing_cli_in_chinese() {
        let dir = tempfile::tempdir().unwrap();
        let probe = LocalProbe::new();

        let detected = probe.detect_with(HarnessKind::ClaudeCode, &test_dirs(dir.path()));
        assert!(!detected.found);
        assert_eq!(detected.executable, None);
        assert_eq!(detected.version, None);
        assert_eq!(
            detected.error.as_deref(),
            Some("未找到 claude 命令：请确认已安装并加入 PATH")
        );
    }

    #[test]
    fn detect_does_not_run_anything_for_mock() {
        let probe = LocalProbe::new();
        let detected = probe.detect_with(HarnessKind::Mock, &[]);

        assert!(!detected.found);
        assert!(detected.error.unwrap().contains("mock"));
    }

    #[test]
    fn check_uses_executable_override() {
        let dir = tempfile::tempdir().unwrap();
        let executable = fake_executable(dir.path(), "my-claude", "echo '2.1.296 (Claude Code)'");
        let codex_home = tempfile::tempdir().unwrap();
        let mut instance = harness(HarnessKind::ClaudeCode);
        instance.executable = Some(executable.display().to_string());
        let probe = LocalProbe::new();

        let check = probe.check_with(&instance, &test_dirs(dir.path()), codex_home.path());
        assert_eq!(check.status, HealthStatus::Ok);
        assert_eq!(check.version.as_deref(), Some("2.1.296"));
        assert!(
            check
                .detail
                .unwrap()
                .contains(&executable.display().to_string())
        );
        assert_eq!(check.checked_at, None);
    }

    #[test]
    fn check_falls_back_to_path_lookup() {
        let dir = tempfile::tempdir().unwrap();
        fake_executable(dir.path(), "codex", "echo 'codex-cli 0.145.0'");
        let codex_home = tempfile::tempdir().unwrap();
        let probe = LocalProbe::new();

        let check = probe.check_with(
            &harness(HarnessKind::Codex),
            &test_dirs(dir.path()),
            codex_home.path(),
        );
        assert_eq!(check.status, HealthStatus::Ok);
        assert_eq!(check.version.as_deref(), Some("0.145.0"));
    }

    #[test]
    fn check_reports_missing_executable_override() {
        let dir = tempfile::tempdir().unwrap();
        let codex_home = tempfile::tempdir().unwrap();
        let mut instance = harness(HarnessKind::ClaudeCode);
        instance.executable = Some(dir.path().join("absent").display().to_string());
        let probe = LocalProbe::new();

        let check = probe.check_with(&instance, &[], codex_home.path());
        assert_eq!(check.status, HealthStatus::Error);
        assert_eq!(check.version, None);
        assert_eq!(check.checked_at, None);
        assert!(check.detail.unwrap().contains("不存在或不可执行"));
    }

    #[test]
    fn check_reports_failing_cli() {
        let dir = tempfile::tempdir().unwrap();
        let executable = fake_executable(dir.path(), "claude", "echo 'boom' >&2\nexit 3");
        let codex_home = tempfile::tempdir().unwrap();
        let mut instance = harness(HarnessKind::ClaudeCode);
        instance.executable = Some(executable.display().to_string());
        let probe = LocalProbe::new();

        let check = probe.check_with(&instance, &test_dirs(dir.path()), codex_home.path());
        assert_eq!(check.status, HealthStatus::Error);
        assert!(check.detail.unwrap().contains("boom"));
    }

    #[test]
    fn version_command_times_out_and_kills_process() {
        let dir = tempfile::tempdir().unwrap();
        let executable = fake_executable(dir.path(), "claude", "/bin/sleep 30");
        let probe = LocalProbe::new();

        let started = Instant::now();
        let err = probe
            .version_of(
                &executable,
                &test_dirs(dir.path()),
                Duration::from_millis(300),
            )
            .expect_err("超时应报错");
        assert!(err.contains("超时"), "实际错误：{err}");
        assert!(
            started.elapsed() < Duration::from_secs(5),
            "应尽快杀掉子进程"
        );
    }

    #[test]
    fn version_command_without_output_is_an_error() {
        let dir = tempfile::tempdir().unwrap();
        let executable = fake_executable(dir.path(), "claude", "exit 0");
        let probe = LocalProbe::new();

        let err = probe
            .version_of(&executable, &test_dirs(dir.path()), Duration::from_secs(5))
            .expect_err("无版本输出应报错");
        assert!(err.contains("未返回可解析的版本号"), "实际错误：{err}");
    }

    #[test]
    fn check_claude_profile_requires_existing_settings_file() {
        let dir = tempfile::tempdir().unwrap();
        let executable = fake_executable(dir.path(), "claude", "echo '2.1.296 (Claude Code)'");
        let settings = dir.path().join("settings.json");
        std::fs::write(&settings, "{}\n").unwrap();
        let codex_home = tempfile::tempdir().unwrap();
        let probe = LocalProbe::new();
        let mut instance = harness(HarnessKind::ClaudeCode);
        instance.executable = Some(executable.display().to_string());

        instance.profile = Some(settings.display().to_string());
        let ok = probe.check_with(&instance, &test_dirs(dir.path()), codex_home.path());
        assert_eq!(ok.status, HealthStatus::Ok);
        assert!(ok.detail.unwrap().contains("settings.json"));

        instance.profile = Some(dir.path().join("absent.json").display().to_string());
        let err = probe.check_with(&instance, &test_dirs(dir.path()), codex_home.path());
        assert_eq!(err.status, HealthStatus::Error);
        assert_eq!(err.version.as_deref(), Some("2.1.296"));
        assert!(err.detail.unwrap().contains("未找到 Claude Code 配置文件"));
    }

    #[test]
    fn check_codex_profile_reads_config_toml_and_standalone_file() {
        let dir = tempfile::tempdir().unwrap();
        let executable = fake_executable(dir.path(), "codex", "echo 'codex-cli 0.145.0'");
        let codex_home = tempfile::tempdir().unwrap();
        std::fs::write(
            codex_home.path().join("config.toml"),
            "[profiles.work]\nmodel = \"gpt-5\"\n",
        )
        .unwrap();
        let probe = LocalProbe::new();
        let mut instance = harness(HarnessKind::Codex);
        instance.executable = Some(executable.display().to_string());

        instance.profile = Some("work".to_string());
        let ok = probe.check_with(&instance, &test_dirs(dir.path()), codex_home.path());
        assert_eq!(ok.status, HealthStatus::Ok);
        assert!(ok.detail.unwrap().contains("config.toml"));

        instance.profile = Some("missing".to_string());
        let err = probe.check_with(&instance, &test_dirs(dir.path()), codex_home.path());
        assert_eq!(err.status, HealthStatus::Error);
        assert!(
            err.detail
                .unwrap()
                .contains("未找到 Codex profile：missing")
        );

        std::fs::write(
            codex_home.path().join("solo.config.toml"),
            "model = \"gpt-5\"\n",
        )
        .unwrap();
        instance.profile = Some("solo".to_string());
        let standalone = probe.check_with(&instance, &test_dirs(dir.path()), codex_home.path());
        assert_eq!(standalone.status, HealthStatus::Ok);
    }

    #[test]
    fn check_skips_profile_lookup_when_not_configured() {
        let dir = tempfile::tempdir().unwrap();
        let executable = fake_executable(dir.path(), "claude", "echo '2.1.296 (Claude Code)'");
        let codex_home = tempfile::tempdir().unwrap();
        let mut instance = harness(HarnessKind::ClaudeCode);
        instance.executable = Some(executable.display().to_string());
        instance.profile = Some("   ".to_string());
        let probe = LocalProbe::new();

        let check = probe.check_with(&instance, &test_dirs(dir.path()), codex_home.path());
        assert_eq!(check.status, HealthStatus::Ok);
    }

    #[test]
    fn check_does_not_run_anything_for_mock() {
        let probe = LocalProbe::new();
        let check = probe.check_with(&harness(HarnessKind::Mock), &[], Path::new("/nonexistent"));

        assert_eq!(check.status, HealthStatus::Unknown);
        assert_eq!(check.version, None);
        assert!(check.detail.unwrap().contains("mock"));
    }

    #[test]
    fn path_dirs_are_deduplicated_and_include_common_install_dirs() {
        let dirs = resolve_path_dirs();
        let mut unique = dirs.clone();
        unique.dedup();
        assert_eq!(unique.len(), dirs.len(), "PATH 不应包含重复目录");
        assert!(dirs.iter().any(|dir| dir.ends_with("/usr/bin")));

        let cargo_bin = dirs.iter().any(|dir| dir.ends_with(".cargo/bin"));
        let homebrew = dirs.iter().any(|dir| dir.ends_with("/opt/homebrew/bin"));
        if let Some(home) = home_dir() {
            assert_eq!(cargo_bin, dirs.contains(&home.join(".cargo/bin")));
            assert_eq!(homebrew, dirs.contains(&PathBuf::from("/opt/homebrew/bin")));
        }
    }

    #[test]
    fn probe_caches_path_between_calls() {
        let probe = LocalProbe::new();
        let first = probe.path_dirs().to_vec();
        let second = probe.path_dirs().to_vec();
        assert_eq!(first, second);
        assert!(!first.is_empty());
    }

    #[test]
    fn first_non_empty_line_trims_and_skips_blanks() {
        assert_eq!(first_non_empty_line("\n  x  \n y\n").as_deref(), Some("x"));
        assert_eq!(first_non_empty_line("  \n\n"), None);
    }
}
