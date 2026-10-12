# DEV-02-07 本地审计消费执行包

- 本轮为07既有BLOCKED整改，直接依赖02-01已DONE；不启动DEV-05、02-14 dispatcher或通知策略。
- Codex主控串行整合。现有允许的Luna/Terra不可调用；使用R3门禁允许的第二遍运行时反例审查，不冒称独立模型复核，不恢复外派。
- 已核对development四基线、specs索引及Domain/Data/Permission/Error、07专属标准和上轮原子到期报告。冻结规格保持原样。

## 实施和验收

1. 先写真实SQLite消费测试，保留缺失API的编译RED；来源由上轮生产expire_message创建，不用手写SQL冒充业务提交。
2. 增加固定System内部audit消费者：请求只携Team及Outbox引用，不接收actor/allow、consumer名称、目标、Event内容或SQL。Application编排事务，SQLite实现端口。
3. 消费限定message-expiry-v1的MESSAGE_EXPIRED。校验Outbox/Event/Team关联、来源hash及前驱链接、已保护字段和白名单JSON；未知合法格式保持未消费，不能写成已派发。
4. 0032只新增可重建audit_expiry_projection及限定审计消费者去重记录的DELETE保护。先记消费再写投影，必须同事务提交；重复返回第一次收据。去重记录存在而投影缺失时，按原consumed_at从Event重建，不重做业务变更或重置去重。
5. 两连接/WAL同Event竞争；反序消费；不同consumer隔离；每处写入/COMMIT失败完整回滚；零行静默丢写不得成功；升级保留旧表、ledger和已有消费记录。
6. 测试子进程在marker后、projection后、commit后实际停止并由父进程终止。就绪证据必须查询该连接实际状态，不能只写阶段标志。重启重放分别恢复未提交写入或返回已提交收据；Event/Outbox/Message原始事实不变。
7. 第二遍行为变异、相关fmt/clippy/tests及完整canonical Mac21；完整门禁前工程输入冻结，报告和入口在门禁后同步并另核工程SHA。

写集：application lib/audit消费模块；persistence lib/audit消费/hash格式模块及expiry writer共用格式整合、0032；新消费测试及专用测试支持文件；上轮0031迁移测试固定升级边界以保留历史命题；本包、新审核和现行文档入口。根manifest/lock预计不变，若必要仅主控串行整合。0001–0031、历史报告和sources只读。

验收：cargo test -p persistence-sqlite --test audit_event_consumption；cargo test -p persistence-sqlite；cargo fmt --all -- --check；cargo clippy --workspace --all-targets --all-features --locked -- -D warnings；现有已获准Mac21与gate/replay自检，证据写仓外。

审计投影只保存已验证ID、时间、版本、hash等固定字段，不复制Message正文、任意payload/metadata或凭据。暂不开放用户查询/导出/IPC，也不宣称所有Event类型、通知、后台派发或完整产品链完成。0032后旧31加载器不能回退；只操作隔离测试数据库。

正式节点和阶段准入另行逐项审核，不能因子包通过而自动晋级。PENDING到期语义、ADR-0007与有效branch覆盖率仍未获准；本包不改变它们。
