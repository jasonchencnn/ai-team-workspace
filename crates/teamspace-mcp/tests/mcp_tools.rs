//! 集成测试：真的起一个 Teamspace（临时数据库 + 临时目录里的控制 socket），
//! 再以 stdio 启动 `teamspace-mcp` 二进制，按 MCP 协议调用工具。
//!
//! 覆盖：工具清单、查看类工具、按名称建任务并设依赖、开始执行（Mock Harness）、
//! 查询进度与结果、退回验收、越权调用、应用未运行时的提示。

use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};
use std::sync::Arc;
use std::time::{Duration, Instant};

use serde_json::{Value, json};
use teamspace_core::control::ControlServer;
use teamspace_core::model::{AcpPreset, CreateHarnessInput, CreateTeamInput, NewRole, RunStatus};
use teamspace_core::system_probe::LocalProbe;
use teamspace_core::{AgentCommand, HostConfig, Teamspace};

/// 一个团队、一个岗位、一个 Mock Harness，以及控制 socket。
struct Env {
    _dir: tempfile::TempDir,
    ts: Arc<Teamspace>,
    _control: ControlServer,
    socket: PathBuf,
    team_id: String,
    role_id: String,
}

impl Env {
    /// 临时环境：普通文件夹工作空间（原地执行），Mock Agent 当 Harness。
    fn new() -> Env {
        let dir = tempfile::tempdir().unwrap();
        let workspace = dir.path().join("ws");
        std::fs::create_dir_all(&workspace).unwrap();
        std::fs::create_dir_all(dir.path().join("data")).unwrap();
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
                roles: vec![NewRole {
                    name: "前端开发".into(),
                    code: None,
                    task_prefix: "FE".into(),
                    description: "负责界面".into(),
                    guidelines: String::new(),
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
        let socket = dir.path().join("teamspace-control.sock");
        let control = teamspace_core::control::serve(Arc::clone(&ts), &socket).unwrap();
        Env {
            _dir: dir,
            ts,
            _control: control,
            socket,
            team_id: team.id,
            role_id: role.id,
        }
    }
}

/// 内置 Mock Agent 的可执行文件（`cargo test` 会先编译例子目标）。
fn mock_agent_binary() -> PathBuf {
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

/// 一次工具调用的结果。
struct ToolCall {
    text: String,
    is_error: bool,
    /// 协议层的错误（工具不存在时）。
    protocol_error: Option<String>,
}

impl ToolCall {
    /// 工具返回的 JSON。
    fn json(&self) -> Value {
        serde_json::from_str(&self.text).unwrap_or_else(|err| {
            panic!("工具返回的不是 JSON：{err}\n{}", self.text);
        })
    }
}

/// 最小 MCP 客户端：以 stdio 启动 `teamspace-mcp`，按 JSON-RPC（逐行）说话。
struct Client {
    child: Child,
    stdin: ChildStdin,
    stdout: BufReader<ChildStdout>,
    next_id: i64,
}

impl Client {
    /// 启动服务并完成 initialize 握手。
    fn start(socket: &Path) -> Client {
        let mut child = Command::new(env!("CARGO_BIN_EXE_teamspace-mcp"))
            .env("TEAMSPACE_CONTROL_SOCKET", socket)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .expect("启动 teamspace-mcp");
        let stdin = child.stdin.take().expect("stdin");
        let stdout = BufReader::new(child.stdout.take().expect("stdout"));
        let mut client = Client {
            child,
            stdin,
            stdout,
            next_id: 0,
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
    fn request(&mut self, method: &str, params: Value) -> Value {
        self.next_id += 1;
        let id = self.next_id;
        let message = json!({ "jsonrpc": "2.0", "id": id, "method": method, "params": params });
        self.send(&message);
        loop {
            let line = self.read_line();
            let value: Value = serde_json::from_str(&line).unwrap_or_else(|err| {
                panic!("响应不是 JSON：{err}\n{line}");
            });
            if value.get("id").and_then(Value::as_i64) == Some(id) {
                return value;
            }
            // 服务端通知（没有 id 或 id 不匹配）忽略。
        }
    }

    /// 发一条请求并取出 `result`。
    fn result(&mut self, method: &str, params: Value) -> Option<Value> {
        let response = self.request(method, params);
        if let Some(err) = response.get("error") {
            panic!("请求 {method} 失败：{err}");
        }
        Some(response.get("result").cloned().unwrap_or(Value::Null))
    }

    /// 发一条通知。
    fn notify(&mut self, method: &str, params: Value) {
        let message = json!({ "jsonrpc": "2.0", "method": method, "params": params });
        self.send(&message);
    }

    /// 调一个工具。
    fn call(&mut self, name: &str, arguments: Value) -> ToolCall {
        let response = self.request(
            "tools/call",
            json!({ "name": name, "arguments": arguments }),
        );
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

    /// 写一行 JSON。
    fn send(&mut self, message: &Value) {
        let line = message.to_string();
        self.stdin.write_all(line.as_bytes()).unwrap();
        self.stdin.write_all(b"\n").unwrap();
        self.stdin.flush().unwrap();
    }

    /// 读一行 JSON。
    fn read_line(&mut self) -> String {
        let mut line = String::new();
        let read = self.stdout.read_line(&mut line).expect("读响应");
        assert!(read > 0, "服务已退出");
        line.trim().to_owned()
    }
}

impl Drop for Client {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// 执行状态在协议里的写法（snake_case）。
fn status_text(status: RunStatus) -> &'static str {
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

/// 等到执行到达某个状态。
fn wait_for_status(client: &mut Client, run_id: &str, wanted: RunStatus) -> Value {
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

/// 工具清单里有这些名字。
const EXPECTED_TOOLS: &[&str] = &[
    "list_teams",
    "team_overview",
    "list_roles",
    "list_harnesses",
    "task_board",
    "get_task",
    "run_status",
    "run_progress",
    "list_activity",
    "get_review",
    "pending_permissions",
    "create_task",
    "update_task",
    "set_dependencies",
    "change_task_status",
    "start_run",
    "cancel_run",
    "reject_review",
];

/// 不该由 Leader 触发的操作（D7、D8、D26）。
const FORBIDDEN_TOOLS: &[&str] = &[
    "approve_review",
    "respond_permission",
    "bind_role_harness",
    "unbind_role_harness",
    "create_harness",
    "create_team",
    "create_role",
    "resolve_merge_manually",
    "takeover_run",
];

#[test]
fn lists_the_leader_tools() {
    let env = Env::new();
    let mut client = Client::start(&env.socket);
    let tools = client.result("tools/list", json!({})).expect("tools/list");
    let list = tools["tools"].as_array().expect("工具数组");
    let names: Vec<&str> = list
        .iter()
        .filter_map(|tool| tool["name"].as_str())
        .collect();
    for expected in EXPECTED_TOOLS {
        assert!(names.contains(expected), "缺少工具 {expected}：{names:?}");
    }
    for forbidden in FORBIDDEN_TOOLS {
        assert!(
            !names.contains(forbidden),
            "不该提供 {forbidden}：{names:?}"
        );
    }
    // 描述是中文，参数有 JSON Schema。
    let list_teams = list
        .iter()
        .find(|tool| tool["name"] == "list_teams")
        .expect("list_teams");
    let description = list_teams["description"].as_str().unwrap_or_default();
    assert!(description.contains("团队"), "{description}");
    let create = list
        .iter()
        .find(|tool| tool["name"] == "create_task")
        .expect("create_task");
    let schema = &create["inputSchema"];
    assert_eq!(schema["type"], "object", "{schema}");
    let title = schema["properties"]["title"]["description"]
        .as_str()
        .unwrap_or_default();
    assert!(title.contains("标题"), "{title}");
}

#[test]
fn shows_teams_roles_harnesses_and_board() {
    let env = Env::new();
    let mut client = Client::start(&env.socket);

    let teams = client.call("list_teams", json!({}));
    assert!(!teams.is_error, "{}", teams.text);
    let teams = teams.json();
    assert_eq!(teams[0]["name"], "产品团队", "{teams}");

    let roles = client.call("list_roles", json!({}));
    assert!(!roles.is_error, "{}", roles.text);
    let roles = roles.json();
    assert_eq!(roles[0]["role"]["name"], "前端开发", "{roles}");

    let harnesses = client.call("list_harnesses", json!({}));
    assert!(!harnesses.is_error, "{}", harnesses.text);
    let harnesses = harnesses.json();
    assert_eq!(harnesses[0]["name"], "Mock", "{harnesses}");

    let board = client.call("task_board", json!({}));
    assert!(!board.is_error, "{}", board.text);
    let board = board.json();
    assert!(board["columns"].is_array(), "{board}");

    let overview = client.call("team_overview", json!({ "team": "产品团队" }));
    assert!(!overview.is_error, "{}", overview.text);
    let overview = overview.json();
    assert_eq!(overview["team"]["name"], "产品团队", "{overview}");
    assert_eq!(
        overview["roles"][0]["role"]["name"], "前端开发",
        "{overview}"
    );
}

#[test]
fn creates_a_task_by_role_name_and_sets_dependencies() {
    let env = Env::new();
    let mut client = Client::start(&env.socket);

    // 用岗位名称建任务。
    let first = client.call(
        "create_task",
        json!({ "title": "写登录页", "role": "前端开发", "description": "按设计稿实现" }),
    );
    assert!(!first.is_error, "{}", first.text);
    let first = first.json();
    let first_key = first["task"]["key"].as_str().unwrap().to_owned();
    let first_id = first["task"]["id"].as_str().unwrap().to_owned();
    assert!(first_key.starts_with("FE-"), "{first_key}");

    // 用岗位前缀建任务，并按编号写依赖。
    let second = client.call(
        "create_task",
        json!({ "title": "写登录接口", "role": "FE", "depends_on": [first_key] }),
    );
    assert!(!second.is_error, "{}", second.text);
    let second = second.json();
    assert_eq!(second["task"]["dependsOn"][0], first_id, "{second}");
    // 内核里也是同样的依赖。
    let stored = env.ts.get_task(&first_id).unwrap();
    assert_eq!(stored.task.team_id, env.team_id);
    let second_id = second["task"]["id"].as_str().unwrap().to_owned();
    let stored_second = env.ts.get_task(&second_id).unwrap();
    assert_eq!(stored_second.task.depends_on, vec![first_id.clone()]);

    // 给第三个任务设依赖（用编号指代）。
    let third = client.call(
        "create_task",
        json!({ "title": "前后端联调", "role": "前端开发" }),
    );
    assert!(!third.is_error, "{}", third.text);
    let third_key = third.json()["task"]["key"].as_str().unwrap().to_owned();
    let updated = client.call(
        "set_dependencies",
        json!({ "task": third_key, "depends_on": [first_key] }),
    );
    assert!(!updated.is_error, "{}", updated.text);
    let updated = updated.json();
    assert_eq!(updated["task"]["dependsOn"][0], first_id, "{updated}");

    // 编辑任务：只改标题，其他保持原样。
    let edited = client.call(
        "update_task",
        json!({ "task": first_key, "title": "写登录页（含校验）" }),
    );
    assert!(!edited.is_error, "{}", edited.text);
    let edited = edited.json();
    assert_eq!(edited["task"]["title"], "写登录页（含校验）", "{edited}");
    assert_eq!(edited["task"]["description"], "按设计稿实现", "{edited}");
    assert_eq!(edited["task"]["ownerRoleId"], env.role_id, "{edited}");

    // 岗位名解析不到时给出候选。
    let bad = client.call("create_task", json!({ "title": "x", "role": "后端开发" }));
    assert!(bad.is_error, "{}", bad.text);
    assert!(bad.text.contains("前端开发"), "{}", bad.text);

    // 任务名解析不到时给出候选。
    let bad = client.call("get_task", json!({ "task": "不存在的任务" }));
    assert!(bad.is_error, "{}", bad.text);
    assert!(bad.text.contains("FE-001"), "{}", bad.text);
}

#[test]
fn starts_a_run_and_reads_progress_and_result() {
    let env = Env::new();
    let mut client = Client::start(&env.socket);
    let created = client.call(
        "create_task",
        json!({
            "title": "实现登录页",
            "role": "前端开发",
            "description": "写一个页面",
            "status": "ready",
        }),
    );
    assert!(!created.is_error, "{}", created.text);
    let key = created.json()["task"]["key"].as_str().unwrap().to_owned();

    let started = client.call("start_run", json!({ "task": key }));
    assert!(!started.is_error, "{}", started.text);
    let run_id = started.json()["id"].as_str().unwrap().to_owned();

    let finished = wait_for_status(&mut client, &run_id, RunStatus::Succeeded);
    assert_eq!(finished["status"], "succeeded", "{finished}");
    assert!(
        finished["summary"].as_str().is_some_and(|s| !s.is_empty()),
        "{finished}"
    );
    assert!(finished["changedFiles"].is_array(), "{finished}");

    // 进度：能读到条目，也能用游标续读。
    let progress = client.call("run_progress", json!({ "run": run_id }));
    assert!(!progress.is_error, "{}", progress.text);
    let progress = progress.json();
    let updates = progress["updates"].as_array().expect("进度条目");
    assert!(!updates.is_empty(), "{progress}");
    assert!(
        updates.iter().any(|update| update["summary"]
            .as_str()
            .is_some_and(|s| s.contains("Agent"))),
        "{progress}"
    );
    let cursor = progress["nextCursor"].as_str().expect("游标").to_owned();
    let again = client.call("run_progress", json!({ "run": run_id, "cursor": cursor }));
    assert!(!again.is_error, "{}", again.text);
    let again = again.json();
    assert_eq!(again["updates"].as_array().unwrap().len(), 0, "{again}");

    // 任务详情里能看到这次执行。
    let task = client.call("get_task", json!({ "task": key }));
    assert!(!task.is_error, "{}", task.text);
    let task = task.json();
    assert_eq!(task["task"]["status"], "review", "{task}");
    assert_eq!(task["latestRun"]["id"], run_id, "{task}");
}

#[test]
fn rejects_a_review_with_a_comment() {
    let env = Env::new();
    let mut client = Client::start(&env.socket);
    let created = client.call(
        "create_task",
        json!({ "title": "实现登录页", "role": "前端开发", "status": "ready" }),
    );
    assert!(!created.is_error, "{}", created.text);
    let key = created.json()["task"]["key"].as_str().unwrap().to_owned();
    let started = client.call("start_run", json!({ "task": key }));
    assert!(!started.is_error, "{}", started.text);
    let run_id = started.json()["id"].as_str().unwrap().to_owned();
    wait_for_status(&mut client, &run_id, RunStatus::Succeeded);

    // 验收视图能看到改动文件与可退回。
    let review = client.call("get_review", json!({ "task": key }));
    assert!(!review.is_error, "{}", review.text);
    let review = review.json();
    assert_eq!(review["task"]["key"], key, "{review}");
    assert!(review["run"].is_object(), "{review}");
    assert_eq!(review["canReject"], true, "{review}");

    // 退回必须带意见。
    let missing = client.call("reject_review", json!({ "task": key, "comment": "   " }));
    assert!(missing.is_error, "{}", missing.text);
    assert!(missing.text.contains("意见"), "{}", missing.text);

    // 带意见退回：内核会续接会话重做，返回的是新的一次执行。
    let rejected = client.call(
        "reject_review",
        json!({ "task": key, "comment": "按钮颜色不对，改成主色" }),
    );
    assert!(!rejected.is_error, "{}", rejected.text);
    let rejected = rejected.json();
    assert_eq!(rejected["revisionOf"], run_id.as_str(), "{rejected}");
    assert_eq!(rejected["attempt"], 2, "{rejected}");

    // 被退回的那次执行上记着意见。
    let first = client.call("run_status", json!({ "run": run_id }));
    assert!(!first.is_error, "{}", first.text);
    let first = first.json();
    assert_eq!(first["review"]["outcome"], "rejected", "{first}");
    assert_eq!(
        first["review"]["comment"], "按钮颜色不对，改成主色",
        "{first}"
    );

    // 退回后活动里有 Leader 的记录。
    let activity = client.call("list_activity", json!({ "task": key }));
    assert!(!activity.is_error, "{}", activity.text);
    let activity = activity.json();
    let events = activity["events"].as_array().expect("活动");
    assert!(
        events
            .iter()
            .any(|event| event["actor"] == "leader" && event["kind"] == "review_rejected"),
        "{activity}"
    );
}

#[test]
fn lists_pending_permissions_without_answering_them() {
    let env = Env::new();
    let mut client = Client::start(&env.socket);
    let created = client.call(
        "create_task",
        json!({
            "title": "跑一下命令",
            "role": "前端开发",
            "description": "[mock:perm]",
            "status": "ready",
        }),
    );
    assert!(!created.is_error, "{}", created.text);
    let key = created.json()["task"]["key"].as_str().unwrap().to_owned();
    let started = client.call("start_run", json!({ "task": key }));
    assert!(!started.is_error, "{}", started.text);
    let run_id = started.json()["id"].as_str().unwrap().to_owned();
    wait_for_status(&mut client, &run_id, RunStatus::WaitingPermission);

    let pending = client.call("pending_permissions", json!({}));
    assert!(!pending.is_error, "{}", pending.text);
    let pending = pending.json();
    assert_eq!(pending["count"], 1, "{pending}");
    let prompt = &pending["permissions"][0];
    assert_eq!(prompt["runId"], run_id.as_str(), "{pending}");
    assert_eq!(prompt["taskKey"], key.as_str(), "{pending}");
    assert!(prompt["reason"].as_str().is_some(), "{pending}");
    assert!(prompt["options"].is_array(), "{pending}");

    // 答复权限不在这套工具里（只能由人在应用里点）。
    let denied = client.call("respond_permission", json!({}));
    assert!(denied.protocol_error.is_some(), "{}", denied.text);
    assert!(
        denied.protocol_error.unwrap().contains("tool not found"),
        "{}",
        denied.text
    );
}

#[test]
fn refuses_operations_the_leader_may_not_do() {
    let env = Env::new();
    let mut client = Client::start(&env.socket);
    let created = client.call(
        "create_task",
        json!({ "title": "写登录页", "role": "前端开发" }),
    );
    assert!(!created.is_error, "{}", created.text);
    let key = created.json()["task"]["key"].as_str().unwrap().to_owned();

    // 置为已完成只能由人在应用里点。
    let done = client.call("change_task_status", json!({ "task": key, "to": "done" }));
    assert!(done.is_error, "{}", done.text);
    assert!(done.text.contains("应用"), "{}", done.text);

    // 状态取值不合法。
    let bad = client.call(
        "change_task_status",
        json!({ "task": key, "to": "running" }),
    );
    assert!(bad.is_error, "{}", bad.text);
    assert!(bad.text.contains("todo"), "{}", bad.text);

    // 合并（通过验收）不提供。
    let approve = client.call("approve_review", json!({ "run": "x" }));
    assert!(approve.protocol_error.is_some(), "{}", approve.text);
    assert!(
        approve.protocol_error.unwrap().contains("tool not found"),
        "{}",
        approve.text
    );
}

#[test]
fn changes_task_status() {
    let env = Env::new();
    let mut client = Client::start(&env.socket);
    let created = client.call(
        "create_task",
        json!({ "title": "写登录页", "role": "前端开发" }),
    );
    assert!(!created.is_error, "{}", created.text);
    let key = created.json()["task"]["key"].as_str().unwrap().to_owned();

    let ready = client.call("change_task_status", json!({ "task": key, "to": "ready" }));
    assert!(!ready.is_error, "{}", ready.text);
    assert_eq!(ready.json()["task"]["status"], "ready", "{}", ready.text);

    let doing = client.call("change_task_status", json!({ "task": key, "to": "doing" }));
    assert!(!doing.is_error, "{}", doing.text);
    assert_eq!(doing.json()["task"]["status"], "doing", "{}", doing.text);

    // 受阻必须给原因。
    let blocked = client.call(
        "change_task_status",
        json!({ "task": key, "to": "blocked" }),
    );
    assert!(blocked.is_error, "{}", blocked.text);

    let blocked = client.call(
        "change_task_status",
        json!({ "task": key, "to": "blocked", "reason": "等接口" }),
    );
    assert!(!blocked.is_error, "{}", blocked.text);
    assert_eq!(
        blocked.json()["task"]["status"],
        "blocked",
        "{}",
        blocked.text
    );
}

#[test]
fn cancels_a_running_execution() {
    let env = Env::new();
    let mut client = Client::start(&env.socket);
    let created = client.call(
        "create_task",
        json!({
            "title": "跑一下命令",
            "role": "前端开发",
            "description": "[mock:perm]",
            "status": "ready",
        }),
    );
    assert!(!created.is_error, "{}", created.text);
    let key = created.json()["task"]["key"].as_str().unwrap().to_owned();
    let started = client.call("start_run", json!({ "task": key }));
    assert!(!started.is_error, "{}", started.text);
    let run_id = started.json()["id"].as_str().unwrap().to_owned();
    wait_for_status(&mut client, &run_id, RunStatus::WaitingPermission);

    let cancelled = client.call("cancel_run", json!({ "run": run_id }));
    assert!(!cancelled.is_error, "{}", cancelled.text);
    let cancelled = cancelled.json();
    assert_eq!(cancelled["status"], "cancelled", "{cancelled}");

    // 任务回到 READY。
    let task = client.call("get_task", json!({ "task": key }));
    assert!(!task.is_error, "{}", task.text);
    assert_eq!(task.json()["task"]["status"], "ready", "{}", task.text);
}

#[test]
fn says_the_app_is_not_running_when_the_socket_is_missing() {
    let dir = tempfile::tempdir().unwrap();
    let mut client = Client::start(&dir.path().join("teamspace-control.sock"));
    let call = client.call("list_teams", json!({}));
    assert!(call.is_error, "{}", call.text);
    assert!(
        call.text
            .contains("Teamspace 应用没有运行，请先打开 Teamspace"),
        "{}",
        call.text
    );
    // 服务本身不崩：换个工具还是能给出同样的提示。
    let call = client.call("task_board", json!({}));
    assert!(call.is_error, "{}", call.text);
    assert!(
        call.text
            .contains("Teamspace 应用没有运行，请先打开 Teamspace"),
        "{}",
        call.text
    );
}

#[test]
fn shows_the_activity_of_a_task_and_filters_by_role() {
    let env = Env::new();
    let mut client = Client::start(&env.socket);
    let created = client.call(
        "create_task",
        json!({ "title": "写登录页", "role": "前端开发" }),
    );
    assert!(!created.is_error, "{}", created.text);
    let key = created.json()["task"]["key"].as_str().unwrap().to_owned();

    let activity = client.call("list_activity", json!({ "task": key }));
    assert!(!activity.is_error, "{}", activity.text);
    let activity = activity.json();
    let events = activity["events"].as_array().expect("活动");
    assert!(!events.is_empty(), "{activity}");
    assert!(
        events
            .iter()
            .all(|event| event["kind"] == "task_created" || event["taskId"].is_string()),
        "{activity}"
    );

    // 按岗位筛选。
    let by_role = client.call("list_activity", json!({ "role": "前端开发" }));
    assert!(!by_role.is_error, "{}", by_role.text);
    let by_role = by_role.json();
    let events = by_role["events"].as_array().expect("活动");
    assert!(
        events
            .iter()
            .all(|event| event["roleId"] == env.role_id.as_str()),
        "{by_role}"
    );
}
