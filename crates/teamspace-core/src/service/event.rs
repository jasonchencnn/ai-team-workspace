//! 活动记录：写入与游标分页。

use rusqlite::{Connection, params};
use serde_json::Value;

use crate::error::Result;
use crate::model::{Actor, Event, EventKind, EventPage, EventQuery};
use crate::store::{enum_col, enum_str, new_id, now};

pub(crate) const DEFAULT_PAGE: u32 = 50;
pub(crate) const MAX_PAGE: u32 = 200;
pub(crate) const RECENT_LIMIT: u32 = 10;

const COLS: &str = "id, team_id, task_id, role_id, kind, actor, summary, data, created_at";

/// 一条待写入的活动。
pub(crate) struct NewEvent<'a> {
    pub team_id: &'a str,
    pub task_id: Option<&'a str>,
    pub role_id: Option<&'a str>,
    pub kind: EventKind,
    pub summary: String,
    pub data: Value,
}

/// 追加一条活动（actor 固定为 human）。
pub(crate) fn record(conn: &Connection, event: NewEvent<'_>) -> Result<()> {
    conn.execute(
        "INSERT INTO events (id, team_id, task_id, role_id, kind, actor, summary, data, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
        params![
            new_id(),
            event.team_id,
            event.task_id,
            event.role_id,
            enum_str(event.kind),
            enum_str(Actor::Human),
            event.summary,
            event.data.to_string(),
            now(),
        ],
    )?;
    Ok(())
}

fn from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<Event> {
    let data: String = row.get(7)?;
    Ok(Event {
        id: row.get(0)?,
        team_id: row.get(1)?,
        task_id: row.get(2)?,
        role_id: row.get(3)?,
        kind: enum_col(row, 4)?,
        actor: enum_col(row, 5)?,
        summary: row.get(6)?,
        data: serde_json::from_str(&data).unwrap_or(Value::Null),
        created_at: row.get(8)?,
    })
}

/// 团队最近的活动，时间倒序。
pub(crate) fn recent(conn: &Connection, team_id: &str, limit: u32) -> Result<Vec<Event>> {
    let mut stmt = conn.prepare(&format!(
        "SELECT {COLS} FROM events WHERE team_id = ?1 ORDER BY id DESC LIMIT ?2"
    ))?;
    let rows = stmt.query_map(params![team_id, limit], from_row)?;
    Ok(rows.collect::<rusqlite::Result<_>>()?)
}

/// 与某任务相关的全部活动，时间倒序。
pub(crate) fn for_task(conn: &Connection, task_id: &str) -> Result<Vec<Event>> {
    let mut stmt = conn.prepare(&format!(
        "SELECT {COLS} FROM events WHERE task_id = ?1 ORDER BY id DESC"
    ))?;
    let rows = stmt.query_map(params![task_id], from_row)?;
    Ok(rows.collect::<rusqlite::Result<_>>()?)
}

/// 按团队分页；游标为上一页最后一条的 id（UUID v7 按时间有序）。
pub(crate) fn page(conn: &Connection, query: &EventQuery) -> Result<EventPage> {
    super::team::get_active(conn, &query.team_id)?;
    let limit = query.limit.unwrap_or(DEFAULT_PAGE).clamp(1, MAX_PAGE);
    let mut stmt = conn.prepare(&format!(
        "SELECT {COLS} FROM events
         WHERE team_id = ?1 AND (?2 IS NULL OR id < ?2)
         ORDER BY id DESC LIMIT ?3"
    ))?;
    let mut events: Vec<Event> = stmt
        .query_map(params![query.team_id, query.cursor, limit + 1], from_row)?
        .collect::<rusqlite::Result<_>>()?;
    let next_cursor = if events.len() > limit as usize {
        events.truncate(limit as usize);
        events.last().map(|e| e.id.clone())
    } else {
        None
    };
    Ok(EventPage {
        events,
        next_cursor,
    })
}
