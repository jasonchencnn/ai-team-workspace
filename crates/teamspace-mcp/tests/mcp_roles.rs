//! 集成测试：Leader 经 MCP 组建团队（D41）——新建岗位、绑定已注册的 Harness、
//! 顺带注册新的 Harness、给已有岗位补绑；Leader 岗位本身不能由 Leader 创建或绑定。

mod common;

use serde_json::{Value, json};
use teamspace_core::model::{CreateRoleInput, NewRole};

use common::{Client, Env};

fn env() -> Env {
    Env::new(false, &[("前端开发", "FE")])
}

/// 团队里按名称找岗位卡片。
fn role_named(env: &Env, name: &str) -> Option<teamspace_core::model::RoleView> {
    env.ts
        .list_roles(&env.team_id)
        .unwrap()
        .into_iter()
        .find(|view| view.role.name == name)
}

/// 活动里某类事件的操作者。
fn actors_of(client: &mut Client, kind: &str) -> Vec<Value> {
    let activity = client
        .call("list_activity", json!({ "limit": 200 }))
        .ok_json();
    activity["events"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|event| event["kind"] == kind)
        .map(|event| event["actor"].clone())
        .collect()
}

#[test]
fn creates_a_role_bound_to_a_registered_harness() {
    let env = env();
    let mut client = Client::start(&env.socket);

    let created = client.call(
        "create_role",
        json!({
            "name": "后端开发",
            "task_prefix": "be",
            "description": "负责接口",
            "harness": "Mock",
        }),
    );
    let created = created.ok_json();
    assert_eq!(created["name"], "后端开发", "{created}");
    assert_eq!(created["taskPrefix"], "BE", "{created}");
    assert_eq!(
        created["harness"]["id"],
        env.harness_id.as_str(),
        "{created}"
    );
    assert_eq!(created["harness"]["registered"], false, "{created}");

    let view = role_named(&env, "后端开发").expect("岗位应已建好");
    assert_eq!(
        view.role.harness_id.as_deref(),
        Some(env.harness_id.as_str())
    );
    assert!(!view.role.is_leader);
    assert_eq!(actors_of(&mut client, "role_created"), [json!("leader")]);
    assert!(
        actors_of(&mut client, "role_harness_bound").contains(&json!("leader")),
        "绑定的操作者应是 Leader"
    );
}

#[test]
fn registers_a_new_harness_and_suggests_a_prefix() {
    let env = env();
    let mut client = Client::start(&env.socket);

    let created = client.call(
        "create_role",
        json!({
            "name": "Data Analyst",
            "new_harness": { "name": "Mock 分析", "preset": "mock" },
        }),
    );
    let created = created.ok_json();
    assert_eq!(created["harness"]["name"], "Mock 分析", "{created}");
    assert_eq!(created["harness"]["registered"], true, "{created}");
    assert!(
        !created["taskPrefix"]
            .as_str()
            .unwrap_or_default()
            .is_empty(),
        "省略前缀时应自动建议：{created}"
    );

    let harnesses = env.ts.list_harnesses(&env.team_id).unwrap();
    assert!(harnesses.iter().any(|harness| harness.name == "Mock 分析"));
    // 夹具里你注册的 Mock 是 human，这次顺带注册的是 leader。
    let actors = actors_of(&mut client, "harness_created");
    assert!(actors.contains(&json!("leader")), "{actors:?}");
}

#[test]
fn a_role_without_harness_is_left_unassigned() {
    let env = env();
    let mut client = Client::start(&env.socket);

    let created = client.call(
        "create_role",
        json!({ "name": "设计", "task_prefix": "DS" }),
    );
    let created = created.ok_json();
    assert!(created["harness"].is_null(), "{created}");
    assert!(
        created["hint"]
            .as_str()
            .unwrap_or_default()
            .contains("bind_role_harness"),
        "{created}"
    );

    // 之后用 bind_role_harness 补绑。
    let bound = client.call(
        "bind_role_harness",
        json!({ "role": "DS", "harness": "Mock" }),
    );
    let bound = bound.ok_json();
    assert_eq!(bound["harness"]["id"], env.harness_id.as_str(), "{bound}");
}

#[test]
fn bad_input_is_reported_before_the_role_is_created() {
    let env = env();
    let mut client = Client::start(&env.socket);

    // 找不到的 Harness：列出候选，岗位不建。
    let missing = client.call(
        "create_role",
        json!({ "name": "运维", "harness": "不存在" }),
    );
    assert!(missing.is_error, "{}", missing.text);
    assert!(missing.text.contains("Mock"), "{}", missing.text);
    // 写错的预设：列出可选预设，岗位不建。
    let preset = client.call(
        "create_role",
        json!({ "name": "运维", "new_harness": { "name": "X", "preset": "nope" } }),
    );
    assert!(preset.is_error, "{}", preset.text);
    assert!(preset.text.contains("claude_code"), "{}", preset.text);
    // 两个都给：拒绝。
    let both = client.call(
        "create_role",
        json!({
            "name": "运维",
            "harness": "Mock",
            "new_harness": { "name": "X", "preset": "mock" },
        }),
    );
    assert!(both.is_error, "{}", both.text);
    assert!(role_named(&env, "运维").is_none(), "校验失败时不该建岗位");

    // 前缀冲突由内核拒绝。
    let taken = client.call(
        "create_role",
        json!({ "name": "前端二组", "task_prefix": "FE" }),
    );
    assert!(taken.is_error, "{}", taken.text);
    assert!(taken.text.contains("FE"), "{}", taken.text);
}

#[test]
fn the_leader_role_stays_with_the_human() {
    let env = env();
    let leader = env
        .ts
        .create_role(CreateRoleInput {
            team_id: env.team_id.clone(),
            role: NewRole {
                name: "Leader".into(),
                code: None,
                task_prefix: "LD".into(),
                description: String::new(),
                guidelines: String::new(),
                is_leader: true,
            },
        })
        .unwrap();
    let mut client = Client::start(&env.socket);

    // MCP 新建的岗位一律不是 Leader。
    let created = client.call(
        "create_role",
        json!({ "name": "测试", "task_prefix": "QA" }),
    );
    created.ok_json();
    assert!(!role_named(&env, "测试").unwrap().role.is_leader);

    // Leader 岗位的绑定：经 MCP 被拒，绑定不变。
    let bind = client.call(
        "bind_role_harness",
        json!({ "role": "LD", "harness": "Mock" }),
    );
    assert!(bind.is_error, "{}", bind.text);
    assert!(bind.text.contains("Leader 岗位"), "{}", bind.text);
    let after = role_named(&env, "Leader").unwrap();
    assert_eq!(after.role.harness_id, leader.role.harness_id);

    // 绕过 MCP 直接让内核建 Leader 岗位：同样被拒。
    let reply = common::socket_request(
        &env.socket,
        "create_role",
        json!({ "input": { "teamId": env.team_id, "role": {
            "name": "第二个 Leader", "taskPrefix": "LX", "description": "", "guidelines": "",
            "isLeader": true,
        }}}),
    );
    assert!(reply.get("ok").is_none(), "{reply}");
    let message = reply["err"]["message"].as_str().unwrap_or_default();
    assert!(message.contains("新增 Leader 岗位"), "{message}");
}
