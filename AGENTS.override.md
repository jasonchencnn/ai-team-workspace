# AI Team Workspace 项目执行规则

- 开始任何实现任务前，依次完整阅读：
  1. `docs/development/README.md`
  2. `docs/development/TECH_STACK_AND_STANDARDS.md`
  3. `docs/development/DEVELOPMENT_NODES.md`
  4. `docs/development/QUALITY_GATES.md`
- 技术栈、领域不变量、协议边界和质量门禁已经冻结。变更必须新增或替代 ADR，并获得 Human 明确确认；普通实现任务不得隐式改栈。
- 每个开发任务必须关联一个 `DEV-*` 节点，并使用节点表指定模型：复杂任务 `gpt-5.6-sol`、普通任务 `gpt-5.6-terra`、简单任务 `gpt-5.6-luna`。
- 进入具体节点前，先阅读 `docs/specs/README.md`，再完整阅读该节点关联的冻结规格；实现不得自行补造状态、权限、错误码或协议语义。
- 节点必须同时通过专属审核标准和通用质量门禁。未通过时保持 `BLOCKED` 或 `REVIEW`，不得声明完成或启动依赖节点。
- Figma 是体验与视觉依据，不是领域状态机、权限或真实完成状态的事实源；禁止用静态跳转、前端本地状态或假数据替代后端规则。
- `sources/` 下文件保持只读；其他 PRD、Figma 索引与审计材料视为需求依据，冲突时按开发基线的事实优先级处理。
- 未经 Human 明确授权，不提交、推送、部署、删除大量文件、访问生产凭据或操作生产数据。
