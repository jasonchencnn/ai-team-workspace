//! 权限请求的自动答复规则（D43），纯函数，不访问数据库。
//!
//! 权限规则（[`super::policy`]）判定「要确认」的请求，先过这里：
//! - 自动拒绝：重度危险（[`super::danger`]：推送与发布、提权与系统级改动、凭据与改写 Git 历史），
//!   `git stash`（会把改动藏到执行分支之外），递归删除落在执行目录以外，以及团队配置的拒绝前缀；
//! - 自动允许：执行命令的每一段都在白名单里（内置的常用构建、测试、只读 Git 与文件命令，加上团队
//!   配置的允许前缀），而且命令里出现的路径、请求涉及的路径都在本次执行目录里；
//! - 都不命中：转给 Leader（经 MCP `answer_permission` 答复，必须写理由），Leader 拿不准再升级给 Human。
//!
//! 只自动答复执行命令（`execute`）与没有声明类型的请求：编辑、删除、移动、联网等仍按原规则转交。

use std::path::Path;

use super::danger::{self, program_words, segments};
use super::policy;
use crate::model::PermissionPrompt;

/// 自动答复的结果（附中文原因）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AutoAnswer {
    Allow(String),
    Deny(String),
}

/// 团队配置的额外前缀。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TeamRules {
    pub allow: Vec<String>,
    pub deny: Vec<String>,
}

/// 内置自动允许的命令：程序名，以及（需要时）允许的子命令。子命令为空表示程序本身就允许。
///
/// `echo`、`printf` 不在里面：它们常被用来往文件里写东西（`echo … > 文件`），需要时团队自己加。
const BUILTIN_ALLOW: &[(&str, &[&str])] = &[
    (
        "cargo",
        &[
            "fmt", "clippy", "test", "build", "check", "doc", "metadata", "tree", "nextest",
        ],
    ),
    (
        "git",
        &[
            "status",
            "diff",
            "log",
            "show",
            "add",
            "commit",
            "rev-parse",
            "ls-files",
            "blame",
            "describe",
        ],
    ),
    (
        "pnpm",
        &["run", "test", "lint", "typecheck", "build", "check", "exec"],
    ),
    ("npm", &["run", "test"]),
    ("yarn", &["run", "test", "lint", "build"]),
    ("npx", &["tsc", "prettier", "eslint", "vitest", "jest"]),
    ("go", &["test", "build", "vet", "fmt"]),
    ("ls", &[]),
    ("find", &[]),
    ("cat", &[]),
    ("head", &[]),
    ("tail", &[]),
    ("wc", &[]),
    ("grep", &[]),
    ("rg", &[]),
    ("pwd", &[]),
    ("mkdir", &[]),
    ("touch", &[]),
    ("which", &[]),
    ("tree", &[]),
    ("sort", &[]),
    ("uniq", &[]),
    ("diff", &[]),
    ("file", &[]),
    ("stat", &[]),
    ("true", &[]),
    ("cd", &[]),
    ("pytest", &[]),
];

/// 对一条需要确认的权限请求做自动答复；不命中返回 `None`（转给 Leader）。
pub fn decide(prompt: &PermissionPrompt, exec_dir: &Path, team: &TeamRules) -> Option<AutoAnswer> {
    if !matches!(prompt.tool_kind.as_deref(), None | Some("execute")) {
        return None;
    }
    let command = prompt.command.as_deref().unwrap_or(&prompt.title);
    let normalized = normalize(command);
    // 自动拒绝
    if let Some(prefix) = team
        .deny
        .iter()
        .find(|prefix| segments(&normalized).any(|seg| starts_with_words(seg, prefix)))
    {
        return Some(AutoAnswer::Deny(format!("团队规则自动拒绝：{prefix}")));
    }
    if let Some(reason) = danger::heavy_danger(prompt) {
        return Some(AutoAnswer::Deny(format!("重度危险操作自动拒绝：{reason}")));
    }
    for segment in segments(&normalized) {
        let words = program_words(segment);
        if words.first() == Some(&"git") && git_sub(&words) == Some("stash") {
            return Some(AutoAnswer::Deny(
                "自动拒绝：git stash 会把改动藏到执行分支之外".to_owned(),
            ));
        }
        if words.first() == Some(&"rm")
            && words[1..]
                .iter()
                .any(|w| w.starts_with('-') && (w.contains('r') || w.contains('R')))
            && words[1..]
                .iter()
                .filter(|w| !w.starts_with('-'))
                .any(|w| !inside(w, exec_dir))
        {
            return Some(AutoAnswer::Deny(
                "自动拒绝：递归删除落在执行目录以外".to_owned(),
            ));
        }
    }
    // 自动允许
    if normalized.trim().is_empty() {
        return None;
    }
    if !prompt.paths.iter().all(|path| inside(path, exec_dir)) {
        return None;
    }
    for segment in segments(&normalized) {
        let words = program_words(segment);
        if words.is_empty() {
            continue;
        }
        let allowed = builtin_allows(&words)
            || team
                .allow
                .iter()
                .any(|prefix| starts_with_words(segment, prefix));
        if !allowed {
            return None;
        }
        if words[1..]
            .iter()
            .filter_map(|w| path_token(w))
            .any(|path| !inside(path, exec_dir))
        {
            return None;
        }
    }
    Some(AutoAnswer::Allow(
        "常用命令且只涉及本次执行目录，自动允许".to_owned(),
    ))
}

/// 去掉不影响判断的写法：`2>&1` 这类重定向会被当成命令分隔符拆开。
fn normalize(command: &str) -> String {
    command
        .replace("2>&1", " ")
        .replace("1>&2", " ")
        .replace("&>", ">")
}

/// 一段命令是否在内置白名单里。
fn builtin_allows(words: &[&str]) -> bool {
    let program = words[0];
    let Some((_, subs)) = BUILTIN_ALLOW.iter().find(|(name, _)| *name == program) else {
        return chmod_exec(words);
    };
    if subs.is_empty() {
        return match program {
            // find 带 -delete / -exec 就不是只读了。
            "find" => !words
                .iter()
                .any(|w| matches!(*w, "-delete" | "-exec" | "-execdir")),
            _ => true,
        };
    }
    let sub = if program == "git" {
        git_sub(words)
    } else {
        words[1..].iter().copied().find(|w| !w.starts_with('-'))
    };
    sub.is_some_and(|sub| subs.contains(&sub))
}

/// `chmod +x <文件>`：只加可执行位。
fn chmod_exec(words: &[&str]) -> bool {
    words[0] == "chmod" && words.get(1) == Some(&"+x")
}

/// git 的子命令（跳过 `-C <目录>`、`-c key=value`）。
fn git_sub<'a>(words: &[&'a str]) -> Option<&'a str> {
    let mut index = 1;
    while index < words.len() && words[index].starts_with('-') {
        index += if matches!(words[index], "-c" | "-C") {
            2
        } else {
            1
        };
    }
    words.get(index).copied()
}

/// 一段命令是否以给定前缀（按词比较）开头。
fn starts_with_words(segment: &str, prefix: &str) -> bool {
    let words = program_words(segment);
    let wanted: Vec<&str> = prefix.split_whitespace().collect();
    !wanted.is_empty() && words.len() >= wanted.len() && words[..wanted.len()] == wanted[..]
}

/// 看起来像路径的词（含 `/`，或以 `.`、`~` 开头），去掉重定向符号与 `--flag=` 前缀。
fn path_token(word: &str) -> Option<&str> {
    let word = word.trim_start_matches(['>', '<']);
    let word = match word.split_once('=') {
        Some((flag, value)) if flag.starts_with('-') => value,
        _ => word,
    };
    let word = word.trim_matches(|c| matches!(c, '"' | '\''));
    let looks = word.contains('/') || word.starts_with('.') || word.starts_with('~');
    (looks && !word.contains("://")).then_some(word)
}

/// 路径是否在执行目录里（`/dev/null` 也算）。`~`、`..` 一律算在外面。
fn inside(path: &str, exec_dir: &Path) -> bool {
    let path = path.trim_matches(|c| matches!(c, '"' | '\''));
    if path == "/dev/null" {
        return true;
    }
    if path.starts_with('~') {
        return false;
    }
    let Ok(real) = policy::resolve(Path::new(path), exec_dir) else {
        return false;
    };
    let base = exec_dir
        .canonicalize()
        .unwrap_or_else(|_| exec_dir.to_path_buf());
    real.starts_with(&base)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn prompt(command: &str) -> PermissionPrompt {
        PermissionPrompt {
            request_id: "r".into(),
            tool_call_id: "t".into(),
            title: format!("运行 {command}"),
            tool_kind: Some("execute".into()),
            paths: Vec::new(),
            command: Some(command.into()),
            reason: String::new(),
            options: Vec::new(),
            requested_at: String::new(),
        }
    }

    fn answer(command: &str, dir: &Path) -> Option<AutoAnswer> {
        decide(&prompt(command), dir, &TeamRules::default())
    }

    #[test]
    fn everyday_commands_in_the_worktree_are_allowed() {
        let dir = tempfile::tempdir().unwrap();
        for command in [
            "cargo test --workspace 2>&1 | tail -20",
            "cargo fmt --all && cargo clippy -- -D warnings",
            "git status && git diff --stat",
            "git add -A && git commit -m 'feat: x'",
            "ls -la src",
            "mkdir -p crates/new/src",
            "chmod +x scripts/check.sh",
            "pnpm run check",
            "cat ./Cargo.toml",
            "cargo build > /dev/null",
        ] {
            assert!(
                matches!(answer(command, dir.path()), Some(AutoAnswer::Allow(_))),
                "{command}"
            );
        }
    }

    #[test]
    fn dangerous_commands_are_denied() {
        let dir = tempfile::tempdir().unwrap();
        for command in [
            "git push origin HEAD",
            "git stash",
            "git -C . stash pop",
            "sudo cargo install x",
            "rm -rf /Users/me/other",
            "rm -rf ../sibling",
            "cat ~/.ssh/id_rsa",
        ] {
            assert!(
                matches!(answer(command, dir.path()), Some(AutoAnswer::Deny(_))),
                "{command}"
            );
        }
    }

    #[test]
    fn everything_else_goes_to_the_leader() {
        let dir = tempfile::tempdir().unwrap();
        for command in [
            "cat /Volumes/repo/README.md",
            "curl -O https://example.com/x.tar.gz",
            "pnpm install",
            "rm -rf target",
            "python script.py",
            "",
        ] {
            assert_eq!(answer(command, dir.path()), None, "{command}");
        }
        // 编辑类请求不在这里自动答复。
        let mut edit = prompt("cargo test");
        edit.tool_kind = Some("edit".into());
        assert_eq!(decide(&edit, dir.path(), &TeamRules::default()), None);
    }

    #[test]
    fn team_rules_extend_both_lists() {
        let dir = tempfile::tempdir().unwrap();
        let team = TeamRules {
            allow: vec!["make test".into()],
            deny: vec!["cargo publish".into(), "docker".into()],
        };
        assert!(matches!(
            decide(&prompt("make test"), dir.path(), &team),
            Some(AutoAnswer::Allow(_))
        ));
        assert_eq!(decide(&prompt("make deploy"), dir.path(), &team), None);
        assert!(matches!(
            decide(&prompt("docker build ."), dir.path(), &team),
            Some(AutoAnswer::Deny(_))
        ));
    }
}
