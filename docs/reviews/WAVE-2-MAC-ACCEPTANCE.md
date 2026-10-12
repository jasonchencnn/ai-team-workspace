# Wave 2 macOS 正式验收记录：首轮阻断

> 日期：2026-09-28；结论：**Wave 2 未通过，不得放行依赖节点**。依据 [验收方案](../coordination/WAVE2_MAC_ACCEPTANCE_PLAN.md)、[ADR-0005](../adr/0005-macos-first-quality-and-release-gates.md)和 [G0～G9](../development/QUALITY_GATES.md)。本记录保留历史 `SELF_CHECK_PASS` 原件，不改写旧审核结论。证据目录：[wave2-mac-2026-09-28](evidence/wave2-mac-2026-09-28/)。

## 审核分工与输入

- 总控与正式状态判定：Codex 当前 GPT-6 会话；精确子型号未由本机工具输出。负责冻结输入、执行 Mac 门禁、核依赖和签署结论。
- SQLite 独立反例审核：Codex Subagent `gpt-5.6-terra`，`/root/wave2_sqlite_independent`；只读源码，以本机真实 SQLite 运行反例，无仓库写集。
- Adapter 独立反例审核：另一 Codex Subagent `gpt-5.6-terra`，`/root/wave2_adapter_independent`；只读源码，以仓库外临时工程离线复现，无仓库写集。两位审核者均未参与被审实现，原 Gemini 自验收不参与独立结论。
- Mac 证据独立复核：第三位 Codex Subagent `gpt-5.6-terra`，`/root/wave2_evidence_independent`；复算 JSON、manifest、命令结果及历史扫描，指出后哈希和历史扫描元数据缺口；总控据此补齐元数据并收紧表述。未签 Gate PASS。
- 输入：macOS 27.0 ARM64；HEAD `1a14b8795e76fc373bef326eb5df96f9ac173f71`；非 shallow 仓库；工作区含此前会话及归档形成的未提交、未跟踪文件。门禁开始时 205 项输入，总 SHA-256 `1b178a744a8b612c0f64bda2633f92d732f2c8c287bb1dbedc9f78049b2b455b`。失败后曾人工按同一算法复算出相同值，但仅保存哈希文本，**不能替代脚本的运行后输入校验**；脚本因失败未写 `input_manifest_verified_after_commands`。此后为保存本记录和证据所新增的文件属于**下一输入**，今后放行必须重跑完整门禁。

门禁脚本首次在命令执行前遇到三份已归档文件仍留在 Git 主索引中的路径，无法 `lstat`。为保持用户主索引及工作区不变，总控复制索引到仓库外，只在临时索引中移除这三个已删除路径，再让脚本对实际工作区执行；归档后的原件作为未跟踪输入仍纳入清单。这是透明的验收环境处理，不能把它当成脚本已修复。随后本仓库 pnpm store 缺包、只读全局 store 无写权限；总控离线复制现有全局缓存到仓库外临时目录，才完成第 1～17 项。各次失败日志及 JSON 均保存在[证据目录](evidence/wave2-mac-2026-09-28/)，临时索引和缓存位于 `/private/tmp/teamspace-wave2-acceptance.KC6JrF/`。

## Mac 门禁与安全证据

| 检查 | 实际结果 | 证据及限制 |
|---|---|---|
| `local-gates.mjs --platform macos` 21 项 | **FAIL：前 17 项退出 0；第 18 项 `pnpm audit --audit-level=high` 因 `registry.npmmirror.com` DNS 不可达退出 1；第 19～21 项未在完整脚本中执行。** | [门禁 JSON](evidence/wave2-mac-2026-09-28/wave2-macos-arm64-local-clone.json)、[完整日志](evidence/wave2-mac-2026-09-28/wave2-macos-arm64-local-clone.log)。JSON SHA-256 `6c032dc42bab95e95a3716f23f77954b6d34b04aa35f9eaeadcfb751`。不得写成 21/21。 |
| 安装、测试、构建、静态检查 | 完整脚本中的第 1～17 项通过，含本地秘密扫描、离线安装、工具链、workflow、格式、TS lint/typecheck/test/build、Rust test/build/Clippy、契约漂移和 unsafe。 | 安装使用从本机只读全局缓存复制到 `/private/tmp` 的 pnpm store；无新增包下载。单独定向 `cargo test -p persistence-sqlite --locked` 为 31/31，`cargo test -p adapter-supervisor --locked -- --test-threads=1` 为 59/59；这些既有测试没有覆盖下列反例。 |
| 全 refs Gitleaks history | **单项 PASS**：非 shallow；`git fsck --full` 退出 0（报告悬空对象，无损坏）；12 条 HEAD/refs 列表记录，扫描 4 个 commit，零命中。独立证据审核要求补录调用元数据后，已重新扫描并记录命令、时间、退出码 0、HEAD、refs/config/log/report 哈希。它不使整体 Gate PASS。 | [第二次扫描元数据](evidence/wave2-mac-2026-09-28/gitleaks-history-metadata.json)、[报告](evidence/wave2-mac-2026-09-28/gitleaks-history-verified.json)、[日志](evidence/wave2-mac-2026-09-28/gitleaks-history-verified.log)、[refs](evidence/wave2-mac-2026-09-28/refs.txt)、[工具与扫描哈希](evidence/wave2-mac-2026-09-28/tool-and-history-sha256.txt)。 |
| 脚本未完成的安全/许可证项 | `cargo audit --no-fetch`、`cargo deny ... licenses`、`pnpm run licenses:check` 单独退出 0；Rust audit 使用本机既有 advisory DB，报告 7 条允许 warning。**不能替代完整脚本第 18～21 项或最新远程公告库。** | [单独检查退出码](evidence/wave2-mac-2026-09-28/supplemental-exit-codes.txt)及相邻日志。工具路径、版本和 SHA-256 已记录；下载来源链仍待正式 PASS 时逐项核对。 |

本轮没有取得当前最终输入的 21/21 PASS、脚本前后哈希确认、Node 最新漏洞库结果和完整证据包独立 PASS。Mac Gate B/G9 因此阻断所有正式 `DONE`。

原始门禁 JSON/日志作为本机证据包含主机名与绝对工作区路径，文件保持本地且 JSON 权限为 `0600`；对外分享前须另制脱敏副本，不能把脱敏副本冒充原始哈希证据。[主机工具摘要](evidence/wave2-mac-2026-09-28/host-and-tools.txt)及[工具哈希清单](evidence/wave2-mac-2026-09-28/tool-and-history-sha256.txt)已去除用户名与设备名。

## 独立反例与节点归属

| ID / 风险 | 关联节点 | 复现及结论 | 必要关闭条件 |
|---|---|---|---|
| DB-01 / S1 | DEV-02-03～10；Team 隔离 | `0002` 的 Binding 可把 Team A 与 Team B 的 Role/Harness/Adapter 关联；`0003` 的 Task owner 可跨 Team，后续 Run/Artifact/Message 等也多为单列外键。`foreign_keys=ON` 下实际插入成功。 | 新的**前向** migration 与应用层最终执行点双重校验；逐表 INSERT/UPDATE 跨 Team 负向测试及真实升级/回退证据。 |
| DB-02 / S1 | DEV-02-04 | `0003` 仅拒自环/两节点互环；真实 SQLite 接受 A→B→C→A 三跳环。 | 事务内递归防环或等价 Domain Graph Service，覆盖三/四跳及 UPDATE 改边。 |
| DB-03 / S1 | DEV-02-05 | `0004` 接受 Task-2 的 Lease 指向 Task-1 的 Run，且 lease/run fencing token 不同。 | 校验 Task/Run/Binding/Role/Team/token 一致性，补真实 SQLite 竞争与拒绝测试。 |
| AD-01 / S1 | DEV-03-02 | supervisor 只 kill/reap 直接子进程；直接子进程退出后 detached 后代 PID 仍存活，临时脚本清理后确认消失。 | 受控进程组/树策略及有界清理测试；不能只验证继承 pipe 不阻塞。 |
| AD-02 / S1 | DEV-03-02 | `initialize` 50ms 超时返回 `Err(TimedOut)` 后，未 `drop/shutdown` 的直接子进程仍存活且 handle 可继续使用；测试后已清理。 | 超时后明确关闭/隔离/UNKNOWN 对账语义，验证 API 返回后有界状态与回收。 |
| AD-03 / 协议阻断 | DEV-03-02/03 | `RunNotificationValidator::validate_line` 接受 `observed_at="not-a-rfc3339-timestamp"`、`progress_percent=101` 并返回 `Ok(Heartbeat(...))`，违反冻结协议时间及 Schema 范围。 | 执行完整机器 Schema 或等价校验，补 UUIDv7、UTC 毫秒、字段/字节上限、重复键等负向测试。 |

DB 反例的完整 SQL、四项实际输出及校验和见 [sqlite-counterexamples.sql](evidence/wave2-mac-2026-09-28/sqlite-counterexamples.sql)和[运行日志](evidence/wave2-mac-2026-09-28/sqlite-counterexamples.log)；结果为 `cross_team_binding=1`、`cross_team_task_owner=1`、`three_hop_cycle_edges=3`、`mismatched_lease=1`。Adapter 离线临时复现源码在 [adapter-repro](evidence/wave2-mac-2026-09-28/adapter-repro/)；[运行日志](evidence/wave2-mac-2026-09-28/adapter-counterexamples.log)记录命令、退出码、PID 存活及清理，SHA-256 `4520aafe53b461735b7f8ae6cb0d164444c7562f73da403e5f77d0c2ac5a66b8`。另有 Mock 声明 `run.status` 但未实现、序号跳跃未进入对账、stdout 非协议行被忽略等静态审核疑点；本轮未逐项运行反例，不据此单独定 S1，但后续节点验收必须覆盖。

## 总控逐节点判定

| 节点 | 判定 | 原因及依赖影响 |
|---|---|---|
| DEV-02-02 | **维持 `REVIEW`** | 其 `0001` 局部专项反例已有历史独立 PASS，当前 31 项 SQLite 测试也通过；但 Mac 完整门禁缺 21/21，不能签 `DONE`。下游 DB-01～03 按所属节点修复，不把所有下游缺陷追溯算为 02-02 已实现范围。 |
| DEV-03-02 | **转为 `BLOCKED`** | AD-01/02 已由独立 Terra 离线复现，为进程生命周期 S1；AD-03 违反冻结协议。当前门禁亦未通过。DEV-03-03 不可准入。 |
| DEV-02-03～10、DEV-03-03～05 | **仍为 `PLANNED`** | 已有源码和 `SELF_CHECK_PASS`，但正式依赖未 `DONE`；只登记 DB/AD 问题，不能倒推准入或批量晋级。 |
| Wave 2 批次 | **未通过；停止下游准入** | 完整 Mac Gate、R3 独立反例及节点依赖均未关闭。DEV-04/05/06、V2 R2～R6 继续冻结。 |

## 修复与复验顺序（尚未派工）

1. `DEV-01-05` 门禁环境：为已归档文件在主索引仍为 deleted 的场景建立可审计输入清单处理；在不降低检查的前提下解决 `pnpm audit` 的可信漏洞数据来源和零费用网络/离线证据。若需外部 registry，按仓库规则单独确认；不得凭单独的旧 advisory DB 查询标记 Gate PASS。
2. `DEV-03-02`：总控把 AD-01/02/03 拆成互斥写集与失败先行测试；R3 修复由符合冻结路由的执行者实施，未参与实现的 Terra 重新做反例与进程树清理审核，总控逐差异终审。
3. `DEV-02-02` 在当前输入完整 Mac Gate 和独立证据审核通过后方可 `DONE`；之后按依赖审 `DEV-02-03/04/05…`。DB-01～03 以新前向 migration 和应用校验修复，不改写 `0001～0009`；每节点分别重跑专项反例与完整 Mac Gate。
4. 任一输入变更使本轮门禁证据失效。全部问题关闭后按同一最终输入重跑 21 项、全 refs 历史扫描和工具来源核验，再由另一 Terra 独立审核证据，总控逐节点判定。Linux/Oracle/Docker 不在本轮验证范围。

本轮未提交、推送、部署、修改产品代码、旧 migration、历史审核原件或生产数据。
