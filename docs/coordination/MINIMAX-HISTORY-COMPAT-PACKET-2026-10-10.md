# HC-01｜DEV-02-08 改派后换绑的历史与兼容专项

Codex 总控签发，Human 转发给 Minimaxcode。仅此包获准参与，不恢复 2026-10-09 暂停的整批外派；不要执行旧 MM/FC/AF/VF 接续表。

## 1. 授权、状态与分工

- 授权：Human 本轮要求“核对历史与兼容责任，如果有需要minimax参与的任务可以发给我”。本包是该请求中的明确专项；不直接联系外部模型。
- 关联：DEV-02-08，基础存储 R3 兼容整改候选；需求来自 `STATE_MACHINES.md` §4/5、`DOMAIN_MODEL.md` §3/5、`DATA_CONVENTIONS.md` §5/6；产品跨换绑闭环仍属于 DEV-05-07/DEV-07-10。
- 正式状态：34 DONE / 7 BLOCKED / 60 PLANNED，DEV-02-08 仍 BLOCKED，Gate C 仍 BLOCKED。直接依赖 DEV-02-01、DEV-00-04 均 DONE。无需先实现依赖08的12/13/14/05-07。
- 主责：Minimaxcode；启动前回填实际 Harness、模型、版本、时间、访问条件。不得把旧 `MiniMax-M3.1-Flash-Preview` 或 ADR 中的候选当本次实际模型。
- 难度：边界明确的 R3 存储与兼容工作，独立复验一个已发现缺口，并在仓外副本提出最小前向修复；不承担完整 Messaging service。
- Codex：保留唯一生产写入、迁移编号签发、根 manifest/lockfile 整合、反例复核、最终 Mac 门禁与正式准入责任。
- 预算：新增费用 $0。不安装工具、联网、开账号、上传或联系第三方，不操作真实用户/生产库，不访问凭据，不提交、推送、部署。若当前执行环境需要新增费用/连接，停止回报。

## 2. 输入与先读顺序

仓库：`/Users/chenzhiqing/Documents/MyVault/协作工作`；分支 `codex/wave-2-ci-validation`，HEAD `1a14b8795e76fc373bef326eb5df96f9ac173f71`。工程包含大量未提交及未跟踪实现，**不能只 checkout HEAD**。

精确输入见 `docs/reviews/evidence/dev02-08-history-2026-10-10/handoff-input.json`，先逐项核对 SHA-256；`git-status-handoff.txt` 只说明工作树状态，不代替文件哈希。回传记录整份清单的 SHA。若必要输入不共享或哈希不一致，先报告差异，不能在旧代码上声称复验当前缺口。

依次完整阅读：

1. `AGENTS.md`、`AGENTS.override.md`；本包授权只覆盖此专项。
2. `docs/development/README.md`、`TECH_STACK_AND_STANDARDS.md`、`DEVELOPMENT_NODES.md`、`QUALITY_GATES.md`。
3. `docs/specs/README.md`，以及 `DOMAIN_MODEL.md`、`STATE_MACHINES.md`、`DATA_CONVENTIONS.md`、`PERMISSION_MATRIX.md`、`ERROR_CATALOG.md`。
4. `docs/development/ADVERSARIAL_REVIEW_PLAYBOOK.md`、`docs/coordination/README.md`、ADR-0001/0005/0008；ADR-0007 仍未接受。
5. 本轮 `docs/reviews/DEV-02-08-HISTORY-COMPAT-REVIEW-2026-10-10.md`、前轮 `DEV-02-08-ROUTE-SCHEMA-REVIEW-2026-10-09.md`、0030实施包及 AF-02契约。历史报告不是当前节点准入。
6. `crates/persistence-sqlite/migrations/0007*`、`0017*`、`0018*`、`0025*`、`0028*`、`0030*`、`0031*`、`0032*`，迁移注册、新旧 route fixture及六套专项测试。

Rust 生产工具链固定1.98.0 stable；SQLx0.8.6。ADR-0009隔离nightly仅用于已授权覆盖率，本包无需使用。

## 3. 要独立确认的具体问题

原 Message 寻址为 A，合法改派 A→B 后，override保存 B的 Binding generation1，attempt3保存同一真实快照。attempt3失败后，B的旧Binding撤销，B generation2成为唯一ACTIVE Binding。冻结规则要求新投递解析当前Binding，同时原Message、override决策快照与旧attempt不改写。

当前0030的 `delivery_attempts_route_override_insert` 同时要求新attempt的 Binding ID/generation等于 override的决策快照，导致上述新attempt被拒；NULL override路径又只认原始A，B→B的新override被CHECK拒绝。Codex的仓外探针包含原始B消息使用同一新Binding的正向对照，排除了无效ID、错generation、外键和忙槽误拒。

先独立核对场景、Frozen依据和归属；若认为不存在缺口，必须提供保持所有历史且可执行的合法SQLx路径及原始日志，不能以改写原Message、伪造B→C→B改派、重置旧Binding/attempt、DROP保护或删除ledger作为替代。

## 4. 允许写集与交付

生产仓库所有工程、迁移0001～0032、manifest/lockfile、Frozen规格、sources及历史证据均只读。

唯一仓内回传目录：`docs/reviews/evidence/minimax-history-compat/HC-01/`，此前材料不得覆盖。使用新的 run-id 子目录，交付：

- `RETURN.md`：实际模型/输入、独立判断、责任矩阵、SELF_CHECK_PASS或SELF_CHECK_BLOCKED。
- `candidate.patch`：仅候选前向迁移、注册与专项测试的差异；不直接应用生产仓库。
- `results.json`、原始非截断日志、输入与输出SHA清单、回退说明。

仓外实验目录：`/Users/chenzhiqing/.minimax/teamspace-audits/hc-01-20261010/<新run-id>/`，只在新建完整工作副本中写。禁止复用带旧增补测试的 target；不能复制/打开生产数据库。若没有共享该路径，可在执行端新临时目录复验，必须记录绝对路径与输入身份。

候选副本写集限定：

- `crates/persistence-sqlite/migrations/0033_message_route_attempt_binding.sql`（**候选编号，尚未签入生产**，正式编号由Codex整合时核对；注册 `no_tx=false`）。
- `crates/persistence-sqlite/src/lib.rs` 仅候选注册。
- `crates/persistence-sqlite/tests/message_route_binding_compatibility.rs`、必要合成fixture。
- 现有route测试的副本只允许新增回归，或调整“新最新迁移版本”辅助断言；不删除、放松原有安全断言。

方案应区分 **override发生时的决策快照** 与 **每个attempt实际采用的Binding快照**。新attempt仍必须引用同Team、同Message、同序号和同目标Role的override，并保存完整、匹配该Role的Binding/generation；原寻址路径继续受保护。最终ACTIVE/权限/TTL/最新路由选择仍是应用执行点责任，不据此发明数据库状态语义。

不要通过简单移除整条route关联守卫来修复。若需要扩大到新状态、权限、协议、对象身份或业务语义，停止回报，提出具体ADR候选，不能自行落地。

## 5. 必须执行的验收

### 当前基线与RED

在当前仓库根运行只读驱动，输出目录须全新：

```sh
python3 docs/reviews/evidence/dev02-08-history-2026-10-10/run_review.py \
  /Users/chenzhiqing/.minimax/teamspace-audits/hc-01-20261010/<新run-id>/baseline
```

驱动成功exit0表示：六套原专项57/0、观察探针1/0，以及Frozen新Binding回归 **0/1、exit101** 被精确确认。须保留编译成功、非空套件、指定panic标记和目标SQL守卫诊断；种子/编译/路径/缓存错误不算RED。随后另建候选副本，保留基线日志。

候选副本中运行：

```sh
cargo test -p persistence-sqlite --locked --offline \
  --test message_route_binding_compatibility
cargo test -p persistence-sqlite --locked --offline \
  --test thread_message_delivery_migration \
  --test message_delivery_integrity_forward_migration \
  --test message_delivery_insert_state_guard_forward_migration \
  --test message_routing_team_integrity \
  --test message_route_overrides_migration \
  --test retained_entity_integrity
cargo test -p persistence-sqlite --locked --offline
cargo fmt --all -- --check
cargo clippy -p persistence-sqlite --all-targets --all-features --locked --offline -- -D warnings
```

以上均应exit0且非空。当前观察探针断言的是旧实现的拒绝事实，不能在候选上冒称应继续通过；Frozen回归须移入新正式候选测试并保持“新Binding接受/旧历史不变”的语义。若照搬仓外probe到候选33，只可把环境schema辅助断言32改为33，不得改变Frozen成功断言。

至少覆盖：

1. 原寻址A换绑重试、改派B后B换绑重试，两个正向对照。Message原寻址/正文、override决策快照和所有旧attempt完整字段不变；新attempt保存B generation2，并采纳原override。
2. 跨Team、错Message、错目标Role、错Binding所属Role、错generation、缺Binding/半截override、错误override编号、四元组抢占仍拒绝，拒绝前后全行相同。
3. 多次改派A→B→C→A后再换绑，逐attempt读真实历史；历史UPDATE/DELETE/REPLACE仍拒绝。补序号跳跃、错误前驱的独立正负对照。
4. 从含旧attempt的29版、含override的32版前向升级；旧列值保留，29版新列NULL解释不变；迁移重跑不产生变化。旧0001～0032逐文件SHA不变。
5. 新DROP/CREATE间实际SQLx失败注入：完整schema、业务行及ledger恢复原状，随后未变的候选迁移能成功。至少一个删除/弱化候选关联约束的仓外变异应编译后以指定行为断言RED，恢复后非空套件GREEN。
6. 29-only加载器对当前schema仍为VersionMissing(30)。这不等于运行旧发布二进制。另说明30/31-only加载器对候选schema的首个缺失版本，禁止声称二进制可安全降级。

本包不要求联网Mac21，不使用旧Gate摘要替代候选当前证据；Codex整合后负责完整Mac21、第二遍反例与节点终审。多连接升级/一致性备份、最低系统桌面发布演练分别保留到DEV-02-15/DEV-08-05，不声称本包已证明它们。

## 6. 回传与停止条件

回传文件列表、所有命令与exit、每套实际passed/failed/ignored、RED/GREEN原始日志、反例补丁、输入和旧迁移SHA、历史/二进制兼容边界及未测项目。只给SELF_CHECK结论，不能改节点表或宣布DONE。

身份来源不明、SHA不符、写集冲突、Frozen歧义、新语义、生产数据/凭据需求、新联网/费用条件均停止对应工作并说明具体阻塞；其他独立只读核对可继续。失败证据不覆盖，禁止reset历史工作树。

回退遵循前向修复或一致性备份恢复；禁止删ledger、仅DROP守卫、改写旧migration或把原Message寻址改成最新目标。Human转发完成后，Codex等回传并独立复核，正式状态保持34/7/60。
