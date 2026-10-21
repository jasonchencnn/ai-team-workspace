# teamspace-mcp

Teamspace 的 MCP stdio 服务：给 Leader（Claude Code / Codex 等外部会话）一套工具，用来操作
正在运行的 Teamspace 桌面应用。

它**只做转发**：每个工具调用都通过本机控制 socket（`teamspace-control.sock`）发给桌面应用，
由应用里的内核执行（D26）。它不打开数据库、不启动 Agent、不绕过任何权限边界。

## 前提

- **Teamspace 应用要先打开**。没打开时工具会返回「Teamspace 应用没有运行，请先打开 Teamspace」，
  服务本身不会崩。
- 控制 socket 默认在 macOS 应用数据目录里：
  `~/Library/Application Support/com.ai-team-workspace.desktop/teamspace-control.sock`
  （仅当前用户可读写，权限 0600）。可用环境变量 `TEAMSPACE_CONTROL_SOCKET` 覆盖路径（测试用）。

## 在 Claude Code 中接入

先构建二进制：

```bash
cargo build -p teamspace-mcp --release
# 产物：target/release/teamspace-mcp
```

再在 Claude Code 里登记（`claude mcp add <名字> -- <命令> [参数…]`）：

```bash
claude mcp add teamspace -- /绝对路径/teamspace-mcp
```

例如：

```bash
claude mcp add teamspace -- "$HOME/Documents/ai-team-workspace/target/release/teamspace-mcp"
```

登记后在 Claude Code 里用 `/mcp` 确认 `teamspace` 已连接；也可以把上面这条命令写进
工作空间的 `.mcp.json`（M4-03 会在应用里生成可直接复制的片段）。

Codex 等支持 stdio MCP 的客户端同理：在其 MCP 配置里登记同一个可执行文件即可。

## 工具清单

工具名是英文 snake_case，描述与参数说明是中文；结果是一段便于阅读的 JSON。

### 查看

| 工具 | 参数 | 说明 |
|---|---|---|
| `list_teams` | — | 列出所有团队（id、名称、工作空间目录） |
| `team_overview` | `team`（可选） | 团队概览：工作空间、岗位卡片、各状态任务数、进行中的任务、最近活动 |
| `list_roles` | `team`（可选） | 岗位：名称、岗位标识、任务编号前缀、绑定的 Harness、任务统计 |
| `list_harnesses` | `team`（可选） | Harness 实例与最近的握手状态 |
| `task_board` | `team`（可选）、`include_cancelled`（可选） | 看板：按状态分列的任务 |
| `get_task` | `task`、`team`（可选） | 任务详情：说明、负责岗位、依赖与下游、可执行的状态流转、最近执行、活动 |
| `run_status` | `task` 或 `run`（二选一）、`team`（可选） | 最近一次执行的状态、摘要、改动文件、失败原因 |
| `run_progress` | `run` 或 `task`、`cursor`（可选）、`limit`（可选） | 执行进度：消息、计划、工具调用、权限请求、状态变化；用 `nextCursor` 续读 |
| `list_activity` | `team`/`task`/`role`（均可选）、`cursor`、`limit` | 活动与审计记录，可按任务或岗位筛选 |
| `get_review` | `task`、`team`（可选） | 待验收视图：执行摘要、改动文件、可否通过/退回、历史退回意见 |
| `pending_permissions` | `team`（可选） | 正在执行、等待答复的权限请求（只读） |

### 操作

| 工具 | 参数 | 说明 |
|---|---|---|
| `create_task` | `title`、`role`、`team`（可选）、`description`、`status`（`todo`/`ready`）、`depends_on`、`file_refs` | 创建任务 |
| `update_task` | `task`、`title`/`description`/`role`/`depends_on`/`file_refs`（均可选） | 编辑任务；没给的字段保持原样 |
| `set_dependencies` | `task`、`depends_on` | 设置依赖（空数组表示清空）；成环会被拒绝 |
| `change_task_status` | `task`、`to`、`reason`（受阻时必填） | 变更状态：`todo`/`ready`/`doing`/`blocked`/`review`/`cancelled` |
| `start_run` | `task` | 开始执行任务 |
| `cancel_run` | `run` 或 `task` | 取消正在进行的执行 |
| `reject_review` | `task` 或 `run`、`comment`（必填） | 退回验收，续接会话重做 |

### 怎么指代团队、岗位和任务

Leader 可以用名称或编号指代，服务端负责解析成 id（先调用查看命令再解析）：

- 团队：名称或 id；只有一个团队时可省略。
- 岗位：名称、岗位标识（`role_frontend`）、任务编号前缀（`FE`）或 id。
- 任务：编号（`FE-001`）、标题或 id。

解析不到或有歧义时，工具会返回中文说明并列出候选，让 Leader 补一句就能继续。

## 限制

权限边界在桌面应用的内核里把守（D7、D8、D26），这套工具不提供、也不绕过下面这些操作：

- **通过验收并合并到主分支**：只能在 Teamspace 应用里由你点。
- **答复 Agent 的权限请求**：`pending_permissions` 只做查询与提醒，答复仍在应用里点。
- **换绑 / 新建 / 编辑 Harness、岗位与团队**：同样只能由你在应用里做。
- **把任务置为已完成**：走验收流程，不能用 `change_task_status` 直接置为 `done`。
- 应用没运行时，所有工具都返回「Teamspace 应用没有运行，请先打开 Teamspace」。

越权请求会被内核拒绝，并在活动页里留一条操作者为 Leader 的记录。

## 开发

```bash
cargo build -p teamspace-mcp            # 构建
cargo test  -p teamspace-mcp            # 单元测试 + 集成测试
cargo clippy -p teamspace-mcp --all-targets
```

集成测试（`tests/mcp_tools.rs`）会真的起一个 Teamspace：临时数据库 + 临时目录里的控制 socket +
内置 Mock Agent（`examples/mock_agent.rs`），再以 stdio 启动 `teamspace-mcp` 二进制，按 MCP
协议调用工具，不调用任何真实模型、不消耗额度。
