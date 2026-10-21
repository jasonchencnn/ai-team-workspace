//! 本机控制 socket：外部会话（Leader）操作 Teamspace 的唯一入口。
//!
//! 桌面应用在数据目录开一个 `teamspace-control.sock`，外部会话通过它请求命令：
//! 请求一行一个 JSON `{"id":…, "cmd":"…", "args":{…}}`，响应一行一个 JSON
//! `{"id":…, "ok":…}` 或 `{"id":…, "err":{"kind":…,"message":…}}`。
//!
//! 所有请求都以 Leader 身份走 [`crate::dispatch`]：权限边界（白名单）在那里统一把守，
//! 越权请求被拒绝并留一条操作者为 Leader 的活动。
//!
//! socket 只监听本机 Unix domain socket，文件权限 0600（只有当前用户能连）。
//! 路径已存在时：已有进程在监听就报错，不覆盖；没人监听（上次退出留下的残留）就删掉重建。

#[cfg(unix)]
mod imp {
    use std::io::{BufRead, BufReader, Write};
    use std::os::unix::fs::{FileTypeExt, PermissionsExt};
    use std::os::unix::net::{UnixListener, UnixStream};
    use std::path::{Path, PathBuf};
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::{Arc, Mutex};
    use std::thread::JoinHandle;
    use std::time::Duration;

    use serde_json::{Value, json};

    use crate::dispatch::{self, Caller};
    use crate::{Error, Teamspace};

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
    /// socket 文件权限为 0600。路径已存在时：已有进程在监听就报错（不覆盖）；没人监听
    /// （上次退出留下的残留文件）就删掉重建。
    pub fn serve(ts: Arc<Teamspace>, path: &Path) -> std::io::Result<ControlServer> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        prepare_path(path)?;
        let listener = UnixListener::bind(path)?;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))?;
        listener.set_nonblocking(true)?;
        let stop = Arc::new(AtomicBool::new(false));
        let worker_stop = Arc::clone(&stop);
        let worker = std::thread::Builder::new()
            .name("teamspace-control".to_owned())
            .spawn(move || accept_loop(&listener, ts, &worker_stop))?;
        Ok(ControlServer {
            path: path.to_path_buf(),
            stop,
            worker: Mutex::new(Some(worker)),
        })
    }

    /// 准备 socket 路径：有人监听就报错（不覆盖），没人监听的残留 socket 文件删掉重建。
    ///
    /// 路径上不是 socket 文件（普通文件、目录等）时报错，不动用户的数据。
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
                    "控制 socket 路径已被占用（不是 socket 文件）：{}",
                    path.display()
                ),
            ));
        }
        match UnixStream::connect(path) {
            Ok(_) => Err(std::io::Error::new(
                std::io::ErrorKind::AddrInUse,
                format!("控制 socket 已被占用：{}（已有进程在监听）", path.display()),
            )),
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

    /// 接受连接：每个连接一个线程，连接处理期间不影响其他连接。
    fn accept_loop(listener: &UnixListener, ts: Arc<Teamspace>, stop: &AtomicBool) {
        while !stop.load(Ordering::SeqCst) {
            match listener.accept() {
                Ok((stream, _)) => {
                    // 监听 socket 是非阻塞的，接过来的连接恢复为阻塞模式。
                    let _ = stream.set_nonblocking(false);
                    let ts = Arc::clone(&ts);
                    let _ = std::thread::Builder::new()
                        .name("teamspace-control-conn".to_owned())
                        .spawn(move || {
                            let _ = serve_connection(stream, ts);
                        });
                }
                Err(err) if err.kind() == std::io::ErrorKind::WouldBlock => {
                    std::thread::sleep(Duration::from_millis(20));
                }
                Err(_) => break,
            }
        }
    }

    /// 按行读请求、按行写响应。
    fn serve_connection(stream: UnixStream, ts: Arc<Teamspace>) -> std::io::Result<()> {
        let mut writer = stream.try_clone()?;
        let reader = BufReader::new(stream);
        for line in reader.lines() {
            let line = line?;
            if line.trim().is_empty() {
                continue;
            }
            let reply = handle_line(&ts, &line);
            writer.write_all(reply.as_bytes())?;
            writer.write_all(b"\n")?;
            writer.flush()?;
        }
        Ok(())
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

#[cfg(all(test, unix))]
mod tests {
    use std::io::{BufRead, BufReader, Write};
    use std::os::unix::fs::PermissionsExt;
    use std::os::unix::net::{UnixListener, UnixStream};
    use std::path::{Path, PathBuf};
    use std::sync::Arc;

    use serde_json::{Value, json};

    use super::*;
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
        let path = dir.join("teamspace-control.sock");
        let server = serve(Arc::clone(&ts), &path).unwrap();
        (ts, path, server)
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

    /// 路径上是普通文件时报错，不覆盖用户的数据。
    #[test]
    fn a_regular_file_at_the_socket_path_is_not_replaced() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("teamspace-control.sock");
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

    /// Leader 经 socket 换绑 Harness 被拒，并留下操作者为 Leader 的越权活动。
    #[test]
    fn leader_binding_harness_is_rejected_over_the_socket() {
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

        let reply = request(
            &path,
            "bind_role_harness",
            json!({ "roleId": role.id, "harnessId": harness.id }),
        );
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
        assert_eq!(denied.data["command"], "bind_role_harness");
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
