# DEV-02-07 Event/Outbox 表 —— 节点终审（2026-09-29）

> 结论：**DONE（附一项降级说明）**。依据 [DEVELOPMENT_NODES](../development/DEVELOPMENT_NODES.md) 节点专属标准、[QUALITY_GATES](../development/QUALITY_GATES.md) G0～G9、[TECH_STACK_AND_STANDARDS](../development/TECH_STACK_AND_STANDARDS.md) 冻结技术栈、[ADR-0005](../adr/0005-macos-first-quality-and-release-gates.md) 与[冻结清单](../v2/TASK_FREEZE.md)。当前验证平台为 macOS ARM64。
> 本记录不判定其余 Wave 2 节点，也不代表 V2 R1 完成。

## 审核分工与模型路由

- **执行模型**：`minimax/MiniMax-M3.1-Flash-Preview`
- **模型路由决策**：Human 于 2026-09-29 明确授权 MiniMax-M3 暂代 ChatGPT/Codex 推进本节点，并限定未经允许不使用其他模型。此为对 `AGENTS.override.md` 路由规则与 ADR-0004「MiniMax-M3 仅边角辅助」的一次显式 Human 例外，**仅适用于本节点**。
- **独立性降级**：R3 关键变更要求「独立复核或第二遍反例审查」。因无可用独立复审方（Human 限制不使用其他模型），本节点采用**第二遍对抗性反例审查**：在 `recursive_triggers=0` 的独立数据库上自建 Team A/B 全套图谱，自行设计 24 条攻击向量，每条使用独立数据库副本，**不复用实现者自建用例**。此为标准允许的第二种满足方式，不等同于独立复核人签署。
- 原 Wave 2 实现者 Gemini-3.8-Flash 的 `SELF_CHECK_PASS` 仅作待审资料，不构成本节点的正式依据。

## 范围与非目标

**范围**：DEV-02-07 业务变更与 Event/Outbox 原子提交、消费可重放与去重、Event 无更新接口。

**本轮实际变更**：

| 文件 | 变更 |
|---|---|
| `crates/persistence-sqlite/migrations/0016_event_dedup_state_immutability.sql` | 新增（前向迁移） |
| `crates/persistence-sqlite/src/lib.rs` | 注册 0016 |
| `crates/persistence-sqlite/tests/event_dedup_state_immutability_forward_migration.rs` | 新增 6 个回归测试 |
| `crates/persistence-sqlite/tests/event_actor_outbox_integrity_forward_migration.rs` | `role_event()` 由 10 参数重构为 `RoleEventSpec`/`RoleActor` 结构体（修 clippy `too_many_arguments`） |

**非目标**：`0001`～`0015` 已发布 migration 未修改一字节；应用层 Outbox dispatcher 合法流转定义（属 DEV-02-14）；Artifact 可见性与 `ROLE_RESTRICTED` 权限判定（属后续 application 节点）。

## 前序状态与本轮发现

`0014`、`0015` 由前一轮会话写入，`0015` 写完后会话中断，**从未被复审**。会话归档（`~/.minimax/codex-import/协作工作/`）显示原独立复审者 Ampere（`dev_02_07_independent_review`）于 2026-09-28 18:49 对 **0014** 判定 **BLOCKED**，提出两处 R3 缺口：

1. `events` 缺冻结要求的 Role-Binding 执行者审计快照
2. Outbox 与消费去重可被 `INSERT OR REPLACE` 绕过

`0015` 照该规格实现。本轮第二遍审查确认**缺口①已完整闭合**，**缺口②仅部分闭合**：`0015` 只安装了 `BEFORE INSERT` 守卫，`UPDATE` 路径仍可重置去重状态。

## 本轮复现的绕过与修复

在 `0015` 基线上复现 6 项绕过，全部由 `0016` 关闭：

| 攻击 | 观测 | 0016 守卫 |
|---|---|---|
| `UPDATE event_consumptions SET consumed_at=0` | `7 → 0` | `event_consumptions_deduplication_immutable_update` |
| `UPDATE event_consumptions SET consumer_id='attacker'` | `projector → attacker` | 同上 |
| `UPDATE outbox SET status='PENDING', retry_count=0` | `PUBLISHED/3 → PENDING/0` | `outbox_deduplication_state_immutable_update` |
| `UPDATE outbox SET published_at=NULL` | 丢失发布时间 | 同上 |
| `UPDATE outbox SET idempotency_key='x'` | 幂等键可改写 | 同上 |
| Binding 为 `REVOKED`/`FAILED` 时写入执行者快照 | `events 1 → 2` | 重建 `events_actor_snapshot_consistency_insert`，增加 `status IN ('ACTIVE','DRAINING')` |

**语义依据**：`ACTIVE`/`DRAINING` 并非本轮新造语义。冻结的 `bindings` 表 CHECK 约束已把这两项单独归组，规定只有它们要求 `effective_from` 非空且 `effective_to` 为空，即 schema 自身定义的「可执行」状态。`PENDING_CHECK`/`REVOKED`/`FAILED` 均不满足该分组。

`0016` 采用 `DROP TRIGGER` + `CREATE TRIGGER` 前向重建，未修改 `0015` 文件，故其 SQLx checksum 不漂移。

## 领域与数据影响

- 迁移为**纯前向**，`0001`～`0015` 未改写
- **脏历史原子拒绝**：已发布却无 `published_at`、消费记录无 `consumed_at`、死信却标记已发布——三类状态在升级前被守卫拒绝，不静默修复、不伪造回填
- 合法演进仍可用：`PENDING→PROCESSING→FAILED→PENDING(重试)→PUBLISHED` 全部放行；`retry_count` 只能递增；历史快照在其 Binding 后续被吊销或版本递增后保持有效
- 保留已知设计：`DELETE` 后重新 `INSERT` 被 `0015` 注释显式声明为合法 replay，本轮未改动该语义

## 安全与隐私影响

- Event 追加写不可绕过：UPDATE / DELETE / `INSERT OR REPLACE` 全部拒绝
- 跨 Team 的 actor / subject / 投递引用全部拒绝
- 消费去重主键与时间戳不可改写，消除重复消费路径
- 无真实秘密进入 Event、日志或 fixture；全 refs Gitleaks 零命中

## 验证命令与结果

| 命令 | 结果 |
|---|---|
| `cargo test --workspace --locked` | **190 passed / 0 failed**（原 184 + 0016 的 6） |
| `cargo fmt --all -- --check` | 零差异 |
| `cargo clippy --workspace --all-targets --all-features -- -D warnings` | **exit 0，零告警**（修前 exit 101） |
| `node scripts/ci/local-gates.mjs --platform macos` | **21/21 退出码为 0，`result=pass`** |
| `gitleaks git --log-opts='--all'` | 4 commits / 10 refs 扫描，**0 命中** |

**门禁证据**：`~/.minimax/teamspace-audits/dev-02-07/evidence/wave2-macos-arm64-dev0207-closure-final.json`

- 该 JSON 的 `input_manifest.total_sha256` 与 `input_manifest_verified_after_commands` 记录**最终输入**（含本记录与节点表更新）的清单哈希，脚本已独立复核两者相同
- 本记录及节点表写入会改变此前任何一次门禁所对应的输入，**只有上述 closure-final 证据对本节点生效**；以证据文件内记录的哈希为唯一准绳
- 环境覆盖：`PATH` 追加 `~/.minimax/toolchain/bin`、`CI=true`、`npm_config_registry=https://registry.npmjs.org`（`~/.npmrc` 的 npmmirror 镜像不实现 audit API，第 18 项无法在该源完成）

**工具来源证据**：`evidence/tool-provenance.json`

| 工具 | 版本 | 来源 | 二进制 SHA-256 |
|---|---|---|---|
| gitleaks | 8.30.0 | 官方 release `gitleaks_8.30.0_darwin_arm64.tar.gz`，archive SHA-256 `b251ab2b…` 与官方 checksums 文件及前次已验证记录三方一致 | `095bc4be…`（与前次已验证记录逐字节一致） |
| cargo-deny | 0.18.6 | 官方 release `cargo-deny-0.18.6-aarch64-apple-darwin.tar.gz`，archive SHA-256 与官方 `.sha256` 一致 | `62a4417d…` |
| cargo-audit | 0.22.0 | crates.io `cargo install --locked` 源码构建（0.22.0 无 aarch64-apple-darwin 资产，x86_64 资产在本机无 Rosetta 2 不可用） | `0be59786…`（与前次已验证记录逐字节一致） |

三个工具在本轮开始前均未安装；`gitleaks` 与 `cargo-audit` 装好后与 2026-09-28 前次会话记录的已验证哈希**完全一致**，构成可核对的来源链。

## 失败与边界用例

第二遍对抗审查共 24 条攻击 + 对照：

- **A 组（快照不可变性）10/10 拦截**：Event 的 UPDATE/DELETE/REPLACE、Role/Binding/Harness 被引用后改 `team_id`、REPLACE 同 id
- **B 组（去重）10/10 拦截**：INSERT OR REPLACE 两条、UPDATE 重置 `consumed_at`、改 `consumer_id`、改 `event_id`、Outbox 回退 `status`/`retry_count`、清 `published_at`、改 `idempotency_key`、改 `team_id`
- **C 组（引用与语义）5/5 拦截**：Outbox/消费指向不存在 Event、重复 Event id、SYSTEM 携带残留快照字段、ADAPTER 作为 actor
- **对照组 2/2 放行**：合法 ROLE_BINDING 快照、合法新增 consumption；另有 7 条合法 Outbox 向前流转逐条验证仍可用
- **事务原子性**：Event 写入成功而 Outbox 写入失败时整体回滚，`events` 与 `outbox` 计数均未变

## G0～G9 逐项

| Gate | 结论 | 依据 |
|---|---|---|
| G0 范围 | PASS | 仅 DEV-02-07；已发布 migration 零修改；无无关重构 |
| G1 构建 | PASS | 门禁第 5、12、13、14 项全绿，lockfile 冻结 |
| G2 静态质量 | PASS | fmt / lint / typecheck / clippy 零错误零告警；未使用 `#[allow]` 绕过 |
| G3 行为测试 | PASS | 6 个新增回归测试 + 原有 184，含成功与关键失败 |
| G4 数据一致性 | PASS | 脏历史原子拒绝；竞争与终态有确定结果；失败零副作用 |
| G5 安全 | PASS | 默认拒绝；跨 Team 全拒；无秘密泄露 |
| G6 UX/A11y | 不适用 | 本节点无 UI（`QUALITY_GATES.md` §2 允许 G0/G1/G2/G3/G9 之外的适用性判定） |
| G7 可观察/审计 | PASS | actor 快照、request_id、append-only、hash chain 材料未被改写 |
| G8 性能/资源 | PASS | 无无界日志/输出/查询；本节点无进程生命周期 |
| G9 文档/证据 | PASS（降级） | 见下方降级说明 |

## 降级说明（必须显式登记，不得当作已验证）

1. **无独立复审人**。R3 关键变更的「独立复核」由「第二遍反例审查」替代，满足标准允许的第二条路径，但**不等同于独立审核人签署**。后续若条件具备，应补一次独立复核。
2. **门禁环境覆盖**。`CI=true` 与官方 npm registry 均为使门禁可完成的必要环境设置，非门禁削弱；但这意味着本次证据不等于 hosted CI 的 branch protection 结果。
3. **零提交**。全部 65 条改动仍未提交（18 modified / 44 untracked / 3 deleted），HEAD 仍为 `1a14b87`。本节点证据绑定输入清单哈希，不依赖提交，但版本控制仍处未收口状态。

## 遗留风险

- `application` / `security` / `workspace` 三个 crate 仍为空壳（各 1 行），所有权限判定目前仅存在于 SQL trigger 层，**无应用层执行点**。数据库层不可单独保证「每个业务变更必有 Event/Outbox」，该原子性须在 DEV-02-14 的应用事务中另证。
- `App.tsx` 11 行，无真实产品 UI。
- 真实 Codex ACP Spike 零次、桌面 E2E 零次、Figma 逐画板复验零次。
- RustSec 6 条 unmaintained + 1 条 unsound informational warning（由 Tauri `urlpattern` 链引入，无对应漏洞公告）；npm `ajv` moderate ReDoS、`esbuild` low（Mac 门禁阈值 high，不阻断）。**总控须在 2026-10-28 前复查上游替换/升级，Gate I 前重新评估，不得把本结论当作永久豁免。**
- `DELETE` 后重新 `INSERT` 仍可重置去重状态，属 `0015` 显式接受的设计，须在应用层限制。

## 迁移与回退

- 前向迁移，回退方式为恢复备份 `~/.minimax/teamspace-backups/20260929-102144/`（已实测可还原至输入清单 `f2982d21…`）
- 脏旧库升级选择原子拒绝，不自动删除或改写历史
- 未修改 `sources/`，未提交、推送、部署或操作生产数据

## 审核结论

**PASS**（第二遍对抗性反例审查 + 21/21 macOS 门禁 + 全 refs 秘密扫描）

按 `QUALITY_GATES.md` §1 完成定义 10 条与节点专属标准「业务变更与 Event/Outbox 原子提交；消费可重放和去重；Event 无更新接口」逐条满足，DEV-02-07 由 `PLANNED` 关闭为 **`DONE`**，并附带上述三项降级说明与遗留风险登记。

依据冻结清单，后续节点 DEV-02-08（Thread/Message/Delivery）可按依赖逐项准入；任一输入变化将使本证据失效，须重跑完整 Mac 门禁。
