# MM-R03｜限制旧 Run 的非法状态倒退

- 关联节点：DEV-02-05
- 状态：既有缺陷修复可执行。
- 主责：Human 指定的 Minimaxcode；启动回填实际 Harness、模型/版本、额度/新增费用与共享路径；不猜测 M3。主控/独立终审：Codex。
- 难度：R3 / P1。R3 必须保留独立反例、失败注入和 Codex 逐差异复核。
- 授权：Human 2026-10-08 接续指令及 ADR-0008；具体写集以本包为准。
- 前置及范围：R02 已整合；DEV-02-04 基线有效，节点仍 BLOCKED。
- 仓库：`/Users/chenzhiqing/Documents/MyVault/协作工作`；HEAD `1a14b8795e76fc373bef326eb5df96f9ac173f71` 加继承未提交工作树。
- 审计起点输入 SHA：`8845a053b778b67aeac7a2138ebeaa16432781572cdb2232fa085c30816de37f`（299个范围文件，不是后续任务执行时的最终输入）；实际启动重新固定当前输入并登记差异。

## 必读与允许写集

按 AGENTS.md → AGENTS.override.md → docs/development/README.md、TECH_STACK_AND_STANDARDS.md、DEVELOPMENT_NODES.md、QUALITY_GATES.md → specs/README.md → 对应全部冻结规格及ADVERSARIAL_REVIEW_PLAYBOOK → ADR0005/0006/0008 → DEEP-AUDIT-2026-10-08.md 的顺序完整阅读。ADR0007为Proposed。

允许修改：

- `crates/persistence-sqlite/migrations/0027_run_transition_integrity.sql`
- `crates/persistence-sqlite/src/lib.rs`
- `crates/persistence-sqlite/tests/run_transition_integrity.rs`
- `crates/persistence-sqlite/tests/run_lease_consistency_forward_migration.rs`

另允许新增本包的 `docs/coordination/minimax-return/MM-R03-RESULT.md` 与 `docs/reviews/evidence/minimax-return/MM-R03/`（不得覆盖旧产物）。未列文件只读；需要新增依赖、共享文件或更大写集，先回传具体提案供总控整合。

禁止修改 sources/、旧migration、历史报告/evidence、生成TS（应由Rust源生成）、根manifest/lockfile、正式节点状态/ADR接受状态；不得reset继承内容、提交/推送/部署、调用外部服务/下载或操作生产凭据/数据。新增费用$0，外部操作须Human另行确认。固定技术栈、领域状态、wire字段/错误码不改变。

## 目标和 RED 输入

把已冻结 Run 有向迁移与旧attempt不可重置落在当前存储边界；RUNNING不能回QUEUED/STARTING，重试不修改旧Run。允许合法UNKNOWN对账边及无状态变化的业务字段更新。

先复现：提取 existing_running_attempt_cannot_return_to_queued：旧RUNNING attempt2/token2可被改回QUEUED并清started_at。先保留UNKNOWN→RUNNING正常对照。

验收：正负状态矩阵由冻结表核对；旧token/attempt/终态历史守卫不弱化；创建轴也评估并登记，若加QUEUED入口限制需合法历史升级/fixture方案，不能加测试专用逃生开关或改旧migration。

顺序：只做数据库状态守卫，不以它宣称取消/重试/当前fencing应用链完成。

## 验收命令与回传

```text
cargo test -p persistence-sqlite --test run_transition_integrity --test run_lease_consistency_forward_migration --locked --offline
```

代码包完成后还需匹配变更的 tests/build/clippy/fmt/strict rustdoc/TS检查，主控再运行最终统一门禁。不要用 cargo fmt --all 写入无关文件；只格式化本包文件。仅运行--offline/已有本地依赖的必要命令，离线通过不是完整在线Mac gate。

回传一个 RESULT Markdown：实际模型/环境；输入与输出SHA；修改文件与逐差异说明；命令、退出码、完整日志；真实RED/GREEN与合法对照；失败/升级回滚/兼容；未运行项及理由；SELF_CHECK_PASS / SELF_CHECK_BLOCKED；剩余风险和回退。自验收不等于DONE，不启动依赖包。

停止条件：规格冲突、新状态/权限/错误码/协议含义、模型/输入不明、写集冲突、无法证明正对照、S0/S1未闭环、外部权限/费用影响或未满足新节点依赖。保留证据并回传，不临时关外键/守卫、改规格/预期、返回固定allow或用假数据保绿。
