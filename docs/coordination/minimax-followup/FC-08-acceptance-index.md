# FC-08 验收索引（01-05、02-11：文档、变异、覆盖证据）

- 签发：Codex 总控，[接续任务表 FC-08](../MINIMAXCODE-FOLLOWUP-2026-10-08.md#fc-08-详细要求) 第 10 包
- 执行：Minimaxcode Worker 子会话，2026-10-09
- 输入：HEAD `1a14b8795e76fc373bef326eb5df96f9ac173f71`，分支 `codex/wave-2-ci-validation`，共享 dirty worktree（126 项继承改动）**未 reset / stash / clean / checkout**
- 证据根：`docs/reviews/evidence/minimax-followup/FC-08/`
- 结论：**SELF_CHECK_PASS**（本包五项均已按要求落到可复核证据；不构成任何节点的 DONE，DONE 权限属 Codex）

---

## 0. 本包五项落点速查

| # | 任务包要求 | 落点 | 结果 |
|---|---|---|---|
| 1 | 撤回「11 值」指控 | [FC-08-errata-artifacts-12-values.md](FC-08-errata-artifacts-12-values.md) + `errata/` | 5 处旧指控全部撤回，旧文字一字未删 |
| 2 | R09 最终差异 | [§2](#2-r09-最终差异纯注释成立) + `04-r09-reconciliation.txt` | 纯注释成立；21699 已正名；文件数已实数 |
| 3 | R10 重跑 16 变异 | [§3](#3-r10-16-个变异重跑) + `mutation/LEDGER.md` | 16 杀 / 0 未证（附两项限定） |
| 4 | 重新度量覆盖率 | [§4](#4-覆盖率重新度量) + `coverage/coverage-summary.json` | 2016/2197 = 91.76%，branch 仍未验证 |
| 5 | 最终清单与 hash 登记 | [§5](#5-最终清单权限与-hash-登记) + `hash-registry/` | lib.rs 最终 hash 已串行登记 |

---

## 1. 撤回旧指控（artifacts 12 值）

完整文档：[FC-08-errata-artifacts-12-values.md](FC-08-errata-artifacts-12-values.md)。此处只列结论与三条独立证据。

**撤回范围**：5 处旧文字提出的「审计 INSERT 只给 11 个值 → 该条 RED 不可复现」——`MM-R02-RESULT.md:167 / :169 / :267`、`MM-R03-RESULT.md:257`、`MM-COORD-review-gate-2026-10-08.md:80-82`。

**旧文件一字未删。** 撤回全部通过新勘误表达。

三条互相独立的证据：

| # | 证据 | 结果 |
|---|---|---|
| A | 用原 main-repro 的 24 份迁移在真实 SQLite 建库，`PRAGMA table_info(artifacts)` | **12 列** |
| B | 原 `audit_storage_boundaries.rs:55` 的 INSERT 顶层逗号切分 | **12 个值**，第 8 位 `?`（绑定参数）、第 9 位显式 `NULL` |
| C | 逐字节复制原 main-repro（129 文件 SHA 全一致）+ **全新 target** 重跑 SQLx | **exit 101，1 passed / 3 failed**，`retained history DELETE accepted for ["runs","reviews","artifacts","approvals","messages"]`；日志中列数不匹配错误出现 **0** 次 |

C 的 panic 行号本身即反证：`:55` 是 INSERT、`:64` 是 `COUNT(*)` 断言、`:67` 才是 DELETE 后的断言——**测试能走到第 67 行，说明 INSERT 执行成功了**。

证据：`errata/artifacts-value-count.txt`、`errata/red-replay.raw.log`（未过滤）、`errata/red-replay-exit.txt`。

**不撤回、继续保留**：`"references"` 与 `delivery_attempts` 两表的 DELETE 守卫缺口（真实缺陷，见审计 §3 MR-03）。

**顺带记录的成因**：本轮我自己第一次用正则数列 `artifacts` 表，得到的**也正好是 11**（漏掉 `checksum_sha256`）。一个看起来完全合理的机械计数在这张表上同样给出 11——这解释了旧指控的成因，也说明必须让数据库自己回答。

---

## 2. R09 最终差异：纯注释成立

证据：`04-r09-reconciliation.txt`、`01-r09-reverify.txt`、`02-r09-final-diff.txt`、`03-r09-inventory.txt`。

### 2.1 最终 PRE/POST 差异确为纯注释 ✅

用**两侧都被保留的快照**对拍（不是对拍工作树，也不是引用 `02-*.txt`）：

| 文件 | 删/增 | 行数 |
|---|---|---|
| `crates/domain/src/lib.rs` | 1 删 / 1 增 | 643 → 643 |
| `crates/domain/src/task_state_machine.rs` | 3 删 / 3 增 | 668 → 668 |

所有变更行均为 `///` 文档注释；非注释变更行 **0** 条。

工作树当前仍与 R09 的 POST-EDIT 快照一致（`lib.rs 5072ba4b…`、`task_state_machine.rs f7f2ea56…` 均 MATCH），R09 之后无人改动这两个文件。

### 2.2 `02-comment-only-diff.txt` 是中间件，与最终不一致 ⚠️

| | `02-comment-only-diff.txt`（中间） | 保留的最终 POST-EDIT |
|---|---|---|
| `task_state_machine.rs` 行数 | **669** | **668** |
| 该 hunk | 4 删 / 5 增（净 +1） | 3 删 / 3 增（净 0） |

且 `02-*.txt` 右侧对的是**工作树**（`+++ crates/domain/src/task_state_machine.rs`）而非保留快照，工作树一动就无法复现。

**处置**：标为 INTERMEDIATE，不得作为最终差异引用。最终差异以 `FC-08/02-r09-final-diff.txt` 为准。
注：`MM-R09-RESULT.md:153` 的正文数字（3 删 3 增、643/668 不变）本身是**对的**，缺陷只在 02 证据文件。

### 2.3 `21699` 的口径 ✅

本轮实测：`grep -c '^  '` 对两份 IR 都恰好 = **21699**；IR 总行数各 58004。

**它是「以两个空格开头的 IR 文本行数」，不是严格 instruction 数**——LLVM IR 在同一缩进层里混排指令、标签、attribute group、注释与空行。已按此口径书写，不冒称 instruction 数。

### 2.4 failed rlib 路径：自我证伪，**不是 GREEN** ✅

rlib 前后确实不同（`3f14be0f…` → `a22872d8…`），原因是被测改动本身把 `random[1]`→`random[2]` 编进了 crate metadata。两个 SHA 已从留存证据重新读出并与 `MM-R09-RESULT.md:140-141` 一致（POST rlib 字节未留存）。

**该路径状态：提出假设 → 被证伪 → 换用 LLVM IR 重新测量。** 它**不是 GREEN 结果**，也**不是产品缺陷**，更不得计入任何通过项。承载「纯注释」结论的是 §2.1/§2.3 的 IR 恒等（两份 `.ll` SHA 同为 `c70b72f6…`，`cmp` IDENTICAL）。

### 2.5 文件数量已实数（不按编号范围）⚠️

| 口径 | 实测 |
|---|---|
| `MM-R09/` 目录常规文件（不含 `.DS_Store`） | **23** |
| `MANIFEST.txt` 条目数 | **22**（不含其自身） |
| MANIFEST 有条目但磁盘无 / 磁盘有但 MANIFEST 无 | 0 / 1（`MANIFEST.txt` 自身） |
| `MM-R09-RESULT.md:105` 与 `:247` 的声明 | **「21 份」——错的** |

成因：§7 表格 17 个数据行里，有 4行各含 2 个文件、1 行含 4 个文件，**行数不是文件数**；只数 `00`–`14` 则得 15。三个可辩护的口径（23 / 22 / 17 / 15）都不是 21。

---

## 3. R10：16 个变异重跑

台账：`mutation/LEDGER.md`（由 `ledger.json` 生成，表格/汇总/逐行不会漂移）；脚本 `mutation/run_mutations_fc08.py`。

- **变异前绿基线**：exit 0，**40 passed / 0 failed**
- **16 个变异**：**16 杀（`killed_by_assertion`），0 未证，0 编译失败**
- **全部撤销后绿回归**：exit 0，**40 passed / 0 failed**
- **还原验证**（针对实际保留的副本 `mutation/workspace/`）：137 文件比对 **0 差异**；80 份 `.rs` 与生产 **0 差异**
- 每条都有：脚本、补丁 `patches/M*.patch`、**未过滤原始日志** `logs/M*.raw.log`、退出码、还原哈希

### 3.1 读这张表必须带的两条限定

1. **这是对「声明」的复现，不是对「原日志」的复现。** R10 §4 自述「变异日志未逐条存盘为本轮正式 evidence」，原始 M1–M16 注入现场从未归档。本台账证明的是「按 R10 文字描述重建的变异在**当前**代码上会被断言检出」，**不能**证明当时跑的就是同一批变异。
2. **M14 的原始表述无法编译。** 只加一个枚举变体，`approval_state_machine.rs:65` 的穷尽 `as_str` 立刻 E0004（见 `HARNESS-ITERATION-LOG.txt` 的 run-1 实测）。R10 原表称其「被 2 个测试检出」；本轮补了 `ALL` 与 `as_str` 两处配套改动才使其成为**真实可编译**的变异，实测由 **1 个**测试检出。**R10 的「2 个」未能复现。**

### 3.2 标题 / 表格 / 输出三处数字一致 ✅

R10 §4 标题写「12/12」而表列 M1–M16，这个不一致由本包结清：本包三处（标题、表格、输出）一律为 **16 / 16 / 16**，且该数字指「声明数 = 重跑数 = 全部被断言检出数」，不含未证项（本轮未证为 0）。

### 3.3 台架本身踩到的坑（已留证）

`mutation/HARNESS-ITERATION-LOG.txt`：run-1 的还原哈希**全部通过**，但绿回归**失败**——`shutil.copy2` 保留了旧 mtime，cargo 判定「已编译」而复用了最后一个变异的二进制。改为 `copyfile` + `os.utime` 后 run-2 全绿。run-1 的原始日志已被 run-2 覆盖，该段为会话控制台输出的转录，**不是**留存产物，已在文件中注明。

---

## 4. 覆盖率重新度量

产物：`coverage/coverage-summary.json`、`coverage/input-manifest.json`、`coverage/run_coverage_fc08.py`。

### 4.1 固定口径

| 项 | 值 |
|---|---|
| **分母** | `crates/{domain,application}/src/**/*.rs` 的**全部映射行**；含从未执行的函数的映射；**不过滤任何低覆盖 Rust 文件**；tests/examples/dependencies 不计入 |
| **测试集** | `teamspace-domain` + `application` 的**当前完整测试集**（`cargo test`，全部 target，无过滤、无 ignored 替换） |
| **工具版本** | 编译 rustc/cargo 1.98.0（`rust-toolchain.toml` channel=1.98.0，LLVM 22.x 产 profile/covmap）；读取 Apple LLVM **21.0.0** `llvm-profdata`/`llvm-cov` |
| **source SHA** | domain `src/` 树 `5626cb9d…`（16 文件）；domain `tests/` 树 `4cfc1fac…`（16）；application `src/` 树 `ca2208b8…`；根 manifest `e2540025…`；`Cargo.lock` `5822d1d1…`；`rust-toolchain.toml` `495c69d1…` |
| **新鲜度** | 本轮**新副本 + 新 target + 新 profile 目录 + 新 manifest**；46 份 raw profile；`coverage.profdata` 374,200 bytes；与 MM-R10 / 审计的历史 profile 无复用 |

### 4.2 数字

| crate | 文件 | 行覆盖 | region | branch |
|---|---:|---|---|---|
| `teamspace-domain` | 16 | **2016 / 2197 = 91.76%** | 2227 / 2478 = 89.87% | **未验证** |
| `application` | 0 | **无可执行行**（`line_percent: null`） | — | **未验证** |

- 独立复现了 MM-R10 的 2016/2197 = 91.76%，但**是从零重测**，不是沿用旧数字。
- **`application` 仍是占位，绝不写 100%**，summary 中 `is_placeholder_not_product_coverage: true`。
- **region 89.87% 不是 branch**，不得顶替。

### 4.3 `branch_coverage_verified` 保持 `false` ✅

`llvm-cov export` 在所有选中文件上返回 **0 个非空 `branches` 数组**——本次构建未产出任何 branch 映射。因此：

- **不写**「0 分支所以 100%」；
- Gate C 的**分支 ≥ 85% 仍然未验证**；
- 需要能与 Rust LLVM 版本匹配的 `llvm-tools` + `llvm-cov --show-branch-summary` 后另行测量。

### 4.4 覆盖副本可重跑性的一个真实缺陷 ⚠️

MM-R10 §6/A-3 记载：为了省 932 KB，把 `source-copy/apps/` 整个删掉，并断言「`2016/2197 = 91.76%` 与其证据不受影响」。

**该断言对数字成立，对可重跑性不成立。** 根 manifest 是 `members = ["apps/*/src-tauri", "crates/*"]`，删掉 `apps/` 后 glob 空匹配，cargo 无法加载工作区。本轮实测：

```
$ cargo metadata --manifest-path MM-R10/source-copy/Cargo.toml --locked --offline --no-deps
error: failed to load manifest for workspace member .../source-copy/apps/*/src-tauri
Caused by: No such file or directory (os error 2)
exit 101
```

**MM-R10 留存的那个副本今天已经跑不起来了。** 时间戳佐证：覆盖结果写于 14:56，`source-copy` 目录改于 15:26——即数字是在删除 `apps/` **之前**测得的，删除后从未重跑。

本轮副本因此包含 `apps/*/src-tauri`（不构建、不计入分母，只为让工作区能加载），并在脚本里加了 **`cargo metadata` 预检**：工作区加载不了就直接拒绝出数字，避免再把 `exit 101`（一个测试都没跑）误当测量结果。这个坑本轮实际踩中过两次——第一次 `exit 101` 就是 manifest 加载失败。

---

## 5. 最终清单、权限与 hash 登记

### 5.1 共享 `crates/persistence-sqlite/src/lib.rs` 串行登记

产物：`hash-registry/shared-lib-rs-hash-registry.txt`（由 `build_hash_registry.py` 生成）。

| 包 | 迁移 | 输入 SHA（前 16） | 输出 SHA（前 16） |
|---|---|---|---|
| MM-R01 | 0025 | —（链起点） | `7a3fe2a0d12d22e9` |
| MM-R02 | 0026 | `7a3fe2a0d12d22e9` | `4611e7e940d986ac` |
| MM-R03 | 0027 | `4611e7e940d986ac` | `fe3561c4a288170d` |
| FC-02 | 0028 | `fe3561c4a288170d` | `4a06dfa1d1962263` |
| FC-03 | 0029 | `4a06dfa1d1962263` | `6023d62acd8f9174` |

- **时序映射连续性：0 处断裂**（每包的输入 = 上一包的输出）。
- **最终值**（本轮从工作树重算）：`6023d62acd8f91741599ea75d5141f873a28d53f9924f34703ec5cb99786f818`，与 FC-03 登记一致。
- 迁移注册现状：`Migration::new` 共 **29** 个，**连续 1..29**；磁盘 `.sql` **29** 份（最高 `0029_run_terminal_integrity.sql`）；两者相等。

⚠️ **登记缺口（如实登记，不代填）**：**FC-03-RESULT.md 未记录其输入 hash**（`4a06dfa1…`）。该值由 FC-02 的输出衔接而来，链条自洽，但 FC-03 文档自身缺这一行。历史 RESULT 属只读归档，本包不改。

### 5.2 本包实际最后一次：结果 / 失败 / 未运行 / 权限

| 类别 | 内容 |
|---|---|
| **本包结果（通过）** | ① 撤回勘误（3 条独立机器证据）② R09 纯注释成立 + 21699 正名 + 文件数实数 ③ 16/16 变异重跑，基线与回归均绿，还原零差异 ④ 覆盖率重测 2016/2197 = 91.76%，branch 保持 `false` ⑤ lib.rs 最终 hash 串行登记 |
| **本包发现的问题** | R09 `02-*.txt` 为中间件且与最终不一致；R09「21 份 evidence」计数错误（实为 23）；R10 §4 标题「12/12」与 M1–M16 不符；R10 M14「被 2 个测试检出」未能复现（且原表述不可编译）；MM-R10 留存副本已不可重跑；FC-03 未记录输入 hash |
| **失败项** | **无。本包自身 0 条失败。** 上述「本包发现的问题」均为对**历史文档/证据**的勘误，不影响当前代码状态 |
| **未运行 / 未验证** | 完整 canonical Mac 21；在线 advisory / `pnpm install` / `cargo audit` / RustSec；真实 Harness / Keychain / 备份恢复；最低 macOS 14 实机；A11y；性能；分支覆盖率；application 真实代码覆盖（占位）；全产品覆盖 |
| **权限** | **本包未自判任何 DONE，未解除任何 BLOCKED，未启动依赖节点。** `docs/adr/0007-…` 仍为 **Proposed**，未改动。 |

### 5.3 源码改变后的门禁重跑

FC-08 **未改动任何生产源码**（只新建 `docs/` 下的文档与 evidence）。为不拿过去的日志当最终结果，受影响门禁在**当前源码**上重跑：

| 门禁 | exit | 结果 | 原始日志 |
|---|---:|---|---|
| `cargo test -p persistence-sqlite --locked --offline --no-fail-fast` | **0** | **233 passed / 0 failed / 0 ignored**，31 个 target 全 ok | `gate-persistence-sqlite.raw.log` |
| `cargo test -p teamspace-domain --locked --offline --no-fail-fast` | **0** | **287 passed / 0 failed / 1 ignored**，18 个 target 全 ok | `gate-domain.raw.log` |

两条均与上一轮记录的终态一致，**无回归**。（ignored 是既有文档示例，非新增屏蔽。）

### 5.4 未在本包范围内、未改动的东西

`crates/**`、`scripts/**`、`Cargo.toml`、`Cargo.lock`、`eslint.config.mjs`、`sources/**`、旧迁移 `0001`–`0029`、历史报告 / evidence、根 manifest / lockfile、节点状态、ADR 接受状态——**全部未改**。

**因此没有需要回传裁决的写集冲突。**

---

## 6. 证据目录

```
docs/reviews/evidence/minimax-followup/FC-08/            （27 MB）
├── 01-r09-reverify.txt / 02-r09-final-diff.txt / 03-r09-inventory.txt
├── 04-r09-reconciliation.txt          ← R09 五项结论
├── gate-persistence-sqlite.raw.log    ← 当前源码重跑，233/0
├── gate-domain.raw.log                ← 当前源码重跑，287/0
├── errata/
│   ├── derive_artifacts_value_count.py
│   ├── artifacts-value-count.txt      ← 12 列 / 12 值的机器推导
│   ├── red-replay/                    ← 原 main-repro 逐字节副本（129 文件）
│   └── red-replay.raw.log             ← 原 RED 独立回放，exit 101，1/3
├── mutation/
│   ├── run_mutations_fc08.py          ← 台架脚本
│   ├── render_ledger.py
│   ├── LEDGER.md / ledger.json        ← 16 条台账
│   ├── HARNESS-ITERATION-LOG.txt      ← run-1 失效与修复
│   ├── restore-verification.txt       ← 还原哈希（针对保留副本）
│   ├── logs/M*.raw.log                ← 未过滤原始日志 ×18（16 变异 + 2 基线）
│   ├── patches/M*.patch               ← 补丁 ×16
│   └── workspace/ pristine/           ← 保留副本与基线副本
├── coverage/
│   ├── README-authoritative-files.txt ← 哪个文件权威、哪个已作废
│   ├── run_coverage_fc08.py           ← 含 cargo metadata 预检
│   ├── coverage-summary.json          ← 分母/测试/工具/SHA 全登记
│   ├── input-manifest.json / command.json
│   ├── *exit.txt / instrumented-test.log / merge.log / export.json
│   ├── coverage.profdata / profiles/  ← 46 份 raw profile
│   └── source-copy/                   ← 被测的逐字节副本
└── hash-registry/
    ├── build_hash_registry.py
    └── shared-lib-rs-hash-registry.txt
```

**三个 `target/` 构建目录（合计 856 MB）已在取证后移除**，目录从 884 MB 降到 27 MB。它们是可重新生成的 cargo 构建输出，不是证据；移除不影响上面任何一个数字，runner 每次运行都会从零重建。此举与 R10 当初移除 `node_modules` 拷贝同源——但本轮**没有**因此影响任何已登记数字，因为覆盖是在副本 + 独立 target 上测的。`coverage/README-authoritative-files.txt` 说明了哪些文件权威、以及一个已作废的 `runner-stdout.log` 为何被删除。

---

## 7. 边界与费用

全部 cargo 命令带 `--locked --offline`，`CARGO_NET_OFFLINE=true`，PATH 固定到本机工具链。**无联网、无下载、无外部账号、无新增依赖、未提交/推送/部署、未操作生产数据。费用恒为 $0。**

只格式化本包新增文件（两个 `.py` 与本 `.md`，未运行 `cargo fmt --all` 或 `cargo fmt -p`——本包没有改动任何继承 dirty 的 Rust 文件）。历史 evidence 与历史 RESULT 均只读。

**自检不等于 DONE；有需要 Codex 独立反例审查与最终准入。**