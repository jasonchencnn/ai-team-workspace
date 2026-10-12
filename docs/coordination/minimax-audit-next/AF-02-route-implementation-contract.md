# AF-02 路由改派载体：正式实现事务契约与回退边界

本文件只描述**方案与装置**。不签入产品实现，不签生产 migration 编号，不改任何生产代码。
所有实证结论的原始日志见 `docs/reviews/evidence/minimax-audit-next/AF-02/`。

裁决沿用 Codex 在 `docs/reviews/FC-RETURN-AUDIT-2026-10-09.md` §6 已定的前提，本包不重新论证：
改派载体符合已有 Frozen 语义，不因前向替换旧 trigger 自动需要新 ADR；只有真改变身份/权限/协议才另提。

---

## 1. 正式执行的唯一事务边界

**正式实现必须是 SQLx 的单 migration 事务，`no_tx = false`。**

- 一个 migration 文件，一次执行，整体一个事务。19 条语句（三对 `DROP TRIGGER` / `CREATE TRIGGER`、
  一张新表、两个 `ALTER TABLE`、六个新 trigger）要么全成，要么全不成。
- 不得把 DDL 拆成多个 migration 来"缩小爆炸半径"。三对 DROP/CREATE 必须留在同一个事务里，
  否则 `DROP` 已提交而 `CREATE` 未提交时，守卫真的会消失。
- 实验中的版本号 **30 只是内存审计库的临时编号，不是生产编号**。正式编号由 Codex 在实现写集里签发。

依据（本包实测，见 §5）：在**有真实历史业务行**的当前 29 版库上，三处 DROP 后的 CREATE 前各注入一条
非法语句，三次都失败、**完整 schema 逐字节相同**、**逐表完整行快照逐字节相同**、11 项旧关键拒绝行为
全部照旧拒绝、`_sqlx_migrations` 无版本 30 成功记录（总数仍为 29）。

**Codex 已有的空库证明之外，本包补的是"存量业务行升级"这一半**：
前者的数据库是空的，后者有 4 个 Message、5 个历史 attempt、12 个 Event、Outbox 行、四元组幂等行、
以及 `tasks` / `references` 留存对象行。空库能过不代表存量库能过。

---

## 2. Python 展示路径的事务边界（**经实测，不是假设**）

任务表要求：不能认为"调用前 BEGIN 会自动跨 `executescript` 保留"。
本包把这句话**实测证伪**了，并给出可用的替代写法。

装置：`af02_python_tx.py`，4 条执行路径 × 3 个注入点 = 12 次观测，**0 失败**。
每个变体都在**有真实历史行**的 29 版库上运行。

| 执行路径 | 结果 | 失败后残留 |
|---|---|---|
| `poc-no-transaction`（原 POC 路径：autocommit + 裸 `executescript`） | **非原子** | schema 变、行变、守卫丢失 |
| `begin-then-executescript`（先 `BEGIN` 再 `executescript`） | **非原子** | 同上；调用方 `ROLLBACK` 报 `cannot rollback - no transaction is active`，`in_transaction=False` |
| `explicit-statement-loop`（`BEGIN IMMEDIATE` + 逐条 `execute` + 异常分支 `ROLLBACK`） | **原子** | 零残留 |
| `executescript-with-begin-in-script`（`BEGIN` 写进脚本文本，调用方回滚） | **原子** | 零残留 |

### 结论（必须写进实现说明）

1. **`executescript` 之前发出的 `BEGIN` 不会跨 `executescript` 保留。** 实测证据是失败分支里
   `ROLLBACK` 直接报 `cannot rollback - no transaction is active`，且事后 `in_transaction=False`。
   也就是说 `executescript` 自己把外层事务提交掉了，随后脚本的语句在 autocommit 下逐条生效。
2. 因此，**Python 展示路径不得使用裸 `executescript`**。若保留，两条可用写法（均已实测原子）：
   - 逐条 `execute` 包在一个显式事务里，异常分支显式 `ROLLBACK`（推荐）；
   - 把 `BEGIN` 写进脚本文本、由调用方负责 `ROLLBACK`。
3. 逐条切分必须用 `sqlite3.complete_statement`，因为 `CREATE TRIGGER ... BEGIN ... END;` 的
   分号不是语句边界。**且必须在剥掉 `--` 注释后的文本上判定完整性**——本包第一版没有剥注释，
   静默丢掉了 `CREATE TABLE`、`CREATE UNIQUE INDEX` 和第一条 `ALTER TABLE`，报出
   `no such table: message_route_overrides` 这种与真实原因无关的错误。正确切分结果是 19 条。
4. **这套测量只关于 CPython `sqlite3`，不能外推到 SQLx。** SQLx 的原子性由 §1 的
   真实 `Migrator` 实验独立证明，两者互不引用、互不替代。

---

## 3. 成功路径的联合验收

装置：`af02_success_matrix.py`，**39 项观测 / 0 失败**。概念迁移同样走 §2 实测为原子的那条路径。

### 3.1 三条写入路径

- **旧目标路径 INSERT**（`route_override_id IS NULL`，attempt 目标 = Message 冻结的 `to_role_id`）：接受。
- **新 override 路径 INSERT**：接受，且 attempt 携带的收件人与 Binding 与它所采纳的 override 一致
  （`attempt=B/bindB override=B/bindB`）。
- 三条必须拒绝的负例全部拒绝：旧路径瞄向别的 Role、attempt 收件人与所采纳 override 不符、
  只写 override id 不写编号（半截关联）。

### 3.2 UPDATE / REPLACE

合法状态机推进接受；改 attempt 收件人、改 attempt Binding 快照、改 override 目标 Role、
`INSERT OR REPLACE` 撞既有 override id、`INSERT OR REPLACE` 抢已占四元组幂等键——五项全部拒绝。

### 3.3 三次改派后旧 attempt 的快照

M1 经 **3 次改派**（A→B→C→A），每次都是完整命令事务。终态：

```
attempt 1: recipient=A binding=bindA override=-     <- 改派前的历史行
attempt 2: recipient=A binding=bindA override=-
attempt 3: recipient=B binding=bindB override=1
attempt 4: recipient=C binding=bindC override=2
attempt 5: recipient=A binding=bindA override=3
```

- 旧 attempt 1、2 在三次改派前后**逐字节相同**，收件人仍是 A，不被最新 override 改写。
- 逐 attempt 存储的读法与"取最新 override 重新解释"的读法在这份数据上**明确不同**：
  `stored=[(1,'A'),(2,'A'),(3,'B'),(4,'C'),(5,'A')]` vs `naive=[(1,'A'),(2,'A'),(3,'A'),(4,'A'),(5,'A')]`。
  这正是被撤回的旧推荐 (b) 会产生的差异。

### 3.4 Event 先于立即 FK 关联

`message_route_overrides.event_id` 是 IMMEDIATE 外键（29 个已发布 migration 无一声明 `DEFERRABLE`）。
实测：**先插 override 后插 Event 被拒**（`FOREIGN KEY constraint failed`）；
按"幂等记录 → Event → override → attempt → Message → Outbox → 幂等记录置终态"顺序则接受。
三次改派都走的这个顺序。

### 3.5 FC-02 留存联合矩阵

0028 保护的六个对象，各问 DELETE / REPLACE / 改身份 UPDATE，共 18 项，
**在"只有 29 个 migration"和"29 + 概念"两个库上逐项对比**：

- 18 项判定**完全一致**，且 18 项**全部是拒绝**。
- 0028 的 18 个留存 trigger 在概念迁移后**全部仍在**。
- 新对象 `message_route_overrides` 自己也带留存守卫（`DELETE` 被拒）。

---

## 4. 回退路径

### 4.1 回退**不是**"仅 DROP 守卫"或"移除注册行"

这两件事都不构成回退：

- **DROP 守卫**就是本包 §1 故障注入所证伪的那个失败形态本身。已实测：丢掉
  `delivery_attempts_team_consistency_insert` 之后，"瞄向别的 Role 的 attempt"立即变成
  `ALLOWED`。DROP 是**同事务前向替换的手段**，不是回退方案。
- **移除注册行**（改小 `_sqlx_migrations` 或改 migration 描述）只会让数据库状态与实际不符，
  既不会还原已提交的 DDL，也不会恢复守卫，只会让下一次迁移在错误的前提上运行。

### 4.2 允许的回退只有两条

1. **经审核的兼容二进制**：回退到"不认识 `message_route_overrides`、也不写新列"的已发布二进制。
   前提是该二进制对多出来的表和两列完全容忍——**这一点必须在实现包验证，本包未验证**（见 §6）。
2. **后续前向修复**：写一条新的 migration 把它改回去。已发布 migration 永不修改。

### 4.3 不得被弱化的东西

- **0028 留存不被弱化**：六个对象的 18 项判定在概念前后完全一致（§3.5）。
- **原 Message / attempt 历史不被弱化**：三处故障注入后逐表完整行快照逐字节相同（§1）；
  三次改派后旧 attempt 的收件人 / Binding / override 编号不被重解释（§3.3）。
- 概念**只新增**，不回写任何既有行：两个新列由 `ALTER TABLE` 添加，既有 attempt 取值为 `NULL`，
  这正是"该 attempt 走的是 Message 原寻址"的正确值。

---

## 5. 概念库证明的边界（**不是产品权限闭环**）

本包全部证据来自**一次性概念数据库**。它证明的是 DDL 原子性、守卫语义与留存不变量，
**不构成任何权限闭环**。以下仍然必须在应用执行点落地，概念库一条都没有证明：

| 仍需应用执行点 | 概念库为何证明不了 |
|---|---|
| **actor 真实性** | 概念库里 `actor_fingerprint` 是一个稳定 32 字节值，四元组只是被比对。真实 actor 身份由 K1 可信身份链给出 |
| **最终八层权限** | 概念库没有权限求值器。`Message reassign` 的 Owner `H` / Leader `S` 必须在服务层判定 |
| **版本 / TTL / 幂等** | `expected_version` 冲突、TTL 过期、幂等首次结果的稳定重放，都要在命令层实现；库里只有列 |
| **业务 Event / Outbox 投递** | 库里只证明 Event/Outbox 与业务变更同事务落库；**投递、重试、死信仍需 dispatcher** |
| **可见性隔离** | 跨 Team 的读拒绝要靠查询层，概念库不提供 |

一句话：**概念库证明 DDL 与守卫，不证明身份、不证明权限、不证明投递。**

---

## 6. 未验项（实现包必须接住）

1. 兼容二进制对新增表/新增列的实际容忍度（§4.2 前提）。
2. 生产 migration 的正式编号与 `no_tx=false` 的落地形态（本包只用临时 30）。
3. 真实 actor 链、八层权限、版本/TTL/幂等的应用层实现与回归。
4. Event/Outbox 的 dispatcher 投递与重试。
5. 通知 A/B 分类与 B-01～B-08 仍是 Human 待决输入，ADR-0007 仍 Proposed，本包未触碰。
6. 本包未验证多连接并发下的迁移锁行为（装置为单连接内存库）。
