# DEV-02-10 遗留规格缺口补齐 —— 补充终审（2026-09-29）

> 结论：**DONE**。本记录是 [DEV-02-10 主终审](WAVE-2-DEV-02-10-CLOSURE-2026-09-29.md) 的补充轮，补齐该轮登记但未修复的两项规格缺口。
> 依据同主终审的规格与门禁要求。Human 于 2026-09-29 对三项待裁决回复「**按你的倾向解决**」。
> 本记录不重开主终审的任何结论。

---

## 授权与范围

主终审登记了两项规格缺口（本节点"知道有问题但按纪律停手"的部分）：

| 缺口 | 事实 | Human 裁决 | 本轮交付 |
|---|---|---|---|
| **A** Approval↔Permission 无对应关系 | 两表 `resource_type` 词表交集为空、**均无 `resource_id`**、全库无对应落点 | 按倾向 → **A1** | `approvals.permission_id` |
| **B** CANCELLED 取消 actor 无落点 | §7 要求「记录取消 actor」，但只有 `decided_by_human_id`（Human 专用），Role 取消表达不了 | 按倾向 → **B2** | `approvals.cancelled_by_actor_type` + `_actor_id` |

**排除的备选**（裁决时一并记录理由）：

- **A2**（统一 `resource_type` 词表）——需改 `0009` 的 CHECK，**已发布迁移的 SQLx checksum 不得漂移**；且会丢掉「高风险动作类别」这个独立语义。
- **A3**（11×6 映射表）——需定义多条策略的合并语义（AND 还是 OR），属发明语义。
- **B1**（只加 `cancelled_by_human_id`）——「Role 请求 + Role 取消」是 P0 常态，B1 记不下，等于半拉子。

**裁决时我留白的一问，已用 schema 事实排除**：原计划问「请求时如何确定对应的 `permissions` 策略」。查证发现 `0009` 对 `permissions` 只建了**普通**索引 `(team_id, role_id, resource_type, action)`，**无 UNIQUE 约束**——`(team_id, action, resource_type)` 可合法命中多行，「按 type+action 匹配唯一策略」在数据层不成立。故只做 A1 的直接形式：**显式 `permission_id`**，DB 层只保证关联存在且同 Team，「该选哪条策略」留给 DEV-02-12/13。

## 变更

| 文件 | 变更 |
|---|---|
| `crates/persistence-sqlite/migrations/0021_approval_policy_linkage.sql` | 新增（**232 行 / 3 个 ADD COLUMN / 9 个 trigger**） |
| `crates/persistence-sqlite/src/lib.rs` | 注册 0021 |
| `crates/persistence-sqlite/tests/permission_approval_integrity_forward_migration.rs` | 9 → **15 个用例**；新增 `V20OnlyMigrations` 基线证明源；`INSERT INTO approvals VALUES(...)` 全量改为**显式列名** |

`0001`～`0020` 与开工前备份**逐文件比对零差异**，SQLx checksum 不漂移。

### 为什么没有加 FOREIGN KEY

SQLite 的 `ALTER TABLE ADD COLUMN` **不允许**列上带 `REFERENCES`；要真外键只能重建整张表。改用 trigger 保证引用完整性，与 0020 处理 Team 一致性的方式一致。被引用的策略不可删除由 0020 的 `permissions_delete_guard`（无条件拒绝 DELETE）兜住。

### 两套 `resource_type` 词表：刻意保留并存

`approvals.resource_type`（6 值）问「这是哪一类高风险动作」，`permissions.resource_type`（11 值）问「作用在哪个对象上」。有了 `permission_id` 后对应关系由外键直接给出，**不需要映射表**，也不存在词表对不上导致授权失效的可能。统一词表要改 `0009` 的 CHECK 且丢语义，故不改。

### 9 个 trigger

**A1 侧**：`permission_link_insert` / `permission_link_update`（存在且同 Team——`permissions.id` 是全局主键，**外键本身挡不住跨 Team**）、`permission_immutable_update`（NULL → 值允许补上指向；值 → 另一个值禁止）。

**P4 追加**：`approvals_permission_effect_insert` / `_update`（`0021:142,151`）——
本节初稿写成 7 个，漏了 P4 修复新增的这两个。**实测 `grep -c "CREATE TRIGGER" 0021_*.sql` = 9。**

**B2 侧**：`cancellations_status_insert` / `_update`（只有 CANCELLED 才谈得上被谁取消）、`cancellations_identity_update`（type 与 id 同进同出；ROLE 须落到本 Team 真实 Role）、`cancellations_immutable_update`（终态后不可改写、不可抹除——与 0020 处理决定痕迹同一手法）。

`cancelled_by_actor_type` 沿用 `requester_actor_type` 词表（ROLE/HUMAN/SYSTEM），**不**采用 `events.actor_type` 的四值——那多出的 `ADAPTER` 对审批的请求/取消无法解释。

## 对抗审查：G 组

做法沿用已落仓的[对抗审查手册](../development/ADVERSARIAL_REVIEW_PLAYBOOK.md)：先回看覆盖面再设计向量。

**18 条攻击 + 6 条对照**，四轴（创建/变更/删除/替换）分法。

| 轮次 | 结果 |
|---|---|
| 基线（`0020` + 只加列的影子版，无 trigger） | **13 绕过** / 11 拦截 / 0 误伤 |
| 加固后（`0001..0021`） | **18/18 拦截** / 0 绕过 / 0 误伤 |

基线被挡住的 5 条来自已有机制：列级 UUIDv7 CHECK（2）、0020 的 `approvals_replace_guard`（1）、`permissions_delete_guard`（1）、状态机（1）。

**本轮补上的覆盖缺口**：G 组首版 15 条全部走「先建 PENDING 再 UPDATE」路径，**创建轴直接带列的情形整组缺失**——这与 DEV-02-08 初版「35 条全是 UPDATE」同类。补 G-16/G-17/G-18（INSERT 直接带跨 Team 策略 / 带取消痕迹 / 带悬空引用）后，创建轴才闭合。

## 装置三次自伤（本轮）

延续主终审的纪律，本轮又出了三次，全部作废重跑：

| # | 现象 | 根因 | 处置 |
|---|---|---|---|
| **1** | 4 条向量（G-06 / GC-02 / GC-04 / GC-05）**根本没加载影子版**：靠 `sed`/python 精确替换注入 `$SHADOW_ARG`，漏了 4 处，**语法检查通过、零报错** | 逐条向量手写注入参数 | 改由 `build_base` 从 `GAP_EXTRA` 环境变量统一加载，**物理上无从漏起** |
| **2** | **基线假性全拦（24/0）**——差点得出「G 组缺口不存在、0021 多余」 | `build_base` 的循环是 `00*.sql`，**`0021_*.sql` 以 `00` 开头被通配匹配**、无 case 条目 → 正式 0021 被无条件加载，影子版形同虚设 | case 增加 `0021_*` 分支；**并把开关变成可测的**：直接调用 `build_base` 断言「影子 = 0 trigger / 正式 = **9** trigger」，先证装置能区分两种模式再信输出 |
| **3** | G-11「抹掉取消者」判绕过，实为 **no-op** | seed 里的 CANCELLED 审批是 0021 加列**之前**种下的，两列本就 NULL，「抹成 NULL」= `NULL → NULL` | seed 增设 E4：一条**已带真实取消者**的 CANCELLED 审批，G-11/12/13 改打它 |

**第 2 条最值得记**：基线「全绕过」会推着我去写修复；基线「全拦」会让我**停止修复并认定缺口是臆想的**——一个让我不再干活的错误结论，代价远大于多写一个迁移。

**第 1、3 条是同一条老病的复发**（逐条传参必漏、no-op 被计为绕过）。两者的规则其实早立在主终审的 J11/J12，但**规则写在日志里不会自动生效**，最终还是靠人工排查发现，不是靠机制拦住的。

## 回归

| 项 | 结果 | 存档 |
|---|---|---|
| P 组（26 攻击 + 6 对照） | 32 拦截 / 0 绕过 / 0 误伤 / **0 装置故障** | `results-p-hardened.txt` |
| T 组（17 攻击 + 7 对照） | 24 拦截 / 0 绕过 / 0 误伤 / **0 装置故障** | `results-t-hardened.txt` |
| G 组（20 攻击 + 7 对照） | 27 拦截 / 0 绕过 / 0 误伤 / **0 装置故障** | `results-g-hardened.txt` |
| G 组基线（影子版） | 12 拦截 / **15 绕过** / 0 误伤 | `results-g-baseline.txt` |
| **三组合计攻击向量** | **63 条，加固后全部拦截** |
| `cargo test --workspace` | **230 passed / 0 failed**（主终审时 223，本轮 +7） |
| `cargo clippy --workspace --all-targets` | 零告警 |
| `cargo fmt --all --check` | 零差异 |

P/T 两组首轮回归曾报**装置故障 32**——共享装置的 `seed` 已新增一条依赖 0021 新列的行，而它们不加载 0021。契约一改所有调用方都得跟着改，这一条已写进 `lib.sh` 注释。

## 收尾核验轮（Human 要求「对以上内容核验」）

本节记录交付后的一次完整核验，以及核验**推翻**的自身结论。

### 独立核验的结论

派 verifier 独立重跑（不复用本轮任何输出），判 **PARTIAL**，抓出两个红项：

| 级别 | 问题 | 根因 |
|---|---|---|
| 🔴 **P1** | P 组**没有任何有效的加固后存档** | 重跑时我写了 `"results-${f%%-*}-regress.txt"`，`${f%%-*}` 对 `attack-0009-baseline.sh` 取到 `attack` —— **P 组与 T 组的结果写进了同一个文件**，后者覆盖前者。终审文档引用的 `results-p-regress.txt` 则是更早那次**全部 32 条装置故障**的存档 |
| 🔴 **P2** | 装置**不自我声明加载哪套 schema** | `0018/0019/0020/0021` 由 `INCLUDE_*` 开关控制且**默认 0**，而脚本只 export 了 `INCLUDE_0021=1`。直接跑脚本会得到「24 绕过 / 16 绕过 / 2 绕过」——**汇总行形状与真结果完全一样**，照出「0 误伤」 |
| 🟡 **P3** | `build_base` 的两个 glob 重叠 | `"$MIG"/00*.sql` 已匹配 `0010~0017`，`"$MIG"/001*.sql` 又匹配一遍，同一文件 apply 两次；错误被 `|| true` 吞掉，使「应用失败」与「已应用」无法区分 |
| 🟡 **P4** | 0021 的跨 Team 防线**寄生在 0020 上** | 两个 `permission_link_*` trigger 都是 `BEFORE UPDATE OF permission_id`，单独改 `team_id` 不触发；真正兜住的是 0020 的无条件 `team_id` 禁令 |
| 🟢 **P5** | 门禁证据不覆盖攻击装置 | 装置在仓库外（`~/.minimax/…`），其漂移不在 manifest 范围内 |

P1 与 P2 属同一族：**输出形状与真结果一致，所以错误不可见**。P2 尤其值得记 —— 它与 playbook 反复警告的缺陷同族，只是这次发生在**加固后一侧**而非基线侧。

### 处置

- **P1** 三组全部重跑，存档改用**正确且互不覆盖**的文件名（`results-p/t/g-hardened.txt`）。
- **P2** 装置改为自声明：`build_base` 新增 **schema 双向断言** ——
  期望集合由 `EXPECT_SCHEMA` **显式声明**；声明里有的 trigger 必须存在，
  **没声明的必须不存在**（后者防的正是 J19 的原始事故）。
  三个攻击脚本各自 `export EXPECT_SCHEMA`，不再依赖调用方在命令行记三个变量。
  另配 `work/selftest-p2.sh`（**7 个用例**）证明这条断言真的会拦 ——
  包括「0021 存在但未声明」的反向用例。该自检在编写过程中直接测出两处设计缺陷
  （漏加载时条件为假等于没断言；`local a b=""` 在本机 bash 下不初始化 `a`），
  均已修正。**这是本轮唯一一处「规则在写下的当场就被机制验证」的改动。**
- **P3** 改为单一 glob + `sort` + case 过滤。
- **P4** 在 `0021` 文件内写明这条依赖关系，标注「修改 0020 前请先处理本节」。
- **P5** 不改（装置置于仓库外是有意选择），登记为遗留。

### 我自己核出的一个问题（verifier 未覆盖）

构造「0020 时代就有数据的库 → 应用 0021」的场景，测出**补记边界**：

- 先 `UPDATE status='CANCELLED'` 再补 `cancelled_by_*`（两步走）→ **被拦**
- 同一次 UPDATE 内写 `status` 与 `cancelled_by_*`（§7 要求的路径）→ 畅通

**这是设计边界，不是缺陷**，理由与对下游的硬约束已写进 `0021` 迁移文件注释：
DEV-02-12 必须把 actor 写入与状态迁移放在**同一次 UPDATE**；
存量回填应从 `APPROVAL_CANCELLED` 事件的 actor 取可信值，而非让 DB 接受无来源的手写值。

### 核验同时暴露的自身问题

写核验脚本时我连续踩了两个**已写进 playbook** 的坑：`00*.sql` 把 0021 一并装了两遍；
以及用 19 列的表配 16 值的 `INSERT ... VALUES(...)`，所有 INSERT 静默失败、
UPDATE 命中 0 行返回 0，差点把「补写被冻结」误判成「补写被放行」。

**两次都发生在「核验」这种看起来最该小心的场合。** 结论是：
playbook 立了规矩之后，**只要人不主动去读它，它就等于不存在**。
真正有效的只有 P2 那种「写完当场自测」的机制。

## 裁决轮（Human 授权「后续裁决按倾向来，做好记录，往下做」）

本节是本节点第三轮工作。授权后，凡规格层歧义由本节点裁决并记录，不再逐条停手回报。
**授权范围仅限规格歧义的裁决**，不改变「不提交、不改冻结规格主体、不发明新领域概念」三条底线。

### 裁决一：`PERMISSION_MATRIX §6` 优先于 `DOMAIN_MODEL:67`

两者对 Approval 字段集的列举不同。**裁决以 §6 为准，`DOMAIN_MODEL:67` 是索引摘要，不构成字段集定义。**

依据：① §6 用「**必含**」措辞，是规范性列举；`DOMAIN_MODEL:67` 位于索引表的一行，中列为核心字段简写，**全文无一处声称是完整字段集**。② 同文件 `DOMAIN_MODEL:50` 把 Permission Policy 写成「Team/Role/路径/动作」四个词覆盖 11+10 个枚举值，可见该表本就是概括。③ 既有实现已按 §6 走（`0009` 建了 `resource_type`，`0021` 按 §6 加 `permission_id`）；以 `DOMAIN_MODEL:67` 为准等于回退两个迁移。

**处置：不改 `DOMAIN_MODEL`**（冻结规格，且不存在事实错误）。本裁决作为 DEV-02-12 的字段依据。

### 裁决二：Approval 只能指向 `effect='ALLOW'` 的策略 —— 由此发现一条真绕过

原计划第三问（「请求时选哪条 `permissions` 策略」）已被 schema 事实排除自动匹配
（`permissions` 无 UNIQUE 约束，同一 `(team_id, action, resource_type)` 可合法命中多行），
只剩显式 `permission_id`。选定显式指认后必须回答**能指向什么**。

**裁决：只能指向 `effect='ALLOW'` 的策略。** 依据 `PERMISSION_MATRIX §2` 末句：

> Approval 只能缩小或临时补充**已允许的**动作，不能突破 Team 安全上限。

随即实测出**这条约束此前没有任何守卫**：审批指向同 Team 的 `effect='DENY'` 策略**被放行**。
语义后果是应用层只要看到「有一条 Approval 指向该策略」就可能据此放行，
那等于让审批**突破 Team 安全上限** —— 正是 §2 禁止的那件事。

**处置**：`0021` 新增 `approvals_permission_effect_insert` / `_update` 两个 trigger
（7 → 9），错误码 `Approval cannot be granted against a denied policy`，不与跨 Team 复用。
G 组补 G-19（UPDATE 路径）、G-20（INSERT 路径）与 GC-07（指向 ALLOW 的对照）；
seed 增加一条**同 Team** 的 DENY 策略（CD）—— 必须同 Team，
否则跨 Team 与 DENY 两个条件会同时成立，测不出是哪一条在拦。
G 组基线因此由 13 绕过升至 **15 绕过**，加固后 20 条全拦。

**这个缺口是「裁决」逼出来的，不是「审查」逼出来的。**
显式指认方案里「能指向什么」原本是空白，而空白处正好是 §2 禁止的行为。
若当初选了另两个备选方案（A2 统一词表 / A3 映射表），同一问题会以另一种形式留在那里。

### 装置机制化：复用探针

本轮实测环节我**第四次**用 `INSERT INTO approvals VALUES(...)` 位置插入，
列数一变即静默失败、UPDATE 命中 0 行返回 0，把「被拦」误判成「放行」。
该规则此前已写进判断日志与 playbook，**三次规则、三次照犯**。

**机制化处置**：新增 `probe.sh`，提供 `probe_new_db` / `probe_exec` / `probe_assert` /
`probe_seed_teams` / `probe_policy` / `probe_approval` 六个函数，**全部使用显式列名**，
调用方不再手写 INSERT；`probe_assert` 对每条 SQL 明确打印「放行/拦截」，不吞结果。

它自身也出过两次错（路径拼成 `work/work/…` 导致 db 打不开，于是「拦截」也是假的；
`set -u` 下 `PROBE_DB` 未初始化）。两次都是**先看到输出形状不对、回头查**才发现 ——
说明「打印明确结果」比「返回退出码」更能暴露装置问题。

## 遗留

1. **「请求时该选哪条 `permissions` 策略」** —— 已由裁决二把范围收窄为「只能指向 ALLOW 策略」，
   但**多条 ALLOW 策略并存时选哪一条**仍未定义，属 DEV-02-13 求交逻辑。DB 层只保证关联自洽
   （存在、同 Team、`effect='ALLOW'`、不可换绑），选型逻辑不在本节点发明。
2. Human 身份仍无 DB 层落地（沿用 DEV-02-09 / DEV-02-10 的裁决）。
3. `adapter-supervisor::node_mock_adapter_fault_crash` 并行下偶发失败，沿用既有登记。
4. `PERMISSION_MATRIX §6` 与 `DOMAIN_MODEL:67` 的字段集差异**已由裁决一裁定**（以 §6 为准，
   `DOMAIN_MODEL:67` 为摘要）。若将来要让 `DOMAIN_MODEL` 承载完整字段集，属规格侧改动，
   应先更新 §6 再同步，而不是反过来删 §6 的要求。
5. **门禁证据不覆盖攻击装置**（P5）。装置与判断日志位于 `~/.minimax/teamspace-audits/`，不在仓库内，因此其漂移无法被门禁 manifest 发现。若要让装置本身也受门禁约束，需先把它们纳入仓库或另建校验。
6. **0021 的跨 Team 防线寄生在 0020**（P4）。当前两种 actor 下均实测无绕过，但这是已知的耦合点，不是不变式。

## 门禁证据

| 文件 | 说明 |
|---|---|
| `~/.minimax/teamspace-audits/dev-02-08/evidence/wave2-macos-arm64-dev0210-verify.json` | 核验轮门禁（21/21，输入 `7369bbd7…`，276/276 逐文件一致） |
| `~/.minimax/teamspace-audits/dev-02-08/evidence/wave2-macos-arm64-dev0210-gap.json` | 缺口补齐轮门禁 |
| `~/.minimax/teamspace-audits/dev-02-10/results-p-hardened.txt` | P 组加固后 |
| `~/.minimax/teamspace-audits/dev-02-10/results-t-hardened.txt` | T 组 |
| `~/.minimax/teamspace-audits/dev-02-10/results-g-baseline.txt` | G 组基线（13 绕过） |
| `~/.minimax/teamspace-audits/dev-02-10/results-g-hardened.txt` | G 组加固后（18/18） |
| `~/.minimax/teamspace-audits/dev-02-10/work/selftest-p2.sh` | 装置 schema 断言自检（7 用例） |
| `~/.minimax/teamspace-audits/dev-02-10/JUDGMENT-LOG.md` | 判断日志 J0～J20 |
