//! M4 Leader 编排独立回归测试（QA-M4）。
//!
//! 18 个独立用例，聚焦现有测试未覆盖的边界场景：
//! 1. `qa_ambiguous_role_names_returns_candidate_hints_and_disambiguates`：同名岗位解析歧义与消歧
//! 2. `qa_ambiguous_task_titles_returns_candidate_hints_and_disambiguates`：同名任务标题解析歧义与消歧
//! 3. `qa_chinese_english_mixed_resolution_and_case_insensitivity`：中英文混排、大小写匹配与文件锚点保留
//! 4. `qa_leader_forbidden_command_casing_variations_rejected`：大小写命令名绕过尝试全被拒
//! 5. `qa_leader_spoofed_extra_arguments_cannot_bypass_privileges`：伪造额外参数（actor/caller 等）无法越权并留痕
//! 6. `qa_leader_cannot_create_task_or_dispatch_with_archived_role`：针对已归档岗位建任务在 MCP 与内核均被拒
//! 7. `qa_cross_team_task_and_dependency_isolation`：跨团队操作与依赖隔离拒绝
//! 8. `qa_task_status_transition_boundaries_invalid_paths`：任务状态机非法路径流转拦截与合法流转
//! 9. `qa_task_status_blocked_requires_reason_and_clears_on_resume`：进入受阻必须带原因，离开受阻自动清空
//! 10. `qa_dependency_cycle_detection_via_mcp_reports_complete_loop`：MCP 设置依赖成环时返回完整环路编号链条
//! 11. `qa_run_lifecycle_running_and_cancellation_via_mcp`：执行中状态查询与通过 MCP 取消执行
//! 12. `qa_run_lifecycle_waiting_permission_and_failure_reporting`：待答复权限与执行失败状态查询及详情
//! 13. `qa_run_progress_cursor_pagination_and_exhaustion`：run_progress 游标续读、分页与末尾穷尽
//! 14. `qa_socket_occupied_by_regular_file_handled_gracefully`：控制 socket 路径被普通文件占用时的清晰提示与容错
//! 15. `qa_socket_in_use_by_another_process_refuses_overwrite`：socket 被已有进程监听时拒绝覆盖，退出时清理
//! 16. `qa_activity_audit_log_strict_actor_attribution_leader_vs_human`：活动日志中 Leader 与 Human 的严格归属区分
//! 17. `qa_update_task_partial_edits_preserve_unspecified_fields`：任务局部编辑时不影响未指定字段与引用锚点
//! 18. `qa_empty_and_whitespace_only_arguments_validation`：空白与非法输入参数的校验拒绝

mod common;

use serde_json::json;
use std::sync::Arc;
use teamspace_core::model::{
    Actor, CreateRoleInput, CreateTaskInput, CreateTeamInput, NewRole, RunStatus, TaskStatus,
};

use common::{Client, Env, socket_request, wait_for_status, wait_until};

// ============================================================================
// 1. 名称 / 编号 / 标题解析与消歧
// ============================================================================

/// 用例 1：团队内存在同名岗位时，Leader 用岗位名指代触发歧义提示，包含候选代号；
/// 随后使用唯一前缀或代号能精确消歧并成功建任务。
#[test]
fn qa_ambiguous_role_names_returns_candidate_hints_and_disambiguates() {
    let env = Env::new(false, &[("测试工程师", "QA"), ("测试工程师", "TEST")]);
    let mut client = Client::start(&env.socket);

    // 1. Leader 用同名岗位名称建任务，返回歧义错误并列出候选
    let ambiguous = client.call(
        "create_task",
        json!({
            "title": "编写测试用例",
            "role": "测试工程师",
            "description": "验证边界条件",
        }),
    );
    assert!(ambiguous.is_error, "重名岗位应返回错误：{}", ambiguous.text);
    assert!(
        ambiguous.text.contains("有多个岗位与「测试工程师」匹配"),
        "应提示岗位歧义：{}",
        ambiguous.text
    );
    assert!(
        ambiguous.text.contains("role_") || ambiguous.text.contains("QA"),
        "候选提示中应包含岗位代号：{}",
        ambiguous.text
    );

    // 2. 用前缀 QA 消歧成功创建
    let by_prefix_qa = client.call(
        "create_task",
        json!({
            "title": "编写冒烟测试",
            "role": "QA",
        }),
    );
    assert!(!by_prefix_qa.is_error, "{}", by_prefix_qa.text);
    let qa_key = by_prefix_qa.json()["task"]["key"]
        .as_str()
        .unwrap()
        .to_owned();
    assert!(qa_key.starts_with("QA-"), "前缀应为 QA：{qa_key}");

    // 3. 用前缀 TEST 消歧成功创建
    let by_prefix_test = client.call(
        "create_task",
        json!({
            "title": "编写压力测试",
            "role": "TEST",
        }),
    );
    assert!(!by_prefix_test.is_error, "{}", by_prefix_test.text);
    let test_key = by_prefix_test.json()["task"]["key"]
        .as_str()
        .unwrap()
        .to_owned();
    assert!(test_key.starts_with("TEST-"), "前缀应为 TEST：{test_key}");
}

/// 用例 2：同一团队内存在相同标题的任务时，Leader 按标题查询触发歧义提示；
/// 随后按任务编号查询能精确消歧。
#[test]
fn qa_ambiguous_task_titles_returns_candidate_hints_and_disambiguates() {
    let env = Env::new(false, &[("开发", "DEV")]);
    let mut client = Client::start(&env.socket);

    // 创建两个相同标题的任务
    let task1 = client.call(
        "create_task",
        json!({ "title": "重构认证模块", "role": "DEV" }),
    );
    assert!(!task1.is_error, "{}", task1.text);
    let key1 = task1.json()["task"]["key"].as_str().unwrap().to_owned();

    let task2 = client.call(
        "create_task",
        json!({ "title": "重构认证模块", "role": "DEV" }),
    );
    assert!(!task2.is_error, "{}", task2.text);
    let key2 = task2.json()["task"]["key"].as_str().unwrap().to_owned();

    // 1. Leader 用重名标题查询，返回歧义错误并列出候选
    let ambiguous = client.call("get_task", json!({ "task": "重构认证模块" }));
    assert!(ambiguous.is_error, "同名任务应返回错误：{}", ambiguous.text);
    assert!(
        ambiguous.text.contains("有多个任务与「重构认证模块」匹配"),
        "应提示任务歧义：{}",
        ambiguous.text
    );
    assert!(
        ambiguous.text.contains(&key1) && ambiguous.text.contains(&key2),
        "候选提示中应包含任务编号：{}",
        ambiguous.text
    );

    // 2. 用具体编号查询，消歧成功
    let found1 = client.call("get_task", json!({ "task": key1 }));
    assert!(!found1.is_error, "{}", found1.text);
    assert_eq!(found1.json()["task"]["key"], key1.as_str());

    let found2 = client.call("get_task", json!({ "task": key2 }));
    assert!(!found2.is_error, "{}", found2.text);
    assert_eq!(found2.json()["task"]["key"], key2.as_str());
}

/// 用例 3：中英文混排任务标题、小写编号指代，以及带锚点的中文文件引用解析与保存。
#[test]
fn qa_chinese_english_mixed_resolution_and_case_insensitivity() {
    let env = Env::new(false, &[("核心研发", "CORE")]);
    let mut client = Client::start(&env.socket);

    let created = client.call(
        "create_task",
        json!({
            "title": "实现 OAuth2.0 Token 刷新 API",
            "role": "CORE",
            "description": "支持 JWT Bearer 规范",
            "file_refs": ["docs/api/OAuth_Spec.md#认证接口"],
        }),
    );
    assert!(!created.is_error, "{}", created.text);
    let key = created.json()["task"]["key"].as_str().unwrap().to_owned();

    // 1. 任务编号小写匹配（如 core-001 匹配 CORE-001）
    let lower_key = key.to_ascii_lowercase();
    let by_lower_key = client.call("get_task", json!({ "task": lower_key }));
    assert!(!by_lower_key.is_error, "{}", by_lower_key.text);
    assert_eq!(by_lower_key.json()["task"]["key"], key.as_str());

    // 2. 标题英文部分大小写混排匹配
    let mixed_title = "实现 oauth2.0 token 刷新 api";
    let by_mixed_title = client.call("get_task", json!({ "task": mixed_title }));
    assert!(!by_mixed_title.is_error, "{}", by_mixed_title.text);
    assert_eq!(by_mixed_title.json()["task"]["key"], key.as_str());

    // 3. 中英文混排文件引用及其锚点完整保留
    let task_detail = by_mixed_title.json();
    let refs = task_detail["task"]["fileRefs"]
        .as_array()
        .expect("fileRefs 数组");
    assert_eq!(refs.len(), 1);
    assert_eq!(refs[0]["path"], "docs/api/OAuth_Spec.md");
    assert_eq!(refs[0]["anchor"], "认证接口");
}

// ============================================================================
// 2. Leader 越权防范与留痕
// ============================================================================

/// 用例 4：通过大小写、驼峰等命令名变体尝试越权调用，均被识别为未知命令拦截。
#[test]
fn qa_leader_forbidden_command_casing_variations_rejected() {
    let env = Env::new(false, &[("前端", "FE")]);

    let attempts = [
        ("APPROVE_REVIEW", "全大写命令"),
        ("Approve_Review", "大驼峰命令"),
        ("approveReview", "小驼峰命令"),
        ("START_RUN", "白名单命令的全大写变体"),
        ("Create_Task", "白名单命令的首字母大写变体"),
    ];

    for (cmd, label) in attempts {
        let reply = socket_request(&env.socket, cmd, json!({}));
        assert!(
            reply.get("ok").is_none(),
            "{label}（{cmd}）不应执行成功：{reply}"
        );
        let err_message = reply["err"]["message"].as_str().unwrap_or_default();
        assert!(
            err_message.contains("未知命令"),
            "{label}（{cmd}）应返回未知命令错误：{err_message}"
        );
    }
}

/// 用例 5：Leader 试图在参数中注入伪造身份（actor=human, caller=Human, force=true），
/// 控制层严格按连接身份拒绝，并在活动表中留下 leader_denied 审计记录。
#[test]
fn qa_leader_spoofed_extra_arguments_cannot_bypass_privileges() {
    let env = Env::new(false, &[("前端", "FE")]);

    let attempts = [
        (
            "approve_review",
            json!({
                "input": { "runId": "fake-run-id" },
                "actor": "human",
                "caller": "Human",
                "role": "admin",
                "force": true,
            }),
            "通过验收（合并）",
        ),
        (
            "respond_permission",
            json!({
                "input": { "runId": "fake-run-id", "requestId": "req-1", "optionId": "allow" },
                "actor": "human",
                "isLeader": false,
            }),
            "答复权限请求",
        ),
        (
            "bind_role_harness",
            json!({
                "roleId": env.role_ids[0],
                "harnessId": env.harness_id,
                "actor": "human",
                "override": true,
            }),
            "换绑岗位的 Harness",
        ),
    ];

    for (cmd, args, label) in attempts {
        let reply = socket_request(&env.socket, cmd, args);
        assert!(
            reply.get("ok").is_none(),
            "{cmd} 注入伪造参数后不应被放行：{reply}"
        );
        let message = reply["err"]["message"].as_str().unwrap_or_default();
        assert!(
            message.starts_with("Leader 无权执行此操作："),
            "{cmd} 应返回越权拒绝提示：{message}"
        );
        assert!(message.contains(label), "{cmd}: {message}");
    }

    // 验证越权事件已留痕，且 actor 均为 leader
    let mut client = Client::start(&env.socket);
    let activity = client.call("list_activity", json!({ "limit": 50 }));
    assert!(!activity.is_error, "{}", activity.text);
    let events = activity.json()["events"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    let denied_events: Vec<_> = events
        .iter()
        .filter(|e| e["kind"] == "leader_denied")
        .collect();
    assert_eq!(
        denied_events.len(),
        3,
        "3 次越权尝试均应记录 leader_denied：{events:?}"
    );
    for e in denied_events {
        assert_eq!(e["actor"], "leader", "越权记录操作者必须为 leader：{e}");
    }
}

/// 用例 6：已归档岗位不可再用于派工或建任务，MCP 提示找不到岗位，内核直接请求同样拦截。
#[test]
fn qa_leader_cannot_create_task_or_dispatch_with_archived_role() {
    let env = Env::new(false, &[("常规开发", "DEV")]);
    let mut client = Client::start(&env.socket);

    // 1. 由 Human 创建一个岗位，随后将其归档
    let new_role = env
        .ts
        .create_role(CreateRoleInput {
            team_id: env.team_id.clone(),
            role: NewRole {
                name: "临时岗位".into(),
                code: None,
                task_prefix: "TEMP".into(),
                description: "将被归档的岗位".into(),
                guidelines: String::new(),
                is_leader: false,
            },
        })
        .unwrap();
    let archived_role_id = new_role.role.id;
    let archived_role_code = new_role.role.code;
    env.ts.archive_role(&archived_role_id).unwrap();

    // 2. Leader 尝试通过 MCP create_task 指向已归档岗位名称或代号
    let mcp_call = client.call(
        "create_task",
        json!({
            "title": "已归档岗位的任务",
            "role": "临时岗位",
        }),
    );
    assert!(
        mcp_call.is_error,
        "已归档岗位不应允许选派：{}",
        mcp_call.text
    );
    assert!(
        mcp_call.text.contains("找不到岗位「临时岗位」"),
        "{}",
        mcp_call.text
    );

    let mcp_call_code = client.call(
        "create_task",
        json!({
            "title": "已归档岗位的任务2",
            "role": archived_role_code,
        }),
    );
    assert!(mcp_call_code.is_error, "{}", mcp_call_code.text);

    // 3. Leader 绕过 MCP 直接用已归档岗位 ID 请求内核 create_task
    let raw_reply = socket_request(
        &env.socket,
        "create_task",
        json!({
            "input": {
                "teamId": env.team_id,
                "title": "绕过 MCP 建任务",
                "description": "",
                "ownerRoleId": archived_role_id,
                "status": "todo",
                "fileRefs": [],
                "dependsOn": [],
            }
        }),
    );
    assert!(
        raw_reply.get("ok").is_none(),
        "内核应拒绝已归档岗位：{raw_reply}"
    );
    let err_msg = raw_reply["err"]["message"].as_str().unwrap_or_default();
    assert!(
        err_msg.contains("负责岗位不存在或已归档"),
        "错误提示应明确：{err_msg}"
    );
}

/// 用例 7：跨团队任务与依赖严格隔离，不能跨团队设置依赖或查看未指派团队的任务。
#[test]
fn qa_cross_team_task_and_dependency_isolation() {
    let env = Env::new(false, &[("团队A开发", "TA")]);
    let mut client = Client::start(&env.socket);

    // 创建团队 B
    let ws_b = env.dir.path().join("ws_b");
    std::fs::create_dir_all(&ws_b).unwrap();
    let _team_b = env
        .ts
        .create_team(CreateTeamInput {
            name: "团队B".into(),
            description: "独立团队B".into(),
            workspace_path: ws_b.display().to_string(),
            roles: vec![NewRole {
                name: "团队B开发".into(),
                code: None,
                task_prefix: "TB".into(),
                description: String::new(),
                guidelines: String::new(),
                is_leader: false,
            }],
        })
        .unwrap();

    // 在团队 A 建任务 A1
    let task_a = client.call(
        "create_task",
        json!({ "title": "团队A任务", "role": "TA", "team": "产品团队" }),
    );
    assert!(!task_a.is_error, "{}", task_a.text);
    let id_a = task_a.json()["task"]["id"].as_str().unwrap().to_owned();

    // 在团队 B 建任务 B1
    let task_b = client.call(
        "create_task",
        json!({ "title": "团队B任务", "role": "TB", "team": "团队B" }),
    );
    assert!(!task_b.is_error, "{}", task_b.text);
    let id_b = task_b.json()["task"]["id"].as_str().unwrap().to_owned();
    let key_b = task_b.json()["task"]["key"].as_str().unwrap().to_owned();

    // 1. MCP 层面：跨团队设依赖被拒
    let mcp_dep = client.call(
        "set_dependencies",
        json!({
            "task": id_a,
            "team": "产品团队",
            "depends_on": [key_b],
        }),
    );
    assert!(mcp_dep.is_error, "跨团队依赖应被拒绝：{}", mcp_dep.text);
    assert!(mcp_dep.text.contains("找不到任务"), "{}", mcp_dep.text);

    // 2. 内核层面：直接发 socket 请求设跨团队依赖
    let raw_reply = socket_request(
        &env.socket,
        "check_task_dependencies",
        json!({
            "input": {
                "teamId": env.team_id,
                "taskId": id_a,
                "dependsOn": [id_b],
            }
        }),
    );
    assert!(
        raw_reply.get("ok").is_none(),
        "内核应校验依赖同团队：{raw_reply}"
    );
    let message = raw_reply["err"]["message"].as_str().unwrap_or_default();
    assert!(
        message.contains("依赖的任务不存在或不属于该团队"),
        "{}",
        message
    );
}

// ============================================================================
// 3. 任务状态机边界与依赖环路
// ============================================================================

/// 用例 8：任务状态机流转边界：验证非法流转被拦截，合法撤销与流转顺畅。
#[test]
fn qa_task_status_transition_boundaries_invalid_paths() {
    let env = Env::new(false, &[("前端", "FE")]);
    let mut client = Client::start(&env.socket);

    let created = client.call(
        "create_task",
        json!({ "title": "状态流转边界任务", "role": "FE", "status": "todo" }),
    );
    assert!(!created.is_error, "{}", created.text);
    let key = created.json()["task"]["key"].as_str().unwrap().to_owned();

    // 1. todo -> doing 非法流转（必须先 ready）
    let bad1 = client.call("change_task_status", json!({ "task": key, "to": "doing" }));
    assert!(bad1.is_error, "todo 不能直接转 doing：{}", bad1.text);
    assert!(
        bad1.text.contains("不能从「TODO」变为「执行中」"),
        "{}",
        bad1.text
    );

    // todo -> ready 合法
    let ready = client.call("change_task_status", json!({ "task": key, "to": "ready" }));
    assert!(!ready.is_error, "{}", ready.text);

    // ready -> cancelled 合法
    let cancelled = client.call(
        "change_task_status",
        json!({ "task": key, "to": "cancelled" }),
    );
    assert!(!cancelled.is_error, "{}", cancelled.text);

    // 2. cancelled -> ready 非法流转（cancelled 只能撤销回 todo）
    let bad2 = client.call("change_task_status", json!({ "task": key, "to": "ready" }));
    assert!(bad2.is_error, "cancelled 不能直接转 ready：{}", bad2.text);
    assert!(
        bad2.text.contains("不能从「已取消」变为「READY」"),
        "{}",
        bad2.text
    );

    // 3. cancelled -> todo 合法撤销
    let back_todo = client.call("change_task_status", json!({ "task": key, "to": "todo" }));
    assert!(!back_todo.is_error, "{}", back_todo.text);
    assert_eq!(back_todo.json()["task"]["status"], "todo");

    // 转为 ready 并开始执行到 succeeded 进入 review
    let _ = client.call("change_task_status", json!({ "task": key, "to": "ready" }));
    let run = client.call("start_run", json!({ "task": key }));
    assert!(!run.is_error, "{}", run.text);
    let run_id = run.json()["id"].as_str().unwrap().to_owned();
    wait_for_status(&mut client, &run_id, RunStatus::Succeeded);
    wait_until("任务进入待验收", || {
        let task = client.call("get_task", json!({ "task": key }));
        task.json()["task"]["status"] == "review"
    });

    // 4. review -> todo 非法流转（review 只能转 done/ready/cancelled）
    let bad3 = client.call("change_task_status", json!({ "task": key, "to": "todo" }));
    assert!(bad3.is_error, "review 不能直接转 todo：{}", bad3.text);
    assert!(
        bad3.text.contains("不能从「待验收」变为「TODO」"),
        "{}",
        bad3.text
    );
}

/// 用例 9：变更任务状态为 blocked 必须填写原因，恢复为 ready 时 blockedReason 必须清空。
#[test]
fn qa_task_status_blocked_requires_reason_and_clears_on_resume() {
    let env = Env::new(false, &[("开发", "DEV")]);
    let mut client = Client::start(&env.socket);

    let created = client.call(
        "create_task",
        json!({ "title": "受阻原因校验任务", "role": "DEV", "status": "ready" }),
    );
    assert!(!created.is_error, "{}", created.text);
    let key = created.json()["task"]["key"].as_str().unwrap().to_owned();

    // 先进入 doing
    let doing = client.call("change_task_status", json!({ "task": key, "to": "doing" }));
    assert!(!doing.is_error, "{}", doing.text);

    // 1. 转为 blocked 未提供原因被拒
    let missing_reason = client.call(
        "change_task_status",
        json!({ "task": key, "to": "blocked" }),
    );
    assert!(
        missing_reason.is_error,
        "未给受阻原因应拒绝：{}",
        missing_reason.text
    );

    // 纯空白字符原因被拒
    let blank_reason = client.call(
        "change_task_status",
        json!({ "task": key, "to": "blocked", "reason": "   \n\t  " }),
    );
    assert!(
        blank_reason.is_error,
        "空白受阻原因应拒绝：{}",
        blank_reason.text
    );

    // 2. 带有效原因成功转为 blocked
    let blocked = client.call(
        "change_task_status",
        json!({
            "task": key,
            "to": "blocked",
            "reason": "等待外部支付网关开通沙箱账号",
        }),
    );
    assert!(!blocked.is_error, "{}", blocked.text);
    let task_detail = client.call("get_task", json!({ "task": key })).ok_json();
    assert_eq!(task_detail["task"]["status"], "blocked");
    assert_eq!(
        task_detail["task"]["blockedReason"],
        "等待外部支付网关开通沙箱账号"
    );

    // 3. 从 blocked 恢复为 ready，验证 blockedReason 被清空
    let ready = client.call("change_task_status", json!({ "task": key, "to": "ready" }));
    assert!(!ready.is_error, "{}", ready.text);
    let resumed_detail = client.call("get_task", json!({ "task": key })).ok_json();
    assert_eq!(resumed_detail["task"]["status"], "ready");
    assert!(
        resumed_detail["task"]["blockedReason"].is_null(),
        "恢复后受阻原因应被清空：{resumed_detail}"
    );
}

/// 用例 10：MCP 设置依赖触发成环（含三方环路与自依赖），错误信息中带出完整环路编号链条。
#[test]
fn qa_dependency_cycle_detection_via_mcp_reports_complete_loop() {
    let env = Env::new(false, &[("后端", "BE")]);
    let mut client = Client::start(&env.socket);

    let t1 = client
        .call("create_task", json!({ "title": "任务一", "role": "BE" }))
        .ok_json();
    let t2 = client
        .call("create_task", json!({ "title": "任务二", "role": "BE" }))
        .ok_json();
    let t3 = client
        .call("create_task", json!({ "title": "任务三", "role": "BE" }))
        .ok_json();
    let k1 = t1["task"]["key"].as_str().unwrap().to_owned();
    let k2 = t2["task"]["key"].as_str().unwrap().to_owned();
    let k3 = t3["task"]["key"].as_str().unwrap().to_owned();

    // 链条：T2 依赖 T1，T3 依赖 T2
    let _ = client.call(
        "set_dependencies",
        json!({ "task": k2, "depends_on": [k1.as_str()] }),
    );
    let _ = client.call(
        "set_dependencies",
        json!({ "task": k3, "depends_on": [k2.as_str()] }),
    );

    // 1. 让 T1 依赖 T3，形成闭环
    let cycle_call = client.call(
        "set_dependencies",
        json!({ "task": k1, "depends_on": [k3.as_str()] }),
    );
    assert!(cycle_call.is_error, "形成环路应被拒：{}", cycle_call.text);
    assert!(
        cycle_call.text.contains("循环依赖"),
        "应提示循环依赖：{}",
        cycle_call.text
    );
    assert!(
        cycle_call.text.contains("→"),
        "应输出依赖链箭头：{}",
        cycle_call.text
    );
    assert!(
        cycle_call.text.contains(&k1)
            && cycle_call.text.contains(&k2)
            && cycle_call.text.contains(&k3),
        "应包含环路上全部任务编号：{}",
        cycle_call.text
    );

    // 2. 自依赖：T1 依赖自身
    let self_dep = client.call(
        "set_dependencies",
        json!({ "task": k1, "depends_on": [k1.as_str()] }),
    );
    assert!(self_dep.is_error, "自依赖应被拒：{}", self_dep.text);
    assert!(self_dep.text.contains("循环依赖"), "{}", self_dep.text);
}

// ============================================================================
// 4. 执行生命周期与进度查询
// ============================================================================

/// 用例 11：长任务在执行中（running）状态可经 MCP 查得，取消执行后状态为 cancelled，任务回到 ready。
#[test]
fn qa_run_lifecycle_running_and_cancellation_via_mcp() {
    let env = Env::new(false, &[("前端", "FE")]);
    let mut client = Client::start(&env.socket);

    let created = client.call(
        "create_task",
        json!({
            "title": "长耗时任务",
            "role": "FE",
            "description": "[mock:hang]",
            "status": "ready",
        }),
    );
    assert!(!created.is_error, "{}", created.text);
    let key = created.json()["task"]["key"].as_str().unwrap().to_owned();

    let started = client.call("start_run", json!({ "task": key }));
    assert!(!started.is_error, "{}", started.text);
    let run_id = started.json()["id"].as_str().unwrap().to_owned();

    // 1. 等待并验证 running 状态能被 MCP 查到
    let running = wait_for_status(&mut client, &run_id, RunStatus::Running);
    assert_eq!(running["status"], "running", "{running}");

    // 2. 取消执行
    let cancelled = client.call("cancel_run", json!({ "run": run_id }));
    assert!(!cancelled.is_error, "{}", cancelled.text);
    assert_eq!(
        cancelled.json()["status"],
        "cancelled",
        "{}",
        cancelled.text
    );

    // 3. 查 run_status 确认取消终态
    let status = client.call("run_status", json!({ "run": run_id }));
    assert!(!status.is_error, "{}", status.text);
    assert_eq!(status.json()["status"], "cancelled", "{}", status.text);

    // 4. 任务状态回到 ready
    let task = client.call("get_task", json!({ "task": key }));
    assert!(!task.is_error, "{}", task.text);
    assert_eq!(task.json()["task"]["status"], "ready", "{}", task.text);
}

/// 用例 12：执行失败（failed）状态与待答复权限（waiting_permission）经 MCP 查询及详情展示。
#[test]
fn qa_run_lifecycle_waiting_permission_and_failure_reporting() {
    let env = Env::new(false, &[("开发", "DEV")]);
    let mut client = Client::start(&env.socket);

    // 1. 执行失败场景：[mock:fail]
    let fail_task = client.call(
        "create_task",
        json!({
            "title": "失败用例",
            "role": "DEV",
            "description": "[mock:fail]",
            "status": "ready",
        }),
    );
    assert!(!fail_task.is_error, "{}", fail_task.text);
    let fail_key = fail_task.json()["task"]["key"].as_str().unwrap().to_owned();

    let started_fail = client.call("start_run", json!({ "task": fail_key }));
    assert!(!started_fail.is_error, "{}", started_fail.text);
    let fail_run_id = started_fail.json()["id"].as_str().unwrap().to_owned();

    let failed = wait_for_status(&mut client, &fail_run_id, RunStatus::Failed);
    assert_eq!(failed["status"], "failed", "{failed}");
    assert!(
        failed["failure"]
            .as_str()
            .is_some_and(|f| !f.trim().is_empty()),
        "执行失败应记录失败原因：{failed}"
    );

    // 2. 待答复权限场景：[mock:perm]
    let perm_task = client.call(
        "create_task",
        json!({
            "title": "需要权限的任务",
            "role": "DEV",
            "description": "[mock:perm]",
            "status": "ready",
        }),
    );
    assert!(!perm_task.is_error, "{}", perm_task.text);
    let perm_key = perm_task.json()["task"]["key"].as_str().unwrap().to_owned();

    let started_perm = client.call("start_run", json!({ "task": perm_key }));
    assert!(!started_perm.is_error, "{}", started_perm.text);
    let perm_run_id = started_perm.json()["id"].as_str().unwrap().to_owned();

    let waiting = wait_for_status(&mut client, &perm_run_id, RunStatus::WaitingPermission);
    assert_eq!(waiting["status"], "waiting_permission", "{waiting}");

    // Leader 查询待答复权限列表（只读）
    let pending = client.call("pending_permissions", json!({}));
    assert!(!pending.is_error, "{}", pending.text);
    let pending_json = pending.json();
    assert!(
        pending_json["count"].as_u64().unwrap_or(0) >= 1,
        "{pending_json}"
    );
    let perm = pending_json["permissions"]
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p["runId"] == perm_run_id.as_str())
        .expect("应能查到该次执行的待答复权限");
    assert_eq!(perm["taskKey"], perm_key.as_str());
    assert!(
        pending_json["hint"]
            .as_str()
            .unwrap()
            .contains("应用中答复")
    );

    // 收尾清理
    let _ = client.call("cancel_run", json!({ "run": perm_run_id }));
}

/// 用例 13：run_progress 游标续读：通过 limit 与 cursor 分页拉取进度，验证前后页不重叠且穷尽。
#[test]
fn qa_run_progress_cursor_pagination_and_exhaustion() {
    let env = Env::new(false, &[("前端", "FE")]);
    let mut client = Client::start(&env.socket);

    let created = client.call(
        "create_task",
        json!({
            "title": "进度测试任务",
            "role": "FE",
            "description": "跑一个正常流程产生多条进度",
            "status": "ready",
        }),
    );
    assert!(!created.is_error, "{}", created.text);
    let key = created.json()["task"]["key"].as_str().unwrap().to_owned();

    let started = client.call("start_run", json!({ "task": key }));
    assert!(!started.is_error, "{}", started.text);
    let run_id = started.json()["id"].as_str().unwrap().to_owned();

    wait_for_status(&mut client, &run_id, RunStatus::Succeeded);

    // 1. 第一页：limit = 2
    let page1 = client.call("run_progress", json!({ "run": run_id, "limit": 2 }));
    assert!(!page1.is_error, "{}", page1.text);
    let p1 = page1.json();
    let updates1 = p1["updates"].as_array().expect("条目数组");
    assert_eq!(updates1.len(), 2, "第一页应恰好返回 2 条：{p1}");
    let cursor1 = p1["nextCursor"]
        .as_str()
        .expect("应有 nextCursor")
        .to_owned();

    // 2. 第二页：带 cursor 且 limit = 2
    let page2 = client.call(
        "run_progress",
        json!({ "run": run_id, "cursor": cursor1, "limit": 2 }),
    );
    assert!(!page2.is_error, "{}", page2.text);
    let p2 = page2.json();
    let updates2 = p2["updates"].as_array().expect("条目数组");
    assert!(!updates2.is_empty(), "第二页应有后续条目：{p2}");

    // 验证两页条目不重叠（id 互斥）
    let id1: Vec<_> = updates1.iter().map(|u| u["id"].clone()).collect();
    for u in updates2 {
        assert!(!id1.contains(&u["id"]), "分页条目不应重复：{u}");
    }

    // 3. 持续翻页直到穷尽
    let mut current_cursor = p2["nextCursor"].as_str().map(str::to_owned);
    let mut total_fetched = updates1.len() + updates2.len();
    while let Some(c) = current_cursor {
        let next_page = client.call(
            "run_progress",
            json!({ "run": run_id, "cursor": c, "limit": 5 }),
        );
        assert!(!next_page.is_error, "{}", next_page.text);
        let np = next_page.json();
        let items = np["updates"].as_array().unwrap();
        if items.is_empty() {
            break;
        }
        total_fetched += items.len();
        current_cursor = np["nextCursor"].as_str().map(str::to_owned);
    }
    assert!(total_fetched >= 3, "总进度条目数应合理：{total_fetched}");
}

// ============================================================================
// 5. 应用状态与 Socket 异常边界
// ============================================================================

/// 用例 14：控制 socket 路径被普通文件占用时，MCP 客户端返回清晰的未运行提示且进程存活；
/// 内核启动服务时报错保护普通文件不被覆盖。
#[test]
fn qa_socket_occupied_by_regular_file_handled_gracefully() {
    let dir = tempfile::tempdir().unwrap();
    let file_path = dir.path().join("teamspace-control.sock");
    std::fs::write(&file_path, "not a unix socket, just plain text").unwrap();

    // 1. MCP 客户端试图通过该普通文件路径连接：返回清晰中文错误，不崩溃
    let mut client = Client::start(&file_path);
    let call = client.call("list_teams", json!({}));
    assert!(call.is_error, "应返回工具错误：{}", call.text);
    assert!(
        call.text
            .contains("Teamspace 应用没有运行，请先打开 Teamspace"),
        "应提示应用未运行：{}",
        call.text
    );
    // 再次调用仍能正常响应，证明 client 进程健康
    let call2 = client.call("task_board", json!({}));
    assert!(call2.is_error, "{}", call2.text);
    assert!(
        call2
            .text
            .contains("Teamspace 应用没有运行，请先打开 Teamspace")
    );

    // 2. 内核服务试图在普通文件路径上启动：报错保护用户数据，拒绝覆盖
    let env = Env::new(false, &[("前端", "FE")]);
    let result = teamspace_core::control::serve(Arc::clone(&env.ts), &file_path);
    assert!(result.is_err(), "内核不能覆盖普通文件");
    let err = result.unwrap_err();
    assert_eq!(err.kind(), std::io::ErrorKind::AlreadyExists);
    assert!(
        err.to_string().contains("不是 socket 文件"),
        "应明确指出不是 socket 文件：{}",
        err
    );
}

/// 用例 15：socket 文件已被另一个活动进程占用监听时拒绝覆盖；服务退出时清理 socket 文件。
#[test]
#[ignore = "缺陷 M4-G-1：并行启动子进程时监听 fd 可能被继承，旧监听关闭后探测仍连得上，重新绑定被拒（macOS）"]
fn qa_socket_in_use_by_another_process_refuses_overwrite() {
    let dir = tempfile::tempdir().unwrap();
    let socket_path = dir.path().join("active.sock");
    // 先占用该 socket
    let listener = std::os::unix::net::UnixListener::bind(&socket_path).unwrap();

    // 内核尝试绑定已被占用的 socket
    let env = Env::new(false, &[("开发", "DEV")]);
    let result = teamspace_core::control::serve(Arc::clone(&env.ts), &socket_path);
    assert!(result.is_err(), "应拒绝覆盖正在被监听的 socket");
    let err = result.unwrap_err();
    assert_eq!(err.kind(), std::io::ErrorKind::AddrInUse);
    assert!(
        err.to_string().contains("已有进程在监听"),
        "错误信息应说明已有进程监听：{}",
        err
    );

    // 释放旧监听后，再次 serve 成功
    drop(listener);
    let server = teamspace_core::control::serve(Arc::clone(&env.ts), &socket_path);
    assert!(
        server.is_ok(),
        "旧监听关闭后应能绑定成功：{:?}",
        server.err()
    );
    let server = server.unwrap();
    assert!(socket_path.exists(), "socket 文件应存在");

    // 停止服务后 socket 文件被清理
    server.shutdown();
    assert!(!socket_path.exists(), "shutdown 后 socket 文件应被删除");
}

// ============================================================================
// 6. 审计日志与通用参数校验边界
// ============================================================================

/// 用例 16：活动记录严格区分 Leader 与 Human 操作者，并支持按岗位精确筛选。
#[test]
fn qa_activity_audit_log_strict_actor_attribution_leader_vs_human() {
    let env = Env::new(false, &[("前端", "FE"), ("后端", "BE")]);
    let mut client = Client::start(&env.socket);

    // 1. Leader 通过 MCP 创建任务
    let leader_task = client
        .call(
            "create_task",
            json!({ "title": "Leader 建的任务", "role": "FE" }),
        )
        .ok_json();
    let leader_task_id = leader_task["task"]["id"].as_str().unwrap().to_owned();

    // 2. Leader 更新任务
    let _ = client.call(
        "update_task",
        json!({ "task": leader_task_id, "description": "补充说明" }),
    );

    // 3. Leader 越权尝试
    let _ = socket_request(
        &env.socket,
        "archive_role",
        json!({ "roleId": env.role_ids[0] }),
    );

    // 4. Human 直接通过内核创建任务
    let human_task = env
        .ts
        .create_task_as(
            Actor::Human,
            CreateTaskInput {
                team_id: env.team_id.clone(),
                title: "Human 建的任务".into(),
                description: String::new(),
                owner_role_id: env.role_ids[1].clone(),
                status: TaskStatus::Todo,
                file_refs: vec![],
                depends_on: vec![],
            },
        )
        .unwrap();

    // 5. Leader 通过 list_activity 查询并核对 actor 归属
    let activity = client
        .call("list_activity", json!({ "limit": 50 }))
        .ok_json();
    let events = activity["events"].as_array().expect("活动列表");

    let leader_created = events
        .iter()
        .find(|e| e["taskId"] == leader_task_id.as_str() && e["kind"] == "task_created")
        .expect("找到 Leader 创建任务事件");
    assert_eq!(leader_created["actor"], "leader");

    let leader_updated = events
        .iter()
        .find(|e| e["taskId"] == leader_task_id.as_str() && e["kind"] == "task_updated")
        .expect("找到 Leader 更新任务事件");
    assert_eq!(leader_updated["actor"], "leader");

    let leader_denied = events
        .iter()
        .find(|e| e["kind"] == "leader_denied")
        .expect("找到越权拦截事件");
    assert_eq!(leader_denied["actor"], "leader");
    assert_eq!(leader_denied["data"]["command"], "archive_role");

    let human_created = events
        .iter()
        .find(|e| e["taskId"] == human_task.task.id.as_str() && e["kind"] == "task_created")
        .expect("找到 Human 创建任务事件");
    assert_eq!(human_created["actor"], "human");

    // 6. 按岗位筛选活动
    let fe_activity = client
        .call("list_activity", json!({ "role": "FE" }))
        .ok_json();
    let fe_events = fe_activity["events"].as_array().unwrap();
    assert!(
        fe_events
            .iter()
            .all(|e| e["roleId"] == env.role_ids[0].as_str()),
        "按岗位筛选应只返回该岗位的活动：{fe_events:?}"
    );
}

/// 用例 17：update_task 局部更新时，未传的字段（title、role、dependsOn、fileRefs 及其锚点）原样保留。
#[test]
fn qa_update_task_partial_edits_preserve_unspecified_fields() {
    let env = Env::new(false, &[("前端", "FE"), ("后端", "BE")]);
    let mut client = Client::start(&env.socket);

    // 先建前置任务
    let dep = client
        .call(
            "create_task",
            json!({ "title": "前置基础库", "role": "BE" }),
        )
        .ok_json();
    let dep_id = dep["task"]["id"].as_str().unwrap().to_owned();

    // 建主任务：带完整字段、文件引用锚点、前置依赖
    let main = client
        .call(
            "create_task",
            json!({
                "title": "原始标题",
                "role": "FE",
                "description": "原始任务详细说明",
                "file_refs": ["README.md#使用方法"],
                "depends_on": [dep_id.as_str()],
            }),
        )
        .ok_json();
    let main_id = main["task"]["id"].as_str().unwrap().to_owned();

    // 1. 只更新 description：其他字段必须原样保留
    let update1 = client.call(
        "update_task",
        json!({
            "task": main_id.as_str(),
            "description": "更新后的说明文字",
        }),
    );
    assert!(!update1.is_error, "{}", update1.text);
    let t1 = update1.json()["task"].clone();
    assert_eq!(t1["title"], "原始标题", "未传 title 应保持原样");
    assert_eq!(t1["description"], "更新后的说明文字");
    assert_eq!(t1["ownerRoleId"], env.role_ids[0].as_str());
    assert_eq!(t1["dependsOn"][0], dep_id.as_str(), "依赖不应被冲掉");
    assert_eq!(t1["fileRefs"][0]["path"], "README.md");
    assert_eq!(t1["fileRefs"][0]["anchor"], "使用方法", "锚点不应丢失");

    // 2. 只更新 role 为后端岗位：其他字段保持原样
    let update2 = client.call(
        "update_task",
        json!({
            "task": main_id.as_str(),
            "role": "BE",
        }),
    );
    assert!(!update2.is_error, "{}", update2.text);
    let t2 = update2.json()["task"].clone();
    assert_eq!(t2["ownerRoleId"], env.role_ids[1].as_str(), "岗位应换到 BE");
    assert_eq!(t2["title"], "原始标题");
    assert_eq!(t2["description"], "更新后的说明文字");
    assert_eq!(t2["dependsOn"][0], dep_id.as_str());
    assert_eq!(t2["fileRefs"][0]["anchor"], "使用方法");
}

/// 用例 18：空白或非法参数校验：全空白标题、无效状态名、越权状态 done、空白退回意见等。
#[test]
fn qa_empty_and_whitespace_only_arguments_validation() {
    let env = Env::new(false, &[("开发", "DEV")]);
    let mut client = Client::start(&env.socket);

    // 1. create_task 标题全空白
    let empty_title = client.call(
        "create_task",
        json!({ "title": "   \n\t  ", "role": "DEV" }),
    );
    assert!(empty_title.is_error, "空标题应拒绝：{}", empty_title.text);
    assert!(
        empty_title.text.contains("请给任务写个标题"),
        "{}",
        empty_title.text
    );

    // 创建一个合法的任务用于后续测试
    let valid_task = client
        .call(
            "create_task",
            json!({ "title": "校验基准任务", "role": "DEV" }),
        )
        .ok_json();
    let key = valid_task["task"]["key"].as_str().unwrap().to_owned();

    // 2. change_task_status 非法状态名
    let invalid_status = client.call(
        "change_task_status",
        json!({ "task": key.as_str(), "to": "not_a_valid_status" }),
    );
    assert!(invalid_status.is_error, "{}", invalid_status.text);
    assert!(
        invalid_status.text.contains("任务状态只能是"),
        "{}",
        invalid_status.text
    );

    // change_task_status 试图置为 done
    let done_status = client.call(
        "change_task_status",
        json!({ "task": key.as_str(), "to": "done" }),
    );
    assert!(done_status.is_error, "{}", done_status.text);
    assert!(
        done_status.text.contains("不能把任务置为已完成"),
        "{}",
        done_status.text
    );

    // 3. reject_review 空白意见
    let blank_comment = client.call(
        "reject_review",
        json!({ "task": key.as_str(), "comment": "   \t  " }),
    );
    assert!(
        blank_comment.is_error,
        "空白意见应拒绝：{}",
        blank_comment.text
    );
    assert!(
        blank_comment.text.contains("退回验收必须写意见"),
        "{}",
        blank_comment.text
    );

    // 4. get_task 空白任务标识
    let blank_task = client.call("get_task", json!({ "task": "   " }));
    assert!(
        blank_task.is_error,
        "空白任务指代应拒绝：{}",
        blank_task.text
    );
    assert!(
        blank_task.text.contains("请指明任务"),
        "{}",
        blank_task.text
    );
}
