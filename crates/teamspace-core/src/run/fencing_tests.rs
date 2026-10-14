//! QA 第 2 轮：K-RUN-014 / K-RUN-015 的 fencing 测试（PLAN §3「写入前检查自己仍是当前 Run」，疑点 Q-03）。
//!
//! 这些场景在公开接口上无法稳定触发（需要 driver 的写入晚于中断），因此放在 `run` 模块内部，
//! 直接构造 [`Driver`] 或调用存储层函数。断言按用例预期写。

use std::sync::atomic::AtomicU8;
use std::sync::mpsc;
use std::sync::{Arc, Condvar, Mutex};

use super::*;
use crate::model::{AcpPreset, TaskStatus};
use crate::tests::{Fixture, plain_fixture};

struct Seeded {
    f: Fixture,
    core: Arc<Core>,
    team_id: String,
    /// （Run ID，任务 ID）
    runs: Vec<(String, String)>,
}

/// 为每个状态建立一个任务（DOING）和一条该状态的 Run（worktree 隔离，不受普通文件夹唯一索引限制）。
fn seed(statuses: &[&str]) -> Seeded {
    let f = plain_fixture();
    let team = f.team("T");
    let fe = f.role(&team.id, "role_frontend");
    let h = f.harness(&team.id, "Mock", AcpPreset::Mock);
    f.ts.bind_role_harness(&fe.id, &h.id).unwrap();
    let mut runs = Vec::new();
    for (i, status) in statuses.iter().enumerate() {
        let task = f.readyable(&team.id, &fe, &format!("任务{i}"));
        let id = format!("run-{i}-{status}");
        let ended = if matches!(*status, "queued" | "running" | "waiting_permission") {
            "NULL".to_owned()
        } else {
            "'2026-10-13T01:00:00.000Z'".to_owned()
        };
        let (failure, summary) = match *status {
            "failed" => ("'原始失败原因'", "'原始摘要'"),
            "succeeded" => ("NULL", "'原始摘要'"),
            "cancelled" => ("'已取消'", "NULL"),
            "interrupted" => ("'应用退出或重启时执行仍在进行'", "NULL"),
            _ => ("NULL", "NULL"),
        };
        f.raw_sql(&format!(
            "UPDATE tasks SET status = 'doing' WHERE id = '{task_id}';
             INSERT INTO runs (id, team_id, task_id, role_id, harness_id, harness_name, harness_preset,
                               harness_command, attempt, status, session_id, workspace_path, exec_dir,
                               isolation, capabilities, started_at, ended_at, failure, summary)
             VALUES ('{id}', '{team}', '{task_id}', '{role}', '{h}', 'Mock', 'mock', 'x', 1,
                     '{status}', 's-1', '{ws}', '{ws}', 'worktree',
                     '{{\"loadSession\":true,\"image\":false,\"audio\":false,\"embeddedContext\":false,\"mcpHttp\":false,\"mcpSse\":false}}',
                     '2026-10-13T00:00:00.000Z', {ended}, {failure}, {summary});",
            task_id = task.task.id,
            team = team.id,
            role = fe.id,
            h = h.id,
            ws = f.ws.display(),
        ));
        runs.push((id, task.task.id.clone()));
    }
    let core = f.ts.core_handle();
    Seeded {
        team_id: team.id,
        f,
        core,
        runs,
    }
}

fn get_run(core: &Core, id: &str) -> Run {
    store_run::get(&core.lock(), id).unwrap()
}

fn task_status(core: &Core, id: &str) -> TaskStatus {
    task::get(&core.lock(), id).unwrap().status
}

fn count_kind(f: &Fixture, team_id: &str, kind: EventKind) -> usize {
    f.ts.list_events(crate::model::EventQuery {
        team_id: team_id.to_owned(),
        cursor: None,
        limit: Some(200),
    })
    .unwrap()
    .events
    .into_iter()
    .filter(|e| e.kind == kind)
    .count()
}

fn driver_for(core: &Arc<Core>, run_id: &str) -> Driver {
    let run = get_run(core, run_id);
    let (tx, rx) = mpsc::channel();
    let active = Arc::new(ActiveRun {
        tx: Mutex::new(tx.clone()),
        stop: AtomicU8::new(Stop::None as u8),
        process: Mutex::new(None),
        done: Mutex::new(false),
        done_cv: Condvar::new(),
    });
    Driver::new(Arc::clone(core), active, run, tx, rx)
}

/// 对四个终态 Run 逐个执行一次迟到的写入，断言 Run 与任务都不变。
/// `skip_interrupted`：`reopen` 对 interrupted 本来就生效，不在「不得生效」之列。
fn assert_late_write_is_ignored(
    label: &str,
    skip_interrupted: bool,
    write: impl Fn(&Core, &str) -> Result<bool>,
) {
    let s = seed(&["cancelled", "succeeded", "failed", "interrupted"]);
    for (id, task_id) in &s.runs {
        let before = get_run(&s.core, id);
        if skip_interrupted && before.status == RunStatus::Interrupted {
            continue;
        }
        let task_before = task_status(&s.core, task_id);
        assert!(!before.status.is_active(), "{id}");
        assert!(!write(&s.core, id).unwrap(), "{id}: {label} 应报告未生效");
        assert_eq!(
            get_run(&s.core, id),
            before,
            "{id}: {label} 对终态 Run 不应生效"
        );
        assert_eq!(
            task_status(&s.core, task_id),
            task_before,
            "{id}: 任务状态不变"
        );
    }
}

/// K-RUN-014（1）：迟到的 `end(failed)` 不得覆盖终态 Run。
#[test]
fn k_run_014_late_end_does_not_overwrite_a_terminal_run() {
    assert_late_write_is_ignored("end()", false, |core, id| {
        core.write(|tx| {
            store_run::end(
                tx,
                id,
                RunStatus::Failed,
                &Ending {
                    failure: Some("迟到的失败".into()),
                    summary: Some("迟到的摘要".into()),
                    ..Ending::default()
                },
            )
        })
    });
}

/// K-RUN-014（2）：迟到的 `set_active_status(running)` 不得把终态 Run 改回活动。
#[test]
fn k_run_014_late_set_active_status_does_not_revive_a_terminal_run() {
    assert_late_write_is_ignored("set_active_status()", false, |core, id| {
        core.write(|tx| store_run::set_active_status(tx, id, RunStatus::Running, None, 99))
    });
}

/// K-RUN-014（3）：`reopen` 仅对 interrupted 生效，其他终态不得被重新打开。
#[test]
fn k_run_014_reopen_only_applies_to_interrupted_runs() {
    assert_late_write_is_ignored("reopen()", true, |core, id| {
        core.write(|tx| store_run::reopen(tx, id))
    });
}

/// K-RUN-014 的对照：这些写入对活动 Run 正常生效（保证上面的「不生效」不是因为接口坏了）。
#[test]
fn k_run_014_control_active_runs_accept_the_same_writes() {
    let s = seed(&["running"]);
    let (id, _) = &s.runs[0];
    s.core
        .write(|tx| store_run::set_active_status(tx, id, RunStatus::WaitingPermission, None, 3))
        .unwrap();
    let run = get_run(&s.core, id);
    assert_eq!(run.status, RunStatus::WaitingPermission);
    assert_eq!(run.tool_call_count, 3);
    s.core
        .write(|tx| {
            store_run::end(
                tx,
                id,
                RunStatus::Failed,
                &Ending {
                    failure: Some("真正的失败".into()),
                    failure_kind: Some(FailureKind::Protocol),
                    ..Ending::default()
                },
            )
        })
        .unwrap();
    let run = get_run(&s.core, id);
    assert_eq!(run.status, RunStatus::Failed);
    assert_eq!(run.failure.as_deref(), Some("真正的失败"));
    assert!(run.ended_at.is_some());
    // 中断的 Run 可以续接（reopen 对 interrupted 生效）。
    let s = seed(&["interrupted"]);
    let (id, _) = &s.runs[0];
    assert!(s.core.write(|tx| store_run::reopen(tx, id)).unwrap());
    assert_eq!(get_run(&s.core, id).status, RunStatus::Queued);
}

/// K-RUN-015（1）：中断之后到达的 `finish(Succeeded)` 不得改写 Run 或任务。
#[test]
fn k_run_015_finish_after_interruption_does_not_rewrite_run_or_task() {
    let s = seed(&["running"]);
    let (id, task_id) = &s.runs[0];
    let mut driver = driver_for(&s.core, id);

    // 与 shutdown_runs 相同的写入：把活动执行标记为已中断。
    s.core
        .write(|tx| interrupt(tx, id, "应用退出时执行仍在进行，已中断"))
        .unwrap();
    assert_eq!(get_run(&s.core, id).status, RunStatus::Interrupted);
    assert_eq!(count_kind(&s.f, &s.team_id, EventKind::RunInterrupted), 1);

    // driver 已越过 finish() 开头的 Shutdown 检查，之后才执行写入。
    driver.finish(End::Succeeded("end_turn".into()));

    let run = get_run(&s.core, id);
    assert_eq!(
        run.status,
        RunStatus::Interrupted,
        "Run 应保持 interrupted: {run:#?}"
    );
    assert_eq!(run.stop_reason, None);
    assert_eq!(
        task_status(&s.core, task_id),
        TaskStatus::Doing,
        "任务应保持 DOING"
    );
    assert_eq!(
        count_kind(&s.f, &s.team_id, EventKind::RunFinished),
        0,
        "不应出现 succeeded 与 run_interrupted 并存的矛盾历史"
    );
}

/// K-RUN-015（2）：中断之后到达的 Human 权限决定（driver 侧的状态写入）不得把 Run 改回 running。
#[test]
fn k_run_015_late_permission_decision_does_not_revive_an_interrupted_run() {
    let s = seed(&["waiting_permission"]);
    let (id, task_id) = &s.runs[0];
    let mut driver = driver_for(&s.core, id);
    s.core
        .write(|tx| interrupt(tx, id, "应用退出时执行仍在进行，已中断"))
        .unwrap();
    assert_eq!(get_run(&s.core, id).status, RunStatus::Interrupted);

    // on_human_decision 在回应 Agent 之后做的状态写入。
    driver.set_status(RunStatus::Running);

    assert_eq!(
        get_run(&s.core, id).status,
        RunStatus::Interrupted,
        "Run 不应被改回 running"
    );
    assert_eq!(task_status(&s.core, task_id), TaskStatus::Doing);
}

/// K-RUN-015（3）：执行已结束后到达的权限决定经公开路径返回 conflict，Run 状态不变。
#[test]
fn k_run_015_permission_decision_after_interruption_is_a_conflict() {
    let s = seed(&["waiting_permission"]);
    let (id, _) = &s.runs[0];
    s.core
        .write(|tx| interrupt(tx, id, "应用退出时执行仍在进行，已中断"))
        .unwrap();
    let err =
        s.f.ts
            .respond_permission(crate::model::RespondPermissionInput {
                run_id: id.clone(),
                request_id: "any".into(),
                option_id: "allow".into(),
            })
            .unwrap_err();
    assert_eq!(err.kind(), crate::ErrorKind::Conflict);
    assert!(err.to_string().contains("执行已结束"), "{err}");
    assert_eq!(get_run(&s.core, id).status, RunStatus::Interrupted);
}

/// 06a #4：`set_exec`、`set_capabilities`、`set_session` 对已结束（含已中断）的执行不生效。
#[test]
fn late_exec_capability_and_session_writes_are_fenced() {
    assert_late_write_is_ignored("set_session()", false, |core, id| {
        core.write(|tx| store_run::set_session(tx, id, "旧会话"))
    });
    assert_late_write_is_ignored("set_exec()", false, |core, id| {
        core.write(|tx| store_run::set_exec(tx, id, "/elsewhere", Some("b"), Some("c")))
    });
    assert_late_write_is_ignored("set_capabilities()", false, |core, id| {
        core.write(|tx| store_run::set_capabilities(tx, id, &AcpCapabilities::default()))
    });
    // 活动执行照常生效。
    let s = seed(&["running"]);
    let (id, _) = &s.runs[0];
    assert!(
        s.core
            .write(|tx| store_run::set_session(tx, id, "新会话"))
            .unwrap()
    );
    assert_eq!(get_run(&s.core, id).session_id.as_deref(), Some("新会话"));
}

/// 06a #9：被忽略的迟到写入在执行记录里留一条状态说明。
#[test]
fn ignored_late_writes_leave_a_status_note() {
    let s = seed(&["interrupted"]);
    let (id, _) = &s.runs[0];
    assert!(
        !s.core
            .write(|tx| store_run::set_active_status(tx, id, RunStatus::Running, None, 1))
            .unwrap()
    );
    let page =
        s.f.ts
            .list_run_updates(crate::model::RunUpdateQuery {
                run_id: id.clone(),
                cursor: None,
                limit: None,
            })
            .unwrap();
    let note = page.updates.iter().find_map(|u| match &u.body {
        RunUpdateBody::Status { status, detail } => detail
            .as_deref()
            .filter(|d| d.contains("迟到的「活动状态切换」已忽略"))
            .map(|d| (*status, d.to_owned())),
        _ => None,
    });
    let (status, detail) = note.unwrap_or_else(|| panic!("{page:#?}"));
    assert_eq!(status, RunStatus::Interrupted, "{detail}");
    assert!(detail.contains("已中断"), "{detail}");
}

/// 06a #3：接管只在任务仍停在这次执行留下的状态时生效（失败 → 受阻，中断 → 执行中）。
#[test]
fn takeover_requires_the_task_to_still_be_where_the_run_left_it() {
    let s = seed(&["failed", "interrupted"]);
    let (failed, failed_task) = &s.runs[0];
    let (interrupted, interrupted_task) = &s.runs[1];

    // 失败的执行，但任务已被改为 REVIEW：不拉回执行中，执行也不改。
    s.f.raw_sql(&format!(
        "UPDATE tasks SET status = 'review' WHERE id = '{failed_task}';"
    ));
    let before = get_run(&s.core, failed);
    let err = s.f.ts.takeover_run(failed).unwrap_err();
    assert_eq!(err.kind(), crate::ErrorKind::Conflict);
    assert!(err.to_string().contains("任务已是"), "{err}");
    assert_eq!(get_run(&s.core, failed), before);
    assert_eq!(task_status(&s.core, failed_task), TaskStatus::Review);

    // 任务仍受阻：接管生效，任务回到执行中。
    s.f.raw_sql(&format!(
        "UPDATE tasks SET status = 'blocked', blocked_reason = '执行失败：x' WHERE id = '{failed_task}';"
    ));
    assert!(s.f.ts.takeover_run(failed).unwrap().taken_over);
    assert_eq!(task_status(&s.core, failed_task), TaskStatus::Doing);

    // 中断的执行：任务仍在执行中时可以接管；已受阻时不行。
    s.f.raw_sql(&format!(
        "UPDATE tasks SET status = 'blocked', blocked_reason = 'x' WHERE id = '{interrupted_task}';"
    ));
    assert_eq!(
        s.f.ts.takeover_run(interrupted).unwrap_err().kind(),
        crate::ErrorKind::Conflict
    );
    s.f.raw_sql(&format!(
        "UPDATE tasks SET status = 'doing', blocked_reason = NULL WHERE id = '{interrupted_task}';"
    ));
    assert!(s.f.ts.takeover_run(interrupted).unwrap().taken_over);
    assert_eq!(task_status(&s.core, interrupted_task), TaskStatus::Doing);
}
