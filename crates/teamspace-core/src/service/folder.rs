//! 岗位文件夹的数据库部分：读出同步计划、写回同步结果、生成工作日志条目、改名前的冲突检查数据。
//!
//! 文件系统操作在 [`crate::role_folder`]，由 `api` 层在数据库锁之外执行。
//!
//! 工作日志与活动记录的对应：`roles.journaled_seq` 记录已写入日志的最后一条活动序号，每次同步把
//! 序号更大的相关活动追加到日志，写入成功后再推进序号。因此写入失败（工作空间不可用等）不会丢日志，
//! 恢复后下一次同步自动补写；迁移前已有的岗位（序号为 0）首次同步时会补写全部历史。
//! 「相关活动」：`role_id` 为该岗位的活动（岗位变更、绑定、该岗位负责任务的创建/编辑/状态变化、
//! 文件夹问题），以及把任务从该岗位改派走的任务编辑。`role_created` 不重复写入，岗位创建由日志文件
//! 首行（按岗位创建时间）记录，团队初始岗位也因此有创建记录。

use std::path::PathBuf;

use rusqlite::{Connection, OptionalExtension, params};
use serde_json::{Value, json};

use super::event::{self, NewEvent};
use super::harness;
use crate::error::{Error, Result};
use crate::model::{EventKind, Role, Run, RunJournal};
use crate::role_folder::{self, BoundHarness, JournalEntry, RolePlan, SyncOutcome};
use crate::store::enum_col;

const ROLE_COLS: &str = "r.id, r.team_id, r.name, r.code, r.task_prefix, r.description, \
    r.guidelines, r.is_leader, r.harness_id, r.created_at, r.updated_at, r.archived_at, \
    r.folder, r.folder_problem, r.journaled_seq";

fn role_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<Role> {
    Ok(Role {
        id: row.get(0)?,
        team_id: row.get(1)?,
        name: row.get(2)?,
        code: row.get(3)?,
        task_prefix: row.get(4)?,
        description: row.get(5)?,
        guidelines: row.get(6)?,
        is_leader: row.get(7)?,
        harness_id: row.get(8)?,
        created_at: row.get(9)?,
        updated_at: row.get(10)?,
        archived_at: row.get(11)?,
    })
}

/// 岗位（含已归档，所属团队须未归档）所在的团队 ID；否则 not_found。
pub(crate) fn team_of(conn: &Connection, role_id: &str) -> Result<String> {
    conn.query_row(
        "SELECT team_id FROM roles WHERE id = ?1
         AND team_id IN (SELECT id FROM teams WHERE archived_at IS NULL)",
        params![role_id],
        |row| row.get(0),
    )
    .optional()?
    .ok_or_else(|| Error::not_found("岗位不存在"))
}

/// 岗位文件夹的登记信息：工作空间路径、文件夹相对路径、最近的问题。
pub(crate) fn location(
    conn: &Connection,
    role_id: &str,
) -> Result<(PathBuf, Option<String>, Option<String>)> {
    conn.query_row(
        "SELECT t.workspace_path, r.folder, r.folder_problem FROM roles r
         JOIN teams t ON t.id = r.team_id
         WHERE r.id = ?1 AND t.archived_at IS NULL",
        params![role_id],
        |row| {
            Ok((
                PathBuf::from(row.get::<_, String>(0)?),
                row.get(1)?,
                row.get(2)?,
            ))
        },
    )
    .optional()?
    .ok_or_else(|| Error::not_found("岗位不存在"))
}

/// 同一工作空间中（任何团队、含已归档岗位）除 `role_id` 外的岗位已登记的文件夹。
fn other_folders(conn: &Connection, workspace: &str, role_id: &str) -> Result<Vec<String>> {
    let mut stmt = conn.prepare_cached(
        "SELECT r.folder FROM roles r JOIN teams t ON t.id = r.team_id
         WHERE t.workspace_path = ?1 AND r.id <> ?2 AND r.folder IS NOT NULL",
    )?;
    let rows = stmt.query_map(params![workspace, role_id], |row| row.get(0))?;
    Ok(rows.collect::<rusqlite::Result<_>>()?)
}

/// 改名需要移动文件夹时，返回（工作空间、目标相对路径、其他岗位的文件夹）供锁外检查；
/// 岗位还没有文件夹或新名称仍对应当前文件夹时返回 `None`。
pub(crate) fn rename_target(
    conn: &Connection,
    role_id: &str,
    new_name: &str,
) -> Result<Option<(PathBuf, String, Vec<String>)>> {
    let row: Option<(String, String, Option<String>)> = conn
        .query_row(
            "SELECT t.workspace_path, r.code, r.folder FROM roles r JOIN teams t ON t.id = r.team_id
             WHERE r.id = ?1",
            params![role_id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .optional()?;
    let Some((workspace, code, Some(folder))) = row else {
        return Ok(None);
    };
    if new_name.trim().is_empty() {
        // 名称为空交给岗位校验报 invalid。
        return Ok(None);
    }
    let base = role_folder::sanitize_name(new_name.trim(), &code);
    let current = folder.rsplit('/').next().unwrap_or(&folder);
    if role_folder::name_matches(current, &base, &code) {
        return Ok(None);
    }
    let others = other_folders(conn, &workspace, role_id)?;
    Ok(Some((
        PathBuf::from(workspace),
        role_folder::relative(&base),
        others,
    )))
}

/// 读出团队全部岗位（含已归档）的同步计划。
pub(crate) fn plans(conn: &Connection, team_id: &str) -> Result<(PathBuf, Vec<RolePlan>)> {
    let (team_name, workspace): (String, String) = conn
        .query_row(
            "SELECT name, workspace_path FROM teams WHERE id = ?1 AND archived_at IS NULL",
            params![team_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()?
        .ok_or_else(|| Error::not_found("团队不存在或已归档"))?;
    let mut stmt = conn.prepare(&format!(
        "SELECT {ROLE_COLS}, h.name, h.preset, h.command, h.args FROM roles r
         LEFT JOIN harnesses h ON h.id = r.harness_id
         WHERE r.team_id = ?1 ORDER BY r.created_at, r.id"
    ))?;
    let rows = stmt
        .query_map(params![team_id], |row| {
            let harness = match row.get::<_, Option<String>>(15)? {
                Some(name) => Some(BoundHarness {
                    name,
                    preset: enum_col(row, 16)?,
                    command: row.get(17)?,
                    args: serde_json::from_str(&row.get::<_, String>(18)?).unwrap_or_default(),
                }),
                None => None,
            };
            Ok((
                role_from_row(row)?,
                row.get::<_, Option<String>>(12)?,
                row.get::<_, Option<String>>(13)?,
                row.get::<_, i64>(14)?,
                harness,
            ))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    let mut plans = Vec::with_capacity(rows.len());
    for (role, folder, problem, journaled_seq, harness) in rows {
        let entries = journal_entries(conn, &role, journaled_seq)?;
        plans.push(RolePlan {
            others: other_folders(conn, &workspace, &role.id)?,
            team_name: team_name.clone(),
            harness,
            folder,
            problem,
            journaled_seq,
            entries,
            role,
        });
    }
    Ok((PathBuf::from(workspace), plans))
}

/// 序号大于 `after` 的相关活动转成日志条目。
fn journal_entries(conn: &Connection, role: &Role, after: i64) -> Result<Vec<(i64, JournalEntry)>> {
    let mut stmt = conn.prepare_cached(
        "SELECT e.seq, e.kind, e.summary, e.data, e.created_at, t.key FROM events e
         LEFT JOIN tasks t ON t.id = e.task_id
         WHERE e.team_id = ?1 AND e.seq > ?2 AND (e.role_id = ?3
           OR (e.kind = 'task_updated' AND json_extract(e.data, '$.previousOwnerRoleId') = ?3))
         ORDER BY e.seq",
    )?;
    let rows = stmt
        .query_map(params![role.team_id, after, role.id], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                enum_col::<EventKind>(row, 1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, String>(4)?,
                row.get::<_, Option<String>>(5)?,
            ))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    let mut out = Vec::with_capacity(rows.len());
    for (seq, kind, summary, data, at, task_key) in rows {
        let data: Value = serde_json::from_str(&data).unwrap_or(Value::Null);
        let Some(text) = journal_text(conn, &role.id, kind, &summary, &data)? else {
            continue;
        };
        // 摘要以任务编号开头时去掉，避免与条目中的编号重复。
        let text = match &task_key {
            Some(key) => text
                .strip_prefix(key.as_str())
                .map(|rest| rest.trim_start().to_owned())
                .unwrap_or(text),
            None => text,
        };
        // 工作日志留在工作空间里、可能被提交进 Git，写入前统一脱敏（D6）。
        out.push((
            seq,
            JournalEntry {
                at,
                task_key,
                text: crate::redact::text(&text),
            },
        ));
    }
    Ok(out)
}

fn str_field<'a>(data: &'a Value, path: &[&str]) -> Option<&'a str> {
    path.iter()
        .try_fold(data, |value, key| value.get(*key))
        .and_then(Value::as_str)
}

fn harness_name(conn: &Connection, id: Option<&str>) -> Result<Option<String>> {
    Ok(match id {
        Some(id) => harness::get_any(conn, id)?.map(|h| h.name),
        None => None,
    })
}

/// 日志中的中文描述；`None` 表示不写入日志。
fn journal_text(
    conn: &Connection,
    role_id: &str,
    kind: EventKind,
    summary: &str,
    data: &Value,
) -> Result<Option<String>> {
    Ok(Some(match kind {
        EventKind::RoleCreated => return Ok(None),
        EventKind::RoleUpdated => {
            let mut changes = Vec::new();
            let (before, after) = (
                str_field(data, &["before", "name"]),
                str_field(data, &["after", "name"]),
            );
            if let (Some(before), Some(after)) = (before, after)
                && before != after
            {
                changes.push(format!("岗位改名：「{before}」→「{after}」"));
            }
            let (before, after) = (
                str_field(data, &["before", "taskPrefix"]),
                str_field(data, &["after", "taskPrefix"]),
            );
            if let (Some(before), Some(after)) = (before, after)
                && before != after
            {
                changes.push(format!("任务前缀 {before} → {after}"));
            }
            let changed = |field: &str| {
                data.get("changed")
                    .and_then(Value::as_array)
                    .is_some_and(|list| list.iter().any(|v| v.as_str() == Some(field)))
            };
            if changed("description") {
                changes.push("修改职责".to_owned());
            }
            if changed("guidelines") {
                changes.push("修改工作约定".to_owned());
            }
            if changed("isLeader") {
                changes.push("修改 Leader 标记".to_owned());
            }
            if changes.is_empty() {
                summary.to_owned()
            } else {
                format!("编辑岗位：{}", changes.join("；"))
            }
        }
        EventKind::RoleHarnessBound => {
            // 优先用绑定时记下的名称（v3 起写入），旧活动再按 ID 查当前名称。
            let new = match str_field(data, &["harnessName"]) {
                Some(name) => name.to_owned(),
                None => harness_name(conn, str_field(data, &["harnessId"]))?.unwrap_or_default(),
            };
            let old = match str_field(data, &["previousHarnessName"]) {
                Some(name) => Some(name.to_owned()),
                None => harness_name(conn, str_field(data, &["previousHarnessId"]))?,
            };
            match old {
                Some(old) => format!("更换 Harness：「{old}」→「{new}」"),
                None => format!("绑定 Harness「{new}」"),
            }
        }
        EventKind::TaskUpdated => {
            let owner = str_field(data, &["ownerRoleId"]);
            let previous = str_field(data, &["previousOwnerRoleId"]);
            if owner == previous {
                summary.to_owned()
            } else if previous == Some(role_id) {
                format!("{summary}（已从本岗位改派走）")
            } else {
                format!("{summary}（改派到本岗位）")
            }
        }
        EventKind::RunFinished => {
            let mut text = summary.to_owned();
            if let Some(failure) = str_field(data, &["failure"])
                && !data
                    .get("takenOver")
                    .and_then(Value::as_bool)
                    .unwrap_or(false)
            {
                text.push_str(&format!("；原因：{failure}"));
            }
            if let Some(s) = str_field(data, &["summary"]) {
                text.push_str(&format!("；摘要：{s}"));
            }
            let files: Vec<&str> = data
                .get("changedFiles")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .filter_map(Value::as_str)
                .collect();
            if !files.is_empty() {
                let shown: Vec<&str> = files.iter().take(RUN_FILES_IN_JOURNAL).copied().collect();
                text.push_str(&format!("；改动文件：{}", shown.join("、")));
                if files.len() > shown.len() {
                    text.push_str(&format!(" 等 {} 个", files.len()));
                }
            }
            text
        }
        _ => summary.to_owned(),
    }))
}

/// 执行摘要写入工作日志时最多列出的改动文件数。
const RUN_FILES_IN_JOURNAL: usize = 10;

/// 某次执行的完成条目在岗位工作日志里的真实状态：已写入时给出日志里真实的那一行，未写入时给出原因。
///
/// 判据与 [`apply`] 一致：条目对应的活动序号已被 `roles.journaled_seq` 覆盖才算写入成功。写入失败
/// （岗位文件夹不可写等）时序号不推进，条目留在待补写队列里，恢复后由下一次同步补上。
pub(crate) fn run_journal(conn: &Connection, run: &Run) -> Result<RunJournal> {
    let event: Option<(i64, String, String, String)> = conn
        .query_row(
            "SELECT seq, summary, data, created_at FROM events
             WHERE team_id = ?1 AND kind = 'run_finished' AND json_extract(data, '$.runId') = ?2
             ORDER BY seq DESC LIMIT 1",
            params![run.team_id, run.id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
        )
        .optional()?;
    let (journaled_seq, problem): (i64, Option<String>) = conn.query_row(
        "SELECT journaled_seq, folder_problem FROM roles WHERE id = ?1",
        params![run.role_id],
        |row| Ok((row.get(0)?, row.get(1)?)),
    )?;
    let Some((seq, summary, data, at)) = event else {
        return Ok(RunJournal::default());
    };
    let data: Value = serde_json::from_str(&data).unwrap_or(Value::Null);
    let Some(text) = journal_text(conn, &run.role_id, EventKind::RunFinished, &summary, &data)?
    else {
        return Ok(RunJournal::default());
    };
    // 与 [`journal_entries`] 一致：摘要以任务编号开头时去掉（行首已经带编号），描述统一脱敏。
    let text = text
        .strip_prefix(run.task_key.as_str())
        .map(str::trim_start)
        .unwrap_or(&text);
    let entry = JournalEntry {
        at,
        task_key: Some(run.task_key.clone()),
        text: crate::redact::text(text),
    };
    let written = journaled_seq >= seq;
    Ok(RunJournal {
        written,
        line: Some(entry.display()),
        problem: (!written).then(|| {
            problem
                .unwrap_or_else(|| "工作日志尚未写入，岗位文件夹恢复可写后会自动补写。".to_owned())
        }),
    })
}

/// 写回同步结果；问题首次出现或变化时记录一条 `role_folder_error` 活动（同一问题不重复记录）。
pub(crate) fn apply(conn: &Connection, plans: &[RolePlan], outcomes: &[SyncOutcome]) -> Result<()> {
    for (plan, out) in plans.iter().zip(outcomes) {
        if plan.folder == out.folder
            && plan.problem == out.problem
            && plan.journaled_seq == out.journaled_seq
        {
            continue;
        }
        conn.execute(
            "UPDATE roles SET folder = ?2, folder_problem = ?3,
                              journaled_seq = MAX(journaled_seq, ?4)
             WHERE id = ?1",
            params![plan.role.id, out.folder, out.problem, out.journaled_seq],
        )?;
        if let Some(problem) = &out.problem
            && plan.problem.as_ref() != Some(problem)
        {
            event::record(
                conn,
                NewEvent {
                    team_id: &plan.role.team_id,
                    task_id: None,
                    role_id: Some(&plan.role.id),
                    kind: EventKind::RoleFolderError,
                    summary: format!("岗位「{}」的文件夹写入失败：{problem}", plan.role.name),
                    data: json!({ "folder": out.folder, "problem": problem }),
                },
            )?;
        }
    }
    Ok(())
}
