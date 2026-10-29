//! 集成测试：权限请求的自动答复与 Leader 答复（D42、D43），以及 Leader 通过验收。
//!
//! 覆盖：常用命令由规则自动允许、重度危险命令由规则自动拒绝；规则拿不准的交给 Leader（必须写理由），
//! Leader 也拿不准时升级给你；普通文件夹与 Git 工作空间的任务都能由 Leader 通过验收。

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

/// 执行进度里各条权限答复的说明（`decision.reason`）。
fn decisions(env: &Env, run_id: &str) -> String {
    let progress = env
        .ts
        .list_run_updates(teamspace_core::model::RunUpdateQuery {
            run_id: run_id.to_owned(),
            cursor: None,
            limit: None,
        })
        .unwrap();
    serde_json::to_value(&progress).unwrap().to_string()
}

/// 常用命令只涉及执行目录：团队规则当场允许，不打扰 Leader 和你（D43）。
#[test]
fn everyday_commands_are_allowed_by_the_rules() {
    let env = Env::new(false, &[("前端开发", "FE")]);
    let mut client = Client::start(&env.socket);
    let (_, run_id) = start(
        &mut client,
        "跑测试",
        "[mock:permcmd=cargo test --workspace]",
    );
    wait_for_status(&mut client, &run_id, RunStatus::Succeeded);
    let text = decisions(&env, &run_id);
    assert!(text.contains("自动允许"), "{text}");
}

/// 重度危险的命令：团队规则当场拒绝，并记一条活动（D43）。
#[test]
fn dangerous_commands_are_rejected_by_the_rules() {
    let env = Env::new(false, &[("前端开发", "FE")]);
    let mut client = Client::start(&env.socket);
    for command in ["git push origin main", "sudo make install", "git stash"] {
        let (_, run_id) = start(&mut client, command, &format!("[mock:permcmd={command}]"));
        wait_for_status(&mut client, &run_id, RunStatus::Succeeded);
        let text = decisions(&env, &run_id);
        assert!(text.contains("自动拒绝"), "{command}: {text}");
    }
    let rejected = events(&mut client, "permission_auto_answered");
    assert_eq!(rejected.len(), 3, "{rejected:?}");
}

/// 规则拿不准的交给 Leader：必须写理由，理由记进执行记录与活动（D43）。
#[test]
fn the_leader_answers_what_the_rules_leave_with_a_reason() {
    let env = Env::new(false, &[("前端开发", "FE")]);
    let mut client = Client::start(&env.socket);
    let (key, run_id) = start(&mut client, "装依赖", "[mock:permcmd=pnpm install]");
    wait_for_status(&mut client, &run_id, RunStatus::WaitingPermission);

    let missing = client.call(
        "answer_permission",
        json!({ "task": key, "option": "allow", "reason": "  " }),
    );
    assert!(missing.is_error, "{}", missing.text);
    assert!(missing.text.contains("理由"), "{}", missing.text);

    let answered = client.call(
        "answer_permission",
        json!({ "task": key, "option": "allow", "reason": "锁文件已提交，只装项目依赖" }),
    );
    let answered = answered.ok_json();
    assert_eq!(answered["kind"], "allow_once", "{answered}");
    wait_for_status(&mut client, &run_id, RunStatus::Succeeded);
    let text = decisions(&env, &run_id);
    assert!(text.contains("Leader 选择"), "{text}");
    assert!(text.contains("锁文件已提交"), "{text}");
    let logged = events(&mut client, "permission_answered_by_leader");
    assert_eq!(logged.len(), 1, "{logged:?}");
    assert_eq!(logged[0]["actor"], "leader");
    assert_eq!(logged[0]["data"]["reason"], "锁文件已提交，只装项目依赖");
}

/// Leader 拿不准时升级给你：请求继续等待，记一条活动（D43）。
#[test]
fn the_leader_can_escalate_to_the_human() {
    let env = Env::new(false, &[("前端开发", "FE")]);
    let mut client = Client::start(&env.socket);
    let (key, run_id) = start(
        &mut client,
        "读主仓库",
        "[mock:permcmd=cat /Volumes/other/secret-plan.md]",
    );
    wait_for_status(&mut client, &run_id, RunStatus::WaitingPermission);

    let escalated = client.call(
        "escalate_permission",
        json!({ "task": key, "reason": "要读工作空间外的文件，不确定是否涉密" }),
    );
    let escalated = escalated.ok_json();
    assert_eq!(escalated["escalated"], true, "{escalated}");
    let still = client
        .call("run_status", json!({ "run": run_id }))
        .ok_json();
    assert_eq!(still["status"], "waiting_permission", "{still}");
    let logged = events(&mut client, "permission_escalated");
    assert_eq!(logged.len(), 1, "{logged:?}");
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

/// 受阻的任务（例如 Agent 要求登录导致执行失败）登录后可以由 Leader 直接重新开始执行。
#[test]
fn a_blocked_task_can_be_started_again() {
    let env = Env::new(false, &[("前端开发", "FE")]);
    let mut client = Client::start(&env.socket);
    let (key, run_id) = start(&mut client, "交叉评审", "[mock:auth]");
    wait_for_status(&mut client, &run_id, RunStatus::Failed);
    let task = client.call("get_task", json!({ "task": key })).ok_json();
    assert_eq!(task["task"]["status"], "blocked", "{task}");

    let again = client.call("start_run", json!({ "task": key }));
    let again = again.ok_json();
    assert_eq!(again["attempt"], 2, "{again}");
}

/// wait_events：派了任务就等，执行完进入待验收时返回这件事（不用轮询看板）。
#[test]
fn wait_events_returns_when_a_task_needs_the_leader() {
    let env = Env::new(false, &[("前端开发", "FE")]);
    let mut client = Client::start(&env.socket);
    let quiet = client
        .call("wait_events", json!({ "timeout_seconds": 1 }))
        .ok_json();
    assert_eq!(quiet["timedOut"], true, "{quiet}");

    let (key, _) = start(&mut client, "实现登录页", "");
    let woke = client
        .call("wait_events", json!({ "task": key, "timeout_seconds": 60 }))
        .ok_json();
    assert_eq!(woke["timedOut"], false, "{woke}");
    assert_eq!(woke["items"][0]["category"], "review", "{woke}");
    assert_eq!(woke["items"][0]["taskKey"], key.as_str(), "{woke}");

    // only_new：已有的这条不算，等不到新的就超时。
    let none = client
        .call(
            "wait_events",
            json!({ "only_new": true, "timeout_seconds": 1 }),
        )
        .ok_json();
    assert_eq!(none["timedOut"], true, "{none}");
}
