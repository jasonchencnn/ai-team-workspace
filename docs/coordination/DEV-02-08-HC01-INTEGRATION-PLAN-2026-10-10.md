# DEV-02-08 HC-01 Codex复核、整合与终审计划

> 已完成：0033正式整合，当前Mac21及节点终审通过；08恢复DONE，正式35/6/60，Gate C仍BLOCKED。见[终审](../reviews/DEV-02-08-HC01-CLOSURE-REVIEW-2026-10-10.md)。以下计划与34/7/60为签发时点。

Human明确要求“核对输入SHA与原始日志、补足第二遍反例、签发正式迁移编号、串行整合，随后完整Mac21门禁与节点终审。开始执行”。本包执行该授权，关联DEV-02-08；其直接依赖DEV-02-01与DEV-00-04均DONE，当前正式34 DONE / 7 BLOCKED / 60 PLANNED。没有恢复整批Minimax外派，也不启动新产品节点。

## 已完成的输入核对

HC-01 `run-20261010-01` 回传实际Minimax-M3.1-Flash-Preview；281项交接输入SHA全部匹配，12条命令的原始log SHA与测试结果块匹配，返回清单21项artifact SHA匹配。补丁在明确的仓外绝对目录应用，五文件与候选SHA完全一致，独立复跑原候选10/0。回传临时误写生产工作树的事故保留，当前字节已恢复到交接输入；不得把恢复后的SHA描述成全程只读证明。

## 计划及写集

Codex主控串行负责R3审查、反例和最终门禁；本轮不创建仓库路由禁止的高阶Subagent。生产写集仅：

- `crates/persistence-sqlite/migrations/0033_message_route_attempt_binding.sql`：本包正式签发 **0033**，最后已注册版本为32，无编号冲突；SQLx `no_tx=false` 单事务替换一条守卫。保留override決策快照，每个attempt记自己的实际Binding；不新增状态、权限、协议或Frozen语义。
- `crates/persistence-sqlite/src/lib.rs`：仅注册0033及嵌入式注册的事务标志测试。
- `crates/persistence-sqlite/tests/message_route_binding_compatibility.rs`：整合候选并补强完整历史、弱化generation与已合法准备的四元组抢占对照。
- `crates/persistence-sqlite/tests/message_route_overrides_migration.rs`：两处守卫诊断归属更新，拒绝及全行零副作用断言保留。
- `crates/persistence-sqlite/tests/audit_event_consumption.rs`：0032专项加载器范围固定到32，保留原ledger精确数量及所有原行断言；0033专项单独负责32→33联合升级。
- `.gitleaks.toml`：首次Mac21命中旧输入清单的真实设计token测试文件SHA。保留旧清单字节，增加generic-api-key规则下精确“两路径AND整行/固定SHA”例外，配不同值/模拟凭据/未列路径负向对照，不跳过整个文件或目录。
- 新报告、独立证据与当前进度入口；旧0001～0032、Frozen规格、根manifest/lock、sources及历史报告/回传证据均只读。

执行顺序：

1. 保留原回传；在仓外候选副本补强测试。真实SQLite成功/拒绝对照，拒绝前后完整行一致；验证29→33、含override的32→33、重跑、SQLx DROP/CREATE失败回滚及加载器兼容边界。
2. 第二遍独立反例：恢复旧Binding全等、缺失Binding完整性、编号比较弱化、generation比较弱化、四元组抢占守卫弱化、绕过SQLx migration事务；另验证嵌入式注册的事务标志。必须实际编译、非空执行，并在指定断言RED；诊断变化不计行为检出，所有变异仅仓外。SQLx0.8.6 SQLite的apply始终开启事务，因此no_tx=true实验保持GREEN，不把该旗标误述为实际关闭SQLite事务。恢复后重跑套件且SHA一致。
3. 串行整合五文件，核32个旧migration和根锁SHA；完整persistence定向验证后在当前生产工程输入运行canonical stable Mac21，保存21条命令、两个companion前后raw-log digest及输入前后身份。
4. 按08冻结专属标准和G0～G9终审；全部通过才单独恢复08 DONE。Message应用权限/dispatcher、备份/产品闭环仍按12/13/14/15/05/07/08节点归属，Gate C不因08终审自动通过。

预算新增0；沿用已授权Mac门禁服务与公共registry，不安装或新增工具、账号；不提交、推送、部署或操作真实用户/生产数据库。回退仅经审查前向修复/一致性备份恢复，旧二进制降级不作承诺。
