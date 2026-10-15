//! 通过 [`Teamspace`] 公开命令测试业务规则。全部使用临时目录和假探测，不调用真实 Git、CLI 或 Agent。

mod dependencies;
mod events;
mod files;
mod folders;
mod harnesses;
#[cfg(unix)]
mod qa_round2;
#[cfg(unix)]
mod qa_round2_run;
pub(crate) mod reviews;
mod roles;
mod runs;
mod tasks;
mod teams;
mod workbench;

use std::fs;
use std::path::{Path, PathBuf};

use tempfile::TempDir;

use crate::acp::AgentLaunch;
use crate::error::{Error, ErrorKind, Result};
use crate::model::*;
use crate::probe::SystemProbe;
use crate::{AgentCommand, HostConfig, Teamspace};

/// 假探测：目录自身或任一上级目录含 `.git` 时视为 Git 仓库（分支 main、提交 a31f8c2），否则为普通
/// 文件夹；文件搜索遍历目录（跳过以 `.` 开头的条目）；命令查找与握手按约定返回。
pub(crate) struct FakeProbe;

thread_local! {
    /// [`FakeProbe::reveal_path`] 收到的路径（每个测试线程各一份）。
    pub(crate) static REVEALED: std::cell::RefCell<Vec<PathBuf>> = const { std::cell::RefCell::new(Vec::new()) };
    /// [`FakeProbe::open_path`] 收到的路径。
    pub(crate) static OPENED: std::cell::RefCell<Vec<PathBuf>> = const { std::cell::RefCell::new(Vec::new()) };
    /// [`FakeProbe::handshake`] 收到的启动信息。
    pub(crate) static LAUNCHED: std::cell::RefCell<Vec<AgentLaunch>> = const { std::cell::RefCell::new(Vec::new()) };
}

/// 握手时命令为此值则返回 error。
pub(crate) const BROKEN_COMMAND: &str = "/broken/agent";
/// 握手时命令为此值则返回 needs_auth。
pub(crate) const NEEDS_AUTH_COMMAND: &str = "needs-auth-agent";
/// 测试宿主注入的 Mock Agent 命令。
pub(crate) const MOCK_HOST: &str = "/opt/teamspace/host";
/// 假探测在本机「找不到」的命令（codebuddy 在本机已安装，按「找到」处理）。
pub(crate) const MISSING_COMMANDS: &[&str] = &[
    "codex-acp",
    "gemini",
    "claude",
    "qodercli",
    "grok",
    "qwen",
    "kimi",
    "opencode",
];

impl SystemProbe for FakeProbe {
    fn inspect_workspace(&self, path: &Path) -> Result<WorkspaceInfo> {
        if !path.is_dir() {
            return Err(Error::invalid("所选路径不是目录"));
        }
        let root = path.ancestors().find(|dir| dir.join(".git").is_dir());
        Ok(WorkspaceInfo {
            // 内核会用所选目录覆盖 path；这里故意返回仓库根，以验证不会被提升。
            path: root.unwrap_or(path).to_string_lossy().into_owned(),
            git: root.map(|_| GitInfo {
                branch: Some("main".into()),
                commit: Some("a31f8c2".into()),
            }),
        })
    }

    fn search_files(&self, workspace: &Path, query: &str, limit: usize) -> Result<Vec<String>> {
        let out = walk_files(workspace);
        let query = query.to_lowercase();
        Ok(out
            .into_iter()
            .filter(|p| p.to_lowercase().contains(&query))
            .take(limit)
            .collect())
    }

    fn list_markdown_files(&self, workspace: &Path, limit: usize) -> Result<Vec<String>> {
        let mut files: Vec<String> = walk_files(workspace)
            .into_iter()
            .filter(|path| path.ends_with(".md"))
            .collect();
        files.sort();
        files.truncate(limit);
        Ok(files)
    }

    fn locate_command(&self, command: &str) -> Option<String> {
        (!MISSING_COMMANDS.contains(&command)).then(|| format!("/usr/local/bin/{command}"))
    }

    fn handshake(&self, launch: &AgentLaunch) -> Handshake {
        LAUNCHED.with(|launched| launched.borrow_mut().push(launch.clone()));
        match launch.command.as_str() {
            BROKEN_COMMAND => Handshake::error("无法启动 /broken/agent"),
            NEEDS_AUTH_COMMAND => Handshake {
                status: HandshakeStatus::NeedsAuth,
                auth_methods: vec!["ChatGPT 登录".into()],
                detail: Some("Agent 要求先登录".into()),
                ..Handshake::unknown()
            },
            command => Handshake {
                status: HandshakeStatus::Connected,
                protocol_version: Some(1),
                agent: Some(AgentInfo {
                    name: command.rsplit('/').next().unwrap_or(command).to_owned(),
                    title: None,
                    version: Some("0.9.2".into()),
                }),
                capabilities: Some(AcpCapabilities {
                    load_session: true,
                    ..AcpCapabilities::default()
                }),
                auth_methods: Vec::new(),
                detail: Some("握手成功".into()),
                checked_at: None,
            },
        }
    }

    fn reveal_path(&self, path: &Path) -> Result<()> {
        REVEALED.with(|revealed| revealed.borrow_mut().push(path.to_path_buf()));
        Ok(())
    }

    fn open_path(&self, path: &Path) -> Result<()> {
        OPENED.with(|opened| opened.borrow_mut().push(path.to_path_buf()));
        Ok(())
    }
}

/// 遍历工作空间里的文件（相对 `/` 分隔，跳过以 `.` 开头的条目），供假探测的搜索与列举共用。
fn walk_files(root: &Path) -> Vec<String> {
    fn walk(root: &Path, dir: &Path, out: &mut Vec<String>) {
        for entry in fs::read_dir(dir).unwrap().flatten() {
            let path = entry.path();
            if path.file_name().unwrap().to_string_lossy().starts_with('.') {
                continue;
            }
            if path.is_dir() {
                walk(root, &path, out);
            } else {
                let rel = path.strip_prefix(root).unwrap();
                out.push(rel.to_string_lossy().replace('\\', "/"));
            }
        }
    }
    let mut out = Vec::new();
    walk(root, root, &mut out);
    out.sort();
    out
}

pub(crate) struct Fixture {
    pub dir: TempDir,
    pub ws: PathBuf,
    pub ts: Teamspace,
}

/// 工作空间是一个 Git 仓库（含 `.git` 目录）。
pub(crate) fn fixture() -> Fixture {
    let f = plain_fixture();
    fs::create_dir_all(f.ws.join(".git")).unwrap();
    f
}

/// 工作空间是普通文件夹（不是 Git 仓库）。
pub(crate) fn plain_fixture() -> Fixture {
    let dir = tempfile::tempdir().unwrap();
    let ws = dir.path().join("ws");
    fs::create_dir_all(&ws).unwrap();
    let config = HostConfig {
        mock_agent: Some(AgentCommand {
            program: MOCK_HOST.into(),
            args: vec![crate::mock_agent::MOCK_AGENT_FLAG.into()],
        }),
    };
    let ts = Teamspace::open_with(
        &dir.path().join("teamspace.db"),
        Box::new(FakeProbe),
        config,
    )
    .unwrap();
    Fixture { dir, ws, ts }
}

impl Fixture {
    pub fn db_path(&self) -> PathBuf {
        self.dir.path().join("teamspace.db")
    }

    /// 在工作区中创建文件（含父目录）。
    pub fn touch(&self, rel: &str) {
        let path = self.ws.join(rel);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, "x").unwrap();
    }

    /// 以全部岗位示例（模板）创建团队。
    pub fn team(&self, name: &str) -> Team {
        self.ts
            .create_team(CreateTeamInput {
                name: name.into(),
                description: String::new(),
                workspace_path: self.ws.to_string_lossy().into_owned(),
                roles: templates(),
            })
            .unwrap()
    }

    /// 团队中 code 对应的岗位。
    pub fn role(&self, team_id: &str, code: &str) -> Role {
        self.ts
            .list_roles(team_id)
            .unwrap()
            .into_iter()
            .map(|v| v.role)
            .find(|r| r.code == code)
            .unwrap()
    }

    pub fn harness(&self, team_id: &str, name: &str, preset: AcpPreset) -> HarnessInstance {
        self.ts
            .create_harness(CreateHarnessInput {
                team_id: team_id.into(),
                name: name.into(),
                preset,
                command: None,
                args: None,
                env_profile: None,
            })
            .unwrap()
    }

    /// 创建 TODO 任务。
    pub fn task(&self, team_id: &str, owner: &Role, title: &str) -> TaskDetail {
        self.try_task(team_id, owner, title, TaskStatus::Todo, vec![], vec![])
            .unwrap()
    }

    pub fn try_task(
        &self,
        team_id: &str,
        owner: &Role,
        title: &str,
        status: TaskStatus,
        refs: Vec<&str>,
        deps: Vec<String>,
    ) -> Result<TaskDetail> {
        self.ts.create_task(CreateTaskInput {
            team_id: team_id.into(),
            title: title.into(),
            description: String::new(),
            owner_role_id: owner.id.clone(),
            status,
            file_refs: refs.into_iter().map(file_ref).collect(),
            depends_on: deps,
        })
    }

    /// 创建一个满足就绪条件的 TODO 任务（引用的文件已存在）。
    pub fn readyable(&self, team_id: &str, owner: &Role, title: &str) -> TaskDetail {
        self.touch("docs/spec.md");
        self.try_task(
            team_id,
            owner,
            title,
            TaskStatus::Todo,
            vec!["docs/spec.md"],
            vec![],
        )
        .unwrap()
    }

    pub fn move_to(&self, task_id: &str, to: TaskStatus) -> Result<TaskDetail> {
        self.ts.change_task_status(ChangeTaskStatusInput {
            task_id: task_id.into(),
            to,
            reason: (to == TaskStatus::Blocked).then(|| "等待接口".to_owned()),
        })
    }

    /// 依次流转到目标状态。
    pub fn drive(&self, task_id: &str, path: &[TaskStatus]) -> TaskDetail {
        let mut last = self.ts.get_task(task_id).unwrap();
        for to in path {
            last = self.move_to(task_id, *to).unwrap();
        }
        last
    }

    /// 绕过内核直接执行 SQL（模拟其他途径造成的数据状态，如团队归档）。
    pub fn raw_sql(&self, sql: &str) {
        rusqlite::Connection::open(self.db_path())
            .unwrap()
            .execute_batch(sql)
            .unwrap();
    }
}

pub(crate) fn templates() -> Vec<NewRole> {
    role_templates()
        .into_iter()
        .map(|t| NewRole {
            name: t.name,
            code: Some(t.code),
            task_prefix: t.task_prefix,
            description: t.description,
            guidelines: t.guidelines,
            is_leader: t.is_leader,
        })
        .collect()
}

pub(crate) fn new_role(name: &str, code: &str, prefix: &str, is_leader: bool) -> NewRole {
    NewRole {
        code: Some(code.into()),
        is_leader,
        ..custom_role(name, prefix)
    }
}

/// 用户自定义岗位：不填 code（由后端生成），非 Leader。
pub(crate) fn custom_role(name: &str, prefix: &str) -> NewRole {
    NewRole {
        name: name.into(),
        code: None,
        task_prefix: prefix.into(),
        description: String::new(),
        guidelines: String::new(),
        is_leader: false,
    }
}

pub(crate) fn file_ref(path: &str) -> FileRef {
    FileRef {
        path: path.into(),
        anchor: None,
    }
}

#[track_caller]
pub(crate) fn assert_kind<T: std::fmt::Debug>(result: Result<T>, kind: ErrorKind) -> String {
    match result {
        Ok(value) => panic!("expected {kind:?} error, got Ok({value:?})"),
        Err(err) => {
            assert_eq!(err.kind(), kind, "unexpected error: {err}");
            err.to_string()
        }
    }
}
