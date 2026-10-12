//! 团队。

use rusqlite::{Connection, OptionalExtension, params};
use serde_json::json;

use super::event::{self, NewEvent};
use super::{role, task};
use crate::error::{Error, Result};
use crate::model::{CreateTeamInput, EventKind, Team, TeamOverview, WorkspaceInfo};
use crate::store::{new_id, now};

const COLS: &str = "id, name, description, workspace_path, created_at, updated_at, archived_at";

fn from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<Team> {
    Ok(Team {
        id: row.get(0)?,
        name: row.get(1)?,
        description: row.get(2)?,
        workspace_path: row.get(3)?,
        created_at: row.get(4)?,
        updated_at: row.get(5)?,
        archived_at: row.get(6)?,
    })
}

/// 未归档的团队；不存在或已归档时为 not_found。
pub(crate) fn get_active(conn: &Connection, id: &str) -> Result<Team> {
    conn.query_row(
        &format!("SELECT {COLS} FROM teams WHERE id = ?1 AND archived_at IS NULL"),
        params![id],
        from_row,
    )
    .optional()?
    .ok_or_else(|| Error::not_found("团队不存在或已归档"))
}

pub(crate) fn list(conn: &Connection) -> Result<Vec<Team>> {
    let mut stmt = conn.prepare(&format!(
        "SELECT {COLS} FROM teams WHERE archived_at IS NULL ORDER BY created_at, id"
    ))?;
    let rows = stmt.query_map([], from_row)?;
    Ok(rows.collect::<rusqlite::Result<_>>()?)
}

/// 校验团队名称（在探测工作区之前调用，尽早报错）。
pub(crate) fn validate_name(name: &str) -> Result<String> {
    let name = name.trim();
    if name.is_empty() {
        return Err(Error::invalid("团队名称不能为空"));
    }
    Ok(name.to_owned())
}

/// 创建团队及初始岗位。`workspace` 为已探测的工作区。
pub(crate) fn create(
    conn: &Connection,
    input: CreateTeamInput,
    workspace: &WorkspaceInfo,
) -> Result<Team> {
    let name = validate_name(&input.name)?;
    let taken: bool = conn.query_row(
        "SELECT EXISTS (SELECT 1 FROM teams WHERE name = ?1 AND archived_at IS NULL)",
        params![name],
        |row| row.get(0),
    )?;
    if taken {
        return Err(Error::conflict(format!("已有同名团队「{name}」")));
    }
    let id = new_id();
    let at = now();
    conn.execute(
        "INSERT INTO teams (id, name, description, workspace_path, created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?5)",
        params![id, name, input.description.trim(), workspace.path, at],
    )?;
    let mut role_names = Vec::new();
    for new_role in input.roles {
        let created = role::insert(conn, &id, new_role)?;
        role_names.push(created.name);
    }
    let summary = if role_names.is_empty() {
        format!("创建团队「{name}」")
    } else {
        format!("创建团队「{name}」，含 {} 个岗位", role_names.len())
    };
    event::record(
        conn,
        NewEvent {
            team_id: &id,
            task_id: None,
            role_id: None,
            kind: EventKind::TeamCreated,
            summary,
            data: json!({ "workspacePath": workspace.path, "roles": role_names }),
        },
    )?;
    get_active(conn, &id)
}

/// 概览中来自数据库的部分；工作区状态由调用方在锁外探测后填入。
pub(crate) fn overview(conn: &Connection, team_id: &str) -> Result<TeamOverview> {
    let team = get_active(conn, team_id)?;
    Ok(TeamOverview {
        roles: role::list_views(conn, team_id)?,
        task_counts: task::counts(conn, team_id)?,
        active_tasks: task::active(conn, team_id)?,
        recent_events: event::recent(conn, team_id, event::RECENT_LIMIT)?,
        team,
        workspace: None,
        workspace_problem: None,
    })
}
