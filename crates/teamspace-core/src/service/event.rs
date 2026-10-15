//! 活动记录：写入与游标分页。

use rusqlite::types::ToSql;
use rusqlite::{Connection, params};
use serde_json::Value;
use time::OffsetDateTime;
use time::UtcOffset;
use time::format_description::well_known::Rfc3339;

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

/// 把 RFC3339 时间解析成活动记录使用的固定格式（UTC、毫秒精度，与 [`now`] 一致）。
///
/// 归一化是必要的：`created_at` 恒为 `…T%H:%M:%S.mmmZ`，而调用方可能传不带毫秒或带时区偏移的
/// RFC3339（如 `2026-10-15T02:00:00Z`）。统一格式后，时间比较才能直接用字符串比较
/// （与本文件的排序和分页口径一致），不会因为缺小数位而错判先后。
fn instant(raw: &str, field: &str) -> Result<String> {
    let parsed = OffsetDateTime::parse(raw, &Rfc3339)
        .map_err(|_| {
            Error::invalid(format!(
                "活动筛选的{field}不是合法的 RFC3339 时间：{raw}（示例 2026-10-15T02:00:00.000Z）"
            ))
        })?
        // 统一换算到 UTC 再格式化：带时区偏移的输入（如 `+08:00`）否则会按本地小时落盘，
        // 与 `created_at` 的 UTC 串差一个时区。
        .to_offset(UtcOffset::UTC);
    Ok(format!(
        "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}.{:03}Z",
        parsed.year(),
        u8::from(parsed.month()),
        parsed.day(),
        parsed.hour(),
        parsed.minute(),
        parsed.second(),
        parsed.millisecond(),
    ))
}

/// 按团队分页，按写入顺序倒序。游标为上一页最后一条的 `seq`（十进制字符串，对前端不透明）。
///
/// 筛选条件（任务、岗位、种类、时间范围）与游标一起构成 WHERE：游标语义不变，仍然是
/// 「比上一页更早的记录」，因此筛选条件在翻页时不会丢也不会重。任务或岗位不属于该团队时
/// 与 `team_id` 条件互斥，结果为空而不是错误。
pub(crate) fn page(conn: &Connection, query: &EventQuery) -> Result<EventPage> {
    super::team::get_active(conn, &query.team_id)?;
    let limit = query.limit.unwrap_or(DEFAULT_PAGE).clamp(1, MAX_PAGE);
    let since = query
        .since
        .as_deref()
        .map(|raw| instant(raw, "开始时间 since"))
        .transpose()?;
    let until = query
        .until
        .as_deref()
        .map(|raw| instant(raw, "结束时间 until"))
        .transpose()?;

    let mut args: Vec<Box<dyn ToSql>> = vec![Box::new(query.team_id.clone())];
    let mut clauses: Vec<String> = Vec::new();
    if let Some(task_id) = query.task_id.as_deref() {
        clauses.push(format!("task_id = ?{}", args.len() + 1));
        args.push(Box::new(task_id.to_owned()));
    }
    if let Some(role_id) = query.role_id.as_deref() {
        clauses.push(format!("role_id = ?{}", args.len() + 1));
        args.push(Box::new(role_id.to_owned()));
    }
    if let Some(kinds) = query.kinds.as_deref().filter(|k| !k.is_empty()) {
        let start = args.len() + 1;
        let placeholders: Vec<String> = (0..kinds.len())
            .map(|i| format!("?{}", start + i))
            .collect();
        for kind in kinds {
            args.push(Box::new(enum_str(*kind)));
        }
        clauses.push(format!("kind IN ({})", placeholders.join(", ")));
    }
    // `since` 含、`until` 不含，两者都是归一化后的定长字符串，可直接比较。
    if let Some(since) = since {
        clauses.push(format!("created_at >= ?{}", args.len() + 1));
        args.push(Box::new(since));
    }
    if let Some(until) = until {
        clauses.push(format!("created_at < ?{}", args.len() + 1));
        args.push(Box::new(until));
    }
    if let Some(cursor) = query.cursor.as_deref() {
        let before: i64 = cursor
            .parse()
            .map_err(|_| Error::invalid("活动分页游标无效"))?;
        clauses.push(format!("seq < ?{}", args.len() + 1));
        args.push(Box::new(before));
    }
    // 多取一条用来判断还有没有下一页。
    let limit_placeholder = format!("?{}", args.len() + 1);
    args.push(Box::new(i64::from(limit) + 1));

    let where_clause = if clauses.is_empty() {
        String::new()
    } else {
        format!(" AND {}", clauses.join(" AND "))
    };
    let sql = format!(
        "SELECT seq, {COLS} FROM events WHERE team_id = ?1{where_clause}
         ORDER BY seq DESC LIMIT {limit_placeholder}"
    );
    let mut stmt = conn.prepare_cached(&sql)?;
    let rows: Vec<(i64, Event)> = stmt
        .query_map(rusqlite::params_from_iter(args.iter()), with_seq)?
        .collect::<rusqlite::Result<_>>()?;

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

    /// 按岗位筛选要走 v9 建的 `events_role`，否则 1 万条活动时每次筛选都是全表扫描。
    #[test]
    fn role_filter_uses_the_role_index() {
        let dir = tempfile::tempdir().unwrap();
        let conn = store::open(&dir.path().join("t.db")).unwrap();
        let plan = plan(
            &conn,
            "SELECT * FROM events WHERE team_id = 't' AND role_id = 'r' ORDER BY seq DESC LIMIT 51",
        );
        assert!(plan.contains("events_role"), "{plan}");
        assert!(!plan.contains("TEMP B-TREE"), "{plan}");
    }
}
