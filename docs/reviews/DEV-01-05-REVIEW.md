# DEV-01-05 macOS + Ubuntu 质量门禁审核记录

- 节点：`DEV-01-05`
- 状态：`DONE`（本记录纳入最终输入快照，且下方“终审生效条件”全部满足时生效；在此之前保持 `REVIEW`）
- 执行模型：实现与独立审核均使用 `gpt-5.6-terra`；总控使用 `gpt-5.6-sol` 完成整合与最终门禁。
- 决策依据：Human 于 2026-09-23 批准 ADR-0003，接受 macOS ARM64 + Ubuntu ARM64 覆盖，以及私有零预算阶段没有 GitHub branch protection/required-check 服务端强制的差异；新增费用上限为 `$0`。
- 范围：CI workflow、依赖/许可证/秘密/Rust unsafe 门禁、零费用双环境回放和可复核证据。未修改领域语义、migration 或 `sources/`。

## 结论

DEV-01-05 采用 ADR-0003 的零费用替代门禁完成验收。仓库中的 GitHub workflow 和聚合项 `ci / required checks` 继续保留，但本次结论不声称已经启用 GitHub-hosted runner、branch protection 或 Ubuntu x86_64 验证。

同一规范化输入快照必须在以下两个独立环境运行仓库内同一门禁脚本并全部通过：

1. 本机 macOS ARM64；
2. Oracle Cloud Ubuntu ARM64 隔离容器。

最终输入快照包含 151 项受版本控制或待纳入实现的文件。权威总 SHA-256、双端 JSON/日志 SHA-256、工具来源、镜像与历史扫描摘要由最终证据包生成并由独立审核逐项复核；本记录不嵌入包含自身的最终总哈希，避免产生无法稳定的自引用哈希。

## 变更与文件归属

- `.github/workflows/ci.yml`：macOS/Ubuntu CI 定义、固定工具版本、测试、构建、生成漂移、Gitleaks、漏洞、许可证和聚合 required-check 契约。
- `.github/dependabot.yml`、`.github/cargo-deny.toml`、`.gitleaks.toml`：依赖更新、Rust 许可证和秘密扫描策略；Gitleaks 只保留两个精确 fixture 假阳性例外，并排除依赖缓存、构建产物和 AppleDouble 文件。
- `scripts/ci/verify-workflows.mjs`、`scripts/ci/check-rust-unsafe.mjs`、`scripts/ci/check-node-licenses.mjs`：静态 workflow 契约、Rust unsafe 和 Node 传递许可证检查。
- `scripts/ci/local-gates.mjs`：21 条门禁、冻结工具版本强制、完整输入清单、前后快照一致性、证据排他写入和失败关闭。
- `scripts/ci/local-gates.test.mjs`：输入规范化、路径/文件类型拒绝、工具身份、命令计划、证据安全和 Gitleaks 边界测试。
- `scripts/ci/ubuntu-gate.Dockerfile`：固定 Ubuntu 基础镜像 digest、下载校验、pnpm integrity、Rust 工具精确版本与 `--locked` 安装。
- `docs/adr/0003-zero-cost-dual-environment-quality-gates.md`：Human 已接受的零费用差异与恢复条件。

根 manifest、lockfile 和 `sources/` 未在本轮补强中修改。

## TDD 与反例记录

| 阶段            | 结果                                                                                               |
| --------------- | -------------------------------------------------------------------------------------------------- |
| RED：workflow   | 缺少 workflow 时静态验证按预期失败。                                                               |
| GREEN：workflow | job、平台、固定 Action、关键命令、最小权限和聚合依赖关系全部通过。                                 |
| RED：本地门禁   | 模块缺失、预检顺序错误、输入变化、符号链接、非普通文件、路径逃逸、版本漂移等反例按预期失败。       |
| GREEN：本地门禁 | `node --test scripts/ci/local-gates.test.mjs` 为 11/11 PASS。                                      |
| RED：macOS      | 首轮发现系统 Node 版本不符、npm 镜像不支持 audit，以及构建缓存二进制元数据假阳性；失败证据均保留。 |
| RED：Ubuntu     | 首轮发现 AppleDouble 文件导致格式门禁失败；失败证据保留。                                          |
| RED：历史/汇总  | 首轮暴露 Gitleaks 8.30 参数变化和停止容器的网络端点判断问题；修正后重跑，不把失败冒充为 PASS。     |
| GREEN：双环境   | macOS ARM64 与 Ubuntu ARM64 均完成 21/21 门禁，输入快照前后一致。                                  |

## 门禁覆盖

两个环境执行同一套检查：

- Node/pnpm/Rust/Gitleaks/cargo-audit/cargo-deny 版本与身份；
- Gitleaks 文件快照扫描；
- frozen-lockfile 安装与冻结工具链校验；
- workflow 静态契约；
- Prettier、ESLint、TypeScript 类型检查；
- TypeScript 与 Rust 测试；
- Web 与 Rust workspace 构建；
- Clippy `-D warnings`；
- Adapter/Schema 生成漂移；
- Rust unsafe；
- Node/Rust 漏洞；
- Node/Rust 许可证。

验证基线：Node 24.21.0、pnpm 10.33.0、Rust/Cargo 1.98.0、Gitleaks 8.30.0、cargo-audit 0.22.0、cargo-deny 0.18.6。测试基线为 TypeScript 35/35、Rust 25/25；安全门禁要求 high/critical 级未豁免漏洞为 0。

## Git 历史与供应链证明

- 真实仓库必须为非 shallow clone，记录 HEAD、全部 refs 和全部可达对象，并用 Gitleaks 8.30.0 对 `--all` 可达历史扫描。
- 审核基线为 9 refs、241 个可达对象、4 个提交，历史扫描无秘密命中。
- macOS 下载型工具记录官方 URL 与 SHA-256；Rust 工具记录 crate/version/lock。
- Ubuntu 基础镜像固定为 `ubuntu@sha256:008173c23f95b170204355c12626cb5a965d779a7e1283b09e9cffbb1bf33ca3`，同时封存 APT 包清单、镜像 ID和工具来源。

## Oracle 隔离边界

Ubuntu 门禁仅在测试隔离环境运行：

- 普通用户 `1000:1000`；
- read-only root filesystem；
- 2 CPU、8 GiB、PID 1024；
- `cap-drop=ALL`、`no-new-privileges`；
- 专用 bridge 网络；
- 只挂载代码、证据和隔离 HOME；
- 不挂载 Docker socket、生产目录、生产卷或生产网络；
- 不访问 `~/readest/docker`；
- `sing-box` 前后保持 active/running，状态文件完全一致。

## 独立审核

第一次最终候选证据包位于 Oracle 的 `~/ai-team-gate-evidence-dev0105-final-v4`。独立 `gpt-5.6-terra` 审核确认：根 `SHA256SUMS` 34 项全部匹配；双端均为 151 项输入、21/21 门禁、前后快照一致；工具来源、APT、镜像、容器/网络 inspect、Git 历史和失败记录完整；`sources/` 未修改；结论为 PASS。

该候选证据形成后本审核记录发生更新，因此不能把候选包冒充当前最终快照。总控必须生成包含本记录的下一版双环境证据，并交由同一独立审核再次核验。

## 终审生效条件

以下条件全部成立时，本记录顶部状态生效为 `DONE`，无需再次修改本文件：

1. 包含本记录的最终规范化输入快照在 macOS ARM64 与 Ubuntu ARM64 均为 21/21 PASS；
2. 两端记录的 151 项输入清单总 SHA-256 相同，且门禁前后不变；
3. 非 shallow 全 refs Git 历史扫描重新通过，并绑定同一输入总哈希；
4. Oracle 镜像、APT、工具来源、容器/网络 inspect 与 `sing-box` 状态证据重新封存且校验值有效；
5. 独立 `gpt-5.6-terra` 终审返回 PASS；
6. 新增费用为 `$0`，且没有提交、推送、部署、浏览器操作、生产目录访问或 `sources/` 修改。

任一条件失败时，本节点仍为 `REVIEW`，不得启动依赖节点。

## 已接受差异与遗留风险

- 没有 GitHub branch protection/required-check 服务端强制；依赖总控维护节点状态。
- 没有 GitHub-hosted runner 证据，也不证明 hosted 镜像或缓存行为。
- 平台覆盖是 macOS ARM64 + Ubuntu ARM64，不包含 Ubuntu x86_64。
- RustSec 的现有 Tauri/GTK 传递依赖 warning，以及 npm 的 low/moderate 项继续跟踪；它们不属于当前 high/critical 阻断项。
- 任何源码、未跟踪输入、锁文件、门禁脚本、workflow、策略、工具版本、来源或安全公告库变化都会使旧证据失效，必须重跑受影响平台。

## 回退

- 若零费用替代条件失效，DEV-01-05 恢复为 `REVIEW/BLOCKED`，依赖节点立即停止。
- 只有 ADR-0003 列出的恢复条件全部满足，并再次获得符合费用政策的 Human 授权后，才能把 GitHub-hosted required checks 重新作为 Gate B 的事实来源。
