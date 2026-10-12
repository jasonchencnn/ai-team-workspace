# 领域词汇、事实归属与聚合边界

> 对应节点：DEV-00-02  
> 状态：Frozen  
> 任何改变对象身份、所有权或聚合边界的实现必须新增 ADR。

> 2026-10-10 已接受补充：[ADR-0007 第 1 条](../adr/0007-notification-resolution-and-policy-semantics-proposal.md)规定 Notification 与 Policy 使用同一主程序可信安装 Owner 身份端口；UUID 格式与 Role 排除不构成身份来源。第 2～7 条仍 Proposed，不改变本规格的其余边界。

## 1. 最高层事实归属

```text
组织与控制权属于 Human
稳定岗位身份属于 Role
执行能力属于 Harness Instance
岗位与执行者关系属于 Binding
长期知识与业务正文属于 Workspace
工作状态属于 Task
一次执行尝试属于 Run
可验证结果属于 Artifact
系统事实属于 Event
岗位协作属于 Message / Thread
Human 投递属于 Notification
风险授权属于 Approval
```

## 2. 统一词汇

| 术语 | 精确定义 | 不是 | 身份稳定性 |
|---|---|---|---|
| Human | 有真实控制权的自然人主体；P0 只有 Owner | Role、Harness | 安装内稳定 |
| Team | 协作、权限、任务、Workspace 和 Event 的隔离边界 | Workspace、项目目录 | 稳定，归档不复用 ID |
| Role | Team 内长期岗位身份和授权主体 | 模型、Harness、Session | 稳定；换绑不变 |
| Harness Type | 一类执行工具与能力模型 | 已登录实例 | 稳定枚举/注册项 |
| Harness Instance | 已配置、可由平台联系的具体执行实例 | Role、Run | 稳定；可禁用 |
| Adapter | 平台与一类 Harness 的协议翻译和进程边界 | 领域服务、任务调度器 | 按版本识别 |
| Binding | Role 与 Harness Instance 在某时间段的代表关系 | Role 所有权转移 | 版本化且追加历史 |
| Workspace | Team 的逻辑项目事实源，可映射本地/Git 目录 | 数据库、日志 | 稳定绑定，可重新验证 |
| Task | 轻量工作状态索引，分配给 Role | 需求正文、一次执行 | 稳定；可重开/取消 |
| Dependency | Task 间的有向约束 | 消息顺序 | 随 Task 图版本化 |
| Run | 某 Binding 为某 Task 进行的一次执行尝试 | Task 最终状态 | 不可覆盖；重试新建 |
| Lease | 允许某 Run 暂时成为 Task 主执行的时效授权 | 永久权限 | 可过期/冻结 |
| Fencing Token | 单调变化的写入世代，拒绝旧 Run 晚到写入 | Session ID | 只增不减 |
| Reference | 指向 Workspace 文件/锚点、Commit、报告或受控 URL 的稳定引用 | 内容副本 | 可失效但历史保留 |
| Artifact | 对可验证产物的结构化索引 | 第二套文件仓库 | 版本化、可追溯 |
| Event | 已发生系统事实的追加写记录 | 可回复消息、业务正文 | 不可更新/删除 |
| Thread | 同一岗位协作话题的持久化容器 | Harness 私聊 | 稳定，归属 Team |
| Message | Role-to-Role 的短指令、问题、回答、交接或状态信息 | Human 通知、长期知识 | 原文和路由历史不可改 |
| Delivery Attempt | 一次 Message 投递尝试 | Message 本身 | 每次重试新建 |
| Notification | 面向 Human 的可读投递 | Role Message | 可读/确认，事实来源仍是 Event |
| Attention Item | Human 工作台的待处理索引 | 新业务事实 | 可由 Approval/Run/Message 等派生 |
| Permission Policy | Team/Role/路径/动作的授权规则 | Harness capability | 版本化 |
| Approval | Human 对特定高风险动作、范围和期限的授权决定 | 永久扩权 | 范围/期限受限且不可复用 |
| Review | 对 Task 产物的验收决定与意见 | Run 成功 | 追加历史，当前结果可查询 |

## 3. 聚合边界

| 聚合根 | 内含实体/值对象 | 可直接保证的不变量 | 跨聚合只保存 |
|---|---|---|---|
| Team | Team settings、状态 | Team 隔离、归档门禁 | 其他对象 ID |
| Role | Role profile、职责、默认目录、状态 | Role ID 稳定；停用后不领新任务 | Team/Policy ID |
| HarnessInstance | type、location、capabilities、credential_ref、health | 禁用实例不可启动新 Run；无明文凭据 | Adapter/Team ID |
| Binding | 版本、有效区间、策略、快照 | Role 同时最多一个 ACTIVE 主 Binding | Role/Harness/Actor ID |
| Workspace | root reference、kind、validation、policy | Team 唯一逻辑 Workspace；路径需验证 | Team ID |
| Task | owner、status、references、results、version | 状态条件、owner 是 Role、乐观锁 | Role/Run/Artifact ID |
| Run | attempt、status、lease、fencing、workspace、exit | 状态不可倒退；旧 token 无写权 | Task/Binding/Artifact ID |
| Artifact | type、location、version、checksum、visibility | creator Role/Run/位置不可缺 | Task/Run/Reference ID |
| Thread | participants、Messages | 目标为 Role；reply_to 在同 Thread | Task/Reference/Binding snapshot |
| NotificationPolicy | recipient Human、filters、dedupe、escalation | 接收者是 Human；策略不改变 Event | Team/Role/Event type |
| Approval | action、scope、requester、decider、expiry、status | 决定不可改写；授权不超 scope/expiry | Task/Run/Actor ID |
| Event | actor、type、subject、metadata、hash | 追加写、关联完整、脱敏 | 所有关联对象 ID |

`TaskDependency` 是 Team 内 Task 图的一部分，由 Domain Graph Service 在单事务中维护。它不独立承载业务生命周期。

`DeliveryAttempt` 隶属 Message 投递历史；Message 内容不可因重试而改写。需要修改内容时创建新 Message。

## 4. 命令写入所有权

| 事实 | 唯一写入者 | 禁止写入者 |
|---|---|---|
| Team/Role/Policy | Application service，经 Human actor 授权 | UI、Adapter、Harness |
| Binding | Binding service，经 Human-only 门禁 | Harness、Role 自身 |
| Task 状态 | Task service，经状态机/权限/版本校验 | UI 本地状态、Adapter 直接写库 |
| Run/Lease/Fencing | Run service / Adapter supervisor | Harness 自报覆盖、UI |
| Workspace 文件 | 获授权 Run/Human，经 path + fencing 检查 | 旧 Run、未授权 Adapter |
| Artifact 索引 | Artifact service，经 Run/Role/版本校验 | Adapter 直接写库 |
| Event/Outbox | 与业务命令同事务的 Event writer | 任意客户端更新/删除 |
| Message/Thread | Messaging service | Activity Stream、Adapter 直写 |
| Notification | Notification service，由 Event/Policy 派生 | Message 服务伪造 Human Role |
| Approval | Approval service，由 Human 决定 | 请求者自批、Harness 自批 |

## 5. 引用关系与删除语义

- 核心对象通过稳定 ID 关联，不级联硬删除。
- Team 归档后对象只读；导出/恢复流程仍可读取。
- Role 停用后保留 Task、Thread、Artifact、Binding 历史；新分配和新投递被拒。
- Harness Instance 禁用后保留 Binding/Run 快照；不能创建新 Binding/Run。
- Task 取消不删除 Run/Artifact/Event；重开使用同一 Task ID 和递增 version。
- Workspace 引用失效不删除 Reference；记录验证状态并引导修复。
- Message 到期/失败不删除原文或投递尝试。

## 6. 跨边界一致性

单个命令只修改一个主聚合；必要的 Event/Outbox 与其同事务提交。跨聚合副作用通过 Outbox 驱动，并遵循：

1. 消费者幂等；
2. Event 至少一次；
3. 失败不回滚已发生的业务事实；
4. 派生 read model 可重建；
5. 权限在最终执行点重新校验。

需要原子保证的例外：Task claim 同时检查 Task、依赖、Binding、权限并创建 Run/Lease；该用例由 Application transaction script 明确包裹，不能分散到 UI 或 Adapter。

## 7. 明确区分

| 容易混淆 | 冻结结论 |
|---|---|
| Role vs Harness | Role 是负责人；Harness 仅为当前执行者。 |
| Task vs Run | Task 是工作；Run 是一次尝试。Run 成功不自动等于 Task DONE。 |
| Event vs Message | Event 是事实；Message 是需要另一个 Role 理解/处理/回复的内容。 |
| Message vs Notification | Message 面向 Role；Notification 面向 Human。 |
| Workspace vs Database | Workspace 保存业务正文；数据库保存工作状态与索引。 |
| Capability vs Permission | Capability 表示“能做”；Permission 表示“被允许做”。 |
| Session vs Context | Session 是 Harness 私有执行引用；显式 Context 来自 Workspace/Task/Artifact/Event。 |

## 8. DEV-00-02 审核结论

核心对象定义、身份稳定性、事实归属、聚合内不变量、跨聚合写入者和删除语义均已唯一化；不存在用 Harness 替代 Role、用 Run 替代 Task、用 Message 替代 Event/Notification 的循环定义。

结论：**PASS**。
