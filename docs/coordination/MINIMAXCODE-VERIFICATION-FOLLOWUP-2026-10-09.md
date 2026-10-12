# Minimaxcode AF验收后的精确接续任务表（2026-10-09）

Human可直接转发本表。依据 [AF独立审计](../reviews/AF-RETURN-AUDIT-2026-10-09.md)。主责Minimaxcode，实际模型/版本回填；Codex独立复核与正式准入。AF01漏检与AF02存量概念实验已限定接受，不要求重做三个已交付包；只补以下两个互斥包。

**当前20 DONE / 21 BLOCKED / 60 PLANNED不变；ADR0007 Proposed、下游节点/V2/生产迁移/发布未获准。** 当前HEAD `1a14b8795e76fc373bef326eb5df96f9ac173f71`＋继承dirty工作树，输入重新固定，不能checkout HEAD丢掉前轮成果。

## 公共要求

- 依序读取AGENTS/override、四开发基线、specs索引、相关Frozen规格、对抗审查手册、本轮报告；每包关联DEV节点。
- 全程已有本地工具/offline/费用$0；不提交/推送/部署/安装下载/生产操作/外发消息。不reset/clean/stash、不删除大量文件。
- sources、旧迁移0001–0029、旧AF/FC报告及evidence、根manifest/lockfile、正式节点/ADR状态只读。本轮主控`.gitleaks.toml`和observability最终两文件只读，哈希不得改变。
- 新建结果 `docs/coordination/minimax-verification-next/VF-XX-RESULT.md`，新证据 `docs/reviews/evidence/minimax-verification-next/VF-XX/`。每次尝试独立raw log，命令/exit/模型/输入输出SHA/非空性/反例/还原/未验俱全。
- 两包不能同时跑最终全仓门禁；VF02等VF01源码稳定后再收口。任何失败保留，不用绿重跑覆盖，不自判DONE。

| 包 | DEV节点与难度 | 主责/复核 | 唯一允许写集 | 完成条件 |
|---|---|---|---|---|
| VF-01 | DEV-03-02，关联DEV-01-05；进程测试/故障诊断，Terra级难度 | Minimax主做，Codex独立进程复核 | `crates/adapter-supervisor/src/bin/fixture_adapter.rs`、`crates/adapter-supervisor/tests/supervisor_fixture.rs`＋本包新增结果/证据 | shutdown/Drop都确实形成pipe-holder；记录成功退出与有界终止；隔离、target并行、workspace默认/串行均有独立实测，失败不掩盖 |
| VF-02 | DEV-01-05/01-08/02-08，Luna级整理、主控裁决 | Minimax收口，Codex重放 | 本包新增结果/证据、`docs/coordination/minimax-verification-next/VF-02-final-index.md` | 当前输入口径准确；所有门禁由新rawlog派生；已关闭与未验分列；不改旧原件 |

## VF-01：真实pipe-holder就绪与失败定位

主控默认全仓 **652/1/1 exit101**；精确隔离目标 **0/1 exit101**；target完整并行29/0；全仓串行653/0/1。原始日志 `af-return-audit-2026-10-09/main/rust-tests.log`、`watchdog-isolated.log`、`fixture-parallel.log`、`rust-tests-serial.log`。

失败实际在子测试 `report.status.success()`（supervisor_fixture.rs:374），外层:142仅报告子测试exit101。没有当时的report status/termination/stderr，因此不能确定唯一根因，更不能写“隔离永远通过、仅全仓负载失败、已排除所有影响”。Drop分支start后立即drop，没有pipe-holder已形成的证据。

必须：

1. 在测试fixture中建立**目标后代真实就绪确认**：后代已启动且持有继承pipes，用本轮唯一token/独占临时资源关联，避免固定旧文件或伪造readiness。shutdown和Drop两段均等待该确认后才执行被测关闭操作。直接父进程可在就绪后按EOF协议退出；测试启动等待和被测关闭边界分别记录，完整总边界仍保留。
2. 记录并断言direct child status、`ExitReport.termination`及已有可观测stderr/输出。验证后代退出/同组回收，不能仅凭Drop返回快或“没有延迟marker”推出子进程曾存在；若某项不可观测，明确保留边界。
3. **不增加现有1s grace、3s总上界、6s watchdog、10s持有值**，不接受ForcedShutdown冒充正常成功、不删success断言、不新增固定sleep换绿。 readiness等待可用现有受控测试启动预算，不能替代关闭性能断言。
4. 对新测试做独立副本真实反例：未形成目标后代、阻塞/错误关闭、忽略readiness等必须由有意义断言检出；编译失败、0test filter不算检出。缺少可重现反例如实报告。
5. 用原始默认命令复验，不能只改成单线程规避。若失败，保留ExitReport与环境/并发证据，不加大时限猜测修好。若证实生产supervisor问题，先回传最小反例与具体建议写集；本包禁止修改src/lib.rs、协议、时间策略或manifest。

验收至少：

```sh
cargo test -p adapter-supervisor --test supervisor_fixture --locked --offline descendant_pipe_holders_cannot_block_shutdown_or_drop -- --exact --nocapture
cargo test -p adapter-supervisor --test supervisor_fixture --locked --offline
cargo test -p adapter-supervisor --locked --offline
cargo test --workspace --locked --offline --no-fail-fast
cargo test --workspace --locked --offline --no-fail-fast -- --test-threads=1
cargo clippy -p adapter-supervisor --all-targets --all-features --locked --offline -- -D warnings
cargo fmt --all -- --check
```

多次执行以有限、预先登记的重复次数覆盖冷启动/隔离/并行，不进行“重复直到绿”选择性收集。保留完整次数、失败比率和失败日志，不能用一次绿声称绝无竞争。

## VF-02：最新验收索引与勘误映射

不重写旧AF原件，新增当前索引并引用主控勘误：

- AF01真正前轮输入为lib `d945dd5b…` / tests `f843839d…`，当前 `8d2fe94e…` / `368eff5f…`。HEAD9251/fcdf只是历史对照；`checkout HEAD`回退及“HEAD有29测试”作废。需要回退时仅由主控从上轮保留副本核验恢复，不在本包回退。
- 最长合法匹配已获主控接受。当前去重DP上界成立；M1、M2断言红；M3、M4存活不算kill。“distinct界证明duplicate工作同界”、无条件二叉指数模拟均不采信。资源验证不只有wall-clock方式，首次Vec扩容不写成绝对零分配。
- AF02存量概念/SQLx原子性已验证；**当前29版加载器对概念30报VersionMissing(30)**，不是兼容回退二进制。未来兼容方案须同时验迁移登记/checksum、启动、查询/写入和override历史；不DROP守卫、不删登记、不擅设ignore_missing。
- AF03历史38/38与当前门禁分列。原R10 15+1未证、新M14-R实验16/16保留分栏，domain91.76%为既有固定输入测量、application占位、branch仍未验。
- 本轮主控Gitleaks精确例外已整合：**规则级**路径AND完整行，19项正负对照，global路径提前豁免反例保留。不得搬回全局文件豁免、忽略整个evidence目录或修改例外凑绿。
- 所有当前最终数字从VF01稳定后的新日志派生；默认并行、串行、隔离结果分别记录，0passed且filtered>0不得计通过。Mac dry-run不是完整Mac21；在线advisory/install、真实Harness/Keychain等仍未运行。

运行与VF01相关的完整offline检查以及TS36/Gate25/replay8、lint/typecheck/build/fmt/contracts/guards/workflow/strict rustdoc、本地tree和all-refs secret scan。已有工具支持多少就记录多少，缺工具不要下载。节点仍BLOCKED，直到Codex按Frozen专属标准和G0–G9独立正式准入。

## 下一步开发的条件队列

| 顺序 | 下一步 | 开工前提 |
|---|---|---|
| 1 | VF01、VF02收口与Codex重验 | 当前获准的精确整改范围 |
| 2 | 当前输入完整Mac21/branch有效instrumentation/剩余正式节点门禁 | 联网、账号或安装影响单独由Human确认；原21项证据不能沿用 |
| 3 | K1可信actor链→K2已冻结通知身份→K3应用事务/幂等/Event/Outbox→K4dispatcher/Run/fencing | 逐项依赖正式DONE；Codex另签边界/写集/测试任务包，Minimax执行 |
| 4 | Workspace/IPC/UI及真实Harness、恢复/A11y/性能 | 相关前置DONE及专属授权 |

ADR0007待决通知字典、V2 R1以后新语义、部署/发布不在自动队列中；本表不批准任何生产migration编号或新依赖。
