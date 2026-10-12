# MM-R02｜补齐核心历史留存与错误测试期望 — 执行回传

- 关联节点：DEV-02-05（关联 02-06/08/10）
- 审计条目：DA-02（P1，留存风险）
- 状态：**SELF_CHECK_PASS**（自验收通过；不等于节点 DONE，不启动依赖包）
- 验收口径：**除 2 处已登记的迁移版本计数断言外全绿**。**不得**表述为「全工作区通过」。

---

## 1. 实际模型、版本与环境

| 项 | 实际值 |
|---|---|
| 执行端 | Minimaxcode（本包实现者） |
| 模型 / 版本 | `MiniMax-M3.1-Flash-Preview`（会话启动时由端点回报；不猜测、不代填） |
| 工具链 | `cargo 1.98.0 (797e8a9bc 2026-08-05)` / `rustc 1.98.0 (88d9e12ae 2026-08-18)` |
| PATH | `/Users/chenzhiqing/.minimax/toolchain/bin` 前置 |
| 网络 | `CARGO_NET_OFFLINE=true`，全部命令带 `--locked --offline`，**零下载、零新增依赖** |
| 平台 | macOS（darwin 27.0.0, arm64） |
| 仓库 | `/Users/chenzhiqing/Documents/MyVault/协作工作`，分支 `codex/wave-2-ci-validation` |
| 新增费用 | $0；无外部服务调用、无凭据/生产数据访问 |

未运行：在线 Mac Gate（21 项）、干净安装、最低 macOS 14 实机、真实 Harness/Keychain、备份恢复、性能与完整 A11y。理由：任务包明令 `--offline`，且这些均属 Codex 最终统一门禁范围。**离线通过不等于在线 Mac gate 通过。**

## 2. 输入与输出 SHA

HEAD：`1a14b8795e76fc373bef326eb5df96f9ac173f71`（与任务包一致；继承约 120 项未提交成果全程保留，未执行任何 `reset`/`checkout`/`clean`/`stash`）。

### 输入（改动前）

| 文件 | SHA-256 |
|---|---|
| `crates/persistence-sqlite/src/lib.rs` | `7a3fe2a0d12d22e936dc5da096d27c9ec9ae97a4925677799bf20620d18da026` |
| `crates/persistence-sqlite/migrations/0025_message_routing_team_integrity.sql` | `e76eabe95529f3bfb1d8c8ee6800ac23f5a1c5b6fca4e8f3a0074f28fc1af966` |
| `crates/persistence-sqlite/tests/permission_approval_integrity_forward_migration.rs` | `ad63eb78febb4b6d6a7a8e77afe0a050b2bd6fccbd792db9fa43e837e7357123` |
| 审计 RED 复现件 `audit_storage_boundaries.rs` | `6e2e1d2741527ea3644bdfc524ebe1c0e44016df88fa2951bae3f32c5811d8ef` |
| 审计种子 `audit_core_seed.sql` | `3295f48c0b0b511fe2e4d9dfd4d6b88c7a66d7f6811934b702e1813cd4d8b051` |
| 审计失败日志 `storage-boundaries-red-v2.log` | `110dc40e488093eadb7ff00082f1801c5eed4b0e7e04aacd0954abe4e347138d` |

### 输出（本包最终）

| 文件 | SHA-256 |
|---|---|
| `crates/persistence-sqlite/migrations/0026_core_history_retention.sql`（新增） | `4f303dc55e80856cf5f0806aafcf128074060175c2e151ff492d3c69c7b3a856` |
| `crates/persistence-sqlite/src/lib.rs` | `4611e7e940d986acd08a87a6c4d4a5f83c2ad8fa594adef8044118dc254f7aaa` |
| `crates/persistence-sqlite/tests/core_history_retention.rs`（新增） | `8e5a2957e2287a330e635cbb3bd8622da45d64a0dc528465947cd2747bb167aa` |
| `crates/persistence-sqlite/tests/permission_approval_integrity_forward_migration.rs` | `570eac1d959771fd9c929da50f5165c210eaa57b15d5e52bd6e8ba03c522c7ec` |

`0025` 的注册行与 `0025` 迁移文件本身**逐字节未改**（SHA 与输入一致）。`0001`～`0025` 全部未改。

## 3. 逐文件修改说明

### 3.1 新增 `crates/persistence-sqlite/migrations/0026_core_history_retention.sql`

11 个 trigger，分三条轴。**所有 11 个都经过变异验证（§6），不存在未经证明的守卫。**

| 表 | 删除守卫 | 替换守卫 | 身份守卫 |
|---|---|---|---|
| `runs` | ✅ | —（`0012` 已覆盖） | ✅ `id, team_id` |
| `reviews` | ✅ | ✅ | ✅ `id` |
| `artifacts` | ✅ | ✅ | ✅ `id, team_id` |
| `approvals` | ✅ | —（`0020` 已覆盖） | —（`0020` 已覆盖） |
| `messages` | ✅ | —（`0017` 已覆盖） | ✅ `id, team_id` |

设计要点：

1. **删除守卫无条件**。`0013` 的 Reference guard 与 `0014` 的 Event subject guard **原样保留、未删未改**；`0026` 只补齐「无引用行」这一空档，不替换任何既有守卫。
2. **替换守卫只给 2 张表**，这是实测结论而非遗漏。理由写入迁移注释：`runs` 被 `0012 runs_consistency_insert` 拒绝（`OR REPLACE` 触发 `BEFORE INSERT` 时冲突行仍在，`attempt = MAX(attempt)+1` 无法成立，其 `(task_id, attempt)` 与 `(task_id, fencing_token)` 冲突臂同理）；`approvals` 被 `0020 approvals_replace_guard` 以**完全相同**的谓词拒绝；`messages` 被 `0017 messages_identity_replace_guard`（`id`）与 `messages_idempotency_replace_guard`（`(team_id, idempotency_key)`）拒绝。只有 `reviews` 与 `artifacts` 此前无覆盖。
3. **身份守卫只给 4 张表**。`approvals` 已被 `0020 approvals_request_immutable_update` 覆盖。`reviews` 只守 `id`：`0012 reviews_consistency_update` 已无条件拒绝 `team_id`/`task_id`/`run_id`/`reviewer_role_id`，在此重复会**遮蔽该守卫自己的报错消息**（见 §7 冲突记录）。
4. **不建升级守卫表**。`0025`/`0020` 需要是因为它们约束**已存在**的行；`0026` 只约束其生效后发出的语句，任何既有行天然满足，加一个 `valid = 1` 探针正是 `ADVERSARIAL_REVIEW_PLAYBOOK` 规则 5 警告的恒真断言。升级的正确保证是「什么都不改」，由 `upgrading_to_0026_preserves_every_existing_row` 与 `a_failed_upgrade_has_no_side_effects` 直接验证。
5. **Attention 明确不在本包集合内**，迁移注释写明依据审计 §4，不宣称已解决其删除问题。

### 3.2 `crates/persistence-sqlite/src/lib.rs`

仅两处，均沿用 0024/0025 既有风格：

- 新增 `const CORE_HISTORY_RETENTION_SQL: &str = include_str!("../migrations/0026_core_history_retention.sql");`
- `Migration::new(...)` 多行列表**末尾**追加 `26`，描述 `"core history retention"`

未触碰 0025 的常量与注册行。

### 3.3 新增 `crates/persistence-sqlite/tests/core_history_retention.rs`

19 个用例。从审计 RED 复现件搬运 `retained_histories_cannot_be_physically_deleted` 与种子，并按包的验收要求补齐三轴反例、合法对照、升级/失败路径与装置自检。

计分纪律（全部写入文件头，避免「因错误原因而绿」）：

- 每条负向断言 = **语句报错 + 行仍在且原值不变**。外键拒绝、0 行更新、CHECK 冲突都不计分。
- `table_contents` 逐列拼接全行值（不使用 `SELECT *`）。
- 种子自带逐表计数断言 + `pragma_foreign_key_check`。
- 装置自声明 schema：`the_new_guards_are_installed` **双向**断言（加固后必须存在 11 个、`v25` 基线必须不存在）。
- 基线轮与加固轮确实不同：`those_guards_block_on_the_pre_0026_schema_too` 在 `v25` schema 上证明既有守卫仍生效，且**证明反例本身在基线上真的 1→0**。

覆盖清单：

| 用例 | 作用 |
|---|---|
| `retained_histories_cannot_be_physically_deleted` | 审计 RED 搬运版：五表 DELETE 必须全部被拒且行存活 |
| `deleting_nothing_is_not_a_bypass_and_is_not_an_error` | 合法对照：0 行 DELETE 不报错，证明守卫没有过度触发 |
| `retained_histories_cannot_be_rewritten_by_insert_or_replace` | 替换轴：拒绝 + 全行内容逐列比对不变 |
| `each_replace_vector_is_refused_and_by_what` | 记录每条替换向量在基线/加固两侧各由谁拒绝（`each_replace_vector_is_refused_and_by_what` 的输出见 §6） |
| `replace_vectors_depend_on_the_shipped_recursive_triggers_setting` | 钉住 `recursive_triggers = 0`，说明替换轴为何需要显式守卫 |
| `replace_vectors_each_change_a_different_value` | 反例值确实不同于种子值，排除 no-op |
| `existing_reference_and_event_guards_still_apply` | **证明 0013/0014 守卫仍然生效**（7 组引用组合） |
| `those_guards_block_on_the_pre_0026_schema_too` | 在 `v25` 基线上证明既有守卫拦截 + 反例 1→0 |
| `reviews_had_no_pre_0026_guard_of_either_kind` | 钉住「reviews 两侧皆无守卫」这一事实 |
| `retained_history_identity_is_not_repointed` | 身份轴：五表 `id` 重指向全部被拒 |
| `pending_approval_is_cancelled_not_deleted` | PENDING 不可物理删除；取消走 `CANCELLED` + actor |
| `an_approved_decision_stays_approved_and_present` | 已决定审批不可复活、不可删除 |
| `the_tables_are_guarded_not_welded_shut` | 合法对照：Run 终结、重试追加新 attempt、三表可继续新增 |
| `the_new_guards_are_per_team_and_add_no_new_reach` | 跨 Team 寻址仍被拒 |
| `upgrading_to_0026_preserves_every_existing_row` | 升级零副作用 + 升级后守卫生效 |
| `a_failed_upgrade_has_no_side_effects` | 失败路径：占用最后一个 trigger 名，验证事务整体回滚 |
| `the_new_guards_are_installed` | 双向 schema 断言 |
| `this_package_owns_exactly_version_26` | 版本号归属 |
| `foreign_keys_are_on_and_the_seed_is_clean` | 证明无拒绝来自 FK |

### 3.4 `crates/persistence-sqlite/tests/permission_approval_integrity_forward_migration.rs`

**只改测试期望，不改旧迁移。** `replace_and_delete_cannot_destroy_audit_records` 原有断言：

```rust
// （原）A pending approval may still be withdrawn by a direct delete
assert!(query("DELETE FROM approvals WHERE status='PENDING'").execute(&pool).await.is_ok(), ...);
```

依据 `DATA_CONVENTIONS §5`（「P0 核心对象无通用 DELETE 命令」）与 `§6`（Approval「随 Team 保留」，不限于已决定态），该期望是错的。改为：

- 把 `delete a still-pending approval` 并入既有攻击表，断言 `is_err()`；
- 追加两条**行为**断言：拒绝后 `approvals` 总行数不变、`status='PENDING'` 行数不变；
- 注释写明区分：旧断言之所以通过，是因为 `0020 approvals_terminal_delete_guard` 的 `WHEN OLD.status != 'PENDING'` **故意**放行了这一态；新拒绝来自 `0026` 的无条件守卫。

**未改动** `v19_really_did_accept_terminal_state_at_creation`（它在 `v19` schema 上断言 delete 成功，是钉住历史回归的正确写法，0026 不影响）。

## 4. 命令、退出码与完整日志路径

全部日志在 `docs/reviews/evidence/minimax-return/MM-R02/`。

| # | 命令 | exit | 日志 | 结论 |
|---|---|---|---|---|
| 1 | `cargo test -p persistence-sqlite --test core_history_retention --locked --offline --no-fail-fast`（0026 落地前） | 101 | `red-before-0026.log` | **RED**：9 failed / 9 passed |
| 2 | `cargo test -p persistence-sqlite --test core_history_retention --locked --offline --no-fail-fast`（0026 落地后，中间态） | 101 | `green-after-0026.log` | 装置迭代记录 |
| 3 | `cargo test -p persistence-sqlite --test core_history_retention --test permission_approval_integrity_forward_migration --locked --offline`（**包验收命令**） | **0** | `acceptance-command.log` | 19 + 16 = **35 passed / 0 failed** |
| 4 | `cargo build -p persistence-sqlite --locked --offline` | **0** | `build.log` | 零错误零告警 |
| 5 | `cargo clippy -p persistence-sqlite --all-targets --locked --offline -- -D warnings` | **0** | `clippy.log` | 零告警 |
| 6 | `RUSTDOCFLAGS="-D warnings" cargo doc -p persistence-sqlite --no-deps --locked --offline` | **0** | `doc.log` | 严格 rustdoc 通过 |
| 7 | `rustfmt --edition 2024 --check <本包 3 个 .rs>` | **0** | `fmt.log` | 三个文件均 FMT-CLEAN |
| 8 | `cargo test -p persistence-sqlite --locked --offline --no-fail-fast`（完整 crate） | **101** | `full-suite.log` | **仅 2 处已登记失败**，见 §5 |
| 9 | 7 组变异验证 | 见 `mutation.log` | `mutation.log` | **7/7 全杀** |
| — | `input-sha256.txt` | — | `input-sha256.txt` | 输入哈希 |

`rustfmt` 仅对本包 3 个 `.rs` 逐个执行，**未执行 `cargo fmt --all`**，未写入任何无关文件。

## 5. 真实 RED → GREEN 与合法对照

### 5.1 RED（`red-before-0026.log`，0026 落地前，同一份测试文件）

```
DELETE was accepted for ["runs", "reviews", "artifacts", "approvals", "messages"]
INSERT OR REPLACE was accepted for ["reviews", "artifacts"]
a PENDING approval request was physically deleted
a retained row's identity was repointed in ["runs", "artifacts", "messages"]
```

五表 `DELETE` 全部 1→0，与 DA-02 记述一致。

**与审计 RED 输入的一处偏差（必须登记）**：审计提交的复现件 `audit_storage_boundaries.rs` 中，`artifacts` 分支的 INSERT 只给了 **11 个值**，而 `0005` 的 `artifacts` 表有 **12 列**（`git_commit_sha` 位于 `checksum_sha256` 与 `visibility` 之间），该语句在当前 schema 上必然报 `table artifacts has 12 columns but 11 values were supplied`。但 `storage-boundaries-red-v2.log` 却记录 `"artifacts"` 出现在被绕过名单中。

本端已核实：`main-repro/.../0005_artifact_reference.sql` 与仓库内 `0005` **逐字节相同**（`diff` 无输出），列数确实为 12。**结论：审计那份 RED 日志的 `artifacts` 一条在本仓库上不可复现，属审计装置缺陷（正是 PLAYBOOK 第 0 节「错误的输出和正确的输出长得一模一样」）。** 其余四表的向量结构正确、可复现。本包按正确列数重建了 `artifacts` 向量并实测确认 `artifacts` 同样可被物理删除（基线 1→0），故 DA-02 的**结论**成立，仅审计日志的**可复现性**在该条上不成立。请 Codex 复核时注意此处。

### 5.2 GREEN

包验收命令 35/35 通过，exit 0。

### 5.3 合法对照（证明表没被焊死）

| 路径 | 结果 |
|---|---|
| Run `RUNNING → TIMED_OUT → SUCCEEDED`（含 `completed_at`/`exit_code`） | 允许 |
| Run 重试**追加** attempt 3（新行），attempt 1/2 记录保留 | 允许，`runs` 行数 2→3 |
| 0 行 DELETE（不匹配任何行） | 成功、不报错、0 行受影响 |
| Message `DRAFT` 正文可修订 | 允许（DATA §5） |
| PENDING Approval → `CANCELLED` + `cancelled_by_actor_type/id` + `version` bump | 允许，且行留存 |
| 已决定 Approval → 复活为 PENDING | 拒绝 |
| reviews / artifacts / approvals 新增记录 | 允许 |
| 跨 Team 寻址审批 | 拒绝 |

### 5.4 既有守卫仍然有效的证明（要求 2）

- `existing_reference_and_event_guards_still_apply`：7 组「有 Reference 或 Event 引用」的删除全部被拒且行存活。
- `those_guards_block_on_the_pre_0026_schema_too`：**在 `v25` 基线 schema 上**同样证明既有守卫拦截，证明 0026 是叠加而非替换。

**并且**，007 项断言**刻意不**断言具体报错消息：同 timing 多个 `BEFORE DELETE` 触发器的执行顺序 SQLite 未定义，用消息区分守卫会得到脆弱结论。「到底是谁拦的」由基线轮单独回答。

### 5.5 完整 crate 套件口径

`full-suite.log`：**2 处失败，均为已登记项**，均为 `left: 26, right: 24`：

| 文件:行 | 断言 |
|---|---|
| `crates/persistence-sqlite/tests/notification_execution_integrity.rs:470` | `assert_eq!(MAX(version) …, 24)` |
| `crates/persistence-sqlite/tests/outbox_default_forward_migration.rs:146`（函数 `upgrade_preserves_all_rows_and_changes_only_the_column_default`） | 同上 |

登记见 `docs/coordination/minimax-return/MM-COORD-migration-version-assertions.md`（Human 2026-10-08 裁决：保持写集不扩张，留给 Codex 最终统一门禁处理）。**本包未修改这两个文件、未改期望值、未加 `#[ignore]`。**

另 2 处 `MAX(version)` 回滚断言（期望 `22`、`23`）**保持绿**，已由完整套件运行确认。

## 6. 变异验证（7/7 全杀）

`mutation.log`。每个守卫单独改成恒假（`WHEN 0` / `WHEN 0 AND …`）后重跑：

| 变异体 | 杀死情况 |
|---|---|
| M1：五个删除守卫全部 `WHEN 0` | **3 个用例转红**（`retained_histories_cannot_be_physically_deleted`、`pending_approval_is_cancelled_not_deleted`、`upgrading_to_0026_preserves_every_existing_row`） |
| M2：`reviews` 替换守卫 `WHEN 0 AND …` | **2 个用例转红** |
| M2：`artifacts` 替换守卫 `WHEN 0 AND …` | **2 个用例转红** |
| M3：`runs` 身份守卫 `WHEN 0` | **1 个用例转红** |
| M3：`reviews` 身份守卫 `WHEN 0` | **1 个用例转红** |
| M3：`artifacts` 身份守卫 `WHEN 0` | **1 个用例转红** |
| M3：`messages` 身份守卫 `WHEN 0` | **1 个用例转红** |

### 变异验证抓到的「因错误原因而绿」并已修正

按包要求（MM-R01 经验）如实登记 **3 处**，均已重写断言后重新验证：

1. **外键冒充留存守卫**。首版用种子 Run `…0007` 做 `runs` 删除探针，但 `reviews`/`artifacts` 都指向它，`ON DELETE RESTRICT` 直接拒绝——测试因错误原因而绿。**修法**：新增一条谁都不指向的 `LONE_RUN`（attempt 2、无 lease、无 review/artifact），删除/替换/身份三条轴的 `runs` 探针全部改用它，并在种子里加断言证明它确实无人指向。

2. **`table_contents` 只读了第一列**（装置缺陷）。原写法 `SELECT a, b, c AS row_text` 被 SQLite 解析为多列、别名只落在最后一列，于是「行是否被改写」的比较**只覆盖第一列**，把 5 条替换向量全部误报为「未改写」。**修法**：改为逐列 `ifnull(cast(col as text),'~')||':'||` 拼接成单值。修正后实测确认 `reviews`、`artifacts` 在基线上**真的被改写**（`APPROVED → CHANGES_REQUESTED/reversed/9`，`uri/checksum/version` 被换）。

3. **`approvals` 替换向量列数错误**。沿用了 0020 测试的 16 值写法，而 `0021` 追加了 3 列（`permission_id`、`cancelled_by_actor_type`、`cancelled_by_actor_id`），实际为 19 列，语句报 `table approvals has 19 columns but 16 values were supplied`——被拒原因是装置错误而非守卫。**修法**：改用显式列名。

修正后额外发现并**删除**了 4 个无守卫价值的 trigger（`runs`/`approvals`/`messages` 替换守卫、`approvals` 身份守卫）：它们在被改成恒假后测试**仍然全绿**，属于未经证明的代码。删除理由已逐条写入迁移注释。`each_replace_vector_is_refused_and_by_what` 固化了「基线侧由谁拒绝」的实测记录。

## 7. 升级、回滚与兼容性

- **升级零副作用**：`upgrading_to_0026_preserves_every_existing_row` 对 12 张表逐表快照，升级前后逐字节相同；`_sqlx_migrations` 记录 `version=26 AND success=TRUE`；升级后守卫在**存量数据库**上同样生效。
- **失败路径零副作用**：`a_failed_upgrade_has_no_side_effects` 占用 `0026` 创建的**最后一个** trigger 名，迫使迁移失败，验证同事务内先创建的 10 个 trigger 全部回滚、数据快照不变、`_sqlx_migrations` 无 26 记录。
- **回滚**：前向迁移，`0026` 无 down 路径；回退方式为回退二进制并保留含 0026 的数据库（0026 只增加触发器，不改数据，回退后行数据无损，只是防线消失）。
- **兼容性**：不改表结构、不加列、不改默认值、不加索引、不新增依赖；既有 25 条迁移逐字节未改，SQLx checksum 不漂移。
- **既有测试兼容**：完整套件除已登记 2 处外全绿。

### 期间发现并已就地解决的一处越界（未扩大写集）

首版给 `reviews` 加了 `BEFORE UPDATE OF id, team_id`，导致 `run_lease_consistency_forward_migration.rs:76` 转红：该用例用**报错消息白名单**断言「Review 换 Team 必须由 `0012 reviews_consistency_update` 拒绝」，我的守卫抢先触发并返回了不同消息。该文件**不在本包写集内**。

处置：把 `reviews` 身份守卫收窄为 `BEFORE UPDATE OF id`（`0012` 已无条件拒绝其余四列），既保住唯一无覆盖的 `id` 轴（变异验证确认仍可杀），又不再遮蔽既有守卫的消息。未修改写集外任何文件。

## 8. 未运行项及理由

| 未运行 | 理由 |
|---|---|
| `cargo test --workspace` | 本包验收口径为 `persistence-sqlite`；其余 crate 不受 0026 影响，且受 DA-01/DA-03/DA-04/DA-05/DA-06 各自任务包影响，交 Codex 统一门禁 |
| 在线 Mac Gate 21 项 / 干净安装 / 最低 macOS 14 实机 | `--offline` 约束；属 ADR-0005 门禁范围 |
| 真实 Harness / Keychain / 备份恢复 / 性能 / A11y | 本包无相关代码面 |
| `cargo audit` / `cargo-deny` 在线校验 | 离线缓存非当日新鲜；本包未新增依赖 |

## 9. SELF_CHECK_PASS

依据：包验收命令 exit 0（35/35）；build/clippy/strict-rustdoc/rustfmt 全 exit 0；完整 crate 套件除 2 处**已登记**版本计数断言外全绿；7/7 变异体全杀；既有 `0013`/`0014` 守卫有效性有独立证明；升级与失败路径零副作用已实测。

**本包不构成任何节点 DONE 判定，不启动依赖包，不解除任何节点 BLOCKED 状态，未提交、未推送、未部署。**

## 10. 剩余风险与回退

1. **P1（本包外，已登记待 Codex 裁决）**：`"references"` 表在 `0001..0025` 中**没有任何 `BEFORE DELETE` 守卫**，而 `DATA_CONVENTIONS §5` 规定引用失效只更新 `validation_status`、不删除历史 URI/path/hash。属独立发现，超出本包五表写集，未擅自处理。**它不重开 DA-02**：有了 0026 后，五表历史无论 `"references"` 行是否存活都被保留。
2. **P2**：`runs` 的 `team_id` 身份守卫虽未被本包测试直接探测（探测会被自身外键拒绝，不计分），但与已验证可杀的 `id` 臂共用同一 trigger 谓词，可信度依赖同一条 `BEFORE UPDATE OF id, team_id` 语句。
3. **审计装置缺陷**：§5.1 记录的 `artifacts` 向量列数问题使审计 RED 日志该条不可复现。建议 Codex 复核 DA-02 时以本包实测的基线 1→0 为准。
4. **已登记待 Codex 统一处理**：`notification_execution_integrity.rs:470` 与 `outbox_default_forward_migration.rs:146` 的版本计数断言，0027 落地后将变为 `left: 27, right: 24`。
5. **回退**：删除 `0026` 的常量与注册行即可退回 R01 状态；0026 不改数据，回退无数据损失，仅防线消失。R01 的 `0025` 不受影响。

## 11. 相关回传与证据入口

- 任务包：`docs/coordination/task-packets/minimaxcode-2026-10-08/MM-R02.md`
- 版本断言登记：`docs/coordination/minimax-return/MM-COORD-migration-version-assertions.md`
- 本包证据：`docs/reviews/evidence/minimax-return/MM-R02/`
  （`red-before-0026.log`、`acceptance-command.log`、`build.log`、`clippy.log`、`doc.log`、`fmt.log`、`full-suite.log`、`mutation.log`、`input-sha256.txt`）
- 同包 R01：`docs/coordination/minimax-return/MM-R01-RESULT.md`