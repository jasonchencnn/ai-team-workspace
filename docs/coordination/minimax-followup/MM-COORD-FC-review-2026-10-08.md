# MM-COORD-FC｜本轮（FC）复核记录与待裁决（2026-10-08）

本端对 Codex `MINIMAXCODE-FOLLOWUP-2026-10-08` 各包的**独立复核**记录。本文件不解除任何 BLOCKED，不构成 DONE 判定。

---

## 1. 已复核通过的包

| 包 | 本端独立实跑结果 |
|---|---|
| FC-01 | 三个 target **37 passed / 0 failed**，exit 0。`:424` 的 `MAX(version)==22` 与 `:253` 的 `==23` 两处**回滚断言逐字未动**；三处脆弱断言已改为 `WHERE version = N AND success = TRUE`。另核实其额外补的 `description == "core history retention"` 绑定确实存在 |
| FC-04 | `cargo test -p observability` **29 passed / 0 failed**，exit 0。全文件唯一 `to_ascii_lowercase` 位于 `is_sensitive_field` 归一化**字段名**，不作用于输入输出，无全串折叠。`Cargo.lock` mtime 为 `Oct 3 15:51`（早于本轮），`git diff` 的 284 行属**继承 dirty 状态**，本包未改 lockfile/manifest |
| FC-05 | lib target **64 passed / 0 failed**，integration `supervisor_fixture` **29 passed / 0 failed**。raw stdout 证据 `non-JSON=0 blank=0 harness-chatter=0`。FC-05 判 `SELF_CHECK_BLOCKED` 并自陈三项待签收（见 §3） |
| FC-06 | gate 单测 **25 passed / 0 failed**（原 22 全保留 + 新 3）；`verify-workflows.mjs` exit 0；dry-run 实测 **恰好 21** 条命令且显式声明 `stays 21`。本包未碰 `eslint.config.mjs`，历史字节未动 |

### 一次已排除的假失败
FC-05 的 `descendant_pipe_holders_cannot_block_shutdown_or_drop` 曾失败于 `:142 assert!(status.success())`（内层子进程非零退出，非 watchdog 超时）。本端在**本机重启后负载 5/15 分钟均值 44/39 的条件下**单独复跑**通过**，复跑全量亦全绿，判定为负载导致的超时抖动，**非行为缺陷**。FC-05 此前放宽相关 deadline 的处理与此一致。

---

## 2. 需 Codex 裁决：FC-06 的风险分析口径不准确（不需改代码，需改结论表述）

FC-06 把 `verify-workflows.mjs` 从纯静态改为**执行从 `ci.yml` 抽取的 shell 片段**（`spawnSync("bash", [guardPath], { cwd: <临时目录>, env: {...process.env, PATH: <sandbox>:...} })`），并在 RESULT 中说明其安全性前提是「片段来自仓库内**受评审的** ci.yml」。

**该前提对 PR 场景不成立。** 本端已核实：

- `.github/workflows/ci.yml:6` 触发条件含 `pull_request`；
- `.github/workflows/ci.yml:56` 的 `workflow-validation` job 执行 `node scripts/ci/verify-workflows.mjs`；
- GitHub 的 `pull_request` 事件使用**合并后的 workflow 定义**，即 **PR 自身修改后的 `ci.yml`**。

因此一个 fork PR 只要把任意 shell 写入 guard 步骤，`workflow-validation` job 即会执行它。FC-06 所写「受评审」只适用于主干/同仓分支，不适用于 fork PR。

**本端评估：代码可接受，结论表述需更正。** 理由：

1. **实际爆炸半径有限**：workflow 顶层 `permissions: contents: read`，且 `:113` 断言不向 job 传 `secrets.`；runner 为临时实例。
2. **不构成新的信任边界跨越**：同一次 workflow 的 `toolchain` / `test` 等 job 本就在执行 PR 的 `pnpm run test`、`cargo test` 等任意代码。`verify-workflows` 额外执行一个受限 shell 片段，边际增量有限。
3. **已做的收敛是正向的**：抽取逻辑显式拒绝含 `${{ }}` 的脚本（`verify-workflows.mjs:231-233`），因此无法依赖 workflow 表达式展开取 secret；执行目录为新建临时目录，`git` 为沙箱内桩件，场景固定为 4 种。
4. **与原缺陷相比是净改善**：原实现仅匹配「这行被打印过」，无法区分「打印但不判定」与「真正 fail-closed」；审计点名的原始 R06 workflow 在修复前被**接受**，修复后被拒。

**建议处置**：保留实现；请 FC-06 或 Codex 在结论中把前提改写为「片段取自本次运行所使用的 workflow 定义，PR 场景下即 PR 自带版本；本 job 权限为只读且不注入 secret，风险可接受但不等同于『受评审』」。

**若 Codex 认为不可接受**，退路是降回静态 token 断言——代价是重新打开「只匹配打印过这一行」这一原始缺陷。**本端不建议退回去。**

---

## 3. FC-05 自陈的三项待签收（尚未被 Codex 裁定）

1. **4 条测试保留在 lib target**，理由是 `send_protocol_line` 为 crate 私有，integration target 不可见；迁移它们必须放宽**生产可见性**，超出本包授权。本端认同该判断。
2. **4 条测试的子进程**从 libtest fixture 换成 `/bin/cat` / `/bin/sleep`，并有 1 处断言由「exit success」改为「`GracefulShutdown` + exit success」。属语义判断，需签收。
3. **测试时间常量放宽**：成功路径 deadline `1s → 5s`；后代用例 grace `100ms → 1000ms`、elapsed 上界 `800ms → 3000ms`、watchdog `1200ms → 6000ms`，同时把后代管道持有 `3s → 10s`。
   本端复核：**期望超时的用例（30ms/50ms）刻意保持原值**（它们断言 deadline 被强制执行）；后代用例的证明裕度因上界远小于 10s 持有时间而**变大**，命题「shutdown 绝不等后代」未被削弱。
4. **`src/bin/fixture_adapter.rs` 是新增可执行 target**，改变 `cargo build --workspace` 的产物集合。该 bin 未注册、未包装、未声明为获准生产 Adapter，文件头 doc comment 写明不得晋升。

---

## 4. 本端在本轮的错误登记（延续上一轮的自查纪律）

| 轮次 | 错误 | 处置 |
|---|---|---|
| 第一轮 | 仅扫 `MAX(version)`，漏掉 `COUNT(*) WHERE success=TRUE` 形态，误报「只有两处失败」 | 已在 `MM-COORD-review-gate` §3b 认领 |
| 第一轮 | 未自行计数即采信 MM-R02 转述，发布「审计 artifacts 不可复现」的不实指控 | 已撤回，原文折叠留痕 |
| 第一轮 | 建议 R05「新增 `src/bin/` 即可」，未验证 `CARGO_BIN_EXE_*` 在 lib unit test 中的可用性 | 已被 Codex 独立证伪，本轮 FC-05 简报中已前置该事实 |
| 第二轮 | 对 FC-06 的 `pullback_request` 触发场景未先行核实即接受其风险前提 | 已在本文件 §2 更正 |
| 第二轮 | FC-03 基线日志仍在追加时读取，得出 189/0，险些把「与 FC-02 的 227/0 差 38」当成回归上报 | 等日志写完再统计；终值 **227 passed / 0 failed**，31 个 target / 31 条 result，与 FC-02 一致，无回归 |
| 第二轮 | 独立复现 FC-03 矩阵时用 glob `00[0-2][0-8]*.sql`，漏掉尾数 9 的 `0009`、`0019`，得到 `no such table: approvals` 的假失败 | 改用 `ls migrations/*.sql | sort \| head -28`；该报错是探针的错，不是迁移的错 |
| 第二轮 | 对子代理「变异台账已重建」过早采信，只读摘要未跑保留副本的基线，漏掉 M4 未还原 | 复跑基线得 16/4 并定位到副本 `0027` 哈希不符；此后所有变异结论一律本端自行复跑 |

---

## 5. FC-03 复核：核心结论独立坐实，变异台账整体退回

**核心结论（本端独立重建 v28/v29 库复现，不依赖子代理探针）**

- `ls migrations/*.sql | sort | head -28` 建 v28，套 0029 建 v29：v28 = **232** 触发器，v29 = **233**，恰好 +1。
- v28 `ACCEPTED=29 REFUSED=27` → v29 `ACCEPTED=17 REFUSED=39`，**恰好关闭 12 格**，全部落在终态来源行。
- 剩余 17 格与 `docs/specs/STATE_MACHINES.md` §3 冻结合法边**逐条相等**（`QUEUED→{STARTING,CANCELLED}`、`STARTING→{RUNNING,FAILED,TIMED_OUT,CANCELLED,UNKNOWN}`、`RUNNING→{SUCCEEDED,FAILED,TIMED_OUT,CANCELLED,UNKNOWN}`、`UNKNOWN→{RUNNING,SUCCEEDED,FAILED,TIMED_OUT,CANCELLED}`）。
- 同态写回（self）保留，`NEW.status IS NOT OLD.status` 的写法与 `0027` 一致。

**本端发现并已回传的两处缺陷（子代理已修其一）**

1. `0029` 头注释矩阵的 `QUEUED` 行画错：写成全 `refuse`，把 `QUEUED→STARTING`、`QUEUED→CANCELLED` 两条**冻结合法边**标成拒绝，导致该表自身求和 = 27，却声明 `ACCEPTED=29`，与其下方「17 frozen legal edges」及 11/16/12 分解全部对不上。已改为 `self ACCEPT refuse refuse refuse refuse ACCEPT refuse`，现 8 行求和 = 29。**测试抓不到它**——`the_status_matrix_matches_the_frozen_edge_table` 用的是 spec 派生表而非注释，所以全绿也掩盖不了。

**变异台账：整体退回重做（本端复现，非推断）**

`main/mutation.log` 三条变异均记 `EXIT=101`，但每条 `--- failing tests ---` 与 `--- summary ---` **两段全空**。本端在 `mutation/crate/` 原样复跑得到：

```
error: failed to load manifest for dependency `teamspace-domain`
Caused by: failed to read `.../FC-03/mutation/domain/Cargo.toml`
Caused by: No such file or directory (os error 2)
EXIT=101
```

`mutation/crate/` 只有 `Cargo.toml` + `.gitkeep`，无 `src/`、无 `tests/`、无 `migrations/`。**EXIT=101 发生在 manifest 加载阶段，早于编译与测试执行，不能证明测试抓到了变异**——即 MR-01 判定的「因错误原因而红」。另三个缺陷：

- `copy-source-after.sha256` **缺失**，尽管 `mutation/README.md` 明写 before/after 两个哈希都在该目录 → **还原验证无证据**，而还原是本包硬要求。
- `persistence-sqlite-src-copy` 是**过期副本**（时间 20:33，早于 0029 定稿），其 `src/lib.rs` 中 `RUN_TERMINAL_INTEGRITY_SQL` 出现 **0** 次。
- 两个副本的 `Cargo.toml` 均写 `teamspace-domain = { path = "../domain" }`，而 `mutation/domain/` 不存在。

**保留有效的部分**：`probe/matrix_probe.sh` 及 `probe-v26/v28/v29-matrix.log` 是纯 sqlite3 的 schema 级探针，不受 cargo 影响，**有效且是本包的主证据**。

---

## 6. FC-03 变异台账：两次退回，最终本端重建并自行复跑

**第一次（作废）**：`mutation/crate/` 只有 `Cargo.toml` + `.gitkeep`，其
`teamspace-domain = { path = "../domain" }` 指向不存在目录。本端复现得到
`failed to load manifest for dependency teamspace-domain` / `EXIT=101`——
**发生在 manifest 加载阶段，早于编译与任何测试执行**，不能证明测试抓到了变异。

**第二次（子代理重建后仍有缺陷）**：变异改为在 `mutation/workspace/` 自建工作区运行，
台账数字变为真实（BASELINE 20/0，M1 14/6、M2 18/2、M3 14/6、M4 16/4），补丁为真实
unified diff。但本端复跑保留副本的基线时得到 **16 passed / 4 failed**，签名与 M4
（中和 `0027`）完全吻合：

- 副本 `0027_run_transition_integrity.sql` 哈希 `77cd01be…`，生产 `58a689fc…`；
- 第 145 行仍是 `SELECT RAISE(IGNORE);`；
- 台账里那段「restore verification」哈希打自一个**随后被删除的 `/var/folders/.../tmp`
  工作区**，而真正留在 evidence 目录的副本处于被变异状态。

即：**声称已还原的证据副本本身没还原**，且原始日志缺失（只有 grep 过滤后的摘要）、
cargo 命令漏 `--locked`、`copy-source-after.sha256` 被 README 提及却从未创建。

**最终（由本端重建并自行复跑）**：`mutation/run_mutations.py` 驱动，
`--locked --offline`，原始未过滤输出落 `mutation/raw-logs/`，每条变异附 unified diff，
还原后重新哈希且不一致即非零退出。台账见 `main/mutation-ledger.md`。每条失败都带
原始断言行，例如 M1 的
`SUCCEEDED -> FAILED (a terminal Run has no outgoing edge): the statement was accepted`、
M2 的 `writing the same terminal status back was refused (code: 1811) Run terminal status violation`——
证明 RED 是断言失败，不是编译错误。作废的第一次运行保留在 `main/mutation.log` 留痕。

**交付前另做**：对本包改动的两个测试文件单独 `rustfmt`（未用 `cargo fmt -p/--all`，
写集外文件 mtime 未变）；格式化改变了字节，故**全量终值与变异台账均在格式化之后重新
取得并重跑**，未沿用旧日志。改名
`this_package_owns_exactly_version_27` → `this_package_pins_exactly_versions_27_and_29`
（函数体同时钉 27 与 29，原名已不准确）。

**FC-03 终值**：全量 **233 passed / 0 failed**，31 target / 31 result，exit 0
（基线 227，+6 = `run_transition_integrity` 15→20、`run_lease_consistency_forward_migration`
19→20、`core_history_retention` 19→19 重构未增）。交付物 `docs/coordination/minimax-followup/FC-03-RESULT.md`。

---

## 7. 跨轮延续的系统性发现

留存守卫缺失在本轮被 Codex 完整普查扩展为**六对象**：`references`、`delivery_attempts`、以及无子记录时的 `teams` / `roles` / `tasks` / `bindings`。本轮第一版 `MM-COORD-review-gate` §B-4 曾提出「留存守卫是逐表零散补齐，建议改为一次性普查」——**该判断已被 Codex 的普查证实并扩展**，FC-02 正在补齐其中一部分。

Codex 复审明确：这些缺口**不要求 Minimax 未授权改更多表**，但列为**准入阻断**，待前向守卫与合法业务对照收口。本端据此严守写集。
