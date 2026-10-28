//! 本机控制 socket：外部会话（Leader）操作 Teamspace 的唯一入口。
//!
//! 桌面应用在数据目录开一个 `teamspace-control.sock`，外部会话通过它请求命令：
//! 请求一行一个 JSON `{"id":…, "cmd":"…", "args":{…}}`，响应一行一个 JSON
//! `{"id":…, "ok":…}` 或 `{"id":…, "err":{"kind":…,"message":…}}`。
//!
//! 所有请求都以 Leader 身份走 [`crate::dispatch`]：权限边界（白名单）在那里统一把守，
//! 越权请求被拒绝并留一条操作者为 Leader 的活动。
//!
//! socket 只监听本机 Unix domain socket，文件权限 0600（只有当前用户能连），
//! 放在应用数据目录下一个 0700 的专用子目录里（其他本机用户连目录都进不去）。
//! 路径已存在时：已有进程在监听就报错，不覆盖；没人监听（上次退出留下的残留）就删掉重建；
//! 路径上不是 socket 文件时报错并给出可操作的中文提示（宿主应用据此提示用户，应用本身照常运行）。
//!
//! 「有人在监听」要握手确认：连得上不等于有人在服务——macOS 上并行启动子进程时，监听 socket 的
//! 文件描述符可能被子进程继承，旧监听关闭后仍能连上。所以连上后发一次握手请求
//! （`{"id":0,"cmd":"__ping"}`），对方在短时间内应答才算真的有监听者。

#[cfg(unix)]
mod imp {
    use std::io::{BufRead, BufReader, ErrorKind, Write};
    use std::os::unix::fs::{DirBuilderExt, FileTypeExt, PermissionsExt};
    use std::os::unix::net::{UnixListener, UnixStream};
    use std::path::{Path, PathBuf};
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
    use std::sync::{Arc, Mutex};
    use std::thread::JoinHandle;
    use std::time::Duration;

    use serde_json::{Value, json};

    use crate::dispatch::{self, Caller};
    use crate::{Error, Teamspace};

    /// 单行请求的上限（1 MiB）：本机失控的客户端不能靠一行把内存吃光。
    pub(super) const MAX_LINE_BYTES: usize = 1024 * 1024;
    /// 探测 socket 上是否真有监听者的握手命令。真正的监听者必须应答它。
    pub(super) const PROBE_CMD: &str = "__ping";
    /// 探测应答的编号。
    const PROBE_ID: i64 = 0;
    /// 探测应答的等待上限：超过这个时间没应答就当残留。
    const PROBE_TIMEOUT: Duration = Duration::from_millis(500);
    /// 读请求的超时：对端不发数据也不断开时，不长期占着一条连接。
    const READ_TIMEOUT: Duration = Duration::from_secs(30);
    /// 同时在线的控制连接上限。
    pub(super) const MAX_CONNECTIONS: usize = 32;
    /// socket 所在专用子目录的权限（只有当前用户能进）。
    const DIR_MODE: u32 = 0o700;
    /// socket 文件的权限（只有当前用户能连）。
    const SOCKET_MODE: u32 = 0o600;

    /// 控制服务：监听本机 socket，接受 Leader 的请求。
    ///
    /// drop 或调用 [`ControlServer::shutdown`] 时停止监听并删除 socket 文件。
    #[derive(Debug)]
    pub struct ControlServer {
        path: PathBuf,
        stop: Arc<AtomicBool>,
        worker: Mutex<Option<JoinHandle<()>>>,
    }

    impl ControlServer {
        /// socket 文件路径。
        pub fn path(&self) -> &Path {
            &self.path
        }

        /// 停止监听并删除 socket 文件。可重复调用。
        pub fn shutdown(&self) {
            self.stop.store(true, Ordering::SeqCst);
            let worker = self
                .worker
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .take();
            if let Some(worker) = worker {
                let _ = worker.join();
            }
            let _ = std::fs::remove_file(&self.path);
        }
    }

    impl Drop for ControlServer {
        fn drop(&mut self) {
            self.shutdown();
        }
    }

    /// 在 `path` 上监听本机控制 socket。
    ///
    /// socket 放在应用数据目录下的专用子目录里（目录 0700、socket 文件 0600，只有当前用户能连）。
    /// 路径已存在时：已有进程在监听就报错（不覆盖）；没人监听（上次退出留下的残留文件）就删掉重建；
    /// 路径上不是 socket 文件时报错（不覆盖用户的数据），由宿主应用提示用户处理。
    pub fn serve(ts: Arc<Teamspace>, path: &Path) -> std::io::Result<ControlServer> {
        if let Some(parent) = path.parent() {
            ensure_private_dir(parent)?;
        }
        prepare_path(path)?;
        let listener = UnixListener::bind(path)?;
        if let Err(err) =
            std::fs::set_permissions(path, std::fs::Permissions::from_mode(SOCKET_MODE))
        {
            // 权限没设成功：删掉残留的 socket 文件，不在盘上留一个谁都能连的文件。
            let _ = std::fs::remove_file(path);
            return Err(err);
        }
        listener.set_nonblocking(true)?;
        let stop = Arc::new(AtomicBool::new(false));
        let worker_stop = Arc::clone(&stop);
        let live = Arc::new(AtomicUsize::new(0));
        let worker_live = Arc::clone(&live);
        let worker = std::thread::Builder::new()
            .name("teamspace-control".to_owned())
            .spawn(move || accept_loop(&listener, ts, &worker_stop, &worker_live))?;
        Ok(ControlServer {
            path: path.to_path_buf(),
            stop,
            worker: Mutex::new(Some(worker)),
        })
    }

    /// 建立 socket 所在的专用子目录，权限 0700（只有当前用户能进）。
    fn ensure_private_dir(dir: &Path) -> std::io::Result<()> {
        std::fs::DirBuilder::new()
            .recursive(true)
            .mode(DIR_MODE)
            .create(dir)?;
        // 目录已存在时 `create` 不会改权限，这里补一次；失败不致命（socket 文件本身是 0600）。
        if let Err(err) = std::fs::set_permissions(dir, std::fs::Permissions::from_mode(DIR_MODE)) {
            eprintln!(
                "teamspace: 无法把控制 socket 目录 {} 的权限设为 0700：{err}",
                dir.display()
            );
        }
        Ok(())
    }

    /// 准备 socket 路径：有人监听就报错（不覆盖），没人监听的残留 socket 文件删掉重建。
    ///
    /// 路径上不是 socket 文件（普通文件、目录等）时报错，不动用户的数据，并给出可操作的中文提示。
    ///
    /// 「有人在监听」不能只看连不连得上：macOS 上并行启动子进程时，监听 socket 的文件描述符
    /// 可能被子进程继承，旧监听关闭后内核仍认得这个 socket，探测照样连得上。这时若直接判定
    /// 「已有进程在监听」，重新绑定就被挡住，应用再也起不来。所以连上之后还要握手一次
    /// （[`PROBE_CMD`]）：真正的监听者必须应答；收不到应答就当残留，删掉重建。
    fn prepare_path(path: &Path) -> std::io::Result<()> {
        let meta = match std::fs::symlink_metadata(path) {
            Ok(meta) => meta,
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(()),
            Err(err) => return Err(err),
        };
        if !meta.file_type().is_socket() {
            return Err(std::io::Error::new(
                std::io::ErrorKind::AlreadyExists,
                format!(
                    "控制 socket 路径被占用（不是 socket 文件）：{}；\
                     请删除该文件（或改名）后重新启动 Teamspace",
                    path.display()
                ),
            ));
        }
        match UnixStream::connect(path) {
            Ok(stream) => {
                if probe_is_live(&stream) {
                    Err(std::io::Error::new(
                        std::io::ErrorKind::AddrInUse,
                        format!("控制 socket 已被占用：{}（已有进程在监听）", path.display()),
                    ))
                } else {
                    // 连得上但没人应答：残留（监听 fd 被子进程继承之类），删掉重建。
                    std::fs::remove_file(path)?;
                    Ok(())
                }
            }
            // 有人在监听之外的失败（连不上但文件在）视为残留，删掉重建。
            Err(err)
                if matches!(
                    err.kind(),
                    std::io::ErrorKind::ConnectionRefused | std::io::ErrorKind::NotFound
                ) =>
            {
                std::fs::remove_file(path)?;
                Ok(())
            }
            Err(err) => Err(err),
        }
    }

    /// 探测 socket 上是否真有监听者：发一次握手请求，看对方是否应答。
    ///
    /// 应答在 [`PROBE_TIMEOUT`] 内到达且格式正确才算「有人在听」；
    /// 超时、连接被关、应答不是合法 JSON，都当作残留。
    fn probe_is_live(stream: &UnixStream) -> bool {
        let mut stream = match stream.try_clone() {
            Ok(clone) => clone,
            Err(_) => return false,
        };
        if stream.set_read_timeout(Some(PROBE_TIMEOUT)).is_err()
            || stream.set_write_timeout(Some(PROBE_TIMEOUT)).is_err()
        {
            return false;
        }
        let request = json!({ "id": PROBE_ID, "cmd": PROBE_CMD }).to_string();
        if stream.write_all(request.as_bytes()).is_err()
            || stream.write_all(b"\n").is_err()
            || stream.flush().is_err()
        {
            return false;
        }
        let mut reader = BufReader::new(stream);
        let mut line = String::new();
        match reader.read_line(&mut line) {
            Ok(0) => false,
            Ok(_) => is_probe_reply(&line),
            Err(_) => false,
        }
    }

    /// 探测应答是不是有效的：合法 JSON、编号对得上、带 ok 或 err。
    ///
    /// 只看格式不看内容：对面可能是旧版本（不认识 `__ping`，回一个错误），
    /// 那也说明对面确实有个活着的监听者，不能当成残留删掉。
    pub(super) fn is_probe_reply(line: &str) -> bool {
        let Ok(value) = serde_json::from_str::<Value>(line.trim()) else {
            return false;
        };
        let Some(object) = value.as_object() else {
            return false;
        };
        if object.get("id").and_then(Value::as_i64) != Some(PROBE_ID) {
            return false;
        }
        object.contains_key("ok") || object.contains_key("err")
    }

    /// 接受连接：每个连接一个线程，连接处理期间不影响其他连接。
    ///
    /// 同时在线的连接数有上限，超了的直接回一句「连接过多」后断开，不无限开线程。
    fn accept_loop(
        listener: &UnixListener,
        ts: Arc<Teamspace>,
        stop: &AtomicBool,
        live: &Arc<AtomicUsize>,
    ) {
        while !stop.load(Ordering::SeqCst) {
            match listener.accept() {
                Ok((stream, _)) => {
                    if live.load(Ordering::SeqCst) >= MAX_CONNECTIONS {
                        let _ = refuse(stream);
                        continue;
                    }
                    // 监听 socket 是非阻塞的，接过来的连接恢复为阻塞模式。
                    let _ = stream.set_nonblocking(false);
                    let ts = Arc::clone(&ts);
                    let counter = Arc::clone(live);
                    live.fetch_add(1, Ordering::SeqCst);
                    let spawned = std::thread::Builder::new()
                        .name("teamspace-control-conn".to_owned())
                        .spawn(move || {
                            let _ = serve_connection(stream, ts);
                            counter.fetch_sub(1, Ordering::SeqCst);
                        });
                    if spawned.is_err() {
                        // 线程没起来：连接数要还回去。
                        live.fetch_sub(1, Ordering::SeqCst);
                    }
                }
                Err(err) if err.kind() == std::io::ErrorKind::WouldBlock => {
                    std::thread::sleep(Duration::from_millis(20));
                }
                Err(_) => break,
            }
        }
    }

    /// 连接数超限时回一句说明再断开。
    fn refuse(mut stream: UnixStream) -> std::io::Result<()> {
        let reply = error_reply(
            Value::Null,
            &Error::conflict(format!(
                "控制连接数已达上限（{MAX_CONNECTIONS}），请稍后重试"
            )),
        );
        stream.write_all(reply.as_bytes())?;
        stream.write_all(b"\n")?;
        stream.flush()
    }

    /// 按行读请求、按行写响应。
    ///
    /// 读请求有超时（对端不发数据也不断开时不当成一直占着连接），单行有长度上限。
    fn serve_connection(stream: UnixStream, ts: Arc<Teamspace>) -> std::io::Result<()> {
        let _ = stream.set_read_timeout(Some(READ_TIMEOUT));
        let mut writer = stream.try_clone()?;
        let mut reader = BufReader::new(stream);
        let mut buf = Vec::new();
        loop {
            match read_line(&mut reader, &mut buf) {
                // 对端关掉了连接。
                Ok(false) => return Ok(()),
                Ok(true) => {
                    let line = String::from_utf8_lossy(&buf);
                    if line.trim().is_empty() {
                        continue;
                    }
                    let reply = handle_line(&ts, &line);
                    writer.write_all(reply.as_bytes())?;
                    writer.write_all(b"\n")?;
                    writer.flush()?;
                }
                // 超长的一行：回一句错误后断开，不再往下读。
                Err(err) if err.kind() == ErrorKind::InvalidData => {
                    let reply = error_reply(Value::Null, &Error::invalid(err.to_string()));
                    let _ = writer.write_all(reply.as_bytes());
                    let _ = writer.write_all(b"\n");
                    let _ = writer.flush();
                    return Ok(());
                }
                Err(err) => return Err(err),
            }
        }
    }

    /// 读一行（到换行符为止）。单行超过 [`MAX_LINE_BYTES`] 时报错。
    ///
    /// 返回 `false` 表示对端已经关闭（没有读到任何内容）。
    fn read_line(reader: &mut BufReader<UnixStream>, buf: &mut Vec<u8>) -> std::io::Result<bool> {
        buf.clear();
        loop {
            let available = reader.fill_buf()?;
            if available.is_empty() {
                return Ok(!buf.is_empty());
            }
            match available.iter().position(|byte| *byte == b'\n') {
                Some(at) => {
                    buf.extend_from_slice(&available[..=at]);
                    reader.consume(at + 1);
                    if buf.len() > MAX_LINE_BYTES {
                        return Err(std::io::Error::new(
                            ErrorKind::InvalidData,
                            format!("请求超过 {MAX_LINE_BYTES} 字节上限"),
                        ));
                    }
                    return Ok(true);
                }
                None => {
                    let len = available.len();
                    buf.extend_from_slice(available);
                    reader.consume(len);
                    if buf.len() > MAX_LINE_BYTES {
                        return Err(std::io::Error::new(
                            ErrorKind::InvalidData,
                            format!("请求超过 {MAX_LINE_BYTES} 字节上限"),
                        ));
                    }
                }
            }
        }
    }

    /// 处理一行请求，返回一行响应。
    fn handle_line(ts: &Teamspace, line: &str) -> String {
        let request: Value = match serde_json::from_str(line) {
            Ok(value) => value,
            Err(err) => {
                return error_reply(
                    Value::Null,
                    &Error::invalid(format!("请求不是合法的 JSON：{err}")),
                );
            }
        };
        let id = request.get("id").cloned().unwrap_or(Value::Null);
        let cmd = request
            .get("cmd")
            .and_then(Value::as_str)
            .unwrap_or_default();
        let args = request.get("args").cloned().unwrap_or(Value::Null);
        // 握手请求：只回答「这里有人在听吗」，不进命令分发、不需要任何权限。
        // 别的进程正是靠它分辨「真的有监听者」与「只剩残留的 socket 文件」。
        if cmd == PROBE_CMD {
            return json!({ "id": id, "ok": "pong" }).to_string();
        }
        match dispatch::dispatch(ts, Caller::Leader, cmd, &args) {
            Ok(value) => json!({ "id": id, "ok": value }).to_string(),
            Err(err) => json!({ "id": id, "err": err }).to_string(),
        }
    }

    /// 错误响应。
    fn error_reply(id: Value, err: &Error) -> String {
        json!({ "id": id, "err": err.payload() }).to_string()
    }
}

#[cfg(not(unix))]
mod imp {
    use std::path::{Path, PathBuf};

    use crate::Teamspace;

    /// 非 Unix 平台没有控制服务（socket 文件不参与编译）。
    #[derive(Debug)]
    pub struct ControlServer {
        path: PathBuf,
    }

    impl ControlServer {
        /// socket 文件路径（非 Unix 平台没有 socket 文件）。
        pub fn path(&self) -> &Path {
            &self.path
        }

        /// 停止监听（非 Unix 平台没有监听可停）。
        pub fn shutdown(&self) {}
    }

    /// 非 Unix 平台暂不支持本机控制 socket。
    pub fn serve(_ts: std::sync::Arc<Teamspace>, _path: &Path) -> std::io::Result<ControlServer> {
        Err(std::io::Error::new(
            std::io::ErrorKind::Unsupported,
            "本机控制 socket 暂不支持当前平台（只支持 Unix）",
        ))
    }
}

pub use imp::{ControlServer, serve};

use crate::Teamspace;

/// 应用退出收尾：**先**停控制 socket（Leader 连不上了，不会再有新请求进来），
/// **再**中断执行（终止 Agent 进程、把执行标记为已中断）。
///
/// 顺序很重要：中断执行要跑 Git 提交，可能有几秒；这段时间里 socket 还在监听的话，
/// Leader 就能在退出窗口里再启动一个执行，退出后留下没人管的 Agent 进程。
/// 控制 socket 没起来时（`None`）直接跳过那一步。
pub fn shutdown_on_exit(control: Option<&ControlServer>, teamspace: &Teamspace) {
    if let Some(control) = control {
        control.shutdown();
    }
    // 退出中一律拒绝分发：即使还有别的东西在连 socket，也不会再启动新的执行。
    teamspace.begin_exit();
    teamspace.shutdown();
}

#[cfg(all(test, unix))]
mod tests {
    use std::io::{BufRead, BufReader, Write};
    use std::os::unix::fs::PermissionsExt;
    use std::os::unix::net::{UnixListener, UnixStream};
    use std::path::{Path, PathBuf};
    use std::sync::Arc;
    use std::time::Duration;

    use serde_json::{Value, json};

    use super::*;
    use crate::dispatch;
    use crate::model::{AcpPreset, Actor, EventKind, TaskStatus};
    use crate::tests::{FakeProbe, MOCK_HOST, templates};
    use crate::{AgentCommand, HostConfig, Teamspace};

    /// 开一个控制服务（临时目录里的 socket），返回内核与 socket 路径。
    fn serve_in(dir: &Path) -> (Arc<Teamspace>, PathBuf, ControlServer) {
        let config = HostConfig {
            mock_agent: Some(AgentCommand {
                program: MOCK_HOST.into(),
                args: vec![crate::mock_agent::MOCK_AGENT_FLAG.into()],
            }),
        };
        let ts = Arc::new(
            Teamspace::open_with(&dir.join("teamspace.db"), Box::new(FakeProbe), config).unwrap(),
        );
        let path = socket_path_in(dir);
        let server = serve(Arc::clone(&ts), &path).unwrap();
        (ts, path, server)
    }

    /// 控制 socket 的路径：应用数据目录下的 `control/` 子目录里。
    fn socket_path_in(dir: &Path) -> PathBuf {
        dir.join("control").join("teamspace-control.sock")
    }

    /// 用标准库 `UnixStream` 发一条请求，读取一行响应。
    fn request(path: &Path, cmd: &str, args: Value) -> Value {
        let mut stream = UnixStream::connect(path).unwrap();
        let line = json!({ "id": 7, "cmd": cmd, "args": args }).to_string();
        stream.write_all(line.as_bytes()).unwrap();
        stream.write_all(b"\n").unwrap();
        stream.flush().unwrap();
        let mut reader = BufReader::new(stream);
        let mut reply = String::new();
        reader.read_line(&mut reply).unwrap();
        serde_json::from_str(&reply).unwrap()
    }

    /// socket 文件只有当前用户能连。
    #[test]
    fn socket_file_is_owner_only() {
        let dir = tempfile::tempdir().unwrap();
        let (_ts, path, _server) = serve_in(dir.path());
        let mode = std::fs::metadata(&path).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o600, "socket 权限应为 0600，实际 {mode:o}");
    }

    /// 关闭后 socket 文件被删除。
    #[test]
    fn shutdown_removes_the_socket_file() {
        let dir = tempfile::tempdir().unwrap();
        let (_ts, path, server) = serve_in(dir.path());
        assert!(path.exists());
        server.shutdown();
        assert!(!path.exists(), "关闭后 socket 文件应被删除");
        // 重复关闭不报错。
        server.shutdown();
    }

    /// 服务被丢弃时 socket 文件同样被删除。
    #[test]
    fn dropping_the_server_removes_the_socket_file() {
        let dir = tempfile::tempdir().unwrap();
        let (_ts, path, server) = serve_in(dir.path());
        drop(server);
        assert!(!path.exists(), "丢弃服务后 socket 文件应被删除");
    }

    /// 已有进程在监听时，第二个 serve 报错且不覆盖已有的 socket。
    #[test]
    fn second_serve_fails_while_the_first_is_listening() {
        let dir = tempfile::tempdir().unwrap();
        let (ts, path, _server) = serve_in(dir.path());
        let err = serve(Arc::clone(&ts), &path).unwrap_err();
        assert_eq!(err.kind(), std::io::ErrorKind::AddrInUse, "{err}");
        assert!(path.exists(), "已监听的 socket 不能被删掉");
        // 原来的服务仍然可用。
        let reply = request(&path, "list_teams", json!({}));
        assert!(reply.get("ok").is_some(), "{reply}");
    }

    /// 残留的 socket 文件（上次退出留下的）会被清掉重建。
    #[test]
    fn stale_socket_file_is_replaced() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("teamspace-control.sock");
        // 真实的残留 socket：绑定后直接丢弃监听，文件还在但没人监听。
        drop(UnixListener::bind(&path).unwrap());
        assert!(path.exists());

        let config = HostConfig::default();
        let ts = Arc::new(
            Teamspace::open_with(
                &dir.path().join("teamspace.db"),
                Box::new(FakeProbe),
                config,
            )
            .unwrap(),
        );
        let server = serve(Arc::clone(&ts), &path).unwrap();
        assert!(path.exists());
        let reply = request(&path, "list_teams", json!({}));
        assert!(reply.get("ok").is_some(), "{reply}");
        drop(ts);
        drop(server);
    }

    /// 监听者会应答握手请求：不进命令分发，也不需要任何权限。
    #[test]
    fn the_server_answers_the_probe_ping() {
        let dir = tempfile::tempdir().unwrap();
        let (_ts, path, _server) = serve_in(dir.path());

        // 与探测端同样的请求：编号 0，命令 `__ping`。
        let mut stream = UnixStream::connect(&path).unwrap();
        stream
            .write_all(format!("{{\"id\":0,\"cmd\":\"{}\"}}\n", super::imp::PROBE_CMD).as_bytes())
            .unwrap();
        stream.flush().unwrap();
        let mut reader = BufReader::new(stream);
        let mut reply = String::new();
        reader.read_line(&mut reply).unwrap();
        let value: Value = serde_json::from_str(&reply).unwrap();

        assert_eq!(value["id"], 0, "{reply}");
        assert_eq!(value["ok"], "pong", "{reply}");
        assert!(value.get("err").is_none(), "{reply}");
    }

    /// 探测应答不是合法 JSON 时不算「有人在听」。
    #[test]
    fn a_malformed_probe_reply_is_not_taken_as_a_live_listener() {
        assert!(!super::imp::is_probe_reply("不是 JSON"));
        assert!(!super::imp::is_probe_reply("{}"));
        assert!(!super::imp::is_probe_reply(r#"{"id":7,"ok":"pong"}"#));
        assert!(super::imp::is_probe_reply(r#"{"id":0,"ok":"pong"}"#));
        // 旧版本不认识 `__ping`，回的是错误：那也说明对面有活着的监听者。
        assert!(super::imp::is_probe_reply(
            r#"{"id":0,"err":{"kind":"invalid"}}"#
        ));
    }

    /// 缺陷 M4-G-1：socket 文件还在、连得上，但没人应答握手（监听 fd 被子进程继承的残留）。
    ///
    /// 这种情况必须当残留处理：删掉重建，否则应用永远起不来。
    #[test]
    fn a_socket_that_does_not_answer_the_probe_is_treated_as_a_residue() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("teamspace-control.sock");
        // 只监听、不接受连接：连得上（进 backlog），但不会有任何应答。
        let listener = UnixListener::bind(&path).unwrap();

        let config = HostConfig::default();
        let ts = Arc::new(
            Teamspace::open_with(
                &dir.path().join("teamspace.db"),
                Box::new(FakeProbe),
                config,
            )
            .unwrap(),
        );
        let server = serve(Arc::clone(&ts), &path).unwrap();
        assert!(path.exists());
        // 新服务可用：说明残留已被清掉并重新绑定。
        let reply = request(&path, "list_teams", json!({}));
        assert!(reply.get("ok").is_some(), "{reply}");
        drop(ts);
        drop(server);
        drop(listener);
    }

    /// 路径上是普通文件时报错，不覆盖用户的数据，并给出可操作的中文提示。
    #[test]
    fn a_regular_file_at_the_socket_path_is_not_replaced() {
        let dir = tempfile::tempdir().unwrap();
        let path = socket_path_in(dir.path());
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, b"not a socket").unwrap();
        let config = HostConfig::default();
        let ts = Arc::new(
            Teamspace::open_with(
                &dir.path().join("teamspace.db"),
                Box::new(FakeProbe),
                config,
            )
            .unwrap(),
        );
        let err = serve(Arc::clone(&ts), &path).unwrap_err();
        assert_eq!(err.kind(), std::io::ErrorKind::AlreadyExists, "{err}");
        assert_eq!(std::fs::read(&path).unwrap(), b"not a socket");
        // 提示要能照着做：说清是哪个文件、要怎么处理。
        let message = err.to_string();
        assert!(
            message.contains(path.to_string_lossy().as_ref()),
            "{message}"
        );
        assert!(message.contains("请删除该文件"), "{message}");
    }

    /// socket 在应用数据目录下的 0700 专用子目录里，socket 文件本身是 0600。
    #[test]
    fn the_socket_lives_in_a_private_directory() {
        let dir = tempfile::tempdir().unwrap();
        let (_ts, path, _server) = serve_in(dir.path());
        let parent = path.parent().unwrap();
        let mode = std::fs::metadata(parent).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o700, "socket 目录权限应为 0700，实际 {mode:o}");
        let mode = std::fs::metadata(&path).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o600, "socket 权限应为 0600，实际 {mode:o}");
    }

    /// 超过单行上限（1 MiB）的请求被拒绝，连接随即断开。
    #[test]
    fn an_oversized_request_is_rejected() {
        let dir = tempfile::tempdir().unwrap();
        let (_ts, path, _server) = serve_in(dir.path());
        let stream = UnixStream::connect(&path).unwrap();
        // 写入在另一个线程里：服务端拒绝后会断开连接，这里的写入可能失败。
        let writer = std::thread::spawn(move || {
            let mut stream = stream;
            let mut line = vec![b'x'; 1024 * 1024 + 32];
            line.push(b'\n');
            let _ = stream.write_all(&line);
            let _ = stream.flush();
            stream
        });
        let mut reader = BufReader::new(writer.join().unwrap());
        let mut reply = String::new();
        reader.read_line(&mut reply).unwrap();
        let value: Value = serde_json::from_str(&reply).unwrap();
        let message = value["err"]["message"].as_str().unwrap_or_default();
        assert!(message.contains("上限"), "{value}");
    }

    /// 同时在线的连接数有上限：超了的连接收到一句「连接过多」。
    #[test]
    fn too_many_connections_are_refused() {
        let dir = tempfile::tempdir().unwrap();
        let (_ts, path, _server) = serve_in(dir.path());
        let mut held = Vec::new();
        for _ in 0..super::imp::MAX_CONNECTIONS {
            let stream = UnixStream::connect(&path).unwrap();
            held.push(stream);
        }
        // 稍等服务把连接都收下。
        std::thread::sleep(Duration::from_millis(200));
        let stream = UnixStream::connect(&path).unwrap();
        let mut reader = BufReader::new(stream.try_clone().unwrap());
        let mut reply = String::new();
        reader.read_line(&mut reply).unwrap();
        let value: Value = serde_json::from_str(&reply).unwrap();
        let message = value["err"]["message"].as_str().unwrap_or_default();
        assert!(message.contains("上限"), "{value}");
    }

    /// 退出收尾：先停控制 socket，再中断执行——退出窗口里不再接受任何命令。
    #[test]
    fn exit_stops_the_control_socket_and_rejects_further_commands() {
        let dir = tempfile::tempdir().unwrap();
        let (ts, path, server) = serve_in(dir.path());
        assert!(path.exists());
        shutdown_on_exit(Some(&server), &ts);
        assert!(!path.exists(), "退出时控制 socket 应被停掉、文件被删除");
        // 退出中一律拒绝：不会再启动新的执行。
        let err = dispatch::dispatch(&ts, dispatch::Caller::Leader, "list_teams", &json!({}))
            .unwrap_err();
        assert!(err.message.contains("正在退出"), "{:?}", err.message);
    }

    /// Leader 经 socket 查询团队成功。
    #[test]
    fn leader_can_query_teams_over_the_socket() {
        let dir = tempfile::tempdir().unwrap();
        let (ts, path, _server) = serve_in(dir.path());
        ts.create_team(crate::model::CreateTeamInput {
            name: "团队".into(),
            description: String::new(),
            workspace_path: dir.path().to_string_lossy().into_owned(),
            roles: templates(),
        })
        .unwrap();

        let reply = request(&path, "list_teams", json!({}));
        assert_eq!(reply["id"], 7);
        let teams = reply["ok"].as_array().unwrap();
        assert_eq!(teams.len(), 1);
        assert_eq!(teams[0]["name"], "团队");
    }

    /// Leader 经 socket 创建任务成功，活动记录的操作者是 Leader。
    #[test]
    fn leader_creates_a_task_and_the_activity_is_leader() {
        let dir = tempfile::tempdir().unwrap();
        let (ts, path, _server) = serve_in(dir.path());
        let team = ts
            .create_team(crate::model::CreateTeamInput {
                name: "团队".into(),
                description: String::new(),
                workspace_path: dir.path().to_string_lossy().into_owned(),
                roles: templates(),
            })
            .unwrap();
        let role = ts
            .list_roles(&team.id)
            .unwrap()
            .into_iter()
            .find(|view| view.role.code == "role_frontend")
            .unwrap()
            .role;

        let reply = request(
            &path,
            "create_task",
            json!({ "input": {
                "teamId": team.id,
                "title": "Leader 建的任务",
                "description": "",
                "ownerRoleId": role.id,
                "status": "todo",
                "fileRefs": [],
                "dependsOn": [],
            }}),
        );
        assert!(reply.get("ok").is_some(), "{reply}");
        assert_eq!(reply["ok"]["task"]["title"], "Leader 建的任务");

        let events = ts
            .list_events(crate::model::EventQuery {
                team_id: team.id.clone(),
                ..Default::default()
            })
            .unwrap();
        let created = events
            .events
            .iter()
            .find(|event| event.kind == EventKind::TaskCreated)
            .expect("创建任务应留下活动");
        assert_eq!(created.actor, Actor::Leader);
    }

    /// Leader 经 socket 解绑 Harness 被拒，并留下操作者为 Leader 的越权活动。
    #[test]
    fn leader_unbinding_harness_is_rejected_over_the_socket() {
        let dir = tempfile::tempdir().unwrap();
        let (ts, path, _server) = serve_in(dir.path());
        let team = ts
            .create_team(crate::model::CreateTeamInput {
                name: "团队".into(),
                description: String::new(),
                workspace_path: dir.path().to_string_lossy().into_owned(),
                roles: templates(),
            })
            .unwrap();
        let role = ts
            .list_roles(&team.id)
            .unwrap()
            .into_iter()
            .find(|view| view.role.code == "role_frontend")
            .unwrap()
            .role;
        let harness = ts
            .create_harness(crate::model::CreateHarnessInput {
                team_id: team.id.clone(),
                name: "Mock".into(),
                preset: AcpPreset::Mock,
                command: None,
                args: None,
                env_profile: None,
            })
            .unwrap();
        ts.bind_role_harness(&role.id, &harness.id).unwrap();

        let reply = request(&path, "unbind_role_harness", json!({ "roleId": role.id }));
        assert!(reply.get("ok").is_none(), "{reply}");
        let message = reply["err"]["message"].as_str().unwrap();
        assert!(message.starts_with("Leader 无权执行此操作："), "{message}");

        let events = ts
            .list_events(crate::model::EventQuery {
                team_id: team.id.clone(),
                ..Default::default()
            })
            .unwrap();
        let denied = events
            .events
            .iter()
            .find(|event| event.kind == EventKind::LeaderDenied)
            .expect("越权操作应留下活动");
        assert_eq!(denied.actor, Actor::Leader);
        assert_eq!(denied.data["command"], "unbind_role_harness");
    }

    /// Leader 不能把任务置为已完成。
    #[test]
    fn leader_cannot_mark_a_task_done_over_the_socket() {
        let dir = tempfile::tempdir().unwrap();
        let (ts, path, _server) = serve_in(dir.path());
        let team = ts
            .create_team(crate::model::CreateTeamInput {
                name: "团队".into(),
                description: String::new(),
                workspace_path: dir.path().to_string_lossy().into_owned(),
                roles: templates(),
            })
            .unwrap();
        let role = ts
            .list_roles(&team.id)
            .unwrap()
            .into_iter()
            .find(|view| view.role.code == "role_frontend")
            .unwrap()
            .role;
        let detail = ts
            .create_task(crate::model::CreateTaskInput {
                team_id: team.id.clone(),
                title: "任务".into(),
                description: String::new(),
                owner_role_id: role.id,
                status: TaskStatus::Todo,
                file_refs: vec![],
                depends_on: vec![],
            })
            .unwrap();

        let reply = request(
            &path,
            "change_task_status",
            json!({ "input": { "taskId": detail.task.id, "to": "done" } }),
        );
        assert!(reply.get("ok").is_none(), "{reply}");
        assert_eq!(
            ts.get_task(&detail.task.id).unwrap().task.status,
            TaskStatus::Todo
        );
    }
}
