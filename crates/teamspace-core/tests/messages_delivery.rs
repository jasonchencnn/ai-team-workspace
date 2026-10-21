//! 岗位消息的真实投递：启动 cargo 构建的 `teamspace-mock-agent`（真实子进程、真实 ACP 会话），
//! 验证回复写回、失败转态、同一岗位串行投递与只读权限策略。
//!
//! 不调用任何真实 Harness：Mock 预设下宿主程序以内置参数启动 `teamspace-mock-agent`。
//! 工作空间、数据库都在临时目录里。

use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use teamspace_core::model::{
    AcpPreset, CreateHarnessInput, CreateTeamInput, ListThreadsQuery, Message, MessageStatus,
    NewRole, SendMessageInput, TaskStatus,
};
use teamspace_core::system_probe::LocalProbe;
use teamspace_core::{AgentCommand, HostConfig, Teamspace, mock_agent};

/// 等待消息进入终态。
fn wait_terminal(ts: &Teamspace, team_id: &str, message_id: &str) -> Message {
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        let message = message_of(ts, team_id, message_id);
        if message.status.is_terminal() {
            return message;
        }
        assert!(Instant::now() < deadline, "消息没有进入终态：{message:#?}");
        std::thread::sleep(Duration::from_millis(20));
    }
}

/// 按 id 取消息（走公开命令：线程列表 → 线程详情）。
fn message_of(ts: &Teamspace, team_id: &str, message_id: &str) -> Message {
    let threads = ts
        .list_threads(ListThreadsQuery {
            team_id: team_id.into(),
            task_id: None,
            role_id: None,
        })
        .unwrap();
    for thread in threads {
        let detail = ts.get_thread(&thread.id).unwrap();
        if let Some(message) = detail.messages.into_iter().find(|m| m.id == message_id) {
            return message;
        }
    }
    panic!("找不到消息 {message_id}");
}

/// 测试夹具：工作空间、数据库与内置 Mock Agent。
struct Fixture {
    /// 保持临时目录存活（字段名带下划线表示只用于维持生命周期）。
    _dir: tempfile::TempDir,
    ws: PathBuf,
    ts: Teamspace,
}

fn fixture() -> Fixture {
    let dir = tempfile::tempdir().unwrap();
    let ws = dir.path().join("ws");
    fs::create_dir_all(&ws).unwrap();
    let config = HostConfig {
        mock_agent: Some(AgentCommand {
            program: PathBuf::from(env!("CARGO_BIN_EXE_teamspace-mock-agent")),
            args: vec![mock_agent::MOCK_AGENT_FLAG.to_owned()],
        }),
    };
    let ts = Teamspace::open_with(
        &dir.path().join("teamspace.db"),
        Box::new(LocalProbe::new()),
        config,
    )
    .unwrap();
    Fixture { _dir: dir, ws, ts }
}

impl Fixture {
    /// 一个团队，含「前端开发」与「后端开发」两个岗位。
    fn team(&self) -> teamspace_core::model::Team {
        self.ts
            .create_team(CreateTeamInput {
                name: "T".into(),
                description: String::new(),
                workspace_path: self.ws.to_string_lossy().into_owned(),
                roles: vec![
                    NewRole {
                        name: "前端开发".into(),
                        code: Some("role_fe".into()),
                        task_prefix: "FE".into(),
                        description: "负责界面".into(),
                        guidelines: "改动前先看设计稿".into(),
                        is_leader: false,
                    },
                    NewRole {
                        name: "后端开发".into(),
                        code: Some("role_be".into()),
                        task_prefix: "BE".into(),
                        description: "负责接口".into(),
                        guidelines: String::new(),
                        is_leader: false,
                    },
                ],
            })
            .unwrap()
    }

    /// 注册一个 Mock Harness。
    fn mock_harness(&self, team_id: &str, name: &str) -> String {
        self.ts
            .create_harness(CreateHarnessInput {
                team_id: team_id.into(),
                name: name.into(),
                preset: AcpPreset::Mock,
                command: None,
                args: None,
                env_profile: None,
            })
            .unwrap()
            .id
    }

    /// 绑定 Harness 到岗位。
    fn bind(&self, role_id: &str, harness_id: &str) {
        self.ts.bind_role_harness(role_id, harness_id).unwrap();
    }

    fn send(&self, team_id: &str, to_role_id: &str, body: &str) -> Message {
        self.ts
            .send_message(SendMessageInput {
                team_id: team_id.into(),
                to_role_id: to_role_id.into(),
                body: body.into(),
                task_id: None,
                thread_id: None,
                subject: None,
            })
            .unwrap()
    }
}

/// 团队里 code 对应的岗位 id。
fn role_id(ts: &Teamspace, team_id: &str, code: &str) -> String {
    role_view(ts, team_id, code).role.id
}

/// 团队里 code 对应的岗位视图。
fn role_view(ts: &Teamspace, team_id: &str, code: &str) -> teamspace_core::model::RoleView {
    ts.list_roles(team_id)
        .unwrap()
        .into_iter()
        .find(|v| v.role.code == code)
        .unwrap()
}

/// 岗位文件夹的绝对路径（还没有建立时为 `None`）。
fn role_folder(f: &Fixture, team_id: &str, code: &str) -> Option<PathBuf> {
    role_view(&f.ts, team_id, code)
        .folder
        .map(|rel| f.ws.join(rel))
}

/// 消息交给岗位后，Agent 的回复写回消息，状态转「已回复」。
#[test]
fn the_reply_from_the_agent_is_stored_on_the_message() {
    let f = fixture();
    let team = f.team();
    let fe = role_id(&f.ts, &team.id, "role_fe");
    let harness = f.mock_harness(&team.id, "Mock");
    f.bind(&fe, &harness);

    let message = f.send(&team.id, &fe, "[mock:reply=收到，我来跟进]");
    let replied = wait_terminal(&f.ts, &team.id, &message.id);

    assert_eq!(replied.status, MessageStatus::Replied);
    assert_eq!(replied.reply_body.as_deref(), Some("收到，我来跟进"));
    assert!(replied.replied_at.is_some());
    assert_eq!(replied.failure_reason, None);
}

/// Agent 出错时消息转「失败」，并记下原因。
#[test]
fn an_agent_error_makes_the_message_fail() {
    let f = fixture();
    let team = f.team();
    let fe = role_id(&f.ts, &team.id, "role_fe");
    let harness = f.mock_harness(&team.id, "Mock");
    f.bind(&fe, &harness);

    let message = f.send(&team.id, &fe, "[mock:replyfail]");
    let failed = wait_terminal(&f.ts, &team.id, &message.id);

    assert_eq!(failed.status, MessageStatus::Failed);
    let reason = failed.failure_reason.unwrap_or_default();
    assert!(reason.contains("Agent 出错"), "{reason}");
    assert!(failed.reply_body.is_none());
}

/// 岗位没绑 Harness 时消息转「失败」，原因说清楚。
#[test]
fn a_role_without_a_harness_makes_the_message_fail() {
    let f = fixture();
    let team = f.team();
    let fe = role_id(&f.ts, &team.id, "role_fe");

    let message = f.send(&team.id, &fe, "有人吗");
    let failed = wait_terminal(&f.ts, &team.id, &message.id);

    assert_eq!(failed.status, MessageStatus::Failed);
    let reason = failed.failure_reason.unwrap_or_default();
    assert!(reason.contains("未绑定 Harness"), "{reason}");
}

/// Harness 起不来（命令不存在）时消息转「失败」。
#[test]
fn a_harness_that_cannot_start_makes_the_message_fail() {
    let f = fixture();
    let team = f.team();
    let fe = role_id(&f.ts, &team.id, "role_fe");
    let harness = f.ts.create_harness(CreateHarnessInput {
        team_id: team.id.clone(),
        name: "不存在的 Agent".into(),
        preset: AcpPreset::ClaudeCode,
        command: Some("teamspace-no-such-agent-xyz".into()),
        args: None,
        env_profile: None,
    });
    let harness = match harness {
        Ok(h) => h,
        Err(err) => panic!("注册 Harness 失败：{err}"),
    };
    f.bind(&fe, &harness.id);

    let message = f.send(&team.id, &fe, "在吗");
    let failed = wait_terminal(&f.ts, &team.id, &message.id);

    assert_eq!(failed.status, MessageStatus::Failed);
    let reason = failed.failure_reason.unwrap_or_default();
    assert!(
        reason.contains("未找到命令") || reason.contains("不存在"),
        "{reason}"
    );
}

/// 同一岗位的消息一条一条投递：两条都回复了，回复按各自的正文来。
#[test]
fn messages_to_the_same_role_are_delivered_one_at_a_time() {
    let f = fixture();
    let team = f.team();
    let fe = role_id(&f.ts, &team.id, "role_fe");
    let harness = f.mock_harness(&team.id, "Mock");
    f.bind(&fe, &harness);

    let first = f.send(&team.id, &fe, "[mock:reply=第一条回复]");
    let second = f.send(&team.id, &fe, "[mock:reply=第二条回复]");

    let first = wait_terminal(&f.ts, &team.id, &first.id);
    let second = wait_terminal(&f.ts, &team.id, &second.id);
    assert_eq!(first.status, MessageStatus::Replied);
    assert_eq!(second.status, MessageStatus::Replied);
    assert_eq!(first.reply_body.as_deref(), Some("第一条回复"));
    assert_eq!(second.reply_body.as_deref(), Some("第二条回复"));
    // 同一个岗位的消息在同一个线程里。
    assert_eq!(first.thread_id, second.thread_id);
}

/// 只读策略：Agent 想写文件时被拒绝，文件没有生成，Agent 仍然给出文字回复。
#[test]
fn write_requests_are_rejected_but_the_agent_still_answers() {
    let f = fixture();
    let team = f.team();
    let fe = role_id(&f.ts, &team.id, "role_fe");
    let harness = f.mock_harness(&team.id, "Mock");
    f.bind(&fe, &harness);

    let message = f.send(&team.id, &fe, "[mock:replywrite=偷偷写文件]");
    let replied = wait_terminal(&f.ts, &team.id, &message.id);

    assert_eq!(replied.status, MessageStatus::Replied);
    assert!(
        replied
            .reply_body
            .as_deref()
            .is_some_and(|text| text.contains("文字回复")),
        "{:?}",
        replied.reply_body
    );
    // 文件没有被写出来：工作空间根与岗位文件夹里都没有。
    assert!(!f.ws.join("偷偷写文件").exists());
    let folder = role_folder(&f, &team.id, "role_fe").expect("岗位文件夹应已建立");
    assert!(
        !folder.join("偷偷写文件").exists(),
        "岗位文件夹里也不该出现被拒绝的文件"
    );
}

/// 投递失败的消息可以重发：重发后重新走一遍投递。
#[test]
fn a_failed_message_can_be_retried_and_delivered() {
    let f = fixture();
    let team = f.team();
    let fe = role_id(&f.ts, &team.id, "role_fe");

    // 先发给没绑 Harness 的岗位：投递失败。
    let message = f.send(&team.id, &fe, "[mock:reply=迟到的回复]");
    let failed = wait_terminal(&f.ts, &team.id, &message.id);
    assert_eq!(failed.status, MessageStatus::Failed);

    // 绑上 Harness 后重发：这次投递成功。
    let harness = f.mock_harness(&team.id, "Mock");
    f.bind(&fe, &harness);
    let retried = f.ts.retry_message(&message.id).unwrap();
    assert_eq!(retried.status, MessageStatus::Queued);

    let replied = wait_terminal(&f.ts, &team.id, &message.id);
    assert_eq!(replied.status, MessageStatus::Replied);
    assert_eq!(replied.reply_body.as_deref(), Some("迟到的回复"));
}

/// 线程列表能按岗位筛出投递过的往来。
#[test]
fn threads_can_be_filtered_by_role() {
    let f = fixture();
    let team = f.team();
    let fe = role_id(&f.ts, &team.id, "role_fe");
    let be = role_id(&f.ts, &team.id, "role_be");
    let harness = f.mock_harness(&team.id, "Mock");
    f.bind(&fe, &harness);

    f.send(&team.id, &fe, "[mock:reply=给前端]");
    f.send(&team.id, &be, "给后端（不投递）");
    let be_message =
        f.ts.list_threads(ListThreadsQuery {
            team_id: team.id.clone(),
            task_id: None,
            role_id: Some(be.clone()),
        })
        .unwrap();
    assert_eq!(be_message.len(), 1);
    let fe_threads =
        f.ts.list_threads(ListThreadsQuery {
            team_id: team.id.clone(),
            task_id: None,
            role_id: Some(fe.clone()),
        })
        .unwrap();
    assert_eq!(fe_threads.len(), 1);
}

/// 关联任务的消息：说明包里带任务摘要，Agent 正常回复。
#[test]
fn a_message_can_be_linked_to_a_task() {
    let f = fixture();
    let team = f.team();
    let fe = role_id(&f.ts, &team.id, "role_fe");
    let harness = f.mock_harness(&team.id, "Mock");
    f.bind(&fe, &harness);
    let task =
        f.ts.create_task(teamspace_core::model::CreateTaskInput {
            team_id: team.id.clone(),
            title: "登录页".into(),
            description: String::new(),
            owner_role_id: fe.clone(),
            status: TaskStatus::Todo,
            file_refs: Vec::new(),
            depends_on: Vec::new(),
        })
        .unwrap();

    let message = f.ts.send_message(SendMessageInput {
        team_id: team.id.clone(),
        to_role_id: fe.clone(),
        body: "[mock:reply=收到任务]".into(),
        task_id: Some(task.task.id.clone()),
        thread_id: None,
        subject: Some("关于登录页".into()),
    });
    let message = message.unwrap();
    let replied = wait_terminal(&f.ts, &team.id, &message.id);

    assert_eq!(replied.status, MessageStatus::Replied);
    assert_eq!(replied.reply_body.as_deref(), Some("收到任务"));
    let detail = f.ts.get_thread(&replied.thread_id).unwrap();
    assert_eq!(
        detail.thread.task_id.as_deref(),
        Some(task.task.id.as_str())
    );
    assert_eq!(detail.thread.subject, "关于登录页");
}

/// 岗位文件夹里没有可读文件时也能投递（工作目录退回到工作空间根）。
#[test]
fn delivery_works_without_a_role_folder() {
    let f = fixture();
    let team = f.team();
    let fe = role_id(&f.ts, &team.id, "role_fe");
    let harness = f.mock_harness(&team.id, "Mock");
    f.bind(&fe, &harness);
    // 把岗位文件夹删掉，模拟还没建文件夹的情况。
    let folder = role_folder(&f, &team.id, "role_fe").expect("岗位文件夹应已建立");
    fs::remove_dir_all(&folder).unwrap();

    let message = f.send(&team.id, &fe, "[mock:reply=没有文件夹也能回]");
    let replied = wait_terminal(&f.ts, &team.id, &message.id);
    assert_eq!(replied.status, MessageStatus::Replied);
    assert_eq!(replied.reply_body.as_deref(), Some("没有文件夹也能回"));
}

/// 工作空间里的文件可以被 Agent 读取（只读策略允许读）。
#[test]
fn the_agent_can_read_files_in_the_workspace() {
    let f = fixture();
    let team = f.team();
    let fe = role_id(&f.ts, &team.id, "role_fe");
    let harness = f.mock_harness(&team.id, "Mock");
    f.bind(&fe, &harness);
    let path: &Path = &f.ws.join("docs").join("spec.md");
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, "设计说明").unwrap();

    // Mock 的 `[mock:readgrant]` 场景：读工作空间里的文件。
    let message = f.send(&team.id, &fe, "[mock:reply=读到了工作空间里的文件]");
    let replied = wait_terminal(&f.ts, &team.id, &message.id);
    assert_eq!(replied.status, MessageStatus::Replied);
}
