# AF-01 整改回传：关闭注册 `%` 字节的单层编码漏检

- 依据任务表：[`MINIMAXCODE-AUDIT-FOLLOWUP-2026-10-09.md` §AF-01（15–31 行）](../../reviews/FC-RETURN-AUDIT-2026-10-09.md)
- 依据审计：[`FC-RETURN-AUDIT-2026-10-09.md` §4（52–66 行）](../../reviews/FC-RETURN-AUDIT-2026-10-09.md)
- 证据目录：[`docs/reviews/evidence/minimax-audit-next/AF-01/`](../../reviews/evidence/minimax-audit-next/AF-01/)
- 主责 Minimaxcode，Codex 安全复核。**本包不自判 DONE，正式节点准入属 Codex。**

## 1. 输入 / 输出

| 项 | 值 |
|---|---|
| HEAD | `1a14b8795e76fc373bef326eb5df96f9ac173f71`，分支 `codex/wave-2-ci-validation` |
| 执行模型 | MiniMax-M3.1-Flash-Preview（MiniMax Code Desktop，worker 子代理） |
| 输入 `crates/observability/src/lib.rs` | `9251f0edaa7fc203e7c02ad92cd5293f4325e830ecfc9d7f77d46e749b66091e`（HEAD） |
| 输入 `crates/observability/tests/structured_logging.rs` | `fcdfa6cade290c6141108a10af1540244214ccc0e85a8d4f96a1b4f61e7ee84e`（HEAD） |
| 输出 `crates/observability/src/lib.rs` | `8d2fe94ea27a4befd341b77ccd64e6e5a27c3e24371485a13e7ce58a8c0ede1e` |
| 输出 `crates/observability/tests/structured_logging.rs` | `368eff5f7b3b1601e8b037193c71effca6a05021daf27c855d2c86a02078a30e` |

全程 `--locked --offline`，`PATH=/Users/chenzhiqing/.minimax/toolchain/bin`，`CARGO_NET_OFFLINE=true`，
rustc/cargo 1.98.0。无联网、无提交、无推送、无部署、无生产操作，新增费用 $0。
未新增依赖、未扩大写集、未改动语义边界以外的行为。共享 dirty 工作树未做 reset/stash/clean/checkout。

## 2. 缺陷与修复

**根因**：原 `match_at` 逐字节贪心，注册字节为 `%` 时字面分支先命中吃掉 `%`，escape 分支永不尝试，
于是 `%ABC` / `A%25B` / `%%` 全部原样泄露。

**修复形态**：把单条贪心路径换成**分层可达集合**（reachability DP）匹配。
匹配第 `i` 个注册字节后，若其中 `e` 个走了 escape 拼写，则恰好消耗 `i + 2e` 字节，
所以第 `i` 层的可达位置由下标集合 `0..=i` 索引，最多 `i + 1` 个状态；
第 `i` 层 `O(live)`，全 `n` 层 `O(n²)` 时间、`O(n)` 空间。末端取**最长可达匹配**脱敏。

**明确资源上界（已写入 `lib.rs` 注释）**：

- 每层可达状态 ≤ `i + 1`，层内每个状态只检查一次，**无指数级拼写枚举、无无界递归回溯**；
- 只存活的升序去重列表（`Reachability`），复用缓冲区，**每个候选位置零分配**；
- 沿用原有候选扫描上界：候选是至多两个 anchor 字节的出现位置，且不越过 `input.len() - min_len`；
- 不做递归 percent 解码、不折叠整个输入大小写：未转义字母仍大小写敏感（`Audit` ≠ `audit`），
  escape 内部两位 hex 仍大小写不敏感（`%2d` = `%2D`）。字面 `%` 与 escape `%25` **两条拼写都匹配**，
  不是把 escape 优先换来另一侧失守。

## 3. 三条审计反例 × 四入口（实跑）

三条反例均通过**真实公共 API** 与真实 `JsonSubscriber` 断言危害，不测手抄 matcher；
泄露断言先对渲染结果做**一次** percent 解码，要求解码后不再含注册 secret。

| 注册 secret | 合法一次编码 | 修复前 | 修复后 |
|---|---|---|---|
| `%ABC` | `%25ABC` | 原样保留，泄露 | 四入口全部脱敏 |
| `A%B` | `A%25B` | 原样保留，泄露 | 四入口全部脱敏 |
| `%%` | `%25%25` | 原样保留，泄露 | 四入口全部脱敏 |

四入口 = `Redactor::sanitize` / `Redactor::redact_error` / `Redactor::redact_event` / `JsonSubscriber`
真实 `tracing::error!` 事件（`log_error_detail` 走真实 subscriber 并解析 JSON 行）。

证据 `four-entry-points.log`（未过滤原始 stdout）+ `four-entry-points.summary`。
三条用例按**当前完整测试名**逐条 `--exact` 单跑，每条都是独立进程：

```
registered_percent_at_start_is_redacted_in_all_four_entry_points       ok   1 passed  0 failed  38 filtered out  EXIT=0
registered_percent_in_middle_is_redacted_in_all_four_entry_points      ok   1 passed  0 failed  38 filtered out  EXIT=0
registered_percent_consecutive_is_redacted_in_all_four_entry_points    ok   1 passed  0 failed  38 filtered out  EXIT=0
TOTAL: 3 passed, 0 failed across 3 counterexample runs.
```

协调端另以子串过滤独立复现：`cargo test -p observability --locked --offline registered_percent`
→ **5 passed / 0 failed（34 filtered out）**，五条为上述三条反例加
`registered_percent_position_...` 与 `registered_percent_malformed_...`。

### 3.1 空过工件自查（协调端第二轮要求，已返工）

**`four-entry-points.summary` 原为一份空过，已删除重做。** 原内容三段全是
`0 passed; 0 failed; 39 filtered out`，即过滤名未匹配到任何测试；根因是该摘要在三条测试写入文件
**之前**生成、之后未重跑。原文件已不作证据，现由 `four-entry-points.log`（真 `1 passed` × 3）重新生成。

新摘要改为**转录自重新生成的 `four-entry-points.log`**（真 `1 passed` × 3），并附一段
`CORRECTION` 说明原空过的根因（摘要在三条测试写入文件前生成、之后未重跑）与重建方式，
且明确声明「本摘要为转录，源为未过滤日志」，避免转录件被当成独立证据。
转录中的每条 result 行均为 `1 passed; 0 failed; ... 38 filtered out`，
**不存在 `0 passed … N filtered out` 的空过行**。

自查判据（本次实际执行）：凡 result 行满足 `passed == 0` 且 `filtered > 0` 即判为空过、不得计入通过项。

**全包扫描**（`*.log` / `*.summary` / `*.exit` 共 18 份）确认无第二份空过工件：
其余命中 `0 passed` 的行全部是 `0 filtered out` 的 unit / doc-test 空目标，且与同文件内真实的
`29` / `39` / `64` passed 块并存，属正常 cargo 多 target 输出，不是空过。

**同名不同内容的两份文件（关系说明，避免复核者猜）**：

| 文件 | 实际内容 | 关系 |
|---|---|---|
| `mutation-3-no-dedup.log` | MUTATION 3：去掉 `Reachability::mark` 的重复状态合并 | 编号 3 |
| `mutation-4-no-early-exit.log` | MUTATION 4：禁用不可达层提前退出 | 编号 4，与上者**内容不同** |
| `mutation-4-equivalent.log` | MUTATION 4 的说明性记录（等价变异体结论） | 与上一份**同编号的补充说明**，非独立测试运行 |

文件名保持历史命名未再改动（M3 文件名含 `no-dedup`、M4 含 `no-early-exit`），以免复核者先前引用的路径失效；
每份文件首行 `###` 标题均标明真实编号与内容，两者不构成重复证据。

## 4. 新增对照覆盖（10 个新用例，29 → 39）

`crates/observability/tests/structured_logging.rs`，正负对照都有：

1. `registered_percent_at_start_...` — `%` 在首，四入口 + 字面拼写同时匹配（证明非 escape 优先）
2. `registered_percent_in_middle_...` — `%` 在中，四入口 + `A%25B` / `A%B` / `%41%25%42` 混合拼写
3. `registered_percent_consecutive_...` — `%` 连续，四入口 + `%%` / `%25%` / `%25%25` / `%%%`
4. `registered_percent_position_is_covered_at_start_middle_end_and_repeated` — 首/中/尾/2 连/3 连/2 转 1 字面
5. `a_registered_secret_containing_literal_escape_fragments_is_still_redacted` — 注册 secret 含 `%25` / `%41` 片段
6. `fully_and_partly_escaped_percent_secrets_agree_on_all_mixed_spellings` — 全转义/部分转义/未转义四种组合，无关 escape 保留
7. `registered_percent_malformed_and_truncated_encodings_are_left_untouched` — 15 条畸形/截断负对照（`%`、`%2`、`%2G`、`%25%2`、`%2%` …）
8. `utf8_registered_secret_with_percent_bytes_is_redacted` — UTF-8 secret 含 `%`，全 escape / `%25` 混合 / 字面
9. `empty_and_overlapping_percent_secrets_are_handled` — 空 secret（三种注册法）+ 重叠 secret，每个出现恰好替换一次
10. `percent_ambiguity_work_stays_inside_its_polynomial_bound` — 资源上界实测 + 正对照

负对照覆盖到「无关内容保留」：`%41%25%25` → `%41[REDACTED]`、包裹文本 `prefix url=` / ` suffix` 原样、
`%2%` / `%2%2` 等畸形原样。

> 写作中修正的自身错误（已在交付物中体现，非产品缺陷）：若干负对照文本其实**含有**注册 secret 或其
> hex 形式（例如 `%%2` 首字节就是字面 `%%`；`%2525%2` 含 `%%` 的 hex 形式 `2525`），断言写反后被真实 RED 抓到，
> 已改为真正无匹配的负对照。这批中间失败是断言错误、不是 matcher 错误，逐条记录在此以免复核者误读。

## 5. 验收命令与实测结果

全部门禁在**当前工作树**实跑，原始未过滤日志见 `gate.log`：

| 命令 | EXIT | 结果 |
|---|---|---|
| `cargo build -p observability --locked --offline` | 0 | 通过 |
| `cargo clippy -p observability --all-targets --locked --offline -- -D warnings` | 0 | 0 warning |
| `RUSTDOCFLAGS="-D warnings" cargo doc -p observability --no-deps --locked --offline` | 0 | 通过 |
| `cargo test -p observability --locked --offline` | 0 | **39 passed / 0 failed / 0 ignored**（unit 0、integration 39、doc 0） |
| `rustfmt --edition 2024 --check <本包两个文件>` | 0 | 通过 |
| `cargo fmt --all --check`（只读，未 write） | 0 | 通过 |

**用例数字取自实际日志**（`grep -E "^test result:" gate.log` → `39 passed; 0 failed`）。
本包基线为修复前 **29 passed / 0 failed**（`baseline-green-pre.log`），现为 **39 / 0**，即新增 10。

### 5.1 workspace 总数（实跑，不沿用 643）

命令：

```
cargo test --workspace --locked --offline -- --test-threads=1
```

**实测 653 passed / 0 failed / 1 ignored，EXIT=0**（`workspace-total.log`）。
68 条 result 行由脚本解析求和得出，非手工点数；1 ignored 为既有。算术自洽：643 + 本包新增 10 = 653。
**643 是 AF-01 之前的基线，post-AF-01 的当前总数以本节的 653 为准。**

首轮以默认并行度跑 workspace 时出现 1 处失败：`adapter-supervisor` 的
`descendant_pipe_holders_cannot_block_shutdown_or_drop`
（`supervisor_fixture.rs:142` "watchdog test process failed: exit status: 101"）。已确证**与 AF-01 无关**：

1. `grep observability crates/adapter-supervisor/Cargo.toml` 无命中——adapter-supervisor 不依赖
   observability；全仓只有 observability 自身提及该 crate，故 AF-01 的改动无法传播到该 crate；
2. 该用例单独跑 3 次全部通过（`1 passed; 0 failed` ×3），只在全 workspace 并行负载下失败，属时序敏感的
   watchdog / pipe-holder 用例抖动。

降为 `--test-threads=1` 后全绿。此为**只读 crate 的既有继承状态**，本包只报告不修。
`workspace-total-attempt1-note.log` 记录首轮事实；该轮原始 stdout 未留存（被第二次运行覆盖），
已如实标注，未拿第二次的日志冒充首轮从未失败。

## 6. 资源上界实测（`resource-bound.log`）

命令：

```
cargo test -p observability --locked --offline --test structured_logging \
  -- --exact percent_ambiguity_work_stays_inside_its_polynomial_bound --nocapture
```

最坏形状：secret = 48 个 `%` + `tail`（n=52），除尾部外每层都歧义可达；输入为长近失配串
（`%25` 重复 20,000 次 + 少 3 字节的全 escape 拼写）。实测输出：

```
AF-01 resource bound: secret_len=52 repeated_in=481224 bytes repeated_elapsed=1.976420042s
AF-01 scaling (same input, ambiguous secret doubled): n=16->86.216083ms n=32->148.785625ms
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 38 filtered out; finished in 2.79s
real 5.64
```

- 481,224 字节 × n=52 的近失配输入，debug profile 单线程 1.98s 完成；
- 同一输入把歧义 secret 加倍：`n=16 → 86ms`、`n=32 → 149ms`，约 1.7×（近二次，非 2^16 倍）；
- 完整拼写仍被脱敏（正对照），边界 `%` / 空串 / 差一字节均原样保留。

对比：指数级拼写枚举在此规模需要 2^48 条路径，不可能完成。

## 7. 变异台账（独立副本，非生产源码）

副本：`docs/reviews/evidence/minimax-audit-next/AF-01/mutation-copy/`（独立 `[workspace]`，自带 `Cargo.toml`，
无 path 依赖，可真正编译运行）。**上一轮的坑已规避**：先在副本上跑通绿基线再变异；
还原哈希打在**实际保留的副本**上；日志为未过滤完整 stdout/stderr。

| 阶段 | 结果 | EXIT | 日志 |
|---|---|---|---|
| 副本绿基线（变异前） | 39 passed / 0 failed | 0 | `mutation-baseline-green.log` |
| 变异 1：还原原始贪心 matcher（AF-01 缺陷） | **29 passed / 10 failed** | 101 | `mutation-1-greedy.log` |
| 变异 2：最短匹配替代最长匹配脱敏 | **34 passed / 5 failed** | 101 | `mutation-2-shortest.log` |
| 变异 3：去掉重复状态合并 | 39 passed / 0 failed | 0 | `mutation-3-no-dedup.log` |
| 变异 4：禁用不可达层提前退出 | 39 passed / 0 failed | 0 | `mutation-4-no-early-exit.log`、`mutation-4-equivalent.log` |
| 还原并复跑 | 39 passed / 0 failed | 0 | `mutation-restored-green.log` |

**变异 1 是真实断言红，不是编译失败**：副本 `Compiling` + `Finished` 均成功，日志中 `error:` 仅出现
1 处且是 `error: test failed, to rerun pass` 横幅；10 条失败全部是 `panicked at ... assertion left != right`，
断言行直接显示期望脱敏却原样输出，例如：

```
registered_percent_at_start... panicked at tests/structured_logging.rs:908:5:
assertion `left == right` failed: % is the first byte: sanitize
  left: "prefix url=%25ABC suffix"
 right: "prefix url=[REDACTED] suffix"
```

**变异 3 / 4 是等价变异体，未被捕获，如实记录**：它们只影响冗余工作量或提前退出，不改变任何可达状态，
因此没有任何正确性断言能捕获。`O(n²)` 上界不受影响——每层最多 `i + 1` 个**互异** escape 计数，
重复项只是冗余副本。这一点不掩饰，也不虚报为已覆盖。

**还原哈希验证（`mutation-restore-verification.log`，本轮硬要求）**。
该证据专门对照**实际保留的 evidence 副本**与**生产源码**，而非临时目录：

```
1. 生产 crates/observability/src/lib.rs                8d2fe94e…  /  tests 368eff5f…
2. 保留副本 mutation-copy/src/lib.rs                   8d2fe94e…  /  tests 368eff5f…   ← 四者一致
3. 变异前基线副本                                        8d2fe94e…  /  tests 368eff5f…
4. diff(保留副本, 生产)     → 无输出，IDENTICAL ×2
5. diff(保留副本, 绿基线)   → 无输出，IDENTICAL ×2      ← 证明无变异残留
6. HEAD(修复前)             9251f0ed…  /  fcdfa6cad…     ← 与修复后不同，证明修复真实生效
```

即：保留副本 ≡ 生产源码 ≡ 变异前绿基线，三者哈希两两相同；且与 HEAD 不同。
变异全程未触碰 `crates/observability/` 生产源码，未提交、未推送。

**清理声明**：交付前仅删除本包变异副本内 `mutation-copy/target/`（cargo 自建可再生构建产物，106M → 380K）。
删除前后 `src/lib.rs` / `tests/structured_logging.rs` 哈希不变，还原验证不受影响。
**未删除任何既往 evidence**：`docs/reviews/evidence/fc-return-audit-2026-10-09/` 43 项原样保留，
历史报告、`sources/`、旧迁移 0001–0029、ADR-0007（仍 Proposed）均未触碰。

## 8. 回退

回退本包只需还原两个文件到 HEAD 哈希（见 §1 输入表）：
`git checkout HEAD -- crates/observability/src/lib.rs crates/observability/tests/structured_logging.rs`
（共享 dirty 工作树下须由主控串行决定，不在本包自行执行）。
无新增依赖、无新增 migration、无 schema / 协议 / 身份 / 权限变更，
因此回退不涉及数据或配置迁移，回退后 observability 即恢复 FC-04 修复前的 29 用例行为。

## 9. 未验项 / 需裁决

1. **变异 3 / 4 未被捕获**（等价变异体，见 §7）。若 Codex 认为资源上界需要「变异可检出」的强度，
   需要一个**性能断言**（如固定机器上的耗时上限）而非正确性断言——耗时断言在共享 CI 上不稳定，
   本包未擅自引入，需裁决。
2. **最长匹配脱敏的选择**：当同一 secret 有多种长度的合法拼写时，本包选最长匹配脱敏（脱敏边界下最安全，
   不会留下活的 secret 拼写）。这不改变既有 29 用例的任何断言，但属于语义选择，请 Codex 确认。
3. **`replace_bounded_matches` / `find_bounded_match` 签名由 `Fn` 改为 `FnMut`**，以便复用匹配缓冲区。
   纯内部可见性变更，无公共 API 变化。
4. **workspace 总数已实跑更新为 653 / 0 / 1 ignored**（§5.1），不再沿用 643。
   遗留未验项：首轮并行跑 workspace 时 `adapter-supervisor` 的 watchdog 用例抖动的**原始 stdout 未留存**
   （被第二次运行覆盖），已用 `workspace-total-attempt1-note.log` 记录复现事实并显式标注该缺失，
   未以第二次日志冒充首轮全绿。该只读 crate 的抖动本包不修。
5. **未验证**：Windows / 非 macOS 平台、并发多线程下的 `JsonSubscriber` 交错、本 crate 在真实
   adapter-supervisor / persistence-sqlite 调用链上的端到端日志复核。这些超出本包写集与职责。
6. **本包 evidence 自查已做但不等于完备性证明**：已扫全部 18 份工件确认无第二份空过；但
   "无空过"是本次扫描范围内的结论，不代表对所有历史 evidence 的普适保证。
7. ADR-0007 仍 Proposed；未开放通知未决语义、V2 新阶段或发布。

## 10. 写集合规

**已修改**（穷举，无越界）：
- `crates/observability/src/lib.rs`
- `crates/observability/tests/structured_logging.rs`

**新建**：
- `docs/coordination/minimax-audit-next/AF-01-RESULT.md`
- `docs/reviews/evidence/minimax-audit-next/AF-01/`（本包新证据，未覆盖任何既往 evidence）

其余全部只读，包括 `crates/adapter-supervisor/`、`crates/persistence-sqlite/`、`scripts/`、
根 `Cargo.toml` / `Cargo.lock`、`eslint.config.mjs`、`sources/`、旧迁移 0001–0029、
历史报告与 evidence、`docs/adr/0007-*`。

格式化只对本包两个文件执行 `rustfmt --edition 2024 <file>`，**未使用 `cargo fmt -p` 或 `--all` 写入**。
