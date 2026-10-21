//! 性能测试用的数据生成（测试专用，不是正式命令）。
//!
//! 任务上万时，用业务接口一条条创建既慢又会把活动表写满，与真实使用（任务早已创建、
//! 只是量大）也不像。这里直接写库造一批数据，只保证数据自洽：状态合法、依赖只指向更早的
//! 任务（不成环）、受阻任务带原因。
//!
//! 内核基准测试与端到端测试共用这一份实现，避免两边各写一套。

use rusqlite::{Connection, params};

use crate::error::Result;
use crate::model::TaskStatus;
use crate::store::enum_str;

/// 造出来的数据概况。
#[derive(Debug, Clone)]
pub struct PerfSeed {
    pub task_count: usize,
    pub event_count: usize,
    pub run_count: usize,
    pub thread_count: usize,
    /// 一个「待验收」的任务（带执行记录），用来测任务详情与待我处理。
    pub review_task_id: String,
    /// 一个事件特别多的任务，用来测任务详情里的活动列表。
    pub busy_task_id: String,
}

/// 数据基准时间（2026-10-01T00:00:00Z）。
const BASE_EPOCH: i64 = 1_789_516_800;

/// 固定格式的 UTC 时间戳（与 `store::now()` 同格式，可直接字符串比较）。
fn stamp(offset_secs: i64) -> String {
    let t = time::OffsetDateTime::from_unix_timestamp(BASE_EPOCH + offset_secs)
        .expect("基准时间在时间戳范围内");
    format!(
        "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}.{:03}Z",
        t.year(),
        u8::from(t.month()),
        t.day(),
        t.hour(),
        t.minute(),
        t.second(),
        t.millisecond()
    )
}

/// 状态分布（每 100 条任务）：待办 20、就绪 15、执行中 15、受阻 10、待验收 15、
/// 已完成 20、已取消 5。已取消与已完成是数量大的两列，正是分页要照顾的对象。
const DISTRIBUTION: [(TaskStatus, usize); 7] = [
    (TaskStatus::Todo, 20),
    (TaskStatus::Ready, 15),
    (TaskStatus::Doing, 15),
    (TaskStatus::Blocked, 10),
    (TaskStatus::Review, 15),
    (TaskStatus::Done, 20),
    (TaskStatus::Cancelled, 5),
];

/// 按分布取某个任务的状态。
fn status_of(index: usize) -> TaskStatus {
    let mut slot = index % 100;
    for (status, weight) in DISTRIBUTION {
        if slot < weight {
            return status;
        }
        slot -= weight;
    }
    TaskStatus::Todo
}

/// 往库里造一批任务与活动。
///
/// `role_ids` 是团队里的岗位（至少一个），任务轮流分给它们；`harness_id` 给了才写执行记录。
pub fn seed_large_team(
    conn: &Connection,
    team_id: &str,
    role_ids: &[String],
    harness_id: Option<&str>,
    task_count: usize,
) -> Result<PerfSeed> {
    if role_ids.is_empty() {
        return Err(crate::error::Error::internal("造性能数据需要至少一个岗位"));
    }
    let tx = conn.unchecked_transaction()?;

    let mut task_ids: Vec<String> = Vec::with_capacity(task_count);
    let mut review_task_id = String::new();
    {
        let mut insert_task = tx.prepare(
            "INSERT INTO tasks (id, team_id, key, title, description, status, owner_role_id,
                                blocked_reason, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
        )?;
        let mut insert_ref = tx.prepare(
            "INSERT INTO task_file_refs (task_id, position, path, anchor) VALUES (?1, ?2, ?3, NULL)",
        )?;
        let mut insert_dep = tx.prepare(
            "INSERT INTO task_dependencies (task_id, depends_on_id, position) VALUES (?1, ?2, 0)",
        )?;
        for index in 0..task_count {
            let id = format!("perf-task-{index:06}");
            let owner = &role_ids[index % role_ids.len()];
            let status = status_of(index);
            let blocked_reason = (status == TaskStatus::Blocked).then(|| "等待后端接口".to_owned());
            let created = index as i64 * 7;
            insert_task.execute(params![
                id,
                team_id,
                format!("T-{index:06}"),
                format!("性能任务 {index}"),
                "性能测试用的任务说明。",
                enum_str(status),
                owner,
                blocked_reason,
                stamp(created),
                stamp(created + 3),
            ])?;
            // 引用文件：每三条任务带一个引用。
            if index % 3 == 0 {
                insert_ref.execute(params![&id, 0, format!("docs/spec-{}.md", index % 50)])?;
            }
            // 依赖：指向更早的任务（不成环）。
            if index > 0 && index % 4 == 0 {
                insert_dep.execute(params![&id, &task_ids[index - 1]])?;
            }
            if status == TaskStatus::Review && review_task_id.is_empty() {
                review_task_id = id.clone();
            }
            task_ids.push(id);
        }
    }
    // 事件特别多的任务：前 300 条活动都挂在它下面（测任务详情的活动列表）。
    let busy_task_id = task_ids.first().cloned().unwrap_or_default();

    let mut event_count = 0usize;
    {
        let mut insert_event = tx.prepare(
            "INSERT INTO events (id, team_id, task_id, role_id, kind, actor, summary, data, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
        )?;
        for (index, id) in task_ids.iter().enumerate() {
            let owner = &role_ids[index % role_ids.len()];
            let created = index as i64 * 7;
            insert_event.execute(params![
                format!("perf-evt-created-{index:06}"),
                team_id,
                id,
                owner,
                "task_created",
                "human",
                format!("创建任务 T-{index:06}"),
                "{}",
                stamp(created),
            ])?;
            event_count += 1;
            insert_event.execute(params![
                format!("perf-evt-status-{index:06}"),
                team_id,
                id,
                owner,
                "task_status_changed",
                "human",
                format!("T-{index:06} 状态变更"),
                "{\"from\":\"todo\",\"to\":\"ready\"}",
                stamp(created + 1),
            ])?;
            event_count += 1;
        }
        // 执行相关事件（待我处理要按这些判断）。
        for index in 0..300usize {
            let id = &task_ids[index % task_ids.len()];
            let owner = &role_ids[index % role_ids.len()];
            insert_event.execute(params![
                format!("perf-evt-run-{index:06}"),
                team_id,
                id,
                owner,
                "run_started",
                "system",
                format!("开始执行 T-{index:06}"),
                "{}",
                stamp(index as i64 * 11),
            ])?;
            event_count += 1;
            insert_event.execute(params![
                format!("perf-evt-run-done-{index:06}"),
                team_id,
                id,
                owner,
                "run_finished",
                "system",
                format!("执行结束 T-{index:06}"),
                format!("{{\"runId\":\"perf-run-{index:06}\"}}"),
                stamp(index as i64 * 11 + 1),
            ])?;
            event_count += 1;
            insert_event.execute(params![
                format!("perf-evt-denied-{index:06}"),
                team_id,
                id,
                owner,
                "leader_denied",
                "leader",
                format!("Leader 越权被拒绝 T-{index:06}"),
                "{\"message\":\"权限之外的操作\"}",
                stamp(index as i64 * 11 + 2),
            ])?;
            event_count += 1;
        }
        // 挂在同一个任务上的大量活动（任务详情里的活动列表）。
        for index in 0..300usize {
            insert_event.execute(params![
                format!("perf-evt-busy-{index:06}"),
                team_id,
                &busy_task_id,
                &role_ids[0],
                "task_updated",
                "human",
                format!("任务被更新 {index} 次"),
                "{}",
                stamp(100_000 + index as i64),
            ])?;
            event_count += 1;
        }
    }

    // 执行记录：待验收任务各一条成功执行（待我处理要取它的结束时间与改动统计）。
    let mut run_count = 0usize;
    if let Some(harness_id) = harness_id {
        let mut insert_run = tx.prepare(
            "INSERT INTO runs (id, team_id, task_id, role_id, harness_id, harness_name,
                                harness_preset, harness_command, harness_args, attempt, status,
                                session_id, workspace_path, exec_dir, isolation, branch, base_commit,
                                capabilities, started_at, ended_at, stop_reason, failure, failure_kind,
                                summary, changed_files, tool_call_count, taken_over, pending_permission,
                                out_of_scope, change_note, change_incomplete, base_branch, merged_commit,
                                revision_of, review_outcome, review_comment, reviewed_at, no_changes,
                                interrupted_commit, interrupted_problem)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, '[]', 1, ?9, NULL, ?10, ?11, 'worktree',
                     'feat/task', 'a31f8c2', NULL, ?12, ?13, NULL, NULL, NULL, '执行完成',
                     '[]', 3, 0, NULL, '[]', NULL, 0, 'main', NULL, NULL, NULL, NULL, NULL, ?14,
                     NULL, NULL)",
        )?;
        for (index, id) in task_ids.iter().enumerate() {
            if status_of(index) != TaskStatus::Review {
                continue;
            }
            let started = index as i64 * 7 + 10;
            insert_run.execute(params![
                format!("perf-run-{run_count:06}"),
                team_id,
                id,
                &role_ids[index % role_ids.len()],
                harness_id,
                "性能用 Mock",
                "mock",
                "teamspace-mock-agent",
                "succeeded",
                ".",
                format!("/tmp/perf-exec/{run_count:06}"),
                stamp(started),
                stamp(started + 1),
                if run_count.is_multiple_of(7) { 1 } else { 0 },
            ])?;
            run_count += 1;
        }
    }

    // 岗位通信：40 个线程，每个线程一条消息。
    let mut thread_count = 0usize;
    {
        let mut insert_thread = tx.prepare(
            "INSERT INTO message_threads (id, team_id, task_id, subject, participants, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        )?;
        let mut insert_message = tx.prepare(
            "INSERT INTO messages (id, thread_id, team_id, task_id, sender_kind, sender_role_id,
                                   to_role_id, body, status, failure_reason, reply_body, created_at,
                                   delivered_at, replied_at, queued_at)
             VALUES (?1, ?2, ?3, ?4, 'role', ?5, ?6, ?7, ?8, NULL, ?9, ?10, ?11, ?12, ?10)",
        )?;
        for index in 0..40usize {
            let thread_id = format!("perf-thread-{index:04}");
            let task_id = &task_ids[index % task_ids.len()];
            let from = &role_ids[index % role_ids.len()];
            let to = &role_ids[(index + 1) % role_ids.len()];
            let at = stamp(index as i64 * 13);
            insert_thread.execute(params![
                thread_id,
                team_id,
                task_id,
                format!("性能线程 {index}"),
                format!("[{{\"roleId\":\"{from}\"}},{{\"roleId\":\"{to}\"}}]"),
                at,
                at,
            ])?;
            thread_count += 1;
            insert_message.execute(params![
                format!("perf-msg-{index:04}-0"),
                thread_id,
                team_id,
                task_id,
                from,
                to,
                "请看一下这个任务。",
                "replied",
                "好的。",
                at,
                at,
                at,
            ])?;
        }
    }

    // 决策记录（待我处理里「记过决策就不再出现」的判断要用）。
    {
        let mut insert_decision = tx.prepare(
            "INSERT INTO decisions (id, team_id, task_id, message_id, role_id, text, resumed_task, created_at)
             VALUES (?1, ?2, ?3, NULL, ?4, ?5, 0, ?6)",
        )?;
        for index in 0..20usize {
            insert_decision.execute(params![
                format!("perf-decision-{index:04}"),
                team_id,
                &task_ids[index % task_ids.len()],
                &role_ids[index % role_ids.len()],
                "先这样处理",
                stamp(index as i64 * 17),
            ])?;
        }
    }

    tx.commit()?;
    Ok(PerfSeed {
        task_count,
        event_count,
        run_count,
        thread_count,
        review_task_id,
        busy_task_id,
    })
}
