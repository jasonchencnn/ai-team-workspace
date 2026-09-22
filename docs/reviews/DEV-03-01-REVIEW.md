# DEV-03-01 Adapter V0 Schema 契约整改与复核

- 执行模型：`gpt-5.6-sol`
- 关联需求/AC/审计：DEV-00-08 冻结 Adapter V0；AUD-03-01-001～004；Gate D Adapter 契约前置条件
- 范围与非目标：只补强既有 Adapter V0 冻结语义的机器约束、反例测试和审核证据；不新增方法、字段、错误码、业务能力，不实现 supervisor/真实 Adapter，不启动 Wave 2
- 变更文件/模块：ADR-0002、Adapter V0 协议与两份 Schema、契约测试、DEV/Wave 1 状态与审核记录
- 领域与数据影响：无数据库、迁移、领域状态或合法 payload 语义变化；协议版本保持 `0.1.0`
- 安全与隐私影响：超限或无事实 output 在边界拒绝；自定义错误不能绕过冻结码表或透传未知 data；测试不使用真实凭据或公网
- 验证结果：专项审计列出的 pnpm、Cargo locked、构建、生成物漂移和静态安全检查全部通过
- 失败/边界用例：UUIDv4/大写 UUID、非 UTC/非三位毫秒/非法日期、空与错配 output、多字节超 64 KiB、整消息超 1 MiB、缺失 data/未冻结或错配错误、SemVer 前导零
- 迁移与回退：无迁移；按 ADR-0002 回退 Schema、测试和文档，并将 DEV-03-01/Wave 1 恢复为 BLOCKED 后重新复核
- 遗留风险：字节预算关键字必须由未来 Adapter runtime/supervisor 实现或提供等价原始行检查；该实现属于后续节点，本节点只冻结并验证机器契约
- 审核结论：**PASS / DONE**

## 机器契约整改

1. `$defs.uuid` 同时执行 UUID format 与规范小写 UUIDv7 pattern，包含版本位与 variant 校验。
2. `$defs.timestamp` 同时执行有效 `date-time` format 与固定 `YYYY-MM-DDTHH:mm:ss.sssZ` pattern。
3. `run.output` 按 `kind` 强制互斥载荷：文本类必须且只能携带非空 `content`；`artifact_candidate` 必须且只能携带 `kind=artifact` 的 reference。
4. `x-maxUtf8Bytes=65536` 与 `x-maxJsonUtf8Bytes=1048576` 由契约 validator 执行；测试覆盖等于上限与超过 1 byte。
5. 标准 JSON-RPC 错误使用独立兼容分支；自定义错误只允许冻结的十个 code，必须携带统一 data，并校验 code/domain/retryable 映射。
6. Manifest、协议协商与 Adapter 版本使用规范 SemVer 2.0.0 pattern，拒绝核心版本及纯数字预发布标识的前导零。

## TDD 证据

- RED：新增审计反例后，`pnpm test:contract` 为 21 个用例中 9 组失败，失败分别来自当前 Schema 过宽或标准错误兼容分支缺失。
- GREEN：最小 Schema 修正后，同一命令 21/21 通过；全部原有示例继续通过。
- 未创建 TDD checkpoint commit：Human 明确禁止提交，本次只保留可重放命令和审核记录。

## 最终验证记录

| 命令 | 结果 |
| --- | --- |
| `pnpm run check:toolchain` | PASS：Node 24.21.0、pnpm 10.33.0、Rust 1.98.0 |
| `pnpm run fmt:check` | PASS |
| `pnpm run typecheck` | PASS |
| `pnpm run lint` | PASS：ESLint + Rust clippy 零告警 |
| `pnpm run verify:guards` | PASS |
| `pnpm run contracts:check` | PASS：无生成物漂移 |
| `pnpm run test` | PASS：Vitest 35/35 |
| `pnpm run build` | PASS |
| `cargo test --workspace --locked` | PASS：Rust 13/13 |
| `cargo build --workspace --locked` | PASS |
| `cargo clippy --workspace --all-targets --all-features --locked -- -D warnings` | PASS |
| `cargo metadata --locked --no-deps` | PASS |

补充静态复核：Tauri `main-window` capability 的 `permissions` 为空；manifest/lockfile 未引入 shell/fs/opener plugin；业务 Rust 未发现 unsafe 实现或 `allow(unsafe_code)`；代码与测试目录未发现形似真实秘密的赋值；运行时代码未新增 `console.*`，仅生成物检查 CLI 保留成功/失败输出。

`verify:guards` 与 lint 首次并行执行时，守卫脚本临时创建的故意违规 fixture 被 lint 抢读，导致一次基础设施竞态；按门禁语义顺序重跑完整 `pnpm run lint` 后通过。该竞态没有修改产品代码、Schema 或最终验证结果。

结论：专项审计四项发现全部关闭，DEV-03-01 恢复为 **DONE**。
