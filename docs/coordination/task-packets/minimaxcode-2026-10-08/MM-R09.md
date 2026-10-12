# MM-R09｜整理当前可复核验收与文档补充

- 关联节点：DEV-01-05（关联01-08、02-01/11）
- 状态：离线资料/文档可执行；工具下载与在线Gate待授权。
- 主责：Human 指定的 Minimaxcode；启动回填实际 Harness、模型/版本、额度/新增费用与共享路径；不猜测 M3。主控/独立终审：Codex。
- 难度：R0 文档 / R3 验收。R3 必须保留独立反例、失败注入和 Codex 逐差异复核。
- 授权：Human 2026-10-08 接续指令及 ADR-0008；具体写集以本包为准。
- 前置及范围：与R06串行，R01～R05完成后取得最终证据；修复完成但依赖未通过仍BLOCKED。
- 仓库：`/Users/chenzhiqing/Documents/MyVault/协作工作`；HEAD `1a14b8795e76fc373bef326eb5df96f9ac173f71` 加继承未提交工作树。
- 审计起点输入 SHA：`8845a053b778b67aeac7a2138ebeaa16432781572cdb2232fa085c30816de37f`（299个范围文件，不是后续任务执行时的最终输入）；实际启动重新固定当前输入并登记差异。

## 必读与允许写集

按 AGENTS.md → AGENTS.override.md → docs/development/README.md、TECH_STACK_AND_STANDARDS.md、DEVELOPMENT_NODES.md、QUALITY_GATES.md → specs/README.md → 对应全部冻结规格及ADVERSARIAL_REVIEW_PLAYBOOK → ADR0005/0006/0008 → DEEP-AUDIT-2026-10-08.md 的顺序完整阅读。ADR0007为Proposed。

允许修改：

- `docs/coordination/minimax-return/MM-R09-acceptance-index.md`
- `crates/domain/src/lib.rs`
- `crates/domain/src/task_state_machine.rs`

另允许新增本包的 `docs/coordination/minimax-return/MM-R09-RESULT.md` 与 `docs/reviews/evidence/minimax-return/MM-R09/`（不得覆盖旧产物）。未列文件只读；需要新增依赖、共享文件或更大写集，先回传具体提案供总控整合。

禁止修改 sources/、旧migration、历史报告/evidence、生成TS（应由Rust源生成）、根manifest/lockfile、正式节点状态/ADR接受状态；不得reset继承内容、提交/推送/部署、调用外部服务/下载或操作生产凭据/数据。新增费用$0，外部操作须Human另行确认。固定技术栈、领域状态、wire字段/错误码不改变。

## 目标和 RED 输入

归档每包输入/输出/命令/exit/RED/GREEN/作用域；修正random[1]与前置校验顺序的纯注释。给缺失历史JSON/旧provenance/归档死链新映射与当前替代证据，禁止伪造旧件或覆写历史。使用本批coverage-tools/run_domain_coverage.py的方法，在新的证据目录复制当前完整源码/测试并调整repro/target/profile/output，重新固定输入测量，针对86.80%低覆盖模块提有意义的测试补充任务；补测试需独立精确写集，未经确认不安装新工具。不得把region当branch。

先复现：审计已证明UUID被variant屏蔽的是random[2]；Task变更应先验证再条件写入；旧TS总数错误已由Codex纠正，不归MiniMax。

验收：文档改动仅注释；已验证/未运行/失败/缓存分别标注；domain当前86.80%未达90；分支未测不写85%，测试数从日志自动汇总；节点状态由Codex裁定。

顺序：不提交/推送/部署；旧证据只读，缺失旧JSON永久标缺失，新报告必须引用新输入。

## 验收命令与回传

```text
cargo fmt --all -- --check
```

```text
RUSTDOCFLAGS="-D warnings" cargo doc -p teamspace-domain --no-deps --locked --offline
```

```text
最终统一offline checks，完整在线Mac21+配套证据须先获独立授权
```

代码包完成后还需匹配变更的 tests/build/clippy/fmt/strict rustdoc/TS检查，主控再运行最终统一门禁。不要用 cargo fmt --all 写入无关文件；只格式化本包文件。仅运行--offline/已有本地依赖的必要命令，离线通过不是完整在线Mac gate。

回传一个 RESULT Markdown：实际模型/环境；输入与输出SHA；修改文件与逐差异说明；命令、退出码、完整日志；真实RED/GREEN与合法对照；失败/升级回滚/兼容；未运行项及理由；SELF_CHECK_PASS / SELF_CHECK_BLOCKED；剩余风险和回退。自验收不等于DONE，不启动依赖包。

停止条件：规格冲突、新状态/权限/错误码/协议含义、模型/输入不明、写集冲突、无法证明正对照、S0/S1未闭环、外部权限/费用影响或未满足新节点依赖。保留证据并回传，不临时关外键/守卫、改规格/预期、返回固定allow或用假数据保绿。
