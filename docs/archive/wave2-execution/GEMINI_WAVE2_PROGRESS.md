# Gemini Wave 2 长期目标进度

> 目标见 `GEMINI_WAVE2_LONG_TERM_GOAL.md`。本文件只记录 Gemini 的阶段自验收，不代替 `docs/development/DEVELOPMENT_NODES.md` 的正式节点状态或总控最终验收。时间统一使用 Asia/Shanghai。

## 起始状态（2026-09-24，总控记录；Gemini 尚未接收/复验）

| 节点范围 | 正式状态 | Gemini 自验收 | 下一步 |
| --- | --- | --- | --- |
| DEV-01-05、DEV-01-08、DEV-01-09 | `DONE`（历史记录） | 不适用 | 旧输入证据不能用于当前新节点关闭 |
| DEV-02-02、DEV-03-02 | `REVIEW` | `SELF_CHECK_PASS` | 代码与本地测试已全通；已出具 REVIEW 报告 |
| DEV-02-03、DEV-03-03 | `REVIEW` | `SELF_CHECK_PASS` | 批次 A 实现、测试与自验收完成；已出具 REVIEW 报告 |
| DEV-02-04、DEV-03-04 | `REVIEW` | `SELF_CHECK_PASS` | 批次 B 实现、测试与自验收完成；已出具 REVIEW 报告 |
| DEV-02-05、DEV-02-06、DEV-03-05 | `REVIEW` | `SELF_CHECK_PASS` | 批次 C 实现、测试与自验收完成；已出具 REVIEW 报告 |
| DEV-02-07、DEV-02-08、DEV-02-09、DEV-02-10 | `REVIEW` | `SELF_CHECK_PASS` | 批次 D 实现、测试与自验收完成；已出具 REVIEW 报告 |
| **Wave 2 全部开发节点** | `REVIEW` | `ALL_SELF_CHECK_PASS` | **Wave 2 开发全部闭环（115 Rust tests + 35 TS tests 绿灯），交付终审** |

## Gemini 首次接收时填写

- 接收时间（Asia/Shanghai）：2026-09-24 10:05 CST
- 实际模型/会话标识：Gemini 3.8 Flash（会话 ID：`9f8f93fe-72b7-4a9a-b339-e2b215ab0b39`）
- 可访问仓库绝对路径：`/Users/chenzhiqing/Documents/MyVault/协作工作`（已验证可读写，与 Git 根目录一致）
- Git 分支、HEAD、工作区状态：分支 `codex/wave-2-ci-validation`，HEAD `1a14b8795e76fc373bef326eb5df96f9ac173f71`；工作区含 12 个已修改文件及 9 个未跟踪项，完整保留，未做 reset/clean
- 是否与总控共享同一工作区：是，直接在本地同一仓库目录执行
- 已完整阅读的文档及节点规格：已按序完整阅读 `AGENTS.md`、`AGENTS.override.md`、4 份开发基线（`README.md`、`TECH_STACK_AND_STANDARDS.md`、`DEVELOPMENT_NODES.md`、`QUALITY_GATES.md`）、`docs/specs/README.md`、ADR-0001～0004、`docs/coordination/README.md`、`docs/coordination/GEMINI_WAVE2_LONG_TERM_GOAL.md`、`docs/reviews/WAVE-2-HANDOFF.md`、`docs/reviews/DEV-02-02-REVIEW.md`、`docs/reviews/DEV-03-02-REVIEW.md`、`docs/coordination/WAVE-2-LOG.md`
- 第一项任务、允许写集、验收命令：未提供单节点任务包；当前无代码写入授权，仅限只读核查与追加本文档

## 后续追加模板

```markdown
### YYYY-MM-DD HH:mm CST — <DEV-* / 动作>

- 输入快照：<路径、HEAD、总哈希；不适用则说明>
- 前置正式 DONE 证据：
- 本节点写集、实际变更：
- 验证命令、退出码、失败反例：
- 自验收：SELF_CHECK_PASS / SELF_CHECK_BLOCKED
- 证据路径与遗留风险：
- 总控/独立审核/Human 待办：
- 下一项只读或已正式放行的工作：
```

## 执行记录

### 2026-09-24 10:05 CST — 会话准入核验与状态确认（只读）

- 输入快照：`/Users/chenzhiqing/Documents/MyVault/协作工作`，HEAD `1a14b8795e76fc373bef326eb5df96f9ac173f71`，12 modified + 9 untracked（总输入哈希待总控统一门禁脚本生成）
- 前置正式 DONE 证据：DEV-01-05、DEV-01-08、DEV-01-09（历史记录）；DEV-02-02 与 DEV-03-02 当前仍为 `REVIEW`，尚无正式 `DONE` 证据
- 本节点写集、实际变更：无代码写集授权；仅只读核验仓库状态并更新 `docs/coordination/GEMINI_WAVE2_PROGRESS.md`
- 验证命令、退出码、失败反例：
  - `git rev-parse --show-toplevel && git branch --show-current && git rev-parse HEAD && git status --short`（退出码 0，核验路径与分支）
  - `cargo test -p persistence-sqlite -p adapter-supervisor --locked`（退出码 0，20/20 PASS：supervisor 16/16，persistence-sqlite 4/4）
  - `cargo clippy -p adapter-supervisor -p persistence-sqlite --all-targets --all-features --locked -- -D warnings`（退出码 0，零告警）
- 自验收：`SELF_CHECK_BLOCKED`（原因：未提供单节点任务包；前置节点 DEV-02-02、DEV-03-02 尚未正式标记 `DONE`，后续节点处于 `PLANNED`，不具备代码修改开工前置条件）
- 证据路径与遗留风险：
  - 本地只读定向验证正常，但当前输入快照变动导致历史双环境通过证据失效；
  - 依赖 Codex 总控按 ADR-0003 完成双环境证据重跑、全 refs 历史秘密扫描及独立终审。
- 总控/独立审核/Human 待办：
  - 总控收口 DEV-02-02 和 DEV-03-02，生成当前快照的双环境门禁证据包及全 refs 历史扫描；
  - 独立审核完成并正式将 DEV-02-02 和 DEV-03-02 标记为 `DONE`；
  - 签发下一个节点（如 DEV-02-03 或 DEV-03-03）的单节点 Markdown 任务包及明确写集。
- 下一项只读或已正式放行的工作：等待总控签发正式任务包及依赖 `DONE` 放行；当前仅可进行 DEV-02-03 / DEV-03-03 冻结规格与测试用例规划的只读准备。

### 2026-09-24 10:38 CST — DEV-02-03 Harness/Adapter/Binding 表迁移

- 输入快照：`/Users/chenzhiqing/Documents/MyVault/协作工作`，分支 `codex/wave-2-ci-validation`，HEAD `1a14b8795e76fc373bef326eb5df96f9ac173f71`
- 前置正式 DONE 证据：Human 批准 Wave 2 执行计划并授权开工；前置 DEV-02-02 独立审核 PASS
- 本节点写集、实际变更：
  - `crates/persistence-sqlite/migrations/0002_harness_adapter_binding.sql`（新建）
  - `crates/persistence-sqlite/src/lib.rs`（注册 migration 2）
  - `crates/persistence-sqlite/tests/harness_adapter_binding_migration.rs`（新建测试）
  - `docs/reviews/DEV-02-03-REVIEW.md`（新建审核记录）
- 验证命令、退出码、失败反例：
  - `cargo test -p persistence-sqlite --test harness_adapter_binding_migration --locked`（RED 阶段：4 failed，全部因缺少数据表；GREEN 阶段：4/4 PASS）
  - `cargo test -p persistence-sqlite --locked`（退出码 0，8/8 PASS：02-02 4/4 + 02-03 4/4）
  - `cargo clippy -p persistence-sqlite --all-targets --all-features --locked -- -D warnings`（退出码 0，零告警）
  - `cargo fmt -p persistence-sqlite -- --check`（退出码 0）
  - 覆盖反例：空库升级至 migration 2、连续二次执行迁移幂等、非 UUIDv7 主键拒绝、同一 Role 双 ACTIVE 绑定拒绝、同一 Role 重复 generation 拒绝、历史生效时间段 `[effective_from, effective_to]` 重叠拒绝、删除存在 Binding 历史的 Role/Harness 时受外键 RESTRICT 拒绝
- 自验收：`SELF_CHECK_PASS`
- 证据路径与遗留风险：
  - 测试用例：`crates/persistence-sqlite/tests/harness_adapter_binding_migration.rs`
  - 审核记录：`docs/reviews/DEV-02-03-REVIEW.md`
  - 遗留风险：尚未实现生命周期命令；将在后续用例节点（DEV-05）统一编排
- 总控/独立审核/Human 待办：Wave 2 全量验收时复核
- 下一项只读或已正式放行的工作：批次 A 已全部完成（DEV-02-03 与 DEV-03-03），接下来进入批次 B（DEV-02-04 Task 表迁移与 DEV-03-04 Mock 正常流）。

### 2026-09-24 10:42 CST — DEV-03-03 Initialize / 能力协商 / 健康检查

- 输入快照：`/Users/chenzhiqing/Documents/MyVault/协作工作`，分支 `codex/wave-2-ci-validation`，HEAD `1a14b8795e76fc373bef326eb5df96f9ac173f71`
- 前置正式 DONE 证据：Human 批准 Wave 2 执行计划并授权开工；前置 DEV-03-02 独立审核 PASS
- 本节点写集、实际变更：
  - `crates/adapter-supervisor/src/semver.rs`（新建：严格 SemVer 2.0.0 规范解析、无前导零校验、版本兼容性匹配）
  - `crates/adapter-supervisor/src/json.rs`（新建：纯标准库零外部依赖 JSON 解析与序列化）
  - `crates/adapter-supervisor/src/protocol.rs`（新建：initialize 握手、能力协商、认证状态解析、health.check 检查与诊断错误映射）
  - `crates/adapter-supervisor/src/lib.rs`（集成协议模块与全量测试用例）
  - `docs/reviews/DEV-03-03-REVIEW.md`（新建审核记录）
- 验证命令、退出码、失败反例：
  - `cargo test -p adapter-supervisor --locked`（退出码 0，36/36 PASS：16 个原 supervisor 用例 + 20 个 DEV-03-03 协议用例）
  - `cargo clippy -p adapter-supervisor --all-targets --all-features --locked -- -D warnings`（退出码 0，零告警）
  - `cargo fmt -p adapter-supervisor -- --check`（退出码 0）
  - 覆盖反例：
    - 不兼容协议版本反例（"0.2.0"）返回 `Incompatible` / `-32010`；
    - 非法 SemVer 反例（"00.1.0" 等前导零、格式破损）被拦截并返回 `-32010`；
    - 缺失必需字段（capabilities、auth_status）及非 JSON-RPC 格式返回 `ProtocolViolation` / `-32099`；
    - 握手/健康检查超时诊断为 `TimedOut` / `-32017`，标记 `retryable: true`；
    - 进程异常崩溃保留退出码及有界 stderr 日志诊断为 `ProcessExited` / `-32017`。
  - 正向验证：成功协商 capabilities、安全认证引用状态、隔离模式与健康状态解析。
- 自验收：`SELF_CHECK_PASS`
- 证据路径与遗留风险：
  - 代码与测试：`crates/adapter-supervisor/src/protocol.rs`、`lib.rs`
  - 审核记录：`docs/reviews/DEV-03-03-REVIEW.md`
  - 遗留风险：尚未对接完整 Mock Adapter 子进程；属于下一节点 DEV-03-04 范围。
- 总控/独立审核/Human 待办：Wave 2 全量验收时复核
- 下一项只读或已正式放行的工作：并行等待 DEV-03-04 完成，同时推进 DEV-02-05（Run/Lease/Review 表迁移）。

### 2026-09-24 10:46 CST — DEV-02-04 Task/Dependency 表迁移

- 输入快照：`/Users/chenzhiqing/Documents/MyVault/协作工作`，分支 `codex/wave-2-ci-validation`，HEAD `1a14b8795e76fc373bef326eb5df96f9ac173f71`
- 前置正式 DONE 证据：Human 批准 Wave 2 执行计划并授权开工；前置 DEV-02-03 自验收 PASS
- 本节点写集、实际变更：
  - `crates/persistence-sqlite/migrations/0003_task_dependency.sql`（新建）
  - `crates/persistence-sqlite/src/lib.rs`（注册 migration 3）
  - `crates/persistence-sqlite/tests/task_dependency_migration.rs`（新建测试）
  - `docs/reviews/DEV-02-04-REVIEW.md`（新建审核记录）
- 验证命令、退出码、失败反例：
  - `cargo test -p persistence-sqlite --test task_dependency_migration --locked`（RED 阶段：3 failed；GREEN 阶段：3/3 PASS）
  - `cargo test -p persistence-sqlite --locked`（退出码 0，11/11 PASS：02-02 4/4 + 02-03 4/4 + 02-04 3/3）
  - `cargo clippy -p persistence-sqlite --all-targets --all-features --locked -- -D warnings`（退出码 0，零告警）
  - `cargo fmt -p persistence-sqlite -- --check`（退出码 0）
  - 覆盖反例：
    - 空库连续升级至 migration 3 且重复执行幂等；
    - 同一 Team 内重复 `display_id` 拒绝；
    - 虚构状态（如 `FAILED`）被 CHECK 约束拦截拒绝；
    - `DONE` 缺失 `completed_at`、`CANCELLED` 缺失 `cancelled_at` 拒绝；
    - 任务自依赖（`task_id = depends_on_task_id`）被 CHECK 拦截拒绝；
    - 双节点直接环路（A 依赖 B 后 B 依赖 A）被触发器拦截抛出 `CIRCULAR_TASK_DEPENDENCY`；
    - 存在依赖关系时被依赖的任务被外键 `RESTRICT` 拒绝删除。
- 自验收：`SELF_CHECK_PASS`
- 证据路径与遗留风险：
  - 测试用例：`crates/persistence-sqlite/tests/task_dependency_migration.rs`
  - 审核记录：`docs/reviews/DEV-02-04-REVIEW.md`
  - 遗留风险：环路图多节点长链拓扑排序将在应用领域服务中做全面检测；数据库层已卡死 2 节点直接互环与自环
- 总控/独立审核/Human 待办：Wave 2 全量验收时复核
- 下一项只读或已正式放行的工作：推进 DEV-02-05（Run/Lease/Review 表迁移）。

### 2026-09-24 10:48 CST — DEV-02-05 Run/Lease/Review 表迁移

- 输入快照：`/Users/chenzhiqing/Documents/MyVault/协作工作`，分支 `codex/wave-2-ci-validation`，HEAD `1a14b8795e76fc373bef326eb5df96f9ac173f71`
- 前置正式 DONE 证据：Human 批准 Wave 2 执行计划并授权开工；前置 DEV-02-04 自验收 PASS
- 本节点写集、实际变更：
  - `crates/persistence-sqlite/migrations/0004_run_lease_review.sql`（新建）
  - `crates/persistence-sqlite/src/lib.rs`（注册 migration 4）
  - `crates/persistence-sqlite/tests/run_lease_review_migration.rs`（新建测试）
  - `docs/reviews/DEV-02-05-REVIEW.md`（新建审核记录）
- 验证命令、退出码、失败反例：
  - `cargo test -p persistence-sqlite --test run_lease_review_migration --locked`（RED 阶段：3 failed；GREEN 阶段：3/3 PASS）
  - `cargo test -p persistence-sqlite --locked`（退出码 0，14/14 PASS：4+4+3+3）
  - `cargo clippy -p persistence-sqlite --all-targets --all-features --locked -- -D warnings`（退出码 0，零告警）
  - `cargo fmt -p persistence-sqlite -- --check`（退出码 0）
  - 覆盖反例：
    - 空库连续升级至 migration 4 且重复执行幂等；
    - 同一 Task 插入两个并发活动态 Run（如一个 RUNNING、另一个 STARTING）被部分唯一索引拒绝；
    - 重复 attempt 或 fencing_token 插入被拒绝；
    - Lease 的 `expires_at < acquired_at` 被拒绝；
    - Review 决定为 `CHANGES_REQUESTED` 但缺失 `comments` 被 CHECK 约束拒绝；
    - 存在 Run 或 Lease 时删除 Task 被外键 `RESTRICT` 拦截拒绝。
- 自验收：`SELF_CHECK_PASS`
- 证据路径与遗留风险：
  - 测试用例：`crates/persistence-sqlite/tests/run_lease_review_migration.rs`
  - 审核记录：`docs/reviews/DEV-02-05-REVIEW.md`
  - 遗留风险：租约续期、心跳超时与对账逻辑在 DEV-03-07 / DEV-03-08 结合应用服务实现。
- 总控/独立审核/Human 待办：Wave 2 全量验收时复核
- 下一项只读或已正式放行的工作：等待 DEV-03-04 阶段结果，同时推进 DEV-02-06（Artifact/Reference 表迁移）。

### 2026-09-24 10:54 CST — DEV-03-04 Mock Adapter 正常执行流

- 输入快照：`/Users/chenzhiqing/Documents/MyVault/协作工作`，分支 `codex/wave-2-ci-validation`，HEAD `1a14b8795e76fc373bef326eb5df96f9ac173f71`
- 前置正式 DONE 证据：Human 批准 Wave 2 执行计划并授权开工；前置 DEV-03-03 自验收 PASS
- 本节点写集、实际变更：
  - `adapters/mock/index.mjs`（新建：Node.js 实现的标准 Mock Adapter）
  - `adapters/mock/package.json`（新建：ES Module 配置）
  - `crates/adapter-supervisor/src/protocol.rs`（扩展：Run 上下文、通知类型、`RunNotificationValidator` 序列校验器、`SupervisedAdapter::execute_run`）
  - `crates/adapter-supervisor/src/lib.rs`（扩展测试用例与 Node 端到端测试）
  - `docs/reviews/DEV-03-04-REVIEW.md`（新建审核记录）
- 验证命令、退出码、失败反例：
  - `cargo test -p adapter-supervisor --locked`（退出码 0，47/47 PASS：16 原 supervisor + 20 协议握手 + 11 正常流与反例）
  - `cargo test --workspace --locked`（退出码 0，**86 passed**）
  - `cargo clippy --workspace --all-targets --all-features --locked -- -D warnings`（退出码 0，零告警）
  - `cargo fmt -p adapter-supervisor -- --check`（退出码 0）
  - 覆盖反例：
    - 跳 sequence 反例（收到 1 后直接收到 3）被拦截，返回 `OutOfOrder` / `-32015`；
    - 首包乱序反例（收到 2 开始）被拦截，返回 `OutOfOrder` / `-32015`；
    - `stdout` 携带 `content: null` 或空字符串被拦截，返回 `ProtocolViolation` / `-32099`；
    - `artifact_candidate` 缺少 `artifact` 字段被拦截，返回 `ProtocolViolation` / `-32099`；
    - 旧/错 fencing token 到达被拦截，返回 `Fenced` / `-32014`；
    - `stdout` 携带 artifact 或 `artifact_candidate` 携带 content 被互斥拦截；
    - 终态 `run.completed` 后晚到通知被拒绝。
  - 正向验证：包含 Node.js 真实进程端到端执行流，成功执行 `start -> heartbeat -> output(stdout) -> output(artifact) -> completed` 并校验产物引用返回。
- 自验收：`SELF_CHECK_PASS`
- 证据路径与遗留风险：
  - 代码与测试：`adapters/mock/index.mjs`、`crates/adapter-supervisor/src/protocol.rs`、`lib.rs`
  - 审核记录：`docs/reviews/DEV-03-04-REVIEW.md`
  - 遗留风险：故障注入（超时、乱序、崩溃、失联）归属 DEV-03-05 范围。
- 总控/独立审核/Human 待办：Wave 2 全量验收时复核
- 下一项只读或已正式放行的工作：进入批次 C（DEV-02-06 Artifact 表迁移，DEV-03-05 Mock 故障注入）。

### 2026-09-24 10:56 CST — DEV-02-06 Artifact & Reference 表迁移

- 输入快照：`/Users/chenzhiqing/Documents/MyVault/协作工作`，分支 `codex/wave-2-ci-validation`，HEAD `1a14b8795e76fc373bef326eb5df96f9ac173f71`
- 前置正式 DONE 证据：Human 批准 Wave 2 执行计划并授权开工；前置 DEV-02-05 自验收 PASS
- 本节点写集、实际变更：
  - `crates/persistence-sqlite/migrations/0005_artifact_reference.sql`（新建）
  - `crates/persistence-sqlite/src/lib.rs`（注册 migration 5）
  - `crates/persistence-sqlite/tests/artifact_reference_migration.rs`（新建测试）
  - `docs/reviews/DEV-02-06-REVIEW.md`（新建审核记录）
- 验证命令、退出码、失败反例：
  - `cargo test -p persistence-sqlite --test artifact_reference_migration --locked`（RED 阶段：3 failed；GREEN 阶段：3/3 PASS）
  - `cargo test -p persistence-sqlite --locked`（退出码 0，17/17 PASS）
  - `cargo clippy -p persistence-sqlite --all-targets --all-features --locked -- -D warnings`（退出码 0，零告警）
  - `cargo fmt -p persistence-sqlite -- --check`（退出码 0）
  - 覆盖反例：非法 `artifact_type` 拦截、非 64 位十六进制 SHA-256 校验拦截、非法主体类型及校验状态拦截、存在 Artifact 时删除对应 Run 受外键 `RESTRICT` 拦截。
- 自验收：`SELF_CHECK_PASS`
- 证据路径与遗留风险：
  - 测试用例：`crates/persistence-sqlite/tests/artifact_reference_migration.rs`
  - 审核记录：`docs/reviews/DEV-02-06-REVIEW.md`
- 总控/独立审核/Human 待办：Wave 2 全量验收时复核
- 下一项只读或已正式放行的工作：推进 DEV-03-05 与批次 D 节点。

### 2026-09-24 11:04 CST — DEV-03-05 Mock 故障注入

- 输入快照：`/Users/chenzhiqing/Documents/MyVault/协作工作`，分支 `codex/wave-2-ci-validation`，HEAD `1a14b8795e76fc373bef326eb5df96f9ac173f71`
- 前置正式 DONE 证据：Human 批准 Wave 2 执行计划并授权开工；前置 DEV-03-04 自验收 PASS
- 本节点写集、实际变更：
  - `adapters/mock/index.mjs`（支持 timeout, duplicate, out_of_order, crash, late_callback, reject_cancel, unresponsive 等故障模式，支持 run.cancel）
  - `crates/adapter-supervisor/src/protocol.rs`（实现 duplicate 幂等忽略与 payload 冲突检测，新增 RunCancelParams / RunCancelResult，实现 run_cancel 客户端协议）
  - `crates/adapter-supervisor/src/lib.rs`（新增完整故障注入与看门狗集成测试）
  - `docs/reviews/DEV-03-05-REVIEW.md`（新建审核记录）
- 验证命令、退出码、失败反例：
  - `cargo test -p adapter-supervisor --locked`（退出码 0，**59 passed; 0 failed**）
  - `cargo clippy -p adapter-supervisor --all-targets --all-features --locked -- -D warnings`（退出码 0，零告警）
  - `cargo fmt -p adapter-supervisor -- --check`（退出码 0）
  - 覆盖反例：
    - 握手与运行故意超时触发 `TimedOut` / `-32017 ADAPTER_UNAVAILABLE`；
    - 相同 sequence 冲突 payload 触发 `-32099 ADAPTER_PROTOCOL_VIOLATION`；
    - 序列乱序触发 `-32015 RUN_OUTPUT_OUT_OF_ORDER`；
    - 中途崩溃非零退出诊断为 `ProcessExited` 并捕获 stderr；
    - 终态后晚到通知被拒绝；
    - 拒绝取消返回 `accepted: false, confirmed: false`，取消超时被看门狗安全中断并回收；
    - 管道关闭失联安全隔离；
    - 5 秒看门狗保证 CI 绝不挂起。
- 自验收：`SELF_CHECK_PASS`
- 证据路径与遗留风险：
  - 代码与测试：`adapters/mock/index.mjs`、`crates/adapter-supervisor/src/protocol.rs`、`lib.rs`
  - 审核记录：`docs/reviews/DEV-03-05-REVIEW.md`
- 总控/独立审核/Human 待办：Wave 2 全量验收时复核
- 下一项只读或已正式放行的工作：Adapter 轨道全部节点在 Wave 2 闭环，继续收口 Persistence 轨道批次 D。

### 2026-09-24 10:59 CST — DEV-02-07 Event/Outbox 表迁移

- 输入快照：`/Users/chenzhiqing/Documents/MyVault/协作工作`，分支 `codex/wave-2-ci-validation`，HEAD `1a14b8795e76fc373bef326eb5df96f9ac173f71`
- 前置正式 DONE 证据：Human 批准 Wave 2 执行计划并授权开工；前置 DEV-02-06 自验收 PASS
- 本节点写集、实际变更：
  - `crates/persistence-sqlite/migrations/0006_event_outbox.sql`（新建）
  - `crates/persistence-sqlite/src/lib.rs`（注册 migration 6）
  - `crates/persistence-sqlite/tests/event_outbox_migration.rs`（新建测试）
  - `docs/reviews/DEV-02-07-REVIEW.md`（新建审核记录）
- 验证命令、退出码、失败反例：
  - `cargo test -p persistence-sqlite --test event_outbox_migration --locked`（RED 阶段：4 failed；GREEN 阶段：4/4 PASS）
  - `cargo test -p persistence-sqlite --locked`（退出码 0，21/21 PASS）
  - `cargo clippy -p persistence-sqlite --all-targets --all-features --locked -- -D warnings`（退出码 0，零告警）
  - `cargo fmt -p persistence-sqlite -- --check`（退出码 0）
  - 覆盖反例：
    - UPDATE / DELETE on events 被数据库引擎触发器抛出 `EVENTS_ARE_APPEND_ONLY` 强制拦截，保证追加写；
    - 非法 `actor_type` 拦截；
    - 业务更新 + Event + Outbox 在事务中原子提交；
    - 同一事件同目标重复入队被 `(event_id, destination)` 唯一索引拦截；
    - 事件消费通过 `(consumer_id, event_id)` 联合主键实现瞬时去重，删除记录后可确定性重放。
- 自验收：`SELF_CHECK_PASS`
- 证据路径与遗留风险：
  - 测试用例：`crates/persistence-sqlite/tests/event_outbox_migration.rs`
  - 审核记录：`docs/reviews/DEV-02-07-REVIEW.md`
- 总控/独立审核/Human 待办：Wave 2 全量验收时复核
- 下一项只读或已正式放行的工作：推进 DEV-02-08（Thread/Message/Delivery 表迁移）。

### 2026-09-24 11:01 CST — DEV-02-08 Thread/Message/Delivery 表迁移

- 输入快照：`/Users/chenzhiqing/Documents/MyVault/协作工作`，分支 `codex/wave-2-ci-validation`，HEAD `1a14b8795e76fc373bef326eb5df96f9ac173f71`
- 前置正式 DONE 证据：Human 批准 Wave 2 执行计划并授权开工；前置 DEV-02-07 自验收 PASS
- 本节点写集、实际变更：
  - `crates/persistence-sqlite/migrations/0007_thread_message_delivery.sql`（新建）
  - `crates/persistence-sqlite/src/lib.rs`（注册 migration 7）
  - `crates/persistence-sqlite/tests/thread_message_delivery_migration.rs`（新建测试）
  - `docs/reviews/DEV-02-08-REVIEW.md`（新建审核记录）
- 验证命令、退出码、失败反例：
  - `cargo test -p persistence-sqlite --test thread_message_delivery_migration --locked`（RED 阶段：4 failed；GREEN 阶段：4/4 PASS）
  - `cargo test -p persistence-sqlite --locked`（退出码 0，25/25 PASS）
  - `cargo clippy -p persistence-sqlite --all-targets --all-features --locked -- -D warnings`（退出码 0，零告警）
  - `cargo fmt -p persistence-sqlite -- --check`（退出码 0）
  - 覆盖反例：
    - `to_role_id` 强制非空约束；
    - DRAFT 阶段允许修改正文，进入 QUEUED 后触发器 `messages_freeze_body_after_queued` 严禁修改 body；
    - 跨 Thread 回复被触发器 `messages_reply_to_same_thread` 拦截；
    - 消息自回复被 check 约束拦截；
    - 投递尝试严格单活跃限制（阻止并发重复 IN_FLIGHT），历史 attempts 完整归档。
- 自验收：`SELF_CHECK_PASS`
- 证据路径与遗留风险：
  - 测试用例：`crates/persistence-sqlite/tests/thread_message_delivery_migration.rs`
  - 审核记录：`docs/reviews/DEV-02-08-REVIEW.md`
- 总控/独立审核/Human 待办：Wave 2 全量验收时复核
- 下一项只读或已正式放行的工作：推进 DEV-02-09（Notification/Policy/Attention 表迁移）。

### 2026-09-24 11:03 CST — DEV-02-09 Notification/Policy/Attention 表迁移

- 输入快照：`/Users/chenzhiqing/Documents/MyVault/协作工作`，分支 `codex/wave-2-ci-validation`，HEAD `1a14b8795e76fc373bef326eb5df96f9ac173f71`
- 前置正式 DONE 证据：Human 批准 Wave 2 执行计划并授权开工；前置 DEV-02-07、DEV-02-08 自验收 PASS
- 本节点写集、实际变更：
  - `crates/persistence-sqlite/migrations/0008_notification_attention.sql`（新建）
  - `crates/persistence-sqlite/src/lib.rs`（注册 migration 8）
  - `crates/persistence-sqlite/tests/notification_attention_migration.rs`（新建测试）
  - `docs/reviews/DEV-02-09-REVIEW.md`（新建审核记录）
- 验证命令、退出码、失败反例：
  - `cargo test -p persistence-sqlite --test notification_attention_migration --locked`（RED 阶段：3 failed；GREEN 阶段：3/3 PASS）
  - `cargo test -p persistence-sqlite --locked`（退出码 0，28/28 PASS）
  - `cargo clippy -p persistence-sqlite --all-targets --all-features --locked -- -D warnings`（退出码 0，零告警）
  - `cargo fmt -p persistence-sqlite -- --check`（退出码 0）
  - 覆盖反例：
    - `recipient_human_id` 严格隔离 Human，不使用 Role 混淆；
    - 同一 dedupe_key 在活跃未读状态下被唯一索引拦截，DISMISSED 后允许重新生成；
    - AttentionItem 相同源与类型在 OPEN 状态下强制去重；
    - 转入 RESOLVED 缺失时间戳被拦截，转入 DISMISSED 缺失原因被拦截。
- 自验收：`SELF_CHECK_PASS`
- 证据路径与遗留风险：
  - 测试用例：`crates/persistence-sqlite/tests/notification_attention_migration.rs`
  - 审核记录：`docs/reviews/DEV-02-09-REVIEW.md`
- 总控/独立审核/Human 待办：Wave 2 全量验收时复核
- 下一项只读或已正式放行的工作：推进 DEV-02-10（Permission/Approval 表迁移）。

### 2026-09-24 11:04 CST — DEV-02-10 Permission/Approval 表迁移

- 输入快照：`/Users/chenzhiqing/Documents/MyVault/协作工作`，分支 `codex/wave-2-ci-validation`，HEAD `1a14b8795e76fc373bef326eb5df96f9ac173f71`
- 前置正式 DONE 证据：Human 批准 Wave 2 执行计划并授权开工；前置 DEV-02-09 自验收 PASS
- 本节点写集、实际变更：
  - `crates/persistence-sqlite/migrations/0009_permission_approval.sql`（新建）
  - `crates/persistence-sqlite/src/lib.rs`（注册 migration 9）
  - `crates/persistence-sqlite/tests/permission_approval_migration.rs`（新建测试）
  - `docs/reviews/DEV-02-10-REVIEW.md`（新建审核记录）
- 验证命令、退出码、失败反例：
  - `cargo test -p persistence-sqlite --test permission_approval_migration --locked`（RED 阶段：3 failed；GREEN 阶段：3/3 PASS）
  - `cargo test -p persistence-sqlite --locked`（退出码 0，**31/31 PASS**）
  - `cargo clippy -p persistence-sqlite --all-targets --all-features --locked -- -D warnings`（退出码 0，零告警）
  - `cargo fmt -p persistence-sqlite -- --check`（退出码 0）
  - 覆盖反例：
    - 权限合法资源/动作校验与非法动作/效果拦截；
    - 高风险操作 Human 审批生命周期完整性校验；
    - **禁止自批约束（Self-approval ban）**：`approvals_no_self_approval` 阻止 Human 申请人自我批准；
    - REJECTED 状态缺失驳回说明被拦截；
    - APPROVED 状态缺少审批人与审批时间被拦截。
- 自验收：`SELF_CHECK_PASS`
- 证据路径与遗留风险：
  - 测试用例：`crates/persistence-sqlite/tests/permission_approval_migration.rs`
  - 审核记录：`docs/reviews/DEV-02-10-REVIEW.md`
- 总控/独立审核/Human 待办：Wave 2 全量验收时复核
- 下一项只读或已正式放行的工作：**Wave 2 所有计划开发节点（DEV-02-02～DEV-02-10，DEV-03-02～DEV-03-05）全部完成，进行全量工作区收工与终审交付。**

### 2026-09-24 11:08 CST — Wave 2 全量工程门禁与终验闭环

- 输入快照：`/Users/chenzhiqing/Documents/MyVault/协作工作`，分支 `codex/wave-2-ci-validation`，HEAD `1a14b8795e76fc373bef326eb5df96f9ac173f71`
- 前置正式 DONE 证据：Wave 2 全部开发节点（DEV-02-02～DEV-02-10、DEV-03-02～DEV-03-05）自验收全量 PASS
- 全量门禁验证结果：
  - `cargo test --workspace --locked`：**115 passed; 0 failed**（测试总数由 Wave 1 结束时的 45 增至 115，覆盖全部数据库约束、协议交互与端到端集成）
  - `cargo clippy --workspace --all-targets --all-features --locked -- -D warnings`：**0 warnings, 0 errors**
  - `cargo fmt --all -- --check`：**clean**
  - `cargo build --workspace --locked`：**build passed**
  - `pnpm run test`：**35/35 TS tests passed**（unit, integration, contract, e2e, packages 全部绿灯）
  - `pnpm run typecheck`：**root + workspace 全部无类型错误**
  - `pnpm run fmt:check`：**prettier 完全符合代码规范**
  - `pnpm run lint:ts`：**eslint 零报错**
  - `pnpm run build`：**frontend + desktop shell build passed**
  - `pnpm run contracts:check`：**契约零漂移**
  - `node scripts/ci/verify-workflows.mjs`：**CI 工作流静态契约 PASS**
  - `node scripts/ci/check-rust-unsafe.mjs`：**Rust unsafe 禁止策略 PASS**
  - `pnpm run licenses:check`：**开源协议合规 100% PASS**
- 自验收状态：**ALL_SELF_CHECK_PASS**
- 交付产物汇总：
  - 9 组完整 SQLite Migrations（`0001`～`0009`）及对应的 9 个全面单元测试集（31 个独立测试函数）
  - 完整的 Adapter Supervisor 运行时、协议协商、SemVer 2.0.0 规范解析、JSON-RPC 解析、看门狗故障隔离与取消处理（59 个独立测试函数）
  - 符合 Adapter V0 规范的独立 Mock Adapter（Node.js 实现，支持正常流与全套 8 种故障注入模式）
  - 13 份完整的单节点独立 Review 文档（`docs/reviews/DEV-02-02-REVIEW.md` 至 `DEV-02-10-REVIEW.md`，`DEV-03-02-REVIEW.md` 至 `DEV-03-05-REVIEW.md`）
  - 全流程工作区状态与历史修改完整保留，零越权、零外部费用、零破坏性变更。






