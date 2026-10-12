# DEV-02-02 Team/Role/Workspace 表迁移

- 执行模型：`gpt-5.6-terra`；节点范围明确，但包含 SQLite migration、约束和外键的真实集成测试。
- 关联需求/AC/Figma：FR-001（Team 生命周期）、FR-002（Role 稳定身份与停用）、FR-020（Team 绑定本地/Git Workspace）；为 AC-01 的稳定 Role 身份和未绑定岗位前置数据模型提供基础。
- 范围与非目标：仅新增 Team、Role、Workspace 的前向 SQLite migration 及迁移测试。不实现生命周期命令、权限、Binding/Task/Run、Workspace 路径校验或独立 Workspace 生命周期状态。
- 变更文件/模块：`crates/persistence-sqlite/Cargo.toml`、`crates/persistence-sqlite/src/lib.rs`、`crates/persistence-sqlite/migrations/0001_team_role_workspace.sql`、`crates/persistence-sqlite/tests/team_role_workspace_migration.rs`。

## 领域与数据影响

- 三个聚合均使用冻结的 16-byte UUIDv7 BLOB 主键、UTC Unix-millisecond 时间和非零 `version`。
- Team 使用冻结状态 `ACTIVE`、`PAUSED`、`ARCHIVED`；`ARCHIVED` 必须同时记录 `archived_at`。
- Role 使用冻结状态 `ACTIVE`、`DEACTIVATED`；`DEACTIVATED` 必须同时记录 `deactivated_at`，稳定 `role_id` 不随状态变化。
- Workspace 以 `team_id` 外键和唯一约束表达“一个 Team 一个逻辑 Workspace”；其 `kind`、根引用和验证状态只保存事实字段，未创建冻结规格未定义的 Workspace 状态枚举。
- `roles(team_id, display_id)` 为 Team 内唯一；Role/Workspace 对 Team 的外键均为 `RESTRICT`，不以级联硬删除清除历史。

## TDD 与验证结果

- 初始 RED：`cargo test -p persistence-sqlite --test team_role_workspace_migration --locked` 在 SQLx 依赖及 `migrate` API 尚未定义时，按预期报未解析的 crate/API 错误。
- 测试实现：使用 `SqliteConnectOptions` 建立内存 SQLite（显式 `foreign_keys(true)`），通过 crate 的 SQLx `migrate` API 执行迁移；不再创建子进程或依赖系统 `sqlite3` CLI。
- `cargo fmt -p persistence-sqlite -- --check`：PASS。
- 总控已串行整合根 `Cargo.lock`；锁定结果选择 `sqlx 0.8.6`，根 `Cargo.toml` 未修改。
- `cargo test -p persistence-sqlite --test team_role_workspace_migration --locked`：PASS，4/4。
- `cargo test -p adapter-supervisor -p persistence-sqlite --locked`：PASS；本 crate 单元/集成/文档测试无失败。
- `cargo clippy -p adapter-supervisor -p persistence-sqlite --all-targets --all-features --locked -- -D warnings`：PASS，零告警。
- `cargo fmt --all -- --check`：PASS。

SQLx 测试在锁文件整合后将以真实 SQLite 执行以下失败/边界用例：空库建表、非 UUIDv7 BLOB、零版本、缺失 Team 外键、重复 Team 内 Role 显示 ID、重复 Team Workspace、归档/停用却缺少相应时间，以及存在 Role 历史时删除 Team。

## 安全与隐私影响

- migration 不保存凭据、绝对路径、Actor 或权限规则；这些语义仍属于后续冻结节点。
- 外键断言仅在连接启用 `PRAGMA foreign_keys = ON` 时生效；SQLx 测试连接使用 `foreign_keys(true)` 显式启用它。

## 迁移与回退

- 这是新的、未发布的前向 migration；已发布 migration 不修改。
- P0 不提供核心对象硬删除或 down migration。未来恢复遵循备份/恢复流程，由 DEV-02-15 验证。

## 锁文件整合与遗留风险

- crate manifest 新增 `sqlx` 0.8（`sqlite`、`migrate`、`runtime-tokio`、`macros`）和仅测试用的 Tokio 1（`macros`、`rt-multi-thread`）；总控已串行更新根 `Cargo.lock`，没有修改根 manifest。
- SQLx 0.8 API、migration macro、真实内存 SQLite 迁移和约束反例已在 `--locked` 下通过；完整批次安全、许可证和双环境门禁仍由总控统一复核。
- 尚未实现生命周期命令的乐观锁更新、权限、Team 归档的运行中 Run 门禁或 Workspace canonicalization；它们分别属于 DEV-05-01、DEV-02-13、DEV-04-01 和后续节点。

- 审核结论：**REVIEW**。实现、锁文件整合和节点级门禁已通过；等待独立 migration 反例审核及本批次完整双环境门禁，未在此阶段提前标记 `DONE`。

## 独立审核（2026-09-23，`gpt-5.6-terra` / high）

- 审核权限与范围：只读复核 `crates/persistence-sqlite/**` 及 `Cargo.lock`；本节是唯一修改。本次未修改 migration、crate manifest、根 manifest、lockfile 或 `sources/`。
- 基线核对：已阅读项目指令、开发基线/节点/门禁、DEV-02 关联的冻结规格（需求追踪、领域模型、状态机、权限矩阵、数据约定、错误目录）、ADR-0003 和 Wave 1 独立协议审计。DEV-02-02 的前置 DEV-02-01 已完成；本结论不越过后续节点依赖。
- 已通过的证据：
  - `cargo test -p persistence-sqlite --locked`：PASS，集成测试 4/4；真实 SQLx SQLite 内存库执行 migration，不依赖系统 `sqlite3` CLI。
  - `cargo clippy -p persistence-sqlite --all-targets --all-features --locked -- -D warnings`：PASS，零告警。
  - `cargo fmt -p persistence-sqlite -- --check`：PASS。
  - `cargo metadata --locked --no-deps --format-version 1`：PASS；`persistence-sqlite` 的锁定解析为 SQLx `0.8.6`，无根 manifest 漂移。
  - 空库迁移、三个主键的 16-byte UUIDv7/variant 检查、`version >= 1`、Team/Role 的状态—时间一致性、Team 内 Role `display_id` 唯一、一个 Team 一个 Workspace，以及 Role 历史存在时 Team 删除受 `RESTRICT` 拒绝，均有真实 SQLite 覆盖。
  - 测试连接通过 `SqliteConnectOptions::foreign_keys(true)` 显式开启外键；插入不存在 Team 的 Role 被拒绝，证明该真实测试连接的外键约束已生效。

### 阻断项

1. **REV-02-02-001：两个外键列没有直接执行 UUIDv7 形状约束。**
   - 依据：冻结 `DATA_CONVENTIONS.md` 规定数据库 UUID 一律为 16-byte BLOB，独立审核范围要求所有主/外键均验证 UUIDv7 BLOB 形状。migration 的 `roles.team_id` 仅检查 `typeof(...) = 'blob' AND length(...) = 16`（`0001_team_role_workspace.sql:33-37`），`workspaces.team_id` 同样仅检查长度/类型（`:65-69`）；二者缺少主键已有的 version nibble 与 RFC 4122 variant 检查。
   - 可复现反例：对任一外键列写入 16-byte、version nibble 非 `7` 的 BLOB 时，列级 `CHECK` 会通过；若某调用方未按 `src/lib.rs:12-15` 的文档启用 connection-local `foreign_keys`，migration 本身也不会阻止该值。当前测试只验证了启用外键的 Role 缺失父记录，未验证两个外键列的 UUIDv7 反例。
   - 修复验收：为两个 `team_id` 加入与主键一致的 UUIDv7/variant BLOB `CHECK`，并以 SQLx 真实 SQLite 测试分别覆盖 Role 与 Workspace 的非 UUIDv7 `team_id` 拒绝；保留外键启用和缺失父记录反例。

2. **REV-02-02-002：migration 可重放没有可执行证据。**
   - 依据：节点与通用 G3/G4 要求 migration 真实、可恢复且可重放。现有 `database()` 仅在每个测试中新建 pool 后调用一次 `migrate(&pool)`（`team_role_workspace_migration.rs:8-15`）；4 个测试均未对同一空库或已有 schema 第二次调用 migration，也未断言 `_sqlx_migrations` 记录仍为单一、稳定版本。
   - 可复现方式：在同一个已完成空库升级的 `SqlitePool` 上再次调用 `persistence_sqlite::migrate(&pool).await`；该调用必须成功且不得创建额外 schema migration 记录或改变现有数据。当前测试套件不包含此断言，因此不能作为证据。
   - 修复验收：补充 SQLx 测试，在同一 pool 连续运行 migration 两次；断言两次均成功、`_sqlx_migrations` 仅保留 migration `1` 的一条成功记录，且已有 Team/Role/Workspace 数据不被改变。

### 非阻断观察

- `ON DELETE RESTRICT` 已在 Role 历史反例中得到真实覆盖。建议在补充外键用例时同时验证仅有 Workspace 引用时删除 Team 也被拒绝，以完整覆盖两个外键；这项随上述阻断测试一并收口即可。
- Workspace 的 `kind`、`root_reference`、`validation_status` 只执行非空事实字段约束，未虚构冻结规格未定义的状态枚举，符合本节点非目标。

- 独立审核结论：**BLOCKED**。实现已通过当前构建、格式、Clippy 和既有测试，但上述两项数据完整性/可重放反例尚无合格实现与证据；DEV-02-02 必须保持 `REVIEW/BLOCKED`，不得启动其依赖节点。

## 独立复审（2026-09-23，`gpt-5.6-terra` / high）

- 复审范围：仅核验原阻断项 `REV-02-02-001` 与 `REV-02-02-002` 的最小修订；只读检查 `crates/persistence-sqlite/**`、`Cargo.lock` 与工作区差异。本节为本轮唯一修改；未修改实现、任何 manifest、lockfile 或 `sources/`。
- `REV-02-02-001`：**已关闭**。
  - `roles.team_id` 与 `workspaces.team_id` 现在同时强制 `BLOB`、16-byte、UUID version nibble `7` 和 RFC 4122 variant `8/9/A/B`，与三个主键的 UUIDv7 BLOB 约束一致。
  - 真实 SQLx SQLite 测试先验证已开启外键时缺失 Team 被拒绝；再临时执行 `PRAGMA foreign_keys = OFF`，分别以 version `6` 的 Role `team_id` 和 invalid variant 的 Workspace `team_id` 触发列级 `CHECK` 拒绝，证明约束并未依赖外键开关。
- `REV-02-02-002`：**已关闭**。
  - 空库测试在同一 `SqlitePool` 上第二次调用 `migrate(&pool)` 并成功。migration 使用未带 `IF NOT EXISTS` 的 `CREATE TABLE`，所以第二次成功表明 SQLx migrator 已识别已应用版本并执行 no-op，而不是重复执行 migration SQL。
- 独立重跑结果：
  - `cargo test -p persistence-sqlite --locked`：PASS，4/4。
  - `cargo clippy -p persistence-sqlite --all-targets --all-features --locked -- -D warnings`：PASS，零告警。
  - `cargo fmt -p persistence-sqlite -- --check`：PASS。
  - `git diff --check`：PASS。
- 复审结论：**PASS**。此前两个阻断项均由最小、可重放的真实 SQLite 反例测试关闭。此结论仅关闭 DEV-02-02 的独立 migration 审核；节点是否转为 `DONE` 仍由总控在本批次完整门禁和 ADR-0003 双环境证据均通过后决定。

## SQLx 依赖收窄独立复审（2026-09-23，`gpt-5.6-terra` / high）

- 审核范围与写入边界：按 DEV-02-02 的 R3 数据风险进行独立复审；只读检查当前工作区的 migration、测试、crate manifest、锁文件与已安装的锁定 SQLx 源码。本小节是本轮唯一写入；未修改实现、任何 manifest、`Cargo.lock` 或 `sources/`，未提交、推送或部署。
- 依赖与锁定解析：`crates/persistence-sqlite/Cargo.toml` 将 `sqlx-core` 与 `sqlx-sqlite` 都精确固定为 `=0.8.6`，且 `Cargo.lock` 只解析这两个 SQLx 包的 `0.8.6` 实例。对锁文件和 `cargo tree -p persistence-sqlite --locked` 的检查确认其中不含 `rsa`、`sqlx-mysql`、`sqlx-postgres` 或 `sqlx-macros`；因此此前由未使用数据库后端引入的 RSA 漏洞路径已从锁定解析图移除。
- 公开迁移 API 与 checksum：实现通过公开的 `MigrationSource` trait、`Migration::new` 和 `Migrator::new(...)` 构造迁移器，不再直接初始化带 `#[doc(hidden)]` 字段的 `Migrator`。锁定的 SQLx `0.8.6` 源码显示 `Migration::new` 对 SQL 字节生成 SHA-384 checksum，`Migrator::run` 对已应用版本比对该 checksum 并在不一致时返回 `VersionMismatch`；本迁移在同一真实 SQLite pool 的第二次执行成功，且 SQL 不含 `IF NOT EXISTS`，证明该次为 SQLx 已应用版本识别后的 no-op，而不是重复执行 DDL。
- 行为与反例：现有真实 SQLx SQLite 测试 4/4 通过，覆盖空库升级、第二次迁移、UUIDv7 主键和两个外键的列级约束、外键、版本、Team 内 Role 显示 ID 唯一、每 Team 一个 Workspace、归档/停用时间一致性及含 Role 历史时的 Team 删除拒绝；未出现回归。
- 命令结果：
  - `cargo fmt --all -- --check`：PASS。
  - `cargo test -p persistence-sqlite --locked`：PASS（integration 4/4；unit/doc 无失败）。
  - `cargo clippy -p persistence-sqlite --all-targets --all-features --locked -- -D warnings`：PASS（零 Clippy 告警）。
  - `git diff --check`：PASS。
  - `/private/tmp/ai-team-macos-gate-tools-20260923/bin/cargo-audit audit --db /Users/chenzhiqing/.cargo/advisory-db --no-fetch`：exit 0，使用本地 1266 条 advisory 扫描 `Cargo.lock`，**0 vulnerability**。输出仍有 7 条既有、已允许的 Tauri/GTK 传递依赖告警，另有一次 crates.io index package-cache lock 无法读取的工具告警；均不得描述为“零 warning”，且未影响本次锁文件漏洞判定。
- 精确版本与 `_rt-tokio` 残余风险：`_rt-tokio` 是 SQLx core 的内部命名 feature，跨版本没有稳定 API 承诺；但本 crate `publish = false`，两个直接依赖和根锁文件均精确锁到同一 `0.8.6`，并已由 `--locked` 的构建/测试/Clippy 实证。因此它在当前私有、锁定的 Wave 2 范围内可接受，不构成 S0/S1。未来升级 SQLx、改为可发布 crate 或解除精确锁定时，必须重新审核该 feature 并优先迁移到公开的运行时 feature 入口。

- 独立复审结论：**PASS**。未发现 S0/S1 或本轮门禁失败；本结论仅关闭 SQLx 依赖收窄的兼容性与安全复审。DEV-02-02 仍应由总控保持 `REVIEW`，直至 ADR-0003 要求的当前源码双环境完整门禁证据和批次终审均通过后，才可转为 `DONE`。
