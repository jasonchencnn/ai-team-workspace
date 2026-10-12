# 全项目深入审计与复核结论（2026-10-08）

本轮审计已完成当前交付范围的扫描、分片复核、常规检查、独立反例及依赖判断；**验收结论 FAIL，不能声明 MiniMax 交付全部无误或进入产品阶段。** 下一步应先执行已签发的既有缺陷整改包，再经 Codex 复核解除依赖阻断。审计完成不等于所有功能和门禁通过，也不是逐行无缺陷保证。

## 1. 输入、分工和范围

HEAD 为 `1a14b8795e76fc373bef326eb5df96f9ac173f71` 加继承工作树。审计输入共 299 个非忽略文件：生产/工具 116、测试 53、生成/构建材料 3、根配置 14、参考资料 103、归档参考 10。历史/current evidence 内复制源码不重复计产出；所有原始资料和旧 evidence 只读。代码、24 份 migration、manifest/lockfile 在检查期间未变；状态/报告更新在检查结束后单独登记。

| 分片 | 模型 | 关联 DEV | 范围及主控复核 |
|---|---|---|---|
| 权限、审批、cache、fencing | gpt-5.6-terra | 02-10/13、03-07 | F2～F5 修复独立核查；生产调用链与最终落库边界 |
| Message/Notification SQLite | gpt-5.6-terra | 02-08/09 | 六份对应 migration 的地址、状态、快照、留存和索引 |
| Adapter/Mock | gpt-5.6-terra | 03-01～07 | stdio/schema、sequence、取消/进程资源；主控重放 stdout 反例 |
| 工程/前端/契约 | gpt-5.6-terra | 01、03-01、06-01 | CI/本机 Gate、输入清单、生成物、壳与占位测试 |
| 核心持久层 | gpt-5.6-terra | 02-02～07/10/14 | 其他 migration、历史、Run/Review/Artifact/Approval、幂等/Outbox |
| 节点/文件/证据盘点 | gpt-5.6-luna | 全部 | 101 节点、直接依赖、299 文件、链接/证据路径机械核对 |
| 全局整合和第二遍 R3 | GPT-6 系列总控 | 全部 | 当前 24 份迁移重放、SQLx 新失败回归、脱敏/输入反例、状态/计划 |

最多同时 3 个 Subagent，写集为各自新 evidence 子目录；未修改业务源码或旧迁移。Terra 承担有界工程/安全独立复核，Luna 不判断安全结论，主控逐项收敛 R3。没有调用 MiniMax 或向外部会话发送消息。

## 2. 验证结果与可信边界

| 检查 | 本轮实测 |
|---|---|
| Rust workspace locked/offline | 503 passed / 0 failed / 1 ignored，61 份结果汇总 |
| TypeScript 全部现有分层 | **36 passed**，六组为 5/1/21/1/4/4；部分为工具链占位 |
| local-gates 脚本测试 | 12 passed |
| build、clippy 全目标全特性 -D warnings、fmt、严格 rustdoc、TS lint/typecheck、contracts、guards、工作流、许可证/unsafe | 本批离线检查共 24 条，全部 exit 0；不是 canonical Mac 21 项 |
| Gitleaks 工作树 / 全 refs 历史 | exit 0、0 命中；非 shallow，6 refs / 4 commits；二进制 SHA 与上轮相同 |
| RustSec/许可证离线缓存 | exit 0；公告库非本日新鲜，index 锁不可读告警仍在，不能等同在线零漏洞 |
| 新 SQLx/项目 bundled SQLite 回归 | **1 合法对照通过、3 行为断言失败**：跨 Team DRAFT、Run 倒退、五类无引用历史删除 |
| 新脱敏回归 | **1 合法对照通过、2 行为断言失败**：大小写混合百分号转义 |
| 值对象追加边界 | 2 passed；四位年份每月首末日/越界日、UUID 实际随机位布局 |
| domain 行覆盖率 | **1907/2197 = 86.80%，低于冻结90%**；16份src，249个instrumented用例通过；application占位无可执行行，不能写100% |
| input manifest 忽略环境文件 | 隔离无提交仓库中，两份不同 `.env.production` 得同一 hash；盲区确认 |
| Adapter stdout 独立复现 | 非 JSON 前缀 + 合法响应，与仅合法 JSON 对照都被成功握手 |
| ErrorEnvelope schema 实验 | 合法对照及未知 code、错 retryable、无效 correlation、越权 details 均被接受；仅证明当前 transport schema 无语义校验 |

覆盖率没有停在缺工具结论：虽然没有 `cargo-llvm-cov` 和配套Rust LLVM tools，但现有Apple LLVM21通过RustLLVM22的实际profile/covmap探针，已离线完成domain行/region测量，无下载。范围为当前同字节domain/application src，现有domain测试加本轮两个边界测试，排除tests/examples/dependencies；application为空不计产品覆盖。**region 84.75%不是分支覆盖率**，稳定编译未产生branch映射，85%分支门槛仍未验证；全体Rust/TS80%也未测量。没有当前完整在线 Mac 21 项/干净安装、最低 macOS 14 实机、业务桌面 E2E、真实 Harness/Keychain、备份恢复、性能或完整 A11y 结论。相关能力缺失时不能造成功能测试。

测试装置的初始两处列名/字段顺序错误以及一次 Python 注释切分错误已记录，不计为产品缺陷或有效失败证据；正式 SQLx 结果取 `storage-boundaries-red-v2.log`，均为成功编译后的行为断言失败。旧 /tmp 的权限测试记录不再计独立可核验结果，改引持久化统一 Rust 日志。合法对照、实际行数、FK=ON、当前全部迁移和日志/源码哈希均保留。

## 3. 总控确认的实现与输入缺陷

| ID | 优先级/风险 | 事实及冻结依据 | 验收证据 | 主责节点 |
|---|---|---|---|---|
| DA-01 | P1，隔离风险 | `0017:88` Team guard 只有 INSERT；`:185` 地址锁跳过 DRAFT。DRAFT 改跨 Team Role 后可 QUEUED，并创建该错误路由的 attempt。违反 DOMAIN_MODEL Team 隔离、PERMISSION_MATRIX §3 | Python 当前 24 迁移重放及 SQLx DRAFT 更新失败回归 | 02-08 |
| DA-02 | P1，留存风险 | Run/Message/Artifact/Approval 的删除防线部分只在 Event/Reference 等引用存在时生效；无此引用的行可删，PENDING Approval 旧测试还明确允许 DELETE。Review 决定也可删除。DATA_CONVENTIONS §5～6 要求历史/索引随 Team 保留 | SQLx 独立同 Team 种子逐个删除 runs/reviews/artifacts/approvals/messages，五表均实际 1→0 | 02-05/06/08/10 |
| DA-03 | P1，状态风险 | `0012:110～162` 只禁止终态复活，允许已有 RUNNING→QUEUED 并清 started_at。STATE_MACHINES §3 禁止重置旧 Run，重试必须新建 | 先合法 UNKNOWN→RUNNING，再尝试 QUEUED；SQLx 确认非法边成功 | 02-05 |
| DA-04 | P1，脱敏风险 | `observability/src/lib.rs:572～604` 将整个 percent 字符串小写，误改未转义字母；保留原字母大小写的合法 `%2d/%2f` 混合形式泄出已注册模拟 secret。违反 TECH_STACK §7/§11 与 DEV-01-08 | `observability-percent.log`，两条失败断言、plain/大写转义正对照通过 | 01-08 |
| DA-05 | P2，协议风险 | `protocol.rs:1816` 等五个入口跳过 `running `/`test `/空白 stdout。PROTOCOL §2 要求每行完整 JSON-RPC，诊断属于 stderr | 主控 malformed/control 两次握手；子报告列齐五处 | 03-03，关联03-02/04/05 |
| DA-06 | P1，证据风险 | `local-gates.mjs:315` 仅收集 Git 非忽略文件，漏掉 Vite 会读取的 `.env*`。不同外部构建输入仍可有同源码 hash，削弱 ADR-0005 同输入证明 | `env-manifest-replay.json`；没有读取真实环境文件或秘密 | 01-05 |

这些是已复现的库/存储/验证缺口，**没有宣称已发生生产泄密、越权、删除或双主执行**。业务入口本身尚未接通。DA-02 不能夸大为“所有 Run/Message 都能删除”：`0014` 的 Event subject DELETE guard 和 `0013` 的 Reference guard 已存在且会保护有关联的行。整改需同时保留其现有效果、保护合法升级历史，并保证业务/Event 原子关联，而不是删除既有守卫。

## 4. 机制缺口、待开发能力和待决事项

| 项目 | 分类与结论 | 后续责任 |
|---|---|---|
| 全 refs 扫描没有随 fixed 21 自动落进 Gate 证据 | 验证机制缺口；本轮另跑全 refs 0 命中，**不等于当前存在秘密**。ADR-0005 允许固定 21 加配套历史证据，不需要把 canonical 命令数随意改成 22/23 | 01-05：将补充检查与 scope/hash 自动关联，独立审阅证据完整性 |
| verify:guards 没有自动接入 CI/local fixed chain | 自动化加强项；本轮该命令已实跑通过，不能因此断言当前 G2 失败 | 01-03/05：在独立补充 required 阶段执行，保留 canonical 21 |
| ErrorEnvelope String/任意 details | 尚未实现错误目录语义 validator/builder；实验确认 broad schema 接受目录非法组合，但 Rust→Schema→TS 无漂移本身已通过 | 在应用错误边界/05-11 前落实 catalog 单源、retryability、UUID、details 脱敏白名单；不把通用 transport DTO 自动判作01-07失败，也不补造未知字段兼容规则 |
| 权限/cache/grant/fencing helper | F2～F5 及上批零世代/终态/UNKNOWN 修复成立，58 个相关用例在本批统一日志通过；无生产外部调用，不能证明执行点已授权 | 02-13/14、03-06/07：真实 resolver、拒绝 Event、原子事务、当前 DB 世代/租约/sequence |
| Message route override | 冻结 §5 要求改派后新 attempt 与 route override，当前无专用历史载体，attempt 又须等于原 to_role。直接改不可变原目标本就应被拒；子反例目标未完整建立有效 Binding，**不能把它计为合法改派被错误拒绝** | 02-08/05-07：设计追加路由历史，原 Message/旧 attempts 保留；与权限/事务联测 |
| Attention DELETE | OPEN 索引实际可消失，但 Attention 是可重建派生索引，DATA 留存表没有单列它。不能只以“没有 DELETE 状态边”推出永久禁止所有底层删除 | 应用层不得把删索引当解决源事项；明确受控重建/处理审计的边界，必要时补决策，暂不计 DA-02 永久核心历史集合 |
| Notification options、mandatory、SYSTEM 解决 | 0023 局部存储修复保留；filters/escalation/真实 Owner/投递与解决事实仍缺失，ADR-0007 Proposed | Human 决策后分任务实现；不得用本轮改派路由授权冒充接受通知语义 |
| 应用、安全存储、Workspace、真实 Codex、业务 UI、AC-01～12 | 当前占位/未实现。四种根 TS 测试层的命令存在不能外推成四种产品验证；integration/e2e 的恒真断言不计验收 | 按下一节/任务表冻结依赖推进，不能跳过核心整改先批量造业务页 |

## 5. 文档/证据纠正与节点准入

原项目报告 TS 37、整改报告 TS 38 是总控汇总口径错误：对原始日志逐组求和实际分别 **35、36**，当前仍为 **36**。原始日志和历史报告保持，纠正见本报告、`ts-count-correction.json` 与 WORKLOG 追加；不将该错误归给 MiniMax。

机械盘点还确认：一个归档本地死链、一个历史本地 provenance 引用缺失；DEV-02-08 外部 resign-final JSON 仍不存在，不能伪造。旧 r13 不覆盖当前输入。UUID 生成器文档把被 variant 屏蔽的字节写成 `random[1]`，实际是 `random[2]`；追加边界测试确认实现正确，这属于文档订正。TaskTransition 注释的“先 persist 再验证前置”不能作事务实现顺序依据，应先校验再条件写入。

原盘点 34 DONE / 7 BLOCKED / 60 PLANNED。本轮直接重开 01-05、01-08、02-05、02-06、02-10、03-03；按真实依赖暂停 02-11、02-12、03-02、03-04、03-05。当前正式准入表更新为 **23 DONE / 18 BLOCKED / 60 PLANNED，共 101**。这是文档/依赖状态，不是产品完成率；纯机、进程/Mock和旧门禁成果仍保留，依赖暂停不表示其代码全部失效。

## 6. 下一步及交付入口

Human 本轮已明确选择 Minimaxcode 接续；[ADR-0008](../adr/0008-minimaxcode-remediation-routing.md) 仅记录该最新路由，未改变栈、语义、门禁、费用或依赖。由 Human 转发任务包，Codex 继续做独立复核与正式节点判定。

先做 DA-01～06 和必要的配套门禁、证据修复，再处理 ADR-0007/route override/合法索引重建边界，最后接真实权限事务、Outbox、取消/重试/回调和恢复。后续 Workspace/IPC/UI/AC/发布保持条件排期。具体任务、写集、RED/GREEN、先后和停止条件见 [Minimaxcode 总交接表](../coordination/MINIMAXCODE-TASKS-2026-10-08.md)，只需转发该文件；子包在同目录签发。

证据入口：[机器摘要](evidence/deep-audit-2026-10-08/summary.json)、[完整范围/节点](evidence/deep-audit-2026-10-08/inventory/inventory.json)、[检查命令](evidence/deep-audit-2026-10-08/offline-checks-summary.json)、[SQLx 失败回归](evidence/deep-audit-2026-10-08/storage-boundaries-red-v2.log)、[脱敏失败回归](evidence/deep-audit-2026-10-08/observability-percent.log)、[反例副本](evidence/deep-audit-2026-10-08/main-repro/)、[忽略输入反例](evidence/deep-audit-2026-10-08/env-manifest-replay.json)、[行覆盖率明细](evidence/deep-audit-2026-10-08/coverage-tools/domain-coverage-summary.json)、[源输入稳定性](evidence/deep-audit-2026-10-08/input-stability-after-checks.json)、各分片目录及交付 manifest。

本轮没有修改业务实现、历史迁移、manifest/lockfile、生产凭据/数据，没有提交、推送、部署、外部服务调用或费用操作。新失败回归位于隔离审计 workspace，作为下一批修复的 RED 输入，不伪装成当前正式套件已加入的新通过测试。
