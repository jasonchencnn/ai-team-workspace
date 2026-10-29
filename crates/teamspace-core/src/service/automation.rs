//! 团队自动化设置的读写（D43）。表里没有行时按默认值。

use rusqlite::{Connection, OptionalExtension, params};

use crate::error::{Error, Result};
use crate::model::{TeamAutomation, UpdateTeamAutomationInput};
use crate::store::now;

/// 列表类设置每项的最大长度与条数。
const MAX_ITEM: usize = 200;
const MAX_ITEMS: usize = 100;

/// 团队的自动化设置；没改过时为默认值。
pub(crate) fn get(conn: &Connection, team_id: &str) -> Result<TeamAutomation> {
    let row = conn
        .query_row(
            "SELECT autonomy_level, check_command, protected_paths, allow_commands, deny_commands,
                    notify_leader, updated_at
             FROM team_automation WHERE team_id = ?1",
            params![team_id],
            |row| {
                Ok((
                    row.get::<_, u8>(0)?,
                    row.get::<_, Option<String>>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, String>(4)?,
                    row.get::<_, bool>(5)?,
                    row.get::<_, String>(6)?,
                ))
            },
        )
        .optional()?;
    let Some((level, check, protected, allow, deny, notify, updated)) = row else {
        return Ok(TeamAutomation {
            team_id: team_id.to_owned(),
            autonomy_level: 0,
            check_command: None,
            protected_paths: Vec::new(),
            allow_commands: Vec::new(),
            deny_commands: Vec::new(),
            notify_leader: false,
            updated_at: None,
        });
    };
    Ok(TeamAutomation {
        team_id: team_id.to_owned(),
        autonomy_level: level,
        check_command: check,
        protected_paths: list(&protected)?,
        allow_commands: list(&allow)?,
        deny_commands: list(&deny)?,
        notify_leader: notify,
        updated_at: Some(updated),
    })
}

/// 整体替换团队的自动化设置。
pub(crate) fn update(
    conn: &Connection,
    input: &UpdateTeamAutomationInput,
) -> Result<TeamAutomation> {
    if input.autonomy_level > 2 {
        return Err(Error::invalid("自治级别只能是 0、1 或 2"));
    }
    let check = input
        .check_command
        .as_deref()
        .map(str::trim)
        .filter(|command| !command.is_empty());
    if check.is_some_and(|command| command.chars().count() > MAX_ITEM * 5) {
        return Err(Error::invalid("检查命令太长"));
    }
    let protected = clean_list("受保护路径", &input.protected_paths)?;
    if let Some(bad) = protected
        .iter()
        .find(|path| path.starts_with('/') || path.split('/').any(|part| part == ".."))
    {
        return Err(Error::invalid(format!(
            "受保护路径要写相对工作空间的路径：{bad}"
        )));
    }
    let allow = clean_list("自动允许的命令", &input.allow_commands)?;
    let deny = clean_list("自动拒绝的命令", &input.deny_commands)?;
    conn.execute(
        "INSERT INTO team_automation (team_id, autonomy_level, check_command, protected_paths,
                                      allow_commands, deny_commands, notify_leader, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
         ON CONFLICT (team_id) DO UPDATE SET
             autonomy_level = excluded.autonomy_level, check_command = excluded.check_command,
             protected_paths = excluded.protected_paths, allow_commands = excluded.allow_commands,
             deny_commands = excluded.deny_commands, notify_leader = excluded.notify_leader,
             updated_at = excluded.updated_at",
        params![
            input.team_id,
            input.autonomy_level,
            check,
            serde_json::to_string(&protected)?,
            serde_json::to_string(&allow)?,
            serde_json::to_string(&deny)?,
            input.notify_leader,
            now(),
        ],
    )?;
    get(conn, &input.team_id)
}

fn list(text: &str) -> Result<Vec<String>> {
    Ok(serde_json::from_str(text)?)
}

/// 去掉空白项与重复项，校验长度与条数。
fn clean_list(label: &str, items: &[String]) -> Result<Vec<String>> {
    let mut out: Vec<String> = Vec::new();
    for item in items {
        let item = item.trim();
        if item.is_empty() || out.iter().any(|seen| seen == item) {
            continue;
        }
        if item.chars().count() > MAX_ITEM {
            return Err(Error::invalid(format!("{label}太长：{item}")));
        }
        out.push(item.to_owned());
    }
    if out.len() > MAX_ITEMS {
        return Err(Error::invalid(format!("{label}最多 {MAX_ITEMS} 条")));
    }
    Ok(out)
}
