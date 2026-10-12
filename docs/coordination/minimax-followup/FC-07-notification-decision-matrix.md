# FC-07｜Notification 与 Attention 决策矩阵（R3 重写，只读）

> 关联节点：DEV-02-09（后续 DEV-05-08、DEV-05-09、DEV-06-15）
> 性质：**只读方案。未实现任何语义、未改 ADR 状态、未改数据库、未改冻结规格。**
> 被退回对象：`docs/coordination/minimax-return/MM-R08-notification-decision-matrix.md`（历史留痕，**未覆盖、未改写、未删除**）
> 退回意见：`docs/reviews/MINIMAX-RETURN-AUDIT-2026-10-08.md` §3 MR-06 / P1
> 前提：ADR-0007 仍为 **Proposed**，本轮 Minimaxcode 路由（ADR-0008）**不是**接受 ADR-0007。

---

## 0. 分类原则（本轮唯一的硬规则）

> **A 类 = 冻结文本里逐字写着的规则**，不需要等待任何裁决。
> **B 类 = 冻结文本未定义的语义**，必须 Human 逐条裁决；**未接受前不得实现，也不得由执行者「补字典」后自行实施**。

上一轮 R08 违反的正是这条：A-01 把一个**冻结文本从未列出的集合**写成了「补字典，不是新语义，可直接实施」，
而 D-02 又要求 Human 确认同一集合——同一件事在两处被定性为相反的东西，执行者按 A 类动手就会改掉冻结规格。
本轮把该集合**整体移入 B 类**。

---

## 1. 退回意见逐条纠正

| # | Codex 的退回理由（R08 的错误） | 本方案的纠正 |
|---|---|---|
| R08-1 | **A/B 自相矛盾**：A-01 声称 `PERMISSION_DENIED` 是 P0 唯一 mandatory 成员属「补字典」可直接实施，D-02 又要 Human 确认同一集合。冻结文本根本没有列出该唯一集合，ADR-0007 仍 Proposed。 | **`PERMISSION_DENIED` 唯一 mandatory 集合整体移入 B 类（B-02）**。A 类中**不含**任何关于 mandatory 集合的条目。执行者**不得**依据 A 类修改冻结规格。 |
| R08-2 | **不得仅因「没有同名 SYSTEM 枚举」就认定无合法类型**：`SYSTEM` 是 `source_type`，`SECURITY_ALERT` 是 `attention_type`，两者是不同维度。应先证明具体语义缺失再提决策，且不先新增状态/类型。 | §3 先做**两轴分离**的事实陈述，再逐条证明缺的是哪一个语义。**本方案不新增任何 `attention_type` 或 `source_type`**；把「需要新增类型」降级为 B-03 的一个**待裁决选项**，不是结论。 |
| R08-3 | **D-01 重建示例错误**：写成「旧失败 Run retry 成功变 SUCCEEDED」，与「追加新 Run」语义不符。 | D-05 已重写：**旧 Run 永远是 FAILED，永不变为 SUCCEEDED**。三者关系（Task / 新 attempt / 处置证明）在 §4.5 写清，并附负向反例。 |
| R08-4 | **缺少规定的 RESULT 与 evidence/模型输入命令归档**——上一轮执行端的遗漏。 | 本包补齐：`FC-07-RESULT.md` 记录实际模型/版本、输入 SHA、读过的文件与规格条款、验证命令/exit/日志、风险与回退；evidence 目录内含可重放脚本与逐字节可复现的日志。 |

---

## 2. 两轴事实（先分清维度，再谈缺口）

`attention_items`（`0008`）的两个枚举是**正交**的：

| 维度 | 列 | 取值 | 含义 |
|---|---|---|---|
| **事项种类** | `attention_type` | `APPROVAL_REQUIRED / TASK_BLOCKED / RUN_FAILED / RUN_TIMED_OUT / MESSAGE_NEEDS_HUMAN / SECURITY_ALERT` | 这是**什么样的待办** |
| **事实来源** | `source_type` | `TASK / RUN / MESSAGE / APPROVAL / SYSTEM` | 这件事**由什么对象的事实**引起 |

因此：

- 「没有叫 `SYSTEM` 的 `attention_type`」**不是缺陷**——`SYSTEM` 从来就不是 `attention_type` 的候选值，两列不可互换。
- 真正要问的是：当 `source_type='SYSTEM'` 时，`source_id` 指向什么、该用哪一个**已存在的** `attention_type` 表达、以及什么凭据允许把它关掉。
- ADR-0007 §7 已经回答了后两问的一部分（`source_id` 引用同 Team 既有不可变 Event；由 Human Owner 专用用例带原因解决；关联记录与 RESOLVED 原子提交）。**那是 ADR 的建议，不是冻结文本**，所以仍属 B 类。

**本轮实测的机制现状（只读，随 0023 一并复核）**：`notifications` 有无条件 DELETE 拦截（`0023:136`）；`attention_items` 无 DELETE 守卫。这是已知的两侧不对称，已如实记录，但**不作为「必须加永久守卫」的论据**（见 A-04）。

---

## 3. A 类（仅冻结文本明写，可按现有冻结文本实施）

| ID | 规则 | 冻结原文 | 兼容成本 | 验收 |
|---|---|---|---|---|
| A-01 | Notification 接收者是 Human，不得是 Role；反向也不得把已用于 Notification 的身份变成 Role | `DOMAIN_MODEL §3`（NotificationPolicy「接收者是 Human」）；`0023` 已有双向触发器 | 零（存储层已有） | 真库：任意 Role id 作接收者被拒；反向插入亦被拒 |
| A-02 | Notification 可从视图隐藏，但**事实保留**，不得物理删除 | `DATA §6`「Notification 展示可隐藏但事实保留」；`0023:136` 无条件 DELETE 拦截 | 零（已有） | 真库：任意 `notifications` 行 DELETE 被拒且行未变 |
| A-03 | 重复投递只合并**呈现**，不删除源 Event | `STATE_MACHINES §6`「去重只合并投递呈现，不删除源 Event」 | 低 | 真库：去重后源 Event 仍可查、哈希链不受影响 |
| A-04 | AttentionItem **不加**永久 DELETE 守卫 | `DATA §6` 留存表**未单列** Attention；`DATA §5` 不更新历史语义清单**不含**它；`DOMAIN_MODEL §6` 第 4 条明写「派生 read model 可重建」 | 零 | 应用层反例：源事项仍 OPEN 时，受控重建不得让该事项从待办队列消失且不留处置痕迹 |
| A-05 | `READ` 只表示已查看，**不表示关联事项已解决** | `STATE_MACHINES §6` 明写 | 零 | UI/查询层不得以 `READ` 推导 `RESOLVED` |
| A-06 | 底层 Approval/Run/Message 仍需处理时，不得仅从 UI 把它标为 RESOLVED | `STATE_MACHINES §6`；`0023:42 attention_items_source_resolved_update` 已实现「无证明不自动解决」 | 零（已有） | 真库：Task 仍 BLOCKED / Approval 仍 PENDING / Message 仍 FAILED\|NEEDS_HUMAN / Run 仍 UNKNOWN 时 RESOLVED 被拒 |

**A 类红线**：不得用「删掉索引」宣称事项已解决；任何用户可见的消失必须伴随源状态变更或带原因的处置审计。

---

## 4. B 类（冻结文本未定义，**必须 Human 逐条裁决**，未接受前不得实现）

每条给出：具体问题 / 推荐 / 兼容成本 / 验收。**Human 应逐条接受或修改，不宜一次性泛确认**——泛确认会连带放大未列出的渠道、权限与费用影响。

### B-01｜Attention 派生索引的受控重建边界

- **问题**：`attention_items` 可被物理删除。危险路径是源事项（Run 仍 FAILED / Approval 仍 PENDING）仍需处理，而派生索引消失后该事项从「待我处理」队列静默消失。
- **推荐**：允许受控重建，但必须同时满足：① 重建结果由**源状态**推导，不得由旧索引行回填；② 同一 `(team_id, source_type, source_id, attention_type)` 重建后仍唯一（`0008` 已有该索引）；③ 重建与源状态变更、Event 在**同一事务**原子提交，并追加一条不可变审计 Event。
- **成本**：低—中。无需改 `0008`/`0019`/`0023` 任何字节；仅在需要记录重建审计时新增前向迁移。
- **验收**：负向——源 Run 仍 FAILED 时删除索引，重建必须**仍产出 OPEN** 且有重建审计；跨 Team `source_id` 被拒。

### B-02｜强制安全通知的成员集合（**本轮从 A 类移入**）

- **为什么不能留在 A 类**：`PERMISSION_MATRIX §3` 只写了「不能屏蔽强制安全通知」，**没有列出任何成员**；`ADR-0007` §4 把它写成建议且**仍是 Proposed**。因此「`PERMISSION_DENIED` 是 P0 唯一成员」是**未冻结的冻结文本缺口**，不是可由执行者补上的字典。
- **问题**：P0 强制集合的成员是谁；mandatory 能否被移除、过滤、改 Event 类型或换渠道。
- **推荐**：定为 `PERMISSION_DENIED`；mandatory 不可移除、不可关闭、不可过滤、不可改 Event 类型、不可换外部渠道；不设自动到期；已读可从未读列表移走但事实保留。今后扩充集合须同时列出已有稳定 Event 名与需求依据。
- **成本**：低。`0023` 已有 `is_mandatory` 列与「mandatory 策略不得停用/改 event_type」触发器；缺的是分类与投递保证。
- **验收**：真库负向——尝试关闭/过滤/改类型/降级 mandatory 全部被拒且零业务副作用；正向——mandatory 投递必达。
- **裁决要点**：Human 须明确「唯一成员」这一断言本身，而不只是「mandatory 不可关闭」这条已冻结规则。

### B-03｜`source_type='SYSTEM'` 用哪个既有 `attention_type` 表达

- **问题**：`SYSTEM` 是来源维度，六个 `attention_type` 中没有「SYSTEM 事项」这一成员。**但这不等于缺类型**——需要先确定 SYSTEM 事项的**语义**属于哪一类。
- **推荐（按序）**：先判断该事项的事实性质。若为安全类事实，用既有 `SECURITY_ALERT`；若为运行/审批/消息类事实，则其 `source_type` 本应分别是 `RUN`/`APPROVAL`/`MESSAGE`，`SYSTEM` 只保留给「既无业务聚合、又必须留痕」的事实（对账异常、策略冲突等）。**在 Human 确认前不新增任何枚举值。**
- **成本**：若最终结论确需新增 `attention_type`，则要走前向迁移改 CHECK 并同步 `ERROR_CATALOG` 与规格，属破坏性变更，**建议单独立项**。
- **验收**：跨 Team SYSTEM Event 引用被拒；伪造处置凭据被拒；合法 Owner 处置成功且审计可回放。

### B-04｜NotificationPolicy `filters` 的参数字典

- **问题**：`0023` 的 `notification_policy_options.filters` 只校验「是合法 JSON 对象」，未定义允许键。
- **推荐**：只支持可选 `role_ids`（同 Team 的 Role UUID 数组）与 `min_severity`（`INFO/WARNING/CRITICAL`）。Team 与 `event_type` 取自主 Policy。空对象 = 无额外过滤。未知字段或非法 Role 一律拒绝，返回**既有** `NOTIFICATION_POLICY_INVALID`。不引入表达式语言。
- **成本**：中。`0023` 的 CHECK 只保证 JSON 合法；键级校验需应用层 + 可能的前向迁移。
- **验收**：非法 `role_ids`（非 UUID / 跨 Team）、非法 `min_severity`、未知键全部被拒；合法组合正常投递。

### B-05｜`escalation` 的参数字典

- **问题**：`escalation` 同上，未定义允许键与范围。
- **推荐**：空对象 = 关闭提醒。仅支持成对的 `after_seconds`（正整数）与 `max_reminders`（1～3）。P0 **只在应用内**向同一可信 Owner 提醒，不接入外部渠道。保留源 Event，用消费幂等键区分提醒次数。每条普通策略的提醒受 TTL 限制。
- **成本**：中。
- **验收**：重复提醒按幂等键去重；`max_reminders` 上界生效；TTL 到期后不再提醒。

### B-06｜Run 失败/超时 Attention 的关闭时点（**已纠正 R08 的重建示例**）

- **被退回的错误示例**：R08 写「Run 从 FAILED 经合法 retry→SUCCEEDED 后重建，Attention 正确转 RESOLVED」——这把**旧 Run** 说成变成了 SUCCEEDED。
- **冻结事实**：`STATE_MACHINES §3` 末行明写「自动重试只读取终态 Run，**创建新 Run**；禁止把旧 Run 改回 QUEUED」；`0027` 已把终态→终态与回退一并封死。**旧 Run 永远是 FAILED/TIMED_OUT，永远不会变成 SUCCEEDED。**
- **三者的关系**（`D-05` 的正确表述）：

  | 对象 | 发生了什么 | 是否被改写 |
  |---|---|---|
  | **旧 Run** | 显式 retry 成功后**保持 FAILED**（原失败记录永久留存） | 否 |
  | **新 attempt（新 Run）** | `attempt` 与 `fencing_token` 更高，`QUEUED`，匹配当前有效 lease，并**完成 Task claim** | 新建，不覆盖旧 Run |
  | **处置证明** | 指向**新 Run** 的一组证据，用于授权关闭**旧 Run** 的失败/超时 Attention | 新建，与 Attention RESOLVED 原子提交 |

- **问题**：旧失败 Attention 何时可关闭。
- **推荐**：仅当显式 retry **成功创建**了更高 attempt/token 的 QUEUED Run、匹配当前有效 lease、**并完成 Task claim** 后，才可在**同一事务**关闭旧 Run 的失败/超时 Attention。以下**均不能**关闭：失败的 retry、仅多出一条旧历史 Run、**UNKNOWN 未对账**、新 Run 尚未完成 Task claim。`SUCCEEDED`/`CANCELLED`，或 Task 经合法用例转 `DONE`/`CANCELLED`，亦可关闭相应事项。
- **成本**：中。`0023` 已实现「无证明不自动解决」，本条补**有证明**时的正向路径。
- **验收（正反对照成对）**：
  - 正：retry 成功 + 高 token + 有效 lease + 完成 Task claim → 旧失败 Attention 同事务 RESOLVED；
  - 负 1：retry 创建失败 → 不得关闭；
  - 负 2：仅多出一条**旧历史** Run，无 claim → 不得关闭；
  - 负 3：旧 Run 为 `UNKNOWN` 未对账 → 不得关闭（`0023` 已按 `r.status != 'UNKNOWN'` 拒绝）；
  - 负 4：跨 Team Run → 不得关闭；
  - 负 5：**尝试把旧 FAILED Run 直接改成 SUCCEEDED** → 必须被 `0027` 拒绝（本轮**明确不实现**任何绕过）。

### B-07｜Message 投递异常 Attention 的关闭时点

- **问题**：投递异常（如 `MESSAGE_NEEDS_HUMAN`）何时算已处理。
- **推荐**：显式 repair/retry/reassign 后重新 QUEUED 可关闭原投递异常 Attention。`EXPIRED` **本身不证明**事项已处理——须有同 Thread、引用原消息的新后续 Message 成功 QUEUED，或 Human 依允许类型执行带原因的 DISMISSED。
- **成本**：低—中。`0023` 已有「MESSAGE 源未解决即拒绝 RESOLVED」的守卫。
- **验收**：仅 EXPIRED 而无合法后续 Message 时不得关闭；有合法后续 Message 时可关闭。

### B-08｜Owner 身份的事实来源

- **问题**：通知接收者与 SYSTEM/安全处置的决策者，其可信身份从哪来。
- **推荐**：来源必须是**主程序身份端口**的可信安装 Owner。以下**均不构成**身份证明：IPC payload 或 Adapter 传来的 `allow`/`owner_id`；UUID 形状或版本位/variant 位；`role.name` 或 Harness 显示名；数据库中「该 id 不与任何 Role 碰撞」这一**排除性**结论（`0023` 触发器只是附加防御）。Policy 使用同一可信身份，不另建可任意指定的接收者集合。
- **成本**：中。需 identity 端口（属 K1/K2 范畴，不在本节点）。
- **验收**：任意 UUID / 任意 Role 冒充 Owner 被拒并有拒绝 Event；跨 Team 伪装被拒。

---

## 5. 闭环测试矩阵（全部要求真 SQLite，负向零业务副作用且有适用审计）

| 组 | 场景 | 期望 | 类别 |
|---|---|---|---|
| 强制通知 | `PERMISSION_DENIED` mandatory 策略被关闭/过滤/改 event_type/取消 mandatory | 全部拒绝；投递仍必达；零业务副作用 | **B-02** |
| 强制通知 | 普通策略开关、过滤、TTL、去重、升级作用于 mandatory 事件 | 不得使强制投递消失 | **B-02** |
| 保留 | `notifications` 物理 DELETE | 拒绝，行未变 | A-02 |
| 身份 | 任意 Role 作接收者；反向把已用身份变成 Role | 双向拒绝 | A-01 |
| 去重 | 去重窗口内重复 | 只合并呈现，源 Event 仍可查 | A-03 |
| filters | 合法 `role_ids` + `min_severity` | 正常投递 | B-04 |
| filters | 非 UUID Role、跨 Team Role、非法 severity、未知键 | 拒绝 `NOTIFICATION_POLICY_INVALID` | B-04 |
| escalation | 合法 `after_seconds` + `max_reminders` | 按幂等键去重，次数受上界 | B-05 |
| escalation | 缺其一、越界值、TTL 过期后 | 拒绝或不提醒 | B-05 |
| Run 关闭 | retry 创建更高 token QUEUED Run + 有效 lease + **完成 Task claim** | 旧失败/超时 Attention 同事务 RESOLVED | B-06 |
| Run 关闭 | retry 失败 / 仅多出旧历史 Run / UNKNOWN 未对账 / 未 claim | **不得**关闭 | B-06 |
| Run 关闭 | 把旧 FAILED Run 改成 SUCCEEDED | 必须被拒绝（**本轮不实现绕过**） | B-06 |
| Message 关闭 | repair/retry/reassign 后重新 QUEUED | 原投递异常 Attention 可关闭 | B-07 |
| Message 关闭 | 仅 EXPIRED、无合法后续 Message | **不得**关闭 | B-07 |
| SYSTEM | 引用跨 Team Event | 拒绝 | B-03 |
| SYSTEM | 无处置凭据的自动解决 | 拒绝 | B-03 |
| SYSTEM | 合法 Owner 带原因处置 | 成功，审计 Event 与 RESOLVED 原子提交，可回放 | B-03 |
| 身份伪装 | 任意 UUID / 任意 Role 冒充 Owner | 拒绝并有拒绝 Event | B-08 |
| 派生重建 | 源仍 OPEN 时删除索引后重建 | 必须仍产出 OPEN，且有重建审计 | B-01 / A-04 |

---

## 6. 授权与费用边界（供 Human 逐条确认时注意）

逐条接受 B-01～B-08 **不会**带来以下任何一项；如需，请单独提出：

- 不新增任何 Notification 渠道（无邮件/IM/推送/短信）；P0 仅应用内提醒。
- 不新增任何外部服务调用、联网或第三方 SDK。
- 不扩大任何既有权限矩阵条目；不使任何 Harness/Role 获得 Human-only 能力。
- 新增费用 **$0**。
- 不改技术栈、不改状态集合、不改 wire 字段、不改错误码集合（复用 `NOTIFICATION_POLICY_INVALID` 与既有 `ATTENTION_*` / `NOTIFICATION_*` 码）。

**唯二可能触及破坏性变更的是 B-03**（若结论是需要新增 `attention_type`，须改 CHECK）——已单列并建议独立立项。

---

## 7. 明确不做的事

- 不改 `ADR-0007` 状态（仍 **Proposed**）；不占用任何 ADR 编号；不改任何 Accepted 状态。
- 不改任何迁移字节（`0001`～`0028` 全部只读）。
- 不改 `DEVELOPMENT_NODES.md` 中任何节点状态；DEV-02-09 仍 BLOCKED，依赖节点不得因本方案启动。
- **不实现 B 类任何条款**；不把待决条款写成已接受。
- 不用「Attention 可被删除」推出「必须加永久守卫」，也不用「它是派生索引」推出「删掉就算解决」。