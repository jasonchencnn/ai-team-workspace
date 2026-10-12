# DEV-02-11 Task/Run/Binding 纯状态机 —— 第一轮终审（2026-09-29）

> 结论：**DONE（第一轮：Task 状态机）**。依据 [DEVELOPMENT_NODES](../development/DEVELOPMENT_NODES.md) 节点专属标准、[QUALITY_GATES](../development/QUALITY_GATES.md) G0～G9、[STATE_MACHINES](../specs/STATE_MACHINES.md) §2 冻结迁移表、[TECH_STACK_AND_STANDARDS](../development/TECH_STACK_AND_STANDARDS.md) §5 状态机标准与 [ADR-0005](../adr/0005-macos-first-quality-and-release-gates.md)。当前验证平台为 macOS ARM64。
> **本记录只判定 §2 Task 状态机。Run（§3）与 Binding（§4）状态机未实现，见「未完成部分」。**

---

## 审核分工与模型路由

- **执行模型**：`minimax/MiniMax-M3.1-Flash-Preview`，全程未调用其他模型。
- **模型路由**：Human 于 2026-09-29 授权本模型暂代 ChatGPT/Codex，并授权「后续裁决按倾向来，做好记录，往下做」。该授权是对 `AGENTS.override.md` 与 ADR-0004 的一次显式 Human 例外。
- **独立性降级**：本节点以**表驱动测试 + 对抗审查 + 性质探针**替代独立审核方签署，满足 `QUALITY_GATES.md` §1 完成定义第 9 条第二条路径，**不等同于独立审核人签署**。

## 范围

| 文件 | 变更 |
|---|---|
| `crates/domain/src/task_state_machine.rs` | 新增（648 行） |
| `crates/domain/src/lib.rs` | 注册模块并 re-export |
| `crates/domain/tests/task_state_machine.rs` | 新增 15 个表驱动用例 |

**无迁移变更。** 本节点是纯领域层实现，不触碰 `persistence-sqlite`。

## 编译期事实（由 `count` 探针直接读常量表，非 grep）

| 量 | 值 |
|---|---|
| `TASK_EDGES.len()` | **14**（§2.1 生命周期边） |
| `TASK_CREATIONS.len()` | **2**（`创建 → TODO`、`创建 → READY`） |
| `TaskStatus::ALL.len()` | **7** |
| 表内不同的 §2.1 拒绝码 | **10** |
| 不同事件名 | **12** |
| 不同前置条件 | **12** |

> 本轮**第一次把「能脚本计数的量先跑计数命令」变成编译期事实**而不是 grep。
> 更重要的是：`TASK_EDGES.len() == 14` 与 `TASK_CREATIONS.len() == 2`
> 已由测试**硬断言**，表被误删一行就会红 —— 比任何文档里的数字都可靠。
> 第一版我用 `grep -c 'TaskEdge {'` 数出 15、`TaskCreation {` 数出 4，都把 `impl` 块数了进去。

## 分层裁决：状态机管边，事务管前置条件

`TECH_STACK_AND_STANDARDS.md` §5 同时写了两句话：

- 「状态迁移实现为纯领域函数：输入当前状态、命令、上下文，输出新状态或稳定业务错误码」
- 「Task 领取在同一事务中校验状态、依赖、Binding、权限、并发上限并创建 lease/fencing token」

第二句里「依赖、Binding、权限、并发上限」全都要读库，纯函数做不到。**裁决**：

- `domain::transition(status, command) -> Result<TaskTransition, TaskRejection>` 判定 §2 图上有没有这条边、这条边带哪个冻结拒绝码；
- §2.1 的「前置条件」一列以 `TaskPrecondition` 描述出来，交给 application 层在事务里验；
- **第三参数「上下文」不进签名** —— 进了就必须有 I/O，那就不是纯函数了。

**零状态副作用在类型上如何保证**：`Err` 分支返回的 `TaskRejection` 只带 `current` / `command` / `code`，
**没有任何「新状态」字段**；没有 `Partial` 值，也没有「改了但标记一下」的路径。

## 对抗审查

本轮攻击对象不是 SQL 触发器，而是**类型系统与常量表**，形态与前四轮完全不同：

| | DEV-02-07～10 | 本轮 |
|---|---|---|
| 攻击面 | SQL 触发器 | 类型 + 常量表 |
| 「拦截」怎么表达 | `sqlite3` 返回非 0 | **`rustc` 编译失败** |
| 装置自检 | 种子行数断言 | 期望 `compile-fail` 的探针必须真的编译失败 |

### Q-01～Q-11：规格逐字核对（42/42 通过）

`ERROR_CATALOG.md` 原文：「错误码是协议契约；改名或改变语义属于破坏性变更」。
本轮用到的 **13 个错误码、12 个事件名、7 个状态名、10 个命令名全部逐字对照冻结规格**，
**没有一个是发明的**。这是 DEV-02-07 以来第一次把「不发明语义」变成**可机检的断言**。

### 性质探针（8/8 通过）

单元测试已做 7 状态 × 10 命令 = 70 组合的全对遍历，状态边覆盖完整，因此探针专打单测打不到的东西：

| # | 探针 | 证明 |
|---|---|---|
| Q-03 | 改写 `TASK_EDGES[0].to` | 应编译失败 → 迁移表是 `const`，调用方改不了 |
| Q-04 | `let mut t = …; t.to = Done;` | 应编译失败 → 决策不可被调用方改写 |
| Q-05 | 访问 `TaskRejection.to` | 应编译失败 → 拒绝里没有「新状态」 |
| Q-06 | 调用 `t.apply()` | 应编译失败 → 状态机不暴露任何副作用入口 |
| Q-07 | 70 组合 × 1000 轮 | 确定性 + 迁移表不被自身改动 |
| Q-08 | 10 万步随机游走 | `CANCELLED` 是吸收态；`DONE` 唯一出边是 reopen |
| Q-09 | 错误码稳定性 | 「无边」恒为 `DOMAIN_INVALID_TRANSITION` 且不宣告具体码；「有边」必带 §2.1 的具体码 |
| Q-10 | 创建旁路 | 只能创建到 `TODO` / `READY` |

## 本轮抓到的真缺陷

### Q-04：`TaskTransition` 的输出可被调用方改写

**发现路径**：Q-04 第一版我把向量写错了 —— 写的是「移动 + 读取」，那是合法操作，于是报 BAD。
**没有直接改对，而是先查它为什么 BAD**，按正确形式重写后得到：

```rust
let mut t = transition(TaskStatus::Ready, TaskCommand::Claim).unwrap();
t.to = TaskStatus::Done;   // 编译成功
```

**后果**：application 层只要这样写，就能持久化一个状态机从未批准的状态 ——
「READY 直接跳 DONE」，连 `task.complete` 的免 Review 前置条件都不经过。
整台状态机被绕过，**且从代码上完全看不出来**（一次普通的字段赋值）。

**根因**：`TaskTransition` 字段是 `pub`。在 Rust 里「`pub` 字段 + `Copy`」等于「可随意改的值」，
我把它当成了「只读的数据描述」。

**处置**：`TaskTransition` / `TaskRejection` / `TaskCreation` 三类**输出类型**字段改为私有，
配 `const fn` 只读访问器。`TaskEdge` 保持 `pub` 字段 —— 它是表本身而非决策，
且只出现在 `&[TaskEdge]` 里，外部无法就地改。Q-04/Q-05 随即由「编译成功」变为「编译失败」。

**元教训**：纯函数返回「不可变值」这件事，Rust 不会替你保证。
「状态机是纯的」是**运行时**性质，**调用方能否改坏它的输出**是**类型**性质，两者必须分别测。
写探针时：**一条向量报 BAD，先假设向量错，再怀疑实现。**

## 规格缺陷登记（未擅自改冻结规格）

### §2 的 ASCII 图缺少 `CANCELLED` 节点

Q-09 核对状态名时 4 个状态判 FAIL。查证发现不是实现错，是我检测模式太严
（规格里这些状态是代码块内裸词，无反引号）。但顺藤摸到一个真问题：

**`CANCELLED` 在 §2 的 ASCII 图里出现 0 次。** 图上那条 `┌──cancel──` 横线没有画到任何节点：

```text
TODO ──ready──> READY ──claim──> DOING ──submit──> REVIEW ──approve──> DONE
 │                │                │                  │                 │
 └──cancel────────┴──cancel────────┼──cancel──────────┴──cancel         └──reopen──> READY
                                   └──block──> BLOCKED ──resolve──> READY
```

而 §2.1 表里有 **5 条 cancel 边**。**只读图的实现者会漏掉全部 5 条。**

本节点实现以 §2.1 表为准（14 条边），并在装置里加 **Q-11** 显式记录该差异，
让「图与表不一致」成为每次审查都看得到的事实，而不是一次性发现。
是否补图节点属 `STATE_MACHINES.md` 的改动，**登记为遗留，不擅自修改冻结规格**。

## 验证结果

| 项 | 结果 |
|---|---|
| `cargo test --workspace` | **245 passed / 0 failed**（DEV-02-10 收口时 230，本轮 +15） |
| `cargo clippy --workspace --all-targets` | 零告警 |
| `cargo fmt --all --check` | 零差异 |
| 表驱动用例 | 15 个（含**反向**检查：表中有而规格无的边会被抓出来） |
| 规格逐字核对 | 42/42 |
| 性质探针 | 8/8 |
| 门禁（macOS ARM64，21 项） | 见文末「门禁证据」 |

## 未完成部分

本节点验收标准为「**全部迁移有表驱动与性质测试；非法迁移零状态副作用**」，
覆盖 Task、Run、Binding 三个状态机。本轮**只完成 Task**：

| 状态机 | 规格 | 状态 |
|---|---|---|
| Task | `STATE_MACHINES` §2 | **本轮完成** |
| Run | §3 | 未开始 |
| Binding | §4 | 未开始 |

拆分理由：三个状态机合计 5 张表、约 40 条迁移边，强行一轮做完会让每部分都做不深。
本轮已把可复用的东西建好 —— 表驱动模式、性质探针、规格逐字核对装置，
后两个状态机可直接套用。**DEV-02-11 尚未整体关闭。**

## 遗留

1. **Run（§3）与 Binding（§4）状态机** —— 第二轮，依赖 `00-03`、`02-02～05`，均已 `DONE`。
2. **`STATE_MACHINES §2` 的 ASCII 图缺 `CANCELLED` 节点** —— 规格侧未改。补图或标注「以 §2.1 表为准」由 Human 决定。
3. **`TaskPrecondition::condition_key()` 的具体取值是我定义的** —— 规格只规定 `DOMAIN_PRECONDITION_FAILED` 的 details 用 `condition_keys` 白名单，未规定各键取值。属派生常量，已在代码注释中标注。
4. **application 层的事务实现未开始** —— `crates/application/src/lib.rs` 仍为空。本节点交付的 `preconditions()` 描述需由它消费。
5. Human 身份仍无 DB 层落地（沿用既有裁决）。
6. `adapter-supervisor::node_mock_adapter_fault_crash` 并行下偶发失败，沿用既有登记。

## 门禁证据

| 文件 | 说明 |
|---|---|
| `~/.minimax/teamspace-audits/dev-02-08/evidence/wave2-macos-arm64-dev0211-task-sm.json` | 本轮门禁 |
| `~/.minimax/teamspace-audits/dev-02-11/JUDGMENT-LOG.md` | 判断日志 J26～J30 |
| `~/.minimax/teamspace-audits/dev-02-11/q01-q02-spec-codes.sh` | 规格逐字核对装置（42 项 + Q-11 图/表差异） |
| `~/.minimax/teamspace-audits/dev-02-11/run-q-probes.sh` | 性质探针装置（8 项） |
| `~/.minimax/teamspace-audits/dev-02-11/work/count/` | 编译期计数探针 |
