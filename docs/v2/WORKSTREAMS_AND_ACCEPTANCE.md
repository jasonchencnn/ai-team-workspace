# V2 阶段分工、交付文件与验收门

> 版本：R0 冻结版，2026-09-28。依据 [V2 目标](../../TEAMSPACE_V2_项目重启基线与Harness分工.md)、[Rescan](../../PROJECT_RESCAN_REPORT.md)、[任务冻结清单](TASK_FREEZE.md)和[现行协作入口](../coordination/README.md)。**本表不是已接受 ADR、DEV 节点放行或外部服务调用授权；全部阶段暂停新实现派工。**每项正式实现任务仍须关联获准的 `DEV-*` 节点、其依赖、冻结规格和 G0～G9；V2 的新节点编号须在 R1 冻结。

## 角色与模型约定

| 开发岗位 / Harness | 模型（实际派工须登记准确版本） | 责任边界 |
|---|---|---|
| Human Owner | 无模型 | 接受/拒绝 ADR、确定权限上限和费用/外部操作授权；不承担人工重写任务内容。 |
| Codex 总控 | `gpt-5.6-sol` 或更高能力模型 | 架构、协议和权限边界，节点依赖、根 manifest/lockfile、跨模块整合、R3 逐差异复核、正式 `DONE`/Go-No-Go。 |
| Codex Subagent | `gpt-5.6-luna`（资料/机械）；`gpt-5.6-terra`（工程实现或独立审核） | 总控签发 `DEV-*`、精确写集和验收命令；不与其他执行者写同一文件；Terra 可担任 ADR-0005 指定的 Mac 证据独立审核。 |
| Gemini 外部开发 Harness | `Gemini-3.8-Flash` | 仅在 ADR-0004 适用的旧 Wave 2 节点和总控任务包范围内主实现/阶段自验收；是否用于新 V2 节点待 R1 模型路由决策。 |
| MiniMaxCode | `MiniMax-M3`（V2 后端候选） | V2 目标指定的后端/基础设施主力候选；**当前旧 Wave 2** 仍受 ADR-0004“边角辅助”限制。新 V2 R3 任务需先获路由决策、独立审核和能力证据。 |
| Antigravity | **实际模型/版本待核实，正式派工前必填** | V2 前端、Figma 原型和浏览器 QA 候选；模型未知时只能编排工作包，不可把某模型写成已验证执行者。 |

“Leader / Frontend / Backend”在此表是**开发期间的 Harness 分工**；产品运行时的稳定 Role 仍由 Teamspace 保存，不能把 Harness 或模型写入 Task owner。外部 Gemini/MiniMax/Antigravity 的实时额度和访问能力未验证；不预设自动调用、免费或共享会话。[权限矩阵](../specs/PERMISSION_MATRIX.md)、[ADR-0004](../adr/0004-cross-model-r3-routing.md)。

## 阶段任务与交付

| 阶段与关联旧 DEV | 准入 / 分工内容 | 主责 Harness·模型；支持/独立复核 | 明确交付文件（计划路径） | 验收标准与出口 |
|---|---|---|---|---|
| **R0-A：历史文件治理**（DEV-00 文档基线） | 已有 Rescan；盘点全仓文件，归档过时规划和自验收快照，保留冻结规则、代码及未结证据。 | Codex 总控 `gpt-5.6-sol+`；Luna 只读清点；Terra 规则复核。 | `docs/README.md`、`docs/archive/README.md`、`docs/v2/FILE_AUDIT.md`、`docs/coordination/README.md`。 | 归档原件哈希不变；当前入口无过时派工；旧节点状态不变；引用和 `git diff --check` 通过。此阶段不声明产品验收。 |
| **R0-B：旧 Wave 2 收口**（DEV-02-02～10、DEV-03-02～05） | 按[正式验收方案](../coordination/WAVE2_MAC_ACCEPTANCE_PLAN.md)逐项审查；02-02/03-02 的后续结论见[前置节点终审](../reviews/WAVE-2-ENTRY-CLOSURE-2026-09-28.md)。后续按依赖复核跨 Team 关联、DAG 多跳环、Adapter 协议与故障注入；必要修复另签任务包；每次变更后在当前输入执行 Mac 完整门禁。 | Codex 总控 `gpt-5.6-sol+` 负责 R3 决策/整合及正式判定；未参与实现的 Codex Terra 独立审核；Luna 可采集机械证据；Gemini-3.8-Flash 仅说明既有实现或按新任务包修复；MiniMax-M3 仅互斥低风险辅助。 | [首轮独立审查记录](../reviews/WAVE-2-MAC-ACCEPTANCE.md)、[前置节点终审](../reviews/WAVE-2-ENTRY-CLOSURE-2026-09-28.md)、可重放反例、Mac 门禁及全 refs 扫描证据；后续节点补充附录按精确任务包列明。 | 每个节点专项标准 + G0～G9；旧 migration 不改写、使用前向修复；当前最终输入 macOS 21 项、全 refs 扫描、工具来源、证据哈希与 Terra 独立审核通过后总控逐节点决定 `DONE/REVIEW/BLOCKED`。首轮失败记录保留；02-03～10、03-03～05 尚未正式验收。 |
| **R1：V2 需求与架构冻结**（旧 DEV-00/02/03/05/06 的变更决策；新 DEV 节点待定） | 当前冻结，仅保留已有草案供只读评议；解冻后再冻结对象、事实归属、Conversation/Thread 迁移、ProviderSession、Gateway/ACP/Adapter、MCP、Leader 权限和 V2 AC/Gate。 | Codex 总控 `gpt-5.6-sol+` 主笔；Terra 独立做权限/迁移/协议反例审查；Human 最终接受。MiniMax/Antigravity 可提供只读可行性与设计反馈。 | `docs/adr/<新编号>-v2-architecture.md`（提案）；`docs/v2/DOMAIN_AND_CONVERSATION.md`、`HARNESS_GATEWAY.md`、`MCP_AND_PERMISSION.md`、`MIGRATION_PLAN.md`、`AC_AND_NODES.md`（均为拟建）；Figma 差异清单。 | 每项与旧冻结规格的冲突都有兼容/迁移/回退、负向用例和唯一事实来源；Human 明确接受 ADR 后同步正式规格和 DEV 依赖图。接受前文件只是草案，**不得实施 V2 新语义**。 |
| **R2：Harness Gateway**（旧 DEV-03-06～11；V2 新节点待定） | R1 通过且相关旧节点 `DONE`；实现统一 session 生命周期、能力/健康、ProviderSession；先验证目标 Harness 的 ACP 能力，保留 Adapter V0 fallback。 | MiniMaxCode `MiniMax-M3` 为 V2 后端主实现候选；Codex `gpt-5.6-sol+` 审核协议/安全并整合；Terra 独立复核；Gemini 是否参与由 R1 路由决定。 | `crates/<gateway>/`、`crates/persistence-sqlite/migrations/<forward>.sql`、契约/故障测试、`docs/reviews/<V2-DEV>-REVIEW.md`（具体路径在任务包冻结）。 | 同一上层 Harness Interface 不泄漏 provider 私有语义；create/resume/send/stream/status/cancel/interrupt/permission/close 的能力缺口显式表达；超时、乱序、晚到、fencing、崩溃恢复有负向证据；真实 Harness Spike 单独记录，不用 Mock 冒充。 |
| **R3：Conversation System**（旧 DEV-02-08、05-07、06-14 的演进；V2 新节点待定） | R1 数据/留存决策通过，R2 最小 Gateway 契约已冻结；以 Teamspace 为消息总线，平台保存 Conversation、Message、DeliveryAttempt、Event 和 ProviderSession 引用。 | MiniMaxCode `MiniMax-M3` 后端候选；Antigravity（模型待核实）负责后续 UI 原型；Codex `gpt-5.6-sol+` 定义迁移、权限与最终审核；Terra 复核。 | 前向 migration 与 repository/router、`docs/v2/CONVERSATION_CONTRACT.md`、持久化/投递/脱敏测试；Figma Conversation 增量画板和映射。 | Conversation ID 与外部 session/thread ID 分离；Role 换绑后消息、历史和投递快照连续；队列/重试/TTL/离线/人工接管可审计；无权限越界和明文秘密；不可导入的旧外部会话明确标为缺失，不伪造。 |
| **R4：Leader Control Plane**（旧 DEV-02-13、DEV-05、DEV-07；V2 新节点待定） | R1 权限矩阵/MCP Schema 已接受，R2/R3 核心命令和消息边界可用；以 Teamspace MCP 对授权 Leader 暴露团队、Task、Role、Conversation、Review 等工具。 | MiniMaxCode `MiniMax-M3` 后端候选；Codex `gpt-5.6-sol+` 主导 MCP Schema/权限审查；Terra 做独立安全负向审核。 | MCP tool Schema/Server、对应 application commands/read models、权限拒绝与审计测试、`docs/reviews/<V2-DEV>-REVIEW.md`。 | 每个工具在 Rust 控制面重新计算权限/版本/幂等；Leader 创建 Role/换绑能力严格按**R1 已批准**的矩阵，Human-only 与自提权拒绝；无直接 Harness-to-Harness 正式消息旁路。 |
| **R5：Teamspace UI V2 与 Figma**（旧 DEV-06-02～17；V2 新节点待定） | R1 交互语义和真实 read model 已冻结；保留现有视觉/Role/Task 原型，补 Conversation、Leader、Gateway/ProviderSession 流程。 | Antigravity（**模型待核实**）主责设计/前端/浏览器 QA；MiniMaxCode `MiniMax-M3` 提供已验收 API；Codex `gpt-5.6-sol+` 做领域/产品一致性审核。 | Figma V2 增量画板与节点映射、`apps/desktop/src/**`、组件/浏览器测试、设计偏差记录、`docs/reviews/<V2-DEV>-REVIEW.md`。 | 页面由真实 IPC 数据驱动；加载/空/失败/断连/权限/版本冲突/失效引用俱全；Role 为主体；键盘、窄屏、WCAG 2.2 AA 与异常闭环；静态成功页不算通过。Figma 写操作另按外部服务规则确认。 |
| **R6：真实纵向集成与候选判断**（旧 DEV-07/08；V2 新节点待定） | R2～R5 节点已正式 `DONE`；使用真实控制面和至少一个受控 Worker Harness 验证 Human→Leader→Teamspace→Worker→Teamspace→Leader。 | Codex 总控 `gpt-5.6-sol+` 集成与 Go/No-Go；Antigravity（模型待核实）做浏览器/桌面 QA；MiniMaxCode `MiniMax-M3` 修复后端；Terra 独立安全/数据审核；Human 决定发布。 | 纵向 E2E/故障注入、AC-01～12 与新增 V2 AC 追踪、性能/安全/恢复/A11y 报告、`docs/reviews/V2-INTEGRATION-REVIEW.md`（拟建）。 | 真权限、真实 SQLite/Workspace、取消/恢复/fencing、换绑与 Conversation 历史、审计回放全部可验证；无 S0/S1，G0～G9 与阶段门通过；仅 Human 作最终发布/费用/生产决定。 |

## 执行约束与停止条件

- **当前冻结**：按[任务冻结清单](TASK_FREEZE.md)暂停全部新实现派工、节点晋级、V2 R1～R6 执行及 Figma 修改。本轮只完成 R0 文档治理与 R0-B 验收准备；正式验收尚未开始。解冻逐节点按正式验收方案、依赖和任务包执行。R2～R6 的模型与文件路径是可评审分工，非开工许可。
- 每个外部任务包先确定 `DEV-*`、实际 Harness/模型、难度理由、精确写集、前置正式 `DONE`、冻结规格、成功/失败用例、验收命令、产物/回退。数据库、Gateway、前端各自单写者；根 manifest/lockfile 总控串行。
- 额度：Codex 用于边界与终审，Luna/Terra 承担资料及独立工程审核；外部模型仅在实际额度/费用和文件访问核实后领任务。新增费用沿用 ADR-0003 的 `$0` 上限；不能以低成本牺牲 R3 独立审核。
- 任一规格冲突、依赖非 `DONE`、未经批准的权限/协议变化、S0/S1、写集冲突、模型不明或可能产生外部费用，停止该任务并回到总控/Human 决策。
