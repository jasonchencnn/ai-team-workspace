# 工作进度与后续开发计划（2026-10-09）

> 2026-10-10 DEV-02-09最新：[限定整改审查](../reviews/DEV-02-09-SCOPE-REVIEW-2026-10-10.md)已修复通知异ID去重替换与Task父端跨Team引用，当前迁移0001～0035。新专项13/0、注册2/0、恢复后persistence303/0，当前stable Mac21整体PASS（21/21，Rust723/0/1、TS36/0，226项工程/策略输入不变），当前C7两层行/分支均达标。可信Human身份未闭环，09仍BLOCKED；正式 **35 DONE / 6 BLOCKED / 60 PLANNED**、Gate C BLOCKED不变。最小后续为[Owner身份决策](../coordination/DEV-02-09-OWNER-DECISION-2026-10-10.md)，不把全部05-08用例倒挂为09前置，不接受ADR-0007或恢复整批外派。下列HC-01等按历史时点保留。


> 2026-10-10 HC-01终审最新：DEV-02-08基础存储恢复DONE，[终审](../reviews/DEV-02-08-HC01-CLOSURE-REVIEW-2026-10-10.md)确认正式0033、完整历史及改派后换绑；原57+兼容11+注册1、全persistence288/0，当前stable Mac21整体PASS，Rust708/0/1、TS36/0，222项工程/扫描策略输入不变。正式 **35 DONE / 6 BLOCKED / 60 PLANNED**；Gate C仍BLOCKED，未启动下游产品用例。当前覆盖率Domain行91.67%/分支93.36%、Application行93.75%/分支100%。以下34/7/60、HC-01待回传等按历史时点保留。

> 2026-10-10 历史/兼容核对最新：[08专项审查](../reviews/DEV-02-08-HISTORY-COMPAT-REVIEW-2026-10-10.md)现有六套57/0，旧历史保留；新仓外回归确认改派到B后B换绑无法采用当前Binding，HC-01阻断08。已签[Minimax专项包](MINIMAX-HISTORY-COMPAT-PACKET-2026-10-10.md)供Human转发，仅仓外修复候选与互斥回传目录，不恢复整批外派。生产工程219项SHA不变，本轮未重跑Mac21或重签节点；正式 **34 DONE / 7 BLOCKED / 60 PLANNED**、Gate C BLOCKED不变。后续权限/幂等/dispatcher/产品用例仍按12/13/14/05/07归属，不倒挂为08新前置。以下按日期保留历史。

> 2026-10-10 07终审最新：按冻结专属标准完成[责任核对与单节点终审](../reviews/DEV-02-07-SCOPE-CLOSURE-REVIEW-2026-10-10.md)，07基础节点恢复DONE；正式 **34 DONE / 7 BLOCKED / 60 PLANNED**。58专项/七项真实反例、当前stable Mac21整体PASS，Rust696/0/1、TS36/0，219项工程输入不变。原先笼统列入07的后续格式扩展、dispatcher和host接线已按冻结依赖归还02-14/05/07后续验收，不新增语义或启动下游。Gate C仍阻塞；下列07 BLOCKED和33/8/60等段落为历史时点。


> 2026-10-10 分支补测最终：四个Application遗漏结果已覆盖，Domain行91.67%/branch93.36%，Application行93.75%/branch100%；当前受检输入stable Mac21整体PASS，Rust696/0/1、TS36/0。依据[本轮验收及单节点复核](../reviews/DEV-02-07-BRANCH-TEST-REVIEW-2026-10-10.md)，仅DEV-02-11恢复DONE，正式 **33 DONE / 8 BLOCKED / 60 PLANNED**。07整节点与Gate C继续阻塞；生产源码、迁移、manifest/lock和用户stable/default不变，工程仅补测试。下列旧32/9/60、branch未证或77.78%均为历史时点。


> 2026-10-10 C7最新：Human已接受ADR-0009，官方候选核验、仓外隔离安装与单路/双路真实branch探针PASS；[专项验收](../reviews/C7-BRANCH-COVERAGE-REVIEW-2026-10-10.md)测得Domain行91.67%/分支93.36%，Application行91.25%/分支77.78%。Application分支未达85%，Gate C继续阻塞；正式 **32 DONE / 9 BLOCKED / 60 PLANNED** 不变，未批量重签节点。四个未覆盖结果及补测方向已记录；生产源码、lock与用户stable/default不变。下列“branch未证/工具待Human决策”仅属历史时点。


> 2026-10-10 最新执行：DEV-02-07 原子消费/去重、投影重建及三处真实进程崩溃重放已在限定范围通过，[验收](../reviews/DEV-02-07-AUDIT-CONSUMER-REVIEW-2026-10-10.md)为当前 canonical Mac21 PASS，Rust692/0/1、TS36/0。当前迁移0001～0032；行覆盖率Domain91.90%、Application93.41%，branch仍未证。正式 **32 DONE / 9 BLOCKED / 60 PLANNED** 不变；dispatcher、广义历史格式/host装配、通知和完整阶段未准入。下列旧统计和迁移范围按历史时点保留。

当前08基础存储终审已完成，HC-01无需再次派发；下一步按剩余六个BLOCKED节点的Frozen责任继续，02-09/ADR-0007未决输入先做决策准备。以下为07终审后的历史接续计划：本轮07基础节点终审完成，ADR-0009工具与分支补测均已通过。下一步优先核对DEV-02-08的正式改派历史/兼容责任并签限定包；不重复派发已通过的原子消费、崩溃或四个分支补测。02-14仍受12/13直接依赖阻断；ADR-0007、PENDING语义及产品host接线继续独立处理。详见[07终审](../reviews/DEV-02-07-SCOPE-CLOSURE-REVIEW-2026-10-10.md)，以下更早接续段落保留为历史计划。

> 本轮最新执行：DEV-02-07 内部 Message 到期事务子包已通过，[验收](../reviews/DEV-02-07-ATOMIC-EXPIRY-REVIEW-2026-10-09.md)为 canonical Mac21 整体 PASS、Rust674/0/1、TS36/0，13项应用真库测试与3项行为反例通过。当前迁移0001～0031；application不再仅为占位，但consumer/dispatcher、可信Human/Role权限、PENDING到期语义与覆盖率仍未完成。正式 **32 DONE / 9 BLOCKED / 60 PLANNED** 不变；旧段落数字和迁移范围按历史时点保留。

本轮已按先计划后执行落地 [DEV-02-07 执行包](DEV-02-07-ATOMIC-EXPIRY-PACKET-2026-10-09.md)。接续顺序：先补07的本地消费投影、event_consumptions原子去重和至少一次重放证据；再按02-11～14的直接依赖逐项复核权限、dispatcher与运行时。PENDING到期与ADR-0007需正式语义决策，branch需有效工具证据；这些未决不妨碍独立的07消费整改，但不得穿透完整节点或阶段门禁。以下 C5/C6 段落是历史盘点，不是本輪代码仍为空的声明。

> 执行更新：C6已完成并重签12个底座节点；随后DEV-02-08的前向0030及存量存储整改已交付，[最终验收](../reviews/DEV-02-08-ROUTE-SCHEMA-REVIEW-2026-10-09.md)为Mac21整体PASS、Rust661/0/1、TS36/0。正式32 DONE / 9 BLOCKED / 60 PLANNED不变，08应用执行链仍BLOCKED。下文C5/C6数字按测量时点保留，后续优先真实Event/Outbox事务与应用接线。

本项目已完成冻结基线、工程壳、领域规则和一批真实 SQLite / Mock Adapter 能力；10-09 本轮整改及完整 Mac21 工程门禁通过。真实应用服务、Workspace、安全执行点及业务 UI 尚未接通，当前仍处于底座收口阶段。

正式状态为 **32 DONE / 9 BLOCKED / 60 PLANNED，共 101 个节点**。C6已完成，12个底座节点已按依赖重签；本轮实际结论见[C6/C7准入审查](../reviews/C6-C7-ADMISSION-REVIEW-2026-10-09.md)。该数字是节点账本，不能换算为产品完成率。本文件整理进度和条件开发顺序，不改变节点状态、冻结语义、任务授权或发布权限。Minimaxcode 外派继续暂停，由 Codex 内部维护实施与独立审核。

## 1. 事实来源与本次核对范围

优先采用 [正式节点表](../development/DEVELOPMENT_NODES.md)、[10-09 收口审计](../reviews/CODEX-AUDIT-CLOSURE-2026-10-09.md)、[当前台账](../reviews/evidence/codex-audit-closure-2026-10-09/inventory/closure-ledger.current.json) 和 [C6–C10 接续安排](CODEX-AUDIT-CLOSURE-PLAN-2026-10-09.md)。专项标准和通用门禁见 [开发基线](../development/README.md)、[质量门禁](../development/QUALITY_GATES.md)、[冻结规格索引](../specs/README.md)。

本次只做仓库盘点与计划文档同步：核对正式节点分组、现有源文件、最终 gate JSON / rawlog 的保存哈希，以及本轮 6 个工程文件与收口报告所列 SHA 的一致性。结果为两份门禁证据哈希匹配、6/6 工程文件匹配。未重新执行产品测试、覆盖率测量或完整 Mac21；下列测试数字引用已保存的 C5 实测证据。

审计与本次计划文档均在门禁后输出，全仓输入因此不能继续声称与受检 4,676 文件完全相同。工程文件匹配只证明本次检查的范围；后续源码、配置或锁文件发生变化后，应按门禁失效规则重新验证。

## 2. 当前进度

| 开发域              | DONE | BLOCKED | PLANNED | 当前交付与缺口                                                  |
| ------------------- | ---: | ------: | ------: | --------------------------------------------------------------- |
| DEV-00 规格与决策   |   10 |       0 |       0 | V1 基线已冻结；新增通知语义及 V2 决策另行处理                   |
| DEV-01 工程底座     |    9 |       0 |       0 | 工程底座全部正式DONE；应用host接线与产品门禁另行推进            |
| DEV-02 领域与持久化 |    7 |       7 |       1 | 02-02～06/10已重签；07/08/09及11～14继续BLOCKED                 |
| DEV-03 Adapter      |    5 |       2 |       4 | V0、supervisor、握手及Mock正常/故障流DONE；06/07运行时仍BLOCKED |
| DEV-04 Workspace    |    0 |       0 |       8 | crate 仍为占位，路径、Git 隔离和 Artifact 用例待开发            |
| DEV-05 应用与 IPC   |    0 |       0 |      11 | 已有07内部事务与固定审计投影；业务 IPC 与完整用例待开发        |
| DEV-06 前端         |    1 |       0 |      16 | Token 与桌面壳已有；业务页面、键盘和窄屏流程待开发              |
| DEV-07 纵向闭环     |    0 |       0 |      12 | AC 主链、异常恢复与审计回放待开发验收                           |
| DEV-08 系统验收     |    0 |       0 |       8 | 性能、安全、恢复、设备、安装与候选版本评审待执行                |
| 合计                |   32 |       9 |      60 | 不批量晋级，不跳过依赖                                          |

当前代码盘点：application 已有内部到期事务、审计消费与专用端口；workspace/security 仍为占位；`apps/desktop/src/App.tsx` 展示桌面壳。现有名为 integration / e2e 的测试包含工具链验证，不能据名称认定产品端到端验收已经完成。

10-09 已完成的工程验证：

- canonical Mac21 **整体 PASS，21/21 exit0，两个 companion 前后匹配**；当前开发机为 macOS 27.0.1 / arm64，不代替最低 macOS14 设备验收。
- Rust **653 passed / 0 failed / 1 ignored**，TS **36/0**，Gate26 与 replay8 合计 **34/0**。ignored 项不计为通过。
- Adapter fixture、Drop 后代形成及退出、crash 快照竞态和日志时钟归一化均有定向及独立反例证据；修复后的完整 Adapter 默认并行 3 轮，各 **93/0**。
- 树及全部 refs 秘密扫描无 finding；source-map-js 高危补丁已整合。npm 保留 1 moderate / 1 low，RustSec 保留 8 条信息性告警，许可证保留 3 条 unused allowance 告警。
- domain 既有行覆盖率 **2016/2197 = 91.76%**；branch ≥85% 尚未证明。本轮当前行测量Domain91.90%、Application93.41%；branch仍未验证，Gate C 尚未通过。

## 3. 已关闭的问题与仍需收口的节点

六对象留存、Run 非法终态迁移、stdout 非 JSON 前缀、ignored dotenv 输入身份、单层百分号编码脱敏、Drop 非空性及本轮工程竞态已在各自审验范围闭环。后续不重复派发这些已关闭故障；C6 应检查它们能否满足完整节点标准，并验证证据与最终输入的关联。

以下保留开工时21个阻塞节点的初步盘点，属于重签前计划快照；当前12个已重签、9个继续BLOCKED。完整逐节点G0–G9与剩余任务以[C6/C7准入审查](../reviews/C6-C7-ADMISSION-REVIEW-2026-10-09.md)为准，不再把下表全部当作当前阻塞。

| 重签前节点（21 个） | 已有成果                                                                 | 剩余工作与下一步                                                                                                      |
| ------------------- | ------------------------------------------------------------------------ | --------------------------------------------------------------------------------------------------------------------- |
| DEV-01-05           | canonical Mac21、输入身份和扫描例外修复通过                              | 核对专属标准、required gate 实际执行边界和可复现证据；本机结果不冒称 hosted branch protection；满足全部条件后单独重签 |
| DEV-01-08           | 脱敏行为及 39 个相关测试通过                                             | 核对 request/task/run 关联及日志、错误、Event 的覆盖范围；尚无应用 Event 输出的部分明确登记，再判定重签条件           |
| DEV-02-02/03/04     | Team/Role/Workspace、Harness/Binding、Task/Dependency 迁移与留存专项成果 | 按对象逐项核对稳定身份、唯一性、历史区间、DAG、版本及升级证据；已修留存不继续当开放故障                               |
| DEV-02-05           | Run/Lease/Review 约束和 0029 终态矩阵                                    | 核对 claim、世代、attempt、Review 关系及真实事务证据，区分 DB 约束与应用执行责任                                      |
| DEV-02-06           | Artifact/Reference 迁移及留存成果                                        | 核对 creator/Run/版本/基线/可见性，区分元数据约束与后续 Workspace 真文件证据                                          |
| DEV-02-07           | Event/Outbox 迁移及 0024 默认重试值修复                                  | 补齐业务变更 + Event + Outbox 原子提交、消费去重与失败恢复证据                                                        |
| DEV-02-08           | Message/Delivery 历史、路由与概念存量实验                                | 核对缺失的最终证据和可信投递边界；概念版本30不是生产 migration，旧29加载器回退不可用；后续生产方案须另签范围          |
| DEV-02-09           | 已明确规则下的 Notification/Attention 守卫                               | ADR-0007 待 Human 决策；Owner、filters、escalation、强制通知、Run/Message/SYSTEM 处理语义未准入                       |
| DEV-02-10           | Permission/Approval 约束及局部修复                                       | 核对真实主体、scope、期限、策略版本与撤销证据，避免由格式正确的 UUID 推断可信身份                                     |
| DEV-02-11           | Task/Run/Binding 纯状态机                                                | 前置 02-02～05 重签后，核对当前专项、性质与有效覆盖率证据                                                             |
| DEV-02-12           | Message/Delivery/Approval 纯状态机                                       | 前置 02-08～10 重签后复核；通知未决不能由纯状态机测试代替                                                             |
| DEV-02-13           | 权限矩阵、求交、到期和审批 context 纯规则                                | 接入可信 actor 与当前资源/路径/Binding/Run 查询；拒绝零业务副作用并有适用 Event                                       |
| DEV-02-14           | 乐观锁、幂等记录、Outbox 重试纯规则                                      | 两连接竞争、相同 key 首结果回放、payload 冲突、dispatcher claim/去重/PROCESSING 恢复及原子事务                        |
| DEV-03-02/03/04/05  | supervisor、握手、Mock 正常及故障注入；本轮进程证据通过                  | 01-08 重签后按 03-02→03-03→03-04→03-05 顺序核对全部标准；保持 ADR-0006 进程责任边界                                   |
| DEV-03-06/07        | 取消/超时/重试及 fencing 纯判定                                          | 在实际取消、重试、续期和最终写入点验证；旧 token、UNKNOWN/终态、晚到回调、并发及恢复均须真实 DB + Mock 证据           |

## 4. 推荐开发顺序与出口

继续旧基线收口是当前执行路径。近期沿用 C6–C10 编号，后续以 DEV 节点排期。时间估算待 C6 查清实际缺口后按工作包给出，暂不承诺发布日期。

| 顺序              | 工作 / 节点                                                   | 进入条件与范围                                                                                               | 可验收产物 / 出口                                                                                                                              |
| ----------------- | ------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------ | ---------------------------------------------------------------------------------------------------------------------------------------------- |
| 1，立即           | **C6 差距核对与首批重签候选**；01-05/08、02-02～10、03-02～05 | 先只读规格、代码和证据；在既有修复授权内签发精确补证/修复包                                                  | 21 个 BLOCKED 各自的专属标准 + G0–G9 矩阵，缺口类型、owner、证据、写集、验收命令；优先核对 01-05/08，再按依赖核对迁移及 Mock；逐节点签核       |
| 2，与 C6 调研并行 | **C7 覆盖率方案；C8 通知决策准备**                            | C7 调研现有工具与真实分母；C8 保持 ADR-0007 Proposed                                                         | branch 有效证据方案；若需更改冻结工具路径，形成 Human 可决策的 ADR。通知七条建议与 B01–B08 审计项对应清楚，接受后才同步规格、迁移和实现        |
| 3                 | **C9 权限、事务与 Outbox**；02-11～14                         | 对应迁移节点、02-09 决策及直接依赖正式 DONE；先签内部端口和实施范围                                          | 可信身份/当前上下文、权限拒绝审计、版本竞争、claim、幂等首结果、业务/Event/Outbox 原子事务及 dispatcher 恢复的真库测试；补齐覆盖率并逐节点重签 |
| 4                 | **C9 Adapter 运行时收口**；03-06/07                           | 03-02～05、02-05/11/14 等直接依赖正式 DONE                                                                   | SQLite + Mock 的取消确认/超时/重试、旧 token 三类回调拒绝、最终写入 fencing、无双主 Run；满足原节点标准                                        |
| 5                 | **C10 第一批基础能力**；02-15、03-08/09、04-01～03            | 各自直接依赖 DONE；02-15 完成后才进入03-08，04-01 完成后才进入04-02/03；备份、恢复、凭据和路径任务按写集分开 | 一致性备份/迁移恢复、UNKNOWN 对账、Keychain 引用、Workspace 路径与 Markdown 锚点、Git 只读查询；Gate C 所需证据完整                            |
| 6                 | **隔离写入与真实 Harness**；04-04～08、03-10/11               | Workspace/fencing/Keychain 等直接依赖 DONE；真实账号/服务操作先获具体授权                                    | worktree 隔离、非 Git 单写者、Artifact 和冲突证据；真实 Codex Spike、Gate D/E、第二 Harness Go/No-Go                                           |
| 7                 | **应用与界面接通**；05-01～11、06-02～17                      | 每个节点直接依赖 DONE；基础组件06-02可在单独获准任务包内提前准备                                             | 按领域能力逐条接 IPC/read model：初始化、文件驱动 Task、Run、Review、协作消息/通知、审计；真实数据、失败状态和键盘流程；Gate F/G               |
| 8                 | **产品主链与候选验收**；07-01～12、08-01～08                  | 相应业务/界面依赖 DONE；真实设备与账号边界明确                                                               | AC-01～12、并发/崩溃/换绑/接管/审计、性能/A11y/恢复、macOS14与安装；Gate H/I，最终发布由 Human 决定                                            |

**接线边界要在 C6 明确**：02-13/14 的原节点标准已经要求执行点与事务；所需最小 application 端口和编排必须归入这两个节点的精确收口包。完整 DEV-05 产品用例仍按原依赖推进，不能先启动 DEV-05 再倒填前置 DONE，也不能把本计划视为扩大写集的许可。若现有节点范围确实无法覆盖，列出具体冲突供 Human 决定拆分或替代 ADR。

C9 内部建议分成互斥工作包：权限执行点、事务/幂等/Outbox、Adapter 运行时。共享 `application/src/lib.rs` 和 persistence 根模块由主控串行整合；具体模块路径在开工包中冻结，避免多人同时修改占位入口。迁移仅前向追加，现有 0001～0029 及历史证据保留。

## 5. 下一轮任务包、验证与分工

下一轮先做以下准备，不开启未满足条件的新产品实现：

1. **C6**：主控负责节点差距报告和重签判定；Luna 可做证据/链接/哈希盘点，Terra 可做独立规格与代码审核。允许写集仅为新差距报告及新审核证据，正式状态表由主控在验收成立后单独修改。验收为 21 节点无遗漏、每个适用标准有证据或明确缺口、直接依赖逐项核对、链接和差异检查通过。
2. **C7**：只读核对冻结工具支持的 line/branch 指标及 domain/application 实际分母，允许写新方案/证据；不安装 nightly、不改工具链、不用 region 充当 branch。不满足 ≥90% line / ≥85% branch 时保留 Gate C 阻断。
3. **C8**：主控整理 ADR-0007 七条建议与审计 B 项，明确接受、修订或保留 Proposed 的影响。决策后才签真实身份端口、策略解释、投递/Attention 原子事务与前向迁移实现包。

内部模型遵守 AGENTS 的 Luna/Terra 路由，最多 3 个并发、写集互斥；模型工具不可用时记录限制，由主控承担范围内工作，不自动升级 Subagent。根 manifest/lockfile、共享入口、最终 Mac 门禁与正式准入由主控串行负责。R3 实现者与独立审核者分离，实际模型、分工、命令和反例进入交付记录。

实现包按风险选择定向测试，然后执行受影响检查；正式门禁仍用原 runner 和 canonical21，例如：

```sh
cargo test --workspace --locked --offline --no-fail-fast
cargo clippy --workspace --all-targets --all-features --locked --offline -- -D warnings
cargo fmt --all -- --check
pnpm run test:all
pnpm run typecheck
pnpm run lint:ts
pnpm run build
pnpm run contracts:check
node --test scripts/ci/local-gates.test.mjs scripts/ci/replay-env-inputs.test.mjs
node scripts/ci/local-gates.mjs --platform macos --evidence /absolute/path/outside-repository/gate.json
```

最后一条必须使用实际仓库外证据目录、当前冻结工具和批准的联网范围；完整 runner 自带扫描、工具、输入身份及 companion 验证，不能用前几条命令代替。本次文档整理只运行链接/统计/哈希/差异检查，不为未变的代码重复全量套件。

## 6. 决策、环境与风险

- **ADR-0007**：仍待 Human 对七条通知建议明确决定，阻断02-09及依赖；一般计划请求不视为接受。
- **V2 路径**：R0 Rescan 已有；Conversation、ProviderSession、Gateway/ACP、Leader 权限和 MCP 的 R1 仍未冻结。本轮继续旧节点收口。若 Human 改为 V2 优先，应先确认 R1 解冻范围，形成新/替代 ADR、AC 与节点图，并明确旧基线保留/替代关系后再排实现。
- **真实环境**：Keychain、真实 Harness、最低 macOS14、桌面 E2E、安装/签名相关行为依各自边界验收；外部服务、账号、设备权限和费用先说明具体影响再确认，费用上限保持 $0。既有公共依赖审计授权不扩大到真实 Harness 或发布。
- **维护风险**：中低危和 Rust 信息性告警按目标依赖树评估，最小补丁后做受影响验证；不把非 Mac 平台写成已验证，也不以告警存在为由整批升级栈。
- **工作树留存**：当前有大量既存未提交/未跟踪改动，计划与审核应绑定 dirty 输入和证据哈希，不能用 HEAD 当当前状态或安全回退点。提交、推送、部署和生产数据操作仍需 Human 明确要求。

C6差距矩阵和首批12个基础节点重签已经完成，C7工具调查已完成但branch门槛仍未满足。接下来优先落实DEV-02-07的真实Event/Outbox事务包、DEV-02-08的正式改派历史与兼容包，并准备覆盖率工具决策；ADR-0007保持待Human明确决定。
