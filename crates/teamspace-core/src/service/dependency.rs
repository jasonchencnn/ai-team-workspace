//! 前置任务完成后的直接下游推进，以及重新打开时的下游提示。

use std::path::Path;

use rusqlite::{Connection, params};
use serde_json::json;

use super::event::{self, NewEvent};
use super::{task, team};
use crate::error::Result;
use crate::model::{Actor, EventKind, TaskStatus};
use crate::store::now;

/// 在调用方的事务内推进直接下游；文件检查复用就绪校验，日志由提交后的岗位同步写入。
pub(crate) fn advance_dependents(conn: &Connection, done_task_id: &str) -> Result<Vec<String>> {
    let done = task::get(conn, done_task_id)?;
    if done.status != TaskStatus::Done {
        return Ok(Vec::new());
    }
    let workspace = team::get_active(conn, &done.team_id)?.workspace_path;
    let mut stmt = conn.prepare(
        "SELECT t.id FROM tasks t JOIN task_dependencies d ON d.task_id = t.id
         WHERE d.depends_on_id = ?1 AND t.team_id = ?2 AND t.status = 'todo'
         ORDER BY t.key, t.id",
    )?;
    let candidates = stmt
        .query_map(params![done_task_id, done.team_id], |row| {
            row.get::<_, String>(0)
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    let mut advanced = Vec::new();
    for id in candidates {
        let dependent = task::get(conn, &id)?;
        let files = task::FileCheck::scan(Path::new(&workspace), &dependent.file_refs);
        if !task::readiness(
            conn,
            &dependent.owner_role_id,
            &dependent.file_refs,
            &dependent.depends_on,
            &files,
        )?
        .is_empty()
        {
            continue;
        }
        conn.execute(
            "UPDATE tasks SET status = 'ready', updated_at = ?2 WHERE id = ?1",
            params![id, now()],
        )?;
        event::record_as(
            conn,
            Actor::System,
            NewEvent {
                team_id: &dependent.team_id,
                task_id: Some(&id),
                role_id: Some(&dependent.owner_role_id),
                kind: EventKind::TaskStatusChanged,
                summary: format!("前置任务 {} 已完成，{} 自动就绪", done.key, dependent.key),
                data: json!({
                    "from": TaskStatus::Todo,
                    "to": TaskStatus::Ready,
                    "triggerTaskId": done.id,
                    "triggerTaskKey": done.key,
                }),
            },
        )?;
        advanced.push(id);
    }
    Ok(advanced)
}

pub(crate) fn active_dependent_keys(conn: &Connection, task_id: &str) -> Result<Vec<String>> {
    let mut stmt = conn.prepare(
        "SELECT t.key FROM tasks t JOIN task_dependencies d ON d.task_id = t.id
         WHERE d.depends_on_id = ?1 AND t.status IN ('ready', 'doing') ORDER BY t.key, t.id",
    )?;
    Ok(stmt
        .query_map(params![task_id], |row| row.get(0))?
        .collect::<rusqlite::Result<_>>()?)
}
