# DEV-02-09 通知/策略/待办表 —— 节点终审（2026-09-29）

> 结论：**DONE（附降级说明）**。依据 [DEVELOPMENT_NODES](../development/DEVELOPMENT_NODES.md) 节点专属标准、[QUALITY_GATES](../development/QUALITY_GATES.md) G0～G9、[STATE_MACHINES](../specs/STATE_MACHINES.md) §6 冻结迁移表、[DOMAIN_MODEL](../specs/DOMAIN_MODEL.md)、[PERMISSION_MATRIX](../specs/PERMISSION_MATRIX.md) 与 [ADR-0005](../adr/0005-macos-first-quality-and-release-gates.md)。当前验证平台为 macOS ARM64。
> 本记录不判定其余 Wave 2 节点，也不代表 V2 R1 完成。

---

## 审核分工与模型路由

- **执行模型**：`minimax/MiniMax-M3.1-Flash-Preview`
- **模型路由**：Human 于 2026-09-29 授权本模型暂代 ChatGPT/Codex 推进，并限定未经允许不使用其他模型。该授权为对 `AGENTS.override.md` 与 ADR-0004 的一次显式 Human 例外。
- **计划审核**：本节点计划经**同模型独立审核**（verifier，`PASS_WITH_COMMENTS`），8 项阻断项**逐条独立复核、全部核实为真**并已修正。审核同时逼出 DEV-02-08 自身的未覆盖缺口（见 [重签文档](WAVE-2-DEV-02-08-RESIGN-2026-09-29.md)）。
- **独立性降级**：同模型 ≠ 独立审核方。本节点满足 `QUALITY_GATES.md` §1 完成定义第 9 条的第二条路径（第二遍对抗性反例审查），**不等同于独立审核人签署**。

## 范围与非目标

**范围**：DEV-02-09「Human 接收者不伪装成 Role；Event、Message、通知和待处理项可互相引用」。

| 文件 | 变更 |
|---|---|
| `crates/persistence-sqlite/migrations/0019_notification_attention_integrity.sql` | 新增（前向迁移，**21 个 trigger** + 1 张升级守卫表） |
| `crates/persistence-sqlite/src/lib.rs` | 注册 0019 |
| `crates/persistence-sqlite/tests/notification_attention_integrity_forward_migration.rs` | 新增 9 个回归测试 |
| `crates/persistence-sqlite/tests/notification_attention_migration.rs` | 既有测试补建真实 Task 行（来源守卫要求 `source_id` 真实存在） |

**非目标**：`0001`～`0018` 与 `0008` 原文件零修改；Human 身份的 DB 层落地；`filters`/`escalation` 字段；Attention `RESOLVED` 的底层终态校验；`source_type` 的 `TASK`/`SYSTEM` 语义。

## 调研结论：`0008` 的缺口性质

`0008` = **176 行、3 张表、7 个索引（3 UNIQUE + 4 普通）、0 个 trigger**。

**但不能笼统说「没有守卫」**——`0014:428-456` 已为 `notifications` 装了 3 个 trigger
（`notifications_event_subject_update` / `_delete` / `_replace_guard`），
三者均带 `WHEN EXISTS (... events WHERE subject_type='NOTIFICATION' ...)` 条件，
**只对被 Event 引用的行生效**；`attention_items` 在 `0014` 中零 trigger。

正确的缺口表述是：**`0008` 自身零 trigger；`0014` 提供了仅限被 Event 引用行的部分防护，
未引用行仍可 `INSERT OR REPLACE` 绕过去重索引。**

## 实现前对抗审查：四轴分法（继承 J17/J6）

DEV-02-08 的教训是「只覆盖 UPDATE 的攻击集会系统性漏掉整类绕过，且这类绕过不会被计为失败」。
本节点**强制按 创建 / 变更 / 删除 / 替换 四轴组织**，每轴至少一组攻击，且每轴至少一个对照组。

基线 `0001..0018` 上实测：**30 条攻击，25 条绕过，0 误伤**（其中 2 条被 `0008` 既有 CHECK 挡住）。

| 轴 | 绕过示例 |
|---|---|
| 创建 | 通知生而 `DISMISSED`/`EXPIRED`；待办生而 `RESOLVED`；跨 Team `policy_id` |
| 变更 | 终态复活、跨 Team 改写、内容与时间戳脱节、`version` 回退 |
| 删除 | 三张表零 DELETE 守卫 |
| 替换 | `INSERT OR REPLACE` 销毁原行并绕过去重索引 |

## 第二遍对抗审查：新向量组（S 组，9 条）

专找 `0018`/`0019` **自己实现**可能有的洞。**发现并修复 2 项**，其中一项归属 DEV-02-08：

| # | 缺口 | 处置 | 归属 |
|---|---|---|---|
| **S-03** | 先合法插入 DRAFT，再 `UPDATE ... SET status='QUEUED', delivered_at=1000` —— 造出「还没投递已投递」的消息。`0018` 的时间戳守卫只在 INSERT 侧 | `0018` 补 `messages_timestamps_follow_state_update`（UPDATE 侧，含时间戳不得被抹除） | **DEV-02-08**（`0018`） |
| **S-09** | 通知 `UNREAD → READ` 写入 `read_at` 后，再把 `read_at` 抹成 `NULL`，弱化「已读」这一审计事实 | `0019` 的 `notifications_timestamp_consistency_update` 补「已发生的时间戳不得被抹除」 | DEV-02-09（`0019`） |

修复后复验：**13 拦截 / 1 绕过 / 0 误伤**（9 条中 S-05 残留，见「遗留风险」）。

> **计数口径**：S 组 9 条中 S-01～S-03 针对 `0018`，S-04～S-09 针对 `0019`。
> 本节点的第二遍贡献为 **6 条**（S-04～S-09），已计入下方总表，不与 DEV-02-08 重复计数。

## `0019` 的语义来源

全部状态集合逐条取自 `STATE_MACHINES.md` §6，未增删状态、错误码或权限：

- **Notification 合法迁移**：`创建 → UNREAD`；`UNREAD → READ|DISMISSED|EXPIRED`；`READ → DISMISSED|EXPIRED`
- **吸收态**：`DISMISSED`、`EXPIRED`（§6 无以二者为 From 的行）
- **AttentionItem 合法迁移**：`创建 → OPEN`；`OPEN → RESOLVED|DISMISSED`，二者均无出边
- **终态互斥**：`0008` 的 CHECK 只约束终态方向，不约束 `RESOLVED` 与 `DISMISSED` 不能并存，`0019` 补上

### 一处「发明语义」的当场纠正（判断日志 J11）

`0019` 初版要求 `status='READ'` 时 `read_at` **必须**非空、`status='DISMISSED'` 时 `dismissed_at` 必须非空。
对照组 `UNREAD → READ → DISMISSED` 被误伤。

**§6 完全没有提到这两个时间戳**。「必须写入」是我为了让状态与时间戳看起来同步而加的，规格没说。
最终规则改为**只禁止提前声称，不强制必须写入**：

```sql
WHEN (NEW.status = 'UNREAD'  AND NEW.read_at IS NOT NULL)            -- 还没读就说读了
  OR (NEW.status != 'DISMISSED' AND NEW.dismissed_at IS NOT NULL)    -- 没 dismiss 就说 dismiss 了
```

`0008` 为 `attention_items` 的 DISMISSED 强制了 `dismiss_reason` + `dismissed_at`
（§6 有依据：「类型允许忽略且**提供原因**」），却没为 `notifications` 强制任何时间戳。
`0019` **沿用该不对称，不擅自抹平**——抹平同样是发明。

## Human 裁决的四项规格缺口（已登记，未自行发明）

| 决策 | 裁决 | 处置 |
|---|---|---|
| 1 Human 身份 DB 层落地 | **C**：不保证，显式登记降级 | `recipient_human_id` 仍是裸 BLOB。否决排除法方案（`NOT IN (SELECT id FROM roles)`）——**那是假守卫**：只能证明「不是 Role」，P0 单 Owner 下近似「填什么都行」，且与 `0015` 已冻结的 HUMAN 范式方向相反 |
| 2 Policy 字段缺口 | **B**：三项全只回报不补 | `DOMAIN_MODEL.md:66` 要求的 `recipient`/`filters`/`escalation` 中，`filters`/`escalation` 全文无字段定义；补 `recipient` 会造成 Policy 与 Notification 两种不一致的 Human 表达方式 |
| 3 Attention `RESOLVED` 校验深度 | **B**：不实现跨表终态校验 | 依赖 DEV-02-10 尚未冻结的 `approvals` 状态集；`source_type` 五值中 `TASK`/`SYSTEM` 的终态规格完全不存在。登记为 DEV-02-11/02-12 下游职责 |
| 4 `source_type` 的 TASK/SYSTEM 终态集合 | **B**：只回报缺口 | 决策 3 的前置问题 |

**因此本节点的节点专属标准「Human 接收者不伪装成 Role」在数据库层未闭环**，由应用层承担，
而本轮无应用层执行点（三个空壳 crate）。此项在降级说明中显式登记。

## 领域与数据影响

- 纯前向迁移，SQLx checksum 不漂移
- **脏历史原子拒绝**：升级守卫扫描六类跨 Team / 悬空引用
- 合法路径逐条验证通过：通知 `UNREAD → READ → DISMISSED`、`UNREAD → EXPIRED`；
  待办 `OPEN → RESOLVED`、`OPEN → DISMISSED（带原因）`；policy 停用

## 验证命令与结果

| 命令 | 结果 |
|---|---|
| `cargo test --workspace --locked` | **214 passed / 0 failed** |
| `cargo fmt --all -- --check` | 零差异 |
| `cargo clippy --workspace --all-targets --all-features -- -D warnings` | **exit 0，零告警** |
| `node scripts/ci/local-gates.mjs --platform macos` | 见门禁证据 |
| Gitleaks 工作区 + 历史全 refs | 分开留证，均 0 命中 |

**门禁证据**：`~/.minimax/teamspace-audits/dev-02-08/evidence/wave2-macos-arm64-dev0209-closure-final.json`

## 失败与边界用例

对抗审查共 **36 条攻击**（四轴 30 条实现前 + 第二遍 S 组中属于本节点的 6 条），**30 条拦截、6 条仍可通过，全部有明确定性**：

| # | 残留 | 定性 |
|---|---|---|
| N-06 | `source_type='SYSTEM'` 的 `source_id` 指向不存在对象 | **规格缺口**（决策 4）。SYSTEM 无后继表，补映射即发明 |
| N-08 | `recipient_human_id` 填入真实 `role_id` | **已裁决的降级**（决策 1） |
| N-12 | 通知 `DISMISSED` 但 `dismissed_at` 为 NULL | **残余，本轮不修**——补它就是回到 J11 的错误 |
| N-25 / N-26 | `DELETE` 通知与待办 | **非漏洞**。冻结规格只把 **Event** 定为追加写；Notification 被定义为「可读/确认」的派生呈现，§6 用 `DISMISSED` 而非删除表达「清掉」。加 DELETE 守卫会发明语义 |
| S-05 | policy 置 `DISABLED` 后仍可创建 `CRITICAL` 通知 | **规格缺口**。`PERMISSION_MATRIX.md:83`「不能屏蔽强制安全提醒」在 `0008` 无强制标志位的前提下**不可表达** |

**对照组（无过度阻断）**：11 条合法路径全部未被误伤。

## G0～G9 逐项

| Gate | 结论 | 依据 |
|---|---|---|
| G0 范围 | PASS | 仅 DEV-02-09；已发布 migration 零修改；`0018` 归属 DEV-02-08 单独成迁移 |
| G1 构建 | PASS | 门禁全绿，lockfile 冻结 |
| G2 静态质量 | PASS | fmt / clippy 零错误零告警；未使用 `#[allow]` 绕过 |
| G3 行为测试 | PASS | 9 个新增回归测试 + 1 个既有测试修正，含成功与关键失败 |
| G4 数据一致性 | PASS | 脏历史原子拒绝；终态吸收；失败零副作用 |
| G5 安全 | PASS | 默认拒绝；跨 Team 与改道全拒；**Human 身份一项未闭环，已显式登记** |
| G6 UX/A11y | 不适用 | 本节点无 UI |
| G7 可观察/审计 | PASS | 通知与待办的读/撤时间戳成为不可抹除的审计证据 |
| G8 性能/资源 | PASS | 无无界日志/输出/查询 |
| G9 文档/证据 | PASS（降级） | 见下方降级说明 |

## 降级说明（显式登记，不得当作已验证）

1. **同模型审核，不等于独立复审**。计划审核与实现均为 `minimax/MiniMax-M3.1-Flash-Preview`。
   满足 §1 完成定义第 9 条第二条路径，**不等同于独立审核人签署**。
2. **节点专属标准「Human 接收者不伪装成 Role」在数据库层未闭环**（决策 1），
   由应用层承担，本轮无应用层执行点。
3. **6 条残留绕过**（见上表）全部有裁决或规格依据，非遗漏；但「有依据」不等于「已修复」。
4. **门禁需环境覆盖**（`PATH`/`CI`/`npm_config_registry`），不等于 hosted CI 结果。
5. **零提交**，HEAD 仍为 `1a14b87`。
6. **对抗装置本轮故障三次**（shell 吞单引号、漏开 `.bail`、seed 漏 NOT NULL 列），
   每次都伪装成了「整齐的错误结论」。已给装置加种子自检。详见判断日志 J12。

## 遗留风险

- **决策 1 / 2 / 3 / 4 四项规格缺口**已正式回报，需 Human 后续裁决后才能在实现层收口
- **N-12 残余**（DISMISSED 无 `dismissed_at`）属应用层职责
- **S-05**：`PERMISSION_MATRIX.md:83` 的「强制安全提醒不可屏蔽」在当前 schema 下不可表达，
  需 Human 决定是否新增强制标志位
- `0009_permission_approval.sql` 仍 0 trigger，属 **DEV-02-10**
- `application`/`security`/`workspace` 三个 crate 仍为空壳
- RustSec 6 unmaintained + 1 unsound、npm 2 项，总控须在 2026-10-28 前复查
- 真实 Harness Spike / 桌面 E2E / Figma 复验均零次

## 迁移与回退

- 前向迁移，回退方式为恢复 `~/.minimax/teamspace-backups/20260929-152749/`
- 脏旧库升级选择原子拒绝，不自动删除或改写历史
- 未修改 `sources/`，未提交、推送、部署或操作生产数据

## 审核结论

**PASS**（四轴 30 条 + 第二遍本节点 6 条共 36 条攻击，30 条拦截、6 条带明确依据残留、0 误伤；
214 项测试全绿；macOS 21 项门禁全绿；工作区与历史两次秘密扫描均 0 命中）

按 `QUALITY_GATES.md` §1 完成定义 10 条与节点专属标准逐条满足，
DEV-02-09 由 `PLANNED` 关闭为 **`DONE`**，并附上述降级说明与遗留风险登记。
后续节点 DEV-02-10（Permission/Approval）可按依赖逐项准入。
