# 归档：Codex 时期（2026-09-20 至 10-10）

这里保存项目重置前的旧代码、旧工具和冻结规格。它们不参与构建、测试和 CI，需要时可以直接参考或取回。

| 路径 | 内容 |
|---|---|
| `codex-era/crates/domain` | 纯状态机（Task / Run / Binding / Message / Approval）、权限矩阵、幂等、Outbox、fencing |
| `codex-era/crates/persistence-sqlite` | 35 个 SQLite 迁移（257 个触发器）及数据库测试 |
| `codex-era/crates/adapter-supervisor` | JSON-RPC 2.0 over stdio 适配协议、子进程管理 |
| `codex-era/crates/observability` | 结构化日志与脱敏 |
| `codex-era/crates/contracts` | Rust → JSON Schema → TypeScript 契约生成 |
| `codex-era/crates/application`、`security` | Message 到期事务、审计消费、安装 Owner 身份 |
| `codex-era/crates/workspace` | 空占位 |
| `codex-era/adapters/mock` | Node 实现的 Mock 适配器，含故障注入 |
| `codex-era/packages` | 生成的 TypeScript 契约包、空的测试夹具包 |
| `codex-era/scripts`、`tests`、`config` | 21 项本地门禁运行器、工具链守卫、契约漂移检查及其测试和配置 |
| `codex-era/ci/ci.yml` | 旧的 14 任务 CI 工作流 |
| `codex-era/specs` | 冻结规格：领域模型、状态机、权限矩阵、错误码、数据约定、信息架构、需求追踪、Adapter V0 协议 |

## 取回方式

- **参考**：直接阅读本目录。
- **取回某个 crate**：`git mv archive/codex-era/crates/<名称> crates/<名称>`，再把它的依赖接回新代码。旧 crate 之间互相引用，单独取回时要一起检查依赖。
- **完整可运行状态**：检出提交 `519d4d6`（分支 `codex/wave-2-ci-validation`），当时 Rust 测试 737 通过、0 失败、1 忽略。
- **已删除的过程文档**（审查报告、协作任务包、节点账本、ADR）：用 `git show 519d4d6:<路径>` 查看。
- **原始审计证据**（日志、覆盖率导出、变异补丁）：不在仓库中，保存在本机 `协作工作-过程归档/` 目录。
