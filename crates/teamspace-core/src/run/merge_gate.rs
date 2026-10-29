//! Leader 合并的闸门（D43）：自治级别、受保护路径、文档或测试类改动的判断，以及合并前的检查命令。
//!
//! 检查命令在临时 worktree 里跑：从基线检出、把执行分支合进来（不动真正的基线），在那里以
//! `sh -c` 运行团队配置的命令，退出码为 0 才算通过。跑完删掉临时 worktree。

use std::ffi::OsStr;
use std::io::Read;
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use crate::run::worktree::{GIT_TIMEOUT, git, git_with_timeout};

/// 检查命令最多跑多久。
pub const CHECK_TIMEOUT: Duration = Duration::from_secs(20 * 60);
/// 失败时带回的输出末尾长度（字符）。
const OUTPUT_TAIL: usize = 2000;

/// 检查命令的结果。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CheckOutcome {
    pub passed: bool,
    /// 失败原因（退出码、超时、合并冲突）与输出末尾。
    pub detail: String,
}

/// 改动里碰到受保护路径的文件。`dir/` 结尾的规则表示整个目录，其余按文件精确匹配。
pub fn protected_hits(files: &[String], protected: &[String]) -> Vec<String> {
    files
        .iter()
        .filter(|file| {
            protected.iter().any(|rule| match rule.strip_suffix('/') {
                Some(dir) => file.starts_with(&format!("{dir}/")),
                None => *file == rule,
            })
        })
        .cloned()
        .collect()
}

/// 改动是否只有文档或测试（自治级别 1 只合并这类）：Markdown 与文本、`docs/`、测试目录与测试文件，
/// 以及 Teamspace 自己维护的岗位文件夹。返回第一个不是这类的文件。
pub fn first_non_doc_or_test(files: &[String]) -> Option<&String> {
    files.iter().find(|file| !doc_or_test(file))
}

fn doc_or_test(file: &str) -> bool {
    let name = file.rsplit('/').next().unwrap_or(file);
    let parts: Vec<&str> = file.split('/').collect();
    let dirs = &parts[..parts.len().saturating_sub(1)];
    let doc = name.ends_with(".md") || name.ends_with(".txt") || name.ends_with(".rst");
    let in_doc_dir = dirs
        .iter()
        .any(|d| matches!(*d, "docs" | "doc" | "Teamspace"));
    let in_test_dir = dirs
        .iter()
        .any(|d| matches!(*d, "tests" | "test" | "__tests__" | "e2e" | "fixtures"));
    let test_file = name.contains("_test.")
        || name.contains(".test.")
        || name.contains(".spec.")
        || name.starts_with("test_");
    doc || in_doc_dir || in_test_dir || test_file
}

/// 在 `dir` 里以 `sh -c` 运行检查命令，最多 `timeout`。
pub fn run_check(
    dir: &Path,
    command: &str,
    path_env: Option<&OsStr>,
    timeout: Duration,
) -> CheckOutcome {
    let mut cmd = Command::new("sh");
    cmd.arg("-c").arg(command).current_dir(dir);
    if let Some(path) = path_env {
        cmd.env("PATH", path);
    }
    let mut child = match cmd
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
    {
        Ok(child) => child,
        Err(err) => {
            return CheckOutcome {
                passed: false,
                detail: format!("无法运行检查命令：{err}"),
            };
        }
    };
    let readers: Vec<_> = [
        child
            .stdout
            .take()
            .map(|s| Box::new(s) as Box<dyn Read + Send>),
        child
            .stderr
            .take()
            .map(|s| Box::new(s) as Box<dyn Read + Send>),
    ]
    .into_iter()
    .flatten()
    .map(|mut stream| {
        std::thread::spawn(move || {
            let mut buf = Vec::new();
            let _ = stream.read_to_end(&mut buf);
            buf
        })
    })
    .collect();
    let deadline = Instant::now() + timeout;
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break Some(status),
            Ok(None) if Instant::now() >= deadline => {
                let _ = child.kill();
                let _ = child.wait();
                break None;
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(100)),
            Err(_) => break None,
        }
    };
    let mut output = String::new();
    for reader in readers {
        output.push_str(&String::from_utf8_lossy(&reader.join().unwrap_or_default()));
    }
    let tail: String = {
        let chars: Vec<char> = output.trim_end().chars().collect();
        chars[chars.len().saturating_sub(OUTPUT_TAIL)..]
            .iter()
            .collect()
    };
    match status {
        Some(status) if status.success() => CheckOutcome {
            passed: true,
            detail: String::new(),
        },
        Some(status) => CheckOutcome {
            passed: false,
            detail: format!(
                "检查命令失败（退出码 {}）：\n{tail}",
                status.code().map_or("未知".to_owned(), |c| c.to_string())
            ),
        },
        None => CheckOutcome {
            passed: false,
            detail: format!("检查命令超时（{} 秒）：\n{tail}", timeout.as_secs()),
        },
    }
}

/// 把执行分支合进基线后在临时 worktree 里跑检查命令；不动真正的基线与执行分支。
///
/// `scratch` 是放临时 worktree 的目录（应用数据目录下），跑完删掉。
pub fn check_merged(
    repo_dir: &Path,
    base_branch: &str,
    run_branch: &str,
    command: &str,
    scratch: &Path,
    path_env: Option<&OsStr>,
    timeout: Duration,
) -> Result<CheckOutcome, String> {
    std::fs::create_dir_all(scratch).map_err(|err| format!("无法创建临时目录：{err}"))?;
    let dir = scratch.join(format!("check-{}", crate::store::new_id()));
    let dir_text = dir.display().to_string();
    git_with_timeout(
        repo_dir,
        &["worktree", "add", "--detach", &dir_text, base_branch],
        path_env,
        GIT_TIMEOUT,
    )
    .map_err(|err| format!("无法准备检查用的临时 worktree：{err}"))?;
    let merged = git_with_timeout(
        &dir,
        &[
            "-c",
            "user.name=Teamspace",
            "-c",
            "user.email=teamspace@localhost",
            "-c",
            "commit.gpgsign=false",
            "merge",
            "--no-ff",
            "--no-edit",
            run_branch,
        ],
        path_env,
        GIT_TIMEOUT,
    );
    let outcome = match merged {
        Ok(_) => run_check(&dir, command, path_env, timeout),
        Err(err) => CheckOutcome {
            passed: false,
            detail: format!("执行分支合进基线时冲突或失败：{err}"),
        },
    };
    let _ = git(
        repo_dir,
        &["worktree", "remove", "--force", &dir_text],
        path_env,
    );
    let _ = std::fs::remove_dir_all(&dir);
    Ok(outcome)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn files(list: &[&str]) -> Vec<String> {
        list.iter().map(|f| (*f).to_owned()).collect()
    }

    #[test]
    fn protected_rules_match_files_and_directories() {
        let rules = files(&["docs/DECISIONS.md", ".github/", "Cargo.toml"]);
        let changed = files(&[
            "docs/DECISIONS.md",
            "docs/PLAN.md",
            ".github/workflows/ci.yml",
            "crates/a/Cargo.toml",
            "Cargo.toml",
        ]);
        assert_eq!(
            protected_hits(&changed, &rules),
            files(&[
                "docs/DECISIONS.md",
                ".github/workflows/ci.yml",
                "Cargo.toml"
            ])
        );
    }

    #[test]
    fn docs_and_tests_are_recognised() {
        let ok = files(&[
            "README.md",
            "docs/guide/setup.png",
            "crates/x/tests/flow.rs",
            "src/app.test.ts",
            "e2e/login.spec.ts",
            "Teamspace/前端/工作日志.md",
        ]);
        assert_eq!(first_non_doc_or_test(&ok), None);
        let code = files(&["docs/a.md", "src/main.rs"]);
        assert_eq!(
            first_non_doc_or_test(&code).map(String::as_str),
            Some("src/main.rs")
        );
    }

    #[test]
    fn check_commands_report_exit_codes_and_output() {
        let dir = tempfile::tempdir().unwrap();
        assert!(run_check(dir.path(), "true", None, Duration::from_secs(10)).passed);
        let failed = run_check(
            dir.path(),
            "echo boom; exit 3",
            None,
            Duration::from_secs(10),
        );
        assert!(!failed.passed);
        assert!(failed.detail.contains("退出码 3"), "{}", failed.detail);
        assert!(failed.detail.contains("boom"), "{}", failed.detail);
        let slow = run_check(dir.path(), "sleep 5", None, Duration::from_millis(200));
        assert!(slow.detail.contains("超时"), "{}", slow.detail);
    }
}
