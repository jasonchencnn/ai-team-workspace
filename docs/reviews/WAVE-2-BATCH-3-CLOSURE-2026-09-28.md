# Wave 2 第三批依赖节点终审（2026-09-28）

> 范围：DEV-02-06、DEV-03-05。仅在本记录的最终输入门禁条件满足时，节点结论生效；不判定其余 Wave 2 节点或 V2 R1 完成。当前验证平台为 macOS ARM64。

## 依赖、分工与独立复核

- DEV-02-06 的 DEV-02-01/04/05、DEV-03-05 的 DEV-03-04 均已按前两批终审正式关闭。两节点原有代码来自 Gemini 3.8 Flash 的 `SELF_CHECK_PASS`；本批仅补真实反例揭示的缺口，不改写 `0001～0012`。
- DEV-02-06：Codex Terra `dev_02_06_tests_red` 独占两份新测试，先得到跨 Task、缺失主体、父主键更新/删除与 `INSERT OR REPLACE` 的真实 RED；Terra `dev_02_05_sql_guards` 独占新增 `0013_artifact_reference_consistency.sql` 及其 migration 注册；未参与实现的 Terra `dev_02_06_preaudit` 多轮只读审核，最后专项 PASS。总控负责冻结语义裁决、写集协调和最终门禁。
- DEV-03-05：Codex Terra `dev_03_05_faults` 独占 Mock 故障模式及 Rust 真实进程测试；未参与该节点实现的 Terra `adapter_lifecycle_independent_review` 只读复核，并在 macOS 上重放四条新增真实 Node 反例，专项 PASS。

## 逐节点结论

| 节点 | 总控结论 | 专项证据 |
|---|---|---|
| DEV-02-06 | `DONE`（最终输入门禁通过后生效） | `0013` 以升级 guard 和 INSERT/UPDATE/父记录保护保证 Artifact 的 Team/Task/Run/creator Role 元组一致；Reference 的 TASK/RUN/ARTIFACT/MESSAGE 主体必须存在且同 Team。四类主体的主键更新/删除不能留下悬空 Reference；对已被引用旧行，插入前检查全部实际主键与二级唯一键冲突，防止默认 `recursive_triggers=OFF` 下 `INSERT OR REPLACE` 隐式删除绕过。合法 v12 历史升级、脏 v12 原子失败和两条真实 REPLACE 反例通过；新定向 13/13、2/2，整个 persistence crate 58/58，Clippy、fmt、空白检查及独立 Terra 专项复核 PASS。 |
| DEV-03-05 | `DONE`（最终输入门禁通过后生效） | Mock 新增同序不同 payload 的 `duplicate_conflict`，真实 Node 进程返回 `-32099 ADAPTER_PROTOCOL_VIOLATION`；`timeout_stream` 在 `run.start` 后停滞并于 deadline 有界失败；`unresponsive --fault-stage run` 证明启动后失联分类；取消超时后验证直接 Node PID 已回收。已有重复幂等、乱序、崩溃、晚到和取消拒绝场景保留；adapter-supervisor 84/84、Node 语法、Clippy、fmt、独立 Terra 专项复核 PASS。 |

`0013` 只保证当前冻结字段的关系一致性，不把 Workspace URI、受控 URL、`ROLE_RESTRICTED` 可见性、expected-version 或 Artifact 发布幂等伪装为数据库已完成的权限判定。这些由后续 Workspace、权限与 Artifact application 节点在最终执行点实现并验收。Run/Message 的 REPLACE guard 已按现有全部唯一键静态复核；后续可补真实进程外的两条动态回归。`DEV-03-05` 只覆盖 Mock 故障注入；sequence 跳跃后的 UNKNOWN/对账状态机属于 DEV-03-08，不随本节点关闭。

## 最终输入门禁、失效与回退

本记录及动态状态文件写定后，须对**同一最终输入**运行 macOS ARM64 的 `scripts/ci/local-gates.mjs` 21 项门禁，全部退出 0 且输入前后 SHA-256 相同；另以全部 Git refs 的 Gitleaks 零命中、工具版本与官方来源、官方 npm/RustSec 只读漏洞结果、证据文件哈希和独立 G9 只读复核构成生效条件。证据目标为仓库外受限目录 `wave2-mac-evidence/wave2-macos-arm64-batch-3-final.json` 及同批 provenance。失败则两个节点保持 `REVIEW/BLOCKED`，不得启动其依赖；后续输入变化使本批工程门禁对新输入失效。

未修改 `sources/`，未提交、推送、部署或操作生产数据。脏旧库升级原子拒绝，不自动改写或删除历史。此次不设置连接级 `recursive_triggers`；迁移在默认值为 OFF 的真实 SQLite 上直接拒绝了已证明的关系破坏 REPLACE 路径。
