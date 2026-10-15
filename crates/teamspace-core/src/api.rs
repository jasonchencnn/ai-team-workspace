//! IPC 命令清单。
//!
//! [`Teamspace`] 的每个公开方法对应一个同名 Tauri 命令（M1-06 包装为 `#[tauri::command]`），
//! 前端唯一入口是 `apps/desktop/src/api/commands.ts`。
//!
//! 参数约定：ID 参数在前端以 camelCase 传入（`team_id` → `teamId`）；结构体参数统一命名为
//! `input`。所有命令失败时返回 [`crate::Error`]（前端收到 `ErrorPayload`）。
//!
//! 文件系统和外部进程访问（工作空间文件存在性、Git、ACP Agent 握手、岗位文件夹读写）都在数据库锁之外完成。
//!
//! 所有写操作在同一事务内完成数据变更并追加一条 [`crate::model::Event`]（actor 为 human，中文摘要由后端生成）。
//!
//! 岗位文件夹（D14，见 [`crate::role_folder`]）：涉及团队的写命令在事务提交、释放数据库锁之后同步
//! 该团队的岗位文件夹（建立、改名、重写岗位说明、追加工作日志）；打开团队概览和读取岗位文件夹时
//! 也会同步一次，从而为迁移前已有的岗位补建文件夹。文件写入失败不回滚已提交的数据，失败原因记入
//! 岗位视图的 `folderProblem` 并记录一条 `role_folder_error` 活动。

use std::collections::HashSet;
use std::ops::Deref;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard, RwLock};

use rusqlite::Connection;

use crate::acp::AgentLaunch;
use crate::error::{Error, Result};
use crate::model::*;
use crate::probe::FILE_SEARCH_LIMIT;
use crate::probe::SystemProbe;
use crate::role_folder;
use crate::run::review_seam::{self, Cleanup, ReviewGit};
use crate::run::{Mode, Stop};
use crate::service::run::{self as run_svc, Ending, NewRun};
use crate::service::task::FileCheck;
use crate::service::{event, folder, harness, review, role, task, team, workspace};
use crate::store;

/// 只在 Tauri 层实现、不经过 [`Teamspace`] 的命令。
pub const TAURI_ONLY_COMMANDS: &[&str] = &[
    // 打开系统文件夹对话框；用户取消返回 `null`，否则返回所选目录的工作区信息
    // （等同于对所选目录调用 `inspect_workspace`）。返回类型 `WorkspaceInfo | null`。
    "pick_workspace_folder",
];

/// [`Teamspace`] 对外的全部命令名，顺序同下方方法定义。
pub const COMMANDS: &[&str] = &[
    "list_teams",
    "create_team",
    "get_team_overview",
    "inspect_workspace",
    "list_role_templates",
    "suggest_task_prefix",
    "list_roles",
    "create_role",
    "update_role",
    "archive_role",
    "bind_role_harness",
    "unbind_role_harness",
    "get_role_folder",
    "reveal_role_folder",
    "get_role_workbench",
    "open_role_path",
    "list_acp_presets",
    "list_harnesses",
    "get_harness",
    "create_harness",
    "update_harness",
    "archive_harness",
    "check_harness",
    "list_task_statuses",
    "get_task_board",
    "get_task",
    "create_task",
    "update_task",
    "change_task_status",
    "search_workspace_files",
    "start_run",
    "cancel_run",
    "resume_run",
    "mark_run_failed",
    "takeover_run",
    "open_run_dir",
    "open_run_file",
    "get_run",
    "list_runs",
    "list_run_updates",
    "respond_permission",
    "get_review",
    "get_review_file_patch",
    "approve_review",
    "reject_review",
    "resolve_merge_manually",
    "rerun_after_conflict",
    "list_events",
];

/// 执行事件的 Tauri 事件名，载荷为 [`RunEvent`]（见 [`Teamspace::set_run_listener`]）。
pub const RUN_EVENT: &str = "teamspace:run";

/// 岗位工作台「最近执行」的条数。
pub const WORKBENCH_RUN_LIMIT: u32 = 5;

/// 岗位工作台「最近任务」的条数。
pub const WORKBENCH_TASK_LIMIT: usize = 8;
/// 岗位工作台「最近文件」的条数。
pub const WORKBENCH_FILE_LIMIT: usize = 10;

/// 启动一个程序的命令（可执行文件路径 + 参数）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentCommand {
    pub program: PathBuf,
    pub args: Vec<String>,
}

/// 宿主程序（桌面应用、端到端测试桥接）提供给内核的配置。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct HostConfig {
    /// 启动内置 Mock Agent 的命令，通常是宿主程序自身路径 +
    /// [`crate::mock_agent::MOCK_AGENT_FLAG`]。为 `None` 时 Mock 预设不可用（握手报错）。
    pub mock_agent: Option<AgentCommand>,
}

impl HostConfig {
    /// 以当前可执行文件作为 Mock Agent 宿主（宿主程序的 `main` 需要处理
    /// [`crate::mock_agent::MOCK_AGENT_FLAG`]）。取不到当前可执行文件路径时不提供 Mock Agent。
    pub fn with_current_exe_mock_agent() -> Self {
        Self {
            mock_agent: std::env::current_exe().ok().map(|program| AgentCommand {
                program,
                args: vec![crate::mock_agent::MOCK_AGENT_FLAG.to_owned()],
            }),
        }
    }
}

/// 内核入口：持有 SQLite 存储、系统探测实现和执行引擎（见 [`Core`]）。
///
/// 数据库连接由互斥锁保护；写命令在单个事务中执行。系统探测（Git、ACP 握手）在锁外进行，
/// 避免慢操作阻塞其他命令。执行（Run）由后台线程驱动，与命令共享同一个 [`Core`]。
///
/// drop 时（或调用 [`Teamspace::shutdown`]）终止所有执行中的 Agent 进程，并把这些执行标记为已中断。
pub struct Teamspace {
    core: Arc<Core>,
}

/// 内核共享状态：命令与执行线程共用。
pub struct Core {
    conn: Mutex<Connection>,
    pub(crate) probe: Box<dyn SystemProbe>,
    pub(crate) config: HostConfig,
    /// 串行化岗位文件夹的文件 I/O（与数据库锁分开），避免并发同步重复追加日志或抢同一个文件夹名。
    folder_io: Mutex<()>,
    /// 应用数据目录（数据库所在目录），worktree 建在其下的 `worktrees/`。
    pub(crate) data_dir: PathBuf,
    /// 执行引擎状态。
    pub(crate) runs: crate::run::Runs,
    /// 验收用的 Git 层（测试可替换，见 [`review_seam`]）。
    review_git: RwLock<Arc<dyn ReviewGit>>,
    /// 正在处理验收（通过 / 退回）的任务：合并在数据库锁之外进行，同一任务的并发验收在这里拒绝。
    reviewing: Mutex<HashSet<String>>,
}

impl Deref for Teamspace {
    type Target = Core;

    fn deref(&self) -> &Core {
        &self.core
    }
}

#[cfg(test)]
impl Teamspace {
    /// 仅测试用：取得内核共享状态的句柄，用于在 crate 内部构造 Driver 等对象。
    pub(crate) fn core_handle(&self) -> Arc<Core> {
        Arc::clone(&self.core)
    }
}

impl Drop for Teamspace {
    fn drop(&mut self) {
        self.core.shutdown_runs();
    }
}

impl Teamspace {
    /// 打开（必要时创建并迁移）`db_path` 处的数据库，不提供 Mock Agent。
    pub fn open(db_path: &Path, probe: Box<dyn SystemProbe>) -> Result<Self> {
        Self::open_with(db_path, probe, HostConfig::default())
    }

    /// 打开数据库，并使用宿主程序提供的配置（如内置 Mock Agent 的启动命令）。
    ///
    /// 打开时把上次退出时仍在进行的执行标记为已中断（任务保持执行中，可续接、重新执行或标记失败）。
    pub fn open_with(
        db_path: &Path,
        probe: Box<dyn SystemProbe>,
        config: HostConfig,
    ) -> Result<Self> {
        let data_dir = db_path
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_else(|| PathBuf::from("."));
        let core = Arc::new(Core {
            conn: Mutex::new(store::open(db_path)?),
            probe,
            config,
            folder_io: Mutex::new(()),
            data_dir,
            runs: crate::run::Runs::default(),
            review_git: RwLock::new(Arc::new(review_seam::RealGit)),
            reviewing: Mutex::new(HashSet::new()),
        });
        core.recover_runs()?;
        Ok(Self { core })
    }

    /// 设置执行事件的接收者（宿主程序把事件推给前端，见 [`RUN_EVENT`]）。回调在执行线程中调用，
    /// 应尽快返回。
    pub fn set_run_listener(&self, listener: impl Fn(&RunEvent) + Send + Sync + 'static) {
        self.core.runs.set_listener(Arc::new(listener));
    }

    /// 调整执行引擎的超时设置（宿主程序与测试用）。
    pub fn set_run_settings(&self, settings: crate::run::RunSettings) {
        self.core.runs.set_settings(settings);
    }

    /// 终止所有执行中的 Agent 进程并把这些执行标记为已中断（应用退出时调用；drop 时也会调用）。
    pub fn shutdown(&self) {
        self.core.shutdown_runs();
    }
}

/// 正在验收某任务的标记；drop 时释放。
pub(crate) struct ReviewGuard<'a> {
    set: &'a Mutex<HashSet<String>>,
    task_id: String,
}

impl Drop for ReviewGuard<'_> {
    fn drop(&mut self) {
        self.set
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .remove(&self.task_id);
    }
}

impl Core {
    /// 验收用的 Git 层。
    pub(crate) fn review_git(&self) -> Arc<dyn ReviewGit> {
        Arc::clone(&self.review_git.read().unwrap_or_else(|p| p.into_inner()))
    }

    /// 仅测试用：替换验收用的 Git 层。
    #[cfg(test)]
    pub(crate) fn set_review_git(&self, git: Arc<dyn ReviewGit>) {
        *self.review_git.write().unwrap_or_else(|p| p.into_inner()) = git;
    }

    /// 开始处理某任务的验收；同一任务已在处理时为 conflict。
    fn begin_review(&self, task_id: &str, key: &str) -> Result<ReviewGuard<'_>> {
        let mut set = self.reviewing.lock().unwrap_or_else(|p| p.into_inner());
        if !set.insert(task_id.to_owned()) {
            return Err(Error::conflict(format!(
                "任务 {key} 正在处理验收，请稍候刷新"
            )));
        }
        Ok(ReviewGuard {
            set: &self.reviewing,
            task_id: task_id.to_owned(),
        })
    }

    /// worktree 根目录：执行目录在 `<数据目录>/worktrees/<团队>/<任务>-r<序号>[-n]/` 之下（工作空间是仓库
    /// 子目录时执行目录是它的子目录）；不在这个结构下时就是执行目录本身。
    fn worktree_root(&self, run: &Run) -> PathBuf {
        let base = self.data_dir.join("worktrees").join(&run.team_id);
        let exec = Path::new(&run.exec_dir);
        exec.ancestors()
            .find(|a| a.parent() == Some(base.as_path()))
            .unwrap_or(exec)
            .to_path_buf()
    }

    pub(crate) fn lock(&self) -> MutexGuard<'_, Connection> {
        // 持锁线程 panic 时事务已回滚，连接仍可用。
        self.conn
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    /// 在单个事务中执行写操作；出错时回滚。
    pub(crate) fn write<T>(&self, f: impl FnOnce(&Connection) -> Result<T>) -> Result<T> {
        let mut conn = self.lock();
        let tx = conn.transaction()?;
        let value = f(&tx)?;
        tx.commit()?;
        Ok(value)
    }

    /// 同步团队全部岗位的文件夹（见模块说明）。调用时不得持有数据库锁：读计划和写回结果各短暂
    /// 加锁一次，文件 I/O 在锁外进行。同步本身的失败只体现在 `folderProblem`，不让命令失败。
    pub(crate) fn sync_role_folders(&self, team_id: &str) {
        let _io = self
            .folder_io
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let Ok((workspace, mut plans)) = folder::plans(&self.lock(), team_id) else {
            return;
        };
        let mut outcomes = Vec::with_capacity(plans.len());
        for i in 0..plans.len() {
            let outcome = role_folder::sync(&workspace, &plans[i]);
            // 本轮新占用的文件夹名对后面的岗位同样不可用。
            if let Some(chosen) = &outcome.folder {
                for later in &mut plans[i + 1..] {
                    later.others.push(chosen.clone());
                }
            }
            outcomes.push(outcome);
        }
        // 写回失败（数据库故障）时下一次同步会重做：日志序号未推进，文件夹按标记识别为本岗位所有。
        let _ = self.write(|tx| folder::apply(tx, &plans, &outcomes));
    }

    /// 重新读取岗位卡片（文件夹同步之后，带上最新的 `folder` / `folderProblem`）。
    fn role_view(&self, role_id: &str) -> Result<RoleView> {
        let conn = self.lock();
        role::view(&conn, role::get_active(&conn, role_id)?)
    }

    /// 规范化工作空间目录并实时探测：目录须存在；是 Git 仓库（或其子目录）时附带分支与提交。
    /// 返回的 `path` 始终是规范化后的所选目录本身。
    pub(crate) fn inspect(&self, path: &str) -> Result<WorkspaceInfo> {
        let dir = workspace::resolve(path)?;
        let mut info = self.probe.inspect_workspace(&dir)?;
        info.path = workspace::display(&dir);
        Ok(info)
    }

    /// 实例的启动信息；Mock 预设使用宿主注入的命令。
    pub(crate) fn launch_for(
        &self,
        preset: AcpPreset,
        command: &str,
        args: &[String],
        env_profile: Option<&str>,
    ) -> std::result::Result<AgentLaunch, String> {
        if preset == AcpPreset::Mock {
            let cmd = self
                .config
                .mock_agent
                .as_ref()
                .ok_or_else(|| "当前宿主程序没有提供内置 Mock Agent".to_owned())?;
            return Ok(AgentLaunch {
                command: cmd.program.display().to_string(),
                args: cmd.args.clone(),
                env_profile: None,
            });
        }
        Ok(AgentLaunch {
            command: command.to_owned(),
            args: args.to_vec(),
            env_profile: env_profile.map(str::to_owned),
        })
    }
}

impl Teamspace {
    // ---------------------------------------------------------------- 团队

    /// 列出未归档团队，按创建时间升序。
    pub fn list_teams(&self) -> Result<Vec<Team>> {
        team::list(&self.lock())
    }

    /// 创建团队及初始岗位：名称必填且在未归档团队中唯一；工作空间为任意已存在的目录
    /// （规范化为绝对路径，不要求是 Git 仓库，选中仓库子目录时不提升为仓库根）；
    /// 初始岗位可以为空，逐个按岗位规则校验（code 缺省自动生成、前缀唯一、至多一个 Leader）。
    pub fn create_team(&self, input: CreateTeamInput) -> Result<Team> {
        team::validate_name(&input.name)?;
        let workspace = self.inspect(&input.workspace_path)?;
        let team = self.write(|tx| team::create(tx, input, &workspace))?;
        self.sync_role_folders(&team.id);
        Ok(team)
    }

    /// 团队概览：团队、实时工作空间状态（检测失败不报错，写入 `workspaceProblem`）、岗位卡片、
    /// 各状态任务数、进行中的任务和最近 10 条活动。打开概览时先同步岗位文件夹（补建缺失的文件夹）。
    pub fn get_team_overview(&self, team_id: &str) -> Result<TeamOverview> {
        self.sync_role_folders(team_id);
        let mut overview = team::overview(&self.lock(), team_id)?;
        match self.inspect(&overview.team.workspace_path) {
            Ok(info) => overview.workspace = Some(info),
            Err(err) => overview.workspace_problem = Some(err.to_string()),
        }
        Ok(overview)
    }

    /// 检测某个目录作为工作空间的信息（创建团队前预览路径，是 Git 仓库时显示分支与提交）。
    /// 目录不存在或不是目录时为 invalid；普通文件夹返回 `git: null`。
    pub fn inspect_workspace(&self, path: &str) -> Result<WorkspaceInfo> {
        self.inspect(path)
    }

    // ---------------------------------------------------------------- 岗位

    /// 可选的「快速添加示例」岗位（Leader / 产品经理 / 前端开发 / 后端开发 / QA 测试）。
    /// 岗位完全由用户自定义，示例默认不加载，只在用户主动选择时用于预填。
    pub fn list_role_templates(&self) -> Vec<RoleTemplate> {
        role_templates()
    }

    /// 根据岗位名称建议任务前缀（供表单预填，不写数据）：取名称中的英文字母生成，
    /// 取不到时给出 `R1`、`R2`…；避开团队未归档岗位已用前缀（传入 `teamId` 时）和 `taken`。
    pub fn suggest_task_prefix(&self, input: SuggestTaskPrefixInput) -> Result<String> {
        let mut taken = input.taken.unwrap_or_default();
        if let Some(team_id) = input.team_id.as_deref() {
            taken.extend(role::active_prefixes(&self.lock(), team_id)?);
        }
        Ok(role::suggest_prefix(&input.name, &taken))
    }

    /// 列出团队的未归档岗位卡片，Leader 在前，其余按创建时间。
    pub fn list_roles(&self, team_id: &str) -> Result<Vec<RoleView>> {
        role::list_views(&self.lock(), team_id)
    }

    /// 新增岗位：code 可缺省（自动生成 `role_N`），填写时须格式合法且团队内唯一（含已归档）；
    /// 前缀必填，2–4 位大写字母或数字、以字母开头，在未归档岗位中唯一；至多一个 Leader。
    /// 同时在工作空间建立岗位文件夹 `Teamspace/<岗位名>/`（重名时加「（岗位标识）」）。
    pub fn create_role(&self, input: CreateRoleInput) -> Result<RoleView> {
        let view = self.write(|tx| role::create(tx, input))?;
        self.sync_role_folders(&view.role.team_id);
        self.role_view(&view.role.id)
    }

    /// 编辑岗位名称、前缀、职责、工作约定和 Leader 标记；code 不可改，前缀修改不影响已有任务编号。
    /// 改名时岗位文件夹随之改名；工作空间中已有同名的其他文件夹（或其他岗位的文件夹）时拒绝
    /// （conflict），岗位不改名。
    pub fn update_role(&self, input: UpdateRoleInput) -> Result<RoleView> {
        let target = folder::rename_target(&self.lock(), &input.role_id, &input.name)?;
        if let Some((workspace, target, others)) = target {
            role_folder::check_rename_target(
                &workspace,
                &input.role_id,
                input.name.trim(),
                &target,
                &others,
            )?;
        }
        let view = self.write(|tx| role::update(tx, input))?;
        self.sync_role_folders(&view.role.team_id);
        self.role_view(&view.role.id)
    }

    /// 归档岗位；岗位仍负责未结束任务（非 done / cancelled）时拒绝（conflict）。
    /// 岗位文件夹保留，岗位说明标注已归档，工作日志记一条归档。
    pub fn archive_role(&self, role_id: &str) -> Result<Role> {
        let role = self.write(|tx| role::archive(tx, role_id))?;
        self.sync_role_folders(&role.team_id);
        Ok(role)
    }

    /// 绑定或更换岗位的 Harness；Harness 须为同团队未归档实例。记录活动。
    pub fn bind_role_harness(&self, role_id: &str, harness_id: &str) -> Result<RoleView> {
        let view = self.write(|tx| role::bind(tx, role_id, harness_id))?;
        self.sync_role_folders(&view.role.team_id);
        self.role_view(role_id)
    }

    /// 解除岗位的 Harness 绑定（岗位变为「暂不派工」）。记录活动。
    pub fn unbind_role_harness(&self, role_id: &str) -> Result<RoleView> {
        let view = self.write(|tx| role::unbind(tx, role_id))?;
        self.sync_role_folders(&view.role.team_id);
        self.role_view(role_id)
    }

    /// 岗位文件夹（含已归档岗位）：先同步一次，再返回绝对路径、是否存在和工作日志最近 `limit` 条
    /// （默认 20，最多 200，最新的在前）。文件夹不可用时不报错，原因写入 `problem`。
    pub fn get_role_folder(&self, role_id: &str, limit: Option<u32>) -> Result<RoleFolder> {
        let team_id = folder::team_of(&self.lock(), role_id)?;
        self.sync_role_folders(&team_id);
        let (workspace, relative, mut problem) = folder::location(&self.lock(), role_id)?;
        let limit = limit
            .unwrap_or(role_folder::DEFAULT_JOURNAL_LIMIT)
            .clamp(1, role_folder::MAX_JOURNAL_LIMIT) as usize;
        let path = relative
            .as_deref()
            .map(|rel| role_folder::absolute(&workspace, rel));
        let exists = path.as_deref().is_some_and(Path::is_dir);
        let journal = match &path {
            Some(dir) if exists => match role_folder::read_recent_journal(dir, limit) {
                Ok(lines) => lines,
                Err(err) => {
                    problem.get_or_insert_with(|| format!("无法读取工作日志：{err}"));
                    Vec::new()
                }
            },
            _ => Vec::new(),
        };
        if !exists && problem.is_none() {
            problem = Some(match &path {
                Some(dir) => format!("岗位文件夹不存在：{}", dir.display()),
                None => "岗位文件夹尚未建立".to_owned(),
            });
        }
        Ok(RoleFolder {
            role_id: role_id.to_owned(),
            relative_path: relative,
            path: path.map(|p| workspace::display(&p)),
            exists,
            journal,
            problem,
        })
    }

    /// 在 Finder（系统文件管理器）中打开岗位文件夹；文件夹不存在时为 not_found，并说明原因。
    pub fn reveal_role_folder(&self, role_id: &str) -> Result<()> {
        let folder = self.get_role_folder(role_id, Some(1))?;
        match folder.path {
            Some(path) if folder.exists => self.probe.reveal_path(Path::new(&path)),
            _ => Err(Error::not_found(
                folder
                    .problem
                    .unwrap_or_else(|| "岗位文件夹尚未建立".to_owned()),
            )),
        }
    }

    /// 岗位工作台（未归档岗位）：岗位卡片（含绑定的 Harness 与最近握手结果）、岗位文件夹（路径、
    /// 是否存在、问题、工作日志最近 20 条）、该岗位负责的最近 8 个任务（按更新时间倒序）与任务总数、
    /// `过程文件/` 下最近修改的 10 个文件。先同步岗位文件夹；文件读取在数据库锁外进行。
    pub fn get_role_workbench(&self, role_id: &str) -> Result<RoleWorkbench> {
        role::get_active(&self.lock(), role_id)?;
        let folder = self.get_role_folder(role_id, None)?;
        let (view, recent_tasks, task_count) = {
            let conn = self.lock();
            let view = role::view(&conn, role::get_active(&conn, role_id)?)?;
            let (tasks, total) = task::recent_for_owner(&conn, role_id, WORKBENCH_TASK_LIMIT)?;
            (view, tasks, total)
        };
        let recent_files = match &folder.path {
            Some(path) if folder.exists => {
                role_folder::recent_process_files(Path::new(path), WORKBENCH_FILE_LIMIT)
            }
            _ => Vec::new(),
        };
        let recent_runs = run_svc::recent_for_role(&self.lock(), role_id, WORKBENCH_RUN_LIMIT)?;
        Ok(RoleWorkbench {
            view,
            folder,
            recent_tasks,
            task_count,
            recent_files,
            recent_runs,
        })
    }

    /// 在 Finder 中打开岗位文件夹内的文件或目录（如 `工作日志.md`、`过程文件/素材清单.md`）；
    /// `relative_path` 缺省或为空时打开岗位文件夹本身。目录直接打开，文件在 Finder 中选中。
    /// 路径须在岗位文件夹之内：绝对路径、`..`、规范化后越出岗位文件夹（含符号链接）一律 invalid；
    /// 文件夹或目标不存在为 not_found。
    pub fn open_role_path(&self, role_id: &str, relative_path: Option<&str>) -> Result<()> {
        let folder = self.get_role_folder(role_id, Some(1))?;
        let dir = match folder.path {
            Some(path) if folder.exists => PathBuf::from(path),
            _ => {
                return Err(Error::not_found(
                    folder
                        .problem
                        .unwrap_or_else(|| "岗位文件夹尚未建立".to_owned()),
                ));
            }
        };
        let target = role_folder::resolve_inside(&dir, relative_path)?;
        self.probe.reveal_path(&target)
    }

    // ---------------------------------------------------------------- Harness

    /// ACP 预设与本机检测结果（Claude Code、Codex、MiniMax Code、Gemini CLI、CodeBuddy 等原生 ACP、自定义、Mock），
    /// 不写入任何数据：默认命令是否在 PATH 中、适配器依赖的底层 CLI 是否存在、缺少时的中文提示。
    /// 只查找命令，不运行任何程序。
    pub fn list_acp_presets(&self) -> Vec<AcpPresetInfo> {
        AcpPreset::ALL
            .into_iter()
            .map(|preset| self.preset_info(preset))
            .collect()
    }

    fn preset_info(&self, preset: AcpPreset) -> AcpPresetInfo {
        let spec = harness::preset_spec(preset);
        let command_path = match preset {
            AcpPreset::Custom => None,
            AcpPreset::Mock => self
                .config
                .mock_agent
                .as_ref()
                .map(|cmd| cmd.program.display().to_string()),
            _ => spec
                .command
                .and_then(|command| self.probe.locate_command(command)),
        };
        let command_found = command_path.is_some();
        let cli_found = spec
            .requires_cli
            .map(|cli| self.probe.locate_command(cli).is_some());
        let mut missing = Vec::new();
        if !command_found && let Some(install) = spec.install {
            missing.push(install);
        }
        if cli_found == Some(false)
            && let Some(install) = spec.install_cli
        {
            missing.push(install);
        }
        let hint = match preset {
            AcpPreset::Mock if !command_found => {
                Some("当前宿主程序没有提供内置 Mock Agent".to_owned())
            }
            _ if missing.is_empty() => None,
            _ => Some(missing.join("；")),
        };
        AcpPresetInfo {
            preset,
            label: preset.label().to_owned(),
            description: spec.description.to_owned(),
            default_command: spec.command.map(str::to_owned),
            default_args: spec.args.iter().map(|a| (*a).to_owned()).collect(),
            command_path,
            command_found,
            requires_cli: spec.requires_cli.map(str::to_owned),
            cli_found,
            hint,
        }
    }

    /// 列出团队的未归档 Harness 实例，按创建时间升序。
    pub fn list_harnesses(&self, team_id: &str) -> Result<Vec<HarnessInstance>> {
        harness::list(&self.lock(), team_id)
    }

    /// 单个未归档 Harness 实例（含最近健康检查）；已归档或不存在为 not_found。
    pub fn get_harness(&self, harness_id: &str) -> Result<HarnessInstance> {
        harness::get_active(&self.lock(), harness_id)
    }

    /// 注册 Harness 实例（ACP Agent）：名称在团队未归档实例中唯一；启动命令缺省时用预设默认命令
    /// （`custom` 必填，`mock` 固定），须为不含空格的命令名或绝对路径；参数缺省时用预设默认参数；
    /// 环境配置档案若填写须为绝对路径（只记路径）。空白字符串视为未填写。握手状态初始为 unknown。
    pub fn create_harness(&self, input: CreateHarnessInput) -> Result<HarnessInstance> {
        self.write(|tx| harness::create(tx, input))
    }

    /// 编辑 Harness 实例（预设不可改，字段规则同创建）；握手状态重置为 unknown。
    /// 绑定该实例的岗位的岗位说明随之更新。
    pub fn update_harness(&self, input: UpdateHarnessInput) -> Result<HarnessInstance> {
        let instance = self.write(|tx| harness::update(tx, input))?;
        self.sync_role_folders(&instance.team_id);
        Ok(instance)
    }

    /// 归档 Harness 实例；仍有未归档岗位绑定它时拒绝（conflict）。
    pub fn archive_harness(&self, harness_id: &str) -> Result<HarnessInstance> {
        self.write(|tx| harness::archive(tx, harness_id))
    }

    /// 对实例做 ACP 握手检查（启动 Agent → `initialize` → 终止进程，不发送 prompt、不消耗额度，
    /// 超时 20 秒）并持久化结果；Mock 预设启动宿主内置的 Mock Agent。握手失败不是命令错误，
    /// 体现在 `handshake` 中（`error` / `needs_auth` 与中文说明）。握手在数据库锁外进行。
    pub fn check_harness(&self, harness_id: &str) -> Result<HarnessInstance> {
        let instance = harness::get_active(&self.lock(), harness_id)?;
        let launch = self.launch_for(
            instance.preset,
            &instance.command,
            &instance.args,
            instance.env_profile.as_deref(),
        );
        let mut handshake = match launch {
            Ok(launch) => self.probe.handshake(&launch),
            Err(message) => Handshake::error(message),
        };
        handshake.checked_at = Some(store::now());
        self.write(|tx| harness::save_handshake(tx, harness_id, &handshake))
    }

    // ---------------------------------------------------------------- 任务

    /// 全部任务状态及显示名，按标准顺序。
    pub fn list_task_statuses(&self) -> Vec<TaskStatusInfo> {
        TaskStatus::ALL.into_iter().map(Into::into).collect()
    }

    /// 看板数据：按状态分组，始终包含 done 列，cancelled 列仅在 `includeCancelled` 时包含。
    pub fn get_task_board(&self, input: TaskBoardQuery) -> Result<TaskBoard> {
        task::board(&self.lock(), &input)
    }

    /// 任务详情：依赖、下游、文件存在情况、相关活动、可执行的流转和就绪问题。
    pub fn get_task(&self, task_id: &str) -> Result<TaskDetail> {
        let (workspace, refs) = task::workspace_and_refs(&self.lock(), task_id)?;
        let files = FileCheck::scan(Path::new(&workspace), &refs);
        task::detail(&self.lock(), task_id, &files)
    }

    /// 创建任务：标题、负责岗位必填；初始状态只能是 todo 或 ready（ready 需满足就绪条件）；
    /// 编号取负责岗位前缀的下一个序号；引用路径须为工作区内相对路径（最多 50 个）；
    /// 依赖须为同团队任务（最多 20 个）且不成环。
    pub fn create_task(&self, mut input: CreateTaskInput) -> Result<TaskDetail> {
        input.file_refs = task::normalize_refs(input.file_refs)?;
        let workspace = team::get_active(&self.lock(), &input.team_id)?.workspace_path;
        let files = FileCheck::scan(Path::new(&workspace), &input.file_refs);
        let detail = self.write(|tx| task::create(tx, input, &files))?;
        self.sync_role_folders(&detail.task.team_id);
        Ok(detail)
    }

    /// 编辑任务内容（不改状态，编号不变）；保存依赖时拒绝成环并指出环路；
    /// 任务处于 ready 而修改后不再满足就绪条件时拒绝（conflict）；任务有活动执行（排队、运行、
    /// 等待授权）时整个编辑被拒绝（conflict）。
    pub fn update_task(&self, mut input: UpdateTaskInput) -> Result<TaskDetail> {
        input.file_refs = task::normalize_refs(input.file_refs)?;
        let (workspace, _) = task::workspace_and_refs(&self.lock(), &input.task_id)?;
        let files = FileCheck::scan(Path::new(&workspace), &input.file_refs);
        let detail = self.write(|tx| task::update(tx, input, &files))?;
        self.sync_role_folders(&detail.task.team_id);
        Ok(detail)
    }

    /// 按流转规则变更状态：进入 ready 或 doing（含 blocked → doing）需满足就绪条件
    /// （负责岗位有效、文件引用可选，填写的引用须存在于工作区、所有依赖已完成）；进入 blocked 必须给出原因，
    /// 离开 blocked 清空原因。
    pub fn change_task_status(&self, input: ChangeTaskStatusInput) -> Result<TaskDetail> {
        let (workspace, refs) = task::workspace_and_refs(&self.lock(), &input.task_id)?;
        let files = FileCheck::scan(Path::new(&workspace), &refs);
        let detail = self.write(|tx| task::change_status(tx, input, &files))?;
        self.sync_role_folders(&detail.task.team_id);
        Ok(detail)
    }

    /// 在团队工作空间中搜索文件（用于添加引用），返回相对路径，最多 50 条。
    /// Git 目录与普通文件夹的搜索范围见 [`SystemProbe::search_files`]。
    pub fn search_workspace_files(&self, input: FileSearchQuery) -> Result<Vec<String>> {
        let workspace = team::get_active(&self.lock(), &input.team_id)?.workspace_path;
        let mut files = self.probe.search_files(
            Path::new(&workspace),
            input.query.trim(),
            FILE_SEARCH_LIMIT,
        )?;
        files.truncate(FILE_SEARCH_LIMIT);
        Ok(files)
    }

    // ---------------------------------------------------------------- 执行

    /// 开始执行任务（D13 / D15）：任务须为 READY 或受阻（重试；清除受阻原因），或执行中且最近一次执行
    /// 已中断或人工接管（重新执行；已中断的那次记为失败）；须满足就绪条件，负责岗位已绑定 Harness 且
    /// 握手状态不是 error；同一任务、同一个普通文件夹同时只能有一个活动执行。不满足时为 conflict，
    /// 原因同 `TaskDetail.runProblems`。
    ///
    /// 任务转为执行中，返回状态为 queued 的执行；之后由后台准备执行目录（Git 工作空间新建 worktree，
    /// 普通文件夹原地）、启动 Agent、建立会话、发送任务说明，进度与状态通过执行事件推送
    /// （[`RUN_EVENT`]），也可用 `get_run` / `list_run_updates` 读取。
    pub fn start_run(&self, task_id: &str) -> Result<Run> {
        let (workspace, refs) = task::workspace_and_refs(&self.lock(), task_id)?;
        let files = FileCheck::scan(Path::new(&workspace), &refs);
        let info = self
            .inspect(&workspace)
            .map_err(|err| Error::conflict(format!("工作空间不可用：{err}")))?;
        let isolation = if info.git.is_some() {
            RunIsolation::Worktree
        } else {
            RunIsolation::InPlace
        };
        let mut registry = self.runs.registry();
        // 顶掉的中断执行：不会再续接，它的快照在事务提交之后再删（不在持锁的事务里做文件操作）。
        let mut replaced: Option<String> = None;
        let run = self.write(|tx| {
            let task = task::get(tx, task_id)?;
            let problems = run_svc::start_problems(tx, &task, &files)?;
            if !problems.is_empty() {
                return Err(Error::conflict(format!(
                    "不能开始执行：{}",
                    problems.join("；")
                )));
            }
            if let Some(prev) = run_svc::latest_for_task(tx, task_id)?
                && prev.status == RunStatus::Interrupted
            {
                let failure = "执行中断后已重新执行".to_owned();
                let ended = run_svc::end_interrupted(
                    tx,
                    &prev.id,
                    RunStatus::Failed,
                    &Ending {
                        failure: Some(failure.clone()),
                        failure_kind: Some(FailureKind::Interrupted),
                        summary: prev.summary.clone(),
                        changed_files: prev.changed_files.clone(),
                        out_of_scope: prev.out_of_scope.clone(),
                        change_note: prev.change_note.clone(),
                        change_incomplete: prev.change_incomplete,
                        tool_call_count: prev.tool_call_count,
                        ..Ending::default()
                    },
                )?;
                if ended {
                    run_svc::status_update(tx, &prev.id, RunStatus::Failed, Some(failure))?;
                    replaced = Some(prev.id.clone());
                }
            }
            let owner = role::get_active(tx, &task.owner_role_id)?;
            let instance = harness::get_active(tx, owner.harness_id.as_deref().unwrap_or_default())?;
            let attempt = run_svc::next_attempt(tx, task_id)?;
            let exec_dir = match isolation {
                RunIsolation::InPlace => workspace.clone(),
                RunIsolation::Worktree => self
                    .data_dir
                    .join("worktrees")
                    .join(&task.team_id)
                    .join(format!("{}-r{attempt}", task.key))
                    .display()
                    .to_string(),
            };
            let run = run_svc::insert(
                tx,
                NewRun {
                    task: &task,
                    harness: HarnessSnapshot {
                        harness_id: instance.id.clone(),
                        name: instance.name.clone(),
                        preset: instance.preset,
                        command: instance.command.clone(),
                        args: instance.args.clone(),
                    },
                    workspace: &workspace,
                    exec_dir: &exec_dir,
                    isolation,
                },
            )?;
            if task.status != TaskStatus::Doing {
                run_svc::move_task(tx, task_id, TaskStatus::Doing, None, Actor::Human, "开始执行")?;
            }
            let how = match isolation {
                RunIsolation::Worktree => "Git worktree",
                RunIsolation::InPlace => "原地执行",
            };
            run_svc::record_event(
                tx,
                &run,
                Actor::Human,
                EventKind::RunStarted,
                format!(
                    "{} 开始第 {} 次执行（Harness「{}」，{how}）",
                    run.task_key, run.attempt, run.harness.name
                ),
                serde_json::json!({ "runId": run.id, "attempt": run.attempt, "isolation": isolation }),
            )?;
            run_svc::status_update(tx, &run.id, RunStatus::Queued, None)?;
            Ok(run)
        })?;
        if let Some(id) = &replaced {
            self.remove_snapshots(id);
        }
        self.core
            .spawn_driver(&mut registry, run.clone(), Mode::Start);
        drop(registry);
        self.sync_role_folders(&run.team_id);
        self.emit_run(&run.id);
        Ok(run)
    }

    /// 取消活动执行：通知 Agent（`session/cancel`），撤销待确认的权限请求，宽限期（默认 10 秒）内
    /// Agent 未结束则整组终止进程。等执行结束后返回（状态 cancelled，任务回到 READY）。
    /// 执行不在活动中时为 conflict。
    pub fn cancel_run(&self, run_id: &str) -> Result<Run> {
        let run = run_svc::get(&self.lock(), run_id)?;
        if !run.status.is_active() || !self.stop_run(run_id, Stop::Cancel) {
            return Err(Error::conflict(format!(
                "执行已结束（{}），无需取消",
                run.status.label()
            )));
        }
        run_svc::get(&self.lock(), run_id)
    }

    /// 人工接管：停止执行（活动执行先取消，已中断或已失败的执行直接结束），执行记为 cancelled 并标注
    /// `takenOver`，任务回到执行中，由 Human 在执行目录（`execDir`，可用 `open_run_dir` 打开）中处理；
    /// 之后可再次 `start_run`，或把任务直接提交验收（`change_task_status` → review）。
    /// 其他状态为 conflict。
    pub fn takeover_run(&self, run_id: &str) -> Result<Run> {
        let run = run_svc::get(&self.lock(), run_id)?;
        if run.status.is_active() && self.stop_run(run_id, Stop::Takeover) {
            return run_svc::get(&self.lock(), run_id);
        }
        self.write(|tx| run_svc::takeover(tx, run_id))?;
        self.remove_snapshots(&run.id);
        self.sync_role_folders(&run.team_id);
        self.emit_run(run_id);
        run_svc::get(&self.lock(), run_id)
    }

    /// 续接已中断的执行：Agent 支持 `loadSession` 且会话已建立时，重新启动 Agent 并用 `session/load`
    /// 续接原会话，再发一条「继续」提示；任务须仍在执行中。不支持时为 conflict（请重新执行）。
    pub fn resume_run(&self, run_id: &str) -> Result<Run> {
        let run = run_svc::get(&self.lock(), run_id)?;
        if run.status != RunStatus::Interrupted {
            return Err(Error::conflict(format!(
                "只有已中断的执行可以续接（当前：{}）",
                run.status.label()
            )));
        }
        if run.session_id.is_none() {
            return Err(Error::conflict(
                "执行在会话建立前就中断了，无法续接，请重新执行",
            ));
        }
        if !run.can_resume {
            return Err(Error::conflict(format!(
                "Harness「{}」不支持续接会话（loadSession），请重新执行",
                run.harness.name
            )));
        }
        if !Path::new(&run.exec_dir).is_dir() {
            return Err(Error::conflict(format!(
                "执行目录已不存在：{}，请重新执行",
                run.exec_dir
            )));
        }
        let mut registry = self.runs.registry();
        let reopened = self.write(|tx| {
            let task = task::get(tx, &run.task_id)?;
            if task.status != TaskStatus::Doing {
                return Err(Error::conflict(format!(
                    "任务 {} 已不在执行中（{}），无法续接",
                    task.key,
                    task.status.label()
                )));
            }
            let workspace = run_svc::workspace_of(tx, run_id)?;
            if run.isolation == RunIsolation::InPlace
                && let Some(busy) = run_svc::active_in_place(tx, &workspace)?
            {
                return Err(Error::conflict(format!(
                    "工作空间（普通文件夹）正在执行 {}：同一文件夹同时只能运行一个执行",
                    busy.task_key
                )));
            }
            if !run_svc::reopen(tx, run_id)? {
                return Err(Error::conflict("只有已中断的执行可以续接"));
            }
            run_svc::status_update(tx, run_id, RunStatus::Queued, Some("续接会话".to_owned()))?;
            run_svc::record_event(
                tx,
                &run,
                Actor::Human,
                EventKind::RunResumed,
                format!("{} 续接第 {} 次执行", run.task_key, run.attempt),
                serde_json::json!({ "runId": run.id, "attempt": run.attempt }),
            )?;
            run_svc::get(tx, run_id)
        })?;
        self.core
            .spawn_driver(&mut registry, reopened.clone(), Mode::Resume);
        drop(registry);
        self.sync_role_folders(&run.team_id);
        self.emit_run(run_id);
        Ok(reopened)
    }

    /// 把已中断的执行标记为失败：任务转为受阻（「执行失败：执行中断后由 Human 标记为失败」）。
    pub fn mark_run_failed(&self, run_id: &str) -> Result<Run> {
        let run = run_svc::get(&self.lock(), run_id)?;
        if run.status != RunStatus::Interrupted {
            return Err(Error::conflict(format!(
                "只有已中断的执行可以标记为失败（当前：{}）",
                run.status.label()
            )));
        }
        self.write(|tx| {
            let failure = "执行中断后由 Human 标记为失败".to_owned();
            let ended = run_svc::end_interrupted(
                tx,
                run_id,
                RunStatus::Failed,
                &Ending {
                    failure: Some(failure.clone()),
                    failure_kind: Some(FailureKind::Interrupted),
                    summary: run.summary.clone(),
                    changed_files: run.changed_files.clone(),
                    out_of_scope: run.out_of_scope.clone(),
                    change_note: run.change_note.clone(),
                    change_incomplete: run.change_incomplete,
                    tool_call_count: run.tool_call_count,
                    ..Ending::default()
                },
            )?;
            if !ended {
                return Err(Error::conflict("只有已中断的执行可以标记为失败"));
            }
            run_svc::status_update(tx, run_id, RunStatus::Failed, Some(failure.clone()))?;
            if task::get(tx, &run.task_id)?.status == TaskStatus::Doing {
                run_svc::move_task(
                    tx,
                    &run.task_id,
                    TaskStatus::Blocked,
                    Some(format!("{}{failure}", run_svc::FAILED_PREFIX)),
                    Actor::Human,
                    "",
                )?;
            }
            run_svc::record_event(
                tx,
                &run,
                Actor::Human,
                EventKind::RunFinished,
                format!(
                    "{} 第 {} 次执行失败（{}）",
                    run.task_key, run.attempt, run.harness.name
                ),
                serde_json::json!({ "runId": run.id, "attempt": run.attempt,
                                    "status": RunStatus::Failed, "failure": failure }),
            )
        })?;
        self.remove_snapshots(&run.id);
        self.sync_role_folders(&run.team_id);
        self.emit_run(run_id);
        run_svc::get(&self.lock(), run_id)
    }

    /// 在 Finder 中打开执行目录（worktree 或原地执行的工作空间）；目录不存在时为 not_found。
    pub fn open_run_dir(&self, run_id: &str) -> Result<()> {
        let run = run_svc::get(&self.lock(), run_id)?;
        let dir = Path::new(&run.exec_dir);
        if !dir.is_dir() {
            return Err(Error::not_found(format!(
                "执行目录不存在：{}",
                run.exec_dir
            )));
        }
        self.probe.reveal_path(dir)
    }

    /// 用系统默认程序打开执行目录内的文件（验收页「打开文件」，D22）。`path` 是相对执行目录的路径
    /// （即验收页改动文件的 `path`）：空、绝对路径、`..`、规范化后越出执行目录（含符号链接）一律 invalid；
    /// 执行目录或文件不存在为 not_found；目录不是文件，invalid。
    pub fn open_run_file(&self, run_id: &str, path: &str) -> Result<()> {
        let run = run_svc::get(&self.lock(), run_id)?;
        let dir = Path::new(&run.exec_dir);
        if !dir.is_dir() {
            return Err(Error::not_found(format!(
                "执行目录不存在：{}",
                run.exec_dir
            )));
        }
        if path.trim().is_empty() {
            return Err(Error::invalid("请指定要打开的文件"));
        }
        let target = role_folder::resolve_inside(dir, Some(path)).map_err(|err| match err {
            Error::Invalid(_) => Error::invalid(format!("只能打开执行目录内的文件：{path}")),
            Error::NotFound(_) => Error::not_found(format!("执行目录中没有「{path}」")),
            other => other,
        })?;
        if target.is_dir() {
            return Err(Error::invalid(format!("「{path}」是目录，不是文件")));
        }
        self.probe.open_path(&target)
    }

    /// 单个执行记录。
    pub fn get_run(&self, run_id: &str) -> Result<Run> {
        run_svc::get(&self.lock(), run_id)
    }

    /// 任务的全部执行，最近的在前。
    pub fn list_runs(&self, task_id: &str) -> Result<Vec<Run>> {
        run_svc::list_for_task(&self.lock(), task_id)
    }

    /// 执行进度增量读取：返回 `cursor` 之后新增或被更新的条目（见 [`RunUpdatePage`]）。
    pub fn list_run_updates(&self, input: RunUpdateQuery) -> Result<RunUpdatePage> {
        self.run_updates(&input)
    }

    /// 回应等待 Human 确认的权限请求：`optionId` 须是该请求的选项之一；请求不存在或已处理为
    /// not_found，执行已结束为 conflict。返回最新的执行记录。
    pub fn respond_permission(&self, input: RespondPermissionInput) -> Result<Run> {
        self.decide_permission(&input.run_id, &input.request_id, &input.option_id)?;
        run_svc::get(&self.lock(), &input.run_id)
    }

    // ---------------------------------------------------------------- 验收

    /// 验收页数据（见 [`ReviewView`]）：任务、待验收的执行（任务处于 REVIEW 时最近一次执行）、摘要、
    /// 改动文件（Git：执行分支相对基线提交的 diff；普通文件夹：执行记录里的改动文件，多轮退回合在一起）、
    /// 能否通过 / 退回及原因、历次退回意见。任务不在待验收时 `run` 为 `null`、不能通过也不能退回。
    pub fn get_review(&self, task_id: &str) -> Result<ReviewView> {
        let (task, run, rejections, workspace, files_in_place, instance, conflict) = {
            let conn = self.lock();
            let task = task::get(&conn, task_id)?;
            let conflicted = review::conflict_run(&conn, &task)?;
            let run = match &conflicted {
                Some(r) => Some(r.clone()),
                None => review::pending_run(&conn, &task)?,
            };
            let rejections = review::rejections(&conn, task_id)?;
            let workspace = team::get_active(&conn, &task.team_id)?.workspace_path;
            let files = match &run {
                Some(r) if r.isolation == RunIsolation::InPlace => {
                    Some(review::chain_files(&conn, r)?)
                }
                _ => None,
            };
            let instance = match &run {
                Some(r) => harness::get_any(&conn, &r.harness.harness_id)?,
                None => None,
            };
            let conflict = conflicted.is_some();
            (task, run, rejections, workspace, files, instance, conflict)
        };
        let mut problems = Vec::new();
        let mut approve_problems = Vec::new();
        let mut reject_problems = Vec::new();
        let mut files = Vec::new();
        let mut files_problem = None;
        if conflict {
            problems.push(
                "合并有冲突，任务已受阻：请在仓库中手动合并后「已手动合并，标记完成」，或「重新执行」".to_owned(),
            );
        } else if task.status != TaskStatus::Review {
            problems.push(format!("任务当前是「{}」，不在待验收", task.status.label()));
        }
        match &run {
            None if task.status == TaskStatus::Review => problems.push(
                "任务没有可验收的执行记录（手动提交的验收），请用状态流转直接完成或退回 READY"
                    .to_owned(),
            ),
            None => {}
            Some(run) => {
                match run.isolation {
                    RunIsolation::InPlace => {
                        files = files_in_place
                            .unwrap_or_default()
                            .into_iter()
                            .map(|f| ReviewFile {
                                path: f.path,
                                change: f.change,
                                binary: false,
                                added: None,
                                removed: None,
                                has_patch: false,
                            })
                            .collect();
                    }
                    RunIsolation::Worktree => match (&run.base_commit, &run.branch) {
                        (Some(base), Some(branch)) => {
                            let path_env = self.probe.command_path_env();
                            match self.review_git().diff_files(
                                Path::new(&workspace),
                                base,
                                branch,
                                Some(&path_env),
                            ) {
                                Ok(list) => {
                                    files = list
                                        .into_iter()
                                        .map(|d| ReviewFile {
                                            path: d.path,
                                            change: d.change,
                                            has_patch: !d.binary,
                                            added: (!d.binary).then_some(d.added),
                                            removed: (!d.binary).then_some(d.removed),
                                            binary: d.binary,
                                        })
                                        .collect();
                                }
                                Err(err) => {
                                    // 读不到分支 diff 时退回执行记录里的改动文件（没有 diff）。
                                    files_problem = Some(format!("无法读取执行分支的改动：{err}"));
                                    files = run
                                        .changed_files
                                        .iter()
                                        .map(|f| ReviewFile {
                                            path: f.path.clone(),
                                            change: f.change,
                                            binary: false,
                                            added: None,
                                            removed: None,
                                            has_patch: false,
                                        })
                                        .collect();
                                }
                            }
                        }
                        _ => {
                            approve_problems.push("执行没有记录分支或基线提交，无法合并".to_owned())
                        }
                    },
                }
                if !Path::new(&run.exec_dir).is_dir() {
                    reject_problems.push(format!(
                        "执行目录已不存在：{}，无法在原目录续接；请把任务退回 READY 后重新执行",
                        run.exec_dir
                    ));
                }
                match &instance {
                    Some(h) if h.archived_at.is_none() => {}
                    Some(h) => reject_problems.push(format!(
                        "Harness「{}」已归档，无法续接；请把任务退回 READY 后重新执行",
                        h.name
                    )),
                    None => reject_problems.push("这次执行的 Harness 不存在，无法续接".to_owned()),
                }
            }
        }
        let rerun_blockers = if conflict {
            self.rerun_readiness(&task)?
        } else {
            Vec::new()
        };
        problems.extend(
            rerun_blockers
                .iter()
                .map(|p| format!("重新执行前需要：{p}")),
        );
        let can_approve =
            !conflict && problems.is_empty() && approve_problems.is_empty() && run.is_some();
        let can_reject =
            !conflict && problems.is_empty() && reject_problems.is_empty() && run.is_some();
        problems.extend(approve_problems);
        problems.extend(reject_problems);
        Ok(ReviewView {
            summary: run.as_ref().and_then(|r| r.summary.clone()),
            isolation: run.as_ref().map(|r| r.isolation),
            base_branch: run.as_ref().and_then(|r| r.base_branch.clone()),
            branch: run.as_ref().and_then(|r| r.branch.clone()),
            approve_action: run.as_ref().map(|r| match r.isolation {
                RunIsolation::Worktree => ApproveAction::Merge,
                RunIsolation::InPlace => ApproveAction::Complete,
            }),
            files,
            files_problem,
            can_approve,
            can_reject,
            problems,
            rejections,
            conflict,
            can_resolve_manually: conflict,
            can_rerun: conflict && rerun_blockers.is_empty(),
            task,
            run,
        })
    }

    /// 一个改动文件的补丁（执行分支相对基线提交）。只对 Git 工作空间的执行提供（D20）：普通文件夹的执行
    /// 为 invalid；路径不在改动文件中、或 Git 读取失败为 conflict。
    pub fn get_review_file_patch(&self, run_id: &str, path: &str) -> Result<ReviewFilePatch> {
        let (run, workspace) = {
            let conn = self.lock();
            (
                run_svc::get(&conn, run_id)?,
                run_svc::workspace_of(&conn, run_id)?,
            )
        };
        if run.isolation == RunIsolation::InPlace {
            return Err(Error::invalid(
                "普通文件夹的执行没有逐文件 diff（改动已在原地发生），请直接查看文件",
            ));
        }
        let (Some(base), Some(branch)) = (&run.base_commit, &run.branch) else {
            return Err(Error::conflict("执行没有记录分支或基线提交，无法读取 diff"));
        };
        let path_env = self.probe.command_path_env();
        let patch = self
            .review_git()
            .file_patch(Path::new(&workspace), base, branch, path, Some(&path_env))
            .map_err(|err| Error::conflict(format!("无法读取 {path} 的 diff：{err}")))?;
        Ok(ReviewFilePatch {
            run_id: run.id,
            path: path.to_owned(),
            binary: patch.binary,
            truncated: patch.truncated,
            patch: patch.patch,
        })
    }

    /// 通过验收（D20）。只接受任务当前待验收的执行（任务处于 REVIEW、是它的最近一次执行），否则为
    /// conflict；同一任务的验收同时只处理一个。
    ///
    /// - 普通文件夹：任务直接完成（DONE）。
    /// - Git：先把执行目录里尚未提交的改动提交到执行分支，再以 `--no-ff` 合并进基线分支（不推送）。
    ///   合并成功 → 记录合并提交、任务完成，然后删除 worktree 与执行分支（清理失败不回滚，记一条活动，
    ///   结果里给出 `cleanupProblem`）；有冲突 → 合并已中止、基线不变，任务转为受阻（原因写明冲突文件），
    ///   执行分支与 worktree 保留；现在不能合并（基线分支有未提交改动等）→ 任务保持待验收，结果给出原因。
    ///
    /// 每种结果都写活动（进而写入岗位工作日志）。任务完成时在同一事务里调用依赖推进钩子
    /// （`service::task::on_task_done`）。
    pub fn approve_review(&self, input: ApproveReviewInput) -> Result<ApproveReviewResult> {
        let (first, _) = review::check_pending(&self.lock(), &input.run_id)?;
        let _guard = self.begin_review(&first.id, &first.key)?;
        // 取得标记后重新校验（另一个验收可能刚刚处理完）。
        let (task, run) = review::check_pending(&self.lock(), &input.run_id)?;
        let mut result = ApproveReviewResult {
            outcome: ApproveOutcome::Completed,
            merged_commit: None,
            conflict_files: Vec::new(),
            reason: None,
            cleanup_problem: None,
            task: task.clone(),
            run: run.clone(),
        };
        match run.isolation {
            RunIsolation::InPlace => {
                self.write(|tx| {
                    let (task, run) = review::check_pending(tx, &run.id)?;
                    review::set_outcome(tx, &run.id, ReviewOutcome::Approved, None, None)?;
                    run_svc::move_task(
                        tx,
                        &task.id,
                        TaskStatus::Done,
                        None,
                        Actor::Human,
                        "验收通过",
                    )?;
                    run_svc::record_event(
                        tx,
                        &run,
                        Actor::Human,
                        EventKind::ReviewApproved,
                        format!(
                            "{} 第 {} 次执行验收通过（普通文件夹，改动已在原地）",
                            run.task_key, run.attempt
                        ),
                        serde_json::json!({ "runId": run.id, "attempt": run.attempt }),
                    )?;
                    task::on_task_done(tx, &task.id)
                })?;
            }
            RunIsolation::Worktree => self.approve_merge(&task, &run, &mut result)?,
        }
        self.sync_role_folders(&run.team_id);
        self.emit_run(&run.id);
        let conn = self.lock();
        result.task = task::get(&conn, &task.id)?;
        result.run = run_svc::get(&conn, &run.id)?;
        Ok(result)
    }

    /// Git 工作空间的「通过」：提交、合并、记录结果、清理（见 [`Teamspace::approve_review`]）。
    fn approve_merge(
        &self,
        task: &Task,
        run: &Run,
        result: &mut ApproveReviewResult,
    ) -> Result<()> {
        let git = self.review_git();
        let path_env = self.probe.command_path_env();
        let workspace = run_svc::workspace_of(&self.lock(), &run.id)?;
        let Some(branch) = run.branch.clone() else {
            return Err(Error::conflict("执行没有记录分支，无法合并"));
        };
        // 基线分支：执行开始时记下的；M3 之前的执行没有记录，退回到工作空间当前所在的分支。
        let base_branch = run.base_branch.clone().or_else(|| {
            self.inspect(&workspace)
                .ok()
                .and_then(|info| info.git)
                .and_then(|git| git.branch)
        });
        let outcome = match &base_branch {
            None => review_seam::Merge::Blocked {
                reason: "没有记录基线分支，工作空间也不在任何分支上，无法确定合并到哪里".to_owned(),
            },
            Some(base) => {
                let exec = Path::new(&run.exec_dir);
                let committed = if exec.is_dir() {
                    git.commit_all(
                        exec,
                        &format!(
                            "teamspace: {} attempt {} (review)",
                            run.task_key, run.attempt
                        ),
                        Some(&path_env),
                    )
                    .map(|_| ())
                } else {
                    Ok(())
                };
                match committed {
                    Err(err) => review_seam::Merge::Blocked {
                        reason: format!("无法提交执行目录里的改动：{err}"),
                    },
                    Ok(()) => git
                        .merge_into_base(
                            Path::new(&workspace),
                            base,
                            &branch,
                            &format!("merge: {} {}", task.key, task.title),
                            Some(&path_env),
                        )
                        .unwrap_or_else(|err| review_seam::Merge::Blocked {
                            reason: format!("合并失败：{err}"),
                        }),
                }
            }
        };
        let base = base_branch.unwrap_or_default();
        match outcome {
            review_seam::Merge::Merged { commit } => {
                result.outcome = ApproveOutcome::Merged;
                result.merged_commit = Some(commit.clone());
                self.write(|tx| {
                    // 合并已经发生：结果总要记在执行上；任务只在仍待验收时转为完成。
                    review::set_outcome(tx, &run.id, ReviewOutcome::Approved, None, Some(&commit))?;
                    run_svc::record_event(
                        tx,
                        run,
                        Actor::Human,
                        EventKind::ReviewApproved,
                        format!(
                            "{} 第 {} 次执行验收通过，已把 {branch} 合并进 {base}（{}）",
                            run.task_key,
                            run.attempt,
                            &commit[..commit.len().min(7)]
                        ),
                        serde_json::json!({ "runId": run.id, "attempt": run.attempt,
                                            "mergedCommit": commit, "branch": branch,
                                            "baseBranch": base }),
                    )?;
                    if task::get(tx, &task.id)?.status == TaskStatus::Review {
                        run_svc::move_task(
                            tx,
                            &task.id,
                            TaskStatus::Done,
                            None,
                            Actor::Human,
                            "验收通过并合并",
                        )?;
                        task::on_task_done(tx, &task.id)?;
                    }
                    Ok(())
                })?;
                result.cleanup_problem =
                    self.cleanup_run(run, &workspace, &branch, Cleanup::Merged);
            }
            review_seam::Merge::Conflict { files } => {
                result.outcome = ApproveOutcome::Conflict;
                result.conflict_files = files.clone();
                let list = files.join("、");
                let reason: String = format!(
                    "合并冲突：{list}。已中止合并，基线分支 {base} 未改变；请在仓库中手动合并 {branch} 后再处理"
                )
                .chars()
                .take(crate::service::MAX_REASON)
                .collect();
                self.write(|tx| {
                    review::set_outcome(tx, &run.id, ReviewOutcome::Conflict, Some(&reason), None)?;
                    run_svc::move_task(
                        tx,
                        &task.id,
                        TaskStatus::Blocked,
                        Some(reason.clone()),
                        Actor::Human,
                        "验收合并冲突",
                    )?;
                    run_svc::record_event(
                        tx,
                        run,
                        Actor::Human,
                        EventKind::ReviewConflict,
                        format!(
                            "{} 第 {} 次执行验收通过但合并冲突：{list}",
                            run.task_key, run.attempt
                        ),
                        serde_json::json!({ "runId": run.id, "attempt": run.attempt,
                                            "conflictFiles": files, "branch": branch,
                                            "baseBranch": base }),
                    )
                })?;
            }
            review_seam::Merge::Blocked { reason } => {
                result.outcome = ApproveOutcome::Blocked;
                result.reason = Some(reason.clone());
                self.write(|tx| {
                    run_svc::record_event(
                        tx,
                        run,
                        Actor::Human,
                        EventKind::ReviewMergeBlocked,
                        format!(
                            "{} 第 {} 次执行验收通过但现在不能合并：{reason}",
                            run.task_key, run.attempt
                        ),
                        serde_json::json!({ "runId": run.id, "attempt": run.attempt,
                                            "reason": reason }),
                    )
                })?;
            }
        }
        Ok(())
    }

    /// 退回验收：意见必填（去掉首尾空白后不能为空，最多 2000 字）；只接受任务当前待验收的执行（否则
    /// conflict）。被退回的执行记下意见，任务回到执行中，新建一次执行（`revisionOf` 指向被退回的执行）
    /// 在**同一执行目录**续接：Agent 支持 `session/load` 时续接原会话、把意见作为新提示发送；不支持时在同一
    /// 目录新开会话，提示带上完整任务说明、上次摘要与意见。不新建 worktree。返回状态为 queued 的新执行。
    pub fn reject_review(&self, input: RejectReviewInput) -> Result<Run> {
        let comment = crate::service::text("退回意见", &input.comment, review::MAX_COMMENT, true)?;
        let (task, run) = review::check_pending(&self.lock(), &input.run_id)?;
        let _guard = self.begin_review(&task.id, &task.key)?;
        if !Path::new(&run.exec_dir).is_dir() {
            return Err(Error::conflict(format!(
                "执行目录已不存在：{}，无法在原目录续接；请把任务退回 READY 后重新执行",
                run.exec_dir
            )));
        }
        let mut registry = self.runs.registry();
        let revision = self.write(|tx| {
            let (task, run) = review::check_pending(tx, &input.run_id)?;
            let instance = match harness::get_any(tx, &run.harness.harness_id)? {
                Some(h) if h.archived_at.is_none() => h,
                Some(h) => {
                    return Err(Error::conflict(format!(
                        "Harness「{}」已归档，无法续接；请把任务退回 READY 后重新执行",
                        h.name
                    )));
                }
                None => return Err(Error::conflict("这次执行的 Harness 不存在，无法续接")),
            };
            let workspace = run_svc::workspace_of(tx, &run.id)?;
            if run.isolation == RunIsolation::InPlace
                && let Some(busy) = run_svc::active_in_place(tx, &workspace)?
            {
                return Err(Error::conflict(format!(
                    "工作空间（普通文件夹）正在执行 {}：同一文件夹同时只能运行一个执行",
                    busy.task_key
                )));
            }
            review::set_outcome(tx, &run.id, ReviewOutcome::Rejected, Some(&comment), None)?;
            run_svc::move_task(
                tx,
                &task.id,
                TaskStatus::Doing,
                None,
                Actor::Human,
                "验收退回",
            )?;
            let revision = review::insert_revision(
                tx,
                &task,
                &run,
                HarnessSnapshot {
                    harness_id: instance.id.clone(),
                    name: instance.name.clone(),
                    preset: instance.preset,
                    command: instance.command.clone(),
                    args: instance.args.clone(),
                },
                &workspace,
            )?;
            run_svc::record_event(
                tx,
                &run,
                Actor::Human,
                EventKind::ReviewRejected,
                format!(
                    "{} 第 {} 次执行验收退回，意见：{comment}",
                    run.task_key, run.attempt
                ),
                serde_json::json!({ "runId": run.id, "attempt": run.attempt,
                                    "comment": comment, "revisionRunId": revision.id }),
            )?;
            run_svc::record_event(
                tx,
                &revision,
                Actor::Human,
                EventKind::RunStarted,
                format!(
                    "{} 按退回意见开始第 {} 次执行（Harness「{}」，续接第 {} 次执行的执行目录）",
                    revision.task_key, revision.attempt, revision.harness.name, run.attempt
                ),
                serde_json::json!({ "runId": revision.id, "attempt": revision.attempt,
                                    "isolation": revision.isolation, "revisionOf": run.id }),
            )?;
            run_svc::status_update(
                tx,
                &revision.id,
                RunStatus::Queued,
                Some(format!("按退回意见续接第 {} 次执行", run.attempt)),
            )?;
            run_svc::get(tx, &revision.id)
        })?;
        self.core
            .spawn_driver(&mut registry, revision.clone(), Mode::Revise);
        drop(registry);
        self.sync_role_folders(&revision.team_id);
        self.emit_run(&revision.id);
        Ok(revision)
    }

    /// 回到 READY 的就绪问题（为空表示满足）。
    fn rerun_readiness(&self, task: &Task) -> Result<Vec<String>> {
        let (workspace, refs) = task::workspace_and_refs(&self.lock(), &task.id)?;
        let files = FileCheck::scan(Path::new(&workspace), &refs);
        task::readiness(
            &self.lock(),
            &task.owner_role_id,
            &task.file_refs,
            &task.depends_on,
            &files,
        )
    }

    /// 合并冲突后「已手动合并，标记完成」（D22）：只接受任务最近一次执行，且任务因这次执行的合并冲突而
    /// 受阻（否则 conflict）。校验执行分支已并入基线分支（`git merge-base --is-ancestor`），未并入时拒绝；
    /// 通过后任务完成（同一事务里调用依赖推进钩子），再删除 worktree 与执行分支（清理失败不回滚，结果里
    /// 给出 `cleanupProblem` 并记活动）。写活动与岗位工作日志。
    pub fn resolve_merge_manually(
        &self,
        input: ResolveMergeManuallyInput,
    ) -> Result<ApproveReviewResult> {
        let (first, _) = review::check_conflicted(&self.lock(), &input.run_id)?;
        let _guard = self.begin_review(&first.id, &first.key)?;
        let (task, run) = review::check_conflicted(&self.lock(), &input.run_id)?;
        let workspace = run_svc::workspace_of(&self.lock(), &run.id)?;
        let Some(branch) = run.branch.clone() else {
            return Err(Error::conflict("执行没有记录分支，无法校验是否已合并"));
        };
        let base = run
            .base_branch
            .clone()
            .or_else(|| {
                self.inspect(&workspace)
                    .ok()
                    .and_then(|i| i.git)
                    .and_then(|g| g.branch)
            })
            .ok_or_else(|| Error::conflict("没有记录基线分支，无法校验是否已合并"))?;
        let git = self.review_git();
        let path_env = self.probe.command_path_env();
        let merged = git
            .is_merged_into(Path::new(&workspace), &branch, &base, Some(&path_env))
            .map_err(|err| Error::conflict(format!("无法校验执行分支是否已合并：{err}")))?;
        if !merged {
            return Err(Error::conflict(format!(
                "执行分支 {branch} 还没有并入 {base}：请先在仓库中完成合并并提交，再标记完成"
            )));
        }
        let head = git.branch_head(Path::new(&workspace), &base, Some(&path_env));
        self.write(|tx| {
            let (task, run) = review::check_conflicted(tx, &run.id)?;
            review::set_outcome(tx, &run.id, ReviewOutcome::Approved, None, head.as_deref())?;
            run_svc::move_task(
                tx,
                &task.id,
                TaskStatus::Done,
                None,
                Actor::Human,
                "已手动合并",
            )?;
            run_svc::record_event(
                tx,
                &run,
                Actor::Human,
                EventKind::ReviewApproved,
                format!(
                    "{} 第 {} 次执行的合并冲突已手动解决：{branch} 已并入 {base}，标记完成",
                    run.task_key, run.attempt
                ),
                serde_json::json!({ "runId": run.id, "attempt": run.attempt, "manual": true,
                                    "mergedCommit": head, "branch": branch, "baseBranch": base }),
            )?;
            task::on_task_done(tx, &task.id)
        })?;
        let mut result = ApproveReviewResult {
            outcome: ApproveOutcome::Merged,
            merged_commit: head,
            conflict_files: Vec::new(),
            reason: None,
            cleanup_problem: None,
            task: task.clone(),
            run: run.clone(),
        };
        result.cleanup_problem = self.cleanup_run(&run, &workspace, &branch, Cleanup::Merged);
        self.sync_role_folders(&run.team_id);
        self.emit_run(&run.id);
        let conn = self.lock();
        result.task = task::get(&conn, &task.id)?;
        result.run = run_svc::get(&conn, &run.id)?;
        Ok(result)
    }

    /// 合并冲突后「重新执行」（D22）：fencing 同 `resolve_merge_manually`；须满足就绪条件（否则 conflict）。
    /// 先在活动里记下旧执行分支的名称与提交，再强制删除旧 worktree 与执行分支（未合并的也删除），任务回到
    /// READY；之后由「开始执行」基于最新基线新建执行。清理失败不阻止回到 READY，另记一条活动。
    pub fn rerun_after_conflict(&self, input: RerunAfterConflictInput) -> Result<TaskDetail> {
        let (first, _) = review::check_conflicted(&self.lock(), &input.run_id)?;
        let _guard = self.begin_review(&first.id, &first.key)?;
        let (task, run) = review::check_conflicted(&self.lock(), &input.run_id)?;
        let problems = self.rerun_readiness(&task)?;
        if !problems.is_empty() {
            return Err(Error::conflict(format!(
                "不能重新执行：{}",
                problems.join("；")
            )));
        }
        let workspace = run_svc::workspace_of(&self.lock(), &run.id)?;
        let path_env = self.probe.command_path_env();
        let branch = run.branch.clone();
        let commit = branch.as_deref().and_then(|b| {
            self.review_git()
                .branch_head(Path::new(&workspace), b, Some(&path_env))
        });
        // 先记下要删除的分支与提交，再删除。
        self.write(|tx| {
            let (task, run) = review::check_conflicted(tx, &run.id)?;
            run_svc::record_event(
                tx,
                &run,
                Actor::Human,
                EventKind::ReviewRerun,
                format!(
                    "{} 第 {} 次执行合并冲突后重新执行：删除执行分支 {}（提交 {}）及其 worktree，任务回到 READY",
                    run.task_key,
                    run.attempt,
                    branch.as_deref().unwrap_or("（无）"),
                    commit.as_deref().unwrap_or("（未知）")
                ),
                serde_json::json!({ "runId": run.id, "attempt": run.attempt,
                                    "branch": branch, "commit": commit }),
            )?;
            run_svc::move_task(
                tx,
                &task.id,
                TaskStatus::Ready,
                None,
                Actor::Human,
                "合并冲突后重新执行",
            )
        })?;
        if let Some(branch) = &branch {
            self.cleanup_run(&run, &workspace, branch, Cleanup::Force);
        }
        self.sync_role_folders(&run.team_id);
        self.emit_run(&run.id);
        self.get_task(&task.id)
    }

    /// 删除执行的 worktree 与分支；失败时记一条活动并返回说明。
    fn cleanup_run(
        &self,
        run: &Run,
        workspace: &str,
        branch: &str,
        cleanup: Cleanup,
    ) -> Option<String> {
        let path_env = self.probe.command_path_env();
        let root = self.worktree_root(run);
        let err = self
            .review_git()
            .remove_run_worktree(
                Path::new(workspace),
                &root,
                branch,
                cleanup,
                Some(&path_env),
            )
            .err()?;
        let problem = format!("清理 worktree 或执行分支 {branch} 失败：{err}；可在仓库中手动删除");
        let _ = self.write(|tx| {
            run_svc::record_event(
                tx,
                run,
                Actor::System,
                EventKind::ReviewCleanupFailed,
                format!("{} {problem}", run.task_key),
                serde_json::json!({ "runId": run.id, "problem": problem }),
            )
        });
        Some(problem)
    }

    // ---------------------------------------------------------------- 活动

    /// 按团队分页列出活动，时间倒序；每页默认 50 条、最多 200 条。
    pub fn list_events(&self, input: EventQuery) -> Result<EventPage> {
        event::page(&self.lock(), &input)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 前端命令封装必须与命令清单一一对应。
    #[test]
    fn frontend_wrapper_covers_every_command() {
        let path =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../../apps/desktop/src/api/commands.ts");
        let source = std::fs::read_to_string(&path).expect("read commands.ts");
        let invoked: Vec<&str> = source
            .split("invoke<")
            .skip(1)
            .filter_map(|rest| rest.split_once("(\"").map(|(_, tail)| tail))
            .filter_map(|tail| tail.split('"').next())
            .collect();
        let mut expected: Vec<&str> = COMMANDS
            .iter()
            .chain(TAURI_ONLY_COMMANDS)
            .copied()
            .collect();
        let mut actual = invoked.clone();
        expected.sort_unstable();
        actual.sort_unstable();
        assert_eq!(actual, expected, "commands.ts 与 api::COMMANDS 不一致");
    }
}
