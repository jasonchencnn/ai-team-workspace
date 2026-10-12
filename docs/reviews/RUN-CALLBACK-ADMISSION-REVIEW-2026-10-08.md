# Run 回调判定补齐（2026-10-08）

## 结论

**本批规则与文档修复通过局部验收；DEV-03-07 仍 BLOCKED，持续整改目标未达成。** 主控复核依赖后推进本批：DEV-03-07 依赖 03-04～05、02-05、02-11，不依赖待确认的 Notification 语义。没有以本批 helper 结果替代数据库当前世代查询或正式执行链路。

## 修改与事实依据

- `run_fencing.rs`：零世代不能授权写入、续租或心跳。`0004` 要求 fencing_token >= 1，上一实现却把 0/0 当作 Current。
- 普通写入及心跳判定必须接收 `RunStatus`。STATE_MACHINES §3 的四个终态不能接收新执行效果；UNKNOWN 冻结写入并通过专用 reconcile 处理。旧/超前/零 token 优先返回既有 RUN_FENCED；当前 UNKNOWN 返回 RUN_STATUS_UNKNOWN；当前终态返回 DOMAIN_INVALID_TRANSITION。
- 保留有效启动路径。STARTING 可以接收启动失败，普通判定不代替具体 callback/command 的状态迁移检查，不把所有非 RUNNING 状态一概当作非法。
- 新 `RunWriteRefusal` 是内部 Rust 类型，全部使用既有目录错误码；没有增删领域状态或 Adapter 字段。`decide_write`/`decide_heartbeat` 的内部 API 和对应测试同步。
- 修正判定范围说明。当前 token 不能证明有效租约；DOMAIN_MODEL §2 的时效授权仍须在应用事务内核对。Accept 只证明世代/状态层通过，不能当作完整权限或最终落库授权。短 TTL 可以保留原 expiry，文档不再误称每次必然延长窗口。
- rustdoc 修复 12 个断链诊断。Luna 仅修改六个文件的文档链接目标；主控修正 RunErrorCode::RunFenced 的过期名字，并更新 crate 范围说明。六份原文按上一轮哈希复原并验证，精确 diff 全为注释。

实现归属：主控（GPT-6 系列）负责 DEV-03-07 R3 规则及第二遍反例；gpt-5.6-luna 负责 DEV-02-11/12/13/14 的 R0 链接修复，允许写集仅六份注释，不参与权限/状态逻辑或根 manifest/lockfile。

## 验证

| 检查 | 当前可核验结果 |
|---|---|
| 旧源码重放 | 与 10-07 已记录 SHA-256 相等；零世代写入、续租、心跳 **0 passed / 3 failed**，均为行为断言 |
| 当前 fencing 专项 | **25 passed / 0 failed**，比上一输入增加 5 个用例 |
| Rust 全工作区 | **499 passed / 0 failed / 1 ignored，exit 0** |
| 第二遍变异 | 健康基线 25/25；**7/7 被行为断言捕获**；没有编译错误计数 |
| clippy 全工作区/all-targets/all-features -D warnings | exit 0 |
| fmt check、workspace build | exit 0 |
| 严格 rustdoc | workspace no-deps -D warnings 通过；最终 domain 文档再验通过 |
| 迁移/manifest | 23 份迁移、所有 manifest/lockfile 无本批修改 |

7 个变异分别放行终态、放行 UNKNOWN、放行零世代写入、放行零世代续租、跳过心跳状态、跳过世代比较、错误码漂移。源码、运行器和逐项日志一并保留。Scope 对照覆盖启动状态、RUNNING、终态、UNKNOWN 和旧 token 拒绝优先级。

工作区测试后只澄清了一份文件的文档范围，未修改可执行代码。按前后哈希与精确 diff 证明为纯 `///` 注释，随后 clippy/build/严格 domain rustdoc 验证。**不声称完整 21 项在完全相同最终字节输入上通过**；当前在线审计授权仍待答复，覆盖率及业务 E2E 未验证。

## 验证过程异常与证据恢复

本轮 `/private/tmp/teamspace-*` 临时验证目录意外消失，原因未确认；若干日志和副本不再可读，其中一次早期 workspace 检查终态为 exit 101，但无法恢复其诊断，未计作通过。正式源码、先前仓库内证据保留。

后续输出全部改为直接写入仓库证据目录，并重新执行工作区测试、构建、clippy、fmt、rustdoc 和 7 个变异。旧基线使用已归档的原片段及本批逆向补丁重建，SHA-256 必须等于上一轮摘要才执行；新日志是本日重放，不冒充丢失的历史日志或旧 Mac Gate JSON。

## 尚未关闭的实际运行要求

1. application 当前仍是占位 crate：缺少可信权限解析、拒绝 Event、事务/幂等/Outbox 和实际回调持久化。Supervisor 仍只比较启动快照；本批未接入它的生产消费路径。
2. 当前状态、Task/Run/Binding 身份、当前世代、有效租约、权限、sequence 与最终效果必须由同一应用事务保证。传入运行状态这个参数不等于已经从数据库重读。
3. DEV-02-13 依赖 02-02～10；02-08/09 未正式验收，ADR-0007 待 Human 确认。完整权限/事务执行链保持依赖阻断；不通过伪造权限 bool、固定 RUNNING 或测试中的人工 SQL 接线绕过。
4. 依赖节点与整个目标保持未完成。当前输入完整 Mac Gate、覆盖率、真实 Mock/SQLite 取消重试与晚到回调落库仍须完成。

## 证据

[机器摘要](evidence/run-admission-2026-10-08/summary.json)、[工作区测试](evidence/run-admission-2026-10-08/workspace-tests.log)、[旧基线来源](evidence/run-admission-2026-10-08/previous-source-provenance.json)、[变异台账](evidence/run-admission-2026-10-08/mutations/results.json)、[文档修改证明](evidence/run-admission-2026-10-08/rustdoc-links-proof.json)、[输入范围证明](evidence/run-admission-2026-10-08/input-scope.json)。

运行器 `mutate-admission.py` 使用本目录下的 repro 副本；执行 `python3 docs/reviews/evidence/run-admission-2026-10-08/mutate-admission.py`，finally 恢复副本代码并移除其临时历史 API 测试，正式项目不被变异。repro 是当前边界重放材料，不作为正式工程成员或运行时服务。

未提交、推送、部署、访问外部服务或修改生产数据。回退仅限本批文件补丁，不 reset 继承工作树；历史代码/证据保持可追溯。
