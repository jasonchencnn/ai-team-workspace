# DEV-02-07 责任核对与单节点终审

**DEV-02-07 恢复 DONE（Event/Outbox基础节点范围）。** 当前受检输入完整stable Mac21整体PASS，七套专项58/0、第二遍七项真实变异全部检出，工程219项字节均未改变。正式账本为 **34 DONE / 7 BLOCKED / 60 PLANNED**；Gate C仍未通过，没有启动dispatcher、应用host后台服务或其他依赖节点。

本轮按Human于2026-10-10“制定计划并开始”及[执行计划](../coordination/DEV-02-07-SCOPE-CLOSURE-PLAN-2026-10-10.md)执行。依据DEV-02-07专属标准、四开发基线、Domain/Data/Permission/Error冻结规格、ADR-0009及[上一批补测](DEV-02-07-BRANCH-TEST-REVIEW-2026-10-10.md)。关联FR-042的不可变Event基础、AC-12基础证据；完整Activity Stream和审计闭环仍有自己的后续验收。

## 修正上一轮范围判断

上一轮把“广义历史格式、后台装配尚未交付”笼统列为07整节点阻塞，未对应其专属条款。这是主控审核范围判断过宽，本轮重新按冻结节点定义和实际证据裁定；旧报告保留原时点事实与判断，不回写历史。

冻结07专属标准逐字为：**“业务变更与 Event/Outbox 原子提交；消费可重放和去重；Event 无更新接口。”** 本轮没有把它降为表存在：已交付的production到期命令及固定审计消费者确实经过Application事务、真实SQLite写入/竞争/失败/崩溃测试。其限定格式/入口仍如实记录，不能宣称整个产品完成。

| 责任 | 归属与结论 |
|---|---|
| 当前业务/Event/Outbox同事务提交、消费幂等/重建、Event追加事实 | 07，当前生产入口及真实库证据通过 |
| 未知历史格式保留、拒绝时不消费；迁移不改写历史 | 07兼容边界，现有测试通过；不意味着未知格式已经被成功解析或全链验证 |
| 新历史格式解码、全类型读模型/导出及完整hash链回放 | 05-10/07-11；具体旧格式配方必须有来源，必要时ADR；不能按想象补造 |
| Outbox claim/ack/retry/PROCESSING恢复、至少一次自动派发 | 02-14；当前BLOCKED，不在07实现，也未因为07 DONE自动完成 |
| 应用host自动选取工作、spawn/取消/关闭与恢复接线 | 02-14的执行机制及05具体用例/03-08恢复/07纵向验收；当前没有生产后台worker |
| Human/Role执行点身份权限、所有业务命令、Notification策略 | 02-13/05用例及07-07/11；System固定入口不是通用actor授权接口 |
| PENDING attempt到期冲突、ADR-0007通知规则 | 独立未决语义，不修改，不用假投递化解；完整Message到期服务仍未验收 |

依赖图实际解析证明：02-14直接依赖07/11/12/13；05-10直接依赖07/15；07-11间接依赖07。把这些后续交付倒挂为07 DONE先决条件会产生人为循环。此处只纠正验收归属，没有修改节点定义/依赖、FR/AC追踪、栈、协议、门槛或已接受ADR。

## 专属标准实证

| 冻结标准 | 当前实证与范围 |
|---|---|
| 业务变更与Event/Outbox原子提交 | `expire_message`由Application负责BEGIN/commit/rollback；同一真实SQLite事务更新Message/attempt、写MESSAGE_EXPIRED、audit Outbox及首次结果。逐写入与deferred-FK COMMIT失败全表还原；同key首结果、payload冲突、两连接版本竞争与重启保留均通过。不是手写SQL事务冒充production命令 |
| 消费可重放和去重 | `consume_audit`固定audit-projection-v1、message-expiry-v1；marker与投影同事务，两个写入各核rows_affected=1。重复返回首收据；反序、WAL双连接、一Applied一Duplicate；缺投影沿首consumed_at重建，失败零副作用。真实marker后/projection后/commit后三处进程崩溃重放仍通过。消费不把Outbox写成已发布，不回滚原业务事实 |
| Event无更新接口 | 生产Application/SQLite公开模块仅有专用事务端口和migrate，无Event update/delete repository/通用SQL或Tauri暴露。0006保护UPDATE/DELETE，0014保护recursive_triggers关闭时REPLACE；0015/16保护Role-Binding快照、Outbox与去重状态。真库测试及本轮三项Event防线反向验证通过。SQLx pool仍是可信host基础设施能力，不能据此宣称抵御用户直接改数据库文件 |

07直接依赖仅 **DEV-02-01 DONE**，未把下游未完成节点当作前置。Event字段/Team/subject/Role-Binding关系、去重键、默认重试3及历史配置保留、空库/升级/失败回滚均有七套当前专项证据。已注册消费者执行点重新验证来源/Team/Outbox引用、固定actor、白名单JSON、protected hash与同Team前驱链接；它只证明当前可识别段，不冒称所有历史全链验证。

合法未知格式/type返回DOMAIN_PRECONDITION_FAILED且全表不变；已声明格式的actor/metadata/payload/前驱/hash损坏返回STORAGE_INTEGRITY_FAILED。0001～0032迁移与production代码保持字节不变；合法v13/v14/v15/30/31历史升级保留既有行，脏历史升级失败不暗改原值。旧版本加载器不支持已升级库的直接回退，沿用已有风险登记。

## 当前验证

- 七套专项 **58 passed / 0 failed**；涵盖到期15、消费20、Event基础4、0014专项5、0015专项4、0016专项6、默认值专项4。消费20含一个普通空子进程fixture入口，实际三处崩溃由父用例调用，不额外当作产品场景。原地未写新测试、未改production或迁移。
- 第二遍七项仓外行为变异：漏写Outbox、漏写投影、rollback改commit、忽略源hash，以及停用Event UPDATE/DELETE/REPLACE防线；各实际编译后对应测试1 failed、exit101，无编译或SQL装置故障计为检出。所有改靶文件已恢复，SHA与工作区一致；恢复七套58/0。补丁、命令和日志随证据保存，不只口述杀伤数量。
- 当前stable Rust1.98.0完整 **Mac21整体PASS，21/21 exit0**；两个companion前后实际log body的规范化digest相同且与保存字段一致。输入manifest **4732 entries**，identity `90fc8ec9f0605e4595405f60fb3f57f92b410f4f51d50729c661ecee8173f9d9` 前后相同。全refs历史秘密扫描、tooling guards、fmt/lint/typecheck/clippy、Rust/TS测试、build、生成物和依赖/许可证门禁通过。
- Rust **696/0/1 ignored**、71个result blocks；TypeScript **36/0**。ignored不计通过，既有许可证信息性warning仍存在。没有新增账号、工具、安装、网络来源或费用；门禁沿用已授权工具PATH与公共registry，不操作真实Harness/凭据/生产数据。
- 覆盖率复核对应输入：当前production与有效测试的SHA均与上批实际测量相同，所有19个executable、150份raw profile与rustc/LLVM二进制digest逐项复核，原始export/profdata/text report的digest保存。本轮是**复核沿用测量，不冒称重新测量**。Domain行1937/2113=91.67%、branch211/226=93.36%；Application行150/160=93.75%、branch18/18=100%。fixture/example/测试/依赖/其他crate不入分母，未知映射不伪报100%。

## G0～G9与正式结论

| Gate | 结论 |
|---|---|
| G0 | 07与唯一直接依赖成立，三条冻结标准分别有证据；后续责任明确，无倒挂依赖或新增语义 |
| G1/G2 | 当前stable完整构建、生成、fmt/lint/typecheck/clippy和工具身份通过，根manifest/lock未改 |
| G3 | 七套58/0、崩溃/竞争/失败、第二遍七项真实反例及当前对应覆盖率通过 |
| G4 | Application持有事务边界；commit前全表回滚、commit后重放去重、迁移保留与恢复测试成立；系统备份/升级全演练仍属02-15 |
| G5 | 固定System能力、执行点来源/Team重验、字段/错误脱敏；schema快照约束通过。未宣称通用Human/Role权限执行点或直接DB访问防护 |
| G6 | 本节点无UI变更，产品可访问性和异常页面由后续节点负责 |
| G7 | 真实业务Event含request/version/correlation/hash，追加事实不被消费失败改写；投影仅保留固定验证字段。完整查询/导出仍未准入 |
| G8 | 固定单条消费、512-byte JSON加载边界、事务明确；无新后台循环、任务或无限输出。1万数据列表/启动恢复性能属于后续用例和08-02，未冒称测过 |
| G9 | 当前命令、来源、SHA、日志、失效规则、限制与回退可重放；R3采用第二遍反例审查，明确没有独立模型复核 |

主控串行执行/复核（GPT-6系列）；当前不可调用仓库允许的Luna/Terra，未用高阶Subagent替代，也未恢复Minimax外派。结论是 **07基础节点DONE**，不等于FR-042 Activity Stream、AC-12、消费所有历史类型、自动投递或Gate C完成。

其余BLOCKED：DEV-02-08/09/12/13/14、DEV-03-06/07；02-14仍被12/13等依赖阻断，05-10仍依赖15，未启动。下一步优先对08的正式改派历史/兼容责任签限定包；ADR-0007与PENDING语义继续独立等待正式决定，不能在存储整改中偷改。

## 证据、失效与回退

[仓外完整证据](/Users/chenzhiqing/.codex/visualizations/2026/10/10/01a1237c-0b1c-7121-9922-cc68642a02ff/dev02-07-scope-closure)含engineering before/after、node table before、责任依赖解析、specialist-tests、counterexamples（源码副本/补丁/日志/恢复SHA）、Mac21命令/原始日志/结果及companion digest、coverage-input-verification。摘要与digest见[结果](evidence/dev02-07-scope-2026-10-10/result.json)，反例脚本见[run_review.py](evidence/dev02-07-scope-2026-10-10/run_review.py)。

本报告、节点重签和进度入口是门禁后输出；另行核对全部219项工程SHA不变，不把文档输出后的新全仓身份冒称为已复跑。当前开发机证据不代替最低支持版本设备、hosted branch protection或产品E2E。

本轮没有数据库迁移或生产行为变更，无数据库回退步骤；审查材料可回退，源码副本已还原，历史证据不删除。后续工程变更须按影响重审本节点；新格式或新增权限/协议语义仍须按原ADR流程。未提交、推送、部署或修改生产数据。
