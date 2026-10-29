//! 集成测试：真的起一个 Teamspace（临时数据库 + 临时目录里的控制 socket），
//! 再以 stdio 启动 `teamspace-mcp` 二进制，按 MCP 协议调用工具。
//!
//! 覆盖：工具清单、查看类工具、按名称建任务并设依赖、开始执行（Mock Harness）、
//! 查询进度与结果、退回验收、越权调用、应用未运行时的提示。

mod common;

use serde_json::json;
use teamspace_core::model::{CreateTeamInput, NewRole, RunStatus};

use common::{Client, Env, wait_for_status};

/// 一个团队、一个岗位、一个 Mock Harness，以及控制 socket（普通文件夹工作空间）。
fn env() -> Env {
    Env::new(false, &[("前端开发", "FE")])
}

/// 工具清单里有这些名字。
const EXPECTED_TOOLS: &[&str] = &[
    "list_teams",
    "team_overview",
    "list_roles",
    "list_harnesses",
    "task_board",
    "get_task",
    "run_status",
    "run_progress",
    "list_activity",
    "get_review",
    "pending_permissions",
    "list_threads",
    "get_thread",
    "inbox",
    "create_task",
    "update_task",
    "set_dependencies",
    "change_task_status",
    "start_run",
    "cancel_run",
    "reject_review",
    "send_message",
    "wait_reply",
    "create_role",
    "bind_role_harness",
    "approve_review",
    "answer_permission",
    "escalate_permission",
    "wait_events",
];

/// 不该由 Leader 触发的操作（D7、D8、D26、D32、D41、D42）。
const FORBIDDEN_TOOLS: &[&str] = &[
    "unbind_role_harness",
    "create_harness",
    "create_team",
    "resolve_merge_manually",
    "takeover_run",
    // 重发消息、记录决策、通知订阅：都不在 Leader 白名单里（D32）。
    "retry_message",
    "record_decision",
    "list_decisions",
    "get_notification_settings",
    "update_notification_settings",
];

#[test]
fn lists_the_leader_tools() {
    let env = env();
    let mut client = Client::start(&env.socket);
    let tools = client.result("tools/list", json!({})).expect("tools/list");
    let list = tools["tools"].as_array().expect("工具数组");
    let names: Vec<&str> = list
        .iter()
        .filter_map(|tool| tool["name"].as_str())
        .collect();
    for expected in EXPECTED_TOOLS {
        assert!(names.contains(expected), "缺少工具 {expected}：{names:?}");
    }
    for forbidden in FORBIDDEN_TOOLS {
        assert!(
            !names.contains(forbidden),
            "不该提供 {forbidden}：{names:?}"
        );
    }
    // 描述是中文，参数有 JSON Schema。
    let list_teams = list
        .iter()
        .find(|tool| tool["name"] == "list_teams")
        .expect("list_teams");
    let description = list_teams["description"].as_str().unwrap_or_default();
    assert!(description.contains("团队"), "{description}");
    let create = list
        .iter()
        .find(|tool| tool["name"] == "create_task")
        .expect("create_task");
    let schema = &create["inputSchema"];
    assert_eq!(schema["type"], "object", "{schema}");
    let title = schema["properties"]["title"]["description"]
        .as_str()
        .unwrap_or_default();
    assert!(title.contains("标题"), "{title}");
}

#[test]
fn shows_teams_roles_harnesses_and_board() {
    let env = env();
    let mut client = Client::start(&env.socket);

    let teams = client.call("list_teams", json!({}));
    assert!(!teams.is_error, "{}", teams.text);
    let teams = teams.json();
    assert_eq!(teams[0]["name"], "产品团队", "{teams}");

    let roles = client.call("list_roles", json!({}));
    assert!(!roles.is_error, "{}", roles.text);
    let roles = roles.json();
    assert_eq!(roles[0]["role"]["name"], "前端开发", "{roles}");

    let harnesses = client.call("list_harnesses", json!({}));
    assert!(!harnesses.is_error, "{}", harnesses.text);
    let harnesses = harnesses.json();
    assert_eq!(harnesses[0]["name"], "Mock", "{harnesses}");

    let board = client.call("task_board", json!({}));
    assert!(!board.is_error, "{}", board.text);
    let board = board.json();
    assert!(board["columns"].is_array(), "{board}");

    let overview = client.call("team_overview", json!({ "team": "产品团队" }));
    assert!(!overview.is_error, "{}", overview.text);
    let overview = overview.json();
    assert_eq!(overview["team"]["name"], "产品团队", "{overview}");
    assert_eq!(
        overview["roles"][0]["role"]["name"], "前端开发",
        "{overview}"
    );
}

#[test]
fn creates_a_task_by_role_name_and_sets_dependencies() {
    let env = env();
    let mut client = Client::start(&env.socket);

    // 用岗位名称建任务。
    let first = client.call(
        "create_task",
        json!({ "title": "写登录页", "role": "前端开发", "description": "按设计稿实现" }),
    );
    assert!(!first.is_error, "{}", first.text);
    let first = first.json();
    let first_key = first["task"]["key"].as_str().unwrap().to_owned();
    let first_id = first["task"]["id"].as_str().unwrap().to_owned();
    assert!(first_key.starts_with("FE-"), "{first_key}");

    // 用岗位前缀建任务，并按编号写依赖。
    let second = client.call(
        "create_task",
        json!({ "title": "写登录接口", "role": "FE", "depends_on": [first_key] }),
    );
    assert!(!second.is_error, "{}", second.text);
    let second = second.json();
    assert_eq!(second["task"]["dependsOn"][0], first_id, "{second}");
    // 内核里也是同样的依赖。
    let stored = env.ts.get_task(&first_id).unwrap();
    assert_eq!(stored.task.team_id, env.team_id);
    let second_id = second["task"]["id"].as_str().unwrap().to_owned();
    let stored_second = env.ts.get_task(&second_id).unwrap();
    assert_eq!(stored_second.task.depends_on, vec![first_id.clone()]);

    // 给第三个任务设依赖（用编号指代）。
    let third = client.call(
        "create_task",
        json!({ "title": "前后端联调", "role": "前端开发" }),
    );
    assert!(!third.is_error, "{}", third.text);
    let third_key = third.json()["task"]["key"].as_str().unwrap().to_owned();
    let updated = client.call(
        "set_dependencies",
        json!({ "task": third_key, "depends_on": [first_key] }),
    );
    assert!(!updated.is_error, "{}", updated.text);
    let updated = updated.json();
    assert_eq!(updated["task"]["dependsOn"][0], first_id, "{updated}");

    // 编辑任务：只改标题，其他保持原样。
    let edited = client.call(
        "update_task",
        json!({ "task": first_key, "title": "写登录页（含校验）" }),
    );
    assert!(!edited.is_error, "{}", edited.text);
    let edited = edited.json();
    assert_eq!(edited["task"]["title"], "写登录页（含校验）", "{edited}");
    assert_eq!(edited["task"]["description"], "按设计稿实现", "{edited}");
    assert_eq!(edited["task"]["ownerRoleId"], env.role_ids[0], "{edited}");

    // 岗位名解析不到时给出候选。
    let bad = client.call("create_task", json!({ "title": "x", "role": "后端开发" }));
    assert!(bad.is_error, "{}", bad.text);
    assert!(bad.text.contains("前端开发"), "{}", bad.text);

    // 任务名解析不到时给出候选。
    let bad = client.call("get_task", json!({ "task": "不存在的任务" }));
    assert!(bad.is_error, "{}", bad.text);
    assert!(bad.text.contains("FE-001"), "{}", bad.text);
}

#[test]
fn starts_a_run_and_reads_progress_and_result() {
    let env = env();
    let mut client = Client::start(&env.socket);
    let created = client.call(
        "create_task",
        json!({
            "title": "实现登录页",
            "role": "前端开发",
            "description": "写一个页面",
            "status": "ready",
        }),
    );
    assert!(!created.is_error, "{}", created.text);
    let key = created.json()["task"]["key"].as_str().unwrap().to_owned();

    let started = client.call("start_run", json!({ "task": key }));
    assert!(!started.is_error, "{}", started.text);
    let run_id = started.json()["id"].as_str().unwrap().to_owned();

    let finished = wait_for_status(&mut client, &run_id, RunStatus::Succeeded);
    assert_eq!(finished["status"], "succeeded", "{finished}");
    assert!(
        finished["summary"].as_str().is_some_and(|s| !s.is_empty()),
        "{finished}"
    );
    assert!(finished["changedFiles"].is_array(), "{finished}");

    // 进度：能读到条目，也能用游标续读。
    let progress = client.call("run_progress", json!({ "run": run_id }));
    assert!(!progress.is_error, "{}", progress.text);
    let progress = progress.json();
    let updates = progress["updates"].as_array().expect("进度条目");
    assert!(!updates.is_empty(), "{progress}");
    assert!(
        updates.iter().any(|update| update["summary"]
            .as_str()
            .is_some_and(|s| s.contains("Agent"))),
        "{progress}"
    );
    let cursor = progress["nextCursor"].as_str().expect("游标").to_owned();
    let again = client.call("run_progress", json!({ "run": run_id, "cursor": cursor }));
    assert!(!again.is_error, "{}", again.text);
    let again = again.json();
    assert_eq!(again["updates"].as_array().unwrap().len(), 0, "{again}");

    // 任务详情里能看到这次执行。
    let task = client.call("get_task", json!({ "task": key }));
    assert!(!task.is_error, "{}", task.text);
    let task = task.json();
    assert_eq!(task["task"]["status"], "review", "{task}");
    assert_eq!(task["latestRun"]["id"], run_id, "{task}");
}

#[test]
fn rejects_a_review_with_a_comment() {
    let env = env();
    let mut client = Client::start(&env.socket);
    let created = client.call(
        "create_task",
        json!({ "title": "实现登录页", "role": "前端开发", "status": "ready" }),
    );
    assert!(!created.is_error, "{}", created.text);
    let key = created.json()["task"]["key"].as_str().unwrap().to_owned();
    let started = client.call("start_run", json!({ "task": key }));
    assert!(!started.is_error, "{}", started.text);
    let run_id = started.json()["id"].as_str().unwrap().to_owned();
    wait_for_status(&mut client, &run_id, RunStatus::Succeeded);

    // 验收视图能看到改动文件与可退回。
    let review = client.call("get_review", json!({ "task": key }));
    assert!(!review.is_error, "{}", review.text);
    let review = review.json();
    assert_eq!(review["task"]["key"], key, "{review}");
    assert!(review["run"].is_object(), "{review}");
    assert_eq!(review["canReject"], true, "{review}");

    // 退回必须带意见。
    let missing = client.call("reject_review", json!({ "task": key, "comment": "   " }));
    assert!(missing.is_error, "{}", missing.text);
    assert!(missing.text.contains("意见"), "{}", missing.text);

    // 带意见退回：内核会续接会话重做，返回的是新的一次执行。
    let rejected = client.call(
        "reject_review",
        json!({ "task": key, "comment": "按钮颜色不对，改成主色" }),
    );
    assert!(!rejected.is_error, "{}", rejected.text);
    let rejected = rejected.json();
    assert_eq!(rejected["revisionOf"], run_id.as_str(), "{rejected}");
    assert_eq!(rejected["attempt"], 2, "{rejected}");

    // 被退回的那次执行上记着意见。
    let first = client.call("run_status", json!({ "run": run_id }));
    assert!(!first.is_error, "{}", first.text);
    let first = first.json();
    assert_eq!(first["review"]["outcome"], "rejected", "{first}");
    assert_eq!(
        first["review"]["comment"], "按钮颜色不对，改成主色",
        "{first}"
    );

    // 退回后活动里有 Leader 的记录。
    let activity = client.call("list_activity", json!({ "task": key }));
    assert!(!activity.is_error, "{}", activity.text);
    let activity = activity.json();
    let events = activity["events"].as_array().expect("活动");
    assert!(
        events
            .iter()
            .any(|event| event["actor"] == "leader" && event["kind"] == "review_rejected"),
        "{activity}"
    );
}

#[test]
fn lists_pending_permissions_and_answers_them() {
    let env = env();
    let mut client = Client::start(&env.socket);
    let created = client.call(
        "create_task",
        json!({
            "title": "跑一下命令",
            "role": "前端开发",
            "description": "[mock:perm]",
            "status": "ready",
        }),
    );
    assert!(!created.is_error, "{}", created.text);
    let key = created.json()["task"]["key"].as_str().unwrap().to_owned();
    let started = client.call("start_run", json!({ "task": key }));
    assert!(!started.is_error, "{}", started.text);
    let run_id = started.json()["id"].as_str().unwrap().to_owned();
    wait_for_status(&mut client, &run_id, RunStatus::WaitingPermission);

    let pending = client.call("pending_permissions", json!({}));
    assert!(!pending.is_error, "{}", pending.text);
    let pending = pending.json();
    assert_eq!(pending["count"], 1, "{pending}");
    let prompt = &pending["permissions"][0];
    assert_eq!(prompt["runId"], run_id.as_str(), "{pending}");
    assert_eq!(prompt["taskKey"], key.as_str(), "{pending}");
    assert!(prompt["reason"].as_str().is_some(), "{pending}");
    assert!(prompt["options"].is_array(), "{pending}");

    // Leader 答复（普通命令可以放行，D42）。
    let answered = client.call(
        "answer_permission",
        json!({ "task": key, "option": "allow", "reason": "只是 echo，无副作用" }),
    );
    let answered = answered.ok_json();
    assert_eq!(answered["kind"], "allow_once", "{answered}");
    let done = wait_for_status(&mut client, &run_id, RunStatus::Succeeded);
    assert_eq!(done["status"], "succeeded", "{done}");
}

#[test]
fn refuses_operations_the_leader_may_not_do() {
    let env = env();
    let mut client = Client::start(&env.socket);
    let created = client.call(
        "create_task",
        json!({ "title": "写登录页", "role": "前端开发" }),
    );
    assert!(!created.is_error, "{}", created.text);
    let key = created.json()["task"]["key"].as_str().unwrap().to_owned();

    // 不能直接置为已完成：要走验收。
    let done = client.call("change_task_status", json!({ "task": key, "to": "done" }));
    assert!(done.is_error, "{}", done.text);
    assert!(done.text.contains("approve_review"), "{}", done.text);

    // 状态取值不合法。
    let bad = client.call(
        "change_task_status",
        json!({ "task": key, "to": "running" }),
    );
    assert!(bad.is_error, "{}", bad.text);
    assert!(bad.text.contains("todo"), "{}", bad.text);

    // 还没执行的任务不能通过验收。
    let approve = client.call("approve_review", json!({ "task": key }));
    assert!(approve.is_error, "{}", approve.text);
    assert!(approve.text.contains("不能通过验收"), "{}", approve.text);
}

#[test]
fn changes_task_status() {
    let env = env();
    let mut client = Client::start(&env.socket);
    let created = client.call(
        "create_task",
        json!({ "title": "写登录页", "role": "前端开发" }),
    );
    assert!(!created.is_error, "{}", created.text);
    let key = created.json()["task"]["key"].as_str().unwrap().to_owned();

    let ready = client.call("change_task_status", json!({ "task": key, "to": "ready" }));
    assert!(!ready.is_error, "{}", ready.text);
    assert_eq!(ready.json()["task"]["status"], "ready", "{}", ready.text);

    let doing = client.call("change_task_status", json!({ "task": key, "to": "doing" }));
    assert!(!doing.is_error, "{}", doing.text);
    assert_eq!(doing.json()["task"]["status"], "doing", "{}", doing.text);

    // 受阻必须给原因。
    let blocked = client.call(
        "change_task_status",
        json!({ "task": key, "to": "blocked" }),
    );
    assert!(blocked.is_error, "{}", blocked.text);

    let blocked = client.call(
        "change_task_status",
        json!({ "task": key, "to": "blocked", "reason": "等接口" }),
    );
    assert!(!blocked.is_error, "{}", blocked.text);
    assert_eq!(
        blocked.json()["task"]["status"],
        "blocked",
        "{}",
        blocked.text
    );
}

#[test]
fn cancels_a_running_execution() {
    let env = env();
    let mut client = Client::start(&env.socket);
    let created = client.call(
        "create_task",
        json!({
            "title": "跑一下命令",
            "role": "前端开发",
            "description": "[mock:perm]",
            "status": "ready",
        }),
    );
    assert!(!created.is_error, "{}", created.text);
    let key = created.json()["task"]["key"].as_str().unwrap().to_owned();
    let started = client.call("start_run", json!({ "task": key }));
    assert!(!started.is_error, "{}", started.text);
    let run_id = started.json()["id"].as_str().unwrap().to_owned();
    wait_for_status(&mut client, &run_id, RunStatus::WaitingPermission);

    let cancelled = client.call("cancel_run", json!({ "run": run_id }));
    assert!(!cancelled.is_error, "{}", cancelled.text);
    let cancelled = cancelled.json();
    assert_eq!(cancelled["status"], "cancelled", "{cancelled}");

    // 任务回到 READY。
    let task = client.call("get_task", json!({ "task": key }));
    assert!(!task.is_error, "{}", task.text);
    assert_eq!(task.json()["task"]["status"], "ready", "{}", task.text);
}

#[test]
fn says_the_app_is_not_running_when_the_socket_is_missing() {
    let dir = tempfile::tempdir().unwrap();
    let mut client = Client::start(&dir.path().join("teamspace-control.sock"));
    let call = client.call("list_teams", json!({}));
    assert!(call.is_error, "{}", call.text);
    assert!(
        call.text
            .contains("Teamspace 应用没有运行，请先打开 Teamspace"),
        "{}",
        call.text
    );
    // 服务本身不崩：换个工具还是能给出同样的提示。
    let call = client.call("task_board", json!({}));
    assert!(call.is_error, "{}", call.text);
    assert!(
        call.text
            .contains("Teamspace 应用没有运行，请先打开 Teamspace"),
        "{}",
        call.text
    );
}

#[test]
fn shows_the_activity_of_a_task_and_filters_by_role() {
    let env = env();
    let mut client = Client::start(&env.socket);
    let created = client.call(
        "create_task",
        json!({ "title": "写登录页", "role": "前端开发" }),
    );
    assert!(!created.is_error, "{}", created.text);
    let key = created.json()["task"]["key"].as_str().unwrap().to_owned();

    let activity = client.call("list_activity", json!({ "task": key }));
    assert!(!activity.is_error, "{}", activity.text);
    let activity = activity.json();
    let events = activity["events"].as_array().expect("活动");
    assert!(!events.is_empty(), "{activity}");
    assert!(
        events
            .iter()
            .all(|event| event["kind"] == "task_created" || event["taskId"].is_string()),
        "{activity}"
    );

    // 按岗位筛选。
    let by_role = client.call("list_activity", json!({ "role": "前端开发" }));
    assert!(!by_role.is_error, "{}", by_role.text);
    let by_role = by_role.json();
    let events = by_role["events"].as_array().expect("活动");
    assert!(
        events
            .iter()
            .all(|event| event["roleId"] == env.role_ids[0].as_str()),
        "{by_role}"
    );
}

#[test]
fn a_cancelled_task_is_still_found_and_can_go_back_to_todo() {
    let env = env();
    let mut client = Client::start(&env.socket);
    let created = client.call(
        "create_task",
        json!({ "title": "写登录页", "role": "前端开发" }),
    );
    assert!(!created.is_error, "{}", created.text);
    let key = created.json()["task"]["key"].as_str().unwrap().to_owned();

    // 取消任务。
    let cancelled = client.call(
        "change_task_status",
        json!({ "task": key, "to": "cancelled" }),
    );
    assert!(!cancelled.is_error, "{}", cancelled.text);
    assert_eq!(
        cancelled.json()["task"]["status"],
        "cancelled",
        "{}",
        cancelled.text
    );

    // 取消后仍能按编号查到。
    let found = client.call("get_task", json!({ "task": key }));
    assert!(!found.is_error, "{}", found.text);
    assert_eq!(found.json()["task"]["key"], key.as_str(), "{}", found.text);
    assert_eq!(
        found.json()["task"]["status"],
        "cancelled",
        "{}",
        found.text
    );

    // 也能按标题查到。
    let by_title = client.call("get_task", json!({ "task": "写登录页" }));
    assert!(!by_title.is_error, "{}", by_title.text);
    assert_eq!(
        by_title.json()["task"]["key"],
        key.as_str(),
        "{}",
        by_title.text
    );

    // 改回 todo：取消是撤销，不是终局。
    let back = client.call("change_task_status", json!({ "task": key, "to": "todo" }));
    assert!(!back.is_error, "{}", back.text);
    assert_eq!(back.json()["task"]["status"], "todo", "{}", back.text);
}

#[test]
fn a_task_of_another_team_is_never_touched_silently() {
    let env = env();
    let mut client = Client::start(&env.socket);

    // 另一个团队与它自己的任务。
    let other_workspace = env.dir.path().join("ws2");
    std::fs::create_dir_all(&other_workspace).unwrap();
    env.ts
        .create_team(CreateTeamInput {
            name: "另一个团队".into(),
            description: "跨团队检查".into(),
            workspace_path: other_workspace.display().to_string(),
            roles: vec![NewRole {
                name: "后端开发".into(),
                code: None,
                task_prefix: "BE".into(),
                description: String::new(),
                guidelines: String::new(),
                is_leader: false,
            }],
        })
        .unwrap();
    let created = client.call(
        "create_task",
        json!({ "title": "别的团队的任务", "role": "后端开发", "team": "另一个团队" }),
    );
    assert!(!created.is_error, "{}", created.text);
    let other_id = created.json()["task"]["id"].as_str().unwrap().to_owned();

    // 指名了别的团队时，按 id 指代要被挡住，并说清楚是哪个团队。
    let call = client.call("get_task", json!({ "task": other_id, "team": "产品团队" }));
    assert!(call.is_error, "{}", call.text);
    assert!(call.text.contains("产品团队"), "{}", call.text);
    assert!(call.text.contains("另一个团队"), "{}", call.text);

    // 编辑同样被挡住：不能悄悄改别的团队的任务。
    let updated = client.call(
        "update_task",
        json!({ "task": other_id, "team": "产品团队", "title": "被偷偷改了" }),
    );
    assert!(updated.is_error, "{}", updated.text);
    assert_eq!(
        env.ts.get_task(&other_id).unwrap().task.title,
        "别的团队的任务",
        "别的团队的任务不该被改"
    );
}

#[test]
fn file_references_keep_their_anchor() {
    let env = env();
    let mut client = Client::start(&env.socket);
    let created = client.call(
        "create_task",
        json!({
            "title": "按验收标准改",
            "role": "前端开发",
            "file_refs": ["docs/spec.md#验收标准", "README.md"],
        }),
    );
    assert!(!created.is_error, "{}", created.text);
    let created = created.json();
    let id = created["task"]["id"].as_str().unwrap().to_owned();
    let refs = created["task"]["fileRefs"].as_array().expect("文件引用");
    assert_eq!(refs.len(), 2, "{created}");
    assert_eq!(refs[0]["path"], "docs/spec.md", "{created}");
    assert_eq!(refs[0]["anchor"], "验收标准", "{created}");
    assert_eq!(refs[1]["path"], "README.md", "{created}");
    assert!(refs[1]["anchor"].is_null(), "{created}");

    // 编辑时重设引用，锚点不丢。
    let updated = client.call(
        "update_task",
        json!({ "task": id, "file_refs": ["docs/plan.md#里程碑"] }),
    );
    assert!(!updated.is_error, "{}", updated.text);
    let updated = updated.json();
    let refs = updated["task"]["fileRefs"].as_array().expect("文件引用");
    assert_eq!(refs[0]["path"], "docs/plan.md", "{updated}");
    assert_eq!(refs[0]["anchor"], "里程碑", "{updated}");
}

#[test]
fn too_many_candidates_are_summarized() {
    let env = env();
    let mut client = Client::start(&env.socket);
    for index in 1..=12 {
        let created = client.call(
            "create_task",
            json!({ "title": format!("任务{index}"), "role": "前端开发" }),
        );
        assert!(!created.is_error, "{}", created.text);
    }

    // 解析不到时，候选最多列 10 条，并给出总数。
    let call = client.call("get_task", json!({ "task": "不存在的任务" }));
    assert!(call.is_error, "{}", call.text);
    assert!(call.text.contains("共 12 个候选"), "{}", call.text);
    let listed = call.text.matches("FE-").count();
    assert_eq!(listed, 10, "候选最多列 10 条：{}", call.text);
    assert!(call.text.contains("FE-012"), "{}", call.text);
    assert!(!call.text.contains("FE-001"), "{}", call.text);
}
