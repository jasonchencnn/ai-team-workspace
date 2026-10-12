# 持续整改目标（2026-10-07）

Human `/goal`：依据整改验收报告修改项目，直到 MiniMax 交付范围内没有未关闭错误。当前为 **ACTIVE**，尚未达成。

## 本阶段范围与写集

先修复已存在节点，不启动依赖未通过的新产品节点：

1. DEV-02-08：当前Schema/消息生命周期定向复核；补可验证的新证据，不伪造缺失的历史JSON。
2. DEV-02-09：主控R3修复，新增前向migration及注册、专项真库测试；Recipient不得与Role身份混淆，Attention必须验证实际源状态，policy的既有filters/escalation要能持久化表达。保持旧migration字节不变。
3. DEV-02-13：前置修复验收后，构建可信权限执行与拒绝审计端口/SQLite实现。
4. DEV-02-14：真实事务、幂等/版本竞争、Outbox dispatcher/去重/崩溃恢复。
5. DEV-03-06/07：真实Mock与SQLite联动的取消、新Run重试、当前世代最终写入校验。
6. 当前输入最终Mac完整21项、全refs秘密扫描与证据收口。此前外部服务授权问题仍待Human答复，不把本次goal自动解释为对在线服务的确认。

本阶段主控串行修改 `crates/persistence-sqlite/migrations/0023_*`、注册文件与专项测试；后续应用端口/基础设施写集按节点追加。根manifest/lockfile由主控串行整合。保持所有旧成果与未提交内容，不reset、提交、推送或发布。

## DEV-02-09 实现边界

- Task BLOCKED、Run FAILED/TIMED_OUT/UNKNOWN、Message NEEDS_HUMAN/FAILED、Approval PENDING仍需处理，不能只改Attention时间戳伪装解决。
- Task解除BLOCKED后可关闭对应事项；这是STATE_MACHINES Task resolve_block的已有副作用，不要求Task必须DONE。
- Recipient/Role碰撞在INSERT及逆向Role创建/修改都拒绝；完整自然人身份仍必须由可信安装Owner执行点校验，不能从一个格式正确UUID推断身份。
- Policy options属于既有NotificationPolicy聚合，独立存储避免改写旧表列布局和历史migration；options修改必须跟随主policy版本。
- SYSTEM来源和升级规则中冻结文档未定义的部分继续默认拒绝未证明的自动解决，不发明产品状态或新权限。

每轮先构造有效种子和允许路径，再验证违规拒绝的具体来源；失败零副作用、升级回滚、正反对照均须保留。本文件只登记执行计划，不宣布任何节点DONE。


## 2026-10-08 进展

DEV-02-09 数据约束候选 0023 已完成局部复核；新发现并修复同 Team 来源改写与 Notification 事实删除。14 个真库用例、15 个行为变异和当前 Rust 全工作区 494/0/1 完成，详见 [本轮验收](../reviews/GOAL-REMEDIATION-2026-10-08.md)。旧迁移全保持。

初稿两项推导已撤回：不能按当前受阻状态否定历史解决；不能把任意更新 attempt 当作真实 retry 解决凭据。Policy options 仅表达存储，尚不能证明强制通知必达。

[ADR-0007](../adr/0007-notification-resolution-and-policy-semantics-proposal.md) 已形成可审阅方案并提交 Human，状态 Proposed。当前目标 ACTIVE，节点 BLOCKED；没有启动依赖未通过的新产品节点。后续执行链仍按依赖整改，既有外部服务授权问题仍待答复。


DEV-03-07 的冻结前置不含 Notification，故先修复其独立规则：零世代、终态和 UNKNOWN 拒绝已验证，当前 Rust 499/0/1，严格 rustdoc 断链已关闭；[本轮报告](../reviews/RUN-CALLBACK-ADMISSION-REVIEW-2026-10-08.md)。这不启动依赖未通过的新产品节点，也不代替应用权限/事务的正式准入。当前仍需 ADR-0007 确认和完整门禁，目标保持 ACTIVE。


## 2026-10-08 Outbox 默认值整改

关联 DEV-02-07：前向 0024 将新 Outbox 的默认重试上限修正为冻结的 3；保留全部历史值和 3 个命名索引/4 个守卫。4 个专项测试、8/8 行为变异通过，Rust 全工作区 503/0/1，build/clippy/fmt/严格 rustdoc/Gitleaks 通过；现有 23 份 migration 与 Rust manifest/lockfile 不变。详见[本批验收](../reviews/OUTBOX-DEFAULT-REMEDIATION-2026-10-08.md)。

DEV-02-07 当前 BLOCKED；其余六个整改节点保持 BLOCKED。本轮不把省略字段缺陷修复当作 dispatcher 或原子应用链完成，不启动依赖未通过的新节点。后续仍需 ADR-0007 决策、实际执行链和当前输入完整门禁；此前外部服务授权问题未答复。


## 2026-10-08 新目标：深入审计与外部接续

Human本轮明确要求先完成全面深入测试/审验，再提供由Minimaxcode执行的开发任务表。按[深入审计计划](DEEP-AUDIT-PLAN-2026-10-08.md)完成本轮审计，结论FAIL且发现新的已复现缺陷；没有继续直接改业务实现。后续由Human转发[Minimaxcode任务表](MINIMAXCODE-TASKS-2026-10-08.md)，Codex保留独立验收。ADR0008记录最新模型路由，不等于接受ADR0007或外部服务权限。

目标工具因旧paused目标尚未完成而拒绝创建新目标；没有为修改目标名称虚报旧整改目标complete。本轮用户新任务及进度由本计划和交接表明确记录。原整改目标仍未达成，当前节点23 DONE/18 BLOCKED/60 PLANNED。
