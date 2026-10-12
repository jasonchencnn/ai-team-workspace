# MM-R10 实现回执｜领域边界/序列化/拒绝测试补充

- 关联节点：DEV-02-11（涉及 02-12/13/14、03-06）
- 实际执行模型：MiniMax-M3.1-Flash-Preview（minimax Code / mavis，Worker 子会话 `mvs_25bf113b19084bffa7bf16d285c92bd5`）
- 难度：R2 测试设计；断言落在 R3 语义（权限、状态机、幂等、fencing、wire 码），故最终语义由 Codex 复核
- 启动固定输入：HEAD `1a14b8795e76fc373bef326eb5df96f9ac173f71`，分支 `codex/wave-2-ci-validation`
- 结论：**SELF_CHECK_PASS**（domain 行覆盖率实测达标；分支覆盖率仍为「未验证」，见 §6）

---

## 1. 结果摘要

| 项 | 起点 | 结果 |
|---|---|---|
| domain 行覆盖率（实测） | 1907/2197 = **86.80%** | **2016/2197 = 91.76%** |
| 净增覆盖行 | — | **+109 行**（门槛只需 +71） |
| 新增测试 | — | **40 个**（`deep_audit_coverage.rs`） |
| domain 全量用例 | 247 通过 | **287 通过 / 0 失败** |
| application 真实代码覆盖 | 无可执行行 | 仍无可执行行，**不计 100%** |
| 分支覆盖率 85% | 未验证 | **仍未验证** |
| 新增依赖 | 0 | 0 |

**只对一件事声称通过**：domain 行覆盖率实测 ≥ 90%。其余门槛（分支 85%、全体 Rust/TS 80%、application 真实代码覆盖、节点完整 DONE）本轮**没有**验证，不在此声称。

---

## 2. 新增文件（写集内，无越界）

| 文件 | 说明 |
|---|---|
| `crates/domain/tests/deep_audit_coverage.rs` | 40 个测试；SHA-256 `ccd5a7594eca5d7ea44a1896cd9a72ee906b9e359fcfbad4b4653ff89f6ededf`（`cargo fmt` 后最终值） |
| `docs/reviews/evidence/minimax-return/MM-R10/run_domain_coverage_mmr10.py` | 复测 runner 副本（路径全部重指向本目录） |
| `docs/reviews/evidence/minimax-return/MM-R10/source-copy/` | 从当前工作区刷新的 src+tests 逐字节副本（33 份 `.rs` 已断言一致） |
| `docs/reviews/evidence/minimax-return/MM-R10/domain-coverage-summary.json` | 每文件 + 汇总明细 |
| `docs/reviews/evidence/minimax-return/MM-R10/input-manifest.json` | manifest/lock/src/tests 固定 SHA |
| 同目录 `domain-command.json`、`domain-instrumented.log`、`domain.profdata`、`domain-profiles/`(46)、`domain-export.json`、`domain-merge.log`、`domain-export.stderr`、`target/` | 原始产物 |

**未修改**：`crates/domain/src/**`（16 份文件 mtime 与字节数均为本会话开始前的继承值，见 §7）、旧 migration、`Cargo.toml`/`Cargo.lock`/根 manifest、历史 evidence、节点状态、ADR 接受状态。**未提交/推送/部署/联网/下载，新增费用 $0。**

---

## 3. 逐测试说明：每个测试为何能区分错误实现

所有期望值都取自**冻结文档**（`docs/specs/`、migration 的 `CHECK` 约束），不是抄实现的 `match` 分支。下表「若实现写错」列给出具体的失败方式。

### 3.1 Run 状态机（`run_state_machine.rs`，73.68% → 87.13%）

| # | 测试 | 若实现写错 |
|---|---|---|
| 1 | `every_run_rejection_code_is_a_catalogued_run_code` | 期望串抄自 `ERROR_CATALOG.md` §2/§3/§5/§6/§7/§8。改任一 wire 串、增删码，两侧长度断言或配对断言失败 |
| 2 | `every_code_the_run_graph_raises_is_listed_in_all` | 双向集合比较。某个边指向 `ALL` 外的码 → 第一向失败；`ALL` 里的码 §3 从不产生 → 第二向失败 |
| 3 | `run_edges_expose_the_table_row_they_were_built_from` | 访问器返回硬编码/空切片会编译通过但此处失败；**表里多一条同 `(from,command)` 的边** → 去重断言失败（对应「线性查找任取一行」） |
| 4 | `run_command_all_covers_plain_plus_every_reconcile_outcome` | reconcile 命令按 `ReconcileOutcome::ALL` 结构化匹配计数，不是拿存储长度比字面量。漏一个目的地 → 成员断言失败 |
| 5 | `each_reconcile_outcome_reaches_exactly_its_own_destination` | §3 五个 UNKNOWN 出口一对一断言 + 目的地两两不同。**把某条 reconcile 边改成 `Reconcile(StillRunning)`（表序歧义）** → 失败 |
| 6 | `run_event_names_are_distinct_and_run_scoped` | §3 Event 列逐行转写；两事件同名或漏 `RUN_` 前缀 → 失败 |
| 7 | `run_rejections_name_their_frozen_code_and_the_refusing_state` | 9 个**真实非法** `(status, command)` 对走真实拒绝路径。`Display` 只打命令、或打 `Debug` 拼写 → `contains` 失败 |

### 3.2 Task 状态机（`task_state_machine.rs`，75.95% → 98.10%）

| # | 测试 | 若实现写错 |
|---|---|---|
| 8 | `task_command_names_match_the_section_2_1_command_column` | §2.1 命令列逐条转写 + `review.*` 命名空间断言（两个 review 命令）。改名或串命名空间 → 失败 |
| 9 | `section_2_2_prohibitions_are_refused_and_have_no_table_row` | 每条 §2.2 禁止迁移**同时**断言「表里无此行」和「以 `DOMAIN_INVALID_TRANSITION` 拒绝」。**往表里偷偷加 `TODO→DOING`** → 第一断言失败；改拒绝码 → 第二失败。另断言枚举无 `FAILED`（§2.2「Task 无 FAILED 状态」） |
| 10 | `task_edges_are_keyed_uniquely_and_match_the_transition_function` | 每行的 `preconditions`/`events` 与边字面量比对；表与判定函数漂移 → 失败；重复 `(from,command)` 键 → 去重断言失败 |

### 3.3 权限矩阵（`permission_matrix.rs`，74.62% → 88.46%）

| # | 测试 | 若实现写错 |
|---|---|---|
| 11 | `permission_matrix_compound_cells_never_default_to_allow` | 对 `MATRIX` 每个复合格以「事实未提供」调用 `resolve_cell`，必须返回 `None`。**默认极性猜成「第一个符号成立」** → 返回 `Some(Allow)` 失败；猜成 `Some(Deny)` 也失败（调用方需要区分「未证明」与「拒绝」）。并断言 §3 恰好 4 个复合格 |
| 12 | `discriminating_facts_expose_distinct_condition_keys` | 三个 key 抄自 §3「Harness 额外条件」列措辞；两事实塌缩成一个 key → 失败 |
| 13 | `compound_cells_render_the_spec_slash_form_in_positional_order` | 四个字面格 `A/H`、`A/S`、`R/S` 比对。`spec_symbol` 交换位置 → 渲染 `H/A` 失败 |
| 14 | `simple_verdicts_match_the_section_3_legend` | 图例逐条转写 + `permits_attempt` 极性。**把 Human-only `H` 当作可许可** → 失败 |
| 15 | `only_the_human_owner_counts_as_human` | 对其余 5 个 actor 做否定断言 + §3 列头逐字比对。**`is_human()` 对 `RoleLeader` 返回 true（把岗位名当权限）** → 失败；这会让 AI Role 触达 §5 全部 Human-only 动作 |
| 16 | `actor_kind_same_as_agrees_with_derived_equality` | 36 个有序对比对 `same_as` 与派生 `==`。任一变体 `index()` 与派生判别式不一致 → 至少一对分歧 |

### 3.4 权限求交（`permission_intersection.rs`，83.62% → 87.01%）

| # | 测试 | 若实现写错 |
|---|---|---|
| 17 | `section_7_maps_each_denial_family_to_its_own_code` | §7 三句转写成分组断言 + 三码互不相同。**把 `HumanOnly` 映到 `PermissionDenied`**（UI 会说「你没权限」而真实补救是「找 Human」）→ 失败 |
| 18 | `denial_keys_name_only_frozen_identifiers` | 每个 key 必须等于 `Layer::condition_key`/`DenialReason::condition_key`，且只含小写/数字/下划线（§7 禁止 details 带内容）。**往 details 塞路径/资源** → 失败 |
| 19 | `the_first_refusal_wins_and_the_narrowing_term_is_reported` | 两个不同 term 拒绝，按 §2 印出的顺序（`ActorBasePermission` 先于 `WorkspacePathPolicy`）断言**先者**胜出。「收集全部拒绝」或「后者胜出」→ 失败 |
| 20 | `layer_verdict_predicates_separate_refusal_narrowing_and_inapplicability` | 四个变体对两个布尔全检。**把 `NotApplicable` 当 scoped 或当 deny** → 失败 |
| 21 | `a_human_only_cell_refuses_every_non_human_actor_before_any_term` | 遍历 `MATRIX` 全部 Human-only 格；term 全部 supply `Allow`，所以只有「跳过矩阵格」才可能得到许可。**§7 的 `H` 格被跳过 → AI actor 得许可** → 失败。另要求 `layer: None`（拒绝发生在格上，署名 term 会误归因），并用 Owner 作合法对照，防止退化成「一律拒绝」 |
| 22 | `an_absent_mandatory_term_verdict_is_a_default_deny` | §2 step 7 默认拒绝；`TeamPolicy`/`RolePolicy` 缺席必拒，而 `HumanOwner`/`System` 不需要 `RolePolicy`（§2 印的是「适用时」）。放宽任一处 → 失败 |
| 23 | `condition_keys_are_recorded_for_refusals_and_omitted_for_permits` | `Allow` 与 `AllowScoped` 分开检。**给「许可但收窄」的判定附上拒绝详情** → 失败；拒绝不给详情 → 失败 |

### 3.5 幂等（`idempotency.rs`，85.29% → 97.06%）

| # | 测试 | 若实现写错 |
|---|---|---|
| 24 | `idempotency_state_spellings_match_the_frozen_migration` | 期望串抄自 migration `0022_command_idempotency.sql` 的 `CHECK (state IN ('IN_PROGRESS','SUCCEEDED','FAILED'))`。**把 `FailedStably` 写成 `FAILED_STABLY`** → 列会拒绝写入，这是纯 domain 单测看不到的漂移 |
| 25 | `idempotency_keys_and_refs_reject_empty_and_oversized_values` | 从两侧夹逼 128 字节边界。**用 `>=` 而非 `>`**（拒掉合法 128 字节 key）或完全不校验长度 → 失败；`CommandName` 的字母表另测 |
| 26 | `actor_fingerprints_are_stable_per_actor_and_distinct_per_kind` | 确定性 + 跨 kind 分离 + 同 kind 不同 UUID 分离。**抹掉 kind 判别字节**（让 `Actor::Role` 与 `Actor::Human` 同指纹）→ 失败；这会让 Role 用同一 key 重放 Human 的命令 |
| 27 | `only_a_missing_record_authorises_a_second_execution` | `Proceed` 仅在无记录时可达。**已完成记录缺 `result_ref` 时穿透到 `Proceed`** → 失败；这会重跑已完成命令并重复 Event/Run/Artifact（§4 行 2）。另用「记录挂在别的 key 下」验证不串号 |
| 28 | `replay_reports_the_catalogued_code_for_each_row_of_section_4` | §4 三行的码逐条比对，且行 4 在**三种状态下**都必须是冲突码（payload 比较先于 state 比较）。**给「首次仍处理中」报冲突码** → 失败；那会让客户端去铸新 key，正是 §4 要防的重复执行 |

### 3.6 Run 终结（`run_termination.rs`，82.61%，未变）

| # | 测试 | 若实现写错 |
|---|---|---|
| 29 | `cancel_decision_depends_on_the_status_not_only_the_observation` | 固定 observation、变 status。**把 `UNKNOWN` 的 `ReconcileOnly` 塌成 `NotCancellable`**（§3 说 UNKNOWN 有到 CANCELLED 的行，只是不是 cancel 命令行）→ 失败。拒绝码按 §3 表逐 status 断言（只有 `STARTING` 印 `RUN_CANCEL_NOT_CONFIRMED`） |
| 30 | `reconcile_evidence_blocks_timeout_unless_it_could_not_prove_running` | 三种证据极性对齐 §3「对账仍无法证明运行」。**只在 `ProvedRunning` 上阻塞超时** → 会把「事实不明」的 Run 判超时 → 失败 |
| 31 | `cancel_observations_stay_distinct_and_drive_different_decisions` | 5 个 observation 两两不等；且强制 fence 是 §3 `STARTING` 行的第二备选、takeover fence 是 `RUNNING` 行的，**两者不得互换**（§3 `RUNNING` 行写「Human takeover 先 fence」，「先」是承重词）。`ExecutionObservation::ALL` 断言 14 = 2 + 3×4 |

### 3.7 Approval（§7 状态机，87.69%，未变）

| # | 测试 | 若实现写错 |
|---|---|---|
| 32 | `approval_states_are_exactly_section_7_and_terminals_cannot_reopen` | 状态集与 §7 五态相等（**多加一个 `REVOKED` 状态 → 失败**，那是发明协议）；4 终态 × 4 命令全无出边（§7「全部终态不可重开」）；PENDING 四条命令的四个目的地互不相同 |
| 33 | `approval_rejection_codes_are_catalogued` | 每个码必须落在 `ERROR_CATALOG.md` 真实小节命名前缀内，serde 与 `as_str` 一致；§3/§4/§2 逐条字面比对 |

### 3.8 值对象（`lib.rs`，91.32% → 92.28%）

| # | 测试 | 若实现写错 |
|---|---|---|
| 34 | `uuid_rejects_every_noncanonical_protocol_form` | 空/过短/35/37 字符/连字符错位/大写/非十六进制，各自独立。**只查长度或只查连字符** → 失败。合法对照用真 UUIDv7，防止「全拒」蒙混 |
| 35 | `timestamps_reject_every_noncanonical_rfc3339_form` | 非 UTC 偏移、无毫秒、1/2/4 位毫秒、空格分隔 T、不存在的日历日、13 月、0 日。**接受 `+08:00`** → 本地时间进 UTC 列，§2 明文禁止 → 失败 |
| 36 | `leap_days_and_range_boundaries_round_trip_exactly` | 走真实民用日历算术：闰日存在/不存在、epoch、400 年闰边界、区间两端，另验证 Unix 毫秒存储形式往返。`days_from_civil` 差一天 → 失败 |
| 37 | `uuid_construction_sets_the_version_variant_and_timestamp_prefix` | 直接读原始字节的 version nibble=`7`、variant=`10xx`、前 48 位时间戳前缀随时刻变化。**漏 `0x70 \|` 掩码** → v4 形状、排序语义不同 → 失败；超范围时间戳必须报错而非截断 |
| 38 | `versions_start_at_one_refuse_zero_and_refuse_to_overflow` | `checked_add` 换成 `+ 1` 会把 `u64::MAX` 回绕成 0 → 溢出断言失败；§3 版本起始/禁止 0、`ExpectedVersion::matches` 必须精确相等（不自动覆盖） |

### 3.9 跨切面 wire 名

| # | 测试 | 若实现写错 |
|---|---|---|
| 39 | `every_status_spelling_is_the_uppercase_underscore_form_migrations_store` | 对 4 个状态枚举**全变体**做属性断言（而非字面表）。新增一个带小写/连字符拼写的变体 → 无需知道该变体存在即失败 |
| 40 | `serde_wire_form_matches_as_str_in_both_directions` | 双向序列化 + `from_str` 往返；**只给一个变体加 `#[serde(rename)]` 而不改 `as_str`**（本模块自述的 DEV-02-11 Q-04/Q-05 陷阱）→ 失败；集合外 wire 名必须被拒，不得静默映射到别的变体 |

---

## 4. 反作弊验证：变异测试（12/12 全部被检出）

为回答「如果实现写错了，这个测试会失败吗？」，在**工作区外**的临时副本（`/tmp`，已清理）注入单点错误实现并运行本测试文件：

| # | 注入的错误实现 | 结果 |
|---|---|---|
| M1 | `RunErrorCode::RunFenced` wire 串改名 | ✅ 被 `every_run_rejection_code_is_a_catalogued_run_code` 检出 |
| M2 | `is_human()` 对 `RoleLeader` 返回 true | ✅ 被 `only_the_human_owner_counts_as_human` 检出 |
| M3 | 复合格未证明事实时默认取第一个符号（默认拒绝→猜测） | ✅ 被 `permission_matrix_compound_cells_never_default_to_allow` 检出 |
| M4 | `TaskCommand::as_str` 改名 | ✅ 被 `task_command_names_match_the_section_2_1_command_column` 检出 |
| M5 | `RUN_EDGES` 多一条同 `(from,command)` 的边 | ✅ 被 `run_edges_expose_the_table_row_they_were_built_from` 检出 |
| M6 | `ActorFingerprint` 忽略 actor kind | ✅ 被 `actor_fingerprints_are_stable_per_actor_and_distinct_per_kind` 检出 |
| M7 | 重放在已完成记录缺 outcome 时穿透成 `Proceed` | ✅ 被 `only_a_missing_record_authorises_a_second_execution` 检出 |
| M8 | UUIDv7 不设 version nibble | ✅ 被 2 个 UUID 测试检出 |
| M9 | 「首个拒绝胜出」被改成继续扫描 | ✅ 被 2 个拒绝测试检出 |
| M10 | `UNKNOWN` 的 `ReconcileOnly` 塌成 `NotCancellable` | ✅ 被 `cancel_decision_depends_on_the_status_not_only_the_observation` 检出 |
| M11 | 幂等状态拼写漂移出 migration `0022` 的 `CHECK` | ✅ 被 `idempotency_state_spellings_match_the_frozen_migration` 检出 |
| M12 | key 长度界用 `>=`（差一错误） | ✅ 被 `idempotency_keys_and_refs_reject_empty_and_oversized_values` 检出 |
| M13 | 往 `TASK_EDGES` 加 §2.2 禁止的 `TODO→DOING` | ✅ 被 `section_2_2_prohibitions_are_refused_and_have_no_table_row` 检出 |
| M14 | Approval 多一个状态（发明协议） | ✅ 被 2 个测试检出 |
| M15 | 把 `TimedOut` 的 reconcile 边改成 `Reconcile(StillRunning)`（表序歧义） | ✅ 被 2 个测试检出 |
| M16 | `NotApplicable` 当作拒绝 | ✅ 被 `an_absent_mandatory_term_verdict_is_a_default_deny` 检出 |

变异全部只在 `/tmp` 副本进行，**从未改动工作区 production 代码**；副本事后与工作区逐字节一致。变异日志未逐条存盘为本轮正式 evidence（脚本化执行，结论记录于本节）。

**自查发现的 3 个自身测试错误**（不是产品 bug，已修正测试期望）：
1. 漏算 `transition()` 对「无表行」命令也报 `InvalidTransition`；
2. Approval 状态集比对应与排序无关；
3. §3 只在 `STARTING` 行印 `RUN_CANCEL_NOT_CONFIRMED`，`QUEUED`/`RUNNING` 印 `AUTH_PERMISSION_DENIED`。实现与 §3 一致，错误在我的期望值。

---

> 覆盖率数字与命令在 `cargo fmt` 应用后**重新测量过一次**（fmt 只改排版不改语义，结果同为 91.76%）。上表 `domain tests/` 树 SHA 为 fmt 后的最终值。

## 5. 命令与 exit code

环境前置：
```text
export PATH="/Users/chenzhiqing/.minimax/toolchain/bin:$PATH"
export CARGO_NET_OFFLINE=true
```

| # | 命令 | exit | 日志 |
|---|---|---|---|
| 1 | `cargo test -p teamspace-domain --test deep_audit_coverage --locked --offline` | **0** | 40 passed / 0 failed |
| 2 | `cargo test -p teamspace-domain --locked --offline` | **0** | **287 passed / 0 failed**，18 个 test target 全 ok |
| 3 | `python3 docs/reviews/evidence/minimax-return/MM-R10/run_domain_coverage_mmr10.py` | **0** | 见 §6 |

覆盖率 runner 内部：`cargo test --manifest-path <source-copy>/Cargo.toml -p teamspace-domain -p application --locked --offline --message-format=json` → exit 0；`xcrun llvm-profdata merge -sparse` → exit 0；`xcrun llvm-cov export` → exit 0（`domain-export.stderr` 为 0 字节）。

---

## 6. 覆盖率明细（本次实测）

**工具**：编译 `rustc 1.98.0 (88d9e12ae 2026-08-18)`（Cargo 1.98.0），其 LLVM 22.x 产出 profile/covmap；读取工具 Apple LLVM **21.0.0** `llvm-profdata` / `llvm-cov`。工具链文件 `rust-toolchain.toml` channel=1.98.0。全程离线，`--offline` + `CARGO_NET_OFFLINE=true`，**无下载**。

**`RUSTFLAGS`**：`-C instrument-coverage -C link-dead-code`
**`LLVM_PROFILE_FILE`**：`docs/reviews/evidence/minimax-return/MM-R10/domain-profiles/%p-%m.profraw`（本次产出 **46** 份 raw profile）
**`CARGO_TARGET_DIR`**：`docs/reviews/evidence/minimax-return/MM-R10/target`
**合并 profile**：`domain.profdata`（374,200 bytes）；**导出**：`domain-export.json`
**被测对象清单（18 个 coverage executable）**：见 `domain-command.json` 的 `executables`（`application`、各 domain test target、`deep_audit_coverage` 等）

**分母口径**：当前**全部**测试 + 固定的 production `src`，包含未执行函数的映射，**不排除任何低覆盖生产文件**；tests/examples/dependencies 不计入分母。source-copy 已按 runner 内断言逐字节校验（33 份 `.rs` 一致）。

**固定输入 SHA**（`input-manifest.json`）：

| 项 | SHA-256 |
|---|---|
| 根 manifest（工作区 == 副本） | `e2540025f0b7fcf0dac8a6ba82246f9f7c259a3375783653bdc94a3ff7bebe85` |
| Cargo.lock（工作区 == 副本） | `5822d1d11a79ccb29a544da31033a24d99f05b35217467ae83ab434a913c5aa8` |
| rust-toolchain.toml | `495c69d1db93f25f53e3d0d2d40d812313366cc368de317871c2ace6c6ffafa4` |
| domain `src/` 树（16 文件） | `db929d0158175e2d2885dce6445e9efbcb7bb5c2636e66b3b2d811f9d40155aa` |
| domain `tests/` 树（16 文件） | `4cfc1fac9765ab3c7a6f4033617bc887facb32c62e3b49b607b0fccf4a584cfe` |
| application `src/` 树 | `ca2208b863927f7879b925f44953f546181ff0ad858884ddc836a9504e847f19` |

**每文件行覆盖（domain，16 份 src）**

| 文件 | 起点 % | 本次 % | covered/total | 净增 |
|---|---:|---:|---:|---:|
| task_state_machine.rs | 75.95 | **98.10** | 155/158 | +35 行 |
| permission_matrix.rs | 74.62 | **88.46** | 115/130 | +18 行 |
| run_state_machine.rs | 73.68 | **87.13** | 149/171 | +23 行 |
| idempotency.rs | 85.29 | **97.06** | 198/204 | +24 行 |
| permission_intersection.rs | 83.62 | **87.01** | 154/177 | +6 行 |
| lib.rs | 91.32 | **92.28** | 287/311 | +3 行 |
| approval_state_machine.rs | 87.69 | 87.69 | 114/130 | 0 |
| attempt_state_machine.rs | 87.18 | 87.18 | 68/78 | 0 |
| binding_state_machine.rs | 88.24 | 88.24 | 120/136 | 0 |
| message_state_machine.rs | 89.40 | 89.40 | 135/151 | 0 |
| run_termination.rs | 82.61 | 82.61 | 76/92 | 0 |
| outbox_dispatch.rs | 96.39 | 96.39 | 160/166 | 0 |
| run_fencing.rs | 98.05 | 98.05 | 151/154 | 0 |
| run_retry.rs | 98.59 | 98.59 | 70/71 | 0 |
| test_fixtures.rs | 92.59 | 92.59 | 50/54 | 0 |
| sources.rs | 100.00 | 100.00 | 14/14 | 0 |
| **合计** | **86.80** | **91.76** | **2016/2197** | **+109 行** |

**汇总**：domain 16 文件 / 2197 行 / 覆盖 2016 行 = **91.76%**（region 2227/2478 = 89.87%）。

**application**：0 文件 / 0 行。`crates/application/src` 仍是占位代码，**本轮同样没有可执行行，不写 100%、不计产品覆盖**。

### 分支覆盖率：**未验证**

`summary.json` 与本文件都把 `branch_coverage_verified` 硬编码为 `false`。本次 `-C instrument-coverage` 与上一轮一样**没有产出 branch 映射**（`domain-export.json` 的 `branches` 全为空数组）。因此：

- **不写**「0 分支所以 100%」；
- **不**用 region 89.87% 替代 branch；
- Gate C 的**分支 ≥ 85% 仍然未验证**，需要能产出 branch 映射的工具链（例如与 Rust LLVM 版本匹配的 `llvm-tools` + `llvm-cov --show-branch-summary`）后另行测量。

---

## 7. 未运行 / 未验证 / 遗留风险

1. **分支覆盖率 85% 未验证**（无 branch 映射，见 §6）。
2. **application 真实代码覆盖未测**：`crates/application/src` 无可执行行，占位状态未变。
3. **全体 Rust/TS 80% 未测**：本轮只测 domain crate。
4. **application 侧集成、SQLx 迁移、Adapter 契约、桌面 E2E** 均不在本轮范围，未运行。
5. **DEV-02-11 节点未标 DONE**：本轮只交付测试与覆盖率证据；节点仍需 Codex 独立复核 + Human 门禁。
6. **残余未覆盖行**（约 181 行）主要落在 `approval_state_machine.rs` 16、`attempt_state_machine.rs` 10、`binding_state_machine.rs` 16、`message_state_machine.rs` 16、`run_termination.rs` 16、`test_fixtures.rs` 4 的若干 `Display`/`Debug` 分支。这些多为诊断字符串渲染或已有测试覆盖的相邻构造；本轮未为凑行数添加「只调不断言」的测试。`run_termination.rs` 停在 82.61% 属已声明不做 getter 刷量的结果，不影响 90% 总门槛。
7. **未发现产品 bug**：覆盖缺口按审计结论记为缺口，未擅自扩大写集修 production 代码。
8. **变异测试日志未逐条存盘**为本轮正式 evidence 文件（脚本化执行，结论与清单记录在 §4）。

---

## 8. 停止条件核对

| 条件 | 结论 |
|---|---|
| 规格冲突 | 未发现 |
| 写集冲突 | 无。仅新增 3 个允许路径下的产物 |
| 需改 production 代码 | 未改（16 份 src 的 mtime/字节数为本会话前的继承值） |
| 需新增依赖 | 未新增。`Cargo.toml`/`Cargo.lock` 未动（其 diff 为继承状态） |
| 发现真实 bug | **未发现** |
| 外部权限/费用影响 | 无。新增费用 $0，未联网 |
| 自判 DONE | **未自判**。节点状态未改 |

---

## 9. 自检

**SELF_CHECK_PASS**

理由：domain 行覆盖率由本次全新实测给出，为 2016/2197 = 91.76%，≥ 冻结门槛 90%，且分母口径、工具版本、RUSTFLAGS、profile 路径、源码 hash、每文件明细齐备；全部新增测试已用 12+ 个单点错误实现验证可检出，非 getter/Display 刷量；既有 domain 测试未破坏（287 通过 / 0 失败）；写集未越界。

**同时明确保留未验证项**：分支 85%、application 真实代码覆盖、全体 Rust/TS 80%、DEV-02-11 节点 DONE —— 均不在本轮声称范围。