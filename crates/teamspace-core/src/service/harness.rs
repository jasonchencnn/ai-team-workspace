//! Harness 实例（ACP Agent，D13）与 ACP 预设。

use std::path::Path;

use rusqlite::{Connection, OptionalExtension, params};
use serde_json::json;

use super::event::{self, NewEvent};
use super::{MAX_NAME, team, text};
use crate::error::{Error, Result};
use crate::mock_agent;
use crate::model::{
    AcpPreset, AgentInfo, CreateHarnessInput, EventKind, Handshake, HandshakeStatus,
    HarnessInstance, UpdateHarnessInput,
};
use crate::store::{enum_col, enum_str, new_id, non_blank, now};

/// 启动命令与单个参数的字符上限。
const MAX_COMMAND: usize = 1_000;
/// 参数个数上限。
const MAX_ARGS: usize = 64;

/// 预设的固定信息（与本机检测无关）。
pub(crate) struct PresetSpec {
    pub description: &'static str,
    /// 默认启动命令；`custom` 为 `None`。
    pub command: Option<&'static str>,
    pub args: &'static [&'static str],
    /// 适配器依赖的底层 CLI。
    pub requires_cli: Option<&'static str>,
    /// 默认命令缺失时需要安装的东西（中文）。
    pub install: Option<&'static str>,
    /// 底层 CLI 缺失时需要安装的东西（中文）。
    pub install_cli: Option<&'static str>,
}

/// 预设的默认命令、参数和安装提示。
pub(crate) fn preset_spec(preset: AcpPreset) -> PresetSpec {
    match preset {
        AcpPreset::ClaudeCode => PresetSpec {
            description: "经适配器 claude-code-acp",
            command: Some("claude-code-acp"),
            args: &[],
            requires_cli: Some("claude"),
            install: Some(
                "需要安装 Claude Code ACP 适配器（npm 包 @zed-industries/claude-code-acp，提供 claude-code-acp 命令）",
            ),
            install_cli: Some("需要安装 Claude Code（claude 命令）并完成登录"),
        },
        AcpPreset::Codex => PresetSpec {
            description: "经适配器 codex-acp",
            command: Some("codex-acp"),
            args: &[],
            requires_cli: Some("codex"),
            install: Some(
                "需要安装 Codex ACP 适配器（npm 包 @zed-industries/codex-acp，提供 codex-acp 命令）",
            ),
            install_cli: Some("需要安装 Codex CLI（codex 命令）并完成登录"),
        },
        AcpPreset::MinimaxCode => PresetSpec {
            description: "原生 ACP：mcode acp",
            command: Some("mcode"),
            args: &["acp"],
            requires_cli: None,
            install: Some("需要安装 MiniMax Code（mcode 命令）"),
            install_cli: None,
        },
        AcpPreset::GeminiCli => PresetSpec {
            description: "原生 ACP：gemini --experimental-acp",
            command: Some("gemini"),
            args: &["--experimental-acp"],
            requires_cli: None,
            install: Some("需要安装 Gemini CLI（gemini 命令）"),
            install_cli: None,
        },
        AcpPreset::Custom => PresetSpec {
            description: "填写任意 ACP Agent 的启动命令",
            command: None,
            args: &[],
            requires_cli: None,
            install: None,
            install_cli: None,
        },
        AcpPreset::Mock => PresetSpec {
            description: "内置测试 Agent，不消耗额度",
            command: Some(mock_agent::AGENT_NAME),
            args: &[],
            requires_cli: None,
            install: None,
            install_cli: None,
        },
    }
}

const COLS: &str = "id, team_id, name, preset, command, args, env_profile, handshake_status, \
                    protocol_version, agent_name, agent_title, agent_version, capabilities, \
                    auth_methods, handshake_detail, handshake_checked_at, created_at, updated_at, \
                    archived_at";

/// JSON 列解析失败时转成 rusqlite 错误。
fn json_col<T: serde::de::DeserializeOwned>(
    row: &rusqlite::Row<'_>,
    idx: usize,
) -> rusqlite::Result<Option<T>> {
    let raw: Option<String> = row.get(idx)?;
    raw.map(|raw| {
        serde_json::from_str(&raw).map_err(|err| {
            rusqlite::Error::FromSqlConversionFailure(
                idx,
                rusqlite::types::Type::Text,
                Box::new(err),
            )
        })
    })
    .transpose()
}

fn from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<HarnessInstance> {
    let agent = row
        .get::<_, Option<String>>(9)?
        .map(|name| -> rusqlite::Result<AgentInfo> {
            Ok(AgentInfo {
                name,
                title: row.get(10)?,
                version: row.get(11)?,
            })
        })
        .transpose()?;
    Ok(HarnessInstance {
        id: row.get(0)?,
        team_id: row.get(1)?,
        name: row.get(2)?,
        preset: enum_col(row, 3)?,
        command: row.get(4)?,
        args: json_col(row, 5)?.unwrap_or_default(),
        env_profile: row.get(6)?,
        handshake: Handshake {
            status: enum_col(row, 7)?,
            protocol_version: row.get(8)?,
            agent,
            capabilities: json_col(row, 12)?,
            auth_methods: json_col(row, 13)?.unwrap_or_default(),
            detail: row.get(14)?,
            checked_at: row.get(15)?,
        },
        created_at: row.get(16)?,
        updated_at: row.get(17)?,
        archived_at: row.get(18)?,
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
    command: String,
    args: Vec<String>,
    env_profile: Option<String>,
}

fn has_control(value: &str) -> bool {
    value.chars().any(char::is_control)
}

/// 校验启动命令：命令名（不含空白和 `/`）或绝对路径。
fn validate_command(command: &str) -> Result<()> {
    if command.chars().count() > MAX_COMMAND || has_control(command) {
        return Err(Error::invalid(format!(
            "启动命令不能超过 {MAX_COMMAND} 个字符，且不能包含控制字符"
        )));
    }
    if command.contains('/') {
        if !Path::new(command).is_absolute() {
            return Err(Error::invalid(format!(
                "启动命令须为命令名或绝对路径：{command}"
            )));
        }
    } else if command.chars().any(char::is_whitespace) {
        return Err(Error::invalid(format!(
            "启动命令「{command}」包含空格：参数请填在「参数」里"
        )));
    }
    Ok(())
}

fn validate_args(args: &[String]) -> Result<()> {
    if args.len() > MAX_ARGS {
        return Err(Error::invalid(format!("参数不能超过 {MAX_ARGS} 个")));
    }
    if args
        .iter()
        .any(|arg| arg.chars().count() > MAX_COMMAND || has_control(arg))
    {
        return Err(Error::invalid(format!(
            "单个参数不能超过 {MAX_COMMAND} 个字符，且不能包含控制字符"
        )));
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn validate(
    conn: &Connection,
    team_id: &str,
    exclude: Option<&str>,
    preset: AcpPreset,
    name: &str,
    command: Option<String>,
    args: Option<Vec<String>>,
    env_profile: Option<String>,
) -> Result<Fields> {
    let name = text("Harness 名称", name, MAX_NAME, true)?;
    let spec = preset_spec(preset);
    let (command, args) = if preset == AcpPreset::Mock {
        // Mock 由宿主程序以内置参数启动，命令与参数固定。
        (mock_agent::AGENT_NAME.to_owned(), Vec::new())
    } else {
        let command = match (non_blank(command), spec.command) {
            (Some(command), _) => command,
            (None, Some(default)) => default.to_owned(),
            (None, None) => return Err(Error::invalid("自定义 ACP Agent 需要填写启动命令")),
        };
        validate_command(&command)?;
        let args = args.unwrap_or_else(|| spec.args.iter().map(|a| (*a).to_owned()).collect());
        validate_args(&args)?;
        (command, args)
    };
    let env_profile = non_blank(env_profile);
    if let Some(path) = &env_profile
        && (!Path::new(path).is_absolute() || has_control(path))
    {
        return Err(Error::invalid(format!("环境配置档案须为绝对路径：{path}")));
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
        name,
        command,
        args,
        env_profile,
    })
}

fn args_json(args: &[String]) -> String {
    serde_json::to_string(args).unwrap_or_else(|_| "[]".to_owned())
}

pub(crate) fn create(conn: &Connection, input: CreateHarnessInput) -> Result<HarnessInstance> {
    team::get_active(conn, &input.team_id)?;
    let f = validate(
        conn,
        &input.team_id,
        None,
        input.preset,
        &input.name,
        input.command,
        input.args,
        input.env_profile,
    )?;
    let id = new_id();
    conn.execute(
        "INSERT INTO harnesses (id, team_id, name, preset, command, args, env_profile,
                                created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?8)",
        params![
            id,
            input.team_id,
            f.name,
            enum_str(input.preset),
            f.command,
            args_json(&f.args),
            f.env_profile,
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
            data: json!({
                "harnessId": harness.id,
                "preset": harness.preset,
                "command": harness.command,
                "args": harness.args,
            }),
        },
    )?;
    Ok(harness)
}

/// 编辑实例（预设不可改）；握手状态重置为 unknown。
pub(crate) fn update(conn: &Connection, input: UpdateHarnessInput) -> Result<HarnessInstance> {
    let old = get_active(conn, &input.harness_id)?;
    let f = validate(
        conn,
        &old.team_id,
        Some(&old.id),
        old.preset,
        &input.name,
        input.command,
        input.args,
        input.env_profile,
    )?;
    conn.execute(
        "UPDATE harnesses SET name = ?2, command = ?3, args = ?4, env_profile = ?5,
                handshake_status = 'unknown', protocol_version = NULL, agent_name = NULL,
                agent_title = NULL, agent_version = NULL, capabilities = NULL,
                auth_methods = '[]', handshake_detail = NULL, handshake_checked_at = NULL,
                updated_at = ?6
         WHERE id = ?1",
        params![
            old.id,
            f.name,
            f.command,
            args_json(&f.args),
            f.env_profile,
            now()
        ],
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
            data: json!({
                "harnessId": harness.id,
                "previousName": old.name,
                "command": harness.command,
                "args": harness.args,
            }),
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

/// 握手状态的中文名。
pub(crate) fn status_label(status: HandshakeStatus) -> &'static str {
    match status {
        HandshakeStatus::Connected => "已连接",
        HandshakeStatus::NeedsAuth => "需要登录",
        HandshakeStatus::Error => "异常",
        HandshakeStatus::Unknown => "未检查",
    }
}

/// 保存握手结果（不改 updated_at）。
pub(crate) fn save_handshake(
    conn: &Connection,
    id: &str,
    handshake: &Handshake,
) -> Result<HarnessInstance> {
    let harness = get_active(conn, id)?;
    let agent = handshake.agent.as_ref();
    let capabilities = handshake
        .capabilities
        .as_ref()
        .map(|caps| serde_json::to_string(caps).unwrap_or_default());
    conn.execute(
        "UPDATE harnesses SET handshake_status = ?2, protocol_version = ?3, agent_name = ?4,
                agent_title = ?5, agent_version = ?6, capabilities = ?7, auth_methods = ?8,
                handshake_detail = ?9, handshake_checked_at = ?10
         WHERE id = ?1",
        params![
            id,
            enum_str(handshake.status),
            handshake.protocol_version,
            agent.map(|a| &a.name),
            agent.and_then(|a| a.title.as_ref()),
            agent.and_then(|a| a.version.as_ref()),
            capabilities,
            args_json(&handshake.auth_methods),
            handshake.detail,
            handshake.checked_at
        ],
    )?;
    let mut summary = format!(
        "Harness「{}」握手检查：{}",
        harness.name,
        status_label(handshake.status)
    );
    if let Some(agent) = agent {
        match &agent.version {
            Some(version) => summary.push_str(&format!("（{} {version}）", agent.name)),
            None => summary.push_str(&format!("（{}）", agent.name)),
        }
    }
    event::record(
        conn,
        NewEvent {
            team_id: &harness.team_id,
            task_id: None,
            role_id: None,
            kind: EventKind::HarnessChecked,
            summary,
            data: json!({ "harnessId": id, "handshake": handshake }),
        },
    )?;
    get_active(conn, id)
}
