//! 集成测试：Leader 通过 MCP 给岗位发消息、等回复、查线程与看待我处理。
//!
//! 真的起一个 Teamspace（临时数据库 + 临时目录里的控制 socket + 内置 Mock Agent），
//! 再以 stdio 启动 `teamspace-mcp` 二进制，按 MCP 协议调用工具。
//! Mock Agent 用 `[mock:reply=…]` 与 `[mock:replyfail]` 标记模拟回复与失败，
//! 不调用任何真实模型、不消耗额度。

mod common;

use serde_json::{Value, json};
use teamspace_core::model::{CreateTeamInput, NewRole};

use common::{Client, Env, raw_sql};

/// 一个团队、一个岗位、一个 Mock Harness，以及控制 socket。
fn env() -> Env {
    Env::new(false, &[("前端开发", "FE")])
}

/// 发一条消息，返回内核里的那条消息。
fn send(client: &mut Client, to: &str, body: &str) -> Value {
    let call = client.call(
        "send_message",
        json!({ "to": to, "body": body, "subject": "请确认进度" }),
    );
    assert!(!call.is_error, "{}", call.text);
    call.json()
}

/// 等一条消息的回复。
fn wait_reply(client: &mut Client, message_id: &str, timeout_seconds: u64) -> Value {
    let call = client.call(
        "wait_reply",
        json!({ "message": message_id, "timeout_seconds": timeout_seconds }),
    );
    assert!(!call.is_error, "{}", call.text);
    call.json()
}

/// 另一个团队：用来验证跨团队的操作会被挡住。
fn other_team(env: &Env) -> (String, String) {
    let workspace = env.dir.path().join("ws2");
    std::fs::create_dir_all(&workspace).unwrap();
    let team = env
        .ts
        .create_team(CreateTeamInput {
            name: "另一个团队".into(),
            description: "跨团队检查".into(),
            workspace_path: workspace.display().to_string(),
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
    let role_id = env
        .ts
        .list_roles(&team.id)
        .unwrap()
        .into_iter()
        .next()
        .expect("岗位")
        .role
        .id;
    (team.id, role_id)
}
/// 发消息 → 岗位回复 → wait_reply 拿到回复。
#[test]
fn sends_a_message_and_waits_for_the_reply() {
    let env = env();
    let mut client = Client::start(&env.socket);

    let sent = send(&mut client, "前端开发", "[mock:reply=收到，我来跟进]");
    let message_id = sent["messageId"].as_str().expect("消息 id").to_owned();
    let thread_id = sent["threadId"].as_str().expect("线程 id").to_owned();
    assert!(!message_id.is_empty(), "{sent}");
    assert!(!thread_id.is_empty(), "{sent}");
    assert_eq!(sent["status"], "queued", "{sent}");
    assert_eq!(sent["statusLabel"], "排队中", "{sent}");
    assert_eq!(sent["toRoleName"], "前端开发", "{sent}");

    // 等回复：轮询到「已回复」为止。
    let waited = wait_reply(&mut client, &message_id, 30);
    assert_eq!(waited["status"], "replied", "{waited}");
    assert_eq!(waited["statusLabel"], "已回复", "{waited}");
    assert_eq!(waited["replyBody"], "收到，我来跟进", "{waited}");
    assert_eq!(waited["timedOut"], false, "{waited}");
    assert_eq!(waited["threadId"], thread_id.as_str(), "{waited}");

    // 线程详情里能看到这条消息与它的回复。
    let thread = client.call("get_thread", json!({ "thread": thread_id }));
    assert!(!thread.is_error, "{}", thread.text);
    let thread = thread.json();
    assert_eq!(thread["subject"], "请确认进度", "{thread}");
    let messages = thread["messages"].as_array().expect("消息列表");
    assert_eq!(messages.len(), 1, "{thread}");
    assert_eq!(messages[0]["messageId"], message_id.as_str(), "{thread}");
    assert_eq!(messages[0]["sender"], "leader", "{thread}");
    assert_eq!(messages[0]["senderLabel"], "Leader", "{thread}");
    assert_eq!(messages[0]["status"], "replied", "{thread}");
    assert_eq!(messages[0]["replyBody"], "收到，我来跟进", "{thread}");
}

/// Agent 出错时消息转「失败」，失败原因带给 Leader。
#[test]
fn a_failed_delivery_is_reported_with_its_reason() {
    let env = env();
    let mut client = Client::start(&env.socket);

    let sent = send(&mut client, "前端开发", "[mock:replyfail]");
    let message_id = sent["messageId"].as_str().expect("消息 id").to_owned();

    let waited = wait_reply(&mut client, &message_id, 30);
    assert_eq!(waited["status"], "failed", "{waited}");
    assert_eq!(waited["statusLabel"], "失败", "{waited}");
    assert_eq!(waited["timedOut"], false, "{waited}");
    let reason = waited["failureReason"].as_str().unwrap_or_default();
    assert!(reason.contains("Agent 出错"), "{reason}");

    // 线程里也记着失败原因。
    let thread_id = waited["threadId"].as_str().expect("线程 id").to_owned();
    let thread = client.call("get_thread", json!({ "thread": thread_id }));
    assert!(!thread.is_error, "{}", thread.text);
    let thread = thread.json();
    let messages = thread["messages"].as_array().expect("消息列表");
    assert_eq!(messages[0]["status"], "failed", "{thread}");
    assert!(
        messages[0]["failureReason"]
            .as_str()
            .is_some_and(|reason| reason.contains("Agent 出错")),
        "{thread}"
    );
}

/// 岗位可以按名称、前缀或 id 指代。
#[test]
fn resolves_the_receiving_role_by_name_prefix_and_id() {
    let env = env();
    let mut client = Client::start(&env.socket);

    // 按岗位名称。
    let by_name = send(&mut client, "前端开发", "[mock:reply=按名称]");
    assert_eq!(by_name["toRoleId"], env.role_ids[0].as_str(), "{by_name}");

    // 按任务编号前缀。
    let by_prefix = send(&mut client, "FE", "[mock:reply=按前缀]");
    assert_eq!(
        by_prefix["toRoleId"],
        env.role_ids[0].as_str(),
        "{by_prefix}"
    );

    // 按岗位 id。
    let by_id = send(&mut client, &env.role_ids[0], "[mock:reply=按 id]");
    assert_eq!(by_id["toRoleId"], env.role_ids[0].as_str(), "{by_id}");

    // 岗位解析不到时给出候选。
    let bad = client.call("send_message", json!({ "to": "后端开发", "body": "在吗" }));
    assert!(bad.is_error, "{}", bad.text);
    assert!(bad.text.contains("前端开发"), "{}", bad.text);

    // 正文不能为空。
    let empty = client.call("send_message", json!({ "to": "前端开发", "body": "  " }));
    assert!(empty.is_error, "{}", empty.text);
    assert!(empty.text.contains("正文"), "{}", empty.text);
}

/// 关联任务可以按编号指代；别的团队的任务不能被悄悄用上。
#[test]
fn links_a_task_by_number_and_refuses_another_team() {
    let env = env();
    let mut client = Client::start(&env.socket);

    // 本团队的任务：按编号关联。
    let created = client.call(
        "create_task",
        json!({ "title": "写登录页", "role": "前端开发" }),
    );
    assert!(!created.is_error, "{}", created.text);
    let key = created.json()["task"]["key"].as_str().unwrap().to_owned();
    let task_id = created.json()["task"]["id"].as_str().unwrap().to_owned();

    let sent = client.call(
        "send_message",
        json!({ "to": "前端开发", "body": "看下这个任务", "task": key }),
    );
    assert!(!sent.is_error, "{}", sent.text);
    let sent = sent.json();
    assert_eq!(sent["taskId"], task_id.as_str(), "{sent}");
    assert_eq!(sent["taskKey"], key.as_str(), "{sent}");

    // 现在有第二个团队了。
    let (_other_team_id, other_role_id) = other_team(&env);

    // 别的团队的任务：按 id 指代要被挡住，并说清楚是哪个团队。
    let other_task = client.call(
        "create_task",
        json!({ "title": "别的团队的任务", "role": "后端开发", "team": "另一个团队" }),
    );
    assert!(!other_task.is_error, "{}", other_task.text);
    let other_task_id = other_task.json()["task"]["id"].as_str().unwrap().to_owned();
    let crossed = client.call(
        "send_message",
        json!({
            "to": "前端开发",
            "body": "看下这个任务",
            "task": other_task_id,
            "team": "产品团队",
        }),
    );
    assert!(crossed.is_error, "{}", crossed.text);
    assert!(crossed.text.contains("产品团队"), "{}", crossed.text);
    assert!(crossed.text.contains("另一个团队"), "{}", crossed.text);

    // 别的团队的岗位：不能直接发给它。
    let crossed_role = client.call(
        "send_message",
        json!({ "to": other_role_id, "body": "在吗", "team": "产品团队" }),
    );
    assert!(crossed_role.is_error, "{}", crossed_role.text);
    assert!(
        crossed_role.text.contains("前端开发"),
        "{}",
        crossed_role.text
    );
}

/// 线程列表能按团队、岗位与任务筛选。
#[test]
fn lists_threads_and_filters_them_by_role_and_task() {
    let env = env();
    let mut client = Client::start(&env.socket);
    let created = client.call(
        "create_task",
        json!({ "title": "写登录页", "role": "前端开发" }),
    );
    assert!(!created.is_error, "{}", created.text);
    let key = created.json()["task"]["key"].as_str().unwrap().to_owned();

    let with_task = send(&mut client, "前端开发", "[mock:reply=带任务的消息]");
    assert!(with_task["messageId"].is_string(), "{with_task}");
    let linked = client.call(
        "send_message",
        json!({
            "to": "前端开发",
            "body": "[mock:reply=带任务的消息]",
            "task": key,
            "subject": "请确认进度",
        }),
    );
    assert!(!linked.is_error, "{}", linked.text);

    // 按任务筛选：只留下关联该任务的那个线程。
    let by_task = client.call("list_threads", json!({ "task": key }));
    assert!(!by_task.is_error, "{}", by_task.text);
    let by_task = by_task.json();
    assert_eq!(by_task["count"], 1, "{by_task}");
    let threads = by_task["threads"].as_array().expect("线程列表");
    assert_eq!(threads[0]["taskId"], linked.json()["taskId"], "{by_task}");
    assert_eq!(threads[0]["subject"], "请确认进度", "{by_task}");
    let participants = threads[0]["participants"].as_array().expect("参与方");
    assert!(
        participants
            .iter()
            .any(|participant| participant["name"] == "前端开发"),
        "{by_task}"
    );

    // 按岗位筛选：同一个岗位参与的线程都在。
    let by_role = client.call("list_threads", json!({ "role": "前端开发" }));
    assert!(!by_role.is_error, "{}", by_role.text);
    let by_role = by_role.json();
    assert_eq!(by_role["count"], 2, "{by_role}");

    // 不筛选：两个线程。
    let all = client.call("list_threads", json!({}));
    assert!(!all.is_error, "{}", all.text);
    let all = all.json();
    assert_eq!(all["count"], 2, "{all}");

    // 别的团队的任务不是本团队的线程。
    other_team(&env);
    let other = client.call("list_threads", json!({ "team": "另一个团队" }));
    assert!(!other.is_error, "{}", other.text);
    assert_eq!(other.json()["count"], 0, "{}", other.text);
}

/// 待我处理：投递失败的消息要出现在「需要拍板」里，并说明要人在应用里处理。
#[test]
fn shows_the_inbox_with_a_hint_to_handle_it_in_the_app() {
    let env = env();
    let mut client = Client::start(&env.socket);

    let empty = client.call("inbox", json!({}));
    assert!(!empty.is_error, "{}", empty.text);
    let empty = empty.json();
    assert_eq!(empty["counts"]["total"], 0, "{empty}");
    assert_eq!(empty["team"], "产品团队", "{empty}");
    let hint = empty["hint"].as_str().unwrap_or_default();
    assert!(hint.contains("approve_review"), "{hint}");
    assert!(hint.contains("answer_permission"), "{hint}");
    assert!(hint.contains("决策"), "{hint}");
    assert!(hint.contains("应用"), "{hint}");

    // 投递失败的消息要出现在待我处理里。
    let sent = send(&mut client, "前端开发", "[mock:replyfail]");
    let message_id = sent["messageId"].as_str().expect("消息 id").to_owned();
    let waited = wait_reply(&mut client, &message_id, 30);
    assert_eq!(waited["status"], "failed", "{waited}");

    let inbox = client.call("inbox", json!({}));
    assert!(!inbox.is_error, "{}", inbox.text);
    let inbox = inbox.json();
    assert_eq!(inbox["counts"]["total"], 1, "{inbox}");
    assert_eq!(inbox["counts"]["decision"], 1, "{inbox}");
    let items = inbox["items"].as_array().expect("事项");
    assert_eq!(items[0]["category"], "decision", "{inbox}");
    assert_eq!(items[0]["categoryLabel"], "需要拍板", "{inbox}");
    assert_eq!(items[0]["messageId"], message_id.as_str(), "{inbox}");
    assert!(
        items[0]["title"]
            .as_str()
            .is_some_and(|title| title.contains("前端开发")),
        "{inbox}"
    );
}

/// 等回复时不占住别的工具调用：消息还在排队时，别的工具照样能调。
#[test]
fn waiting_for_a_reply_does_not_block_other_tools() {
    let env = env();
    // 停掉投递线程：这条消息停在「排队」，wait_reply 只能等到超时。
    env.ts.shutdown();
    let mut client = Client::start(&env.socket);

    let sent = send(&mut client, "前端开发", "在吗");
    let message_id = sent["messageId"].as_str().expect("消息 id").to_owned();

    // 不等 wait_reply 的响应，先把它发出去。
    let wait_id = client.call_async(
        "wait_reply",
        json!({ "message": message_id, "timeout_seconds": 3 }),
    );
    // 等待期间别的工具照样能用，并且先回来。
    let teams = client.call("list_teams", json!({}));
    assert!(!teams.is_error, "{}", teams.text);
    assert!(!client.response_received(wait_id), "wait_reply 不该先结束");

    // 最后 wait_reply 到超时：状态仍是排队。
    let waited = client.tool_result(wait_id);
    assert!(!waited.is_error, "{}", waited.text);
    let waited = waited.json();
    assert_eq!(waited["status"], "queued", "{waited}");
    assert_eq!(waited["timedOut"], true, "{waited}");
    assert_eq!(waited["messageId"], message_id.as_str(), "{waited}");
    // 实际生效的等待时长要告诉 Leader。
    assert_eq!(waited["timeoutSeconds"], 3, "{waited}");
    // 整个调用不超过生效的超时（解析也算在等待时间里）。
    let waited_seconds = waited["waitedSeconds"].as_u64().unwrap_or_default();
    assert!(waited_seconds <= 4, "{waited}");
}

/// 消息过期（终态）时，wait_reply 直接给「已过期」，并说清过期原因。
#[test]
fn an_expired_message_is_reported_as_expired() {
    let env = env();
    // 停掉投递线程：这条消息停在「排队」，不会被岗位回复。
    env.ts.shutdown();
    let mut client = Client::start(&env.socket);

    let sent = send(&mut client, "前端开发", "很久以前的消息");
    let message_id = sent["messageId"].as_str().expect("消息 id").to_owned();

    // 「过期」是内核自己才会产生的数据（超过 7 天未回复）：测试直接把它改出来。
    raw_sql(
        &env.db_path(),
        &format!("UPDATE messages SET status = 'expired' WHERE id = '{message_id}'"),
    );

    let waited = wait_reply(&mut client, &message_id, 30);
    assert_eq!(waited["status"], "expired", "{waited}");
    assert_eq!(waited["statusLabel"], "已过期", "{waited}");
    assert_eq!(waited["messageId"], message_id.as_str(), "{waited}");
    // 过期是终态：不是等超时。
    assert_eq!(waited["timedOut"], false, "{waited}");
    assert_eq!(waited["timeoutSeconds"], 30, "{waited}");
    let hint = waited["hint"].as_str().unwrap_or_default();
    assert!(hint.contains("过期"), "{hint}");

    // 线程详情里也是「已过期」。
    let thread_id = waited["threadId"].as_str().expect("线程 id").to_owned();
    let thread = client.call("get_thread", json!({ "thread": thread_id }));
    assert!(!thread.is_error, "{}", thread.text);
    let thread = thread.json();
    let messages = thread["messages"].as_array().expect("消息列表");
    assert_eq!(messages[0]["status"], "expired", "{thread}");
    assert_eq!(messages[0]["statusLabel"], "已过期", "{thread}");
}

/// 超过上限的超时被钳制到 600 秒，结果里能看到实际生效的值。
#[test]
fn the_effective_timeout_is_reported_back() {
    let env = env();
    let mut client = Client::start(&env.socket);

    let sent = send(&mut client, "前端开发", "[mock:reply=收到，我来跟进]");
    let message_id = sent["messageId"].as_str().expect("消息 id").to_owned();

    // 要 900 秒：内核按 600 秒算，结果里写明实际生效的值。
    let waited = wait_reply(&mut client, &message_id, 900);
    assert_eq!(waited["status"], "replied", "{waited}");
    assert_eq!(waited["timeoutSeconds"], 600, "{waited}");
    assert_eq!(waited["timedOut"], false, "{waited}");
}

/// 查线程时给了空 id 或不存在的线程，要给出中文说明。
#[test]
fn asking_for_an_empty_or_unknown_thread_says_so_in_chinese() {
    let env = env();
    let mut client = Client::start(&env.socket);

    // 空字符串：告诉 Leader 要给线程 id。
    let empty = client.call("get_thread", json!({ "thread": "  " }));
    assert!(empty.is_error, "{}", empty.text);
    assert!(empty.text.contains("线程"), "{}", empty.text);
    assert!(empty.text.contains("list_threads"), "{}", empty.text);

    // 不存在的线程：内核说线程不存在。
    let unknown = client.call(
        "get_thread",
        json!({ "thread": "0198c0de-1234-7abc-8def-0123456789ab" }),
    );
    assert!(unknown.is_error, "{}", unknown.text);
    assert!(unknown.text.contains("线程"), "{}", unknown.text);
    assert!(unknown.text.contains("不存在"), "{}", unknown.text);
}

/// 等回复时找不到消息要说清楚，并给出下一步。
#[test]
fn waiting_for_an_unknown_message_says_so() {
    let env = env();
    let mut client = Client::start(&env.socket);

    let call = client.call(
        "wait_reply",
        json!({ "message": "0198c0de-1234-7abc-8def-0123456789ab" }),
    );
    assert!(call.is_error, "{}", call.text);
    assert!(call.text.contains("找不到消息"), "{}", call.text);
    assert!(call.text.contains("产品团队"), "{}", call.text);
}

/// 新工具的描述与参数说明是中文，Leader 才知道怎么填。
#[test]
fn the_new_tools_describe_themselves_in_chinese() {
    let env = env();
    let mut client = Client::start(&env.socket);
    let tools = client.result("tools/list", json!({})).expect("tools/list");
    let list = tools["tools"].as_array().expect("工具数组");
    let find = |name: &str| {
        list.iter()
            .find(|tool| tool["name"] == name)
            .unwrap_or_else(|| panic!("缺少工具 {name}"))
            .clone()
    };

    let send = find("send_message");
    let description = send["description"].as_str().unwrap_or_default();
    assert!(description.contains("消息"), "{description}");
    let to = send["inputSchema"]["properties"]["to"]["description"]
        .as_str()
        .unwrap_or_default();
    assert!(to.contains("岗位"), "{to}");
    let body = send["inputSchema"]["properties"]["body"]["description"]
        .as_str()
        .unwrap_or_default();
    assert!(body.contains("正文"), "{body}");

    let wait = find("wait_reply");
    let timeout = wait["inputSchema"]["properties"]["timeout_seconds"]["description"]
        .as_str()
        .unwrap_or_default();
    assert!(timeout.contains("60"), "{timeout}");
    assert!(timeout.contains("600"), "{timeout}");

    let inbox = find("inbox");
    let inbox_description = inbox["description"].as_str().unwrap_or_default();
    assert!(inbox_description.contains("验收"), "{inbox_description}");
    assert!(inbox_description.contains("应用"), "{inbox_description}");
}
