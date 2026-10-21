//! Git worktree（D7 / D15）：每次执行在应用数据目录下新建 worktree 与分支，基于当前 HEAD；
//! 完成后保留，不自动合并（M3 验收时合并）。以及执行结束时统计改动文件。
//!
//! 只调用 `git` 命令行；`path_env` 为 `None` 时用进程继承的 PATH。

use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, ExitStatus, Stdio};
use std::time::{Duration, Instant};

use crate::model::{ChangedFile, FileChangeKind};

/// 同名分支或目录已存在时最多尝试的后缀数。
const MAX_SUFFIX: usize = 20;

/// 普通 Git 命令的超时：不让应用被一个卡住的 Git 无限阻塞。
pub(crate) const GIT_TIMEOUT: Duration = Duration::from_secs(120);

/// 保全提交（退出、启动时的中断保全）的超时：更短。
///
/// 保全失败只是记一条问题，不能让应用关不掉、也等不到窗口出现。
pub(crate) const COMMIT_TIMEOUT: Duration = Duration::from_secs(20);

/// 新建好的 worktree。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Worktree {
    /// worktree 根目录。
    pub root: PathBuf,
    /// 执行目录：工作空间是仓库子目录时为 worktree 中对应的子目录，否则就是根目录。
    pub exec_dir: PathBuf,
    pub branch: String,
    /// 基线提交（完整哈希）。
    pub base_commit: String,
    /// 创建时 HEAD 所在分支；分离 HEAD 时为 None。
    pub base_branch: Option<String>,
}

pub(crate) fn git(dir: &Path, args: &[&str], path_env: Option<&OsStr>) -> Result<String, String> {
    git_with_timeout(dir, args, path_env, GIT_TIMEOUT)
}

/// 同 [`git`]，但带超时：超时后 kill 掉子进程并报错（不让调用方无限等下去）。
pub(crate) fn git_with_timeout(
    dir: &Path,
    args: &[&str],
    path_env: Option<&OsStr>,
    timeout: Duration,
) -> Result<String, String> {
    let mut command = Command::new("git");
    command.arg("-C").arg(dir).args(args);
    if let Some(path) = path_env {
        command.env("PATH", path);
    }
    crate::system_probe::git_env(&mut command);
    let mut child = command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|err| format!("无法运行 git：{err}"))?;
    // 输出用单独的线程读：管道写满时子进程会卡住，等待与超时都会失灵。
    let mut stdout = child.stdout.take();
    let mut stderr = child.stderr.take();
    let out = std::thread::spawn(move || {
        let mut buf = Vec::new();
        if let Some(stream) = stdout.as_mut() {
            use std::io::Read;
            let _ = stream.read_to_end(&mut buf);
        }
        buf
    });
    let err_out = std::thread::spawn(move || {
        let mut buf = Vec::new();
        if let Some(stream) = stderr.as_mut() {
            use std::io::Read;
            let _ = stream.read_to_end(&mut buf);
        }
        buf
    });
    let status = match wait_with_timeout(&mut child, timeout, args) {
        Ok(status) => status,
        Err(err) => {
            let _ = child.kill();
            let _ = child.wait();
            let _ = out.join();
            let _ = err_out.join();
            return Err(err);
        }
    };
    let out = out.join().unwrap_or_default();
    let err = err_out.join().unwrap_or_default();
    if status.success() {
        Ok(String::from_utf8_lossy(&out).into_owned())
    } else {
        let stderr = String::from_utf8_lossy(&err);
        Err(stderr
            .lines()
            .map(str::trim)
            .rfind(|l| !l.is_empty())
            .unwrap_or("git 执行失败")
            .to_owned())
    }
}

/// 等 Git 子进程结束，最多等 `timeout`；超时返回错误（调用方负责 kill）。
fn wait_with_timeout(
    child: &mut Child,
    timeout: Duration,
    args: &[&str],
) -> Result<ExitStatus, String> {
    let deadline = Instant::now() + timeout;
    loop {
        match child.try_wait() {
            Ok(Some(status)) => return Ok(status),
            Ok(None) => {}
            Err(err) => return Err(format!("等待 git 结束失败：{err}")),
        }
        if Instant::now() >= deadline {
            return Err(format!(
                "git 命令超时（{} 秒）：git {}",
                timeout.as_secs(),
                args.join(" ")
            ));
        }
        std::thread::sleep(Duration::from_millis(20));
    }
}

/// 在 `workspace`（Git 仓库或其子目录）的当前 HEAD 上新建分支 `branch` 和位于 `target` 的 worktree。
/// 分支或目录已存在时依次加后缀 `-2`、`-3`…。仓库没有任何提交时报错。
pub fn create(
    workspace: &Path,
    target: &Path,
    branch: &str,
    path_env: Option<&OsStr>,
) -> Result<Worktree, String> {
    let base_commit = git(workspace, &["rev-parse", "--verify", "HEAD"], path_env)
        .map(|s| s.trim().to_owned())
        .map_err(|_| "Git 仓库还没有任何提交，无法创建 worktree；请先提交一次".to_owned())?;
    let base_branch = git(workspace, &["symbolic-ref", "--quiet", "HEAD"], path_env)
        .ok()
        .and_then(|name| name.trim().strip_prefix("refs/heads/").map(str::to_owned));
    let prefix = git(workspace, &["rev-parse", "--show-prefix"], path_env)
        .map_err(|err| format!("无法读取仓库信息：{err}"))?
        .trim()
        .trim_end_matches('/')
        .to_owned();
    let mut chosen = None;
    for n in 1..=MAX_SUFFIX {
        let (name, dir) = if n == 1 {
            (branch.to_owned(), target.to_owned())
        } else {
            (
                format!("{branch}-{n}"),
                PathBuf::from(format!("{}-{n}", target.display())),
            )
        };
        let taken = git(
            workspace,
            &[
                "show-ref",
                "--verify",
                "--quiet",
                &format!("refs/heads/{name}"),
            ],
            path_env,
        )
        .is_ok();
        if !taken && !dir.exists() {
            chosen = Some((name, dir));
            break;
        }
    }
    let (branch, root) = chosen.ok_or_else(|| format!("分支 {branch} 及其后缀都已存在"))?;
    if let Some(parent) = root.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|err| format!("无法创建 worktree 目录 {}：{err}", parent.display()))?;
    }
    let root_str = root.to_string_lossy().into_owned();
    git(
        workspace,
        &["worktree", "add", "-b", &branch, &root_str, &base_commit],
        path_env,
    )
    .map_err(|err| format!("创建 worktree 失败：{err}"))?;
    let exec_dir = if prefix.is_empty() {
        root.clone()
    } else {
        root.join(&prefix)
    };
    if !exec_dir.is_dir() {
        std::fs::create_dir_all(&exec_dir)
            .map_err(|err| format!("无法创建执行目录 {}：{err}", exec_dir.display()))?;
    }
    Ok(Worktree {
        root,
        exec_dir,
        branch,
        base_commit,
        base_branch,
    })
}

/// worktree 相对基线提交的改动（含已提交、未提交与未跟踪但未忽略的文件），路径相对仓库根，按路径排序。
/// `dir` 可以是 worktree 中的任意目录。
pub fn changed_files(
    dir: &Path,
    base_commit: &str,
    path_env: Option<&OsStr>,
) -> Result<Vec<ChangedFile>, String> {
    let root = PathBuf::from(
        git(dir, &["rev-parse", "--show-toplevel"], path_env)?
            .trim()
            .to_owned(),
    );
    let root = root.as_path();
    let mut files = Vec::new();
    let diff = git(
        root,
        &["diff", "--name-status", "--no-renames", "-z", base_commit],
        path_env,
    )?;
    let mut parts = diff.split('\0').filter(|s| !s.is_empty());
    while let (Some(status), Some(path)) = (parts.next(), parts.next()) {
        let change = match status.chars().next() {
            Some('A') => FileChangeKind::Added,
            Some('D') => FileChangeKind::Deleted,
            _ => FileChangeKind::Modified,
        };
        files.push(ChangedFile {
            path: path.to_owned(),
            change,
        });
    }
    let untracked = git(
        root,
        &["ls-files", "--others", "--exclude-standard", "-z"],
        path_env,
    )?;
    for path in untracked.split('\0').filter(|s| !s.is_empty()) {
        files.push(ChangedFile {
            path: path.to_owned(),
            change: FileChangeKind::Added,
        });
    }
    files.sort_by(|a, b| a.path.cmp(&b.path));
    files.dedup_by(|a, b| a.path == b.path);
    Ok(files)
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;

    fn run(dir: &Path, args: &[&str]) {
        let ok = Command::new("git")
            .arg("-C")
            .arg(dir)
            .args(args)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .unwrap()
            .success();
        assert!(ok, "git {args:?}");
    }

    fn repo(dir: &Path) {
        fs::create_dir_all(dir).unwrap();
        run(dir, &["init", "-q", "-b", "main"]);
        run(dir, &["config", "user.email", "t@example.com"]);
        run(dir, &["config", "user.name", "t"]);
        run(dir, &["config", "commit.gpgsign", "false"]);
    }

    /// 超时的 Git 子进程会被 kill 掉：不让退出或启动被一个卡住的 Git 无限阻塞。
    #[cfg(unix)]
    #[test]
    fn a_git_command_that_takes_too_long_is_killed() {
        let mut child = Command::new("sh")
            .args(["-c", "sleep 5"])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        let err =
            wait_with_timeout(&mut child, Duration::from_millis(200), &["sleep", "5"]).unwrap_err();
        assert!(err.contains("超时"), "{err}");
        let _ = child.kill();
        let _ = child.wait();
    }

    #[test]
    fn creates_worktree_with_suffix_and_reports_changes() {
        let tmp = tempfile::tempdir().unwrap();
        let ws = tmp.path().join("repo");
        repo(&ws);
        let err = create(&ws, &tmp.path().join("wt/a"), "teamspace/A-r1", None).unwrap_err();
        assert!(err.contains("没有任何提交"), "{err}");

        fs::create_dir_all(ws.join("app")).unwrap();
        fs::write(ws.join("app/old.txt"), "1").unwrap();
        fs::write(ws.join("gone.txt"), "1").unwrap();
        run(&ws, &["add", "."]);
        run(&ws, &["commit", "-q", "-m", "init"]);
        run(&ws, &["tag", "main"]);

        let wt = create(&ws, &tmp.path().join("wt/A-r1"), "teamspace/A-r1", None).unwrap();
        assert_eq!(wt.branch, "teamspace/A-r1");
        assert_eq!(wt.exec_dir, wt.root);
        assert_eq!(wt.base_commit.len(), 40);
        assert_eq!(wt.base_branch.as_deref(), Some("main"));
        assert!(wt.root.join("app/old.txt").is_file());

        // 同名分支已存在：加后缀。
        let wt2 = create(&ws, &tmp.path().join("wt/A-r1"), "teamspace/A-r1", None).unwrap();
        assert_eq!(wt2.branch, "teamspace/A-r1-2");
        assert!(wt2.root.ends_with("A-r1-2"));

        // 仓库子目录作为工作空间：执行目录是 worktree 中的同一子目录。
        let sub = create(
            &ws.join("app"),
            &tmp.path().join("wt/S"),
            "teamspace/S",
            None,
        )
        .unwrap();
        assert_eq!(sub.exec_dir, sub.root.join("app"));

        fs::write(wt.root.join("app/old.txt"), "2").unwrap();
        fs::write(wt.root.join("new.txt"), "n").unwrap();
        fs::remove_file(wt.root.join("gone.txt")).unwrap();
        let changes = changed_files(&wt.root.join("app"), &wt.base_commit, None).unwrap();
        assert_eq!(
            changes,
            vec![
                ChangedFile {
                    path: "app/old.txt".into(),
                    change: FileChangeKind::Modified
                },
                ChangedFile {
                    path: "gone.txt".into(),
                    change: FileChangeKind::Deleted
                },
                ChangedFile {
                    path: "new.txt".into(),
                    change: FileChangeKind::Added
                },
            ]
        );
        // 工作空间本身不受影响。
        assert_eq!(fs::read_to_string(ws.join("app/old.txt")).unwrap(), "1");
    }

    #[test]
    fn detached_head_has_no_base_branch() {
        let tmp = tempfile::tempdir().unwrap();
        let ws = tmp.path().join("repo");
        repo(&ws);
        run(&ws, &["commit", "--allow-empty", "-q", "-m", "init"]);
        run(&ws, &["checkout", "--detach", "-q"]);
        let wt = create(&ws, &tmp.path().join("wt"), "run", None).unwrap();
        assert_eq!(wt.base_branch, None);
    }
}
