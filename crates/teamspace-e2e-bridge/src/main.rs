//! 端到端测试专用桥接：把浏览器里的 `invoke(cmd, args)` 转发给真实的 `Teamspace` 内核。
//!
//! 用法：`teamspace-e2e-bridge <数据库路径> <端口>`。只监听 127.0.0.1，只提供 `POST /invoke`，
//! 请求体 `{ "cmd": "...", "args": {...} }`，响应 `{ "ok": 值 }` 或 `{ "err": ErrorPayload }`。
//! 以 `--teamspace-mock-agent` 参数启动时作为内置 Mock ACP Agent 运行（Mock 预设的握手与执行会这样启动它）。
//!
//! 执行事件（Tauri 中由 `app.emit` 推送）在这里缓存在内存里，浏览器端用桥接专用命令
//! `__poll_run_events`（参数 `{ after: number }`，返回 `{ events: [{ seq, event }], next }`）轮询，
//! 再交给 mock 的事件系统分发给 `listen` 的订阅者，前端代码无需区分。
//! 通知事件（应用内通知与消息状态变化）另有一份缓存，命令是 `__poll_notify_events`，参数与返回同上。
//! 测试专用命令 `__simulate_restart`：等同应用退出再启动（终止所有 Agent 进程，把进行中的执行标记为
//! 已中断），用来验证「已中断」状态与续接、重新执行、标记为失败。重启会重新打开同一个数据库，
//! 消息投递线程随之重新起来——真实重启也是这样，否则重启之后发的消息永远不会被投递。
//! 不属于正式应用。

use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::Path;
use std::sync::{Arc, Mutex, RwLock};
use std::thread;

use serde_json::{Value, json};
use teamspace_core::acp::AgentLaunch;
use teamspace_core::dispatch::Caller;
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

    fn list_markdown_files(
        &self,
        workspace: &Path,
        limit: usize,
    ) -> teamspace_core::Result<Vec<String>> {
        self.0.list_markdown_files(workspace, limit)
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

    fn open_path(&self, path: &Path) -> teamspace_core::Result<()> {
        println!("open {}", path.display());
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

/// 通知事件（应用内通知与消息状态变化）缓存：与执行事件分开，页面各自订阅。
type NotifyEvents = Arc<Mutex<EventLog>>;

/// 当前的内核实例。模拟重启时换一个新的：真实重启会重新打开数据库并重启消息投递线程，
/// 桥接要还原这一点——只停不启的话，重启之后再发的消息永远停在「排队中」。
/// 重启的短暂窗口里是 `None`：在途请求会看到「内核正在重启」。
type Shared = Arc<RwLock<Option<Arc<Teamspace>>>>;

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
    let db = Path::new(&db).to_path_buf();
    let events: Events = Arc::default();
    let notify_events: NotifyEvents = Arc::default();
    let shared: Shared = Arc::new(RwLock::new(Some(open_core(&db, &events, &notify_events))));
    let listener = TcpListener::bind(format!("127.0.0.1:{port}"))
        .unwrap_or_else(|err| panic!("bind port {port}: {err}"));
    println!("bridge listening on 127.0.0.1:{port}");
    for stream in listener.incoming().flatten() {
        let shared = Arc::clone(&shared);
        let events = Arc::clone(&events);
        let notify_events = Arc::clone(&notify_events);
        let db = db.clone();
        thread::spawn(move || {
            let _ = handle(stream, &shared, &db, &events, &notify_events);
        });
    }
}

/// 打开内核实例并挂上执行事件与通知事件的监听。
///
/// 旧实例的实例锁要等它析构后才释放，所以这里重试几次：模拟重启时旧实例可能还被在途请求引用着。
fn open_core(db: &Path, events: &Events, notify_events: &NotifyEvents) -> Arc<Teamspace> {
    let mut last = String::from("未知错误");
    for _ in 0..40 {
        match Teamspace::open_with(
            db,
            Box::new(BridgeProbe(LocalProbe::new())),
            HostConfig::with_current_exe_mock_agent(),
        ) {
            Ok(ts) => {
                let sink = Arc::clone(events);
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
                let notify_sink = Arc::clone(notify_events);
                ts.set_notify_listener(move |event| {
                    let mut log = notify_sink.lock().unwrap_or_else(|p| p.into_inner());
                    log.next += 1;
                    let seq = log.next;
                    log.events
                        .push_back((seq, serde_json::to_value(event).unwrap_or(Value::Null)));
                    while log.events.len() > EVENT_BUFFER {
                        log.events.pop_front();
                    }
                });
                return Arc::new(ts);
            }
            Err(err) => {
                last = err.to_string();
                std::thread::sleep(std::time::Duration::from_millis(50));
            }
        }
    }
    panic!("open database: {last}");
}

/// 当前的内核实例；正在重启时返回错误（调用方稍后重试即可）。
fn current(shared: &Shared) -> Result<Arc<Teamspace>, ErrorPayload> {
    let guard = shared.read().unwrap_or_else(|p| p.into_inner());
    guard
        .as_ref()
        .map(Arc::clone)
        .ok_or_else(|| Error::Conflict("内核正在重启，请稍后重试".to_owned()).payload())
}

fn handle(
    mut stream: TcpStream,
    shared: &Shared,
    db: &Path,
    events: &Events,
    notify_events: &NotifyEvents,
) -> std::io::Result<()> {
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
            } else if cmd == "__poll_notify_events" {
                poll_events(notify_events, &args)
            } else {
                dispatch(shared, db, events, notify_events, cmd, &args)
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

/// 分发命令：内核命令交给 [`teamspace_core::dispatch`]（桥接的身份是 human），
/// 只有桥接专用的两条命令留在这里。
fn dispatch(
    shared: &Shared,
    db: &Path,
    events: &Events,
    notify_events: &NotifyEvents,
    cmd: &str,
    args: &Value,
) -> CmdResult {
    match cmd {
        // 模拟用户在系统对话框里选了 E2E_WORKSPACE 指定的目录。
        "pick_workspace_folder" => {
            let ts = current(shared)?;
            match std::env::var("E2E_WORKSPACE") {
                Ok(path) => out(ts.inspect_workspace(&path)),
                Err(_) => Ok(Value::Null),
            }
        }
        // 测试专用命令：等同应用退出再启动——先停掉当前实例（终止所有 Agent 进程、
        // 把进行中的执行标记为已中断），再用同一个数据库打开一个新实例。
        // 真实重启会重新启动消息投递线程，桥接要还原这一点：只停不启的话，
        // 重启之后再发的消息永远停在「排队中」。
        "__simulate_restart" => {
            let old = {
                let mut guard = shared.write().unwrap_or_else(|p| p.into_inner());
                guard.take()
            };
            if let Some(old) = old {
                old.shutdown();
                drop(old);
            }
            let fresh = open_core(db, events, notify_events);
            {
                let mut guard = shared.write().unwrap_or_else(|p| p.into_inner());
                *guard = Some(fresh);
            }
            Ok(Value::Null)
        }
        other => {
            let ts = current(shared)?;
            teamspace_core::dispatch::dispatch(&ts, Caller::Human, other, args)
        }
    }
}
