# FC-05｜fixture 传输层整改 — 执行回传

- 关联节点：DEV-03-03（关联 03-02/04/05）
- 审计项：MR-04 第二、三段（`docs/reviews/MINIMAX-RETURN-AUDIT-2026-10-08.md` §3）
- 上游：MM-R05 遗留的 11 条暂红（`docs/coordination/minimax-return/MM-R05-RESULT.md` §9/§10）
- 执行环境：Minimaxcode（MiniMax-M3.1-Flash-Preview），Worker 会话 `mvs_89c2c957e6b34545bdb07f1f04b09beb`
- 工具链：cargo/rustc 1.98.0、node v24.21.0；全部 `--locked --offline`；无联网、无新增依赖、未改 manifest/lockfile、未提交/推送/部署
- 判定：**`SELF_CHECK_BLOCKED`** — 本包范围内全部完成且验收命令全绿（exit 0）；但**自行扩大了写集之外的一处判断**、**改了 4 条测试的子进程来源**、**放宽了若干测试时间常量**，需总控签收（见 §12 待决项）

## 1. SHA

| 项 | 值 |
|---|---|
| 起始 HEAD | `1a14b8795e76fc373bef326eb5df96f9ac173f71` |
| 分支 | `codex/wave-2-ci-validation`（继承 dirty worktree，全程未 reset/checkout/clean/stash） |
| `src/lib.rs`（输出） | `008b5c477b638a8c5fe7ab63e779193beae615e851734baad28694a2eb1aa5a3` |
| `src/bin/fixture_adapter.rs`（新增） | `64ef04b24ae03fa85c140c5985f64989cfc44a20c61e4c545c66e95f4bc6ea11` |
| `tests/supervisor_fixture.rs`（新增） | `3c06e54d618790b0d5bd133bb29c220dcf623ea4238c6f1c2b76e769afa96596` |
| `adapters/mock/index.mjs`（**未改动**） | `cc75198c1f3f8a4485bfba96a03329e58bdaea7f082cc4a21ea8d29d2ca51ceb`（与 MM-R05 输出一致） |
| `src/protocol.rs`（**未改动**） | `66c3633fbe791cea8ab0d94474a771965589b22ff0a19d607c6d5d8879247e9c`（与 MM-R05 输出一致） |
| `Cargo.toml` / `Cargo.lock` | 未改动（bin 与 integration test 由 Cargo 自动发现） |

## 2. 根因确认：Codex 的复审结论成立

MM-R05 提案的「新增 `src/bin/fixture_adapter.rs`」**确实不充分**，但缺口不在 bin 本身，而在**测试 target 的选择**。

实测（`01-RED-preexisting-test.log`，先 `cargo clean -p adapter-supervisor`）：`cargo test -p adapter-supervisor` = **79 passed / 11 failed**，11 条全部是 `handshake_*` / `health_check_*` / `run_normal_execution_flow_via_fixture`，失败体统一为 `ProtocolViolation … "Malformed JSON: unexpected end of JSON input"`，即对 libtest 注入的空行求值。

独立无依赖 offline crate 实测 `CARGO_BIN_EXE_*` 的可见性：

| target | `CARGO_BIN_EXE_<name>` |
|---|---|
| lib unit test | `Err(NotPresent)` |
| integration test | 有值 |

因此 bin target 只解决「stdout 干净」，不解决「谁能定位它」。解法是 **bin + integration target 两者同时落地**。

## 3. 交付物

### 新增 `crates/adapter-supervisor/src/bin/fixture_adapter.rs`

Rust 自动发现 `src/bin/*.rs`，**未改 `Cargo.toml`、未碰 lockfile**。要点：

- **stdout 只写协议 JSON Lines**，全部诊断走 stderr。
- mode 是**单个位置参数**，不是环境变量：不经过 shell，无参数拼接歧义。
- **缺 mode / 未知 mode / 前置条件不成立一律 exit 64 + stderr 说明**，stdout 保持 0 字节。实测见 `11-raw-stdout-evidence.log` 的两个负例。
- 原 libtest fixture 的所有 mode 都实现了：`environment`、`echo_line`、`literal_argument`、`sleep`、`ignore_eof`、`wait_for_eof`、`write_marker_after_delay`、`large_stderr`、`spawn_pipe_holder_then_exit`、`spawn_marker_descendant_then_exit`、`hold_pipes`、`oversized_then_valid`、`handshake_ok`、`handshake_incompatible_version`、`handshake_illegal_semver`、`handshake_missing_fields`、`handshake_not_json_rpc`、`handshake_exit_early`、`handshake_rpc_error`、`health_check_ok`、`health_check_degraded`、`health_check_missing_fields`、`health_check_exit_early`、`run_normal_ok`。
- `sleep`/`ignore_eof` 的**模拟崩溃状态码 42 / 43 原样保留**，这些是对外可观测的 Adapter 行为，不是 fixture 实现细节。
- 子进程派生（`spawn_pipe_holder_then_exit` 等）改用**本进程 `current_exe()`**，不再依赖测试二进制路径。
- 该 bin **仅供测试使用**：未注册、未包装、未在任何 manifest/Adapter 清单中声明为获准生产 Adapter；文件头 doc comment 明确写了不得晋升。

### 新增 `crates/adapter-supervisor/tests/supervisor_fixture.rs`

- 用 `env!("CARGO_BIN_EXE_fixture_adapter")` 定位 bin，编译期由 Cargo 注入。**无运行时 `cargo run/build`、无 shell 参数拼接、无 target 路径猜测、不依赖任何预先手工构建**（§5 用 `cargo clean -p` 后的完整跑证明）。
- 除 25 条迁移用例外，新增 4 条针对传输层本身的守卫：`fixture_stdout_contains_only_protocol_json_lines`、`fixture_rejects_a_missing_or_unknown_mode_loudly`、`fixture_raw_stdout_evidence_is_reproducible`、`fixture_echo_line_mode_returns_the_request_verbatim`。

## 4. 测试数量变化（逐项解释）

| target | 之前 | 之后 |
|---|---|---|
| lib unit | 90 | **64** |
| integration | 0 | **29**（25 迁移 + 4 新增守卫） |
| 合计 | 90 | **93** |

lib 的 90 → 64：其中 **25 条按测试名一对一迁到 integration target**，**1 条删除**（`fixture_adapter` 本身——它就是那个基于 libtest 的 fixture 实现，已被 bin 取代），**4 条留在原处**。合计 25+1+4 = 30。

**完整映射清单见 `docs/reviews/evidence/minimax-followup/FC-05/12-TEST-MAPPING.md`**，逐条写明每个原测试迁到哪个用例、为什么。

**没有任何测试被 `#[ignore]`、没有预期值被放宽、没有断言被删除。**

### 为什么 4 条没有迁走（需总控确认）

`protocol_lines_round_trip_as_literal_json_lines`、`protocol_input_rejects_cr_lf_and_messages_over_the_frozen_limit`、`blocked_stdin_write_obeys_deadline_and_still_allows_reap`（含 `_watchdog_case`）、`cancelled_stdin_write_stops_the_unsafe_protocol_stream` 调用 **`pub(crate) send_protocol_line`**。该方法按设计是 crate 私有（注释明确写：调用方不得手写 Run JSON-RPC 消息），integration target 以外部消费者身份链接 crate，**看不到它**。让它们迁走就必须放宽生产可见性——超出本包写集与授权。

它们同时**不再用 libtest 二进制充当 Adapter**：改为监督一个普通 POSIX 子进程，字面参数向量、无 shell、无 PATH 查找：

- `/bin/cat` ← 原 `echo_line` / `wait_for_eof`
- `/bin/sleep` ← 原 `sleep` / `ignore_eof`

`posix_child()` 会 assert 子进程存在，**缺子进程直接失败**，不会假通过。唯一的行为改动：echo 用例原本靠 fixture 自己退出，现在靠 `shutdown()` 关闭 stdin 让 `cat` 退出，断言从「exit success」改为「`GracefulShutdown` + exit success」，语义等价且更贴近真实 EOF 语义。

## 5. 验收命令与结果

全部在 `cargo clean -p adapter-supervisor` 之后执行，证明不依赖历史 build 产物。

| 命令 | exit | 日志 |
|---|---|---|
| `cargo clean -p adapter-supervisor` | 0 | `07-final-clean.log` |
| `cargo test -p adapter-supervisor --locked --offline --no-fail-fast` | **0**（lib 64 / integration 29 / bin 0 / doc 0） | `08-final-acceptance.log` |
| `cargo clippy -p adapter-supervisor --all-targets --locked --offline -- -D warnings` | **0** | `08-final-acceptance.log` |
| `RUSTDOCFLAGS="-D warnings" cargo doc -p adapter-supervisor --no-deps --locked --offline` | **0** | `08-final-acceptance.log` |
| `rustfmt --edition 2024 --check`（仅本包 3 个文件） | **0** | `08-final-acceptance.log` |
| 加压重复跑（8-10 个 CPU burner，load≈29） | integration target 29/29 稳定 | `09-stability-under-load.log` |

**未执行 `cargo fmt --all`**（会重排本包无权修改的其它 crate）。只对本包三个文件调用 `rustfmt`。

### 11 条失败的实测结果

**全部转绿，无一条被跳过或放宽**：`handshake_success_negotiates_capabilities_and_auth_state`、`handshake_incompatible_version…`、`handshake_illegal_semver…`、`handshake_rpc_error_is_properly_mapped`、`handshake_timeout_is_diagnosed_as_unavailable`、`handshake_unexpected_process_exit…`、`health_check_success…`、`health_check_degraded…`、`health_check_process_exit…`、`health_check_timeout…`、`run_normal_execution_flow_via_fixture`。

## 6. raw stdout 证据

`11-raw-stdout-evidence.log` 用 `cat -v -e -t` 逐字节渲染真实二进制 stdout：

| mode | stdout 行数 | 非 JSON 行 | 空行 | harness 噪声行 |
|---|---|---|---|---|
| `handshake_ok` | 1 | 0 | 0 | 0 |
| `health_check_ok` | 1 | 0 | 0 | 0 |
| `run_normal_ok` | 6 | 0 | 0 | 0 |

两个负例（缺 mode / 未知 mode）：**exit 64、stdout 0 字节、stderr 给出原因**。

## 7. 六处静默假绿 → 明确失败

`src/lib.rs` 中缺 Node/Mock 就 `return` 的路径全部改为硬失败：

- `node_and_mock_script()` 改为返回 tuple 而非 `Option`，缺 node 或缺 `adapters/mock/index.mjs` 直接 `panic!`，消息写明「these cases cannot be skipped, they must run」。
- 连带清掉 `noisy_mock_command()` 的 `Option`、以及 `node_mock_*` 共 17 处 `None => return`、`stdout_noise_*` 五处 `let Some(..) else { eprintln!("skipping…"); return }`、`node_mock_adapter_end_to_end_execution` 的 `eprintln!("skipping…"); return`。
- 缺 mode、缺可执行文件、子进程 panic、进程提前退出都已是硬失败（bin exit 64 + stderr；integration 的 `fixture_rejects_a_missing_or_unknown_mode_loudly` 断言）。

**验证（`06-DEPENDENCY-ABSENCE-node-missing.log`）**：在**独立源码副本**里模拟「Node 不存在」（即旧代码走 silent `return` 的那个分支），结果是 **42 passed / 22 failed、EXIT=101**，每个失败都带 `node executable not found … cannot be skipped`。修复前这里会是全绿且什么都没断言——这正是 R05 §6 记录的「装置故障伪装成通过」陷阱，现已关闭。

## 8. 变异验证（独立源码副本）

两处变异都在 `/tmp` 的 rsync 副本里做，**共享 `src/lib.rs` / `src/protocol.rs` 未被临时改动**（§1 的 SHA 即还原后校验值）。

| 变异 | 目的 | 结果 |
|---|---|---|
| A：fixture 在 stdout 打 `running 1 test` | 证明新守卫抓得住 libtest 式污染 | integration **19 passed / 10 failed**，EXIT=101（`04-MUTATION-A-fixture-stdout-chatter.log`） |
| B：生产 `read_response_line` 恢复跳过非 JSON 行 | 证明 R05 的严格性没被本包放宽 | `stdout_noise_is_rejected_at_the_initialize_stage` + `handshake_non_json_rpc_fails_with_protocol_violation` 失败，EXIT=101（`05-MUTATION-B-parser-skip-bypass.log`） |

变异 B 在 `protocol.rs` 内，本包对该文件只读，故只在副本中改动并如实登记。

## 9. 行为断言保留情况

正常 Mock、重复/乱序、取消拒绝、EOF/强制回收、ADR-0006 获准后代进程等断言**全部保留**，位置见映射清单。其中：

- 正常 Mock → `node_mock_adapter_end_to_end_execution`、`node_mock_without_noise_still_completes_the_handshake_and_a_run`（lib，原位）
- 重复/乱序 → `run_validator_accepts_identical_duplicate_notification_idempotently`、`…_rejects_duplicate_sequence_with_conflicting_payload`、`…_rejects_out_of_order_sequence`、`…_rejects_skipped_sequence`（lib，原位）
- 取消拒绝 → `node_mock_adapter_normal_cancel`、`node_mock_adapter_fault_reject_cancel(_timeout)`、`node_mock_rejects_run_operations_before_trusted_negotiation_allows_them`
- EOF/强制回收 → `closing_stdin_allows_an_eof_aware_adapter_to_shut_down_gracefully`、`shutdown_escalates_to_termination_after_the_grace_period`
- ADR-0006 获准后代进程 → `exited_adapter_cannot_leave_a_same_group_descendant_running`（已迁至 integration）

## 10. 已知遗留问题（非本包引入，未擅自修）

`tests::node_mock_adapter_fault_crash` 在**持续 CPU 高负载**下（实测 load≈29）偶发失败于 `lib.rs:2113 assertion failed: exit_status.is_some()`。

- **不是 FC-05 引入**：与既有快照逐行对比，该用例只有 fixture 查找的前导几行不同（`match … None => return` → 直接调用），断言体**逐字节相同**（见 `10-PREEXISTING-FLAKE-node-mock-fault-crash.md`）。
- **不在本包写集**：退出状态的读取在 `src/protocol.rs`，本包对该文件只读。修它需要改生产读取逻辑，或放松一个先于本包存在的断言——两者都超出授权。
- **实测分布**：重负载（load≈29）下单跑该用例 12/12 通过、整套 6 次里失败 1 次；中等负载（load≈13，无人工加压）整套 8/8 通过。差异来自整套并发争用，与 fixture 传输层无关。
- **诚实登记**：本包自己的 integration target 曾在一次 load 13-15 的尖峰中抖过一次（当时成功路径 deadline 仍是 1s、fixture 后代管道持有仅 3s）。已在包内按 §12 第 3 项修掉，此后该 target 在每一次跑里都是 29/29，包括 5 次刻意重负载跑。
- **对本包门禁的影响**：FC-05 自身的四条验收命令在无人工加压下连续 5 次全绿；加压跑时新建的 integration target 每次 29/29 绿（`09-stability-under-load.log`）。此问题请总控另行路由。

## 11. 未改动 / 未执行

- `src/protocol.rs`（五入口生产解析）：**SHA 未变**。
- `adapters/mock/index.mjs`（Node mock）：**未改动**。
- `Cargo.toml` / `Cargo.lock`：**未改动**（bin 与 integration test 由 Cargo 自动发现）。
- 迁移、其他 crate、CI 配置：未触碰。
- 未提交、未推送、未部署、未访问外部服务、未新增依赖、未联网。
- 未跑 workspace 全量门禁（按任务包只跑与本包匹配的定向检查）。

## 12. 待总控签收的事项（不自判 DONE）

1. **4 条测试留在 lib target**，理由是 `send_protocol_line` 为 crate 私有（§4）。若总控认为必须迁到 integration，唯一途径是放宽该方法的生产可见性——那超出本包授权，需另行签发。
2. **4 条测试的子进程从「libtest fixture」换成 `/bin/cat` / `/bin/sleep`**，并有 1 处断言由「exit success」改为「`GracefulShutdown` + exit success」。这是本包为遵守「不得用 libtest 当 Adapter」而做的判断，请复核是否接受。
3. **测试时间常量放宽**（两处，均为消除负载导致的假失败，未削弱任何被断言的性质）：
   - integration target 的**成功路径** deadline 由 `1s` 统一改为 `ADAPTER_DEADLINE = 5s`。这些用例断言「Adapter 会响应/会退出」，放宽的是**度量机器负载**的那部分，不是行为。**期望超时**的用例（30ms / 50ms）刻意保持原值，因为它们断言的是「deadline 被强制执行」。
   - `descendant_pipe_holders_…`：grace 100ms→1000ms、elapsed 上界 800ms→3000ms、watchdog 1200ms→6000ms，**同时把 fixture 的后代管道持有从 3s 提到 10s**。原来的 1 秒 deadline 在本机 load≈9 时实测会产生假失败（`TimedOut` / `ForcedShutdown`）。**语义未放宽**：所有上界仍远小于 10 秒持有时间，「shutdown 绝不等后代」这一原命题的证明裕度反而变大了。
4. **`src/bin/fixture_adapter.rs` 是新增的可执行 target**。虽未注册为生产 Adapter，但它的存在本身改变了 `cargo build --workspace` 的产物集合，请确认可接受。

## 13. 回退方式

回退本包 = 删除 `src/bin/fixture_adapter.rs` 与 `tests/supervisor_fixture.rs`，并把 `src/lib.rs` 恢复到 MM-R05 输出版本（§1 SHA 之外可用 `git diff` 对照 MM-R05 记录）。无数据迁移、无 schema 变化、无残留状态。**不建议回退**：回退等于把 11 条测试放回必红状态。