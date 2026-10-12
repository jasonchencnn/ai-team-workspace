# FC-07｜Route Override 方案（Message 改派）— R3 重写

> 关联节点：DEV-02-08（后续 DEV-05-07）
> 状态：**只读产品方案。未实现任何生产代码、未修改任何迁移、未占用任何 migration 编号、未改任何冻结规格。**
> 被退回对象：`docs/coordination/minimax-return/MM-R07-route-override-proposal.md`（历史留痕，**未覆盖、未改写、未删除**）
> 退回意见：`docs/reviews/MINIMAX-RETURN-AUDIT-2026-10-08.md` §3 MR-05 / P1
> 可执行验证：`docs/reviews/evidence/minimax-followup/FC-07/`（概念库，非生产）

---

## 0. 一句话结论

改派必须是**每次投递自己的事实**，不是「Message 当前指向谁」的读模型展开。

本方案给 `DeliveryAttempt` 加两个列（它采用的 override 与序号），给 override 一张追加式载体；新 attempt **自己**携带它所采用 override 的真实 target Role、真实 Binding 与真实 generation。因此**任何一次后续改派都无法重新解释任何一条历史 attempt 的收件人**。原 `Message.to_role`、正文与全部旧 attempt 一行不动。

概念验证已在**真实 28 个 migration** 建的 SQLite 库上跑通，输出逐字节可复现（§7）。

---

## 1. 退回意见逐条纠正

| # | Codex 的退回理由（R07 的错误） | 本方案的纠正 | 概念验证证据 |
|---|---|---|---|
| R07-1 | 推荐路线 (b)：attempt 仍写原目标，真实改派收件人只放 override，target Binding 也脱离 attempt，查询依「当前最大 override」。这**改变了每次投递的事实快照含义**——历史 attempt 会被后续改派重新解释，且借空 Binding snapshot 绕开真实快照。 | **路线 (b) 作废。** 每个新 attempt 永久记录 `route_override_id` / `route_override_number`，并**自己**携带真实 `target_role_id`、`target_binding_id`、`target_binding_generation`。`0017` 的等值式由 `DROP TRIGGER` + 重建为**两条显式路径**，不再有任何一行写「当前收件人」。 | S3：一条 Message 连续改派 3 次后，attempt#1/#2/#3 的整行事实与两次时**逐字节相同**；同一条消息上「最大 override」读法把 #1/#2 读错（见 §7.1） |
| R07-2 | `UNIQUE(message_id, override_number)` **只保证序号唯一，不保证连续无洞**。 | 保留该 UNIQUE（唯一性仍需要），但**不依赖它**保证无洞。新增 `message_route_overrides_contiguous_insert`：要求 `override_number = 1 + (该 Message 已有 override 行数)`，从 0 行起强制 1、2、3…… | S5a：一张只带该 UNIQUE 的表**成功写入 1,3（洞在 2）**；S5b：概念库拒绝同样的洞 |
| R07-3 | `UNIQUE(team_id, idempotency_key)` **不是** `DATA §4` 的 actor/command/key **四元组**。 | 删除该索引。新表携带 `actor_fingerprint / command_name / idempotency_key` 三列（加上 `team_id` 恰为四元组），并以**复合外键指向 `0022 command_idempotency`** 的四元组——payload hash 判定仍只有 `0022` 一处实现，不写第二套。 | S6a：同 Team、**同 key 文本**、不同 actor 两行都成立（R07 的两列索引会拒绝第二行）；S6b：同四元组不同 payload 被拒 |
| R07-4 | override 有**立即 Event FK**，却把 Event INSERT 写在事务**最后**，次序不成立。 | 事务次序改为 Event 先于 override。实测本仓 28 个 migration **零个声明 `DEFERRABLE`**，故 FK 全部立即；SQLite 的诊断不指明是哪一列被违反，故按构造隔离变量（只缺一个目标）来归因。 | S4-B1：Event 未存在 → `FOREIGN KEY constraint failed`，且零残留；S4-B2：Event 先建 → 同一行成功 |
| R07-5 | （附带）`delivery_attempts` 缺留存守卫。 | **已由 FC-02 的 `0028` 关闭。** 本包实测并把它做成可复现的 before/after（§7.4）。本方案不重复做，也不得依赖它回退。 | S7i |

---

## 2. 冻结依据（逐条，不扩展）

| 约束 | 冻结原文 | 落地 |
|---|---|---|
| 改派边 | `STATE_MACHINES §5` `NEEDS_HUMAN → QUEUED`：副作用「新 attempt，记录 route override」 | 新表 + attempt 两列 |
| 目标必须合法 | 同表前置条件「新目标/Binding 有效，权限通过，TTL 有效」 | `..._target_insert` 要求同 Team + `status='ACTIVE'` 且 generation 相符的 Binding |
| 事件二选一 | 同表 Event 列 `MESSAGE_REASSIGNED`/`MESSAGE_REQUEUED` | `CHECK (from_role_id IS NOT to_role_id)` 把 `/` 的互斥变成机械判据；不改变目标的形状**不落本表** |
| attempt 历史不可改写 | `§5` 末「历史 attempt 和 Event 不可改写」 | 重建 `delivery_attempts_identity_immutable_update`，把 override 两列纳入冻结 |
| 正文与寻址冻结 | `DATA §5` | `messages_addressing_immutable_update` / `messages_freeze_body_after_queued` **零改动** |
| 版本并发 | `DATA §3` | 命令带 `expected_version`；`messages.version` 是唯一序列化点，败者 `CONCURRENCY_VERSION_CONFLICT`，不自动覆盖 |
| 幂等四元组 | `DATA §4` | 复用 `0022` 的 `command_idempotency` 四元组 + `payload_hash` |
| Event/Outbox 同事务 | `DATA §8` / `DOMAIN §6` | 事件事务次序见 §5 |
| 失败零副作用 | `STATE_MACHINES §1` | 单事务，任一步失败整体回滚（概念验证 S4-B3 逐表比对） |
| 留存 | `DATA §5/§6` | override 无条件拒 DELETE + `replace_guard` + 身份列 UPDATE 冻结 |
| 不级联硬删 | `DOMAIN §5` | 全部 FK `ON DELETE RESTRICT` |
| 单写者 | `DOMAIN §4`「Message/Thread → Messaging service」 | 新表只由 Messaging service 写 |

**本方案不新增任何状态、错误码、权限、协议字段或 wire 名。** 复用既有码：`MESSAGE_REASSIGN_DENIED`、`BINDING_NOT_FOUND`、`MESSAGE_TTL_EXPIRED`、`CONCURRENCY_VERSION_CONFLICT`、`CONCURRENCY_IDEMPOTENCY_CONFLICT`、`DOMAIN_INVALID_TRANSITION`（均已在 `ERROR_CATALOG`）。

---

## 3. 数据结构（设计稿；概念版见 evidence 目录）

```sql
CREATE TABLE message_route_overrides (
    id, team_id, message_id,
    override_number      INTEGER NOT NULL CHECK (>= 1),
    from_role_id, to_role_id,
    target_binding_id    BLOB    NOT NULL,      -- 改派时解析到的真实 Binding
    target_binding_generation INTEGER NOT NULL CHECK (>= 1),
    -- DATA §4 四元组（payload hash 的唯一实现在 0022）
    actor_fingerprint, command_name, idempotency_key,
    event_id             BLOB NOT NULL,        -- 立即 FK，故 Event 必须先存在
    request_id, created_at,
    CHECK (from_role_id IS NOT to_role_id),
    FOREIGN KEY (message_id)  REFERENCES messages(id)       ON DELETE RESTRICT,
    FOREIGN KEY (team_id)     REFERENCES teams(id)          ON DELETE RESTRICT,
    FOREIGN KEY (from_role_id/to_role_id) REFERENCES roles(id) ON DELETE RESTRICT,
    FOREIGN KEY (target_binding_id) REFERENCES bindings(id)  ON DELETE RESTRICT,
    FOREIGN KEY (event_id)    REFERENCES events(id)         ON DELETE RESTRICT,
    FOREIGN KEY (team_id, actor_fingerprint, command_name, idempotency_key)
        REFERENCES command_idempotency (team_id, actor_fingerprint, command_name, idempotency_key)
        ON DELETE RESTRICT
);
CREATE UNIQUE INDEX ..._message_number ON (message_id, override_number);  -- 唯一性；连续性另由触发器保证

ALTER TABLE delivery_attempts ADD COLUMN route_override_id BLOB REFERENCES message_route_overrides(id);
ALTER TABLE delivery_attempts ADD COLUMN route_override_number INTEGER;
```

新增/重建的守卫：

| 守卫 | 保证 |
|---|---|
| `..._contiguous_insert` | 序号从 1 连续无洞 |
| `..._predecessor_insert` | 第 1 条离开 `messages.to_role_id`，之后每条离开**前一条的** `to_role_id`（`IS NOT` 而非 `!=`，缺前驱也拒） |
| `..._target_insert` | 同 Team + `ACTIVE` Binding + generation 相符 |
| `..._no_delete_guard` / `..._replace_guard` / `..._identity_immutable_update` | 留存与不可改写 |
| `delivery_attempts_route_override_insert` | attempt 的收件人与 Binding **必须等于**它所采用的 override |
| 重建 `delivery_attempts_team_consistency_insert/update` | 两条显式路径（原始寻址 / 采用 override），取代单一等值式 |
| 重建 `delivery_attempts_identity_immutable_update` | override 两列随 attempt 历史冻结 |

**为什么两列都可空**：走原始寻址的 attempt 没有 override，NULL 正是它的正确值；SQLite `ALTER TABLE ADD COLUMN` 不改写任何既有行。

---

## 4. 读模型（新增，替代被退回的「最大 override」）

```sql
-- 每条 attempt 的既成事实：经由它自己采用的 override
SELECT a.attempt_number, a.target_role_id, a.target_binding_id, a.target_binding_generation,
       a.route_override_number, o.from_role_id, o.to_role_id
FROM delivery_attempts a
LEFT JOIN message_route_overrides o ON o.id = a.route_override_id;
```

**「当前投递到哪里」是另一件事**，应由应用层显式查询最新 override 得到，且该查询**不得**被任何历史 attempt 复用。

---

## 5. 事务次序（概念验证 B 段）

```
BEGIN IMMEDIATE
 1  expected_version 校验（失败 → CONCURRENCY_VERSION_CONFLICT）
 2  INSERT command_idempotency (IN_PROGRESS)      ← 立即 FK 目标 ①
 3  INSERT events (MESSAGE_REASSIGNED)            ← 立即 FK 目标 ②
 4  INSERT message_route_overrides
 5  INSERT delivery_attempts (route_override_id/number + 真实 target/Binding)
 6  UPDATE messages (NEEDS_HUMAN → QUEUED, version + 1)
 7  INSERT outbox
 8  UPDATE command_idempotency → SUCCEEDED, result_ref = override.id
COMMIT
```

**为什么是 2、3 在前**：28 个已发布 migration **零个**声明 `DEFERRABLE`，故全部 FK 立即生效。override 行同时立即依赖 `events` 与 `command_idempotency`，两者必须先存在。

**可选替代已实测但本方案不推荐**：SQLite 支持 `DEFERRABLE INITIALLY DEFERRED`（概念验证 B4 实测通过，可让 override 先于 Event），但那会让本仓首次出现延迟 FK 模式，并放弃「每条语句即时拒绝」的默认。**除非 Codex 明确要求，事件按 2→3→4 执行。**

---

## 6. 负向用例（全部真库、零业务副作用）

| # | 场景 | 期望 | 依据 |
|---|---|---|---|
| N1 | 跨 Team 改派目标 | 拒 | `DOMAIN §1` |
| N2 | 目标 Role 无 Binding | 拒 | `STATE §5:148` |
| N3 | Binding 已 `REVOKED` / 非 `ACTIVE` | 拒 | 同上 + `§4` |
| N4 | Binding 属于别的 Role 或 generation 不符 | 拒 | `0025` 同款判据 |
| N5 | TTL 过期 | 拒 `MESSAGE_TTL_EXPIRED`，零写入 | `STATE §5:148` |
| N6 | 序号有洞 / 不从 1 起 | 拒 | §3 |
| N7 | `from_role` 与前驱不符 | 拒 | §3 |
| N8 | `from = to` | CHECK 拒 | `STATE §5` 的 `/` |
| N9 | UPDATE / DELETE override | 拒 | `DATA §5/§6` |
| N10 | `INSERT OR REPLACE` 劫持 id 或四元组 | 拒 | `0013/0014/0016/0022` 既有模式 |
| N11 | attempt 收件人与其 override 不符 | 拒 | 本方案核心 |
| N12 | attempt 只填 override_id 不填序号 | 拒 | 同上 |
| N13 | 原路径 attempt 指向非 Message 目标 | 拒 | 重建后的 `delivery_attempts_team_consistency_insert` |
| N14 | 同四元组不同 payload | 拒 → `CONCURRENCY_IDEMPOTENCY_CONFLICT` | `DATA §4` |
| N15 | 两个并发改派 | 一成一败，败者版本冲突，不自动覆盖 | `DATA §3`、`TECH_STACK §13` |

---

## 7. 概念验证结果（详见 `docs/reviews/evidence/minimax-followup/FC-07/`）

命令：

```bash
cd docs/reviews/evidence/minimax-followup/FC-07
python3 concept_route_override_poc.py --repo /Users/chenzhiqing/Documents/MyVault/协作工作
# exit 0
```

输入：真实 28 个 migration（哈希见 `INPUT-MIGRATIONS.txt`）＋ 本目录概念迁移。两次独立运行输出**逐字节一致**（SHA-256 `7177dbf3…`）。

### 7.1 核心证明：三次改派后历史 attempt 含义不变

```
per-attempt fact after THREE reassignments:
  attempt#1  delivered_to=A  binding=/gen1  adopted_override=-    override: -
  attempt#2  delivered_to=B  binding=/gen1  adopted_override=1    override: to=B from=A
  attempt#3  delivered_to=C  binding=/gen2  adopted_override=2    override: to=C from=B
  attempt#4  delivered_to=A  binding=/gen2  adopted_override=3    override: to=A from=C
```

attempt#1/#2/#3 的整行与「两次改派时」**逐字段相同**；`Message` 的 `to_role` 与正文自始至终未动。

被退回的「最大 override」读法在同一份数据上：

```
attempt#1  attempt row says=A  largest override says=A
attempt#2  attempt row says=B  largest override says=A   <-- WRONG
attempt#3  attempt row says=C  largest override says=A   <-- WRONG
attempt#4  attempt row says=A  largest override says=A
```

**第三次改派回到 A 之后，该读法对 #1/#4 恰好「碰对了」**——错误因此是静默且依赖数据的，恰恰是最难被发现的一种。这正是必须让收件人成为 attempt 自身事实的原因。

### 7.2 事务次序

| 探针 | 结果 |
|---|---|
| B1 幂等记录已存在、Event 未存在 | `FOREIGN KEY constraint failed`，零残留 → **R07 的次序不成立** |
| B1c Event 已存在、幂等记录未存在 | `FOREIGN KEY constraint failed` → override 有**两个**立即 FK 目标 |
| B2 Event 先建 | 同一行**成功** |
| B3 Event 与 override 之后 attempt 出错 | 回滚后 `events/overrides/attempts/outbox/idem/message` 与事前**完全相同** |
| B4 `DEFERRABLE INITIALLY DEFERRED` | 实测可用，但本仓 28 个 migration **零个**声明，不推荐 |

### 7.3 序号与前驱链

S5a 在只带 UNIQUE 的表上成功写入 `[1, 3]`（洞在 2）；概念库拒绝同样的洞，且拒绝**不是**来自 UNIQUE。S5d 在序号合法的前提下，仅把 `from_role` 写错即被前驱链拒绝，改对后接受——两条规则各自成立且互不冒充。

### 7.4 `delivery_attempts` 留存：一次输入漂移的如实记录

本包开始时 `migrations/` 为 27 个文件，`0028_retained_entity_integrity.sql`（FC-02）在**本包运行途中**落入共享工作树，migration 计数因此从 27 变 28。

- 27 文件（隐去 0028）基线：`DELETE FROM delivery_attempts` **ALLOWED**
- 当前 28 文件基线：`REFUSED — Delivery attempt history is retained with the Team`，守卫为 `delivery_attempts_retained_entity_retention_delete`

**结论**：R07 §1.7 的缺口已由 FC-02 关闭。本方案**不依赖**它，但也**不得**让任何实现回退它；若该守卫日后消失，本方案的「旧 attempt 保留」承诺将立即失去存储层防线。

---

## 8. ADR 口径

**遵循已冻结的改派行为本身，不自动需要新 ADR。** `STATE_MACHINES §5:148` 已经写明「新 attempt，记录 route override」；本方案做的是把这句话变成可存储、可证明的形状，未改变所有权、身份、权限或协议含义。逐项自检：

| 维度 | 是否改变 | 依据 |
|---|---|---|
| 所有权 | 否 | `Message` 仍由 `messages.to_role_id` 表述；override 是 Message 聚合内的追加历史，不是新的聚合根 |
| 身份 | 否 | Role ID 不变；`DOMAIN_MODEL §2`「Role 是稳定身份」不受影响 |
| 权限 | 否 | `PERMISSION_MATRIX §3`「Message reassign」Owner `H` / Leader `S` 原样；授权仍在应用层八层求交 |
| 协议 | 否 | 无新状态、无新 wire 字段、无新错误码 |
| 聚合边界 | **需要 Codex 裁决** | 见下 |

**需 Codex 裁决的唯一一点**：本方案 `DROP TRIGGER delivery_attempts_team_consistency_insert/update` 并以两条路径重建。`DOMAIN_MODEL` 抬头写「任何改变对象身份、所有权或聚合边界的实现必须新增 ADR」。我的判断是这**不**构成聚合边界改变（`DeliveryAttempt` 仍在 `DOMAIN_MODEL §3`「隶属 Message 投递历史」，两条路径都由同一聚合保证），但**放宽一条已发布守卫**本身值得一次显式签字。

若 Codex 判定需要，本目录**已备好未编号的 ADR 草案**，需 Human 逐条确认后才占用正式编号：

`docs/reviews/evidence/minimax-followup/FC-07/ADR-DRAFT-unnumbered.md`

**本包未占用任何 ADR 编号，未改动 `ADR-0007` 的 Proposed 状态，未改动任何 Accepted 状态。**

---

## 9. 遗留冲突与待裁决

| # | 事项 | 需要的裁决 |
|---|---|---|
| C1 | 重建 `0017` 的 attempt 守卫是否需 ADR | Codex（草案已备，未编号） |
| C2 | 「当前投递到哪里」的读模型由应用层出查询，还是也落库为显式 current 指针 | Codex。**建议不落库**：可变的 current 列会引入规格没有的失效竞态 |
| C3 | 实现包的 migration 编号 | Codex 签发时给出，本包**未占用** |
| C4 | `0028` 留存守卫若被回退 | 阻断项，见 §7.4 |
| C5 | `delivery_attempts` 的替换/替换冲突守卫由 `0028` 覆盖，本方案不重复造 | 无需裁决，仅需实现时不重复 |

---

## 10. 写集与停止条件

* 本包写集：`docs/coordination/minimax-followup/FC-07-route-override-proposal.md`、`FC-07-notification-decision-matrix.md`、`FC-07-RESULT.md`、`docs/reviews/evidence/minimax-followup/FC-07/`。
* **未**修改任何 migration、源码、测试、冻结规格、节点状态或 ADR 状态。
* **未**覆盖/改写/删除 `MM-R07-route-override-proposal.md`。
* **未**占用 migration 编号，未提交/推送/部署，未联网、未下载、未新增依赖、未新增费用（$0）。
* 概念实现只存在于本包 evidence 副本，未写入任何生产 crate。
* 自验收不等于 `DONE`，不启动依赖节点。