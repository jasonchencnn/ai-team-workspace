//! 执行记录与执行进度的数据库部分；执行引擎见 [`crate::run`]。
//!
//! 任务状态联动（由引擎或命令在同一事务中调用 [`move_task`]）：
//! - 开始执行：READY / BLOCKED → DOING（从 BLOCKED 开始时清除原因）；DOING（上次执行已中断或人工接管）
//!   保持 DOING；
//! - 成功 → REVIEW；失败 → BLOCKED（「执行失败：原因」）；Human 取消 → READY；
//! - 人工接管、中断：保持 DOING；中断后标记失败 → BLOCKED。

use std::path::{Path, PathBuf};

use rusqlite::{Connection, params};
use serde_json::json;

use super::event::{self, NewEvent};
use super::folder;
use super::task::{self, FileCheck};
use super::{harness, role};
use crate::error::{Error, Result};
use crate::model::{
    AcpCapabilities, ActiveRunSummary, Actor, ChangedFile, EventKind, FailureKind, HandshakeStatus,
    HarnessSnapshot, PermissionPrompt, Run, RunIsolation, RunJournal, RunReview, RunStatus,
    RunUpdate, RunUpdateBody, RunUpdatePage, RunUpdateQuery, Task, TaskStatus,
};
use crate::store::{enum_col, enum_str, new_id, now};

pub(crate) const DEFAULT_UPDATE_PAGE: u32 = 200;
pub(crate) const MAX_UPDATE_PAGE: u32 = 500;
/// 执行失败时任务受阻原因的前缀。
pub(crate) const FAILED_PREFIX: &str = "执行失败：";

const COLS: &str = "r.id, r.team_id, r.task_id, r.role_id, r.harness_id, r.harness_name, \
    r.harness_preset, r.harness_command, r.harness_args, r.attempt, r.status, r.session_id, \
    r.exec_dir, r.isolation, r.branch, r.base_commit, r.capabilities, r.started_at, r.ended_at, \
    r.stop_reason, r.failure, r.failure_kind, r.summary, r.changed_files, r.tool_call_count, \
    r.taken_over, r.pending_permission, t.key, r.workspace_path, r.out_of_scope, r.change_note, \
    r.change_incomplete, r.base_branch, r.merged_commit, r.revision_of, r.review_outcome, \
    r.review_comment, r.reviewed_at, r.no_changes, r.interrupted_commit, r.interrupted_problem";

fn json_opt<T: serde::de::DeserializeOwned>(
    row: &rusqlite::Row<'_>,
    idx: usize,
) -> rusqlite::Result<Option<T>> {
    let raw: Option<String> = row.get(idx)?;
    Ok(raw.and_then(|s| serde_json::from_str(&s).ok()))
}

fn from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<Run> {
    let status: RunStatus = enum_col(row, 10)?;
    let capabilities: Option<AcpCapabilities> = json_opt(row, 16)?;
    let session_id: Option<String> = row.get(11)?;
    let failure_kind: Option<String> = row.get(21)?;
    let taken_over: bool = row.get(25)?;
    Ok(Run {
        id: row.get(0)?,
        team_id: row.get(1)?,
        task_id: row.get(2)?,
        role_id: row.get(3)?,
        harness: HarnessSnapshot {
            harness_id: row.get(4)?,
            name: row.get(5)?,
            preset: enum_col(row, 6)?,
            command: row.get(7)?,
            args: json_opt(row, 8)?.unwrap_or_default(),
        },
        attempt: row.get(9)?,
        status,
        // 人工接管的执行状态仍是「已取消」，但对 Human 的说法统一为「已被接管」（与 FE-002 一致）。
        status_label: if taken_over {
            "已被接管".to_owned()
        } else {
            status.label().to_owned()
        },
        can_resume: status == RunStatus::Interrupted
            && session_id.is_some()
            && capabilities.as_ref().is_some_and(|c| c.load_session),
        session_id,
        exec_dir: row.get(12)?,
        isolation: enum_col(row, 13)?,
        branch: row.get(14)?,
        base_commit: row.get(15)?,
        capabilities,
        started_at: row.get(17)?,
        ended_at: row.get(18)?,
        stop_reason: row.get(19)?,
        failure: row.get(20)?,
        failure_kind: failure_kind
            .and_then(|k| serde_json::from_value(serde_json::Value::String(k)).ok()),
        summary: row.get(22)?,
        changed_files: json_opt(row, 23)?.unwrap_or_default(),
        out_of_scope: json_opt(row, 29)?.unwrap_or_default(),
        change_note: row.get(30)?,
        change_incomplete: row.get(31)?,
        tool_call_count: row.get(24)?,
        no_changes: row.get(38)?,
        interrupted_commit: row.get(39)?,
        interrupted_problem: row.get(40)?,
        taken_over,
        pending_permission: json_opt(row, 26)?,
        task_key: row.get(27)?,
        journal: RunJournal::default(),
        base_branch: row.get(32)?,
        merged_commit: row.get(33)?,
        revision_of: row.get(34)?,
        review: match row.get::<_, Option<String>>(35)? {
            Some(_) => Some(RunReview {
                outcome: enum_col(row, 35)?,
                comment: row.get(36)?,
                reviewed_at: row.get::<_, Option<String>>(37)?.unwrap_or_default(),
            }),
            None => None,
        },
    })
}

fn query(conn: &Connection, filter: &str, param: &dyn rusqlite::ToSql) -> Result<Vec<Run>> {
    let mut stmt = conn.prepare(&format!(
        "SELECT {COLS} FROM runs r JOIN tasks t ON t.id = r.task_id WHERE {filter}"
    ))?;
    let rows = stmt.query_map([param], from_row)?;
    let mut runs = rows.collect::<rusqlite::Result<Vec<_>>>()?;
    // 工作日志的写入结果不在 `runs` 表里，按条补上（界面据此显示，不自己拼装）。
    for run in &mut runs {
        run.journal = folder::run_journal(conn, run)?;
    }
    Ok(runs)
}

/// 执行记录（所属团队须未归档）；否则 not_found。
pub(crate) fn get(conn: &Connection, id: &str) -> Result<Run> {
    query(
        conn,
        "r.id = ?1 AND r.team_id IN (SELECT id FROM teams WHERE archived_at IS NULL)",
        &id,
    )?
    .pop()
    .ok_or_else(|| Error::not_found("执行记录不存在"))
}

/// 执行目录所在的工作空间路径。
pub(crate) fn workspace_of(conn: &Connection, id: &str) -> Result<String> {
    Ok(conn.query_row(
        "SELECT workspace_path FROM runs WHERE id = ?1",
        params![id],
        |row| row.get(0),
    )?)
}

/// 任务的全部执行，最近的在前。
pub(crate) fn list_for_task(conn: &Connection, task_id: &str) -> Result<Vec<Run>> {
    task::get(conn, task_id)?;
    query(conn, "r.task_id = ?1 ORDER BY r.attempt DESC", &task_id)
}

/// 任务最近一次执行。
pub(crate) fn latest_for_task(conn: &Connection, task_id: &str) -> Result<Option<Run>> {
    Ok(query(
        conn,
        "r.task_id = ?1 ORDER BY r.attempt DESC LIMIT 1",
        &task_id,
    )?
    .pop())
}

/// 岗位最近的执行，最多 `limit` 条。
pub(crate) fn recent_for_role(conn: &Connection, role_id: &str, limit: u32) -> Result<Vec<Run>> {
    query(
        conn,
        &format!("r.role_id = ?1 ORDER BY r.started_at DESC, r.id DESC LIMIT {limit}"),
        &role_id,
    )
}

const ACTIVE_SQL: &str = "r.status IN ('queued', 'running', 'waiting_permission')";

/// 任务的活动执行。
pub(crate) fn active_for_task(conn: &Connection, task_id: &str) -> Result<Option<Run>> {
    Ok(query(conn, &format!("r.task_id = ?1 AND {ACTIVE_SQL}"), &task_id)?.pop())
}

/// 岗位进行中的执行（按开始时间倒序）。
///
/// 换绑用：待生效换绑要等这个列表空下来才生效（M6-01）。
pub(crate) fn active_runs_for_role(conn: &Connection, role_id: &str) -> Result<Vec<Run>> {
    query(
        conn,
        &format!("r.role_id = ?1 AND {ACTIVE_SQL} ORDER BY r.started_at DESC, r.id DESC"),
        &role_id,
    )
}

/// 岗位进行中的执行摘要：换绑界面要看的「现在有没有在跑的活」（M6-04）。
///
/// 「进行中」用同一套判定（[`ACTIVE_SQL`]），但只取界面要的字段：执行 id、任务编号与标题、
/// 第几次执行、状态与 Harness 实例快照。界面不再自己列状态，也不用逐个任务去查执行。
pub(crate) fn active_summaries_for_role(
    conn: &Connection,
    role_id: &str,
) -> Result<Vec<ActiveRunSummary>> {
    let mut stmt = conn.prepare(&format!(
        "SELECT r.id, r.task_id, t.key, t.title, r.attempt, r.status, r.taken_over,
                r.harness_id, r.harness_name, r.harness_preset, r.harness_command, r.harness_args,
                r.started_at
         FROM runs r JOIN tasks t ON t.id = r.task_id
         WHERE r.role_id = ?1 AND {ACTIVE_SQL}
         ORDER BY r.started_at DESC, r.id DESC"
    ))?;
    let rows = stmt.query_map(params![role_id], |row| {
        let status: RunStatus = enum_col(row, 5)?;
        let taken_over: bool = row.get(6)?;
        Ok(ActiveRunSummary {
            run_id: row.get(0)?,
            task_id: row.get(1)?,
            task_key: row.get(2)?,
            task_title: row.get(3)?,
            attempt: row.get(4)?,
            status,
            // 人工接管的执行状态仍是「已取消」，但对 Human 的说法统一为「已被接管」（与执行详情一致）。
            status_label: if taken_over {
                "已被接管".to_owned()
            } else {
                status.label().to_owned()
            },
            harness: HarnessSnapshot {
                harness_id: row.get(7)?,
                name: row.get(8)?,
                preset: enum_col(row, 9)?,
                command: row.get(10)?,
                args: json_opt(row, 11)?.unwrap_or_default(),
            },
            started_at: row.get(12)?,
        })
    })?;
    Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
}

/// 活动执行所在的工作空间路径（原地执行），与执行记录一起读出。
fn active_in_place_workspaces(conn: &Connection) -> Result<Vec<(String, Run)>> {
    let mut stmt = conn.prepare(&format!(
        "SELECT {COLS} FROM runs r JOIN tasks t ON t.id = r.task_id
         WHERE r.isolation = 'in_place' AND {ACTIVE_SQL}"
    ))?;
    let rows = stmt
        .query_map([], |row| Ok((row.get::<_, String>(28)?, from_row(row)?)))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(rows)
}

/// 两条工作空间路径是否指向同一片区域（D15「同一文件夹同时只运行一个执行」）。
///
/// 先把两边都解析成真实路径（解符号链接），macOS 默认文件系统不区分大小写，因此比较时统一转小写；
/// 两个路径互为祖先或后代（嵌套文件夹）也算同一片区域。
fn same_workspace(left: &str, right: &str) -> bool {
    let real = |path: &str| {
        let real = std::fs::canonicalize(path).unwrap_or_else(|_| PathBuf::from(path));
        if cfg!(target_os = "macos") {
            real.to_string_lossy().to_lowercase()
        } else {
            real.to_string_lossy().into_owned()
        }
    };
    let (left, right) = (real(left), real(right));
    let (left, right) = (Path::new(&left), Path::new(&right));
    left == right || left.starts_with(right) || right.starts_with(left)
}

/// 与 `workspace` 指向同一片文件夹（或其子目录）的活动原地执行。
///
/// 按真实路径判断，符号链接别名、只差大小写的路径和嵌套的工作空间都会命中。
pub(crate) fn active_in_place(conn: &Connection, workspace: &str) -> Result<Option<Run>> {
    Ok(active_in_place_workspaces(conn)?
        .into_iter()
        .find(|(path, _)| same_workspace(path, workspace))
        .map(|(_, run)| run))
}

/// 所有活动执行（重启恢复用）。
pub(crate) fn all_active(conn: &Connection) -> Result<Vec<Run>> {
    query(conn, &format!("{ACTIVE_SQL} AND ?1 = 1"), &1)
}

/// 现在开始执行会被拒绝的原因（见 [`crate::model::TaskDetail::run_problems`]）。
pub(crate) fn start_problems(
    conn: &Connection,
    task: &Task,
    files: &FileCheck,
) -> Result<Vec<String>> {
    let mut problems = Vec::new();
    let latest = latest_for_task(conn, &task.id)?;
    match (task.status, &latest) {
        (_, Some(run)) if run.status.is_active() => {
            problems.push(format!("任务正在执行（第 {} 次）", run.attempt));
            return Ok(problems);
        }
        (TaskStatus::Ready | TaskStatus::Blocked, _) => {}
        (TaskStatus::Doing, Some(run))
            if run.status == RunStatus::Interrupted || run.taken_over => {}
        (status, _) => problems.push(format!(
            "任务状态为「{}」：只有 READY、受阻，或执行中断、人工接管后的任务可以开始执行",
            status.label()
        )),
    }
    problems.extend(task::readiness(
        conn,
        &task.owner_role_id,
        &task.file_refs,
        &task.depends_on,
        files,
    )?);
    if let Ok(owner) = role::get_active(conn, &task.owner_role_id) {
        match &owner.harness_id {
            None => problems.push(format!(
                "负责岗位「{}」未绑定 Harness，请先在「岗位与绑定」中绑定",
                owner.name
            )),
            Some(id) => match harness::get_any(conn, id)? {
                Some(h) if h.archived_at.is_some() => {
                    problems.push(format!("Harness「{}」已归档，请重新绑定", h.name));
                }
                Some(h) if h.handshake.status == HandshakeStatus::Error => problems.push(format!(
                    "Harness「{}」握手检查失败，请先修正配置并重新检查：{}",
                    h.name,
                    h.handshake.detail.as_deref().unwrap_or("（无说明）")
                )),
                Some(_) => {}
                None => problems.push("绑定的 Harness 不存在".to_owned()),
            },
        }
    }
    let workspace = super::team::get_active(conn, &task.team_id)?.workspace_path;
    if let Some(busy) = active_in_place(conn, &workspace)?
        && busy.task_id != task.id
    {
        problems.push(format!(
            "工作空间（普通文件夹）正在执行 {}：同一文件夹同时只能运行一个执行",
            busy.task_key
        ));
    }
    Ok(problems)
}

/// 新执行。
pub(crate) struct NewRun<'a> {
    pub task: &'a Task,
    pub harness: HarnessSnapshot,
    pub workspace: &'a str,
    pub exec_dir: &'a str,
    pub isolation: RunIsolation,
}

pub(crate) fn next_attempt(conn: &Connection, task_id: &str) -> Result<u32> {
    Ok(conn.query_row(
        "SELECT COALESCE(MAX(attempt), 0) + 1 FROM runs WHERE task_id = ?1",
        params![task_id],
        |row| row.get(0),
    )?)
}

pub(crate) fn insert(conn: &Connection, new: NewRun<'_>) -> Result<Run> {
    let id = new_id();
    let attempt = next_attempt(conn, &new.task.id)?;
    // 快照里的参数是给人看的，疑似密钥在此脱敏（D6）；启动 Agent 时从 `harnesses` 表读原始参数
    // （见 [`crate::run`] 的启动流程），所以不影响实际执行。
    let snapshot_args = crate::redact::args(&new.harness.args);
    conn.execute(
        "INSERT INTO runs (id, team_id, task_id, role_id, harness_id, harness_name, harness_preset,
                           harness_command, harness_args, attempt, status, workspace_path, exec_dir,
                           isolation, started_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, 'queued', ?11, ?12, ?13, ?14)",
        params![
            id,
            new.task.team_id,
            new.task.id,
            new.task.owner_role_id,
            new.harness.harness_id,
            new.harness.name,
            enum_str(new.harness.preset),
            new.harness.command,
            serde_json::to_string(&snapshot_args)?,
            attempt,
            new.workspace,
            new.exec_dir,
            enum_str(new.isolation),
            now(),
        ],
    )?;
    get(conn, &id)
}

/// 记录执行目录（worktree 建立之后）与基线分支。只对仍在活动中的执行生效（fencing），返回是否生效。
pub(crate) fn set_exec(
    conn: &Connection,
    id: &str,
    exec_dir: &str,
    branch: Option<&str>,
    base_commit: Option<&str>,
    base_branch: Option<&str>,
) -> Result<bool> {
    let changed = conn.execute(
        &format!(
            "UPDATE runs SET exec_dir = ?2, branch = ?3, base_commit = ?4, base_branch = ?5
             WHERE id = ?1 AND status IN {ACTIVE_STATUSES}"
        ),
        params![id, exec_dir, branch, base_commit, base_branch],
    )?;
    fenced(conn, changed, id, "记录执行目录")
}

/// 记录 Agent 能力（握手之后）。只对仍在活动中的执行生效（fencing），返回是否生效。
pub(crate) fn set_capabilities(
    conn: &Connection,
    id: &str,
    caps: &AcpCapabilities,
) -> Result<bool> {
    let changed = conn.execute(
        &format!("UPDATE runs SET capabilities = ?2 WHERE id = ?1 AND status IN {ACTIVE_STATUSES}"),
        params![id, serde_json::to_string(caps)?],
    )?;
    fenced(conn, changed, id, "记录 Agent 能力")
}

/// 记录会话 ID（`session/new` 之后）。只对仍在活动中的执行生效：旧 driver 迟到的会话 ID 不会覆盖
/// 续接后新会话的 ID（fencing），返回是否生效。
pub(crate) fn set_session(conn: &Connection, id: &str, session_id: &str) -> Result<bool> {
    let changed = conn.execute(
        &format!("UPDATE runs SET session_id = ?2 WHERE id = ?1 AND status IN {ACTIVE_STATUSES}"),
        params![id, session_id],
    )?;
    fenced(conn, changed, id, "记录会话")
}

/// 活动状态的 SQL 条件（fencing，Q-03）：执行结束后迟到的写入因此不生效。
const ACTIVE_STATUSES: &str = "('queued', 'running', 'waiting_permission')";

/// 条件更新影响 0 行时（执行已不在预期状态，迟到的写入被忽略）记日志，并在执行记录里留一条状态说明，
/// 以便在界面上看到。返回是否生效。
fn fenced(conn: &Connection, changed: usize, id: &str, what: &str) -> Result<bool> {
    if changed > 0 {
        return Ok(true);
    }
    eprintln!("teamspace run {id}: {what} 被忽略：执行已不在预期状态");
    let current: Option<RunStatus> = conn
        .query_row(
            "SELECT status FROM runs WHERE id = ?1",
            params![id],
            |row| enum_col(row, 0),
        )
        .ok();
    if let Some(status) = current {
        status_update(
            conn,
            id,
            status,
            Some(format!(
                "迟到的「{what}」已忽略：执行已是「{}」",
                status.label()
            )),
        )?;
    }
    Ok(false)
}

/// 活动状态之间的切换（queued / running / waiting_permission）与待确认的权限请求。
/// 只对仍在活动中的执行生效；返回是否生效（已结束的执行不会被改回活动）。
pub(crate) fn set_active_status(
    conn: &Connection,
    id: &str,
    status: RunStatus,
    pending: Option<&PermissionPrompt>,
    tool_call_count: u32,
) -> Result<bool> {
    debug_assert!(status.is_active());
    let pending = pending.map(serde_json::to_string).transpose()?;
    let changed = conn.execute(
        &format!(
            "UPDATE runs SET status = ?2, pending_permission = ?3, tool_call_count = ?4
             WHERE id = ?1 AND status IN {ACTIVE_STATUSES}"
        ),
        params![id, enum_str(status), pending, tool_call_count],
    )?;
    fenced(conn, changed, id, "活动状态切换")
}

/// 执行结束时写入的内容。
#[derive(Debug, Clone, Default)]
pub(crate) struct Ending {
    pub stop_reason: Option<String>,
    pub failure: Option<String>,
    pub failure_kind: Option<FailureKind>,
    pub summary: Option<String>,
    pub changed_files: Vec<ChangedFile>,
    /// 快照比对发现的范围外改动（见 [`Run::out_of_scope`]）。
    pub out_of_scope: Vec<String>,
    /// 改动统计的补充说明（见 [`Run::change_note`]）。
    pub change_note: Option<String>,
    /// 改动统计不完整（见 [`Run::change_incomplete`]）。
    pub change_incomplete: bool,
    /// 成功但相对基线没有任何改动（见 [`Run::no_changes`]）。
    pub no_changes: bool,
    pub tool_call_count: u32,
    pub taken_over: bool,
}

/// 把仍在活动中的执行置为结束状态（succeeded / failed / cancelled），清除待确认的权限请求。
/// 返回是否生效：执行已结束（含已中断）时不改动，迟到的结局不能覆盖（fencing，Q-03）。
///
/// 失败说明可能含 Agent 的 stderr 尾部，落库前统一脱敏（D6）。
pub(crate) fn end(conn: &Connection, id: &str, status: RunStatus, ending: &Ending) -> Result<bool> {
    end_from(conn, id, status, ending, ACTIVE_STATUSES, "结束执行")
}

/// 把已中断的执行置为结束状态（重新执行前标记失败、人工接管、Human 标记失败）。
/// 返回是否生效：执行已不是 interrupted 时不改动。
pub(crate) fn end_interrupted(
    conn: &Connection,
    id: &str,
    status: RunStatus,
    ending: &Ending,
) -> Result<bool> {
    end_from(
        conn,
        id,
        status,
        ending,
        "('interrupted')",
        "结束已中断的执行",
    )
}

fn end_from(
    conn: &Connection,
    id: &str,
    status: RunStatus,
    ending: &Ending,
    from: &str,
    what: &str,
) -> Result<bool> {
    debug_assert!(!status.is_active());
    let failure = ending.failure.as_deref().map(crate::redact::text);
    // 「成功但无改动」只对成功的执行成立：失败、取消、中断、人工接管的执行恒为 `false`。
    let no_changes = status == RunStatus::Succeeded && ending.no_changes;
    let changed = conn.execute(
        &format!(
            "UPDATE runs SET status = ?2, ended_at = ?3, stop_reason = ?4, failure = ?5,
                             failure_kind = ?6, summary = ?7, changed_files = ?8, tool_call_count = ?9,
                             taken_over = ?10, out_of_scope = ?11, change_note = ?12,
                             change_incomplete = ?13, pending_permission = NULL, no_changes = ?14
             WHERE id = ?1 AND status IN {from}"
        ),
        params![
            id,
            enum_str(status),
            now(),
            ending.stop_reason,
            failure,
            ending.failure_kind.map(enum_str),
            ending.summary,
            serde_json::to_string(&ending.changed_files)?,
            ending.tool_call_count,
            ending.taken_over,
            serde_json::to_string(&ending.out_of_scope)?,
            ending.change_note,
            ending.change_incomplete,
            no_changes,
        ],
    )?;
    fenced(conn, changed, id, what)
}

/// 续接：interrupted → queued，清除结束信息。只对 interrupted 生效，返回是否生效。
pub(crate) fn reopen(conn: &Connection, id: &str) -> Result<bool> {
    let changed = conn.execute(
        "UPDATE runs SET status = 'queued', ended_at = NULL, failure = NULL, failure_kind = NULL,
                         stop_reason = NULL WHERE id = ?1 AND status = 'interrupted'",
        params![id],
    )?;
    fenced(conn, changed, id, "续接")
}

/// 可以由 Human 接管的已结束执行：已中断的，或已失败的（Figma 94-1001 状态区 B 的「人工接管」）。
pub(crate) fn can_take_over(run: &Run) -> bool {
    matches!(run.status, RunStatus::Interrupted | RunStatus::Failed) && !run.taken_over
}

/// 接管已结束的执行：标记 `taken_over` 并结束这条执行，任务从受阻回到执行中，由 Human 在执行目录里
/// 继续处理；之后可重新执行或直接提交验收。活动与工作日志沿用既有的接管文案。
///
/// 活动执行由引擎在收到停止请求时走同一条路径（见 [`end`] 的 `taken_over`），本函数只处理已经结束的。
pub(crate) fn takeover(conn: &Connection, run_id: &str) -> Result<()> {
    // 在同一个写事务里读取，避免用事务外的陈旧快照做判断。
    let run = &get(conn, run_id)?;
    if !can_take_over(run) {
        return Err(Error::conflict(format!(
            "执行已结束（{}），不能接管",
            run.status.label()
        )));
    }
    // 任务须仍停在这次执行留下的状态：失败 → 受阻，中断 → 执行中；已被改动（如提交验收）时不拉回。
    let task_status = task::get(conn, &run.task_id)?.status;
    let expected = match run.status {
        RunStatus::Failed => TaskStatus::Blocked,
        _ => TaskStatus::Doing,
    };
    if task_status != expected {
        return Err(Error::conflict(format!(
            "任务已是「{}」，不能接管这次执行",
            task_status.label()
        )));
    }
    let failure = "人工接管".to_owned();
    // 只从已中断或已失败的状态结束（fencing）：状态已被别处改变时不覆盖，返回冲突。
    let ended = end_from(
        conn,
        &run.id,
        RunStatus::Cancelled,
        &Ending {
            failure: Some(failure.clone()),
            failure_kind: run.failure_kind,
            summary: run.summary.clone(),
            changed_files: run.changed_files.clone(),
            out_of_scope: run.out_of_scope.clone(),
            change_note: run.change_note.clone(),
            change_incomplete: run.change_incomplete,
            tool_call_count: run.tool_call_count,
            taken_over: true,
            ..Ending::default()
        },
        "('interrupted', 'failed')",
        "接管已结束的执行",
    )?;
    if !ended {
        return Err(Error::conflict("执行状态已改变，不能接管"));
    }
    status_update(conn, &run.id, RunStatus::Cancelled, Some(failure))?;
    move_task(
        conn,
        &run.task_id,
        TaskStatus::Doing,
        None,
        Actor::Human,
        "人工接管",
    )?;
    record_event(
        conn,
        run,
        Actor::Human,
        EventKind::RunFinished,
        format!(
            "{} 第 {} 次执行已停止，人工接管（{}）",
            run.task_key, run.attempt, run.harness.name
        ),
        json!({ "runId": run.id, "attempt": run.attempt,
                "status": RunStatus::Cancelled, "takenOver": true }),
    )
}

/// 系统（执行引擎）或 Human 触发的任务状态变化，绕过人工流转表；记录活动。状态不变时不做任何事。
pub(crate) fn move_task(
    conn: &Connection,
    task_id: &str,
    to: TaskStatus,
    reason: Option<String>,
    actor: Actor,
    note: &str,
) -> Result<()> {
    let task = task::get(conn, task_id)?;
    if task.status == to && task.blocked_reason == reason {
        return Ok(());
    }
    let reason = if to == TaskStatus::Blocked {
        // 受阻原因会显示在任务详情、写入活动与工作日志，落库前统一脱敏（D6）。
        Some(crate::redact::text(
            &reason.unwrap_or_else(|| "执行失败".to_owned()),
        ))
    } else {
        None
    };
    conn.execute(
        "UPDATE tasks SET status = ?2, blocked_reason = ?3, updated_at = ?4 WHERE id = ?1",
        params![task.id, enum_str(to), reason, now()],
    )?;
    let mut summary = format!("{} 状态 {} → {}", task.key, task.status.label(), to.label());
    if let Some(reason) = &reason {
        summary.push_str(&format!("：{reason}"));
    }
    if !note.is_empty() {
        summary.push_str(&format!("（{note}）"));
    }
    event::record_as(
        conn,
        actor,
        NewEvent {
            team_id: &task.team_id,
            task_id: Some(&task.id),
            role_id: Some(&task.owner_role_id),
            kind: EventKind::TaskStatusChanged,
            summary,
            data: json!({ "from": task.status, "to": to, "reason": reason, "byRun": true }),
        },
    )
}

/// 记录与执行相关的活动（role_id 为执行的岗位，因此会写入岗位工作日志）。
pub(crate) fn record_event(
    conn: &Connection,
    run: &Run,
    actor: Actor,
    kind: EventKind,
    summary: String,
    data: serde_json::Value,
) -> Result<()> {
    event::record_as(
        conn,
        actor,
        NewEvent {
            team_id: &run.team_id,
            task_id: Some(&run.task_id),
            role_id: Some(&run.role_id),
            kind,
            summary,
            data,
        },
    )
}

// ---------------------------------------------------------------- 执行进度

fn update_kind(body: &RunUpdateBody) -> &'static str {
    match body {
        RunUpdateBody::Message { .. } => "message",
        RunUpdateBody::Plan { .. } => "plan",
        RunUpdateBody::ToolCall { .. } => "tool_call",
        RunUpdateBody::Permission { .. } => "permission",
        RunUpdateBody::FileWrite { .. } => "file_write",
        RunUpdateBody::Status { .. } => "status",
        RunUpdateBody::Warning { .. } => "warning",
    }
}

fn next_rev(conn: &Connection, run_id: &str) -> Result<i64> {
    Ok(conn.query_row(
        "SELECT COALESCE(MAX(rev), 0) + 1 FROM run_updates WHERE run_id = ?1",
        params![run_id],
        |row| row.get(0),
    )?)
}

const UPDATE_COLS: &str = "id, run_id, seq, body, created_at, updated_at";

fn update_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<RunUpdate> {
    let body: String = row.get(3)?;
    Ok(RunUpdate {
        id: row.get(0)?,
        run_id: row.get(1)?,
        seq: row.get(2)?,
        body: serde_json::from_str(&body).map_err(|err| {
            rusqlite::Error::FromSqlConversionFailure(3, rusqlite::types::Type::Text, Box::new(err))
        })?,
        created_at: row.get(4)?,
        updated_at: row.get(5)?,
    })
}

fn get_update(conn: &Connection, id: &str) -> Result<RunUpdate> {
    Ok(conn.query_row(
        &format!("SELECT {UPDATE_COLS} FROM run_updates WHERE id = ?1"),
        params![id],
        update_from_row,
    )?)
}

/// 新增一条执行进度。`key` 用于之后找回同一条目（工具调用 ID、权限请求 ID、`plan`）。
pub(crate) fn insert_update(
    conn: &Connection,
    run_id: &str,
    key: Option<&str>,
    body: &RunUpdateBody,
) -> Result<RunUpdate> {
    let id = new_id();
    let at = now();
    conn.execute(
        "INSERT INTO run_updates (id, run_id, rev, kind, key, body, created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?7)",
        params![
            id,
            run_id,
            next_rev(conn, run_id)?,
            update_kind(body),
            key,
            serde_json::to_string(body)?,
            at
        ],
    )?;
    get_update(conn, &id)
}

/// 替换一条执行进度的内容（位置 `seq` 不变，变更序号前移）。
pub(crate) fn replace_update(
    conn: &Connection,
    run_id: &str,
    update_id: &str,
    body: &RunUpdateBody,
) -> Result<RunUpdate> {
    conn.execute(
        "UPDATE run_updates SET body = ?3, rev = ?4, updated_at = ?5 WHERE id = ?1 AND run_id = ?2",
        params![
            update_id,
            run_id,
            serde_json::to_string(body)?,
            next_rev(conn, run_id)?,
            now()
        ],
    )?;
    get_update(conn, update_id)
}

/// 增量读取执行进度（见 [`RunUpdatePage`]）。
pub(crate) fn page_updates(conn: &Connection, query: &RunUpdateQuery) -> Result<RunUpdatePage> {
    let run = get(conn, &query.run_id)?;
    let after: i64 = match query.cursor.as_deref().map(str::trim) {
        None | Some("") => 0,
        Some(raw) => raw
            .parse()
            .ok()
            .filter(|v: &i64| *v >= 0)
            .ok_or_else(|| Error::invalid("游标无效"))?,
    };
    let limit = query
        .limit
        .unwrap_or(DEFAULT_UPDATE_PAGE)
        .clamp(1, MAX_UPDATE_PAGE);
    let mut stmt = conn.prepare_cached(&format!(
        "SELECT {UPDATE_COLS}, rev FROM run_updates WHERE run_id = ?1 AND rev > ?2
         ORDER BY rev LIMIT ?3"
    ))?;
    let mut rows = stmt
        .query_map(params![run.id, after, limit + 1], |row| {
            Ok((update_from_row(row)?, row.get::<_, i64>(6)?))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    let has_more = rows.len() > limit as usize;
    rows.truncate(limit as usize);
    let next = rows.last().map_or(after, |(_, rev)| *rev);
    Ok(RunUpdatePage {
        run_id: run.id,
        updates: rows.into_iter().map(|(u, _)| u).collect(),
        next_cursor: next.to_string(),
        has_more,
        status: run.status,
    })
}

/// 记录一条状态变化进度。
pub(crate) fn status_update(
    conn: &Connection,
    run_id: &str,
    status: RunStatus,
    detail: Option<String>,
) -> Result<RunUpdate> {
    insert_update(
        conn,
        run_id,
        None,
        &RunUpdateBody::Status { status, detail },
    )
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::*;
    use crate::service::task::{self, FileCheck};
    use crate::tests::plain_fixture;

    /// K-RUN-006：真实路径相同的文件夹算同一片；无关的文件夹不算。
    #[test]
    fn same_workspace_compares_real_paths_and_nesting() {
        let dir = tempfile::tempdir().unwrap();
        let base = dir.path().join("ws");
        std::fs::create_dir_all(base.join("sub")).unwrap();
        let base = base.to_string_lossy().into_owned();
        let sub = Path::new(&base).join("sub").to_string_lossy().into_owned();

        // 完全相同、带尾斜杠、子目录、嵌套，都算同一片。
        assert!(same_workspace(&base, &base));
        assert!(same_workspace(&base, &format!("{base}/")));
        assert!(same_workspace(&base, &sub));
        assert!(same_workspace(&sub, &base));
        // 同级兄弟目录不算（避免把 `ws2` 误判成 `ws` 的子目录）。
        let sibling = dir.path().join("ws2").to_string_lossy().into_owned();
        std::fs::create_dir_all(&sibling).unwrap();
        assert!(!same_workspace(&base, &sibling));
        // 另一个临时目录（完全无关的路径）不算。
        let other = tempfile::tempdir().unwrap();
        assert!(!same_workspace(&base, &other.path().to_string_lossy()));
    }

    /// K-RUN-006：符号链接别名与只差大小写的路径都指向同一片文件夹。
    #[cfg(unix)]
    #[test]
    fn same_workspace_resolves_symlink_and_case() {
        let dir = tempfile::tempdir().unwrap();
        let base = dir.path().join("ws");
        std::fs::create_dir_all(&base).unwrap();
        let base = base.to_string_lossy().into_owned();

        let link = dir.path().join("link");
        std::os::unix::fs::symlink(&base, &link).unwrap();
        assert!(same_workspace(&base, &link.to_string_lossy()));

        let upper = base.to_uppercase();
        if std::path::Path::new(&upper).is_dir() {
            // 文件系统不区分大小写（如 macOS 默认）时才算同一片。
            assert!(same_workspace(&base, &upper));
        }
    }

    /// RUN-PRE-004 B：负责岗位记录不存在（数据损坏）时，前置检查报告「未指定负责岗位」，而不是崩溃。
    #[test]
    fn start_problems_reports_a_missing_owner_role() {
        let f = plain_fixture();
        let team = f.team("T");
        let owner = f.role(&team.id, "role_frontend");
        let detail = f.task(&team.id, &owner, "没有负责人的任务");
        f.raw_sql(&format!(
            "PRAGMA foreign_keys = OFF; UPDATE tasks SET owner_role_id = 'role-gone' WHERE id = '{}'",
            detail.task.id
        ));
        let conn = Connection::open(f.db_path()).unwrap();
        let task = task::get(&conn, &detail.task.id).unwrap();
        let files = FileCheck::scan(Path::new(&f.ws), &task.file_refs);
        let problems = start_problems(&conn, &task, &files).unwrap();
        assert!(
            problems.iter().any(|p| p == "未指定负责岗位"),
            "{problems:?}"
        );
    }
}
