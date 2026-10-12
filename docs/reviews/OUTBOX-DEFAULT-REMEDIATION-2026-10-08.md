# Outbox 默认重试上限整改验收（2026-10-08）

本批 **局部整改 PASS；DEV-02-07 当前 BLOCKED，整个持续整改目标尚未达成**。这是原项目 review §5 已登记的明确缺陷：ERROR_CATALOG §11 默认最多重试 3 次，数据库省略字段时却写入 5。没有新增产品语义，不需要 ADR；历史终审和已发布 migration 保留原样。

主控负责 DEV-02-07 的 R3 实现和第二遍反例审查，本批没有委派子任务。DEV-02-14 的运行时 dispatcher 仍须单独验收，不能以修正默认值代替。

## 修改及数据影响

- 新增并注册前向 migration `0024_outbox_retry_default.sql`。SQLite 不支持直接修改列默认值，因此只重建 Outbox；显式复制全部 13 列，仅把新记录的 `max_retries DEFAULT 5` 改为 `DEFAULT 3`。
- 保留历史记录的自定义上限、状态、payload、错误诊断、幂等键和时间；重建原有 3 个命名索引及 4 个守卫。当前 schema 没有指向 Outbox 的外键，真实升级后 `foreign_key_check` 为 0。
- 专项测试覆盖空库省略字段 INSERT、版本 23 升级、五种 Outbox 状态、不同历史重试设置、升级重复执行、违规写入零副作用，以及有损历史导致升级失败时的完整事务回滚。种子检查实际行数，REPLACE 使用 PROCESSING/retry_count=1 的记录，确保测试尝试真实重置已发生的尝试。
- Notification 测试 helper 明确区分固定历史版本与当前版本，避免把版本 23 当作“最新版本”。修改仅适配测试加载，不改变 Notification 行为。
- 替换只证明 `3 != 5` 的旧测试，改为冻结默认值检查和真实数据库省略列 INSERT；同步 domain 注释，删除“不能在 schema 修复”的错误说法。

0001～0023 共 23 份旧 migration、全部 Rust manifest 与 Cargo.lock 的 SHA-256 均与上批快照相同。没有改变 wire 错误码、状态集合、依赖或技术栈。

## 验证结果

| 检查 | 结果 |
|---|---|
| 修复前真实 INSERT 回归 | 0 passed / 1 failed，数据库实际 5，期望 3 |
| Outbox 专项健康基线 | 变异前后各 4 passed / 0 failed |
| 第二遍反例 | 8 个变异全部由数据或违规写入行为断言捕获；编译、装置故障不计分 |
| Rust 全工作区 | 503 passed / 0 failed / 1 ignored，61 份测试结果汇总，exit 0 |
| workspace build / clippy 全目标全特性 -D warnings / fmt check | 全部 exit 0 |
| workspace 严格 rustdoc -D warnings | exit 0 |
| git diff --check / 工作树 Gitleaks | exit 0；秘密扫描 0 命中 |

8 个变异分别恢复默认 5、覆盖历史重试配置、丢失 last_error、撤掉跨 Team UPDATE 守卫、撤掉 REPLACE 守卫、撤掉去重状态守卫、撤掉 JSON CHECK、撤掉主键 UUID CHECK。数据保留测试在比较 DDL 前就因实际数据损失失败；守卫测试直接捕获禁止写入成功，不依赖“名字仍存在”的文本检查。

本轮测试启动后修正了一段默认值 rustdoc；前后 SHA 与精确 diff 证明仅 `///` 注释变化，可执行 Rust/SQL 未变。随后 build、clippy、fmt 和严格 rustdoc 对更新后的文件通过。源码及逐项命令记录保留，**不声称完整 Mac 21 项对最终字节输入通过**。

## 节点状态、风险及后续

DEV-02-07 的历史有条件 DONE 当前不再用于依赖准入：省略列缺陷已修复，但当前输入完整 Mac 门禁未成立，业务变更/Event/Outbox 的应用执行链仍未验收。此状态更新不撤销历史 24 条攻击或历史门禁实际发生过的事实。

DEV-02-08/09/13/14、DEV-03-06/07 继续 BLOCKED。application/security 仍是占位，真实权限执行、事务竞争、投递/恢复、取消重试和当前世代最终落库未闭环。ADR-0007 仍 Proposed；此前外部服务授权问题仍待答复。没有实现未获确认的通知语义或启动依赖未通过的新节点。

本次未改 TS 业务源码，没有重复 TS 全套检查；覆盖率、完整在线依赖审计、干净安装和业务 E2E 未验收。升级失败必须停止开放写入并保留诊断；迁移前使用一致性备份，不建议对已升级的业务库做手工降级。代码回退限于本批补丁，不能 reset 继承工作树或修改已发布迁移。

## 可复核证据

[机器摘要](evidence/outbox-default-2026-10-08/summary.json)、[全工作区测试](evidence/outbox-default-2026-10-08/workspace-tests.log)、[检查命令与退出码](evidence/outbox-default-2026-10-08/checks.json)、[修复前失败](evidence/outbox-default-2026-10-08/red.log)、[变异结果](evidence/outbox-default-2026-10-08/mutations-run-01/summary.json)、[运行器](evidence/outbox-default-2026-10-08/run_mutations.py)、[旧迁移与 manifest 证明](evidence/outbox-default-2026-10-08/protected-input-proof.json)、[输入范围](evidence/outbox-default-2026-10-08/input-scope.json)、[纯文档差异](evidence/outbox-default-2026-10-08/doc-only-change-proof.json)、[结构计数](evidence/outbox-default-2026-10-08/schema-inventory.json)。

运行器复制工程到本批独立的嵌套 evidence workspace，`--locked --offline` 执行，finally 恢复副本迁移；正式代码不被变异。已有 capsule 不覆盖；再次完整重放应复制运行器到新的证据目录，使用同一正式源码输入。旧证据保持只读。

没有提交、推送、部署、访问外部服务或操作生产数据。
