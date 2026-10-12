# WAVE-2 DEV-02-12 终审：Message / DeliveryAttempt / Approval 状态机

- 节点：`DEV-02-12` 实现 Message/Delivery/Approval 状态机
- 依赖：`00-04`～`00-05`、`02-08`～`02-10`（均已 `DONE`）
- 冻结来源：`docs/specs/STATE_MACHINES.md` §5 / §7、`docs/specs/ERROR_CATALOG.md`、
  `docs/specs/PERMISSION_MATRIX.md` §6
- 审核模型：`minimax/MiniMax-M3.1-Flash-Preview`（未使用其他模型）
- 节点审核标准：「RETRYING 瞬态正确回流；FAILED/EXPIRED/NEEDS_HUMAN 不丢历史」
- 结论：**DONE**

---

## 1. 交付物

| 文件 | 行数 | 内容 |
|---|---:|---|
| `crates/domain/src/message_state_machine.rs` | 720 | §5 Message：10 状态 / 15 边 + 1 创建 |
| `crates/domain/src/attempt_state_machine.rs` | 319 | §5 DeliveryAttempt：5 状态 / 4 边 + 1 创建 |
| `crates/domain/src/approval_state_machine.rs` | 479 | §7 Approval：5 状态 / 4 边 + 1 创建 |
| `crates/domain/tests/message_state_machine.rs` | 886 | 19 用例 |
| `crates/domain/tests/attempt_state_machine.rs` | 292 | 10 用例 |
| `crates/domain/tests/approval_state_machine.rs` | 532 | 15 用例 |

节点标准逐条核对：

| 标准 | 结论 | 断言位置 |
|---|---|---|
| `RETRYING` 瞬态正确回流 | ✅ | `there_is_no_retrying_state`：枚举无 `RETRYING`；重试是 `ROUTED → QUEUED`（发 `MESSAGE_RETRY_SCHEDULED`），**回到 QUEUED 而非停留态** |
| `FAILED` 不丢历史 | ✅ | `replied_and_expired_absorb_and_failed_does_not`（出度恰为 1）+ `failed_needs_human_and_expired_preserve_history`（三个终态各发自己的事件，且事件名不共用） |
| `EXPIRED` 不丢历史 | ✅ | 同上；`EXPIRED` 出度 **0**，§5 明文「不在原 Message 上重开」 |
| `NEEDS_HUMAN` 不丢历史 | ✅ | `NEEDS_HUMAN` 出度 **2**（`Reassign` / `RequeueNeedsHuman`），两条边各发自己的事件 |

领域层合计 **44 个用例**，`crates/domain` 全量 **117 个用例**；
`cargo test --workspace` **330 passed / 0 failed**。

## 2. 本轮的核心发现：冻结规格用标点区分事件基数（**派生读法**）

把 §2/§3/§4/§5/§7 的 **Event 列**全文扫了一遍，得到一条贯穿全规格的规律：

| 标点 | 语义 | 出现处 |
|---|---|---|
| `、` | **两个事件都发**（一条边，多事件） | 全规格 7 行 = §2 六行 + §5 一行（`DRAFT → QUEUED` 发 `MESSAGE_SENT`、`MESSAGE_QUEUED`） |
| `/` | **互斥二选一**（必须拆成两条边） | **全规格仅两行**：§4 `ROLE_BOUND`/`ROLE_REBOUND`、§5 `MESSAGE_REASSIGNED`/`MESSAGE_REQUEUED` |

> **这条约定规格从未成文。** 证据是 n = 2，且其中一条（§4）正是据以推广的那个先例，
> 因此不构成独立确认。它记为**派生判断**而非机械事实（见 §12）。

之所以仍按它拆边：拆分**不丢任何冻结值**——两条边逐字保留该行的前置条件与拒绝码，
只差事件；若 Human 后来认为 `/` 意为「都发」，把两条边合回一条带两事件的边即可，
其余不受影响。当前读法由 `reassign_and_requeue_are_distinguishable` 钉住。

这条规律同时做了两件事：

- **追认** DEV-02-11 对 §4 的拆边判断（当时只是语义直觉，现在有全文证据）
- **决定** §5 的 `NEEDS_HUMAN → QUEUED` 必须同样拆成 `Reassign` / `RequeueNeedsHuman`

若没扫全表，很容易把 `/` 与 `、` 当成同一种「或者」，给该行挂上两个事件——
那会让**改派与重投无法区分**，丢掉 §5 明写的副作用「记录 route override」。

`send_emits_both_events_because_the_spec_joins_them_with_a_comma` 与
`reassign_and_requeue_are_distinguishable` 成对断言这个约定，防止单边被改坏。

## 3. 展开规则与边数对账

| 规格章节 | 打印行数 | 生命周期边 | 创建 | 展开原因 |
|---|---:|---:|---:|---|
| §2 Task | 16 | 14 | 2 | — |
| §3 Run | 16 | 17 | 1 | `UNKNOWN → FAILED/TIMED_OUT/CANCELLED` 目标扇出 3 |
| §4 Binding | 7 | 7 | 1 | `ROLE_BOUND`/`ROLE_REBOUND` 互斥事件拆边 |
| §5 Message | 13 | 15 | 1 | 两处源扇出 + 一处互斥事件拆边 |
| §7 Approval | 5 | 4 | 1 | — |

Q-06 装置按三条规则机械解析规格并直接给出这些数字，**先在已收口的 §2/§3/§4
上自证一致**（14 / 17 / 7，与 Rust 实际完全吻合）再用于 §5 / §7。

## 4. 对抗审查抓到的真缺陷：Q-09

`NEEDS_HUMAN → QUEUED` 的两条边，§5 的主要拒绝码是 **`AUTH_HUMAN_REQUIRED`**，
实现写成了 `AUTH_PERMISSION_DENIED`，且该码**根本不在** `MessageErrorCode` 枚举里。

**不只是拼写错**：两个码的补救路径不同。`AUTH_HUMAN_REQUIRED` = 打开审批/接管入口
（该行 Actor 是「Human 操作」），`AUTH_PERMISSION_DENIED` = 你没有权限。
报错方向错会把人引到「申请权限」而不是「找人接管」。

**19 个测试全部漏掉**，因为 `EXPECTED_ROWS` 是照着**同一份错误读法**抄的——
实现与测试来自同一次阅读，彼此无法互证。

### 修法：让装置的输入不是我的记忆

新建 **Q-07 `q07-spec-vs-rust-edges.py`**：逐行比对 `(from, to, events, rejection)`
四元组，左边从 `STATE_MACHINES.md` 机械解析，右边从 Rust 边表解析，
中间经 `as_str()` 把变体名换回线名再比。

**Q-07 上线第一个跑出来的就是 Q-09**（`PASS=56 FAIL=2`）。修复后 57/57。

**并做了反向验证**：把修好的边**故意改回错误码**重跑，确认装置报
`PASS=56 FAIL=2`。装置能抓已知坏输入才算可信——否则「全绿」和「根本没在比」无法区分。

| 缺陷 | 手抄错的东西 | 抓出者 |
|---|---|---|
| Q-07 | 事件变体名带 `Run` 前缀，serde 线名分叉 | 测试（serde 展开 vs `as_str()`） |
| Q-08 | 30 个错误码 serde 线名丢领域前缀 | Q-05（读 `ERROR_CATALOG.md`） |
| Q-09 | 2 条边拒绝码整个抄错，且码不在枚举里 | Q-07（读 `STATE_MACHINES.md`） |

**结论：断言必须锚在规格文件或目录文件上，不能锚在「我读了一遍规格」上。**

## 5. 机检装置（六台机全绿）

| 装置 | 覆盖 | 结果 |
|---|---|---|
| Q-04 `q04-catalog-retryable.py` | 六机每个错误码的「目录存在性 + retryable 一致性 + 跨机变体名一致性」 | **97/97** |
| Q-05 `q05-serde-wire-names.py` | 六机 16 个枚举逐变体「serde 实际线名 == `as_str()` 冻结线名」 | **129/129** |
| Q-06 `q06-spec-table-canonical.py` | 从冻结规格解析边表并**只比对边数**（不打开 `.rs`，见下） | **10/10** |
| Q-07 `q07-spec-vs-rust-edges.py` | 逐行比对规格规范边与 Rust 边表的四元组 | **57/57** |

四台装置的共同点：**输入都不是我的记忆**——分别锚定
`ERROR_CATALOG.md`（存在性、retryable、线名）与 `STATE_MACHINES.md`（边表四元组）。

**Q-05 对本轮三台新机一次就全绿**——因为 DEV-02-11 的 Q-08 修复（显式
`#[serde(rename)]` + 变体名采用规格拼写）已成**结构性保证**而非需要重记的规则。
这是「靠机制不靠自觉」在本节点上第一次直接兑现收益。

### 装置自身的失误（全部先查探针）

| 轮次 | 装置错在哪 | 对账方式 |
|---|---|---|
| Q-06 首版 | 漏「目标扇出」与「互斥事件拆边」两条规则，对 §3 少算 2 条、§4 少算 1 条 | 与已收口 §2/§3/§4 的 Rust 实际边数对账 |
| Q-07 首版 | `const[:-1]?Edge` 正则拼错；`balanced()` 为 `impl X {}` 写，对 `&[...]` 常量只吃到第一条边 | 五个章节全解析出 0 条边 |
| Q-07 二版 | 不认识 `RUN_RECONCILED_*` 通配，把 §3 三行事件解析成空集 | §3 报 3 条「规格行未找到」 |
| Q-04 演进期 | 正则漏 `Enum::Variant` 前缀；范围未限定在 ErrorCode 枚举 | 提取到 0 个 / 32 个假 FAIL |
| Q-05 修完 Task 后 | 只模拟 `rename_all`，不认识显式 rename，报 16 条假红 | 回源码确认 rename 确实在位 |

**两条硬要求（均由本轮付代价换来）**：
1. 装置先用**已知正确答案**的输入自证，再处理新输入。
2. 装置必须**对已知坏输入报得出错**，否则全绿没有意义。

## 6. 独立对抗审查（同模型 verifier，结论 PARTIAL，五条全部接受并已修）

| # | 级别 | 发现 | 处置 |
|---|---|---|---|
| 1 | HIGH | Q-09 拒绝码错（与 Q-07 **独立**发现同一处） | 已修；审查方独立复核确认修复正确 |
| 2 | MED | **Q-06 装置的 docstring 撒谎** | 见下 |
| 3 | LOW | 四处文档数字/措辞不准 | 全部更正 |
| 4 | LOW | `AttemptTransition` 两个访问器硬编码 `&[]`，测试断言恒真 | 让访问器真读边字段（见下） |
| 5 | INFO | 标点规律是派生读法，不是机械规律 | 文档已降级为「派生」+ 补安全性论证 |

### 发现 2：一个**打印出正确答案却报 OK** 的装置

Q-06 在 Rust 还写着 `AUTH_PERMISSION_DENIED` 时，
它自己的输出打印了 `code=AUTH_HUMAN_REQUIRED`，然后报 `OK=10 FAIL=0`
——因为它唯一的比较对象是硬编码边数，从不看 Rust。

> **一个打印出正确答案却报 OK 的装置，比没有装置更坏**：它制造虚假信心，
> 而虚假信心会阻止你去查。

Q-06 的文档已改写，开头明写「本装置不做什么」以及这次假绿的经过；
**Q-07 补上 spec↔Rust 逐行比对这个洞之后，Q-06 才敢继续存在**。

### 发现 4 的处置选择

两条正解：删断言，或让被断言的东西变成真的。**选后者**——
`AttemptTransition` 改为携带边上的 `preconditions`/`events`（值本来就是空），
访问器从字段读。删断言会让「§5 的 attempt 链没有前置条件」少一道防线；
改成真读字段则保住防线，且将来若某行真的声明了前置条件，它会**显形**而不是被静默丢弃。

## 7. 刻意的「不建模」（8 处）

| 不建模的东西 | 理由 |
|---|---|
| `RETRYING` 状态 | §5 明文「派生瞬态，不持久化为 Message 终态」。做成状态会让「能否被领取」「是否过期」两问无解 |
| DeliveryAttempt 的 **Event 列** | §5 只给状态链。所有 attempt 观测记在 **Message 事件**上（"attempt SUCCEEDED" 是 `MESSAGE_DELIVERED` 的一部分）。编 `ATTEMPT_STARTED` 即自造协议 |
| DeliveryAttempt 的 **前置条件 / 拒绝码** | §5 的 attempt 链**根本没有这两列**。拒绝码只能是「无此边」的 `DOMAIN_INVALID_TRANSITION`，无需派生 |
| `MessageCommand` 的 `as_str()` / serde | §5 的 Actor/命令列给的是 `message.send` / `message.retry`，那是**权限矩阵的输入**，不是本机拥有的线格式词表 |
| `AttemptCommand` 的 `as_str()` / serde | §5 未给 attempt 命令词表 |
| `ApprovalCommand` 的 `as_str()` / serde | §7 只给 Actor 列（"Human Owner/有权 Admin"） |
| DeliveryAttempt 的 **创建行** | §5 只有一句话的链，无创建行。`PENDING` 是链上唯一入度为 0 的状态，创建目标为**派生** |
| Approval / Message 的 actor 枚举 | 权限在事务里查（§5 分层）。自造 actor→权限映射等于自造权限语义 |

DeliveryAttempt 那三处**全部是「空」**，
`every_edge_declares_no_events_and_no_preconditions` 是它们的绊线：
后人想「顺手补全」会立刻看到测试红。

## 8. 目录里有、但 §5/§7 没用掉的码 —— 明确不许「用掉」

冻结目录另含 `MESSAGE_DELIVERY_FAILED`、`MESSAGE_REASSIGN_DENIED`、
`APPROVAL_ALREADY_DECIDED`、`APPROVAL_SELF_DECISION_DENIED`。

这四个**读起来都像是该用在对应的行上**（Delivery failed 行、Reassign 行、
Already-decided 状态、Self-decision 前置条件），但 §5/§7 的表里没有它们。
接上去是「改进」，实质是改协议契约。

测试显式断言它们**不出现**
（`the_catalogue_codes_spec_section_7_uses_but_we_do_not_invent`、
`error_codes_match_the_frozen_catalogue`），把「不许用掉」变成可执行断言。

## 9. 一个反直觉的冻结设定，原样保留

§7 的 `PENDING → CANCELLED` 是**合法边**，但其主要拒绝码是
`DOMAIN_INVALID_TRANSITION` —— 与「根本没有这条边」**同一个线名**。
含义是「取消被拒，因为已有决定」，属状态级拒绝挂在合法边上。

**没有改**。`ERROR_CATALOG.md` 原文「改名或改变语义属于破坏性变更」；
且目录里**恰好存在** `APPROVAL_ALREADY_DECIDED`，更说明这是刻意选择而非笔误。
测试 `a_refused_cancel_keeps_the_invalid_transition_wire_string`
把「合法边的拒绝」与「确实无此边」分开断言。

## 10. 派生 vs 冻结

| 位置 | 性质 |
|---|---|
| §5 四行拒绝码为 `—`（`QUEUED→NEEDS_HUMAN`、`QUEUED/ROUTED→EXPIRED`、`ROUTED→QUEUED`、`ROUTED→FAILED`） | **派生** `DOMAIN_PRECONDITION_FAILED` |
| §7 一行拒绝码为 `—`（`PENDING→EXPIRED`） | **派生** 同上 |
| DeliveryAttempt 全部拒绝码 | **无需派生**：§5 未给码，唯一可能的是「无此边」 |
| 三机的 `condition_key()` 取值 | **派生**（规格未规定标识符本身） |
| 全部状态名 / 事件名 / 错误码线名 | **逐字冻结**，19 个 §5/§7 事件名全部核验出自 `STATE_MACHINES.md` |

## 11. 遗留登记（未擅自改冻结规格）

1. **§5 `MESSAGE_REASSIGNED`/`MESSAGE_REQUEUED` 的标点约定未在规格中成文**。
   本实现据全规格标点规律拆为两条边。若 Human 认为该处 `/` 意为「两者都发」，
   只需把两条边合并为一条带两个事件的边，其余不受影响。
2. **目录中的 `MESSAGE_DELIVERY_FAILED` / `MESSAGE_REASSIGN_DENIED` /
   `APPROVAL_ALREADY_DECIDED` / `APPROVAL_SELF_DECISION_DENIED` 未被 §5/§7 引用**。
   登记为「目录比表宽」，不改表。
3. **§5 的 DeliveryAttempt 无创建行、无 Event 列、无拒绝码列**，实现按链推导。
   若 Human 认为需要显式创建行，属规格增补，不在本节点擅自做。
4. **`、` 与 `/` 的事件基数约定未在规格中成文**。本实现按 §2 的归纳拆边，
   理由与可逆性见 §2。若 Human 认为 `/` 应读作「都发」，改动仅限把
   `Reassign` / `RequeueNeedsHuman` 两条边合并为一条带两事件的边。

## 12. 门禁

- 平台：macOS ARM64（`--platform macos`，实测 `darwin/arm64`）
- 结果与逐轮台账：见 `~/.minimax/teamspace-audits/dev-02-08/evidence/`
  下 `GATE-LOG-dev0212-state-machines.md`（**按 DEV-02-11 J41 的结论，哈希记在仓库外**，
  仓库内只保留「取编号最大且非 `VOIDED-`/`SUPERSEDED-` 前缀的那一份」这条识别规则）

## 13. 节点关闭

`docs/development/DEVELOPMENT_NODES.md` 中 `DEV-02-12` 改为 **`DONE`**。

`DEV-02-13`（集中式权限求交与拒绝原因）依赖 `00-05`、`02-02`～`02-10`，已具备
进入条件；`DEV-02-14`（乐观锁、幂等与 Outbox dispatcher）依赖 `02-07`、`02-11`～`02-13`，
需等 `02-13` 关闭。**后续节点需 Human 显式批准后开工。**
