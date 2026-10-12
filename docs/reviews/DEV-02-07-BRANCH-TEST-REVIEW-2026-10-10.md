# DEV-02-07 分支补测验收与 DEV-02-11 单节点重签

本批限定测试包 **PASS**。四个Application遗漏结果已覆盖，生产源码未修改；最终Domain行 **91.67%**、branch **93.36%**，Application行 **93.75%**、branch **100%**。当前受检输入stable canonical Mac21整体PASS。主控单独恢复 **DEV-02-11 DONE**；DEV-02-07整节点仍BLOCKED，正式账本为 **33 DONE / 8 BLOCKED / 60 PLANNED**。Gate C未通过，未启动依赖节点。

## 授权与改动

Human于2026-10-10要求“可以制定计划开始了”，并再次要求“继续”。按[限定计划](../coordination/DEV-02-07-BRANCH-TEST-PACKET-2026-10-10.md)、[C7真实缺口](C7-BRANCH-COVERAGE-REVIEW-2026-10-10.md)、ADR-0009、冻结Domain/State/Data/Permission/Error规格、07/11专属标准和G0～G9执行。关联DEV-02-07原子事务/审计拒绝、AC-12基础能力，DEV-02-11三台纯状态机规则。

Codex主控串行整合和第二遍反例审查；当前没有可调用的获准Luna/Terra，未创建高阶替代Subagent，未声称独立模型审核。采用[rust-testing技能](/Users/chenzhiqing/.agents/skills/ecc/rust-testing/SKILL.md)的边界测试/失败注入方法，工具和门槛仍按冻结ADR，不安装技能示例中的额外工具或依赖。

工程仅修改 `tests/atomic_message_expiry.rs`、`tests/audit_event_consumption.rs`，新增 `tests/support/audit_port_faults.rs`。其余216项原有工程输入SHA不变，包含production src、全部0001～0032迁移、根manifest/lock和生产工具链。仅测试数据/返回DTO异常注入，无生产行为、迁移、协议、状态或权限变化；sources/只读。无提交、推送、部署、生产数据、凭据或新增费用操作。

## 四项补测与反向验证

| 防线 | 当前测试证据 | 反向变异的实际失败 |
|---|---|---|
| Team不存在 | 真SQLite查询确认不存在，生产命令返回ObjectNotFound；全部表/迁移ledger不变，ID源不消耗；存在Team对照可到期并写Event/Outbox | 将该路径错误改为IntegrityFailed，新测试稳定错误断言失败 |
| SQLite版本上限 | 保持全部migration/FK/trigger的合成边界快照：i64::MAX拒绝IntegrityFailed，幂等claim回滚、整库不变、ID源不消耗；i64::MAX-1对照成功到MAX且各有一个Event/Outbox | 绕过应用上限判定，后续SQLite仍拒绝，但ID已消耗；新测试ID断言失败，能够区分两道防线 |
| 跨Team端口Event | 真实SqliteAuditTransaction验证后仅替换返回DTO的Team；应用IntegrityFailed并rollback；调用序列begin/load_event/rollback，未查询state/写marker/投影/commit；整库不变，正常委托对照Applied | 绕过应用Team判定，调用序列/拒绝来源变化，新测试失败 |
| 不匹配消费收据 | 从真实首次Applied收据开始，仅替换state返回DTO中的hash位；应用IntegrityFailed并rollback；没有commit/marker/投影写入，整库及首次时间不变；正常委托对照Duplicate且仍为首次收据 | 绕过应用收据判定，错误收据被报告Duplicate，新测试失败 |

版本边界是测试数据库的合成既有快照，不冒称从v1进行了海量真实业务迁移；没有DROP/替换生产trigger。端口故障DTO从未写库，SQLite事实由真实transaction负责；Mutex调用记录只作拒绝位置见证，另有真实全表比较与后续合法控制证明副作用与连接恢复。

新旧两套stable专项 **35 passed / 0 failed**（到期15、消费20；消费包含一个普通空子进程fixture入口，实际三处崩溃由父用例执行）。最终四项变异均实际编译、运行、exit101、对应1 failed，无编译故障冒充检出；仓外副本还原后35/0，两个生产文件SHA与工作区一致。补测基线RED为原C7真实branch14/18，不把新增测试初次通过伪称为修复了业务漏洞。

## 最终覆盖率与stable门禁

| 生产源范围 | 行covered/count | 行比例 | branch covered/count | 分支比例 |
|---|---:|---:|---:|---:|
| Domain：15映射源 | 1937/2113 | 91.67% | 211/226 | 93.36% |
| Application：2映射源 | 150/160 | 93.75% | 18/18 | 100% |

沿用已有nightly-2026-10-09隔离RUSTUP_HOME及LLVM23.1.3，不再联网下载/安装；重新运行单路1/2、双路2/2探针。独立新target/profile、locked/offline，Domain287/0及真库34/0（filtered空入口1），19个executable和150个runtime profiles；生产源前后SHA相同。逐源分支位置/计数与LLVM JSON summary、两份llvm-cov report的TOTAL完全一致。测试/fixture/example/依赖/其他crate不入分母，application lib模块声明无映射单列；指标定义和分母未改变。

首次补测汇总失败关闭：LLVM按泛型实例汇总，异常包装端口与普通端口实例的互补结果不能直接累加冒称达标。最终加入包装端口自身的正常Applied/Duplicate对照，让同一实例覆盖正反路径，也验证包装器确实正常委托SQLite；没有修改测量脚本、门槛或生产源码。首次数据、后续最终数据和两轮变异分别留存，旧输出未覆盖。其他证据核对中曾把result字段误写为passed、漏识别伴随日志前缀，校验未通过时没有写PASS；已按实际pass字段及真实日志marker完成复核，保留失败核验输出，不混入业务失败计数。

最终stable Rust1.98.0 Mac21：**21/21 exit0、整体PASS**；两个companion前后实际log body摘要各自相同，且与保存的log_sha256匹配。全refs秘密扫描、tooling guards、fmt/lint/typecheck/clippy、生成契约、构建、测试和依赖/许可证检查通过。Rust **696/0/1 ignored**、71个result blocks；TypeScript **36/0**。ignored不计通过，依赖许可证仍有既有信息性warning，不能声称所有工具无warning。沿用已授权工具PATH与公共registry，现有Gitleaks/cargo-audit/cargo-deny二进制SHA与既有来源证据一致；用户原stable/default/installed list不变。

受检manifest **4728 entries**，总identity `e1d57af34484f526db8833fcd20c32957fabb8ae1d03b08c3264f90e84a64c90` 前后相同。本报告及进度/节点重签属于门禁后输出；工程SHA另行复核，不能宣称这些文档输出后的全仓已重跑Mac21。当前开发机仍不能代替最低支持版本设备、hosted checks或产品E2E。

## DEV-02-11 正式重签

直接依赖 **DEV-00-03、DEV-02-02/03/04/05全部DONE**。当前三机源码未变，结合历史冻结规格对账、当轮逐节点复核和当前输入证据重新审核，不直接复用旧门禁身份。

- 专属标准：Task14生命周期边+2创建、Run17+1、Binding7+1；合法边表双向核对、所有状态/命令对非法空间、终态吸收、创建范围、Reconcile各目标、三种换绑策略、稳定拒绝/事件线名均有当前行为证据。56项专属测试全绿；有限集合穷举性质覆盖Task7×10、Run8×12、Binding5×6。纯函数参数按值、无I/O/可变引用，拒绝仅返回原current/command/error，Transition/Creation字段私有，compile-fail doctest防止外部改写结果。
- 第二遍反例：在仓外分别把TODO MarkReady、QUEUED Start、PENDING_CHECK Activate的目标改错，三台现有规格对账测试各实际1 failed、exit101；无编译错误，恢复源码后56/0。变异不作用于工作区，补丁/命令/日志可重放。
- G0：纯规则范围与5个直接依赖成立；G1/G2：当前stable构建/生成/静态门禁通过；G3：表驱动、穷举、wire和实际反例通过；G4：纯规则零状态副作用，事务持久化在后续节点；G5/G7：冻结权限前置条件/稳定拒绝与事件描述准确，执行点重算/真实Event追加仍属应用责任；G6无UI变更；G8有限边表查找，无新进程或资源；G9当前证据、边界、回退和分工明确。

结论：**DEV-02-11恢复DONE，仅三机纯规则范围**。不把precondition描述当作执行点鉴权，不把Event描述当作实际提交，不放行尚被其他直接依赖阻断的03-06/07或02-14。

## DEV-02-07与阶段剩余边界

本轮覆盖率缺口关闭，固定System到期命令的原子Event/Outbox、本地审计投影去重、重建及崩溃重放继续成立。完整07仍 **BLOCKED**：已有消费者仅处理message-expiry-v1，广义历史格式及host后台装配责任尚未完整交付/正式审验，沿用[前包限定结论](DEV-02-07-AUDIT-CONSUMER-REVIEW-2026-10-10.md)。dispatcher另属02-14，不能混算成07已完成；后续须先明确07剩余写集/标准归属再签包，不重复补测已覆盖的四个分支或重做已通过的原子消费。

其余BLOCKED为02-08/09/12/13/14、03-06/07；02-12仍有08/09直接依赖，ADR-0007、PENDING到期、可信Human/Role执行点、备份/恢复及完整产品阶段未准入。Gate C的覆盖率条目通过，不等于Gate C整体通过。V2新语义与Minimax外派继续暂停。

## 重放与回退

命令：`cargo test -p persistence-sqlite --test atomic_message_expiry --test audit_event_consumption --locked --offline`；固定隔离nightly的测量使用既有[run_specialist.py](evidence/c7-branch-2026-10-10/run_specialist.py)；变异与11复核脚本见[evidence目录](evidence/c7-branch-tests-2026-10-10)。全部执行命令/退出码、来源/版本/二进制SHA、profile、完整映射和gate/input见[仓外证据](/Users/chenzhiqing/.codex/visualizations/2026/10/10/01a1237c-0b1c-7121-9922-cc68642a02ff/c7-branch-tests)，摘要与digest见[结果](evidence/c7-branch-tests-2026-10-10/result.json)。

回退只移除本批三份测试变更，并重新声明相关证据失效；生产源码和数据无需回退。仓外变异源码已恢复，既有工具/证据保留。节点表如有新工程变更须按影响重审，不以本轮DONE无限沿用。
