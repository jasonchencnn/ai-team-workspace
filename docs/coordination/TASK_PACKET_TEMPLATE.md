# 单节点 Markdown 任务包模板

> Codex 总控签发；空字段不授权，未列文件只读。先核对[当前状态](README.md)、[阶段分工](../v2/WORKSTREAMS_AND_ACCEPTANCE.md)、对应 `DEV-*` 依赖与已接受 ADR。此模板可用于旧 Wave 2 收口和未来获准的 V2 节点，实际模型不能从候选表自动推断。

> 当前[全项目任务冻结](../v2/TASK_FREEZE.md)；本模板本身不授权执行。Wave 2 正式验收的执行/独立审核分离按[准备方案](WAVE2_MAC_ACCEPTANCE_PLAN.md)填写，待逐节点解冻后方可签发。

```markdown
# <DEV-* 节点>｜<任务标题>

- 阶段：R0-B / R1 / R2 / R3 / R4 / R5 / R6
- 状态与前置正式 DONE 证据：
- 派工时间：YYYY-MM-DD HH:mm CST（Asia/Shanghai）
- 发起者：Codex 总控；Human 已接受的 ADR/授权引用：
- 主责 Harness 与实际模型/版本：
- 协作者 Harness 与实际模型/版本（可选）：
- 独立审核者 Harness 与实际模型/版本：
- 模型选择及 R0～R3 难度理由：
- 当前额度/费用条件：新增费用 $0；外部账号/连接/上传是否另获确认：
- 输入仓库、分支、HEAD、未提交/未跟踪文件与输入哈希：
- 必读：AGENTS.md → AGENTS.override.md → 四份开发基线 → docs/specs/README.md → 本节点全部冻结规格 → 已接受 ADR → 当前协作入口
- 关联需求/AC、Figma 节点、冻结规格与节点审核标准：
- 目标与非目标：
- 主责允许修改的精确文件/目录：
- 协作者允许修改的互斥精确文件/目录（无则只读）：
- 禁止修改：未列文件、sources/、根 manifest/lockfile（除非总控串行整合）；不得提交/推送/部署
- 交付文件及预期内容：
- 必须先复现的失败测试、权限/迁移/协议负向反例：
- 定向验收命令、预期退出码与阶段 Gate：
- 数据迁移、兼容与回退方式：
- 停止条件：依赖未 DONE、规格冲突、新状态/权限/错误码/协议字段、S0/S1、写集冲突、模型不明或费用/外部权限影响时停止并上报

## 回传结果

- 实际修改文件/交付件：
- 命令、退出码、RED/GREEN、负向证据：
- 输入快照及当前 Mac 门禁证据（其他平台仅在对应 ADR 准入后填写；未验证须明说）：
- 专项审核及 G0～G9 证据路径：
- 主责自验收：SELF_CHECK_PASS / SELF_CHECK_BLOCKED（不等于正式 DONE）
- 独立审核结论及审核者：
- 遗留风险与回退：
- 建议状态：REVIEW / BLOCKED；仅总控按门禁决定 DONE
```

旧 Wave 2 使用本模板时仍须按 [ADR-0004](../adr/0004-cross-model-r3-routing.md) 填写 Gemini-3.8-Flash 主责、MiniMax-M3 边角辅助的适用范围；不能借模板扩大到尚未批准的 V2 实现。外部会话若不共享工作区，只回传 Markdown 结果与补丁/差异说明，由总控核验后整合。
