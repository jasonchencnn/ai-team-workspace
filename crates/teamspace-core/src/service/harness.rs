//! Harness 实例。

use std::path::Path;

use rusqlite::{Connection, OptionalExtension, params};
use serde_json::json;

use super::event::{self, NewEvent};
use super::team;
use crate::error::{Error, Result};
use crate::model::{
    CreateHarnessInput, EventKind, HarnessInstance, HealthCheck, HealthStatus, UpdateHarnessInput,
};
use crate::store::{enum_col, enum_str, new_id, non_blank, now};

const COLS: &str = "id, team_id, name, kind, executable, model, profile, health_status, \
                    health_version, health_detail, health_checked_at, created_at, updated_at, \
                    archived_at";

fn from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<HarnessInstance> {
    Ok(HarnessInstance {
        id: row.get(0)?,
        team_id: row.get(1)?,
        name: row.get(2)?,
        kind: enum_col(row, 3)?,
        executable: row.get(4)?,
        model: row.get(5)?,
        profile: row.get(6)?,
        health: HealthCheck {
            status: enum_col(row, 7)?,
            version: row.get(8)?,
            detail: row.get(9)?,
            checked_at: row.get(10)?,
        },
        created_at: row.get(11)?,
        updated_at: row.get(12)?,
        archived_at: row.get(13)?,
    })
}

/// 任意状态的实例（含已归档），用于岗位卡片展示绑定。
pub(crate) fn get_any(conn: &Connection, id: &str) -> Result<Option<HarnessInstance>> {
    Ok(conn
        .query_row(
            &format!("SELECT {COLS} FROM harnesses WHERE id = ?1"),
            params![id],
            from_row,
        )
        .optional()?)
}

/// 未归档实例（所属团队也须未归档）；否则 not_found。
pub(crate) fn get_active(conn: &Connection, id: &str) -> Result<HarnessInstance> {
    conn.query_row(
        &format!(
            "SELECT {COLS} FROM harnesses WHERE id = ?1 AND archived_at IS NULL
             AND team_id IN (SELECT id FROM teams WHERE archived_at IS NULL)"
        ),
        params![id],
        from_row,
    )
    .optional()?
    .ok_or_else(|| Error::not_found("Harness 不存在或已归档"))
}

pub(crate) fn list(conn: &Connection, team_id: &str) -> Result<Vec<HarnessInstance>> {
    team::get_active(conn, team_id)?;
    let mut stmt = conn.prepare(&format!(
        "SELECT {COLS} FROM harnesses WHERE team_id = ?1 AND archived_at IS NULL
         ORDER BY created_at, id"
    ))?;
    let rows = stmt.query_map(params![team_id], from_row)?;
    Ok(rows.collect::<rusqlite::Result<_>>()?)
}

struct Fields {
    name: String,
    executable: Option<String>,
    model: Option<String>,
    profile: Option<String>,
}

fn validate(
    conn: &Connection,
    team_id: &str,
    exclude: Option<&str>,
    name: &str,
    executable: Option<String>,
    model: Option<String>,
    profile: Option<String>,
) -> Result<Fields> {
    let name = name.trim();
    if name.is_empty() {
        return Err(Error::invalid("Harness 名称不能为空"));
    }
    let executable = non_blank(executable);
    if let Some(path) = &executable
        && !Path::new(path).is_absolute()
    {
        return Err(Error::invalid(format!(
            "可执行文件路径必须是绝对路径：{path}"
        )));
    }
    let taken: bool = conn.query_row(
        "SELECT EXISTS (SELECT 1 FROM harnesses WHERE team_id = ?1 AND name = ?2
         AND archived_at IS NULL AND id IS NOT ?3)",
        params![team_id, name, exclude],
        |row| row.get(0),
    )?;
    if taken {
        return Err(Error::conflict(format!(
            "团队中已有名为「{name}」的 Harness"
        )));
    }
    Ok(Fields {
        name: name.to_owned(),
        executable,
        model: non_blank(model),
        profile: non_blank(profile),
    })
}

pub(crate) fn create(conn: &Connection, input: CreateHarnessInput) -> Result<HarnessInstance> {
    team::get_active(conn, &input.team_id)?;
    let f = validate(
        conn,
        &input.team_id,
        None,
        &input.name,
        input.executable,
        input.model,
        input.profile,
    )?;
    let id = new_id();
    conn.execute(
        "INSERT INTO harnesses (id, team_id, name, kind, executable, model, profile,
                                created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?8)",
        params![
            id,
            input.team_id,
            f.name,
            enum_str(input.kind),
            f.executable,
            f.model,
            f.profile,
            now()
        ],
    )?;
    let harness = get_active(conn, &id)?;
    event::record(
        conn,
        NewEvent {
            team_id: &harness.team_id,
            task_id: None,
            role_id: None,
            kind: EventKind::HarnessCreated,
            summary: format!("注册 Harness「{}」", harness.name),
            data: json!({ "harnessId": harness.id, "kind": harness.kind }),
        },
    )?;
    Ok(harness)
}

/// 编辑实例；健康状态重置为 unknown。
pub(crate) fn update(conn: &Connection, input: UpdateHarnessInput) -> Result<HarnessInstance> {
    let old = get_active(conn, &input.harness_id)?;
    let f = validate(
        conn,
        &old.team_id,
        Some(&old.id),
        &input.name,
        input.executable,
        input.model,
        input.profile,
    )?;
    conn.execute(
        "UPDATE harnesses SET name = ?2, executable = ?3, model = ?4, profile = ?5,
                health_status = 'unknown', health_version = NULL, health_detail = NULL,
                health_checked_at = NULL, updated_at = ?6
         WHERE id = ?1",
        params![old.id, f.name, f.executable, f.model, f.profile, now()],
    )?;
    let harness = get_active(conn, &old.id)?;
    event::record(
        conn,
        NewEvent {
            team_id: &harness.team_id,
            task_id: None,
            role_id: None,
            kind: EventKind::HarnessUpdated,
            summary: format!("编辑 Harness「{}」", harness.name),
            data: json!({ "harnessId": harness.id, "previousName": old.name }),
        },
    )?;
    Ok(harness)
}

/// 归档实例；仍被未归档岗位绑定时为 conflict。
pub(crate) fn archive(conn: &Connection, id: &str) -> Result<HarnessInstance> {
    let harness = get_active(conn, id)?;
    let mut stmt = conn.prepare(
        "SELECT name FROM roles WHERE harness_id = ?1 AND archived_at IS NULL
         ORDER BY created_at, id",
    )?;
    let bound: Vec<String> = stmt
        .query_map(params![id], |row| row.get(0))?
        .collect::<rusqlite::Result<_>>()?;
    if !bound.is_empty() {
        let names: Vec<String> = bound.iter().map(|n| format!("「{n}」")).collect();
        return Err(Error::conflict(format!(
            "Harness「{}」仍被岗位{}绑定，请先解绑",
            harness.name,
            names.join("、")
        )));
    }
    let at = now();
    conn.execute(
        "UPDATE harnesses SET archived_at = ?2, updated_at = ?2 WHERE id = ?1",
        params![id, at],
    )?;
    event::record(
        conn,
        NewEvent {
            team_id: &harness.team_id,
            task_id: None,
            role_id: None,
            kind: EventKind::HarnessArchived,
            summary: format!("归档 Harness「{}」", harness.name),
            data: json!({ "harnessId": id }),
        },
    )?;
    Ok(HarnessInstance {
        archived_at: Some(at.clone()),
        updated_at: at,
        ..harness
    })
}

/// 保存健康检查结果（不改 updated_at）。
pub(crate) fn save_health(
    conn: &Connection,
    id: &str,
    health: &HealthCheck,
) -> Result<HarnessInstance> {
    let harness = get_active(conn, id)?;
    conn.execute(
        "UPDATE harnesses SET health_status = ?2, health_version = ?3, health_detail = ?4,
                health_checked_at = ?5
         WHERE id = ?1",
        params![
            id,
            enum_str(health.status),
            health.version,
            health.detail,
            health.checked_at
        ],
    )?;
    let result = match health.status {
        HealthStatus::Ok => "正常",
        HealthStatus::Error => "异常",
        HealthStatus::Unknown => "未知",
    };
    let mut summary = format!("Harness「{}」健康检查：{result}", harness.name);
    if let Some(version) = &health.version {
        summary.push_str(&format!("（{version}）"));
    }
    event::record(
        conn,
        NewEvent {
            team_id: &harness.team_id,
            task_id: None,
            role_id: None,
            kind: EventKind::HarnessChecked,
            summary,
            data: json!({ "harnessId": id, "health": health }),
        },
    )?;
    get_active(conn, id)
}
