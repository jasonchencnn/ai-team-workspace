# C6 节点准入复核与 C7 覆盖率调查（2026-10-09）

本轮完成 C6 的 21 个阻塞节点差距核对、盘点装置修正、C7 现有工具实测和当前输入完整 Mac21。总控按依赖顺序重签 **12 个底座节点**，正式状态由 **20 DONE / 21 BLOCKED / 60 PLANNED** 更新为 **32 DONE / 9 BLOCKED / 60 PLANNED**。产品阶段 Gate C/D/H/I 尚未通过，ADR-0007 仍 Proposed，V2 和 Minimaxcode 外派继续暂停。

## 1. 执行范围与证据来源

Human 要求“制定计划然后继续”，本轮承接 [进度计划](../coordination/PROGRESS_AND_NEXT_PLAN-2026-10-09.md) 的 C6/C7。执行者为 Codex 主控；本会话可用 Subagent 列表不包含要求的 Luna/Terra，因此没有创建高阶替代子模型，没有新独立模型审查声明。

已有 [NA-01 盘点](evidence/codex-node-admission-2026-10-09/inventory/node-inventory.json)、[NA-02 数据审核](evidence/codex-node-admission-2026-10-09/database-review/matrix.json)、[C5 收口审计](CODEX-AUDIT-CLOSURE-2026-10-09.md) 和其中保存的独立反例证据均保留原件。总控现场核源码/规格/锁的字节关联，复核专项标准及职责边界，再以本轮真实门禁作准入判断；未把旧 SELF_CHECK 或模型自陈直接转成 DONE。

旧 NA 保护清单 209 项中，208 项字节未变，唯一变化是本次已经授权的节点文档入口段；实际节点定义、依赖与状态在开工时未变。202 项属于本轮 canonical 输入并逐项 SHA 匹配；另有 4 个生成 Schema 和 3 个 `.DS_Store` 不在 canonical 清单，仅作额外保护核对，均与旧快照一致。不能把 209 项保护计数与 4,692 项 Gate 输入计数混为一谈。

生产 Rust/TS、manifest/lockfile、现有 migration 0001～0029、冻结规格和旧证据未改。新增代码仅为本轮 evidence 中的只读节点校验及离线覆盖率能力探针；不接入产品、不改变 canonical21、不新增第三方依赖。

## 2. 真实发现与修正

旧 NA-01 解析器在 dict 赋值前没有检查物理定义重复；`len(set(nodes)) == len(nodes)` 无法发现已经被覆盖的行。追加同一个 DEV-00-01 定义后，实际 102 行仍被归并为 101 项，旧验证返回无错误。旧自检只是添加第 102 个不同 key，未测这一形状。[原反例与 SHA](evidence/progress-execution-2026-10-09/legacy-duplicate-counterexample.json) 保存该事实，未覆盖旧装置或旧结果。

新增 [只读校验器](evidence/progress-execution-2026-10-09/verify_node_inventory.py) 在建立索引前拒绝重复物理定义；严格消费依赖 token，并验证缺项、未知依赖、自依赖、环、无法解析的依赖和重复依赖。7 类负向输入均以对应原因拒绝；正常输入为 101 节点、404 依赖，与旧清单的 ID、依赖、状态和专属标准相同。旧记录行号因入口插入而过时，新清单重新记录真实定义行号。[核对结果](evidence/progress-execution-2026-10-09/inventory-verification.json)。

NA-02 报告把“四个候选”写成“三个”，且 DEV-02-10 的未决说明越出了该节点“表结构可表达最小权限”的专属范围。总控纠正如下：

- DEV-02-10 的 schema/审批历史及约束验收，与 DEV-02-13 的最终执行点权限检查和 DEV-05-09 的审批服务分开。真实 actor、scope 消费和拒绝 Event 仍是后两者的必需工作，不能因为它们未实现而永久拒绝一个已满足标准的表结构节点。
- ADR-0007 补齐的是通知与 Attention 语义，直接阻断 DEV-02-09；没有新增 Permission/Approval 表结构语义，因此不能直接把它写成 DEV-02-10 的审批表阻断。
- DEV-02-07 的专属标准明确包含业务变更与 Event/Outbox 的原子提交，不能同样缩减成“表存在”。它继续 BLOCKED。
- DEV-02-08 的授权改派及旧 attempts 不可重解释要求尚只有概念升级证据；正式迁移/兼容/真实执行验证未落地，继续 BLOCKED，不采用旧二进制回退或自动签入0030。

## 3. 本轮完整门禁与原始日志核证

第一轮在默认 PATH 找不到 gitleaks，未进入 canonical 检查；保留 `gate.raw.log`。使用已经存在的 `/Users/chenzhiqing/.minimax/toolchain/bin` 后完成第二轮，未下载或安装扫描工具。公共 npm registry / RustSec 操作沿用已经明确授权的范围，费用 $0，没有账号、发布或生产操作。

[Gate JSON](/Users/chenzhiqing/.codex/visualizations/2026/10/09/01a11f77-308c-70a1-aa48-5061d8a6e6f2/c6-c7-execution/gate-with-tools.json)、[原始日志](/Users/chenzhiqing/.codex/visualizations/2026/10/09/01a11f77-308c-70a1-aa48-5061d8a6e6f2/c6-c7-execution/gate-with-tools.raw.log)、[总控核证](/Users/chenzhiqing/.codex/visualizations/2026/10/09/01a11f77-308c-70a1-aa48-5061d8a6e6f2/c6-c7-execution/gate-verification.json)：

| 核验                                                                | 实际结果                                                                                             |
| ------------------------------------------------------------------- | ---------------------------------------------------------------------------------------------------- |
| canonical Mac21                                                     | 整体 PASS，21/21 exit0，命令/参数与 C5 冻结向量相同                                                  |
| Rust                                                                | 653 passed / 0 failed / 1 ignored；68 个结果块按 rawlog 求和                                         |
| TS / Gate / replay                                                  | 36/0；26/0；8/0                                                                                      |
| 输入身份                                                            | 4,692 项，前后相同；gate identity `be50fea7c28f56b3fc2cbe58bb1c6ccd83c5b97aaac6aa008c87e5dedcf42978` |
| ignored dotenv / 全 refs / guards                                   | 输入身份相同；两个 companion 的前后原始正文分别重算摘要，一致且匹配 Gate JSON                        |
| 工具                                                                | 7/7 工具路径的 binary SHA 与 C5 来源记录相同；固定版本核验通过                                       |
| 构建、fmt、lint、typecheck、clippy、contracts、unsafe、扫描、许可证 | 对应 canonical 命令全部 exit0；依赖与许可证信息性告警仍按原日志保留                                  |

Gate JSON SHA：`3523938dad0599bf9362d9099f1d5df5bc8dac5fe8c86e7347d07551e609d0dc`；rawlog SHA：`1ea6337cca846a79542ee07ec85bf74791cfe67eb1a5133ac1534e837c9b4204`。

独立计算使用另一份 Python 过程读取 rawlog 的两轮 companion 正文，不把 runner 仅保存的 after 数组冒称 before/after 原始记录。结果是总控第二遍核证，不冒称本轮又启动了独立模型。C5 及更早独立审查只在实际字节相同的实现与相同适用边界内复用。

下述状态表、矩阵及报告属于门禁后的审核输出。它们写出后全仓输入会变化，不能声称仍等于受检4,692项；受检工程源码、锁、工具、配置和旧迁移不变，后续工程变更须重新验证。

## 4. 21 节点专属标准与准入结论

总控先重签01-05/08，然后02-02/03/04，再02-05、02-06、02-10，最后依次03-02→03-03→03-04→03-05。每一步的直接依赖已 DONE；未同时放行仍有阻断的09/13/14或运行时节点。

| 节点      | 专属标准对应的当前证据与边界                                                                                                    | 结论 / 唯一接续责任                                                             |
| --------- | ------------------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------- |
| DEV-01-05 | 当前完整Mac21、固定工具、输入/companion、workflow/guards负向行为；本机门禁不冒称 hosted branch protection                       | DONE：总控重签，工程门禁框架范围成立                                            |
| DEV-01-08 | Correlation span传播与生命周期；JSON输出、敏感字段、注册值及派生拼写；日志/error/Event统一脱敏的39测试，C5独立反例与当前SHA一致 | DONE：框架范围成立；host接线/轮转随实际后续用例验收                             |
| DEV-02-02 | 0001/0028、Team/Role/Workspace真库测试；UUID/版本、状态、唯一性、外键、留存及前向拒绝                                           | DONE：schema范围；实际Workspace路径访问归DEV-04                                 |
| DEV-02-03 | 0002/0010/0028；单ACTIVE、历史区间、Team/Role/Harness关系及留存正负对照                                                         | DONE：schema范围；换绑服务归DEV-05-03                                           |
| DEV-02-04 | 0003/0011/0028；Task owner/状态/显示ID/Reference关系、直接与多跳环、依赖更新及留存                                              | DONE：schema范围；readiness/claim命令仍未完成                                   |
| DEV-02-05 | 0004/0012/0027/0029；active Run、lease世代/tuple、attempt、Review关系和回滚；非法终态修复已局部独立复核且现输入不变             | DONE：真库约束；生产claim及回调写入归02-14/03-07                                |
| DEV-02-06 | 0005/0013/0026；Artifact/Reference的Role/Run/版本/URI/hash/可见性、跨Team、升级拒绝和历史保留                                   | DONE：元数据schema；真文件授权/发布仍归DEV-04/05                                |
| DEV-02-07 | Event追加、Outbox默认值及消费去重约束存在；production仅migrate，业务/Event/Outbox原子命令尚无实现                               | BLOCKED：Codex主控签最小真实事务执行包；不能用手写SQL测试替代                   |
| DEV-02-08 | 0017/0018/0025与现有真库测试；AF概念39矩阵保留；授权改派仍缺正式迁移与存量/兼容闭环                                             | BLOCKED：Codex明确生产改派写集、不可变路由历史及兼容/回退；原29加载器回退不可用 |
| DEV-02-09 | 0019/0023守卫保留；07/08未DONE，Owner/filters/escalation/强制通知与解决凭据待决策                                               | BLOCKED：Human决定ADR-0007；Codex在被接受条款内实施                             |
| DEV-02-10 | 0009/0020/0021；resource/action/path scope、有效期、审批人、策略关联/版本、决定不可变及自批拒绝可表达最小权限                   | DONE：表结构范围；最终actor/审批消费检查仍在02-13/05-09                         |
| DEV-02-11 | 三台纯状态机及有限集合穷举性质、非法迁移无副作用、wire/error对账存在；领域branch指标未证                                        | BLOCKED：Codex补有效branch方案，保持纯规则与应用责任分离                        |
| DEV-02-12 | Message/Attempt/Approval纯规则存在；08/09尚阻断，branch证据未证                                                                 | BLOCKED：先补08/09及有效覆盖率，不能把通知/投递服务算入已交付纯规则             |
| DEV-02-13 | 矩阵/八层纯求交及局部回归已有；07/08/09阻断；可信actor、当前资源检查与PERMISSION_DENIED Event未接                               | BLOCKED：Codex的实际权限执行点包，禁止信任Adapter/payload allow                 |
| DEV-02-14 | 0022与版本/幂等/退避判定已有；07/11/12/13未DONE；无production dispatcher与两连接命令竞争                                        | BLOCKED：真实事务/首结果/消费claim/去重/崩溃恢复包                              |
| DEV-03-02 | 当前真实进程测试；启动/超时/退出/关闭、同组与获准Mock脱组后代、Drop形成与消失；C5独立反例字节复用                               | DONE：ADR-0006获准入口边界；无任意恶意脱组后代保证                              |
| DEV-03-03 | SemVer/版本、能力、认证、健康分类；五入口stdout非法行反例及稳定诊断；完整协议与真进程测试                                       | DONE：握手/健康范围，不表示真实Harness已接入                                    |
| DEV-03-04 | node_mock_adapter_end_to_end_execution真实stdio正常流，sequence、output互斥、结果引用                                           | DONE：Mock正常流，不表示真实Artifact发布或Task完成                              |
| DEV-03-05 | 超时、重复/冲突、乱序、crash、晚到、拒绝取消/失联；C5独立crash反证、真实reader/退出及当前全量                                   | DONE：故障注入；当前数据库世代的最终裁决仍归03-07                               |
| DEV-03-06 | 取消/超时/重试纯判定已修；02-11/14阻断，运行时取消确认/fence后释放与唯一新Run未接                                               | BLOCKED：真实取消/重试事务包，先满足直接依赖                                    |
| DEV-03-07 | 世代/状态/续期纯判定与真库lease约束已修；02-11阻断，最终写入点未读取当前世代并原子提交                                          | BLOCKED：SQLite+Mock三类旧token回调、并发/恢复包                                |

## 5. G0–G9 矩阵与适用性

P表示本节点范围已满足；D是直接依赖非DONE；I是必需实现缺失；U是证据未验证；Q是Human未决；N是不适用并在下段注明范围。G1/G2是本轮当前输入工程检查，不是产品可用结论；G3只认可已有领域/真库/协议行为范围。N没有用于永远必填的G0/G1/G2/G3/G9。

| 节点  | G0  | G1  | G2  | G3  | G4  | G5  | G6  | G7  | G8  | G9  |
| ----- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| 01-05 | P   | P   | P   | P   | N   | P   | N   | P   | N   | P   |
| 01-08 | P   | P   | P   | P   | N   | P   | N   | P   | P   | P   |
| 02-02 | P   | P   | P   | P   | P   | P   | N   | N   | N   | P   |
| 02-03 | P   | P   | P   | P   | P   | P   | N   | N   | N   | P   |
| 02-04 | P   | P   | P   | P   | P   | P   | N   | N   | N   | P   |
| 02-05 | P   | P   | P   | P   | P   | P   | N   | N   | N   | P   |
| 02-06 | P   | P   | P   | P   | P   | P   | N   | N   | N   | P   |
| 02-07 | P   | P   | P   | U   | I   | U   | N   | I   | U   | P   |
| 02-08 | P   | P   | P   | U   | I   | U   | N   | U   | N   | P   |
| 02-09 | D/Q | P   | P   | U   | U   | Q/I | N   | I   | U   | P   |
| 02-10 | P   | P   | P   | P   | P   | P   | N   | N   | N   | P   |
| 02-11 | P   | P   | P   | U   | N   | N   | N   | N   | N   | P   |
| 02-12 | D   | P   | P   | U   | N   | N   | N   | N   | N   | P   |
| 02-13 | D   | P   | P   | U   | I   | I   | N   | I   | U   | P   |
| 02-14 | D   | P   | P   | U   | I   | U   | N   | I   | I   | P   |
| 03-02 | P   | P   | P   | P   | N   | P   | N   | P   | P   | P   |
| 03-03 | P   | P   | P   | P   | N   | P   | N   | P   | P   | P   |
| 03-04 | P   | P   | P   | P   | N   | P   | N   | P   | P   | P   |
| 03-05 | P   | P   | P   | P   | N   | P   | N   | P   | P   | P   |
| 03-06 | D   | P   | P   | U   | I   | U   | N   | I   | I   | P   |
| 03-07 | D   | P   | P   | U   | I   | I   | N   | I   | U   | P   |

基础schema节点的G5仅表示存储约束和隔离能表达/保护已冻结规则；没有获准的执行用例，不能声称已验证运行时授权。其G7不适用是因为本节点不执行业务命令，存储的历史关系已在G4中验证；02-07涉及Event与原子业务写入，因此G7必须验。schema的查询性能、业务日志、UI/A11y在对应应用/产品节点验收，未借N越过阶段出口。01-08 G8仅为当前matcher及span资源边界；host文件轮转不是此框架范围。Adapter的G7/G8限于实际有界诊断和进程，未外推至产品Event、最低支持设备或5并发Run性能。

## 6. C7 已完成调查，branch门槛仍未满足

[探针源码](evidence/progress-execution-2026-10-09/probe_coverage.py) 使用现有 Rust1.98.0 与 Apple LLVM21，实际编译并执行一个if的正反两路，合并profile并导出映射。2/2行、11/11 region被覆盖，但 **branch mapping为0**；stable拒绝`-Z coverage-options=branch`，exit1。[实际探针证据](/Users/chenzhiqing/.codex/visualizations/2026/10/09/01a11f77-308c-70a1-aa48-5061d8a6e6f2/c6-c7-execution/coverage/coverage-capability.json)。这不是达到85%的测试结果，也不是分支覆盖率0%的业务结论。

现场核对domain src16份、tests16份及application占位src，与 [FC覆盖率输入](evidence/fc-return-audit-2026-10-09/coverage/input-manifest.json) 的tree SHA一致。有效历史行测量仍为 **2016/2197=91.76%**，region89.87%不能替代branch；application无映射行，覆盖率保持null。

继续路径：先提出独立覆盖率工具的准确版本/来源/隔离/回退及真实branch映射验证方案，由Human决定冻结工具路径的调整；本轮未安装nightly、未设置RUSTC_BOOTSTRAP、未改rust-toolchain或把0分母写成100%。该工具决策不改变生产编译器或覆盖率门槛，接受后仍须真实测量。

## 7. 接续任务与限制

1. **DEV-02-07**：优先签出最小生产应用事务执行包。输入、可信actor及领域行为先按冻结规格定界；application持有事务，persistence实现端口，业务变更/Event/Outbox共同提交，任一失败零业务副作用。根manifest/lock由主控串行整合，真SQLite失败注入验收；不以generic helper或裸SQL演示冒充完成。
2. **DEV-02-08**：将已审概念的不可变route override/attempt快照转为明确生产迁移及兼容任务包；保留既有29版，不声称可回退旧二进制，真实SQLx存量/失败零副作用验证后才能重签。
3. **C7工具决策与ADR-0007**：两项分别保留待决。通知语义已通过异步问题提交Human；未回复不视为接受，不阻止其他获准收口工作。
4. 之后按02-11/12→02-13→02-14及03-06/07各自依赖接真实执行链，再进入备份/恢复、Workspace、真实Harness、IPC/UI与AC验收。

本轮没有新的产品功能完成、没有提交/推送/部署，正式阶段和发布条件仍按冻结规则执行。
