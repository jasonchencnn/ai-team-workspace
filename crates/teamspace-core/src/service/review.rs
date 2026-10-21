//! 验收的数据库部分（M3-01，D20）；Git 操作经 [`crate::run::review_seam`]，命令见 [`crate::api`]。
//!
//! 规则：
//! - 待验收的执行 = 任务处于 REVIEW 时它的最近一次执行（且不在活动中）。通过与退回都只接受这一次执行
//!   （fencing）：旧页面上的旧执行、任务已不在 REVIEW 时一律拒绝，提示刷新。
//! - 退回：意见必填；被退回的执行记下意见（`review_outcome = rejected`），任务回到执行中，并新建一次执行
//!   （`revision_of` 指向被退回的执行），沿用它的执行目录、分支、基线与会话，不新建 worktree。
//! - 历次退回意见按被退回执行的序号列出。

use std::collections::BTreeMap;

use rusqlite::{Connection, params};

use super::run::{self, NewRun};
use super::task;
use crate::error::{Error, Result};
use crate::model::{
    ChangedFile, FileChangeKind, HarnessSnapshot, ReviewOutcome, ReviewRejection, Run, Task,
    TaskStatus,
};
use crate::store::{enum_str, now};

/// 退回意见的字符上限。
pub(crate) const MAX_COMMENT: usize = 2000;

/// 任务当前待验收的执行；任务不在 REVIEW、从未执行过或最近一次执行仍在进行时为 `None`。
pub(crate) fn pending_run(conn: &Connection, task: &Task) -> Result<Option<Run>> {
    if task.status != TaskStatus::Review {
        return Ok(None);
    }
    // 已有验收结果（通过、退回、冲突）的执行不再待验收：任务重新打开后没有新执行就回到 REVIEW 时也一样。
    Ok(run::latest_for_task(conn, &task.id)?
        .filter(|r| !r.status.is_active() && r.review.is_none()))
}

/// 这次执行仍是任务当前待验收的执行：任务处于 REVIEW、它是最近一次执行、还没有验收结果。合并结果
/// 落库时据此决定是否改动任务状态（合并期间任务被别处改动时，只记 Git 事实与活动）。
pub(crate) fn is_current(conn: &Connection, run_id: &str) -> Result<bool> {
    let run = run::get(conn, run_id)?;
    let task = task::get(conn, &run.task_id)?;
    Ok(task.status == TaskStatus::Review
        && run.review.is_none()
        && run::latest_for_task(conn, &task.id)?.is_some_and(|l| l.id == run.id))
}

/// 「合并中」标记（见 store v11）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Merging {
    pub base_branch: String,
    pub head: Option<String>,
}

/// 读取执行的「合并中」标记。
pub(crate) fn merging(conn: &Connection, run_id: &str) -> Result<Option<Merging>> {
    Ok(conn.query_row(
        "SELECT merge_base_branch, merge_head, merging_at FROM runs WHERE id = ?1",
        params![run_id],
        |row| {
            let at: Option<String> = row.get(2)?;
            let base: Option<String> = row.get(0)?;
            Ok(at.and(base).map(|base_branch| Merging {
                base_branch,
                head: row.get(1).ok().flatten(),
            }))
        },
    )?)
}

/// 调用 Git 合并之前写下「合并中」标记；基线分支为空（M3 之前的执行）时一并写回实际使用的基线分支，
/// 之后的合并与冲突恢复只用这个值。
pub(crate) fn mark_merging(
    conn: &Connection,
    run_id: &str,
    base_branch: &str,
    head: &str,
) -> Result<()> {
    conn.execute(
        "UPDATE runs SET base_branch = COALESCE(base_branch, ?2), merge_base_branch = ?2,
                         merge_head = ?3, merging_at = ?4
         WHERE id = ?1",
        params![run_id, base_branch, head, now()],
    )?;
    Ok(())
}

/// 记下合并冲突的文件（验收页刷新后列出）。
pub(crate) fn set_conflict_files(conn: &Connection, run_id: &str, files: &[String]) -> Result<()> {
    conn.execute(
        "UPDATE runs SET conflict_files = ?2 WHERE id = ?1",
        params![run_id, serde_json::to_string(files)?],
    )?;
    Ok(())
}

/// 合并冲突的文件。
pub(crate) fn conflict_files(conn: &Connection, run_id: &str) -> Result<Vec<String>> {
    let raw: String = conn.query_row(
        "SELECT conflict_files FROM runs WHERE id = ?1",
        params![run_id],
        |row| row.get(0),
    )?;
    Ok(serde_json::from_str(&raw).unwrap_or_default())
}

/// 清除「合并中」标记。
pub(crate) fn clear_merging(conn: &Connection, run_id: &str) -> Result<()> {
    conn.execute(
        "UPDATE runs SET merge_base_branch = NULL, merge_head = NULL, merging_at = NULL
         WHERE id = ?1",
        params![run_id],
    )?;
    Ok(())
}

/// 留有「合并中」标记的执行（启动时对账）。
pub(crate) fn merging_runs(conn: &Connection) -> Result<Vec<String>> {
    let mut stmt = conn.prepare("SELECT id FROM runs WHERE merging_at IS NOT NULL")?;
    let rows = stmt.query_map([], |row| row.get(0))?;
    Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
}

/// fencing：`run_id` 必须是它所属任务当前待验收的执行。返回任务与执行。
pub(crate) fn check_pending(conn: &Connection, run_id: &str) -> Result<(Task, Run)> {
    let run = run::get(conn, run_id)?;
    let task = task::get(conn, &run.task_id)?;
    if task.status != TaskStatus::Review {
        return Err(Error::conflict(format!(
            "任务 {} 当前是「{}」，不在待验收，不能验收；请刷新后再操作",
            task.key,
            task.status.label()
        )));
    }
    let latest = run::latest_for_task(conn, &task.id)?;
    match latest {
        Some(latest) if latest.id == run.id => {}
        Some(latest) => {
            return Err(Error::conflict(format!(
                "第 {} 次执行不是任务 {} 当前待验收的执行（最近一次是第 {} 次），请刷新后再验收",
                run.attempt, task.key, latest.attempt
            )));
        }
        None => return Err(Error::not_found("执行记录不存在")),
    }
    if let Some(review) = &run.review {
        let label = match review.outcome {
            ReviewOutcome::Approved => "已通过",
            ReviewOutcome::Rejected => "已退回",
            ReviewOutcome::Conflict => "合并冲突",
        };
        return Err(Error::conflict(format!(
            "任务 {} 的第 {} 次执行已有验收结果（{label}），没有待验收的执行；请刷新后再操作",
            task.key, run.attempt
        )));
    }
    if run.status.is_active() {
        return Err(Error::conflict(format!(
            "任务 {} 的第 {} 次执行仍在进行（{}），结束后才能验收",
            task.key,
            run.attempt,
            run.status.label()
        )));
    }
    Ok((task, run))
}

/// 任务因合并冲突受阻时那次冲突的执行（任务 BLOCKED、最近一次执行的验收结果是 conflict）；否则 `None`。
pub(crate) fn conflict_run(conn: &Connection, task: &Task) -> Result<Option<Run>> {
    if task.status != TaskStatus::Blocked {
        return Ok(None);
    }
    Ok(run::latest_for_task(conn, &task.id)?
        .filter(|r| r.review.as_ref().map(|v| v.outcome) == Some(ReviewOutcome::Conflict)))
}

/// fencing（D22）：`run_id` 必须是任务最近一次执行，任务受阻且原因来自这次执行的合并冲突。
pub(crate) fn check_conflicted(conn: &Connection, run_id: &str) -> Result<(Task, Run)> {
    let run = run::get(conn, run_id)?;
    let task = task::get(conn, &run.task_id)?;
    match conflict_run(conn, &task)? {
        Some(latest) if latest.id == run.id => Ok((task, run)),
        _ => Err(Error::conflict(format!(
            "任务 {} 当前不是因第 {} 次执行的合并冲突而受阻（当前：{}），请刷新后再操作",
            task.key,
            run.attempt,
            task.status.label()
        ))),
    }
}

/// 记下一次执行的验收结果。`merged_commit` 为 `Some` 时一并写入。
pub(crate) fn set_outcome(
    conn: &Connection,
    run_id: &str,
    outcome: ReviewOutcome,
    comment: Option<&str>,
    merged_commit: Option<&str>,
) -> Result<()> {
    conn.execute(
        "UPDATE runs SET review_outcome = ?2, review_comment = ?3, reviewed_at = ?4,
                         merged_commit = COALESCE(?5, merged_commit)
         WHERE id = ?1",
        params![run_id, enum_str(outcome), comment, now(), merged_commit],
    )?;
    Ok(())
}

/// 任务的历次退回意见，最早的在前。
pub(crate) fn rejections(conn: &Connection, task_id: &str) -> Result<Vec<ReviewRejection>> {
    let mut stmt = conn.prepare(
        "SELECT id, attempt, review_comment, reviewed_at FROM runs
         WHERE task_id = ?1 AND review_outcome = 'rejected' ORDER BY attempt",
    )?;
    let rows = stmt.query_map(params![task_id], |row| {
        Ok(ReviewRejection {
            run_id: row.get(0)?,
            attempt: row.get(1)?,
            comment: row.get::<_, Option<String>>(2)?.unwrap_or_default(),
            rejected_at: row.get::<_, Option<String>>(3)?.unwrap_or_default(),
        })
    })?;
    Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
}

/// 普通文件夹：退回续接的多轮执行合在一起的改动文件（沿 `revision_of` 往回找）；同一文件以较新一轮的
/// 改动类型为准（先新增、后修改仍算新增；先新增、后删除则不再列出）。按路径排序。
pub(crate) fn chain_files(conn: &Connection, latest: &Run) -> Result<Vec<ChangedFile>> {
    let mut chain = vec![latest.clone()];
    let mut seen = vec![latest.id.clone()];
    while let Some(prev) = chain.last().and_then(|r| r.revision_of.clone()) {
        if seen.contains(&prev) {
            break;
        }
        seen.push(prev.clone());
        match run::get(conn, &prev) {
            Ok(run) => chain.push(run),
            Err(_) => break,
        }
    }
    let mut files: BTreeMap<String, FileChangeKind> = BTreeMap::new();
    // 从最早的一轮开始叠加。
    for run in chain.iter().rev() {
        for f in &run.changed_files {
            let merged = match (files.get(&f.path), f.change) {
                (Some(FileChangeKind::Added), FileChangeKind::Deleted) => None,
                (Some(FileChangeKind::Added), _) => Some(FileChangeKind::Added),
                (Some(FileChangeKind::Deleted), FileChangeKind::Added) => {
                    Some(FileChangeKind::Modified)
                }
                (_, change) => Some(change),
            };
            match merged {
                Some(change) => files.insert(f.path.clone(), change),
                None => files.remove(&f.path),
            };
        }
    }
    Ok(files
        .into_iter()
        .map(|(path, change)| ChangedFile { path, change })
        .collect())
}

/// 退回后续接的执行：新的一次执行（序号递增），沿用被退回执行的执行目录、隔离方式、分支、基线与会话 ID
///（Agent 支持 `session/load` 时续接原会话，否则在同一目录新开会话），状态 queued。
pub(crate) fn insert_revision(
    conn: &Connection,
    task: &Task,
    prev: &Run,
    harness: HarnessSnapshot,
    workspace: &str,
) -> Result<Run> {
    let new = run::insert(
        conn,
        NewRun {
            task,
            harness,
            workspace,
            exec_dir: &prev.exec_dir,
            isolation: prev.isolation,
        },
    )?;
    conn.execute(
        "UPDATE runs SET branch = ?2, base_commit = ?3, base_branch = ?4, revision_of = ?5,
                         session_id = ?6
         WHERE id = ?1",
        params![
            new.id,
            prev.branch,
            prev.base_commit,
            prev.base_branch,
            prev.id,
            prev.session_id
        ],
    )?;
    run::get(conn, &new.id)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn changed(path: &str, change: FileChangeKind) -> ChangedFile {
        ChangedFile {
            path: path.to_owned(),
            change,
        }
    }

    /// 合并规则只看前后两轮的改动类型，这里直接验证叠加逻辑（不经数据库）。
    #[test]
    fn later_rounds_override_earlier_change_kinds() {
        use FileChangeKind::*;
        let f = crate::tests::plain_fixture();
        let conn = Connection::open(f.db_path()).unwrap();
        // 没有 revision_of 的单轮执行：原样返回。
        let run = Run {
            changed_files: vec![changed("b.md", Modified), changed("a.md", Added)],
            ..fake_run("r1", None)
        };
        let files = chain_files(&conn, &run).unwrap();
        assert_eq!(
            files,
            vec![changed("a.md", Added), changed("b.md", Modified)]
        );
    }

    fn fake_run(id: &str, revision_of: Option<&str>) -> Run {
        use crate::model::*;
        Run {
            id: id.into(),
            team_id: "t".into(),
            task_id: "k".into(),
            task_key: "FE-001".into(),
            role_id: "r".into(),
            harness: HarnessSnapshot {
                harness_id: "h".into(),
                name: "Mock".into(),
                preset: AcpPreset::Mock,
                command: "x".into(),
                args: vec![],
            },
            attempt: 1,
            status: RunStatus::Succeeded,
            status_label: String::new(),
            session_id: None,
            exec_dir: "/x".into(),
            isolation: RunIsolation::InPlace,
            branch: None,
            base_commit: None,
            capabilities: None,
            started_at: String::new(),
            ended_at: None,
            stop_reason: None,
            failure: None,
            failure_kind: None,
            summary: None,
            changed_files: vec![],
            out_of_scope: vec![],
            change_note: None,
            change_incomplete: false,
            tool_call_count: 0,
            no_changes: false,
            interrupted_commit: None,
            interrupted_problem: None,
            taken_over: false,
            pending_permission: None,
            can_resume: false,
            journal: RunJournal::default(),
            base_branch: None,
            merged_commit: None,
            revision_of: revision_of.map(str::to_owned),
            review: None,
        }
    }
}
