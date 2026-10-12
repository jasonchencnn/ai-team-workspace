# Wave 2 跨模型协作入口

> 更新：2026-09-24 09:56 CST（Asia/Shanghai）；本地仓库路径、分支与 HEAD 于 09:52 再核对，不代表云端复验。
> 用途：给 Codex 总控、MiniMax-M3、Gemini-3.8-Flash 提供同一份 Markdown 交接入口。冻结规格、节点依赖与门禁仍以开发基线和已接受 ADR 为准。

## 1. 当前事实与准入

| 项目           | 当前记录                                                                                                         | 证据边界                                                                                        |
| -------------- | ---------------------------------------------------------------------------------------------------------------- | ----------------------------------------------------------------------------------------------- |
| Git 仓库       | `https://github.com/jasonchencnn/ai-team-workspace.git`（`origin`）                                              | 2026-09-24 本地 `git remote -v`；本轮未访问 GitHub                                              |
| 本地仓库       | `/Users/chenzhiqing/Documents/MyVault/协作工作`                                                                  | 2026-09-24 09:52 CST 迁移后 `pwd` 与 Git 根目录核对；含原用户未提交及未跟踪改动，禁止清理或覆盖 |
| 本地 Git 状态  | 分支 `codex/wave-2-ci-validation`，HEAD `1a14b87`                                                                | 2026-09-24 09:11 CST 快照；接手时重新核对，不把 HEAD 当作完整输入快照                           |
| 云端验证环境   | Oracle Cloud Ubuntu ARM64；SSH 别名 `oracle`，系统用户 `ubuntu`，经 Cloudflare Tunnel                            | 取自 `docs/reviews/WAVE-2-HANDOFF.md`；**本轮未连接复验**，不记录 IP、凭据或生产路径内容        |
| 云端专用工作区 | 远程用户目录下 `~/ai-team-wave2-batch2-v1/`；有效历史输入在 `workspace-v2/`，历史证据在 `evidence-v2/container/` | 交接记录；目录和容器现状需只读核对；不要使用已废弃 `workspace/` 作为证据                        |
| 本地历史证据   | `/private/tmp/ai-team-wave2-batch2-macos-v6.json`                                                                | 旧 155 文件快照的 21/21 PASS；新增交接与本文后已失效，不能证明当前输入                          |
| 云端历史证据   | `~/ai-team-wave2-batch2-v1/evidence-v2/container/WAVE2-batch2-ubuntu-arm64-v1.json`                              | 同一旧快照的 21/21 PASS；未补全最终证据包且本轮未复验                                           |

任何云端操作都先遵守已接受的 `docs/adr/0003-zero-cost-dual-environment-quality-gates.md` 和交接文档第 4、6 节：新增费用 `$0`；不得触碰现有生产容器、网络、卷、凭据与数据；不得上传完整 Git 历史。连接、上传、容器操作或其他账号/外部服务动作开始前，总控先向 Human 说明影响并取得确认。本文件的路径只是记录，不是执行授权。

## 2. 文档读取顺序与事实优先级

每位执行者从仓库根目录按顺序**完整阅读**：

1. `AGENTS.md`、`AGENTS.override.md`；
2. `docs/development/README.md`；
3. `docs/development/TECH_STACK_AND_STANDARDS.md`；
4. `docs/development/DEVELOPMENT_NODES.md`；
5. `docs/development/QUALITY_GATES.md`；
6. `docs/specs/README.md`，随后完整阅读所领 `DEV-*` 节点关联的冻结规格；
7. `docs/adr/0001-mvp-implementation-baseline.md`、`docs/adr/0002-strengthen-adapter-v0-machine-contract.md`、`docs/adr/0003-zero-cost-dual-environment-quality-gates.md`；
8. `docs/adr/0004-cross-model-r3-routing.md`、本文件、`docs/coordination/GEMINI_WAVE2_LONG_TERM_GOAL.md`；
9. `docs/reviews/WAVE-2-HANDOFF.md`、对应的 `docs/reviews/DEV-*-REVIEW.md` 和 `docs/coordination/WAVE-2-LOG.md` 最新条目，并只读核对实际 Git/环境状态。

节点专项规格按 `docs/specs/README.md` 的适用节点栏选择，不以本文件摘要替代原文。发生冲突时：Human 最新明确决策和**已接受** ADR → PRD 已确认原则/AC → 冻结开发基线和规格 → 有效审计调整 → Figma 体验/视觉 → 旧清单或演示。ADR-0004 已获 Human 接受，仅调整尚未开始节点的执行模型路由；`sources/` 只读，Figma 不定义状态、权限或真实完成语义。

## 3. 阶段状态与下一道门

| 范围                            | 状态                   | 下一步                                                                                                                         |
| ------------------------------- | ---------------------- | ------------------------------------------------------------------------------------------------------------------------------ |
| DEV-00、Wave 1                  | `DONE` / 独立复核 PASS | 作为冻结前置，不重做                                                                                                           |
| DEV-01-05、DEV-01-08、DEV-01-09 | `DONE`（既有审核记录） | 当前输入变化使旧批次证据不能直接用于新节点关闭                                                                                 |
| DEV-02-02、DEV-03-02            | `REVIEW`               | 对最终工作区重跑 macOS/Ubuntu 完整门禁、全 refs 历史秘密扫描、证据包核验与 ADR-0003 指定的 Terra 独立审核；总控才可判定 `DONE` |
| DEV-02-03～10、DEV-03-03～05    | `PLANNED`              | 等各自依赖 `DONE`；不以“实现建议”当作开工许可                                                                                  |
| Wave 3 及以后                   | 未准入                 | Wave 2 所有节点 `DONE`、总审核报告和 Human 下一阶段决策后再议                                                                  |

收口步骤以 `docs/reviews/WAVE-2-HANDOFF.md` 第 6 节为操作依据；其中示例证据文件名/批次号仅供参考，执行时必须使用新的排他路径并核对实际输入。旧的 21/21、45/45 Rust、35/35 TS 数字是**历史结果**，不是 2026-09-24 当前快照的测试结论。所有输入变化都按 ADR-0003 使旧证据失效。

## 4. 分工与写集

| 角色                               | 分工意向与当前允许动作                                                                              | 不可代替的职责/边界                                                                                     |
| ---------------------------------- | --------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------- |
| Codex 总控（`gpt-5.6-sol` 或更高） | 依赖与风险判断、分派、根 manifest/lockfile 串行整合、双环境与全历史证据核验、正式节点放行和最终验收 | 不因 Gemini 自报 `SELF_CHECK_PASS` 就把正式节点从 `REVIEW` 改为 `DONE`；R3 逐差异复核必需               |
| Gemini-3.8-Flash                   | 主要开发模型；依赖正式 `DONE` 后按精确任务包承担 R1/R2/R3 实现、测试及阶段自验收                    | 自验收不等于正式 `DONE`；已参与某节点代码写入时不得兼任独立审核者；不能自行更改根 manifest/lockfile     |
| MiniMax-M3                         | 仅做边角辅助：资料检索、文档/链接同步、fixture、机械性检查或互斥文件中的低风险测试补充              | 不担任本阶段节点主实现者或 R3 独立审核者；不得修改 Gemini 正在写的文件、根 manifest/lockfile 或冻结规格 |

当前 Wave 2 的建议派工如下；ADR-0004 已接受，但**每个节点仍以依赖正式 `DONE` 和总控签发精确任务包为开工前置**。Gemini 负责期间自验收和证据，总控负责正式放行及最终验收；`SELF_CHECK_PASS` 不是节点状态：

| 顺序 | 节点                                      | Gemini 主力开发 / MiniMax 边角辅助                                              | 生效条件与总控职责                                                                 |
| ---- | ----------------------------------------- | ------------------------------------------------------------------------------- | ---------------------------------------------------------------------------------- |
| 0    | DEV-02-02、DEV-03-02                      | Gemini 可辅助只读核查旧实现的负向用例；MiniMax 可整理证据索引，均不改实现       | 总控完成当前快照完整门禁、历史扫描、ADR-0003 要求的 Terra 独立审核和终审后才可关闭 |
| A    | DEV-02-03、DEV-03-03                      | Gemini 按节点任务包主实现、自测及阶段自验收；MiniMax 只做另行分派的互斥边角辅助 | 两个当前节点先正式 `DONE`；R3 独立复核和总控放行仍必需                             |
| B    | DEV-02-04、DEV-03-04                      | Gemini 主实现、自测及阶段自验收；MiniMax 可补另行分派的低风险 fixture/文档      | 分别等待 DEV-02-03、DEV-03-03 正式 `DONE`；总控放行                                |
| C    | DEV-02-05、DEV-03-05                      | Gemini 主实现、自测与失败注入；MiniMax 仅做边角文档/机械检查                    | 分别等待 DEV-02-04、DEV-03-04 正式 `DONE`；R3 独立复核及总控放行                   |
| D～H | DEV-02-06 → 02-07 → 02-08 → 02-09 → 02-10 | Gemini 按单写者顺序逐节点主实现和阶段自验收；MiniMax 仅做互斥低风险支持         | 严格按节点依赖和交接第 7～8 节；migration 均按 R3 门禁处理                         |

同一时刻最多 3 名执行者，且最多一个 `crates/persistence-sqlite/**` 写者、一个 `crates/adapter-supervisor/**` 写者；任何人不得同时改同一文件。独立审核者不碰本节点实现文件。总控独占根 `Cargo.toml`、`Cargo.lock`、质量门禁/ADR、最终节点状态和跨节点整合。每个派工必须写明 `DEV-*`、实际模型、难度理由、依赖、允许写入的**具体文件/目录**、禁止写入、验收命令、产物路径与停止条件。未分派的文件均为只读。

Gemini 与 MiniMax 的外部会话由 Human/总控通过 Markdown 任务包交接；**本仓库没有自动调用这两个模型的能力证明**。如不共享同一个工作区，执行者只提交 Markdown 结果和补丁/差异说明，由总控核验后整合，不能声称外部修改已自动进入本地仓库。Gemini 的长期目标与阶段自验收格式见 `docs/coordination/GEMINI_WAVE2_LONG_TERM_GOAL.md`、`docs/coordination/GEMINI_WAVE2_PROGRESS.md`。Gemini 作为实现者的节点仍需另派独立审核者；MiniMax 的边角辅助不默认等于独立 R3 审核。ADR-0003 指定的 Terra 独立证据审核继续有效，ADR-0004 不改变这项要求。

## 5. 每节点 Markdown 任务包与回传

总控按 `docs/coordination/TASK_PACKET_TEMPLATE.md` 发出单节点 Markdown 任务包，并在 `docs/coordination/WAVE-2-LOG.md` 新增带 `Asia/Shanghai` 时间的派工记录。开发者在节点审核文件或交接记录回传：实际文件、测试先失败后通过的证据、负向反例、完整命令及退出码、风险与回退。审核者另起只读小节指出 PASS/BLOCKED 及依据。总控复核后记录最终节点状态、当前输入哈希、macOS/Ubuntu 证据路径和下个可开始节点；证据缺失时保持 `REVIEW/BLOCKED`。

不提交、推送、部署、清理大量文件或访问生产环境；外部服务、权限、账号、费用动作须先向 Human 说明影响并取得确认。Wave 2 全部 16 个节点 `DONE`、最终双环境门禁和全 refs 扫描及总审核报告完成后停止，等待 Human 决定是否进入 Wave 3。
