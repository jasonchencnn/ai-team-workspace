//! 应用内通知的生成与订阅设置（M5-02）。
//!
//! 订阅模型：每个（团队，类别，作用域）一条开关记录。作用域 `*` 是类别的总开关，其余是岗位 id。
//! 没有记录时默认开启，因此表里只保存用户改过的选择。
//!
//! 判定一条通知是否推送：该类别的总开关与该岗位的开关都不能是「关」。两者都没记录时默认推送。
//! 通知的生成与推送分离：这里只负责「按订阅决定要不要生成」，推送由 [`crate::api::Core`] 在
//! 事务提交后完成（推送时不再持有数据库锁）。

use rusqlite::{Connection, OptionalExtension, params};

use crate::error::{Error, Result};
use crate::model::{
    Notification, NotificationCategory, NotificationCategorySetting, NotificationRoleSetting,
    NotificationSettings, NotificationSpec, Role, UpdateNotificationSettingsInput,
};
use crate::store::{enum_str, new_id, now};

/// 类别总开关的作用域。
const SCOPE_ALL: &str = "*";

/// 该类别、该岗位的通知是否开启（默认开启）。
pub(crate) fn subscribed(
    conn: &Connection,
    team_id: &str,
    category: NotificationCategory,
    role_id: Option<&str>,
) -> Result<bool> {
    let mut enabled = switch(conn, team_id, category, SCOPE_ALL)?.unwrap_or(true);
    if let Some(role_id) = role_id
        && let Some(role_enabled) = switch(conn, team_id, category, role_id)?
    {
        enabled &= role_enabled;
    }
    Ok(enabled)
}

/// 读一条开关记录；没有记录时为 `None`（即默认开启）。
fn switch(
    conn: &Connection,
    team_id: &str,
    category: NotificationCategory,
    scope: &str,
) -> Result<Option<bool>> {
    let value: Option<i64> = conn
        .query_row(
            "SELECT enabled FROM notification_settings
             WHERE team_id = ?1 AND category = ?2 AND scope = ?3",
            params![team_id, enum_str(category), scope],
            |row| row.get(0),
        )
        .optional()?;
    Ok(value.map(|v| v != 0))
}

/// 写一条开关记录（覆盖同一条）。
fn set_switch(
    conn: &Connection,
    team_id: &str,
    category: NotificationCategory,
    scope: &str,
    enabled: bool,
) -> Result<()> {
    conn.execute(
        "INSERT INTO notification_settings (team_id, category, scope, enabled)
         VALUES (?1, ?2, ?3, ?4)
         ON CONFLICT (team_id, category, scope) DO UPDATE SET enabled = ?4",
        params![team_id, enum_str(category), scope, i64::from(enabled)],
    )?;
    Ok(())
}

/// 按订阅设置生成通知；该类别（或该岗位）关掉时不生成（返回 `None`）。
pub(crate) fn build(conn: &Connection, spec: &NotificationSpec) -> Result<Option<Notification>> {
    if !subscribed(conn, &spec.team_id, spec.category, spec.role_id.as_deref())? {
        return Ok(None);
    }
    Ok(Some(Notification {
        id: new_id(),
        team_id: spec.team_id.clone(),
        category: spec.category,
        category_label: spec.category.label().to_owned(),
        title: spec.title.clone(),
        body: spec.body.clone(),
        task_id: spec.task_id.clone(),
        task_key: spec.task_key.clone(),
        message_id: spec.message_id.clone(),
        thread_id: spec.thread_id.clone(),
        run_id: spec.run_id.clone(),
        role_id: spec.role_id.clone(),
        occurred_at: spec.occurred_at.clone().unwrap_or_else(now),
    }))
}

/// 团队的通知订阅设置：按类别列出总开关与各岗位的开关。
///
/// 岗位开关回显的是**岗位自己的存储值**（没有记录时为开），不是「与总开关相与」后的生效值：
/// 总开关关着时也要让用户看到每个岗位原本的设置，别在再次保存时把岗位设置冲掉。
/// 是否真的发通知仍由 [`subscribed`] 判断。
pub(crate) fn settings(conn: &Connection, team_id: &str) -> Result<NotificationSettings> {
    let roles = active_roles(conn, team_id)?;
    let mut categories = Vec::with_capacity(NotificationCategory::ALL.len());
    for category in NotificationCategory::ALL {
        let mut role_settings = Vec::with_capacity(roles.len());
        for role in &roles {
            role_settings.push(NotificationRoleSetting {
                role_id: role.id.clone(),
                role_name: role.name.clone(),
                role_code: role.code.clone(),
                enabled: switch(conn, team_id, category, &role.id)?.unwrap_or(true),
            });
        }
        categories.push(NotificationCategorySetting {
            category,
            category_label: category.label().to_owned(),
            enabled: switch(conn, team_id, category, SCOPE_ALL)?.unwrap_or(true),
            roles: role_settings,
        });
    }
    Ok(NotificationSettings {
        team_id: team_id.to_owned(),
        categories,
    })
}

/// 修改订阅设置并返回修改后的设置。
///
/// 只改动输入里给出的项：类别总开关缺省不改，岗位开关列表缺省不改。
pub(crate) fn update(
    conn: &Connection,
    input: &UpdateNotificationSettingsInput,
) -> Result<NotificationSettings> {
    let roles = active_roles(conn, &input.team_id)?;
    for category in &input.categories {
        if let Some(enabled) = category.enabled {
            set_switch(conn, &input.team_id, category.category, SCOPE_ALL, enabled)?;
        }
        for role in &category.roles {
            if !roles.iter().any(|r| r.id == role.role_id) {
                return Err(Error::invalid("岗位不在该团队中"));
            }
            set_switch(
                conn,
                &input.team_id,
                category.category,
                &role.role_id,
                role.enabled,
            )?;
        }
    }
    settings(conn, &input.team_id)
}

/// 团队内未归档的岗位。
fn active_roles(conn: &Connection, team_id: &str) -> Result<Vec<Role>> {
    let mut stmt = conn.prepare(
        "SELECT id, team_id, name, code, task_prefix, description, guidelines, is_leader,
                harness_id, created_at, updated_at, archived_at
         FROM roles WHERE team_id = ?1 AND archived_at IS NULL ORDER BY created_at, id",
    )?;
    let rows = stmt
        .query_map(params![team_id], from_row)?
        .collect::<rusqlite::Result<_>>()?;
    Ok(rows)
}

fn from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<Role> {
    Ok(Role {
        id: row.get(0)?,
        team_id: row.get(1)?,
        name: row.get(2)?,
        code: row.get(3)?,
        task_prefix: row.get(4)?,
        description: row.get(5)?,
        guidelines: row.get(6)?,
        is_leader: row.get::<_, i64>(7)? != 0,
        harness_id: row.get(8)?,
        created_at: row.get(9)?,
        updated_at: row.get(10)?,
        archived_at: row.get(11)?,
    })
}
