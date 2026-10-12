# ADR-0007：通知策略与待处理事项的执行语义补齐

- 状态：**Partially Accepted：仅第 1 条身份方案于 2026-10-10 获 Human 明确接受；第 2～7 条仍 Proposed**
- 日期：2026-10-08
- 关联：DEV-02-09 整改、DEV-05-08 后续用例；不改变技术栈或既有状态集合。
- 范围：仅补齐冻结规格中未给出参数字典、强制通知分类和 SYSTEM 解决凭据的部分。明确冻结要求的修复无需等待本 ADR。

## 已有规则与未定义部分

2026-10-10 Human 对 [最小 Owner 身份决策](../coordination/DEV-02-09-OWNER-DECISION-2026-10-10.md)回复“接受，继续执行”。该确认仅接受第 1 条及其限定执行计划，不接受其余六条，不授权外部渠道/费用/生产操作。新增实施记录见 [Owner 身份计划](../coordination/DEV-02-09-OWNER-PLAN-2026-10-10.md)。以下 0023“候选”表述按 10-08 历史保留，当前已注册至 0035。

`DOMAIN_MODEL.md §3` 要求 NotificationPolicy 包含 recipient/filters/dedupe/escalation；PRD §17.3 允许按 Team、Role、严重级别和事件类型订阅。`PERMISSION_MATRIX.md §3` 要求不能屏蔽强制安全通知，却没有列出其事件集合。`STATE_MACHINES.md §6` 要求底层事项解决才能关闭 Attention，但没有定义 SYSTEM 的事实源，或新 Run 重试后旧失败 Attention 的关闭时点。

目前 0023 为前向修复候选：策略 options 保存 JSON 对象和 mandatory 标记，**尚未解释 filters/escalation，也未自动分类或投递强制通知**。仅凭存储标记不能验收整个节点。当前 Run Guard 不把“任意更新 attempt 存在”当作解决证据；SYSTEM 自动解决默认拒绝。

## 建议决策

1. **身份**：P0 所有通知接收者均为可信安装 Owner。Owner 来自主程序身份端口，不能从 IPC/Adapter payload 或 UUID 格式推断。Policy 使用该同一可信身份，不复制另一套可任意指定的接收者。数据库排除 Role 碰撞只是附加防御。
2. **普通策略 filters**：JSON 对象只支持可选 `role_ids`（同 Team 的 Role UUID 数组）和 `min_severity`（INFO/WARNING/CRITICAL）。Team 与 event_type 来自主 Policy。空对象表示无额外过滤；未知字段/无效 Role 拒绝，返回既有 `NOTIFICATION_POLICY_INVALID`。不引入表达式语言。
3. **escalation**：空对象表示关闭提醒；仅支持 `after_seconds`（正整数）和 `max_reminders`（1～3）两个成对字段。P0 仅在应用内向同一 Owner 提醒；保留源 Event，以消费幂等键区分提醒次数。每条普通策略的提醒受 TTL 限制；不接入外部渠道。
4. **强制安全通知**：P0 的 `PERMISSION_DENIED` 是强制集合的首个且当前唯一成员。平台保证应用内投递；其 mandatory 标记不能移除，不能关闭、过滤、改 Event 类型或换成外部渠道。强制通知不设置自动到期，不允许忽略；已读可从未读列表移走，事实仍保留。普通策略的开关、过滤、TTL、去重与升级不能让该投递消失。今后扩充集合须列出已有稳定 Event 名称和需求依据。
5. **Run 失败事项**：显式 retry 用例成功创建更高 attempt/token 的 QUEUED Run、匹配当前有效 lease，并完成 Task claim 后，可在同一事务关闭旧 Run 的失败/超时 Attention。失败的 retry、仅多出一条旧历史 Run、UNKNOWN 未对账均不能关闭。SUCCEEDED/CANCELLED 或 Task 经合法用例 DONE/CANCELLED 也可关闭相应事项。
6. **Message 事项**：显式修复/retry/reassign 后重新 QUEUED 可关闭原投递异常 Attention。EXPIRED 本身不证明事项已处理；须有同 Thread、引用原消息的新后续 Message 成功 QUEUED，或 Human 依允许类型执行带原因的 DISMISSED。
7. **SYSTEM 事项**：source_id 必须引用同 Team 的既有不可变 Event。解决由 Human Owner 专用用例完成，保存 source Event、Human 身份、原因、request_id 和追加审计 Event 的不可变关联；关联记录与 Attention RESOLVED 原子提交。没有关联凭据不能自动解决。安全事项不能仅用一般 DISMISSED 清除。

## 备选与取舍

- 接受以上方案：可完成确定的应用服务、真实 SQLite 事务与负向测试，不再依赖 UI 标记或任意 JSON。
- 保持冻结文本不补语义：明确规则内的数据库修复继续保留，SYSTEM、完整通知策略、重试关闭时点仍 BLOCKED，依赖节点不能正式验收。
- Human 修改部分条款：只在确认后的条款上实现；本记录继续 Proposed，直到接受的决策写明。

## 验收与回退

接受后先更新对应冻结规格的补充引用，新增前向 migration（已发布 migration 不改），再逐项实现。需真库验证任意 UUID/Role 伪装 Owner、强制通知屏蔽、非法过滤/提醒参数、重复提醒、retry 回滚、UNKNOWN、跨 Team SYSTEM Event 与伪造处理凭据；所有拒绝零业务副作用并有适用的审计。完整节点仍需当前输入 Mac Gate 与覆盖率，不以接受 ADR 代替验收。

迁移前使用一致性备份；不把当前源状态反推历史 Attention 处理无效，不删除旧事实。部署、提交、外部服务或生产操作不在本提案授权范围内。
