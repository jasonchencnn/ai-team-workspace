# DEV-02-09 范围、历史兼容与限定整改审查

日期：2026-10-10。执行/复核：Codex 主控串行执行，采用第二遍真实反例和失败注入；未调用子模型，不声称独立审核人签署。本轮使用 rust-testing 技能。关联节点 DEV-02-09，关联 FR-043 的基础存储；依赖 DEV-02-01、07、08 均 DONE。

**结论：限定存储整改 PASS；DEV-02-09 仍 BLOCKED。** 当前正式 **35 DONE / 6 BLOCKED / 60 PLANNED**，Gate C 仍 BLOCKED。已修复两个真实存储缺陷；可信 Human 身份执行点未闭环，不以“不是 Role”代替身份认证。11 类通知及完整策略执行继续由 DEV-05-08 承担，未把全部应用功能倒挂为基础节点前置，也未接受 ADR-0007。

## 责任核对

| 项目 | 冻结依据 / 当前证据 | 归属与结论 |
|---|---|---|
| 表、索引、Role/Human 身份碰撞 | 0008/0019/0023；Role 接收者及反向 Role 重用均拒绝，原始通知专项复验 | 09 存储责任；现有防御保留，但不证明任意非 Role UUID 是 Human |
| Event/Message/Policy 与通知引用 | 0019 同 Team、0023 源不可变，0014 Event subject 约束 | 09 存储责任；原测试全绿，不新增 ATTENTION Event subject 类型 |
| 通知事实留存 | DATA_CONVENTIONS §6、STATE_MACHINES §6：去重不删除事实 | 09 存储责任；0034 关闭异 ID 去重替换路径 |
| Attention 多态引用的两端与旧库 | DOMAIN_MODEL §2/5 稳定身份、同 Team 引用；0019 只校验 Attention 写入 | 09 存储责任；0035 关闭 Task 父端移 Team，升级拒绝四类旧跨 Team 引用 |
| 可信 Human / Policy 接收者 | DOMAIN_MODEL §2/3：P0 Owner，recipient Human；现 host/security 仍无可信身份执行点 | 必须闭环的身份缺口；最小后续任务见 [Owner 决策](../coordination/DEV-02-09-OWNER-DECISION-2026-10-10.md)，不以任意 UUID、自报 Owner 或角色排除代替 |
| 11 类通知、去重窗口、过滤/提醒执行、事务审计及工作台重建 | REQUIREMENTS_TRACEABILITY 将 FR-043/通知触发分配给 05-08；02-12～14 承担状态/权限/事务基础 | 后续节点责任，不宣称已实现或提前启动依赖用例 |
| 参数字典、mandatory 成员、SYSTEM 事实源、retry/EXPIRED 关闭时点 | ADR-0007 仍 Proposed | 未决语义；只登记，不从一般执行授权推断接受 |

FC-07 / MM-R08 为回传建议，不是冻结事实源。两项纠正：把 `PERMISSION_DENIED` 选为唯一 mandatory 成员是 ADR-0007 的待决条款，不能借“A 类补字典”自动接受；`attention_type` 与 `source_type` 是不同维度，现有 `SECURITY_ALERT + SYSTEM` 已可落库，并非机械地缺一个 SYSTEM 状态值。SYSTEM 的映射与处置凭据仍需决策。Attention 属派生索引，没有擅加永久 DELETE 守卫；删除索引也不等于解决源事项。

## 真实缺陷及最小修复

**N34：异 ID 的活跃通知替换。** v33、外键 ON、recursive_triggers OFF，旧通知 UNREAD 或 READ 时，新 ID 的 `INSERT OR REPLACE` 命中 `(team_id,recipient_human_id,dedupe_key)` 部分唯一索引，实测旧 ID 行消失、新 ID 行存在。0019 的同 ID 守卫未命中；0023 的 DELETE 守卫不在隐式替换删除时触发。

0034 新增一个 BEFORE INSERT 守卫覆盖该活跃 secondary key。同 ID 仍交给原守卫；NULL key、不同 Team/recipient/key 和终态释放槽仍合法。守卫对冲突的 OR IGNORE 也返回 ABORT，调用方需显式查询并复用呈现；当前无生产 Notification writer 依赖 IGNORE/REPLACE。没有解释新的去重窗口或策略字典。

**A35：Task 父端破坏 Attention 引用。** v34 真实 SQLx 测试构造无 Run/Event 历史的 Task 与 OPEN Attention，一次 UPDATE 同时改 Team 和 owner，实测命中 1 行并留下 1 条跨 Team Attention。0019 升级扫描对四种 source 只查存在，旧 v18 跨 Team 记录还能升级到 v34。

0035 用事务内临时升级守卫检查 TASK/RUN/MESSAGE/APPROVAL 的同 Team 来源存在性，脏历史拒绝、业务行不改写；新增一个 Task Team UPDATE 守卫，覆盖 OPEN、DISMISSED 与历史 RESOLVED 引用。Run、Message、Approval 的 Team 已有守卫，不重复添加。升级不依据当前源状态否定历史解决；SYSTEM 映射不变。

工程写集仅六个文件：新迁移 0034/0035、新专项 `notification_dedupe_retention.rs` / `attention_source_reference_integrity.rs`、串行注册 `src/lib.rs`、HC-01 兼容测试的加载口径维护。0034 为 36 行/1 trigger/0 index；0035 为 38 行/1 trigger/0 index，升级 guard 表创建后删除。根 manifest/lockfile、0001～0033、冻结规格、旧 fixture 和交接原件保持。

HC-01 测试维护：当前生产链的检查改为 0033 成功记录恰为 1；32→33 专项固定加载到 CANDIDATE=33，保留 ledger 精确 +1、整行比较和幂等断言。没有把“33”改成“35”凑绿。其余真实改派/换绑行为仍加载完整生产链并通过。

## 验证与原始证据

仓外证据根：`/Users/chenzhiqing/.codex/visualizations/2026/10/10/01a1237c-0b1c-7121-9922-cc68642a02ff/dev02-09-scope-review`。

| 验证 | 当前结果 | 证据 |
|---|---|---|
| 原三套通知专项（8+3+14） | 25/0 | `baseline.log`；最终全包及 Mac21 再覆盖 |
| 新通知/引用专项 | 7+6=13/0；含 v33/v34 危害对照、合法路径、整行不变、升级与重复执行 | `compat-green-03.log` |
| HC-01 兼容专项 | 11/0 | 同上；最终 Mac21 再覆盖 |
| 新注册契约 | 2/0；33 原注册契约仍通过 | 全包 lib 3/0 |
| 两个 SQLx 升级失败注入 | DDL 创建后注入失败，schema、业务行、ledger 全部恢复 | 两个新专项中的 post-CREATE failure 测试 |
| 第二遍反例 | 六项各实际编译、0/1、exit101，恢复输入 SHA 全匹配 | `counterexamples-02/results.json` 与逐项日志；仓内 [驱动](evidence/dev02-09-scope-2026-10-10/run_counterexamples.py) |
| 恢复后 persistence 全包 / fmt / clippy | 303/0；fmt/clippy exit0 | `counterexamples-02/restored-*.log` |
| 当前 stable Mac21 | **整体 PASS，21/21，Rust723/0/1，TS36/0；74 Rust result blocks** | `gate-01/mac21.json`、`.log`、`gate-summary.json`；[驱动](evidence/dev02-09-scope-2026-10-10/run_mac21.py) |
| companion raw digest | 历史全 refs 扫描与 tooling guards 各前后两份摘要相等，均与记录值相等 | `gate-01/companion-verification.json` |
| 当前 C7 真实 branch | Domain 行1937/2113=91.67%，branch211/226=93.36%；Application 行150/160=93.75%，branch18/18=100% | `coverage/summary.json`、LLVM export / profile 原件；复用已接受 ADR-0009 的固定隔离 nightly，生产 stable/default 不变 |

六项反例分列：3 项危害（去重守卫禁用、READ 槽漏保护、Task 父端守卫禁用），1 项合法对照误伤（不同 key 被拒），1 项脏历史升级遗漏，1 项事务注册契约。注册契约不证明 SQLite driver 使用 `no_tx` 字段；事务性以实际 SQLx post-CREATE 失败回滚验证。

Mac21 受检 **4790 entries**，identity `333eacb6ae89ddfd13d4da3dc700b473cb21348937c61c620167aefeae6c6c28`；完整输入前后相同，**226 项工程/扫描策略**前后相同。开工 222 项中仅 lib.rs 与兼容测试改变，新增工程 4 项。另与 HC-01 保存输入比较的 62 项旧迁移/规格/ADR/根锁等无漂移，0033 与本轮开工快照匹配。

失败轮保留，不覆盖：首个 RED 先命中了 UNIQUE 诊断差异，随后调整顺序，在 `red-behavior.log` 中实测替换返回 `changes:1`，另以 v33 行消失证明危害；`green-02.log` 中错误假设 Message 可以移 Team导致 4/2，确认既有守卫后移除重复实现；全包首轮及副本首轮捕获 HC-01 最新版本硬编码，修正后完整重跑。早期 Python 探针三次种子构造失败没有形成结论，不计入反例。正式结论使用已自检的 SQLx 测试。

## G0～G9 与节点准入

| Gate | 本轮判断 |
|---|---|
| G0 | 限定整改范围明确、直接依赖 DONE；身份未闭环，整节点不晋级 |
| G1/G2/G3 | 当前 Mac21、真实测试、fmt/clippy/build 全部通过 |
| G4 | 两个迁移真实失败注入零副作用、旧行保留、重复迁移通过；不宣称已有生产备份恢复演练 |
| G5 | Role 碰撞与同 Team 引用通过；可信 Human/Owner 执行点未实现，整节点 BLOCKED |
| G6 | SQL/内部测试无 UI；体验与 A11y 不宣称通过 |
| G7 | 既有 Event 与关联事实保留；真实通知权限拒绝 Event/消费事务属于后续执行点，未宣称完成 |
| G8 | 新约束为引用存在性查询；未声称已验证 1 万记录/P95 性能，后续由 DEV-08-02 验收 |
| G9 | 当前报告、可复跑驱动与机器摘要齐全；输出后工程 SHA 另核，报告不冒充 Mac21 受检文档 |

文档、节点入口及 [机器摘要](evidence/dev02-09-scope-2026-10-10/result.json) 为 gate 后输出，改变了全仓输入；因此不声称当前全部文档仍是受检的 4790 entries。工程/策略 226 项另做输出后核对与工作树秘密扫描。

回退：0034/0035 是前向迁移，每条 SQLx migration 各自事务；失败保持该条执行前的 ledger/schema/业务行。若 0034 已成功而 0035 拒绝，ledger 停在 34，不冒称整条链回到 33。生产升级前须一致性备份，回退恢复备份，不删除事实或修改已发布 SQL。本轮只在合成测试库测升级，没有生产库/凭据操作，也没有提交、推送、部署、外发或新增费用。

下一步只解阻可信 Human 身份：主程序受信 Owner → 通知/Policy 接收者执行校验，不能接受 IPC/Adapter 自报。详见独立决策材料；ADR-0007 其余条款分别归属后续策略和处理用例。未决期间 09 与 Gate C 保持 BLOCKED，依赖节点不开工。DEV-03-07 的独立执行链可另按已 DONE 前置签发任务，本轮没有扩入该节点。
