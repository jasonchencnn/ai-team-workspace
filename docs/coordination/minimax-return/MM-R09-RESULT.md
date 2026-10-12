# MM-R09-RESULT｜归档与文档订正包执行回传

- 关联节点：DEV-01-05（关联 01-08、02-01/11）
- 审计依据：`docs/reviews/DEEP-AUDIT-2026-10-08.md` §5「文档/证据纠正与节点准入」
- 结论：**`SELF_CHECK_PASS`**（本包自身交付全绿；**未修改任何节点正式状态**）
- **自验收不等于节点 DONE。正式准入由 Codex 裁定，本包不解除任何 BLOCKED、不启动依赖节点。**

---

## 1. 实际模型、环境与固定输入

| 项 | 实际值 |
|---|---|
| 执行端 | Minimaxcode（Worker 子会话 `mvs_474961e970b84da8ad3df48ecc53f4d2`） |
| 实际模型/版本 | `MiniMax-M3.1-Flash-Preview`（执行端回填；**不猜测 M3**，权重版本号执行端不可见） |
| Harness | MiniMax Code Desktop，本地分支会话 |
| 仓库 | `/Users/chenzhiqing/Documents/MyVault/协作工作` |
| HEAD | `1a14b8795e76fc373bef326eb5df96f9ac173f71` |
| 继承工作树 | **121 项未提交成果全程保留**。未执行 `reset`/`checkout`/`clean`/`stash`（会话起止 `git status --porcelain \| wc -l` 均为 121） |
| 平台 | darwin 27.0.0 arm64（macOS） |
| 工具链 | `cargo 1.98.0 (797e8a9bc 2026-08-05)` / `rustc 1.98.0 (88d9e12ae 2026-08-18)`，PATH 前置 `/Users/chenzhiqing/.minimax/toolchain/bin` |
| 网络 | `CARGO_NET_OFFLINE=true`，全部 cargo 命令带 `--locked --offline`，**零下载、零新增依赖** |
| 新增费用 | **$0**。未联网、未调用外部服务、未访问生产凭据/数据、未提交/推送/部署 |
| 源码 | `docs/reviews/evidence/minimax-return/MM-R09/00-pre-edit-state.txt` |

---

## 2. 输入与输出 SHA

### 输入（本包改动前）

| 文件 | SHA-256 |
|---|---|
| `crates/domain/src/lib.rs` | `19df10f71739789210c4dffe68fe96bea29e134d054af2374602bab03f832dc5` |
| `crates/domain/src/task_state_machine.rs` | `09a3bbf934050388dff45e4f5adf36aff43163ed987be6e02de41975a42e285a` |

### 输出（本包最终）

| 文件 | SHA-256 |
|---|---|
| `crates/domain/src/lib.rs` | `5072ba4b1e7e37cf479fcc87581a8e73bd6c028c83e75360a460680f48618d42` |
| `crates/domain/src/task_state_machine.rs` | `f7f2ea56e315a613112132a7314f27a20a59b7ac9f4f435848d373261cbdc3d9` |
| domain `src/` 树（16 文件） | `5626cb9df788838feeebd055407fcf56e5da732c63becfb6c4d7d85e1cc07792`（MM-R10 实测时为 `db929d01…`，**仅因本包 2 份文件的注释字节而变**） |
| domain `tests/` 树（16 文件） | `4cfc1fac9765ab3c7a6f4033617bc887facb32c62e3b49b607b0fccf4a584cfe`（**与 MM-R10 实测时相同**） |
| application `src/` 树 | `ca2208b863927f7879b925f44953f546181ff0ad858884ddc836a9504e847f19`（**未变**） |
| 根 `Cargo.toml` / `Cargo.lock` / `rust-toolchain.toml` | `e2540025…` / `5822d1d1…` / `495c69d1…` —— **均未变**（沿用 MM-R10 实测值，本包逐一复核一致） |

树哈希用 MM-R10 runner 的**同一** `tree_digest` 算法重算，来源：`MM-R09/10-post-edit-input-sha.txt`。

各包（R01/R02/R03/R04/R05/R06/R10）登记的 **20 个输出文件 SHA-256 与当前工作树逐一一致，零漂移**：`MM-R09/14-recorded-output-sha-drift-check.txt`。

---

## 3. 逐文件修改说明（**只改注释，一个字节的可执行代码都没动**）

### 3.1 `crates/domain/src/lib.rs`（1 行，仅 `///` 文档）

```diff
 @@ impl UuidV7 {
     /// Creates a UUIDv7 from a UTC Unix timestamp in milliseconds and 74 bits
-    /// of caller-provided randomness. The leading two bits of `random[1]` are
+    /// of caller-provided randomness. The leading two bits of `random[2]` are
     /// ignored because UUIDv7 reserves them for the RFC 4122 variant.
```

**订正依据（可独立复核）**：实际实现为

```rust
bytes[6] = 0x70 | (random[0] & 0x0F);   // 版本 nibble
bytes[7] = random[1];                    // 原样
bytes[8] = 0x80 | (random[2] & 0x3F);   // ← 变体位屏蔽的是 random[2] 的高两位
bytes[9..].copy_from_slice(&random[3..]);
```

被 variant 屏蔽的两个最高位来自 `random[2]`，**不是 `random[1]`**。MM-R10 已用边界测试
`uuid_construction_sets_the_version_variant_and_timestamp_prefix` 确认**实现正确**，
因此这是**文档订正，不是代码缺陷**。「74 bits」一节经核算无误（4+8+6+56=74），未改动。

### 3.2 `crates/domain/src/task_state_machine.rs`（3 行，仅 `///` 文档）

```diff
 /// A legal edge, fully described but not applied.
 ///
-/// Returning this is a *decision*, not a mutation. The caller persists the new
-/// status, re-evaluates [`TaskTransition::preconditions`], and only then writes
-/// [`TaskTransition::events`].
+/// Returning this is a *decision*, not a mutation. The caller validates
+/// [`TaskTransition::preconditions`] first and only then conditionally writes the
+/// new status together with [`TaskTransition::events`], never the other way round.
```

**订正依据**：原表述「先 persist 再验证前置」把**领域决策顺序**写成了**存储事务顺序**，
不能作为任何事务实现的依据。审计 §5 要求正确表述为**先校验、再条件写入**，已照此改写。
刻意保持**行数不变（3→3）**，使下游行的行号与调试信息完全不动（见 §4）。
同文件 `TaskCreation::preconditions` 的既有注释「Preconditions the application layer must
verify before persisting」本就是正确顺序，**无需改动**。§329–340 的私有字段说明与
` ```ignore ` 反例块**未动**（该块讲的是字段可写会绕过 §2，与写入顺序无关）。

### 3.3 新增文档

| 文件 | 说明 |
|---|---|
| `docs/coordination/minimax-return/MM-R09-acceptance-index.md` | 归档索引主体：R01/R02/R03/R04/R05/R06/R10 逐包归档（输入/输出 SHA、修改文件、命令与 exit code、RED/GREEN、覆盖范围、判定、未运行项，每格标注来源文件路径）+ 跨包汇总 + 覆盖率处理 + 缺失历史证据新映射 + 节点状态 + 本轮新发现 |
| `docs/coordination/minimax-return/MM-R09-RESULT.md` | 本文件 |
| `docs/reviews/evidence/minimax-return/MM-R09/` | 21 份 evidence（见 §7 清单与 `MANIFEST.txt`） |

### 3.4 **未触碰**（写集纪律）

`sources/`、24 份 migration、任何行为逻辑、任何测试、`crates/persistence-sqlite/**`、
`crates/observability/**`、`crates/adapter-supervisor/**`、`crates/domain/tests/**`、
其他 domain src 文件、生成 TS、根 `Cargo.toml`/`Cargo.lock`/workspace manifest、
历史报告与历史 evidence（含 `docs/reviews/evidence/deep-audit-2026-10-08/**` **只读**）、
`docs/development/DEVELOPMENT_NODES.md`（正式节点状态）、任何 ADR 的接受状态。均**未修改**。

---

## 4. 可执行行未变的证明（两处订正的验收核心）

### 4.1 结论

**订正前后，rustc 为 `teamspace-domain` 生成的 LLVM IR 模块逐字节相同。**

| 项 | 值 |
|---|---|
| 订正前 IR SHA-256 | `c70b72f673ccb70744560a8bd959c5b417fdb77176a5c93d2c66ab1e0c5642b3` |
| 订正后 IR SHA-256 | `c70b72f673ccb70744560a8bd959c5b417fdb77176a5c93d2c66ab1e0c5642b3` |
| `cmp` | **IDENTICAL** |
| IR 指令行数 | 21699 → 21699（无变化） |
| 证据 | `MM-R09/lib.PRE-EDIT.ll`、`lib.POST-EDIT.ll`、`05-llvm-ir-invariance.txt` |

复现命令：`cargo rustc -p teamspace-domain --locked --offline -- --emit=llvm-ir`
（两侧 IR 均由**本包会话内实际编译**产生，两次都出现 `Compiling teamspace-domain`，非缓存命中。）

### 4.2 为什么用 IR 而不是 rlib（如实登记一次失败的证明路径）

首次尝试比较 **rlib**，结果**不同**：

| 项 | 值 |
|---|---|
| 订正前 rlib | `3f14be0fc78ebbfb13563e58673248e1ddf13891d255c4209c2b0b5e59d0fd2a` |
| 订正后 rlib | `a22872d81610f0bee812fe90a5a96ddd75da12082a18b76c2619c1695d49c012` |
| 原因 | **rustc 把 `#[doc]` 字符串编入 crate metadata**。`strings` 验证：订正前 rlib 含 `random[1]`，订正后含 `random[2]` |
| 判定 | rlib 字节差异**由文档字符串完整解释**，因此 **rlib 不是可执行行变化的判据**，该路径**不成立** |
| 改用 | LLVM IR：rustc 可执行表示的直接产物，**不编码文档注释**，是正确判据 |
| 证据 | `01-rlib-sha-before.txt`、`03-rlib-sha-after.txt`、`04-executable-line-invariance.txt` |

> **仅缓存命中登记**：订正前那份 rlib 来自 cargo 的既有构建（`Finished in 0.32s`，无 `Compiling`），
> 属**仅缓存命中**。cargo 判定其对订正前源码新鲜，故与订正前字节对应，但**不是本包会话内重新生成的**。
> 这正是不采用 rlib 作为主判据的另一个理由。IR 证明的两次编译**均为本包实际执行**。

### 4.3 源级复核

`diff` 显示 `lib.rs` 改动 **2 行**（1 删 1 增）、`task_state_machine.rs` 改动 **6 行**（3 删 3 增），
**全部是 `///` 行**，行数分别保持 643 / 668 不变。完整 diff：`MM-R09/02-comment-only-diff.txt`。

---

## 5. 命令、退出码与日志

| # | 命令 | exit | 日志 | 结论 |
|---|---|---:|---|---|
| 1 | `cargo fmt --all -- --check` | **0** | `MM-R09/06-cargo-fmt-check.log` | **无输出、无漂移**（含未改动文件）。**未执行 `cargo fmt --all` 写入** |
| 2 | `RUSTDOCFLAGS="-D warnings" cargo doc -p teamspace-domain --no-deps --locked --offline` | **0** | `MM-R09/07-cargo-doc-strict.log` | 严格 rustdoc 通过，文档订正在 `-D warnings` 下干净 |
| 3 | `cargo test -p teamspace-domain --locked --offline` | **0** | `MM-R09/08-cargo-test-domain.log` | **287 passed / 0 failed / 1 ignored**，18 个 test target 全 ok（与 MM-R10 §5 记录一致） |
| 4 | `cargo build -p teamspace-domain --locked --offline` | 0 | — | 订正后正常构建 |
| 5 | `cargo rustc … -- --emit=llvm-ir`（订正前 / 后各一次） | 0 / 0 | `MM-R09/lib.PRE-EDIT.ll` / `lib.POST-EDIT.ll` | **两次均实际 `Compiling`**，IR 恒等 |
| 6 | `cargo test -p persistence-sqlite --locked --offline --no-fail-fast` | **101** | `MM-R09/09-preexisting-persistence-sqlite.log` | **206 passed / 3 failed** —— 3 处既有版本计数断言，见 §6.1 |
| 7 | `cargo test -p adapter-supervisor --locked --offline --no-fail-fast` | **101** | `MM-R09/11-preexisting-adapter-supervisor.log` | **79 passed / 11 failed** —— 既有 fixture 传输层问题，见 §6.2 |
| 8 | `pnpm run lint:ts` | **2** | `MM-R09/12-preexisting-lint-ts.log` | **崩溃**，非既有的 exit 1。见 §6.3（本轮新发现） |
| 9 | `pnpm exec eslint <单个文件>` ×2（因果隔离探针） | 1 / 2 | `MM-R09/13-lint-crash-root-cause-probe.log` | 证明 §6.3 的根因 |

**未运行的命令及理由**：

| 未运行 | 理由 |
|---|---|
| `cargo fmt --all`（写入式） | 任务包与 `AGENTS.override.md` 明令禁止；`--check` 变体已跑且 exit 0，无漂移需要修 |
| 任何写入式格式化/修复命令 | 本包只做注释订正 |
| 重跑 MM-R10 覆盖率测量 | 见 §8，**明确不重复** |
| canonical Mac 21 项在线门禁 | 属 Codex 最终统一门禁；`--offline` 约束。需要联网/审计服务的条目本包无权运行 |
| `pnpm install` / `pnpm audit` / `cargo audit`（在线） | 禁止联网与下载 |
| 任何提交/推送/部署 | 明令禁止，且未获 Human 授权 |

---

## 6. 失败项登记（**先复现确认，再如实登记；一律不修**）

### 6.1 3 处版本计数断言（既有，Human 已裁决留给 Codex）

`MM-R09/09-preexisting-persistence-sqlite.log` 实测 **206 passed / 3 failed，exit 101**：

| # | panic 位置（本轮实测） | 断言 | 实测 |
|---|---|---|---|
| 1 | `crates/persistence-sqlite/tests/core_history_retention.rs:1274` | `COUNT(*) … WHERE success=TRUE == 26` | `left: 27, right: 26` |
| 2 | `crates/persistence-sqlite/tests/notification_execution_integrity.rs:470` | `MAX(version) == 24` | `left: 27, right: 24` |
| 3 | `crates/persistence-sqlite/tests/outbox_default_forward_migration.rs:146` | `MAX(version) == 24` | `left: 27, right: 24` |

三处**全部是纯版本计数**，与行为无关。Human 2026-10-08 裁决：**保持写集不扩张，留给 Codex**。
本包遵办：**未改期望值、未加 `#[ignore]`、未碰这三个文件**（均不在写集内）。
登记见 `MM-COORD-migration-version-assertions.md`。

> **行号差异登记**：任务书给的是 `1275` / `150`；实测 panic 行是 `1274` / `146`，
> 与 `MM-COORD-migration-version-assertions.md` 记载的 1275/150 也不完全一致。**以日志为准。**

### 6.2 11 个 adapter-supervisor 失败（既有，根因非产品代码）

`MM-R09/11-preexisting-adapter-supervisor.log` 实测 **79 passed / 11 failed，exit 101**，
与 MM-R05 记录**完全一致**。根因是 libtest fixture 传输层（`MM-R05-RESULT.md` §5），
**不在产品解析逻辑**。MM-R05 的写集扩展提案（新增 `src/bin/fixture_adapter.rs`）**尚未签发**。
本包**未删除、未 ignore、未改预期值、未放宽解析**。

### 6.3 `pnpm run lint:ts` —— **本轮新发现的回退（非既有登记）**

| 项 | 实测 |
|---|---|
| 当前 | **exit 2，ESLint 崩溃**（`Oops! Something went wrong!`） |
| 崩溃文件 | `docs/reviews/evidence/minimax-return/MM-R10/source-copy/apps/desktop/dist/assets/index-Dp_6cwvq.js` |
| 根因 | `eslint.config.mjs:12` 的全局 ignore 只匹配 `apps/desktop/dist/**`（仓库内规范路径）。MM-R10 的覆盖率 runner 复制整棵源码树进 evidence 目录时**把构建产物也复制了**，副本路径不匹配该 ignore，`eslint .` 于是爬进 193 KB 打包 JS，因缺 typed-lint 配置而抛错 |
| 被遮蔽的既有错误 | 单独 lint `docs/reviews/evidence/deep-audit-2026-10-08/replay_env_manifest.mjs` → **exit 1，1 error**：`24:22 no-unsafe-finally`（**该错误仍存在，未被修**） |
| 因果隔离 | 单独 lint 崩溃文件 → **exit 2**（PROBE A/B 双向验证） |
| 影响 | ① R04/R06 记录的「lint:ts exit 1，1 个错误」**已不是当前可复现状态**；② 任何依赖 `pnpm run lint:ts` 的门禁现在会因**配置错误**而非 lint 违规失败 |
| 处置 | **本包不修**：`eslint.config.mjs`、MM-R10 evidence 目录、`apps/desktop/dist/**` **均不在写集内**。候选修法（需 Codex 签发写集）：① `eslint.config.mjs` 增加 `docs/reviews/evidence/**` ignore；② 或 evidence 复制时排除构建产物。**两者都改变仓库级 lint 作用域** |

---

## 7. evidence 清单

目录：`docs/reviews/evidence/minimax-return/MM-R09/`（21 份，逐份 SHA-256 见 `MANIFEST.txt`）

| 文件 | 内容 |
|---|---|
| `00-pre-edit-state.txt` | 工具链、HEAD、未提交计数、订正前两文件 SHA |
| `01-rlib-sha-before.txt` / `03-rlib-sha-after.txt` | rlib 哈希前后对照（**结果：不同**，原因见 §4.2） |
| `02-comment-only-diff.txt` | 两处订正的完整 `diff -u` + 前后行数 |
| `04-executable-line-invariance.txt` | rlib 字节比较结果 |
| `05-llvm-ir-invariance.txt` | **IR 恒等证明**（`cmp` + SHA + 指令行数） |
| `lib.PRE-EDIT.ll` / `lib.POST-EDIT.ll` | 两侧 LLVM IR（**同一 SHA `c70b72f6…`**），供独立 `cmp` 复核 |
| `libteamspace_domain.PRE-EDIT.rlib` | 订正前 rlib，供 `strings` 复核文档字符串内嵌 |
| `lib.rs.PRE-EDIT` / `lib.rs.POST-EDIT` / `task_state_machine.rs.PRE-EDIT` / `task_state_machine.rs.POST-EDIT` | 两侧源码快照，供独立复算 diff 与 SHA |
| `06-cargo-fmt-check.log` | 验收命令 1，exit 0 |
| `07-cargo-doc-strict.log` | 验收命令 2，exit 0 |
| `08-cargo-test-domain.log` | 验收命令 3，exit 0，287 passed |
| `09-preexisting-persistence-sqlite.log` | 既有失败复现，206/3 |
| `10-post-edit-input-sha.txt` | 订正后树/文件 SHA（与 MM-R10 实测输入逐项对照） |
| `11-preexisting-adapter-supervisor.log` | 既有失败复现，79/11 |
| `12-preexisting-lint-ts.log` / `13-lint-crash-root-cause-probe.log` | lint 崩溃复现与根因隔离 |
| `14-recorded-output-sha-drift-check.txt` | 各包 20 个输出 SHA 与当前工作树漂移复核（零漂移） |
| `MANIFEST.txt` | 本目录 21 份文件的 SHA-256 清单 |

---

## 8. 覆盖率处理：**引用 MM-R10，不重复测量**

**本包未重跑覆盖率。** 理由：MM-R10 已在新证据目录完成完整离线重测（Apple LLVM
`llvm-profdata`/`llvm-cov`，新 runner 副本、新 source-copy、新 profile、新 target，
旧 profile/log/summary 未被覆盖）。重跑需重新 instrument 编译 18 个 coverage executable
并消费 4.9 MB export，**属高成本重复且不产生新信息**。

**但本包改了 domain 注释，故已重新固定 SHA 并给出更强证明**：

- domain `src/` 树：`db929d01…` → `5626cb9d…`（**变**，2 份文件注释字节）；
  domain `tests/` 树、application `src/` 树、根 manifest、lockfile、toolchain **全部未变**。
- 注释**不产生任何可执行行映射**，因此不进入 llvm-cov 的行覆盖率分母。
  该点由 §4.1 的 **LLVM IR 逐字节恒等**直接证明（IR 才是可执行表示，且不含注释）。
- 故 **2197 分母与 2016 覆盖行均不受影响，MM-R10 实测的 2016/2197 = 91.76% 继续有效。**

**本包明确未声称**：

- `crates/application/src` **仍无可执行行**（占位代码）→ **不计 100%**；
- **全体 Rust/TS 80% 门槛未测**（只测 domain）；
- **分支覆盖率仍未验证**（`domain-export.json` 的 `branches` 为空数组）
  → **不写「0 分支所以 100%」，不用 region 89.87% 顶替**，分支 ≥ 85% 门槛仍未验证；
- **DEV-02-11 未标 DONE**；
- 旧值 86.80% 已被 MM-R10 取代，但**前值本身是审计在 0025 之前的实测**，不是错误数据。

---

## 9. 缺失历史证据的新映射（**不伪造、不覆写历史**）

| # | 项 | 处置 | 落点 |
|---|---|---|---|
| 1 | **DEV-02-08 外部 resign-final JSON** | **永久标记为缺失**。路径 `/Users/chenzhiqing/.minimax/teamspace-audits/dev-02-08/evidence/wave2-macos-arm64-dev0208-resign-final.json` 属 Human 侧**外部**目录，**不创建、不重建、不代填、不以任何当前证据冒充** | `deep-audit-2026-10-08/historical-external-evidence-check.json`（`exists:false`） |
| 2 | **归档本地死链** | `docs/archive/v1-planning/审计整改说明.md:4` → `../PRD-Figma审计报告.md`（解析为 `docs/…`，不存在）。**当前替代证据 = `docs/archive/v1-planning/PRD-Figma审计报告.md`**（同目录，SHA `253b7ad2…`）。死因是**多写一个 `../`**，非内容缺失。**只建映射，不改死链本身**（该文件不在写集内） | `inventory/dead-links.json`（`markdown_dead_links: 1`） |
| 3 | **历史本地 provenance 引用缺失** | `docs/reviews/WAVE-2-DEV-02-07-CLOSURE-2026-09-29.md:84` → `evidence/tool-provenance.json`（不存在）。**当前替代证据**：工具侧 `tool-binary-hashes.json`（`fa990c6f…`）、`tool-versions.log`（`9eae9821…`）、`toolchains.log`；source 侧 `previous-source-provenance.json`（`569c35fa…`）、`batch-source-provenance.json`（`bdc0c10f…`）、`main-messaging-replay-provenance.json`（`24a4453b…`）。**旧件不重建；本包不声称替代件与旧件等价** | `inventory/evidence-references.json`（`exists:false`） |
| 4 | **旧 r13 证据** | `wave2-macos-arm64-dev0307-fencing-r13.json`（摘要 `ddf55717…f47`）**不覆盖当前输入**（当前摘要 `b3dcb135…fb`，313 个输入中 3 个文件已变）。按 ADR-0005 失效规则，**不得当作当前结论**。本包不以 r13 支撑任何结论 | 审计 §5；`docs/coordination/README.md:20` |
| 5 | **TS 测试总数** | 原项目报告 37、整改报告 38 是总控汇总口径错误。逐组求和实际 **35 / 36**，当前 **36**（六组 5/1/21/1/4/4）。**该错误不归 MiniMax**，仅作事实记录；原始日志与历史报告**保持不变** | `deep-audit-2026-10-08/ts-count-correction.json`；审计 §5 |

详细表格见 `MM-R09-acceptance-index.md` §4。

---

## 10. 节点状态（**未修改任何正式状态**）

| 节点 | 口径 |
|---|---|
| DEV-01-05、01-08、02-05、02-06、02-10、03-03 | 审计 §5 **直接重开**，仍 BLOCKED |
| DEV-02-11、02-12、03-02、03-04、03-05 | 按**真实依赖暂停** |
| 正式准入表 | **23 DONE / 18 BLOCKED / 60 PLANNED，共 101**（文档/依赖状态，**不是产品完成率**） |

本包**未修改** `docs/development/DEVELOPMENT_NODES.md`，**未解除任何 BLOCKED，未自判 DONE**。
**「修复完成但依赖未通过」仍为 `BLOCKED`**（MM-R01/R03/R05 即属此类）。
**正式准入由 Codex 裁定。**

---

## 11. `SELF_CHECK_PASS`

判定依据：

1. 两处订正**只改注释**，且由 **LLVM IR 逐字节恒等**（`c70b72f6…`）证明**零可执行行变化**——
   这是比「可执行行计数相同」更强的判据；
2. 三条验收命令**全部 exit 0**：`cargo fmt --all -- --check`（无漂移）、
   严格 rustdoc（`-D warnings`）、`cargo test -p teamspace-domain`（287 passed / 0 failed）；
3. 既有失败**先复现后登记**（206/3、79/11、lint exit 2），逐条定位 panic 行，**一律未修**；
4. 覆盖率**引用 MM-R10 实测**并重新固定 SHA，未重复昂贵测量，未声称任何未测项；
5. 缺失历史证据**未伪造、未覆写**，三项建立到当前证据的新映射；
6. 各包 20 个输出 SHA 与当前工作树**零漂移**；
7. 写集**未越界**：未改任何行为代码、迁移、测试、其他源码、历史 evidence、生成 TS、
   根 manifest/lockfile、正式节点状态、ADR 接受状态；
8. **零新增依赖、零联网、零费用、未提交/推送/部署**。

### 11.1 剩余风险

| # | 风险 | 等级 | 说明 |
|---|---|---|---|
| 1 | **`pnpm run lint:ts` 崩溃（exit 2）遮蔽既有 lint 错误** | 中 | §6.3。根因在 MM-R10 evidence 布局 + `eslint.config.mjs` ignore 作用域。**需 Codex 签发写集**才能修。本包如实登记，不掩盖 |
| 2 | **`crates/adapter-supervisor/src/lib.rs` 输出 SHA 未登记** | 低 | MM-R05 RESULT 未记录该文件 SHA，本包无法用既有证据复核「当前值 = MM-R05 交付值」。已在索引 §1.5 登记为**归档缺口** |
| 3 | **MM-R10 的变异日志未逐条存盘** | 低 | 16/16 被检出的结论记录在 RESULT §4，但原始变异日志不可独立复核。已在索引 §6 登记 |
| 4 | **11 个 adapter-supervisor 测试仍红** | 中 | 根因在 fixture 传输层，非产品代码。写集扩展待 Codex 签发 |
| 5 | **3 处版本计数断言仍红** | 中 | Human 已裁决留给 Codex。若 Codex 未在最终门禁前处理，persistence-sqlite 将保持 3 红 |
| 6 | **本包 IR 证明覆盖 domain crate 整体** | 低 | 两侧 IR 均为 `teamspace-domain` 全 crate 编译产物，**不是**只编译这两个文件。因此该证明同时覆盖「这两处订正未影响 domain 的任何可执行代码」，强于所需 |
| 7 | **canonical Mac 21 项在线门禁未验证** | — | **未运行**，属 Codex 最终门禁。离线通过**不等于**在线门禁通过 |
| 8 | **分支覆盖率未验证** | — | branch 映射未产出，≥ 85% 门槛仍未验证 |

### 11.2 回退

1. **单文件回退**：`cp MM-R09/lib.rs.PRE-EDIT crates/domain/src/lib.rs`、
   `cp MM-R09/task_state_machine.rs.PRE-EDIT crates/domain/src/task_state_machine.rs`，
   然后 `cargo build -p teamspace-domain --locked --offline`。
   两份 PRE-EDIT 快照的 SHA 已在 §2 登记，可校验。
2. **无 migration、无 lockfile、无生成物、无外部状态需要回滚**。
3. **不建议回退**：回退等于把已确认错误的文档写回去。
4. 本包**未提交/推送/部署**，回退**不需要联系任何外部系统**。

---

## 12. 停止条件核对

| 条件 | 结论 |
|---|---|
| 规格冲突 | **未发现**。两处订正均与审计 §5 方向一致，未改技术栈、领域状态、wire 字段/错误码 |
| 写集冲突 | **无**。全部写入落在 5 条允许路径内 |
| 需改行为代码 | **未改**，且已用 IR 恒等证明 |
| 需新增依赖 | **未新增**。`Cargo.toml`/`Cargo.lock` 未动 |
| 外部权限/费用影响 | **无**。新增费用 $0，未联网 |
| 发现真实 bug | **本包两处目标：均非代码缺陷**（UUID 注释错误但实现正确；TaskTransition 注释表述错误但代码无关）。§6.3 为**新发现的门禁回退**，已登记未修 |
| 自判 DONE | **未自判**。节点状态未改 |
