# DEV-03-02 Adapter 子进程 Supervisor

- 执行模型：`gpt-5.6-terra`（Human 在本任务中明确指定；节点为边界清晰但涉及进程安全的 Rust 多文件实现）。
- 关联要求：DEV-03-02；`docs/specs/adapter/v0/PROTOCOL.md` 第 2、9、11 节；`TECH_STACK_AND_STANDARDS.md` 的参数数组启动、受控 stderr、超时、取消与关闭要求。
- 范围与非目标：仅建立 Adapter 直接子进程的启动、stdio、超时、退出、EOF 关闭与回收边界。不实现 manifest 校验、`initialize`/健康检查、JSON-RPC Schema 解析、Run 状态机、fencing、权限、凭据读取或任何领域/协议字段。
- 变更文件：`crates/adapter-supervisor/src/lib.rs`、本记录。

## 实现与安全边界

- `AdapterCommand` 使用 `Command::new(executable).env_clear().args(arguments)` 直接启动，绝不调用 shell，也不继承主控进程的环境；只有 `AdapterCommand` 显式提供的键值会注入。参数和工作目录必须先由 manifest/Workspace 边界验证。
- stdout 与 stderr 分别管道化。stdout 在 JSON 解析前逐字节按配置的 Adapter V0 1 MiB 行上限进行框定，超限行返回 `LineTooLarge` 后继续读取后续行/EOF，并施加有界反压；stderr 持续排空，仅保留调用方指定大小的诊断前缀和截断标记，避免管道填满阻塞子进程。
- `wait_for_exit` 在本地 deadline 后终止并 `wait` 回收直接子进程；`shutdown` 先关闭 stdin 发送 EOF，超过 grace period 后升级为终止并回收；`Drop` 同样终止并回收直接子进程。
- stderr 从不由本 crate 写入日志，且 API 文档要求调用方在持久化日志前交给 DEV-01-08 的脱敏边界处理。没有新增 JSON-RPC/领域错误码、Run 状态或协议字段。

## TDD 证据

- RED：先只加入生命周期测试，运行 `cargo test -p adapter-supervisor --locked`。测试目标可编译，但因 `AdapterCommand`、`Supervisor`、`SupervisorConfig` 和 `Termination` 尚不存在而以 `E0432` 失败。
- RED（总控反例修订）：新增环境隔离测试后，以仅标准库的 `rustc --edition=2024 --test ...` 编译并运行目标测试；未调用 `env_clear()` 时子 Adapter 继承 `HOME/PATH`，子进程退出失败，父测试断言失败。
- GREEN：加入 `env_clear()` 后，同一独立测试二进制 11/11 通过。
- 测试覆盖：参数以字面值传递（含 shell 元字符）、父环境清空且仅显式变量存在、stderr 受限且持续排空、字面 JSON Lines 往返、CR/LF/超过 1 MiB 的输入拒绝、超过 1 MiB 的 stdout 行返回 `LineTooLarge` 后仍可读取有效行和 EOF、自然退出、超时终止与回收、EOF 优雅关闭、grace 超时后的强制终止、Drop 后直接子进程不会继续运行。
- TDD 工作流建议的 Git checkpoint 与本任务“不得提交”冲突，故未创建提交；上述可复放命令和结果保留为 RED/GREEN 证据。

## 验证结果

| 命令 | 结果 |
| --- | --- |
| `rustc --edition=2024 --test crates/adapter-supervisor/src/lib.rs -o /private/tmp/adapter-supervisor-green`，随后 `/private/tmp/adapter-supervisor-green --nocapture` | PASS：11/11 标准库单元测试。用于绕过当前工作区无关依赖解析阻断。 |
| `cargo fmt -p adapter-supervisor -- --check` | PASS（格式化后复检）。 |
| `cargo test -p adapter-supervisor -p persistence-sqlite --locked` | PASS：Supervisor 11/11；同批 persistence migration 4/4；文档测试无失败。总控已串行整合 SQLx 根锁文件。 |
| `cargo clippy -p adapter-supervisor -p persistence-sqlite --all-targets --all-features --locked -- -D warnings` | PASS：零告警。 |
| `cargo fmt --all -- --check` | PASS。 |
| `git diff --check` | PASS：无空白错误。 |

## 数据、回退与遗留风险

- 无 migration、持久化、领域状态或协议 Schema 变更；未修改根 `Cargo.toml`、`Cargo.lock`、其他 crate 或 `sources/`，也未新增依赖。
- 回退只需移除本 crate 的实现及本审核记录；无数据回滚步骤。
- 该 supervisor 可控地清理**直接** Adapter 子进程。冻结协议同时要求 Adapter 在收到 EOF/`adapter.shutdown` 时清理它自行创建的后代进程；尚无真实 Adapter 可对此作端到端证明。跨平台递归进程树清理未在本节点引入，不能把 Adapter 不遵守该契约的后代存活误称为已验证。
- stdout 当前提供有界原始 JSON Lines 事件，不承担 JSON-RPC/Schema/UTF-8 语义验证；这些属于后续 DEV-03-03，必须在消费事件前完成。当前只在传输层执行原始行 1 MiB 边界。

- 审核结论：**REVIEW**。进程边界反例、Cargo 测试、格式与 Clippy 已通过；等待独立进程/协议边界审核及本批次完整双环境门禁，未在此阶段提前标记 `DONE`。

## 独立审核（2026-09-23）

- 审核模型：`gpt-5.6-terra`，high。选择理由：本节点位于不可信 Adapter 子进程、环境变量、stdio 与进程生命周期的安全/兼容边界。
- 审核权限：只读复核 `crates/adapter-supervisor/**` 与 `Cargo.lock`；本节是唯一修改。
- 已完整核对：`AGENTS.md`、`AGENTS.override.md`、开发基线、质量门禁、冻结 ID/时间和错误规格、Adapter V0 协议及两份 Schema、ADR-0002、ADR-0003 与 Wave 1 协议专项审计。

### 已通过的审查项

- 参数使用 `Command::new(...).args(...)`，没有 shell 拼接；字面 shell 元字符回归测试通过。
- 启动使用 `env_clear()`，只注入 `AdapterCommand` 的显式环境；对 `HOME`、`PATH` 未继承的反例测试通过。
- stdin 拒绝 CR、LF 及超过 1 MiB 的原始字节输入；stdout 使用原始字节计数，在解析前将超过 1 MiB 的行报告为 `LineTooLarge`，并可继续读取随后合法行和 EOF。
- stderr 由独立读取线程持续排空，保留内容有上限和截断标识；现有大 stderr 测试通过。
- EOF 优雅关闭、超时强制终止、直接子进程的 `Drop` 回收与常规退出均有单元测试。
- `cargo fmt -p adapter-supervisor -- --check`：PASS。
- `cargo test -p adapter-supervisor --locked`：PASS，11/11；doc test 0/0。
- `cargo clippy -p adapter-supervisor --all-targets --all-features --locked -- -D warnings`：PASS，零告警。

### 阻断发现

#### AUD-03-02-001：遗留后代持有 stdio 时，关闭和 Drop 可无限阻塞

- 风险：S1 / R3；违反 DEV-03-02 的“关闭与孤儿进程清理可控”标准，以及 `QUALITY_GATES.md` G8 的无孤儿进程要求。
- 根因：`finish` 在直接子进程已退出后于 `src/lib.rs:328-335` 无期限 `join` stdout/stderr 读取线程；`Drop` 同样在 `src/lib.rs:356-360` 无期限 join。若不可信 Adapter 启动继承 stdout/stderr 的后代，直接 Adapter 退出或被 kill 后，后代仍保持管道写端，读取线程不能收到 EOF，两个 join 都无法返回。
- 可复现：独立临时审计程序以 `/bin/sh -c "/usr/bin/perl -e 'sleep 30' adapter-supervisor-orphan-audit-20260923 & exit"` 启动 Adapter，随后调用 `shutdown(Duration::from_millis(50))`。外层 3 秒 watchdog 结束该程序，结果 `orphan-repro-exit=142`，未出现预期的 `shutdown returned`。测试进程随后按唯一标签清理。
- 现有“Drop 后无 marker”只覆盖直接子进程，不能覆盖后代仍持有 pipe 的情况。
- 修复验收：在不可信 Adapter 遗留继承 stdio 后代的情况下，`shutdown` 与 `Drop` 均须在有界时间返回；直接子进程必须被 reap；不得因 join 永久占用控制面线程。跨平台策略需明确并测试（例如受控进程组/作业对象，或在关闭时可中断的 pipe reader），不能仅在文档中声明后代由 Adapter 自行清理。

#### AUD-03-02-002：允许的 1 MiB stdin 消息可在 Adapter 不读取时永久阻塞

- 风险：S1 / R3；违反协议“平台为每个请求设置 timeout”以及 G8 的可取消/可关闭后台任务要求。
- 根因：`send_protocol_line` 在 `src/lib.rs:255-258` 对 pipe 执行同步 `write_all` 和 `flush`，没有写入 deadline、取消路径或可从并发控制路径触发的终止机制。1 MiB 是冻结协议允许的单条上限，但普通 pipe 缓冲远小于该值；恶意或故障 Adapter 不读取 stdin 时，调用线程会阻塞在 write，无法进入 `wait_for_exit`/`shutdown` 的 timeout 逻辑。
- 可复现：独立临时审计程序以不读取 stdin 的 `/usr/bin/perl -e 'sleep 30' adapter-supervisor-stdin-audit-20260923` 为 Adapter，并调用 `send_protocol_line(vec![b'x'; 1_048_576])`。外层 3 秒 watchdog 结束该程序，结果 `stdin-repro-exit=142`，未出现预期的 `write returned`。测试进程随后按唯一标签清理。
- 修复验收：传输写入必须可设置 deadline/cancel，并在 Adapter 不读 stdin 时在有界时间返回稳定错误；同时必须证明超时后可终止并 reap 直接子进程，且不引入绕过 1 MiB/CR/LF 边界或 shell/环境隔离的回归。

### 独立审核结论

**BLOCKED**。现有单元测试和三项 Rust 检查均通过，但两条可重放的 S1 生命周期反例说明：故障或恶意 Adapter 可以使关闭/Drop 或单次允许的协议写入无限阻塞。未完成上述修复、负向回归测试与本批次 ADR-0003 双环境门禁前，DEV-03-02 必须保持 `BLOCKED`，不得启动依赖 DEV-03-03。

## S1 阻断修复（2026-09-23）

- 实际分工：原 `gpt-5.6-terra` / high 执行 Agent 完成 Tokio 方案与看门狗测试草案；其验证阶段异常滞留后由总控停止 Agent，并在不扩大节点范围的前提下完成一致性整合、锁文件串行更新、错误语义补齐和定向验证。
- `adapter-supervisor` 现在使用冻结技术栈中的 Tokio 1.x 管理子进程、异步 stdio、deadline、取消和 reader task；crate 只启用 `io-util`、`macros`、`process`、`rt`、`sync`、`time` 最小 feature 集。
- `send_protocol_line` 已改为 async API，调用方必须同时提供绝对 deadline 和 `CancellationToken`。deadline、取消或 pipe 错误会关闭 stdin、启动直接子进程终止，并返回稳定的 `WriteTimedOut`、`WriteCancelled` 或 `Process` 错误；后续 `shutdown`/`wait_for_exit` 会 reap 直接子进程，并把退出归类为 `ForcedShutdown`，不会把强制终止误报为优雅 EOF。
- stdout/stderr reader 改为 Tokio task。直接子退出后先给管道 100 ms 有界排空窗口；若后代仍持有 pipe，则 abort reader task，关闭本端 pipe handle，并在 `ExitReport` 中记录 `CancelledAfterChildExit`。`finish` 和 `Drop` 均不再执行无期限线程 `join`。
- `Drop` 关闭 stdin、启动直接子进程 kill、丢弃启用 `kill_on_drop` 的 child，并 abort 两个 reader task；显式 `shutdown`/`wait_for_exit` 仍是取得确定 reap/exit report 的首选路径。Adapter 自身后代仍必须遵循冻结协议在 EOF/`adapter.shutdown` 时清理；本修复不把跨进程树强制清理冒充为已实现能力。

### 新增反例与验证

- 遗留后代继承 stdout/stderr：fixture 直接参数启动同一测试二进制作为后代并持有 pipe；外层 1.2 s watchdog 证明显式 `shutdown` 与 `Drop` 均在 800 ms 内返回，不使用 shell。
- 不读取 stdin：向 fixture 写入冻结上限内的 1 MiB 行，75 ms deadline 必须返回 `WriteTimedOut`；随后 100 ms shutdown 必须终止并 reap，且报告 `ForcedShutdown`。外层 watchdog 防止测试自身挂起。
- 预先取消：`CancellationToken` 必须返回 `WriteCancelled`，关闭不安全的部分协议流，并由 shutdown 以 `ForcedShutdown` 回收。
- 原有参数字面传递、环境清空、stderr 上限、JSON Lines 往返、CR/LF/超限拒绝、超限 stdout 后续恢复、自然退出、EOF、超时、强制关闭和直接子 Drop 用例全部保留。

| 命令 | 修复后结果 |
| --- | --- |
| `cargo test -p adapter-supervisor --locked -- --test-threads=1` | PASS：16/16；doc test 0/0。 |
| `cargo clippy -p adapter-supervisor --all-targets --all-features --locked -- -D warnings` | PASS：零告警。 |
| `cargo fmt --all -- --check` | PASS。 |
| `git diff --check` | PASS。 |

- 修复后审核结论：**REVIEW**。`AUD-03-02-001` 与 `AUD-03-02-002` 已有最小实现和有界负向回归证据；仍须由新的独立 `gpt-5.6-terra` 复审这两项 S1 修复，并通过本批次完整 macOS ARM64 + Ubuntu ARM64 门禁后才能标记 `DONE` 或启动 DEV-03-03。

## S1 修复第二位独立复审（2026-09-23）

- 审核模型：`gpt-5.6-terra`，high。理由：本次只复审不可信子进程的取消、回收与资源边界，属于 R3 进程安全/恢复风险。
- 审核权限：只读 `crates/adapter-supervisor/**` 与 `Cargo.lock`；本节为唯一写入。未创建或运行任何额外外部复现程序，未单独启动 30 秒 sleeper。
- 已完整核对：`AGENTS.md`、`AGENTS.override.md`、开发基线、节点表、质量门禁、`DATA_CONVENTIONS.md`、`ERROR_CATALOG.md`、`STATE_MACHINES.md`、`PERMISSION_MATRIX.md`、Adapter V0 协议与两份 Schema、ADR-0002、ADR-0003 及本记录。

### AUD-03-02-001：遗留 pipe 后代不再造成无界关闭

- `Supervisor::start`（`src/lib.rs:287-314`）将 stdout/stderr 读取器置于 Tokio task；不再使用不可中断的 OS 线程 join。
- 显式 `wait_for_exit` / `shutdown` 都先 `Child::wait` 回收直接子进程，必要时 `start_kill` 后再次 `wait`（`405-465`）。`finish_reader` 只给予每个 reader 100 ms 排空窗口，随后 `abort` 并 await 其取消结果（`513-531`）；继承 pipe 的后代不能使调用无限等待，报告会明确为 `CancelledAfterChildExit`。
- `Drop`（`496-510`）没有 await/join：关闭 stdin、请求终止直接子进程并 abort 两个 reader，因此保持有界。需要可观察的直接子进程 reap/exit status 时，API 明确要求调用 `shutdown` 或 `wait_for_exit`，而不是依赖 `Drop`。
- 现有仓库 watchdog 用例 `descendant_pipe_holders_cannot_block_shutdown_or_drop`（`883-919`）以 1.2 秒外层时限验证显式关闭与 Drop 均在 800 ms 内返回；本次运行通过。

### AUD-03-02-002：不读取 stdin 的 1 MiB 写入具有 deadline 和取消边界

- `send_protocol_line` 继续在写入前拒绝 CR、LF 与超过冻结 1 MiB 的输入（`347-358`），并以绝对 deadline 和 `CancellationToken` 包裹异步 `write_all` / `flush`（`360-376`）。
- deadline、取消或 pipe 错误都会丢弃 stdin、调用 `start_kill` 并设置强制终止标记（`379-385`）；后续显式 `shutdown` / `wait_for_exit` 会 reap 直接子进程，并将已开始的强制终止报告为 `ForcedShutdown`（`405-444`）。
- 现有 watchdog 用例写入恰好 1 MiB、75 ms deadline，断言 `WriteTimedOut`，继而 100 ms `shutdown` 得到已 reap 的 `ForcedShutdown`（`921-954`）；预取消路径同样断言 `WriteCancelled` 和强制回收（`957-980`）。本次运行通过。

### 回归与门禁证据

| 命令 | 结果 |
| --- | --- |
| `cargo test -p adapter-supervisor --locked -- --test-threads=1` | PASS：16/16；在 1.2 秒内完成，未触发 watchdog；doc test 0/0。 |
| `cargo fmt -p adapter-supervisor -- --check` | PASS。 |
| `cargo clippy -p adapter-supervisor --all-targets --all-features --locked -- -D warnings` | PASS：零告警。 |

### 独立复审结论

**PASS（仅针对 AUD-03-02-001 / AUD-03-02-002 的 S1 修复）**。静态反例审查与受 watchdog 约束的现有回归测试均证明：reader 不会因遗留 pipe 后代产生无界 join，Drop 保持有界，允许的 1 MiB stdin 写入可按 deadline/cancel 返回，且显式关闭/等待会终止并 reap 直接子进程、正确报告 `Termination`。这不是 DEV-03-02 的 `DONE` 结论：节点仍须通过本批次 ADR-0003 的完整 macOS ARM64 与 Ubuntu ARM64 门禁，才可由总控从 `REVIEW` 更新为 `DONE` 并解锁 DEV-03-03。
