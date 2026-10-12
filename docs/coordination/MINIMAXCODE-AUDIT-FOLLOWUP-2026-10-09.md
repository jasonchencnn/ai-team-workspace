# Minimaxcode 下一轮精确整改任务（2026-10-09）

Human 可直接转发本表。依据 [FC九包独立审计](../reviews/FC-RETURN-AUDIT-2026-10-09.md)；主责仍为 Minimaxcode，实际模型/版本执行端回填，Codex 独立终审。仅三个整改包，不重做已确认通过的全部九包。

当前仍20 DONE / 21 BLOCKED / 60 PLANNED。ADR-0007仍Proposed，未开放通知未决语义、V2新阶段或发布。HEAD `1a14b8795e76fc373bef326eb5df96f9ac173f71` 加继承dirty工作树；启动重新固定当前输入。

## 公共约束

- 依序读 AGENTS/override、四开发基线、specs索引与具体冻结规格、ADVERSARIAL_REVIEW_PLAYBOOK及本轮审计。
- 以下穷举写集以外只读；sources、旧迁移0001–0029、历史报告/evidence、根manifest/lockfile、正式节点及ADR状态不动。不要reset/clean/stash继承成果。
- 每包新回传 `docs/coordination/minimax-audit-next/AF-XX-RESULT.md`，新证据 `docs/reviews/evidence/minimax-audit-next/AF-XX/`，不覆盖此前自陈和原始日志。输入/输出SHA、模型、命令/exit/rawlog、真实RED/GREEN与正对照、回退、未验项俱全；不自判DONE。
- 全程offline/已有工具、新增费用$0；不提交/推送/部署/联网/生产操作。若需新依赖、写集或语义，回传具体提案，不自行扩大。
- 三包写集互斥，可并行；每轮最多三个执行者。最终证据等AF-01稳定后收口；主控串行统一门禁与正式准入。

## AF-01：关闭 `%` 字节单层编码漏检（P1 / R3）

- DEV-01-08；主责Minimax，Codex安全复核。
- 允许修改：`crates/observability/src/lib.rs`、`crates/observability/tests/structured_logging.rs`，另加本包结果/证据目录。
- 冻结依据：TECH_STACK §7/11；原FC-04“每个注册字节可字面或一次 `%XX`”；本轮B01反例。

当前 `match_at` 优先按字面消费 `%`，没有处理同一 `%` 可能是 `%25` 前缀的歧义。先复现本轮三条真实回归：`%ABC→%25ABC`、`A%B→A%25B`、`%%→%25%25`，应全部脱敏，现在0/3失败。反例源/日志在 `fc-return-audit-2026-10-09/main/percent-repro/` 与 `percent-regression-red.log`。

修复必须：

1. 正确处理 `%` 的字面/escape两种可能，保持未转义字母大小写和hex位大小写规则；不能只换成escape优先而让另一种合法混合拼写失守。
2. 不做递归percent解码、不折叠整个输入大小写；不得用指数级拼写枚举或无界递归回溯。算法必须有明确资源上界，并用多 `%`/近失配长输入证明。
3. 增加 `%` 位于首/中/尾/连续、原secret含形如`%25`/`%41`片段、与其它字节全/部分转义混合、畸形/截断、UTF-8、空secret/重叠secret的正负对照，证明注册secret消失且无关内容保留。
4. 用实际 `sanitize/redact_error/redact_event/JsonSubscriber` 四入口断言危害，不只测试手抄matcher；一次decode后的秘密仍不得出现。
5. 保留现有29用例及原修复效果，变异只在独立当前副本，保存编译成功、真实断言红、还原哈希及绿基线。缺覆盖不可用假断言补数字。

验收：`cargo test -p observability --locked --offline`；build/clippy `-D warnings`/strict rustdoc/本包rustfmt；资源测试命令和结果。`cargo fmt --all --check`只读可跑，不全仓write。修复后Codex独立复核本轮公开日志反例。

## AF-02：路由概念的正式事务/回退及联合验收（R3，只改方案与装置）

- DEV-02-08，关联DEV-05-07；不签生产migration编号或新产品实现。
- 允许新增：`docs/coordination/minimax-audit-next/AF-02-route-implementation-contract.md`；本包结果/证据目录下的概念SQL、Python/Rust测试及独立manifest/lock。旧FC07文件保持只读。

Codex裁决：改派载体符合已有Frozen语义，不因前向替换旧trigger自动需要新ADR；真改变身份/权限/协议才另提。**正式执行必须是SQLx单migration事务、no_tx=false**；原POC无事务executescript不能直接用于安装。Codex已用当前29版内存库、三处DROP分别故障注入证明完整schema回滚，并用正常概念migration验证0028/0029守卫保留，证据 `main/route-ddl-sqlx-locked.log`。实验中的30仅临时库测试版本，不能当生产编号。

本包补完：

- 新装置显式采用上述事务边界；Python展示路径若继续使用executescript，必须显式事务/异常回滚并实测，不能认为调用前BEGIN会自动跨executescript保留。
- 在**有真实历史业务行**的当前29版库，三处DROP后的CREATE前各注入失败：逐表完整行快照、完整schema、旧关键拒绝行为恢复、无新migration成功记录。保持失败零副作用。
- 成功路径重跑旧目标/新override路径的INSERT、UPDATE、REPLACE以及FC02留存联合矩阵；两次以上改派后旧attempt真实收件人/Binding/override快照不重解释、不更改，Event先于立即FK关联。
- 回退只能为经审核的兼容二进制/后续前向修复；不能仅DROP守卫或移除注册行声称回退安全。原Message/attempt历史与0028留存不得弱化。
- 写明actor真实性、最终八层权限、版本/TTL/幂等、业务Event/Outbox仍需应用执行点；概念库证明不是产品权限闭环。

验收命令写入本包RESULT：独立概念测试、实际SQLx迁移故障注入（已有本地依赖、locked/offline）、源码/生产差异0。不得新增production依赖或在crates/migrations放新文件。

## AF-03：签回最终证据口径与当前开放项（R1资料 / R3结论）

- DEV-01-05/DEV-02-11，关联DEV-01-08/02-08；纯文档和独立重放脚本。
- 允许新增 `docs/coordination/minimax-audit-next/AF-03-final-acceptance-index.md`，本包结果/证据目录。未列工程文件只读；不重新生成/覆盖当时不存在的日志。

必须采用主控裁决：

1. 原R10：**15项当前按声明意图重建＋原M14不可编译/历史“2 tests”未证**，不能写原声明16/16。新实验：15项重建＋明确配套ALL/as_str的 **M14-R** 共16项，独立16/16断言检出。两列分别说明来源，不以“重新定义声明数”替代任务要求。日志/补丁保持原样，新增映射解释旧M14编号和新M14-R。
2. 更新旧开放清单：refs/DeliveryAttempt DELETE已由0028关闭；FC03终态矩阵已关闭；FC05四私有unit/POSIX子进程/时间裕度已获本轮签收；FC06机制已验证；FC09精确eslint配置已主控整合并全仓lint0。仍开的是AF01、AF02联合验收、通知Human决策、可信应用链/完整Mac门禁等。
3. 修正FC06前提：PR里的ci.yml为PR自带内容；临时目录+stub PATH不是安全沙箱，不能假设片段已评审，不外推到带秘密的执行环境。当前CI只读/无secret且原本已执行PR源码的边界如实记录。
4. 修正FC02/03对RAISE(IGNORE)的描述：它是fail-silent，语句成功、行未改；测试拒绝码/实际行断言发现了这种假成功，不等于数据库已有非法脏写。旧记录通过新勘误说明，不覆盖留痕。
5. 新覆盖仍固定全16src、完整当前测试、工具/manifest/source SHA，domain当前91.76%，application占位、branch未验证。读取工具不能凭空产生branch映射，不写0 branch=100%，不拿region代替。AF01不改domain无需重复凑覆盖数字；若分母/源码改变再新测。
6. 当前最终离线证据数字：Rust643/0/1 ignored、TS36/0、Gate25/0、replay8/0；AF01修复增测试后必须从实际日志更新，而非硬编码保留643。已确认当时存量数字与未来终值分开。

本轮主控来源：`docs/reviews/evidence/fc-return-audit-2026-10-09/`。独立当前源副本的137/80还原、原M14 E0004及撤销后40/0、公开日志反例、SQLx DDL故障和精确lint范围均有新证据；不要求回写旧FC08原件。也不要继续把FC08称为任务表“第10包”，本轮实际九包。

## 后续准入

完成以上三包后由Codex复审。现有离线套件全绿不能豁免AF01或正式节点G0～G9。联网Mac21/advisory须另行获Human明确授权，未授权先完整交回离线证据；不自行安装或下载。

通知B01～B08只作为Human决策输入，不能自改ADR0007或实现待决字典。后续K1可信身份、K2已冻结通知身份、K3应用事务/幂等/Event/Outbox、K4dispatcher/Run/fencing以及Workspace/IPC/UI仍按前置DONE和具体写集另签；本表不自动开工这些节点。
