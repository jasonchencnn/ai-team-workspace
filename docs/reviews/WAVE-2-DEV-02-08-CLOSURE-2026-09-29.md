# DEV-02-08 Thread/Message/Delivery 表 —— 节点终审（2026-09-29）

> ⚠️ **本文档的结论已作废，仅作过程留痕保留。**
> 有效结论见 [DEV-02-08 重签](WAVE-2-DEV-02-08-RESIGN-2026-09-29.md)。
> 作废原因：初版写的「35 条攻击全部拦截，0 绕过」不完整——那 35 条**全部是 UPDATE 语义**，
> 整类「创建轴」绕过从未被尝试。同模型独立审核指出后实测确认：消息可直接以 `DELIVERED` 状态诞生，
> 投递记录可直接以 `SUCCEEDED` 诞生且 `completed_at` 为空。本轮新增 `0018` 修复后重签。
> 本文档中未受影响的记录（调研结论、语义来源、领域影响、迁移与回退）仍然有效。

> 结论：**DONE（附降级说明）**。依据 [DEVELOPMENT_NODES](../development/DEVELOPMENT_NODES.md) 节点专属标准、[QUALITY_GATES](../development/QUALITY_GATES.md) G0～G9、[STATE_MACHINES](../specs/STATE_MACHINES.md) §5 冻结迁移表、[TECH_STACK_AND_STANDARDS](../development/TECH_STACK_AND_STANDARDS.md) 冻结技术栈与 [ADR-0005](../adr/0005-macos-first-quality-and-release-gates.md)。当前验证平台为 macOS ARM64。
> 本记录不判定其余 Wave 2 节点，也不代表 V2 R1 完成。

## 审核分工与模型路由

- **执行模型**：`minimax/MiniMax-M3.1-Flash-Preview`
- **模型路由**：Human 于 2026-09-29 授权本模型暂代 ChatGPT/Codex 推进，并限定未经允许不使用其他模型。该授权为对 `AGENTS.override.md` 路由规则与 ADR-0004 的一次显式 Human 例外。
- **独立性降级**：R3 关键变更要求「独立复核或第二遍反例审查」。因无可用独立复审方，本节点采用**第二遍对抗性反例审查**，且分为两个阶段：
  - **实现前**：在 `0007` 基线上自建 Team A/B 图谱，24 条攻击复现 13 项绕过
  - **实现后**：另设计 11 条此前未尝试的新向量（G/H 组）复验，并据此补出一个实现后才发现的缺口
- 该方式满足标准允许的第二条路径，**不等同于独立审核人签署**。

## 范围与非目标

**范围**：DEV-02-08「`to_role_id` 强制存在；Binding 快照、reply_to、TTL、幂等和 attempt 历史完整」。

| 文件 | 变更 |
|---|---|
| `crates/persistence-sqlite/migrations/0017_message_delivery_integrity.sql` | 新增（前向迁移，18 个 trigger） |
| `crates/persistence-sqlite/src/lib.rs` | 注册 0017 |
| `crates/persistence-sqlite/tests/message_delivery_integrity_forward_migration.rs` | 新增 10 个回归测试 |
| `crates/persistence-sqlite/tests/thread_message_delivery_migration.rs` | 既有 4 个用例的 fixture 修正为符合 0017 与冻结规格 |

**非目标**：`0001`～`0016` 与 `0007` 原文件零修改；消息状态机合法集合的**定义**（取自冻结规格，非本轮发明）；TTL 过期判定、幂等同键异 payload 检测（属应用层）。

## 调研结论：`0007` 的缺口性质

`0007` 仅含 2 个 trigger（正文冻结、`reply_to` 同 thread），**无 Team 守卫、无状态机约束、无投递记录不可变性**。而 0010～0016 已为 Binding、Task、Run、Artifact、Event、Outbox、Consumption 建立了该模式——`0007` 被整体跳过。

这与 DEV-02-07 的缺口性质不同：DEV-02-07 是「守卫只有 INSERT 缺 UPDATE」，本节点是**根本没有守卫**。

### 实现前复现的 13 项绕过

跨 Team 完整性 5 项：跨 Team 收件人、跨 Team thread、跨 Team sender binding 快照、跨 Team 投递目标、attempt 收件人 ≠ 消息 `to_role_id`（投递改道）。

不可变性与状态机 8 项：已投递消息改投收件人、`DELIVERED→DRAFT` 回退、`EXPIRED` 复活、`idempotency_key` 改写（含改为与他人重复）、sender binding 快照改指别 Team、投递结果 `FAILED→SUCCEEDED` 翻转、attempt `target_role_id` 改写、`attempt_number` 改写复用编号。

### 实现后新发现并修复的 1 项

第二遍审查的 G6：`sender_actor_type='SYSTEM'` 的消息可携带 Role-Binding 快照。
0015 已为 Event 冻结「HUMAN/SYSTEM 无持久化映射、不得携带快照」，消息侧此前更宽松。
`0017` 补 `messages_non_role_snapshot_rejected_insert` 对齐 H1 同理。**该项若不做实现后复验不会被发现。**

## `0017` 的语义来源

全部状态集合逐条取自 `STATE_MACHINES.md` §5，未增删状态、错误码或权限：

- **Message 合法迁移**（13 条）：`创建→DRAFT`；`DRAFT→QUEUED`；`QUEUED→ROUTED|NEEDS_HUMAN|EXPIRED`；`ROUTED→EXPIRED|DELIVERED|QUEUED|FAILED`；`DELIVERED→ACKNOWLEDGED`；`ACKNOWLEDGED→PROCESSING|REPLIED`；`PROCESSING→REPLIED`；`FAILED→QUEUED`；`NEEDS_HUMAN→QUEUED`
- **吸收态**：`EXPIRED`、`REPLIED`（§5 无以二者为 From 的行；第 150 行明文「`EXPIRED` 不在原 Message 上重开」）
- **DeliveryAttempt**：`PENDING→IN_FLIGHT`；`IN_FLIGHT→SUCCEEDED|FAILED|ABANDONED`；三者均吸收
- **sender Binding snapshot**：§5 第 137 行定为 `DRAFT→QUEUED` 的副作用

### 语义判断记录

全部取舍见 `~/.minimax/teamspace-audits/dev-02-08/JUDGMENT-LOG.md`（J1～J10）。三处最关键：

| 判断 | 决定 | 依据 |
|---|---|---|
| J3 | 不开放 `PENDING→FAILED` 直达终态 | 规格只给出 `IN_FLIGHT` 到终态的链路；宁可收紧也不发明语义 |
| J4 | 仅 `ROLE` sender 强制快照；HUMAN/SYSTEM 允许为空但**不得携带** | §5 第 136 行 Human 代发合法；0015 已为 Event 冻结同规则 |
| J5 | `to_role_id` 离开 DRAFT 后不可改写；改派体现在新 attempt 上 | §5 第 137 行「冻结内容」；第 148 行副作用只提「新 attempt」与 route override |

## 领域与数据影响

- 纯前向迁移，`0001`～`0016` 与 `0007` 原文件零修改，SQLx checksum 不漂移
- **脏历史原子拒绝**：三类状态在升级前被守卫拒绝——跨 Team 寻址记录、attempt 与 message 的 Team/收件人不一致、已离开 DRAFT 的 Role sender 缺失或不匹配快照
- 合法路径仍可用：`DRAFT→QUEUED→ROUTED→DELIVERED→ACKNOWLEDGED` 逐条验证通过；`PENDING→IN_FLIGHT→SUCCEEDED` 通过；`FAILED` 消息经显式 retry 回到 `QUEUED` 并新建 attempt 时通过

### 一处既有测试的修正

`thread_message_delivery_migration.rs` 原有断言 `UPDATE messages SET status='DELIVERED'`（自 `QUEUED`）应当成功。该边在 §5 中**不存在**——`ROUTED` 是必经态。测试已改为：先断言该捷径被拒，再走合法路径。同时补断言 sender Binding 快照在离开 DRAFT 后被冻结。

**这不是把测试改成迁就实现，而是把测试对齐冻结规格。**

## 安全与隐私影响

- 跨 Team 消息寻址、跨 Team 投递、投递改道全部拒绝
- 幂等键与消息身份不可改写，`INSERT OR REPLACE` 不再能绕过 `(team_id, idempotency_key)` 唯一索引
- 投递结果不可翻转、attempt 编号不可复用，投递历史成为可信审计证据
- 无真实秘密进入 Event、日志或 fixture

## 验证命令与结果

| 命令 | 结果 |
|---|---|
| `cargo test --workspace --locked` | **200 passed / 0 failed**（DEV-02-07 收口时 190 + 0017 的 10） |
| `cargo fmt --all -- --check` | 零差异 |
| `cargo clippy --workspace --all-targets --all-features -- -D warnings` | **exit 0，零告警** |
| `node scripts/ci/local-gates.mjs --platform macos` | 21/21 命令 exit 0，`result=pass` |
| `gitleaks detect --source . --no-git`（门禁内，工作区 2.23 MB） | 0 命中 |
| `gitleaks git --log-opts='--all'`（**门禁外单独执行**，历史 4 commits / 709.70 KB） | 0 命中；证据 `~/.minimax/teamspace-audits/dev-02-08/evidence/wave2-gitleaks-history-all-refs-20260929.json` |

**门禁证据**：`~/.minimax/teamspace-audits/dev-02-08/evidence/wave2-macos-arm64-dev0208-closure-final.json`

该 JSON 的 `input_manifest.total_sha256` 与 `input_manifest_verified_after_commands` 记录**最终输入**（含本记录与节点表更新）的清单哈希，脚本已独立复核两者相同。**本记录及节点表写入会使此前任何一次门禁失效，只有上述 closure-final 证据对本节点生效。**

> **两次秘密扫描的分工（不可混为一谈）**：门禁 21 项内含的是**工作区文件**扫描（`--no-git`），历史全 refs 扫描**不在门禁计划内**，是本轮单独执行并单独留证。两者均为 0 命中，但证据文件不同，不可相互引用。

**环境覆盖**（沿用 DEV-02-07，非门禁削弱）：`PATH` 追加 `~/.minimax/toolchain/bin`、`CI=true`、`npm_config_registry=https://registry.npmjs.org`（`~/.npmrc` 的 npmmirror 镜像不实现 audit API）。

**工具来源**：`~/.minimax/teamspace-audits/dev-02-07/evidence/tool-provenance.json` 继续有效——gitleaks 8.30.0、cargo-audit 0.22.0、cargo-deny 0.18.6，其中 gitleaks 与 cargo-audit 的二进制 SHA-256 与 2026-09-28 前次 Codex 会话记录的已验证哈希逐字节一致。

## 失败与边界用例

> ⛔ **以下小节的结论已作废。** 这 35 条攻击**全部是 UPDATE 语义**，
> 整类「创建轴」绕过从未被尝试，因此「0 绕过」不成立为完整结论。
> 有效结论见[重签文档](WAVE-2-DEV-02-08-RESIGN-2026-09-29.md)。
> 本小节仅保留攻击清单作为方法留痕。

~~对抗审查共 35 条攻击（D/E/F 组 24 条实现前 + G/H 组 11 条实现后），全部拦截，0 绕过：~~

- **跨 Team**：收件人、thread、sender 快照、投递目标、attempt 收件人改道
- **不可变性**：改投收件人、状态回退、EXPIRED 复活、幂等键改写、快照改指、结果翻转、目标改写、编号复用
- **替换路径**：消息 `INSERT OR REPLACE`、attempt `INSERT OR REPLACE` 复用编号、thread `INSERT OR REPLACE`
- **父记录保护**：被引用 thread 改 Team、被引用 thread 删除
- **快照语义**：SYSTEM/HUMAN 携带快照、ROLE sender 缺失 `from_role_id`

**对照组（无过度阻断）**：合法 `DRAFT` 消息无快照、合法 SYSTEM sender 无快照、合法投递到 `to_role_id`、合法状态推进链、合法 attempt 推进链、`FAILED` 显式 retry。

## G0～G9 逐项

| Gate | 结论 | 依据 |
|---|---|---|
| G0 范围 | PASS | 仅 DEV-02-08；已发布 migration 零修改；无无关重构 |
| G1 构建 | PASS | 门禁第 5、12、13、14 项全绿，lockfile 冻结 |
| G2 静态质量 | PASS | fmt / lint / typecheck / clippy 零错误零告警；未使用 `#[allow]` 绕过 |
| G3 行为测试 | PASS | 10 个新增回归测试 + 4 个修正后的既有测试，含成功与关键失败 |
| G4 数据一致性 | PASS | 脏历史原子拒绝；终态吸收；失败零副作用 |
| G5 安全 | PASS | 默认拒绝；跨 Team 与改道全拒；无秘密泄露 |
| G6 UX/A11y | 不适用 | 本节点无 UI |
| G7 可观察/审计 | PASS | 消息寻址与投递历史成为可信审计证据 |
| G8 性能/资源 | PASS | 无无界日志/输出/查询；本节点无进程生命周期 |
| G9 文档/证据 | PASS（降级） | 见下方降级说明 |

## 降级说明（显式登记，不得当作已验证）

1. **无独立复审人**。以第二遍对抗性反例审查替代独立复核，满足标准第二条路径，但**不等同于独立审核人签署**。
2. **门禁需环境覆盖**。本次证据不等于 hosted CI 的 branch protection 结果。
3. **零提交**。全部改动未提交，HEAD 仍为 `1a14b87`。本节点证据绑定输入清单哈希，不依赖提交。
4. **两处自我纠错**（见判断日志 J8）。首轮攻击连接未开 `foreign_keys` 造成 4 项假阳性；`0017` 首版 `format!` 无参调用被 clippy 拦截。均已修正并复验。
5. **勘误（2026-09-29 收尾修正）**：本记录初版把 `0017` 的 trigger 数写作 **16**，实际为 **18**（`grep -oE "CREATE TRIGGER"` 逐条计数：`messages_*` 8 个 + `delivery_attempts_*` 7 个 + `threads_*` 3 个）。该错误在启动 DEV-02-09 调研、比对各 migration trigger 密度时被发现。更正时我判断「事实结论不受影响，仅计数笔误」——**这个判断本身是错的**，同一次调研即发现 35 条攻击全是 UPDATE 语义。本条已由[重签文档](WAVE-2-DEV-02-08-RESIGN-2026-09-29.md)推翻。登记为判断日志 J16。

## 遗留风险

- **J5 的张力**：`NEEDS_HUMAN → QUEUED` 的规格前置含「新目标有效」，本轮按「改派体现在新 attempt」实现，`messages.to_role_id` 保持不可变。若 DEV-05-07 的改派用例确需改写该字段，**须先更新 `STATE_MACHINES.md` 再放宽守卫**，不得在实现层绕过。
- **TTL 过期判定**（`now >= expires_at`）属应用层（DEV-05-07）；数据库只保证字段、部分索引与 `EXPIRED` 不可回退。
- **幂等同键异 payload 检测**属 DEV-02-14 应用事务职责，数据库只保证键不可改写与 `OR REPLACE` 不绕过。
- **`PENDING→FAILED` 未开放**（J3），排队期超时需先置 `IN_FLIGHT`，多一次写入。
- `application`/`security`/`workspace` 三个 crate 仍为空壳，权限判定只在 SQL trigger 层，无应用层执行点。
- 真实 Harness Spike / 桌面 E2E / Figma 逐画板复验均零次。
- RustSec 6 unmaintained + 1 unsound、npm 2 项，**总控须在 2026-10-28 前复查**。

## 迁移与回退

- 前向迁移，回退方式为恢复 `~/.minimax/teamspace-backups/20260929-105552/`（已实测可还原）
- 脏旧库升级选择原子拒绝，不自动删除或改写历史
- 未修改 `sources/`，未提交、推送、部署或操作生产数据

## 审核结论

> ⛔ **本节结论已作废，不得引用。** 有效结论见[重签文档](WAVE-2-DEV-02-08-RESIGN-2026-09-29.md)。

~~**PASS**（两阶段对抗性反例审查共 35 条攻击全部拦截 + macOS 21 项门禁全绿）~~

~~按 `QUALITY_GATES.md` §1 完成定义 10 条与节点专属标准逐条满足，DEV-02-08 由 `PLANNED` 关闭为 `DONE`。~~
重签结论：DEV-02-08 仍为 **`DONE`**，但攻击覆盖已扩充到创建轴（`0018`），攻击总数 13 条全部拦截。

依据冻结清单，后续节点 DEV-02-09（Notification/Policy/Attention）可按依赖逐项准入；任一输入变化将使本证据失效，须重跑完整 Mac 门禁。
