//! 岗位与 Harness 绑定。

use rusqlite::{Connection, OptionalExtension, params};
use serde_json::json;

use super::event::{self, NewEvent};
use super::{MAX_LONG_TEXT, MAX_NAME, harness, team, text};
use crate::error::{Error, Result};
use crate::model::{CreateRoleInput, EventKind, NewRole, Role, RoleRef, RoleView, UpdateRoleInput};
use crate::store::{new_id, now};

const COLS: &str = "id, team_id, name, code, task_prefix, description, guidelines, is_leader, \
                    harness_id, created_at, updated_at, archived_at";

fn from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<Role> {
    Ok(Role {
        id: row.get(0)?,
        team_id: row.get(1)?,
        name: row.get(2)?,
        code: row.get(3)?,
        task_prefix: row.get(4)?,
        description: row.get(5)?,
        guidelines: row.get(6)?,
        is_leader: row.get(7)?,
        harness_id: row.get(8)?,
        created_at: row.get(9)?,
        updated_at: row.get(10)?,
        archived_at: row.get(11)?,
    })
}

/// 未归档岗位（所属团队也须未归档）；否则 not_found。
pub(crate) fn get_active(conn: &Connection, id: &str) -> Result<Role> {
    conn.query_row(
        &format!(
            "SELECT {COLS} FROM roles WHERE id = ?1 AND archived_at IS NULL
             AND team_id IN (SELECT id FROM teams WHERE archived_at IS NULL)"
        ),
        params![id],
        from_row,
    )
    .optional()?
    .ok_or_else(|| Error::not_found("岗位不存在或已归档"))
}

/// 引用岗位（含已归档）。
pub(crate) fn role_ref(conn: &Connection, id: &str) -> Result<RoleRef> {
    conn.query_row(
        "SELECT id, name, code, task_prefix, archived_at IS NOT NULL FROM roles WHERE id = ?1",
        params![id],
        |row| {
            Ok(RoleRef {
                id: row.get(0)?,
                name: row.get(1)?,
                code: row.get(2)?,
                task_prefix: row.get(3)?,
                archived: row.get(4)?,
            })
        },
    )
    .optional()?
    .ok_or_else(|| Error::not_found("岗位不存在"))
}

pub(crate) fn view(conn: &Connection, role: Role) -> Result<RoleView> {
    let harness = match &role.harness_id {
        Some(id) => harness::get_any(conn, id)?,
        None => None,
    };
    let (open, doing): (u32, u32) = conn.query_row(
        "SELECT COUNT(*) FILTER (WHERE status NOT IN ('done', 'cancelled')),
                COUNT(*) FILTER (WHERE status = 'doing')
         FROM tasks WHERE owner_role_id = ?1",
        params![role.id],
        |row| Ok((row.get(0)?, row.get(1)?)),
    )?;
    Ok(RoleView {
        role,
        harness,
        open_task_count: open,
        doing_task_count: doing,
    })
}

/// 团队的未归档岗位卡片：Leader 在前，其余按创建时间。
pub(crate) fn list_views(conn: &Connection, team_id: &str) -> Result<Vec<RoleView>> {
    team::get_active(conn, team_id)?;
    let mut stmt = conn.prepare(&format!(
        "SELECT {COLS} FROM roles WHERE team_id = ?1 AND archived_at IS NULL
         ORDER BY is_leader DESC, created_at, id"
    ))?;
    let roles = stmt
        .query_map(params![team_id], from_row)?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    roles.into_iter().map(|r| view(conn, r)).collect()
}

fn valid_code(code: &str) -> bool {
    let mut chars = code.chars();
    code.len() <= 40
        && chars.next().is_some_and(|c| c.is_ascii_lowercase())
        && chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
}

fn valid_prefix(prefix: &str) -> bool {
    (2..=4).contains(&prefix.len()) && prefix.chars().all(|c| c.is_ascii_uppercase())
}

fn validate_name(name: &str) -> Result<String> {
    text("岗位名称", name, MAX_NAME, true)
}

/// 校验职责与工作约定。
fn validate_texts(description: &str, guidelines: &str) -> Result<(String, String)> {
    Ok((
        text("职责", description, MAX_LONG_TEXT, false)?,
        text("工作约定", guidelines, MAX_LONG_TEXT, false)?,
    ))
}

fn validate_prefix(prefix: &str) -> Result<String> {
    let prefix = prefix.trim();
    if !valid_prefix(prefix) {
        return Err(Error::invalid(format!(
            "任务前缀「{prefix}」格式不正确：需为 2–4 位大写字母"
        )));
    }
    Ok(prefix.to_owned())
}

/// 前缀在未归档岗位中唯一、Leader 至多一个（`exclude` 为正在编辑的岗位）。
fn check_unique(
    conn: &Connection,
    team_id: &str,
    exclude: Option<&str>,
    prefix: &str,
    is_leader: bool,
) -> Result<()> {
    let holder: Option<String> = conn
        .query_row(
            "SELECT name FROM roles WHERE team_id = ?1 AND task_prefix = ?2
             AND archived_at IS NULL AND id IS NOT ?3",
            params![team_id, prefix, exclude],
            |row| row.get(0),
        )
        .optional()?;
    if let Some(holder) = holder {
        return Err(Error::conflict(format!(
            "任务前缀「{prefix}」已被岗位「{holder}」使用"
        )));
    }
    if is_leader {
        let leader: Option<String> = conn
            .query_row(
                "SELECT name FROM roles WHERE team_id = ?1 AND is_leader = 1
                 AND archived_at IS NULL AND id IS NOT ?2",
                params![team_id, exclude],
                |row| row.get(0),
            )
            .optional()?;
        if let Some(leader) = leader {
            return Err(Error::conflict(format!(
                "团队已有 Leader「{leader}」，每个团队至多一个 Leader"
            )));
        }
    }
    Ok(())
}

/// 校验并插入岗位（不写活动）。
pub(crate) fn insert(conn: &Connection, team_id: &str, input: NewRole) -> Result<Role> {
    let name = validate_name(&input.name)?;
    let code = input.code.trim();
    if !valid_code(code) {
        return Err(Error::invalid(format!(
            "岗位标识「{code}」格式不正确：需以小写字母开头，只含小写字母、数字和下划线，最长 40 位"
        )));
    }
    let prefix = validate_prefix(&input.task_prefix)?;
    let (description, guidelines) = validate_texts(&input.description, &input.guidelines)?;
    let code_taken: bool = conn.query_row(
        "SELECT EXISTS (SELECT 1 FROM roles WHERE team_id = ?1 AND code = ?2)",
        params![team_id, code],
        |row| row.get(0),
    )?;
    if code_taken {
        return Err(Error::conflict(format!("团队中已有岗位标识「{code}」")));
    }
    check_unique(conn, team_id, None, &prefix, input.is_leader)?;
    let id = new_id();
    let at = now();
    conn.execute(
        "INSERT INTO roles (id, team_id, name, code, task_prefix, description, guidelines,
                            is_leader, created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?9)",
        params![
            id,
            team_id,
            name,
            code,
            prefix,
            description,
            guidelines,
            input.is_leader,
            at
        ],
    )?;
    get_active(conn, &id)
}

pub(crate) fn create(conn: &Connection, input: CreateRoleInput) -> Result<RoleView> {
    team::get_active(conn, &input.team_id)?;
    let role = insert(conn, &input.team_id, input.role)?;
    event::record(
        conn,
        NewEvent {
            team_id: &role.team_id,
            task_id: None,
            role_id: Some(&role.id),
            kind: EventKind::RoleCreated,
            summary: format!("新增岗位「{}」（{}）", role.name, role.task_prefix),
            data: json!({ "code": role.code, "taskPrefix": role.task_prefix }),
        },
    )?;
    view(conn, role)
}

pub(crate) fn update(conn: &Connection, input: UpdateRoleInput) -> Result<RoleView> {
    let old = get_active(conn, &input.role_id)?;
    let name = validate_name(&input.name)?;
    let prefix = validate_prefix(&input.task_prefix)?;
    let (description, guidelines) = validate_texts(&input.description, &input.guidelines)?;
    check_unique(conn, &old.team_id, Some(&old.id), &prefix, input.is_leader)?;
    conn.execute(
        "UPDATE roles SET name = ?2, task_prefix = ?3, description = ?4, guidelines = ?5,
                          is_leader = ?6, updated_at = ?7
         WHERE id = ?1",
        params![
            old.id,
            name,
            prefix,
            description,
            guidelines,
            input.is_leader,
            now()
        ],
    )?;
    let role = get_active(conn, &old.id)?;
    event::record(
        conn,
        NewEvent {
            team_id: &role.team_id,
            task_id: None,
            role_id: Some(&role.id),
            kind: EventKind::RoleUpdated,
            summary: format!("编辑岗位「{}」", role.name),
            data: json!({
                "before": { "name": old.name, "taskPrefix": old.task_prefix, "isLeader": old.is_leader },
                "after": { "name": role.name, "taskPrefix": role.task_prefix, "isLeader": role.is_leader },
            }),
        },
    )?;
    view(conn, role)
}

/// 归档岗位；仍负责未结束任务时为 conflict。同时解除 Harness 绑定（原绑定记入活动 data）。
pub(crate) fn archive(conn: &Connection, id: &str) -> Result<Role> {
    let role = get_active(conn, id)?;
    let open: u32 = conn.query_row(
        "SELECT COUNT(*) FROM tasks WHERE owner_role_id = ?1
         AND status NOT IN ('done', 'cancelled')",
        params![id],
        |row| row.get(0),
    )?;
    if open > 0 {
        return Err(Error::conflict(format!(
            "岗位「{}」还负责 {open} 个未结束的任务，请先转交或结束后再归档",
            role.name
        )));
    }
    let at = now();
    conn.execute(
        "UPDATE roles SET archived_at = ?2, updated_at = ?2, harness_id = NULL WHERE id = ?1",
        params![id, at],
    )?;
    event::record(
        conn,
        NewEvent {
            team_id: &role.team_id,
            task_id: None,
            role_id: Some(id),
            kind: EventKind::RoleArchived,
            summary: format!("归档岗位「{}」", role.name),
            data: json!({ "code": role.code, "previousHarnessId": role.harness_id }),
        },
    )?;
    Ok(Role {
        archived_at: Some(at.clone()),
        updated_at: at,
        harness_id: None,
        ..role
    })
}

pub(crate) fn bind(conn: &Connection, role_id: &str, harness_id: &str) -> Result<RoleView> {
    let role = get_active(conn, role_id)?;
    let harness = harness::get_active(conn, harness_id)?;
    if harness.team_id != role.team_id {
        return Err(Error::not_found("Harness 不属于该岗位所在的团队"));
    }
    if role.harness_id.as_deref() == Some(harness_id) {
        return view(conn, role);
    }
    conn.execute(
        "UPDATE roles SET harness_id = ?2, updated_at = ?3 WHERE id = ?1",
        params![role_id, harness_id, now()],
    )?;
    event::record(
        conn,
        NewEvent {
            team_id: &role.team_id,
            task_id: None,
            role_id: Some(role_id),
            kind: EventKind::RoleHarnessBound,
            summary: format!("岗位「{}」绑定 Harness「{}」", role.name, harness.name),
            data: json!({ "harnessId": harness_id, "previousHarnessId": role.harness_id }),
        },
    )?;
    view(conn, get_active(conn, role_id)?)
}

pub(crate) fn unbind(conn: &Connection, role_id: &str) -> Result<RoleView> {
    let role = get_active(conn, role_id)?;
    let Some(previous) = role.harness_id.clone() else {
        return view(conn, role);
    };
    let previous_name = harness::get_any(conn, &previous)?
        .map(|h| h.name)
        .unwrap_or_default();
    conn.execute(
        "UPDATE roles SET harness_id = NULL, updated_at = ?2 WHERE id = ?1",
        params![role_id, now()],
    )?;
    event::record(
        conn,
        NewEvent {
            team_id: &role.team_id,
            task_id: None,
            role_id: Some(role_id),
            kind: EventKind::RoleHarnessUnbound,
            summary: format!("岗位「{}」解除绑定 Harness「{previous_name}」", role.name),
            data: json!({ "previousHarnessId": previous }),
        },
    )?;
    view(conn, get_active(conn, role_id)?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn code_format() {
        assert!(valid_code("role_frontend"));
        assert!(valid_code("qa2"));
        assert!(!valid_code(""));
        assert!(!valid_code("Role"));
        assert!(!valid_code("2role"));
        assert!(!valid_code("role-x"));
        assert!(!valid_code(&"a".repeat(41)));
    }

    #[test]
    fn prefix_format() {
        assert!(valid_prefix("FE"));
        assert!(valid_prefix("ABCD"));
        assert!(!valid_prefix("F"));
        assert!(!valid_prefix("ABCDE"));
        assert!(!valid_prefix("Fe"));
        assert!(!valid_prefix("F1"));
    }
}
