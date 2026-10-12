# C7 分支工具与真实测量验收（2026-10-10）

**工具核验、隔离安装和真实分支探针 PASS；生产源专项已完成，Application 分支阈值 FAIL。** Domain 行与分支均达标，Application 行达标、分支未达85%；Gate C 不通过。正式账本保持 **32 DONE / 9 BLOCKED / 60 PLANNED**，没有节点自动晋级。

依据为 Human 本会话“接受ADR-0009,制定完善计划并开始执行”、[已接受 ADR-0009](../adr/0009-isolated-branch-coverage-tool-proposal.md)、[执行计划](../coordination/C7-BRANCH-COVERAGE-PLAN-2026-10-10.md)、DEV-02-07/11/12、冻结开发基线和 Domain/State/Data/Error 规格。主控串行执行并第二遍交叉核对；允许的 Luna/Terra 不可调用，未声称独立模型审核。

## 官方核验与隔离

- 官方 manifest 与 `.sha256` 都 HTTP200、curl exit0、TLS verify0；manifest 日期为2026-10-09，923,466 bytes，SHA256 为 `930f5f10135531883913e308093fdc56c4223465fec412023566fd2efc5e711c`。官方 URL 为 `https://static.rust-lang.org/dist/2026-10-09/channel-rust-nightly.toml`，原始返回头/body 已保存。
- `nightly-2026-10-09-aarch64-apple-darwin`：rustc **1.101.0-nightly (a30aa9064 2026-10-08)**，commit `a30aa9064df6a504b6a323c6884a2f4cfbfff666`；rustc 与配套 llvm-cov / llvm-profdata 都 **LLVM23.1.3**。
- 现有 rustup 安装 minimal profile + llvm-tools-preview，exit0；仅安装 cargo、rustc、rust-std、llvm-tools-preview 四组件。安装到本聊天仓外全新 `rustup-home`，没有安装到用户既有Rustup目录、self-update、镜像或候选替换。
- rustup 校验下载包；安装后 manifest 的组件版本、host available、四组件的官方 gz/xz URL 与 hashes 再逐字段匹配官方 manifest。二进制 SHA 保存于 tools.json。工具链文件逻辑大小 **622,366,758 bytes**，专项构建/映射/profile等测量目录 **1,084,646,968 bytes**；这是最终文件大小，不冒称下载流量或实际磁盘块占用。
- 新隔离 RUSTUP_HOME 自身的 default 为该 nightly（rustup 首次安装行为）；用户既有 default、installed list、active toolchain 和 rustc verbose 前后完全相同，生产仍 Rust1.98.0。未使用 RUSTC_BOOTSTRAP、账号、付费服务、生产数据库或真实 Harness。

## 探针与实测

受控一个 if 编译时显式 `-C instrument-coverage -C link-dead-code -Z coverage-options=branch`，使用同 sysroot LLVM。单路 true/false计数 `[1,0]`，branch **1/2=50%**；双路 `[1,1]`，branch **2/2=100%**。源映射、可执行文件 SHA、两份原始 profile、合并 profile 和完整导出均保留。region 没有作为 branch 分母。

现有 Domain 套件 **287 passed / 0 failed**，两个真实 SQLite Application 套件 **30 passed / 0 failed / 1 filtered**；过滤的是普通空 `crash_child_helper` 入口，父用例实际执行三处崩溃。共19个测试 executable、150个 runtime profiles，合并和导出 exit0。使用 `--locked --offline` 和独立仓外 target，没有下载业务依赖或变更 lock。

| 生产范围 | 行 covered/count | 行覆盖率 | branch covered/count | 分支覆盖率 | 阈值 |
|---|---:|---:|---:|---:|---|
| Domain（15个映射源） | 1937/2113 | **91.67%** | 211/226 | **93.36%** | PASS |
| Application（2个映射源） | 146/160 | **91.25%** | 14/18 | **77.78%** | FAIL：分支<85% |

只统计当前 production src，排除测试、example、其他crate、依赖和 `domain/src/test_fixtures.rs`。Application lib.rs 只有模块声明，无可执行映射，单列 unmapped，未冒称100%。旧stable报告包含src内fixture，且编译器映射行数不同，旧91.90%/93.41%与本表不可当作相同分母的回退对比；旧证据原样保留。

LLVM 多个泛型实例会产生重复源位置记录。按 `(起止行列, file/expanded-file ID, kind)` 合并 true/false计数后，逐文件真实结果分母与 LLVM summary完全一致；再以独立 `llvm-cov report` 的文本TOTAL交叉验证上述分子/分母。导出中 branch 是该编译器提供的条件结果映射，不代表 MC/DC 或所有状态迁移证明；原有性质、错误与失败注入测试仍强制。

## Application 缺口与接续

| 源位置 | 未覆盖结果 | 当前 true/false 计数 | 后续测试方向 |
|---|---|---:|---|
| `audit_consumer.rs:133` | event Team 与 delivery Team 不一致 | 0/36 | 端口异常返回跨Team event；应用拒绝且rollback，无投影/marker副作用 |
| `audit_consumer.rs:138` | 已消费收据 event 与当前event不一致 | 0/5 | 端口返回不匹配收据；应用拒绝且rollback，不伪报Duplicate |
| `message_expiry.rs:164` | Team查询返回None | 78/0 | 不存在Team的真实入口，稳定ObjectNotFound且零副作用 |
| `message_expiry.rs:229` | 下个version超SQLite整数范围 | 0/58 | i64::MAX边界，真实库/受控端口验证IntegrityFailed及原子回滚 |

这些是未测防线，不是已经证明存在的业务漏洞。原SQLite实现更早拦截部分异常，真库负向测试通过不等于应用端口异常分支也被执行。后续应签发 DEV-02-07 限定测试包，保持源码和语义不变，按风险选择真库和端口替身补测；18结果分母至少还需覆盖2个才能达85%，建议四个全部补齐。补测会改变工程输入，需新一轮专项测量及stable canonical Mac21，不复用本轮身份作最终准入。DEV-02-11 可进入单节点证据复核候选；DEV-02-12 仍受08/09直接依赖阻断，均未在本包重签。

## 失败记录、证据和验证边界

两次证据装置问题如实保留：安装manifest最初全对象对比失败（rustup删除 git_commit_hash 元数据并补空components/extensions，版本/下载hash实际一致）；第一次测量汇总因把全部泛型实例记录当作独立分支而失败关闭。后续分别逐字段核验官方版本/URL/hash，以及按源位置合并并双报告核对。没有换候选、降低门槛、修改业务/测试或把失败装置输出称为达标。原始 LLVM导出和测试结果未重写，汇总修复使用已保存数据。

仓外完整目录：[c7-branch-coverage](/Users/chenzhiqing/.codex/visualizations/2026/10/10/01a1237c-0b1c-7121-9922-cc68642a02ff/c7-branch-coverage)。含官方manifest/headers/checksum、install结果/日志、安装核验、probe、measurement原始数据、逐文件结果、未覆盖分支、两份文本report、218项工程输入SHA前后快照及用户工具快照。摘要与证据digest见[仓内结果](evidence/c7-branch-2026-10-10/result.json)；可重放脚本为[run_specialist.py](evidence/c7-branch-2026-10-10/run_specialist.py)。使用新仓外输出运行 probe/measure；仅复核已有数据可用summarize模式。

218项工程输入前后SHA完全相同，覆盖率源前后相同；用户stable/default不变。文档/证据链接与脚本语法、当前写集另行核验。本轮没有工程代码改动，所以未重跑完整stable Mac21、fmt/clippy/TS lint；上一消费包stable完整Mac21保留为历史证据，不宣称本轮文档输出后的全仓身份已经复跑。

本专项工具路线完成，Application阈值与Gate C继续阻塞。ADR-0007、PENDING语义、dispatcher、权限/运行时/Workspace/UI、产品阶段和发布均未获本包准入。工具和证据保留便于补测；回退只需停止专项命令，必要时清理本轮新建仓外目录，不动用户工具、生产数据或历史证据。
