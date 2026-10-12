# DEV-02-08 Review: Thread, Message, and Delivery Attempt Migration (0007_thread_message_delivery.sql)

## 1. 目标与实现范围
依据 `docs/specs/STATE_MACHINES.md` §5、`docs/specs/DOMAIN_MODEL.md` 与 `docs/development/DEVELOPMENT_NODES.md`，完成结构化会话（Threads）、角色间消息（Messages）与投递尝试历史（Delivery Attempts）的持久化设计与迁移。

- **创建表与完整约束**：
  - `threads`：包含 UUIDv7 主键、`team_id` 外键约束、`subject`、`status`（`OPEN`, `CLOSED`, `ARCHIVED`）、版本与创建/更新时间戳。
  - `messages`：
    - `to_role_id` 强制存在（`NOT NULL` 并引用 `roles(id)`），明确 Message 严格面向 Role；
    - 支持 `sender_actor_type`（`ROLE`, `HUMAN`, `SYSTEM`），记录发送者 Binding 快照（`sender_binding_snapshot_id` 与 generation）；
    - 支持 `reply_to_message_id`，禁止自我引用（`reply_to_message_id != id`）；
    - **正文冻结约束**：在数据库引擎层创建触发器 `messages_freeze_body_after_queued`，非 `DRAFT` 状态（一旦进入 `QUEUED` 及后续）禁止修改 `body`；
    - **同会话回复约束**：创建触发器 `messages_reply_to_same_thread`，保证 `reply_to` 必须指向同一 `thread_id` 内的消息；
    - 支持幂等键（`idempotency_key` 唯一索引）与 TTL（`expires_at` 索引）。
  - `delivery_attempts`：
    - 记录可重试的投递尝试历史，包含 UUIDv7、`message_id`、`attempt_number`、目标角色/Binding 快照、`status`（`PENDING`, `IN_FLIGHT`, `SUCCEEDED`, `FAILED`, `ABANDONED`）、错误码与耗时；
    - 唯一序号索引：`CREATE UNIQUE INDEX delivery_attempts_message_attempt ON delivery_attempts (message_id, attempt_number)`；
    - **唯一活跃投递约束**：`CREATE UNIQUE INDEX delivery_attempts_single_active ON delivery_attempts (message_id) WHERE status IN ('PENDING', 'IN_FLIGHT')`，严防并发双重投递。

## 2. TDD 验证
- **RED 阶段**：编写包含正文冻结、会话一致性、唯一单活跃 attempt 与历史追溯的反例测试 `crates/persistence-sqlite/tests/thread_message_delivery_migration.rs`，在未注册前触发预期的 `no such table: threads` 失败。
- **GREEN 阶段**：在 `crates/persistence-sqlite/src/lib.rs` 中注册 `0007_thread_message_delivery.sql`，测试全部通过：
  - `empty_database_upgrades_to_thread_message_delivery_schema`: 幂等升级验证。
  - `migration_enforces_mandatory_to_role_and_body_freezing`: 验证 DRAFT 状态允许修改 body，进入 QUEUED 后修改 body 触发触发器异常拦截，状态更新正常执行。
  - `migration_enforces_reply_to_same_thread_and_self_ref`: 同 Thread 回复成功，跨 Thread 回复被触发器拦截，自我回复被 Check 约束拦截。
  - `migration_enforces_delivery_attempt_invariants`: 单活跃 attempt 约束生效（阻止两次并发 IN_FLIGHT），失败后新建 attempt 成功，多 attempt 历史完整保留。
- **Command Output**:
  - `cargo test -p persistence-sqlite --test thread_message_delivery_migration --locked`: 4 passed, 0 failed.
  - `cargo test -p persistence-sqlite --locked`: 25 passed, 0 failed.
  - `cargo clippy -p persistence-sqlite --all-targets --all-features --locked -- -D warnings`: 0 warnings, 0 errors.
  - `cargo fmt -p persistence-sqlite -- --check`: clean.

## 3. 验收结论
DEV-02-08 自验收通过 (`SELF_CHECK_PASS`)。
