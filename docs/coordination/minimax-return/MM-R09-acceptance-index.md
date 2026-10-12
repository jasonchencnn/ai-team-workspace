# MM-R09｜可复核验收归档索引（本轮可交付范围内）

- 关联节点：DEV-01-05（关联 01-08、02-01/11）。**本文件不修改任何节点正式状态**。
- 归档对象：MM-R01、R02、R03、R04、R05、R06、R10。
- 审计依据：`docs/reviews/DEEP-AUDIT-2026-10-08.md`（尤其 §5「文档/证据纠正与节点准入」）。
- 仓库：HEAD `1a14b8795e76fc373bef326eb5df96f9ac173f71` + 继承未提交工作树（未 reset/checkout/clean/stash）。
- 本文件是**归档与索引**，不是验收通过证明，也不是节点准入裁定。正式准入由 **Codex** 裁定。

---

## 0. 口径纪律（阅读本文件前必读）

### 0.1 状态标签定义

本文件每个结论格必须落在以下四类之一，**不得合并表述**：

| 标签 | 含义 | 判定依据 |
|---|---|---|
| **已验证** | 本轮或该包会话内**实际执行**了命令，退出码与结果均有日志 | 该包 evidence 目录内的原始日志 |
| **未运行** | 该项**没有执行**，不因任何理由推断其结果 | 缺日志即缺证据 |
| **失败** | 执行了且退出码非 0，失败体已逐条定位 | 原始日志的 panic/assertion 行 |
| **仅缓存命中** | 结论依赖编译/工具缓存产物，**未在声明的会话内重新生成** | 构建输出显示「Finished」而无 `Compiling` |

### 0.2 明确禁止的表述（本文件及任何引用本文件的文档都不得写）

1. **不得**把「除已登记的 3 处版本计数断言外全绿」写成「全工作区通过」。真实口径是：
   `persistence-sqlite` = 206 passed / 3 failed；`adapter-supervisor` = 79 passed / 11 failed；
   `teamspace-domain` = 287 passed / 0 failed / 1 ignored。**没有任何一次全工作区运行是绿的。**
2. **不得**声称 canonical Mac 21 项**在线**门禁已验证。本轮与各包均未实跑该 21 项。
3. **不得**声称真实 Harness / Keychain、**最低 macOS 14 实机**、业务桌面 E2E、**性能**、**完整 A11y**、**备份恢复**已验证。全部为「未运行」。
4. **不得**声称分支覆盖率已验证。**分支映射未产出**，`domain-export.json` 的 `branches` 为空数组。
   **不得**写「0 分支所以 100%」，**不得**用 region 89.87% 顶替 branch。分支 ≥ 85% 门槛**仍未验证**。
5. **不得**把 domain 行覆盖率以外的门槛（全体 Rust/TS 80%、application 真实代码覆盖）写成已测。
   `crates/application/src` 仍**无可执行行**，**不计 100%**。
6. **不得**把「修复完成但依赖未通过」写成通过。仍为 **BLOCKED**。
7. **不得**伪造或覆写任何历史 evidence；缺失的历史件**永久标记为缺失**。

### 0.3 domain 行覆盖率口径（引用 MM-R10 实测，非本包重测）

| 项 | 值 | 来源 |
|---|---|---|
| domain 行覆盖率 | **2016 / 2197 = 91.76%** | `docs/reviews/evidence/minimax-return/MM-R10/domain-coverage-summary.json`、`MM-R10-RESULT.md` §6 |
| 前值 | 1907 / 2197 = 86.80%（审计实测） | `docs/reviews/evidence/deep-audit-2026-10-08/coverage-tools/domain-coverage-summary.json` |
| region | 2227 / 2478 = 89.87% —— **region 不是 branch，不得顶替** | 同上 |
| application | 0 文件 / 0 行，**无可执行行，不计 100%** | 同上 |
| 分支 | **未验证**（`branch_coverage_verified: false`） | 同上 |

---

## 1. 逐包归档

> 每格的「来源」列给出可复现该结论的文件路径。凡 RESULT 文本与原始日志不一致处，**以原始日志为准**，并在 §6 登记差异。

### 1.1 MM-R01｜DA-01 DRAFT/投递 Team 一致性

| 项 | 内容 | 来源 |
|---|---|---|
| 判定 | **`SELF_CHECK_BLOCKED`** | `MM-R01-RESULT.md` 头部、§7 |
| 节点 | DEV-02-08，本包**不构成晋级** | `MM-R01-RESULT.md` §0 |
| 输入 SHA | `src/lib.rs` `7a3fe2a0d12d22e936dc5da096d27c9ec9ae97a4925677799bf20620d18da026`；审计起点 `8845a053…`（299 文件）为历史口径 | `MM-R01-RESULT.md` §2；`MM-R01/11-shas-and-scope.txt` |
| 输出 SHA | 写集 4 文件合并 `ae51bf1bb46eb32f890b82a27f859c36ec2353998527c81a1c24820087288725`；`0025` `e76eabe9…`；`src/lib.rs` `7a3fe2a0…`；`message_routing_team_integrity.rs` `ef602642…`；`message_delivery_integrity_forward_migration.rs` `a80ab0e4…` | `MM-R01/11-shas-and-scope.txt`；复核见 `MM-R09/14-recorded-output-sha-drift-check.txt` |
| 修改文件 | 新增 `migrations/0025_message_routing_team_integrity.sql`（239 行 / 6 trigger + 1 升级守卫表）；改 `crates/persistence-sqlite/src/lib.rs`（2 处增量）；新增 `tests/message_routing_team_integrity.rs`（6 用例）；追加 `tests/message_delivery_integrity_forward_migration.rs`（5 用例） | `MM-R01-RESULT.md` §3 |
| 覆盖范围 | `persistence-sqlite` 的 messages / delivery_attempts 写入路径 + `0025` 升级路径 | 同上 |
| RED | `cargo test -p persistence-sqlite --test message_routing_team_integrity --locked --offline` → **exit 101**，**2 passed / 4 failed**（`01-RED-…log`） | `MM-R01/01-RED-message-routing-team-integrity.log` |
| GREEN | 同命令 → **exit 0**，6 passed（`02-GREEN-…log`）；验收命令（两 target 合并）15 + 6 = 21 passed，exit 0（`05-acceptance-target-tests.log`） | `MM-R01/02-GREEN-…log`、`MM-R01/05-acceptance-target-tests.log` |
| 变异 | 1 组：UPDATE Team 守卫改 `WHEN 0 AND …` → exit 101，3 用例转红 | `MM-R01/04-mutation-disable-update-team-guard.log` |
| 全量套件 | exit 101，**173 passed / 2 failed** | `MM-R01/09-full-suite.log` |
| **失败（未闭环）** | 2 处版本计数断言（`MAX(version)==24`），写集外，Human 已裁决留给 Codex | `MM-COORD-migration-version-assertions.md` §4 |
| build/clippy/rustdoc/rustfmt | 全部 exit 0（`06`~`08`、`10`） | `MM-R01/06-build.log`…`08-rustdoc.log`、`10-rustfmt-check.log` |
| 未运行 | `cargo fmt --all`（明令禁止）；TS 检查（无 Rust→Schema/TS 变更）；canonical Mac 21 项在线门禁；干净安装；备份恢复；性能；A11y | `MM-R01-RESULT.md` §8 |
| 「因错误原因而绿」自陈 | 2 处装置故障（还原后陈旧二进制致 3 条假红；2 处反例被 `0017` 提前挡下） | `MM-R01-RESULT.md` §5.4 |

### 1.2 MM-R02｜DA-02 核心历史留存

| 项 | 内容 | 来源 |
|---|---|---|
| 判定 | **`SELF_CHECK_PASS`** | `MM-R02-RESULT.md` §9 |
| 节点 | DEV-02-05（关联 02-06/08/10），**不构成节点 DONE** | 同上 |
| 输入 SHA | `src/lib.rs` `7a3fe2a0…`；`0025` `e76eabe9…`；`permission_approval_integrity_forward_migration.rs` `ad63eb78…`；审计复现件 `6e2e1d27…`；审计种子 `3295f48c…`；审计 RED 日志 `110dc40e…` | `MM-R02/input-sha256.txt`；`MM-R02-RESULT.md` §2 |
| 输出 SHA | `0026` `4f303dc5…`；`src/lib.rs` `4611e7e9…`；`core_history_retention.rs` `8e5a2957…`；`permission_approval_integrity_forward_migration.rs` `570eac1d…` | `MM-R02-RESULT.md` §2；复核见 `MM-R09/14-…txt` |
| 修改文件 | 新增 `0026_core_history_retention.sql`（11 trigger）；改 `src/lib.rs`（2 处）；新增 `tests/core_history_retention.rs`（19 用例）；改 `tests/permission_approval_integrity_forward_migration.rs`（**只改测试期望**，`0020` 的历史放行行为被纠正） | `MM-R02-RESULT.md` §3 |
| 覆盖范围 | runs/reviews/artifacts/approvals/messages 五表 DELETE / INSERT-OR-REPLACE / 身份重指向三轴 + 升级零副作用 | 同上 |
| RED | `cargo test … --test core_history_retention`（0026 前）→ **exit 101**，**9 passed / 9 failed** | `MM-R02/red-before-0026.log` |
| GREEN | 验收命令（`core_history_retention` + `permission_approval_integrity_forward_migration`）→ **exit 0**，19 + 16 = **35 passed** | `MM-R02/acceptance-command.log` |
| 变异 | **7/7 全杀** | `MM-R02/mutation.log` |
| 全量套件 | exit 101，**192 passed / 2 failed**（均为已登记版本计数断言） | `MM-R02/full-suite.log` |
| build/clippy/rustdoc/rustfmt | 全部 exit 0 | `MM-R02/build.log`、`clippy.log`、`doc.log`、`fmt.log` |
| 未运行 | `cargo test --workspace`（越界，交 Codex 统一门禁）；canonical Mac 21 项在线门禁；干净安装；macOS 14 实机；真实 Harness/Keychain；备份恢复；性能；A11y；`cargo audit`/`cargo-deny` 在线校验（离线缓存非当日新鲜） | `MM-R02-RESULT.md` §8 |
| 「因错误原因而绿」自陈 | 3 处装置缺陷（FK 冒充留存守卫；`table_contents` 只读第一列；`approvals` 替换向量列数错误）→ 修正后**删除 4 个无守卫价值的 trigger** | `MM-R02-RESULT.md` §6 |
| **审计装置缺陷（事实登记）** | 审计 RED 日志中 `artifacts` 一条**不可复现**（INSERT 给 11 值 vs 表 12 列）。DA-02 结论本身不受影响 | `MM-R02-RESULT.md` §5.1；`MM-COORD-review-gate-2026-10-08.md` B-2 |

### 1.3 MM-R03｜DA-03 旧 Run 状态倒退

| 项 | 内容 | 来源 |
|---|---|---|
| 判定 | **`SELF_CHECK_BLOCKED`**（本包自身全绿；因发现第 3 处**未登记**版本计数断言而自判 BLOCKED） | `MM-R03-RESULT.md` 头部、§5.6 |
| 节点 | DEV-02-05，仍 BLOCKED | 同上 |
| 输入 SHA | `src/lib.rs` `4611e7e9…`；`0026` `4f303dc5…`；`0025` `e76eabe9…`；`run_lease_consistency_forward_migration.rs` `54476cba…`；审计复现件/种子/RED 日志同上 | `MM-R03/input-sha256.txt`；`MM-R03-RESULT.md` §2 |
| 输出 SHA | `0027` `58a689fc…`；`src/lib.rs` `fe3561c4…`；`run_transition_integrity.rs` `ba7da5da…`；`run_lease_consistency_forward_migration.rs` `54476cba…`（**与输入逐字节相同，实测无需修改**） | `MM-R03/output-sha256.txt`；`MM-R03-RESULT.md` §2 |
| 修改文件 | 新增 `0027_run_transition_integrity.sql`（**1 个** trigger，17 条边写成 1 个 `CASE` 谓词）；改 `src/lib.rs`（2 处）；新增 `tests/run_transition_integrity.rs`（15 用例） | `MM-R03-RESULT.md` §3 |
| 覆盖范围 | runs 状态迁移 UPDATE 轴（64 格全矩阵双向核对）+ 升级/失败路径 | 同上 |
| RED | 审计日志 `an existing RUNNING attempt was reset to QUEUED`；基线矩阵实测 56 个非对角格中 `0012` 仅拒 8 个 | `storage-boundaries-red-v2.log`；`MM-R03-RESULT.md` §5.1–5.2 |
| GREEN | 验收命令（`run_transition_integrity` + `run_lease_consistency_forward_migration`）→ **exit 0**，19 + 15 = **34 passed** | `MM-R03/acceptance-command.log` |
| 变异 | **4/4 全杀** | `MM-R03/mutation.log` |
| 全量套件 | exit 101，**206 passed / 3 failed** | `MM-R03/full-suite.log` |
| build/clippy/rustdoc/rustfmt | 全部 exit 0 | `MM-R03/build.log`、`clippy.log`、`doc.log`、`fmt.log` |
| **失败** | 3 处版本计数断言：2 处已登记 + `core_history_retention.rs:1274`（`COUNT(*) WHERE success=TRUE == 26`）为本包新发现 | `MM-R03-RESULT.md` §5.5；`MM-COORD-migration-version-assertions.md` §3b |
| **如实登记的开放缺口** | 终态→终态（12 格）仍可写；创建轴（INSERT 非 QUEUED）无守卫 | `MM-R03-RESULT.md` §3.1 第 4 点、§8 |
| 「因错误原因而绿」自陈 | 7 处（5 处装置缺陷 + 2 处断言值错误），含守卫遮蔽 `0012` 报错消息 | `MM-R03-RESULT.md` §7 |
| 未运行 | 同 R02（无 workspace 全量；无 canonical 21 在线门禁等） | `MM-R03-RESULT.md` |

### 1.4 MM-R04｜DA-04 百分号转义脱敏漏检

| 项 | 内容 | 来源 |
|---|---|---|
| 判定 | **`SELF_CHECK_PASS`** | `MM-R04-RESULT.md` §13 |
| 节点 | DEV-01-08，仍 BLOCKED，**本包不解除** | 同上 |
| 输入 SHA | `crates/observability/src/lib.rs` `9251f0edaa7fc203e7c02ad92cd5293f4325e830ecfc9d7f77d46e749b66091e`（**与审计复现副本逐字节相同**）；`tests/structured_logging.rs` `fcdfa6ca…` | `MM-R04/input-manifest.txt`；`MM-R04-RESULT.md` §2 |
| 输出 SHA | `src/lib.rs` `fc1d5bf801c37be327d434655c9be2874b1b2854683222098b419a1b268e7586`；`tests/structured_logging.rs` `2bed8f18…` | `MM-R04-RESULT.md` §2；复核见 `MM-R09/14-…txt` |
| 修改文件 | `crates/observability/src/lib.rs`（+230/−27，`Redactor.dangerous_forms` 改类型、新增 `PercentSegment`/`PercentForm`、**删除错误的 `percent.to_ascii_lowercase()` 形式**）；`tests/structured_logging.rs`（+239/−0，11 个新用例）。**未新建** `audit_percent_case.rs` | `MM-R04-RESULT.md` §4 |
| 覆盖范围 | observability 脱敏派生形式（8 种），含实际 `JsonSubscriber` 字符串输出断言 | 同上 |
| RED | `red-1-migrated-audit-tests.log` → **exit 101**，**8 passed / 2 failed**；`red-2-full-new-suite.log` → **exit 101**，**10 passed / 7 failed** | 两份日志 |
| GREEN | `cargo test -p observability --locked --offline` → **exit 0**，**18 passed** | `MM-R04/test.log` |
| 变异 | M1 杀（7 failed）、M2 **首次存活**→补反例后杀、M3 杀（2 failed）、**M4 存活**（`is_char_boundary` 非承重件，已给出不可达证明） | `MM-R04/mutation-reversal.log`；`MM-R04-RESULT.md` §6.3、§9.2 |
| **越界自查（失败）** | `cargo test --workspace` → exit 101，唯一失败 `a_later_task_block_does_not_invalidate_historical_attention_resolution`（`left: 25 / right: 24`）。已举证与本包无关（换回修复前字节仍同样失败；工作区无 crate 依赖 observability） | `MM-R04/workspace-regression-check.log`；`MM-R04-RESULT.md` §8 |
| build/clippy/rustdoc/rustfmt | 全部 exit 0；`fmt-check.log` 为**修复前**差异留档（exit 1），`fmt.log` 为修复后 exit 0 | `MM-R04/build.log`…`fmt.log` |
| **行为收紧 / 放宽（如实登记，待 Codex 判定）** | 收紧：`audit%2donly%2faab` 不再脱敏（纠正误伤）。放宽：非字母数字字节可字面或 `%XX`（**derived reading**） | `MM-R04-RESULT.md` §7；`MM-COORD-review-gate-2026-10-08.md` C-1/C-3 |
| **同族未修风险 R1** | 纯 hex 整串大小写混合仍可绕过（DA-04 明确限定 percent） | `MM-R04-RESULT.md` §11 R1；`MM-COORD-review-gate-2026-10-08.md` C-2 |
| 未运行 | canonical Mac 21 项在线门禁；TS lint/typecheck/test/build/contracts；`cargo audit`/许可证；Gitleaks 全 refs / hosted CI；覆盖率门槛 | `MM-R04-RESULT.md` §10 |
| lint:ts | **exit 1**，唯一错误 `docs/reviews/evidence/deep-audit-2026-10-08/replay_env_manifest.mjs:24:22 no-unsafe-finally`（**只读历史证据**，不在写集内）。⚠ **本轮该状态已变化，见 §5.1** | `MM-R04/13-pnpm-lint-ts.log` |

### 1.5 MM-R05｜DA-05 严格 JSON-RPC stdout 边界

| 项 | 内容 | 来源 |
|---|---|---|
| 判定 | **`SELF_CHECK_BLOCKED`**（生产修复完成并验证；11 个既有测试因 fixture 传输层暂红） | `MM-R05-RESULT.md` §12 |
| 节点 | DEV-03-03（关联 03-02/04/05），**本包不解除任何 BLOCKED** | 同上 |
| 输出 SHA | `crates/adapter-supervisor/src/protocol.rs` `66c3633fbe791cea8ab0d94474a771965589b22ff0a19d607c6d5d8879247e9c`；`adapters/mock/index.mjs` `cc75198c1f3f8a4485bfba96a03329e58bdaea7f082cc4a21ea8d29d2ca51ceb` | `MM-R05-RESULT.md` §1；复核见 `MM-R09/14-…txt` |
| **归档缺口** | `crates/adapter-supervisor/src/lib.rs`（本包新增 6 个测试与辅助函数）**RESULT 未登记其 SHA**。当前实测 `500ae3f41157190ca170d2ded06a88780ea7ce67ee8d24b9da43c81df5b13b26`，但**本包会话内是否为此值无法从既有证据复核** | `MM-R05-RESULT.md` §3；本包 `14-…txt` |
| 修改文件 | `protocol.rs`：**删除五处** `running `/`test `/空白跳过（行 1816/1877/1963/2051/2115 → 1807/1853/1927/2003/2054），四处响应读取器去掉因此失效的 `loop`；`adapters/mock/index.mjs`：**纯 opt-in** 噪声注入 `MOCK_STDOUT_NOISE` / `MOCK_STDOUT_NOISE_STAGE`（默认路径零变化）；`src/lib.rs`：仅测试代码 +6 用例 | `MM-R05-RESULT.md` §2–3 |
| 覆盖范围 | adapter-supervisor 五处 stdout 读入口 → 既有 `ADAPTER_PROTOCOL_VIOLATION`（`-32099`），**未新造错误码/状态/权限/wire 字段** | 同上 |
| RED | 独立目录还原五处跳过后：malformed 与 control **均 exit 0 且握手成功** = 绕过成立 | `MM-R05/10-PRE-FIX-RED.log` |
| GREEN | malformed → exit 101（`-32099`）；control → exit 0。新增验收 6 passed / 0 failed | `MM-R05/07-repro-red-green.log`、`11-stdout-noise-acceptance.log` |
| 变异 | 新测试放到修复前解析器 → **5 failed**（前两次尝试是装置故障伪装成通过，已如实登记） | `MM-R05/12-MUTATION-PREFIX-DETECTED.log`；`MM-R05-RESULT.md` §6 |
| **失败** | `cargo test -p adapter-supervisor` → **exit 101，79 passed / 11 failed**。根因在 **libtest fixture 传输层**，不在产品解析逻辑 | `MM-R05/01-cargo-test-after-fix.log`；`MM-R05-RESULT.md` §5、§9 |
| 已证伪的方案 | 去掉 4 处 `--nocapture` → 子进程 stdout **逐字节相同**（各 7 行），建议已撤回 | `MM-R05/09-nocapture-removal-disproof.log` |
| build/clippy/rustdoc/fmt | 全部 exit 0（`cargo fmt -p adapter-supervisor -- --check`，**仅本包文件**） | `MM-R05/02`~`06` 日志 |
| **待 Codex 签发写集扩展** | 方案 A（推荐）：新增 `crates/adapter-supervisor/src/bin/fixture_adapter.rs`，**无需改 Cargo.toml/lockfile** | `MM-R05-RESULT.md` §10；`MM-COORD-review-gate-2026-10-08.md` A-2 |
| 未运行 | canonical Mac 21 项在线门禁；覆盖率；完整在线验收 | `MM-R05-RESULT.md` §7 |
| 交付测试自陈隐患 | MM-R05 新用例含 **node 不可用即静默 return** 的假通过模式（本端实测当前环境未触发）；总控要求补「node 不可用必须失败」断言 | `MM-COORD-review-gate-2026-10-08.md` §F |

### 1.6 MM-R06｜DA-06 + 两条机制缺口

| 项 | 内容 | 来源 |
|---|---|---|
| 判定 | **`SELF_CHECK_PASS`** | `MM-R06-RESULT.md` §8 |
| 节点 | DEV-01-05（关联 01-03），**不构成节点 DONE** | 同上 |
| 输入 SHA | `scripts/ci/local-gates.mjs` `84bd09eb…`；`local-gates.test.mjs` `17f01e27…`；`verify-workflows.mjs` `6f94f05f…`；`.github/workflows/ci.yml` `e118bee1…`（前三者中前两文件在 HEAD 中未被跟踪，SHA 来自审计期工作树快照） | `MM-R06/input-output-sha256.tsv`；`MM-R06-RESULT.md` §1 |
| 输出 SHA | `4cfdd3da…` / `35f04f45…` / `29eacc66…` / `88a3eb5c…`（**逐一复核一致**） | `MM-R06/output-sha256.txt`；本包 `14-…txt` |
| 修改文件 | 上述 4 文件：`listIgnoredBuildInputPaths()`（DA-06 修复）、`maxBuffer` 提到 64 MiB、`collectGateInputs()` 合一性哈希、`companionCheckDefinitions()`（`history-secret-scan` + `command`）、`assertMatchingCompanionEvidence()`、`assertCompletedGateRun()`；`ci.yml` 新增 `history-secret-scan` 与 `tooling-guards` 两个 job | `MM-R06-RESULT.md` §2 |
| 覆盖范围 | Gate 输入同一性 + 全 refs 历史秘密扫描 + CI 契约 | 同上 |
| RED | 审计脚本 `replay_env_manifest.mjs` **未运行**（会写入只读历史证据目录）；改为临时目录抽取同反例：两份不同 `.env.production` 的 legacy manifest 哈希同为 `196ed327…`，`input_manifest_still_blind: true` | `MM-R06/red-da06-reproduction.log`；`MM-R06-RESULT.md` §4 |
| GREEN | 修复后三段 Gate 输入同一性互不相同；`values_present_in_evidence: false` | 同上 |
| 合法对照 | 无 ignored dotenv 的隔离仓库，两次 `collectGateInputs` 哈希相同且稳定；`.env.example` 仍计入 manifest | `MM-R06-RESULT.md` §4 |
| 变异 | **10/10 全杀**（R1–R10） | `MM-R06/red-mutation-summary.tsv` + `MM-R06/R{1..10}*.log` |
| gate 测试 | `node --test scripts/ci/local-gates.test.mjs` → **exit 0，22 pass / 0 fail**（既有 12 + 新增 10） | `MM-R06/accept-01-gate-tests.log` |
| 已验证（其余） | `typecheck` 0；`verify:guards` 0；`local-gates.mjs --dry-run` 0；`verify-workflows.mjs` 0；`prettier --check`（4 文件）0；`eslint`（3 mjs）0；companion 探针 0；Gitleaks 工作树/全 refs **0 / 0** | `MM-R06/accept-02`…`accept-08`、`companion-checks-probe.json`、`secret-scan-both-scopes.log` |
| **失败** | `pnpm run lint:ts` → **exit 1**，唯一错误 `docs/reviews/evidence/deep-audit-2026-10-08/replay_env_manifest.mjs:24:22 no-unsafe-finally`。改动前后各复现一次，位置与数量完全一致。⚠ **本轮该状态已变化，见 §5.1** | `MM-R06/baseline-lint-ts.log`、`accept-02-lint-ts.log` |
| canonical 21 完整性 | `canonicalGateCommandCount` 是字面量 `21`，列表长度不符即抛错（源码级断言 + R9 变异）。**但 21 项本身未实跑** | `MM-R06-RESULT.md` §5；`accept-05-dry-run.log` |
| **未运行** | **完整 21 项 canonical 门禁实跑**（含 `pnpm install --frozen-lockfile`、`pnpm audit`、`cargo audit`、workspace Rust 测试/构建/clippy）；`pnpm audit`/`cargo audit` 在线判定；RustSec 在线；GitHub Actions 实跑与 branch protection 服务端强制；`.github/workflows/ci.yml` 未在 GitHub 上运行 | `MM-R06-RESULT.md` §6 |
| 剩余风险 | ignored 摘要的敏感度取舍；`--log-opts=--all` 语义（不含 reflog/dangling）；配套检查双跑代价；maxBuffer 64 MiB fail-closed | `MM-R06-RESULT.md` §7 |

### 1.7 MM-R10｜领域边界/序列化/拒绝测试补充 + 覆盖率离线重测

| 项 | 内容 | 来源 |
|---|---|---|
| 判定 | **`SELF_CHECK_PASS`**（domain 行覆盖率实测达标；分支仍「未验证」） | `MM-R10-RESULT.md` 头部、§9 |
| 节点 | **DEV-02-11 未标 DONE** | `MM-R10-RESULT.md` §7.5 |
| 写入文件 | 新增 `crates/domain/tests/deep_audit_coverage.rs`（**40 个测试**）`ccd5a7594eca5d7ea44a1896cd9a72ee906b9e359fcfbad4b4653ff89f6ededf`；`crates/domain/src/**`（16 份）**未修改** | `MM-R10-RESULT.md` §2、§8 |
| 修改文件 | **无 production 代码改动**。仅新增 1 个测试文件 + 本包 evidence 目录 | 同上 |
| 覆盖范围 | 16 份 domain src：Run/Task/权限矩阵/权限求交/幂等/Run 终结/Approval/值对象 + 跨切面 wire 名 | `MM-R10-RESULT.md` §6 |
| GREEN | `cargo test -p teamspace-domain --locked --offline` → **exit 0，287 passed / 0 failed**，18 个 test target 全 ok | `MM-R10-RESULT.md` §5；**本轮复现见 `MM-R09/08-cargo-test-domain.log`** |
| 变异 | **16/16 全部被检出**（M1–M16，全部在 `/tmp` 副本进行，从未改动工作区 production 代码） | `MM-R10-RESULT.md` §4 |
| 自查发现的测试自身错误 | 3 处（`transition()` 对无表行命令也报 `InvalidTransition`；Approval 状态集与顺序无关；§3 只在 `STARTING` 行印 `RUN_CANCEL_NOT_CONFIRMED`） | `MM-R10-RESULT.md` §4 |
| 覆盖率实测 | **2016/2197 = 91.76%**（+109 行，门槛只需 +71）。分母口径：全部测试 + 固定 production src，含未执行函数映射，**不排除任何低覆盖生产文件**；tests/examples/dependencies 不计入分母 | `MM-R10/domain-coverage-summary.json`；`MM-R10/input-manifest.json` |
| 测量工具 | 编译 `rustc 1.98.0 (88d9e12ae 2026-08-18)`（LLVM 22.x）产 profile/covmap；读取工具 **Apple LLVM 21.0.0** `llvm-profdata`/`llvm-cov`。`RUSTFLAGS="-C instrument-coverage -C link-dead-code"`，`LLVM_PROFILE_FILE` 落在**本包新目录**，`CARGO_TARGET_DIR` 为**本包新 target**，46 份 raw profile，合并 `domain.profdata`（374,200 bytes）。**旧 profile/log/summary 未被覆盖** | `MM-R10-RESULT.md` §6；`MM-R10/domain-command.json` |
| 固定输入 SHA | 根 manifest `e2540025…`；`Cargo.lock` `5822d1d1…`；`rust-toolchain.toml` `495c69d1…`；domain `src/` 树 `db929d01…`(16)；domain `tests/` 树 `4cfc1fac…`(16)；application `src/` 树 `ca2208b8…`；33 份 `.rs` 逐字节校验一致 | `MM-R10/input-manifest.json` |
| **失败** | 无。本包命令 1/2/3 全部 exit 0 | `MM-R10-RESULT.md` §5 |
| **未验证（不得写成通过）** | 分支 85%（**branch 映射未产出**，`branches` 为空数组）；application 真实代码覆盖（**无可执行行**）；全体 Rust/TS 80%；application 侧集成 / SQLx 迁移 / Adapter 契约 / 桌面 E2E | `MM-R10-RESULT.md` §6、§7 |
| 归档自陈 | 变异测试日志**未逐条存盘**为正式 evidence 文件（脚本化执行，结论记录在 RESULT §4） | `MM-R10-RESULT.md` §7.8 |
| **副作用（新增，见 §5.1）** | 覆盖率 runner 的 source-copy 把 `apps/desktop/dist/` 构建产物一并复制进 `MM-R10/source-copy/`，导致仓库级 `eslint .` 崩溃 | `MM-R10/source-copy/apps/desktop/dist/assets/`；`MM-R09/12-…log`、`13-…log` |

---

## 2. 跨包汇总

### 2.1 测试结果汇总（**逐条标注来源，禁止合并表述**）

| 范围 | 已验证（实际执行） | 失败 | 未运行 | 来源 |
|---|---|---|---|---|
| `persistence-sqlite` 全量 | **206 passed / 3 failed**，exit 101 | 3 处版本计数断言（见 §2.2） | — | `MM-R09/09-preexisting-persistence-sqlite.log`（本轮复现）；R03 当时同数 `MM-R03/full-suite.log` |
| `adapter-supervisor` 全量 | **79 passed / 11 failed**，exit 101 | 11 项（fixture 传输层，根因非产品） | — | `MM-R09/11-preexisting-adapter-supervisor.log`（本轮复现）；`MM-R05/01-…log` |
| `teamspace-domain` 全量 | **287 passed / 0 failed / 1 ignored**，exit 0 | 无 | — | `MM-R09/08-cargo-test-domain.log`（本轮复现）；`MM-R10-RESULT.md` §5 |
| `observability` | **18 passed / 0 failed**，exit 0 | 无 | — | `MM-R04/test.log`（R04 会话内） |
| `cargo test --workspace` | — | — | **未运行**（R04 曾越界自查并 exit 101，**不是全绿**） | `MM-R04/workspace-regression-check.log` |
| local-gates 脚本测试 | **22 pass / 0 fail**，exit 0 | 无 | — | `MM-R06/accept-01-gate-tests.log` |
| TypeScript 全部现有分层 | — | — | **本轮未运行**。审计记录当前为 **36 passed**（六组 5/1/21/1/4/4），其中部分为工具链占位 | `docs/reviews/evidence/deep-audit-2026-10-08/ts-tests.log`；见 §4.4 |
| canonical Mac **21** 项在线门禁 | — | — | **未运行**（R06 仅跑 `--dry-run`） | `MM-R06/accept-05-dry-run.log`；`MM-R06-RESULT.md` §6 |
| Gitleaks 工作树 / 全 refs | exit 0 / exit 0，**0 命中**；非 shallow，6 refs / 4 commits | 无 | — | `MM-R06/secret-scan-both-scopes.log` |
| RustSec / 许可证 | — | — | **仅缓存命中且非当日新鲜**（公告库非本日，index 锁不可读告警仍在）。**不能等同在线零漏洞** | `docs/reviews/evidence/deep-audit-2026-10-08/rust-advisories-cached.log`、`rust-licenses-cached.log` |
| 真实 Harness / Keychain | — | — | **未运行** | 无证据 |
| 最低 macOS 14 实机 | — | — | **未运行** | 无证据 |
| 完整 A11y / 性能 / 备份恢复 | — | — | **未运行** | 无证据 |
| 业务桌面 E2E | — | — | **未运行**（`integration`/`e2e` 恒真断言不计验收） | 审计 §4 |
| GitHub Actions 实跑 / branch protection | — | — | **未运行**（仅本地静态契约 `verify-workflows.mjs`） | `MM-R06/accept-06-verify-workflows.log` |

### 2.2 3 处版本计数断言（Human 已裁决留给 Codex，本包不修）

| # | 文件:行（**本轮实测 panic 行**） | 断言 | 实测 | 来源 |
|---|---|---|---|---|
| 1 | `crates/persistence-sqlite/tests/core_history_retention.rs:1274` | `COUNT(*) FROM _sqlx_migrations WHERE success=TRUE == 26` | `left: 27, right: 26` | `MM-R09/09-…log`；`MM-COORD-migration-version-assertions.md` §3b |
| 2 | `crates/persistence-sqlite/tests/notification_execution_integrity.rs:470` | `MAX(version) == 24` | `left: 27, right: 24` | 同上 |
| 3 | `crates/persistence-sqlite/tests/outbox_default_forward_migration.rs:146` | `MAX(version) == 24` | `left: 27, right: 24` | 同上 |

> **行号差异登记**：本包任务书给出的是 `core_history_retention.rs:1275` 与 `outbox_default_forward_migration.rs:150`。
> **本轮实测的 panic 行是 1274 与 146**，与 `MM-COORD-migration-version-assertions.md` §3b 记载的 1275/150 **不完全一致**。
> 以原始日志的 panic 行为准；差额应为断言宏所在行与语句行的偏移。**三处失败性质完全相同：纯版本计数，与行为无关。**

Human 裁决（2026-10-08）：**保持写集不扩张，不修改，留给 Codex 在最终统一门禁中处理。** 本包遵办，未改期望值、未加 `#[ignore]`、未关闭任何外键或守卫。

### 2.3 11 个 adapter-supervisor 失败（待 Codex 签发写集扩展）

`handshake_illegal_semver_is_rejected_with_adapter_incompatible_code`、`handshake_incompatible_version_is_rejected_with_adapter_incompatible_code`、`handshake_rpc_error_is_properly_mapped`、`handshake_success_negotiates_capabilities_and_auth_state`、`handshake_timeout_is_diagnosed_as_unavailable`、`handshake_unexpected_process_exit_is_diagnosed_with_status_and_stderr`、`health_check_degraded_reports_diagnostic_codes`、`health_check_process_exit_diagnosed_with_status_and_stderr`、`health_check_success_reports_healthy_status_and_timestamp`、`health_check_timeout_diagnosed_as_timed_out`、`run_normal_execution_flow_via_fixture`。

失败体统一为 `ProtocolViolation … "Malformed JSON: unexpected end of JSON input"`。**未被删除、未加 `#[ignore]`、未改预期值、未放宽解析。** 来源：`MM-R05-RESULT.md` §9；`MM-R09/11-preexisting-adapter-supervisor.log`。

### 2.4 输入 SHA 漂移复核

各包 RESULT 登记的 **20 个输出文件 SHA-256 与当前工作树逐一一致，零漂移**。完整清单：`MM-R09/14-recorded-output-sha-drift-check.txt`。
唯一未登记项：`crates/adapter-supervisor/src/lib.rs`（见 §1.5「归档缺口」）。

---

## 3. 覆盖率：引用 MM-R10，不重复测量

**本包未重跑覆盖率测量。** 理由与代价：

- MM-R10 已在新证据目录完成完整离线重测（Apple LLVM `llvm-profdata`/`llvm-cov`，新 runner 副本、新 source-copy、新 profile、新 target，旧 profile/log/summary 未被覆盖）。重跑需重新 instrument 编译 domain + application 全部 18 个 coverage executable 并消费 4.9 MB export，**属高成本重复劳动且不产生新信息**。
- **但本包改了 domain 的注释**，因此必须重新固定相关 SHA 以证明分母未受影响 —— 见 §3.2。

### 3.1 为什么注释不影响 llvm-cov 的行覆盖率分母

llvm-cov 的行覆盖率分母来自 **covmap 中记录的可执行行**（`DomainCoverage` 的 line 计数），**注释不产生任何可执行行映射**。本包用更强的方式直接证明：**同一份源码在订正前后，rustc 输出的 LLVM IR 模块逐字节相同**。

| 证明项 | 值 | 来源 |
|---|---|---|
| 订正前 LLVM IR SHA-256 | `c70b72f673ccb70744560a8bd959c5b417fdb77176a5c93d2c66ab1e0c5642b3` | `MM-R09/lib.PRE-EDIT.ll`、`05-llvm-ir-invariance.txt` |
| 订正后 LLVM IR SHA-256 | `c70b72f673ccb70744560a8bd959c5b417fdb77176a5c93d2c66ab1e0c5642b3`（**完全相同**） | `MM-R09/lib.POST-EDIT.ll` |
| `cmp` 结果 | **IDENTICAL** | `MM-R09/05-llvm-ir-invariance.txt` |
| IR 指令行数 | 21699 → 21699 | 同上 |
| 复现命令 | `cargo rustc -p teamspace-domain --locked --offline -- --emit=llvm-ir` | 本包执行记录 |

> **为什么不用 rlib 做证明（如实登记）**：`libteamspace_domain.rlib` 在订正前后**确实不同**
> （订正前 `3f14be0f…`，订正后 `a22872d8…`）。原因是 **rustc 把 `#[doc]` 字符串编入 crate metadata**
> （`strings` 验证：订正前 rlib 含 `random[1]`，订正后含 `random[2]`），因此 rlib 字节差异**由文档字符串解释，
> 不能作为可执行行变化的判据**。本包据此改用 LLVM IR——它不含文档注释，才是正确的判据。
> 见 `MM-R09/01-rlib-sha-before.txt`、`03-rlib-sha-after.txt`、`04-executable-line-invariance.txt`。

### 3.2 重新固定的 SHA（与 MM-R10 实测输入的差异）

| 项 | MM-R10 实测输入 | 本包订正后 | 差异 |
|---|---|---|---|
| domain `src/` 树（16 文件） | `db929d0158175e2d2885dce6445e9efbcb7bb5c2636e66b3b2d811f9d40155aa` | `5626cb9df788838feeebd055407fcf56e5da732c63becfb6c4d7d85e1cc07792` | **变**（2 份文件的注释字节） |
| domain `tests/` 树（16 文件） | `4cfc1fac9765ab3c7a6f4033617bc887facb32c62e3b49b607b0fccf4a584cfe` | `4cfc1fac9765ab3c7a6f4033617bc887facb32c62e3b49b607b0fccf4a584cfe` | **不变** |
| application `src/` 树 | `ca2208b863927f7879b925f44953f546181ff0ad858884ddc836a9504e847f19` | `ca2208b863927f7879b925f44953f546181ff0ad858884ddc836a9504e847f19` | **不变** |
| 根 `Cargo.toml` | `e2540025f0b7fcf0dac8a6ba82246f9f7c259a3375783653bdc94a3ff7bebe85` | 同 | **不变** |
| `Cargo.lock` | `5822d1d11a79ccb29a544da31033a24d99f05b35217467ae83ab434a913c5aa8` | 同 | **不变** |
| `rust-toolchain.toml` | `495c69d1db93f25f53e3d0d2d40d812313366cc368de317871c2ace6c6ffafa4` | 同 | **不变** |

来源：`MM-R09/10-post-edit-input-sha.txt`（用 MM-R10 runner 同一 `tree_digest` 算法重算）。
**结论：domain src 树字节变了，但由 §3.1 的 IR 恒等可证可执行行集合未变，故 2197 分母与 2016 覆盖行均不受影响，91.76% 结论继续有效。**

### 3.3 本轮**明确未声称**的覆盖率事项

- **application 仍无可执行行**（`crates/application/src` 为占位代码）→ **不计 100%**。
- **全体 Rust / TS 80% 门槛未测**（本轮只测 domain）。
- **分支覆盖率仍未验证** → 不写「0 分支所以 100%」，不用 region 89.87% 顶替。
- **DEV-02-11 未标 DONE**。

---

## 4. 缺失历史证据的新映射（**不伪造、不覆写历史**）

### 4.1 DEV-02-08 外部 resign-final JSON —— **永久标记为缺失**

| 项 | 值 |
|---|---|
| 路径 | `/Users/chenzhiqing/.minimax/teamspace-audits/dev-02-08/evidence/wave2-macos-arm64-dev0208-resign-final.json` |
| 状态 | `exists: false`，`not_reconstructed_or_fabricated: true` |
| 处置 | **永久标记为缺失。** 该件属**外部**证据（Human 侧审计目录），不在本仓库写集内，**本包不创建、不重建、不代填、不以任何当前证据冒充之。** |
| 来源 | `docs/reviews/evidence/deep-audit-2026-10-08/historical-external-evidence-check.json`（SHA `caf92f5c…`）；审计 §5 |

### 4.2 归档本地死链 → 当前替代证据的新映射

| 项 | 值 |
|---|---|
| 死链来源 | `docs/archive/v1-planning/审计整改说明.md:4` |
| 死链目标 | `../PRD-Figma审计报告.md`（解析为 `docs/PRD-Figma审计报告.md`，**不存在**） |
| **当前替代证据** | **`docs/archive/v1-planning/PRD-Figma审计报告.md`**（**与来源文件同目录**），SHA-256 `253b7ad2348aaa4d3d31c26d05ebd8da9514c7dbe89eb5cdee87c683e559fb0d` |
| 死因 | 目标文件确实存在，**但链接多写了一个 `../`**，指向了上一级目录。这是**相对路径错误，不是内容缺失** |
| 处置 | 该归档文档**不在本包写集内**，本包**只建立映射、不修改死链本身**。修法（供 Codex）：`../PRD-Figma审计报告.md` → `./PRD-Figma审计报告.md` |
| 盘点来源 | `docs/reviews/evidence/deep-audit-2026-10-08/inventory/dead-links.json`（SHA `9c750750…`，`markdown_dead_links: 1`） |

### 4.3 历史本地 provenance 引用缺失 → 当前替代证据的新映射

| 项 | 值 |
|---|---|
| 引用来源 | `docs/reviews/WAVE-2-DEV-02-07-CLOSURE-2026-09-29.md:84` |
| 缺失目标 | `evidence/tool-provenance.json`（解析为 `docs/reviews/evidence/tool-provenance.json`，**不存在**） |
| **当前替代证据（工具来源/工具链身份）** | ① `docs/reviews/evidence/deep-audit-2026-10-08/tool-binary-hashes.json`（SHA `fa990c6f75455846f43712de684a50729096fdb3715aecb13b521ccc0072bb3f`）<br>② `docs/reviews/evidence/deep-audit-2026-10-08/tool-versions.log`（SHA `9eae9821400d8735771be6f683e791f8271a89b6dae9634ebfd7cde476195d2f`）<br>③ `docs/reviews/evidence/deep-audit-2026-10-08/toolchains.log` |
| **当前替代证据（source provenance）** | ① `.../run-admission-2026-10-08/previous-source-provenance.json`（SHA `569c35fa…`）<br>② `.../outbox-default-2026-10-08/batch-source-provenance.json`（SHA `bdc0c10f…`）<br>③ `.../deep-audit-2026-10-08/main-messaging-replay-provenance.json`（SHA `24a4453b…`） |
| 处置 | 缺失的旧件**不重建**。WAVE-2 closure 记录中列出的 gitleaks 8.30.0 / cargo-deny 0.18.6 / cargo-audit 0.22.0 版本与二进制摘要，**当前可核验的落点是上述 ①②**；**本包不声称二者等价**，只登记映射关系 |
| 盘点来源 | `docs/reviews/evidence/deep-audit-2026-10-08/inventory/evidence-references.json`（SHA `289f77e3…`，`exists: false`） |

### 4.4 旧 r13 证据 —— **不覆盖当前输入，不得当作当前结论**

| 项 | 值 |
|---|---|
| 旧件 | `wave2-macos-arm64-dev0307-fencing-r13.json`，摘要 `ddf55717…f47`（记录称 21 条命令全部 exit=0） |
| **当前输入摘要** | `b3dcb135…fb` |
| 事实 | 对当前 313 个输入逐项比较**仅 3 个文件变了**（`run_lease_consistency_forward_migration.rs`、`docs/development/DEVELOPMENT_NODES.md`、DEV-03-07 终审）。按 ADR-0005 的失效规则，**r13 不能证明当前最终输入** |
| 登记 | 审计 §5 已确认。`docs/coordination/README.md:20` 与 `docs/coordination/REMEDIATION-PLAN-2026-10-07.md:36` 均要求**不沿用旧 r13** |
| 处置 | 本包**不以 r13 支撑任何当前结论**，仅登记其失效 |

### 4.5 TypeScript 测试总数 —— **事实记录，不归 MiniMax**

| 来源日志 | 六组 | 实际求和 | 原始声明 |
|---|---|---|---|
| `docs/reviews/evidence/project-review-2026-10-07/typescript-tests.log` | 4/1/21/1/4/4 | **35** | 37 |
| `docs/reviews/evidence/remediation-2026-10-07/typescript-tests.log` | 5/1/21/1/4/4 | **36** | 38 |
| `docs/reviews/evidence/deep-audit-2026-10-08/ts-tests.log` | 5/1/21/1/4/4 | **36** | —（当前） |

来源：`docs/reviews/evidence/deep-audit-2026-10-08/ts-count-correction.json`（SHA `6654f019…`）；审计 §5。
**这是总控汇总口径错误，审计已明确「不将该错误归给 MiniMax」。原始日志与历史报告保持不变，仅作事实记录。**

---

## 5. 本轮新发现（不属既有登记）

### 5.1 `pnpm run lint:ts` 已从「exit 1 / 1 个错误」恶化为「exit 2 / 崩溃」，并**遮蔽**了原错误

| 项 | 实测 |
|---|---|
| 本轮 `pnpm run lint:ts` | **exit 2**，`Oops! Something went wrong!` |
| 崩溃点 | `Error while loading rule '@typescript-eslint/await-thenable': You have used a rule which requires type information…` |
| 崩溃文件 | `docs/reviews/evidence/minimax-return/MM-R10/source-copy/apps/desktop/dist/assets/index-Dp_6cwvq.js`（193,648 字节的 Vite 构建产物） |
| 根因 | `eslint.config.mjs:12` 的全局 ignore 是 `apps/desktop/dist/**`，**只匹配仓库内规范路径**。MM-R10 的覆盖率 runner 把整棵源码树复制进 evidence 目录时**连构建产物一起复制**，其副本路径不匹配该 ignore 模式，于是 `eslint .` 爬进一个 193 KB 的打包 JS 并因缺少 typed-lint 配置而抛错 |
| **被遮蔽的既有错误仍存在** | 单独 lint 该文件得 **exit 1 / 1 error**：`docs/reviews/evidence/deep-audit-2026-10-08/replay_env_manifest.mjs:24:22 no-unsafe-finally`（PROBE A） |
| 因果隔离证明 | 单独 lint 崩溃文件得 **exit 2**（PROBE B） |
| 来源 | `MM-R09/12-preexisting-lint-ts.log`、`MM-R09/13-lint-crash-root-cause-probe.log` |
| 影响 | ① R04/R06 记录的「lint:ts exit 1，1 个错误」**已不是当前可复现状态**；② 任何依赖 `pnpm run lint:ts` 的门禁现在会在**配置错误**而非 lint 违规上失败；③ 这是**新引入的回退**，此前不存在 |
| 处置 | **本包不修**：`eslint.config.mjs`、MM-R10 evidence 目录、`apps/desktop/dist/**` 均**不在本包写集内**。候选修法（供 Codex 裁定）：① 在 `eslint.config.mjs` 增加 `docs/reviews/evidence/**` 的 ignore；② 或让 evidence 目录不复制构建产物。**两者都会改变仓库级 lint 作用域，属需签发写集的动作** |

> 该发现不影响 `teamspace-domain` / `persistence-sqlite` / `adapter-supervisor` 的任何结论，也不改变本包两处注释订正的可执行行恒等证明。

---

## 6. RESULT 文本与原始日志的差异登记（以日志为准）

| # | 包 | RESULT 文本 | 原始日志实测 | 处置 |
|---|---|---|---|---|
| 1 | MM-R04 | `red-1-migrated-audit-tests.log`「10 passed / 2 failed」 | **8 passed / 2 failed**（共 10 个用例） | 以日志为准；RESULT 表述把「总数」写成了「passed」。**不影响失败结论** |
| 2 | MM-R02 / R03 / 本包 | outbox 断言行号 150；core_history 断言行号 1275 | **146** / **1274** | 以 panic 行为准，见 §2.2 |
| 3 | MM-R05 | 未登记 `crates/adapter-supervisor/src/lib.rs` 的输出 SHA | 本包实测 `500ae3f4…` | **归档缺口**：该 SHA 无法用既有证据在本包会话内复核，见 §1.5 |
| 4 | MM-R10 | 变异测试 16/16 被检出 | 变异日志**未逐条存盘**为 evidence 文件 | 以 RESULT §4 的清单为准，但**变异原始日志不可独立复核**，见 §1.7「归档自陈」 |

---

## 7. 节点状态（**本包未修改任何正式状态**）

| 节点 | 当前口径 | 依据 |
|---|---|---|
| DEV-01-05、01-08、02-05、02-06、02-10、03-03 | 审计 §5 **直接重开**（仍 BLOCKED） | 审计 §5；`docs/reviews/evidence/deep-audit-2026-10-08/final-node-status-proof.json`（SHA `ccd73cd9…`） |
| DEV-02-11、02-12、03-02、03-04、03-05 | 按**真实依赖暂停** | 同上 |
| 正式准入表 | **23 DONE / 18 BLOCKED / 60 PLANNED，共 101**。这是**文档/依赖状态，不是产品完成率** | 同上（`final-node-status-proof.json`，`source_sha256: 7eb61dbd…`） |
| **准入裁定权** | **Codex**。本包**不自判 DONE**、不解除任何 BLOCKED、不启动依赖包 | 任务包停止条件；各 RESULT 尾部声明 |

**「修复完成但依赖未通过」仍为 `BLOCKED`**：MM-R01、MM-R03、MM-R05 三包即属此类——实现与本包证据已绿，阻塞在写集外/依赖上。

---

## 8. 复现指引

```bash
export PATH="/Users/chenzhiqing/.minimax/toolchain/bin:$PATH"; export CARGO_NET_OFFLINE=true
cd /Users/chenzhiqing/Documents/MyVault/协作工作

# 1) 各包登记的输出 SHA 是否仍与工作树一致（应全部一致、零漂移）
cat docs/reviews/evidence/minimax-return/MM-R09/14-recorded-output-sha-drift-check.txt

# 2) 可执行行恒等证明（本包两处注释订正）
cmp docs/reviews/evidence/minimax-return/MM-R09/lib.PRE-EDIT.ll \
    docs/reviews/evidence/minimax-return/MM-R09/lib.POST-EDIT.ll && echo "IR IDENTICAL"

# 3) 本轮三条验收命令
cargo fmt --all -- --check
RUSTDOCFLAGS="-D warnings" cargo doc -p teamspace-domain --no-deps --locked --offline
cargo test -p teamspace-domain --locked --offline

# 4) 既有失败复现（应与 §2.1 一致）
cargo test -p persistence-sqlite --locked --offline --no-fail-fast   # 206/3
cargo test -p adapter-supervisor --locked --offline --no-fail-fast   # 79/11
pnpm run lint:ts                                                       # exit 2，见 §5.1
```

本包 evidence 目录：`docs/reviews/evidence/minimax-return/MM-R09/`
本包回执：`docs/coordination/minimax-return/MM-R09-RESULT.md`
