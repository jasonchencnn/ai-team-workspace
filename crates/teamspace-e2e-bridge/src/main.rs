//! 端到端测试专用桥接：把浏览器里的 `invoke(cmd, args)` 转发给真实的 `Teamspace` 内核。
//!
//! 用法：`teamspace-e2e-bridge <数据库路径> <端口>`。只监听 127.0.0.1，只提供 `POST /invoke`，
//! 请求体 `{ "cmd": "...", "args": {...} }`，响应 `{ "ok": 值 }` 或 `{ "err": ErrorPayload }`。
//! 以 `--teamspace-mock-agent` 参数启动时作为内置 Mock ACP Agent 运行（Mock 预设的握手与执行会这样启动它）。
//!
//! 执行事件（Tauri 中由 `app.emit` 推送）在这里缓存在内存里，浏览器端用桥接专用命令
//! `__poll_run_events`（参数 `{ after: number }`，返回 `{ events: [{ seq, event }], next }`）轮询，
//! 再交给 mock 的事件系统分发给 `listen` 的订阅者，前端代码无需区分。
//! 测试专用命令 `__simulate_restart`：等同应用退出时的收尾（终止所有 Agent 进程，把进行中的执行标记为
//! 已中断），用来验证「已中断」状态与续接、重新执行、标记为失败。
//! 不属于正式应用。

use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::thread;

use serde::de::DeserializeOwned;
use serde_json::{Value, json};
use teamspace_core::acp::AgentLaunch;
use teamspace_core::model::{Handshake, WorkspaceInfo};
use teamspace_core::system_probe::LocalProbe;
use teamspace_core::{Error, ErrorPayload, HostConfig, SystemProbe, Teamspace, mock_agent};

/// 真实探测，但「在 Finder 中打开」只打印路径，不在测试时弹出 Finder 窗口。
struct BridgeProbe(LocalProbe);

impl SystemProbe for BridgeProbe {
    fn inspect_workspace(&self, path: &Path) -> teamspace_core::Result<WorkspaceInfo> {
        self.0.inspect_workspace(path)
    }

    fn search_files(
        &self,
        workspace: &Path,
        query: &str,
        limit: usize,
    ) -> teamspace_core::Result<Vec<String>> {
        self.0.search_files(workspace, query, limit)
    }

    fn locate_command(&self, command: &str) -> Option<String> {
        self.0.locate_command(command)
    }

    fn handshake(&self, launch: &AgentLaunch) -> Handshake {
        self.0.handshake(launch)
    }

    fn reveal_path(&self, path: &Path) -> teamspace_core::Result<()> {
        println!("reveal {}", path.display());
        Ok(())
    }

    fn command_path_env(&self) -> std::ffi::OsString {
        self.0.command_path_env()
    }
}

/// 缓存的执行事件上限。
const EVENT_BUFFER: usize = 5000;

/// 执行事件缓存：（序号，事件）。
#[derive(Default)]
struct EventLog {
    next: u64,
    events: std::collections::VecDeque<(u64, Value)>,
}

type Events = Arc<Mutex<EventLog>>;

type CmdResult = Result<Value, ErrorPayload>;

fn main() {
    if std::env::args().nth(1).as_deref() == Some(mock_agent::MOCK_AGENT_FLAG) {
        if let Err(err) = mock_agent::run_stdio() {
            eprintln!("mock agent: {err}");
            std::process::exit(1);
        }
        return;
    }
    let mut args = std::env::args().skip(1);
    let (Some(db), Some(port)) = (args.next(), args.next()) else {
        eprintln!("usage: teamspace-e2e-bridge <db-path> <port>");
        std::process::exit(2);
    };
    let ts = Teamspace::open_with(
        Path::new(&db),
        Box::new(BridgeProbe(LocalProbe::new())),
        HostConfig::with_current_exe_mock_agent(),
    )
    .unwrap_or_else(|err| panic!("open database: {err}"));
    let events: Events = Arc::default();
    let sink = Arc::clone(&events);
    ts.set_run_listener(move |event| {
        let mut log = sink.lock().unwrap_or_else(|p| p.into_inner());
        log.next += 1;
        let seq = log.next;
        log.events
            .push_back((seq, serde_json::to_value(event).unwrap_or(Value::Null)));
        while log.events.len() > EVENT_BUFFER {
            log.events.pop_front();
        }
    });
    let ts = Arc::new(ts);
    let listener = TcpListener::bind(format!("127.0.0.1:{port}"))
        .unwrap_or_else(|err| panic!("bind port {port}: {err}"));
    println!("bridge listening on 127.0.0.1:{port}");
    for stream in listener.incoming().flatten() {
        let ts = Arc::clone(&ts);
        let events = Arc::clone(&events);
        thread::spawn(move || {
            let _ = handle(stream, &ts, &events);
        });
    }
}

fn handle(mut stream: TcpStream, ts: &Teamspace, events: &Events) -> std::io::Result<()> {
    let mut reader = BufReader::new(stream.try_clone()?);
    let mut request_line = String::new();
    reader.read_line(&mut request_line)?;
    let mut content_length = 0usize;
    loop {
        let mut line = String::new();
        reader.read_line(&mut line)?;
        let line = line.trim();
        if line.is_empty() {
            break;
        }
        if let Some((name, value)) = line.split_once(':')
            && name.eq_ignore_ascii_case("content-length")
        {
            content_length = value.trim().parse().unwrap_or(0);
        }
    }
    let mut body = vec![0u8; content_length];
    reader.read_exact(&mut body)?;

    if request_line.starts_with("OPTIONS") {
        return respond(&mut stream, "204 No Content", "");
    }
    if !request_line.starts_with("POST /invoke") {
        return respond(&mut stream, "404 Not Found", "{}");
    }
    let reply = match serde_json::from_slice::<Value>(&body) {
        Ok(request) => {
            let cmd = request["cmd"].as_str().unwrap_or_default();
            let args = request.get("args").cloned().unwrap_or(Value::Null);
            let result = if cmd == "__poll_run_events" {
                poll_events(events, &args)
            } else {
                dispatch(ts, cmd, &args)
            };
            match result {
                Ok(value) => json!({ "ok": value }),
                Err(err) => json!({ "err": err }),
            }
        }
        Err(err) => {
            json!({ "err": { "kind": "invalid", "message": format!("bad request: {err}") } })
        }
    };
    respond(&mut stream, "200 OK", &reply.to_string())
}

fn respond(stream: &mut TcpStream, status: &str, body: &str) -> std::io::Result<()> {
    write!(
        stream,
        "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\
         Access-Control-Allow-Origin: *\r\nAccess-Control-Allow-Headers: *\r\n\
         Access-Control-Allow-Methods: POST, OPTIONS\r\nConnection: close\r\n\r\n{body}",
        body.len()
    )
}

fn bad(message: String) -> ErrorPayload {
    Error::Invalid(message).payload()
}

fn arg<T: DeserializeOwned>(args: &Value, name: &str) -> Result<T, ErrorPayload> {
    let value = args.get(name).cloned().unwrap_or(Value::Null);
    serde_json::from_value(value).map_err(|err| bad(format!("参数 {name} 无效：{err}")))
}

fn out<T: serde::Serialize>(result: teamspace_core::Result<T>) -> CmdResult {
    let value = result.map_err(|err| err.payload())?;
    serde_json::to_value(value).map_err(|err| bad(err.to_string()))
}

/// 返回序号大于 `after` 的执行事件。
fn poll_events(events: &Events, args: &Value) -> CmdResult {
    let after = args.get("after").and_then(Value::as_u64).unwrap_or(0);
    let log = events.lock().unwrap_or_else(|p| p.into_inner());
    let list: Vec<Value> = log
        .events
        .iter()
        .filter(|(seq, _)| *seq > after)
        .map(|(seq, event)| json!({ "seq": seq, "event": event }))
        .collect();
    Ok(json!({ "events": list, "next": log.next.max(after) }))
}

fn dispatch(ts: &Teamspace, cmd: &str, args: &Value) -> CmdResult {
    match cmd {
        "list_teams" => out(ts.list_teams()),
        "create_team" => out(ts.create_team(arg(args, "input")?)),
        "get_team_overview" => out(ts.get_team_overview(&arg::<String>(args, "teamId")?)),
        "inspect_workspace" => out(ts.inspect_workspace(&arg::<String>(args, "path")?)),
        // 模拟用户在系统对话框里选了 E2E_WORKSPACE 指定的目录。
        "pick_workspace_folder" => match std::env::var("E2E_WORKSPACE") {
            Ok(path) => out(ts.inspect_workspace(&path)),
            Err(_) => Ok(Value::Null),
        },
        "list_role_templates" => out(Ok(ts.list_role_templates())),
        "suggest_task_prefix" => out(ts.suggest_task_prefix(arg(args, "input")?)),
        "list_roles" => out(ts.list_roles(&arg::<String>(args, "teamId")?)),
        "create_role" => out(ts.create_role(arg(args, "input")?)),
        "update_role" => out(ts.update_role(arg(args, "input")?)),
        "archive_role" => out(ts.archive_role(&arg::<String>(args, "roleId")?)),
        "bind_role_harness" => out(ts.bind_role_harness(
            &arg::<String>(args, "roleId")?,
            &arg::<String>(args, "harnessId")?,
        )),
        "unbind_role_harness" => out(ts.unbind_role_harness(&arg::<String>(args, "roleId")?)),
        "get_role_folder" => out(ts.get_role_folder(
            &arg::<String>(args, "roleId")?,
            arg::<Option<u32>>(args, "limit")?,
        )),
        "reveal_role_folder" => out(ts.reveal_role_folder(&arg::<String>(args, "roleId")?)),
        "get_role_workbench" => out(ts.get_role_workbench(&arg::<String>(args, "roleId")?)),
        "open_role_path" => out(ts.open_role_path(
            &arg::<String>(args, "roleId")?,
            arg::<Option<String>>(args, "relativePath")?.as_deref(),
        )),
        "list_acp_presets" => out(Ok(ts.list_acp_presets())),
        "list_harnesses" => out(ts.list_harnesses(&arg::<String>(args, "teamId")?)),
        "get_harness" => out(ts.get_harness(&arg::<String>(args, "harnessId")?)),
        "create_harness" => out(ts.create_harness(arg(args, "input")?)),
        "update_harness" => out(ts.update_harness(arg(args, "input")?)),
        "archive_harness" => out(ts.archive_harness(&arg::<String>(args, "harnessId")?)),
        "check_harness" => out(ts.check_harness(&arg::<String>(args, "harnessId")?)),
        "list_task_statuses" => out(Ok(ts.list_task_statuses())),
        "get_task_board" => out(ts.get_task_board(arg(args, "input")?)),
        "get_task" => out(ts.get_task(&arg::<String>(args, "taskId")?)),
        "create_task" => out(ts.create_task(arg(args, "input")?)),
        "update_task" => out(ts.update_task(arg(args, "input")?)),
        "change_task_status" => out(ts.change_task_status(arg(args, "input")?)),
        "search_workspace_files" => out(ts.search_workspace_files(arg(args, "input")?)),
        "start_run" => out(ts.start_run(&arg::<String>(args, "taskId")?)),
        "cancel_run" => out(ts.cancel_run(&arg::<String>(args, "runId")?)),
        "resume_run" => out(ts.resume_run(&arg::<String>(args, "runId")?)),
        "mark_run_failed" => out(ts.mark_run_failed(&arg::<String>(args, "runId")?)),
        "takeover_run" => out(ts.takeover_run(&arg::<String>(args, "runId")?)),
        "open_run_dir" => out(ts.open_run_dir(&arg::<String>(args, "runId")?)),
        "get_run" => out(ts.get_run(&arg::<String>(args, "runId")?)),
        "list_runs" => out(ts.list_runs(&arg::<String>(args, "taskId")?)),
        "list_run_updates" => out(ts.list_run_updates(arg(args, "input")?)),
        "respond_permission" => out(ts.respond_permission(arg(args, "input")?)),
        "get_review" => out(ts.get_review(&arg::<String>(args, "taskId")?)),
        "get_review_file_patch" => out(ts.get_review_file_patch(
            &arg::<String>(args, "runId")?,
            &arg::<String>(args, "path")?,
        )),
        "approve_review" => out(ts.approve_review(arg(args, "input")?)),
        "reject_review" => out(ts.reject_review(arg(args, "input")?)),
        "resolve_merge_manually" => out(ts.resolve_merge_manually(arg(args, "input")?)),
        "rerun_after_conflict" => out(ts.rerun_after_conflict(arg(args, "input")?)),
        "__simulate_restart" => {
            ts.shutdown();
            Ok(Value::Null)
        }
        "list_events" => out(ts.list_events(arg(args, "input")?)),
        other => Err(bad(format!("未知命令：{other}"))),
    }
}
