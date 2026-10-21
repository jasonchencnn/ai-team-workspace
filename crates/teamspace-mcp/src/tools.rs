//! MCP 工具：Leader 通过它们操作 Teamspace。
//!
//! 每个工具只做两件事：把 Leader 说的名字/编号解析成 id，再把请求转给桌面应用的本机控制
//! socket。权限边界在桌面应用的内核里把守（D7、D8、D26）：这里不提供合并、换绑、权限答复
//! 等越权操作；后端拒绝时把中文原因原样带回。
//!
//! 工具失败（应用没运行、名字解析不到、后端拒绝）都作为「工具级错误」返回，
//! Leader 能直接看到中文说明，不会看到协议错误。

use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::{CallToolResult, ContentBlock, Implementation, ServerCapabilities, ServerConfig};
use rmcp::{ServerHandler, tool, tool_handler, tool_router};
use serde_json::{Value, json};

use crate::client::{ControlClient, ControlError};
use crate::error::ToolError;
use crate::params::{
    ActivityArgs, BoardArgs, CancelRunArgs, ChangeStatusArgs, CreateTaskArgs, RejectReviewArgs,
    RunProgressArgs, RunStatusArgs, SetDependenciesArgs, TaskArgs, TeamArgs, UpdateTaskArgs,
};
use crate::resolve::{self, TaskRef, TeamRef};

/// 任务状态里 Leader 不能设置的：置为已完成要走验收（合并），只能由你在应用里点（D7、D8）。
const DONE_STATUS: &str = "done";

/// Leader 可以设置的任务状态。
const ALLOWED_STATUS: &[&str] = &["todo", "ready", "doing", "blocked", "review", "cancelled"];

/// Teamspace 的 MCP 服务：把工具调用转给桌面应用。
#[derive(Debug, Clone)]
pub struct TeamspaceServer {
    client: ControlClient,
}

impl TeamspaceServer {
    /// 用给定的控制 socket 客户端建服务。
    pub fn new(client: ControlClient) -> Self {
        Self { client }
    }

    /// 控制 socket 客户端。
    pub fn client(&self) -> &ControlClient {
        &self.client
    }

    /// 解析团队：省略时只有一个团队才认。
    async fn team(&self, reference: Option<&str>) -> Result<TeamRef, ToolError> {
        resolve::resolve_team(&self.client, reference).await
    }

    /// 解析团队：省略时返回 `None`（供任务编号在多个团队重复的场景）。
    async fn team_hint(&self, reference: Option<&str>) -> Result<Option<TeamRef>, ToolError> {
        match clean(reference) {
            Some(reference) => Ok(Some(self.team(Some(reference)).await?)),
            None => Ok(None),
        }
    }

    /// 解析任务。
    async fn task(&self, team: Option<&TeamRef>, reference: &str) -> Result<TaskRef, ToolError> {
        resolve::resolve_task(&self.client, team, reference).await
    }

    /// 解析任务：团队可选（省略时在所有团队里找）。
    async fn task_of(&self, team: Option<&str>, reference: &str) -> Result<TaskRef, ToolError> {
        let team = self.team_hint(team).await?;
        self.task(team.as_ref(), reference).await
    }

    /// 取出要看的执行：给了 `run` 就用它，给了 `task` 就取该任务最近一次执行。
    async fn pick_run(
        &self,
        run: Option<&str>,
        task: Option<&str>,
        team: Option<&str>,
    ) -> Result<Value, ToolError> {
        if let Some(run_id) = clean(run) {
            return resolve::run_by_id(&self.client, run_id).await;
        }
        let Some(task_reference) = clean(task) else {
            return Err(ToolError::Message(
                "请指明任务（task）或执行（run）。".to_owned(),
            ));
        };
        let team = self.team_hint(team).await?;
        let task = self.task(team.as_ref(), task_reference).await?;
        resolve::latest_run(&self.client, &task).await
    }
}

#[tool_router]
impl TeamspaceServer {
    /// 列出所有团队。
    #[tool(
        name = "list_teams",
        description = "列出 Teamspace 里的所有团队（id、名称、工作空间目录）。",
        annotations(read_only_hint = true)
    )]
    async fn list_teams(&self) -> CallToolResult {
        report(self.client.call("list_teams", json!({})).await)
    }

    /// 团队概览。
    #[tool(
        name = "team_overview",
        description = "查看团队概览：工作空间、岗位卡片、各状态任务数、进行中的任务与最近活动。",
        annotations(read_only_hint = true)
    )]
    async fn team_overview(&self, Parameters(args): Parameters<TeamArgs>) -> CallToolResult {
        match self.team(args.team.as_deref()).await {
            Ok(team) => report(
                self.client
                    .call("get_team_overview", json!({ "teamId": team.id }))
                    .await,
            ),
            Err(err) => fail(err.message()),
        }
    }

    /// 岗位列表。
    #[tool(
        name = "list_roles",
        description = "列出团队的岗位：名称、岗位标识、任务编号前缀、绑定的 Harness 与任务统计。",
        annotations(read_only_hint = true)
    )]
    async fn list_roles(&self, Parameters(args): Parameters<TeamArgs>) -> CallToolResult {
        match self.team(args.team.as_deref()).await {
            Ok(team) => report(
                self.client
                    .call("list_roles", json!({ "teamId": team.id }))
                    .await,
            ),
            Err(err) => fail(err.message()),
        }
    }

    /// Harness 列表。
    #[tool(
        name = "list_harnesses",
        description = "列出团队注册的 Harness（Agent 接入）实例及其最近的握手状态。",
        annotations(read_only_hint = true)
    )]
    async fn list_harnesses(&self, Parameters(args): Parameters<TeamArgs>) -> CallToolResult {
        match self.team(args.team.as_deref()).await {
            Ok(team) => report(
                self.client
                    .call("list_harnesses", json!({ "teamId": team.id }))
                    .await,
            ),
            Err(err) => fail(err.message()),
        }
    }

    /// 任务看板。
    #[tool(
        name = "task_board",
        description = "查看任务看板：按状态分列的任务（编号、标题、负责岗位、依赖数量）。",
        annotations(read_only_hint = true)
    )]
    async fn task_board(&self, Parameters(args): Parameters<BoardArgs>) -> CallToolResult {
        match self.team(args.team.as_deref()).await {
            Ok(team) => report(
                self.client
                    .call(
                        "get_task_board",
                        json!({ "input": {
                            "teamId": team.id,
                            "includeCancelled": args.include_cancelled.unwrap_or(false),
                        }}),
                    )
                    .await,
            ),
            Err(err) => fail(err.message()),
        }
    }

    /// 任务详情。
    #[tool(
        name = "get_task",
        description = "查看任务详情：说明、负责岗位、依赖与下游、可执行的状态流转、最近执行与相关活动。",
        annotations(read_only_hint = true)
    )]
    async fn get_task(&self, Parameters(args): Parameters<TaskArgs>) -> CallToolResult {
        match self.task_of(args.team.as_deref(), &args.task).await {
            Ok(task) => report(
                self.client
                    .call("get_task", json!({ "taskId": task.id }))
                    .await,
            ),
            Err(err) => fail(err.message()),
        }
    }

    /// 最新执行的状态。
    #[tool(
        name = "run_status",
        description = "查看最近一次执行的状态、摘要、改动文件与失败原因（可按任务或执行 id）。",
        annotations(read_only_hint = true)
    )]
    async fn run_status(&self, Parameters(args): Parameters<RunStatusArgs>) -> CallToolResult {
        match self
            .pick_run(
                args.run.as_deref(),
                args.task.as_deref(),
                args.team.as_deref(),
            )
            .await
        {
            Ok(run) => ok(run_summary(&run)),
            Err(err) => fail(err.message()),
        }
    }

    /// 执行进度。
    #[tool(
        name = "run_progress",
        description = "读取执行进度：Agent 的消息、计划、工具调用、权限请求与状态变化；用 nextCursor 续读。",
        annotations(read_only_hint = true)
    )]
    async fn run_progress(&self, Parameters(args): Parameters<RunProgressArgs>) -> CallToolResult {
        let run = match self
            .pick_run(
                args.run.as_deref(),
                args.task.as_deref(),
                args.team.as_deref(),
            )
            .await
        {
            Ok(run) => run,
            Err(err) => return fail(err.message()),
        };
        let run_id = run.get("id").and_then(Value::as_str).unwrap_or_default();
        let mut input = json!({ "runId": run_id });
        if let Some(cursor) = clean(args.cursor.as_deref()) {
            input["cursor"] = json!(cursor);
        }
        if let Some(limit) = args.limit {
            input["limit"] = json!(limit);
        }
        match self
            .client
            .call("list_run_updates", json!({ "input": input }))
            .await
        {
            Ok(page) => ok(render_progress(&page)),
            Err(err) => fail(err.message()),
        }
    }

    /// 活动记录。
    #[tool(
        name = "list_activity",
        description = "查活动与审计记录：谁在什么时候做了什么；可按任务或岗位筛选，用 nextCursor 翻页。",
        annotations(read_only_hint = true)
    )]
    async fn list_activity(&self, Parameters(args): Parameters<ActivityArgs>) -> CallToolResult {
        match self.activity(args).await {
            Ok(value) => ok(value),
            Err(err) => fail(err.message()),
        }
    }

    /// 验收视图。
    #[tool(
        name = "get_review",
        description = "查看待验收任务：执行摘要、改动文件与逐文件改动、通过/退回是否可用、历史退回意见。",
        annotations(read_only_hint = true)
    )]
    async fn get_review(&self, Parameters(args): Parameters<TaskArgs>) -> CallToolResult {
        match self.task_of(args.team.as_deref(), &args.task).await {
            Ok(task) => report(
                self.client
                    .call("get_review", json!({ "taskId": task.id }))
                    .await,
            ),
            Err(err) => fail(err.message()),
        }
    }

    /// 待答复的权限请求。
    #[tool(
        name = "pending_permissions",
        description = "查询正在执行、等待你答复的权限请求（只读）。答复只能由你在 Teamspace 应用里点。",
        annotations(read_only_hint = true)
    )]
    async fn pending_permissions(&self, Parameters(args): Parameters<TeamArgs>) -> CallToolResult {
        match self.pending(args.team.as_deref()).await {
            Ok(value) => ok(value),
            Err(err) => fail(err.message()),
        }
    }

    /// 创建任务。
    #[tool(
        name = "create_task",
        description = "创建任务：标题、说明、负责岗位、依赖与初始状态（todo / ready）。任务出现在看板上。\
                       文件引用写成「路径」或「路径#锚点」（如 docs/spec.md#验收标准），锚点会一起存下来。",
        annotations(destructive_hint = false)
    )]
    async fn create_task(&self, Parameters(args): Parameters<CreateTaskArgs>) -> CallToolResult {
        match self.create(args).await {
            Ok(value) => ok(value),
            Err(err) => fail(err.message()),
        }
    }

    /// 编辑任务。
    #[tool(
        name = "update_task",
        description = "编辑任务：改标题、说明、负责岗位、依赖或文件引用；没给的字段保持原样，编号不变。\
                       文件引用写成「路径」或「路径#锚点」（如 docs/spec.md#验收标准），锚点会一起写回。",
        annotations(destructive_hint = false)
    )]
    async fn update_task(&self, Parameters(args): Parameters<UpdateTaskArgs>) -> CallToolResult {
        match self.update(args).await {
            Ok(value) => ok(value),
            Err(err) => fail(err.message()),
        }
    }

    /// 设置依赖。
    #[tool(
        name = "set_dependencies",
        description = "设置任务的依赖（前置任务）：传编号或标题列表，空数组表示清空依赖；成环会被拒绝。",
        annotations(destructive_hint = false)
    )]
    async fn set_dependencies(
        &self,
        Parameters(args): Parameters<SetDependenciesArgs>,
    ) -> CallToolResult {
        match self.dependencies(args).await {
            Ok(value) => ok(value),
            Err(err) => fail(err.message()),
        }
    }

    /// 变更任务状态。
    #[tool(
        name = "change_task_status",
        description = "变更任务状态：todo / ready / doing / blocked / review / cancelled。不能置为已完成（需在应用里验收）。",
        annotations(destructive_hint = false)
    )]
    async fn change_task_status(
        &self,
        Parameters(args): Parameters<ChangeStatusArgs>,
    ) -> CallToolResult {
        match self.change_status(args).await {
            Ok(value) => ok(value),
            Err(err) => fail(err.message()),
        }
    }

    /// 开始执行。
    #[tool(
        name = "start_run",
        description = "开始执行任务：任务转为执行中，由负责岗位绑定的 Harness 干活。",
        annotations(destructive_hint = false)
    )]
    async fn start_run(&self, Parameters(args): Parameters<TaskArgs>) -> CallToolResult {
        match self.task_of(args.team.as_deref(), &args.task).await {
            Ok(task) => report(
                self.client
                    .call("start_run", json!({ "taskId": task.id }))
                    .await,
            ),
            Err(err) => fail(err.message()),
        }
    }

    /// 取消执行。
    #[tool(
        name = "cancel_run",
        description = "取消正在进行的执行：通知 Agent 停止，任务回到 READY。可按执行 id 或任务指定。",
        annotations(destructive_hint = true)
    )]
    async fn cancel_run(&self, Parameters(args): Parameters<CancelRunArgs>) -> CallToolResult {
        match self
            .pick_run(
                args.run.as_deref(),
                args.task.as_deref(),
                args.team.as_deref(),
            )
            .await
        {
            Ok(run) => {
                let run_id = run.get("id").and_then(Value::as_str).unwrap_or_default();
                report(
                    self.client
                        .call("cancel_run", json!({ "runId": run_id }))
                        .await,
                )
            }
            Err(err) => fail(err.message()),
        }
    }

    /// 退回验收。
    #[tool(
        name = "reject_review",
        description = "退回验收：必须写意见，Agent 会在同一执行目录续接会话重做。通过并合并仍由你在应用里点。",
        annotations(destructive_hint = false)
    )]
    async fn reject_review(
        &self,
        Parameters(args): Parameters<RejectReviewArgs>,
    ) -> CallToolResult {
        match self.reject(args).await {
            Ok(value) => ok(value),
            Err(err) => fail(err.message()),
        }
    }
}

#[tool_handler(router = Self::tool_router())]
impl ServerHandler for TeamspaceServer {
    fn get_info(&self) -> ServerConfig {
        ServerConfig::new(ServerCapabilities::builder().enable_tools().build())
            .with_server_info(Implementation::new("teamspace", env!("CARGO_PKG_VERSION")))
            .with_instructions(
                "Teamspace 的 Leader 工具箱：查看团队、岗位、Harness、看板、任务、执行进度与结果、\
                 活动与验收；创建与编辑任务、设置依赖、开始与取消执行、退回验收。\
                 所有操作都转给正在运行的 Teamspace 桌面应用，合并到主分支、换绑 Harness、\
                 答复权限请求只能由你在 Teamspace 应用中操作。"
                    .to_owned(),
            )
    }
}

impl TeamspaceServer {
    /// 活动记录。
    async fn activity(&self, args: ActivityArgs) -> Result<Value, ToolError> {
        let team_hint = self.team_hint(args.team.as_deref()).await?;
        let task = match clean(args.task.as_deref()) {
            Some(reference) => Some(self.task(team_hint.as_ref(), reference).await?),
            None => None,
        };
        let team = match team_hint {
            Some(team) => team,
            None => match &task {
                Some(task) => TeamRef {
                    id: task.team_id.clone(),
                    name: task.team_name.clone(),
                },
                None => self.team(None).await?,
            },
        };
        let role = match clean(args.role.as_deref()) {
            Some(reference) => Some(resolve::resolve_role(&self.client, &team, reference).await?),
            None => None,
        };
        let mut input = json!({ "teamId": team.id });
        if let Some(task) = &task {
            input["taskId"] = json!(task.id);
        }
        if let Some(role) = &role {
            input["roleId"] = json!(role.id);
        }
        if let Some(cursor) = clean(args.cursor.as_deref()) {
            input["cursor"] = json!(cursor);
        }
        if let Some(limit) = args.limit {
            input["limit"] = json!(limit);
        }
        Ok(self
            .client
            .call("list_events", json!({ "input": input }))
            .await?)
    }

    /// 待答复的权限请求（只读）。
    async fn pending(&self, team_reference: Option<&str>) -> Result<Value, ToolError> {
        let teams = match clean(team_reference) {
            Some(reference) => vec![self.team(Some(reference)).await?],
            None => resolve::teams(&self.client).await?,
        };
        let mut items: Vec<Value> = Vec::new();
        for team in teams {
            let board = self
                .client
                .call("get_task_board", json!({ "input": { "teamId": team.id } }))
                .await?;
            let Some(columns) = board.get("columns").and_then(Value::as_array) else {
                continue;
            };
            for column in columns {
                if column.get("status").and_then(Value::as_str) != Some("doing") {
                    continue;
                }
                let Some(tasks) = column.get("tasks").and_then(Value::as_array) else {
                    continue;
                };
                for task in tasks {
                    let task_id = task.get("id").and_then(Value::as_str).unwrap_or_default();
                    let runs = self
                        .client
                        .call("list_runs", json!({ "taskId": task_id }))
                        .await?;
                    let Some(runs) = runs.as_array() else {
                        continue;
                    };
                    for run in runs {
                        let Some(prompt) = run.get("pendingPermission").filter(|p| !p.is_null())
                        else {
                            continue;
                        };
                        items.push(json!({
                            "team": team.name,
                            "taskId": task_id,
                            "taskKey": task.get("key"),
                            "taskTitle": task.get("title"),
                            "runId": run.get("id"),
                            "status": run.get("status"),
                            "requestedAt": prompt.get("requestedAt"),
                            "title": prompt.get("title"),
                            "toolKind": prompt.get("toolKind"),
                            "paths": prompt.get("paths"),
                            "command": prompt.get("command"),
                            "reason": prompt.get("reason"),
                            "options": prompt.get("options"),
                        }));
                    }
                }
            }
        }
        Ok(json!({
            "count": items.len(),
            "permissions": items,
            "hint": "权限请求只能由你在 Teamspace 应用中答复；这里只做查询与提醒。",
        }))
    }

    /// 创建任务。
    async fn create(&self, args: CreateTaskArgs) -> Result<Value, ToolError> {
        let title = args.title.trim().to_owned();
        if title.is_empty() {
            return Err(ToolError::Message("请给任务写个标题。".to_owned()));
        }
        let team = self.team(args.team.as_deref()).await?;
        let role = resolve::resolve_role(&self.client, &team, &args.role).await?;
        let mut depends_on: Vec<Value> = Vec::new();
        for reference in &args.depends_on {
            let task = self.task(Some(&team), reference).await?;
            depends_on.push(json!(task.id));
        }
        let status = match clean(args.status.as_deref()) {
            None | Some("todo") => "todo",
            Some("ready") => "ready",
            Some(other) => {
                return Err(ToolError::Message(format!(
                    "初始状态只能是 todo 或 ready（收到的是「{other}」）。"
                )));
            }
        };
        let file_refs: Vec<Value> = file_refs(&args.file_refs);
        let input = json!({
            "teamId": team.id,
            "title": title,
            "description": args.description.unwrap_or_default(),
            "ownerRoleId": role.id,
            "status": status,
            "fileRefs": file_refs,
            "dependsOn": depends_on,
        });
        Ok(self
            .client
            .call("create_task", json!({ "input": input }))
            .await?)
    }

    /// 编辑任务。
    async fn update(&self, args: UpdateTaskArgs) -> Result<Value, ToolError> {
        let team_hint = self.team_hint(args.team.as_deref()).await?;
        let task = self.task(team_hint.as_ref(), &args.task).await?;
        let detail = self
            .client
            .call("get_task", json!({ "taskId": task.id }))
            .await?;
        let current = detail.get("task").cloned().unwrap_or(Value::Null);
        let team = TeamRef {
            id: task.team_id.clone(),
            name: task.team_name.clone(),
        };
        let title = match clean(args.title.as_deref()) {
            Some(title) => title.to_owned(),
            None => string_of(&current, "title"),
        };
        let description = match args.description.as_deref() {
            Some(description) => description.to_owned(),
            None => string_of(&current, "description"),
        };
        let owner_role_id = match clean(args.role.as_deref()) {
            Some(reference) => {
                resolve::resolve_role(&self.client, &team, reference)
                    .await?
                    .id
            }
            None => string_of(&current, "ownerRoleId"),
        };
        let depends_on: Value = match &args.depends_on {
            Some(list) => json!(self.resolve_tasks(&team, list).await?),
            None => current
                .get("dependsOn")
                .cloned()
                .unwrap_or_else(|| json!([])),
        };
        let file_refs: Value = match &args.file_refs {
            Some(list) => json!(file_refs(list)),
            None => current
                .get("fileRefs")
                .cloned()
                .unwrap_or_else(|| json!([])),
        };
        let input = json!({
            "taskId": task.id,
            "title": title,
            "description": description,
            "ownerRoleId": owner_role_id,
            "fileRefs": file_refs,
            "dependsOn": depends_on,
        });
        Ok(self
            .client
            .call("update_task", json!({ "input": input }))
            .await?)
    }

    /// 设置依赖。
    async fn dependencies(&self, args: SetDependenciesArgs) -> Result<Value, ToolError> {
        let team_hint = self.team_hint(args.team.as_deref()).await?;
        let task = self.task(team_hint.as_ref(), &args.task).await?;
        let team = TeamRef {
            id: task.team_id.clone(),
            name: task.team_name.clone(),
        };
        let depends_on = self.resolve_tasks(&team, &args.depends_on).await?;
        // 先看会不会成环（内核也会拦，这里先给一句清楚的话）。
        let check = self
            .client
            .call(
                "check_task_dependencies",
                json!({ "input": {
                    "teamId": team.id,
                    "taskId": task.id,
                    "dependsOn": depends_on,
                }}),
            )
            .await?;
        if let Some(cycle) = check.get("cycle").and_then(Value::as_array)
            && !cycle.is_empty()
        {
            let loop_chain = cycle
                .iter()
                .map(|item| item.as_str().unwrap_or_default().to_owned())
                .collect::<Vec<_>>()
                .join(" → ");
            return Err(ToolError::Message(format!(
                "这样设置会形成循环依赖：{loop_chain}"
            )));
        }
        let detail = self
            .client
            .call("get_task", json!({ "taskId": task.id }))
            .await?;
        let current = detail.get("task").cloned().unwrap_or(Value::Null);
        let input = json!({
            "taskId": task.id,
            "title": string_of(&current, "title"),
            "description": string_of(&current, "description"),
            "ownerRoleId": string_of(&current, "ownerRoleId"),
            "fileRefs": current.get("fileRefs").cloned().unwrap_or_else(|| json!([])),
            "dependsOn": depends_on,
        });
        Ok(self
            .client
            .call("update_task", json!({ "input": input }))
            .await?)
    }

    /// 变更任务状态。
    async fn change_status(&self, args: ChangeStatusArgs) -> Result<Value, ToolError> {
        let team_hint = self.team_hint(args.team.as_deref()).await?;
        let task = self.task(team_hint.as_ref(), &args.task).await?;
        let to = args.to.trim().to_ascii_lowercase();
        if to == DONE_STATUS {
            return Err(ToolError::Message(
                "不能把任务置为已完成：通过验收（合并）只能由你在 Teamspace 应用中执行。"
                    .to_owned(),
            ));
        }
        if !ALLOWED_STATUS.contains(&to.as_str()) {
            return Err(ToolError::Message(format!(
                "任务状态只能是 {}（收到的是「{}」）。",
                ALLOWED_STATUS.join("、"),
                args.to
            )));
        }
        let mut input = json!({ "taskId": task.id, "to": to });
        if let Some(reason) = clean(args.reason.as_deref()) {
            input["reason"] = json!(reason);
        }
        Ok(self
            .client
            .call("change_task_status", json!({ "input": input }))
            .await?)
    }

    /// 退回验收。
    async fn reject(&self, args: RejectReviewArgs) -> Result<Value, ToolError> {
        let comment = args.comment.trim().to_owned();
        if comment.is_empty() {
            return Err(ToolError::Message(
                "退回验收必须写意见：请说明要改什么、怎么改。".to_owned(),
            ));
        }
        let run = self
            .pick_run(
                args.run.as_deref(),
                args.task.as_deref(),
                args.team.as_deref(),
            )
            .await?;
        let run_id = run.get("id").and_then(Value::as_str).unwrap_or_default();
        let input = json!({ "runId": run_id, "comment": comment });
        Ok(self
            .client
            .call("reject_review", json!({ "input": input }))
            .await?)
    }

    /// 把一组任务引用解析成 id 列表。
    async fn resolve_tasks(
        &self,
        team: &TeamRef,
        references: &[String],
    ) -> Result<Vec<Value>, ToolError> {
        let mut ids: Vec<Value> = Vec::new();
        for reference in references {
            let task = self.task(Some(team), reference).await?;
            ids.push(json!(task.id));
        }
        Ok(ids)
    }
}

/// 把一组文件引用解析成内核要的形状：`path` 与 `anchor` 分开。
///
/// 允许 `path#anchor` 形式（如 `docs/spec.md#验收标准`）：`#` 之后是锚点，
/// 不含 `#` 时锚点为空。锚点不能再被悄悄丢掉——它常是任务的验收依据。
fn file_refs(references: &[String]) -> Vec<Value> {
    references
        .iter()
        .filter_map(|raw| {
            let text = raw.trim();
            if text.is_empty() {
                return None;
            }
            let (path, anchor) = match text.split_once('#') {
                Some((path, anchor)) => (path.trim(), Some(anchor.trim())),
                None => (text, None),
            };
            if path.is_empty() {
                return None;
            }
            let anchor = anchor.filter(|anchor| !anchor.is_empty());
            Some(json!({ "path": path, "anchor": anchor }))
        })
        .collect()
}

/// 成功的工具结果：把数据渲染成便于 Leader 阅读的 JSON。
fn ok(value: Value) -> CallToolResult {
    let text = serde_json::to_string_pretty(&value).unwrap_or_else(|_| value.to_string());
    CallToolResult::success(vec![ContentBlock::text(text)])
}

/// 失败结果：中文说明。
fn fail(message: impl Into<String>) -> CallToolResult {
    CallToolResult::error(vec![ContentBlock::text(message.into())])
}

/// 转发一条命令的结果。
fn report(result: Result<Value, ControlError>) -> CallToolResult {
    match result {
        Ok(value) => ok(value),
        Err(err) => fail(err.message()),
    }
}

/// 空的引用按「没给」处理。
fn clean(reference: Option<&str>) -> Option<&str> {
    reference.map(str::trim).filter(|value| !value.is_empty())
}

/// 取字符串字段。
fn string_of(value: &Value, key: &str) -> String {
    value
        .get(key)
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_owned()
}

/// 一次执行里 Leader 最关心的几项。
fn run_summary(run: &Value) -> Value {
    json!({
        "runId": run.get("id"),
        "taskId": run.get("taskId"),
        "taskKey": run.get("taskKey"),
        "roleId": run.get("roleId"),
        "harness": run.pointer("/harness/name"),
        "attempt": run.get("attempt"),
        "status": run.get("status"),
        "statusLabel": run.get("statusLabel"),
        "startedAt": run.get("startedAt"),
        "endedAt": run.get("endedAt"),
        "summary": run.get("summary"),
        "changedFiles": run.get("changedFiles"),
        "changeNote": run.get("changeNote"),
        "noChanges": run.get("noChanges"),
        "outOfScope": run.get("outOfScope"),
        "failure": run.get("failure"),
        "failureKind": run.get("failureKind"),
        "pendingPermission": run.get("pendingPermission"),
        "review": run.get("review"),
        "isolation": run.get("isolation"),
        "execDir": run.get("execDir"),
    })
}

/// 进度条目：把内核的结构化条目翻译成一句中文。
fn render_progress(page: &Value) -> Value {
    let mut updates: Vec<Value> = Vec::new();
    if let Some(list) = page.get("updates").and_then(Value::as_array) {
        for update in list {
            let body = update.get("body").cloned().unwrap_or(Value::Null);
            let (kind, summary) = update_summary(&body);
            updates.push(json!({
                "seq": update.get("seq"),
                "id": update.get("id"),
                "kind": kind,
                "summary": summary,
                "at": update.get("updatedAt"),
            }));
        }
    }
    json!({
        "runId": page.get("runId"),
        "status": page.get("status"),
        "hasMore": page.get("hasMore"),
        "nextCursor": page.get("nextCursor"),
        "updates": updates,
    })
}

/// 一条进度条目的中文摘要。
fn update_summary(body: &Value) -> (String, String) {
    let kind = body
        .get("type")
        .and_then(Value::as_str)
        .unwrap_or("unknown")
        .to_owned();
    let text = match kind.as_str() {
        "message" => {
            let role = body.get("role").and_then(Value::as_str).unwrap_or("agent");
            let content = string_of(body, "text");
            match role {
                "agent" => format!("Agent：{content}"),
                "thought" => format!("Agent 思考：{content}"),
                _ => format!("发出去的消息：{content}"),
            }
        }
        "plan" => {
            let entries = body
                .get("entries")
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default();
            let done = entries
                .iter()
                .filter(|entry| entry.get("status").and_then(Value::as_str) == Some("completed"))
                .count();
            format!("计划：共 {} 项，已完成 {done} 项", entries.len())
        }
        "tool_call" => {
            let title = string_of(body, "title");
            let status = string_of(body, "status");
            let status = if status.is_empty() {
                String::new()
            } else {
                format!("（{status}）")
            };
            format!("工具调用：{title}{status}")
        }
        "permission" => {
            let title = string_of(body, "title");
            if body.get("decision").is_some_and(|d| !d.is_null()) {
                format!("权限请求：{title}（已答复）")
            } else {
                format!("权限请求：{title}（等你或用户在 Teamspace 应用里答复）")
            }
        }
        "file_write" => {
            let path = string_of(body, "path");
            let allowed = body
                .get("allowed")
                .and_then(Value::as_bool)
                .unwrap_or(false);
            if allowed {
                format!("写入 {path}：已允许")
            } else {
                format!("写入 {path}：已拒绝（{}）", string_of(body, "reason"))
            }
        }
        "status" => {
            let status = string_of(body, "status");
            let detail = string_of(body, "detail");
            if detail.is_empty() {
                format!("状态：{status}")
            } else {
                format!("状态：{status}（{detail}）")
            }
        }
        "warning" => format!("注意：{}", string_of(body, "text")),
        _ => String::new(),
    };
    (kind, text)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn run_summary_picks_the_fields_a_leader_needs() {
        let run = json!({
            "id": "r1", "status": "failed", "summary": "做了一半",
            "changedFiles": [{"path": "a.rs", "change": "modified"}],
            "failure": "需要登录", "failureKind": "needs_auth",
        });
        let summary = run_summary(&run);
        assert_eq!(summary["status"], "failed");
        assert_eq!(summary["summary"], "做了一半");
        assert_eq!(summary["failure"], "需要登录");
        assert_eq!(summary["changedFiles"][0]["path"], "a.rs");
    }

    #[test]
    fn progress_entries_are_translated_to_chinese() {
        let (kind, text) = update_summary(&json!({
            "type": "tool_call", "title": "运行 cargo test", "status": "completed",
        }));
        assert_eq!(kind, "tool_call");
        assert!(text.contains("运行 cargo test"), "{text}");

        let (_, text) = update_summary(&json!({
            "type": "permission", "title": "写入 out.md", "decision": null,
        }));
        assert!(text.contains("权限请求"), "{text}");

        let (_, text) = update_summary(&json!({
            "type": "status", "status": "succeeded", "detail": null,
        }));
        assert!(text.contains("succeeded"), "{text}");
    }

    /// 文件引用：`path#anchor` 要拆成路径与锚点，不能把锚点丢掉。
    #[test]
    fn file_references_keep_their_anchor() {
        let refs = file_refs(&[
            "docs/spec.md#验收标准".to_owned(),
            "  docs/plan.md  ".to_owned(),
            "README.md#".to_owned(),
            "   ".to_owned(),
            "#只有锚点".to_owned(),
        ]);
        assert_eq!(refs.len(), 3, "{refs:?}");
        assert_eq!(refs[0]["path"], "docs/spec.md", "{refs:?}");
        assert_eq!(refs[0]["anchor"], "验收标准", "{refs:?}");
        assert_eq!(refs[1]["path"], "docs/plan.md", "{refs:?}");
        assert_eq!(refs[1]["anchor"], Value::Null, "{refs:?}");
        // 只有锚点没有路径的，丢掉。
        assert_eq!(refs[2]["path"], "README.md", "{refs:?}");
        assert_eq!(refs[2]["anchor"], Value::Null, "{refs:?}");
    }
}
