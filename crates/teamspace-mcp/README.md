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
| `list_threads` | `team`（可选）、`role`（可选）、`task`（可选） | 岗位通信的线程：主题、参与岗位、关联任务、最近更新时间 |
| `get_thread` | `thread` | 线程详情：往来消息、每条消息的状态、回复正文与失败原因 |
| `inbox` | `team`（可选） | 待我处理（只读）：待验收、执行失败或受阻、待答复权限、需要拍板的消息、Leader 越权记录 |

### 操作

| 工具 | 参数 | 说明 |
|---|---|---|
| `create_task` | `title`、`role`、`team`（可选）、`description`、`status`（`todo`/`ready`）、`depends_on`、`file_refs` | 创建任务；`file_refs` 可写 `路径#锚点` |
| `update_task` | `task`、`title`/`description`/`role`/`depends_on`/`file_refs`（均可选） | 编辑任务；没给的字段保持原样；`file_refs` 可写 `路径#锚点`，锚点不会被丢掉 |
| `set_dependencies` | `task`、`depends_on` | 设置依赖（空数组表示清空）；成环会被拒绝 |
| `change_task_status` | `task`、`to`、`reason`（受阻时必填） | 变更状态：`todo`/`ready`/`doing`/`blocked`/`review`/`cancelled` |
| `start_run` | `task` | 开始执行任务 |
| `cancel_run` | `run` 或 `task` | 取消正在进行的执行 |
| `reject_review` | `task` 或 `run`、`comment`（必填） | 退回验收，续接会话重做 |
| `send_message` | `to`、`body`、`task`（可选）、`subject`（可选）、`team`（可选） | 给岗位发消息；返回消息 id、线程 id 与当前状态 |
| `wait_reply` | `message`、`timeout_seconds`（可选，默认 60、最多 600）、`thread`（可选）、`team`（可选） | 等一条消息的回复：轮询到「已回复 / 失败 / 过期」为止 |

### 给岗位发消息

`send_message` 的 `to` 可以写岗位名称、岗位标识（`role_frontend`）、任务编号前缀（`FE`）或岗位 id；
`task` 可以写任务编号（`FE-002`）或标题，也可以不给。消息进入「排队」，由接收岗位绑定的 Harness 接手，
回复写回同一个线程。

`wait_reply` 拿到回复：轮询间隔不超过 2 秒，等待期间不阻塞别的工具调用。等不到时返回当前状态与
`timedOut: true`（不是错误——消息还在路上，可以稍后再等）。

失败与过期都会保留原消息、失败原因与时间；`get_thread` 能看到这些。

### 怎么指代团队、岗位和任务

Leader 可以用名称或编号指代，服务端负责解析成 id（先调用查看命令再解析）：

- 团队：名称或 id；只有一个团队时可省略。
- 岗位：名称、岗位标识（`role_frontend`）、任务编号前缀（`FE`）或 id。
- 任务：编号（`FE-001`）、标题或 id。

已取消的任务同样能按编号或标题查到（取消可以撤销，Leader 能把它改回 `todo`）。
指明了团队时，任务必须属于该团队：拿别的团队的任务 id 来会被挡下并说明是哪个团队。

解析不到或有歧义时，工具会返回中文说明并列出候选（最多 10 条，并给出候选总数），让 Leader 补一句就能继续。

## 限制

权限边界在桌面应用的内核里把守（D7、D8、D26、D32），这套工具不提供、也不绕过下面这些操作：

- **通过验收并合并到主分支**：只能在 Teamspace 应用里由你点。
- **答复 Agent 的权限请求**：`pending_permissions` 只做查询与提醒，答复仍在应用里点。
- **记录决策与改通知订阅**：`inbox` 只做查询与提醒，两件事都是你的控制权（D32）。
- **重发失败或过期的消息**：不在 Leader 白名单里。
- **换绑 / 新建 / 编辑 Harness、岗位与团队**：同样只能由你在应用里做。
- **把任务置为已完成**：走验收流程，不能用 `change_task_status` 直接置为 `done`。
- 应用没运行时，所有工具都返回「Teamspace 应用没有运行，请先打开 Teamspace」。
- 应用卡住不回应时，工具调用会在 30 秒后返回「Teamspace 应用响应超时，可能正在忙或已卡住，请稍后再试」，不会一直挂着。

越权请求会被内核拒绝，并在活动页里留一条操作者为 Leader 的记录。

## 开发

```bash
cargo build -p teamspace-mcp            # 构建
cargo test  -p teamspace-mcp            # 单元测试 + 集成测试
cargo clippy -p teamspace-mcp --all-targets
```

集成测试（`tests/mcp_tools.rs`、`tests/mcp_messages.rs`）会真的起一个 Teamspace：临时数据库 + 临时目录里的控制 socket +
内置 Mock Agent（`examples/mock_agent.rs`），再以 stdio 启动 `teamspace-mcp` 二进制，按 MCP
协议调用工具，不调用任何真实模型、不消耗额度。消息相关的用例用 Mock 的 `[mock:reply=…]` 与
`[mock:replyfail]` 标记模拟回复与失败。
