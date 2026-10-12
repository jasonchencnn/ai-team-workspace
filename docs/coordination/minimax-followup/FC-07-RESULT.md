# FC-07｜执行回传（R3 重写包：两份方案 + 可执行概念验证）

- 关联任务：**FC-07**（`docs/coordination/MINIMAXCODE-FOLLOWUP-2026-10-08.md` §FC-07）
- 退回意见：`docs/reviews/MINIMAX-RETURN-AUDIT-2026-10-08.md` §3 **MR-05**、**MR-06**
- 性质：**只读产品方案包**。未实现任何生产代码、未占任何 migration 编号、未占任何 ADR 编号、未改任何冻结规格。
- **判定：`SELF_CHECK_PASS`**（本包范围内全部完成，验收命令 exit 0 且两次运行逐字节一致；**仍有 4 项须 Codex 先裁决**，见 §8）

---

## 1. 实际模型与环境

| 项 | 值 |
|---|---|
| Harness | MiniMax Code（本地 runtime） |
| 实际模型/版本 | `MiniMax-M3.1-Flash-Preview`（执行端回填，不假定为更高档位） |
| 会话 | Worker `mvs_7d0d7c14bcca4ced890452c7baec2524`（parent `mvs_71dc1bf32cd54569bc842fdb942a740d`） |
| 平台 | macOS darwin 27.0.0 arm64 |
| 仓库 | `/Users/chenzhiqing/Documents/MyVault/协作工作` |
| HEAD / 分支 | `1a14b8795e76fc373bef326eb5df96f9ac173f71` / `codex/wave-2-ci-validation` |
| 共享工作树 | **全部继承成果保留**；未执行 `reset` / `checkout` / `clean` / `stash` |
| 验证工具链 | Python 3.14.6 + SQLite **3.53.4**（`sqlite3.sqlite_version`），无第三方依赖 |
| 网络 | **无**。全程未联网、未下载、未新增依赖 |
| 新增费用 | **$0** |
| 提交/推送/部署 | **未** |
| cargo | **本包基本不跑 cargo**（见 §6「未运行项」） |

## 2. 输入与输出 SHA

### 2.1 输入（概念验证实际读取的 migration，哈希记录在 `INPUT-MIGRATIONS.txt`）

本包开始时 `crates/persistence-sqlite/migrations/` 有 **27** 个文件。运行途中 **FC-02 的 `0028_retained_entity_integrity.sql` 落入同一共享工作树**（计数→28），随后 **0028 本身又被改写一次**（20:16，20291→20976 字节）。

**因此输入漂移了两次。** 为使结论可复现、且不被后续漂移伪装成「装置坏了」，本包把运行当时的 **28 个 migration 冻结为副本** `docs/reviews/evidence/minimax-followup/FC-07/input-migrations/`，验证命令显式指向该快照：

```bash
python3 concept_route_override_poc.py --repo <repo> --migrations ./input-migrations
```

脚本对 migration **动态发现**（绝不硬编码计数）——硬编码 `!= 27` 正是把这次漂移变成 exit 2 装置故障的那一步（见 §5.7）。

```
INPUT-MIGRATIONS.txt（28 个文件逐个哈希）  SHA-256 e0a4dc1c6ae8fb7b286b1838bdf6c9f5f5a29ad4ddc9bf6576c967d62f0213d5
```

### 2.2 本包产出

| 文件 | SHA-256 |
|---|---|
| `docs/reviews/evidence/minimax-followup/FC-07/concept_route_override.sql` | `c0b4bbbff5a8fda6cc46eea59cc7accc1aef25ab1126366f591d89bccf56856f` |
| `docs/reviews/evidence/minimax-followup/FC-07/concept_route_override_poc.py` | `72a8017b424701c27cfb0c9dfab0884d9a2b7d145bb0044a3ef10f5641f76f45` |
| `docs/reviews/evidence/minimax-followup/FC-07/ADR-DRAFT-unnumbered.md` | `66ad97b4df5190bee39f4e4ff2c76acdf1483c0fdd13500a73acdbb9c3a81057` |
| `docs/reviews/evidence/minimax-followup/FC-07/01-concept-proof-run-1.log` | `bd78f2c382e38e2353ac1d07b085ee70bba2d2d1395771feba65f37cfee8ba03` |
| `docs/reviews/evidence/minimax-followup/FC-07/02-concept-proof-run-2.log` | `bd78f2c382e38e2353ac1d07b085ee70bba2d2d1395771feba65f37cfee8ba03` |
| `docs/reviews/evidence/minimax-followup/FC-07/03-determinism-check.log` | `b6126a4ff2989e7e47eb3d6ac99eca68f1d8c62b3fda856bc37878debea01ade` |

> 两个运行日志 SHA **相同**——这就是确定性证据本身（§5.3）。
>
> **注意**：本包第一次归档的两份日志 SHA 为 `7177dbf3…`，与上表不同。原因是归档之后 **0028 又被改写**，基于实时工作树的重跑自然得到不同输出。归档件已用冻结输入重跑并**整体替换**；`7177dbf3…` 不再作为交付证据。此处如实记录该次替换，不掩盖。

## 3. 读了哪些文件与规格条款

**规则**：`AGENTS.override.md`；`docs/development/README.md`、`TECH_STACK_AND_STANDARDS.md`（§4.1/§4.2/§4.4/§13）、`DEVELOPMENT_NODES.md`、`QUALITY_GATES.md`、`ADVERSARIAL_REVIEW_PLAYBOOK.md`（七条铁律 + 自检清单）。

**冻结规格**：`docs/specs/README.md`；`STATE_MACHINES.md` **§5**（Message/Delivery 全表，含 `:148` `NEEDS_HUMAN→QUEUED`、`:150` 历史不可改写）、**§6**（Notification/Attention）、§3 Run（末行「自动重试只读取终态 Run，创建新 Run」）、§4 Binding；`PERMISSION_MATRIX.md` **§3**（`Message reassign` 行）、§2（八层求交）、§5 Human-only；`DOMAIN_MODEL.md` §3/§5/§6/§7；`DATA_CONVENTIONS.md` **§3**（版本并发）、**§4**（幂等四元组）、**§5/§6**（留存）、§8（Event 同事务）；`ERROR_CATALOG.md`（§2–§11，重点 §4 `CONCURRENCY_IDEMPOTENCY_CONFLICT`、§9 Message/Notification）。

**ADR 与任务**：`docs/adr/0007-...md`（**仍 Proposed**）；`docs/reviews/MINIMAX-RETURN-AUDIT-2026-10-08.md` §3 MR-05/MR-06；`docs/coordination/MINIMAXCODE-FOLLOWUP-2026-10-08.md` FC-07 段。

**已落地迁移**：`0007`、`0014`、`0015`、`0016`、`0017`、`0018`、`0022`、`0023`、`0025`、`0026`、`0027`、`0028`。

**被退回的旧方案（作为被退回对象读取，未改动）**：`MM-R07-route-override-proposal.md`、`MM-R08-notification-decision-matrix.md`。

## 4. 两份方案的核心结构

### 4.1 `FC-07-route-override-proposal.md`

1. **退回意见逐条纠正表**（R07-1…5，每条附证据）
2. **冻结依据逐条表**（不扩展任何一句）
3. **数据结构**：追加式 `message_route_overrides` + `delivery_attempts` 两列 + 9 条新/重建守卫
4. **读模型**：每条 attempt 经**它自己的** override 读取既成事实；「当前投递到哪里」是另一个查询
5. **事务次序**：幂等记录 → **Event** → override → attempt → message → outbox → 幂等收口
6. **负向 N1–N15**
7. **概念验证结果**（两次改派不变性、次序、序号/前驱链、0028 前后对照）
8. **ADR 口径**（不改所有权重裁定 + 唯一待裁决点 + 未编号草案路径）
9. **遗留冲突 C1–C5**
10. **写集与停止条件**

### 4.2 `FC-07-notification-decision-matrix.md`

1. **分类原则**（A = 冻结文本逐字；B = 未定义须 Human 逐条）
2. **两轴事实**（`attention_type` × `source_type` 正交，先分维度再谈缺口）
3. **A 类 A-01…A-06**（仅冻结明写）
4. **B 类 B-01…B-08**（逐条：问题 / 推荐 / 成本 / 验收）
5. **闭环测试矩阵**（19 组，标 A/B 归属）
6. **授权与费用边界**（$0，无新渠道）
7. **明确不做的事**

## 5. 概念验证：命令、exit、日志

### 5.1 命令与结果

```bash
cd docs/reviews/evidence/minimax-followup/FC-07
python3 concept_route_override_poc.py \
    --repo /Users/chenzhiqing/Documents/MyVault/协作工作 \
    --migrations ./input-migrations
```

| 运行 | exit | 日志 |
|---|---|---|
| run 1 | **0** | `01-concept-proof-run-1.log`（SHA `bd78f2c3…`） |
| run 2 | **0** | `02-concept-proof-run-2.log`（SHA `bd78f2c3…`，**与 run 1 相同**） |
| 确定性检查 | — | `03-determinism-check.log` |

脚本自身语义：**0** = 全部探针执行且全部断言成立；**1** = 有断言不成立（真实发现）；**2** = 装置故障（**不得据此下任何结论**）。

结果：**42 条观察，0 条断言失败，0 条测量发现**，`SELF_CHECK of the concept proof: PASS`。

### 5.2 场景 A（核心）：三次改派后历史 attempt 含义不变

```
per-attempt fact after THREE reassignments:
  attempt#1  delivered_to=A  binding=/gen1  adopted_override=-    override: -
  attempt#2  delivered_to=B  binding=/gen1  adopted_override=1    override: to=B from=A
  attempt#3  delivered_to=C  binding=/gen2  adopted_override=2    override: to=C from=B
  attempt#4  delivered_to=A  binding=/gen2  adopted_override=3    override: to=A from=C
```

attempt#1/#2/#3 的整行与「两次改派时」**逐字段相同**；`Message.to_role` 与正文自始至终未动；Message version 6→17 每次业务变更 +1。

被退回的「最大 override」读法在同一份数据上把 #2/#3 读错；且因第三次改派回到 A，它对 #1/#4 **恰好碰对**——**错误是静默且依赖数据的**。

### 5.3 场景 B：事务次序满足立即 FK

| 探针 | 结果 |
|---|---|
| B1 幂等记录已存在、Event 未存在 | `FOREIGN KEY constraint failed`，**零残留** → R07 次序不成立 |
| B1c Event 已存在、幂等记录未存在 | `FOREIGN KEY constraint failed` → override 有**两个**立即 FK 目标 |
| B2 Event 先建 | 同一行**成功** |
| B3 Event+override 后 attempt 出错 | 回滚后 6 项计数/状态与事前**完全相同** |
| B4 `DEFERRABLE INITIALLY DEFERRED` | 实测可用；本仓 28 个 migration **零个**声明，不推荐 |

归因方法：SQLite 诊断不指明是哪一列被违反，故按**构造隔离变量**（只缺一个目标）归因，不解析错误字符串。

### 5.4 序号连续 / 前目标链：真实约束

- S5a：只带 `UNIQUE(message_id, override_number)` 的表**成功写入 `[1, 3]`**（洞在 2）→ 证明 UNIQUE **不**蕴含连续。
- S5b：概念库拒绝同样的洞，且拒绝**不来自** UNIQUE。
- S5d：序号合法时，仅把 `from_role` 写错即被前驱链拒绝，改对后接受 → 两条规则各自成立、互不冒充。

### 5.5 幂等四元组

S6a：同 Team、**同 key 文本**、不同 `actor_fingerprint` 的两行**都成立**（R07 的两列 UNIQUE 会拒绝第二行）；S6b：同四元组不同 payload 被拒（`0022` 自带 `replace_guard` 先于 UNIQUE 触发），应用层映射为既有 `CONCURRENCY_IDEMPOTENCY_CONFLICT`。

### 5.6 `delivery_attempts` 留存的前后对照（如实记录输入漂移）

| 输入 | `DELETE FROM delivery_attempts` | 守卫 |
|---|---|---|
| 隐去 0028（27 文件） | **ALLOWED** | 无 |
| 当前 28 文件 | **REFUSED** | `delivery_attempts_retained_entity_retention_delete` |

R07 §1.7 的缺口**已由 FC-02 的 `0028` 关闭**。本方案不重复实现，但**依赖它不回退**。

### 5.7 装置质量（`ADVERSARIAL_REVIEW_PLAYBOOK`）

遵守并被真实触发的规则：

- **规则 3（构造路径自检）**：所有 id 经唯一出口 `uid()`；**首次运行即被拦下**——种子 `bindings` 少一列 → 改为装置故障（exit 2）而非结论。
- **规则 1-b（基线双向断言）**：加载后**显式断言**四个对象在基线不存在、在概念存在，并断言两套 schema 指纹不同。
- **规则 2（装置故障 ≠ 结论）**：种子被拒、reassign 合法路径被拒、`reassign()` 缺回滚 → 全部改为显式 `APPARATUS FAULT`。
- **规则 2 的 no-op 伪装**：S7d 一度把 override#1 的 `to_role` 写成它**本来的 B**——纯 no-op，装置会输出「被拦下」的假象。由 `expect_fragment` + 值域比较抓出，改为写 D。
- **动态发现而非硬编码计数**：27→28 的漂移由硬编码 `!= 27` 变成装置故障（exit 2）抓出——**这正是手册警告的「基线偷偷变化」形态**，改为动态发现。
- **确定性必须在固定输入上验证**：第一次归档的两份日志虽然彼此相同，但随后一次基于实时工作树的重跑给出了**不同**摘要。差异被定位到 `0028` 在归档之后被再次改写。若不冻结输入，"装置坏了" 与 "输入变了" 无法区分——这与规则 2「装置故障与真结果不得同形」是同一件事。故归档件改为冻结输入重跑，并整体替换旧日志。
- **不靠解析错误字符串归因**：见 §5.3。

### 5.8 未运行项与理由

| 未运行 | 理由 |
|---|---|
| `cargo test` / clippy / build / rustdoc / fmt | 本包**不写任何 Rust**，无生产 crate 变更可测；本任务为只读方案包 |
| `pnpm lint` / typecheck | 未触碰前端 |
| 完整 Mac Gate（G0–G9） | 本包无实现交付；且需当前最终输入，由总控串行执行 |
| 覆盖率 | 无生产代码变更 |
| 0028 与本概念的联合冲突矩阵（除 S7i 外） | FC-02 仍在写入；0028 在本包运行中途才出现，尚不能作为稳定输入做穷举交互验证。**列为 §8 待裁决项** |
| 网络/依赖安装 | 明令禁止 |

## 6. 我如何逐条纠正被退回项

### 6.1 MR-05 / R07（5 处）

| # | 退回理由 | 纠正动作 | 可复验位置 |
|---|---|---|---|
| 1 | 路线 (b) 改变每次投递的事实快照含义、令历史 attempt 被重新解释、并借空 Binding snapshot 绕开真实快照 | **作废 (b)**。attempt 自带 `route_override_id/number` **及**真实 target/Binding/generation；`0017` 等值式重建为两条显式路径；全库不再有任何一行表达「当前收件人」 | S3 + 方案 §1/§3/§4 |
| 2 | `UNIQUE(message_id, override_number)` 只保唯一、不保无洞 | 保留 UNIQUE，另加 `..._contiguous_insert`（`= 1 + 已有行数`），并**先证伪** UNIQUE 蕴含连续 | S5a/S5b |
| 3 | `UNIQUE(team_id, idempotency_key)` 不是四元组 | 删除该索引；新表携带 `team_id + actor_fingerprint + command_name + idempotency_key` 并**复合 FK 指向 `0022`**，payload hash 判定不写第二套 | S6a/S6b |
| 4 | 有立即 Event FK 却把 Event INSERT 写在事务最后 | 改为 Event 先于 override；实测 28 个 migration **零个** `DEFERRABLE`，FK 全立即；并实测 deferred 备选可用但不推荐 | S4 B1/B2/B4 |
| 5 | （附带）`delivery_attempts` 缺留存守卫 | 做成可复现 before/after：27 文件 ALLOWED → 28 文件 REFUSED，归属 FC-02 的 `0028` | S7i + §5.6 |

### 6.2 MR-06 / R08（4 处）

| # | 退回理由 | 纠正动作 |
|---|---|---|
| 1 | A/B 自相矛盾；`PERMISSION_DENIED` 唯一 mandatory 集合属未冻结，执行者据 A 类会改冻结规格 | **整体移入 B-02**，并在 B-02 显式写明「为什么不能留在 A 类」；A 类中不再有任何 mandatory 集合条目 |
| 2 | 不得仅因无同名 `SYSTEM` 枚举就认定无合法类型；`SYSTEM` 是 `source_type`、`SECURITY_ALERT` 是 `attention_type`，不同维度 | §2 做两轴正交表；**不新增任何枚举值**；把「是否需新增类型」降级为 B-03 的**待裁决选项**而非结论 |
| 3 | D-01 重建示例错误（写成旧失败 Run retry 成功变 SUCCEEDED） | B-06 重写：旧 Run **永远**保持 FAILED；三者关系用表列出（旧 Run / 新 attempt / 处置证明），并加一条「尝试把旧 FAILED 改成 SUCCEEDED 必须被拒」的负向，且**本轮不实现绕过** |
| 4 | 上一轮缺 RESULT 与 evidence/模型输入命令归档 | 本文件 + evidence 目录补齐：模型/版本、输入 SHA（`INPUT-MIGRATIONS.txt`）、读过的条款、命令/exit/日志、风险与回退 |

## 7. 未伪造声明

- **未**覆盖、改写或删除 `MM-R07-route-override-proposal.md` 与 `MM-R08-notification-decision-matrix.md`（二者在共享工作树中保持原样）。
- **未**修改任何 migration 字节（`0001`–`0028` 全部只读；`git diff -- crates/persistence-sqlite/migrations/` 为空）。
- **未**改动任何冻结规格、节点状态或 ADR 状态；`ADR-0007` 仍 **Proposed**。
- **未**占用任何 migration 编号：概念迁移文件名**故意不带任何序号**。
- **未**占用任何 ADR 编号：草案为 `ADR-DRAFT-unnumbered.md`，`docs/adr/` 下无对应文件。
- **未**把待决条款写成已接受；**未**自判 DONE。
- 0028 是**别人（FC-02）**在本包运行途中写入的，本包如实记录为输入漂移，未据其存在宣称任何本包功劳。

## 8. 须 Codex 先裁决的问题

| # | 问题 | 我的建议 | 阻断了什么 |
|---|---|---|---|
| **Q1** | 重建 `0017` 的 `delivery_attempts_team_consistency_insert/update`（两条显式路径取代单一等值式）是否需要 ADR？ | 判定为**不改变所有权/身份/权限/协议**，故不自动需要 ADR；但**放宽一条已发布守卫本身值得一次显式签字**。未编号草案已备于 `ADR-DRAFT-unnumbered.md`，等裁决后才谈编号 | route 方案的 migration 编号签发 |
| **Q2** | 「当前投递到哪里」应落库为显式 `current` 指针，还是由应用层查最新 override？ | **不落库**：可变 current 列会引入冻结规格没有的失效竞态，且与「追加式历史」冲突 | route 方案的读模型定稿 |
| **Q3** | B-03：`source_type='SYSTEM'` 该用哪个**既有** `attention_type`？是否需要新增枚举值？ | 先判语义归属；**在裁决前不新增任何枚举值**。若确需新增 `attention_type`，属破坏性变更，建议单独立项 | 通知方案 B-03 及其测试矩阵 |
| **Q4** | 0028（FC-02）与本概念的**交互矩阵**尚未穷举验证——它在本包运行中途才落入共享工作树。 | 等 FC-02 定稿后，对 `0028` × 本概念做一次联合冲突回归（重点：`delivery_attempts` 的 replace/identity 守卫与本概念重建的 `identity_immutable_update` 是否重复或互相削弱） | route 方案的最终准入 |

## 9. 剩余风险与回退

| 风险 | 说明 | 回退 |
|---|---|---|
| **R-1 `0028` 回退** | 若 `delivery_attempts_retained_entity_retention_delete` 被回退，本方案「旧 attempt 保留」的核心承诺立即失去存储层防线 | 在实现写集中把该守卫列为**显式前置条件**，并加一条守护测试 |
| **R-2 共享工作树漂移（已实际发生两次）** | 本包运行期间输入从 27→28 个 migration，且 **0028 随后又被改写**。任何基于实时树的归档件都可能在下一次重跑时静默失效 | 已用 `input-migrations/` 冻结输入 + `INPUT-MIGRATIONS.txt` 逐文件哈希绑定；重跑时必须带 `--migrations ./input-migrations`。**不得**把本包结论外推到别的输入哈希 |
| **R-3 `DROP TRIGGER` 覆盖不足** | 本概念只重建 attempt 的两条一致性守卫；`0025`/`0028` 与之的完整交互未穷举 | Q4 的联合回归；实现阶段需逐条列出被 `DROP` 的触发器与重建后的**前后语义对照** |
| **R-4 deferred FK 分支未实现** | B4 只证明 SQLite 支持 `DEFERRABLE INITIALLY DEFERRED`，本方案**不采用** | 若 Codex 要求采用，需新增针对延迟 FK 的失败注入测试 |
| **R-5 授权判定仍在应用层** | 存储层只固化**结构**（actor 必填、身份列自洽、不可改写），「谁有权改派」属 `PERMISSION_MATRIX §2` 八层求交 | 实现阶段必须补 `MESSAGE_REASSIGN_DENIED` 的应用层用例；**本包不实现** |
| **R-6 概念库 ≠ 生产库** | 概念 SQL 在 evidence 副本，未注册、未进任何 crate | 实现写集由 Codex 签发时给出正式编号；概念文件**不得**直接改名搬入 `migrations/`（须重新评审编号与上游迁移号） |

**总回退**：本包只新增 5 个文档/证据文件 + 1 个 evidence 目录，删除即完全回退；未触碰任何生产资产、迁移字节或历史证据。

---

## 10. 判定

**`SELF_CHECK_PASS`**

- 本包写集内全部工作完成；概念验证 exit 0；两次独立运行**逐字节一致**（SHA-256 相同）。
- 42 条观察、0 条断言失败、0 条装置故障（装置故障均已修正后重跑，不产生结论）。
- 未实现生产代码、未占 migration 编号、未占 ADR 编号、未改旧方案与旧证据、未改冻结规格与 ADR 状态。
- **但**：`SELF_CHECK_PASS` **不等于**节点 `DONE`，**不等于**两份方案已获准。Q1–Q4 须 Codex 裁决；B 类条款须 Human 逐条决定。