# DEV-03-03 Initialize / 能力协商 / 健康检查

- 执行模型：Gemini 3.8 Flash；依据 Human 批准的 Wave 2 逐节点执行计划与 ADR-0004。
- 关联需求/AC/Figma：FR-005（Harness 能力探测与协商）、FR-006（运行时健康检查与诊断）、AC-02（Adapter 协议握手与版本兼容性）。
- 范围与非目标：实现 Adapter V0 的 `initialize` 协议握手、SemVer 2.0.0 严格版本判定、能力与认证状态协商，以及 `health.check` 健康检查与故障留痕。不实现 Run 状态机、Task claim 或具体 Mock 进程。
- 变更文件/模块：`crates/adapter-supervisor/src/semver.rs`、`crates/adapter-supervisor/src/json.rs`、`crates/adapter-supervisor/src/protocol.rs`、`crates/adapter-supervisor/src/lib.rs`。

## 协议与兼容性设计

- 依据冻结规范 [PROTOCOL.md](file:///Users/chenzhiqing/Documents/MyVault/%E5%8D%8F%E4%BD%9C%E5%B7%A5%E4%BD%9C/docs/specs/adapter/v0/PROTOCOL.md) §3～§5 与 [adapter-message.schema.json](file:///Users/chenzhiqing/Documents/MyVault/%E5%8D%8F%E4%BD%9C%E5%B7%A5%E4%BD%9C/docs/specs/adapter/v0/adapter-message.schema.json)。
- 严格遵循 SemVer 2.0.0，禁止核心版本号或纯数字预发布标识携带前导零。
- 当前协议版本为 `0.1.0`。若 Adapter 返回不兼容的主版本或非法版本，返回 JSON-RPC `-32010` 并映射至 `ADAPTER_INCOMPATIBLE`，标记 `retryable: false`。
- 格式违规、缺少必需字段或非 JSON-RPC 响应，映射至 JSON-RPC `-32099`（`ADAPTER_PROTOCOL_VIOLATION`）。
- 握手与健康检查超时或进程异常崩溃，诊断保留进程退出码与有界捕获的 stderr 输出，映射至 JSON-RPC `-32017`（`ADAPTER_UNAVAILABLE`），标记 `retryable: true`。
- 认证信息仅传递 `keychain://` 等安全引用，不泄露明文凭据。

## TDD 与验证结果

- 测试结果：`cargo test -p adapter-supervisor --locked`，36/36 PASS（16 个既有用例 + 20 个 DEV-03-03 新用例）。
- 覆盖负向反例：
  - `00.1.0`、`01.1.0`、`1.2.3-01` 等前导零非法 SemVer 被拒绝；
  - 返回 `"0.2.0"` 不兼容协议版本被拦截（返回 `Incompatible` / `-32010`）；
  - 缺失必需字段（缺少 `capabilities`、`auth_status`）返回 `ProtocolViolation`（`-32099`）；
  - 非 JSON-RPC 格式响应返回 `ProtocolViolation`；
  - 握手超时被诊断为 `TimedOut`（`-32017`）；
  - 握手崩溃保留退出码及有界 stderr 日志（`-32017`）；
  - 健康检查超时与异常退出诊断测试。
- 正向协商：
  - 成功协商协议版本 `0.1.0`、capabilities、认证状态、隔离与恢复模式；
  - 成功执行健康检查，解析 `Healthy` / `Degraded` 状态及诊断码。
- 静态质量：
  - `cargo clippy -p adapter-supervisor --all-targets --all-features --locked -- -D warnings`：PASS，零告警。
  - `cargo fmt -p adapter-supervisor -- --check`：PASS。

## 阶段自验收结论

- 结论：**SELF_CHECK_PASS**。
- 证据路径：`crates/adapter-supervisor/src/protocol.rs`、`crates/adapter-supervisor/src/lib.rs`。
- 状态建议：`REVIEW`（待 Wave 2 全量验收门禁）。
