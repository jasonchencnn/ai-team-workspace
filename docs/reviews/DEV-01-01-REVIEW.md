# DEV-01-01 创建 workspace 与目录骨架

- 执行模型：`gpt-5.6-luna`
- 关联需求/AC/Figma：DEV-01-01；Gate B 的 workspace/build/test 基础要求；技术基线建议仓库结构
- 范围与非目标：创建 pnpm workspace、Cargo workspace、Tauri/React 入口目录、Rust crate 目录、Adapter/包/测试/迁移/脚本目录；不实现业务、状态机、权限、协议、迁移、UI 或生产集成，不修改 `sources/`、冻结规格或需求原始材料。
- 变更文件/模块：
  - 根清单：`pnpm-workspace.yaml`、`package.json`、`pnpm-lock.yaml`、`Cargo.toml`、`Cargo.lock`
  - 桌面入口：`apps/desktop/src/`、`apps/desktop/src-tauri/`（最小 Rust library manifest）
  - Rust crate：`crates/domain`、`application`、`contracts`、`persistence-sqlite`、`workspace`、`adapter-supervisor`、`security`、`observability`（均为最小空 library）
  - 其他骨架：`adapters/mock`、`adapters/codex`、`packages/contracts-ts`、`packages/design-tokens`、`packages/test-fixtures`、`migrations`、`tests/{contract,integration,e2e}`、`scripts`
- 领域与数据影响：无；Rust 空 crate 仅设置 `#![forbid(unsafe_code)]`，未定义领域类型、数据库表或业务 API。
- 安全与隐私影响：无运行时能力、无进程、无凭据、无网络访问；未添加 Tauri capability 或依赖。
- 验证命令：
  - `pnpm install --frozen-lockfile --offline`
  - `pnpm run build`
  - `pnpm run test`
  - `cargo metadata --no-deps --format-version 1`
  - `cargo build --workspace`
  - `cargo test --workspace`
- 验证结果：全部命令退出码为 0。pnpm 工作区当前没有子包项目，递归 build/test 输出 `No projects matched the filters` 后正常退出；Cargo workspace 的 9 个空 crate 均成功编译，单元测试和 doctest 均为 0 tests、0 failures。
- 失败/边界用例：首次验证发现 wildcard Cargo workspace 仅有目录占位而缺少 manifest，Cargo 无法加载成员；已为每个计划 Rust 成员补齐无业务的最小 manifest 与 `src/lib.rs`，随后全量验证通过。pnpm 子包将在后续节点建立，不在本节点添加伪 build/test 实现。
- 视觉/无障碍证据（适用时）：不适用；本节点无 UI 实现。
- 迁移与回退：无数据库 migration、协议或运行时迁移。回退时仅移除本节点新增 workspace/骨架文件；未触碰既有资料。
- 遗留风险：Node/Rust 版本固定、lint/typecheck、Tauri 最小壳、Schema 生成和分层命令属于 DEV-01-02～09，尚未声明完成；当前 workspace 仅证明空骨架可解析、构建和测试。
- 审核结论：**PASS**

