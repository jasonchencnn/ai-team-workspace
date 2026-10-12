# FC-03 — Run 终态矩阵完整性

**状态**：交付，待 Codex 独立复核。本端自判不构成 DONE。
**基线**：`1a14b8795e76fc373bef326eb5df96f9ac173f71`，分支 `codex/wave-2-ci-validation`，共享 dirty worktree。
**费用**：$0。

---

## 1. 缺陷与修法

`MR-01` 实测 `SUCCEEDED -> FAILED` 写入后行确实变为 `FAILED`。`0027` 关闭了非终态来源的 11 个非法格，`0012:142-145` 关闭了 16 个「终态 → 活动」格，**剩下 12 个「终态 → 终态」格没有任何守卫**。

上一轮把这条缺口写成 `terminal_to_terminal_is_a_registered_open_gap`（断言 `is_ok()`），等于把非法写入**固化成期望行为**。Codex 判「因错误原因而绿」。本包反转该断言，真正补齐守卫。

新增 `migrations/0029_run_terminal_integrity.sql`，注册为 `Migration::new(29, "run terminal integrity")`：

```sql
CREATE TRIGGER runs_terminal_status_integrity_update
BEFORE UPDATE OF status ON runs
FOR EACH ROW
WHEN OLD.status IN ('SUCCEEDED', 'FAILED', 'TIMED_OUT', 'CANCELLED')
     AND NEW.status IS NOT OLD.status
BEGIN
    SELECT RAISE(ABORT, 'Run terminal status violation');
END;
```

一个谓词而非十二个触发器——冻结语句是单条规则，十二份近乎相同的函数体就是十二个藏变异的地方。`NEW.status IS NOT OLD.status` 保留同态写回，使已终结 Run 的 `failure_reason`、诊断引用、`version` 等业务字段更新照常工作，与 `0027` 对非终态象限的处理一致。

它同时覆盖 16 个「终态 → 活动」格，因为那是同一条冻结规则；把象限切开只为保住一个错误字符串，等于重新引入本迁移要消灭的那个理由。

## 2. 矩阵：实测而非手算

`mutation/probe/matrix_probe.sh`（纯 `sqlite3`，不依赖 cargo）逐格实测。**本端独立重建 v28/v29 库复核**：

| | 触发器总数 | ACCEPTED | REFUSED |
|---|---|---|---|
| v28（`0001..0028`） | 232 | 29 | 27 |
| v29（`+0029`） | 233 | 17 | 39 |

恰好 +1 个触发器，恰好关闭 12 格。8×8 分解（全部取自实测）：

```
64 格 = 8 个 self（不是迁移）
     + 56 个 off-diagonal
         = 17 条冻结合法边（STATE_MACHINES §3）
         + 39 个非法 off-diagonal
             = 11  非终态来源  -> 0027
             = 16  终态→活动   -> 0012:142-145
             = 12  终态→终态   -> 本迁移
```

剩余 17 格与 `docs/specs/STATE_MACHINES.md` §3 的合法边**逐条相等**：`QUEUED→{STARTING,CANCELLED}`、`STARTING→{RUNNING,FAILED,TIMED_OUT,CANCELLED,UNKNOWN}`、`RUNNING→{SUCCEEDED,FAILED,TIMED_OUT,CANCELLED,UNKNOWN}`、`UNKNOWN→{RUNNING,SUCCEEDED,FAILED,TIMED_OUT,CANCELLED}`。

三方一致：探针实测 = `STATE_MACHINES.md` §3 = 17 + 12 = 29。原始日志：`main/probe-v26/v28/v29-matrix.log`、`main/matrix-decomposition.log`、`main/matrix-reconciliation.log`。

### `0027` 注释的数字是陈旧的

`0027` 头注释称「32 个 off-diagonal 格，0012 拒绝 8 个，其余 24 个被接受」。这三个数与实际发布的触发器不符：off-diagonal 实为 56，`0012` 拒绝 16，`0027` 拒绝 11。`0027` 已应用到已发布数据库、字节冻结，**本包未改动它**，差异记录在 `0029` 注释与本文件。

`0029` 注释另有一处由本端复核发现并已修正的错误：矩阵的 `QUEUED` 行原写成全 `refuse`，把 `QUEUED→STARTING`、`QUEUED→CANCELLED` 两条**冻结合法边**标成拒绝，导致该表自身求和 = 27 却声明 `ACCEPTED=29`。**测试抓不到它**——`the_status_matrix_matches_the_frozen_edge_table` 用的是 spec 派生表而非注释，所以全绿也掩盖不了。

## 3. 错误遮蔽：如何处理而非隐藏

SQLite 的 `BEFORE` 触发器按**创建顺序逆序**触发（实测，非假设：同表两个触发器，后创建者先触发）。因此 `0029` 会先于 `0012`、`0027` 触发，终态象限调用方看到的错误消息会变成 `0029` 的。

`0027` 曾以此为由判定这些格必须保持开放。**理由成立，结论不成立**：为一个错误字符串保留一个真实缺陷，是拿真缺陷换表面缺陷。

- 受影响断言改为断言稳定的**领域原因**（终态 Run 没有出边）+ 完整行内容证明拒绝后行未动，而非钉死某个触发器的字符串。
- `0012` **未被削弱**：`the_pre_existing_terminal_resurrection_guard_still_bears_weight` 在只有 `0001..0028` 的库上直接证明 `0012` 以自己的消息拒绝「终态 → RUNNING」，旧守卫被证明仍承重，而不只是仍然存在。
- M4 变异（中和 `0027`）导致 4 个测试失败，进一步确认三层守卫各自独立承重。
- 错误字符串不是被冻结的业务边。`STATE_MACHINES` §3 约束 Run **可以持有哪些**下一个状态，不约束由哪个守卫报告拒绝；`ERROR_CATALOG` 中若干字符串由应用层（K3/K4）映射，不由存储触发器映射。

## 4. 测试

`tests/run_transition_integrity.rs`（20 个，原 15）：

- `terminal_to_terminal_is_refused_and_the_row_does_not_move` —— **反转**上一轮固化缺口的断言。
- `a_terminal_status_preserving_update_is_not_a_transition` —— 同态写回仍合法（M2 变异证明它承重）。
- `the_pre_existing_terminal_resurrection_guard_still_bears_weight` —— v28-only 库上证明 `0012` 仍承重。
- `the_full_schema_refuses_the_terminal_quadrant`
- `upgrading_to_0029_preserves_every_existing_row` —— 逐表 before/after 快照。
- `a_failed_0029_upgrade_has_no_side_effects` —— 触发器名被占用、迁移失败、快照不变。
- `the_status_matrix_matches_the_frozen_edge_table` —— 去掉终态来源的跳过，覆盖全部 64 格，并断言格数。
- `this_package_pins_exactly_versions_27_and_29` —— 采用 MR-04 口径 `COUNT(*) WHERE version = N AND success = TRUE`，并断言成功/失败拆分。

`tests/core_history_retention.rs`（19 个，数量不变）—— **纠正 R02 的错误对照**：

原 `:1017` 的 `the_tables_are_guarded_not_welded_shut` 把单行 `RUNNING -> TIMED_OUT -> SUCCEEDED` 当作合法路径。`STATE_MACHINES` §3 规定四个终态无出边，`TIMED_OUT -> SUCCEEDED` 正是本包关闭的 12 格之一。**该对照能通过，只因为没有守卫守那个格**——与它本应排除的缺陷是同一个原因。因缺陷之故而绿的正对照比没有正对照更糟，因为它会被读成「守卫没有过严」的证据。

改为每条终态路径在**各自新 seed 的 Run** 上从 `RUNNING` 抵达，终态 Run 不复用；`0004` 要求终态必须有 `completed_at`，故一并提供。

`tests/run_lease_consistency_forward_migration.rs`（19 → 20）—— 受遮蔽协议约束的两处断言改为稳定领域原因 + 行未动，并新增 pre-0029 基线证明 `0012` 的两个守卫仍承重。

## 5. 变异台账

完整台账：**`main/mutation-ledger.md`**。原始未过滤输出：`mutation/raw-logs/*.raw.log`。驱动脚本 `mutation/run_mutations.py` 可重跑。

变异只在 `mutation/workspace/` 独立副本上进行；`crates/persistence-sqlite` 只读。命令一律 `--locked --offline`。

| 变异 | EXIT | 结果 | 失败原因（原始日志逐字） |
|---|---|---|---|
| BASELINE | 0 | 20 passed / 0 failed | — |
| M1 `0029` 体改 `RAISE(IGNORE)` | 101 | 14 / 6 | `SUCCEEDED -> FAILED (a terminal Run has no outgoing edge): the statement was accepted` |
| M2 `0029` 去掉 `NEW.status IS NOT OLD.status` | 101 | 18 / 2 | `writing the same terminal status back was refused (code: 1811) Run terminal status violation` |
| M3 删除 `0029` 触发器 | 101 | 14 / 6 | 含 `the_new_guard_is_installed` |
| M4 `0027` 体改 `RAISE(IGNORE)` | 101 | 16 / 4 | 证明 `0027` 仍独立承重 |

每条变异落 unified diff（`mutation/patches/`）、命令、exit、原始 stdout/stderr，并在还原后重新哈希。脚本在还原哈希与生产不一致时以非零码退出。

### 第一次变异运行无效（已作废，留痕）

`main/mutation.log` 保留了第一次运行及其作废理由。要点：

1. 当时记录 M1–M3 `EXIT=101` 但 `--- failing tests ---` 为空。本端复现：`mutation/crate/` 只有 `Cargo.toml` 与 `.gitkeep`，其 `teamspace-domain = { path = "../domain" }` 指向不存在的目录，**cargo 在 manifest 加载阶段就失败**，早于编译与任何测试执行——该 exit 不能证明测试抓到了变异，即 MR-01 判定的「因错误原因而红」。
2. 当时的「restore verification」哈希打自一个随后被删除的 `/var/folders/.../tmp` 工作区；而**实际留在 evidence 目录的副本仍带着 M4 的变异**——其中 `0027` 哈希 `77cd01be…`，生产为 `58a689fc…`，第 145 行仍是 `SELECT RAISE(IGNORE);`。对保留副本跑基线复现出 M4 签名，16 passed / 4 failed。
3. 原 cargo 命令漏了 `--locked`；`copy-source-after.sha256` 被 `README.md` 提及但从未创建。

## 6. 前向迁移与零漂移

`0001..0028` 一个字节未改。本端核验：今日修改的迁移文件恰为 `0023..0028` 六个**本轮新建**文件；`0001..0022` 今日漂移数 = **0**。`0029` 为追加，`0028` 及更早的校验和不变。

## 7. 创建轴：只登记，不授权

`STATE_MACHINES` §3 的创建行只有 `create -> QUEUED`，而 `runs_consistency_insert`（`0012:72-108`）完全不约束 status，因此 Run 仍可直接 INSERT 为 `SUCCEEDED`。

**本包不修，也不声称已修**：

- `creating_a_run_outside_queued_is_still_possible_and_that_is_measured` 继续**测量**这一事实，而不是假设它。
- 此类行在本 crate 中绝大多数是**留存历史 fixture**，不是生产创建命令。`DATA_CONVENTIONS` §5/§6 要求 Run 历史留存，`DOMAIN_MODEL` §5 要求 Run 不可覆写、重试新建——直接插入已终结 Run 是在表达被允许的留存历史。
- 强制 `NEW.status = 'QUEUED'` 的 `BEFORE INSERT` 守卫经实测会拒绝 `crates/persistence-sqlite/tests/` 中每一个非 `QUEUED` 插入，其中多数在本包写集之外。**本包无权强制所有 INSERT 为 `QUEUED`**，故不加该守卫，也不为让 fixture 通过而加 fixture 专用逃逸参数。
- 冻结的创建行描述的是 `task.claim` 命令，运行在应用层。生产创建与重试是否真的走 `QUEUED`（连同 actor、version 与 Event/Outbox）属 K3/K4 可信执行链的责任。**本包不声称应用创建或重试路径已被验证。**

升级守卫同样**有意缺席**：触发器约束语句而非行，每条历史 `runs` 行按构造都合法（谓词在 `UPDATE` 上求值，此处不更新任何行），故 `valid = 1` 探针表是重言式（`ADVERSARIAL_REVIEW_PLAYBOOK` 规则 5）。升级保证改由 `upgrading_to_0029_preserves_every_existing_row` 与 `a_failed_0029_upgrade_has_no_side_effects` 直接断言。

## 8. 终值

| 项 | 值 |
|---|---|
| 全量 `cargo test -p persistence-sqlite --locked --offline` | **233 passed / 0 failed**，31 target / 31 result，exit 0 |
| 基线（同命令，本包改动前） | 227 passed / 0 failed |
| 增量 | +6：`run_transition_integrity` 15→20，`run_lease_consistency_forward_migration` 19→20，`core_history_retention` 19→19（重构，未增） |
| 费用 | $0 |

原始日志：`main/baseline-full-suite.log`、`main/final-full-suite.log`。

**格式**：交付前对本包改动的两个测试文件单独执行 `rustfmt --edition 2024`
（`tests/run_transition_integrity.rs`、`tests/run_lease_consistency_forward_migration.rs`）。
**未使用 `cargo fmt -p` 或 `--all`**，以免改写写集之外的继承 dirty 文件；写集外文件
mtime 经抽查未变。格式化改变了字节，故上表哈希、全量终值与变异台账均在格式化**之后**
重新取得并重跑，非沿用格式化前的旧日志。

**最终哈希**

```
c47cfc597b7bc36d34115745b20a6d9bdc1058a86df06e5db0e9cdb5bb51b0c4  migrations/0029_run_terminal_integrity.sql
6023d62acd8f91741599ea75d5141f873a28d53f9924f34703ec5cb99786f818  src/lib.rs
0b42366cef8ba0974eca28e24e2cf5cbf473f79af8b4199b0cd012d3a850d45e  tests/run_transition_integrity.rs
03a483f81cd2db0d09f5c4bb12eaf2cbaf24f3a5c97e3f0dd7953c3bbc9106f7  tests/core_history_retention.rs
fdc84e2e36d7c0a3cd83aef574395764dac4a08582825fc24898e05b75e6a237  tests/run_lease_consistency_forward_migration.rs
```

## 9. 未运行 / 未验证（不得声称已通过）

- 完整 canonical Mac 21 未运行；在线 advisory / install 未运行；无网络授权。
- 真实 Harness / Keychain、最低 macOS 14 实机、A11y、性能、备份恢复均未运行。
- 应用层创建与重试路径**未验证**（见 §7）。
- 分支覆盖未度量，`branch_coverage_verified` 不因本包而变为 true。
- 本包不含新增依赖，`Cargo.toml`/`Cargo.lock` 未因本包改动。

## 10. 待 Codex 裁决

1. `0029` 同时覆盖 16 个「终态 → 活动」格（与 `0012` 重叠），换取单一谓词与稳定领域原因；代价是这些格现在由 `0029` 报告而非 `0012`。
2. 创建轴**只登记不关闭**（§7）——若 Codex 要求本轮一并关闭，需另行签发写集，因其影响多个包的 fixture。
3. `0027` 注释数字陈旧但字节冻结，本包以文档记录而非修改处理。
