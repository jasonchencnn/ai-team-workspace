# DEV-02-07 Review: Event, Outbox, and Consumption Deduplication Migration (0006_event_outbox.sql)

## 1. 目标与实现范围
依据 `docs/specs/DATA_CONVENTIONS.md` §8、`docs/specs/DOMAIN_MODEL.md` §6 与 `docs/development/DEVELOPMENT_NODES.md`，完成不可变事件日志（Events）、跨聚合 Outbox 队列（Outbox）及消费幂等去重（Event Consumptions）的持久化设计与迁移。

- **创建表与约束**：
  - `events`：包含 UUIDv7 主键（`id`）、`team_id`（外键 RESTRICT）、`event_type`、`actor_type`（`HUMAN`, `ROLE`, `SYSTEM`, `ADAPTER`）、`actor_id`（可选 UUIDv7）、`subject_type`、`subject_id`、`request_id`、`object_version`、JSON 载荷与元数据、`occurred_at`（UTC 毫秒）、`previous_hash` 及 `hash`（64 位大写十六进制 SHA-256）。
  - **追加写（Append-Only）约束**：在数据库引擎层创建 `BEFORE UPDATE` 与 `BEFORE DELETE` 触发器，抛出 `EVENTS_ARE_APPEND_ONLY` 中止任何篡改或删除历史事实的尝试。
  - `outbox`：记录事件的跨边界派发状态，包含 UUIDv7、`event_id` 外键、`destination`、JSON `payload`、`status`（`PENDING`, `PROCESSING`, `PUBLISHED`, `FAILED`, `DEAD_LETTER`）、重试计数与最大限制、`idempotency_key`、`created_at`、`scheduled_at` 及 `published_at`。包含 `(event_id, destination)` 唯一联合索引，严防重复分发。
  - `event_consumptions`：以 `(consumer_id, event_id)` 为联合主键，提供基于数据库唯一约束的无竞争事件消费去重与重放机制。
  - 高性能索引：`events_team_occurred`、`events_subject`、`outbox_team_id`、`outbox_pending_dispatch`、`event_consumptions_team_consumer`。

## 2. TDD 验证
- **RED 阶段**：编写包含追加写约束、原子提交、去重与重放的测试 `crates/persistence-sqlite/tests/event_outbox_migration.rs`，在未注册迁移前触发预期的 `no such table: events` 失败。
- **GREEN 阶段**：在 `crates/persistence-sqlite/src/lib.rs` 中注册 `0006_event_outbox.sql`，测试全部通过：
  - `empty_database_upgrades_to_event_and_outbox_schema`: 幂等迁移验证，成功创建 `events`, `outbox`, `event_consumptions`。
  - `migration_enforces_event_append_only_and_invariants`: 验证正常插入，验证 `UPDATE` / `DELETE` 触发器拦截，验证非法 `actor_type` 拦截。
  - `migration_enforces_outbox_atomic_commit_and_deduplication`: 事务内 Task 更新 + Event 写入 + Outbox 写入原子提交验证，唯一索引防同目标重复入队验证。
  - `migration_enforces_consumption_deduplication_and_replay`: 首次消费成功，同 consumer 重复消费唯一主键冲突拦截，删除消费记录后重放成功验证。
- **Command Output**:
  - `cargo test -p persistence-sqlite --test event_outbox_migration --locked`: 4 passed, 0 failed.
  - `cargo test -p persistence-sqlite --locked`: 21 passed, 0 failed.
  - `cargo clippy -p persistence-sqlite --all-targets --all-features --locked -- -D warnings`: 0 warnings, 0 errors.
  - `cargo fmt -p persistence-sqlite -- --check`: clean.

## 3. 验收结论
DEV-02-07 自验收通过 (`SELF_CHECK_PASS`)。
