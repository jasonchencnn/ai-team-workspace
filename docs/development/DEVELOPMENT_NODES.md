# 细化开发节点、依赖与模型路由

> 版本：1.0  
> 原则：节点按依赖推进；每个节点必须同时通过本表“审核标准”和 `QUALITY_GATES.md` 的通用门禁。

## 1. 模型分配规则

| 复杂度 | 执行模型 | 使用范围 |
|---|---|---|
| 复杂 | `gpt-5.6-sol` | 架构、领域不变量、状态机、权限、安全、并发、恢复、协议、关键 E2E 与 Go/No-Go。 |
| 普通 | `gpt-5.6-terra` | 边界清晰的业务实现、CRUD、页面、查询、集成测试和常规修复。 |
| 简单 | `gpt-5.6-luna` | 脚手架、Token 搬运、生成物校验、静态页面骨架、文档和机械性测试补齐。 |

模型不能因“改动行数少”自动降级：触及权限、凭据、迁移、租约/fencing、审计、恢复或协议兼容的节点一律使用 `gpt-5.6-sol`。若一个节点实际范围扩大，先拆节点或升级模型，不得降格执行。

## 2. 总体依赖

```text
DEV-00 决策与契约冻结
  ├── DEV-01 工程底座
  ├── DEV-02 领域与持久化
  └── DEV-03 Adapter Spike
          ↓
DEV-04 Workspace / Artifact
          ↓
DEV-05 应用用例与 IPC
          ↓
DEV-06 前端体验
          ↓
DEV-07 纵向闭环与异常恢复
          ↓
DEV-08 系统验收与候选发布
```

### 当前执行状态

| 范围 | 状态 | 证据 |
|---|---|---|
| DEV-00-01～10 | `DONE` | `docs/reviews/DEV-00-FREEZE-REVIEW.md` |
| DEV-01-01～04、DEV-01-06～07、DEV-02-01、DEV-06-01 | `DONE` | Wave 1 独立复核中保持通过 |
| DEV-03-01 | `DONE` | ADR-0002 机器约束补强、全部协议反例与 Wave 1 独立复核通过 |
| 其余 DEV-01～08 | `PLANNED` | Wave 2 尚未启动；后续仍须按 Wave 2～7 和依赖顺序执行 |

## 3. DEV-00：决策、语义与契约冻结

阶段出口：产品、设计、数据和协议不再各自解释同一状态；所有后续实现有可引用的规则表。

| 节点 | 工作内容 | 依赖 | 模型 | 节点审核标准 |
|---|---|---|---|---|
| DEV-00-01 | 建立 PRD FR/AC—审计项—Figma 节点追踪矩阵 | 无 | `gpt-5.6-terra` | FR-001～054、AC-01～12 和全部 P0 审计项均有唯一负责人节点；无孤儿需求。 |
| DEV-00-02 | 冻结领域词汇与聚合边界 | 无 | `gpt-5.6-sol` | Team/Role/Binding/Harness/Task/Run/Artifact/Event/Message/Notification/Approval 定义无循环或同义冲突。 |
| DEV-00-03 | 冻结 Task/Run/Binding 状态迁移表 | 00-02 | `gpt-5.6-sol` | 每条迁移含 actor、前置条件、副作用、Event、错误码；PRD 全部合法/非法迁移可映射。 |
| DEV-00-04 | 冻结 Message/Delivery/Notification 状态与边界 | 00-02 | `gpt-5.6-sol` | Human Notification 与 Role Message 分离；重试、TTL、改派、换绑快照和终态无歧义。 |
| DEV-00-05 | 冻结权限矩阵与 Human-only 动作 | 00-02 | `gpt-5.6-sol` | 每个命令列出主体、资源、路径、上下文和拒绝码；Harness 无自提权路径。 |
| DEV-00-06 | 冻结 ID、时间、版本、归档与留存规则 | 00-02 | `gpt-5.6-terra` | UUIDv7、显示 ID、UTC、expected_version、归档与不可变记录规则能落到数据字典。 |
| DEV-00-07 | 冻结错误码目录和用户提示映射 | 00-03～05 | `gpt-5.6-terra` | 认证、权限、冲突、非法状态、依赖、Harness、Workspace、系统错误可稳定区分且不泄密。 |
| DEV-00-08 | 冻结 Adapter V0 方法、事件和兼容策略 | 00-02～07 | `gpt-5.6-sol` | JSON Schema 可验证；初始化、健康、启动、心跳、取消、对账、结束与失败语义完整。 |
| DEV-00-09 | 冻结 Figma Token、正式导航和响应式断点 | 00-01 | `gpt-5.6-terra` | 覆盖 36 画板核心入口；Harness、活动、依赖、执行记录、设置均有正式导航归属。 |
| DEV-00-10 | 评审并接受实施 ADR | 00-01～09 | `gpt-5.6-sol` | ADR 明确选择、代价、否决方案和复审触发条件；Human 确认后才进入代码实现。 |

## 4. DEV-01：工程底座与可重复开发环境

阶段出口：新机器能按单一命令完成安装、检查、测试和桌面壳启动；生成契约与手写代码边界清晰。

| 节点 | 工作内容 | 依赖 | 模型 | 节点审核标准 |
|---|---|---|---|---|
| DEV-01-01 | 创建 pnpm workspace、Cargo workspace 与目录骨架 | 00-10 | `gpt-5.6-luna` | 结构符合技术基线；无业务实现；空工作区 build/test 成功。 |
| DEV-01-02 | 固定 Node、pnpm、Rust 工具链和 lockfile | 01-01 | `gpt-5.6-luna` | 干净环境版本一致；未锁依赖和跨 major 漂移被 CI 拒绝。 |
| DEV-01-03 | 配置 fmt/lint/typecheck/clippy 基线 | 01-01 | `gpt-5.6-luna` | TS strict 与 Rust `-D warnings` 生效；故意违规样例能让检查失败。 |
| DEV-01-04 | 建立测试分层与统一命令 | 01-01 | `gpt-5.6-terra` | unit/integration/contract/e2e 可独立运行；退出码与报告稳定。 |
| DEV-01-05 | 建立 macOS + Ubuntu CI | 01-02～04 | `gpt-5.6-terra` | 缓存不影响正确性；格式、lint、类型、测试、依赖扫描均为 required gate。 |
| DEV-01-06 | 创建 Tauri/React 最小桌面壳 | 01-01～03 | `gpt-5.6-terra` | macOS 可启动/关闭；Tauri capability 最小化；无通用 shell/文件权限。 |
| DEV-01-07 | 建立 Rust Schema → TypeScript 类型生成 | 01-01～04 | `gpt-5.6-sol` | 单一来源可重现；生成物漂移使 CI 失败；前端无重复手写 DTO。 |
| DEV-01-08 | 建立结构化日志、correlation ID 与脱敏框架 | 01-06 | `gpt-5.6-sol` | request/task/run 可关联；测试秘密在日志、错误、Event 输出中均被遮蔽。 |
| DEV-01-09 | 建立开发 fixture、时钟/ID/随机源注入 | 01-04 | `gpt-5.6-terra` | 测试可重复；不依赖真实时间、用户目录或在线服务。 |

## 5. DEV-02：领域内核、数据模型与一致性

阶段出口：不依赖 UI/Adapter 即可通过领域和数据库测试证明身份、状态、权限、并发与审计规则。

| 节点 | 工作内容 | 依赖 | 模型 | 节点审核标准 |
|---|---|---|---|---|
| DEV-02-01 | 实现强类型 ID、时间、版本与 Actor 值对象 | 01-01、00-06 | `gpt-5.6-terra` | 禁止混用 Role/Task/Run ID；序列化、排序和 UTC 往返无损。 |
| DEV-02-02 | 设计并迁移 Team/Role/Workspace 表 | 02-01 | `gpt-5.6-terra` | 稳定 ID、版本、停用/归档、唯一约束和外键有 migration 测试。 |
| DEV-02-03 | 设计并迁移 Harness/Adapter/Binding 表 | 02-01 | `gpt-5.6-sol` | 一个 Role 最多一个有效主 Binding；历史有效区间不可重叠。 |
| DEV-02-04 | 设计并迁移 Task/Dependency 表 | 02-01 | `gpt-5.6-sol` | Task 状态、owner Role、引用、版本、依赖约束完整；显示 ID Team 内唯一。 |
| DEV-02-05 | 设计并迁移 Run/Lease/Review 表 | 02-01、02-04 | `gpt-5.6-sol` | active Run、租约、fencing、尝试序号和 Review 约束能在事务中验证。 |
| DEV-02-06 | 设计并迁移 Artifact/Reference 表 | 02-01、02-04～05 | `gpt-5.6-terra` | creator Role、Run、版本、URI、commit/checksum 与可见范围可追溯。 |
| DEV-02-07 | 设计并迁移 Event/Outbox 表 | 02-01 | `gpt-5.6-sol` | 业务变更与 Event/Outbox 原子提交；消费可重放和去重；Event 无更新接口。 |
| DEV-02-08 | 设计并迁移 Thread/Message/Delivery 表 | 02-01、00-04 | `gpt-5.6-sol` | `to_role_id` 强制存在；Binding 快照、reply_to、TTL、幂等和 attempt 历史完整。 |
| DEV-02-09 | 设计并迁移 Notification/Policy/Attention 表 | 02-01、02-07～08 | `gpt-5.6-sol` | Human 接收者不伪装成 Role；Event、Message、通知和待处理项可互相引用。 |
| DEV-02-10 | 设计并迁移 Permission/Approval 表 | 02-01 | `gpt-5.6-sol` | 资源、动作、路径、有效期、审批人和范围可表达最小权限。 |
| DEV-02-11 | 实现 Task/Run/Binding 纯状态机 | 00-03、02-02～05 | `gpt-5.6-sol` | 全部迁移有表驱动与性质测试；非法迁移零状态副作用。 |
| DEV-02-12 | 实现 Message/Delivery/Approval 状态机 | 00-04～05、02-08～10 | `gpt-5.6-sol` | RETRYING 瞬态正确回流；FAILED/EXPIRED/NEEDS_HUMAN 不丢历史。 |
| DEV-02-13 | 实现集中式权限求交与拒绝原因 | 00-05、02-02～10 | `gpt-5.6-sol` | Team∩Role∩Task∩Path∩Capability∩Approval 生效；拒绝不改变资源并写 Event。 |
| DEV-02-14 | 实现乐观锁、幂等命令记录与 Outbox dispatcher | 02-07、02-11～13 | `gpt-5.6-sol` | 重复命令返回同一结果；payload 冲突拒绝；版本竞争仅一个成功；事件至少一次且可去重。 |
| DEV-02-15 | 实现备份、迁移升级与恢复验证 | 02-02～10、02-14 | `gpt-5.6-sol` | 空库升级、旧版迁移、运行中一致性备份与损坏恢复演练有证据。 |

## 6. DEV-03：Adapter 契约与运行时 Spike

阶段出口：Mock Adapter 可在受控进程中完成领取—心跳—结果/失败—取消—对账；真实 Codex 接入风险有数据支撑。

| 节点 | 工作内容 | 依赖 | 模型 | 节点审核标准 |
|---|---|---|---|---|
| DEV-03-01 | 发布 Adapter V0 JSON Schema 与示例 | 00-08、01-07 | `gpt-5.6-sol` | 正反例通过 Schema 测试；版本兼容、未知字段和错误 envelope 已定义。 |
| DEV-03-02 | 实现 Adapter 子进程 supervisor | 03-01、01-08 | `gpt-5.6-sol` | 启动、超时、退出、stderr、关闭与孤儿进程清理可控；无 shell 拼接。 |
| DEV-03-03 | 实现 initialize/能力/健康检查 | 03-02 | `gpt-5.6-terra` | 不兼容版本、缺能力、未认证和离线可稳定诊断并留痕。 |
| DEV-03-04 | 实现 Mock Adapter 正常执行流 | 03-01～03 | `gpt-5.6-terra` | start→heartbeat→output→completed 顺序可重复验证，产物用引用返回。 |
| DEV-03-05 | 实现 Mock 故障注入 | 03-04 | `gpt-5.6-sol` | 支持超时、重复、乱序、崩溃、晚到回调、拒绝取消和失联场景。 |
| DEV-03-06 | 实现取消、超时、重试与退避 | 03-02～05、02-11、02-14 | `gpt-5.6-sol` | 取消有确认/超时边界；自动重试只覆盖瞬时错误；每次新 Run。 |
| DEV-03-07 | 实现心跳、租约续期与 fencing 校验 | 03-04～05、02-05、02-11 | `gpt-5.6-sol` | 旧 token 的心跳、输出和完成都被拒；并发与恢复测试无双主 Run。 |
| DEV-03-08 | 实现启动重连与 Run reconcile | 03-02～07、02-15 | `gpt-5.6-sol` | 重启先 UNKNOWN；可恢复/失败/需 Human 三类结果稳定且不重复执行。 |
| DEV-03-09 | 接入 OS Keychain 的 CredentialReference | 03-02、02-10 | `gpt-5.6-sol` | 数据库和 UI 只见引用；秘密只注入目标进程且日志/诊断无明文。 |
| DEV-03-10 | Codex Adapter 技术 Spike | 03-01～09 | `gpt-5.6-sol` | 真实验证启动、输入、状态、取消、结果、配置隔离和恢复；列出不可验证项。 |
| DEV-03-11 | Adapter Spike Go/No-Go 与第二 Harness 决策 | 03-10 | `gpt-5.6-sol` | 以测量证据决定继续/调整；任何契约变更先回写 ADR/Schema/契约测试。 |

## 7. DEV-04：Workspace、Git 隔离与 Artifact

阶段出口：任务引用可靠定位，Run 写入受控且互不覆盖，结果能形成完整 Artifact 事实链。

| 节点 | 工作内容 | 依赖 | 模型 | 节点审核标准 |
|---|---|---|---|---|
| DEV-04-01 | Workspace 绑定、路径规范化与可用性检查 | 02-02、02-13 | `gpt-5.6-sol` | 相对路径、symlink、无权限、缺失、跨 Team 越界均被安全处理。 |
| DEV-04-02 | Markdown 文件/标题锚点引用解析 | 04-01、02-06 | `gpt-5.6-terra` | 中文标题、重复标题、文件移动和失效引用有确定行为与错误。 |
| DEV-04-03 | Git 状态、基线 Commit 与只读元数据 | 04-01 | `gpt-5.6-terra` | dirty/branch/commit 可查询；查询不修改用户仓库。 |
| DEV-04-04 | 每 Run branch/worktree 生命周期 | 04-01、03-07 | `gpt-5.6-sol` | 并行 Run 目录独立；创建失败可回滚；清理不误删用户分支/改动。 |
| DEV-04-05 | 非 Git Workspace 单写者锁 | 04-01、03-07 | `gpt-5.6-sol` | 第二写者被拒并生成冲突 Event；崩溃后的锁恢复不会双写。 |
| DEV-04-06 | 文件写入路径授权与 fencing | 04-01、02-13、03-07 | `gpt-5.6-sol` | 未授权路径、旧 token、越界 symlink 写入均拒绝且不泄密。 |
| DEV-04-07 | Artifact 登记、校验和预览元数据 | 04-02～06、02-06 | `gpt-5.6-terra` | document/code/test/build/decision/question 均可追到 Role/Run/版本。 |
| DEV-04-08 | 基线过期、同文件冲突与集成任务提示 | 04-03～07、02-04 | `gpt-5.6-sol` | 不静默覆盖；陈旧结果、合并冲突和上游重开均产生可操作状态。 |

## 8. DEV-05：应用用例、读模型与类型化 IPC

阶段出口：所有核心业务都可通过稳定命令/查询完成，UI 无需直碰数据库、Git 或 Adapter。

| 节点 | 工作内容 | 依赖 | 模型 | 节点审核标准 |
|---|---|---|---|---|
| DEV-05-01 | Team/Role/Workspace 生命周期命令与查询 | 02-02、04-01 | `gpt-5.6-terra` | 创建、编辑、停用、恢复、归档完整；运行中 Team 不可误归档。 |
| DEV-05-02 | Harness 注册、健康、诊断与禁用用例 | 02-03、03-03、03-09 | `gpt-5.6-terra` | 凭据不回显；健康与诊断结果可追踪且错误分类稳定。 |
| DEV-05-03 | Binding 预检、三种换绑和历史查询 | 02-03、02-11、03-08 | `gpt-5.6-sol` | 完成后换绑/取消重开/接管分支均不改 role_id、不篡改历史。 |
| DEV-05-04 | Task 创建、分配、依赖、领取与迁移 | 02-04、02-11～14、04-02 | `gpt-5.6-sol` | TODO 不可领取；引用/依赖/权限/版本在同一命令边界校验。 |
| DEV-05-05 | Run 启动、取消、重试、对账与接管 | 02-05、03-06～08、04-04～06 | `gpt-5.6-sol` | 重试幂等、旧 token 失效、UNKNOWN 对账和人工身份留痕成立。 |
| DEV-05-06 | Review 提交、通过、退回与重开 | 02-05～06、02-11 | `gpt-5.6-terra` | REVIEW 有 Artifact；DONE 有验收；退回有意见并进入合法状态。 |
| DEV-05-07 | Thread/Message 发送、回复、确认、重试与改派 | 02-08、02-12～14、05-03 | `gpt-5.6-sol` | 路由按投递时有效 Binding；改派权限、TTL、幂等和历史快照正确。 |
| DEV-05-08 | NotificationPolicy、Notification 与 AttentionItem | 02-09、02-12～14 | `gpt-5.6-sol` | 11 类默认通知、去重、升级和待处理聚合均可验证。 |
| DEV-05-09 | Approval 创建、批准、拒绝与失效 | 02-10、02-12～14 | `gpt-5.6-sol` | scope/期限/actor 全校验；拒绝或过期不授予能力；结果驱动正确后续状态。 |
| DEV-05-10 | Activity/Audit 查询、游标分页与导出任务 | 02-07、02-15 | `gpt-5.6-terra` | 可按 Team/Role/Object/Event/Time 查询；导出可取消、校验且无秘密。 |
| DEV-05-11 | 页面 read model 与 IPC 契约集成 | 05-01～10、01-07 | `gpt-5.6-terra` | 每个 Figma 核心页有最小 read model；无 N+1 热点；错误 envelope 一致。 |

## 9. DEV-06：前端设计系统与产品界面

阶段出口：Figma 核心体验成为真实可操作界面，所有写操作由 IPC 领域规则裁决，关键流程桌面/窄屏/键盘可用。

| 节点 | 工作内容 | 依赖 | 模型 | 节点审核标准 |
|---|---|---|---|---|
| DEV-06-01 | 设计 Token、字体、图标与基础主题 | 00-09、01-06 | `gpt-5.6-luna` | 色值/间距/圆角/字号与 Figma 基线一致；无魔法色散落。 |
| DEV-06-02 | Button/Input/Dialog/Table/Status 等基础组件 | 06-01 | `gpt-5.6-terra` | 键盘、焦点、禁用、错误、loading、危险态和 axe 测试通过。 |
| DEV-06-03 | 正式应用壳、导航、错误边界与断连条 | 06-01～02、05-11 | `gpt-5.6-terra` | 所有正式入口可达；断连不伪装实时；错误边界保留可恢复上下文。 |
| DEV-06-04 | Team/Workspace 创建、编辑与归档界面 | 06-02～03、05-01 | `gpt-5.6-terra` | 必填、冲突、路径权限、加载/失败和影响确认完整。 |
| DEV-06-05 | 团队概览与 Role 席位 | 06-03、05-01～03 | `gpt-5.6-terra` | Role 视觉优先，Harness 标注当前绑定；在线状态不冒充任务完成。 |
| DEV-06-06 | Task 看板、列表、筛选与空状态 | 06-02～03、05-04 | `gpt-5.6-terra` | TODO/READY 可识别且 TODO 不可领取；状态、Role、依赖、待处理可筛选。 |
| DEV-06-07 | Task 创建/编辑真实表单 | 06-02～03、05-04 | `gpt-5.6-terra` | owner/reference/dependency/initial status 全可编辑；服务端 field error 精确落位。 |
| DEV-06-08 | Task 详情与控制页 | 06-03、05-04～06、05-11 | `gpt-5.6-sol` | Task/Run 状态分离；引用、依赖、Run、Artifact、活动和控制入口同步真实状态。 |
| DEV-06-09 | Run 详情、失败、UNKNOWN、重试与接管界面 | 06-02～03、05-05 | `gpt-5.6-sol` | 影响、旧租约、新 Run 和 Human 行为清晰；重复点击不产生重复 Run。 |
| DEV-06-10 | Review 提交、验收通过与退回界面 | 06-02～03、05-06 | `gpt-5.6-terra` | 评审意见真实保存；缺 Artifact/权限时不可伪成功。 |
| DEV-06-11 | Workspace 文件、Markdown、Artifact 与引用修复 | 06-03、04-02～08、05-11 | `gpt-5.6-terra` | Task↔文件双向定位；Git 状态、版本、失效/过期/冲突可理解。 |
| DEV-06-12 | Role/Binding 历史与三种换绑流程 | 06-02～03、05-03 | `gpt-5.6-sol` | 运行中影响、待生效状态、旧/新 Binding 时间线和消息路由均准确。 |
| DEV-06-13 | Harness 注册、健康检查与诊断界面 | 06-02～03、05-02 | `gpt-5.6-terra` | 凭据只显示引用；能力/版本/失败诊断与重试入口完整。 |
| DEV-06-14 | Message/Thread、发送、回复和投递终态 | 06-02～03、05-07 | `gpt-5.6-sol` | Role 目标、Task/Reference、reply_to、状态时间线、重试/改派/新消息均真实。 |
| DEV-06-15 | NotificationPolicy 与“待我处理”队列 | 06-02～03、05-08～09 | `gpt-5.6-sol` | 默认 11 类通知可配置；审批、Run/消息异常、决策和验收统一可处理。 |
| DEV-06-16 | Activity Stream、审计详情与导出状态 | 06-02～03、05-10 | `gpt-5.6-terra` | 筛选、分页、关联跳转、导出进行中/成功/失败均有真实数据。 |
| DEV-06-17 | 响应式、键盘、读屏与减少动态效果收口 | 06-04～16 | `gpt-5.6-sol` | 窄屏可处理失败 Run/审批/消息；四个关键流程全键盘完成；WCAG 2.2 AA 证据齐全。 |

## 10. DEV-07：纵向闭环、异常与恢复

阶段出口：用户故事不是页面拼图，而是通过真实 DB、权限、Adapter、Workspace 与 UI 的端到端闭环。

| 节点 | 工作内容 | 依赖 | 模型 | 节点审核标准 |
|---|---|---|---|---|
| DEV-07-01 | Team→Role→Harness→Workspace 初始化闭环 | 05-01～03、06-04～05、06-12～13 | `gpt-5.6-terra` | AC-01/02：未绑定 Role 可存在；绑定后 ID/权限/历史不变。 |
| DEV-07-02 | 文件驱动 Task→Run→Review→DONE 主链 | 05-04～06、06-06～11 | `gpt-5.6-sol` | AC-03/04：引用可读、Event 完整、REVIEW/DONE 条件不可绕过。 |
| DEV-07-03 | 双 Task 并行 worktree 与冲突闭环 | 03-07、04-04、04-08、06-08～11 | `gpt-5.6-sol` | AC-05：不同目录执行；同文件不静默覆盖；出现明确集成/阻塞处理。 |
| DEV-07-04 | 依赖 DAG、循环拒绝和下游通知 | 02-04、05-04、05-08、06-06～08 | `gpt-5.6-sol` | AC-06：前置未完成不可自动执行；完成后 READY+通知；环路可定位。 |
| DEV-07-05 | Harness 崩溃、失败、超时与幂等重试 | 03-05～08、05-05、06-09 | `gpt-5.6-sol` | AC-07：旧 Run/Artifact 保留；新 Run 唯一；晚到结果被 fencing 拒绝。 |
| DEV-07-06 | 三种换绑与上下文继承 | 05-03、05-07、06-12、06-14 | `gpt-5.6-sol` | AC-08：Role/权限/任务/线程不变；新 Harness 从显式启动包继续。 |
| DEV-07-07 | 越权、审批、凭据脱敏闭环 | 02-13、03-09、05-09、06-15 | `gpt-5.6-sol` | AC-09：越权零副作用且有 Event；Human-only 门禁；全部输出无秘密。 |
| DEV-07-08 | 平台重启、UNKNOWN 对账与恢复 | 02-15、03-08、05-05、06-09 | `gpt-5.6-sol` | AC-10：实体完整；未知 Run 不重复派发；三类对账结果可处理。 |
| DEV-07-09 | Human 暂停、接管、改派与取消 | 05-05、06-08～09、06-15 | `gpt-5.6-sol` | AC-11：旧执行写权限撤销；Human actor 留痕；恢复路径明确。 |
| DEV-07-10 | Message 跨换绑、失败/过期与改派 | 05-07～08、06-14～15 | `gpt-5.6-sol` | 新投递走新 Binding；历史快照不改；终态可重试/改派/新建后续。 |
| DEV-07-11 | Task 到 Event/Binding/Run/Artifact/Review 的审计回放 | 05-10、06-16、07-02～10 | `gpt-5.6-sol` | AC-12：任一 DONE Task 可一跳或有限跳数得到完整证据链，导出校验通过。 |
| DEV-07-12 | 断连、引用失效、版本冲突与恢复 UX | 06-03、06-07～17 | `gpt-5.6-terra` | 错误不丢输入、不误报成功；用户能刷新对比、修复引用或安全重试。 |

## 11. DEV-08：系统验收、性能、安全与候选发布

阶段出口：AC-01～AC-12 全绿，质量、安全、恢复与可访问性证据齐全，才允许称为 MVP 候选版本。

| 节点 | 工作内容 | 依赖 | 模型 | 节点审核标准 |
|---|---|---|---|---|
| DEV-08-01 | 完成需求追踪与 AC-01～12 自动验收套件 | 07-01～12 | `gpt-5.6-sol` | 每项 AC 至少一个自动用例和证据链接；失败直接阻断候选版本。 |
| DEV-08-02 | 1 万 Task/Event 与 5 并发 Run 性能基准 | 07-01～12 | `gpt-5.6-sol` | 常用页 P95 <2s、分派可见 P95 <10s；报告含环境、数据、瓶颈与回归阈值。 |
| DEV-08-03 | 安全测试与威胁模型复核 | 07-03～10 | `gpt-5.6-sol` | 路径穿越、命令注入、权限提升、秘密泄露、恶意 Adapter、依赖漏洞均有结论。 |
| DEV-08-04 | 可访问性与窄屏真实设备验收 | 06-17、07-12 | `gpt-5.6-terra` | axe 无严重问题；关键流程键盘/读屏通过；窄屏异常处置闭环。 |
| DEV-08-05 | 备份、升级、回滚与崩溃恢复演练 | 02-15、07-05、07-08 | `gpt-5.6-sol` | 从上一 schema 升级、备份恢复、进程中断后恢复均不丢关键事实。 |
| DEV-08-06 | macOS 安装、签名前检查与最小权限复核 | 01-06、08-01～05 | `gpt-5.6-sol` | 新用户环境可安装/卸载；权限提示与实际 capability 一致；不声明未完成签名/公证。 |
| DEV-08-07 | 文档、运维手册与 Adapter 接入说明校准 | 08-01～06 | `gpt-5.6-terra` | 安装、备份、恢复、诊断、数据位置、隐私和已知限制与实际行为一致。 |
| DEV-08-08 | 候选版本 Go/No-Go 评审 | 08-01～07 | `gpt-5.6-sol` | 所有 P0 gate 通过；遗留项有 owner/风险/版本，不存在安全、数据或 AC 阻断项。 |

## 12. 建议首批并行波次

只有 `DEV-00-10` 获得 Human 确认后才启动：

| 波次 | 可并行节点 | 说明 |
|---|---|---|
| Wave 1 | DEV-01-01～04、DEV-02-01、DEV-03-01、DEV-06-01 | 工程壳、基础值对象、Adapter Schema、设计 Token 互不抢占。 |
| Wave 2 | DEV-01-05～09、DEV-02-02～10、DEV-03-02～05 | 先建立可验证底座与故障注入。 |
| Wave 3 | DEV-02-11～15、DEV-03-06～11、DEV-04-01～03 | 状态/一致性、真实 Adapter Spike、只读 Workspace 可并行。 |
| Wave 4 | DEV-04-04～08、DEV-05-01～11、DEV-06-02～03 | Workspace 隔离、应用用例、前端基础并行，契约统一。 |
| Wave 5 | DEV-06-04～17 | 页面按领域能力逐条接通，不按静态画板批量造假。 |
| Wave 6 | DEV-07-01～12 | 以 AC 纵向闭环收敛系统。 |
| Wave 7 | DEV-08-01～08 | 系统级验证与候选版本判断。 |

## 13. 节点状态

统一状态为：`PLANNED → READY → DOING → REVIEW → DONE`，另有 `BLOCKED` 和 `CANCELLED`。节点只有在依赖均为 `DONE`、输入资料齐全且模型可用时进入 `READY`。任何安全、数据或协议缺口必须进入 `BLOCKED`，不得降为“已知问题”后继续穿透阶段门禁。
