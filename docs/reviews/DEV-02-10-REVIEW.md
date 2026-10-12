# DEV-02-10 Review: Permission Policy and Human Approval Migration (0009_permission_approval.sql)

## 1. 目标与实现范围
依据 `docs/specs/PERMISSION_MATRIX.md`、`docs/specs/DOMAIN_MODEL.md` 与 `docs/specs/STATE_MACHINES.md` §7，完成权限策略（Permissions）与人类审批（Approvals）的持久化设计与迁移。

- **创建表与严格约束**：
  - `permissions`：包含 UUIDv7 主键、`team_id` 外键约束、可选 `role_id`（支持团队级策略或角色特化）、`resource_type`、`action`、`scope_pattern`、`effect`（`ALLOW`, `DENY`）与 `is_human_only` 标记。
  - `approvals`：
    - 支持高风险操作审批流程：`PENDING → APPROVED / REJECTED / EXPIRED / CANCELLED`；
    - **禁止自批约束（Self-approval ban）**：
      `CONSTRAINT approvals_no_self_approval CHECK (decided_by_human_id IS NULL OR requester_human_id IS NULL OR decided_by_human_id != requester_human_id)`，在数据库引擎层面禁止申请人自批；
    - **决策元数据完整性约束**：
      `CONSTRAINT approvals_decision_requires_human CHECK (status NOT IN ('APPROVED', 'REJECTED') OR (decided_by_human_id IS NOT NULL AND decided_at IS NOT NULL))`；
    - **驳回理由必填约束**：
      `CONSTRAINT approvals_rejection_requires_notes CHECK (status != 'REJECTED' OR (decision_notes IS NOT NULL AND length(trim(decision_notes)) > 0))`；
    - 索引：`approvals_team_status`、`approvals_pending_expiry`。

## 2. TDD 验证
- **RED 阶段**：编写包含禁止自批、驳回理由必填、批准元数据检查的测试 `crates/persistence-sqlite/tests/permission_approval_migration.rs`，在未注册前触发预期的 `no such table: approvals` 失败。
- **GREEN 阶段**：在 `crates/persistence-sqlite/src/lib.rs` 中注册 `0009_permission_approval.sql`，测试全部通过：
  - `empty_database_upgrades_to_permission_approval_schema`: 幂等创建两张表。
  - `migration_enforces_permission_rules_and_default_deny`: 校验合法权限与非法动作/效果拦截。
  - `migration_enforces_approval_lifecycle_and_self_approval_ban`: 验证 Human 审批成功；自批行为被 check 约束拦截；REJECTED 缺少驳回说明被拦截。
- **Command Output**:
  - `cargo test -p persistence-sqlite --test permission_approval_migration --locked`: 3 passed, 0 failed.
  - `cargo test -p persistence-sqlite --locked`: 31 passed, 0 failed.
  - `cargo clippy -p persistence-sqlite --all-targets --all-features --locked -- -D warnings`: 0 warnings, 0 errors.
  - `cargo fmt -p persistence-sqlite -- --check`: clean.

## 3. 验收结论
DEV-02-10 自验收通过 (`SELF_CHECK_PASS`)。
