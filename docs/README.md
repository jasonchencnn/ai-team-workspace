# AI Teamspace 项目文档入口

> 更新：2026-09-28。本文只整理入口与权威边界，不批准新的架构、权限或协议语义。

| 阅读目的 | 入口 | 当前效力 |
|---|---|---|
| 2026-10-09 Codex内部审计收口 | [内部计划](coordination/CODEX-AUDIT-CLOSURE-PLAN-2026-10-09.md)、[收口审计](reviews/CODEX-AUDIT-CLOSURE-2026-10-09.md) | 本轮offline整改审验PASS：Rust默认/串行653/0/1，稳定性10+3轮、两独立变异及19扫描范围对照通过。Minimax外派暂停；完整Mac21/branch/产品链等仍待验，正式20/21/60不变。 |
| 2026-10-09 AF三包历史验收 | [AF审计](reviews/AF-RETURN-AUDIT-2026-10-09.md)、[精确接续两包](coordination/MINIMAXCODE-VERIFICATION-FOLLOWUP-2026-10-09.md) | AF01漏检修复与AF02存量概念实验通过限定审验；串行Rust653/0/1，默认并行652/1/1，Drop就绪证据待补。精确秘密扫描例外已由主控整合；正式20 DONE / 21 BLOCKED / 60 PLANNED不变。 |
| 2026-10-09 FC九包历史审核 | [FC审计](reviews/FC-RETURN-AUDIT-2026-10-09.md)、[下一轮三个整改包](coordination/MINIMAXCODE-AUDIT-FOLLOWUP-2026-10-09.md) | 现有离线套件全绿，FC04仍有新反例；M14原15+1、新16/16分列；FC09已整合。当前20 DONE / 21 BLOCKED / 60 PLANNED，未放行依赖。 |
| 2026-10-08 Minimax十包交付终审入口 | [交付审计](reviews/MINIMAX-RETURN-AUDIT-2026-10-08.md)、[补充任务表](coordination/MINIMAXCODE-FOLLOWUP-2026-10-08.md) | 本批不准入，20 DONE / 21 BLOCKED / 60 PLANNED；只整改，未放行依赖或ADR-0007语义。 |
| 2026-10-08 深入审计与接续任务 | [深入审计](reviews/DEEP-AUDIT-2026-10-08.md)、[Minimaxcode总交接表](coordination/MINIMAXCODE-TASKS-2026-10-08.md)、[ADR-0008路由记录](adr/0008-minimaxcode-remediation-routing.md) | 当前审计结论FAIL；先执行既有缺陷包，再满足依赖逐节点准入。Human转发任务，Codex保留终审；ADR-0007仍Proposed。 |
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
