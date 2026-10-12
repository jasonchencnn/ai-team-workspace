# DEV-02-05 Run/Lease/Review 表迁移

- 执行模型：Gemini 3.8 Flash；依据 Human 批准的 Wave 2 逐节点执行计划与 ADR-0004。
- 关联需求/AC/Figma：FR-013（Run 生命周期管理）、FR-014（单 Task 排他执行租约 Lease 与 Fencing Token）、FR-015（Review 审核与验收意见）、AC-04（执行隔离与人工审核）。
- 范围与非目标：新增 Run、Lease、Review 的前向 SQLite migration (`0004_run_lease_review.sql`) 及约束测试。不实现生命周期应用用例、IPC 命令或具体 Adapter 进程。
- 变更文件/模块：`crates/persistence-sqlite/src/lib.rs`、`crates/persistence-sqlite/migrations/0004_run_lease_review.sql`、`crates/persistence-sqlite/tests/run_lease_review_migration.rs`。

## 领域与数据影响

- 遵循冻结规范 [STATE_MACHINES.md](file:///Users/chenzhiqing/Documents/MyVault/%E5%8D%8F%E4%BD%9C%E5%B7%A5%E4%BD%9C/docs/specs/STATE_MACHINES.md) §3 与 [DATA_CONVENTIONS.md](file:///Users/chenzhiqing/Documents/MyVault/%E5%8D%8F%E4%BD%9C%E5%B7%A5%E4%BD%9C/docs/specs/DATA_CONVENTIONS.md)。
- `runs`：
  - 16-byte UUIDv7 BLOB 主键与外键（`team_id`、`task_id`、`role_id`、`binding_id`、`harness_instance_id`）。
  - 单任务单活 Run 约束：部分唯一索引 `runs_single_active_run_per_task ON runs (task_id) WHERE status IN ('QUEUED', 'STARTING', 'RUNNING', 'UNKNOWN')`，确保 Task 任意时刻最多只有一个处于活动态的 Run。
  - 世代与尝试号唯一：`UNIQUE (task_id, attempt)`，`UNIQUE (task_id, fencing_token)`。
  - 终态与时间戳一致性 CHECK 约束。
- `leases`：
  - 主键为 `task_id`（每个 Task 只能有一份有效 Lease 记录，外键关联 `tasks` 与当前 `runs`）。
  - CHECK 约束 `expires_at >= acquired_at`。
- `reviews`：
  - 记录 `decision` (`APPROVED` / `CHANGES_REQUESTED`)。
  - CHECK 约束：当 `decision = 'CHANGES_REQUESTED'` 时必须提供非空 `comments`（对应领域规则 `REVIEW_COMMENT_REQUIRED`）。
  - 全部外键使用 `ON UPDATE RESTRICT ON DELETE RESTRICT`。

## TDD 与验证结果

- 初始 RED：在迁移 4 注册前，`cargo test -p persistence-sqlite --test run_lease_review_migration --locked` 运行 3 个测试全部失败（`0 passed; 3 failed`）。
- 编写实现：编写 `0004_run_lease_review.sql` 并在 `crates/persistence-sqlite/src/lib.rs` 中注册第 4 项迁移。
- 修复与 GREEN：
  - `cargo test -p persistence-sqlite --locked`：PASS，14/14（4+4+3+3）。
  - 覆盖反例：
    - 空库连续升级至 migration 4 且重跑迁移幂等；
    - 同一 Task 尝试插入两个并发活动态 Run（如一个 RUNNING、另一个 STARTING）被部分唯一索引拒绝；
    - 重复 attempt 或 fencing_token 被拒绝；
    - Lease 的 `expires_at < acquired_at` 被拒绝；
    - Review 决定为 `CHANGES_REQUESTED` 但缺失 `comments` 被 CHECK 约束拒绝；
    - 存在 Run 或 Lease 时删除 Task 被外键 `RESTRICT` 拦截拒绝。
- 静态质量：
  - `cargo clippy -p persistence-sqlite --all-targets --all-features --locked -- -D warnings`：PASS，零告警。
  - `cargo fmt -p persistence-sqlite -- --check`：PASS。

## 阶段自验收结论

- 结论：**SELF_CHECK_PASS**。
- 证据路径：`crates/persistence-sqlite/tests/run_lease_review_migration.rs`。
- 状态建议：`REVIEW`（待 Wave 2 全量验收门禁）。
