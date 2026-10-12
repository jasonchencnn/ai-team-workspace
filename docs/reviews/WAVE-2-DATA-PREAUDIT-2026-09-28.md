# Wave 2 数据节点只读预审（2026-09-28）

> 结论：这是 DEV-02-03～10 的待修缺口清单，**不是节点验收或开工许可**。DEV-02-02 仍为 `REVIEW`，这些节点依[冻结清单](../v2/TASK_FREEZE.md)保持原状态。macOS 为当前唯一验证平台。

未参与数据库实现的 Codex `gpt-5.6-terra` Subagent `wave2_data_audit` 只读核对 `0001～0009` migration、现有测试、冻结领域/状态/权限/数据规格，并在本机 SQLite 重新执行[首轮反例 SQL](evidence/wave2-mac-2026-09-28/sqlite-counterexamples.sql)。四项非法事实仍能写入：`cross_team_binding=1`、`cross_team_task_owner=1`、`three_hop_cycle_edges=3`、`mismatched_lease=1`。这与[首轮正式验收](WAVE-2-MAC-ACCEPTANCE.md)的 DB-01～03 一致；现有测试全绿不能推翻反例。

| 节点 | 需关闭的真实约束缺口 | 最低反例 |
| --- | --- | --- |
| DEV-02-03 | Binding 的 Role/Harness/Adapter 可跨 Team 关联；历史区间之外还需核实 INSERT/UPDATE 双路径 | 跨 Team 元组必须拒绝，合法同 Team 绑定与换绑仍成功 |
| DEV-02-04 | Task owner 可跨 Team；依赖只禁止两节点环，且缺 UPDATE 路径 | 三跳、四跳、UPDATE 改边成环及跨 Team 依赖均拒绝 |
| DEV-02-05 | Run 的 Task/Role/Binding/Harness 元组未彼此校验；Lease 可指向别的 Task/Run 或不同 fencing token | 错配 INSERT/UPDATE 拒绝；并发 claim 仅一方成功；失败后无部分写入 |
| DEV-02-06～10 | Artifact/Reference、Event/Outbox、Message/Delivery、Notification/Attention、Permission/Approval 的具名外键大多仅验证对象存在，未验证 Team 一致；Outbox 成功提交测试缺失败回滚用例 | 各节点按依赖顺序补跨 Team INSERT/UPDATE 反例；Outbox 故意失败时业务/Event/Outbox 均回滚 |

修复须新增**前向** SQLx migration，不改写 `0001～0009`。SQLite 的单列外键不能表达跨表 Team 一致性；可在新 migration 中针对具名关系增加 INSERT/UPDATE trigger，依赖图用递归 CTE 校验可达性，并在未来应用层最终执行点重新鉴权。多态 subject/source 需结合冻结类型语义在应用层校验，不能仅凭 migration 声称全部封闭。数据库也不能单独保证每个业务变更必有 Event/Outbox；该原子性须在 DEV-02-14 的应用事务中另证。

逐节点验收至少运行 `cargo test -p persistence-sqlite --locked`、`cargo clippy -p persistence-sqlite --all-targets --all-features --locked -- -D warnings`、`cargo fmt -p persistence-sqlite -- --check`，并保留真实 SQLite 的拒绝错误和零副作用证据。旧反例脚本当前退出 0 代表**漏洞重现成功**，不能被写成 Gate PASS。任何代码修复改变输入后，仍须最终 Mac 21 项、全 refs 扫描与独立审核。

本次只读预审没有修改 migration、源代码或生产数据，也没有提前晋级节点。
