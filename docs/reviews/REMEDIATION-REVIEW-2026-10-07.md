# 2026-10-07 整改方案复核与修改验收

关联方案：[总控整改任务包](../coordination/REMEDIATION-PLAN-2026-10-07.md)。Human 已授权“制定方案审查、修改”。本批修复既有冻结要求内的缺陷，不提交、推送、发布、改栈或实现 V2 新语义。

## 方案审核结论

**本批局部修复 PASS；相关完整节点仍 BLOCKED。** 本批没有真实应用事务、dispatcher、业务结果写入或 Notification 服务，不会把 helper 全绿当作整节点完成。

审批和缓存修改是内部 Rust API 调整，相关测试同步；没有增加领域状态、稳定错误码或 Adapter wire 字段。既有 migration、manifest、lockfile、生成协议包和前端业务页面未修改。历史终审、上轮报告、输出及作废证据保留。

## 已修改并验证

| 原发现 | 修改 | 验证与边界 |
|---|---|---|
| F2 权限缺省放行 | TeamPolicy、ActorBasePermission、Role actor 的 RolePolicy 不允许 NotApplicable | 全NA、单独跳过必检层均拒绝；条件性 Task/path/capability/approval 的适用性仍由可信应用 resolver 判定，数组不是“检查过”的证明 |
| F3 审批身份/范围不足 | ApprovalGrant 使用实际 Actor/HumanId；P0决定者必须等于可信Owner；加入Team、BindingId/version、action、resource、Task/Run与canonical scope digest的精确上下文 | 非Human、非Owner、自批、同类别不同Role、跨Team、同version不同Binding、改变action/resource/Task/Run/scope均拒绝。scope hash不解析策略语言，应用层须从已验证的canonical参数生成；grant消费须与执行原子提交 |
| F4 过期缓存 | may_reuse 增加注入当前时刻；now必须严格早于expiry | deadline前放行、等于和之后拒绝。方法仅验证已按身份隔离的cache key的新鲜度/版本，不能替代执行点授权 |
| F5 租约token/缩短 | 校验lease token等于当前世代；新expiry取已有expiry与now+ttl最大值 | 不匹配租约拒绝；短/零/负TTL不缩短；过期租约不复活。既有租约到期边界未改变 |
| F6 总时长/TTL | 预算耗尽进入已有DeadLetter判定；增加实际started_at/deadline的结果判定与执行前deadline检查 | 把投递、排队、离线经过时间算入预算；不能在/晚于deadline调度或开始。旧helper只负责已计算的总elapsed预算，真实driver必须使用绝对deadline入口并持久化决定 |
| F8 中文路径guard | URL转换用fileURLToPath；入口可测试；补.d.mts声明，不用any/as掩盖导入类型 | 真实中文仓库路径guard PASS；中文/空格URL单测 PASS；发现并修复新增测试TS7016 |
| F9 当前状态漂移 | 节点表、协作入口、工作日志、冻结清单追加现行复核；保留历史结论 | 02-08/09/13/14、03-06/07改为当前BLOCKED；纠正六机状态、折叠Git条目、测试快照与跨模型复核范围 |

## 本批代码范围

- `crates/domain/src/permission_intersection.rs`、`run_fencing.rs`、`outbox_dispatch.rs`
- 相应既有测试；新增 `crates/domain/tests/review_regressions.rs`
- `scripts/verify-tooling-guards.mjs`、对应类型声明及路径测试
- 现行状态文档、本方案/验收报告与证据。格式化只对允许写集执行；没有cargo fmt --all改写冻结文件。

主控负责R3实现与第二遍反例审查；gpt-5.6-luna只处理DEV-01-03低风险路径任务。该Subagent最初漏验typecheck，主控全检查发现TS7016后扩写集补声明并复验；没有把最初局部PASS当全门禁PASS。

## 验证结果

| 检查 | 结果 |
|---|---|
| 修复前5条行为回归 | **0通过、5失败**，均为行为断言失败 |
| 修复后 `cargo test --workspace --locked --offline` | **480通过、0失败、1 ignored** |
| `pnpm run test:all` | **38通过**，含新增路径用例；业务E2E仍未实现 |
| `node --test scripts/ci/local-gates.test.mjs` | **12通过** |
| cargo build / all-targets、all-features clippy -D warnings | PASS |
| fmt、TS lint、typecheck、frontend build、contracts、unsafe、licenses、toolchain、workflow | PASS；typecheck最初失败后补声明复验通过 |
| `pnpm run verify:guards` | PASS，真实ESLint违规与Rust告警均按预期拦截 |
| Gitleaks工作树/全refs历史 | PASS，redact，0命中 |
| Rust离线audit/licenses | exit0；沿用缓存公告库，仍有既有unmaintained/许可未使用警告，不声明公告最新 |
| 第二遍行为变异 | 健康基线通过；**9个变异全部被行为断言捕获**，无编译错误冒充拦截 |
| 当前最终输入完整21项Mac Gate | **未运行**，尚需外部服务确认；本批不声明整节点DONE |

9个变异分别去掉/弱化：Team必检、Human决定者、scope匹配、grant到期边界、缓存到期边界、lease世代、续期单调性、预算耗尽、实际deadline边界。每次只改`/tmp`副本，恢复基线，核验真实`test result: FAILED`且没有`could not compile`。

Rust全量测试期间发生的后续变更仅为guard类型声明与TS测试去掉cast；Rust源码未再变。对应TS的typecheck、format/lint/unit重新运行并验证前后输入一致：`942f241a3e4f70c39c251cd084e36ca5c366a5b3f8598ce4755377ffa142aab4`。本验收报告与证据是随后新增，不在该清单内。本记录不冒充完整21项“同最终输入”证据。

## 未关闭项与具体下一步

1. **F1运行时接线**：应用层当前没有可信授权查询、事务端口、Event/Outbox原子写入、dispatcher和业务回调落库。必须按方案的DEV-02-13→02-14→03-06/07顺序建设并独立验收。不能让Supervisor直接碰DB，也不能仅加信任调用方的bool或持有启动时token。
2. **F7证据**：DEV-02-08的旧重签最终JSON仍缺失；历史引用不能伪造。本轮新证据证明局部修复，不能重建当年的缺失验收。需在当前输入完成对应Node重验和完整21项。
3. **DEV-02-09**：Human接收者、policy filters/escalation、Attention底层终态与source语义的既有缺口仍保留，当前BLOCKED。涉及真实语义冲突时形成精确ADR提案交Human，而不是自行补状态。
4. **全节点门禁**：在线Node漏洞审计、RustSec公告更新以及冻结依赖安装等最终21项仍未采集；外部服务/权限按Human规则另行确认。未测覆盖率，不宣称达到90%/85%；未验真实业务E2E/恢复/发布。

## 证据

- [机器摘要](evidence/remediation-2026-10-07/summary.json)、[检查记录](evidence/remediation-2026-10-07/results.json)
- [修复前失败](evidence/remediation-2026-10-07/domain-red.log)、[Rust全量](evidence/remediation-2026-10-07/rust-tests.log)、[TS全量](evidence/remediation-2026-10-07/typescript-tests.log)
- [TS首次失败](evidence/remediation-2026-10-07/types.log)、[补声明复验](evidence/remediation-2026-10-07/types-fixed.log)
- [变异结果](evidence/remediation-2026-10-07/mutation-results.json)、[变异源码](evidence/remediation-2026-10-07/mutations.py)
- [检查前输入](evidence/remediation-2026-10-07/input-final-before.json)、[检查后输入](evidence/remediation-2026-10-07/input-final-after.json)
- `evidence/remediation-2026-10-07/before/` 保存本批修改前字节，用于精确对比和受控回退，禁止整库reset丢失此前其他工作。

旧review反例源码保留的是修复前API；内部API已经调整，当前复现应使用workspace的`review_regressions`和本轮测试，而不是要求历史artifact在新API上继续编译。
