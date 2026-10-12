# FC-02 RESULT — 六对象 DELETE/REPLACE 留存（MR-03）

> 执行端：Minimaxcode 子代理（Worker）。**实际模型/版本由执行端回填，不代填。**
> 本包只覆盖 FC-02 的穷举写集；不构成 DONE，正式准入仍由 Codex 独立复核。

## 1. 模型、环境与实际执行身份

| 项 | 值 |
|---|---|
| 实际模型 | `MiniMax-M3.1-Flash-Preview`（运行时自报，模型标识见会话 system context） |
| 执行端 | Minimax Code 子会话（`agent: worker`），非总控、非 Codex |
| 主机 / 平台 | aarch64-apple-darwin，macOS 27.0.0 arm64 |
| rustc | 1.98.0 (88d9e12ae 2026-08-18)，LLVM 22.1.8 |
| cargo | 1.98.0 (797e8a9bc 2026-08-05) |
| 工具链固定 | 仓库 `rust-toolchain.toml` = `1.98.0`，profile `minimal` |
| 依赖模式 | 全部 `--locked --offline`，`CARGO_NET_OFFLINE=true`；未联网、未下载、未新增依赖 |
| 费用 | $0 |
| HEAD | `1a14b8795e76fc373bef326eb5df96f9ac173f71`，共享 dirty worktree |
| 环境证据 | `docs/reviews/evidence/minimax-followup/FC-02/environment.txt` |

**上一轮中断说明**：总控已核验本次启动时 `0028`、测试文件、`src/lib.rs` 登记均不存在（`ls crates/persistence-sqlite/migrations/` 止于 `0027`，`ls tests/` 无 `retained_entity_integrity.rs`），本包从勘查直接开始，未清理任何残留。

## 2. 输入 / 输出 SHA-256

输入基线取自 `docs/reviews/evidence/minimax-return-audit-2026-10-08/main/input-before.json`（299 条）。
比对结果见 `evidence/minimax-followup/FC-02/input-before-compare.txt`：

- 287 条未变；12 条相对基线已变，**全部继承自 FC-01**（其中 persistence-sqlite 仅 `tests/notification_execution_integrity.rs` 与 `tests/outbox_default_forward_migration.rs` 两个文件）。
- **旧迁移零漂移**：逐个校验 `migrations/0001..0027` 共 25 个文件，与 `input-before.json` 字节一致（`MISSING/CHANGED` 均为空）。
- `src/lib.rs` 在本包之前的基线 SHA 为 `fe3561c4a288170da34ec26fd5a9a55476c350d45dec6a96a7617ce3814e0463`。

本包产出（`evidence/minimax-followup/FC-02/output-sha256.txt`）：

| 文件 | SHA-256 |
|---|---|
| `crates/persistence-sqlite/migrations/0028_retained_entity_integrity.sql`（新增） | `0c3b348a21814564d1785e53a04267385fee027bd95675b24c245ad627d21dba` |
| `crates/persistence-sqlite/src/lib.rs`（追加 0028 登记） | `4a06dfa1d19622635d4bfe6856ae140667a28bbdfa12270667689c6903784da3` |
| `crates/persistence-sqlite/tests/retained_entity_integrity.rs`（新增） | `8398411236d5a0751287ee53fe27d1476ed3b5edea82e6463923f35d6234506c` |

变异副本源码基线（`mutation-source-baseline-sha256.txt`）与上表三项一致，证明变异期间共享源码未被改动。

## 3. 逐文件修改

| 文件 | 动作 | 说明 |
|---|---|---|
| `crates/persistence-sqlite/migrations/0028_retained_entity_integrity.sql` | **新增** | 18 个触发器，六对象逐表留存 |
| `crates/persistence-sqlite/src/lib.rs` | **追加 9 行** | `RETAINED_ENTITY_INTEGRITY_SQL` 常量 + `Migration::new(28, "retained entity integrity", …)`，追加到列表**末尾**（27 之后）。未改动任何既有登记行 |
| `crates/persistence-sqlite/tests/retained_entity_integrity.rs` | **新增** | 18 个测试，含负向/正向/竞争/失败注入/升级/装置自检 |
| `docs/coordination/minimax-followup/FC-02-RESULT.md` | **新增** | 本文件 |
| `docs/reviews/evidence/minimax-followup/FC-02/` | **新增** | 全部证据 |

**未触碰**：任何 0001–0027 旧迁移、其他测试文件、生产代码、历史 evidence、根 `Cargo.toml`/`Cargo.lock`、正式节点状态、ADR。

## 4. 逐表处置

### 4.1 前置判定：既有守卫是"条件式"的，不是不存在

普查说这些表 `1 -> 0`，容易被误读成"这六张表没有保护"。逐表枚举既有触发器后的真实情况：

| 表 | 既有 DELETE 守卫 | 条件 |
|---|---|---|
| `teams` | `0014 teams_event_subject_delete` | 仅当有 Event 以 TEAM 为 subject |
| `roles` | `0014 roles_event_subject_delete` + `0015 roles_event_actor_snapshot_delete` | 仅当有 Event subject/actor/ROLE_BINDING |
| `tasks` | `0013 tasks_referenced_subject_delete` + `0014 tasks_event_subject_delete` | 仅当有 Reference 或 Event |
| `bindings` | `0014 bindings_event_subject_delete` + `0015 …snapshot_delete` | 仅当有 Event |
| `references` | **无任何 `BEFORE DELETE`** | — |
| `delivery_attempts` | **无任何 `BEFORE DELETE`** | — |

所以叶子行确实可删；`OR REPLACE` 另需独立 arm，因为 SQLite 的 `OR REPLACE` 是"隐藏删除 + 插入"，在出厂 `PRAGMA recursive_triggers = 0` 下**不触发**该表的 `BEFORE DELETE`（本测试显式断言 `recursive_triggers = 0`）。

### 4.2 新增守卫一览（18 个）

| 表 | DELETE 留存 | REPLACE guard | 身份不可变 UPDATE |
|---|---|---|---|
| `teams` | `teams_retained_entity_retention_delete` | `teams_retained_entity_replace_guard`（仅主键） | `UPDATE OF id` |
| `roles` | `roles_retained_entity_retention_delete` | `roles_retained_entity_replace_guard`（主键 **+** `(team_id, display_id)`） | `UPDATE OF id` |
| `tasks` | `tasks_retained_entity_retention_delete` | `tasks_retained_entity_replace_guard`（主键 **+** `(team_id, display_id)`） | `UPDATE OF id` |
| `bindings` | `bindings_retained_entity_retention_delete` | `bindings_retained_entity_replace_guard`（主键 **+** `(role_id, binding_generation)`） | `UPDATE OF id` |
| `references` | `references_retained_entity_retention_delete` | `references_retained_entity_replace_guard`（仅主键） | `UPDATE OF id, target_uri, checksum_sha256` |
| `delivery_attempts` | `delivery_attempts_retained_entity_retention_delete` | `delivery_attempts_retained_entity_active_slot_replace_guard`（部分唯一索引单活跃位） | `UPDATE OF id` |

### 4.3 逐表冻结依据与保留的既有守卫

**Team** — 依据 DATA §6（Team「随 Team 永久保留」）、DOMAIN §5（稳定 ID 关联不级联硬删除）、DOMAIN §2（Team 身份稳定）。
`teams` 只有主键一个唯一键，故"换不同 ID 撞唯一业务键"这一形态在该表**不存在**，只测同 ID；如实说明而不是为它虚构第二个唯一键。保留 `0014` 三件套。

**Role** — 依据 DATA §6、DOMAIN §2、DOMAIN §5（Role 停用后保留 Task/Thread/Artifact/Binding 历史）。
`roles` 有 `UNIQUE (team_id, display_id)`，**换 ID 撞这条业务键**同样会删掉原行，`0014`/`0015` 的 replace guard 只有在 Event join 成立时才拦——这正是本包补的关键 arm。
`team_id` **不加守卫**：`0010/0011/0012/0013` 在存在 Binding/Task/Run/Review/Reference 时已拒绝，且 `run_lease_consistency_forward_migration.rs:1452` **断言 `0012` 的确切报文** `Run consistency violation`；在此重述会遮蔽另一包已证明的诊断。
保留 `0014`/`0015` 的 subject/actor/snapshot 三件套、`0010`–`0013` 的 team_id 守卫、`0023` 的 `roles_notification_human_identity_update`。

**Task** — 依据 DATA §5（Task 用 `CANCELLED`）、§6、DOMAIN §5（Task 取消不删除 Run/Artifact/Event；重开同 ID 递增 version）。
同样有 `UNIQUE (team_id, display_id)` 业务键 arm。`owner_role_id`/`team_id` 由 `0011`/`0012`/`0013` 负责。
保留 `0013`（Reference 一致性 + referenced subject）、`0014`（Event subject）、`0011`（依赖一致性）。

**Binding** — 依据 DATA §5（Binding 属"不更新历史语义，只追加后续事实"）、§6（随 Team 永久保留）、DOMAIN §2（「版本化且追加历史」）。
**保留 `configuration_snapshot` 可编辑**：`binding_team_consistency_forward_migration.rs:328` 就合法地更新它并期望成功，冻结它等于焊死正常业务编辑。本包锁的是"行被销毁/被静默替换"，不是"对象被冻"。
`(role_id, binding_generation)` 唯一索引补 REPLACE arm。

**Reference**（表名带引号，SQL 保留字）— 依据 DATA §5（「引用失效只更新 `validation_status`；不删除历史 URI/path/hash」）、DOMAIN §5（「Workspace 引用失效不删除 Reference；记录验证状态并引导修复」）。
除主键 REPLACE 外，补了 **UPDATE 侧**的 `target_uri`/`checksum_sha256`：普查记录的是 REPLACE 改写 `target_uri`，而 UPDATE 原地改写是同一危害的另一种写法，且 §5 明确只有 `validation_status` 会变。`validation_status`/`validated_at`/`version` 保持可写并有正向测试。

**DeliveryAttempt** — 依据 DATA §5（DeliveryAttempt 不更新历史语义，只追加）、§6（Delivery 随 Team 保留）、DOMAIN §5（「Message 到期/失败不删除原文或投递尝试」）。
**`0017` 的既有 REPLACE guard 原样保留**：不复制、不改写、不削弱，并由 `delivery_attempts_keep_their_original_replace_guard` **断言其确切报文** `Delivery attempt deduplication is immutable`（主键臂与 `(message_id, attempt_number)` 臂各一条）。
新增两处 `0017` 未覆盖的面：`delivery_attempts_single_active` 部分唯一索引的**换 ID 撞活跃位**（这是"不仅同 ID"要求在本题的唯一真实实例），以及 `0017` 身份触发器未覆盖的 `id`。

### 4.4 报错遮蔽的处置

先做的正是任务书提示的那一步：**逐表枚举既有触发器，再全仓检索既有测试是否断言了它们的具体报文**。结论：全仓 `contains("…")` 断言中，命中本包六张表的只有两处——
`run_lease_consistency_forward_migration.rs:1443`（`roles.team_id` 必须报 `Run consistency violation`，由 `0012` 产生）与 `notification_execution_integrity.rs:540`（Attention，非本包对象）。其余对本包六表的 REPLACE 测试（`notification_execution_integrity.rs:238`、`artifact_reference_consistency_forward_migration.rs:533`）都是**刻意写成只断言 `is_err()`** 的，不依赖触发器顺序。

处置采用**合并而非让位**：SQLite 中先触发的 `BEFORE` 触发器先 `RAISE(ABORT)`，且同事件多触发器顺序未定义。因此每个新守卫的 `WHEN` 都显式排除"既有守卫已经会拒绝这一行"的情形，让旧守卫保留自己的报文、新守卫只补差集。覆盖与不覆盖的并集不变，只是诊断归属更准确。

由此得到一个可测的正面结论：`delivery_attempts` 同 ID 替换现在**仍由 `0017` 先拒绝**并报它自己的报文——测试 `a_replacement_on_the_same_primary_key_is_refused` 对该表显式断言 `0017` 的报文，把"没重复、没遮蔽"从注释变成断言。`existing_reference_and_event_guards_still_apply` 与 `those_guards_block_on_the_pre_0028_schema_too`（后者在**只有 0001..0027** 的库上跑）把"既有守卫仍在承重"钉死。

### 4.5 刻意不加的规则（逐表核对"身份 UPDATE 是否可绕开历史关联"）

| 表 | 加了 | 没加及理由 |
|---|---|---|
| `teams` | `id` | 无第二个身份列 |
| `roles` | `id` | `team_id` 见 4.3；剩余"无任何历史关联的叶子 Role"没有可被绕开的历史，且加重会遮蔽 `0012` 报文 |
| `tasks` | `id` | `team_id`/`owner_role_id` 由 `0011/0012/0013` 条件式负责 |
| `bindings` | `id` | `role_id`/`harness_instance_id`/`team_id`/`adapter_id` 见下方"实测纠偏" |
| `references` | `id`+`target_uri`+`checksum_sha256` | `subject_type`/`subject_id` 联动由 `0013` 负责 |
| `delivery_attempts` | `id` | 其余七个身份列 `0017` 已无条件覆盖 |

**实测纠偏（本包真实踩到并修正）**：0028 初稿曾按 `0015` 的口径把 `bindings.role_id`/`harness_instance_id` 一并冻结。全量套件随即**红**：`binding_team_consistency_forward_migration.rs::valid_v9_history_upgrades_and_enforces_team_consistency_afterward`（第 458–462 行）在前向升级后把一个 `ACTIVE` Binding **改指同 Team 的另一个 Harness/Adapter 并期望成功**。结论是"同 Team 换执行者"是正常业务行为，不是历史篡改。已把该守卫收窄为仅 `id`，并在迁移注释里写明这次实测。**没有为了让套件变绿去改那个既有测试**。

**同理刻意不加的还有两处**：
- `bindings_single_active_per_role`（部分唯一索引）：同一 Role 的两个 `ACTIVE` Binding 有效区间必然重叠，`0002 bindings_no_overlapping_ranges_insert` 先拒绝。给它加 arm 就是"断言别人已提供的保证的无proven代码"，与 `0026` 对 `runs`/`approvals`/`messages` 的同一判断一致。有专门测试 `the_single_active_binding_slot_is_already_refused` 断言它**仍然被拒**且报文来自重叠守卫。
- **Attention**：未加任何永久 DELETE 守卫。`0028` 不触碰 `attention_items`。
- 无新增状态、错误码、权限或协议语义；所有触发器只用 `RAISE(ABORT, '…')`。
- 未建升级守卫表：0028 只约束其生效后发出的语句，历史行必然已满足，`valid = 1` 探针会退化成 ADVERSARIAL_REVIEW_PLAYBOOK 规则 5 警告的恒真式。升级要保证的是"什么都没变"，由两条升级测试直接验证。

## 5. RED → GREEN

### 5.1 真实 RED（保留在 `evidence/minimax-followup/FC-02/red-pre-0028.log`）

先写测试文件、在**尚未注册 0028** 的 v27 schema 上运行，exit 101：

```
test result: FAILED. 9 passed; 9 failed
```

失败项全部是 FC-02 缺口本身，不是装置故障：

| 失败测试 | RED 现场（实际语句与实际行数） |
|---|---|
| `leaf_rows_of_all_six_entities_cannot_be_deleted` | `the write was accepted and changed 1 row(s): DELETE FROM "teams" WHERE id=X'…0020'` |
| `a_replacement_on_the_same_primary_key_is_refused` | `INSERT OR REPLACE INTO teams VALUES(…,'hijacked','ARCHIVED',9,…)` 被接受 |
| `a_replacement_under_a_different_id_colliding_on_a_business_key_is_refused` | `INSERT OR REPLACE INTO roles VALUES(X'…0102',X'…0001','L-1','hijacked',…)`（**不同 ID**，撞 `UNIQUE(team_id,display_id)`）被接受 |
| `identity_columns_are_not_repointed` | `UPDATE teams SET id=X'…0101' WHERE id=X'…0020'` 被接受 |
| `a_reference_cannot_be_repointed_at_a_new_target` | `UPDATE "references" SET target_uri='docs/other.md'` 被接受 |
| `the_new_guards_are_installed` | `teams_retained_entity_retention_delete is missing from the hardened schema` |
| `this_package_owns_exactly_version_28` | `left: 0, right: 1` |
| `upgrading_to_0028_preserves_every_existing_row` / `a_failed_upgrade_has_no_side_effects` | 0028 尚不存在 |

同时 9 项**通过**，且这 9 项正是装置可信度的证据：既有 `0013`/`0014` 条件守卫在加固后仍生效、`0017` 既有 REPLACE 报文原样保留、正向业务编辑在 v27 下本就可用、`PRAGMA foreign_keys=1` 与 `recursive_triggers=0` 已确认、`the_single_active_binding_slot_is_already_refused` 确认该路径本来就由 `0002` 拒绝。

**RED 阶段的两次自身装置故障（如实记录）**：初版 `seed()` 把 `tasks` 表自身算作 LONE_TASK 的"子行"，以及随后把 `source_id` 当成所有子表的共有列——两次都让所有 18 项以同一句 rig 断言失败。修正为按表给出真实 FK 列后重跑，才得到上表这份干净 RED。这两次失败都发生在装置自检里、没有产出任何假绿结论。

### 5.2 GREEN

`evidence/minimax-followup/FC-02/green-0028.log`，exit 0：`18 passed; 0 failed`。

## 6. 变异证据

变异在**独立源码副本** `evidence/minimax-followup/FC-02/mutation-copy/`（自带 `Cargo.toml`、非工作区成员、离线自解析）进行，**共享 `crates/persistence-sqlite/src/lib.rs` 与迁移目录全程未被改动**：每轮结束后副本 SHA 还原为 `0c3b348a…`，且共享源码 SHA 与基线逐字节一致。
副本基线独立跑通 `18 passed`（`mutation-baseline.log`），说明该测试不依赖工作区才有意义。

| 变异 | 命令 | exit | 结果 | 命中的测试 |
|---|---|---|---|---|
| M1 全部守卫改为 `RAISE(IGNORE)`（装了但什么都不拒） | `cargo test --offline --test retained_entity_integrity` | 101 | 12 passed / **6 failed** | `leaf_rows_…cannot_be_deleted`, `a_replacement_on_the_same_primary_key…`, `a_replacement_under_a_different_id…`, `identity_columns_are_not_repointed`, `a_reference_cannot_be_repointed_at_a_new_target`, `upgrading_to_0028_preserves_every_existing_row` |
| M2 删除 delivery 活跃位 arm | 同上 | 101 | 15 / **3** | `a_replacement_under_a_different_id…`, `the_new_guards_are_installed`, `upgrading_to_0028_preserves_every_existing_row` |
| M3 roles replace 收窄为主键 | 同上 | 101 | 17 / **1** | `a_replacement_under_a_different_id…` |
| M4 删除 references DELETE 守卫 | 同上 | 101 | 15 / **3** | `leaf_rows_…cannot_be_deleted`, `the_new_guards_are_installed`, `upgrading_to_0028_preserves_every_existing_row` |
| M5 只把 tasks DELETE 守卫掏空 | 同上 | 101 | 13 / **5** | `leaf_rows_…cannot_be_deleted`, `a_replacement_on_the_same_primary_key…`, `a_replacement_under_a_different_id…`, `the_new_guards_are_installed`, `upgrading_to_0028_preserves_every_existing_row` |

逐条命令、exit、失败用例、还原哈希见 `mutation-runs.txt`；完整日志 `mutation-M*.log`；补丁 `mutation-copy/mutations/*.sql` 与 `mutation-0028-pristine.sql`。

M1 是关键对照：把每个 `RAISE(ABORT, …)` 换成 SQLite 唯一合法的无报文 `RAISE(IGNORE)`——守卫名字还在、装置的 `the_new_guards_are_installed` 仍然全绿——而六对象删除/替换/身份重写立刻全部失守。装置不会因为"触发器存在"而误判。

## 7. 验收命令（串行，全部实跑）

| # | 命令 | exit | 结果 |
|---|---|---|---|
| 1 | `cargo test -p persistence-sqlite --test retained_entity_integrity --locked --offline` | **0** | 18 passed / 0 failed |
| 2 | `cargo test -p persistence-sqlite --locked --offline --no-fail-fast` | **0** | **227 passed / 0 failed** |
| 3 | `cargo build -p persistence-sqlite --locked --offline` | **0** | — |
| 4 | `cargo clippy -p persistence-sqlite --all-targets --locked --offline -- -D warnings` | **0** | 零 warning |
| 5 | `RUSTDOCFLAGS="-D warnings" cargo doc -p persistence-sqlite --no-deps --locked --offline` | **0** | — |

格式：只对本包改动的两个文件跑 `rustfmt --edition 2024`，未执行 `cargo fmt --all`；`cargo fmt -p persistence-sqlite -- --check` 现为 exit 0、零 diff（`fmt-check-after.log` 为空）。日志见 `accept-*.log`，汇总 `acceptance.txt`。

### 全量套件与 FC-03 遗留的真实状态

全量套件本次 **exit 0、227 通过 0 失败**，`run_transition_integrity` 15 项全绿。

需要准确说明的一点：本轮**没有**出现"FC-03 已登记问题导致红"。原因在 `run_transition_integrity.rs:709-737`：`terminal_to_terminal_is_a_registered_open_gap` 断言的是 `outcome.is_ok()`——它**主动断言该缺口仍然敞开**，并在缺口被 FC-03 关闭时要求更新 RESULT 与迁移注释而不是删用例。所以当前这套"绿"里包含一个刻意保留的"尚未修复"标记。终态→终态 12 格仍待 FC-03 处理，本包未越界改动，也未修改该文件。

首次全量运行时确实红过一次，且是本包自身引入的真实回归（`bindings` 身份守卫过度覆盖，见 4.5），已修正后复跑转绿。全程未删除或修补任何脏历史来保绿。

## 8. 自检

**SELF_CHECK_PASS**

依据：五条验收命令全 exit 0；真实 RED 先于 GREEN 且 RED 内容是缺口本身；变异全部命中且源码按 SHA 还原；旧迁移零漂移；共享源码未被变异触碰；未提交/推送/部署；未加 Attention 永久守卫；未焊死合法编辑（且有一次过度覆盖被既有测试抓出并修正）。

自检不等于 DONE。正式准入由 Codex 独立复核。

## 9. 剩余风险与回退

**剩余风险**

1. **存储层边界，非应用层**。0028 只约束直接打到 SQLite 的 `DELETE`/`REPLACE`/`UPDATE`。仓库 production API 不暴露硬删除能力，本次也未发现生产 `DELETE` 接口；本包不声称存在生产损失。
2. **残留叶子面**：`roles.team_id`、`tasks.team_id`/`owner_role_id`、`bindings` 的 `role_id`/`harness_instance_id`/`adapter_id`，仅在**存在历史关联时**由既有守卫拒绝。"完全无历史的行被跨 Team/换执行者改写"这一格未被任何守卫覆盖——因为没有冻结条款支持新增规则，且加则会遮蔽 `0012` 的已断言报文。此处按"不把一般编辑当历史篡改"处理。若总控认为该格需要独立裁决，应作为单独决策项，不宜由执行端自行扩写。
3. **`references` UPDATE 冻结 `target_uri`/`checksum_sha256` 是一处解释性判断**：DATA §5 原文是"不删除历史 URI/path/hash"，字面指向删除；本包据同句"引用失效只更新 `validation_status`"取其强含义（原地改写同样禁止）。若裁决认为过严，回退方式是仅保留 `id`。
4. **触发器顺序**：SQLite 未定义同表同事件多触发器的执行顺序。本包靠"新守卫 `WHEN` 排除旧守卫已覆盖的行"消除顺序依赖，而不是依赖顺序本身；但 SQLite 实现若变更该行为，需重跑对照。
5. `bindings` 活跃位那一条**依赖 `0002` 的区间重叠守卫**而非独立 arm。若将来 `0002` 的区间语义变化，该路径的覆盖会同步变化；`the_single_active_binding_slot_is_already_refused` 会在那时报警。
6. 证据目录保留了独立源码副本（含离线 `Cargo.lock`），体积约 716K；构建产物已移除。

**回退**

- 0028 是纯前向迁移，已落库的 trigger 不会因移除登记行而消失。回退需保留迁移链，或以后续前向迁移显式 `DROP TRIGGER`；**不承诺"退二进制防线自然消失"**（公共约束第 6 条）。
- 如需临时撤销本包而不动数据库：从 `src/lib.rs` 末尾删除 `Migration::new(28, …)` 与常量即可；但此时已落库实例仍带 18 个 trigger，且 SQLx 可能因迁移记录不符报错——需按上述前向方案处理。
- 本包未改动任何既有文件内容（`src/lib.rs` 仅末尾追加），故回退不涉及恢复他人成果。

## 10. 证据索引

`docs/reviews/evidence/minimax-followup/FC-02/`

| 文件 | 内容 |
|---|---|
| `environment.txt` | rustc/cargo/工具链 |
| `input-before-compare.txt` | 与审计输入基线的 299 条比对 |
| `red-pre-0028.log` | **真实 RED**（v27，exit 101，9 failed） |
| `green-0028.log` | GREEN（18 passed，exit 0） |
| `mutation-source-baseline-sha256.txt` | 变异副本源码基线 SHA |
| `mutation-baseline.log` | 副本独立跑通 18/18 |
| `mutation-M1..M5-*.log` | 五个变异的完整日志 |
| `mutation-runs.txt` | 命令 / exit / 失败用例 / 还原哈希台账 |
| `mutation-copy/` | 独立源码副本 + `mutations/*.sql` 补丁 + `mutation-0028-pristine.sql` |
| `mutation-0028-pristine.sql` | 变异前后的权威原文 |
| `accept-1..5-*.log`, `acceptance.txt` | 五条验收命令完整日志与 exit |
| `fmt-check-before.log` / `fmt-check-after.log` | 格式化前后（after 为空，exit 0） |
| `build.log` / `clippy.log` / `rustdoc.log` | 中间轮次日志 |
| `full-suite.log` | 全量套件轮次日志 |
| `output-sha256.txt` | 产出文件 SHA |