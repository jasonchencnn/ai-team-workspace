# DEV-02-07 原子到期事务整改验收

限定事务子包 **PASS**；DEV-02-07/08 完整节点继续 **BLOCKED**。正式账本保持 **32 DONE / 9 BLOCKED / 60 PLANNED**，101 节点、404 条展开依赖。本轮已执行[任务包](../coordination/DEV-02-07-ATOMIC-EXPIRY-PACKET-2026-10-09.md)，没有启动 DEV-05、外派、提交、推送或部署。

## 行为与边界

- 执行：Codex 总控（GPT-6 系列），串行整合 manifest/lock。允许的 Luna/Terra 当前不可调用，未用高阶 Subagent 替代；R3 按通用门禁允许的第二遍反例审查执行，不冒称独立模型复核。
- 关联：DEV-02-07 原子提交、冻结 Message TTL、AC-12 审计。直接依赖 DEV-02-01 为 DONE；使用已有 Message 规则进行 07/08 阻塞整改，不宣称 02-12/14 准入。
- Application 编排 BEGIN/commit/rollback，Domain 无 SQLite 依赖，SQLite 实现专用 trait。内部入口固定 System，仅可执行到期动作；无 actor/allow、原始 SQL、任意 Event payload、任意目的地或 IPC 输入。
- 在 BEGIN IMMEDIATE 内重读 Team/Message、版本、TTL 和 pending attempt。仅 QUEUED/ROUTED 且 `now >= expires_at` 可进入 EXPIRED。ARCHIVED 只读；不额外把 PAUSED 等同 ARCHIVED。
- 成功时同事务保存 Message version+1、IN_FLIGHT→ABANDONED、MESSAGE_EXPIRED、audit Outbox 和首次结果；旧终态 attempt、正文、收件人、Binding 与改派历史保留。Outbox 默认 retry ceiling 为 3。
- 相同 key/payload 返回首次结果；不同 request_id/时钟/新生成 ID 不构成新 payload，expected_version 构成 payload。稳定失败保留首次 code/current_version；基础设施失败回滚所有写入和 key。未知 Team 不创建无效外键幂等行，仅返回隐藏对象存在性的拒绝。
- 新 Event 使用 SHA-256、大写 hex、紧凑 JSON 字典序 key；受保护材料包含全部 Event 字段及 nullable actor snapshot。`hash_format=message-expiry-v1` 标识此 writer 格式，tail 以同 Team 插入顺序取得，时间回拨不分叉。没有重写或宣称验证历史 Event 的 hash 格式，也没有提供 Event update/delete 接口。
- `audit` 是本包内部 Outbox destination；后台调度、consumer、dispatcher、审计导出与 UI 尚未装配。本包不处理 ADR-0007 的通知策略。

## 变更与兼容

12 个工程文件的完整 SHA 见仓外 `source-verification.json`：application manifest/lib/expiry；persistence manifest/lib/expiry、0031、13 项测试、两个 fixture、0030 存量测试；Cargo.lock。

0031 增加可空 `first_error_details` 和两个守卫（共 3 条顶层 SQL），支持首次版本冲突详情精确回放。旧记录的新列为 NULL，旧 key/hash/code/result/timestamp 不变；FAILED 后详情不能再改写。0001–0030 和根 Cargo.toml 与上一轮受检字节匹配。0030 测试只适配新增可空列和实际注册迁移数量，原字段与历史 ledger 的逐行断言保留。

根 lock 串行整合：application→domain，persistence→application/serde_json/sha2 共四条直接依赖边；已对比上一受检 lock 的真实副本，包集合和版本没有变化。sha2 沿用已锁 0.10.9，用于幂等和审计摘要；serde_json 沿用既有版本，用于结构化白名单详情与规范 JSON。未新增工具、账户或费用。

已升级库不能用仅知 0001–0030 的旧加载器回退，实际拒绝为 VersionMissing(31)。本轮只操作可丢弃测试库；真实库升级与一致性备份仍待对应节点，不提供降 schema 或历史二进制安全回退承诺。

## 验证结果

| 验证 | 实测结果 |
|---|---|
| 新 SQLite Application 集成测试 | 定义 13 项，当前最终执行 13 passed / 0 failed |
| 失败边界 | key claim、Message、attempt、Event、Outbox、首次结果六处触发器失败；另有真实 COMMIT deferred-FK 失败。对所有表和迁移 ledger 做完整前后比较；原 key 重试成功 |
| 并发与持久结果 | WAL 两连接同 key 仅一结果；同 expected_version 一成功一稳定冲突；关闭池后重新打开仍返回首次结果及待消费 Outbox |
| 拒绝与历史 | 未到期、NULL TTL、跨 Team、归档、非法状态、PENDING 安全拒绝；旧终态不改；生成 ID 冲突、ID source 失败和无效 key 零业务副作用；错误不透传 SQL/fixture 内容 |
| 审计 | 正文不进入 Event；两段链在控制面时间回拨时仍关联；每个受保护字段变动使摘要变化 |
| 第二遍反例 | 回滚误提交、绕过 PENDING、遗漏 previous_hash 关联三项真实编译并运行，各 1 个失败、exit101；无编译错误冒充行为 RED；还原副本 13/0，两份生产源码 SHA 匹配 |
| canonical Mac21 | 整体 PASS；21/21 exit0，两个 companion 的前/后原始 log body 均与记录 digest 匹配 |
| Rust / TS / gate / replay | Rust **674 passed / 0 failed / 1 ignored**（70 个 result blocks，其中 persistence **254/0**）；TS **36/0**；gate **26/0**、replay **8/0** |
| 静态和构建 | 当前最终输入的 fmt、clippy `-D warnings`、lint、类型、生成漂移及构建均通过 |

首次 TDD RED 是缺失 Application API 的编译失败，不能当作既有行为漏洞证据。中间编译/fixture 故障、旧 0030 测试的迁移数量失败及 clippy 失败日志均保留，最终结论来自修复后的实际完整门禁。当前仍保留既有依赖与信息性告警，不宣称全工具零告警；ignored 不计通过。

复现：`cargo test -p persistence-sqlite --test atomic_message_expiry`。完整门禁按已获准的 PATH 和公共 registry 执行：

```sh
PATH="/Users/chenzhiqing/.minimax/toolchain/bin:/opt/homebrew/opt/node@24/bin:$PATH" \
npm_config_registry=https://registry.npmjs.org \
node scripts/ci/local-gates.mjs --platform macos --evidence <仓外新文件绝对路径>
```

本轮证据目录：`/Users/chenzhiqing/.codex/visualizations/2026/10/09/01a11f77-308c-70a1-aa48-5061d8a6e6f2/atomic-expiry`。主要产物为 mac21.json、mac21.raw.log、mutation-results.json、run_mutations.py、companion-verification.json、source-verification.json、final-verification.json 和 inventory.json。

门禁输入为 4,709 entries；identity 为 `f61fd1c1d2e51a9f2b46f14073f9ab9b481ee7749fd963177eadf45e1e891232`，执行前后相同。报告与入口同步是门禁后输出；不能把同步后的全仓身份仍称作该输入。12 个工程文件在报告输出后另行复核。报告不会替代 macOS14 最低支持设备、hosted branch protection 或产品 E2E 证据。

## 保留阻塞与下一步

1. 冻结 attempt 图没有 PENDING→ABANDONED，但旧 SQL 允许它。当前命令遇 PENDING 失败关闭，没有伪造 dispatch/start。完整 expiry 和 Message 节点仍阻塞；必须先有正式语义裁决，不能隐式修改 FSM 或旧迁移。
2. DEV-02-07 尚缺真实 consumer 与 event_consumptions 的同事务投影/去重、至少一次重放和恢复证据。下一包先签发这个限定写集与失败测试，满足 07 专属标准后才审核依赖准入；本包的 Outbox 入队不等于派发成功。
3. 02-13 的可信 Human/Role 身份、权限最终执行点与拒绝审计，以及 02-14 的 dispatcher claim/PROCESSING 恢复仍未完成。内部固定 System 能力不替代它们，也没有宣称后台调用已经接线。
4. 关闭重开池属于持久化恢复测试，不是杀进程崩溃演练。崩溃、消费者重复投递、调度恢复和产品 UI 仍须补证。
5. application 已有可执行代码，不能再用空分母计 100%。本包未重新测量 domain/application 行覆盖率，留作完整节点覆盖率包；此前 domain 91.76% 只属于既有 domain 输入，不能扩展为 application 或两者合计。branch≥85% 仍未证实，Gate C/D/H/I 与 ADR-0007 仍未准入。

G0/G1/G2/G3/G4/G7/G9 在以上子包范围通过；G5 只证明固定内部 System/Team/TTL 边界与输出白名单，未完成全权限用例；G6 无新增 UI，G8 未做 1 万记录性能预算测量。完整节点结论为 **BLOCKED**，不放行发布或不满足依赖的后续节点。
