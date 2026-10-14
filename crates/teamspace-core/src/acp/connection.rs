//! 长连接 ACP 客户端（M2）：一个 Agent 子进程上的 JSON-RPC 2.0 会话。
//!
//! - 读线程逐行解析 Agent 的 stdout：带 `id` 的响应按 id 交给发起请求时登记的回调（顺序无关，
//!   乱序响应也能正确匹配）；通知（`session/update`）和 Agent 发来的请求（`session/request_permission`、
//!   `fs/*`、`terminal/*`）交给 `on_incoming`，由执行管理器路由处理；
//! - 写入经互斥锁串行化，一条消息一行，不会互相穿插；
//! - stdout 关闭（进程退出或管道断开）时，所有等待中的请求以 [`RpcFailure::Closed`] 结束，
//!   并发出 [`Incoming::Closed`]；
//! - [`AgentProcess`] 负责启动子进程（独立进程组）、收集 stderr 尾部、整组终止。
//!
//! 协议逻辑（[`Connection`]）只依赖任意读写端，测试中用管道直接驱动；进程部分用真实子进程测试。

use std::collections::HashMap;
use std::ffi::OsString;
use std::io::{BufRead, BufReader, Read, Write};
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc;
use std::sync::{Arc, Mutex, MutexGuard};
use std::thread;
use std::time::{Duration, Instant};

use serde_json::{Value, json};

use crate::system_probe::{
    isolate_in_process_group, kill_process_group, scrub_inherited_claude_env,
};

/// 保留的 stderr 尾部字节数（只用于失败说明）。
const STDERR_TAIL: usize = 16 * 1024;
/// 阻塞请求检查中止条件的间隔。
const ABORT_POLL: Duration = Duration::from_millis(50);

/// Agent 主动发来的消息。
#[derive(Debug, Clone, PartialEq)]
pub enum Incoming {
    /// 通知（无 id），如 `session/update`。
    Notification { method: String, params: Value },
    /// Agent 发给 Client 的请求，须用 [`Connection::respond`] 或 [`Connection::respond_error`] 回复。
    Request {
        id: Value,
        method: String,
        params: Value,
    },
    /// 无法解析为 JSON-RPC 消息的一行（截断后的原文）。
    Malformed(String),
    /// stdout 已关闭：Agent 进程退出或管道断开。之后不会再有消息。
    Closed,
}

/// 请求失败的原因。
#[derive(Debug, Clone, PartialEq)]
pub enum RpcFailure {
    /// Agent 返回了 JSON-RPC 错误。
    Error {
        code: i64,
        message: String,
        data: Option<Value>,
    },
    /// 期限内没有收到响应。
    Timeout,
    /// 连接已关闭（Agent 退出）。
    Closed,
    /// 无法写入请求。
    Write(String),
    /// 调用方中止了等待（如执行被取消）。
    Aborted,
}

impl RpcFailure {
    /// 中文说明。
    pub fn describe(&self) -> String {
        match self {
            Self::Error { code, message, .. } => {
                format!("Agent 返回错误（错误码 {code}）：{message}")
            }
            Self::Timeout => "Agent 没有在期限内响应".to_owned(),
            Self::Closed => "Agent 进程已退出或关闭了输出".to_owned(),
            Self::Write(err) => format!("无法向 Agent 发送消息：{err}"),
            Self::Aborted => "已中止".to_owned(),
        }
    }
}

/// 请求结果的回调。
pub type Callback = Box<dyn FnOnce(Result<Value, RpcFailure>) + Send>;

#[derive(Default)]
struct Pending {
    waiting: HashMap<u64, Callback>,
    closed: bool,
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// 一条 JSON-RPC 连接。
pub struct Connection {
    writer: Mutex<Box<dyn Write + Send>>,
    pending: Arc<Mutex<Pending>>,
    next_id: AtomicU64,
}

impl Connection {
    /// 在给定的读写端上建立连接并启动读线程。`on_incoming` 在读线程中按收到的顺序调用。
    pub fn start(
        reader: impl Read + Send + 'static,
        writer: impl Write + Send + 'static,
        on_incoming: impl Fn(Incoming) + Send + 'static,
    ) -> Self {
        let pending = Arc::new(Mutex::new(Pending::default()));
        let shared = Arc::clone(&pending);
        thread::spawn(move || read_loop(reader, &shared, &on_incoming));
        Self {
            writer: Mutex::new(Box::new(writer)),
            pending,
            next_id: AtomicU64::new(1),
        }
    }

    /// 连接是否已关闭（stdout 已 EOF）。
    pub fn is_closed(&self) -> bool {
        lock(&self.pending).closed
    }

    fn send(&self, message: &Value) -> std::io::Result<()> {
        let mut line = message.to_string();
        line.push('\n');
        let mut writer = lock(&self.writer);
        writer.write_all(line.as_bytes())?;
        writer.flush()
    }

    /// 发出请求，结果交给 `callback`（在读线程或当前线程中调用，各恰好一次）。返回请求 id。
    pub fn request_with(&self, method: &str, params: Value, callback: Callback) -> u64 {
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        {
            let mut pending = lock(&self.pending);
            if pending.closed {
                drop(pending);
                callback(Err(RpcFailure::Closed));
                return id;
            }
            pending.waiting.insert(id, callback);
        }
        let message = json!({ "jsonrpc": "2.0", "id": id, "method": method, "params": params });
        if let Err(err) = self.send(&message) {
            let callback = lock(&self.pending).waiting.remove(&id);
            if let Some(callback) = callback {
                callback(Err(RpcFailure::Write(err.to_string())));
            }
        }
        id
    }

    /// 发出请求并等待响应，最多 `timeout`；`abort()` 返回 `true` 时提前放弃（[`RpcFailure::Aborted`]）。
    pub fn request(
        &self,
        method: &str,
        params: Value,
        timeout: Duration,
        abort: &dyn Fn() -> bool,
    ) -> Result<Value, RpcFailure> {
        let (tx, rx) = mpsc::channel();
        let id = self.request_with(
            method,
            params,
            Box::new(move |result| {
                let _ = tx.send(result);
            }),
        );
        let deadline = Instant::now() + timeout;
        loop {
            let remaining = deadline.saturating_duration_since(Instant::now());
            match rx.recv_timeout(remaining.min(ABORT_POLL)) {
                Ok(result) => return result,
                Err(mpsc::RecvTimeoutError::Disconnected) => return Err(RpcFailure::Closed),
                Err(mpsc::RecvTimeoutError::Timeout) => {
                    let failure = if abort() {
                        RpcFailure::Aborted
                    } else if remaining.is_zero() {
                        RpcFailure::Timeout
                    } else {
                        continue;
                    };
                    lock(&self.pending).waiting.remove(&id);
                    return Err(failure);
                }
            }
        }
    }

    /// 发出通知（如 `session/cancel`）。
    pub fn notify(&self, method: &str, params: Value) -> std::io::Result<()> {
        self.send(&json!({ "jsonrpc": "2.0", "method": method, "params": params }))
    }

    /// 回复 Agent 的请求。
    pub fn respond(&self, id: &Value, result: Value) -> std::io::Result<()> {
        self.send(&json!({ "jsonrpc": "2.0", "id": id, "result": result }))
    }

    /// 以错误回复 Agent 的请求。
    pub fn respond_error(&self, id: &Value, code: i64, message: &str) -> std::io::Result<()> {
        self.send(&json!({
            "jsonrpc": "2.0", "id": id, "error": { "code": code, "message": message }
        }))
    }
}

fn read_loop(reader: impl Read, pending: &Mutex<Pending>, on_incoming: &dyn Fn(Incoming)) {
    let mut reader = BufReader::new(reader);
    loop {
        let mut buf = Vec::new();
        match reader.read_until(b'\n', &mut buf) {
            Ok(0) | Err(_) => break,
            Ok(_) => {
                let line = String::from_utf8_lossy(&buf);
                let line = line.trim();
                if !line.is_empty() {
                    dispatch(line, pending, on_incoming);
                }
            }
        }
    }
    let callbacks: Vec<Callback> = {
        let mut pending = lock(pending);
        pending.closed = true;
        pending.waiting.drain().map(|(_, cb)| cb).collect()
    };
    for callback in callbacks {
        callback(Err(RpcFailure::Closed));
    }
    on_incoming(Incoming::Closed);
}

fn dispatch(line: &str, pending: &Mutex<Pending>, on_incoming: &dyn Fn(Incoming)) {
    let Ok(message) = serde_json::from_str::<Value>(line) else {
        on_incoming(Incoming::Malformed(super::quote(line)));
        return;
    };
    if !message.is_object() {
        on_incoming(Incoming::Malformed(super::quote(line)));
        return;
    }
    let id = message.get("id").filter(|id| !id.is_null()).cloned();
    let params = message.get("params").cloned().unwrap_or(Value::Null);
    if let Some(method) = message.get("method").and_then(Value::as_str) {
        let method = method.to_owned();
        on_incoming(match id {
            Some(id) => Incoming::Request { id, method, params },
            None => Incoming::Notification { method, params },
        });
        return;
    }
    // 响应：只认本连接发出的数字 id，其他（包括无法匹配的）忽略。
    let Some(id) = id.as_ref().and_then(Value::as_u64) else {
        return;
    };
    let Some(callback) = lock(pending).waiting.remove(&id) else {
        return;
    };
    let result = match message.get("error") {
        Some(error) => Err(RpcFailure::Error {
            code: error.get("code").and_then(Value::as_i64).unwrap_or(0),
            message: error
                .get("message")
                .and_then(Value::as_str)
                .unwrap_or("（无说明）")
                .to_owned(),
            data: error.get("data").cloned(),
        }),
        None => Ok(message.get("result").cloned().unwrap_or(Value::Null)),
    };
    callback(result);
}

/// 启动 Agent 子进程所需的全部信息（已解析为可执行文件路径与完整环境变量补充）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SpawnSpec {
    pub program: PathBuf,
    pub args: Vec<String>,
    pub cwd: PathBuf,
    /// 在继承的环境之上设置的变量（PATH、环境配置档案中的变量）。
    pub env: Vec<(OsString, OsString)>,
}

/// 一个运行中的 Agent 子进程及其连接。drop 时整组终止。
pub struct AgentProcess {
    child: Mutex<Child>,
    pub conn: Connection,
    stderr: Arc<Mutex<Vec<u8>>>,
}

impl AgentProcess {
    /// 启动子进程（独立进程组）并建立连接。
    pub fn spawn(
        spec: &SpawnSpec,
        on_incoming: impl Fn(Incoming) + Send + 'static,
    ) -> std::io::Result<Self> {
        let mut command = Command::new(&spec.program);
        command
            .args(&spec.args)
            .current_dir(&spec.cwd)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        // 继承的 Claude Code 会话变量会让 claude-code-acp 拒绝启动；配置档案里显式设置的除外。
        scrub_inherited_claude_env(&mut command, &spec.env);
        for (key, value) in &spec.env {
            command.env(key, value);
        }
        isolate_in_process_group(&mut command);
        let mut child = command.spawn()?;
        let (Some(stdin), Some(stdout), Some(stderr_pipe)) =
            (child.stdin.take(), child.stdout.take(), child.stderr.take())
        else {
            kill_process_group(&mut child);
            let _ = child.wait();
            return Err(std::io::Error::other("无法连接进程的标准输入输出"));
        };
        let stderr = Arc::new(Mutex::new(Vec::new()));
        let tail = Arc::clone(&stderr);
        thread::spawn(move || keep_tail(stderr_pipe, &tail));
        let conn = Connection::start(stdout, stdin, on_incoming);
        Ok(Self {
            child: Mutex::new(child),
            conn,
            stderr,
        })
    }

    /// 进程 id（即进程组 id）。
    pub fn pid(&self) -> u32 {
        lock(&self.child).id()
    }

    /// 等进程自己退出（最多 `wait`），返回退出情况的中文描述；仍在运行返回 `None`。
    pub fn exit_status(&self, wait: Duration) -> Option<String> {
        let deadline = Instant::now() + wait;
        loop {
            match lock(&self.child).try_wait() {
                Ok(Some(status)) => {
                    return Some(match status.code() {
                        Some(code) => format!("退出码 {code}"),
                        None => "被信号终止".to_owned(),
                    });
                }
                Ok(None) if Instant::now() < deadline => {}
                _ => return None,
            }
            thread::sleep(Duration::from_millis(20));
        }
    }

    /// stderr 最后一行非空内容（截断）。
    pub fn stderr_tail(&self) -> Option<String> {
        let text = String::from_utf8_lossy(&lock(&self.stderr)).into_owned();
        super::last_meaningful_line(&text)
    }

    /// 整组终止并回收进程（可重复调用）。
    pub fn kill(&self) {
        let mut child = lock(&self.child);
        if matches!(child.try_wait(), Ok(Some(_))) {
            // 组长已退出，但组内可能还有孙进程：仍按组发一次信号。
            kill_process_group(&mut child);
            return;
        }
        kill_process_group(&mut child);
        let _ = child.wait();
    }
}

impl Drop for AgentProcess {
    fn drop(&mut self) {
        self.kill();
    }
}

fn keep_tail(mut stream: impl Read, tail: &Mutex<Vec<u8>>) {
    let mut chunk = [0u8; 4096];
    while let Ok(n) = stream.read(&mut chunk) {
        if n == 0 {
            break;
        }
        let mut kept = lock(tail);
        kept.extend_from_slice(&chunk[..n]);
        if kept.len() > STDERR_TAIL {
            let excess = kept.len() - STDERR_TAIL;
            kept.drain(..excess);
        }
    }
}

#[cfg(test)]
mod tests {
    use std::io::{BufRead, BufReader};
    use std::sync::mpsc::Receiver;

    use super::*;

    /// 一端是 [`Connection`]，另一端由测试扮演 Agent。
    struct Harness {
        conn: Connection,
        incoming: Receiver<Incoming>,
        agent_out: std::io::PipeWriter,
        agent_in: BufReader<std::io::PipeReader>,
    }

    fn harness() -> Harness {
        let (client_read, agent_out) = std::io::pipe().unwrap();
        let (agent_read, client_write) = std::io::pipe().unwrap();
        let (tx, incoming) = mpsc::channel();
        let conn = Connection::start(client_read, client_write, move |msg| {
            let _ = tx.send(msg);
        });
        Harness {
            conn,
            incoming,
            agent_out,
            agent_in: BufReader::new(agent_read),
        }
    }

    impl Harness {
        fn agent_says(&mut self, line: &str) {
            self.agent_out.write_all(line.as_bytes()).unwrap();
            self.agent_out.write_all(b"\n").unwrap();
        }

        fn agent_reads(&mut self) -> Value {
            let mut line = String::new();
            self.agent_in.read_line(&mut line).unwrap();
            serde_json::from_str(&line).unwrap()
        }

        fn next(&self) -> Incoming {
            self.incoming.recv_timeout(Duration::from_secs(5)).unwrap()
        }
    }

    fn never() -> bool {
        false
    }

    #[test]
    fn matches_out_of_order_responses_and_interleaved_notifications() {
        let mut h = harness();
        let (tx, rx) = mpsc::channel();
        for method in ["a", "b", "c"] {
            let tx = tx.clone();
            h.conn.request_with(
                method,
                json!({}),
                Box::new(move |r| tx.send((method, r)).unwrap()),
            );
        }
        let ids: Vec<u64> = (0..3)
            .map(|_| h.agent_reads()["id"].as_u64().unwrap())
            .collect();
        h.agent_says(&format!(
            r#"{{"jsonrpc":"2.0","id":{},"result":"C"}}"#,
            ids[2]
        ));
        h.agent_says(r#"{"jsonrpc":"2.0","method":"session/update","params":{"n":1}}"#);
        h.agent_says(&format!(
            r#"{{"jsonrpc":"2.0","id":{},"error":{{"code":-32000,"message":"auth"}}}}"#,
            ids[0]
        ));
        h.agent_says(
            r#"{"jsonrpc":"2.0","id":"x","method":"fs/read_text_file","params":{"path":"/a"}}"#,
        );
        h.agent_says(&format!(
            r#"{{"jsonrpc":"2.0","id":{},"result":{{"ok":1}}}}"#,
            ids[1]
        ));
        h.agent_says(r#"{"jsonrpc":"2.0","id":999,"result":"unmatched"}"#);
        h.agent_says("not json");

        let mut got: Vec<_> = (0..3)
            .map(|_| rx.recv_timeout(Duration::from_secs(5)).unwrap())
            .collect();
        got.sort_by_key(|(m, _)| *m);
        assert_eq!(
            got[0].1,
            Err(RpcFailure::Error {
                code: -32000,
                message: "auth".into(),
                data: None
            })
        );
        assert_eq!(got[1].1, Ok(json!({ "ok": 1 })));
        assert_eq!(got[2].1, Ok(json!("C")));

        assert_eq!(
            h.next(),
            Incoming::Notification {
                method: "session/update".into(),
                params: json!({ "n": 1 })
            }
        );
        let Incoming::Request { id, method, .. } = h.next() else {
            panic!("expected request")
        };
        assert_eq!(
            (id.clone(), method.as_str()),
            (json!("x"), "fs/read_text_file")
        );
        assert_eq!(h.next(), Incoming::Malformed("not json".into()));

        h.conn.respond(&id, json!({ "content": "hi" })).unwrap();
        let reply = h.agent_reads();
        assert_eq!(reply["id"], "x");
        assert_eq!(reply["result"]["content"], "hi");
        h.conn.respond_error(&json!(5), -32601, "no").unwrap();
        assert_eq!(h.agent_reads()["error"]["code"], -32601);
        h.conn
            .notify("session/cancel", json!({ "sessionId": "s" }))
            .unwrap();
        let note = h.agent_reads();
        assert_eq!(note["method"], "session/cancel");
        assert!(note.get("id").is_none());
    }

    #[test]
    fn blocking_request_times_out_aborts_and_closes() {
        let mut h = harness();
        let err = h
            .conn
            .request("slow", json!({}), Duration::from_millis(100), &never)
            .unwrap_err();
        assert_eq!(err, RpcFailure::Timeout);
        // 超时后才到的响应被忽略，不影响后续请求。
        let first = h.agent_reads();
        h.agent_says(&format!(
            r#"{{"jsonrpc":"2.0","id":{},"result":1}}"#,
            first["id"]
        ));

        let err = h
            .conn
            .request("aborted", json!({}), Duration::from_secs(10), &|| true)
            .unwrap_err();
        assert_eq!(err, RpcFailure::Aborted);
        h.agent_reads();

        let Harness {
            conn,
            incoming,
            agent_out,
            mut agent_in,
        } = h;
        let waiter = thread::scope(|s| {
            let handle =
                s.spawn(|| conn.request("pending", json!({}), Duration::from_secs(10), &never));
            let mut line = String::new();
            agent_in.read_line(&mut line).unwrap();
            drop(agent_out);
            handle.join().unwrap()
        });
        assert_eq!(waiter, Err(RpcFailure::Closed));
        assert_eq!(
            incoming.recv_timeout(Duration::from_secs(5)).unwrap(),
            Incoming::Closed
        );
        assert!(conn.is_closed());
        assert_eq!(
            conn.request("after", json!({}), Duration::from_secs(1), &never),
            Err(RpcFailure::Closed)
        );
    }

    #[test]
    fn process_reports_exit_and_stderr_and_kills_group() {
        let dir = tempfile::tempdir().unwrap();
        let spec = SpawnSpec {
            program: "/bin/sh".into(),
            args: vec![
                "-c".into(),
                "echo 'boom: not logged in' >&2; read line; echo \"$line\"; exit 3".into(),
            ],
            cwd: dir.path().into(),
            env: vec![],
        };
        let (tx, rx) = mpsc::channel();
        let process = AgentProcess::spawn(&spec, move |m| {
            let _ = tx.send(m);
        })
        .unwrap();
        process.conn.notify("ping", json!({})).unwrap();
        let echoed = rx.recv_timeout(Duration::from_secs(5)).unwrap();
        assert!(matches!(echoed, Incoming::Notification { ref method, .. } if method == "ping"));
        assert_eq!(
            rx.recv_timeout(Duration::from_secs(5)).unwrap(),
            Incoming::Closed
        );
        assert_eq!(
            process.exit_status(Duration::from_secs(5)).as_deref(),
            Some("退出码 3")
        );
        assert_eq!(
            process.stderr_tail().as_deref(),
            Some("boom: not logged in")
        );

        // 孙进程攥着 stdout：整组终止后读线程能收到 EOF。
        let spec = SpawnSpec {
            args: vec!["-c".into(), "/bin/sleep 30 & exec /bin/sleep 30".into()],
            ..spec
        };
        let (tx, rx) = mpsc::channel();
        let process = AgentProcess::spawn(&spec, move |m| {
            let _ = tx.send(m);
        })
        .unwrap();
        let started = Instant::now();
        process.kill();
        assert_eq!(
            rx.recv_timeout(Duration::from_secs(5)).unwrap(),
            Incoming::Closed
        );
        assert!(started.elapsed() < Duration::from_secs(5));
    }
}
