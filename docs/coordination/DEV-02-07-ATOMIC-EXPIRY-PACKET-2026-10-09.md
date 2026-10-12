# DEV-02-07 原子到期执行包

- 主控：Codex（当前会话）；不外派 Minimaxcode。允许的 Luna/Terra 当前不可调用，采用主控串行实现与第二遍反例审查，不声称独立模型复核。
- 依赖：DEV-02-01 DONE；本包为 02-07/08 既有阻塞整改，不启动 DEV-05 或宣称 02-12/14 完成。
- 输入：development 四基线、specs 索引及 DOMAIN_MODEL、DATA_CONVENTIONS、STATE_MACHINES、PERMISSION_MATRIX、ERROR_CATALOG；已完整核对。
- 关联：冻结 Message TTL 与 AC-12 审计；02-07 业务/Event/Outbox 原子性标准。

## 执行顺序

1. 先写真实 SQLite 集成测试并保留 RED；新增 Application 专用内部到期事务端口。
2. Application 使用控制面 Clock、UUID source；SQLite 只实现端口，不自行编排业务事务。固定 System attribution，不接收 actor、allow、任意 SQL、Event payload 或目的地。
3. 同一 BEGIN IMMEDIATE 中重新读取 Team/Message、版本、TTL 和投递状态；合法迁移才保存版本+1、终止 IN_FLIGHT attempt、MESSAGE_EXPIRED、audit Outbox 和首次结果。默认重试沿用 3。
4. 稳定失败保存首次错误及必要 current_version；新增 0031 可空错误详情列，旧记录保持原值。基础设施失败回滚，允许同 key 重试。幂等摘要不含 request_id、时钟和新生成 ID。
5. 真实触发器逐写入边界注入失败；两连接/WAL 竞争；重复请求首结果、payload 冲突、到期等号、未到期、归档、跨 Team、非法状态、终态留存、hash 字段与链关联。
6. 反例审查、相关 Rust 检查、完整 Mac21；结果写入新审查记录和现行入口，不覆盖历史证据。

## 精确范围与限制

写集：application manifest/lib/expiry 模块；persistence manifest/lib/expiry 模块、0031、新到期测试及必要新增 fixture；0030存量测试兼容0031可空列与迁移计数（保留旧字段断言）；Cargo.lock 仅主控串行更新；本包及新审核记录、现行进度入口。

冻结 attempt 图只有 PENDING→IN_FLIGHT→终态，而旧 SQL 另允许 PENDING→ABANDONED。不得伪造 dispatch/start 来终止 PENDING。本轮存在 PENDING attempt 时失败关闭，保留原始历史；该语义差距登记阻塞，完整到期服务不能声明完成。不新增通知策略、Attention 解决规则或 IPC 协议，不改旧 migration/纯状态机。

audit 只是内部 Outbox destination；本轮不声称已有 dispatcher、消费者、导出或 UI。System 入口是 Rust host 内部能力，尚无后台调度装配，不向不可信输入暴露。

验收命令：cargo test -p persistence-sqlite --test atomic_message_expiry；cargo test -p persistence-sqlite；cargo fmt --all -- --check；cargo clippy --workspace --all-targets --all-features -- -D warnings；已获准的 canonical Mac21（证据写仓外）。发布 migration 不可编辑；0031 后旧加载器回退不安全，只允许从隔离测试备份恢复。

正式账本保持 32 DONE / 9 BLOCKED / 60 PLANNED，只有完整节点标准和门禁满足后才另行签核。
