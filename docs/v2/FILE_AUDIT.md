# 全仓文件审计与归档决定

> 2026-09-28；关联 DEV-00 文档治理 / V2 R0。审计以 `rg --files`、`git status`、交叉引用扫描及 Rescan 为依据；排除 `node_modules/`、`target/`、`.pnpm-store/` 与 `.git/` 的生成/依赖缓存。它不是对每行源码的安全审计，也不改变节点验收状态。

| 文件群 | 审计结论 | 当前处理 |
|---|---|---|
| 根 manifest、lockfile、工具链、`.github/`、`scripts/ci/` | 当前构建/门禁输入；部分有用户未提交改动。 | 保持原位，根文件由 Codex 串行整合；不能清理以求“干净”。 |
| `apps/`、`crates/`、`adapters/`、`packages/`、`tests/` | 当前代码和验证资产；Wave 2 部分为未跟踪文件。 | 全部保留，不以 V2 重启为由重做或归档；审计结果见 [Rescan](../../PROJECT_RESCAN_REPORT.md)。 |
| `docs/development/`、`docs/specs/`、`docs/adr/` | 冻结规格与已接受 ADR-0001～0004，仍约束实现。 | 保持权威位置；V2 冲突须经新/替代 ADR 与 Human 接受。 |
| `docs/reviews/DEV-*.md`、Wave 1 复核 | 正式或待完成节点证据。 | 保留原位；历史数字不当作当前快照证据。 |
| 旧 `WAVE-2-HANDOFF.md`、`GEMINI_WAVE2_*`、`WAVE-2-LOG.md` | 未关闭 Wave 2 的执行/阶段自验收证据，其中旧长期目标和旧模型路由不能继续派工。 | 原样移入 [历史索引](../archive/README.md)，保留哈希与新路径；第 6 节仍可供旧节点门禁复核。 |
| `docs/coordination/TASK_PACKET_TEMPLATE.md` | 当前可用的单节点任务包模板。 | 保留原位并改为模型/阶段中立；现行状态与派工以新 `docs/coordination/README.md` 为入口。 |
| V1 根 PRD、`design/界面设计方案.md`、`design/figma-state.json` | 旧基线仍引用其已确认原则、视觉/原型索引。 | 暂留原位；V2 变更未获 ADR 前不可悄然丢失其事实效力。 |
| 旧开发修改清单、PRD-Figma 第四轮报告、设计整改说明 | 旧规划/审计过程材料，不能直接指导 V2 派工。 | 已原样移至 [历史索引](../archive/README.md)；原件哈希留存，现行开发基线的路径引用已更新。 |
| Wave 2 “最终”自验收报告、2026-09-24 旧动态协作 README | 标题/旧路由易把 `SELF_CHECK_PASS` 误读为正式 `DONE` 或继续执行过时派工。 | 已原样移至 [历史索引](../archive/README.md)；新协作入口重述当前正式状态。 |
| `TEAMSPACE_V2_项目重启基线与Harness分工.md`、`PROJECT_RESCAN_REPORT.md`、`docs/v2/` | V2 目标、R0 现状和拟议阶段计划。 | 保持活跃入口；未接受的架构/模型路线均标“提案/待决”。 |
| `sources/` | ChatGPT 项目同步源，当前无可盘点的普通文件；仓库规则要求只读。 | 未触碰；后续即使出现文件也不移动或编辑。 |

本轮归档严格限定 [索引](../archive/README.md)列明的 9 个原件。旧 `多-Harness-AI-开发团队协作平台-PRD.md` 和冻结规格、ADR、未结 `DEV-*` 审核记录未移走；未结 Wave 2 的交接/进度资料仅移动位置、仍可审计。这使历史与当前要求能被迁移，不让 V1 自动覆盖 V2。新开发任务从 [文档入口](../README.md)和[阶段计划](WORKSTREAMS_AND_ACCEPTANCE.md)读取，不从归档文件派工。
