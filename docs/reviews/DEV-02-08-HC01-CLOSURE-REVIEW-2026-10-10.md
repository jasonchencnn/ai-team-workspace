# DEV-02-08 HC-01 基础存储节点终审（2026-10-10）

**DEV-02-08恢复DONE。** 0033已正式签发和整合，改派后的Role换绑可新增采用当前Binding的attempt，原Message寻址、override决策快照及旧attempt完整字段保留。当前stable Mac21整体PASS，Rust **708/0/1 ignored**、TS **36/0**。正式 **35 DONE / 6 BLOCKED / 60 PLANNED**；Gate C仍BLOCKED，未启动下游产品用例。

执行/审核：Minimaxcode回传自报 `MiniMax-M3.1-Flash-Preview` 提出仓外候选，Codex主控独立核对、补强测试、第二遍反例、串行整合及最终准入。本轮没有内部Subagent，不把SELF_CHECK视作DONE。授权与写集见[整合计划](../coordination/DEV-02-08-HC01-INTEGRATION-PLAN-2026-10-10.md)。

## 1. 输入及回传裁定

- 交接281项SHA全部匹配；回传12条命令的log SHA/实际结果块、21项artifact SHA匹配。补丁仅在明确的仓外目录应用，五文件与回传候选SHA完全一致；原候选10/0独立复跑。
- 回传记录的“误写生产工作树后恢复”事故保留。复核开始和整合前的281项均匹配，证明恢复后的当前字节，**不能证明全程只读**。
- 接受最小DDL修复，补强候选证据：attempt历史读取扩展到全部15字段，override读取全部14字段；多次改派断言补上Binding/generation等遗漏快照；32→33检查所有旧表完整行和ledger，并验证重跑不变。
- 回传M1的两项失败中，一项确实接受了非法写入，另一项只改变拒绝诊断。M2的四项失败含诊断变化及RowNotFound；其旧六套7/8失败也仅是拒绝诊断变化。因此不把失败数量全部计成行为检出。本轮另做指定断言、实际写入判据的反例。
- 09-29缺失最终JSON及旧重签降级事实保留；本报告建立当前输入的准入证据，没有补造旧日志或追认当时的缺证。

## 2. 实施与范围

[正式0033](../../crates/persistence-sqlite/migrations/0033_message_route_attempt_binding.sql)仅一条DROP和一条CREATE，零业务行DML，嵌入式注册 `no_tx=false`。override继续保存改派发生时的决策Binding；新attempt保存自己采用的Binding，仍要求完整快照、同Team/Message/override序号及目标Role。0017继续验证Binding属于该Role且generation匹配，原寻址路径和全部历史守卫保留。

工程写集：新SQL、迁移注册及注册测试、新兼容套件、旧route测试两处诊断归属、0032专项加载范围；另有扫描策略的精确哈希例外。旧0001～0032、根manifest/lockfile、Frozen规格与sources未改。

候选将0032 ledger精确数量改成宽松比较的做法未整合。主控把0032专项固定为31→32，保留原 `values.len()+1` 及旧行断言；0033专项负责32→33完整联合升级。旧route四个拒绝向量和全行零副作用检查均保留。

08直接依赖DEV-02-01、DEV-00-04均DONE。此节点负责 `to_role_id`、快照、reply_to、TTL/幂等字段及历史的存储表达；可信actor/权限属13和应用边界，命令竞争/首次结果/dispatcher属14，生产Messaging与投递时ACTIVE选择属05-07，Binding生命周期属05-03。不能把这些依赖08的用例倒挂为08前置，也不能因08DONE宣称它们已完成。

## 3. 验证及第二遍反例

| 检查 | 当前实测 |
|---|---|
| 原六套基础专项 | 57/0，全部保留且非空 |
| 新兼容/注册 | 11/0 + 1/0；含原寻址/改派后换绑、完整历史、坏关联、合法准备的四元组抢占与正向对照 |
| 完整persistence恢复验证 | **288/0**，fmt零差异，定向clippy `-D warnings` 通过 |
| 升级/回滚 | 29→33旧列保留且追加NULL解释不变；含override的32→33全行/ledger保留；SQLx DROP/CREATE失败恢复schema/业务行/ledger，随后正常迁移成功 |
| 加载器 | 29/30/31/32-only对33库分别首先返回VersionMissing(30/31/32/33)；不是旧桌面二进制测试 |
| 当前真实覆盖率 | Domain行1937/2113 **91.67%**、分支211/226 **93.36%**；Application行150/160 **93.75%**、分支18/18 **100%**；新测19 executables/150 profiles，非旧测量冒用 |
| 完整stable Mac21 | **21/21及两个companion整体PASS**，Rust708/0/1、TS36/0；72个Rust结果块，222项工程/扫描策略输入前后相同 |

本轮七项仓外反例均实际编译、非空运行，在指定断言0/1、exit101：

1. 恢复override旧Binding全等：合法当前Binding被拒，正向回归失败。
2. 移除Binding完整性：缺快照写入实际成功，拒绝断言失败。
3. override编号比较从 `=` 弱化为 `<=`：错误编号实际写入成功。
4. Binding generation从 `=` 弱化为 `>=`：较低的陈旧generation实际写入成功；补充的旧值反例检出，不能只验超前值。
5. 移除四元组抢占保护：除已用key外其余字段均合法的新override实际成功，专属断言失败。
6. 嵌入式注册改 `no_tx=true`：事务注册契约断言失败（注册约束检查，单列计数）。
7. 失败注入对照绕过Migrator直接执行raw SQL：DROP实际残留，完整schema断言失败（装置事务边界对照，单列计数）。

不把七项全部混称生产行为变异：前五项是行为反例，第六是注册契约，第七是失败注入对照。SQLx0.8.6 SQLite的apply始终开启事务，单改no_tx=true的实验保持GREEN；读取已锁定本地源码并核SHA确认这一点，未误述该旗标能关闭SQLite事务。迁移仍按Frozen约定注册false。

首轮判据写成其他驱动的 `rows_affected`，实际SQLx SQLite打印 `changes: 1`；修正判据，保留该轮日志。后续no_tx实验GREEN亦保留为边界事实。最终第03轮七项有效，恢复后全包288/0且被变异文件SHA回到原值，未修改生产旧迁移。

## 4. Mac秘密扫描阻断及修复

首轮Mac21在工作树扫描发现两项，失败证据保留。两项均是旧输入清单中 `packages/design-tokens/test/tokens.test.ts` 的实际SHA-256，逐项重算相同，不是凭据。

保留旧清单字节，在generic-api-key下新增 **两条明确路径AND整行固定字段/固定SHA** 的例外，不跳过文件或目录。四个隔离对照全部符合预期：精确元数据通过；同路径换值、同路径加入模拟凭据、同内容放到未列路径均被拦截。无真实秘密进入对照或报告，负例报告使用redact。

第二轮当前输入Mac21通过；工作树及全refs历史秘密扫描都通过，工具来源SHA与既有Gitleaks8.30/cargo-audit0.22/cargo-deny0.18.6记录一致。原依赖信息性告警保留，不宣称零信息性告警，不关闭或放松其他门禁。

## 5. G0～G9及限制

| Gate | 结论与边界 |
|---|---|
| G0 | PASS；08两个直接依赖DONE，六文件范围明确，无新Frozen语义 |
| G1/G2 | PASS；仓外同源复验及当前Mac21，构建、fmt、lint、typecheck、clippy、生成漂移通过 |
| G3 | PASS；原57、新11与注册1、真实SQLite失败路径及第二遍反例通过 |
| G4 | PASS（存储范围）；完整历史、升级重跑、失败零副作用及SQLx事务回滚；多连接升级和真实一致性恢复仍属15/08-05 |
| G5 | PASS（存储/扫描范围）；跨Team、错Role/Binding/序号和历史改写仍拒；精确扫描例外有正负对照；不冒称应用actor认证已完成 |
| G6 | N/A；无UI变更 |
| G7 | PASS；override已有Event/request关联及历史保护，未增加审计改写接口 |
| G8 | PASS（本节点）；一条主键关联查验，无新后台循环或无界输出；产品性能留到应用/系统节点 |
| G9 | PASS；输入、回传、失败轮、来源、原始日志、反例、恢复SHA及回退边界可复核 |

旧桌面二进制兼容、真实用户数据库恢复、最低系统桌面证据和产品投递闭环仍未验证。只迁移隔离测试库；未安装新工具、改用户stable/default、提交、推送、部署或操作生产数据。新增费用0。

剩余BLOCKED：DEV-02-09/12/13/14、DEV-03-06/07。ADR-0007和PENDING到期语义继续独立等待决策，Gate C没有通过；下游按冻结依赖逐项准入。

## 6. 证据身份、失效与回退

受检Mac21共 **4781项输入**，gate identity `e7437fb5e2816320f1095373468950d54fa092bce125e0a7543ff090de4f50ac` 前后相同。两个companion的前后raw-log规范化digest均匹配保存字段；完整原文和结果见[第二轮Gate](/Users/chenzhiqing/.codex/visualizations/2026/10/10/01a1237c-0b1c-7121-9922-cc68642a02ff/hc01-codex-closure/gate-02/mac21.json)及[原始日志](/Users/chenzhiqing/.codex/visualizations/2026/10/10/01a1237c-0b1c-7121-9922-cc68642a02ff/hc01-codex-closure/gate-02/mac21.log)。

[仓外完整目录](/Users/chenzhiqing/.codex/visualizations/2026/10/10/01a1237c-0b1c-7121-9922-cc68642a02ff/hc01-codex-closure)保留回传校验、应用/整合、三轮反例及恢复、SQLx源码边界、首轮失败Gate、脱敏诊断、扫描正负对照、当前覆盖率和第二轮Gate。仓内[摘要](evidence/hc01-codex-closure-2026-10-10/result.json)含关键SHA；[反例驱动](evidence/hc01-codex-closure-2026-10-10/run_counterexamples.py)、[Mac21核证驱动](evidence/hc01-codex-closure-2026-10-10/run_mac21.py)可重放。

本报告、节点/进度登记和摘要为验收后输出，不声称它们写入后全仓仍等于受检4781项身份；受检222项工程/扫描策略另核前后及输出后SHA一致。后续工程、策略、工具或公告库变化须重新验证。0033正式迁移此后保持不可变；恢复仅经审查的后续前向修复或一致性备份恢复，不删ledger、不单DROP守卫、不承诺旧二进制直接降级。
