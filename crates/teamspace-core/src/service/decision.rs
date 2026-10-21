//! 决策记录（M5-02）。
//!
//! 一次决策写两处：
//! - `decisions` 表：结构化保存，供 `list_decisions` 查询，也用于「需要拍板」的消息在决策后消失；
//! - 一条 `decision_recorded` 活动：`role_id` 指向关联岗位，岗位工作日志据此追加一行「决策：…」
//!   （D14：工作日志只追加，不改写既有内容）。
//!
//! 可选把受阻任务恢复到 READY：走现有的就绪条件校验（与 `change_task_status` 同一套规则），
//! 不满足就绪条件时整个决策一起回滚。

use rusqlite::{Connection, OptionalExtension, params};
use serde_json::json;

use crate::error::{Error, Result};
use crate::model::{
    Actor, ChangeTaskStatusInput, Decision, EventKind, ListDecisionsQuery, RecordDecisionInput,
    TaskStatus,
};
use crate::service::event::NewEvent;
use crate::service::task::FileCheck;
use crate::service::{event, message, task, team};
use crate::store::{new_id, now};

/// 决策内容的字符上限。
pub(crate) const MAX_DECISION_TEXT: usize = 2000;

/// 记录一条决策。
///
/// `files` 是关联任务的文件存在情况（预先在工作区里扫好），恢复任务时用它做就绪条件校验。
pub(crate) fn record(
    conn: &Connection,
    input: &RecordDecisionInput,
    files: &FileCheck,
) -> Result<Decision> {
    let team = team::get_active(conn, &input.team_id)?;
    let text = crate::service::text("决策内容", &input.text, MAX_DECISION_TEXT, true)?;

    let linked_task = match input.task_id.as_deref() {
        Some(id) => {
            let task = task::get(conn, id)?;
            if task.team_id != team.id {
                return Err(Error::invalid("关联任务不在该团队中"));
            }
            Some(task)
        }
        None => None,
    };
    let linked_message = match input.message_id.as_deref() {
        Some(id) => {
            let message = message::get(conn, id)?;
            if message.team_id != team.id {
                return Err(Error::invalid("关联消息不在该团队中"));
            }
            Some(message)
        }
        None => None,
    };

    // 关联岗位：任务取负责岗位；消息取发出方岗位（岗位发来的消息），否则取接收岗位。
    let role_id = linked_task
        .as_ref()
        .map(|t| t.owner_role_id.clone())
        .or_else(|| {
            linked_message.as_ref().map(|m| {
                m.sender_role_id
                    .clone()
                    .unwrap_or_else(|| m.to_role_id.clone())
            })
        });

    let resume = input.resume_task.unwrap_or(false);
    if resume {
        let Some(task) = &linked_task else {
            return Err(Error::invalid("请指定要恢复的任务"));
        };
        if task.status != TaskStatus::Blocked {
            return Err(Error::conflict(format!(
                "任务 {} 当前是「{}」，不是「受阻」，无需恢复",
                task.key,
                task.status.label()
            )));
        }
    }

    let id = new_id();
    let created_at = now();
    conn.execute(
        "INSERT INTO decisions (id, team_id, task_id, message_id, role_id, text, resumed_task,
                                created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
        params![
            id,
            team.id,
            linked_task.as_ref().map(|t| t.id.clone()),
            linked_message.as_ref().map(|m| m.id.clone()),
            role_id,
            text,
            i64::from(resume),
            created_at,
        ],
    )?;

    // 决策先记活动（工作日志里「决策：…」在前），再恢复任务（状态变化在后）。
    event::record_as(
        conn,
        Actor::Human,
        NewEvent {
            team_id: &team.id,
            task_id: linked_task.as_ref().map(|t| t.id.as_str()),
            role_id: role_id.as_deref(),
            kind: EventKind::DecisionRecorded,
            summary: format!("决策：{text}"),
            data: json!({
                "decisionId": id,
                "text": text,
                "messageId": linked_message.as_ref().map(|m| m.id.as_str()),
            }),
        },
    )?;

    if resume {
        let task = linked_task.as_ref().expect("恢复任务时必有任务");
        task::change_status(
            conn,
            ChangeTaskStatusInput {
                task_id: task.id.clone(),
                to: TaskStatus::Ready,
                reason: None,
            },
            files,
            None,
            Actor::Human,
        )?;
    }

    Ok(Decision {
        id,
        team_id: team.id,
        task_id: linked_task.as_ref().map(|t| t.id.clone()),
        task_key: linked_task.as_ref().map(|t| t.key.clone()),
        message_id: linked_message.as_ref().map(|m| m.id.clone()),
        thread_id: linked_message.as_ref().map(|m| m.thread_id.clone()),
        role_id,
        role_name: role_name(conn, &linked_task, &linked_message),
        text,
        resumed_task: resume,
        created_at,
    })
}

/// 决策列表：按团队（必填）、任务与消息筛选，最近的在前。
pub(crate) fn list(conn: &Connection, query: &ListDecisionsQuery) -> Result<Vec<Decision>> {
    team::get_active(conn, &query.team_id)?;
    let mut sql = String::from(
        "SELECT d.id, d.team_id, d.task_id, t.key, d.message_id, m.thread_id, d.role_id, r.name,
                d.text, d.resumed_task, d.created_at
         FROM decisions d
         LEFT JOIN tasks t ON t.id = d.task_id
         LEFT JOIN messages m ON m.id = d.message_id
         LEFT JOIN roles r ON r.id = d.role_id
         WHERE d.team_id = ?1",
    );
    let mut args: Vec<Box<dyn rusqlite::ToSql>> = vec![Box::new(query.team_id.clone())];
    if let Some(task_id) = query.task_id.as_deref() {
        sql.push_str(&format!(" AND d.task_id = ?{}", args.len() + 1));
        args.push(Box::new(task_id.to_owned()));
    }
    if let Some(message_id) = query.message_id.as_deref() {
        sql.push_str(&format!(" AND d.message_id = ?{}", args.len() + 1));
        args.push(Box::new(message_id.to_owned()));
    }
    sql.push_str(" ORDER BY d.created_at DESC, d.id DESC");
    let mut stmt = conn.prepare(&sql)?;
    let refs: Vec<&dyn rusqlite::ToSql> = args.iter().map(|a| a.as_ref()).collect();
    let rows = stmt
        .query_map(refs.as_slice(), from_row)?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(rows)
}

/// 团队里已有决策的消息 id（「需要拍板」的消息有了决策就不再出现）。
pub(crate) fn decided_messages(conn: &Connection, team_id: &str) -> Result<Vec<String>> {
    let mut stmt = conn.prepare(
        "SELECT message_id FROM decisions WHERE team_id = ?1 AND message_id IS NOT NULL",
    )?;
    let rows = stmt.query_map(params![team_id], |row| row.get::<_, String>(0))?;
    Ok(rows.collect::<rusqlite::Result<_>>()?)
}

/// 关联岗位的名字（任务取负责岗位，消息取发出方岗位，否则取接收岗位）。
fn role_name(
    conn: &Connection,
    linked_task: &Option<crate::model::Task>,
    linked_message: &Option<crate::model::Message>,
) -> Option<String> {
    let role_id = linked_task
        .as_ref()
        .map(|t| t.owner_role_id.clone())
        .or_else(|| {
            linked_message.as_ref().map(|m| {
                m.sender_role_id
                    .clone()
                    .unwrap_or_else(|| m.to_role_id.clone())
            })
        })?;
    conn.query_row(
        "SELECT name FROM roles WHERE id = ?1",
        params![role_id],
        |row| row.get::<_, String>(0),
    )
    .optional()
    .ok()
    .flatten()
}

fn from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<Decision> {
    Ok(Decision {
        id: row.get(0)?,
        team_id: row.get(1)?,
        task_id: row.get(2)?,
        task_key: row.get(3)?,
        message_id: row.get(4)?,
        thread_id: row.get(5)?,
        role_id: row.get(6)?,
        role_name: row.get(7)?,
        text: row.get(8)?,
        resumed_task: row.get::<_, i64>(9)? != 0,
        created_at: row.get(10)?,
    })
}
