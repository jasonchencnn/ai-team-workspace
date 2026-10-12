//! IPC 命令清单。
//!
//! [`Teamspace`] 的每个公开方法对应一个同名 Tauri 命令（M1-06 包装为 `#[tauri::command]`），
//! 前端唯一入口是 `apps/desktop/src/api/commands.ts`。
//!
//! 参数约定：ID 参数在前端以 camelCase 传入（`team_id` → `teamId`）；结构体参数统一命名为
//! `input`。所有命令失败时返回 [`crate::Error`]（前端收到 `ErrorPayload`）。
//!
//! 所有写操作在同一事务内完成数据变更并追加一条 [`crate::model::Event`]（actor 为 human，中文摘要由后端生成）。

use std::path::{Path, PathBuf};

use crate::error::Result;
use crate::model::*;
use crate::probe::SystemProbe;

/// 只在 Tauri 层实现、不经过 [`Teamspace`] 的命令。
pub const TAURI_ONLY_COMMANDS: &[&str] = &[
    // 打开系统文件夹对话框；用户取消返回 `null`，否则返回所选目录的工作区信息
    // （等同于对所选目录调用 `inspect_workspace`）。返回类型 `WorkspaceInfo | null`。
    "pick_workspace_folder",
];

/// [`Teamspace`] 对外的全部命令名，顺序同下方方法定义。
pub const COMMANDS: &[&str] = &[
    "list_teams",
    "create_team",
    "get_team_overview",
    "inspect_workspace",
    "list_role_templates",
    "list_roles",
    "create_role",
    "update_role",
    "archive_role",
    "bind_role_harness",
    "unbind_role_harness",
    "detect_harnesses",
    "list_harnesses",
    "create_harness",
    "update_harness",
    "archive_harness",
    "check_harness",
    "list_task_statuses",
    "get_task_board",
    "get_task",
    "create_task",
    "update_task",
    "change_task_status",
    "search_workspace_files",
    "list_events",
];

/// 内核入口：持有 SQLite 存储和系统探测实现。
pub struct Teamspace {
    #[allow(dead_code)] // M1-03 使用
    db_path: PathBuf,
    probe: Box<dyn SystemProbe>,
}

// M1-03 实现方法体后移除 `unused_variables`。
#[allow(unused_variables)]
impl Teamspace {
    /// 打开（必要时创建并迁移）`db_path` 处的数据库。
    pub fn open(db_path: &Path, probe: Box<dyn SystemProbe>) -> Result<Self> {
        todo!("M1-03")
    }

    // ---------------------------------------------------------------- 团队

    /// 列出未归档团队，按创建时间升序。
    pub fn list_teams(&self) -> Result<Vec<Team>> {
        todo!("M1-03")
    }

    /// 创建团队及初始岗位：名称必填且在未归档团队中唯一；工作区须为 Git 仓库并规范化为顶层目录；
    /// 初始岗位逐个按岗位规则校验（code / 前缀唯一、至多一个 Leader）。
    pub fn create_team(&self, input: CreateTeamInput) -> Result<Team> {
        todo!("M1-03")
    }

    /// 团队概览：团队、实时工作区状态（检测失败不报错，写入 `workspaceProblem`）、岗位卡片、
    /// 各状态任务数、进行中的任务和最近 10 条活动。
    pub fn get_team_overview(&self, team_id: &str) -> Result<TeamOverview> {
        todo!("M1-03")
    }

    /// 检测某个路径的 Git 工作区（创建团队前预览「代码库已检测：main @ a31f8c2」）。
    pub fn inspect_workspace(&self, path: &str) -> Result<WorkspaceInfo> {
        self.probe.inspect_workspace(Path::new(path))
    }

    // ---------------------------------------------------------------- 岗位

    /// 内置岗位模板（Leader / 产品经理 / 前端开发 / 后端开发 / QA 测试）。
    pub fn list_role_templates(&self) -> Vec<RoleTemplate> {
        role_templates()
    }

    /// 列出团队的未归档岗位卡片，Leader 在前，其余按创建时间。
    pub fn list_roles(&self, team_id: &str) -> Result<Vec<RoleView>> {
        todo!("M1-03")
    }

    /// 新增岗位：code 格式合法且团队内唯一（含已归档），前缀 2–4 位大写字母且在未归档岗位中唯一，
    /// 至多一个 Leader。
    pub fn create_role(&self, input: CreateRoleInput) -> Result<RoleView> {
        todo!("M1-03")
    }

    /// 编辑岗位名称、前缀、职责、工作约定和 Leader 标记；code 不可改，前缀修改不影响已有任务编号。
    pub fn update_role(&self, input: UpdateRoleInput) -> Result<RoleView> {
        todo!("M1-03")
    }

    /// 归档岗位；岗位仍负责未结束任务（非 done / cancelled）时拒绝（conflict）。
    pub fn archive_role(&self, role_id: &str) -> Result<Role> {
        todo!("M1-03")
    }

    /// 绑定或更换岗位的 Harness；Harness 须为同团队未归档实例。记录活动。
    pub fn bind_role_harness(&self, role_id: &str, harness_id: &str) -> Result<RoleView> {
        todo!("M1-03")
    }

    /// 解除岗位的 Harness 绑定（岗位变为「暂不派工」）。记录活动。
    pub fn unbind_role_harness(&self, role_id: &str) -> Result<RoleView> {
        todo!("M1-03")
    }

    // ---------------------------------------------------------------- Harness

    /// 检测本机 Claude Code 与 Codex CLI（不含 mock），不写入任何数据。
    pub fn detect_harnesses(&self) -> Vec<HarnessDetection> {
        [HarnessKind::ClaudeCode, HarnessKind::Codex]
            .into_iter()
            .map(|kind| self.probe.detect_harness(kind))
            .collect()
    }

    /// 列出团队的未归档 Harness 实例，按创建时间升序。
    pub fn list_harnesses(&self, team_id: &str) -> Result<Vec<HarnessInstance>> {
        todo!("M1-03")
    }

    /// 注册 Harness 实例：名称在团队未归档实例中唯一；executable 若填写须为绝对路径；
    /// 空白字符串视为未填写。健康状态初始为 unknown。
    pub fn create_harness(&self, input: CreateHarnessInput) -> Result<HarnessInstance> {
        todo!("M1-03")
    }

    /// 编辑 Harness 实例（kind 不可改）；健康状态重置为 unknown。
    pub fn update_harness(&self, input: UpdateHarnessInput) -> Result<HarnessInstance> {
        todo!("M1-03")
    }

    /// 归档 Harness 实例；仍有未归档岗位绑定它时拒绝（conflict）。
    pub fn archive_harness(&self, harness_id: &str) -> Result<HarnessInstance> {
        todo!("M1-03")
    }

    /// 对实例做健康检查并持久化结果（mock 直接为 ok）；检查失败不是命令错误，体现在 `health` 中。
    pub fn check_harness(&self, harness_id: &str) -> Result<HarnessInstance> {
        todo!("M1-03")
    }

    // ---------------------------------------------------------------- 任务

    /// 全部任务状态及显示名，按标准顺序。
    pub fn list_task_statuses(&self) -> Vec<TaskStatusInfo> {
        TaskStatus::ALL.into_iter().map(Into::into).collect()
    }

    /// 看板数据：按状态分组，始终包含 done 列，cancelled 列仅在 `includeCancelled` 时包含。
    pub fn get_task_board(&self, input: TaskBoardQuery) -> Result<TaskBoard> {
        todo!("M1-03")
    }

    /// 任务详情：依赖、下游、文件存在情况、相关活动、可执行的流转和就绪问题。
    pub fn get_task(&self, task_id: &str) -> Result<TaskDetail> {
        todo!("M1-03")
    }

    /// 创建任务：标题、负责岗位必填；初始状态只能是 todo 或 ready（ready 需满足就绪条件）；
    /// 编号取负责岗位前缀的下一个序号；引用路径须为工作区内相对路径；依赖须为同团队任务且不成环。
    pub fn create_task(&self, input: CreateTaskInput) -> Result<TaskDetail> {
        todo!("M1-03")
    }

    /// 编辑任务内容（不改状态，编号不变）；保存依赖时拒绝成环并指出环路；
    /// 任务处于 ready 而修改后不再满足就绪条件时拒绝（conflict）。
    pub fn update_task(&self, input: UpdateTaskInput) -> Result<TaskDetail> {
        todo!("M1-03")
    }

    /// 按流转规则变更状态：进入 ready 需满足就绪条件（有负责人、≥1 个存在于工作区的引用、
    /// 所有依赖已完成）；进入 blocked 必须给出原因，离开 blocked 清空原因。
    pub fn change_task_status(&self, input: ChangeTaskStatusInput) -> Result<TaskDetail> {
        todo!("M1-03")
    }

    /// 在团队工作区中搜索文件（用于添加引用），返回相对路径，最多 50 条。
    pub fn search_workspace_files(&self, input: FileSearchQuery) -> Result<Vec<String>> {
        todo!("M1-03")
    }

    // ---------------------------------------------------------------- 活动

    /// 按团队分页列出活动，时间倒序；每页默认 50 条、最多 200 条。
    pub fn list_events(&self, input: EventQuery) -> Result<EventPage> {
        todo!("M1-03")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 前端命令封装必须与命令清单一一对应。
    #[test]
    fn frontend_wrapper_covers_every_command() {
        let path =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../../apps/desktop/src/api/commands.ts");
        let source = std::fs::read_to_string(&path).expect("read commands.ts");
        let invoked: Vec<&str> = source
            .split("invoke<")
            .skip(1)
            .filter_map(|rest| rest.split_once("(\"").map(|(_, tail)| tail))
            .filter_map(|tail| tail.split('"').next())
            .collect();
        let mut expected: Vec<&str> = COMMANDS
            .iter()
            .chain(TAURI_ONLY_COMMANDS)
            .copied()
            .collect();
        let mut actual = invoked.clone();
        expected.sort_unstable();
        actual.sort_unstable();
        assert_eq!(actual, expected, "commands.ts 与 api::COMMANDS 不一致");
    }
}
