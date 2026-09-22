# DEV-01-02 固定 Node、pnpm、Rust 工具链和 lockfile

- 执行模型：`gpt-5.6-terra`
- 关联需求/AC/Figma：DEV-01-02；Gate B 的可重复安装、构建和锁文件基础要求。
- 范围与非目标：固定工程工具链与依赖解析结果；不实现业务、领域语义、协议、迁移、UI 或 CI，不修改 `sources/` 或冻结规格。
- 变更文件/模块：`.nvmrc`、`rust-toolchain.toml`、根 `package.json`、`pnpm-lock.yaml`、`Cargo.lock`、`scripts/check-toolchain.mjs`、`.gitignore`。
- 领域与数据影响：无。`Cargo.lock` 由根工具链所有者在并行领域 Agent 稳定后更新，仅解析其已声明的 Serde 依赖，不改变领域源码或测试。
- 安全与隐私影响：无网络运行时能力或秘密；版本校验只读取本地工具版本。`.gitignore` 排除本地依赖、构建/覆盖率报告、环境文件和系统噪声，不忽略 lockfile 或生成契约。
- 验证命令：
  - `PATH=/opt/homebrew/opt/node@24/bin:$PATH pnpm install --frozen-lockfile`
  - `PATH=/opt/homebrew/opt/node@24/bin:$PATH pnpm run check:toolchain`
  - `cargo metadata --locked --no-deps --format-version 1`
  - `cargo build --workspace --locked`
- 验证结果：全部退出码为 0。锁定版本为 Node `24.21.0`、pnpm `10.33.0`、Rust `1.98.0`；冻结安装没有改变 `pnpm-lock.yaml`，Cargo 的 `--locked` 元数据与构建均可解析。
- 失败/边界用例：初始环境只有 Node 22，`pnpm` 正确报告 engine 不匹配；已安装并显式使用 Node 24.21.0 后通过。`pnpm env` 不能管理非 standalone 安装，改用系统包管理器安装 Node 24；项目不依赖该全局管理方式。
- 视觉/无障碍证据（适用时）：不适用；本节点无 UI。
- 迁移与回退：无数据或协议迁移。回退时移除本节点工具配置与 lockfile 变更即可；不影响持久化数据。
- 遗留风险：Node 24 与 Ubuntu 的 required CI 检查归 DEV-01-05；当前只证明本地锁定环境可重放。
- 审核结论：**PASS**
