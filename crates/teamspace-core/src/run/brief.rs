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
    /// 引用不可用的原因（[`ref_problem`]），如「文件不存在」；可用时为 `None`。
    pub unavailable: Option<String>,
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
    /// 工作空间中本岗位工作日志的绝对路径（Teamspace 写入的那一份；worktree 执行时不在执行目录内）。
    /// 为 `None` 时按 `role_folder` 给出相对路径。
    pub journal_path: Option<String>,
}

/// 工作日志条目的格式说明（与 [`crate::role_folder::JournalEntry`] 写入的行一致）。
fn journal_format(task_key: &str) -> String {
    format!("`- YYYY-MM-DD HH:MM:SS UTC±HH:MM · {task_key} · 进展`（本地时间）")
}

/// 「可以在工作日志末尾追加进展」的要求（D14）。
fn journal_rule(journal: &str, task_key: &str) -> String {
    format!(
        "执行过程中可以在本岗位工作日志 `{journal}` 的末尾追加进展，每条一行，格式与已有条目一致：{}。\
         只能追加：写入前先重新读取文件，新内容必须以文件现有内容开头；修改或删除已有内容需要 Human 确认。",
        journal_format(task_key)
    )
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
        let note = r
            .unavailable
            .as_deref()
            .map(|why| format!("（{why}）"))
            .unwrap_or_default();
        push(
            &mut out,
            &format!("- `{}`{anchor}：{}{note}", r.path, r.absolute.display()),
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
    let journal = input
        .journal_path
        .clone()
        .unwrap_or_else(|| format!("{folder}/{JOURNAL_FILE}"));
    push(
        &mut out,
        &format!("2. {}", journal_rule(&journal, &input.task_key)),
    );
    push(
        &mut out,
        &format!("3. 不要修改 `{folder}/{BRIEF_FILE}`，它由 Teamspace 维护。"),
    );
    push(
        &mut out,
        "4. 完成后用一段话总结：做了什么、改了哪些文件。这段总结会写入岗位工作日志。",
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

/// 引用不可用的原因：文件不存在时为「文件不存在」，真实路径不在 `roots` 之内时为
/// 「引用不可用：真实路径不在工作空间内」；可用时为 `None`。
pub fn ref_problem(path: &Path, roots: &[&Path]) -> Option<String> {
    if path.canonicalize().is_err() {
        return Some("文件不存在".to_owned());
    }
    (!ref_inside(path, roots)).then(|| "引用不可用：真实路径不在工作空间内".to_owned())
}

/// 引用文件的真实路径（解析符号链接后）是否在 `roots`（工作空间、执行目录）之一之内（Q-06）。
/// 文件不存在或无法解析时为 `false`。
pub fn ref_inside(path: &Path, roots: &[&Path]) -> bool {
    let Ok(real) = path.canonicalize() else {
        return false;
    };
    roots
        .iter()
        .filter_map(|root| root.canonicalize().ok())
        .any(|root| real.starts_with(root))
}

/// `session/prompt` 的内容块：正文一个 text 块，之后每个引用文件一个 resource（嵌入）或
/// resource_link 块。真实路径不在 `roots` 之内的引用（指向外部的符号链接）既不嵌入也不链接。
pub fn prompt_blocks(
    text: &str,
    refs: &[BriefRef],
    embedded_context: bool,
    roots: &[&Path],
) -> Vec<Value> {
    let mut blocks = vec![json!({ "type": "text", "text": text })];
    for r in refs
        .iter()
        .filter(|r| r.unavailable.is_none() && ref_inside(&r.absolute, roots))
    {
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

/// 续接（`session/load` 之后）时发送的提示：重申过程文件位置、工作日志追加规则与总结要求。
/// `journal` 为工作日志路径（`None` 时按 `role_folder` 给出相对路径）。
pub fn resume_text(task_key: &str, role_folder: &str, journal: Option<&str>) -> String {
    let journal = journal.map_or_else(|| format!("{role_folder}/{JOURNAL_FILE}"), str::to_owned);
    format!(
        "Teamspace 重新连接了这个会话（之前的执行因应用退出而中断）。请检查任务 {task_key} 的进度并继续完成。\
         过程文件仍放在 `{role_folder}/{PROCESS_DIR}/`（相对工作目录）。{}不要修改 `{role_folder}/{BRIEF_FILE}`；\
         完成后用一段话总结：做了什么、改了哪些文件。",
        journal_rule(&journal, task_key)
    )
}

/// 行首的开始标签名（`<name` 后紧跟空白、`>` 或 `/`）。标签名为小写字母开头，后接小写字母、数字和连字符。
fn opening_tag_name(line: &str) -> Option<&str> {
    let rest = line.trim_start().strip_prefix('<')?;
    let end = rest
        .find(|c: char| !(c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-'))
        .unwrap_or(rest.len());
    let name = &rest[..end];
    let starts_with_letter = name.starts_with(|c: char| c.is_ascii_lowercase());
    let boundary_ok = rest[end..]
        .chars()
        .next()
        .is_none_or(|c| c.is_whitespace() || c == '>' || c == '/');
    (starts_with_letter && boundary_ok).then_some(name)
}

/// 去掉 Agent 界面用的显示标记（如 `<deliver-assets>…</deliver-assets>`、独立成行的 `<media … />`）。
///
/// 只处理整行开始的标签：成对标签块（可跨多行、可含自闭合标签）整块删除；独立成行的自闭合标签删除；
/// 正文里的普通尖括号文字（如 `a < b`）不动。清理后压缩多余空行并去掉首尾空白；清理后为空则保留原文（去首尾空白）。
pub fn strip_display_markup(text: &str) -> String {
    let lines: Vec<&str> = text.lines().collect();
    let mut kept: Vec<&str> = Vec::with_capacity(lines.len());
    let mut i = 0;
    while i < lines.len() {
        let line = lines[i];
        let trimmed = line.trim();
        if let Some(name) = opening_tag_name(line) {
            let close = format!("</{name}>");
            if trimmed.ends_with("/>") {
                // 独立成行的自闭合标签。
                i += 1;
                continue;
            }
            if trimmed.contains(&close) {
                // 同一行内的成对标签：整行以闭合标签结尾时整行删除。
                if trimmed.ends_with(&close) {
                    i += 1;
                    continue;
                }
            } else if let Some(j) = (i + 1..lines.len()).find(|&j| lines[j].trim() == close) {
                // 跨行的成对标签块：从开始行到闭合行整块删除。
                i = j + 1;
                continue;
            }
        }
        kept.push(line);
        i += 1;
    }

    let mut out = String::new();
    let mut prev_blank = true;
    for line in kept {
        let blank = line.trim().is_empty();
        if blank && prev_blank {
            continue;
        }
        if !out.is_empty() || !blank {
            out.push_str(line);
            out.push('\n');
        }
        prev_blank = blank;
    }
    let cleaned = out.trim();
    if cleaned.is_empty() {
        text.trim().to_string()
    } else {
        cleaned.to_string()
    }
}

/// 从 Agent 最后的消息中截取执行摘要：先去掉显示标记（见 [`strip_display_markup`]）和首尾空白，
/// 超过 [`SUMMARY_LIMIT`] 个字符时截断并加省略号。
pub fn summarize(text: &str) -> Option<String> {
    let cleaned = strip_display_markup(text);
    let text = cleaned.as_str();
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
                unavailable: None,
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
            journal_path: Some("/ws/Teamspace/前端开发/工作日志.md".into()),
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
            "可以在本岗位工作日志 `/ws/Teamspace/前端开发/工作日志.md` 的末尾追加进展",
            "`- YYYY-MM-DD HH:MM:SS UTC±HH:MM · FE-001 · 进展`",
            "只能追加",
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
            journal_path: None,
            ..input()
        });
        assert!(text.contains("直接在工作空间中工作"));
        assert!(text.contains("（暂无记录）"));
        assert!(text.contains("## 引用文件\n（无）"));
        assert!(text.contains("## 依赖任务\n（无）"));
        assert!(text.contains("`Teamspace/前端开发/过程文件/`"));
        assert!(text.contains("工作日志 `Teamspace/前端开发/工作日志.md` 的末尾追加进展"));
        assert!(!text.contains("不要修改 `Teamspace/前端开发/工作日志.md`"));
    }

    #[test]
    fn resume_text_restates_requirements() {
        let text = resume_text(
            "FE-001",
            "Teamspace/前端",
            Some("/ws/Teamspace/前端/工作日志.md"),
        );
        assert!(text.contains("FE-001"));
        assert!(text.contains("`/ws/Teamspace/前端/工作日志.md` 的末尾追加进展"));
        assert!(text.contains("不要修改 `Teamspace/前端/岗位说明.md`"));
        let text = resume_text("FE-001", "Teamspace/前端", None);
        assert!(text.contains("`Teamspace/前端/工作日志.md` 的末尾追加进展"));
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
                unavailable: None,
            },
            BriefRef {
                path: "big.txt".into(),
                anchor: None,
                absolute: big,
                unavailable: None,
            },
        ];
        let root = dir.path();
        let blocks = prompt_blocks("正文", &refs, true, &[root]);
        assert_eq!(blocks[0], json!({ "type": "text", "text": "正文" }));
        assert_eq!(blocks[1]["type"], "resource");
        assert_eq!(blocks[1]["resource"]["text"], "# 标题");
        assert_eq!(blocks[1]["resource"]["mimeType"], "text/markdown");
        assert_eq!(blocks[2]["type"], "resource_link");
        assert_eq!(blocks[2]["name"], "big.txt");

        let blocks = prompt_blocks("正文", &refs, false, &[root]);
        assert_eq!(blocks[1]["type"], "resource_link");
        assert_eq!(
            path_from_uri(blocks[1]["uri"].as_str().unwrap()).unwrap(),
            small
        );

        // 指向根目录外的符号链接、不存在的文件：不出现在内容块里。
        let other = tempfile::tempdir().unwrap();
        let secret = other.path().join("secret.txt");
        std::fs::write(&secret, "TOP").unwrap();
        let link = dir.path().join("link.md");
        std::os::unix::fs::symlink(&secret, &link).unwrap();
        let outside = [
            BriefRef {
                path: "link.md".into(),
                anchor: None,
                absolute: link.clone(),
                unavailable: None,
            },
            BriefRef {
                path: "missing.md".into(),
                anchor: None,
                absolute: dir.path().join("missing.md"),
                unavailable: None,
            },
        ];
        assert_eq!(prompt_blocks("正文", &outside, true, &[root]).len(), 1);
        assert_eq!(
            ref_problem(&link, &[root]).as_deref(),
            Some("引用不可用：真实路径不在工作空间内")
        );
        assert_eq!(
            ref_problem(&dir.path().join("missing.md"), &[root]).as_deref(),
            Some("文件不存在")
        );
        assert_eq!(ref_problem(&small, &[root]), None);
    }

    #[test]
    fn unavailable_refs_are_marked_in_the_brief() {
        let mut input = input();
        input.refs[0].unavailable = Some("文件不存在".into());
        let text = render(&input);
        assert!(
            text.contains("- `docs/设计 稿.md`（定位：#登录）：/ws/docs/设计 稿.md（文件不存在）"),
            "{text}"
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

    #[test]
    fn summary_strips_minimax_display_markup() {
        let raw = "已完成需求梳理。\n未执行任何命令。\n\n<deliver-assets>\n<media type=\"file\" src=\"/abs/path/summary.md\" caption=\"WA-001 需求总结\" />\n</deliver-assets>";
        assert_eq!(
            strip_display_markup(raw),
            "已完成需求梳理。\n未执行任何命令。"
        );
        assert_eq!(
            summarize(raw).as_deref(),
            Some("已完成需求梳理。\n未执行任何命令。")
        );
    }

    #[test]
    fn summary_keeps_plain_angle_brackets() {
        let raw = "检查条件：x < 5 并且 y > 3\n代码里 a <b 不是标签\n<T> 泛型示例";
        assert_eq!(strip_display_markup(raw), raw);
    }

    #[test]
    fn summary_keeps_original_when_only_markup() {
        let raw = "  <deliver-assets>\n<media type=\"file\" src=\"/a.md\" />\n</deliver-assets>  ";
        assert_eq!(
            strip_display_markup(raw),
            "<deliver-assets>\n<media type=\"file\" src=\"/a.md\" />\n</deliver-assets>"
        );
        assert_eq!(
            strip_display_markup("<media type=\"file\" />"),
            "<media type=\"file\" />"
        );
    }

    #[test]
    fn summary_strips_multiple_markup_blocks() {
        let raw = "第一段\n<deliver-assets>\n<media src=\"a.md\" />\n</deliver-assets>\n第二段\n\n\n<media type=\"file\" src=\"/b.md\" />\n<notes>\n内部备注\n</notes>\n结尾";
        assert_eq!(strip_display_markup(raw), "第一段\n第二段\n\n结尾");
    }
}
