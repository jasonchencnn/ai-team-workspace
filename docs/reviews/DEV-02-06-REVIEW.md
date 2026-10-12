# DEV-02-06 Review: Artifact & Reference Migration (0005_artifact_reference.sql)

## 1. 目标与实现范围
依据 `docs/specs/storage/SQLITE_SCHEMA.md` 与 `docs/specs/DOMAIN_MODEL.md`，完成产物追踪（Artifacts）与跨实体引用（References）数据表的迁移与完整约束保证。

- **创建表与索引**：
  - `artifacts`：记录任务产物元数据，包含 UUIDv7 主键及外键（`team_id`、`task_id`、`run_id`、`creator_role_id`），支持 `artifact_type` 枚举、URI、SHA-256、Git commit SHA、`visibility`（`PUBLIC`, `TEAM_INTERNAL`, `ROLE_RESTRICTED`）及版本。外键策略均为 `ON UPDATE RESTRICT ON DELETE RESTRICT`。
  - `references`：多对一主体跨系统引用，支持 `subject_type`（`TASK`, `RUN`, `ARTIFACT`, `MESSAGE`）、`reference_kind`（`WORKSPACE_PATH`, `HEADING_ANCHOR`, `COMMIT`, `EXTERNAL_URL`, `ARTIFACT_LINK`）与 `validation_status`（`VALID`, `INVALID`, `UNVERIFIED`, `STALE`），校验状态时间戳及外键。
  - 为两表的外键和主体查询建立了高性能索引：`artifacts_team_id`、`artifacts_task_id`、`artifacts_run_id`、`artifacts_creator_role_id`、`references_team_id`、`references_subject`。

## 2. TDD 验证
- **RED 阶段**：编写反例及测试用例，未注册迁移前测试构建与执行失败。
- **GREEN 阶段**：在 `crates/persistence-sqlite/src/lib.rs` 中注册 `0005_artifact_reference.sql`，测试全部通过：
  - `empty_database_upgrades_to_artifact_and_reference_schema`: 幂等迁移验证，成功创建两表。
  - `migration_enforces_artifact_traceability_and_visibility`: 验证非法类型拦截、SHA-256 格式校验、外键 `RESTRICT` 拦截删除。
  - `migration_enforces_reference_integrity`: 验证非法主体类型与验证状态拦截。
- **Command Output**:
  - `cargo test -p persistence-sqlite --test artifact_reference_migration --locked`: 3 passed, 0 failed.
  - `cargo test -p persistence-sqlite --locked`: 17 passed, 0 failed.
  - `cargo clippy -p persistence-sqlite --all-targets --all-features --locked -- -D warnings`: 0 warnings, 0 errors.
  - `cargo fmt -p persistence-sqlite -- --check`: clean.

## 3. 验收结论
DEV-02-06 自验收通过 (`SELF_CHECK_PASS`)。
