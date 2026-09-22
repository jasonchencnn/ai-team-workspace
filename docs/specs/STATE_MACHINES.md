# 领域状态机与迁移规则

> 对应节点：DEV-00-03、DEV-00-04  
> 状态：Frozen  
> 原则：状态迁移由领域服务执行；UI 和 Adapter 只能提交命令，不能直接写状态。

## 1. 通用迁移规则

每个变更命令必须携带 `actor`、`request_id`、`idempotency_key` 和适用时的 `expected_version`。迁移成功时，在同一事务写入新版本、Event 与 Outbox；失败必须零业务副作用。

稳定错误码引用 `ERROR_CATALOG.md`：

- `DOMAIN_INVALID_TRANSITION`
- `DOMAIN_PRECONDITION_FAILED`
- `CONCURRENCY_VERSION_CONFLICT`
- `AUTH_PERMISSION_DENIED`
- `TASK_DEPENDENCY_UNMET`
- `RUN_FENCED`
- `BINDING_STALE_VERSION`
- `MESSAGE_TTL_EXPIRED`

## 2. Task 状态机

```text
TODO ──ready──> READY ──claim──> DOING ──submit──> REVIEW ──approve──> DONE
 │                │                │                  │                 │
 └──cancel────────┴──cancel────────┼──cancel──────────┴──cancel         └──reopen──> READY
                                   └──block──> BLOCKED ──resolve──> READY
                                   └──direct-complete──> DONE
```

`CANCELLED` 为 P0 终态；若工作需要恢复，创建新 Task 并引用旧 Task。`DONE` 可由 Human/授权 Leader 重开为 `READY`。

### 2.1 迁移表

| From → To | 命令/Actor | 前置条件 | 原子副作用 | Event | 主要拒绝码 |
|---|---|---|---|---|---|
| 创建 → TODO | `task.create`；Human/授权 Role | title、owner 或 references/依赖尚未完全可执行 | 创建 Task v1 | `TASK_CREATED` | `VALIDATION_INVALID_FIELD` |
| 创建 → READY | `task.create`；Human/授权 Role | owner 是 ACTIVE Role；引用有效；全部硬依赖 DONE；Role 可接收工作 | 创建 Task v1，产生可领取通知 | `TASK_CREATED`、`TASK_READY` | `TASK_DEPENDENCY_UNMET` |
| TODO → READY | `task.mark_ready`；Human/Leader/System | 信息完整、引用有效、硬依赖 DONE、owner ACTIVE | version+1，产生通知 | `TASK_READY` | `DOMAIN_PRECONDITION_FAILED` |
| TODO → CANCELLED | `task.cancel`；Human/授权 Leader | 无活动 Run | version+1，关闭开放 Attention | `TASK_CANCELLED` | `AUTH_PERMISSION_DENIED` |
| READY → DOING | `task.claim`；绑定 Role/System | 当前 Binding 有效；依赖满足；版本匹配；无有效 Run/lease；权限通过 | 创建 Run/lease/fencing，Task version+1 | `TASK_CLAIMED`、`RUN_QUEUED` | `TASK_ALREADY_CLAIMED` |
| READY → CANCELLED | `task.cancel`；Human/授权 Leader | 无活动 Run | version+1 | `TASK_CANCELLED` | `DOMAIN_PRECONDITION_FAILED` |
| DOING → BLOCKED | `task.block`；owner Role/Human/System | 提供短原因或有效 question reference；活动 Run 进入暂停/失败策略 | version+1，创建 Attention/通知 | `TASK_BLOCKED` | `TASK_BLOCK_REASON_REQUIRED` |
| DOING → REVIEW | `task.submit_review`；owner Role/Human | 至少一个可访问 Result Artifact；活动 Run 已 SUCCEEDED 或完成提交原子化 | version+1，创建 Review/通知 | `TASK_REVIEW_REQUESTED` | `TASK_RESULT_REQUIRED` |
| DOING → DONE | `task.complete`；授权策略/Leader/Human | Task 允许免 Review；结果 Artifact 存在；验收策略满足 | version+1，记录 acceptance | `TASK_COMPLETED` | `TASK_REVIEW_REQUIRED` |
| DOING → CANCELLED | `task.cancel`；Human/授权 Leader | 先完成 Run cancel/fence；不能只改 Task | version+1，关闭 lease | `TASK_CANCELLED` | `RUN_CANCEL_NOT_CONFIRMED` |
| BLOCKED → READY | `task.resolve_block`；Human/Leader/授权 Role | 阻塞原因已解决并留引用；owner/依赖/引用有效 | version+1，关闭对应 Attention，通知 owner | `TASK_UNBLOCKED`、`TASK_READY` | `DOMAIN_PRECONDITION_FAILED` |
| BLOCKED → CANCELLED | `task.cancel`；Human/授权 Leader | 无仍有效写入租约 | version+1 | `TASK_CANCELLED` | `RUN_FENCED` |
| REVIEW → DONE | `review.approve`；授权 Reviewer/Leader/Human | Review 对应当前 Task version/结果；验收项通过 | 新 Review decision，Task version+1 | `TASK_REVIEW_APPROVED`、`TASK_COMPLETED` | `REVIEW_STALE_RESULT` |
| REVIEW → READY | `review.request_changes`；授权 Reviewer/Leader/Human | 必须有意见；旧 Run 已终结 | 新 Review decision，Task version+1 | `TASK_REVIEW_CHANGES_REQUESTED`、`TASK_READY` | `REVIEW_COMMENT_REQUIRED` |
| REVIEW → CANCELLED | `task.cancel`；Human/授权 Leader | 说明取消原因 | version+1 | `TASK_CANCELLED` | `AUTH_PERMISSION_DENIED` |
| DONE → READY | `task.reopen`；Human/授权 Leader | 提供重开原因；owner/引用仍有效 | version+1，标记旧结果历史，通知 owner | `TASK_REOPENED`、`TASK_READY` | `DOMAIN_PRECONDITION_FAILED` |

### 2.2 明确禁止

- `TODO → DOING`：必须先通过 readiness 校验进入 READY。
- 任意状态 → `FAILED`：Task 无 FAILED 状态，失败属于 Run。
- `CANCELLED → *`：P0 不恢复已取消 Task。
- 在活动 Run 未 cancel/fence 前把 DOING 改为 CANCELLED/READY。
- 缺少 Artifact 进入 REVIEW，缺少验收记录进入 DONE。

## 3. Run 状态机

```text
QUEUED → STARTING → RUNNING → SUCCEEDED
   │         │          ├────→ FAILED
   │         │          ├────→ TIMED_OUT
   │         │          ├────→ CANCELLED
   │         │          └────→ UNKNOWN
   │         ├───────────────→ FAILED / TIMED_OUT / CANCELLED / UNKNOWN
   └─────────────────────────→ CANCELLED

UNKNOWN → RUNNING / SUCCEEDED / FAILED / CANCELLED / TIMED_OUT
```

`UNKNOWN` 是平台重启或 Adapter 失联后的恢复态，不是正常执行路径。`SUCCEEDED/FAILED/TIMED_OUT/CANCELLED` 为单个 Run 终态。

| From → To | 触发者 | 前置条件 | 副作用 | Event | 主要拒绝码 |
|---|---|---|---|---|---|
| 创建 → QUEUED | Task claim/重试服务 | Task READY、Binding 有效、租约可获得 | attempt+1、分配新 fencing token | `RUN_QUEUED` | `TASK_ALREADY_CLAIMED` |
| QUEUED → STARTING | Supervisor | Adapter compatible/healthy；凭据/Workspace 可准备 | 记录 process/session ref | `RUN_STARTING` | `ADAPTER_UNAVAILABLE` |
| QUEUED → CANCELLED | Human/授权 Leader | 尚未开始或已安全停止 | 释放租约、fence token | `RUN_CANCELLED` | `AUTH_PERMISSION_DENIED` |
| STARTING → RUNNING | Adapter | initialize/start 已确认，token/Binding version 匹配 | 设置 started_at、heartbeat deadline | `RUN_STARTED` | `BINDING_STALE_VERSION` |
| STARTING → FAILED | Supervisor/Adapter | 稳定非超时启动错误 | 保存 error class/diagnostic ref，释放租约 | `RUN_FAILED` | — |
| STARTING → TIMED_OUT | Supervisor | 超过 start timeout | 终止/隔离进程，fence token | `RUN_TIMED_OUT` | — |
| STARTING → CANCELLED | Human/Supervisor | cancel 确认或达到强制 fence 条件 | fence token、释放租约 | `RUN_CANCELLED` | `RUN_CANCEL_NOT_CONFIRMED` |
| STARTING → UNKNOWN | Recovery service | 平台无法确认外部进程状态 | 冻结租约，不派新 Run | `RUN_STATUS_UNKNOWN` | — |
| RUNNING → SUCCEEDED | Adapter/Supervisor | token、sequence、Binding snapshot 有效；结果引用校验通过 | 终结 Run、释放租约、登记 Artifact | `RUN_SUCCEEDED` | `RUN_FENCED` |
| RUNNING → FAILED | Adapter/Supervisor | 结构化失败分类；token 有效 | 终结 Run、释放租约、生成 Attention（适用时） | `RUN_FAILED` | `RUN_FENCED` |
| RUNNING → TIMED_OUT | Supervisor | 心跳/总时长超限且对账仍无法证明运行 | fence、释放租约、Attention | `RUN_TIMED_OUT` | — |
| RUNNING → CANCELLED | Human/Supervisor | cancel 确认；或 Human takeover 先 fence | 终结 Run、释放租约 | `RUN_CANCELLED` | `AUTH_PERMISSION_DENIED` |
| RUNNING → UNKNOWN | Recovery service | 平台重启/连接丢失且事实不明 | 冻结租约与写入，进入对账 | `RUN_STATUS_UNKNOWN` | — |
| UNKNOWN → RUNNING | Reconcile service | Adapter 证明同 session/token 仍运行；未产生更新世代 | 恢复 heartbeat deadline | `RUN_RECONCILED_RUNNING` | `RUN_FENCED` |
| UNKNOWN → SUCCEEDED | Reconcile service | 可验证完成结果及 sequence/token | 登记结果、释放租约 | `RUN_RECONCILED_SUCCEEDED` | `ARTIFACT_INVALID` |
| UNKNOWN → FAILED/TIMED_OUT/CANCELLED | Reconcile service/Human | Adapter 证明终态，或无法恢复并按策略/Human 决定 | fence、释放租约、保留诊断 | `RUN_RECONCILED_*` | `DOMAIN_PRECONDITION_FAILED` |

自动重试只读取终态 Run，创建新 Run；禁止把旧 Run 改回 QUEUED。

## 4. Binding 状态机

状态：`PENDING_CHECK`、`ACTIVE`、`DRAINING`、`REVOKED`、`FAILED`。未绑定由“无 ACTIVE Binding”表达，不创建 `UNBOUND` 伪记录。

| From → To | Actor/场景 | 前置条件 | 副作用 | Event | 主要拒绝码 |
|---|---|---|---|---|---|
| 创建 → PENDING_CHECK | Human Owner/Admin | Role/Harness ACTIVE；无冲突变更 | 创建新 version，锁定变更流程 | `BINDING_CHANGE_REQUESTED` | `BINDING_CHANGE_IN_PROGRESS` |
| PENDING_CHECK → ACTIVE | Binding service | Adapter 健康、能力/版本/凭据通过；Role 无其他 ACTIVE | 设置 effective_from | `ROLE_BOUND`/`ROLE_REBOUND` | `ADAPTER_INCOMPATIBLE` |
| PENDING_CHECK → FAILED | Binding service/Human | 检查失败或 Human 取消 | 保存脱敏诊断；不影响旧 ACTIVE | `BINDING_CHANGE_FAILED` | — |
| ACTIVE → DRAINING | Human 选择“完成后换绑” | 存在活动 Run | 禁止领取新任务；旧 Run 可完成；排队消息投递时重新解析 | `BINDING_DRAINING` | `AUTH_HUMAN_REQUIRED` |
| DRAINING → REVOKED | Binding service | 活动 Run 全部终结；新 Binding 检查通过 | effective_to，旧实例拒收新消息 | `ROLE_UNBOUND` | `RUN_ACTIVE_EXISTS` |
| ACTIVE → REVOKED | Human 解绑/取消重开/接管 | 活动 Run 已完成 cancel+fence，或无活动 Run | effective_to，撤销新领取/投递 | `ROLE_UNBOUND` | `RUN_CANCEL_NOT_CONFIRMED` |
| DRAINING → ACTIVE | Human 取消待生效换绑 | 新 Binding 尚未 ACTIVE，旧 Run 仍合法 | 恢复新领取 | `BINDING_CHANGE_CANCELLED` | `BINDING_STALE_VERSION` |

三种换绑策略：

1. **完成后换绑**：旧 `ACTIVE→DRAINING`；新为 `PENDING_CHECK`；旧 Run 终结后旧 `REVOKED`、新 `ACTIVE`。
2. **取消并重开**：先取消/fence 旧 Run，旧 `REVOKED`；新检查通过后 `ACTIVE`；Task 回 READY，新 Run 另建。
3. **人工接管**：先取消/fence，旧 `REVOKED`；Role 可暂时无 ACTIVE Binding，Human actor 接管；后续另行绑定。

## 5. Message 与 Delivery 状态机

Message 是持久内容；DeliveryAttempt 是可重试的投递尝试。主 Message 状态用于 UI 汇总：

```text
DRAFT → QUEUED → ROUTED → DELIVERED → ACKNOWLEDGED → PROCESSING → REPLIED
           │         │
           ├─────────┴──→ FAILED
           ├────────────→ EXPIRED
           └────────────→ NEEDS_HUMAN
```

`RETRYING` 是“已安排下一次 attempt”的派生瞬态，不持久化为 Message 终态。每次 attempt 单独记录 `PENDING → IN_FLIGHT → SUCCEEDED/FAILED/ABANDONED`。

| From → To | Actor/命令 | 前置条件 | 副作用 | Event | 主要拒绝码 |
|---|---|---|---|---|---|
| 创建 → DRAFT | Sender Role/Human 代发 | sender 有权向目标 Role 发送 | 保存草稿，不投递 | `MESSAGE_DRAFTED` | `AUTH_PERMISSION_DENIED` |
| DRAFT → QUEUED | `message.send`；sender | to_role、type、body/引用、TTL、幂等键有效 | 冻结内容，记录 sender Binding snapshot | `MESSAGE_SENT`、`MESSAGE_QUEUED` | `VALIDATION_INVALID_FIELD` |
| QUEUED → ROUTED | Router | TTL 有效；目标 Role ACTIVE；解析当前 ACTIVE Binding；权限/订阅允许 | 新建 attempt 与 target Binding snapshot | `MESSAGE_ROUTED` | `BINDING_NOT_FOUND` |
| QUEUED → NEEDS_HUMAN | Router | 无有效 Binding、Role 停用、改派需授权 | 创建 Attention Item | `MESSAGE_NEEDS_HUMAN` | — |
| QUEUED/ROUTED → EXPIRED | Scheduler | `now >= expires_at` | 终止待处理 attempt | `MESSAGE_EXPIRED` | — |
| ROUTED → DELIVERED | Adapter | attempt、binding、sequence 匹配，Adapter 已接收 | attempt SUCCEEDED，记录 delivered_at | `MESSAGE_DELIVERED` | `BINDING_STALE_VERSION` |
| ROUTED → QUEUED | Retry scheduler | retryable 失败、未达上限、TTL 有效 | 当前 attempt FAILED；计算退避；派生 RETRYING | `MESSAGE_RETRY_SCHEDULED` | — |
| ROUTED → FAILED | Router | 非重试错误或达到上限 | attempt FAILED，创建 Attention | `MESSAGE_FAILED` | — |
| DELIVERED → ACKNOWLEDGED | Target Adapter/Harness | 目标确认 message_id，Binding snapshot 匹配 | acknowledged_at | `MESSAGE_ACKNOWLEDGED` | `BINDING_STALE_VERSION` |
| ACKNOWLEDGED → PROCESSING | Target Harness/Adapter | 显式声明处理；不自动 claim Task | processing_at | `MESSAGE_PROCESSING` | `AUTH_PERMISSION_DENIED` |
| PROCESSING/ACKNOWLEDGED → REPLIED | Target Role | 新 Reply Message 同 thread，`reply_to` 指向原消息 | 创建新 Message/attempt；原消息标记 replied_at | `MESSAGE_REPLIED` | `MESSAGE_THREAD_MISMATCH` |
| FAILED → QUEUED | `message.retry`；Human/授权 Role | TTL 有效；失败原因可重试；幂等键新且内容不改 | 新 attempt，保留旧 attempts | `MESSAGE_REQUEUED` | `MESSAGE_TTL_EXPIRED` |
| NEEDS_HUMAN → QUEUED | 绑定修复/授权改派后 Human 操作 | 新目标/Binding 有效，权限通过，TTL 有效 | 新 attempt，记录 route override | `MESSAGE_REASSIGNED`/`MESSAGE_REQUEUED` | `AUTH_HUMAN_REQUIRED` |

`EXPIRED` 不在原 Message 上重开；创建新后续 Message 并引用旧 ID。`FAILED` 是某投递周期终态，只有显式 retry 才能回到 QUEUED，历史 attempt 和 Event 不可改写。

## 6. Notification 与 AttentionItem

Notification 面向 Human，不经过 Role Binding。

Notification 状态：`UNREAD → READ → DISMISSED`，以及 `UNREAD/READ → EXPIRED`。`READ` 表示已查看，不表示关联事项已解决。去重只合并投递呈现，不删除源 Event。

AttentionItem 状态：`OPEN → RESOLVED/DISMISSED`；若底层 Approval/Run/Message 仍需处理，不允许仅从 UI 把它标为 RESOLVED。

| 迁移 | Actor | 条件 | Event/规则 |
|---|---|---|---|
| 创建 → UNREAD | Notification service | Event 匹配 policy；dedupe key 不违反窗口规则 | `NOTIFICATION_CREATED` |
| UNREAD → READ | Human recipient | recipient 匹配 | `NOTIFICATION_READ` |
| READ/UNREAD → DISMISSED | Human recipient | 非强制保留通知 | `NOTIFICATION_DISMISSED`；不改变源 Event |
| UNREAD/READ → EXPIRED | Scheduler | policy TTL 到期 | `NOTIFICATION_EXPIRED` |
| Attention OPEN → RESOLVED | 对应领域用例 | 底层事项已进入已解决终态 | `ATTENTION_RESOLVED` |
| Attention OPEN → DISMISSED | Human | 类型允许忽略且提供原因 | `ATTENTION_DISMISSED` |

## 7. Approval 状态机

```text
PENDING → APPROVED / REJECTED / EXPIRED / CANCELLED
```

全部终态不可重开；新请求必须新建 Approval。

| 迁移 | Actor | 前置条件 | 副作用 | Event | 主要拒绝码 |
|---|---|---|---|---|---|
| 创建 → PENDING | Role/Harness 通过控制面/Human | action、resource scope、理由、期限完整；请求者不能预授权 | 创建 Attention/Notification；暂停相关步骤 | `APPROVAL_REQUESTED` | `APPROVAL_SCOPE_INVALID` |
| PENDING → APPROVED | Human Owner/有权 Admin | 未过期；decider 与 requester 不构成自批；scope 在 Team 上限内 | 生成受限 grant；恢复等待步骤 | `APPROVAL_APPROVED` | `AUTH_HUMAN_REQUIRED` |
| PENDING → REJECTED | Human | 提供决定/可选原因 | 不授予权限；相关步骤失败或保持 BLOCKED | `APPROVAL_REJECTED` | `CONCURRENCY_VERSION_CONFLICT` |
| PENDING → EXPIRED | Scheduler | expires_at 到达 | 不授予权限；关闭 Attention | `APPROVAL_EXPIRED` | — |
| PENDING → CANCELLED | requester/Human | 尚未决定 | 不授予权限；记录取消 actor | `APPROVAL_CANCELLED` | `DOMAIN_INVALID_TRANSITION` |

Approval grant 每次执行时再次校验 action/resource/scope/expiry/actor/context；“已批准”不是永久 Role 权限。

## 8. DEV-00-03/04 审核结论

- Task、Run、Binding 的每条 P0 迁移均包含 actor、前置条件、副作用、Event 和拒绝码。
- Message 内容、DeliveryAttempt、Human Notification、AttentionItem 和 Approval 已分离。
- 重试、TTL、显式改派、换绑快照、UNKNOWN 对账和 fencing 语义唯一。
- 非法迁移清单可直接生成后续表驱动/性质测试。

结论：**PASS**。
