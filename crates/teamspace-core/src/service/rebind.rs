//! 岗位换绑的持久化与规则（M6-01）。
//!
//! 换绑只改变岗位与 Harness 实例的绑定，不改变岗位、任务与历史（原产品原则）。规则都在这里，
//! 界面只给「方式」，怎么换、何时换、留什么痕迹由内核决定：
//!
//! - 待生效换绑存在 `role_pending_bindings` 表里，一个岗位最多一条。
//! - 待生效换绑在该岗位「没有进行中的执行」时自动生效（见 [`apply_pending`]），生效后写一条
//!   `role_rebind_applied` 活动。待生效期间新的派工仍走旧绑定。
//! - 撤销待生效换绑只删记录，不动当前绑定（`role_rebind_cancelled` 活动留痕）。
//! - 换绑属 Human 的控制权（D8、D32）：Leader 既不能发起也不能撤销。

use rusqlite::{Connection, OptionalExtension, params};
use serde_json::json;

use crate::error::{Error, Result};
use crate::model::{Actor, EventKind, HarnessInstance, PendingBinding, RebindMode, Role};
use crate::service::event::NewEvent;
use crate::service::{event, harness, run as run_svc};
use crate::store::{enum_str, new_id, now};

/// 换绑生效后的结果：用于写活动与推通知。
pub(crate) struct AppliedRebind {
    pub team_id: String,
    pub role_id: String,
    pub role_name: String,
    /// 换绑前的 Harness 实例名；之前没绑定时为 `None`。
    pub previous_harness_name: Option<String>,
    /// 换过去的 Harness 实例名。
    pub harness_name: String,
}

/// 换绑方式转字符串（写入数据库）。
fn mode_str(mode: RebindMode) -> String {
    enum_str(mode)
}

/// 读回换绑方式。
fn parse_mode(raw: &str) -> Result<RebindMode> {
    serde_json::from_value(serde_json::Value::String(raw.to_owned()))
        .map_err(|err| Error::internal(format!("换绑方式无效：{err}")))
}

/// 岗位的待生效换绑；没有时为 `None`。
///
/// 岗位名、目标 Harness 名与发起时的旧绑定一并带出（留痕用）：岗位或 Harness 归档后仍能看清
/// 这条换绑是什么、是从哪个实例换过去的。
pub(crate) fn pending(conn: &Connection, role_id: &str) -> Result<Option<PendingBinding>> {
    let row = conn
        .query_row(
            "SELECT p.id, p.role_id, p.harness_id, p.mode, p.active_run_id, p.requested_at,
                    p.previous_harness_id, p.previous_harness_name,
                    r.name, h.name
             FROM role_pending_bindings p
             JOIN roles r ON r.id = p.role_id
             LEFT JOIN harnesses h ON h.id = p.harness_id
             WHERE p.role_id = ?1",
            params![role_id],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, Option<String>>(4)?,
                    row.get::<_, String>(5)?,
                    row.get::<_, Option<String>>(6)?,
                    row.get::<_, Option<String>>(7)?,
                    row.get::<_, String>(8)?,
                    row.get::<_, String>(9)?,
                ))
            },
        )
        .optional()?;
    let Some((
        id,
        role_id,
        harness_id,
        mode,
        active_run_id,
        requested_at,
        previous_harness_id,
        previous_harness_name,
        role_name,
        harness_name,
    )) = row
    else {
        return Ok(None);
    };
    Ok(Some(PendingBinding {
        id,
        role_id,
        role_name,
        harness_id,
        harness_name,
        previous_harness_id,
        previous_harness_name,
        mode: parse_mode(&mode)?,
        requested_at,
        active_run_id,
    }))
}

/// 登记待生效换绑：当前绑定不动，等当前执行结束后再生效。
///
/// 同一岗位已有待生效换绑时以本次为准（旧的直接替换）。
/// 发起时岗位的当前绑定记进 `previous_harness_*`：待生效期间旧实例名要看得准，
/// 不能靠岗位现在的绑定反推。
pub(crate) fn request(
    conn: &Connection,
    role: &Role,
    target: &HarnessInstance,
    mode: RebindMode,
    active_run_id: Option<String>,
) -> Result<PendingBinding> {
    let id = new_id();
    let requested_at = now();
    let previous_harness_id = role.harness_id.clone();
    let previous_harness_name = previous_harness_id
        .as_deref()
        .and_then(|id| harness::get_any(conn, id).ok().flatten())
        .map(|instance| instance.name);
    conn.execute(
        "DELETE FROM role_pending_bindings WHERE role_id = ?1",
        params![role.id],
    )?;
    conn.execute(
        "INSERT INTO role_pending_bindings
            (id, role_id, team_id, harness_id, mode, active_run_id, requested_at, created_at,
             previous_harness_id, previous_harness_name)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
        params![
            id,
            role.id,
            role.team_id,
            target.id,
            mode_str(mode),
            active_run_id,
            requested_at,
            requested_at,
            previous_harness_id,
            previous_harness_name,
        ],
    )?;
    event::record_as(
        conn,
        Actor::Human,
        NewEvent {
            team_id: &role.team_id,
            task_id: None,
            role_id: Some(&role.id),
            kind: EventKind::RoleRebindRequested,
            summary: format!(
                "岗位「{}」换绑待生效：当前执行结束后切换到 Harness「{}」",
                role.name, target.name
            ),
            data: json!({
                "mode": mode,
                "harnessId": target.id,
                "harnessName": target.name,
                "activeRunId": active_run_id,
                "previousHarnessId": previous_harness_id,
                "previousHarnessName": previous_harness_name,
            }),
        },
    )?;
    Ok(PendingBinding {
        id,
        role_id: role.id.clone(),
        role_name: role.name.clone(),
        harness_id: target.id.clone(),
        harness_name: target.name.clone(),
        previous_harness_id,
        previous_harness_name,
        mode,
        requested_at,
        active_run_id,
    })
}

/// 撤销待生效换绑：删掉记录，当前绑定不变。没有待生效换绑时为 `None`。
pub(crate) fn cancel(conn: &Connection, role: &Role) -> Result<Option<PendingBinding>> {
    let Some(pending) = pending(conn, &role.id)? else {
        return Ok(None);
    };
    conn.execute(
        "DELETE FROM role_pending_bindings WHERE id = ?1",
        params![pending.id],
    )?;
    event::record_as(
        conn,
        Actor::Human,
        NewEvent {
            team_id: &role.team_id,
            task_id: None,
            role_id: Some(&role.id),
            kind: EventKind::RoleRebindCancelled,
            summary: format!(
                "岗位「{}」撤销换绑待生效：不再切换到 Harness「{}」",
                role.name, pending.harness_name
            ),
            data: json!({
                "mode": pending.mode,
                "harnessId": pending.harness_id,
                "harnessName": pending.harness_name,
            }),
        },
    )?;
    Ok(Some(pending))
}

/// 换绑立即生效：把岗位的当前绑定换成目标实例，写一条生效活动。
///
/// 生效后该岗位不再有待生效换绑（有待生效记录时一并清掉）。
pub(crate) fn apply_to(
    conn: &Connection,
    role: &Role,
    target: &HarnessInstance,
    mode: RebindMode,
    actor: Actor,
) -> Result<AppliedRebind> {
    let previous_harness_id = role.harness_id.clone();
    let previous_harness_name = previous_harness_id
        .as_deref()
        .and_then(|id| harness::get_any(conn, id).ok().flatten())
        .map(|harness| harness.name);
    conn.execute(
        "UPDATE roles SET harness_id = ?2, updated_at = ?3 WHERE id = ?1",
        params![role.id, target.id, now()],
    )?;
    conn.execute(
        "DELETE FROM role_pending_bindings WHERE role_id = ?1",
        params![role.id],
    )?;
    event::record_as(
        conn,
        actor,
        NewEvent {
            team_id: &role.team_id,
            task_id: None,
            role_id: Some(&role.id),
            kind: EventKind::RoleRebindApplied,
            summary: format!(
                "岗位「{}」换绑生效：已切换到 Harness「{}」",
                role.name, target.name
            ),
            data: json!({
                "mode": mode,
                "harnessId": target.id,
                "harnessName": target.name,
                "previousHarnessId": previous_harness_id,
                "previousHarnessName": previous_harness_name,
            }),
        },
    )?;
    Ok(AppliedRebind {
        team_id: role.team_id.clone(),
        role_id: role.id.clone(),
        role_name: role.name.clone(),
        previous_harness_name,
        harness_name: target.name.clone(),
    })
}

/// 待生效换绑自动生效：当前执行结束后调用。
///
/// 只有「该岗位没有进行中的执行」时才生效：还在跑的执行不会被中途换掉。
/// 目标 Harness 已归档或已删除时不生效（待生效记录保留，等 Human 处理）。
pub(crate) fn apply_pending(
    conn: &Connection,
    role_id: &str,
    actor: Actor,
) -> Result<Option<AppliedRebind>> {
    let Some(pending) = pending(conn, role_id)? else {
        return Ok(None);
    };
    if run_svc::active_runs_for_role(conn, role_id)?.is_empty() {
        let Some(target) = harness::get_any(conn, &pending.harness_id)? else {
            return Ok(None);
        };
        if target.archived_at.is_some() {
            return Ok(None);
        }
        let role = crate::service::role::get_active(conn, role_id)?;
        return Ok(Some(apply_to(conn, &role, &target, pending.mode, actor)?));
    }
    Ok(None)
}
