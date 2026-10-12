# DEV-02-08 改派历史存储整改验收（2026-10-09）

**本轮前向迁移与限定存储整改 PASS；DEV-02-08 完整节点保持 BLOCKED。** 正式状态仍为 **32 DONE / 9 BLOCKED / 60 PLANNED**。本轮承接[C6/C7准入审查](C6-C7-ADMISSION-REVIEW-2026-10-09.md)，执行[0030实施包](../coordination/DEV-02-08-ROUTE-MIGRATION-PACKET-2026-10-09.md)。

执行者：Codex主控，R3第二遍反例审查；本轮未调用独立子模型，不冒称新独立模型审核。使用[database-migrations技能](/Users/chenzhiqing/.agents/skills/ecc/database-migrations/SKILL.md)核对前向迁移、存量、原子性及回退边界；具体SQLite事务与历史规则以Frozen规格和已审AF-02契约为准。

## 1. 已实现的改动

- 新增并注册[0030](../../crates/persistence-sqlite/migrations/0030_message_route_overrides.sql)，一个SQLx migration，`no_tx=false`。新增不可变route override及attempt的两个nullable关联字段；旧attempt的全部原字段保留，新字段为NULL，表示原Message寻址。
- 重建三条attempt守卫，使原寻址与显式override两条路径都能表达；每个attempt保存自己真实的目标Role、Binding及采纳的override，后续改派不重解释旧记录。
- override关联Event须属于同Team、同Message和同request；补齐原概念未检查的审计关联。关联序号采用整数约束，历史DELETE/REPLACE/UPDATE继续拒绝。
- 新增[8项真SQLite测试](../../crates/persistence-sqlite/tests/message_route_overrides_migration.rs)及两个合成fixture；旧[留存测试](../../crates/persistence-sqlite/tests/retained_entity_integrity.rs)只修正位置式INSERT和v28专项执行边界，保留原断言。

工程写集共6个文件：新SQL、迁移注册、专项测试、两个fixture、留存测试。根manifest/lockfile、旧0001～0029、Frozen规格、历史AF/C5/C6证据未改。只执行隔离测试数据库，没有打开或迁移用户/生产数据库。

## 2. 原始失败与整改证据

1. RED实际编译并运行1个测试：29版没有override表，0/1、exit101；不是编译失败或空filter。
2. 第一轮5项得到4/1：错目标Role被相关Team守卫拒绝，测试却只接受另一条守卫文案。按具体向量修正判据；后续先释放活跃attempt槽位再测坏关联，避免忙槽拒绝伪装成关联检查。
3. 概念基线实际接受引用另一个Message的既有Event；生产0030拒绝。同事务的Event与幂等行在失败回滚后均不残留，合法Event对照成功。另有request不匹配的独立正负对照。
4. 首轮persistence全套捕获留存装置16个失败：新增列使裸VALUES的13值INSERT不兼容；v27→v28行快照测试又调用了全量最新迁移，误把30合法增加NULL列视作28改写历史。改为明确原字段列名、把28专项固定到28；30专项仍逐字段比较存量全部原列并验证两新列NULL，没有删列快照或降低拒绝标准。

历史AF装置文档中“QUEUED没有到NEEDS_HUMAN边”的说法与冻结表及0017不符；其绕行仍是合法路径，原件保留。新fixture直接使用合法QUEUED→NEEDS_HUMAN，attempt通过PENDING→IN_FLIGHT→FAILED释放槽位。未在本轮重写旧状态机或其审查记录。

## 3. 限定验收结果

| 检查                | 实际结果                                                                                                                                                        |
| ------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| 新专项套件          | 8/0；存量完整字段、迁移重跑、三次改派、坏关联、不可变历史、Event/request、回退边界、六对象留存对照                                                              |
| DROP/CREATE故障注入 | 三个DROP后分别失败；全schema、全部业务表行与migration账本一致；还原后生产migrate成功                                                                            |
| 改派历史            | 三次改派后目标序列A,A,B,C,A；原Message寻址及全部旧attempt原字段保留                                                                                             |
| 留存联合验收        | 六个既有对象的DELETE/REPLACE/身份UPDATE在29/30均拒绝，旧诊断与行快照一致                                                                                        |
| 第二遍实际变异      | 移除Event关联、禁用半截关联、DELETE改为RAISE(IGNORE)、移除request关联：4个均编译成功、运行非空测试并以指定行为断言RED；完整副本恢复8/0且SQL SHA与production一致 |
| persistence全套     | 第一轮暴露装置缺口；修正后240/0，再追加request用例，最终Mac21包含当前全部241个persistence测试                                                                   |
| Clippy/fmt          | 定向Clippy零告警；最终workspace Clippy/fmt通过                                                                                                                  |
| 最终canonical Mac21 | **整体PASS，21/21及前后两个companion核证一致**；Rust **661/0/1 ignored**、TS36/0、Gate26/replay8                                                                |

变异只在独立完整workspace副本，不修改生产迁移；“第二遍”是主控审查方式，不是独立模型身份。RAISE(IGNORE)反例是静默丢写/错误成功报告，未将其误述为历史数据已被删除。

本轮SQL实际有 **19条顶层语句、10条CREATE TRIGGER、3条DROP TRIGGER**（其中3条重建、7条新增）。旧AF合同写“六个新trigger”是计数口径错误，不能沿用；新DDL未拆成多个提交。

原始证据保存于[本轮目录](/Users/chenzhiqing/.codex/visualizations/2026/10/09/01a11f77-308c-70a1-aa48-5061d8a6e6f2/c6-c7-execution)：`route-red.log`、`route-first-green.log`、`route-persistence-all.log`及后续修正日志分别保留，变异7项/8项测试输入分目录，不覆盖前轮尝试。

最终[Gate JSON](/Users/chenzhiqing/.codex/visualizations/2026/10/09/01a11f77-308c-70a1-aa48-5061d8a6e6f2/c6-c7-execution/route-gate.json)、[rawlog](/Users/chenzhiqing/.codex/visualizations/2026/10/09/01a11f77-308c-70a1-aa48-5061d8a6e6f2/c6-c7-execution/route-gate.raw.log)、[现场核证与六文件SHA](/Users/chenzhiqing/.codex/visualizations/2026/10/09/01a11f77-308c-70a1-aa48-5061d8a6e6f2/c6-c7-execution/route-final-verification.json)。受检4,701项输入前后相同，identity为`28e63d74c8e70e1586a97d7598a76408741998a55173c6b17a01982b94833577`。旧29迁移与5个根文件共34项SHA保持；npm/RustSec/许可证的原有信息性告警保留，不冒称零告警。

报告与入口属于验收后输出，不继续声称全仓输入等于受检4,701项。六个受检工程文件、根锁、工具与旧迁移保持最终受检字节；后续工程变化重新验证。

## 4. 回退、仍未验的责任与下一步

29-only SQLx加载器在真实30版测试库返回`VersionMissing(30)`；此为冻结版本集合下的加载器实测，没有运行一个历史桌面发布二进制。不能据此承诺旧二进制回退。已落地的迁移保持前向不可变；接入真实用户数据前仍须一致性备份/恢复或经审核的前向修复，不删除migration记录或单独DROP守卫。

当前仍未闭环：可信actor与八层权限、expected_version/TTL/幂等首次结果、生产改派事务及Event/Outbox dispatcher、跨Team查询可见性、多连接迁移锁/并发、产品性能和真实恢复。测试中的SQL业务事务只证明存储约束与回滚，不是可调用的Messaging服务。

接下来优先补DEV-02-07真实业务/Event/Outbox事务，再把0030接到获准的权限/幂等与消息用例。ADR-0007及有效branch工具仍待明确决定；08/09/11～14及03-06/07保持BLOCKED，不启动其未满足依赖的新产品节点或发布。
