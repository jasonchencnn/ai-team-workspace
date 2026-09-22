# AI Team Workspace 开发准备基线

> 状态：**DEV-00 已冻结；Wave 1 独立复核 PASS；尚未启动 Wave 2**  
> 版本：1.0  
> 日期：2026-09-21  
> 适用范围：local-first MVP（PRD AC-01～AC-12）

## 1. 基线目的

本目录把 PRD、Figma 原型与四轮审计结论转成后续开发必须遵守的工程基线。后续任务必须先关联开发节点，再按节点指定模型执行和审核；不能自行改技术栈、状态机、权限边界或协议语义。

开发准备、决策冻结与 DEV-00 规格已完成；Wave 1 已交付工程底座、基础值对象、Adapter Schema 契约测试和设计 Token。2026-09-22 独立复核发现的 DEV-03-01 机器契约缺口已按 ADR-0002 完成整改并通过全部 Wave 1 门禁；本次工作到 Wave 1 复核为止，尚未启动 Wave 2。

## 2. 输入材料与事实优先级

已核对材料：

1. `多-Harness-AI-开发团队协作平台-PRD.md` V1.0；
2. `design/界面设计方案.md`；
3. `design/figma-state.json`：36 个画板、129 条原型连接、610 个可见文字节点；
4. `PRD-Figma审计报告.md` 第四轮复核版；
5. `design/审计整改说明.md`；
6. `开发修改清单.md`。

发生冲突时按以下顺序处理：

1. Human 最新明确决策与已接受 ADR；
2. PRD 已确认的产品原则、状态机和 AC-01～AC-12；
3. 本目录已冻结的技术与工程基线；
4. 第四轮审计中仍有效的开发中调整项；
5. Figma 页面所表达的信息架构和视觉意图；
6. 旧版开发清单、演示数据和原型跳转。

Figma 是体验与视觉依据，不是业务状态机或权限规则的事实源。原型的静态成功页不能替代真实后端校验。

## 3. 已关闭的开发前决策

下列原审计未决项现已冻结：

| 决策 | 冻结结论 |
|---|---|
| 产品形态 | Tauri 2 桌面应用；React UI + Rust 模块化单体控制面；MVP 不拆微服务。 |
| 持久化 | 每个本地安装使用 SQLite；SQLx 迁移；WAL、外键、事务、乐观锁与 Outbox 默认开启。 |
| 凭据 | 明文只进入操作系统安全存储；数据库、Event、日志和 Workspace 仅保存不透明引用。 |
| Adapter | 语言无关的 JSON-RPC 2.0 over stdio；版本化 manifest + JSON Schema；先 Mock，再 Codex。 |
| Workspace | Git Workspace 采用每 Run 独立 worktree；非 Git Workspace 仅单写者。 |
| Human 通知 | `Notification` 是面向 Human 的投递对象；`Message/Thread` 只做 Role-to-Role 协作；两者都引用 Event，但互不冒充。 |
| ID 与时间 | 内部 ID 用 UUIDv7；显示 ID 独立；持久化和协议统一 UTC/RFC 3339，UI 按本地时区展示。 |
| 删除语义 | P0 不硬删除核心对象；使用停用、取消、归档和保留期。Event、Run、Binding 历史不可改写。 |
| 平台优先级 | MVP 首验 macOS 14+；Rust 核心在 Ubuntu CI 验证；Windows 不作为首发门槛，但代码不得依赖未封装的 POSIX 假设。 |
| 首批 Harness | Mock Adapter → Codex Adapter → 第二真实 Harness；第二个真实 Harness在 Spike 证据后决定。 |
| Task 看板 | `TODO` 与 `READY` 在数据和操作上始终分离；“待开始”只能是展示聚合，卡片必须显示真实子状态。 |

正式决策记录见 `docs/adr/0001-mvp-implementation-baseline.md`。

## 4. 必读顺序

1. `TECH_STACK_AND_STANDARDS.md`：技术栈、架构边界、领域与编码规范；
2. `DEVELOPMENT_NODES.md`：节点、依赖、模型与逐节点审核标准；
3. `QUALITY_GATES.md`：通用门禁、阶段出口、测试与证据要求；
4. `docs/specs/README.md`：DEV-00 冻结规格索引；
5. `docs/adr/0001-mvp-implementation-baseline.md`：冻结决策及其后果。

## 5. 开发范围

P0 只实现 PRD local-first MVP：Team、Role、Binding、Harness、Workspace、Task、Run、Artifact、Event、Message、Notification、Permission、Approval、Human takeover 及可恢复审计闭环。

下列内容明确不在 P0：自动任务拆解、能力/成本自动调度、自动合并、生产部署、生产数据操作、复杂策略语言、邮件/Slack/Webhook、组织级跨 Team、多 Human 邀请与云端分布式控制面。

## 6. 变更控制

- 技术栈或架构边界变更：必须新增 ADR，列出动机、兼容/迁移成本、风险、回退方案和验证结果。
- PRD 语义变更：必须先更新需求决策记录和 AC 追踪，再改代码。
- 设计偏差：在实现说明中登记 Figma 节点、偏差原因和替代验收证据。
- 安全或数据兼容性变更：必须由 `gpt-5.6-sol` 执行或复核。
- 任一阶段门禁未通过：后续依赖节点暂停，不得以“后补测试/后补审计”方式越过。

## 7. 当前准入结论

Human 已于 2026-09-21 确认基线。DEV-00-01～10 已完成，Gate A 结论为 PASS；执行记录见 `docs/reviews/DEV-00-FREEZE-REVIEW.md`。Human 于 2026-09-22 接受 ADR-0002，DEV-03-01 已使用 `gpt-5.6-sol` 完成整改并通过独立协议复核，Wave 1 结论恢复为 PASS；证据见 `docs/reviews/WAVE-1-INDEPENDENT-PROTOCOL-AUDIT.md`。Wave 2 尚未启动，前端仍不得复制领域状态机或权限规则。
