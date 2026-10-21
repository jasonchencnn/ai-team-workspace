//! 本机控制 socket 客户端：把 MCP 工具调用转成 Teamspace 桌面应用的命令请求。
//!
//! 协议与 [`teamspace_core::control`] 一致：一行一个 JSON 请求
//! `{"id":…, "cmd":"…", "args":{…}}`，一行一个 JSON 响应
//! `{"id":…, "ok":…}` 或 `{"id":…, "err":{"kind":…,"message":…}}`。
//!
//! 应用没有运行时（socket 连不上）不是崩溃，而是给 Leader 一句清楚的提示：
//! 「Teamspace 应用没有运行，请先打开 Teamspace」。

use std::path::{Path, PathBuf};

use serde_json::{Value, json};

/// 环境变量名：覆盖控制 socket 的路径（测试用）。
pub const SOCKET_ENV: &str = "TEAMSPACE_CONTROL_SOCKET";

/// 控制 socket 文件名（放在应用数据目录下的 `control/` 子目录里）。
pub const SOCKET_NAME: &str = "teamspace-control.sock";

/// 控制 socket 所在子目录（放在应用数据目录下，权限 0700：只有当前用户能进）。
pub const SOCKET_DIR_NAME: &str = "control";

/// 桌面应用标识（macOS 上是应用数据目录名）。
pub const APP_IDENTIFIER: &str = "com.ai-team-workspace.desktop";

/// 应用没有运行时的提示（D26：应用未运行时提示先打开 Teamspace）。
pub const APP_NOT_RUNNING_HINT: &str = "Teamspace 应用没有运行，请先打开 Teamspace";

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

/// 控制 socket 的默认路径（应用数据目录下的 `control/teamspace-control.sock`）。
pub fn default_socket_path() -> PathBuf {
    app_data_dir().join(SOCKET_DIR_NAME).join(SOCKET_NAME)
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
        }
    }
}

/// 控制 socket 客户端。每次请求新建一个连接（桌面应用按连接逐行处理）。
#[derive(Debug, Clone)]
pub struct ControlClient {
    path: PathBuf,
}

impl ControlClient {
    /// 指定 socket 路径。
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
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

    /// 发一条命令，返回 `ok` 的内容；失败时给出中文说明。
    pub async fn call(&self, cmd: &str, args: Value) -> Result<Value, ControlError> {
        let path = self.path.clone();
        let cmd = cmd.to_owned();
        // socket 读写是阻塞的，放到阻塞线程池里，不占住异步运行时。
        tokio::task::spawn_blocking(move || call_blocking(&path, &cmd, args))
            .await
            .map_err(|err| ControlError::Protocol {
                detail: format!("控制 socket 请求线程异常：{err}"),
            })?
    }
}

/// 阻塞地发一条命令并读回响应。
#[cfg(unix)]
fn call_blocking(path: &Path, cmd: &str, args: Value) -> Result<Value, ControlError> {
    use std::io::{BufRead, BufReader, Write};
    use std::os::unix::net::UnixStream;

    let mut stream = UnixStream::connect(path).map_err(|err| ControlError::AppNotRunning {
        path: path.to_path_buf(),
        detail: err.to_string(),
    })?;
    let request = json!({ "id": 1, "cmd": cmd, "args": args }).to_string();
    stream
        .write_all(request.as_bytes())
        .and_then(|()| stream.write_all(b"\n"))
        .and_then(|()| stream.flush())
        .map_err(|err| ControlError::Protocol {
            detail: format!("写请求失败：{err}"),
        })?;
    let mut reader = BufReader::new(stream);
    let mut line = String::new();
    reader
        .read_line(&mut line)
        .map_err(|err| ControlError::Protocol {
            detail: format!("读响应失败：{err}"),
        })?;
    let reply: Value = serde_json::from_str(line.trim()).map_err(|err| ControlError::Protocol {
        detail: format!("响应不是合法 JSON：{err}"),
    })?;
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
fn call_blocking(path: &Path, _cmd: &str, _args: Value) -> Result<Value, ControlError> {
    Err(ControlError::Protocol {
        detail: format!(
            "本机控制 socket 暂不支持当前平台（只支持 Unix）：{}",
            path.display()
        ),
    })
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
        // 放在专用子目录里（权限 0700）。
        assert!(
            path.parent().unwrap().ends_with(SOCKET_DIR_NAME),
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
            call_blocking(&self.path, cmd, args)
        }
    }
}
