# Minimaxcode 接续任务表：十包独立审核后的整改

签发：Codex 总控，2026-10-08。Human 转发本表即可作为本轮精确接续范围；主责 Minimaxcode，回填实际模型/版本，不代填 M3。独立审核与最终准入仍由 Codex 执行。

依据：[独立审计](../reviews/MINIMAX-RETURN-AUDIT-2026-10-08.md)、ADR-0008、原 MM-R01～R10 与冻结规格。**本表没有接受 ADR-0007，没有开放新产品节点或发布。** 当前为 20 DONE / 21 BLOCKED / 60 PLANNED；先整改，不启动依赖开发。

## 公共约束与回传

1. 依序完整读 AGENTS/override、四份 development 基线、specs README、具体节点冻结规格与 ADVERSARIAL_REVIEW_PLAYBOOK。HEAD `1a14b8795e76fc373bef326eb5df96f9ac173f71` 加共享 dirty worktree；启动固定当前输入，与 `evidence/minimax-return-audit-2026-10-08/main/input-before.json` 比较，不 reset/stash/clean 继承成果。
2. 仅修改各 FC 的穷举写集。旧 0001–0027、sources、历史报告/evidence、根 manifest/lockfile、正式节点状态、ADR 接受状态只读。新结果分别放 `docs/coordination/minimax-followup/FC-XX-RESULT.md`，新证据放 `docs/reviews/evidence/minimax-followup/FC-XX/`；这些目录分别归本包写者，不覆盖上轮。
3. 修改生产行为必须先保留实际 RED（合法种子、正对照、实际状态/行数），再 GREEN；拒绝不能只是 unrelated FK/CHECK 或 0-row no-op。独立源码副本作 mutation，保存补丁、命令、exit、完整日志、源码还原哈希。不得在共享 production 源码上临时变异导致别人测到错误输入。
4. 所有命令 offline/locked，已有本地工具，新增费用 $0；不安装/下载/联网/访问外部账号、提交/推送/部署、操作生产数据。发现需要外部操作、共享写集、依赖或新语义，回传精确问题，继续不受其阻碍的工作。
5. RESULT 必须包含模型/环境、输入输出 SHA、逐文件修改、冻结依据、命令 exit/log、正负/竞争/失败/升级对照、剩余风险、回退与 `SELF_CHECK_PASS/BLOCKED`。自检不等于 DONE；有失败就不能称整个包全绿。
6. SQL migrations 是前向修复，不编辑 0025–0027 或更早 checksum。按 **0028 → 0029** 顺序串行注册。移除 Migration 注册行不等于撤掉已落库 trigger，也可能使 SQLx 对迁移记录报错；回退需保留迁移链的兼容二进制/后续前向方案，不承诺“退二进制防线自然消失”。

## 调度与写集

| 次序 | 包 / DEV 节点 | 工作 | 难度 | 允许工程写集（另加本包结果/证据目录） | 主要验收 |
|---|---|---|---|---|---|
| 1 | FC-01 / 02-05、02-09、02-14 | 三处计数断言 | R1，机械但保留行为 | `crates/persistence-sqlite/tests/notification_execution_integrity.rs`、`outbox_default_forward_migration.rs`、`core_history_retention.rs` | 对应三个 test targets；失败升级回滚断言仍通过 |
| 2 | FC-02 / 02-02/03/04/06/08 | 六对象 DELETE/REPLACE 留存 | R3，数据库边界 | 新 `migrations/0028_retained_entity_integrity.sql`、`crates/persistence-sqlite/src/lib.rs`、新 `tests/retained_entity_integrity.rs` | 全表留存矩阵、合法编辑、v27→28 升级/失败原子性 |
| 3 | FC-03 / 02-05 | 终态矩阵与错误合法对照 | R3，状态/历史 | 新 `migrations/0029_run_terminal_integrity.sql`、同 crate `src/lib.rs`、`tests/run_transition_integrity.rs`、`core_history_retention.rs`、`run_lease_consistency_forward_migration.rs` | 冻结64格矩阵、全套 persistence |
| 可并行 | FC-04 / 01-08 | percent/hex 编码脱敏 | R3，安全边界 | `crates/observability/src/lib.rs`、`tests/structured_logging.rs` | 真 JsonSubscriber/错误/Event、大小写/字节边界、资源有界 |
| 可并行 | FC-05 / 03-03，关联03-02/04/05 | JSON-only 独立 fixture | R2工程，R3复核 | `crates/adapter-supervisor/src/lib.rs`、新 `src/bin/fixture_adapter.rs`、新 `tests/supervisor_fixture.rs` | 独立 cargo test 全套，全mode、噪声负向与退出/取消 |
| 可并行 | FC-06 / 01-05 | Gate P2 补强 | R2，证据契约 | `.github/workflows/ci.yml`、`scripts/ci/local-gates.mjs`、`local-gates.test.mjs`、`verify-workflows.mjs` | Gate 单测、verify-workflows、dry-run、反向fixture；canonical仍21 |
| 可并行 | FC-07 / 02-08、02-09 | 重写两份方案及缺失回传 | R3方案，只读产品 | 新 `docs/coordination/minimax-followup/FC-07-route-override-proposal.md`、`FC-07-notification-decision-matrix.md`；旧方案不覆盖 | 概念SQLite副本、两次改派历史回放、待决项分类 |
| 上述之后 | FC-08 / 01-05、02-11 | 文档、变异、覆盖证据 | R2证据整理，R3语义复核 | 新 `docs/coordination/minimax-followup/FC-08-acceptance-index.md`；全部新执行副本/脚本在本包evidence | 当前40-test mutation全台账、当前domain覆盖、最终差异索引 |
| 可并行 | FC-09 / 01-05，关联01-03 | lint 历史材料范围收口提案 | R1脚本+R2工具 | 新 `scripts/ci/replay-env-inputs.mjs`、新 `scripts/ci/replay-env-inputs.test.mjs`；配置仅提案 | 新文件eslint/测试0；主控串行配置整合后全仓lint必须0 |

FC-01/02/03 **顺序串行**，同 `src/lib.rs` 或 `core_history_retention.rs` 同时仅一个写者。其余工程写集互斥，每轮最多三个执行者；实际模型由执行端记录，不能把模型名当验收。FC-08 等实际最终输入稳定后再跑。不得因 R06 已完成机制或 R10 行覆盖达标跳过任何前置。

## FC-01 详细要求

- 纠正当前三条 `MAX(version)==24`/`COUNT(success)==26`：验证 `WHERE version = 24/26 AND success = TRUE` 恰一条，也保留该版本升级前后的业务数据断言。不换为 27/29，不过滤失败用例。
- 全仓检索 `_sqlx_migrations`、MAX/COUNT、migration数及其它数字形态，不只扫 MAX；故意失败升级停在 22/23 的断言语义不同，不能动。
- `cargo test -p persistence-sqlite --test notification_execution_integrity --test outbox_default_forward_migration --test core_history_retention --locked --offline --no-fail-fast`。预先记录三条原失败，再验证新未来迁移注册后不会再漂移。

## FC-02 详细要求

逐表按 DATA §5/6 与 DOMAIN §5 落地 Team、Role、Task、Binding、Reference、DeliveryAttempt 留存。DELIVERY 已有 REPLACE guard，保持原效果；其他表新增或补齐冲突路径，特别检验不同 ID 撞唯一业务键的 REPLACE，而不仅同 ID。

- 负向：无 Event/Reference/子记录的合法叶子数据实际 DELETE/REPLACE；有现有保护的关联行；分别测试同 ID 与各真实 UNIQUE 键冲突；拒绝后逐列未变。
- 正向：Team pause/archive、Role停用、Task取消/编辑、Binding合法状态推进、Reference `validation_status` 更新、Delivery正常状态推进/追加重试。不得把可变名称/标题/业务字段全部焊死。
- 身份 UPDATE 是否可绕开历史关联也逐表核对；仅加冻结依据明确的规则，不把一般编辑都当历史篡改。
- 不加 Attention 永久DELETE guard；不删除/修补脏历史以保绿。旧迁移字节保持；v27同库升级、失败注入、FK与完整行快照必须测试。
- `cargo test -p persistence-sqlite --test retained_entity_integrity --locked --offline`，再 `cargo test -p persistence-sqlite --locked --offline --no-fail-fast`。后一命令此时可能仍有 FC-03 的已登记问题，准确报告。

## FC-03 详细要求

- 0029 补齐四个终态无出边，只允许同状态写回；保留 0027 非终态 17 条冻结边、UNKNOWN对账、状态不变字段更新、旧attempt/token/tuple/lease守卫。
- 把 `terminal_to_terminal_is_a_registered_open_gap` 改为真实拒绝与行未变的断言；全64格按 Frozen STATE_MACHINES 检验，不接受实现自造的允许表。
- R02 的 `RUNNING→TIMED_OUT→SUCCEEDED` 改为分别独立证明 RUNNING→TIMED_OUT、RUNNING/UNKNOWN→SUCCEEDED 等合法路径；不能把旧终态 Run 改为成功来复用fixture。
- 若新增拒绝抢先改变 SQL diagnostic：旧测试可以断言稳定领域原因/相应允许来源和实际行未动；在**只有旧迁移的库**上仍直接验证旧guard承重，另做新guard的mutation。错误字符串不构成被冻结的业务状态边。
- 创建轴必须在RESULT精确登记：历史终态fixture不是生产创建命令。**本包不授权所有INSERT强制QUEUED，也不声称应用创建已验证**；后续 K3可信执行链负责生产创建/重试、actor、版本、Event/Outbox等。不得加fixture专用逃生参数。
- `cargo test -p persistence-sqlite --test run_transition_integrity --test core_history_retention --test run_lease_consistency_forward_migration --locked --offline`，全套 persistence / clippy / build / strict rustdoc / 本包fmt。

## FC-04 详细要求

一次 percent 编码下，**每个secret字节**可字面或 `%XX` 表示，hex位大小写不敏感，解码出的字节保持原secret大小写。不能仅允许符号转义。覆盖本轮主控真实路径的四种漏检、全部编码、UTF-8、重复/多segment、前后普通文本、畸形/截断输入、空secret、多个secret重叠，以及 sensitive 字段。

pure hex 每位大小写混合与全小/大写一样处理，保持文本中不是secret的内容；采用有界扫描，不枚举2^n变体，不引入依赖或全串大小写折叠。复制当前crate/测试、变异还原在独立副本，不在共享src上改来改去。

`cargo test -p observability --locked --offline`、clippy/build/strict rustdoc/本包fmt；给可重放资源上界证据。多层percent解码的深度策略仍待决，不实现递归。

## FC-05 详细要求（本轮正式扩写集）

允许新增 bin 与 integration test；不用改 manifest/lockfile。独立Cargo实验已证明当前lib unit无法 `env!` 获取该bin。

- 迁移**所有**消费原 `fixture_command` 的测试及相应helper、所有fixture mode，不只迁11条失败；integration target 用 `env!("CARGO_BIN_EXE_fixture_adapter")`。无运行时 `cargo run/build`、shell参数拼接、target路径猜测或依赖预先手工构建。
- 删除已迁出重复unit fixture实现/空转test时保留测试映射清单；数量变少必须逐个解释为迁移/去重，不掩盖行为。
- 原5入口的生产解析代码、protocol.rs与Node mock保持只读；测试bin仅测试使用，不注册/包装为获准生产Adapter。
- 六处 Node/Mock 缺失直接return必须变明确失败；Node mock本轮噪声测试真实执行。缺可执行文件、缺mode、panic、进程提前退出一律不能假通过。
- 独立 `cargo test -p adapter-supervisor --locked --offline --no-fail-fast` 必须不依赖之前 build，也可正确运行新增 integration target；workspace clippy/alltargets、strict rustdoc、fmt。附 rawstdout证明只含协议JSONL，诊断写stderr。

## FC-06 详细要求

CI历史扫描前断言 `git rev-parse --is-shallow-repository` 的结果**必须为false**，true或命令失败非零退出。静态检查不能只匹配“打印过这一行”；反向fixture删断言、true、缺 `fetch-depth:0`、缺 `--all` 均应被拒。

companion helper每个声明ID两侧各恰一条，kind/command/args匹配固定definition；同ID额外failed记录和换扫描命令必须被拒。明确这些是独立校验契约补强，不宣称已证明当前main受外部JSON注入。保留日志归一化只除时耗/颜色等、保留判定内容及前后scope比较。

`node --test scripts/ci/local-gates.test.mjs`、`node scripts/ci/verify-workflows.mjs`、`node scripts/ci/local-gates.mjs --platform macos --dry-run`，本包prettier/eslint。canonical仍21，companion单列。没有联网授权，不跑install/audit/hosted CI。

## FC-07 详细要求（方案，不实现生产）

route方案必须保留：原Message.to_role、正文、旧attempt实际收件人及Binding快照；新attempt永久关联使用的override/序号与真实target。演示同一Message两次改派后每个历史attempt不因当前最大override变化；有冻结依据的同Team/有效Binding/TTL/actor/版本/幂等/事件事务与失败零副作用。

- 拒绝原推荐(b)“attempt还是原目标、真实Binding只在override”路线。
- 前向替换不完整旧trigger以满足冻结改派语义，不意味着必须新ADR；若真改变冻结所有权/身份/权限/协议才提交精确ADR草案，不自动占用编号或改Accepted状态。
- UNIQUE只保序号唯一，不保证无洞；序号连续/前目标链需真实约束。幂等遵从DATA四元组及payload比对，不能全Team一个key冲突。
- 立即FK要求Event先于关联INSERT存在，或者正式可证明的合法延迟约束；实际SQLite概念库证明事务次序，不只写伪SQL。概念实现只在本包evidence副本，不占生产migration编号。

通知方案：A类仅明确Frozen规则；`PERMISSION_DENIED`唯一mandatory集合移到B/Human待决。SYSTEM来源与attention类型两轴区分，旧失败Run不能retry变SUCCEEDED；关联新Run和处置凭据。ADR-0007的未决项逐条给推荐/成本/验收，不能以“补字典”直接实现。补R08缺失模型/输入/命令/exit/风险的RESULT型归档。只重写新方案，不伪造旧自验收或改旧历史证据。

## FC-08 详细要求

- 撤回旧 R02/R03/COORD 的“artifacts只有11值所以原审计不可复现”指控，通过新勘误逐项引用原12-value语句与本轮SQLx原RED回放；不要覆写旧文字抹去历史。refs/DeliveryAttempt真发现继续保留。
- R09最终PRE/POST差异纯注释成立；failed rlib路径是自我证伪，正常保留，明确不是GREEN。补最终diff标注中间02文件；21699注明匹配`^  `的IR文本行，不冒称严格instruction数；核验文件数量，避免仅按编号范围计数。
- R10在当前精确副本重跑已声明16个mutation，保存脚本/补丁/原日志/退出/源码还原。无法重现者标未证，不再写16/16；标题、表格、输出三处数字一致。
- 每次重新量覆盖先新副本、新target/profile、新manifest；不带node_modules/dist、不过滤低覆盖Rust文件。复用本轮Apple LLVM离线方法；固定分母/测试/工具/source SHA，branch无映射保持false。应用仍占位不要写100%。
- 形成实际最后一次包结果/失败/未运行/权限清单，串行登记共享src/lib.rs最终hash，历史中间包hash与最终hash有时序映射。改变源码后重新跑受影响门禁，不以过去日志当最终结果。

## FC-09 与主控收口

旧 `deep-audit-2026-10-08/replay_env_manifest.mjs` 只读。新增严格可执行replay/test，在系统临时隔离仓库对legacy普通manifest blind与现行Gate identity区分做正反测试；安全清理结构不在finally抛异常覆盖原错误，不写真实env/真实Git历史。

提交精确配置提案：仅把**该一个不可变历史mjs复现件**按归档材料处理；当前新replay及所有active脚本仍严格lint。不得 blanket ignore所有evidence、关 `no-unsafe-finally`、删旧文件或自改eslint配置。由Codex串行审阅并整合该精确配置，补guard证明active反例仍可让lint失败；全仓lint必须0后才算G2通过。这是历史材料与active源码边界的工具实现，不改产品权限/状态或技术栈。

最终由Codex执行：当前所有Rust/TS/Gate测试、build/fmt/typecheck/clippy/contracts、工作树与全refs本地秘密扫描、输入前后一致、覆盖与证据哈希。需要新鲜公开advisory/联网install的canonical步骤另行获得Human确认；没有授权不得把离线替代写成Mac21全部通过。

## 后续开发顺序（条件排期，尚未放行）

1. 上述确定性整改经Codex终审，恢复受影响底层节点；若未通过仍BLOCKED。
2. 原计划 K1可信身份/权限执行边界 → K2 Owner/通知策略（Frozen部分先做，ADR0007语义等Human逐条决定）→ K3应用事务/幂等/Event/Outbox与并发 → K4真实dispatcher/Run创建取消重试/fencing/恢复。逐项取得前置DONE后另签单节点写集，不在本表擅自实现。
3. 完成真实执行链后再推进DEV-04 Workspace、DEV-05用例/IPC、DEV-06真实数据UI；V2新阶段仍待R1冻结。不以本轮domain覆盖或方案交付跳过系统闭环。
4. 每轮Human转发任务包，Minimax回传，Codex独立反例审查与最终准入；从未授权外发消息、生产部署或新增费用。
