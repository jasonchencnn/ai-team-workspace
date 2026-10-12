# DEV-02-03 Harness/Adapter/Binding 表迁移

- 执行模型：Gemini 3.8 Flash；依据 Human 批准的 Wave 2 逐节点执行计划与 ADR-0004。
- 关联需求/AC/Figma：FR-003（Harness 实例注册与配置）、FR-004（Role 与 Harness 动态绑定）、AC-01（岗位换绑与执行能力切换）。
- 范围与非目标：新增 HarnessInstance、Adapter、Binding 的前向 SQLite migration (`0002_harness_adapter_binding.sql`) 及约束测试。不实现生命周期命令、权限、Task/Run、Supervisor 或具体 Adapter 进程。
- 变更文件/模块：`crates/persistence-sqlite/src/lib.rs`、`crates/persistence-sqlite/migrations/0002_harness_adapter_binding.sql`、`crates/persistence-sqlite/tests/harness_adapter_binding_migration.rs`。

## 领域与数据影响

- 三个聚合均使用冻结的 16-byte UUIDv7 BLOB 主键、UTC Unix-millisecond 时间和非零 `version`。
- `harness_instances`：状态为 `ACTIVE` 或 `DISABLED`；`DISABLED` 时必须记录 `disabled_at`；`credential_reference` 保存 Keychain 引用，不存明文凭据。
- `adapters`：记录 `protocol_version`、`executable_path`、`manifest_json` 事实字段。
- `bindings`：
  - 状态集合为 `PENDING_CHECK`、`ACTIVE`、`DRAINING`、`REVOKED`、`FAILED`。
  - 单主绑定：同一 Role 在任意时刻最多只能有一个 `ACTIVE` 状态的主 Binding（SQLite 部分唯一索引 `bindings_single_active_per_role`）。
  - 代数单调：同一 Role 的 `binding_generation` 唯一（`bindings_role_generation_unique`）。
  - 历史有效区间不重叠：通过 SQLite 触发器（`bindings_no_overlapping_ranges_insert` 与 `bindings_no_overlapping_ranges_update`）强制同一 Role 的生效时间段 `[effective_from, effective_to]` 不得冲突。
  - 外键：对 `teams`、`roles`、`harness_instances`、`adapters` 均为 `ON UPDATE RESTRICT ON DELETE RESTRICT`，严禁级联物理删除历史。

## TDD 与验证结果

- 初始 RED：在迁移 2 未定义时，`cargo test -p persistence-sqlite --test harness_adapter_binding_migration --locked` 运行 4 个测试，全部因表未定义失败（`0 passed; 4 failed`）。
- 编写实现：编写 `0002_harness_adapter_binding.sql` 并在 `crates/persistence-sqlite/src/lib.rs` 中注册第 2 项迁移。
- 修复与 GREEN：
  - `cargo test -p persistence-sqlite --locked`：PASS，8/8（4 个 DEV-02-02 既有用例 + 4 个 DEV-02-03 新用例）。
  - 覆盖反例：空库升至 migration 2、二次迁移幂等、非 UUIDv7 主键拒绝、同一 Role 双 ACTIVE 拒绝、同一 Role 重复 generation 拒绝、历史有效区间重叠拒绝、存在 Binding 历史时删除 Role 或 Harness 被外键 RESTRICT 拒绝。
- 静态质量：
  - `cargo clippy -p persistence-sqlite --all-targets --all-features --locked -- -D warnings`：PASS，零告警。
  - `cargo fmt -p persistence-sqlite -- --check`：PASS。

## 阶段自验收结论

- 结论：**SELF_CHECK_PASS**。
- 证据路径：`crates/persistence-sqlite/tests/harness_adapter_binding_migration.rs`。
- 状态建议：`REVIEW`（待 Wave 2 全量验收门禁）。
