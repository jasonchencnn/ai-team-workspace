# DEV-02-04 Task/Dependency 表迁移

- 执行模型：Gemini 3.8 Flash；依据 Human 批准的 Wave 2 逐节点执行计划与 ADR-0004。
- 关联需求/AC/Figma：FR-010（Task 创建与分配）、FR-011（Task 状态流转）、FR-012（Task 有向依赖约束）、AC-03（Task 看板与状态迁移）。
- 范围与非目标：新增 Task 与 TaskDependency 的前向 SQLite migration (`0003_task_dependency.sql`) 及约束测试。不实现生命周期应用用例、IPC 命令、Run 调度或前端展示。
- 变更文件/模块：`crates/persistence-sqlite/src/lib.rs`、`crates/persistence-sqlite/migrations/0003_task_dependency.sql`、`crates/persistence-sqlite/tests/task_dependency_migration.rs`。

## 领域与数据影响

- 遵循冻结规范 [STATE_MACHINES.md](file:///Users/chenzhiqing/Documents/MyVault/%E5%8D%8F%E4%BD%9C%E5%B7%A5%E4%BD%9C/docs/specs/STATE_MACHINES.md) §2 与 [DATA_CONVENTIONS.md](file:///Users/chenzhiqing/Documents/MyVault/%E5%8D%8F%E4%BD%9C%E5%B7%A5%E4%BD%9C/docs/specs/DATA_CONVENTIONS.md)。
- `tasks`：
  - 主键为 16-byte UUIDv7 BLOB，外键 `team_id` 与 `owner_role_id` 均为 16-byte UUIDv7 BLOB。
  - 冻结状态：`TODO`、`READY`、`DOING`、`BLOCKED`、`REVIEW`、`DONE`、`CANCELLED`。明确禁止虚构 `FAILED` 状态（失败归属 Run）。
  - `tasks(team_id, display_id)` 唯一索引，保证 Team 内显示标识唯一。
  - 终态一致性 CHECK 约束：`CANCELLED` 必须有 `cancelled_at`；`DONE` 必须有 `completed_at`。
- `task_dependencies`：
  - 复合主键 `(task_id, depends_on_task_id)`。
  - CHECK 约束：`task_id != depends_on_task_id`，禁止自依赖。
  - SQLite 触发器 `task_dependencies_no_direct_cycle_insert`，禁止直接双节点环路（若 A 依赖 B，则 B 依赖 A 在插入时抛出 `CIRCULAR_TASK_DEPENDENCY`）。
  - 外键：对 `teams` 与 `tasks` 均为 `ON UPDATE RESTRICT ON DELETE RESTRICT`，被依赖的任务受外键保护无法被物理删除。

## TDD 与验证结果

- 初始 RED：在迁移 3 注册前，`cargo test -p persistence-sqlite --test task_dependency_migration --locked` 运行 3 个测试全部失败（`0 passed; 3 failed`）。
- 编写实现：编写 `0003_task_dependency.sql` 并在 `crates/persistence-sqlite/src/lib.rs` 中注册第 3 项迁移。
- 修复与 GREEN：
  - `cargo test -p persistence-sqlite --locked`：PASS，11/11（02-02 4/4 + 02-03 4/4 + 02-04 3/3）。
  - 覆盖反例：
    - 空库连续升级至 migration 3 且重跑迁移幂等；
    - 同一 Team 内重复 `display_id` 被拒绝；
    - 非法状态（如虚构的 `FAILED`）被 CHECK 约束拦截；
    - `DONE` 缺失 `completed_at`、`CANCELLED` 缺失 `cancelled_at` 被拒绝；
    - 任务自依赖（`task_id = depends_on_task_id`）被 CHECK 拦截；
    - 双节点直接环路（A 依赖 B 后 B 依赖 A）被触发器拦截并抛出错误；
    - 存在依赖关系时被依赖的任务被外键 `RESTRICT` 拒绝删除。
- 静态质量：
  - `cargo clippy -p persistence-sqlite --all-targets --all-features --locked -- -D warnings`：PASS，零告警。
  - `cargo fmt -p persistence-sqlite -- --check`：PASS。

## 阶段自验收结论

- 结论：**SELF_CHECK_PASS**。
- 证据路径：`crates/persistence-sqlite/tests/task_dependency_migration.rs`。
- 状态建议：`REVIEW`（待 Wave 2 全量验收门禁）。
