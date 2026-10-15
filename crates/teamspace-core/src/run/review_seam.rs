//! 验收用到的 Git 操作的接缝（M3-01）：内核只通过 [`ReviewGit`] 调用 Git 层，真实实现在
//! [`super::git_review`]（M3-02）。测试可以换成假的实现，验证合并成功、冲突、受阻、清理失败等分支下的
//! 状态规则，而不依赖真实仓库。
//!
//! 这里的类型是内核自己的，与 Git 层的类型解耦：Git 层改动返回值细节时只需改 [`RealGit`] 的转换。

use std::ffi::OsStr;
use std::path::Path;

use super::git_review;
use super::worktree::{Worktree, git};
use crate::model::FileChangeKind;

/// 删除执行分支的方式。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Cleanup {
    /// 合并之后：先校验执行分支已并入该基线分支，再删除。
    Merged(String),
    /// 冲突后重新执行：未合并的分支也删除。
    Force,
}

/// 执行分支相对基线的一个改动文件。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct DiffEntry {
    pub path: String,
    pub change: FileChangeKind,
    pub binary: bool,
    pub added: u32,
    pub removed: u32,
}

/// 一个文件的补丁。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PatchText {
    pub binary: bool,
    pub truncated: bool,
    pub patch: String,
}

/// 合并的结果。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Merge {
    /// 已合并，基线分支上的合并提交。
    Merged { commit: String },
    /// 有冲突，已中止合并、基线不变。
    Conflict { files: Vec<String> },
    /// 现在不能合并（基线分支有未提交改动等），什么也没做。
    Blocked { reason: String },
}

/// 验收用到的 Git 操作。错误为中文说明。
pub(crate) trait ReviewGit: Send + Sync {
    /// 把执行目录（worktree）里的全部改动提交到执行分支；没有改动时返回 `None`。
    fn commit_all(
        &self,
        dir: &Path,
        message: &str,
        path_env: Option<&OsStr>,
    ) -> Result<Option<String>, String>;

    /// `base_commit..head` 的改动文件。
    fn diff_files(
        &self,
        repo_dir: &Path,
        base_commit: &str,
        head: &str,
        path_env: Option<&OsStr>,
    ) -> Result<Vec<DiffEntry>, String>;

    /// `base_commit..head` 中一个文件的补丁；路径必须在 [`ReviewGit::diff_files`] 的结果中。
    fn file_patch(
        &self,
        repo_dir: &Path,
        base_commit: &str,
        head: &str,
        path: &str,
        path_env: Option<&OsStr>,
    ) -> Result<PatchText, String>;

    /// 在工作空间仓库里以 `--no-ff` 把执行分支合并进基线分支。
    fn merge_into_base(
        &self,
        repo_dir: &Path,
        base_branch: &str,
        run_branch: &str,
        message: &str,
        path_env: Option<&OsStr>,
    ) -> Result<Merge, String>;

    /// 删除执行的 worktree 与执行分支：`Merged` 只安全删除已合并的分支；`Force` 连同未合并的分支一起删除
    ///（冲突后重新执行，调用方须先在活动里记下分支名与提交）。
    fn remove_run_worktree(
        &self,
        repo_dir: &Path,
        worktree_root: &Path,
        branch: &str,
        cleanup: Cleanup,
        path_env: Option<&OsStr>,
    ) -> Result<(), String>;

    /// 执行分支是否已并入基线分支（`git merge-base --is-ancestor`）。分支不存在时为错误。
    fn is_merged_into(
        &self,
        repo_dir: &Path,
        run_branch: &str,
        base_branch: &str,
        path_env: Option<&OsStr>,
    ) -> Result<bool, String>;

    /// 分支当前指向的提交；分支不存在时为 `None`。
    fn branch_head(
        &self,
        repo_dir: &Path,
        branch: &str,
        path_env: Option<&OsStr>,
    ) -> Option<String>;

    /// 新建 worktree 时工作空间所在的分支（处于分离 HEAD 时为 `None`）。
    fn base_branch(&self, worktree: &Worktree) -> Option<String>;

    /// 执行目录当前检出的是不是 `refs/heads/<branch>`（`git symbolic-ref HEAD`）；不是时返回说明。
    fn check_on_branch(
        &self,
        dir: &Path,
        branch: &str,
        path_env: Option<&OsStr>,
    ) -> Result<(), String>;

    /// `dir` 所在仓库的根目录（规范路径）；不是 Git 仓库时为 `None`。
    fn repo_root(&self, dir: &Path, path_env: Option<&OsStr>) -> Option<std::path::PathBuf>;
}

/// 真实的 Git 层（M3-02 的 `run::git_review`）。验收用到的 Git 调用都集中在这里，Git 层签名变化时只改这里。
pub(crate) struct RealGit;

impl ReviewGit for RealGit {
    fn commit_all(
        &self,
        dir: &Path,
        message: &str,
        path_env: Option<&OsStr>,
    ) -> Result<Option<String>, String> {
        git_review::commit_all(dir, message, path_env)
    }

    fn diff_files(
        &self,
        repo_dir: &Path,
        base_commit: &str,
        head: &str,
        path_env: Option<&OsStr>,
    ) -> Result<Vec<DiffEntry>, String> {
        Ok(
            git_review::diff_files(repo_dir, base_commit, head, path_env)?
                .into_iter()
                .map(|d| DiffEntry {
                    path: d.path,
                    change: d.change,
                    binary: d.binary,
                    added: d.added,
                    removed: d.removed,
                })
                .collect(),
        )
    }

    fn file_patch(
        &self,
        repo_dir: &Path,
        base_commit: &str,
        head: &str,
        path: &str,
        path_env: Option<&OsStr>,
    ) -> Result<PatchText, String> {
        let p = git_review::file_patch(repo_dir, base_commit, head, path, path_env)?;
        Ok(PatchText {
            binary: p.binary,
            truncated: p.truncated,
            patch: p.patch,
        })
    }

    fn merge_into_base(
        &self,
        repo_dir: &Path,
        base_branch: &str,
        run_branch: &str,
        message: &str,
        path_env: Option<&OsStr>,
    ) -> Result<Merge, String> {
        Ok(
            match git_review::merge_into_base(repo_dir, base_branch, run_branch, message, path_env)?
            {
                git_review::MergeOutcome::Merged { commit } => Merge::Merged { commit },
                git_review::MergeOutcome::Conflict { files } => Merge::Conflict { files },
                git_review::MergeOutcome::Blocked { reason } => Merge::Blocked { reason },
            },
        )
    }

    fn remove_run_worktree(
        &self,
        repo_dir: &Path,
        worktree_root: &Path,
        branch: &str,
        cleanup: Cleanup,
        path_env: Option<&OsStr>,
    ) -> Result<(), String> {
        let cleanup = match &cleanup {
            Cleanup::Merged(base) => git_review::BranchCleanup::MergedInto(base),
            Cleanup::Force => git_review::BranchCleanup::Force,
        };
        git_review::remove_run_worktree(repo_dir, worktree_root, branch, cleanup, path_env)
    }

    fn is_merged_into(
        &self,
        repo_dir: &Path,
        run_branch: &str,
        base_branch: &str,
        path_env: Option<&OsStr>,
    ) -> Result<bool, String> {
        git_review::is_merged_into(repo_dir, run_branch, base_branch, path_env)
    }

    fn branch_head(
        &self,
        repo_dir: &Path,
        branch: &str,
        path_env: Option<&OsStr>,
    ) -> Option<String> {
        git(
            repo_dir,
            &[
                "rev-parse",
                "--verify",
                "--quiet",
                &format!("refs/heads/{branch}"),
            ],
            path_env,
        )
        .ok()
        .map(|s| s.trim().to_owned())
        .filter(|s| !s.is_empty())
    }

    fn base_branch(&self, worktree: &Worktree) -> Option<String> {
        worktree.base_branch.clone()
    }

    fn check_on_branch(
        &self,
        dir: &Path,
        branch: &str,
        path_env: Option<&OsStr>,
    ) -> Result<(), String> {
        let head = git(dir, &["symbolic-ref", "--quiet", "HEAD"], path_env)
            .map(|s| s.trim().to_owned())
            .unwrap_or_default();
        if head == format!("refs/heads/{branch}") {
            return Ok(());
        }
        let current = match head.strip_prefix("refs/heads/") {
            Some(name) => name.to_owned(),
            None if head.is_empty() => "分离的 HEAD".to_owned(),
            None => head,
        };
        Err(format!(
            "执行目录不在执行分支 {branch} 上（当前：{current}）"
        ))
    }

    fn repo_root(&self, dir: &Path, path_env: Option<&OsStr>) -> Option<std::path::PathBuf> {
        let root = git(dir, &["rev-parse", "--show-toplevel"], path_env).ok()?;
        Path::new(root.trim()).canonicalize().ok()
    }
}
