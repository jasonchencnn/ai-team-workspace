# PRD × Figma 设计审计报告（2026-09-21 第四轮复核版）

## 1. 审计结论

视觉方向和核心信息关系符合要求。本次更新后的 Figma 已从“关键路径演示版”提升为“核心 MVP 状态覆盖版”，Harness-to-Harness 非即时通信、通知策略和换绑期间待投递消息均已有可演示链路；但仍不能直接视为完整可实现交互方案通过。

当前设计已经覆盖 Role、Harness、Task、Workspace 的核心关系，以及 Team 初始化、Adapter 健康、任务控制、换绑、人工接管、阻塞决策、验收退回、失败重试、依赖冲突、权限审批、Run 对账、Activity Stream 和 Role-to-Role 通信等关键状态。

第四轮复核确认，PRD 已补充 Message/Thread 对象、状态、操作、幂等和 Binding 解析规则；Figma 当前已补充 Harness 注册与健康检查、默认通知类别、消息 `FAILED / EXPIRED` 终态，以及此前关键状态页的主操作闭环。当前没有发现阻断开发启动的视觉或原型问题；剩余问题统一登记为“开发中调整”，不再回改 Figma 或设计方案。

## 2. 审计范围

- PRD：`多-Harness-AI-开发团队协作平台-PRD.md` V1.0。
- 设计方案：`design/界面设计方案.md`。
- Figma 文件：`C1roCB6shYmzP6oEu5ZAs4`。
- Figma 节点索引：`design/figma-state.json`。
- 首次审计核对了 16 个画板；整改后 Figma 当前为 36 个画板。本轮核对了全部 36 个顶层画板、节点结构、129 条原型反应和新增页面截图，重点覆盖 Harness 注册与健康检查、消息投递终态，以及前几轮新增的状态变更通知、QA 回复 Leader、回复已送达、通知订阅与投递策略、换绑期间待投递消息、任务控制和异常处理画板。

## 3. 已覆盖内容

- Role 与 Harness 身份区分，岗位身份在换绑后保持稳定。
- 团队概览、岗位席位、当前任务和基础在线状态。
- 任务看板、负责人、Harness、文件引用、依赖和阻塞原因的基础展示。
- 任务详情中的需求引用、结果、执行记录、负责人和权限摘要。
- 文件目录、Markdown 预览、任务关联和版本记录。
- Harness 换绑的“完成后换绑／取消并重新执行／人工接管”设计意图。
- Human 人工接管时停止执行、冻结后续写入和保留已有产物的风险说明。
- 产品决策、阻塞恢复和“解除阻塞不等于任务完成”的表达。
- 空任务状态、创建任务摘要和创建成功反馈。
- 基础视觉规范、桌面布局和一个待我处理窄屏概览。
- 状态变更通知：`TASK_REVIEW_REQUESTED`、`BLOCKED` 和高风险审批按订阅/权限策略生成岗位消息，并按当前 Binding 投递。
- QA `ANSWER` 回复：关联 `THREAD-001`、`reply_to`、Task、Reference 和 Workspace 决策文件，回复后展示 `MESSAGE_REPLIED → MESSAGE_QUEUED → MESSAGE_ROUTED → MESSAGE_DELIVERED`。
- 通信异常处理：未绑定时持久化为 `NEEDS_HUMAN`，支持绑定后重试、授权改派和查看消息审计。
- 通知订阅与投递策略：已展示 Role 接收范围、严重级别、去重窗口、最多 3 次重试、无 Binding 的 `MESSAGE_NEEDS_HUMAN` 和后续策略生效边界。
- 换绑期间待投递消息：已展示入队 Binding 快照、当前有效 Binding 解析、旧 Binding 拒收、重试、改派和 Human 处理。
- Harness 注册与健康检查：已展示 Adapter、运行位置、凭据安全引用、能力协商、版本兼容、失败诊断和保存后重新检查。
- 消息投递终态：已展示 `RETRYING` 瞬态、`FAILED`、`EXPIRED`、幂等键、TTL、重试、改派、新建后续消息和审计关联。

## 4. 缺失与不足

### 4.1 P0：影响 MVP 闭环

| 审计项 | 本次复核结论 | 仍需补强 | 对应 PRD |
|---|---|---|---|
| Team/Workspace 初始化 | 部分覆盖 | 已有创建 Team、选择 Git Workspace、建立 Role 的画板；仍缺 Role 编辑/停用、Team 编辑/归档及表单错误/加载状态 | FR-001～004、AC-01 |
| Harness 管理 | 已覆盖设计链路 | 已补注册与健康检查、Adapter 能力、版本兼容、凭据安全引用和失败诊断；真实凭据接入与运行时验证列为开发中调整 | FR-010～014 |
| Task 真实表单 | 已覆盖设计链路 | 已补“创建并分配”跳转；真实输入、选择器、错误、加载和持久化校验列为开发中调整 | FR-030～031 |
| Task 状态机 | 部分覆盖 | 新表单明确 `TODO` 不能领取、`READY` 可领取，任务控制展示 DOING/Run 状态和迁移；主看板仍用“待开始”聚合，非法迁移反馈未单独呈现 | PRD 9.1、AC-04 |
| Review 闭环 | 已覆盖设计链路 | 已有 Review、Run 成功、Artifact、评审意见、通过和退回；真实评论输入、结果持久化和权限校验列为开发中调整 | FR-035 |
| 失败与重试 | 已覆盖设计链路 | 已有错误分类、历史 Run、幂等重试、阻塞、人工接管和主操作跳转；超时、心跳丢失和 Harness 不可用的真实运行验证列为开发中调整 | FR-034、AC-07 |
| 依赖管理 | 已覆盖设计链路 | 已有依赖编辑、循环校验、软依赖、结果过期、集成任务和相关跳转；下游通知及保存失败反馈列为开发中调整 | FR-032、AC-06 |
| 并发与冲突 | 基本覆盖 | 已展示 Worktree、基线 Commit、fencing token、结果过期和集成任务；同一文件同时写入的显式冲突状态仍未单独画出 | FR-023、AC-05 |
| 权限审批 | 已覆盖设计链路 | 已有动作范围、有效期、权限计算、批准/拒绝、后续状态跳转和审计说明；真实权限计算与审批服务接入列为开发中调整 | FR-040～041、AC-09 |
| Activity Stream 与审计 | 基本覆盖 | 已有事件筛选、不可变关联链、事件详情、导出入口和同一 Thread 的 `MESSAGE_SENT → MESSAGE_ROUTED → MESSAGE_DELIVERED → MESSAGE_ACKNOWLEDGED → MESSAGE_REPLIED / MESSAGE_NEEDS_HUMAN` 事件链；仍需补更完整的事件筛选、分页和导出结果状态 | FR-042、AC-12 |
| 执行异常处理 | 基本覆盖 | 已新增失败、状态未知和消息投递异常画板；这些异常尚未明确接入“待我处理”主列表 | FR-043、PRD 17、22 |
| 重启恢复 | 已覆盖设计状态 | 已有 UNKNOWN、Adapter 对账、租约冻结、恢复监控、标记 FAILED、新 Run 和 Human 决定分支 | AC-10 |
| Role-to-Role 非即时通信 | 已覆盖主要设计链路 | 已有岗位线程、Role 路由、消息类型、Task/Reference、Binding 快照、状态变更自动通知、QA `ANSWER`、完整审计链、订阅策略、换绑期间待投递处理和消息终态；Human 通知接收模型列为开发前协议确认项 | PRD 6.12、14、17、18、AC-12 |
| 通知触发覆盖 | 已覆盖设计范围 | 已覆盖任务分配/验收/受阻/依赖满足、Run 失败/超时/离线、审批/换绑/冲突/结果过期等默认类别；真实订阅配置、去重和投递服务列为开发中调整 | FR-043、PRD 17.2 |
| 原型反应完整性 | 已通过结构检查 | 当前共检查到 129 条 reactions、32 个唯一目标；正式导航和 Hover 组件目标均可通过 `getNodeByIdAsync` 解析 | 设计方案原型范围 |
| 核心操作闭环 | 已通过原型检查 | Harness 注册、任务创建、Review、失败重试、依赖处理、审批、Run 对账、消息重试/改派和消息终态均已连接；真实业务动作列为开发中调整 | FR-010～014、FR-030～035、FR-040～052、AC-01～AC-11 |

### 4.2 P1：影响完整性与可落地性

| 审计项 | 本次复核结论 | 仍需补强 | 对应 PRD |
|---|---|---|---|
| 文件与 Artifact 详情 | 部分覆盖 | 新画板已展示 Commit、test report、document、Run 和基线；仍缺文件 Git 状态、Artifact 创建者/版本/位置的完整详情 | FR-022、FR-024 |
| 信息架构 | 部分覆盖 | 已有独立审计画板，但主导航仍未呈现 Harness 连接、活动、依赖、执行记录、设置等正式入口 | PRD 18 |
| Binding 历史 | 仍缺 | 已展示当前 Binding 和旧 Binding 失效语义，但岗位页没有完整 Binding 版本、生效/失效时间、绑定人历史 | FR-004、FR-012 |
| Human 控制 | 部分覆盖 | 已有取消、改派、换绑、接管和恢复分支；Team/Role 暂停恢复、重新打开和统一控制入口仍未完整呈现 | FR-050～052 |
| 通知设置 | 已覆盖设计范围 | 已覆盖订阅范围、事件类型、严重级别、去重窗口、重试上限、默认通知类别和无 Binding 处理；真实配置持久化列为开发中调整 | PRD 17 |
| 响应式闭环 | 仍缺 | 窄屏仍主要覆盖“待我处理”，任务详情、岗位通信和异常处理没有完整移动端闭环 | 设计方案 4.1、96 行 |
| 系统状态 | 部分覆盖 | 已补失败、未知 Run、依赖冲突、权限审批和消息投递失败；加载、断连、引用失效、通用并发更新冲突仍未逐一画出 | 设计方案 51～59、97 行 |
| 无障碍状态 | 未验证 | Figma 结构检查不能证明键盘聚焦、减少动态效果、错误朗读和实际控件可访问性 | 设计方案 53、59、103 行 |

## 5. 重点设计问题

### 5.1 “待开始”需要重新定义

PRD 将 `TODO` 定义为信息、负责人或前置条件尚未就绪，将 `READY` 定义为可以被负责人领取。当前 Figma 以“待开始”统一展示，建议明确以下方案之一：

1. 看板拆分为 `TODO` 与 `READY`；或
2. 保留“待开始”作为视觉聚合，但在卡片和筛选中显示明确子状态，并禁止 `TODO` 被 Harness 领取。

### 5.2 “待我处理”需要补充执行异常

本次已补充 Run 失败、Run 状态未知和消息投递异常画板，异常本身已有表达。但当前“待我处理”主页面仍未明确出现执行异常入口，建议将这些异常统一接入 Human 工作流，避免用户只能从任务详情或独立状态画板找到异常。

### 5.3 任务详情需要从“展示页”变成“控制页”

本次新增“FE-023 · 任务控制”画板，已经补充状态迁移、心跳、Run、Artifact、取消、阻塞、提交验收、改派、失败重试和活动审计入口。遗留问题是这些控制画板与原有任务详情、看板和“待我处理”之间的实际导航/状态同步规则还没有完全表达。

第四轮已确认上述状态页的主操作均已补齐 reaction，包括任务创建、验收、失败重试、依赖处理、审批、Run 对账、Harness 健康检查和消息投递终态。真实表单校验、权限计算、运行时状态同步和错误处理不再作为 Figma 阻断项，登记为开发中调整。

### 5.4 Role-to-Role 非即时通信的页面链路已基本闭环，状态定义仍需收口

新增“岗位通信”“发送岗位消息”和“消息需要处理”画板，已经表达 Harness 通过 Team Space 间接沟通的核心模型。Leader 不识别 QA 具体使用的 Harness，而是向稳定岗位发送消息：

```text
Leader → QA：请阅读 specs/login-acceptance.md，完成登录回归。
QA → Leader：请确认 XXXXX 的定义和验收边界。
```

Team Space 应根据结构化目标 Role 将消息转发到该 Role 的当前 Binding。当前 Figma 已展示目标 Role、当前 Binding、消息类型、Task/Reference、投递状态、路由记录和未绑定异常。Harness 只与 Team Space 的通用协议交互，不与其他 Harness 建立私有即时通信协议。

本轮已确认的页面闭环：

- `FE-023 · 任务控制` → `状态变更通知` → `岗位通信`。
- `岗位通信` → `发送岗位消息` / `消息需要处理` / `QA 回复 Leader`。
- `QA 回复 Leader` → `回复已送达` → `岗位通信` 或 `活动与审计`。
- 所有正式 `NAVIGATE` 目标均为当前 34 个画板中的有效节点；3 条 Hover 目标也可解析为有效组件变体。

本轮不再要求回改设计，以下事项转为开发中调整：

1. 通知策略、活动审计、Harness 连接、依赖和执行记录继续接入正式主导航或统一工作台入口。
2. 完成真实 Message/Notification/Event 服务接入，包括重试、改派、TTL、权限过滤、幂等和状态同步。
3. 修复消息终态页 3 处文字裁切，并在开发阶段通过真实窗口尺寸、响应式和无障碍验证。

这项能力和 Activity Stream 不等价：Activity Stream 记录系统事实，Role-to-Role Thread 承载需要被另一个岗位理解、处理和回复的协作内容；两者应能互相引用，但不能互相替代。

## 6. Role-to-Role 通信设计要求

### 6.1 通信模型

建议将通信抽象为“消息信封 + 持久化线程”，而不是直接把自然语言文本当作路由依据：

```yaml
message_id: MSG-001
thread_id: THREAD-001
from_role_id: role_leader
to_role_id: role_qa
type: COMMAND              # COMMAND / STATUS / QUESTION / ANSWER / HANDOFF
task_id: QA-006
reply_to: null
references:
  - specs/login-acceptance.md#regression
body: 请阅读引用章节，完成登录回归。
created_by: role_leader
binding_snapshot: binding_leader_v3
delivery_status: QUEUED
idempotency_key: ...
```

其中 `to_role_id` 必须是稳定 Role，而不是 Codex、Claude Code、MiniMax 等 Harness 名称。Team Space 在投递时解析目标 Role 的当前有效 Binding；换绑只影响后续投递，不改变历史线程和原消息归属。

### 6.2 建议的非即时状态

窗口应明确展示非即时通信的处理状态，避免让用户误以为对方正在实时在线：

```text
DRAFT → QUEUED → ROUTED → DELIVERED → ACKNOWLEDGED → PROCESSING → REPLIED
                         ↘ FAILED / EXPIRED / NEEDS_HUMAN
```

至少需要支持：已排队、已路由、已送达目标 Adapter、目标 Harness 已确认、处理中、已回复、投递失败、目标岗位未绑定/离线、需要 Human 处理。消息投递失败不能丢失原消息，必须能重试或改派。

### 6.3 与任务和状态变更的关系

- Task 分配、状态变化、阻塞、Review 请求和结果提交可以按 Team/Role 订阅策略生成消息。
- 消息正文可以保持通用自然语言，但路由目标、消息类型、Task、Reference 和回复关系必须结构化。
- `COMMAND` 适合“阅读文件、完成任务、提交 Review”等任务指令。
- `QUESTION`/`ANSWER` 适合上下文澄清；回复应回到同一 `thread_id`，并可关联决策文件。
- 消息不能绕过现有权限、依赖、租约和 Human 审批；收到消息不等于任务自动进入 `DOING`。
- 重要答案和决策仍应写入 Workspace，消息只作为协作入口和可追踪上下文。

### 6.4 Figma 本次已覆盖内容

本次新增画板已经覆盖：

1. Team Space 的“岗位通信”入口，按 Role/Task 展示非即时线程。
2. Leader → QA 的发送消息窗口，支持目标 Role、消息类型、Task 和文件引用。
3. QA 的确认和反问，展示 `reply_to`、线程上下文和路由快照。
4. 已排队、已路由、已送达、已确认、处理中、已回复、投递失败和目标未绑定等状态说明。
5. 消息与 Task、Reference、Binding、Adapter 和审计记录的关联。
6. 未绑定/离线后的持久化、重试、改派和 Human 处理入口。
7. 状态事件触发通知、QA `ANSWER` 回复和回复送达页面。

### 6.5 开发中调整登记

1. Message 仅面向 Role，还是允许 Human 作为统一接收主体，需要在开发前确定数据模型：推荐将 Human 投递建模为独立 `Notification`，并与 Event/Message 关联。
2. 完成 `Message`、`Notification`、`Event` 的数据字典、错误码、订阅配置接口和状态回放接口；当前 Figma 已足够支持实现，但 PRD 仍需在开发文档中细化。
3. 将通知策略、活动审计、Harness 连接、依赖和执行记录纳入正式主导航或统一“我的工作台”入口。
4. 接入真实 Task/Message 表单校验、加载、失败、权限和并发版本控制。
5. 修复消息终态页 3 处文字裁切，并完成响应式、键盘操作、焦点、错误朗读和减少动态效果验证。

## 6.6 PRD 通用协议与 Figma 对齐情况

本轮确认 PRD 已在 6.12、14.2、14.3 正式补充 Message/Thread 对象、操作、状态、幂等、订阅、重试和 Binding 规则；Figma 也已补齐对应页面。Message 的 `RETRYING`、`FAILED`、`EXPIRED` 现在与 PRD 一致，上一轮协议缺口已关闭。

- 仍需在开发数据模型中明确 Human 通知与 Role-to-Role Message 的关系。PRD 的 Message 字段要求 `to_role_id`，而通知策略包含 `Human · Leader` 接收范围；这属于协议边界确认项，不要求本轮回改 Figma。
- `retry_message`、`reassign_message` 的幂等键虽由 14.3 的通用规则覆盖，但开发接口应显式采用统一幂等参数或请求头，避免实现分叉。
- 14.1 的“协议只支持身份绑定、任务路由、状态更新和查询”与 14.2 已列出的 Message/Thread 操作存在文字范围不一致，登记为 PRD/开发协议文档修订项。

## 7. 开发中调整登记

本轮不再安排 Figma 设计整改，以下事项进入开发阶段：

1. 锁定 Human 通知与 Role-to-Role Message 的数据模型和投递边界。
2. 接入真实表单、权限、状态机、通知、审计、重试、改派和 Binding 解析服务。
3. 将 Harness 连接、活动审计、通知策略、依赖和执行记录纳入正式产品导航。
4. 补齐 Team/Workspace/Role 编辑、停用、归档、Binding 历史、完整 Artifact 详情和移动端任务详情。
5. 修复 `消息投递终态` 的 3 处文字裁切，完成响应式、无障碍和真实 Harness 接入验证。

## 8. 验证记录

- 已完成 PRD、设计方案和 Figma 节点索引的交叉核对。
- 已复核当前 Figma 的 36 个顶层画板、节点结构、129 条原型 reactions 和新增页面截图；新增画板节点范围为 `13:353`～`13:365`、`29:731`～`29:733`、`45:806`、`46:815`、`59:839`、`64:848`。
- 当前共检查到 129 条 reactions、32 个唯一目标；所有正式导航和 Hover 组件目标均可通过 `getNodeByIdAsync` 解析。
- 已确认 Harness 注册与健康检查、任务创建、Review、失败重试、依赖处理、审批、Run 对账、消息重试/改派和消息 `FAILED / EXPIRED` 终态均已有主操作跳转。
- 已发现 `消息投递终态` 页面有 3 个可见文字节点超出其 `clipsContent` 父容器，属于开发阶段视觉修正项，不阻断开发启动。
- `design/figma-state.json` 当前已同步记录 36 个画板、129 条 reactions、12 条 Hover reactions、610 个可见文字节点和 211 个组件实例；其结构检查结果与本轮 Figma 读取一致。
- 本轮未修改 Figma、PRD、设计方案或 Figma 索引；仅更新本审计报告，剩余问题按“开发中调整”登记。
- 当前项目没有可运行前端、接口或测试工程，因此本报告不包含浏览器端交互、无障碍、性能和真实 Harness 接入验证。
