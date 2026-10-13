//! ACP（Agent Client Protocol）客户端（D13）：Teamspace 是 Client，Harness 是 Agent。
//!
//! 传输：Agent 作为子进程启动，stdin / stdout 上是按行分隔的 JSON-RPC 2.0 消息（一行一条，
//! stdout 只能输出协议消息），stderr 是 Agent 自己的日志。
//!
//! 目前只实现握手检查：启动 Agent → 发送 `initialize`（协议版本 1，声明 Client 不提供文件读写和
//! 终端能力）→ 解析响应 → 整组终止进程。握手**不发送任何 prompt、不创建会话、不代为登录**，
//! 因此不消耗模型额度；真实 Agent 在启动时可能自行做网络或登录状态检查，那是 Agent 自己的行为。
//!
//! 结构上把协议逻辑（[`handshake_over`]，只依赖一条读消息的通道和一个写端）与进程管理
//! （[`handshake_process`]）分开，协议逻辑可以直接用内存通道测试，进程部分用假 Agent 脚本测试。
//! M2 的长连接会话见 [`connection`]，执行管理见 [`crate::run`]。

use std::ffi::OsStr;
use std::io::{BufRead, BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError};
use std::thread;
use std::time::{Duration, Instant};

use serde_json::{Value, json};

use crate::model::{AcpCapabilities, AgentInfo, Handshake, HandshakeStatus};
use crate::system_probe::{isolate_in_process_group, kill_process_group};

pub mod connection;

/// Teamspace 支持的 ACP 协议版本。
pub const PROTOCOL_VERSION: u64 = 1;
/// 握手超时：从启动进程到收到 `initialize` 响应。
pub const HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(20);
/// ACP 规定的「需要认证」错误码。
pub const AUTH_REQUIRED_CODE: i64 = -32000;
/// JSON-RPC「方法不存在」错误码。
pub const METHOD_NOT_FOUND_CODE: i64 = -32601;

/// 终止进程后等待 stderr 读线程收尾的时间。
const STDERR_GRACE: Duration = Duration::from_secs(2);
/// stdout 提前关闭时，等进程自己退出以取得退出码的时间。
const EXIT_GRACE: Duration = Duration::from_secs(2);
/// 保留的 stderr 字节上限（只用于错误说明）。
const STDERR_LIMIT: usize = 16 * 1024;
/// 错误说明中引用 Agent 输出时的字符上限。
const QUOTE_LIMIT: usize = 160;
/// `initialize` 请求的 id。
const INITIALIZE_ID: u64 = 1;

/// 启动一个 ACP Agent 所需的信息（由内核根据 Harness 实例生成，交给 [`crate::SystemProbe::handshake`]）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentLaunch {
    /// 命令名（在补全后的 PATH 中查找）或绝对路径。
    pub command: String,
    pub args: Vec<String>,
    /// 环境配置档案路径（只检查存在，不读取内容）。
    pub env_profile: Option<String>,
}

/// `initialize` 请求（一行 JSON，不含换行）。握手检查不提供任何 Client 能力。
pub fn initialize_request() -> String {
    json!({
        "jsonrpc": "2.0",
        "id": INITIALIZE_ID,
        "method": "initialize",
        "params": initialize_params(false)
    })
    .to_string()
}

/// `initialize` 的参数。`fs` 为 `true` 时声明 Client 提供文件读写（执行时；读写范围由权限规则限定，
/// 见 [`crate::run::policy`]）；终端能力始终不提供（Agent 自带的命令执行会通过权限请求来问）。
pub fn initialize_params(fs: bool) -> Value {
    json!({
        "protocolVersion": PROTOCOL_VERSION,
        "clientCapabilities": {
            "fs": { "readTextFile": fs, "writeTextFile": fs },
            "terminal": false
        },
        "clientInfo": {
            "name": "teamspace",
            "title": "Teamspace",
            "version": env!("CARGO_PKG_VERSION")
        }
    })
}

/// 握手在协议层面的失败（进程层再补上退出码和 stderr）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HandshakeFailure {
    /// 期限内没有收到响应。
    Timeout,
    /// stdout 关闭（进程退出）而没有收到响应。
    Closed,
    /// 收到不是 JSON 的一行。
    NotJson(String),
    /// 无法写入请求（通常是进程已退出）。
    Write(String),
}

/// 在已建立的传输上完成握手：写出 `initialize`，从 `lines` 读取 Agent 的消息直到收到响应或 `deadline`。
///
/// `lines` 是 Agent stdout 的逐行内容，发送端关闭表示 stdout 已关闭。期间 Agent 发来的通知被忽略，
/// 发来的请求一律回复「方法不存在」（握手阶段 Client 不提供任何能力）。
pub fn handshake_over(
    lines: &Receiver<String>,
    writer: &mut impl Write,
    deadline: Instant,
) -> std::result::Result<Handshake, HandshakeFailure> {
    send_line(writer, &initialize_request())
        .map_err(|err| HandshakeFailure::Write(err.to_string()))?;
    loop {
        let remaining = deadline.saturating_duration_since(Instant::now());
        let line = match lines.recv_timeout(remaining) {
            Ok(line) => line,
            Err(RecvTimeoutError::Timeout) => return Err(HandshakeFailure::Timeout),
            Err(RecvTimeoutError::Disconnected) => return Err(HandshakeFailure::Closed),
        };
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let message: Value =
            serde_json::from_str(line).map_err(|_| HandshakeFailure::NotJson(quote(line)))?;
        if !message.is_object() {
            return Err(HandshakeFailure::NotJson(quote(line)));
        }
        let id = message.get("id");
        if message.get("method").is_some() {
            // Agent 发来的请求：回复「方法不存在」；通知（无 id）直接忽略。
            if let Some(id) = id.filter(|id| !id.is_null()) {
                let reply = json!({
                    "jsonrpc": "2.0",
                    "id": id,
                    "error": { "code": METHOD_NOT_FOUND_CODE, "message": "Teamspace 在握手阶段不处理请求" }
                });
                // 回复失败不影响等待 initialize 的响应。
                let _ = send_line(writer, &reply.to_string());
            }
            continue;
        }
        if id.and_then(Value::as_u64) != Some(INITIALIZE_ID) {
            continue;
        }
        return Ok(interpret_response(&message));
    }
}

fn send_line(writer: &mut impl Write, line: &str) -> std::io::Result<()> {
    writer.write_all(line.as_bytes())?;
    writer.write_all(b"\n")?;
    writer.flush()
}

/// 把 `initialize` 的响应（含 `result` 或 `error` 的 JSON-RPC 消息）解释为握手结果（不含 `checkedAt`）。
pub fn interpret_response(message: &Value) -> Handshake {
    if let Some(error) = message.get("error") {
        let code = error.get("code").and_then(Value::as_i64);
        let text = error
            .get("message")
            .and_then(Value::as_str)
            .unwrap_or("（无说明）");
        if code == Some(AUTH_REQUIRED_CODE) {
            return Handshake {
                status: HandshakeStatus::NeedsAuth,
                auth_methods: auth_method_names(error.get("data")),
                detail: Some(format!(
                    "Agent 要求先登录：{text}。请在对应工具中完成登录后重新检查"
                )),
                ..Handshake::unknown()
            };
        }
        let code = code.map_or_else(|| "未知".to_owned(), |c| c.to_string());
        return Handshake::error(format!(
            "Agent 拒绝了 initialize 请求（错误码 {code}）：{text}"
        ));
    }
    let Some(result) = message.get("result").filter(|r| r.is_object()) else {
        return Handshake::error("Agent 对 initialize 的响应缺少 result");
    };
    let version = match result.get("protocolVersion") {
        Some(v) => match v.as_u64() {
            Some(v) => v,
            None => {
                return Handshake::error(format!(
                    "协议版本不兼容：Agent 返回的协议版本 {v} 无法识别，Teamspace 支持 ACP v{PROTOCOL_VERSION}"
                ));
            }
        },
        None => return Handshake::error("Agent 的 initialize 响应缺少 protocolVersion"),
    };
    if version != PROTOCOL_VERSION {
        return Handshake::error(format!(
            "协议版本不兼容：Agent 使用 ACP v{version}，Teamspace 支持 ACP v{PROTOCOL_VERSION}"
        ));
    }
    let agent = agent_info(result.get("agentInfo"));
    let detail = match &agent {
        Some(info) => match &info.version {
            Some(v) => format!("握手成功：{} {v}，ACP v{version}", info.name),
            None => format!("握手成功：{}，ACP v{version}", info.name),
        },
        None => format!("握手成功：ACP v{version}（Agent 未提供名称与版本）"),
    };
    Handshake {
        status: HandshakeStatus::Connected,
        protocol_version: u32::try_from(version).ok(),
        agent,
        capabilities: Some(capabilities(result.get("agentCapabilities"))),
        auth_methods: auth_method_names(result.get("authMethods")),
        detail: Some(detail),
        checked_at: None,
    }
}

fn str_of<'a>(value: Option<&'a Value>, key: &str) -> Option<&'a str> {
    value
        .and_then(|v| v.get(key))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
}

fn agent_info(value: Option<&Value>) -> Option<AgentInfo> {
    let name = str_of(value, "name")?;
    Some(AgentInfo {
        name: name.to_owned(),
        title: str_of(value, "title").map(str::to_owned),
        version: str_of(value, "version").map(str::to_owned),
    })
}

fn flag(value: Option<&Value>, path: &[&str]) -> bool {
    path.iter()
        .try_fold(value, |v, key| Some(v?.get(*key)))
        .flatten()
        .and_then(Value::as_bool)
        .unwrap_or(false)
}

fn capabilities(value: Option<&Value>) -> AcpCapabilities {
    AcpCapabilities {
        load_session: flag(value, &["loadSession"]),
        image: flag(value, &["promptCapabilities", "image"]),
        audio: flag(value, &["promptCapabilities", "audio"]),
        embedded_context: flag(value, &["promptCapabilities", "embeddedContext"]),
        mcp_http: flag(value, &["mcpCapabilities", "http"]),
        mcp_sse: flag(value, &["mcpCapabilities", "sse"]),
    }
}

/// `authMethods`（或 `error.data.authMethods`）中的名称；缺名称时用 id。
fn auth_method_names(value: Option<&Value>) -> Vec<String> {
    let list = value.and_then(|v| v.as_array().or_else(|| v.get("authMethods")?.as_array()));
    list.into_iter()
        .flatten()
        .filter_map(|m| str_of(Some(m), "name").or_else(|| str_of(Some(m), "id")))
        .map(str::to_owned)
        .collect()
}

/// 截断过长的引用文本。
fn quote(text: &str) -> String {
    let mut out: String = text.chars().take(QUOTE_LIMIT).collect();
    if text.chars().count() > QUOTE_LIMIT {
        out.push('…');
    }
    out
}

/// 启动 Agent 进程并完成握手，结束后整组终止进程。
///
/// `program` 须已解析为可执行文件路径；进程的工作目录为 `cwd`，环境为当前进程环境、PATH 换成
/// `path_env`（不额外注入任何密钥）。失败一律体现在返回值中（`status = error`，中文 `detail`）。
pub fn handshake_process(
    program: &Path,
    args: &[String],
    cwd: &Path,
    path_env: &OsStr,
    timeout: Duration,
) -> Handshake {
    let display = command_line(program, args);
    let mut command = Command::new(program);
    command
        .args(args)
        .current_dir(cwd)
        .env("PATH", path_env)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    isolate_in_process_group(&mut command);
    let mut child = match command.spawn() {
        Ok(child) => child,
        Err(err) => return Handshake::error(format!("无法启动 {display}：{err}")),
    };

    let deadline = Instant::now() + timeout;
    let lines = child.stdout.take().map(read_lines);
    let stderr = child.stderr.take().map(read_capped);
    let result = match (child.stdin.take(), &lines) {
        (Some(mut stdin), Some(lines)) => {
            let result = handshake_over(lines, &mut stdin, deadline);
            drop(stdin);
            result
        }
        _ => Err(HandshakeFailure::Write(
            "无法连接进程的标准输入输出".to_owned(),
        )),
    };

    // stdout 已关闭说明进程多半已经退出：稍等一下拿到真实退出码，再整组终止。
    let exit = match &result {
        Err(HandshakeFailure::Closed | HandshakeFailure::Write(_)) => wait_exit(&mut child),
        _ => None,
    };
    stop(&mut child);
    let stderr = stderr
        .and_then(|rx| rx.recv_timeout(STDERR_GRACE).ok())
        .and_then(|text| last_meaningful_line(&text));

    match result {
        Ok(handshake) => handshake,
        Err(failure) => {
            let mut detail = match failure {
                HandshakeFailure::Timeout => format!(
                    "握手超时：{display} 在 {} 秒内没有响应 initialize",
                    timeout.as_secs_f32().ceil() as u64
                ),
                HandshakeFailure::Closed | HandshakeFailure::Write(_) => match exit {
                    Some(code) => {
                        format!("Agent 进程在握手完成前退出（{code}）：{display}")
                    }
                    None => format!("Agent 进程在握手完成前关闭了输出：{display}"),
                },
                HandshakeFailure::NotJson(line) => format!(
                    "Agent 的输出不是 JSON-RPC 消息（请确认 {display} 是 ACP Agent 的启动命令）：{line}"
                ),
            };
            if let Some(stderr) = stderr {
                detail.push_str(&format!("；stderr：{stderr}"));
            }
            Handshake::error(detail)
        }
    }
}

/// 界面上显示的命令行：命令与参数以空格连接，含空白的部分加引号。
pub fn command_line(program: &Path, args: &[String]) -> String {
    std::iter::once(program.display().to_string())
        .chain(args.iter().cloned())
        .map(|part| {
            if part.is_empty() || part.chars().any(char::is_whitespace) {
                format!("\"{part}\"")
            } else {
                part
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// 逐行读 stdout，发到通道；EOF 或读错时关闭通道。
fn read_lines<R: Read + Send + 'static>(stream: R) -> Receiver<String> {
    let (sender, receiver) = mpsc::channel();
    thread::spawn(move || {
        let mut reader = BufReader::new(stream);
        loop {
            let mut buf = Vec::new();
            match reader.read_until(b'\n', &mut buf) {
                Ok(0) | Err(_) => break,
                Ok(_) => {
                    if sender
                        .send(String::from_utf8_lossy(&buf).into_owned())
                        .is_err()
                    {
                        break;
                    }
                }
            }
        }
    });
    receiver
}

/// 读完 stderr（最多保留 [`STDERR_LIMIT`] 字节，其余丢弃但继续读，避免 Agent 写满管道卡住）。
fn read_capped<R: Read + Send + 'static>(mut stream: R) -> Receiver<String> {
    let (sender, receiver) = mpsc::channel();
    thread::spawn(move || {
        let mut kept = Vec::new();
        let mut chunk = [0u8; 4096];
        while let Ok(n) = stream.read(&mut chunk) {
            if n == 0 {
                break;
            }
            let room = STDERR_LIMIT.saturating_sub(kept.len());
            kept.extend_from_slice(&chunk[..n.min(room)]);
        }
        let _ = sender.send(String::from_utf8_lossy(&kept).into_owned());
    });
    receiver
}

/// stderr 最后一行非空内容（通常是错误原因），截断后返回。
fn last_meaningful_line(text: &str) -> Option<String> {
    text.lines()
        .map(str::trim)
        .rfind(|line| !line.is_empty())
        .map(quote)
}

/// 等进程自己退出（最多 [`EXIT_GRACE`]），返回退出情况的中文描述。
fn wait_exit(child: &mut Child) -> Option<String> {
    let deadline = Instant::now() + EXIT_GRACE;
    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                return Some(match status.code() {
                    Some(code) => format!("退出码 {code}"),
                    None => "被信号终止".to_owned(),
                });
            }
            Ok(None) if Instant::now() < deadline => thread::sleep(Duration::from_millis(20)),
            _ => return None,
        }
    }
}

/// 整组终止并回收进程。
fn stop(child: &mut Child) {
    kill_process_group(child);
    let _ = child.wait();
}

/// 握手用的临时工作目录：在系统临时目录下新建，用完删除（只删除自己建的这个空目录及 Agent 在其中
/// 留下的内容）。
pub(crate) struct ScratchDir(PathBuf);

impl ScratchDir {
    pub(crate) fn new() -> std::io::Result<Self> {
        let dir = std::env::temp_dir().join(format!("teamspace-acp-{}", uuid::Uuid::now_v7()));
        std::fs::create_dir_all(&dir)?;
        Ok(Self(dir))
    }

    pub(crate) fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for ScratchDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[cfg(test)]
mod tests {
    use std::os::unix::fs::PermissionsExt;

    use super::*;

    fn response(result: Value) -> Value {
        json!({ "jsonrpc": "2.0", "id": 1, "result": result })
    }

    #[test]
    fn initialize_request_declares_no_client_capabilities() {
        let request: Value = serde_json::from_str(&initialize_request()).unwrap();
        assert_eq!(request["method"], "initialize");
        assert_eq!(request["params"]["protocolVersion"], 1);
        let caps = &request["params"]["clientCapabilities"];
        assert_eq!(caps["fs"]["readTextFile"], false);
        assert_eq!(caps["fs"]["writeTextFile"], false);
        assert_eq!(caps["terminal"], false);
        assert_eq!(request["params"]["clientInfo"]["title"], "Teamspace");
        assert!(!initialize_request().contains('\n'));
    }

    #[test]
    fn interprets_successful_response_with_capabilities() {
        let hs = interpret_response(&response(json!({
            "protocolVersion": 1,
            "agentCapabilities": {
                "loadSession": true,
                "promptCapabilities": { "image": true, "embeddedContext": true },
                "mcpCapabilities": { "http": true }
            },
            "authMethods": [{ "id": "claude-login", "name": "Log in with Claude" }, { "id": "key" }],
            "agentInfo": { "name": "claude-code-acp", "version": "0.9.2" }
        })));
        assert_eq!(hs.status, HandshakeStatus::Connected);
        assert_eq!(hs.protocol_version, Some(1));
        let agent = hs.agent.unwrap();
        assert_eq!(agent.name, "claude-code-acp");
        assert_eq!(agent.version.as_deref(), Some("0.9.2"));
        assert_eq!(
            hs.capabilities.unwrap(),
            AcpCapabilities {
                load_session: true,
                image: true,
                audio: false,
                embedded_context: true,
                mcp_http: true,
                mcp_sse: false,
            }
        );
        assert_eq!(hs.auth_methods, vec!["Log in with Claude", "key"]);
        assert!(hs.detail.unwrap().contains("claude-code-acp 0.9.2"));
    }

    #[test]
    fn missing_agent_info_is_empty_not_an_error() {
        let hs = interpret_response(&response(json!({ "protocolVersion": 1 })));
        assert_eq!(hs.status, HandshakeStatus::Connected);
        assert_eq!(hs.agent, None);
        assert_eq!(hs.capabilities, Some(AcpCapabilities::default()));
        assert!(hs.auth_methods.is_empty());
    }

    #[test]
    fn incompatible_or_missing_protocol_version_is_an_error() {
        for (result, needle) in [
            (json!({ "protocolVersion": 2 }), "协议版本不兼容"),
            (json!({ "protocolVersion": "1.0" }), "协议版本不兼容"),
            (json!({}), "缺少 protocolVersion"),
        ] {
            let hs = interpret_response(&response(result));
            assert_eq!(hs.status, HandshakeStatus::Error);
            assert!(hs.detail.unwrap().contains(needle));
        }
        let hs = interpret_response(&json!({ "jsonrpc": "2.0", "id": 1 }));
        assert_eq!(hs.status, HandshakeStatus::Error);
    }

    #[test]
    fn auth_required_error_means_needs_auth() {
        let hs = interpret_response(&json!({
            "jsonrpc": "2.0", "id": 1,
            "error": { "code": -32000, "message": "Authentication required",
                       "data": { "authMethods": [{ "id": "oauth", "name": "ChatGPT 登录" }] } }
        }));
        assert_eq!(hs.status, HandshakeStatus::NeedsAuth);
        assert_eq!(hs.auth_methods, vec!["ChatGPT 登录"]);
        assert!(hs.detail.unwrap().contains("要求先登录"));

        let other = interpret_response(&json!({
            "jsonrpc": "2.0", "id": 1, "error": { "code": -32603, "message": "boom" }
        }));
        assert_eq!(other.status, HandshakeStatus::Error);
        assert!(other.detail.unwrap().contains("-32603"));
    }

    #[test]
    fn handshake_over_skips_notifications_and_answers_requests() {
        let (tx, rx) = mpsc::channel();
        tx.send("\n".into()).unwrap();
        tx.send(r#"{"jsonrpc":"2.0","method":"session/update","params":{}}"#.into())
            .unwrap();
        tx.send(r#"{"jsonrpc":"2.0","id":7,"method":"fs/read_text_file","params":{}}"#.into())
            .unwrap();
        tx.send(r#"{"jsonrpc":"2.0","id":99,"result":{}}"#.into())
            .unwrap();
        tx.send(r#"{"jsonrpc":"2.0","id":1,"result":{"protocolVersion":1}}"#.into())
            .unwrap();
        let mut written = Vec::new();
        let hs =
            handshake_over(&rx, &mut written, Instant::now() + Duration::from_secs(5)).unwrap();
        assert_eq!(hs.status, HandshakeStatus::Connected);
        let written = String::from_utf8(written).unwrap();
        let lines: Vec<Value> = written
            .lines()
            .map(|l| serde_json::from_str(l).unwrap())
            .collect();
        assert_eq!(lines[0]["method"], "initialize");
        assert_eq!(lines[1]["id"], 7);
        assert_eq!(lines[1]["error"]["code"], METHOD_NOT_FOUND_CODE);
    }

    #[test]
    fn handshake_over_reports_not_json_closed_and_timeout() {
        let (tx, rx) = mpsc::channel();
        tx.send("Welcome to my agent!".into()).unwrap();
        let err = handshake_over(
            &rx,
            &mut Vec::new(),
            Instant::now() + Duration::from_secs(5),
        )
        .unwrap_err();
        assert_eq!(
            err,
            HandshakeFailure::NotJson("Welcome to my agent!".into())
        );

        let (tx, rx) = mpsc::channel::<String>();
        drop(tx);
        let err = handshake_over(
            &rx,
            &mut Vec::new(),
            Instant::now() + Duration::from_secs(5),
        )
        .unwrap_err();
        assert_eq!(err, HandshakeFailure::Closed);

        let (_tx, rx) = mpsc::channel::<String>();
        let err = handshake_over(
            &rx,
            &mut Vec::new(),
            Instant::now() + Duration::from_millis(50),
        )
        .unwrap_err();
        assert_eq!(err, HandshakeFailure::Timeout);
    }

    // ---------------------------------------------------------------- 真实子进程（shell 脚本假 Agent）

    fn script(dir: &Path, body: &str) -> PathBuf {
        let path = dir.join("agent.sh");
        std::fs::write(&path, format!("#!/bin/sh\n{body}\n")).unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
        path
    }

    fn run(body: &str, timeout: Duration) -> Handshake {
        let dir = tempfile::tempdir().unwrap();
        let program = script(dir.path(), body);
        handshake_process(
            &program,
            &[],
            dir.path(),
            OsStr::new("/usr/bin:/bin"),
            timeout,
        )
    }

    #[test]
    fn process_handshake_succeeds_with_fake_agent() {
        let hs = run(
            r#"read line
case "$line" in *'"method":"initialize"'*) ;; *) echo "bad request: $line" >&2; exit 9;; esac
echo '{"jsonrpc":"2.0","method":"session/update","params":{}}'
echo '{"jsonrpc":"2.0","id":1,"result":{"protocolVersion":1,"agentCapabilities":{"loadSession":true},"authMethods":[],"agentInfo":{"name":"fake-agent","version":"1.2.3"}}}'
# 握手后不退出，验证 Teamspace 会主动终止进程。
exec /bin/sleep 30"#,
            Duration::from_secs(10),
        );
        assert_eq!(hs.status, HandshakeStatus::Connected, "{:?}", hs.detail);
        assert_eq!(hs.agent.unwrap().name, "fake-agent");
        assert!(hs.capabilities.unwrap().load_session);
    }

    #[test]
    fn process_handshake_reports_needs_auth() {
        let hs = run(
            r#"read line
echo '{"jsonrpc":"2.0","id":1,"error":{"code":-32000,"message":"Authentication required"}}'"#,
            Duration::from_secs(10),
        );
        assert_eq!(hs.status, HandshakeStatus::NeedsAuth);
    }

    #[test]
    fn process_handshake_reports_exit_with_stderr() {
        let hs = run(
            "echo 'Error: not logged in' >&2\nexit 3",
            Duration::from_secs(10),
        );
        assert_eq!(hs.status, HandshakeStatus::Error);
        let detail = hs.detail.unwrap();
        assert!(detail.contains("退出码 3"), "{detail}");
        assert!(detail.contains("not logged in"), "{detail}");
    }

    #[test]
    fn process_handshake_reports_non_json_output() {
        let hs = run(
            "read line\necho 'Usage: agent [options]'\nexec /bin/sleep 30",
            Duration::from_secs(10),
        );
        assert_eq!(hs.status, HandshakeStatus::Error);
        assert!(hs.detail.unwrap().contains("不是 JSON-RPC"));
    }

    #[test]
    fn process_handshake_reports_incompatible_protocol() {
        let hs = run(
            r#"read line
echo '{"jsonrpc":"2.0","id":1,"result":{"protocolVersion":2}}'"#,
            Duration::from_secs(10),
        );
        assert_eq!(hs.status, HandshakeStatus::Error);
        assert!(hs.detail.unwrap().contains("协议版本不兼容"));
    }

    #[test]
    fn process_handshake_times_out_and_kills_group() {
        let started = Instant::now();
        // 孙进程也攥着 stdout，只杀直接子进程的话读线程会一直等下去。
        let hs = run(
            "/bin/sh -c 'exec /bin/sleep 30' &\nexec /bin/sleep 30",
            Duration::from_millis(400),
        );
        assert_eq!(hs.status, HandshakeStatus::Error);
        assert!(hs.detail.unwrap().contains("握手超时"));
        assert!(
            started.elapsed() < Duration::from_secs(8),
            "{:?}",
            started.elapsed()
        );
    }

    #[test]
    fn process_handshake_reports_unlaunchable_program() {
        let dir = tempfile::tempdir().unwrap();
        let hs = handshake_process(
            &dir.path().join("absent"),
            &["acp".into()],
            dir.path(),
            OsStr::new("/usr/bin:/bin"),
            Duration::from_secs(1),
        );
        assert_eq!(hs.status, HandshakeStatus::Error);
        assert!(hs.detail.unwrap().contains("无法启动"));
    }

    #[test]
    fn command_line_quotes_parts_with_spaces() {
        assert_eq!(
            command_line(Path::new("gemini"), &["--experimental-acp".into()]),
            "gemini --experimental-acp"
        );
        assert_eq!(
            command_line(Path::new("/My Apps/agent"), &["a b".into(), String::new()]),
            "\"/My Apps/agent\" \"a b\" \"\""
        );
    }

    #[test]
    fn scratch_dir_is_removed_on_drop() {
        let dir = ScratchDir::new().unwrap();
        let path = dir.path().to_path_buf();
        std::fs::write(path.join("x"), "x").unwrap();
        drop(dir);
        assert!(!path.exists());
    }
}
