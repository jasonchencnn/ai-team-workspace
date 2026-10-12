# AF 三包独立验收与接续裁定（2026-10-09）

**结论：AF-01 的 `%` 漏检修复通过独立行为审验；AF-02 的概念事务方案通过限定验收；AF-03 的历史证据重放通过。全项目仍不能准入：本轮默认并行全仓实测有 1 个失败，且 pipe-holder 的 Drop 用例存在确定的非空性缺口。**

本报告替代当前动态入口中的“AF-01 仍未修”及“AF-02 存量业务行联合实验未做”描述；保留所有历史文档、失败日志和自陈原件。正式节点仍 **20 DONE / 21 BLOCKED / 60 PLANNED**，ADR-0007 仍 Proposed，未开放下游节点、V2、生产迁移或发布。

依据：[三个任务包](../coordination/MINIMAXCODE-AUDIT-FOLLOWUP-2026-10-09.md)、[上轮审计](FC-RETURN-AUDIT-2026-10-09.md)、AGENTS/override、未漂移的四开发基线及 Frozen 规格。主控负责独立门禁与裁决；内部两名 Terra 分别审查 matcher/资源和路由/进程故障，一名 Luna 检查机械证据，符合写集互斥及最多三个 Subagent 的约束。Minimax 原交付者登记 MiniMax-M3.1-Flash-Preview；自验收不代替本报告。

## 1. 输入、写集与实测门禁

HEAD `1a14b8795e76fc373bef326eb5df96f9ac173f71` 加继承 dirty 工作树。本轮先固定 [313 项输入](evidence/af-return-audit-2026-10-09/main/input-before.json)，再运行验证。与上一轮最终输入相比，仅 AF-01 两个授权文件改变；根 manifest/lockfile、生产迁移0001–0029和其余工程输入没有漂移。

| 文件 | 真正的 AF-01 修复前输入（前轮工作树） | 当前输出 |
|---|---|---|
| `crates/observability/src/lib.rs` | `d945dd5bd98c2d137a4a2e5c92583fbddba5668463788957e26b957f1eb4b756` | `8d2fe94ea27a4befd341b77ccd64e6e5a27c3e24371485a13e7ce58a8c0ede1e` |
| `crates/observability/tests/structured_logging.rs` | `f843839d48297857d6664fb19260b59a7d253a09690577485eef1dbe92463e76` | `368eff5f7b3b1601e8b037193c71effca6a05021daf27c855d2c86a02078a30e` |

主控独立实跑，原始日志写入全新 [main 证据](evidence/af-return-audit-2026-10-09/main/)，不覆盖 Minimax 或上一轮 evidence。数字由日志机器解析：[summary.json](evidence/af-return-audit-2026-10-09/main/summary.json)、[Rust 命令与 exit](evidence/af-return-audit-2026-10-09/main/rust-checks.json)、[JS 命令与 exit](evidence/af-return-audit-2026-10-09/main/js-checks.json)。

| 检查 | 独立结果 | 判定边界 |
|---|---|---|
| `cargo test --workspace --locked --offline --no-fail-fast` 默认并行 | **652 passed / 1 failed / 1 ignored，exit 101** | 真实失败，不能以之后绿日志替代 |
| 同命令追加 `-- --test-threads=1` | **653 / 0 / 1，exit 0** | 串行配置通过，不能外推默认并行稳定 |
| observability 独立完整副本 | **39 / 0** | 源码、测试与生产字节一致 |
| 原主控三条 `%` 回归原件复制重跑 | **3 / 0** | 修复前为0/3，修复后转绿 |
| TS完整套件 / Gate脚本 / 环境replay | **36 / 0、25 / 0、8 / 0** | 实跑，不从自陈照抄 |
| build：Rust、TS | exit 0 | 已有本地依赖，offline Rust |
| clippy全workspace/all-targets/all-features `-D warnings` | exit 0 | locked/offline |
| strict rustdoc（四业务crate） | exit 0 | `RUSTDOCFLAGS=-D warnings`，locked/offline |
| fmt、lint:ts、typecheck、contracts、guards、workflow、unsafe | 全部 exit 0 | 没有为了通过修改 lint 规则 |
| 工具链检查 | exit 0 | Node24.21.0、pnpm10.33.0、Rust1.98.0 |
| Mac门禁 dry-run | exit 0 | 仅计划检查，**不是完整 Mac 21** |
| AF-03 历史六裁决重放 | **38/38，exit 0** | 重放旧记录；当前终值另登记 |
| 本地秘密扫描 | 见§5 | 修正 allowlist 实际行为后再扫描 |

保留的942项历史/本次交付文件逐项哈希复查无漂移；上轮615项交付清单由 Luna 在主控整合前核对无missing、无drift；主控收口后仅其中两个现行README入口被授权更新，其余613项不变（previous-delivery-check.json）。产品源码本轮未改；主控仅整合 `.gitleaks.toml` 的精确规则例外及新增审计材料/入口链接。构建产物置于忽略目录，未删除旧证据、未 reset/clean/stash。全程本地、费用$0、未提交/推送/部署/生产操作。

## 2. AF-01：行为修复接受，最长匹配接受

`PercentForm::match_at` 已从贪心消费改为逐层可达 escaped-count。第i层消耗 `i + 2e` 字节，同时保留字面和 `%XX` 两条合法转移；仅escape的两位hex大小写不敏感，字面字节严格相等。无递归percent解码。

主控新建独立公共API探针，以**枚举合法拼写并按长度排序的参考实现**对照实际 `Redactor`，避免用手抄DP验证同一DP：156个生成策略，**6,930次合法/参考对照、6,666次畸形或大小写负对照**；UTF-8有效拼写、含字面`%25/%41`片段、两种注册顺序也通过。另有6组真实 `sanitize/redact_error/redact_event/JsonSubscriber` 四入口观测（含原三反例）。源与原始日志：[generated.rs](evidence/af-return-audit-2026-10-09/main/percent-probe/src/bin/generated.rs)、[percent-generated.log](evidence/af-return-audit-2026-10-09/main/percent-generated.log)。这是有限合成输入验证，不宣称全产品链验证。

**接受最长合法匹配作为本包脱敏边界。** 对注册`%%`、输入`%%25`，字面/混合拼写有歧义，取最长仍在已承诺“每注册字节可字面或一次escape”的集合内；`%41%25%25`中的无关前缀仍保留。不引入新的领域状态、权限、错误码或多层decode语义，不需要为这一常规匹配实现另立ADR。

资源界由生产实现证明：每层升序去重最多i+1个状态，单候选O(n²)时间/O(n)空间。anchor扫描单调推进，与候选匹配成本分开；多注册secret的成本累加，不能宣称全sanitize为O(input)。缓冲复用避免每候选重新创建buffer，但 `Vec` 在第一次使用或首次更大层时仍可扩容，交付的“每候选零分配”须按这个限定理解。

### 2.1 四个变异由主控重新实跑

使用当前独立源副本、锁文件、编译成功后的真实断言结果；每次保存patch，最终实际保留副本与生产两文件字节相同：[台账](evidence/af-return-audit-2026-10-09/main/af01-mutation-ledger.json)、[还原](evidence/af-return-audit-2026-10-09/main/af01-mutation-restore.json)。

| 项 | 实测 | 裁定 |
|---|---|---|
| 当前绿基线 | 39/0，exit0 | 可运行基线 |
| M1回到旧贪心 | 29/10，exit101 | 真实断言检出 |
| M2取最短 | 34/5，exit101 | 真实断言检出 |
| M3移除mark去重 | 39/0，exit0 | 存活；本组未检出，不计kill |
| M4移除不可达层早退 | 39/0，exit0 | 存活；功能结果不变，不计kill |
| 还原回归 | 39/0，exit0 | 还原哈希与生产一致 |

主控不要求用不稳定wall-clock断言凑“N/N全杀”。M3资源等价的全输入证明本轮未建立；“distinct≤i+1，所以重复副本也同界”不是有效证明。资源性质亦可用真实状态/操作计数验证，并非只能用耗时阈值。生产去重仍在，其边界已明确，M3存活不构成本包实际算法缺陷。

**审计自身勘误**：Terra首份模拟无条件让每状态二叉，忽略实际cursor/字节条件，所称“M3指数增长”没有成立。主控要求重做后已撤回，原报告保留，最终以 [CORRECTION.md](evidence/af-return-audit-2026-10-09/matcher/CORRECTION.md) 和真实转移日志为准。重复`%`输入并不使每层全部路径继续歧义，Minimax资源样例的该措辞也不予采信。顶层constant描述的是anchor扫描，不据此误报代码复杂度矛盾。

### 2.2 AF-01 输入/回退勘误

AF-01 §1标作输入的9251…/fcdfa…是**历史HEAD**，不是本包开始时的dirty工作树输入；相对HEAD不同只证明改动存在，不能单独证明本轮修复生效。真实前轮输入是§1表内d945…/f843…；已在上轮完整副本保留。

AF-01 §8 `git checkout HEAD -- <两文件>` 的回退说明作废：它会丢掉前轮FC04等成果，且HEAD测试不是所称29用例。需要回退时只能由主控核实当前输入后，从上轮保留副本取回两份完整文件并验证上述前轮SHA；本轮没有执行回退。历史AF原文不覆盖，通过本报告勘误。

四入口摘要的唯一旧`0 passed/39 filtered`位于CORRECTION说明，三个live result各1/0/38正确；没有把这条历史引用算为空过的新证据。

## 3. AF-02：限定方案与存量实验接受，原二进制回退不成立

Terra独立重放Python显式事务的12项观测与成功矩阵39项，均无失败。主控另复制真实SQLx装置，`--locked --offline`重跑，不改旧装置或产品：三处DROP后的故障均恢复完整schema、29张表的全行快照、全部11项旧拒绝行为，版本30成功记录为0、已登记版本仍29。裸/外置BEGIN的executescript负对照确有部分DDL，不当成安装路径。见 [SQLx日志](evidence/af-return-audit-2026-10-09/main/af02-sqlx.log)、[路由报告](evidence/af-return-audit-2026-10-09/route/REPORT.md)。

成功矩阵确认旧目标和override INSERT、UPDATE/REPLACE拒绝边界、Event先于立即FK、三次改派保留旧attempt真实角色/Binding/override快照。0028六对象18项留存判定在概念前后相同。**接受SQLx单migration、no_tx=false的正式方案方向；这不签生产编号，不是产品权限/事务/投递闭环。**

主控补做回退前提反例：在概念30已成功的库上，调用当前真实 `persistence_sqlite::migrate()`（只有1–29注册表），实得 **`Err(VersionMissing(30))`**。见 [独立增量patch](evidence/af-return-audit-2026-10-09/main/af02-old-loader-probe.patch)、[实跑日志](evidence/af-return-audit-2026-10-09/main/af02-old-loader.log)。

因此“旧二进制不认识新表/列但可以回退”不得成为可执行承诺。兼容性验收还须包含迁移登记/checksum、启动流程、旧查询/旧写入对新override历史的实际行为；**当前29版加载器已经被反例排除**。经另行审核的兼容二进制或前向修复仍是条件性方向，不能靠DROP守卫、删除迁移登记或擅设ignore_missing蒙混通过。多连接迁移锁、真实actor、八层权限、expected_version/TTL/幂等重放、业务Event/Outbox/dispatcher仍待对应已获准实现包。

## 4. 当前真正阻断：Adapter稳定性与Drop非空性

默认并行失败原始日志保留，单独精确过滤失败项也实得0/1，exit101；单target完整并行29/0、全仓串行653/0/1。已编译二进制另5次串行、4次并发为9/9绿，**这些绿不能抹掉前两次红**。命令/exit见 [rust-recheck.json](evidence/af-return-audit-2026-10-09/main/rust-recheck.json)。

真实失败链是子测试 `supervisor_fixture.rs:374` 的 `report.status.success()`，父在:142报告子测试exit101；不是6秒watchdogdeadline。本轮没有记录当时的ExitReport状态/termination/stderr，尚不能唯一归因于启动调度或生产进程竞争。无需依赖observability也不能把所有并行影响完全排除。

**P2：Drop分支缺少目标场景就绪确认。** `Supervisor::start()`后立即drop（:377–379），可在直接fixture创建pipe-holder前杀掉它；目前只证明一条可能为空的路径够快。shutdown分支成功时可知直接fixture已完成spawn，但仍缺后代真实就绪/终止观测。详见 [诊断报告](evidence/af-return-audit-2026-10-09/adapter-diagnosis/REPORT.md)。

接续只允许先补测试fixture readiness及终止观测，保留现有1秒grace/3秒总上界/6秒watchdog/10秒pipe持有设置。不把ForcedShutdown当正常成功，不删断言，不以加大时间掩盖失败。若真实生产缺陷被证明，再提交最小证据与建议写集由主控签发；DEV-03-02继续BLOCKED。

## 5. 主控整合秘密扫描例外，并修正全局路径豁免

首次tree扫描exit1：6条命中均为两份已知假base64 fixture副本和两份AF03机器输出中的断言标签，非真实凭据。全refs历史扫描exit0。但新增例外的负对照暴露了更重要的问题：本机Gitleaks8.30.0的**global allowlist路径会提前豁免整个文件**，其 `condition=AND` 没有在这些试验里实现预期的路径+内容联合限制。已有Figma/production-test例外同样受影响。

保留初始失败及诊断证据：[原findings（redacted）](evidence/af-return-audit-2026-10-09/main/secret-tree-findings.json)、[before/after反例](evidence/af-return-audit-2026-10-09/main/secret-scope-diagnosis.json)、[最小condition装置](evidence/af-return-audit-2026-10-09/main/secret-condition-probes.json)。没有把初版错误配置或负对照失败记为通过。

主控最终改为 `generic-api-key` **规则级** `[[rules.allowlists]]` 的路径AND完整行，另将已保存旧配置中的两个已知非凭据值限定到该快照/配置差异路径及完整匹配值，保留其他默认检测规则。既有Figma locator、production假fixture、精确AF副本和一个审计标签均按明确路径及完整已知内容限定；没有忽略整个evidence目录、整个rs/json文件或任意secret关键字。

最终19项正负对照全部符合预期：指定假数据/标签允许；相邻路径、改动内容、同一允许文件的新凭据模式均拒绝，包含production测试和Figma两个既有路径。见 [scope results](evidence/af-return-audit-2026-10-09/main/secret-scope-results.json)、[配置patch](evidence/af-return-audit-2026-10-09/main/gitleaks-config.patch)。**tree最终exit0、0 findings，全refs历史最终exit0；19项scope全部符合预期。** 见 [final-checks.json](evidence/af-return-audit-2026-10-09/main/final-checks.json)、[tree最终日志](evidence/af-return-audit-2026-10-09/main/secret-tree-approved.log)、[all-refs最终日志](evidence/af-return-audit-2026-10-09/main/secret-all-refs-approved.log)。初次和中间失败日志保留，归档配置/patch内的相同非凭据值也经精确值例外复核。这仍不是联网advisory或完整Mac门禁。

## 6. AF-03当前口径与后续顺序

38/38历史派生与登记一致，原R10 **15项按意图重建＋原M14不可编译/历史2tests未证**；新实验15＋M14-R **16/16断言检出**，两栏不混写。六对象留存、Run终态局部整改、FC05既有四unit可见性、FC06信任边界、FC09精确eslint已整合等既有结论保留。

AF03索引§2.4仍列AF01旧反例和AF02存量实验未做，§6.2仍写终值待定，这是收口时序过期。以本报告更新：AF01漏检已关闭、AF02存量概念联合实验已验证，Rust串行终值653/0/1；**默认并行652/1/1仍开**。FC05“时间裕度签收”不等于本轮发现的Drop非空性和稳定性被验证。

domain源码/测试/分母与先前测量相同，行覆盖沿用2016/2197=91.76%，不重复冒领新测量；region89.87%不当branch，application占位无可执行行不写100%，branch仍UNVERIFIED。canonical Mac21、新鲜在线advisory/install、真实Harness/Keychain、最低macOS14实机、A11y、性能、备份恢复、多线程真实产品日志链仍未运行。ADR0007通知决策和可信应用执行链未关闭。

下一步按 [Minimax精确接续任务表](../coordination/MINIMAXCODE-VERIFICATION-FOLLOWUP-2026-10-09.md) 先完成非空fixture/稳定性，再收口新证据。完整节点准入后才另签K1身份、K2已冻结通知身份、K3应用事务/幂等/Event/Outbox、K4dispatcher/Run/fencing及Workspace/IPC/UI任务。当前不启动这些依赖节点。
