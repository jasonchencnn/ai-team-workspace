# AI Teamspace 项目文档入口

> 更新：2026-10-09。本文只整理入口与权威边界，不批准新的架构、权限或协议语义。

| 阅读目的 | 入口 | 当前效力 |
|---|---|---|
| 2026-10-10 07基础节点终审 | [执行计划](coordination/DEV-02-07-SCOPE-CLOSURE-PLAN-2026-10-10.md)、[责任核对与终审](reviews/DEV-02-07-SCOPE-CLOSURE-REVIEW-2026-10-10.md) | 07三条冻结标准、58专项/七项反例、当前Mac21通过；219工程输入不变，仅恢复07 DONE，正式34/7/60。后续格式、dispatcher、host产品链与Gate C未准入；下方旧07 BLOCKED等为历史时点。 |
| 2026-10-10 Application补测与11重签 | [限定计划](coordination/DEV-02-07-BRANCH-TEST-PACKET-2026-10-10.md)、[最终验收](reviews/DEV-02-07-BRANCH-TEST-REVIEW-2026-10-10.md) | 四项补测及反向变异通过，Application行93.75%/branch100%，Domain行91.67%/branch93.36%；stable Mac21 PASS，Rust696/0/1、TS36/0。仅DEV-02-11恢复DONE，正式33/8/60；07整节点与Gate C仍阻塞。下方旧branch与32/9/60为历史时点。 |
| 2026-10-10 C7真实分支测量 | [已接受ADR-0009](adr/0009-isolated-branch-coverage-tool-proposal.md)、[执行计划](coordination/C7-BRANCH-COVERAGE-PLAN-2026-10-10.md)、[专项验收](reviews/C7-BRANCH-COVERAGE-REVIEW-2026-10-10.md) | 官方核验、隔离安装与单路/双路探针PASS；Domain行91.67%/分支93.36%，Application行91.25%/分支77.78%。Application分支未达85%，Gate C与32/9/60不晋级；生产源码、lock和用户stable/default不变。以下候选未安装等条目为历史快照。 |
| 2026-10-10 DEV-02-07 消费与恢复 | [执行包](coordination/DEV-02-07-AUDIT-CONSUMER-PACKET-2026-10-09.md)、[验收](reviews/DEV-02-07-AUDIT-CONSUMER-REVIEW-2026-10-10.md)、[分支工具提案](adr/0009-isolated-branch-coverage-tool-proposal.md) | 原子投影/去重与三处实际崩溃重放通过，Mac21 PASS、Rust692/0/1、TS36/0。迁移0032；Domain/App行91.90%/93.41%，branch未证；32/9/60不变。ADR-0009为Proposed，未安装候选。 |
| DEV-02-07 原子事务整改 | [执行包](coordination/DEV-02-07-ATOMIC-EXPIRY-PACKET-2026-10-09.md)、[验收](reviews/DEV-02-07-ATOMIC-EXPIRY-REVIEW-2026-10-09.md) | 内部到期业务/Event/Outbox/首结果同事务；13项真库测试、3项行为反例及当前Mac21通过，Rust674/0/1、TS36/0。迁移至0031；07/08完整节点仍BLOCKED，正式32/9/60不变；下列旧输入数字为历史证据。 |
| DEV-02-08 改派历史存储整改 | [0030验收](reviews/DEV-02-08-ROUTE-SCHEMA-REVIEW-2026-10-09.md)、[实施包](coordination/DEV-02-08-ROUTE-MIGRATION-PACKET-2026-10-09.md) | 前向0030及8项存量/事务/留存测试通过；最终Mac21整体PASS，Rust661/0/1、TS36/0。应用身份/权限/投递链仍未完成，正式32 DONE / 9 BLOCKED / 60 PLANNED不变。 |
| 2026-10-09 C6逐节点准入与C7工具调查 | [准入审查](reviews/C6-C7-ADMISSION-REVIEW-2026-10-09.md) | 当前输入Mac21整体PASS，12个底座节点逐项重签；正式32 DONE / 9 BLOCKED / 60 PLANNED。C6已完成，下一步真实Event/Outbox事务与改派历史；branch、ADR-0007及产品阶段仍有准入条件。 |
| 当前工作进度与后续开发计划 | [进度与计划](coordination/PROGRESS_AND_NEXT_PLAN-2026-10-09.md) | 32 DONE / 9 BLOCKED / 60 PLANNED；C6完成并逐项重签12个底座节点，产品执行链待接通。下一步Event/Outbox事务、改派历史与branch方案；ADR-0007 / V2 / 真实环境仍有准入条件。 |
| 2026-10-09 Codex内部审计收口 | [内部计划](coordination/CODEX-AUDIT-CLOSURE-PLAN-2026-10-09.md)、[收口审计](reviews/CODEX-AUDIT-CLOSURE-2026-10-09.md) | 本轮整改及完整canonical Mac21整体PASS：21/21与前后companion匹配，Rust653/0/1、TS36/0，Gate/replay34/0；高危依赖补丁、crash及日志时钟反例闭环。Minimax外派暂停；branch/真实产品链/设备等仍待验，正式20/21/60不变。 |
| 2026-10-09 AF三包历史验收 | [AF审计](reviews/AF-RETURN-AUDIT-2026-10-09.md)、[精确接续两包](coordination/MINIMAXCODE-VERIFICATION-FOLLOWUP-2026-10-09.md) | AF01漏检修复与AF02存量概念实验通过限定审验；串行Rust653/0/1，默认并行652/1/1，Drop就绪证据待补。精确秘密扫描例外已由主控整合；正式20 DONE / 21 BLOCKED / 60 PLANNED不变。 |
| 2026-10-09 FC九包历史审核 | [FC审计](reviews/FC-RETURN-AUDIT-2026-10-09.md)、[下一轮三个整改包](coordination/MINIMAXCODE-AUDIT-FOLLOWUP-2026-10-09.md) | 现有离线套件全绿，FC04仍有新反例；M14原15+1、新16/16分列；FC09已整合。当前20 DONE / 21 BLOCKED / 60 PLANNED，未放行依赖。 |
| 2026-10-08 Minimax十包交付终审入口 | [交付审计](reviews/MINIMAX-RETURN-AUDIT-2026-10-08.md)、[补充任务表](coordination/MINIMAXCODE-FOLLOWUP-2026-10-08.md) | 本批不准入，20 DONE / 21 BLOCKED / 60 PLANNED；只整改，未放行依赖或ADR-0007语义。 |
| 2026-10-08 深入审计与接续任务 | [深入审计](reviews/DEEP-AUDIT-2026-10-08.md)、[Minimaxcode总交接表](coordination/MINIMAXCODE-TASKS-2026-10-08.md)、[ADR-0008路由记录](adr/0008-minimaxcode-remediation-routing.md) | 2026-10-08历史审计结论FAIL；整改进展以最新Codex收口报告为准。原外派任务暂停，正式依赖与ADR-0007未放行。 |
| 2026-10-07 当前整改 | [整改方案](coordination/REMEDIATION-PLAN-2026-10-07.md)、[修改验收](reviews/REMEDIATION-REVIEW-2026-10-07.md) | 已授权的旧review缺陷修复；相关完整节点仍BLOCKED，不放行V2或后续依赖。 |
| V2 产品方向 | [项目重启基线](../TEAMSPACE_V2_项目重启基线与Harness分工.md) | Human 提出的目标方向；与已冻结规则冲突处须经新/替代 ADR 接受后实施。 |
| 现状与差异 | [Project Rescan Report](../PROJECT_RESCAN_REPORT.md) | R0 只读审计结论，不是节点 `DONE` 证据。 |
| 当前分工与阶段门 | [V2 阶段分工和交付验收](v2/WORKSTREAMS_AND_ACCEPTANCE.md) | 拟议执行计划；R1 以后任务在 ADR/节点依赖获准前不得开工。 |
| 全项目任务冻结 | [冻结清单](v2/TASK_FREEZE.md) | Wave 2 前置阻断的精确修复与验收已获授权；其余任务按依赖继续冻结。 |
| Wave 2 正式验收 | [macOS 验收方案](coordination/WAVE2_MAC_ACCEPTANCE_PLAN.md)、[首轮验收记录](reviews/WAVE-2-MAC-ACCEPTANCE.md)、[前置阻断修复复验](reviews/WAVE-2-PREBLOCKER-REPAIR.md) | 首轮未通过；Mock 脱组进程定向反例已独立通过，完整 Mac 门禁与总控终审仍阻断依赖节点。 |
| 当前协作状态与任务包 | [协作入口](coordination/README.md)、[任务包模板](coordination/TASK_PACKET_TEMPLATE.md) | 状态、写集和交接的现行入口。 |
| 仍然冻结的工程规则 | [开发基线](development/README.md)、[规格索引](specs/README.md)、[ADR-0001～0005](adr/)、[ADR-0006 脱组进程决策](adr/0006-macos-adapter-detached-descendant-policy-proposal.md)、[对抗审查手册](development/ADVERSARIAL_REVIEW_PLAYBOOK.md) | ADR-0006 已按 Human 的技术决策授权接受；DEV-03-02 仍须以真实测试和门禁关闭阻断。对抗审查手册只约束证据可信度，不新增任何产品语义。 |
| 审核证据 | [节点审核](reviews/)、[归档索引](archive/README.md) | `SELF_CHECK_PASS` 与正式 `DONE` 分开；历史快照证据不能证明当前输入。 |
| V1 需求与设计依据 | [旧 PRD](../多-Harness-AI-开发团队协作平台-PRD.md)、[设计方案](../design/界面设计方案.md)、[Figma 索引](../design/figma-state.json) | 冻结基线仍引用的旧依据；V2 差异按 Human 决策和 ADR 处理。 |

事实优先级：Human 最新明确决策与已接受 ADR → 旧 PRD 中尚未被替代的已确认原则/AC → 冻结开发基线与规格 → 有效审计调整 → Figma 体验/视觉 → 历史计划与演示。文件移入 `archive/` 不改变已接受 ADR、历史证据或未完成节点状态。

Rescan 报告及旧节点审核中提到的双环境阻断是当时的结论；当前平台门禁以 ADR-0005 和[协作入口](coordination/README.md)为准。
