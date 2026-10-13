//! 活动记录：写入与游标分页。

use rusqlite::{Connection, params};
use serde_json::Value;

use crate::error::{Error, Result};
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

/// 追加一条活动（actor 为 human）。
pub(crate) fn record(conn: &Connection, event: NewEvent<'_>) -> Result<()> {
    record_as(conn, Actor::Human, event)
}

/// 以指定操作者追加一条活动。
pub(crate) fn record_as(conn: &Connection, actor: Actor, event: NewEvent<'_>) -> Result<()> {
    conn.execute(
        "INSERT INTO events (id, team_id, task_id, role_id, kind, actor, summary, data, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
        params![
            new_id(),
            event.team_id,
            event.task_id,
            event.role_id,
            enum_str(event.kind),
            enum_str(actor),
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
        "SELECT {COLS} FROM events WHERE team_id = ?1 ORDER BY seq DESC LIMIT ?2"
    ))?;
    let rows = stmt.query_map(params![team_id, limit], from_row)?;
    Ok(rows.collect::<rusqlite::Result<_>>()?)
}

/// 与某任务相关的全部活动，时间倒序。
pub(crate) fn for_task(conn: &Connection, task_id: &str) -> Result<Vec<Event>> {
    let mut stmt = conn.prepare(&format!(
        "SELECT {COLS} FROM events WHERE task_id = ?1 ORDER BY seq DESC"
    ))?;
    let rows = stmt.query_map(params![task_id], from_row)?;
    Ok(rows.collect::<rusqlite::Result<_>>()?)
}

/// 按团队分页，按写入顺序倒序。游标为上一页最后一条的 `seq`（十进制字符串，对前端不透明）。
/// 有无游标分成两条查询，均可直接走 `events_team (team_id, seq)` 索引。
pub(crate) fn page(conn: &Connection, query: &EventQuery) -> Result<EventPage> {
    super::team::get_active(conn, &query.team_id)?;
    let limit = query.limit.unwrap_or(DEFAULT_PAGE).clamp(1, MAX_PAGE);
    let fetch = i64::from(limit) + 1;
    let rows: Vec<(i64, Event)> = match &query.cursor {
        None => {
            let mut stmt = conn.prepare_cached(&format!(
                "SELECT seq, {COLS} FROM events WHERE team_id = ?1 ORDER BY seq DESC LIMIT ?2"
            ))?;
            stmt.query_map(params![query.team_id, fetch], with_seq)?
                .collect::<rusqlite::Result<_>>()?
        }
        Some(cursor) => {
            let before: i64 = cursor
                .parse()
                .map_err(|_| Error::invalid("活动分页游标无效"))?;
            let mut stmt = conn.prepare_cached(&format!(
                "SELECT seq, {COLS} FROM events WHERE team_id = ?1 AND seq < ?2
                 ORDER BY seq DESC LIMIT ?3"
            ))?;
            stmt.query_map(params![query.team_id, before, fetch], with_seq)?
                .collect::<rusqlite::Result<_>>()?
        }
    };
    let has_more = rows.len() > limit as usize;
    let mut events = Vec::with_capacity(rows.len());
    let mut last_seq = None;
    for (seq, event) in rows.into_iter().take(limit as usize) {
        last_seq = Some(seq);
        events.push(event);
    }
    Ok(EventPage {
        events,
        next_cursor: if has_more {
            last_seq.map(|seq| seq.to_string())
        } else {
            None
        },
    })
}

fn with_seq(row: &rusqlite::Row<'_>) -> rusqlite::Result<(i64, Event)> {
    let seq = row.get(0)?;
    // 其余列与 COLS 顺序一致，整体偏移 1。
    let data: String = row.get(8)?;
    Ok((
        seq,
        Event {
            id: row.get(1)?,
            team_id: row.get(2)?,
            task_id: row.get(3)?,
            role_id: row.get(4)?,
            kind: enum_col(row, 5)?,
            actor: enum_col(row, 6)?,
            summary: row.get(7)?,
            data: serde_json::from_str(&data).unwrap_or(Value::Null),
            created_at: row.get(9)?,
        },
    ))
}

#[cfg(test)]
mod tests {
    use crate::store;

    fn plan(conn: &rusqlite::Connection, sql: &str) -> String {
        let mut stmt = conn.prepare(&format!("EXPLAIN QUERY PLAN {sql}")).unwrap();
        stmt.query_map([], |r| r.get::<_, String>(3))
            .unwrap()
            .collect::<rusqlite::Result<Vec<_>>>()
            .unwrap()
            .join("; ")
    }

    #[test]
    fn page_queries_use_the_team_index_without_sorting() {
        let dir = tempfile::tempdir().unwrap();
        let conn = store::open(&dir.path().join("t.db")).unwrap();
        for sql in [
            "SELECT * FROM events WHERE team_id = 't' ORDER BY seq DESC LIMIT 51",
            "SELECT * FROM events WHERE team_id = 't' AND seq < 10 ORDER BY seq DESC LIMIT 51",
        ] {
            let plan = plan(&conn, sql);
            assert!(plan.contains("events_team"), "{plan}");
            assert!(!plan.contains("TEMP B-TREE"), "{plan}");
        }
    }
}
