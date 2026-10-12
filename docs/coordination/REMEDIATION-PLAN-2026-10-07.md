# 2026-10-07 总控整改任务包

授权：Human 本轮要求“制定方案审查、修改”。这是对既有 review 缺陷的修复授权；不包含提交、推送、发布、外部服务或 V2 新语义。

依据：PROJECT-REVIEW-2026-10-07 F1～F9、冻结开发基线、PERMISSION_MATRIX §2/§5～7、DATA_CONVENTIONS、STATE_MACHINES、ERROR_CATALOG §11、Adapter V0。

## 本轮执行方案与审核结论

方案审核：按已冻结规则纠正实现与证据，保持原栈、领域状态、错误码和 Adapter wire contract。审批身份、到期校验、必检项、续期世代和 TTL 是已明确的要求，不需要用新 ADR 降低它们。变更内部 helper API，不修改已发布 migration 或生成协议。

| 任务 | 节点 | 写集 | 验收 |
|---|---|---|---|
| 权限默认拒绝、实际审批身份/context、时间感知缓存 | DEV-02-13 | domain/src/permission_intersection.rs、domain/tests/permission_matrix.rs、新 domain/tests/review_regressions.rs | 先复现 F2/F3/F4；验证必检 NA、不同实际主体、非 Human、自批、跨 Team/action/resource/Binding/Run、缓存到期 |
| 租约世代匹配与续期不缩短 | DEV-03-07 | domain/src/run_fencing.rs、domain/tests/run_fencing.rs、regressions | 旧/超前租约、短/零/负 TTL、旧执行者正反例 |
| 自动重试按实际 deadline 停止 | DEV-02-14 | domain/src/outbox_dispatch.rs、domain/tests/outbox_dispatch.rs、regressions | 等于/超过 deadline、投递耗时、队列滞留、预算耗尽；失败保留可处理事实 |
| 中文路径 tooling guard | DEV-01-03 | scripts/verify-tooling-guards.mjs、tests/unit/tooling-guard-path.test.ts | 真实 guard、中文/空格路径、unit/lint/format |
| 节点状态与证据索引校准 | DEV-02-08/09/13/14、DEV-03-06/07 | DEVELOPMENT_NODES、coordination README/WORKLOG、TASK_FREEZE、本计划及整改报告/证据 | 保留历史终审；当前明确 REVIEW/BLOCKED；缺失证据不伪造 |

主控负责所有 R3 修改、根文件串行整合与最终审查；低风险路径任务委派 gpt-5.6-luna（写集互斥）。当前可用 Subagent 工具不提供 gpt-5.6-terra，不升级子模型，R3 由主控第二遍反例审查。受影响节点依赖继续暂停，不因 helper 回归通过恢复 DONE。

## F1 的运行时接线方案

现有 application/security/workspace 是占位，不能只在 supervisor 加一个“相信调用方”的布尔或持有启动快照，就声称数据库最终写入点已安全。本轮先整改纯逻辑和状态事实；完整原节点保留 REVIEW/BLOCKED。后续逐节点满足依赖后，必须按以下顺序实施并验收：

1. DEV-02-13：实际 actor/Team/object/state/path/Binding/Run/scope 查询与 PERMISSION_DENIED Event，不信任 Adapter/缓存提供的 allow。
2. DEV-02-14：应用层事务端口内幂等 claim、版本条件更新、业务变更+Event+Outbox 原子提交；真 SQLite 两连接同 key/版本竞争，dispatcher claim、消费去重和 PROCESSING 崩溃恢复。
3. DEV-03-06/07：应用服务从当前事务读取 Task 最大世代、当前 Run/Binding/lease；对 heartbeat/output/completed 在最终写入点再次授权、校验与条件提交。Supervisor 只提供已验证的协议陈述；拒绝不推进业务 sequence 或写结果。取消确认/强制 fence 必须完成后才释放旧写权，重试创建唯一新 Run。
4. 验收：真实 Mock+SQLite 旧进程晚到、状态 UNKNOWN/终态、两连接竞争、事务前后故障注入、取消拒绝、重复结果与崩溃恢复。单线程改本地变量或纯 helper 不能替代。

尚未定义的业务映射或 heartbeat deadline 归属如有真实冲突，先形成具体 ADR 决策项。不会在本批局部修复里暗中建立假业务链。

## 完成与停止条件

- 先保留旧输入与失败日志，再实现，逐文件格式化，不执行 cargo fmt --all 写入无关文件。
- 定向测试、全 workspace test、clippy、TS lint/typecheck/build、contracts/unsafe/licenses、秘密扫描和输入稳定性检查。
- 完整21项 Mac Gate 含在线审计；未获外部服务确认或未实跑时明确保留 BLOCKED，不沿用旧 r13。
- 旧 DEV-02-08 最终证据缺失、Notification 未闭环、本轮无真实 dispatcher/运行时业务写入均不会被局部代码修复掩盖。
- 历史反例 artifact 保留原 API 与日志，内部 API 修改后以本轮新 regression suite 为复现入口，不回写历史输出。


## 2026-10-08 追加：DEV-02-07 默认值缺陷

冻结 ERROR_CATALOG §11 与现有数据库省略列 INSERT 不一致；本批主控串行修改新前向 0024、migration 注册、Outbox 专项测试、Notification 测试版本 helper 及 domain 默认值注释/测试。旧 migration、manifest/lockfile 保持不变。局部验收 PASS，节点 BLOCKED，未放行 DEV-02-14 依赖，详见[验收报告](../reviews/OUTBOX-DEFAULT-REMEDIATION-2026-10-08.md)。
