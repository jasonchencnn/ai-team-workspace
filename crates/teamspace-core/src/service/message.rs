//! 岗位通信的持久化与状态流转（M5-01）。
//!
//! 消息的投递由 [`crate::delivery`] 负责，这里只管数据与规则：
//! - 线程与消息的写入、读取与筛选；
//! - 状态流转：排队 → 投递中 → 已回复 / 失败 / 过期，以及失败与过期后的重发；
//! - 应用重启时把残留的「投递中」退回「排队」；
//! - 超过 7 天仍未回复的消息转「过期」。

use rusqlite::{Connection, OptionalExtension, params};

use crate::error::{Error, Result};
use crate::model::{
    Actor, EventKind, ListThreadsQuery, Message, MessageSender, MessageStatus, MessageThread,
    SendMessageInput, ThreadDetail, ThreadParticipant,
};
use crate::service::event::NewEvent;
use crate::service::{MAX_LONG_TEXT, MAX_REASON, role, task, team};
use crate::store::{enum_col, enum_str, new_id, now};

/// 消息未回复时的有效期：超过 7 天转「过期」。
pub const EXPIRY_DAYS: i64 = 7;
/// 投递说明包里附带的最近消息条数。
pub const CONTEXT_MESSAGES: usize = 10;

/// 线程与消息的列。
const THREAD_COLS: &str = "id, team_id, task_id, subject, participants, created_at, updated_at";
const MESSAGE_COLS: &str = "id, thread_id, team_id, task_id, sender_kind, sender_role_id, \
                           to_role_id, body, status, failure_reason, reply_body, created_at, \
                           delivered_at, replied_at";

/// 发送一条消息。
///
/// `actor` 决定发送方（Human 或 Leader）与活动记录里的操作者。消息进入「排队」，由投递引擎接手。
pub(crate) fn send(conn: &Connection, actor: Actor, input: &SendMessageInput) -> Result<Message> {
    let sender = match actor {
        Actor::Human => MessageSender::Human,
        Actor::Leader => MessageSender::Leader,
        Actor::System => MessageSender::Role,
    };
    let team = team::get_active(conn, &input.team_id)?;
    let target = role::get_active(conn, &input.to_role_id)?;
    if target.team_id != team.id {
        return Err(Error::invalid("接收岗位不在该团队中"));
    }
    let body = crate::service::text("消息正文", &input.body, MAX_LONG_TEXT, true)?;
    let task_id = match input.task_id.as_deref() {
        Some(id) => {
            let linked = task::get(conn, id)?;
            if linked.team_id != team.id {
                return Err(Error::invalid("关联任务不在该团队中"));
            }
            Some(linked.id)
        }
        None => None,
    };

    let thread = match input.thread_id.as_deref() {
        Some(id) => {
            let existing =
                thread_of(conn, id)?.ok_or_else(|| Error::not_found("消息线程不存在"))?;
            if existing.team_id != team.id {
                return Err(Error::invalid("消息线程不在该团队中"));
            }
            Some(existing)
        }
        None => find_thread(conn, &team.id, &target.id, task_id.as_deref())?,
    };

    let (thread, thread_created) = match thread {
        Some(existing) => (existing, false),
        None => {
            let subject = match input.subject.as_deref() {
                Some(text) => crate::service::text("主题", text, 200, true)?,
                None => subject_of(&body),
            };
            (
                create_thread(conn, &team.id, task_id.as_deref(), &subject, &target.id)?,
                true,
            )
        }
    };

    // 参与方：发送方与接收岗位都记上（岗位用角色身份，Human / Leader 只在没有岗位时入列）。
    let sender_participant = ThreadParticipant {
        role_id: match sender {
            MessageSender::Role => Some(target.id.clone()),
            _ => None,
        },
        kind: sender,
        name: match sender {
            MessageSender::Human => "Human".to_owned(),
            MessageSender::Leader => "Leader".to_owned(),
            MessageSender::Role => target.name.clone(),
        },
    };
    let target_participant = ThreadParticipant {
        role_id: Some(target.id.clone()),
        kind: MessageSender::Role,
        name: target.name.clone(),
    };
    add_participants(conn, &thread.id, &[sender_participant, target_participant])?;

    let id = new_id();
    let created_at = now();
    conn.execute(
        "INSERT INTO messages (id, thread_id, team_id, task_id, sender_kind, sender_role_id,
                               to_role_id, body, status, failure_reason, reply_body,
                               created_at, delivered_at, replied_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, 'queued', NULL, NULL, ?9, NULL, NULL)",
        params![
            id,
            thread.id,
            team.id,
            task_id,
            enum_str(sender),
            Option::<String>::None,
            target.id,
            body,
            created_at,
        ],
    )?;
    touch_thread(conn, &thread.id)?;

    let message = get(conn, &id)?;
    crate::service::event::record_as(
        conn,
        actor,
        NewEvent {
            team_id: &team.id,
            task_id: task_id.as_deref(),
            role_id: Some(&target.id),
            kind: EventKind::MessageSent,
            summary: format!("向岗位「{}」发送消息", target.name),
            data: serde_json::json!({
                "messageId": message.id,
                "threadId": thread.id,
                "toRoleId": target.id,
                "sender": enum_str(sender),
                "newThread": thread_created,
            }),
        },
    )?;
    Ok(message)
}

/// 重发一条失败或过期的消息：状态回到「排队」，清掉失败原因，等投递引擎再次接手。
pub(crate) fn retry(conn: &Connection, message_id: &str) -> Result<Message> {
    let message = get(conn, message_id)?;
    if !matches!(
        message.status,
        MessageStatus::Failed | MessageStatus::Expired
    ) {
        return Err(Error::conflict(format!(
            "只有失败或过期的消息可以重发（当前状态：{}）",
            message.status.label()
        )));
    }
    conn.execute(
        "UPDATE messages SET status = 'queued', failure_reason = NULL, delivered_at = NULL,
                             replied_at = NULL, reply_body = NULL
         WHERE id = ?1",
        params![message.id],
    )?;
    touch_thread(conn, &message.thread_id)?;
    let target = role::get_active(conn, &message.to_role_id)?;
    crate::service::event::record_as(
        conn,
        Actor::Human,
        NewEvent {
            team_id: &message.team_id,
            task_id: message.task_id.as_deref(),
            role_id: Some(&target.id),
            kind: EventKind::MessageSent,
            summary: format!("重新发送发给岗位「{}」的消息", target.name),
            data: serde_json::json!({
                "messageId": message.id,
                "threadId": message.thread_id,
                "toRoleId": target.id,
                "retried": true,
            }),
        },
    )?;
    get(conn, message_id)
}

/// 单条消息。
pub(crate) fn get(conn: &Connection, message_id: &str) -> Result<Message> {
    conn.query_row(
        &format!("SELECT {MESSAGE_COLS} FROM messages WHERE id = ?1"),
        params![message_id],
        from_row,
    )
    .optional()?
    .ok_or_else(|| Error::not_found("消息不存在"))
}

/// 线程详情：线程与其中的消息（按时间正序）。
pub(crate) fn thread(conn: &Connection, thread_id: &str) -> Result<ThreadDetail> {
    let thread = thread_of(conn, thread_id)?.ok_or_else(|| Error::not_found("消息线程不存在"))?;
    let mut stmt = conn.prepare(&format!(
        "SELECT {MESSAGE_COLS} FROM messages WHERE thread_id = ?1 ORDER BY created_at, id"
    ))?;
    let messages = stmt
        .query_map(params![thread_id], from_row)?
        .collect::<rusqlite::Result<_>>()?;
    Ok(ThreadDetail { thread, messages })
}

/// 线程列表：按团队（必填）、任务与岗位筛选，按最近更新时间倒序。
pub(crate) fn threads(conn: &Connection, query: &ListThreadsQuery) -> Result<Vec<MessageThread>> {
    team::get_active(conn, &query.team_id)?;
    let mut sql = format!("SELECT {THREAD_COLS} FROM message_threads WHERE team_id = ?1");
    let mut args: Vec<Box<dyn rusqlite::ToSql>> = vec![Box::new(query.team_id.clone())];
    if let Some(task_id) = query.task_id.as_deref() {
        sql.push_str(" AND task_id = ?2");
        args.push(Box::new(task_id.to_owned()));
    }
    sql.push_str(" ORDER BY updated_at DESC, id DESC");
    let mut stmt = conn.prepare(&sql)?;
    let refs: Vec<&dyn rusqlite::ToSql> = args.iter().map(|a| a.as_ref()).collect();
    let rows: Vec<MessageThread> = stmt
        .query_map(refs.as_slice(), from_thread_row)?
        .collect::<rusqlite::Result<_>>()?;
    // 岗位筛选在内存里做：参与方存在 JSON 里，线程数量不大，这样不必依赖 SQL 的 JSON 函数。
    Ok(match query.role_id.as_deref() {
        Some(role_id) => rows
            .into_iter()
            .filter(|t| {
                t.participants
                    .iter()
                    .any(|p| p.role_id.as_deref() == Some(role_id))
            })
            .collect(),
        None => rows,
    })
}

/// 标记消息进入「投递中」。
pub(crate) fn mark_delivering(conn: &Connection, message_id: &str) -> Result<Message> {
    conn.execute(
        "UPDATE messages SET status = 'delivering', delivered_at = ?2 WHERE id = ?1",
        params![message_id, now()],
    )?;
    get(conn, message_id)
}

/// 写回回复：状态转「已回复」，记下回复正文与时间。
pub(crate) fn mark_replied(conn: &Connection, message_id: &str, reply: &str) -> Result<Message> {
    let message = get(conn, message_id)?;
    let reply = crate::service::text("回复正文", reply, MAX_LONG_TEXT, false)?;
    conn.execute(
        "UPDATE messages SET status = 'replied', reply_body = ?2, replied_at = ?3,
                             failure_reason = NULL
         WHERE id = ?1",
        params![message_id, reply, now()],
    )?;
    touch_thread(conn, &message.thread_id)?;
    crate::service::event::record_as(
        conn,
        Actor::System,
        NewEvent {
            team_id: &message.team_id,
            task_id: message.task_id.as_deref(),
            role_id: Some(&message.to_role_id),
            kind: EventKind::MessageReplied,
            summary: "岗位已回复消息".to_owned(),
            data: serde_json::json!({
                "messageId": message.id,
                "threadId": message.thread_id,
                "reply": reply,
            }),
        },
    )?;
    get(conn, message_id)
}

/// 标记投递失败：记下原因。
pub(crate) fn mark_failed(conn: &Connection, message_id: &str, reason: &str) -> Result<Message> {
    let message = get(conn, message_id)?;
    let reason = crate::service::text("失败原因", reason, MAX_REASON, true)?;
    conn.execute(
        "UPDATE messages SET status = 'failed', failure_reason = ?2 WHERE id = ?1",
        params![message_id, reason],
    )?;
    touch_thread(conn, &message.thread_id)?;
    crate::service::event::record_as(
        conn,
        Actor::System,
        NewEvent {
            team_id: &message.team_id,
            task_id: message.task_id.as_deref(),
            role_id: Some(&message.to_role_id),
            kind: EventKind::MessageDeliveryFailed,
            summary: "消息投递失败".to_owned(),
            data: serde_json::json!({
                "messageId": message.id,
                "threadId": message.thread_id,
                "reason": reason,
            }),
        },
    )?;
    get(conn, message_id)
}

/// 标记消息过期。
pub(crate) fn mark_expired(conn: &Connection, message_id: &str) -> Result<Message> {
    let message = get(conn, message_id)?;
    conn.execute(
        "UPDATE messages SET status = 'expired' WHERE id = ?1",
        params![message_id],
    )?;
    touch_thread(conn, &message.thread_id)?;
    crate::service::event::record_as(
        conn,
        Actor::System,
        NewEvent {
            team_id: &message.team_id,
            task_id: message.task_id.as_deref(),
            role_id: Some(&message.to_role_id),
            kind: EventKind::MessageExpired,
            summary: "消息超过 7 天未回复，已过期".to_owned(),
            data: serde_json::json!({
                "messageId": message.id,
                "threadId": message.thread_id,
            }),
        },
    )?;
    get(conn, message_id)
}

/// 把超过期限仍未回复的消息转「过期」，返回被置为过期的消息。
pub(crate) fn expire_stale(conn: &Connection, cutoff: &str) -> Result<Vec<Message>> {
    let mut stmt = conn.prepare(&format!(
        "SELECT {MESSAGE_COLS} FROM messages
         WHERE status IN ('queued', 'delivering') AND created_at < ?1
         ORDER BY created_at"
    ))?;
    let stale: Vec<Message> = stmt
        .query_map(params![cutoff], from_row)?
        .collect::<rusqlite::Result<_>>()?;
    let mut expired: Vec<Message> = Vec::new();
    for message in stale {
        expired.push(mark_expired(conn, &message.id)?);
    }
    Ok(expired)
}

/// 应用重启时把残留的「投递中」退回「排队」，等投递引擎重新接手。返回退回的消息 id。
pub(crate) fn requeue_delivering(conn: &Connection) -> Result<Vec<String>> {
    let ids = {
        let mut stmt = conn
            .prepare("SELECT id FROM messages WHERE status = 'delivering' ORDER BY created_at")?;
        stmt.query_map([], |row| row.get::<_, String>(0))?
            .collect::<rusqlite::Result<Vec<String>>>()?
    };
    for id in &ids {
        conn.execute(
            "UPDATE messages SET status = 'queued', delivered_at = NULL WHERE id = ?1",
            params![id],
        )?;
    }
    Ok(ids)
}

/// 下一条可以投递的消息：状态为「排队」，且接收岗位当前没有正在投递的消息。
///
/// `busy_roles` 是正在投递的岗位 id 集合（同一岗位同时只投递一条）。
pub(crate) fn next_queued(conn: &Connection, busy_roles: &[String]) -> Result<Option<Message>> {
    let mut stmt = conn.prepare(&format!(
        "SELECT {MESSAGE_COLS} FROM messages WHERE status = 'queued' ORDER BY created_at, id"
    ))?;
    let queued = stmt
        .query_map([], from_row)?
        .collect::<rusqlite::Result<Vec<Message>>>()?;
    Ok(queued
        .into_iter()
        .find(|m| !busy_roles.contains(&m.to_role_id)))
}

/// 线程上下文：某个线程里最近的消息（时间正序），供投递说明包附上。
pub(crate) fn context(conn: &Connection, thread_id: &str, limit: usize) -> Result<Vec<Message>> {
    let mut stmt = conn.prepare(&format!(
        "SELECT {MESSAGE_COLS} FROM messages WHERE thread_id = ?1 ORDER BY created_at DESC, id DESC
         LIMIT ?2"
    ))?;
    let mut out = stmt
        .query_map(params![thread_id, limit as i64], from_row)?
        .collect::<rusqlite::Result<Vec<Message>>>()?;
    out.reverse();
    Ok(out)
}

/// 新建线程。
fn create_thread(
    conn: &Connection,
    team_id: &str,
    task_id: Option<&str>,
    subject: &str,
    target_role_id: &str,
) -> Result<MessageThread> {
    let id = new_id();
    let created_at = now();
    conn.execute(
        "INSERT INTO message_threads (id, team_id, task_id, subject, participants, created_at,
                                      updated_at)
         VALUES (?1, ?2, ?3, ?4, '[]', ?5, ?5)",
        params![id, team_id, task_id, subject, created_at],
    )?;
    let target = role::get_active(conn, target_role_id)?;
    add_participants(
        conn,
        &id,
        &[ThreadParticipant {
            role_id: Some(target.id.clone()),
            kind: MessageSender::Role,
            name: target.name.clone(),
        }],
    )?;
    thread_of(conn, &id)?.ok_or_else(|| Error::internal("消息线程写入后读不到"))
}

/// 找可以续用的线程：同一团队、同一接收岗位、同一关联任务（都为空也算相同），取最近更新的那个。
fn find_thread(
    conn: &Connection,
    team_id: &str,
    to_role_id: &str,
    task_id: Option<&str>,
) -> Result<Option<MessageThread>> {
    let all = threads(
        conn,
        &ListThreadsQuery {
            team_id: team_id.to_owned(),
            task_id: task_id.map(str::to_owned),
            role_id: Some(to_role_id.to_owned()),
        },
    )?;
    Ok(all.into_iter().find(|t| t.task_id.as_deref() == task_id))
}

/// 把参与方并进线程（已有的不重复添加）。
fn add_participants(
    conn: &Connection,
    thread_id: &str,
    participants: &[ThreadParticipant],
) -> Result<()> {
    let Some(mut thread) = thread_of(conn, thread_id)? else {
        return Ok(());
    };
    for participant in participants {
        let known = thread
            .participants
            .iter()
            .any(|p| p.role_id == participant.role_id && p.kind == participant.kind);
        if !known {
            thread.participants.push(participant.clone());
        }
    }
    let json = serde_json::to_string(&thread.participants)
        .map_err(|err| Error::internal(format!("无法序列化消息线程的参与方：{err}")))?;
    conn.execute(
        "UPDATE message_threads SET participants = ?2 WHERE id = ?1",
        params![thread_id, json],
    )?;
    Ok(())
}

/// 刷新线程的更新时间（线程列表按它排序）。
fn touch_thread(conn: &Connection, thread_id: &str) -> Result<()> {
    conn.execute(
        "UPDATE message_threads SET updated_at = ?2 WHERE id = ?1",
        params![thread_id, now()],
    )?;
    Ok(())
}

/// 正文的第一行作为默认主题（截断到 100 个字符）。
fn subject_of(body: &str) -> String {
    let first = body.lines().find(|l| !l.trim().is_empty()).unwrap_or("");
    let text = first.trim();
    if text.is_empty() {
        "新消息".to_owned()
    } else {
        text.chars().take(100).collect()
    }
}

/// 按 id 取线程。
fn thread_of(conn: &Connection, thread_id: &str) -> Result<Option<MessageThread>> {
    Ok(conn
        .query_row(
            &format!("SELECT {THREAD_COLS} FROM message_threads WHERE id = ?1"),
            params![thread_id],
            from_thread_row,
        )
        .optional()?)
}

fn from_thread_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<MessageThread> {
    let participants: String = row.get(4)?;
    Ok(MessageThread {
        id: row.get(0)?,
        team_id: row.get(1)?,
        task_id: row.get(2)?,
        subject: row.get(3)?,
        participants: serde_json::from_str(&participants).unwrap_or_default(),
        created_at: row.get(5)?,
        updated_at: row.get(6)?,
    })
}

fn from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<Message> {
    Ok(Message {
        id: row.get(0)?,
        thread_id: row.get(1)?,
        team_id: row.get(2)?,
        task_id: row.get(3)?,
        sender: enum_col(row, 4)?,
        sender_role_id: row.get(5)?,
        to_role_id: row.get(6)?,
        body: row.get(7)?,
        status: enum_col(row, 8)?,
        failure_reason: row.get(9)?,
        reply_body: row.get(10)?,
        created_at: row.get(11)?,
        delivered_at: row.get(12)?,
        replied_at: row.get(13)?,
    })
}
