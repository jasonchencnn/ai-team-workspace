//! 集成测试：Leader 答复权限请求与通过验收（D42）。
//!
//! 覆盖：普通命令可以放行、操作者记为 Leader；重度危险的命令（推送、提权、凭据）只能拒绝，
//! 放行被内核挡下并留越权记录；普通文件夹与 Git 工作空间的任务都能由 Leader 通过验收。

mod common;

use serde_json::{Value, json};
use teamspace_core::model::RunStatus;

use common::{Client, Env, wait_for_status};

/// 建一个任务并开始执行，返回（任务编号，执行 id）。
fn start(client: &mut Client, title: &str, description: &str) -> (String, String) {
    let created = client.call(
        "create_task",
        json!({ "title": title, "role": "前端开发", "description": description, "status": "ready" }),
    );
    let key = created.ok_json()["task"]["key"]
        .as_str()
        .unwrap()
        .to_owned();
    let started = client.call("start_run", json!({ "task": key }));
    let run_id = started.ok_json()["id"].as_str().unwrap().to_owned();
    (key, run_id)
}

/// 活动里某类事件。
fn events(client: &mut Client, kind: &str) -> Vec<Value> {
    let activity = client
        .call("list_activity", json!({ "limit": 200 }))
        .ok_json();
    activity["events"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|event| event["kind"] == kind)
        .cloned()
        .collect()
}

#[test]
fn the_leader_allows_an_ordinary_command() {
    let env = Env::new(false, &[("前端开发", "FE")]);
    let mut client = Client::start(&env.socket);
    let (key, run_id) = start(
        &mut client,
        "跑测试",
        "[mock:permcmd=cargo test --workspace]",
    );
    wait_for_status(&mut client, &run_id, RunStatus::WaitingPermission);

    let answered = client.call(
        "respond_permission",
        json!({ "task": key, "option": "always" }),
    );
    let answered = answered.ok_json();
    assert_eq!(answered["kind"], "allow_always", "{answered}");
    wait_for_status(&mut client, &run_id, RunStatus::Succeeded);

    // 执行进度里的答复记着是 Leader 选的。
    let progress = env
        .ts
        .list_run_updates(teamspace_core::model::RunUpdateQuery {
            run_id: run_id.clone(),
            cursor: None,
            limit: None,
        })
        .unwrap();
    let decision = serde_json::to_value(&progress).unwrap().to_string();
    assert!(decision.contains("Leader 选择"), "{decision}");
}

#[test]
fn heavy_danger_can_only_be_rejected_by_the_leader() {
    let env = Env::new(false, &[("前端开发", "FE")]);
    let mut client = Client::start(&env.socket);
    let (key, run_id) = start(&mut client, "发版", "[mock:permcmd=git push origin main]");
    wait_for_status(&mut client, &run_id, RunStatus::WaitingPermission);

    // 放行被挡下：说明原因，请求仍在等你答复。
    let allow = client.call(
        "respond_permission",
        json!({ "task": key, "option": "allow" }),
    );
    assert!(allow.is_error, "{}", allow.text);
    assert!(allow.text.contains("重度危险"), "{}", allow.text);
    assert!(allow.text.contains("git push"), "{}", allow.text);
    let still = client
        .call("run_status", json!({ "run": run_id }))
        .ok_json();
    assert_eq!(still["status"], "waiting_permission", "{still}");
    let denied = events(&mut client, "leader_denied");
    assert_eq!(denied.len(), 1, "{denied:?}");
    assert_eq!(denied[0]["data"]["command"], "respond_permission");

    // 拒绝总是可以的。
    let reject = client.call(
        "respond_permission",
        json!({ "task": key, "option": "reject" }),
    );
    let reject = reject.ok_json();
    assert_eq!(reject["kind"], "reject_once", "{reject}");
}

#[test]
fn credentials_and_privilege_are_heavy_danger_too() {
    let env = Env::new(false, &[("前端开发", "FE")]);
    let mut client = Client::start(&env.socket);
    for command in ["sudo make install", "cat ~/.ssh/id_ed25519"] {
        let (key, run_id) = start(&mut client, command, &format!("[mock:permcmd={command}]"));
        wait_for_status(&mut client, &run_id, RunStatus::WaitingPermission);
        let allow = client.call(
            "respond_permission",
            json!({ "task": key, "option": "allow" }),
        );
        assert!(allow.is_error, "{command}: {}", allow.text);
        assert!(allow.text.contains("重度危险"), "{}", allow.text);
        client
            .call(
                "respond_permission",
                json!({ "task": key, "option": "reject" }),
            )
            .ok_json();
    }
}

#[test]
fn the_leader_approves_a_review_in_a_plain_folder() {
    let env = Env::new(false, &[("前端开发", "FE")]);
    let mut client = Client::start(&env.socket);
    let (key, run_id) = start(&mut client, "实现登录页", "");
    wait_for_status(&mut client, &run_id, RunStatus::Succeeded);

    let approved = client.call("approve_review", json!({ "task": key }));
    let approved = approved.ok_json();
    assert_eq!(approved["taskStatus"], "done", "{approved}");
    let events = events(&mut client, "review_approved");
    assert_eq!(events.len(), 1, "{events:?}");
    assert_eq!(events[0]["actor"], "leader", "{events:?}");
}

#[test]
fn the_leader_merges_a_git_review() {
    let env = Env::new(true, &[("前端开发", "FE")]);
    let mut client = Client::start(&env.socket);
    let (key, run_id) = start(&mut client, "实现登录页", "");
    wait_for_status(&mut client, &run_id, RunStatus::Succeeded);

    let approved = client.call("approve_review", json!({ "task": key }));
    let approved = approved.ok_json();
    assert_eq!(approved["taskStatus"], "done", "{approved}");
    assert!(approved["mergedCommit"].is_string(), "{approved}");

    // 再通过一次：没有待验收的执行了。
    let again = client.call("approve_review", json!({ "task": key }));
    assert!(again.is_error, "{}", again.text);
}
