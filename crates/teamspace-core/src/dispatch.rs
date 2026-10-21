//! 命令分发：把「命令名 + JSON 参数」变成内核调用，并划定 Leader 的权限边界（D7、D8、D26）。
//!
//! 桌面应用界面、端到端测试桥接与本机控制 socket 都走这里，区别只在 [`Caller`]：界面与桥接是
//! [`Caller::Human`]，经控制 socket 进来的是 [`Caller::Leader`]。调用者身份随参数一路传到写活动
//! 的地方（[`Teamspace`] 的 `*_as` 方法），不依赖全局可变状态。
//!
//! Leader 只能执行 [`LEADER_ALLOWED`] 里的命令，其余一律拒绝：返回中文错误，并记一条
//! `leader_denied` 活动（操作者为 Leader），让越权尝试在活动页里留痕。

use rusqlite::Connection;
use serde::de::DeserializeOwned;
use serde_json::{Value, json};

use crate::api::{COMMANDS, Teamspace};
use crate::error::{Error, ErrorPayload};
use crate::model::{Actor, EventKind};
use crate::service::event::NewEvent;
use crate::service::{event, harness, role, run as run_svc, task, team};

/// 调用者：谁在调用内核。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Caller {
    /// 在 Teamspace 应用里操作的人，以及端到端测试桥接。
    Human,
    /// Leader：外部会话（Claude Code / Codex 等）经本机控制 socket 操作。
    Leader,
}

impl Caller {
    /// 调用者在活动记录里的身份。
    pub fn actor(self) -> Actor {
        match self {
            Caller::Human => Actor::Human,
            Caller::Leader => Actor::Leader,
        }
    }
}

/// Leader（外部会话）经本机控制 socket 可以执行的操作。其余命令一律拒绝。
///
/// 白名单只在这里定义一次；[`COMMANDS`] 里不在此列表的命令全部拒绝。范围见 D7、D8、D26：
/// - 查看类：团队、岗位、Harness、看板、任务详情、执行与进度、活动、验收视图、待答复权限；
/// - 写操作：创建与编辑任务、任务状态变更（不含置为已完成）、开始与取消执行、退回验收。
pub const LEADER_ALLOWED: &[&str] = &[
    // 查看类
    "list_teams",
    "get_team_overview",
    "list_roles",
    "get_role_folder",
    "get_role_workbench",
    "list_harnesses",
    "get_harness",
    "list_task_statuses",
    "get_task_board",
    "get_task",
    "check_task_dependencies",
    "get_run",
    "list_runs",
    "list_run_updates",
    "list_events",
    "get_review",
    "get_review_file_patch",
    // 创建与编辑任务（含依赖、负责岗位）
    "create_task",
    "update_task",
    // 任务状态变更（不含置为已完成）
    "change_task_status",
    // 开始与取消执行
    "start_run",
    "cancel_run",
    // 退回验收
    "reject_review",
    // 岗位消息（D8：消息由 Leader 发起属正常编排）
    "send_message",
    "list_threads",
    "get_thread",
    // 待我处理（只读：Leader 可以看，但不能记录决策、也不能改通知订阅）
    "get_inbox",
];

/// 分发一条命令：按 `cmd` 找到内核方法，参数取自 `args`。
///
/// 调用者不是 Human 时先过权限边界（[`LEADER_ALLOWED`]）：白名单外的命令被拒绝，并记一条
/// 操作者为 Leader 的活动。未知命令返回「未知命令」，与桥接一致。
pub fn dispatch(
    ts: &Teamspace,
    caller: Caller,
    cmd: &str,
    args: &Value,
) -> Result<Value, ErrorPayload> {
    if !COMMANDS.contains(&cmd) {
        return Err(bad(format!("未知命令：{cmd}")));
    }
    // 应用正在退出：不再接受任何命令（退出窗口里不该再启动执行）。
    if ts.exiting() {
        return Err(bad("应用正在退出，请稍后再试".to_owned()));
    }
    if caller == Caller::Leader && !leader_allowed(cmd, args) {
        return Err(deny(ts, cmd, args));
    }
    let actor = caller.actor();
    match cmd {
        "list_teams" => out(ts.list_teams()),
        "create_team" => out(ts.create_team(arg(args, "input")?)),
        "get_team_overview" => out(ts.get_team_overview(&arg::<String>(args, "teamId")?)),
        "inspect_workspace" => out(ts.inspect_workspace(&arg::<String>(args, "path")?)),
        "list_role_templates" => out(Ok(ts.list_role_templates())),
        "suggest_task_prefix" => out(ts.suggest_task_prefix(arg(args, "input")?)),
        "list_roles" => out(ts.list_roles(&arg::<String>(args, "teamId")?)),
        "create_role" => out(ts.create_role(arg(args, "input")?)),
        "update_role" => out(ts.update_role(arg(args, "input")?)),
        "archive_role" => out(ts.archive_role(&arg::<String>(args, "roleId")?)),
        "bind_role_harness" => out(ts.bind_role_harness(
            &arg::<String>(args, "roleId")?,
            &arg::<String>(args, "harnessId")?,
        )),
        "unbind_role_harness" => out(ts.unbind_role_harness(&arg::<String>(args, "roleId")?)),
        "get_role_folder" => out(ts.get_role_folder(
            &arg::<String>(args, "roleId")?,
            arg::<Option<u32>>(args, "limit")?,
        )),
        "reveal_role_folder" => out(ts.reveal_role_folder(&arg::<String>(args, "roleId")?)),
        "get_role_workbench" => out(ts.get_role_workbench(&arg::<String>(args, "roleId")?)),
        "open_role_path" => out(ts.open_role_path(
            &arg::<String>(args, "roleId")?,
            arg::<Option<String>>(args, "relativePath")?.as_deref(),
        )),
        "list_acp_presets" => out(Ok(ts.list_acp_presets())),
        "list_harnesses" => out(ts.list_harnesses(&arg::<String>(args, "teamId")?)),
        "get_harness" => out(ts.get_harness(&arg::<String>(args, "harnessId")?)),
        "create_harness" => out(ts.create_harness(arg(args, "input")?)),
        "update_harness" => out(ts.update_harness(arg(args, "input")?)),
        "archive_harness" => out(ts.archive_harness(&arg::<String>(args, "harnessId")?)),
        "check_harness" => out(ts.check_harness(&arg::<String>(args, "harnessId")?)),
        "list_task_statuses" => out(Ok(ts.list_task_statuses())),
        "get_task_board" => out(ts.get_task_board(arg(args, "input")?)),
        "get_task" => out(ts.get_task(&arg::<String>(args, "taskId")?)),
        "create_task" => out(ts.create_task_as(actor, arg(args, "input")?)),
        "update_task" => out(ts.update_task_as(actor, arg(args, "input")?)),
        "check_task_dependencies" => out(ts.check_task_dependencies(arg(args, "input")?)),
        "change_task_status" => out(ts.change_task_status_as(actor, arg(args, "input")?)),
        "search_workspace_files" => out(ts.search_workspace_files(arg(args, "input")?)),
        "list_workspace_markdown" => out(ts.list_workspace_markdown(arg(args, "input")?)),
        "read_workspace_markdown" => out(ts.read_workspace_markdown(arg(args, "input")?)),
        "list_tasks_referencing_file" => out(ts.list_tasks_referencing_file(arg(args, "input")?)),
        "start_run" => out(ts.start_run_as(actor, &arg::<String>(args, "taskId")?)),
        "cancel_run" => out(ts.cancel_run_as(actor, &arg::<String>(args, "runId")?)),
        "resume_run" => out(ts.resume_run(&arg::<String>(args, "runId")?)),
        "mark_run_failed" => out(ts.mark_run_failed(&arg::<String>(args, "runId")?)),
        "takeover_run" => out(ts.takeover_run(&arg::<String>(args, "runId")?)),
        "open_run_dir" => out(ts.open_run_dir(&arg::<String>(args, "runId")?)),
        "open_run_file" => out(ts.open_run_file(
            &arg::<String>(args, "runId")?,
            &arg::<String>(args, "path")?,
        )),
        "get_run" => out(ts.get_run(&arg::<String>(args, "runId")?)),
        "list_runs" => out(ts.list_runs(&arg::<String>(args, "taskId")?)),
        "list_run_updates" => out(ts.list_run_updates(arg(args, "input")?)),
        "respond_permission" => out(ts.respond_permission(arg(args, "input")?)),
        "get_review" => {
            let task_id = arg::<String>(args, "taskId")?;
            // Leader 轮询验收视图是只读的：不提交执行目录，也不跟 Human 的验收抢仓库锁。
            out(if caller == Caller::Leader {
                ts.get_review_readonly(&task_id)
            } else {
                ts.get_review(&task_id)
            })
        }
        "get_review_file_patch" => out(ts.get_review_file_patch(
            &arg::<String>(args, "runId")?,
            &arg::<String>(args, "path")?,
        )),
        "approve_review" => out(ts.approve_review(arg(args, "input")?)),
        "reject_review" => out(ts.reject_review_as(actor, arg(args, "input")?)),
        "resolve_merge_manually" => out(ts.resolve_merge_manually(arg(args, "input")?)),
        "rerun_after_conflict" => out(ts.rerun_after_conflict(arg(args, "input")?)),
        "list_events" => out(ts.list_events(arg(args, "input")?)),
        "send_message" => out(ts.send_message_as(actor, arg(args, "input")?)),
        "list_threads" => out(ts.list_threads(arg(args, "input")?)),
        "get_thread" => out(ts.get_thread(&arg::<String>(args, "threadId")?)),
        "retry_message" => out(ts.retry_message(&arg::<String>(args, "messageId")?)),
        "leader_integration" => out(ts.leader_integration(&arg::<String>(args, "teamId")?)),
        "write_workspace_mcp_config" => {
            out(ts.write_workspace_mcp_config(&arg::<String>(args, "teamId")?))
        }
        "get_inbox" => out(ts.get_inbox(arg(args, "input")?)),
        "record_decision" => out(ts.record_decision(arg(args, "input")?)),
        "list_decisions" => out(ts.list_decisions(arg(args, "input")?)),
        "get_notification_settings" => out(ts.get_notification_settings(arg(args, "input")?)),
        "update_notification_settings" => out(ts.update_notification_settings(arg(args, "input")?)),
        other => Err(bad(format!("未知命令：{other}"))),
    }
}

/// Leader 能否执行该命令：命令须在白名单内；变更任务状态时还不能置为已完成。
fn leader_allowed(cmd: &str, args: &Value) -> bool {
    if !LEADER_ALLOWED.contains(&cmd) {
        return false;
    }
    if cmd == "change_task_status" {
        return !changes_to_done(args);
    }
    true
}

/// 变更任务状态的请求是否要把任务置为已完成（Leader 不允许）。
fn changes_to_done(args: &Value) -> bool {
    args.get("input")
        .and_then(|input| input.get("to"))
        .and_then(Value::as_str)
        == Some("done")
}

/// 拒绝越权请求：返回中文错误，并记一条操作者为 Leader 的活动。
///
/// 留痕失败时（数据库忙、定位不到团队）错误里会带上「留痕失败」：拒绝本身一定生效，
/// 但 Human 那边看不到这条记录，Leader 应当知道。
fn deny(ts: &Teamspace, cmd: &str, args: &Value) -> ErrorPayload {
    let message = leader_denied_message(cmd);
    match record_denial(ts, cmd, args, &message) {
        Some(reason) => Error::conflict(format!("{message}（{reason}）")).payload(),
        None => Error::conflict(message).payload(),
    }
}

/// 越权提示：说明是哪个操作被拒，并指向 Teamspace 应用。
fn leader_denied_message(cmd: &str) -> String {
    format!(
        "Leader 无权执行此操作：{}，请由你在 Teamspace 应用中操作",
        operation_label(cmd)
    )
}

/// 每个被拒操作的中文说明（用于越权提示）。命令不在表里时用命令名本身。
fn operation_label(cmd: &str) -> String {
    match cmd {
        "create_team" => "创建团队",
        "inspect_workspace" => "探测工作空间目录",
        "list_role_templates" => "读取岗位示例模板",
        "suggest_task_prefix" => "建议任务前缀",
        "create_role" => "新增岗位",
        "update_role" => "编辑岗位",
        "archive_role" => "归档岗位",
        "bind_role_harness" => "换绑岗位的 Harness",
        "unbind_role_harness" => "解绑岗位的 Harness",
        "reveal_role_folder" => "在文件管理器中显示岗位文件夹",
        "open_role_path" => "打开岗位文件或目录",
        "list_acp_presets" => "读取 ACP 预设",
        "create_harness" => "注册 Harness",
        "update_harness" => "编辑 Harness",
        "archive_harness" => "归档 Harness",
        "check_harness" => "检查 Harness 握手",
        "search_workspace_files" => "搜索工作空间文件",
        "list_workspace_markdown" => "列出工作空间 Markdown",
        "read_workspace_markdown" => "读取工作空间 Markdown",
        "list_tasks_referencing_file" => "反查引用文件的任务",
        "resume_run" => "续接执行",
        "mark_run_failed" => "把执行标记为失败",
        "takeover_run" => "人工接管执行",
        "open_run_dir" => "打开执行目录",
        "open_run_file" => "打开执行文件",
        "respond_permission" => "答复权限请求",
        "approve_review" => "通过验收（合并）",
        "resolve_merge_manually" => "已手动合并并标记完成",
        "rerun_after_conflict" => "合并冲突后重新执行",
        "retry_message" => "重发消息",
        "leader_integration" => "生成 Leader 接入配置",
        "write_workspace_mcp_config" => "写入工作空间 .mcp.json",
        "record_decision" => "记录决策",
        "list_decisions" => "查看决策记录",
        "get_notification_settings" => "查看通知订阅设置",
        "update_notification_settings" => "修改通知订阅设置",
        other => other,
    }
    .to_owned()
}

/// 记一条「越权被拒」活动：操作者为 Leader，挂在能定位到的团队上。
///
/// 留痕失败时写一条 stderr 日志，并返回失败原因（给 Leader 的错误里说明「留痕失败」）：
/// 拒绝一定生效，但活动页上没有痕迹，事后无法追查，Leader 应当知道这件事。
fn record_denial(ts: &Teamspace, cmd: &str, args: &Value, message: &str) -> Option<String> {
    let conn = ts.lock();
    let Some((team_id, task_id, role_id)) = resolve_context(&conn, args) else {
        // 一个团队都没有时无处留痕：还没有团队可挂。
        eprintln!(
            "teamspace: Leader 请求{}被拒绝，但没能留痕（定位不到团队）：{message}",
            operation_label(cmd)
        );
        return Some("留痕失败：定位不到团队，这条越权记录没能写进活动页".to_owned());
    };
    if let Err(err) = event::record_as(
        &conn,
        Actor::Leader,
        NewEvent {
            team_id: &team_id,
            task_id: task_id.as_deref(),
            role_id: role_id.as_deref(),
            kind: EventKind::LeaderDenied,
            summary: format!("Leader 请求{}被拒绝", operation_label(cmd)),
            data: json!({ "command": cmd, "message": message }),
        },
    ) {
        eprintln!(
            "teamspace: Leader 请求{}被拒绝，但留痕失败：{err}",
            operation_label(cmd)
        );
        return Some(format!("留痕失败：{err}"));
    }
    None
}

/// 从参数里定位这次操作涉及的团队、任务与岗位，用于把越权记录挂到正确的团队下。
///
/// 定位不到（例如创建团队这类还没有团队上下文的操作）时记到最近创建的团队，保证留痕可见；
/// 一个团队都没有时返回 `None`（无处可挂）。
fn resolve_context(
    conn: &Connection,
    args: &Value,
) -> Option<(String, Option<String>, Option<String>)> {
    let null = Value::Null;
    let input = args.get("input").unwrap_or(&null);
    for source in [args, input] {
        if let Some(id) = source.get("taskId").and_then(Value::as_str)
            && let Ok(task) = task::get(conn, id)
        {
            return Some((task.team_id, Some(task.id), Some(task.owner_role_id)));
        }
        if let Some(id) = source.get("runId").and_then(Value::as_str)
            && let Ok(run) = run_svc::get(conn, id)
        {
            return Some((run.team_id, Some(run.task_id), Some(run.role_id)));
        }
        if let Some(id) = source.get("roleId").and_then(Value::as_str)
            && let Ok(role) = role::get_active(conn, id)
        {
            return Some((role.team_id, None, Some(role.id)));
        }
        if let Some(id) = source.get("harnessId").and_then(Value::as_str)
            && let Ok(harness) = harness::get_active(conn, id)
        {
            return Some((harness.team_id, None, None));
        }
        if let Some(id) = source.get("teamId").and_then(Value::as_str)
            && let Ok(team) = team::get_active(conn, id)
        {
            return Some((team.id, None, None));
        }
    }
    let teams = team::list(conn).ok()?;
    let team = teams.last()?;
    Some((team.id.clone(), None, None))
}

fn bad(message: String) -> ErrorPayload {
    Error::Invalid(message).payload()
}

fn arg<T: DeserializeOwned>(args: &Value, name: &str) -> Result<T, ErrorPayload> {
    let value = args.get(name).cloned().unwrap_or(Value::Null);
    serde_json::from_value(value).map_err(|err| bad(format!("参数 {name} 无效：{err}")))
}

fn out<T: serde::Serialize>(result: crate::Result<T>) -> Result<Value, ErrorPayload> {
    let value = result.map_err(|err| err.payload())?;
    serde_json::to_value(value).map_err(|err| bad(err.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::TaskStatus;
    use crate::tests::plain_fixture;

    /// 白名单里的每一条都必须是真的存在的命令。
    #[test]
    fn whitelist_only_contains_known_commands() {
        for cmd in LEADER_ALLOWED {
            assert!(COMMANDS.contains(cmd), "白名单里的 {cmd} 不是内核命令");
        }
    }

    /// 白名单逐条覆盖：允许的命令判定为允许，其余命令判定为拒绝。
    #[test]
    fn whitelist_decides_every_command() {
        for cmd in COMMANDS {
            let allowed = LEADER_ALLOWED.contains(cmd);
            assert_eq!(
                leader_allowed(cmd, &Value::Null),
                allowed,
                "{cmd} 的权限判定与白名单不一致"
            );
        }
    }

    /// 明确拒绝的操作逐条覆盖（任务要求点名的那些）。
    #[test]
    fn explicitly_denied_commands_are_rejected() {
        let denied = [
            "create_team",
            "create_role",
            "update_role",
            "archive_role",
            "create_harness",
            "update_harness",
            "archive_harness",
            "bind_role_harness",
            "unbind_role_harness",
            "respond_permission",
            "approve_review",
            "resolve_merge_manually",
            "rerun_after_conflict",
            "takeover_run",
            "open_run_dir",
            "open_run_file",
            "open_role_path",
            "reveal_role_folder",
        ];
        for cmd in denied {
            assert!(
                !leader_allowed(cmd, &Value::Null),
                "{cmd} 应当被 Leader 拒绝"
            );
        }
    }

    /// 每个被拒的命令都有中文说明，越权提示不会露出英文命令名。
    #[test]
    fn denied_commands_have_chinese_labels() {
        for cmd in COMMANDS.iter().filter(|cmd| !LEADER_ALLOWED.contains(cmd)) {
            let label = operation_label(cmd);
            assert_ne!(label, *cmd, "{cmd} 缺少中文说明");
            assert!(
                label
                    .chars()
                    .any(|ch| ('\u{4e00}'..='\u{9fff}').contains(&ch)),
                "{cmd} 的说明不是中文"
            );
        }
    }

    /// 任务状态可以变更，但置为已完成不允许。
    #[test]
    fn changing_status_to_done_is_not_allowed() {
        let to_ready = json!({ "input": { "taskId": "t", "to": "ready" } });
        assert!(leader_allowed("change_task_status", &to_ready));
        let to_done = json!({ "input": { "taskId": "t", "to": "done" } });
        assert!(!leader_allowed("change_task_status", &to_done));
    }

    /// 未知命令与「不是内核命令的命令」都报未知命令。
    #[test]
    fn unknown_command_is_rejected() {
        let f = plain_fixture();
        let err = dispatch(&f.ts, Caller::Leader, "nope", &Value::Null).unwrap_err();
        assert_eq!(err.kind, crate::error::ErrorKind::Invalid);
        assert!(err.message.contains("未知命令"), "{}", err.message);
    }

    /// Leader 越权时返回中文错误，并记一条操作者为 Leader 的活动。
    #[test]
    fn denied_command_returns_error_and_records_activity() {
        let f = plain_fixture();
        let team = f.team("团队");
        let role = f.role(&team.id, "role_frontend");
        let harness = f.harness(&team.id, "Mock", crate::model::AcpPreset::Mock);

        let err = dispatch(
            &f.ts,
            Caller::Leader,
            "bind_role_harness",
            &json!({ "roleId": role.id, "harnessId": harness.id }),
        )
        .unwrap_err();
        assert_eq!(err.kind, crate::error::ErrorKind::Conflict);
        assert!(
            err.message.starts_with("Leader 无权执行此操作："),
            "{}",
            err.message
        );
        assert!(
            err.message.ends_with("请由你在 Teamspace 应用中操作"),
            "{}",
            err.message
        );

        let events = f.ts.list_events(crate::model::EventQuery {
            team_id: team.id.clone(),
            ..Default::default()
        });
        let denied = events
            .unwrap()
            .events
            .into_iter()
            .find(|event| event.kind == crate::model::EventKind::LeaderDenied)
            .expect("越权操作应留下活动");
        assert_eq!(denied.actor, Actor::Leader);
        assert_eq!(denied.data["command"], "bind_role_harness");
    }

    /// 定位不到团队时无处留痕：拒绝仍然生效，但错误里说明「留痕失败」。
    #[test]
    fn a_denial_that_cannot_be_recorded_says_so() {
        let f = plain_fixture();
        // 一个团队都没有：越权记录无处可挂。
        let err = dispatch(&f.ts, Caller::Leader, "bind_role_harness", &json!({})).unwrap_err();
        assert_eq!(err.kind, crate::error::ErrorKind::Conflict);
        assert!(
            err.message.starts_with("Leader 无权执行此操作："),
            "{}",
            err.message
        );
        assert!(err.message.contains("留痕失败"), "{}", err.message);
    }

    /// Leader 创建任务成功，活动记录的操作者是 Leader。
    #[test]
    fn leader_created_task_is_recorded_as_leader() {
        let f = plain_fixture();
        let team = f.team("团队");
        let role = f.role(&team.id, "role_frontend");

        let detail = dispatch(
            &f.ts,
            Caller::Leader,
            "create_task",
            &json!({ "input": {
                "teamId": team.id,
                "title": "Leader 建的任务",
                "description": "",
                "ownerRoleId": role.id,
                "status": "todo",
                "fileRefs": [],
                "dependsOn": [],
            }}),
        )
        .unwrap();
        assert_eq!(detail["task"]["title"], "Leader 建的任务");

        let events = f.ts.list_events(crate::model::EventQuery {
            team_id: team.id.clone(),
            ..Default::default()
        });
        let created = events
            .unwrap()
            .events
            .into_iter()
            .find(|event| event.kind == crate::model::EventKind::TaskCreated)
            .expect("创建任务应留下活动");
        assert_eq!(created.actor, Actor::Leader);
    }

    /// 界面与桥接（Human）不受白名单限制：创建团队照常成功，活动操作者是 Human。
    #[test]
    fn human_caller_is_not_restricted() {
        let f = plain_fixture();
        let team = dispatch(
            &f.ts,
            Caller::Human,
            "create_team",
            &json!({ "input": {
                "name": "人类创建",
                "description": "",
                "workspacePath": f.ws.to_string_lossy(),
                "roles": [],
            }}),
        )
        .unwrap();
        assert_eq!(team["name"], "人类创建");
    }

    /// Leader 不能把任务置为已完成：被拒绝并留痕。
    #[test]
    fn leader_cannot_mark_task_done() {
        let f = plain_fixture();
        let team = f.team("团队");
        let role = f.role(&team.id, "role_frontend");
        let task = f.task(&team.id, &role, "任务");

        let err = dispatch(
            &f.ts,
            Caller::Leader,
            "change_task_status",
            &json!({ "input": { "taskId": task.task.id, "to": "done" } }),
        )
        .unwrap_err();
        assert_eq!(err.kind, crate::error::ErrorKind::Conflict);
        assert!(
            err.message.contains("Leader 无权执行此操作"),
            "{}",
            err.message
        );

        // 状态没有被改动。
        assert_eq!(
            f.ts.get_task(&task.task.id).unwrap().task.status,
            TaskStatus::Todo
        );
    }
}
