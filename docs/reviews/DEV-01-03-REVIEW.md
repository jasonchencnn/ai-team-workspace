# DEV-01-03 配置 fmt/lint/typecheck/clippy 基线

- 执行模型：`gpt-5.6-terra`
- 关联需求/AC/Figma：DEV-01-03；G2 静态质量门禁。
- 范围与非目标：建立 TypeScript 严格检查、Prettier、ESLint、Rust rustfmt 与 Clippy 基线；不实现业务规则、领域类型、状态机、协议或 UI。
- 变更文件/模块：`tsconfig.json`、`eslint.config.mjs`、`.prettierrc.json`、`.prettierignore`、根 `package.json`、`scripts/verify-tooling-guards.mjs`、`.gitignore`，以及非领域空 crate placeholder 的最小 rustfmt 空行修正。`typecheck` 串联 root `tsc --noEmit` 与 `pnpm --recursive --if-present run typecheck`；Prettier 显式覆盖 `apps`、`packages` 和既有根文件。`.prettierignore` 仅排除依赖、构建/覆盖率和桌面 `dist`/Tauri `gen` 产物，不排除 `contracts-ts` 生成契约。ESLint 仅忽略明确的桌面构建 `apps/desktop/dist/**` 与 Tauri 生成 `apps/desktop/src-tauri/gen/**`，不忽略桌面源码。
- 领域与数据影响：无。未修改 `crates/domain`；全工作区检查仅在并行领域 Agent 稳定后以只读方式运行。
- 安全与隐私影响：无。违规守卫在系统临时目录和 `tests/` 下短暂创建样例，`finally` 中删除；不保留故意违规源码，也不访问秘密。
- 验证命令：
  - `pnpm run fmt:check`
  - `pnpm run typecheck`
  - `pnpm run lint`
  - `pnpm run verify:guards`
- 验证结果：全部退出码为 0。TypeScript 启用 `strict`、`noUncheckedIndexedAccess`、`exactOptionalPropertyTypes`；root 类型检查和递归子包类型检查均通过（desktop、contracts-ts、design-tokens，pnpm `Scope: 3 of 4`，root 未递归）；ESLint 零错误（含桌面 TS/TSX 源码，排除已声明构建产物）；`cargo clippy --workspace --all-targets --all-features -- -D warnings` 通过；Prettier 检查 root、apps 与 packages 的受支持源码且与 `cargo fmt --all -- --check` 均零差异。
- 失败/边界用例：`verify:guards` 从项目根运行 ESLint，并断言输出命中 `@typescript-eslint/no-explicit-any`；临时 Rust 未使用变量必须输出 `unused variable`、`unused_variables` 或 warnings-denied 诊断。脚本仅在两项预期违规都被对应规则拒绝时成功，并清理临时文件。
- 视觉/无障碍证据（适用时）：不适用；本节点无 UI。
- 迁移与回退：无数据、协议或运行时迁移。回退时移除静态工具配置与脚本；不影响业务数据。
- 遗留风险：规则基线只覆盖当前工程源码；后续 Tauri/React 与业务模块新增后仍须纳入同一命令，并由后续节点补齐风险匹配的测试。
- 审核结论：**PASS**
