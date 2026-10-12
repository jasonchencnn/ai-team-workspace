# MM-R03｜限制旧 Run 的非法状态倒退 — 执行回传

- 关联节点：DEV-02-05（DEV-02-05 仍 `BLOCKED`）
- 审计条目：DA-03（P1，状态风险）
- 状态：**SELF_CHECK_BLOCKED**（本包自身验收全绿；因发现第 3 处**未登记**的版本计数断言失败而自判 BLOCKED，详见 §5.5 与 §9）
- 验收口径：**除 3 处版本计数断言外全绿**（已登记 2 处 + 本包新发现的 1 处）。**不得**表述为「全工作区通过」。

---

## 1. 实际模型、版本与环境

| 项 | 实际值 |
|---|---|
| 执行端 | Minimaxcode（本包实现者） |
| 模型 / 版本 | `MiniMax-M3.1-Flash-Preview`（会话启动时由端点回报；不猜测、不代填） |
| 工具链 | `cargo 1.98.0 (797e8a9bc 2026-08-05)` / `rustc 1.98.0 (88d9e12ae 2026-08-18)` |
| PATH | `/Users/chenzhiqing/.minimax/toolchain/bin` 前置 |
| 网络 | `CARGO_NET_OFFLINE=true`，全部 cargo 命令带 `--locked --offline`，**零下载、零新增依赖** |
| 平台 | macOS（darwin 27.0.0, arm64） |
| 仓库 | `/Users/chenzhiqing/Documents/MyVault/协作工作`，分支 `codex/wave-2-ci-validation` |
| 新增费用 | $0；无外部服务调用、无凭据/生产数据访问 |

继承的约 120 项未提交成果全程保留，未执行任何 `reset`/`checkout`/`clean`/`stash`。

未运行：在线 Mac Gate（21 项）、干净安装、最低 macOS 14 实机、真实 Harness/Keychain、备份恢复、性能与完整 A11y。理由：任务包明令 `--offline`，且这些均属 Codex 最终统一门禁范围。**离线通过不等于在线 Mac gate 通过。**

## 2. 输入与输出 SHA

HEAD：`1a14b8795e76fc373bef326eb5df96f9ac173f71`（与任务包一致）。

### 输入（改动前）

| 文件 | SHA-256 |
|---|---|
| `crates/persistence-sqlite/src/lib.rs` | `4611e7e940d986acd08a87a6c4d4a5f83c2ad8fa594adef8044118dc254f7aaa` |
| `crates/persistence-sqlite/migrations/0026_core_history_retention.sql` | `4f303dc55e80856cf5f0806aafcf128074060175c2e151ff492d3c69c7b3a856` |
| `crates/persistence-sqlite/migrations/0025_message_routing_team_integrity.sql` | `e76eabe95529f3bfb1d8c8ee6800ac23f5a1c5b6fca4e8f3a0074f28fc1af966` |
| `crates/persistence-sqlite/tests/run_lease_consistency_forward_migration.rs` | `54476cbaf2a6ab112b2df3d07aa18751cba56d95f15c1d4866da1e2e50fda75c` |
| 审计 RED 复现件 `audit_storage_boundaries.rs` | `6e2e1d2741527ea3644bdfc524ebe1c0e44016df88fa2951bae3f32c5811d8ef` |
| 审计种子 `audit_core_seed.sql` | `3295f48c0b0b511fe2e4d9dfd4d6b88c7a66d7f6811934b702e1813cd4d8b051` |
| 审计失败日志 `storage-boundaries-red-v2.log` | `110dc40e488093eadb7ff00082f1801c5eed4b0e7e04aacd0954abe4e347138d` |

### 输出（本包最终）

| 文件 | SHA-256 |
|---|---|
| `crates/persistence-sqlite/migrations/0027_run_transition_integrity.sql`（新增） | `58a689fcfe37689edc4ce5b565efc17215c177929dad2175f71f03b0d0308dce` |
| `crates/persistence-sqlite/src/lib.rs` | `fe3561c4a288170da34ec26fd5a9a55476c350d45dec6a96a7617ce3814e0463` |
| `crates/persistence-sqlite/tests/run_transition_integrity.rs`（新增） | `ba7da5da61646a12eea02bffac46456e831032f81464a053446b9fe5dbe8acc2` |
| `crates/persistence-sqlite/tests/run_lease_consistency_forward_migration.rs` | `54476cbaf2a6ab112b2df3d07aa18751cba56d95f15c1d4866da1e2e50fda75c` |

**`run_lease_consistency_forward_migration.rs` 与输入逐字节相同**（SHA 与输入一致）——该文件虽在本包写集内，**实测不需要任何修改**。写集授权不等于需要改动；这一点如实登记，避免 Codex 复核时误判为遗漏。`0001`～`0026` 全部未改，SQLx checksum 不漂移。

## 3. 逐文件修改说明

### 3.1 新增 `crates/persistence-sqlite/migrations/0027_run_transition_integrity.sql`

**1 个 trigger**：`runs_status_transition_integrity_update`（`BEFORE UPDATE OF status ON runs`），把 `STATE_MACHINES §3` 的 17 条有向边落成 `WHEN CASE OLD.status … END = 0`。

设计要点：

1. **写成 1 个 `CASE` 谓词而非 17 个 trigger**。逐边 trigger 无法一眼看出集合就是冻结表，且 17 份近似函数体是 17 个变异藏身点。`the_status_matrix_matches_the_frozen_edge_table` 对**全部 64 格**（非抽样）双向核对。
2. **自迁移放行**（`OLD.status = NEW.status`）。写回同状态不是迁移，拒绝它会砸掉「无状态变化的业务字段更新」这条合法路径。
3. **终态象限整块让给 `0012`**（`AND OLD.status IN ('QUEUED','STARTING','RUNNING','UNKNOWN')`）。这不是削弱：终态→活动态本来就由 `0012:142-145` 拒绝，且 `a_reviewed_run_keeps_its_verdict` 用**报错消息字符串**钉住「是 `0012` 拒绝的」。若新 trigger 也覆盖该象限，它会抢先触发并返回自己的消息，那条测试就不再证明 `runs_lease_review_consistency_update` 生效。**实测证据**：变异 M2 去掉这个限定后，`a_reviewed_run_keeps_its_verdict` 与 `a_terminal_run_cannot_be_brought_back_to_life` 立即转红。这与 MM-R02 在 `reviews` 上撞到的是同一类遮蔽，处置方式也相同——收窄新守卫，而不是改旧测试的期望。
4. **终态→终态（`SUCCEEDED → FAILED` 等 12 格）仍可写**，是**如实登记的开放缺口**，不隐瞒。理由同第 3 点：关闭它要么复用 `0012` 的消息（两个守卫无法被测试区分），要么改旧测试期望（本包明令禁止）。已由 `terminal_to_terminal_is_a_registered_open_gap` 用**断言把缺口钉成显式事实**——将来谁关闭它，这条用例会转红并自报「RESULT §10 已过期」，而不是静默消失。见 §10。
5. **不建升级守卫表**。本迁移约束的是**语句**而非行，每条历史 `runs` 行天然满足；加 `valid = 1` 探针正是 `ADVERSARIAL_REVIEW_PLAYBOOK` 规则 5 警告的恒真断言。升级保证改由 `upgrading_to_0027_preserves_every_existing_row`（逐表前后快照逐字节比对）与 `a_failed_upgrade_has_no_side_effects`（占用 trigger 名迫使迁移失败，验证整体回滚）直接证明。

### 3.2 `crates/persistence-sqlite/src/lib.rs`

仅两处，沿用 0024/0025/0026 既有风格：

- 新增 `const RUN_TRANSITION_INTEGRITY_SQL: &str = include_str!("../migrations/0027_run_transition_integrity.sql");`
- `Migration::new(...)` 多行列表**末尾**追加 `27`，描述 `"run transition integrity"`

未触碰 0025/0026 的常量与注册行（0025/0026 迁移文件本身 SHA 亦未变）。

### 3.3 新增 `crates/persistence-sqlite/tests/run_transition_integrity.rs`

15 个用例。搬运审计 RED 复现件 `existing_running_attempt_cannot_return_to_queued` 与 `audit_core_seed.sql` 种子（**只搬 R03 相关的这一个 + 种子**；同文件里 `draft_recipient_update_cannot_cross_team` 属 MM-R01、`retained_histories_cannot_be_physically_deleted` 属 MM-R02，**本包一个都没碰**）。

计分纪律：

- 每条负向断言 = **语句报错（且消息在白名单内）+ 行仍在且原值逐列不变**。外键拒绝、0 行更新、CHECK 冲突、`str.replace` 未命中的「假 no-op」都不计分。
- 种子逐表计数断言 + `pragma_foreign_key_check`（证明无拒绝来自 FK）。
- **装置双向自声明**：`the_new_guard_is_installed` 断言 `0027` trigger 在加固 schema 存在、**且在 `v26` 基线不存在**。
- **基线轮确实不同**：`the_baseline_schema_accepted_the_rollback_this_migration_refuses` 在真实 `0001..0026` schema 上证明审计那条非法边**当时确实成功**。
- 边集**从领域 `RUN_EDGES` 派生**，不手抄 8 个字符串字面量（照 `LEASE_BEARING_STATUSES` 的既有做法），避免两份列表各自漂移。

覆盖清单：

| 用例 | 作用 |
|---|---|
| `existing_running_attempt_cannot_return_to_queued` | 审计 RED 搬运版：合法 `UNKNOWN→RUNNING` 对照先行，再拒 `RUNNING→QUEUED`，并逐列核对行未动 |
| `no_status_can_be_walked_back_to_queued_or_starting` | 7 个源态 × `QUEUED`/`STARTING` 逐条 |
| `the_status_matrix_matches_the_frozen_edge_table` | **全部 64 格双向核对**（合法必须通过、非法必须拒绝） |
| `the_edge_set_is_the_frozen_one` | 边数 **17** + 每源出度 `QUEUED 2 / STARTING 5 / RUNNING 5 / UNKNOWN 5` |
| `the_legal_forward_edges_are_all_reachable` | 合法对照：`QUEUED→STARTING→RUNNING→SUCCEEDED` 顺序走通；`QUEUED→CANCELLED` |
| `a_status_preserving_update_is_not_a_transition` | 无状态变化的业务字段更新仍可用，且**逐列确认真落地** |
| `a_retry_appends_a_new_attempt_and_leaves_the_old_run_untouched` | 重试**追加** attempt 3，旧 Run 不被改写 |
| `the_new_guard_is_installed` | 双向 schema 断言（加固有 / `v26` 基线无） |
| `the_baseline_schema_accepted_the_rollback_this_migration_refuses` | 基线轮非法边**真的成功** |
| `the_pre_existing_terminal_resurrection_guard_is_not_shadowed` | `0012` 旧守卫未被遮蔽（消息仍是它的） |
| `terminal_to_terminal_is_a_registered_open_gap` | **把开放缺口钉成显式事实**（见 §3.1 第 4 点） |
| `creating_a_run_outside_queued_is_still_possible_and_that_is_measured` | 创建轴缺口**实测**登记（见 §8） |
| `upgrading_to_0027_preserves_every_existing_row` | 升级零副作用 + 升级后守卫生效 |
| `a_failed_upgrade_has_no_side_effects` | 失败路径：占用 trigger 名，验证整体回滚 |
| `this_package_owns_exactly_version_27` | 版本号归属（**用稳健写法**，不制造第 4 处同类断言） |

### 3.4 `crates/persistence-sqlite/tests/run_lease_consistency_forward_migration.rs`

**未修改（SHA 与输入一致）。** 该文件在本包写集内，但实测无需改动——见 §3.1 第 3 点，处置方式是收窄**新**守卫而非放宽**旧**测试断言。

## 4. 命令、退出码与完整日志路径

全部日志在 `docs/reviews/evidence/minimax-return/MM-R03/`。

| # | 命令 | exit | 日志 | 结论 |
|---|---|---|---|---|
| 1 | `cargo test -p persistence-sqlite --test run_transition_integrity --test run_lease_consistency_forward_migration --locked --offline`（**包验收命令**） | **0** | `acceptance-command.log` | 19 + 15 = **34 passed / 0 failed** |
| 2 | `cargo build -p persistence-sqlite --locked --offline` | **0** | `build.log` | 零错误零告警 |
| 3 | `cargo clippy -p persistence-sqlite --all-targets --locked --offline -- -D warnings` | **0** | `clippy.log` | 零告警 |
| 4 | `RUSTDOCFLAGS="-D warnings" cargo doc -p persistence-sqlite --no-deps --locked --offline` | **0** | `doc.log` | 严格 rustdoc 通过 |
| 5 | `rustfmt --edition 2024 --check <本包 3 个 .rs>` | **0** | `fmt.log` | 三个文件均 FMT-CLEAN |
| 6 | `cargo test -p persistence-sqlite --locked --offline --no-fail-fast`（完整 crate） | **101** | `full-suite.log` | **3 处版本计数断言失败**，见 §5.5 |
| 7 | 4 组变异验证 | 见 `mutation.log` | `mutation.log` | **4/4 全杀**，迁移文件已按字节还原 |
| — | 输入/输出哈希 | — | `input-sha256.txt`、`output-sha256.txt` | — |

`rustfmt` 仅对本包 3 个 `.rs` 逐个执行，**未执行 `cargo fmt --all`**，未写入任何无关文件（`git status --porcelain crates/persistence-sqlite/` 仅显示 `M src/lib.rs` 与两个未跟踪目录）。

## 5. 真实 RED → GREEN 与合法对照

### 5.1 基线矩阵（`0001..0026`，迁移落地前实测）

先在未注册 `0027` 的真实 schema 上打印完整 8×8 状态矩阵，得到：

| from \ to | QUEUED | STARTING | RUNNING | SUCCEEDED | FAILED | TIMED_OUT | CANCELLED | UNKNOWN |
|---|---|---|---|---|---|---|---|---|
| QUEUED | — | ACCEPT | ACCEPT | ACCEPT | ACCEPT | ACCEPT | ACCEPT | ACCEPT |
| STARTING | ACCEPT | — | ACCEPT | ACCEPT | ACCEPT | ACCEPT | ACCEPT | ACCEPT |
| RUNNING | ACCEPT | ACCEPT | — | ACCEPT | ACCEPT | ACCEPT | ACCEPT | ACCEPT |
| UNKNOWN | ACCEPT | ACCEPT | ACCEPT | ACCEPT | ACCEPT | ACCEPT | ACCEPT | ACCEPT |
| SUCCEEDED | no | no | no | — | ACCEPT | ACCEPT | ACCEPT | ACCEPT |
| FAILED | no | no | no | ACCEPT | — | ACCEPT | ACCEPT | ACCEPT |
| TIMED_OUT | no | no | no | ACCEPT | ACCEPT | — | ACCEPT | ACCEPT |
| CANCELLED | no | no | no | ACCEPT | ACCEPT | ACCEPT | — | ACCEPT |

`no` = 被 `0012:142-145` 拒绝。**56 个非对角格中 `0012` 只拒了 8 个**，含审计点名的 `RUNNING → QUEUED` 与 `UNKNOWN → QUEUED`。

### 5.2 RED（审计日志 `storage-boundaries-red-v2.log`）

```
an existing RUNNING attempt was reset to QUEUED
```

### 5.3 GREEN

包验收命令 34/34 通过，exit 0。基线→加固的差异由 `the_baseline_schema_accepted_the_rollback_this_migration_refuses` 在**真实 `0001..0026` schema 上**独立证明（该用例在加固 schema 上会读不到违规行，故必须跑基线）。

### 5.4 合法对照（证明表没被焊死）

| 路径 | 结果 |
|---|---|
| `RUNNING → UNKNOWN → RUNNING` 对账边（审计控制路径） | 允许，且逐列确认落地 |
| `QUEUED → STARTING → RUNNING → SUCCEEDED` 顺序走完 | 允许 |
| `QUEUED → CANCELLED`（未开始即取消） | 允许 |
| 无状态变化的业务字段更新（`version`/`failure_reason`） | 允许，且确认真落地 |
| 写回同一状态（`status = status`） | 允许（不是迁移） |
| 重试**追加** attempt 3 / token 3，旧 Run 不被改写 | 允许，旧行仍是 `FAILED` |
| `v26` 升级到 `0027`：9 张表逐表快照 | 升级前后**逐字节相同** |
| 升级后守卫生效 | 拒绝 `RUNNING → QUEUED` |
| `0012` 旧守卫（终态复活、attempt/token 不可改写、lease 身份） | 全部仍拒，消息未被遮蔽 |

### 5.5 完整 crate 套件口径（**3 处失败，不是 2 处**）

`full-suite.log`，全部为**纯版本计数断言**，均为 `left: 27`：

| # | 文件:行 | 断言 | 是否已登记 |
|---|---|---|---|
| 1 | `crates/persistence-sqlite/tests/notification_execution_integrity.rs:470` | `MAX(version) == 24` | ✅ 已登记（MM-COORD） |
| 2 | `crates/persistence-sqlite/tests/outbox_default_forward_migration.rs:146` | `MAX(version) == 24` | ✅ 已登记（MM-COORD） |
| 3 | **`crates/persistence-sqlite/tests/core_history_retention.rs:1274`** | **`COUNT(*) WHERE success=TRUE == 26`** | ❌ **本包新发现，未登记** |

### 5.6 关于第 3 处失败（必须登记，不得隐去）

第 3 处**不是**任务包与 `MM-COORD-migration-version-assertions.md` 登记的那 2 处之一，**断言形状也不同**：它不是 `MAX(version)`，而是 `COUNT(*) FROM _sqlx_migrations WHERE success=TRUE == 26`，位于 `core_history_retention.rs`（MM-R02 的测试文件，**不在本包写集内**）。

因此 `MM-COORD` §2「全仓扫描结论：`MAX(version)` 断言共 4 处，只有上述 2 处脆弱」**在本次实测中不再完整**——它按 `MAX(version)` 这一模式做扫描，漏掉了同类的脆弱写法。本次扫描（同样口径）结果：

- `MAX(version)` 断言：**4 处**（2 处脆弱「最新版本」型 + 2 处「回滚」型，后者保持绿）
- 另有 **1 处** `COUNT(*) WHERE success=TRUE == 26`（`core_history_retention.rs:1274`），语义同为「本包拥有的最新版本号」，同样脆弱

**处置**：本包**未修改**该文件、未改期望值、未加 `#[ignore]`。该文件不在本包写集内，且 `MM-COORD` 记录 Human 已裁决「保持写集不扩张，留给 Codex 最终统一门禁处理」——同一个协调缺口的第 3 个实例应交由同一裁决处理，故本包**自判 BLOCKED** 并回传，请 Codex 在签发写集时一并处理（建议同样改为稳健写法 `COUNT(*) WHERE version = 26 AND success = TRUE == 1`，与 `MM-COORD` §5 的建议同形）。

另 2 处 `MAX(version)` 回滚断言（期望 `22`、`23`）**保持绿**，已由完整套件运行确认（`incompatible_identity_upgrade_rolls_back_and_keeps_old_rows ... ok`）。

## 6. 变异验证（4/4 全杀）

`mutation.log`。每个变异体单独改写后重跑，随后按字节还原（末尾 `diff` 输出 `IDENTICAL`）：

| 变异体 | 改动 | 杀死情况 |
|---|---|---|
| **M1** | 整个 `WHEN` 改恒假（`WHEN 0 AND …`），守卫永不触发 | **4 个用例转红**：`existing_running_attempt_cannot_return_to_queued`、`no_status_can_be_walked_back_to_queued_or_starting`、`the_status_matrix_matches_the_frozen_edge_table`、`upgrading_to_0027_preserves_every_existing_row` |
| **M2** | 去掉终态象限让位条件（`AND OLD.status IN (…)`），守卫开始覆盖终态源 | **4 个用例转红**：本包 `terminal_to_terminal_is_a_registered_open_gap`、`the_pre_existing_terminal_resurrection_guard_is_not_shadowed`；**且 `run_lease_consistency_forward_migration.rs` 的 `a_reviewed_run_keeps_its_verdict`、`a_terminal_run_cannot_be_brought_back_to_life` 转红** ← 这是把「让位」写进迁移的**直接证据**，也证明该文件确实无需修改 |
| **M3** | 去掉自迁移豁免（`NEW.status IS NOT OLD.status`），同状态写回也被拒 | **1 个用例转红**：`a_status_preserving_update_is_not_a_transition` |
| **M4** | 从谓词里删掉一条冻结边（`UNKNOWN → CANCELLED`） | **1 个用例转红**：`the_status_matrix_matches_the_frozen_edge_table` |

**被删掉的冗余守卫：无。** 本包只交付 1 个 trigger，4 个变异体全部被杀死，因此不存在「改成恒假后测试仍全绿」的未被证明代码。MM-R02 曾因此删掉自己 4 个冗余守卫；本包在设计阶段就通过 §3.1 第 3 点的收窄避免了同类问题，并用 M2 反向证明该收窄是必需的而非可选的。

## 7. 变异验证抓到的「因错误原因而绿」并已修正

按包要求（MM-R01/R02 经验）如实登记 **5 处**，全部为**装置缺陷**，均已修正并重验：

1. **ID 长度错误导致守卫从未被测到**。首版为额外 Run 行手写 `...ab0008`、`...ac0000`、`...ad0000`、`...ae0000` 等 ID，实际长度 34～41 而非 32，被 `0001..0004` 的 `length(id) = 16` CHECK 拒绝——**每条用例都在第一步就失败，输出形状与真拦截完全同形**。修法：改为单一 `run_id(tag, n)` 生成器，并在生成器内 `assert` 长度 32、version nibble `7`、variant nibble ∈ `89AB`；该自检随后**真的拦下了一次**越界（ordinal 宽度算错，生成 34 位），这正是它存在的意义。
2. **`row_as_text` 忽略 WHERE 子句**（装置缺陷，no-op 伪装成「行未变」）。首版硬编码 `FROM runs` 且无 `WHERE`，于是所有「被拒的 UPDATE 没有移动行」断言读的是 attempt 1（种子里那行 `SUCCEEDED`），而受测语句改的是 attempt 2——**守卫真的移动了行也会被判过**。修法：`where_clause` 提升为必填参数，调用点逐个显式给出。
3. **多列读取静默只覆盖部分列**。`query_scalar` 对 `(String, i64, i64)` 无 `FromRow`，编译期报错暴露了它；但同源的 `SELECT a, b, c AS x` 别名只落在最后一列，是 MM-R02 已登记的同款陷阱。修法：统一走 `row_as_text`，逐列 `ifnull(cast(col as text),'~')||':'||` 拼成单值，并断言完整字符串（如 `"RUNNING:1:2:2"`），不只比第一列。
4. **`SELECT status FROM runs` 读到的是别的行**。种子已含 attempt 1 `SUCCEEDED`，未限定行的读取在合法的 `QUEUED → STARTING` 后仍报「行仍读作 SUCCEEDED」，一度被误判为守卫故障。修法：所有状态读取一律限定到受测行 id。
5. **`QUEUED → CANCELLED` 漏 `started_at` 而被 CHECK 拒绝**。手写 UPDATE 只给了 `completed_at`，被 `0004` 的「终态必须 `started_at` 非空」CHECK 拒掉——**该用例当时是在给 CHECK 记分**。修法：统一用 `target_timestamps(to)` 生成目标状态的时间戳对，并加注释说明 `CANCELLED` 也在 `0004` 的非空要求内。

另有 **2 处断言值本身错误**，一并登记：

6. **边数写成 16，实为 17**。我按 §3 ASCII 图以为 `UNKNOWN → FAILED/TIMED_OUT/CANCELLED` 一行会「去重成一条」，实际它们 `to` 不同，去重无法合并。这是 `ADVERSARIAL_REVIEW_PLAYBOOK` 规则 6（数字须跑计数命令、不得手算）的实例，现由 `the_edge_set_is_the_frozen_one` 从 `RUN_EDGES` 派生总数 **17** 并逐源断言出度 `[2,5,5,5]`。
7. **守卫遮蔽 `0012` 的报错消息**。首版触发器覆盖整个 8×8，导致 `a_reviewed_run_keeps_its_verdict` 转红（收到 `Run transition violation` 而非 `Run consistency violation`）。**该文件虽在本包写集内，但正确处置不是改它的期望**——那正是任务包禁止的「改预期保绿」。修法：收窄**新**守卫的 `WHEN`，把终态象限让回 `0012`；并用变异 M2 反向证明该收窄必需。

## 8. 创建轴（INSERT 入口）评估结论：登记，不关闭

`STATE_MACHINES §3` 只有一条创建行：`创建 → QUEUED`。`0012:72-108` 的 `runs_consistency_insert` **完全不约束 status**，只校验 tuple、`attempt = MAX(attempt)+1` 与递增 `fencing_token`。因此当前可以直接 `INSERT` 一个 `SUCCEEDED` 的 Run。

**评估结论：本包不新增 INSERT 侧 status 守卫。** 理由是实测而非假定——该守卫已写出来跑过：它会拒绝**本包新增文件之外**的既有 `crates/persistence-sqlite/tests/` 中全部非 `QUEUED` 的 Run 插入（`SUCCEEDED` 13 处、`RUNNING` 6 处、`FAILED` 1 处，分布在 **7 个**含字面量插入的文件：`artifact_reference_consistency_forward_migration.rs`、`artifact_reference_upgrade_migration.rs`、`core_history_retention.rs`、`event_outbox_consistency_forward_migration.rs`、`message_routing_team_integrity.rs`、`run_lease_consistency_forward_migration.rs`、`run_lease_parent_upgrade_migration.rs`；另有 `run_lease_consistency_forward_migration.rs` 经 `run_row()` 参数化构造插入），其中包含 `0013`/`0014`/`0020`/`0026` 的 fixture 与 `run_lease_parent_upgrade_migration.rs` 的升级 fixture，**只有 2 个文件在本包写集内**。

> 计数口径（`ADVERSARIAL_REVIEW_PLAYBOOK` 规则 6）：以上由
> `grep -rnoE "INSERT INTO runs VALUES\([^)]*'(QUEUED|STARTING|RUNNING|SUCCEEDED|FAILED|TIMED_OUT|CANCELLED|UNKNOWN)'" $(ls crates/persistence-sqlite/tests/*.rs | grep -v run_transition_integrity)`
> 得出，**刻意排除本包自己的新测试文件**——把它算进去会把数字从 13/6/1/7 抬到 14/6/1/8，而那 1 处新增恰是本包用来**证明缺口存在**的合法对照，不是受该守卫波及的既有 fixture。

这些 fixture 在冻结规格下是合法的，不属于本包可改的范围：

- §3 的创建行是关于**命令** `task.claim`／重试服务的陈述，那是应用层（K3/K4），不是持久化边界；
- `DATA_CONVENTIONS §5` 与 `DOMAIN_MODEL §5` 要求 Run 历史留存，一个「插入一条已结束 Run」的 fixture 表达的是**留存历史**，不是生产创建路径。

因此创建轴是**已登记的缺口**，不是遗漏。缺口本身由 `creating_a_run_outside_queued_is_still_possible_and_that_is_measured` 钉成显式断言（若将来被关闭，该用例会转红并要求同步更新本节）。正确关闭它需要本包无权作出的决定：要么重写这 7 个文件的 fixture（只有 2 个在写集内），要么在应用层给冻结创建行一个执行点（K3/K4 范畴）。**未加任何测试专用逃生开关。**

创建轴**已有**且保留的部分：`0012:97-104` 钉住 `attempt = MAX(attempt)+1` 与 `fencing_token` 严格递增（新 Run 必是最新世代）；本迁移使终态不可离开。二者合起来正是「重试新建 Run、不重置旧 Run」在存储边界的含义——**没有任何语句能把一个已存在的 Run 状态改到冻结表未列出的值**。

## 9. 升级、回滚与兼容性

- **升级零副作用**：`upgrading_to_0027_preserves_every_existing_row` 对 9 张表逐表快照（含逐行摘要），升级前后逐字节相同；`_sqlx_migrations` 有 `version=27 AND success=TRUE`；升级后守卫生效。
- **失败路径零副作用**：`a_failed_upgrade_has_no_side_effects` 占用 `0027` 创建的 trigger 名，迫使迁移失败，验证快照不变、无 `version=27` 记录、无自身残留 trigger。
- **回滚**：前向迁移，`0027` 无 down 路径。回退方式为回退二进制并保留含 `0027` 的数据库——`0027` 只增加触发器、不改任何数据，回退后行数据无损，只是防线消失。
- **兼容性**：不改表结构、不加列、不改默认值/约束、不加索引、不新增依赖；`0001`～`0026` 逐字节未改，SQLx checksum 不漂移。
- **既有测试兼容**：完整套件除 3 处版本计数断言外全绿（2 处已登记 + 1 处本包新发现，见 §5.6）。

## 10. 剩余风险与回退

1. **P1（本包外，本包新发现，未登记）**：`core_history_retention.rs:1274` 的 `COUNT(*) WHERE success=TRUE == 26` 是第 3 处脆弱版本计数断言，与 `MM-COORD` 已登记的 2 处同源。**该文件不在本包写集内**，且 Human 已裁决此类留给 Codex 最终统一门禁，故本包未动，请 Codex 一并签发写集处理。见 §5.6。
2. **P2（本包外，已如实登记，未关闭）**：**终态 → 终态**（`SUCCEEDED → FAILED` 及其 3 个同族）仍可写。§3 给终态无出边，故这是真实缺口。关闭它需要复用 `0012` 的报错消息（两守卫无法被测试区分）或修改 `a_reviewed_run_keeps_its_verdict` 的期望（本包禁止），两者都不是存储边界修复。已由 `terminal_to_terminal_is_a_registered_open_gap` 钉成显式失败-if-closed 断言。**建议归 DEV-02-05 的应用链批次（K3/K4）一并裁决。**
3. **P2（本包外，已如实登记，未关闭）**：**创建轴**（Run 可被直接 `INSERT` 为 `SUCCEEDED` 等非 `QUEUED` 状态）。评估过程与实测计数见 §8。需 K3/K4 的应用层执行点或跨这 7 个文件的 fixture 重写（见 §8 的口径与计数）。
4. **P2（边界声明）**：本包**只做数据库状态守卫**。**不得**据此宣称取消/重试/当前 fencing 的应用链已完成——那是 K3/K4 的条件工作，本轮未做。
5. **审计装置缺陷（沿用 MM-R02 的独立确认）**：审计 `audit_storage_boundaries.rs` 的 `artifacts` 分支 INSERT 只给 11 个值，而 `0005` 的 `artifacts` 表有 12 列（`git_commit_sha` 位于 `checksum_sha256` 与 `visibility` 之间）。**本包不需要 `artifacts`**，因此未复制该分支、未继承该缺陷，并在 `run_transition_integrity.rs` 的装置注释中写明该行为何缺席，以免与审计日志对照时误判。
6. **回退**：删除 `src/lib.rs` 的 `RUN_TRANSITION_INTEGRITY_SQL` 常量与 `Migration::new(27, …)` 注册行即可退回 R02 状态；`0027` 不改数据，回退无数据损失，仅防线消失。R01 的 `0025`、R02 的 `0026` 均不受影响。

## 11. 未运行项及理由

| 未运行 | 理由 |
|---|---|
| `cargo test --workspace` | 本包验收口径为 `persistence-sqlite`；其余 crate 不受 `0027` 影响，且受 DA-01/DA-04/DA-05/DA-06 各自任务包影响，交 Codex 统一门禁 |
| TS 生成一致性检查 | 本包只改 SQL/迁移注册，不改任何 Rust DTO 或 wire 字段，生成物无变化 |
| 在线 Mac Gate 21 项 / 干净安装 / 最低 macOS 14 实机 | `--offline` 约束；属 ADR-0005 门禁范围 |
| 真实 Harness / Keychain / 备份恢复 / 性能 / A11y | 本包无相关代码面 |
| `cargo audit` / `cargo-deny` 在线校验 | 离线缓存非当日新鲜；本包未新增依赖 |

## 12. SELF_CHECK_BLOCKED

**本包自身全部通过**：包验收命令 exit 0（34/34）；build / clippy（`-D warnings`）/ 严格 rustdoc / rustfmt 全 exit 0；4/4 变异体全杀且迁移文件按字节还原；升级与失败路径零副作用已实测；`0012` 旧守卫未被遮蔽已实测。

**判 BLOCKED 的唯一原因**：完整 crate 套件出现 **3 处**版本计数断言失败，其中第 3 处（`core_history_retention.rs:1274`）**不在 `MM-COORD` 已登记范围内，且不在本包写集内**。按任务包停止条件（「写集冲突」「无法证明正对照」）与「不擅自扩张写集、不改预期保绿」，本包**未修改**该文件、未改期望值、未加 `#[ignore]`，而是如实回传并请求 Codex 在最终统一门禁中一并处理（§5.6、§10.1）。

同一协调缺口（版本计数断言随新迁移漂移）的第 3 个实例已超出 `MM-COORD` §2 的扫描口径（它只扫了 `MAX(version)`），此事实本身建议 Codex 在统一门禁时一并复核扫描口径。

**本包不构成任何节点 DONE 判定，不启动依赖包，不解除任何节点 BLOCKED 状态，未提交、未推送、未部署。**

## 13. 相关回传与证据入口

- 任务包：`docs/coordination/task-packets/minimaxcode-2026-10-08/MM-R03.md`
- 版本断言登记：`docs/coordination/minimax-return/MM-COORD-migration-version-assertions.md`（本包 §5.6 请求扩充其登记范围至第 3 处）
- 本包证据：`docs/reviews/evidence/minimax-return/MM-R03/`
  （`acceptance-command.log`、`build.log`、`clippy.log`、`doc.log`、`fmt.log`、`full-suite.log`、`mutation.log`、`input-sha256.txt`、`output-sha256.txt`）
- 同批 R01：`docs/coordination/minimax-return/MM-R01-RESULT.md`
- 同批 R02：`docs/coordination/minimax-return/MM-R02-RESULT.md`