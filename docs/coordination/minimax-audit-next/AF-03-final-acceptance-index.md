# AF-03 最终证据口径与当前开放项（2026-10-09）

**性质：纯文档 + 独立重放脚本。** 本包未改动任何工程文件（`crates/**`、`scripts/**`、根 manifest/lockfile、`eslint.config.mjs`、`sources/**`、迁移 `0001`–`0029`、历史报告与 evidence 全部只读）。

**权限：本包不自判任何 DONE，不解除任何 BLOCKED，不启动依赖节点。** 正式节点的 DONE 权限属 Codex。节点状态仍为 **20 DONE / 21 BLOCKED / 60 PLANNED**，与本轮开始时一致（`node-status-proof.json`：`node_source_sha256 dd16ad19…`）。

**独立重放：** `docs/reviews/evidence/minimax-audit-next/AF-03/replay_af03_final_acceptance.py` → **`exit 0`，`verdict REPLAY_MATCHES_RECORDED`，38/38 项派生物与登记值一致**（输出同目录 `replay-output.json` / `replay-output.log`）。重放只读既有日志并重新推导，不重跑测试套件、不重新生成任何当时不存在的日志、不联网、费用 $0。

主控来源：`docs/reviews/FC-RETURN-AUDIT-2026-10-09.md`；本轮新证据目录 `docs/reviews/evidence/fc-return-audit-2026-10-09/`。

---

## 1. 裁决 1：R10 / M14 两列口径

原 FC-08 索引 §1「不撤回、继续保留」与 §3.1 保留原字，不覆盖。以下为最终口径，**两列分别说明来源**。

| 口径列 | 声明内容 | 来源 | 能证明什么 / 不能证明什么 |
|---|---|---|---|
| **A. 原 R10 声明口径** | **15 项按声明意图重建 ＋ 1 项（原写法 M14）不可编译、历史「2 tests」未证** | 原 R10 自述；原始 M1–M16 注入现场**从未归档**（FC-08 §3.1 限定 1） | 能证明「按 R10 文字描述重建的变异在**当前**代码上会被断言检出」；**不能**写「原声明 16/16」，也**不能**证明当时跑的是同一批变异 |
| **B. 新实验集口径** | **15 项重建 ＋ M14-R（明确配套 `ALL` / `as_str` 改动）＝ 16 项，16/16 断言检出** | 本轮在独立当前源副本上的重跑台账（`mutation/ledger.json`，新 evidence） | 能证明**本轮新实验** 16 项全部被断言杀死；这是**单列的新统计**，**不以「重新定义声明数」替代 A 列** |

A 列的 M14 依据（原写法只有新增 enum 变体、无配套 `ALL`/`as_str`）：`M14-original-result.json` = **`exit 101`、`compile_error`、失败测试 0 条（未运行）**，撤销后 `restored_test_exit 0`。原「被 2 个测试检出」**未复现**；修订后的 M14 被 **1 个**测试检出。

台架可信度：变异前绿基线 40/0，16 个变异全部 `killed_by_assertion`（**编译失败从未计为 kill**），撤销后 40/0；还原验证 137 文件 **0 差异**、80 份 `.rs` 与生产 **0 差异**。

**新旧编号映射（新增，不改旧补丁/日志）：**

| 旧编号（R10 / 现台账） | 新口径列 | 判定 | 检出测试数 |
|---|---|---|---|
| M1、M2、M3、M4、M5、M6、M7、M8、M9、M10、M11、M12、M13、M15、M16（共 **15**） | A 列「按声明意图重建」 ＋ B 列「新实验」**同一次运行** | `killed_by_assertion` | 多数 1，M8/M9/M15 为 2 |
| **M14（原写法）** | **仅 A 列** | `compile_error` / exit 101 / 0 失败测试 | **0（未运行）** |
| **M14-R（原 M14 ＋ 配套 `ALL`/`as_str`）** | **仅 B 列** | `killed_by_assertion` | **1** |

旧 `mutation/ledger.json` 的 `results[].id = "M14"` 那一行即 **M14-R**（该台账跑的是配好配套改动的版本）；`M14-original-result.json` 是**原写法**的独立结果。两者编号相同、内容不同，此映射即为此而设。

> 术语说明：本包不再把 FC-08 称为任务表「第 10 包」。**本轮实际交付为九包（FC-01…FC-09）。**

---

## 2. 裁决 2：旧开放清单的当前分类

交接中的旧开放清单**按四类重写**。不覆盖历史文档，只在此给出当前状态。

### 2.1 已由 0028 关闭（曾有缺口，不再列为当前故障）

| 旧清单条目 | 当前状态 | 证据 |
|---|---|---|
| `references` 表 DELETE 守卫缺口 | **已关闭** | `0028_retained_entity_integrity.sql:402`（`BEFORE DELETE ON "references"` → `RAISE(ABORT)`）；重放脚本按当前磁盘字节重新定位，确认仍在 `:402` |
| `delivery_attempts` 表 DELETE 守卫缺口 | **已关闭** | `0028_retained_entity_integrity.sql:435`（`BEFORE DELETE ON delivery_attempts` → `RAISE(ABORT)`） |
| FC-02 交付的 `delivery_attempts` 同 id 替换防线 | 仍来自 **0017**，0028 未靠同名空守卫作假证明 | 主控审计 §3 |

> 明确弃用的旧措辞：「references、delivery_attempts 仍无 DELETE 守卫」是**旧状态**。它不得再被列为当前未修缺陷；当前口径是「**曾有缺口，已由 FC-02 的 0028 关闭**」。

### 2.2 已关闭（其他）

| 条目 | 当前状态 | 证据 |
|---|---|---|
| FC-03 终态矩阵缺口（Run 终态互转） | **已关闭（0029）** | Run 完整 64 格：25 接受 = 8 self + 17 Frozen 边 / 39 拒绝；终态互转缺口关闭 |

### 2.3 本轮已签收 / 已验证

| 条目 | 裁定 | 限定（不得删除的边界） |
|---|---|---|
| **FC-05** 四条 crate-private unit 测试（`send_protocol_line` 不可见，故留 unit） | **本轮签收** | 不扩大生产可见性 |
| **FC-05** POSIX 子进程（`/bin/cat`、`/bin/sleep` 字面参数启动） | **本轮签收** | 非 libtest 充当 Adapter |
| **FC-05** 时间裕度（成功路径 1s→5s；后代 grace 100ms→1000ms、上界 800ms→3000ms、watchdog 1200ms→6000ms、后代管道持有 3s→10s） | **本轮签收** | **它不再证明原 1 秒性能目标。** 期望超时的 30ms/50ms 用例保留原值 |
| **FC-06** 机制（shallow 拒绝、companion ID 唯一性、kind/command/args 绑定、不读外部伪造证据 JSON） | **已验证** | 信任前提见 §3 |
| **FC-09** 精确 eslint 配置 | **已由主控整合**，仅归档一个不可变历史件 `deep-audit-2026-10-08/replay_env_manifest.mjs`，规则未关闭、字节未变；**全仓 lint 0**（baseline 1 error → final 0 error / 0 warning） | 工具范围收口 ≠ 完整节点或 Mac Gate 通过 |

### 2.4 仍然开放

| 开放项 | 性质 | 归属 |
|---|---|---|
| **AF-01** `%` 字节单层编码漏检（P1） | 唯一新行为阻断：`%ABC→%25ABC`、`A%B→A%25B`、`%%→%25%25` 实测 0/3 失败（exit 101） | Minimax 主责，Codex 安全复核 |
| **AF-02 联合验收** | 概念库已证（42 observations / 0 failed）与 SQLx `no_tx=false` 空库 DDL 原子性已证；**存量业务行升级、完整三轴语义仍待实现包验收** | Minimax 主责 |
| **通知 B-01～B-08 Human 决策** | 仅为 Human 决策输入；**不得自改 ADR-0007（仍 Proposed）或实现待决字典** | Human |
| **可信应用链 / 完整 Mac 门禁** | 联网 Mac 21 / advisory / 真实 Harness / Keychain / 最低 macOS 14 实机 / 备份恢复 / 性能 / A11y / 全产品覆盖：**未运行** | 需 Human 明确授权后另签 |
| **branch 覆盖率门槛（Gate C 85%）** | **未验证**，见 §5 | 待有效 branch instrumentation |
| **统一节点门禁 / G0～G9** | 未完成；现有离线套件全绿**不能豁免** | Codex |
| **后续节点**（K1 可信身份、K2 已冻结通知身份、K3 应用事务/幂等/Event/Outbox、K4 dispatcher/Run/fencing、Workspace/IPC/UI） | 仍按前置 DONE 与具体写集**另签**；本表不自动开工 | Codex |

---

## 3. 裁决 3：FC-06 信任前提更正

FC-06 **机制成立**，但其原始结论表述的前提需更正（原记录保留，此处为更正说明）：

| 项 | 更正后口径 |
|---|---|
| PR 场景下的 `ci.yml` | **PR 自带内容**（`pull_request` 事件使用合并后的 workflow 定义，即 PR 自身修改后的版本），**不是「已受评审」** |
| 临时目录 + stub `PATH` | **测试隔离手段，不是 shell 安全沙箱**；不能假设被执行的片段已评审 |
| 当前 CI 信任边界 | workflow 顶层 `permissions: contents: read`、**不注入 secret**，且**原本就执行 PR 源码**（`pnpm run test`、`cargo test` 等）；`verify-workflows.mjs` 的边际增量有限 |
| **不外推** | **不外推到带秘密 / 带凭据的任意不可信本地工作区** |

重放确认：`workflow` exit 0、`gate-plan`（macos `--dry-run`）exit 0。这两条**不冒充**完整 Mac 21，也不冒充 hosted CI 实跑。

---

## 4. 裁决 4：`RAISE(IGNORE)` 的 fail-silent 更正

原 FC-02 / FC-03 记录中的「守卫改为 `RAISE(IGNORE)`」表述需按以下口径理解（旧记录一字未删，此为新勘误）：

| 项 | 更正后口径 |
|---|---|
| 机制性质 | **fail-silent：语句成功返回，但行未写入** |
| 被什么发现 | 测试的**拒绝码断言**与**实际行断言**发现了这种「假成功」——例如 FC-03 M1 的 `the statement was accepted` |
| **不**声称 | **不等于数据库已有非法脏写。** 不得把「语句被接受」直接写成「持久化状态被非法改写」 |
| 主控侧证据 | 当前迁移目录中 `RAISE(IGNORE)` 检索 **无命中**：没有引入生产 fail-silent `RAISE(IGNORE)`（`storage/REPORT.md`） |

---

## 5. 裁决 5：覆盖率口径

固定口径（`coverage/coverage-summary.json`，`measured_at_utc 2026-10-09T02:04:59Z`）：

| 项 | 值 | 限定 |
|---|---|---|
| 分母规则 | `all mapped lines of crates/{domain,application}/src/**/*.rs` | **全 16 个 domain src**；不含 tests/examples/dependencies；不排除任何生产 `.rs` |
| 测试集 | teamspace-domain + application **完整当前测试集**（全 target、无 filter、不以 `--ignored` 替代） | — |
| domain 行覆盖 | **2016 / 2197 = 91.76%** | region 89.87%（**region 不代替 branch**） |
| application | **占位（`is_placeholder_not_product_coverage: true`），无可执行行** | **不记 100%** |
| branch | **未验证（`branch_coverage_verified: false`）** | 本次构建 llvm-cov 导出的 `branches` 数组**全为 0**（映射总数 0）。**读取工具不能凭空产生 branch 映射**；**不写「0 branch = 100%」**。Gate C 85% branch 门槛仍 **UNVERIFIED** |
| 工具 / manifest / source SHA | 已固定并逐条登记 | root manifest `e2540025…`、lockfile `5822d1d1…`、toolchain `495c69d1…`、domain src 树 `5626cb9d…`、domain tests 树 `4cfc1fac…`（完整值见 `replay-output.json`） |

**AF-01 为何不重测 domain 覆盖：** AF-01 只改 `crates/observability/**`，**不改 domain**。分母（16 个 domain src 及其映射行）与源码字节均不变，因此 91.76% 不会因 AF-01 移动，**无需重复凑覆盖数字**。**若分母或源码改变（新增/删除 `.rs`、测试集变化、toolchain 变化），必须重新测量并更新本节。**

---

## 6. 裁决 6：最终离线证据数字（两栏分列）

### 6.1 当时存量数字（主控本轮实测，作为存量记录）

| 项 | 值 | 来源 |
|---|---|---|
| Rust `cargo test --workspace --locked --offline --no-fail-fast` | **643 passed / 0 failed / 1 ignored**，exit 0 | `main/rust-tests.log`（68 条 `test result` 逐 target 求和；1 ignored 为既有文档示例） |
| 其中 | domain **287/0**、observability **29/0**、persistence-sqlite **233/0**、adapter-supervisor lib **64/0** ＋ integration `fixture_adapter` **29/0** | 同上，按 target 逐条求和 |
| TS 各层 | **36 / 0** | `main/ts-tests.log` |
| Gate 单测 | **25 / 0** | `main/gate-tests.log` |
| replay 单测 | **8 / 0** | `main/replay-tests.log` |
| lint | baseline **1 error** → final **0 error / 0 warning** | `main/lint-baseline.log`、`main/lint-final.log` |
| **不计入 643** 的新反例 | `%` 回归 **0 passed / 3 failed**，exit 101 | `main/percent-regression-red.log` |

**这 3 条是新反例，不混进 643 条已交付套件的统计。**

### 6.2 未来终值（AF-01 之后）

| 项 | 值 |
|---|---|
| Rust 全量 | **待定 —— 必须从 AF-01 修复后的实际日志更新** |
| 规则 | **不得硬编码保留 643。** AF-01 修复会增加测试，Rust 总数**必然变化**；终值只能来自真实 `cargo test` 输出，由收口轮重新登记 |
| 记录纪律 | 「**当时存量数字**」（§6.1）与「**未来终值**」（本节）**分开记录**，不得把前者当成后者沿用 |

---

## 7. 本包证据目录

`docs/reviews/evidence/minimax-audit-next/AF-03/`

| 文件 | 内容 |
|---|---|
| `replay_af03_final_acceptance.py` | 独立重放脚本（纯读，offline，$0）。按六条裁决逐项重新推导登记值 |
| `replay-output.json` | 机器可读重放结果：六裁决结构化输出、38 项逐项校验、输入文件 SHA-256 清单 |
| `replay-output.log` | 人读重放结果（**exit 0**，`REPLAY_MATCHES_RECORDED`，38/38） |

重放脚本的判据纪律：**不能从既有产物派生的项报为 `not_derivable` / 抛错，绝不记为通过**；编译失败不计为 kill；lint 计数取 eslint 机器可读汇总行而非逐条文本。

本包结果与权限登记见 `AF-03-RESULT.md`。