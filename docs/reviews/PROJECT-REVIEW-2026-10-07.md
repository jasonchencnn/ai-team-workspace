# 项目接管审查与 MiniMaxcode 工作盘点（2026-10-07）

> 结论：工程底座可构建，持久化和领域层有大量可保留成果；当前仍不满足受影响节点的正式完成条件，不能放行产品阶段或发布。
> 本报告不改写历史审核、冻结规格或节点状态。下述 `REVIEW/BLOCKED` 是本次复核建议，须经节点整改与重新验收落地。
> 审查输入：HEAD `1a14b8795e76fc373bef326eb5df96f9ac173f71` 加当前工作树。
> 复跑前输入：313 个文件，SHA-256 `b3dcb135b733f18810176c769e88ce775d28ce32ab5a6f1c9ce5e386b48b67fb`。检查前后摘要一致。本报告及本轮证据是检查结束后新增，不在该输入内。
> 审查方式：全目录、节点、变更和交接证据扫描；重点复核 R3 权限、租约、幂等、恢复及验收边界。不是每一行代码均无缺陷的保证。

## 1. 主要判断

1. MiniMax 接替期间有实际工程产出：10 个节点工作包、7 个前向迁移、13 个新增领域模块、状态机与真 SQLite 回归测试，以及大量反例审查和返工。
2. 主要偏离在于**完成定义缩窄**：部分节点要求运行时闭环，交付却停在纯判定函数、数据库约束或模拟竞争，随后标为 `DONE`。这类成果可复用，但尚未证明要求的运行时效果。
3. 当前已有测试整体通过，独立新反例仍发现权限、审批、缓存、续期和 TTL 缺口。测试数量不能证明这些不变量已经成立。
4. 工程与产品进度要分开：应用服务、安全存储、Workspace 操作、真实 Codex Adapter、业务 IPC 和业务 UI 尚未接通；AC-01～12 没有完整端到端验收。V2 R1 仍未冻结。
5. **不认定 MiniMax 使用身份本身违规**。终审记录了 Human 允许 MiniMax 暂代的明确例外；用户本轮也确认了接替事实。该授权不等于免除节点依赖、冻结语义或验收要求。

## 2. 当前项目进度

冻结栈仍为 Tauri 2 / Rust / Tokio / SQLx SQLite / React 19 / TypeScript / Vite 7 / pnpm 10，未发现本轮接管期间改换框架或数据库的证据。

| 范围 | 当前实际成果 | 未完成 / 本轮判断 |
|---|---|---|
| DEV-00 决策与规格 | 状态、权限、协议、错误目录、数据约定及追踪已存在 | 历史冻结可继续作为事实依据；V2 不能直接覆盖它们 |
| DEV-01 工程底座 | workspace、工具链、CI、本机门禁脚本、契约生成、脱敏框架、桌面壳 | 构建可用；当前完整 21 项门禁尚未重新成立，`verify:guards` 有中文路径缺陷 |
| DEV-02 领域 / 持久化 | 22 个迁移、27 个业务表、当前 184 个触发器；六类状态机、权限矩阵、幂等、Outbox 决策 | 02-13/14 有下文阻断项；02-15 备份、升级、恢复未实现；原子业务命令和真实 dispatcher 未接通 |
| DEV-03 Adapter | stdio supervisor、握手/健康、真实 Node Mock 正常流与故障注入；取消/重试/fencing 纯判定 | 03-06/07 不具备完整运行时闭环；03-08 恢复、03-09 Keychain、03-10 真实 Codex、03-11 Go/No-Go 未实现 |
| DEV-04 Workspace | crate 骨架 | `crates/workspace/src/lib.rs` 只有 `forbid(unsafe_code)`；路径、引用、worktree、单写者、写入 fencing 未实现 |
| DEV-05 应用 / IPC | ErrorEnvelope 与生成机制 | `crates/application/src/lib.rs` 是占位；没有 Task/Run/审批/消息应用事务或页面 read model |
| DEV-06 前端 | Token、React/Tauri 应用壳 | `App.tsx:1` 仍显示“控制面和业务界面将在后续节点接入”；导航、表单、业务页、A11y 闭环未实现 |
| DEV-07 纵向闭环 | 尚无业务闭环入口 | AC-01～12 均不能判定端到端通过 |
| DEV-08 系统 / 发布 | 规则和规划 | 性能、备份恢复、真实桌面、安装、最低 macOS 版本、签名和候选版验收未完成 |
| V2 | R0 Rescan 和分工草案 | Conversation、ProviderSession、Gateway、ACP、MCP、Leader 新权限尚未冻结或实现 |

节点明细共 **101 个**；当前状态表展开后 **41 个宣称 `DONE`**（DEV-00 10、DEV-01 9、DEV-02 14、DEV-03 7、DEV-06 1）。这是文档状态统计，包含历史/条件性关闭，不是产品完成率，也不是本次认可 41 个全部通过。

Git 变更必须展开目录统计：本轮报告加入前 **192 个变更文件 = 19 modified + 3 deleted + 170 untracked**。默认 `git status` 的 84 个 `??` 条目包含折叠目录，不能当作 84 个文件。HEAD 最后提交日期为 2026-09-23。

迁移统计口径：SQL 有 34 次 `CREATE TABLE`，7 个升级 guard 随后被删除，留下 27 个业务表；185 次 `CREATE TRIGGER` 中包含一次重建，同库实际 184 个。`domain/tests` 14 个 `.rs`，`persistence-sqlite/tests` 23 个 `.rs`。

三个显示 deleted 的历史文档均在 `docs/archive/v1-planning/` 有与 HEAD 原件字节完全一致的副本。`design/figma-state.json` 的差异只有审计来源路径更新；本轮没有将这些变更归为 MiniMax 破坏性删除或视觉改造。

## 3. MiniMaxcode 的工作量与进度

归属依据是节点终审的执行者/授权记录和相关 MiniMax 任务证据，不是 Git 作者：9 月 23 日后没有可归属的新提交。不能把所有 192 个变更文件都算给 MiniMax；其中有此前 Codex/Gemini 的实现、资料归档和文档整理。

| 节点工作包 | 可确认成果 | 当前进度与复核意见 |
|---|---|---|
| DEV-02-07 Event / Outbox | `0016`；补 UPDATE 去重不可变性与执行 Binding 状态；新增 6 个迁移测试 | 存储整改有成果；“原子业务提交 / 重放消费”尚需应用层验证，历史声明不能放行当前输入 |
| DEV-02-08 Thread / Message / Delivery | `0017` 18 trigger + `0018` 6 trigger；首次整改后又补创建轴；记录新增 10+6 测试 | 初版终审已作废；有效重签引用的最终 JSON 当前缺失，需补证据 |
| DEV-02-09 Notification / Attention | `0019` 21 trigger；跨 Team、生命周期、不可变性与创建限制；记录新增 9 测试 | Human 身份落库、Policy filters/escalation、Attention 底层终态等仍有登记缺口；不能把已登记等同已实现 |
| DEV-02-10 Permission / Approval | `0020` 16 trigger + `0021` 9 trigger；决定人冻结、策略关联、取消 actor 等 | 数据约束可保留；尚无真实审批服务；领域 grant 另有 F3 缺陷 |
| DEV-02-11 Task / Run / Binding 状态机 | 3 个领域模块及测试；记录新增 56 个用例；修复事件/错误码序列化及迁移歧义 | 纯状态机可保留；不是 Task/Run 应用服务完成 |
| DEV-02-12 Message / Attempt / Approval 状态机 | 3 个模块及测试；记录新增 44 个用例；Q-06/Q-07 规格比对等 | 纯状态机可保留；不代表消息实际投递或审批执行闭环 |
| DEV-02-13 权限 | 36×6 矩阵、八层求交、grant/cache 辅助、example；记录新增 24 用例 | **建议 REVIEW**：F2/F3/F4，且执行点权限与拒绝 Event 未接通 |
| DEV-02-14 并发 / 幂等 / Outbox | `0022` 6 trigger；幂等/版本纯判定、重试预算；记录新增 50 用例 | **建议 REVIEW**：F1/F6；未实现真实 dispatcher / 竞争事务 / 崩溃恢复 |
| DEV-03-06 取消 / 超时 / 重试 | `run_termination`、`run_retry`、example；记录新增 34 用例 | **建议 REVIEW**：纯判定尚未接 supervisor、事务、新 Run 创建和取消确认 |
| DEV-03-07 心跳 / 续期 / fencing | `run_fencing`、领域和真 SQLite 约束测试；多轮反例返工 | **建议 REVIEW**：F1/F5；新判定没有接入回调运行时，最新门禁输入也有漂移 |

规模实测：以上领域新增模块 **13 个 / 7,367 行**，新增领域测试文件 12 个 / 8,134 行；`0016`～`0022` **7 个迁移 / 1,837 行 / 79 次 trigger 声明**。行数包含注释与空行，是文件规模而非生产代码净增；未包括外部审查装置、迁移测试和其他文档。

终审记录的工作区测试从 DEV-02-07 开始前的 184 增到当前实测 471，净差 287。这里只核验最终 471；起点和各阶段增量是历史记录，不能据此计算工时、需求覆盖率或把全部测试归为某一次实现。

值得保留的做法：优先前向迁移；区分事实与派生判断；保存作废证据；SQLite INSERT/UPDATE/DELETE/REPLACE 多轴反例；将部分 SQL 文本匹配改为真库行为验证；用变异验证测试的有效性。MiniMax 对多次错误与返工有主动记录。

效率问题也有证据：DEV-03-07 终审已有第 11 次审查章节、外部台账经过 r13；多轮返工集中在计数器、错误命题、文本匹配、变异台账和证据自校验。下一阶段应把审查预算集中到可信执行点的真实场景，避免继续只加“验证验证器”的装置而不接通产品路径。没有可靠工时或 token 数据，本报告不虚构投入小时数或费用。

## 4. Review findings（按优先级）

P1 表示应在相关节点关闭或后续集成前修复；P2 表示工程或证据修复项。权限/续期代码尚未被产品入口调用，因此本报告描述的是**可复现的领域缺陷和验收阻断**，没有宣称已经发生生产越权或双主写入。

### F1 [P1] 纯判定被升级成运行时完成，关键 fencing 缺口仍在原位置

- 文件：`crates/adapter-supervisor/src/protocol.rs:1223`；`crates/application/src/lib.rs:1`；`crates/domain/tests/idempotency.rs:458`（模拟两次版本判断）。
- `RunNotificationValidator` 比较通知 token 与 `expected_context` 的启动快照。新 `decide_write`/`decide_heartbeat` 没有来自 supervisor/application 的生产调用点，也没有读当前 DB 世代的事务边界。t0 token=7 启动、t1 控制面换为8、t2 旧执行者上报7，这个验证器仍将7与7比较。
- DEV-03-07 终审自己在 §3 描述了这个原缺口，但交付没有接线修复它。真库约束保证记录关系，不证明旧进程写入已被拒绝。
- DEV-02-14 的“竞争仅一个成功”测试顺序修改本地 `stored` 变量，不是真 SQLite 竞争；Outbox 模块没有 IO dispatcher、崩溃后 PROCESSING 恢复或真实消费者事务。DEV-03-06 同样只有取消/超时判定，没有实际取消确认、新 Run 创建与审计。
- 修复方向：保持领域纯函数；补端口/应用事务和 supervisor 的调用，在每次最终写入点读取当前 Binding/Run/世代，验证两连接竞争、旧回调晚到、取消拒绝、重启恢复和失败零副作用。若只交付纯判定，拆分子交付并明确原节点仍 REVIEW。

### F2 [P1] 八层定长数组不能保证必检层已检查，全部 NotApplicable 会放行

- 文件：`crates/domain/src/permission_intersection.rs:518`、`:524`。
- `NotApplicable` 与 `Allow` 同样被跳过；对静态矩阵允许的动作，即使八项全为 NotApplicable，最终仍返回 Allow。本轮直接调用 Team 创建/编辑、Human Owner、八项全 NA，实测 `Allow`。
- TeamPolicy / ActorBasePermission 对该动作是必需检查。定长数组仅保证八个槽存在，不能保证身份、Team 上限或适用条件真的被验证；模块头声称“忘检在类型上不可表达”过强。
- 修复方向：区分必检项、条件适用项与未验证项。必检项不能为 NA；适用性由可信上下文导出，缺失/未验证默认拒绝。补全部 NA、TeamPolicy NA、假路径 NA 等反例。

### F3 [P1] Approval grant 接受非 Human 决定者，且无法表达真正的 actor / scope 匹配

- 文件：`crates/domain/src/permission_intersection.rs:537`、`:581`、`:595`。
- `approval_grant_is_usable` 检查消费、expiry、自批及分类相等，但没有 `decided_by.is_human()` 门禁。构造 Developer 请求、Leader 决定、binding_version=7，Developer 使用，实测返回 None（可用）。
- requested_by/decided_by/use_actor 只有 ActorKind 分类，不是 HumanId/RoleId；也没有 Team、BindingId、action/resource/scope、Run context。两个不同 Developer、两个同版本 Binding 在该 API 中不能被区分。
- 冻结要求 Human-only 决定与每次实际身份、scope、上下文校验。DB 的 Human 字段形状和调用方布尔声明不能补足这个 API 声称的“可用”结果。
- 修复方向：判定输入使用实际主体和 grant scope/context；至少拒绝非 Human decider，并提供相同类别但不同 ID、不同 Binding 同 version、换 action/resource 的负向测试。若仅返回局部分类结果，应改契约与命名，不能充当执行授权。

### F4 [P1] 缓存只比较 expiry 数值，没有检查已经越过该时刻

- 文件：`crates/domain/src/permission_intersection.rs:609`、`:637`。
- `CacheContext::may_reuse` 没有当前时间参数；expiry=100 的 context 在 now=101 时与缓存完全相同，实测仍返回 true。真实到期不会自动修改 expiry 字段。
- 现有测试只改变 expiry 字段值，漏掉“时间前进、对象未修改”。方法注释却承诺已到期 grant 不被复用。
- 修复方向：接受注入时钟/当前时刻，或将新鲜度与上下文等同性分为两个都必须成立的判定；补 expiry 前、等于、之后的时间测试，并保证执行点仍重新验证 grant。

### F5 [P1] 续期没有匹配租约自己的 token；还可能缩短当前有效租约

- 文件：`crates/domain/src/run_fencing.rs:304`、`:312`、`:334`。
- `LeaseSnapshot.fencing_token` 在续期判定中未使用。lease.token=7、current=8、presented=8、租约尚未过期，实测返回 `Renewed`。判定只证明上报者是当前世代，没证明它续的是当前世代的租约。
- 同 token 的 expiry=1000、now=100、ttl=100，实测续到200，把现有窗口缩短。零/负 TTL 被夹成0，也得到 Renewed，而不是“延长”。缩短的边界为 P2 次级问题，须明确合法续期策略。
- 修复方向：匹配租约、Run 与当前世代，结合当前 Run 状态；拒绝跨世代租约；合法续期需保证 deadline 单调或明确受控缩短命令。补旧租约+新 token、终态 Run、短 TTL 的真实持久化反例。

### F6 [P1] 重试预算已耗尽仍会返回 Retry，不能保证 TTL 上限

- 文件：`crates/domain/src/outbox_dispatch.rs:298`、`:440`。
- 预算100ms、已用101ms、now=101，`decide_outcome` 实测仍返回 Retry，scheduled_at=101。`next_delay_millis` 把等待夹成0，不停止重试；这只限制等待和，不限制命令/Message 的总生命周期。
- 延迟消费、投递耗时、重启期间时间也未进入该判定。已有“总延迟不超过预算”测试不能证明“总时长不得超过 TTL”。
- 修复方向：对冻结的总时长/TTL 使用实际 deadline，在调度和执行时检查；到期后明确终止自动重试并保留可处理事实。到期不应被转换为立即再试。
- 另一个实验把 `AUTH_PERMISSION_DENIED` 包装为 `DeliveryFailure::Retryable` 后仍获得 Retry。这项责任已在现有注释明确交给调用方，本报告把它列为 F1 的接线验收责任，而不单列为已发生的安全缺陷。

### F7 [P1] 正式关闭证据不覆盖当前输入，DEV-02-08 重签最终证据缺失

- 最新 `wave2-macos-arm64-dev0307-fencing-r13.json` 实际有 21 条命令且全部 exit=0；不是“只跑 Rust”。其摘要为 `ddf55717...f47`。
- 对当前313个输入逐项比较，仅3个文件变了：`crates/persistence-sqlite/tests/run_lease_consistency_forward_migration.rs`、`docs/development/DEVELOPMENT_NODES.md`、DEV-03-07 终审。当前摘要为 `b3dcb135...fb`；按 ADR-0005 的失效规则，r13 不能证明当前最终输入。
- DEV-02-08 重签引用的 `wave2-macos-arm64-dev0208-resign-final.json` 不存在。旧 closure-final 只剩标记 SUPERSEDED 的版本，不能当生效证据。
- 09-29 后大部分装置、补丁、台账和门禁 JSON 只在 `~/.minimax/teamspace-audits/`，未进入项目交付范围。本轮只核验相关路径/摘要/命令与选择规则，没有完整重跑所有外部变异矩阵。
- 修复方向：固定真实输入和生效证据索引，保留可重放装置及来源证明，补缺失引用；受影响节点修复后再取得最终21项、全refs扫描和总控复核。测试全绿不能替代该过程。

### F8 [P2，旧缺陷] tooling guard 使用 URL.pathname，中文路径下验证装置失败

- 文件：`scripts/verify-tooling-guards.mjs:34`。
- `writeFileSync(URL)` 正确创建中文目录文件，但 ESLint 收到 percent-encoded `.pathname`；实测“No files matching ...%E5%8D%8F...”并 exit=1，没测到预期的 no-explicit-any 规则。
- 与 HEAD 比较该文件未变，不能归为 MiniMax 新引入。
- 修复方向：向命令传 `fileURLToPath(invalidTypeScriptUrl)`；覆盖中文、空格路径，同时区分装置失败与违规被拦截。

### F9 [P2] 状态和工作日志存在漂移，不能作为实时单一快照

- `docs/coordination/README.md:12` 的长段仍说 DEV-02-11 仅 Task 完成、Run/Binding 未开始，而节点表已说三机 DONE。
- WORKLOG 的454/458是已标注过时的快照；本轮实测471。其“DEV-02-11→03-07 8个节点”口径不成立，这个范围实际4+2=6；“84个未跟踪文件”统计的是折叠条目。
- WORKLOG “Wave 2 至今没有真正跨模型独立复核”范围过宽：09-28 前置和三批终审都有 Terra 独立复核。应限定为后续 MiniMax 节点的同模型降级，不能抹去既有审核。
- 修复方向：由总控串行更新当前状态索引，将历史快照、后续追加和当前事实分开，不改写原审查发生过的事实。

## 5. 已登记但仍需裁决或完成的事项

这些不都属于新发现，也不一律需要改冻结规格；应先区分实现缺失与真实语义冲突。

| 事项 | 建议处理 |
|---|---|
| Notification 接收者的 Human 身份与 Role 的 DB 区分 | 这是冻结的不变量，实现必须闭环；需要新增/替代语义时才走 ADR |
| NotificationPolicy filters/escalation、Attention RESOLVED 底层事项验证、TASK/SYSTEM source_type | 将具体缺口关联节点与验收，不能笼统登记后宣称 DONE |
| heartbeat deadline 是 lease 派生还是独立列 | 先明确 deadline 的语义与恢复证据；两者可能不同，不能因缺列自动认定规格未定义 |
| 权限矩阵 H 与 Human-only 列表覆盖面、36动作与DB动作/资源映射 | 逐项映射并登记有实际语义冲突的地方，保持默认拒绝 |
| `0006` Outbox 默认重试5、冻结默认3 | 新应用服务必须显式使用冻结默认；需要修 schema 时用前向迁移，不能修改旧 migration，也不能靠 assert_ne 当修复 |
| 终态/UNKNOWN Run 的 write admission | 必须在真实回调执行点校验；不能只靠 LEASE_BEARING_STATUSES 常量或留给尚未完成的恢复节点 |
| 未提交文件与仓库外证据 | 先形成可恢复的节点交付快照；提交、推送继续由 Human 明确授权，本轮没有执行 |

## 6. 本轮验证结果与边界

环境：macOS 27.0 / ARM64；Node 24.21.0、pnpm10.33.0、Rust/Cargo1.98.0。既有 MiniMax 工具目录内 gitleaks8.30.0、cargo-audit0.22.0、cargo-deny0.18.6 的实际二进制 SHA-256 与已有 provenance 记录一致，没有安装或下载工具。

| 检查 | 本轮结果 |
|---|---|
| `cargo test --workspace --locked --offline` | **471 passed / 0 failed / 1 ignored**（按58份测试结果汇总） |
| `pnpm run test:all` | **37 passed**（4 unit + 1 integration + 21 contract + 1 e2e + 4 desktop + 4 tokens） |
| `node --test scripts/ci/local-gates.test.mjs` | **12 passed** |
| `pnpm run fmt:check`、lint:ts、typecheck | PASS |
| `cargo build`、clippy（workspace/locked/offline，clippy all-targets/all-features/-D warnings） | PASS |
| `pnpm run build`、contracts:check、unsafe:check、licenses:check | PASS |
| check:toolchain、verify-workflows、git diff --check | PASS |
| `pnpm run verify:guards` | **FAIL**，见F8；临时违规文件已清理 |
| Gitleaks 工作树、全refs历史扫描（redact） | PASS，0命中；非shallow仓库，历史日志记录4 commits |
| `cargo-audit audit --no-fetch` | exit0；缓存公告库 `ef6173cb`（2026-10-03）；7条 allowed unmaintained warnings，crates.io index因缓存锁权限不可读取 |
| `cargo-deny --offline ... licenses` | exit0，licenses ok；3条 unused license allowance warnings |
| 新领域反例装置 | 7项观察输出，见F2～F6及F1分类责任；直接调用public API，未改业务代码 |
| 完整21项 / 干净安装 / 新鲜Node与Rust在线漏洞审计 | **未执行**；现有依赖下复跑，且本轮不连接外部审计服务。不能声明当前最终Mac Gate PASS |
| 真桌面E2E、真实Harness、Keychain、备份恢复、性能、最低支持macOS、A11y | **未验收**；相关业务入口未完成 |

TS 的 integration/e2e 命令目前各只有一个工具链占位断言，例如 `expect("e2e").toBe("e2e")`；37绿不等于产品E2E通过。Rust状态机/SQLite/Mock测试是真实成果，也不能外推为完整AC验收。覆盖率未测量，不宣称达到领域90%/分支85%。

本轮检查前后源输入一致。所有业务源码、规格、节点表、manifest/lockfile保持原样；只新增本报告和本轮证据，没有提交、推送、部署或向其他外部会话发送消息。

## 7. 建议的接管顺序

1. 保留现有成果和审查记录；先固定当前可恢复快照、生效证据与准确状态索引。
2. 精确整改 DEV-02-13（F2/F3/F4）、DEV-02-14（F6与真实竞争/dispatcher）、DEV-03-07（F5与F1执行点），并使 DEV-03-06 的取消/超时/重试接入真实运行时。写集互斥，根manifest/lockfile由总控串行整合。
3. 补 DEV-02-08 缺失证据和必要复跑，处理 Notification/权限/heartbeat 待闭环项；受影响节点重新独立验收与当前输入21项Mac门禁。
4. 依赖正式通过后推进 DEV-02-15 备份恢复、DEV-03-08 对账、03-09 Keychain及Workspace；应用用例接通后再做业务UI。
5. V2 R1 由 Human 接受 ADR/AC/权限/Conversation/Gateway/MCP与迁移方案后另行冻结。不在旧节点整改过程中混入新语义。发布仍须Gate I与Human单独决定。

## 8. 证据与复现入口

- [本轮机器摘要](evidence/project-review-2026-10-07/summary.json)
- [检查执行记录](evidence/project-review-2026-10-07/results.json)：含工具未在默认PATH时的首次失败；已用既有验证工具的绝对路径另行复跑，结果见summary与独立日志。
- [Rust测试日志](evidence/project-review-2026-10-07/rust-tests.log)、[TS测试日志](evidence/project-review-2026-10-07/typescript-tests.log)
- [领域反例源码](evidence/project-review-2026-10-07/repro/src/main.rs)、[观察输出](evidence/project-review-2026-10-07/domain-counterexamples.log)
- [guard失败日志](evidence/project-review-2026-10-07/guards.log)
- [检查前输入清单](evidence/project-review-2026-10-07/input-before.json)、[检查后输入清单](evidence/project-review-2026-10-07/input-after.json)

从仓库根复现领域反例：

```bash
cargo run --offline --manifest-path docs/reviews/evidence/project-review-2026-10-07/repro/Cargo.toml
```

主控完成代码/规格/反例与最终结论。资料盘点由两个只读 `gpt-5.6-luna` 子任务承担（progress_inventory：DEV-02-07～14/03-06～07交接证据；scope_inventory：DEV-01～08范围统计）。它们未修改文件、未运行变异装置、未替代R3代码审核。主控独立复算并纠正了折叠Git条目计为文件的盘点错误。
