//! 执行引擎集成测试共用的环境：真实子进程（cargo 构建的 `teamspace-mock-agent`）、真实 Git、临时目录。
#![allow(dead_code)]

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use teamspace_core::model::*;
use teamspace_core::system_probe::LocalProbe;
use teamspace_core::{AgentCommand, HostConfig, SystemProbe, Teamspace};

pub fn mock_agent() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_teamspace-mock-agent"))
}

pub fn git(dir: &Path, args: &[&str]) {
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

pub struct Env {
    pub dir: tempfile::TempDir,
    pub ws: PathBuf,
    pub ts: Teamspace,
    pub team: Team,
    pub role: Role,
    pub harness: HarnessInstance,
    pub events: Arc<Mutex<Vec<RunEvent>>>,
}

/// 工作空间（`git_repo` 为真时是已有提交的 Git 仓库）、一个绑定了 Mock Harness 的岗位「前端」。
pub fn env(git_repo: bool) -> Env {
    env_with(git_repo, Box::new(LocalProbe::new()))
}

/// 同 [`env`]，但可换掉系统探测（用来制造「命令找不到」之类的状态）。
pub fn env_with(git_repo: bool, probe: Box<dyn SystemProbe>) -> Env {
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
    let config = HostConfig {
        mock_agent: Some(AgentCommand {
            program: mock_agent(),
            args: vec![],
        }),
    };
    let ts = Teamspace::open_with(&dir.path().join("data/t.db"), probe, config).unwrap();
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
        ws: ws.canonicalize().unwrap(),
        dir,
        ts,
        team,
        role,
        harness,
        events,
    }
}

/// 轮询等待满足条件的事件（事件由执行线程推送，最多等 30 秒），返回命中时的事件列表副本。
///
/// 事件是异步推送的：执行状态先写进数据库，岗位文件夹同步和事件推送才陆续发生，断言前必须等。
/// 返回副本而不是在锁内断言：不在持锁期间 panic，避免毒化互斥锁。
pub fn wait_event(
    events: &Arc<Mutex<Vec<RunEvent>>>,
    what: &str,
    done: impl Fn(&[RunEvent]) -> bool,
) -> Vec<RunEvent> {
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        let snapshot = events
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clone();
        if done(&snapshot) {
            return snapshot;
        }
        assert!(
            Instant::now() < deadline,
            "timed out waiting for event: {what}"
        );
        std::thread::sleep(Duration::from_millis(20));
    }
}

impl Env {
    pub fn task(&self, title: &str) -> Task {
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

    pub fn wait(&self, run_id: &str, what: &str, done: impl Fn(&Run) -> bool) -> Run {
        let deadline = Instant::now() + Duration::from_secs(60);
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

    /// 等岗位工作日志出现某段内容（日志在执行结束后异步同步，最多等 30 秒），返回当时的全文。
    pub fn wait_journal(&self, what: &str) -> String {
        let deadline = Instant::now() + Duration::from_secs(30);
        loop {
            let text = std::fs::read_to_string(self.role_file("工作日志.md")).unwrap_or_default();
            if text.contains(what) {
                return text;
            }
            assert!(
                Instant::now() < deadline,
                "timed out waiting for journal to contain {what:?}: {text}"
            );
            std::thread::sleep(Duration::from_millis(30));
        }
    }

    /// 等到满足条件的事件出现（最多等 30 秒），返回当时的事件列表副本。
    ///
    /// 事件是异步推送的：执行结束后最终的 `run_changed` 才到，此前断言事件会偶发失败。
    pub fn wait_event(&self, what: &str, done: impl Fn(&[RunEvent]) -> bool) -> Vec<RunEvent> {
        wait_event(&self.events, what, done)
    }

    pub fn wait_end(&self, run_id: &str) -> Run {
        self.wait(run_id, "run end", |r| !r.status.is_active())
    }

    pub fn updates(&self, run_id: &str) -> Vec<RunUpdate> {
        let mut list = self
            .ts
            .list_run_updates(RunUpdateQuery {
                run_id: run_id.into(),
                cursor: None,
                limit: Some(500),
            })
            .unwrap()
            .updates;
        list.sort_by_key(|u| u.seq);
        list
    }

    pub fn respond(&self, run: &Run, prompt: &PermissionPrompt, option_id: &str) {
        self.ts
            .respond_permission(RespondPermissionInput {
                run_id: run.id.clone(),
                request_id: prompt.request_id.clone(),
                option_id: option_id.into(),
            })
            .unwrap();
    }

    pub fn role_file(&self, name: &str) -> PathBuf {
        self.ws.join("Teamspace/前端").join(name)
    }
}

/// 进度里的写入记录：(路径, 是否放行, 原因)。
pub fn file_writes(updates: &[RunUpdate]) -> Vec<(String, bool, String)> {
    updates
        .iter()
        .filter_map(|u| match &u.body {
            RunUpdateBody::FileWrite {
                path,
                allowed,
                reason,
            } => Some((path.clone(), *allowed, reason.clone())),
            _ => None,
        })
        .collect()
}

pub fn hash(path: &Path) -> Option<Vec<u8>> {
    std::fs::read(path).ok()
}

/// 最后一条 Agent 消息（Mock 把各场景的结果写进总结）。
pub fn summary(run: &Run) -> String {
    run.summary.clone().unwrap_or_default()
}
