# MM-R02｜补齐核心历史留存与错误测试期望

- 关联节点：DEV-02-05（关联02-06/08/10）
- 状态：既有缺陷修复可执行。
- 主责：Human 指定的 Minimaxcode；启动回填实际 Harness、模型/版本、额度/新增费用与共享路径；不猜测 M3。主控/独立终审：Codex。
- 难度：R3 / P1。R3 必须保留独立反例、失败注入和 Codex 逐差异复核。
- 授权：Human 2026-10-08 接续指令及 ADR-0008；具体写集以本包为准。
- 前置及范围：在 R01 注册完成后串行；这些是既有节点整改，不启动新节点。
- 仓库：`/Users/chenzhiqing/Documents/MyVault/协作工作`；HEAD `1a14b8795e76fc373bef326eb5df96f9ac173f71` 加继承未提交工作树。
- 审计起点输入 SHA：`8845a053b778b67aeac7a2138ebeaa16432781572cdb2232fa085c30816de37f`（299个范围文件，不是后续任务执行时的最终输入）；实际启动重新固定当前输入并登记差异。

## 必读与允许写集

按 AGENTS.md → AGENTS.override.md → docs/development/README.md、TECH_STACK_AND_STANDARDS.md、DEVELOPMENT_NODES.md、QUALITY_GATES.md → specs/README.md → 对应全部冻结规格及ADVERSARIAL_REVIEW_PLAYBOOK → ADR0005/0006/0008 → DEEP-AUDIT-2026-10-08.md 的顺序完整阅读。ADR0007为Proposed。

允许修改：

- `crates/persistence-sqlite/migrations/0026_core_history_retention.sql`
- `crates/persistence-sqlite/src/lib.rs`
- `crates/persistence-sqlite/tests/core_history_retention.rs`
- `crates/persistence-sqlite/tests/permission_approval_integrity_forward_migration.rs`

另允许新增本包的 `docs/coordination/minimax-return/MM-R02-RESULT.md` 与 `docs/reviews/evidence/minimax-return/MM-R02/`（不得覆盖旧产物）。未列文件只读；需要新增依赖、共享文件或更大写集，先回传具体提案供总控整合。

禁止修改 sources/、旧migration、历史报告/evidence、生成TS（应由Rust源生成）、根manifest/lockfile、正式节点状态/ADR接受状态；不得reset继承内容、提交/推送/部署、调用外部服务/下载或操作生产凭据/数据。新增费用$0，外部操作须Human另行确认。固定技术栈、领域状态、wire字段/错误码不改变。

## 目标和 RED 输入

保护 Run、Review 决定、Artifact 索引、Message 和 Approval 的持久化历史，覆盖无 Event/Reference 引用的旧版本/孤立行；已有 Event/Reference delete/replace 防线保留。PENDING Approval 取消走现有 CANCELLED+actor，不能物理删请求。Attention 不列入本包永久核心历史集合。

先复现：提取 retained_histories_cannot_be_physically_deleted：五表均实际1→0。检查0020现有测试把PENDING DELETE当合法的期望，依据DATA§5～6纠正；同时证明有Event/Reference时原守卫继续有效。

验收：原始/升级历史全保留；五表 DELETE/REPLACE/引用变化的适用反例覆盖，不以FK碰巧拒绝或0行更新计分；取消、终结、版本更新仍可进行；升级/失败零副作用。不可恢复已删的数据，不能补造历史Event。

顺序：R01 → R02 → R03，三包均修改lib.rs，只能串行。

## 验收命令与回传

```text
cargo test -p persistence-sqlite --test core_history_retention --test permission_approval_integrity_forward_migration --locked --offline
```

代码包完成后还需匹配变更的 tests/build/clippy/fmt/strict rustdoc/TS检查，主控再运行最终统一门禁。不要用 cargo fmt --all 写入无关文件；只格式化本包文件。仅运行--offline/已有本地依赖的必要命令，离线通过不是完整在线Mac gate。

回传一个 RESULT Markdown：实际模型/环境；输入与输出SHA；修改文件与逐差异说明；命令、退出码、完整日志；真实RED/GREEN与合法对照；失败/升级回滚/兼容；未运行项及理由；SELF_CHECK_PASS / SELF_CHECK_BLOCKED；剩余风险和回退。自验收不等于DONE，不启动依赖包。

停止条件：规格冲突、新状态/权限/错误码/协议含义、模型/输入不明、写集冲突、无法证明正对照、S0/S1未闭环、外部权限/费用影响或未满足新节点依赖。保留证据并回传，不临时关外键/守卫、改规格/预期、返回固定allow或用假数据保绿。
