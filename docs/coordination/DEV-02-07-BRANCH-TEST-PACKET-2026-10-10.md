# DEV-02-07 Application 分支补测与准入复核计划

> 执行完成：[验收及11重签](../reviews/DEV-02-07-BRANCH-TEST-REVIEW-2026-10-10.md)。四项补测与最终变异通过，Application行93.75%/branch100%，Domain行91.67%/branch93.36%；当前stable Mac21整体PASS，Rust696/0/1、TS36/0。DEV-02-11单独恢复DONE；正式33 DONE / 8 BLOCKED / 60 PLANNED，07整节点及Gate C仍阻塞。

- 授权：Human于2026-10-10明确“可以制定计划开始了”，承接四项补测、固定nightly重测、stable Mac21及逐节点复核。
- 节点：DEV-02-07；DEV-02-11仅进入单节点证据审核，不启动下游实现。依据冻结 Domain/State/Data/Permission/Error、07/11专属标准、G0～G9、已接受ADR-0009和[C7缺口](../reviews/C7-BRANCH-COVERAGE-REVIEW-2026-10-10.md)。
- 范围：仅补测试与证据；生产源码、迁移、manifest/lock、状态/权限/错误码不变。ADR-0007、PENDING语义、dispatcher、host装配、UI及发布不在本批。
- 执行：主控串行；当前Subagent列表不含允许的Luna/Terra，不创建高阶替代子模型；第二遍反例审查如实记录，不冒称独立模型复核。

## 写集

1. `crates/persistence-sqlite/tests/atomic_message_expiry.rs`：两个真库边界测试。
2. `crates/persistence-sqlite/tests/audit_event_consumption.rs` 与新增 `tests/support/audit_port_faults.rs`：受控端口异常返回与真实事务回滚证据。
3. 本计划、新验收/小型证据、现行进度入口；DEVELOPMENT_NODES正式状态只有完成逐节点标准审核后才能修改。
4. 反向变异只作用于本轮新建仓外工程副本，源码不得在工作区临时改坏；大型target/profile/log留仓外独立目录。

## 执行顺序与验收

1. 保存生产源码、migration、manifest/lock与测试的输入SHA和工具快照；保留既有未提交改动。本批RED基线为C7真实Application14/18分支，不能伪造业务代码故障。
2. 真实SQLite：不存在Team请求返回ObjectNotFound，全表及迁移ledger不变；`i64::MAX`版本请求返回IntegrityFailed，幂等claim和所有业务/Event/Outbox写入回滚。每项有合法对照，注入ID源不得在拒绝时被消耗；不得撤销生产trigger才能构造场景。若数据库版本守卫禁止直接跨越，则用合规创建轴建立边界记录，或登记需受控端口的原因。
3. 消费端口：包裹实际SqliteAuditTransaction，只替换load_event/state的返回DTO，模拟跨Team Event及不匹配收据；核对稳定IntegrityFailed、真实rollback、没有commit/marker/projection写入、全表与首次收据不变。坏DTO不直接写库。正常对照仍通过生产SQLite端口。
4. 新测试先运行；再在仓外副本逐一绕过四个防线，必须实际编译并因对应行为断言失败，编译/装置故障不能计作检出。恢复基线SHA并重跑GREEN。使用rust-testing技能的方法，但不新增mockall、cargo-llvm-cov等依赖或工具；冻结ADR和门槛优先。
5. Stable专项命令：`cargo test -p persistence-sqlite --test atomic_message_expiry --test audit_event_consumption --locked --offline`；仅格式化本批测试文件。覆盖率沿用既有nightly-2026-10-09隔离RUSTUP_HOME和配套LLVM，独立新target/profile，重新运行探针与当前Domain/两套真库测试；逐文件JSON与LLVM文本report交叉核对，production SHA前后一致。
6. Application行≥90%、branch≥85%，Domain同样达标后，运行当前stable完整 `node scripts/ci/local-gates.mjs --platform macos --evidence <新仓外路径>`。沿用既有已授权工具PATH与公共registry；不安装新工具、不产生费用。21项及两个companion必须整体PASS，输入前后一致。
7. 对DEV-02-07复核原子性、重放、去重和Event不可变范围，显式保留未完成责任；对DEV-02-11完整核对直接依赖、三台状态机表驱动/性质/拒绝零副作用、协议线名与专属标准。满足全部适用门禁才单独重签，不因覆盖率批量晋级。Gate C还要求其他领域/恢复节点，不因指标PASS自动通过。
8. 同步最终报告、命令/退出码/源SHA、原始profiles与工具来源、变异补丁和实际分工、正式账本及后续计划。

## 停止与回退

发现需要改变生产语义、迁移、依赖或工具pin时，保持相应BLOCKED并报告事实，不自行扩展本包。工具/映射/零分母/版本/输入身份检查失败则证据不准入，不使用region代替branch。补测回退仅移除本批新增测试变更，保留原源码和既有证据；不提交、推送、部署、清理用户数据或操作生产数据库。
