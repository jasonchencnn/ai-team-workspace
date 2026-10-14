//! 本机系统探测实现（M1-04；M1.3 起 Harness 检查改为 ACP 握手）。
//!
//! [`LocalProbe`] 是 [`SystemProbe`] 的真实实现：工作空间是任意已存在的本地文件夹，探测时看它是不是
//! Git 仓库（是就附上分支与提交，不是也正常返回），文件搜索在 Git 目录里问 Git、在普通文件夹里遍历
//! 目录；Harness 检查是 ACP `initialize` 握手（见 [`crate::acp`]），不发送 prompt。全程不读取也不
//! 保存任何密钥，也不让 Harness 执行任务。
//!
//! macOS 上以 GUI 方式启动的应用继承不到登录 shell 的 PATH（Homebrew、NVM、Volta、Cargo
//! 装的 CLI 都会找不到），因此这里先用 `$SHELL -ilc` 打印一段带标记的 PATH，再补上常见安装
//! 目录，去重后缓存，所有子进程共用这一份 PATH。
//!
//! 依赖只用标准库：子进程超时用 `try_wait` 轮询实现，超时时按**进程组**整组杀掉，不引入新依赖。

use std::path::{Path, PathBuf};
use std::process::{Child, Command, ExitStatus, Stdio};
use std::sync::OnceLock;
use std::sync::mpsc::{self, Receiver};
use std::thread;
use std::time::{Duration, Instant};

use crate::acp::{self, AgentLaunch, HANDSHAKE_TIMEOUT, ScratchDir};
use crate::error::{Error, Result};
use crate::model::{GitInfo, Handshake, WorkspaceInfo};
use crate::probe::{MAX_SCANNED_FILES, SystemProbe};

/// 读取登录 shell PATH 的超时。
const SHELL_PATH_TIMEOUT: Duration = Duration::from_secs(5);
/// Git 查询的超时（正常是毫秒级，给慢盘留余量）。
const GIT_TIMEOUT: Duration = Duration::from_secs(30);
/// 等待子进程退出时的轮询间隔。
const POLL_INTERVAL: Duration = Duration::from_millis(20);
/// 杀掉子进程后等读线程收尾的宽限时间。
///
/// 正常情况下进程一死管道立刻 EOF，这里只是兜底：万一有个漏网进程一直攥着管道写端，界面也不能
/// 被它一直卡住，超时就当没读到输出。
const OUTPUT_GRACE: Duration = Duration::from_secs(2);
/// 进程组收不到信号时的重试次数。
const KILL_ATTEMPTS: usize = 3;

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

/// [`SystemProbe`] 的真实实现，探测本机的 Git 工作区、命令和 ACP Agent。
///
/// PATH 只解析一次并缓存在实例里（[`LocalProbe::new`] 之后不变），因此同一个实例的多次
/// 探测看到的是同一份环境。
#[derive(Debug)]
pub struct LocalProbe {
    path: OnceLock<Vec<PathBuf>>,
    handshake_timeout: Duration,
}

impl Default for LocalProbe {
    fn default() -> Self {
        Self {
            path: OnceLock::new(),
            handshake_timeout: HANDSHAKE_TIMEOUT,
        }
    }
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

    /// 用给定 PATH 和超时做握手（拆出来是为了能在测试里传临时目录和短超时）。
    fn handshake_with(
        &self,
        launch: &AgentLaunch,
        dirs: &[PathBuf],
        timeout: Duration,
    ) -> Handshake {
        let command = launch.command.trim();
        let Some(program) = find_in_path(command, dirs) else {
            return Handshake::error(if command.contains('/') {
                format!("启动命令不存在或不可执行：{command}")
            } else {
                format!("未找到命令 {command}：请确认已安装并加入 PATH")
            });
        };
        if let Some(profile) = launch.env_profile.as_deref()
            && !Path::new(profile).is_file()
        {
            return Handshake::error(format!("环境配置档案不存在：{profile}"));
        }
        let scratch = match ScratchDir::new() {
            Ok(dir) => dir,
            Err(err) => return Handshake::error(format!("无法创建握手用的临时目录：{err}")),
        };
        acp::handshake_process(
            &program,
            &launch.args,
            scratch.path(),
            &path_env(dirs),
            timeout,
        )
    }

    /// 执行一条 Git 查询。`Ok(Some(text))` 为成功，`Ok(None)` 为 Git 正常拒绝（退出码非 0），
    /// `Err` 为 Git 本身跑不起来。
    fn git_text(&self, path: &Path, args: &[&str]) -> Result<Option<String>> {
        let mut command = Command::new("git");
        command.arg("-C").arg(path);
        command.args(args);
        command.env("PATH", path_env(self.path_dirs()));
        git_env(&mut command);

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

    /// `git ls-files` 列出的文件：相对 `workspace`、`/` 分隔，含已跟踪与未跟踪但未忽略的文件。
    ///
    /// `Ok(None)` 表示这个目录拿不到 Git 清单（不是仓库，或 Git 跑不起来），调用方改用目录遍历；
    /// 只有 Git 卡住不动才算错误——那说明确实出了别的问题，不该悄悄换成遍历继续搜。
    fn git_list_files(&self, workspace: &Path) -> Result<Option<Vec<String>>> {
        let mut command = Command::new("git");
        command.arg("-C").arg(workspace);
        command.args([
            "ls-files",
            "--cached",
            "--others",
            "--exclude-standard",
            "-z",
        ]);
        command.env("PATH", path_env(self.path_dirs()));
        git_env(&mut command);

        let output = match run_command(&mut command, GIT_TIMEOUT) {
            Ok(output) => output,
            // 没装 Git 或没执行权限不算搜索失败：这个目录本来就能当普通文件夹搜。
            Err(_) => return Ok(None),
        };
        if output.timed_out {
            return Err(Error::internal("搜索文件超时，请稍后重试"));
        }
        if !output.success {
            return Ok(None);
        }
        Ok(Some(split_nul_paths(&output.stdout)))
    }
}

/// 收集普通文件夹里的文件：相对 `root`、`/` 分隔，最多 `max_files` 个。
///
/// 跳过以 `.` 开头的文件和目录，以及 [`SKIPPED_DIRS`] 里的常见大目录；不跟随符号链接（指向目录的
/// 链接不会被走进去，避免顺着链接爬出工作空间或绕成环）；单个子目录读不动（权限不足、刚好被删）
/// 就跳过。一次搜索不该把整块硬盘读一遍，所以扫够 [`MAX_SCANNED_FILES`] 个文件就停。
fn collect_files(root: &Path, max_files: usize) -> Vec<String> {
    let mut files = Vec::new();
    collect_files_into(root, root, max_files, &mut files);
    files
}

/// 遍历普通文件夹时整棵跳过的目录（任何层级都跳）：装依赖和构建产物，文件多又不值得搜索。
const SKIPPED_DIRS: [&str; 4] = ["node_modules", "target", "dist", "build"];

/// 递归收集 `dir` 下的文件，结果相对 `root`（`root` 本身不必是 `dir`）。
fn collect_files_into(root: &Path, dir: &Path, max_files: usize, files: &mut Vec<String>) {
    if files.len() >= max_files {
        return;
    }
    // 读不动就当这个子目录里没有文件，不因为一个子目录失败让整次搜索失败。
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };

    for entry in entries.flatten() {
        if files.len() >= max_files {
            return;
        }
        let name = entry.file_name();
        if name.to_string_lossy().starts_with('.') {
            continue;
        }
        // file_type 取自 readdir，不跟随符号链接：链接到目录不会在这里被当成目录递归下去。
        let Ok(kind) = entry.file_type() else {
            continue;
        };
        if kind.is_dir() {
            if SKIPPED_DIRS.contains(&name.to_string_lossy().as_ref()) {
                continue;
            }
            collect_files_into(root, &entry.path(), max_files, files);
        } else if kind.is_symlink() {
            // 符号链接只当文件看待（与 git ls-files 一致）：指向文件的列出来，指向目录的直接跳过。
            if entry.path().metadata().is_ok_and(|meta| meta.is_file()) {
                files.push(relative_path(root, &entry.path()));
            }
        } else {
            files.push(relative_path(root, &entry.path()));
        }
    }
}

/// 相对 `root` 的路径字符串，分隔符统一成 `/`（Windows 上也是），方便直接展示和作为任务引用。
fn relative_path(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .components()
        .map(|component| component.as_os_str().to_string_lossy())
        .collect::<Vec<_>>()
        .join("/")
}

/// 给 Git 子进程统一设置的环境变量。
///
/// `GIT_TERMINAL_PROMPT=0` 不做交互式凭据提问，避免远程仓库查询把界面卡住；`GIT_CONFIG_NOSYSTEM=1`
/// 忽略系统级 `/etc/gitconfig`，免得机器上装的代理、模板仓库等全局设置影响只读查询的结果。
pub(crate) fn git_env(command: &mut Command) -> &mut Command {
    command
        .stdin(Stdio::null())
        .env("GIT_TERMINAL_PROMPT", "0")
        .env("GIT_CONFIG_NOSYSTEM", "1")
}

/// 探测前确认工作区目录还在且确实是目录。
///
/// 工作空间可以是任意本地文件夹，不要求是 Git 仓库，所以这里只区分「目录没了」和「这不是目录」
/// （通常是选错了文件）；是不是 Git 仓库由 [`LocalProbe::git_text`] 再判断。
fn ensure_workspace_dir(path: &Path) -> Result<()> {
    if !path.exists() {
        return Err(Error::invalid(format!(
            "工作区目录不存在或已移动：{}",
            path.display()
        )));
    }
    if !path.is_dir() {
        return Err(Error::invalid(format!(
            "所选路径不是目录：{}",
            path.display()
        )));
    }
    Ok(())
}

impl SystemProbe for LocalProbe {
    fn inspect_workspace(&self, path: &Path) -> Result<WorkspaceInfo> {
        ensure_workspace_dir(path)?;

        // 工作空间就是所选目录本身：选中仓库子目录时也不提升为仓库根（内核同样会用规范化后的
        // 所选目录覆盖这里的 path），所以直接原样返回传入的目录。
        let selected = path.display().to_string();
        // Git 在非仓库目录里正常拒绝（退出码非 0），即「普通文件夹」，git 为 None 而不是错误；
        // 只有 Git 自己跑不起来才算错误。
        if self
            .git_text(path, &["rev-parse", "--show-toplevel"])?
            .is_none()
        {
            return Ok(WorkspaceInfo {
                path: selected,
                git: None,
            });
        }

        // detached HEAD 时 symbolic-ref 退出码非 0，即分支为 None。
        let branch = self.git_text(path, &["symbolic-ref", "--short", "-q", "HEAD"])?;
        // 还没有任何提交时 rev-parse 失败，即提交为 None。
        let commit = self.git_text(path, &["rev-parse", "--short=7", "HEAD"])?;

        Ok(WorkspaceInfo {
            path: selected,
            git: Some(GitInfo { branch, commit }),
        })
    }

    fn search_files(&self, workspace: &Path, query: &str, limit: usize) -> Result<Vec<String>> {
        ensure_workspace_dir(workspace)?;
        if limit == 0 {
            return Ok(Vec::new());
        }

        // Git 目录（含仓库子目录）走 git ls-files：路径相对所选目录、已跟踪与未跟踪但未忽略的
        // 文件都在里面；不是仓库（或 Git 用不了）就退回遍历目录，普通文件夹照样能搜。
        let paths = match self.git_list_files(workspace)? {
            Some(paths) => paths,
            None => collect_files(workspace, MAX_SCANNED_FILES),
        };
        Ok(select_files(paths, query, limit))
    }

    fn locate_command(&self, command: &str) -> Option<String> {
        find_in_path(command.trim(), self.path_dirs()).map(|path| path.display().to_string())
    }

    fn handshake(&self, launch: &AgentLaunch) -> Handshake {
        self.handshake_with(launch, self.path_dirs(), self.handshake_timeout)
    }

    fn command_path_env(&self) -> std::ffi::OsString {
        path_env(self.path_dirs())
    }

    /// 用系统自带命令打开（macOS `open`、Windows `explorer`、其他平台 `xdg-open`），不加插件。
    /// 文件：macOS 用 `open -R` 在 Finder 中选中，Windows 用 `explorer /select,`，其他平台打开所在目录。
    fn reveal_path(&self, path: &Path) -> Result<()> {
        if !path.exists() {
            return Err(Error::not_found(format!("路径不存在：{}", path.display())));
        }
        let is_file = !path.is_dir();
        let opener = if cfg!(target_os = "macos") {
            "/usr/bin/open"
        } else if cfg!(windows) {
            "explorer"
        } else {
            "xdg-open"
        };
        let mut command = Command::new(opener);
        match (is_file, cfg!(target_os = "macos"), cfg!(windows)) {
            (true, true, _) => {
                command.arg("-R").arg(path);
            }
            (true, _, true) => {
                command.arg(format!("/select,{}", path.display()));
            }
            (true, _, _) => {
                command.arg(path.parent().unwrap_or(path));
            }
            (false, _, _) => {
                command.arg(path);
            }
        }
        let status = command
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .map_err(|err| Error::internal(format!("无法打开 {}：{err}", path.display())))?;
        // Windows 的 explorer 即使成功也常返回非 0，只在其他平台检查退出码。
        if !status.success() && !cfg!(windows) {
            return Err(Error::internal(format!(
                "无法打开 {}（{opener} 退出码 {status}）",
                path.display()
            )));
        }
        Ok(())
    }
}

/// 一条子进程的执行结果。
struct CommandOutput {
    success: bool,
    stdout: String,
    timed_out: bool,
}

/// 等待子进程的结果：要么自己退出了，要么超时被杀掉。
enum WaitOutcome {
    Exited(ExitStatus),
    Killed,
}

/// 运行子进程并在超时后杀掉它。标准库没有跨平台超时，用 `try_wait` 轮询实现。
///
/// 子进程启动时就被放进**独立进程组**（见 [`isolate_in_process_group`]），超时时对整个进程组发
/// 信号：Harness CLI 几乎都会再 fork 自己的孙进程，只杀直接子进程的话孙进程会继续持有
/// stdout / stderr 的管道写端，下面两个读线程就永远等不到 EOF。
///
/// stdout / stderr 各自在一个线程里读，避免管道写满后互相死锁。
fn run_command(command: &mut Command, timeout: Duration) -> std::io::Result<CommandOutput> {
    isolate_in_process_group(command);
    let mut child = command
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;

    // 读线程把结果发回通道而不是直接 join：万一有进程漏网一直占着管道，最多等 OUTPUT_GRACE，
    // 到点就放弃（线程随之detach），绝不能把界面卡在一次探测上。
    let out_rx = child.stdout.take().map(read_in_background);
    // stderr 也要读走（避免写满管道卡住子进程），内容目前不用。
    let err_rx = child.stderr.take().map(read_in_background);

    let deadline = Instant::now() + timeout;
    let mut timed_out = false;
    let outcome = loop {
        match child.try_wait() {
            Ok(Some(status)) => break WaitOutcome::Exited(status),
            Ok(None) => {
                if Instant::now() >= deadline {
                    timed_out = true;
                    kill_process_group(&mut child);
                    break WaitOutcome::Killed;
                }
                thread::sleep(POLL_INTERVAL);
            }
            Err(err) => {
                // 问不出状态就不能放着不管：同样杀掉整组并回收，不留孤儿和僵尸。
                kill_process_group(&mut child);
                let _ = child.wait();
                return Err(err);
            }
        }
    };

    // 杀掉之后 wait 只是回收子进程（拿退出码），整组已死，管道随即关闭，读线程可以正常结束。
    let status = match outcome {
        WaitOutcome::Exited(status) => status,
        WaitOutcome::Killed => child.wait()?,
    };

    // 通道里没东西（读线程被放弃，或本来就没管道）就当没输出，不算错误。
    let collect = |receiver: Option<Receiver<String>>| {
        receiver
            .and_then(|receiver| receiver.recv_timeout(OUTPUT_GRACE).ok())
            .unwrap_or_default()
    };
    let _ = collect(err_rx);
    Ok(CommandOutput {
        success: status.success(),
        stdout: collect(out_rx),
        timed_out,
    })
}

/// 是否是「Claude Code 会话」环境变量：`CLAUDECODE` 与 `CLAUDE_CODE_*`。
///
/// Teamspace 若是从 Claude Code 的终端启动的，会继承这些变量；`claude-code-acp` 看到它们会报
/// 「cannot be launched inside another Claude Code session」而无法启动，所以启动 Agent 时要去掉。
pub(crate) fn is_claude_session_var(name: &str) -> bool {
    name == "CLAUDECODE" || name.starts_with("CLAUDE_CODE_")
}

/// 从将要启动的 Agent 子进程的环境中去掉继承来的 Claude Code 会话变量（见 [`is_claude_session_var`]）。
///
/// `inherited` 是当前进程环境的变量名；`explicit` 是用户在配置档案里显式给出的变量，它们照常传给子进程
/// （因此要在设置完 `explicit` 之后调用，或在其之前调用都可以：这里不会动 `explicit` 里的名字）。
pub(crate) fn scrub_claude_session_env(
    command: &mut Command,
    inherited: impl IntoIterator<Item = std::ffi::OsString>,
    explicit: &[(std::ffi::OsString, std::ffi::OsString)],
) {
    for name in inherited {
        let is_session = name.to_str().is_some_and(is_claude_session_var);
        if is_session && !explicit.iter().any(|(key, _)| *key == name) {
            command.env_remove(&name);
        }
    }
}

/// 同 [`scrub_claude_session_env`]，`inherited` 取当前进程的真实环境。
pub(crate) fn scrub_inherited_claude_env(
    command: &mut Command,
    explicit: &[(std::ffi::OsString, std::ffi::OsString)],
) {
    scrub_claude_session_env(command, std::env::vars_os().map(|(key, _)| key), explicit);
}

/// 让子进程成为新进程组的组长，这样超时可以整组一起杀。
///
/// 子进程的后代默认都继承这个进程组，所以一次 `kill` 能收干净整棵进程树；`spawn` 失败时调用方
/// 直接报错，不会留下半启动的进程。非 Unix 平台没有进程组概念，退化为只杀直接子进程。
#[cfg(unix)]
pub(crate) fn isolate_in_process_group(command: &mut Command) {
    use std::os::unix::process::CommandExt;
    // 0 表示「用子进程自己的 pid 作为进程组 id」，即 setpgid(0, 0)。
    command.process_group(0);
}

#[cfg(not(unix))]
pub(crate) fn isolate_in_process_group(_command: &mut Command) {}

/// 杀掉子进程所在的整个进程组，再兜底杀一次子进程本身，最后由调用方 `wait` 回收。
#[cfg(unix)]
pub(crate) fn kill_process_group(child: &mut Child) {
    kill_group(child.id());
    let _ = child.kill();
}

#[cfg(not(unix))]
pub(crate) fn kill_process_group(child: &mut Child) {
    let _ = child.kill();
}

/// 对进程组 `pid` 发 SIGKILL。进程组 id 就是组长的 pid，负号表示「整个进程组」。
///
/// 只用 `kill` 子进程的话，它 fork 出来的孙进程会活下来并继续占着管道。走 `/bin/kill` 不用手写
/// `unsafe` 的 `libc::kill`，也不引入新依赖；`--` 结束选项解析，避免 `-123` 被当成选项。
#[cfg(unix)]
fn kill_group(pid: u32) {
    let target = format!("-{pid}");
    // `/bin/kill` 自己也可能因为机器繁忙没起成，那样信号就落空了，孙进程会一直攥着管道。
    // 没送达就再试几次；正常情况下第一条就返回，不额外花时间。
    for attempt in 1..=KILL_ATTEMPTS {
        if send_sigkill(&target) {
            return;
        }
        if attempt < KILL_ATTEMPTS {
            thread::sleep(POLL_INTERVAL);
        }
    }
}

/// 用 `/bin/kill -KILL -- <target>` 给进程组发信号，信号送达返回 `true`。
#[cfg(unix)]
fn send_sigkill(target: &str) -> bool {
    Command::new("/bin/kill")
        .args(["-KILL", "--", target])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|status| status.success())
}

/// 起一个后台线程读完一条管道，把结果发回通道。
///
/// 通道的所有权只在读线程和调用方之间传递：线程读完就发结果、发完就关闭发送端，调用方最多等
/// [`OUTPUT_GRACE`]。读线程被放弃时只是 detach，不会留下僵尸线程。
fn read_in_background<R: std::io::Read + Send + 'static>(stream: R) -> Receiver<String> {
    let (sender, receiver) = mpsc::channel();
    thread::spawn(move || {
        let _ = sender.send(read_stream(stream));
    });
    receiver
}

/// 读完一个管道并按 UTF-8 宽松解码（文件路径理论上都是 UTF-8，但不因为个别坏字节丢掉整条命令的输出）。
fn read_stream<R: std::io::Read>(mut stream: R) -> String {
    let mut buffer = Vec::new();
    let _ = stream.read_to_end(&mut buffer);
    String::from_utf8_lossy(&buffer).into_owned()
}

/// 按规则筛选排序候选文件：命中判断不区分大小写，排序为「文件名命中 → 路径更短 → 字典序」，
/// 最多 `limit` 条。Git 清单与目录遍历共用这一套，所以两种工作空间的搜索结果一致。
fn select_files(paths: Vec<String>, query: &str, limit: usize) -> Vec<String> {
    if limit == 0 {
        return Vec::new();
    }
    let needle = query.trim().to_lowercase();

    let mut files: Vec<String> = paths
        .into_iter()
        .filter(|path| !path.is_empty())
        .filter(|path| needle.is_empty() || path.to_lowercase().contains(&needle))
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

/// 拆开 `git ls-files -z` 的 NUL 分隔输出（路径里可以含换行，所以不能按行读）。
fn split_nul_paths(stdout: &str) -> Vec<String> {
    stdout
        .split('\0')
        .filter(|path| !path.is_empty())
        .map(str::to_string)
        .collect()
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
pub(crate) fn find_in_path(command: &str, dirs: &[PathBuf]) -> Option<PathBuf> {
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

/// 进程当前的用户主目录。
fn home_dir() -> Option<PathBuf> {
    std::env::var_os("HOME")
        .filter(|home| !home.is_empty())
        .or_else(|| std::env::var_os("USERPROFILE").filter(|home| !home.is_empty()))
        .map(PathBuf::from)
}

/// 把目录列表拼成给子进程用的 `PATH`。
pub(crate) fn path_env(dirs: &[PathBuf]) -> std::ffi::OsString {
    std::env::join_paths(dirs).unwrap_or_default()
}

/// 第一个非空行（去掉首尾空白），用于「查询无输出」的判断。
fn first_non_empty_line(text: &str) -> Option<String> {
    text.lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .map(str::to_string)
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

    /// 一个确定不存在的路径：先建一个唯一的临时目录再删掉，保证它确实不在了。
    fn gone_workspace() -> PathBuf {
        let dir = tempfile::tempdir().unwrap();
        let gone = dir.path().join("not-here");
        drop(dir);
        assert!(!gone.exists());
        gone
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
    fn inspect_reports_branch_and_commit_in_repository() {
        let dir = temp_repo(true);
        let probe = LocalProbe::new();

        let info = probe
            .inspect_workspace(dir.path())
            .expect("应识别为 Git 仓库");
        assert_eq!(info.path, dir.path().display().to_string());
        let git = info.git.expect("应有 Git 信息");
        assert_eq!(git.branch.as_deref(), Some("probe-branch"));
        let commit = git.commit.expect("有提交时应返回短哈希");
        assert_eq!(commit.len(), 7);
        assert!(commit.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn inspect_keeps_subdirectory_and_still_reports_git() {
        let dir = temp_repo(true);
        let nested = dir.path().join("crates");
        std::fs::create_dir_all(&nested).unwrap();
        let probe = LocalProbe::new();

        let info = probe
            .inspect_workspace(&nested)
            .expect("仓库子目录也是合法工作空间");
        // 工作空间就是所选目录本身，不提升为仓库根。
        assert_eq!(info.path, nested.display().to_string());
        assert_ne!(info.path, canonical(dir.path()));
        let git = info.git.expect("仓库子目录仍应有 Git 信息");
        assert_eq!(git.branch.as_deref(), Some("probe-branch"));
        assert!(git.commit.is_some());
    }

    #[test]
    fn inspect_reports_none_commit_for_empty_repository() {
        let dir = temp_repo(false);
        let probe = LocalProbe::new();

        let info = probe.inspect_workspace(dir.path()).expect("空仓库也是仓库");
        assert_eq!(info.path, dir.path().display().to_string());
        let git = info.git.expect("应有 Git 信息");
        assert_eq!(git.commit, None);
        assert_eq!(git.branch.as_deref(), Some("probe-branch"));
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
        let git = info.git.expect("应有 Git 信息");
        assert_eq!(git.branch, None);
        assert!(git.commit.is_some());
    }

    #[test]
    fn inspect_rejects_missing_directory() {
        let probe = LocalProbe::new();
        let err = probe
            .inspect_workspace(&gone_workspace())
            .expect_err("目录不存在应报错");
        assert!(matches!(err, Error::Invalid(_)));
        assert!(
            err.to_string().contains("不存在或已移动"),
            "实际错误：{err}"
        );
    }

    #[test]
    fn inspect_accepts_plain_folder_without_git() {
        let dir = tempfile::tempdir().expect("创建临时目录");
        std::fs::write(dir.path().join("notes.md"), "x").unwrap();
        std::fs::create_dir_all(dir.path().join("src/deep")).unwrap();
        std::fs::write(dir.path().join("src/deep/main.rs"), "fn main() {}").unwrap();
        let probe = LocalProbe::new();

        let info = probe
            .inspect_workspace(dir.path())
            .expect("普通文件夹也是合法工作空间");
        assert_eq!(info.path, dir.path().display().to_string());
        assert_eq!(info.git, None, "不是 Git 仓库时 git 为 None，不是错误");
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
    fn search_in_git_subdirectory_returns_only_files_inside() {
        let dir = temp_repo(true);
        let nested = dir.path().join("crates");
        std::fs::create_dir_all(&nested).unwrap();
        std::fs::write(dir.path().join("root.md"), "x").unwrap();
        std::fs::write(nested.join("inside.md"), "x").unwrap();
        git(dir.path(), &["add", "-A"]);
        let probe = LocalProbe::new();

        let files = probe.search_files(&nested, "", 50).expect("搜索应成功");
        // 只列所选目录内的文件，且路径相对所选目录。
        assert_eq!(files, vec!["inside.md".to_string()]);
    }

    #[test]
    fn search_in_plain_folder_returns_files_relative_to_workspace() {
        let dir = tempfile::tempdir().expect("创建临时目录");
        std::fs::create_dir_all(dir.path().join("src/deep/nested")).unwrap();
        std::fs::create_dir_all(dir.path().join("docs")).unwrap();
        std::fs::write(dir.path().join("README.md"), "x").unwrap();
        std::fs::write(dir.path().join("src/main.rs"), "x").unwrap();
        std::fs::write(dir.path().join("src/deep/nested/target.rs"), "x").unwrap();
        std::fs::write(dir.path().join("docs/guide.md"), "x").unwrap();
        let probe = LocalProbe::new();

        let files = probe
            .search_files(dir.path(), "target", 50)
            .expect("搜索应成功");
        assert_eq!(files, vec!["src/deep/nested/target.rs".to_string()]);

        // 与 Git 目录同一套排序：路径短者优先。
        let all = probe.search_files(dir.path(), "", 50).expect("搜索应成功");
        assert_eq!(
            all,
            vec![
                "README.md".to_string(),
                "src/main.rs".to_string(),
                "docs/guide.md".to_string(),
                "src/deep/nested/target.rs".to_string(),
            ]
        );
    }

    #[test]
    fn search_in_plain_folder_is_case_insensitive_and_respects_limit() {
        let dir = tempfile::tempdir().expect("创建临时目录");
        std::fs::create_dir_all(dir.path().join("src")).unwrap();
        std::fs::write(dir.path().join("src/Probe.TS"), "x").unwrap();
        std::fs::write(dir.path().join("probe.md"), "x").unwrap();
        std::fs::write(dir.path().join("other.md"), "x").unwrap();
        let probe = LocalProbe::new();

        let files = probe
            .search_files(dir.path(), "PROBE", 50)
            .expect("搜索应成功");
        // 与 Git 目录同一套规则：文件名命中优先，其次路径短者优先。
        assert_eq!(
            files,
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
    fn search_in_plain_folder_skips_hidden_and_large_directories() {
        let dir = tempfile::tempdir().expect("创建临时目录");
        for relative in [
            ".hidden/secret.md",
            ".env",
            "node_modules/pkg/index.js",
            "target/debug/out.bin",
            "dist/bundle.js",
            "build/out.bin",
            "keep.md",
        ] {
            let path = dir.path().join(relative);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, "x").unwrap();
        }
        let probe = LocalProbe::new();

        let files = probe.search_files(dir.path(), "", 50).expect("搜索应成功");
        assert_eq!(files, vec!["keep.md".to_string()]);
    }

    #[test]
    fn search_in_plain_folder_does_not_follow_symlinks() {
        use std::os::unix::fs::symlink;

        let dir = tempfile::tempdir().expect("创建临时目录");
        let outside = tempfile::tempdir().expect("创建目录外的目录");
        std::fs::write(outside.path().join("secret.md"), "x").unwrap();
        std::fs::write(dir.path().join("real.md"), "x").unwrap();
        // 指向目录的链接不该被走进去（否则搜索会爬出工作空间，甚至绕成环）。
        symlink(outside.path(), dir.path().join("linked-dir")).unwrap();
        symlink(
            outside.path().join("secret.md"),
            dir.path().join("linked.md"),
        )
        .unwrap();
        let probe = LocalProbe::new();

        let files = probe.search_files(dir.path(), "", 50).expect("搜索应成功");
        // 目录链接下的 secret.md 没有出现；指向文件的链接当文件看待（与 git ls-files 一致）。
        assert_eq!(files, vec!["real.md".to_string(), "linked.md".to_string()]);
    }

    #[test]
    fn plain_folder_scan_stops_at_file_limit() {
        let dir = tempfile::tempdir().expect("创建临时目录");
        for index in 0..8 {
            std::fs::write(dir.path().join(format!("f{index}.md")), "x").unwrap();
        }

        // search_files 用 [`MAX_SCANNED_FILES`] 当上限，这里用小数字验证「扫够就停」这件事本身。
        assert_eq!(collect_files(dir.path(), 3).len(), 3);
        assert_eq!(collect_files(dir.path(), 8).len(), 8);
        assert_eq!(collect_files(dir.path(), MAX_SCANNED_FILES).len(), 8);
        assert!(collect_files(dir.path(), 0).is_empty());
    }

    #[test]
    fn search_rejects_missing_directory_with_its_own_message() {
        let probe = LocalProbe::new();

        let err = probe
            .search_files(&gone_workspace(), "", 10)
            .expect_err("目录不存在应报错");
        assert!(matches!(err, Error::Invalid(_)));
        let message = err.to_string();
        assert!(message.contains("不存在或已移动"), "实际错误：{message}");
        assert!(
            !message.contains("不是 Git 仓库"),
            "目录不存在不该被说成不是仓库：{message}"
        );
    }

    #[test]
    fn search_rejects_file_path() {
        let dir = tempfile::tempdir().expect("创建临时目录");
        let file = dir.path().join("notes.txt");
        std::fs::write(&file, "x").unwrap();
        let probe = LocalProbe::new();

        let err = probe
            .search_files(&file, "", 10)
            .expect_err("文件路径应报错");
        assert!(matches!(err, Error::Invalid(_)));
        assert!(err.to_string().contains("不是目录"));
    }

    #[test]
    fn plain_folder_is_usable_while_missing_directory_is_an_error() {
        let probe = LocalProbe::new();
        let plain = tempfile::tempdir().unwrap();

        // 普通文件夹：探测与搜索都正常，探测不带 Git 信息。
        assert!(probe.inspect_workspace(plain.path()).unwrap().git.is_none());
        assert!(probe.search_files(plain.path(), "", 10).unwrap().is_empty());

        // 目录不存在才是错误，且说的是「不存在或已移动」。
        let err = probe
            .inspect_workspace(&gone_workspace())
            .expect_err("目录不存在应报错");
        assert!(matches!(err, Error::Invalid(_)));
        assert!(
            err.to_string().contains("不存在或已移动"),
            "实际错误：{err}"
        );
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
    fn run_command_kills_whole_process_group() {
        let dir = tempfile::tempdir().unwrap();
        // 孙进程把 pid 打到继承来的 stdout 上：管道被 run_command 收走，不用等文件落盘去抢时序。
        let executable = fake_executable(
            dir.path(),
            "claude",
            "/bin/sh -c 'echo $$; exec /bin/sleep 30' &\n/bin/sleep 30",
        );
        let mut command = Command::new(&executable);
        command.stdin(Stdio::null());

        let started = Instant::now();
        // 超时给足 2 秒，保证孙进程一定来得及启动并打出 pid；只杀直接子进程的话这里会拖满 30 秒。
        let output = run_command(&mut command, Duration::from_secs(2)).expect("运行应返回");
        let elapsed = started.elapsed();

        assert!(output.timed_out, "应判定为超时");
        assert!(
            elapsed < Duration::from_secs(20),
            "孙进程还占着管道时应立刻返回，实际耗时 {elapsed:?}"
        );

        let grandchild: i32 = first_non_empty_line(&output.stdout)
            .expect("孙进程应已把 pid 打到 stdout")
            .parse()
            .expect("pid 应是数字");
        // 决定性断言：孙进程必须真的没了。回收要被 init 收养，机器繁忙时慢，给足时间。
        assert!(
            wait_until_gone(grandchild, Duration::from_secs(15)),
            "孙进程 {grandchild} 还活着，说明只杀了直接子进程"
        );
    }

    /// 轮询等待 pid 不再存在（被杀掉并回收后 `/bin/kill -0` 会失败）。
    fn wait_until_gone(pid: i32, budget: Duration) -> bool {
        let deadline = Instant::now() + budget;
        loop {
            let alive = Command::new("/bin/kill")
                .args(["-0", &pid.to_string()])
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status()
                .is_ok_and(|status| status.success());
            if !alive {
                return true;
            }
            if Instant::now() >= deadline {
                return false;
            }
            // 这里每次轮询都要起一个 /bin/kill，间隔放大一点，别跟测试本身抢机器。
            thread::sleep(Duration::from_millis(200));
        }
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
    fn locate_command_searches_given_path_or_checks_absolute_path() {
        let dir = tempfile::tempdir().unwrap();
        let exe = fake_executable(dir.path(), "claude-code-acp", "exit 0");
        std::fs::write(dir.path().join("not-exec"), "x").unwrap();
        let dirs = test_dirs(dir.path());
        assert_eq!(find_in_path("claude-code-acp", &dirs), Some(exe.clone()));
        assert_eq!(find_in_path("not-exec", &dirs), None);
        assert_eq!(find_in_path("codex-acp", &dirs), None);
        assert_eq!(find_in_path(exe.to_str().unwrap(), &[]), Some(exe));
    }

    fn launch(command: &str, args: &[&str]) -> AgentLaunch {
        AgentLaunch {
            command: command.into(),
            args: args.iter().map(|a| a.to_string()).collect(),
            env_profile: None,
        }
    }

    #[test]
    fn handshake_finds_agent_in_path_and_passes_args() {
        let dir = tempfile::tempdir().unwrap();
        fake_executable(
            dir.path(),
            "mcode",
            r#"[ "$1" = "acp" ] || { echo "missing acp arg" >&2; exit 2; }
read line
echo '{"jsonrpc":"2.0","id":1,"result":{"protocolVersion":1,"agentInfo":{"name":"mcode","version":"3.1.0"}}}'"#,
        );
        let probe = LocalProbe::new();
        let hs = probe.handshake_with(
            &launch("mcode", &["acp"]),
            &test_dirs(dir.path()),
            Duration::from_secs(10),
        );
        assert_eq!(
            hs.status,
            crate::model::HandshakeStatus::Connected,
            "{:?}",
            hs.detail
        );
        assert_eq!(hs.agent.unwrap().version.as_deref(), Some("3.1.0"));
    }

    #[test]
    fn handshake_reports_missing_command_and_env_profile() {
        let dir = tempfile::tempdir().unwrap();
        let probe = LocalProbe::new();
        let dirs = test_dirs(dir.path());

        let hs = probe.handshake_with(&launch("gemini", &[]), &dirs, Duration::from_secs(1));
        assert_eq!(hs.status, crate::model::HandshakeStatus::Error);
        assert_eq!(
            hs.detail.as_deref(),
            Some("未找到命令 gemini：请确认已安装并加入 PATH")
        );

        let absent = dir.path().join("absent-agent").display().to_string();
        let hs = probe.handshake_with(&launch(&absent, &[]), &dirs, Duration::from_secs(1));
        assert!(hs.detail.unwrap().contains("不存在或不可执行"));

        // 环境配置档案只检查存在，不读取内容；不存在时不启动进程。
        let marker = dir.path().join("started");
        fake_executable(
            dir.path(),
            "agent",
            &format!("/usr/bin/touch '{}'", marker.display()),
        );
        let mut with_profile = launch("agent", &[]);
        with_profile.env_profile = Some(dir.path().join("missing.env").display().to_string());
        let hs = probe.handshake_with(&with_profile, &dirs, Duration::from_secs(1));
        assert!(hs.detail.unwrap().contains("环境配置档案不存在"));
        assert!(!marker.exists(), "档案缺失时不应启动 Agent");
    }

    #[test]
    fn first_non_empty_line_trims_and_skips_blanks() {
        assert_eq!(first_non_empty_line("\n  x  \n y\n").as_deref(), Some("x"));
        assert_eq!(first_non_empty_line("  \n\n"), None);
    }
    #[test]
    fn claude_session_vars_are_recognised() {
        for name in [
            "CLAUDECODE",
            "CLAUDE_CODE_ENTRYPOINT",
            "CLAUDE_CODE_SSE_PORT",
        ] {
            assert!(is_claude_session_var(name), "{name}");
        }
        for name in [
            "CLAUDE",
            "CLAUDECODE2",
            "CLAUDE_CODE",
            "MY_CLAUDE_CODE_X",
            "PATH",
        ] {
            assert!(!is_claude_session_var(name), "{name}");
        }
    }

    /// 启动 Agent 时去掉继承来的 `CLAUDECODE` / `CLAUDE_CODE_*`，配置档案里显式给出的保留。
    #[test]
    fn scrub_removes_inherited_claude_session_vars_but_keeps_explicit_ones() {
        let os = std::ffi::OsString::from;
        let mut command = Command::new("/usr/bin/env");
        command
            .env_clear()
            .env("CLAUDECODE", "1")
            .env("CLAUDE_CODE_ENTRYPOINT", "cli")
            .env("CLAUDE_CODE_USE_BEDROCK", "profile-value")
            .env("OTHER_VAR", "keep");
        let explicit = vec![(os("CLAUDE_CODE_USE_BEDROCK"), os("profile-value"))];
        scrub_claude_session_env(
            &mut command,
            [
                os("CLAUDECODE"),
                os("CLAUDE_CODE_ENTRYPOINT"),
                os("CLAUDE_CODE_USE_BEDROCK"),
                os("OTHER_VAR"),
            ],
            &explicit,
        );
        let out = command.output().unwrap();
        let text = String::from_utf8_lossy(&out.stdout).into_owned();
        let lines: Vec<&str> = text.lines().collect();
        assert!(
            !lines.iter().any(|l| l.starts_with("CLAUDECODE=")),
            "{text}"
        );
        assert!(
            !lines
                .iter()
                .any(|l| l.starts_with("CLAUDE_CODE_ENTRYPOINT=")),
            "{text}"
        );
        assert!(
            lines.contains(&"CLAUDE_CODE_USE_BEDROCK=profile-value"),
            "{text}"
        );
        assert!(lines.contains(&"OTHER_VAR=keep"), "{text}");
    }

    /// 真实启动路径：即使当前进程（例如在 Claude Code 终端里运行测试）带着这些变量，
    /// Agent 子进程里也看不到；显式传入的同名变量照常生效。
    #[test]
    fn agent_process_does_not_inherit_claude_session_vars() {
        let dir = tempfile::tempdir().unwrap();
        let spec = crate::acp::connection::SpawnSpec {
            program: "/bin/sh".into(),
            args: vec![
                "-c".into(),
                r#"printf 'CLAUDECODE=[%s] ENTRY=[%s] KEEP=[%s]\n' "$CLAUDECODE" "$CLAUDE_CODE_ENTRYPOINT" "$CLAUDE_CODE_KEEP" >&2; read line"#
                    .into(),
            ],
            cwd: dir.path().into(),
            env: vec![("CLAUDE_CODE_KEEP".into(), "explicit".into())],
        };
        let process = crate::acp::connection::AgentProcess::spawn(&spec, |_| {}).unwrap();
        let started = Instant::now();
        let stderr = loop {
            if let Some(text) = process.stderr_tail()
                && text.contains("KEEP=")
            {
                break text;
            }
            assert!(started.elapsed() < Duration::from_secs(5), "子进程没有输出");
            thread::sleep(Duration::from_millis(20));
        };
        process.kill();
        assert_eq!(stderr.trim(), "CLAUDECODE=[] ENTRY=[] KEEP=[explicit]");
    }
}
