//! 执行引擎（M2，D13 / D15）：把任务派给负责岗位绑定的 ACP Agent，驱动一次会话直到结束。
//!
//! 每个活动执行由一个后台线程（driver）驱动：
//! 1. 准备执行目录：Git 工作空间新建 worktree（[`worktree`]），普通文件夹原地执行；
//! 2. 启动 Agent 子进程（独立进程组，环境 = 继承环境 + 补全的 PATH + 环境配置档案），`initialize`
//!    声明 Client 提供文件读写、不提供终端；
//! 3. `session/new`（cwd = 执行目录、无 MCP 服务器）或续接时 `session/load`（丢弃回放的历史）；
//! 4. 发送任务说明包（[`brief`]）作为第一个 `session/prompt`；
//! 5. 处理 Agent 的消息：`session/update` 写入执行进度（消息块合并、计划、工具调用状态）；
//!    `session/request_permission` 按权限规则（[`policy`]）自动允许或转交 Human；`fs/read_text_file`、
//!    `fs/write_text_file` 限定在执行范围内；其他请求回复「方法不存在」；
//! 6. 结束：根据 `stopReason` 或失败原因写入结果、联动任务状态、写活动（进而写入岗位工作日志），
//!    终止 Agent 进程组。
//!
//! 取消：发 `session/cancel`、撤销待确认的权限请求，宽限期内 Agent 未结束则整组终止。
//! 超时：Agent 连续 [`IDLE_TIMEOUT`]（可配置）没有任何输出（等待 Human 确认期间不计）视为超时。
//! 应用退出：整组终止，执行标记为已中断；下次打开时，残留的活动执行同样标记为已中断。
//!
//! 事件：每次执行记录或执行进度变化后（事务提交后）通过 [`Runs::set_listener`] 设置的回调推送
//! [`RunEvent`]。

#[cfg(test)]
mod fencing_tests;

pub mod brief;
pub mod env_profile;
pub mod policy;
pub mod worktree;

use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::sync::{Arc, Condvar, Mutex, RwLock};
use std::thread;
use std::time::{Duration, Instant};

use serde_json::{Value, json};

use crate::acp::connection::{AgentProcess, Incoming, RpcFailure, SpawnSpec};
use crate::acp::{self, AUTH_REQUIRED_CODE, METHOD_NOT_FOUND_CODE};
use crate::api::Core;
use crate::error::{Error, Result};
use crate::model::{
    AcpCapabilities, Actor, ChangedFile, DiffStat, EventKind, FailureKind, FileChangeKind,
    Handshake, HandshakeStatus, MessageRole, PermissionDecision, PermissionOption,
    PermissionPrompt, PlanEntry, Run, RunEvent, RunIsolation, RunStatus, RunUpdate, RunUpdateBody,
    RunUpdateQuery, TaskStatus,
};
use crate::role_folder;
use crate::service::run::{self as store_run, Ending};
use crate::service::{MAX_REASON, harness, task};
use crate::store::{new_id, now};
use policy::{Scope, Verdict};

/// Agent 连续没有任何输出视为超时的默认时长。
pub const IDLE_TIMEOUT: Duration = Duration::from_secs(30 * 60);
/// 启动阶段（`initialize`、`session/new`、`session/load`）每个请求的超时。
pub const STARTUP_TIMEOUT: Duration = Duration::from_secs(90);
/// 取消后等待 Agent 自己结束的宽限时间，超过后整组终止。
pub const CANCEL_GRACE: Duration = Duration::from_secs(10);
/// Agent 退出后等待取得退出码的时间。
const EXIT_WAIT: Duration = Duration::from_secs(2);
/// 等待 driver 处理 Human 权限决定的时间。
const REPLY_WAIT: Duration = Duration::from_secs(10);
/// JSON-RPC「内部错误」，用于拒绝文件读写。
const INTERNAL_ERROR_CODE: i64 = -32603;

/// 执行引擎的超时设置。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunSettings {
    pub idle_timeout: Duration,
    pub startup_timeout: Duration,
    pub cancel_grace: Duration,
}

impl Default for RunSettings {
    fn default() -> Self {
        Self {
            idle_timeout: IDLE_TIMEOUT,
            startup_timeout: STARTUP_TIMEOUT,
            cancel_grace: CANCEL_GRACE,
        }
    }
}

/// 执行事件回调。
pub type RunListener = Arc<dyn Fn(&RunEvent) + Send + Sync>;

/// 执行引擎状态（属于 [`Core`]）。
#[derive(Default)]
pub struct Runs {
    active: Mutex<HashMap<String, Arc<ActiveRun>>>,
    settings: RwLock<RunSettings>,
    listener: RwLock<Option<RunListener>>,
}

impl Runs {
    pub(crate) fn set_listener(&self, listener: RunListener) {
        *self.listener.write().unwrap_or_else(|p| p.into_inner()) = Some(listener);
    }

    pub(crate) fn set_settings(&self, settings: RunSettings) {
        *self.settings.write().unwrap_or_else(|p| p.into_inner()) = settings;
    }

    fn settings(&self) -> RunSettings {
        self.settings
            .read()
            .unwrap_or_else(|p| p.into_inner())
            .clone()
    }

    pub(crate) fn registry(&self) -> std::sync::MutexGuard<'_, HashMap<String, Arc<ActiveRun>>> {
        self.active.lock().unwrap_or_else(|p| p.into_inner())
    }

    fn get(&self, run_id: &str) -> Option<Arc<ActiveRun>> {
        self.registry().get(run_id).cloned()
    }
}

/// 停止请求。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub(crate) enum Stop {
    None = 0,
    Cancel = 1,
    Takeover = 2,
    Shutdown = 3,
}

enum Msg {
    Agent(Incoming),
    PromptDone(std::result::Result<Value, RpcFailure>),
    Permission {
        request_id: String,
        option_id: String,
        reply: Sender<Result<()>>,
    },
    Stop,
}

/// 一个活动执行的控制句柄。
pub(crate) struct ActiveRun {
    tx: Mutex<Sender<Msg>>,
    stop: AtomicU8,
    process: Mutex<Option<Arc<AgentProcess>>>,
    done: Mutex<bool>,
    done_cv: Condvar,
}

impl ActiveRun {
    fn stop(&self) -> Stop {
        match self.stop.load(Ordering::SeqCst) {
            1 => Stop::Cancel,
            2 => Stop::Takeover,
            3 => Stop::Shutdown,
            _ => Stop::None,
        }
    }

    /// 请求停止（先到的请求生效，应用退出总是生效）。
    fn request_stop(&self, stop: Stop) {
        if stop == Stop::Shutdown {
            self.stop.store(stop as u8, Ordering::SeqCst);
        } else {
            let _ = self.stop.compare_exchange(
                Stop::None as u8,
                stop as u8,
                Ordering::SeqCst,
                Ordering::SeqCst,
            );
        }
        let _ = self.send(Msg::Stop);
    }

    fn send(&self, msg: Msg) -> bool {
        self.tx
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .send(msg)
            .is_ok()
    }

    fn kill(&self) {
        if let Some(process) = self
            .process
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .as_ref()
        {
            process.kill();
        }
    }

    fn wait_done(&self, timeout: Duration) -> bool {
        let guard = self.done.lock().unwrap_or_else(|p| p.into_inner());
        let (guard, _) = self
            .done_cv
            .wait_timeout_while(guard, timeout, |done| !*done)
            .unwrap_or_else(|p| p.into_inner());
        *guard
    }

    fn mark_done(&self) {
        *self.done.lock().unwrap_or_else(|p| p.into_inner()) = true;
        self.done_cv.notify_all();
    }
}

/// driver 的启动方式。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Mode {
    Start,
    Resume,
}

/// 中断时写入的原因。
pub(crate) const INTERRUPTED_RESTART: &str =
    "应用退出或重启时执行仍在进行，与 Agent 的会话已断开；已写入执行目录的改动仍然保留";

impl Core {
    /// 推送执行事件。
    pub(crate) fn emit(&self, event: RunEvent) {
        let listener = self
            .runs
            .listener
            .read()
            .unwrap_or_else(|p| p.into_inner())
            .clone();
        if let Some(listener) = listener {
            listener(&event);
        }
    }

    pub(crate) fn emit_run(&self, run_id: &str) {
        if let Ok(run) = store_run::get(&self.lock(), run_id) {
            self.emit(RunEvent::RunChanged { run: Box::new(run) });
        }
    }

    /// 为已写入数据库的执行登记控制句柄并启动 driver 线程。调用方须持有执行登记表的锁（`registry`）。
    pub(crate) fn spawn_driver(
        self: &Arc<Self>,
        registry: &mut HashMap<String, Arc<ActiveRun>>,
        run: Run,
        mode: Mode,
    ) {
        let (tx, rx) = mpsc::channel();
        let active = Arc::new(ActiveRun {
            tx: Mutex::new(tx.clone()),
            stop: AtomicU8::new(Stop::None as u8),
            process: Mutex::new(None),
            done: Mutex::new(false),
            done_cv: Condvar::new(),
        });
        registry.insert(run.id.clone(), Arc::clone(&active));
        let core = Arc::clone(self);
        thread::spawn(move || {
            let run_id = run.id.clone();
            let mut driver = Driver::new(Arc::clone(&core), Arc::clone(&active), run, tx, rx);
            let end = driver.execute(mode);
            driver.finish(end);
            core.runs.registry().remove(&run_id);
            active.mark_done();
        });
    }

    /// 请求停止活动执行并等待它结束。执行不在活动中时返回 `false`。
    pub(crate) fn stop_run(&self, run_id: &str, stop: Stop) -> bool {
        let Some(active) = self.runs.get(run_id) else {
            return false;
        };
        active.request_stop(stop);
        let grace = self.runs.settings().cancel_grace;
        if !active.wait_done(grace + Duration::from_secs(5)) {
            active.kill();
            active.wait_done(Duration::from_secs(5));
        }
        true
    }

    /// 把 Human 的权限决定交给 driver 处理。
    pub(crate) fn decide_permission(
        &self,
        run_id: &str,
        request_id: &str,
        option_id: &str,
    ) -> Result<()> {
        let Some(active) = self.runs.get(run_id) else {
            let run = store_run::get(&self.lock(), run_id)?;
            return Err(Error::conflict(format!(
                "执行已结束（{}），权限请求已失效",
                run.status.label()
            )));
        };
        let (reply, rx) = mpsc::channel();
        if !active.send(Msg::Permission {
            request_id: request_id.to_owned(),
            option_id: option_id.to_owned(),
            reply,
        }) {
            return Err(Error::conflict("执行已结束，权限请求已失效"));
        }
        rx.recv_timeout(REPLY_WAIT)
            .map_err(|_| Error::conflict("执行已结束，权限请求已失效"))?
    }

    /// 应用退出：终止所有 Agent 进程，执行标记为已中断（任务保持执行中）。
    pub(crate) fn shutdown_runs(&self) {
        let actives: Vec<(String, Arc<ActiveRun>)> = self
            .runs
            .registry()
            .iter()
            .map(|(id, a)| (id.clone(), Arc::clone(a)))
            .collect();
        for (_, active) in &actives {
            active.request_stop(Stop::Shutdown);
            active.kill();
        }
        for (id, _) in &actives {
            let _ = self.write(|tx| {
                interrupt(
                    tx,
                    id,
                    "应用退出时执行仍在进行，已中断；已写入执行目录的改动仍然保留",
                )
            });
        }
        for (_, active) in &actives {
            active.wait_done(Duration::from_secs(3));
        }
    }

    /// 打开数据库时：把残留的活动执行标记为已中断。
    pub(crate) fn recover_runs(&self) -> Result<()> {
        let stale = store_run::all_active(&self.lock())?;
        for run in &stale {
            self.write(|tx| interrupt(tx, &run.id, INTERRUPTED_RESTART))?;
        }
        Ok(())
    }

    /// 执行进度增量读取。
    pub(crate) fn run_updates(
        &self,
        query: &RunUpdateQuery,
    ) -> Result<crate::model::RunUpdatePage> {
        store_run::page_updates(&self.lock(), query)
    }
}

/// 把活动执行标记为已中断（只改状态与原因，保留其余记录）；不是活动执行时不做任何事。
fn interrupt(conn: &rusqlite::Connection, run_id: &str, reason: &str) -> Result<()> {
    let run = store_run::get(conn, run_id)?;
    if !run.status.is_active() {
        return Ok(());
    }
    conn.execute(
        "UPDATE runs SET status = 'interrupted', ended_at = ?2, failure = ?3, failure_kind = NULL,
                         pending_permission = NULL WHERE id = ?1",
        rusqlite::params![run_id, now(), reason],
    )?;
    store_run::status_update(
        conn,
        run_id,
        RunStatus::Interrupted,
        Some(reason.to_owned()),
    )?;
    store_run::record_event(
        conn,
        &run,
        Actor::System,
        EventKind::RunInterrupted,
        format!("{} 第 {} 次执行已中断：{reason}", run.task_key, run.attempt),
        json!({ "attempt": run.attempt, "runId": run.id }),
    )
}

/// driver 的结局。
#[derive(Debug, Clone, PartialEq, Eq)]
enum End {
    Succeeded(String),
    Failed {
        kind: FailureKind,
        failure: String,
        stop_reason: Option<String>,
    },
    Cancelled(Option<String>),
    TakenOver,
    Shutdown,
}

fn failed(kind: FailureKind, failure: impl Into<String>) -> End {
    End::Failed {
        kind,
        failure: failure.into(),
        stop_reason: None,
    }
}

#[derive(Debug, Clone)]
struct ToolState {
    update_id: String,
    title: String,
    kind: Option<String>,
    status: Option<String>,
    paths: Vec<String>,
    command: Option<String>,
    diffs: Vec<DiffStat>,
}

impl ToolState {
    fn body(&self, id: &str) -> RunUpdateBody {
        RunUpdateBody::ToolCall {
            tool_call_id: id.to_owned(),
            title: self.title.clone(),
            kind: self.kind.clone(),
            status: self.status.clone(),
            paths: self.paths.clone(),
            command: self.command.clone(),
            diffs: self.diffs.clone(),
        }
    }
}

struct PendingPermission {
    rpc_id: Value,
    update_id: String,
    prompt: PermissionPrompt,
}

/// 启动时从数据库读出的上下文。
struct Context {
    task: crate::model::Task,
    role: crate::model::Role,
    workspace: PathBuf,
    role_folder: Option<String>,
    env_profile: Option<String>,
    dependencies: Vec<brief::BriefDependency>,
}

struct Driver {
    core: Arc<Core>,
    active: Arc<ActiveRun>,
    run: Run,
    tx: Sender<Msg>,
    rx: Receiver<Msg>,
    settings: RunSettings,
    process: Option<Arc<AgentProcess>>,
    scope: Option<Scope>,
    workspace: PathBuf,
    auth_methods: Vec<String>,
    /// 当前可合并的消息条目：（条目 ID、来源、累计文本）。
    message: Option<(String, MessageRole, String)>,
    last_agent_text: String,
    tools: HashMap<String, ToolState>,
    plan_id: Option<String>,
    pending: Vec<PendingPermission>,
    /// 经 `fs/write_text_file` 写入的文件（规范化路径）与改动类型。
    writes: BTreeMap<PathBuf, FileChangeKind>,
    /// 权限请求被允许的编辑路径（规范化），之后的 `fs/write_text_file` 据此放行。
    granted: Vec<PathBuf>,
    tool_call_count: u32,
}

impl Driver {
    fn new(
        core: Arc<Core>,
        active: Arc<ActiveRun>,
        run: Run,
        tx: Sender<Msg>,
        rx: Receiver<Msg>,
    ) -> Self {
        let settings = core.runs.settings();
        Self {
            core,
            active,
            tool_call_count: run.tool_call_count,
            run,
            tx,
            rx,
            settings,
            process: None,
            scope: None,
            workspace: PathBuf::new(),
            auth_methods: Vec::new(),
            message: None,
            last_agent_text: String::new(),
            tools: HashMap::new(),
            plan_id: None,
            pending: Vec::new(),
            writes: BTreeMap::new(),
            granted: Vec::new(),
        }
    }

    fn aborted(&self) -> Option<End> {
        match self.active.stop() {
            Stop::None => None,
            Stop::Cancel => Some(End::Cancelled(None)),
            Stop::Takeover => Some(End::TakenOver),
            Stop::Shutdown => Some(End::Shutdown),
        }
    }

    fn conn(&self) -> &crate::acp::connection::Connection {
        &self.process.as_ref().expect("agent process").conn
    }

    /// 写数据库（事务），失败时只记日志不中断执行（数据库故障时执行仍以 Agent 为准）。
    fn db<T>(&self, f: impl FnOnce(&rusqlite::Connection) -> Result<T>) -> Option<T> {
        match self.core.write(f) {
            Ok(v) => Some(v),
            Err(err) => {
                eprintln!("teamspace run {}: {err}", self.run.id);
                None
            }
        }
    }

    fn emit_update(&self, update: RunUpdate) {
        self.core.emit(RunEvent::RunUpdated {
            run_id: self.run.id.clone(),
            task_id: self.run.task_id.clone(),
            update,
        });
    }

    fn insert(&mut self, key: Option<&str>, body: &RunUpdateBody) -> Option<String> {
        if !matches!(body, RunUpdateBody::Message { .. }) {
            self.message = None;
        }
        let run_id = self.run.id.clone();
        let update = self.db(|tx| store_run::insert_update(tx, &run_id, key, body))?;
        let id = update.id.clone();
        self.emit_update(update);
        Some(id)
    }

    fn replace(&self, update_id: &str, body: &RunUpdateBody) {
        let run_id = self.run.id.clone();
        if let Some(update) = self.db(|tx| store_run::replace_update(tx, &run_id, update_id, body))
        {
            self.emit_update(update);
        }
    }

    fn set_status(&mut self, status: RunStatus) {
        let pending = self.pending.first().map(|p| p.prompt.clone());
        let (id, count) = (self.run.id.clone(), self.tool_call_count);
        self.db(|tx| store_run::set_active_status(tx, &id, status, pending.as_ref(), count));
        self.run.status = status;
        self.core.emit_run(&self.run.id);
    }

    // ------------------------------------------------------------ 启动

    fn load_context(&self) -> Result<Context> {
        let conn = self.core.lock();
        let task = task::get(&conn, &self.run.task_id)?;
        let role = crate::service::role::get_active(&conn, &self.run.role_id)?;
        let role_folder: Option<String> = conn.query_row(
            "SELECT folder FROM roles WHERE id = ?1",
            rusqlite::params![self.run.role_id],
            |row| row.get(0),
        )?;
        let workspace = PathBuf::from(store_run::workspace_of(&conn, &self.run.id)?);
        let env_profile =
            harness::get_any(&conn, &self.run.harness.harness_id)?.and_then(|h| h.env_profile);
        let mut dependencies = Vec::new();
        for dep in &task.depends_on {
            let d = task::get(&conn, dep)?;
            dependencies.push(brief::BriefDependency {
                key: d.key,
                title: d.title,
                status_label: d.status.label().to_owned(),
            });
        }
        Ok(Context {
            role,
            role_folder,
            task,
            workspace,
            env_profile,
            dependencies,
        })
    }

    fn execute(&mut self, mode: Mode) -> End {
        match self.startup(mode) {
            Ok(()) => self.event_loop(),
            Err(end) => end,
        }
    }

    fn startup(&mut self, mode: Mode) -> std::result::Result<(), End> {
        let ctx = self
            .load_context()
            .map_err(|err| failed(FailureKind::Launch, format!("无法读取任务信息：{err}")))?;
        self.workspace = ctx.workspace.clone();
        if let Some(end) = self.aborted() {
            return Err(end);
        }

        // 1. 执行目录
        if mode == Mode::Start && self.run.isolation == RunIsolation::Worktree {
            let path_env = self.core.probe.command_path_env();
            let target = self
                .core
                .data_dir
                .join("worktrees")
                .join(&self.run.team_id)
                .join(format!("{}-r{}", self.run.task_key, self.run.attempt));
            let branch = format!("teamspace/{}-r{}", self.run.task_key, self.run.attempt);
            let wt = worktree::create(&ctx.workspace, &target, &branch, Some(&path_env))
                .map_err(|err| failed(FailureKind::Launch, format!("无法创建执行目录：{err}")))?;
            let exec = wt.exec_dir.display().to_string();
            let id = self.run.id.clone();
            self.db(|tx| {
                store_run::set_exec(tx, &id, &exec, Some(&wt.branch), Some(&wt.base_commit))
            });
            self.run.exec_dir = exec;
            self.run.branch = Some(wt.branch);
            self.run.base_commit = Some(wt.base_commit);
            self.core.emit_run(&self.run.id);
        }
        let exec_dir = PathBuf::from(&self.run.exec_dir);
        if !exec_dir.is_dir() {
            return Err(failed(
                FailureKind::Launch,
                format!("执行目录不存在：{}", exec_dir.display()),
            ));
        }

        // 2. 权限范围
        let mut role_folders = Vec::new();
        if let Some(rel) = &ctx.role_folder {
            role_folders.push(role_folder::absolute(&ctx.workspace, rel));
            if self.run.isolation == RunIsolation::Worktree {
                role_folders.push(role_folder::absolute(&exec_dir, rel));
            }
        }
        let mut refs = Vec::new();
        for r in &ctx.task.file_refs {
            refs.push(ctx.workspace.join(&r.path));
            if self.run.isolation == RunIsolation::Worktree {
                refs.push(exec_dir.join(&r.path));
            }
        }
        self.scope = Some(Scope::new(
            &exec_dir,
            self.run.isolation,
            &role_folders,
            &refs,
        ));
        if mode == Mode::Resume {
            self.restore_progress();
        }

        // 3. 启动 Agent
        let spec = self.spawn_spec(&ctx, &exec_dir)?;
        let tx = self.tx.clone();
        let process = AgentProcess::spawn(&spec, move |m| {
            let _ = tx.send(Msg::Agent(m));
        })
        .map_err(|err| {
            failed(
                FailureKind::Launch,
                format!(
                    "无法启动 {}：{err}",
                    acp::command_line(&spec.program, &spec.args)
                ),
            )
        })?;
        let process = Arc::new(process);
        *self
            .active
            .process
            .lock()
            .unwrap_or_else(|p| p.into_inner()) = Some(Arc::clone(&process));
        self.process = Some(process);
        if let Some(end) = self.aborted() {
            return Err(end);
        }

        // 4. initialize
        let result = self.call("initialize", acp::initialize_params(true))?;
        let handshake = acp::interpret_response(&json!({ "result": result }));
        if handshake.status != HandshakeStatus::Connected {
            return Err(failed(
                FailureKind::Protocol,
                handshake
                    .detail
                    .unwrap_or_else(|| "Agent 的 initialize 响应无效".to_owned()),
            ));
        }
        let caps = handshake.capabilities.unwrap_or_default();
        self.auth_methods = handshake.auth_methods;
        let id = self.run.id.clone();
        self.db(|tx| store_run::set_capabilities(tx, &id, &caps));
        self.run.capabilities = Some(caps.clone());

        // 5. 会话
        let mcp: Vec<Value> = Vec::new();
        match mode {
            Mode::Start => {
                let result = self.call(
                    "session/new",
                    json!({ "cwd": self.run.exec_dir, "mcpServers": mcp }),
                )?;
                let session = result
                    .get("sessionId")
                    .and_then(Value::as_str)
                    .filter(|s| !s.is_empty())
                    .ok_or_else(|| {
                        failed(
                            FailureKind::Protocol,
                            "Agent 的 session/new 响应缺少 sessionId",
                        )
                    })?
                    .to_owned();
                let id = self.run.id.clone();
                self.db(|tx| store_run::set_session(tx, &id, &session));
                self.run.session_id = Some(session);
            }
            Mode::Resume => {
                let session = self.run.session_id.clone().unwrap_or_default();
                self.call(
                    "session/load",
                    json!({ "sessionId": session, "cwd": self.run.exec_dir, "mcpServers": mcp }),
                )?;
                self.discard_replay();
            }
        }
        if let Some(end) = self.aborted() {
            return Err(end);
        }

        // 6. 任务说明
        let text = match mode {
            Mode::Start => brief::render(&self.brief_input(&ctx, &exec_dir)),
            Mode::Resume => brief::resume_text(
                &self.run.task_key,
                &ctx.role_folder
                    .clone()
                    .unwrap_or_else(|| format!("{}/{}", role_folder::ROOT_DIR, ctx.role.name)),
            ),
        };
        let refs: Vec<brief::BriefRef> = match mode {
            Mode::Start => self.brief_refs(&ctx, &exec_dir),
            Mode::Resume => Vec::new(),
        };
        let blocks = brief::prompt_blocks(&text, &refs, caps.embedded_context);
        self.append_message(MessageRole::User, &text);
        self.message = None;
        let run_id = self.run.id.clone();
        if let Some(update) =
            self.db(|tx| store_run::status_update(tx, &run_id, RunStatus::Running, None))
        {
            self.emit_update(update);
        }
        self.set_status(RunStatus::Running);
        let tx = self.tx.clone();
        self.conn().request_with(
            "session/prompt",
            json!({ "sessionId": self.run.session_id, "prompt": blocks }),
            Box::new(move |result| {
                let _ = tx.send(Msg::PromptDone(result));
            }),
        );
        Ok(())
    }

    fn brief_refs(&self, ctx: &Context, exec_dir: &Path) -> Vec<brief::BriefRef> {
        ctx.task
            .file_refs
            .iter()
            .map(|r| {
                let in_exec = exec_dir.join(&r.path);
                brief::BriefRef {
                    path: r.path.clone(),
                    anchor: r.anchor.clone(),
                    absolute: if in_exec.exists() {
                        in_exec
                    } else {
                        ctx.workspace.join(&r.path)
                    },
                }
            })
            .collect()
    }

    fn brief_input(&self, ctx: &Context, exec_dir: &Path) -> brief::BriefInput {
        let journal = ctx
            .role_folder
            .as_deref()
            .map(|rel| role_folder::absolute(&ctx.workspace, rel))
            .and_then(|dir| role_folder::read_recent_journal(&dir, brief::JOURNAL_LINES).ok())
            .unwrap_or_default();
        brief::BriefInput {
            role_name: ctx.role.name.clone(),
            role_description: ctx.role.description.clone(),
            role_guidelines: ctx.role.guidelines.clone(),
            journal,
            task_key: ctx.task.key.clone(),
            task_title: ctx.task.title.clone(),
            task_description: ctx.task.description.clone(),
            refs: self.brief_refs(ctx, exec_dir),
            dependencies: ctx.dependencies.clone(),
            exec_dir: exec_dir.display().to_string(),
            isolation: self.run.isolation,
            branch: self.run.branch.clone(),
            role_folder: ctx.role_folder.clone(),
        }
    }

    fn spawn_spec(&self, ctx: &Context, exec_dir: &Path) -> std::result::Result<SpawnSpec, End> {
        // 执行快照里的参数是脱敏过的（D6），启动 Agent 要用 `harnesses` 表里的原始参数。
        // 实例查不到时（已删除）退回快照值，让失败原因更贴近实际。
        let instance = self
            .db(|tx| harness::get_any(tx, &self.run.harness.harness_id))
            .flatten();
        let command = instance
            .as_ref()
            .map_or(self.run.harness.command.as_str(), |h| h.command.as_str());
        let args: &[String] = instance
            .as_ref()
            .map_or(self.run.harness.args.as_slice(), |h| h.args.as_slice());
        let launch = self
            .core
            .launch_for(
                self.run.harness.preset,
                command,
                args,
                ctx.env_profile.as_deref(),
            )
            .map_err(|err| failed(FailureKind::Launch, err))?;
        let command = launch.command.trim();
        let program = if command.contains('/') {
            let path = PathBuf::from(command);
            if !path.is_file() {
                return Err(failed(
                    FailureKind::Launch,
                    format!("启动命令不存在或不可执行：{command}"),
                ));
            }
            path
        } else {
            self.core
                .probe
                .locate_command(command)
                .map(PathBuf::from)
                .ok_or_else(|| {
                    failed(
                        FailureKind::Launch,
                        format!("未找到命令 {command}：请确认已安装并加入 PATH"),
                    )
                })?
        };
        let mut env = vec![("PATH".into(), self.core.probe.command_path_env())];
        if let Some(profile) = launch.env_profile.as_deref() {
            let vars = env_profile::load(Path::new(profile))
                .map_err(|err| failed(FailureKind::Launch, err))?;
            env.extend(vars.into_iter().map(|(k, v)| (k.into(), v.into())));
        }
        Ok(SpawnSpec {
            program,
            args: launch.args,
            cwd: exec_dir.to_path_buf(),
            env,
        })
    }

    /// 启动阶段的阻塞请求，失败映射为结局。
    fn call(&self, method: &str, params: Value) -> std::result::Result<Value, End> {
        let active = Arc::clone(&self.active);
        let result =
            self.conn()
                .request(method, params, self.settings.startup_timeout, &move || {
                    active.stop() != Stop::None
                });
        match result {
            Ok(v) => Ok(v),
            Err(RpcFailure::Aborted) => Err(self.aborted().unwrap_or(End::Cancelled(None))),
            Err(RpcFailure::Timeout) => Err(failed(
                FailureKind::Timeout,
                format!(
                    "超时：Agent 在 {} 秒内没有响应 {method}",
                    self.settings.startup_timeout.as_secs()
                ),
            )),
            Err(failure) => Err(self.rpc_failure(method, failure)),
        }
    }

    fn rpc_failure(&self, method: &str, failure: RpcFailure) -> End {
        match failure {
            RpcFailure::Error { code, message, .. } if code == AUTH_REQUIRED_CODE => {
                self.needs_auth(&message)
            }
            RpcFailure::Error { code, message, .. } => failed(
                FailureKind::Protocol,
                format!("协议错误：Agent 对 {method} 返回错误（错误码 {code}）：{message}"),
            ),
            RpcFailure::Closed | RpcFailure::Write(_) => self.agent_exited(),
            RpcFailure::Timeout | RpcFailure::Aborted => failed(
                FailureKind::Protocol,
                format!("协议错误：{method} 没有完成"),
            ),
        }
    }

    fn needs_auth(&self, message: &str) -> End {
        let methods = if self.auth_methods.is_empty() {
            String::new()
        } else {
            format!("（登录方式：{}）", self.auth_methods.join("、"))
        };
        failed(
            FailureKind::NeedsAuth,
            format!(
                "需要登录：{message}{methods}。登录由 Agent 自己管理，Teamspace 不保存凭据；请在终端完成登录后重试"
            ),
        )
    }

    fn agent_exited(&self) -> End {
        let (exit, stderr) = match &self.process {
            Some(p) => (p.exit_status(EXIT_WAIT), p.stderr_tail()),
            None => (None, None),
        };
        let mut text = match exit {
            Some(exit) => format!("Agent 进程退出（{exit}）"),
            None => "Agent 关闭了输出".to_owned(),
        };
        // Agent 的 stderr 可能回显启动参数里的密钥，落库前脱敏（D6）。
        if let Some(stderr) = stderr {
            text.push_str(&format!("：{}", crate::redact::text(&stderr)));
        }
        failed(FailureKind::AgentExited, text)
    }

    /// `session/load` 之后丢弃 Agent 回放的历史消息；其他消息保留下来按顺序处理。
    fn discard_replay(&mut self) {
        let mut keep = Vec::new();
        while let Ok(msg) = self.rx.try_recv() {
            match msg {
                Msg::Agent(Incoming::Notification { ref method, .. })
                    if method == "session/update" => {}
                other => keep.push(other),
            }
        }
        for msg in keep {
            let _ = self.tx.send(msg);
        }
    }

    /// 续接时从数据库恢复工具调用、计划与写入记录，使之后的更新落到原有条目上。
    fn restore_progress(&mut self) {
        let page = self.core.run_updates(&RunUpdateQuery {
            run_id: self.run.id.clone(),
            cursor: None,
            limit: Some(store_run::MAX_UPDATE_PAGE),
        });
        let Ok(mut page) = page else { return };
        loop {
            for update in &page.updates {
                match &update.body {
                    RunUpdateBody::ToolCall {
                        tool_call_id,
                        title,
                        kind,
                        status,
                        paths,
                        command,
                        diffs,
                    } => {
                        self.tools.insert(
                            tool_call_id.clone(),
                            ToolState {
                                update_id: update.id.clone(),
                                title: title.clone(),
                                kind: kind.clone(),
                                status: status.clone(),
                                paths: paths.clone(),
                                command: command.clone(),
                                diffs: diffs.clone(),
                            },
                        );
                    }
                    RunUpdateBody::Plan { .. } => self.plan_id = Some(update.id.clone()),
                    RunUpdateBody::FileWrite {
                        path,
                        allowed: true,
                        ..
                    } => {
                        self.writes
                            .entry(PathBuf::from(path))
                            .or_insert(FileChangeKind::Modified);
                    }
                    _ => {}
                }
            }
            if !page.has_more {
                break;
            }
            match self.core.run_updates(&RunUpdateQuery {
                run_id: self.run.id.clone(),
                cursor: Some(page.next_cursor.clone()),
                limit: Some(store_run::MAX_UPDATE_PAGE),
            }) {
                Ok(next) => page = next,
                Err(_) => break,
            }
        }
    }

    // ------------------------------------------------------------ 事件循环

    fn event_loop(&mut self) -> End {
        let idle = self.settings.idle_timeout;
        let mut idle_deadline = Instant::now() + idle;
        let mut stop_deadline: Option<Instant> = None;
        loop {
            let now = Instant::now();
            if !self.pending.is_empty() {
                idle_deadline = now + idle;
            }
            let deadline = stop_deadline.unwrap_or(idle_deadline);
            let wait = deadline
                .saturating_duration_since(now)
                .max(Duration::from_millis(1));
            match self.rx.recv_timeout(wait) {
                Ok(Msg::Agent(Incoming::Notification { method, params })) => {
                    idle_deadline = Instant::now() + idle;
                    if method == "session/update" {
                        self.on_update(params.get("update").unwrap_or(&Value::Null));
                    }
                }
                Ok(Msg::Agent(Incoming::Request { id, method, params })) => {
                    idle_deadline = Instant::now() + idle;
                    self.on_request(id, &method, &params);
                }
                Ok(Msg::Agent(Incoming::Malformed(_) | Incoming::Closed)) => {}
                Ok(Msg::PromptDone(result)) => return self.prompt_finished(result),
                Ok(Msg::Permission {
                    request_id,
                    option_id,
                    reply,
                }) => {
                    let result = self.on_human_decision(&request_id, &option_id);
                    let _ = reply.send(result);
                }
                Ok(Msg::Stop) => {
                    if self.active.stop() == Stop::Shutdown {
                        return End::Shutdown;
                    }
                    if stop_deadline.is_none() {
                        self.begin_cancel();
                        stop_deadline = Some(Instant::now() + self.settings.cancel_grace);
                    }
                }
                Err(RecvTimeoutError::Timeout) => {
                    if stop_deadline.is_some_and(|d| Instant::now() >= d) {
                        return self.aborted().unwrap_or(End::Cancelled(None));
                    }
                    if stop_deadline.is_none()
                        && self.pending.is_empty()
                        && Instant::now() >= idle_deadline
                    {
                        let minutes = idle.as_secs() / 60;
                        let text = if minutes >= 1 {
                            format!("超时：{minutes} 分钟无输出")
                        } else {
                            format!("超时：{} 秒无输出", idle.as_secs().max(1))
                        };
                        return failed(FailureKind::Timeout, text);
                    }
                }
                Err(RecvTimeoutError::Disconnected) => {
                    return failed(FailureKind::Protocol, "执行引擎内部错误：消息通道已关闭");
                }
            }
        }
    }

    fn prompt_finished(&mut self, result: std::result::Result<Value, RpcFailure>) -> End {
        if let Some(end) = self.aborted() {
            return end;
        }
        match result {
            Ok(value) => {
                let reason = value
                    .get("stopReason")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_owned();
                let with = |kind, text: &str| End::Failed {
                    kind,
                    failure: text.to_owned(),
                    stop_reason: Some(reason.clone()),
                };
                match reason.as_str() {
                    "end_turn" => End::Succeeded(reason),
                    "cancelled" => End::Cancelled(Some(reason)),
                    "refusal" => with(FailureKind::Refused, "Agent 拒绝了这个任务（refusal）"),
                    "max_tokens" => with(FailureKind::Limit, "达到 Agent 的输出上限（max_tokens）"),
                    "max_turn_requests" => with(
                        FailureKind::Limit,
                        "达到 Agent 的轮次上限（max_turn_requests）",
                    ),
                    "" => with(
                        FailureKind::Protocol,
                        "协议错误：session/prompt 的响应缺少 stopReason",
                    ),
                    other => with(
                        FailureKind::Protocol,
                        &format!("协议错误：未知的结束原因 {other}"),
                    ),
                }
            }
            Err(failure) => self.rpc_failure("session/prompt", failure),
        }
    }

    /// 取消：通知 Agent，撤销所有待确认的权限请求。
    fn begin_cancel(&mut self) {
        if let Some(session) = &self.run.session_id
            && let Some(process) = &self.process
        {
            let _ = process
                .conn
                .notify("session/cancel", json!({ "sessionId": session }));
        }
        let pending = std::mem::take(&mut self.pending);
        for p in pending {
            let _ = self
                .conn()
                .respond(&p.rpc_id, json!({ "outcome": { "outcome": "cancelled" } }));
            let decision = PermissionDecision {
                option_id: None,
                kind: "cancelled".to_owned(),
                by: "cancelled".to_owned(),
                reason: "执行已停止，权限请求已撤销".to_owned(),
                decided_at: now(),
            };
            self.replace(&p.update_id, &permission_body(&p.prompt, Some(decision)));
        }
        if self.run.status == RunStatus::WaitingPermission {
            self.set_status(RunStatus::Running);
        }
    }

    // ------------------------------------------------------------ session/update

    fn on_update(&mut self, update: &Value) {
        match update.get("sessionUpdate").and_then(Value::as_str) {
            Some("agent_message_chunk") => {
                self.append_message(MessageRole::Agent, &content_text(update.get("content")))
            }
            Some("agent_thought_chunk") => {
                self.append_message(MessageRole::Thought, &content_text(update.get("content")))
            }
            Some("user_message_chunk") => {
                self.append_message(MessageRole::User, &content_text(update.get("content")))
            }
            Some("tool_call" | "tool_call_update") => {
                self.upsert_tool(update);
            }
            Some("plan") => self.set_plan(update),
            _ => {}
        }
    }

    fn append_message(&mut self, role: MessageRole, text: &str) {
        if text.is_empty() {
            return;
        }
        if let Some((id, current, buf)) = &mut self.message
            && *current == role
        {
            buf.push_str(text);
            let (id, body) = (
                id.clone(),
                RunUpdateBody::Message {
                    role,
                    text: buf.clone(),
                },
            );
            if role == MessageRole::Agent {
                self.last_agent_text = buf.clone();
            }
            self.replace(&id, &body);
            return;
        }
        let body = RunUpdateBody::Message {
            role,
            text: text.to_owned(),
        };
        if let Some(id) = self.insert(None, &body) {
            self.message = Some((id, role, text.to_owned()));
        }
        if role == MessageRole::Agent {
            self.last_agent_text = text.to_owned();
        }
    }

    /// 合并 `tool_call` / `tool_call_update`（或权限请求中的 `toolCall`），返回工具调用 ID。
    fn upsert_tool(&mut self, update: &Value) -> Option<String> {
        let id = update.get("toolCallId").and_then(Value::as_str)?.to_owned();
        let str_field = |key: &str| {
            update
                .get(key)
                .and_then(Value::as_str)
                .map(str::to_owned)
                .filter(|s| !s.is_empty())
        };
        let mut paths: Option<Vec<String>> =
            update
                .get("locations")
                .and_then(Value::as_array)
                .map(|list| {
                    list.iter()
                        .filter_map(|l| l.get("path").and_then(Value::as_str).map(str::to_owned))
                        .collect()
                });
        let diffs = update
            .get("content")
            .and_then(Value::as_array)
            .map(|items| {
                items
                    .iter()
                    .filter(|c| c.get("type").and_then(Value::as_str) == Some("diff"))
                    .filter_map(diff_stat)
                    .collect::<Vec<_>>()
            });
        if let Some(diffs) = &diffs {
            let list = paths.get_or_insert_with(Vec::new);
            for d in diffs {
                if !list.contains(&d.path) {
                    list.push(d.path.clone());
                }
            }
        }
        let command = update.get("rawInput").and_then(command_of);
        let existed = self.tools.contains_key(&id);
        let state = self.tools.entry(id.clone()).or_insert_with(|| ToolState {
            update_id: String::new(),
            title: String::new(),
            kind: None,
            status: None,
            paths: Vec::new(),
            command: None,
            diffs: Vec::new(),
        });
        if let Some(title) = str_field("title") {
            state.title = title;
        }
        if let Some(kind) = str_field("kind") {
            state.kind = Some(kind);
        }
        if let Some(status) = str_field("status") {
            state.status = Some(status);
        }
        if let Some(paths) = paths
            && !paths.is_empty()
        {
            state.paths = paths;
        }
        if let Some(diffs) = diffs
            && !diffs.is_empty()
        {
            state.diffs = diffs;
        }
        if command.is_some() {
            state.command = command;
        }
        if state.title.is_empty() {
            state.title = state
                .command
                .clone()
                .unwrap_or_else(|| "工具调用".to_owned());
        }
        let body = state.body(&id);
        if existed && !state.update_id.is_empty() {
            let update_id = state.update_id.clone();
            self.replace(&update_id, &body);
        } else {
            self.tool_call_count += 1;
            if let Some(update_id) = self.insert(Some(&id), &body)
                && let Some(state) = self.tools.get_mut(&id)
            {
                state.update_id = update_id;
            }
        }
        Some(id)
    }

    fn set_plan(&mut self, update: &Value) {
        let entries = update
            .get("entries")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .map(|e| PlanEntry {
                content: e
                    .get("content")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_owned(),
                priority: e
                    .get("priority")
                    .and_then(Value::as_str)
                    .unwrap_or("medium")
                    .to_owned(),
                status: e
                    .get("status")
                    .and_then(Value::as_str)
                    .unwrap_or("pending")
                    .to_owned(),
            })
            .collect();
        let body = RunUpdateBody::Plan { entries };
        match self.plan_id.clone() {
            Some(id) => self.replace(&id, &body),
            None => self.plan_id = self.insert(Some("plan"), &body),
        }
    }

    // ------------------------------------------------------------ Agent 的请求

    fn on_request(&mut self, id: Value, method: &str, params: &Value) {
        match method {
            "session/request_permission" => self.on_permission(id, params),
            "fs/read_text_file" => self.on_read(id, params),
            "fs/write_text_file" => self.on_write(id, params),
            other => {
                let _ = self.conn().respond_error(
                    &id,
                    METHOD_NOT_FOUND_CODE,
                    &format!("Teamspace 不提供 {other}"),
                );
            }
        }
    }

    fn scope(&self) -> &Scope {
        self.scope
            .as_ref()
            .expect("scope is set before the session starts")
    }

    fn on_permission(&mut self, rpc_id: Value, params: &Value) {
        let tool = params.get("toolCall").cloned().unwrap_or(Value::Null);
        let tool_id = self
            .upsert_tool(&tool)
            .unwrap_or_else(|| format!("unknown-{}", new_id()));
        let state = self.tools.get(&tool_id).cloned();
        let (title, kind, paths, command) = match &state {
            Some(s) => (
                s.title.clone(),
                s.kind.clone(),
                s.paths.clone(),
                s.command.clone(),
            ),
            None => ("工具调用".to_owned(), None, Vec::new(), None),
        };
        let options: Vec<PermissionOption> = params
            .get("options")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(|o| {
                Some(PermissionOption {
                    option_id: o.get("optionId")?.as_str()?.to_owned(),
                    name: o
                        .get("name")
                        .and_then(Value::as_str)
                        .unwrap_or_default()
                        .to_owned(),
                    kind: o
                        .get("kind")
                        .and_then(Value::as_str)
                        .unwrap_or_default()
                        .to_owned(),
                })
            })
            .collect();
        let prompt = |reason: String| PermissionPrompt {
            request_id: new_id(),
            tool_call_id: tool_id.clone(),
            title: title.clone(),
            tool_kind: kind.clone(),
            paths: paths.clone(),
            command: command.clone(),
            reason,
            options: options.clone(),
            requested_at: now(),
        };
        let reason = match self.scope().decide(kind.as_deref(), &paths) {
            Verdict::Allow(reason) => {
                let allow = ["allow_once", "allow_always"]
                    .iter()
                    .find_map(|k| options.iter().find(|o| o.kind == *k));
                if let Some(option) = allow {
                    self.grant(&paths);
                    let _ = self.conn().respond(
                        &rpc_id,
                        json!({ "outcome": { "outcome": "selected", "optionId": option.option_id } }),
                    );
                    let p = prompt(reason.clone());
                    let decision = PermissionDecision {
                        option_id: Some(option.option_id.clone()),
                        kind: option.kind.clone(),
                        by: "rule".to_owned(),
                        reason,
                        decided_at: now(),
                    };
                    self.insert(Some(&p.request_id), &permission_body(&p, Some(decision)));
                    return;
                }
                format!("{reason}，但 Agent 没有提供「允许」选项，需要确认")
            }
            Verdict::Ask(reason) => reason,
        };
        let p = prompt(reason);
        let Some(update_id) = self.insert(Some(&p.request_id), &permission_body(&p, None)) else {
            let _ = self
                .conn()
                .respond(&rpc_id, json!({ "outcome": { "outcome": "cancelled" } }));
            return;
        };
        self.pending.push(PendingPermission {
            rpc_id,
            update_id,
            prompt: p.clone(),
        });
        self.set_status(RunStatus::WaitingPermission);
        self.core.emit(RunEvent::PermissionRequested {
            run_id: self.run.id.clone(),
            task_id: self.run.task_id.clone(),
            team_id: self.run.team_id.clone(),
            prompt: p,
        });
    }

    fn grant(&mut self, paths: &[String]) {
        let base = self.scope().exec_dir().to_path_buf();
        for path in paths {
            if let Ok(real) = policy::resolve(Path::new(path), &base) {
                self.granted.push(real);
            }
        }
    }

    fn on_human_decision(&mut self, request_id: &str, option_id: &str) -> Result<()> {
        let index = self
            .pending
            .iter()
            .position(|p| p.prompt.request_id == request_id)
            .ok_or_else(|| Error::not_found("权限请求不存在或已处理"))?;
        let option = self.pending[index]
            .prompt
            .options
            .iter()
            .find(|o| o.option_id == option_id)
            .cloned()
            .ok_or_else(|| Error::invalid("所选选项不属于这个权限请求"))?;
        let p = self.pending.remove(index);
        if option.kind.starts_with("allow") {
            let paths = p.prompt.paths.clone();
            self.grant(&paths);
        }
        let _ = self.conn().respond(
            &p.rpc_id,
            json!({ "outcome": { "outcome": "selected", "optionId": option.option_id } }),
        );
        let decision = PermissionDecision {
            option_id: Some(option.option_id.clone()),
            kind: option.kind.clone(),
            by: "human".to_owned(),
            reason: format!("Human 选择「{}」", option.name),
            decided_at: now(),
        };
        self.replace(&p.update_id, &permission_body(&p.prompt, Some(decision)));
        let status = if self.pending.is_empty() {
            RunStatus::Running
        } else {
            RunStatus::WaitingPermission
        };
        self.set_status(status);
        Ok(())
    }

    fn on_read(&mut self, id: Value, params: &Value) {
        let path = params
            .get("path")
            .and_then(Value::as_str)
            .unwrap_or_default();
        let result = self.scope().can_read(path).and_then(|real| {
            std::fs::read_to_string(&real).map_err(|err| format!("无法读取 {path}：{err}"))
        });
        match result {
            Ok(text) => {
                let line = params
                    .get("line")
                    .and_then(Value::as_u64)
                    .unwrap_or(1)
                    .max(1) as usize;
                let limit = params
                    .get("limit")
                    .and_then(Value::as_u64)
                    .map(|l| l as usize);
                let content = if line > 1 || limit.is_some() {
                    let lines = text.split_inclusive('\n').skip(line - 1);
                    match limit {
                        Some(n) => lines.take(n).collect(),
                        None => lines.collect(),
                    }
                } else {
                    text
                };
                let _ = self.conn().respond(&id, json!({ "content": content }));
            }
            Err(reason) => {
                let _ = self.conn().respond_error(&id, INTERNAL_ERROR_CODE, &reason);
            }
        }
    }

    fn on_write(&mut self, id: Value, params: &Value) {
        let path = params
            .get("path")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned();
        let content = params
            .get("content")
            .and_then(Value::as_str)
            .unwrap_or_default();
        let base = self.scope().exec_dir().to_path_buf();
        let checked = match self.scope().can_write(&path) {
            Ok(real) => Ok((real, "在自动批准范围内".to_owned())),
            Err(reason) => match policy::resolve(Path::new(&path), &base) {
                Ok(real) if self.granted.contains(&real) => {
                    Ok((real, "Human 已允许对应的权限请求".to_owned()))
                }
                _ => Err(reason),
            },
        };
        let outcome = checked.and_then(|(real, reason)| {
            let existed = real.exists();
            if let Some(parent) = real.parent() {
                std::fs::create_dir_all(parent).map_err(|err| format!("无法创建目录：{err}"))?;
            }
            std::fs::write(&real, content).map_err(|err| format!("写入失败：{err}"))?;
            self.writes.entry(real).or_insert(if existed {
                FileChangeKind::Modified
            } else {
                FileChangeKind::Added
            });
            Ok(reason)
        });
        let body = match &outcome {
            Ok(reason) => {
                let _ = self.conn().respond(&id, json!({}));
                RunUpdateBody::FileWrite {
                    path: path.clone(),
                    allowed: true,
                    reason: reason.clone(),
                }
            }
            Err(reason) => {
                let _ = self.conn().respond_error(
                    &id,
                    INTERNAL_ERROR_CODE,
                    &format!("Teamspace 未允许这次写入：{reason}"),
                );
                RunUpdateBody::FileWrite {
                    path: path.clone(),
                    allowed: false,
                    reason: reason.clone(),
                }
            }
        };
        self.insert(None, &body);
    }

    // ------------------------------------------------------------ 结束

    fn changed_files(&self) -> Vec<ChangedFile> {
        if self.run.isolation == RunIsolation::Worktree {
            let Some(base) = &self.run.base_commit else {
                return Vec::new();
            };
            let path_env = self.core.probe.command_path_env();
            return worktree::changed_files(Path::new(&self.run.exec_dir), base, Some(&path_env))
                .unwrap_or_default();
        }
        let root = self
            .workspace
            .canonicalize()
            .unwrap_or_else(|_| self.workspace.clone());
        let mut changes: BTreeMap<String, FileChangeKind> = BTreeMap::new();
        let rel = |p: &Path| match p.strip_prefix(&root) {
            Ok(r) => r
                .components()
                .map(|c| c.as_os_str().to_string_lossy())
                .collect::<Vec<_>>()
                .join("/"),
            Err(_) => p.display().to_string(),
        };
        for (path, kind) in &self.writes {
            changes.insert(rel(path), *kind);
        }
        for tool in self.tools.values() {
            let kind = match tool.kind.as_deref() {
                Some("edit" | "move") => FileChangeKind::Modified,
                Some("delete") => FileChangeKind::Deleted,
                _ => continue,
            };
            if tool.status.as_deref() == Some("failed") {
                continue;
            }
            for path in &tool.paths {
                if let Ok(real) = policy::resolve(Path::new(path), &root) {
                    changes.entry(rel(&real)).or_insert(kind);
                }
            }
        }
        changes
            .into_iter()
            .map(|(path, change)| ChangedFile { path, change })
            .collect()
    }

    fn finish(&mut self, end: End) {
        if let Some(process) = &self.process {
            process.kill();
        }
        if end == End::Shutdown || self.active.stop() == Stop::Shutdown {
            // 应用退出：由 shutdown_runs 标记为已中断。
            return;
        }
        let summary = brief::summarize(&self.last_agent_text);
        let mut ending = Ending {
            summary,
            changed_files: self.changed_files(),
            tool_call_count: self.tool_call_count,
            ..Ending::default()
        };
        let (status, task_to, reason, actor, verb) = match &end {
            End::Succeeded(stop) => {
                ending.stop_reason = Some(stop.clone());
                (
                    RunStatus::Succeeded,
                    Some(TaskStatus::Review),
                    None,
                    Actor::System,
                    "完成",
                )
            }
            End::Failed {
                kind,
                failure,
                stop_reason,
            } => {
                ending.stop_reason = stop_reason.clone();
                // 失败说明会进入执行快照、任务受阻原因、活动与工作日志，落库前统一脱敏（D6）。
                let failure = crate::redact::text(failure);
                ending.failure = Some(failure.clone());
                ending.failure_kind = Some(*kind);
                let reason: String = format!("{}{failure}", store_run::FAILED_PREFIX)
                    .chars()
                    .take(MAX_REASON)
                    .collect();
                (
                    RunStatus::Failed,
                    Some(TaskStatus::Blocked),
                    Some(reason),
                    Actor::System,
                    "失败",
                )
            }
            End::Cancelled(stop) => {
                ending.stop_reason = stop.clone();
                ending.failure = Some(if stop.is_some() && self.active.stop() == Stop::None {
                    "Agent 取消了本轮执行".to_owned()
                } else {
                    "已取消".to_owned()
                });
                (
                    RunStatus::Cancelled,
                    Some(TaskStatus::Ready),
                    None,
                    Actor::Human,
                    "已取消",
                )
            }
            End::TakenOver => {
                ending.taken_over = true;
                ending.failure = Some("人工接管".to_owned());
                (
                    RunStatus::Cancelled,
                    None,
                    None,
                    Actor::Human,
                    "已停止，人工接管",
                )
            }
            End::Shutdown => unreachable!(),
        };
        let run = self.run.clone();
        let auth = matches!(
            &end,
            End::Failed {
                kind: FailureKind::NeedsAuth,
                ..
            }
        );
        let auth_methods = self.auth_methods.clone();
        let capabilities = self.run.capabilities.clone();
        let detail = ending.failure.clone();
        let saved = self.db(|tx| {
            store_run::end(tx, &run.id, status, &ending)?;
            let update = store_run::status_update(tx, &run.id, status, detail.clone())?;
            if let Some(to) = task_to
                && task::get(tx, &run.task_id)?.status == TaskStatus::Doing
            {
                store_run::move_task(
                    tx,
                    &run.task_id,
                    to,
                    reason.clone(),
                    actor,
                    &format!("执行{verb}"),
                )?;
            }
            let files: Vec<&str> = ending
                .changed_files
                .iter()
                .map(|f| f.path.as_str())
                .collect();
            store_run::record_event(
                tx,
                &run,
                actor,
                EventKind::RunFinished,
                format!(
                    "{} 第 {} 次执行{verb}（{}）",
                    run.task_key, run.attempt, run.harness.name
                ),
                json!({
                    "runId": run.id,
                    "attempt": run.attempt,
                    "status": status,
                    "takenOver": ending.taken_over,
                    "summary": ending.summary,
                    "failure": ending.failure,
                    "changedFiles": files,
                }),
            )?;
            if auth {
                mark_needs_auth(tx, &run, &auth_methods, capabilities, detail.as_deref())?;
            }
            Ok(update)
        });
        if let Some(update) = saved {
            self.emit_update(update);
        }
        self.core.sync_role_folders(&self.run.team_id);
        self.core.emit_run(&self.run.id);
    }
}

/// 执行时 Agent 要求登录：把 Harness 握手状态更新为 needs_auth。
fn mark_needs_auth(
    tx: &rusqlite::Connection,
    run: &Run,
    auth_methods: &[String],
    capabilities: Option<AcpCapabilities>,
    detail: Option<&str>,
) -> Result<()> {
    let Some(instance) = harness::get_any(tx, &run.harness.harness_id)? else {
        return Ok(());
    };
    if instance.archived_at.is_some() {
        return Ok(());
    }
    let methods = if auth_methods.is_empty() {
        "Agent 没有列出登录方式".to_owned()
    } else {
        format!("登录方式：{}", auth_methods.join("、"))
    };
    let handshake = Handshake {
        status: HandshakeStatus::NeedsAuth,
        protocol_version: instance.handshake.protocol_version.or(Some(1)),
        agent: instance.handshake.agent.clone(),
        capabilities: capabilities.or(instance.handshake.capabilities.clone()),
        auth_methods: auth_methods.to_vec(),
        detail: Some(format!(
            "执行 {} 时 Agent 要求先登录（{methods}）。请在对应工具中完成登录后重新检查。{}",
            run.task_key,
            detail.unwrap_or_default()
        )),
        checked_at: Some(now()),
    };
    harness::save_handshake(tx, &instance.id, &handshake)?;
    Ok(())
}

fn permission_body(p: &PermissionPrompt, decision: Option<PermissionDecision>) -> RunUpdateBody {
    RunUpdateBody::Permission {
        request_id: p.request_id.clone(),
        tool_call_id: p.tool_call_id.clone(),
        title: p.title.clone(),
        tool_kind: p.tool_kind.clone(),
        paths: p.paths.clone(),
        command: p.command.clone(),
        reason: p.reason.clone(),
        options: p.options.clone(),
        decision,
    }
}

/// 内容块的文字形式。
fn content_text(content: Option<&Value>) -> String {
    let Some(content) = content else {
        return String::new();
    };
    match content.get("type").and_then(Value::as_str) {
        Some("text") => content
            .get("text")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned(),
        Some("image") => "[图片]".to_owned(),
        Some("audio") => "[音频]".to_owned(),
        Some("resource_link") => format!(
            "[链接] {}",
            content
                .get("name")
                .and_then(Value::as_str)
                .unwrap_or_default()
        ),
        Some("resource") => format!(
            "[资源] {}",
            content
                .pointer("/resource/uri")
                .and_then(Value::as_str)
                .unwrap_or_default()
        ),
        _ => String::new(),
    }
}

/// 工具调用 rawInput 中的命令（`command` 为字符串或字符串数组，或 `cmd`）。
fn command_of(raw: &Value) -> Option<String> {
    let value = raw.get("command").or_else(|| raw.get("cmd"))?;
    match value {
        Value::String(s) if !s.trim().is_empty() => Some(s.trim().to_owned()),
        Value::Array(parts) => {
            let parts: Vec<&str> = parts.iter().filter_map(Value::as_str).collect();
            (!parts.is_empty()).then(|| parts.join(" "))
        }
        _ => None,
    }
}

/// ACP diff 内容的行数统计（按行的多重集合差计算）。
fn diff_stat(content: &Value) -> Option<DiffStat> {
    let path = content.get("path")?.as_str()?.to_owned();
    let new = content
        .get("newText")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let old = content
        .get("oldText")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let mut counts: HashMap<&str, i64> = HashMap::new();
    for line in old.lines() {
        *counts.entry(line).or_default() += 1;
    }
    let mut added = 0u32;
    for line in new.lines() {
        match counts.get_mut(line) {
            Some(n) if *n > 0 => *n -= 1,
            _ => added += 1,
        }
    }
    let removed = counts.values().filter(|n| **n > 0).map(|n| *n as u32).sum();
    Some(DiffStat {
        path,
        added,
        removed,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn diff_stats_count_line_changes() {
        let stat = diff_stat(&json!({
            "type": "diff", "path": "/a.md",
            "oldText": "a\nb\nc\n", "newText": "a\nB\nc\nd\n"
        }))
        .unwrap();
        assert_eq!((stat.added, stat.removed), (2, 1));
        let created =
            diff_stat(&json!({ "type": "diff", "path": "/n.md", "newText": "x\ny" })).unwrap();
        assert_eq!((created.added, created.removed), (2, 0));
        assert!(diff_stat(&json!({ "type": "diff" })).is_none());
    }

    #[test]
    fn commands_and_content_are_rendered() {
        assert_eq!(
            command_of(&json!({ "command": " cargo test " })).as_deref(),
            Some("cargo test")
        );
        assert_eq!(
            command_of(&json!({ "command": ["git", "status"] })).as_deref(),
            Some("git status")
        );
        assert_eq!(command_of(&json!({ "cmd": "ls" })).as_deref(), Some("ls"));
        assert_eq!(command_of(&json!({ "path": "/x" })), None);
        assert_eq!(
            content_text(Some(&json!({ "type": "text", "text": "hi" }))),
            "hi"
        );
        assert_eq!(content_text(Some(&json!({ "type": "image" }))), "[图片]");
        assert_eq!(
            content_text(Some(
                &json!({ "type": "resource_link", "name": "a.md", "uri": "file:///a.md" })
            )),
            "[链接] a.md"
        );
        assert_eq!(content_text(None), "");
    }
}
