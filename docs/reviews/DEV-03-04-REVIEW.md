# DEV-03-04 Mock Adapter 正常执行流

- 执行模型：Gemini 3.8 Flash；依据 Human 批准的 Wave 2 逐节点执行计划与 ADR-0004。
- 关联需求/AC/Figma：FR-007（Adapter 进程监管与通信）、FR-008（Mock Adapter 实现）、AC-02（Mock 执行流自测闭环）。
- 范围与非目标：实现 Mock Adapter 正常执行流 (`start → heartbeat → output → completed`)、通知 sequence 严格自增验证、产物使用 Artifact 引用返回。不包含故障注入（归属 DEV-03-05）或真实 Codex 进程。
- 变更文件/模块：`adapters/mock/index.mjs`、`adapters/mock/package.json`、`crates/adapter-supervisor/src/protocol.rs`、`crates/adapter-supervisor/src/lib.rs`。

## 协议与执行流设计

- 遵循冻结规范 [PROTOCOL.md](file:///Users/chenzhiqing/Documents/MyVault/%E5%8D%8F%E4%BD%9C%E5%B7%A5%E4%BD%9C/docs/specs/adapter/v0/PROTOCOL.md) §5～§8 与 [adapter-message.schema.json](file:///Users/chenzhiqing/Documents/MyVault/%E5%8D%8F%E4%BD%9C%E5%B7%A5%E4%BD%9C/docs/specs/adapter/v0/adapter-message.schema.json)。
- 独立 Mock Adapter：位于 `adapters/mock/index.mjs`，通过 Node.js 直接执行，响应 `initialize`、`health.check`、`run.start`，按 sequence 1～4 依序发送 `run.heartbeat`、`run.output(stdout)`、`run.output(artifact_candidate)`、`run.completed`。
- 严格序列校验器 (`RunNotificationValidator`)：
  - 强制 sequence 从 1 开始、单调递增，步长为 1；跳序或乱序判定为 `RUN_OUTPUT_OUT_OF_ORDER` (-32015)；
  - 校验 run context 与 fencing token；
  - 校验 output 载荷互斥：`stdout` 必有非空 content 且禁止 artifact；`artifact_candidate` 必有合法 `kind=artifact` 引用且禁止 content；
  - 终态 `run.completed` 后禁止晚到通知。

## TDD 与验证结果

- 初始 RED 阶段：编写 8 个针对跳序、乱序、空 content、null content、缺少 artifact、旧 fencing token、互斥载荷及终态晚到的反例测试，均先断言失败（退出码 101）。
- 编写 GREEN 阶段：实现 `RunNotificationValidator` 与 `SupervisedAdapter::execute_run`，反例全部转绿。
- 测试结果：`cargo test -p adapter-supervisor --locked`，47/47 PASS。
- 包含 Node.js 真实进程端到端测试 `node_mock_adapter_end_to_end_execution`：启动 `node adapters/mock/index.mjs`，走完握手、健康检查及完整执行流，产物严格以引用返回。
- 静态质量：
  - `cargo clippy -p adapter-supervisor --all-targets --all-features --locked -- -D warnings`：PASS，零告警。
  - `cargo fmt -p adapter-supervisor -- --check`：PASS。

## 阶段自验收结论

- 结论：**SELF_CHECK_PASS**。
- 证据路径：`adapters/mock/index.mjs`、`crates/adapter-supervisor/src/protocol.rs`、`crates/adapter-supervisor/src/lib.rs`。
- 状态建议：`REVIEW`（待 Wave 2 全量验收门禁）。
