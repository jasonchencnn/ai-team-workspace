//! 任务：编号、引用、依赖与环路检测、就绪条件、状态流转。

use std::collections::{HashMap, HashSet};
use std::path::Path;

use rusqlite::{Connection, OptionalExtension, params};
use serde_json::json;

use super::event::{self, NewEvent};
use super::{MAX_LONG_TEXT, MAX_REASON, MAX_TITLE, role, team, text};
use crate::error::{Error, Result};
use crate::model::{
    BoardColumn, ChangeTaskStatusInput, CheckTaskDependenciesInput, CreateTaskInput, EventKind,
    FileRef, FileRefView, FileTaskRef, RoleRef, StatusCount, Task, TaskBoard, TaskBoardQuery,
    TaskDependencyCheck, TaskDetail, TaskStatus, TaskSummary, UpdateTaskInput,
};
use crate::store::{enum_col, enum_str, new_id, non_blank, now};

/// 每个任务的文件引用上限。
pub(crate) const MAX_FILE_REFS: usize = 50;
/// 每个任务的依赖上限。
pub(crate) const MAX_DEPENDENCIES: usize = 20;
const DEPENDENCY_NOTICE: &str = "前置完成后会自动变为 READY；须同时满足其他就绪条件：负责岗位有效，填写的引用文件存在（文件引用可选）。";

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
         WHERE d.task_id = t.id AND u.status <> 'done'),
       COALESCE((SELECT x.change_incomplete FROM runs x WHERE x.task_id = t.id
                 ORDER BY x.attempt DESC LIMIT 1), 0)
FROM tasks t JOIN roles r ON r.id = t.owner_role_id";

/// 团队中引用了 `path` 的任务（共享文件页右侧），按状态顺序再按编号排序。
///
/// 一个任务引用同一文件多次只出现一次（锚点取第一条引用）。团队没有任务引用这个文件时返回空列表，
/// 不报错——页面要显示的是「暂无任务引用」。
pub(crate) fn referencing_file(
    conn: &Connection,
    team_id: &str,
    path: &str,
) -> Result<Vec<FileTaskRef>> {
    if path.is_empty() {
        return Err(Error::invalid("请选择文件"));
    }
    let mut stmt = conn.prepare_cached(
        "SELECT t.id, t.key, t.title, t.status,
                r.id, r.name, r.code, r.task_prefix, r.archived_at IS NOT NULL, t.updated_at,
                (SELECT a.anchor FROM task_file_refs a
                  WHERE a.task_id = t.id AND a.path = ?2 ORDER BY a.position LIMIT 1)
         FROM tasks t JOIN roles r ON r.id = t.owner_role_id
         WHERE t.team_id = ?1
           AND t.team_id IN (SELECT id FROM teams WHERE archived_at IS NULL)
           AND EXISTS (SELECT 1 FROM task_file_refs f WHERE f.task_id = t.id AND f.path = ?2)
         ORDER BY t.key",
    )?;
    let mut refs: Vec<FileTaskRef> = stmt
        .query_map(params![team_id, path], |row| {
            Ok(FileTaskRef {
                id: row.get(0)?,
                key: row.get(1)?,
                title: row.get(2)?,
                status: enum_col(row, 3)?,
                owner: RoleRef {
                    id: row.get(4)?,
                    name: row.get(5)?,
                    code: row.get(6)?,
                    task_prefix: row.get(7)?,
                    archived: row.get(8)?,
                },
                updated_at: row.get(9)?,
                anchor: row.get(10)?,
            })
        })?
        .collect::<rusqlite::Result<_>>()?;
    // 状态按标准顺序（待办 → 取消）而不是字母序，进行中的任务才排在前面。
    refs.sort_by_key(|entry| {
        (
            TaskStatus::ALL
                .iter()
                .position(|status| *status == entry.status)
                .unwrap_or(usize::MAX),
            entry.key.clone(),
        )
    });
    Ok(refs)
}

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
            change_incomplete: row.get(14)?,
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

/// 岗位负责的最近任务（任何状态），按更新时间倒序，最多 `limit` 条；以及该岗位负责的任务总数。
pub(crate) fn recent_for_owner(
    conn: &Connection,
    role_id: &str,
    limit: usize,
) -> Result<(Vec<TaskSummary>, u32)> {
    let mut tasks = summaries(conn, "t.owner_role_id = ?1", role_id)?;
    let total = u32::try_from(tasks.len()).unwrap_or(u32::MAX);
    tasks.truncate(limit);
    Ok((tasks, total))
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

/// 文件存在性检查结果；人工命令在锁外预检，自动依赖推进在事务内检查下游引用。
///
/// 未检查过的路径视为不存在。
pub(crate) struct FileCheck {
    workspace_ok: bool,
    existing: HashSet<String>,
    /// 存在、但真实路径（解析符号链接后）在工作空间之外的引用（Q-06）。
    outside: HashSet<String>,
}

impl FileCheck {
    /// 检查工作区目录与各引用文件是否存在，且真实路径在工作空间内。
    pub(crate) fn scan(workspace: &Path, refs: &[FileRef]) -> Self {
        let workspace_ok = workspace.is_dir();
        let mut existing = HashSet::new();
        let mut outside = HashSet::new();
        if let Some(root) = workspace_ok
            .then(|| workspace.canonicalize().ok())
            .flatten()
        {
            for r in refs {
                match workspace.join(&r.path).canonicalize() {
                    Ok(real) if real.starts_with(&root) => {
                        existing.insert(r.path.clone());
                    }
                    Ok(_) => {
                        outside.insert(r.path.clone());
                    }
                    Err(_) => {}
                }
            }
        }
        Self {
            workspace_ok,
            existing,
            outside,
        }
    }

    /// 引用存在且真实路径在工作空间内。
    fn exists(&self, path: &str) -> bool {
        self.workspace_ok && self.existing.contains(path)
    }

    fn is_outside(&self, path: &str) -> bool {
        self.workspace_ok && self.outside.contains(path)
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
    let latest_run = super::run::latest_for_task(conn, &task.id)?;
    let running = latest_run.as_ref().is_some_and(|r| r.status.is_active());
    let needs_merge = review_needs_merge(conn, &task)?;
    let allowed_transitions = if running {
        Vec::new()
    } else {
        task.status
            .next()
            .iter()
            .copied()
            .filter(|to| !needs_readiness(*to) || readiness.is_empty())
            .filter(|to| *to != TaskStatus::Done || !needs_merge)
            .collect()
    };
    let run_problems = super::run::start_problems(conn, &task, files)?;
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
        latest_run,
        run_problems,
        dependency_notice: (task.status == TaskStatus::Todo && !task.depends_on.is_empty())
            .then(|| DEPENDENCY_NOTICE.to_owned()),
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
    for r in refs {
        if files.is_outside(&r.path) {
            problems.push(format!("引用文件不在工作空间内：{}", r.path));
        } else if !files.exists(&r.path) {
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
    let (deps, cycle) = inspect_deps(conn, team_id, task_id, deps)?;
    if !cycle.is_empty() {
        let message = format!("依赖形成环路：{}", cycle.join(" → "));
        return Err(if cycle.len() == 2 {
            Error::invalid(message)
        } else {
            Error::conflict(message)
        });
    }
    Ok(deps)
}

pub(crate) fn check_dependencies(
    conn: &Connection,
    input: CheckTaskDependenciesInput,
) -> Result<TaskDependencyCheck> {
    team::get_active(conn, &input.team_id)?;
    if let Some(task_id) = &input.task_id {
        let task = get(conn, task_id)?;
        if task.team_id != input.team_id {
            return Err(Error::not_found("任务不存在或不属于该团队"));
        }
    }
    let (_, cycle) = inspect_deps(
        conn,
        &input.team_id,
        input.task_id.as_deref(),
        input.depends_on,
    )?;
    Ok(TaskDependencyCheck {
        cycle,
        readiness_notice: DEPENDENCY_NOTICE.to_owned(),
    })
}

fn inspect_deps(
    conn: &Connection,
    team_id: &str,
    task_id: Option<&str>,
    deps: Vec<String>,
) -> Result<(Vec<String>, Vec<String>)> {
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
    if !out.is_empty() {
        let mut stmt = conn.prepare(
            "SELECT d.task_id, d.depends_on_id FROM task_dependencies d
             JOIN tasks t ON t.id = d.task_id
             WHERE t.team_id = ?1 AND (?2 IS NULL OR d.task_id <> ?2)",
        )?;
        let mut edges: HashMap<String, Vec<String>> = HashMap::new();
        for edge in stmt.query_map(params![team_id, task_id], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })? {
            let (from, to) = edge?;
            edges.entry(from).or_default().push(to);
        }
        if let Some(task_id) = task_id {
            edges.insert(task_id.to_owned(), out.clone());
        }
        let starts = task_id.map_or_else(|| out.clone(), |id| vec![id.to_owned()]);
        for start in starts {
            let Some(cycle) = find_cycle(&edges, &start) else {
                continue;
            };
            let keys = cycle
                .iter()
                .map(|id| {
                    conn.query_row("SELECT key FROM tasks WHERE id = ?1", params![id], |row| {
                        row.get::<_, String>(0)
                    })
                })
                .collect::<rusqlite::Result<Vec<_>>>()?;
            return Ok((out, keys));
        }
    }
    Ok((out, vec![]))
}

/// 从 `start` 出发沿依赖边查找可达环路；返回首尾相同的完整环路。
pub(crate) fn find_cycle(edges: &HashMap<String, Vec<String>>, start: &str) -> Option<Vec<String>> {
    fn dfs(
        edges: &HashMap<String, Vec<String>>,
        node: &str,
        path: &mut Vec<String>,
        seen: &mut HashSet<String>,
        active: &mut HashMap<String, usize>,
    ) -> Option<Vec<String>> {
        if let Some(&index) = active.get(node) {
            let mut cycle = path[index..].to_vec();
            cycle.push(node.to_owned());
            return Some(cycle);
        }
        if !seen.insert(node.to_owned()) {
            return None;
        }
        active.insert(node.to_owned(), path.len());
        path.push(node.to_owned());
        for next in edges.get(node).into_iter().flatten() {
            if let Some(cycle) = dfs(edges, next, path, seen, active) {
                return Some(cycle);
            }
        }
        path.pop();
        active.remove(node);
        None
    }
    dfs(
        edges,
        start,
        &mut Vec::new(),
        &mut HashSet::new(),
        &mut HashMap::new(),
    )
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

/// 有活动执行时拒绝编辑任务的原因（前端同文案展示）。
pub(crate) const EDIT_BLOCKED_BY_RUN: &str = "任务正在执行中，结束或取消执行后才能编辑";

pub(crate) fn update(
    conn: &Connection,
    input: UpdateTaskInput,
    files: &FileCheck,
) -> Result<TaskDetail> {
    let old = get(conn, &input.task_id)?;
    if super::run::active_for_task(conn, &old.id)?.is_some() {
        return Err(Error::conflict(EDIT_BLOCKED_BY_RUN));
    }
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
    dependent_files: Option<&super::dependency::FileChecks>,
) -> Result<TaskDetail> {
    let task = get(conn, &input.task_id)?;
    let team = team::get_active(conn, &task.team_id)?;
    if let Some(run) = super::run::active_for_task(conn, &task.id)? {
        return Err(Error::conflict(format!(
            "任务 {} 正在执行（第 {} 次），请先取消执行或人工接管",
            task.key, run.attempt
        )));
    }
    let (from, to) = (task.status, input.to);
    if from == TaskStatus::Review && to == TaskStatus::Done && review_needs_merge(conn, &task)? {
        return Err(Error::conflict(format!(
            "任务 {} 的改动在执行分支上，请在验收页「通过」，由 Teamspace 合并到基线分支（直接改为已完成会跳过合并）",
            task.key
        )));
    }
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
    let mut data = json!({ "from": from, "to": to, "reason": reason });
    if from == TaskStatus::Done && to == TaskStatus::Ready {
        let affected = super::dependency::active_dependent_keys(conn, &task.id)?;
        if !affected.is_empty() {
            summary.push_str(&format!("；下游 {} 个任务已在进行，未回退", affected.len()));
        }
        data["affectedDependentKeys"] = json!(affected);
    }
    event::record(
        conn,
        NewEvent {
            team_id: &team.id,
            task_id: Some(&task.id),
            role_id: Some(&task.owner_role_id),
            kind: EventKind::TaskStatusChanged,
            summary,
            data,
        },
    )?;
    if to == TaskStatus::Done {
        let dependent_files = dependent_files
            .ok_or_else(|| Error::conflict("缺少下游引用文件检查，请重试完成任务"))?;
        on_task_done(conn, &task.id, dependent_files)?;
    }
    build_detail(conn, get(conn, &task.id)?, problems, files)
}

/// 待验收的任务改动在 Git 执行分支上（最近一次执行是 worktree 执行）：只能经验收「通过」合并后完成，
/// 不能用 `change_task_status` 直接改为 DONE（D20，避免跳过合并）。普通文件夹的执行改动已在原地，
/// 直接改为 DONE 与验收通过的效果相同，保留原行为。
pub(crate) fn review_needs_merge(conn: &Connection, task: &Task) -> Result<bool> {
    if task.status != TaskStatus::Review {
        return Ok(false);
    }
    // 只有尚未验收的 Git 执行需要合并；已有验收结果的旧执行（任务重新打开后手动提交验收）不拦。
    Ok(super::run::latest_for_task(conn, &task.id)?
        .is_some_and(|r| r.isolation == crate::model::RunIsolation::Worktree && r.review.is_none()))
}

/// 任务转为 DONE 之后的依赖推进钩子：前置任务完成后，满足就绪条件的下游任务自动转为 READY
/// （[`super::dependency::advance_dependents`]）。
///
/// 调用约定：在把任务置为 DONE 的**同一个事务**里、紧随状态更新（及其活动记录）之后调用——验收通过
/// （[`crate::api`] 的 `approve_review`）、手动合并完成、合并对账与 `change_task_status` 置为 DONE。这样「上游完成」与
/// 「下游就绪」一起提交或一起回滚，不会出现上游已完成、下游仍停在 TODO 的中间状态；钩子返回错误时整个
/// 事务回滚，任务也不会变成 DONE。调用方在数据库锁外扫描下游引用文件，再传入快照；钩子内重新校验
/// 候选、引用和就绪条件，快照失效时报错回滚。钩子不做文件或进程 I/O，岗位文件夹由调用方在事务提交后同步。
pub(crate) fn on_task_done(
    conn: &Connection,
    task_id: &str,
    files: &super::dependency::FileChecks,
) -> Result<Vec<String>> {
    super::dependency::advance_dependents(conn, task_id, files)
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
