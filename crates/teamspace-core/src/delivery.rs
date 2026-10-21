//! 岗位消息的投递引擎（M5-01）。
//!
//! 消息由 Teamspace 中转：发给某岗位的消息交给该岗位绑定的 Harness 处理——复用执行引擎的 ACP 会话
//! 能力，以「轻量执行」方式开一个会话：说明包包含岗位说明、线程上下文（最近若干条）、关联任务摘要与
//! 本条消息正文，要求 Agent 只回答、不修改文件。权限请求一律按只读策略处理（读取工作空间内允许，
//! 写入与命令执行拒绝）；拒绝时 Agent 仍应给出文字回复。Agent 的最终文字作为回复写回消息，状态转
//! 「已回复」。
//!
//! 排队与并发：同一岗位同时只投递一条，其余排队；不同岗位可以并行。应用重启时未完成的投递回到排队。
//! 岗位未绑定 Harness、握手失败或 Agent 出错时消息转「失败」并记下原因。

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex, MutexGuard, mpsc};
use std::thread;
use std::time::{Duration, Instant};

use serde_json::{Value, json};

use crate::acp::connection::{AgentProcess, Incoming, RpcFailure, SpawnSpec};
use crate::acp::{self, METHOD_NOT_FOUND_CODE};
use crate::api::Core;
use crate::model::{AcpPreset, HandshakeStatus, Message, MessageSender, RunIsolation};
use crate::role_folder;
use crate::run::env_profile;
use crate::run::policy::Scope;
use crate::service::{harness, message as message_svc, role, task, team};

/// 启动阶段的超时（`initialize`、`session/new`）。
const STARTUP_TIMEOUT: Duration = Duration::from_secs(90);
/// 等待 Agent 给出最终回复的超时。
const REPLY_TIMEOUT: Duration = Duration::from_secs(10 * 60);
/// 队列为空时投递线程的等待时长（到期后做一次过期检查）。
const IDLE_WAIT: Duration = Duration::from_secs(60);
/// 目标岗位正在投递、其余消息排队时的重查间隔。
const BUSY_WAIT: Duration = Duration::from_millis(200);
/// 过期检查的间隔。
const EXPIRY_INTERVAL: Duration = Duration::from_secs(60);

/// 投递引擎状态（属于 [`Core`]）。
#[derive(Default)]
pub struct Deliveries {
    /// 正在投递的岗位：同一岗位同时只投递一条。
    busy: Mutex<HashSet<String>>,
    /// 新消息入队时唤醒投递线程。
    wake: Mutex<bool>,
    wake_cv: Condvar,
    /// 投递线程是否已启动。
    started: AtomicBool,
    /// 是否已停止（应用退出）。
    stopped: AtomicBool,
}

impl Deliveries {
    /// 唤醒投递线程。
    pub(crate) fn schedule(&self) {
        let mut wake = lock(&self.wake);
        *wake = true;
        self.wake_cv.notify_all();
    }

    /// 停止投递线程（应用退出时调用）。
    pub(crate) fn stop(&self) {
        self.stopped.store(true, Ordering::SeqCst);
        self.schedule();
    }

    /// 正在投递的岗位 id。
    fn busy_roles(&self) -> Vec<String> {
        lock(&self.busy).iter().cloned().collect()
    }

    /// 占用一个岗位（同岗位已有投递在跑时不占用）。
    fn acquire(&self, role_id: &str) -> bool {
        let mut busy = lock(&self.busy);
        if busy.contains(role_id) {
            return false;
        }
        busy.insert(role_id.to_owned());
        true
    }

    /// 释放岗位。
    fn release(&self, role_id: &str) {
        lock(&self.busy).remove(role_id);
    }

    /// 等待新消息或超时。
    fn wait(&self, timeout: Duration) {
        let wake = lock(&self.wake);
        let _ = self.wake_cv.wait_timeout(wake, timeout);
    }
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// 启动投递线程：反复从队列里取消息，交给目标岗位的 Harness 处理。
pub(crate) fn start(core: Arc<Core>) {
    if core.deliveries.started.swap(true, Ordering::SeqCst) {
        return;
    }
    thread::spawn(move || {
        let mut last_expiry = Instant::now();
        loop {
            if core.deliveries.stopped.load(Ordering::SeqCst) {
                break;
            }
            if last_expiry.elapsed() >= EXPIRY_INTERVAL {
                expire_stale(&core);
                last_expiry = Instant::now();
            }
            let picked = {
                let conn = core.lock();
                let busy = core.deliveries.busy_roles();
                message_svc::next_queued(&conn, &busy)
            };
            match picked {
                Ok(Some(message)) => {
                    if !core.deliveries.acquire(&message.to_role_id) {
                        continue;
                    }
                    let worker = Arc::clone(&core);
                    let role_id = message.to_role_id.clone();
                    thread::spawn(move || {
                        deliver(&worker, &message);
                        worker.deliveries.release(&role_id);
                    });
                }
                Ok(None) => {
                    // 队列里可能还有消息，只是目标岗位正在投递：稍后再看。
                    let pending = {
                        let conn = core.lock();
                        message_svc::next_queued(&conn, &[])
                            .ok()
                            .flatten()
                            .is_some()
                    };
                    if pending {
                        core.deliveries.wait(BUSY_WAIT);
                    } else {
                        core.deliveries.wait(IDLE_WAIT);
                    }
                }
                Err(err) => {
                    eprintln!("teamspace delivery: {err}");
                    core.deliveries.wait(BUSY_WAIT);
                }
            }
        }
    });
}

/// 把超过期限仍未回复的消息转「过期」。
pub(crate) fn expire_stale(core: &Core) {
    let cutoff = expiry_cutoff();
    if let Err(err) = core.write(|tx| message_svc::expire_stale(tx, &cutoff)) {
        eprintln!("teamspace delivery: 过期检查失败：{err}");
    }
}

/// 过期期限：现在往前推 7 天。
fn expiry_cutoff() -> String {
    let now = time::OffsetDateTime::now_utc();
    let cutoff = now - time::Duration::days(message_svc::EXPIRY_DAYS);
    cutoff
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap_or_else(|_| {
            now.format(&time::format_description::well_known::Rfc3339)
                .unwrap_or_default()
        })
}

/// 投递一条消息：成功写回回复，失败记下原因。
fn deliver(core: &Arc<Core>, message: &Message) {
    let context = match load_context(core, message) {
        Ok(context) => context,
        Err(reason) => {
            fail(core, message, &reason);
            return;
        }
    };
    // 状态转「投递中」。
    if let Err(err) = core.write(|tx| message_svc::mark_delivering(tx, &message.id)) {
        eprintln!("teamspace delivery {}: {err}", message.id);
        return;
    }
    match run_session(core, &context, message) {
        Ok(reply) => {
            if let Err(err) = core.write(|tx| message_svc::mark_replied(tx, &message.id, &reply)) {
                eprintln!("teamspace delivery {}: {err}", message.id);
            }
        }
        Err(reason) => fail(core, message, &reason),
    }
}

/// 投递失败：状态转「失败」并记下原因。
fn fail(core: &Arc<Core>, message: &Message, reason: &str) {
    if let Err(err) = core.write(|tx| message_svc::mark_failed(tx, &message.id, reason)) {
        eprintln!("teamspace delivery {}: {err}", message.id);
    }
}

/// 投递所需的信息：接收岗位、它绑定的 Harness、工作空间与岗位文件夹。
struct Context {
    role_name: String,
    role_description: String,
    role_guidelines: String,
    harness_preset: AcpPreset,
    harness_command: String,
    harness_args: Vec<String>,
    env_profile: Option<String>,
    workspace: PathBuf,
    /// 岗位文件夹（绝对路径）；还没有时为 `None`。
    role_folder: Option<PathBuf>,
}

/// 取出投递上下文；任何一步不满足都给出中文失败原因。
fn load_context(core: &Arc<Core>, message: &Message) -> std::result::Result<Context, String> {
    let conn = core.lock();
    let team = team::get_active(&conn, &message.team_id)
        .map_err(|err| format!("消息所属团队不可用：{err}"))?;
    let role = role::get_active(&conn, &message.to_role_id)
        .map_err(|_| "接收岗位不存在或已归档".to_owned())?;
    let Some(harness_id) = role.harness_id.clone() else {
        return Err(format!("岗位「{}」未绑定 Harness，无法接收消息", role.name));
    };
    let instance = harness::get_active(&conn, &harness_id)
        .map_err(|_| format!("岗位「{}」绑定的 Harness 已归档或不存在", role.name))?;
    let folder: Option<String> = conn
        .query_row(
            "SELECT folder FROM roles WHERE id = ?1",
            rusqlite::params![role.id],
            |row| row.get::<_, Option<String>>(0),
        )
        .ok()
        .flatten();
    let workspace = PathBuf::from(&team.workspace_path);
    Ok(Context {
        role_folder: folder
            .as_deref()
            .map(|rel| role_folder::absolute(&workspace, rel))
            .filter(|path| path.is_dir()),
        role_name: role.name.clone(),
        role_description: role.description.clone(),
        role_guidelines: role.guidelines.clone(),
        harness_preset: instance.preset,
        harness_command: instance.command.clone(),
        harness_args: instance.args.clone(),
        env_profile: instance.env_profile.clone(),
        workspace,
    })
}

/// 开一个 ACP 会话，把消息交给 Agent，收回最终文字。
fn run_session(
    core: &Arc<Core>,
    context: &Context,
    message: &Message,
) -> std::result::Result<String, String> {
    let spec = spawn_spec(core, context)?;
    // 会话的工作目录：优先用岗位文件夹（Agent 能直接看到自己的岗位说明与工作日志），
    // 岗位文件夹还没有时用工作空间根目录。
    let cwd = context
        .role_folder
        .clone()
        .unwrap_or_else(|| context.workspace.clone());
    if !cwd.is_dir() {
        return Err(format!("工作目录不存在：{}", cwd.display()));
    }
    let spec = SpawnSpec {
        program: spec.program,
        args: spec.args,
        cwd: cwd.clone(),
        env: spec.env,
    };

    // 权限策略：只读。读取工作空间内允许，写入与命令执行一律拒绝。
    let scope = Scope::with_workspace(&cwd, &context.workspace, RunIsolation::InPlace, &[], &[]);

    let (tx, rx) = mpsc::channel::<Incoming>();
    let process = Arc::new(
        AgentProcess::spawn(&spec, move |incoming| {
            let _ = tx.send(incoming);
        })
        .map_err(|err| {
            format!(
                "无法启动 {}：{err}",
                acp::command_line(&spec.program, &spec.args)
            )
        })?,
    );

    // 1. initialize
    let result = request(&process, "initialize", acp::initialize_params(true))
        .map_err(|err| format!("Agent 握手失败：{err}"))?;
    let handshake = acp::interpret_response(&json!({ "result": result }));
    if handshake.status != HandshakeStatus::Connected {
        return Err(handshake
            .detail
            .unwrap_or_else(|| "Agent 的 initialize 响应无效".to_owned()));
    }

    // 2. session/new
    let response = request(
        &process,
        "session/new",
        json!({ "cwd": cwd.to_string_lossy(), "mcpServers": [] }),
    )
    .map_err(|err| format!("无法建立会话：{err}"))?;
    let session = response
        .get("sessionId")
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
        .ok_or_else(|| "Agent 的 session/new 响应缺少 sessionId".to_owned())?
        .to_owned();

    // 3. 让 Agent 在需要时向 Teamspace 请求权限（切换不成不阻断：下面的只读策略已经够严）。
    switch_to_ask_mode(&process, &session, &response);

    // 4. 说明包
    let text = brief(core, context, message)?;
    let params = json!({ "sessionId": session, "prompt": [acp_text_block(&text)] });

    // 5. 发提示，等最终回复。Agent 的文字通过通知流过来，最后的结果通过提示的响应回来。
    let (reply_tx, reply_rx) = mpsc::channel::<std::result::Result<Value, RpcFailure>>();
    process.conn.request_with(
        "session/prompt",
        params,
        Box::new(move |result| {
            let _ = reply_tx.send(result);
        }),
    );

    // 收集 Agent 文字的通道：读线程把通知里的文字攒起来。
    let agent_text = Arc::new(Mutex::new(String::new()));
    let reader = {
        let agent_text = Arc::clone(&agent_text);
        let scope = scope.clone();
        let process = Arc::clone(&process);
        thread::spawn(move || {
            while let Ok(incoming) = rx.recv() {
                match incoming {
                    Incoming::Request { id, method, params } => {
                        handle_request(&process, &scope, &id, &method, &params);
                    }
                    Incoming::Notification { method, params } => {
                        collect_text(&method, &params, &agent_text);
                    }
                    Incoming::Malformed(_) => {}
                    Incoming::Closed => break,
                }
            }
        })
    };

    let outcome = reply_rx.recv_timeout(REPLY_TIMEOUT);
    // 收尾：终止 Agent 进程（stdout 关闭后读线程结束），再取回复正文。
    process.kill();
    let _ = reader.join();

    match outcome {
        Ok(Ok(value)) => {
            let reason = value
                .get("stopReason")
                .and_then(Value::as_str)
                .unwrap_or_default();
            if reason == "refusal" {
                return Err("Agent 拒绝处理这条消息".to_owned());
            }
            let text = lock(&agent_text).trim().to_owned();
            if text.is_empty() {
                Ok("（Agent 没有给出文字回复）".to_owned())
            } else {
                Ok(text)
            }
        }
        Ok(Err(failure)) => Err(format!("Agent 出错：{}", failure.describe())),
        Err(mpsc::RecvTimeoutError::Timeout) => Err("Agent 未在期限内给出回复".to_owned()),
        Err(mpsc::RecvTimeoutError::Disconnected) => Err("Agent 进程已退出".to_owned()),
    }
}

/// 处理 Agent 发来的请求：只读策略下允许读取，拒绝写入与命令执行。
fn handle_request(
    process: &Arc<AgentProcess>,
    scope: &Scope,
    id: &Value,
    method: &str,
    params: &Value,
) {
    match method {
        "session/request_permission" => {
            // 只读策略：读取工作空间内的文件可以放行；写入、删除、移动与命令执行一律拒绝。
            // 拒绝时 Agent 仍应继续给出文字回复。
            let tool = params.get("toolCall");
            let kind = tool
                .and_then(|t| t.get("kind"))
                .and_then(Value::as_str)
                .unwrap_or_default();
            let path = tool
                .and_then(|t| t.get("path"))
                .and_then(Value::as_str)
                .or_else(|| {
                    tool.and_then(|t| t.pointer("/locations/0/path"))
                        .and_then(Value::as_str)
                })
                .unwrap_or_default();
            let readable = matches!(kind, "read" | "search")
                && !path.is_empty()
                && scope.can_read(path).is_ok();
            let wanted = if readable { "allow" } else { "reject" };
            let option = params
                .get("options")
                .and_then(Value::as_array)
                .and_then(|options| {
                    options
                        .iter()
                        .find(|o| {
                            o.get("optionId")
                                .and_then(Value::as_str)
                                .is_some_and(|id| id.starts_with(wanted))
                        })
                        .and_then(|o| o.get("optionId").and_then(Value::as_str))
                        .map(str::to_owned)
                });
            let reply = match option {
                Some(option_id) => {
                    json!({ "outcome": { "outcome": "selected", "optionId": option_id } })
                }
                None => json!({ "outcome": { "outcome": "cancelled" } }),
            };
            let _ = process.conn.respond(id, reply);
        }
        "fs/read_text_file" => {
            let path = params
                .get("path")
                .and_then(Value::as_str)
                .unwrap_or_default();
            match scope.can_read(path) {
                Ok(real) => match std::fs::read_to_string(&real) {
                    Ok(content) => {
                        let _ = process.conn.respond(id, json!({ "content": content }));
                    }
                    Err(err) => {
                        let _ =
                            process
                                .conn
                                .respond_error(id, -32603, &format!("无法读取文件：{err}"));
                    }
                },
                Err(reason) => {
                    let _ = process.conn.respond_error(id, -32603, &reason);
                }
            }
        }
        "fs/write_text_file" => {
            let _ = process.conn.respond_error(
                id,
                -32603,
                "消息回复只允许读取工作空间内的文件，不能写入",
            );
        }
        other => {
            let _ = process.conn.respond_error(
                id,
                METHOD_NOT_FOUND_CODE,
                &format!("Teamspace 不提供 {other}"),
            );
        }
    }
}

/// 从 `session/update` 通知里收集 Agent 的文字。
fn collect_text(method: &str, params: &Value, out: &Arc<Mutex<String>>) {
    if method != "session/update" {
        return;
    }
    let Some(update) = params.get("update") else {
        return;
    };
    if update.get("sessionUpdate").and_then(Value::as_str) != Some("agent_message_chunk") {
        return;
    }
    if let Some(text) = update.pointer("/content/text").and_then(Value::as_str) {
        lock(out).push_str(text);
    }
}

/// 让 Agent 在需要时向 Teamspace 请求权限（切换失败不阻断：只读策略已经够严）。
fn switch_to_ask_mode(process: &Arc<AgentProcess>, session: &str, response: &Value) {
    let plan = crate::acp::permission_mode::plan(response, false);
    for switch in plan.switches {
        let (method, params) = match &switch {
            crate::acp::permission_mode::Switch::Mode { mode_id } => (
                "session/set_mode",
                json!({ "sessionId": session, "modeId": mode_id }),
            ),
            crate::acp::permission_mode::Switch::Config { config_id, value } => (
                "session/set_config_option",
                json!({ "sessionId": session, "configId": config_id, "value": value }),
            ),
        };
        let _ = process
            .conn
            .request(method, params, STARTUP_TIMEOUT, &|| false);
    }
}

/// 启动 Agent 所需的命令与环境。
fn spawn_spec(core: &Arc<Core>, context: &Context) -> std::result::Result<SpawnSpec, String> {
    let launch = core
        .launch_for(
            context.harness_preset,
            &context.harness_command,
            &context.harness_args,
            context.env_profile.as_deref(),
        )
        .map_err(|err| err.to_string())?;
    let command = launch.command.trim();
    let program = if command.contains('/') {
        let path = PathBuf::from(command);
        if !path.is_file() {
            return Err(format!("启动命令不存在或不可执行：{command}"));
        }
        path
    } else {
        core.probe
            .locate_command(command)
            .map(PathBuf::from)
            .ok_or_else(|| format!("未找到命令 {command}：请确认已安装并加入 PATH"))?
    };
    let mut env = vec![("PATH".into(), core.probe.command_path_env())];
    if let Some(profile) = launch.env_profile.as_deref() {
        let vars = env_profile::load(Path::new(profile)).map_err(|err| err.to_string())?;
        env.extend(vars.into_iter().map(|(k, v)| (k.into(), v.into())));
    }
    Ok(SpawnSpec {
        program,
        args: launch.args,
        cwd: context.workspace.clone(),
        env,
    })
}

/// 发起一个阻塞请求。
fn request(
    process: &Arc<AgentProcess>,
    method: &str,
    params: Value,
) -> std::result::Result<Value, String> {
    process
        .conn
        .request(method, params, STARTUP_TIMEOUT, &|| false)
        .map_err(|failure| failure.describe())
}

/// 说明包：岗位说明、线程上下文、关联任务摘要与本条消息正文。
fn brief(
    core: &Arc<Core>,
    context: &Context,
    message: &Message,
) -> std::result::Result<String, String> {
    let (subject, recent, task_summary) = {
        let conn = core.lock();
        let subject = message_svc::thread(&conn, &message.thread_id)
            .map(|detail| detail.thread.subject)
            .unwrap_or_default();
        let recent = message_svc::context(&conn, &message.thread_id, message_svc::CONTEXT_MESSAGES)
            .unwrap_or_default();
        let task_summary = match message.task_id.as_deref() {
            Some(task_id) => task::get(&conn, task_id).ok().map(|t| {
                let owner = role::get_active(&conn, &t.owner_role_id)
                    .map(|r| r.name)
                    .unwrap_or_default();
                (t.key, t.title, t.status.label().to_owned(), owner)
            }),
            None => None,
        };
        (subject, recent, task_summary)
    };

    let mut out = String::new();
    let mut push = |line: &str| {
        out.push_str(line);
        out.push('\n');
    };
    push("# 岗位消息（由 Teamspace 生成）");
    push("");
    push(&format!(
        "你是团队中的「{}」岗位，Teamspace 给你发来一条消息。",
        context.role_name
    ));
    push("");
    push("## 岗位职责");
    push(if context.role_description.trim().is_empty() {
        "（未填写）"
    } else {
        context.role_description.trim()
    });
    push("");
    push("## 工作约定");
    push(if context.role_guidelines.trim().is_empty() {
        "（未填写）"
    } else {
        context.role_guidelines.trim()
    });
    push("");
    push(&format!("## 线程主题\n{subject}"));
    push("");
    push("## 线程上下文（最近的往来）");
    if recent.is_empty() {
        push("（暂无）");
    }
    for item in &recent {
        let who = match item.sender {
            MessageSender::Human => "Human",
            MessageSender::Leader => "Leader",
            MessageSender::Role => "岗位",
        };
        let mut line = format!("- {who}：{}", item.body.replace('\n', " "));
        if let Some(reply) = &item.reply_body {
            line.push_str(&format!("（已回复：{}）", reply.replace('\n', " ")));
        }
        push(&line);
    }
    push("");
    push("## 关联任务");
    match &task_summary {
        Some((key, title, status, owner)) => {
            push(&format!("- 编号：{key}"));
            push(&format!("- 标题：{title}"));
            push(&format!("- 状态：{status}"));
            push(&format!("- 负责岗位：{owner}"));
        }
        None => push("（无）"),
    }
    push("");
    push("## 消息");
    push(&message.body);
    push("");
    push("## 要求");
    push(
        "只回答这条消息，不要修改任何文件，也不要执行命令。用文字回复，回复内容会作为本条消息的回复写回 Teamspace。",
    );
    Ok(out)
}

/// 说明包的内容块：只发文本。
fn acp_text_block(text: &str) -> Value {
    json!({ "type": "text", "text": text, "annotations": { "audience": ["user"] } })
}
