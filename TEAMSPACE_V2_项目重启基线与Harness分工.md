# AI Teamspace V2｜项目重启基线与 Harness 分工

> 状态：项目重启基线
> 目的：在原开发会话丢失后，为 Codex、Antigravity、MiniMaxCode 重新建立统一项目认知、目标架构、协议边界、开发分工与人工协作规则。
> 原则：先重新扫描现有项目，再基于本文件进行 V2 架构改造；不要假设现有代码已经完整实现旧 PRD，也不要直接推翻已有可复用模块。

# 1. 项目一句话重新定义

AI Teamspace 不是单纯的“多 Agent 任务面板”，也不是多个 Coding CLI 的启动器。

它是：

**一个以 Human 为最终 Owner、以 Leader Harness 为主要智能决策者、以 Teamspace 为团队控制平面，通过 ACP / Adapter 调度多个专业 Harness，并通过统一 Workspace、Task、Conversation、Event 保存团队事实的 AI 开发团队协作系统。**

日常使用体验应尽量接近：

```
Human
  ↓
Leader Harness
  ↓
Teamspace
  ├── Frontend Harness
  ├── Backend Harness
  ├── QA Harness
  ├── Design / Browser / Figma Tools
  └── Future Harnesses
```

Human 主要与 Leader 交互。

Leader 通过 Teamspace 了解整个团队状态、创建任务、调派其他 Harness、查看执行结果、发起 Review 和组织项目推进。

# 2. Teamspace 的核心定位

Teamspace 有两个不同层面的定位，必须同时成立。

## 2.1 产品体验定位

从用户体验上：

**Teamspace 是 Leader Harness 的“外挂团队操作系统”。**

Human 可以主要待在 Codex 等 Leader Harness 中工作。

例如 Human 对 Leader 说：

> 把登录模块拆成前端和后端两个任务，让 MiniMax 做后端，让 Antigravity 做前端，完成后进行浏览器验收。

Leader 应可以直接操作 Teamspace：

```
创建任务
→ 设置负责人 Role
→ 查看 Harness 状态
→ 分派任务
→ 给目标 Role 发消息
→ 查询执行状态
→ 查看结果
→ 发起 Review
```

Human 不应该必须切换 Teamspace GUI 才能完成这些操作。

## 2.2 系统架构定位

从系统架构上：

**Teamspace 必须独立于任何具体 Harness。**

不能设计成：

```
Codex
└── Teamspace
```

而应该是：

```
Human
  │
  ▼
Teamspace
  │
  ├── Role: Leader
  │      └── Binding → Codex
  │
  ├── Role: Frontend
  │      └── Binding → Antigravity
  │
  └── Role: Developer
         └── Binding → MiniMaxCode
```

今天 Leader 可以是 Codex。

以后也可以替换为其他 Harness。

Leader 更换后：

- Team 不变
- Role 不变
- Task 不变
- Conversation 不变
- Workspace 不变
- 历史记录不变

只改变 Binding。

# 3. V2 核心架构

目标架构：

```
                         Human
                           │
                           ▼
                ┌────────────────────┐
                │     Teamspace      │
                │   Control Plane    │
                └─────────┬──────────┘
                          │
       ┌──────────────────┼──────────────────┐
       │                  │                  │
       ▼                  ▼                  ▼
     Tasks           Conversations       Workspace
       │                  │                  │
       └──────────┬───────┴───────┬──────────┘
                  │               │
             Permission        Event/Audit
                  │
                  ▼
            Harness Gateway
                  │
          ┌───────┴────────┐
          │                │
      ACP Provider    Adapter Provider
          │                │
          ▼                ▼
       Harness          Harness
```

Harness 内部又可以通过 MCP 等工具协议调用：

```
Harness
  │
  ├── Teamspace MCP
  ├── Figma MCP
  ├── GitHub MCP
  ├── Browser
  └── 其他 Tools
```

# 4. 协议体系重新定义

V2 不再把所有东西统称为“Adapter”。

应该明确区分三层。

## Layer 1：Team Protocol

Team Protocol 负责：

```
Team
Role
Binding
Task
Conversation
Message
Workspace
Artifact
Run
Permission
Event
Review
```

它回答：

> 谁负责什么？

> 谁可以做什么？

> 工作做到哪里了？

> 谁需要与谁沟通？

> 项目的事实和结果在哪里？

Team Protocol 不关心下面使用 Codex、MiniMax 还是其他 Harness。

## Layer 2：Harness Gateway

负责：

**Teamspace ↔ Harness**

统一 Harness Interface 应至少抽象：

```
create_session()
resume_session()

send_prompt()
send_context()

stream_events()
get_status()

cancel()
interrupt()

request_permission()

close_session()
```

Harness Gateway 有两条实现路径。

### A. ACP Provider

优先用于支持 ACP 或存在成熟 ACP Bridge 的 Harness。

```
Teamspace
   ↓
ACP
   ↓
Harness
```

ACP 是标准路径。

### B. Adapter Provider

对于无法直接使用 ACP 的 Harness：

```
Teamspace
   ↓
Unified Harness Interface
   ↓
Custom Adapter
   ↓
CLI / SDK / API / App Server
```

Adapter 是兼容层，不是第二套 Team Protocol。

核心要求：

> 上层业务不能知道 Harness 底层到底使用 ACP、CLI、SDK 还是私有 API。

# 5. MCP 的新定位

ACP 和 MCP 不承担相同职责。

建议明确：

```
ACP / Adapter
= Teamspace 如何控制 Harness

MCP / Tool Connector
= Harness 如何操作 Teamspace 或其他工具
```

因此非常重要的一项 V2 能力是：

# Teamspace MCP Server

Teamspace 应向获得授权的 Harness 暴露工具能力。

例如：

```
get_team()

list_roles()
get_role()

list_harnesses()
get_harness_status()

create_task()
update_task()
assign_task()
cancel_task()

list_tasks()
get_task()
get_task_status()

send_message()
reply_message()
list_conversations()

read_workspace_reference()
publish_artifact()

request_review()
complete_review()
```

Leader Harness 是 Teamspace MCP 的主要消费者。

# 6. Conversation 成为一级核心对象

旧架构主要关注：

```
Task
Event
Message
Thread
```

V2 应增加正式的：

# Conversation

Conversation 是 Teamspace 保存的团队通信记录。

Conversation 不是某个 Harness 的 Session。

必须明确：

```
Conversation ID
≠ ACP Session ID
≠ Codex Thread ID
≠ MiniMax Session ID
```

建议关系：

```
Conversation
│
├── Message
│
├── Message
│
├── DeliveryAttempt
│
├── Event
│
├── Run
│
└── ProviderSession
     ├── provider
     ├── protocol
     ├── external_session_id
     └── external_thread_id
```

Provider Session 只是 Conversation 下的一次具体 Harness 通讯实现。

# 7. Teamspace 必须保存完整会话

不能依赖 Codex、MiniMax、Antigravity 等 Harness 自己的 GUI 保存团队历史。

原因包括：

- 外部创建的 Session 不一定出现在 Harness GUI。
- 不同 Harness 的历史机制不同。
- 更换 Harness 后原 Session 不一定可以继续。
- Harness GUI 不属于 Teamspace 的事实来源。
- 团队审计记录不能依赖某家 Harness 的产品行为。

所以：

**平台必须拥有自己的 Conversation View。**

例如：

```
Frontend / Login Task
────────────────────────────

Leader · Codex
请根据 TASK-102 完成 Login UI。

System
消息已持久化。
已路由 Frontend Role。
当前 Binding: Antigravity.

Antigravity
我已经读取任务文件……

▸ Read specification
▸ Modify component
▸ Browser test

Antigravity
页面完成，已经提交结果。

System
Artifact published.
Task moved to REVIEW.
```

# 8. Harness 与 Harness 通讯规则

V2 允许 Harness 与 Harness 协作。

但是：

# 禁止把直接点对点通信作为正式团队通讯方式

不采用：

```
Codex ─────────→ MiniMax
```

统一采用：

```
Harness A
   ↓
Teamspace
   ↓
持久化 Message
   ↓
Conversation
   ↓
Router
   ↓
Harness B
```

回复同样：

```
Harness B
   ↓
Teamspace
   ↓
持久化
   ↓
Harness A
```

也就是：

# Harness A → Teamspace → Harness B

而不是 A → B。

# 9. 为什么必须通过 Teamspace 中转

这样可以保证：

- 通讯有完整记录。
- 可以关联 Task。
- 可以关联 Artifact。
- 可以关联 Role。
- 可以知道消息发生时的 Binding。
- Harness 换绑后历史不丢失。
- Harness 离线可以排队。
- 可以重试。
- 可以人工接管。
- 可以权限审计。
- Leader 可以查看整个团队通讯。
- Human 可以回放项目历史。

Teamspace 必须成为团队唯一可信的通讯总线。

# 10. 通讯保持“非即时”模型

目前不追求实时聊天系统。

推荐保留异步消息机制：

```
DRAFT
  ↓
QUEUED
  ↓
ROUTED
  ↓
DELIVERED
  ↓
ACKNOWLEDGED
  ↓
PROCESSING
  ↓
REPLIED
```

异常：

```
RETRYING
FAILED
EXPIRED
NEEDS_HUMAN
```

因此 Teamspace 更接近：

> AI 团队内部邮件 + Task 协作 + Agent Session

而不是 Discord / Slack 实时聊天。

# 11. Role 仍然是稳定团队身份

继续严格坚持：

```
Role ≠ Harness
Role ≠ Model
Role ≠ Session
```

例如：

```
Role:
Frontend

Binding V1:
Antigravity

Binding V2:
MiniMaxCode
```

Frontend 的：

- Task
- Conversation
- Artifact
- 权限
- 历史
- Workspace Context

全部继续存在。

只切换执行 Harness。

# 12. Leader 的定位升级

旧 Leader 更接近：

> 高权限任务创建者。

V2 Leader 应升级为：

# Team Orchestrator

Leader Harness 应能够：

- 查看团队结构。
- 查看所有 Role。
- 查看可用 Harness。
- 创建 Task。
- 修改 Task。
- 分配任务。
- 设置依赖。
- 创建 Review。
- 查看任务状态。
- 查看 Run 状态。
- 查看阻塞。
- 向其他 Role 发送消息。
- 查看 Conversation。
- 读取 Workspace。
- 创建计划。
- 创建或修改普通团队成员。
- 在 Human 授权范围内调整 Binding。
- 调度其他 Harness 工作。

# 13. Leader 不是 Root

Human 永远拥有最高权限。

建议权限边界：

## Leader 默认允许

```
创建 Task
分配 Task
修改 Task
取消普通 Task

创建普通 Role
修改普通 Role 职责

查询 Harness
调用 Human 已批准 Harness Pool

发送 Team Message
查看 Conversation

设置任务依赖
发起 Review
完成普通 Review

读取团队状态
读取 Workspace
发布计划与决策
```

## 可以由 Human 额外授权

```
普通 Role 的 Harness 换绑

从已批准 Harness Pool
新增团队成员

修改部分并发策略

重新分配失败任务
```

## 必须 Human 控制

```
给 Leader 自己扩权

修改 Team 权限上限

修改安全策略

添加 API Key / 登录凭据

访问秘密信息

安装未经批准 Harness

删除整个 Team

高风险外部操作

生产部署

付款

访问生产数据
```

原则：

> Leader 可以管理团队，但不能突破 Human 制定的 Team Security Boundary。

# 14. Teamspace 的核心事实来源

V2 明确区分四类事实。

## Workspace owns Knowledge

保存：

```
PRD
Architecture
Specification
Plan
Decision
Long-form Task Description
Review Report
Result Report
```

## Task owns Work State

保存：

```
谁负责
当前状态
优先级
依赖
Run
结果引用
```

## Conversation owns Communication

保存：

```
谁向谁说了什么
为什么发消息
对应哪个 Task
回复关系
投递状态
```

## Event owns Audit

保存：

```
谁
什么时间
对哪个对象
进行了什么操作
结果是什么
```

# 15. V2 核心对象

现有对象继续保留：

```
Team
Role
Binding
HarnessType
HarnessInstance
Workspace
Task
Run
Artifact
Event
PermissionPolicy
Message
```

新增或提升：

```
Conversation
ProviderSession
DeliveryAttempt
HarnessConnection
ToolConnection
```

关系建议：

```
Team
├── Roles
│    └── Binding
│          └── HarnessInstance
│
├── Tasks
│    └── Runs
│          └── ProviderSession
│
├── Conversations
│    ├── Messages
│    └── DeliveryAttempts
│
├── Workspace
│    └── Artifacts
│
└── Events
```

# 16. 当前开发资源

当前人工开发阶段使用三个主要 Harness：

```
Codex
Antigravity
MiniMaxCode
```

目前 Human 人工承担 Teamspace Router 的角色。

未来 Teamspace 自动完成的事情，现在先人工执行。

# 17. Harness 岗位分工

## 17.1 Codex

岗位：

# Leader / Architect / Reviewer / Integration Owner

主要负责：

- 重新扫描整个项目。
- 理解现有代码。
- 对照旧 PRD 和 V2 基线。
- 识别已有实现与缺口。
- 冻结 Architecture V2。
- 定义 Team Protocol V2。
- 定义 Harness Interface。
- 定义 ACP Provider 边界。
- 定义 Adapter Provider 边界。
- 定义 Teamspace MCP Tool Schema。
- 设计权限模型。
- 设计 Conversation Model。
- 拆解开发任务。
- Review MiniMaxCode 的后端实现。
- Review Antigravity 的前端实现。
- 解决跨模块接口冲突。
- 最终集成验收。

Codex 原则：

> 少承担大批量机械代码，主要把高推理能力用于架构、拆解、接口、Review 和集成。

# 18. MiniMaxCode

岗位：

# Main Developer / Backend & Infrastructure

主要负责大量实现工作。

优先负责：

```
Domain Model
Database / Migration

Conversation Store

Message Router
Delivery Engine

Harness Gateway

ACP Provider Implementation

Adapter Skeleton

ProviderSession Mapping

Teamspace MCP Server

Task Service

Run Service

Permission Enforcement

Event / Audit

API

Backend Unit Tests

Integration Tests

Refactoring
```

原则：

> Codex 定义边界和验收，MiniMaxCode 承担大量工程实现。

# 19. Antigravity

岗位：

# Frontend / UX / Browser QA

主要负责：

```
Teamspace Frontend

Conversation Window

Task Board

Task Detail

Role / Binding UI

Harness Management UI

Leader Control UI

Workspace Browser

Run Detail

Activity / Audit UI

Permission / Approval UI

Browser E2E

Responsive UI

UX Regression
```

同时利用其浏览器能力进行实际 UI 验收。

原则：

> Antigravity 重点负责“用户实际看到并操作的 Teamspace”。

# 20. 当前团队结构

当前人工开发阶段建议固定：

```
Human
│
├── Owner
│
└── Temporary Router
│
▼
Codex
Leader / Architect
│
├───────────────┐
│               │
▼               ▼
MiniMaxCode   Antigravity
Developer     Frontend / QA
```

# 21. 人工 Teamspace 工作方式

正式 Teamspace Router 尚未开发完成之前：

Human 暂时只承担：

# Message Router

不要再承担：

> 人工重新解释任务。

不要将一个 Harness 的大段回答复制粘贴给另一个 Harness。

所有正式任务必须落到 Markdown 文件。

# 22. 临时 Markdown 驱动协议

建议在当前协作目录中逐步形成：

```
协作工作/
│
├── TEAMSPACE_V2_项目重启基线与Harness分工.md
│
├── STATUS.md
│
├── decisions/
│
├── tasks/
│
├── messages/
│
├── reviews/
│
└── results/
```

当前阶段不要求立刻实现完整自动化。

# 23. Task 文件原则

Leader 创建任务时生成：

```
tasks/TASK-001.md
```

内容至少包含：

```
Task ID
Title

Owner Role

Recommended Harness

Objective

Background

Relevant Files

Scope

Out of Scope

Dependencies

Acceptance Criteria

Expected Output
```

Human 转发给目标 Harness 时，不复制任务正文。

只说：

> 请读取 `tasks/TASK-001.md`，按任务要求执行。

# 24. Result 文件原则

Harness 完成工作后写：

```
results/TASK-001-result.md
```

至少包含：

```
Task ID

Status

Summary

Files Changed

Implementation Notes

Tests

Known Issues

Questions

Recommended Next Step
```

然后 Human 只告诉 Leader：

> TASK-001 已返回，请读取对应 Result。

# 25. 人工 Harness-to-Harness 通讯

未来：

```
Harness A
→ Teamspace
→ Harness B
```

当前临时实现：

```
Harness A
→ messages/THREAD-xxx.md
→ Human Router
→ Harness B
```

Human 只负责通知：

> THREAD-003 有新消息，请读取。

不要人工重新组织内容。

# 26. Human 当前最重要的工作纪律

Human 不应该成为：

```
Prompt Engineer
Context Rewriter
Task Explainer
Result Summarizer
```

Human 当前应该只是：

```
Router
Approver
Owner
Decision Maker
```

这样当前人工流程才真正模拟未来 Teamspace。

# 27. 项目重新启动时 Codex 第一阶段任务

Codex 接手项目后不要直接大规模修改代码。

首先执行：

# PROJECT RESCAN

需要扫描：

```
项目目录

package / dependency

docs

architecture

database

backend

frontend

adapter

task

message/thread

permission

event

tests

existing mock
```

然后将现有实现映射为：

```
Implemented

Partially Implemented

Designed but Not Implemented

Obsolete

Conflicting With V2

Unknown
```

# 28. Rescan 必须重点检查

## Harness Layer

检查当前是否已有：

```
Adapter Registry
Harness Instance
Health Check
Session
CLI Integration
ACP
MCP
```

## Communication Layer

检查：

```
Message
Thread
Delivery
Queue
Retry
Notification
Event
```

判断能否升级成 Conversation。

## Leader

检查目前 Leader 到底只能：

```
Task Create / Assign
```

还是已经拥有其他控制面能力。

## UI

检查是否已有：

```
Task UI
Role UI
Harness UI
Message UI
Activity UI
Workspace UI
```

并判断哪些可复用。

# 29. Codex Rescan 的第一份输出

建议生成：

```
PROJECT_RESCAN_REPORT.md
```

内容包括：

```
1. 当前项目实际结构

2. 当前技术栈

3. 已实现模块

4. 未完成模块

5. 与旧 PRD 的差异

6. 与 V2 架构冲突的部分

7. 可直接复用部分

8. 建议删除/重构部分

9. 数据模型变化

10. Migration 风险

11. V2 开发顺序

12. 可立即派给 MiniMax 的任务

13. 可立即派给 Antigravity 的任务
```

# 30. V2 推荐开发顺序

## Phase R0 — Project Rescan

Owner：

```
Codex
```

目标：

重新理解项目。

## Phase R1 — Architecture V2 Freeze

Owner：

```
Codex
```

冻结：

```
Core Objects

Team Protocol

Conversation Model

Harness Interface

ACP / Adapter Boundary

MCP Tool Schema

Permission Matrix
```

冻结后再开始大规模并行开发。

## Phase R2 — Harness Gateway

Architecture：

```
Codex
```

Implementation：

```
MiniMaxCode
```

目标：

```
Unified Harness Interface
ACP Provider
Adapter Provider
Provider Session
Health / Capability
Session Lifecycle
```

## Phase R3 — Conversation System

Architecture：

```
Codex
```

Backend：

```
MiniMaxCode
```

Frontend：

```
Antigravity
```

目标：

```
Conversation
Message
Delivery
Routing
Retry
Audit
Conversation UI
```

## Phase R4 — Leader Control Plane

Architecture / Permissions：

```
Codex
```

Backend：

```
MiniMaxCode
```

UI：

```
Antigravity
```

目标：

```
Teamspace MCP

Leader Task Control

Leader Role Control

Leader Message

Leader Status Query

Delegated Admin
```

## Phase R5 — Teamspace UI V2

Owner：

```
Antigravity
```

重点：

```
Conversation Window

Leader Team View

Harness Status

Role / Binding

Task Dashboard

Run / Session View

Activity / Audit
```

MiniMax 提供 API。

Codex Review 产品和架构一致性。

## Phase R6 — Integration

Owner：

```
Codex
```

QA：

```
Antigravity
```

Bug Fix：

```
MiniMaxCode
```

目标：

完成真实：

```
Human
→ Leader
→ Teamspace
→ Worker Harness
→ Teamspace
→ Leader
```

闭环。

# 31. 第一阶段建议任务分配

## CODEX-001

```
Project Rescan
```

## CODEX-002

```
Architecture V2 Gap Analysis
```

## CODEX-003

```
Unified Harness Interface Specification
```

## CODEX-004

```
Conversation Data Model
```

## CODEX-005

```
Leader Permission Matrix
```

## CODEX-006

```
Teamspace MCP Tool Schema
```

完成这些之后，再正式派发 MiniMax / Antigravity 开发任务。

# 32. MiniMax 第一批候选任务

在 Codex 完成扫描后，应优先考虑：

```
MM-001
Conversation persistence

MM-002
ProviderSession model

MM-003
Harness Gateway skeleton

MM-004
ACP Provider spike

MM-005
Message Router

MM-006
Teamspace MCP server skeleton
```

实际编号和范围必须以 Rescan 后 Codex 创建的 Task 文件为准。

# 33. Antigravity 第一批候选任务

在 Codex 完成扫描后，应优先考虑：

```
AG-001
Conversation UI prototype against real domain model

AG-002
Role / Harness / Binding UI audit

AG-003
Leader Control panel

AG-004
Task Dashboard V2

AG-005
Run / ProviderSession detail view

AG-006
Browser E2E baseline
```

同样必须等 Rescan 后确认现有代码可复用情况。

# 34. 开发过程中禁止事项

## 禁止 Harness 私下形成团队事实

任何重要结果必须返回：

```
Workspace
Task
Conversation
Artifact
Event
```

## 禁止把 ACP 业务化

ACP 只负责 Harness 通讯。

不能在 ACP 中硬编码：

```
frontend_task

backend_task

leader_review
```

这些属于 Team Protocol。

## 禁止 Team Protocol 感知具体 Harness

不能出现：

```
assign_to_codex()

send_to_minimax()
```

只能：

```
assign_task(role_id)

send_message(role_id)
```

Binding 决定最终 Harness。

## 禁止将 Harness Session 当项目记忆

Session 是执行上下文。

Workspace / Task / Conversation 才是团队事实。

## 禁止依赖 Harness GUI 保存会话

Teamspace 必须保存自己完整的：

```
Conversation
Message
Event
ProviderSession Reference
```

## 禁止 Leader 自提权

所有 Leader 行为都受：

```
Team Boundary
∩ Role Permission
∩ Task Scope
∩ Harness Capability
∩ Human Policy
```

限制。

# 35. V2 最重要的产品体验

最终 Human 应能够主要停留在 Leader Harness：

```
Human：
现在项目进展怎么样？

Leader：
Frontend 完成 80%。
Backend TASK-34 已完成。
QA 发现 2 个阻塞问题。

Human：
让 MiniMax 修 TASK-41，
修完让 Antigravity 做浏览器回归。

Leader：
调用 Teamspace：
→ 创建任务
→ 分配 Role
→ 发送 Message
```

接下来：

```
Teamspace
↓
ACP / Adapter
↓
MiniMax
```

MiniMax 完成：

```
MiniMax
↓
Teamspace
↓
结果 / Conversation / Artifact
↓
Leader
```

Human 不需要人工管理每个 Agent 的上下文。

这就是本项目最终需要解决的问题。

# 36. 最终架构原则

项目后续所有产品和技术决策必须尽量符合以下原则：

1. **Human owns Authority**
2. **Leader owns Orchestration**
3. **Harness owns Intelligence**
4. **Teamspace owns Coordination**
5. **Workspace owns Knowledge**
6. **Task owns Work State**
7. **Conversation owns Communication**
8. **Event owns Audit**
9. **Role owns Identity**
10. **Binding owns Harness Replacement**
11. **ACP / Adapter owns Harness Transport**
12. **MCP owns Tool Access**

最终形成：

```
Human
  ↓
Leader Harness
  ↕
Teamspace
  ↕
Professional Harnesses
  ↕
Tools
```

其中 Teamspace 始终是：

# AI Team Control Plane

而不是某个 Harness 的附属数据库。

# 37. Codex 重新进入项目后的首要指令

进入 Codex 后，优先执行以下工作：

> 阅读本文件及现有 PRD、开发基线和项目源码。
> 不要假设旧开发会话仍然有效。
> 以实际代码为准重新扫描项目。
> 先输出 PROJECT_RESCAN_REPORT，确认当前实现、可复用模块、与 Teamspace V2 的差异以及迁移风险。
> 在 Architecture V2、Conversation Model、Harness Gateway、Leader Permission 和 Teamspace MCP 边界冻结前，不进行无边界的大规模重构。
> 完成扫描后，由 Codex 作为 Leader 将实现任务分别拆给 MiniMaxCode 与 Antigravity，并以 Markdown Task 文件作为人工 Teamspace 的任务载体。

# 38. 当前项目重启结论

本次项目不是从零开始。

旧版本中以下思想继续保留：

```
Role 稳定
Harness 可替换
Binding 可版本化
Workspace 为事实来源
Task 为状态索引
Event 可审计
Human 拥有最终权限
消息通过 Teamspace 路由
```

V2 主要新增和升级：

```
ACP-first Harness Gateway
Adapter fallback

完整 Conversation

Provider Session

Teamspace 自持会话历史

Harness → Teamspace MCP

Leader Team Orchestration

Harness-to-Harness via Teamspace

人工 Markdown Teamspace 开发流程
```

从本文件开始，后续项目开发以：

**“Leader + Teamspace + 专业 Harness 团队”**

作为核心架构目标。