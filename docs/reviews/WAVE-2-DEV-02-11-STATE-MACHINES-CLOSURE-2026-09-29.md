# WAVE-2 DEV-02-11 终审：Task/Run/Binding 纯状态机

- 节点：`DEV-02-11` 实现 Task/Run/Binding 纯状态机
- 依赖：`00-03`、`02-02`～`02-05`
- 冻结来源：`docs/specs/STATE_MACHINES.md` §2 / §3 / §4、`docs/specs/ERROR_CATALOG.md`、
  `docs/development/TECH_STACK_AND_STANDARDS.md` §3 §5 §9.1
- 审核模型：`minimax/MiniMax-M3.1-Flash-Preview`（未使用其他模型）
- 裁决：Human 已授权「按你倾向来，做好记录，往下做」
- 结论：**DONE**

---

## 1. 交付物

| 文件 | 行数 | 内容 |
|---|---:|---|
| `crates/domain/src/task_state_machine.rs` | 668 | §2 Task：7 状态 / 14 边 + 2 创建目标 |
| `crates/domain/src/run_state_machine.rs` | 720 | §3 Run：8 状态 / 17 边 + 1 创建目标 |
| `crates/domain/src/binding_state_machine.rs` | 586 | §4 Binding：5 状态 / 7 边 + 1 创建目标 |
| `crates/domain/tests/task_state_machine.rs` | 532 | 16 用例 |
| `crates/domain/tests/run_state_machine.rs` | 900 | 21 用例 |
| `crates/domain/tests/binding_state_machine.rs` | 680 | 19 用例 |

三机合计 **56 个表驱动用例**、**38 条边**、**4 个创建目标**。

注册于 `crates/domain/src/lib.rs`；`transition` / `rejection_code` 按机别名导出
（`transition` / `run_transition` / `binding_transition`），避免三机同名冲突。

## 2. 分层裁决

`domain::transition(status, command)` 是**纯函数**：无 I/O、无时钟、无第三参数。

规格 §5 第二句已把前置条件的求值位置定死——依赖、Binding、权限、并发上限
都要**在事务里查**。因此状态机只管「边是否存在、带哪个冻结拒绝码」，
`RunPrecondition` / `BindingPrecondition` 只**描述**不**检查**，
由应用层在同一事务内验证。

`condition_key()` 的具体取值是**派生常量**：规格只规定用目录的
`condition_keys` 白名单做 details，从未规定条件标识符本身。

## 3. 对抗审查抓到的真缺陷

全部为本轮新发现，均已修复并加装防回退断言。

| 编号 | 缺陷 | 危害 | 修复 |
|---|---|---|---|
| Q-04 | `TaskTransition` 字段 `pub` | 调用方可改掉状态机决策，绕过 `task.complete` 的免 Review 前置条件 | 三类输出类型字段私有化 + `const fn` 访问器 |
| Q-05 | `ADAPTER_UNAVAILABLE` 漏出 retryable 集合 | 目录标 `是`，实现标否 → UI 把可重试的 Adapter 抖动呈现为硬失败 | 按目录表重读，改为 3 个 |
| Q-06 | `(from, command)` 不唯一 → `transition` 静默选中 `UNKNOWN → RUNNING` | 调用方已证明 Run 死亡，却被记成仍在运行，且不报错 | 新增 `ReconcileOutcome`，把证据提升进命令类型，歧义在类型上不可表达 |
| Q-07 | `BindingEvent` 变体名带 `Run` 前缀 | serde 序列化成 `RUN_BINDING_CHANGE_REQUESTED`，`as_str()` 是 `BINDING_CHANGE_REQUESTED`——同一事件两条线名 | 变体名改为 §4 拼写的 Rust 形式，使两者结构性相等 |
| Q-08 | **30 个错误码的 serde 线名全错** | `TaskErrorCode` 13 个中 9 个、`RunErrorCode` 9 个中 4 个、`BindingErrorCode` 8 个中 3 个丢掉 `DOMAIN_`/`AUTH_`/`TASK_`/`REVIEW_` 前缀，产出目录里不存在的线名 | 三个 `*ErrorCode` 每个变体加显式 `#[serde(rename = "<冻结码>")]` |

**Q-08 波及第一轮已过门禁的 Task**，且第一轮门禁 21/21 对它无能为力——
当时测试只断言 `as_str()`，从未断言 serde。**门禁全绿不等于协议正确。**

## 4. 机检装置

| 装置 | 覆盖 | 结果 |
|---|---|---|
| Q-01/Q-02 规格逐字核对 | 42 项（第一轮） | 42/42 |
| 性质探针 | 8 项（第一轮） | 8/8 |
| **Q-04** `q04-catalog-retryable.py` | 三机每个错误码的「目录存在性 + retryable 一致性 + 跨机变体名一致性」 | 67/67 |
| **Q-05** `q05-serde-wire-names.py` | 三机 10 个枚举逐变体「serde 实际线名 == `as_str()` 冻结线名」 | 81/81 |

Q-04 抓到跨机变体名分叉（`TASK_ALREADY_CLAIMED` 在两机曾叫
`AlreadyClaimed` / `TaskAlreadyClaimed`；`RUN_CANCEL_NOT_CONFIRMED` 在 Binding 侧
曾叫 `CancelNotConfirmed`）。Q-05 抓到 Q-07/Q-08。

装置自身在 Q-04 演进中失败三次、Q-05 报假红一次，**全部是装置错**，
按 J27 先查探针再动实现，详见 `~/.minimax/teamspace-audits/dev-02-11/JUDGMENT-LOG.md` J35/J38。

## 5. 刻意的「不建模」

四处，均在代码注释写明「等什么条件出现才该做」，防止被当遗漏顺手补上：

| 不建模的东西 | 理由 |
|---|---|
| §2/§3/§4 的 **Actor/触发者** 列 | 权限在事务里查（§5 分层）。自造 actor→权限映射等于自造权限语义 |
| `RunCommand` / `BindingCommand` 的 `as_str()` | §2 给了动作词表（`task.claim` 等即冻结权限动作串），§3/§4 只有触发者。写 `run.*`/`binding.*` 就是自造协议 |
| `RunCommand` / `BindingCommand` / `ReconcileOutcome` 的 serde | 同上。带 payload 的新元组变体会序列化成 `{"RECONCILE":...}`，等于默认冻结一个规格里没有的线格式 |
| Binding 的 `UNBOUND` 状态 | §4 明文「不创建 `UNBOUND` 伪记录」；未绑定是「无 ACTIVE Binding」的**缺席**，不是状态 |

## 6. 派生 vs 冻结

| 位置 | 性质 |
|---|---|
| §3 表 15 行中 5 行拒绝码为 `—` | **派生** `DOMAIN_PRECONDITION_FAILED`（目录通用前置条件失败码），已在代码与测试中标注 |
| §3 末行 `UNKNOWN→FAILED/TIMED_OUT/CANCELLED` | **冻结**，§3 明写 `DOMAIN_PRECONDITION_FAILED` |
| §4 表 7 行中 1 行拒绝码为 `—` | **派生**，同上 |
| 三机的 `condition_key()` 取值 | **派生**（规格未规定标识符本身） |
| 三机全部状态名 / 事件名 / 错误码线名 | **逐字冻结**，无一发明 |

## 7. 遗留登记（未擅自改冻结规格）

1. **`STATE_MACHINES §2` 的 ASCII 图缺 `CANCELLED` 节点**：§2.1 表有 5 条 cancel 边，
   图中 `CANCELLED` 出现 0 次。实现以表为准（14 条边），差异在装置中显式记录。
2. **§3 拒绝码 `—` 行数**：5 行（`STARTING→FAILED/TIMED_OUT/UNKNOWN`、
   `RUNNING→TIMED_OUT/UNKNOWN`），非实现初稿注释所写的 6 行。已按 awk 计数修正。
3. **目录近名陷阱**：`RUN_ACTIVE_EXISTS` 与 `BINDING_ACTIVE_RUNS_EXIST` 都在冻结目录中，
   §4 印的是前者。测试显式断言后者**不出现**，防止被"纠正"成看起来更合理的那个。

## 8. 门禁

- 平台：macOS ARM64（`--platform macos`，实测 `darwin/arm64`）
- 结果：**21/21 通过**（`result: pass`，21 条命令退出码全 0）
- 收口证据：`~/.minimax/teamspace-audits/dev-02-08/evidence/` 下
  **`wave2-macos-arm64-dev0211-state-machines-r<N>.json`，取编号最大且非
  `VOIDED-` / `SUPERSEDED-` 前缀的那一份**为收口证据
- 逐轮结果、`total_sha256` 与作废依据：见同目录
  `GATE-LOG-dev0211-state-machines.md`

### 为什么本文档不写死证据文件名与哈希

门禁的 `input_manifest` 覆盖整个工作区，**包括本文档**。
若把「验证本文档的那份证据」的文件名和 `total_sha256` 写进本文档，
改动本身就会让那份证据失效，于是必须再跑一次门禁，
而新证据又会让文档失效——这是一个无法收敛的循环。

本轮实际跑过 5 次门禁才想明白这一点（前 3 次分别是 clippy 失败、
改文档后 r3 失效、改文档后 r4 失效）。改为：

- **哈希与逐轮记录写在仓库外**（`~/.minimax/.../GATE-LOG-*.md`），不在仓库内引用；
- **仓库内只保留稳定的识别规则**（见上），任何一轮的证据都能被认出来；
- **最后一次门禁之后不再改任何仓库文件**。

这条规则本身比本节点的结果更值得记：**「收口证据」与「收口文档」互相验证时，
必须有一方在仓库外**，否则两者只能逼近、不能达到。

### 本轮门禁失败与失效记录（全部留档，不删）

| 证据 | 结果 | 依据 |
|---|---|---|
| `VOIDED-dev0211-state-machines-FAIL-clippy-bool-assert.json` | fail | clippy `bool_assert_comparison`；同处另有 1 条**重言断言**（恒真、无覆盖），一并清理 |
| `SUPERSEDED-dev0211-state-machines-r3-*.json` | pass | 代码态正确，但早于本文档定稿，输入清单与最终工作区不一致 |
| `SUPERSEDED-…-r4-*.json` | pass | 同上，本文继续修改后失效 |
| `VOIDED-dev0211-task-sm-FAIL-fmt-check.json` | fail | 第一轮 `pnpm run fmt:check` exit 1（改完测试忘跑 `cargo fmt --all`） |

两次失败都不是「测试跑不过」：一次是格式门禁先抓到漏跑的一步，
一次是 clippy 在 `cargo test` 之后才执行——**前两项绿不等于收口完成**。

**恒真断言比没有断言更糟**：它让用例数量看起来更可信，而实际覆盖没有增加。
`assert_eq!(x, x)` 这类写法在本轮审查中出现过一次，已清理。

## 9. 完成定义核对（`QUALITY_GATES.md` §1）

| # | 条目 | 结论 |
|---|---|---|
| 1 | 三机状态/边/创建目标与冻结规格逐字一致 | ✅ 56 用例双向校验（规格有表有 + 规格无表无） |
| 2 | 每条边声明 actor/前置条件/副作用/Event/拒绝码 | ✅ 前置条件与 Event 全覆盖；actor 按 §5 分层不建模（§5 已述） |
| 3 | 非法迁移零状态副作用 | ✅ 纯函数，无 I/O；拒绝类型不含新状态 |
| 4 | 全部迁移有表驱动与性质测试 | ✅ 56 用例 + Q-01～Q-05 装置 |
| 5 | 冻结规格零修改 | ✅ `docs/specs/` 全程未改 |
| 6 | 已发布迁移 `0001`～`0020` 零修改 | ✅ SQLx checksum 未漂移 |
| 7 | 零 git 提交 | ✅ HEAD 恒为 `1a14b8795e76fc373bef326eb5df96f9ac173f71` |
| 8 | 不新增依赖 | ✅ |
| 9 | 门禁 macOS ARM64 通过、输入前后哈希一致 | ✅ 21/21 |
| 10 | 判断与裁决留痕 | ✅ J26～J40 |

全量回归：`cargo test --workspace` **286 passed / 0 failed**（原 245 → 新增 41）。

## 10. 节点关闭

`docs/development/DEVELOPMENT_NODES.md` 中 `DEV-02-11` 由
`IN_PROGRESS`（第一轮）改为 **`DONE`**，审核标准「全部迁移有表驱动与性质测试；
非法迁移零状态副作用」已满足。

后续 `DEV-02-12`（Message/Delivery/Approval 状态机）依赖 `02-08`～`02-10`，
已具备进入条件，但**需 Human 显式批准后开工**。
