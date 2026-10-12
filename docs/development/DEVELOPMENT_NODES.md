# 细化开发节点、依赖与模型路由

> 2026-10-10 DEV-02-09最新：[限定整改审查](../reviews/DEV-02-09-SCOPE-REVIEW-2026-10-10.md)已修复通知异ID去重替换与Task父端跨Team引用，当前迁移0001～0035。新专项13/0、注册2/0、恢复后persistence303/0，当前stable Mac21整体PASS（21/21，Rust723/0/1、TS36/0，226项工程/策略输入不变），当前C7两层行/分支均达标。可信Human身份未闭环，09仍BLOCKED；正式 **35 DONE / 6 BLOCKED / 60 PLANNED**、Gate C BLOCKED不变。最小后续为[Owner身份决策](../coordination/DEV-02-09-OWNER-DECISION-2026-10-10.md)，不把全部05-08用例倒挂为09前置，不接受ADR-0007或恢复整批外派。下列HC-01等按历史时点保留。


> 2026-10-10 HC-01终审最新：DEV-02-08基础存储恢复DONE，[终审](../reviews/DEV-02-08-HC01-CLOSURE-REVIEW-2026-10-10.md)确认正式0033、完整历史及改派后换绑；原57+兼容11+注册1、全persistence288/0，当前stable Mac21整体PASS，Rust708/0/1、TS36/0，222项工程/扫描策略输入不变。正式 **35 DONE / 6 BLOCKED / 60 PLANNED**；Gate C仍BLOCKED，未启动下游产品用例。当前覆盖率Domain行91.67%/分支93.36%、Application行93.75%/分支100%。以下34/7/60、HC-01待回传等按历史时点保留。

> 2026-10-10 历史/兼容核对最新：[08专项审查](../reviews/DEV-02-08-HISTORY-COMPAT-REVIEW-2026-10-10.md)现有六套57/0，旧历史保留；新仓外回归确认改派到B后B换绑无法采用当前Binding，HC-01阻断08。已签[Minimax专项包](../coordination/MINIMAX-HISTORY-COMPAT-PACKET-2026-10-10.md)供Human转发，仅仓外修复候选与互斥回传目录，不恢复整批外派。生产工程219项SHA不变，本轮未重跑Mac21或重签节点；正式 **34 DONE / 7 BLOCKED / 60 PLANNED**、Gate C BLOCKED不变。后续权限/幂等/dispatcher/产品用例仍按12/13/14/05/07归属，不倒挂为08新前置。以下按日期保留历史。

> 2026-10-10 07终审最新：按冻结专属标准完成[责任核对与单节点终审](../reviews/DEV-02-07-SCOPE-CLOSURE-REVIEW-2026-10-10.md)，07基础节点恢复DONE；正式 **34 DONE / 7 BLOCKED / 60 PLANNED**。58专项/七项真实反例、当前stable Mac21整体PASS，Rust696/0/1、TS36/0，219项工程输入不变。原先笼统列入07的后续格式扩展、dispatcher和host接线已按冻结依赖归还02-14/05/07后续验收，不新增语义或启动下游。Gate C仍阻塞；下列07 BLOCKED和33/8/60等段落为历史时点。


> 2026-10-10 分支补测最终：四个Application遗漏结果已覆盖，Domain行91.67%/branch93.36%，Application行93.75%/branch100%；当前受检输入stable Mac21整体PASS，Rust696/0/1、TS36/0。依据[本轮验收及单节点复核](../reviews/DEV-02-07-BRANCH-TEST-REVIEW-2026-10-10.md)，仅DEV-02-11恢复DONE，正式 **33 DONE / 8 BLOCKED / 60 PLANNED**。07整节点与Gate C继续阻塞；生产源码、迁移、manifest/lock和用户stable/default不变，工程仅补测试。下列旧32/9/60、branch未证或77.78%均为历史时点。


> 2026-10-10 C7最新：Human已接受ADR-0009，官方候选核验、仓外隔离安装与单路/双路真实branch探针PASS；[专项验收](../reviews/C7-BRANCH-COVERAGE-REVIEW-2026-10-10.md)测得Domain行91.67%/分支93.36%，Application行91.25%/分支77.78%。Application分支未达85%，Gate C继续阻塞；正式 **32 DONE / 9 BLOCKED / 60 PLANNED** 不变，未批量重签节点。四个未覆盖结果及补测方向已记录；生产源码、lock与用户stable/default不变。下列“branch未证/工具待Human决策”仅属历史时点。


> 版本：1.0  
> 原则：节点按依赖推进；每个节点必须同时通过本表“审核标准”和 `QUALITY_GATES.md` 的通用门禁。

> 2026-09-28 派工冻结提示：已审核节点逐批按[前置节点终审](../reviews/WAVE-2-ENTRY-CLOSURE-2026-09-28.md)、[第一批终审](../reviews/WAVE-2-BATCH-1-CLOSURE-2026-09-28.md)、[第二批终审](../reviews/WAVE-2-BATCH-2-CLOSURE-2026-09-28.md)及[第三批终审](../reviews/WAVE-2-BATCH-3-CLOSURE-2026-09-28.md)有条件关闭。
> 2026-09-29 更新：DEV-02-07 由 [DEV-02-07 终审](../reviews/WAVE-2-DEV-02-07-CLOSURE-2026-09-29.md) 关闭。DEV-02-08 由 [DEV-02-08 终审](../reviews/WAVE-2-DEV-02-08-CLOSURE-2026-09-29.md) 关闭，同日经[重签](../reviews/WAVE-2-DEV-02-08-RESIGN-2026-09-29.md)补上同模型独立审核发现的创建轴缺口。DEV-02-09 由 [DEV-02-09 终审](../reviews/WAVE-2-DEV-02-09-CLOSURE-2026-09-29.md) 关闭。DEV-02-10 由 [DEV-02-10 终审](../reviews/WAVE-2-DEV-02-10-CLOSURE-2026-09-29.md) 关闭。四者均以第二遍对抗性反例审查替代独立复核并显式登记该降级。其余节点仍按[冻结清单](../v2/TASK_FREEZE.md)和依赖逐项准入，不因已有代码或自验收批量晋级。

## 1. 模型分配规则

| 复杂度 | 执行模型 | 使用范围 |
|---|---|---|
| 复杂 | `gpt-5.6-sol` | 架构、领域不变量、状态机、权限、安全、并发、恢复、协议、关键 E2E 与 Go/No-Go。 |
| 普通 | `gpt-5.6-terra` | 边界清晰的业务实现、CRUD、页面、查询、集成测试和常规修复。 |
| 简单 | `gpt-5.6-luna` | 脚手架、Token 搬运、生成物校验、静态页面骨架、文档和机械性测试补齐。 |

模型不能因“改动行数少”自动降级：触及权限、凭据、迁移、租约/fencing、审计、恢复或协议兼容的节点一律使用 `gpt-5.6-sol`。若一个节点实际范围扩大，先拆节点或升级模型，不得降格执行。

2026-09-24 Human 接受 ADR-0004 后，上表与节点“模型”列保留为历史复杂度参考；Wave 2 尚未开始节点的实际外部模型路由按 `docs/adr/0004-cross-model-r3-routing.md` 执行。R3 独立复核、失败注入、节点依赖、G0～G9 和总控最终验收不因此降低。

## 2. 总体依赖

```text
DEV-00 决策与契约冻结
  ├── DEV-01 工程底座
  ├── DEV-02 领域与持久化
  └── DEV-03 Adapter Spike
          ↓
DEV-04 Workspace / Artifact
          ↓
DEV-05 应用用例与 IPC
          ↓
DEV-06 前端体验
          ↓
DEV-07 纵向闭环与异常恢复
          ↓
DEV-08 系统验收与候选发布
```

### 当前执行状态

> 2026-10-10 最新执行：DEV-02-07 原子消费/去重、投影重建及三处真实进程崩溃重放已在限定范围通过，[验收](../reviews/DEV-02-07-AUDIT-CONSUMER-REVIEW-2026-10-10.md)为当前 canonical Mac21 PASS，Rust692/0/1、TS36/0。当前迁移0001～0032；行覆盖率Domain91.90%、Application93.41%，branch仍未证。正式 **32 DONE / 9 BLOCKED / 60 PLANNED** 不变；dispatcher、广义历史格式/host装配、通知和完整阶段未准入。下列旧统计和迁移范围按历史时点保留。

> 本轮最新执行：DEV-02-07 内部 Message 到期事务子包已通过，[验收](../reviews/DEV-02-07-ATOMIC-EXPIRY-REVIEW-2026-10-09.md)为 canonical Mac21 整体 PASS、Rust674/0/1、TS36/0，13项应用真库测试与3项行为反例通过。当前迁移0001～0031；application不再仅为占位，但consumer/dispatcher、可信Human/Role权限、PENDING到期语义与覆盖率仍未完成。正式 **32 DONE / 9 BLOCKED / 60 PLANNED** 不变；旧段落数字和迁移范围按历史时点保留。

> C6后的执行进展：DEV-02-08前向0030与限定存储整改PASS，当前最终Mac21整体PASS（Rust661/0/1、TS36/0）；见[0030验收](../reviews/DEV-02-08-ROUTE-SCHEMA-REVIEW-2026-10-09.md)。08完整节点继续BLOCKED，正式32 DONE / 9 BLOCKED / 60 PLANNED不变；当前生产迁移为0001～0030，旧29版加载器回退不可用，应用权限/投递责任未准入。

> 2026-10-09 C6逐节点重签最新覆盖：按直接依赖与专属标准，恢复DEV-01-05/08、DEV-02-02～06/10、DEV-03-02～05共12个底座节点为DONE。现行 **32 DONE / 9 BLOCKED / 60 PLANNED**；本轮当前Mac21整体PASS，剩余执行链、改派历史、通知未决及branch证据继续阻断相应节点和阶段。详见[C6/C7准入审查](../reviews/C6-C7-ADMISSION-REVIEW-2026-10-09.md)。以下20/21/60为本轮重签前历史快照。

> 2026-10-09 Codex内部收口最新覆盖：本轮整改及完整canonical Mac21整体PASS（21/21及两个companion前后匹配），进程fixture/crash与脱敏等旧故障已在限定审验范围关闭。正式 **20 DONE / 21 BLOCKED / 60 PLANNED** 不变；下列FC及更早记录为历史输入，不继续把已闭环故障或完整Mac21列为当前未修/未验。当前剩余节点差距和后续顺序见[进度与计划](../coordination/PROGRESS_AND_NEXT_PLAN-2026-10-09.md)，依据[收口审计](../reviews/CODEX-AUDIT-CLOSURE-2026-10-09.md)逐节点复核，未批量晋级。

> 2026-10-09 FC回传复核：六对象留存与Run终态矩阵已通过局部独立审查，不能再把这两项旧缺口列为当前未修；但完整当前Mac门禁与相关执行链仍未准入，FC04新增%字节单层编码漏检使DEV-01-08继续BLOCKED。现行仍20 DONE / 21 BLOCKED / 60 PLANNED，无节点恢复DONE。见[FC审计](../reviews/FC-RETURN-AUDIT-2026-10-09.md)及[精确接续](../coordination/MINIMAXCODE-AUDIT-FOLLOWUP-2026-10-09.md)；下方10-08为历史快照。

> 2026-10-08 Minimax回传独立审计：本批仍未准入；新增无子记录保护的Team/Role/Task/Binding留存阻断，DEV-02-02/03/04重开为BLOCKED。现行为 **20 DONE / 21 BLOCKED / 60 PLANNED**；下方23/18为上轮快照。十包裁定见[交付审计](../reviews/MINIMAX-RETURN-AUDIT-2026-10-08.md)，接续只按[补充任务表](../coordination/MINIMAXCODE-FOLLOWUP-2026-10-08.md)整改；旧终审不回写、ADR-0007仍Proposed。

> 2026-10-07 总控复核：DEV-02-08/09/13/14、DEV-03-06/07 的现行状态为 `BLOCKED`，历史 DONE 留作审查过程，不再构成当前派工准入。原因及精确修复写集见 [整改方案](../coordination/REMEDIATION-PLAN-2026-10-07.md)。当前输入的完整门禁、执行点授权/事务/审计与缺失证据未关闭前，不启动受影响依赖节点。

> 2026-10-08 补充复核：DEV-02-07 当前也为 `BLOCKED`。Outbox 默认重试 5 与冻结的 3 冲突已用前向 0024 修复并通过局部验收；当前完整门禁及应用原子执行链未验收，历史条件性 DONE 不放行依赖。见 [本批验收](../reviews/OUTBOX-DEFAULT-REMEDIATION-2026-10-08.md)。

> 2026-10-08 深入审计：当前状态以 [全项目深入复核](../reviews/DEEP-AUDIT-2026-10-08.md) 和 [Minimaxcode任务表](../coordination/MINIMAXCODE-TASKS-2026-10-08.md) 为准。新增直接阻断 DEV-01-05/08、DEV-02-05/06/10、DEV-03-03；依赖暂停 DEV-02-11/12、DEV-03-02/04/05。与原有七个阻断合计为 23 DONE / 18 BLOCKED / 60 PLANNED。纯规则和历史交付保留，不能据历史DONE启动依赖。

| 范围 | 状态 | 证据 |
|---|---|---|
| DEV-00-01～10 | `DONE` | `docs/reviews/DEV-00-FREEZE-REVIEW.md` |
| DEV-01-01～04、DEV-01-06～07、DEV-02-01、DEV-06-01 | `DONE` | Wave 1 独立复核中保持通过 |
| DEV-03-01 | `DONE` | ADR-0002 机器约束补强、全部协议反例与 Wave 1 独立复核通过 |
| DEV-01-05、DEV-01-08 | `DONE`（2026-10-09 C6逐节点重签；原BLOCKED与历史证据保留） | [C6重签审查](../reviews/C6-C7-ADMISSION-REVIEW-2026-10-09.md)；当前完整Mac21与专项证据通过。以下为历史记录： `docs/archive/wave2-execution/WAVE-2-HANDOFF.md`；旧双环境证据仅证明历史输入。DEV-01-05 当前 deleted 路径修复见 `docs/reviews/WAVE-2-PREBLOCKER-REPAIR.md`，不替代最终 Mac Gate；现行缺陷及证据见[深入审计](../reviews/DEEP-AUDIT-2026-10-08.md) |
| DEV-01-09 | `DONE`（历史审核） | `docs/archive/wave2-execution/WAVE-2-HANDOFF.md`；旧双环境证据仅证明历史输入。DEV-01-05 当前 deleted 路径修复见 `docs/reviews/WAVE-2-PREBLOCKER-REPAIR.md`，不替代最终 Mac Gate |
| DEV-02-02 | `DONE`（2026-10-09 C6逐节点重签；原BLOCKED与历史证据保留） | [C6重签审查](../reviews/C6-C7-ADMISSION-REVIEW-2026-10-09.md)；当前完整Mac21与专项证据通过。以下为历史记录： 迁移专项反例与独立复审通过；完整 Mac 21/21 和证据独立审核通过；最终输入复跑见[前置节点终审](../reviews/WAVE-2-ENTRY-CLOSURE-2026-09-28.md) |
| DEV-03-02 | `DONE`（2026-10-09 C6逐节点重签；原BLOCKED与历史证据保留） | [C6重签审查](../reviews/C6-C7-ADMISSION-REVIEW-2026-10-09.md)；当前完整Mac21与专项证据通过。以下为历史记录： 进程、超时和获准 Mock 脱组后代的真实 Mac 反例及独立复核通过；按 ADR-0006 界定平台与 Adapter 责任；最终输入复跑见[前置节点终审](../reviews/WAVE-2-ENTRY-CLOSURE-2026-09-28.md) |
| DEV-02-03、DEV-02-04 | `DONE`（2026-10-09 C6逐节点重签；原BLOCKED与历史证据保留） | [C6重签审查](../reviews/C6-C7-ADMISSION-REVIEW-2026-10-09.md)；当前完整Mac21与专项证据通过。以下为历史记录： 前向迁移/协议负向测试及独立 Terra 专项审核通过；见[第一批节点终审](../reviews/WAVE-2-BATCH-1-CLOSURE-2026-09-28.md) |
| DEV-03-03 | `DONE`（2026-10-09 C6逐节点重签；原BLOCKED与历史证据保留） | [C6重签审查](../reviews/C6-C7-ADMISSION-REVIEW-2026-10-09.md)；当前完整Mac21与专项证据通过。以下为历史记录： 前向迁移/协议负向测试及独立 Terra 专项审核通过；见[第一批节点终审](../reviews/WAVE-2-BATCH-1-CLOSURE-2026-09-28.md)；stdout非法前缀及现行验收见[深入审计](../reviews/DEEP-AUDIT-2026-10-08.md) |
| DEV-02-05、DEV-03-04 | `DONE`（2026-10-09 C6逐节点重签；原BLOCKED与历史证据保留） | [C6重签审查](../reviews/C6-C7-ADMISSION-REVIEW-2026-10-09.md)；当前完整Mac21与专项证据通过。以下为历史记录： Run/Lease/Review 前向约束与真实 Node Mock 正常流通过专项反例和独立 Terra 复核；见[第二批节点终审](../reviews/WAVE-2-BATCH-2-CLOSURE-2026-09-28.md) |
| DEV-02-06、DEV-03-05 | `DONE`（2026-10-09 C6逐节点重签；原BLOCKED与历史证据保留） | [C6重签审查](../reviews/C6-C7-ADMISSION-REVIEW-2026-10-09.md)；当前完整Mac21与专项证据通过。以下为历史记录： Artifact/Reference 关系与 Mock 真实故障注入通过专项反例和独立 Terra 复核；见[第三批节点终审](../reviews/WAVE-2-BATCH-3-CLOSURE-2026-09-28.md) |
| DEV-02-07 | `DONE`（2026-10-10基础节点单独终审；原BLOCKED与历史证据保留） | [当前07终审](../reviews/DEV-02-07-SCOPE-CLOSURE-REVIEW-2026-10-10.md)：唯一直接依赖02-01 DONE，真实业务/Event/Outbox原子提交、消费重放去重、Event不可变与当前Mac21通过；不宣称dispatcher/全类型审计/host产品链完成。以下为历史交付： `0014`/`0015`/`0016` 三次前向迁移关闭 Event 主体、Role-Binding 执行者快照与投递去重；24 条自建对抗攻击全部拦截；第二遍反例审查替代独立复核（已登记降级）；见[DEV-02-07 终审](../reviews/WAVE-2-DEV-02-07-CLOSURE-2026-09-29.md)。当前默认值整改及剩余验收见[本批报告](../reviews/OUTBOX-DEFAULT-REMEDIATION-2026-10-08.md) |
| DEV-02-08 | `DONE`（2026-10-10 HC-01基础存储终审；历史保留） | [当前终审](../reviews/DEV-02-08-HC01-CLOSURE-REVIEW-2026-10-10.md)：正式0033、当前Binding投递快照、完整历史/升级回滚、第二遍反例及当前Mac21通过；应用权限/dispatcher/产品链仍属后续节点。以下为重签前历史：[当前核对](../reviews/DEV-02-08-HISTORY-COMPAT-REVIEW-2026-10-10.md)：原57专项通过，新发现改派后Role换绑的新attempt被0030拒绝；HC-01仓外候选待独立回传/整合。 `0017`（18 trigger）+ `0018`（6 trigger）关闭 Thread/Message/Delivery 的 Team 守卫、状态机、不可变性与**创建轴**；两轮对抗审查 13 条攻击全部拦截；同模型独立审核替代独立复核（已登记降级）；见[DEV-02-08 重签](../reviews/WAVE-2-DEV-02-08-RESIGN-2026-09-29.md) |
| DEV-02-09 | `BLOCKED`（2026-10-10限定整改已通过；可信Human身份未闭环） | 新增0034/0035与13项专项；当前Mac21整体PASS，persistence303/0；[当前审查](../reviews/DEV-02-09-SCOPE-REVIEW-2026-10-10.md)。以下为历史交付： `0019`（21 trigger）补齐 `0008` 缺失的 Team 守卫、状态机、不可变性与删除/替换轴；四轴 30 条 + 第二遍 6 条共 36 条攻击，30 条拦截、6 条带明确裁决依据残留；四项规格缺口已回报 Human；同模型独立审核替代独立复核（已登记降级）；见[DEV-02-09 终审](../reviews/WAVE-2-DEV-02-09-CLOSURE-2026-09-29.md) |
| DEV-02-10 | `DONE`（2026-10-09 C6逐节点重签；原BLOCKED与历史证据保留） | [C6重签审查](../reviews/C6-C7-ADMISSION-REVIEW-2026-10-09.md)；当前完整Mac21与专项证据通过。以下为历史记录： `0020`（16 trigger）关闭 `0009` 缺失的 Approval 状态机、创建入口、决定痕迹双向冻结、请求不可变与 Team/版本/删除/替换轴，并补 Permission 策略的跨 Team、改动须 bump version 与删除/替换守卫；两轮共 43 条攻击（实现前 26 + 第二遍 17）全部拦截、0 误伤；第二遍发现决定人可被改写的真实绕过（1 处修复）与我自己守卫的自相矛盾（1 处移除）；同模型独立审核替代独立复核（已登记降级）；见[DEV-02-10 终审](../reviews/WAVE-2-DEV-02-10-CLOSURE-2026-09-29.md)与[缺口补齐补充终审](../reviews/WAVE-2-DEV-02-10-GAP-REMEDIATION-2026-09-29.md)（`0021`，**9 trigger**，补齐 Approval↔Permission 关联与 CANCELLED 取消 actor，三组合计 61 条攻击全拦） |
| DEV-02-11 | `DONE`（2026-10-10单节点重签；原BLOCKED与历史证据保留） | [当前重签](../reviews/DEV-02-07-BRANCH-TEST-REVIEW-2026-10-10.md)：5个直接依赖DONE、三机56项与第二遍三项反例、当前stable Mac21及有效line/branch通过。以下为历史交付： §2/§3/§4 三机纯状态机已交付：`domain` 层表驱动实现（Task 14 边 + 2 创建目标 / Run 17 边 + 1 / Binding 7 边 + 1），共 56 个表驱动用例 + Q-01～Q-05 机检装置；对抗审查抓到并修复 5 处真缺陷（Q-04 状态机输出可被改写、Q-05 `ADAPTER_UNAVAILABLE` 漏出 retryable、Q-06 reconcile 命令歧义致 `transition` 静默选错边、Q-07 事件 serde 线名分叉、Q-08 **30 个错误码 serde 线名全错且波及第一轮已过门禁的 Task**）；另登记 `STATE_MACHINES §2` ASCII 图缺 `CANCELLED` 节点（未擅自改冻结规格）；见[DEV-02-11 终审](../reviews/WAVE-2-DEV-02-11-STATE-MACHINES-CLOSURE-2026-09-29.md) |
| DEV-02-12 | `BLOCKED`（2026-10-08依赖复核；以下历史交付） | §5 Message（10 状态 / 15 边 + 1 创建）、§5 DeliveryAttempt（5 状态 / 4 边）、§7 Approval（5 状态 / 4 边 + 1 创建）纯状态机已交付，共 44 个表驱动用例；节点标准四条全部有断言：`RETRYING` 非状态且重试回到 `QUEUED`、`FAILED` 出度恰为 1、`EXPIRED` 出度为 0、`NEEDS_HUMAN` 出度为 2 且三终态各发自己的事件；关键发现是冻结规格**用标点区分事件基数**（`、`=都发，`/`=互斥，全规格仅两行用 `/`），据此把 `NEEDS_HUMAN → QUEUED` 拆为 `Reassign`/`RequeueNeedsHuman` 两条边；新建 **Q-06/Q-07 两台装置**直接从冻结规格机械解析并逐行比对边表（Q-06 含源扇出/目标扇出/互斥事件三条展开规则并先在已收口的 §2/§3/§4 上自证；Q-07 比对 `(from,to,events,rejection)` 四元组）；Q-06/Q-07 上线即抓到 **Q-09**：`NEEDS_HUMAN → QUEUED` 两条边的拒绝码被抄成 `AUTH_PERMISSION_DENIED`，规格实为 `AUTH_HUMAN_REQUIRED`（补救路径不同：前者指向「申请权限」，后者指向「打开审批/接管入口」），且该码原本不在枚举里——**19 个测试全部漏掉，因为期望值是照同一份错误读法手抄的**；已修并做反向验证（故意改回错码，确认装置报错）；Q-04 97/97、Q-05 129/129、Q-06 10/10、Q-07 57/57 六机全绿——Q-05 对三台新机一次通过，因 DEV-02-11 的 Q-08 修复已成结构性保证；见[DEV-02-12 终审](../reviews/WAVE-2-DEV-02-12-STATE-MACHINES-CLOSURE-2026-09-30.md) |
| DEV-02-13 | `BLOCKED`（2026-10-07复核；以下为历史交付） | §3 权限矩阵（36 动作 × 6 主体 = 216 格）与 §2 八层求交已交付：`PermissionRequest.terms` 是定长 `[LayerVerdict; 8]`，**「漏检某一项检查」在类型上不可表达**，§2 的默认拒绝成为扫描的性质；扫描顺序即 §2 印刷顺序且首个拒绝即终止，`ValidApprovalGrant` 排最后使「Approval 不能突破 Team 安全上限」无需特例规则。实现抓到 1 处真缺陷：Human-only 行里的 `—` 曾误报 `AUTH_PERMISSION_DENIED`，而 §7 两码补救路径不同（申请授权 vs 打开审批入口），§5 冻结的动作不应指向不存在的授权——已按 Owner 格为 `H` 重新定义并做反向验证（短接成恒 false 确认 2 个用例失败）。**本轮 5 个失败用例中 4 个是测试写错**（`Team 创建/编辑` Owner 是 `A` 不是 `H`、`Task 分配/改派` Product 是 `S` 不是 `—`、用全行 `—` 的动作去测 approval 补充永远测不到），全部是手抄规格抄错。另**主动把一处判断从 mechanical 降级为 derived reading**：机械比对发现 §3 的 `H` 与 §5 的 8 类覆盖面互不包含（3 行有 `H` 而 §5 未列、2 类 §5 列了而 §3 无行），故该映射规格未成文，代码与测试均如实标注。独立审查判定 **FAIL**，6 条全部成立并已修：Q-09 存在 `zip` 静默截断、结尾字符串硬编码（打印正确句子却没做那件事）、`#COLUMN` 解析后从不校验、无 36 行绝对锚点、DUMMY 死代码致 docstring 撒谎、Q-08 缺格时 IndexError 崩溃且其 docstring 把 36×6=216 写成 35×6=210——**每条修复都构造已知坏输入做了反向验证**。Q-09 终态 PASS=263（原 254，新增 9 项结构检查）；见[DEV-02-13 终审](../reviews/WAVE-2-DEV-02-13-PERMISSION-INTERSECTION-CLOSURE-2026-09-30.md) |
| DEV-02-14 | `BLOCKED`（2026-10-07复核；以下为历史交付） | §3 乐观锁、§4 幂等命令记录、§11 Outbox 重试决策已交付：新增迁移 `0022`（幂等记录表 + 6 个守卫）、`domain::idempotency`、`domain::outbox_dispatch`，新增 50 个用例（全工作区 354→404），clippy 零告警。**§4 的五行分两处落地**：前三行是记录态（`IN_PROGRESS`/`SUCCEEDED`+`result_ref`/`FAILED`+`first_error_code`），第四行「不同 payload」是 **UNIQUE 冲突而非状态**（要写第二行必须先删第一行，而 DELETE 与 REPLACE 双双被拒，故重复执行在数据库层不可能），第五行「key 超保留期」**不进状态机**——§4 原文说防线是「唯一约束/业务引用」，建模成 `EXPIRED` 等于暗示「过期后可放行」，与该行正相反。§3 的冲突结果**不含 `next` 字段**，「不自动覆盖」由类型保证。独立审查判定 **FAIL**，7 条全部成立并已修且各带反向验证：**D1 阻断**——`INSERT OR REPLACE` 完全绕过 `no_delete_guard`（SQLite 隐式 DELETE 不触发 DELETE 触发器），一次 REPLACE 即劫持 `result_ref`、改写 `payload_hash`；**本项目 `0013`/`0014`/`0016` 已有四处 `*_replace_guard` 先例，`0014` 注释甚至写明该机制，我读到了却没套用到新表**——`0022` 曾是唯一漏掉的表；**D2** §11「只有 retryable=是 时允许」因 `Retryable` 标签无约束而形同虚设；**D3** 我写的「`CONCURRENCY_RESOURCE_BUSY` 是 §4 唯一可重试码」是**事实性误述**（实有三个），代码选择对、论据错；**D4** `ceiling_millis` 只夹单次延迟而 §11 约束的是**总时长**（新增 `RetryBudget` 逐次扣减）；**D5** 负预算 panic；**D6** `entry.max_retries` 是死字段；**D7** 4 处悬空引用，其中一处**听起来像一道并不存在的规格漂移防线**。修复中我又抓到三个自己的错：守卫把合法的 `IN_PROGRESS→SUCCEEDED` 推进也拦了；**预算测试注入「忽略预算」时仍 23/23 全过——它自己算 `spent`、根本没经过被测函数**；两处手抄（UUID 33 位、pragma 返回大写类型名）。另登记 `0006` 的 `max_retries DEFAULT 5` 与 §11 的 3 冲突（既有测试全部显式传 5，故从未暴露；`0006` 已发布不可改，用 `assert_ne!` 把冲突钉死）；见[DEV-02-14 终审](../reviews/WAVE-2-DEV-02-14-CONCURRENCY-IDEMPOTENCY-OUTBOX-CLOSURE-2026-09-30.md) |
| DEV-03-06 | `BLOCKED`（2026-10-07复核；以下为历史交付） | §3 取消/超时**纯判定**与 §11 重试准入已交付：新增 `domain::run_termination`、`domain::run_retry` 与 `examples::dump_retry_catalog`，新增 34 个用例（全工作区 404→438），clippy 零告警，退避直接复用 DEV-02-14 的 `RetryPolicy`/`RetryBudget` 未重写推导。**§3 三行取消的措辞差异被逐格建模**：`QUEUED` 只认「尚未开始」、`STARTING` 才认「达到强制 fence 条件」、`RUNNING` 只认「Human takeover 先 fence」，三者拆成五个 `CancelObservation` 变体；三行拒绝码也不同（`AUTH_PERMISSION_DENIED`/`RUN_CANCEL_NOT_CONFIRMED`/`AUTH_PERMISSION_DENIED`），全部与 `RUN_EDGES` 的派生拒绝码对账。**`RUNNING → TIMED_OUT` 的第二条件建模为三态 `ReconcileEvidence`**：`Indeterminate`→`NeedsReconcile`（「事实不明」）、`ProvedRunning`→`KeepRunning`（**derived reading**：§3 两行都不适用，规格无此行，代码与测试均标注）、`CouldNotProve`→`TimeOutFromReconcile`（「对账仍无法证明运行」）。§11 第四条承接为既非拒绝也非放行的第三种判定 `QueryFirstResult`；**优先级是本节点的选择，§11 未规定顺序**，代码与测试各钉一次。**「每次新 Run」由类型保证**：`RunCreation` 四字段私有、唯一实例 `CREATE_QUEUED`，「建到 RUNNING」直接编译不过。**两处关于规格的不实陈述在自查中被抓出并修正**：① 曾断言「§3 只在 STARTING/RUNNING 下列出 TIMED_OUT」（漏 `UNKNOWN` 那行）；② 曾断言「§3 对 `UNKNOWN` 无 `→ CANCELLED` 行」（实有，但属 reconcile 行，故新增 `ReconcileOnly` 与 `NotCancellable` 区分）。独立审查判定 **FAIL**，6 条全部成立已修：**D1** `satisfies` 表格两格声称了 §3 没有的保真度（`QUEUED × 强制 fence`、`RUNNING × 强制 fence`），且与同文件变体 doc 自相矛盾；**D2** 字段名 `reconciliation_proves_running`（完成态）与所需语义（可能性）相反，且把 §3 两行压成一个 bool；**D3** `assert_eq!(ALL.len(), 9)` 是恒真断言，加第 10 个变体后两套件全绿——**修法自身先失败一次**（const 块用 `ALL.len()` 推导变体全集，等于自证），改为对 `RUN_EDGES`+`CREATE_QUEUED` 的实际拒绝码求覆盖；**D4** §11 规则 3 的「概念→码」映射有两条是发明的（`BINDING_STALE_VERSION` 不是版本冲突码）；**D5** 上述①；**D6** `NotYet` 的 doc 漏了实际覆盖的情况。新建两台机检装置：**Q-12** 比对目录 `Retryable` 列 vs `is_retryable()` vs 判定函数的准入结果（Rust 侧由运行 example 取得，不扫 `.rs`）；**Q-13** 逐字核对五个文件里 31 段规格引用——**Q-13 上线即抓到 §11 第四条被截断的引用**（少了「可能产生外部副作用的命令」）。变异测试 12/12 全红、1 个被编译器拦截、1 个被新测试拦下；两台装置的 S2 自我证明**各失败过一次**（Q-12 注入的码落在比对集合之外导致假绿，Q-13 漏剥 `///` 前缀）；见[DEV-03-06 终审](../reviews/WAVE-2-DEV-03-06-TERMINATION-RETRY-CLOSURE-2026-09-30.md)**第二轮复核仍判 FAIL**：D1/D2/D4/D5/D6 确认修好，**D3 实质收口但交付机制不合格**——`ALL_HAS_NO_DUPLICATES` 是惰性求值的关联常量且全仓无人引用，塞重复项后 `cargo build` 仍 exit 0，而 doc 写的是「fails the build」，**属本项目第三次「自证」事故**；改法是折进 `ALL` 的定义并**如实写明只在使用点触发**（测试/example 编译期 `error[E0080]`），另修 F2（`ExecutionObservation::ALL` doc 停在 bool 时代）、F3（rustdoc 指向已删除的 `TimeoutObservation::reconciliation_proves_running`）、F4（`14` 是第三处手动同步点，已在 doc 标注）。第二轮 17 个变异全红。**冻结在第二轮再次被破**（14:10:13 五个文件，改动者即本会话的 `cargo fmt --all`）。 |
| DEV-03-07 | `BLOCKED`（2026-10-07复核；以下为历史交付） | §5 租约续期与 fencing 判定已交付：新增 `domain::run_fencing`（493 行）与 16 个用例、`persistence-sqlite` 13 个真库用例，全工作区 438→471，clippy 零告警。**本节点的主依据是 `TECH_STACK_AND_STANDARDS.md` §5**——前五个节点都没用过它：「心跳只能延长匹配当前 token 的租约；接管/超时生成新 token，旧执行者所有写入返回 `RUN_FENCED`」。**找到「双主 Run」漏洞的确切位置在执行层**：`adapter-supervisor/protocol.rs:1223` 只把上报的 token 与**它启动时被配置进去的** `expected_context` 比，之后再没从数据库重读过当前世代，所以「旧进程恢复后继续上报」能通过它的全部检查；**`0012` 挡不住这个**——它检查「一个 Task 不得同时有两个活动 Run **记录**」，而旧 Run 早已被 fence 成终态，记录层确实只有一个活动 Run。**`0012` 停住重复的记录，停不住那条记录曾所属的进程。****世代比较而非 `!=`**：单调性使 `<`（上一任执行者）、`==`（当前）、`>`（§2 下不可能＝数据故障）三态可区分，`!=` 会把前两者的异常情形压成一个布尔。**`RUN_FENCED` 与 `RUN_LEASE_EXPIRED` 分开**：前者是身份、后者是时间窗，持有当前 token 但租约过期的执行者**没被 fence**，且其心跳**不能复活租约**——否则租约永不过期、§3 的 `RUNNING → TIMED_OUT` 对活进程不可达。**裁决：`RUN_LEASE_EXPIRED` 不加入 `RunErrorCode`**，因为会打破 DEV-03-06 立的「`ALL` 里每个码都能被 `RUN_EDGES`/`CREATE_QUEUED` 产生」不变式（§3 无行产生它），故按 `INTERNAL_UNEXPECTED` 先例作为独立形状携带冻结线名，**DEV-02-11/03-06 冻结结构零改动**。三处刻意不建模已登记：`decide_write` 不收 `LeaseSnapshot`（§5 把写入准入限定在身份上，收下不分支的参数会让签名暗示规格没有的耦合）；「输出」作为写入种类是 **derived reading**（§3 无此迁移）；**heartbeat deadline 不新建列**——§3 说要「设置/恢复 heartbeat deadline」而 `0001`～`0022` 无任何 heartbeat 列（`grep -rniE heartbeat migrations/` 无输出），本节点按 `leases.expires_at` 的派生量处理，**待裁决**。另登记一项：`LEASE_BEARING_STATUSES` **生产零调用点**——`decide_write` 看不到 Run 状态，故已终态 Run 出示当前 token 时 `Completion` 仍会放行；本节点不擅自补这个耦合，**建议归 DEV-03-08**（那里本就按状态决定对账动作）。变异测试 **9 个行为变异全杀**（表 10 行，M11 作用于零调用点的纯 API），且**台账连补丁一起交付**（`run-all.sh` + `patches/m*.py`）——上一版只留了「一句话描述 + 杀几个用例」，独立审查复现不出来（记的「续期不校验世代→2」实际是 4），**变异台账不随补丁走就不是台账**。审查还查出 `Generation::is_current` 的 `Ahead` 分支没测住（放宽成 `Current | Ahead` 时 16/16 全绿），已补断言，现被 M11 杀死。上一版的第 10 个变异体是等价变异体（`decide_renewal` 本就拒非当前世代，那次调用是**代码多做的一件事**），已删冗余让世代判定只存在于 `decide_renewal` 一处；该行代码已不存在，故不再列为可复现变异体。**Q-13 扩展而非克隆**：覆盖增到 7 文件、冻结规格增到 4 份，新增 **C2 in-repo citation** 检查（`path.rs:1223 → 文本` 打开那文件那行核对）、**C3「引用形状的行却落在检查范围外」**与 **C4「引用数下限」**——C2 原有静默绕过（把引用移出 ```text 围栏后装置报 `0 in-repo citations` 仍 PASS），**首轮修复只堵了一半**（C3 的归一化只剥三种前缀，且删掉引用仍能 PASS），第二次审查查实后放宽归一化并补上下限，**六种 evasion 全部 exit=1**——**C2 上线即抓到模块头行号写成 1227、真实是 1223**，而我第一眼误判那 5 条报错为假阳性（见 J99）；**第七次审查判定 FAIL 后换了检查形态**：原 domain 用例靠**读 `0012` 文本**核对常量，两轮被走出六种一行绕过（最省事的是文末追加两行 `DROP TRIGGER`——四个字面量一个不少，套件 16/16 绿，而数据库已不再拒绝终态 Run 的租约）。**文本检查永远可以被文本满足，所以那类检查整体撤掉**：domain 只断言「等于全部非终态」，**11 个守卫装没装、8 个状态正反行为、UPDATE 路径放不放**，全部交给 `persistence-sqlite` 的真内存库去问 SQLite。反向验证装置 `bypass-matrix.py` 同步换靶并扩到 **19 行**（13 红 6 绿），新增 `--preflight`——**它当场抓出两行测错了东西**（只注释掉 `CREATE TRIGGER` 那一行，文件不再是 SQL，红是语法错误的红，不是守卫被删的红）。**第八次审查再判 FAIL，一条阻断面**：`leases_consistency_insert` 的**身份子句**`task_id` / `fencing_token` **零覆盖**——删掉这两行，domain 16/16、persistence 10/10、整个 persistence 包 137/137 全绿，而 `RUNNING` 的 Run 已能挂上陈旧或超前的 token。**而 `0012:50-55` 的升级回填正把这两条当成历史数据违规**——同一份迁移对过去断言、对前向不强制。已补 `a_lease_must_name_its_own_task_and_generation`（四个案例，每个否定案例在同库先插正确租约作对照），并给 11 名存在检查**加上 `tbl_name` 约束**。矩阵同步扩到 **24 行**（17 红 7 绿），新增 4 行身份子句变异 + 1 行**真语义等价**改写（`IN` 四非终态 → `NOT IN` 四终态，实测绿）。**这一轮最该记的一句是：上一轮那四条命题按字面都是真的，但命题比不变式窄。****第十次审查再判 FAIL，八条阻断面**：它把 `0012` 展开成 76 个子句做 **193 条变异**（删除 + 弱化），`0012:283`/`0012:294` 两条守卫整条换成 `RAISE(IGNORE)`（**SQLite 会连带放弃同一行后续的触发器程序，所以 `0010` 也没轮到执行**）、`0012:312` 与 `0012:298` 的**列清单**、`0012:78/90/91` 三条元组子句——**八条在 15 个用例与整个 142 个测试的包下全绿**。`0012:79` 那条查实是**冗余**（`NEW.team_id` 同时出现在四条 EXISTS 里，可由其余子句推出），审查者描述的漏洞其实是 `0012:78` 那条——**这是本节点第三次「先判因再改期望」，三次都是我的期望错**。**另修 M2**：`LEASE_BEARING_STATUSES` 与 `0012` 之间**一直没有任何断言**（persistence 侧硬编码 8 个字面量，常量在它代码里一次没出现），把 `RunStatus::Queued` 换成第二个 `Unknown`，**domain 套件 16/16 全绿**；现加 `teamspace-domain` 的 dev 依赖（工作区内、无环），期望集**从 `RunStatus::ALL` 导出**——**验证：domain 仍全绿，persistence 转红**。矩阵扩到 **41 行**并**改打整个 `persistence-sqlite` 包**（此前只打一个测试二进制，把「被另一个二进制逮住」的两条误判成缺口）；另把 7 处裸 `is_err()` 改走带来源白名单的 `assert_refused`。**第十一次审查（中止，未跑完）查出一条阻断面**：`reviews_consistency_update`（`0012:213`，`RAISE` 在 `:231`）**既无测试也无矩阵行**——整条换 `RAISE(IGNORE)` 时**整个 145 个测试的包全绿**。**原因值得单列**：该 trigger 此前**只有一行 L09**（整条删除），而整条删除恰是全套件唯一看得见的形状（`sqlite_master` 里名字没了）；`RAISE(IGNORE)` 之后它**仍以原名装着**，存在性检查对它无感。已补 `a_review_cannot_be_moved_off_the_tuple_it_was_filed_against`（6 案，含「同一行 `version` 自增必须成功」的正向对照）与矩阵 **L41**。**随后把同形状一次补齐**：11 个 trigger 里当时只有 6 个有 `RAISE(IGNORE)` 行，另 5 个只有子句行（子句行问「这条子句是否承重」，没有一行问过「这里到底有没有在拒绝」），已补 **L42～L46**，矩阵 **41 → 47 行**；并给装置加 `shape_coverage()` 结构断言，从迁移读 trigger 名、从装置读 neuter 目标，对不上即 `exit=1`。**同一段的谓词在 `0012` 里写了两遍（insert 与 update），测试只覆盖了一遍——「兄弟守卫有测试」不等于「这条也有测试」。**该审查另**纠正了我终审 §13.6 的定性**：`RAISE(IGNORE)` 是 **fail-silent（静默丢写：`ok=True`、`rowcount=0`、行未变）**，**不是「不变量被破坏」**——库是干净的，脏的是调用方的认知，风险是**静默丢更新**；B1/B2 的测试覆盖依然有效，**要改的只是定性**。**这一轮我自己的新测试也空转过**：`a_run_must_name…` 初版三列下标写错，每个案例都被无关外键拒掉，**对它本该抓的变异全绿通过**——**是矩阵发现的，不是审查**。**第九次审查再判 FAIL，四条阻断面全部落在我装置覆盖不到的地方**：它把 `0012` 十一个 trigger 展开成 **76 个子句**逐个做删除与弱化，**100 条定向变异**，结果 `0012:186` 的 `=` 改 **`>=`**（守卫仍拒超前世代、**陈旧世代畅通无阻**）、`0012:135` 的 `attempt` 不可变、`0012:142-145` 的终态复活禁令、`0012:201` 的 review Task 绑定**四条全在 11/11 全绿下被绕过**。已各补一个测试，四条我均自复现。**根因是我的矩阵 24 行全是删除类变异**——删除最容易被发现，**弱化（`=`→`>=`）守卫还在、还在拒绝东西，只验删除的套件全绿通过**；且此前**一行都没碰过其余九个 trigger**。已加 `edit_clause` 与 2 行弱化、6 行其余 trigger，矩阵扩到 **31 行**（24 红 6 绿 + 基线；另删掉一行与 L00 重复的「restored baseline」——「7 绿」实为 6 个不同变异，靠逐行 sha256 比对发现，不靠人眼数）。装置另修：`preflight` 原读仓库、矩阵再 tar 一份去跑，**两次独立读取**，现改为**读被测的那份副本**；`sync-closure-lengths.py` 的 `EXPECTED_ROWS` 是**死代码**（`main()` 从不读，上面却有 6 行注释解释它提供的保证），已删。另更正台账两处：顶部「生效证据」表**整表仍指向已作废的 r9**（已改指 r10，并把「门禁期间工作区未变」从人推改成两个机器字段相等）；`sync-closure-lengths.py` 的行正则写死 `crates/domain/` 使其**只看到 §11 三行里的两行**，而 `expected_rows=2` 把缺口固化成了通过条件——放宽后**立刻抓到真过期数字**。更早的「配平右括号」一处：代码是 `find(')')` 取第一个——是描述与代码不符。详见终审 §13.2/§13.3 与仓库外台账。见[DEV-03-07 终审](../reviews/WAVE-2-DEV-03-07-FENCING-CLOSURE-2026-09-30.md) §13.6 |
| 其余 DEV-02-15 及后续节点 | `PLANNED` | 仅已满足直接依赖的节点可进入单节点审查；尚无正式 `DONE`。动态执行安排见 `docs/coordination/README.md` |

## 3. DEV-00：决策、语义与契约冻结

阶段出口：产品、设计、数据和协议不再各自解释同一状态；所有后续实现有可引用的规则表。

| 节点 | 工作内容 | 依赖 | 模型 | 节点审核标准 |
|---|---|---|---|---|
| DEV-00-01 | 建立 PRD FR/AC—审计项—Figma 节点追踪矩阵 | 无 | `gpt-5.6-terra` | FR-001～054、AC-01～12 和全部 P0 审计项均有唯一负责人节点；无孤儿需求。 |
| DEV-00-02 | 冻结领域词汇与聚合边界 | 无 | `gpt-5.6-sol` | Team/Role/Binding/Harness/Task/Run/Artifact/Event/Message/Notification/Approval 定义无循环或同义冲突。 |
| DEV-00-03 | 冻结 Task/Run/Binding 状态迁移表 | 00-02 | `gpt-5.6-sol` | 每条迁移含 actor、前置条件、副作用、Event、错误码；PRD 全部合法/非法迁移可映射。 |
| DEV-00-04 | 冻结 Message/Delivery/Notification 状态与边界 | 00-02 | `gpt-5.6-sol` | Human Notification 与 Role Message 分离；重试、TTL、改派、换绑快照和终态无歧义。 |
| DEV-00-05 | 冻结权限矩阵与 Human-only 动作 | 00-02 | `gpt-5.6-sol` | 每个命令列出主体、资源、路径、上下文和拒绝码；Harness 无自提权路径。 |
| DEV-00-06 | 冻结 ID、时间、版本、归档与留存规则 | 00-02 | `gpt-5.6-terra` | UUIDv7、显示 ID、UTC、expected_version、归档与不可变记录规则能落到数据字典。 |
| DEV-00-07 | 冻结错误码目录和用户提示映射 | 00-03～05 | `gpt-5.6-terra` | 认证、权限、冲突、非法状态、依赖、Harness、Workspace、系统错误可稳定区分且不泄密。 |
| DEV-00-08 | 冻结 Adapter V0 方法、事件和兼容策略 | 00-02～07 | `gpt-5.6-sol` | JSON Schema 可验证；初始化、健康、启动、心跳、取消、对账、结束与失败语义完整。 |
| DEV-00-09 | 冻结 Figma Token、正式导航和响应式断点 | 00-01 | `gpt-5.6-terra` | 覆盖 36 画板核心入口；Harness、活动、依赖、执行记录、设置均有正式导航归属。 |
| DEV-00-10 | 评审并接受实施 ADR | 00-01～09 | `gpt-5.6-sol` | ADR 明确选择、代价、否决方案和复审触发条件；Human 确认后才进入代码实现。 |

## 4. DEV-01：工程底座与可重复开发环境

阶段出口：新机器能按单一命令完成安装、检查、测试和桌面壳启动；生成契约与手写代码边界清晰。

| 节点 | 工作内容 | 依赖 | 模型 | 节点审核标准 |
|---|---|---|---|---|
| DEV-01-01 | 创建 pnpm workspace、Cargo workspace 与目录骨架 | 00-10 | `gpt-5.6-luna` | 结构符合技术基线；无业务实现；空工作区 build/test 成功。 |
| DEV-01-02 | 固定 Node、pnpm、Rust 工具链和 lockfile | 01-01 | `gpt-5.6-luna` | 干净环境版本一致；未锁依赖和跨 major 漂移被 CI 拒绝。 |
| DEV-01-03 | 配置 fmt/lint/typecheck/clippy 基线 | 01-01 | `gpt-5.6-luna` | TS strict 与 Rust `-D warnings` 生效；故意违规样例能让检查失败。 |
| DEV-01-04 | 建立测试分层与统一命令 | 01-01 | `gpt-5.6-terra` | unit/integration/contract/e2e 可独立运行；退出码与报告稳定。 |
| DEV-01-05 | 建立 macOS 首发质量门禁；保留历史 Ubuntu 验证资产 | 01-02～04 | `gpt-5.6-terra` | 按 ADR-0005 以 macOS 当前输入验证；缓存不影响正确性；格式、lint、类型、测试、依赖扫描均为 required gate。 |
| DEV-01-06 | 创建 Tauri/React 最小桌面壳 | 01-01～03 | `gpt-5.6-terra` | macOS 可启动/关闭；Tauri capability 最小化；无通用 shell/文件权限。 |
| DEV-01-07 | 建立 Rust Schema → TypeScript 类型生成 | 01-01～04 | `gpt-5.6-sol` | 单一来源可重现；生成物漂移使 CI 失败；前端无重复手写 DTO。 |
| DEV-01-08 | 建立结构化日志、correlation ID 与脱敏框架 | 01-06 | `gpt-5.6-sol` | request/task/run 可关联；测试秘密在日志、错误、Event 输出中均被遮蔽。 |
| DEV-01-09 | 建立开发 fixture、时钟/ID/随机源注入 | 01-04 | `gpt-5.6-terra` | 测试可重复；不依赖真实时间、用户目录或在线服务。 |

## 5. DEV-02：领域内核、数据模型与一致性

阶段出口：不依赖 UI/Adapter 即可通过领域和数据库测试证明身份、状态、权限、并发与审计规则。

| 节点 | 工作内容 | 依赖 | 模型 | 节点审核标准 |
|---|---|---|---|---|
| DEV-02-01 | 实现强类型 ID、时间、版本与 Actor 值对象 | 01-01、00-06 | `gpt-5.6-terra` | 禁止混用 Role/Task/Run ID；序列化、排序和 UTC 往返无损。 |
| DEV-02-02 | 设计并迁移 Team/Role/Workspace 表 | 02-01 | `gpt-5.6-terra` | 稳定 ID、版本、停用/归档、唯一约束和外键有 migration 测试。 |
| DEV-02-03 | 设计并迁移 Harness/Adapter/Binding 表 | 02-01 | `gpt-5.6-sol` | 一个 Role 最多一个有效主 Binding；历史有效区间不可重叠。 |
| DEV-02-04 | 设计并迁移 Task/Dependency 表 | 02-01 | `gpt-5.6-sol` | Task 状态、owner Role、引用、版本、依赖约束完整；显示 ID Team 内唯一。 |
| DEV-02-05 | 设计并迁移 Run/Lease/Review 表 | 02-01、02-04 | `gpt-5.6-sol` | active Run、租约、fencing、尝试序号和 Review 约束能在事务中验证。 |
| DEV-02-06 | 设计并迁移 Artifact/Reference 表 | 02-01、02-04～05 | `gpt-5.6-terra` | creator Role、Run、版本、URI、commit/checksum 与可见范围可追溯。 |
| DEV-02-07 | 设计并迁移 Event/Outbox 表 | 02-01 | `gpt-5.6-sol` | 业务变更与 Event/Outbox 原子提交；消费可重放和去重；Event 无更新接口。 |
| DEV-02-08 | 设计并迁移 Thread/Message/Delivery 表 | 02-01、00-04 | `gpt-5.6-sol` | `to_role_id` 强制存在；Binding 快照、reply_to、TTL、幂等和 attempt 历史完整。 |
| DEV-02-09 | 设计并迁移 Notification/Policy/Attention 表 | 02-01、02-07～08 | `gpt-5.6-sol` | Human 接收者不伪装成 Role；Event、Message、通知和待处理项可互相引用。 |
| DEV-02-10 | 设计并迁移 Permission/Approval 表 | 02-01 | `gpt-5.6-sol` | 资源、动作、路径、有效期、审批人和范围可表达最小权限。 |
| DEV-02-11 | 实现 Task/Run/Binding 纯状态机 | 00-03、02-02～05 | `gpt-5.6-sol` | 全部迁移有表驱动与性质测试；非法迁移零状态副作用。 |
| DEV-02-12 | 实现 Message/Delivery/Approval 状态机 | 00-04～05、02-08～10 | `gpt-5.6-sol` | RETRYING 瞬态正确回流；FAILED/EXPIRED/NEEDS_HUMAN 不丢历史。 |
| DEV-02-13 | 实现集中式权限求交与拒绝原因 | 00-05、02-02～10 | `gpt-5.6-sol` | Team∩Role∩Task∩Path∩Capability∩Approval 生效；拒绝不改变资源并写 Event。 |
| DEV-02-14 | 实现乐观锁、幂等命令记录与 Outbox dispatcher | 02-07、02-11～13 | `gpt-5.6-sol` | 重复命令返回同一结果；payload 冲突拒绝；版本竞争仅一个成功；事件至少一次且可去重。 |
| DEV-02-15 | 实现备份、迁移升级与恢复验证 | 02-02～10、02-14 | `gpt-5.6-sol` | 空库升级、旧版迁移、运行中一致性备份与损坏恢复演练有证据。 |

## 6. DEV-03：Adapter 契约与运行时 Spike

阶段出口：Mock Adapter 可在受控进程中完成领取—心跳—结果/失败—取消—对账；真实 Codex 接入风险有数据支撑。

| 节点 | 工作内容 | 依赖 | 模型 | 节点审核标准 |
|---|---|---|---|---|
| DEV-03-01 | 发布 Adapter V0 JSON Schema 与示例 | 00-08、01-07 | `gpt-5.6-sol` | 正反例通过 Schema 测试；版本兼容、未知字段和错误 envelope 已定义。 |
| DEV-03-02 | 实现 Adapter 子进程 supervisor | 03-01、01-08 | `gpt-5.6-sol` | 启动、超时、退出、stderr、关闭可控且无 shell 拼接；平台有界回收直接进程并清理同组后代，受批准 Adapter 按 ADR-0006 以真实 Mac 反例证明其自建脱组后代在 EOF、关闭和崩溃后有界退出；不得宣称任意恶意后代可被平台强制清理。 |
| DEV-03-03 | 实现 initialize/能力/健康检查 | 03-02 | `gpt-5.6-terra` | 不兼容版本、缺能力、未认证和离线可稳定诊断并留痕。 |
| DEV-03-04 | 实现 Mock Adapter 正常执行流 | 03-01～03 | `gpt-5.6-terra` | start→heartbeat→output→completed 顺序可重复验证，产物用引用返回。 |
| DEV-03-05 | 实现 Mock 故障注入 | 03-04 | `gpt-5.6-sol` | 支持超时、重复、乱序、崩溃、晚到回调、拒绝取消和失联场景。 |
| DEV-03-06 | 实现取消、超时、重试与退避 | 03-02～05、02-11、02-14 | `gpt-5.6-sol` | 取消有确认/超时边界；自动重试只覆盖瞬时错误；每次新 Run。 |
| DEV-03-07 | 实现心跳、租约续期与 fencing 校验 | 03-04～05、02-05、02-11 | `gpt-5.6-sol` | 旧 token 的心跳、输出和完成都被拒；并发与恢复测试无双主 Run。 |
| DEV-03-08 | 实现启动重连与 Run reconcile | 03-02～07、02-15 | `gpt-5.6-sol` | 重启先 UNKNOWN；可恢复/失败/需 Human 三类结果稳定且不重复执行。 |
| DEV-03-09 | 接入 OS Keychain 的 CredentialReference | 03-02、02-10 | `gpt-5.6-sol` | 数据库和 UI 只见引用；秘密只注入目标进程且日志/诊断无明文。 |
| DEV-03-10 | Codex Adapter 技术 Spike | 03-01～09 | `gpt-5.6-sol` | 真实验证启动、输入、状态、取消、结果、配置隔离和恢复；列出不可验证项。 |
| DEV-03-11 | Adapter Spike Go/No-Go 与第二 Harness 决策 | 03-10 | `gpt-5.6-sol` | 以测量证据决定继续/调整；任何契约变更先回写 ADR/Schema/契约测试。 |

## 7. DEV-04：Workspace、Git 隔离与 Artifact

阶段出口：任务引用可靠定位，Run 写入受控且互不覆盖，结果能形成完整 Artifact 事实链。

| 节点 | 工作内容 | 依赖 | 模型 | 节点审核标准 |
|---|---|---|---|---|
| DEV-04-01 | Workspace 绑定、路径规范化与可用性检查 | 02-02、02-13 | `gpt-5.6-sol` | 相对路径、symlink、无权限、缺失、跨 Team 越界均被安全处理。 |
| DEV-04-02 | Markdown 文件/标题锚点引用解析 | 04-01、02-06 | `gpt-5.6-terra` | 中文标题、重复标题、文件移动和失效引用有确定行为与错误。 |
| DEV-04-03 | Git 状态、基线 Commit 与只读元数据 | 04-01 | `gpt-5.6-terra` | dirty/branch/commit 可查询；查询不修改用户仓库。 |
| DEV-04-04 | 每 Run branch/worktree 生命周期 | 04-01、03-07 | `gpt-5.6-sol` | 并行 Run 目录独立；创建失败可回滚；清理不误删用户分支/改动。 |
| DEV-04-05 | 非 Git Workspace 单写者锁 | 04-01、03-07 | `gpt-5.6-sol` | 第二写者被拒并生成冲突 Event；崩溃后的锁恢复不会双写。 |
| DEV-04-06 | 文件写入路径授权与 fencing | 04-01、02-13、03-07 | `gpt-5.6-sol` | 未授权路径、旧 token、越界 symlink 写入均拒绝且不泄密。 |
| DEV-04-07 | Artifact 登记、校验和预览元数据 | 04-02～06、02-06 | `gpt-5.6-terra` | document/code/test/build/decision/question 均可追到 Role/Run/版本。 |
| DEV-04-08 | 基线过期、同文件冲突与集成任务提示 | 04-03～07、02-04 | `gpt-5.6-sol` | 不静默覆盖；陈旧结果、合并冲突和上游重开均产生可操作状态。 |

## 8. DEV-05：应用用例、读模型与类型化 IPC

阶段出口：所有核心业务都可通过稳定命令/查询完成，UI 无需直碰数据库、Git 或 Adapter。

| 节点 | 工作内容 | 依赖 | 模型 | 节点审核标准 |
|---|---|---|---|---|
| DEV-05-01 | Team/Role/Workspace 生命周期命令与查询 | 02-02、04-01 | `gpt-5.6-terra` | 创建、编辑、停用、恢复、归档完整；运行中 Team 不可误归档。 |
| DEV-05-02 | Harness 注册、健康、诊断与禁用用例 | 02-03、03-03、03-09 | `gpt-5.6-terra` | 凭据不回显；健康与诊断结果可追踪且错误分类稳定。 |
| DEV-05-03 | Binding 预检、三种换绑和历史查询 | 02-03、02-11、03-08 | `gpt-5.6-sol` | 完成后换绑/取消重开/接管分支均不改 role_id、不篡改历史。 |
| DEV-05-04 | Task 创建、分配、依赖、领取与迁移 | 02-04、02-11～14、04-02 | `gpt-5.6-sol` | TODO 不可领取；引用/依赖/权限/版本在同一命令边界校验。 |
| DEV-05-05 | Run 启动、取消、重试、对账与接管 | 02-05、03-06～08、04-04～06 | `gpt-5.6-sol` | 重试幂等、旧 token 失效、UNKNOWN 对账和人工身份留痕成立。 |
| DEV-05-06 | Review 提交、通过、退回与重开 | 02-05～06、02-11 | `gpt-5.6-terra` | REVIEW 有 Artifact；DONE 有验收；退回有意见并进入合法状态。 |
| DEV-05-07 | Thread/Message 发送、回复、确认、重试与改派 | 02-08、02-12～14、05-03 | `gpt-5.6-sol` | 路由按投递时有效 Binding；改派权限、TTL、幂等和历史快照正确。 |
| DEV-05-08 | NotificationPolicy、Notification 与 AttentionItem | 02-09、02-12～14 | `gpt-5.6-sol` | 11 类默认通知、去重、升级和待处理聚合均可验证。 |
| DEV-05-09 | Approval 创建、批准、拒绝与失效 | 02-10、02-12～14 | `gpt-5.6-sol` | scope/期限/actor 全校验；拒绝或过期不授予能力；结果驱动正确后续状态。 |
| DEV-05-10 | Activity/Audit 查询、游标分页与导出任务 | 02-07、02-15 | `gpt-5.6-terra` | 可按 Team/Role/Object/Event/Time 查询；导出可取消、校验且无秘密。 |
| DEV-05-11 | 页面 read model 与 IPC 契约集成 | 05-01～10、01-07 | `gpt-5.6-terra` | 每个 Figma 核心页有最小 read model；无 N+1 热点；错误 envelope 一致。 |

## 9. DEV-06：前端设计系统与产品界面

阶段出口：Figma 核心体验成为真实可操作界面，所有写操作由 IPC 领域规则裁决，关键流程桌面/窄屏/键盘可用。

| 节点 | 工作内容 | 依赖 | 模型 | 节点审核标准 |
|---|---|---|---|---|
| DEV-06-01 | 设计 Token、字体、图标与基础主题 | 00-09、01-06 | `gpt-5.6-luna` | 色值/间距/圆角/字号与 Figma 基线一致；无魔法色散落。 |
| DEV-06-02 | Button/Input/Dialog/Table/Status 等基础组件 | 06-01 | `gpt-5.6-terra` | 键盘、焦点、禁用、错误、loading、危险态和 axe 测试通过。 |
| DEV-06-03 | 正式应用壳、导航、错误边界与断连条 | 06-01～02、05-11 | `gpt-5.6-terra` | 所有正式入口可达；断连不伪装实时；错误边界保留可恢复上下文。 |
| DEV-06-04 | Team/Workspace 创建、编辑与归档界面 | 06-02～03、05-01 | `gpt-5.6-terra` | 必填、冲突、路径权限、加载/失败和影响确认完整。 |
| DEV-06-05 | 团队概览与 Role 席位 | 06-03、05-01～03 | `gpt-5.6-terra` | Role 视觉优先，Harness 标注当前绑定；在线状态不冒充任务完成。 |
| DEV-06-06 | Task 看板、列表、筛选与空状态 | 06-02～03、05-04 | `gpt-5.6-terra` | TODO/READY 可识别且 TODO 不可领取；状态、Role、依赖、待处理可筛选。 |
| DEV-06-07 | Task 创建/编辑真实表单 | 06-02～03、05-04 | `gpt-5.6-terra` | owner/reference/dependency/initial status 全可编辑；服务端 field error 精确落位。 |
| DEV-06-08 | Task 详情与控制页 | 06-03、05-04～06、05-11 | `gpt-5.6-sol` | Task/Run 状态分离；引用、依赖、Run、Artifact、活动和控制入口同步真实状态。 |
| DEV-06-09 | Run 详情、失败、UNKNOWN、重试与接管界面 | 06-02～03、05-05 | `gpt-5.6-sol` | 影响、旧租约、新 Run 和 Human 行为清晰；重复点击不产生重复 Run。 |
| DEV-06-10 | Review 提交、验收通过与退回界面 | 06-02～03、05-06 | `gpt-5.6-terra` | 评审意见真实保存；缺 Artifact/权限时不可伪成功。 |
| DEV-06-11 | Workspace 文件、Markdown、Artifact 与引用修复 | 06-03、04-02～08、05-11 | `gpt-5.6-terra` | Task↔文件双向定位；Git 状态、版本、失效/过期/冲突可理解。 |
| DEV-06-12 | Role/Binding 历史与三种换绑流程 | 06-02～03、05-03 | `gpt-5.6-sol` | 运行中影响、待生效状态、旧/新 Binding 时间线和消息路由均准确。 |
| DEV-06-13 | Harness 注册、健康检查与诊断界面 | 06-02～03、05-02 | `gpt-5.6-terra` | 凭据只显示引用；能力/版本/失败诊断与重试入口完整。 |
| DEV-06-14 | Message/Thread、发送、回复和投递终态 | 06-02～03、05-07 | `gpt-5.6-sol` | Role 目标、Task/Reference、reply_to、状态时间线、重试/改派/新消息均真实。 |
| DEV-06-15 | NotificationPolicy 与“待我处理”队列 | 06-02～03、05-08～09 | `gpt-5.6-sol` | 默认 11 类通知可配置；审批、Run/消息异常、决策和验收统一可处理。 |
| DEV-06-16 | Activity Stream、审计详情与导出状态 | 06-02～03、05-10 | `gpt-5.6-terra` | 筛选、分页、关联跳转、导出进行中/成功/失败均有真实数据。 |
| DEV-06-17 | 响应式、键盘、读屏与减少动态效果收口 | 06-04～16 | `gpt-5.6-sol` | 窄屏可处理失败 Run/审批/消息；四个关键流程全键盘完成；WCAG 2.2 AA 证据齐全。 |

## 10. DEV-07：纵向闭环、异常与恢复

阶段出口：用户故事不是页面拼图，而是通过真实 DB、权限、Adapter、Workspace 与 UI 的端到端闭环。

| 节点 | 工作内容 | 依赖 | 模型 | 节点审核标准 |
|---|---|---|---|---|
| DEV-07-01 | Team→Role→Harness→Workspace 初始化闭环 | 05-01～03、06-04～05、06-12～13 | `gpt-5.6-terra` | AC-01/02：未绑定 Role 可存在；绑定后 ID/权限/历史不变。 |
| DEV-07-02 | 文件驱动 Task→Run→Review→DONE 主链 | 05-04～06、06-06～11 | `gpt-5.6-sol` | AC-03/04：引用可读、Event 完整、REVIEW/DONE 条件不可绕过。 |
| DEV-07-03 | 双 Task 并行 worktree 与冲突闭环 | 03-07、04-04、04-08、06-08～11 | `gpt-5.6-sol` | AC-05：不同目录执行；同文件不静默覆盖；出现明确集成/阻塞处理。 |
| DEV-07-04 | 依赖 DAG、循环拒绝和下游通知 | 02-04、05-04、05-08、06-06～08 | `gpt-5.6-sol` | AC-06：前置未完成不可自动执行；完成后 READY+通知；环路可定位。 |
| DEV-07-05 | Harness 崩溃、失败、超时与幂等重试 | 03-05～08、05-05、06-09 | `gpt-5.6-sol` | AC-07：旧 Run/Artifact 保留；新 Run 唯一；晚到结果被 fencing 拒绝。 |
| DEV-07-06 | 三种换绑与上下文继承 | 05-03、05-07、06-12、06-14 | `gpt-5.6-sol` | AC-08：Role/权限/任务/线程不变；新 Harness 从显式启动包继续。 |
| DEV-07-07 | 越权、审批、凭据脱敏闭环 | 02-13、03-09、05-09、06-15 | `gpt-5.6-sol` | AC-09：越权零副作用且有 Event；Human-only 门禁；全部输出无秘密。 |
| DEV-07-08 | 平台重启、UNKNOWN 对账与恢复 | 02-15、03-08、05-05、06-09 | `gpt-5.6-sol` | AC-10：实体完整；未知 Run 不重复派发；三类对账结果可处理。 |
| DEV-07-09 | Human 暂停、接管、改派与取消 | 05-05、06-08～09、06-15 | `gpt-5.6-sol` | AC-11：旧执行写权限撤销；Human actor 留痕；恢复路径明确。 |
| DEV-07-10 | Message 跨换绑、失败/过期与改派 | 05-07～08、06-14～15 | `gpt-5.6-sol` | 新投递走新 Binding；历史快照不改；终态可重试/改派/新建后续。 |
| DEV-07-11 | Task 到 Event/Binding/Run/Artifact/Review 的审计回放 | 05-10、06-16、07-02～10 | `gpt-5.6-sol` | AC-12：任一 DONE Task 可一跳或有限跳数得到完整证据链，导出校验通过。 |
| DEV-07-12 | 断连、引用失效、版本冲突与恢复 UX | 06-03、06-07～17 | `gpt-5.6-terra` | 错误不丢输入、不误报成功；用户能刷新对比、修复引用或安全重试。 |

## 11. DEV-08：系统验收、性能、安全与候选发布

阶段出口：AC-01～AC-12 全绿，质量、安全、恢复与可访问性证据齐全，才允许称为 MVP 候选版本。

| 节点 | 工作内容 | 依赖 | 模型 | 节点审核标准 |
|---|---|---|---|---|
| DEV-08-01 | 完成需求追踪与 AC-01～12 自动验收套件 | 07-01～12 | `gpt-5.6-sol` | 每项 AC 至少一个自动用例和证据链接；失败直接阻断候选版本。 |
| DEV-08-02 | 1 万 Task/Event 与 5 并发 Run 性能基准 | 07-01～12 | `gpt-5.6-sol` | 常用页 P95 <2s、分派可见 P95 <10s；报告含环境、数据、瓶颈与回归阈值。 |
| DEV-08-03 | 安全测试与威胁模型复核 | 07-03～10 | `gpt-5.6-sol` | 路径穿越、命令注入、权限提升、秘密泄露、恶意 Adapter、依赖漏洞均有结论。 |
| DEV-08-04 | 可访问性与窄屏真实设备验收 | 06-17、07-12 | `gpt-5.6-terra` | axe 无严重问题；关键流程键盘/读屏通过；窄屏异常处置闭环。 |
| DEV-08-05 | 备份、升级、回滚与崩溃恢复演练 | 02-15、07-05、07-08 | `gpt-5.6-sol` | 从上一 schema 升级、备份恢复、进程中断后恢复均不丢关键事实。 |
| DEV-08-06 | macOS 安装、签名前检查与最小权限复核 | 01-06、08-01～05 | `gpt-5.6-sol` | 新用户环境可安装/卸载；权限提示与实际 capability 一致；不声明未完成签名/公证。 |
| DEV-08-07 | 文档、运维手册与 Adapter 接入说明校准 | 08-01～06 | `gpt-5.6-terra` | 安装、备份、恢复、诊断、数据位置、隐私和已知限制与实际行为一致。 |
| DEV-08-08 | 候选版本 Go/No-Go 评审 | 08-01～07 | `gpt-5.6-sol` | 所有 P0 gate 通过；遗留项有 owner/风险/版本，不存在安全、数据或 AC 阻断项。 |

## 12. 建议首批并行波次

只有 `DEV-00-10` 获得 Human 确认后才启动：

| 波次 | 可并行节点 | 说明 |
|---|---|---|
| Wave 1 | DEV-01-01～04、DEV-02-01、DEV-03-01、DEV-06-01 | 工程壳、基础值对象、Adapter Schema、设计 Token 互不抢占。 |
| Wave 2 | DEV-01-05～09、DEV-02-02～10、DEV-03-02～05 | 先建立可验证底座与故障注入。 |
| Wave 3 | DEV-02-11～15、DEV-03-06～11、DEV-04-01～03 | 状态/一致性、真实 Adapter Spike、只读 Workspace 可并行。 |
| Wave 4 | DEV-04-04～08、DEV-05-01～11、DEV-06-02～03 | Workspace 隔离、应用用例、前端基础并行，契约统一。 |
| Wave 5 | DEV-06-04～17 | 页面按领域能力逐条接通，不按静态画板批量造假。 |
| Wave 6 | DEV-07-01～12 | 以 AC 纵向闭环收敛系统。 |
| Wave 7 | DEV-08-01～08 | 系统级验证与候选版本判断。 |

## 13. 节点状态

统一状态为：`PLANNED → READY → DOING → REVIEW → DONE`，另有 `BLOCKED` 和 `CANCELLED`。节点只有在依赖均为 `DONE`、输入资料齐全且模型可用时进入 `READY`。任何安全、数据或协议缺口必须进入 `BLOCKED`，不得降为“已知问题”后继续穿透阶段门禁。
