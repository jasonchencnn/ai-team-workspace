# DEV-02-07 本地审计消费与恢复验收（2026-10-10）

本轮限定消费子包 **PASS**：真实生产到期命令生成 Event/Outbox，固定消费者将审计投影和去重记录原子提交，支持重复投递、缺失投影重建及三处实际进程崩溃后的重放。当前完整 Mac21 整体 PASS。正式账本仍为 **32 DONE / 9 BLOCKED / 60 PLANNED**，DEV-02-07 整节点继续 BLOCKED，不自动放行依赖或产品阶段。

执行依据为[本轮任务包](../coordination/DEV-02-07-AUDIT-CONSUMER-PACKET-2026-10-09.md)、[前一批原子事务验收](DEV-02-07-ATOMIC-EXPIRY-REVIEW-2026-10-09.md)、冻结 Domain/Data/Permission/Error 规格、07 专属标准与 G0～G9。关联 DEV-02-07 原子性/消费重放去重、AC-12 审计基础能力。Codex 总控（GPT-6 系列）串行整合；允许的 Luna/Terra 当前不可调用，使用 R3 允许的第二遍反例审查，未声称独立模型复核或恢复 Minimax 外派。

## 实现与边界

- Application 决定 BEGIN/commit/rollback，SQLite 实现专用端口；Domain 依赖和冻结规则不变。入口只接受可信 host 的 Team/Outbox 引用，不接收 actor、allow、consumer 名称、目的地或 Event 内容。
- 固定内部 `audit-projection-v1` 消费者仅处理 `audit` 目的地、`MESSAGE_EXPIRED`、`message-expiry-v1` 来源。旧的合法未知格式返回 DOMAIN_PRECONDITION_FAILED，并保留未消费；已声明格式的损坏返回 STORAGE_INTEGRITY_FAILED。SQL、JSON 和诊断原文不进入错误输出。
- 执行点重读 Outbox/Event 关联、Team、固定 System actor/nullable snapshot、Message subject、request/version、白名单 JSON；验证受保护字段摘要和同 Team 插入序前驱链接。这里只证明本消费者所处理段，未宣称验证全部历史格式或全链。
- 固定格式的 JSON 在 SQL 查询中限制为最多 512 bytes 再加载到 Rust；投影保存验证后的 ID、时间、版本和 hash，不复制正文、任意 payload/metadata、凭据或路径。
- 先写 event_consumptions、再写 audit_expiry_projection，必须同一事务提交；两处均断言实际 affected rows=1，静默丢写不能成功。相同 consumer/event 返回原收据和第一次 consumed_at。
- 仅派生投影缺失时，从 Event 和保留的去重记录重建，沿用首次时间；错误收据不伪装成正常重复结果。生产 API 无 Event update/delete、marker delete 或通用 SQL 接口。
- 消费成功不更新 Outbox 派发状态，也不产生新的 Message/Event。这里只实现本地投影效果，dispatcher claim、ack、PROCESSING 恢复、通知和用户查询/导出另属后续节点。

## 数据与变更范围

11 个工程文件的 SHA 在仓外 source-verification.json：application lib/consumer；persistence lib/consumer/expiry_hash、到期 writer 的格式共用整合、0032；新消费测试、两个测试支持模块、0031 升级测试的精确边界调整。

0032 新增 1 张可重建投影表、4 个 trigger、1 个 index（6 条顶层 SQL）。投影与 Event/首收据建立外键及字段一致性，禁止 update/replace 覆盖收据；本实现的 audit marker 禁止 DELETE。派生投影可重建，旧消费者未被强行赋予新的留存策略，旧记录均不回填、不改写。

0001–0031、根 manifest/lock、application/persistence manifest 共 35 项受保护输入 SHA 不变；无新增依赖或版本。到期 writer 共用的 v1 格式保持既有字段及输出行为，原 13 项原子事务/审计测试继续通过。0031 专项升级测试明确截止 31，保留原行与 NULL 详情的断言；0032 的升级、重跑与回滚由新增真库测试负责。

旧 31 加载器对已升级库实际返回 VersionMissing(32)，不支持历史二进制直接回退；仅测试库执行了升级与重建。本轮无真实数据库、生产数据、提交、推送、部署、账号或费用操作。

## 验证证据

| 验证面 | 当前实测 |
|---|---|
| 新消费测试 | **18 passed / 0 failed**；其中 17 项行为/迁移测试与 1 个子进程 fixture 入口。入口在普通套件中为空启动项，实际由父测试调用三次，不把它额外计作一条业务场景 |
| 原事务回归 | 13/0；业务变更、Event、Outbox、首结果与 hash 材料继续成立 |
| 重放与竞争 | 相同投递返回首次收据；不同 consumer 隔离；反序交付；真实 WAL 两连接一 Applied、一 Duplicate；失去投影和失败重建后保持第一次时间 |
| 失败与拒绝 | marker、投影写前/写后三处故障和真实 deferred-FK COMMIT 失败；两处 RAISE(IGNORE)；跨 Team、缺失、错误目的地/引用、未知格式、actor/JSON/hash/前驱损坏及过大 JSON。对所有表/ledger 作前后比较，失败后原始业务/Event/Outbox 不回滚或被改写 |
| 0032 升级 | 真实 31 writer 与既有消费记录全部保留；迁移 ledger 只追加 32；重跑不动数据；早/晚 DDL 注入失败恢复完整 schema、所有表与 ledger |
| 实际进程崩溃 | marker 后、projection 后、commit 后三处暂停，使用该事务/已提交库的实际查询生成就绪证据；父进程确认仍存活后终止。提交前整库还原，重放 Applied；提交后未确认重放 Duplicate，首收据时间仍为 200。出处 crash-witness.log |
| 第二遍运行时变异 | 误提交回滚、未写投影即返回成功、跳过 hash、忽略首时间、允许删 marker 五项均真实编译/运行，各 1 failed、exit101；还原 18/0。编译故障不计为行为检出，改靶源码还原 SHA 与生产匹配 |
| canonical Mac21 | **整体 PASS**，21/21 exit0；两个 companion 前后真实 log body 与记录 digest 匹配；受检输入前后相同 |
| Rust/TS/self-check | Rust **692 passed / 0 failed / 1 ignored**，71 个 result blocks，persistence **272/0**；TS **36/0**；gate **26/0**、replay **8/0** |
| 静态/生成/构建 | 最终输入的 fmt、lint、typecheck、clippy `-D warnings`、contracts 漂移与 build 全通过 |

初次 RED 为缺少消费 API 的编译失败，单独保留，不当作既有行为漏洞。中间加入 JSON 查询限界时出现列别名读取不匹配，套件实报 2/16；已修复并重跑。clippy 的 chunks_exact 建议也已按冻结 Rust1.98 修复，无 suppress。失败日志按次保留，不用于最终 PASS 计数。依赖与许可证既有信息性告警仍存在，不能说全工具零告警；ignored 不计通过。

## 当前有效覆盖率

使用现有 Rust stable `instrument-coverage`/`link-dead-code` 和已安装 Apple LLVM；离线、locked、独立仓外 target/profile，无安装、nightly 或 RUSTC_BOOTSTRAP。

| 当前生产源范围 | Covered / Lines | 行覆盖率 | 实际 branch 映射 |
|---|---:|---:|---|
| Domain，16 个有映射源文件 | 2019 / 2197 | **91.90%** | 0，百分比 null，未验证 |
| Application，2 个有可执行映射源文件 | 156 / 167 | **93.41%** | 0，百分比 null，未验证 |

源 SHA 在测试前后相同；只以 production src 作分母，测试/fixture/example/依赖/其他 crate 不入分母，空 lib 模块声明不被称为 100%。当前 Domain 测试和两个真实 SQLite Application 套件参与，普通空子进程入口被排除，但父测试仍实际执行崩溃。19 个 executable、150 个 runtime profile 成功合并/导出；精确源码、命令、工具版本、逐文件映射见 coverage/summary.json 与 tool-versions.json。

Application 已有真实非空分母且满足本轮行阈值；此前 91.76% 只属旧 Domain 测量。region 不是 branch，现有 stable 的零 branch 映射不能证明 ≥85%；Gate C 继续未通过。

## 复现、门禁身份与接续

```sh
cargo test -p persistence-sqlite --test audit_event_consumption
cargo test -p persistence-sqlite --test atomic_message_expiry
cargo test -p persistence-sqlite --test audit_event_consumption actual_process_crashes -- --nocapture
cargo clippy --workspace --all-targets --all-features --locked -- -D warnings
```

完整门禁使用既有已获准 PATH/公共 registry、`node scripts/ci/local-gates.mjs --platform macos --evidence <仓外新文件绝对路径>`。本轮目录：`/Users/chenzhiqing/.codex/visualizations/2026/10/09/01a11f77-308c-70a1-aa48-5061d8a6e6f2/audit-consumer`；包含 mac21.json/raw.log、source/final/post-report-verification、mutation-results、run_mutations.py、crash-witness、coverage/summary、run_coverage.py、companion-verification、gate/replay logs、inventory-before/final。

门禁为 4,718 entries，identity `3d7977e9947abf7e0c7805e98f8b190e17523337b3beab96124eec8fb52ebc6d` 前后匹配。报告和现行入口属于门禁后输出，不能称同步后的全仓仍为受检身份；11 个工程文件和覆盖率源 SHA 在输出后另核。当前开发机证据不能代替最低支持版本设备、hosted branch protection 或产品 E2E。

下一步优先处理[ADR-0009分支工具提案](../adr/0009-isolated-branch-coverage-tool-proposal.md)的Human决策及逐节点准入证据；无需再派发已经通过的原子消费/三处崩溃缺口。07 的广义历史格式扩展和后台装配仍未交付，02-14 dispatcher 的直接依赖须正式 DONE 后才可准入。可信 Human/Role 权限、PENDING 到期冲突、ADR-0007、运行时/Workspace/UI 与产品验收继续独立推进，不由本包替代。

G0/G1/G2/G3/G4/G7/G9 在上述限定范围通过（分支阈值未验证单列阻塞）；G5 限内部 System、Team、来源验证与固定字段输出，未完成全部 Human/Role 权限；G6 未新增 UI，G8 有单条/JSON 限界但未测 1 万数据性能预算。**正式完整节点保持 BLOCKED**。
