# DEV-02-08 历史与兼容责任核对（2026-10-10）

**历史保留专项通过；新发现“改派后的Role再换绑，新attempt无法采用当前Binding”的存储缺口，DEV-02-08继续BLOCKED。** 正式状态仍34 DONE / 7 BLOCKED / 60 PLANNED，Gate C仍阻塞。本轮是责任核对、现有专项复验与仓外失败探针，不是完整节点终审，不启动依赖节点。

执行：Codex主控串行，无独立子模型；HC-01交由Human转发Minimaxcode独立复验及修复候选。该参与仅按Human本轮“核对历史与兼容责任，如果有需要minimax参与的任务可以发给我”，不恢复10-09暂停的整批外派，不假称已经收到外部复审。费用0，无联网、安装、生产数据操作、提交或推送。

## 1. 历史结论保留与当前事实

| 时点/材料 | 当时交付/裁定 | 当前责任与限制 |
|---|---|---|
| 09-29初版终审 | 0017加固Thread/Message/attempt，初版35个攻击只有UPDATE轴 | 初版结论已作废，保留方法和失败事实，不引用为当前准入 |
| 09-29重签 | 0018新增创建轴与sender快照入口，历史记录DONE及降级说明 | 记录未改写；其引用的 `~/.minimax/teamspace-audits/dev-02-08/evidence/wave2-macos-arm64-dev0208-resign-final.json` 本轮仍不存在，不能补造旧JSON或用当前结果冒充当时证据 |
| 10-07复核 | 最终重签JSON缺失；当前节点重开BLOCKED | 后续测试可建立当前输入证据，不消除当时缺证的事实 |
| FC/AF-02概念 | 不改原Message寻址；另存override；逐attempt读真实目标；概念事务/存量审查 | 不是生产Messaging服务；契约中“六个新trigger”计数、可用旧二进制回退前提都不能当已验结论 |
| 10-09正式0030 | 单SQLx前向迁移 `no_tx=false`，8专项、真实失败注入与四项行为变异；限定存储PASS | 生产编号已落地，当前迁移0001～0032；旧“概念30尚未签入”仅历史快照；本轮发现此前未覆盖的换绑组合 |
| 10-10本轮 | 六套当前专项57/0；观察探针1/0；Frozen新Binding回归0/1 | 当前历史约束成果保留，新增真实存储缺口HC-01阻断08，不把后续应用责任倒挂为08前置 |

旧报告所说“application全为空壳”“branch未证/ADR-0009待决”已被07与C7后续验收覆盖。ADR-0007仍未接受；不据本轮核对变更通知语义、PENDING到期规则或Frozen状态/权限。

## 2. 冻结责任矩阵

08直接依赖只有DEV-02-01、DEV-00-04，均DONE。以下归属按原节点目标及DAG解释，未改依赖或专属标准。

| 责任 | 所属节点/边界 | 本轮判断 |
|---|---|---|
| Role目标必填、sender/target快照、同Thread reply_to、TTL字段、幂等键、attempt留存 | DEV-02-08存储 | 既有专项通过；控制面TTL判定不由SQL字段自动证明 |
| 改派后历史不重解释；新attempt能表达该Role当前Binding | DEV-02-08存储表达能力 | 旧历史成立；后半发现HC-01实际拒绝，需前向修复 |
| Message/Delivery/Approval纯状态机、FAILED/EXPIRED/NEEDS_HUMAN边 | DEV-02-12 | 依赖08/09/10；不能要求先关闭12再关闭08 |
| 可信actor、八层权限及拒绝审计 | DEV-02-13及应用可信边界 | 依赖08；schema关联不等于actor认证/授权 |
| expected_version竞争、payload冲突、首次结果重放、dispatcher | DEV-02-14 | 依赖12/13；不塞回08作为新前置 |
| 实际路由选当前Binding、授权改派事务、TTL与跨Team查询可见性 | DEV-05-07；Binding生命周期DEV-05-03 | 未启动；修好存储不等于生产用例可调用 |
| Event/Outbox基础原子提交、固定消费重放/去重 | DEV-02-07 | 已单节点DONE；完整Message dispatcher仍属14/05，不重复派修07 |
| 一致性备份、升级恢复、多连接迁移/运行态验证 | DEV-02-15；系统演练DEV-08-05 | 本輪单连接迁移失败回滚不替代真实恢复和并发升级证据 |
| Message跨换绑产品闭环、真实桌面/性能 | DEV-07-10、DEV-08相关节点 | 未验；不得用SQL夹具或旧加载器测试声称产品兼容/发布完成 |

## 3. HC-01 可复现的存储缺口

场景：原Message目标A → 授权改派到B（override1保存B Binding generation1；attempt3保存实际快照）→ attempt3合法失败 → B旧Binding撤销 → 新B Binding generation2成为唯一ACTIVE → 新attempt采纳override1并保存B generation2。

Frozen `STATE_MACHINES.md` §4要求排队消息投递时重新解析Binding，§5要求 `QUEUED→ROUTED` 解析当前ACTIVE Binding；`DOMAIN_MODEL.md` §3和`DATA_CONVENTIONS.md` §5/6要求旧attempt和路由历史保留。override的决策快照不能替代每次attempt的真实投递快照。

0030 `delivery_attempts_route_override_insert` 除关联Team、Message、序号和Role外，还要求attempt Binding ID/generation与override发生时完全相同。真实SQLx执行在当前32版返回：

```text
FROZEN_CURRENT_BINDING_ROUTE_REJECTED
Delivery attempt does not match the override it adopted
```

排除装置误拒的对照：同一B generation2用于原始目标B的Message，INSERT实际1行成功；旧Binding被合法撤销，新Binding经过PENDING_CHECK→ACTIVE且区间不重叠，唯一ACTIVE/Role/generation查询为1。旧attempt完整快照在换绑后不变，坏写拒绝前后全行相同。

不能用NULL override绕行（只认原寻址A），B→B“新override”也被 `from_role_id IS NOT to_role_id` CHECK拒绝。改写旧Message、Binding或attempt会破坏历史；伪造B→C→B不构成该场景的合法修复。此为08存储表达能力的缺口，不需要等生产Router实现才存在。未运行真实Adapter发送，不能据此声称实际发生旧Binding投递。

修复方向：新前向迁移区分override决策快照与attempt实际快照，保留关联和全部历史保护；具体SQL先由HC-01独立核对并在仓外给候选。0033只在任务包中预留候选名，生产仓库没有新增或注册0033。

## 4. 当前验证与兼容边界

最终有效证据在[仓外第03轮](/Users/chenzhiqing/.codex/visualizations/2026/10/10/01a1237c-0b1c-7121-9922-cc68642a02ff/dev02-08-history-review-03)，仓内[摘要及原始日志](evidence/dev02-08-history-2026-10-10/summary.json)。驱动及探针可重放，新增测试只追加到完整仓外副本，不进入生产测试目录。

| 验证 | 实际结果 |
|---|---|
| 六套既有专项 | 57 passed / 0 failed，全部非空：基础Thread/Message/Delivery、0017/0018、0025路由、0030及0028留存 |
| 当前拒绝观察探针 | 1/0，含合法新Binding对照、NULL绕行与同Roleoverride的拒绝，全行零副作用 |
| Frozen新Binding回归 | 编译成功，运行1项、0/1、exit101，在指定实际路由INSERT断言失败；确认HC-01 |
| 29→32完整存量与重跑 | 0030原8项之一通过；全部原attempt列值保留，追加NULL/NULL表示原寻址；0031幂等行追加NULL错误details，旧首次结果不变 |
| 0030三处DROP/CREATE失败注入 | 本轮8项之一通过，真实SQLx migration、schema/业务行/ledger恢复，然后正常升级 |
| 六对象留存及多次改派 | 18项既有DELETE/REPLACE/身份UPDATE前后拒绝一致；A,A,B,C,A真实历史通过 |
| 旧加载器 | 29-only SQLx迁移集合对当前32版库仍首先返回VersionMissing(30)；不是只测30库，也不是运行旧桌面二进制 |
| 工程SHA | 本轮Rust/Tauri/根Cargo输入157项前后相同；另核07终审219项工程清单，结果见仓内校验JSON |

第01轮原57项通过，但新探针把seed中已IN_FLIGHT的a2再次按PENDING推进，rows_affected=0，属于夹具错误，不计产品RED。第02轮复用target且copy2保留mtime，Cargo复用第01轮增补测试二进制，所谓“基线”跑成10项；该轮不计有效验证。第03轮修正a2起点并使用独立新target，精确确认57/0、1/0和指定0/1。所有失败轮原始日志保留，不覆盖或改写为成功。

本轮未修改生产工程，不重跑完整Mac21、全workspace lint或覆盖率，原因是此次只做文档/只读核对及仓外探针，且HC-01尚未修复，不能终审08。07最近Mac21的696/0/1与TS36/0只作为已验证工程字节的历史证据；新增报告/任务包不包含在旧Gate总输入里。后续候选整合必须重新验收当前输入。

二进制兼容仍未验证。恢复仅可经审查的前向修复或一致性备份恢复；不能删除migration ledger、改旧SQL、单DROP守卫或承诺旧桌面二进制直接降级。多连接升级、真实备份恢复和最低系统桌面发布证据仍独立保留。

## 5. 下一步及准入

可由Human转发[HC-01专项任务包](../coordination/MINIMAX-HISTORY-COMPAT-PACKET-2026-10-10.md)：Minimaxcode独立复现并提出仓外最小前向修复、回归/反例和历史兼容证据，实际模型由执行端回填。生产文件只读，唯一仓内回传目录互斥，根manifest/lock与最终整合由Codex负责。

收到SELF_CHECK后，Codex核对输入SHA与原始日志，独立审查、补足行为反例并串行整合；所有08专属及适用通用门禁通过后才考虑恢复08 DONE。当前不恢复08、不改变07/11已DONE成果，也不启动依赖08的新产品用例。
