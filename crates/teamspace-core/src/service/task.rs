//! 任务：编号、引用、依赖与环路检测、就绪条件、状态流转。

use std::collections::{HashMap, HashSet};
use std::path::Path;

use rusqlite::{Connection, OptionalExtension, params};
use serde_json::json;

use super::event::{self, NewEvent};
use super::{MAX_LONG_TEXT, MAX_REASON, MAX_TITLE, role, team, text};
use crate::error::{Error, Result};
use crate::model::{
    BoardColumn, ChangeTaskStatusInput, CreateTaskInput, EventKind, FileRef, FileRefView,
    StatusCount, Task, TaskBoard, TaskBoardQuery, TaskDetail, TaskStatus, TaskSummary,
    UpdateTaskInput,
};
use crate::store::{enum_col, enum_str, new_id, non_blank, now};

/// 每个任务的文件引用上限。
pub(crate) const MAX_FILE_REFS: usize = 50;
/// 每个任务的依赖上限。
pub(crate) const MAX_DEPENDENCIES: usize = 20;

// ---------------------------------------------------------------- 读取

const COLS: &str = "id, team_id, key, title, description, status, owner_role_id, blocked_reason, created_at, updated_at";

/// 任务（所属团队须未归档）；否则 not_found。
pub(crate) fn get(conn: &Connection, id: &str) -> Result<Task> {
    let mut task = conn
        .query_row(
            &format!(
                "SELECT {COLS} FROM tasks WHERE id = ?1
                 AND team_id IN (SELECT id FROM teams WHERE archived_at IS NULL)"
            ),
            params![id],
            |row| {
                Ok(Task {
                    id: row.get(0)?,
                    team_id: row.get(1)?,
                    key: row.get(2)?,
                    title: row.get(3)?,
                    description: row.get(4)?,
                    status: enum_col(row, 5)?,
                    owner_role_id: row.get(6)?,
                    blocked_reason: row.get(7)?,
                    created_at: row.get(8)?,
                    updated_at: row.get(9)?,
                    file_refs: Vec::new(),
                    depends_on: Vec::new(),
                })
            },
        )
        .optional()?
        .ok_or_else(|| Error::not_found("任务不存在"))?;
    let mut stmt = conn
        .prepare("SELECT path, anchor FROM task_file_refs WHERE task_id = ?1 ORDER BY position")?;
    task.file_refs = stmt
        .query_map(params![id], |row| {
            Ok(FileRef {
                path: row.get(0)?,
                anchor: row.get(1)?,
            })
        })?
        .collect::<rusqlite::Result<_>>()?;
    let mut stmt = conn.prepare(
        "SELECT depends_on_id FROM task_dependencies WHERE task_id = ?1 ORDER BY position",
    )?;
    task.depends_on = stmt
        .query_map(params![id], |row| row.get(0))?
        .collect::<rusqlite::Result<_>>()?;
    Ok(task)
}

const SUMMARY_SQL: &str = "
SELECT t.id, t.key, t.title, t.status, t.blocked_reason, t.updated_at,
       r.id, r.name, r.code, r.task_prefix, r.archived_at IS NOT NULL,
       (SELECT COUNT(*) FROM task_file_refs f WHERE f.task_id = t.id),
       (SELECT COUNT(*) FROM task_dependencies d WHERE d.task_id = t.id),
       (SELECT COUNT(*) FROM task_dependencies d JOIN tasks u ON u.id = d.depends_on_id
         WHERE d.task_id = t.id AND u.status <> 'done')
FROM tasks t JOIN roles r ON r.id = t.owner_role_id";

fn summaries(conn: &Connection, filter: &str, param: &str) -> Result<Vec<TaskSummary>> {
    let mut stmt = conn.prepare(&format!(
        "{SUMMARY_SQL} WHERE {filter} ORDER BY t.updated_at DESC, t.id DESC"
    ))?;
    let rows = stmt.query_map(params![param], |row| {
        Ok(TaskSummary {
            id: row.get(0)?,
            key: row.get(1)?,
            title: row.get(2)?,
            status: enum_col(row, 3)?,
            blocked_reason: row.get(4)?,
            updated_at: row.get(5)?,
            owner: crate::model::RoleRef {
                id: row.get(6)?,
                name: row.get(7)?,
                code: row.get(8)?,
                task_prefix: row.get(9)?,
                archived: row.get(10)?,
            },
            file_ref_count: row.get(11)?,
            dependency_count: row.get(12)?,
            open_dependency_count: row.get(13)?,
        })
    })?;
    Ok(rows.collect::<rusqlite::Result<_>>()?)
}

/// 各状态任务数，按标准顺序，含全部状态。
pub(crate) fn counts(conn: &Connection, team_id: &str) -> Result<Vec<StatusCount>> {
    let mut stmt =
        conn.prepare("SELECT status, COUNT(*) FROM tasks WHERE team_id = ?1 GROUP BY status")?;
    let found: HashMap<TaskStatus, u32> = stmt
        .query_map(params![team_id], |row| Ok((enum_col(row, 0)?, row.get(1)?)))?
        .collect::<rusqlite::Result<_>>()?;
    Ok(TaskStatus::ALL
        .into_iter()
        .map(|status| StatusCount {
            status,
            label: status.label().to_owned(),
            count: found.get(&status).copied().unwrap_or(0),
        })
        .collect())
}

/// 进行中的任务（执行中、受阻、待验收）。
pub(crate) fn active(conn: &Connection, team_id: &str) -> Result<Vec<TaskSummary>> {
    summaries(
        conn,
        "t.team_id = ?1 AND t.status IN ('doing', 'blocked', 'review')",
        team_id,
    )
}

pub(crate) fn board(conn: &Connection, query: &TaskBoardQuery) -> Result<TaskBoard> {
    team::get_active(conn, &query.team_id)?;
    let include_cancelled = query.include_cancelled.unwrap_or(false);
    let mut all = summaries(conn, "t.team_id = ?1", &query.team_id)?;
    let columns = TaskStatus::ALL
        .into_iter()
        .filter(|s| include_cancelled || *s != TaskStatus::Cancelled)
        .map(|status| {
            let (tasks, rest): (Vec<_>, Vec<_>) = all.drain(..).partition(|t| t.status == status);
            all = rest;
            BoardColumn {
                status,
                label: status.label().to_owned(),
                tasks,
            }
        })
        .collect();
    Ok(TaskBoard {
        team_id: query.team_id.clone(),
        columns,
    })
}

/// 在锁外预先完成的文件存在性检查结果（避免在全局锁和写事务内做文件系统 I/O）。
///
/// 未检查过的路径视为不存在。
pub(crate) struct FileCheck {
    workspace_ok: bool,
    existing: HashSet<String>,
}

impl FileCheck {
    /// 检查工作区目录与各引用文件是否存在。
    pub(crate) fn scan(workspace: &Path, refs: &[FileRef]) -> Self {
        let workspace_ok = workspace.is_dir();
        let existing = if workspace_ok {
            refs.iter()
                .filter(|r| workspace.join(&r.path).exists())
                .map(|r| r.path.clone())
                .collect()
        } else {
            HashSet::new()
        };
        Self {
            workspace_ok,
            existing,
        }
    }

    fn exists(&self, path: &str) -> bool {
        self.workspace_ok && self.existing.contains(path)
    }
}

/// 读取任务的工作区路径与文件引用（用于在锁外做 [`FileCheck`]）。
pub(crate) fn workspace_and_refs(conn: &Connection, id: &str) -> Result<(String, Vec<FileRef>)> {
    let task = get(conn, id)?;
    let workspace = team::get_active(conn, &task.team_id)?.workspace_path;
    Ok((workspace, task.file_refs))
}

/// 需要满足就绪条件才能进入的状态。
fn needs_readiness(to: TaskStatus) -> bool {
    matches!(to, TaskStatus::Ready | TaskStatus::Doing)
}

pub(crate) fn detail(conn: &Connection, id: &str, files: &FileCheck) -> Result<TaskDetail> {
    let task = get(conn, id)?;
    let problems = readiness(
        conn,
        &task.owner_role_id,
        &task.file_refs,
        &task.depends_on,
        files,
    )?;
    build_detail(conn, task, problems, files)
}

/// 用已算好的就绪问题组装详情（不再重复计算就绪条件，也不做文件 I/O）。
fn build_detail(
    conn: &Connection,
    task: Task,
    readiness: Vec<String>,
    files: &FileCheck,
) -> Result<TaskDetail> {
    let allowed_transitions = task
        .status
        .next()
        .iter()
        .copied()
        .filter(|to| !needs_readiness(*to) || readiness.is_empty())
        .collect();
    let file_refs = task
        .file_refs
        .iter()
        .map(|r| FileRefView {
            path: r.path.clone(),
            anchor: r.anchor.clone(),
            exists: files.exists(&r.path),
        })
        .collect();
    let id = task.id.as_str();
    Ok(TaskDetail {
        owner: role::role_ref(conn, &task.owner_role_id)?,
        file_refs,
        dependencies: summaries(
            conn,
            "t.id IN (SELECT depends_on_id FROM task_dependencies WHERE task_id = ?1)",
            id,
        )?,
        dependents: summaries(
            conn,
            "t.id IN (SELECT task_id FROM task_dependencies WHERE depends_on_id = ?1)",
            id,
        )?,
        allowed_transitions,
        readiness,
        events: event::for_task(conn, id)?,
        task,
    })
}

// ---------------------------------------------------------------- 规则

/// 规范化文件引用：相对工作区、`/` 分隔、禁止绝对路径和 `..`；`a.md#x` 拆出锚点；去重。
/// 最多 [`MAX_FILE_REFS`] 个。
pub(crate) fn normalize_refs(refs: Vec<FileRef>) -> Result<Vec<FileRef>> {
    if refs.len() > MAX_FILE_REFS {
        return Err(Error::invalid(format!("文件引用最多 {MAX_FILE_REFS} 个")));
    }
    let mut out: Vec<FileRef> = Vec::with_capacity(refs.len());
    let mut seen: HashSet<(String, Option<String>)> = HashSet::with_capacity(refs.len());
    for r in refs {
        let raw = r.path.trim().replace('\\', "/");
        let (raw_path, inline_anchor) = match raw.split_once('#') {
            Some((p, a)) => (p.to_owned(), Some(a.to_owned())),
            None => (raw, None),
        };
        let anchor = non_blank(r.anchor.or(inline_anchor))
            .map(|a| a.trim_start_matches('#').to_owned())
            .filter(|a| !a.is_empty());
        let bytes = raw_path.as_bytes();
        let is_drive = bytes.len() >= 2 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':';
        if raw_path.starts_with('/') || is_drive {
            return Err(Error::invalid(format!(
                "文件引用必须是相对工作区的路径：{raw_path}"
            )));
        }
        let mut parts = Vec::new();
        for part in raw_path.split('/') {
            match part {
                "" | "." => {}
                ".." => {
                    return Err(Error::invalid(format!(
                        "文件引用不能包含「..」：{raw_path}"
                    )));
                }
                p => parts.push(p),
            }
        }
        if parts.is_empty() {
            return Err(Error::invalid("文件引用的路径不能为空"));
        }
        let normalized = FileRef {
            path: parts.join("/"),
            anchor,
        };
        if seen.insert((normalized.path.clone(), normalized.anchor.clone())) {
            out.push(normalized);
        }
    }
    Ok(out)
}

/// 进入 ready / doing 的未满足条件（中文）；为空表示就绪。文件存在性来自预先完成的 [`FileCheck`]。
pub(crate) fn readiness(
    conn: &Connection,
    owner_role_id: &str,
    refs: &[FileRef],
    depends_on: &[String],
    files: &FileCheck,
) -> Result<Vec<String>> {
    if !files.workspace_ok {
        return Ok(vec!["工作区目录不存在或已移动".to_owned()]);
    }
    let mut problems = Vec::new();
    match role::role_ref(conn, owner_role_id) {
        Ok(owner) if owner.archived => {
            problems.push(format!("负责岗位「{}」已归档", owner.name));
        }
        Ok(_) => {}
        Err(_) => problems.push("未指定负责岗位".to_owned()),
    }
    if refs.is_empty() {
        problems.push("至少需要一个文件引用".to_owned());
    }
    for r in refs {
        if !files.exists(&r.path) {
            problems.push(format!("引用的文件在工作区中不存在：{}", r.path));
        }
    }
    for dep in depends_on {
        let (key, status): (String, TaskStatus) = conn.query_row(
            "SELECT key, status FROM tasks WHERE id = ?1",
            params![dep],
            |row| Ok((row.get(0)?, enum_col(row, 1)?)),
        )?;
        if status != TaskStatus::Done {
            problems.push(format!("依赖任务 {key} 尚未完成（{}）", status.label()));
        }
    }
    Ok(problems)
}

/// 校验依赖：最多 [`MAX_DEPENDENCIES`] 个、去重、不能依赖自身、须为同团队任务、不能成环（报出环路）。
fn validate_deps(
    conn: &Connection,
    team_id: &str,
    task_id: Option<&str>,
    deps: Vec<String>,
) -> Result<Vec<String>> {
    if deps.len() > MAX_DEPENDENCIES {
        return Err(Error::invalid(format!(
            "依赖任务最多 {MAX_DEPENDENCIES} 个"
        )));
    }
    let mut out: Vec<String> = Vec::with_capacity(deps.len());
    let mut seen: HashSet<String> = HashSet::with_capacity(deps.len());
    for dep in deps {
        if !seen.insert(dep.clone()) {
            continue;
        }
        if Some(dep.as_str()) == task_id {
            return Err(Error::invalid("任务不能依赖自身"));
        }
        let same_team: bool = conn.query_row(
            "SELECT EXISTS (SELECT 1 FROM tasks WHERE id = ?1 AND team_id = ?2)",
            params![dep, team_id],
            |row| row.get(0),
        )?;
        if !same_team {
            return Err(Error::not_found("依赖的任务不存在或不属于该团队"));
        }
        out.push(dep);
    }
    if let Some(task_id) = task_id {
        let mut stmt = conn.prepare(
            "SELECT d.task_id, d.depends_on_id FROM task_dependencies d
             JOIN tasks t ON t.id = d.task_id WHERE t.team_id = ?1 AND d.task_id <> ?2",
        )?;
        let mut edges: HashMap<String, Vec<String>> = HashMap::new();
        for edge in stmt.query_map(params![team_id, task_id], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })? {
            let (from, to) = edge?;
            edges.entry(from).or_default().push(to);
        }
        edges.insert(task_id.to_owned(), out.clone());
        if let Some(cycle) = find_cycle(&edges, task_id) {
            let keys = cycle
                .iter()
                .map(|id| {
                    conn.query_row("SELECT key FROM tasks WHERE id = ?1", params![id], |row| {
                        row.get::<_, String>(0)
                    })
                })
                .collect::<rusqlite::Result<Vec<_>>>()?;
            return Err(Error::conflict(format!(
                "依赖形成环路：{}",
                keys.join(" → ")
            )));
        }
    }
    Ok(out)
}

/// 从 `start` 出发沿依赖边查找回到 `start` 的路径；返回首尾相同的环路。
pub(crate) fn find_cycle(edges: &HashMap<String, Vec<String>>, start: &str) -> Option<Vec<String>> {
    fn dfs(
        edges: &HashMap<String, Vec<String>>,
        start: &str,
        node: &str,
        path: &mut Vec<String>,
        seen: &mut HashSet<String>,
    ) -> bool {
        for next in edges.get(node).into_iter().flatten() {
            if next == start {
                path.push(next.clone());
                return true;
            }
            if seen.insert(next.clone()) {
                path.push(next.clone());
                if dfs(edges, start, next, path, seen) {
                    return true;
                }
                path.pop();
            }
        }
        false
    }
    let mut path = vec![start.to_owned()];
    let mut seen = HashSet::new();
    dfs(edges, start, start, &mut path, &mut seen).then_some(path)
}

/// 下一个任务编号：前缀 + 团队内该前缀的序号，补零到 3 位。
fn next_key(conn: &Connection, team_id: &str, prefix: &str) -> Result<String> {
    let seq: i64 = conn.query_row(
        "INSERT INTO task_sequences (team_id, prefix, last) VALUES (?1, ?2, 1)
         ON CONFLICT (team_id, prefix) DO UPDATE SET last = last + 1
         RETURNING last",
        params![team_id, prefix],
        |row| row.get(0),
    )?;
    Ok(format!("{prefix}-{seq:03}"))
}

fn validate_title(title: &str) -> Result<String> {
    text("任务标题", title, MAX_TITLE, true)
}

/// 负责岗位：未归档且属于该团队。
fn owner_in_team(
    conn: &Connection,
    team_id: &str,
    owner_role_id: &str,
) -> Result<crate::model::Role> {
    let owner = role::get_active(conn, owner_role_id)
        .map_err(|_| Error::not_found("负责岗位不存在或已归档"))?;
    if owner.team_id != team_id {
        return Err(Error::not_found("负责岗位不属于该团队"));
    }
    Ok(owner)
}

fn write_refs_and_deps(
    conn: &Connection,
    task_id: &str,
    refs: &[FileRef],
    deps: &[String],
) -> Result<()> {
    conn.execute(
        "DELETE FROM task_file_refs WHERE task_id = ?1",
        params![task_id],
    )?;
    conn.execute(
        "DELETE FROM task_dependencies WHERE task_id = ?1",
        params![task_id],
    )?;
    for (i, r) in refs.iter().enumerate() {
        conn.execute(
            "INSERT INTO task_file_refs (task_id, position, path, anchor) VALUES (?1, ?2, ?3, ?4)",
            params![task_id, i as i64, r.path, r.anchor],
        )?;
    }
    for (i, d) in deps.iter().enumerate() {
        conn.execute(
            "INSERT INTO task_dependencies (task_id, depends_on_id, position) VALUES (?1, ?2, ?3)",
            params![task_id, d, i as i64],
        )?;
    }
    Ok(())
}

fn not_ready(problems: &[String]) -> Error {
    Error::conflict(format!("任务尚未就绪：{}", problems.join("；")))
}

// ---------------------------------------------------------------- 写入

pub(crate) fn create(
    conn: &Connection,
    input: CreateTaskInput,
    files: &FileCheck,
) -> Result<TaskDetail> {
    let team = team::get_active(conn, &input.team_id)?;
    let title = validate_title(&input.title)?;
    let description = text("任务描述", &input.description, MAX_LONG_TEXT, false)?;
    if !input.status.is_initial() {
        return Err(Error::invalid("新任务的状态只能是 TODO 或 READY"));
    }
    let owner = owner_in_team(conn, &team.id, &input.owner_role_id)?;
    let refs = normalize_refs(input.file_refs)?;
    let deps = validate_deps(conn, &team.id, None, input.depends_on)?;
    let problems = readiness(conn, &owner.id, &refs, &deps, files)?;
    if needs_readiness(input.status) && !problems.is_empty() {
        return Err(not_ready(&problems));
    }
    let id = new_id();
    let key = next_key(conn, &team.id, &owner.task_prefix)?;
    conn.execute(
        "INSERT INTO tasks (id, team_id, key, title, description, status, owner_role_id,
                            created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?8)",
        params![
            id,
            team.id,
            key,
            title,
            description,
            enum_str(input.status),
            owner.id,
            now()
        ],
    )?;
    write_refs_and_deps(conn, &id, &refs, &deps)?;
    event::record(
        conn,
        NewEvent {
            team_id: &team.id,
            task_id: Some(&id),
            role_id: Some(&owner.id),
            kind: EventKind::TaskCreated,
            summary: format!("创建任务 {key}「{title}」，负责岗位「{}」", owner.name),
            data: json!({ "status": input.status, "ownerRoleId": owner.id }),
        },
    )?;
    build_detail(conn, get(conn, &id)?, problems, files)
}

pub(crate) fn update(
    conn: &Connection,
    input: UpdateTaskInput,
    files: &FileCheck,
) -> Result<TaskDetail> {
    let old = get(conn, &input.task_id)?;
    let team = team::get_active(conn, &old.team_id)?;
    let title = validate_title(&input.title)?;
    let description = text("任务描述", &input.description, MAX_LONG_TEXT, false)?;
    let owner = owner_in_team(conn, &team.id, &input.owner_role_id)?;
    let refs = normalize_refs(input.file_refs)?;
    let deps = validate_deps(conn, &team.id, Some(&old.id), input.depends_on)?;
    let problems = readiness(conn, &owner.id, &refs, &deps, files)?;
    if old.status == TaskStatus::Ready && !problems.is_empty() {
        return Err(Error::conflict(format!(
            "任务处于 READY，修改后不再满足就绪条件：{}",
            problems.join("；")
        )));
    }
    conn.execute(
        "UPDATE tasks SET title = ?2, description = ?3, owner_role_id = ?4, updated_at = ?5
         WHERE id = ?1",
        params![old.id, title, description, owner.id, now()],
    )?;
    write_refs_and_deps(conn, &old.id, &refs, &deps)?;
    let mut summary = format!("编辑任务 {}「{title}」", old.key);
    if owner.id != old.owner_role_id {
        summary.push_str(&format!("，负责岗位改为「{}」", owner.name));
    }
    event::record(
        conn,
        NewEvent {
            team_id: &team.id,
            task_id: Some(&old.id),
            role_id: Some(&owner.id),
            kind: EventKind::TaskUpdated,
            summary,
            data: json!({
                "previousOwnerRoleId": old.owner_role_id,
                "ownerRoleId": owner.id,
                "fileRefCount": refs.len(),
                "dependencyCount": deps.len(),
            }),
        },
    )?;
    build_detail(conn, get(conn, &old.id)?, problems, files)
}

pub(crate) fn change_status(
    conn: &Connection,
    input: ChangeTaskStatusInput,
    files: &FileCheck,
) -> Result<TaskDetail> {
    let task = get(conn, &input.task_id)?;
    let team = team::get_active(conn, &task.team_id)?;
    let (from, to) = (task.status, input.to);
    if !from.can_transition_to(to) {
        return Err(Error::conflict(format!(
            "任务 {} 不能从「{}」变为「{}」",
            task.key,
            from.label(),
            to.label()
        )));
    }
    let problems = readiness(
        conn,
        &task.owner_role_id,
        &task.file_refs,
        &task.depends_on,
        files,
    )?;
    if needs_readiness(to) && !problems.is_empty() {
        return Err(not_ready(&problems));
    }
    let reason = if to == TaskStatus::Blocked {
        let reason = text(
            "受阻原因",
            input.reason.as_deref().unwrap_or_default(),
            MAX_REASON,
            false,
        )?;
        if reason.is_empty() {
            return Err(Error::invalid("请填写受阻原因"));
        }
        Some(reason)
    } else {
        None
    };
    conn.execute(
        "UPDATE tasks SET status = ?2, blocked_reason = ?3, updated_at = ?4 WHERE id = ?1",
        params![task.id, enum_str(to), reason, now()],
    )?;
    let mut summary = format!("{} 状态 {} → {}", task.key, from.label(), to.label());
    if let Some(reason) = &reason {
        summary.push_str(&format!("：{reason}"));
    }
    event::record(
        conn,
        NewEvent {
            team_id: &team.id,
            task_id: Some(&task.id),
            role_id: Some(&task.owner_role_id),
            kind: EventKind::TaskStatusChanged,
            summary,
            data: json!({ "from": from, "to": to, "reason": reason }),
        },
    )?;
    build_detail(conn, get(conn, &task.id)?, problems, files)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn r(path: &str) -> FileRef {
        FileRef {
            path: path.to_owned(),
            anchor: None,
        }
    }

    #[test]
    fn file_check_only_trusts_scanned_paths() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("a.md"), "x").unwrap();
        let check = FileCheck::scan(dir.path(), &[r("a.md"), r("b.md")]);
        assert!(check.exists("a.md"));
        assert!(!check.exists("b.md"));
        // 未检查过的路径（如锁外检查后数据被改动）一律视为不存在。
        assert!(!check.exists("c.md"));
        let gone = FileCheck::scan(&dir.path().join("missing"), &[r("a.md")]);
        assert!(!gone.workspace_ok && !gone.exists("a.md"));
    }

    #[test]
    fn refs_are_normalized() {
        let out = normalize_refs(vec![
            r("./docs//PLAN.md#当前状态"),
            r("src\\main.rs"),
            FileRef {
                path: "docs/PLAN.md".into(),
                anchor: Some("#当前状态".into()),
            },
        ])
        .unwrap();
        assert_eq!(
            out,
            vec![
                FileRef {
                    path: "docs/PLAN.md".into(),
                    anchor: Some("当前状态".into())
                },
                r("src/main.rs"),
            ]
        );
    }

    #[test]
    fn refs_reject_absolute_parent_and_empty() {
        for bad in ["/etc/passwd", "C:/x", "a/../b", "..", "", "./", "#x"] {
            let err = normalize_refs(vec![r(bad)]).unwrap_err();
            assert_eq!(err.kind(), crate::ErrorKind::Invalid, "{bad}");
        }
    }

    #[test]
    fn finds_cycle_path() {
        let mut edges: HashMap<String, Vec<String>> = HashMap::new();
        edges.insert("a".into(), vec!["b".into()]);
        edges.insert("b".into(), vec!["x".into(), "c".into()]);
        edges.insert("c".into(), vec!["a".into()]);
        assert_eq!(find_cycle(&edges, "a").unwrap(), vec!["a", "b", "c", "a"]);
        edges.insert("c".into(), vec![]);
        assert!(find_cycle(&edges, "a").is_none());
    }
}
