//! Git 验收层：提交执行改动、读取逐文件 diff、本地合并与清理（D20）。

use std::collections::{BTreeMap, BTreeSet};
use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::model::FileChangeKind;
use crate::run::worktree::git;

/// 单个文件补丁的最大字节数。
pub const MAX_PATCH_BYTES: usize = 256 * 1024;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileDiff {
    pub path: String,
    pub change: FileChangeKind,
    pub binary: bool,
    pub added: u32,
    pub removed: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FilePatch {
    pub path: String,
    pub binary: bool,
    pub truncated: bool,
    pub patch: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MergeOutcome {
    Merged { commit: String },
    Conflict { files: Vec<String> },
    Blocked { reason: String },
}

const IDENTITY: &[&str] = &[
    "-c",
    "user.name=Teamspace",
    "-c",
    "user.email=teamspace@localhost",
    "-c",
    "commit.gpgsign=false",
    "-c",
    "core.hooksPath=/dev/null",
];

fn repo_root(dir: &Path, path_env: Option<&OsStr>) -> Result<PathBuf, String> {
    let root = git(dir, &["rev-parse", "--show-toplevel"], path_env)
        .map_err(|err| format!("无法读取仓库根目录：{err}"))?;
    Path::new(root.strip_suffix('\n').unwrap_or(&root))
        .canonicalize()
        .map_err(|err| format!("无法定位仓库根目录：{err}"))
}

fn resolve_commit(dir: &Path, revision: &str, path_env: Option<&OsStr>) -> Result<String, String> {
    git(
        dir,
        &[
            "rev-parse",
            "--verify",
            "--end-of-options",
            &format!("{revision}^{{commit}}"),
        ],
        path_env,
    )
    .map(|value| value.trim().to_owned())
    .map_err(|err| format!("无法读取提交 {revision}：{err}"))
}

fn with_identity(dir: &Path, args: &[&str], path_env: Option<&OsStr>) -> Result<String, String> {
    let mut configured = IDENTITY.to_vec();
    configured.extend_from_slice(args);
    git(dir, &configured, path_env)
}

fn branch_ref(branch: &str) -> String {
    format!("refs/heads/{branch}")
}

fn valid_branch(dir: &Path, branch: &str, path_env: Option<&OsStr>) -> bool {
    !branch.is_empty()
        && !branch.starts_with('-')
        && git(dir, &["check-ref-format", &branch_ref(branch)], path_env).is_ok()
}

/// 提交全部已跟踪与未被忽略的未跟踪改动；无改动时不创建空提交。
pub fn commit_all(
    dir: &Path,
    message: &str,
    path_env: Option<&OsStr>,
) -> Result<Option<String>, String> {
    let root = repo_root(dir, path_env)?;
    git(&root, &["add", "--all", "--", "."], path_env)
        .map_err(|err| format!("暂存执行改动失败：{err}"))?;
    let staged = git(
        &root,
        &["diff", "--cached", "--name-only", "-z", "--"],
        path_env,
    )
    .map_err(|err| format!("读取暂存改动失败：{err}"))?;
    if staged.is_empty() {
        return Ok(None);
    }
    with_identity(
        &root,
        &[
            "commit",
            "--author=Teamspace <teamspace@localhost>",
            "-m",
            message,
        ],
        path_env,
    )
    .map_err(|err| format!("提交执行改动失败：{err}"))?;
    resolve_commit(&root, "HEAD", path_env).map(Some)
}

/// 两个提交间按路径排序的逐文件改动；改名按删除与新增统计。
pub fn diff_files(
    repo_dir: &Path,
    base_commit: &str,
    head: &str,
    path_env: Option<&OsStr>,
) -> Result<Vec<FileDiff>, String> {
    let root = repo_root(repo_dir, path_env)?;
    let base = resolve_commit(&root, base_commit, path_env)?;
    let head = resolve_commit(&root, head, path_env)?;
    let names = diff(&root, &base, &head, &["--name-status", "-z"], path_env)?;
    let stats = diff(&root, &base, &head, &["--numstat", "-z"], path_env)?;
    let mut files = BTreeMap::new();
    let mut parts = names.split_terminator('\0');
    while let Some(status) = parts.next() {
        let path = parts.next().ok_or("Git 文件改动列表格式不完整")?;
        let change = match status {
            "A" => FileChangeKind::Added,
            "D" => FileChangeKind::Deleted,
            "M" | "T" => FileChangeKind::Modified,
            _ => return Err(format!("无法识别 Git 文件改动类型：{status}")),
        };
        files.insert(
            path.to_owned(),
            FileDiff {
                path: path.to_owned(),
                change,
                binary: false,
                added: 0,
                removed: 0,
            },
        );
    }
    for record in stats.split_terminator('\0') {
        let mut fields = record.splitn(3, '\t');
        let added = fields.next().ok_or("Git 行数统计格式不完整")?;
        let removed = fields.next().ok_or("Git 行数统计格式不完整")?;
        let path = fields.next().ok_or("Git 行数统计缺少文件路径")?;
        let file = files.get_mut(path).ok_or("Git 文件列表与行数统计不一致")?;
        file.binary = added == "-" && removed == "-";
        if !file.binary {
            file.added = added.parse().map_err(|_| "Git 新增行数无效或超出范围")?;
            file.removed = removed.parse().map_err(|_| "Git 删除行数无效或超出范围")?;
        }
    }
    Ok(files.into_values().collect())
}

fn diff(
    dir: &Path,
    base: &str,
    head: &str,
    options: &[&str],
    path_env: Option<&OsStr>,
) -> Result<String, String> {
    let mut args = vec![
        "diff",
        "--no-renames",
        "--no-ext-diff",
        "--no-textconv",
        "--no-color",
        "--ignore-submodules=none",
    ];
    args.extend_from_slice(options);
    args.extend_from_slice(&[base, head, "--"]);
    git(dir, &args, path_env).map_err(|err| format!("读取 Git 差异失败：{err}"))
}

/// 只返回改动列表内指定文件的补丁；二进制补丁为空，过大的补丁按行截断。
pub fn file_patch(
    repo_dir: &Path,
    base_commit: &str,
    head: &str,
    path: &str,
    path_env: Option<&OsStr>,
) -> Result<FilePatch, String> {
    let root = repo_root(repo_dir, path_env)?;
    let base = resolve_commit(&root, base_commit, path_env)?;
    let head = resolve_commit(&root, head, path_env)?;
    let file = diff_files(&root, &base, &head, path_env)?
        .into_iter()
        .find(|file| file.path == path)
        .ok_or_else(|| format!("文件不在本次改动列表中，无法读取补丁：{path}"))?;
    let mut patch = String::new();
    if !file.binary {
        patch = git(
            &root,
            &[
                "diff",
                "--no-renames",
                "--no-ext-diff",
                "--no-textconv",
                "--no-color",
                "--ignore-submodules=none",
                "--patch",
                &base,
                &head,
                "--",
                &format!(":(top,literal){path}"),
            ],
            path_env,
        )
        .map_err(|err| format!("读取文件补丁失败：{err}"))?;
    }
    let truncated = patch.len() > MAX_PATCH_BYTES;
    if truncated {
        let end = patch.as_bytes()[..MAX_PATCH_BYTES]
            .iter()
            .rposition(|byte| *byte == b'\n')
            .map_or(0, |index| index + 1);
        patch.truncate(end);
    }
    Ok(FilePatch {
        path: path.to_owned(),
        binary: file.binary,
        truncated,
        patch,
    })
}

fn blocked(reason: impl Into<String>) -> MergeOutcome {
    MergeOutcome::Blocked {
        reason: reason.into(),
    }
}

fn preflight(
    dir: &Path,
    base: &str,
    run: &str,
    path_env: Option<&OsStr>,
) -> Result<Result<String, Vec<String>>, String> {
    let mut command = Command::new("git");
    command
        .arg("-C")
        .arg(dir)
        .args(["merge-tree", "--write-tree", "--name-only", "-z", base, run]);
    if let Some(path) = path_env {
        command.env("PATH", path);
    }
    crate::system_probe::git_env(&mut command);
    let output = command
        .output()
        .map_err(|err| format!("无法运行 Git 合并预检：{err}"))?;
    if !output.status.success() && output.status.code() != Some(1) {
        return Err(format!(
            "Git 合并预检失败：{}",
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    let text = String::from_utf8_lossy(&output.stdout);
    let mut fields = text.split('\0');
    let tree = fields.next().ok_or("Git 合并预检未返回结果树")?;
    if !matches!(tree.len(), 40 | 64) || !tree.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err("Git 合并预检返回了无效结果树".into());
    }
    if output.status.success() {
        Ok(Ok(tree.to_owned()))
    } else {
        let mut files: Vec<String> = fields
            .take_while(|field| !field.is_empty())
            .map(str::to_owned)
            .collect();
        files.sort();
        files.dedup();
        Ok(Err(files))
    }
}

fn checkout_for_branch(
    dir: &Path,
    branch: &str,
    path_env: Option<&OsStr>,
) -> Result<Vec<PathBuf>, String> {
    let listing = git(dir, &["worktree", "list", "--porcelain", "-z"], path_env)
        .map_err(|err| format!("无法读取 worktree 列表：{err}"))?;
    let mut checkouts = Vec::new();
    let mut root = None;
    for field in listing.split('\0') {
        if let Some(path) = field.strip_prefix("worktree ") {
            root = Some(PathBuf::from(path));
        } else if field.strip_prefix("branch ") == Some(branch) {
            checkouts.push(root.clone().ok_or("Git worktree 列表缺少目录")?);
        } else if field.is_empty() {
            root = None;
        }
    }
    Ok(checkouts)
}

fn operation_in_progress(dir: &Path, path_env: Option<&OsStr>) -> Result<bool, String> {
    for name in [
        "MERGE_HEAD",
        "CHERRY_PICK_HEAD",
        "REVERT_HEAD",
        "rebase-merge",
        "rebase-apply",
        "sequencer",
    ] {
        let path = git(dir, &["rev-parse", "--git-path", name], path_env)
            .map_err(|err| format!("无法检查 Git 操作状态：{err}"))?;
        if dir.join(path.strip_suffix('\n').unwrap_or(&path)).exists() {
            return Ok(true);
        }
    }
    Ok(false)
}

fn checkout_collision(
    dir: &Path,
    base: &str,
    tree: &str,
    path_env: Option<&OsStr>,
) -> Result<Option<String>, String> {
    let untracked = git(dir, &["ls-files", "--others", "-z", "--"], path_env)
        .map_err(|err| format!("无法检查未跟踪文件：{err}"))?;
    let untracked: BTreeSet<&str> = untracked
        .split_terminator('\0')
        .map(|path| path.trim_end_matches('/'))
        .collect();
    let added = diff(
        dir,
        base,
        tree,
        &["--name-only", "--diff-filter=A", "-z"],
        path_env,
    )?;
    for path in added.split_terminator('\0') {
        let mut ancestor = path;
        loop {
            if untracked.contains(ancestor) {
                return Ok(Some(path.to_owned()));
            }
            let Some((parent, _name)) = ancestor.rsplit_once('/') else {
                break;
            };
            ancestor = parent;
        }
        let prefix = format!("{path}/");
        if untracked
            .range(prefix.as_str()..)
            .next()
            .is_some_and(|local| local.starts_with(&prefix))
        {
            return Ok(Some(path.to_owned()));
        }
    }
    Ok(None)
}

/// 预检成功后以 --no-ff 合并；未检出的基线通过双亲提交与 update-ref CAS 推进。
pub fn merge_into_base(
    repo_dir: &Path,
    base_branch: &str,
    run_branch: &str,
    message: &str,
    path_env: Option<&OsStr>,
) -> Result<MergeOutcome, String> {
    let root = repo_root(repo_dir, path_env)?;
    if base_branch.is_empty() {
        return Ok(blocked(
            "无法确定基线分支，工作空间可能处于分离 HEAD 状态；请先选择基线分支",
        ));
    }
    if !valid_branch(&root, base_branch, path_env) || !valid_branch(&root, run_branch, path_env) {
        return Ok(blocked("基线或执行分支名称无效，无法合并"));
    }
    let base_ref = branch_ref(base_branch);
    let run_ref = branch_ref(run_branch);
    if base_ref == run_ref {
        return Ok(blocked("基线分支与执行分支不能相同"));
    }
    let base = match resolve_commit(&root, &base_ref, path_env) {
        Ok(commit) => commit,
        Err(err) => {
            return Ok(blocked(format!(
                "基线分支 {base_branch} 不存在或无法读取：{err}"
            )));
        }
    };
    let run = match resolve_commit(&root, &run_ref, path_env) {
        Ok(commit) => commit,
        Err(err) => {
            return Ok(blocked(format!(
                "执行分支 {run_branch} 不存在或无法读取：{err}"
            )));
        }
    };
    let tree = match preflight(&root, &base, &run, path_env)? {
        Ok(tree) => tree,
        Err(files) => return Ok(MergeOutcome::Conflict { files }),
    };
    let checkouts = checkout_for_branch(&root, &base_ref, path_env)?;
    if checkouts
        .iter()
        .any(|path| path.canonicalize().ok().as_ref() != Some(&root))
    {
        return Ok(blocked(format!(
            "基线分支 {base_branch} 已被其他 worktree 检出，请先释放该分支"
        )));
    }
    if checkouts.is_empty() {
        let commit = match with_identity(
            &root,
            &["commit-tree", &tree, "-p", &base, "-p", &run, "-m", message],
            path_env,
        ) {
            Ok(commit) => commit.trim().to_owned(),
            Err(err) => return Ok(blocked(format!("创建合并提交失败，基线保持不变：{err}"))),
        };
        if let Err(err) = git(
            &root,
            &["update-ref", "-m", message, &base_ref, &commit, &base],
            path_env,
        ) {
            return Ok(blocked(format!(
                "更新基线失败（可能已被其他操作推进），未覆盖基线：{err}"
            )));
        }
        return Ok(MergeOutcome::Merged { commit });
    }
    if operation_in_progress(&root, path_env)? {
        return Ok(blocked(
            "工作空间存在尚未完成的 Git 合并、变基或拣选，请先处理",
        ));
    }
    let status = git(
        &root,
        &[
            "status",
            "--porcelain=v1",
            "-z",
            "--untracked-files=no",
            "--ignore-submodules=none",
        ],
        path_env,
    )
    .map_err(|err| format!("无法检查基线工作目录：{err}"))?;
    if !status.is_empty() {
        return Ok(blocked(
            "基线工作目录存在已跟踪文件的未提交改动，请先提交或暂存到其他位置后再合并",
        ));
    }
    if let Some(path) = checkout_collision(&root, &base, &tree, path_env)? {
        return Ok(blocked(format!(
            "合并将覆盖未跟踪或被忽略的文件，请先移走该路径的本地内容：{path}"
        )));
    }
    let current_branch = git(&root, &["symbolic-ref", "--quiet", "HEAD"], path_env)
        .map_err(|err| format!("无法确认当前检出分支：{err}"))?;
    if current_branch.trim() != base_ref || resolve_commit(&root, "HEAD", path_env)? != base {
        return Ok(blocked("基线在预检后已发生变化，请重新验收后再合并"));
    }
    if let Err(err) = with_identity(
        &root,
        &[
            "-c",
            &format!("branch.{base_branch}.mergeOptions="),
            "-c",
            "rerere.enabled=false",
            "merge",
            "--no-ff",
            "--no-edit",
            "--commit",
            "--no-autostash",
            "--no-overwrite-ignore",
            "-m",
            message,
            &run,
        ],
        path_env,
    ) {
        let files = git(
            &root,
            &["diff", "--name-only", "--diff-filter=U", "-z", "--"],
            path_env,
        );
        let abort = git(&root, &["merge", "--abort"], path_env);
        if abort.is_err() && operation_in_progress(&root, path_env)? {
            return Ok(blocked(format!(
                "合并失败且无法自动中止，请人工处理：{err}"
            )));
        }
        let mut files: Vec<String> = files
            .unwrap_or_default()
            .split_terminator('\0')
            .map(str::to_owned)
            .collect();
        files.sort();
        files.dedup();
        return Ok(if files.is_empty() {
            blocked(format!("合并受阻，基线保持不变：{err}"))
        } else {
            MergeOutcome::Conflict { files }
        });
    }
    resolve_commit(&root, "HEAD", path_env).map(|commit| MergeOutcome::Merged { commit })
}

/// 删除执行 worktree 后安全删除分支；目录和分支已经不存在时可重复调用。
pub fn remove_run_worktree(
    repo_dir: &Path,
    worktree_root: &Path,
    branch: &str,
    path_env: Option<&OsStr>,
) -> Result<(), String> {
    let root = repo_root(repo_dir, path_env)?;
    if !valid_branch(&root, branch, path_env) {
        return Err("执行分支名称无效，无法清理".into());
    }
    if worktree_root.exists() {
        git(
            &root,
            &[
                "worktree",
                "remove",
                "--force",
                "--",
                &worktree_root.to_string_lossy(),
            ],
            path_env,
        )
        .map_err(|err| format!("删除执行 worktree 失败：{err}"))?;
    } else {
        git(&root, &["worktree", "prune"], path_env)
            .map_err(|err| format!("清理失效 worktree 记录失败：{err}"))?;
    }
    let refs = git(
        &root,
        &["for-each-ref", "--format=%(refname)", &branch_ref(branch)],
        path_env,
    )
    .map_err(|err| format!("检查执行分支失败：{err}"))?;
    if refs.lines().any(|name| name == branch_ref(branch)) {
        git(&root, &["branch", "-d", "--", branch], path_env)
            .map_err(|err| format!("安全删除执行分支失败（未合并的分支将保留）：{err}"))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::PathBuf;

    use super::*;
    use crate::run::worktree::{self, git};

    struct Repo {
        temp: tempfile::TempDir,
        root: PathBuf,
        base: String,
    }

    impl Repo {
        fn new() -> Self {
            let temp = tempfile::tempdir().unwrap();
            let root = temp.path().join("repo");
            fs::create_dir(&root).unwrap();
            checked(&root, &["init", "-q", "-b", "main"]);
            checked(&root, &["config", "user.name", "Test"]);
            checked(&root, &["config", "user.email", "test@localhost"]);
            checked(&root, &["config", "commit.gpgsign", "false"]);
            fs::write(root.join("file.txt"), "base\n").unwrap();
            fs::write(root.join("gone.txt"), "gone\n").unwrap();
            checked(&root, &["add", "."]);
            checked(&root, &["commit", "-q", "-m", "init"]);
            let base = checked(&root, &["rev-parse", "HEAD"]).trim().to_owned();
            Self { temp, root, base }
        }

        fn worktree(&self) -> worktree::Worktree {
            worktree::create(&self.root, &self.temp.path().join("run"), "run", None).unwrap()
        }

        fn merge(&self) -> MergeOutcome {
            merge_into_base(&self.root, "main", "run", "Accept run", None).unwrap()
        }

        fn main_head(&self) -> String {
            checked(&self.root, &["rev-parse", "main"])
                .trim()
                .to_owned()
        }
    }

    fn checked(dir: &Path, args: &[&str]) -> String {
        git(dir, args, None).unwrap_or_else(|err| panic!("git {args:?}: {err}"))
    }

    fn save(dir: &Path) -> String {
        commit_all(dir, "Save run", None).unwrap().unwrap()
    }

    fn merged(outcome: MergeOutcome) -> String {
        match outcome {
            MergeOutcome::Merged { commit } => commit,
            other => panic!("合并失败：{other:?}"),
        }
    }

    #[test]
    fn commit_all_returns_none_without_changes() {
        let repo = Repo::new();
        assert_eq!(commit_all(&repo.root, "Empty", None).unwrap(), None);
        assert_eq!(repo.main_head(), repo.base);
    }

    #[test]
    fn commit_all_includes_tracked_untracked_and_ignores_excluded_files() {
        let repo = Repo::new();
        fs::create_dir(repo.root.join("sub")).unwrap();
        fs::write(repo.root.join("file.txt"), "changed\n").unwrap();
        fs::remove_file(repo.root.join("gone.txt")).unwrap();
        fs::write(repo.root.join("sub/new.txt"), "new\n").unwrap();
        fs::write(repo.root.join(".gitignore"), "ignored.txt\n").unwrap();
        fs::write(repo.root.join("ignored.txt"), "secret\n").unwrap();
        checked(&repo.root, &["config", "--unset", "user.name"]);
        checked(&repo.root, &["config", "--unset", "user.email"]);
        let commit = save(&repo.root.join("sub"));
        assert_eq!(commit.len(), repo.base.len());
        assert_ne!(commit, repo.base);
        assert_eq!(
            checked(
                &repo.root,
                &["show", "-s", "--format=%an <%ae>|%cn <%ce>", &commit]
            )
            .trim(),
            "Teamspace <teamspace@localhost>|Teamspace <teamspace@localhost>"
        );
        assert_eq!(checked(&repo.root, &["status", "--porcelain"]), "");
        assert!(
            !checked(&repo.root, &["ls-tree", "-r", "--name-only", "HEAD"]).contains("ignored.txt")
        );
        assert_eq!(commit_all(&repo.root, "Empty", None).unwrap(), None);
    }

    #[test]
    fn commit_all_supports_unborn_repository() {
        let temp = tempfile::tempdir().unwrap();
        checked(temp.path(), &["init", "-q", "-b", "main"]);
        fs::write(temp.path().join("first"), "first\n").unwrap();
        assert!(!save(temp.path()).is_empty());
    }

    #[test]
    fn diff_files_reports_sorted_counts_and_binary() {
        let repo = Repo::new();
        fs::write(repo.root.join("file.txt"), "one\ntwo\n").unwrap();
        fs::remove_file(repo.root.join("gone.txt")).unwrap();
        fs::write(repo.root.join("added.txt"), "new\n").unwrap();
        fs::write(repo.root.join("binary.bin"), [0, 1, 2, 3]).unwrap();
        let head = save(&repo.root);
        assert_eq!(
            diff_files(&repo.root, &repo.base, &head, None).unwrap(),
            vec![
                FileDiff {
                    path: "added.txt".into(),
                    change: FileChangeKind::Added,
                    binary: false,
                    added: 1,
                    removed: 0
                },
                FileDiff {
                    path: "binary.bin".into(),
                    change: FileChangeKind::Added,
                    binary: true,
                    added: 0,
                    removed: 0
                },
                FileDiff {
                    path: "file.txt".into(),
                    change: FileChangeKind::Modified,
                    binary: false,
                    added: 2,
                    removed: 1
                },
                FileDiff {
                    path: "gone.txt".into(),
                    change: FileChangeKind::Deleted,
                    binary: false,
                    added: 0,
                    removed: 1
                },
            ]
        );
        assert!(
            diff_files(&repo.root, &head, &head, None)
                .unwrap()
                .is_empty()
        );
    }

    #[test]
    fn rename_is_a_deletion_and_addition() {
        let repo = Repo::new();
        fs::rename(repo.root.join("gone.txt"), repo.root.join("renamed.txt")).unwrap();
        let head = save(&repo.root);
        let files = diff_files(&repo.root, &repo.base, &head, None).unwrap();
        assert_eq!(files.len(), 2);
        assert_eq!(files[0].change, FileChangeKind::Deleted);
        assert_eq!(files[1].change, FileChangeKind::Added);
    }

    #[test]
    fn file_patch_returns_unified_diff_for_only_requested_file() {
        let repo = Repo::new();
        fs::write(repo.root.join("file.txt"), "changed\n").unwrap();
        fs::write(repo.root.join("other.txt"), "other\n").unwrap();
        fs::remove_file(repo.root.join("gone.txt")).unwrap();
        let head = save(&repo.root);
        let patch = file_patch(&repo.root, &repo.base, &head, "file.txt", None).unwrap();
        assert!(!patch.binary && !patch.truncated);
        assert!(patch.patch.contains("@@") && patch.patch.contains("+changed\n"));
        assert!(!patch.patch.contains("other.txt"));
        let added = file_patch(&repo.root, &repo.base, &head, "other.txt", None).unwrap();
        assert!(added.patch.contains("/dev/null"));
        let deleted = file_patch(&repo.root, &repo.base, &head, "gone.txt", None).unwrap();
        assert!(deleted.patch.contains("+++ /dev/null") && deleted.patch.contains("-gone\n"));
    }

    #[test]
    fn file_patch_returns_empty_patch_for_binary() {
        let repo = Repo::new();
        fs::write(repo.root.join("binary"), [0, 1]).unwrap();
        let head = save(&repo.root);
        let patch = file_patch(&repo.root, &repo.base, &head, "binary", None).unwrap();
        assert!(patch.binary && !patch.truncated && patch.patch.is_empty());
    }

    #[test]
    fn file_patch_truncates_at_line_boundary() {
        let repo = Repo::new();
        fs::write(repo.root.join("large"), "很长的一行内容\n".repeat(30000)).unwrap();
        let head = save(&repo.root);
        let patch = file_patch(&repo.root, &repo.base, &head, "large", None).unwrap();
        assert!(patch.truncated && !patch.binary);
        assert!(patch.patch.len() <= MAX_PATCH_BYTES);
        assert!(patch.patch.ends_with('\n'));
        assert!(!patch.patch.contains('\u{fffd}'));
    }

    #[test]
    fn file_patch_rejects_paths_not_in_diff() {
        let repo = Repo::new();
        fs::write(repo.root.join("file.txt"), "changed\n").unwrap();
        let head = save(&repo.root);
        for path in ["gone.txt", "../outside", "/etc/passwd", ":(glob)*", ""] {
            assert!(
                file_patch(&repo.root, &repo.base, &head, path, None).is_err(),
                "{path}"
            );
        }
    }

    #[test]
    fn diff_paths_are_nul_delimited_and_patch_pathspec_is_literal() {
        let repo = Repo::new();
        let path = "特别\t[abc]*\n.txt";
        fs::write(repo.root.join(path), "literal\n").unwrap();
        fs::write(repo.root.join("another.txt"), "other\n").unwrap();
        fs::create_dir(repo.root.join("sub")).unwrap();
        let head = save(&repo.root);
        let files = diff_files(&repo.root.join("sub"), &repo.base, &head, None).unwrap();
        assert!(files.iter().any(|file| file.path == path));
        let patch = file_patch(&repo.root.join("sub"), &repo.base, &head, path, None).unwrap();
        assert!(patch.patch.contains("+literal"));
        assert!(!patch.patch.contains("+other"));
    }

    #[test]
    fn diff_rejects_invalid_revisions() {
        let repo = Repo::new();
        for revision in ["--output=/tmp/not-allowed", "missing"] {
            assert!(diff_files(&repo.root, revision, "HEAD", None).is_err());
            assert!(file_patch(&repo.root, "HEAD", revision, "file.txt", None).is_err());
        }
    }

    #[test]
    fn merge_checked_out_base_creates_two_parent_commit() {
        let repo = Repo::new();
        let run = repo.worktree();
        fs::write(run.root.join("new.txt"), "new\n").unwrap();
        let head = save(&run.root);
        checked(&repo.root, &["config", "--unset", "user.name"]);
        checked(&repo.root, &["config", "--unset", "user.email"]);
        let commit = merged(repo.merge());
        assert_eq!(repo.main_head(), commit);
        assert_eq!(
            checked(&repo.root, &["show", "-s", "--format=%P", &commit]).trim(),
            format!("{} {head}", repo.base)
        );
        assert_eq!(
            fs::read_to_string(repo.root.join("new.txt")).unwrap(),
            "new\n"
        );
        assert_eq!(checked(&repo.root, &["status", "--porcelain"]), "");
        assert_eq!(
            checked(&repo.root, &["show", "-s", "--format=%cn <%ce>", &commit]).trim(),
            "Teamspace <teamspace@localhost>"
        );
    }

    #[test]
    fn dirty_checked_out_base_is_blocked_and_unchanged() {
        for staged in [false, true] {
            let repo = Repo::new();
            let run = repo.worktree();
            fs::write(run.root.join("new.txt"), "new\n").unwrap();
            save(&run.root);
            fs::write(repo.root.join("file.txt"), "local\n").unwrap();
            if staged {
                checked(&repo.root, &["add", "file.txt"]);
            }
            let status = checked(&repo.root, &["status", "--porcelain=v1", "-z"]);
            assert!(
                matches!(repo.merge(), MergeOutcome::Blocked { reason } if reason.contains("已跟踪"))
            );
            assert_eq!(repo.main_head(), repo.base);
            assert_eq!(
                checked(&repo.root, &["status", "--porcelain=v1", "-z"]),
                status
            );
            assert_eq!(
                fs::read_to_string(repo.root.join("file.txt")).unwrap(),
                "local\n"
            );
        }
    }

    #[test]
    fn conflict_preserves_refs_index_and_working_directories() {
        let repo = Repo::new();
        let run = repo.worktree();
        for path in ["file.txt", "gone.txt"] {
            fs::write(run.root.join(path), "run\n").unwrap();
        }
        let head = save(&run.root);
        for path in ["file.txt", "gone.txt"] {
            fs::write(repo.root.join(path), "main\n").unwrap();
        }
        let base = save(&repo.root);
        assert_eq!(
            repo.merge(),
            MergeOutcome::Conflict {
                files: vec!["file.txt".into(), "gone.txt".into()]
            }
        );
        assert_eq!(repo.main_head(), base);
        assert_eq!(checked(&run.root, &["rev-parse", "HEAD"]).trim(), head);
        assert_eq!(checked(&repo.root, &["status", "--porcelain"]), "");
        assert_eq!(checked(&run.root, &["status", "--porcelain"]), "");
        assert_eq!(
            fs::read_to_string(repo.root.join("file.txt")).unwrap(),
            "main\n"
        );
        assert_eq!(
            fs::read_to_string(run.root.join("file.txt")).unwrap(),
            "run\n"
        );
        assert!(git(&repo.root, &["rev-parse", "--verify", "MERGE_HEAD"], None).is_err());
    }

    #[test]
    fn merge_accepts_advanced_nonconflicting_base() {
        let repo = Repo::new();
        let run = repo.worktree();
        fs::write(run.root.join("run.txt"), "run\n").unwrap();
        let head = save(&run.root);
        fs::write(repo.root.join("base.txt"), "base\n").unwrap();
        let base = save(&repo.root);
        let commit = merged(repo.merge());
        assert_eq!(
            checked(&repo.root, &["show", "-s", "--format=%P", &commit]).trim(),
            format!("{base} {head}")
        );
        assert!(repo.root.join("run.txt").exists() && repo.root.join("base.txt").exists());
    }

    #[test]
    fn configured_merge_options_do_not_change_merge_semantics() {
        let repo = Repo::new();
        let run = repo.worktree();
        fs::write(run.root.join("run.txt"), "run\n").unwrap();
        save(&run.root);
        checked(
            &repo.root,
            &["config", "branch.main.mergeOptions", "--no-commit -s ours"],
        );
        let commit = merged(repo.merge());
        assert_ne!(commit, repo.base);
        assert_eq!(repo.main_head(), commit);
        assert_eq!(
            fs::read_to_string(repo.root.join("run.txt")).unwrap(),
            "run\n"
        );
        assert!(!operation_in_progress(&repo.root, None).unwrap());
        assert_eq!(checked(&repo.root, &["status", "--porcelain"]), "");
    }

    #[test]
    fn unexpected_commit_failure_aborts_and_restores_checkout() {
        let repo = Repo::new();
        let run = repo.worktree();
        fs::write(run.root.join("run.txt"), "run\n").unwrap();
        save(&run.root);
        let outcome = merge_into_base(&repo.root, "main", "run", "", None).unwrap();
        assert!(
            matches!(outcome, MergeOutcome::Blocked { .. }),
            "{outcome:?}"
        );
        assert_eq!(repo.main_head(), repo.base);
        assert!(!repo.root.join("run.txt").exists());
        assert!(!operation_in_progress(&repo.root, None).unwrap());
        assert_eq!(checked(&repo.root, &["status", "--porcelain"]), "");
    }

    #[test]
    fn merge_preserves_unrelated_untracked_files() {
        let repo = Repo::new();
        let run = repo.worktree();
        fs::write(run.root.join("run.txt"), "run\n").unwrap();
        save(&run.root);
        fs::write(repo.root.join("local.txt"), "local\n").unwrap();
        merged(repo.merge());
        assert_eq!(
            fs::read_to_string(repo.root.join("local.txt")).unwrap(),
            "local\n"
        );
        assert_eq!(
            checked(&repo.root, &["status", "--porcelain"]).trim(),
            "?? local.txt"
        );
    }

    #[test]
    fn merge_does_not_overwrite_ignored_untracked_files() {
        let repo = Repo::new();
        let run = repo.worktree();
        fs::write(run.root.join("collision"), "run\n").unwrap();
        save(&run.root);
        fs::write(repo.root.join(".git/info/exclude"), "collision\n").unwrap();
        fs::write(repo.root.join("collision"), "local\n").unwrap();
        assert!(matches!(repo.merge(), MergeOutcome::Blocked { .. }));
        assert_eq!(repo.main_head(), repo.base);
        assert_eq!(
            fs::read_to_string(repo.root.join("collision")).unwrap(),
            "local\n"
        );
    }

    #[test]
    fn unchecked_conflicting_base_does_not_move_refs_or_checkout() {
        let repo = Repo::new();
        let run = repo.worktree();
        fs::write(run.root.join("file.txt"), "run\n").unwrap();
        save(&run.root);
        fs::write(repo.root.join("file.txt"), "main\n").unwrap();
        let base = save(&repo.root);
        checked(&repo.root, &["checkout", "-q", "-b", "other"]);
        fs::write(repo.root.join("file.txt"), "local\n").unwrap();
        assert_eq!(
            repo.merge(),
            MergeOutcome::Conflict {
                files: vec!["file.txt".into()]
            }
        );
        assert_eq!(repo.main_head(), base);
        assert_eq!(
            fs::read_to_string(repo.root.join("file.txt")).unwrap(),
            "local\n"
        );
    }

    #[test]
    fn detached_checkout_requires_known_base_and_is_not_modified() {
        let repo = Repo::new();
        let run = repo.worktree();
        fs::write(run.root.join("run.txt"), "run\n").unwrap();
        save(&run.root);
        checked(&repo.root, &["checkout", "--detach", "-q"]);
        assert!(matches!(
            merge_into_base(&repo.root, "", "run", "Accept", None).unwrap(),
            MergeOutcome::Blocked { .. }
        ));
        assert_eq!(repo.main_head(), repo.base);
        merged(repo.merge());
        assert_eq!(
            checked(&repo.root, &["rev-parse", "HEAD"]).trim(),
            repo.base
        );
        assert!(!repo.root.join("run.txt").exists());
    }

    #[test]
    fn unchecked_base_is_merged_without_touching_any_checkout() {
        let repo = Repo::new();
        let run = repo.worktree();
        fs::write(run.root.join("run.txt"), "run\n").unwrap();
        let head = save(&run.root);
        checked(&repo.root, &["checkout", "-q", "-b", "other"]);
        fs::write(repo.root.join("file.txt"), "local\n").unwrap();
        let status = checked(&repo.root, &["status", "--porcelain=v1", "-z"]);
        let commit = merged(repo.merge());
        assert_eq!(repo.main_head(), commit);
        assert_eq!(
            checked(&repo.root, &["show", "-s", "--format=%P", &commit]).trim(),
            format!("{} {head}", repo.base)
        );
        assert_eq!(
            checked(&repo.root, &["rev-parse", "HEAD"]).trim(),
            repo.base
        );
        assert_eq!(
            checked(&repo.root, &["status", "--porcelain=v1", "-z"]),
            status
        );
        assert!(!repo.root.join("run.txt").exists());
        assert_eq!(
            fs::read_to_string(repo.root.join("file.txt")).unwrap(),
            "local\n"
        );
        assert_eq!(checked(&run.root, &["status", "--porcelain"]), "");
    }

    #[test]
    fn merge_missing_or_unknown_branch_is_blocked() {
        let repo = Repo::new();
        repo.worktree();
        for (base, run) in [
            ("missing", "run"),
            ("main", "missing"),
            ("", "run"),
            ("--bad", "run"),
        ] {
            assert!(matches!(
                merge_into_base(&repo.root, base, run, "Accept", None).unwrap(),
                MergeOutcome::Blocked { .. }
            ));
            assert_eq!(repo.main_head(), repo.base);
        }
    }

    #[test]
    fn base_checked_out_elsewhere_is_blocked() {
        let repo = Repo::new();
        repo.worktree();
        checked(&repo.root, &["checkout", "-q", "-b", "other"]);
        let other = repo.temp.path().join("base checkout");
        checked(
            &repo.root,
            &["worktree", "add", other.to_str().unwrap(), "main"],
        );
        assert!(
            matches!(repo.merge(), MergeOutcome::Blocked { reason } if reason.contains("其他 worktree"))
        );
        assert_eq!(repo.main_head(), repo.base);
        assert_eq!(checked(&other, &["status", "--porcelain"]), "");
    }

    #[test]
    fn untracked_collision_aborts_without_advancing_base() {
        let repo = Repo::new();
        let run = repo.worktree();
        fs::write(run.root.join("collision"), "run\n").unwrap();
        save(&run.root);
        fs::write(repo.root.join("collision"), "local\n").unwrap();
        assert!(matches!(repo.merge(), MergeOutcome::Blocked { .. }));
        assert_eq!(repo.main_head(), repo.base);
        assert_eq!(
            fs::read_to_string(repo.root.join("collision")).unwrap(),
            "local\n"
        );
        assert!(git(&repo.root, &["rev-parse", "--verify", "MERGE_HEAD"], None).is_err());
    }

    #[test]
    fn update_ref_failure_preserves_base_and_checkout() {
        let repo = Repo::new();
        let run = repo.worktree();
        fs::write(run.root.join("run.txt"), "run\n").unwrap();
        save(&run.root);
        checked(&repo.root, &["checkout", "-q", "-b", "other"]);
        fs::write(repo.root.join(".git/refs/heads/main.lock"), "locked").unwrap();
        assert!(matches!(repo.merge(), MergeOutcome::Blocked { .. }));
        assert_eq!(repo.main_head(), repo.base);
        assert!(!repo.root.join("run.txt").exists());
        assert_eq!(checked(&repo.root, &["status", "--porcelain"]), "");
    }

    #[test]
    fn existing_merge_is_not_aborted_or_overwritten() {
        let repo = Repo::new();
        let run = repo.worktree();
        fs::write(run.root.join("new.txt"), "new\n").unwrap();
        save(&run.root);
        checked(&repo.root, &["merge", "--no-ff", "--no-commit", "run"]);
        let merge_head = checked(&repo.root, &["rev-parse", "MERGE_HEAD"]);
        assert!(matches!(repo.merge(), MergeOutcome::Blocked { .. }));
        assert_eq!(repo.main_head(), repo.base);
        assert_eq!(
            checked(&repo.root, &["rev-parse", "MERGE_HEAD"]),
            merge_head
        );
    }

    #[test]
    fn cleanup_removes_worktree_and_branch_and_is_repeatable() {
        let repo = Repo::new();
        let run = repo.worktree();
        fs::write(run.root.join("new.txt"), "new\n").unwrap();
        save(&run.root);
        merged(repo.merge());
        for _attempt in 0..2 {
            remove_run_worktree(&repo.root, &run.root, "run", None).unwrap();
            assert!(!run.root.exists());
            assert!(
                git(
                    &repo.root,
                    &["show-ref", "--verify", "refs/heads/run"],
                    None
                )
                .is_err()
            );
        }
    }

    #[test]
    fn cleanup_prunes_missing_directory_before_deleting_branch() {
        let repo = Repo::new();
        let run = repo.worktree();
        fs::remove_dir_all(&run.root).unwrap();
        remove_run_worktree(&repo.root, &run.root, "run", None).unwrap();
        assert!(
            !checked(&repo.root, &["worktree", "list", "--porcelain"]).contains("refs/heads/run")
        );
    }

    #[test]
    fn cleanup_keeps_unmerged_branch() {
        let repo = Repo::new();
        let run = repo.worktree();
        fs::write(run.root.join("new.txt"), "new\n").unwrap();
        let head = save(&run.root);
        assert!(remove_run_worktree(&repo.root, &run.root, "run", None).is_err());
        assert_eq!(checked(&repo.root, &["rev-parse", "run"]).trim(), head);
        assert_eq!(repo.main_head(), repo.base);
    }

    #[test]
    fn supplied_path_env_is_used() {
        let repo = Repo::new();
        let empty = repo.temp.path().join("empty-path");
        fs::create_dir(&empty).unwrap();
        assert!(diff_files(&repo.root, &repo.base, "HEAD", Some(empty.as_os_str())).is_err());
    }
}
