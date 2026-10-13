//! 执行时的权限规则（D15），纯函数，不访问数据库。
//!
//! - 工具调用类型为读取（read）、搜索（search）→ 自动允许；
//! - 编辑（edit）且所有路径都可写 → 自动允许；可写范围：
//!   - 本岗位文件夹（工作空间中的 `Teamspace/<岗位>/`）；
//!   - 任务引用的文件（引用的是目录时含其中的文件）；
//!   - Git worktree 执行时，worktree 内的任何位置（`.git` 除外）；
//!   - 例外：岗位文件夹中的 `岗位说明.md` 与 `工作日志.md` 由 Teamspace 维护，不自动允许；
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
    pub fn new(
        exec_dir: &Path,
        isolation: RunIsolation,
        role_folders: &[PathBuf],
        refs: &[PathBuf],
    ) -> Self {
        let exec_dir = resolve(exec_dir, Path::new("/")).unwrap_or_else(|_| exec_dir.to_owned());
        let canon = |p: &PathBuf| resolve(p, &exec_dir).ok();
        let role_folders: Vec<PathBuf> = role_folders.iter().filter_map(canon).collect();
        let protected = role_folders
            .iter()
            .flat_map(|dir| [dir.join(BRIEF_FILE), dir.join(JOURNAL_FILE)])
            .collect();
        Self {
            refs: refs.iter().filter_map(canon).collect(),
            exec_dir,
            isolation,
            role_folders,
            protected,
        }
    }

    pub fn exec_dir(&self) -> &Path {
        &self.exec_dir
    }

    /// 可否读取：执行目录、岗位文件夹或引用文件之内。返回规范化后的路径或中文原因。
    pub fn can_read(&self, path: &str) -> Result<PathBuf, String> {
        let real = resolve(Path::new(path), &self.exec_dir)?;
        if real.starts_with(&self.exec_dir)
            || self.role_folders.iter().any(|d| real.starts_with(d))
            || self.refs.iter().any(|r| real.starts_with(r))
        {
            Ok(real)
        } else {
            Err(format!("读取执行目录与岗位文件夹之外的文件：{path}"))
        }
    }

    /// 可否自动写入（见模块说明）。返回规范化后的路径或中文原因。
    pub fn can_write(&self, path: &str) -> Result<PathBuf, String> {
        let real = resolve(Path::new(path), &self.exec_dir)?;
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
    pub fn decide(&self, kind: Option<&str>, paths: &[String]) -> Verdict {
        match kind {
            Some("read") => Verdict::Allow("读取文件，自动允许".to_owned()),
            Some("search") => Verdict::Allow("搜索，自动允许".to_owned()),
            Some("edit") => {
                if paths.is_empty() {
                    return Verdict::Ask("编辑操作没有给出文件路径，需要确认".to_owned());
                }
                for path in paths {
                    if let Err(reason) = self.can_write(path) {
                        return Verdict::Ask(reason);
                    }
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
    fn read_and_search_are_always_allowed() {
        let e = env();
        let scope = in_place(&e);
        for kind in ["read", "search"] {
            assert!(matches!(
                scope.decide(Some(kind), &[s(&e.outside.join("x"))]),
                Verdict::Allow(_)
            ));
        }
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
        for name in [BRIEF_FILE, JOURNAL_FILE] {
            let err = scope.can_write(&s(&e.role.join(name))).unwrap_err();
            assert!(err.contains("由 Teamspace 维护"), "{err}");
        }
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
