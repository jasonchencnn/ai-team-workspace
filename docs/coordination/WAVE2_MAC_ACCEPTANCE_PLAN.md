# Wave 2 macOS 正式验收准备方案

> 状态：**首轮正式验收已执行，结论未通过**；日期：2026-09-28。适用 DEV-02-02～10 与 DEV-03-02～05；结果见[验收记录](../reviews/WAVE-2-MAC-ACCEPTANCE.md)。依据[节点表](../development/DEVELOPMENT_NODES.md)、[质量门禁](../development/QUALITY_GATES.md)、[冻结规格](../specs/README.md)、[ADR-0004](../adr/0004-cross-model-r3-routing.md)、[ADR-0005](../adr/0005-macos-first-quality-and-release-gates.md)和[冻结清单](../v2/TASK_FREEZE.md)。不替代节点专项标准，也不提前判定任何节点 `DONE`。

## 执行与验收职责

| 职责 | Harness / 模型 | 交付与边界 |
|---|---|---|
| 验收总控及最终判定 | Codex `gpt-5.6-sol` 或更高 | 冻结输入、核依赖和规格、签发互斥任务包、逐差异复核 R3、串行整合根 manifest/lockfile、最后逐节点签署 `DONE/REVIEW/BLOCKED`；不能用下游已有代码倒推前置节点 `DONE`。 |
| 证据采集与定向复现 | Codex `gpt-5.6-luna` 可做机械采集；复杂测试由 `gpt-5.6-terra` | 记录环境、命令、退出码、失败日志及输入哈希；采集者不签独立审核结论。Mac 21 门禁由总控控制最终输入、在本机串行运行或现场监督。 |
| 独立技术与证据审核 | 未参与被审实现的 Codex `gpt-5.6-terra` | 对 migration 跨 Team 约束、DAG 多跳环、Adapter 进程/协议及至少一种 R3 失败注入做反例审核；逐项核 Mac 21 门禁、全 refs 扫描、工具来源、哈希与结果，出具独立 PASS/阻断清单。不得由 Gemini 自验收替代。 |
| 原实现者说明与必要修复 | 旧 Wave 2 的 Gemini-3.8-Flash；MiniMax-M3 仅互斥低风险辅助 | 解释既有实现和自验收；仅在总控针对已准入节点签发新任务包、精确写集及验收命令后修复。原实现者不得签独立验收或正式 `DONE`。 |
| Human Owner | 无模型 | 只对需要改变冻结语义/ADR、外部账号或费用、发布等事项作决定；不替代工程独立审核。 |

首轮实际模型、职责、输入、证据与状态已填入[验收记录](../reviews/WAVE-2-MAC-ACCEPTANCE.md)；后续修复尚未派工。Antigravity 与 V2 MiniMax 后端候选不参与这批旧 Wave 2 主验收。全批新增费用上限 `$0`。

## 顺序、交付件和出口

| 批次 | 节点与前置条件 | 执行工作与交付文件 | 独立验收标准 |
|---|---|---|---|
| A：入口 | 启动时 DEV-02-02、DEV-03-02 均为 `REVIEW`；首轮后分别为 `REVIEW`、`BLOCKED` | 总控登记 HEAD、全部未提交/未跟踪输入、适用规格及历史审核；Terra 复核两节点反例和新输入差异；在稳定输入上完成完整 Mac 门禁和历史扫描。首轮结果已写入 `docs/reviews/WAVE-2-MAC-ACCEPTANCE.md`；后续节点补充证据另建带日期的附录，不改旧结论。 | Team/Role/Workspace 迁移和进程关闭/超时反例在真实 Mac 环境可重放；无未关闭 S0/S1；专项标准、G0～G9、Mac 证据包及独立审核均通过后，总控才可决定这两个前置节点 `DONE`。首轮未满足。 |
| B：数据基础 | A 中 DEV-02-02 正式 `DONE` 后，审 DEV-02-03、04、07、08、10；DEV-02-05 还依赖 04 | 核对既有 migration 与测试；先构造跨 Team Binding、Task owner、Run 引用，直接环及三跳以上依赖环等失败用例；必要修复用新的前向 migration，不改写已发布 migration。交付各 `DEV-*-REVIEW` 的新增验收附录和可复现测试。 | 每节点依赖先 `DONE`；唯一主 Binding、Team 隔离、Task/Run/fencing、Outbox 原子性、Message 投递快照、Human-only 权限符合节点专标；真实 SQLite 反例拒绝非法写入且合法写入不回归。 |
| C：数据汇合 | DEV-02-05 正式 `DONE` 后审 02-06；02-07/08 正式 `DONE` 后审 02-09 | 核 Artifact/Reference 追溯与 Notification/Policy/Attention 关联；输出节点附录与回归证据。 | Run/Role/版本/基线可追溯；Human Notification 不冒充 Role Message；事件与待办无丢失、重复或权限越界。 |
| D：Adapter 链 | A 中 DEV-03-02 正式 `DONE` 后，依次审 03-03 → 03-04 → 03-05 | 复核 initialize/能力/健康、Mock 正常流和故障注入；交付协议与进程负向测试、节点附录。 | 不兼容/离线/未认证有稳定错误；乱序、重复、超时、取消拒绝、晚到回调及失联有可重放证据；无 shell 拼接、无无界阻塞；获准 Adapter 子进程按 ADR-0006 逐入口验证。Mock 通过不冒充真实 Harness Spike。 |
| E：最终输入与批次关闭 | B～D 已完成必要修复和节点定向测试；任何依赖未 `DONE` 的节点仍不能关闭 | 若 A 后输入发生变化，总控先重跑受影响定向测试与完整 Mac 门禁、全 refs Gitleaks 历史扫描，复核已关闭前置节点在新输入上的证据；Terra 独立核对；总控写 `docs/reviews/WAVE-2-MAC-ACCEPTANCE.md` 与逐节点状态决定。 | 所有适用节点专标及 G0～G9、R3 反例/失败注入均 PASS；最终输入 21/21、输入前后总 SHA-256 一致、全 refs 扫描、工具版本/来源、证据文件哈希可复核；任何失败保持 `REVIEW/BLOCKED`，不放行依赖。 |

批次 B、C、D 的**只读检查可以并行准备**，正式节点晋级严格按表中依赖串行。任一节点关闭时必须有其所依据输入的完整 Mac 门禁；若 B～D 期间输入不变，可复用 A 的同哈希证据，若变化则在关闭相应节点前重跑并复核已关闭节点受影响范围。现有 `SELF_CHECK_PASS` 只能作为待审资料，不能替代上述交付件。

## Mac 证据包操作边界

最终输入稳定后，使用脚本支持的命令：

```bash
node scripts/ci/local-gates.mjs --platform macos --evidence /绝对路径/wave2-macos-arm64.json
```

证据路径设在仓库外的持久、安全位置，执行前创建目录并确保文件不存在；脚本以独占方式写证据 JSON。运行前核对非 shallow Git 仓库、HEAD、`git status`、全部 refs 和输入清单；另行对全部 refs 运行 `gitleaks git --log-opts='--all'`，保存配置、范围、退出码、脱敏日志及哈希。脚本本身**不**做历史扫描、工具来源验证或证据 JSON 自身校验和，必须在批次 E 的证据包中补齐。记录 macOS 版本、ARM64、Node/pnpm/Rust/Gitleaks 等固定版本与来源；`result=pass`、21 个退出码为零且输入前后哈希相同才算工程门禁通过。若修复、规格、依赖锁文件或门禁输入变化，旧证据失效，重跑对应定向检查与完整 Mac 门禁。

Linux、Oracle、Docker 不在本轮运行和阻断范围。这里的 Wave 2 工程验收**不等于** DEV-07/08 产品闭环、macOS 14+ 最低支持版本、真实桌面 E2E 或发布批准；这些仍由 Gate I 单独验收。

## 停止与回退

出现未获接受的状态/权限/协议语义变更、跨 Team 越权、数据不可恢复、审计失真、进程无界阻塞、S0/S1、依赖未 `DONE`、任务包写集冲突或证据缺口时，停止相应节点晋级并记录 `REVIEW/BLOCKED`；总控按冻结规格与 ADR 决定修复范围。不得修改旧 migration、清理用户工作区、关闭失败测试或以 Ubuntu 历史结果补 Mac 证据。必要修复须另签单节点任务包、保留前后差异及回退方法，随后重新独立审核。首轮无批次 PASS 结论。
