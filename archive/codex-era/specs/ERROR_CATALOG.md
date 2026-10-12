# 稳定错误码与用户提示目录

> 对应节点：DEV-00-07  
> 状态：Frozen  
> 错误码是协议契约；改名或改变语义属于破坏性变更。

## 1. 错误 Envelope

```json
{
  "code": "CONCURRENCY_VERSION_CONFLICT",
  "message": "这项内容已被更新，请刷新后比较再提交。",
  "retryable": false,
  "field": null,
  "correlation_id": "0199...",
  "details": {
    "current_version": 8
  }
}
```

规则：

- `code` 稳定、面向程序；`message` 面向 Human，可本地化，不能包含堆栈和秘密。
- `details` 使用每个错误码允许的白名单字段；禁止透传环境变量、命令行、绝对凭据路径或 Adapter 原始 stderr。
- `retryable=true` 只表示相同业务意图在满足建议条件后可重试，不表示客户端应无限自动重试。
- field error 填稳定字段路径，如 `references[0].path`；领域/系统错误为 `null`。
- UI 必须显示 `correlation_id` 的短形式供诊断，但不能把它当错误原因。

## 2. 校验与领域错误

| Code | Retryable | 默认用户提示 | 建议动作/允许 details |
|---|:---:|---|---|
| VALIDATION_INVALID_FIELD | 否 | 请检查标出的字段。 | 修正字段；`field`,`reason_key` |
| VALIDATION_REQUIRED_FIELD | 否 | 这项内容不能为空。 | 补充字段；`field` |
| VALIDATION_REFERENCE_INVALID | 否 | 关联内容无法定位，请重新选择。 | 修复引用；`reference_id`,`reason_key` |
| DOMAIN_INVALID_TRANSITION | 否 | 当前状态不能执行此操作，请刷新后查看可用操作。 | 刷新；`object_type`,`current_state`,`requested_state` |
| DOMAIN_PRECONDITION_FAILED | 否 | 还不满足执行条件，请处理提示项后重试。 | 展示结构化 unmet 条件；`condition_keys` |
| DOMAIN_OBJECT_NOT_FOUND | 否 | 这项内容不存在或你无权查看。 | 返回列表；不暴露对象是否真实存在 |
| DOMAIN_OBJECT_INACTIVE | 否 | 这项内容已停用，不能继续操作。 | 由有权限 Human 恢复；`object_type` |
| DOMAIN_TEAM_ARCHIVED | 否 | 团队已归档，只能查看和导出。 | 恢复 Team；`team_id` |

## 3. 认证、权限与审批

| Code | Retryable | 默认用户提示 | 建议动作/允许 details |
|---|:---:|---|---|
| AUTH_UNAUTHENTICATED | 是 | 身份状态已失效，请重新验证。 | 重新验证本地身份；不回显凭据 |
| AUTH_PERMISSION_DENIED | 否 | 你没有执行此操作的权限。 | 查看权限依据/请求授权；`action`,`resource_type`,`policy_version` |
| AUTH_HUMAN_REQUIRED | 否 | 此操作必须由 Human 确认。 | 打开审批/接管入口；`action`,`approval_required` |
| AUTH_SCOPE_MISMATCH | 否 | 当前授权不包含这项资源或范围。 | 缩小动作或重新申请；`requested_scope`,`allowed_scope`（脱敏） |
| AUTH_APPROVAL_REQUIRED | 否 | 此操作需要先获得审批。 | 创建 Approval；`action`,`resource_type` |
| APPROVAL_SCOPE_INVALID | 否 | 审批范围不完整或超出团队上限。 | 修正 action/resource/scope；`field` |
| APPROVAL_EXPIRED | 否 | 此审批已过期，请重新申请。 | 新建 Approval；`approval_id`,`expired_at` |
| APPROVAL_ALREADY_DECIDED | 否 | 此审批已经处理。 | 刷新查看决定；`approval_id`,`status` |
| APPROVAL_SELF_DECISION_DENIED | 否 | 请求人不能批准自己的高风险操作。 | 交由 Human Owner；无敏感 details |

## 4. 并发、版本与幂等

| Code | Retryable | 默认用户提示 | 建议动作/允许 details |
|---|:---:|---|---|
| CONCURRENCY_VERSION_CONFLICT | 否 | 这项内容已被更新，请刷新后比较再提交。 | 刷新/对比；`expected_version`,`current_version` |
| CONCURRENCY_IDEMPOTENCY_CONFLICT | 否 | 同一请求标识已用于不同内容，请重新发起操作。 | 仅新业务意图生成新 key；不返回原 payload |
| CONCURRENCY_RESOURCE_BUSY | 是 | 资源正在处理中，请稍后重试。 | 有限退避；`retry_after_ms` |
| CONCURRENCY_LIMIT_REACHED | 是 | 当前并发任务已达上限，请等待已有任务结束。 | 等待/取消；`limit`,`active_count` |
| OUTBOX_DELIVERY_PENDING | 是 | 操作已保存，相关通知仍在处理中。 | 查询状态，不重复业务命令；`event_id` |

## 5. Task、依赖与 Review

| Code | Retryable | 默认用户提示 | 建议动作/允许 details |
|---|:---:|---|---|
| TASK_DEPENDENCY_UNMET | 否 | 前置任务尚未完成。 | 查看依赖；`blocking_task_ids` |
| TASK_DEPENDENCY_CYCLE | 否 | 这些依赖会形成循环，请调整后保存。 | 显示环路；`cycle_task_ids` |
| TASK_ALREADY_CLAIMED | 是 | 任务已由另一次执行领取。 | 查看当前 Run；`active_run_id` |
| TASK_BLOCK_REASON_REQUIRED | 否 | 标记受阻前需要填写原因或关联问题。 | 补充原因/Reference |
| TASK_RESULT_REQUIRED | 否 | 提交验收前需要至少一个有效结果。 | 关联 Artifact；`invalid_reference_ids` |
| TASK_REVIEW_REQUIRED | 否 | 此任务必须经过验收后才能完成。 | 提交 Review |
| TASK_OWNER_INVALID | 否 | 负责人岗位不可用，请重新选择。 | 选择 ACTIVE Role；`role_id` |
| TASK_REFERENCE_STALE | 否 | 任务引用的内容已变化，请确认后继续。 | 刷新基线/标记继续；`reference_id`,`expected`,`current` |
| REVIEW_STALE_RESULT | 否 | 结果已变化，不能使用旧验收决定。 | 刷新结果并重新验收；`review_id`,`current_task_version` |
| REVIEW_COMMENT_REQUIRED | 否 | 退回任务时需要填写修改意见。 | 补充意见 |

## 6. Run、租约与恢复

| Code | Retryable | 默认用户提示 | 建议动作/允许 details |
|---|:---:|---|---|
| RUN_FENCED | 否 | 此执行已失去写入权限，结果不会被接收。 | 打开当前有效 Run；`current_run_id` |
| RUN_LEASE_EXPIRED | 否 | 执行租约已过期，需要重新领取任务。 | 新 Run；`expired_at` |
| RUN_ACTIVE_EXISTS | 否 | 仍有执行在运行，需先完成、取消或接管。 | 展示 Run；`active_run_ids` |
| RUN_CANCEL_NOT_CONFIRMED | 是 | 尚未确认执行已停止，系统不会提前释放写入权限。 | 等待/对账/强制 Human 接管；`run_id` |
| RUN_STATUS_UNKNOWN | 是 | 暂时无法确认执行状态，已冻结新的写入和派发。 | 对账；`run_id`,`last_heartbeat_at` |
| RUN_RETRY_NOT_ALLOWED | 否 | 当前错误或状态不能直接重试。 | 查看改派/修复/接管选项；`reason_key` |
| RUN_OUTPUT_OUT_OF_ORDER | 是 | 收到的执行更新顺序异常，正在等待对账。 | Adapter 重发/对账；`expected_sequence`,`received_sequence` |
| RUN_OUTPUT_TOO_LARGE | 否 | 执行输出超过限制，已改为受控产物或被截断。 | 打开 Artifact；`artifact_id`,`limit_bytes` |

## 7. Binding、Harness 与 Adapter

| Code | Retryable | 默认用户提示 | 建议动作/允许 details |
|---|:---:|---|---|
| BINDING_NOT_FOUND | 否 | 目标岗位当前未绑定可用 Harness。 | 由 Human 绑定或改派；`role_id` |
| BINDING_STALE_VERSION | 否 | 岗位绑定已变化，此请求不会交给旧执行者。 | 重新读取 Role context；`current_binding_version` |
| BINDING_CHANGE_IN_PROGRESS | 是 | 岗位正在更换 Harness，请等待或取消当前更换。 | 查看待生效 Binding |
| BINDING_ACTIVE_RUNS_EXIST | 否 | 该岗位仍有运行中任务，请先选择处理方式。 | 完成后换绑/取消重开/接管；`run_ids` |
| HARNESS_INSTANCE_DISABLED | 否 | Harness 实例已停用。 | 选择其他实例/恢复 |
| ADAPTER_UNAVAILABLE | 是 | Harness 连接暂时不可用。 | 健康检查/重试/换绑；`health_check_id`,`retry_after_ms` |
| ADAPTER_INCOMPATIBLE | 否 | Adapter 版本与平台协议不兼容。 | 升级/降级；`supported_versions`,`reported_version` |
| ADAPTER_CAPABILITY_MISSING | 否 | 当前 Harness 不支持这项操作。 | 换实例/调整任务；`required_capability` |
| ADAPTER_AUTHENTICATION_FAILED | 否 | Harness 身份验证失败，请检查安全凭据引用。 | Human 更新 credential_ref；不返回秘密 |
| ADAPTER_PROTOCOL_VIOLATION | 否 | Adapter 返回了无法验证的数据，已停止接受此次结果。 | 查看脱敏诊断；`method`,`violation_key` |
| ADAPTER_PROCESS_EXITED | 是 | Adapter 进程意外结束。 | 对账/重试；`exit_kind`，不直接透传完整 stderr |

## 8. Workspace 与 Artifact

| Code | Retryable | 默认用户提示 | 建议动作/允许 details |
|---|:---:|---|---|
| WORKSPACE_NOT_AVAILABLE | 是 | 工作空间暂时不可用。 | 检查挂载/权限；`reason_key` |
| WORKSPACE_PATH_OUTSIDE_SCOPE | 否 | 该路径不在允许范围内。 | 选择授权路径/申请审批；返回规范相对路径，不回显秘密绝对路径 |
| WORKSPACE_PATH_NOT_FOUND | 否 | 文件或目录不存在。 | 修复引用/重新选择；`relative_path` |
| WORKSPACE_PATH_NOT_WRITABLE | 否 | 当前任务无权写入该位置。 | 调整 Task scope/路径；`relative_path` |
| WORKSPACE_SYMLINK_ESCAPE | 否 | 该路径会离开授权工作空间，已拒绝访问。 | 移除/改用安全路径；`relative_path` |
| WORKSPACE_NOT_GIT | 否 | 此操作需要 Git 工作空间。 | 使用单写者模式或绑定 Git Workspace |
| WORKTREE_CREATE_FAILED | 是 | 无法创建隔离执行空间。 | 查看诊断/修复 Git 状态；`reason_key` |
| WORKTREE_CONFLICT | 否 | 结果与当前基线冲突，需要创建集成任务或人工处理。 | 打开冲突；`conflict_id`,`paths`（授权范围内） |
| ARTIFACT_INVALID | 否 | 产物信息不完整或无法验证。 | 修复 location/version/checksum；`field` |
| ARTIFACT_REFERENCE_STALE | 否 | 产物基于旧版本，需要重新验证。 | 重验/重跑/确认继续；`baseline`,`current` |

## 9. Message、Notification 与导出

| Code | Retryable | 默认用户提示 | 建议动作/允许 details |
|---|:---:|---|---|
| MESSAGE_TTL_EXPIRED | 否 | 消息已过有效期，请创建一条新的后续消息。 | 新 Message 引用旧 ID；`expired_at` |
| MESSAGE_THREAD_MISMATCH | 否 | 回复与当前线程不一致。 | 刷新 Thread；`thread_id` |
| MESSAGE_DELIVERY_FAILED | 是 | 消息未送达，原内容已保留。 | 重试/改派/接管；`message_id`,`attempt_count`,`reason_key` |
| MESSAGE_REASSIGN_DENIED | 否 | 你无权改派这条消息。 | 交给 Human/授权 Leader |
| NOTIFICATION_POLICY_INVALID | 否 | 通知规则不完整或会屏蔽强制安全提醒。 | 修正规则；`field`,`reason_key` |
| EXPORT_JOB_FAILED | 是 | 导出未完成，未影响原始审计记录。 | 重试/查看诊断；`export_job_id`,`reason_key` |
| EXPORT_PERMISSION_DENIED | 否 | 只有 Human Owner 可以导出完整审计。 | 交由 Owner |

## 10. 系统与存储

| Code | Retryable | 默认用户提示 | 建议动作/允许 details |
|---|:---:|---|---|
| STORAGE_UNAVAILABLE | 是 | 本地数据暂时不可用，操作未完成。 | 有限重试/恢复；`retry_after_ms` |
| STORAGE_MIGRATION_REQUIRED | 否 | 数据需要升级后才能继续使用。 | 执行受控 migration/备份 |
| STORAGE_MIGRATION_FAILED | 否 | 数据升级失败，已停止打开以避免损坏。 | 使用备份/诊断；`migration_version`,`correlation_id` |
| STORAGE_INTEGRITY_FAILED | 否 | 检测到数据完整性问题，已进入保护模式。 | 停止写入、备份、修复；不暴露原始数据 |
| SECRET_STORE_UNAVAILABLE | 是 | 系统安全存储暂时不可用。 | 解锁/重试；不降级到明文文件 |
| INTERNAL_UNEXPECTED | 是 | 发生未预期错误，本次操作未确认完成。 | 用 correlation ID 诊断；客户端先查询结果再决定是否重试 |

## 11. 重试规则

- 只有目录中 `retryable=是` 且命令本身幂等时允许自动重试。
- 自动重试默认最多 3 次、指数退避并加入 jitter；总时长不得超过命令/Message TTL。
- 权限、校验、非法状态、版本冲突、fencing、TTL 过期不自动重试。
- `INTERNAL_UNEXPECTED` 后先按 idempotency key 查询首次结果，禁止盲目重放可能产生外部副作用的命令。

## 12. DEV-00-07 审核结论

认证、权限、并发、状态、依赖、Run、Binding、Adapter、Workspace、Message 和系统错误已稳定区分；每个用户提示可行动且 details 白名单不会泄露秘密。

结论：**PASS**。
