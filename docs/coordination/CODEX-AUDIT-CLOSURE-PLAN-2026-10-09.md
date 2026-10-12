# Codex内部整改与审计收口计划（2026-10-09）

Human最新决定：暂停向Minimaxcode外派；当前多轮整改由Codex整理、调整和独立审验直至可通过。本决定替代本轮继续外派的执行安排，不追溯历史、不改变Frozen技术栈/领域语义、ADR0007、节点依赖或费用权限。历史Minimax任务表保留，但VF01/VF02不再作为外派开工指令。

## 验收目标

关闭本轮已发现的真实工程/测试/证据缺陷，形成可复现的当前输入离线审计PASS。完整产品准入另按G0–G9及阶段门禁判定；未实现应用链、未授权的在线/账号动作、真实Harness/Keychain、最低支持系统/A11y/恢复以及未验证branch不假写通过。当前20 DONE / 21 BLOCKED / 60 PLANNED不因一次套件绿而改变。

## 固定输入与保护范围

起点为AF主控最终输入，HEAD `1a14b8795e76fc373bef326eb5df96f9ac173f71`加继承dirty工作树；开工时工程输入与该终值0漂移。源码/文档/工具前快照和两份fixture原件保存在 `docs/reviews/evidence/codex-audit-closure-2026-10-09/main/`。

sources、旧迁移0001–0029、旧AF/FC/Minimax报告和证据只读；根manifest/lockfile不变。C1–C4离线，不提交/推送/部署/生产操作；C5联网按下方Human明确授权，本轮费用$0。任何正式语义调整先Human决定，普通修复不擅自扩栈。

## 当前台账

| 类别 | 当前事实 | 本轮处理 |
|---|---|---|
| `%` 单层编码漏检 | AF01主控39/0、原反例3/0、生成对照通过 | 保留修复，验证源码SHA不漂移；最长匹配已接受 |
| Route概念DDL与存量行 | SQLx三故障零副作用；39项联合矩阵通过 | 不写生产迁移；当前29加载器VersionMissing(30)排除旧二进制回退 |
| M14证据 | 原15+1不可编译/未证，新M14-R16/16 | 保持分列，不补造历史日志 |
| 资源/还原证据 | production去重界成立；M3/M4存活；副本与production同字节 | 不凑全杀；撤回错误二叉模拟，保留勘误 |
| Gitleaks allowlist | 已改规则级AND，19正负对照通过 | 保持精确例外；最终新输入复跑tree/all-refs |
| Adapter fixture稳定性 | 起点默认652/1/1、隔离0/1；C4及C5整改后默认653/0/1 | 已关闭；保留旧RED，预登记10＋3轮及C5完整Adapter3轮通过，不扩大时间 |
| Drop pipe-holder非空性 | 起点start后立即drop；当前已确认真实后代/继承pipes/唯一token/PID存在与退出 | C1/C2独立反例及还原已完成，不再作为待补项 |
| AF输入/回退与索引时序 | HEAD不等于dirty输入、AF03开放表过期 | 新当前索引统一，历史原件不覆盖 |
| 完整阶段门禁与产品链 | C5完整Mac21已整体PASS；branch/真实Harness/产品链仍有缺口 | 具体分列，禁止用工程PASS解除完整节点BLOCKED |

## 内部分工与顺序

| 工作 | DEV与难度 | 执行者 | 允许写集 | 验收 |
|---|---|---|---|---|
| C1 修复真实pipe-holder fixture与观测 | DEV-03-02 / DEV-01-05，Terra级进程测试/诊断 | 内部Terra（复用af_route_review） | `crates/adapter-supervisor/src/bin/fixture_adapter.rs`、`crates/adapter-supervisor/tests/supervisor_fixture.rs`、新 evidence `implementation/` | 原1s/3s/6s/10s不变；真实readiness；direct成功退出/termination、reader与后代退出；target测试/fmt/clippy |
| C2 独立测试设计及完成后源码复核 | DEV-03-02，Terra级独立审核 | 内部Terra（af_matcher_review，未写C1源码） | 仅新 evidence `independent-review/` | 验非空性/身份关联/无遗留；当前副本真实断言RED/GREEN，不能用编译失败/空filter |
| C3 多轮台账与证据一致性盘点 | DEV-01-05/01-08/02-08，Luna级整理 | 内部Luna（af_evidence_review） | 仅新 evidence `inventory/` | 问题→修复→反例→证据→剩余门禁映射、旧材料保护；不动当前入口/正式状态 |
| C4 根整合与最终门禁 | 涉及节点最终审验，R3主控 | Codex主控 | 计划/当前入口/新审计；生产源必要扩写集由主控明确签发 | 串行整合，默认/单线程结果分列，全量构建/静态/测试/扫描；最终前后SHA一致 |

每轮最多3个内部Subagent，源码单写者；主控独立复核后才接受。C1实现与C2设计、C3盘点可并行；C2修改后审查及C4最终门禁等C1源码稳定。全仓门禁只由主控跑，避免并行构建掩盖故障。

## 修复与验证方式

1. 先登记旧真实RED（AF默认及隔离日志），追加能确定发现“后代未形成/就绪不真实”的失败测试或独立装置。
2. fixture确认目标后代实际启动并持有pipes；shutdown和Drop均等确认，再执行被测动作。采用唯一token/独占临时资源，记录身份和终止。禁止新固定sleep替代同步。
3. 保持已有关闭预算。就绪前启动预算与关闭预算分开且如实记录；不能悄悄丢掉已有总预算。正确判断直接子进程与后代回收责任，不宣称平台可杀任意主动脱组后代。
4. 在确定写集内最小修复。若真实生产supervisor缺陷出现，主控根据反例明确补充允许修改范围后修，不以“只读范围外”无限往返。
5. 独立审核无参与实现的测试空过、死锁、FD/PID复用、伪造marker、孤儿泄漏、错误reader状态与失败清理。
6. 固定重复计划：失败用例隔离10次、完整fixture默认并行3次（含一个干净构建/冷启动）、全workspace默认及串行各1次；首次失败即诊断修复，保留全部尝试，不重复直到绿选样。新改动后只重跑受影响检查，再进行最终整合轮。
7. 当前稳定输入执行locked/offline Rust test/build/clippy/strict rustdoc/fmt；TS test/build/lint/typecheck/contracts/guards/workflow/Gate/replay；本地secret tree和all-refs、例外负对照及旧证据SHA。已有工具可做的本地许可证检查记录结果；未授权的在线advisory不调用。
8. 汇总PASS/FAIL/UNVERIFIED和适用性，不用数字总和替代具体危险断言。当前源码、测试集、工具/配置和原始日志绑定；已关闭事项不循环派发。

## 收口条件

整改批次离线PASS必须：C1/C2独立验收、默认和串行全仓无失败、静态/构建全绿、秘密扫描/范围负对照通过、旧证据无非授权漂移、当前索引无虚假准入/回退/覆盖声明。未达到则继续本轮内部修复审验。

如果唯一剩余条件需联网/账号/费用、真实设备或Human未决语义，先完成全部可做工作并给出具体材料，再提出所需授权/决策；不能在没有运行的情况下宣布完整项目可发布。本表不自动批准这些外部动作。

## C4追加最小写集（主控串行）

最终Gate单测实得24/1：原测试硬编码三个global allowlist，与上轮主控修正到rule级AND后的配置冲突。允许主控只修改 `scripts/ci/local-gates.test.mjs`：校验全局仅生成目录、规则级路径+完整内容及AND，不把数字3改成1凑绿；加入全局文件豁免/OR/宽内容反例。保持canonical21、其他生产脚本和`.gitleaks.toml`不变。修改后重跑Gate25、相关fmt/lint/typecheck/TS/Gate/replay/guards及剩余检查；Rust源码/锁/工具未变，不重复已经通过的两轮Rust全量，保存输入差异与分项证据绑定。

## 当前结果

C1–C4本轮offline整改已经通过限定审验，见 [收口报告](../reviews/CODEX-AUDIT-CLOSURE-2026-10-09.md)。默认和单线程Rust653/0/1，10次聚焦＋3轮完整fixture（含冷启动）均绿，两独立反例真实RED并恢复绿，Gate旧硬编码已修为property guard并25/0；TS36/0、replay8/0、静态/构建/本地扫描及840项保护检查通过。C5完整Mac21后续已通过，branch、真实产品链及Human未决事项按报告第6节继续，正式节点不晋级。Minimax外派继续暂停。

## C5联网权限

Human于本轮问答明确选择“允许联网执行”，授权当前输入完整Mac21、npm registry/RustSec公共依赖安装与新鲜漏洞检查；费用继续$0、使用现有锁与工具、不提交/推送/部署。执行和日志置仓库外持久目录，避免改变受检输入；权限不扩展到生产账号/真实Harness/发布或未决语义。

## C5发现与主控最小补丁写集

首轮Mac21实际前17项通过，第18项因本机npmmirror没有audit端点而失败，原始gate.json/rawlog保留。执行级指定`npm_config_registry=https://registry.npmjs.org`，不改全局配置、canonical21或严重度阈值。官方接口随后检出`source-map-js 1.2.1`高危`GHSA-68fv-2mgg-jv7q`，补丁版1.2.2已修复。

该已确认缺陷属于Human授权的“调整、审计直到通过”范围；主控增加唯一工程写集`pnpm-lock.yaml`，仅更新该传递依赖的版本、官方integrity及相应snapshot引用。保持package.json、其他包版本、Cargo.lock、工具链和Frozen语义不变。先在隔离副本验证官方包元数据和frozen安装，再串行整合并完整重跑Mac21。此前“不改根锁”只描述C1–C4离线批次，不能被用来豁免这次真实高危漏洞；不把旧离线结果冒充新锁输入结果。

普通`pnpm update --recursive`的隔离候选牵动其他包且命中副本目录默认Node22，已排除整包采用，保留日志和候选；最终仅采纳官方核证的1.2.2条目，由Node24.21.0/pnpm10.33.0的frozen安装验证。npm另有ajv中危和esbuild低危待评估，RustSec无漏洞但维护/unsound告警如实保留，不扩大更新或冒称零告警。

G9还发现`closure-ledger.final.json`仍留有C1/C2 pending及未更新的在线权限文字。原文件作为过程证据保留，另生成`closure-ledger.current.json`并切换当前入口；机械保护验证与语义状态核对分列，避免checksum通过被误解为台账内容已同步。

C5第二轮实际在Rust lib测试捕获`node_mock_adapter_fault_crash`竞态（63/1，exit101）：stdout EOF后`try_wait()`合法地尚无退出状态，测试却要求瞬时Some。主控再增加仅`crates/adapter-supervisor/src/lib.rs`中该测试及对应cfg(test) import的写集；不修改生产实现或协议。既有错误码/重试语义仍验，立即快照若有状态仍需42，最终必须经原API有界wait取得自然退出42、完整crash诊断及两reader EOF；原2秒请求和5秒watchdog保留，新增1秒wait包含在该watchdog内。独立Terra复核当前完整crate副本，验证错误退出码、诊断缺失、EOF但存活三个反例真实断言RED并还原绿；预登记修复后完整crate默认并行3轮，不用单线程凑绿。此轮变化后重新执行完整Mac21。

C5第三轮canonical21全部exit0，最终判定仍FAIL：Gitleaks实际`1:22PM`与`1:25PM`前缀未被归一化，导致同一全refs扫描误报摘要漂移。允许主控增加`scripts/ci/local-gates.mjs`的日志前缀归一化及既有测试文件的新行为测试；只处理行首合法12小时钟且后接已知logger级别，保留级别、findings、commit数、扫描字节及payload内部时钟。新增测试先0/1真实RED，再修为绿；旧25项保留，新全量26项及replay8项复验，canonical命令数、参数、threshold、input/refs/companion核对不改，再执行最终完整Mac21。当前结果不能将21个命令exit0冒充整体门禁PASS。


## C5结果与下一步内部任务表

本轮整改＋完整canonical Mac21整体PASS：最终gate-accepted.json 21/21及前后companion通过，4676文件全输入运行前后0漂移；Rust653/0/1、TS36/0、Gate26/replay8，Adapter默认3轮93/0；独立crash与clock反证均完成。具体rawlog、输入SHA、告警和审查报告见收口报告第7节。不新增正式DONE，Minimax外派继续暂停。

| 顺序/任务 | DEV节点 | 负责与可执行范围 | 准入/交付要求 |
|---|---|---|---|
| C6 正式节点差距核对 | DEV-01-05/01-08、DEV-02/03已交付阻断节点 | Codex主控＋边界明确的内部Terra审核；先只读逐项规格/G0–G9矩阵 | 明确每个BLOCKED为行为缺陷/未实现/未验/未决及唯一owner，不按一次Mac绿批量晋级；存量正式表保持20/21/60直到逐节点正式签核 |
| C7 覆盖率有效证据方案 | DEV-02-11～14、DEV-05及Gate C | Codex内部调查现有冻结工具的能力和实际分母；不安装nightly或用region冒充branch | domain既有行91.76%与当前源绑定；branch要求85%仍未验证，application占位不写100%；若需变更Frozen工具路径，提供具体ADR供Human决定 |
| C8 通知未决语义 | DEV-02-09、DEV-05-08 | Human先裁ADR0007，Codex记录并在明确接受条款内实现 | 当前Proposed，七条含Owner/filters/escalation/强制通知/Run/Message/SYSTEM；不从一般整改授权推断接受，不签生产0030或旧二进制回退 |
| C9 执行点与事务链 | DEV-02-13/14、DEV-03-04～07、DEV-05 | Codex按前置逐项拆分，源码单写者、Terra级独立审核；root串行整合锁 | 真实actor/权限、claim/幂等、Event/Outbox、旧token、失败回滚与Run取消；先满足节点依赖，不用纯函数/概念SQL替代真实执行点 |
| C10 真实平台与阶段门 | DEV-02-15、DEV-03-08～11、DEV-07/08 | Codex＋Human提供所需设备/账号明确授权；先制定可复现用例与隔离数据 | 备份恢复、性能/A11y、最低macOS14、真实Harness/Keychain/桌面AC主链逐项真实证据；当前开发机工程门禁不替代它们 |
| 维护风险评估 | DEV-01-05/08 | Codex内部按实际目标评估中低危和信息性告警，不扩散依赖更新 | ajv未启用$data；esbuild为Windows dev-server；glib不在当前Mac树。零ignore新增，非Mac不作已验证声明，补丁修改后重新跑受影响及正式门禁 |

本表为内部接续计划，不是恢复外派、发布授权或跳过节点依赖。审计输出（报告/入口/current台账/交付清单）在正式测试后写出，逐项登记与受检输入差异；工程源、工具、manifest/锁、旧迁移及旧证据均保持最终受检状态。
