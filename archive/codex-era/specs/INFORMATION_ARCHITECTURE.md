# 正式信息架构、Figma 映射与响应式规范

> 对应节点：DEV-00-09  
> 状态：Frozen  
> Figma 文件：`C1roCB6shYmzP6oEu5ZAs4`，36 个顶层画板

## 1. 导航原则

- 全局导航回答“我需要处理什么、有哪些团队、Harness 是否可用、全局如何配置”。
- Team 导航回答“这个团队由谁工作、做什么、事实在哪里、发生了什么”。
- Task Detail 是工作控制页，不只是展示页。
- Role 是负责人，Harness 始终作为“当前绑定”附属显示。
- UI route 不是权限边界；所有读取和命令仍由控制面检查。
- Modal/Sheet 关闭后返回触发点焦点；结果页不以静态演示状态冒充持久化结果。

## 2. 正式导航树

```text
全局
├── 我的工作台 /workbench
│   ├── 待我处理 /workbench/attention
│   ├── 当前运行 /workbench/runs
│   ├── 异常与阻塞 /workbench/incidents
│   └── 最近活动 /workbench/activity
├── 团队 /teams
│   ├── 新建团队 /teams/new
│   └── Team /teams/:teamId
│       ├── 概览 /overview
│       ├── 任务 /tasks
│       │   ├── 看板 /board
│       │   ├── 列表 /list
│       │   ├── 依赖 /dependencies
│       │   ├── 执行记录 /runs
│       │   └── Task /:taskId
│       ├── 工作空间 /workspace
│       │   ├── 文件 /files
│       │   ├── Artifact /artifacts
│       │   └── 最近变更 /changes
│       ├── 岗位通信 /messages
│       ├── 活动与审计 /activity
│       ├── 团队成员 /members
│       │   ├── Role /roles
│       │   └── Binding 历史 /bindings
│       └── 设置 /settings
│           ├── 权限与审批 /permissions
│           ├── Harness/Adapter /harnesses
│           ├── 通知策略 /notifications
│           └── 安全与审计 /security
├── Harness 连接 /harnesses
└── 全局设置 /settings
```

“依赖”和“执行记录”在桌面宽屏可作为任务二级导航；紧凑/窄屏收纳到任务页 Tab/菜单，但 route 不改变。

## 3. 36 个 Figma 画板映射

| Node | 画板 | 正式 route/Surface | 实现归属 |
|---|---|---|---|
| `4:291` | 团队空间 | `/teams/:teamId/overview` Page | DEV-06-05 |
| `4:336` | 任务面板 | `/teams/:teamId/tasks/board` Page | DEV-06-06 |
| `4:369` | 实现 Login UI | `/teams/:teamId/tasks/:taskId` Page | DEV-06-08 |
| `4:402` | 岗位与绑定 | `/teams/:teamId/members/roles` Page | DEV-06-12 |
| `4:435` | 共享文件 | `/teams/:teamId/workspace/files` Page | DEV-06-11 |
| `4:468` | 待我处理 | `/workbench/attention` Page | DEV-06-15 |
| `6:403` | 更换 Harness | Role 页上的 Change Binding Dialog | DEV-06-12 |
| `6:404` | 换绑待生效 | Role 页 Binding 状态 Panel | DEV-06-12 |
| `6:405` | 移动端·待我处理 | `/workbench/attention` Narrow layout | DEV-06-17 |
| `8:311` | 决策已记录 | Attention 决策操作 Result state | DEV-06-15 |
| `8:312` | 人工接管确认 | Task/Run Takeover Dialog | DEV-06-09 |
| `8:313` | 已进入人工接管 | Task/Run Takeover Result state | DEV-06-09 |
| `8:314` | 任务面板·空状态 | `/tasks/board` Empty state | DEV-06-06 |
| `8:315` | 设计规范与交互索引 | Storybook/开发规范，不进入产品导航 | DEV-06-01～02 |
| `8:421` | 创建任务 | `/tasks/new` 或 Task Create Dialog | DEV-06-07 |
| `8:422` | 任务已创建 | Task Create Result/Redirect | DEV-06-07 |
| `13:353` | 创建团队与工作空间 | `/teams/new` Wizard | DEV-06-04 |
| `13:354` | Harness 与 Adapter | `/harnesses` 和 Team settings/harnesses | DEV-06-13 |
| `13:355` | FE-023·任务控制 | Task Detail Control tab | DEV-06-08 |
| `13:356` | 岗位通信 | `/teams/:teamId/messages` Page | DEV-06-14 |
| `13:357` | 活动与审计 | `/teams/:teamId/activity` Page | DEV-06-16 |
| `13:358` | 创建任务·完整表单 | Task Create Page/Dialog form | DEV-06-07 |
| `13:359` | 验收与退回 | Task Review Drawer/Page | DEV-06-10 |
| `13:360` | Run 已失败 | Task Run Detail Failure state | DEV-06-09 |
| `13:361` | 依赖与冲突 | `/tasks/dependencies` + Conflict Drawer | DEV-06-06/11 |
| `13:362` | 高风险操作审批 | Attention Approval Detail | DEV-06-15 |
| `13:363` | Run 状态未知 | Task Run Reconcile state | DEV-06-09 |
| `13:364` | 发送岗位消息 | Messages Compose Dialog | DEV-06-14 |
| `13:365` | 消息需要处理 | Attention/Message Failure Detail | DEV-06-14/15 |
| `29:731` | 状态变更通知 | Notification Detail Drawer | DEV-06-15 |
| `29:732` | QA 回复 Leader | Thread Reply Composer | DEV-06-14 |
| `29:733` | 回复已送达 | Thread Delivery Result state | DEV-06-14 |
| `45:806` | 通知订阅与投递策略 | `/settings/notifications` Page | DEV-06-15 |
| `46:815` | 换绑期间待投递消息 | Binding Change Message Impact Drawer | DEV-06-12/14 |
| `59:839` | Harness 注册与健康检查 | `/harnesses/new` / Instance Detail | DEV-06-13 |
| `64:848` | 消息投递终态 | Message Delivery Detail | DEV-06-14 |

全部画板均有唯一正式归属。`64:848` 已知的 3 处文字裁切在 DEV-06-14 实现中修复，不复制该缺陷。

## 4. 页面级信息责任

| 页面 | 必须回答 | 不承担 |
|---|---|---|
| 我的工作台 | Human 现在需要处理什么、影响和下一步 | 完整团队管理 |
| Team 概览 | Role 状态、当前绑定、任务健康、待处理、最近活动 | 详细 Run 日志 |
| Task 看板/列表 | 工作状态、owner Role、依赖、主要引用、当前 Run | 需求全文 |
| Task 详情 | 状态控制、引用、依赖、Run、Artifact、Review、Activity | 隐藏 Harness 对话 |
| Workspace | 文件事实、版本/Git 状态、引用关系 | Task 状态的权威写入 |
| Role/Binding | 稳定岗位、权限摘要、当前/历史 Binding、换绑影响 | Task owner 改成 Harness |
| Harness 连接 | Instance、Adapter、能力、健康、凭据引用 | 展示秘密值 |
| 岗位通信 | Role-to-Role Thread、Task/Reference、投递审计 | Human Notification 收件箱 |
| 活动与审计 | 已发生 Event 的过滤、关联、回放、导出 | 可回复业务消息 |
| 设置 | 权限、通知、安全上限 | 绕过 Human-only |

## 5. 响应式断点

| 模式 | 宽度 | 布局规则 |
|---|---:|---|
| Wide | `≥1280px` | 216px 固定左导航；主内容最大宽度按页面；Task/Workspace 可双栏。 |
| Compact | `768–1279px` | 左导航折叠为图标/可展开；详情副栏转 Drawer；表格允许列优先级收纳。 |
| Narrow | `360–767px` | 单列、顶栏+导航 Drawer；详情为全屏 route；主操作固定但不遮挡内容。 |

最小支持 viewport 为 `360×640`。不要求所有桌面密集视图在窄屏保留同等信息密度，但以下闭环必须完成：处理 Approval、失败/UNKNOWN Run、Message 异常、Task Review、Human takeover。

## 6. Token 冻结

| Token | 值/规则 |
|---|---|
| Canvas | `#F5F6F8` |
| Surface | `#FFFFFF` |
| Ink | `#20242D` |
| Muted | `#626978` |
| Line | `#E1E5EC` |
| Brand/Selected | `#465BD8` |
| Control/Panel radius | `8px / 12px` |
| Pill radius | `999px` |
| Desktop content padding | `32px` |
| Section gap | `24px` |
| Type | Page 28、Section 18、Body 14、Caption 12；正文 line-height ≥1.5 |
| Font | `Noto Sans SC` 优先，系统 sans-serif 回退 |

状态色由实现节点补齐对比度验证；执行中/待处理/完成/错误均必须配文字或图标，禁止只靠颜色。

## 7. 通用页面状态

每个数据页面至少定义：

- Initial loading：骨架，保持布局稳定；
- Empty：说明为什么为空和唯一主行动；
- Partial data：保留已加载内容，局部错误就地重试；
- Offline/disconnected：展示最后更新时间，禁止误显示“实时”；
- Permission denied：说明不可执行及授权入口，不泄露不可见对象；
- Version conflict：刷新并对比，保留未提交输入；
- Stale/broken reference：修复、重选或返回 Task；
- Fatal error：Error Boundary + correlation ID + 安全重试。

## 8. 对话框与焦点

- 危险操作使用明确影响说明和二次确认；文本确认只用于高破坏性动作。
- Dialog 必须焦点锁定、Esc 规则明确、关闭后返回触发元素。
- 异步提交期间禁用重复提交，但取消/关闭策略必须可解释。
- 成功后以控制面返回对象/版本更新页面；不跳到预制“成功画板”假装持久化。
- 错误就地显示且保留输入；field error 绑定具体控件，领域错误显示在操作区。

## 9. 可访问性基线

- WCAG 2.2 AA；键盘顺序与视觉顺序一致。
- 状态变化使用 live region，但高频 heartbeat 不逐条朗读。
- Task、Run、Role、Harness 的可访问名称必须包含概念类型，避免只读出品牌名。
- `prefers-reduced-motion` 下取消非必要动画。
- 200% 缩放和窄屏下关键操作仍可达；不产生水平+垂直双滚动陷阱。

## 10. DEV-00-09 审核结论

36 个画板均映射到正式 route/surface 和实现节点；Harness、活动、依赖、执行记录、通知与设置不再依赖隐藏入口；Wide/Compact/Narrow 断点和关键窄屏闭环已冻结。

结论：**PASS**。
