//! 执行引擎的端到端测试：真实子进程（cargo 构建的 `teamspace-mock-agent`）、真实 Git、临时目录。
//! 不调用任何真实 Agent。

mod common;

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use teamspace_core::model::*;
use teamspace_core::system_probe::LocalProbe;
use teamspace_core::{AgentCommand, ErrorKind, HostConfig, RunSettings, Teamspace};

fn mock_agent() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_teamspace-mock-agent"))
}

struct Env {
    dir: tempfile::TempDir,
    ws: PathBuf,
    ts: Teamspace,
    team: Team,
    role: Role,
    harness: HarnessInstance,
    events: Arc<Mutex<Vec<RunEvent>>>,
}

fn git(dir: &Path, args: &[&str]) {
    let ok = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(args)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .unwrap()
        .success();
    assert!(ok, "git {args:?}");
}

fn open(dir: &Path) -> Teamspace {
    let config = HostConfig {
        mock_agent: Some(AgentCommand {
            program: mock_agent(),
            args: vec![],
        }),
    };
    Teamspace::open_with(&dir.join("data/t.db"), Box::new(LocalProbe::new()), config).unwrap()
}

/// 工作空间（`git` 为真时是一个已有提交的 Git 仓库）、一个绑定了 Mock Harness 的岗位「前端」。
fn env(git_repo: bool) -> Env {
    let dir = tempfile::tempdir().unwrap();
    let ws = dir.path().join("ws");
    std::fs::create_dir_all(ws.join("docs")).unwrap();
    std::fs::create_dir_all(dir.path().join("data")).unwrap();
    std::fs::write(ws.join("docs/spec.md"), "# 规格\n").unwrap();
    if git_repo {
        git(&ws, &["init", "-q", "-b", "main"]);
        git(&ws, &["config", "user.email", "t@example.com"]);
        git(&ws, &["config", "user.name", "t"]);
        git(&ws, &["config", "commit.gpgsign", "false"]);
        git(&ws, &["add", "."]);
        git(&ws, &["commit", "-q", "-m", "init"]);
    }
    let ts = open(dir.path());
    let team = ts
        .create_team(CreateTeamInput {
            name: "T".into(),
            description: String::new(),
            workspace_path: ws.display().to_string(),
            roles: vec![NewRole {
                name: "前端".into(),
                code: None,
                task_prefix: "FE".into(),
                description: "负责页面".into(),
                guidelines: "先写测试".into(),
                is_leader: false,
            }],
        })
        .unwrap();
    let harness = ts
        .create_harness(CreateHarnessInput {
            team_id: team.id.clone(),
            name: "Mock".into(),
            preset: AcpPreset::Mock,
            command: None,
            args: None,
            env_profile: None,
        })
        .unwrap();
    let role = ts.list_roles(&team.id).unwrap().remove(0).role;
    ts.bind_role_harness(&role.id, &harness.id).unwrap();
    let events = Arc::new(Mutex::new(Vec::new()));
    let sink = Arc::clone(&events);
    ts.set_run_listener(move |e| {
        sink.lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .push(e.clone())
    });
    Env {
        dir,
        ws: ws.canonicalize().unwrap(),
        ts,
        team,
        role,
        harness,
        events,
    }
}

impl Env {
    fn task(&self, title: &str) -> Task {
        self.ts
            .create_task(CreateTaskInput {
                team_id: self.team.id.clone(),
                title: title.into(),
                description: "按规格实现".into(),
                owner_role_id: self.role.id.clone(),
                status: TaskStatus::Ready,
                file_refs: vec![FileRef {
                    path: "docs/spec.md".into(),
                    anchor: None,
                }],
                depends_on: vec![],
            })
            .unwrap()
            .task
    }

    /// 等执行满足条件（最多 30 秒）。
    fn wait(&self, run_id: &str, what: &str, done: impl Fn(&Run) -> bool) -> Run {
        let deadline = Instant::now() + Duration::from_secs(30);
        loop {
            let run = self.ts.get_run(run_id).unwrap();
            if done(&run) {
                return run;
            }
            assert!(
                Instant::now() < deadline,
                "timed out waiting for {what}: {run:#?}"
            );
            std::thread::sleep(Duration::from_millis(30));
        }
    }

    fn wait_end(&self, run_id: &str) -> Run {
        self.wait(run_id, "run end", |r| !r.status.is_active())
    }

    fn task_status(&self, task_id: &str) -> Task {
        self.ts.get_task(task_id).unwrap().task
    }

    fn updates(&self, run_id: &str) -> Vec<RunUpdate> {
        let page = self
            .ts
            .list_run_updates(RunUpdateQuery {
                run_id: run_id.into(),
                cursor: None,
                limit: Some(500),
            })
            .unwrap();
        let mut list = page.updates;
        list.sort_by_key(|u| u.seq);
        list
    }

    fn journal(&self) -> String {
        std::fs::read_to_string(self.ws.join("Teamspace/前端/工作日志.md")).unwrap()
    }

    /// 轮询等待工作日志包含指定内容（`sync_role_folders` 在 DB 状态更新之后异步写入）。
    fn wait_journal(&self, needle: &str) -> String {
        let deadline = Instant::now() + Duration::from_secs(30);
        let path = self.ws.join("Teamspace/前端/工作日志.md");
        loop {
            if let Ok(text) = std::fs::read_to_string(&path)
                && text.contains(needle)
            {
                return text;
            }
            assert!(
                Instant::now() < deadline,
                "timed out waiting for journal to contain: {needle}"
            );
            std::thread::sleep(Duration::from_millis(30));
        }
    }

    /// 等到满足条件的事件出现（最多等 30 秒），返回当时的事件列表副本。
    ///
    /// 事件是异步推送的：执行结束后最终的 `run_changed` 才到，此前断言事件会偶发失败。
    fn wait_event(&self, what: &str, done: impl Fn(&[RunEvent]) -> bool) -> Vec<RunEvent> {
        common::wait_event(&self.events, what, done)
    }
}

fn messages(updates: &[RunUpdate], role: MessageRole) -> Vec<String> {
    updates
        .iter()
        .filter_map(|u| match &u.body {
            RunUpdateBody::Message { role: r, text } if *r == role => Some(text.clone()),
            _ => None,
        })
        .collect()
}

#[test]
fn plain_folder_run_succeeds_in_place_and_writes_journal() {
    let e = env(false);
    let task = e.task("登录页");
    let run = e.ts.start_run(&task.id).unwrap();
    assert_eq!(run.status, RunStatus::Queued);
    assert_eq!(run.attempt, 1);
    assert_eq!(run.isolation, RunIsolation::InPlace);
    assert_eq!(run.harness.name, "Mock");
    assert_eq!(e.task_status(&task.id).status, TaskStatus::Doing);

    let run = e.wait_end(&run.id);
    assert_eq!(run.status, RunStatus::Succeeded, "{run:#?}");
    assert_eq!(run.stop_reason.as_deref(), Some("end_turn"));
    assert_eq!(Path::new(&run.exec_dir).canonicalize().unwrap(), e.ws);
    assert!(
        run.session_id
            .as_deref()
            .unwrap()
            .starts_with("mock-session-")
    );
    assert!(run.capabilities.as_ref().unwrap().load_session);
    let summary = run.summary.clone().unwrap();
    assert!(
        summary.starts_with("已完成：在 Teamspace/前端/过程文件/ 写入了"),
        "{summary}"
    );
    let written = format!(
        "Teamspace/前端/过程文件/mock-{}.md",
        run.session_id.as_deref().unwrap()
    );
    assert_eq!(
        run.changed_files,
        vec![ChangedFile {
            path: written.clone(),
            change: FileChangeKind::Added
        }]
    );
    assert!(e.ws.join(&written).is_file());
    assert_eq!(run.tool_call_count, 2);
    assert!(run.ended_at.is_some());

    let detail = e.ts.get_task(&task.id).unwrap();
    assert_eq!(detail.task.status, TaskStatus::Review);
    assert_eq!(detail.latest_run.as_ref().unwrap().id, run.id);

    // 进度：任务说明、消息块合并、计划、工具调用状态、按规则自动允许的权限、写入记录、结束状态。
    let updates = e.updates(&run.id);
    let user = messages(&updates, MessageRole::User);
    assert_eq!(user.len(), 1);
    assert!(user[0].contains("你是团队中的「前端」岗位"));
    assert!(user[0].contains("## 工作约定\n先写测试"));
    let agent = messages(&updates, MessageRole::Agent);
    assert_eq!(agent[0], "收到任务，开始处理。", "chunks merged: {agent:?}");
    assert_eq!(agent.last().unwrap(), &summary);
    let plan = updates
        .iter()
        .filter_map(|u| match &u.body {
            RunUpdateBody::Plan { entries } => Some(entries.clone()),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(plan.len(), 1, "plan is replaced in place");
    assert!(plan[0].iter().all(|p| p.status == "completed"));
    let tools: Vec<(String, Option<String>, Option<String>)> = updates
        .iter()
        .filter_map(|u| match &u.body {
            RunUpdateBody::ToolCall {
                tool_call_id,
                kind,
                status,
                ..
            } => Some((tool_call_id.clone(), kind.clone(), status.clone())),
            _ => None,
        })
        .collect();
    assert_eq!(
        tools,
        vec![
            (
                "mock-read".into(),
                Some("read".into()),
                Some("completed".into())
            ),
            (
                "mock-write".into(),
                Some("edit".into()),
                Some("completed".into())
            ),
        ]
    );
    let perm = updates
        .iter()
        .find_map(|u| match &u.body {
            RunUpdateBody::Permission { decision, .. } => decision.clone(),
            _ => None,
        })
        .unwrap();
    assert_eq!(perm.by, "rule");
    assert_eq!(perm.kind, "allow_once");
    assert!(
        updates
            .iter()
            .any(|u| matches!(&u.body, RunUpdateBody::FileWrite { allowed: true, .. }))
    );
    assert!(matches!(
        updates.last().unwrap().body,
        RunUpdateBody::Status {
            status: RunStatus::Succeeded,
            ..
        }
    ));

    // 增量读取：游标之后没有新内容。
    let page =
        e.ts.list_run_updates(RunUpdateQuery {
            run_id: run.id.clone(),
            cursor: None,
            limit: Some(3),
        })
        .unwrap();
    assert!(page.has_more);
    assert_eq!(page.updates.len(), 3);
    let mut cursor = page.next_cursor;
    let mut seen = 3;
    loop {
        let page =
            e.ts.list_run_updates(RunUpdateQuery {
                run_id: run.id.clone(),
                cursor: Some(cursor.clone()),
                limit: Some(3),
            })
            .unwrap();
        seen += page.updates.len();
        cursor = page.next_cursor;
        if !page.has_more {
            break;
        }
    }
    assert_eq!(seen, updates.len());
    assert_eq!(page_len(&e, &run.id, &cursor), 0);

    // 日志：执行摘要与改动文件（sync_role_folders 在 DB 更新后异步写入，需等待）。
    let journal = e.wait_journal("第 1 次执行完成（Mock）；摘要：已完成");
    assert!(
        journal.contains("FE-001 · 开始第 1 次执行（Harness「Mock」，原地执行）"),
        "{journal}"
    );
    assert!(
        journal.contains(&format!("改动文件：{written}")),
        "{journal}"
    );
    assert!(journal.contains("执行中 → 待验收"), "{journal}");

    // 事件：状态变化、进度、无权限请求。事件异步推送（最终的 run_changed 在岗位文件夹同步之后
    // 才到），断言前先等它出现；拿到的是事件列表副本，不在持锁期间断言。
    let events = e.wait_event("run_changed(succeeded)", |events| {
        events.iter().any(|ev| {
            matches!(ev, RunEvent::RunChanged { run: r } if r.id == run.id && r.status == RunStatus::Succeeded)
        })
    });
    assert!(
        events
            .iter()
            .any(|ev| matches!(ev, RunEvent::RunUpdated { .. }))
    );
    assert!(
        !events
            .iter()
            .any(|ev| matches!(ev, RunEvent::PermissionRequested { .. }))
    );
}

fn page_len(e: &Env, run_id: &str, cursor: &str) -> usize {
    e.ts.list_run_updates(RunUpdateQuery {
        run_id: run_id.into(),
        cursor: Some(cursor.into()),
        limit: None,
    })
    .unwrap()
    .updates
    .len()
}

#[test]
fn git_workspace_runs_in_a_worktree_branch() {
    let e = env(true);
    let task = e.task("接口");
    let run = e.ts.start_run(&task.id).unwrap();
    assert_eq!(run.isolation, RunIsolation::Worktree);
    let run = e.wait_end(&run.id);
    assert_eq!(run.status, RunStatus::Succeeded, "{run:#?}");
    assert_eq!(run.branch.as_deref(), Some("teamspace/FE-001-r1"));
    assert_eq!(run.base_commit.as_ref().unwrap().len(), 40);
    let exec = Path::new(&run.exec_dir);
    assert!(
        exec.starts_with(e.dir.path().join("data/worktrees")),
        "{exec:?}"
    );
    let written = format!(
        "Teamspace/前端/过程文件/mock-{}.md",
        run.session_id.as_deref().unwrap()
    );
    assert!(exec.join(&written).is_file());
    assert!(!e.ws.join(&written).exists(), "workspace untouched");
    assert!(
        run.changed_files.contains(&ChangedFile {
            path: written,
            change: FileChangeKind::Added
        }),
        "{:?}",
        run.changed_files
    );
    assert_eq!(e.task_status(&task.id).status, TaskStatus::Review);
    // 说明包中写明了 worktree 分支。
    let brief = &messages(&e.updates(&run.id), MessageRole::User)[0];
    assert!(brief.contains("分支 `teamspace/FE-001-r1`"), "{brief}");
}

#[test]
fn permission_needing_human_waits_and_resumes_on_decision() {
    let e = env(false);
    let task = e.task("构建 [mock:perm]");
    let run = e.ts.start_run(&task.id).unwrap();
    let waiting = e.wait(&run.id, "waiting_permission", |r| {
        r.status == RunStatus::WaitingPermission
    });
    let prompt = waiting.pending_permission.clone().unwrap();
    assert_eq!(prompt.tool_kind.as_deref(), Some("execute"));
    assert_eq!(prompt.command.as_deref(), Some("echo hello"));
    assert!(prompt.reason.contains("执行命令"), "{}", prompt.reason);
    assert_eq!(prompt.options.len(), 3);
    e.wait_event("permission_requested", |events| {
        events.iter().any(|ev| {
            matches!(
                ev,
                RunEvent::PermissionRequested { prompt: p, .. } if p.request_id == prompt.request_id
            )
        })
    });
    // 人工流转被拒绝。
    let err =
        e.ts.change_task_status(ChangeTaskStatusInput {
            task_id: task.id.clone(),
            to: TaskStatus::Blocked,
            reason: Some("x".into()),
        })
        .unwrap_err();
    assert_eq!(err.kind(), ErrorKind::Conflict);
    assert!(
        e.ts.get_task(&task.id)
            .unwrap()
            .allowed_transitions
            .is_empty()
    );

    let bad = e.ts.respond_permission(RespondPermissionInput {
        run_id: run.id.clone(),
        request_id: prompt.request_id.clone(),
        option_id: "nope".into(),
    });
    assert_eq!(bad.unwrap_err().kind(), ErrorKind::Invalid);
    let after =
        e.ts.respond_permission(RespondPermissionInput {
            run_id: run.id.clone(),
            request_id: prompt.request_id.clone(),
            option_id: "reject".into(),
        })
        .unwrap();
    assert!(after.pending_permission.is_none());
    let run = e.wait_end(&run.id);
    assert_eq!(run.status, RunStatus::Succeeded);
    assert!(run.summary.unwrap().contains("被拒绝"));
    let decided = e
        .updates(&run.id)
        .into_iter()
        .find_map(|u| match u.body {
            RunUpdateBody::Permission {
                request_id,
                decision: Some(d),
                ..
            } if request_id == prompt.request_id => Some(d),
            _ => None,
        })
        .unwrap();
    assert_eq!(
        (decided.by.as_str(), decided.kind.as_str()),
        ("human", "reject_once")
    );
    // 已处理的请求再回应：执行已结束。
    let again = e.ts.respond_permission(RespondPermissionInput {
        run_id: run.id.clone(),
        request_id: prompt.request_id,
        option_id: "allow".into(),
    });
    assert_eq!(again.unwrap_err().kind(), ErrorKind::Conflict);
}

#[test]
fn failures_block_the_task_and_can_be_retried() {
    let e = env(false);
    let task = e.task("坏任务 [mock:fail]");
    let run = e.ts.start_run(&task.id).unwrap();
    let run = e.wait_end(&run.id);
    assert_eq!(run.status, RunStatus::Failed);
    assert_eq!(run.failure_kind, Some(FailureKind::Protocol));
    assert!(
        run.failure
            .as_ref()
            .unwrap()
            .contains("Mock 模拟的内部错误")
    );
    let t = e.task_status(&task.id);
    assert_eq!(t.status, TaskStatus::Blocked);
    assert!(t.blocked_reason.unwrap().starts_with("执行失败：协议错误"));
    e.wait_journal("第 1 次执行失败（Mock）；原因：协议错误");

    // 重试：从受阻开始第 2 次执行（标题里的标记还在，仍会失败）。
    assert!(e.ts.get_task(&task.id).unwrap().run_problems.is_empty());
    let retry = e.ts.start_run(&task.id).unwrap();
    assert_eq!(retry.attempt, 2);
    assert_eq!(e.wait_end(&retry.id).status, RunStatus::Failed);
    assert_eq!(e.ts.list_runs(&task.id).unwrap().len(), 2);

    let refused = e.task("[mock:refuse]");
    let run = e.wait_end(&e.ts.start_run(&refused.id).unwrap().id);
    assert_eq!(run.failure_kind, Some(FailureKind::Refused));
    assert_eq!(run.stop_reason.as_deref(), Some("refusal"));

    let exits = e.task("[mock:exit]");
    let run = e.wait_end(&e.ts.start_run(&exits.id).unwrap().id);
    assert_eq!(run.failure_kind, Some(FailureKind::AgentExited));
    let failure = run.failure.unwrap();
    assert!(failure.contains("Agent 进程退出（退出码 1）"), "{failure}");
    assert!(failure.contains("Mock 模拟的异常退出"), "{failure}");
}

#[test]
fn auth_required_fails_and_marks_harness() {
    let e = env(false);
    let task = e.task("[mock:auth]");
    let run = e.wait_end(&e.ts.start_run(&task.id).unwrap().id);
    assert_eq!(run.status, RunStatus::Failed);
    assert_eq!(run.failure_kind, Some(FailureKind::NeedsAuth));
    assert!(run.failure.unwrap().starts_with("需要登录"));
    let h = e.ts.get_harness(&e.harness.id).unwrap();
    assert_eq!(h.handshake.status, HandshakeStatus::NeedsAuth);
    assert!(h.handshake.detail.unwrap().contains("FE-001"));
}

#[test]
fn cancel_timeout_and_single_active_run() {
    let e = env(false);
    let task = e.task("长任务 [mock:hang]");
    let other = e.task("另一个");
    let run = e.ts.start_run(&task.id).unwrap();
    e.wait(&run.id, "running", |r| r.status == RunStatus::Running);
    // 同一任务、同一个普通文件夹同时只能有一个执行。
    let err = e.ts.start_run(&task.id).unwrap_err();
    assert_eq!(err.kind(), ErrorKind::Conflict);
    assert!(err.to_string().contains("正在执行"), "{err}");
    let err = e.ts.start_run(&other.id).unwrap_err();
    assert!(
        err.to_string().contains("同一文件夹同时只能运行一个执行"),
        "{err}"
    );
    assert!(!e.ts.get_task(&other.id).unwrap().run_problems.is_empty());

    let cancelled = e.ts.cancel_run(&run.id).unwrap();
    assert_eq!(cancelled.status, RunStatus::Cancelled);
    assert!(!cancelled.taken_over);
    assert_eq!(e.task_status(&task.id).status, TaskStatus::Ready);
    assert_eq!(
        e.ts.cancel_run(&run.id).unwrap_err().kind(),
        ErrorKind::Conflict
    );

    // 超时：无输出超过设定时长。
    e.ts.set_run_settings(RunSettings {
        idle_timeout: Duration::from_millis(800),
        ..RunSettings::default()
    });
    let run = e.ts.start_run(&task.id).unwrap();
    let run = e.wait_end(&run.id);
    assert_eq!(run.status, RunStatus::Failed);
    assert_eq!(run.failure_kind, Some(FailureKind::Timeout));
    assert_eq!(run.failure.as_deref(), Some("超时：1 秒无输出"));
    assert_eq!(e.task_status(&task.id).status, TaskStatus::Blocked);
    // 文件夹空出来了。
    assert!(e.ts.get_task(&other.id).unwrap().run_problems.is_empty());
}

#[test]
fn takeover_keeps_task_doing_and_allows_new_run_or_review() {
    let e = env(false);
    let task = e.task("[mock:hang]");
    let run = e.ts.start_run(&task.id).unwrap();
    e.wait(&run.id, "running", |r| r.status == RunStatus::Running);
    let taken = e.ts.takeover_run(&run.id).unwrap();
    assert_eq!(taken.status, RunStatus::Cancelled);
    assert!(taken.taken_over);
    assert_eq!(e.task_status(&task.id).status, TaskStatus::Doing);
    assert!(e.journal().contains("人工接管"));
    e.ts.open_run_dir(&run.id).unwrap();

    let detail = e.ts.get_task(&task.id).unwrap();
    assert!(detail.run_problems.is_empty(), "{:?}", detail.run_problems);
    assert!(detail.allowed_transitions.contains(&TaskStatus::Review));
    e.ts.change_task_status(ChangeTaskStatusInput {
        task_id: task.id.clone(),
        to: TaskStatus::Review,
        reason: None,
    })
    .unwrap();
}

#[test]
fn restart_marks_runs_interrupted_and_resume_continues_the_session() {
    let e = env(false);
    let task = e.task("[mock:hang]");
    let run = e.ts.start_run(&task.id).unwrap();
    e.wait(&run.id, "running", |r| r.status == RunStatus::Running);
    let Env { dir, ts, .. } = e;
    drop(ts);

    let ts = open(dir.path());
    let run = ts.get_run(&run.id).unwrap();
    assert_eq!(run.status, RunStatus::Interrupted);
    assert!(run.can_resume);
    assert!(run.failure.unwrap().contains("应用退出"));
    assert_eq!(
        ts.get_task(&task.id).unwrap().task.status,
        TaskStatus::Doing
    );

    let resumed = ts.resume_run(&run.id).unwrap();
    assert_eq!(resumed.status, RunStatus::Queued);
    let deadline = Instant::now() + Duration::from_secs(30);
    let run = loop {
        let r = ts.get_run(&run.id).unwrap();
        if !r.status.is_active() {
            break r;
        }
        assert!(Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(30));
    };
    // 续接提示里没有 [mock:hang]，Mock 照默认流程完成。
    assert_eq!(run.status, RunStatus::Succeeded, "{run:#?}");
    let updates = ts
        .list_run_updates(RunUpdateQuery {
            run_id: run.id.clone(),
            cursor: None,
            limit: Some(500),
        })
        .unwrap()
        .updates;
    let texts: Vec<String> = updates
        .iter()
        .filter_map(|u| match &u.body {
            RunUpdateBody::Message { text, .. } => Some(text.clone()),
            _ => None,
        })
        .collect();
    assert!(!texts.iter().any(|t| t.contains("（回放）")), "{texts:?}");
    assert!(texts.iter().any(|t| t.contains("重新连接了这个会话")));
    assert_eq!(
        ts.get_task(&task.id).unwrap().task.status,
        TaskStatus::Review
    );
    assert_eq!(
        ts.resume_run(&run.id).unwrap_err().kind(),
        ErrorKind::Conflict
    );
}

#[test]
fn interrupted_runs_can_be_marked_failed_or_rerun() {
    let e = env(false);
    let a = e.task("[mock:hang] A");
    let run = e.ts.start_run(&a.id).unwrap();
    e.wait(&run.id, "running", |r| r.status == RunStatus::Running);
    let Env { dir, ts, .. } = e;
    drop(ts);
    let ts = open(dir.path());
    let failed = ts.mark_run_failed(&run.id).unwrap();
    assert_eq!(failed.status, RunStatus::Failed);
    assert_eq!(failed.failure_kind, Some(FailureKind::Interrupted));
    let task = ts.get_task(&a.id).unwrap().task;
    assert_eq!(task.status, TaskStatus::Blocked);
    assert!(task.blocked_reason.unwrap().starts_with("执行失败："));
    assert_eq!(
        ts.mark_run_failed(&run.id).unwrap_err().kind(),
        ErrorKind::Conflict
    );

    // 重新执行：中断的那次记为失败，新开第 2 次。
    let second = ts.start_run(&a.id).unwrap();
    std::thread::sleep(Duration::from_millis(300));
    drop(ts);
    let ts = open(dir.path());
    assert_eq!(
        ts.get_run(&second.id).unwrap().status,
        RunStatus::Interrupted
    );
    let third = ts.start_run(&a.id).unwrap();
    assert_eq!(third.attempt, 3);
    let second = ts.get_run(&second.id).unwrap();
    assert_eq!(second.status, RunStatus::Failed);
    assert_eq!(second.failure.as_deref(), Some("执行中断后已重新执行"));
    ts.cancel_run(&third.id).unwrap();
}
