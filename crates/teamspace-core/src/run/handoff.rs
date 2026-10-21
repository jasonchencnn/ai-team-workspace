//! 岗位启动包（交接节）：岗位换绑到新的 Harness 后，新环境第一次执行时随任务说明包附上的
//! 交接内容——岗位说明、未完成任务、最近几次执行的结果、工作日志末尾与相关文件引用。
//!
//! 纯函数：内容由 [`HandoffInput`] 给出，渲染与长度控制都在这里（便于测试）。是否需要交接由
//! [`crate::service::handoff`] 判断（要看该岗位的执行历史），本模块不碰数据库。

/// 交接节的字符上限。超出时按「文件引用 → 日志 → 执行摘要 → 任务列表」的顺序截断，末尾标注「已截断」。
pub const HANDOFF_LIMIT: usize = 12_000;
/// 未完成任务最多列出条数。
pub const OPEN_TASK_LIMIT: usize = 20;
/// 近期执行结果最多列出条数。
pub const RECENT_RUN_LIMIT: usize = 5;
/// 工作日志最多附上的行数（取文件末尾的行）。
pub const JOURNAL_LINES: usize = 40;
/// 单条执行结果里摘要首段的字符上限。
pub const SUMMARY_LIMIT: usize = 200;

/// 交接节的小标题。
pub const HEADING: &str = "## 岗位交接";
/// 截断标记（放在末尾）。
pub const TRUNCATED_MARK: &str = "（已截断）";

/// 一个未完成任务。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HandoffTask {
    /// 任务编号（如 `FE-002`）。
    pub key: String,
    pub title: String,
    /// 状态显示名（如「执行中」）。
    pub status: String,
}

/// 一次已结束的执行。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HandoffRun {
    /// 任务编号。
    pub task_key: String,
    /// 执行结果（如「成功」「失败」）。
    pub status: String,
    /// 开始时间（本地时间）。
    pub at: String,
    /// 摘要首段。
    pub summary: String,
}

/// 交接内容的全部素材。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct HandoffInput {
    /// 岗位职责。
    pub role_description: String,
    /// 工作约定。
    pub role_guidelines: String,
    /// 未完成任务（按编号升序）。
    pub open_tasks: Vec<HandoffTask>,
    /// 最近几次执行（最新的在前）。
    pub recent_runs: Vec<HandoffRun>,
    /// 工作日志末尾的行（文件中旧的在前）。
    pub journal: Vec<String>,
    /// 未完成任务引用的文件路径（已去重）。
    pub files: Vec<String>,
}

/// 渲染过程中的可变部分：四组条目各自可以按优先级丢弃。
struct Groups {
    tasks: Vec<String>,
    runs: Vec<String>,
    journal: Vec<String>,
    files: Vec<String>,
    /// 下面四组是否因为长度限制被丢弃过内容。
    tasks_cut: bool,
    runs_cut: bool,
    journal_cut: bool,
    files_cut: bool,
}

impl Groups {
    fn from(input: &HandoffInput) -> Self {
        Self {
            tasks: input
                .open_tasks
                .iter()
                .map(|t| format!("- {} {}（{}）", t.key, t.title, t.status))
                .collect(),
            runs: input
                .recent_runs
                .iter()
                .map(|r| {
                    let mut line = format!("- {} · {} · {}", r.task_key, r.status, r.at);
                    if !r.summary.is_empty() {
                        line.push_str(&format!(" · {}", r.summary));
                    }
                    line
                })
                .collect(),
            journal: input
                .journal
                .iter()
                .map(|line| format!("- {line}"))
                .collect(),
            files: input
                .files
                .iter()
                .map(|path| format!("- `{path}`"))
                .collect(),
            tasks_cut: false,
            runs_cut: false,
            journal_cut: false,
            files_cut: false,
        }
    }

    /// 按「文件引用 → 日志 → 执行摘要 → 任务列表」的顺序丢掉一条内容。
    ///
    /// 返回 `false` 表示已经无处可丢。文件引用与任务列表丢末尾的（保留靠前的），
    /// 日志丢开头的（保留最近写入的），执行结果丢末尾的（保留最近几次）。
    fn drop_next(&mut self) -> bool {
        if !self.files.is_empty() {
            self.files.pop();
            self.files_cut = true;
            return true;
        }
        if !self.journal.is_empty() {
            self.journal.remove(0);
            self.journal_cut = true;
            return true;
        }
        if !self.runs.is_empty() {
            self.runs.pop();
            self.runs_cut = true;
            return true;
        }
        if !self.tasks.is_empty() {
            self.tasks.pop();
            self.tasks_cut = true;
            return true;
        }
        false
    }
}

fn or_none(text: &str) -> &str {
    if text.trim().is_empty() {
        "（未填写）"
    } else {
        text.trim()
    }
}

/// 一组条目：空时给出占位说明（被截断掉的写「（略）」，本来就没有的写「（无）」）。
fn push_group(out: &mut String, items: &[String], cut: bool) {
    if items.is_empty() {
        out.push_str(if cut { "（略）" } else { "（无）" });
        out.push('\n');
        return;
    }
    for item in items {
        out.push_str(item);
        out.push('\n');
    }
}

/// 拼出完整的交接节。
fn compose(input: &HandoffInput, groups: &Groups) -> String {
    let mut out = String::new();
    out.push_str(HEADING);
    out.push_str("\n\n");
    out.push_str("这个岗位刚接手到新的执行环境，下面是接手前的上下文。\n\n");
    out.push_str("### 岗位说明\n\n");
    out.push_str(&format!("职责：{}\n\n", or_none(&input.role_description)));
    out.push_str("工作约定：\n");
    out.push_str(or_none(&input.role_guidelines));
    out.push('\n');
    out.push_str("\n### 未完成任务\n\n");
    push_group(&mut out, &groups.tasks, groups.tasks_cut);
    out.push_str("\n### 近期执行结果\n\n");
    push_group(&mut out, &groups.runs, groups.runs_cut);
    out.push_str("\n### 工作日志（最近部分）\n\n");
    push_group(&mut out, &groups.journal, groups.journal_cut);
    out.push_str("\n### 相关文件引用\n\n");
    push_group(&mut out, &groups.files, groups.files_cut);
    out
}

/// 渲染交接节，长度不超过 `limit` 个字符。
///
/// 超出上限时按「文件引用 → 日志 → 执行摘要 → 任务列表」的顺序丢内容，末尾标注「已截断」。
/// 上限是常量 [`HANDOFF_LIMIT`]，测试里可以传更小的值。
pub fn render(input: &HandoffInput, limit: usize) -> String {
    let mut groups = Groups::from(input);
    let mut cut = false;
    loop {
        let text = compose(input, &groups);
        let mut full = text.clone();
        if cut {
            full.push('\n');
            full.push_str(TRUNCATED_MARK);
            full.push('\n');
        }
        if full.chars().count() <= limit {
            return full.trim_end().to_owned();
        }
        if !groups.drop_next() {
            break;
        }
        cut = true;
    }
    // 固定部分本身就超限：按字符边界截断，并尽量留下截断标记。
    let text = compose(input, &groups);
    let mark_len = TRUNCATED_MARK.chars().count();
    if limit <= mark_len {
        return take_chars(&text, limit);
    }
    let keep = limit - mark_len - 1;
    let mut out = take_chars(&text, keep);
    out.push('\n');
    out.push_str(TRUNCATED_MARK);
    out
}

/// 取前 `limit` 个字符（不切开代理对）。
fn take_chars(text: &str, limit: usize) -> String {
    text.chars().take(limit).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn input() -> HandoffInput {
        HandoffInput {
            role_description: "负责页面".into(),
            role_guidelines: "先写测试".into(),
            open_tasks: vec![
                HandoffTask {
                    key: "FE-002".into(),
                    title: "登录页".into(),
                    status: "执行中".into(),
                },
                HandoffTask {
                    key: "FE-003".into(),
                    title: "注册页".into(),
                    status: "TODO".into(),
                },
            ],
            recent_runs: vec![HandoffRun {
                task_key: "FE-001".into(),
                status: "成功".into(),
                at: "2026-10-13 10:20:00 UTC+08:00".into(),
                summary: "做完了登录页".into(),
            }],
            journal: vec!["2026-10-13 10:00 · FE-001 · 开始".into()],
            files: vec!["docs/spec.md".into()],
        }
    }

    #[test]
    fn section_contains_every_part() {
        let text = render(&input(), HANDOFF_LIMIT);
        assert!(text.starts_with(HEADING), "{text}");
        for needle in [
            "### 岗位说明",
            "职责：负责页面",
            "工作约定：",
            "先写测试",
            "### 未完成任务",
            "FE-002 登录页（执行中）",
            "FE-003 注册页（TODO）",
            "### 近期执行结果",
            "FE-001 · 成功 · 2026-10-13 10:20:00 UTC+08:00 · 做完了登录页",
            "### 工作日志（最近部分）",
            "2026-10-13 10:00 · FE-001 · 开始",
            "### 相关文件引用",
            "`docs/spec.md`",
        ] {
            assert!(text.contains(needle), "缺少 {needle}：\n{text}");
        }
        assert!(!text.contains(TRUNCATED_MARK), "{text}");
    }

    #[test]
    fn empty_role_still_produces_a_section() {
        let text = render(&HandoffInput::default(), HANDOFF_LIMIT);
        assert!(text.starts_with(HEADING), "{text}");
        assert!(text.contains("职责：（未填写）"), "{text}");
        assert!(text.contains("### 未完成任务\n\n（无）"), "{text}");
        assert!(text.contains("### 相关文件引用\n\n（无）"), "{text}");
    }

    #[test]
    fn truncation_drops_in_the_documented_order() {
        // 上限从大到小：内容按「文件引用 → 日志 → 执行摘要 → 任务列表」的顺序消失。
        let full = render(&input(), HANDOFF_LIMIT).chars().count();
        assert!(!render(&input(), HANDOFF_LIMIT).contains(TRUNCATED_MARK));
        let mut order: Vec<&str> = Vec::new();
        for limit in (0..=full).rev() {
            let text = render(&input(), limit);
            assert!(text.chars().count() <= limit, "{}", text.chars().count());
            for (part, needle) in [
                ("文件引用", "docs/spec.md"),
                ("日志", "FE-001 · 开始"),
                ("执行摘要", "做完了登录页"),
                ("任务列表", "注册页"),
            ] {
                if !text.contains(needle) && !order.contains(&part) {
                    order.push(part);
                }
            }
        }
        assert_eq!(order, vec!["文件引用", "日志", "执行摘要", "任务列表"]);
    }

    #[test]
    fn truncation_marks_the_end_and_keeps_the_tasks_longer() {
        // 上限小到只剩固定部分与任务列表：文件引用、日志与执行结果都被丢掉，末尾标注已截断。
        let text = render(&input(), 200);
        assert!(text.chars().count() <= 200, "{}", text.chars().count());
        assert!(text.contains(TRUNCATED_MARK), "{text}");
        assert!(!text.contains("docs/spec.md"), "{text}");
        assert!(!text.contains("FE-001 · 开始"), "{text}");
        assert!(!text.contains("做完了登录页"), "{text}");
        // 任务列表最后才丢。
        assert!(text.contains("FE-002"), "{text}");
    }

    #[test]
    fn truncation_keeps_the_most_recent_journal_lines() {
        let mut input = input();
        input.journal = (0..40).map(|i| format!("第 {i} 行")).collect();
        // 额度只够放一部分：日志开头的（最旧的）先被丢掉。
        let text = render(&input, 300);
        assert!(text.chars().count() <= 300, "{}", text.chars().count());
        assert!(text.contains(TRUNCATED_MARK), "{text}");
        assert!(text.contains("第 39 行"), "{text}");
        assert!(!text.contains("第 0 行"), "{text}");
    }

    #[test]
    fn tiny_limit_still_returns_bounded_text() {
        let text = render(&input(), 20);
        assert!(text.chars().count() <= 20, "{}", text.chars().count());
    }

    #[test]
    fn limit_is_not_exceeded_without_truncation() {
        let text = render(&input(), HANDOFF_LIMIT);
        assert!(text.chars().count() <= HANDOFF_LIMIT);
    }
}
