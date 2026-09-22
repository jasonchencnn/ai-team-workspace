# Wave 1 独立复核暨 DEV-03-01 Adapter 协议专项审计

- 审核日期：2026-09-22
- 审核范围：Wave 1 九个节点的可重放门禁；重点复核 R3 节点 DEV-01-07、DEV-03-01 与 Tauri capability
- 风险路由：协议整改与复核必须使用 `gpt-5.6-sol`
- 最终结论：**PASS**

## 1. 总结

Wave 1 的工具链、格式、类型、Lint、生成物漂移、测试、构建、Rust locked checks、Tauri 最小 capability 和静态秘密/unsafe 扫描均通过。独立反例审查曾发现 DEV-03-01 的 Adapter V0 JSON Schema 与冻结数据/协议规范存在可复现偏差，因此在整改前撤销了原 Wave 1 `PASS`。

Human 已接受 ADR-0002。偏差现已通过 Schema、运行时字节关键字、负向契约测试和第二遍反例复核关闭；DEV-03-01 恢复为 `DONE`，Wave 1 恢复为 `PASS`。本次整改没有启动依赖 DEV-03-01 的节点，也没有进入 Wave 2。

## 2. 已通过的独立验证

使用项目固定工具链 Node `24.21.0`、pnpm `10.33.0`、Rust `1.98.0` 重跑：

| 检查                                                                            | 结果                                  |
| ------------------------------------------------------------------------------- | ------------------------------------- |
| `pnpm run check:toolchain`                                                      | PASS                                  |
| `pnpm run fmt:check`                                                            | PASS                                  |
| `pnpm run typecheck`                                                            | PASS                                  |
| `pnpm run lint`                                                                 | PASS                                  |
| `pnpm run verify:guards`                                                        | PASS                                  |
| `pnpm run contracts:check`                                                      | PASS，无生成物漂移                    |
| `pnpm run test`                                                                 | PASS，TypeScript/Vitest 35/35         |
| `pnpm run build`                                                                | PASS                                  |
| `cargo test --workspace --locked`                                               | PASS，Rust 13/13                      |
| `cargo build --workspace --locked`                                              | PASS                                  |
| `cargo clippy --workspace --all-targets --all-features --locked -- -D warnings` | PASS                                  |
| `cargo metadata --locked --no-deps`                                             | PASS                                  |

补充静态审查结果：

- Tauri `main-window` capability 的 `permissions` 为空，未引入 shell/fs/opener plugin；
- 业务 Rust crate 未发现 `unsafe` 或 `allow(unsafe_code)`；
- 代码与测试目录未发现形似真实 API key、token、cookie 或 credential 的赋值；
- `console.log/error` 仅位于生成物检查 CLI 的成功/失败提示，不是桌面应用运行时日志。

## 3. 历史阻断发现

### AUD-03-01-001：UUIDv7 与 UTC 毫秒时间未被机器契约执行

- 风险：R3，阶段阻断
- 冻结规则：内部 ID/协议 ID 使用规范小写 UUIDv7；协议时间使用 UTC `Z`、RFC 3339、固定三位毫秒。
- 实现现状：`$defs.uuid` 仅声明通用 `format: uuid`；`$defs.timestamp` 仅声明通用 `format: date-time`。
- 反例结果：UUIDv4、大写 UUID、`+08:00` 时区、无小数秒和六位微秒时间均通过当前 Adapter Schema。
- 影响：契约层接受的数据会在严格领域值对象处被拒绝，造成跨边界行为不一致；非规范表示还会破坏稳定比较、追踪和重放假设。

### AUD-03-01-002：`run.output` 可通过空载荷、类型错配与超字节预算载荷

- 风险：R3，阶段阻断
- 冻结规则：output 必须携带受限 `content` 或 Artifact reference；单条 `content` 最大 64 KiB。
- 实现现状：`anyOf` 只要求属性存在，`content: null` 或 `artifact: null` 仍通过；`artifact_candidate` 可只携带普通文本；`maxLength: 65536` 计算字符而非 UTF-8 字节。
- 反例结果：空 content、空 artifact、artifact kind 与文本载荷错配、196608 字节的多字节字符串均通过 Schema。
- 影响：运行时可能接收无事实内容或超预算消息，削弱顺序、资源和 Artifact 边界。

### AUD-03-01-003：自定义错误响应未强制统一错误数据与冻结码表

- 风险：R3，阶段阻断
- 冻结规则：Adapter 自定义错误映射到已定义领域错误，并携带 `domain_code`、`retryable`、`correlation_id` 与白名单字段。
- 实现现状：整个 `error.data` 可省略；自定义错误范围内未限制为已冻结的 `-32010`～`-32018`、`-32099`。
- 反例结果：`-32014` 不带 `data`、未定义的 `-32098` 均通过 Schema。
- 影响：无法稳定映射 `RUN_FENCED` 等关键错误，也无法保证诊断关联和 retryable 语义。

### AUD-03-01-004：SemVer 约束允许非规范前导零

- 风险：R2，与本次协议整改一并修复
- 实现现状：协议版本模式允许 `00.1.0`；manifest 的版本模式同样未完整执行 SemVer 规范。
- 影响：版本协商可能对同一逻辑版本产生多种表示，增加兼容判断歧义。

## 4. 修复与复核标准

由于 Adapter V0 Schema 属冻结协议边界，整改前必须新增并由 Human 接受 ADR。建议 ADR 明确这是对既有冻结语义的机器约束补强，而非新增业务能力。

整改至少满足：

1. 为协议 ID 使用规范小写 UUIDv7 约束，并为时间同时使用有效日期校验与固定 `YYYY-MM-DDTHH:mm:ss.sssZ` 形式约束；
2. 为上述全部已证实反例增加负向契约测试；
3. 按 `kind` 对 `run.output` 的非空 content / Artifact reference 建立条件约束，并在运行时测试 UTF-8 字节上限与整条消息 1 MiB 上限；
4. 自定义错误必须携带统一 `data`，错误码限制为冻结集合；JSON-RPC 标准错误另行保留兼容分支；
5. 协议版本使用唯一规范 SemVer 表示；
6. 重跑本报告第 2 节全部门禁，并由独立 `gpt-5.6-sol` 进行第二遍反例复核。

## 5. 整改与第二遍复核结果

| 审计项 | 整改证据 | 复核结果 |
| --- | --- | --- |
| AUD-03-01-001 | UUID 同时执行 format + 小写 UUIDv7/variant pattern；时间同时执行有效 date-time + 固定 UTC 三位毫秒 pattern | PASS：UUIDv4、大写 UUID、偏移时区、缺毫秒、六位微秒、非法日期全部拒绝 |
| AUD-03-01-002 | `run.output` 按 kind 互斥 content/Artifact；`x-maxUtf8Bytes` 与 `x-maxJsonUtf8Bytes` 执行字节预算 | PASS：null、空载荷、kind 错配、多字节超限与整消息超限全部拒绝；边界值通过 |
| AUD-03-01-003 | 标准错误与自定义错误拆分；自定义 code 为冻结离散集合，data 必填并绑定 domain/retryable | PASS：缺 data、`-32098`、映射错配均拒绝；标准错误兼容用例通过 |
| AUD-03-01-004 | Manifest 与消息版本字段共用规范 SemVer 2.0.0 pattern | PASS：核心/数字预发布前导零拒绝，规范预发布和构建元数据通过 |

契约测试从 15 个扩展为 21 个；新增反例时先得到 9 组预期 RED，最小 Schema 修正后同一套测试 21/21 GREEN。全部既有 manifest/message 示例保持通过。

本报告第 2 节所有命令已使用 Node `24.21.0`、pnpm `10.33.0`、Rust `1.98.0` 重跑并通过。`verify:guards` 与 lint 首次并行执行时出现临时故意违规 fixture 的读取竞态；改为顺序执行后完整 lint 通过，该竞态不属于产品或协议缺陷。

## 6. 非阻断遗留项

- 当前默认 shell 的 Node 是 `22.22.3`，项目要求 `24.21.0`；本机已安装正确版本，固定工具链下所有检查通过。后续执行必须先选择项目版本，不能忽略 engine 警告。
- 当前目录不是 Git 仓库，仍无法提供基于提交历史的 diff 证据。
- 现有 integration/e2e 是底座入口，不是产品级真实 E2E；完整证据属于后续节点。
- Tauri 临时图标、`720px` 最小窗口宽度与 CLI 依赖分类在后续 UI/发布或 Gate B 收口，不影响本次协议复核结论。
- Wave 1 尚无覆盖率报告；DEV-02-01 范围较小且专属测试覆盖关键边界，Gate C 的覆盖率阈值仍由后续领域阶段统一验收。

## 7. 最终结论

专项审计确认的协议门禁缺陷已按 Human 接受的 ADR-0002 全部关闭。机器契约、正反例、全量工程门禁和第二遍反例审查均通过。

当前结论为 **PASS**：DEV-03-01 为 `DONE`，Wave 1 为 `PASS`。本次工作到此停止，未启动 Wave 2。
