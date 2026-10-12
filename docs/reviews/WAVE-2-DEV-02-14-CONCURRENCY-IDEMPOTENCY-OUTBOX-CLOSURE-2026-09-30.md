# WAVE-2 DEV-02-14 终审：乐观锁、幂等命令记录与 Outbox dispatcher

> 冻结来源：`docs/specs/DATA_CONVENTIONS.md` §3/§4/§6、
> `docs/specs/ERROR_CATALOG.md` §4/§11、`docs/specs/STATE_MACHINES.md` 开篇、
> `docs/development/TECH_STACK_AND_STANDARDS.md` §4.2/§4.3/§5
> 节点审核标准：重复命令返回同一结果；payload 冲突拒绝；版本竞争仅一个成功；事件至少一次且可去重。
> 风险定级：**R3**（迁移/恢复/审计不可变性），`QUALITY_GATES §3` 要求独立复核 + 至少一种失败注入。
> 本文件在最后一次门禁之后不再改动。

## 1. 交付物

| 文件 | 行数 | 内容 |
|---|---:|---|
| `crates/domain/src/idempotency.rs` | 658 | §3 乐观锁 + §4 幂等四行纯判定 + 键四元组 |
| `crates/domain/src/outbox_dispatch.rs` | 467 | §11 重试 + §4.3 至少一次投递 + 累计预算 |
| `crates/persistence-sqlite/migrations/0022_command_idempotency.sql` | 245 | 幂等记录表 + 6 个守卫 |
| `crates/domain/tests/idempotency.rs` | 523 | 15 个用例 |
| `crates/domain/tests/outbox_dispatch.rs` | 757 | 23 个用例 |
| `crates/persistence-sqlite/tests/command_idempotency_migration.rs` | 570 | 12 个用例 |

新增 50 个用例（DEV-02-13 收口时全工作区 354，本轮收口时 404），
`cargo clippy --workspace --all-targets --all-features --locked -- -D warnings` 无告警。

**既有基线复用**（实测，非推测）：`Version` / `ExpectedVersion` / `Clock` /
`RandomSource` / `UuidV7Generator` 已在 `lib.rs`；`0006` 已提供 outbox 表
（5 态 + `retry_count`/`max_retries`/`scheduled_at`）、`outbox_event_destination_unique`
去重索引与 `event_consumptions`（`PRIMARY KEY (consumer_id, event_id)`）。
**真正缺的只有幂等记录表与 dispatcher 决策逻辑。**

## 2. §4 的五行分别落到哪里（最容易做错的一处）

| §4 行 | 落点 | 为什么这样放 |
|---|---|---|
| 1 同 payload 仍处理中 | `state=IN_PROGRESS` → `CONCURRENCY_RESOURCE_BUSY` | 记录态 |
| 2 同 payload 首次成功 | `state=SUCCEEDED` + `result_ref` | 记录态 |
| 3 同 payload 稳定失败 | `state=FAILED` + `first_error_code` | 记录态 |
| 4 **不同 payload** | **UNIQUE 冲突**，非状态 | 这不是状态迁移，是「别人复用了我的 key」 |
| 5 **key 超保留期** | **不进状态机** | §4 原文说防线是「唯一约束/业务引用」 |

**第 4 行做成 UNIQUE 是唯一正确的形式**：要写第二行必须先删第一行，
而 `no_delete_guard` + `replace_guard` 双双拒绝，于是重复执行在数据库层不可能。

**第 5 行做成状态是错的**：若建模成 `EXPIRED`，等于暗示「过期后可以放行」，
与该行「客户端不应复用」正相反。§6 留存表也只说「至少 30 天」，
未冻结清理时机与批次 → 不提供清理能力，理由写进迁移注释。
测试 `the_column_shapes_match_the_specification` 以「`EXPIRED` 被拒绝」把它证成行为。

## 3. 结构性保证

**`PermissionRequest` 式定长数组的做法没有搬到这里，但等价的性质拿到了**：
`IdempotencyOutcome` 是调用方**唯一**可分支的东西，且只有 `Proceed` 一个变体
授权执行——它仅在「无任何记录」时可达。终态记录缺 `first_outcome` 时
落到 `InProgress` 而**不是** `Proceed`（测试
`a_finished_record_without_its_outcome_never_authorises_a_second_run`），
因为放行意味着同一逻辑操作跑第二遍。

**§3 的冲突结果不含 `next` 字段**——调用方拿不到可写入的替代版本，
「不自动覆盖」由类型保证而非约定。

**§11 的「总时长」是累加的**：`RetryBudget` 逐次扣减，
`decide_outcome` 接收 `budget_spent_millis`。这是本轮修掉的一个真缺陷（见 §5 D4）。

## 4. 机检装置

| 装置 | 终态 | 覆盖 | 反向验证 |
|---|---|---|---|
| Q-10 `q10-version-idempotency-rules.py` | PASS=5 | §3 五条 + §4 五行 + 键四元组 + 五个必幂等命令 | 2/2（payload 码被抹掉、键段改名） |
| Q-11 `q11-concurrency-codes.py` | PASS=6 | §4 四个 `CONCURRENCY_*` 码 + retryable 值 | 2/2（retryable 是→否、删掉整行） |

**Q-10 第一版自诊断出一个探针 bug**：幂等键定义在 §4 的正文行，
我却去 §3 的 bullets 里找，报成「规格缺失」。**先怀疑探针**（J66）。

**Q-11 的「删掉一个码仍 PASS」实验证明绝对锚点有效**——
装置若在「什么都没匹配到」时报 PASS，它就是个永远绿的空壳。

两台装置都**显式声明不验 Rust**：Q-10/Q-11 确立规格侧事实，
Rust 侧由测试的字面量断言承担。**Q-10 的 PASS 不等于实现正确。**

## 5. 独立审查判定 FAIL：7 条全部成立并已修

| 编号 | 严重度 | 缺陷 | 处置 | 反向验证 |
|---|---|---|---|---|
| D1 | **阻断** | `INSERT OR REPLACE` 完全绕过 `no_delete_guard`：SQLite 的隐式 DELETE 不触发 DELETE 触发器，一次 REPLACE 即劫持 `result_ref`、改写 `payload_hash`，§4 第 2/4 行同时失效 | 补 `command_idempotency_replace_guard`（BEFORE INSERT + WHEN EXISTS）+ 两条 UPDATE 不可变守卫 | 移除后 `insert_or_replace_cannot_hijack_a_claimed_key` 与 `the_payload_hash_and_first_outcome_cannot_be_rewritten_in_place` 双双失败 |
| D2 | 重要 | `DeliveryFailure::Retryable` 接受任意字符串，§11 第一条「**只有** retryable=是 时允许」形同虚设 | 显式声明该判定属调用方（本模块无错误码目录，复制即漂移），并补测 `Permanent` 分支在所有 retry_count 上不可重试 | — |
| D3 | 重要 | 注释断言 `CONCURRENCY_RESOURCE_BUSY` 是 §4 **唯一** retryable=是 的码，实测有三个 | 改写为列出三个码 + 按语义（补救为「有限退避；`retry_after_ms`」）选 BUSY | — |
| D4 | 重要 | `ceiling_millis` 只夹**单次**延迟，§11 的「**总时长**不得超过 TTL」未实现；字段注释还承诺了 TTL 保证 | 新增 `RetryBudget` 逐次扣减；`decide_outcome` 接收 `budget_spent_millis` | 注入「忽略预算」→ 报 `accumulated 6200 exceeds the 5000 budget at retry 4` |
| D5 | 次要 | `ceiling_millis` 为负时 `clamp(0, -1)` panic，破坏总函数承诺 | `backoff_millis` 只做 `.max(0)`；预算用 `.saturating_sub().max(0)` | 测试 `a_negative_budget_does_not_panic` |
| D6 | 次要 | `entry.max_retries` 是死字段：行级设 0（明确禁重试）仍重试 3 次 | 行级值优先（§11「默认」暗示可覆盖，`0006` 存的是行级列）；`OutboxEntry::pending` 的占位 0 改为 §11 的 3 | 注入「忽略行级上限」→ `the_rows_own_ceiling_overrides_the_policy_default` 失败 |
| D7 | 次要 | 4 处悬空引用，其中 `every_spec_row_has_a_case` **听起来像一道规格漂移防线，实际不存在** | 改写为指向真实存在的测试名，并注明那只是评审时的人工检查、不是自动防线 | — |

**D1 最刺眼的地方**：本项目 `0013`/`0014`/`0016` 已有四处 `*_replace_guard` 先例，
`0014` 的注释甚至写明「OR REPLACE 会先删除冲突行再插入，等于绕过上面全部 UPDATE 守卫」。
**我读到了那些注释却没把模式套到新表上**——`0022` 成了唯一漏掉的表（J77）。

## 6. 修复过程中我自己抓到的三个错误

1. **守卫写对了，测试却归错了类**：第一版 `result_ref` 不可变守卫把**合法的
   `IN_PROGRESS → SUCCEEDED` 推进也拦了**。「写一次」不等于「不能从空变为有值」。
2. **预算测试测的是我自己的算术**：注入「`decide_outcome` 忽略预算」时 23/23 全过，
   因为测试自己算 `spent`、根本没经过被测函数。改成从 `decide_outcome` 的
   `scheduled_at` 反推 delay 后，同一注入报出上面那个 6200 的数字。
3. **两处手抄错误**：UUID 多打一位（33 位，SQL 报 `unrecognized token`）、
   `pragma_table_info` 返回大写类型名。两处都是测试错，实现对。

## 7. 刻意的不建模（代码注释均写明「等什么条件出现才该做」）

| 项 | 原因 |
|---|---|
| `actor_fingerprint` 的哈希算法 | §4 只说键里有这一段，**没说怎么算**。`spread128` 明确声明无安全声明（我第一版写的 `fnv1a128` 常数溢出，是自找的） |
| `payload_hash` 的算法 | §4 只说「保存 payload hash」。只比较，生成由应用层注入 |
| `command_name` 的枚举 | §4 只点名 5 个必须幂等的命令，未给完整词表 → 字符串承载 + 逐字断言那 5 个 |
| Outbox 的新状态值 | `0006` 已冻结 5 个 |
| 退避的**随机源** | §11 要求 jitter 但未说谁抽 → jitter 由调用方注入，本模块不读时钟、不持随机源 |
| 真正的 IO 派发 | 节点标准是「至少一次且可去重」，属**投递语义**；投递通道由 DEV-03 Adapter 契约冻结 |
| 幂等记录清理 | §6 只说「至少 30 天」，未冻结时机与批次；P0 用 UNIQUE 兜底 |
| `EXPIRED` 幂等状态 | §4 第五行说防线是唯一约束，不是过期判断 |

## 8. 遗留登记（未擅自改冻结规格）

### 8.1 `0006` 的 `max_retries DEFAULT 5` 与 §11 的 3 冲突

| 来源 | 数值 | 能否修改 |
|---|---|---|
| `ERROR_CATALOG §11` | 自动重试默认最多 **3** 次 | 冻结规格 |
| `0006_event_outbox.sql` | `max_retries ... DEFAULT 5` | **已发布迁移，改动漂移 SQLx checksum** |

**至今没人发现的原因**：`0015`/`0016` 的既有测试**全部显式传 5**，
列默认值从未被任何断言观察到。**对调用方的后果**：省略该列的 `INSERT`
静默拿到 5 次重试，违反 §11。

**本轮处置**：`DEFAULT_MAX_RETRIES = 3` 作为唯一居所；docstring 写明冲突与
「调用方必须显式设值」；测试用 `assert_ne!(DEFAULT_MAX_RETRIES, 5)` 把冲突钉死
——若有人日后「修好」成 5，测试失败并指向 §11。

**不在本轮修复**：改 `0006` 漂移 checksum；用 `0023` trigger 覆盖默认值则需发明
「何时该用 3 何时该用 5」的规则（§11 说「默认」暗示可覆盖，但没说条件）。
**登记给有权改 schema 的节点。**

### 8.2 其余

1. **§3 的 `H` 与 §5 的 8 类覆盖面不一致**（DEV-02-13 发现，待 Human 裁决）
2. **§3 36 行与迁移 0009 的 9 动作 × 11 资源粒度不同**，规格无映射键
3. **`ERROR_CATALOG` 目录比 §7 宽**：另有 `WORKSPACE_SYMLINK_ESCAPE` /
   `PATH_NOT_FOUND` / `PATH_NOT_WRITABLE`
4. **退避 base（200ms）与总预算（60s）是本实现的选择**，§11 只给次数、形状与 TTL 约束
5. **`already_consumed` 是恒等函数**，无调用点——真实去重依赖 `event_consumptions` 表，
   端到端超出本轮三份产物范围

## 9. 门禁

见仓库外台账。识别规则：取编号最大且文件名前缀**非** `VOIDED-` / `SUPERSEDED-` 的证据文件。
