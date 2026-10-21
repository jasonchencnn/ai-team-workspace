//! 端到端：模拟 Leader 通过 MCP 编排团队（`docs/PLAN.md` 第 4 节 M4「可用标志」）。
//!
//! 一个模拟 Leader 的 MCP 客户端（stdio 起 `teamspace-mcp` 二进制）走完真实链路：
//!
//! 1. 看岗位：团队里有「前端」「后端」两个岗位，都绑定内置 Mock 接入；
//! 2. 拆任务：建「登录接口」（后端，READY）与「登录页面」（前端，TODO，依赖「登录接口」）；
//! 3. 派工执行：`start_run` 后轮询 `run_status` 直到待验收，`run_progress` 能读到进度；
//! 4. 验收与退回：`get_review` 看到改动，`reject_review` 带意见退回，续接后再次待验收；
//! 5. 你在应用里点「通过」：测试代码以 Human 身份直接调内核通过验收，下游任务自动就绪；
//! 6. 越权：绕过 MCP 直接经控制 socket 发「通过验收 / 答复权限 / 换绑 Harness」，全被拒，
//!    并在活动里留下操作者为 Leader 的越权记录；
//! 7. 留痕：Leader 的操作在活动里都标为 Leader，你在应用里的操作标为 Human。
//!
//! 不调用任何真实模型、不消耗额度；工作空间是临时 Git 仓库，执行走独立 worktree。

mod common;

use serde_json::{Value, json};
use teamspace_core::model::{ApproveOutcome, ApproveReviewInput, RunStatus, TaskStatus};

use common::{Client, Env, socket_request, wait_for_status, wait_until};

/// 看板某一列里的任务。
fn column_tasks(board: &Value, status: &str) -> Vec<Value> {
    board["columns"]
        .as_array()
        .expect("看板列")
        .iter()
        .filter(|column| column["status"] == status)
        .flat_map(|column| column["tasks"].as_array().cloned().unwrap_or_default())
        .collect()
}

/// 看板某一列里的任务标题。
fn column_titles(board: &Value, status: &str) -> Vec<String> {
    column_tasks(board, status)
        .iter()
        .map(|task| task["title"].as_str().unwrap_or_default().to_owned())
        .collect()
}

/// 活动里某一种记录。
fn events_of(activity: &Value, kind: &str) -> Vec<Value> {
    activity["events"]
        .as_array()
        .expect("活动")
        .iter()
        .filter(|event| event["kind"] == kind)
        .cloned()
        .collect()
}

/// 错误说明要中文、可读（Leader 直接看到这句话）。
fn assert_chinese(text: &str) {
    assert!(
        text.chars()
            .any(|ch| ('\u{4e00}'..='\u{9fff}').contains(&ch)),
        "错误说明应为中文：{text}"
    );
}

#[test]
fn a_leader_orchestrates_the_team_through_mcp() {
    // 临时 Git 工作空间 + 临时数据库 + 控制 socket；两个岗位都绑定内置 Mock 接入。
    let env = Env::new(true, &[("前端", "FE"), ("后端", "BE")]);
    let mut client = Client::start(&env.socket);

    // ---- 1. Leader 先看团队里有哪些岗位 ------------------------------------
    let roles = client.call("list_roles", json!({}));
    let roles = roles.ok_json();
    let mut names: Vec<String> = roles
        .as_array()
        .expect("岗位数组")
        .iter()
        .map(|role| role["role"]["name"].as_str().unwrap_or_default().to_owned())
        .collect();
    names.sort();
    assert_eq!(names, ["前端", "后端"], "{roles}");
    // 两个岗位都绑在 Mock 接入上（Leader 看到的是「谁来干活」）。
    for role in roles.as_array().unwrap() {
        assert_eq!(role["harness"]["name"], "Mock", "{roles}");
    }

    // ---- 2. 拆任务：后端先做接口，前端页面依赖它 ----------------------------
    let backend = client.call(
        "create_task",
        json!({
            "title": "登录接口",
            "role": "后端",
            "description": "实现 POST /login，返回 token",
            "status": "ready",
        }),
    );
    let backend = backend.ok_json();
    let backend_key = backend["task"]["key"].as_str().unwrap().to_owned();
    let backend_id = backend["task"]["id"].as_str().unwrap().to_owned();
    assert!(backend_key.starts_with("BE-"), "{backend_key}");

    // 依赖用标题指代（Leader 说的是「登录接口」）。
    let frontend = client.call(
        "create_task",
        json!({
            "title": "登录页面",
            "role": "前端",
            "description": "按接口实现登录表单",
            "status": "todo",
            "depends_on": ["登录接口"],
        }),
    );
    let frontend = frontend.ok_json();
    let frontend_key = frontend["task"]["key"].as_str().unwrap().to_owned();
    let frontend_id = frontend["task"]["id"].as_str().unwrap().to_owned();
    assert!(frontend_key.starts_with("FE-"), "{frontend_key}");
    assert_eq!(
        frontend["task"]["dependsOn"][0],
        backend_id.as_str(),
        "登录页面应依赖登录接口：{frontend}"
    );
    // 依赖没完成，前端任务还不能开工。
    assert_eq!(frontend["task"]["status"], "todo", "{frontend}");
    let early = client.call("start_run", json!({ "task": frontend_key }));
    assert!(early.is_error, "{}", early.text);
    assert_chinese(&early.text);
    // 拒绝的原因要指明是哪个前置任务挡着。
    assert!(early.text.contains("依赖"), "{}", early.text);
    assert!(early.text.contains(&backend_key), "{}", early.text);

    // ---- 3. 派工执行：开始后端任务，轮询到待验收 ----------------------------
    let started = client.call("start_run", json!({ "task": backend_key }));
    let started = started.ok_json();
    let run_id = started["id"].as_str().unwrap().to_owned();
    let finished = wait_for_status(&mut client, &run_id, RunStatus::Succeeded);
    assert_eq!(finished["status"], "succeeded", "{finished}");
    assert!(
        finished["summary"].as_str().is_some_and(|s| !s.is_empty()),
        "{finished}"
    );
    // 任务进入待验收。
    let task = client.call("get_task", json!({ "task": backend_key }));
    let task = task.ok_json();
    assert_eq!(task["task"]["status"], "review", "{task}");
    assert_eq!(task["latestRun"]["id"], run_id.as_str(), "{task}");

    // 进度能读到（Agent 的消息、计划、工具调用）。
    let progress = client.call("run_progress", json!({ "run": run_id }));
    let progress = progress.ok_json();
    let updates = progress["updates"].as_array().expect("进度条目");
    assert!(!updates.is_empty(), "{progress}");
    assert!(
        updates.iter().any(|update| update["summary"]
            .as_str()
            .is_some_and(|s| s.contains("Agent"))),
        "{progress}"
    );

    // ---- 4. 验收与退回 ------------------------------------------------------
    let review = client.call("get_review", json!({ "task": backend_key }));
    let review = review.ok_json();
    let files = review["files"].as_array().expect("改动文件");
    assert!(!files.is_empty(), "验收页应看到改动：{review}");
    assert_eq!(review["canReject"], true, "{review}");
    assert!(
        review["summary"].as_str().is_some_and(|s| !s.is_empty()),
        "{review}"
    );

    // 退回必须带意见（错误信息是中文、可读）。
    let missing = client.call(
        "reject_review",
        json!({ "task": backend_key, "comment": "   " }),
    );
    assert!(missing.is_error, "{}", missing.text);
    assert!(missing.text.contains("意见"), "{}", missing.text);
    assert_chinese(&missing.text);

    // 带意见退回：内核续接会话重做，返回新的一次执行。
    let rejected = client.call(
        "reject_review",
        json!({ "task": backend_key, "comment": "接口要返回错误码，改一下" }),
    );
    let rejected = rejected.ok_json();
    assert_eq!(rejected["revisionOf"], run_id.as_str(), "{rejected}");
    assert_eq!(rejected["attempt"], 2, "{rejected}");
    let revision_id = rejected["id"].as_str().unwrap().to_owned();

    // 续接后再次待验收。
    let revised = wait_for_status(&mut client, &revision_id, RunStatus::Succeeded);
    assert_eq!(revised["status"], "succeeded", "{revised}");
    wait_until("任务回到待验收", || {
        let task = client.call("get_task", json!({ "task": backend_key }));
        task.ok_json()["task"]["status"] == "review"
    });
    // 被退回的那次执行上记着意见。
    let first = client.call("run_status", json!({ "run": run_id }));
    let first = first.ok_json();
    assert_eq!(first["review"]["outcome"], "rejected", "{first}");
    assert_eq!(
        first["review"]["comment"], "接口要返回错误码，改一下",
        "{first}"
    );

    // ---- 5. 你在应用里点「通过」（测试代码以 Human 身份直接调内核） ---------
    let view = env.ts.get_review(&backend_id).unwrap();
    assert!(view.can_approve, "应当可以通过：{:?}", view.problems);
    let head = view.head_commit.clone().expect("验收页上的执行分支 head");
    let approved = env
        .ts
        .approve_review(ApproveReviewInput {
            run_id: revision_id.clone(),
            expected_head: Some(head),
        })
        .unwrap();
    assert_eq!(approved.outcome, ApproveOutcome::Merged);
    assert_eq!(approved.task.status, TaskStatus::Done);

    // 下游任务自动就绪：Leader 在看板上能看到「登录页面」已经是 READY。
    let board = client.call("task_board", json!({}));
    let board = board.ok_json();
    assert_eq!(
        column_titles(&board, "ready"),
        ["登录页面"],
        "下游任务应自动就绪：{board}"
    );
    let ready = column_tasks(&board, "ready");
    assert_eq!(ready[0]["id"], frontend_id.as_str(), "{board}");
    assert!(column_titles(&board, "done").contains(&"登录接口".to_owned()));

    // ---- 6. 越权：绕过 MCP 直接问内核，全被拒 -------------------------------
    // MCP 侧根本没有这些工具。
    for tool in ["approve_review", "respond_permission", "bind_role_harness"] {
        let call = client.call(tool, json!({}));
        assert!(
            call.protocol_error.is_some(),
            "{tool} 不该由 Leader 调用：{}",
            call.text
        );
    }
    // 也不能把任务直接置为已完成（完成只能走验收，由你在应用里点）。
    let done = client.call(
        "change_task_status",
        json!({ "task": frontend_key, "to": "done" }),
    );
    assert!(done.is_error, "{}", done.text);
    assert!(done.text.contains("应用"), "{}", done.text);
    assert_chinese(&done.text);

    // 直接经控制 socket 发同样的命令：内核按 Leader 身份拒绝。
    let attempts = [
        (
            "approve_review",
            json!({ "input": { "runId": revision_id } }),
            "通过验收（合并）",
        ),
        (
            "respond_permission",
            json!({ "input": { "runId": revision_id, "requestId": "r1", "optionId": "allow" } }),
            "答复权限请求",
        ),
        (
            "bind_role_harness",
            json!({ "roleId": env.role_ids[0], "harnessId": env.harness_id }),
            "换绑岗位的 Harness",
        ),
    ];
    for (cmd, args, label) in attempts {
        let reply = socket_request(&env.socket, cmd, args);
        assert!(reply.get("ok").is_none(), "{cmd} 应被拒绝：{reply}");
        let message = reply["err"]["message"].as_str().unwrap_or_default();
        assert!(
            message.starts_with("Leader 无权执行此操作："),
            "{cmd} 的拒绝说明应为中文：{message}"
        );
        assert!(message.contains(label), "{cmd}: {message}");
        assert_chinese(message);
    }

    // ---- 7. 活动留痕：Leader 的操作标为 Leader，越权也有记录 -----------------
    let activity = client.call("list_activity", json!({ "team": "产品团队", "limit": 200 }));
    let activity = activity.ok_json();

    // 越权记录：操作者是 Leader，data.command 是被拒的命令。
    let denied = events_of(&activity, "leader_denied");
    assert_eq!(denied.len(), 3, "{activity}");
    for event in &denied {
        assert_eq!(event["actor"], "leader", "{event}");
    }
    let denied_commands: Vec<String> = denied
        .iter()
        .map(|event| {
            event["data"]["command"]
                .as_str()
                .unwrap_or_default()
                .to_owned()
        })
        .collect();
    for cmd in ["approve_review", "respond_permission", "bind_role_harness"] {
        assert!(
            denied_commands.iter().any(|c| c == cmd),
            "越权记录里应有 {cmd}：{denied_commands:?}"
        );
    }

    // Leader 的正常操作：建任务、开始执行、退回验收，操作者都是 Leader。
    let created = events_of(&activity, "task_created");
    let leader_task = created
        .iter()
        .find(|event| event["taskId"] == backend_id.as_str())
        .expect("建任务的活动");
    assert_eq!(leader_task["actor"], "leader", "{leader_task}");
    let rejected_event = events_of(&activity, "review_rejected");
    assert_eq!(rejected_event.len(), 1, "{activity}");
    assert_eq!(rejected_event[0]["actor"], "leader", "{rejected_event:?}");
    let started_event = events_of(&activity, "run_started");
    assert!(
        started_event.iter().all(|event| event["actor"] == "leader"),
        "{started_event:?}"
    );

    // 你在应用里的操作（通过验收）标为 Human。
    let approved_event = events_of(&activity, "review_approved");
    assert_eq!(approved_event.len(), 1, "{activity}");
    assert_eq!(approved_event[0]["actor"], "human", "{approved_event:?}");
}
