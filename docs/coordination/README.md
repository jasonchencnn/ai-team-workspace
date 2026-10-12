# 当前协作与派工入口

> 2026-10-10 DEV-02-09最新：[限定整改审查](../reviews/DEV-02-09-SCOPE-REVIEW-2026-10-10.md)已修复通知异ID去重替换与Task父端跨Team引用，当前迁移0001～0035。新专项13/0、注册2/0、恢复后persistence303/0，当前stable Mac21整体PASS（21/21，Rust723/0/1、TS36/0，226项工程/策略输入不变），当前C7两层行/分支均达标。可信Human身份未闭环，09仍BLOCKED；正式 **35 DONE / 6 BLOCKED / 60 PLANNED**、Gate C BLOCKED不变。最小后续为[Owner身份决策](../coordination/DEV-02-09-OWNER-DECISION-2026-10-10.md)，不把全部05-08用例倒挂为09前置，不接受ADR-0007或恢复整批外派。下列HC-01等按历史时点保留。


> 2026-10-10 HC-01终审最新：DEV-02-08基础存储恢复DONE，[终审](../reviews/DEV-02-08-HC01-CLOSURE-REVIEW-2026-10-10.md)确认正式0033、完整历史及改派后换绑；原57+兼容11+注册1、全persistence288/0，当前stable Mac21整体PASS，Rust708/0/1、TS36/0，222项工程/扫描策略输入不变。正式 **35 DONE / 6 BLOCKED / 60 PLANNED**；Gate C仍BLOCKED，未启动下游产品用例。当前覆盖率Domain行91.67%/分支93.36%、Application行93.75%/分支100%。以下34/7/60、HC-01待回传等按历史时点保留。

> 2026-10-10 历史/兼容核对最新：[08专项审查](../reviews/DEV-02-08-HISTORY-COMPAT-REVIEW-2026-10-10.md)现有六套57/0，旧历史保留；新仓外回归确认改派到B后B换绑无法采用当前Binding，HC-01阻断08。已签[Minimax专项包](MINIMAX-HISTORY-COMPAT-PACKET-2026-10-10.md)供Human转发，仅仓外修复候选与互斥回传目录，不恢复整批外派。生产工程219项SHA不变，本轮未重跑Mac21或重签节点；正式 **34 DONE / 7 BLOCKED / 60 PLANNED**、Gate C BLOCKED不变。后续权限/幂等/dispatcher/产品用例仍按12/13/14/05/07归属，不倒挂为08新前置。以下按日期保留历史。

> 2026-10-10 07终审最新：按冻结专属标准完成[责任核对与单节点终审](../reviews/DEV-02-07-SCOPE-CLOSURE-REVIEW-2026-10-10.md)，07基础节点恢复DONE；正式 **34 DONE / 7 BLOCKED / 60 PLANNED**。58专项/七项真实反例、当前stable Mac21整体PASS，Rust696/0/1、TS36/0，219项工程输入不变。原先笼统列入07的后续格式扩展、dispatcher和host接线已按冻结依赖归还02-14/05/07后续验收，不新增语义或启动下游。Gate C仍阻塞；下列07 BLOCKED和33/8/60等段落为历史时点。


> 2026-10-10 分支补测最终：四个Application遗漏结果已覆盖，Domain行91.67%/branch93.36%，Application行93.75%/branch100%；当前受检输入stable Mac21整体PASS，Rust696/0/1、TS36/0。依据[本轮验收及单节点复核](../reviews/DEV-02-07-BRANCH-TEST-REVIEW-2026-10-10.md)，仅DEV-02-11恢复DONE，正式 **33 DONE / 8 BLOCKED / 60 PLANNED**。07整节点与Gate C继续阻塞；生产源码、迁移、manifest/lock和用户stable/default不变，工程仅补测试。下列旧32/9/60、branch未证或77.78%均为历史时点。


> 2026-10-10 C7最新：Human已接受ADR-0009，官方候选核验、仓外隔离安装与单路/双路真实branch探针PASS；[专项验收](../reviews/C7-BRANCH-COVERAGE-REVIEW-2026-10-10.md)测得Domain行91.67%/分支93.36%，Application行91.25%/分支77.78%。Application分支未达85%，Gate C继续阻塞；正式 **32 DONE / 9 BLOCKED / 60 PLANNED** 不变，未批量重签节点。四个未覆盖结果及补测方向已记录；生产源码、lock与用户stable/default不变。下列“branch未证/工具待Human决策”仅属历史时点。


> 2026-10-10 最新执行：DEV-02-07 原子消费/去重、投影重建及三处真实进程崩溃重放已在限定范围通过，[验收](../reviews/DEV-02-07-AUDIT-CONSUMER-REVIEW-2026-10-10.md)为当前 canonical Mac21 PASS，Rust692/0/1、TS36/0。当前迁移0001～0032；行覆盖率Domain91.90%、Application93.41%，branch仍未证。正式 **32 DONE / 9 BLOCKED / 60 PLANNED** 不变；dispatcher、广义历史格式/host装配、通知和完整阶段未准入。下列旧统计和迁移范围按历史时点保留。

> 本轮最新执行：DEV-02-07 内部 Message 到期事务子包已通过，[验收](../reviews/DEV-02-07-ATOMIC-EXPIRY-REVIEW-2026-10-09.md)为 canonical Mac21 整体 PASS、Rust674/0/1、TS36/0，13项应用真库测试与3项行为反例通过。当前迁移0001～0031；application不再仅为占位，但consumer/dispatcher、可信Human/Role权限、PENDING到期语义与覆盖率仍未完成。正式 **32 DONE / 9 BLOCKED / 60 PLANNED** 不变；旧段落数字和迁移范围按历史时点保留。

> 本轮继续执行：[DEV-02-08存储整改验收](../reviews/DEV-02-08-ROUTE-SCHEMA-REVIEW-2026-10-09.md)。0030已注册，8项专项及最终Mac21通过（Rust661/0/1、TS36/0），四个行为变异真实RED并恢复。08完整节点仍BLOCKED，正式32/9/60不变；当前后续是07真实业务/Event/Outbox事务和获准的应用接线，ADR-0007未接受。

> 最新执行结果：[C6/C7准入审查](../reviews/C6-C7-ADMISSION-REVIEW-2026-10-09.md)。C6已完成，当前输入完整Mac21整体PASS；DEV-01-05/08、DEV-02-02～06/10、DEV-03-02～05共12个节点依次重签DONE，正式 **32 DONE / 9 BLOCKED / 60 PLANNED**。下一步补真实Event/Outbox事务、正式改派历史与有效branch方案；ADR-0007仍Proposed。下方20/21/60和“下一步C6”均为重签前快照，Minimax外派继续暂停。

> 更新：2026-10-09。此处替代 2026-09-24 的 Wave 2 动态交接入口；旧版本原样保存在 [归档快照](../archive/coordination-snapshots/WAVE2-README-2026-09-24.md)。本文件不修改冻结规格或节点状态。

> 当前工作进度与后续开发顺序见 [进度与计划](PROGRESS_AND_NEXT_PLAN-2026-10-09.md)：本轮整改及 canonical Mac21 已通过，正式20 DONE / 21 BLOCKED / 60 PLANNED不变。下一步为C6逐节点差距核对及重签候选，C7覆盖率方案和C8通知决策准备；应用/Workspace/安全执行点与业务UI仍待接通。以下旧日期段为历史快照，未关闭项以最新收口报告和正式节点表为准。

> 2026-10-03 新增 **[工作日志 WORKLOG.md](WORKLOG.md)**：面向 Codex 总控审阅的当前状态快照，含本轮实测门禁结果、未提交规模盘点、待裁决疑点与未闭环项登记。**节点状态仍以 [`DEVELOPMENT_NODES.md`](../development/DEVELOPMENT_NODES.md) 为准**；两者冲突时以状态表为准并在工作日志登记该冲突。审阅 Wave 2 现状请先读该文件。

**任务冻结按节点维持**：见[全项目冻结清单](../v2/TASK_FREEZE.md)。Human 已启动[Wave 2 macOS 正式验收](WAVE2_MAC_ACCEPTANCE_PLAN.md)，并授权总控决定技术实现原则、推进至 Wave 2 实现。[首轮记录](../reviews/WAVE-2-MAC-ACCEPTANCE.md)仍保留未通过事实；[前置节点终审](../reviews/WAVE-2-ENTRY-CLOSURE-2026-09-28.md)在最终输入门禁生效后关闭 DEV-02-02 和 DEV-03-02。下游只可按依赖、单节点任务包和独立验收逐项准入；旧任务包模板不是派工授权。

## 当前状态

> 最新（2026-10-09 Human调整）：暂停向Minimaxcode外派，本轮由Codex内部调整和审计收口。执行[内部计划](CODEX-AUDIT-CLOSURE-PLAN-2026-10-09.md)，旧VF01/VF02外派表暂停；历史交付、ADR0008和正式节点记录保留。源码修复、独立反例审查与主控最终门禁分工明确，不降低Frozen标准。现已形成[收口审计](../reviews/CODEX-AUDIT-CLOSURE-2026-10-09.md)：本轮整改与完整canonical Mac21整体PASS（21/21及前后companion一致），Rust653/0/1、TS36/0，Gate/replay34/0；branch/真实产品链/设备门槛仍未准入，20 DONE / 21 BLOCKED / 60 PLANNED不变。

> 上轮（2026-10-09 AF复验）：[AF独立审计](../reviews/AF-RETURN-AUDIT-2026-10-09.md)接受AF01行为修复/最长匹配及AF02存量概念实验；主控实跑Rust串行653/0/1、默认并行652/1/1，真实watchdog失败与Drop目标就绪缺口仍待补。Gitleaks规则级精确例外已整合，AF01历史HEAD回退作废，当前29版路由加载器回退报VersionMissing(30)。正式20 DONE / 21 BLOCKED / 60 PLANNED不变，仅按[接续两包](MINIMAXCODE-VERIFICATION-FOLLOWUP-2026-10-09.md)整改，未放行下游/ADR0007/V2。

> 历史（2026-10-09 FC审核）：[FC九包独立审计](../reviews/FC-RETURN-AUDIT-2026-10-09.md)确认六对象留存与Run终态局部修复成立；Rust643/0/1 ignored、TS36/0、Gate25/0、replay8/0，主控精确整合FC09后lint0。FC04仍有单层%25脱敏反例，正式状态保持20 DONE / 21 BLOCKED / 60 PLANNED；原M14口径15+1与修订实验16/16分列。仅按[三个接续整改包](MINIMAXCODE-AUDIT-FOLLOWUP-2026-10-09.md)继续，未放行真实应用链/未决通知语义。

> 最新（2026-10-08 Minimax回传审核）：[独立审计](../reviews/MINIMAX-RETURN-AUDIT-2026-10-08.md)判定本批继续整改，新增DEV-02-02/03/04留存阻断，现行 **20 DONE / 21 BLOCKED / 60 PLANNED**。下列旧23/18及503测试是上轮快照。Human可转发[补充任务表](MINIMAXCODE-FOLLOWUP-2026-10-08.md)，不得以十包SELF_CHECK启动依赖。

> 现行快照：2026-10-08。Human 已授权总控制定方案并修复 review 缺陷。对应节点的历史终审保持原样，当前准入以节点表及本轮整改验收为准。

- V2：R0 Rescan 已有报告；R1 架构、Conversation、Gateway、Leader 权限、MCP 与迁移尚未冻结，不启动 V2 新语义。
- 历史底座、迁移、Mock、DEV-02-11/12 纯状态机成果保留；旧审查存在失败、整改和独立 Terra 复核，不因本轮回归抹去这些记录。
- DEV-02-08/09/13/14、DEV-03-06/07 **当前 BLOCKED**：分别涉及最终证据缺失、Notification 未闭环、权限执行点/拒绝审计、真实 Outbox/事务竞争、取消重试运行时、当前世代/状态下的最终写入。领域逻辑修复通过不等于这些节点重新 DONE。
- DEV-02-07 **当前 BLOCKED**：前向 0024 已修正 Outbox 新记录默认重试上限，保留历史配置；4 个专项测试、8 个行为变异通过。当前 Rust 503/0/1 不等于完整 Mac 门禁或应用原子执行验收，见[本批报告](../reviews/OUTBOX-DEFAULT-REMEDIATION-2026-10-08.md)。ADR-0007 与在线审计授权仍待 Human 答复。
- 2026-10-08 深入审计后，新增直接阻断 DEV-01-05/08、DEV-02-05/06/10、DEV-03-03，并按依赖暂停 DEV-02-11/12、DEV-03-02/04/05。全表现为 **23 DONE / 18 BLOCKED / 60 PLANNED**，不是产品完成率。新反例与总控裁定见[深入审计](../reviews/DEEP-AUDIT-2026-10-08.md)，Human可转发[Minimaxcode任务表](MINIMAXCODE-TASKS-2026-10-08.md)开始既有缺陷整改。
- DEV-02-11 已交付 Task/Run/Binding 三机；DEV-02-12 已交付 Message/Delivery/Approval 三机。此前“Run/Binding 未开始、02-12 PLANNED”的动态长段已经过时，详见保留的历史终审。
- 本轮计划及写集：[REMEDIATION-PLAN-2026-10-07](REMEDIATION-PLAN-2026-10-07.md)；上一轮问题来源：[PROJECT-REVIEW-2026-10-07](../reviews/PROJECT-REVIEW-2026-10-07.md)。
- 最终 Mac 门禁仍要求当前输入完整21项、输入前后一致、工具来源与全refs历史秘密扫描。旧 r13 与当前输入不同，不能沿用。Ubuntu/Oracle 不进入本轮验收。

## 派工顺序与权限

1. 开始任一开发任务，按仓库 `AGENTS.md`、`AGENTS.override.md`、四份开发基线、`docs/specs/README.md`、对应冻结规格与已接受 ADR 的顺序阅读。
2. 先核对前置 `DEV-*` 正式 `DONE` 和现行 [阶段计划](../v2/WORKSTREAMS_AND_ACCEPTANCE.md)。R1 方案未获 Human 接受前，只能做只读调研、ADR 草案和已获授权的旧节点收口，不能实现 V2 新语义。
3. Codex 总控为每个外部任务签发 [Markdown 任务包](TASK_PACKET_TEMPLATE.md)，写明实际 Harness/模型、关联 DEV 节点、难度、互斥写集、验收命令、证据、回退与停止条件。外部会话没有自动共享仓库的默认假设。
4. Human 于2026-10-08明确指定 Minimaxcode 接续总控签发的整改及获准后续开发包，路由记录见 [ADR-0008](../adr/0008-minimaxcode-remediation-routing.md)。该10-08历史安排于10-09被Human暂停外派决定覆盖，当前由Codex内部按上述收口计划执行；不追溯历史，执行证据保留实际分工/模型信息。V2 分工仍为候选，不因路由更新开放 R1 以后新语义。
5. Root manifest/lockfile、跨节点整合及最终门禁由 Codex 串行负责；同一文件同一时刻一个写者。Human 保持最终权限。新增费用上限 `$0`；远程账号、连接、上传、容器或可能产生费用的动作先说明影响并取得确认。Mac 正式发布仍须 Gate I 和 Human 单独决定。

历史 Wave 2 执行记录仍保留在 [进度账本](../archive/wave2-execution/GEMINI_WAVE2_PROGRESS.md)、[时间日志](../archive/wave2-execution/WAVE-2-LOG.md)、[长期目标](../archive/wave2-execution/GEMINI_WAVE2_LONG_TERM_GOAL.md) 和[节点审核目录](../reviews/)；其中旧计划/历史测试数字不作为新派工指令或当前输入证据。
