# Wave 2 第一批依赖节点终审（2026-09-28）

> 范围：DEV-02-03、DEV-02-04、DEV-03-03。首轮失败与前置节点终审记录不改写；本记录不判定 Wave 2 整批完成。当前只验 macOS ARM64。

## 输入、执行与独立审核

- 前置 DEV-02-02、DEV-03-02 已依[前置终审](WAVE-2-ENTRY-CLOSURE-2026-09-28.md)关闭，直接依赖满足。
- 原有 `0002`、`0003` 和握手/健康实现来自 Gemini 3.8 Flash 的 `SELF_CHECK_PASS`。总控接收既有成果，不重写旧 migration；本批必要修复由 Codex `gpt-5.6-terra` 完成，原因是冻结语义内的跨 Team/DAG SQLite 约束和 Adapter 协议负向边界。总控独占节点状态、完整门禁和最终结论。
- DEV-02-03/04：Terra `dev_02_03_binding` 只写 `crates/persistence-sqlite/src/lib.rs`、新增 `0010/0011` migration 和各自新测试；未参与实现的 Terra `wave2_gate_independent_review` 只读复核，多轮反例审核最终均 PASS。
- DEV-03-03：Terra `dev_03_03_handshake` 只写 `crates/adapter-supervisor/src/lib.rs`、`protocol.rs`、`adapters/mock/index.mjs`；未参与本节点实现的 Terra `adapter_lifecycle_independent_review` 两次找出能力虚报、未认证仍可写入和原始 transport 公共绕过，修复后独立终审 PASS。

## 逐节点验收结论

| 节点 | 总控结论 | 交付与关键失败证据 |
|---|---|---|
| DEV-02-03 | `DONE`（最终输入门禁通过后生效） | 新前向 migration `0010_binding_team_consistency.sql` 在 Binding INSERT/UPDATE 和 Role/Harness/Adapter Team 变更时拒绝跨 Team 事实；合法 v9 历史 Binding 能升级、换绑和保留区间；脏 v9 升级原子失败并保留数据。真实 SQLite 负向测试及独立 Terra 复核 PASS。 |
| DEV-02-04 | `DONE`（最终输入门禁通过后生效） | 新前向 migration `0011_task_dependency_consistency.sql` 拒绝跨 Team Task owner/依赖及 3、4 跳和 UPDATE 改边成环；合法 v10 DAG 升级后 5 个触发器实际安装，非法写入无副作用；脏 v10 owner、边、3/4 跳环升级原子失败。10 秒外部 watchdog 下脏库升级反例 0.358 秒内退出，证据位于仓库外 `wave2-mac-evidence/dev-02-04-dirty-upgrade-watchdog.json`。独立 Terra 复核 PASS。 |
| DEV-03-03 | `DONE`（最终输入门禁通过后生效） | 真实 Node Mock 验证不兼容版本、缺能力、缺认证和健康不可用。握手结果限于本子进程实例；`run.start/cancel` 在任何协议写入前拒绝未初始化（`-32099`）、未认证（`-32012`）和缺能力（`-32011`），返回脱敏结构化诊断；原始写入 API 收窄为 crate 内部。Mock 不再虚报尚未实现的 `run.status` 或 `status_query`。81/81 Adapter 测试及独立 Terra 复核 PASS。 |

三节点适用 G0～G5/G7～G9 已按其范围核查：版本/迁移与现有 Schema 保持一致；数据拒绝零副作用；协议失败在可信边界拒绝且结构化返回；进程/诊断有界；定向测试、Clippy、格式和 Node 语法检查通过。没有 UI，本批 G6 不适用。完整 Mac 工程门禁仍须在本记录和动态状态文件写入后的**同一最终输入**重跑 21 项，并由独立 Terra 只读核证据。证据目标为仓库外 `wave2-mac-evidence/wave2-macos-arm64-batch-1-final.json`；若失败，三节点保持 `REVIEW/BLOCKED`，不得启动其依赖。

`DEV-02-03/04` 对历史脏库选择原子拒绝升级，绝不静默删除或改写数据。受控人工修复流程属于后续数据迁移/恢复节点；当前应用连接必须遵守冻结的 `foreign_keys=ON` 前提。`DEV-03-03` 提供协议层结构化错误和协商事实，持久化 Event、应用层权限与恢复属于后续 DEV-02/05/03 节点；`run.status`、`run.reconcile`、`adapter.shutdown` 的真实方法仍未完成，不在本节点冒充通过。后续输入变化须重跑 Mac 门禁并复核受影响节点。

本批未改写 `0001～0009`，未修改 `sources/`，未提交、推送、部署或操作生产数据。
