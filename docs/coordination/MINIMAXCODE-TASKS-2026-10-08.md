# Minimaxcode 接续任务表与转发入口（2026-10-08）

**Human 可以直接转发本文件给 Minimaxcode。** 本表由 Codex 深入审计后签发；先修当前缺陷，不宣布项目完成、不跳过冻结依赖。主责已由 Human 明确指定 Minimaxcode，Codex 保留独立复核和正式 DONE 权限，见 ADR-0008。实际模型/版本由执行端回填，不假定就是 MiniMax-M3。

## 给 Minimaxcode 的启动指令

读取 AGENTS/override、四份开发基线、specs索引/对应冻结规格、当前 [深入审计](../reviews/DEEP-AUDIT-2026-10-08.md) 和下方单包。保留全部未提交/未跟踪成果；不reset、不提交、不推送、不部署，不改 sources/旧migration/历史证据，不启动 V2。先回传实际模型、HEAD/输入SHA、任务ID、精确写集及先要复现的失败；完成一包就回传RESULT供Codex复核，不自行DONE或穿透依赖。

可以从 **MM-R01 或 MM-R04** 开始。数据库 R01→R02→R03 必须串行，只有当前包持有 `persistence-sqlite/src/lib.rs`。R04、R05、R06 的文件互斥，可按执行端能力独立并行；任何新增/冲突写集先回传，不擅自扩展。根manifest/lockfile始终由Codex串行整合。

## 第一阶段：既有缺陷整改与审阅材料

“可执行”是已存在节点的缺陷修复授权，不是将节点改为READY/DONE；方案包只写方案，条件包须等其明确门槛。修复通过后由Codex核查专属标准、全部依赖和完整当前Mac门禁，才能恢复正式状态。

| 包 | DEV关联 | 风险 | 交付 | 开工范围/先后 |
|---|---|---|---|---|
| [MM-R01](task-packets/minimaxcode-2026-10-08/MM-R01.md) | DEV-02-08 | R3 / P1 | 修复 DRAFT/投递的 Team 一致性 | 既有缺陷修复可执行 |
| [MM-R02](task-packets/minimaxcode-2026-10-08/MM-R02.md) | DEV-02-05（关联02-06/08/10） | R3 / P1 | 补齐核心历史留存与错误测试期望 | 既有缺陷修复可执行 |
| [MM-R03](task-packets/minimaxcode-2026-10-08/MM-R03.md) | DEV-02-05 | R3 / P1 | 限制旧 Run 的非法状态倒退 | 既有缺陷修复可执行 |
| [MM-R04](task-packets/minimaxcode-2026-10-08/MM-R04.md) | DEV-01-08 | R3 / P1 | 修复百分号转义脱敏漏检 | 既有缺陷修复可执行 |
| [MM-R05](task-packets/minimaxcode-2026-10-08/MM-R05.md) | DEV-03-03（关联03-02/04/05） | R3 / P2 | 恢复严格 JSON-RPC stdout 边界 | 既有缺陷修复可执行 |
| [MM-R06](task-packets/minimaxcode-2026-10-08/MM-R06.md) | DEV-01-05（关联01-03） | R3 证据 / P1 | 补齐 Gate 输入与配套证据机制 | 本地脚本修复可执行；在线运行待独立授权 |
| [MM-R07](task-packets/minimaxcode-2026-10-08/MM-R07.md) | DEV-02-08（后续05-07） | R3 方案 | 制定追加 route override 的兼容方案 | 方案可执行；实现待总控签发 |
| [MM-R08](task-packets/minimaxcode-2026-10-08/MM-R08.md) | DEV-02-09（后续05-08） | R3 方案 | 整理通知与 Attention 的决策及验收表 | 只读方案可执行；语义实现等待Human |
| [MM-R09](task-packets/minimaxcode-2026-10-08/MM-R09.md) | DEV-01-05（关联01-08、02-01/11） | R0 文档 / R3 验收 | 整理当前可复核验收与文档补充 | 离线资料/文档可执行；工具下载与在线Gate待授权 |
| [MM-R10](task-packets/minimaxcode-2026-10-08/MM-R10.md) | DEV-02-11（关联02-12/13/14、03-06） | R2测试 / R3复核 | domain86.80%覆盖缺口：有意义的边界/serde测试 | 既有测试补充可执行，依赖保持BLOCKED |

## 第二阶段：收口真实内核（条件计划，尚未签发实现）

| 次序 | DEV | 实际交付要求 | 前置/验收 |
|---|---|---|---|
| K0 | 02-08/09 | route override历史及通知决策落地；原Message/旧attempt保留 | R01/R02、R07方案审阅；ADR0007适用条款经Human确认；24+前向升级/回滚和同Team、Owner、mandatory/TTL/重复投递反例 |
| K1 | 02-11/12 | 保留纯机成果，复核更新后的依赖与条件映射 | 02-02～10重新验收；合法/非法边及稳定码、源码无漂移；不能将metadata lookup当已完成前置事实校验 |
| K2 | 02-13 | 可信Actor/Team/Role/Task/Path/Capability/Approval resolver，查询可见性与拒绝Event；缓存identity隔离+时间 | 00-05与02-02～10DONE；真实SQLite policy/grant查询、未检查默认拒绝、原子消费、防自批/旧Binding；不是UI/Adapter传来的allow bool |
| K3 | 02-14 | 应用事务端口、条件版本更新、首次结果、Event+Outbox原子提交、真实dispatcher/消费者去重及PROCESSING恢复 | 02-07、02-11～13DONE；两个真实连接竞争、事务前后故障、ACK前后崩溃、deadline/TTL/不可重试分类 |
| K4 | 03-06/07 | 实际Mock取消确认/超时/fence，唯一新Run重试；每次callback最终写入读取当前Run/Task/Binding/lease/token/permission/sequence | 各节点冻结依赖DONE；t0 token7/t1换代8/t2旧7 heartbeat/output/completed全部零业务写入拒绝，有适用审计；不把spawn snapshot当当前授权 |
| K5 | 02-15、03-08/09 | 一致性备份/旧版本升级/损坏恢复；UNKNOWN重连不重复派发；凭据仅引用、最小生命周期注入 | 按下表直接依赖；仅模拟/新建测试secret，不访问生产凭据；Keychain权限/真实服务/下载须独立授权 |

K2必须逐项说明36×6矩阵与DB动作/资源、Role分类和可信身份的事实来源，不能从role.name、Harness名或IPC声明推导权限；缺少持久字段/冻结映射就回传具体迁移或决策方案。K2～K4先由Minimaxcode提出有界端口/事务和测试设计，Codex审阅后签发精确写集。application只依赖domain及端口，不能直接引SQLx/Tauri/Git/具体Harness；基础设施实现端口，host装配。所有前置条件在条件写入前验证，并与效果/Event/Outbox同事务；失败不改变业务资源。若节点与实际依赖形成语义循环，回传具体冲突及最小边界方案，不能借整改提前实现DEV-04/05。

## 第三阶段：全部未实现节点的条件开发表

以下60个节点均为PLANNED；本表只规划，不签发新实现。必须逐节点满足冻结直接依赖、Wave 2出口与具体任务包，不能凭表中出现了名字立即开工。各包主责为Minimaxcode，R3由Codex独立复核；下表风险按冻结模型历史参考分类，正式写集在开工前签发。

| DEV | 工作 | 冻结直接依赖 | 风险/审核 | 当前 |
|---|---|---|---|---|
| DEV-02-15 | 实现备份、迁移升级与恢复验证 | DEV-02-02, DEV-02-03, DEV-02-04, DEV-02-05, DEV-02-06, DEV-02-07, DEV-02-08, DEV-02-09, DEV-02-10, DEV-02-14 | R3 / Codex独立反例 | 条件排期，未开工 |
| DEV-03-08 | 实现启动重连与 Run reconcile | DEV-02-15, DEV-03-02, DEV-03-03, DEV-03-04, DEV-03-05, DEV-03-06, DEV-03-07 | R3 / Codex独立反例 | 条件排期，未开工 |
| DEV-03-09 | 接入 OS Keychain 的 CredentialReference | DEV-02-10, DEV-03-02 | R3 / Codex独立反例 | 条件排期，未开工 |
| DEV-03-10 | Codex Adapter 技术 Spike | DEV-03-01, DEV-03-02, DEV-03-03, DEV-03-04, DEV-03-05, DEV-03-06, DEV-03-07, DEV-03-08, DEV-03-09 | R3 / Codex独立反例 | 条件排期，未开工 |
| DEV-03-11 | Adapter Spike Go/No-Go 与第二 Harness 决策 | DEV-03-10 | R3 / Codex独立反例 | 条件排期，未开工 |
| DEV-04-01 | Workspace 绑定、路径规范化与可用性检查 | DEV-02-02, DEV-02-13 | R3 / Codex独立反例 | 条件排期，未开工 |
| DEV-04-02 | Markdown 文件/标题锚点引用解析 | DEV-02-06, DEV-04-01 | R2 / Codex工程复核 | 条件排期，未开工 |
| DEV-04-03 | Git 状态、基线 Commit 与只读元数据 | DEV-04-01 | R2 / Codex工程复核 | 条件排期，未开工 |
| DEV-04-04 | 每 Run branch/worktree 生命周期 | DEV-03-07, DEV-04-01 | R3 / Codex独立反例 | 条件排期，未开工 |
| DEV-04-05 | 非 Git Workspace 单写者锁 | DEV-03-07, DEV-04-01 | R3 / Codex独立反例 | 条件排期，未开工 |
| DEV-04-06 | 文件写入路径授权与 fencing | DEV-02-13, DEV-03-07, DEV-04-01 | R3 / Codex独立反例 | 条件排期，未开工 |
| DEV-04-07 | Artifact 登记、校验和预览元数据 | DEV-02-06, DEV-04-02, DEV-04-03, DEV-04-04, DEV-04-05, DEV-04-06 | R2 / Codex工程复核 | 条件排期，未开工 |
| DEV-04-08 | 基线过期、同文件冲突与集成任务提示 | DEV-02-04, DEV-04-03, DEV-04-04, DEV-04-05, DEV-04-06, DEV-04-07 | R3 / Codex独立反例 | 条件排期，未开工 |
| DEV-05-01 | Team/Role/Workspace 生命周期命令与查询 | DEV-02-02, DEV-04-01 | R2 / Codex工程复核 | 条件排期，未开工 |
| DEV-05-02 | Harness 注册、健康、诊断与禁用用例 | DEV-02-03, DEV-03-03, DEV-03-09 | R2 / Codex工程复核 | 条件排期，未开工 |
| DEV-05-03 | Binding 预检、三种换绑和历史查询 | DEV-02-03, DEV-02-11, DEV-03-08 | R3 / Codex独立反例 | 条件排期，未开工 |
| DEV-05-04 | Task 创建、分配、依赖、领取与迁移 | DEV-02-04, DEV-02-11, DEV-02-12, DEV-02-13, DEV-02-14, DEV-04-02 | R3 / Codex独立反例 | 条件排期，未开工 |
| DEV-05-05 | Run 启动、取消、重试、对账与接管 | DEV-02-05, DEV-03-06, DEV-03-07, DEV-03-08, DEV-04-04, DEV-04-05, DEV-04-06 | R3 / Codex独立反例 | 条件排期，未开工 |
| DEV-05-06 | Review 提交、通过、退回与重开 | DEV-02-05, DEV-02-06, DEV-02-11 | R2 / Codex工程复核 | 条件排期，未开工 |
| DEV-05-07 | Thread/Message 发送、回复、确认、重试与改派 | DEV-02-08, DEV-02-12, DEV-02-13, DEV-02-14, DEV-05-03 | R3 / Codex独立反例 | 条件排期，未开工 |
| DEV-05-08 | NotificationPolicy、Notification 与 AttentionItem | DEV-02-09, DEV-02-12, DEV-02-13, DEV-02-14 | R3 / Codex独立反例 | 条件排期，未开工 |
| DEV-05-09 | Approval 创建、批准、拒绝与失效 | DEV-02-10, DEV-02-12, DEV-02-13, DEV-02-14 | R3 / Codex独立反例 | 条件排期，未开工 |
| DEV-05-10 | Activity/Audit 查询、游标分页与导出任务 | DEV-02-07, DEV-02-15 | R2 / Codex工程复核 | 条件排期，未开工 |
| DEV-05-11 | 页面 read model 与 IPC 契约集成 | DEV-01-07, DEV-05-01, DEV-05-02, DEV-05-03, DEV-05-04, DEV-05-05, DEV-05-06, DEV-05-07, DEV-05-08, DEV-05-09, DEV-05-10 | R2 / Codex工程复核 | 条件排期，未开工 |
| DEV-06-02 | Button/Input/Dialog/Table/Status 等基础组件 | DEV-06-01 | R2 / Codex工程复核 | 条件排期，未开工 |
| DEV-06-03 | 正式应用壳、导航、错误边界与断连条 | DEV-05-11, DEV-06-01, DEV-06-02 | R2 / Codex工程复核 | 条件排期，未开工 |
| DEV-06-04 | Team/Workspace 创建、编辑与归档界面 | DEV-05-01, DEV-06-02, DEV-06-03 | R2 / Codex工程复核 | 条件排期，未开工 |
| DEV-06-05 | 团队概览与 Role 席位 | DEV-05-01, DEV-05-02, DEV-05-03, DEV-06-03 | R2 / Codex工程复核 | 条件排期，未开工 |
| DEV-06-06 | Task 看板、列表、筛选与空状态 | DEV-05-04, DEV-06-02, DEV-06-03 | R2 / Codex工程复核 | 条件排期，未开工 |
| DEV-06-07 | Task 创建/编辑真实表单 | DEV-05-04, DEV-06-02, DEV-06-03 | R2 / Codex工程复核 | 条件排期，未开工 |
| DEV-06-08 | Task 详情与控制页 | DEV-05-04, DEV-05-05, DEV-05-06, DEV-05-11, DEV-06-03 | R3 / Codex独立反例 | 条件排期，未开工 |
| DEV-06-09 | Run 详情、失败、UNKNOWN、重试与接管界面 | DEV-05-05, DEV-06-02, DEV-06-03 | R3 / Codex独立反例 | 条件排期，未开工 |
| DEV-06-10 | Review 提交、验收通过与退回界面 | DEV-05-06, DEV-06-02, DEV-06-03 | R2 / Codex工程复核 | 条件排期，未开工 |
| DEV-06-11 | Workspace 文件、Markdown、Artifact 与引用修复 | DEV-04-02, DEV-04-03, DEV-04-04, DEV-04-05, DEV-04-06, DEV-04-07, DEV-04-08, DEV-05-11, DEV-06-03 | R2 / Codex工程复核 | 条件排期，未开工 |
| DEV-06-12 | Role/Binding 历史与三种换绑流程 | DEV-05-03, DEV-06-02, DEV-06-03 | R3 / Codex独立反例 | 条件排期，未开工 |
| DEV-06-13 | Harness 注册、健康检查与诊断界面 | DEV-05-02, DEV-06-02, DEV-06-03 | R2 / Codex工程复核 | 条件排期，未开工 |
| DEV-06-14 | Message/Thread、发送、回复和投递终态 | DEV-05-07, DEV-06-02, DEV-06-03 | R3 / Codex独立反例 | 条件排期，未开工 |
| DEV-06-15 | NotificationPolicy 与“待我处理”队列 | DEV-05-08, DEV-05-09, DEV-06-02, DEV-06-03 | R3 / Codex独立反例 | 条件排期，未开工 |
| DEV-06-16 | Activity Stream、审计详情与导出状态 | DEV-05-10, DEV-06-02, DEV-06-03 | R2 / Codex工程复核 | 条件排期，未开工 |
| DEV-06-17 | 响应式、键盘、读屏与减少动态效果收口 | DEV-06-04, DEV-06-05, DEV-06-06, DEV-06-07, DEV-06-08, DEV-06-09, DEV-06-10, DEV-06-11, DEV-06-12, DEV-06-13, DEV-06-14, DEV-06-15, DEV-06-16 | R3 / Codex独立反例 | 条件排期，未开工 |
| DEV-07-01 | Team→Role→Harness→Workspace 初始化闭环 | DEV-05-01, DEV-05-02, DEV-05-03, DEV-06-04, DEV-06-05, DEV-06-12, DEV-06-13 | R2 / Codex工程复核 | 条件排期，未开工 |
| DEV-07-02 | 文件驱动 Task→Run→Review→DONE 主链 | DEV-05-04, DEV-05-05, DEV-05-06, DEV-06-06, DEV-06-07, DEV-06-08, DEV-06-09, DEV-06-10, DEV-06-11 | R3 / Codex独立反例 | 条件排期，未开工 |
| DEV-07-03 | 双 Task 并行 worktree 与冲突闭环 | DEV-03-07, DEV-04-04, DEV-04-08, DEV-06-08, DEV-06-09, DEV-06-10, DEV-06-11 | R3 / Codex独立反例 | 条件排期，未开工 |
| DEV-07-04 | 依赖 DAG、循环拒绝和下游通知 | DEV-02-04, DEV-05-04, DEV-05-08, DEV-06-06, DEV-06-07, DEV-06-08 | R3 / Codex独立反例 | 条件排期，未开工 |
| DEV-07-05 | Harness 崩溃、失败、超时与幂等重试 | DEV-03-05, DEV-03-06, DEV-03-07, DEV-03-08, DEV-05-05, DEV-06-09 | R3 / Codex独立反例 | 条件排期，未开工 |
| DEV-07-06 | 三种换绑与上下文继承 | DEV-05-03, DEV-05-07, DEV-06-12, DEV-06-14 | R3 / Codex独立反例 | 条件排期，未开工 |
| DEV-07-07 | 越权、审批、凭据脱敏闭环 | DEV-02-13, DEV-03-09, DEV-05-09, DEV-06-15 | R3 / Codex独立反例 | 条件排期，未开工 |
| DEV-07-08 | 平台重启、UNKNOWN 对账与恢复 | DEV-02-15, DEV-03-08, DEV-05-05, DEV-06-09 | R3 / Codex独立反例 | 条件排期，未开工 |
| DEV-07-09 | Human 暂停、接管、改派与取消 | DEV-05-05, DEV-06-08, DEV-06-09, DEV-06-15 | R3 / Codex独立反例 | 条件排期，未开工 |
| DEV-07-10 | Message 跨换绑、失败/过期与改派 | DEV-05-07, DEV-05-08, DEV-06-14, DEV-06-15 | R3 / Codex独立反例 | 条件排期，未开工 |
| DEV-07-11 | Task 到 Event/Binding/Run/Artifact/Review 的审计回放 | DEV-05-10, DEV-06-16, DEV-07-02, DEV-07-03, DEV-07-04, DEV-07-05, DEV-07-06, DEV-07-07, DEV-07-08, DEV-07-09, DEV-07-10 | R3 / Codex独立反例 | 条件排期，未开工 |
| DEV-07-12 | 断连、引用失效、版本冲突与恢复 UX | DEV-06-03, DEV-06-07, DEV-06-08, DEV-06-09, DEV-06-10, DEV-06-11, DEV-06-12, DEV-06-13, DEV-06-14, DEV-06-15, DEV-06-16, DEV-06-17 | R2 / Codex工程复核 | 条件排期，未开工 |
| DEV-08-01 | 完成需求追踪与 AC-01～12 自动验收套件 | DEV-07-01, DEV-07-02, DEV-07-03, DEV-07-04, DEV-07-05, DEV-07-06, DEV-07-07, DEV-07-08, DEV-07-09, DEV-07-10, DEV-07-11, DEV-07-12 | R3 / Codex独立反例 | 条件排期，未开工 |
| DEV-08-02 | 1 万 Task/Event 与 5 并发 Run 性能基准 | DEV-07-01, DEV-07-02, DEV-07-03, DEV-07-04, DEV-07-05, DEV-07-06, DEV-07-07, DEV-07-08, DEV-07-09, DEV-07-10, DEV-07-11, DEV-07-12 | R3 / Codex独立反例 | 条件排期，未开工 |
| DEV-08-03 | 安全测试与威胁模型复核 | DEV-07-03, DEV-07-04, DEV-07-05, DEV-07-06, DEV-07-07, DEV-07-08, DEV-07-09, DEV-07-10 | R3 / Codex独立反例 | 条件排期，未开工 |
| DEV-08-04 | 可访问性与窄屏真实设备验收 | DEV-06-17, DEV-07-12 | R2 / Codex工程复核 | 条件排期，未开工 |
| DEV-08-05 | 备份、升级、回滚与崩溃恢复演练 | DEV-02-15, DEV-07-05, DEV-07-08 | R3 / Codex独立反例 | 条件排期，未开工 |
| DEV-08-06 | macOS 安装、签名前检查与最小权限复核 | DEV-01-06, DEV-08-01, DEV-08-02, DEV-08-03, DEV-08-04, DEV-08-05 | R3 / Codex独立反例 | 条件排期，未开工 |
| DEV-08-07 | 文档、运维手册与 Adapter 接入说明校准 | DEV-08-01, DEV-08-02, DEV-08-03, DEV-08-04, DEV-08-05, DEV-08-06 | R2 / Codex工程复核 | 条件排期，未开工 |
| DEV-08-08 | 候选版本 Go/No-Go 评审 | DEV-08-01, DEV-08-02, DEV-08-03, DEV-08-04, DEV-08-05, DEV-08-06, DEV-08-07 | R3 / Codex独立反例 | 条件排期，未开工 |

实施主线是：备份/恢复与Adapter准入 → Workspace路径/隔离/写入fencing → 应用命令/查询与typed IPC → 真实业务UI → AC-01～12纵向及故障 → macOS系统/候选版本。可并行仅限已经满足依赖、写集互斥的包。Figma只决定体验/视觉，不代替领域状态或完成事实；暂不开展V2、其他平台发布、自动部署或自动合并。

## 当前待Human/环境事项

- ADR-0007尚待明确决定；本轮没有把“Minimaxcode主责”解释为接受通知新语义。
- 完整Mac21项含在线npm/RustSec审计和可能的依赖获取：会向公开审计服务发送依赖名称/版本或获取公告/工具，不上传源码/凭据；此前授权问题未答复，未执行。先交本地可审阅实现和证据，再由Human决定外部操作。
- 已用现有Apple LLVM工具离线测得domain行覆盖率86.80%，低于90%；优先按coverage明细补有意义的前置/拒绝/serde/边界测试，不靠排除生产文件或只刷getter达标。application仍占位，分支85%及全体Rust/TS80%未验证。复测可用本批现有runner，所需新增工具/下载仍须独立确认。
- 真实Harness/Keychain、最低macOS、安装/签名/公证、外部账号、费用与发布各保留其独立准入，不由任务表自动授权。

每包回传保留命令/exit、完整日志、合法对照、RED/GREEN、源码与输入SHA、升级/回滚、未运行及风险。只有Codex可裁定正式节点DONE。优先关闭本轮真实反例，避免只继续加验证验证器而不接实际执行链。
