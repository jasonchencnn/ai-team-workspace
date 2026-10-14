//! 岗位文件夹（D14）的文件系统部分：`<工作空间>/Teamspace/<岗位文件夹>/`。
//!
//! 每个岗位文件夹包含：
//! - `岗位说明.md`：由 Teamspace 生成，岗位信息或绑定变化时整体重写，用户不应手改；
//! - `工作日志.md`：只追加，记录与岗位相关的事件和执行摘要（[`append_journal`]）；Harness 执行时也可以
//!   在末尾追加进展（D14，经 [`write_journal_from_agent`]，只接受纯追加）；
//! - `过程文件/`：Harness 执行时的过程产物和用户手写内容，Teamspace 只放一个 `README.md`。
//!
//! 本模块只做文件系统操作、不访问数据库，由 `api` 层在数据库锁之外调用（M1 审查 #2 原则）。
//! 规则：
//! - 文件夹名为岗位名的文件名安全形式（[`sanitize_name`]）；与其他岗位（同一工作空间内任何团队，
//!   含已归档岗位）的文件夹或磁盘上不属于本岗位的同名条目冲突时，依次尝试
//!   `名称（岗位标识）`、`名称（岗位标识）2`…（见 [`candidate`]）。
//! - 是否「属于本岗位」以 `岗位说明.md` 中的隐藏标记 `teamspace-role-id: <id>` 判断。
//! - 只创建、重写本模块生成的文件，只重命名岗位自己的文件夹；从不删除任何文件。

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use chrono::{DateTime, Local, TimeZone};

use crate::error::{Error, Result};
use crate::model::{AcpPreset, Role, RoleFileEntry};

/// 工作空间下的岗位工作区根目录名。
pub const ROOT_DIR: &str = "Teamspace";
/// 岗位说明文件名。
pub const BRIEF_FILE: &str = "岗位说明.md";
/// 工作日志文件名。
pub const JOURNAL_FILE: &str = "工作日志.md";
/// 过程文件目录名。
pub const PROCESS_DIR: &str = "过程文件";
/// 说明文件名（`Teamspace/` 根目录与 `过程文件/` 各一个）。
pub const README_FILE: &str = "README.md";
/// `get_role_folder` 默认返回的日志条数。
pub const DEFAULT_JOURNAL_LIMIT: u32 = 20;
/// `get_role_folder` 最多返回的日志条数。
pub const MAX_JOURNAL_LIMIT: u32 = 200;

/// 岗位说明里标记所属岗位的隐藏注释键。
const ROLE_ID_MARKER: &str = "teamspace-role-id:";
/// 文件夹名（不含冲突后缀）的字节上限；常见文件系统单个路径段上限 255 字节，留出后缀空间。
const MAX_NAME_BYTES: usize = 150;
/// 冲突时最多尝试的候选名数量。
const MAX_CANDIDATES: usize = 100;

const ROOT_README: &str = "# Teamspace 岗位工作区\n\n\
这个目录由 Teamspace 创建和维护，每个子文件夹对应团队中的一个岗位：\n\n\
- `岗位说明.md`：岗位信息与当前绑定的 Harness，由 Teamspace 自动生成，请勿手改；\n\
- `工作日志.md`：与该岗位相关的派工、状态变化等记录，只追加；\n\
- `过程文件/`：执行过程中的产物和手写笔记。\n\n\
更换 Harness 时，新执行者先读对应岗位的文件夹即可了解岗位进程。\
Teamspace 不会删除这里的任何文件；岗位归档后文件夹保留。\n";

const PROCESS_README: &str = "# 过程文件\n\n\
存放该岗位执行任务时的过程产物（草稿、调研记录、中间结果等）和手写笔记。\n\n\
Teamspace 不会修改或删除这里的文件。`岗位说明.md` 会被自动重写，手写内容请放在这里。\n";

// ---------------------------------------------------------------- 命名

/// 岗位名的文件名安全形式：`/ \ : * ? " < > |` 和控制字符替换为 `_`，去掉首尾空白和点，
/// 过长时按字符截断到 [`MAX_NAME_BYTES`] 字节以内；结果为空时用岗位标识 `code`。
pub fn sanitize_name(name: &str, code: &str) -> String {
    let replaced: String = name
        .chars()
        .map(|c| {
            if c.is_control() || matches!(c, '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|') {
                '_'
            } else {
                c
            }
        })
        .collect();
    let mut out = String::new();
    for c in replaced
        .trim_matches(|c: char| c.is_whitespace() || c == '.')
        .chars()
    {
        if out.len() + c.len_utf8() > MAX_NAME_BYTES {
            break;
        }
        out.push(c);
    }
    let out = out.trim_end_matches(|c: char| c.is_whitespace() || c == '.');
    if out.is_empty() {
        code.to_owned()
    } else {
        out.to_owned()
    }
}

/// 第 `n` 个候选文件夹名：`0` 为 `base`，`1` 为 `base（code）`，之后为 `base（code）n`。
pub fn candidate(base: &str, code: &str, n: usize) -> String {
    match n {
        0 => base.to_owned(),
        1 => format!("{base}（{code}）"),
        n => format!("{base}（{code}）{n}"),
    }
}

/// 文件夹名是否仍对应岗位当前名称（`base` 本身或带冲突后缀的形式），即无需改名。
pub(crate) fn name_matches(dir_name: &str, base: &str, code: &str) -> bool {
    dir_name == base || dir_name.starts_with(&format!("{base}（{code}）"))
}

/// 岗位文件夹相对工作空间的路径（`/` 分隔），如 `Teamspace/前端开发`。
pub(crate) fn relative(dir_name: &str) -> String {
    format!("{ROOT_DIR}/{dir_name}")
}

/// 相对路径的最后一段（文件夹名）。
fn dir_name(rel: &str) -> &str {
    rel.rsplit('/').next().unwrap_or(rel)
}

/// 工作空间内的绝对路径。
pub(crate) fn absolute(workspace: &Path, rel: &str) -> PathBuf {
    rel.split('/')
        .fold(workspace.to_path_buf(), |path, part| path.join(part))
}

/// `dir` 是否是 `role_id` 的岗位文件夹（岗位说明中带有该岗位的标记）。
fn owned_by(dir: &Path, role_id: &str) -> bool {
    fs::read_to_string(dir.join(BRIEF_FILE))
        .map(|text| text.contains(&format!("{ROLE_ID_MARKER} {role_id} ")))
        .unwrap_or(false)
}

/// 候选路径是否可用：不与其他岗位登记的文件夹重名（不区分大小写），磁盘上不存在或就是本岗位的文件夹。
fn is_free(workspace: &Path, rel: &str, role_id: &str, others: &[String]) -> bool {
    let lower = rel.to_lowercase();
    if others.iter().any(|o| o.to_lowercase() == lower) {
        return false;
    }
    let path = absolute(workspace, rel);
    fs::symlink_metadata(&path).is_err() || (path.is_dir() && owned_by(&path, role_id))
}

/// 改名前的检查（在锁外调用）：目标文件夹已被其他岗位登记，或磁盘上已有不属于本岗位的同名条目时为
/// conflict。工作空间不可用时不阻止改名（文件夹问题由同步记录）。
pub(crate) fn check_rename_target(
    workspace: &Path,
    role_id: &str,
    new_name: &str,
    target: &str,
    others: &[String],
) -> Result<()> {
    if !workspace.is_dir() || is_free(workspace, target, role_id, others) {
        return Ok(());
    }
    Err(Error::conflict(format!(
        "工作空间中已有文件夹「{target}」，无法把岗位改名为「{new_name}」；请换一个名称，或先移走该文件夹"
    )))
}

// ---------------------------------------------------------------- 时间与日志条目

/// 把存储的 UTC 时间转为本地可读时间，如 `2026-10-13 14:05:01 UTC+08:00`；无法解析时原样返回。
pub fn local_time(utc: &str) -> String {
    format_time(utc, &Local)
}

fn format_time<Tz: TimeZone>(utc: &str, tz: &Tz) -> String
where
    Tz::Offset: std::fmt::Display,
{
    match DateTime::parse_from_rfc3339(utc) {
        Ok(t) => t
            .with_timezone(tz)
            .format("%Y-%m-%d %H:%M:%S UTC%:z")
            .to_string(),
        Err(_) => utc.to_owned(),
    }
}

/// 一条工作日志。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JournalEntry {
    /// UTC RFC3339 时间（写入时转为本地时间）。
    pub at: String,
    /// 相关任务编号，如 `FE-001`。
    pub task_key: Option<String>,
    /// 中文描述；换行会被替换为空格，保证一条日志一行。
    pub text: String,
}

impl JournalEntry {
    /// 日志里的正文：本地时间 · 任务编号 · 描述（无任务时省略编号）。界面展示用，不含行首的 `- `。
    pub fn display(&self) -> String {
        let text: String = self
            .text
            .chars()
            .map(|c| if c.is_control() { ' ' } else { c })
            .collect();
        match &self.task_key {
            Some(key) => format!("{} · {key} · {}", local_time(&self.at), text.trim()),
            None => format!("{} · {}", local_time(&self.at), text.trim()),
        }
    }

    /// 日志文件中的一行：`- 本地时间 · 任务编号 · 描述`（无任务时省略编号）。
    fn line(&self) -> String {
        format!("- {}\n", self.display())
    }
}

/// 工作日志写入锁：Teamspace 自己的追加（[`append_journal`]）与 Agent 经 `fs/write_text_file` 的写入
/// （[`write_journal_from_agent`]）在这把锁内进行，保证「重新读取 → 判断 → 追加」不被另一方插入。
/// 两边追加都用 `O_APPEND`，即使绕过锁也只会改变行的先后，不会互相覆盖。
static JOURNAL_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

fn journal_lock() -> std::sync::MutexGuard<'static, ()> {
    JOURNAL_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

fn append_text(path: &Path, text: &str) -> std::io::Result<()> {
    let mut file = fs::OpenOptions::new().append(true).open(path)?;
    file.write_all(text.as_bytes())?;
    file.flush()
}

/// 向岗位文件夹 `folder` 的工作日志追加条目（只追加，不改已有内容）。
///
/// 日志文件须已存在（由 Teamspace 同步岗位文件夹时创建）。在 [`JOURNAL_LOCK`] 内以 `O_APPEND` 写入，
/// 与 Agent 追加的进展交错时双方的行都保留。
pub fn append_journal(folder: &Path, entries: &[JournalEntry]) -> std::io::Result<()> {
    if entries.is_empty() {
        return Ok(());
    }
    let text: String = entries.iter().map(JournalEntry::line).collect();
    let _guard = journal_lock();
    append_text(&folder.join(JOURNAL_FILE), &text)
}

/// Agent 写工作日志（ACP `fs/write_text_file` 是整文件写入）的结果。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AgentJournalWrite {
    /// 新内容以磁盘上的现有内容为前缀：只把多出的部分追加到文件末尾（字节数）。
    Appended(usize),
    /// 不是纯追加，但 Human 已允许：按新内容整体写入。
    Overwritten,
    /// 不是纯追加（覆盖、删改、截断），且没有 Human 的允许；或日志文件不存在：未写入。
    NotAppend,
}

/// 处理 Agent 对工作日志 `path` 的整文件写入 `content`（D14）。
///
/// 在 [`JOURNAL_LOCK`] 内重新读取磁盘内容：`content` 以它为前缀时只追加多出的部分（`O_APPEND`），
/// 因此不会冲掉 Teamspace 在 Agent 读取之后追加的行——那种情况下 `content` 不再以磁盘内容为前缀，
/// 判为非追加。非追加的写入仅在 `allow_overwrite`（Human 已允许）且文件存在时整体写入。
pub fn write_journal_from_agent(
    path: &Path,
    content: &str,
    allow_overwrite: bool,
) -> std::io::Result<AgentJournalWrite> {
    let _guard = journal_lock();
    let current = match fs::read_to_string(path) {
        Ok(text) => Some(text),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => None,
        Err(err) => return Err(err),
    };
    if let Some(suffix) = current.as_deref().and_then(|c| content.strip_prefix(c)) {
        if !suffix.is_empty() {
            append_text(path, suffix)?;
        }
        return Ok(AgentJournalWrite::Appended(suffix.len()));
    }
    // 日志文件不存在时不由 Agent 重建（即使 Human 允许过改写），由 Teamspace 同步时创建。
    if allow_overwrite && current.is_some() {
        fs::write(path, content)?;
        return Ok(AgentJournalWrite::Overwritten);
    }
    Ok(AgentJournalWrite::NotAppend)
}

/// 读取工作日志最近的 `limit` 条（以 `- ` 开头的行），最新的在前，去掉行首的 `- `。
pub fn read_recent_journal(folder: &Path, limit: usize) -> std::io::Result<Vec<String>> {
    let text = fs::read_to_string(folder.join(JOURNAL_FILE))?;
    Ok(text
        .lines()
        .rev()
        .filter_map(|line| line.strip_prefix("- "))
        .take(limit)
        .map(str::to_owned)
        .collect())
}

// ---------------------------------------------------------------- 生成内容

/// 岗位说明里显示的绑定信息。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct BoundHarness {
    pub name: String,
    pub preset: AcpPreset,
    pub command: String,
    pub args: Vec<String>,
}

fn section(body: &str) -> String {
    if body.trim().is_empty() {
        "（未填写）".to_owned()
    } else {
        body.trim().to_owned()
    }
}

/// 岗位说明全文。内容只取决于岗位数据，未变化时不重写文件。
///
/// 启动参数里的疑似密钥在此脱敏（D6）：岗位说明会留在工作空间里，可能被提交进 Git。
pub(crate) fn render_brief(plan: &RolePlan) -> String {
    let role = &plan.role;
    let harness = match &plan.harness {
        Some(h) => format!(
            "{}（{} · ACP，启动命令 `{}`）",
            h.name,
            h.preset.label(),
            crate::acp::command_line(Path::new(&h.command), &crate::redact::args(&h.args))
        ),
        None => "未绑定（暂不派工）".to_owned(),
    };
    let status = match &role.archived_at {
        Some(at) => format!("已归档（{}），文件夹保留供查阅", local_time(at)),
        None => "在岗".to_owned(),
    };
    format!(
        "<!-- 由 Teamspace 自动生成，请勿手改；手写内容请放到 过程文件/。{ROLE_ID_MARKER} {id} -->\n\
         # 岗位说明：{name}\n\n\
         > 由 Teamspace 自动生成，请勿手改；手写内容请放到 `过程文件/`。\n\n\
         - 岗位名：{name}\n\
         - 岗位标识：`{code}`\n\
         - 任务前缀：{prefix}\n\
         - 是否 Leader：{leader}\n\
         - 所属团队：{team}\n\
         - 状态：{status}\n\
         - 当前绑定 Harness：{harness}\n\
         - 更新时间：{updated}\n\n\
         ## 职责\n\n{description}\n\n\
         ## 工作约定\n\n{guidelines}\n\n\
         ## 给新执行者\n\n\
         先读同目录的 `工作日志.md` 了解岗位进程；过程产物和手写笔记放在 `过程文件/`。\n",
        id = role.id,
        name = role.name,
        code = role.code,
        prefix = role.task_prefix,
        leader = if role.is_leader { "是" } else { "否" },
        team = plan.team_name,
        updated = local_time(&role.updated_at),
        description = section(&role.description),
        guidelines = section(&role.guidelines),
    )
}

fn journal_header(role: &Role) -> String {
    format!(
        "# 工作日志\n\n\
         > 岗位标识 `{}`。由 Teamspace 自动追加，只增不改；时间为本地时间。\n\n",
        role.code
    )
}

// ---------------------------------------------------------------- 同步

/// 同步一个岗位文件夹所需的数据（在锁内从数据库读出，锁外使用）。
#[derive(Debug, Clone)]
pub(crate) struct RolePlan {
    pub role: Role,
    pub team_name: String,
    /// 绑定的 Harness 名称、预设与启动命令。
    pub harness: Option<BoundHarness>,
    /// 已登记的文件夹相对路径。
    pub folder: Option<String>,
    /// 上次记录的文件夹问题。
    pub problem: Option<String>,
    /// 已写入日志的最后一条活动序号。
    pub journaled_seq: i64,
    /// 同一工作空间中其他岗位登记的文件夹。
    pub others: Vec<String>,
    /// 待追加的日志（活动序号，条目），按序号升序。
    pub entries: Vec<(i64, JournalEntry)>,
}

/// 一次同步的结果，由调用方写回数据库。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SyncOutcome {
    pub folder: Option<String>,
    pub journaled_seq: i64,
    pub problem: Option<String>,
}

fn io_problem(what: &str, path: &Path, err: std::io::Error) -> String {
    format!("无法写入{what}「{}」：{err}", path.display())
}

/// 创建目录（含上级目录）。
fn ensure_dir(path: &Path, what: &str) -> std::result::Result<(), String> {
    fs::create_dir_all(path).map_err(|err| io_problem(what, path, err))
}

/// 文件不存在时写入初始内容；已存在则不动。
fn ensure_file(path: &Path, content: &str, what: &str) -> std::result::Result<(), String> {
    match fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
    {
        Ok(mut file) => file
            .write_all(content.as_bytes())
            .map_err(|err| io_problem(what, path, err)),
        Err(err) if err.kind() == std::io::ErrorKind::AlreadyExists => Ok(()),
        Err(err) => Err(io_problem(what, path, err)),
    }
}

/// 同步一个岗位文件夹：必要时建立或改名，重写变化了的岗位说明，补齐过程文件目录，追加待写日志。
/// 失败不 panic，原因写入结果的 `problem`。
///
/// 已归档岗位只维护仍然存在的文件夹，不会重新创建被用户移走的文件夹。
pub(crate) fn sync(workspace: &Path, plan: &RolePlan) -> SyncOutcome {
    let mut out = SyncOutcome {
        folder: plan.folder.clone(),
        journaled_seq: plan.journaled_seq,
        problem: None,
    };
    if let Err(problem) = sync_inner(workspace, plan, &mut out) {
        out.problem = Some(problem);
    }
    out
}

fn sync_inner(
    workspace: &Path,
    plan: &RolePlan,
    out: &mut SyncOutcome,
) -> std::result::Result<(), String> {
    let role = &plan.role;
    if !workspace.is_dir() {
        return Err(format!(
            "工作空间目录不存在或不是目录：{}",
            workspace.display()
        ));
    }
    let current = plan
        .folder
        .as_deref()
        .filter(|rel| absolute(workspace, rel).is_dir());
    if role.archived_at.is_some() && current.is_none() {
        return Ok(());
    }
    let root = workspace.join(ROOT_DIR);
    ensure_dir(&root, "岗位工作区目录")?;
    ensure_file(&root.join(README_FILE), ROOT_README, "说明文件")?;

    let base = sanitize_name(&role.name, &role.code);
    let rel = match current {
        Some(rel) if name_matches(dir_name(rel), &base, &role.code) => rel.to_owned(),
        Some(rel) => {
            let target = pick_free(workspace, &base, plan)?;
            let (from, to) = (absolute(workspace, rel), absolute(workspace, &target));
            fs::rename(&from, &to)
                .map_err(|err| format!("无法把岗位文件夹「{rel}」改名为「{target}」：{err}"))?;
            target
        }
        None => {
            let target = pick_free(workspace, &base, plan)?;
            ensure_dir(&absolute(workspace, &target), "岗位文件夹")?;
            target
        }
    };
    out.folder = Some(rel.clone());
    let dir = absolute(workspace, &rel);

    let brief = render_brief(plan);
    let brief_path = dir.join(BRIEF_FILE);
    if fs::read_to_string(&brief_path).ok().as_deref() != Some(brief.as_str()) {
        fs::write(&brief_path, &brief).map_err(|err| io_problem("岗位说明", &brief_path, err))?;
    }

    let process = dir.join(PROCESS_DIR);
    ensure_dir(&process, "过程文件目录")?;
    ensure_file(&process.join(README_FILE), PROCESS_README, "说明文件")?;

    let journal_path = dir.join(JOURNAL_FILE);
    if fs::symlink_metadata(&journal_path).is_err() {
        let created = JournalEntry {
            at: role.created_at.clone(),
            task_key: None,
            text: format!("创建岗位「{}」（任务前缀 {}）", role.name, role.task_prefix),
        };
        let content = format!("{}{}", journal_header(role), created.line());
        ensure_file(&journal_path, &content, "工作日志")?;
    }
    if let Some((last, _)) = plan.entries.last() {
        let entries: Vec<JournalEntry> = plan.entries.iter().map(|(_, e)| e.clone()).collect();
        append_journal(&dir, &entries).map_err(|err| io_problem("工作日志", &journal_path, err))?;
        out.journaled_seq = *last;
    }
    Ok(())
}

/// 第一个可用的候选文件夹（相对路径）。
fn pick_free(workspace: &Path, base: &str, plan: &RolePlan) -> std::result::Result<String, String> {
    (0..MAX_CANDIDATES)
        .map(|n| relative(&candidate(base, &plan.role.code, n)))
        .find(|rel| is_free(workspace, rel, &plan.role.id, &plan.others))
        .ok_or_else(|| format!("找不到可用的岗位文件夹名（「{base}」的候选名都已被占用）"))
}

// ---------------------------------------------------------------- 工作台：最近文件与路径校验

/// 「最近文件」扫描的文件数上限，避免过程文件很多时一次请求读遍整棵目录。
const MAX_SCANNED_PROCESS_FILES: usize = 5_000;

/// `过程文件/` 下最近修改的 `limit` 个文件（递归），按修改时间倒序、同一时间按路径升序。
///
/// 跳过以 `.` 开头的文件和目录、符号链接（不跟随，避免越出岗位文件夹）以及 Teamspace 生成的
/// `过程文件/README.md`；读不动的子目录跳过。目录不存在时返回空列表。
pub fn recent_process_files(folder: &Path, limit: usize) -> Vec<RoleFileEntry> {
    let mut found = Vec::new();
    let mut scanned = 0usize;
    let mut stack = vec![folder.join(PROCESS_DIR)];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            if scanned >= MAX_SCANNED_PROCESS_FILES {
                break;
            }
            if entry.file_name().to_string_lossy().starts_with('.') {
                continue;
            }
            let Ok(kind) = entry.file_type() else {
                continue;
            };
            let path = entry.path();
            if kind.is_dir() {
                stack.push(path);
                continue;
            }
            if !kind.is_file() {
                continue;
            }
            scanned += 1;
            let rel = relative_to(folder, &path);
            if rel == format!("{PROCESS_DIR}/{README_FILE}") {
                continue;
            }
            let Ok(meta) = entry.metadata() else {
                continue;
            };
            let modified = meta.modified().unwrap_or(std::time::UNIX_EPOCH);
            found.push((modified, rel, meta.len()));
        }
    }
    found.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.cmp(&b.1)));
    found.truncate(limit);
    found
        .into_iter()
        .map(|(modified, relative_path, size)| RoleFileEntry {
            relative_path,
            size,
            modified_at: chrono::DateTime::<chrono::Utc>::from(modified)
                .format("%Y-%m-%dT%H:%M:%S%.3fZ")
                .to_string(),
        })
        .collect()
}

/// 相对 `root` 的 `/` 分隔路径。
fn relative_to(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .components()
        .map(|c| c.as_os_str().to_string_lossy())
        .collect::<Vec<_>>()
        .join("/")
}

/// 把岗位文件夹内的相对路径解析为磁盘上的真实路径，确保它在岗位文件夹之内。
///
/// `relative` 为空时就是岗位文件夹本身。拒绝绝对路径、`..`、控制字符（invalid）；目标不存在为
/// not_found；规范化（解析符号链接）后不在岗位文件夹内的同样拒绝（invalid），因此指向文件夹外的
/// 符号链接无法被打开。
pub fn resolve_inside(folder: &Path, relative: Option<&str>) -> Result<PathBuf> {
    let base = folder
        .canonicalize()
        .map_err(|err| Error::not_found(format!("岗位文件夹不可用：{err}")))?;
    let relative = relative.map(str::trim).unwrap_or_default();
    if relative.is_empty() {
        return Ok(base);
    }
    if relative.chars().any(char::is_control) {
        return Err(Error::invalid("路径包含不允许的控制字符"));
    }
    if relative.starts_with('/') || relative.starts_with('\\') || Path::new(relative).is_absolute()
    {
        return Err(Error::invalid(format!(
            "只能打开岗位文件夹内的相对路径：{relative}"
        )));
    }
    let mut target = base.clone();
    for part in relative.split(['/', '\\']) {
        match part {
            "" | "." => {}
            ".." => {
                return Err(Error::invalid(format!("路径不能包含「..」：{relative}")));
            }
            part => target.push(part),
        }
    }
    if fs::symlink_metadata(&target).is_err() {
        return Err(Error::not_found(format!("岗位文件夹中没有「{relative}」")));
    }
    let real = target
        .canonicalize()
        .map_err(|_| Error::not_found(format!("「{relative}」指向的位置不存在")))?;
    if !real.starts_with(&base) {
        return Err(Error::invalid(format!(
            "「{relative}」指向岗位文件夹之外，不能打开"
        )));
    }
    Ok(real)
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::FixedOffset;

    #[test]
    fn sanitizes_names() {
        assert_eq!(sanitize_name("前端开发", "role_fe"), "前端开发");
        assert_eq!(
            sanitize_name(" a/b\\c:d*e?f\"g<h>i|j ", "x"),
            "a_b_c_d_e_f_g_h_i_j"
        );
        assert_eq!(sanitize_name("a\u{7}b\tc", "x"), "a_b_c");
        assert_eq!(sanitize_name(" ..QA. ", "role_qa"), "QA");
        assert_eq!(sanitize_name("...", "role_qa"), "role_qa");
        assert_eq!(sanitize_name("  ", "role_qa"), "role_qa");
        let long = sanitize_name(&"长".repeat(100), "x");
        assert!(long.len() <= MAX_NAME_BYTES && long.chars().all(|c| c == '长'));
    }

    #[test]
    fn candidates_and_matching() {
        assert_eq!(candidate("QA", "role_qa", 0), "QA");
        assert_eq!(candidate("QA", "role_qa", 1), "QA（role_qa）");
        assert_eq!(candidate("QA", "role_qa", 3), "QA（role_qa）3");
        assert!(name_matches("QA", "QA", "role_qa"));
        assert!(name_matches("QA（role_qa）2", "QA", "role_qa"));
        assert!(!name_matches("QA 测试", "QA", "role_qa"));
        assert!(!name_matches("QA（role_x）", "QA", "role_qa"));
    }

    #[test]
    fn formats_time_with_offset() {
        let tz = FixedOffset::east_opt(8 * 3600).unwrap();
        assert_eq!(
            format_time("2026-10-13T06:05:01.262Z", &tz),
            "2026-10-13 14:05:01 UTC+08:00"
        );
        assert_eq!(format_time("bad", &tz), "bad");
    }

    #[test]
    fn journal_lines_are_single_line_and_read_newest_first() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join(JOURNAL_FILE), "# 工作日志\n\n").unwrap();
        let entry = |key: Option<&str>, text: &str| JournalEntry {
            at: "2026-10-13T06:05:01.262Z".into(),
            task_key: key.map(Into::into),
            text: text.into(),
        };
        append_journal(
            dir.path(),
            &[
                entry(None, "第一条"),
                entry(Some("FE-001"), "受阻：\n等待接口"),
            ],
        )
        .unwrap();
        append_journal(dir.path(), &[entry(None, "第三条")]).unwrap();
        let recent = read_recent_journal(dir.path(), 2).unwrap();
        assert_eq!(recent.len(), 2);
        assert!(recent[0].ends_with(" · 第三条"), "{recent:?}");
        assert!(
            recent[1].contains(" · FE-001 · 受阻： 等待接口"),
            "{recent:?}"
        );
        assert!(append_journal(&dir.path().join("missing"), &[entry(None, "x")]).is_err());
    }

    fn entry_at(text: &str) -> JournalEntry {
        JournalEntry {
            at: "2026-10-13T06:05:01.262Z".into(),
            task_key: Some("FE-001".into()),
            text: text.into(),
        }
    }

    #[test]
    fn agent_journal_writes_only_append() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(JOURNAL_FILE);
        fs::write(&path, "# 工作日志\n\n- a\n").unwrap();

        // 纯追加：只追加多出的部分。
        let out = write_journal_from_agent(&path, "# 工作日志\n\n- a\n- b\n", false).unwrap();
        assert_eq!(out, AgentJournalWrite::Appended(4));
        assert_eq!(
            fs::read_to_string(&path).unwrap(),
            "# 工作日志\n\n- a\n- b\n"
        );
        // 内容不变也算追加（空追加）。
        let same = fs::read_to_string(&path).unwrap();
        assert_eq!(
            write_journal_from_agent(&path, &same, false).unwrap(),
            AgentJournalWrite::Appended(0)
        );

        // 覆盖、截断、删改：不写入。
        for bad in [
            "全新内容\n",
            "# 工作日志\n\n- a\n",
            "# 工作日志\n\n- A\n- b\n- c\n",
            "",
        ] {
            assert_eq!(
                write_journal_from_agent(&path, bad, false).unwrap(),
                AgentJournalWrite::NotAppend,
                "{bad:?}"
            );
        }
        assert_eq!(
            fs::read_to_string(&path).unwrap(),
            "# 工作日志\n\n- a\n- b\n"
        );

        // 文件不存在：不算追加，也不创建。
        let missing = dir.path().join("其他.md");
        assert_eq!(
            write_journal_from_agent(&missing, "- x\n", false).unwrap(),
            AgentJournalWrite::NotAppend
        );
        assert!(!missing.exists());
        // 即使 Human 允许过改写，也不重建不存在的日志文件。
        assert_eq!(
            write_journal_from_agent(&missing, "- x\n", true).unwrap(),
            AgentJournalWrite::NotAppend
        );
        assert!(!missing.exists());

        // Human 已允许时整体写入。
        assert_eq!(
            write_journal_from_agent(&path, "重写\n", true).unwrap(),
            AgentJournalWrite::Overwritten
        );
        assert_eq!(fs::read_to_string(&path).unwrap(), "重写\n");
    }

    #[test]
    fn teamspace_and_agent_appends_interleave_without_loss() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(JOURNAL_FILE);
        fs::write(&path, "# 工作日志\n\n").unwrap();

        // Agent 读取后 Teamspace 追加：Agent 基于旧内容的写入被判为非追加，Teamspace 的行不丢。
        let seen = fs::read_to_string(&path).unwrap();
        append_journal(dir.path(), &[entry_at("开始第 1 次执行")]).unwrap();
        assert_eq!(
            write_journal_from_agent(&path, &format!("{seen}- 进展一\n"), false).unwrap(),
            AgentJournalWrite::NotAppend
        );
        // 重新读取后追加成功，之后 Teamspace 再追加。
        let seen = fs::read_to_string(&path).unwrap();
        write_journal_from_agent(&path, &format!("{seen}- 进展一\n"), false).unwrap();
        append_journal(dir.path(), &[entry_at("第 1 次执行完成")]).unwrap();
        let text = fs::read_to_string(&path).unwrap();
        let lines: Vec<&str> = text.lines().filter(|l| l.starts_with("- ")).collect();
        assert_eq!(lines.len(), 3, "{text}");
        assert!(lines[0].ends_with("FE-001 · 开始第 1 次执行"), "{text}");
        assert_eq!(lines[1], "- 进展一");
        assert!(lines[2].ends_with("FE-001 · 第 1 次执行完成"), "{text}");

        // 并发：两个线程交错写，所有行都在。
        let teamspace = {
            let folder = dir.path().to_path_buf();
            std::thread::spawn(move || {
                for i in 0..50 {
                    append_journal(&folder, &[entry_at(&format!("ts-{i}"))]).unwrap();
                }
            })
        };
        let agent = {
            let path = path.clone();
            std::thread::spawn(move || {
                for i in 0..50 {
                    loop {
                        let seen = fs::read_to_string(&path).unwrap();
                        let next = format!("{seen}- agent-{i}\n");
                        if let AgentJournalWrite::Appended(_) =
                            write_journal_from_agent(&path, &next, false).unwrap()
                        {
                            break;
                        }
                    }
                }
            })
        };
        teamspace.join().unwrap();
        agent.join().unwrap();
        let text = fs::read_to_string(&path).unwrap();
        for i in 0..50 {
            assert_eq!(text.matches(&format!(" · ts-{i}\n")).count(), 1, "ts-{i}");
            assert_eq!(
                text.matches(&format!("- agent-{i}\n")).count(),
                1,
                "agent-{i}"
            );
        }
    }

    fn set_mtime(path: &Path, secs: u64) {
        let file = fs::File::options().write(true).open(path).unwrap();
        file.set_modified(std::time::UNIX_EPOCH + std::time::Duration::from_secs(secs))
            .unwrap();
    }

    #[test]
    fn recent_files_are_sorted_by_mtime_and_limited() {
        let dir = tempfile::tempdir().unwrap();
        let process = dir.path().join(PROCESS_DIR);
        fs::create_dir_all(process.join("子目录/更深")).unwrap();
        fs::create_dir_all(process.join(".hidden")).unwrap();
        let files = [
            ("a.md", 100),
            ("b.md", 300),
            ("子目录/c.srt", 200),
            ("子目录/更深/d.prproj", 400),
            ("e.md", 300),
            (".secret", 999),
            (".hidden/x.md", 999),
            ("README.md", 999),
        ];
        for (rel, secs) in files {
            let path = process.join(rel);
            fs::write(&path, rel).unwrap();
            set_mtime(&path, secs);
        }
        std::os::unix::fs::symlink(dir.path().join("岗位说明.md"), process.join("link.md"))
            .unwrap();

        let recent = recent_process_files(dir.path(), 10);
        let paths: Vec<&str> = recent.iter().map(|f| f.relative_path.as_str()).collect();
        assert_eq!(
            paths,
            [
                "过程文件/子目录/更深/d.prproj",
                "过程文件/b.md",
                "过程文件/e.md",
                "过程文件/子目录/c.srt",
                "过程文件/a.md",
            ]
        );
        assert_eq!(recent[0].size, "子目录/更深/d.prproj".len() as u64);
        assert_eq!(recent[0].modified_at, "1970-01-01T00:06:40.000Z");

        assert_eq!(recent_process_files(dir.path(), 2).len(), 2);
        // 上限 10 条。
        for i in 0..15 {
            fs::write(process.join(format!("f{i}.txt")), "x").unwrap();
        }
        assert_eq!(recent_process_files(dir.path(), 10).len(), 10);
        assert!(recent_process_files(&dir.path().join("missing"), 10).is_empty());
    }

    #[test]
    fn resolve_inside_rejects_escapes() {
        let root = tempfile::tempdir().unwrap();
        let folder = root.path().join("Teamspace/剪辑师");
        fs::create_dir_all(folder.join(PROCESS_DIR)).unwrap();
        fs::write(folder.join(JOURNAL_FILE), "x").unwrap();
        fs::write(folder.join(PROCESS_DIR).join("a.md"), "x").unwrap();
        fs::write(root.path().join("secret.md"), "x").unwrap();
        std::os::unix::fs::symlink(root.path().join("secret.md"), folder.join("out.md")).unwrap();
        std::os::unix::fs::symlink(root.path(), folder.join(PROCESS_DIR).join("up")).unwrap();
        std::os::unix::fs::symlink(folder.join(JOURNAL_FILE), folder.join("in.md")).unwrap();
        let base = folder.canonicalize().unwrap();

        assert_eq!(resolve_inside(&folder, None).unwrap(), base);
        assert_eq!(resolve_inside(&folder, Some("  ")).unwrap(), base);
        assert_eq!(
            resolve_inside(&folder, Some(JOURNAL_FILE)).unwrap(),
            base.join(JOURNAL_FILE)
        );
        assert_eq!(
            resolve_inside(&folder, Some("过程文件/./a.md")).unwrap(),
            base.join("过程文件/a.md")
        );
        assert_eq!(
            resolve_inside(&folder, Some("in.md")).unwrap(),
            base.join(JOURNAL_FILE)
        );
        for bad in [
            "../../secret.md",
            "过程文件/../../secret.md",
            "/etc/hosts",
            "out.md",
            "过程文件/up/secret.md",
            "a\u{0}b",
        ] {
            let err = resolve_inside(&folder, Some(bad)).unwrap_err();
            assert_eq!(err.kind(), crate::ErrorKind::Invalid, "{bad}: {err}");
        }
        let err = resolve_inside(&folder, Some("过程文件/missing.md")).unwrap_err();
        assert_eq!(err.kind(), crate::ErrorKind::NotFound);
    }
}
