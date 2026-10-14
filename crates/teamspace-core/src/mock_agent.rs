//! 内置 Mock ACP Agent：用于测试和演示，不调用任何模型、不消耗额度。
//!
//! 宿主程序（桌面应用、端到端测试桥接）收到命令行参数 [`MOCK_AGENT_FLAG`] 时调用 [`run_stdio`]，
//! 以 ACP Agent 身份在 stdin / stdout 上收发按行分隔的 JSON-RPC 消息；内核通过
//! [`crate::api::HostConfig::mock_agent`] 得到启动它的命令。
//!
//! 支持的方法：
//! - `initialize`：协议版本 1、`agentInfo.name = teamspace-mock-agent`、支持 `loadSession`，
//!   无登录方式；
//! - `authenticate`：直接成功；
//! - `session/new`：记下 cwd，返回新的会话 ID；
//! - `session/load`：回放两条历史消息（`user_message_chunk`、`agent_message_chunk`）后返回；
//! - `session/prompt`：按提示文字中的标记模拟一次工作（见下）；
//! - `session/cancel` 通知：正在处理的提示以 `cancelled` 结束。
//!
//! `session/prompt` 的模式（标记写在任务标题或说明里，会随任务说明包出现在提示中）：
//! - 默认：分块发消息 → 计划 → 读取引用文件（`fs/read_text_file`）→ 申请编辑权限并通过
//!   `fs/write_text_file` 在岗位过程文件夹写 `mock-<会话>.md` → 总结 → `end_turn`；
//! - `[mock:perm]`：在默认流程中再申请一次「执行命令」权限（需要 Human 确认），允许则成功执行，
//!   拒绝则该工具调用失败，之后照常结束；
//! - `[mock:outside]`：默认流程之后，申请编辑工作目录根的 `out.md`（岗位文件夹与任务引用文件之外）的权限，
//!   不论 Human 允许与否都随后通过 `fs/write_text_file` 写入（拒绝时由 Client 拦下）；
//! - `[mock:readgrant=<kind>|<路径>]`：申请一次 `read` 或 `search`（`kind`）类权限，`locations` 为该路径
//!   （相对路径按工作目录解析），随后对同一路径调用 `fs/write_text_file`；省略参数时为 `read|out.md`；
//! - `[mock:editref]`：申请编辑任务引用文件 `文档/需求说明_v1.md` 的权限，随后写入该文件；
//! - `[mock:editjournal]`：申请编辑岗位文件夹中的 `工作日志.md` 的权限（diff 为整体覆盖），随后整体覆盖该文件；
//! - `[mock:appendjournal]`：经 `fs/read_text_file` 读出工作日志（路径取自任务说明包「本岗位工作日志 `…`」），
//!   申请编辑权限（diff 的 `oldText` 为现有内容、`newText` 为其后追加一条进展），随后把追加后的全文经
//!   `fs/write_text_file` 写回（D14 的纯追加）；
//! - `[mock:fsprobe=<范围外文件>]`：不申请权限，直接依次：`fs/read_text_file` 读取范围外文件，
//!   `fs/write_text_file` 写岗位过程文件夹内的 `fsprobe-ok.md`、写范围外文件、写 `工作日志.md`；省略参数时范围外
//!   文件为工作目录上一级的 `fsprobe-outside.txt`；每一步的结果写进总结；
//! - `[mock:linger]`：与其他标记合用；其余步骤（含权限请求的结果）都完成后不立刻总结，发一条消息后等待
//!   `session/cancel`（或 stdin 关闭），用来在界面上观察「权限处理之后仍在执行中」的状态；
//! - `[mock:failafter]`：默认流程（含写入过程文件）都完成后，不总结，发一条消息并以 JSON-RPC 错误结束提示，
//!   用来制造「已经改了文件才失败」的执行；
//! - `[mock:fail]`：发一条消息后以 JSON-RPC 错误结束提示；
//! - `[mock:refuse]`：以 `refusal` 结束；
//! - `[mock:auth]`：返回 ACP `auth_required` 错误（-32000）；
//! - `[mock:hang]`：发一条消息后一直等待，直到收到 `session/cancel`（或 stdin 关闭）；
//! - `[mock:exit]`：发一条消息后进程异常退出。
//!
//! 其他请求回复「方法不存在」，无法解析的行回复「解析错误」；stdin 关闭时退出。

use std::collections::HashMap;
use std::io::{BufRead, Write};
use std::path::{Path, PathBuf};

use serde_json::{Value, json};

use crate::run::brief::path_from_uri;

/// 宿主程序以 Mock Agent 身份运行的命令行参数。
pub const MOCK_AGENT_FLAG: &str = "--teamspace-mock-agent";
/// Mock Agent 在 `agentInfo` 中报告的名称，也是 Mock 预设显示的启动命令。
pub const AGENT_NAME: &str = "teamspace-mock-agent";
/// Mock 发给 Client 的请求 id 起点（与 Client 的 id 区分开，便于测试预先写好响应）。
pub const FIRST_REQUEST_ID: u64 = 1000;

/// 在当前进程的 stdin / stdout 上运行 Mock Agent，直到 stdin 关闭。
pub fn run_stdio() -> std::io::Result<()> {
    let stdin = std::io::stdin();
    let stdout = std::io::stdout();
    run(stdin.lock(), stdout.lock())
}

/// 在给定的输入输出上运行 Mock Agent（便于测试），直到输入结束。`[mock:exit]` 以错误返回。
pub fn run(input: impl BufRead, output: impl Write) -> std::io::Result<()> {
    let mut mock = Mock {
        input: Box::new(input),
        output: Box::new(output),
        sessions: HashMap::new(),
        count: 0,
        next_id: FIRST_REQUEST_ID,
    };
    while let Some(message) = mock.next_message()? {
        mock.handle(message)?;
    }
    Ok(())
}

struct Mock<'a> {
    input: Box<dyn BufRead + 'a>,
    output: Box<dyn Write + 'a>,
    sessions: HashMap<String, PathBuf>,
    count: u64,
    next_id: u64,
}

/// 等待 Client 响应的结果。
enum Reply {
    Result(Value),
    /// Client 返回的 JSON-RPC 错误消息。
    Error(String),
    /// 输入已结束。
    Eof,
}

impl Mock<'_> {
    fn write(&mut self, message: &Value) -> std::io::Result<()> {
        self.output.write_all(message.to_string().as_bytes())?;
        self.output.write_all(b"\n")?;
        self.output.flush()
    }

    /// 下一条可解析的消息；输入结束返回 `None`。
    fn next_message(&mut self) -> std::io::Result<Option<Value>> {
        loop {
            let mut line = String::new();
            if self.input.read_line(&mut line)? == 0 {
                return Ok(None);
            }
            let line = line.trim();
            if line.is_empty() {
                continue;
            }
            match serde_json::from_str::<Value>(line) {
                Ok(v) if v.is_object() => return Ok(Some(v)),
                _ => {
                    self.write(&json!({ "jsonrpc": "2.0", "id": null,
                                          "error": { "code": -32700, "message": "Parse error" } }))?
                }
            }
        }
    }

    fn handle(&mut self, message: Value) -> std::io::Result<()> {
        let Some(id) = message.get("id").filter(|id| !id.is_null()).cloned() else {
            // 通知（如提示之外的 session/cancel）或 Client 的响应：不回复。
            return Ok(());
        };
        let Some(method) = message.get("method").and_then(Value::as_str) else {
            return Ok(());
        };
        let params = message.get("params").cloned().unwrap_or(Value::Null);
        match method {
            "initialize" => self.write(&ok(id, initialize_result())),
            "authenticate" => self.write(&ok(id, json!({}))),
            "session/new" => {
                self.count += 1;
                let session = format!("mock-session-{}", self.count);
                let cwd = params.get("cwd").and_then(Value::as_str).unwrap_or(".");
                self.sessions.insert(session.clone(), PathBuf::from(cwd));
                self.write(&ok(id, json!({ "sessionId": session })))
            }
            "session/load" => {
                let session = str_of(&params, "sessionId").to_owned();
                let cwd = params.get("cwd").and_then(Value::as_str).unwrap_or(".");
                self.sessions.insert(session.clone(), PathBuf::from(cwd));
                self.update(&session, json!({ "sessionUpdate": "user_message_chunk",
                                              "content": { "type": "text", "text": "（回放）任务说明" } }))?;
                self.update(&session, json!({ "sessionUpdate": "agent_message_chunk",
                                              "content": { "type": "text", "text": "（回放）之前的回复" } }))?;
                self.write(&ok(id, json!({})))
            }
            "session/prompt" => self.prompt(id, &params),
            other => self.write(&json!({
                "jsonrpc": "2.0", "id": id,
                "error": { "code": -32601, "message": format!("Method not found: {other}") }
            })),
        }
    }

    fn update(&mut self, session: &str, update: Value) -> std::io::Result<()> {
        self.write(&json!({
            "jsonrpc": "2.0",
            "method": "session/update",
            "params": { "sessionId": session, "update": update }
        }))
    }

    fn say(&mut self, session: &str, text: &str) -> std::io::Result<()> {
        self.update(
            session,
            json!({ "sessionUpdate": "agent_message_chunk", "content": { "type": "text", "text": text } }),
        )
    }

    /// 向 Client 发请求并等待响应；期间收到的 `session/cancel` 记入 `cancelled`。
    fn request(
        &mut self,
        method: &str,
        params: Value,
        cancelled: &mut bool,
    ) -> std::io::Result<Reply> {
        let id = self.next_id;
        self.next_id += 1;
        self.write(&json!({ "jsonrpc": "2.0", "id": id, "method": method, "params": params }))?;
        loop {
            let Some(message) = self.next_message()? else {
                return Ok(Reply::Eof);
            };
            if message.get("method").and_then(Value::as_str) == Some("session/cancel") {
                *cancelled = true;
                continue;
            }
            if message.get("method").is_some() {
                // 处理提示期间不接受其他请求。
                if let Some(rid) = message.get("id").filter(|v| !v.is_null()).cloned() {
                    self.write(&json!({ "jsonrpc": "2.0", "id": rid,
                        "error": { "code": -32603, "message": "Mock Agent 正忙" } }))?;
                }
                continue;
            }
            if message.get("id").and_then(Value::as_u64) != Some(id) {
                continue;
            }
            return Ok(match message.get("error") {
                Some(error) => Reply::Error(
                    error
                        .get("message")
                        .and_then(Value::as_str)
                        .unwrap_or_default()
                        .to_owned(),
                ),
                None => Reply::Result(message.get("result").cloned().unwrap_or(Value::Null)),
            });
        }
    }

    fn plan(&mut self, session: &str, statuses: [&str; 3]) -> std::io::Result<()> {
        let items = [
            "读取任务说明与引用文件",
            "在过程文件夹写入工作记录",
            "总结并结束",
        ];
        let entries: Vec<Value> = items
            .iter()
            .zip(statuses)
            .map(|(content, status)| json!({ "content": content, "priority": "medium", "status": status }))
            .collect();
        self.update(
            session,
            json!({ "sessionUpdate": "plan", "entries": entries }),
        )
    }

    fn tool(&mut self, session: &str, fields: Value) -> std::io::Result<()> {
        let mut update = json!({ "sessionUpdate": "tool_call_update" });
        if let (Some(target), Some(source)) = (update.as_object_mut(), fields.as_object()) {
            for (k, v) in source {
                target.insert(k.clone(), v.clone());
            }
        }
        self.update(session, update)
    }

    /// 一次 `fs/write_text_file`；`None` 表示输入已结束，`Some(Err(消息))` 为 Client 拒绝。
    fn write_file(
        &mut self,
        session: &str,
        path: &Path,
        content: &str,
        cancelled: &mut bool,
    ) -> std::io::Result<Option<Result<(), String>>> {
        let reply = self.request(
            "fs/write_text_file",
            json!({ "sessionId": session, "path": path.display().to_string(), "content": content }),
            cancelled,
        )?;
        Ok(match reply {
            Reply::Eof => None,
            Reply::Result(_) => Some(Ok(())),
            Reply::Error(message) => Some(Err(message)),
        })
    }

    /// 越权场景：先报一次工具调用并申请权限（`kind` 为 `edit`、`read` 或 `search`），
    /// 不论 Human 或规则怎么决定，随后都对同一路径调用 `fs/write_text_file`。
    fn probe_write(
        &mut self,
        session: &str,
        call_id: &str,
        title: &str,
        kind: &str,
        path: &Path,
        cancelled: &mut bool,
    ) -> std::io::Result<Probe> {
        let shown = path.display().to_string();
        let content = format!("# Mock 越权写入\n\n场景：{call_id}\n");
        let mut call = json!({
            "sessionUpdate": "tool_call", "toolCallId": call_id, "title": title,
            "kind": kind, "status": "pending", "locations": [{ "path": shown }]
        });
        if kind == "edit" {
            call["content"] =
                json!([{ "type": "diff", "path": shown, "oldText": null, "newText": content }]);
        }
        self.update(session, call)?;
        let reply = self.request(
            "session/request_permission",
            json!({ "sessionId": session, "toolCall": { "toolCallId": call_id }, "options": options() }),
            cancelled,
        )?;
        match reply {
            Reply::Eof => return Ok(Probe::Eof),
            Reply::Result(r)
                if r.pointer("/outcome/outcome").and_then(Value::as_str) == Some("cancelled") =>
            {
                *cancelled = true;
            }
            _ => {}
        }
        if *cancelled {
            self.tool(
                session,
                json!({ "toolCallId": call_id, "status": "failed" }),
            )?;
            return Ok(Probe::Cancelled);
        }
        let Some(result) = self.write_file(session, path, &content, cancelled)? else {
            return Ok(Probe::Eof);
        };
        let status = if result.is_ok() {
            "completed"
        } else {
            "failed"
        };
        self.tool(session, json!({ "toolCallId": call_id, "status": status }))?;
        Ok(Probe::Done(result))
    }

    /// `[mock:appendjournal]`：读出工作日志，申请编辑权限（diff 为纯追加），再写回追加后的全文。
    fn append_journal(
        &mut self,
        session: &str,
        path: &Path,
        task_key: &str,
        cancelled: &mut bool,
    ) -> std::io::Result<Probe> {
        let shown = path.display().to_string();
        let current = match self.request(
            "fs/read_text_file",
            json!({ "sessionId": session, "path": shown }),
            cancelled,
        )? {
            Reply::Eof => return Ok(Probe::Eof),
            Reply::Result(r) => r
                .get("content")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_owned(),
            Reply::Error(message) => return Ok(Probe::Done(Err(message))),
        };
        let now = crate::role_folder::local_time(&chrono::Utc::now().to_rfc3339());
        let content = format!("{current}- {now} · {task_key} · Mock 进展：已写入过程文件\n");
        self.update(session, json!({
            "sessionUpdate": "tool_call", "toolCallId": "mock-appendjournal", "title": "追加工作日志",
            "kind": "edit", "status": "pending", "locations": [{ "path": shown }],
            "content": [{ "type": "diff", "path": shown, "oldText": current, "newText": content }]
        }))?;
        let reply = self.request(
            "session/request_permission",
            json!({ "sessionId": session, "toolCall": { "toolCallId": "mock-appendjournal" }, "options": options() }),
            cancelled,
        )?;
        match reply {
            Reply::Eof => return Ok(Probe::Eof),
            Reply::Result(r)
                if r.pointer("/outcome/outcome").and_then(Value::as_str) == Some("cancelled") =>
            {
                *cancelled = true;
            }
            _ => {}
        }
        if *cancelled {
            self.tool(
                session,
                json!({ "toolCallId": "mock-appendjournal", "status": "failed" }),
            )?;
            return Ok(Probe::Cancelled);
        }
        let Some(result) = self.write_file(session, path, &content, cancelled)? else {
            return Ok(Probe::Eof);
        };
        let status = if result.is_ok() {
            "completed"
        } else {
            "failed"
        };
        self.tool(
            session,
            json!({ "toolCallId": "mock-appendjournal", "status": status }),
        )?;
        Ok(Probe::Done(result))
    }

    fn finish(&mut self, id: Value, stop_reason: &str) -> std::io::Result<()> {
        self.write(&ok(id, json!({ "stopReason": stop_reason })))
    }

    /// 处理一次提示。
    fn prompt(&mut self, id: Value, params: &Value) -> std::io::Result<()> {
        let session = str_of(params, "sessionId").to_owned();
        let text = prompt_text(params);
        let cwd = self
            .sessions
            .get(&session)
            .cloned()
            .unwrap_or_else(|| PathBuf::from("."));
        let task = task_section(&text);
        let has = |mark: &str| task.contains(&format!("[mock:{mark}]"));
        let mut cancelled = false;

        if has("auth") {
            return self.write(&json!({ "jsonrpc": "2.0", "id": id,
                "error": { "code": -32000, "message": "Authentication required",
                           "data": { "authMethods": [{ "id": "mock-login", "name": "Mock 登录" }] } } }));
        }
        self.say(&session, "收到任务，")?;
        self.say(&session, "开始处理。")?;
        if has("refuse") {
            return self.finish(id, "refusal");
        }
        if has("fail") {
            self.say(&session, "遇到问题，无法继续。")?;
            return self.write(&json!({ "jsonrpc": "2.0", "id": id,
                "error": { "code": -32603, "message": "Mock 模拟的内部错误" } }));
        }
        if has("exit") {
            return Err(std::io::Error::other("Mock 模拟的异常退出"));
        }
        if has("hang") {
            self.say(&session, "正在长时间处理……")?;
            loop {
                let Some(message) = self.next_message()? else {
                    return Ok(());
                };
                if message.get("method").and_then(Value::as_str) == Some("session/cancel") {
                    return self.finish(id, "cancelled");
                }
            }
        }

        // 1. 计划与读取
        self.plan(&session, ["in_progress", "pending", "pending"])?;
        let link = resource_paths(params).into_iter().next();
        let read_target = link.clone().unwrap_or_else(|| cwd.clone());
        self.update(
            &session,
            json!({
                "sessionUpdate": "tool_call", "toolCallId": "mock-read", "title": "读取引用文件",
                "kind": "read", "status": "in_progress",
                "locations": [{ "path": read_target.display().to_string() }]
            }),
        )?;
        let mut read_ok = link.is_none();
        if let Some(path) = &link {
            let reply = self.request(
                "fs/read_text_file",
                json!({ "sessionId": session, "path": path.display().to_string() }),
                &mut cancelled,
            )?;
            match reply {
                Reply::Eof => return Ok(()),
                Reply::Result(_) => read_ok = true,
                Reply::Error(_) => {}
            }
        }
        self.tool(
            &session,
            json!({ "toolCallId": "mock-read",
            "status": if read_ok { "completed" } else { "failed" } }),
        )?;
        if cancelled {
            return self.finish(id, "cancelled");
        }

        // 2. 在过程文件夹写文件：先申请编辑权限，再经 fs/write_text_file 写入
        self.plan(&session, ["completed", "in_progress", "pending"])?;
        let folder = process_folder(&text).unwrap_or_else(|| "Teamspace/过程文件".to_owned());
        let file = cwd.join(&folder).join(format!("mock-{session}.md"));
        let content = format!("# Mock 工作记录\n\n会话：{session}\n");
        let file_str = file.display().to_string();
        self.update(&session, json!({
            "sessionUpdate": "tool_call", "toolCallId": "mock-write", "title": "写入过程文件",
            "kind": "edit", "status": "pending",
            "locations": [{ "path": file_str }],
            "content": [{ "type": "diff", "path": file_str, "oldText": null, "newText": content }]
        }))?;
        let reply = self.request(
            "session/request_permission",
            json!({ "sessionId": session, "toolCall": { "toolCallId": "mock-write" }, "options": options() }),
            &mut cancelled,
        )?;
        let allowed = match reply {
            Reply::Eof => return Ok(()),
            Reply::Result(r) => selected(&r).is_some_and(|o| o.starts_with("allow")),
            Reply::Error(_) => false,
        };
        let mut wrote = false;
        if allowed && !cancelled {
            match self.request(
                "fs/write_text_file",
                json!({ "sessionId": session, "path": file_str, "content": content }),
                &mut cancelled,
            )? {
                Reply::Eof => return Ok(()),
                Reply::Result(_) => wrote = true,
                Reply::Error(_) => {}
            }
        }
        self.tool(
            &session,
            json!({ "toolCallId": "mock-write",
            "status": if wrote { "completed" } else { "failed" } }),
        )?;
        if cancelled {
            return self.finish(id, "cancelled");
        }

        // 2b. 越权与边界场景（标记见模块说明）：每个场景把结果记入 notes，写进总结
        let mut notes: Vec<String> = Vec::new();
        let role_dir = Path::new(&folder).parent().map(Path::to_path_buf);
        let journal = cwd
            .join(role_dir.clone().unwrap_or_else(|| "Teamspace".into()))
            .join("工作日志.md");
        let mut scenarios: Vec<(&str, &str, &str, PathBuf)> = Vec::new();
        if has("outside") {
            scenarios.push(("outside", "edit", "写入 out.md", cwd.join("out.md")));
        }
        if let Some(spec) = marker_arg(task, "readgrant") {
            let (kind, path) = spec.split_once('|').unwrap_or(("read", spec));
            let kind = if kind == "search" { "search" } else { "read" };
            let target = Path::new(if path.is_empty() { "out.md" } else { path });
            scenarios.push(("readgrant", kind, "读取后写入", cwd.join(target)));
        } else if has("readgrant") {
            scenarios.push(("readgrant", "read", "读取后写入", cwd.join("out.md")));
        }
        if has("editref") {
            scenarios.push((
                "editref",
                "edit",
                "编辑引用文件",
                cwd.join("文档/需求说明_v1.md"),
            ));
        }
        if has("editjournal") {
            scenarios.push(("editjournal", "edit", "编辑工作日志", journal.clone()));
        }
        for (name, kind, title, path) in scenarios {
            let call_id = format!("mock-{name}");
            match self.probe_write(&session, &call_id, title, kind, &path, &mut cancelled)? {
                Probe::Eof => return Ok(()),
                Probe::Cancelled => return self.finish(id, "cancelled"),
                Probe::Done(result) => notes.push(note(name, &result)),
            }
        }
        if has("appendjournal") {
            let target = journal_path(&text).map_or_else(|| journal.clone(), PathBuf::from);
            let key = task_key(task).unwrap_or("?");
            match self.append_journal(&session, &target, key, &mut cancelled)? {
                Probe::Eof => return Ok(()),
                Probe::Cancelled => return self.finish(id, "cancelled"),
                Probe::Done(result) => notes.push(note("appendjournal", &result)),
            }
        }
        if has("fsprobe") || marker_arg(task, "fsprobe").is_some() {
            let outside = marker_arg(task, "fsprobe").map_or_else(
                || cwd.parent().unwrap_or(&cwd).join("fsprobe-outside.txt"),
                |p| cwd.join(p),
            );
            let in_scope = cwd.join(&folder).join("fsprobe-ok.md");
            let read = match self.request(
                "fs/read_text_file",
                json!({ "sessionId": session, "path": outside.display().to_string() }),
                &mut cancelled,
            )? {
                Reply::Eof => return Ok(()),
                Reply::Result(_) => Ok(()),
                Reply::Error(message) => Err(message),
            };
            notes.push(note("fsprobe-read", &read));
            for (label, path) in [
                ("fsprobe-write-in", in_scope),
                ("fsprobe-write-out", outside),
                ("fsprobe-write-journal", journal),
            ] {
                match self.write_file(&session, &path, "# Mock fsprobe\n", &mut cancelled)? {
                    None => return Ok(()),
                    Some(result) => notes.push(note(label, &result)),
                }
            }
        }
        if cancelled {
            return self.finish(id, "cancelled");
        }

        // 3. [mock:perm]：执行命令需要 Human 确认
        let mut ran = None;
        if has("perm") {
            self.update(&session, json!({
                "sessionUpdate": "tool_call", "toolCallId": "mock-exec", "title": "运行 echo hello",
                "kind": "execute", "status": "pending", "rawInput": { "command": "echo hello" }
            }))?;
            let reply = self.request(
                "session/request_permission",
                json!({ "sessionId": session, "toolCall": { "toolCallId": "mock-exec" }, "options": options() }),
                &mut cancelled,
            )?;
            let outcome = match reply {
                Reply::Eof => return Ok(()),
                Reply::Result(r) => r,
                Reply::Error(_) => Value::Null,
            };
            if cancelled
                || outcome.pointer("/outcome/outcome").and_then(Value::as_str) == Some("cancelled")
            {
                self.tool(
                    &session,
                    json!({ "toolCallId": "mock-exec", "status": "failed" }),
                )?;
                return self.finish(id, "cancelled");
            }
            let allowed = selected(&outcome).is_some_and(|o| o.starts_with("allow"));
            self.tool(
                &session,
                json!({ "toolCallId": "mock-exec",
                "status": if allowed { "completed" } else { "failed" } }),
            )?;
            ran = Some(allowed);
        }

        // 3b. [mock:linger]：其余步骤已完成，保持执行中直到被取消
        if has("linger") {
            self.say(&session, "步骤已处理完，等待收尾……")?;
            loop {
                let Some(message) = self.next_message()? else {
                    return Ok(());
                };
                if message.get("method").and_then(Value::as_str) == Some("session/cancel") {
                    return self.finish(id, "cancelled");
                }
            }
        }

        // 3c. [mock:failafter]：改完文件才失败
        if has("failafter") {
            self.say(&session, "写完文件后出错了。")?;
            return self.write(&json!({ "jsonrpc": "2.0", "id": id,
                "error": { "code": -32603, "message": "Mock 模拟的收尾失败" } }));
        }

        // 4. 总结
        self.plan(&session, ["completed", "completed", "completed"])?;
        let mut summary = if wrote {
            format!("已完成：在 {folder}/ 写入了 mock-{session}.md。")
        } else {
            "已完成：没有写入任何文件。".to_owned()
        };
        match ran {
            Some(true) => summary.push_str("命令 echo hello 已执行。"),
            Some(false) => summary.push_str("命令 echo hello 被拒绝，未执行。"),
            None => {}
        }
        for n in &notes {
            summary.push_str(n);
        }
        let half = summary
            .char_indices()
            .nth(summary.chars().count() / 2)
            .map_or(0, |(i, _)| i);
        let (a, b) = summary.split_at(half);
        // 新的消息条目：前面有工具调用，Client 会另起一条。
        self.say(&session, a)?;
        self.say(&session, b)?;
        self.finish(id, "end_turn")
    }
}

/// [`Mock::probe_write`] 的结果。
enum Probe {
    /// 输入已结束。
    Eof,
    /// 权限请求或之后被取消。
    Cancelled,
    /// 写入的结果：`Err` 为 Client 拒绝时返回的消息。
    Done(Result<(), String>),
}

/// 总结里一个场景的结果，如「 [outside:ok]」「 [outside:err:Teamspace 未允许这次写入：…]」（错误消息取前 30 字）。
fn note(name: &str, result: &Result<(), String>) -> String {
    match result {
        Ok(()) => format!(" [{name}:ok]"),
        Err(message) => {
            // 总结有长度上限，错误只留开头（关键词都在开头）。
            let head: String = message.chars().take(30).collect();
            format!(" [{name}:err:{head}]")
        }
    }
}

/// 任务里带参数的标记 `[mock:<name>=<参数>]` 的参数。
fn marker_arg<'a>(task: &'a str, name: &str) -> Option<&'a str> {
    let open = format!("[mock:{name}=");
    let rest = &task[task.find(&open)? + open.len()..];
    Some(&rest[..rest.find(']')?])
}

fn options() -> Value {
    json!([
        { "optionId": "allow", "name": "允许一次", "kind": "allow_once" },
        { "optionId": "always", "name": "总是允许", "kind": "allow_always" },
        { "optionId": "reject", "name": "拒绝", "kind": "reject_once" }
    ])
}

/// 权限响应中选中的选项 kind（按 Mock 自己的选项表还原）。
fn selected(result: &Value) -> Option<&'static str> {
    if result.pointer("/outcome/outcome").and_then(Value::as_str) != Some("selected") {
        return None;
    }
    match result
        .pointer("/outcome/optionId")
        .and_then(Value::as_str)?
    {
        "allow" => Some("allow_once"),
        "always" => Some("allow_always"),
        _ => Some("reject_once"),
    }
}

fn initialize_result() -> Value {
    json!({
        "protocolVersion": 1,
        "agentCapabilities": {
            "loadSession": true,
            "promptCapabilities": { "image": false, "audio": false, "embeddedContext": false },
            "mcpCapabilities": { "http": false, "sse": false }
        },
        "authMethods": [],
        "agentInfo": {
            "name": AGENT_NAME,
            "title": "Teamspace Mock Agent",
            "version": env!("CARGO_PKG_VERSION")
        }
    })
}

fn str_of<'a>(params: &'a Value, key: &str) -> &'a str {
    params.get(key).and_then(Value::as_str).unwrap_or_default()
}

/// 提示中所有文本块拼接起来的内容。
fn prompt_text(params: &Value) -> String {
    blocks(params)
        .filter(|block| block.get("type").and_then(Value::as_str) == Some("text"))
        .filter_map(|block| block.get("text").and_then(Value::as_str))
        .collect::<Vec<_>>()
        .join("\n")
}

fn blocks(params: &Value) -> impl Iterator<Item = &Value> {
    params
        .get("prompt")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
}

/// 任务说明包中「## 任务」一节（标记只认这里，避免工作日志里旧任务的标题干扰）；
/// 没有这一节时为全文。
fn task_section(text: &str) -> &str {
    let Some(start) = text.find("\n## 任务\n") else {
        return text;
    };
    let rest = &text[start + 1..];
    let end = rest[3..].find("\n## ").map_or(rest.len(), |i| i + 3);
    &rest[..end]
}

/// 提示中 resource_link / resource 指向的本地文件。
fn resource_paths(params: &Value) -> Vec<PathBuf> {
    blocks(params)
        .filter_map(|b| {
            b.get("uri")
                .or_else(|| b.pointer("/resource/uri"))
                .and_then(Value::as_str)
        })
        .filter_map(path_from_uri)
        .collect()
}

/// 任务说明包中给出的本岗位工作日志路径（「本岗位工作日志 `…`」）。
fn journal_path(text: &str) -> Option<String> {
    let start = text.find("本岗位工作日志 `")? + "本岗位工作日志 `".len();
    let rest = &text[start..];
    Some(rest[..rest.find('`')?].to_owned())
}

/// 「## 任务」一节中的任务编号（「- 编号：FE-001」）。
fn task_key(task: &str) -> Option<&str> {
    task.lines()
        .find_map(|l| l.strip_prefix("- 编号："))
        .map(str::trim)
}

/// 从任务说明包中找出岗位过程文件夹（相对工作目录），如 `Teamspace/前端/过程文件`。
fn process_folder(text: &str) -> Option<String> {
    text.match_indices("`Teamspace/").find_map(|(start, _)| {
        let rest = &text[start + 1..];
        let folder = rest[..rest.find('`')?].trim_end_matches('/');
        (folder.ends_with(crate::role_folder::PROCESS_DIR)
            && !Path::new(folder)
                .components()
                .any(|c| c == std::path::Component::ParentDir))
        .then(|| folder.to_owned())
    })
}

fn ok(id: Value, result: Value) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "result": result })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::acp::initialize_request;

    fn exchange(input: &str) -> (Vec<Value>, std::io::Result<()>) {
        let mut out = Vec::new();
        let result = run(input.as_bytes(), &mut out);
        let replies = String::from_utf8(out)
            .unwrap()
            .lines()
            .map(|l| serde_json::from_str(l).unwrap())
            .collect();
        (replies, result)
    }

    fn line(v: Value) -> String {
        format!("{v}\n")
    }

    fn prompt(id: u64, text: &str) -> String {
        line(
            json!({ "jsonrpc": "2.0", "id": id, "method": "session/prompt",
            "params": { "sessionId": "mock-session-1", "prompt": [{ "type": "text", "text": text }] } }),
        )
    }

    const NEW: &str = r#"{"jsonrpc":"2.0","id":1,"method":"session/new","params":{"cwd":"/tmp/ws","mcpServers":[]}}"#;

    #[test]
    fn answers_initialize_with_agent_info() {
        let (replies, _) = exchange(&format!("{}\n", initialize_request()));
        assert_eq!(replies.len(), 1);
        let result = &replies[0]["result"];
        assert_eq!(result["protocolVersion"], 1);
        assert_eq!(result["agentInfo"]["name"], AGENT_NAME);
        assert_eq!(result["authMethods"], json!([]));
        assert_eq!(result["agentCapabilities"]["loadSession"], true);
        let hs = crate::acp::interpret_response(&replies[0]);
        assert_eq!(hs.status, crate::model::HandshakeStatus::Connected);
    }

    #[test]
    fn simple_modes_and_unknown_methods() {
        let input = [
            format!("{NEW}\n"),
            prompt(2, "任务 [mock:refuse]"),
            prompt(3, "任务 [mock:auth]"),
            prompt(4, "任务 [mock:fail]"),
            line(json!({ "jsonrpc": "2.0", "method": "session/cancel", "params": { "sessionId": "mock-session-1" } })),
            line(json!({ "jsonrpc": "2.0", "id": 5, "method": "terminal/create", "params": {} })),
            "not json\n".to_owned(),
        ]
        .concat();
        let (replies, result) = exchange(&input);
        result.unwrap();
        assert_eq!(replies[0]["result"]["sessionId"], "mock-session-1");
        let by_id = |id: u64| {
            replies
                .iter()
                .find(|r| r["id"] == id && r.get("method").is_none())
                .unwrap()
        };
        assert_eq!(by_id(2)["result"]["stopReason"], "refusal");
        assert_eq!(by_id(3)["error"]["code"], -32000);
        assert_eq!(by_id(4)["error"]["code"], -32603);
        assert_eq!(by_id(5)["error"]["code"], -32601);
        assert_eq!(replies.last().unwrap()["error"]["code"], -32700);
        let chunks: Vec<&str> = replies
            .iter()
            .filter_map(|r| {
                r.pointer("/params/update/content/text")
                    .and_then(Value::as_str)
            })
            .collect();
        assert_eq!(&chunks[..2], &["收到任务，", "开始处理。"]);
    }

    #[test]
    fn hang_waits_for_cancel_and_exit_fails() {
        let input = [
            format!("{NEW}\n"),
            prompt(2, "[mock:hang]"),
            line(json!({ "jsonrpc": "2.0", "method": "session/cancel", "params": { "sessionId": "mock-session-1" } })),
        ]
        .concat();
        let (replies, result) = exchange(&input);
        result.unwrap();
        assert_eq!(replies.last().unwrap()["result"]["stopReason"], "cancelled");

        let (_, result) = exchange(&[format!("{NEW}\n"), prompt(2, "[mock:exit]")].concat());
        assert!(result.is_err());
    }

    #[test]
    fn default_flow_asks_permission_and_writes_through_client() {
        let brief = "过程文件放在 `Teamspace/前端/过程文件/`（相对工作目录）";
        let input = [
            format!("{NEW}\n"),
            prompt(2, brief),
            // 1000：编辑权限 → 允许；1001：写文件 → 成功
            line(json!({ "jsonrpc": "2.0", "id": 1000, "result": { "outcome": { "outcome": "selected", "optionId": "allow" } } })),
            line(json!({ "jsonrpc": "2.0", "id": 1001, "result": {} })),
        ]
        .concat();
        let (replies, result) = exchange(&input);
        result.unwrap();
        let perm = replies
            .iter()
            .find(|r| r["method"] == "session/request_permission")
            .unwrap();
        assert_eq!(perm["id"], 1000);
        assert_eq!(perm["params"]["toolCall"]["toolCallId"], "mock-write");
        let write = replies
            .iter()
            .find(|r| r["method"] == "fs/write_text_file")
            .unwrap();
        assert_eq!(
            write["params"]["path"],
            "/tmp/ws/Teamspace/前端/过程文件/mock-mock-session-1.md"
        );
        assert_eq!(replies.last().unwrap()["result"]["stopReason"], "end_turn");
        let text: String = replies
            .iter()
            .filter_map(|r| {
                r.pointer("/params/update/content/text")
                    .and_then(Value::as_str)
            })
            .collect();
        assert!(
            text.contains("已完成：在 Teamspace/前端/过程文件/ 写入了"),
            "{text}"
        );
    }

    #[test]
    fn load_replays_history() {
        let input = line(json!({ "jsonrpc": "2.0", "id": 1, "method": "session/load",
            "params": { "sessionId": "s-9", "cwd": "/tmp", "mcpServers": [] } }));
        let (replies, _) = exchange(&input);
        assert_eq!(replies.len(), 3);
        assert_eq!(
            replies[0]["params"]["update"]["sessionUpdate"],
            "user_message_chunk"
        );
        assert_eq!(replies[2]["id"], 1);
    }

    #[test]
    fn markers_only_count_in_task_section() {
        let brief = "# 说明\n## 岗位工作日志\n- [mock:fail] 旧任务\n## 任务\n- 标题：新 [mock:perm]\n## 依赖任务\n";
        let section = task_section(brief);
        assert!(section.contains("[mock:perm]"));
        assert!(!section.contains("[mock:fail]"));
        assert_eq!(task_section("[mock:hang]"), "[mock:hang]");
    }

    #[test]
    fn finds_process_folder_in_brief() {
        assert_eq!(
            process_folder("放在 `Teamspace/前端/过程文件/`（相对）").as_deref(),
            Some("Teamspace/前端/过程文件")
        );
        assert_eq!(process_folder("`Teamspace/x/岗位说明.md`"), None);
        assert_eq!(
            process_folder("`Teamspace/x/a.md` 与 `Teamspace/x/过程文件/`").as_deref(),
            Some("Teamspace/x/过程文件")
        );
        assert_eq!(process_folder("无"), None);
    }
    // ---- 越权与边界场景标记 ----

    const BRIEF: &str = "过程文件放在 `Teamspace/前端/过程文件/`（相对工作目录）";

    fn allow() -> Value {
        json!({ "outcome": { "outcome": "selected", "optionId": "allow" } })
    }

    fn reject() -> Value {
        json!({ "outcome": { "outcome": "selected", "optionId": "reject" } })
    }

    /// 带标记运行一次默认流程；`replies` 是预先写好的 Client 响应（id、result 或 error 消息）。
    fn with_marker(marker: &str, replies: Vec<(u64, Result<Value, &str>)>) -> Vec<Value> {
        let mut input = vec![format!("{NEW}\n"), prompt(2, &format!("{BRIEF}\n{marker}"))];
        // 默认流程：1000 编辑权限，1001 写过程文件。
        input.push(line(
            json!({ "jsonrpc": "2.0", "id": 1000, "result": allow() }),
        ));
        input.push(line(json!({ "jsonrpc": "2.0", "id": 1001, "result": {} })));
        for (id, reply) in replies {
            input.push(line(match reply {
                Ok(result) => json!({ "jsonrpc": "2.0", "id": id, "result": result }),
                Err(message) => json!({ "jsonrpc": "2.0", "id": id,
                    "error": { "code": -32603, "message": message } }),
            }));
        }
        let (replies, result) = exchange(&input.concat());
        result.unwrap();
        assert_eq!(replies.last().unwrap()["result"]["stopReason"], "end_turn");
        replies
    }

    fn of_method<'a>(replies: &'a [Value], method: &str) -> Vec<&'a Value> {
        replies.iter().filter(|r| r["method"] == method).collect()
    }

    fn tool_call<'a>(replies: &'a [Value], id: &str) -> &'a Value {
        replies
            .iter()
            .find(|r| r.pointer("/params/update/toolCallId") == Some(&json!(id)))
            .map(|r| &r["params"]["update"])
            .unwrap()
    }

    fn all_text(replies: &[Value]) -> String {
        replies
            .iter()
            .filter_map(|r| {
                r.pointer("/params/update/content/text")
                    .and_then(Value::as_str)
            })
            .collect()
    }

    #[test]
    fn outside_asks_for_edit_of_out_md_and_writes_after_any_decision() {
        let replies = with_marker(
            "[mock:outside]",
            vec![
                (1002, Ok(reject())),
                (1003, Err("Teamspace 未允许这次写入：…")),
            ],
        );
        let call = tool_call(&replies, "mock-outside");
        assert_eq!(call["kind"], "edit");
        assert_eq!(call["locations"][0]["path"], "/tmp/ws/out.md");
        let perms = of_method(&replies, "session/request_permission");
        assert_eq!(perms.len(), 2, "默认的一次加 outside 的一次");
        assert_eq!(perms[1]["id"], 1002);
        assert_eq!(perms[1]["params"]["toolCall"]["toolCallId"], "mock-outside");
        let kinds: Vec<&str> = perms[1]["params"]["options"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(|o| o["kind"].as_str())
            .collect();
        assert!(kinds.contains(&"allow_once") && kinds.contains(&"reject_once"));
        let writes = of_method(&replies, "fs/write_text_file");
        assert_eq!(writes[1]["params"]["path"], "/tmp/ws/out.md");
        assert!(all_text(&replies).contains("[outside:err:Teamspace 未允许这次写入"));
        assert_eq!(tool_call(&replies, "mock-outside")["kind"], "edit");
        // 之后一次工具更新把调用标为失败。
        let last_status = replies
            .iter()
            .rev()
            .find(|r| r.pointer("/params/update/toolCallId") == Some(&json!("mock-outside")))
            .unwrap();
        assert_eq!(last_status["params"]["update"]["status"], "failed");
    }

    #[test]
    fn outside_reports_success_when_the_client_writes() {
        let replies = with_marker(
            "[mock:outside]",
            vec![(1002, Ok(allow())), (1003, Ok(json!({})))],
        );
        assert!(all_text(&replies).contains("[outside:ok]"));
    }

    #[test]
    fn readgrant_requests_read_or_search_then_writes_the_same_path() {
        for kind in ["read", "search"] {
            let replies = with_marker(
                &format!("[mock:readgrant={kind}|/tmp/outside.txt]"),
                vec![
                    (1002, Ok(allow())),
                    (1003, Err("Teamspace 未允许这次写入：x")),
                ],
            );
            let call = tool_call(&replies, "mock-readgrant");
            assert_eq!(call["kind"], kind);
            assert_eq!(call["locations"][0]["path"], "/tmp/outside.txt");
            assert!(call.get("content").is_none(), "读取类调用不带 diff");
            let perms = of_method(&replies, "session/request_permission");
            assert_eq!(perms[1]["id"], 1002);
            let options: Vec<&str> = perms[1]["params"]["options"]
                .as_array()
                .unwrap()
                .iter()
                .filter_map(|o| o["kind"].as_str())
                .collect();
            assert!(options.contains(&"allow_once"));
            let write = of_method(&replies, "fs/write_text_file")[1];
            assert_eq!(write["id"], 1003);
            assert_eq!(write["params"]["path"], "/tmp/outside.txt");
            assert!(all_text(&replies).contains("[readgrant:err:"));
        }
        // 相对路径按工作目录解析；省略参数时为 read|out.md。
        let replies = with_marker(
            "[mock:readgrant=read|.git]",
            vec![(1002, Ok(allow())), (1003, Ok(json!({})))],
        );
        assert_eq!(
            of_method(&replies, "fs/write_text_file")[1]["params"]["path"],
            "/tmp/ws/.git"
        );
        let replies = with_marker(
            "[mock:readgrant]",
            vec![(1002, Ok(allow())), (1003, Ok(json!({})))],
        );
        assert_eq!(tool_call(&replies, "mock-readgrant")["kind"], "read");
        assert_eq!(
            of_method(&replies, "fs/write_text_file")[1]["params"]["path"],
            "/tmp/ws/out.md"
        );
    }

    #[test]
    fn editref_edits_the_referenced_file() {
        let replies = with_marker(
            "[mock:editref]",
            vec![(1002, Ok(allow())), (1003, Ok(json!({})))],
        );
        let call = tool_call(&replies, "mock-editref");
        assert_eq!(call["kind"], "edit");
        assert_eq!(call["locations"][0]["path"], "/tmp/ws/文档/需求说明_v1.md");
        assert_eq!(
            of_method(&replies, "fs/write_text_file")[1]["params"]["path"],
            "/tmp/ws/文档/需求说明_v1.md"
        );
        assert!(all_text(&replies).contains("[editref:ok]"));
    }

    #[test]
    fn editjournal_edits_the_journal_even_after_a_rejection() {
        let replies = with_marker(
            "[mock:editjournal]",
            vec![
                (1002, Ok(reject())),
                (1003, Err("Teamspace 未允许这次写入：x")),
            ],
        );
        let call = tool_call(&replies, "mock-editjournal");
        assert_eq!(call["kind"], "edit");
        assert_eq!(
            call["locations"][0]["path"],
            "/tmp/ws/Teamspace/前端/工作日志.md"
        );
        assert_eq!(
            of_method(&replies, "fs/write_text_file")[1]["params"]["path"],
            "/tmp/ws/Teamspace/前端/工作日志.md"
        );
        assert!(all_text(&replies).contains("[editjournal:err:"));
    }

    #[test]
    fn appendjournal_reads_then_appends_one_line() {
        let journal = "/tmp/ws/Teamspace/前端/工作日志.md";
        let input = [
            format!("{NEW}\n"),
            prompt(
                2,
                &format!(
                    "{BRIEF}\n2. 执行过程中可以在本岗位工作日志 `{journal}` 的末尾追加进展\n\
                     ## 任务\n- 编号：FE-001\n- 标题：[mock:appendjournal]\n"
                ),
            ),
            line(json!({ "jsonrpc": "2.0", "id": 1000, "result": allow() })),
            line(json!({ "jsonrpc": "2.0", "id": 1001, "result": {} })),
            line(
                json!({ "jsonrpc": "2.0", "id": 1002, "result": { "content": "# 工作日志\n\n- a\n" } }),
            ),
            line(json!({ "jsonrpc": "2.0", "id": 1003, "result": allow() })),
            line(json!({ "jsonrpc": "2.0", "id": 1004, "result": {} })),
        ];
        let (replies, result) = exchange(&input.concat());
        result.unwrap();
        assert_eq!(replies.last().unwrap()["result"]["stopReason"], "end_turn");
        let read = of_method(&replies, "fs/read_text_file");
        assert_eq!(read.last().unwrap()["params"]["path"], journal);
        let call = tool_call(&replies, "mock-appendjournal");
        assert_eq!(call["kind"], "edit");
        assert_eq!(call["content"][0]["oldText"], "# 工作日志\n\n- a\n");
        let write = of_method(&replies, "fs/write_text_file")[1];
        assert_eq!(write["params"]["path"], journal);
        let content = write["params"]["content"].as_str().unwrap();
        let added = content.strip_prefix("# 工作日志\n\n- a\n").unwrap();
        assert!(
            added.starts_with("- ") && added.ends_with(" · FE-001 · Mock 进展：已写入过程文件\n"),
            "{added}"
        );
        assert_eq!(added.lines().count(), 1);
        assert!(all_text(&replies).contains("[appendjournal:ok]"));
    }

    #[test]
    fn fsprobe_reads_outside_then_writes_inside_outside_and_journal() {
        let replies = with_marker(
            "[mock:fsprobe=/tmp/secret.txt]",
            vec![
                (
                    1002,
                    Err("读取执行目录与岗位文件夹之外的文件：/tmp/secret.txt"),
                ),
                (1003, Ok(json!({}))),
                (1004, Err("Teamspace 未允许这次写入：a")),
                (1005, Err("Teamspace 未允许这次写入：b")),
            ],
        );
        let read = of_method(&replies, "fs/read_text_file")[0];
        assert_eq!(read["params"]["path"], "/tmp/secret.txt");
        let paths: Vec<&str> = of_method(&replies, "fs/write_text_file")[1..]
            .iter()
            .map(|w| w["params"]["path"].as_str().unwrap())
            .collect();
        assert_eq!(
            paths,
            [
                "/tmp/ws/Teamspace/前端/过程文件/fsprobe-ok.md",
                "/tmp/secret.txt",
                "/tmp/ws/Teamspace/前端/工作日志.md"
            ]
        );
        assert_eq!(
            of_method(&replies, "session/request_permission").len(),
            1,
            "不申请权限"
        );
        let text = all_text(&replies);
        assert!(
            text.contains("[fsprobe-read:err:读取执行目录与岗位文件夹之外的文件"),
            "{text}"
        );
        assert!(text.contains("[fsprobe-write-in:ok]"));
        assert!(text.contains("[fsprobe-write-out:err:Teamspace 未允许这次写入"));
        assert!(text.contains("[fsprobe-write-journal:err:"));
    }

    #[test]
    fn fsprobe_without_argument_uses_a_sibling_of_the_workdir() {
        let replies = with_marker(
            "[mock:fsprobe]",
            vec![
                (1002, Ok(json!({ "content": "x" }))),
                (1003, Ok(json!({}))),
                (1004, Ok(json!({}))),
                (1005, Ok(json!({}))),
            ],
        );
        assert_eq!(
            of_method(&replies, "fs/read_text_file")[0]["params"]["path"],
            "/tmp/fsprobe-outside.txt"
        );
    }

    #[test]
    fn linger_keeps_the_run_going_until_cancelled() {
        let input = [
            format!("{NEW}\n"),
            prompt(2, &format!("{BRIEF}\n[mock:linger]")),
            line(json!({ "jsonrpc": "2.0", "id": 1000, "result": allow() })),
            line(json!({ "jsonrpc": "2.0", "id": 1001, "result": {} })),
            line(json!({ "jsonrpc": "2.0", "method": "session/cancel", "params": { "sessionId": "mock-session-1" } })),
        ]
        .concat();
        let (replies, result) = exchange(&input);
        result.unwrap();
        assert_eq!(replies.last().unwrap()["result"]["stopReason"], "cancelled");
        assert!(all_text(&replies).contains("等待收尾"));
        assert!(!all_text(&replies).contains("已完成：在"), "不应给出总结");
        // 没有取消就一直等：输入结束时安静退出，不回复提示。
        let input = [
            format!("{NEW}\n"),
            prompt(2, &format!("{BRIEF}\n[mock:linger]")),
            line(json!({ "jsonrpc": "2.0", "id": 1000, "result": allow() })),
            line(json!({ "jsonrpc": "2.0", "id": 1001, "result": {} })),
        ]
        .concat();
        let (replies, result) = exchange(&input);
        result.unwrap();
        assert!(replies.iter().all(|r| r["id"] != 2));
    }

    #[test]
    fn failafter_writes_first_and_then_fails_the_prompt() {
        let input = [
            format!("{NEW}\n"),
            prompt(2, &format!("{BRIEF}\n[mock:failafter]")),
            line(json!({ "jsonrpc": "2.0", "id": 1000, "result": allow() })),
            line(json!({ "jsonrpc": "2.0", "id": 1001, "result": {} })),
        ]
        .concat();
        let (replies, result) = exchange(&input);
        result.unwrap();
        let writes = of_method(&replies, "fs/write_text_file");
        assert_eq!(writes.len(), 1, "过程文件已经写了");
        let last = replies.last().unwrap();
        assert_eq!(last["id"], 2);
        assert_eq!(last["error"]["code"], -32603);
        assert!(all_text(&replies).contains("写完文件后出错了"));
        assert!(!all_text(&replies).contains("已完成：在"));
    }

    #[test]
    fn acp_005_session_ids_and_marker_results_have_the_documented_fields() {
        let prompt_for = |id: u64, session: &str, text: &str| {
            line(
                json!({ "jsonrpc": "2.0", "id": id, "method": "session/prompt",
                "params": { "sessionId": session, "prompt": [{ "type": "text", "text": text }] } }),
            )
        };
        let new = |id: u64| {
            line(json!({ "jsonrpc": "2.0", "id": id, "method": "session/new",
                "params": { "cwd": "/tmp/ws", "mcpServers": [] } }))
        };
        let input = [
            new(1),
            new(2),
            prompt_for(3, "mock-session-2", "[mock:refuse]"),
            prompt_for(4, "mock-session-2", "[mock:auth]"),
            prompt_for(5, "mock-session-2", "[mock:fail]"),
        ]
        .concat();
        let (replies, result) = exchange(&input);
        result.unwrap();
        let by_id = |id: u64| {
            replies
                .iter()
                .find(|r| r["id"] == id && r.get("method").is_none())
                .unwrap()
        };
        assert_eq!(by_id(1)["result"]["sessionId"], "mock-session-1");
        assert_eq!(by_id(2)["result"]["sessionId"], "mock-session-2");
        assert_eq!(by_id(3)["result"], json!({ "stopReason": "refusal" }));
        let auth = by_id(4);
        assert_eq!(auth["error"]["code"], -32000);
        assert_eq!(auth["error"]["data"]["authMethods"][0]["id"], "mock-login");
        let fail = by_id(5);
        assert_eq!(fail["error"]["code"], -32603);
        assert_eq!(fail["error"]["message"], "Mock 模拟的内部错误");
        // 提示用的是会话 2：更新通知都带着这个会话 ID
        let sessions: Vec<&str> = replies
            .iter()
            .filter(|r| r["method"] == "session/update")
            .filter_map(|r| r.pointer("/params/sessionId").and_then(Value::as_str))
            .collect();
        assert!(!sessions.is_empty() && sessions.iter().all(|s| *s == "mock-session-2"));
    }

    #[test]
    fn marker_arg_reads_the_parameter() {
        assert_eq!(
            marker_arg("a [mock:readgrant=read|x y] b", "readgrant"),
            Some("read|x y")
        );
        assert_eq!(marker_arg("[mock:readgrant]", "readgrant"), None);
        assert_eq!(marker_arg("[mock:readgrant=oops", "readgrant"), None);
    }
}
