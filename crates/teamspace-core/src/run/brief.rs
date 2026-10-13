//! 任务说明包：第一个 `session/prompt` 的内容（中文），纯函数（读引用文件内容除外）。
//!
//! 内容：岗位名与职责、工作约定、岗位工作日志最近若干条、任务编号/标题/说明、引用文件、依赖任务状态、
//! 工作环境（执行目录与隔离方式）、工作要求。引用文件在 Agent 支持 `embeddedContext` 时以嵌入文本
//! （`resource`）附上，否则（或文件过大、不是文本）以 `resource_link` 附上；两者都是 ACP 基线能力之内的
//! 内容块。

use std::path::{Path, PathBuf};

use serde_json::{Value, json};

use crate::model::RunIsolation;
use crate::role_folder::{BRIEF_FILE, JOURNAL_FILE, PROCESS_DIR};

/// 说明包中带上的工作日志条数。
pub const JOURNAL_LINES: usize = 10;
/// 嵌入引用文件内容的大小上限（字节），超过时改为链接。
pub const EMBED_LIMIT: u64 = 64 * 1024;
/// 执行摘要的字符上限。
pub const SUMMARY_LIMIT: usize = 600;

/// 一个引用文件。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BriefRef {
    /// 相对工作空间的路径。
    pub path: String,
    pub anchor: Option<String>,
    /// 磁盘上的绝对路径（执行目录中存在时用执行目录中的，否则用工作空间中的）。
    pub absolute: PathBuf,
}

/// 依赖任务。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BriefDependency {
    pub key: String,
    pub title: String,
    pub status_label: String,
}

/// 生成说明包所需的全部信息。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BriefInput {
    pub role_name: String,
    pub role_description: String,
    pub role_guidelines: String,
    /// 工作日志最近的条目，最新的在前。
    pub journal: Vec<String>,
    pub task_key: String,
    pub task_title: String,
    pub task_description: String,
    pub refs: Vec<BriefRef>,
    pub dependencies: Vec<BriefDependency>,
    pub exec_dir: String,
    pub isolation: RunIsolation,
    pub branch: Option<String>,
    /// 岗位文件夹相对工作空间（也即相对执行目录）的路径，如 `Teamspace/前端开发`；尚未建立时为 `None`。
    pub role_folder: Option<String>,
}

fn or_none(text: &str) -> &str {
    if text.trim().is_empty() {
        "（未填写）"
    } else {
        text.trim()
    }
}

/// 说明包正文（Markdown）。
pub fn render(input: &BriefInput) -> String {
    let mut out = String::new();
    let push = |out: &mut String, line: &str| {
        out.push_str(line);
        out.push('\n');
    };
    push(&mut out, "# 任务说明（由 Teamspace 生成）");
    push(&mut out, "");
    push(
        &mut out,
        &format!(
            "你是团队中的「{}」岗位，正在通过 Teamspace 执行任务 {}。",
            input.role_name, input.task_key
        ),
    );
    push(&mut out, "");
    push(&mut out, "## 岗位职责");
    push(&mut out, or_none(&input.role_description));
    push(&mut out, "");
    push(&mut out, "## 工作约定");
    push(&mut out, or_none(&input.role_guidelines));
    push(&mut out, "");
    push(
        &mut out,
        &format!("## 岗位工作日志（最近 {} 条，最新的在前）", JOURNAL_LINES),
    );
    if input.journal.is_empty() {
        push(&mut out, "（暂无记录）");
    }
    for line in input.journal.iter().take(JOURNAL_LINES) {
        push(&mut out, &format!("- {line}"));
    }
    push(&mut out, "");
    push(&mut out, "## 任务");
    push(&mut out, &format!("- 编号：{}", input.task_key));
    push(&mut out, &format!("- 标题：{}", input.task_title));
    push(&mut out, "- 说明：");
    push(&mut out, or_none(&input.task_description));
    push(&mut out, "");
    push(&mut out, "## 引用文件");
    if input.refs.is_empty() {
        push(&mut out, "（无）");
    }
    for r in &input.refs {
        let anchor = r
            .anchor
            .as_deref()
            .map(|a| format!("（定位：#{a}）"))
            .unwrap_or_default();
        push(
            &mut out,
            &format!("- `{}`{anchor}：{}", r.path, r.absolute.display()),
        );
    }
    if !input.refs.is_empty() {
        push(&mut out, "引用文件的内容或链接附在本说明之后。");
    }
    push(&mut out, "");
    push(&mut out, "## 依赖任务");
    if input.dependencies.is_empty() {
        push(&mut out, "（无）");
    }
    for d in &input.dependencies {
        push(
            &mut out,
            &format!("- {} {}（{}）", d.key, d.title, d.status_label),
        );
    }
    push(&mut out, "");
    push(&mut out, "## 工作环境");
    push(&mut out, &format!("- 工作目录：{}", input.exec_dir));
    match (input.isolation, &input.branch) {
        (RunIsolation::Worktree, branch) => push(
            &mut out,
            &format!(
                "- 隔离方式：独立的 Git worktree，分支 `{}`。改动留在这个分支上，由 Human 验收后合并；\
                 不需要你合并或推送。",
                branch.as_deref().unwrap_or("（未知）")
            ),
        ),
        (RunIsolation::InPlace, _) => push(
            &mut out,
            "- 隔离方式：直接在工作空间中工作（普通文件夹）。写入本岗位文件夹和引用文件之外的位置、\
             执行命令都需要 Human 确认，请尽量把产出放在岗位文件夹里。",
        ),
    }
    push(&mut out, "");
    push(&mut out, "## 工作要求");
    let folder = input
        .role_folder
        .clone()
        .unwrap_or_else(|| format!("Teamspace/{}", input.role_name));
    push(
        &mut out,
        &format!(
            "1. 过程文件（草稿、调研记录、中间结果）放在 `{folder}/{PROCESS_DIR}/`（相对工作目录）。"
        ),
    );
    push(
        &mut out,
        &format!(
            "2. 不要修改 `{folder}/{BRIEF_FILE}` 和 `{folder}/{JOURNAL_FILE}`，它们由 Teamspace 维护。"
        ),
    );
    push(
        &mut out,
        "3. 完成后用一段话总结：做了什么、改了哪些文件。这段总结会写入岗位工作日志。",
    );
    out
}

/// 把绝对路径转成 `file://` URI（非安全字符按 UTF-8 字节百分号编码）。
pub fn file_uri(path: &Path) -> String {
    let mut out = String::from("file://");
    for byte in path.to_string_lossy().bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' | b'/' => {
                out.push(byte as char)
            }
            other => out.push_str(&format!("%{other:02X}")),
        }
    }
    out
}

/// [`file_uri`] 的逆运算；不是 `file://` URI 时返回 `None`。
pub fn path_from_uri(uri: &str) -> Option<PathBuf> {
    let rest = uri.strip_prefix("file://")?;
    let bytes = rest.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%'
            && i + 2 < bytes.len()
            && let Ok(v) = u8::from_str_radix(rest.get(i + 1..i + 3)?, 16)
        {
            out.push(v);
            i += 3;
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    Some(PathBuf::from(String::from_utf8(out).ok()?))
}

/// 引用文件的文本内容（不超过 [`EMBED_LIMIT`] 且是 UTF-8 文本时）。
fn embeddable(path: &Path) -> Option<String> {
    let meta = std::fs::metadata(path).ok()?;
    if !meta.is_file() || meta.len() > EMBED_LIMIT {
        return None;
    }
    String::from_utf8(std::fs::read(path).ok()?).ok()
}

/// `session/prompt` 的内容块：正文一个 text 块，之后每个引用文件一个 resource（嵌入）或
/// resource_link 块。
pub fn prompt_blocks(text: &str, refs: &[BriefRef], embedded_context: bool) -> Vec<Value> {
    let mut blocks = vec![json!({ "type": "text", "text": text })];
    for r in refs {
        let uri = file_uri(&r.absolute);
        let content = embedded_context.then(|| embeddable(&r.absolute)).flatten();
        blocks.push(match content {
            Some(text) => json!({
                "type": "resource",
                "resource": { "uri": uri, "text": text, "mimeType": mime(&r.path) }
            }),
            None => json!({
                "type": "resource_link",
                "uri": uri,
                "name": r.path,
            }),
        });
    }
    blocks
}

fn mime(path: &str) -> &'static str {
    match path.rsplit('.').next().unwrap_or_default() {
        "md" | "markdown" => "text/markdown",
        _ => "text/plain",
    }
}

/// 续接（`session/load` 之后）时发送的提示：重申过程文件位置与总结要求。
pub fn resume_text(task_key: &str, role_folder: &str) -> String {
    format!(
        "Teamspace 重新连接了这个会话（之前的执行因应用退出而中断）。请检查任务 {task_key} 的进度并继续完成。\
         过程文件仍放在 `{role_folder}/{PROCESS_DIR}/`（相对工作目录），不要修改 `{role_folder}/{BRIEF_FILE}` \
         和 `{role_folder}/{JOURNAL_FILE}`；完成后用一段话总结：做了什么、改了哪些文件。"
    )
}

/// 从 Agent 最后的消息中截取执行摘要：去掉首尾空白，超过 [`SUMMARY_LIMIT`] 个字符时截断并加省略号。
pub fn summarize(text: &str) -> Option<String> {
    let text = text.trim();
    if text.is_empty() {
        return None;
    }
    let mut out: String = text.chars().take(SUMMARY_LIMIT).collect();
    if text.chars().count() > SUMMARY_LIMIT {
        out.push('…');
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn input() -> BriefInput {
        BriefInput {
            role_name: "前端开发".into(),
            role_description: "负责页面".into(),
            role_guidelines: String::new(),
            journal: (1..=12).map(|i| format!("日志 {i}")).collect(),
            task_key: "FE-001".into(),
            task_title: "登录页 [mock:perm]".into(),
            task_description: "按设计稿实现".into(),
            refs: vec![BriefRef {
                path: "docs/设计 稿.md".into(),
                anchor: Some("登录".into()),
                absolute: "/ws/docs/设计 稿.md".into(),
            }],
            dependencies: vec![BriefDependency {
                key: "BE-001".into(),
                title: "登录接口".into(),
                status_label: "已完成".into(),
            }],
            exec_dir: "/data/worktrees/FE-001-r1".into(),
            isolation: RunIsolation::Worktree,
            branch: Some("teamspace/FE-001-r1".into()),
            role_folder: Some("Teamspace/前端开发".into()),
        }
    }

    #[test]
    fn brief_contains_every_section() {
        let text = render(&input());
        for needle in [
            "「前端开发」岗位",
            "## 岗位职责\n负责页面",
            "## 工作约定\n（未填写）",
            "- 日志 1\n",
            "- 日志 10\n",
            "- 编号：FE-001",
            "- 标题：登录页 [mock:perm]",
            "按设计稿实现",
            "`docs/设计 稿.md`（定位：#登录）：/ws/docs/设计 稿.md",
            "- BE-001 登录接口（已完成）",
            "- 工作目录：/data/worktrees/FE-001-r1",
            "分支 `teamspace/FE-001-r1`",
            "`Teamspace/前端开发/过程文件/`",
            "不要修改 `Teamspace/前端开发/岗位说明.md`",
            "用一段话总结",
        ] {
            assert!(text.contains(needle), "missing {needle:?} in\n{text}");
        }
        assert!(
            !text.contains("日志 11"),
            "only {JOURNAL_LINES} journal lines"
        );
    }

    #[test]
    fn in_place_brief_mentions_confirmation_and_empty_sections() {
        let text = render(&BriefInput {
            isolation: RunIsolation::InPlace,
            branch: None,
            journal: vec![],
            refs: vec![],
            dependencies: vec![],
            role_folder: None,
            ..input()
        });
        assert!(text.contains("直接在工作空间中工作"));
        assert!(text.contains("（暂无记录）"));
        assert!(text.contains("## 引用文件\n（无）"));
        assert!(text.contains("## 依赖任务\n（无）"));
        assert!(text.contains("`Teamspace/前端开发/过程文件/`"));
    }

    #[test]
    fn resume_text_restates_requirements() {
        let text = resume_text("FE-001", "Teamspace/前端");
        assert!(text.contains("FE-001"));
        assert!(text.contains("`Teamspace/前端/过程文件/`"));
        assert!(text.contains("用一段话总结"));
    }

    #[test]
    fn uris_round_trip() {
        let path = Path::new("/ws/docs/设计 稿#1.md");
        let uri = file_uri(path);
        assert!(uri.starts_with("file:///ws/docs/"));
        assert!(!uri.contains(' ') && !uri.contains('#'));
        assert_eq!(path_from_uri(&uri).unwrap(), path);
        assert_eq!(path_from_uri("https://x"), None);
    }

    #[test]
    fn prompt_blocks_embed_small_text_or_link() {
        let dir = tempfile::tempdir().unwrap();
        let small = dir.path().join("a.md");
        std::fs::write(&small, "# 标题").unwrap();
        let big = dir.path().join("big.txt");
        std::fs::write(&big, vec![b'x'; EMBED_LIMIT as usize + 1]).unwrap();
        let refs = vec![
            BriefRef {
                path: "a.md".into(),
                anchor: None,
                absolute: small.clone(),
            },
            BriefRef {
                path: "big.txt".into(),
                anchor: None,
                absolute: big,
            },
        ];
        let blocks = prompt_blocks("正文", &refs, true);
        assert_eq!(blocks[0], json!({ "type": "text", "text": "正文" }));
        assert_eq!(blocks[1]["type"], "resource");
        assert_eq!(blocks[1]["resource"]["text"], "# 标题");
        assert_eq!(blocks[1]["resource"]["mimeType"], "text/markdown");
        assert_eq!(blocks[2]["type"], "resource_link");
        assert_eq!(blocks[2]["name"], "big.txt");

        let blocks = prompt_blocks("正文", &refs, false);
        assert_eq!(blocks[1]["type"], "resource_link");
        assert_eq!(
            path_from_uri(blocks[1]["uri"].as_str().unwrap()).unwrap(),
            small
        );
    }

    #[test]
    fn summaries_are_trimmed_and_truncated() {
        assert_eq!(summarize("  \n"), None);
        assert_eq!(summarize(" 完成了 ").as_deref(), Some("完成了"));
        let long = "字".repeat(SUMMARY_LIMIT + 5);
        let s = summarize(&long).unwrap();
        assert_eq!(s.chars().count(), SUMMARY_LIMIT + 1);
        assert!(s.ends_with('…'));
    }
}
