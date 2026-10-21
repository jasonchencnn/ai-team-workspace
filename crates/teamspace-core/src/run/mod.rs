//! 执行引擎（M2，D13 / D15）：把任务派给负责岗位绑定的 ACP Agent，驱动一次会话直到结束。
//!
//! 每个活动执行由一个后台线程（driver）驱动：
//! 1. 准备执行目录：Git 工作空间新建 worktree（[`worktree`]），普通文件夹原地执行；
//! 2. 启动 Agent 子进程（独立进程组，环境 = 继承环境 + 补全的 PATH + 环境配置档案），`initialize`
//!    声明 Client 提供文件读写、不提供终端；
//! 3. `session/new`（cwd = 执行目录、无 MCP 服务器）或续接时 `session/load`（丢弃回放的历史）；之后把会话切到
//!    「向 Client 请求权限」的模式（[`session_mode`]，S-1a）；
//! 4. 发送任务说明包（[`brief`]）作为第一个 `session/prompt`；
//! 5. 处理 Agent 的消息：`session/update` 写入执行进度（消息块合并、计划、工具调用状态）；
//!    `session/request_permission` 按权限规则（[`policy`]）自动允许或转交 Human；`fs/read_text_file`、
//!    `fs/write_text_file` 限定在执行范围内；其他请求回复「方法不存在」；
//! 6. 结束：根据 `stopReason` 或失败原因写入结果、联动任务状态、写活动（进而写入岗位工作日志），
//!    终止 Agent 进程组。
//!
//! 改动统计兜底（S-1b）：Agent 可能用 shell 直接写文件而不发权限请求。普通文件夹原地执行时，开始前对执行目录
//! 拍快照（[`snapshot`]），结束或中断时再比对，改动 = ACP 报告的写入 ∪ 快照比对；Git worktree 用 Git 统计。
//! 原地执行中落在权限范围外的改动（不在本岗位文件夹、不在任务引用文件、Human 也没有明确允许写入）会在
//! 执行记录里醒目标出，成功的执行因此不进入待验收而是受阻，等 Human 检查。
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
pub mod git_review;
mod grants;
pub mod policy;
pub(crate) mod review_seam;
mod session_mode;
pub mod snapshot;
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
use policy::{EditPreview, Scope, Verdict, WriteTarget};

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
    /// 执行目录快照最多记录的文件与目录数（超过时降级，见 [`snapshot`]）。
    pub snapshot_max_entries: usize,
}

impl Default for RunSettings {
    fn default() -> Self {
        Self {
            idle_timeout: IDLE_TIMEOUT,
            startup_timeout: STARTUP_TIMEOUT,
            cancel_grace: CANCEL_GRACE,
            snapshot_max_entries: snapshot::MAX_ENTRIES,
        }
    }
}

/// 执行事件回调。
pub type RunListener = Arc<dyn Fn(&RunEvent) + Send + Sync>;

/// 执行引擎状态（属于 [`Core`]）。
#[derive(Default)]
pub struct Runs {
    active: Mutex<HashMap<String, Arc<ActiveRun>>>,
    /// 执行锁：执行结束时「确认仍是当前执行 → 提交到执行分支」与中断、续接互斥，旧 driver 的提交不会
    /// 落在已被中断或续接的执行上。
    exec_lock: Mutex<()>,
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

    /// 取得执行锁（见 `exec_lock`）。
    pub(crate) fn exec_lock(&self) -> std::sync::MutexGuard<'_, ()> {
        self.exec_lock.lock().unwrap_or_else(|p| p.into_inner())
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
    /// 验收退回后续接（M3-01）：执行记录的 `revision_of` 指向被退回的执行，沿用它的执行目录与会话；
    /// Agent 支持 `session/load` 时续接原会话并发送退回意见，否则在同一目录新开会话、发送完整任务说明
    /// 加上次摘要与退回意见。不新建 worktree。
    Revise,
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
            self.interrupt_run(
                id,
                "应用退出时执行仍在进行，已中断；已写入执行目录的改动仍然保留",
            );
        }
        for (_, active) in &actives {
            active.wait_done(Duration::from_secs(3));
        }
    }

    /// 打开数据库时：把残留的活动执行标记为已中断。
    pub(crate) fn recover_runs(&self) -> Result<()> {
        let stale = store_run::all_active(&self.lock())?;
        for run in &stale {
            let changes = self.interrupted_changes(run);
            self.write(|tx| {
                interrupt_with_changes(tx, &run.id, INTERRUPTED_RESTART, Some(&changes))
            })?;
        }
        self.discard_stale_snapshots(SNAPSHOT_KEEP);
        Ok(())
    }

    /// 把活动执行标记为已中断，并用快照（或 Git）算出它已经改了哪些文件一并写入（G-6）。
    fn interrupt_run(&self, run_id: &str, reason: &str) {
        let _exec = self.runs.exec_lock();
        // 先读出执行记录再统计：统计过程中还要读数据库，不能持着锁。
        let run = store_run::get(&self.lock(), run_id).ok();
        let changes = run
            .filter(|run| run.status.is_active())
            .map(|run| self.interrupted_changes(&run));
        let _ = self.write(|tx| interrupt_with_changes(tx, run_id, reason, changes.as_ref()));
    }

    /// 执行目录快照的保存位置（应用数据目录，不在执行目录里）。
    pub(crate) fn snapshot_path(&self, run_id: &str) -> PathBuf {
        self.data_dir
            .join("run-snapshots")
            .join(format!("{run_id}.json"))
    }

    /// 中断那一刻的快照：续接时用它分清「中断到续接之间」的改动。
    fn interrupted_snapshot_path(&self, run_id: &str) -> PathBuf {
        self.data_dir
            .join("run-snapshots")
            .join(format!("{run_id}.interrupted.json"))
    }

    /// 删除一次执行的全部快照文件（执行已经结束，不会再续接）。
    pub(crate) fn remove_snapshots(&self, run_id: &str) {
        let _ = std::fs::remove_file(self.snapshot_path(run_id));
        let _ = std::fs::remove_file(self.interrupted_snapshot_path(run_id));
    }

    /// 删除不再需要的快照：只有仍处于已中断（可能续接）的执行才保留，而且最多保留 `keep`
    ///（长期无人处理的中断执行，快照会过期；续接时没有快照只是少了对比基准，会在记录里说明）。
    pub(crate) fn discard_stale_snapshots(&self, keep: Duration) {
        let dir = self.data_dir.join("run-snapshots");
        let Ok(entries) = std::fs::read_dir(&dir) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let Some(name) = path.file_name().and_then(|s| s.to_str()) else {
                continue;
            };
            let id = name
                .strip_suffix(".interrupted.json")
                .or_else(|| name.strip_suffix(".json"))
                .unwrap_or(name);
            let interrupted = store_run::get(&self.lock(), id)
                .is_ok_and(|run| run.status == RunStatus::Interrupted);
            let expired = entry
                .metadata()
                .and_then(|m| m.modified())
                .ok()
                .and_then(|t| t.elapsed().ok())
                .is_some_and(|age| age > keep);
            if !interrupted || expired {
                let _ = std::fs::remove_file(&path);
            }
        }
    }

    /// 本次执行的岗位的文件夹（相对工作空间，如 `Teamspace/前端`）。
    fn role_folder_of(&self, run: &Run) -> Option<String> {
        self.lock()
            .query_row(
                "SELECT folder FROM roles WHERE id = ?1",
                rusqlite::params![run.role_id],
                |row| row.get::<_, Option<String>>(0),
            )
            .ok()
            .flatten()
    }

    /// 被中断的执行已经改了哪些文件：worktree 用 Git 统计；原地执行把存下的执行前快照与现在的
    /// 目录比对（没有快照时只能用 Agent 经 ACP 写入的记录）。同时把「中断这一刻」的快照存下来，
    /// 续接时用它分清中断期间别人改的文件。
    fn interrupted_changes(&self, run: &Run) -> Changes {
        let fallback = || Changes {
            files: run.changed_files.clone(),
            note: None,
            incomplete: false,
        };
        if run.isolation == RunIsolation::Worktree {
            let Some(base) = &run.base_commit else {
                return fallback();
            };
            let path_env = self.probe.command_path_env();
            return match worktree::changed_files(Path::new(&run.exec_dir), base, Some(&path_env)) {
                Ok(files) => Changes {
                    files,
                    note: None,
                    incomplete: false,
                },
                Err(err) => Changes {
                    note: Some(format!("无法统计 worktree 的改动：{err}")),
                    ..fallback()
                },
            };
        }
        let dir = Path::new(&run.exec_dir);
        if let Some(before) = snapshot::load(&self.snapshot_path(&run.id)) {
            let own_role = self.role_folder_of(run);
            let after = snapshot::take(dir, before.max_entries, &before.priority);
            let diff = snapshot::diff(dir, &before, &after, own_role.as_deref());
            let mut note = diff.note;
            if let Err(err) = snapshot::save(&self.interrupted_snapshot_path(&run.id), &after) {
                let text =
                    format!("无法保存中断时的快照（{err}），续接后无法分清中断期间别人改动的文件");
                note = Some(match note {
                    Some(n) => format!("{n}；{text}"),
                    None => text,
                });
            }
            return Changes {
                files: diff.changes,
                note,
                incomplete: diff.incomplete,
            };
        }
        // 没有执行前的快照（执行在拍快照之前就中断了）：只能汇总 Agent 经 ACP 写入的文件。
        let root = dir.canonicalize().unwrap_or_else(|_| dir.to_path_buf());
        let mut changes: BTreeMap<String, FileChangeKind> = BTreeMap::new();
        for path in self.written_paths(&run.id) {
            let real = policy::resolve(Path::new(&path), &root).unwrap_or(PathBuf::from(&path));
            changes
                .entry(relative_to(&root, &real))
                .or_insert(FileChangeKind::Modified);
        }
        // 快照在建立会话之前就拍好了：已有会话却没有快照，说明快照丢了，统计不完整，要说明。
        let note = run
            .session_id
            .is_some()
            .then(|| "没有执行前的快照，改动文件只包含 Agent 经 ACP 写入的记录".to_owned());
        Changes {
            files: changes
                .into_iter()
                .map(|(path, change)| ChangedFile { path, change })
                .collect(),
            note,
            incomplete: false,
        }
    }

    /// 执行进度里记录的、被允许写入的文件路径。
    fn written_paths(&self, run_id: &str) -> Vec<String> {
        let mut paths = Vec::new();
        let mut cursor = None;
        while let Ok(page) = self.run_updates(&RunUpdateQuery {
            run_id: run_id.to_owned(),
            cursor: cursor.clone(),
            limit: Some(store_run::MAX_UPDATE_PAGE),
        }) {
            for update in &page.updates {
                if let RunUpdateBody::FileWrite {
                    path,
                    allowed: true,
                    ..
                } = &update.body
                {
                    paths.push(path.clone());
                }
            }
            if !page.has_more {
                break;
            }
            cursor = Some(page.next_cursor);
        }
        paths
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
#[cfg(test)]
fn interrupt(conn: &rusqlite::Connection, run_id: &str, reason: &str) -> Result<()> {
    interrupt_with_changes(conn, run_id, reason, None)
}

/// 把活动执行标记为已中断（改状态与原因；`changes` 给出时一并写入改动文件与统计说明，G-6），
/// 保留其余记录；不是活动执行时不做任何事。
fn interrupt_with_changes(
    conn: &rusqlite::Connection,
    run_id: &str,
    reason: &str,
    changes: Option<&Changes>,
) -> Result<()> {
    let run = store_run::get(conn, run_id)?;
    if !run.status.is_active() {
        return Ok(());
    }
    let (files, note, incomplete) = match changes {
        Some(c) => (c.files.clone(), c.note.clone(), c.incomplete),
        None => (
            run.changed_files.clone(),
            run.change_note.clone(),
            run.change_incomplete,
        ),
    };
    let changed = conn.execute(
        "UPDATE runs SET status = 'interrupted', ended_at = ?2, failure = ?3, failure_kind = NULL,
                         pending_permission = NULL, changed_files = ?4, change_note = ?5,
                         change_incomplete = ?6
         WHERE id = ?1 AND status IN ('queued', 'running', 'waiting_permission')",
        rusqlite::params![
            run_id,
            now(),
            reason,
            serde_json::to_string(&files)?,
            note,
            incomplete
        ],
    )?;
    if changed == 0 {
        return Ok(());
    }
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
    /// diff 原文，用于判断对工作日志的编辑是否纯追加。
    edits: Vec<EditPreview>,
    /// rawInput 中能取到的路径（[`raw_input_paths`]），读取、搜索请求按它们与 locations 判断（D18）。
    raw_paths: Vec<String>,
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
    /// 退回续接时：被退回执行的（退回意见，摘要）。
    revision: Option<(String, Option<String>)>,
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
    /// Human 明确允许的编辑（`edit`）请求的目标路径（见 [`grants`]）：允许一次只放行一次写入，
    /// 本次执行内总是允许才持续有效。规则自动允许的请求、Human 允许的其他类型请求（读取、搜索、
    /// 执行命令等）都不会记入这里（Q-01）。
    granted: grants::Grants,
    /// Human 允许过的编辑、删除、移动请求涉及的路径（规范化）及类型，含「允许一次」已用掉的。只用于判断
    /// 快照发现的改动是否「未经确认」，不放开 `fs/write_text_file`。
    approved: Vec<(PathBuf, Approved)>,
    /// 普通文件夹原地执行：执行前的目录快照（S-1b）；Git worktree 执行为 `None`。
    baseline: Option<snapshot::Snapshot>,
    /// 本岗位文件夹（相对工作空间）：快照比对只排除这个文件夹里 Teamspace 维护的文件。
    own_role: Option<String>,
    /// 与快照有关的补充说明（如快照没能保存），并入改动统计说明。
    baseline_notes: Vec<String>,
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
            granted: grants::Grants::default(),
            approved: Vec::new(),
            baseline: None,
            own_role: None,
            baseline_notes: Vec::new(),
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
        // fencing（Q-03）：执行已被中断或结束时不生效，不把它改回活动。
        let applied =
            self.db(|tx| store_run::set_active_status(tx, &id, status, pending.as_ref(), count));
        if applied == Some(false) {
            return;
        }
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
        let revision = match &self.run.revision_of {
            Some(prev) => {
                let prev = store_run::get(&conn, prev)?;
                let comment = prev.review.and_then(|r| r.comment).unwrap_or_default();
                Some((comment, prev.summary))
            }
            None => None,
        };
        Ok(Context {
            role,
            role_folder,
            task,
            workspace,
            env_profile,
            dependencies,
            revision,
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
            let base_branch = self.core.review_git().base_branch(&wt);
            let id = self.run.id.clone();
            self.db(|tx| {
                store_run::set_exec(
                    tx,
                    &id,
                    &exec,
                    Some(&wt.branch),
                    Some(&wt.base_commit),
                    base_branch.as_deref(),
                )
            });
            self.run.base_branch = base_branch;
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
        self.scope = Some(Scope::with_workspace(
            &exec_dir,
            &ctx.workspace,
            self.run.isolation,
            &role_folders,
            &refs,
        ));
        if mode == Mode::Resume {
            self.restore_progress();
        }

        // 2b. 原地执行：对执行目录拍快照（续接时沿用存下的执行前快照），结束时比对出真实改动
        if self.run.isolation == RunIsolation::InPlace {
            self.own_role = ctx.role_folder.clone();
            let priority = priority_paths(&ctx);
            self.baseline = Some(self.prepare_baseline(mode, &exec_dir, &priority));
        }

        // 3. 启动 Agent
        let spec = self.spawn_spec(&ctx, &exec_dir)?;
        let session_vars = crate::system_probe::explicit_session_vars(&spec.env);
        if !session_vars.is_empty() {
            // 环境配置档案里显式设置了会让适配器拒绝启动的变量：照常传，但在执行记录里说清楚。
            self.insert(
                None,
                &RunUpdateBody::Warning {
                    text: format!(
                        "环境配置档案里显式设置了 {}：它们会原样传给 Agent，Claude Code 适配器可能因此\
                         拒绝启动（cannot be launched inside another Claude Code session）",
                        session_vars.join("、")
                    ),
                    files: Vec::new(),
                },
            );
        }
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
        // 退回续接：会话已建立且 Agent 支持 session/load 时续接原会话，否则在同一目录新开会话。
        let load = match mode {
            Mode::Start => false,
            Mode::Resume => true,
            Mode::Revise => self.run.session_id.is_some() && caps.load_session,
        };
        match load {
            false => {
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
                self.run.session_id = Some(session.clone());
                self.enforce_permission_mode(&session, &result)?;
            }
            true => {
                let session = self.run.session_id.clone().unwrap_or_default();
                let result = self.call(
                    "session/load",
                    json!({ "sessionId": session, "cwd": self.run.exec_dir, "mcpServers": mcp }),
                )?;
                self.discard_replay();
                self.enforce_permission_mode(&session, &result)?;
            }
        }
        if let Some(end) = self.aborted() {
            return Err(end);
        }

        // 6. 任务说明
        let folder = ctx
            .role_folder
            .clone()
            .unwrap_or_else(|| format!("{}/{}", role_folder::ROOT_DIR, ctx.role.name));
        let (comment, previous) = ctx.revision.clone().unwrap_or_default();
        let text = match (mode, load) {
            (Mode::Resume, _) => {
                brief::resume_text(&self.run.task_key, &folder, journal_path(&ctx).as_deref())
            }
            (Mode::Revise, true) => brief::revise_text(
                &self.run.task_key,
                &folder,
                journal_path(&ctx).as_deref(),
                &comment,
            ),
            (Mode::Revise, false) => format!(
                "{}\n\n{}",
                brief::render(&self.brief_input(&ctx, &exec_dir)).trim_end(),
                brief::revise_section(previous.as_deref(), &comment)
            ),
            (Mode::Start, _) => brief::render(&self.brief_input(&ctx, &exec_dir)),
        };
        let refs: Vec<brief::BriefRef> = if load {
            Vec::new()
        } else {
            self.brief_refs(&ctx, &exec_dir)
        };
        // 不可用的引用（不存在、越出工作空间）不附上内容，在执行记录里留一条说明（06a #2）。
        for r in refs.iter().filter(|r| r.unavailable.is_some()) {
            let detail = format!(
                "引用文件 {} 未附上：{}",
                r.path,
                r.unavailable.as_deref().unwrap_or_default()
            );
            let status = self.run.status;
            self.insert(
                None,
                &RunUpdateBody::Status {
                    status,
                    detail: Some(detail),
                },
            );
        }
        let blocks = brief::prompt_blocks(
            &text,
            &refs,
            caps.embedded_context,
            &[exec_dir.as_path(), ctx.workspace.as_path()],
        );
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

    /// 执行前快照：续接时读回存下的那一份，并以现在为界重定基线（中断到续接之间别人改的文件不算
    /// 本次执行的）；否则现在拍一份并存下来（应用重启后据此算出中断时的改动）。
    fn prepare_baseline(
        &mut self,
        mode: Mode,
        exec_dir: &Path,
        priority: &[String],
    ) -> snapshot::Snapshot {
        let path = self.core.snapshot_path(&self.run.id);
        let max = self.settings.snapshot_max_entries;
        if mode == Mode::Resume
            && let Some(saved) = snapshot::load(&path)
        {
            let current = snapshot::take(exec_dir, saved.max_entries, &saved.priority);
            let rebased = match snapshot::load(&self.core.interrupted_snapshot_path(&self.run.id)) {
                Some(interrupted) => snapshot::rebase_after_gap(
                    exec_dir,
                    &saved,
                    &interrupted,
                    &current,
                    self.own_role.as_deref(),
                ),
                None => {
                    let mut base = saved;
                    base.notes.push(
                        "没有中断时的快照，中断到续接期间别人改动的文件无法分清，可能被计入本次执行"
                            .to_owned(),
                    );
                    base
                }
            };
            if let Err(err) = snapshot::save(&path, &rebased) {
                self.baseline_notes
                    .push(format!("无法保存重定基线后的快照（{err}）"));
            }
            return rebased;
        }
        let taken = snapshot::take(exec_dir, max, priority);
        if let Err(err) = snapshot::save(&path, &taken) {
            eprintln!("teamspace run {}: 无法保存执行前快照：{err}", self.run.id);
            self.baseline_notes.push(format!(
                "执行前的快照没有保存成功（{err}）：本次执行仍会比对改动，但应用中途退出后无法据此统计"
            ));
        }
        taken
    }

    fn brief_refs(&self, ctx: &Context, exec_dir: &Path) -> Vec<brief::BriefRef> {
        ctx.task
            .file_refs
            .iter()
            .map(|r| {
                let in_exec = exec_dir.join(&r.path);
                let absolute = if in_exec.exists() {
                    in_exec
                } else {
                    ctx.workspace.join(&r.path)
                };
                brief::BriefRef {
                    path: r.path.clone(),
                    anchor: r.anchor.clone(),
                    unavailable: brief::ref_problem(&absolute, &[exec_dir, &ctx.workspace]),
                    absolute,
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
            journal_path: journal_path(ctx),
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
            Some(p) => {
                let exit = p.exit_status(EXIT_WAIT);
                // 进程已退出：等 stderr 读取线程结束，缓冲区才完整。
                p.wait_stderr();
                (exit, p.stderr_tail())
            }
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
                                edits: Vec::new(),
                                raw_paths: Vec::new(),
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
        let edits: Vec<EditPreview> = update
            .get("content")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter(|c| c.get("type").and_then(Value::as_str) == Some("diff"))
            .filter_map(|c| {
                Some(EditPreview {
                    path: c.get("path")?.as_str()?.to_owned(),
                    old_text: c.get("oldText").and_then(Value::as_str).map(str::to_owned),
                    new_text: c
                        .get("newText")
                        .and_then(Value::as_str)
                        .unwrap_or_default()
                        .to_owned(),
                })
            })
            .collect();
        if let Some(diffs) = &diffs {
            let list = paths.get_or_insert_with(Vec::new);
            for d in diffs {
                if !list.contains(&d.path) {
                    list.push(d.path.clone());
                }
            }
        }
        let command = update.get("rawInput").and_then(command_of);
        let raw_paths = update
            .get("rawInput")
            .map(raw_input_paths)
            .unwrap_or_default();
        let existed = self.tools.contains_key(&id);
        let state = self.tools.entry(id.clone()).or_insert_with(|| ToolState {
            update_id: String::new(),
            title: String::new(),
            kind: None,
            status: None,
            paths: Vec::new(),
            command: None,
            diffs: Vec::new(),
            edits: Vec::new(),
            raw_paths: Vec::new(),
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
        let finished = matches!(state.status.as_deref(), Some("completed" | "failed"));
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
        if !raw_paths.is_empty() {
            state.raw_paths = raw_paths;
        }
        if !edits.is_empty() {
            state.edits = edits;
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
        if finished {
            // 工具调用结束：它未用掉的「允许一次」失效。
            self.granted.expire_call(&id);
        }
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
        let (title, kind, paths, command, edits) = match &state {
            Some(s) => (
                s.title.clone(),
                s.kind.clone(),
                s.paths.clone(),
                s.command.clone(),
                s.edits.clone(),
            ),
            None => ("工具调用".to_owned(), None, Vec::new(), None, Vec::new()),
        };
        // 读取、搜索请求：rawInput 里的路径（如 Grep 的搜索目录）一并判断，也展示给 Human（D18）。
        let mut paths = paths;
        if matches!(kind.as_deref(), Some("read" | "search"))
            && let Some(s) = &state
        {
            for raw in &s.raw_paths {
                if !paths.contains(raw) {
                    paths.push(raw.clone());
                }
            }
        }
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
        let reason = match self.scope().decide_with(kind.as_deref(), &paths, &edits) {
            Verdict::Allow(reason) => {
                let allow = ["allow_once", "allow_always"]
                    .iter()
                    .find_map(|k| options.iter().find(|o| o.kind == *k));
                if let Some(option) = allow {
                    // 规则自动允许不授予写权限：编辑本就在范围内，读取与搜索不能变成可写（Q-01）。
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

    /// 记录 Human 对编辑请求的允许。目标路径以这次编辑的 diff 为准；没有 diff 时逐个收下请求里的路径。
    fn grant(&mut self, prompt: &PermissionPrompt, always: bool) {
        let base = self.scope().exec_dir().to_path_buf();
        let edits: Vec<String> = self
            .tools
            .get(&prompt.tool_call_id)
            .map(|t| t.edits.iter().map(|e| e.path.clone()).collect())
            .unwrap_or_default();
        let targets = if edits.is_empty() {
            &prompt.paths
        } else {
            &edits
        };
        let real: Vec<PathBuf> = targets
            .iter()
            .filter_map(|path| policy::resolve(Path::new(path), &base).ok())
            .collect();
        self.granted.add(&prompt.tool_call_id, real, always);
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
        // 只有 Human 对编辑请求本身的允许才放开写入；允许执行命令、读取等不让路径变成可写（Q-01/N-02）。
        if option.kind.starts_with("allow") && p.prompt.tool_kind.as_deref() == Some("edit") {
            self.grant(&p.prompt, option.kind == "allow_always");
        }
        // Human 允许读取、搜索工作空间之外的路径：之后的 fs/read_text_file 可以读取（D18），不能写入。
        if option.kind.starts_with("allow")
            && matches!(p.prompt.tool_kind.as_deref(), Some("read" | "search"))
        {
            let base = self.scope().exec_dir().to_path_buf();
            let real: Vec<PathBuf> = p
                .prompt
                .paths
                .iter()
                .filter_map(|path| policy::resolve(Path::new(path), &base).ok())
                .collect();
            self.granted
                .add_read(&p.prompt.tool_call_id, real, option.kind == "allow_always");
        }
        // Human 明确允许过对这些路径的编辑、删除或移动：快照发现的同路径改动不算「未经确认」。
        if option.kind.starts_with("allow")
            && matches!(
                p.prompt.tool_kind.as_deref(),
                Some("edit" | "delete" | "move")
            )
        {
            let base = self.scope().exec_dir().to_path_buf();
            let edit_targets: Vec<String> = self
                .tools
                .get(&p.prompt.tool_call_id)
                .map(|t| t.edits.iter().map(|e| e.path.clone()).collect())
                .unwrap_or_default();
            let kind = if p.prompt.tool_kind.as_deref() == Some("edit") {
                Approved::Edit
            } else {
                Approved::Remove
            };
            for path in p.prompt.paths.iter().chain(&edit_targets) {
                if let Ok(real) = policy::resolve(Path::new(path), &base) {
                    self.approved.push((real, kind));
                }
            }
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
        let base = self.scope().exec_dir().to_path_buf();
        // 工作空间之外：须有 Human 允许的读取授权（D18），允许一次只读一次；写入授权不放行读取。
        let allowed = self.scope().can_read(path).or_else(|reason| {
            match policy::resolve(Path::new(path), &base) {
                Ok(real) if self.granted.take_read(&real).is_some() => Ok(real),
                _ => Err(reason),
            }
        });
        let result = allowed.and_then(|real| {
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
        let outcome = match self.scope().write_target(&path) {
            Ok(WriteTarget::Journal(real)) => self.write_journal(real, &path, content),
            Ok(WriteTarget::Free(real)) => self.write_plain(real, content, "在自动批准范围内"),
            Err(reason) => match policy::resolve(Path::new(&path), &base) {
                Ok(real) => match self.granted.take(&real) {
                    Some(used) => self.write_plain(real, content, used.reason()),
                    None => Err(reason),
                },
                Err(_) => Err(reason),
            },
        };
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

    /// 写入范围内（或已被允许）的普通文件，记入改动。
    fn write_plain(
        &mut self,
        real: PathBuf,
        content: &str,
        reason: &str,
    ) -> std::result::Result<String, String> {
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
        Ok(reason.to_owned())
    }

    /// 写本岗位工作日志（D14）：每次都在写入锁内重新读取并判断是否纯追加，纯追加只追加多出的部分；
    /// 非追加写入只在 Human 允许过对该路径的编辑时整体写入，且每次允许只放行一次改写。
    fn write_journal(
        &mut self,
        real: PathBuf,
        path: &str,
        content: &str,
    ) -> std::result::Result<String, String> {
        let by_human = self.granted.has(&real);
        match role_folder::write_journal_from_agent(&real, content, by_human) {
            // 追加进展不算任务产出，不记入改动文件。
            Ok(role_folder::AgentJournalWrite::Appended(_)) => {
                Ok("在工作日志末尾追加，自动允许".to_owned())
            }
            Ok(role_folder::AgentJournalWrite::Overwritten) => {
                self.granted.take_once(&real);
                self.writes.entry(real).or_insert(FileChangeKind::Modified);
                Ok(grants::GrantUse::Once.reason().to_owned())
            }
            Ok(role_folder::AgentJournalWrite::NotAppend) => Err(format!(
                "工作日志只允许在末尾追加（新内容须以文件现有内容开头），\
                 修改或删除已有内容需要确认；请重新读取工作日志后再追加：{path}"
            )),
            Err(err) => Err(format!("写入失败：{err}")),
        }
    }

    // ------------------------------------------------------------ 结束

    /// 本次执行的改动统计：Git worktree 用 Git；原地执行取 ACP 报告的改动与快照比对结果的并集，
    /// 并找出快照发现的范围外改动。
    fn audit_changes(&self) -> Audit {
        if self.run.isolation == RunIsolation::Worktree {
            let Some(base) = &self.run.base_commit else {
                return Audit::default();
            };
            let path_env = self.core.probe.command_path_env();
            return match worktree::changed_files(
                Path::new(&self.run.exec_dir),
                base,
                Some(&path_env),
            ) {
                Ok(files) => Audit {
                    files,
                    ..Audit::default()
                },
                Err(err) => Audit {
                    note: Some(format!("无法统计 worktree 的改动：{err}")),
                    ..Audit::default()
                },
            };
        }
        let root = self
            .workspace
            .canonicalize()
            .unwrap_or_else(|_| self.workspace.clone());
        let mut changes: BTreeMap<String, FileChangeKind> = BTreeMap::new();
        for (path, kind) in &self.writes {
            changes.insert(relative_to(&root, path), *kind);
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
                // 工作日志的追加不算改动文件（Human 允许的整体改写已在 `writes` 中）。
                let journal = self
                    .scope
                    .as_ref()
                    .is_some_and(|s| matches!(s.write_target(path), Ok(WriteTarget::Journal(_))));
                if journal {
                    continue;
                }
                if let Ok(real) = policy::resolve(Path::new(path), &root) {
                    changes.entry(relative_to(&root, &real)).or_insert(kind);
                }
            }
        }
        // 快照比对：不依赖 Agent 自觉，shell 直接写的文件也在内。
        let mut out_of_scope = Vec::new();
        let mut notes: Vec<String> = self.baseline_notes.clone();
        let mut incomplete = false;
        if let Some(before) = &self.baseline {
            let after = snapshot::take_light(&root, before.max_entries, &before.priority);
            let diff = snapshot::diff(&root, before, &after, self.own_role.as_deref());
            incomplete = diff.incomplete;
            notes.extend(diff.note);
            for change in diff.changes {
                if self.is_out_of_scope(&root, &change.path, change.change) {
                    out_of_scope.push(change.path.clone());
                }
                // 快照看到的是真实的新增 / 修改 / 删除，覆盖 ACP 报告里粗略的类型。
                changes.insert(change.path, change.change);
            }
        }
        out_of_scope.sort();
        Audit {
            files: changes
                .into_iter()
                .map(|(path, change)| ChangedFile { path, change })
                .collect(),
            out_of_scope,
            note: (!notes.is_empty()).then(|| notes.join("；")),
            incomplete,
        }
    }

    /// 快照发现的改动（相对 `root`）是否落在权限范围外：
    /// - 删除：只有 Human 允许过针对该路径的删除或移动才算范围内（允许过编辑不等于允许删除）；
    /// - 其他改动：在本岗位文件夹或任务引用文件里（[`Scope::write_target`] 放行），或 Human 允许过
    ///   对该路径的编辑（含「允许一次」已用掉的）、删除、移动。
    fn is_out_of_scope(&self, root: &Path, rel: &str, change: FileChangeKind) -> bool {
        let Some(scope) = &self.scope else {
            return false;
        };
        let abs = root.join(rel);
        let real = policy::resolve(&abs, scope.exec_dir()).ok();
        let approved = |kinds: &[Approved]| {
            real.as_ref().is_some_and(|real| {
                self.approved
                    .iter()
                    .any(|(path, kind)| path == real && kinds.contains(kind))
            })
        };
        if change == FileChangeKind::Deleted {
            return !approved(&[Approved::Remove]);
        }
        if scope.write_target(&abs.display().to_string()).is_ok() {
            return false;
        }
        match &real {
            Some(real) => {
                !(self.granted.has(real) || approved(&[Approved::Edit, Approved::Remove]))
            }
            None => true,
        }
    }

    /// 把 worktree 里的改动提交到执行分支（英文提交信息，含任务编号与执行序号）；返回失败说明。
    ///
    /// fencing：持执行锁，先确认本执行在数据库里仍是活动执行（没有被中断、接管或由别处结束）才提交；
    /// 中断与续接也要取同一把锁，因此旧 driver 的提交不会晚于它们落到执行分支上。
    fn commit_run_changes(&self) -> Option<String> {
        let _exec = self.core.runs.exec_lock();
        let current =
            store_run::get(&self.core.lock(), &self.run.id).is_ok_and(|run| run.status.is_active());
        if !current {
            return None;
        }
        let message = format!(
            "teamspace: {} attempt {}",
            self.run.task_key, self.run.attempt
        );
        match self.core.commit_run_dir(&self.run, &message) {
            Ok(_) => None,
            Err(err) => Some(format!(
                "无法把改动提交到执行分支：{err}；打开验收页时会再试一次"
            )),
        }
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
        // Git 执行成功：转待验收之前把执行目录里的改动提交到执行分支（验收的 diff 与合并都基于分支）。
        // 提交失败时任务照常转待验收，问题写进执行记录（验收通过时还会再提交一次）。
        let commit_problem = match &end {
            End::Succeeded(_) if self.run.isolation == RunIsolation::Worktree => {
                self.commit_run_changes()
            }
            _ => None,
        };
        let mut audit = self.audit_changes();
        if let Some(problem) = &commit_problem {
            audit.note = Some(match audit.note.take() {
                Some(n) => format!("{n}；{problem}"),
                None => problem.clone(),
            });
        }
        let mut ending = Ending {
            summary,
            changed_files: audit.files.clone(),
            out_of_scope: audit.out_of_scope.clone(),
            change_note: audit.note.clone(),
            change_incomplete: audit.incomplete,
            tool_call_count: self.tool_call_count,
            ..Ending::default()
        };
        let scope_text = out_of_scope_text(&audit.out_of_scope);
        let (status, task_to, reason, actor, verb) = match &end {
            End::Succeeded(stop) => {
                ending.stop_reason = Some(stop.clone());
                if audit.out_of_scope.is_empty() {
                    (
                        RunStatus::Succeeded,
                        Some(TaskStatus::Review),
                        None,
                        Actor::System,
                        "完成",
                    )
                } else {
                    // 执行成功，但 Agent 未经确认改了权限范围外的文件：不进入待验收，交给 Human 检查。
                    let reason: String = format!(
                        "Agent 未经确认改动了权限范围外的文件：{}，请检查后处理",
                        file_list(&audit.out_of_scope)
                    )
                    .chars()
                    .take(MAX_REASON)
                    .collect();
                    (
                        RunStatus::Succeeded,
                        Some(TaskStatus::Blocked),
                        Some(reason),
                        Actor::System,
                        "完成，但有范围外改动",
                    )
                }
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
        let out_of_scope = audit.out_of_scope.clone();
        let saved = self.db(|tx| {
            // fencing（Q-03）：执行已被中断（或已由别处结束）时，迟到的结局不改写执行、不推进任务、
            // 不记活动。
            if !store_run::end(tx, &run.id, status, &ending)? {
                return Ok(None);
            }
            let mut updates = Vec::new();
            if let Some(problem) = &commit_problem {
                updates.push(store_run::insert_update(
                    tx,
                    &run.id,
                    None,
                    &RunUpdateBody::Warning {
                        text: problem.clone(),
                        files: Vec::new(),
                    },
                )?);
            }
            if !out_of_scope.is_empty() {
                // 执行记录里醒目地标出范围外改动（所有结局都标，不止成功的）。
                updates.push(store_run::insert_update(
                    tx,
                    &run.id,
                    None,
                    &RunUpdateBody::Warning {
                        text: format!(
                            "Agent 未经确认改动了范围外的文件（执行中没有收到对应的权限请求，改动已落在\
                             工作空间里，不会自动回滚）：{}",
                            file_list(&out_of_scope)
                        ),
                        files: out_of_scope.clone(),
                    },
                )?);
            }
            updates.push(store_run::status_update(
                tx,
                &run.id,
                status,
                detail.clone(),
            )?);
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
                    "{} 第 {} 次执行{verb}（{}）{scope_text}",
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
                    "outOfScope": out_of_scope,
                }),
            )?;
            if auth {
                mark_needs_auth(tx, &run, &auth_methods, capabilities, detail.as_deref())?;
            }
            Ok(Some(updates))
        });
        if let Some(Some(updates)) = &saved {
            for update in updates {
                self.emit_update(update.clone());
            }
        }
        // 执行确实在这里结束了，就不再需要快照（被中断的执行才保留，以便续接）。结局被 fencing 忽略
        // （执行已被中断或别处结束）时保留，别的路径可能正在读它，由启动时的清理处理。
        if matches!(saved, Some(Some(_))) {
            self.core.remove_snapshots(&self.run.id);
        }
        self.core.sync_role_folders(&self.run.team_id);
        self.core.emit_run(&self.run.id);
    }
}

/// Human 允许过的操作类型（只用于判断快照发现的改动是否「未经确认」）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Approved {
    /// 编辑。
    Edit,
    /// 删除或移动。
    Remove,
}

/// 保留中断执行的快照的最长时间。
const SNAPSHOT_KEEP: Duration = Duration::from_secs(30 * 24 * 3600);

/// 中断时算出的改动统计。
#[derive(Debug, Clone, Default)]
pub(crate) struct Changes {
    pub files: Vec<ChangedFile>,
    pub note: Option<String>,
    pub incomplete: bool,
}

/// 降级时优先完整扫描的路径：本岗位文件夹和任务引用所在的目录（文件夹引用就是它本身；根目录下的
/// 文件引用就是该文件）。
fn priority_paths(ctx: &Context) -> Vec<String> {
    let mut paths: Vec<String> = ctx.role_folder.iter().cloned().collect();
    for r in &ctx.task.file_refs {
        let rel = r.path.trim_matches('/');
        if rel.is_empty() {
            continue;
        }
        if ctx.workspace.join(rel).is_dir() {
            paths.push(rel.to_owned());
        } else {
            match rel.rsplit_once('/') {
                Some((parent, _)) => paths.push(parent.to_owned()),
                None => paths.push(rel.to_owned()),
            }
        }
    }
    paths
}

/// 一次执行的改动统计结果。
#[derive(Debug, Default)]
struct Audit {
    files: Vec<ChangedFile>,
    /// 改动统计不完整（快照降级）。
    incomplete: bool,
    /// 快照发现的范围外改动（相对工作空间，已排序）。
    out_of_scope: Vec<String>,
    /// 统计的补充说明（快照降级等）。
    note: Option<String>,
}

/// 路径相对 `root` 的 `/` 分隔形式；不在 `root` 之下的保持原样。
fn relative_to(root: &Path, path: &Path) -> String {
    match path.strip_prefix(root) {
        Ok(rel) => rel
            .components()
            .map(|c| c.as_os_str().to_string_lossy())
            .collect::<Vec<_>>()
            .join("/"),
        Err(_) => path.display().to_string(),
    }
}

/// 摘要用的文件清单：最多列出 5 个，其余用「等 N 个文件」。
fn file_list(files: &[String]) -> String {
    const SHOWN: usize = 5;
    let mut text = files
        .iter()
        .take(SHOWN)
        .cloned()
        .collect::<Vec<_>>()
        .join("、");
    if files.len() > SHOWN {
        text.push_str(&format!("等 {} 个文件", files.len()));
    }
    text
}

/// 追加在执行完成活动后面的范围外改动说明；没有范围外改动时为空。
fn out_of_scope_text(files: &[String]) -> String {
    if files.is_empty() {
        String::new()
    } else {
        format!("；Agent 未经确认改动了范围外的文件：{}", file_list(files))
    }
}

/// 工作空间中本岗位工作日志的绝对路径（岗位文件夹尚未建立时为 `None`）。
fn journal_path(ctx: &Context) -> Option<String> {
    ctx.role_folder.as_deref().map(|rel| {
        role_folder::absolute(&ctx.workspace, rel)
            .join(role_folder::JOURNAL_FILE)
            .display()
            .to_string()
    })
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

/// rawInput 中常见的路径字段（`path`、`file_path`、`filePath`、`abs_path`、`absPath`、`paths` 数组）。
fn raw_input_paths(raw: &Value) -> Vec<String> {
    let mut out = Vec::new();
    for key in ["path", "file_path", "filePath", "abs_path", "absPath"] {
        if let Some(p) = raw.get(key).and_then(Value::as_str)
            && !p.is_empty()
        {
            out.push(p.to_owned());
        }
    }
    if let Some(list) = raw.get("paths").and_then(Value::as_array) {
        out.extend(
            list.iter()
                .filter_map(Value::as_str)
                .filter(|p| !p.is_empty())
                .map(str::to_owned),
        );
    }
    out
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
