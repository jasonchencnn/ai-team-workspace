//! 执行目录快照（S-1b / G-6）：不依赖 Agent 自觉的改动统计兜底。
//!
//! 真实 Agent 可能用 shell 直接写文件，既不走 `fs/write_text_file` 也不发权限请求，ACP 报告的改动
//! 因此漏掉这些文件。普通文件夹原地执行时，执行开始前对执行目录拍一份快照（相对路径、大小、修改时间），
//! 结束（或中断）时再拍一份比对，得出真实的新增、修改、删除（[`diff`]）。Git worktree 执行直接用 Git
//! 统计（[`super::worktree::changed_files`]），不用快照。
//!
//! 范围与上限：
//! - 跳过隐藏的文件和目录（以 `.` 开头，含 `.git`）、符号链接，以及 [`SKIP_DIRS`] 里的目录
//!   （`node_modules`、`target` 等构建与依赖目录）；
//! - 最多记录 [`MAX_ENTRIES`] 个文件与目录。超过上限时快照降级：只覆盖按路径顺序排在前面的部分，
//!   [`Snapshot::truncated`] 为真；比对时只看两份快照都覆盖到的路径，并在 [`Diff::note`] 里说明，
//!   绝不把没扫到的文件误报成新增或删除；
//! - 工作日志（`Teamspace/<岗位>/工作日志.md`）额外记录内容摘要：Teamspace 与 Agent 都会向它追加（D14），
//!   纯追加不算改动，被改写或截断才算；
//! - Teamspace 自己维护的文件（岗位说明、说明文件）不计入。
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
/// 不进入的目录（依赖与构建产物，体量大且不是任务产出）。隐藏目录（`.` 开头）另行整体跳过。
pub const SKIP_DIRS: &[&str] = &["node_modules", "target", "__pycache__", "dist", "build"];
/// 目录深度上限，防止病态的深层嵌套。
const MAX_DEPTH: usize = 64;
/// 工作日志超过这个大小时不计内容摘要（只按大小判断是否被截断）。
const DIGEST_LIMIT: u64 = 4 * 1024 * 1024;

/// 一个文件的快照。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Entry {
    pub size: u64,
    /// 修改时间（自 UNIX 纪元起的纳秒）；取不到为 0。
    pub mtime: u64,
    /// 工作日志的内容摘要（FNV-1a）；其他文件为 `None`。
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
    /// 降级时最后扫描到的路径（按路径顺序，之后的都没有扫描）。
    pub last: Option<String>,
    /// 扫描时使用的上限。
    pub max_entries: usize,
}

/// 两份快照的比对结果。
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Diff {
    /// 新增、修改、删除的文件，按路径排序（不含 Teamspace 自己维护的文件与纯追加的工作日志）。
    pub changes: Vec<ChangedFile>,
    /// 快照降级时的说明（中文）；完整时为 `None`。
    pub note: Option<String>,
}

/// 对 `root` 拍快照，最多记录 `max_entries` 个文件与目录。读取失败的目录或文件被跳过。
pub fn take(root: &Path, max_entries: usize) -> Snapshot {
    let mut snap = Snapshot {
        files: BTreeMap::new(),
        truncated: false,
        last: None,
        max_entries,
    };
    let mut visited = 0usize;
    walk(root, "", 0, &mut visited, &mut snap);
    snap
}

/// 递归扫描 `dir`（相对路径 `rel`，空串为根）。返回 `false` 表示已达上限、应当停止整个扫描。
fn walk(dir: &Path, rel: &str, depth: usize, visited: &mut usize, snap: &mut Snapshot) -> bool {
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
        if name.starts_with('.') {
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
        if *visited >= snap.max_entries {
            snap.truncated = true;
            return false;
        }
        *visited += 1;
        let path = if rel.is_empty() {
            name.clone()
        } else {
            format!("{rel}/{name}")
        };
        snap.last = Some(path.clone());
        if is_dir {
            if !walk(&entry.path(), &path, depth + 1, visited, snap) {
                return false;
            }
        } else if let Ok(meta) = entry.metadata() {
            let digest = if is_journal(&path) && meta.len() <= DIGEST_LIMIT {
                std::fs::read(entry.path()).ok().map(|bytes| fnv1a(&bytes))
            } else {
                None
            };
            snap.files.insert(
                path,
                Entry {
                    size: meta.len(),
                    mtime: meta
                        .modified()
                        .ok()
                        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
                        .map_or(0, |d| d.as_nanos() as u64),
                    digest,
                },
            );
        }
    }
    true
}

/// FNV-1a 64 位摘要（稳定、无依赖；只用来判断工作日志的原有内容是否原样保留，不涉及安全）。
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

/// `Teamspace/<岗位文件夹>/工作日志.md`。
fn is_journal(rel: &str) -> bool {
    let parts: Vec<&str> = components(rel).collect();
    matches!(parts.as_slice(), [root, _, file] if *root == ROOT_DIR && *file == JOURNAL_FILE)
}

/// Teamspace 自己维护的文件（说明文件、岗位说明）：它们的变化不是 Agent 的改动。
fn is_teamspace_owned(rel: &str) -> bool {
    let parts: Vec<&str> = components(rel).collect();
    match parts.as_slice() {
        [root, file] => *root == ROOT_DIR && *file == README_FILE,
        [root, _, file] => *root == ROOT_DIR && *file == BRIEF_FILE,
        [root, _, dir, file] => *root == ROOT_DIR && *dir == PROCESS_DIR && *file == README_FILE,
        _ => false,
    }
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

/// 比对 `before`（执行前）与 `after`（执行后），`root` 用来核对工作日志的内容。
pub fn diff(root: &Path, before: &Snapshot, after: &Snapshot) -> Diff {
    // 降级的快照只覆盖路径顺序靠前的部分：只比对两边都覆盖到的路径。
    let mut bound: Option<&str> = None;
    let mut nothing_covered = false;
    for snap in [before, after] {
        if !snap.truncated {
            continue;
        }
        match snap.last.as_deref() {
            None => nothing_covered = true,
            Some(last) => {
                if bound.is_none_or(|b| cmp_paths(last, b) == Ordering::Less) {
                    bound = Some(last);
                }
            }
        }
    }
    let note = (before.truncated || after.truncated).then(|| {
        format!(
            "执行目录文件太多，快照只覆盖了前 {} 个文件与文件夹（按路径顺序），之后的改动没有统计，\
             范围外改动的检查也只覆盖这一部分",
            before.max_entries.min(after.max_entries)
        )
    });
    if nothing_covered {
        return Diff {
            changes: Vec::new(),
            note,
        };
    }
    let covered = |path: &str| bound.is_none_or(|b| cmp_paths(path, b) != Ordering::Greater);

    let mut changes = Vec::new();
    for (path, old) in &before.files {
        if !covered(path) || is_teamspace_owned(path) {
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
                    old.size != new.size || old.mtime != new.mtime
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
        // 新出现的工作日志是 Teamspace 建岗位文件夹时创建的，不算 Agent 的改动。
        if before.files.contains_key(path)
            || !covered(path)
            || is_teamspace_owned(path)
            || is_journal(path)
        {
            continue;
        }
        changes.push(ChangedFile {
            path: path.clone(),
            change: FileChangeKind::Added,
        });
    }
    changes.sort_by(|a, b| a.path.cmp(&b.path));
    Diff { changes, note }
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
    use std::time::Duration;

    use super::*;

    fn write(root: &Path, rel: &str, content: &str) {
        let path = root.join(rel);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, content).unwrap();
    }

    fn changes(diff: &Diff) -> Vec<(String, FileChangeKind)> {
        diff.changes
            .iter()
            .map(|c| (c.path.clone(), c.change))
            .collect()
    }

    /// 修改时间有时同一刻相同：改完后把时间往后拨一点，确保修改被看到。
    fn bump_mtime(root: &Path, rel: &str) {
        let file = fs::File::options()
            .write(true)
            .open(root.join(rel))
            .unwrap();
        file.set_modified(std::time::SystemTime::now() + Duration::from_secs(5))
            .unwrap();
    }

    #[test]
    fn reports_added_modified_and_deleted_files() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        write(root, "keep.txt", "k");
        write(root, "docs/edit.md", "old");
        write(root, "docs/gone.md", "bye");
        let before = take(root, MAX_ENTRIES);

        write(root, "docs/edit.md", "new content");
        bump_mtime(root, "docs/edit.md");
        fs::remove_file(root.join("docs/gone.md")).unwrap();
        write(root, "out/shell.txt", "x");
        let after = take(root, MAX_ENTRIES);

        let d = diff(root, &before, &after);
        assert_eq!(
            changes(&d),
            vec![
                ("docs/edit.md".into(), FileChangeKind::Modified),
                ("docs/gone.md".into(), FileChangeKind::Deleted),
                ("out/shell.txt".into(), FileChangeKind::Added),
            ]
        );
        assert_eq!(d.note, None);
        // 没变化时没有改动。
        assert!(
            diff(root, &after, &take(root, MAX_ENTRIES))
                .changes
                .is_empty()
        );
    }

    #[test]
    fn same_size_rewrite_is_seen_through_the_modified_time() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        write(root, "a.txt", "1234");
        let before = take(root, MAX_ENTRIES);
        write(root, "a.txt", "abcd");
        bump_mtime(root, "a.txt");
        let d = diff(root, &before, &take(root, MAX_ENTRIES));
        assert_eq!(
            changes(&d),
            vec![("a.txt".into(), FileChangeKind::Modified)]
        );
    }

    #[test]
    fn hidden_symlinked_and_build_directories_are_skipped() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        write(root, "src/a.txt", "a");
        write(root, ".git/config", "g");
        write(root, ".hidden.txt", "h");
        write(root, "node_modules/p/index.js", "n");
        write(root, "target/debug/x", "t");
        write(root, "dist/bundle.js", "d");
        std::os::unix::fs::symlink(root.join("src"), root.join("link")).unwrap();
        let snap = take(root, MAX_ENTRIES);
        let paths: Vec<&str> = snap.files.keys().map(String::as_str).collect();
        assert_eq!(paths, vec!["src/a.txt"]);
        assert!(!snap.truncated);

        // 跳过的位置里的写入看不见。
        write(root, ".git/HEAD", "x");
        write(root, "node_modules/p/more.js", "x");
        assert!(
            diff(root, &snap, &take(root, MAX_ENTRIES))
                .changes
                .is_empty()
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
        let before = take(root, MAX_ENTRIES);

        // Teamspace 或 Agent 追加；Teamspace 重写岗位说明。
        write(root, journal, "# 日志\n- 第一条\n- 第二条\n");
        write(root, "Teamspace/前端/岗位说明.md", "brief v2 longer");
        write(root, "Teamspace/README.md", "readme changed");
        let appended = take(root, MAX_ENTRIES);
        assert!(diff(root, &before, &appended).changes.is_empty());

        // 被整体改写（不再以原内容开头）：算改动，即使大小相同。
        write(root, journal, "# 日志\n- 第一条\n");
        write(root, journal, "# 日志\n- 被改了\n");
        let rewritten = take(root, MAX_ENTRIES);
        assert_eq!(
            changes(&diff(root, &before, &rewritten)),
            vec![(journal.to_owned(), FileChangeKind::Modified)]
        );

        // 被截断也算改动。
        write(root, journal, "# 日");
        let truncated = take(root, MAX_ENTRIES);
        assert_eq!(
            changes(&diff(root, &before, &truncated)),
            vec![(journal.to_owned(), FileChangeKind::Modified)]
        );

        // 被删除算改动。
        fs::remove_file(root.join(journal)).unwrap();
        let removed = take(root, MAX_ENTRIES);
        assert_eq!(
            changes(&diff(root, &before, &removed)),
            vec![(journal.to_owned(), FileChangeKind::Deleted)]
        );
    }

    #[test]
    fn newly_created_role_folder_files_are_not_agent_changes() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        write(root, "a.txt", "a");
        let before = take(root, MAX_ENTRIES);
        write(root, "Teamspace/README.md", "r");
        write(root, "Teamspace/后端/岗位说明.md", "b");
        write(root, "Teamspace/后端/工作日志.md", "j");
        write(root, "Teamspace/后端/过程文件/README.md", "p");
        write(root, "Teamspace/后端/过程文件/draft.md", "agent output");
        let d = diff(root, &before, &take(root, MAX_ENTRIES));
        assert_eq!(
            changes(&d),
            vec![(
                "Teamspace/后端/过程文件/draft.md".into(),
                FileChangeKind::Added
            )]
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
        let full = take(root, MAX_ENTRIES);
        assert!(!full.truncated);
        assert_eq!(full.files.len(), 10);

        let limited = take(root, 4);
        assert!(limited.truncated);
        assert_eq!(limited.files.len(), 4);
        assert_eq!(limited.last.as_deref(), Some("f03.txt"));

        // 靠前的新增被发现；靠后的新增、已存在文件的修改都在覆盖范围之外，不报。
        write(root, "a-new.txt", "n");
        write(root, "z-new.txt", "n");
        write(root, "f09.txt", "changed after the bound");
        bump_mtime(root, "f09.txt");
        let after = take(root, 4);
        let d = diff(root, &limited, &after);
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
        let note = d.note.expect("降级时必须有说明");
        assert!(note.contains("快照只覆盖了前 4 个"), "{note}");
        assert!(note.contains("没有统计"), "{note}");

        // 完整快照对完整快照没有说明。
        assert_eq!(diff(root, &full, &take(root, MAX_ENTRIES)).note, None);
    }

    #[test]
    fn directories_count_toward_the_limit() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        for i in 0..6 {
            fs::create_dir_all(root.join(format!("d{i}"))).unwrap();
        }
        write(root, "z.txt", "z");
        let snap = take(root, 3);
        assert!(snap.truncated);
        assert_eq!(snap.last.as_deref(), Some("d2"));
        // 只有空目录，没有记录到文件，也不会崩。
        assert!(snap.files.is_empty());
    }

    #[test]
    fn snapshot_survives_a_save_and_load_round_trip() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("ws");
        write(&root, "a.txt", "a");
        write(&root, "Teamspace/前端/工作日志.md", "j");
        let snap = take(&root, MAX_ENTRIES);
        let file = dir.path().join("data/run-snapshots/r1.json");
        save(&file, &snap).unwrap();
        assert_eq!(load(&file), Some(snap));
        assert_eq!(load(&dir.path().join("missing.json")), None);
        fs::write(&file, "not json").unwrap();
        assert_eq!(load(&file), None);
    }

    #[test]
    fn path_order_matches_scan_order() {
        // 逐段比较：`a/b` 排在 `a-b` 之前（目录 `a` 的内容在 `a-b` 之前扫描）。
        assert_eq!(cmp_paths("a/b", "a-b"), Ordering::Less);
        assert_eq!(cmp_paths("a", "a/b"), Ordering::Less);
        assert_eq!(cmp_paths("x/y", "x/y"), Ordering::Equal);
    }
}
