//! `teamspace-mcp` 集成测试共用的环境与最小 MCP 客户端。
//!
//! 真的起一个 Teamspace：临时数据库 + 临时目录里的控制 socket + 内置 Mock Agent，
//! 再以 stdio 启动 `teamspace-mcp` 二进制，按 MCP 协议（逐行 JSON-RPC）调用工具。
//! 不调用任何真实模型、不消耗额度。

#![allow(dead_code)]

use std::collections::HashMap;
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::Arc;
use std::sync::mpsc::{self, Receiver, RecvTimeoutError};
use std::time::{Duration, Instant};

use serde_json::{Value, json};
use teamspace_core::control::ControlServer;
use teamspace_core::model::{AcpPreset, CreateHarnessInput, CreateTeamInput, NewRole, RunStatus};
use teamspace_core::system_probe::LocalProbe;
use teamspace_core::{AgentCommand, HostConfig, Teamspace};

/// 内置 Mock Agent 的可执行文件（`cargo test` 会先编译例子目标）。
pub fn mock_agent_binary() -> PathBuf {
    // 测试可执行文件在 target/<profile>/deps/ 下，例子在同级 examples/ 下。
    let exe = std::env::current_exe().expect("当前可执行文件");
    let profile_dir = exe
        .parent()
        .and_then(Path::parent)
        .expect("target/<profile>");
    let candidate = profile_dir.join("examples").join("mock_agent");
    assert!(
        candidate.exists(),
        "找不到 Mock Agent：{}（先跑 `cargo build -p teamspace-mcp --example mock_agent`）",
        candidate.display()
    );
    candidate
}

/// 跑一条 git 命令（失败即测试失败）。
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

/// 一个团队、若干岗位、一个 Mock Harness，以及控制 socket。
pub struct Env {
    /// 控制服务：drop 时停止监听并删除 socket 文件。
    pub _control: ControlServer,
    /// 内核。
    pub ts: Arc<Teamspace>,
    /// 临时目录（数据库、socket、worktree 都在里面）。
    pub dir: tempfile::TempDir,
    /// 控制 socket 路径。
    pub socket: PathBuf,
    /// 团队 id。
    pub team_id: String,
    /// 岗位 id（与建团队时给的顺序一致）。
    pub role_ids: Vec<String>,
    /// Mock Harness 实例 id。
    pub harness_id: String,
    /// 工作空间目录（真实路径）。
    pub ws: PathBuf,
}

impl Env {
    /// 临时环境：工作空间（`git_repo` 为真时是一个已有提交的 Git 仓库）、Mock Harness、控制 socket。
    ///
    /// `roles` 是「岗位名 + 任务编号前缀」，所有岗位都绑定到同一个 Mock Harness。
    pub fn new(git_repo: bool, roles: &[(&str, &str)]) -> Env {
        let dir = tempfile::tempdir().unwrap();
        let workspace = dir.path().join("ws");
        std::fs::create_dir_all(&workspace).unwrap();
        std::fs::create_dir_all(dir.path().join("data")).unwrap();
        std::fs::write(workspace.join("README.md"), "# 临时工作空间\n").unwrap();
        if git_repo {
            git(&workspace, &["init", "-q", "-b", "main"]);
            git(&workspace, &["config", "user.email", "t@example.com"]);
            git(&workspace, &["config", "user.name", "t"]);
            git(&workspace, &["config", "commit.gpgsign", "false"]);
            git(&workspace, &["add", "."]);
            git(&workspace, &["commit", "-q", "-m", "init"]);
        }
        // 用真实路径（macOS 上临时目录是符号链接），避免 Git 与文件范围判断看到两条路径。
        let workspace = workspace.canonicalize().unwrap();
        let config = HostConfig {
            mock_agent: Some(AgentCommand {
                program: mock_agent_binary(),
                args: vec![],
            }),
        };
        let ts = Arc::new(
            Teamspace::open_with(
                &dir.path().join("data/teamspace.db"),
                Box::new(LocalProbe::new()),
                config,
            )
            .unwrap(),
        );
        // 取消执行时的宽限期缩短，测试不用等十秒。
        ts.set_run_settings(teamspace_core::RunSettings {
            cancel_grace: Duration::from_millis(200),
            ..Default::default()
        });
        let team = ts
            .create_team(CreateTeamInput {
                name: "产品团队".into(),
                description: "集成测试用的团队".into(),
                workspace_path: workspace.display().to_string(),
                roles: roles
                    .iter()
                    .map(|(name, prefix)| NewRole {
                        name: (*name).into(),
                        code: None,
                        task_prefix: (*prefix).into(),
                        description: format!("{name}岗位"),
                        guidelines: String::new(),
                        is_leader: false,
                    })
                    .collect(),
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
        let role_ids: Vec<String> = ts
            .list_roles(&team.id)
            .unwrap()
            .into_iter()
            .map(|view| view.role.id)
            .collect();
        for role_id in &role_ids {
            ts.bind_role_harness(role_id, &harness.id).unwrap();
        }
        let socket = dir.path().join("teamspace-control.sock");
        let control = teamspace_core::control::serve(Arc::clone(&ts), &socket).unwrap();
        Env {
            _control: control,
            ts,
            dir,
            socket,
            team_id: team.id,
            role_ids,
            harness_id: harness.id,
            ws: workspace,
        }
    }
}

/// 一次工具调用的结果。
pub struct ToolCall {
    /// 工具返回的文本（成功时是 JSON，失败时是中文说明）。
    pub text: String,
    /// 工具级错误（`isError`）。
    pub is_error: bool,
    /// 协议层的错误（工具不存在时）。
    pub protocol_error: Option<String>,
}

impl ToolCall {
    /// 工具返回的 JSON。
    pub fn json(&self) -> Value {
        serde_json::from_str(&self.text).unwrap_or_else(|err| {
            panic!("工具返回的不是 JSON：{err}\n{}", self.text);
        })
    }

    /// 断言调用成功并返回 JSON。
    pub fn ok_json(&self) -> Value {
        assert!(!self.is_error, "{}", self.text);
        self.json()
    }
}

/// 读一行响应的上限：服务不回应时测试要失败，不能挂死。
pub const READ_TIMEOUT: Duration = Duration::from_secs(60);

/// 一个请求的总上限（等 id 匹配的响应）。
const REQUEST_TIMEOUT: Duration = Duration::from_secs(120);

/// 最小 MCP 客户端：以 stdio 启动 `teamspace-mcp`，按 JSON-RPC（逐行）说话。
pub struct Client {
    child: Child,
    stdin: ChildStdin,
    /// 服务端输出由后台线程读进来，这里按行取（每次取都有 deadline）。
    lines: Receiver<String>,
    next_id: i64,
    /// 已经收到、但调用方还没来取的响应（并发请求时用）。
    buffered: HashMap<i64, Value>,
}

impl Client {
    /// 启动服务并完成 initialize 握手。
    pub fn start(socket: &Path) -> Client {
        let mut child = Command::new(env!("CARGO_BIN_EXE_teamspace-mcp"))
            .env("TEAMSPACE_CONTROL_SOCKET", socket)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .expect("启动 teamspace-mcp");
        let stdin = child.stdin.take().expect("stdin");
        let stdout = child.stdout.take().expect("stdout");
        let (sender, lines) = mpsc::channel();
        std::thread::spawn(move || read_lines(stdout, sender));
        let mut client = Client {
            child,
            stdin,
            lines,
            next_id: 0,
            buffered: HashMap::new(),
        };
        let result = client
            .result(
                "initialize",
                json!({
                    "protocolVersion": "2025-06-18",
                    "capabilities": {},
                    "clientInfo": { "name": "teamspace-mcp-test", "version": "0.1.0" },
                }),
            )
            .expect("initialize");
        assert!(
            result["protocolVersion"].is_string(),
            "initialize 应返回协议版本：{result}"
        );
        client.notify("notifications/initialized", json!({}));
        client
    }

    /// 发一条请求，返回完整的 JSON-RPC 响应。
    pub fn request(&mut self, method: &str, params: Value) -> Value {
        let id = self.send_request(method, params);
        self.response(id)
    }

    /// 发一条请求，只返回它的 id（不等响应）。
    ///
    /// 要同时观察两个请求谁先回来时用它：发出去之后用 [`Client::response`] 取响应。
    pub fn send_request(&mut self, method: &str, params: Value) -> i64 {
        self.next_id += 1;
        let id = self.next_id;
        let message = json!({ "jsonrpc": "2.0", "id": id, "method": method, "params": params });
        self.send(&message);
        id
    }

    /// 等指定 id 的响应；别的 id 的响应先存起来（不丢弃）。
    pub fn response(&mut self, id: i64) -> Value {
        if let Some(value) = self.buffered.remove(&id) {
            return value;
        }
        let deadline = Instant::now() + REQUEST_TIMEOUT;
        loop {
            let line = self.read_line();
            let value: Value = serde_json::from_str(&line).unwrap_or_else(|err| {
                panic!("响应不是 JSON：{err}\n{line}");
            });
            if value.get("id").and_then(Value::as_i64) == Some(id) {
                return value;
            }
            // 别的请求的响应：存起来，等它的调用方来取。
            if let Some(other) = value.get("id").and_then(Value::as_i64) {
                self.buffered.insert(other, value);
            }
            // 服务端通知（没有 id 或 id 不匹配）忽略，但不能无限等下去。
            assert!(
                Instant::now() < deadline,
                "等待响应 {id} 超时（{REQUEST_TIMEOUT:?}）"
            );
        }
    }

    /// 某个请求的响应是不是已经收到了（并发请求时用来判断谁先回来）。
    pub fn response_received(&self, id: i64) -> bool {
        self.buffered.contains_key(&id)
    }

    /// 发一条请求并取出 `result`。
    pub fn result(&mut self, method: &str, params: Value) -> Option<Value> {
        let response = self.request(method, params);
        if let Some(err) = response.get("error") {
            panic!("请求 {method} 失败：{err}");
        }
        Some(response.get("result").cloned().unwrap_or(Value::Null))
    }

    /// 发一条通知。
    pub fn notify(&mut self, method: &str, params: Value) {
        let message = json!({ "jsonrpc": "2.0", "method": method, "params": params });
        self.send(&message);
    }

    /// 调一个工具。
    pub fn call(&mut self, name: &str, arguments: Value) -> ToolCall {
        let id = self.call_async(name, arguments);
        self.tool_result(id)
    }

    /// 调一个工具，只返回请求 id（不等响应）。
    ///
    /// 要观察「另一个工具调用是否被阻塞」时用它：发出去之后用 [`Client::tool_result`] 取结果。
    pub fn call_async(&mut self, name: &str, arguments: Value) -> i64 {
        self.send_request(
            "tools/call",
            json!({ "name": name, "arguments": arguments }),
        )
    }

    /// 取一条工具调用的结果（配合 [`Client::call_async`]）。
    pub fn tool_result(&mut self, id: i64) -> ToolCall {
        let response = self.response(id);
        tool_call_of(&response)
    }

    /// 写一行 JSON。
    fn send(&mut self, message: &Value) {
        let line = message.to_string();
        self.stdin.write_all(line.as_bytes()).unwrap();
        self.stdin.write_all(b"\n").unwrap();
        self.stdin.flush().unwrap();
    }

    /// 读一行 JSON（带 deadline：服务不回应时测试失败，而不是挂死）。
    fn read_line(&mut self) -> String {
        match self.lines.recv_timeout(READ_TIMEOUT) {
            Ok(line) => line.trim().to_owned(),
            Err(RecvTimeoutError::Timeout) => {
                panic!("读响应超时（{READ_TIMEOUT:?}）：服务没有回应")
            }
            Err(RecvTimeoutError::Disconnected) => panic!("服务已退出"),
        }
    }
}

/// 一条 JSON-RPC 响应里的工具结果。
fn tool_call_of(response: &Value) -> ToolCall {
    if let Some(err) = response.get("error") {
        return ToolCall {
            text: err.to_string(),
            is_error: true,
            protocol_error: err
                .get("message")
                .and_then(Value::as_str)
                .map(str::to_owned),
        };
    }
    let result = response.get("result").cloned().unwrap_or(Value::Null);
    let text = result
        .get("content")
        .and_then(Value::as_array)
        .map(|blocks| {
            blocks
                .iter()
                .filter_map(|block| block.get("text").and_then(Value::as_str))
                .collect::<Vec<_>>()
                .join("\n")
        })
        .unwrap_or_default();
    ToolCall {
        text,
        is_error: result
            .get("isError")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        protocol_error: None,
    }
}

/// 后台读服务端的输出，逐行送进通道（服务退出时通道断开）。
fn read_lines(stdout: std::process::ChildStdout, sender: mpsc::Sender<String>) {
    let mut reader = BufReader::new(stdout);
    loop {
        let mut line = String::new();
        match reader.read_line(&mut line) {
            Ok(0) | Err(_) => break,
            Ok(_) => {
                if sender.send(line).is_err() {
                    break;
                }
            }
        }
    }
}

impl Drop for Client {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// 直接经控制 socket 发一条命令，返回内核的响应。
///
/// Leader 只能经 MCP 工具访问 Teamspace；这里绕过 MCP 直接问内核，用来验证权限边界
/// （白名单在 [`teamspace_core::dispatch`] 里把守）。
#[cfg(unix)]
pub fn socket_request(socket: &Path, cmd: &str, args: Value) -> Value {
    use std::os::unix::net::UnixStream;

    let mut stream = UnixStream::connect(socket).expect("连接控制 socket");
    // 读也有上限：服务不回应时测试要失败，而不是挂死。
    stream
        .set_read_timeout(Some(READ_TIMEOUT))
        .expect("设置读超时");
    let line = json!({ "id": 1, "cmd": cmd, "args": args }).to_string();
    stream.write_all(line.as_bytes()).unwrap();
    stream.write_all(b"\n").unwrap();
    stream.flush().unwrap();
    let mut reader = BufReader::new(stream);
    let mut reply = String::new();
    reader.read_line(&mut reply).unwrap();
    serde_json::from_str(reply.trim()).expect("响应是 JSON")
}

/// 执行状态在协议里的写法（snake_case）。
pub fn status_text(status: RunStatus) -> &'static str {
    match status {
        RunStatus::Queued => "queued",
        RunStatus::Running => "running",
        RunStatus::WaitingPermission => "waiting_permission",
        RunStatus::Succeeded => "succeeded",
        RunStatus::Failed => "failed",
        RunStatus::Cancelled => "cancelled",
        RunStatus::Interrupted => "interrupted",
    }
}

/// 轮询等待执行到达某个状态（带上限，不用固定 sleep）。
pub fn wait_for_status(client: &mut Client, run_id: &str, wanted: RunStatus) -> Value {
    let deadline = Instant::now() + Duration::from_secs(60);
    loop {
        let call = client.call("run_status", json!({ "run": run_id }));
        assert!(!call.is_error, "{}", call.text);
        let status = call.json();
        let current = status["status"].as_str().unwrap_or_default();
        if current == status_text(wanted) {
            return status;
        }
        assert!(
            Instant::now() < deadline,
            "等待 {wanted:?} 超时，当前：{status}"
        );
        std::thread::sleep(Duration::from_millis(50));
    }
}

/// 轮询直到条件成立（带上限，不用固定 sleep）。
pub fn wait_until(what: &str, mut done: impl FnMut() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(60);
    loop {
        if done() {
            return;
        }
        assert!(Instant::now() < deadline, "等待 {what} 超时");
        std::thread::sleep(Duration::from_millis(50));
    }
}
