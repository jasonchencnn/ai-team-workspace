# FC 九包交付独立审计与裁决（2026-10-09）

**结论：本轮有实质进展，现有离线套件全绿，但整批准入仍 BLOCKED。** FC-04 存在新复现的单层 percent 脱敏漏检；FC-08 对原 M14 的“16/16”口径不予接受。其余成立的整改予以保留，不因这两项否定全部交付。

## 1. 输入、工作量和分工

- HEAD `1a14b8795e76fc373bef326eb5df96f9ac173f71`，继承 dirty worktree 保留。
- 与上轮实际交付哈希比较，工程文件改动 **13 个、新增 7 个**：新 0028/0029、retained_entity_integrity 测试、fixture bin/integration target、replay 两文件，共 20 个工程文件。没有把全部继承 Git diff 归给 Minimax。
- 0001–0027 与上轮输入逐字节一致；0028 与 FC-02 交付相衔接，0029 为前向追加。根 manifest/lockfile 未变；共享 persistence src/lib.rs 最终 hash 与登记一致。
- 313 项输入快照、变化清单、Git 状态见 [main](evidence/fc-return-audit-2026-10-09/main/input-before.json)。旧审计 242 项产物在本轮开始核验全部一致，Minimax 清理自身可再生 target 不等于删旧证据。
- 三个 Terra 分别负责存储、Adapter/Gate、脱敏/方案，一个 Luna 负责哈希/变异/覆盖机械核对，最多三名并发。主控独立跑全 workspace、16 个修订变异及原 M14、公开日志反例、当前覆盖、实际 SQLx DDL 失败注入，并串行整合 FC-09。
- 本轮主控工程修改仅 `eslint.config.mjs` 的一个精确归档路径；业务源码、迁移和 Minimax 已交付测试未改。新反例全部在新 evidence 的独立测试程序，不覆盖旧产物。未提交、推送、部署、联网、访问生产凭据或数据。

## 2. 主控独立验证

| 检查 | 结果 | 限定 |
|---|---|---|
| `cargo test --workspace --locked --offline --no-fail-fast` | **643 passed / 0 failed / 1 ignored**，exit 0 | 1 ignored 为既有文档示例 |
| domain | 287 / 0 | 同上 workspace 日志的独立 target 计数 |
| observability | 29 / 0 | 现有套件漏测下述 `%` 反例 |
| persistence-sqlite | 233 / 0 | 三处脆弱计数已关闭 |
| adapter-supervisor | lib 64 + integration 29，全绿 | 新 bin target 0 tests；无非法 stdout 跳过 |
| TS 各层 | 36 / 0 | 命名 E2E 不等于真实桌面 AC 闭环 |
| Gate / replay 单测 | **25 / 0、8 / 0** | 独立实跑 |
| lint 整合前 → 整合后 | **1 error → 0 error / 0 warning** | 精确归档一文件；反例规则仍有效 |
| fmt/typecheck/clippy/build/contracts/guards/strict rustdoc | 全 exit 0 | 当前本机已有依赖、offline/locked |
| workflow 检查 / dry-run | exit 0；canonical 恰 21 | 不冒充完整 Mac 21 或 hosted CI 实跑 |
| 工作树与全 refs 本地秘密扫描 | 两项 exit 0，no leaks found | 非 shallow，工具哈希/refs 前后一致有记录；不是生产秘密审计 |
| 当前 domain 覆盖重测 | **2016/2197 = 91.76%**，16 src | 全当前测试、完整生产分母；33 rs 同字节 |
| region / branch / application | 89.87% / **未验证** / **占位无可执行行** | region 不代替 branch，application 不记 100% |

完整命令与退出码：[checks.json](evidence/fc-return-audit-2026-10-09/main/checks.json)，各同名 `.log`。整合后的 lint 与范围反例另见 `lint-final.log`、`exact-lint-scope.log`；不能把 baseline 那条 exit 1 藏掉。

未运行 canonical 的联网 install/新鲜 advisory、真实 Harness/Keychain、最低 macOS 14 桌面、备份恢复、性能/A11y及全产品覆盖。没有把缓存、dry-run 或静态校验冒充这些能力。

## 3. FC-01/02/03：确定性存储整改通过局部审查

三条版本计数改为目标迁移 `WHERE version=N AND success=TRUE` 成功记录；故意失败升级停在 22/23 的回滚断言保留。未来新增迁移不再令这三条脆弱失败。

独立全 29 迁移真 SQLite 有效种子/FK 检查：

- Team/Role/Task/Binding/Reference/DeliveryAttempt 的 DELETE 全拒绝且 1→1，六条同 ID REPLACE 全拒绝且原内容保留。
- 原 DeliveryAttempt REPLACE 防线仍来自 0017；0028 不靠添加同名空守卫作假证明。
- Reference validation_status、终态 attempt 后新 retry、Run 合法终结与同态业务更新均成功。
- Run 完整 64 格：**25 接受 = 8 self + 17 Frozen 边，39 拒绝**。终态互转缺口已关闭。
- 0028 升级无业务数据改写、0029 强占 trigger 名失败后回滚，专项 Cargo 同样全绿。

见 [真库脚本及日志](evidence/fc-return-audit-2026-10-09/storage/REPORT.md)。交接中“references、delivery_attempts 仍无 DELETE 守卫”是旧状态，不能继续列为当前未修缺陷；当前防线在 0028:402、:435。

创建轴继续限定为可信生产创建命令的后续责任；不强制历史 fixture INSERT 全部先 QUEUED，不宣称应用创建、重试、权限/事务链已经完成。`RAISE(IGNORE)` 变异须称为 **fail-silent：返回成功但未写入**，不能只因负向测试红就称持久化状态被非法改写。

## 4. FC-04：P1 单层 `%25` 漏检，必须修复

`observability/src/lib.rs:665-678` 优先消费相同的字面字节。当注册字节自身是 `%`，输入 `%25` 的首字节被误当字面 `%`，后续匹配失败且不尝试正确 escape 路径。

| 注册的模拟 secret | 合法一次编码 | 当前 sanitize/error/Event/真实日志 |
|---|---|---|
| `%ABC` | `%25ABC` | **原样保留，泄露** |
| `A%B` | `A%25B` | **原样保留，泄露** |
| `%%` | `%25%25` | **原样保留，泄露** |

这不是未授权的递归解码：编码输入一次解码即得到注册 secret，属于 FC-04 明确承诺的“每个 secret 字节可字面或 `%XX`”。字母数字与 mixed hex 原缺陷修复成立，但不能据此宣称整个范围无误。

主控用**当前真实 crate 公共 API**和 `JsonSubscriber` 输出复现，见 [公开路径日志](evidence/fc-return-audit-2026-10-09/main/percent-public-repro.log)。新独立回归应要求 REDACTED，实测 **0 passed / 3 failed，exit 101**，见 [回归 RED](evidence/fc-return-audit-2026-10-09/main/percent-regression-red.log)。这 3 条是本次新反例，不混进 643 条已交付套件的统计。

需要有界地处理 `%` 的字面/转义歧义；不能简单换匹配优先级而把另一侧混合拼写漏掉，也不能用指数级回溯。新增开头/中间/连续 `%`、原文本/混合编码/畸形/UTF-8/资源上界对照，再走四个实际输出入口复核。业务修复仍交 Minimax 主责。

## 5. FC-05/06/09 裁决

**FC-05 的调整接受。** 25 个 fixture 消费用例迁入 integration target、由 `CARGO_BIN_EXE_fixture_adapter` 定位；四条 crate-private `send_protocol_line` 测试保留 unit，用字面参数启动 `/bin/cat` 或 `/bin/sleep`，不扩大生产可见性。watchdog 启动测试进程不等于用 libtest 做 Adapter。当前 64+29 实跑通过，没有缺 Node 后直接 return 的假绿路径，生产五入口 parser 未放宽。

成功路径 1→5 秒以及后代用例 1秒 grace/3秒上界/6秒 watchdog 对10秒管道持有者仍检验“不会等后代自然结束”，接受测试裕度调整；**它不再证明原1秒性能目标**。期望超时的30/50ms用例保留。测试bin未注册或包装为获准生产Adapter。

**FC-06 机制通过，修正文档信任前提。** shallow 为 true/命令失败/空值均被拒；companion ID 唯一性及 kind/command/args 绑定生效；main 不读取外部伪造证据 JSON。PR 场景里的 ci.yml 是 PR 自带版本，不是“已受评审”。临时目录和 PATH 桩件是测试隔离手段，**不是 shell 安全沙箱**；明确在现有执行 PR 源码、只读权限/无 secret 的 CI 信任边界中使用，不外推到带凭据的任意不可信本地工作区。

**FC-09 已由主控整合。** 仅归档 `docs/reviews/evidence/deep-audit-2026-10-08/replay_env_manifest.mjs` 这一不可变历史件，规则未关闭、其字节未变。virtual active probe 和同目录 neighbour 均仍被 `no-unsafe-finally` 拒绝，全仓 lint 0。新 replay 的8项验证通过。该工具范围收口不等于完整节点或 Mac Gate 已通过。

## 6. FC-07：方案改善成立，DDL 执行路径裁决

route 概念副本独立重跑 **42 observations / 0 failed**：每个 attempt 保留真实收件人、override 与 Binding 快照，三次改派不重写旧尝试；Event 先于立即 FK 关联、四元组幂等和连续前驱链均有对照。旧推荐(b)已撤回。新增载体落实 Frozen 改派语义本身不自动需要新 ADR；改变身份/权限/协议含义才另行提交 Human。

Subagent 复现出原 Python `executescript` 不显式包事务时，DROP 后失败会丢守卫。**该结果只能证明原 POC 的 DDL 执行方式不足，不能外推为 SQLx 同样不原子。**

主控追加实际 **SQLx 0.8.6 Migrator、no_tx=false** 实验：在当前29版库，三处 DROP 各在重建前注入非法语句，均失败并恢复**完整 sqlite_master schema 字节**、不留下临时30版记录；正常概念 migration 成功，三条重建守卫和0028留存/0029终态守卫均在。见 [SQLx故障注入](evidence/fc-return-audit-2026-10-09/main/route-ddl-sqlx-locked.log)。30只用于内存审计库，不占生产迁移编号。

裁决：正式实现必须走 SQLx 的单 migration 事务，不得沿用无事务 POC 直接安装。此实验补足**空库DDL原子性**；存量业务行升级、完整三轴语义等联合回归仍须实现包验收。DROP 是同事务前向替换手段，不是准许任意删除防线的“回退方案”；不弱化0028。当前只接受方案方向，不签入产品实现，也不宣布节点 DONE。

通知 A/B 分类和旧Run/新Run/处置证明已修正，B-01～B-08 仍为 Human 待决输入；ADR-0007 Proposed 未改变，未决策略不能实现。

## 7. FC-08：M14 口径的最终裁定

主控在**全新当前副本**独立重跑：绿基线40/0，16个修订实验全 `killed_by_assertion`，恢复后40/0；137个文件还原无差异，80个 `.rs` 与当前生产无差异。旧补丁、原始日志和 SHA 也经机械核对，未把编译失败当测试杀死。

同时单独跑**只有新增 enum 变体、没有配套 ALL/as_str 改动**的原写法 M14：**exit101、E0004、0失败测试（未运行）**；撤销后40/0。修订后的 M14 被**1个**测试检出，原“2个”没有复现。

因此：

1. **原 R10 声明口径：15项按声明意图重建复现＋1项原写法未证/不可编译。不能写原声明16/16。** 即使另外15项也没有当时原日志，仍只能证明当前重建实验，不能证明历史执行现场。
2. **新实验集口径：15项重建＋M14-R（明确配套修改）共16项，16/16断言检出。** 可以单列这一新统计，不能用“重新定义声明数”覆盖第1项。
3. FC-08 index/ledger 应引用新裁决，原历史文档/证据继续保留。接受它的证据机制改进，拒绝原口径偷换；这不意味着当前测试代码必须推倒重写。

根证据：[当前重跑台账](evidence/fc-return-audit-2026-10-09/mutation/ledger.json)、[原M14独立结果](evidence/fc-return-audit-2026-10-09/mutation/M14-original-result.json)。覆盖复测仍为91.76%；branch mapping为空，安装读取工具本身不会凭空产生分支映射，后续需有效 branch instrumentation 后再测85%门槛。

artifacts 12值撤回与R09最终注释/文件数/IR口径订正成立；FC-08中“refs/delivery仍缺守卫”与交接旧开放清单需更新为“曾有缺口，已由FC02关闭”。

## 8. 逐包与项目状态

| 包 | 本轮裁定 | 尚需处理 |
|---|---|---|
| FC-01 | 局部通过 | 统一节点门禁仍未完成 |
| FC-02 | 局部通过 | 不外推应用权限/历史导入保证 |
| FC-03 | 局部通过 | 创建执行点未验；文档别把IGNORE误称脏写 |
| FC-04 | **BLOCKED** | 新 `%25` 漏检 |
| FC-05 | 局部通过，范围调整已接受 | 真Harness/平台准入仍另验 |
| FC-06 | 机制通过，文档补充 | PR信任前提与沙箱表述 |
| FC-07 | 方案方向接受，实施仍REVIEW | 事务执行方式/存量联合验收、Human通知决策 |
| FC-08 | 证据通过，统计口径REVIEW | 原15+1、新16/16分列，旧开放状态同步 |
| FC-09 | **主控已整合并通过局部验收** | 完整Mac21仍未运行 |

当前依旧 **20 DONE / 21 BLOCKED / 60 PLANNED**，没有恢复任何正式节点 DONE 或启动依赖。已有局部缺陷修复关闭与节点总准入分开记录，避免下轮又把旧缺口列为当前故障。

可转发 [下一轮精确整改表](../coordination/MINIMAXCODE-AUDIT-FOLLOWUP-2026-10-09.md)。先关闭唯一新行为阻断，再补方案/证据口径；后续可信身份、应用事务、dispatcher/fencing、真实桌面开发仍按冻结依赖另签任务。
