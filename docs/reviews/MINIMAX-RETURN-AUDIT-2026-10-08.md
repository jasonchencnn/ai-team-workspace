# Minimaxcode 十包交付独立审计（2026-10-08）

**结论：本批不准入，继续整改；不接受“内容已全部无误”。** 原始审计点已有实质修复，但终态历史、编码脱敏、fixture 与最终门禁尚未闭环，两个方案还存在语义错误。没有新增 DONE 节点。本报告是本批输入的审计结论，不能代表全项目或上线验收完成。

## 1. 输入、范围及工作量

- HEAD：`1a14b8795e76fc373bef326eb5df96f9ac173f71`，继承 dirty worktree 保留。
- 输入与 Minimax 启动清单逐文件比较：299 个旧范围文件中，14 个源码/测试/配置文件变化；另新增 3 个迁移、4 个测试文件，共 21 个工程文件。详见 [workload.json](evidence/minimax-return-audit-2026-10-08/main/workload.json)。不能把 HEAD 的全部 dirty diff 归给 Minimax。
- 可取得且按启动哈希核验的 16 个新旧文件合计 +7664/-265 文本行，含大量测试与注释；另 5 个文件没有同哈希旧字节副本，未猜测其增删行。此量不是产品完成率。
- 0025/0026/0027 共新增 18 个 trigger（6/11/1）；domain 新增 40 个测试，observability 新增 11 个。R07/R08 是方案，未计为功能实现。
- 0001–0024、根 Cargo/package manifest、lockfile 未变。此前审计 delivery-manifest 中的 301 个产物哈希全部一致，旧证据未被本批覆盖。
- 本轮审计仅新增证据、报告、接续任务表并同步当前准入记录；未修改产品代码、提交、推送、部署或访问外部服务。Minimax 的历史费用/外部行为仅作为执行端声明，本地哈希不等于账单或网络审计。

本轮证据：[evidence/minimax-return-audit-2026-10-08](evidence/minimax-return-audit-2026-10-08/main/checks.json)。四名 Subagent 实际使用三个 `gpt-5.6-terra` 专项审核及一个 `gpt-5.6-luna` 机械核对，最多三名并发；主控作第二遍反例、真实 SQLx 回放、公开日志路径复现和最终裁定。

## 2. 独立实跑

| 检查 | 本轮结果 | 解释 |
|---|---|---|
| domain 全套 | 287 passed / 0 failed / 1 ignored，exit 0 | ignored 是既有文档示例，不是新增屏蔽 |
| observability | 18 / 0，exit 0 | 绿套件仍漏下述真实反例 |
| persistence-sqlite | 206 / 3，exit 101 | 三处确为版本断言：24/24/26 对实际 27 |
| adapter-supervisor | 79 / 11，exit 101 | 每条失败先遇 libtest 非协议 stdout；不是按原目标测成功 |
| 其余 workspace Rust 测试 | 1 / 0，exit 0 | application/security/workspace 多数仍占位 |
| Rust 合并口径 | 591 passed / 14 failed / 1 ignored | 按互斥 crate 检查合并；不包含审计装置的故意 RED |
| Gate 单测 | 22 / 0，exit 0 | `node --test scripts/ci/local-gates.test.mjs` |
| TS 全层测试 | 36 / 0，exit 0 | 5+1+21+1+4+4；E2E 名称不等于真实桌面闭环 |
| lint:ts | exit 1，1 error / 0 warnings | 历史 `replay_env_manifest.mjs:24` 的 `no-unsafe-finally`；崩溃已消除 |
| typecheck / contracts / tooling guards | exit 0 | 当前输入 |
| workspace clippy / Rust build / TS build / fmt | exit 0 | offline、locked；未通过这些结果豁免失败测试 |
| workflow 静态检查 / Gate dry-run | exit 0 | 不是 hosted CI 或完整 canonical 21 实跑 |
| 本地工作树 / 全 refs 秘密扫描 | exit 0 / 0，no leaks found | 固定已在本机的工具哈希；非 shallow，4 个可达提交；不代表真实凭据/生产环境审计 |
| 当前 domain 覆盖复测 | **2016/2197 = 91.76%** | 16 个 src、全部当前测试、33 份 rs 与当前输入同字节 |
| 分支 / application 产品覆盖 | **未验证 / 无可执行行** | region 89.87% 不代替 branch；application 不记 100% |

命令、退出码和完整日志在 `main/checks.json`、`main/additional-checks.json`、`main/other-rust-command.json` 及同目录各 `.log`。主控第一次 Gate 单测命令写错文件名，产生一个路径不存在的 exit 1；已用上述正确命令补跑 22/0。该装置错误保留在日志，不计产品失败。

本地扫描额外记录在 `main/local-safety-checks.json`。初始 PATH 未包含已装 gitleaks，后按既有 provenance 指定绝对路径；一次误用 dir 的 `--source` 参数被拒，改为位置参数后成功。错误调用日志保留，未安装或联网，最终有效两项扫描均exit 0。

覆盖复测见 [当前 input-manifest](evidence/minimax-return-audit-2026-10-08/coverage/input-manifest.json) 与 [summary](evidence/minimax-return-audit-2026-10-08/coverage/domain-coverage-summary.json)。R10 原副本早于 R09 两处注释改动，本轮重新测量当前字节后数字一致，不再靠旧覆盖外推。

未运行联网 install/advisory 更新、完整 Mac 21、最低 macOS 14 桌面、真实 Harness/Keychain/备份恢复、性能/A11y 或全产品分支覆盖。没有把缓存扫描、离线检查或 mock 命名计作这些能力已验证。

## 3. 需要修复的行为

### MR-01 / P1：R03 留出 12 个非法终态到终态格

`0027_run_transition_integrity.sql:105` 只覆盖四个非终态来源；`SUCCEEDED/FAILED/TIMED_OUT/CANCELLED` 之间的 12 个非自迁移格仍可写。真库实测 `SUCCEEDED → FAILED` 后行确实变为 FAILED。冻结 `STATE_MACHINES §3` 的终态无出边，R03 包要求正负矩阵按冻结表核对，因此这不是仅靠登记就可以关闭的范围例外。

R02 `core_history_retention.rs:1017` 还把 `RUNNING → TIMED_OUT → SUCCEEDED` 标成合法对照；R03 的 `terminal_to_terminal_is_a_registered_open_gap` 固化了非法 acceptance。纠正测试应依据冻结规格，不能为保留旧 trigger 报错字符串而保留漏洞；存储修复本身必须证明拒绝、行未动和合法非终态路径。

**创建轴另行限定**：直接 INSERT 非 QUEUED Run 仍可能，但冻结创建行描述的是创建命令，历史 fixture 可以表达已结束 Run。R03 包明确要求评估登记创建轴，没有无条件授权强制所有历史 INSERT 都从 QUEUED 开始。本项不直接归为 R03 越界遗漏；可信 Run 创建执行点未实现，整体仍 BLOCKED。后续必须证明生产创建/重试走 QUEUED，导入/历史 fixture 与生产命令分开；不能以历史种子为生产放行依据。

### MR-02 / P1：R04 对字母数字的合法 percent 转义仍漏检

已登记模拟 secret `Audit-Only/AaB`：

| 输入 | 单次解码 | 当前真实 `Redactor` / 日志结果 |
|---|---|---|
| `Audit%2dOnly%2fAaB` | secret | 脱敏 |
| `%41udit%2dOnly%2fAaB` | secret | **泄露** |
| `Audit%2d%4fnly%2fAaB` | secret | **泄露** |
| `Audit%2dOnly%2f%41aB` | secret | **泄露** |
| 全字节 `%XX` 编码 | secret | **泄露** |

`observability/src/lib.rs:644` 将字母数字固定为 Literal，`:679` 只接受字面字节。主控使用实际 crate 的公开 `sanitize/redact_error/redact_event/JsonSubscriber` 复现，日志 JSON 的 detail 中真的保留该模拟秘密编码。见 [真实路径日志](evidence/minimax-return-audit-2026-10-08/main/real-redactor.log)。Subagent 的 std-only 模拟只作辅助，不作唯一证据。

R04 的两项边界裁决：允许字面与一次 percent 转义混合，符合防泄露意图，接受该加固；保留未转义 secret 字母大小写，`audit%2donly%2faab` 解码不是已登记 secret，保持原文正确。纯 hex 大小写混合也有实际漏检，同批补确定性的 hex 匹配，不枚举指数变体。递归 `%25` 多层解码的深度/误伤策略未冻结，单列待决，不擅自实现递归解码。

### MR-03 / P1：完整留存普查发现六个写集外缺口

[完整普查](evidence/minimax-return-audit-2026-10-08/storage/RETENTION_INVENTORY.md) 区分无条件守卫、条件 Event/Reference 守卫和子表 FK。

| 对象 | 真库结果 | 冻结依据 |
|---|---|---|
| Reference | DELETE 1→0；REPLACE 改 target_uri | DATA §5、DOMAIN §5 |
| DeliveryAttempt | DELETE 1→0；已有 REPLACE 防线 | DATA §5/6、DOMAIN §5 |
| 无子记录的 Team / Role / Task / Binding | 各 DELETE 1→0；REPLACE 改名称/标题/快照 | DATA §5/6；Binding 为追加历史 |

这些是 R02 原五表写集之外的追加缺口；**不要求 Minimax 未授权改更多表**。R02 对 Run/Review/Artifact/Message/Approval 的留存修复成立。不能把父对象恰有子记录时 FK 拒删，推广成全部历史行永远保留；同样不能宣称存在生产外部 DELETE 接口或已发生生产损失。本轮把上述存储留存保证列为准入阻断，待前向守卫与合法业务对照收口。

Run/Event/Review/Notification/Approval/Message/Artifact/idempotency 的现有保护保留；Attention 仍是可重建派生索引，不混入永久核心历史集合。留存与实体 UPDATE 业务语义要分别测试，不把 Team/Role/Task 合法编辑焊死。

### MR-04 / P1 门禁：fixture 和计数仍未闭环

三处计数断言应改为验证**该迁移成功记录恰一条**，不改成最新版本 27，也不删行为断言；故意失败升级的 MAX(version)=22/23 回滚证明保留。

R05 五入口的严格 JSON-only 修复成立，11 条失败证实 fixture 需要改造。仅新增 bin **不充分**：Cargo lib unit tests 中 `CARGO_BIN_EXE_fixture_adapter` 不存在，integration tests 中才可可靠 `env!` 定位；已用无依赖 offline crate 实验。将所有相关 fixture 消费测试迁入 integration target，覆盖所有 mode 和退出/取消/EOF路径，生产 parser 不得放宽。

`lib.rs:2090/2120/2145/2173/2207/2239` 对缺 Node/Mock 直接 return，会假绿。当前本机可运行不等于测试可重复保证；改为缺依赖明确失败。

### MR-05 / P1 方案：R07 不得改变每次投递的事实快照含义

R07 推荐 (b)：attempt 仍写原目标、实际改派收件人仅在 override，target Binding 也脱离 attempt；查询依当前最大 override。该方案不能作为“无语义改变的读模型澄清”准入：一条历史 attempt 不能因后续改派重新解释其实际收件人，也不能借空 Binding snapshot 绕开真实快照。

改为每个新 attempt 永久关联其所采用的 override、实际 target Role 与 Binding/generation。保留原 Message 和所有旧 attempts；通过**前向迁移**替换不完整的旧判据，保留原始目标/新授权 override 两条明确路径及 Team/Binding 校验。遵循已经冻结的改派行为本身不自动需要新 ADR；真正改变身份/所有权/权限/协议含义才提交 ADR 给 Human。

其他方案错误：`UNIQUE(message_id, override_number)` 只防重复不保证连续无洞；`UNIQUE(team_id,idempotency_key)` 不是 DATA §4 的 actor/command/key 四元组；override 有立即 Event FK，却把 Event INSERT 写在事务最后，次序不成立。先补可执行的概念验证再签实现写集。

### MR-06 / P1 方案：R08 把未冻结集合误列为可直接实施

R08 A-01 声称 P0 唯一 mandatory 成员 `PERMISSION_DENIED` 是“补字典，不是新语义”，但自己 D-02 又要求 Human 确认同一集合。Frozen 文本没有列出该唯一集合，ADR-0007 仍 Proposed，必须移到待决项，不能让执行者依据 A 类直接修改冻结规格。

SYSTEM 是 source_type，SECURITY_ALERT 是 attention_type，两种维度不应仅因没有同名 SYSTEM 枚举就认定“无合法类型”；证明具体语义缺失再提决策，不先新增状态/类型。D-01 重建示例把旧失败 Run retry 成功写成旧 Run 变 SUCCEEDED，与追加新 Run 语义不符，应明确 Task、新 attempt 和处置证明。R08 缺规定的 RESULT 与 evidence/模型输入命令归档，需补齐。方案仍只读，未决条款不得实现。

## 4. 证据与机制补充

- R06 ignored dotenv 和 main 的前后输入/companion/全 refs/digest 绑定成立，canonical 21 保留。CI shallow 命令只打印、不要求 false，应 fail-closed。helper 接受重复 ID/伪造 command 的反例仅可直接传参，**当前 main 不读取外部证据 JSON，不能称真实门禁绕过**；补唯一性/definition绑定以满足独立校验契约。两项为 P2 补强。
- R09 两处最终改动确实只含注释，IR PRE/POST 字节相同；rlib 的失败路径及自我证伪被正确披露，失败文件本身不是缺陷。`02-comment-only-diff.txt` 是中间版本，补最终差异并明确中间件；21699 可机械重现为 `^  ` 开头的 IR 文本行，不能未经定义称严格 instruction 数（IR 文件各 58004 行）。
- R10 40 个测试和当前行覆盖通过；其“12/12”标题却列 M1–M16，原逐条 mutation 日志没有归档，16/16 属自陈。补当前副本的脚本、补丁、完整 exit/log/还原哈希，不能伪造当时日志。getter/渲染检查中有 wire 名与边界意义，不能仅因有 as_str 就全部判为刷量。
- R09 文档/evidence 文件数量口径不同；R06/R09 的交接索引仍留旧两处失败、旧 lint 崩溃等中间结论。按时间追加最终索引；Human 已授权移除的 R10 构建副本不必为了旧崩溃重放再塞回项目，旧日志明确标历史即可。
- lint 仍有 1 条真实错误，G2 不通过；历史文件只读与当前全仓 lint 的范围冲突由主控收口，不允许 Minimax 擅自 blanket ignore evidence、关闭规则或改历史字节。

## 5. 对“审计自身 artifacts 装置缺陷”的裁决

**不成立，要求撤回/更正这条指控。** 原 `audit_storage_boundaries.rs:55` INSERT 是 12 个值，其中 checksum 后的 `NULL` 即 git_commit_sha；schema 也是 12 列。301 项旧产物完整性核验为零变化。

主控用原 main-repro 源码、独立新 target 和新日志重新跑 SQLx：**1 passed / 3 failed**，仍输出五表全部可删（包含 artifacts）。见 [原 RED 独立回放](evidence/minimax-return-audit-2026-10-08/main/old-storage-red-replay.log)。因此 B-2、R02/R03 多处“11列值、不可复现”的文字错误；不能用它订正原审计有效结论。其余真实 refs/DeliveryAttempt 发现保留，不因这条错误全部否定。

## 6. 逐包裁定与状态

| 包 | 本批裁定 | 已接受部分 / 尚需处理 |
|---|---|---|
| R01 | 局部行为通过，整包待统一门禁 | Team UPDATE、合法无 Binding 路径、升级/失败证明；版本断言另签写集 |
| R02 | REVIEW，需测试/文档整改 | 五表守卫成立；非法终态合法对照、新计数断言、artifacts 错误指控 |
| R03 | BLOCKED | 非终态倒退修复成立；终态矩阵未实现、测试正预期错误 |
| R04 | BLOCKED | 原 RED 修复成立；合法字母数字 percent 及 mixed hex 仍漏检 |
| R05 | BLOCKED | 严格五入口成立；可靠 fixture、缺依赖假绿未闭环 |
| R06 | REVIEW | main 机制成立；CI shallow/独立校验契约补强，完整 Mac Gate 未跑 |
| R07 | 方案退回 | 原缺口/完整场景复现有价值；推荐方案改变投递历史、幂等/序号/事务错误 |
| R08 | 方案退回 | 派生重建区别成立；A/B 分类冲突、Run示例与归档需纠正 |
| R09 | 注释通过，证据 REVIEW | 最终diff、索引时序/计数补充；不把 failed rlib 路径算作漏报 |
| R10 | 测试/行覆盖通过，mutation证据 REVIEW | 当前91.76%；分支仍未验，补可重放变异证据 |

独立审计新增 Team/Role/Binding/Task 留存阻断涉及 DEV-02-02/03/04，当前从历史 DONE 重开为 BLOCKED；其历史审核保持只读。正式当前统计 **20 DONE / 21 BLOCKED / 60 PLANNED**（101 节点）。其他既有阻断保留；不把本包局部通过当节点 DONE，不启动依赖。

## 7. 接续

Human 可转发 [补充任务表](../coordination/MINIMAXCODE-FOLLOWUP-2026-10-08.md)。先修确定性行为、可靠测试与本批文档；再由 Codex 串行整合、复跑最终输入。ADR-0007 未决语义和联网验收单列，不混进本轮离线实现。底层整改后仍需原计划 K1–K4 的可信身份/事务/dispatcher/fencing 执行链，才能推进真正应用和 UI 开发。
