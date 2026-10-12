# AF-02 RESULT — 路由概念的正式事务/回退及联合验收

**本包只改方案与装置，不碰产品实现。不自判 DONE（正式节点 DONE 权限属 Codex）。**

- 任务表：`docs/coordination/MINIMAXCODE-AUDIT-FOLLOWUP-2026-10-09.md` `## AF-02`（第 33–48 行）
- 权威审计：`docs/reviews/FC-RETURN-AUDIT-2026-10-09.md` `## 6`（第 78–88 行）
- 方案文档：`docs/coordination/minimax-audit-next/AF-02-route-implementation-contract.md`
- 证据目录：`docs/reviews/evidence/minimax-audit-next/AF-02/`

---

## 1. 输入固定

| 项 | 值 |
|---|---|
| 仓库 HEAD | `1a14b8795e76fc373bef326eb5df96f9ac173f71`（本包全程未变） |
| 分支 | `codex/wave-2-ci-validation` |
| 工作树 | 继承 dirty，未 reset / stash / clean / checkout |
| 已发布 migration | `crates/persistence-sqlite/migrations/*.sql` **29 个**，只读 |
| 概念 SQL | `concept_route_override.sql`，SHA-256 `c0b4bbbff5a8fda6cc46eea59cc7accc1aef25ab1126366f591d89bccf56856f` |
| 概念 SQL 来源 | 与 `fc-return-audit-2026-10-09/route-concept/` 及 `minimax-followup/FC-07/` **逐字节相同**（三份 SHA 一致） |
| 工具链 | rustc/cargo 1.98.0；sqlx 0.8.6；Python 3.14.6；SQLite 3.53.4 |
| 模型 | minimax-M3.1-Flash-Preview |
| 网络 / 费用 | 全程 offline，`--locked --offline`，新增费用 $0 |

## 2. 装置（新增文件）

| 文件 | 作用 | SHA-256 |
|---|---|---|
| `af02_common.py` | 29 版库构建 + **真实历史业务行**种子 + 完整 schema / 逐表完整行快照 + 11 项关键拒绝探针 | `8251a6708cb50606034e395e27dbd09b814a317ae538f861b4292f1f6197578c` |
| `af02_python_tx.py` | **Python `executescript` 事务边界实测**（4 路径 × 3 注入点） | `40520b1047df6e25f22a0a81406c5c8df4a95390c6a95bd5353964fea111f967` |
| `af02_success_matrix.py` | 成功路径联合矩阵（INSERT/UPDATE/REPLACE + FC-02 + 三次改派） | `f1869e9649c829075aad8005dcb94f5404077b4abb464ba4e0fa5657d79287ae` |
| `af02-sqlx/src/main.rs` | **真实 SQLx Migrator 故障注入**装置 | `04cf86d28ee825976395ae33ca986277330734b9c2ac33b9739dd9fc5b0d9f6a` |
| `af02-sqlx/Cargo.toml` | 独立 manifest，空 `[workspace]` 与根 workspace 完全隔离 | `bd51fb86636d986ece8341cb657208fe991a9884ee3caeb240401fea3e1ea905` |
| `af02-sqlx/Cargo.lock` | 独立 lock，171 包，**未改根 `Cargo.lock`** | `4d5b9c51d0707793693058af124bdd706c582396d34825c383f3094bd340cb8f` |
| `seed_historical.sql` | 历史行脚本，Python 与 Rust **同一份**，杜绝两边漂移 | `8dd44d281ed76c412481d319dd9295e867b77d50621066112b55d48779345cc3` |
| `refusal_probes.json` | 11 项关键拒绝探针，同上 | `5559b252a0e5e8c4595ac36d894d12f3a5f00e74a17640594af06f41851a5932` |
| `production-manifest.sha256` | 全部只读路径 171 个文件的 SHA-256 清单 | `4cc300e47f3d1b80ea99444eb411945628591aee816ce8063b697643218df999` |

原始日志（**未过滤，完整 stdout/stderr**）：
`sqlx-fault-injection.log`、`python-tx-boundary.log`、`success-matrix.log`、
`sqlx-attempt-1-repo-root-depth-failure.log`（装置第一版的失败留痕，见 §7）。

## 3. 验收命令与结果

```bash
export PATH="/Users/chenzhiqing/.minimax/toolchain/bin:$PATH"
export CARGO_NET_OFFLINE=true
cd docs/reviews/evidence/minimax-audit-next/AF-02

# 1) 实际 SQLx 迁移故障注入（已有本地依赖、locked、offline）
cargo run --manifest-path af02-sqlx/Cargo.toml --locked --offline   # EXIT=0

# 2) 独立概念测试
python3 af02_python_tx.py        # EXIT=0
python3 af02_success_matrix.py   # EXIT=0

# 3) 源码 / 生产差异 = 0
git status --porcelain -- crates/ Cargo.toml Cargo.lock scripts/ sources/ \
    docs/coordination/minimax-followup/ docs/adr/   # 与本包开工时逐项一致，见 §6
```

## 4. 实测结果

### 4.1 三处 DROP 故障注入（真实 SQLx Migrator，`no_tx = false`）

装置在**有真实历史业务行**的当前 29 版库上运行（4 个 Message、5 个历史 attempt、9 个 Event、
Outbox 行、四元组幂等行、`tasks` / `references` 留存对象行），在每处 `DROP TRIGGER` 之后、
对应的 `CREATE TRIGGER` 之前注入一条非法语句：

| 注入点 | 完整 schema | 逐表完整行快照 | 旧关键拒绝 | migration 成功记录 | 结论 |
|---|---|---|---|---|---|
| `delivery_attempts_team_consistency_insert` | 与前**逐字节相同** | 与前**逐字节相同** | 11/11 照旧拒绝 | version30=0，总数 29 | **失败零副作用** |
| `delivery_attempts_team_consistency_update` | 与前**逐字节相同** | 与前**逐字节相同** | 11/11 照旧拒绝 | version30=0，总数 29 | **失败零副作用** |
| `delivery_attempts_identity_immutable_update` | 与前**逐字节相同** | 与前**逐字节相同** | 11/11 照旧拒绝 | version30=0，总数 29 | **失败零副作用** |

三次错误均为 `near "THIS_IS_AUDIT_FAILURE": syntax error`（完整未截断，见日志）。
**30 仅是内存审计库的临时编号，不是生产编号。**

### 4.2 真实 RED 与正对照

装置先跑**绿基线**（无注入，概念迁移干净落地，5 个守卫齐全），再跑**负对照**：

- **负对照**：同一份故障 DDL，**去掉事务**（`raw_sql`，无显式事务）执行 →
  被 DROP 的 trigger **剩 0 份**、残留 `message_route_overrides` 表与 2 个 `route_override` 列、
  schema 与行**均变化**、且 `"attempt aimed away from the Message target"` **由 REFUSED 变成 ALLOWED**。
- 装置对该负对照写了硬断言（守卫必须消失、且至少一项关键拒绝必须丢失），否则退出码非 0。

没有这个负对照，上表三行只能证明"注入的故障是良性的"，不能证明"是事务救了它"。**负对照是红的，绿是事务带来的。**

### 4.3 Python `executescript` 事务边界（**实测证伪"调用前 BEGIN 会保留"**）

12 次观测 / **0 失败**：

| 路径 | 结论 | 失败后 |
|---|---|---|
| `poc-no-transaction`（原 POC） | **非原子** | schema 变、行变、守卫丢失 |
| `begin-then-executescript` | **非原子** | 同上；`ROLLBACK` 报 `cannot rollback - no transaction is active`，`in_transaction=False` |
| `explicit-statement-loop` | **原子** | 零残留 |
| `executescript-with-begin-in-script` | **原子** | 零残留 |

`begin-then-executescript` 那行就是任务表要求的那条证伪：`executescript` **不会**保留它之前的 `BEGIN`。
方案文档已把两条实测可用的替代写法写成硬要求，并写明这套测量**只关于 CPython `sqlite3`、不能外推 SQLx**。

### 4.4 成功路径联合矩阵

**39 项观测 / 0 失败**：

- **INSERT**：旧目标路径接受；新 override 路径接受且 attempt 携带 override 的真实收件人与 Binding
  （`attempt=B/bindB override=B/bindB`）；3 项负例全部拒绝。
- **UPDATE / REPLACE**：合法状态机推进接受；改收件人 / 改 Binding 快照 / 改 override 目标 /
  `INSERT OR REPLACE` 撞 id / 抢四元组幂等键——5 项全部拒绝。
- **三次改派**（A→B→C→A）后旧 attempt 快照不重解释：
  `attempt1=A/bindA/override-`、`attempt2=A/bindA/override-` 逐字节未变；
  逐 attempt 存储读法 `[A,A,B,C,A]` 与"取最新 override 重新解释"读法 `[A,A,A,A,A]` **明确不同**。
- **Event 先于立即 FK**：先插 override 后插 Event 被拒（`FOREIGN KEY constraint failed`）；
  按"幂等 → Event → override → attempt → Message → Outbox → 幂等置终态"顺序被接受。
- **FC-02 留存联合矩阵**：六对象 × (DELETE / REPLACE / 改身份) = **18 项**，
  在"仅 29 版"与"29 + 概念"两个库上**逐项一致，且 18 项全部是拒绝**；
  0028 的 **18 个留存 trigger 全部仍在**；新对象自身 DELETE 亦被拒。

### 4.5 确定性与冷启动重跑

`af02_python_tx.py` 与 `af02_success_matrix.py` 各连跑两次，输出**逐字节相同**。

随后把 `af02-sqlx/target/`（512 MB）与 `__pycache__/` 整个删除做**冷启动重跑**：
`cargo run --locked --offline` 从零重新编译并**再次 EXIT=0、三处故障注入仍全部零副作用**，
两份 Python 日志的 SHA-256 与删除前**完全一致**。装置不是靠缓存跑出来的。

## 5. 回退路径（摘要，详见方案文档 §4）

- **回退不是**"仅 DROP 守卫"——那正是 §4.1 故障注入证伪的失败形态，实测丢守卫后越权写立即变 `ALLOWED`。
- **回退不是**"移除注册行"——不还原已提交 DDL、不恢复守卫，只会让下一次迁移在错误前提上运行。
- 允许的回退只有：**经审核的兼容二进制**，或**后续前向修复**（已发布 migration 永不修改）。
- **0028 留存未被弱化**（18/18 一致拒绝），**原 Message / attempt 历史未被弱化**
  （三处故障后行快照逐字节相同；三次改派后旧 attempt 快照未变）。

## 6. 源码 / 生产差异 = 0

本包**只写入**两个新目录：

- `docs/reviews/evidence/minimax-audit-next/AF-02/**`
- `docs/coordination/minimax-audit-next/AF-02-route-implementation-contract.md`、`AF-02-RESULT.md`

机械证据：

1. `git status --porcelain` 对只读路径（`crates/`、`Cargo.toml`、`Cargo.lock`、`scripts/`、`sources/`、
   `docs/coordination/minimax-followup/`、`docs/adr/`）的输出，与本包开工时**逐项一致**，无新增、无删除。
2. `production-manifest.sha256` 给出上述路径全部 171 个文件的 SHA-256，可与复核者自己的快照直接 diff。
3. `crates/persistence-sqlite/migrations/` **仍是 29 个文件，未新增任何文件，未签任何生产编号**。
4. 概念 DDL 只存在于证据目录，文件名无任何序号前缀。
5. **未新增 production 依赖**：`af02-sqlx` 自带空 `[workspace]`，与根 workspace 完全隔离，
   其依赖全部是根 `Cargo.lock` 已有版本，根 manifest / lockfile 未被本包修改。

> 说明：`crates/observability/**` 等少数生产文件在本包执行期间 mtime 有变化，
> 那是**并行执行的其它整改包**（AF-01 拥有 `crates/observability/**`）与继承 dirty 状态所致，
> **不是本包写的**。本包的写入调用只指向上述两个目录，可用 `production-manifest.sha256` 复核。

## 7. 踩坑与留痕（不改结论，只留证据）

| 坑 | 实际发生 | 处理 |
|---|---|---|
| 失败发生在编译之前 | 首次 `cargo generate-lockfile` 因 path 依赖层数错（5 应为 6）报 `failed to load manifest`，**一个测试都没跑** | 修正层数后重跑；原始留痕保留在 `sqlx-attempt-1-repo-root-depth-failure.log` |
| 语句切分静默丢语句 | 第一版切分器在**未剥注释**的文本上判定完整性，因 `CREATE TABLE` 体内有 `--` 注释而提前截断，丢掉 `CREATE TABLE` / `CREATE UNIQUE INDEX` / 第一条 `ALTER TABLE`，报出与真因无关的 `no such table: message_route_overrides` | 改为在剥注释文本上判定，并修正 `complete_statement("")` 返回 True 的边界；正确结果 **19 条**，并留正对照 |
| 探针被别的守卫"代答" | 关键拒绝探针曾瞄向 Message 1，被 0028 单活跃位守卫先拒，导致守卫已丢仍显示"通过" | 探针改瞄 Message 3（其唯一 attempt 已终态）并加负对照硬断言，确保守卫一丢探针就变红 |
| 只留摘要 | 首版曾只保留 grep 摘要 | 现三份日志均为**未过滤完整 stdout/stderr**，含完整 `near "THIS_IS_AUDIT_FAILURE"` 与 `test result` 类计数行 |

**清理说明**：删除了 `af02-sqlx/target/`（512 MB）与 `__pycache__/`（56 KB），二者均为**本包自身可再生构建产物**，
由 `cargo build --locked --offline` 离线重跑即可重新生成（已验证）。**未删除任何既有 evidence**。

## 8. 未验项（交给实现包 / Codex）

1. 兼容二进制对新增表与两个新列的实际容忍度（回退路径第 1 条的前提）。
2. 生产 migration 的正式编号与 `no_tx=false` 的落地形态（本包只用临时 30）。
3. actor 真实性、最终八层权限、版本/TTL/幂等的应用层实现与回归。
4. 业务 Event/Outbox 的 dispatcher 投递、重试与死信。
5. 多连接并发下的迁移锁行为（装置为单连接内存库）。
6. 通知 A/B 分类与 B-01～B-08 仍是 Human 待决输入；ADR-0007 仍 Proposed，本包未触碰。

> **概念库证明的边界**：以上全部证据来自一次性概念数据库。它证明 DDL 原子性、守卫语义与留存不变量，
> **不是产品权限闭环**。身份、权限、投递三件事概念库一条都没有证明（详见方案文档 §5）。

**不自判 DONE。** 正式节点状态由 Codex 终审。
