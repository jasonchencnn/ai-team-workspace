//! 执行时的权限规则（D15、D18），纯函数，不访问数据库。
//!
//! - 工具调用类型为读取（read）、搜索（search）：涉及的路径（请求的 locations 与 rawInput 中能取到的
//!   路径）按真实路径都在执行目录、工作空间或本岗位文件夹内 → 自动允许；没有路径（如在当前目录里
//!   搜索）→ 自动允许；任意一个越界（含 `..` 穿越、指向外部的符号链接）→ 转交 Human（D18）；
//! - 编辑（edit）且所有路径都可写 → 自动允许；可写范围：
//!   - 本岗位文件夹（工作空间中的 `Teamspace/<岗位>/`）；
//!   - 任务引用的文件（引用的是目录时含其中的文件）；
//!   - Git worktree 执行时，worktree 内的任何位置（`.git` 除外）；
//!   - 例外：岗位文件夹中的 `岗位说明.md` 由 Teamspace 维护，不自动允许；
//!   - 岗位文件夹中的 `工作日志.md`（D14）：只自动允许纯追加——新内容以磁盘上的现有内容为前缀。
//!     权限请求阶段按 ACP diff 内容判断（[`journal_edit_appends`]）：diff 可以是整文件
//!     （`oldText` 为空或等于现有内容），也可以是片段替换（`oldText` 只带文件末尾的一段，
//!     `newText` 是「这一段 + 追加的内容」，Claude Code 的 Edit 工具就是这种）；
//!     判断不了就转交 Human；
//!     `fs/write_text_file` 阶段由 driver 在写入锁内重新读取再判断（见
//!     [`crate::role_folder::write_journal_from_agent`]），这一步不因权限请求曾被允许而跳过；
//! - 其他（执行命令、删除、移动、联网、切换模式、未声明类型、编辑范围外的路径）→ 转交 Human。
//!
//! 路径一律先规范化再比较：相对路径按执行目录解析；含 `..` 的路径直接视为越界；已存在的部分解析
//! 符号链接（指向范围外的链接因此不会被当作范围内），不存在的尾部按字面拼接。
//! `fs/read_text_file`、`fs/write_text_file` 用同一套范围（[`Scope::can_read`]、[`Scope::can_write`]）。

use std::path::{Component, Path, PathBuf};

use crate::model::RunIsolation;
use crate::role_folder::{BRIEF_FILE, JOURNAL_FILE};

/// 一次执行的读写范围。构造时根目录已规范化。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Scope {
    exec_dir: PathBuf,
    isolation: RunIsolation,
    role_folders: Vec<PathBuf>,
    refs: Vec<PathBuf>,
    protected: Vec<PathBuf>,
    journals: Vec<PathBuf>,
    workspace: PathBuf,
}

/// 写入目标的分类（[`Scope::write_target`]）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WriteTarget {
    /// 自动批准范围内的普通文件。
    Free(PathBuf),
    /// 岗位文件夹中的工作日志：只自动允许纯追加。
    Journal(PathBuf),
}

/// 编辑类工具调用附带的 ACP diff 内容（`{type: "diff", path, oldText, newText}`）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EditPreview {
    pub path: String,
    pub old_text: Option<String>,
    pub new_text: String,
}

/// 规则判断结果，附中文原因。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Verdict {
    Allow(String),
    Ask(String),
}

impl Scope {
    /// `exec_dir`：执行目录；`role_folders`：岗位文件夹的绝对路径（工作空间中的，以及 worktree 中的
    /// 对应位置）；`refs`：任务引用文件的绝对路径（工作空间与执行目录中的都可以给出）。
    /// 工作空间视为与执行目录相同（原地执行）；worktree 执行用 [`Scope::with_workspace`]。
    pub fn new(
        exec_dir: &Path,
        isolation: RunIsolation,
        role_folders: &[PathBuf],
        refs: &[PathBuf],
    ) -> Self {
        Self::with_workspace(exec_dir, exec_dir, isolation, role_folders, refs)
    }

    /// 同 [`Scope::new`]，另给出工作空间根目录。引用文件按真实路径（解析符号链接后）计入范围，
    /// 真实路径不在执行目录或工作空间之内的引用（指向外部的符号链接）不计入（Q-06）。
    pub fn with_workspace(
        exec_dir: &Path,
        workspace: &Path,
        isolation: RunIsolation,
        role_folders: &[PathBuf],
        refs: &[PathBuf],
    ) -> Self {
        let exec_dir = resolve(exec_dir, Path::new("/")).unwrap_or_else(|_| exec_dir.to_owned());
        let workspace = resolve(workspace, Path::new("/")).unwrap_or_else(|_| workspace.to_owned());
        let canon = |p: &PathBuf| resolve(p, &exec_dir).ok();
        let refs = refs
            .iter()
            .filter_map(canon)
            .filter(|real| real.starts_with(&exec_dir) || real.starts_with(&workspace))
            .collect();
        let role_folders: Vec<PathBuf> = role_folders.iter().filter_map(canon).collect();
        let protected = role_folders
            .iter()
            .map(|dir| dir.join(BRIEF_FILE))
            .collect();
        let journals = role_folders
            .iter()
            .map(|dir| dir.join(JOURNAL_FILE))
            .collect();
        Self {
            refs,
            exec_dir,
            isolation,
            role_folders,
            protected,
            journals,
            workspace,
        }
    }

    pub fn exec_dir(&self) -> &Path {
        &self.exec_dir
    }

    /// 可否不经确认读取：执行目录、工作空间、岗位文件夹或引用文件之内（D18）。
    /// 返回规范化后的路径或中文原因（工作空间之外须先发起读取权限请求并经 Human 允许）。
    pub fn can_read(&self, path: &str) -> Result<PathBuf, String> {
        let real = resolve(Path::new(path), &self.exec_dir)?;
        if self.readable(&real) {
            Ok(real)
        } else {
            Err(format!(
                "读取工作空间之外的文件需要先发起读取权限请求并经 Human 允许：{path}"
            ))
        }
    }

    fn readable(&self, real: &Path) -> bool {
        real.starts_with(&self.exec_dir)
            || real.starts_with(&self.workspace)
            || self.role_folders.iter().any(|d| real.starts_with(d))
            || self.refs.iter().any(|r| real.starts_with(r))
    }

    /// 读取、搜索请求涉及的路径中第一个越出可读范围的（无法解析的，如含 `..`，也算越界）。
    fn first_unreadable<'a>(&self, paths: &'a [String]) -> Option<&'a str> {
        paths
            .iter()
            .find(|p| {
                !resolve(Path::new(p.as_str()), &self.exec_dir)
                    .is_ok_and(|real| self.readable(&real))
            })
            .map(String::as_str)
    }

    /// 可否不看内容直接自动写入（见模块说明）。工作日志只允许追加，因此这里也返回 `Err`；
    /// 需要区分工作日志时用 [`Scope::write_target`]。返回规范化后的路径或中文原因。
    pub fn can_write(&self, path: &str) -> Result<PathBuf, String> {
        match self.write_target(path)? {
            WriteTarget::Free(real) => Ok(real),
            WriteTarget::Journal(_) => Err(journal_reason(path)),
        }
    }

    /// 写入目标分类：工作日志、范围内的普通文件，或需要确认（`Err` 附中文原因）。
    pub fn write_target(&self, path: &str) -> Result<WriteTarget, String> {
        let real = resolve(Path::new(path), &self.exec_dir)?;
        if self.journals.contains(&real) {
            return Ok(WriteTarget::Journal(real));
        }
        let real = self.free_write(real, path)?;
        Ok(WriteTarget::Free(real))
    }

    fn free_write(&self, real: PathBuf, path: &str) -> Result<PathBuf, String> {
        if self.protected.contains(&real) {
            return Err(format!(
                "{} 由 Teamspace 维护，修改需要确认：{path}",
                real.file_name().unwrap_or_default().to_string_lossy()
            ));
        }
        if let Ok(rest) = real.strip_prefix(&self.exec_dir)
            && rest.components().any(|c| c.as_os_str() == ".git")
        {
            return Err(format!("修改 Git 内部文件需要确认：{path}"));
        }
        if self.role_folders.iter().any(|d| real.starts_with(d)) {
            return Ok(real);
        }
        if self.refs.iter().any(|r| real.starts_with(r)) {
            return Ok(real);
        }
        if self.isolation == RunIsolation::Worktree && real.starts_with(&self.exec_dir) {
            return Ok(real);
        }
        Err(match self.isolation {
            RunIsolation::Worktree => format!("写入 worktree 之外需要确认：{path}"),
            RunIsolation::InPlace => {
                format!("写入本岗位文件夹与任务引用文件之外需要确认：{path}")
            }
        })
    }

    /// 对一次工具调用的权限请求做判断。`kind` 为 ACP 工具类型，`paths` 为涉及的路径。
    /// 不带 diff 内容，因此涉及工作日志的编辑一律转交 Human；见 [`Scope::decide_with`]。
    pub fn decide(&self, kind: Option<&str>, paths: &[String]) -> Verdict {
        self.decide_with(kind, paths, &[])
    }

    /// 同 [`Scope::decide`]，`edits` 为工具调用附带的 diff 内容，用来判断对工作日志的编辑是否纯追加。
    pub fn decide_with(
        &self,
        kind: Option<&str>,
        paths: &[String],
        edits: &[EditPreview],
    ) -> Verdict {
        match kind {
            Some(kind @ ("read" | "search")) => match self.first_unreadable(paths) {
                Some(path) => Verdict::Ask(format!("读取工作空间之外的文件需要确认：{path}")),
                None if kind == "read" => {
                    Verdict::Allow("读取工作空间内的文件，自动允许".to_owned())
                }
                None => Verdict::Allow("在工作空间内搜索，自动允许".to_owned()),
            },
            Some("edit") => {
                if paths.is_empty() {
                    return Verdict::Ask("编辑操作没有给出文件路径，需要确认".to_owned());
                }
                let mut journal = false;
                for path in paths {
                    match self.write_target(path) {
                        Ok(WriteTarget::Free(_)) => {}
                        Ok(WriteTarget::Journal(real)) => {
                            if !self.journal_edits_append(&real, edits) {
                                return Verdict::Ask(journal_reason(path));
                            }
                            journal = true;
                        }
                        Err(reason) => return Verdict::Ask(reason),
                    }
                }
                if journal {
                    return Verdict::Allow(
                        if paths.len() == 1 {
                            "在工作日志末尾追加进展，自动允许"
                        } else {
                            "在工作日志末尾追加进展并编辑范围内的文件，自动允许"
                        }
                        .to_owned(),
                    );
                }
                Verdict::Allow(
                    match self.isolation {
                        RunIsolation::Worktree => "编辑 worktree 或岗位文件夹内的文件，自动允许",
                        RunIsolation::InPlace => "编辑本岗位文件夹或任务引用文件，自动允许",
                    }
                    .to_owned(),
                )
            }
            Some("execute") => Verdict::Ask("执行命令需要确认".to_owned()),
            Some("delete") => Verdict::Ask("删除文件需要确认".to_owned()),
            Some("move") => Verdict::Ask("移动或重命名文件需要确认".to_owned()),
            Some("fetch") => Verdict::Ask("访问网络需要确认".to_owned()),
            Some("switch_mode") => Verdict::Ask("切换 Agent 工作模式需要确认".to_owned()),
            Some(other) => Verdict::Ask(format!("「{other}」类操作需要确认")),
            None => Verdict::Ask("Agent 没有说明操作类型，需要确认".to_owned()),
        }
    }

    /// 对工作日志 `real` 的所有 diff 是否都是纯追加；没有对应 diff（无法判断）时为 `false`。
    fn journal_edits_append(&self, real: &Path, edits: &[EditPreview]) -> bool {
        let mine: Vec<&EditPreview> = edits
            .iter()
            .filter(|e| resolve(Path::new(&e.path), &self.exec_dir).as_deref() == Ok(real))
            .collect();
        if mine.is_empty() {
            return false;
        }
        let Ok(disk) = std::fs::read_to_string(real) else {
            return false;
        };
        mine.iter()
            .all(|e| journal_edit_appends(&disk, e.old_text.as_deref(), &e.new_text))
    }
}

fn journal_reason(path: &str) -> String {
    format!("工作日志只允许在末尾追加，修改或删除已有内容需要确认：{path}")
}

/// 一次 diff（`old` → `new`）作用在工作日志现有内容 `disk` 上是否只是在末尾追加。
///
/// 判定按「改完之后的内容」来算，而不是逐字比较片段：
/// - `old` 为空（整文件写入）或等于 `disk`：`new` 须以 `disk` 为前缀；
/// - `old` 是片段（按片段替换的编辑工具，如 Claude Code 的 Edit：`oldText` 只带文件末尾的
///   一小段，`newText` 是「这一段 + 追加的内容」）：`old` 须在 `disk` 中**唯一**出现，否则
///   替换结果无法确定；把这一段换成 `new` 之后，整个文件须仍以 `disk` 开头——也就是
///   「已有内容一字未动，只在末尾接上新的内容」。
///
/// 片段不必正好是文件末尾的字节：只要替换之后整体仍以现有内容开头（如 `oldText` 少了行尾
/// 换行、`newText` 把它补回来），就是纯追加。任何改动已有内容的情况（覆盖、截断、删改、
/// 插在中间、片段出现在多处）都不是纯追加。
pub fn journal_edit_appends(disk: &str, old: Option<&str>, new: &str) -> bool {
    let result = match old {
        None | Some("") => new.to_owned(),
        Some(old) if old == disk => new.to_owned(),
        Some(old) => {
            let Some(at) = unique_occurrence(disk, old) else {
                return false;
            };
            let mut result = String::with_capacity(disk.len() + new.len());
            result.push_str(&disk[..at]);
            result.push_str(new);
            result.push_str(&disk[at + old.len()..]);
            result
        }
    };
    result.starts_with(disk)
}

/// `needle` 在 `haystack` 中唯一出现（按字节位置，重叠的出现也算）时的起始位置；
/// 没出现或出现多次时为 `None`（无法确定替换结果）。
fn unique_occurrence(haystack: &str, needle: &str) -> Option<usize> {
    if needle.is_empty() {
        return None;
    }
    let bytes = haystack.as_bytes();
    let needle = needle.as_bytes();
    let mut found: Option<usize> = None;
    let mut at = 0usize;
    while at + needle.len() <= bytes.len() {
        if &bytes[at..at + needle.len()] == needle {
            if found.is_some() {
                return None;
            }
            found = Some(at);
        }
        at += 1;
    }
    found
}

/// 把路径规范化为绝对路径：相对路径按 `base` 解析；含 `..` 时拒绝；已存在的最长前缀解析符号链接，
/// 不存在的尾部按字面拼接。末段是指向不存在位置的符号链接时拒绝（写入会落到链接目标）。
pub fn resolve(path: &Path, base: &Path) -> Result<PathBuf, String> {
    let shown = path.display();
    if path.as_os_str().is_empty() {
        return Err("路径为空".to_owned());
    }
    if path.components().any(|c| c == Component::ParentDir) {
        return Err(format!("路径不能包含「..」：{shown}"));
    }
    let joined = if path.is_absolute() {
        path.to_owned()
    } else {
        base.join(path)
    };
    let mut existing: PathBuf = joined
        .components()
        .filter(|c| *c != Component::CurDir)
        .collect();
    let mut tail = Vec::new();
    loop {
        if std::fs::symlink_metadata(&existing).is_ok() {
            break;
        }
        match existing.file_name() {
            Some(name) => {
                tail.push(name.to_owned());
                existing.pop();
            }
            None => return Err(format!("路径无法解析：{shown}")),
        }
    }
    let mut real = existing
        .canonicalize()
        .map_err(|_| format!("路径指向不存在的位置（符号链接失效）：{shown}"))?;
    for part in tail.into_iter().rev() {
        real.push(part);
    }
    Ok(real)
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::os::unix::fs::symlink;

    use super::*;

    struct Env {
        _dir: tempfile::TempDir,
        ws: PathBuf,
        role: PathBuf,
        outside: PathBuf,
    }

    /// 工作空间 ws/，岗位文件夹 ws/Teamspace/前端/，引用 ws/docs/spec.md 与目录 ws/assets/，
    /// 范围外的 outside/。
    fn env() -> Env {
        let dir = tempfile::tempdir().unwrap();
        let ws = dir.path().join("ws");
        let role = ws.join("Teamspace/前端");
        fs::create_dir_all(role.join("过程文件")).unwrap();
        fs::write(role.join(BRIEF_FILE), "x").unwrap();
        fs::write(role.join(JOURNAL_FILE), "x").unwrap();
        fs::create_dir_all(ws.join("docs")).unwrap();
        fs::write(ws.join("docs/spec.md"), "x").unwrap();
        fs::write(ws.join("docs/other.md"), "x").unwrap();
        fs::create_dir_all(ws.join("assets/img")).unwrap();
        let outside = dir.path().join("outside");
        fs::create_dir_all(&outside).unwrap();
        Env {
            ws: ws.canonicalize().unwrap(),
            role: role.canonicalize().unwrap(),
            outside: outside.canonicalize().unwrap(),
            _dir: dir,
        }
    }

    fn in_place(e: &Env) -> Scope {
        Scope::new(
            &e.ws,
            RunIsolation::InPlace,
            std::slice::from_ref(&e.role),
            &[e.ws.join("docs/spec.md"), e.ws.join("assets")],
        )
    }

    fn s(p: &Path) -> String {
        p.display().to_string()
    }

    #[test]
    fn read_and_search_inside_the_workspace_are_allowed_outside_ask() {
        // D18：工作空间内（含岗位文件夹、相对路径）与没有路径的读取、搜索自动允许。
        let e = env();
        let scope = in_place(&e);
        for kind in ["read", "search"] {
            for paths in [
                vec![s(&e.ws.join("docs/other.md"))],
                vec![s(&e.role.join("工作日志.md")), "docs".to_owned()],
                vec![".".to_owned()],
                vec![s(&e.ws.join("不存在/新文件.md"))],
                vec![],
            ] {
                assert!(
                    matches!(scope.decide(Some(kind), &paths), Verdict::Allow(_)),
                    "{kind} {paths:?}"
                );
            }
        }
        // 越界、`..` 穿越、指向外部的符号链接：转交 Human，原因带上路径。
        symlink(&e.outside, e.ws.join("外链")).unwrap();
        for kind in ["read", "search"] {
            for bad in [
                s(&e.outside.join("x")),
                "/etc/hosts".to_owned(),
                "docs/../../outside/x".to_owned(),
                s(&e.ws.join("外链/secret.txt")),
            ] {
                let paths = vec![s(&e.ws.join("docs/spec.md")), bad.clone()];
                let Verdict::Ask(reason) = scope.decide(Some(kind), &paths) else {
                    panic!("{kind} {bad} should need confirmation");
                };
                assert_eq!(reason, format!("读取工作空间之外的文件需要确认：{bad}"));
            }
        }
        // fs/read_text_file：工作空间外的文件提示先发起读取权限请求。
        let err = scope.can_read(&s(&e.outside.join("x"))).unwrap_err();
        assert!(err.contains("需要先发起读取权限请求"), "{err}");
        assert!(scope.can_read(&s(&e.ws.join("外链/secret.txt"))).is_err());
    }

    #[test]
    fn worktree_reads_cover_the_workspace_too() {
        let e = env();
        let wt = e.outside.join("wt");
        fs::create_dir_all(&wt).unwrap();
        let scope = Scope::with_workspace(
            &wt,
            &e.ws,
            RunIsolation::Worktree,
            std::slice::from_ref(&e.role),
            &[],
        );
        assert!(scope.can_read(&s(&e.ws.join("docs/other.md"))).is_ok());
        assert!(matches!(
            scope.decide(Some("read"), &[s(&e.ws.join("docs/other.md"))]),
            Verdict::Allow(_)
        ));
        assert!(matches!(
            scope.decide(Some("search"), &[s(&e.outside.join("elsewhere"))]),
            Verdict::Ask(_)
        ));
        // 读取范围扩大到工作空间，不影响写入范围。
        assert!(scope.can_write(&s(&e.ws.join("docs/other.md"))).is_err());
    }

    #[test]
    fn in_place_edits_are_limited_to_role_folder_and_refs() {
        let e = env();
        let scope = in_place(&e);
        let allowed = [
            e.role.join("过程文件/草稿.md"),
            e.role.join("过程文件/新目录/深/a.md"),
            e.ws.join("docs/spec.md"),
            e.ws.join("assets/img/new.png"),
        ];
        for path in &allowed {
            assert!(
                matches!(scope.decide(Some("edit"), &[s(path)]), Verdict::Allow(_)),
                "{path:?}"
            );
            assert!(scope.can_write(&s(path)).is_ok(), "{path:?}");
        }
        // 相对路径按执行目录（工作空间）解析。
        assert!(scope.can_write("Teamspace/前端/过程文件/x.md").is_ok());
        assert!(scope.can_write("./docs/spec.md").is_ok());

        for path in [
            e.ws.join("docs/other.md"),
            e.ws.join("src/main.rs"),
            e.ws.join("Teamspace/后端/过程文件/x.md"),
            e.outside.join("x.md"),
        ] {
            let Verdict::Ask(reason) = scope.decide(Some("edit"), &[s(&path)]) else {
                panic!("{path:?} should need confirmation");
            };
            assert!(reason.contains("之外"), "{reason}");
        }
        // 只要有一个路径越界就要确认。
        assert!(matches!(
            scope.decide(
                Some("edit"),
                &[s(&e.role.join("a.md")), s(&e.ws.join("b.md"))]
            ),
            Verdict::Ask(_)
        ));
        assert!(matches!(scope.decide(Some("edit"), &[]), Verdict::Ask(_)));
    }

    #[test]
    fn protected_files_and_parent_components_are_rejected() {
        let e = env();
        let scope = in_place(&e);
        let err = scope.can_write(&s(&e.role.join(BRIEF_FILE))).unwrap_err();
        assert!(err.contains("由 Teamspace 维护"), "{err}");
        // 工作日志不能不看内容直接写（只允许追加，见 journal_edits_are_allowed_only_when_appending）。
        let err = scope.can_write(&s(&e.role.join(JOURNAL_FILE))).unwrap_err();
        assert!(err.contains("只允许在末尾追加"), "{err}");
        for path in [
            format!("{}/过程文件/../../../etc/x", s(&e.role)),
            "Teamspace/前端/../后端/x.md".to_owned(),
            "..".to_owned(),
        ] {
            let err = scope.can_write(&path).unwrap_err();
            assert!(err.contains(".."), "{err}");
            assert!(matches!(
                scope.decide(Some("edit"), &[path]),
                Verdict::Ask(_)
            ));
        }
        assert!(scope.can_write("").is_err());
    }

    fn diff(path: &Path, old: Option<&str>, new: &str) -> EditPreview {
        EditPreview {
            path: s(path),
            old_text: old.map(str::to_owned),
            new_text: new.to_owned(),
        }
    }

    #[test]
    fn journal_edits_are_allowed_only_when_appending() {
        let e = env();
        let journal = e.role.join(JOURNAL_FILE);
        let disk = "# 工作日志\n\n- 2026-10-14 10:00:00 UTC+08:00 · FE-001 · 开始第 1 次执行\n";
        fs::write(&journal, disk).unwrap();
        let scope = in_place(&e);
        assert_eq!(
            scope.write_target(&s(&journal)),
            Ok(WriteTarget::Journal(journal.clone()))
        );
        assert_eq!(
            scope.write_target("Teamspace/前端/工作日志.md"),
            Ok(WriteTarget::Journal(journal.clone()))
        );
        let paths = [s(&journal)];
        let appended = format!("{disk}- 2026-10-14 10:01:00 UTC+08:00 · FE-001 · 进展\n");
        let last = "- 2026-10-14 10:00:00 UTC+08:00 · FE-001 · 开始第 1 次执行\n";

        // 纯追加：整文件（oldText 为空或等于现有内容）与末尾片段替换两种写法。
        for edit in [
            diff(&journal, None, &appended),
            diff(&journal, Some(""), &appended),
            diff(&journal, Some(disk), &appended),
            diff(&journal, Some(last), &format!("{last}- 新进展\n")),
        ] {
            let verdict = scope.decide_with(Some("edit"), &paths, std::slice::from_ref(&edit));
            let Verdict::Allow(reason) = verdict else {
                panic!("{edit:?} should be allowed: {verdict:?}");
            };
            assert!(reason.contains("追加"), "{reason}");
        }
        // 相对路径的 diff 同样识别。
        assert!(matches!(
            scope.decide_with(
                Some("edit"),
                &["Teamspace/前端/工作日志.md".to_owned()],
                &[EditPreview {
                    path: "Teamspace/前端/工作日志.md".into(),
                    old_text: None,
                    new_text: appended.clone(),
                }]
            ),
            Verdict::Allow(_)
        ));

        // 覆盖、截断、删改、片段不在末尾、片段被改动、没有 diff：转交 Human。
        let ask = |edits: &[EditPreview]| {
            let Verdict::Ask(reason) = scope.decide_with(Some("edit"), &paths, edits) else {
                panic!("{edits:?} should need confirmation");
            };
            assert!(reason.contains("只允许在末尾追加"), "{reason}");
        };
        ask(&[diff(&journal, None, "# 新日志\n")]);
        ask(&[diff(&journal, Some(disk), "# 工作日志\n")]);
        ask(&[diff(
            &journal,
            Some(disk),
            &appended.replace("开始", "结束"),
        )]);
        ask(&[diff(&journal, Some("# 工作日志"), "# 工作日志（改）")]);
        ask(&[diff(
            &journal,
            Some("# 工作日志\n"),
            "# 工作日志\n- 插在中间\n",
        )]);
        ask(&[diff(&journal, Some(last), "- 改写最后一行\n")]);
        ask(&[]);
        ask(&[diff(&e.role.join("过程文件/x.md"), None, &appended)]);
        // 同一请求里工作日志追加与范围内文件一起编辑：原因写明含追加。
        let Verdict::Allow(reason) = scope.decide_with(
            Some("edit"),
            &[s(&journal), s(&e.role.join("过程文件/笔记.md"))],
            &[diff(&journal, None, &appended)],
        ) else {
            panic!("mixed append should be allowed");
        };
        assert!(reason.contains("追加进展并编辑"), "{reason}");
        // 不带 diff 的 decide 一律转交。
        assert!(matches!(
            scope.decide(Some("edit"), &paths),
            Verdict::Ask(_)
        ));
        // 同一调用里有一个 diff 不是追加就转交。
        ask(&[
            diff(&journal, None, &appended),
            diff(&journal, None, "# 新日志\n"),
        ]);

        // 岗位说明仍受保护，即使 diff 看起来是追加。
        let brief = e.role.join(BRIEF_FILE);
        let Verdict::Ask(reason) = scope.decide_with(
            Some("edit"),
            &[s(&brief)],
            &[diff(&brief, Some("x"), "x追加")],
        ) else {
            panic!("brief must stay protected");
        };
        assert!(reason.contains("由 Teamspace 维护"), "{reason}");

        // 其他岗位的工作日志不在本岗位写入范围内。
        let other = e.ws.join("Teamspace/后端").join(JOURNAL_FILE);
        fs::create_dir_all(other.parent().unwrap()).unwrap();
        fs::write(&other, disk).unwrap();
        let Verdict::Ask(reason) = scope.decide_with(
            Some("edit"),
            &[s(&other)],
            &[diff(&other, Some(disk), &appended)],
        ) else {
            panic!("other role's journal is out of scope");
        };
        assert!(reason.contains("之外"), "{reason}");
        assert!(scope.write_target(&s(&other)).is_err());

        // 不存在的工作日志：无法判断，转交。
        fs::remove_file(&journal).unwrap();
        ask(&[diff(&journal, None, "- x\n")]);
    }

    #[test]
    fn journal_edit_appends_cases() {
        assert!(journal_edit_appends("a\n", None, "a\nb\n"));
        assert!(journal_edit_appends("a\n", Some("a\n"), "a\n"));
        assert!(journal_edit_appends("x\nb\n", Some("b\n"), "b\nc\n"));
        // 片段出现多次（replace_all 会改动前面的行）。
        assert!(!journal_edit_appends("b\nb\n", Some("b\n"), "b\nc\n"));
        assert!(!journal_edit_appends("a\nb\n", Some("a\n"), "a\nc\n"));
        assert!(!journal_edit_appends("a\n", None, "b\n"));
        assert!(!journal_edit_appends("a\n", None, ""));
    }

    /// M3-11 自举复现：Claude Code 用编辑工具改工作日志时，diff 只带文件末尾的片段
    /// （`oldText` 不含行尾换行，`newText` 是「这一段 + 追加的内容」），内核原先判断不了，
    /// 只能转交 Human。这里按「改完之后的内容」判定：已有内容一字未动、只在末尾追加就允许。
    #[test]
    fn journal_appends_recognize_agent_edit_styles() {
        let disk = "# 工作日志\n\n- 2026-10-14 10:00:00 UTC+08:00 · FE-001 · 开始第 1 次执行\n";
        let tail = "- 2026-10-14 10:00:00 UTC+08:00 · FE-001 · 开始第 1 次执行";

        // Claude Code 的 Edit：oldText 是文件末尾的一段（不含行尾换行），newText 是这一段加追加内容。
        assert!(journal_edit_appends(
            disk,
            Some(tail),
            &format!("{tail}\n- 2026-10-14 10:01:00 UTC+08:00 · FE-001 · 进展")
        ));
        // 末尾片段带行尾换行时同样成立。
        assert!(journal_edit_appends(
            disk,
            Some(&format!("{tail}\n")),
            &format!("{tail}\n- 2026-10-14 10:01:00 UTC+08:00 · FE-001 · 进展\n")
        ));
        // Claude Code 的 Write：oldText 为空，newText 是整个新文件。
        let written = format!("{disk}- 2026-10-14 10:01:00 UTC+08:00 · FE-001 · 进展\n");
        assert!(journal_edit_appends(disk, None, &written));
        assert!(journal_edit_appends(disk, Some(""), &written));
        // MiniMax Code：oldText 是整个现有内容，newText 是现有内容加追加。
        assert!(journal_edit_appends(disk, Some(disk), &written));

        // 改动已有内容的一律不是纯追加。
        assert!(!journal_edit_appends(disk, Some(tail), "- 改写最后一行\n"));
        assert!(!journal_edit_appends(disk, Some(disk), "# 工作日志\n"));
        assert!(!journal_edit_appends(disk, None, "# 工作日志\n"));
        // 片段出现在多处：无法确定替换结果。
        assert!(!journal_edit_appends("x\nx\n", Some("x\n"), "x\ny\n"));
        // 片段不在末尾：改的是中间内容。
        assert!(!journal_edit_appends(
            "# 头\n- 甲\n- 乙\n",
            Some("- 甲\n"),
            "- 甲改\n- 乙\n"
        ));
    }

    /// 片段唯一出现但不在末尾时，改完之后的内容不再以现有内容开头 → 转交 Human。
    #[test]
    fn journal_appends_reject_edits_in_the_middle() {
        let disk = "# 头\n- 甲\n- 乙\n";
        assert!(!journal_edit_appends(
            disk,
            Some("- 甲\n"),
            "- 甲改\n- 乙\n"
        ));
        // 删掉末尾的一行：截断，不是追加。
        assert!(!journal_edit_appends(disk, Some("- 乙\n"), ""));
        // 末尾片段被换成别的内容。
        assert!(!journal_edit_appends(disk, Some("- 乙\n"), "- 丙\n"));
    }

    /// 工作日志的权限判断（`Scope::decide_with`）按上面这套规则放行：Claude Code 风格的片段编辑
    /// 与 MiniMax Code 风格的整文件写入都自动允许，改动已有内容仍然转交 Human。
    #[test]
    fn journal_edits_follow_the_append_rules() {
        let e = env();
        let journal = e.role.join(JOURNAL_FILE);
        let disk = "# 工作日志\n\n- 2026-10-14 10:00:00 UTC+08:00 · FE-001 · 开始第 1 次执行\n";
        fs::write(&journal, disk).unwrap();
        let scope = in_place(&e);
        let paths = [s(&journal)];
        let tail = "- 2026-10-14 10:00:00 UTC+08:00 · FE-001 · 开始第 1 次执行";
        let appended = format!("{disk}- 2026-10-14 10:01:00 UTC+08:00 · FE-001 · 进展\n");

        // 片段编辑（Claude Code）：允许。
        let Verdict::Allow(reason) = scope.decide_with(
            Some("edit"),
            &paths,
            &[diff(
                &journal,
                Some(tail),
                &format!("{tail}\n- 2026-10-14 10:01:00 UTC+08:00 · FE-001 · 进展"),
            )],
        ) else {
            panic!("片段追加应当自动允许");
        };
        assert!(reason.contains("追加"), "{reason}");

        // 整文件写入（oldText 为空）：允许。
        assert!(matches!(
            scope.decide_with(Some("edit"), &paths, &[diff(&journal, None, &appended)]),
            Verdict::Allow(_)
        ));

        // 覆盖已有内容：转交 Human。
        let Verdict::Ask(reason) = scope.decide_with(
            Some("edit"),
            &paths,
            &[diff(&journal, Some(tail), "- 改写\n")],
        ) else {
            panic!("改写已有内容需要确认");
        };
        assert!(reason.contains("只允许在末尾追加"), "{reason}");
    }

    #[test]
    fn symlinks_escaping_the_scope_are_not_trusted() {
        let e = env();
        symlink(&e.outside, e.role.join("过程文件/逃逸")).unwrap();
        symlink(e.outside.join("missing"), e.role.join("过程文件/坏链接")).unwrap();
        symlink(e.role.join("过程文件"), e.outside.join("进入")).unwrap();
        let scope = in_place(&e);
        let err = scope
            .can_write(&s(&e.role.join("过程文件/逃逸/x.md")))
            .unwrap_err();
        assert!(err.contains("之外"), "{err}");
        assert!(
            scope
                .can_read(&s(&e.role.join("过程文件/逃逸/x.md")))
                .is_err()
        );
        let err = scope
            .can_write(&s(&e.role.join("过程文件/坏链接")))
            .unwrap_err();
        assert!(err.contains("符号链接"), "{err}");
        // 从范围外经链接指回岗位文件夹：按真实位置判断，允许。
        assert!(scope.can_write(&s(&e.outside.join("进入/x.md"))).is_ok());
    }

    #[test]
    fn refs_linking_outside_the_workspace_are_not_writable() {
        let e = env();
        let secret = e.outside.join("secret.txt");
        fs::write(&secret, "x").unwrap();
        symlink(&secret, e.ws.join("notes.md")).unwrap();
        symlink(e.ws.join("docs/spec.md"), e.ws.join("inner.md")).unwrap();
        let scope = Scope::new(
            &e.ws,
            RunIsolation::InPlace,
            std::slice::from_ref(&e.role),
            &[e.ws.join("notes.md"), e.ws.join("inner.md")],
        );
        assert!(scope.can_write(&s(&secret)).is_err());
        assert!(scope.can_write(&s(&e.ws.join("notes.md"))).is_err());
        // 指向工作空间内的链接按真实路径计入。
        assert!(scope.can_write(&s(&e.ws.join("docs/spec.md"))).is_ok());
        assert!(scope.can_read(&s(&secret)).is_err());
    }

    #[test]
    fn worktree_allows_edits_inside_but_not_git_or_outside() {
        let e = env();
        let wt = e.outside.join("wt");
        fs::create_dir_all(wt.join("src")).unwrap();
        fs::write(wt.join(".git"), "gitdir: x").unwrap();
        let scope = Scope::new(
            &wt,
            RunIsolation::Worktree,
            &[e.role.clone(), wt.join("Teamspace/前端")],
            &[],
        );
        assert!(scope.can_write(&s(&wt.join("src/main.rs"))).is_ok());
        assert!(scope.can_write("src/new/file.rs").is_ok());
        // 工作空间中的岗位文件夹也可写（过程文件）。
        assert!(scope.can_write(&s(&e.role.join("过程文件/x.md"))).is_ok());
        // worktree 中岗位文件夹的岗位说明同样受保护。
        assert!(
            scope
                .can_write(&s(&wt.join("Teamspace/前端").join(BRIEF_FILE)))
                .is_err()
        );
        let err = scope.can_write(&s(&wt.join(".git"))).unwrap_err();
        assert!(err.contains("Git"), "{err}");
        assert!(scope.can_write(&s(&e.ws.join("src/main.rs"))).is_err());
        assert!(matches!(
            scope.decide(Some("edit"), &[s(&wt.join("src/main.rs"))]),
            Verdict::Allow(_)
        ));
        // 读取：worktree 内与岗位文件夹可读，其余不行。
        assert!(scope.can_read(&s(&wt.join("src/main.rs"))).is_ok());
        assert!(scope.can_read(&s(&e.role.join("工作日志.md"))).is_ok());
        assert!(scope.can_read(&s(&e.ws.join("docs/other.md"))).is_err());
    }

    #[test]
    fn other_kinds_need_confirmation() {
        let e = env();
        let scope = in_place(&e);
        let inside = vec![s(&e.role.join("a.md"))];
        for (kind, needle) in [
            (Some("execute"), "执行命令"),
            (Some("delete"), "删除"),
            (Some("move"), "移动"),
            (Some("fetch"), "网络"),
            (Some("switch_mode"), "模式"),
            (Some("think"), "think"),
            (Some("other"), "other"),
            (None, "没有说明"),
        ] {
            let Verdict::Ask(reason) = scope.decide(kind, &inside) else {
                panic!("{kind:?} should need confirmation");
            };
            assert!(reason.contains(needle), "{kind:?}: {reason}");
        }
    }

    #[test]
    fn reads_are_limited_for_fs_requests() {
        let e = env();
        let scope = in_place(&e);
        assert!(scope.can_read(&s(&e.ws.join("docs/other.md"))).is_ok());
        assert!(scope.can_read(&s(&e.outside.join("x"))).is_err());
        assert_eq!(
            scope.can_read("docs/spec.md").unwrap(),
            e.ws.join("docs/spec.md")
        );
    }
}
