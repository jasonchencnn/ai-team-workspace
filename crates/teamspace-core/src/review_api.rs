//! 验收命令（M3-01，D20 / D22）：`get_review`、`get_review_file_patch`、`approve_review`、`reject_review`、
//! `resolve_merge_manually`、`rerun_after_conflict`，以及它们的并发控制与合并对账。
//!
//! 并发与一致性：
//! - **任务标记**：验收进行中的任务，其他会改动它的入口（状态流转、编辑、开始 / 续接 / 接管执行、再次验收）
//!   一律拒绝（「正在验收合并，请稍候」）。
//! - **仓库锁**：同一仓库（工作空间所在 Git 仓库根目录）同时只有一个「提交—预检—合并—失败恢复」，不同任务
//!   的验收排队进行，Git 层合并失败后的 abort 只会作用于本次合并。
//! - **合并中标记**：调用 Git 合并之前在执行记录上写下目标基线分支与预期 head，结果落库时清除。合并之后、
//!   落库之前退出或落库失败时标记会留下；打开验收页、再次通过与应用启动时据 Git 事实对账：执行分支已并入
//!   基线就补记通过（任务完成、推进下游、清理），否则清除标记回到待验收。
//! - **结果按执行落库**：合并结果写入时只在「任务仍是 REVIEW、这次执行仍是最近一次且未验收」时改动任务
//!   状态，否则只记 Git 事实与活动。

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::{Condvar, Mutex};

use crate::api::{Core, Teamspace};
use crate::error::{Error, Result};
use crate::model::*;
use crate::run::Mode;
use crate::run::review_seam::{self, Cleanup, ReviewGit};
use crate::service::review::{self, Merging};
use crate::service::run::{self as run_svc};
use crate::service::task::FileCheck;
use crate::service::{harness, task, team};

/// 验收的并发控制状态（属于 [`Core`]）。
#[derive(Default)]
pub(crate) struct ReviewLocks {
    state: Mutex<LockState>,
    released: Condvar,
}

#[derive(Default)]
struct LockState {
    /// 正在验收的任务。
    tasks: HashSet<String>,
    /// 正在提交或合并的仓库（规范路径）。
    repos: HashSet<String>,
}

impl ReviewLocks {
    fn state(&self) -> std::sync::MutexGuard<'_, LockState> {
        self.state.lock().unwrap_or_else(|p| p.into_inner())
    }
}

/// 验收标记与仓库锁；drop 时释放并唤醒排队的验收。
pub(crate) struct ReviewGuard<'a> {
    locks: &'a ReviewLocks,
    task_id: Option<String>,
    repo: String,
}

impl Drop for ReviewGuard<'_> {
    fn drop(&mut self) {
        let mut state = self.locks.state();
        if let Some(task) = &self.task_id {
            state.tasks.remove(task);
        }
        state.repos.remove(&self.repo);
        drop(state);
        self.locks.released.notify_all();
    }
}

/// 合并结果落库后的附加信息。
struct Settled {
    advanced: Vec<String>,
    cleanup_problem: Option<String>,
}

impl Core {
    /// 验收用的 Git 层。
    pub(crate) fn review_git(&self) -> std::sync::Arc<dyn ReviewGit> {
        std::sync::Arc::clone(&self.review_git.read().unwrap_or_else(|p| p.into_inner()))
    }

    /// 仅测试用：替换验收用的 Git 层。
    #[cfg(test)]
    pub(crate) fn set_review_git(&self, git: std::sync::Arc<dyn ReviewGit>) {
        *self.review_git.write().unwrap_or_else(|p| p.into_inner()) = git;
    }

    /// 验收进行中时拒绝对该任务的其他改动入口。
    pub(crate) fn ensure_not_reviewing(&self, task_id: &str) -> Result<()> {
        if self.review_locks.state().tasks.contains(task_id) {
            return Err(Error::conflict("正在验收合并，请稍候"));
        }
        Ok(())
    }

    /// 工作空间所在仓库的规范路径（同一仓库的不同工作空间共用一把锁）；不是 Git 仓库时为工作空间自身。
    fn repo_key(&self, workspace: &str) -> String {
        let path_env = self.probe.command_path_env();
        let dir = Path::new(workspace);
        self.review_git()
            .repo_root(dir, Some(&path_env))
            .or_else(|| dir.canonicalize().ok())
            .unwrap_or_else(|| dir.to_path_buf())
            .display()
            .to_string()
    }

    /// 只取仓库锁（读验收页时提交执行目录、对账用），等前一个合并结束。
    fn lock_repo(&self, workspace: &str) -> ReviewGuard<'_> {
        let repo = self.repo_key(workspace);
        let mut state = self.review_locks.state();
        while state.repos.contains(&repo) {
            state = self
                .review_locks
                .released
                .wait(state)
                .unwrap_or_else(|p| p.into_inner());
        }
        state.repos.insert(repo.clone());
        ReviewGuard {
            locks: &self.review_locks,
            task_id: None,
            repo,
        }
    }

    /// 开始验收某任务：同一任务已在验收时立即拒绝；同一仓库有别的合并时排队等待。
    fn begin_review(&self, task_id: &str, workspace: &str) -> Result<ReviewGuard<'_>> {
        let repo = self.repo_key(workspace);
        let mut state = self.review_locks.state();
        if state.tasks.contains(task_id) {
            return Err(Error::conflict("正在验收合并，请稍候"));
        }
        state.tasks.insert(task_id.to_owned());
        while state.repos.contains(&repo) {
            state = self
                .review_locks
                .released
                .wait(state)
                .unwrap_or_else(|p| p.into_inner());
        }
        state.repos.insert(repo.clone());
        Ok(ReviewGuard {
            locks: &self.review_locks,
            task_id: Some(task_id.to_owned()),
            repo,
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

    /// 把 Git 执行目录里未提交的改动提交到执行分支。先校验执行目录当前检出的就是执行分支，不是时不提交
    /// 并返回说明；执行目录已不存在时什么也不做。返回新提交（没有改动时为 `None`）。
    pub(crate) fn commit_run_dir(
        &self,
        run: &Run,
        message: &str,
    ) -> std::result::Result<Option<String>, String> {
        let Some(branch) = run.branch.as_deref() else {
            return Err("执行没有记录分支".to_owned());
        };
        let dir = Path::new(&run.exec_dir);
        if !dir.is_dir() {
            return Ok(None);
        }
        let git = self.review_git();
        let path_env = self.probe.command_path_env();
        git.check_on_branch(dir, branch, Some(&path_env))?;
        git.commit_all(dir, message, Some(&path_env))
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

    /// 启动时：对所有留有「合并中」标记的执行对账（失败只记日志）。
    pub(crate) fn reconcile_merges(&self) {
        let Ok(ids) = review::merging_runs(&self.lock()) else {
            return;
        };
        for id in ids {
            let workspace = match run_svc::workspace_of(&self.lock(), &id) {
                Ok(w) => w,
                Err(_) => continue,
            };
            let _repo = self.lock_repo(&workspace);
            if let Err(err) = self.reconcile_merge(&id) {
                eprintln!("teamspace review {id}: 合并对账失败：{err}");
            }
        }
    }

    /// 对账一次执行的「合并中」标记（调用方持有仓库锁，且这次执行不在验收中）：执行分支已并入目标基线
    /// 就补记通过（任务仍待验收时转为完成并推进下游），然后清理；未并入则清除标记。Git 读取失败时保留
    /// 标记，返回错误。没有标记时什么也不做。返回是否补记了通过。
    fn reconcile_merge(&self, run_id: &str) -> Result<bool> {
        let (run, marker) = {
            let conn = self.lock();
            (
                run_svc::get(&conn, run_id)?,
                review::merging(&conn, run_id)?,
            )
        };
        let Some(Merging { base_branch, .. }) = marker else {
            return Ok(false);
        };
        let Some(branch) = run.branch.clone() else {
            self.write(|tx| review::clear_merging(tx, run_id))?;
            return Ok(false);
        };
        let workspace = run_svc::workspace_of(&self.lock(), run_id)?;
        let git = self.review_git();
        let path_env = self.probe.command_path_env();
        let merged = git
            .is_merged_into(
                Path::new(&workspace),
                &branch,
                &base_branch,
                Some(&path_env),
            )
            .map_err(|err| Error::conflict(format!("无法确认上次合并的结果：{err}")))?;
        if !merged {
            self.write(|tx| review::clear_merging(tx, run_id))?;
            return Ok(false);
        }
        let commit = git
            .branch_head(Path::new(&workspace), &base_branch, Some(&path_env))
            .unwrap_or_default();
        self.settle_merged(&run, &workspace, &branch, &base_branch, &commit, true)?;
        Ok(true)
    }

    /// 合并成功落库：清除合并中标记；执行记为通过并记下合并提交；任务仍是这次执行的待验收时转为完成并
    /// 推进下游（同一事务），否则只记活动。之后清理 worktree 与执行分支。
    fn settle_merged(
        &self,
        run: &Run,
        workspace: &str,
        branch: &str,
        base: &str,
        commit: &str,
        reconciled: bool,
    ) -> Result<Settled> {
        let advanced = self.write(|tx| {
            review::clear_merging(tx, &run.id)?;
            let current = review::is_current(tx, &run.id)?;
            let commit_opt = (!commit.is_empty()).then_some(commit);
            review::set_outcome(tx, &run.id, ReviewOutcome::Approved, None, commit_opt)?;
            let short = &commit[..commit.len().min(7)];
            let how = if reconciled {
                "（上次合并后没有记录结果，按 Git 事实补记）"
            } else {
                ""
            };
            let tail = if current {
                ""
            } else {
                "；合并期间任务已被改动，任务状态未变"
            };
            run_svc::record_event(
                tx,
                run,
                Actor::Human,
                EventKind::ReviewApproved,
                format!(
                    "{} 第 {} 次执行验收通过，已把 {branch} 合并进 {base}（{short}）{how}{tail}",
                    run.task_key, run.attempt
                ),
                serde_json::json!({ "runId": run.id, "attempt": run.attempt,
                                    "mergedCommit": commit, "branch": branch,
                                    "baseBranch": base, "reconciled": reconciled,
                                    "taskUpdated": current }),
            )?;
            if !current {
                return Ok(Vec::new());
            }
            run_svc::move_task(
                tx,
                &run.task_id,
                TaskStatus::Done,
                None,
                Actor::Human,
                "验收通过并合并",
            )?;
            task::on_task_done(tx, &run.task_id)
        })?;
        let cleanup_problem =
            self.cleanup_run(run, workspace, branch, Cleanup::Merged(base.to_owned()));
        self.sync_role_folders(&run.team_id);
        self.emit_run(&run.id);
        Ok(Settled {
            advanced,
            cleanup_problem,
        })
    }

    /// 自动就绪的下游任务。
    fn advanced_tasks(&self, ids: &[String]) -> Vec<AdvancedTask> {
        let conn = self.lock();
        ids.iter()
            .filter_map(|id| task::get(&conn, id).ok())
            .map(|t| AdvancedTask {
                id: t.id,
                key: t.key,
                title: t.title,
            })
            .collect()
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
}

fn review_file(path: String, change: FileChangeKind) -> ReviewFile {
    ReviewFile {
        path,
        change,
        binary: false,
        added: None,
        removed: None,
        has_patch: false,
    }
}

impl Teamspace {
    /// 验收页数据（见 [`ReviewView`]）：任务、待验收的执行（任务处于 REVIEW 时最近一次、尚未验收的执行）、
    /// 摘要、改动文件、能否通过 / 退回及原因、历次退回意见；任务因合并冲突受阻时为那次冲突的执行
    /// （`conflict`，附冲突文件）。
    ///
    /// Git 执行：先对账上次没落库的合并；执行不在活动中时把执行目录里未提交的改动提交到执行分支（先校验
    /// 执行目录在执行分支上），预览即执行分支相对基线提交的 diff，`headCommit` 为执行分支当前 head。
    /// 普通文件夹：执行记录里的改动文件（多轮退回合在一起），没有 diff。
    pub fn get_review(&self, task_id: &str) -> Result<ReviewView> {
        // 对账与提交都要在仓库锁内进行；验收进行中的任务跳过（由进行中的验收处理）。
        let latest = run_svc::latest_for_task(&self.lock(), task_id)?;
        let mut commit_problem = None;
        if let Some(run) = latest.filter(|r| r.isolation == RunIsolation::Worktree)
            && self.ensure_not_reviewing(task_id).is_ok()
        {
            let workspace = run_svc::workspace_of(&self.lock(), &run.id)?;
            let _repo = self.lock_repo(&workspace);
            let pending = {
                let conn = self.lock();
                let task = task::get(&conn, task_id)?;
                review::pending_run(&conn, &task)?
            };
            let _ = self.reconcile_merge(&run.id);
            let still_pending = pending.is_some_and(|p| p.id == run.id)
                && review::merging(&self.lock(), &run.id)?.is_none()
                && run_svc::get(&self.lock(), &run.id)?.review.is_none();
            if still_pending {
                let message = format!(
                    "teamspace: {} attempt {} (review)",
                    run.task_key, run.attempt
                );
                commit_problem = self.commit_run_dir(&run, &message).err();
            }
        }

        let (task, run, rejections, workspace, files_in_place, instance, conflict, merging) = {
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
            let conflict_files = match &conflicted {
                Some(r) => review::conflict_files(&conn, &r.id)?,
                None => Vec::new(),
            };
            let merging = match &run {
                Some(r) => review::merging(&conn, &r.id)?.is_some(),
                None => false,
            };
            (
                task,
                run,
                rejections,
                workspace,
                files,
                instance,
                conflicted.map(|_| conflict_files),
                merging,
            )
        };
        let mut problems = Vec::new();
        let mut approve_problems = Vec::new();
        let mut reject_problems = Vec::new();
        let mut files = Vec::new();
        let mut files_problem = None;
        let mut head_commit = None;
        if conflict.is_none() && task.status != TaskStatus::Review {
            problems.push(format!("任务当前是「{}」，不在待验收", task.status.label()));
        }
        if merging {
            problems.push("这次执行正在合并（或上次合并的结果尚未确认），请稍后刷新".to_owned());
        }
        match &run {
            None if task.status == TaskStatus::Review => problems.push(
                "没有待验收的执行（任务未经新的执行就提交了验收），请用状态流转直接完成或退回 READY"
                    .to_owned(),
            ),
            None => {}
            Some(run) => {
                match run.isolation {
                    RunIsolation::InPlace => {
                        files = files_in_place
                            .unwrap_or_default()
                            .into_iter()
                            .map(|f| review_file(f.path, f.change))
                            .collect();
                    }
                    RunIsolation::Worktree => match (&run.base_commit, &run.branch) {
                        (Some(base), Some(branch)) => {
                            let git = self.review_git();
                            let path_env = self.probe.command_path_env();
                            head_commit = git.branch_head(Path::new(&workspace), branch, Some(&path_env));
                            match git.diff_files(Path::new(&workspace), base, branch, Some(&path_env)) {
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
                                        .map(|f| review_file(f.path.clone(), f.change))
                                        .collect();
                                }
                            }
                            if let Some(err) = &commit_problem {
                                let text = format!("执行目录里的改动无法提交到执行分支：{err}");
                                files_problem = Some(match files_problem.take() {
                                    Some(p) => format!("{p}；{text}"),
                                    None => text.clone(),
                                });
                                approve_problems.push(text);
                            }
                            if head_commit.is_none() {
                                approve_problems.push(format!("执行分支 {branch} 不存在，无法合并"));
                            }
                        }
                        _ => approve_problems.push("执行没有记录分支或基线提交，无法合并".to_owned()),
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
        let in_conflict = conflict.is_some();
        let rerun_blockers = if in_conflict {
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
            !in_conflict && problems.is_empty() && approve_problems.is_empty() && run.is_some();
        let can_reject =
            !in_conflict && problems.is_empty() && reject_problems.is_empty() && run.is_some();
        if !in_conflict {
            problems.extend(approve_problems);
            problems.extend(reject_problems);
        }
        let can_resolve_manually =
            in_conflict && run.as_ref().is_some_and(|r| r.base_branch.is_some());
        Ok(ReviewView {
            summary: run.as_ref().and_then(|r| r.summary.clone()),
            isolation: run.as_ref().map(|r| r.isolation),
            base_branch: run.as_ref().and_then(|r| r.base_branch.clone()),
            branch: run.as_ref().and_then(|r| r.branch.clone()),
            head_commit,
            merging,
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
            conflict: in_conflict,
            conflict_files: conflict.unwrap_or_default(),
            can_resolve_manually,
            can_rerun: in_conflict && rerun_blockers.is_empty(),
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

    /// 通过验收（D20）。只接受任务当前待验收的执行（任务处于 REVIEW、是它的最近一次执行、尚未验收），否则
    /// conflict；验收进行中的任务拒绝，同一仓库的合并排队进行。
    ///
    /// - 普通文件夹：任务直接完成（DONE）。
    /// - Git：`expectedHead` 必须等于执行分支当前 head（即验收页预览的内容），否则 conflict「改动已变化，请刷新
    ///   验收页」；执行目录须在执行分支上。合并前写下「合并中」标记（没有记录基线分支的旧执行在这里确定并写回
    ///   基线分支），再以 `--no-ff` 合并进基线分支（不推送）。合并成功 → 任务完成、推进下游、删除 worktree 与
    ///   执行分支（清理失败不回滚，结果给出 `cleanupProblem`）；冲突 → 合并已中止、基线不变，任务转为受阻
    ///   （冲突文件记在执行上），分支与 worktree 保留；不能合并 → 任务保持待验收，结果给出原因。合并期间任务
    ///   被别处改动时只记 Git 事实与活动，不改任务状态。
    pub fn approve_review(&self, input: ApproveReviewInput) -> Result<ApproveReviewResult> {
        let (first, first_run) = review::check_pending(&self.lock(), &input.run_id)?;
        let workspace = run_svc::workspace_of(&self.lock(), &first_run.id)?;
        let _guard = self.begin_review(&first.id, &workspace)?;
        // 上次没落库的合并先对账（可能已经并入）。
        self.reconcile_merge(&input.run_id)?;
        let (task, run) = review::check_pending(&self.lock(), &input.run_id)?;
        let mut result = ApproveReviewResult {
            outcome: ApproveOutcome::Completed,
            merged_commit: None,
            conflict_files: Vec::new(),
            reason: None,
            cleanup_problem: None,
            advanced_tasks: Vec::new(),
            task: task.clone(),
            run: run.clone(),
        };
        match run.isolation {
            RunIsolation::InPlace => {
                let advanced = self.write(|tx| {
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
                result.advanced_tasks = self.advanced_tasks(&advanced);
                self.sync_role_folders(&run.team_id);
                self.emit_run(&run.id);
            }
            RunIsolation::Worktree => {
                self.approve_merge(&task, &run, &workspace, input.expected_head, &mut result)?
            }
        }
        let conn = self.lock();
        result.task = task::get(&conn, &task.id)?;
        result.run = run_svc::get(&conn, &run.id)?;
        Ok(result)
    }

    /// Git 工作空间的「通过」（见 [`Teamspace::approve_review`]）。调用方持有验收标记与仓库锁。
    fn approve_merge(
        &self,
        task: &Task,
        run: &Run,
        workspace: &str,
        expected_head: Option<String>,
        result: &mut ApproveReviewResult,
    ) -> Result<()> {
        let git = self.review_git();
        let path_env = self.probe.command_path_env();
        let Some(branch) = run.branch.clone() else {
            return Err(Error::conflict("执行没有记录分支，无法合并"));
        };
        let Some(expected) = expected_head.filter(|h| !h.trim().is_empty()) else {
            return Err(Error::invalid("缺少 expectedHead：请刷新验收页后再通过"));
        };
        let head = git.branch_head(Path::new(workspace), &branch, Some(&path_env));
        if head.as_deref() != Some(expected.trim()) {
            return Err(Error::conflict("改动已变化，请刷新验收页"));
        }
        let blocked = |reason: String| -> Result<()> {
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
            self.sync_role_folders(&run.team_id);
            Ok(())
        };
        // 执行目录必须仍在执行分支上（否则分支 head 之外的改动去向不明）。
        if Path::new(&run.exec_dir).is_dir()
            && let Err(err) =
                git.check_on_branch(Path::new(&run.exec_dir), &branch, Some(&path_env))
        {
            result.outcome = ApproveOutcome::Blocked;
            result.reason = Some(err.clone());
            return blocked(err);
        }
        // 基线分支：执行开始时记下的；M3 之前的执行没有记录，用工作空间当前所在的分支并写回执行记录。
        let base = match run.base_branch.clone().or_else(|| {
            self.inspect(workspace)
                .ok()
                .and_then(|info| info.git)
                .and_then(|git| git.branch)
        }) {
            Some(base) => base,
            None => {
                return Err(Error::conflict(
                    "没有记录基线分支，工作空间也不在任何分支上，无法确定合并到哪里",
                ));
            }
        };
        self.write(|tx| {
            review::check_pending(tx, &run.id)?;
            review::mark_merging(tx, &run.id, &base, &expected)
        })?;
        let outcome = git
            .merge_into_base(
                Path::new(workspace),
                &base,
                &branch,
                &format!("merge: {} {}", task.key, task.title),
                Some(&path_env),
            )
            .unwrap_or_else(|err| review_seam::Merge::Blocked {
                reason: format!("合并失败：{err}"),
            });
        match outcome {
            review_seam::Merge::Merged { commit } => {
                result.outcome = ApproveOutcome::Merged;
                result.merged_commit = Some(commit.clone());
                let settled = self.settle_merged(run, workspace, &branch, &base, &commit, false)?;
                result.advanced_tasks = self.advanced_tasks(&settled.advanced);
                result.cleanup_problem = settled.cleanup_problem;
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
                    review::clear_merging(tx, &run.id)?;
                    let current = review::is_current(tx, &run.id)?;
                    if current {
                        review::set_outcome(
                            tx,
                            &run.id,
                            ReviewOutcome::Conflict,
                            Some(&reason),
                            None,
                        )?;
                        review::set_conflict_files(tx, &run.id, &files)?;
                        run_svc::move_task(
                            tx,
                            &task.id,
                            TaskStatus::Blocked,
                            Some(reason.clone()),
                            Actor::Human,
                            "验收合并冲突",
                        )?;
                    }
                    let tail = if current {
                        ""
                    } else {
                        "；合并期间任务已被改动，任务状态未变"
                    };
                    run_svc::record_event(
                        tx,
                        run,
                        Actor::Human,
                        EventKind::ReviewConflict,
                        format!(
                            "{} 第 {} 次执行验收通过但合并冲突：{list}{tail}",
                            run.task_key, run.attempt
                        ),
                        serde_json::json!({ "runId": run.id, "attempt": run.attempt,
                                            "conflictFiles": files, "branch": branch,
                                            "baseBranch": base, "taskUpdated": current }),
                    )
                })?;
                self.sync_role_folders(&run.team_id);
                self.emit_run(&run.id);
            }
            review_seam::Merge::Blocked { reason } => {
                result.outcome = ApproveOutcome::Blocked;
                result.reason = Some(reason.clone());
                self.write(|tx| review::clear_merging(tx, &run.id))?;
                blocked(reason)?;
            }
        }
        Ok(())
    }

    /// 退回验收：意见必填（去掉首尾空白后不能为空，最多 2000 字）；只接受任务当前待验收的执行（否则
    /// conflict），合并中的执行不能退回。被退回的执行记下意见，任务回到执行中，新建一次执行（`revisionOf`
    /// 指向被退回的执行）在**同一执行目录**续接：Agent 支持 `session/load` 时续接原会话、把意见作为新提示
    /// 发送；不支持时在同一目录新开会话，提示带上完整任务说明、上次摘要与意见。不新建 worktree。返回状态为
    /// queued 的新执行。
    pub fn reject_review(&self, input: RejectReviewInput) -> Result<Run> {
        let comment = crate::service::text("退回意见", &input.comment, review::MAX_COMMENT, true)?;
        let (task, run) = review::check_pending(&self.lock(), &input.run_id)?;
        let workspace = run_svc::workspace_of(&self.lock(), &run.id)?;
        let _guard = self.begin_review(&task.id, &workspace)?;
        self.reconcile_merge(&run.id)?;
        if review::merging(&self.lock(), &run.id)?.is_some() {
            return Err(Error::conflict("这次执行正在合并，不能退回；请刷新验收页"));
        }
        if !Path::new(&run.exec_dir).is_dir() {
            return Err(Error::conflict(format!(
                "执行目录已不存在：{}，无法在原目录续接；请把任务退回 READY 后重新执行",
                run.exec_dir
            )));
        }
        let _exec = self.runs.exec_lock();
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
        self.core_arc()
            .spawn_driver(&mut registry, revision.clone(), Mode::Revise);
        drop(registry);
        self.sync_role_folders(&revision.team_id);
        self.emit_run(&revision.id);
        Ok(revision)
    }

    /// 合并冲突后「已手动合并，标记完成」（D22）：只接受任务最近一次执行，且任务因这次执行的合并冲突而
    /// 受阻（否则 conflict）。只用执行记录上的基线分支（没有时拒绝），校验执行分支已并入它
    /// （`git merge-base --is-ancestor`），未并入时拒绝；通过后任务完成（同一事务里推进下游），再删除 worktree
    /// 与执行分支（清理失败不回滚，结果给出 `cleanupProblem` 并记活动）。写活动与岗位工作日志。
    pub fn resolve_merge_manually(
        &self,
        input: ResolveMergeManuallyInput,
    ) -> Result<ApproveReviewResult> {
        let (first, first_run) = review::check_conflicted(&self.lock(), &input.run_id)?;
        let workspace = run_svc::workspace_of(&self.lock(), &first_run.id)?;
        let _guard = self.begin_review(&first.id, &workspace)?;
        let (task, run) = review::check_conflicted(&self.lock(), &input.run_id)?;
        let Some(branch) = run.branch.clone() else {
            return Err(Error::conflict("执行没有记录分支，无法校验是否已合并"));
        };
        let Some(base) = run.base_branch.clone() else {
            return Err(Error::conflict("执行没有记录基线分支，无法校验是否已合并"));
        };
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
        let advanced = self.write(|tx| {
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
        let cleanup_problem = self.cleanup_run(&run, &workspace, &branch, Cleanup::Merged(base));
        self.sync_role_folders(&run.team_id);
        self.emit_run(&run.id);
        let advanced_tasks = self.advanced_tasks(&advanced);
        let conn = self.lock();
        Ok(ApproveReviewResult {
            outcome: ApproveOutcome::Merged,
            merged_commit: head,
            conflict_files: Vec::new(),
            reason: None,
            cleanup_problem,
            advanced_tasks,
            task: task::get(&conn, &task.id)?,
            run: run_svc::get(&conn, &run.id)?,
        })
    }

    /// 合并冲突后「重新执行」（D22）：fencing 同 `resolve_merge_manually`；须满足就绪条件（否则 conflict）。
    /// 先在活动里记下旧执行分支的名称与提交，再强制删除旧 worktree 与执行分支（未合并的也删除），任务回到
    /// READY；之后由「开始执行」基于最新基线新建执行。清理失败不阻止回到 READY，另记一条活动。
    pub fn rerun_after_conflict(&self, input: RerunAfterConflictInput) -> Result<TaskDetail> {
        let (first, first_run) = review::check_conflicted(&self.lock(), &input.run_id)?;
        let workspace = run_svc::workspace_of(&self.lock(), &first_run.id)?;
        let task_id = first.id.clone();
        {
            let _guard = self.begin_review(&first.id, &workspace)?;
            let (task, run) = review::check_conflicted(&self.lock(), &input.run_id)?;
            let problems = self.rerun_readiness(&task)?;
            if !problems.is_empty() {
                return Err(Error::conflict(format!(
                    "不能重新执行：{}",
                    problems.join("；")
                )));
            }
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
        }
        self.get_task(&task_id)
    }
}
