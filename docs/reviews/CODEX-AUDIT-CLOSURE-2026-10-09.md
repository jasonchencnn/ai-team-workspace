# Codex内部整改与独立审计收口（2026-10-09）

**本轮已交付整改批次及完整canonical Mac21工程门禁：PASS。完整项目/正式节点准入：仍BLOCKED。**

Human要求暂停向Minimaxcode外派，现已登记到AGENTS.override与当前入口。此前R十包、FC九包、AF三包的交付和历史失败记录保留；本轮由Codex内部修复并独立复核，未再签发或发送Minimax任务。执行计划：[内部收口计划](../coordination/CODEX-AUDIT-CLOSURE-PLAN-2026-10-09.md)。

当前 **20 DONE / 21 BLOCKED / 60 PLANNED** 不变；ADR0007仍Proposed。PASS仅指C1–C5实跑的整改与完整Mac21工程范围，不表示真实产品链、最低支持系统、备份恢复、性能/A11y或完整阶段门禁已经通过，不启动依赖节点或发布。

## 1. 这次调整了什么

C1–C4离线批次代码写集为三个文件；C5追加写集和当前SHA见第7节：

| 文件 | 改动与理由 | C4完成时SHA256（C5后续见第7节） |
|---|---|---|
| [fixture_adapter.rs](../../crates/adapter-supervisor/src/bin/fixture_adapter.rs) | 后代本人通过继承stdout/stderr发布同一JSON-RPC ready；完成临时文件后用hard_link原子发布，拒绝覆盖既有ready；父进程收到stdin EOF才正常退出 | `6268468dd348687c33a7a0013c0a9a122a82d5f8cd09f0667eead80e6cbfd778` |
| [supervisor_fixture.rs](../../crates/adapter-supervisor/tests/supervisor_fixture.rs) | shutdown和Drop都确认目标后代真正形成；record绑定token/parent/holder、PID正数且不同、真实stdout与record一致；关闭前两PID存在、关闭后两PID消失；shutdown还断言正常退出、stderr与reader终态 | `debbd39610f2ba870a318555d0bd7512f5a8564b3fcbd67906710e63a1e61580` |
| [local-gates.test.mjs](../../scripts/ci/local-gates.test.mjs) | 去掉“三个global allowlist”的脆弱计数，检查全局仅生成目录、文件例外在规则级、路径与内容联合AND及锚定；增加移回global、OR、宽路径反例 | `708a15358912e322e7559200fc84a3cf24caf98773c6305025466e0423ed838d` |

原 **1s grace / 3s总上界 / 6s watchdog / 10s pipe持有** 全保留。3秒从实际启动前开始，包含两次形成目标场景和关闭，不将启动耗时隐藏到预算外。文件发布轮询仅用于有界同步，10秒sleep是被测pipe持有场景本身，不是为修复竞态新增固定等待。

PID观测封装为macOS专属测试；独占0700临时目录仅由本次测试管理，失败时SupervisedAdapter Drop与资源RAII处理清理。ready文件是测试同步，**不被宣称为生产actor认证**。`/bin/kill -0`仅证明存在/消失，未被宣称为OS父子/PG测量；真实创建路径仍是现有supervisor独立进程组和fixture spawn。Drop没有ExitReport，按动作前后的实际PID观察验收；shutdown另检查stderr与reader结果。ADR0006关于任意主动脱组恶意后代的能力边界不变。

C1–C4期间根manifest/lockfile、生产supervisor库/协议、domain与observability、数据库迁移0001–0029未改。C5仅因新鲜npm审计的真实高危漏洞追加pnpm-lock补丁，不改manifest或其他依赖版本。另更新执行安排和入口，新增计划/台账/证据。所有变异在独立副本，C1–C4为离线验证；C5按Human明确授权联网，费用均$0，未提交/推送/部署/生产操作。

## 2. 修复过程中的反例均保留

本轮没有用之后的绿覆盖之前的红：

- AF旧默认全仓652/1/1及隔离0/1仍在原目录，是进入本轮的真实RED。
- C1初稿补marker/PID后仍可能读到未完成文件；仅marker不能证明继承pipes，父可伪造自己PID。C2旧独立变异实得父伪造存活，旧结论BLOCKED保留。
- 主控补原子发布、两pipe实用证据及不同PID关联。尝试`ps`观测时沙箱拒绝，失败日志保留；最终采用原已用的局部PID探针，无额外权限，未虚称已经运行PG查询。
- **上轮主控自己的遗漏**：整合Gitleaks到规则级后没有重跑相关Gate单测。首次最终门禁本次实际24/1，旧测试硬写3个global，与安全修正冲突。本轮补属性断言及反例后25/0，不能把之前25/0误作最终配置同输入通过。

原始结果：[fixture初次主控整合失败](evidence/codex-audit-closure-2026-10-09/main/fixture-root-integration.log)、[最终真实fixture绿](evidence/codex-audit-closure-2026-10-09/main/fixture-root-final.log)、[Gate真实24/1](evidence/codex-audit-closure-2026-10-09/main/gate-tests.log)、[Gate最终25/0](evidence/codex-audit-closure-2026-10-09/main/gate-tests-final.log)。

## 3. 内部分工与独立复核

按AGENTS路由复用两名Terra任务与一名Luna机械盘点，最多三并发，源码由C1单写者改动后转主控串行整合。root负责保护快照、判定反例、扩展唯一必要门禁测试写集及最终门禁；内部委派与最终准入分离，不再外派Minimax。

| 工作 | 结果 | 证据 |
|---|---|---|
| C1 fixture实现与根整合 | 范围内最小调整，当前源码已稳定 | 两源码patch、原件/过程快照、最终source SHA |
| C2独立完整crate副本 | 最终scoped PASS；production/snapshot/恢复副本逐文件一致 | [最终独立报告](evidence/codex-audit-closure-2026-10-09/independent-review/final/C2-FINAL-INDEPENDENT-REVIEW.md) |
| C2变异1：quiet_output=true | 编译成功，0/1、exit101；stdout继承断言拒绝 | 独立final目录patch与rawlog |
| C2变异2：父伪造/无holder | 最新合法三字段record及完整JSON-RPC，0/1、exit101；**holder_pid != parent_pid**精确拒绝 | 同上，旧版存活事实另存 |
| C2还原 | 两次1/0/28filtered绿；C4验收时production/snapshot/恢复源码共16项比较无差异；本地关键依赖版本与root相同 | [主控实际副本核对](evidence/codex-audit-closure-2026-10-09/main/c2-restored-independent-check.json) |
| 门禁policy独立只读复核 | 当前guard保留默认规则、canonical21和有意义反例；不是CLI语义的替代证明 | [独立复核](evidence/codex-audit-closure-2026-10-09/independent-review/gate-policy/DEV-01-05-GITLEAKS-POLICY-REVIEW.md) |
| C3历史台账 | root修正误把旧故障列开放、误用AF942项代替本轮840项快照等草稿错误 | [当前定稿](evidence/codex-audit-closure-2026-10-09/inventory/closure-ledger.current.json)、[机械验证](evidence/codex-audit-closure-2026-10-09/inventory/verification.final.json) |

working crate为独立workspace中的完整adapter-supervisor，未手抄matcher或片段测试。其独立解析lock另存，原root lock保留验证，root lock字节未变；关键Tokio/runtime依赖版本一致。真实变异不是编译失败或0test filter，恢复SHA打在实际保留副本上。

## 4. C4离线实跑结果（C5最终完整门禁见第7节）

当前数字从rawlog求和，不按交付自陈或“应当增加N条”硬编码：[summary.json](evidence/codex-audit-closure-2026-10-09/main/summary.json)。

| 检查 | 当前结果 |
|---|---|
| Rust默认workspace `--locked --offline --no-fail-fast` | **653 passed / 0 failed / 1 ignored，exit0** |
| Rust workspace `--test-threads=1` | **653 / 0 / 1，exit0** |
| 聚焦pipe-holder预登记重复 | **10/10绿**，全部独立原始日志 |
| 完整fixture默认并行 | **3/3轮绿，每轮29/0**；含全新构建目录冷启动一轮 |
| TS完整套件 | **36/0** |
| Gate / 环境replay | **25/0 / 8/0** |
| Rust/TS build、fmt、lint、typecheck | exit0 |
| workspace clippy all-targets/all-features `-D warnings`、四业务crate strict rustdoc | exit0 |
| contracts、guards、workflow、unsafe、工具链检查 | exit0 |
| Node许可证 | exit0，196个已安装包；84个其他平台可选路径缺失按规则列出 |
| Rust许可证locked/offline | exit0；3条unused-license-allowance警告留存，不写成零warning |
| Gitleaks树/全部refs历史 | exit0，无findings；真实非shallow，refs/工具/配置记录 |
| Gitleaks规则范围真实CLI正负对照 | **19/19符合预期**，指定假数据允许、相邻/改动/同文件新凭据模式拒绝 |
| 本轮旧保护文件 | **840项，missing0/字节漂移0** |
| diff与新链接 | 收口时校验，无新增错误 |

命令/exit：[首轮完整尝试](evidence/codex-audit-closure-2026-10-09/main/final-gates.json)、[修正后的相关检查](evidence/codex-audit-closure-2026-10-09/main/final-rechecks.json)、[稳定性预登记与结果](evidence/codex-audit-closure-2026-10-09/main/stability-summary.json)、[扫描范围反例](evidence/codex-audit-closure-2026-10-09/main/secret-scope-results.json)。首轮Gate失败后只改变Node门禁测试文件，Rust源码/manifest/锁/Mock输入/工具未变，两轮Rust证据仍绑定同一Rust输入；修改后的相关Node检查及剩余检查全部补跑，前后快照一致，见recheck-reason与validation-drift。

上表为C4 offline整改证据，未重新命名为canonical Mac21。C5另按原runner完整实跑21项及前后companion，结果见第7节。

## 5. 多轮材料整理后的当前口径

1. `%` 单层编码漏检：AF01已关闭，最长合法匹配接受；当前observability两SHA不变，完整39测试亦在本轮全量通过。生成式拼写/负对照为AF当时的已保存证据，不冒领本轮新测量。
2. 六对象留存/Run终态、五入口stdout/ignored .env输入：已局部闭环，不继续重复列为当前故障；可信创建/权限/应用事务等未完成的责任另列。
3. Route：AF02存量概念与SQLx原子性已经验证；当前29版加载器对概念30报VersionMissing(30)，旧二进制直接回退不可用。未签生产迁移或产品执行链。
4. 原R10为15项按意图重建+原M14不可编译/历史2tests未证，新M14-R实验才16/16。M3/M4存活不算kill，“无条件二叉指数”指控已撤回，资源等价不作无依据外推。
5. HEAD9251/fcdf不等于AF修复前dirty输入；真实d945/f843已有副本，checkout HEAD回退作废。源码重建的Gate旧测试逐字节匹配本轮修改前SHA，明确标作重建源，不补造旧stdout。
6. domain仍沿用**2016/2197=91.76%**，源码/分母/测试/工具未变；region不当branch，branch仍未验证，application占位不写100%。本机stable拒绝`-Z coverage-options=branch`的能力探针已记录，没有换栈/下载工具或凑分支数字。
7. 旧AF942项与当前840项是不同保护清单；C3错误草稿保留并由定稿README明确作废。FC615项旧清单中的两个现行README由主控授权更新，其他项重新校验无非授权漂移。

## 6. 后续计划与真正未验的门槛

| 下一步 | 负责方 | 进入条件/具体边界 |
|---|---|---|
| 当前输入完整canonical Mac21及独立核证 | Codex内部，已通过 | Human明确联网授权；原21命令及前后companion整体PASS，证据见第7节。使用官方npm registry；生产/账号权限不由此扩大 |
| 有效branch证据、应用占位与节点差距审查 | Codex内部方案与复核 | 不把region或0映射当达标，不隐式切换Frozen工具链；按节点依赖另签实际实现与测试范围 |
| ADR0007通知B01–B08 | Human决策，Codex记录落实 | 仍Proposed，未决字典不自行实现 |
| 可信actor/权限/事务/幂等/Event/Outbox与运行时闭环 | Codex内部 | 对应前置正式DONE后逐项实现/验收；不以概念SQL或纯领域函数替代 |
| 真实Harness/Keychain、macOS14、备份恢复、性能/A11y/桌面E2E | Codex内部＋Human必要环境授权 | 真实平台/账号/设备边界，结果未运行即UNVERIFIED，不凭当前开发机推断 |

本轮整改所发现的确定性行为、fixture非空性、伪造就绪、崩溃快照竞态、Gate时钟归一化及高危依赖补丁均已通过限定审验。完整阶段门禁和产品功能缺口仍是准入条件，未借离线PASS解除BLOCKED；Minimax继续暂停外派，后续执行也由Codex维护计划与审核。


## 7. C5联网后的最终审验

Human明确授权npm registry/RustSec联网，费用$0。完整canonical命令数和参数均保留，未减检查、降严重度或新增漏洞ignore。实际macOS 27.0.1 / build26A434 / arm64；Node24.21.0、pnpm10.33.0、Rust1.98.0及固定扫描工具前后binary SHA一致。**这是当前开发机证据，不代替最低macOS14实机。**

四次正式尝试保留在仓库外持久[C5证据目录](/Users/chenzhiqing/.codex/visualizations/2026/10/07/01a113f1-22c9-7142-b9f7-7afa790f837c/codex-audit-closure-mac21-20261009)：

| 尝试 | 真实结果与处理 |
|---|---|
| `gate.json` / `gate.raw.log` | 前17项exit0，第18项npmmirror无audit端点。只为本次执行指定官方npm registry，不改全局配置 |
| `gate-registry.json` / rawlog | 官方审计发现source-map-js1.2.1高危并已补丁；第13项捕获crash测试63/1。生产EOF错误携带Option快照，测试错误地要求即时Some |
| `gate-final.json` / rawlog | canonical21均exit0，最终整体FAIL；Gitleaks真实1:22PM/1:25PM前缀导致companion摘要误漂移 |
| `gate-accepted.json` / rawlog | **整体PASS，21/21 exit0，两项companion前后匹配；完整输入与ignored dotenv身份运行前后相同** |

最终证据：[gate JSON](/Users/chenzhiqing/.codex/visualizations/2026/10/07/01a113f1-22c9-7142-b9f7-7afa790f837c/codex-audit-closure-mac21-20261009/gate-accepted.json)、[完整rawlog](/Users/chenzhiqing/.codex/visualizations/2026/10/07/01a113f1-22c9-7142-b9f7-7afa790f837c/codex-audit-closure-mac21-20261009/gate-accepted.raw.log)、[独立核证](/Users/chenzhiqing/.codex/visualizations/2026/10/07/01a113f1-22c9-7142-b9f7-7afa790f837c/codex-audit-closure-mac21-20261009/gate-independent-verification.json)。独立核证重新读取runner计划、全部命令/exit及原始日志，分别核算Rust/TS、两次真实companion正文摘要，并在本报告输出前现场重算当前全输入。

- 受检文件4,676个；gate input SHA `8a7dd3b3c0751ad823a1de28e556e7b54e3cc55f5b8ef8b4893196e375e81253`。
- Rust **653 passed / 0 failed / 1 ignored**（68结果块）；TS **36/0**。
- Gate26 + replay8合计 **34/0**；新增时钟测试先真实RED后GREEN。
- 完整Adapter默认并行预登记3轮，每轮 **93/0**；聚焦crash **1/0/63filtered**。
- npm canonical high阈值exit0：**high0、critical0**，保留moderate1/low1，不说零漏洞；RustSec1295条新鲜库，漏洞0，维护停止6、glib unsound1、yanked1共8条informational warning保留；Rust许可证3条unused allowance warning保留。
- 全树及全部refs secret scan无finding，非shallow、refs和扫描工具binary SHA未变；本轮保护840项0 missing/0 drift，旧615交付除两项已授权入口更新外0非授权差异。

C5追加最小工程写集如下；合并C1–C4后本轮共6个不同工程文件，没有改生产领域/协议或数据库迁移：

| 文件 | 当前SHA256与实际变化 |
|---|---|
| `pnpm-lock.yaml` | `5e2382e6b22e563ec8ad23587fb1e82bc9ceacdadd982b5d642583d5c677b9b7`；仅source-map-js1.2.1→1.2.2、官方integrity及snapshot引用 |
| `crates/adapter-supervisor/src/lib.rs` | `96083a516a41002396d7c565d79c5a0aca4ebd045bf3e2c6fc9b8bf6b98fe334`；只改cfg(test) import/crash测试，cfg(test)前生产部分逐字节相同 |
| `scripts/ci/local-gates.mjs` | `609fa5d83e44b70e4260e5ea26a8a54124a6155b57881f833431badefd98e17f`；仅行首合法12小时钟、后接已知logger级别的归一化；保留级别/判定/范围/数据 |
| `scripts/ci/local-gates.test.mjs` | `2d73094b40080d32133e9f1e86e6aab37800cb63a434b72b5abbef54a2cb12f7`；保留C4全部断言，加跨AM/PM时钟及findings/bytes/level/payload负对照 |

source-map-js官方元数据核对BSD-3-Clause、无新依赖和Node兼容，隔离frozen安装通过；旧/新包7组差分含正常映射、超大/嵌套/非整数/非有限偏移及实际转换。旧包超大转换被1.5秒有界子进程探针终止，新包由明确校验拒绝，正常对照一致，不将耗时探针加入CI预算。普通pnpm update隔离候选牵动其他版本且目录默认Node22，已排除整包采用；首次对照遗漏sourcesContent的装置错误保留，修正后7/7。

crash测试由错误码、retryable与可用快照开始，新增1秒wait位于原5秒watchdog内；最终必须自然Exited、退出42、含crash诊断及双reader EOF。独立Terra反证退出43、无诊断、**真正node:fs closeSync(1)后存活2秒**均由指定断言检出；恢复绿且主控现场比对12个源/Mock文件0差异。最初stream.end变异不证明OS fd关闭，已在独立报告降级，不计作真实EOF证明。[独立crash报告](/Users/chenzhiqing/.codex/visualizations/2026/10/07/01a113f1-22c9-7142-b9f7-7afa790f837c/codex-audit-closure-mac21-20261009/crash-independent/C5-CRASH-INDEPENDENT-REVIEW.md)。

时钟修复经独立副本回退得0/1、恢复1/0；真实旧扫描1:22PM/1:25PM正文在新算法摘要一致，findings/范围/bytes/level/payload时钟变更仍不同，canonical21和companion定义不变。实际独立工作目录在crash-independent/clock-independent/workspace/scripts/ci，未写生产。[独立clock报告](/Users/chenzhiqing/.codex/visualizations/2026/10/07/01a113f1-22c9-7142-b9f7-7afa790f837c/codex-audit-closure-mac21-20261009/crash-independent/clock-independent/C5-CLOCK-NORMALIZATION-REVIEW.md)。

ajv中危与$data选项有关，当前只在冻结Schema契约测试使用，未启用$data；esbuild低危是Windows开发服务器边界，当前Mac没有验证该平台。glib0.18.5不在当前aarch64-apple-darwin依赖树；这些是保留的风险说明，不新增豁免，不推断其他平台安全。后续独立维护评估应按真实目标和最小补丁处理，避免为追求零warning把Tauri/其他锁版本整批漂移。

本报告、当前台账和入口是**验收后的审计输出**，不声称它们在写出后仍与受检4,676文件全输入完全相同。受检工程文件、manifest/locks、迁移、工具/配置保持最终输入字节；报告输出前已保存实际dirty文档副本，输出差异另机械登记。旧C3 final台账仍误写C1/C2 pending，原件保留、当前入口改current，checksum保护与内容状态核对分列。未新增正式DONE；第6节产品/设备/语义门槛仍有效，Minimax外派继续暂停。
