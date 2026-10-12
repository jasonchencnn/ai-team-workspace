# Wave 2 前置节点终审（2026-09-28）

> 范围：仅 DEV-02-02、DEV-03-02。首轮失败事实仍见 [WAVE-2-MAC-ACCEPTANCE](WAVE-2-MAC-ACCEPTANCE.md)；本记录不判定下游节点或整批 Wave 2 完成。macOS ARM64 是当前验证平台。

## 输入与职责

- 总控：Codex 当前 GPT-6 会话，负责逐节点核冻结规格、依赖、修复边界、最终输入与状态。
- 既有独立专项审核：未参与迁移实现的 Terra 对 DEV-02-02 的 UUIDv7 外键和迁移重放给出 PASS，见 [DEV-02-02-REVIEW](DEV-02-02-REVIEW.md)；未参与进程修复的 Terra 对直接进程、同组后代及获准 Mock 的真实脱组后代给出定向 PASS，见 [前置修复](WAVE-2-PREBLOCKER-REPAIR.md)。
- 独立工程证据审核：未参与实现的 Terra 对 `official-4` 证据核对 21 项退出码、前后输入哈希、官方漏洞查询、工具来源、全 refs Gitleaks 和风险处置，结论为 Mac 工程门禁/G9 PASS；该审核没有替总控签节点 `DONE`。
- 仓库外受限证据：`/Users/chenzhiqing/.codex/visualizations/2026/09/28/01a0e5de-0228-7503-9481-b00f434906ec/wave2-mac-evidence/`。`official-4` 输入总 SHA-256 为 `f5919c20dca9a3a2dbb2b1b27087f1173e50fd54e9d9895c14f45cc7b9748b5b`，前后相同；21/21 退出 0。其 JSON SHA-256 为 `0c564c840be7a51508a33c1534f779f3d72395f3f648eeacb64adc04619de8f8`。

## 总控逐节点结论

| 节点 | 结论 | 专项与通用门禁 |
|---|---|---|
| DEV-02-02 | `DONE` | 前置 DEV-02-01 已 `DONE`。`0001` 的 Team/Role/Workspace UUIDv7、版本、状态时间、唯一及外键约束、重复迁移和非法写入反例已由真实 SQLite 测试及独立 Terra 复核。后续 `0002～0009` 的跨 Team、DAG、Run/Lease 缺陷归其各自节点，不倒灌为 `0001` 已实现范围。G0～G5/G9 适用项及 Mac 工程门禁通过；本节点没有 UI/热路径。 |
| DEV-03-02 | `DONE` | 前置 DEV-03-01、DEV-01-08 已 `DONE`。参数数组、环境隔离、stdio 行/诊断上限、写入 deadline/cancel、直接进程回收、同组后代清理和超时失败注入均通过。按已接受 ADR-0006，获准 Mock 在真实 Mac 的 EOF、强制关闭、直接进程崩溃三路径均证明脱组后代有界退出，独立 Terra 复核。G0～G3/G5/G8/G9 及 Mac 工程门禁通过。 |

`DEV-03-02` 只交付 Supervisor 的传输、子进程和生命周期边界。`run.status`、`run.reconcile`、`adapter.shutdown` 的 JSON-RPC 方法、完整 DTO Schema 等价性及 Mock 故障注入仍须在 DEV-03-03～05 分别验收，不随本节点晋级。任何新增 Adapter 仍须按 ADR-0006 单独准入；平台不宣称能强制回收任意恶意脱组后代。

## 安全、回退与失效

官方 npm high/critical 为 0；RustSec 漏洞为 0。7 条 RustSec informational warning 与 npm moderate/low 各一项的责任和 2026-10-28 复查期限见 [安全处置](WAVE-2-MAC-RECHECK-2026-09-28.md)。没有因公告误匹配而增加 ignore；内部 crate 已更名为 `teamspace-domain`。全 refs Gitleaks 为零命中，所用二进制与官方 release checksum 核对一致。此次未提交、推送、部署或操作生产数据。

本记录及动态状态文档写入会改变 `official-4` 所对应的输入。最终判定以仓库外 `wave2-macos-arm64-entry-final.json` 的 21/21、前后输入同哈希、全 refs 扫描和独立只读复核为生效条件；若任一项失败，两个节点立即恢复 `REVIEW`/`BLOCKED`，依赖继续冻结。后续代码修改也会使该证据对新输入失效，关闭下一节点前必须重跑完整 Mac 门禁并复核受影响范围。回退当前状态只需恢复节点登记；既有 migration、修复代码和失败证据保留。
