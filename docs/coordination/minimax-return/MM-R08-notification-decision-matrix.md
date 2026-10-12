# MM-R08｜通知与 Attention 的决策矩阵及验收表（只读方案，未实现任何语义）

- 关联节点：DEV-02-09（后续 DEV-05-08、DEV-05-09、DEV-06-15）
- 性质：**只读方案包**。本文不含任何实现、不改 ADR 状态、不改数据库、不改冻结规格。
- 冻结依据：`DOMAIN_MODEL §3/§6`、`STATE_MACHINES §6`、`DATA_CONVENTIONS §5/§6`、`PERMISSION_MATRIX §3`、`ERROR_CATALOG`、`docs/adr/0007-notification-resolution-and-policy-semantics-proposal.md`（**Proposed**）、`docs/reviews/DEEP-AUDIT-2026-10-08.md §3/§4`。
- 前提声明：本轮 Minimaxcode 主责路由（ADR-0008）**不是**接受 ADR-0007。ADR-0007 仍为 Proposed，Human 未逐条确认。

---

## 0. 本方案要解决的真问题

审计在 `DEEP-AUDIT-2026-10-08.md §4` 留下两个并存但被混为一谈的问题：

1. `0023` 的存储修复**成立且必须保留**（policy options / mandatory 标记 / 源不可变 / 自动解决默认拒绝），但它**不构成节点验收**。
2. 「OPEN 的 Attention 实际可以被 DELETE」这一事实，**不能**直接推出「Attention 必须是永久核心历史、必须加永久 DELETE 守卫」。

机制层面的缺口是：**通知执行语义、强制通知分类、SYSTEM 解决凭据、重试关闭时点都还没有权威定义**；而存储层已经先行落了标记。缺的是语义，不是又一个触发器。

本文因此把问题拆成两类，分别给出不同性质的建议：**A 类是冻结文本已明确、只缺落地证据的**；**B 类是冻结文本未定义、必须 Human 逐条决策的**。A 类不需要等 ADR-0007；B 类在 Human 逐条接受前不得实现。

---

## 1. 机制现状盘点（本端实测，只读）

### 1.1 DELETE 守卫的实测不对称

| 表 | DELETE 守卫 | 位置 | 效果 |
|---|---|---|---|
| `notifications` | 有，**两道** | `0014_event_outbox_consistency.sql:435`（仅保护被 Event 引用者）；`0023_notification_execution_integrity.sql:137`（`notifications_history_retained_delete`，**无条件拦截全部 DELETE**） | 任何 Notification 行都删不掉 |
| `attention_items` | **零道** | 全仓 `grep "ON attention_items"` 无任何 `BEFORE DELETE` | 任意 Attention 行（含 OPEN）可被物理 DELETE |

这是审计所述「OPEN 索引实际可消失」的机制根因，且**不是**遗漏了某个 `if` 分支，而是两侧守卫策略从一开始就不同。

### 1.2 冻结文本对两者的表述差异（关键判据）

- `DATA_CONVENTIONS §6` 留存表**明确列出了** Notification：「Message/Delivery/Notification/Approval 随 Team 保留……Notification **展示可隐藏但事实保留**」。→ Notification 有明确留存条款。
- 同一张表**没有单列 AttentionItem**。`§5` 也只列了「Event、Run、Binding、Review decision、DeliveryAttempt、Approval decision」不更新历史语义。
- `DOMAIN_MODEL §6` 第 4 条明确：**「派生 read model 可重建」**。

**结论：AttentionItem 是可重建派生索引，Notification 是有留存条款的事实行。** 二者的删除策略不应相同，而当前实现恰好相反（Notification 删死、Attention 随便删）。

### 1.3 `attention_type` 与 `source_type` 的取值张力

- `attention_type` CHECK 仅六值：`APPROVAL_REQUIRED / TASK_BLOCKED / RUN_FAILED / RUN_TIMED_OUT / MESSAGE_NEEDS_HUMAN / SECURITY_ALERT`
- `source_type` CHECK 五值：`TASK / RUN / MESSAGE / APPROVAL / SYSTEM`
- `source_id` 为多态裸 BLOB（无外键），SYSTEM 场景下按 ADR-0007 §7 应引用同 Team 既有不可变 Event。

即：SYSTEM 来源必须借用上述六个 `attention_type` 之一表达。**当前没有为 SYSTEM 事实来源定义的合法 `attention_type` 取值**，这是 ADR-0007 §7 落地前必须先解决的建模问题（见 D-06）。

---

## 2. A 类：冻结文本已明确，只缺落地证据（**不需等 ADR-0007，可按现有冻结文本实施**）

| ID | 问题 | 冻结依据 | 推荐动作 | 兼容成本 | 验收 |
|---|---|---|---|---|---|
| A-01 | 强制安全通知的事件集合在 `PERMISSION_MATRIX §3` 只说「不能屏蔽强制安全通知」，**未列成员** | `PERMISSION_MATRIX §3`、`STATE_MACHINES §6`「非强制保留通知」 | 在 `PERMISSION_MATRIX §3` 补一句显式集合：`PERMISSION_DENIED` 为 P0 唯一成员。**这是补字典，不是新语义** | 低：仅文档 + 一个分类函数 | 真库：把 `PERMISSION_DENIED` 策略标 mandatory 后，普通策略的关闭/过滤/TTL/去重/升级全部无法让该投递消失；负向断言拒绝零业务副作用 |
| A-02 | Notification 接收者「是 Human」的证明只做到「不是 Role」 | `0023` 注释自陈：UUID 形状不是 Human 身份证明，执行点须比对可信安装 Owner | 保留 `0023` 的双向角色排除触发器（它只是附加防御），在应用执行点接入可信 Owner 身份端口 | 中：需 identity 端口（属 K2/K3 范畴） | 真库：任意 UUID / 任意 Role 都不能被当作 Owner；跨 Team 伪装被拒并有拒绝 Event |
| A-03 | `notifications` 无条件 DELETE 拦截是否过严 | `DATA_CONVENTIONS §6`「Notification **展示可隐藏但事实保留**」 | **保留现状**。该表有明确留存条款，「展示隐藏」应通过状态/过滤实现，不通过物理删除 | 零：不动 | 已有 `0023` 守卫测试继续绿 |
| A-04 | `attention_items` 零 DELETE 守卫 | `DATA_CONVENTIONS §6` 未单列 + `DOMAIN_MODEL §6` 派生可重建 | **不加永久 DELETE 守卫**（见 B-01、D-01）。改为在应用层约束「应用层不得把删索引当作解决源事项的手段」 | 零：不动迁移 | 应用层反例：源事项仍 OPEN 时，受控重建流程不得使该 OPEN 事项从用户视图消失而留下无处置痕迹 |

### A 类共同红线
不得用「删掉索引」来宣称问题已解决。任何用户可见的消失都必须伴随源状态变更或带原因的处置审计。

---

## 3. B 类：冻结文本未定义，**必须 Human 逐条决策**（未接受前不得实现）

每条给出：具体问题 / 推荐项 / 兼容成本 / 验收方式。**Human 只应逐条接受或修改，不应给一次性泛确认**——泛确认会连带放大未列的权限、通知渠道与费用影响。

### D-01｜AttentionItem 派生索引的受控重建边界
- **具体问题**：`attention_items` 可被物理删除。是禁止一切底层删除，还是允许受控重建？危险用户路径是：源事项（Run 仍 FAILED / Approval 仍 PENDING）仍需处理，派生索引被删后该事项从「待我处理」队列静默消失，Human 以为已处理。
- **推荐项**：允许受控重建，但**必须**满足全部三条：① 源事项的当前状态仍是权威事实，重建结果必须由源状态推导，不得由旧索引行回填；② 重建须幂等，同一 `(team_id, source_type, source_id, attention_type)` 重建后仍唯一（`0008` 已有该唯一索引）；③ 重建与源状态变更、Event 在**同一事务**原子提交，且追加一条不可变审计 Event。
- **兼容成本**：低—中。无需改 `0008`/`0019`/`0023` 任何字节；新增前向迁移仅在需要记录重建审计时。
- **验收**：正对照——Run 从 FAILED 经合法 retry→SUCCEEDED 后重建，Attention 正确转 RESOLVED。负向——源 Run 仍 FAILED 时执行「删除索引」，随后重建必须仍产出同一条 OPEN 事项，且必须有重建审计；跨 Team source_id 重建被拒。

### D-02｜强制通知集合与不可关闭性
- **具体问题**：P0 强制集合的首个且唯一成员是否定为 `PERMISSION_DENIED`；mandatory 能否被移除、过滤、改 Event 类型或换外部渠道。
- **推荐项**：定为 `PERMISSION_DENIED`。mandatory 标记不可移除、不可关闭、不可过滤、不可改 Event 类型、不可换外部渠道；不设自动到期；已读可从未读列表移走但事实保留。扩充集合须同时列出既有稳定 Event 名与需求依据。
- **兼容成本**：低。`0023` 已有 `is_mandatory` 列与「mandatory 策略不得停用/改 event_type」触发器；补的是分类与投递保证。
- **验收**：真库负向——尝试关闭/过滤/改类型/降级 mandatory 全部被拒且零业务副作用；正向——mandatory 投递必达。

### D-03｜filters 的参数字典
- **具体问题**：`notification_policy_options.filters` 现仅校验「是合法 JSON 对象」，未定义允许键。
- **推荐项**：只支持可选 `role_ids`（同 Team 的 Role UUID 数组）与 `min_severity`（`INFO/WARNING/CRITICAL`）。Team 与 `event_type` 取自主 Policy。空对象 = 无额外过滤。未知字段或非法 Role 一律拒绝，返回**既有** `NOTIFICATION_POLICY_INVALID`。不引入表达式语言。
- **兼容成本**：中。`0023` 的 CHECK 只保证 JSON 合法；键级校验需应用层 + 可能的前向迁移。
- **验收**：非法 `role_ids`（非 UUID / 跨 Team）、非法 `min_severity`、未知键全部被拒；合法组合正常投递。

### D-04｜escalation 的参数字典
- **具体问题**：`escalation` 同上，未定义允许键与范围。
- **推荐项**：空对象 = 关闭提醒。仅支持成对的 `after_seconds`（正整数）与 `max_reminders`（1～3）。P0 **只在应用内**向同一 Owner 提醒，不接入外部渠道。保留源 Event，用消费幂等键区分提醒次数。每条普通策略的提醒受 TTL 限制。
- **兼容成本**：中。
- **验收**：重复提醒按幂等键去重；`max_reminders` 上界生效；TTL 到期后不再提醒。

### D-05｜Run 失败 Attention 的关闭时点
- **具体问题**：新 Run 重试后，旧失败 Attention 何时可关闭。
- **推荐项**：仅当**显式 retry 成功创建**了更高 attempt/token 的 QUEUED Run、匹配当前有效 lease、**并完成 Task claim** 后，才可在**同一事务**关闭旧 Run 的失败/超时 Attention。以下**均不能**关闭：失败的 retry、仅多出一条旧历史 Run、UNKNOWN 未对账。SUCCEEDED/CANCELLED 或 Task 经合法用例转 DONE/CANCELLED 亦可关闭相应事项。
- **兼容成本**：中。`0023` 的 `attention_items_source_resolved_update` 已实现「无证明不自动解决」，本条是补**有证明**时的正向路径。
- **验收**：上述四类正反例逐一真库验证；retry 回滚、UNKNOWN、跨 Team 均不得关闭。

### D-06｜SYSTEM 事项的事实来源与建模
- **具体问题**：ADR-0007 §7 要求 `source_id` 引用同 Team 既有不可变 Event、由 Human Owner 专用用例带原因解决。但 `attention_type` 的六个取值里**没有**表达 SYSTEM 事实来源的成员（见 §1.3）。
- **推荐项**：先确定 SYSTEM 用哪个既有 `attention_type` 表达（`SECURITY_ALERT` 是唯一语义候选，但需 Human 确认其是否专用于此），再定义解决凭据：`source_event_id` + Human 身份 + 原因 + `request_id` + 追加审计 Event 的不可变关联，且关联记录与 `RESOLVED` **原子提交**。无关联凭据不得自动解决。安全事项不得仅用一般 DISMISSED 清除。
- **兼容成本**：中—高。若需新增 `attention_type` 取值，必须走前向迁移改 `CHECK`，并同步 `ERROR_CATALOG` 与规格。**这已接近破坏性变更，建议由 Human 明确后单独立项。**
- **验收**：跨 Team SYSTEM Event 引用被拒；伪造处理凭据被拒；合法 Owner 处置成功且审计可回放。

### D-07｜Message 投递异常 Attention 的关闭时点
- **具体问题**：投递异常（如 `MESSAGE_NEEDS_HUMAN`）何时算已处理。
- **推荐项**：显式 repair/retry/reassign 后重新 QUEUED 可关闭原投递异常 Attention。`EXPIRED` **本身不证明**事项已处理——须有同 Thread、引用原消息的新后续 Message 成功 QUEUED，或 Human 依允许类型执行带原因的 DISMISSED。
- **兼容成本**：低—中。`0023` 已有 MESSAGE 源未解决即拒绝 RESOLVED 的守卫。
- **验收**：仅 EXPIRED 而无后续 Message 时不得关闭；有合法后续 Message 时可关闭。

---

## 4. Owner 事实来源（不变量）

所有通知接收者与所有 SYSTEM/安全处置的决策者，均为**可信安装 Owner**，来源必须是主程序身份端口。以下**均不构成** Owner 身份证明：

- IPC payload 或 Adapter 传来的 `allow` / `owner_id` 布尔或标识；
- UUID 形状、版本位、variant 位；
- `role.name`、Harness 类型名或显示名；
- 数据库中「该 id 不与任何 Role 碰撞」这一**排除性**结论（`0023` 触发器只是附加防御，不是身份来源）。

Policy 使用**同一**可信身份，不允许另建一套可任意指定的接收者集合。

---

## 5. 闭环测试矩阵（全部要求真 SQLite，负向零业务副作用且有适用审计）

| 组 | 场景 | 期望 |
|---|---|---|
| 强制通知 | `PERMISSION_DENIED` mandatory 策略被关闭 / 过滤 / 改 event_type / 取消 mandatory | 全部拒绝；投递仍必达；零业务副作用 |
| 强制通知 | 普通策略开关、过滤、TTL、去重、升级作用于 mandatory 事件 | 不得使强制投递消失 |
| filters | 合法 `role_ids` + `min_severity` | 正常投递 |
| filters | 非 UUID Role、跨 Team Role、非法 severity、未知键 | 拒绝 `NOTIFICATION_POLICY_INVALID` |
| escalation | 合法 `after_seconds` + `max_reminders` | 按幂等键去重，次数受上界约束 |
| escalation | 缺其一、越界值、TTL 过期后 | 拒绝或不提醒 |
| Run 关闭 | 显式 retry 创建更高 attempt/token QUEUED Run + 有效 lease + Task claim | 旧失败/超时 Attention 同事务 RESOLVED |
| Run 关闭 | retry 失败 / 仅多出旧历史 Run / UNKNOWN 未对账 | **不得**关闭 |
| Message 关闭 | repair/retry/reassign 后重新 QUEUED | 原投递异常 Attention 可关闭 |
| Message 关闭 | 仅 EXPIRED、无合法后续 Message | **不得**关闭 |
| SYSTEM | 引用跨 Team Event | 拒绝 |
| SYSTEM | 无处置凭据的自动解决 | 拒绝 |
| SYSTEM | 合法 Owner 带原因处置 | 成功，审计 Event 与 RESOLVED 原子提交，可回放 |
| 身份伪装 | 任意 UUID / 任意 Role 冒充 Owner | 拒绝并有拒绝 Event |
| 派生重建 | 源仍 OPEN 时删除索引后重建 | 必须仍产出 OPEN，且有重建审计 |

---

## 6. 授权与费用边界（供 Human 逐条确认时注意）

逐条接受 D-01～D-07 **不会**带来以下任何一项；如需，请单独提出：

- 不新增任何 Notification 渠道（无邮件/IM/推送/短信）；P0 仅应用内提醒。
- 不新增任何外部服务调用、联网或第三方 SDK。
- 不扩大任何既有权限矩阵条目；不使任何 Harness/Role 获得 Human-only 能力。
- 新增费用 $0。
- 不改技术栈、不改状态集合、不改 wire 字段、不改错误码集合（复用 `NOTIFICATION_POLICY_INVALID` 与既有 `ATTENTION_*` / `NOTIFICATION_*` 码）。

---

## 7. 明确不做的事

- 不改 `ADR-0007` 状态（仍 Proposed）。
- 不改任何迁移字节（`0001`～`0024` 与 `0025`～`0027` 全部只读）。
- 不改 `DEVELOPMENT_NODES.md` 中任何节点的正式状态；DEV-02-09 仍 BLOCKED，依赖节点不得因本方案启动。
- 不实现 B 类任何条款。
- 不用「Attention 可被删除」推出「必须加永久守卫」，也不用「它是派生索引」推出「删掉就算解决」。

---

## 8. 交接

本方案交 Codex 独立复核；B 类需 Human 逐条裁决后，由 Codex 签发精确 migration 编号与写集再实现。实现前须先更新对应冻结规格的补充引用（已发布 migration 不改，只做前向）。