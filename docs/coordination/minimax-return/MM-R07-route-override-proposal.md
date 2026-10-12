# MM-R07｜追加式 route override 方案（Message 改派）

> 关联节点：DEV-02-08（后续 DEV-05-07）
> 状态：**方案包，仅设计**。未实现任何代码、未修改任何迁移、未改变任何状态。
> 正式判定：Codex 复核后才签发实现包与 migration 编号。
> 授权：Human 2026-10-08 接续指令 + ADR-0008。

---

## 0. 一句话结论

冻结规格要求 `NEEDS_HUMAN → QUEUED` 产生「新 attempt，记录 route override」，
但当前存储层**没有任何语句可以合法表达一次改派**：原 `Message.to_role_id` 被
`0017` 冻结，新 attempt 又被 `0017` 强制等于该冻结值。本方案新增一张**追加式**
`message_route_overrides` 载体，**不改**原 Message、**不改**任何旧 attempt，
让 attempt 的目标从「当前有效 override」解析。

本方案**不新增**任何状态、权限、错误码或协议字段。

---

## 1. 我如何独立验证了审计的既有结论

### 1.1 审计结论我先接受，再独立复现

审计 §4 原文（`DEEP-AUDIT-2026-10-08.md:64`）：

> 冻结 §5 要求改派后新 attempt 与 route override，当前无专用历史载体，attempt
> 又须等于原 to_role。直接改不可变原目标本就应被拒；子反例目标未完整建立有效
> Binding，**不能把它计为合法改派被错误拒绝**。

我不推翻这条，而是**补上它要求的那个场景**。任务包禁止「仅原目标改写被拒就算
合法改派误伤」，所以我构造的是一个每个前置条件都被断言成立的完整改派。

### 1.2 可重放的判定路径

脚本：`docs/reviews/evidence/minimax-return/MM-R07/repro_route_override.py`
完整实录：`01-repro-transcript.log`

```bash
cd /Users/chenzhiqing/Documents/MyVault/协作工作
python3 docs/reviews/evidence/minimax-return/MM-R07/repro_route_override.py --repo .
# exit 0
```

脚本从**真实的 27 个 migration 文件**建库（不是手抄 schema），`PRAGMA
foreign_keys = ON`，并遵守 `ADVERSARIAL_REVIEW_PLAYBOOK` 的三条规矩：

| 规矩 | 本脚本怎么落实 |
|---|---|
| 规则 3：被测数据的构造路径必须可自检 | 每个 BLOB id 只经 `uid()` 一个出口，自动校验长度 32 与 `substr(hex,13,1)='7'`、`substr(hex,17,1)∈89AB`。**第一次运行就是被这条拦下的**：我手写的 id 是 31 位，INSERT 在第一条语句就失败——这正是手册警告的「假拦截」形态 |
| 规则 2/3：装置故障与真结果不得同形 | 种子逐表行数断言 + `pragma foreign_key_check` + 「改派目标确有 ACTIVE 且生效中的 Binding」+「两 Role 同 Team」+「Message 确为 NEEDS_HUMAN 且 TTL 有效」全部先断言再探 |
| 规则 2：不把 no-op 伪装成结论 | 每条探针的写入值都与种子值不同；正反对照成对 |

脚本**不预设任何期望值**，只记录观察结果，因此不存在「预期写错导致假红」。

### 1.3 合法对照先成立（否则整个缺口论证塌掉）

审计强调不能拿「原目标改写被拒」冒充误伤。合法对照是这条边的**另一半**：

`STATE_MACHINES.md:148` 的 Event 列是 `MESSAGE_REASSIGNED`/`MESSAGE_REQUEUED`。
按该规格自身的标点约定（`，`=都发，`/`=互斥，全规格仅两行用 `/`，
另一行是 `STATE_MACHINES.md:107` 的 `ROLE_BOUND`/`ROLE_REBOUND`），
这两个事件是**同一条边的两种形状**：

* `MESSAGE_REQUEUED`：目标不变，绑定修好了 → 重投给原目标；
* `MESSAGE_REASSIGNED`：目标改成另一个 Role。

实测（§1 transcript）：`NEEDS_HUMAN → QUEUED` 本身 **ALLOW**，
为**同一冻结目标**建新 attempt 也 **ALLOW**。

**这条对照是缺口论证的地基**：它证明该迁移边本身可达、被拒的不是这条边。

### 1.4 真缺口：五种语句形状逐一体测

| # | 形状（应用层在冻结规则下均可合法发起） | 实测 | 拒绝者 |
|---|---|---|---|
| S1 | 新 attempt 指向改派目标（Message 一字不动） | **REFUSE** | `Delivery attempt Team consistency violation` |
| S2 | 就地把冻结 Message 改向目标 | **REFUSE** | `Message addressing is immutable` |
| S3 | 删掉冻结行再以新目标重建 | **REFUSE** | `Message history is retained with the Team` |
| N1 | 改派到**跨 Team** 的 Role | REFUSE | `Delivery attempt Team consistency violation` |

S2、S3 被拒是**正确的**（`DATA_CONVENTIONS §5` 正文与寻址随 QUEUED 冻结、
`§5/§6` 不删历史）。所以缺口不是「守卫太严」，而是：

> **S1 是规格明写的那条边，而它无法表达。**

### 1.5 精确定位到唯一失败的合取项

拒绝信息只报 trigger 名，不报哪条比较失败。我没有靠「哪条 INSERT 红了」反推，
而是把 `0017:259` 的 `WHEN` 子句**逐条求值**：

```
PASS  message exists         = 1
PASS  team matches           = 1
FAIL  TARGET matches         = 0      <- message.to_role_id = attempt.target_role_id
PASS  binding snapshot ok    = 1
```

**恰好一条失败**。Team、存在性、Binding 快照三项全部满足。
拒绝可唯一归因于那一条被冻结的等值式。

### 1.6 无载体

28 张表全列出，名称含 `override` / `route` / `reassign` 的：**0 张**。
`STATE_MACHINES.md:148` 明写的「记录 route override」没有任何落点。

### 1.7 附带发现：`delivery_attempts` 没有留存守卫

这条**不在**任务包给定的 RED 输入里，但它是本方案的直接依赖，如实上报：

* `DATA_CONVENTIONS §5`：「Event、Run、Binding、Review decision、**DeliveryAttempt**、
  Approval decision 不更新历史语义，只追加后续事实」；
* `DATA_CONVENTIONS §6` 留存表：「Message/**Delivery**/Notification/Approval 随 Team 保留」；
* `0026_core_history_retention.sql` 覆盖 runs / reviews / artifacts / approvals / messages，
  **不含 delivery_attempts**；`0013`/`0014` 的 messages 守卫是**有引用行才生效**的条件式。

实测：`DELETE FROM delivery_attempts` **ALLOW**（1→0 行），
同一时刻 `DELETE FROM messages` **REFUSE**（0026 的守卫确实在，不是装置没加载）。

**为什么它对本方案是承重的**：`STATE_MACHINES.md:150` 要求「历史 attempt 和 Event
不可改写」，而改派方案的核心承诺就是「原 Message 与旧 attempts 保留」。
若 attempt 可被物理删除，这条承诺在存储层没有防线——改派后的旧投递痕迹可以消失。

> **交总控裁决**：这是独立缺陷，建议单列一个前向迁移补齐，**不要**塞进本方案的
> migration，以免两件事互相掩盖。本方案第 5 节已把它列为设计的前置条件。

---

## 2. 单写者前向 schema 方案

### 2.1 设计原则

| 原则 | 落地 |
|---|---|
| **单写者** | `DOMAIN_MODEL §4`：「Message/Thread → Messaging service」。新表只由 Messaging service 写，UI / Adapter / Activity Stream 一律只读。表上以触发器固化「只追加、不可改、不可删」 |
| **原 Message 不改** | 不动 `messages` 任何一列。`0017` 的 `messages_addressing_immutable_update` **保持原样**，一条字都不改 |
| **旧 attempt 不改** | 不动 `delivery_attempts` 任何一行既有数据。`0017` 的 `delivery_attempts_team_consistency_*` 也保持原样 |
| **前向迁移** | `0001`–`0027` 零修改，SQLx checksum 不漂移 |

### 2.2 关键取舍：解析方向

有两条路：

**路线 A（否决）**：放宽 `0017` 的 `delivery_attempts_team_consistency_*`，
让 `attempt.target_role_id` 可以 ≠ `message.to_role_id`。

否决理由——**它会造成已落地守卫的静默削弱**：

1. `0025` 的升级守卫（`0025:86-101`）和 `delivery_attempts_target_role_team_*`
   都以「attempt 自洽」为前提；放宽 0017 后，`0017` 原本拦住的
   「借 DRAFT 洞跨 Team 改向」将失去一半防线，只能靠 0025 单独兜底——
   而 `0025` 的 attempt 分支**恰恰依赖** 0017 的等值式（见 MM-R01 §9 风险表：
   「该分支是纵深防御，本包不声称它能被独立触发」）。放宽 0017 会让那份
   「不声称」的诚实声明变成实际生效的语义变化。
2. `0017:294` `delivery_attempts_target_binding_insert` 要求 Binding 必须指名
   `NEW.target_role_id`；放宽后该守卫的语义从「快照指名收件人」漂移成
   「快照指名任何人」，反而变松。
3. 已发布的 `0017` **不可修改**（`TECH_STACK_AND_STANDARDS §4.4`）。放宽只能靠
   新迁移 `DROP TRIGGER` + 重建，那是「静默削弱既有守卫」，必须走 ADR。

**路线 B（采用）**：新增载体，attempt 目标从「当前有效 override」解析，
**原 Message 与 0017 一字不改**。

路线 B 的代价要说清楚：`0017` 的等值式**依然成立**（原 Message 的
`to_role_id` 永远等于它的 attempt），只是它不再描述「当前收件人」。
解析规则从「读 `messages.to_role_id`」变成「读当前有效 override，
没有则回落到 `messages.to_role_id`」。这是一处**读模型语义**的扩展，
不是对 `0017` 的放宽。

### 2.3 数据结构（设计稿，非最终 SQL）

```sql
CREATE TABLE message_route_overrides (
    id BLOB PRIMARY KEY NOT NULL CHECK (...uuidv7 16-byte...),
    team_id BLOB NOT NULL CHECK (...uuidv7...),

    -- 改派所依附的那条 Message。RESTRICT：不级联，历史不因上游消失。
    message_id BLOB NOT NULL CHECK (...uuidv7...),

    -- 同一 Message 上第几条 override。从 1 递增，单调。
    override_number INTEGER NOT NULL CHECK (typeof(override_number)='integer'
                                            AND override_number >= 1),

    -- 改派「离开」的角色（快照，可与原 Message 不同，但必须同 Team）
    from_role_id BLOB NOT NULL CHECK (...uuidv7...),
    -- 改派「指向」的角色
    to_role_id   BLOB NOT NULL CHECK (...uuidv7...),

    -- 改派时解析到的目标 Binding 快照（PERMISSION_MATRIX §3 要求 Binding 有效）
    target_binding_id BLOB CHECK (NULL | uuidv7),
    target_binding_generation INTEGER CHECK (NULL || >= 1),

    -- 授权主体。谁改派的，不可改写。
    actor_type TEXT NOT NULL CHECK (actor_type IN ('HUMAN','ROLE')),
    actor_role_id BLOB CHECK (NULL | uuidv7),      -- ROLE 时必填
    actor_human_id BLOB CHECK (NULL | uuidv7),     -- HUMAN 时必填

    -- 命令幂等键（DATA_CONVENTIONS §4）：同一逻辑重试复用
    idempotency_key TEXT NOT NULL CHECK (length(trim(idempotency_key))>0
                                        AND length(idempotency_key)<=128),
    request_id BLOB CHECK (NULL | uuidv7),

    -- Event/Outbox 同事务（DATA_CONVENTIONS §8 / DOMAIN_MODEL §6）
    event_id BLOB NOT NULL CHECK (...uuidv7...),

    created_at INTEGER NOT NULL CHECK (typeof(created_at)='integer'),

    -- from ≠ to：一条「没有实际改向」的记录不叫 override，
    -- 那是 STATE_MACHINES 的 MESSAGE_REQUEUED 形状，不落本表。
    CHECK (from_role_id IS NOT to_role_id),
    -- actor 与其身份列必须自洽
    CHECK ((actor_type='HUMAN' AND actor_human_id IS NOT NULL
                        AND actor_role_id IS NULL)
        OR (actor_type='ROLE'  AND actor_role_id  IS NOT NULL
                        AND actor_human_id IS NULL)),
    -- Binding 快照两列同进同出
    CHECK ((target_binding_id IS NULL) = (target_binding_generation IS NULL)),

    CONSTRAINT message_route_overrides_message_fk
        FOREIGN KEY (message_id) REFERENCES messages (id)
        ON UPDATE RESTRICT ON DELETE RESTRICT,
    CONSTRAINT message_route_overrides_event_fk
        FOREIGN KEY (event_id) REFERENCES events (id)
        ON UPDATE RESTRICT ON DELETE RESTRICT,
    CONSTRAINT message_route_overrides_team_fk
        FOREIGN KEY (team_id) REFERENCES teams (id)
        ON UPDATE RESTRICT ON DELETE RESTRICT,
    CONSTRAINT message_route_overrides_from_role_fk
        FOREIGN KEY (from_role_id) REFERENCES roles (id)
        ON UPDATE RESTRICT ON DELETE RESTRICT,
    CONSTRAINT message_route_overrides_to_role_fk
        FOREIGN KEY (to_role_id) REFERENCES roles (id)
        ON UPDATE RESTRICT ON DELETE RESTRICT
);

-- 同一 Message 的 override 序号唯一
CREATE UNIQUE INDEX message_route_overrides_message_number
    ON message_route_overrides (message_id, override_number);
-- 幂等键在 Team 内唯一（DATA_CONVENTIONS §4；Message retry 已被点名必须幂等）
CREATE UNIQUE INDEX message_route_overrides_idempotency
    ON message_route_overrides (team_id, idempotency_key);
CREATE INDEX message_route_overrides_message
    ON message_route_overrides (message_id, created_at);
CREATE INDEX message_route_overrides_team
    ON message_route_overrides (team_id);
```

### 2.4 「当前有效 override」的解析规则

> **取该 Message 上 `override_number` 最大的一行**；没有则回落到 `messages.to_role_id`。

`override_number` 由 `UNIQUE(message_id, override_number)` 保证无洞无重复，
最大行即最新，时序确定、并发下也只有一个赢家（见 §4.3）。

**为什么不用「当前」状态列**：那需要一个可变列，而 `DATA_CONVENTIONS §5` 要求
历史只追加。序号最大是纯读操作，无需写状态，也就没有「谁把它置回失效」的竞态。

**为什么不用 `created_at` 排序**：同毫秒并列时 `DATA_CONVENTIONS §2` 要求用
`(occurred_at, event_id)` 才稳定；`override_number` 是单列单调整数，
天然免掉这个歧义。

---

## 3. 约束清单

| 约束 | 冻结依据 | 落地方式 |
|---|---|---|
| **同 Team** | `DOMAIN_MODEL §1`（Team 是隔离边界） | 触发器直陈：`to_role_id` 与 `message.team_id` 同 Team；`from_role_id` 同理。**不用**「等于 message.to_role_id」这种间接写法——改派后二者本就可能不同 |
| **actor 授权** | `PERMISSION_MATRIX §3`「Message reassign」：Owner `H`、Leader `S`（仅被显式授权者）、其余 `—`；§5 第 7 项 | 存储层只能固化**结构**（actor 必填、身份列自洽、不可改写）；**授权判定属于应用层**求交（`PERMISSION_MATRIX §2` 八层）。存储层不新增权限判定逻辑 |
| **Binding 有效性** | `STATE_MACHINES.md:148`「新目标/Binding 有效」；`:138`「目标 Role ACTIVE；解析当前 ACTIVE Binding」 | INSERT 时校验 Binding 存在、同 Team、`role_id = to_role_id`、`binding_generation` 相符（沿用 `0025:227` 同款判据）。与 `0017` 的 `delivery_attempts_target_binding_insert` 同构 |
| **版本与乐观锁** | `DATA_CONVENTIONS §3`（`expected_version`，不匹配 `CONCURRENCY_VERSION_CONFLICT`，不自动覆盖） | 命令带 `messages.expected_version`；**消息聚合版本 `+1` 由既有 `messages.version` 承载**，新表不自建聚合版本。并发两个改派 → 一个成功、一个 `CONCURRENCY_VERSION_CONFLICT` |
| **TTL** | `STATE_MACHINES.md:148`「TTL 有效」；`ERROR_CATALOG` `MESSAGE_TTL_EXPIRED` | 校验 `now < messages.expires_at`。过期时不写 override、不写 attempt，返回**既有** `MESSAGE_TTL_EXPIRED`，不新增码 |
| **幂等键** | `DATA_CONVENTIONS §4`；`:49` 点名「Message retry 必须幂等」 | 表内 `UNIQUE(team_id, idempotency_key)` + `replace_guard`（沿用 `0022` 同款模式，防 `INSERT OR REPLACE` 劫持 `event_id`） |
| **Event/Outbox** | `DATA_CONVENTIONS §8`；`DOMAIN_MODEL §6`；`STATE_MACHINES.md:148` | `event_id` NOT NULL 且 FK 到 `events`；事件类型用**既有** `MESSAGE_REASSIGNED`，`subject_type='MESSAGE'`、`subject_id=message_id`（`0006:32-38` 已含 `MESSAGE`，**无需扩展枚举**）。改派与 `NEEDS_HUMAN → QUEUED` 同事务 |
| **历史外键** | `DOMAIN_MODEL §5`「不级联硬删除」 | 三个 FK 全部 `ON DELETE RESTRICT`。override 行反过来**保护** Message：Message 想删必须先处理 override |
| **留存** | `DATA_CONVENTIONS §6`；`0026` | 触发器无条件拒绝 DELETE，并加 `replace_guard`。语义与 `0026` 对 `messages` 的做法逐条对齐（见 §6.2） |

### 3.1 「改派」与「重投」的区分

`from_role_id IS NOT to_role_id` 是 CHECK 的一部分。这不是新增语义，而是把
`STATE_MACHINES.md:148` 已经用 `/` 区分开的两件事落到存储层：

* 目标不变 → 不写 override 表，只发 `MESSAGE_REQUEUED`（**今天已可用**，§1.3 实测）；
* 目标改变 → 写 override 表，发 `MESSAGE_REASSIGNED`。

这样 `/` 的互斥性有了机械判据，不必依赖应用层自觉。

---

## 4. 合法对照与失败零副作用

### 4.1 合法对照（必须成立）

同 §1.3 的场景，改为走新表：

1. Message 在 `NEEDS_HUMAN`；`to_role_id = unbound`；TTL 有效；
2. Human Owner 对同 Team 的 `bound` Role 执行改派；
3. 同事务：INSERT override 行 + INSERT 新 attempt（`target_role_id = bound`）
   + UPDATE `messages.status = 'QUEUED'`、`version + 1` + Event + Outbox。

**新 attempt 怎么合法地指向 `bound`？** 这是方案的关键接缝：

新 attempt 的 `target_role_id` 必须满足 `0017` 的 `message.to_role_id =
attempt.target_role_id`。两种做法：

* **(a) 允许 attempt 指向 override 目标**，并让 0017 的等值式加一个同义分支
  （`attempt.target_role_id = message.to_role_id OR attempt.target_role_id =
  某条 override 的 to_role_id`）。这需要 `DROP TRIGGER` + 重建 → 触发 §2.2 的
  ADR 讨论。
* **(b) 让 0017 的等值式原样保留，新 attempt 仍写 `message.to_role_id`**，
  「当前收件人」只存在于 override 表与投递决策里，attempt 的 `target_role_id`
  退化为「该 Message 的原始寻址」。

**本方案推荐 (b)**，理由：

* 0017 **零改动**，`0025` 对 0017 的依赖关系**完全不受影响**；
* §1.5 已证明被拒的正是那一条等值式——**放松它才是危险动作**；
* 改派后的收件人由 override 表 + Router 决定，而 `STATE_MACHINES.md:138`
  明确 Router 的职责就是「解析当前 ACTIVE Binding」，本来就带解析步骤；
* 代价是 `delivery_attempts.target_role_id` 的**语义**从「本次投递的收件人」
  退化为「该 Message 的原始寻址」。这是**读模型语义的澄清**，不是放宽约束。

**(b) 的诚实代价 —— 我实测了，它不是免费的**：

(b) 下新 attempt 的 `target_role_id` 被钉死在**原目标**，于是
`0017:294 delivery_attempts_target_binding_insert`（要求 Binding 快照必须指名
`NEW.target_role_id`）会**拒绝**把改派目标的 Binding 挂到该 attempt 上。

实测（`01-repro-transcript.log` §5）：

| 探针 | 结果 |
|---|---|
| B1：attempt 指原目标 + 挂改派目标的 Binding | **REFUSE**（`Delivery target Binding snapshot violation`） |
| B2：同一 attempt，不带 Binding 快照 | **ALLOW** |

即：**(b) 可写，但改派目标的 Binding 快照只能存在新 override 表里，不能随 attempt 走。**
这削弱了「attempt 自带收件方执行者快照」的审计直观性，必须写进实现说明。

**(b) 的完整代价清单**：
1. `delivery_attempts.target_role_id` 语义退化为「该 Message 的原始寻址」；
2. 改派目标的 Binding 快照不在 attempt 上，只在 override 表上；
3. 按 `delivery_attempts.target_role_id` 做「本次投给谁」的统计必须先解析 override。

> **需 Codex 裁决**：若总控认为 (b) 的三项代价不可接受，则走 (a)，
> 但 (a) 必须先有 ADR 明确「0017 的等值式放宽是受控的语义变更」。

### 4.2 负向用例

| # | 场景 | 期望 | 依据 |
|---|---|---|---|
| N1 | 改派到**跨 Team** 的 Role | 拒（`AUTH_PERMISSION_DENIED`） | `DOMAIN_MODEL §1` |
| N2 | 目标 Role **无 ACTIVE Binding** | 拒（`BINDING_NOT_FOUND`） | `STATE_MACHINES.md:148` |
| N3 | Binding 已 `REVOKED` 或 `effective_to` 已过 | 拒（`BINDING_NOT_FOUND`） | 同上 + `STATE_MACHINES §4` |
| N4 | **TTL 过期** | 拒（`MESSAGE_TTL_EXPIRED`），零写入 | `STATE_MACHINES.md:148` |
| N5 | actor 是 Developer/QA（矩阵 `—`） | 拒（`MESSAGE_REASSIGN_DENIED`） | `PERMISSION_MATRIX §3`（**既有码**，`ERROR_CATALOG §9`） |
| N6 | 改 `from_role_id = to_role_id` | 拒（CHECK） | §3.1 |
| N7 | 改派一条**不在 `NEEDS_HUMAN`** 的 Message | 拒（`DOMAIN_INVALID_TRANSITION`） | `STATE_MACHINES.md` 表只有这一行 |
| N8 | UPDATE/DELETE override 行 | 拒（不可变 + 留存） | `DATA_CONVENTIONS §5/§6` |
| N9 | `INSERT OR REPLACE` 同 `id` | 拒（`replace_guard`） | `0022` 已确立的模式 |
| N10 | 复用 `idempotency_key` 但 payload 不同 | 拒（UNIQUE → `CONCURRENCY_IDEMPOTENCY_CONFLICT`） | `DATA_CONVENTIONS §4` |

### 4.3 竞争

两个改派并发打同一 Message：

* 都带同一个 `expected_version`；
* `messages.version` 乐观锁是**唯一**的序列化点（`DATA_CONVENTIONS §3`）；
* 胜者写 override + attempt + `version+1`；
* 败者收到 `CONCURRENCY_VERSION_CONFLICT`，**不自动覆盖**
  （`TECH_STACK_AND_STANDARDS §13` 明禁「最后写入者获胜」）。

`UNIQUE(message_id, override_number)` 是第二道防线：即便乐观锁被绕过，
同序号也只能成功一次。

### 4.4 失败零副作用

改派是**单事务**内四步：override INSERT → attempt INSERT → message UPDATE
→ Event + Outbox INSERT。

任一步失败 → 整体回滚。存储层保证：

* Event/Outbox 与业务变更同事务（`DATA_CONVENTIONS §8`）；
* 通知失败不回滚业务 Event（`TECH_STACK_AND_STANDARDS §4.1.8`）；
* **旧 attempt 一行不动**——新表是纯 INSERT，不 UPDATE 任何既有行。

§1 transcript 已实测：S1/S2/S3 三种失败形态下
`message unchanged: True`、`delivery_attempts rows: 0`。

---

## 5. 回退方案

| 层 | 回退 |
|---|---|
| **代码** | 移除 `src/lib.rs` 里新迁移的常量与 `Migration::new(N, …)` 两处，回到 27 链。新迁移文件保留不删（与 `0025`/`0026` 同样惯例） |
| **数据** | 新表无旧数据依赖。安装与拒绝同事务（SQLx 事务性 DDL），旧库不会残留半套触发器 |
| **旧迁移** | `0001`–`0027` 零修改，checksum 不漂移 |
| **0025/0026 关系** | 本方案**不触碰**二者任何一行。0025 的 attempt 分支继续以 0017 等值式为前提；0026 的留存语义不受影响 |
| **已产生数据** | 若已上线后回退，新表成为孤儿表。**其内容不得删除**（`DATA_CONVENTIONS §6` 随 Team 保留），只能停止使用 |

### 5.1 前置依赖（非本方案可解决）

§1.7 的 `delivery_attempts` 留存缺口。**若不补**，本方案「旧 attempts 保留」的
承诺在存储层没有防线。建议由总控单列一个前向迁移，不与本方案混做。

---

## 6. 与 0025 / 0026 的逐条兼容性

### 6.1 对 0025 `message_routing_team_integrity.sql`

| 0025 的守卫 | 本方案是否触碰 | 兼容性论证 |
|---|---|---|
| `messages_team_consistency_insert/update` | **不改** | 0025 只校验同 Team，不校验 Role 具体值。本方案不改 `messages` 任何列，两者无交集 |
| `messages_non_role_snapshot_rejected_*` | **不改** | 管的是**发送方** Binding 快照；本表的 `target_binding_*` 是**收件方**快照，语义相反不冲突 |
| `messages_sender_binding_snapshot_*` | **不改** | 管**发送方**快照在 `DRAFT→QUEUED` 边上的产生；本表不写 `messages` |
| `delivery_attempts_target_role_team_insert/update` | **不改** | 校验 `roles` 行属 attempt 的 Team。采用路线 (b) 时 attempt 的 `target_role_id` 仍是原 Message 的 Role，**该守卫行为逐字节不变** |
| `delivery_attempts_target_binding_update` | **不改** | 同上 |
| `message_routing_team_upgrade_guard` | **不改** | 扫描三类既有脏行。本方案不改既有行，升级扫描结果必然不变 |

**关键点**：0025 的 attempt 分支在 MM-R01 §9 被如实登记为「纵深防御，本包不
声称它能被独立触发」。采用路线 (b) 后该性质**继续成立**——若总控改选路线 (a)
并放宽 0017，这段「不声称」会变成实际生效的语义变化，必须重新评估 0025。

### 6.2 对 0026 `core_history_retention.sql`

| 维度 | 0026 对 `messages` | 本方案对新表 | 一致？ |
|---|---|---|---|
| DELETE 守卫 | `messages_core_history_retention_delete` 无条件 ABORT | 同款无条件 ABORT | ✅ |
| `INSERT OR REPLACE` | 0026 **没有**给 messages 加 replace guard（因 `0017` 已有 `messages_identity_replace_guard`） | 新表**需自带** replace guard（新表无此继承保护） | ✅ 且已按更严一侧取 |
| 身份列 UPDATE | `BEFORE UPDATE OF id, team_id` | `BEFORE UPDATE OF id, team_id, message_id` | ✅ 更严（`message_id` 也冻结） |
| 覆盖范围声明 | 0026 §Scope notes 明确**不**声称修了 Attention 与 `references` | 本方案同样**不**声称修 §1.7 的 attempt 缺口 | ✅ |

### 6.3 对 0017

| 0017 守卫 | 处置 |
|---|---|
| `messages_addressing_immutable_update` | **零改动**。路线 (b) 下原目标永不改变 |
| `delivery_attempts_team_consistency_insert/update` | **零改动**。这是 §1.5 定位到的耦合点，方案 (b) 不触碰它 |
| `delivery_attempts_target_binding_insert` | **零改动** |
| `messages_state_machine_update` | **零改动**。`NEEDS_HUMAN → QUEUED` 本就合法（§1.3 实测 ALLOW） |
| `messages_core_history_*`（0026） | 间接加强：新表 FK RESTRICT 后，Message 更难被删 |

---

## 7. 冻结规格冲突清单

| # | 冲突点 | 性质 | 需要的裁决 |
|---|---|---|---|
| C1 | 路线 (a) 需 `DROP TRIGGER` 重建 0017 并放宽等值式 | **架构/不变量变更** | `DOMAIN_MODEL.md` 抬头：「任何改变对象身份、所有权或聚合边界的实现必须新增 ADR」。需 **ADR-0009** |
| C2 | 路线 (b) 下 `delivery_attempts.target_role_id` 退化为「原始寻址」 | 读模型语义澄清 | 若总控认为属语义变更，同样需 ADR；我认为**不需要**，但请总控确认 |
| C3 | `delivery_attempts` 缺留存守卫（§1.7） | 与本方案承重相关 | 建议单独立项，**不由本方案裁决** |
| C4 | 本方案不改任何权限/错误码/状态/协议字段 | — | 无需 ADR。`MESSAGE_REASSIGN_DENIED`、`BINDING_NOT_FOUND`、`MESSAGE_TTL_EXPIRED`、`CONCURRENCY_VERSION_CONFLICT`、`DOMAIN_INVALID_TRANSITION`、`CONCURRENCY_IDEMPOTENCY_CONFLICT` **全部已存在**于 `ERROR_CATALOG` |

**不需要 ADR 的部分**：路线 (b) 全部内容。ADR-0007（Notification 语义，Proposed）
与本方案**无关**，未作为阻断理由。

---

## 8. 需要 Human / Codex 裁决的问题

| # | 问题 | 我的建议 |
|---|---|---|
| Q1 | 采路线 (a)（放宽 0017，需 ADR-0009）还是 (b)（0017 零改动）？ | **(b)**。0017 零改动使 0025 的依赖关系不受影响，风险面小得多 |
| Q2 | (b) 的三项代价（§4.1）可接受吗？ | 可接受，但**必须写进实现说明**，否则日后统计与 Binding 追溯会静默出错 |
| Q3 | `delivery_attempts` 留存缺口是否单独立项？ | 单独立项，不混入本方案 |
| Q4 | override 的 `from_role_id` 是否需要独立于 `messages.to_role_id`？ | 需要——它是「改派链」的证据，也是 §3.1 CHECK 的判据 |

---

## 9. 写集与停止条件

* 本包写集：`docs/coordination/minimax-return/MM-R07-route-override-proposal.md`、
  `MM-R07-RESULT.md`、`docs/reviews/evidence/minimax-return/MM-R07/`。
* **未**修改任何 migration、源码、测试、冻结规格、节点状态或 ADR 接受状态。
* **未**占用 migration 编号。实现包由总控签发时再定。
* **未**提交/推送/部署，未联网，未下载，未新增依赖，未新增费用（$0）。
* 自验收不等于 `DONE`，不启动依赖包。
