# DEV-02-09 Review: Notification, Policy, and Attention Item Migration (0008_notification_attention.sql)

## 1. 目标与实现范围
依据 `docs/specs/STATE_MACHINES.md` §6、`docs/specs/DOMAIN_MODEL.md` 与 `docs/development/DEVELOPMENT_NODES.md`，完成通知策略（Notification Policies）、面向人类的通知（Notifications）与待处理索引（Attention Items）的持久化设计与迁移。

- **创建表与严格约束**：
  - `notification_policies`：包含 UUIDv7 主键、`team_id` 外键约束、`name`、`event_type`、`channel`（`IN_APP`, `OS_SYSTEM`, `EMAIL`, `WEBHOOK`）、`severity`、去重窗口与 TTL；
  - `notifications`：
    - `recipient_human_id` 强制为 UUIDv7，明确 Notification 严格面向 Human，不经过 Role Binding；
    - 可选引用来源 Event（`source_event_id`）与 Message（`source_message_id`），外键均为 `RESTRICT`；
    - 状态机：`UNREAD`, `READ`, `DISMISSED`, `EXPIRED`；
    - **去重机制**：`CREATE UNIQUE INDEX notifications_active_dedupe ON notifications (team_id, recipient_human_id, dedupe_key) WHERE dedupe_key IS NOT NULL AND status IN ('UNREAD', 'READ')`，防止同类未读/已读通知在活跃窗口内刷屏。
  - `attention_items`：
    - 记录工作台待处理索引，支持 `attention_type`（`APPROVAL_REQUIRED`, `TASK_BLOCKED`, `RUN_FAILED`, `RUN_TIMED_OUT`, `MESSAGE_NEEDS_HUMAN`, `SECURITY_ALERT`）；
    - 状态机：`OPEN`, `RESOLVED`, `DISMISSED`；
    - **同源事项唯一去重**：`CREATE UNIQUE INDEX attention_items_unique_open ON attention_items (team_id, source_type, source_id, attention_type) WHERE status = 'OPEN'`；
    - **终态约束校验**：
      - `CHECK (status != 'DISMISSED' OR (dismiss_reason IS NOT NULL AND length(trim(dismiss_reason)) > 0))`；
      - `CHECK (status != 'RESOLVED' OR resolved_at IS NOT NULL)`；
      - `CHECK (status != 'DISMISSED' OR dismissed_at IS NOT NULL)`。

## 2. TDD 验证
- **RED 阶段**：编写包含 Human 接收人隔离、通知活跃去重、Attention 唯一 OPEN 约束与终态理由时间戳检查的测试 `crates/persistence-sqlite/tests/notification_attention_migration.rs`，在未注册前触发预期的 `no such table: attention_items` 失败。
- **GREEN 阶段**：在 `crates/persistence-sqlite/src/lib.rs` 中注册 `0008_notification_attention.sql`，测试全部通过：
  - `empty_database_upgrades_to_notification_attention_schema`: 幂等创建三张表。
  - `migration_enforces_notification_human_recipient_and_deduplication`: 相同 dedupe_key 在 UNREAD 状态被拦截，在标记为 DISMISSED 后允许重新生成。
  - `migration_enforces_attention_item_deduplication_and_lifecycle`: 同源对象相同 OPEN Attention 被唯一索引拦截；状态转为 RESOLVED 缺少时间戳被拦截；状态转为 DISMISSED 缺少原因被拦截；RESOLVED 后允许生成新的 OPEN Attention。
- **Command Output**:
  - `cargo test -p persistence-sqlite --test notification_attention_migration --locked`: 3 passed, 0 failed.
  - `cargo test -p persistence-sqlite --locked`: 28 passed, 0 failed.
  - `cargo clippy -p persistence-sqlite --all-targets --all-features --locked -- -D warnings`: 0 warnings, 0 errors.
  - `cargo fmt -p persistence-sqlite -- --check`: clean.

## 3. 验收结论
DEV-02-09 自验收通过 (`SELF_CHECK_PASS`)。
