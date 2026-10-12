# AI Team Workspace 项目执行规则

- 开始任何实现任务前，依次完整阅读：
  1. `docs/development/README.md`
  2. `docs/development/TECH_STACK_AND_STANDARDS.md`
  3. `docs/development/DEVELOPMENT_NODES.md`
  4. `docs/development/QUALITY_GATES.md`
- 技术栈、领域不变量、协议边界和质量门禁已经冻结。变更必须新增或替代 ADR，并获得 Human 明确确认；普通实现任务不得隐式改栈。
- 每个开发任务必须关联一个 `DEV-*` 节点。节点表中的模型列保留为历史复杂度参考；实际模型分配按下方“主控与 Subagent 模型路由规则”执行。
- 进入具体节点前，先阅读 `docs/specs/README.md`，再完整阅读该节点关联的冻结规格；实现不得自行补造状态、权限、错误码或协议语义。
- 节点必须同时通过专属审核标准和通用质量门禁。未通过时保持 `BLOCKED` 或 `REVIEW`，不得声明完成或启动依赖节点。
- Figma 是体验与视觉依据，不是领域状态机、权限或真实完成状态的事实源；禁止用静态跳转、前端本地状态或假数据替代后端规则。
- `sources/` 下文件保持只读；其他 PRD、Figma 索引与审计材料视为需求依据，冲突时按开发基线的事实优先级处理。
- 未经 Human 明确授权，不提交、推送、部署、删除大量文件、访问生产凭据或操作生产数据。

## 主控与 Subagent 模型路由规则

- 总控会话必须使用 `gpt-5.6-sol` 或能力更高的模型。总控负责读取冻结基线、判断难度、维护节点依赖、分配文件归属、串行整合根 manifest/lockfile、复核证据以及给出最终审核结论。
- Codex 内部委派任务默认使用 Subagent，由总控在 `gpt-5.6-luna` 与 `gpt-5.6-terra` 之间选择；默认不得创建 `gpt-5.6-sol` 或更高模型的 Subagent。Human 明确指定的外部模型协作按 `docs/coordination/README.md` 通过 Markdown 任务包交接，不假定 Codex 可直接调用外部模型。
- 使用 `gpt-5.6-luna`：范围清晰、低风险、单文件或机械性修改、资料检索、简单测试补充、格式/文档同步、已有模式下的直接实现。
- 使用 `gpt-5.6-terra`：普通工程实现、边界明确的多文件修改、测试设计、故障诊断、安全/兼容性检查、数据库或协议实现中的已冻结语义落地、独立代码审核。
- 若任务复杂度超过单个 Terra Subagent 的安全承载范围，总控必须先拆成多个边界明确、写集互斥的 Luna/Terra 子任务；无法安全拆分时保持 `REVIEW` 或 `BLOCKED` 并请求 Human 决策，不得自行升级 Subagent 模型。
- 每轮最多并发 3 个 Subagent。禁止多个 Subagent 同时修改同一文件；根 manifest、lockfile、跨节点冲突整合及最终门禁由总控串行处理。
- 总控在分派时必须写明关联的 `DEV-*` 节点、选择 Luna/Terra 的难度理由、允许修改的文件范围和验收命令；节点审核记录应保留实际模型与分工证据。
- 本规则只改变模型路由与协作方式，不降低节点依赖、冻结规格、专属审核标准或通用质量门禁。
- Human 于 2026-09-24 接受 `docs/adr/0004-cross-model-r3-routing.md`：Gemini-3.8-Flash 可按总控签发的任务包主做其适用的旧 Wave 2 节点，MiniMax-M3 在该范围仅承担边角辅助。Gemini 阶段自验收不替代独立复核、正式节点 `DONE` 或总控最终验收。现行入口为 `docs/README.md`、`docs/coordination/README.md`；旧交接快照见 `docs/archive/README.md`。V2 新阶段的分工是待 R1 冻结的计划，不自动改变本 ADR。
- Human 于2026-10-08明确要求由 Minimaxcode 接续本轮审计后的整改及后续开发，自己转发总控任务表；最新路由见 `docs/adr/0008-minimaxcode-remediation-routing.md`。仅替代本轮接续任务主责模型偏好，不追溯历史，不放宽栈、语义、节点依赖、门禁或费用权限。Minimaxcode实际模型/版本由执行端回填；Codex继续独立复核和最终准入，ADR-0007仍待Human决定。

- Human 于2026-10-09明确暂停向 Minimaxcode 外派本轮整改，改由 Codex 内部调整、审计并收口；当前执行见 `docs/coordination/CODEX-AUDIT-CLOSURE-PLAN-2026-10-09.md`。本轮暂停覆盖 ADR-0008 的继续派工安排，不追溯历史，不改冻结语义、节点依赖和费用权限；内部模型仍按本文件 Luna/Terra 路由。
