# Adapter Protocol V0

> 对应节点：DEV-00-08  
> 状态：Frozen  
> 版本：`0.1.0`  
> Schema：`adapter-manifest.schema.json`、`adapter-message.schema.json`
> 机器约束补强：`docs/adr/0002-strengthen-adapter-v0-machine-contract.md`

## 1. 目标与边界

Adapter V0 只负责 Harness 的发现、健康、执行、心跳、取消、对账和结果回传。它不理解业务需求、不创建权限、不直接写数据库、不直接变更 Task，也不能绕过 Workspace path/fencing 检查。

核心协议中禁止出现 Codex、Claude Code、MiniMax 等专属业务字段。专属配置只能位于 Adapter 自己的受控 `configuration`，核心不据此改变领域语义。

## 2. 传输

- 每个 Adapter 由平台以独立子进程启动。
- stdin/stdout 使用 UTF-8、每行一个完整 JSON-RPC 2.0 对象（JSON Lines）。
- stdout 只允许协议消息；人类诊断写 stderr，仍必须结构化、脱敏且受大小限制。
- 单条协议消息最大 1 MiB；接收方必须在 JSON 解析前按原始 UTF-8 行字节数拒绝超限消息。Run output 单条 `content` 最大 64 KiB，同样按 UTF-8 字节而非字符数计算。更大内容写入受控文件并返回 Artifact candidate 引用。
- Schema 的 `x-maxJsonUtf8Bytes` 与 `x-maxUtf8Bytes` 是强制运行时关键字；validator 必须实现它们或提供等价的传输层字节检查，不能静默忽略。
- 平台为每个请求设置 timeout；超时后不假定请求未执行，按 idempotency/对账处理。
- Adapter 必须响应 EOF/`adapter.shutdown` 并清理其子进程；平台可在 grace period 后强制终止。

## 3. 版本与兼容

- `protocol_version` 与 Adapter 版本使用规范 SemVer 2.0.0；核心版本和纯数字预发布标识禁止前导零，允许规范预发布与构建元数据。V0 当前为 `0.1.0`。
- 握手时平台发送一个版本，Adapter 返回它支持的版本集合和已选择版本。
- patch：只修正文档/约束，不改变合法 payload；minor：增加可选字段/能力；major：破坏性变更。
- 同一 major 下接收方必须忽略 Schema 明确允许的未知扩展字段；核心业务消息默认 `additionalProperties=false`，扩展只能放到 `extensions` 对象。
- 无交集返回 JSON-RPC error `-32010`，映射 `ADAPTER_INCOMPATIBLE`，禁止创建新 Run。

## 4. Manifest 与握手

平台启动前读取并验证 Adapter manifest。manifest 只描述静态信息，不代表当前健康或权限。

`initialize` 请求包含：

- 平台协议/应用版本；
- instance ID 和非秘密配置引用；
- locale、platform、工作目录能力；
- request/correlation ID。

成功结果包含：

- selected protocol version；
- Adapter/Harness 版本；
- capabilities；
- auth state（只含状态和引用，不含秘密）；
- session isolation 与 recovery mode。

能力示例：`run.start`、`run.cancel`、`run.reconcile`、`run.output.streaming`、`session.resume`。Capability 只是能力声明，实际操作仍要权限检查。

## 5. 请求方法

| Method | 目的 | 幂等/超时语义 |
|---|---|---|
| `initialize` | 版本/能力/认证状态协商 | 同一 instance/config generation 幂等 |
| `health.check` | 主动健康和诊断 | 只读；可安全重试 |
| `run.start` | 启动某 Run | `run_id + fencing_token` 唯一；重复返回同一 session 或冲突 |
| `run.status` | 查询当前 Adapter 视角状态 | 只读；不能直接改变核心状态 |
| `run.cancel` | 请求停止 Run | 幂等；返回 accepted/confirmed，confirmed 前平台不释放旧写权 |
| `run.reconcile` | 平台重启后对账 | 只读事实查询；返回 running/terminal/not_found/unknown |
| `adapter.shutdown` | 优雅关闭 Adapter | 幂等；不自动取消仍由 Harness 运行的外部任务，必须报告其状态 |

所有 Run 方法必须携带 `team_id/task_id/run_id/role_id/binding_id/binding_version/fencing_token`。Adapter 缺失或修改这些值视为协议违规。

## 6. Adapter 通知

| Method | 语义 | 必需字段 |
|---|---|---|
| `run.heartbeat` | Run 活性和简短进度 | run context、sequence、observed_at、phase |
| `run.output` | 增量 stdout/stderr/progress 或 Artifact candidate | run context、sequence、kind、content/artifact |
| `run.completed` | Adapter 认为 Run 成功终结 | run context、sequence、completed_at、result_references |
| `run.failed` | Adapter 认为 Run 失败终结 | run context、sequence、failed_at、error_class、code、retryable |

通知是 Adapter 的陈述，不是领域事实的最终裁决。平台验证 Binding version、fencing token、sequence、Schema、权限和引用后，才产生领域 Event/状态变化。

`run.output` 的载荷按 `kind` 互斥：

- `stdout`、`stderr`、`progress` 必须且只能携带非空 `content`；
- `artifact_candidate` 必须且只能携带 `kind=artifact` 的 Artifact reference；
- `content`、`artifact` 均禁止 `null`，不得用空字符串或空引用占位。

## 7. 排序、重复与晚到消息

- 每个 Run 的 Adapter 通知 `sequence` 从 1 单调递增。
- 相同 run/sequence/payload hash 为重复，平台幂等忽略但可返回/记录确认。
- 相同 sequence 不同 payload 为 `ADAPTER_PROTOCOL_VIOLATION`。
- sequence 跳跃使 Run 进入待对账，不直接丢弃后续事实。
- 终态后晚到 heartbeat/output 记录诊断但不改变 Run。
- fencing token 不匹配的所有写入型通知返回/记录 `RUN_FENCED`；旧 token 永不恢复。

## 8. Run 启动上下文

`run.start` 只发送显式上下文：

- 稳定 Role 身份、职责摘要和禁止行为；
- Task ID、标题、Reference 和 Result policy；
- Shared Workspace 根与隔离执行目录；
- 允许读写的相对路径范围；
- 验证命令入口；
- 非秘密环境引用和最小凭据注入结果；
- timeout、heartbeat interval、output limits。

禁止发送旧 Harness 隐藏思考或把完整私聊 Session 当作事实源。

## 9. 取消与恢复

- `run.cancel` 返回 `accepted` 只表示已收到；只有 `confirmed=true` 或控制面已提升 fencing generation，旧执行者才失去有效写权。
- 平台崩溃后，所有非终结 Run 先进入 UNKNOWN，再调用 `run.reconcile`。
- `run.reconcile` 结果：
  - `running`：必须证明相同 session 和 fencing token；
  - `succeeded/failed/cancelled/timed_out`：附可验证终态事实；
  - `not_found`：不能直接视为 failed，由策略/Human 决定；
  - `unknown`：保持冻结并生成 Attention。
- Adapter 不得自行创建替代 Run。

## 10. 错误映射

JSON-RPC 标准错误保留：`-32700` parse、`-32600` invalid request、`-32601` method not found、`-32602` invalid params、`-32603` internal。

平台自定义范围：

| JSON-RPC code | 领域错误 |
|---:|---|
| -32010 | `ADAPTER_INCOMPATIBLE` |
| -32011 | `ADAPTER_CAPABILITY_MISSING` |
| -32012 | `ADAPTER_AUTHENTICATION_FAILED` |
| -32013 | `BINDING_STALE_VERSION` |
| -32014 | `RUN_FENCED` |
| -32015 | `RUN_OUTPUT_OUT_OF_ORDER` |
| -32016 | `WORKSPACE_PATH_OUTSIDE_SCOPE` |
| -32017 | `ADAPTER_UNAVAILABLE` |
| -32018 | `CONCURRENCY_IDEMPOTENCY_CONFLICT` |
| -32099 | `ADAPTER_PROTOCOL_VIOLATION` |

JSON-RPC 标准错误使用独立兼容分支，`data` 保持标准可选语义，不强加领域 envelope。

平台自定义错误只允许上表离散 code，必须携带 `error.data`。`error.data` 仅允许稳定 `domain_code`、`retryable`、`correlation_id`、脱敏 `reason_key` 和白名单字段；Schema 同时强制 code、`domain_code` 与 `retryable` 的冻结映射，禁止以合法 code 搭配另一领域错误。

## 10.1 ID 与时间的规范表示

- 所有领域、运行和追踪 ID 使用小写、带连字符的规范 UUIDv7，并校验 UUID variant；JSON-RPC `id` 仍按 JSON-RPC 2.0 使用字符串或非负整数。
- 所有协议时间必须是有效 UTC RFC 3339，固定 `YYYY-MM-DDTHH:mm:ss.sssZ`；时区偏移、缺失毫秒或超过三位小数秒均不属于 Adapter V0 规范表示。

## 11. 安全约束

- 平台使用参数数组启动 Adapter，禁止 shell 拼接。
- manifest/entrypoint 需在已批准位置；未来签名策略可扩展。
- credential value 不进入 JSON-RPC；平台通过受控环境/文件描述符/系统接口最小化注入。
- working directory 必须是控制面已创建并验证的目录；Adapter 不能请求任意宿主路径。
- output/reference 视为不可信输入，需 Schema、大小、路径、权限和 checksum 校验。
- stderr 和 JSON error 必须脱敏；禁止输出 token、cookie、完整环境或登录态。

## 12. 契约测试最小集

每个 Adapter 必须通过：

1. manifest 正反例；
2. initialize 版本交集/不兼容；
3. health healthy/degraded/unavailable/auth_failed；
4. start 幂等与同 run/different token 冲突；
5. heartbeat/output 正常、重复、乱序、空/错配载荷、UTF-8 64 KiB 与整消息 1 MiB 边界；
6. completed/failed 与晚到事件；
7. cancel accepted/confirmed/timeout；
8. reconcile 全部结果；
9. 崩溃、EOF、shutdown 和孤儿进程；
10. 秘密脱敏、越界路径和恶意 payload；
11. UUIDv7、UTC 三位毫秒时间、规范 SemVer、自定义错误码表与标准错误兼容分支。

## 13. DEV-00-08 审核结论

初始化、健康、启动、状态、心跳、输出、取消、对账、完成、失败、版本兼容、幂等、排序和安全边界均已唯一化并具有机器可读 Schema。

结论：**PASS**。
