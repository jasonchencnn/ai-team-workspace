//! M7-03 性能：1 万条任务与事件下的常用查询基准。
//!
//! 数据由 [`crate::perf_seed`] 造：一个团队 1 万条任务（分布在各状态、带依赖与文件引用）、
//! 1 万多条活动事件、若干执行与消息。
//!
//! 每个用例都断言单次查询的耗时上限。上限按构建配置给（debug 更宽），留足余量：
//! 真实机器上这些查询是毫秒级，阈值只为挡住「退化成全表扫描」这种回退。

use std::time::{Duration, Instant};

use rusqlite::params;

use super::*;
use crate::perf_seed;

/// 性能用例串行跑。
///
/// 七个用例各自造 1 万条数据，并行造会把磁盘与 CPU 打满，计时被别的用例的噪声拉高，
/// 断言就成了随机事件。串行后每个用例独占机器，耗时可复现。
static PERF_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// 取得性能用例的独占权。
fn perf_guard() -> std::sync::MutexGuard<'static, ()> {
    PERF_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// 单次查询的耗时上限。
///
/// debug 构建没有优化，SQLite 的解释执行也慢，阈值放宽到 300ms；
/// release 下同样的查询应当快一个数量级，阈值 150ms。
fn budget() -> Duration {
    if cfg!(debug_assertions) {
        Duration::from_millis(300)
    } else {
        Duration::from_millis(150)
    }
}

/// 造出来的大规模团队。
struct Seed {
    team_id: String,
    role_id: String,
    review_task_id: String,
    busy_task_id: String,
    task_count: usize,
    event_count: usize,
    thread_count: usize,
}

/// 造一个大规模团队：任务、依赖、文件引用、活动事件、执行与消息。
fn seed_large_team(f: &Fixture, task_count: usize) -> Seed {
    let team = f.team("性能组");
    let role_ids: Vec<String> = [
        "role_leader",
        "role_product",
        "role_frontend",
        "role_backend",
        "role_qa",
    ]
    .iter()
    .map(|code| f.role(&team.id, code).id)
    .collect();
    let harness = f.harness(&team.id, "性能用 Mock", AcpPreset::Mock);

    let conn = rusqlite::Connection::open(f.db_path()).unwrap();
    let seeded =
        perf_seed::seed_large_team(&conn, &team.id, &role_ids, Some(&harness.id), task_count)
            .unwrap();

    Seed {
        team_id: team.id,
        role_id: role_ids[2].clone(),
        review_task_id: seeded.review_task_id,
        busy_task_id: seeded.busy_task_id,
        task_count: seeded.task_count,
        event_count: seeded.event_count,
        thread_count: seeded.thread_count,
    }
}

/// 计时跑一次闭包，返回耗时。
fn timed<T>(what: &str, run: impl FnOnce() -> T) -> (T, Duration) {
    let started = Instant::now();
    let value = run();
    let elapsed = started.elapsed();
    eprintln!("[perf] {what}: {elapsed:?}");
    (value, elapsed)
}

/// 断言一次查询在预算内完成。
#[track_caller]
fn within_budget(what: &str, elapsed: Duration) {
    assert!(
        elapsed < budget(),
        "{what} 耗时 {elapsed:?}，超过预算 {:?}",
        budget()
    );
}

/// 优化前的看板查询：一次把团队的全部任务摘要读出来再按状态分列。
///
/// 留着是为了对比：1 万条任务时它要读 1 万行并做 4 次关联子查询，正是分页要替掉的写法。
fn board_before(f: &Fixture, team_id: &str) -> Vec<TaskSummary> {
    let conn = rusqlite::Connection::open(f.db_path()).unwrap();
    let mut stmt = conn
        .prepare(
            "SELECT t.id, t.key, t.title, t.status, t.blocked_reason, t.updated_at,
                    r.id, r.name, r.code, r.task_prefix, r.archived_at IS NOT NULL,
                    (SELECT COUNT(*) FROM task_file_refs f WHERE f.task_id = t.id),
                    (SELECT COUNT(*) FROM task_dependencies d WHERE d.task_id = t.id),
                    (SELECT COUNT(*) FROM task_dependencies d JOIN tasks u ON u.id = d.depends_on_id
                      WHERE d.task_id = t.id AND u.status <> 'done'),
                    COALESCE((SELECT x.change_incomplete FROM runs x WHERE x.task_id = t.id
                              ORDER BY x.attempt DESC LIMIT 1), 0)
             FROM tasks t JOIN roles r ON r.id = t.owner_role_id
             WHERE t.team_id = ?1 ORDER BY t.updated_at DESC, t.id DESC",
        )
        .unwrap();
    let rows = stmt
        .query_map(params![team_id], |row| {
            Ok(TaskSummary {
                id: row.get(0)?,
                key: row.get(1)?,
                title: row.get(2)?,
                status: parse_status(&row.get::<_, String>(3)?),
                blocked_reason: row.get(4)?,
                updated_at: row.get(5)?,
                owner: RoleRef {
                    id: row.get(6)?,
                    name: row.get(7)?,
                    code: row.get(8)?,
                    task_prefix: row.get(9)?,
                    archived: row.get(10)?,
                },
                file_ref_count: row.get(11)?,
                dependency_count: row.get(12)?,
                open_dependency_count: row.get(13)?,
                change_incomplete: row.get(14)?,
            })
        })
        .unwrap();
    rows.collect::<rusqlite::Result<Vec<_>>>().unwrap()
}

/// 读回状态（数据库里存的是 snake_case 字符串）。
fn parse_status(raw: &str) -> TaskStatus {
    serde_json::from_value(serde_json::Value::String(raw.to_owned())).unwrap()
}

/// 看板：1 万条任务时取一页要快；分页后总量仍然准确。
#[test]
fn board_query_with_ten_thousand_tasks_stays_fast() {
    let _guard = perf_guard();
    let f = fixture();
    let seed = seed_large_team(&f, 10_000);
    assert_eq!(seed.task_count, 10_000);
    assert!(seed.event_count > 10_000, "活动事件应当有一万条以上");

    let (before, before_elapsed) = timed("看板（优化前：整列全量）", || {
        board_before(&f, &seed.team_id)
    });
    let (board, elapsed) = timed("看板（优化后：每列 200 条）", || {
        f.ts.get_task_board(TaskBoardQuery {
            team_id: seed.team_id.clone(),
            include_cancelled: Some(true),
            limit: Some(200),
        })
        .unwrap()
    });

    assert_eq!(before.len(), 10_000);
    assert_eq!(board.columns.len(), 7);
    // 每列最多 200 条，且给出的总数是真实的列内任务数。
    for column in &board.columns {
        assert!(column.tasks.len() <= 200, "{}", column.label);
        assert!(column.total as usize >= column.tasks.len());
    }
    let done = board
        .columns
        .iter()
        .find(|column| column.status == TaskStatus::Done)
        .unwrap();
    assert!(done.total > 200, "已完成列应当是大列");
    assert!(done.has_more);
    within_budget("看板分页查询", elapsed);
    eprintln!(
        "[perf] 看板：优化前 {:?} → 优化后 {:?}（{:.1} 倍）",
        before_elapsed,
        elapsed,
        before_elapsed.as_secs_f64() / elapsed.as_secs_f64().max(0.000_001)
    );
}

/// 看板单列分页：翻页能取到全部任务，不重不漏。
#[test]
fn board_column_pagination_walks_every_task_once() {
    let _guard = perf_guard();
    let f = fixture();
    let seed = seed_large_team(&f, 10_000);
    let query = TaskBoardQuery {
        team_id: seed.team_id.clone(),
        include_cancelled: Some(true),
        limit: Some(200),
    };
    let board = f.ts.get_task_board(query).unwrap();
    let done = board
        .columns
        .iter()
        .find(|column| column.status == TaskStatus::Done)
        .unwrap()
        .clone();

    let mut seen: Vec<String> = done.tasks.iter().map(|task| task.id.clone()).collect();
    let mut cursor =
        f.ts.list_board_column(BoardColumnQuery {
            team_id: seed.team_id.clone(),
            status: TaskStatus::Done,
            limit: Some(200),
            cursor: None,
        })
        .unwrap()
        .next_cursor;
    assert!(cursor.is_some());
    let mut pages = 1;
    while let Some(next) = cursor {
        let page =
            f.ts.list_board_column(BoardColumnQuery {
                team_id: seed.team_id.clone(),
                status: TaskStatus::Done,
                limit: Some(200),
                cursor: Some(next),
            })
            .unwrap();
        seen.extend(page.tasks.iter().map(|task| task.id.clone()));
        cursor = page.next_cursor;
        pages += 1;
        assert!(pages < 100, "分页没有收敛");
    }
    assert_eq!(seen.len(), done.total as usize);
    let mut unique = seen.clone();
    unique.sort();
    unique.dedup();
    assert_eq!(unique.len(), seen.len(), "分页取到了重复任务");
}

/// 活动列表：翻页与筛选都要快。
#[test]
fn activity_list_with_filters_stays_fast() {
    let _guard = perf_guard();
    let f = fixture();
    let seed = seed_large_team(&f, 10_000);

    let (page, elapsed) = timed("活动列表（一页 100 条）", || {
        f.ts.list_events(EventQuery {
            team_id: seed.team_id.clone(),
            limit: Some(100),
            ..Default::default()
        })
        .unwrap()
    });
    assert_eq!(page.events.len(), 100);
    assert!(page.next_cursor.is_some());
    within_budget("活动列表（无筛选）", elapsed);

    let (_, elapsed) = timed("活动列表（按岗位筛选）", || {
        f.ts.list_events(EventQuery {
            team_id: seed.team_id.clone(),
            role_id: Some(seed.role_id.clone()),
            limit: Some(100),
            ..Default::default()
        })
        .unwrap()
    });
    within_budget("活动列表（按岗位筛选）", elapsed);

    let (_, elapsed) = timed("活动列表（按种类筛选）", || {
        f.ts.list_events(EventQuery {
            team_id: seed.team_id.clone(),
            kinds: Some(vec![EventKind::TaskStatusChanged]),
            limit: Some(100),
            ..Default::default()
        })
        .unwrap()
    });
    within_budget("活动列表（按种类筛选）", elapsed);

    let (_, elapsed) = timed("活动列表（按时间范围筛选）", || {
        f.ts.list_events(EventQuery {
            team_id: seed.team_id.clone(),
            since: Some("2026-10-01T00:00:00.000Z".to_owned()),
            until: Some("2026-10-02T00:00:00.000Z".to_owned()),
            limit: Some(100),
            ..Default::default()
        })
        .unwrap()
    });
    within_budget("活动列表（按时间范围筛选）", elapsed);
}

/// 任务详情：依赖、下游、文件、活动都要快。
#[test]
fn task_detail_stays_fast() {
    let _guard = perf_guard();
    let f = fixture();
    let seed = seed_large_team(&f, 10_000);

    let (detail, elapsed) = timed("任务详情", || {
        f.ts.get_task(&seed.review_task_id).unwrap()
    });
    assert_eq!(detail.task.id, seed.review_task_id);
    within_budget("任务详情", elapsed);

    // 活动特别多的任务：活动列表本身也要有上限，不能把 1 万条全塞回来。
    let (detail, elapsed) = timed("任务详情（活动很多）", || {
        f.ts.get_task(&seed.busy_task_id).unwrap()
    });
    assert!(detail.events.len() <= 200);
    within_budget("任务详情（活动很多）", elapsed);
}

/// 待我处理：聚合要快。
#[test]
fn inbox_aggregation_stays_fast() {
    let _guard = perf_guard();
    let f = fixture();
    let seed = seed_large_team(&f, 10_000);

    let (inbox, elapsed) = timed("待我处理聚合", || {
        f.ts.get_inbox(GetInboxInput {
            team_id: seed.team_id.clone(),
        })
        .unwrap()
    });
    assert!(inbox.counts.total > 0);
    within_budget("待我处理聚合", elapsed);
}

/// 线程列表：要快。
#[test]
fn thread_list_stays_fast() {
    let _guard = perf_guard();
    let f = fixture();
    let seed = seed_large_team(&f, 10_000);

    let (threads, elapsed) = timed("线程列表", || {
        f.ts.list_threads(ListThreadsQuery {
            team_id: seed.team_id.clone(),
            task_id: None,
            role_id: None,
        })
        .unwrap()
    });
    assert_eq!(threads.len(), seed.thread_count);
    within_budget("线程列表", elapsed);
}

/// 查询计划：常用查询都要走上索引，不许退化成全表扫描。
#[test]
fn query_plans_use_indexes() {
    let _guard = perf_guard();
    let f = fixture();
    let seed = seed_large_team(&f, 10_000);
    let conn = rusqlite::Connection::open(f.db_path()).unwrap();

    let team = seed.team_id.clone();
    let task = seed.busy_task_id.clone();
    let role = seed.role_id.clone();
    let cases: Vec<(&str, &str, Vec<&dyn rusqlite::ToSql>)> = vec![
        (
            "看板单列",
            "SELECT t.id FROM tasks t JOIN roles r ON r.id = t.owner_role_id
             WHERE t.team_id = ?1 AND t.status = 'done'
             ORDER BY t.updated_at DESC, t.id DESC LIMIT 201",
            vec![&team],
        ),
        (
            "活动列表（按种类筛选）",
            "SELECT seq FROM events WHERE team_id = ?1 AND kind IN ('task_status_changed')
             ORDER BY seq DESC LIMIT 101",
            vec![&team],
        ),
        (
            "活动列表（按任务筛选）",
            "SELECT seq FROM events WHERE team_id = ?1 AND task_id = ?2
             ORDER BY seq DESC LIMIT 101",
            vec![&team, &task],
        ),
        (
            "活动列表（按岗位筛选）",
            "SELECT seq FROM events WHERE team_id = ?1 AND role_id = ?2
             ORDER BY seq DESC LIMIT 101",
            vec![&team, &role],
        ),
        (
            "线程列表",
            "SELECT id FROM message_threads WHERE team_id = ?1 ORDER BY updated_at DESC, id DESC",
            vec![&team],
        ),
        (
            "任务状态统计",
            "SELECT status, COUNT(*) FROM tasks WHERE team_id = ?1 GROUP BY status",
            vec![&team],
        ),
        (
            "任务轻量列表",
            "SELECT id, key, title FROM tasks WHERE team_id = ?1 ORDER BY updated_at DESC, id DESC",
            vec![&team],
        ),
    ];
    for (name, sql, args) in cases {
        let plan = explain(&conn, sql, &args);
        eprintln!("[perf] 查询计划 {name}: {plan:?}");
        assert!(
            !plan.iter().any(|line| line.contains("SCAN events")
                || line.contains("SCAN tasks")
                || line.contains("SCAN message_threads")),
            "{name} 退化成全表扫描：{plan:?}"
        );
    }

    // 关键查询用上了专门的索引。
    assert_uses_index(
        &conn,
        "看板单列",
        "SELECT t.id FROM tasks t JOIN roles r ON r.id = t.owner_role_id
         WHERE t.team_id = ?1 AND t.status = 'done'
         ORDER BY t.updated_at DESC, t.id DESC LIMIT 201",
        "tasks_team_status_updated",
        &[&team],
    );
    assert_uses_index(
        &conn,
        "活动列表（按任务筛选）",
        "SELECT seq FROM events WHERE team_id = ?1 AND task_id = ?2
         ORDER BY seq DESC LIMIT 101",
        "events_team_task_seq",
        &[&team, &task],
    );
    assert_uses_index(
        &conn,
        "活动列表（按种类筛选）",
        "SELECT seq FROM events WHERE team_id = ?1 AND kind IN ('task_status_changed')
         ORDER BY seq DESC LIMIT 101",
        "events_team_kind_seq",
        &[&team],
    );
    assert_uses_index(
        &conn,
        "线程列表",
        "SELECT id FROM message_threads WHERE team_id = ?1 ORDER BY updated_at DESC, id DESC",
        "message_threads_team",
        &[&team],
    );
    assert_uses_index(
        &conn,
        "任务轻量列表",
        "SELECT id, key, title FROM tasks WHERE team_id = ?1 ORDER BY updated_at DESC, id DESC",
        "tasks_team_updated",
        &[&team],
    );
    assert_uses_index(
        &conn,
        "岗位最近的任务",
        "SELECT t.id FROM tasks t WHERE t.owner_role_id = ?1
         ORDER BY t.updated_at DESC, t.id DESC LIMIT 10",
        "tasks_owner_updated",
        &[&role],
    );
}

/// 查询计划（`EXPLAIN QUERY PLAN`）的每一行。
fn explain(conn: &rusqlite::Connection, sql: &str, args: &[&dyn rusqlite::ToSql]) -> Vec<String> {
    let mut stmt = conn.prepare(&format!("EXPLAIN QUERY PLAN {sql}")).unwrap();
    let rows = stmt.query_map(args, |row| row.get::<_, String>(3)).unwrap();
    rows.map(|row| row.unwrap()).collect()
}

/// 断言查询计划用上了某个索引。
fn assert_uses_index(
    conn: &rusqlite::Connection,
    name: &str,
    sql: &str,
    index: &str,
    args: &[&dyn rusqlite::ToSql],
) {
    let plan = explain(conn, sql, args);
    assert!(
        plan.iter()
            .any(|line| line.contains(&format!("USING INDEX {index}"))
                || line.contains(&format!("USING COVERING INDEX {index}"))),
        "{name} 没有用上索引 {index}：{plan:?}"
    );
}
