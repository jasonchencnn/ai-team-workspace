# DEV-02-07 剩余责任核对与单节点终审计划

> 执行完成：[责任核对与终审](../reviews/DEV-02-07-SCOPE-CLOSURE-REVIEW-2026-10-10.md)。58专项/七项反例/当前stable Mac21通过，219工程输入不变；仅恢复07基础节点DONE，正式34 DONE / 7 BLOCKED / 60 PLANNED。后续格式、dispatcher、host、产品链和Gate C没有自动准入。

- 授权：Human于2026-10-10要求“制定计划并开始”，承接07剩余历史格式/后台装配责任的核对与执行。
- 节点：DEV-02-07，直接依赖DEV-02-01已DONE；不启动02-14、05或产品后台任务。
- 依据：四份开发基线、specs索引与Domain/Data/Permission/Error、已接受ADR、[上一批验收](../reviews/DEV-02-07-BRANCH-TEST-REVIEW-2026-10-10.md)。冻结技术栈、状态/权限/协议、覆盖率门槛、节点定义和依赖不修改。
- 分工：主控串行审查、证据整合和最终结论；允许的Luna/Terra当前不可调用，不创建高阶替代Subagent。R3采用第二遍真实反例审查并明确登记，无独立模型审核声明。

## 范围与责任核对

冻结07专属标准逐字为“业务变更与 Event/Outbox 原子提交；消费可重放和去重；Event 无更新接口”。本轮不得把它缩成表存在，也不得凭上一轮报告中的“完整节点仍阻塞”循环自证阻塞。

| 待核对责任 | 冻结来源与当前归属 | 本轮处理 |
|---|---|---|
| 当前格式来源验证、未知历史安全拒绝、迁移保留 | 07、Domain跨聚合一致性与Data追加事实/留存 | 必须有真实SQLite证据；不能猜测旧hash配方或把未知格式标为已消费 |
| 新历史格式解码和全链导出兼容 | 05-10、07-11，必要时ADR规定具体格式 | 后续应用/审计验收；没有准确格式输入时不新增默认兼容或改写历史 |
| dispatcher claim/ack/重试和PROCESSING恢复 | 02-14，依赖07/11/12/13 | 不在07实现，不越过当前12/13阻断 |
| 当前固定消费者的原子效果、去重和重建 | 07 | 审验production API与真实库，不用手写SQL代替应用命令 |
| 自动选择工作、后台任务spawn/取消/关闭、host装配 | 02-14负责Outbox执行机制；05用例/03-08恢复与07纵向节点负责host生命周期 | 不把缺少自动派发当成已有消费不幂等；不在基础节点偷偷启动后台服务 |
| Human/Role身份、执行点权限和所有业务Event | 02-13与05具体用例、07-07/11端到端 | 当前System入口不接受任意actor/allow；不将其宣称为通用命令或产品完成 |

以上是依据已有节点定义核对验收归属，不是新技术/产品决策。只有找不到冻结来源或实际不满足07专属条款时才保留07 BLOCKED；其他责任继续记录到本来所属节点，不由07 DONE自动完成。

## 执行与写集

1. 保存开工工程SHA、正式状态/依赖、工具与上轮证据digest；仓库既有改动、sources及历史报告不覆盖。
2. 审查Application事务边界、SQLite公开端口/模块、0006/0014/15/16/24/32与测试；核对未知历史未消费、升级保留与拒绝安全性，以及Event不可变的UPDATE/DELETE/REPLACE真实行为。
3. 运行现有两套生产真库测试、五套Event/Outbox迁移/约束测试及默认值专项。第二遍在仓外副本进行producer/consumer/immutability相关真实变异，逐项必须编译并行为失败，恢复基线再GREEN。
4. 本轮默认不新增实现：若07条款已满足，纠正上一轮过宽的阻塞归属并单独终审；若发现可按冻结语义修复的具体缺陷，记录精确写集、先补行为测试再最小修复。若需猜测历史格式、新增协议/状态/权限或改变栈，保持阻塞，不扩大授权。
5. 完整当前stable canonical Mac21；代码输入SHA与上轮覆盖率映射完全一致时，复核原始profile/export/探针/工具digest并保留对应输入证明，不冒称重新测量；源码或有效测试范围变化时重新测量。未知/零分母仍null。
6. 逐条G0～G9及07专属标准PASS才恢复07 DONE。Gate C、其余BLOCKED和后续节点保持分别审核。同步新报告、正式节点表、进度/工作日志/冻结入口，不覆盖旧结论。

预期仓内写集：本计划、新终审与小型证据脚本、现行进度入口及07状态单行；生产src、migration、manifest/lock和技能文件均保持。大型日志、隔离副本/target放本聊天仓外新目录；不安装工具、不产生费用、不操作真实数据库。发生实际修复需求时仅限07既有模块/测试并追加具体边界，根文件主控串行处理。

## 验收命令与回退

`cargo test -p persistence-sqlite --test atomic_message_expiry --test audit_event_consumption --test event_outbox_migration --test event_outbox_consistency_forward_migration --test event_actor_outbox_integrity_forward_migration --test event_dedup_state_immutability_forward_migration --test outbox_default_forward_migration --locked --offline`。

现有已授权工具PATH及公共registry下执行 `node scripts/ci/local-gates.mjs --platform macos --evidence <新仓外证据>`；21项及两个companion整体PASS、输入前后相同才接受。scope/节点解析与链接需实际校验。

不提交、推送、部署、安装、修改生产数据或访问凭据。审查失败保留真实缺口与证据，不能通过下调门槛关闭；新文档可回退，节点状态在后续工程变更后按影响重审，历史证据只保留其对应输入效力。
