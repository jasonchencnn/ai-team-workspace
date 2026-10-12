# MM-R01-RESULT｜修复 DRAFT/投递的 Team 一致性（DA-01）

- 关联节点：DEV-02-08（节点保持 `BLOCKED`，本包不构成晋级）
- 结论：**`SELF_CHECK_BLOCKED`**（实现与本包证据全绿；被一个**写集外**的既存测试阻断，见 §7）
- 自验收不等于正式节点 `DONE`；正式判定由 Codex 裁定。

## 1. 实际模型与执行环境

| 项目 | 值 |
|---|---|
| 执行端 | Minimaxcode（Worker 子会话 `mvs_7983e5123747480bbe64606d22e31653`） |
| 模型 | `MiniMax-M3.1-Flash-Preview`（会话运行时声明值，未猜测 M3；实际权重版本号执行端不可见） |
| Harness | MiniMax Code Desktop，本地分支会话 |
| 仓库 | `/Users/chenzhiqing/Documents/MyVault/协作工作` |
| HEAD | `1a14b8795e76fc373bef326eb5df96f9ac173f71`，分支 `codex/wave-2-ci-validation` |
| 平台 | darwin 27.0.0 arm64（macOS） |
| 工具链 | `cargo 1.98.0 (797e8a9bc)` / `rustc 1.98.0 (88d9e12ae)`，PATH 前置 `/Users/chenzhiqing/.minimax/toolchain/bin` |
| 新增费用 | $0。未联网、未下载、未调用外部服务、未访问生产凭据/数据、未提交/推送/部署 |
| 共享路径写入 | 仅本包写集与 `docs/reviews/evidence/minimax-return/MM-R01/` |

## 2. 输入与输出 SHA

任务包声明的审计起点输入 SHA 为
`8845a053b778b67aeac7a2138ebeaa16432781572cdb2232fa085c30816de37f`（299 个范围文件）。
**本包启动时重新固定了当前输入**，登记如下（`11-shas-and-scope.txt`）：

| 角色 | 值 |
|---|---|
| HEAD | `1a14b8795e76fc373bef326eb5df96f9ac173f71` |
| 输出（写集 4 文件合并 SHA-256） | `ae51bf1bb46eb32f890b82a27f859c36ec2353998527c81a1c24820087288725` |

逐文件 SHA-256：

| 文件 | SHA-256 |
|---|---|
| `migrations/0025_message_routing_team_integrity.sql` | `e76eabe95529f3bfb1d8c8ee6800ac23f5a1c5b6fca4e8f3a0074f28fc1af966` |
| `src/lib.rs` | `7a3fe2a0d12d22e936dc5da096d27c9ec9ae97a4925677799bf20620d18da026` |
| `tests/message_routing_team_integrity.rs` | `ef60264254b8799d5e024b68322f0491d0269f01f8c4e0c26bc44557a2e9c441` |
| `tests/message_delivery_integrity_forward_migration.rs` | `a80ab0e4b7dba5ce1e224b035a0a5c37e499c02ed26fe937f70fa8e3d5be68dd` |

未提交故无 Git tree SHA；上表为本包可复核的固定输入。

## 3. 逐文件修改说明

### 3.1 新增 `crates/persistence-sqlite/migrations/0025_message_routing_team_integrity.sql`

239 行，**6 个 trigger**（5 × `BEFORE UPDATE` + 1 × `BEFORE INSERT`，按 `CREATE TRIGGER` 块机械计数，不含注释行）+ 1 个升级守卫表。

| # | Trigger | 事件 | 补的是哪条规则的对偶 |
|---|---|---|---|
| 1 | `messages_team_consistency_update` | UPDATE `messages` | **DA-01 核心**。`messages_team_consistency_insert` 只有 INSERT 臂 |
| 2 | `messages_non_role_snapshot_rejected_update` | UPDATE `messages` | `messages_non_role_snapshot_rejected_insert` 只有 INSERT 臂 |
| 3 | `messages_sender_binding_snapshot_update` | UPDATE `messages` | `messages_sender_binding_snapshot_insert` 只有 INSERT 臂（0018 已覆盖 `DRAFT→QUEUED` 边） |
| 4 | `delivery_attempts_target_role_team_insert` | INSERT `delivery_attempts` | 对**真实 Role 表**直陈同 Team 约束 |
| 5 | `delivery_attempts_target_role_team_update` | UPDATE `delivery_attempts` | 同上，UPDATE 侧 |
| 6 | `delivery_attempts_target_binding_update` | UPDATE `delivery_attempts` | `delivery_attempts_target_binding_insert` 只有 INSERT 臂 |

升级守卫 `message_routing_team_upgrade_guard` 扫描三类脏行并以 `CHECK` 失败**原子拒绝**迁移事务：
跨 Team 的 message addressing；HUMAN/SYSTEM sender 携带 Role-Binding snapshot；attempt 的 target Role 不属于 attempt 的 Team（或其 Binding 不属于该 Team/该 Role）。

**守卫 CHECK 命名为 `message_routing_team_upgrade_guard_valid`**，这是与 0017/0023 的一处有意差异：SQLite 的报错形如 `CHECK constraint failed: <约束名>`，0017/0023 未命名 CHECK，其拒绝信息与任何其它 CHECK 失败无法区分。任务要求「保留诊断信息」，未命名的 CHECK 做不到这一点。

### 3.2 `crates/persistence-sqlite/src/lib.rs`（单写者）

仅 2 处增量，无其他改动：新增 `MESSAGE_ROUTING_TEAM_INTEGRITY_SQL` 常量（`include_str!`），并在迁移链末尾追加 `Migration::new(25, "message routing team integrity", ...)`。

### 3.3 新增 `crates/persistence-sqlite/tests/message_routing_team_integrity.rs`

6 个用例，逐条钉住「非法写被拒 **且** 行未被改动」与合法对照：

| 用例 | 作用 |
|---|---|
| `draft_recipient_update_cannot_cross_team` | 审计反例，原样承运；额外断言行值不变 |
| `valid_same_team_draft_can_queue_and_history_passes_foreign_keys` | 审计合法对照，原样承运 |
| `every_mutable_draft_routing_field_rejects_cross_team_and_keeps_the_row` | `to_role_id`/`thread_id`/`team_id`/`sender_binding_snapshot_id` 四个可变字段逐个跨 Team 写入失败且行不变；含同 Team 改派 + 发送的合法对照 |
| `a_draft_human_message_cannot_gain_a_role_binding_snapshot` | HUMAN sender 的 snapshot UPDATE 对偶 |
| `a_draft_cannot_be_misrouted_into_a_foreign_team_attempt` | DA-01 端到端危害：错误路由 → QUEUED → ROUTED → 跨 Team attempt |
| `an_unbound_target_may_still_be_queued_and_escalated` | **合法路径守卫**：无 Binding 的目标仍可 `QUEUED → NEEDS_HUMAN`，且不被强迫生成 attempt |

`database()` 每次连接显式 `foreign_keys(true)`，并对 9 张表做**逐表行数断言** + `pragma_foreign_key_check` 断言（防 no-op 装置）。

### 3.4 `crates/persistence-sqlite/tests/message_delivery_integrity_forward_migration.rs`（追加）

文件头注释改为说明 0025 的升级路径；追加 `V24` 基线（0001..0024 完整链）与 5 个用例：

| 用例 | 作用 |
|---|---|
| `the_0025_guards_exist_only_after_the_migration` | **双向** schema 断言：基线必须没有 6 个 trigger，全量必须有 6 个（防「基线偷偷加载正式迁移」） |
| `the_pre_0025_baseline_really_admits_the_cross_team_draft_retarget` | 证明缺口在 0001..0024 上真实可复现（否则升级断言无意义） |
| `a_misrouted_delivery_attempt_is_rejected_before_0025_installs_guards` | 脏历史原子拒绝 + 诊断可见 + 无 guard 安装 + 未回填假 Team |
| `a_non_role_sender_snapshot_is_rejected_before_0025_installs_guards` | 第二类脏行形状同上 |
| `a_clean_v24_history_upgrades_without_rewriting_anything` | 合法升级正对照：行逐字段不变、无行丢失、升级后仍能正常路由 |

## 4. 命令、exit code 与日志路径

全部日志在 `docs/reviews/evidence/minimax-return/MM-R01/`。

| # | 命令 | exit | 日志 |
|---|---|---:|---|
| 00 | `cargo test -p persistence-sqlite --locked --offline`（**修复前基线**） | 0 | `00-baseline-cargo-test.log` |
| 01 | `cargo test -p persistence-sqlite --test message_routing_team_integrity --locked --offline`（**RED**） | **101** | `01-RED-message-routing-team-integrity.log` |
| 02 | 同上（**GREEN**，加 0025 后） | 0 | `02-GREEN-message-routing-team-integrity.log` |
| 03 | `cargo test -p persistence-sqlite --test message_delivery_integrity_forward_migration --locked --offline` | 0 | `03-forward-migration.log` |
| 04 | 变异测试：把新 UPDATE Team 守卫改成 `WHEN 0 AND …` | **101**（预期杀死） | `04-mutation-disable-update-team-guard.log` |
| 05 | **验收命令**（两 test target 合并） | 0 | `05-acceptance-target-tests.log` |
| 06 | `cargo build -p persistence-sqlite --locked --offline` | 0 | `06-build.log` |
| 07 | `cargo clippy -p persistence-sqlite --all-targets --locked --offline -- -D warnings` | 0 | `07-clippy.log` |
| 08 | `RUSTDOCFLAGS="-D warnings" cargo doc -p persistence-sqlite --no-deps --locked --offline` | 0 | `08-rustdoc.log` |
| 09 | `cargo test -p persistence-sqlite --locked --offline --no-fail-fast` | **101** | `09-full-suite.log` |
| 10 | `rustfmt --edition 2024 --check <仅本包 3 个 .rs>` | 0 | `10-rustfmt-check.log` |
| 11 | SHA/写集范围固定 | — | `11-shas-and-scope.txt` |

未运行 `cargo fmt --all`；未运行 TS 检查（本次无 Rust→Schema/TS 变更，生成物不受影响）。

## 5. 真实 RED → GREEN 证据

### 5.1 RED（0025 尚不存在时）

`01-RED-…log`，exit 101，**4 失败 / 2 通过**。失败项与审计复现的断言逐字对应：

```
draft_recipient_update_cannot_cross_team ......... DRAFT accepted a recipient from another Team
every_mutable_draft_routing_field_… ............. DRAFT accepted recipient Role from another Team
a_draft_human_message_cannot_gain_a_role_binding_snapshot
                                               … DRAFT HUMAN message acquired a Role-Binding snapshot
a_draft_cannot_be_misrouted_into_a_foreign_team_attempt
                                               … DRAFT was retargeted to another Team's Role
--- 当时即通过的合法对照 ---
valid_same_team_draft_can_queue_and_history_passes_foreign_keys ... ok
an_unbound_target_may_still_be_queued_and_escalated ............... ok
```

### 5.2 GREEN（0025 生效后）

`02-GREEN-…log` / `05-acceptance-target-tests.log`，exit 0，`6 passed; 0 failed`。
两个合法对照**自始至终为绿**（RED 轮即通过），证明修复不是靠拒绝合法路径换来的。

### 5.3 变异测试（证明断言真在检验新守卫）

`04-mutation-…log`：仅把 `messages_team_consistency_update` 的 `WHEN` 改为 `WHEN 0 AND …`（等价于永久 no-op），
→ exit 101，`draft_recipient_update_cannot_cross_team`、`every_mutable_draft_routing_field_…`、
`a_draft_cannot_be_misrouted_into_a_foreign_team_attempt` 三条转红。
随后**已还原**（`0025` 现含 6 个 trigger，`grep -c 'WHEN 0 AND'` = 0）。

### 5.4 两次「装置故障伪装成真结果」的记录（依 ADVERSARIAL_REVIEW_PLAYBOOK §2 如实登记）

1. **变异测试还原后一度出现 3 条假红**：`mv` 还原使文件 mtime 早于变异构建产物，cargo 判定「无需重编」而复用**含 no-op 守卫的陈旧二进制**。该失败与真红**输出完全同形**。经 `stat` 核对 mtime 并 `touch` 重编后消失。这不是产品缺陷，是装置故障。
2. **两处反例最初「因错误原因而绿」**：最初写的 delivery-attempt 反例被 `0017` 的 message-match 子句提前挡下，与新守卫无关；`0017` 的 `delivery_attempts_target_binding_insert` 也要求 Binding 必须指名 target Role，故带 Binding 的跨 Team attempt 同样被提前挡下。已改为「不带 Binding 的纯路由 attempt」并改写为端到端链条探针。**若不核对，这三条会带着「已覆盖」的错误结论进复核。**

## 6. 升级 / 回滚 / 兼容性

- **升级**：纯前向迁移，0001～0024 零修改、checksum 不漂移（已用 mtime + SHA 证明仅 0025 为本次新写）。SQLx 以事务执行 DDL，任一失败整体回滚。
- **脏历史**：`0025` **原子拒绝**整条升级，不修复、不回填任何 Team。拒绝信息含约束名 `message_routing_team_upgrade_guard_valid`，可定位到具体迁移。
- **合法升级**：干净 v24 库升级后行逐字段不变、无行丢失、`pragma_foreign_key_check` 为 0、仍可正常创建投递 attempt（`03-forward-migration.log`）。
- **兼容语义**：DRAFT 仍可修订正文、可在**同 Team 内**改派（`DATA_CONVENTIONS §5`）；`QUEUED → NEEDS_HUMAN` 未受影响；未新增状态、错误码、权限或协议语义。
- **回滚**：SQLite 无 `DROP TRIGGER` 回滚脚本的前向迁移惯例，回退方式为回退代码到不注册 0025 的 `lib.rs`（见 §9）。旧库不会残留半套守卫，因为安装与拒绝都在同一事务内。

## 7. 未闭环项：`SELF_CHECK_BLOCKED` 的唯一原因

**这是写集冲突，我按停止条件停下，未自行绕过。**

新增 `0025` 使当前迁移链版本由 24 变为 25。两个**既有**测试把「当前链版本 = 24」硬编码为断言：

| 文件（**不在本包写集内**） | 行 | 断言 | 实际 |
|---|---:|---|---|
| `crates/persistence-sqlite/tests/notification_execution_integrity.rs` | 474 | `MAX(version) == 24` | 25 |
| `crates/persistence-sqlite/tests/outbox_default_forward_migration.rs` | 150 | `MAX(version) == 24` | 25 |

事实认定：
- 两条失败**纯粹是版本号计数**（`left: 25, right: 24`），两用例在此之前的**全部行为断言均已通过**；panic 行即计数行。
- 与 DA-01 语义无关：这是任何新增迁移都会触发的既有脆弱断言。
- 全量套件 `--no-fail-fast` 结果：**173 passed / 2 failed / 0 ignored**，失败的只有这 2 条；`message_routing_team_integrity` 6/6、`message_delivery_integrity_forward_migration` 15/15 全绿。

我没有修改这两个文件，理由有两条，任一条都足以阻止我动手：
1. 任务包写集是穷举清单，「未列文件一律只读」，且停止条件明列「写集冲突」与「需要更大写集」。
2. 验收要求明文禁止「**改预期值**来保绿」——把 `24` 改成 `25` 正是这条禁止的行为。

**给总控的最小提案（二选一，由总控裁定并串行整合）**：
- A（推荐）：授权我把这两个文件各改 1 行常量 `24 → 25`，作为本包唯一例外并在本文件登记；或
- B：由总控/其他包统一更新这两处，并在最终统一门禁时重跑全量。

在此之前，验收命令 05、build、clippy、rustdoc、rustfmt 均 exit 0；**唯一未绿的是全量套件，且失败原因已完全定位、与本包语义无关。**

## 8. 未运行项及理由

| 未运行 | 理由 |
|---|---|
| `cargo fmt --all` | 任务明令禁止；只对本包 3 个 `.rs` 单独 `rustfmt` |
| TS 生成/检查 | 本包无 Rust→Schema/TS 契约变更，生成物不受影响；且总控会跑最终统一门禁 |
| 完整 Mac 21 项门禁 / 干净安装 / 备份恢复 / 性能 / A11y | 属总控最终门禁范围，非本包范围 |
| 在线 Mac Gate、任何联网检查 | 强制 `--offline`，禁止联网与下载 |

## 9. 剩余风险与回退

| 风险 | 说明 / 缓解 |
|---|---|
| **0025 守卫可能过严** | `messages_team_consistency_update` 对**所有**状态生效，不只 DRAFT。非 DRAFT 时 addressing 已被 `0017` 冻结，故该 trigger 只是重申既有值；若存量存在 `0017` 未覆盖的行，会在升级阶段被原子拒绝而非静默通过。已由 `a_clean_v24_history_upgrades_without_rewriting_anything` 覆盖合法路径 |
| **守卫拦住同 Team 内合法改派** | 已由 `every_mutable_draft_routing_field_…` 的正对照覆盖：同 Team 改派 + 发送均成功 |
| **`message_routing_team_upgrade_guard` 的 attempt 分支不可独立触发** | 已查明：`0017` 强制 `message.to_role_id = attempt.target_role_id` 且 `message.team_id = attempt.team_id`，故在 0001..0024 基线上跨 Team 的 attempt 必然伴随跨 Team 的 message，实际触发的是 message 分支。**该分支是纵深防御，本包不声称它能被独立触发**；其 INSERT/UPDATE 实际行为由 `message_routing_team_integrity.rs` 断言 |
| **全量套件 2 条红** | 见 §7，非本包语义问题，待总控裁定 |
| **回退** | 从 `src/lib.rs` 移除 0025 的常量与 `Migration::new(25, …)` 两处即可回到 24 链；0025 文件保留不删。因安装与拒绝同事务，旧库不会残留半套守卫 |

## 10. 未触碰范围声明

- `sources/`、0001～0024 旧迁移、历史报告/evidence、生成 TS、根 `Cargo.toml`/`Cargo.lock`/workspace manifest、正式节点状态、ADR 接受状态：**均未修改**。
- 未新增依赖，未升级任何 crate，未联网，未提交/推送/部署，未解除任何节点 `BLOCKED`。
- 未 reset / checkout / clean / stash 等破坏工作树的命令；全部继承成果保留。
- 同一 `audit_storage_boundaries.rs` 中属于 MM-R03/MM-R02 的两个用例（`existing_running_attempt_cannot_return_to_queued`、`retained_histories_cannot_be_physically_deleted`）**未搬运、未修复、未改动**。
- 未通过关闭 `foreign_keys`、关闭守卫、测试专用逃生开关、`#[ignore]` 或放宽预期值来保绿；所有连接显式 `foreign_keys(true)`。