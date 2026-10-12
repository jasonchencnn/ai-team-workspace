# 工作日志

> 2026-10-10 DEV-02-09最新：[限定整改审查](../reviews/DEV-02-09-SCOPE-REVIEW-2026-10-10.md)已修复通知异ID去重替换与Task父端跨Team引用，当前迁移0001～0035。新专项13/0、注册2/0、恢复后persistence303/0，当前stable Mac21整体PASS（21/21，Rust723/0/1、TS36/0，226项工程/策略输入不变），当前C7两层行/分支均达标。可信Human身份未闭环，09仍BLOCKED；正式 **35 DONE / 6 BLOCKED / 60 PLANNED**、Gate C BLOCKED不变。最小后续为[Owner身份决策](../coordination/DEV-02-09-OWNER-DECISION-2026-10-10.md)，不把全部05-08用例倒挂为09前置，不接受ADR-0007或恢复整批外派。下列HC-01等按历史时点保留。


> 2026-10-10 HC-01终审最新：DEV-02-08基础存储恢复DONE，[终审](../reviews/DEV-02-08-HC01-CLOSURE-REVIEW-2026-10-10.md)确认正式0033、完整历史及改派后换绑；原57+兼容11+注册1、全persistence288/0，当前stable Mac21整体PASS，Rust708/0/1、TS36/0，222项工程/扫描策略输入不变。正式 **35 DONE / 6 BLOCKED / 60 PLANNED**；Gate C仍BLOCKED，未启动下游产品用例。当前覆盖率Domain行91.67%/分支93.36%、Application行93.75%/分支100%。以下34/7/60、HC-01待回传等按历史时点保留。

> 2026-10-10 历史/兼容核对最新：[08专项审查](../reviews/DEV-02-08-HISTORY-COMPAT-REVIEW-2026-10-10.md)现有六套57/0，旧历史保留；新仓外回归确认改派到B后B换绑无法采用当前Binding，HC-01阻断08。已签[Minimax专项包](MINIMAX-HISTORY-COMPAT-PACKET-2026-10-10.md)供Human转发，仅仓外修复候选与互斥回传目录，不恢复整批外派。生产工程219项SHA不变，本轮未重跑Mac21或重签节点；正式 **34 DONE / 7 BLOCKED / 60 PLANNED**、Gate C BLOCKED不变。后续权限/幂等/dispatcher/产品用例仍按12/13/14/05/07归属，不倒挂为08新前置。以下按日期保留历史。

> 2026-10-10 07终审最新：按冻结专属标准完成[责任核对与单节点终审](../reviews/DEV-02-07-SCOPE-CLOSURE-REVIEW-2026-10-10.md)，07基础节点恢复DONE；正式 **34 DONE / 7 BLOCKED / 60 PLANNED**。58专项/七项真实反例、当前stable Mac21整体PASS，Rust696/0/1、TS36/0，219项工程输入不变。原先笼统列入07的后续格式扩展、dispatcher和host接线已按冻结依赖归还02-14/05/07后续验收，不新增语义或启动下游。Gate C仍阻塞；下列07 BLOCKED和33/8/60等段落为历史时点。


> 2026-10-10 分支补测最终：四个Application遗漏结果已覆盖，Domain行91.67%/branch93.36%，Application行93.75%/branch100%；当前受检输入stable Mac21整体PASS，Rust696/0/1、TS36/0。依据[本轮验收及单节点复核](../reviews/DEV-02-07-BRANCH-TEST-REVIEW-2026-10-10.md)，仅DEV-02-11恢复DONE，正式 **33 DONE / 8 BLOCKED / 60 PLANNED**。07整节点与Gate C继续阻塞；生产源码、迁移、manifest/lock和用户stable/default不变，工程仅补测试。下列旧32/9/60、branch未证或77.78%均为历史时点。


> 2026-10-10 C7最新：Human已接受ADR-0009，官方核验、仓外隔离安装与真实分支探针PASS；[专项验收](../reviews/C7-BRANCH-COVERAGE-REVIEW-2026-10-10.md)测得Domain行91.67%/分支93.36%，Application行91.25%/分支77.78%。Application分支未达85%，四个遗漏结果已定位，Gate C和正式32/9/60不晋级。218项工程输入及用户stable/default前后不变；本轮仅文档/证据改动。


> 2026-10-10 最新执行：DEV-02-07 原子消费/去重、投影重建及三处真实进程崩溃重放已在限定范围通过，[验收](../reviews/DEV-02-07-AUDIT-CONSUMER-REVIEW-2026-10-10.md)为当前 canonical Mac21 PASS，Rust692/0/1、TS36/0。当前迁移0001～0032；行覆盖率Domain91.90%、Application93.41%，branch仍未证。正式 **32 DONE / 9 BLOCKED / 60 PLANNED** 不变；dispatcher、广义历史格式/host装配、通知和完整阶段未准入。下列旧统计和迁移范围按历史时点保留。

当前[消费验收](../reviews/DEV-02-07-AUDIT-CONSUMER-REVIEW-2026-10-10.md)已替代“消费尚未实现”的旧缺口；[ADR-0009](../adr/0009-isolated-branch-coverage-tool-proposal.md)已Accepted，固定候选已按Human授权核验/隔离安装并测量。工具可用而Application分支未达标；没有提交、推送、部署或真实数据库操作。

> 本轮最新执行：DEV-02-07 内部 Message 到期事务子包已通过，[验收](../reviews/DEV-02-07-ATOMIC-EXPIRY-REVIEW-2026-10-09.md)为 canonical Mac21 整体 PASS、Rust674/0/1、TS36/0，13项应用真库测试与3项行为反例通过。当前迁移0001～0031；application不再仅为占位，但consumer/dispatcher、可信Human/Role权限、PENDING到期语义与覆盖率仍未完成。正式 **32 DONE / 9 BLOCKED / 60 PLANNED** 不变；旧段落数字和迁移范围按历史时点保留。

执行记录见[本轮审核](../reviews/DEV-02-07-ATOMIC-EXPIRY-REVIEW-2026-10-09.md)；后续优先07消费重放/去重，旧日志和旧模型执行事实保留。没有提交、推送、部署或操作真实数据库。

> 2026-10-09 C6后的继续执行：新增0030及改派存储整改，最终Mac21整体PASS（Rust661/0/1、TS36/0）；8项真库专项、四个反例变异、旧迁移与根锁保护通过。完整DEV-02-08仍BLOCKED，正式32/9/60不变。详见[验收](../reviews/DEV-02-08-ROUTE-SCHEMA-REVIEW-2026-10-09.md)，本机未操作用户或生产数据库。

> 2026-10-09 C6执行完成：[准入审查与C7实测](../reviews/C6-C7-ADMISSION-REVIEW-2026-10-09.md)。12个底座节点按依赖重签，现行 **32 DONE / 9 BLOCKED / 60 PLANNED**；当前Mac21整体PASS。新增只读盘点校验拒绝7类坏输入，修正旧装置静默覆盖重复定义；stable/LLVM实测无branch映射。下方20/21/60均保留为重签前快照。

> 2026-10-09 当前进度入口：[进度与后续开发计划](PROGRESS_AND_NEXT_PLAN-2026-10-09.md)。本轮整改及完整canonical Mac21已整体PASS，正式20 DONE / 21 BLOCKED / 60 PLANNED不变；以下按日期保留的旧日志不作为当前开放故障或外派指令。最新实测及限制见[Codex收口审计](../reviews/CODEX-AUDIT-CLOSURE-2026-10-09.md)，下一轮先做C6差距核对、C7覆盖率方案与C8通知决策准备。

> 用途：给 Codex 总控审阅的**当前状态快照**。回答"截止现在到底做成了什么、什么还没收口、哪里需要你亲自核"。
>
> 本文件**不修改冻结规格、不改节点状态、不构成验收结论**。节点 `DONE` 的判定权在总控与 Human，本文件只做转述和证据登记。
>
> **证据分级**（每条结论都带标记，别混读）：
> - `【实测】` = 本轮（2026-10-03）我实际跑命令拿到的输出，可复现。
> - `【记录】` = 引用既有终审/节点文档的声明，本轮**未**独立复验。
> - `【待核】` = 我发现但无权下结论的疑点，列在第 5 节，请总控裁决。

---

## 0. 快照元信息 `【实测】`

| 项 | 值 |
|---|---|
| 快照时间 | 2026-10-03 10:50 (GMT+8) |
| 仓库 | `/Users/chenzhiqing/Documents/MyVault/协作工作`（TEAMSPACE / AI Team Workspace） |
| HEAD | `1a14b87` — *fix: stabilize hosted CI runners* — 2026-09-23 11:26:17 |
| 已跟踪文件改动 | 22 个（+4145 / −651） |
| 未跟踪文件 | 84 个 |
| 本轮执行者 | Mavis（mavis agent），非本项目默认实现 Harness |

**未跟踪文件分布** `【实测】`：`docs/reviews` 35、`crates/domain` 26、`docs/adr` 4、`scripts/ci` 3、`crates/adapter-supervisor` 3、`adapters/mock` 3、`crates/persistence-sqlite` 2，其余零散 8。

---

## 1. 一句话现状 `【实测】`

**Wave 2 持久化 + 领域层的 8 个节点（DEV-02-11 → DEV-03-07）代码全部躺在未提交的工作区里，测试全绿，但 HEAD 还停在 9-23 的 CI 修复。** 换句话说：可运行的成果全部存在，但没有任何一个字节进入版本历史。

---

## 2. 节点状态（转述，不背书）

节点判定与完整叙事见 [`docs/development/DEVELOPMENT_NODES.md`](../development/DEVELOPMENT_NODES.md) 的「当前执行状态」表与 [`docs/reviews/`](../reviews/) 下的逐份终审。本文件不复述结论，只标出总控审阅时该重点看的几份：

| 节点 | 终审文档 |
|---|---|
| DEV-02-11 状态机 §2/§3/§4 | [WAVE-2-DEV-02-11-STATE-MACHINES-CLOSURE-2026-09-29.md](../reviews/WAVE-2-DEV-02-11-STATE-MACHINES-CLOSURE-2026-09-29.md) |
| DEV-02-12 状态机 §5/§7 | [WAVE-2-DEV-02-12-STATE-MACHINES-CLOSURE-2026-09-30.md](../reviews/WAVE-2-DEV-02-12-STATE-MACHINES-CLOSURE-2026-09-30.md) |
| DEV-02-13 权限矩阵求交 | [WAVE-2-DEV-02-13-PERMISSION-INTERSECTION-CLOSURE-2026-09-30.md](../reviews/WAVE-2-DEV-02-13-PERMISSION-INTERSECTION-CLOSURE-2026-09-30.md) |
| DEV-02-14 乐观锁/幂等/Outbox | [WAVE-2-DEV-02-14-CONCURRENCY-IDEMPOTENCY-OUTBOX-CLOSURE-2026-09-30.md](../reviews/WAVE-2-DEV-02-14-CONCURRENCY-IDEMPOTENCY-OUTBOX-CLOSURE-2026-09-30.md) |
| DEV-03-06 取消超时/重试准入 | [WAVE-2-DEV-03-06-TERMINATION-RETRY-CLOSURE-2026-09-30.md](../reviews/WAVE-2-DEV-03-06-TERMINATION-RETRY-CLOSURE-2026-09-30.md) |
| DEV-03-07 租约续期/fencing | [WAVE-2-DEV-03-07-FENCING-CLOSURE-2026-09-30.md](../reviews/WAVE-2-DEV-03-07-FENCING-CLOSURE-2026-09-30.md) |

DEV-02-15 及后续节点仍为 `PLANNED`。V2 侧 R0 Rescan 已出报告，**R1 架构/Conversation/Gateway/Leader 权限/MCP/迁移策略未冻结，按现行规则不得启动 V2 新语义实现**。

---

## 3. 本轮实测证据

### 3.1 门禁复跑 `【实测】`

| 命令 | 结果 |
|---|---|
| `cargo test --workspace` | **454 passed / 0 failed / 1 ignored** |
| `cargo clippy --workspace --all-targets --all-features -- -D warnings` | **exit 0，零告警** |

454 这个数字与 DEV-03-07 终审记载的「438→454」一致（终审记录为 `454 passed / 0 failed / 1 ignored`，本表原写「0 ignored」，已按实测改正——一个标着【实测】的数字写错，比没有这个数字更糟），即**工作区当前状态与最后一份终审所声称的状态吻合**，中间没有漂移。

> **【后续变更，非本表原始记录】** 上面那张表的两个数字是**当时的快照，现已过时**。
> DEV-03-07 此后经历第七次独立审查（**判定 FAIL**）并返工：原 domain 用例靠**读 `0012` 文本**核对常量，
> 该形态被整体撤除，数据库那半移到 `persistence-sqlite` 用真内存库证明，
> 新增 4 个真库用例。当前实测为 **`458 passed / 0 failed / 1 ignored`**
> （`cargo fmt --all --check` `exit=0`，clippy 零告警）。
> 上句「中间没有漂移」对**该快照时点**成立，**对现在不再成立**。
> 详见 [DEV-03-07 终审](../reviews/WAVE-2-DEV-03-07-FENCING-CLOSURE-2026-09-30.md) §13.2/§13.3。

> ⚠️ 本轮**未**执行：`pnpm test:all`（TS 侧 unit/integration/contract/e2e/workspace）、`pnpm lint:ts`、`pnpm typecheck`、`cargo fmt --check`、`contracts:check`、`licenses:check`、`unsafe:check`、`verify:guards`、macOS ARM64 完整门禁。**TS 侧与门禁脚本本轮状态未知**，不能因为 Rust 侧全绿就推断整体通过。

### 3.2 迁移盘点 `【实测】`

位置：`crates/persistence-sqlite/migrations/`（**注意：仓库根的 `migrations/` 是空壳，只有 `.gitkeep`**，别看错地方）。

共 **22 个 SQL、185 个 `CREATE TRIGGER`**。逐文件：

| 文件 | 行数 | trigger | 文件 | 行数 | trigger |
|---|---|---|---|---|---|
| 0001 | 84 | 0 | 0012 | 317 | 11 |
| 0002 | 185 | 2 | 0013 | 347 | 21 |
| 0003 | 105 | 1 | 0014 | 478 | 45 |
| 0004 | 180 | 0 | 0015 | 226 | 12 |
| 0005 | 103 | 0 | 0016 | 126 | 3 |
| 0006 | 156 | 2 | 0017 | 402 | 18 |
| 0007 | 232 | 2 | 0018 | 145 | 6 |
| 0008 | 176 | 0 | 0019 | 365 | 21 |
| 0009 | 137 | 0 | 0020 | 322 | 16 |
| 0010 | 79 | 5 | 0021 | 232 | **9** |
| 0011 | 92 | 5 | 0022 | 245 | 6 |

`0009` 是 137 行 / **0 trigger**，与 DEV-02-09 终审「`0008` 同为零 trigger 触发本案」的记录同构——这类"零 trigger 裸表"是本项目反复出事的形态，建议总控重点复查是否还有同类。

### 3.3 领域层规模 `【实测】`

`crates/domain/src/` 16 个文件，`run_fencing.rs`（19,715 B）**为本节点新增、不是最大**（`run_state_machine.rs` 31,024 B、`message_state_machine.rs` 30,215 B 都更大）。测试侧 `crates/domain/tests/` **14** 个文件 + `crates/persistence-sqlite/tests/` **23** 个前向/升级迁移测试文件。

> 本节原写「13 / 25 / 最大」，三处均与实测不符，已改。该文件把这类数字标为 `【实测】`，而同一文件里连续错了四处——**一个标着「实测」的数字写错，比没有这个数字更糟**。

---

## 4. 需要总控优先处置的风险 `【实测】`

### 4.1 未提交规模（最高优先）

HEAD 停在 2026-09-23，而 DEV-02-07 → DEV-03-07 的**全部**实现、测试、终审文档、ADR、证据索引都未进入版本历史。后果：

- 任何"回滚到干净状态"操作会一次性丢掉 10 天工作量。
- 无法用 `git bisect` / `git diff` 做回归定位。
- CI 从未见过这批代码，`.github/workflows/ci.yml` 本身也是 modified 状态。

**建议**：在继续任何新节点前，先做一次带完整 message 的提交（或分节点提交），让工作区有可回退点。这是我的建议，不是已执行动作——按 `AGENTS.override.md`，提交需 Human 明确授权，我没有动。

### 4.2 冻结规格曾被工具链改动破坏 `【记录】`

DEV-03-06 终审记载：**「冻结在第二轮再次被破（14:10:13 五个文件，改动者即本会话的 `cargo fmt --all`）」**。即 `cargo fmt --all` 改动了受冻结约束的文件。请总控确认当前工作区是否已重新对齐冻结快照，以及是否有机制阻止格式化工具再次踩线。

### 4.3 三次"自证"事故模式 `【记录】`

DEV-03-06 终审把 `ALL_HAS_NO_DUPLICATES` 恒真断言定性为**「本项目第三次『自证』事故」**（测试用被测代码自己推导期望值，装置与被测对象同源时给出假绿）。DEV-02-14 也记了「预算测试注入『忽略预算』时仍 23/23 全过——它自己算 `spent`、根本没经过被测函数」。

**这是本项目最值得总控独立复核的系统性风险**：不是某条断言写错，而是"用同源数据验证自己"这个模式已经复发三次。已有 Q-04～Q-13 机检装置应对，但装置自身也曾失败过两次（Q-12 假绿、Q-13 漏剥前缀）。

---

## 5. 请总控核对的疑点 `【待核】`

我无权下结论，但下面几条是我核对时发现的不一致：

1. **0021 trigger 计数三处不一致。**
   `DEVELOPMENT_NODES.md` 与 `coordination/README.md` **当时**均写「`0021`（7 trigger）」，而终审文档 [WAVE-2-DEV-02-10-GAP-REMEDIATION-2026-09-29.md](../reviews/WAVE-2-DEV-02-10-GAP-REMEDIATION-2026-09-29.md) 头部写「**9 个 trigger**」，实测文件也是 **9 个**。
   **已修（11:04）**：两份汇总表与源头终审 `GAP-REMEDIATION:44/77` 均已改为 9，并补上 P4 新增的两条 trigger 名。
   我的推断：7 是 P4 修复**前**的数字——该终审第 177 行记载 P4 后新增了 `approvals_permission_effect_insert` / `_update` 两个，7+2=9。**即汇总表没跟上修复，是文档漂移而非 SQL 缺陷。** 请确认。

2. **台账「正式 = 7 trigger」这条断言的可复现性。**
   同一份终审第 74 行要求装置断言「影子 = 0 trigger / **正式 = 7 trigger**」。**已查**：该断言文本在 `GAP-REMEDIATION:77`，原文写 7，与实际的 9 冲突；已随标题一并改为 9。**建议总控亲自跑一次 `build_base` 相关测试确认。**

3. **变异台账在仓库外。** `run-all.sh` + `patches/m*.py` 明确标注为「仓库外装置」，位于 `~/.minimax/teamspace-audits/dev-02-08/evidence/`。DEV-03-07 声称「9 个行为变异全杀」，但**全仓 0 个 `.py` / 0 个 `.sh` 文件**。总控审阅时必须连同仓库外路径一起取证，否则该结论无法复现。

4. **证据台账目录名与内容不匹配。** `dev-02-08/evidence/` 实际存放了 dev0211 / dev0212 / dev0213 / dev0214 / dev0306 / dev0307 全部 6 份 GATE-LOG。目录名会误导检索，建议重整或加索引文件。

5. **仓库内证据目录仅一份。** `docs/reviews/evidence/` 下只有 `wave2-mac-2026-09-28/`，而最新工作停在 09-30。09-29 之后的证据**全部只在仓库外**。

---

## 6. 已登记但未闭环的项（需 Human 或总控裁决）`【记录】`

以下均为既有终审主动登记、**至今未见闭环记录**的项：

| 来源 | 未闭环内容 |
|---|---|
| DEV-03-07 | **heartbeat deadline 不新建列——明确标注「待裁决」**。§3 要求「设置/恢复 heartbeat deadline」，但 `0001`～`0022` 无任何 heartbeat 列，当前按 `leases.expires_at` 派生量处理。 |
| DEV-03-07 | `LEASE_BEARING_STATUSES` **生产零调用点**：已终态 Run 出示当前 token 时 `Completion` 仍放行。本节点不擅自补耦合，**建议归 DEV-03-08**。 |
| DEV-03-07 | `RUN_LEASE_EXPIRED` 裁决为不进 `RunErrorCode`，按 `INTERNAL_UNEXPECTED` 先例作独立形状携带冻结线名。 |
| DEV-02-10 | **「Human 接收者不伪装成 Role」在数据库层未闭环**（四项规格缺口之一）。 |
| DEV-02-09 | 四项规格缺口已回报 Human，其中「Human 身份的 DB 层落地」相关项未闭环。 |
| DEV-02-11 | `STATE_MACHINES §2` ASCII 图缺 `CANCELLED` 节点，**未擅自改冻结规格**，登记待裁。 |
| DEV-02-14 | `0006` 的 `max_retries DEFAULT 5` 与 §11 的 3 冲突；`0006` 已发布不可改，用 `assert_ne!` 把冲突钉死。 |
| DEV-02-07/08/09/10 | 四者均以**同模型第二遍对抗性反例审查替代独立复核**，降级已显式登记。**Wave 2 至今没有真正的跨模型独立复核。** |
| 全局 | 按 [ADR-0005](../adr/0005-macos-first-quality-and-release-gates.md)，关闭节点需**最终输入的** macOS ARM64 完整门禁 + 全 refs 历史秘密扫描 + 工具来源证据 + 独立审核 + 总控结论。旧 21/21 双环境结果只属历史快照。 |

---

## 7. 复现入口

```bash
cd /Users/chenzhiqing/Documents/MyVault/协作工作

# 本轮跑过的
cargo test --workspace
cargo clippy --workspace --all-targets --all-features -- -D warnings

# 本轮未跑、总控审阅时建议补齐
pnpm run test:all && pnpm run lint && pnpm run typecheck && pnpm run fmt:check
pnpm run contracts:check && pnpm run licenses:check && pnpm run unsafe:check && pnpm run verify:guards
```

**仓库外证据台账**（不在版本控制内，审阅时需显式访问）：

```
~/.minimax/teamspace-audits/dev-02-08/evidence/
├── GATE-LOG-dev0211-state-machines.md
├── GATE-LOG-dev0212-state-machines.md
├── GATE-LOG-dev0213-permissions.md
├── GATE-LOG-dev0214-concurrency.md
├── GATE-LOG-dev0306-termination-retry.md
├── GATE-LOG-dev0307-fencing.md
├── VOIDED-*.json          # 作废证据（格式自伤）
└── SUPERSEDED-*.json      # 被后续轮次取代的证据
```

> `VOIDED-*` / `SUPERSEDED-*` 的存在说明证据卫生是**在用**的（装置自伤后主动作废并留痕），这是好信号；但也说明同一装置历史上失败过不少次，读结论时要连作废件一起看。

---

## 8. 本轮明确没做的事（边界声明）

- **没有**修改任何冻结规格、ADR、节点状态或 `DONE` 判定。
- **没有**提交、推送、部署任何内容（`AGENTS.override.md` 要求 Human 明确授权）。
- **没有**运行 TS 侧测试与门禁脚本，因此**不对 TS 侧状态作任何声明**。
- **没有**删除或移动任何文件（包括 3 个 `D` 状态的已跟踪文件：`PRD-Figma审计报告.md`、`design/审计整改说明.md`、`开发修改清单.md`——这三个是删除后仍显示为 modified 的孤儿条目，**其删除动作是谁做的、是否有意，请总控一并核实**）。
- **没有**独立验证任何终审文档里的对抗攻击数字（35/36/43/61 条等），那些是 `【记录】` 级引用。
- **没有**在第 5 节疑点上表态，那些留给总控裁决。

---

## 9. 日志维护约定

- 本文件按**快照**追加，不覆写历史结论；发现旧结论被推翻时，新条目写明"推翻第 N 节的哪条、依据是什么"，不静默删改。
- 任何 `【实测】` 降级为 `【记录】` 或撤销时，必须写明原因。
- 本文件不替代 `DEVELOPMENT_NODES.md` 的状态表——两者冲突时**以状态表为准**，并在本文件登记该冲突。


## 10. 2026-10-07 总控纠正与整改登记

- 【实测/口径纠正】报告加入前展开 `git status --untracked-files=all`：19 modified、3 deleted、170 untracked，共192文件。原第0节84是折叠路径条目，不是文件数量。
- 【实测】10-07初次复跑 Rust471 passed/0 failed/1 ignored；TS37 passed；门禁脚本12 passed。这是整改前快照，整改后以新验收证据为准。
- 【纠正】第1节“DEV-02-11→03-07 的8节点”实际为4个DEV-02+2个DEV-03，共6。第6节“Wave2至今没有跨模型独立复核”范围过宽：09-28前置与三批终审有Terra独立复核；后续MiniMax节点的同模型降级单独记录。
- 【当前状态】02-08/09/13/14、03-06/07 BLOCKED。Human已授权整改；方案与互斥写集在REMEDIATION-PLAN-2026-10-07.md。历史454/458/471及旧终审均保留为相应时点事实，不能冒充整改后输入。
- 【待验收】上一轮发现已形成5条真实失败回归；权限、审批实际身份/scope、缓存时间、租约世代、Outbox deadline与guard按计划修复。运行时原子事务、Outbox消费/恢复、最终写入fencing及Notification缺口仍需对应节点完整验收。


## 11. 2026-10-08 持续整改登记

- 【实测】DEV-02-09 新前向 0023 修复 Role/Human 通知身份碰撞、底层事项未解决即关闭、同 Team 来源偷换和 Notification 删除绕过；保留 0001～0022 字节，22 个哈希与前轮相等。
- 【实测】新增真库 14 个测试；Rust 全工作区 494 passed / 0 failed / 1 ignored，build、clippy、fmt check 通过；15 个守卫变异由行为断言捕获，健康基线通过，构造/编译故障不计分。
- 【纠正】09-29 DEV-02-09 把删除 Notification 判作非漏洞，与 DATA_CONVENTIONS §6 的事实保留要求不符；新轮保留反例与修复证据。0023 初稿按当前源状态拒绝历史 RESOLVED 也不正确：旧 Task 可再次受阻，已移除误伤检查。
- 【当前状态】节点仍 BLOCKED；Owner 身份执行点、策略解释/投递和应用事务链尚未闭环。ADR-0007 为 Proposed，已向 Human 提交具体补充语义，不将未回复解释为接受。
- 【证据】[本轮验收记录](../reviews/GOAL-REMEDIATION-2026-10-08.md)；当前完整 Mac 21 项与覆盖率未验证，不替代旧历史证据。没有提交、推送、部署或外部服务操作。


## 12. 2026-10-08 Run 回调规则及文档复核

- 【实测】DEV-03-07 补零世代、Run 终态、UNKNOWN 普通回调拒绝；API 必须提供 Run 状态，旧 token 拒绝优先，启动失败路径保留。Source/lease/权限完整执行点仍未接入。
- 【实测】Rust 499 passed / 0 failed / 1 ignored；fencing 专项 25/25；7 个行为变异全捕获；严格 workspace rustdoc 通过，12 个断链修复。Luna 仅改六份文档链接，主控按旧哈希和精确 diff 验证仅注释。
- 【证据异常】临时验证目录消失，原因未确认；后续日志直接保存到仓库，所有计分结果重新执行。旧源码按上一轮 SHA-256 验证后重放，零世代三条回归真实失败；不冒充遗失历史日志。
- 【当前状态】相关节点 BLOCKED，目标 ACTIVE，ADR-0007 与此前外部服务授权仍待答复。详见 [Run 回调验收](../reviews/RUN-CALLBACK-ADMISSION-REVIEW-2026-10-08.md)。


## 13. 2026-10-08 Outbox 数据库默认重试值收口

- 【实测】DEV-02-07 的省略列 INSERT 在修复前实际写入 5，冻结要求为 3；前向 0024 已修复。旧 DEFAULT 5 与新值不同的 assert_ne 不构成修复，现改为真实数据库行为验证。
- 【实测】升级保留 13 列、五种状态、不同历史重试配置及错误诊断；保留 3 个命名索引/4 个守卫；失败迁移原子回滚并保留诊断数据。4 个专项测试和 8/8 行为变异通过。
- 【实测】Rust 503 passed / 0 failed / 1 ignored；build、clippy、fmt check、严格 rustdoc 和工作树 Gitleaks 通过。23 份旧 migration、Rust manifest/lockfile 与上一快照相同；默认值注释修正的纯文档差异另有证明。
- 【当前状态】DEV-02-07 当前 BLOCKED，原有六个整改节点继续 BLOCKED。历史终审保持；当前输入完整 Mac 21 项、覆盖率及真实应用执行链尚未验收，ADR-0007 和外部服务授权待答复。
- 【证据】[本批验收](../reviews/OUTBOX-DEFAULT-REMEDIATION-2026-10-08.md)及仓库内 capsule；没有提交、推送、部署或外部服务操作。


## 14. 2026-10-08 深入审计与Minimaxcode接续

- 【实测】5个Terra审查分片、1个Luna机械盘点、总控二次反例；299个范围文件、101节点。代码/24迁移/manifest/lockfile在检查期间无变化，当前统一Rust503/0/1、TS36、门禁脚本12；24项离线检查均exit0，不是完整在线Mac21。
- 【实测】当前SQLx bundled SQLite新增回归：1合法对照通过、3行为断言失败（跨Team DRAFT、Run回QUEUED、五类无引用历史DELETE）；脱敏1对照通过/2失败；值对象补充2通过。装置初始列名/字段顺序错误未计分。
- 【纠正】总控旧TS计数37/38不正确，原日志逐组实际35/36，当前36。保留旧日志/报告，本条覆盖其汇总口径；不归因MiniMax。子报告对“无DELETEguard”和“取消为PENDING唯一出口”的表述也被收敛：已有条件Event/Reference guard，PENDING可批准/拒绝/过期/取消；Attention派生索引不自动按永久核心事实处理。
- 【当前状态】23 DONE /18 BLOCKED /60 PLANNED。新直接阻断01-05/08、02-05/06/10、03-03，依赖暂停02-11/12、03-02/04/05；纯机/Mock成果保留。覆盖率、在线Mac21、业务E2E及ADR0007未关闭。
- 【授权/交接】Human明确Minimaxcode接续，ADR0008仅记录该路由；已签发9个整改/方案/验收包和60节点条件计划，Human转发，总控不直接联系外部模型。节点DONE仍由Codex裁定。
- 【入口】[深入审计](../reviews/DEEP-AUDIT-2026-10-08.md)、[Minimaxcode总交接表](MINIMAXCODE-TASKS-2026-10-08.md)。未改业务代码、旧迁移或生产数据，未提交/推送/部署/联网。


### 第14节覆盖率追加实测

未下载工具，复用现有Apple LLVM21，经RustLLVM22实际probe后测量同字节domain/application源；249个instrumented用例通过，domain16份src的1907/2197行为86.80%，**未达90%**。application没有可执行行，不报100%；region84.75%不是branch，分支85%及全体80%仍未验证。较前述“覆盖率未测”已有进展，详见本批coverage-tools/明细。

覆盖率追加后新增MM-R10；总交接表当前为10个整改/方案/验收/测试包及60节点条件计划，原先9包为该追加前快照。


## 2026-10-08 Minimax十包回传独立审计

- 本批不准入，继续整改；[审计](../reviews/MINIMAX-RETURN-AUDIT-2026-10-08.md)与[接续任务表](MINIMAXCODE-FOLLOWUP-2026-10-08.md)生效。新增DEV-02-02/03/04留存阻断，20 DONE / 21 BLOCKED / 60 PLANNED。
- 独立Rust合计591 passed / 14 failed / 1 ignored；TS36/0、Gate22/0；lint仍1错；当前domain行2016/2197=91.76%，branch未验证。终态/脱敏真反例使绿单测不能关闭行为缺陷。
- artifacts原审计INSERT实为12值，原SQLx RED新target回放1/3仍包含五表；Minimax“11值不可复现”指控不成立。
- 旧迁移0001–0024、manifest/lock、旧审计301产物字节不变。无提交/推送/部署/外部服务。ADR0007与联网Mac最终验收仍待Human，未放行应用执行链或V2新语义。


## 2026-10-09 FC九包独立审计

- [FC审计](../reviews/FC-RETURN-AUDIT-2026-10-09.md)及[三包接续](MINIMAXCODE-AUDIT-FOLLOWUP-2026-10-09.md)生效。六对象留存/Run终态原缺口局部关闭，FC04单层%25新反例使整批准入仍BLOCKED，正式统计20/21/60未变。
- Rust643/0/1 ignored、TS36/0、Gate25/0、replay8/0、当前domain91.76%，branch未验。主控精确归档一旧mjs后全仓lint0，active及同目录邻居反例仍被拒；工程只改eslint配置一处。
- 原M14 E0004不能算断言检出：原声明15项按意图重建＋1原写法未证；新16项含M14-R实跑全杀，137文件与80rs还原0差异。
- 当前SQLx对三处DROP分别失败注入证明完整schema回滚，正常概念迁移保留0028/0029；原无事务PythonPOC不能用作正式执行器。存量联合验收与通知B项另待准入。
- 旧证据只读；无提交/推送/部署/联网/生产操作。
