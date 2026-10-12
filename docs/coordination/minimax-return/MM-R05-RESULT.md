# MM-R05｜恢复严格 JSON-RPC stdout 边界 — 执行回传

- 关联节点：DEV-03-03（关联 03-02/04/05）
- 审计项：DA-05（P2 协议风险），`docs/reviews/DEEP-AUDIT-2026-10-08.md` §3
- 执行环境：Minimaxcode（MiniMax-M3.1-Flash-Preview），Worker 会话 `mvs_c3956ebfaffa45eaa1bc662943905600`
- 工具链：cargo 1.98.0 / rustc 1.98.0 / node v24.21.0，全部 `--locked --offline`，无联网、无新增依赖、无新增费用
- 判定：**`SELF_CHECK_BLOCKED`**（生产修复完成并验证；11 个既有测试因 fixture 传输层限制暂红，需签发写集扩展）

## 1. 输入与输出 SHA

| 项 | 值 |
|---|---|
| 起始 HEAD | `1a14b8795e76fc373bef326eb5df96f9ac173f71` |
| 分支 | `codex/wave-2-ci-validation`（继承未提交工作树，全程未 reset/checkout/clean/stash） |
| 输出 `protocol.rs` | `66c3633fbe791cea8ab0d94474a771965589b22ff0a19d607c6d5d8879247e9c` |
| 输出 `adapters/mock/index.mjs` | `cc75198c1f3f8a4485bfba96a03329e58bdaea7f082cc4a21ea8d29d2ca51ceb` |

审计起点输入 SHA `8845a053…`（299 文件）为历史口径；本包实际输入为上表 HEAD + 继承工作树。

## 2. 被修复的五处读入口

`crates/adapter-supervisor/src/protocol.rs`。五处原本都先 `trim_ascii()`，然后**跳过**空行与以 `running ` / `test ` 开头的行（`continue` 继续往下读），使这些行根本不进入 `JsonValue::parse`：

| # | 入口 | 原位置（审计口径） | 修复后位置 | 处理方式 |
|---|---|---|---|---|
| 1 | `read_response_line`（initialize） | 1816-1822 | 1807 | 删除跳过块；`loop` 因不再有 `continue` 变为死循环，按 clippy `never_loop` 去除，改为单次 `match` |
| 2 | `read_health_response_line`（health.check） | 1877-1883 | 1853 | 同上 |
| 3 | `read_run_start_response_line`（run.start） | 1963-1969 | 1927 | 同上 |
| 4 | `read_run_cancel_response_line`（run.cancel） | 2051-2057 | 2003 | 同上 |
| 5 | `read_run_notification`（通知） | 2115-2121 | 2054 | 删除跳过块；**保留 `loop`**，因为其唯一 `continue` 是 `PROTOCOL §7` 合法幂等去重（`validator.last_was_duplicate()`） |

每处保留既有 UTF-8 / 1 MiB / EOF / 超时分支不变；非 JSON 行交由既有 `parse_*` / `validate_line` 判定，落到**既有** `ADAPTER_PROTOCOL_VIOLATION`（`-32099`，`ERROR_CATALOG §7`，retryable=否）。**未新造任何错误码、状态、权限或 wire 字段**；stderr 仍是诊断通道（`SupervisorConfig::stderr_capture_limit_bytes` 与 `ProcessExited.stderr` 未改）。

## 3. 逐文件修改

### `crates/adapter-supervisor/src/protocol.rs`（生产修复）
删除五处 `running `/`test `/空白跳过；四处响应读取器去掉因此失效的 `loop`（保留原有 `remaining` 超时语义与全部错误分支）。无其他逻辑改动。

### `adapters/mock/index.mjs`（fixture 噪声注入，**必要性论证**）
总控要求说明必要性，不为凑改动量而改。结论：**必要，且不改变默认行为**。

- 未改动的 Mock 原本只有两处 stdout 写入（`JSON.stringify(obj) + '\n'`），是干净的——总控此判断正确，我未发现其中有既存缺陷。
- 但修复后要**证明**违规行被拒绝，必须能主动注入这四类噪声；否则只能靠 libtest fixture（见 §5 不可行）。因此新增**纯 opt-in** 故障注入：`MOCK_STDOUT_NOISE`（`|` 分隔原始行）与 `MOCK_STDOUT_NOISE_STAGE`（限定阶段），在 `initialize` / `health.check` / `run.start` / `run.cancel` / `run notification` 五处响应前调用 `emitStdoutNoise()`。
- **默认路径零变化**：`MOCK_STDOUT_NOISE` 未设置时 `configuredNoiseLines()` 返回 `null`，`emitStdoutNoise()` 直接返回。实测未设置时 initialize 输出仍为 1 行合法 JSON（证据 §15）。
- 一个实测发现的真实 bug（已修）：最初用 `raw.length > 0` 判断，使**空字符串（空行用例）被当成未配置**，导致空行注入失效、5 个测试假红。改用 `undefined` 判定 sentinel 后修复。

### `crates/adapter-supervisor/src/lib.rs`（仅测试代码）
新增 6 个测试（`stdout_noise_*` × 5 阶段 + `node_mock_without_noise_*` 合法对照）与辅助函数/常量。**未修改任何既有测试的断言、预期值或 `#[ignore]` 标记。**

## 4. 命令、退出码与日志

全部日志在 `docs/reviews/evidence/minimax-return/MM-R05/`：

| 命令 | Exit | 日志 |
|---|---:|---|
| `cargo test -p adapter-supervisor --locked --offline` | **101** | `01-cargo-test-after-fix.log`（79 passed / 11 failed，见 §6） |
| `cargo build -p adapter-supervisor --locked --offline` | 0 | `02-cargo-build.log` |
| `cargo clippy -p adapter-supervisor --all-targets --locked --offline -- -D warnings` | 0 | `03-cargo-clippy.log` |
| `RUSTDOCFLAGS="-D warnings" cargo doc -p adapter-supervisor --no-deps --locked --offline` | 0 | `04-cargo-doc.log` |
| `cargo fmt -p adapter-supervisor -- --check` | 0 | `06-cargo-fmt-check.log`（仅格式化本包文件，未 `cargo fmt --all`） |
| `cargo test … -- node_mock`（合法对照，16 项） | 0 | `05-node-mock-legal-control.log` |
| repro malformed / control（修复后） | 101 / 0 | `07-repro-red-green.log` |
| libtest `--format` 穷举 | — | `08-fixture-stdout-noise-analysis.log` |
| `--nocapture` 证伪 | — | `09-nocapture-removal-disproof.log` |
| **修复前 RED**（还原五处跳过的独立构建） | 0 | `10-PRE-FIX-RED.log` |
| 新增 6 项噪声验收测试 | 0 | `11-stdout-noise-acceptance.log` |
| **变异检测**（新测试 vs 修复前解析器） | 101 | `12-MUTATION-PREFIX-DETECTED.log` |
| `pnpm run lint:ts` | 1 | `13-pnpm-lint-ts.log`（**非本包问题**，见 §7） |
| `pnpm run typecheck` | 0 | `14-pnpm-typecheck.log` |
| lint 归属说明 | — | `15-lint-scope-note.log` |

## 5. fixture 传输层：为何 libtest 不能承担（根因）

`fixture_command()`（`lib.rs`）以 `--exact tests::fixture_adapter --nocapture` 拉起**测试二进制自身**作为 Adapter 子进程。libtest 把 `running 1 test` / `test … ok` / `test result: …` 写到**真实 stdout**，因此该子进程的 stdout 天然含非 JSON 行。

穷举 stable `--format`（`08-…log`）：`pretty`、`terse` 均带框架行；`json`、`junit` 需 nightly（`-Z unstable-options`），本机为 stable 1.98.0。**没有任何 libtest flag 能让该子进程 stdout 只含 JSON Lines。**

另证伪了总控首轮建议的「去掉 4 处 `--nocapture`」（`09-…log`）：加与不加 `--nocapture` 的子进程 stdout **逐字节相同**（各 7 行，前置空行 + `running 1 test` + JSON + 尾部框架）。`--nocapture` 只影响测试体内 `print!`/`println!` 的捕获，不影响 harness 自身进度行。**该建议已撤回，我未据此改动任何文件。**

## 6. RED → GREEN 与合法对照

### 修复前（真红，`10-PRE-FIX-RED.log`）
在独立目录还原五处跳过后（`grep -c 'starts_with(b"running ")'` = 5；仓库内为 0）：

- malformed（非 JSON 前缀 + 合法响应）→ `initialize accepted protocol=0.1.0`，**exit 0**
- control（仅合法 JSON Lines）→ `initialize accepted protocol=0.1.0`，**exit 0**

两者完全一致 = 绕过成立。同一构建下 `handshake_success_*`、`health_check_success_*`、`run_normal_execution_flow_via_fixture`、`handshake_timeout_*` 均 ok——**证明这 11 个测试原本正是靠该绕过才通过**。

### 修复后（绿，`07-…log`）
- malformed → `ProtocolViolation { code: -32099, domain_code: "ADAPTER_PROTOCOL_VIOLATION", message: "Malformed JSON: unexpected character 'r' at byte 0" }`，**exit 101**
- control → `initialize accepted protocol=0.1.0`，**exit 0**

### 新增验收测试（`11-…log`，6 passed / 0 failed）
四类噪声（`running `、`test `、空行、普通非 JSON）× 五个阶段全部被拒为 `-32099 ADAPTER_PROTOCOL_VIOLATION`；且断言 initialize 被拒后 `negotiated_initialize().is_none()`（不会继续读后续合法响应当成功）。合法对照 `node_mock_without_noise_still_completes_the_handshake_and_a_run` 证明无噪声时握手与 health.check 仍成功。

### 变异检测（`12-…log`）
把新测试放到**修复前**解析器上运行 → **5 failed**，证明这些测试真的会红，不是恒真断言。

> 诚实登记（`ADVERSARIAL_REVIEW_PLAYBOOK §1/§2`）：该变异检测**前两次尝试是"装置故障伪装成通过"**——(1) scratch 缺 `adapters/mock/` 导致 `node_and_mock_script()` 返回 `None` 全部提前 return；(2) 修正布局后仍因 Supervisor `env_clear()` 清空 PATH、`find_node_executable()` 解析不到 node 而 skip（输出 `skipping: node binary not available`）。第三次补齐仓库目录布局且 node 在 PATH 上，才得到正确的 5 failed。装置故障态未计入任何结论。

## 7. 未运行项及理由

- `pnpm run lint:ts` → **exit 1**，唯一错误在 `docs/reviews/evidence/deep-audit-2026-10-08/replay_env_manifest.mjs:24`（`no-unsafe-finally`）。该文件是**既存未跟踪审计产物，不在本包写集内**，与本包改动无关（`npx eslint adapters/mock/index.mjs` → exit 0，见 `15-…log`）。未擅自修改。
- 完整 21 项 Mac Gate、覆盖率、在线/完整验收：**未运行**，按任务包仅跑与本包匹配的定向检查。
- 未提交、未推送、未部署、未访问外部服务、未新增依赖。

## 8. 兼容性

- wire 字段、JSON-RPC code 映射、`DB` 依赖方向：**均未改动**。
- 仅收紧 stdout 解析：此前被静默吞掉的行现在按既有 `ADAPTER_PROTOCOL_VIOLATION` 失败——这正是 `PROTOCOL §2` 与 DA-05 要求的行为。
- 对合规 Adapter（stdout 仅 JSON Lines）**无行为变化**：control 与 16 项 Node mock 场景全部通过。
- Node mock 默认输出不变（§3）。

## 9. 剩余风险与当前暂红的 11 个测试

**明确登记**：`cargo test -p adapter-supervisor` 当前 **79 passed / 11 failed**。失败项（`01-…log`）：

`handshake_illegal_semver_is_rejected_with_adapter_incompatible_code`、`handshake_incompatible_version_is_rejected_with_adapter_incompatible_code`、`handshake_rpc_error_is_properly_mapped`、`handshake_success_negotiates_capabilities_and_auth_state`、`handshake_timeout_is_diagnosed_as_unavailable`、`handshake_unexpected_process_exit_is_diagnosed_with_status_and_stderr`、`health_check_degraded_reports_diagnostic_codes`、`health_check_process_exit_diagnosed_with_status_and_stderr`、`health_check_success_reports_healthy_status_and_timestamp`、`health_check_timeout_diagnosed_as_timed_out`、`run_normal_execution_flow_via_fixture`。

- 失败体统一为 `ProtocolViolation … "Malformed JSON: unexpected end of JSON input"`，即**对 libtest 注入的空行求值**（`JsonValue::parse("")`）。
- **根因在 fixture 传输层，不在产品解析逻辑**：这 11 项全部依赖以测试二进制自身为子进程，其 stdout 必含 harness 框架行（§5）。
- 这些测试**未被删除、未加 `#[ignore]`、未改预期值、未放宽解析**——按任务包与总控要求，保持真红。
- 风险：若总控在修复 fixture 前合并本包，CI 该 crate 将红。反向风险（为保绿放宽解析）会真实回退 DA-05，因此不采用。

## 10. 提案：fixture 传输层（需签发写集扩展）

| 方案 | 写集 | 代价 / 评估 |
|---|---|---|
| **A（推荐）新增 `crates/adapter-supervisor/src/bin/fixture_adapter.rs`** | **新增 1 个源文件**（不在本包写集） | Rust 自动发现 `src/bin/*.rs`，**无需改 `Cargo.toml`、无需改 lockfile**，故不触碰"根 manifest/lockfile 由总控串行整合"。把 `fixture_adapter` 从 libtest 迁为独立 binary，stdout 只含协议行、诊断走 stderr，`fixture_command()` 改为指向该 binary。11 个测试可原样复用既有 mode 与断言。**推荐此项。** |
| B 改用 `adapters/mock/index.mjs` 作 fixture | 在本包写集内 | 可让 11 个测试复用 Node，但会给这批 Rust 单元测试引入 **node 运行时依赖**（本机已有 `find_node_executable` 先例，且缺失时静默 skip——正是 §6 变异检测踩过的"假通过"陷阱）。覆盖面与确定性下降，**不建议**。 |
| C 去掉 `--nocapture` | — | **已证伪**（`09-…log`），stdout 逐字节相同。不采纳。 |

**请总控签发**：方案 A 的写集扩展（新增 `crates/adapter-supervisor/src/bin/fixture_adapter.rs`），或指定方案 B。获授权前不自行实施。

## 11. 回退方式

回退本包 = 恢复 `protocol.rs` 五处跳过（`10-PRE-FIX-RED.log` 的构建即为等价形态，可直接对照）与 `adapters/mock/index.mjs` 的 `emitStdoutNoise` 注入、移除 `lib.rs` 新增 6 个测试。无数据迁移、无 schema 变化，回退无残留状态。**不建议回退**：回退等于恢复 DA-05 绕过。

## 12. 判定

**`SELF_CHECK_BLOCKED`**

- 生产修复（`protocol.rs` 五处入口）已完成并通过真红→绿、合法对照、变异检测，build/clippy(-D warnings)/rustdoc(-D warnings)/fmt 全绿。
- 阻塞项仅为 fixture 传输层所需的**写集扩展**，属任务包明列的"先回传提案"情形；11 个既有测试在此之前保持真红，根因不在产品代码。
- 自验收不等于 DONE；DEV-03-03 状态与节点准入由 Codex 裁定，本包不解除任何 BLOCKED。
