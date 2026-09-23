# DEV-01-08 结构化日志、Correlation ID 与脱敏框架

- 执行模型：`gpt-5.6-sol`（节点指定模型）。执行 Agent 写入实现后未完成交接，总控终止该 Agent，保留其补丁并完成逐段审查、反例复核和验证。
- 关联要求：DEV-01-08；`TECH_STACK_AND_STANDARDS.md` 的 JSON Lines、UTC 时间、request/team/task/run 关联与日志/错误/Event 统一脱敏边界。
- 范围与非目标：在 `observability` crate 建立可复用框架，不接入具体 host、文件轮转或生产凭据存储，不改变领域、Adapter 协议、migration 或 `sources/`。
- 变更文件：`crates/observability/Cargo.toml`、`crates/observability/src/lib.rs`、`crates/observability/tests/structured_logging.rs`，以及由总控串行更新的 `Cargo.lock`。

## 实现与安全边界

- `Sensitive<T>` 的 `Display`、`Debug` 与 `Serialize` 固定输出 `[REDACTED]`；明文只能在显式回调中短暂暴露。
- `Redactor` 为日志、错误和 Event 文本提供同一 `SafeText` 输出，覆盖已登记 secret 的明文、hex、percent encoding、base64 与 base64url 形式。
- `JsonSubscriber` 输出单行结构化 JSON，固定包含 UTC 毫秒时间、level、module 和 request/team/task/run 字段；所有字符串字段在写出前再次经过 redactor，敏感字段名无条件遮蔽。
- `Correlation` 通过 tracing span 树传播 request ID，并可关联 team/task/run；无关联值时显式输出 `null`。
- 时间 provider 可注入，测试不依赖真实时间。

## TDD 与第二遍反例审查

- RED：新增结构化日志测试最初因 `Correlation`、`JsonSubscriber`、`Redactor`、`Sensitive` 和 `REDACTED` 尚不存在而编译失败。
- GREEN：`cargo test -p observability --locked` 通过 7/7。
- 关键反例：父子 span 传播；父 handle 提前释放；克隆 handle 生命周期；无 correlation；敏感字段名；明文及多种派生编码；错误/Event/Sensitive 统一安全表示。
- 总控逐段复核自定义 subscriber 的 span 引用计数、父引用释放、字段覆盖顺序、锁失败路径与最终写出边界，未发现阻断问题。

## 验证结果

| 门禁                                                                                 | 结果                                                                                                 |
| ------------------------------------------------------------------------------------ | ---------------------------------------------------------------------------------------------------- |
| `cargo test -p observability --locked`                                               | PASS：7/7。                                                                                          |
| `cargo clippy -p observability --all-targets --all-features --locked -- -D warnings` | PASS。                                                                                               |
| Cargo workspace locked test/build/clippy/metadata                                    | PASS：Rust 25/25，构建成功，零 Clippy warning，锁文件解析成功。                                      |
| `pnpm run fmt:check`、`pnpm run typecheck`、串行 Lint、`pnpm run verify:guards`      | PASS。                                                                                               |
| `pnpm run test`、`pnpm run build`、`pnpm run contracts:check`                        | PASS：TS 35/35；构建与生成漂移检查通过。                                                             |
| 依赖安全                                                                             | PASS：npm high/critical 为 0；cargo-audit 无 failure-class vulnerability；Node/Rust 许可证检查通过。 |
| 秘密静态筛查与 `git diff --check`                                                    | PASS。                                                                                               |

- 视觉/无障碍：不适用，无 UI 变更。
- 迁移与回退：无 migration 或数据写入；移除 observability 新依赖和实现并恢复锁文件即可回退。
- 遗留风险：host 接入时仍需配置日志轮转/保留上限，并确保所有运行时 secret 在构建 `Redactor` 时登记；`Sensitive::expose_secret` 的调用点必须保持短生命周期且不得把值带入错误。RustSec 另报告现有 Tauri/GTK 链 7 个非 failure warning，详见 DEV-01-05 审核记录。
- 审核结论：**PASS**；节点可标记 **DONE**。
