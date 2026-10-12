# Wave 2 前置阻断修复与复验（2026-09-28）

> 状态：**部分修复，Wave 2 仍 BLOCKED**。本记录追加于[首轮 Mac 验收](WAVE-2-MAC-ACCEPTANCE.md)，不改写当时的失败证据。macOS 是当前生产与验证平台；Linux 不在本轮范围。

## 范围与分工

| 节点/缺口 | 执行与写集 | 结果 |
| --- | --- | --- |
| DEV-01-05，主索引中已删除的归档旧路径 | `gpt-5.6-terra` Subagent `gate_deleted_paths_fix`；仅 `scripts/ci/local-gates.mjs` 和 `.test.mjs` | 定向修复通过，正式 Gate 待重跑 |
| DEV-03-02，AD-01 进程生命周期 | 总控；仅 `crates/adapter-supervisor/src/lib.rs` | 同组后代清理通过；脱组后代仍阻断 |
| DEV-03-02，AD-02 `initialize` 超时 | `gpt-5.6-terra` Subagent `adapter_protocol_validation`；`protocol.rs` | 超时前终止并回收直接子进程，定向通过 |
| DEV-03-03 前置 AD-03 协议校验 | `gpt-5.6-terra` `adapter_protocol_validation`、`adapter_schema_fields` 先后独占 `protocol.rs`；`gpt-5.6-luna` `adapter_json_duplicate_keys` 独占 `json.rs`；总控串行整合 | 原三个反例与后续 JSON/response 反例均已修；未实现方法仍待后续节点 |
| 独立反例审核 | 未参与实现的 `gpt-5.6-terra` `preblocker_independent_review`；只读 | 三轮分别发现 detached 后代、JSON/response 边界、错误响应 ID 误关联；最终窄范围复验确认 ID 关联与 1 MiB 配置上限通过 |

## 已验证的修复

- 门禁输入清单仅跳过 `git ls-files --deleted` 明确标记、且仍在索引中的 tracked 路径。三个已归档旧路径写入 `skipped_tracked_deleted_paths`，该集合参与运行前后总哈希；归档新路径仍作为实际未跟踪输入读取。其它 `lstat`/读取失败继续阻断。真实工作区已能生成清单，无需临时 Git 索引。
- macOS Adapter 启动时进入独立进程组，直接进程退出、超时或 Drop 后发出组终止信号。同组后代的测试先失败、修复后通过；直接子进程的退出报告仍由 `wait_for_exit`/`shutdown` 回收。
- `initialize` 写入或响应超时，在返回 `TimedOut` 前强制终止并回收直接子进程；回归测试随后观察到 `AlreadyReaped`。其它 Run 请求仍按冻结协议的超时后幂等/对账语义处理，未把超时等同于“请求未执行”。
- 协议通知现在拒绝无效 UTC 三位毫秒日期、超出 0～100 的进度、非规范 UUIDv7、超 64 KiB 的 UTF-8 output、未知字段及错配/null payload。非法通知不会提前消耗 sequence。JSON 解析器在任意对象层级拒绝重复 key，包括 Unicode escape 后等价的 key；同时收紧 RFC 8259 whitespace、数字、控制字符与 Unicode surrogate 处理，拒绝非有限浮点表示。错误信息不回显不可信键名。四个已实现 success/error response 分支现检查根对象和 result/error 必需/封闭字段、类型、长度、数组及冻结自定义错误映射；当前请求只接受相同字符串 ID 的响应。

本机离线重放首轮协议反例得到 `non_uuid_accepted=false`、`over_64kib_content_accepted=false`、`duplicate_json_key_accepted=false`。该重放程序在这些结果之后立即退出，没有启动进程反例。

独立审核随后重放得到 `leading_zero_json_parses=false`、`nbsp_prefixed_json_parses=false`、`extra_root_field_initialize_accepted=false`。最后一轮还以真实 fake Adapter 往返确认：错误响应 `id:0` 被拒为协议违例，匹配字符串 ID 的标准 `-32600` 错误仍保留 `RpcError` 兼容；超出 1 MiB 的 supervisor 配置被拒绝。审核者未修改仓库，临时子进程均已回收。这些定向 PASS 不覆盖尚未实现的 Adapter 方法。

| 验证命令 | 结果 |
| --- | --- |
| `cargo test --workspace --locked --offline` | PASS；Adapter 78/78，workspace 其它测试通过 |
| `cargo clippy --workspace --all-targets --all-features --locked --offline -- -D warnings` | PASS |
| `cargo fmt --all -- --check`、`git diff --check` | PASS |
| `node --test scripts/ci/local-gates.test.mjs` | PASS，12/12 |
| `node scripts/ci/local-gates.mjs --platform macos --dry-run` | PASS，列出全部 21 项计划；不是 Gate PASS |

## 尚未关闭的阻断与决策

1. **AD-01：Mock 定向反例已通过，完整节点证据未齐。** 独立审核以不合作的 `setsid()` 反例实际重放，结果 `setsid_descendant_survived=true`，证明平台同组信号不能清理任意脱组进程。[ADR-0006](../adr/0006-macos-adapter-detached-descendant-policy-proposal.md)已接受受批准 Adapter 的契约责任边界。获准 Mock 的真实脱组子进程在 EOF、强制关闭、直系崩溃后退出的测试与独立复核见下方追加记录；完整 Mac Gate 与总控终审前 DEV-03-02 仍为 `BLOCKED`。
2. **AD-03：完整机器 Schema 等价性尚未证明。** 本轮修复覆盖首轮三个反例、RFC 8259/response 封闭性与错误 ID 关联反例；但没有通用 Schema evaluator，`run.status`、`run.reconcile`、`adapter.shutdown` 的实现与对应响应校验仍属于后续 DEV-03-03～05，全部 request DTO 的字段范围也未逐项镜像。DEV-03-03 不得因现有 78 项通过而提前 `DONE`。
3. **Mac Gate 仍未取得 21/21。** 旧输入在第 18 项 `pnpm audit` 因 registry DNS 失败；本轮修改又产生新输入，必须对最终输入重跑全部命令、前后哈希、全 refs 历史扫描及独立证据审核。任何外部漏洞数据访问遵守仓库的外部服务确认要求；未用旧离线 advisory 数据冒充最新审计。

DEV-02-02 继续 `REVIEW`；DEV-02-03～10 与 DEV-03-03～05 保持原冻结/计划状态。本轮未修改数据库、`sources/`、生产数据，未提交、推送或部署。

## ADR-0006 后续定向复验（2026-09-28）

Human 明确授权总控决定技术实现原则后，总控接受 [ADR-0006](../adr/0006-macos-adapter-detached-descendant-policy-proposal.md) 路线 2。`gpt-5.6-terra` Subagent `adapter_detached_lifecycle` 只修改 `adapters/mock/index.mjs`、新增 `adapters/mock/detached-lifecycle-child.mjs` 并补 `crates/adapter-supervisor/src/lib.rs` 的 Mac 集成反例；总控保留 ADR、根门禁、节点状态与最终整合责任。

Mock 测试入口在真实 Mac 上以 Node `detached: true` 创建独立 session 子进程。该子进程仅监听直系 Mock 持有的控制管道，父进程正常 EOF、超过 grace period 被强制终止或直接崩溃时，管道关闭使其自行退出。测试先等待独占 PID 文件并确认 PID 存活，再在 2 秒上界内以 `/bin/kill -0` 确认 PID 不再存在；5 秒保险仅用于防止测试中断遗留进程，不参与通过条件。该证据不宣称平台能回收主动不合作的恶意脱组后代。

| 定向命令 | 结果 |
| --- | --- |
| `node --check adapters/mock/index.mjs`、`node --check adapters/mock/detached-lifecycle-child.mjs` | PASS |
| `cargo test -p adapter-supervisor --locked --offline` | PASS，79/79；新增三路径反例由独立审核者连续运行 12 次均通过 |
| `cargo clippy -p adapter-supervisor --all-targets --locked --offline -- -D warnings`、`cargo fmt --all -- --check`、`git diff --check` | PASS |
| `cargo test --workspace --locked --offline`、`cargo build --workspace --locked --offline`、`cargo clippy --workspace --all-targets --all-features --locked --offline -- -D warnings` | PASS；此为定向/离线检查，不能替代完整 Mac 21 项门禁 |

未参与实现的 `gpt-5.6-terra` Subagent `adapter_lifecycle_independent_review` 对最终文件执行只读 R3 复核，结论为 **AD-01 定向 PASS**。它复查独立 session、控制管道、三条退出路径、初始 PID 存活与最终 PID 消失，以及同组后代、直接进程回收的原有测试。`adapter.shutdown` JSON-RPC 方法属于后续 DEV-03-03～05，不能用此测试宣称已实现。

当前 DEV-03-02 仍 `BLOCKED`：最终输入的 21 项 Mac Gate、前后输入哈希、工具来源、全 refs 历史扫描和独立证据审核尚未齐全。DEV-02-02 仍为 `REVIEW`；其余节点依赖未放行。
