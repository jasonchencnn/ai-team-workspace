//! QA 第 2 轮（2026-10-14）：执行引擎的真实子进程测试。
//!
//! 使用 cargo 构建的 `teamspace-mock-agent`，以及一个按参数切换行为的 Python 假 Agent（见
//! [`FAKE_AGENT`]，用来覆盖 Mock 不支持的场景：记录收到的方法、不响应取消、连续两个权限请求、
//! 输出 `max_tokens`、批量写文件等）。全部使用临时目录、真实 Git，不调用任何真实 Harness。
//!
//! 断言按 `cases/用例总表.md` 的预期写；确认是产品缺陷的测试保留并 `#[ignore = "缺陷 …"]`。

#![cfg(unix)]

use std::os::unix::fs::{PermissionsExt, symlink};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use teamspace_core::model::*;
use teamspace_core::run::policy::Scope;
use teamspace_core::system_probe::LocalProbe;
use teamspace_core::{AgentCommand, ErrorKind, HostConfig, RunSettings, Teamspace};

/// 假 Agent：`key=value` 形式的参数控制行为（`mode=`、`log=`、`pidfile=`、`path=`、`dir=`、`count=` …）。
const FAKE_AGENT: &str = r#"#!/usr/bin/env python3
import sys, json, os

ARGS = {}
for a in sys.argv[1:]:
    if "=" in a:
        k, v = a.split("=", 1)
        ARGS[k] = v
MODE = ARGS.get("mode", "work")
LOG = ARGS.get("log")

def log(line):
    if LOG:
        with open(LOG, "a", encoding="utf-8") as f:
            f.write(line + "\n")

if ARGS.get("pidfile"):
    with open(ARGS["pidfile"], "w") as f:
        f.write(str(os.getpid()))
if ARGS.get("envvar") and ARGS.get("envout"):
    with open(ARGS["envout"], "w") as f:
        f.write(os.environ.get(ARGS["envvar"], "<unset>"))
if MODE == "stderrleak":
    sys.stderr.write("fatal: invalid api key " + ARGS.get("leak", "") + "\n")
    sys.stderr.flush()
    sys.exit(1)

def send(obj):
    sys.stdout.write(json.dumps(obj, ensure_ascii=False) + "\n")
    sys.stdout.flush()

def read():
    while True:
        line = sys.stdin.readline()
        if line == "":
            sys.exit(0)
        line = line.strip()
        if line:
            return json.loads(line)

NEXT = [7000]

def note(session, update):
    send({"jsonrpc": "2.0", "method": "session/update", "params": {"sessionId": session, "update": update}})

def say(session, text):
    note(session, {"sessionUpdate": "agent_message_chunk", "content": {"type": "text", "text": text}})

def request_nowait(method, params):
    rid = NEXT[0]
    NEXT[0] += 1
    send({"jsonrpc": "2.0", "id": rid, "method": method, "params": params})
    return rid

def wait_for(ids):
    results = {}
    while len(results) < len(ids):
        m = read()
        if m.get("method") == "session/cancel":
            log("cancel")
            continue
        if "method" in m:
            continue
        if m.get("id") in ids:
            results[m["id"]] = m
    return results

def call(method, params):
    rid = request_nowait(method, params)
    return wait_for([rid])[rid]

def finish(mid, reason):
    send({"jsonrpc": "2.0", "id": mid, "result": {"stopReason": reason}})

ALLOW = [
    {"optionId": "allow", "name": "允许一次", "kind": "allow_once"},
    {"optionId": "reject", "name": "拒绝", "kind": "reject_once"},
]
REJECT_ONLY = [{"optionId": "reject", "name": "拒绝", "kind": "reject_once"}]

def error_text(reply):
    return reply["error"].get("message", "") if "error" in reply else "none"

def prompt(mid, params):
    session = params.get("sessionId", "fake-session-1")
    if MODE == "limit_tokens":
        say(session, "输出到上限了")
        return finish(mid, "max_tokens")
    if MODE == "limit_turns":
        say(session, "轮次到上限了")
        return finish(mid, "max_turn_requests")
    if MODE == "hang_log_cancel":
        say(session, "正在长时间处理……")
        while True:
            m = read()
            if m.get("method") == "session/cancel":
                log("cancel")
                return finish(mid, "cancelled")
    if MODE == "hang_ignore_cancel":
        say(session, "正在长时间处理……")
        while True:
            m = read()
            if m.get("method") == "session/cancel":
                log("cancel")
    if MODE == "perm2":
        say(session, "需要两个确认")
        base = {"sessionId": session, "options": ALLOW}
        r1 = request_nowait("session/request_permission", dict(base, toolCall={"toolCallId": "t1", "title": "命令一", "kind": "execute", "status": "pending", "rawInput": {"command": "echo one"}}))
        r2 = request_nowait("session/request_permission", dict(base, toolCall={"toolCallId": "t2", "title": "命令二", "kind": "execute", "status": "pending", "rawInput": {"command": "echo two"}}))
        res = wait_for([r1, r2])
        log("perm1=" + json.dumps(res[r1].get("result")))
        log("perm2=" + json.dumps(res[r2].get("result")))
        say(session, "两个请求都已处理")
        return finish(mid, "end_turn")
    if MODE in ("readperm", "readallow"):
        path = ARGS["path"]
        opts = REJECT_ONLY if MODE == "readperm" else ALLOW
        r = call("session/request_permission", {
            "sessionId": session,
            "toolCall": {"toolCallId": "r1", "title": "读取", "kind": "read", "status": "pending", "locations": [{"path": path}]},
            "options": opts,
        })
        log("perm_outcome=" + json.dumps(r.get("result")))
        w = call("fs/write_text_file", {"sessionId": session, "path": path, "content": "written-by-agent"})
        log("write_error=" + error_text(w))
        say(session, "结束")
        return finish(mid, "end_turn")
    if MODE == "write":
        w = call("fs/write_text_file", {"sessionId": session, "path": ARGS["path"], "content": "written-by-agent"})
        log("write_error=" + error_text(w))
        say(session, "写入完成")
        return finish(mid, "end_turn")
    # work：可选批量写文件与超长摘要
    count = int(ARGS.get("count", "0"))
    for i in range(count):
        call("fs/write_text_file", {"sessionId": session, "path": os.path.join(ARGS["dir"], "f%02d.md" % (i + 1)), "content": "x"})
    n = int(ARGS.get("summary_len", "0"))
    if n:
        text = "摘" * n + "\n<deliver-assets>\n<asset name=\"a\" />\n</deliver-assets>"
    else:
        text = "全部完成。"
    say(session, text)
    finish(mid, "end_turn")

while True:
    m = read()
    method = m.get("method")
    mid = m.get("id")
    if method == "session/cancel":
        log("cancel")
        continue
    if method is None or mid is None:
        continue
    log(method)
    if method == "initialize":
        send({"jsonrpc": "2.0", "id": mid, "result": {
            "protocolVersion": 1,
            "agentCapabilities": {"loadSession": True, "promptCapabilities": {"image": False, "audio": False, "embeddedContext": False}, "mcpCapabilities": {"http": False, "sse": False}},
            "authMethods": [],
            "agentInfo": {"name": "fake-agent", "title": "Fake", "version": "1"},
        }})
    elif method == "session/new":
        send({"jsonrpc": "2.0", "id": mid, "result": {"sessionId": "fake-session-1"}})
    elif method == "session/load":
        send({"jsonrpc": "2.0", "id": mid, "result": {}})
    elif method == "session/prompt":
        prompt(mid, m.get("params", {}))
    else:
        send({"jsonrpc": "2.0", "id": mid, "error": {"code": -32601, "message": "Method not found"}})
"#;

fn mock_agent() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_teamspace-mock-agent"))
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

fn git_init(dir: &Path) {
    git(dir, &["init", "-q", "-b", "main"]);
    git(dir, &["config", "user.email", "t@example.com"]);
    git(dir, &["config", "user.name", "t"]);
    git(dir, &["config", "commit.gpgsign", "false"]);
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

fn pid_alive(pid: &str) -> bool {
    Command::new("kill")
        .args(["-0", pid])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

/// 工作空间形态。
#[derive(Clone, Copy)]
enum Kind {
    /// 普通文件夹。
    Plain,
    /// 有一次提交的 Git 仓库。
    Repo,
    /// `git init` 之后没有任何提交。
    EmptyRepo,
}

struct Env {
    dir: tempfile::TempDir,
    /// 工作空间（规范化）。
    ws: PathBuf,
    script: PathBuf,
    ts: Teamspace,
    team: Team,
    role: Role,
    events: Arc<Mutex<Vec<RunEvent>>>,
}

fn env(kind: Kind) -> Env {
    env_at("ws", None, kind)
}

/// `ws_rel`：工作空间相对临时目录的路径；`repo_rel`：Git 仓库根（缺省即工作空间自身）。
fn env_at(ws_rel: &str, repo_rel: Option<&str>, kind: Kind) -> Env {
    let dir = tempfile::tempdir().unwrap();
    let ws = dir.path().join(ws_rel);
    std::fs::create_dir_all(ws.join("docs")).unwrap();
    std::fs::create_dir_all(dir.path().join("data")).unwrap();
    std::fs::write(ws.join("docs/spec.md"), "# 规格\n").unwrap();
    let root = repo_rel.map_or_else(|| ws.clone(), |r| dir.path().join(r));
    match kind {
        Kind::Plain => {}
        Kind::Repo | Kind::EmptyRepo => {
            git_init(&root);
            if matches!(kind, Kind::Repo) {
                git(&root, &["add", "."]);
                git(&root, &["commit", "-q", "-m", "init"]);
            }
        }
    }
    let script = dir.path().join("fake-agent.py");
    std::fs::write(&script, FAKE_AGENT).unwrap();
    std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
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
    let mock = ts
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
    ts.bind_role_harness(&role.id, &mock.id).unwrap();
    let events = Arc::new(Mutex::new(Vec::new()));
    let sink = Arc::clone(&events);
    ts.set_run_listener(move |e| sink.lock().unwrap().push(e.clone()));
    Env {
        ws: ws.canonicalize().unwrap(),
        dir,
        script,
        ts,
        team,
        role,
        events,
    }
}

impl Env {
    fn task(&self, title: &str) -> Task {
        self.task_with(title, &["docs/spec.md"])
    }

    fn task_with(&self, title: &str, refs: &[&str]) -> Task {
        self.ts
            .create_task(CreateTaskInput {
                team_id: self.team.id.clone(),
                title: title.into(),
                description: "按规格实现".into(),
                owner_role_id: self.role.id.clone(),
                status: TaskStatus::Ready,
                file_refs: refs
                    .iter()
                    .map(|p| FileRef {
                        path: (*p).into(),
                        anchor: None,
                    })
                    .collect(),
                depends_on: vec![],
            })
            .unwrap()
            .task
    }

    /// 创建一个启动假 Agent 的 Custom Harness 并绑定到岗位。
    fn fake(&self, name: &str, args: &[String]) -> HarnessInstance {
        let h = self
            .ts
            .create_harness(CreateHarnessInput {
                team_id: self.team.id.clone(),
                name: name.into(),
                preset: AcpPreset::Custom,
                command: Some(self.script.display().to_string()),
                args: Some(args.to_vec()),
                env_profile: None,
            })
            .unwrap();
        self.ts.bind_role_harness(&self.role.id, &h.id).unwrap();
        h
    }

    fn path(&self, name: &str) -> PathBuf {
        self.dir.path().join(name)
    }

    fn wait(&self, run_id: &str, what: &str, done: impl Fn(&Run) -> bool) -> Run {
        let deadline = Instant::now() + Duration::from_secs(40);
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

    fn task_of(&self, id: &str) -> Task {
        self.ts.get_task(id).unwrap().task
    }

    fn updates(&self, run_id: &str) -> Vec<RunUpdate> {
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

    fn journal(&self) -> String {
        std::fs::read_to_string(self.ws.join("Teamspace/前端/工作日志.md")).unwrap()
    }

    fn db_dump(&self) -> Vec<(String, Vec<u8>)> {
        let mut out = Vec::new();
        for entry in std::fs::read_dir(self.dir.path().join("data"))
            .unwrap()
            .flatten()
        {
            if entry.file_name().to_string_lossy().starts_with("t.db") {
                out.push((
                    entry.file_name().to_string_lossy().into_owned(),
                    std::fs::read(entry.path()).unwrap(),
                ));
            }
        }
        out
    }
}

fn contains_bytes(haystack: &[u8], needle: &str) -> bool {
    haystack
        .windows(needle.len())
        .any(|window| window == needle.as_bytes())
}

fn read_tree(dir: &Path, out: &mut Vec<(PathBuf, String)>) {
    for entry in std::fs::read_dir(dir).unwrap().flatten() {
        let path = entry.path();
        if path.is_dir() {
            read_tree(&path, out);
        } else if let Ok(text) = std::fs::read_to_string(&path) {
            out.push((path, text));
        }
    }
}

fn user_brief(e: &Env, run_id: &str) -> String {
    e.updates(run_id)
        .into_iter()
        .find_map(|u| match u.body {
            RunUpdateBody::Message {
                role: MessageRole::User,
                text,
            } => Some(text),
            _ => None,
        })
        .expect("brief")
}

fn log_lines(path: &Path) -> Vec<String> {
    std::fs::read_to_string(path)
        .unwrap_or_default()
        .lines()
        .map(str::to_owned)
        .collect()
}

/// `- YYYY-MM-DD HH:MM:SS UTC±HH:MM · <KEY> · 正文`，返回 KEY 之后的正文。
fn journal_body(line: &str, key: &str) -> String {
    let rest = line
        .strip_prefix("- ")
        .unwrap_or_else(|| panic!("行首应为「- 」: {line:?}"));
    let (time, tail) = rest
        .split_once(" · ")
        .unwrap_or_else(|| panic!("缺少分隔: {line:?}"));
    let b = time.as_bytes();
    let ok = time.len() == 29
        && b[4] == b'-'
        && b[7] == b'-'
        && b[10] == b' '
        && b[13] == b':'
        && b[16] == b':'
        && &time[19..23] == " UTC"
        && matches!(b[23], b'+' | b'-')
        && b[26] == b':';
    assert!(ok, "时间格式应为 YYYY-MM-DD HH:MM:SS UTC±HH:MM: {time:?}");
    let (k, body) = tail
        .split_once(" · ")
        .unwrap_or_else(|| panic!("缺少任务编号段: {line:?}"));
    assert_eq!(k, key, "{line:?}");
    body.to_owned()
}

fn journal_lines_of(e: &Env, key: &str) -> Vec<String> {
    e.journal()
        .lines()
        .filter(|l| l.starts_with("- ") && l.contains(&format!(" · {key} · ")))
        .map(|l| journal_body(l, key))
        .collect()
}

// ---------------------------------------------------------------- K-HAR

/// K-HAR-004：握手检查只发 `initialize`，不创建会话、不发送任务。
#[test]
fn k_har_004_check_sends_only_initialize() {
    let e = env(Kind::Plain);
    let log = e.path("agent.log");
    let h = e.fake(
        "fake",
        &["mode=work".into(), format!("log={}", log.display())],
    );
    let checked = e.ts.check_harness(&h.id).unwrap();
    assert_eq!(
        checked.handshake.status,
        HandshakeStatus::Connected,
        "{:?}",
        checked.handshake.detail
    );
    assert_eq!(checked.handshake.protocol_version, Some(1));
    assert_eq!(checked.handshake.agent.as_ref().unwrap().name, "fake-agent");
    assert!(
        checked
            .handshake
            .capabilities
            .as_ref()
            .unwrap()
            .load_session
    );
    assert!(checked.handshake.checked_at.is_some());
    assert_eq!(checked.updated_at, h.updated_at, "updatedAt 不变");
    let lines = log_lines(&log);
    assert_eq!(
        lines,
        ["initialize"],
        "Agent 只应收到 initialize: {lines:?}"
    );
    let top =
        e.ts.list_events(EventQuery {
            team_id: e.team.id.clone(),
            cursor: None,
            limit: Some(1),
        })
        .unwrap()
        .events
        .remove(0);
    assert_eq!(top.kind, EventKind::HarnessChecked);
    assert!(
        top.summary.contains("已连接（fake-agent 1）"),
        "{}",
        top.summary
    );
}

/// K-HAR-008（D6）：环境配置档案只记路径；档案内容不进入数据库、活动、执行进度、岗位文件夹，
/// 只在启动 Agent 子进程时注入环境。
#[test]
fn k_har_008_env_profile_secret_is_injected_but_never_stored() {
    let e = env(Kind::Plain);
    const SECRET: &str = "sk-test-123";
    let profile = e.path("agent.env");
    std::fs::write(&profile, format!("SECRET_TOKEN={SECRET}\n")).unwrap();
    let envout = e.path("seen-env.txt");
    let h =
        e.ts.create_harness(CreateHarnessInput {
            team_id: e.team.id.clone(),
            name: "带档案".into(),
            preset: AcpPreset::Custom,
            command: Some(e.script.display().to_string()),
            args: Some(vec![
                "mode=work".into(),
                "envvar=SECRET_TOKEN".into(),
                format!("envout={}", envout.display()),
            ]),
            env_profile: Some(profile.display().to_string()),
        })
        .unwrap();
    assert_eq!(h.env_profile.as_deref(), Some(profile.to_str().unwrap()));
    e.ts.bind_role_harness(&e.role.id, &h.id).unwrap();
    let checked = e.ts.check_harness(&h.id).unwrap();
    assert_eq!(checked.handshake.status, HandshakeStatus::Connected);
    assert!(
        !envout.exists() || std::fs::read_to_string(&envout).unwrap() == "<unset>",
        "握手不读取档案内容"
    );

    let task = e.task("读取环境变量");
    let run = e.ts.start_run(&task.id).unwrap();
    let run = e.wait_end(&run.id);
    assert_eq!(run.status, RunStatus::Succeeded, "{run:#?}");
    // 档案内容只在启动子进程时注入环境。
    assert_eq!(std::fs::read_to_string(&envout).unwrap(), SECRET);

    // 数据库文件（含 WAL）、活动、执行进度、执行记录、岗位文件夹中都没有出现。
    for (name, bytes) in e.db_dump() {
        assert!(!contains_bytes(&bytes, SECRET), "{name} 含密钥");
    }
    let events =
        e.ts.list_events(EventQuery {
            team_id: e.team.id.clone(),
            cursor: None,
            limit: Some(200),
        })
        .unwrap()
        .events;
    for ev in &events {
        let text = format!("{} {}", ev.summary, ev.data);
        assert!(!text.contains(SECRET), "{text}");
    }
    let updates = serde_json::to_string(&e.updates(&run.id)).unwrap();
    assert!(!updates.contains(SECRET));
    assert!(
        !serde_json::to_string(&e.ts.get_run(&run.id).unwrap())
            .unwrap()
            .contains(SECRET)
    );
    let mut files = Vec::new();
    read_tree(&e.ws.join("Teamspace"), &mut files);
    assert!(files.len() >= 3);
    for (path, text) in files {
        assert!(!text.contains(SECRET), "{}", path.display());
    }
    // 实例里只存路径。
    assert_eq!(
        e.ts.get_harness(&h.id).unwrap().env_profile.as_deref(),
        Some(profile.to_str().unwrap())
    );
}

/// K-HAR-009：启动参数里的疑似密钥不应写入工作空间文件、活动、Run 快照、失败原因与日志。
#[test]
fn k_har_009_secret_in_args_does_not_leak_into_stored_text() {
    let e = env(Kind::Plain);
    const SECRET: &str = "sk-test-123";
    let h =
        e.ts.create_harness(CreateHarnessInput {
            team_id: e.team.id.clone(),
            name: "带密钥参数".into(),
            preset: AcpPreset::Custom,
            command: Some(e.script.display().to_string()),
            args: Some(vec![
                "--api-key".into(),
                SECRET.into(),
                "mode=stderrleak".into(),
                format!("leak={SECRET}"),
            ]),
            env_profile: None,
        })
        .unwrap();
    e.ts.bind_role_harness(&e.role.id, &h.id).unwrap();
    let updated =
        e.ts.update_harness(UpdateHarnessInput {
            harness_id: h.id.clone(),
            name: "带密钥参数（改）".into(),
            command: Some(e.script.display().to_string()),
            args: Some(h.args.clone()),
            env_profile: None,
        })
        .unwrap();
    let mut leaks: Vec<String> = Vec::new();

    // a) 岗位说明.md
    let brief = std::fs::read_to_string(e.ws.join("Teamspace/前端/岗位说明.md")).unwrap();
    if brief.contains(SECRET) {
        leaks.push("岗位说明.md「当前绑定 Harness」行".into());
    }
    // b) 活动 data.args
    let events =
        e.ts.list_events(EventQuery {
            team_id: e.team.id.clone(),
            cursor: None,
            limit: Some(200),
        })
        .unwrap()
        .events;
    for ev in events.iter().filter(|ev| {
        matches!(
            ev.kind,
            EventKind::HarnessCreated | EventKind::HarnessUpdated
        )
    }) {
        if ev.data.to_string().contains(SECRET) {
            leaks.push(format!("活动 {:?} 的 data.args", ev.kind));
        }
    }
    // c) 开始一次执行：Agent 立即退出并在 stderr 打印含该密钥的一行。
    let task = e.task("泄露测试");
    let run = e.ts.start_run(&task.id).unwrap();
    let run = e.wait_end(&run.id);
    assert_eq!(run.status, RunStatus::Failed, "{run:#?}");
    assert_eq!(run.failure_kind, Some(FailureKind::AgentExited));
    if run.harness.args.iter().any(|a| a.contains(SECRET)) {
        leaks.push("Run 快照 harness.args（runs.harness_args）".into());
    }
    // d) 失败原因、任务受阻原因、工作日志。
    if run.failure.as_deref().unwrap_or_default().contains(SECRET) {
        leaks.push("Run.failure（Agent stderr 尾部）".into());
    }
    if e.task_of(&task.id)
        .blocked_reason
        .unwrap_or_default()
        .contains(SECRET)
    {
        leaks.push("任务受阻原因".into());
    }
    e.ts.get_team_overview(&e.team.id).unwrap();
    if e.journal().contains(SECRET) {
        leaks.push("工作日志.md".into());
    }
    let _ = updated;
    assert!(leaks.is_empty(), "疑似密钥出现在：{leaks:#?}");
}

// ---------------------------------------------------------------- K-RUN

/// K-RUN-004 B)：Git 仓库没有任何提交时启动失败，任务受阻。
#[test]
fn k_run_004_git_repo_without_commits_fails_to_launch() {
    let e = env(Kind::EmptyRepo);
    let task = e.task("无提交");
    let run = e.ts.start_run(&task.id).unwrap();
    assert_eq!(run.isolation, RunIsolation::Worktree);
    let run = e.wait_end(&run.id);
    assert_eq!(run.status, RunStatus::Failed, "{run:#?}");
    assert_eq!(run.failure_kind, Some(FailureKind::Launch));
    let failure = run.failure.unwrap();
    assert!(failure.contains("没有任何提交"), "{failure}");
    let t = e.task_of(&task.id);
    assert_eq!(t.status, TaskStatus::Blocked);
    assert!(t.blocked_reason.unwrap().starts_with("执行失败："));
}

/// K-RUN-005：并发 `start_run` 恰有一个成功；活动 Run 始终至多一个。
#[test]
fn k_run_005_concurrent_start_run_has_exactly_one_winner() {
    let e = env(Kind::Plain);
    let task = e.task("长任务 [mock:hang]");
    // 1) 第一次 start_run 后立即再调用。
    let first = e.ts.start_run(&task.id).unwrap();
    let err = e.ts.start_run(&task.id).unwrap_err();
    assert_eq!(err.kind(), ErrorKind::Conflict);
    assert!(err.to_string().contains("任务正在执行（第 1 次）"), "{err}");
    e.ts.cancel_run(&first.id).unwrap();

    // 2) 两个线程同时开始，各 20 轮（每轮取消后任务回到 READY）。
    let conn = rusqlite::Connection::open(e.dir.path().join("data/t.db")).unwrap();
    for round in 0..20 {
        let barrier = std::sync::Barrier::new(2);
        let results: Vec<Result<Run, teamspace_core::Error>> = std::thread::scope(|s| {
            let handles: Vec<_> = (0..2)
                .map(|_| {
                    s.spawn(|| {
                        barrier.wait();
                        e.ts.start_run(&task.id)
                    })
                })
                .collect();
            handles.into_iter().map(|h| h.join().unwrap()).collect()
        });
        let wins = results.iter().filter(|r| r.is_ok()).count();
        assert_eq!(wins, 1, "第 {round} 轮应恰有一个成功: {results:?}");
        for r in results.iter().filter_map(|r| r.as_ref().err()) {
            assert_eq!(r.kind(), ErrorKind::Conflict, "第 {round} 轮: {r}");
        }
        let active: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM runs WHERE task_id = ?1
                 AND status IN ('queued', 'running', 'waiting_permission')",
                [&task.id],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(active, 1, "第 {round} 轮");
        let run = results.into_iter().find_map(Result::ok).unwrap();
        e.ts.cancel_run(&run.id).unwrap();
        assert_eq!(e.task_of(&task.id).status, TaskStatus::Ready);
    }
}

fn second_team(e: &Env, name: &str, workspace: &Path) -> (Team, Task) {
    let team =
        e.ts.create_team(CreateTeamInput {
            name: name.into(),
            description: String::new(),
            workspace_path: workspace.display().to_string(),
            roles: vec![NewRole {
                name: "前端".into(),
                code: None,
                task_prefix: "FE".into(),
                description: String::new(),
                guidelines: String::new(),
                is_leader: false,
            }],
        })
        .unwrap();
    let h =
        e.ts.create_harness(CreateHarnessInput {
            team_id: team.id.clone(),
            name: "Mock".into(),
            preset: AcpPreset::Mock,
            command: None,
            args: None,
            env_profile: None,
        })
        .unwrap();
    let role = e.ts.list_roles(&team.id).unwrap().remove(0).role;
    e.ts.bind_role_harness(&role.id, &h.id).unwrap();
    let task =
        e.ts.create_task(CreateTaskInput {
            team_id: team.id.clone(),
            title: "另一个团队的任务 [mock:hang]".into(),
            description: String::new(),
            owner_role_id: role.id,
            status: TaskStatus::Ready,
            file_refs: vec![],
            depends_on: vec![],
        })
        .unwrap()
        .task;
    (team, task)
}

/// K-RUN-006 A)：普通文件夹同一时间只有一个执行，结束后另一个可启动。
#[test]
fn k_run_006_plain_folder_allows_a_single_active_run() {
    let e = env(Kind::Plain);
    let a = e.task("甲 [mock:hang]");
    let b = e.task("乙 [mock:hang]");
    let run_a = e.ts.start_run(&a.id).unwrap();
    e.wait(&run_a.id, "running", |r| r.status == RunStatus::Running);
    let err = e.ts.start_run(&b.id).unwrap_err();
    assert_eq!(err.kind(), ErrorKind::Conflict);
    assert!(
        err.to_string().contains("同一文件夹同时只能运行一个执行"),
        "{err}"
    );
    let problems = e.ts.get_task(&b.id).unwrap().run_problems;
    assert!(
        problems
            .iter()
            .any(|p| p.contains("同一文件夹同时只能运行一个执行")),
        "{problems:?}"
    );
    e.ts.cancel_run(&run_a.id).unwrap();
    let run_b = e.ts.start_run(&b.id).unwrap();
    e.ts.cancel_run(&run_b.id).unwrap();
}

/// K-RUN-006 B)：Git 仓库的两个任务各自有独立 worktree 与分支，可同时执行。
#[test]
fn k_run_006_git_repo_runs_in_parallel_in_separate_worktrees() {
    let e = env(Kind::Repo);
    let a = e.task("甲 [mock:hang]");
    let b = e.task("乙 [mock:hang]");
    let run_a = e.ts.start_run(&a.id).unwrap();
    let run_b = e.ts.start_run(&b.id).unwrap();
    let run_a = e.wait(&run_a.id, "running", |r| r.status == RunStatus::Running);
    let run_b = e.wait(&run_b.id, "running", |r| r.status == RunStatus::Running);
    assert_eq!(run_a.isolation, RunIsolation::Worktree);
    assert_eq!(run_b.isolation, RunIsolation::Worktree);
    assert_eq!(run_a.branch.as_deref(), Some("teamspace/FE-001-r1"));
    assert_eq!(run_b.branch.as_deref(), Some("teamspace/FE-002-r1"));
    assert_ne!(run_a.exec_dir, run_b.exec_dir);
    assert!(Path::new(&run_a.exec_dir).is_dir() && Path::new(&run_b.exec_dir).is_dir());
    e.ts.cancel_run(&run_a.id).unwrap();
    e.ts.cancel_run(&run_b.id).unwrap();
}

/// 在另一个团队里尝试开始执行，期望因同一物理文件夹被占用而 conflict；返回结果供断言。
fn start_in_alias_team(e: &Env, alias: &Path, name: &str) -> Result<Run, teamspace_core::Error> {
    let busy = e.task("占用者 [mock:hang]");
    let run = e.ts.start_run(&busy.id).unwrap();
    e.wait(&run.id, "running", |r| r.status == RunStatus::Running);
    let (_, task) = second_team(e, name, alias);
    e.ts.start_run(&task.id)
}

/// K-RUN-006 C)：同一物理文件夹经符号链接路径创建的另一个团队，也应受「同一文件夹只运行一个执行」约束。
#[test]
fn k_run_006_symlink_alias_of_the_same_folder_is_treated_as_the_same_folder() {
    let e = env(Kind::Plain);
    let link = e.path("ws-link");
    symlink(&e.ws, &link).unwrap();
    let result = start_in_alias_team(&e, &link, "别名团队");
    let err = result.expect_err("符号链接别名应被识别为同一文件夹");
    assert_eq!(err.kind(), ErrorKind::Conflict);
    assert!(
        err.to_string().contains("同一文件夹同时只能运行一个执行"),
        "{err}"
    );
}

/// K-RUN-006 D)：macOS 上只差大小写的路径是同一个文件夹。
#[test]
fn k_run_006_case_only_difference_is_treated_as_the_same_folder() {
    let e = env(Kind::Plain);
    let upper = e.path("WS");
    if cfg!(not(target_os = "macos")) || !upper.is_dir() {
        eprintln!("文件系统区分大小写，跳过 K-RUN-006 D");
        return;
    }
    let result = start_in_alias_team(&e, &upper, "大小写团队");
    let err = result.expect_err("只差大小写的路径应被识别为同一文件夹");
    assert_eq!(err.kind(), ErrorKind::Conflict);
}

/// K-RUN-006 E)：嵌套文件夹（团队 B 的工作空间是团队 A 工作空间的子目录）。
#[test]
fn k_run_006_nested_workspace_is_treated_as_overlapping() {
    let e = env(Kind::Plain);
    let sub = e.ws.join("sub");
    std::fs::create_dir_all(&sub).unwrap();
    let result = start_in_alias_team(&e, &sub, "嵌套团队");
    let err = result.expect_err("嵌套的工作空间应与外层冲突");
    assert_eq!(err.kind(), ErrorKind::Conflict);
}

/// K-RUN-007：取消（Agent 收到 session/cancel；二次取消；等待权限期间取消；不响应取消的 Agent 宽限期后强杀）。
#[test]
fn k_run_007_cancel_notifies_agent_and_second_cancel_conflicts() {
    let e = env(Kind::Plain);
    let log = e.path("agent.log");
    e.fake(
        "fake",
        &[
            "mode=hang_log_cancel".into(),
            format!("log={}", log.display()),
        ],
    );
    let task = e.task("取消我");
    let run = e.ts.start_run(&task.id).unwrap();
    e.wait(&run.id, "running", |r| r.status == RunStatus::Running);
    let cancelled = e.ts.cancel_run(&run.id).unwrap();
    assert_eq!(cancelled.status, RunStatus::Cancelled);
    assert!(!cancelled.taken_over);
    assert_eq!(e.task_of(&task.id).status, TaskStatus::Ready);
    let lines = log_lines(&log);
    assert!(
        lines.contains(&"cancel".to_owned()),
        "Agent 应收到 session/cancel: {lines:?}"
    );
    let finished =
        e.ts.get_task(&task.id)
            .unwrap()
            .events
            .into_iter()
            .find(|ev| ev.kind == EventKind::RunFinished)
            .unwrap();
    assert_eq!(finished.actor, Actor::Human);
    let err = e.ts.cancel_run(&run.id).unwrap_err();
    assert_eq!(err.kind(), ErrorKind::Conflict);
    assert!(
        err.to_string().contains("执行已结束（已取消），无需取消"),
        "{err}"
    );
}

#[test]
fn k_run_007_cancel_while_waiting_for_permission_revokes_the_request() {
    let e = env(Kind::Plain);
    let task = e.task("构建 [mock:perm]");
    let run = e.ts.start_run(&task.id).unwrap();
    let waiting = e.wait(&run.id, "waiting_permission", |r| {
        r.status == RunStatus::WaitingPermission
    });
    let request_id = waiting.pending_permission.clone().unwrap().request_id;
    let cancelled = e.ts.cancel_run(&run.id).unwrap();
    assert_eq!(cancelled.status, RunStatus::Cancelled);
    assert!(cancelled.pending_permission.is_none());
    let decision = e
        .updates(&run.id)
        .into_iter()
        .find_map(|u| match u.body {
            RunUpdateBody::Permission {
                request_id: id,
                decision,
                ..
            } if id == request_id => decision,
            _ => None,
        })
        .expect("权限请求应有决定记录");
    assert_eq!(decision.by, "cancelled");
    assert_eq!(decision.kind, "cancelled");
    assert_eq!(e.task_of(&task.id).status, TaskStatus::Ready);
}

#[test]
fn k_run_007_unresponsive_agent_is_killed_after_the_grace_period() {
    let e = env(Kind::Plain);
    let pidfile = e.path("agent.pid");
    let log = e.path("agent.log");
    e.fake(
        "fake",
        &[
            "mode=hang_ignore_cancel".into(),
            format!("pidfile={}", pidfile.display()),
            format!("log={}", log.display()),
        ],
    );
    e.ts.set_run_settings(RunSettings {
        cancel_grace: Duration::from_millis(200),
        ..RunSettings::default()
    });
    let task = e.task("不听话");
    let run = e.ts.start_run(&task.id).unwrap();
    e.wait(&run.id, "running", |r| r.status == RunStatus::Running);
    let pid = std::fs::read_to_string(&pidfile).unwrap();
    assert!(pid_alive(&pid), "取消前 Agent 进程应在运行");
    let started = Instant::now();
    let cancelled = e.ts.cancel_run(&run.id).unwrap();
    assert!(
        started.elapsed() < Duration::from_secs(10),
        "{:?}",
        started.elapsed()
    );
    assert_eq!(cancelled.status, RunStatus::Cancelled);
    assert!(log_lines(&log).contains(&"cancel".to_owned()));
    let deadline = Instant::now() + Duration::from_secs(5);
    while pid_alive(&pid) && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(50));
    }
    assert!(
        !pid_alive(&pid),
        "宽限期后整个进程应被终止，无残留进程 {pid}"
    );
    assert_eq!(e.task_of(&task.id).status, TaskStatus::Ready);
}

/// K-RUN-008：空闲超时的默认值；等待 Human 确认期间不计时。
#[test]
fn k_run_008_waiting_for_human_does_not_count_towards_the_idle_timeout() {
    assert_eq!(
        RunSettings::default().idle_timeout,
        Duration::from_secs(30 * 60)
    );
    let e = env(Kind::Plain);
    e.ts.set_run_settings(RunSettings {
        idle_timeout: Duration::from_millis(1500),
        ..RunSettings::default()
    });
    let task = e.task("慢确认 [mock:perm]");
    let run = e.ts.start_run(&task.id).unwrap();
    let waiting = e.wait(&run.id, "waiting_permission", |r| {
        r.status == RunStatus::WaitingPermission
    });
    std::thread::sleep(Duration::from_millis(3500));
    let still = e.ts.get_run(&run.id).unwrap();
    assert_eq!(
        still.status,
        RunStatus::WaitingPermission,
        "等待期间不应超时: {still:#?}"
    );
    let request = waiting.pending_permission.unwrap();
    e.ts.respond_permission(RespondPermissionInput {
        run_id: run.id.clone(),
        request_id: request.request_id,
        option_id: "allow".into(),
    })
    .unwrap();
    let done = e.wait_end(&run.id);
    assert_eq!(done.status, RunStatus::Succeeded, "{done:#?}");
    assert_ne!(done.failure_kind, Some(FailureKind::Timeout));
    assert!(done.summary.unwrap().contains("命令 echo hello 已执行"));
}

/// K-RUN-009（limit 类别与重试）：max_tokens / max_turn_requests 归为 limit，任务受阻，可重试。
#[test]
fn k_run_009_limit_failures_block_the_task_and_can_be_retried() {
    let e = env(Kind::Plain);
    for (mode, reason) in [
        ("limit_tokens", "max_tokens"),
        ("limit_turns", "max_turn_requests"),
    ] {
        let h = e.fake(mode, &[format!("mode={mode}")]);
        let task = e.task(&format!("上限 {mode}"));
        let run = e.ts.start_run(&task.id).unwrap();
        let run = e.wait_end(&run.id);
        assert_eq!(run.status, RunStatus::Failed, "{mode}: {run:#?}");
        assert_eq!(run.failure_kind, Some(FailureKind::Limit), "{mode}");
        assert_eq!(run.stop_reason.as_deref(), Some(reason));
        assert!(
            run.failure.as_deref().unwrap().contains(reason),
            "{mode}: {:?}",
            run.failure
        );
        let t = e.task_of(&task.id);
        assert_eq!(t.status, TaskStatus::Blocked, "{mode}");
        assert!(
            t.blocked_reason.unwrap().starts_with("执行失败："),
            "{mode}"
        );
        // 重试产生第 2 次执行。
        let retry = e.ts.start_run(&task.id).unwrap();
        assert_eq!(retry.attempt, 2);
        assert_eq!(e.wait_end(&retry.id).failure_kind, Some(FailureKind::Limit));
        assert_eq!(e.ts.list_runs(&task.id).unwrap().len(), 2);
        let _ = h;
    }
}

/// K-RUN-010：应用退出（drop）后 Agent 进程已被终止，重新打开时执行为已中断。
#[test]
fn k_run_010_agent_process_is_terminated_when_the_app_exits() {
    let e = env(Kind::Plain);
    let pidfile = e.path("agent.pid");
    e.fake(
        "fake",
        &[
            "mode=hang_ignore_cancel".into(),
            format!("pidfile={}", pidfile.display()),
        ],
    );
    let task = e.task("重启我");
    let run = e.ts.start_run(&task.id).unwrap();
    e.wait(&run.id, "running", |r| r.status == RunStatus::Running);
    let pid = std::fs::read_to_string(&pidfile).unwrap();
    assert!(pid_alive(&pid));
    let Env { dir, ts, .. } = e;
    drop(ts);
    let deadline = Instant::now() + Duration::from_secs(5);
    while pid_alive(&pid) && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(50));
    }
    assert!(!pid_alive(&pid), "应用退出后 Agent 子进程应已终止");
    let ts = open(dir.path());
    let run = ts.get_run(&run.id).unwrap();
    assert_eq!(run.status, RunStatus::Interrupted);
    assert!(run.failure.unwrap().contains("应用退出"));
    assert!(run.pending_permission.is_none());
    assert_eq!(
        ts.get_task(&task.id).unwrap().task.status,
        TaskStatus::Doing
    );
}

/// K-RUN-016：执行摘要与改动文件写入岗位工作日志（行首格式、成功、>10 个文件、超长摘要）。
#[test]
fn k_run_016_success_journal_lines_and_changed_file_suffix() {
    let e = env(Kind::Plain);
    let process = e.ws.join("Teamspace/前端/过程文件");
    // 15 个改动文件：只列前 10 个并以「 等 15 个」结尾。
    e.fake(
        "fake15",
        &[
            "mode=work".into(),
            format!("dir={}", process.display()),
            "count=15".into(),
        ],
    );
    let task = e.task("改很多文件");
    let run = e.ts.start_run(&task.id).unwrap();
    let run = e.wait_end(&run.id);
    assert_eq!(run.status, RunStatus::Succeeded, "{run:#?}");
    assert_eq!(run.changed_files.len(), 15);
    e.ts.get_team_overview(&e.team.id).unwrap();
    let lines = journal_lines_of(&e, &task.key);
    assert!(
        lines.iter().all(|l| !l.starts_with(&task.key)),
        "正文开头的任务编号应去掉: {lines:?}"
    );
    assert!(
        lines.contains(&"开始第 1 次执行（Harness「fake15」，原地执行）".to_owned()),
        "{lines:?}"
    );
    let done = lines
        .iter()
        .find(|l| l.starts_with("第 1 次执行完成（fake15）"))
        .expect("完成行");
    assert!(done.contains("；摘要：全部完成。"), "{done}");
    let prefix = "Teamspace/前端/过程文件/f";
    let listed: Vec<String> = (1..=10).map(|i| format!("{prefix}{i:02}.md")).collect();
    assert!(
        done.contains(&format!("；改动文件：{}", listed.join("、"))),
        "{done}"
    );
    assert!(done.ends_with(" 等 15 个"), "{done}");
    assert!(!done.contains("f11.md"), "{done}");
    assert!(
        lines
            .iter()
            .any(|l| l.starts_with("状态 执行中 → 待验收（执行完成）")),
        "{lines:?}"
    );
    assert!(!done.contains('\n'));

    // 恰好 10 个文件：不追加后缀。
    let e = env(Kind::Plain);
    let process = e.ws.join("Teamspace/前端/过程文件");
    e.fake(
        "fake10",
        &[
            "mode=work".into(),
            format!("dir={}", process.display()),
            "count=10".into(),
        ],
    );
    let task = e.task("改十个");
    let run = e.ts.start_run(&task.id).unwrap();
    assert_eq!(e.wait_end(&run.id).changed_files.len(), 10);
    e.ts.get_team_overview(&e.team.id).unwrap();
    let done = journal_lines_of(&e, &task.key)
        .into_iter()
        .find(|l| l.starts_with("第 1 次执行完成"))
        .unwrap();
    assert!(done.ends_with("f10.md"), "恰好 10 个时不追加后缀: {done}");
    assert!(!done.contains(" 等 "), "{done}");
}

#[test]
fn k_run_016_long_summary_is_truncated_and_markup_removed() {
    let e = env(Kind::Plain);
    e.fake("长摘要", &["mode=work".into(), "summary_len=700".into()]);
    let task = e.task("写很长的总结");
    let run = e.ts.start_run(&task.id).unwrap();
    let run = e.wait_end(&run.id);
    let summary = run.summary.clone().unwrap();
    assert_eq!(summary.chars().count(), 601, "600 字符加省略号");
    assert!(summary.ends_with('…'));
    assert!(!summary.contains("deliver-assets"), "展示标记应去掉");
    e.ts.get_team_overview(&e.team.id).unwrap();
    // 每条日志都是单行，行首格式正确。
    for line in e.journal().lines().filter(|l| l.starts_with("- ")) {
        assert!(line.contains(" · "), "{line}");
    }
    let done = journal_lines_of(&e, &task.key)
        .into_iter()
        .find(|l| l.starts_with("第 1 次执行完成"))
        .unwrap();
    assert!(done.contains(&format!("；摘要：{summary}")), "{done}");
    assert!(!done.contains("deliver-assets") && !done.contains('\n'));
}

#[test]
fn k_run_016_failure_cancel_and_takeover_journal_lines() {
    let e = env(Kind::Plain);
    // 失败。
    let failed = e.task("坏任务 [mock:fail]");
    let run = e.ts.start_run(&failed.id).unwrap();
    e.wait_end(&run.id);
    // 取消。
    let cancelled = e.task("取消我 [mock:hang]");
    let run = e.ts.start_run(&cancelled.id).unwrap();
    e.wait(&run.id, "running", |r| r.status == RunStatus::Running);
    e.ts.cancel_run(&run.id).unwrap();
    // 人工接管。
    let taken = e.task("接管我 [mock:hang]");
    let run = e.ts.start_run(&taken.id).unwrap();
    e.wait(&run.id, "running", |r| r.status == RunStatus::Running);
    e.ts.takeover_run(&run.id).unwrap();
    e.ts.get_team_overview(&e.team.id).unwrap();

    let lines = journal_lines_of(&e, &failed.key);
    assert!(
        lines
            .iter()
            .any(|l| l.starts_with("第 1 次执行失败（Mock）；原因：协议错误")),
        "{lines:?}"
    );
    assert!(
        lines
            .iter()
            .any(|l| l.starts_with("状态 执行中 → 受阻：执行失败：协议错误")),
        "{lines:?}"
    );

    let lines = journal_lines_of(&e, &cancelled.key);
    assert!(
        lines
            .iter()
            .any(|l| l.starts_with("第 1 次执行已取消（Mock）；原因：已取消")),
        "{lines:?}"
    );
    assert!(
        lines
            .iter()
            .any(|l| l.starts_with("状态 执行中 → READY（执行已取消）")),
        "{lines:?}"
    );

    let lines = journal_lines_of(&e, &taken.key);
    let stop = lines
        .iter()
        .find(|l| l.starts_with("第 1 次执行已停止，人工接管（Mock）"))
        .unwrap_or_else(|| panic!("{lines:?}"));
    assert!(!stop.contains("；原因："), "{stop}");
    assert!(
        !lines.iter().any(|l| l.contains("执行中 →")),
        "接管后任务保持 DOING，不应有离开执行中的状态行: {lines:?}"
    );
    // 每条日志行都是「- 时间 · 编号 · 正文」且为单行。
    for key in [&failed.key, &cancelled.key, &taken.key] {
        assert!(!journal_lines_of(&e, key).is_empty());
    }
}

// ---------------------------------------------------------------- K-PERM

/// K-PERM-004（规则层）：其他类型的权限请求一律转 Human，读取与搜索自动允许。
#[test]
fn k_perm_004_other_kinds_ask_and_read_search_allow() {
    let dir = tempfile::tempdir().unwrap();
    let ws = dir.path().canonicalize().unwrap();
    let folder = ws.join("Teamspace/前端");
    std::fs::create_dir_all(&folder).unwrap();
    let scope = Scope::new(
        &ws,
        RunIsolation::InPlace,
        std::slice::from_ref(&folder),
        &[],
    );
    let inside = folder.join("过程文件/a.md").display().to_string();
    let cases: [(Option<&str>, &str); 8] = [
        (Some("execute"), "执行命令"),
        (Some("delete"), "删除"),
        (Some("move"), "移动"),
        (Some("fetch"), "网络"),
        (Some("switch_mode"), "模式"),
        (Some("think"), "think"),
        (Some("other"), "other"),
        (None, "没有说明操作类型"),
    ];
    for (kind, needle) in cases {
        match scope.decide(kind, std::slice::from_ref(&inside)) {
            teamspace_core::run::policy::Verdict::Ask(reason) => {
                assert!(reason.contains(needle), "{kind:?}: {reason}")
            }
            other => panic!("{kind:?} 应转 Human，实际 {other:?}"),
        }
    }
    for kind in ["read", "search"] {
        assert!(matches!(
            scope.decide(Some(kind), &["/etc/hosts".to_owned()]),
            teamspace_core::run::policy::Verdict::Allow(_)
        ));
    }
}

/// K-PERM-004（driver 层）：自动允许的 read 请求不授予后续 `fs/write_text_file` 的写权限。
#[test]
#[ignore = "缺陷 K-PERM-004/K-PERM-008（Q-01/N-02）: read 类请求被规则自动允许时也 grant 其路径，之后对该路径的写入被「Human 已允许对应的权限请求」放行"]
fn k_perm_004_auto_allowed_read_does_not_grant_write() {
    let e = env(Kind::Plain);
    let target = e.ws.join("danger.txt");
    e.fake(
        "readallow",
        &[
            "mode=readallow".into(),
            format!("path={}", target.display()),
            format!("log={}", e.path("agent.log").display()),
        ],
    );
    let task = e.task("读后写");
    let run = e.ts.start_run(&task.id).unwrap();
    let run = e.wait_end(&run.id);
    assert_eq!(run.status, RunStatus::Succeeded, "{run:#?}");
    let log = log_lines(&e.path("agent.log"));
    assert!(
        log.iter()
            .any(|l| l.starts_with("write_error=Teamspace 未允许这次写入")),
        "范围外路径不应因 read 请求而可写: {log:?}"
    );
    assert!(!target.exists(), "文件不应被创建");
}

/// K-PERM-007：未知请求 ID、选项不属于请求、多个并发请求依次处理、执行结束后回应。
#[test]
fn k_perm_007_multiple_requests_and_invalid_responses() {
    let e = env(Kind::Plain);
    let log = e.path("agent.log");
    e.fake(
        "perm2",
        &["mode=perm2".into(), format!("log={}", log.display())],
    );
    let task = e.task("两个确认");
    let run = e.ts.start_run(&task.id).unwrap();
    e.wait(&run.id, "waiting_permission", |r| {
        r.status == RunStatus::WaitingPermission
    });
    let deadline = Instant::now() + Duration::from_secs(20);
    let prompts = loop {
        let list: Vec<PermissionPrompt> = e
            .events
            .lock()
            .unwrap()
            .iter()
            .filter_map(|ev| match ev {
                RunEvent::PermissionRequested { prompt, .. } => Some(prompt.clone()),
                _ => None,
            })
            .collect();
        if list.len() >= 2 {
            break list;
        }
        assert!(Instant::now() < deadline, "应收到两个权限请求事件");
        std::thread::sleep(Duration::from_millis(30));
    };
    let respond = |request_id: &str, option: &str| {
        e.ts.respond_permission(RespondPermissionInput {
            run_id: run.id.clone(),
            request_id: request_id.into(),
            option_id: option.into(),
        })
    };
    // 1) 不存在的请求 ID。
    let err = respond("no-such-request", "allow").unwrap_err();
    assert_eq!(err.kind(), ErrorKind::NotFound);
    assert!(err.to_string().contains("权限请求不存在或已处理"), "{err}");
    // 2) 选项不属于该请求。
    assert_eq!(
        respond(&prompts[0].request_id, "nope").unwrap_err().kind(),
        ErrorKind::Invalid
    );
    // 3) 先回应第一个：仍 waiting_permission，pending 指向第二个。
    let after_first = respond(&prompts[0].request_id, "allow").unwrap();
    assert_eq!(after_first.status, RunStatus::WaitingPermission);
    assert_eq!(
        after_first.pending_permission.unwrap().request_id,
        prompts[1].request_id
    );
    // 已处理的请求不能再回应。
    assert_eq!(
        respond(&prompts[0].request_id, "allow").unwrap_err().kind(),
        ErrorKind::NotFound
    );
    let after_second = respond(&prompts[1].request_id, "reject").unwrap();
    assert_eq!(after_second.status, RunStatus::Running);
    assert!(after_second.pending_permission.is_none());
    let done = e.wait_end(&run.id);
    assert_eq!(done.status, RunStatus::Succeeded);
    // 4) 执行已结束后回应：conflict。
    let err = respond(&prompts[1].request_id, "allow").unwrap_err();
    assert_eq!(err.kind(), ErrorKind::Conflict);
    assert!(err.to_string().contains("执行已结束"), "{err}");
    let log = log_lines(&log);
    assert!(
        log.iter()
            .any(|l| l.starts_with("perm1=") && l.contains("allow")),
        "{log:?}"
    );
    assert!(
        log.iter()
            .any(|l| l.starts_with("perm2=") && l.contains("reject")),
        "{log:?}"
    );
}

/// K-PERM-010：read 请求但 Agent 没有 allow 选项时转 Human，且不授予写权限。
#[test]
fn k_perm_010_read_without_allow_option_goes_to_human_and_grants_nothing() {
    let e = env(Kind::Plain);
    let target = e.ws.join("outside-note.md");
    let log = e.path("agent.log");
    e.fake(
        "readperm",
        &[
            "mode=readperm".into(),
            format!("path={}", target.display()),
            format!("log={}", log.display()),
        ],
    );
    let task = e.task("只读请求");
    let run = e.ts.start_run(&task.id).unwrap();
    let waiting = e.wait(&run.id, "waiting_permission", |r| {
        r.status == RunStatus::WaitingPermission
    });
    let prompt = waiting.pending_permission.unwrap();
    assert_eq!(prompt.tool_kind.as_deref(), Some("read"));
    assert!(
        prompt
            .reason
            .contains("但 Agent 没有提供「允许」选项，需要确认"),
        "{}",
        prompt.reason
    );
    e.ts.respond_permission(RespondPermissionInput {
        run_id: run.id.clone(),
        request_id: prompt.request_id,
        option_id: "reject".into(),
    })
    .unwrap();
    let done = e.wait_end(&run.id);
    assert_eq!(done.status, RunStatus::Succeeded, "{done:#?}");
    let lines = log_lines(&log);
    assert!(
        lines
            .iter()
            .any(|l| l.starts_with("write_error=Teamspace 未允许这次写入")),
        "拒绝后写入应被拒绝: {lines:?}"
    );
    assert!(!target.exists(), "文件不应被创建");
    let write = e
        .updates(&run.id)
        .into_iter()
        .find_map(|u| match u.body {
            RunUpdateBody::FileWrite { allowed, .. } => Some(allowed),
            _ => None,
        })
        .unwrap();
    assert!(!write);
}

/// K-PERM-009 B)：引用文件在工作空间存在但未提交时，说明包与写权限使用工作空间中的绝对路径。
#[test]
fn k_perm_009_uncommitted_ref_uses_the_workspace_path_and_stays_writable() {
    let e = env(Kind::Repo);
    let new_file = e.ws.join("docs/new.md");
    std::fs::write(&new_file, "# 新文档\n").unwrap();
    let log = e.path("agent.log");
    let target = PathBuf::from(&e.team.workspace_path).join("docs/new.md");
    e.fake(
        "write-ref",
        &[
            "mode=write".into(),
            format!("path={}", target.display()),
            format!("log={}", log.display()),
        ],
    );
    let task = e.task_with("未提交的引用", &["docs/new.md"]);
    let run = e.ts.start_run(&task.id).unwrap();
    assert_eq!(run.isolation, RunIsolation::Worktree);
    let run = e.wait_end(&run.id);
    assert_eq!(run.status, RunStatus::Succeeded, "{run:#?}");
    let brief = user_brief(&e, &run.id);
    let expected = format!("`docs/new.md`：{}", target.display());
    assert!(
        brief.contains(&expected),
        "说明包应使用工作空间中的绝对路径:\n{brief}"
    );
    let ref_line = brief.lines().find(|l| l.contains("`docs/new.md`")).unwrap();
    assert!(
        !ref_line.contains("worktrees"),
        "引用路径不应指向 worktree: {ref_line}"
    );
    // 写权限：工作空间中的引用文件被列为可写。
    let lines = log_lines(&log);
    assert!(lines.contains(&"write_error=none".to_owned()), "{lines:?}");
    assert_eq!(
        std::fs::read_to_string(&new_file).unwrap(),
        "written-by-agent"
    );
    let write = e
        .updates(&run.id)
        .into_iter()
        .find_map(|u| match u.body {
            RunUpdateBody::FileWrite { allowed, .. } => Some(allowed),
            _ => None,
        })
        .unwrap();
    assert!(write);
}

/// K-PERM-009 C)：Git 仓库子目录作为工作空间时，执行目录是 worktree 中同一子目录，分支与基线取自仓库根。
#[test]
fn k_perm_009_git_subdirectory_workspace_runs_in_the_matching_worktree_subdirectory() {
    let e = env_at("repo/sub", Some("repo"), Kind::Repo);
    let task = e.task("子目录");
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
    assert!(
        exec.ends_with("sub"),
        "执行目录应是 worktree 中的 sub: {exec:?}"
    );
    assert!(exec.join("docs/spec.md").is_file());
    // 分支取自仓库根：在仓库根能看到这个分支。
    let out = Command::new("git")
        .arg("-C")
        .arg(e.dir.path().join("repo"))
        .args(["branch", "--list", "teamspace/FE-001-r1"])
        .output()
        .unwrap();
    assert!(String::from_utf8_lossy(&out.stdout).contains("teamspace/FE-001-r1"));
}

// ---------------------------------------------------------------- K-SEC

/// K-SEC-006：工作空间路径含空格、中文、`#`、引号时全流程可用。
#[test]
fn k_sec_006_special_characters_in_workspace_path_work_end_to_end() {
    let e = env_at("我的 项目 #1 'a'", None, Kind::Repo);
    assert!(e.team.workspace_path.contains("我的 项目 #1 'a'"));
    let task = e.task("特殊路径");
    let run = e.ts.start_run(&task.id).unwrap();
    assert_eq!(run.isolation, RunIsolation::Worktree);
    let run = e.wait_end(&run.id);
    assert_eq!(run.status, RunStatus::Succeeded, "{run:#?}");
    assert!(run.branch.is_some() && run.base_commit.as_ref().unwrap().len() == 40);
    // file:// URI 经百分号编码并能被还原：Mock 通过它读取引用文件。
    let read = e
        .updates(&run.id)
        .into_iter()
        .find_map(|u| match u.body {
            RunUpdateBody::ToolCall {
                tool_call_id,
                status,
                ..
            } if tool_call_id == "mock-read" => status,
            _ => None,
        })
        .unwrap();
    assert_eq!(read, "completed", "引用文件应能读取");
    let brief = user_brief(&e, &run.id);
    assert!(brief.contains("docs/spec.md"), "{brief}");
    e.ts.get_team_overview(&e.team.id).unwrap();
    assert!(e.journal().contains("执行完成"), "{}", e.journal());
    assert!(e.ws.join("Teamspace/前端/岗位说明.md").is_file());
    assert_eq!(
        e.ts.search_workspace_files(FileSearchQuery {
            team_id: e.team.id.clone(),
            query: "spec".into(),
        })
        .unwrap(),
        ["docs/spec.md"]
    );
    // Mock 默认流程不触碰工作空间自身：改动在 worktree 分支上。
    assert!(
        !e.ws
            .join(format!(
                "Teamspace/前端/过程文件/mock-{}.md",
                run.session_id.unwrap()
            ))
            .exists()
    );
}
