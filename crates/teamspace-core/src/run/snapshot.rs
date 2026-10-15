//! 执行目录快照（S-1b / G-6）：不依赖 Agent 自觉的改动统计兜底。
//!
//! 真实 Agent 可能用 shell 直接写文件，既不走 `fs/write_text_file` 也不发权限请求，ACP 报告的改动
//! 因此漏掉这些文件。普通文件夹原地执行时，执行开始前对执行目录拍一份快照（相对路径、大小、修改时间、
//! 小文件的内容摘要），结束（或中断）时再拍一份比对，得出真实的新增、修改、删除（[`diff`]）。
//! worktree 执行直接用 Git 统计（[`super::worktree::changed_files`]），不用快照。
//!
//! 范围与上限：
//! - 隐藏文件和目录也检查（`.env` 这类文件恰恰最不该被悄悄改写），只跳过 `.git`、Finder 的 `.DS_Store`
//!   与 `._*`，以及 [`SKIP_DIRS`] 里的依赖与构建目录（`node_modules`、`target`、`dist`、`build`、
//!   `__pycache__`）和符号链接。这些位置里的写入既不计入改动也不做范围检查，界面据此写明
//!   「未检查 .git、node_modules、target、dist、build 等目录」；
//! - 最多记录 [`MAX_ENTRIES`] 个文件与目录。超过上限时快照降级：先扫「优先目录」（本岗位文件夹、任务引用
//!   所在的目录），再按路径顺序扫其余部分直到用完名额；[`Snapshot::truncated`] 为真。比对时只看两份快照
//!   都覆盖到的路径（[`Snapshot::covers`]），并在 [`Diff::note`] 里说明「改动统计不完整」，绝不把没扫到
//!   的文件误报成新增或删除；
//! - 判定文件被改动：大小、修改时间、内容摘要任一不同。摘要只对不超过 [`FILE_DIGEST_LIMIT`] 的文件记录
//!   （总量受 [`DIGEST_BUDGET`] 限制）；执行结束时先比大小和修改时间，两者都没变才读文件内容比摘要，
//!   这样 `cp -p`、`tar x`、`touch -r` 之类保留修改时间的同大小写入也能发现；
//! - 工作日志（`Teamspace/<岗位>/工作日志.md`）：Teamspace 与 Agent 都会向它追加（D14），纯追加不算改动，
//!   被改写或截断才算。本岗位的岗位说明、日志、`过程文件/README.md` 由 Teamspace 维护，不计入；
//!   其他岗位的同名文件（日志的纯追加除外）按普通文件处理。
//!
//! 快照可以存到应用数据目录（[`save`] / [`load`]），应用重启后仍能据此算出被中断的执行改了哪些文件。

use std::cmp::Ordering;
use std::collections::BTreeMap;
use std::path::Path;
use std::time::UNIX_EPOCH;

use serde::{Deserialize, Serialize};

use crate::model::{ChangedFile, FileChangeKind};
use crate::role_folder::{BRIEF_FILE, JOURNAL_FILE, PROCESS_DIR, README_FILE, ROOT_DIR};

/// 一份快照最多记录的文件与目录数（性能上限）。
pub const MAX_ENTRIES: usize = 20_000;
/// 不进入的目录（依赖与构建产物，体量大且不是任务产出）。`.git` 另行跳过。
pub const SKIP_DIRS: &[&str] = &["node_modules", "target", "__pycache__", "dist", "build"];
/// 目录深度上限，防止病态的深层嵌套。
const MAX_DEPTH: usize = 64;
/// 记内容摘要的单个文件大小上限。
pub const FILE_DIGEST_LIMIT: u64 = 4 * 1024 * 1024;
/// 一份快照里内容摘要读取的总字节上限；用完后其余文件只按大小和修改时间判断。
pub const DIGEST_BUDGET: u64 = 256 * 1024 * 1024;
/// 工作日志记摘要的大小上限（日志要判断是不是纯追加，所以比普通文件宽松）。
const JOURNAL_DIGEST_LIMIT: u64 = 16 * 1024 * 1024;

/// 一个文件的快照。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Entry {
    pub size: u64,
    /// 修改时间（自 UNIX 纪元起的纳秒）；取不到为 0。
    pub mtime: u64,
    /// 内容摘要（FNV-1a）；太大、超出总量上限或没有读取时为 `None`。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub digest: Option<u64>,
}

/// 目录快照。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Snapshot {
    /// 相对根目录的路径（`/` 分隔）→ 文件信息。
    pub files: BTreeMap<String, Entry>,
    /// 超过 `max_entries` 上限而降级。
    pub truncated: bool,
    /// 降级时，按路径顺序扫描的那一部分里最后扫到的路径（之后的都没有扫描）。
    pub last: Option<String>,
    /// 扫描时使用的上限。
    pub max_entries: usize,
    /// 优先扫描的相对路径（目录或文件）：降级时它们仍然完整覆盖。
    #[serde(default)]
    pub priority: Vec<String>,
    /// 已完整扫描的优先路径。
    #[serde(default)]
    pub covered: Vec<String>,
    /// 附加说明（如续接时「中断到续接之间另有文件被改动」），会并入 [`Diff::note`]。
    #[serde(default)]
    pub notes: Vec<String>,
}

impl Snapshot {
    /// 这份快照是否覆盖了 `path`：完整的快照覆盖一切；降级的只覆盖完整扫描过的优先路径和
    /// 按路径顺序靠前的部分。
    pub fn covers(&self, path: &str) -> bool {
        if !self.truncated {
            return true;
        }
        if self.covered.iter().any(|c| within(path, c)) {
            return true;
        }
        self.last
            .as_deref()
            .is_some_and(|last| cmp_paths(path, last) != Ordering::Greater)
    }
}

/// 两份快照的比对结果。
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Diff {
    /// 新增、修改、删除的文件，按路径排序（不含 Teamspace 自己维护的文件与纯追加的工作日志）。
    pub changes: Vec<ChangedFile>,
    /// 统计说明（中文）：快照降级、续接间隙等；没有时为 `None`。
    pub note: Option<String>,
    /// 改动统计不完整（快照降级）。
    pub incomplete: bool,
}

/// 对 `root` 拍快照（记录小文件的内容摘要），最多记录 `max_entries` 个文件与目录。
/// `priority` 是降级时优先完整扫描的相对路径。读取失败的目录或文件被跳过。
pub fn take(root: &Path, max_entries: usize, priority: &[String]) -> Snapshot {
    scan(root, max_entries, priority, true)
}

/// 同 [`take`]，但不读文件内容（执行结束时的快照用它；需要时 [`diff`] 才读未改动文件的内容）。
pub fn take_light(root: &Path, max_entries: usize, priority: &[String]) -> Snapshot {
    scan(root, max_entries, priority, false)
}

struct Scan {
    visited: usize,
    hashed: u64,
    hash: bool,
    track_last: bool,
    skip: Vec<String>,
}

fn scan(root: &Path, max_entries: usize, priority: &[String], hash: bool) -> Snapshot {
    let mut snap = Snapshot {
        files: BTreeMap::new(),
        truncated: false,
        last: None,
        max_entries,
        priority: Vec::new(),
        covered: Vec::new(),
        notes: Vec::new(),
    };
    let mut ctx = Scan {
        visited: 0,
        hashed: 0,
        hash,
        track_last: false,
        skip: Vec::new(),
    };
    // 优先路径：去重，并去掉被别的优先路径包含的。
    let mut wanted: Vec<String> = priority
        .iter()
        .map(|p| components(p).collect::<Vec<_>>().join("/"))
        .filter(|p| !p.is_empty())
        .collect();
    wanted.sort_by(|a, b| cmp_paths(a, b));
    wanted.dedup();
    let mut prios: Vec<String> = Vec::new();
    for p in wanted {
        if !prios.iter().any(|q| within(&p, q)) {
            prios.push(p);
        }
    }
    snap.priority = prios.clone();
    for pr in &prios {
        let full = root.join(pr);
        let complete = match std::fs::symlink_metadata(&full) {
            Ok(meta) if meta.is_dir() => walk(&full, pr, 0, &mut ctx, &mut snap),
            Ok(meta) if meta.is_file() => record_file(&full, pr, &meta, &mut ctx, &mut snap),
            _ => true,
        };
        if !complete {
            snap.truncated = true;
            return snap;
        }
        snap.covered.push(pr.clone());
    }
    ctx.skip = prios;
    ctx.track_last = true;
    if !walk(root, "", 0, &mut ctx, &mut snap) {
        snap.truncated = true;
    }
    snap
}

/// 记录一个文件；名额用完返回 `false`。
fn record_file(
    path: &Path,
    rel: &str,
    meta: &std::fs::Metadata,
    ctx: &mut Scan,
    snap: &mut Snapshot,
) -> bool {
    if ctx.visited >= snap.max_entries {
        return false;
    }
    ctx.visited += 1;
    if ctx.track_last {
        snap.last = Some(rel.to_owned());
    }
    let mut size = meta.len();
    let mut digest = None;
    if ctx.hash {
        let journal = is_journal(rel);
        let wanted = if journal {
            size <= JOURNAL_DIGEST_LIMIT
        } else {
            size <= FILE_DIGEST_LIMIT && ctx.hashed + size <= DIGEST_BUDGET
        };
        if wanted && let Ok(bytes) = std::fs::read(path) {
            let read = bytes.len() as u64;
            // 读的时候文件正被写（Teamspace 追加工作日志）：以读到的内容为准，让大小和摘要对得上。
            if journal {
                size = read;
            }
            if read == size {
                ctx.hashed += read;
                digest = Some(fnv1a(&bytes));
            }
        }
    }
    snap.files.insert(
        rel.to_owned(),
        Entry {
            size,
            mtime: mtime_of(meta),
            digest,
        },
    );
    true
}

fn mtime_of(meta: &std::fs::Metadata) -> u64 {
    meta.modified()
        .ok()
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map_or(0, |d| d.as_nanos() as u64)
}

/// 不检查的文件名：`.git`、Finder 的 `.DS_Store`、AppleDouble 的 `._*`。
fn skipped_name(name: &str) -> bool {
    name == ".git" || name == ".DS_Store" || name.starts_with("._")
}

/// 递归扫描 `dir`（相对路径 `rel`，空串为根）。返回 `false` 表示已达上限、应当停止整个扫描。
fn walk(dir: &Path, rel: &str, depth: usize, ctx: &mut Scan, snap: &mut Snapshot) -> bool {
    if depth >= MAX_DEPTH {
        return true;
    }
    let Ok(read) = std::fs::read_dir(dir) else {
        return true;
    };
    let mut children: Vec<(String, std::fs::DirEntry)> = read
        .filter_map(|e| e.ok())
        .map(|e| (e.file_name().to_string_lossy().into_owned(), e))
        .collect();
    children.sort_by(|a, b| a.0.cmp(&b.0));
    for (name, entry) in children {
        if skipped_name(&name) {
            continue;
        }
        let Ok(kind) = entry.file_type() else {
            continue;
        };
        if kind.is_symlink() {
            continue;
        }
        let is_dir = kind.is_dir();
        if is_dir && SKIP_DIRS.contains(&name.as_str()) {
            continue;
        }
        if !is_dir && !kind.is_file() {
            continue;
        }
        let path = if rel.is_empty() {
            name.clone()
        } else {
            format!("{rel}/{name}")
        };
        // 已经作为优先路径完整扫描过的子树不再重复扫描。
        if ctx.skip.contains(&path) {
            continue;
        }
        if is_dir {
            if ctx.visited >= snap.max_entries {
                return false;
            }
            ctx.visited += 1;
            if ctx.track_last {
                snap.last = Some(path.clone());
            }
            if !walk(&entry.path(), &path, depth + 1, ctx, snap) {
                return false;
            }
        } else if let Ok(meta) = entry.metadata()
            && !record_file(&entry.path(), &path, &meta, ctx, snap)
        {
            return false;
        }
    }
    true
}

/// FNV-1a 64 位摘要（稳定、无依赖；只用来判断内容有没有变，不涉及安全）。
fn fnv1a(bytes: &[u8]) -> u64 {
    let mut hash = 0xcbf2_9ce4_8422_2325u64;
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}

fn components(path: &str) -> impl Iterator<Item = &str> {
    path.split('/').filter(|c| !c.is_empty())
}

/// 按目录遍历顺序（逐段字典序，即扫描顺序）比较两个相对路径。
fn cmp_paths(a: &str, b: &str) -> Ordering {
    components(a).cmp(components(b))
}

/// `path` 是否等于 `prefix` 或在它之下。
fn within(path: &str, prefix: &str) -> bool {
    let mut p = components(path);
    components(prefix).all(|c| p.next() == Some(c))
}

/// `Teamspace/<岗位文件夹>/工作日志.md`（任一岗位）。
fn is_journal(rel: &str) -> bool {
    let parts: Vec<&str> = components(rel).collect();
    matches!(parts.as_slice(), [root, _, file] if *root == ROOT_DIR && *file == JOURNAL_FILE)
}

/// Teamspace 自己维护的、不算 Agent 改动的文件：`Teamspace/README.md`，以及**本岗位**
/// （`own_role`，如 `Teamspace/前端`）的岗位说明和 `过程文件/README.md`。其他岗位的同名文件是普通文件。
fn is_teamspace_owned(rel: &str, own_role: Option<&str>) -> bool {
    let parts: Vec<&str> = components(rel).collect();
    if matches!(parts.as_slice(), [root, file] if *root == ROOT_DIR && *file == README_FILE) {
        return true;
    }
    let Some(own) = own_role else {
        return false;
    };
    let own: Vec<&str> = components(own).collect();
    match parts.as_slice() {
        [root, role, file] => *file == BRIEF_FILE && own == [*root, *role],
        [root, role, dir, file] => {
            *dir == PROCESS_DIR && *file == README_FILE && own == [*root, *role]
        }
        _ => false,
    }
}

/// 是不是本岗位的工作日志。
fn is_own_journal(rel: &str, own_role: Option<&str>) -> bool {
    let Some(own) = own_role else {
        return false;
    };
    let parts: Vec<&str> = components(rel).collect();
    let own: Vec<&str> = components(own).collect();
    matches!(parts.as_slice(), [root, role, file]
        if *file == JOURNAL_FILE && own == [*root, *role])
}

/// 工作日志相对快照里的记录是否只是追加：现在的内容以当时的内容为开头。
fn journal_only_appended(root: &Path, rel: &str, before: &Entry, after: &Entry) -> bool {
    if after.size < before.size {
        return false;
    }
    let Some(digest) = before.digest else {
        // 当时太大没有记摘要：只能按大小判断。
        return true;
    };
    let Ok(bytes) = std::fs::read(root.join(rel)) else {
        return false;
    };
    usize::try_from(before.size)
        .ok()
        .and_then(|len| bytes.get(..len))
        .is_some_and(|prefix| fnv1a(prefix) == digest)
}

/// 普通文件内容是否变了：大小或修改时间不同直接算变；两者都没变时，有摘要才读内容比对。
fn content_changed(root: &Path, rel: &str, old: &Entry, new: &Entry) -> bool {
    if old.size != new.size || old.mtime != new.mtime {
        return true;
    }
    let Some(old_digest) = old.digest else {
        return false;
    };
    let new_digest = new.digest.or_else(|| {
        std::fs::read(root.join(rel))
            .ok()
            .map(|bytes| fnv1a(&bytes))
    });
    new_digest.is_some_and(|d| d != old_digest)
}

/// 降级时的说明。
fn incomplete_note(max_entries: usize) -> String {
    format!(
        "改动统计不完整：执行目录里的文件太多（超过 {max_entries} 个），快照先检查了本岗位文件夹和任务引用所在的\
         目录，其余位置只检查了按路径顺序靠前的一部分；没检查到的位置里的改动没有统计，也没有核对是否越出权限范围"
    )
}

/// 比对 `before`（执行前）与 `after`（执行后），`root` 用来核对文件内容。`own_role` 是本岗位文件夹的
/// 相对路径（如 `Teamspace/前端`），只有它下面 Teamspace 维护的文件被排除。
pub fn diff(root: &Path, before: &Snapshot, after: &Snapshot, own_role: Option<&str>) -> Diff {
    let incomplete = before.truncated || after.truncated;
    let mut notes: Vec<String> = Vec::new();
    if incomplete {
        notes.push(incomplete_note(before.max_entries.min(after.max_entries)));
    }
    notes.extend(before.notes.iter().cloned());
    let note = (!notes.is_empty()).then(|| notes.join("；"));
    let covered = |path: &str| before.covers(path) && after.covers(path);

    let mut changes = Vec::new();
    for (path, old) in &before.files {
        if !covered(path) || is_teamspace_owned(path, own_role) {
            continue;
        }
        match after.files.get(path) {
            None => changes.push(ChangedFile {
                path: path.clone(),
                change: FileChangeKind::Deleted,
            }),
            Some(new) => {
                let changed = if is_journal(path) {
                    !journal_only_appended(root, path, old, new)
                } else {
                    content_changed(root, path, old, new)
                };
                if changed {
                    changes.push(ChangedFile {
                        path: path.clone(),
                        change: FileChangeKind::Modified,
                    });
                }
            }
        }
    }
    for path in after.files.keys() {
        // 新出现的本岗位工作日志是 Teamspace 建岗位文件夹时创建的，不算 Agent 的改动。
        if before.files.contains_key(path)
            || !covered(path)
            || is_teamspace_owned(path, own_role)
            || is_own_journal(path, own_role)
        {
            continue;
        }
        changes.push(ChangedFile {
            path: path.clone(),
            change: FileChangeKind::Added,
        });
    }
    changes.sort_by(|a, b| a.path.cmp(&b.path));
    Diff {
        changes,
        note,
        incomplete,
    }
}

/// 续接时重定基线：`base` 是第一次执行前的快照，`interrupted` 是中断那一刻的，`current` 是现在的。
/// 中断到续接之间的改动（`interrupted` → `current`）不是本次执行的：把这些路径在基线里改成现在的状态，
/// 这样续接后的比对不再把它们算进来；改了几个文件写进基线的说明。
pub fn rebase_after_gap(
    root: &Path,
    base: &Snapshot,
    interrupted: &Snapshot,
    current: &Snapshot,
    own_role: Option<&str>,
) -> Snapshot {
    let gap = diff(root, interrupted, current, own_role);
    let mut rebased = base.clone();
    for change in &gap.changes {
        match change.change {
            FileChangeKind::Deleted => {
                rebased.files.remove(&change.path);
            }
            _ => {
                if let Some(entry) = current.files.get(&change.path) {
                    rebased.files.insert(change.path.clone(), entry.clone());
                }
            }
        }
    }
    if !gap.changes.is_empty() {
        let shown: Vec<&str> = gap
            .changes
            .iter()
            .take(5)
            .map(|c| c.path.as_str())
            .collect();
        let more = if gap.changes.len() > shown.len() {
            format!(" 等 {} 个", gap.changes.len())
        } else {
            String::new()
        };
        rebased.notes.push(format!(
            "中断到续接期间另有 {} 个文件被改动（{}{}），不计入本次执行，也没有核对是否越出权限范围",
            gap.changes.len(),
            shown.join("、"),
            more
        ));
    }
    rebased
}

/// 把快照存到 `path`（先写临时文件再改名）。
pub fn save(path: &Path, snapshot: &Snapshot) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, serde_json::to_vec(snapshot)?)?;
    std::fs::rename(&tmp, path)
}

/// 读出存下的快照；不存在或损坏返回 `None`。
pub fn load(path: &Path) -> Option<Snapshot> {
    serde_json::from_slice(&std::fs::read(path).ok()?).ok()
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::time::{Duration, SystemTime};

    use super::*;

    const ROLE: &str = "Teamspace/前端";

    fn write(root: &Path, rel: &str, content: &str) {
        let path = root.join(rel);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, content).unwrap();
    }

    fn snap(root: &Path) -> Snapshot {
        take(root, MAX_ENTRIES, &[])
    }

    fn changes(diff: &Diff) -> Vec<(String, FileChangeKind)> {
        diff.changes
            .iter()
            .map(|c| (c.path.clone(), c.change))
            .collect()
    }

    fn set_mtime(root: &Path, rel: &str, time: SystemTime) {
        let file = fs::File::options()
            .write(true)
            .open(root.join(rel))
            .unwrap();
        file.set_modified(time).unwrap();
    }

    /// 修改时间有时同一刻相同：改完后把时间往后拨一点，确保修改被看到。
    fn bump_mtime(root: &Path, rel: &str) {
        set_mtime(root, rel, SystemTime::now() + Duration::from_secs(5));
    }

    fn mtime(root: &Path, rel: &str) -> SystemTime {
        fs::metadata(root.join(rel)).unwrap().modified().unwrap()
    }

    #[test]
    fn reports_added_modified_and_deleted_files() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        write(root, "keep.txt", "k");
        write(root, "docs/edit.md", "old");
        write(root, "docs/gone.md", "bye");
        let before = snap(root);

        write(root, "docs/edit.md", "new content");
        bump_mtime(root, "docs/edit.md");
        fs::remove_file(root.join("docs/gone.md")).unwrap();
        write(root, "out/shell.txt", "x");
        let after = take_light(root, MAX_ENTRIES, &[]);

        let d = diff(root, &before, &after, Some(ROLE));
        assert_eq!(
            changes(&d),
            vec![
                ("docs/edit.md".into(), FileChangeKind::Modified),
                ("docs/gone.md".into(), FileChangeKind::Deleted),
                ("out/shell.txt".into(), FileChangeKind::Added),
            ]
        );
        assert_eq!(d.note, None);
        assert!(!d.incomplete);
        // 没变化时没有改动（即使要读文件内容比对摘要）。
        assert!(
            diff(root, &after, &snap(root), Some(ROLE))
                .changes
                .is_empty()
        );
    }

    #[test]
    fn same_size_rewrite_is_seen_through_the_modified_time() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        write(root, "a.txt", "1234");
        let before = snap(root);
        write(root, "a.txt", "abcd");
        bump_mtime(root, "a.txt");
        let d = diff(root, &before, &snap(root), Some(ROLE));
        assert_eq!(
            changes(&d),
            vec![("a.txt".into(), FileChangeKind::Modified)]
        );
    }

    /// 一般 #2：保留修改时间的同大小写入（cp -p、tar x、touch -r）只能靠内容摘要发现。
    #[test]
    fn rewrite_that_keeps_size_and_modified_time_is_seen_through_the_digest() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        write(root, "a.txt", "1234");
        write(root, "same.txt", "unchanged");
        let original = mtime(root, "a.txt");
        let before = snap(root);
        assert!(before.files["a.txt"].digest.is_some());

        write(root, "a.txt", "abcd");
        set_mtime(root, "a.txt", original);
        // 执行结束时的快照不读内容，比对时才对「大小和时间都没变」的文件读内容。
        let after = take_light(root, MAX_ENTRIES, &[]);
        assert_eq!(after.files["a.txt"].digest, None);
        assert_eq!(after.files["a.txt"].mtime, before.files["a.txt"].mtime);
        let d = diff(root, &before, &after, Some(ROLE));
        assert_eq!(
            changes(&d),
            vec![("a.txt".into(), FileChangeKind::Modified)]
        );
        // 带摘要的快照（中断时存下的那种）同样适用。
        let d = diff(root, &before, &snap(root), Some(ROLE));
        assert_eq!(d.changes.len(), 1);
    }

    /// 过大的文件不记摘要，只按大小和修改时间判断（已知的取舍）。
    #[test]
    fn files_over_the_digest_limit_fall_back_to_size_and_time() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let big = vec![b'a'; FILE_DIGEST_LIMIT as usize + 1];
        fs::write(root.join("big.bin"), &big).unwrap();
        let original = mtime(root, "big.bin");
        let before = snap(root);
        assert_eq!(before.files["big.bin"].digest, None);
        let mut other = big.clone();
        other[0] = b'b';
        fs::write(root.join("big.bin"), &other).unwrap();
        set_mtime(root, "big.bin", original);
        assert!(
            diff(root, &before, &snap(root), Some(ROLE))
                .changes
                .is_empty()
        );
    }

    /// 一般 #1：隐藏文件和目录也检查；只跳过 .git、.DS_Store、._*、依赖与构建目录、符号链接。
    #[test]
    fn hidden_files_are_checked_while_git_and_build_directories_are_skipped() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        write(root, "src/a.txt", "a");
        write(root, ".env", "SECRET=1");
        write(root, ".github/workflows/ci.yml", "on: push");
        write(root, ".git/config", "g");
        write(root, ".DS_Store", "finder");
        write(root, "docs/.DS_Store", "finder");
        write(root, "docs/._notes.md", "appledouble");
        write(root, "node_modules/p/index.js", "n");
        write(root, "target/debug/x", "t");
        write(root, "dist/bundle.js", "d");
        write(root, "build/out.o", "b");
        write(root, "pkg/__pycache__/m.pyc", "c");
        std::os::unix::fs::symlink(root.join("src"), root.join("link")).unwrap();
        let before = snap(root);
        let paths: Vec<&str> = before.files.keys().map(String::as_str).collect();
        assert_eq!(paths, vec![".env", ".github/workflows/ci.yml", "src/a.txt"]);
        assert!(!before.truncated);

        // 隐藏文件里的改动看得见；跳过的位置里的写入看不见。
        write(root, ".env", "SECRET=2 changed");
        write(root, ".gitignore", "x");
        write(root, ".git/HEAD", "x");
        write(root, "node_modules/p/more.js", "x");
        write(root, "docs/.DS_Store", "finder again");
        let d = diff(root, &before, &snap(root), Some(ROLE));
        assert_eq!(
            changes(&d),
            vec![
                (".env".into(), FileChangeKind::Modified),
                (".gitignore".into(), FileChangeKind::Added),
            ]
        );
    }

    #[test]
    fn journal_appends_do_not_count_but_rewrites_do() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let journal = "Teamspace/前端/工作日志.md";
        write(root, journal, "# 日志\n- 第一条\n");
        write(root, "Teamspace/前端/岗位说明.md", "brief v1");
        write(root, "Teamspace/README.md", "readme");
        let before = snap(root);

        // Teamspace 或 Agent 追加；Teamspace 重写岗位说明。
        write(root, journal, "# 日志\n- 第一条\n- 第二条\n");
        write(root, "Teamspace/前端/岗位说明.md", "brief v2 longer");
        write(root, "Teamspace/README.md", "readme changed");
        let appended = snap(root);
        assert!(
            diff(root, &before, &appended, Some(ROLE))
                .changes
                .is_empty()
        );

        // 被整体改写（不再以原内容开头）：算改动，即使大小相同。
        write(root, journal, "# 日志\n- 被改了\n");
        assert_eq!(
            changes(&diff(root, &before, &snap(root), Some(ROLE))),
            vec![(journal.to_owned(), FileChangeKind::Modified)]
        );

        // 被截断也算改动。
        write(root, journal, "# 日");
        assert_eq!(
            changes(&diff(root, &before, &snap(root), Some(ROLE))),
            vec![(journal.to_owned(), FileChangeKind::Modified)]
        );

        // 被删除算改动。
        fs::remove_file(root.join(journal)).unwrap();
        assert_eq!(
            changes(&diff(root, &before, &snap(root), Some(ROLE))),
            vec![(journal.to_owned(), FileChangeKind::Deleted)]
        );
    }

    /// 一般 #4：只有本岗位的岗位说明、工作日志、过程文件/README.md 被排除；别的岗位的按普通文件处理
    /// （其他岗位已有日志的纯追加除外，那可能是 Teamspace 自己写的）。
    #[test]
    fn only_the_own_role_files_are_exempt() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        write(root, "a.txt", "a");
        write(root, "Teamspace/测试/工作日志.md", "# 测试日志\n");
        write(root, "Teamspace/测试/岗位说明.md", "brief v1");
        let before = snap(root);

        // 本岗位：Teamspace 新建的文件夹内容都不算。
        write(root, "Teamspace/README.md", "r");
        write(root, "Teamspace/前端/岗位说明.md", "b");
        write(root, "Teamspace/前端/工作日志.md", "j");
        write(root, "Teamspace/前端/过程文件/README.md", "p");
        write(root, "Teamspace/前端/过程文件/draft.md", "agent output");
        // 别的岗位：改岗位说明、新建不存在岗位的日志与说明、新建 README 都是普通改动；
        // 已有日志的纯追加不算。
        write(root, "Teamspace/测试/岗位说明.md", "tampered brief");
        write(root, "Teamspace/测试/工作日志.md", "# 测试日志\n- 追加\n");
        write(root, "Teamspace/编造/工作日志.md", "fake");
        write(root, "Teamspace/编造/过程文件/README.md", "fake");
        let d = diff(root, &before, &snap(root), Some(ROLE));
        assert_eq!(
            changes(&d),
            vec![
                (
                    "Teamspace/前端/过程文件/draft.md".into(),
                    FileChangeKind::Added
                ),
                (
                    "Teamspace/测试/岗位说明.md".into(),
                    FileChangeKind::Modified
                ),
                ("Teamspace/编造/工作日志.md".into(), FileChangeKind::Added),
                (
                    "Teamspace/编造/过程文件/README.md".into(),
                    FileChangeKind::Added
                ),
            ]
        );
        // 不知道本岗位时不做岗位排除（只排除 Teamspace/README.md）。
        let d = diff(root, &before, &snap(root), None);
        assert!(
            d.changes
                .iter()
                .any(|c| c.path == "Teamspace/前端/岗位说明.md")
        );
    }

    /// 文件数超过上限时降级：快照标明被截断，比对只看两边都覆盖到的路径，
    /// 排在后面的新增文件不会被误报，也不会把没扫到的文件当成已删除。
    #[test]
    fn degrades_when_there_are_too_many_files() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        for i in 0..10 {
            write(root, &format!("f{i:02}.txt"), "x");
        }
        let full = snap(root);
        assert!(!full.truncated);
        assert_eq!(full.files.len(), 10);

        let limited = take(root, 4, &[]);
        assert!(limited.truncated);
        assert_eq!(limited.files.len(), 4);
        assert_eq!(limited.last.as_deref(), Some("f03.txt"));

        // 靠前的新增被发现；靠后的新增、已存在文件的修改都在覆盖范围之外，不报。
        write(root, "a-new.txt", "n");
        write(root, "z-new.txt", "n");
        write(root, "f09.txt", "changed after the bound");
        bump_mtime(root, "f09.txt");
        let after = take_light(root, 4, &[]);
        let d = diff(root, &limited, &after, Some(ROLE));
        let seen = changes(&d);
        assert!(
            !seen.iter().any(|(p, _)| p == "z-new.txt" || p == "f09.txt"),
            "{seen:?}"
        );
        // 覆盖范围内的新增文件把别的文件挤出了覆盖范围，这些不会被报成删除。
        assert!(
            !seen.iter().any(|(_, k)| *k == FileChangeKind::Deleted),
            "{seen:?}"
        );
        assert!(d.incomplete);
        let note = d.note.expect("降级时必须有说明");
        assert!(note.contains("改动统计不完整"), "{note}");
        assert!(note.contains("超过 4 个"), "{note}");

        // 完整快照对完整快照没有说明。
        let d = diff(root, &full, &snap(root), Some(ROLE));
        assert_eq!(d.note, None);
        assert!(!d.incomplete);
    }

    /// 一般 #6：降级时先完整扫描优先目录（本岗位文件夹、任务引用所在目录），它们里面的改动照样发现。
    #[test]
    fn priority_paths_stay_covered_when_the_snapshot_degrades() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        // 排在优先目录之后的一大堆文件（中文目录名按字节序排在 ASCII 之后）。
        for i in 0..30 {
            write(root, &format!("素材/clip{i:02}.mov"), "v");
        }
        write(root, "docs/spec.md", "spec");
        write(root, "Teamspace/前端/过程文件/note.md", "n");
        let priority = vec!["Teamspace/前端".to_owned(), "docs".to_owned()];
        let before = take(root, 12, &priority);
        assert!(before.truncated);
        assert_eq!(before.covered, vec!["Teamspace/前端", "docs"]);
        assert!(before.files.contains_key("docs/spec.md"));
        assert!(before.files.contains_key("Teamspace/前端/过程文件/note.md"));
        assert!(!before.files.contains_key("素材/clip29.mov"));

        // 优先目录里的新文件被发现；优先目录之外靠后的改动看不到，并明确说明不完整。
        write(root, "docs/extra.txt", "agent wrote this");
        write(root, "素材/clip29.mov", "changed");
        write(root, "素材/new.mov", "new");
        let after = take_light(root, 12, &priority);
        let d = diff(root, &before, &after, Some(ROLE));
        assert_eq!(
            changes(&d),
            vec![("docs/extra.txt".into(), FileChangeKind::Added)]
        );
        assert!(d.incomplete);
        assert!(d.note.unwrap().contains("改动统计不完整"));
    }

    #[test]
    fn nested_and_duplicate_priority_paths_are_collapsed() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        write(root, "docs/a/b.md", "b");
        write(root, "docs/c.md", "c");
        let s = take(
            root,
            100,
            &[
                "docs".into(),
                "docs/a".into(),
                "docs".into(),
                "".into(),
                "missing/dir".into(),
                "docs/c.md".into(),
            ],
        );
        assert_eq!(s.priority, vec!["docs", "missing/dir"]);
        assert_eq!(s.files.len(), 2);
    }

    #[test]
    fn directories_count_toward_the_limit() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        for i in 0..6 {
            fs::create_dir_all(root.join(format!("d{i}"))).unwrap();
        }
        write(root, "z.txt", "z");
        let s = take(root, 3, &[]);
        assert!(s.truncated);
        assert_eq!(s.last.as_deref(), Some("d2"));
        // 只有空目录，没有记录到文件，也不会崩。
        assert!(s.files.is_empty());
    }

    /// 一般 #3：续接时，中断到续接之间的改动不算本次执行的，只写进说明。
    #[test]
    fn rebase_after_gap_excludes_changes_made_between_interrupt_and_resume() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        write(root, "keep.txt", "k");
        write(root, "docs/edit.md", "old");
        write(root, "docs/gone.md", "bye");
        let base = snap(root);
        // 执行中 Agent 写了 agent.txt，然后中断。
        write(root, "agent.txt", "by agent");
        let interrupted = snap(root);
        // 中断期间 Human 手动改动。
        write(root, "docs/edit.md", "human edit");
        bump_mtime(root, "docs/edit.md");
        fs::remove_file(root.join("docs/gone.md")).unwrap();
        write(root, "human.txt", "by human");
        let current = snap(root);

        let rebased = rebase_after_gap(root, &base, &interrupted, &current, Some(ROLE));
        assert_eq!(rebased.notes.len(), 1);
        assert!(rebased.notes[0].contains("中断到续接期间另有 3 个文件被改动"));
        assert!(rebased.notes[0].contains("不计入本次执行"));

        // 续接后 Agent 又写了 later.txt：只有 agent.txt 和 later.txt 算本次执行的。
        write(root, "later.txt", "later");
        let d = diff(
            root,
            &rebased,
            &take_light(root, MAX_ENTRIES, &[]),
            Some(ROLE),
        );
        assert_eq!(
            changes(&d),
            vec![
                ("agent.txt".into(), FileChangeKind::Added),
                ("later.txt".into(), FileChangeKind::Added),
            ]
        );
        assert!(d.note.unwrap().contains("中断到续接期间"));
        // 原始基线不受影响。
        assert!(base.notes.is_empty());
        // 没有间隙改动时不加说明。
        let same = rebase_after_gap(root, &base, &current, &current, Some(ROLE));
        assert!(same.notes.is_empty());
    }

    #[test]
    fn snapshot_survives_a_save_and_load_round_trip() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("ws");
        write(&root, "a.txt", "a");
        write(&root, "Teamspace/前端/工作日志.md", "j");
        let s = take(&root, MAX_ENTRIES, &["Teamspace/前端".to_owned()]);
        let file = dir.path().join("data/run-snapshots/r1.json");
        save(&file, &s).unwrap();
        assert_eq!(load(&file), Some(s));
        assert_eq!(load(&dir.path().join("missing.json")), None);
        fs::write(&file, "not json").unwrap();
        assert_eq!(load(&file), None);
        // 旧版本存下的快照（没有优先路径等字段）仍能读出。
        fs::write(
            &file,
            r#"{"files":{"a.txt":{"size":1,"mtime":2}},"truncated":false,"last":null,"max_entries":5}"#,
        )
        .unwrap();
        let old = load(&file).unwrap();
        assert!(old.priority.is_empty() && old.covered.is_empty() && old.notes.is_empty());
    }

    #[test]
    fn path_order_matches_scan_order() {
        // 逐段比较：`a/b` 排在 `a-b` 之前（目录 `a` 的内容在 `a-b` 之前扫描）。
        assert_eq!(cmp_paths("a/b", "a-b"), Ordering::Less);
        assert_eq!(cmp_paths("a", "a/b"), Ordering::Less);
        assert_eq!(cmp_paths("x/y", "x/y"), Ordering::Equal);
        assert!(within("a/b/c", "a/b"));
        assert!(within("a/b", "a/b"));
        assert!(!within("a/bc", "a/b"));
        assert!(!within("a", "a/b"));
    }

    /// 性能估算（默认不跑）：`cargo test -p teamspace-core --lib --release perf_20k -- --ignored --nocapture`。
    /// 2 万个 4 KB 文件：执行前带摘要的快照、执行结束时的轻量快照、比对（读 2 万个未改动文件核对摘要）。
    #[test]
    #[ignore = "性能估算，手动运行"]
    fn perf_20k_files() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let body = "x".repeat(4096);
        for i in 0..20_000 {
            write(root, &format!("d{:03}/f{i:05}.txt", i % 200), &body);
        }
        let t = std::time::Instant::now();
        let before = take(root, MAX_ENTRIES, &[]);
        let take_ms = t.elapsed().as_millis();
        let t = std::time::Instant::now();
        let after = take_light(root, MAX_ENTRIES, &[]);
        let light_ms = t.elapsed().as_millis();
        let t = std::time::Instant::now();
        let d = diff(root, &before, &after, Some(ROLE));
        let diff_ms = t.elapsed().as_millis();
        assert!(d.changes.is_empty());
        eprintln!(
            "20000 files x 4KB: take(with digest)={take_ms}ms take_light={light_ms}ms diff(reads all)={diff_ms}ms files={}",
            before.files.len()
        );
    }
}
