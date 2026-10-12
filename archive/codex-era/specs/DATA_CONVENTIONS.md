# ID、时间、版本、幂等、归档与留存规则

> 对应节点：DEV-00-06  
> 状态：Frozen

> 2026-10-10 已接受身份补充见 [ADR-0007 第 1 条](../adr/0007-notification-resolution-and-policy-semantics-proposal.md)：安装 Owner 身份由主程序持久端口提供；旧通知 UUID 不自动采用为 Owner，既有历史 ID 不因本条改写。其余条款仍待决。

安装身份记录及初始化标记须与数据库备份关联保留，恢复不得另造 Human ID；实际备份包/一致性恢复仍由 DEV-02-15 验收，本条不宣称已完成演练。

## 1. ID 规则

| 类型 | 规则 | 示例/说明 |
|---|---|---|
| 内部主键 | UUIDv7；应用层生成；全局唯一 | 不能从名称、路径或 Harness 推导 |
| 显示 ID | Team 内唯一、面向 Human、创建后不变 | `FE-023`、`RUN-0042`；不作外键 |
| request_id | 每次外部命令一次 UUIDv7 | 重试可产生新 request_id |
| idempotency_key | 调用者按逻辑操作生成的稳定字符串/UUID | 同一逻辑重试必须复用 |
| correlation_id | 一次跨层处理链稳定 ID | UI→IPC→Application→Adapter/Event |
| fencing_token | 每 Task 执行世代单调递增整数 | 不能使用随机 Session ID 替代 |
| sequence | 每 Run/DeliveryAttempt 内从 1 递增 | 去重和检测乱序，不跨对象比较 |

数据库对 UUID 采用统一 16-byte BLOB 表示；协议使用小写、带连字符的规范 UUID 字符串。实现阶段不得混用文本/BLOB 外键。

## 2. 时间语义

- 存储与协议一律 UTC、RFC 3339、毫秒精度；SQLite 保存整数 Unix milliseconds 或统一规范文本，脚手架节点二选一后全库一致，推荐整数毫秒。
- UI 按系统时区展示并在详情保留绝对时间；排序和 TTL 使用 UTC。
- 领域服务使用注入时钟，禁止直接散落系统时间调用。
- `created_at` 不修改；`updated_at` 随版本成功变更；Event 使用 `occurred_at`；Outbox 可另有 `published_at`。
- TTL 以控制面时钟为准；Adapter 时间只作诊断，不决定授权/过期。
- 同毫秒 Event 以 UUIDv7 + 本地序列稳定排序；游标包含 `(occurred_at,event_id)`。

## 3. 版本与并发

- 可变聚合初始 `version=1`，每次成功业务变更加 1。
- Command 必须传 `expected_version`，创建命令除外；不匹配返回 `CONCURRENCY_VERSION_CONFLICT` 并附当前版本，不自动覆盖。
- read model 版本不得冒充聚合版本；缓存键至少包含对象 ID + version。
- Binding 使用对象 `version` 和业务 `binding_generation`；同一 Role 的 generation 单调递增。
- Schema migration 版本与领域对象 version 分离。

## 4. 幂等规则

幂等记录键为 `(team_id, actor_fingerprint, command_name, idempotency_key)`，保存 payload hash、状态和首次结果引用。

| 情况 | 行为 |
|---|---|
| 同 key + 同 payload，首次仍处理中 | 返回处理中/等待同一结果，不启动第二次执行 |
| 同 key + 同 payload，首次成功 | 返回首次结果，不重复 Event/Run/Artifact |
| 同 key + 同 payload，首次稳定业务失败 | 返回同一稳定失败；可重试基础设施错误按命令策略处理 |
| 同 key + 不同 payload | `CONCURRENCY_IDEMPOTENCY_CONFLICT` |
| key 已超保留期 | 关键命令仍通过唯一约束/业务引用防重复；客户端不应复用 |

Run retry、Message retry、Approval decide、Binding confirm、Artifact publish 必须幂等。每次“新的业务尝试”使用新 key；网络重试复用原 key。

## 5. 状态与删除

- P0 核心对象无通用 DELETE 命令。
- Team 使用 `ACTIVE/PAUSED/ARCHIVED`；Role/Harness 使用 `ACTIVE/DEACTIVATED`；Task 使用 `CANCELLED`；Binding/Run/Message/Approval 保留终态。
- Event、Run、Binding、Review decision、DeliveryAttempt、Approval decision 不更新历史语义，只追加后续事实。
- 用户可修订 Message 草稿；一旦 QUEUED，正文冻结，修改需创建新 Message。
- 引用失效只更新 `validation_status`；不删除历史 URI/path/hash。
- 测试 fixture 可硬删除，但 repository production API 不暴露该能力。

## 6. 留存策略

| 数据 | P0 默认留存 | 归档后处理 |
|---|---|---|
| Team/Role/Task/Binding/Run/Event/Review | 随 Team 永久保留 | 只读、可导出；不自动清理 |
| Message/Delivery/Notification/Approval | 随 Team 保留 | 只读、可导出；Notification 展示可隐藏但事实保留 |
| Artifact 索引 | 随 Team 保留 | 外部文件缺失时保留 metadata/checksum |
| Adapter 诊断日志 | 14 天或 200 MB，先到者 | 轮转删除；不影响 Event/审计事实 |
| 长任务 stdout/stderr | 默认 14 天或每 Run 上限，超限截断并写 Artifact | 保留摘要、checksum 和截断标记 |
| 幂等记录 | 关键命令至少 30 天；Run/Message/Binding 永久唯一约束 | 归档 Team 内保留 |
| 本地备份 | 由 Owner 策略；P0 提供手动一致性备份 | 导出包含 schema/version/checksum |

任何未来“彻底删除/隐私擦除”需要独立 ADR，定义 Event 链、Artifact 外部位置与备份的处理方式。

## 7. URI 与引用

- Workspace 文件使用 Team Workspace 相对 POSIX 风格路径；显示时由平台适配本机分隔符。
- 禁止把无必要的用户绝对路径写入 Artifact/Event；执行工作区绝对路径仅在受限运行元数据中保存。
- Git 引用优先使用完整 commit SHA；branch 名只作辅助。
- 受控 URL 必须记录 scheme allowlist、可见范围和可选 checksum；P0 不自动抓取任意远程 URL。
- Markdown 锚点保存原始标题与规范化 anchor；验证结果含 resolved version。

## 8. Event/审计完整性

- Event 至少包含 `event_id/team_id/type/actor/subject/occurred_at/request_id/object_version/metadata/previous_hash/hash`。
- hash 以规范序列化后的受保护字段计算；payload 变更会使链校验失败。
- Event/Outbox 与业务变更同事务；通知失败不撤销 Event。
- 导出包含 schema version、首尾 hash、记录数和文件 checksum。
- hash chain 用于篡改检测，不替代数据库文件权限、备份和系统安全。

## 9. DEV-00-06 审核结论

ID、时间、版本、幂等、引用、归档和留存规则均能直接映射到后续数据字典与迁移；不存在以时间戳替代版本、以 Session ID 替代 fencing 或硬删除审计历史的歧义。

结论：**PASS**。
