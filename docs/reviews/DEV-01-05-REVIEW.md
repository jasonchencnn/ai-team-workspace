# DEV-01-05 macOS + Ubuntu CI

- 执行模型：`gpt-5.6-terra`；总控完成冲突整合与第二遍反例审查。
- 关联要求：DEV-01 工程底座；Gate B 的 macOS/Ubuntu required checks；依赖、秘密、许可证与 Rust unsafe 门禁。
- 范围与非目标：建立 CI、Dependabot、依赖安全与许可证策略，以及可在本地复核的 workflow 静态契约。不修改领域、协议、状态机、migration 或 `sources/`。GitHub branch protection 属于远端仓库设置，不由 workflow 文件自行声明。
- 领域与数据影响：无。

## 变更与文件归属

- `.github/workflows/ci.yml`：macOS/Ubuntu 矩阵、固定工具版本、分层测试、Rust locked 检查、生成漂移、Gitleaks、漏洞与许可证门禁，以及唯一聚合项 `ci / required checks`。
- `.github/dependabot.yml`：GitHub Actions、npm、Cargo 更新源。
- `.github/cargo-deny.toml`：Rust 许可证 allowlist；私有 workspace crate 不参与发布许可证判定。
- `scripts/ci/verify-workflows.mjs`、`scripts/ci/check-rust-unsafe.mjs`：检查 job、平台、固定 Action、关键命令、最小权限与 required-checks 依赖关系，并拒绝业务 Rust 源码中的 `unsafe` 或 unsafe allow。
- `scripts/ci/check-node-licenses.mjs`、根 `package.json`：扫描 `pnpm list --recursive --depth Infinity` 解析出的全部可达传递依赖；不再使用只能看到 pnpm 直接链接的 `license-checker`。
- 各 Rust package manifest：总控串行补充 `publish = false`，使 cargo-deny 正确识别内部 crate；未改变依赖或构建语义。
- `apps/desktop/package.json`、`pnpm-lock.yaml`：总控将 Vite `7.3.1` 升至同 major 的 `7.3.5`，清除审计发现的 3 个 high advisories。

## TDD 与反例证据

| 阶段     | 证据                                   | 结果                                                                                         |
| -------- | -------------------------------------- | -------------------------------------------------------------------------------------------- |
| RED      | `node scripts/ci/verify-workflows.mjs` | 缺少 workflow 时失败。                                                                       |
| GREEN    | 同一命令                               | PASS：全部静态 CI 契约成立。                                                                 |
| 反例审查 | 总控检查 Rust matrix 展开              | 修复了会产生缺失 `os/check/command` 组合的笛卡尔积矩阵，改为显式 `include`。                 |
| 反例审查 | `cargo deny` 配置解析                  | 删除当前 cargo-deny 不支持的 `unlicensed` key，补充 Rust 工具链实际许可证并忽略私有 crate。  |
| 反例审查 | Node 传递许可证覆盖                    | 发现 `license-checker` 在 pnpm workspace 下只覆盖直接链接，替换为 280 个可达包的确定性扫描。 |
| 反例审查 | Ubuntu Tauri 编译前置                  | 补充 WebKitGTK、AppIndicator、RSVG、XDo 与 patchelf 系统包，并把包名纳入静态契约。           |
| 漏洞修复 | `pnpm audit`                           | Vite 7.3.1 的 3 个 high advisory 通过升级到 7.3.5 清零。                                     |

## 本地验证

验证环境：macOS；Node 24.21.0、pnpm 10.33.0、Rust 1.98.0。

| 门禁               | 命令/检查                                                                                           | 结果                                                                                         |
| ------------------ | --------------------------------------------------------------------------------------------------- | -------------------------------------------------------------------------------------------- |
| 安装与工具链       | `pnpm install --frozen-lockfile --force`；`pnpm run check:toolchain`                                | PASS；锁文件无更新，版本符合冻结基线。                                                       |
| Workflow 契约/YAML | `node scripts/ci/verify-workflows.mjs`；Ruby YAML 解析                                              | PASS。                                                                                       |
| 格式/类型/Lint     | `pnpm run fmt:check`；`pnpm run typecheck`；串行 `pnpm run lint:ts`；workspace Clippy `-D warnings` | PASS。并行运行 guard 与 ESLint 会因 guard 临时负样例产生竞态，改为串行后通过。               |
| 测试/构建          | `pnpm run test`；`pnpm run build`；Cargo workspace locked test/build                                | PASS：TS 35/35；Rust 25/25；Vite 7.3.5 build 成功。                                          |
| 生成漂移           | `pnpm run contracts:check`；`cargo metadata --locked --no-deps --format-version 1`                  | PASS。                                                                                       |
| Node 漏洞          | `pnpm audit --registry=https://registry.npmjs.org --audit-level=high`                               | PASS：high/critical 为 0；仍有 1 low、1 moderate。                                           |
| Rust 漏洞          | `cargo-audit 0.22.0 audit`（更新公告库）                                                            | PASS：无 failure-class vulnerability；报告 7 个 Tauri/GTK 传递依赖 warning。                 |
| Rust 许可证        | `cargo-deny 0.18.6 check --config .github/cargo-deny.toml licenses`                                 | PASS；仅 3 个 allowlist 未命中提示。                                                         |
| Node 许可证        | `pnpm run licenses:check`                                                                           | PASS：检查 280 个可达传递包；MIT 217，其余 Apache/BSD/ISC/MPL/BlueOak/CC-BY 均在 allowlist。 |
| Rust unsafe        | `pnpm run unsafe:check`                                                                             | PASS：业务 Rust 源码无 `unsafe` 或 unsafe allow。                                            |
| 秘密静态筛查       | 对工作区执行 AWS key、私钥、GitHub token、OpenAI key 模式扫描                                       | PASS：无命中；测试用脱敏 secret 不匹配真实凭据格式。                                         |
| Diff 健康          | `git diff --check`                                                                                  | PASS。                                                                                       |

## Required checks 与远端证据

`ci / required checks` 聚合 toolchain、workflow validation、format、lint、typecheck、五层测试、Rust locked test/build/clippy、contract drift、secret scan、dependency vulnerabilities 和 dependency licenses；任一前置 job 非 success 即失败。Workflow 仅授予 `contents: read`，不引用 `secrets.*`，缓存不替代 `pnpm install --frozen-lockfile`。

当前仓库没有 Git remote，因此无法：

1. 在 GitHub Actions 的真实 macOS 与 Ubuntu runner 上回放全部 job；
2. 验证固定 Action 与 Gitleaks 在远端环境可用；
3. 将 `ci / required checks` 配置为 branch-protection required check 并读取其绿灯证据。

按照 `QUALITY_GATES.md` “外部检查无法执行不得写成已验证”的规则，本节点不能标记 DONE。

## 遗留风险与回退

- RustSec 报告 6 个 unmaintained warning，以及 Linux GTK 链上的 `glib 0.18.5` unsound warning（RUSTSEC-2024-0429）；均来自现有 Tauri 传递依赖，不是本节点新增。它们不是 high/critical 未豁免项，但后续 Tauri 升级应持续跟踪。
- npm 仍有 1 low、1 moderate，当前 high/critical 门禁通过；后续依赖更新继续消化。
- 回退 CI 文件和脚本不会改变数据；Vite 回退会重新引入 3 个 high advisory，因此不建议回退该安全修复。

- 审核结论：**REVIEW**（本地实现与门禁通过；缺少 macOS/Ubuntu 远端 Actions 和 branch-protection required-check 证据）。
