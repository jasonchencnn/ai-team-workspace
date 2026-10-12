//! 通过 [`Teamspace`] 公开命令测试业务规则。全部使用临时目录和假探测，不调用真实 Git 或 CLI。

mod events;
mod harnesses;
mod roles;
mod tasks;
mod teams;

use std::fs;
use std::path::{Path, PathBuf};

use tempfile::TempDir;

use crate::Teamspace;
use crate::error::{Error, ErrorKind, Result};
use crate::model::*;
use crate::probe::SystemProbe;

/// 假探测：任何存在的目录都视为 Git 仓库顶层；文件搜索遍历目录；健康检查按约定返回。
pub(crate) struct FakeProbe;

/// 健康检查时 executable 为此路径则返回 error。
pub(crate) const BROKEN_EXECUTABLE: &str = "/broken/claude";

impl SystemProbe for FakeProbe {
    fn inspect_workspace(&self, path: &Path) -> Result<WorkspaceInfo> {
        if !path.is_dir() {
            return Err(Error::invalid("所选目录不是 Git 仓库"));
        }
        Ok(WorkspaceInfo {
            path: path.to_string_lossy().into_owned(),
            branch: Some("main".into()),
            commit: Some("a31f8c2".into()),
        })
    }

    fn search_files(&self, workspace: &Path, query: &str, limit: usize) -> Result<Vec<String>> {
        fn walk(root: &Path, dir: &Path, out: &mut Vec<String>) {
            for entry in fs::read_dir(dir).unwrap() {
                let path = entry.unwrap().path();
                if path.is_dir() {
                    walk(root, &path, out);
                } else {
                    let rel = path.strip_prefix(root).unwrap();
                    out.push(rel.to_string_lossy().replace('\\', "/"));
                }
            }
        }
        let mut out = Vec::new();
        walk(workspace, workspace, &mut out);
        out.sort();
        let query = query.to_lowercase();
        Ok(out
            .into_iter()
            .filter(|p| p.to_lowercase().contains(&query))
            .take(limit)
            .collect())
    }

    fn detect_harness(&self, kind: HarnessKind) -> HarnessDetection {
        HarnessDetection {
            kind,
            found: true,
            executable: Some("/usr/local/bin/fake".into()),
            version: Some("1.0.0".into()),
            error: None,
        }
    }

    fn check_harness(&self, harness: &HarnessInstance) -> HealthCheck {
        if harness.executable.as_deref() == Some(BROKEN_EXECUTABLE) {
            HealthCheck {
                status: HealthStatus::Error,
                version: None,
                detail: Some("无法运行可执行文件".into()),
                checked_at: None,
            }
        } else {
            HealthCheck {
                status: HealthStatus::Ok,
                version: Some("2.1.288 (Claude Code)".into()),
                detail: None,
                checked_at: None,
            }
        }
    }
}

pub(crate) struct Fixture {
    pub dir: TempDir,
    pub ws: PathBuf,
    pub ts: Teamspace,
}

pub(crate) fn fixture() -> Fixture {
    let dir = tempfile::tempdir().unwrap();
    let ws = dir.path().join("ws");
    fs::create_dir_all(&ws).unwrap();
    let ts = Teamspace::open(&dir.path().join("teamspace.db"), Box::new(FakeProbe)).unwrap();
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

    /// 以全部岗位模板创建团队。
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

    pub fn harness(&self, team_id: &str, name: &str, kind: HarnessKind) -> HarnessInstance {
        self.ts
            .create_harness(CreateHarnessInput {
                team_id: team_id.into(),
                name: name.into(),
                kind,
                executable: None,
                model: None,
                profile: None,
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
            code: t.code,
            task_prefix: t.task_prefix,
            description: t.description,
            guidelines: t.guidelines,
            is_leader: t.is_leader,
        })
        .collect()
}

pub(crate) fn new_role(name: &str, code: &str, prefix: &str, is_leader: bool) -> NewRole {
    NewRole {
        name: name.into(),
        code: code.into(),
        task_prefix: prefix.into(),
        description: String::new(),
        guidelines: String::new(),
        is_leader,
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
