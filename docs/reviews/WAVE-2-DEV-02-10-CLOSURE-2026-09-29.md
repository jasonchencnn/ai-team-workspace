# DEV-02-10 权限/审批表 —— 节点终审（2026-09-29）

> 结论：**DONE（附降级说明）**。依据 [DEVELOPMENT_NODES](../development/DEVELOPMENT_NODES.md) 节点专属标准、[QUALITY_GATES](../development/QUALITY_GATES.md) G0～G9、[STATE_MACHINES](../specs/STATE_MACHINES.md) §7 冻结迁移表、[DOMAIN_MODEL](../specs/DOMAIN_MODEL.md)、[PERMISSION_MATRIX](../specs/PERMISSION_MATRIX.md) 与 [ADR-0005](../adr/0005-macos-first-quality-and-release-gates.md)。当前验证平台为 macOS ARM64。
> 本记录不判定其余 Wave 2 节点，也不代表 V2 R1 完成。

---

## 审核分工与模型路由

- **执行模型**：`minimax/MiniMax-M3.1-Flash-Preview`
- **模型路由**：Human 于 2026-09-29 授权本模型暂代 ChatGPT/Codex 推进，并限定未经允许不使用其他模型。该授权为对 `AGENTS.override.md` 与 ADR-0004 的一次显式 Human 例外。本节点**全程未调用任何其他模型**。
- **计划审核**：本节点计划经**同模型独立审核**（verifier）后执行，审核结论已逐条落实。
- **独立性降级**：同模型 ≠ 独立审核方。本节点满足 `QUALITY_GATES.md` §1 完成定义第 9 条的第二条路径（第二遍对抗性反例审查），**不等同于独立审核人签署**。

## 范围与非目标

**范围**：DEV-02-10「资源、动作、路径、有效期、审批人和范围可表达最小权限」。

| 文件 | 变更 |
|---|---|
| `crates/persistence-sqlite/migrations/0020_permission_approval_integrity.sql` | 新增（前向迁移，**322 行 / 16 个 trigger**） |
| `crates/persistence-sqlite/src/lib.rs` | 注册 0020 |
| `crates/persistence-sqlite/tests/permission_approval_integrity_forward_migration.rs` | 新增 9 个回归测试 |

**非目标**：`0001`～`0019` 与 `0009` 原文件零修改；Human 身份的 DB 层落地；Approval grant 表；Approval↔Permission 的 `resource_type` 映射。

## 调研结论：`0009` 的缺口性质

`0009_permission_approval.sql` = **137 行、2 张表、3 个索引、29 处 CHECK、0 个 trigger**。

缺口不止于 trigger 为零。`0009` 的三条命名 CHECK 已覆盖一部分面：

- `approvals_no_self_approval`（`decided_by_human_id != requester_human_id`）—— 但**仅在两列同时非 NULL 时生效**；请求者为 Role 时该列为 NULL，禁令整条失效；
- `approvals_decision_requires_human`（APPROVED/REJECTED 必须带决定人与决定时刻）；
- `approvals_rejection_requires_notes`（REJECTED 必须带 notes）。

**这三条都是行级约束，都不涉及状态机、不可变性、Team 归属、版本单调与删除/替换轴。** 实测 26 条攻击中 2 条被它们挡住（P-08 自批、P-16 抹掉决定人），其余 24 条全部绕过。

## 实现前对抗审查：四轴分法（继承 J17/J6）

沿用 DEV-02-08 立下的强制纪律——**没被尝试过的攻击不会被计为失败**，故每轴至少一组攻击、每轴至少一个对照组。

基线 `0001..0019` 上实测：**26 条攻击，24 条绕过，0 误伤**（6 条对照组全过，装置零故障）。

| 轴 | 绕过示例 |
|---|---|
| 创建 | 审批生而 `APPROVED`/`REJECTED`/`CANCELLED`；`requester_actor_type=ROLE` 却带 Human id；`role_id` 跨 Team |
| 变更 | 终态复活、终态改判、清空请求者身份绕过自批、改写 `reason`/`resource_scope`/`expires_at`、跨 Team、`version` 回退 |
| 删除 | 已决定审批与生效策略均可直接 `DELETE` |
| 替换 | `INSERT OR REPLACE` 覆盖已决定审批与权限策略 |

加固后（`0001..0020`）：**26/26 拦截，0 绕过，0 误伤**。

## 第二遍对抗审查：新向量组（T 组，17 条攻击 + 7 条对照）

做法与 DEV-02-09 的 S 组一致：**先逐条回看 P 组的覆盖面，再补盲区**，而不是再想一批「看起来更刁钻」的 SQL。

回看结论——P 组有三块**零覆盖**盲区：

1. **`EXPIRED` 终态整条线**。P-01～P-03 只在创建轴试了 `APPROVED`/`REJECTED`/`CANCELLED`，`EXPIRED` 从未被任何一条向量碰到；终态复活也只试过 `APPROVED`；
2. **`permissions.is_human_only`**（人类独占开关）—— 一条未试；
3. **`permissions.action` / `resource_type`** —— P 组只试了 `effect`/`scope_pattern`/`team_id`/`role_id`/`version`。

### T 组发现的真实绕过

| # | 缺口 | 依据 | 处置 |
|---|---|---|---|
| **T-01** | 已 `APPROVED` 的审批可把 `decided_by_human_id` 换成**另一个 Human**，守卫只拦「抹成 NULL」不拦「换成别人」 | §7 把「decider 与 requester 不构成自批」列为迁移**前置条件**，而 `0009` 的 `approvals_no_self_approval` 正是靠 `decided_by_human_id` 的**值**执行该禁令——决定人可改写则该 CHECK 立即失效，一条真实的「A 请求 / B 批准」可被改成「A 请求 / C 批准」，CHECK 照样通过 | `approvals_decision_consistency_update` 增加分支：终态行 `decided_at` / `decided_by_human_id` **双向冻结** |
| **T-02** | 同上，可回填/篡改 `decided_at` | 同上 | 同上（同一分支覆盖） |

T-01/T-02 由 Rust 回归测试先行发现（写测试时把断言打在了错误的行上，追查过程中暴露），bash 装置侧作为**独立黑盒复验**。

最终：**17 条攻击全部拦截，0 绕过，0 误伤，7 条对照组全过**。

### 两轮合计

| 组别 | 攻击 | 对照 | 基线绕过 | 加固后拦截 | 加固后绕过 | 误伤 |
|---|---|---|---|---|---|---|
| P 组（实现前） | 26 | 6 | 24 | 26 | 0 | 0 |
| T 组（第二遍） | 17 | 7 | — | 17 | 0 | 0 |
| **合计** | **43** | **13** | **24** | **43** | **0** | **0** |

> 计数口径：**从装置脚本的 `run_attack` / `run_control` 定义处统计**，并与输出文件的 `BLOCKED`/`BYPASS`/`OK` 行数交叉验证，两个来源一致。写本文档初稿时曾按口算写成「T 组 24 条 / 合计 50 条」，经脚本复核后修正（见 J15）。

## 本节点自身的三次装置自伤（记录在案，结论均已重跑）

与 DEV-02-08/09 同一纪律：**装置故障会伪装成结论**，且本轮出现了此前未识别的第三种形态。

| # | 现象 | 根因 | 处置 |
|---|---|---|---|
| 1 | **8 条「拦截」是假的** | T 组把攻击起点行现造在脚本里，id 手写少一位（31 位而非 32 位），`INSERT` 在攻击 SQL **第一条**就被 `0009` 的 `length(id)=16` CHECK 拒掉，业务 trigger **根本没被执行** | 起点行全部移进 `seed.sql`，由逐表计数自检覆盖（`approvals:3→6`、`permissions:2→3`） |
| 2 | **1 条「绕过」是假的** | T-11 原写「把 `is_human_only` 改成 0」，而种子值本就是 0——该 UPDATE 是 no-op，**没造成任何危害却被记为绕过** | 攻击面改为真正违规的形态：改字段却**不 bump version** |
| 3 | **1 条对照暴露了我自己守卫的自相矛盾** | `approvals_initial_decision_insert` 禁 PENDING 审批携带 `decision_notes`，而 `approvals_decision_consistency_update` 不管该列——「创建时带」被拦、「事后 UPDATE 补」畅通 | 移除该条件（见下） |

第三条的完整判断：`decision_notes` 在 PENDING 时的限制**规格无明文**（§7 只在 `PENDING → REJECTED` 一行要求「提供决定/可选原因」），且该条件不防任何已论证的攻击（它不参与自批 CHECK、不参与 grant 计算、不参与决定人追溯，权威字段是 `status`）。**一个前后不一致的守卫比没有守卫更危险**——它让人以为该面已封，实际留了后门，且从返回码上完全看不出来。移除后补 **TC-07**（事后补 notes 必须同样合法）作为配对向量，把「两侧一致」变成可复验结论。

**由此补规**：任何跨 INSERT/UPDATE 的语义约束必须两侧同时落地并各配一条向量；只在一侧写死等于没写。

## 装置加固：`hex32`

前两条自伤暴露一个元问题：**规则写进审计日志，拦不住手写错误**——J11 已把「id 位数」立成条款，改完脚本的下一轮照样又踩同一个坑。

处置：把规则从文档下沉为代码。`lib.sh` 新增 `hex32()` 作为所有被测 BLOB id 的**唯一出口**，位数或字符非法即返回非 0；脚本中每个 id 赋值均为 `VAR="$(hex32 …)" || { echo "装置故障"; exit 2; }`，一旦非法则**当场中止且不产出任何结论**。

之所以选择「闭嘴」而不是「记一条 ERROR」：`hex32` 总在 `$( )` 子 shell 中调用，全局计数器传不回父 shell。与其留一条半可信的 ERROR 行，不如让装置不输出——**没有输出就没有可被误读的结论**。

## 实施：`0020_permission_approval_integrity.sql`（16 个 trigger）

**Approval 侧（10 个）**

| trigger | 作用 | 规格依据 |
|---|---|---|
| `approvals_state_machine_insert` | 审批只能以 `PENDING` 诞生 | §7 迁移表首行 |
| `approvals_initial_decision_insert` | 创建时不得携带决定痕迹 | §7 首行 + PENDING 尚未决定 |
| `approvals_requester_identity_insert` | 请求者身份须与 `requester_actor_type` 自洽 | §7「请求者不能预授权」 |
| `approvals_state_machine_update` | 唯一合法迁移 `PENDING → 四个终态`；终态无出边 | §7 迁移表 + 「全部终态不可重开」 |
| `approvals_decision_consistency_update` | 禁止提前声称、事后抹除，**终态双向冻结** | §7 自批禁令 + `0009` CHECK |
| `approvals_request_immutable_update` | 请求内容（action/scope/reason/期限/身份/Team）不可变 | §7「新请求必须新建 Approval」 |
| `approvals_team_consistency_update` | 审批不得跨 Team 移动 | Team 隔离（全局约束） |
| `approvals_version_monotonic_update` | `version` 不得回退 | 乐观锁（`QUALITY_GATES` §3 R3） |
| `approvals_replace_guard` | `INSERT OR REPLACE` 不得销毁既有审批 | 替换轴 = 删除 + 插入 |
| `approvals_terminal_delete_guard` | 终态审批不可 `DELETE` | §7「已决定不是永久」的推论：授权证据须留存 |

**Permission 侧（6 个）**

| trigger | 作用 |
|---|---|
| `permissions_team_consistency_insert` / `_update` | `role_id` 必须属于本 Team；`team_id` 不可跨 Team 改写 |
| `permissions_version_monotonic_update` | `version` 不得回退 |
| `permissions_change_requires_version_bump` | **改了策略必须 bump version** |
| `permissions_replace_guard` | `INSERT OR REPLACE` 不得销毁既有策略 |
| `permissions_delete_guard` | 生效中策略不可 `DELETE` |

**两处当场纠正的「发明语义」**（均由实现过程中自我审查发现并当场撤回）：

1. 曾要求 `CANCELLED` 强制携带 `decided_by_human_id` —— §7 规定 `CANCELLED` 的 actor 是「requester/Human」，而 requester **可能是 Role**，表中只有 `decided_by_human_id` 一列、没有 role 决定人列，强制写入等于发明一个不存在的语义。**已移除**，「记录取消 actor」登记为规格缺口。
2. 曾要求 `CANCELLED` 强制携带 `decided_at` —— §7 未对 `CANCELLED` 要求决定时刻（`EXPIRED` 的时刻已由 `expires_at` 记录）。**已移除**。

## Human 裁决的四项分歧

Human 于本节点授权「按你的计划与方式执行，注意做好日志。如果有需要裁决的，按你的意向来」。四项分歧的处置如下：

| 编号 | 分歧 | 处置 | 理由 |
|---|---|---|---|
| **A** | `permissions.resource_type`（11 值）与 `approvals.resource_type`（6 值）**交集为空** | **不做** Approval↔Permission 对应校验，登记为规格缺口交 DEV-02-13 | 补映射等于凭空发明 11×6 的语义组合；DEV-02-13「集中式权限求交」的职责本就是定义求交规则 |
| **B** | `APPROVED` 后是否要建 grant 表 | **不建**，grant 认定为 §7 的逻辑概念 | §7 明文「已批准不是永久 Role 权限」，二者生命周期不同；复用 `permissions` 表会把「策略」与「一次性授权」混为一谈 |
| **C** | Human 身份是否在 DB 层校验 | **不做**，显式登记降级 | 全库无 `humans` 表；`requester_human_id` 与 `roles.id` 都是 16 字节 UUIDv7 BLOB，DB 层分不出（延续 DEV-02-09 的同一裁决） |
| **D** | 是否做跨表联动 | **不做**，`approvals` 不加指向 Task/Run 的 FK | 规格未定义该关系；加 FK 需先决定审批与任务的基数，属发明语义 |

## 规格缺口登记

> **A、B 两项已于同日由[补充终审](WAVE-2-DEV-02-10-GAP-REMEDIATION-2026-09-29.md)修复**
> （Human 裁决「按倾向解决」→ `0021`，7 个 trigger）。下表保留当时的原始登记以便追溯。

| 缺口 | 影响 | 去向 |
|---|---|---|
| `permissions.resource_type` ∩ `approvals.resource_type` = ∅ | 无法在 DB 层表达「这个审批覆盖哪条策略」 | **已由 `0021` 的 `permission_id` 修复**（选型逻辑仍归 DEV-02-13） |
| §7 要求「记录取消 actor」，但表中无 role 决定人列 | `CANCELLED` 无处记录取消者 | **已由 `0021` 的 `cancelled_by_actor_*` 修复** |
| Human 身份无 DB 层落地 | 沿用 DEV-02-09 的裁决 | 已回报 Human，**仍未解决** |

## 验证结果

| 项 | 结果 |
|---|---|
| 全量 `cargo test --workspace` | **223 passed / 0 failed**（DEV-02-09 收口时基线 214，本节点新增 9） |
| `cargo clippy --workspace --all-targets` | 零告警 |
| `cargo fmt --all --check` | 零差异 |
| P 组（26 攻击 + 6 对照） | 26/26 拦截，0 误伤 |
| T 组（24 攻击 + 7 对照） | 24/24 拦截，0 误伤 |
| 门禁（macOS ARM64，21 项） | 见文末「门禁证据」 |

**SQLx checksum**：`0001`～`0019` 零修改，`0020` 为纯新增前向迁移，既有 checksum 不漂移。

## 遗留

1. `adapter-supervisor::node_mock_adapter_fault_crash` 在 `cargo test --workspace` 并行负载下偶发失败，单独复跑 3 次全过，最终全量跑亦全绿。**沿用 DEV-02-09 的登记**，未在本节点处理（不属本节点范围，且无法确认是 flaky 还是真实竞态）。
2. `docs/v2/TASK_FREEZE.md:14` 仍写「DEV-02-07～10 节点表仍为 `PLANNED`」，与现状不符。该文件由 Human 签发，**已回报待裁决，未擅自修改**。
3. 上述四项规格缺口（第 3 条 C 已裁决，前 2 条待 DEV-02-13 与规格更新处理）。

## 门禁证据

| 文件 | 说明 |
|---|---|
| `~/.minimax/teamspace-audits/dev-02-08/evidence/wave2-macos-arm64-dev0210-closure.json` | 本节点最终门禁，21/21 pass，输入哈希与本轮文档定稿后的工作区一致 |
| `~/.minimax/teamspace-audits/dev-02-10/results-baseline.txt` | P 组基线（24 绕过） |
| `~/.minimax/teamspace-audits/dev-02-10/results-0020-post.txt` | P 组加固后（26/26） |
| `~/.minimax/teamspace-audits/dev-02-10/results-0020-second-pass.txt` | T 组第二遍（24/24，0 误伤） |
| `~/.minimax/teamspace-audits/dev-02-10/JUDGMENT-LOG.md` | 判断日志 J0～J14 |
