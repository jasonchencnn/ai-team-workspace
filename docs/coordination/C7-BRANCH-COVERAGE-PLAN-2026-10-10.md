# C7 真实分支覆盖率专项计划（2026-10-10）

> 执行完成：[专项验收](../reviews/C7-BRANCH-COVERAGE-REVIEW-2026-10-10.md)。官方核验/隔离安装/探针通过；Domain行91.67%、branch93.36%达标，Application行91.25%、branch77.78%未达标。Gate C和正式账本不晋级；补测方向已列出。

## 授权、节点与写集

Human 已明确接受 [ADR-0009](../adr/0009-isolated-branch-coverage-tool-proposal.md) 并要求执行。关联 DEV-02-07/11/12 与 Gate C；本包是已获准的阻塞证据补齐，不启动下游开发。正式账本仍为 32 DONE / 9 BLOCKED / 60 PLANNED。

固定候选 nightly-2026-10-09、host aarch64-apple-darwin、minimal profile、配套 llvm-tools-preview；仅访问 Rust 官方静态服务，预算 $0，无账号登录。生产 Rust1.98.0、manifest/lock、冻结规格、领域语义和门槛均保持不变。未知或空分母为 null，region 不替代 branch。

主控负责官方核验、串行安装、探针、测量及最终复核。当前可调用 Subagent 型号不包含仓库允许的 gpt-5.6-luna / gpt-5.6-terra，因此不创建高阶替代 Subagent，不声称独立模型审核。最终进行第二遍证据反例核对。

仓内写集仅为 ADR-0009、本文、专项报告/小型证据及现行进度入口（DEVELOPMENT_NODES、coordination README/PROGRESS/WORKLOG、docs README）。源码、测试、根 manifest/lock、既有证据和 sources/ 不修改。大型下载、target、profile 与导出存入本聊天仓外专属目录；不覆盖已有 target。

## 串行执行与停止条件

1. **输入基线**：保存生产源码、测试、迁移、manifest/lock、rust-toolchain 的 SHA256、当前 rustup/default/installed list 与生产编译器版本。取本轮前后快照核对，日期按 Asia/Shanghai 记录。
2. **官方可用性/完整性**：请求 `https://static.rust-lang.org/dist/2026-10-09/channel-rust-nightly.toml` 及其 `.sha256`，保存 URL、HTTP 状态、返回头、原始 body、命令、退出码和 SHA256。必须 HTTP200、manifest 解析成功且 SHA256 匹配；核对 manifest 日期、host、rust/rustc/cargo/rust-std/llvm-tools-preview 的可用字段及官方 HTTPS 下载 URL。失败即 BLOCKED，不安装、不查询其他日期或镜像。
3. **隔离安装**：通过现有 rustup，在全新仓外 RUSTUP_HOME 中安装这个 pin，`--profile minimal --component llvm-tools-preview`，显式限定官方 dist/update root；不改用户 rustup default。下载包校验由 rustup 执行并保留日志，与已验证 manifest hash 对照。记录版本、二进制 SHA、实际目录大小；构建使用独立 CARGO_TARGET_DIR，生产 lock 只读，禁用 RUSTC_BOOTSTRAP。
4. **探针准入**：用该工具链 rustc 编译仓外受控单个 if，显式 `-C instrument-coverage -Z coverage-options=branch`。分别运行单路与双路，使用同 sysroot 的 llvm-profdata / llvm-cov 合并与 JSON 导出。检查同一个源存在非零 branch mapping、单路覆盖 1/2、双路覆盖 2/2（并核对 true/false 计数）；分子/分母不能仅根据输出文字推断。缺 mapping、编译/运行/合并失败、工具版本不匹配均 BLOCKED。
5. **生产源专项**：探针通过后，显式 `cargo +nightly-2026-10-09`、`--locked --offline` 与独立 target/profile 编译并运行现有 Domain 套件及 `persistence-sqlite` 的 atomic_message_expiry / audit_event_consumption 真库套件。从 cargo JSON 获取可执行文件，父测试实际执行崩溃用例；普通空 fixture 入口不当作额外业务用例。记录每个执行退出码、原始 profraw、profdata 和完整 LLVM JSON。依赖缺失时不自动联网补依赖或改 lock。
6. **分母与审核**：只统计当前 domain/application 的 production src；tests、examples、fixtures、其他 crate 与第三方依赖不入分母。逐文件记录映射存在性、line/branch covered/count、源码 SHA；无可执行映射的模块声明单列，不伪报100%。分别核对 Domain 和 Application 行≥90%、分支≥85%，保持缺失映射/零分母为未知。前后 SHA 不同则该轮证据失效。不足仅报告真实缺口，未经新增任务包不在本专项顺手修改业务或测试。
7. **记录和准入**：保存结果、失败阶段和可重放命令；同步现行进度。nightly 测量不替代 stable Mac21、节点专属标准或正式总控准入。只改文档时核验链接、写集及生产输入不变，不重跑无变化的完整工程门禁；如源码后续变化，须按新输入重跑门禁与测量。

## 回退

任何步骤失败停止依赖步骤，保留失败证据及 null 指标，不自动换 pin、来源、工具或使用 RUSTC_BOOTSTRAP。已有 stable 可继续使用，无数据库回退。本轮新建隔离目录可按 ADR 授权清理，但执行失败时优先保留诊断；不删除用户既有工具或历史证据。换候选须新的明确 Human 决定。

## 验收清单

- 官方 HTTP/manifest/校验、host 和 component 检查明确通过后才安装。
- 安装、探针与生产测量分别有可复核状态；未执行不写为通过。
- 单路/双路探针能区分遗漏结果，branch 明确是真实分支结果分母。
- production 输入与用户 stable/default 不变，预算为0，无业务/协议改动。
- 当前正式节点和 Gate C 不因接受 ADR 自动晋级。
