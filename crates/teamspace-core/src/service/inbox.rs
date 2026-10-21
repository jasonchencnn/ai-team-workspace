//! 待我处理的聚合（M5-02）。
//!
//! 不存「已读」：每次按真实状态重算，事项在处理完成后自然消失。
//!
//! 各类别的判定：
//! - **待验收**：任务状态为 `review`。本次执行没有任何改动时给出「无改动」提示。
//! - **执行失败或受阻**：任务状态为 `blocked`。原因以「执行失败：」开头的是执行失败；
//!   原因里提到合并冲突的单独提示解决冲突的两个动作。
//! - **待答复的权限请求**：状态为 `waiting_permission` 且带待确认请求的执行。
//! - **需要拍板的消息**：岗位发来还没答复的消息，或投递失败 / 过期的消息；
//!   已经为这条消息记过决策的就不再出现。
//! - **Leader 越权记录**：最近 7 天内的 `leader_denied` 活动（越权留痕本身不是待办，
//!   过期后自然消失）。

use rusqlite::{Connection, params};

use crate::error::Result;
use crate::model::{
    Inbox, InboxCategory, InboxCounts, InboxItem, InboxTarget, Message, MessageSender,
    MessageStatus, Task, TaskStatus,
};
use crate::service::{decision, message, run as run_svc, task, team};
use crate::store::{enum_str, now};

/// Leader 越权记录的时间窗：最近 7 天。
pub(crate) const LEADER_DENIED_DAYS: i64 = 7;

/// 执行失败时受阻原因的固定前缀（见 `service::run::FAILED_PREFIX`）。
const FAILED_PREFIX: &str = "执行失败：";

/// 聚合团队当前需要 Human 处理的事项。
pub(crate) fn collect(conn: &Connection, team_id: &str) -> Result<Inbox> {
    team::get_active(conn, team_id)?;
    let mut items: Vec<InboxItem> = Vec::new();

    review_items(conn, team_id, &mut items)?;
    blocked_items(conn, team_id, &mut items)?;
    permission_items(conn, team_id, &mut items)?;
    decision_items(conn, team_id, &mut items)?;
    leader_denied_items(conn, team_id, &mut items)?;

    // 发生时间倒序；同一时间按类别顺序（待验收在前），保证结果稳定。
    items.sort_by(|a, b| {
        b.occurred_at
            .cmp(&a.occurred_at)
            .then_with(|| category_order(a.category).cmp(&category_order(b.category)))
            .then_with(|| a.id.cmp(&b.id))
    });

    let counts = InboxCounts {
        total: items.len() as u32,
        review: count(&items, InboxCategory::Review),
        blocked: count(&items, InboxCategory::Blocked),
        permission: count(&items, InboxCategory::Permission),
        decision: count(&items, InboxCategory::Decision),
        leader_denied: count(&items, InboxCategory::LeaderDenied),
    };
    Ok(Inbox {
        team_id: team_id.to_owned(),
        counts,
        items,
        generated_at: now(),
    })
}

fn count(items: &[InboxItem], category: InboxCategory) -> u32 {
    items
        .iter()
        .filter(|item| item.category == category)
        .count() as u32
}

fn category_order(category: InboxCategory) -> u8 {
    InboxCategory::ALL
        .iter()
        .position(|c| *c == category)
        .unwrap_or(usize::MAX) as u8
}

// ---------------------------------------------------------------- 待验收

fn review_items(conn: &Connection, team_id: &str, items: &mut Vec<InboxItem>) -> Result<()> {
    for task in tasks_in_status(conn, team_id, TaskStatus::Review)? {
        let run = run_svc::latest_for_task(conn, &task.id)?;
        let no_changes = run.as_ref().is_some_and(|r| r.no_changes);
        let impact = match run.as_ref().and_then(|r| r.base_branch.as_deref()) {
            Some(branch) => format!("通过后将合并到 {branch}"),
            None => "通过后任务标记完成".to_owned(),
        };
        let impact = if no_changes {
            format!("{impact}；本次执行没有任何改动，Agent 可能提前结束")
        } else {
            impact
        };
        items.push(InboxItem {
            id: format!("review:{}", task.id),
            category: InboxCategory::Review,
            category_label: InboxCategory::Review.label().to_owned(),
            title: format!("任务 {} 待验收", task.key),
            task_id: Some(task.id.clone()),
            task_key: Some(task.key.clone()),
            message_id: None,
            thread_id: None,
            run_id: run.as_ref().map(|r| r.id.clone()),
            occurred_at: run
                .as_ref()
                .and_then(|r| r.ended_at.clone())
                .unwrap_or_else(|| task.updated_at.clone()),
            impact,
            suggestion: "打开验收页查看改动，通过或退回".to_owned(),
            target: InboxTarget::Task {
                task_id: task.id.clone(),
                task_key: task.key.clone(),
            },
            no_changes,
        });
    }
    Ok(())
}

// ---------------------------------------------------------------- 执行失败或受阻

fn blocked_items(conn: &Connection, team_id: &str, items: &mut Vec<InboxItem>) -> Result<()> {
    for task in tasks_in_status(conn, team_id, TaskStatus::Blocked)? {
        let reason = task.blocked_reason.clone().unwrap_or_default();
        let failed = reason.starts_with(FAILED_PREFIX);
        let conflict = reason.contains("冲突");
        let title = if failed {
            format!("任务 {} 执行失败", task.key)
        } else if conflict {
            format!("任务 {} 合并冲突", task.key)
        } else {
            format!("任务 {} 受阻", task.key)
        };
        let suggestion = if conflict {
            "在验收页「已手动合并，标记完成」或「重新执行」".to_owned()
        } else if failed {
            "查看失败原因，处理后重新执行".to_owned()
        } else {
            "查看受阻原因，处理后把任务恢复为 READY".to_owned()
        };
        items.push(InboxItem {
            id: format!("blocked:{}", task.id),
            category: InboxCategory::Blocked,
            category_label: InboxCategory::Blocked.label().to_owned(),
            title,
            task_id: Some(task.id.clone()),
            task_key: Some(task.key.clone()),
            message_id: None,
            thread_id: None,
            run_id: None,
            occurred_at: task.updated_at.clone(),
            impact: reason,
            suggestion,
            target: InboxTarget::Task {
                task_id: task.id.clone(),
                task_key: task.key.clone(),
            },
            no_changes: false,
        });
    }
    Ok(())
}

// ---------------------------------------------------------------- 待答复的权限请求

fn permission_items(conn: &Connection, team_id: &str, items: &mut Vec<InboxItem>) -> Result<()> {
    for run in run_svc::all_active(conn)? {
        if run.team_id != team_id {
            continue;
        }
        let Some(prompt) = &run.pending_permission else {
            continue;
        };
        items.push(InboxItem {
            id: format!("permission:{}:{}", run.id, prompt.request_id),
            category: InboxCategory::Permission,
            category_label: InboxCategory::Permission.label().to_owned(),
            title: format!("{} 请求确认：{}", run.task_key, prompt.title),
            task_id: Some(run.task_id.clone()),
            task_key: Some(run.task_key.clone()),
            message_id: None,
            thread_id: None,
            run_id: Some(run.id.clone()),
            occurred_at: prompt.requested_at.clone(),
            impact: prompt.reason.clone(),
            suggestion: "在执行面板里答复这次权限请求".to_owned(),
            target: InboxTarget::Permission {
                run_id: run.id.clone(),
                request_id: prompt.request_id.clone(),
            },
            no_changes: false,
        });
    }
    Ok(())
}

// ---------------------------------------------------------------- 需要拍板的消息

fn decision_items(conn: &Connection, team_id: &str, items: &mut Vec<InboxItem>) -> Result<()> {
    let decided = decision::decided_messages(conn, team_id)?;
    let messages = messages_needing_decision(conn, team_id)?;
    for message in messages {
        if decided.contains(&message.id) {
            continue;
        }
        let to_name = role_name_of(conn, &message.to_role_id);
        let from_role = message.sender == MessageSender::Role;
        // 岗位发来的消息报发信岗位；Human / Leader 发的报接收岗位。
        let from_name = message
            .sender_role_id
            .as_deref()
            .map(|id| role_name_of(conn, id))
            .unwrap_or_default();
        let title = if from_role {
            format!("岗位「{from_name}」发来消息，等你拍板")
        } else {
            match message.status {
                MessageStatus::Failed => format!("发给岗位「{to_name}」的消息投递失败"),
                MessageStatus::Expired => format!("发给岗位「{to_name}」的消息已过期"),
                _ => format!("有一条发给岗位「{to_name}」的消息等你处理"),
            }
        };
        let impact = match message.status {
            MessageStatus::Failed => message
                .failure_reason
                .clone()
                .unwrap_or_else(|| "消息投递失败。".to_owned()),
            MessageStatus::Expired => "超过 7 天仍未回复，消息已过期。".to_owned(),
            _ => message.body.clone(),
        };
        items.push(InboxItem {
            id: format!("message:{}", message.id),
            category: InboxCategory::Decision,
            category_label: InboxCategory::Decision.label().to_owned(),
            title,
            task_id: message.task_id.clone(),
            task_key: message
                .task_id
                .as_deref()
                .and_then(|id| task_key_of(conn, id)),
            message_id: Some(message.id.clone()),
            thread_id: Some(message.thread_id.clone()),
            run_id: None,
            occurred_at: message.created_at.clone(),
            impact,
            suggestion: "查看消息内容，记录你的决策".to_owned(),
            target: InboxTarget::Message {
                message_id: message.id.clone(),
                thread_id: message.thread_id.clone(),
            },
            no_changes: false,
        });
    }
    Ok(())
}

/// 需要拍板的消息：岗位发来还没答复的，或投递失败 / 过期的。
fn messages_needing_decision(conn: &Connection, team_id: &str) -> Result<Vec<Message>> {
    let mut stmt = conn.prepare(&format!(
        "SELECT {} FROM messages
         WHERE team_id = ?1
           AND (status IN ('failed', 'expired')
                OR (sender_kind = 'role' AND status <> 'replied'))
         ORDER BY created_at DESC, id",
        message::MESSAGE_COLS,
    ))?;
    let rows = stmt
        .query_map(params![team_id], message::from_row)?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(rows)
}

// ---------------------------------------------------------------- Leader 越权记录

fn leader_denied_items(conn: &Connection, team_id: &str, items: &mut Vec<InboxItem>) -> Result<()> {
    let cutoff = days_ago(LEADER_DENIED_DAYS);
    let mut stmt = conn.prepare(
        "SELECT id, task_id, summary, data, created_at FROM events
         WHERE team_id = ?1 AND kind = 'leader_denied' AND created_at >= ?2
         ORDER BY seq DESC",
    )?;
    let rows = stmt
        .query_map(params![team_id, cutoff], |row| {
            let data: String = row.get(3)?;
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, Option<String>>(1)?,
                row.get::<_, String>(2)?,
                data,
                row.get::<_, String>(4)?,
            ))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    for (id, task_id, summary, data, created_at) in rows {
        let data: serde_json::Value =
            serde_json::from_str(&data).unwrap_or(serde_json::Value::Null);
        let impact = data
            .get("message")
            .and_then(|v| v.as_str())
            .unwrap_or("Leader 请求了权限之外的操作，已被拒绝。")
            .to_owned();
        items.push(InboxItem {
            id: format!("leader_denied:{id}"),
            category: InboxCategory::LeaderDenied,
            category_label: InboxCategory::LeaderDenied.label().to_owned(),
            title: summary,
            task_id: task_id.clone(),
            task_key: task_id.as_deref().and_then(|t| task_key_of(conn, t)),
            message_id: None,
            thread_id: None,
            run_id: None,
            occurred_at: created_at,
            impact,
            suggestion: "在活动页查看越权记录；需要该操作请在 Teamspace 应用里完成".to_owned(),
            target: InboxTarget::Activity { event_id: id },
            no_changes: false,
        });
    }
    Ok(())
}

// ---------------------------------------------------------------- 辅助

/// 团队里处于某个状态的任务。
fn tasks_in_status(conn: &Connection, team_id: &str, status: TaskStatus) -> Result<Vec<Task>> {
    let mut stmt = conn.prepare(
        "SELECT id FROM tasks WHERE team_id = ?1 AND status = ?2 ORDER BY updated_at DESC, id",
    )?;
    let ids = stmt
        .query_map(params![team_id, enum_str(status)], |row| {
            row.get::<_, String>(0)
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    ids.into_iter().map(|id| task::get(conn, &id)).collect()
}

/// 现在往前推若干天（UTC RFC3339，与 `created_at` 同格式，可直接比较）。
fn days_ago(days: i64) -> String {
    let now = time::OffsetDateTime::now_utc();
    let cutoff = now - time::Duration::days(days);
    format!(
        "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}.{:03}Z",
        cutoff.year(),
        u8::from(cutoff.month()),
        cutoff.day(),
        cutoff.hour(),
        cutoff.minute(),
        cutoff.second(),
        cutoff.millisecond(),
    )
}

fn role_name_of(conn: &Connection, role_id: &str) -> String {
    conn.query_row(
        "SELECT name FROM roles WHERE id = ?1",
        params![role_id],
        |row| row.get::<_, String>(0),
    )
    .unwrap_or_default()
}

fn task_key_of(conn: &Connection, task_id: &str) -> Option<String> {
    conn.query_row(
        "SELECT key FROM tasks WHERE id = ?1",
        params![task_id],
        |row| row.get::<_, String>(0),
    )
    .ok()
}
