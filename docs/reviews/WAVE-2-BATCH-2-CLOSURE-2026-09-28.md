# Wave 2 第二批依赖节点终审（2026-09-28）

> 范围：DEV-02-05、DEV-03-04。仅在本记录末尾的最终输入门禁条件满足时，节点结论生效；不判定其余 Wave 2 节点或 V2 R1 已完成。当前验证平台为 macOS ARM64。

## 依赖、分工与独立复核

- DEV-02-05 的直接依赖 DEV-02-01/04 已正式关闭；DEV-03-04 的直接依赖 DEV-03-01～03 已正式关闭。两节点原有实现来自 Gemini 3.8 Flash 的 `SELF_CHECK_PASS`，本轮接收并专项复核，不重写旧 migration。
- DEV-02-05 的新前向 `0012_run_lease_consistency.sql` 由 Codex Terra `dev_02_05_sql_guards` 独占修改；Terra `dev_02_03_binding`、`dev_02_05_tests_red` 在互斥测试文件写入真实 RED 反例；总控仅修正测试夹具、串行整合和签核。未参与 SQL 实现的 Terra `dev_02_05_independent_review` 第一轮判 `BLOCKED`，修复后第二轮只读审查判专项 `PASS`。根 manifest、lockfile 和 `0001～0011` 未因本节点修改。
- DEV-03-04 的现有 Mock 正常流已由未参与实现的 Terra 在真实 Node 进程上独立复核，专项 `PASS`；本轮未扩大到 DEV-03-05 故障注入或 DEV-03-08 对账状态机。

## 专项结论

| 节点 | 总控结论 | 证据与边界 |
|---|---|---|
| DEV-02-05 | `DONE`（最终输入门禁通过后生效） | 新迁移对 Run/Task/Role/Binding/Harness Team 及 owner、Lease 的 run/task/token、Review 的 Team/Task/成功 Run 做前向升级 guard 与 INSERT/UPDATE/父记录 UPDATE 保护。新 Run 的 attempt 从 1 连续加 1、fencing token 严格递增；只有最高世代可活动；终态前须释放 Lease。旧终态 Run 保留原 Role，Task 可在无活动 Run/Lease 后同 Team 改派。合法 v11 升级、脏 v11 原子失败及关系/世代/终态反例由真实 SQLite 测试证明。定向 6/6、4/4、旧测试 3/3、整个 persistence crate 47/47；Clippy、fmt、空白检查通过，独立 Terra 专项复核 PASS。 |
| DEV-03-04 | `DONE`（最终输入门禁通过后生效） | 真实 Node Mock 重复验证 `initialize → health.check → run.start → heartbeat → output → artifact_candidate → completed`，sequence 从 1 递增，结果只返回 Artifact 引用。已有独立 Terra 专项复核 PASS；最终输入的完整 adapter 测试、Clippy 和 Mac 工程门禁仍为生效条件。 |

`DEV-02-05` 只关闭迁移及关系一致性。完整 Run 状态迁移、命令级 actor/权限、expected-version、幂等、Artifact 与当前 Task version 校验，以及 Review decision 的追加写边界属于 DEV-02-11 等下游节点；当前 SQL 的合法关系 UPDATE 不等于授权任意人改写 Review 历史。`DEV-03-05` 的真实 Node 同序异载荷、运行期超时/失联及取消回收仍待修；sequence 跳跃的 UNKNOWN/对账路径属于 DEV-03-08。两者不得凭本记录宣称通过。

## 完整门禁生效条件、失效与回退

本记录和动态状态文件写入后，须在**同一最终输入**上运行 `scripts/ci/local-gates.mjs --platform macos` 的 21 项工程门禁，确认全部退出码为 0、输入前后 SHA-256 一致；另对全部 Git refs 执行 Gitleaks 零命中并保留工具版本/官方来源、官方 npm/RustSec 只读漏洞查询、证据 JSON 校验和及独立 G9 只读复核。证据目标为仓库外受限目录 `wave2-mac-evidence/wave2-macos-arm64-batch-2-final.json` 及同批 provenance。若任一条件失败，DEV-02-05、DEV-03-04 保持 `REVIEW/BLOCKED`，其依赖节点不得启动。后续输入变化使本批工程门禁对新输入失效，关闭下一节点前必须复跑并复核。

没有修改 `sources/`，没有提交、推送、部署或操作生产数据。脏旧库升级选择原子拒绝，不自动删除或改写历史；受控清理归后续迁移/恢复流程。
