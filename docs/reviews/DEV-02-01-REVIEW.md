# DEV-02-01 强类型 ID、UTC 时间、版本与 Actor 值对象

- 执行模型：`gpt-5.6-terra`
- 关联需求/AC/Figma：DEV-00-06 冻结数据约定；DEV-00-02 领域词汇与事实归属；DEV-02-01 节点审核标准。
- 范围与非目标：仅实现 UUIDv7 强类型 ID、UTC 毫秒时间、Version/ExpectedVersion 与 Actor 归因值对象及其 Serde 边界表示；不实现状态机、权限、持久化 migration、ID 生成器或 Adapter 协议。Actor 没有冻结 wire shape，明确留给 contracts DTO。
- 变更文件/模块：`crates/domain/Cargo.toml`、`crates/domain/src/lib.rs`、`crates/domain/tests/value_objects.rs`。
- 领域与数据影响：ID 数据库表示固定为 `[u8; 16]`，协议/Serde 表示为小写带连字符 UUIDv7；时间为已校验的 UTC Unix milliseconds，协议/Serde 表示为固定毫秒 RFC 3339；Version/ExpectedVersion 的 Serde 表示为正整数；可变聚合从 version 1 起单调递增，非创建更新由 `ExpectedVersion` 表达前置条件。
- 安全与隐私影响：Actor 仅保存 Human/Role/System/Adapter 的归因区分，不授予权限或引入状态语义；未接触凭据、路径或外部进程。
- 验证命令：`rustfmt --check crates/domain/src/lib.rs crates/domain/tests/value_objects.rs`；`cargo test -p domain --locked`；`cargo clippy -p domain --all-targets --all-features --locked -- -D warnings`。
- 验证结果：2026-09-22 定向检查全部通过；domain 集成测试 12/12 通过（含 3 个 serde_json 正反/往返用例），doc-tests 通过，clippy 零告警。全工作区格式与门禁由工程 Agent/总控统一复核。
- 失败/边界用例：覆盖非规范 UUID、错误 UUID 版本/variant、UUIDv7 排序、协议/16-byte 往返、UTC 前 epoch/闰日/非法日期、版本零值/溢出、Actor 变体区分，以及 ID/UTC/Version/ExpectedVersion 的 serde_json 规范表示、往返与非法输入拒绝。
- 视觉/无障碍证据（适用时）：不适用（纯 Rust 领域值对象）。
- 迁移与回退：尚无 schema migration；回退为移除此节点新增的 domain 值对象与测试，不涉及已持久化数据。
- 遗留风险：Serde 与 serde_json 依赖已由根 `Cargo.lock` 统一固定；全工作区门禁由总控集成复核。本节点无额外已知风险。
- 审核结论：PASS（节点专属实现与定向质量检查通过；全工作区门禁由总控统一复核）。
