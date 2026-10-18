//! 前置任务完成后的直接下游推进，以及重新打开时的下游提示。

use std::collections::HashMap;
use std::path::Path;

use rusqlite::{Connection, params};
use serde_json::json;

use super::event::{self, NewEvent};
use super::{task, team};
use crate::error::{Error, Result};
use crate::model::{Actor, EventKind, FileRef, TaskStatus};
use crate::store::now;

pub(crate) struct FileCheckPlan {
    workspace: String,
    candidates: Vec<(String, Vec<FileRef>)>,
}

pub(crate) struct FileChecks {
    workspace: String,
    candidates: HashMap<String, (Vec<FileRef>, task::FileCheck)>,
}

impl FileCheckPlan {
    pub(crate) fn scan(self) -> FileChecks {
        let candidates = self
            .candidates
            .into_iter()
            .map(|(id, refs)| {
                let files = task::FileCheck::scan(Path::new(&self.workspace), &refs);
                (id, (refs, files))
            })
            .collect();
        FileChecks {
            workspace: self.workspace,
            candidates,
        }
    }
}

pub(crate) fn file_check_plan(conn: &Connection, task_id: &str) -> Result<FileCheckPlan> {
    let task = task::get(conn, task_id)?;
    let workspace = team::get_active(conn, &task.team_id)?.workspace_path;
    let candidates = candidate_ids(conn, task_id, &task.team_id)?
        .into_iter()
        .map(|id| task::get(conn, &id).map(|task| (id, task.file_refs)))
        .collect::<Result<_>>()?;
    Ok(FileCheckPlan {
        workspace,
        candidates,
    })
}

fn candidate_ids(conn: &Connection, task_id: &str, team_id: &str) -> Result<Vec<String>> {
    let mut stmt = conn.prepare(
        "SELECT t.id FROM tasks t JOIN task_dependencies d ON d.task_id = t.id
         WHERE d.depends_on_id = ?1 AND t.team_id = ?2 AND t.status = 'todo'
         ORDER BY t.key, t.id",
    )?;
    Ok(stmt
        .query_map(params![task_id, team_id], |row| row.get(0))?
        .collect::<rusqlite::Result<_>>()?)
}

/// 在调用方的事务内推进直接下游，只使用锁外文件检查结果；快照失效时回滚，由调用方重试。
pub(crate) fn advance_dependents(
    conn: &Connection,
    done_task_id: &str,
    files: &FileChecks,
) -> Result<Vec<String>> {
    let done = task::get(conn, done_task_id)?;
    if done.status != TaskStatus::Done {
        return Ok(Vec::new());
    }
    let workspace = team::get_active(conn, &done.team_id)?.workspace_path;
    if workspace != files.workspace {
        return Err(Error::conflict("工作空间已改变，请重试完成任务"));
    }
    let mut advanced = Vec::new();
    for id in candidate_ids(conn, done_task_id, &done.team_id)? {
        let dependent = task::get(conn, &id)?;
        let Some((refs, checked)) = files.candidates.get(&id) else {
            return Err(Error::conflict("下游任务已改变，请重试完成任务"));
        };
        if refs != &dependent.file_refs {
            return Err(Error::conflict("下游引用文件已改变，请重试完成任务"));
        }
        if !task::readiness(
            conn,
            &dependent.owner_role_id,
            &dependent.file_refs,
            &dependent.depends_on,
            checked,
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
