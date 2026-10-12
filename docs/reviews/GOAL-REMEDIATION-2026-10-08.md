# 持续整改：DEV-02-09 数据约束复核（2026-10-08）

## 当前结论

**已关闭本批可验证的数据库绕过；整个目标未达成，DEV-02-09 及此前受影响节点保持 BLOCKED。** 本记录接续 [10-07 整改验收](REMEDIATION-REVIEW-2026-10-07.md)，不替代历史验收，也不把存储守卫当作实际通知服务。

- 执行：Codex 总控（GPT-6 系列）；同执行者第二遍反例审查，未宣称独立审核。
- 关联：DEV-02-09；DOMAIN_MODEL §2/3、STATE_MACHINES §2.1/6/7、DATA_CONVENTIONS §6、PERMISSION_MATRIX §3。
- 范围：新增前向 `0023_notification_execution_integrity.sql` 及注册、真库测试；两个旧合法路径 fixture 先解除源 Task 阻塞，再关闭 Attention。
- 非目标：本批没有实现 Notification dispatcher、IPC、Owner 会话认证或其他依赖节点的应用服务。

## 修复与第二遍发现

| 问题 | 本批处理 | 证据与限制 |
|---|---|---|
| recipient 可填实际 Role ID | Notification 插入/收件人更新及逆向 Role 创建/id 更新拒绝碰撞 | 正向与逆向、UPDATE/REPLACE 路径；排除 Role 不等于证明 Human，可信 Owner 执行点仍缺失 |
| UI 时间戳可把未解决事项变 RESOLVED | 更新时读取源 Task/Approval/Message/Run；BLOCKED Task、PENDING Approval、异常 Message、UNKNOWN/未处理失败 Run 不可关闭 | 拒绝后 OPEN 不变；真实源变更后允许路径；事务内失败由调用方回滚，零残留 |
| 同 Team 来源可被偷换 | Notification 的 Event/Message/Policy 引用、Attention 的 source_type/source_id 不可改写 | **新发现**：0019 把列写在 UPDATE OF 内，却漏值比较；v22 实际改动一行并改到新对象，v23 拒绝 |
| 未被 Event 当作 subject 引用的 Notification 可删除 | 所有 Notification 删除均拒绝；DISMISSED 仍可隐藏呈现 | **新发现**：旧审核将删除解释为非漏洞，但 DATA_CONVENTIONS §6 明确“Notification 展示可隐藏但事实保留”；v22 删除一行、v23 保留 |
| 已规定的 Policy filters/escalation 无存储字段 | 添加现有 Policy 聚合的 options 表；JSON 对象、父版本关联、修改版本推进、mandatory 标记不可移除/禁用/改 Event，防替换和删除 | **仅存储约束**；尚无参数字典、自动强制事件分类、渠道/TTL 投递保障，不宣称策略完整 |
| 初版新升级检查误伤历史 | 删除按“当前源状态”反推“历史 RESOLVED 非法”的检查 | Task 曾解决、后续再受阻的历史可合法升级；永久身份碰撞仍原子拒绝 |
| 初版“任意更新 attempt”可作解决依据 | 移除该派生规则 | 需要真实 retry 的原子事实，详见待确认 ADR；SYSTEM 无事实依据时默认拒绝自动 RESOLVED |

0019、0008 和其他已发布迁移零修改。22 个旧迁移的 SHA-256 与上一轮审查快照逐一相等。根 manifest/lockfile 没有本批改动。测试种子仍采用原有 BLOB UUIDv7 表示，不使用生产数据。

## 验证

| 检查 | 结果 |
|---|---|
| 本批最初两条回归（旧行为） | 2 个真实禁止写入成功，回归断言失败 |
| 当前新增 SQLite 专项 | **14 passed / 0 failed**；含旧 schema 对照、真实行值/行数、合法路径、升级与事务回滚 |
| `cargo test --workspace --locked --offline` | **494 passed / 0 failed / 1 ignored，exit 0** |
| `cargo clippy --workspace --all-targets --all-features --locked --offline -- -D warnings` | exit 0 |
| `cargo build --workspace --locked --offline` | exit 0 |
| `cargo fmt --all -- --check` | exit 0；格式修改只使用限定文件 rustfmt |
| 行为变异 | **15/15 被断言捕获**，健康基线 14/14；无编译失败计数 |
| 本轮 Gitleaks 工作树扫描（redact） | exit 0，未发现秘密；全 refs 历史上轮已扫，本轮 HEAD 未变且未重复 |
| Rust 检查输入 | 140 个相关源码/manifest/冻结规格文件检查前后 SHA-256 相等 |

首次扩大测试存在 Rust 字符串转义构造错误；随后 Run fixture 缺少 cancelled_at；clippy 报一处无用 format。均已纠正并复跑。上述构造/编译错误**没有**计入禁止行为拦截或变异成绩；原日志保留，最终结果取 final 日志。

本批没有 TS 变更，未重复前端测试。没有完整 Mac 21 项当前输入证据；此前外部服务授权问题仍待答复。覆盖率未测：本机没有 cargo-llvm-cov，Apple LLVM 21 与 Rust LLVM 22 不一致，未把兼容性未确认的工具输出当作覆盖率证明。

## 尚需完成

1. Human 确认 [ADR-0007](../adr/0007-notification-resolution-and-policy-semantics-proposal.md) 的策略参数、强制事件分类、SYSTEM 处理凭据和 retry/后续 Message 关闭语义；当前 **Proposed**。
2. 构建可信 Owner 身份与 Notification 实际用例，关闭任意合法 UUID 可冒充 Human 的缺口；存储 mandatory 标记不能替代执行保障。
3. 按依赖整改 DEV-02-13 的执行点授权与拒绝审计、DEV-02-14 的真实原子事务与 Outbox，以及 DEV-03-06/07 的取消/重试/当前世代回调落库。现有 helper 全绿不能验收这些能力。
4. DEV-02-08 缺失历史最终 JSON 不补造；以当前输入专项重验及最终完整 Mac Gate 建立新证据。恢复后不得继续引用作废/失配历史证据作为准入。

## 证据与回退

[机器摘要](evidence/goal-remediation-2026-10-08/summary.json)、[全工作区测试](evidence/goal-remediation-2026-10-08/workspace-tests-final.log)、[专项测试](evidence/goal-remediation-2026-10-08/notification-final.log)、[变异台账](evidence/goal-remediation-2026-10-08/mutations/results.json)、[旧迁移哈希](evidence/goal-remediation-2026-10-08/published-migrations.json)。变异 SQL、运行器和完整日志一并保留。

变异只在 `/private/tmp` 副本运行；正式源码没有反复替换。升级碰撞失败时 SQLx 回滚，旧行、版本 22 与原 schema 保持；新迁移不自动修正或删除旧事实。未来实际升级须先做一致性备份，回退使用升级前备份，不在已发布迁移上改字节。没有提交、推送、部署、外部服务访问或生产操作。
