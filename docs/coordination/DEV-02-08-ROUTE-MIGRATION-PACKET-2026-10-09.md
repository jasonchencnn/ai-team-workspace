# DEV-02-08 改派历史前向迁移实施包

Human已要求制定计划并继续；C6重签后的直接依赖DEV-02-01、DEV-00-04均DONE。本包落实既已审核的AF-02载体及Frozen改派/attempt历史规则，关联DEV-02-08；本轮主控签发生产编号 **0030**，采用SQLx单migration事务、`no_tx=false`。不以本包的schema交付代替完整应用权限、dispatcher或Node DONE。

## 允许写集与执行

Codex主控单写者，按迁移/兼容R3范围实施并做第二遍反例审查；当前Luna/Terra模型不在可用工具列表，不升级子模型。

- `crates/persistence-sqlite/migrations/0030_message_route_overrides.sql`
- `crates/persistence-sqlite/src/lib.rs`：注册0030。
- `crates/persistence-sqlite/tests/message_route_overrides_migration.rs`
- `crates/persistence-sqlite/tests/fixtures/route_history.sql`：保存已验证的合成历史种子，不引用产品凭据。
- 必要时仅修改现有测试中“当前最新迁移版本”的辅助断言，不修改其历史baseline或业务期望。
- 首轮persistence全套实际发现`retained_entity_integrity.rs`位置式INSERT不兼容新增列，且v27→v28专项调用了全量最新迁移；主控追加该单文件写集，仅给attempt种子/向量明确列名，并将v28专项执行范围固定到其声明版本。保留全部原状态/拒绝/列快照断言；0030新增完整存量及18项留存对照负责新版本联合验收。
- 新报告/证据及当前入口同步；旧0001～0029、AF材料、根manifest/lockfile和sources保留。

## 验收与边界

1. 先运行新存量历史测试，登记29版不具备override表/字段的RED；随后落实生产0030，旧attempt目标/Binding及全部既有字段升级后相同。
2. 两条合法路径：原Message寻址和显式override；attempt永久保存自己采纳的override、真实Role与Binding。反例覆盖半截关联、错Role/Binding、跨Team、历史UPDATE/DELETE/REPLACE、四元组抢占和序号跳跃；验证正向对照。
3. 三次改派后的旧attempt保持原值，Message正文与原寻址保留。真实SQLx迁移在每个DROP/CREATE间注入失败，完整schema、逐表全行、migration账本均恢复；不能用Python事务实验替代。
4. 核对关联Event/Message/Team的一致性和既有留存守卫；应用层可信actor、八层权限、expected_version/TTL/首次结果及投递责任另列，不把手写SQL当产品命令闭环。
5. 旧29版加载器对30版应明确拒绝；保留向前修复/一致性备份恢复的边界，不删除migration记录或单独DROP守卫“回退”。本包只运行隔离测试库，不操作用户或生产数据库。
6. 定向真库测试、persistence全套、fmt、clippy、当前最终输入完整Mac21及旧迁移/证据SHA保护。若发现触及新权限/状态/协议语义，先列冲突，保持BLOCKED。

参考：[已审实施事务契约](minimax-audit-next/AF-02-route-implementation-contract.md)、[冻结状态机](../specs/STATE_MACHINES.md)、[领域模型](../specs/DOMAIN_MODEL.md)、[权限矩阵](../specs/PERMISSION_MATRIX.md)、[数据规则](../specs/DATA_CONVENTIONS.md)。
