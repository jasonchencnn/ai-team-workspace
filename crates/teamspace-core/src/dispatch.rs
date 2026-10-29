//! 命令分发：把「命令名 + JSON 参数」变成内核调用，并划定 Leader 的权限边界（D7、D8、D26）。
//!
//! 桌面应用界面、端到端测试桥接与本机控制 socket 都走这里，区别只在 [`Caller`]：界面与桥接是
//! [`Caller::Human`]，经控制 socket 进来的是 [`Caller::Leader`]。调用者身份随参数一路传到写活动
//! 的地方（[`Teamspace`] 的 `*_as` 方法），不依赖全局可变状态。
//!
//! Leader 只能执行 [`LEADER_ALLOWED`] 里的命令，且不得执行 [`LEADER_DENIED`] 里点名的操作；
//! 其余一律拒绝：返回中文错误，并记一条 `leader_denied` 活动（操作者为 Leader），
//! 让越权尝试在活动页里留痕。

use rusqlite::Connection;
use serde::de::DeserializeOwned;
use serde_json::{Value, json};

use crate::api::{COMMANDS, Teamspace};
use crate::error::{Error, ErrorPayload};
use crate::model::{Actor, EventKind, RebindMode};
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
/// - 写操作：创建与编辑任务、任务状态变更（不含置为已完成）、开始与取消执行、退回验收；
///   新增岗位、注册 Harness、给岗位绑定 Harness（D41：Leader 岗位本身除外）；
///   答复权限请求（D42：重度危险的请求只能拒绝）、通过验收并合并（D42）。
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
    "list_board_column",
    "list_task_options",
    "get_task",
    "check_task_dependencies",
    "get_run",
    "list_runs",
    "list_active_runs",
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
    // 退回验收与通过验收（D42：合并到基线分支，不推送）
    "reject_review",
    "approve_review",
    // 答复权限请求（D42：重度危险的请求 Leader 只能拒绝，见 [`allows_heavy_danger`]）
    "respond_permission",
    "escalate_permission",
    // 岗位消息（D8：消息由 Leader 发起属正常编排）
    "send_message",
    "list_threads",
    "get_thread",
    "list_role_messages",
    // 待我处理（只读：Leader 可以看，但不能记录决策、也不能改通知订阅）
    "get_inbox",
    // 团队自动化设置（只读：自治级别等只由 Human 修改，D43）
    "get_team_automation",
    // 组建团队（D41）：新增岗位、注册 Harness、给岗位绑定 Harness。
    // Leader 岗位本身由 Human 在应用里创建和绑定，见 [`leader_allowed`] 与 [`binds_leader_role`]。
    "suggest_task_prefix",
    "list_acp_presets",
    "create_role",
    "create_harness",
    "bind_role_harness",
];

/// Leader 明确无权执行的操作：即便将来有人误把它加进白名单，这里也会挡住。
///
/// 数据备份（M7-04）在这张名单上：备份里含整个数据库与各团队工作空间的岗位说明与工作日志，
/// 恢复还会覆盖当前数据——这类操作只能由 Human 在应用里发起。
pub const LEADER_DENIED: &[&str] = &[
    "export_backup",
    "inspect_backup",
    "restore_backup",
    // 自治级别本身只由 Human 调（D43）：Leader 不能给自己扩权。
    "update_team_automation",
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
    if caller == Caller::Leader && (!leader_allowed(cmd, args) || binds_leader_role(ts, cmd, args))
    {
        return Err(deny(ts, cmd, args));
    }
    if caller == Caller::Leader
        && let Some(reason) = allows_heavy_danger(ts, cmd, args)
    {
        let message = format!(
            "这个权限请求属于重度危险操作（{reason}），只能由你在 Teamspace 应用中放行；Leader 可以选择拒绝"
        );
        return Err(deny_with(ts, cmd, args, message));
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
        "create_role" => out(ts.create_role_as(actor, arg(args, "input")?)),
        "update_role" => out(ts.update_role(arg(args, "input")?)),
        "archive_role" => out(ts.archive_role(&arg::<String>(args, "roleId")?)),
        "bind_role_harness" => out(ts.bind_role_harness_as(
            actor,
            &arg::<String>(args, "roleId")?,
            &arg::<String>(args, "harnessId")?,
        )),
        "unbind_role_harness" => out(ts.unbind_role_harness(&arg::<String>(args, "roleId")?)),
        "rebind_role" => out(ts.rebind_role(
            &arg::<String>(args, "roleId")?,
            &arg::<String>(args, "harnessInstanceId")?,
            arg::<RebindMode>(args, "mode")?,
        )),
        "cancel_pending_rebind" => out(ts.cancel_pending_rebind(&arg::<String>(args, "roleId")?)),
        "get_role_folder" => out(ts.get_role_folder(
            &arg::<String>(args, "roleId")?,
            arg::<Option<u32>>(args, "limit")?,
        )),
        "reveal_role_folder" => out(ts.reveal_role_folder(&arg::<String>(args, "roleId")?)),
        "get_role_workbench" => out(ts.get_role_workbench(&arg::<String>(args, "roleId")?)),
        "get_role_handoff_preview" => {
            out(ts.get_role_handoff_preview(&arg::<String>(args, "roleId")?))
        }
        "open_role_path" => out(ts.open_role_path(
            &arg::<String>(args, "roleId")?,
            arg::<Option<String>>(args, "relativePath")?.as_deref(),
        )),
        "list_acp_presets" => out(Ok(ts.list_acp_presets())),
        "list_harnesses" => out(ts.list_harnesses(&arg::<String>(args, "teamId")?)),
        "get_harness" => out(ts.get_harness(&arg::<String>(args, "harnessId")?)),
        "create_harness" => out(ts.create_harness_as(actor, arg(args, "input")?)),
        "update_harness" => out(ts.update_harness(arg(args, "input")?)),
        "archive_harness" => out(ts.archive_harness(&arg::<String>(args, "harnessId")?)),
        "check_harness" => out(ts.check_harness(&arg::<String>(args, "harnessId")?)),
        "list_task_statuses" => out(Ok(ts.list_task_statuses())),
        "get_task_board" => out(ts.get_task_board(arg(args, "input")?)),
        "list_board_column" => out(ts.list_board_column(arg(args, "input")?)),
        "list_task_options" => out(ts.list_task_options(&arg::<String>(args, "teamId")?)),
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
        "list_active_runs" => out(ts.list_active_runs(&arg::<String>(args, "roleId")?)),
        "list_run_updates" => out(ts.list_run_updates(arg(args, "input")?)),
        "respond_permission" => out(ts.respond_permission_as(actor, arg(args, "input")?)),
        "escalate_permission" => out(ts.escalate_permission(arg(args, "input")?)),
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
        "approve_review" => out(ts.approve_review_as(actor, arg(args, "input")?)),
        "reject_review" => out(ts.reject_review_as(actor, arg(args, "input")?)),
        "resolve_merge_manually" => out(ts.resolve_merge_manually(arg(args, "input")?)),
        "rerun_after_conflict" => out(ts.rerun_after_conflict(arg(args, "input")?)),
        "list_events" => out(ts.list_events(arg(args, "input")?)),
        "send_message" => out(ts.send_message_as(actor, arg(args, "input")?)),
        "list_threads" => out(ts.list_threads(arg(args, "input")?)),
        "get_thread" => out(ts.get_thread(&arg::<String>(args, "threadId")?)),
        "list_role_messages" => out(ts.list_role_messages(arg(args, "input")?)),
        "retry_message" => out(ts.retry_message(&arg::<String>(args, "messageId")?)),
        "leader_integration" => out(ts.leader_integration(&arg::<String>(args, "teamId")?)),
        "write_workspace_mcp_config" => {
            out(ts.write_workspace_mcp_config(&arg::<String>(args, "teamId")?))
        }
        "get_inbox" => out(ts.get_inbox(arg(args, "input")?)),
        "record_decision" => out(ts.record_decision(arg(args, "input")?)),
        "list_decisions" => out(ts.list_decisions(arg(args, "input")?)),
        "get_notification_settings" => out(ts.get_notification_settings(arg(args, "input")?)),
        "get_team_automation" => out(ts.get_team_automation(&arg::<String>(args, "teamId")?)),
        "update_team_automation" => out(ts.update_team_automation(arg(args, "input")?)),
        "update_notification_settings" => out(ts.update_notification_settings(arg(args, "input")?)),
        "export_backup" => out(ts.export_backup(arg(args, "input")?)),
        "inspect_backup" => out(ts.inspect_backup(arg(args, "input")?)),
        "restore_backup" => out(ts.restore_backup(arg(args, "input")?)),
        other => Err(bad(format!("未知命令：{other}"))),
    }
}

/// Leader 能否执行该命令：命令须在白名单内、且不在明确拒绝名单里；
/// 变更任务状态时还不能置为已完成。
fn leader_allowed(cmd: &str, args: &Value) -> bool {
    if LEADER_DENIED.contains(&cmd) {
        return false;
    }
    if !LEADER_ALLOWED.contains(&cmd) {
        return false;
    }
    if cmd == "change_task_status" {
        return !changes_to_done(args);
    }
    if cmd == "create_role" {
        return !creates_leader_role(args);
    }
    true
}

/// 新增岗位的请求是否要建 Leader 岗位（Leader 不允许，D41）。
fn creates_leader_role(args: &Value) -> bool {
    args.get("input")
        .and_then(|input| input.get("role"))
        .and_then(|role| role.get("isLeader"))
        .and_then(Value::as_bool)
        == Some(true)
}

/// Leader 要放行的权限请求是否属于重度危险（D42）；是的话返回原因。
///
/// 只看「允许」类选项：拒绝总是可以的。执行或选项不存在时不在这里拦，由内核报错。
fn allows_heavy_danger(ts: &Teamspace, cmd: &str, args: &Value) -> Option<String> {
    if cmd != "respond_permission" {
        return None;
    }
    let input = args.get("input")?;
    let run_id = input.get("runId")?.as_str()?;
    let request_id = input.get("requestId")?.as_str()?;
    let option_id = input.get("optionId")?.as_str()?;
    let run = run_svc::get(&ts.lock(), run_id).ok()?;
    // 没有待答复的请求时交给内核报「不存在或已处理」。执行里只存排在最前的那条请求：
    // Leader 答的不是它时核对不了内容，按危险处理（宁严勿松）。
    let prompt = run.pending_permission?;
    if prompt.request_id != request_id {
        return Some("还没轮到这条请求，无法核对内容".to_owned());
    }
    let allows = prompt
        .options
        .iter()
        .find(|option| option.option_id == option_id)
        .is_some_and(|option| option.kind.starts_with("allow"));
    if !allows {
        return None;
    }
    crate::run::danger::heavy_danger(&prompt)
}

/// 绑定 Harness 的请求是否落在 Leader 岗位上（Leader 不允许，D41）。
///
/// 要查库才知道目标岗位是不是 Leader，所以不放在纯判定的 [`leader_allowed`] 里。
/// 岗位不存在时放行，由内核报「找不到岗位」。
fn binds_leader_role(ts: &Teamspace, cmd: &str, args: &Value) -> bool {
    if cmd != "bind_role_harness" {
        return false;
    }
    let Some(role_id) = args.get("roleId").and_then(Value::as_str) else {
        return false;
    };
    crate::service::role::get_active(&ts.lock(), role_id).is_ok_and(|role| role.is_leader)
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
    deny_with(ts, cmd, args, leader_denied_message(cmd))
}

/// 以给定说明拒绝越权请求并留痕（同 [`deny`]）。
fn deny_with(ts: &Teamspace, cmd: &str, args: &Value, message: String) -> ErrorPayload {
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
        "create_role" => "新增 Leader 岗位",
        "update_role" => "编辑岗位",
        "archive_role" => "归档岗位",
        "bind_role_harness" => "绑定 Leader 岗位的 Harness",
        "unbind_role_harness" => "解绑岗位的 Harness",
        "rebind_role" => "更换岗位的 Harness",
        "cancel_pending_rebind" => "撤销待生效换绑",
        "reveal_role_folder" => "在文件管理器中显示岗位文件夹",
        "get_role_handoff_preview" => "预览岗位启动包",
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
        "update_team_automation" => "修改团队自动化设置（自治级别）",
        "update_notification_settings" => "修改通知订阅设置",
        "export_backup" => "导出数据备份",
        "inspect_backup" => "查看数据备份摘要",
        "restore_backup" => "恢复数据备份",
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

    /// 明确拒绝名单里的每一条都必须是真实存在的命令，且不能与白名单重叠。
    #[test]
    fn denied_list_only_contains_known_commands() {
        for cmd in LEADER_DENIED {
            assert!(COMMANDS.contains(cmd), "拒绝名单里的 {cmd} 不是内核命令");
            assert!(
                !LEADER_ALLOWED.contains(cmd),
                "{cmd} 同时在白名单和拒绝名单里"
            );
        }
    }

    /// 白名单逐条覆盖：允许的命令判定为允许，其余命令判定为拒绝。
    #[test]
    fn whitelist_decides_every_command() {
        for cmd in COMMANDS {
            let allowed = LEADER_ALLOWED.contains(cmd) && !LEADER_DENIED.contains(cmd);
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
            "update_role",
            "archive_role",
            "update_harness",
            "archive_harness",
            "unbind_role_harness",
            "rebind_role",
            "cancel_pending_rebind",
            "resolve_merge_manually",
            "rerun_after_conflict",
            "takeover_run",
            "open_run_dir",
            "open_run_file",
            "open_role_path",
            "reveal_role_folder",
            "export_backup",
            "inspect_backup",
            "restore_backup",
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

    /// Leader 可以新增普通岗位，但不能建 Leader 岗位（D41）。
    #[test]
    fn creating_a_leader_role_is_not_allowed() {
        let plain = json!({ "input": { "teamId": "t", "role": { "isLeader": false } } });
        assert!(leader_allowed("create_role", &plain));
        let leader = json!({ "input": { "teamId": "t", "role": { "isLeader": true } } });
        assert!(!leader_allowed("create_role", &leader));
    }

    /// Leader 组建团队：注册 Harness、新增岗位、绑定，活动的操作者都是 Leader（D41）。
    #[test]
    fn leader_staffs_a_role_and_is_recorded_as_leader() {
        let f = plain_fixture();
        let team = f.team("团队");

        let harness = dispatch(
            &f.ts,
            Caller::Leader,
            "create_harness",
            &json!({ "input": { "teamId": team.id, "name": "Mock 甲", "preset": "mock" } }),
        )
        .unwrap();
        let role = dispatch(
            &f.ts,
            Caller::Leader,
            "create_role",
            &json!({ "input": { "teamId": team.id, "role": {
                "name": "数据分析", "taskPrefix": "DA", "description": "", "guidelines": "",
                "isLeader": false,
            }}}),
        )
        .unwrap();
        let bound = dispatch(
            &f.ts,
            Caller::Leader,
            "bind_role_harness",
            &json!({ "roleId": role["role"]["id"], "harnessId": harness["id"] }),
        )
        .unwrap();
        assert_eq!(bound["role"]["harnessId"], harness["id"]);

        let events =
            f.ts.list_events(crate::model::EventQuery {
                team_id: team.id.clone(),
                ..Default::default()
            })
            .unwrap()
            .events;
        for kind in [
            crate::model::EventKind::HarnessCreated,
            crate::model::EventKind::RoleCreated,
            crate::model::EventKind::RoleHarnessBound,
        ] {
            let event = events
                .iter()
                .find(|event| event.kind == kind)
                .unwrap_or_else(|| panic!("缺少 {kind:?} 活动"));
            assert_eq!(event.actor, Actor::Leader, "{kind:?} 的操作者");
        }
    }

    /// Leader 岗位的 Harness 只能由 Human 绑定：Leader 请求被拒并留痕（D41）。
    #[test]
    fn binding_the_leader_role_is_not_allowed() {
        let f = plain_fixture();
        let team = f.team("团队");
        let leader = match f
            .ts
            .list_roles(&team.id)
            .unwrap()
            .into_iter()
            .map(|view| view.role)
            .find(|role| role.is_leader)
        {
            Some(role) => role,
            None => {
                f.ts.create_role(crate::model::CreateRoleInput {
                    team_id: team.id.clone(),
                    role: crate::tests::new_role("Leader", "role_lead", "LD", true),
                })
                .unwrap()
                .role
            }
        };
        let harness = f.harness(&team.id, "Mock", crate::model::AcpPreset::Mock);

        let err = dispatch(
            &f.ts,
            Caller::Leader,
            "bind_role_harness",
            &json!({ "roleId": leader.id, "harnessId": harness.id }),
        )
        .unwrap_err();
        assert_eq!(err.kind, crate::error::ErrorKind::Conflict);
        assert!(err.message.contains("Leader 岗位"), "{}", err.message);
        let after = f.ts.list_roles(&team.id).unwrap();
        let after = after.iter().find(|view| view.role.id == leader.id).unwrap();
        assert_ne!(after.role.harness_id.as_deref(), Some(harness.id.as_str()));
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

        f.ts.bind_role_harness(&role.id, &harness.id).unwrap();

        let err = dispatch(
            &f.ts,
            Caller::Leader,
            "unbind_role_harness",
            &json!({ "roleId": role.id }),
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
        assert_eq!(denied.data["command"], "unbind_role_harness");
    }

    /// 定位不到团队时无处留痕：拒绝仍然生效，但错误里说明「留痕失败」。
    #[test]
    fn a_denial_that_cannot_be_recorded_says_so() {
        let f = plain_fixture();
        // 一个团队都没有：越权记录无处可挂。
        let err = dispatch(&f.ts, Caller::Leader, "unbind_role_harness", &json!({})).unwrap_err();
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

    /// 界面与桥接（Human）可以预览岗位启动包。
    #[test]
    fn human_can_preview_the_role_handoff() {
        let f = plain_fixture();
        let team = f.team("团队");
        let role = f.role(&team.id, "role_frontend");
        let harness = f.harness(&team.id, "Mock", crate::model::AcpPreset::Mock);
        f.ts.bind_role_harness(&role.id, &harness.id).unwrap();
        // 岗位在这个 Harness 上跑过一次，才有历史可交底
        // （从未执行过的新岗位不附加交接节，D35③）。
        let task = f.task(&team.id, &role, "任务");
        f.raw_sql(&format!(
            "UPDATE tasks SET status = 'doing' WHERE id = '{task_id}';
             INSERT INTO runs (id, team_id, task_id, role_id, harness_id, harness_name,
                               harness_preset, harness_command, attempt, status, session_id,
                               workspace_path, exec_dir, isolation, started_at)
             VALUES ('run-1', '{team}', '{task_id}', '{role}', '{harness}', 'Mock', 'mock', 'x', 1,
                     'failed', 's-1', '{ws}', '{ws}', 'in_place', '2026-10-13T00:00:00.000Z');",
            task_id = task.task.id,
            team = team.id,
            role = role.id,
            harness = harness.id,
            ws = f.ws.display(),
        ));
        // 换到另一个 Harness：换了环境才交底。
        let other = f.harness(&team.id, "Mock 二号", crate::model::AcpPreset::Mock);
        f.ts.bind_role_harness(&role.id, &other.id).unwrap();
        let preview = dispatch(
            &f.ts,
            Caller::Human,
            "get_role_handoff_preview",
            &json!({ "roleId": role.id }),
        )
        .unwrap();
        assert!(preview.as_str().unwrap().contains("岗位交接"), "{preview}");
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
