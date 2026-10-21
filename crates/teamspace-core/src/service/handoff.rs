//! 岗位启动包的数据来源：判断岗位是否需要交接，并取出交接内容（未完成任务、近期执行结果、
//! 相关文件引用）。
//!
//! 工作日志在文件系统里（见 [`crate::role_folder`]），由调用方在数据库锁之外读取。

use rusqlite::{Connection, OptionalExtension, params};

use crate::error::Result;
use crate::model::TaskStatus;
use crate::role_folder;
use crate::run::handoff::{
    HandoffRun, HandoffTask, OPEN_TASK_LIMIT, RECENT_RUN_LIMIT, SUMMARY_LIMIT,
};

/// 交接内容（不含工作日志）。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct HandoffData {
    /// 未完成任务（按编号升序）。
    pub open_tasks: Vec<HandoffTask>,
    /// 最近几次执行（最新的在前）。
    pub recent_runs: Vec<HandoffRun>,
    /// 未完成任务引用的文件路径（去重，按路径排序）。
    pub files: Vec<String>,
}

/// 该岗位是否需要交接内容。
///
/// 判据：上一次执行用的 Harness 实例与本次不同，或本次是该 Harness 在这个岗位上的第一次执行。
/// 也就是「换了环境要交底，同一个环境不重复交底」。
///
/// 岗位从未执行过时（新岗位的第一次执行）不附加交接节：没有可交接的历史（D35③）。
///
/// `exclude_run_id` 是本次执行（已经落库、尚未开始）：判断「此前是否用过这个 Harness」时要把它
/// 排除掉，否则第一次执行会被自己挡住。
pub(crate) fn needs_handoff(
    conn: &Connection,
    role_id: &str,
    harness_id: &str,
    exclude_run_id: Option<&str>,
) -> Result<bool> {
    let excluded = exclude_run_id.unwrap_or_default();
    // 该 Harness 在本岗位此前是否执行过。
    let same: i64 = conn.query_row(
        "SELECT COUNT(*) FROM runs WHERE role_id = ?1 AND harness_id = ?2 AND id <> ?3",
        params![role_id, harness_id, excluded],
        |row| row.get(0),
    )?;
    // 上一次执行用的 Harness。
    let last: Option<String> = conn
        .query_row(
            "SELECT harness_id FROM runs WHERE role_id = ?1 AND id <> ?2
             ORDER BY started_at DESC, id DESC LIMIT 1",
            params![role_id, excluded],
            |row| row.get(0),
        )
        .optional()?;
    Ok(match last {
        Some(last) => last != harness_id || same == 0,
        // 从未执行过：没有历史可交接，不附加交接节（D35③）。
        None => false,
    })
}

/// 取出交接内容：未完成任务、最近几次执行结果、未完成任务引用的文件。
pub(crate) fn collect(
    conn: &Connection,
    role_id: &str,
    exclude_run_id: Option<&str>,
) -> Result<HandoffData> {
    Ok(HandoffData {
        open_tasks: open_tasks(conn, role_id)?,
        recent_runs: recent_runs(conn, role_id, exclude_run_id)?,
        files: files(conn, role_id)?,
    })
}

/// 该岗位未完成的任务（todo / ready / doing / blocked / review），按编号升序，最多
/// [`OPEN_TASK_LIMIT`] 条。
fn open_tasks(conn: &Connection, role_id: &str) -> Result<Vec<HandoffTask>> {
    let mut stmt = conn.prepare_cached(
        "SELECT key, title, status FROM tasks
         WHERE owner_role_id = ?1 AND status NOT IN ('done', 'cancelled')
         ORDER BY key LIMIT ?2",
    )?;
    let rows = stmt.query_map(params![role_id, OPEN_TASK_LIMIT as i64], |row| {
        let status: String = row.get(2)?;
        Ok(HandoffTask {
            key: row.get(0)?,
            title: row.get(1)?,
            status: status_label(&status),
        })
    })?;
    Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
}

/// 该岗位最近几次**已结束**执行的结果（最新的在前），最多 [`RECENT_RUN_LIMIT`] 次。
///
/// 进行中的执行（排队 / 执行中 / 等待确认）不算「最近结果」：交接的是已经发生的事，
/// 正在跑的活没有结果可交。
fn recent_runs(
    conn: &Connection,
    role_id: &str,
    exclude_run_id: Option<&str>,
) -> Result<Vec<HandoffRun>> {
    let mut stmt = conn.prepare_cached(
        "SELECT r.task_id, r.status, r.started_at, r.summary, r.failure, t.key FROM runs r
         JOIN tasks t ON t.id = r.task_id
         WHERE r.role_id = ?1 AND r.id <> ?2
           AND r.status NOT IN ('queued', 'running', 'waiting_permission')
         ORDER BY r.started_at DESC, r.id DESC LIMIT ?3",
    )?;
    let rows = stmt.query_map(
        params![
            role_id,
            exclude_run_id.unwrap_or_default(),
            RECENT_RUN_LIMIT as i64
        ],
        |row| {
            let summary: Option<String> = row.get(3)?;
            let failure: Option<String> = row.get(4)?;
            // 摘要首段；没有摘要时用失败原因（失败执行的「结果」就是原因）。
            let text = match summary.as_deref().map(first_paragraph) {
                Some(summary) if !summary.is_empty() => summary,
                _ => failure.unwrap_or_default(),
            };
            Ok(HandoffRun {
                task_key: row.get(5)?,
                status: run_status_label(&row.get::<_, String>(1)?),
                at: role_folder::local_time(&row.get::<_, String>(2)?),
                summary: text,
            })
        },
    )?;
    Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
}

/// 未完成任务引用的文件路径（去重，保持任务编号与引用顺序）。
fn files(conn: &Connection, role_id: &str) -> Result<Vec<String>> {
    let mut stmt = conn.prepare_cached(
        "SELECT DISTINCT f.path FROM task_file_refs f
         JOIN tasks t ON t.id = f.task_id
         WHERE t.owner_role_id = ?1 AND t.status NOT IN ('done', 'cancelled')
         ORDER BY f.path",
    )?;
    let rows = stmt.query_map(params![role_id], |row| row.get::<_, String>(0))?;
    Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
}

/// 摘要首段（空行分隔的第一段），超过 [`SUMMARY_LIMIT`] 时截断。
fn first_paragraph(summary: &str) -> String {
    let text = summary.trim();
    let head = text.split("\n\n").next().unwrap_or("").trim();
    let head = head.replace('\n', " ");
    if head.chars().count() > SUMMARY_LIMIT {
        let mut out: String = head.chars().take(SUMMARY_LIMIT).collect();
        out.push('…');
        return out;
    }
    head.to_owned()
}

/// 任务状态的显示名。
fn status_label(status: &str) -> String {
    match status {
        "todo" => TaskStatus::Todo.label().to_owned(),
        "ready" => TaskStatus::Ready.label().to_owned(),
        "doing" => TaskStatus::Doing.label().to_owned(),
        "blocked" => TaskStatus::Blocked.label().to_owned(),
        "review" => TaskStatus::Review.label().to_owned(),
        "done" => TaskStatus::Done.label().to_owned(),
        _ => TaskStatus::Cancelled.label().to_owned(),
    }
}

/// 执行状态的显示名。
fn run_status_label(status: &str) -> String {
    match status {
        "succeeded" => "成功",
        "failed" => "失败",
        "cancelled" => "已取消",
        "interrupted" => "已中断",
        "waiting_permission" => "等待确认",
        "running" => "执行中",
        _ => "排队中",
    }
    .to_owned()
}
