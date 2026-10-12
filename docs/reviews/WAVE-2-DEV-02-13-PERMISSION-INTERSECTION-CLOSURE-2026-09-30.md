# WAVE-2 DEV-02-13 终审：集中式权限求交与拒绝原因

> 冻结来源：`docs/specs/PERMISSION_MATRIX.md` §1/§2/§3/§5/§6/§7
> 错误码来源：`docs/specs/ERROR_CATALOG.md`
> 节点审核标准：Team∩Role∩Task∩Path∩Capability∩Approval 生效；拒绝不改变资源并写 Event
> 本文件在最后一次门禁之后不再改动。

## 1. 交付物

| 文件 | 行数 | 内容 |
|---|---:|---|
| `crates/domain/src/permission_matrix.rs` | 614 | §3 查表：6 主体、36 动作、216 格 |
| `crates/domain/src/permission_intersection.rs` | 650 | §2 八层求交、§6 Grant 校验、§7 缓存规则 |
| `crates/domain/tests/permission_matrix.rs` | 1199 | 24 个用例 |
| `crates/domain/examples/dump_matrix.rs` | 36 | 供 Q-09 读编译产物 |

测试：domain 24 用例；全工作区 354 passed / 0 failed / 1 ignored。
（表中行数与上述数字取自门禁 r2 之后的一次实测；此前的行数快照在补写 derived-reading
注释后即失效，故不保留旧值。）
`cargo clippy --workspace --all-targets --all-features --locked -- -D warnings` 无告警。

## 2. 结构性设计：把「漏检」变成不可表达

`PermissionRequest.terms` 是**定长 `[LayerVerdict; 8]`**，判别值 = §2 印刷序号。
构造不出缺项的请求，所以「忘了查路径策略」不是运行时意外而是**类型上无法表达**。
§2 的「默认拒绝」随之成为扫描的性质：一个 term 只在它自己说 permit 时才 permit。

扫描顺序 = §2 印刷顺序，首个拒绝即终止。`ValidApprovalGrant` 排最后，于是
「Approval 只能缩小或临时补充已允许的动作，不能突破 Team 安全上限」
**不需要任何特例规则**：前面任何一项拒绝都已终止扫描。

**这条顺序是依赖，不是巧合**——§2 的印刷顺序不等于因果重要性顺序。
若将来出现「必须先查 Approval 才能知道某动作是否 Human-only」的场景，
这个顺序会给出错误答案。当前三例都不需要，代码注释里已写明。

## 3. 本轮的唯一真缺陷：Human-only 行里的 `—` 报错了拒绝码

**症状**：`Binding 绑定/解绑/换绑` 的 Leader 格是 `—`，实现报
`AUTH_PERMISSION_DENIED`（补救：查看权限依据/请求授权）。

**依据**：`ERROR_CATALOG.md` 里两个码的补救动作不同——
`AUTH_PERMISSION_DENIED` 说「查看权限依据/请求授权」，
`AUTH_HUMAN_REQUIRED` 说「打开审批/接管入口」。
§5 第 3 条把「Binding 绑定/解绑/换绑」冻结为 Human-only，
所以告诉 Leader「去申请授权」指向一个 §5 说永不存在的东西。

**旧定义错在哪**：`is_human_only_action` 原定义为「其余列都不 permits」。
这条规则对 `Task cancel/reopen`（Leader=`S`）正确，
但对 `Team 创建/编辑`（Owner=`A`，其余全 `—`）错误——§5 不含它，
它的 `—` 是普通权限拒绝。两条规则的分界正是 **Owner 格的符号**。

**修法**：`is_human_only_action` = Owner 格为 `H`；
`refusal_is_human_only(action, actor)` = 该行 Human-only 且本格不 permits。

**反向验证**：把新分支短接成恒 `false`，2 个用例立刻失败并定位到
`NotPermitted` vs `HumanOnly`；还原后 24/0。**不是恒真断言。**

## 4. 定性降级：这是 derived reading，不是 mechanical

写文档时机械比对 §3 与 §5，发现两者**不是同一份清单**：

**§3 有 `H` 但 §5 未列**（3 行）：`Task cancel/reopen`、`Message reassign`、
`NotificationPolicy 修改`（三行 Leader 均为 `S`）

**§5 列了但 §3 无对应 `H` 行**（2 类）：`跨 Team 分享`、`删除/清理关键历史`

所以 §5 是散文式能力清单（可含多个 §3 行、也可含 §3 未建模的动作），
§3 的 `H` 是逐格判定标记。**两者各有覆盖面，互不包含。**

结论：第 3 节的映射**规格未成文**。支撑它的只有两条间接证据：
1. `H` 在 §3 全 36 行只出现在 Owner 列（机械可验，16 次）；
2. 两个码在 `ERROR_CATALOG.md` 的补救动作不同。

**实现不改**，理由：另一个选择（`—` 一律报 `AUTH_PERMISSION_DENIED`）同样未成文，
且会让 §5 明文冻结的动作指向不存在的授权。两者都欠定时，
取**与 `ERROR_CATALOG.md` 补救动作一致**的那个，并把依赖写进代码注释，
使不同判读是一处改动。

**待 Human 裁决**：§3 的 `H` 与 §5 的 8 类以哪份为准？
本实现取「§3 的 `H` 标记」为行级判据。

## 5. 刻意的不建模（各处均在代码注释写明「等什么条件出现才该做」）

| 项 | 为什么不建模 |
|---|---|
| `Layer` 的 serde 派生 | §2 把八项印成代码块里的散文，不是线值。派生会产生 `VALID_APPROVAL_GRANT` 这种无规格约束的线名 |
| `ActorKind` 的 serde 派生 | 它是 §3 的矩阵**列**；Event 的 `actor_kind` 唯一冻结值是 `ROLE_BINDING`（迁移 0015 的 CHECK），而 §1 明确 Harness「不作为权限主体持久化」，故 `ROLE_BINDING` 不是 §3 的一列。绑定二者会断言规格未作出的关系 |
| `VerdictPair` 的极性 | `A/H`、`A/S`、`R/S` 是否「默认/例外」规格未成文。按印刷顺序存 `first`/`second`，由应用层用 `resolve(bool)` 声明 |
| `Human Admin` 主体 | §1 标「P0 不开放邀请」，不是 §3 的一列 |
| `Adapter/Harness` 主体 | §1「不作为权限主体持久化」，§3 给了单独的「Harness 额外条件」列而非判定列 |
| `scope` 求值、`expires_at` 时钟 | §6 未给 scope 表达式求值规则；§2 把过期检查放在 `ValidApprovalGrant` **term**，由调用方解析后传入 |
| §3 36 行 → 迁移 0009 的 9 动作 × 11 资源映射 | 规格没给映射键，不发明。行 24/25/26 同落 `(UPDATE, WORKSPACE)` 却读 S/S/H，靠路径层区分 |

## 6. 机检装置

| 装置 | 终态 | 覆盖 |
|---|---|---|
| Q-08 `q08-permission-matrix-canonical.py` | PASS | §3 抽取自洽：矩形、图例、列头、36 行绝对锚点 |
| Q-09 `q09-matrix-vs-spec.py` | **PASS=263 FAIL=0** | Rust 编译产物 vs 规格：216 格 + 36 标签 + 11 结构计数 |

Q-09 通过 `cargo run --example dump_matrix -p teamspace-domain` 读**编译产物**，
不重新 parse `.rs` 源码（已由独立审查确认，`subprocess.run` 见装置 L96-99）。

**两台装置的结论不可混用**：Q-08 只验规格自洽，**不对 Rust 做任何断言**；
Q-09 的 PASS 才等于「Rust 与规格逐字一致」。Q-08 的输出里印有范围声明。

## 7. 独立审查判定 FAIL：装置自身带 3 处假绿（6 条全部接受并已修）

独立 verifier 只读装置源码 + 在 `/tmp` 做变异实验，**判定 FAIL**。全部成立：

1. **`zip` 静默截断（高危）** — 规格某行只有 5 格时，第 6 列完全不被比较却报绿。
   对照：旧版 `PASS=253 FAIL=0`，新版 `FAIL …规格只有 5 格，应为 6 格`。
2. **结尾字符串硬编码（高危）** — `"36 行 × 6 列全部与规格逐字一致"` 是字面量，
   与实际比较次数无关。现改为由 `len(spec)`/`passed` 推导，并加内部断言
   `passed == expected_total`，对不上即报「内部错误」。
3. **`#COLUMN`/`#ACTOR` 解析后从不校验（中危）** — 实测把 `Owner` 改成
   `WRONG_OWNER` 仍 `PASS=254`。列顺序是每格比对的前提，现已逐列对账。
4. **无 36 行绝对锚点（中危）** — 原只比「Rust 长度 vs 规格长度」，
   规格与 Rust 同步漂移到 35 会全绿。现加 `FROZEN_ROWS = 36`。
5. **DUMMY 死代码 + docstring 撒谎** — `DUMMY` 写进 /tmp 但 `cargo` 以 `cwd=REPO`
   运行，`--example` 实际解析到仓库内 example；DUMMY 是死代码。
   残留风险：example 若被改成硬编码打印正确答案，装置即绿。
   现新增 `example_is_trustworthy()`，缺任一关键 marker 即 FAIL。
6. **Q-08 缺格时 IndexError 崩溃** — 且崩溃发生在打印诊断之前，信息全丢。
   另：Q-08 docstring 写「35 行 × 6 = 210 格」，**实际 36 × 6 = 216**。

**每条修复都做了反向验证**（构造已知坏输入，确认新版报错而旧版放行）。

## 8. 测试里的三处手抄错误（本轮 5 个失败中 4 个是测试写错）

| 用例 | 我写的期望 | 规格实际 |
|---|---|---|
| `human_only_actions_admit_no_role_or_system_actor` | `Team 创建/编辑` Owner 是 `H` | L53 明写 `A` |
| `a_matrix_deny_refuses_before_any_term` | `Task 分配/改派` Product 是 `—` | L63 明写 `S` |
| `approval_can_supplement_but_never_rescue` | 用 `HumanTakeover` 测 approval 补充 | 该行非 Owner 全 `—`，矩阵层先拒，**永远测不到** |

三处全是**手抄规格抄错**。另修两处：
`S` 格本身不产生收窄（收窄由**项**决定）、一个 `!x || !x` 的恒真断言。

**又犯了第 8 次「凭印象写数字」**：以为 `Harness 绝不可自换绑` 整行对所有 Role
都应是 `HumanOnly`，没先统计 `H` 出现在哪几列。解药不是「下次注意」，
是让错误无法表达：新增表驱动的
`every_compound_fact_is_reachable_and_unproven_denies` 覆盖全部三个事实，
`every_term_is_supplied_and_the_array_is_the_right_size` 逐字钉死八个项的
`condition_key()` 与顺序（原来是 `!is_empty()` 恒真）。

## 9. 遗留登记（未擅自改冻结规格）

1. **§3 的 `H` 与 §5 的 8 类覆盖面不一致**（第 4 节）——待 Human 裁决。
2. **§3 36 行与迁移 0009 的 9 动作 × 11 资源粒度不同**，规格无映射键。
3. **`ERROR_CATALOG.md` 目录比 §7 宽**：目录另有
   `WORKSPACE_SYMLINK_ESCAPE` / `PATH_NOT_FOUND` / `PATH_NOT_WRITABLE`
   三个 Workspace 路径码，§7 未列。本轮不擅自接上——接上是改协议契约。

## 10. 门禁

见仓库外台账。识别规则：取编号最大且**非** `VOIDED-` / `SUPERSEDED-` 前缀的证据文件。
