# 全项目任务冻结清单

> 2026-10-10 07终审最新：按冻结专属标准完成[责任核对与单节点终审](../reviews/DEV-02-07-SCOPE-CLOSURE-REVIEW-2026-10-10.md)，07基础节点恢复DONE；正式 **34 DONE / 7 BLOCKED / 60 PLANNED**。58专项/七项真实反例、当前stable Mac21整体PASS，Rust696/0/1、TS36/0，219项工程输入不变。原先笼统列入07的后续格式扩展、dispatcher和host接线已按冻结依赖归还02-14/05/07后续验收，不新增语义或启动下游。Gate C仍阻塞；下列07 BLOCKED和33/8/60等段落为历史时点。


> 2026-10-10 分支补测最终：四个Application遗漏结果已覆盖，Domain行91.67%/branch93.36%，Application行93.75%/branch100%；当前受检输入stable Mac21整体PASS，Rust696/0/1、TS36/0。依据[本轮验收及单节点复核](../reviews/DEV-02-07-BRANCH-TEST-REVIEW-2026-10-10.md)，仅DEV-02-11恢复DONE，正式 **33 DONE / 8 BLOCKED / 60 PLANNED**。07整节点与Gate C继续阻塞；生产源码、迁移、manifest/lock和用户stable/default不变，工程仅补测试。下列旧32/9/60、branch未证或77.78%均为历史时点。


> 2026-10-10 最新执行：DEV-02-07 原子消费/去重、投影重建及三处真实进程崩溃重放已在限定范围通过，[验收](../reviews/DEV-02-07-AUDIT-CONSUMER-REVIEW-2026-10-10.md)为当前 canonical Mac21 PASS，Rust692/0/1、TS36/0。当前迁移0001～0032；行覆盖率Domain91.90%、Application93.41%，branch仍未证。正式 **32 DONE / 9 BLOCKED / 60 PLANNED** 不变；dispatcher、广义历史格式/host装配、通知和完整阶段未准入。下列旧统计和迁移范围按历史时点保留。

> 本轮最新执行：DEV-02-07 内部 Message 到期事务子包已通过，[验收](../reviews/DEV-02-07-ATOMIC-EXPIRY-REVIEW-2026-10-09.md)为 canonical Mac21 整体 PASS、Rust674/0/1、TS36/0，13项应用真库测试与3项行为反例通过。当前迁移0001～0031；application不再仅为占位，但consumer/dispatcher、可信Human/Role权限、PENDING到期语义与覆盖率仍未完成。正式 **32 DONE / 9 BLOCKED / 60 PLANNED** 不变；旧段落数字和迁移范围按历史时点保留。

> C6后的获准整改执行：DEV-02-08新增前向0030及存量/回滚/留存验收通过，见[专项验收](../reviews/DEV-02-08-ROUTE-SCHEMA-REVIEW-2026-10-09.md)。实际Mac21整体PASS不关闭08应用执行链，正式32/9/60保持；不开放V2、发布或生产操作。

> 2026-10-09 C6准入最新覆盖：按[逐节点审查](../reviews/C6-C7-ADMISSION-REVIEW-2026-10-09.md)，DEV-01-05/08、DEV-02-02～06/10、DEV-03-02～05共12个底座节点恢复DONE；正式 **32 DONE / 9 BLOCKED / 60 PLANNED**。当前Mac21整体PASS；仍仅按已授权收口及具体节点依赖推进，未开放V2新语义、生产或发布。下列20/21/60及旧缺陷/外派包为历史快照，Minimax继续暂停。

> 生效：2026-09-28，依据 Human 本次“冻结所有任务”指令及已接受的 [ADR-0005](../adr/0005-macos-first-quality-and-release-gates.md)。本清单冻结**新派工、实现写入和阶段晋级**；它不是新的 `DEV-*` 生命周期状态，不改写[节点表](../development/DEVELOPMENT_NODES.md)中的正式状态。

## 2026-10-07 当前复核覆盖

> 2026-10-09 Codex内部收口最新覆盖：Minimaxcode外派暂停；本轮整改与完整canonical Mac21整体PASS，旧FC04脱敏及AF进程fixture缺口已在限定范围关闭。当前正式 **20 DONE / 21 BLOCKED / 60 PLANNED** 不变，未开放V2新语义或后续依赖。执行[进度与计划](../coordination/PROGRESS_AND_NEXT_PLAN-2026-10-09.md)及[内部接续安排](../coordination/CODEX-AUDIT-CLOSURE-PLAN-2026-10-09.md)；下列旧FC三包/AF两包及Minimax任务表保留历史，不再作为本轮外派或待修指令。当前事实以[收口审计](../reviews/CODEX-AUDIT-CLOSURE-2026-10-09.md)为准。

> 2026-10-09最新覆盖：[FC九包审计](../reviews/FC-RETURN-AUDIT-2026-10-09.md)局部确认六对象留存/Run终态修复，现有离线套件全绿不等于准入，FC04仍BLOCKED。状态20 DONE / 21 BLOCKED / 60 PLANNED；只执行[三个精确整改包](../coordination/MINIMAXCODE-AUDIT-FOLLOWUP-2026-10-09.md)。ADR0007、完整Mac21及后续依赖未开放。

> 2026-10-08 Minimax回传独立审核覆盖：DEV-02-02/03/04因Team/Role/Task/Binding留存缺口重开BLOCKED；现行 **20 DONE / 21 BLOCKED / 60 PLANNED**，以下23/18为上轮快照。按[交付审计](../reviews/MINIMAX-RETURN-AUDIT-2026-10-08.md)和[补充整改包](../coordination/MINIMAXCODE-FOLLOWUP-2026-10-08.md)执行，历史DONE不放行，未决语义与上线未开放。

Human 已授权总控制定并落实 review 整改；精确任务包见 [整改方案](../coordination/REMEDIATION-PLAN-2026-10-07.md)。此授权开放已冻结语义内的对应缺陷修复，不开放 V2 或未满足依赖的新产品实现。

DEV-02-08/09/13/14、DEV-03-06/07 当前为 **BLOCKED**。下表原 DONE 行是09-29～30历史交付摘要，不再作为当前准入；现行状态以节点表及10-07复核为准。独立验收和当前最终输入门禁通过后才可恢复，不能把纯 helper 或局部迁移的回归当成运行时节点完成。

2026-10-08 补充：DEV-02-07 当前也为 **BLOCKED**。前向 0024 的默认重试值局部整改通过，当前门禁和应用原子执行链仍未验收；见[验收报告](../reviews/OUTBOX-DEFAULT-REMEDIATION-2026-10-08.md)。下表历史摘要保持，不作为该节点当前依赖准入。

2026-10-08 深入审计：现行全表为23 DONE / 18 BLOCKED / 60 PLANNED，新增阻断及其依赖暂停见[深入审计](../reviews/DEEP-AUDIT-2026-10-08.md)和节点表。Human已选择Minimaxcode接续签发的既有整改包（ADR-0008），[总交接表](../coordination/MINIMAXCODE-TASKS-2026-10-08.md)的60个未来节点仅条件排期；不以模型路由更新开放V2、新产品实现、发布或未确认语义。

## 冻结范围与当前事实

| 范围 | 正式状态 / 已有证据 | 冻结期间允许事项 | 解冻或晋级条件 |
|---|---|---|---|
| DEV-00、Wave 1、DEV-01-05/08/09 | 历史 `DONE`；旧输入证据保留 | 只读追溯；不追溯改写结论 | 后续节点仍须按 ADR-0005 提供当前最终输入的 Mac 证据 |
| DEV-02-02、DEV-03-02 | 首轮曾为 `REVIEW`/`BLOCKED`；[前置节点终审](../reviews/WAVE-2-ENTRY-CLOSURE-2026-09-28.md)在最终输入门禁通过后关闭为 `DONE` | 只读追溯与后续变更的受影响回归 | 后续输入变化须重新取得 Mac 门禁证据；不回写首轮失败记录 |
| DEV-02-03/04、DEV-03-03 | [第一批节点终审](../reviews/WAVE-2-BATCH-1-CLOSURE-2026-09-28.md)的专项独立审核 PASS；最终输入门禁通过后为 `DONE` | 只读追溯与后续变更回归 | 证据随输入变化失效；不以本批结果放行未满足依赖的节点 |
| DEV-02-05、DEV-03-04 | [第二批节点终审](../reviews/WAVE-2-BATCH-2-CLOSURE-2026-09-28.md)专项独立审核 PASS；最终输入门禁通过后为 `DONE` | 只读追溯与后续变更回归 | 证据随输入变化失效；不以本批结果放行未满足依赖的节点 |
| DEV-02-06、DEV-03-05 | [第三批节点终审](../reviews/WAVE-2-BATCH-3-CLOSURE-2026-09-28.md)专项独立审核 PASS；最终输入门禁通过后为 `DONE` | 只读追溯与后续变更回归 | 证据随输入变化失效；不以本批结果放行未满足依赖的节点 |
| DEV-02-07～14 | 节点表已于 2026-09-29～30 逐个关闭为 `DONE`（见[节点表](../development/DEVELOPMENT_NODES.md)与各节点终审；DEV-02-11 三机状态机见[此](../reviews/WAVE-2-DEV-02-11-STATE-MACHINES-CLOSURE-2026-09-29.md)，DEV-02-12 Message/Delivery/Approval 状态机见[此](../reviews/WAVE-2-DEV-02-12-STATE-MACHINES-CLOSURE-2026-09-30.md)，DEV-02-13 权限矩阵与八层求交见[此](../reviews/WAVE-2-DEV-02-13-PERMISSION-INTERSECTION-CLOSURE-2026-09-30.md)，DEV-02-14 乐观锁/幂等/Outbox 重试见[此](../reviews/WAVE-2-DEV-02-14-CONCURRENCY-IDEMPOTENCY-OUTBOX-CLOSURE-2026-09-30.md)） | 只读追溯与后续变更的受影响回归 | 证据随输入变化失效；后续节点仍须满足自身前置 `DONE`、专属标准、G0～G9 与当前输入 Mac 门禁，不得以本批结果放行 |
| DEV-03-06 | 节点表已关闭为 `DONE`：§3 取消/超时纯判定与 §11 重试准入，见[此](../reviews/WAVE-2-DEV-03-06-TERMINATION-RETRY-CLOSURE-2026-09-30.md) | 只读追溯与后续变更的受影响回归 | 证据随输入变化失效；DEV-03-07～11 仍须各自满足前置 `DONE` 与专属标准，不得以本节点结果放行 |
| DEV-03-07 | 节点表已关闭为 `DONE`：§5 租约续期与 fencing 判定，见[此](../reviews/WAVE-2-DEV-03-07-FENCING-CLOSURE-2026-09-30.md) | 只读追溯与后续变更的受影响回归 | 证据随输入变化失效；DEV-03-08～11 仍须各自满足前置 `DONE` 与专属标准，不得以本节点结果放行 |
| 其余 DEV-01～08 未完成节点 | 节点表为 `PLANNED` 或依其现行记录 | 只读查询与风险登记 | Wave 2 及对应前置节点正式关闭、单节点任务包签发；发布另经 Gate I 与 Human 决定 |
| V2 R1 | 需求/架构提案，尚无获接受的 V2 ADR 和新 `DEV-*` 图 | 已有草案只读评议；不写入新产品语义 | Human 接受新/替代 ADR，冻结 AC、规格、迁移、节点与模型路由 |
| V2 R2～R6、Figma 修改 | 仅为[阶段分工](WORKSTREAMS_AND_ACCEPTANCE.md)候选 | 只读参考 | R1 决策、依赖、具体任务包及外部服务授权满足后逐项开启 |
| 外部 Harness 与 Codex Subagent 的新实现任务 | 当前没有已签发的单节点**实现**任务包；首轮只读审核分工见验收记录 | 不创建新实现派工，不给共享写集 | 总控写明实际模型、额度、`DEV-*`、互斥写集、验收命令和审核者后逐项解冻 |

此次冻结不撤销已有文件、不把 `PLANNED` 批量改为 `BLOCKED`，也不把 `SELF_CHECK_PASS` 转成 `DONE`。已存在但未经正式准入的 Wave 2 实现是**待审输入**，不是继续开发的许可。`sources/`、归档原件、历史审核和其他会话的在途改动均保持原样。

V2 基线中的候选编号也全部纳入冻结：`CODEX-001` 已产出只读 Rescan，不能被当成 R1 决策完成；`CODEX-002～006`（架构差异、Harness Interface、Conversation 数据、Leader 权限、MCP Schema）暂停新工作；`MM-001～006`（Conversation/ProviderSession/Gateway/ACP/Router/MCP）和 `AG-001～006`（Conversation/Role/Leader/Task/Run UI 与浏览器 E2E）均未签发为正式 `DEV-*` 任务。它们没有独立开工权，解冻顺序分别归入 R1、R2～R4、R5～R6，须以新节点图和单节点任务包为准。

## 唯一准备通道：Wave 2 正式验收

Human 已启动 [Wave 2 macOS 正式验收](../coordination/WAVE2_MAC_ACCEPTANCE_PLAN.md)；[首轮结论](../reviews/WAVE-2-MAC-ACCEPTANCE.md)为未通过，[前置节点终审](../reviews/WAVE-2-ENTRY-CLOSURE-2026-09-28.md)按当前修复另行判定。此通道开放已满足依赖节点的独立审核、证据采集和精确任务包下的必要修复，不自动解冻其他任务。任何修复改变输入后，相关定向证据和最终 Mac 门禁必须重跑。原实现者不能自审；未满足条件的节点不得穿透依赖。

## 变更控制

改变任务范围、模型主责、允许写集、节点依赖、冻结规格或验收门，须先记录差异并按其性质取得 Human 对 ADR/新任务范围的明确决策。Mac 是当前唯一阻断性平台；Linux、Oracle、Docker 与 Windows 均不进入本次验证范围，历史资产仍可只读参考。新增费用上限 `$0`；外部账号、连接、上传、收费和 Figma 写操作另按项目规则确认。
