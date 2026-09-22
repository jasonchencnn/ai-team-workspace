# 权限矩阵、审批边界与 Human-only 动作

> 对应节点：DEV-00-05  
> 状态：Frozen  
> 决策原则：能力不等于权限；UI 可见性不等于授权；所有检查在控制面最终执行点重算。

## 1. 主体模型

| 主体 | 定义 | P0 说明 |
|---|---|---|
| Human Owner | 安装和 Team 的最终控制者 | 唯一 Human 角色 |
| Human Admin | 未来多 Human 中的受限管理员 | 数据模型保留，P0 不开放邀请 |
| Role: Leader | 被绑定 Harness 代表的团队管理岗位 | 可管理任务，但不能改 Binding/凭据/自身权限 |
| Role: Product | 需求、计划、决策岗位 | 默认不写生产代码、不执行部署 |
| Role: Developer | Frontend/Backend 等实现岗位 | 管理自己的 Task/Run 和授权代码路径 |
| Role: QA/Reviewer | 测试和验收岗位 | 读取产物、写测试/评审、决定授权 Review |
| System | 定时器、Outbox、恢复和策略执行者 | 只能执行预定义内部命令，无自由扩权 |
| Adapter/Harness | 通过当前 Binding 代表 Role 的不完全可信执行源 | 有效权限不超过 Role；每次回调验证 Binding/Run/token |

Harness 不作为权限主体持久化。Event actor 同时记录 `actor_kind=ROLE_BINDING`、`role_id`、`binding_id/version`、`harness_instance_id`，以便既表达稳定岗位又能审计实际执行者。

## 2. 有效权限算法

```text
allow = TeamPolicy(action, resource)
      ∩ ActorBasePermission
      ∩ RolePolicy（Role/Binding actor 时）
      ∩ TaskScope（关联 Task/Run 时）
      ∩ WorkspacePathPolicy（文件动作时）
      ∩ HarnessCapability（由 Harness 执行时）
      ∩ ActiveBindingAndFencing（运行时动作）
      ∩ ValidApprovalGrant（高风险动作）
```

计算顺序：

1. 验证 actor 身份、Team 隔离和对象存在；
2. 验证对象当前状态、版本和归属；
3. 验证 Role/Binding/Run 是否有效；
4. 对路径 canonicalize 后验证范围和 symlink；
5. 验证 Harness 声明 capability；
6. 对 Human-only/高风险动作验证 Approval；
7. 默认拒绝；返回稳定拒绝码并生成脱敏 `PERMISSION_DENIED` Event。

任一层拒绝即终止；Approval 只能缩小或临时补充已允许的动作，不能突破 Team 安全上限。

## 3. 命令权限矩阵

图例：`A` 允许；`S` 限作用域/状态；`R` 仅只读；`H` Human-only；`—` 默认拒绝。

| 动作 | Owner | Leader | Product | Developer | QA | System | Harness 额外条件 |
|---|:---:|:---:|:---:|:---:|:---:|:---:|---|
| Team 创建/编辑 | A | — | — | — | — | — | 不允许 |
| Team 归档/恢复 | H | — | — | — | — | — | 不允许 |
| Role 创建/编辑/停用/恢复 | H | — | — | — | — | — | 不允许 |
| Role/Binding/权限历史读取 | A | R | R | R | R | R | 仅本 Team、脱敏 |
| Harness Instance 注册/编辑/禁用 | H | — | — | — | — | S | System 仅执行状态更新 |
| Harness 健康检查/诊断 | A | R | R | R | R | S | 仅当前/授权实例；无凭据值 |
| Binding 绑定/解绑/换绑 | H | — | — | — | — | — | Harness 绝不可自换绑 |
| CredentialReference 创建/选择/删除 | H | — | — | — | — | S | System 只可按 Run 读取引用对应秘密 |
| Workspace 绑定/更换 | H | — | — | — | — | — | 不允许 |
| Task 创建 | A | A | S | S | S | S | Role 模板和 Team 策略允许；owner 必须合法 |
| Task 分配/改派 | A | A | S | — | — | S | System 只按显式策略/依赖执行，不做 P0 智能派单 |
| Task 编辑轻量字段 | A | A | S | S | S | S | owner Role 只能改自身允许字段；版本匹配 |
| Task 设置依赖/软依赖 | A/H | A/S | S | — | S | S | 软依赖覆盖原因必填；Human 可越过硬依赖 |
| Task claim | A | S | S | S | S | S | 当前 Binding、owner Role、READY、权限、lease/fencing |
| Task block/submit review | A | S | S | S | S | S | 仅 owner/关联 Run；满足状态前置条件 |
| Task cancel/reopen | H | S | — | — | — | — | 活动 Run 必须先 cancel/fence |
| Run start/heartbeat/finish | S | S | S | S | S | A | 仅关联 Task/Binding/token；Harness 不能指定任意 actor |
| Run retry | A | S | S | S | S | S | 只对有权 Task，且旧 Run 终态/已 fence |
| Run cancel | H | S | — | S | S | S | Role 仅取消自己拥有 Task 的 Run；强制取消为 Human-only |
| Human takeover | H | — | — | — | — | — | 必须撤销旧写入权并记 Human actor |
| Review 提交 | A | S | S | S | S | S | 至少一个 Result Artifact |
| Review 通过/退回 | A | S | S | — | A/S | S | Reviewer 不能审批自己未获授权的高风险结果；意见/验收完整 |
| Workspace 读取共享资料 | A | R | R | R | R | S | 当前 Team、Reference/启动包范围 |
| Workspace 写 plans/specs/decisions | A | S | S | — | S | — | 按 Role path policy |
| Workspace 写代码任务区 | A | S | — | S | R/S | S | Task 授权路径 + 有效 fencing；QA 默认只读代码 |
| Workspace 写生产/凭据区 | H | — | — | — | — | — | P0 默认不允许，即使有 capability |
| Artifact 创建/关联 | A | S | S | S | S | S | creator/Run/路径/版本校验 |
| Message 发送/回复/确认 | A | S | S | S | S | S | to_role_id、Team、Task/Reference 可见性校验 |
| Message retry | A | S | S | S | S | S | sender/target 授权、TTL 和错误可重试 |
| Message reassign | H | S | — | — | — | — | 只有被显式授权的 Leader 可改派；保留历史 |
| NotificationPolicy 修改 | H | S | — | — | — | — | Leader 仅 Team 允许范围；不能屏蔽强制安全通知 |
| Approval 请求 | A | S | S | S | S | S | 请求不等于授权；scope/理由/期限必填 |
| Approval 批准/拒绝 | H | — | — | — | — | — | P0 仅 Human；禁止 requester 自批 |
| Event/Activity 读取 | A | R | R | R | R | S | 只显示可见对象，metadata 脱敏 |
| 审计导出 | H | — | — | — | — | S | 后台任务由 System 执行，结果仅 Owner 可取 |
| 部署/付费/生产数据/外发消息 | H | — | — | — | — | — | P0 不实现；即使批准也需未来专门能力 |

`S` 的精确范围来自 Role 模板、Task 授权、Workspace path policy 和命令状态前置条件，不能由前端自行解释。

## 4. 默认 Role 路径模板

| Role | 默认读 | 默认写 | 默认禁止 |
|---|---|---|---|
| Leader | 全部共享资料、获授权代码与产物 | `plans/`、`specs/`、`tasks/`、`questions/`、`decisions/`；按策略写代码 | 凭据存储、Team 安全配置、生产资源 |
| Product | 共享资料、任务和产物 | `plans/`、`specs/`、`questions/`、`decisions/` | 生产代码、部署、凭据 |
| Frontend | 共享计划/规范、任务引用、授权代码 | 自己 Task worktree 的前端授权路径、`tasks/`、`questions/` | 后端/基础设施未授权路径、Team 设置、凭据 |
| Backend | 共享计划/规范、任务引用、授权代码 | 自己 Task worktree 的后端授权路径、`tasks/`、`questions/` | 前端/基础设施未授权路径、Team 设置、凭据 |
| QA/Reviewer | 共享资料、代码、Artifact | 测试路径、评审报告、`tasks/`、`questions/` | 默认不改被评审实现、Binding、凭据 |

目录名只是默认模板；真实检查使用 canonical path 和 Workspace 根的相对范围。隐藏文件、symlink 与 Git 子模块不自动继承宽权限。

## 5. Human-only 动作冻结表

以下动作 P0 不能委托给 Role/Harness：

1. Team 归档/恢复；
2. Role 创建、停用、恢复和权限变更；
3. Harness 注册、凭据引用管理、Binding 绑定/解绑/换绑；
4. 强制取消他人 Run、Human takeover、绕过硬依赖；
5. 批准/拒绝高风险 Approval；
6. Workspace 根更换、跨 Team 分享、审计导出；
7. 删除/清理关键历史；
8. 部署、发布、付费、生产数据修改、生产凭据访问、对外消息发送。

“Leader”是 AI Role 时不视为 Human；岗位名不能提升权限。

## 6. Approval Grant 约束

Approval 必含：

- `requested_action`：单一稳定动作码；
- `resource_type/resource_id`：明确对象；
- `scope`：路径、环境、额度或操作参数上限；
- `requested_by`、`decided_by`；
- `reason`、`expires_at`；
- 可选 `task_id/run_id`，默认绑定当前 Run；
- 单次/次数上限，P0 默认单次。

每次使用时验证未过期、未消费、actor/context 匹配。Approval 不写回 Role Policy，不在换绑后自动转给新 Harness，除非 Human 明确批准新的 Binding scope。

## 7. 拒绝与审计

- 权限拒绝返回 `AUTH_PERMISSION_DENIED`；Human-only 返回 `AUTH_HUMAN_REQUIRED`；越界路径返回 `WORKSPACE_PATH_OUTSIDE_SCOPE`。
- 拒绝 Event 保存 actor、动作、资源、策略版本、request ID 和脱敏原因；不保存秘密、完整环境变量或未授权内容。
- 查询接口也执行对象可见性过滤；“知道 ID”不等于有读取权。
- 缓存的 allow 结果不能跨对象版本、Binding version、Approval expiry 或 fencing token 复用。

## 8. DEV-00-05 审核结论

每类 P0 命令均已指定主体、资源范围、状态/路径上下文和拒绝语义；Human-only 动作封闭且 Harness 无自提权、自换绑、自审批路径。

结论：**PASS**。
