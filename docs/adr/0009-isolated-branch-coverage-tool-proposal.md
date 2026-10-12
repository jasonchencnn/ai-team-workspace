# ADR-0009：隔离的有效分支覆盖率工具

- 状态：**Accepted（2026-10-10 Human 明确接受）；执行结果由专项报告记录**
- 日期：2026-10-10
- 接受依据：Human 在本会话明确要求“接受ADR-0009,制定完善计划并开始执行”，授权下列固定候选的官方核验、下载和隔离安装；不扩展候选、来源、预算或节点准入。
- 执行计划：[C7 专项计划](../coordination/C7-BRANCH-COVERAGE-PLAN-2026-10-10.md)。接受本 ADR 不等于工具可用或覆盖率达标。
- 关联：DEV-02-07/11/12、Gate C；[C7 探针](../reviews/C6-C7-ADMISSION-REVIEW-2026-10-09.md)与[当前测量](../reviews/DEV-02-07-AUDIT-CONSUMER-REVIEW-2026-10-10.md)

## 需要解决的问题

现有 Rust1.98 stable + Apple LLVM 实际执行了正反路径和当前业务测试，但 branch mapping 为0。Domain 行91.90%、Application 行93.41%及 region 数字不能证明分支≥85%。AGENTS.override.md 冻结工具/门禁路径的变更要求新/替代ADR并由Human明确确认；本提案不改变覆盖率门槛。

## 候选与明确授权范围

候选固定工具链 ID 为 **nightly-2026-10-09**，首发 host `aarch64-apple-darwin`，最小 profile，加该工具链自己的 `llvm-tools-preview`。提案时该 pin 尚未联网核验；Human 于2026-10-10接受后，官方核验、隔离安装与真实branch探针已通过：rustc1.101.0-nightly、配套LLVM23.1.3。首次生产专项Application分支77.78%未达85%，来源/校验见[首次验收](../reviews/C7-BRANCH-COVERAGE-REVIEW-2026-10-10.md)；本轮限定补测后行93.75%/branch100%，最终stable Mac21与11单节点重签见[补测验收](../reviews/DEV-02-07-BRANCH-TEST-REVIEW-2026-10-10.md)。Gate C整体和07整节点仍阻塞。

接受本 ADR 将授权：只从 Rust 官方静态发布服务查询该候选的 manifest/校验元数据，下载并通过现有 rustup 安装这一个固定工具链和对应 LLVM tools，仅用于当前本地覆盖率测量；不登录账号、不使用付费服务、预算为0。需要联网下载和新增本地工具/临时构建空间，具体大小以官方 manifest 和安装实测保存，不编造精确占用。

如果候选不存在、下载/校验失败、不支持branch选项或探针不能输出真实branch mapping，则保持BLOCKED、保留失败证据；**不自动改用最新nightly、不换来源、不使用RUSTC_BOOTSTRAP**。改候选或增加工具须另行更新方案并获明确决定。

## 隔离和复现方案

1. 保存官方URL、manifest与校验值、工具链 ID、rustc/LLVM版本、命令和安装来源。先核对候选可用性与来源，再安装；不把未验证字符串写成版本证据。
2. 生产 `rust-toolchain.toml`、manifest/lock与完整 Mac21 继续 Rust1.98 stable。专项命令显式 `cargo +nightly-2026-10-09`，独立仓外 target/profile；不改变 rustup default、不复用发布产物、不变更源码或协议语义。
3. 先对一个受控 if 执行一条和两条路径，使用该工具链配套 LLVM合并/导出。必须证实存在 branch mapping、漏一路时不足、两路时达到预期；编译、profile、mapping缺失或版本不兼容均失败关闭。region绝不代替branch。
4. 探针成立后，使用精确当前production domain/application源及真实领域/SQLite套件，保存前后源码SHA、可执行文件、原始profiles、映射、逐文件真实分子/分母；分别核对行≥90%与branch≥85%，未知/空分母用null。
5. Nightly测试产物不是生产stable证明。正式节点仍同时要求当前stable Mac21、风险匹配测试、专属标准和其他门禁；不得仅凭覆盖率批量恢复DONE。

## 回退与非目标

新工具只承担测量，可停止专项命令并继续原stable环境，不回退数据库。保留报告/映射与必要profiles；本授权允许清理本轮新建专项target和卸载本轮新装候选，不能删除既有工具、用户数据、历史证据或覆盖已有target。未经接受不执行任何安装或清理。

本 ADR 不接受 ADR-0007、不处理 PENDING 状态冲突、不改变模型路由/发布权限，不访问真实Harness/凭据/生产数据。Human 接受本提案后仍需真实探针和实测；接受本身不是branch达标或节点DONE。
