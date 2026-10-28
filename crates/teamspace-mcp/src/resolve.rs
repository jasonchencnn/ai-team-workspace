//! 把 Leader 说的岗位名、任务编号解析成 id。
//!
//! Leader 用中文说话时更习惯说「前端开发」「FE-002」而不是 id。这里先调用查看类命令
//! （`list_teams` / `list_roles` / `get_task_board`），再在结果里按 id、标识、名称、编号
//! 逐级匹配；解析不到或有歧义时把候选列出来，让 Leader 补一句就能继续。

use serde_json::{Value, json};

use crate::client::{ControlClient, ControlError};
use crate::error::ToolError;

/// 团队。
#[derive(Debug, Clone)]
pub struct TeamRef {
    pub id: String,
    pub name: String,
}

/// 岗位。
#[derive(Debug, Clone)]
pub struct RoleRef {
    pub id: String,
    pub name: String,
    pub code: String,
    pub task_prefix: String,
}

/// Harness 实例。
#[derive(Debug, Clone)]
pub struct HarnessRef {
    pub id: String,
    pub name: String,
}

/// 任务。
#[derive(Debug, Clone)]
pub struct TaskRef {
    pub id: String,
    pub key: String,
    pub title: String,
    pub status: String,
    pub team_id: String,
    pub team_name: String,
}

/// 匹配结果。
enum Match<T> {
    /// 唯一命中。
    One(T),
    /// 多个同分命中（有歧义）。
    Ambiguous(Vec<T>),
    /// 没有命中。
    None,
}

/// 全部团队。
pub async fn teams(client: &ControlClient) -> Result<Vec<TeamRef>, ToolError> {
    let value = client.call("list_teams", json!({})).await?;
    Ok(value
        .as_array()
        .map(|list| list.iter().filter_map(team_of).collect())
        .unwrap_or_default())
}

/// 解析团队：按 id 或名称；`None` 表示只有一个团队时用它。
pub async fn resolve_team(
    client: &ControlClient,
    reference: Option<&str>,
) -> Result<TeamRef, ToolError> {
    let teams = teams(client).await?;
    if teams.is_empty() {
        return Err(ToolError::Message(
            "还没有任何团队，请先在 Teamspace 应用中创建团队。".to_owned(),
        ));
    }
    let Some(reference) = clean(reference) else {
        if teams.len() == 1 {
            return Ok(teams[0].clone());
        }
        return Err(ToolError::Message(format!(
            "有多个团队，请指明是哪一个：{}",
            team_candidates(&teams)
        )));
    };
    match best_match(teams.iter(), |team| {
        let mut score = 0;
        if team.id == reference {
            score = 5;
        } else if team.name == reference {
            score = 4;
        } else if team.name.eq_ignore_ascii_case(reference) {
            score = 3;
        }
        score
    }) {
        Match::One(team) => Ok(team),
        Match::Ambiguous(hits) => Err(ToolError::Message(format!(
            "有多个团队与「{reference}」匹配，请指明是哪一个：{}",
            team_candidates(&hits)
        ))),
        Match::None => Err(ToolError::Message(format!(
            "找不到团队「{reference}」。现有团队：{}",
            team_candidates(&teams)
        ))),
    }
}

/// 解析岗位：按 id、岗位标识（`role_frontend`）、任务编号前缀（`FE`）或名称。
pub async fn resolve_role(
    client: &ControlClient,
    team: &TeamRef,
    reference: &str,
) -> Result<RoleRef, ToolError> {
    let value = client
        .call("list_roles", json!({ "teamId": team.id }))
        .await?;
    let roles: Vec<RoleRef> = value
        .as_array()
        .map(|list| list.iter().filter_map(role_of).collect())
        .unwrap_or_default();
    if roles.is_empty() {
        return Err(ToolError::Message(format!(
            "团队「{}」还没有岗位。",
            team.name
        )));
    }
    let Some(reference) = clean(Some(reference)) else {
        return Err(ToolError::Message("请指明岗位。".to_owned()));
    };
    match best_match(roles.iter(), |role| {
        let mut score = 0;
        if role.id == reference {
            score = 5;
        } else if role.code == reference {
            score = 4;
        } else if role.task_prefix.eq_ignore_ascii_case(reference) {
            score = 3;
        } else if role.name == reference {
            score = 2;
        } else if role.name.eq_ignore_ascii_case(reference) {
            score = 1;
        }
        score
    }) {
        Match::One(role) => Ok(role),
        Match::Ambiguous(hits) => Err(ToolError::Message(format!(
            "团队「{}」里有多个岗位与「{reference}」匹配，请指明是哪一个：{}",
            team.name,
            role_candidates(&hits)
        ))),
        Match::None => Err(ToolError::Message(format!(
            "团队「{}」里找不到岗位「{reference}」。现有岗位：{}",
            team.name,
            role_candidates(&roles)
        ))),
    }
}

/// 解析 Harness：按 id 或名称（名称大小写不敏感时次之）。
pub async fn resolve_harness(
    client: &ControlClient,
    team: &TeamRef,
    reference: &str,
) -> Result<HarnessRef, ToolError> {
    let value = client
        .call("list_harnesses", json!({ "teamId": team.id }))
        .await?;
    let harnesses: Vec<HarnessRef> = value
        .as_array()
        .map(|list| list.iter().filter_map(harness_of).collect())
        .unwrap_or_default();
    if harnesses.is_empty() {
        return Err(ToolError::Message(format!(
            "团队「{}」还没有注册 Harness；可以用 new_harness 顺带注册一个。",
            team.name
        )));
    }
    let Some(reference) = clean(Some(reference)) else {
        return Err(ToolError::Message("请指明 Harness。".to_owned()));
    };
    match best_match(harnesses.iter(), |harness| {
        if harness.id == reference {
            3
        } else if harness.name == reference {
            2
        } else if harness.name.eq_ignore_ascii_case(reference) {
            1
        } else {
            0
        }
    }) {
        Match::One(harness) => Ok(harness),
        Match::Ambiguous(hits) => Err(ToolError::Message(format!(
            "团队「{}」里有多个 Harness 与「{reference}」匹配，请用 id 指明：{}",
            team.name,
            harness_candidates(&hits)
        ))),
        Match::None => Err(ToolError::Message(format!(
            "团队「{}」里找不到 Harness「{reference}」。现有 Harness：{}",
            team.name,
            harness_candidates(&harnesses)
        ))),
    }
}

/// 解析任务：按 id、任务编号（`FE-002`）或标题。
///
/// `team` 给了就只在这个团队里找；没给时在所有团队里找（编号前缀在多个团队重复时给出候选）。
/// 已取消的任务也算候选：否则 Leader 取消后就没法再按编号找到它、也没法改回 todo。
pub async fn resolve_task(
    client: &ControlClient,
    team: Option<&TeamRef>,
    reference: &str,
) -> Result<TaskRef, ToolError> {
    let Some(reference) = clean(Some(reference)) else {
        return Err(ToolError::Message("请指明任务。".to_owned()));
    };
    // 看起来就是 id 的，直接问内核要（省一次全量扫描）。
    if is_uuid(reference) {
        match client
            .call("get_task", json!({ "taskId": reference }))
            .await
        {
            Ok(detail) => {
                if let Some(mut task) = task_of_detail(&detail) {
                    match team {
                        // 指名了团队时，这个 id 必须属于该团队：不能悄悄操作别的团队的任务。
                        Some(team) if task.team_id != team.id => {
                            let owner = team_name_of(client, &task.team_id).await;
                            return Err(ToolError::Message(format!(
                                "任务 {reference} 属于团队「{owner}」，你指定的是「{}」；\
                                 请改用该任务所属团队，或用任务编号指代。",
                                team.name
                            )));
                        }
                        Some(team) => task.team_name = team.name.clone(),
                        None => task.team_name = team_name_of(client, &task.team_id).await,
                    }
                    return Ok(task);
                }
            }
            Err(ControlError::Backend { .. }) => {}
            Err(err) => return Err(err.into()),
        }
    }
    let teams: Vec<TeamRef> = match team {
        Some(team) => vec![team.clone()],
        None => teams(client).await?,
    };
    if teams.is_empty() {
        return Err(ToolError::Message(
            "还没有任何团队，请先在 Teamspace 应用中创建团队。".to_owned(),
        ));
    }
    let mut best = 0u8;
    let mut hits: Vec<TaskRef> = Vec::new();
    let mut candidates: Vec<TaskRef> = Vec::new();
    for team in &teams {
        // 已取消的任务也要能看到：取消后还得能按编号找到并改回 todo。
        let board = client
            .call(
                "get_task_board",
                json!({ "input": { "teamId": team.id, "includeCancelled": true } }),
            )
            .await?;
        let Some(columns) = board.get("columns").and_then(Value::as_array) else {
            continue;
        };
        for column in columns {
            let Some(tasks) = column.get("tasks").and_then(Value::as_array) else {
                continue;
            };
            for summary in tasks {
                let Some(task) = task_of_summary(summary, team) else {
                    continue;
                };
                let score = task_score(&task, reference);
                if score == 0 {
                    candidates.push(task);
                    continue;
                }
                if score > best {
                    best = score;
                    hits.clear();
                }
                if score == best {
                    hits.push(task.clone());
                }
                candidates.push(task);
            }
        }
    }
    if hits.len() == 1 {
        return Ok(hits.remove(0));
    }
    if hits.len() > 1 {
        return Err(ToolError::Message(format!(
            "有多个任务与「{reference}」匹配，请指明是哪一个：{}",
            task_candidates(&hits)
        )));
    }
    Err(ToolError::Message(format!(
        "找不到任务「{reference}」。{}",
        if candidates.is_empty() {
            format!("团队「{}」里还没有任务。", teams[0].name)
        } else {
            format!(
                "现有任务：{}。已取消的任务同样能按编号查到；若查不到，多半是它不在这个团队里，\
                 请用 team 指明它所在的团队。",
                task_candidates(&candidates)
            )
        }
    )))
}

/// 任务最近一次执行（最近的在前）。
pub async fn latest_run(client: &ControlClient, task: &TaskRef) -> Result<Value, ToolError> {
    let value = client
        .call("list_runs", json!({ "taskId": task.id }))
        .await?;
    let runs = value.as_array().cloned().unwrap_or_default();
    runs.into_iter()
        .next()
        .ok_or_else(|| ToolError::Message(format!("任务 {} 还没有执行记录。", task.key)))
}

/// 按 id 取一次执行。
pub async fn run_by_id(client: &ControlClient, run_id: &str) -> Result<Value, ToolError> {
    client
        .call("get_run", json!({ "runId": run_id }))
        .await
        .map_err(|err| match err {
            ControlError::Backend { message, .. } => {
                ToolError::Message(format!("找不到执行「{run_id}」：{message}"))
            }
            other => ToolError::Control(other),
        })
}

/// 团队 id 对应的团队名（找不到就空）。
async fn team_name_of(client: &ControlClient, team_id: &str) -> String {
    teams(client)
        .await
        .ok()
        .and_then(|list| {
            list.into_iter()
                .find(|team| team.id == team_id)
                .map(|team| team.name)
        })
        .unwrap_or_default()
}

/// 空的引用按「没给」处理。
fn clean(reference: Option<&str>) -> Option<&str> {
    reference.map(str::trim).filter(|value| !value.is_empty())
}

/// 取匹配度最高的一项；同分多项视为有歧义。
fn best_match<'a, T>(items: impl Iterator<Item = &'a T>, score: impl Fn(&T) -> u8) -> Match<T>
where
    T: Clone + 'a,
{
    let mut best = 0u8;
    let mut hits: Vec<T> = Vec::new();
    for item in items {
        let value = score(item);
        if value == 0 {
            continue;
        }
        if value > best {
            best = value;
            hits.clear();
        }
        if value == best {
            hits.push(item.clone());
        }
    }
    match hits.len() {
        0 => Match::None,
        1 => Match::One(hits.remove(0)),
        _ => Match::Ambiguous(hits),
    }
}

/// 任务的匹配度：编号优先于标题。
fn task_score(task: &TaskRef, reference: &str) -> u8 {
    if task.id == reference {
        5
    } else if task.key.eq_ignore_ascii_case(reference) {
        4
    } else if task.title == reference {
        2
    } else if task.title.eq_ignore_ascii_case(reference) {
        1
    } else {
        0
    }
}

/// `list_teams` 的一项。
fn team_of(value: &Value) -> Option<TeamRef> {
    let id = value.get("id")?.as_str()?.to_owned();
    let name = value.get("name")?.as_str()?.to_owned();
    Some(TeamRef { id, name })
}

/// `list_roles` 的一项（岗位卡片）。
fn role_of(value: &Value) -> Option<RoleRef> {
    let role = value.get("role").unwrap_or(value);
    Some(RoleRef {
        id: role.get("id")?.as_str()?.to_owned(),
        name: role.get("name")?.as_str()?.to_owned(),
        code: role
            .get("code")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned(),
        task_prefix: role
            .get("taskPrefix")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned(),
    })
}

/// `list_harnesses` 的一项。
fn harness_of(value: &Value) -> Option<HarnessRef> {
    Some(HarnessRef {
        id: value.get("id")?.as_str()?.to_owned(),
        name: value.get("name")?.as_str()?.to_owned(),
    })
}

/// 看板里的一项任务。
fn task_of_summary(value: &Value, team: &TeamRef) -> Option<TaskRef> {
    Some(TaskRef {
        id: value.get("id")?.as_str()?.to_owned(),
        key: value.get("key")?.as_str()?.to_owned(),
        title: value.get("title")?.as_str()?.to_owned(),
        status: value
            .get("status")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned(),
        team_id: team.id.clone(),
        team_name: team.name.clone(),
    })
}

/// 任务详情里的一项任务。
fn task_of_detail(detail: &Value) -> Option<TaskRef> {
    let task = detail.get("task")?;
    Some(TaskRef {
        id: task.get("id")?.as_str()?.to_owned(),
        key: task.get("key")?.as_str()?.to_owned(),
        title: task.get("title")?.as_str()?.to_owned(),
        status: task
            .get("status")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned(),
        team_id: task
            .get("teamId")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned(),
        team_name: String::new(),
    })
}

/// 团队的候选写法。
fn team_candidates(teams: &[TeamRef]) -> String {
    teams
        .iter()
        .map(|team| team.name.clone())
        .collect::<Vec<_>>()
        .join("、")
}

/// 岗位的候选写法。
fn role_candidates(roles: &[RoleRef]) -> String {
    roles
        .iter()
        .map(|role| {
            if role.code.is_empty() {
                role.name.clone()
            } else {
                format!("{}（{}）", role.name, role.code)
            }
        })
        .collect::<Vec<_>>()
        .join("、")
}

/// Harness 的候选写法。
fn harness_candidates(harnesses: &[HarnessRef]) -> String {
    harnesses
        .iter()
        .map(|harness| format!("{}（{}）", harness.name, harness.id))
        .collect::<Vec<_>>()
        .join("、")
}

/// 任务的候选写法：最多列 10 条，并给出候选总数（候选太多时别把上下文塞满）。
fn task_candidates(tasks: &[TaskRef]) -> String {
    const MAX: usize = 10;
    let mut text = tasks
        .iter()
        .take(MAX)
        .map(|task| format!("{} {}", task.key, task.title))
        .collect::<Vec<_>>()
        .join("、");
    if tasks.len() > MAX {
        text.push_str(&format!(
            "（共 {} 个候选，这里只列前 {MAX} 个）",
            tasks.len()
        ));
    }
    text
}

/// 是不是内核生成的 id（UUID v7）。
fn is_uuid(value: &str) -> bool {
    let parts: Vec<&str> = value.split('-').collect();
    parts.len() == 5
        && [8, 4, 4, 4, 12]
            .iter()
            .zip(parts.iter())
            .all(|(len, part)| part.len() == *len && part.chars().all(|c| c.is_ascii_hexdigit()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn uuid_like_ids_are_recognised() {
        assert!(is_uuid("0198c0de-1234-7abc-8def-0123456789ab"));
        assert!(!is_uuid("FE-001"));
        assert!(!is_uuid("前端开发"));
        assert!(!is_uuid(""));
    }

    #[test]
    fn task_numbers_win_over_titles() {
        let task = TaskRef {
            id: "id".into(),
            key: "FE-001".into(),
            title: "登录页".into(),
            status: "todo".into(),
            team_id: "team".into(),
            team_name: "团队".into(),
        };
        assert_eq!(task_score(&task, "FE-001"), 4);
        assert_eq!(task_score(&task, "fe-001"), 4);
        assert_eq!(task_score(&task, "登录页"), 2);
        assert_eq!(task_score(&task, "别的"), 0);
    }

    /// 候选太多时只列 10 条，并给出总数。
    #[test]
    fn only_the_first_ten_candidates_are_listed() {
        let tasks: Vec<TaskRef> = (1..=25)
            .map(|index| TaskRef {
                id: format!("id-{index}"),
                key: format!("FE-{index:03}"),
                title: format!("任务{index}"),
                status: "todo".into(),
                team_id: "team".into(),
                team_name: "团队".into(),
            })
            .collect();

        let text = task_candidates(&tasks);
        assert!(text.contains("FE-001"), "{text}");
        assert!(text.contains("FE-010"), "{text}");
        assert!(!text.contains("FE-011"), "{text}");
        assert!(text.contains("共 25 个候选"), "{text}");

        // 不超过 10 条时不加总数说明。
        let few = task_candidates(&tasks[..3]);
        assert!(!few.contains("共 "), "{few}");
    }
}
