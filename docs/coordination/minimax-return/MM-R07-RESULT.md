# MM-R07-RESULT｜追加式 route override 方案（纯方案包）

- 关联节点：DEV-02-08（后续 DEV-05-07）。**节点状态未改，仍是 BLOCKED。**
- 结论：**`SELF_CHECK_PASS`**（方案可审阅；未实现任何代码）
- 自验收不等于正式节点 `DONE`；正式判定与实现包由 Codex 总控签发。

---

## 1. 实际模型与执行环境

| 项目 | 值 |
|---|---|
| 执行端 | Minimaxcode Worker 子会话 `mvs_69942dce49904ac199795119ab736a6a` |
| 模型 | `MiniMax-M3.1-Flash-Preview`（会话运行时声明值，未猜测 M3；实际权重版本号执行端不可见） |
| Harness | MiniMax Code Desktop，本地分支会话 |
| 仓库 | `/Users/chenzhiqing/Documents/MyVault/协作工作` |
| HEAD | `1a14b8795e76fc373bef326eb5df96f9ac173f71`，分支 `codex/wave-2-ci-validation` |
| 平台 | darwin 27.0.0 arm64 |
| 工具链 | `cargo 1.98.0` / `rustc 1.98.0` / `python3 3.14.6` / `sqlite3 3.50.6`；探针走 Python `sqlite3`（绑定 SQLite 3.53.4） |
| 新增费用 | $0。未联网、未下载、未新增依赖、未调用外部服务、未访问生产凭据/数据、未提交/推送/部署 |
| 共享路径写入 | 仅本包写集 3 项 |

---

## 2. 输入与输出 SHA

任务包声明的审计起点输入 SHA 为
`8845a053b778b67aeac7a2138ebeaa16432781572cdb2232fa085c30816de37f`（299 个范围文件）。
**本包启动时重新固定了当前输入**，登记如下：

| 角色 | 值 |
|---|---|
| HEAD | `1a14b8795e76fc373bef326eb5df96f9ac173f71` |
| migration 目录合并 SHA-256（27 份，按文件名排序后逐份哈希再哈希） | `a24c603f4a133a1243742d0f90e02746b75e3e373e1d60caba7988fac7967030` |

**输出（写集 3 文件）**：

| 文件 | SHA-256 |
|---|---|
| `docs/coordination/minimax-return/MM-R07-route-override-proposal.md` | 见 §11（本文件写入后统一回填） |
| `docs/reviews/evidence/minimax-return/MM-R07/repro_route_override.py` | `28608454433ceec7824ceca2b1d86524cc1e199983c8b95748acab4031f6f0f8` |
| `docs/reviews/evidence/minimax-return/MM-R07/01-repro-transcript.log` | `13d9b0d50087cb2e7e4e89b58ca6d31197376e1123092979d1cc1cfb9ae732b3` |
| `docs/reviews/evidence/minimax-return/MM-R07/02-determinism-check.log` | `1e87cd9338207f6ad3c2ca050161ad0ebd6f7969387d7ef9e9522f41215037ad` |

**未漂移证明**：`0025` 当前 SHA `e76eabe9…`，与 MM-R01 §2 登记值逐字符相同；
`0026`、`0027` 亦已记录。本包**未修改任何 migration**。

未提交故无 Git tree SHA；上表为本包可复核的固定输入。

---

## 3. 实际读了哪些文件与规格条款

### 3.1 必读清单（按任务包顺序，全部读完）

| # | 文件 | 读了什么 |
|---|---|---|
| 1 | `AGENTS.override.md` | 全文；确认「冻结技术栈/领域不变量/协议边界/质量门禁」，变更须 ADR + Human 确认 |
| 2 | `docs/development/README.md` | 全文；事实优先级链、§6 变更控制 |
| 3 | `docs/development/TECH_STACK_AND_STANDARDS.md` | 全文；**§4.1 十项不变量**、§4.2 版本/幂等、§4.4「已发布 migration 不修改」、§5 并发、§7 安全、§13 明确禁止 |
| 4 | `docs/development/DEVELOPMENT_NODES.md` | 全文（表格部分有截断，已读 §1～§2 全部行与 DEV-02-08/11/12/13/14 条目）；确认 DEV-02-12 记载的「`/` = 互斥」标点约定 |
| 5 | `docs/development/QUALITY_GATES.md` | 全文；G0～G9、Gate C、§5 测试金字塔、§11 证据模板、§12 缺陷分级 |
| 6 | `docs/specs/README.md` | 全文 |
| 7 | `docs/specs/STATE_MACHINES.md` | **全文**。重点 **§5 全表**，尤其 `:148` 改派行、`:150` 历史不可改写、`:138` Router 解析 ACTIVE Binding |
| 8 | `docs/specs/PERMISSION_MATRIX.md` | **全文**。重点 **§3 全表**（`:80` 发送/回复/确认、`:81` retry、**`:82` `Message reassign` = Owner H / Leader S / 其余 —**）、§2 八层求交、§5 Human-only 八项 |
| 9 | `docs/specs/DOMAIN_MODEL.md` | **全文**。重点 §3 聚合边界、**§4 命令写入所有权**、§5 删除语义、**§6 跨边界一致性** |
| 10 | `docs/specs/DATA_CONVENTIONS.md` | **全文**。重点 **§3 版本与并发**、**§4 幂等规则**、**§5/§6 留存**、§8 Event 完整性 |
| 11 | `docs/specs/ERROR_CATALOG.md` | **全文**。确认 `MESSAGE_REASSIGN_DENIED`（`:133`）、`BINDING_NOT_FOUND`（`:99`）、`MESSAGE_TTL_EXPIRED`（`:130`）、`CONCURRENCY_IDEMPOTENCY_CONFLICT`（`:62`）**均已存在** |
| 12 | `docs/specs/adapter/v0/PROTOCOL.md` | **全文**。确认 Message 路由**不在** Adapter V0 职责内（§1），故本方案不涉及协议字段 |
| 13 | `docs/development/ADVERSARIAL_REVIEW_PLAYBOOK.md` | **全文**。七条铁律逐条对照落实 |
| 14 | `docs/adr/0005` | 全文（Mac 优先门禁；与本包无直接关系，确认门禁归属总控） |
| 15 | `docs/adr/0006` | 全文（Adapter 脱组后代；与本包无关） |
| 16 | `docs/adr/0008` | 全文（Minimaxcode 路由；确认「只报告 SELF_CHECK，不自判 DONE」） |
| 17 | `docs/reviews/DEEP-AUDIT-2026-10-08.md` | 全文；重点 **§4 `Message route override` 行（`:64`）**、§3 DA-01/02 |
| 18 | `docs/coordination/task-packets/…/MM-R07.md` | 全文 |
| 19 | `docs/coordination/minimax-return/MM-R01-RESULT.md` | 全文（0025 背景、§9「attempt 分支不声称可独立触发」） |
| 20 | `MM-R02-RESULT.md`（未通读，取 §7 与迁移本体） | 0026 留存语义 |
| 21 | `MM-R05-RESULT.md` | 前 80 行（确认本轮包形态与证据约定） |

**任务包未列为必读但我主动读了**（判断是否相关）：

| 文件 | 为什么读 |
|---|---|
| `crates/persistence-sqlite/migrations/0002_harness_adapter_binding.sql` | Binding 表结构、`bindings_single_active_per_role`、effective 区间约束 |
| `…/0006_event_outbox.sql` | Event 表；`subject_type` 枚举**已含 `MESSAGE`** |
| `…/0007_thread_message_delivery.sql` | `messages` / `delivery_attempts` 建表原文 |
| `…/0013`、`0014` | messages 的 DELETE 守卫是**条件式**（有引用行才生效） |
| `…/0017_message_delivery_integrity.sql` | **全文**。§1.5 定位的耦合点在此 |
| `…/0018_message_delivery_insert_state_guard.sql` | 创建轴守卫；`NEEDS_HUMAN → QUEUED` 的 UPDATE 侧无阻碍 |
| `…/0022_command_idempotency.sql` | 幂等表与 `replace_guard` 模式来源 |
| `…/0025`、`0026`、`0027` | **全文** |
| `crates/persistence-sqlite/tests/message_routing_team_integrity.rs` | 种子与断言约定 |

### 3.2 逐条核对任务包点名的规格

| 任务包要求 | 我实际核对的行 | 结论 |
|---|---|---|
| §5 迁移表存在改派行 | `STATE_MACHINES.md:148` | ✅ 逐字符合总控转述：Actor「绑定修复/授权改派后 Human 操作」、前置「新目标/Binding 有效，权限通过，TTL 有效」、副作用「新 attempt，记录 route override」、Event `MESSAGE_REASSIGNED`/`MESSAGE_REQUEUED`、拒绝码 `AUTH_HUMAN_REQUIRED` |
| 0017 强制 `to_role_id = target_role_id` | `0017:259-267`、`:276-284` | ✅ 两处（INSERT/UPDATE 双臂） |
| 0025 为 attempt 加同 Team 守卫 | `0025:197-217` | ✅ |
| DATA_CONVENTIONS §5 正文与寻址冻结 | `:56` | ✅ |
| 无专用载体 | 28 张表全枚举 | ✅ |

---

## 4. 方案要点摘要

完整方案见 `MM-R07-route-override-proposal.md`。骨架：

1. **新增追加式 `message_route_overrides` 表**（`id / team_id / message_id /
   override_number / from_role_id / to_role_id / target_binding_id /
   target_binding_generation / actor_type / actor_role_id / actor_human_id /
   idempotency_key / request_id / event_id / created_at`）。
2. **原 `Message` 不改、旧 attempt 不改、0017/0025/0026 一行不动。**
3. **attempt 目标解析规则**：取该 Message 上 `override_number` 最大的 override；
   无则回落 `messages.to_role_id`。纯读，无需可变状态列，故无竞态。
4. **约束**：同 Team（触发器直陈 `roles` 表）、actor 授权（结构由 CHECK 固化，
   判定归应用层八层求交）、Binding 有效性（同 `0025:227` 判据）、版本乐观锁
   （复用 `messages.version`）、TTL（过期用既有 `MESSAGE_TTL_EXPIRED`）、
   幂等（`UNIQUE(team_id, idempotency_key)` + `replace_guard`）、
   Event/Outbox（既有 `MESSAGE_REASSIGNED`，`subject_type='MESSAGE'` 已在枚举内）、
   历史 FK（三处 `ON DELETE RESTRICT`）、留存（无条件 DELETE 守卫 + replace guard）。
5. **两条路线**：(a) 放宽 0017（需 ADR）；(b) 0017 零改动。
   **推荐 (b)**，但如实列出 (b) 的三项实测代价。

---

## 5. 我如何独立验证了审计的既有结论

### 5.1 方法

审计 §4 已给出结论并**主动否定了自己子反例的计数**。我的做法不是复述，而是
**补上它要求的那一个场景**，并给出可重放的判定路径。

脚本 `repro_route_override.py`：从**真实 27 个 migration 文件**建库
（非手抄 schema），`PRAGMA foreign_keys=ON`，**不预设任何期望值**，只记录观察。

### 5.2 装置自检（含一次真实故障，如实登记）

| `ADVERSARIAL_REVIEW_PLAYBOOK` 规矩 | 落实 |
|---|---|
| 规则 3（构造路径须可自检） | 所有 BLOB id 只经 `uid()` 一个出口，自动校验长度 32、`substr(hex,13,1)='7'`、`substr(hex,17,1)∈89AB` |
| 规则 2/3（故障不得伪装成真结果） | 种子逐表行数断言 + `pragma foreign_key_check` + **「改派目标确有 ACTIVE 且生效中的 Binding」** + 「两 Role 同 Team」+ 「Message 确为 NEEDS_HUMAN 且 TTL 有效」，全部**先断言再探** |
| 规则 6（计数须跑命令，不手算） | 28 张表由 `sqlite_master` 实查并打印，不写「若干张」 |
| 确定性 | 两次独立运行 205 行**逐字节相同**（`02-determinism-check.log`，仅随机临时目录名不同） |

**一次真实装置故障（依规则如实登记）**：初版脚本里我手写的 UUID hex 是 **31 位**，
第一条 INSERT 就以 `unrecognized token` 失败——**输出形态与真拦截完全同形**。
这正是手册第 0 节记录的 DEV-02-10「8 条假拦截」事故。修法是引入 `uid()` 单一
校验出口，而不是手改那一个数字。

### 5.3 合法改派场景是否真的被现有实现拒绝 —— 是，可重放

判定路径：

```bash
cd /Users/chenzhiqing/Documents/MyVault/协作工作
python3 docs/reviews/evidence/minimax-return/MM-R07/repro_route_override.py --repo .
# exit 0；完整输出见 01-repro-transcript.log
```

**先立合法对照**（否则缺口论证塌掉）：`STATE_MACHINES.md:148` 的
`MESSAGE_REASSIGNED`/`MESSAGE_REQUEUED` 按该规格自身标点约定（`/` = 互斥，
全规格仅两行用 `/`）是同一条边的两种形状。实测 `MESSAGE_REQUEUED` 形状**成立**：
`NEEDS_HUMAN → QUEUED` **ALLOW**，为同一冻结目标建新 attempt 也 **ALLOW**。

**再测真缺口**，五种语句形状逐一体测：

| # | 形状 | 结果 | 拒绝者 |
|---|---|---|---|
| S1 | 新 attempt 指向改派目标（Message 一字不动） | **REFUSE** | `Delivery attempt Team consistency violation` |
| S2 | 就地改向冻结 Message | REFUSE | `Message addressing is immutable`（**应当拒**） |
| S3 | 删冻结行再重建 | REFUSE | `Message history is retained with the Team`（**应当拒**） |
| N1 | 改派到跨 Team Role | REFUSE | `Delivery attempt Team consistency violation` |

S2/S3 被拒是**正确的**（`DATA_CONVENTIONS §5` 正文与寻址随 QUEUED 冻结）。
所以缺口不是「守卫过严」，而是 **S1 这条规格明写的边无法表达**。

**精确定位到唯一失败的合取项**——没有靠「哪条 INSERT 红了」反推，而是把
`0017:259` 的 `WHEN` 子句逐条求值：

```
PASS  message exists      = 1
PASS  team matches        = 1
FAIL  TARGET matches      = 0    <- message.to_role_id = attempt.target_role_id
PASS  binding snapshot ok = 1
```

**恰好一条失败**，可唯一归因。

**结论**：审计 §4 的判断成立，且我把它从「子反例不足以计数」推进到
「构造出每个前置条件都被断言的完整合法改派，并证明现有实现拒绝它」。
全流程零业务副作用：S1/S2/S3 失败后 `message unchanged: True`、
`delivery_attempts rows: 0`。

### 5.4 两个附带发现（均不在任务包 RED 输入内，如实上报）

| # | 发现 | 证据 | 对方案的影响 |
|---|---|---|---|
| F-A | `delivery_attempts` **没有留存守卫**。`DATA_CONVENTIONS §5/§6` 明列 DeliveryAttempt 随 Team 保留；`0026` 覆盖 runs/reviews/artifacts/approvals/messages，**不含** delivery_attempts | 实测 `DELETE FROM delivery_attempts` **ALLOW**（1→0）；同刻 `DELETE FROM messages` **REFUSE**（证明守卫确实在，不是装置没加载） | **承重**。改派方案的核心承诺就是「旧 attempts 保留」；若 attempt 可删，该承诺在存储层无防线 |
| F-B | 路线 (b) 并非免费：B1（attempt 指原目标 + 挂改派目标的 Binding）**REFUSE**，B2（不带 Binding）**ALLOW** | `0017:294` 要求 Binding 快照指名 `attempt.target_role_id`，而 (b) 把它钉在原目标 | 改派目标的 Binding 快照只能存新表，attempt 带不上。已在方案 §4.1 列为 (b) 的三项代价之一 |

两者都**不改变**审计的既有结论，也**未**被我自行修补。

---

## 6. 命令、退出码与证据路径

| 命令 | Exit | 证据 |
|---|---:|---|
| `python3 …/repro_route_override.py --repo .` | **0** | `01-repro-transcript.log`（205 行） |
| 同上 ×2 + `diff` | **0** | `02-determinism-check.log`（`diff run1 vs run2: IDENTICAL`） |

**没有运行实现门禁**——本包无代码变更，跑 `cargo test` 冒充完成会违反
`MM-R07.md` 验收命令「不运行实现门禁冒充完成」。

---

## 7. 未运行项及理由

| 未运行 | 理由 |
|---|---|
| `cargo test` / `build` / `clippy` / `fmt` / `rustdoc` | 本包**零代码变更**，且任务包明令不得以实现门禁冒充方案完成 |
| 任何 TS / contract / 生成漂移检查 | 无 Rust→Schema 变更 |
| 完整 Mac 21 项门禁 / 干净安装 / 备份恢复 / 性能 / A11y | 属总控最终门禁范围（ADR-0005 §2） |
| 在线检查、任何联网/下载 | 强制禁止 |
| 迁移编号 | **未预留、未占位**，由总控签发实现包时指定 |

---

## 8. 写集与未触碰范围声明

**新增（本包全部产出）**：

* `docs/coordination/minimax-return/MM-R07-route-override-proposal.md`
* `docs/coordination/minimax-return/MM-R07-RESULT.md`（本文件）
* `docs/reviews/evidence/minimax-return/MM-R07/`（3 个文件）

**未修改**：`sources/`、`0001`–`0027` 任何迁移、任何源码、任何测试、
任何冻结规格、任何正式节点状态、任何 ADR 接受状态、根 manifest/lockfile、
生成 TS。`0025` 当前 SHA 与 MM-R01 登记值逐字符相同，可证无漂移。

**未**提交/推送/部署、未联网、未下载、未新增依赖、未新增费用（$0）。
**未**执行 `git reset` / `checkout` / `clean` / `stash`，继承成果全部保留
（`git status` 仍显示 121 项继承改动）。
**未**自判 `DONE`，**未**启动任何依赖包。

---

## 9. 剩余风险与回退

| 风险 | 说明 / 缓解 |
|---|---|
| **路线 (b) 的三项语义代价** | attempt 的 `target_role_id` 退化为「原始寻址」；改派目标的 Binding 快照不在 attempt 上；按该列统计须先解析 override。**必须写进实现说明**，否则日后静默出错 |
| **F-A 未修则方案承诺打折** | 「旧 attempts 保留」无存储层防线。建议单独立项，**不与本方案混做**，避免互相掩盖 |
| **若总控改选路线 (a)** | 0017 的等值式放宽会使 MM-R01 §9「attempt 分支不声称可独立触发」那段诚实声明变成实际生效的语义变化，须重新评估 0025，并先有 ADR |
| **新表与 `notifications`/`attention_items` 的关系未定** | `NEEDS_HUMAN` 会创建 AttentionItem。改派后该 item 如何关闭属 ADR-0007（Proposed）范围，**本方案刻意不碰**，未作为阻断 |
| **回退** | 本包无代码无迁移，回退 = 删除本包 3 个文件。实现阶段回退方式见方案 §5 |

---

## 10. SELF_CHECK_PASS

方案可审阅：

* 单写者前向 schema 已给出（不改原 Message / 不改旧 attempt / 不改 0017/0025/0026）；
* 全部约束逐条对到冻结规格行号；
* 合法对照**已实测成立**，且真缺口**已实测被拒**并定位到唯一合取项；
* 负向 10 项、竞争、失败零副作用均已写明；
* 回退与冻规格冲突清单已给出（4 条，C1/C3 需裁决，C4 明确无需 ADR）；
* **未新增**任何状态、权限、错误码或协议字段——6 个用到的错误码全部已存在于
  `ERROR_CATALOG`，`events.subject_type` 已含 `MESSAGE`，Adapter V0 不涉及本域；
* 装置自检、确定性、装置故障均已如实登记。

**边界**：这是方案，不是实现。「方案可审阅」不等于「实现正确」，
实现包仍需 Codex 逐条复核与签发。

---

## 11. 写集 SHA 回填

方案文件在 §2 表格中标注为「见 §11」，实算值如下（可用
`shasum -a 256 docs/coordination/minimax-return/MM-R07-*.md
docs/reviews/evidence/minimax-return/MM-R07/*` 复核）：

| 文件 | SHA-256 |
|---|---|
| `MM-R07-route-override-proposal.md` | `55e0fd2a39d8b91c436fc55aa1be18a6434d93750cb20df974ae4c7360854d8a` |
| `MM-R07-RESULT.md` | 本文件自身，SHA 随写入变化，故不自引用；以上表与 §2 的 3 个证据文件 SHA 构成完整写集指纹 |
