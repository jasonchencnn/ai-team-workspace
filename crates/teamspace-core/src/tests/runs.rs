//! 执行（Run）的规则：开始条件、启动失败的状态联动、重启恢复与中断后的处理、进度分页。
//! 这里不启动任何 Agent（Mock 预设指向不存在的宿主程序）；真实子进程的全流程见 `tests/runs.rs`。

use std::time::{Duration, Instant};

use super::*;

fn wait_end(f: &Fixture, run_id: &str) -> Run {
    let deadline = Instant::now() + Duration::from_secs(20);
    loop {
        let run = f.ts.get_run(run_id).unwrap();
        if !run.status.is_active() {
            return run;
        }
        assert!(Instant::now() < deadline, "run did not end: {run:#?}");
        std::thread::sleep(Duration::from_millis(20));
    }
}

/// 团队 T、岗位「前端」（绑定 Mock）与一个 READY 任务。
fn setup(f: &Fixture) -> (Team, Role, HarnessInstance, TaskDetail) {
    let team = f.team("T");
    let fe = f.role(&team.id, "role_frontend");
    let h = f.harness(&team.id, "Mock", AcpPreset::Mock);
    f.ts.bind_role_harness(&fe.id, &h.id).unwrap();
    let task = f.readyable(&team.id, &fe, "登录页");
    let task = f.move_to(&task.task.id, TaskStatus::Ready).unwrap();
    (team, fe, h, task)
}

#[test]
fn run_problems_explain_why_start_is_refused() {
    let f = plain_fixture();
    let team = f.team("T");
    let fe = f.role(&team.id, "role_frontend");
    let todo = f.task(&team.id, &fe, "无引用");
    let problems = todo.run_problems.join("\n");
    assert!(problems.contains("任务状态为「TODO」"), "{problems}");
    assert!(!problems.contains("文件引用"), "{problems}");
    assert!(problems.contains("未绑定 Harness"), "{problems}");
    let err = assert_kind(f.ts.start_run(&todo.task.id), ErrorKind::Conflict);
    assert!(err.starts_with("不能开始执行："), "{err}");

    let h =
        f.ts.create_harness(CreateHarnessInput {
            team_id: team.id.clone(),
            name: "坏的".into(),
            preset: AcpPreset::Custom,
            command: Some(BROKEN_COMMAND.into()),
            args: None,
            env_profile: None,
        })
        .unwrap();
    f.ts.bind_role_harness(&fe.id, &h.id).unwrap();
    // 未检查过（unknown）不阻止开始。
    let ready = f.readyable(&team.id, &fe, "可开始");
    let ready = f.move_to(&ready.task.id, TaskStatus::Ready).unwrap();
    assert!(ready.run_problems.is_empty(), "{:?}", ready.run_problems);
    f.ts.check_harness(&h.id).unwrap();
    let detail = f.ts.get_task(&ready.task.id).unwrap();
    assert_eq!(detail.run_problems.len(), 1);
    assert!(detail.run_problems[0].contains("握手检查失败"));
    assert!(detail.latest_run.is_none());
}

#[test]
fn launch_failure_blocks_the_task_and_is_recorded() {
    let f = plain_fixture();
    let (team, fe, _, task) = setup(&f);
    let run = f.ts.start_run(&task.task.id).unwrap();
    assert_eq!(run.status, RunStatus::Queued);
    assert_eq!(run.isolation, RunIsolation::InPlace);
    assert_eq!(run.task_key, task.task.key);
    assert_eq!(run.harness.preset, AcpPreset::Mock);
    let run = wait_end(&f, &run.id);
    assert_eq!(run.status, RunStatus::Failed);
    assert_eq!(run.failure_kind, Some(FailureKind::Launch));
    assert!(
        run.failure.as_deref().unwrap().contains(MOCK_HOST),
        "{run:?}"
    );
    assert!(!run.can_resume);

    let detail = f.ts.get_task(&task.task.id).unwrap();
    assert_eq!(detail.task.status, TaskStatus::Blocked);
    assert!(
        detail
            .task
            .blocked_reason
            .as_deref()
            .unwrap()
            .starts_with("执行失败：启动命令不存在")
    );
    assert_eq!(detail.latest_run.as_ref().unwrap().id, run.id);
    let kinds: Vec<EventKind> = detail.events.iter().map(|e| e.kind).collect();
    assert!(kinds.contains(&EventKind::RunStarted));
    assert!(kinds.contains(&EventKind::RunFinished));
    let system = detail
        .events
        .iter()
        .find(|e| e.kind == EventKind::RunFinished)
        .unwrap();
    assert_eq!(system.actor, Actor::System);

    let page =
        f.ts.list_run_updates(RunUpdateQuery {
            run_id: run.id.clone(),
            cursor: None,
            limit: None,
        })
        .unwrap();
    let statuses: Vec<RunStatus> = page
        .updates
        .iter()
        .filter_map(|u| match &u.body {
            RunUpdateBody::Status { status, .. } => Some(*status),
            _ => None,
        })
        .collect();
    assert_eq!(statuses, vec![RunStatus::Queued, RunStatus::Failed]);
    assert_eq!(page.status, RunStatus::Failed);
    assert!(!page.has_more);

    let wb = f.ts.get_role_workbench(&fe.id).unwrap();
    assert_eq!(wb.recent_runs.len(), 1);
    assert_eq!(f.ts.list_runs(&task.task.id).unwrap().len(), 1);
    let journal = fs::read_to_string(f.ws.join("Teamspace/前端开发/工作日志.md")).unwrap();
    assert!(
        journal.contains("第 1 次执行失败（Mock）；原因："),
        "{journal}"
    );

    // 受阻后可以重试（第 2 次）。
    let retry = f.ts.start_run(&task.task.id).unwrap();
    assert_eq!(retry.attempt, 2);
    wait_end(&f, &retry.id);
    let _ = team;
}

#[test]
fn git_workspace_without_real_repo_fails_to_create_worktree() {
    let f = fixture();
    let (_, _, _, task) = setup(&f);
    let run = f.ts.start_run(&task.task.id).unwrap();
    assert_eq!(run.isolation, RunIsolation::Worktree);
    assert!(run.exec_dir.contains("worktrees"), "{}", run.exec_dir);
    let run = wait_end(&f, &run.id);
    assert_eq!(run.failure_kind, Some(FailureKind::Launch));
    assert!(run.failure.unwrap().contains("无法创建执行目录"));
}

fn insert_active_run(
    f: &Fixture,
    task: &Task,
    harness: &HarnessInstance,
    session: Option<&str>,
) -> String {
    let id = format!("run-{}", task.key);
    let session = session.map_or("NULL".to_owned(), |s| format!("'{s}'"));
    f.raw_sql(&format!(
        "UPDATE tasks SET status = 'doing' WHERE id = '{task_id}';
         INSERT INTO runs (id, team_id, task_id, role_id, harness_id, harness_name, harness_preset,
                           harness_command, attempt, status, session_id, workspace_path, exec_dir,
                           isolation, capabilities, started_at)
         VALUES ('{id}', '{team}', '{task_id}', '{role}', '{h}', 'Mock', 'mock', 'x', 1, 'running',
                 {session}, '{ws}', '{ws}', 'in_place', '{{\"loadSession\":true,\"image\":false,\"audio\":false,\"embeddedContext\":false,\"mcpHttp\":false,\"mcpSse\":false}}',
                 '2026-10-13T00:00:00.000Z');",
        task_id = task.id,
        team = task.team_id,
        role = task.owner_role_id,
        h = harness.id,
        ws = f.ws.display(),
    ));
    id
}

#[test]
fn reopening_marks_active_runs_interrupted() {
    let f = plain_fixture();
    let (_, fe, h, task) = setup(&f);
    let other = f.readyable(&task.task.team_id, &fe, "另一个");
    let a = insert_active_run(&f, &task.task, &h, Some("s-1"));
    // 唯一约束：同一个普通文件夹只能有一个活动执行。
    let conn = rusqlite::Connection::open(f.db_path()).unwrap();
    let err = conn
        .execute(
            "INSERT INTO runs (id, team_id, task_id, role_id, harness_id, harness_name, harness_preset,
                               harness_command, attempt, status, workspace_path, exec_dir, isolation, started_at)
             SELECT 'dup', team_id, ?1, role_id, harness_id, 'Mock', 'mock', 'x', 1, 'queued',
                    workspace_path, exec_dir, 'in_place', started_at FROM runs WHERE id = ?2",
            rusqlite::params![other.task.id, a],
        )
        .unwrap_err();
    assert!(err.to_string().contains("UNIQUE"), "{err}");
    drop(conn);

    let ts =
        Teamspace::open_with(&f.db_path(), Box::new(FakeProbe), HostConfig::default()).unwrap();
    let run = ts.get_run(&a).unwrap();
    assert_eq!(run.status, RunStatus::Interrupted);
    assert!(run.can_resume);
    assert!(run.failure.unwrap().contains("应用退出或重启"));
    assert_eq!(
        ts.get_task(&task.task.id).unwrap().task.status,
        TaskStatus::Doing
    );
    let detail = ts.get_task(&task.task.id).unwrap();
    assert!(
        detail.run_problems.is_empty(),
        "rerun allowed: {:?}",
        detail.run_problems
    );
    assert!(
        detail
            .events
            .iter()
            .any(|e| e.kind == EventKind::RunInterrupted)
    );

    // 已中断的执行不能取消，可以接管：任务保持执行中。
    assert_kind(ts.cancel_run(&a), ErrorKind::Conflict);
    let taken = ts.takeover_run(&a).unwrap();
    assert_eq!(taken.status, RunStatus::Cancelled);
    assert!(taken.taken_over);
    assert_eq!(
        ts.get_task(&task.task.id).unwrap().task.status,
        TaskStatus::Doing
    );
    assert_kind(ts.takeover_run(&a), ErrorKind::Conflict);
    assert_kind(ts.resume_run(&a), ErrorKind::Conflict);
}

#[test]
fn resume_requires_session_and_load_support() {
    let f = plain_fixture();
    let (_, _, h, task) = setup(&f);
    let id = insert_active_run(&f, &task.task, &h, None);
    let ts =
        Teamspace::open_with(&f.db_path(), Box::new(FakeProbe), HostConfig::default()).unwrap();
    let msg = assert_kind(ts.resume_run(&id), ErrorKind::Conflict);
    assert!(msg.contains("会话建立前"), "{msg}");
    let failed = ts.mark_run_failed(&id).unwrap();
    assert_eq!(failed.status, RunStatus::Failed);
    let t = ts.get_task(&task.task.id).unwrap().task;
    assert_eq!(t.status, TaskStatus::Blocked);
    assert_eq!(
        t.blocked_reason.as_deref(),
        Some("执行失败：执行中断后由 Human 标记为失败")
    );
}

#[test]
fn run_lookups_validate_input() {
    let f = plain_fixture();
    let (_, _, _, task) = setup(&f);
    assert_kind(f.ts.get_run("missing"), ErrorKind::NotFound);
    assert_kind(f.ts.list_runs("missing"), ErrorKind::NotFound);
    assert!(f.ts.list_runs(&task.task.id).unwrap().is_empty());
    let run = f.ts.start_run(&task.task.id).unwrap();
    wait_end(&f, &run.id);
    assert_kind(
        f.ts.list_run_updates(RunUpdateQuery {
            run_id: run.id.clone(),
            cursor: Some("abc".into()),
            limit: None,
        }),
        ErrorKind::Invalid,
    );
    assert_kind(
        f.ts.respond_permission(RespondPermissionInput {
            run_id: run.id.clone(),
            request_id: "r".into(),
            option_id: "o".into(),
        }),
        ErrorKind::Conflict,
    );
    f.ts.open_run_dir(&run.id).unwrap();
    let revealed = REVEALED.with(|r| r.borrow().last().cloned()).unwrap();
    assert_eq!(
        revealed.canonicalize().unwrap(),
        f.ws.canonicalize().unwrap()
    );
    assert_kind(f.ts.mark_run_failed(&run.id), ErrorKind::Conflict);
}

#[test]
fn editing_is_refused_only_while_a_run_is_active() {
    let f = plain_fixture();
    let (_, _, h, task) = setup(&f);
    let a = insert_active_run(&f, &task.task, &h, Some("s-1"));
    let edit = |ts: &Teamspace| {
        ts.update_task(UpdateTaskInput {
            task_id: task.task.id.clone(),
            title: "改名".into(),
            description: task.task.description.clone(),
            owner_role_id: task.task.owner_role_id.clone(),
            file_refs: task.task.file_refs.clone(),
            depends_on: task.task.depends_on.clone(),
        })
    };
    // 活动执行（running）：整个编辑被拒绝，标题不变。
    let ts = &f.ts;
    let msg = assert_kind(edit(ts), ErrorKind::Conflict);
    assert_eq!(msg, "任务正在执行中，结束或取消执行后才能编辑");
    assert_eq!(ts.get_task(&task.task.id).unwrap().task.title, "登录页");

    // 重启后执行被标记为中断，任务仍为 DOING：允许编辑。
    let ts =
        Teamspace::open_with(&f.db_path(), Box::new(FakeProbe), HostConfig::default()).unwrap();
    assert_eq!(ts.get_run(&a).unwrap().status, RunStatus::Interrupted);
    assert_eq!(edit(&ts).unwrap().task.title, "改名");

    // 人工接管后同样允许编辑。
    ts.takeover_run(&a).unwrap();
    assert_eq!(edit(&ts).unwrap().task.title, "改名");
}

#[test]
fn task_without_file_refs_can_be_ready_and_start_a_run() {
    let f = plain_fixture();
    let team = f.team("T");
    let fe = f.role(&team.id, "role_frontend");
    let h = f.harness(&team.id, "Mock", AcpPreset::Mock);
    f.ts.bind_role_harness(&fe.id, &h.id).unwrap();
    let task = f.task(&team.id, &fe, "无引用");
    assert!(task.task.file_refs.is_empty());
    let task = f.move_to(&task.task.id, TaskStatus::Ready).unwrap();
    assert!(task.run_problems.is_empty(), "{:?}", task.run_problems);
    let run = f.ts.start_run(&task.task.id).unwrap();
    wait_end(&f, &run.id);
}
