# MM-COORD｜跨包复核与待裁决清单（2026-10-08，供 Codex 独立复核）

本端（Minimaxcode）已完成 MM-R01、R02、R04、R05、R08 的执行与独立复核。以下是**必须由 Codex（总控）或 Human 裁决**的事项。本端未擅自处置任何一项。

**本清单不解除任何节点 BLOCKED，不构成正式 DONE 判定。**

---

## A. 写集阻塞（阻塞既有测试转绿）

### A-1｜`MAX(version) == 24` 两处脆弱断言 —— 已由 Human 裁决
- 文件：`crates/persistence-sqlite/tests/notification_execution_integrity.rs:470`、`crates/persistence-sqlite/tests/outbox_default_forward_migration.rs:150`
- 事实：R01/R02/R03 各新增一个迁移（0025/0026/0027），`MAX(version)` 依次变为 25/26/27，这两处硬编码断言必然失败。实测失败体为纯计数 `left: 26, right: 24`，其前的全部行为断言均已通过。
- **Human 裁决（2026-10-08）：保持写集不扩张，不修改，留给 Codex 最终统一门禁处理。**
- 全仓扫描结论：仓内 `MAX(version)` 断言共 4 处，**仅此 2 处脆弱**；另 2 处（期望 `22` / `23`）是回滚断言，语义为「迁移失败后停在原地」，**不受影响，一直保持绿**。其余迁移断言一律用 `WHERE version = N AND success=TRUE`（version=10/11/12/13/14/15 共六处），说明稳健写法是仓内既有范式，这 2 处是局部偏离。
- 建议最小修法（**本端未执行**）：改为 `WHERE version = 24 AND success = TRUE` == 1，语义等价且不随新迁移漂移。
- 详见 `MM-COORD-migration-version-assertions.md`。

### A-2｜Adapter fixture 传输层 —— 阻塞 11 个既有测试
- 文件：`crates/adapter-supervisor`，MM-R05 判 `SELF_CHECK_BLOCKED`。
- 事实：MM-R05 严格修复了 DA-05（五个 stdout 读入口不再跳过 `running `/`test `/空白行）。但测试用 **libtest 子进程本身**充当 Adapter，而 libtest 无条件把 `running 1 test`、`test ... ok`、`test result:` 及空行写入真实 stdout。MM-R05 已实测穷举：去掉 `--nocapture` 后子进程 stdout **逐字节相同**；`--format=pretty`/`terse` 仍带框架行；`json`/`junit` 需 nightly。故在当前写集下，「审计要求空白行必须拒绝」与「用 libtest 当 fixture」无法同时满足。
- 现状：`cargo test -p adapter-supervisor` = **79 passed / 11 failed**。11 个失败全部是 `handshake_*` / `health_check_*` / `run_normal_execution_flow_via_fixture`，根因是 fixture 传输层，**不是产品解析逻辑**。
- MM-R05 的提案（**未自行实施**）：新增 `crates/adapter-supervisor/src/bin/fixture_adapter.rs`。Rust 自动发现 `src/bin/*.rs`，**无需改 Cargo.toml、无需碰 lockfile**，11 个测试可原样复用现有断言。备选方案（改用 `adapters/mock/index.mjs` 作 fixture）虽在写集内，但会给 Rust 单元测试引入 node 依赖，并重新引入静默跳过假通过风险。
- **请 Codex 签发或否决该写集扩展。**

---

### A-3｜`pnpm run lint:ts` 从 exit 1 恶化为 exit 2（崩溃）—— **本轮 R10 引入的回归，优先处理**
- **事实（本端已独立复现）**：`MM-R10/source-copy/apps/desktop/dist/` 存在（200 KB Vite bundle）。`eslint.config.mjs` 的 `ignores` 只覆盖规范路径 `apps/desktop/dist/**`，**未覆盖证据目录里的副本**，故 ESLint 走到 `index-Dp_6cwvq.js` 并在 `getParserServices` 抛错：
  ```
  Occurred while linting .../MM-R10/source-copy/apps/desktop/dist/assets/index-Dp_6cwvq.js
     ELIFECYCLE  Command failed with exit code 2.
  ```
- **后果**：canonical 21 的第 5 项从「报告 1 个已登记的 `replay_env_manifest.mjs` 错误」恶化为「整体崩溃」，**原先那条真实错误被掩盖**。任何统一门禁运行都会撞上。
- **性质**：这是本轮工作**自己引入**的回归，不是历史遗留。`dist/` 是构建产物，本不该进入源码副本——覆盖率测量只需要 domain/application src。
- **两个可选修法**（**本端均未执行**）：
  1. 删除 `MM-R10/source-copy/apps/`（在 **R10 自己的写集内** `docs/reviews/evidence/minimax-return/MM-R10/`）。最小、不动任何配置，`lint:ts` 恢复到 exit 1 的已知单错。代价：R10 证据目录内容变化，R09 已登记的部分 SHA 需重核。
  2. 在 `eslint.config.mjs` 增加 `docs/reviews/evidence/**/source-copy/**/dist/**` 忽略。**但 `eslint.config.mjs` 不属任何包写集**，需 Codex 签发写集扩展；且会永久放宽 lint 范围，掩盖未来同类问题。
- **本端倾向**：修法 1。它把构建产物移出源码副本本身，而非放宽全局规则；且不需要任何写集扩展。

#### 处置结果（Human 2026-10-08 裁决采纳修法 1，已执行）
- 已用 `mavis-trash`（可恢复删除）移出 `docs/reviews/evidence/minimax-return/MM-R10/source-copy/apps/`（932 KB，含整个 `node_modules` 拷贝，对 Rust 覆盖率无用）。
- **已验证 `pnpm run lint:ts` 恢复为 exit 1**，重新报告 `replay_env_manifest.mjs:24:22 no-unsafe-finally` 这一条真实错误，崩溃消失、信号恢复。
- **未触碰** `eslint.config.mjs`，未扩大任何 lint 范围。
- 交付端影响已核：runner 只跑 `-p teamspace-domain -p application`、只对这两个 crate 做逐字节断言，故 `2016/2197 = 91.76%` 与其证据不受影响。
- **须告知复核方**：被删的 `source-copy` 原有 workspace `members = ["apps/*/src-tauri", "crates/*"]`，`apps/` 移除后该 glob 空匹配（`crates/*` 仍有效）。若将来**重跑**覆盖率，须先按当前工作区重新刷新源码副本（任务包本就要求每次重测都重新复制），不能直接复用现有 `source-copy`。

### A-4｜已交付文档中的数字错误（R04 RESULT）—— **已订正**
- `MM-R04-RESULT.md:136` 原写「10 passed / 2 failed」，实测 `red-1-migrated-audit-tests.log` 为 **`8 passed; 2 failed`**（10 是总数，被误写为通过数）。
- **已改为 8 passed / 2 failed**，并在 §6.1 加注订正来源与「不改变任何判定」。
- **复核更正**：MM-R09 曾报告 `:144` 的「10 passed / 7 failed」同样有误；**经本端核对日志，该行实测正是 `10 passed; 7 failed`，原记录无误，未作改动**。两项建议中只有一项成立。

## B. 审计自身的缺陷（需 Codex 订正审计记录）

### B-1｜`references` 表在 0001～0025 无任何 `BEFORE DELETE` 守卫 —— **本轮新发现的审计盲区**
- 事实（本端已独立核实）：表名带引号为 `"references"`（SQL 保留字）。`0013_artifact_reference_consistency.sql` 只有 `references_consistency_insert`（:82）与 `references_consistency_update`（:98）两个触发器。**全范围 0001～0025 没有任何 `BEFORE DELETE ON "references"`。**
- 冻结依据：`DATA_CONVENTIONS §5` 明确「引用失效只更新 `validation_status`；**不删除历史 URI/path/hash**」。
- 审计 §3 DA-02 称「`0013` 的 Reference guard 已存在且会保护有关联的行」——就 INSERT/UPDATE 而言成立，但该 guard **不覆盖 DELETE**。这是与 DA-02 同类的缺口，只是不在 DA-02 声明的五表（runs/reviews/artifacts/approvals/messages）范围内，故 MM-R02 未越界处理、已如实登记。
- **请 Codex 判定是否追加一个前向迁移补 `references` 的 DELETE 守卫，或扩写某包写集。**

### B-2｜~~审计 RED 日志中 `artifacts` 的装置缺陷~~ —— **本端撤回：该指控不成立**
> **撤回声明（2026-10-08，本端自查后撤回；Codex 复审 §5 裁定一致）**
>
> 本条曾断言：审计 repro 对 `artifacts` 的 INSERT 只给了 11 个值而该表有 12 列，故该条 RED 不可复现。
>
> **该断言错误。** 本端已重新逐值解析：`artifacts` 确为 12 列，而审计的 INSERT **提供了 12 个值**，其中第 9 位是显式 `NULL`，正是 `git_commit_sha`：
> ```
> 1 id  2 team_id  3 task_id  4 run_id  5 creator_role_id
> 6 artifact_type  7 uri  8 checksum_sha256(?)  9 git_commit_sha(NULL)
> 10 visibility  11 version  12 created_at
> ```
> 本端早先读那行长 SQL 时漏看了这个 `NULL`，又未自行计数便采信了 MM-R02 的转述，随后把「已独立核实成立」写入本清单——**该「独立核实」是失实的，本端在此明确认领**。MM-R03 转述的同一错误说法一并撤回。
>
> Codex 已用原 main-repro 源码、独立新 target 重跑 SQLx：`1 passed / 3 failed`，五表仍全部可删（含 artifacts），**原审计 RED 可复现、结论有效**。
>
> 原文字保留于下方历史段落，**不予覆写**，以留痕。本撤回不改变 B-1（`references`）与 B-3（`delivery_attempts`）——那两项本端已独立核实成立，Codex 复审亦确认保留。

<details><summary>已撤回的原文（留痕，不作数）</summary>

- 事实：审计 repro `main-repro/crates/persistence-sqlite/tests/audit_storage_boundaries.rs` 对 `artifacts` 的 INSERT 只给了 **11 个值**，而该表有 **12 列**（`git_commit_sha` 位于 checksum 与 visibility 之间）。且 `main-repro/0005_artifact_reference.sql` 与仓库 `0005` 逐字节相同，故不是副本漂移。
- 影响：`storage-boundaries-red-v2.log` 中 `artifacts` 那一行按该装置实际会报列数不匹配，**该条 RED 的可复现性不成立**。
- DA-02 的结论本身不受影响（MM-R02 用正确列数独立测得五表均 1→0）。**建议 Codex 订正该日志的可复现性声明。**

</details>

---

### B-3｜`delivery_attempts` 表在 0001～0027 无任何 `BEFORE DELETE` 守卫 —— **第三个同类实例**
- 事实（本端已独立核实）：`delivery_attempts` 上共有 10 余个触发器，但**全部**为 `BEFORE INSERT` 或 `BEFORE UPDATE`（`0007` 的索引、`0017` 的六条、`0018` 的两条、`0025` 的三条）。**零个 `BEFORE DELETE`。** MM-R02 的 `0026` 覆盖五张表，**不含** `delivery_attempts`。
- 冻结依据（本端已核对原文）：
  - `DATA_CONVENTIONS.md:55`（§5）：「Event、Run、Binding、Review decision、**DeliveryAttempt**、Approval decision 不更新历史语义，只追加后续事实。」
  - `DATA_CONVENTIONS.md:65`（§6）：「**Message/Delivery**/Notification/Approval | 随 Team 保留 | 只读、可导出」
- 发现者：MM-R07（route override 方案包），在其做合法改派场景复现时实测 `DELETE FROM delivery_attempts` **ALLOW（1→0）**，同刻 `messages` DELETE **REFUSE**，证明守卫确实加载、不是装置问题。
- **对本方案承重**：MM-R07 的方案承诺「旧 attempts 保留」，若该表无留存防线，改派后的投递痕迹可被物理删除，方案承诺打折。MM-R07 已主动登记未自行修补。

### B-4｜同类别缺陷的三实例总结 —— 建议 Codex 按类而非按点处置

本轮在审计 DA-02 声明的五表之外，又发现两张同类表缺失 DELETE 守卫：

| 表 | 冻结依据 | 守卫现状 | 发现者 |
|---|---|---|---|
| `runs` / `reviews` / `artifacts` / `approvals` / `messages` | §5、§6 明列 | 审计 DA-02 已识别，本轮由 `0026` 补齐 | 审计 → MM-R02 |
| `"references"` | §5「引用失效只更新 `validation_status`；不删除历史 URI/path/hash」 | `0013` 只有 INSERT/UPDATE，**零 DELETE** | MM-R02 |
| `delivery_attempts` | §5、§6 **双处**明列 | **零 DELETE** | MM-R07 |

审计 §3 DA-02 明确列出这五表并声明「`0013` 的 Reference guard 已存在且会保护有关联的行」——就 INSERT/UPDATE 成立，但不覆盖 DELETE。**这三处指向同一个系统性成因：留存守卫是逐表零散补齐的，没有一条覆盖全部被 §5/§6 命名留存的数据。**

**建议 Codex 判定是否改为一次性普查**：对 §5 与 §6 命名的全部数据（Event、Run、Binding、Review decision、DeliveryAttempt、Approval decision、Message、Delivery、Notification、Approval、Artifact 索引、references、idempotency 记录…）逐一核实 DELETE 策略，而不是继续按审计发现逐点补。这样可避免下一轮再冒出第四个实例。

## C. 需要 Codex 判断的行为边界（非缺陷，但影响验收）

### C-1｜MM-R04 的一处行为放宽（derived reading）
- 修复后，非字母数字的 secret 字节允许**字面出现或 `%XX` 皆可**（例如 `Audit-Only%2FAaB` 会被脱敏，修复前不会）。
- 冻结规格未逐字规定该粒度。MM-R04 已登记；收紧只需删 `match_at` 中 3 行，钉子用例会同步转红。
- **请 Codex 判定该粒度是否符合 `TECH_STACK §7/§11` 意图。**

### C-2｜MM-R04 同族未修风险 R1
- 纯 hex 全量形式仍要求全小写或全大写，**大小写混合的整串 hex 仍会绕过**。
- DA-04 明确限定 percent 形式，MM-R04 未越界处理。**请 Codex 判断是否同批处理。**

### C-3｜MM-R04 一处行为收紧
- `audit%2donly%2faab`（全小写 percent 串）不再脱敏——它解码为 `audit-only/aab`，本就不是已登记 secret。MM-R04 判定为纠正旧实现的误伤。**请 Codex 确认。**

---

## D. 总控自身失误登记（如实上报，不掩饰）

- 总控在 MM-R05 执行中曾建议「去掉 4 处 `--nocapture` 以消除 stdout 噪声」。该建议**错误**：MM-R05 实测证明去掉后子进程 stdout 逐字节相同，libtest 的进度行无条件写真实 stdout，`--nocapture` 只影响测试体内 `print!` 的捕获。
- 总控已正式撤回该建议并写入 MM-R05 交接；MM-R05 未据此改动任何代码，并以 `09-nocapture-removal-disproof.log` 留下可重放证伪。
- 登记目的：让 Codex 知悉该建议的提出与证伪链路，避免复核时误将其当作有效结论。

---

## E. 已交付并经本端独立复核的包

| 包 | 判定 | 本端独立复核证据 |
|---|---|---|
| MM-R01 Team 一致性 | `SELF_CHECK_BLOCKED`（因 A-1） | 全量 `--no-fail-fast` 实跑：除 A-1 两处外全绿；0025 迁移逐条静态复核；所引用列名逐个核对存在 |
| MM-R02 核心历史留存 | `SELF_CHECK_PASS` | 全量实跑：除 A-1 两处（当时为 26 vs 24）外全绿；0026 的 11 个触发器与其自述一致；`references` 缺口（B-1）经本端独立核实 |
| MM-R04 脱敏 | `SELF_CHECK_PASS` | 本端重跑 `cargo test -p observability`：**18 passed / 0 failed，exit 0**；逐行确认旧的整体小写化已移除 |
| MM-R05 JSON-RPC stdout | `SELF_CHECK_BLOCKED`（因 A-2） | 本端重跑：**79 passed / 11 failed**，与报告一致；抽查新用例确认真实执行（无静默跳过）、四类噪声 × 五阶段全覆盖；`protocol.rs` 仅剩 1 处合法 `continue` |
| MM-R08 通知与 Attention 决策表 | 方案包（无实现） | 已落盘；A 类（可直接实施）与 B 类（须 Human 逐条裁决）分离 |

## F. 本端自评发现的一处隐患（非缺陷登记）

MM-R05 交付的新用例含静默跳过模式：node 不可用时 `noisy_mock_command` 返回 `None`，测试打印 `skipping` 后直接 `return`，等价于假通过。本端已实测确认**当前环境未触发**（20 次拒绝全部真实执行），不影响本次结论；但该模式对交付测试构成隐患，而 MM-R05 仅在 RESULT 中把该风险写给了备选方案，未自报其自身用例。建议 Codex 复核时要求补一条「node 不可用时必须失败而非跳过」的断言。