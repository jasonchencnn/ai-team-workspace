//! 本机控制 socket 客户端：把 MCP 工具调用转成 Teamspace 桌面应用的命令请求。
//!
//! 协议与 [`teamspace_core::control`] 一致：一行一个 JSON 请求
//! `{"id":…, "cmd":"…", "args":{…}}`，一行一个 JSON 响应
//! `{"id":…, "ok":…}` 或 `{"id":…, "err":{"kind":…,"message":…}}`。
//!
//! 应用没有运行时（socket 连不上）不是崩溃，而是给 Leader 一句清楚的提示：
//! 「Teamspace 应用没有运行，请先打开 Teamspace」。

use std::path::{Path, PathBuf};
use std::time::Duration;

use serde_json::{Value, json};

/// 环境变量名：覆盖控制 socket 的路径（测试用）。
pub const SOCKET_ENV: &str = "TEAMSPACE_CONTROL_SOCKET";

/// 控制 socket 文件名（放在应用数据目录里）。
pub const SOCKET_NAME: &str = "teamspace-control.sock";

/// 桌面应用标识（macOS 上是应用数据目录名）。
pub const APP_IDENTIFIER: &str = "com.ai-team-workspace.desktop";

/// 应用没有运行时的提示（D26：应用未运行时提示先打开 Teamspace）。
pub const APP_NOT_RUNNING_HINT: &str = "Teamspace 应用没有运行，请先打开 Teamspace";

/// 应用响应超时的提示：应用卡住时工具调用不能一直挂着。
pub const APP_TIMEOUT_HINT: &str = "Teamspace 应用响应超时，可能正在忙或已卡住，请稍后再试";

/// 控制 socket 客户端的默认超时（连接、读、写各一次）。
pub const DEFAULT_TIMEOUT: Duration = Duration::from_secs(30);

/// 请求里带的 id：响应必须带回同一个 id，否则算协议异常。
const REQUEST_ID: i64 = 1;

/// 应用数据目录：macOS 是 `~/Library/Application Support/<应用标识>`，
/// Linux 是 `$XDG_DATA_HOME/<应用标识>`（缺省 `~/.local/share/<应用标识>`）。
pub fn app_data_dir() -> PathBuf {
    #[cfg(target_os = "macos")]
    {
        let home = std::env::var_os("HOME").map(PathBuf::from);
        if let Some(home) = home {
            return home
                .join("Library")
                .join("Application Support")
                .join(APP_IDENTIFIER);
        }
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        if let Some(dir) = std::env::var_os("XDG_DATA_HOME").map(PathBuf::from) {
            return dir.join(APP_IDENTIFIER);
        }
        if let Some(home) = std::env::var_os("HOME").map(PathBuf::from) {
            return home.join(".local").join("share").join(APP_IDENTIFIER);
        }
    }
    PathBuf::from(APP_IDENTIFIER)
}

/// 控制 socket 的默认路径（应用数据目录下的 `teamspace-control.sock`）。
pub fn default_socket_path() -> PathBuf {
    app_data_dir().join(SOCKET_NAME)
}

/// 与控制 socket 打交道时出的问题。
#[derive(Debug, Clone)]
pub enum ControlError {
    /// 连不上控制 socket：应用没有运行，或 socket 路径不对。
    AppNotRunning { path: PathBuf, detail: String },
    /// 应用明确拒绝了这个请求（中文原因原样带回）。
    Backend { kind: String, message: String },
    /// 响应不符合协议，或当前平台不支持控制 socket。
    Protocol { detail: String },
    /// 等待应用响应超时（应用卡住或正忙）。
    Timeout { path: PathBuf, seconds: u64 },
}

impl ControlError {
    /// 给 Leader 看的中文说明。
    pub fn message(&self) -> String {
        match self {
            Self::AppNotRunning { path, detail } => format!(
                "{APP_NOT_RUNNING_HINT}（控制 socket：{}；{}）",
                path.display(),
                detail
            ),
            Self::Backend { message, .. } => message.clone(),
            Self::Protocol { detail } => format!("Teamspace 控制 socket 响应异常：{detail}"),
            Self::Timeout { path, seconds } => format!(
                "{APP_TIMEOUT_HINT}（控制 socket：{}；已等待 {seconds} 秒）",
                path.display()
            ),
        }
    }
}

/// 控制 socket 客户端。每次请求新建一个连接（桌面应用按连接逐行处理）。
#[derive(Debug, Clone)]
pub struct ControlClient {
    path: PathBuf,
    timeout: Duration,
}

impl ControlClient {
    /// 指定 socket 路径（超时用默认值 [`DEFAULT_TIMEOUT`]）。
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self {
            path: path.into(),
            timeout: DEFAULT_TIMEOUT,
        }
    }

    /// 指定 socket 路径与超时（测试用：把超时压到很短才能验证超时分支）。
    pub fn with_timeout(path: impl Into<PathBuf>, timeout: Duration) -> Self {
        Self {
            path: path.into(),
            timeout,
        }
    }

    /// 路径来自环境变量 `TEAMSPACE_CONTROL_SOCKET`，缺省为应用数据目录下的 socket。
    pub fn from_env() -> Self {
        match std::env::var_os(SOCKET_ENV) {
            Some(value) if !value.is_empty() => Self::new(PathBuf::from(value)),
            _ => Self::new(default_socket_path()),
        }
    }

    /// socket 路径。
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// 这次请求的超时时间。
    pub fn timeout(&self) -> Duration {
        self.timeout
    }

    /// 发一条命令，返回 `ok` 的内容；失败时给出中文说明。
    pub async fn call(&self, cmd: &str, args: Value) -> Result<Value, ControlError> {
        let path = self.path.clone();
        let cmd = cmd.to_owned();
        let timeout = self.timeout;
        // socket 读写是阻塞的，放到阻塞线程池里，不占住异步运行时。
        tokio::task::spawn_blocking(move || call_blocking(&path, &cmd, args, timeout))
            .await
            .map_err(|err| ControlError::Protocol {
                detail: format!("控制 socket 请求线程异常：{err}"),
            })?
    }
}

/// 阻塞地发一条命令并读回响应。
#[cfg(unix)]
fn call_blocking(
    path: &Path,
    cmd: &str,
    args: Value,
    timeout: Duration,
) -> Result<Value, ControlError> {
    use std::io::{BufRead, BufReader, Write};
    use std::os::unix::net::UnixStream;

    let mut stream = UnixStream::connect(path).map_err(|err| ControlError::AppNotRunning {
        path: path.to_path_buf(),
        detail: err.to_string(),
    })?;
    stream
        .set_read_timeout(Some(timeout))
        .map_err(|err| ControlError::Protocol {
            detail: format!("设置读超时失败：{err}"),
        })?;
    stream
        .set_write_timeout(Some(timeout))
        .map_err(|err| ControlError::Protocol {
            detail: format!("设置写超时失败：{err}"),
        })?;
    let request = json!({ "id": REQUEST_ID, "cmd": cmd, "args": args }).to_string();
    stream
        .write_all(request.as_bytes())
        .and_then(|()| stream.write_all(b"\n"))
        .and_then(|()| stream.flush())
        .map_err(|err| io_error(err, path, timeout, "写请求失败"))?;
    let mut reader = BufReader::new(stream);
    let mut line = String::new();
    let read = reader
        .read_line(&mut line)
        .map_err(|err| io_error(err, path, timeout, "读响应失败"))?;
    if read == 0 {
        // 应用在应答之前就关掉了连接：按「应用没有运行」处理。
        return Err(ControlError::AppNotRunning {
            path: path.to_path_buf(),
            detail: "应用没有返回响应就关闭了连接".to_owned(),
        });
    }
    let reply: Value = serde_json::from_str(line.trim()).map_err(|err| ControlError::Protocol {
        detail: format!("响应不是合法 JSON：{err}"),
    })?;
    if !reply.is_object() {
        return Err(ControlError::Protocol {
            detail: "响应不是一个 JSON 对象".to_owned(),
        });
    }
    if reply.get("id").and_then(Value::as_i64) != Some(REQUEST_ID) {
        return Err(ControlError::Protocol {
            detail: format!("响应的 id 与请求不一致（期望 {REQUEST_ID}）"),
        });
    }
    if let Some(err) = reply.get("err") {
        let kind = err
            .get("kind")
            .and_then(Value::as_str)
            .unwrap_or("unknown")
            .to_owned();
        let message = err
            .get("message")
            .and_then(Value::as_str)
            .unwrap_or("Teamspace 拒绝了这次请求")
            .to_owned();
        return Err(ControlError::Backend { kind, message });
    }
    Ok(reply.get("ok").cloned().unwrap_or(Value::Null))
}

/// 非 Unix 平台没有控制 socket（与内核里的处理一致）。
#[cfg(not(unix))]
fn call_blocking(
    path: &Path,
    _cmd: &str,
    _args: Value,
    _timeout: Duration,
) -> Result<Value, ControlError> {
    Err(ControlError::Protocol {
        detail: format!(
            "本机控制 socket 暂不支持当前平台（只支持 Unix）：{}",
            path.display()
        ),
    })
}

/// 把 socket 上的 IO 错误归类：超时是「应用响应超时」，其余算协议异常。
#[cfg(unix)]
fn io_error(err: std::io::Error, path: &Path, timeout: Duration, what: &str) -> ControlError {
    if matches!(
        err.kind(),
        std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
    ) {
        return ControlError::Timeout {
            path: path.to_path_buf(),
            seconds: timeout.as_secs(),
        };
    }
    ControlError::Protocol {
        detail: format!("{what}：{err}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_path_is_inside_the_app_data_dir() {
        let path = default_socket_path();
        assert!(path.ends_with(SOCKET_NAME), "{}", path.display());
        assert!(
            path.to_string_lossy().contains(APP_IDENTIFIER),
            "{}",
            path.display()
        );
    }

    #[test]
    fn connecting_to_a_missing_socket_reports_the_app_is_not_running() {
        let dir = tempfile::tempdir().unwrap();
        let client = ControlClient::new(dir.path().join("teamspace-control.sock"));
        let err = client
            .call_blocking_for_test("list_teams", json!({}))
            .expect_err("连不上应当报错");
        assert!(matches!(err, ControlError::AppNotRunning { .. }), "{err:?}");
        let message = err.message();
        assert!(message.contains(APP_NOT_RUNNING_HINT), "{message}");
    }

    impl ControlClient {
        /// 测试用：同步发一条命令。
        fn call_blocking_for_test(&self, cmd: &str, args: Value) -> Result<Value, ControlError> {
            call_blocking(&self.path, cmd, args, self.timeout)
        }
    }

    /// 假应用的行为。
    #[cfg(unix)]
    enum FakeApp {
        /// 不回应也不断开（模拟应用卡住）。
        Silent,
        /// 读完请求就断开（模拟应用崩溃）。
        HangUp,
        /// 回一句话。
        Say(String),
    }

    /// 起一个假的应用：接受连接后按 `behavior` 回应。
    #[cfg(unix)]
    fn fake_app(dir: &Path, behavior: FakeApp) -> PathBuf {
        use std::io::{BufRead, BufReader, Write};
        use std::os::unix::net::UnixListener;
        use std::thread;

        let path = dir.join("teamspace-control.sock");
        let listener = UnixListener::bind(&path).expect("绑定假 socket");
        thread::spawn(move || {
            for stream in listener.incoming() {
                let Ok(stream) = stream else { continue };
                let mut reader = BufReader::new(stream);
                let mut request = String::new();
                // 读完一整行请求再回应（免得只读到半行就把连接关了）。
                let _ = reader.read_line(&mut request);
                let mut stream = reader.into_inner();
                match &behavior {
                    // 卡住：什么都不做，但把连接留着（客户端会等到超时）。
                    FakeApp::Silent => thread::sleep(Duration::from_secs(5)),
                    // 崩溃：直接断开。
                    FakeApp::HangUp => {}
                    FakeApp::Say(reply) => {
                        let _ = stream.write_all(reply.as_bytes());
                        let _ = stream.write_all(b"\n");
                        let _ = stream.flush();
                    }
                }
            }
        });
        path
    }

    /// 应用不回应时，工具调用要超时返回，而不是一直挂着。
    #[cfg(unix)]
    #[test]
    fn a_silent_app_leads_to_a_timeout() {
        let dir = tempfile::tempdir().unwrap();
        let path = fake_app(dir.path(), FakeApp::Silent);
        let client = ControlClient::with_timeout(path, Duration::from_millis(200));
        let err = client
            .call_blocking_for_test("list_teams", json!({}))
            .expect_err("应用不应回应时应当超时");
        assert!(matches!(err, ControlError::Timeout { .. }), "{err:?}");
        let message = err.message();
        assert!(message.contains(APP_TIMEOUT_HINT), "{message}");
        assert!(message.contains("响应超时"), "{message}");
    }

    /// 应用在应答前断开连接：按「应用没有运行」处理。
    #[cfg(unix)]
    #[test]
    fn an_app_that_hangs_up_is_reported_as_not_running() {
        let dir = tempfile::tempdir().unwrap();
        let path = fake_app(dir.path(), FakeApp::HangUp);
        let client = ControlClient::with_timeout(path, Duration::from_secs(5));
        let err = client
            .call_blocking_for_test("list_teams", json!({}))
            .expect_err("应当报错");
        assert!(matches!(err, ControlError::AppNotRunning { .. }), "{err:?}");
        let message = err.message();
        assert!(message.contains(APP_NOT_RUNNING_HINT), "{message}");
    }

    /// 响应不是对象时算协议异常，不会当成「成功但没有数据」。
    #[cfg(unix)]
    #[test]
    fn a_non_object_reply_is_a_protocol_error() {
        let dir = tempfile::tempdir().unwrap();
        let path = fake_app(dir.path(), FakeApp::Say("[1, 2]".to_owned()));
        let client = ControlClient::with_timeout(path, Duration::from_secs(5));
        let err = client
            .call_blocking_for_test("list_teams", json!({}))
            .expect_err("非对象响应应当报错");
        assert!(matches!(err, ControlError::Protocol { .. }), "{err:?}");
        assert!(err.message().contains("JSON 对象"), "{}", err.message());
    }

    /// 响应的 id 与请求不一致时算协议异常。
    #[cfg(unix)]
    #[test]
    fn a_mismatched_reply_id_is_a_protocol_error() {
        let dir = tempfile::tempdir().unwrap();
        let path = fake_app(
            dir.path(),
            FakeApp::Say(r#"{"id": 99, "ok": []}"#.to_owned()),
        );
        let client = ControlClient::with_timeout(path, Duration::from_secs(5));
        let err = client
            .call_blocking_for_test("list_teams", json!({}))
            .expect_err("id 不一致应当报错");
        assert!(matches!(err, ControlError::Protocol { .. }), "{err:?}");
        assert!(err.message().contains("id"), "{}", err.message());
    }

    /// 正常响应：id 对得上时返回 `ok` 的内容。
    #[cfg(unix)]
    #[test]
    fn a_matching_reply_returns_the_ok_payload() {
        let dir = tempfile::tempdir().unwrap();
        let path = fake_app(
            dir.path(),
            FakeApp::Say(r#"{"id": 1, "ok": {"name": "团队"}}"#.to_owned()),
        );
        let client = ControlClient::with_timeout(path, Duration::from_secs(5));
        let value = client
            .call_blocking_for_test("list_teams", json!({}))
            .expect("应当成功");
        assert_eq!(value["name"], "团队", "{value}");
    }

    /// 后端拒绝时，原样带回中文原因。
    #[cfg(unix)]
    #[test]
    fn a_backend_error_is_passed_through() {
        let dir = tempfile::tempdir().unwrap();
        let path = fake_app(
            dir.path(),
            FakeApp::Say(
                r#"{"id": 1, "err": {"kind": "conflict", "message": "不能这样做"}}"#.to_owned(),
            ),
        );
        let client = ControlClient::with_timeout(path, Duration::from_secs(5));
        let err = client
            .call_blocking_for_test("list_teams", json!({}))
            .expect_err("应当报错");
        assert_eq!(err.message(), "不能这样做");
    }
}
