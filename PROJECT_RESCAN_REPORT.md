# AI Teamspace V2 — Project Rescan Report

> 日期：2026-09-28；范围：V2 Phase R0 / CODEX-001，只读扫描与现状判断。关联旧基线 DEV-00、Wave 1（DEV-01-01～04、DEV-02-01、DEV-03-01、DEV-06-01）及 Wave 2（DEV-01-05～09、DEV-02-02～10、DEV-03-02～05）。本报告不是新 ADR、节点放行记录或 V2 实现授权。

> 后续决策：本报告形成后的 [ADR-0005](docs/adr/0005-macos-first-quality-and-release-gates.md) 已把当前阻断性平台改为 macOS；双环境要求仅为历史背景。当前冻结范围和正式验收安排以[冻结清单](docs/v2/TASK_FREEZE.md)及[Wave 2 验收方案](docs/coordination/WAVE2_MAC_ACCEPTANCE_PLAN.md)为准。

## 1. 结论与证据等级

**接收既有工程资产，暂不重新开始。** Wave 1 已有正式独立复核；Wave 2 已写出 9 个 SQLite migration、Adapter supervisor/Mock 和相应测试，具备复用价值。但 Wave 2 的 `ALL_SELF_CHECK_PASS` 只是实现者阶段自验收，不能解释为正式 `DONE`。当前缺少针对最终工作区输入的 macOS ARM64 完整门禁、全 refs 秘密扫描、工具来源证据和独立终审；迁移约束还需检查跨 Team 关联。V2 的 Conversation、ACP-first Gateway、Teamspace MCP 与 Leader 扩权均尚未获得替代旧冻结规格的 ADR，故不能直接进入实现。[V2 R0/R1](TEAMSPACE_V2_项目重启基线与Harness分工.md#30-v2-推荐开发顺序)、[冻结变更规则](docs/development/README.md)、[Wave 2 协调入口](docs/coordination/README.md)、[Wave 2 自验收归档](docs/archive/wave2-self-check/WAVE-2-FINAL-REPORT.md)。

证据分层如下，避免把不同含义混在一起：

| 层级 | 本次结论 |
|---|---|
| 源码存在 | 当前工作区直接核对，包含未提交和未跟踪文件。 |
| 本机可运行 | 本次复跑 Rust/TS 测试及部分静态检查通过；只证明当前机器的局部行为。 |
| 历史节点状态 | DEV-00、Wave 1 和 DEV-01-05/08/09 有历史 `DONE` 记录；DEV-02-02、DEV-03-02 为 `REVIEW`；其余 Wave 2 有自验收记录而未见总控正式 `DONE`。 |
| 当前正式放行 | 本次未取得当前快照的 macOS ARM64 21 项完整门禁、全 refs 扫描、工具来源及独立终审，不能宣称 Wave 2 通过 Gate。 |

本地 Git 为 `codex/wave-2-ci-validation`，HEAD `1a14b8795e76fc373bef326eb5df96f9ac173f71`；扫描前工作区有 43 条 `git status --porcelain` 记录。V2 基线、Wave 2 migration、Mock、审核材料等包含未跟踪文件，**HEAD 不代表当前输入快照**。没有清理、覆盖或提交这些改动。

## 2. 实际结构与技术栈

仓库是 pnpm + Cargo workspace。`apps/desktop` 为 Tauri 2 / React 19 / TypeScript / Vite 的最小桌面壳；Rust crate 有 `domain`、`contracts`、`persistence-sqlite`、`adapter-supervisor`、`observability`，以及仍为空骨架的 `application`、`security`、`workspace`。`adapters/mock` 有 Node Mock，`adapters/codex` 尚是占位。`packages/contracts-ts` 和 `design-tokens` 已存在；`docs/specs`、ADR、开发节点、审核记录覆盖旧 MVP；`design/figma-state.json` 保存原型索引。见 [根 manifest](package.json)、[Cargo workspace](Cargo.toml)、[桌面壳](apps/desktop/src/App.tsx)、[SQLite 迁移注册](crates/persistence-sqlite/src/lib.rs)、[Adapter 目录](adapters/mock/index.mjs)。

已冻结选型为 Tauri 2 + React/TypeScript 前端、Rust 模块化单体、SQLite/SQLx、JSON-RPC 2.0 over stdio Adapter、每 Run Git worktree、OS 安全存储和类型化 IPC；V2 没有授权替换这些选型。V2 若增加 Teamspace MCP，应先判断是否能在现有本地进程边界内实现；不能默认为拆出 HTTP 服务或微服务。[技术基线](docs/development/TECH_STACK_AND_STANDARDS.md)、[ADR-0001](docs/adr/0001-mvp-implementation-baseline.md)。

## 3. 旧节点与 V2 的接收判断

状态词：`Implemented` 指有源码及本机验证；`Partial` 指仅有部分边界；`Designed` 指冻结规格/原型存在而无业务实现；`Conflicting` 指与 V2 或冻结规则有待决冲突；`Unknown` 指缺少足够证据。此表**不改变正式节点状态**。

| 范围 | 实际状态 | R0 处理 |
|---|---|---|
| DEV-00 | 冻结规格与 Gate A 审核存在，历史 `DONE`。 | **吸纳**为旧 MVP 的事实基线；V2 差异走需求决策与 ADR，不暗改规格。 |
| Wave 1：DEV-01-01～04、DEV-02-01、DEV-03-01、DEV-06-01 | `Implemented`，工程骨架、值对象、Adapter V0 Schema/契约、设计 Token；独立复核 PASS。[Wave 1 审核](docs/reviews/WAVE-1-REVIEW.md)、[协议审计](docs/reviews/WAVE-1-INDEPENDENT-PROTOCOL-AUDIT.md) | **直接吸纳**。这些基础能力与 V2 核心原则兼容。 |
| DEV-01-05～09 | CI/桌面壳/Schema 生成/日志脱敏/fixture 已实现，历史记录部分 `DONE`。 | **吸纳代码**；以后关闭新节点必须重跑最终快照门禁，旧双环境数字不是当前证明。 |
| DEV-02-02～10 | `Implemented` 在数据库迁移层：9 个 migration 和 31 个本机测试；正式状态至多 `REVIEW`。[迁移注册](crates/persistence-sqlite/src/lib.rs)、[自验收账本](docs/archive/wave2-execution/GEMINI_WAVE2_PROGRESS.md) | **有条件吸纳**，先做独立差异/安全审核和负向用例；旧 migration 保留，以前向迁移补正。不能把表存在等同于领域服务完成。 |
| DEV-02-11～15、DEV-04/05 | `Designed`；状态机、集中权限、幂等/Outbox dispatcher、备份、Workspace/应用服务仍无相应实现；`application/security/workspace` crate 为空骨架。[领域 crate](crates/domain/src/lib.rs) | **按冻结节点继续规划**，在 V2 ADR 落地后调整节点依赖。 |
| DEV-03-02～05 | supervisor、握手/健康、Mock 正常与故障流 `Implemented`；正式验收未关闭。[supervisor](crates/adapter-supervisor/src/lib.rs)、[Mock](adapters/mock/index.mjs) | **有条件吸纳**为 Adapter Provider 候选底座；Gateway/ACP 层另建抽象，不能把 V0 协议直接改名为 ACP。 |
| DEV-03-06～11 | `Designed`；取消/重试、租约与恢复、Keychain、真实 Codex Adapter Spike 不完整或未开始。 | **保留节点**，与 V2 Provider 生命周期合并评估。 |
| DEV-06-01 | Token 和主题 `Implemented`。 | **吸纳**。 |
| DEV-06-02～17、DEV-07/08 | `Designed`；当前 `App.tsx` 仅显示“桌面应用壳”，无 Task/Role/Conversation 真正页面或主链。[桌面壳](apps/desktop/src/App.tsx) | **不宣称前端或端到端已完成**；先冻结 V2 数据与接口，再做真实 UI。 |

## 4. 模块映射与 V2 缺口

| V2 能力 | 现有可复用部分 | 缺口 / 冲突 |
|---|---|---|
| Team/Role/Binding/Task/Run/Artifact/Event/Permission | 值对象、表与部分数据库约束；Role 稳定、Binding 历史、Task/Run 分离等原则一致。 | 大多仍只有 DDL；无可信应用命令、完整状态机、权限执行、读模型和 IPC。 |
| Harness Gateway | Adapter V0 Schema、supervisor、initialize/health、Mock/故障注入。 | 无统一 Harness Interface、ACP Provider、ProviderSession、HarnessConnection；现有 `bindings.adapter_id` 强制非空，难以原样表示 ACP 直连。[Binding DDL](crates/persistence-sqlite/migrations/0002_harness_adapter_binding.sql) |
| Conversation | 现有 `threads/messages/delivery_attempts` 支持 Role 消息、回复、Binding 快照与重试记录。[通信 DDL](crates/persistence-sqlite/migrations/0007_thread_message_delivery.sql) | Thread 当前只是岗位协作容器，不等于 V2 保存完整 Harness 会话；缺 Conversation 身份/范围、ProviderSession、运行事件与消息持久化映射、历史导入策略和 Conversation View。不能把 Mock 的 `session_reference` 当平台会话事实。[旧领域模型](docs/specs/DOMAIN_MODEL.md)、[Run DDL](crates/persistence-sqlite/migrations/0004_run_lease_review.sql) |
| Teamspace MCP / Leader | 旧权限矩阵允许 Leader 创建/分配 Task、发 Role 消息、读团队状态。 | MCP Server/Tool Schema 不存在；应用服务也尚未实现。V2 默认允许 Leader 创建普通 Role，直接冲突于旧 `Human-only` Role 创建/编辑/停用规则，必须先获 Human 新决策。[权限矩阵](docs/specs/PERMISSION_MATRIX.md) |
| Workspace / 审计 | Artifact/Reference、Event/Outbox 表有基础；旧事实归属与 V2 大体相容。 | 文件访问、Git 隔离、Event writer/dispatcher、hash chain/导出仍未形成闭环；不可用 DB 表存在替代审计能力。 |
| UI | Figma 画板和信息架构、设计 Token 可复用。 | 产品 UI 尚为空壳；V2 的 Conversation、Leader 控制、Gateway/ProviderSession 没有真实页面实现。 |

`Obsolete`：目前**没有足够证据建议删除既有实现**。旧 Adapter V0 及 Thread UI 语义可能需要兼容/演进；何时弃用必须在 R1 说明历史数据、协议版本和回退方式。`Unknown`：真实 Codex/Antigravity/MiniMax 控制能力、已有用户数据库数量、历史会话可导入性、线上 Figma 的细节交互、外部模型剩余额度均未验证。

## 5. 旧 PRD / 冻结基线与 V2 的待决差异

| 主题 | 旧事实 | V2 提案 | 决策要求 |
|---|---|---|---|
| 长期会话 | 旧 PRD 明确不将完整 Harness 对话历史当长期记忆；`Thread/Message` 只保存岗位通信。[旧 PRD](多-Harness-AI-开发团队协作平台-PRD.md)、[领域模型](docs/specs/DOMAIN_MODEL.md) | Teamspace 保存完整 Conversation，并把 Provider Session 当执行引用。 | 需求语义和数据生命周期变化；更新追踪矩阵/领域规格、保留期/隐私、迁移 ADR。 |
| 传输路径 | ADR-0001 锁定 Adapter V0 JSON-RPC stdio；Mock → Codex Spike。 | ACP-first Gateway + Adapter Provider fallback。 | 新 ADR 明确 ACP/Adapter 能力等价边界、V0 兼容、版本协商、故障/取消/恢复映射；ACP-first 是优先研究方向，不等于现有 Harness 已可接入。 |
| Leader 权限 | Task/消息等受限操作允许；Role 创建/修改及 Binding 换绑 P0 为 Human-only。 | Leader 默认可创建/修改普通 Role；换绑可附加授权。 | 权限矩阵直接冲突；Human 明确选择边界后才能定义 MCP 工具授权。Leader 自提权仍禁止。 |
| 人工入口 | 旧 P0 以 Teamspace UI 操作闭环为核心。 | Human 主要停留在 Leader Harness，Teamspace 兼具 GUI 与 MCP 控制。 | 需明确 P0 验收是否包含 Leader→Teamspace→Worker→Leader 真闭环；旧 AC-01～12 是否扩展。 |
| 成本调度 | 旧 P0 不含自动能力/成本优化。 | V2 允许多 Provider 和 Leader 编排，但未定义自动额度调度。 | 建议 R1 仅做可见的能力、额度/成本预算与人工选路；自动策略另作决策。 |

旧已发布 migration 不直接修改。若 V2 采用 Conversation，可评估把 Thread 作为现有会话容器演进，或创建新 Conversation 并映射旧 Thread；这两条都是待评审方案，不能在 R0 假定一一等价。迁移必须保留旧 ID、Message/DeliveryAttempt/Event、Binding 快照、Run/Artifact 关联；不可凭空补造外部 Harness 的历史全文。

## 6. 数据与验收风险（按优先级）

1. **Team 隔离反例待补**：Binding 的 `team_id` 与 Role/Harness/Adapter 各自外键只验证 ID 存在，未保证相同 Team；Task owner Role、Run 关联也有同类风险。[Binding DDL](crates/persistence-sqlite/migrations/0002_harness_adapter_binding.sql)、[Task DDL](crates/persistence-sqlite/migrations/0003_task_dependency.sql)、[Run DDL](crates/persistence-sqlite/migrations/0004_run_lease_review.sql)。当前测试未覆盖跨 Team 写入；需独立失败用例验证，再选择前向约束与应用层双重校验。
2. **依赖多跳环仍有缺口**：`task_dependencies` 只拒绝自环和二节点互环；冻结要求任意多跳环在写入事务内拒绝。该义务可落在后续 Domain Graph Service，但在实现之前不能宣称 DAG 闭环完成。[依赖 DDL](crates/persistence-sqlite/migrations/0003_task_dependency.sql)、[技术基线](docs/development/TECH_STACK_AND_STANDARDS.md)。
3. **运行时 SQLite 配置未落地**：当前 `migrate()` 要求调用者自行打开外键；WAL、`busy_timeout`、连接初始化及生产 repository 尚未见实现。[迁移入口](crates/persistence-sqlite/src/lib.rs)、[技术基线](docs/development/TECH_STACK_AND_STANDARDS.md)。
4. **Conversation 迁移与隐私**：旧 `messages.to_role_id` 必填，适合 Role 消息，但 V2 完整会话会包含 Human↔Leader、工具事件/流和 provider 原始/摘要内容。需先定义哪些内容持久化、脱敏、保留、权限可见，再设计表和回填；旧外部私有会话可能不可恢复。
5. **进程/会话恢复**：supervisor 本机测试覆盖直接子进程与遗留 pipe 的有界退出，未证明所有 Adapter 后代进程可清理；ACP/ProviderSession 的 cancel、interrupt、resume、reconcile 和 fencing 必须分别验证。[交接归档](docs/archive/wave2-execution/WAVE-2-HANDOFF.md)、[supervisor](crates/adapter-supervisor/src/lib.rs)。
6. **正式门禁**：按 ADR-0005，最终同一输入须在 macOS ARM64 完成 21 项，附全 refs 历史秘密扫描、工具来源和前后输入哈希证据。V2 文件与此报告新增后，旧 21/21 证据已失效；本次本机检查也不等于完整 Gate。[ADR-0005](docs/adr/0005-macos-first-quality-and-release-gates.md)、[协调入口](docs/coordination/README.md)。

## 7. Figma 原型判断

本轮读取了仓库 Figma 索引与冻结信息架构，并只读查询了线上文件 `C1roCB6shYmzP6oEu5ZAs4` 的页面/画板元数据，确认主要画板仍在；**未编辑 Figma，也未完成逐画板视觉/交互复验**。本地索引记录 36 个画板，旧规格已映射至 DEV-06。[Figma 索引](design/figma-state.json)、[信息架构](docs/specs/INFORMATION_ARCHITECTURE.md)。

**不应整套重画，R1 后需要补充/调整原型。** 可复用 Role/Binding、Task 控制、岗位通信、消息投递终态、Run 异常与审批画板；V2 需新增或修订：平台持久化 Conversation View（消息、投递、Event、Run、ProviderSession 关系）、Leader 视角的团队控制/阻塞/Review 操作流、Gateway 的 ACP/Adapter 来源与能力/健康展示、Run 与 ProviderSession 的区别、换绑后会话继续/失败恢复。现有 `13:356`、`13:364`、`29:732/733`、`64:848` 可作为 Conversation 起点；`4:291`、`13:355` 是 Leader 控制起点；`59:839`、`4:402` 是 Gateway/Binding 起点。正式导航还需对齐 [信息架构](docs/specs/INFORMATION_ARCHITECTURE.md) 的 Runs、Activity/Audit、Harness、通知/设置入口。Figma 只定义体验意图；状态、权限、真实完成必须来自后端。

## 8. 建议顺序与可派工边界

1. **R0 收口**：审阅本报告，保留现有工作区；不删除/重做 Wave 1/2。把 Wave 2 标记为“代码可审、本机验证通过、正式门禁未关闭”。
2. **先处理旧 Wave 2 验收阻断**：按[现行验收方案](docs/coordination/WAVE2_MAC_ACCEPTANCE_PLAN.md)独立复核跨 Team 反例及 migration/协议安全边界，确定前向修复；对最终快照执行 ADR-0005 的 macOS 门禁、全 refs 秘密扫描与 Terra 独立审核。逐节点更新正式状态，依赖未 `DONE` 不启动后续旧节点。
3. **R1 Architecture V2 Freeze**：Human 对需求范围、Leader 权限、ACP-first 及 Conversation 留存/迁移作明确决策；新增或替代 ADR，同步 PRD/AC、领域模型、状态机、权限矩阵、协议与节点图。冻结统一 Harness Interface、ACP/Adapter Provider、Teamspace MCP Tool Schema 和 V2 Gate。
4. **R2/R3 可并行准备、不可越门开工**：Gateway spike 验证具体 Harness 的 ACP/Adapter 现实能力；Conversation 存储/路由设计采用前向迁移与负向测试。互斥写集：`crates/adapter-supervisor/**` 与新增 Gateway、`crates/persistence-sqlite/**` 与 Conversation migration 分开；根 manifest/lockfile 总控串行整合。
5. **R4/R5/R6**：应用服务与 MCP 统一执行权限；前端按真实 read model 更新 Figma/页面；最后用真实 Human→Leader→Teamspace→Worker→Teamspace→Leader 闭环验证，不以 Mock 页面或静态跳转代替。

V2 原文的 `MM-001`～`MM-006`、`AG-001`～`AG-006` **只是候选清单，不立即发给外部 Harness**。其 MiniMax 主开发建议与已接受 ADR-0004“Gemini-3.8-Flash 主做尚未开始的 Wave 2、MiniMax-M3 仅边角辅助”有范围/时效冲突；新的 V2 分工需在 R1 明确后形成精确 Markdown 任务包，不能据候选编号推断已授权改代码。[V2 任务候选](TEAMSPACE_V2_项目重启基线与Harness分工.md)、[ADR-0004](docs/adr/0004-cross-model-r3-routing.md)。

## 9. 额度与分工

本次 Codex 额度查询显示当前周期约 **2% 已用、98% 剩余**（查询时快照，非额度承诺）；账户没有可用额外 credits。ADR-0003 对新增外部费用固定为 **$0**。Gemini、MiniMax、Antigravity 的实时额度/费用和可调用权限本轮未验证，不能假设“免费”或无限量。[ADR-0003](docs/adr/0003-zero-cost-dual-environment-quality-gates.md)、[ADR-0004](docs/adr/0004-cross-model-r3-routing.md)。

建议把主控高推理额度留给 R1 边界、权限/迁移/协议反例、跨模块冲突和最终 Gate；Codex 内部 Luna 处理资料/机械核对，Terra 处理独立工程审核。外部 Gemini 可在已批准任务包、正式依赖 `DONE`、明确写集和 `$0` 条件下承担批量实现及阶段自验收；MiniMax 按 ADR-0004 做互斥低风险辅助；Antigravity 重点做原型/UI 与浏览器 QA。外部会话之间通过 Markdown 任务包/结果交接，不假定本仓库已具备自动调用。任何账号、远程环境、连接/上传或产生费用的动作，按项目规则另行说明影响并取得 Human 确认。

## 10. 本次验证与限制

Rescan 当次只读复核复跑：`cargo test --workspace --locked` PASS（含 Adapter 59、Domain 17、Observability 7、SQLite 31）；`pnpm run test` PASS（根层 27 + Desktop 4 + Token 4）；`node scripts/ci/verify-workflows.mjs`、`node scripts/ci/check-rust-unsafe.mjs`、`pnpm run contracts:check` 均 PASS。未执行完整 `local-gates`、Ubuntu/Oracle 门禁、全 refs 秘密扫描、真实 Harness Spike、桌面端 E2E 或 Figma 逐画板验证；这些不属于 R0 的正式放行证据。Rescan 当次没有改代码、规格、Figma，唯一新增文件为本报告；随后按 Human 要求进行的文档归档见 [归档索引](docs/archive/README.md)。
