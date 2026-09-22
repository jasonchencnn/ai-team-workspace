# DEV-00 冻结规格索引

> 状态：Frozen  
> 变更方式：需求语义变更先更新追踪矩阵；架构/不变量/协议破坏性变更必须新增 ADR。

| 规格 | 适用节点 | 主要约束 |
|---|---|---|
| `REQUIREMENTS_TRACEABILITY.md` | 全部 | FR/AC/审计/Figma 与唯一 owner，不得出现孤儿需求 |
| `DOMAIN_MODEL.md` | DEV-02、04、05、07 | 术语、聚合、事实归属和唯一写入者 |
| `STATE_MACHINES.md` | DEV-02、03、05、06、07 | Task/Run/Binding/Message/Notification/Approval 迁移 |
| `PERMISSION_MATRIX.md` | DEV-02、03、04、05、06、07 | 有效权限求交、Human-only、路径与审批 |
| `DATA_CONVENTIONS.md` | DEV-01、02、03、04、05、08 | ID、UTC、version、幂等、归档、留存和审计 hash |
| `ERROR_CATALOG.md` | DEV-01、02、03、04、05、06 | 稳定错误码、用户提示、retryable 和 details 白名单 |
| `adapter/v0/PROTOCOL.md` | DEV-03、05、07 | JSON-RPC stdio、方法、通知、兼容、排序、取消和恢复 |
| `adapter/v0/*.schema.json` | DEV-01、03 | Manifest/消息机器契约；生成类型和契约测试的来源 |
| `INFORMATION_ARCHITECTURE.md` | DEV-06、07 | 正式导航、36 画板映射、断点、页面状态和 A11y |

## 使用规则

1. 节点开始前读取本索引、对应规格和节点审核标准。
2. 若规格之间冲突，按 `docs/development/README.md` 的事实优先级处理并暂停实现，不能自行选择。
3. 添加错误码、状态或 Adapter 字段时同步修改追踪、规格、Schema、测试与审核证据。
4. Figma 与状态机冲突时以 PRD/冻结状态机为准，UI 调整登记设计偏差。
5. 全部规格使用英文稳定标识与中文解释；代码不得依赖中文显示文案作为状态或权限判断。
