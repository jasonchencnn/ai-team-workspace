//! QA 第 2 轮（2026-10-14）：执行相关的服务层单元测试（假探测，不启动任何 Agent 子进程）。
//! 需要真实子进程的用例见 `tests/qa_round2_runs.rs`；内部 fencing 见 `run/fencing_tests.rs`。
//!
//! 约定同 `qa_round2.rs`：断言按用例预期写，产品缺陷保留并 `#[ignore = "缺陷 …"]`。

use std::time::{Duration, Instant};

use super::*;

fn wait_end(ts: &Teamspace, run_id: &str) -> Run {
    let deadline = Instant::now() + Duration::from_secs(20);
    loop {
        let run = ts.get_run(run_id).unwrap();
        if !run.status.is_active() {
            return run;
        }
        assert!(Instant::now() < deadline, "run did not end: {run:#?}");
        std::thread::sleep(Duration::from_millis(20));
    }
}

fn reopen(f: &Fixture) -> Teamspace {
    Teamspace::open_with(&f.db_path(), Box::new(FakeProbe), HostConfig::default()).unwrap()
}

fn events_of(ts: &Teamspace, team_id: &str) -> Vec<Event> {
    ts.list_events(EventQuery {
        team_id: team_id.into(),
        cursor: None,
        limit: Some(200),
    })
    .unwrap()
    .events
}

/// 团队 T、岗位「前端开发」绑定 Mock、一个满足就绪条件的 TODO 任务。
fn setup(f: &Fixture) -> (Team, Role, HarnessInstance, TaskDetail) {
    let team = f.team("T");
    let fe = f.role(&team.id, "role_frontend");
    let h = f.harness(&team.id, "Mock", AcpPreset::Mock);
    f.ts.bind_role_harness(&fe.id, &h.id).unwrap();
    let task = f.readyable(&team.id, &fe, "登录页");
    (team, fe, h, task)
}

const ALL_CAPS: &str = r#"{"loadSession":true,"image":false,"audio":false,"embeddedContext":false,"mcpHttp":false,"mcpSse":false}"#;
const NO_LOAD_CAPS: &str = r#"{"loadSession":false,"image":false,"audio":false,"embeddedContext":false,"mcpHttp":false,"mcpSse":false}"#;

struct RunRow<'a> {
    id: String,
    status: String,
    session: Option<&'a str>,
    caps: &'a str,
    exec_dir: Option<String>,
    isolation: &'a str,
    pending: Option<&'a str>,
}

impl<'a> RunRow<'a> {
    fn new(id: &str, status: &str) -> Self {
        Self {
            id: id.to_owned(),
            status: status.to_owned(),
            session: Some("s-1"),
            caps: ALL_CAPS,
            exec_dir: None,
            isolation: "in_place",
            pending: None,
        }
    }
}

/// 直接写入一条执行记录，并把任务置为执行中。
fn insert_run(f: &Fixture, task: &Task, harness: &HarnessInstance, row: RunRow<'_>) {
    let session = row.session.map_or("NULL".to_owned(), |s| format!("'{s}'"));
    let pending = row.pending.map_or("NULL".to_owned(), |p| format!("'{p}'"));
    let exec = row.exec_dir.unwrap_or_else(|| f.ws.display().to_string());
    f.raw_sql(&format!(
        "UPDATE tasks SET status = 'doing' WHERE id = '{task_id}';
         INSERT INTO runs (id, team_id, task_id, role_id, harness_id, harness_name, harness_preset,
                           harness_command, attempt, status, session_id, workspace_path, exec_dir,
                           isolation, capabilities, pending_permission, started_at)
         VALUES ('{id}', '{team}', '{task_id}', '{role}', '{h}', 'Mock', 'mock', 'x',
                 (SELECT COALESCE(MAX(attempt), 0) + 1 FROM runs WHERE task_id = '{task_id}'),
                 '{status}', {session}, '{ws}', '{exec}', '{isolation}', '{caps}', {pending},
                 '2026-10-13T00:00:00.000Z');",
        id = row.id,
        status = row.status,
        task_id = task.id,
        team = task.team_id,
        role = task.owner_role_id,
        h = harness.id,
        ws = f.ws.display(),
        isolation = row.isolation,
        caps = row.caps,
    ));
}

/// K-RUN-001：开始执行的任务状态条件。
#[test]
fn k_run_001_start_run_status_conditions() {
    use TaskStatus::*;
    let f = plain_fixture();
    let (team, fe, h, _) = setup(&f);
    let refused = |status_label: &str| {
        format!(
            "不能开始执行：任务状态为「{status_label}」：只有 READY、受阻，或执行中断、人工接管后的任务可以开始执行"
        )
    };
    let problem = |status_label: &str| {
        format!(
            "任务状态为「{status_label}」：只有 READY、受阻，或执行中断、人工接管后的任务可以开始执行"
        )
    };

    // 拒绝的状态：TODO / DOING（无执行）/ REVIEW / DONE / CANCELLED。
    let cases: [(&str, &[TaskStatus], &str); 5] = [
        ("TODO", &[], "TODO"),
        ("DOING 无执行", &[Ready, Doing], "执行中"),
        ("REVIEW", &[Ready, Doing, Review], "待验收"),
        ("DONE", &[Ready, Doing, Review, Done], "已完成"),
        ("CANCELLED", &[Cancelled], "已取消"),
    ];
    for (name, path, label) in cases {
        let t = f.readyable(&team.id, &fe, name);
        f.drive(&t.task.id, path);
        let detail = f.ts.get_task(&t.task.id).unwrap();
        assert_eq!(detail.run_problems, [problem(label)], "{name}");
        let msg = assert_kind(f.ts.start_run(&t.task.id), ErrorKind::Conflict);
        assert_eq!(msg, refused(label), "{name}");
        assert!(
            f.ts.list_runs(&t.task.id).unwrap().is_empty(),
            "{name}: 不产生 Run"
        );
        assert_eq!(
            f.ts.get_task(&t.task.id).unwrap().task.status,
            detail.task.status,
            "{name}"
        );
    }

    // 允许的状态：READY、BLOCKED。
    for (name, path) in [
        ("READY", &[Ready][..]),
        ("BLOCKED", &[Ready, Doing, Blocked][..]),
    ] {
        let t = f.readyable(&team.id, &fe, name);
        f.drive(&t.task.id, path);
        assert!(
            f.ts.get_task(&t.task.id).unwrap().run_problems.is_empty(),
            "{name}"
        );
        let run = f.ts.start_run(&t.task.id).unwrap();
        assert_eq!(run.attempt, 1, "{name}");
        wait_end(&f.ts, &run.id);
    }

    // DOING 且最近一次为 interrupted / 人工接管：允许。
    let interrupted = f.readyable(&team.id, &fe, "中断");
    f.drive(&interrupted.task.id, &[Ready]);
    let mut row = RunRow::new("run-int", "running");
    row.isolation = "worktree";
    insert_run(&f, &interrupted.task, &h, row);
    let taken = f.readyable(&team.id, &fe, "接管");
    f.drive(&taken.task.id, &[Ready]);
    let mut row = RunRow::new("run-take", "running");
    row.isolation = "worktree";
    insert_run(&f, &taken.task, &h, row);
    let ts = reopen(&f);
    assert_eq!(
        ts.get_run("run-int").unwrap().status,
        RunStatus::Interrupted
    );
    ts.takeover_run("run-take").unwrap();
    for (name, task) in [
        ("DOING 中断", &interrupted.task),
        ("DOING 接管", &taken.task),
    ] {
        let detail = ts.get_task(&task.id).unwrap();
        assert_eq!(detail.task.status, Doing, "{name}");
        assert!(
            detail.run_problems.is_empty(),
            "{name}: {:?}",
            detail.run_problems
        );
        let run = ts.start_run(&task.id).unwrap();
        assert_eq!(run.attempt, 2, "{name}");
        wait_end(&ts, &run.id);
    }
}

/// K-RUN-002：开始执行的 Harness、就绪与工作空间条件，拒绝时不产生 Run 也不改状态。
#[test]
fn k_run_002_start_run_harness_readiness_and_workspace_conditions() {
    // 每种情形各用一个全新的环境。
    struct Case {
        f: Fixture,
        fe: Role,
        team: Team,
        task: TaskDetail,
    }
    let case = || {
        let f = plain_fixture();
        let team = f.team("T");
        let fe = f.role(&team.id, "role_frontend");
        let task = f.readyable(&team.id, &fe, "目标");
        let task = f.move_to(&task.task.id, TaskStatus::Ready).unwrap();
        Case { f, fe, team, task }
    };
    let custom = |c: &Case, name: &str, command: &str| {
        let h =
            c.f.ts
                .create_harness(CreateHarnessInput {
                    team_id: c.team.id.clone(),
                    name: name.into(),
                    preset: AcpPreset::Custom,
                    command: Some(command.into()),
                    args: None,
                    env_profile: None,
                })
                .unwrap();
        c.f.ts.bind_role_harness(&c.fe.id, &h.id).unwrap();
        h
    };
    let unchanged = |c: &Case| {
        assert!(c.f.ts.list_runs(&c.task.task.id).unwrap().is_empty());
        assert_eq!(
            c.f.ts.get_task(&c.task.task.id).unwrap().task.status,
            c.task.task.status
        );
    };

    // 1) 未绑定。
    let c = case();
    let msg = assert_kind(c.f.ts.start_run(&c.task.task.id), ErrorKind::Conflict);
    assert!(msg.contains("未绑定 Harness"), "{msg}");
    unchanged(&c);

    // 2) 握手 error。
    let c = case();
    let h = custom(&c, "坏", BROKEN_COMMAND);
    c.f.ts.check_harness(&h.id).unwrap();
    let msg = assert_kind(c.f.ts.start_run(&c.task.task.id), ErrorKind::Conflict);
    assert!(msg.contains("握手检查失败"), "{msg}");
    assert!(
        msg.contains("无法启动 /broken/agent"),
        "握手 detail 应出现: {msg}"
    );
    unchanged(&c);

    // 3) 握手 unknown、4) needs_auth：都不阻止开始。
    for needs_auth in [false, true] {
        let c = case();
        let h = custom(
            &c,
            "实例",
            if needs_auth {
                NEEDS_AUTH_COMMAND
            } else {
                "my-agent"
            },
        );
        if needs_auth {
            let checked = c.f.ts.check_harness(&h.id).unwrap();
            assert_eq!(checked.handshake.status, HandshakeStatus::NeedsAuth);
        }
        assert!(
            c.f.ts
                .get_task(&c.task.task.id)
                .unwrap()
                .run_problems
                .is_empty()
        );
        let run = c.f.ts.start_run(&c.task.task.id).unwrap();
        assert_eq!(run.attempt, 1);
        wait_end(&c.f.ts, &run.id);
    }

    // 5) 依赖未完成：READY 之后上游被重开。
    let c = case();
    custom(&c, "实例", "my-agent");
    let up = c.f.readyable(&c.team.id, &c.fe, "上游");
    c.f.drive(
        &up.task.id,
        &[
            TaskStatus::Ready,
            TaskStatus::Doing,
            TaskStatus::Review,
            TaskStatus::Done,
        ],
    );
    let down =
        c.f.try_task(
            &c.team.id,
            &c.fe,
            "下游",
            TaskStatus::Todo,
            vec![],
            vec![up.task.id.clone()],
        )
        .unwrap();
    c.f.move_to(&down.task.id, TaskStatus::Ready).unwrap();
    c.f.move_to(&up.task.id, TaskStatus::Ready).unwrap();
    let detail = c.f.ts.get_task(&down.task.id).unwrap();
    assert!(!detail.readiness.is_empty());
    let msg = assert_kind(c.f.ts.start_run(&down.task.id), ErrorKind::Conflict);
    for reason in &detail.readiness {
        assert!(msg.contains(reason.as_str()), "{reason}\n{msg}");
    }
    assert!(c.f.ts.list_runs(&down.task.id).unwrap().is_empty());

    // 6) 引用文件缺失。
    let c = case();
    custom(&c, "实例", "my-agent");
    fs::remove_file(c.f.ws.join("docs/spec.md")).unwrap();
    let readiness = c.f.ts.get_task(&c.task.task.id).unwrap().readiness;
    assert!(
        readiness.iter().any(|r| r.contains("docs/spec.md")),
        "{readiness:?}"
    );
    let msg = assert_kind(c.f.ts.start_run(&c.task.task.id), ErrorKind::Conflict);
    for reason in &readiness {
        assert!(msg.contains(reason.as_str()), "{reason}\n{msg}");
    }
    unchanged(&c);

    // 7) 工作空间被删除。
    let c = case();
    custom(&c, "实例", "my-agent");
    fs::remove_dir_all(&c.f.ws).unwrap();
    let msg = assert_kind(c.f.ts.start_run(&c.task.task.id), ErrorKind::Conflict);
    assert!(msg.contains("工作空间不可用"), "{msg}");
    unchanged(&c);
}

/// K-RUN-010：重启后 queued / running / waiting_permission 三种残留都标记为已中断。
#[test]
fn k_run_010_restart_marks_every_active_status_interrupted() {
    let f = plain_fixture();
    let (team, fe, h, _) = setup(&f);
    let pending = r#"{"requestId":"req-1","toolCallId":"tc-1","title":"运行","toolKind":"execute","paths":[],"command":"ls","reason":"执行命令需要确认","options":[],"requestedAt":"2026-10-13T00:00:00.000Z"}"#;
    let mut ids = Vec::new();
    for status in ["queued", "running", "waiting_permission"] {
        let task = f.readyable(&team.id, &fe, &format!("任务 {status}"));
        let mut row = RunRow::new(&format!("run-{status}"), status);
        // worktree 隔离不受「同一普通文件夹只有一个活动执行」的唯一索引限制，可同时存在三个。
        row.isolation = "worktree";
        if status == "waiting_permission" {
            row.pending = Some(pending);
        }
        insert_run(&f, &task.task, &h, row);
        ids.push((status, task.task.id.clone(), task.task.key.clone()));
    }
    let before = f.ts.get_run("run-waiting_permission").unwrap();
    assert!(
        before.pending_permission.is_some(),
        "构造的待确认请求应可读出"
    );

    let ts = reopen(&f);
    ts.get_team_overview(&team.id).unwrap(); // 同步岗位工作日志
    for (status, task_id, key) in &ids {
        let run = ts.get_run(&format!("run-{status}")).unwrap();
        assert_eq!(run.status, RunStatus::Interrupted, "{status}");
        assert!(
            run.failure.as_deref().unwrap().contains("应用退出或重启"),
            "{status}"
        );
        assert!(
            run.pending_permission.is_none(),
            "{status}: pendingPermission 应清空"
        );
        assert!(run.can_resume, "{status}: 有会话且支持 loadSession");
        assert_eq!(
            ts.get_task(task_id).unwrap().task.status,
            TaskStatus::Doing,
            "{status}"
        );
        let activity = events_of(&ts, &team.id)
            .into_iter()
            .find(|e| e.kind == EventKind::RunInterrupted && e.task_id.as_deref() == Some(task_id))
            .unwrap_or_else(|| panic!("{status}: 缺少 run_interrupted 活动"));
        assert_eq!(activity.actor, Actor::System);
        assert!(activity.summary.contains(key.as_str()));
    }
    let journal = fs::read_to_string(f.ws.join("Teamspace/前端开发/工作日志.md")).unwrap();
    assert!(
        journal.contains("已中断"),
        "工作日志应记「已中断」:\n{journal}"
    );
}

/// K-RUN-011：续接的各项限制。
#[test]
fn k_run_011_resume_limits() {
    let f = plain_fixture();
    let (team, fe, h, _) = setup(&f);
    let mk = |name: &str, row: RunRow<'_>| -> Task {
        let task = f.readyable(&team.id, &fe, name);
        insert_run(&f, &task.task, &h, row);
        task.task
    };
    // 无 sessionId。
    let mut row = RunRow::new("r-nosession", "running");
    row.session = None;
    row.isolation = "worktree";
    mk("无会话", row);
    // Agent 不支持 loadSession。
    let mut row = RunRow::new("r-noload", "running");
    row.caps = NO_LOAD_CAPS;
    row.isolation = "worktree";
    mk("不支持", row);
    // 执行目录已删除。
    let mut row = RunRow::new("r-nodir", "running");
    row.exec_dir = Some(f.dir.path().join("gone").display().to_string());
    row.isolation = "worktree";
    mk("无目录", row);
    // 任务已不在 DOING。
    let mut row = RunRow::new("r-notdoing", "running");
    row.isolation = "worktree";
    let not_doing = mk("不在执行中", row);
    // 同一普通文件夹正有别的执行（r-busy 是活动的，r-victim 是被中断后想续接的）。
    mk("被占用", RunRow::new("r-victim", "running"));
    let ts = reopen(&f);
    f.raw_sql(&format!(
        "UPDATE tasks SET status = 'review' WHERE id = '{}'",
        not_doing.id
    ));
    let busy = f.readyable(&team.id, &fe, "占用者");
    f.raw_sql(&format!(
        "UPDATE tasks SET status = 'doing' WHERE id = '{id}';
         INSERT INTO runs (id, team_id, task_id, role_id, harness_id, harness_name, harness_preset,
                           harness_command, attempt, status, workspace_path, exec_dir, isolation, started_at)
         VALUES ('r-busy', '{team}', '{id}', '{role}', '{h}', 'Mock', 'mock', 'x', 1, 'running',
                 '{ws}', '{ws}', 'in_place', '2026-10-13T00:00:00.000Z');",
        id = busy.task.id,
        team = team.id,
        role = fe.id,
        h = h.id,
        ws = f.ws.display(),
    ));

    for id in [
        "r-nosession",
        "r-noload",
        "r-nodir",
        "r-notdoing",
        "r-victim",
    ] {
        assert_eq!(
            ts.get_run(id).unwrap().status,
            RunStatus::Interrupted,
            "{id}"
        );
    }
    let expect = |id: &str, needle: &str| {
        let msg = assert_kind(ts.resume_run(id), ErrorKind::Conflict);
        assert!(msg.contains(needle), "{id}: {msg}");
        assert_eq!(
            ts.get_run(id).unwrap().status,
            RunStatus::Interrupted,
            "{id}: 状态不变"
        );
    };
    expect("r-nosession", "会话建立前");
    expect("r-noload", "不支持续接会话");
    expect("r-nodir", "执行目录已不存在");
    expect("r-notdoing", "已不在执行中");
    expect("r-victim", "同一文件夹同时只能运行一个执行");
    // 已结束的执行不能续接（取消 r-busy 对应的记录：直接标成 succeeded）。
    f.raw_sql("UPDATE runs SET status = 'succeeded', ended_at = '2026-10-13T01:00:00.000Z' WHERE id = 'r-busy'");
    let msg = assert_kind(ts.resume_run("r-busy"), ErrorKind::Conflict);
    assert!(msg.contains("只有已中断的执行可以续接"), "{msg}");
    // 占用者结束后，被占用的执行可以续接（开始 queued，随后 driver 因宿主没有 Mock 而失败）。
    let resumed = ts.resume_run("r-victim").unwrap();
    assert_eq!(resumed.status, RunStatus::Queued);
    wait_end(&ts, "r-victim");
}

/// K-RUN-012：标记失败与重新执行（旧执行的活动留痕另见 `qa_round2::k_evt_004_rerun_…`，疑点 Q-12）。
#[test]
fn k_run_012_mark_failed_and_rerun_after_interruption() {
    let f = plain_fixture();
    let (team, fe, h, _) = setup(&f);
    let a = f.readyable(&team.id, &fe, "甲");
    insert_run(&f, &a.task, &h, RunRow::new("r-a", "running"));
    let ts = reopen(&f);

    let failed = ts.mark_run_failed("r-a").unwrap();
    assert_eq!(failed.status, RunStatus::Failed);
    assert_eq!(failed.failure_kind, Some(FailureKind::Interrupted));
    let task = ts.get_task(&a.task.id).unwrap().task;
    assert_eq!(task.status, TaskStatus::Blocked);
    assert_eq!(
        task.blocked_reason.as_deref(),
        Some("执行失败：执行中断后由 Human 标记为失败")
    );
    assert_kind(ts.mark_run_failed("r-a"), ErrorKind::Conflict);

    // 另一个中断的执行直接重新执行：旧的记为失败，新的 attempt + 1。
    let b = f.readyable(&team.id, &fe, "乙");
    insert_run(&f, &b.task, &h, RunRow::new("r-b", "running"));
    let ts2 = reopen(&f);
    assert_eq!(ts2.get_run("r-b").unwrap().status, RunStatus::Interrupted);
    let new_run = ts2.start_run(&b.task.id).unwrap();
    assert_eq!(new_run.attempt, 2);
    let old = ts2.get_run("r-b").unwrap();
    assert_eq!(old.status, RunStatus::Failed);
    assert_eq!(old.failure.as_deref(), Some("执行中断后已重新执行"));
    assert_eq!(old.failure_kind, Some(FailureKind::Interrupted));
    wait_end(&ts2, &new_run.id);
}

/// K-RUN-013：打开执行目录与接管的边界。
#[test]
fn k_run_013_open_run_dir_and_takeover_edges() {
    let f = plain_fixture();
    let (team, fe, h, _) = setup(&f);
    let a = f.readyable(&team.id, &fe, "甲");
    let mut row = RunRow::new("r-a", "running");
    row.exec_dir = Some(f.dir.path().join("missing-dir").display().to_string());
    row.isolation = "worktree";
    insert_run(&f, &a.task, &h, row);
    let b = f.readyable(&team.id, &fe, "乙");
    insert_run(&f, &b.task, &h, RunRow::new("r-b", "running"));
    let ts = reopen(&f);

    // 目录缺失：not_found，不通过 Probe 打开。
    let opened_before = REVEALED.with(|r| r.borrow().len());
    let msg = assert_kind(ts.open_run_dir("r-a"), ErrorKind::NotFound);
    assert!(msg.contains("执行目录不存在"), "{msg}");
    assert_eq!(REVEALED.with(|r| r.borrow().len()), opened_before);
    // 目录存在：通过 Probe 打开。
    ts.open_run_dir("r-b").unwrap();
    assert_eq!(
        REVEALED
            .with(|r| r.borrow().last().cloned())
            .unwrap()
            .canonicalize()
            .unwrap(),
        f.ws.canonicalize().unwrap()
    );

    // 接管已中断的执行：任务仍 DOING，日志含「人工接管」。
    let taken = ts.takeover_run("r-b").unwrap();
    assert_eq!(taken.status, RunStatus::Cancelled);
    assert!(taken.taken_over);
    assert_eq!(
        ts.get_task(&b.task.id).unwrap().task.status,
        TaskStatus::Doing
    );
    let journal = fs::read_to_string(f.ws.join("Teamspace/前端开发/工作日志.md")).unwrap();
    assert!(journal.contains("人工接管"), "{journal}");
    // 已结束的执行再接管：conflict「不能接管」。
    let msg = assert_kind(ts.takeover_run("r-b"), ErrorKind::Conflict);
    assert!(msg.contains("不能接管"), "{msg}");
    // 接管后可直接提交验收。
    let review = ts.change_task_status(ChangeTaskStatusInput {
        task_id: b.task.id.clone(),
        to: TaskStatus::Review,
        reason: None,
    });
    assert_eq!(review.unwrap().task.status, TaskStatus::Review);
}

/// G-7：已失败的执行也可以人工接管（任务从受阻回到执行中，界面进入「已被接管」画面）。
#[test]
fn failed_run_can_be_taken_over() {
    let f = plain_fixture();
    let (team, fe, h, _) = setup(&f);
    let t = f.readyable(&team.id, &fe, "接管失败");
    insert_run(&f, &t.task, &h, RunRow::new("r-failed", "failed"));
    f.raw_sql(&format!(
        "UPDATE runs SET failure = 'Agent 进程退出（退出码 3）', failure_kind = 'agent_exited',
                         ended_at = '2026-10-13T00:10:00.000Z'
         WHERE id = 'r-failed';
         UPDATE tasks SET status = 'blocked', blocked_reason = '执行失败：Agent 进程退出（退出码 3）'
         WHERE id = '{task}';",
        task = t.task.id
    ));
    let ts = reopen(&f);

    let taken = ts.takeover_run("r-failed").unwrap();
    assert_eq!(taken.status, RunStatus::Cancelled);
    assert!(taken.taken_over);
    // 任务从受阻回到执行中，受阻原因清空。
    let detail = ts.get_task(&t.task.id).unwrap();
    assert_eq!(detail.task.status, TaskStatus::Doing);
    assert_eq!(detail.task.blocked_reason, None);
    // 工作日志记一条接管。
    let journal = fs::read_to_string(f.ws.join("Teamspace/前端开发/工作日志.md")).unwrap();
    assert!(journal.contains("人工接管"), "{journal}");
    // 已接管的执行不能再接管。
    let msg = assert_kind(ts.takeover_run("r-failed"), ErrorKind::Conflict);
    assert!(msg.contains("不能接管"), "{msg}");
    // 接管后可以重新执行：新建第 2 次执行。
    let rerun = ts.start_run(&t.task.id).unwrap();
    assert_eq!(rerun.attempt, 2);
}

/// K-RUN-018：Run 记录 Harness 快照，之后编辑或换绑都不影响。
#[test]
fn k_run_018_run_keeps_harness_snapshot_after_edit_and_rebind() {
    let f = plain_fixture();
    let team = f.team("T");
    let fe = f.role(&team.id, "role_frontend");
    let h1 =
        f.ts.create_harness(CreateHarnessInput {
            team_id: team.id.clone(),
            name: "第一个".into(),
            preset: AcpPreset::Custom,
            command: Some("agent-one".into()),
            args: Some(vec!["--a".into(), "b c".into()]),
            env_profile: None,
        })
        .unwrap();
    let h2 = f.harness(&team.id, "第二个", AcpPreset::Mock);
    f.ts.bind_role_harness(&fe.id, &h1.id).unwrap();
    let task = f.readyable(&team.id, &fe, "快照");
    f.move_to(&task.task.id, TaskStatus::Ready).unwrap();
    let run = f.ts.start_run(&task.task.id).unwrap();
    let snapshot = run.harness.clone();
    assert_eq!(
        (
            snapshot.name.as_str(),
            snapshot.command.as_str(),
            snapshot.args.clone()
        ),
        (
            "第一个",
            "agent-one",
            vec!["--a".to_owned(), "b c".to_owned()]
        )
    );
    let ended = wait_end(&f.ts, &run.id);

    f.ts.update_harness(UpdateHarnessInput {
        harness_id: h1.id.clone(),
        name: "改名了".into(),
        command: Some("/opt/bin/other".into()),
        args: Some(vec!["--changed".into()]),
        env_profile: None,
    })
    .unwrap();
    f.ts.bind_role_harness(&fe.id, &h2.id).unwrap();

    for read in [
        f.ts.get_run(&run.id).unwrap(),
        f.ts.list_runs(&task.task.id).unwrap().remove(0),
    ] {
        assert_eq!(read.harness, snapshot);
        assert_eq!(read.harness.harness_id, h1.id);
        assert_eq!(read.status, ended.status);
    }
}

/// K-HAR-010：环境配置档案缺失或格式错误导致执行以 launch 失败结束，且不泄露档案内容。
#[test]
fn k_har_010_env_profile_problems_fail_the_run_with_launch_kind() {
    let f = plain_fixture();
    let team = f.team("T");
    let fe = f.role(&team.id, "role_frontend");
    let missing = f.dir.path().join("missing.env");
    let bad = f.dir.path().join("bad.env");
    fs::write(&bad, "NOT A PAIR\n").unwrap();
    let cases = [
        ("缺失", missing, "无法读取环境配置档案"),
        ("格式错误", bad, "第 1 行格式不正确"),
    ];
    for (i, (name, path, needle)) in cases.into_iter().enumerate() {
        let h =
            f.ts.create_harness(CreateHarnessInput {
                team_id: team.id.clone(),
                name: name.into(),
                preset: AcpPreset::Custom,
                // 启动命令必须是存在的文件，才会走到读取环境配置档案这一步。
                command: Some("/bin/sh".into()),
                args: None,
                env_profile: Some(path.display().to_string()),
            })
            .unwrap();
        f.ts.bind_role_harness(&fe.id, &h.id).unwrap();
        let task = f.readyable(&team.id, &fe, &format!("任务{i}"));
        f.move_to(&task.task.id, TaskStatus::Ready).unwrap();
        let run = f.ts.start_run(&task.task.id).unwrap();
        let run = wait_end(&f.ts, &run.id);
        assert_eq!(run.status, RunStatus::Failed, "{name}");
        assert_eq!(run.failure_kind, Some(FailureKind::Launch), "{name}");
        let failure = run.failure.unwrap();
        assert!(failure.contains(needle), "{name}: {failure}");
        assert!(
            !failure.contains("NOT A PAIR"),
            "不得回显档案内容: {failure}"
        );
        let task = f.ts.get_task(&task.task.id).unwrap().task;
        assert_eq!(task.status, TaskStatus::Blocked, "{name}");
        let reason = task.blocked_reason.unwrap();
        assert!(reason.starts_with("执行失败："), "{name}: {reason}");
        assert!(!reason.contains("NOT A PAIR"), "{reason}");
    }
}

/// K-RUN-005（数据库层）：违反唯一索引的第二个活动 Run 被映射为 conflict。
#[test]
fn k_run_005_second_active_run_for_the_same_task_violates_the_unique_index() {
    let f = plain_fixture();
    let (_, _, h, task) = setup(&f);
    let mut row = RunRow::new("r-1", "running");
    row.isolation = "worktree";
    insert_run(&f, &task.task, &h, row);
    let conn = rusqlite::Connection::open(f.db_path()).unwrap();
    let err = conn
        .execute(
            "INSERT INTO runs (id, team_id, task_id, role_id, harness_id, harness_name, harness_preset,
                               harness_command, attempt, status, workspace_path, exec_dir, isolation, started_at)
             SELECT 'r-2', team_id, task_id, role_id, harness_id, 'Mock', 'mock', 'x', 2, 'queued',
                    workspace_path, exec_dir, 'worktree', started_at FROM runs WHERE id = 'r-1'",
            [],
        )
        .unwrap_err();
    assert!(err.to_string().contains("UNIQUE"), "{err}");
    assert!(
        err.to_string().contains("runs.task_id"),
        "应是任务级唯一索引 runs_active_task: {err}"
    );
    // 错误映射：约束冲突 → conflict。
    let mapped = Error::from(err);
    assert_eq!(mapped.kind(), ErrorKind::Conflict);
    // 终态之后可以再有活动执行（唯一索引只约束活动状态）。
    f.raw_sql("UPDATE runs SET status = 'failed' WHERE id = 'r-1'");
    conn.execute(
        "INSERT INTO runs (id, team_id, task_id, role_id, harness_id, harness_name, harness_preset,
                           harness_command, attempt, status, workspace_path, exec_dir, isolation, started_at)
         SELECT 'r-2', team_id, task_id, role_id, harness_id, 'Mock', 'mock', 'x', 2, 'queued',
                workspace_path, exec_dir, 'worktree', started_at FROM runs WHERE id = 'r-1'",
        [],
    )
    .unwrap();
}
