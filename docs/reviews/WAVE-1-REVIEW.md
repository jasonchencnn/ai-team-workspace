# Wave 1 集成审核记录

- 审核日期：2026-09-22
- 审核范围：DEV-01-01、DEV-01-02、DEV-01-03、DEV-01-04、DEV-01-06、DEV-01-07、DEV-02-01、DEV-03-01、DEV-06-01
- 最终结论：**PASS（ADR-0002 协议整改与独立复核已通过）**

> 2026-09-22 独立复核发现 DEV-03-01 的 Adapter Schema 未执行冻结的 UUIDv7、UTC 毫秒时间、output 载荷与统一错误 envelope 约束。Human 接受 ADR-0002 后已完成整改，全部反例、正向兼容用例与 Wave 1 门禁通过。可重放证据见 `WAVE-1-INDEPENDENT-PROTOCOL-AUDIT.md`。本结论只恢复 Wave 1，不表示 Wave 2 已启动。

## 1. 节点状态

| 节点      | 状态 | 主要交付 |
| --------- | ---- | -------- |
| DEV-01-01 | DONE | Cargo、pnpm 空 workspace 与目录通配成员配置 |
| DEV-01-02 | DONE | Node、pnpm、Rust 工具链版本固定与校验 |
| DEV-01-03 | DONE | TypeScript、Rust 格式化、lint、类型检查与负向违规守卫 |
| DEV-01-04 | DONE | unit、integration、contract、e2e 与递归 workspace 测试入口 |
| DEV-01-06 | DONE | Tauri 2、React 19 最小桌面壳与最小 capability |
| DEV-01-07 | DONE | Rust DTO → JSON Schema → TypeScript 单向生成与漂移检测 |
| DEV-02-01 | DONE | 强类型 UUIDv7 ID、UTC 毫秒时间、Version、ExpectedVersion 与 Actor |
| DEV-03-01 | DONE | Adapter V0 Schema 已执行冻结 UUIDv7、UTC 毫秒、output、错误与 SemVer 约束 |
| DEV-06-01 | DONE | Figma Token、字体、图标约定与基础主题包 |

各节点的专属审核依据、变更范围和定向验证证据分别记录在同目录的 `DEV-*-REVIEW.md`。独立反例审查发现的 DEV-03-01 G3 缺口已关闭，九个 Wave 1 节点均为 DONE。

## 2. 集成审核结果

### Workspace、工具链与质量入口

- Cargo 使用 `apps/*/src-tauri`、`crates/*`；pnpm 使用 `apps/*`、`packages/*`、`adapters/*`，后续添加符合布局的模块无需修改根成员清单。
- 固定 Node `24.21.0`、pnpm `10.33.0`、Rust `1.98.0`。
- 根命令覆盖格式、TypeScript/Rust lint、类型检查、生成物漂移、分层测试、递归模块测试与生产构建。
- `test:workspace` 递归发现子包测试，新增模块不需要手工改根测试清单。

### 依赖边界

- `domain` 仅含领域值对象需要的 `serde`，测试使用 `serde_json`，不依赖其他项目 crate。
- `contracts` 仅依赖 `schemars`、`serde`、`serde_json`，没有引入业务领域实现。
- `desktop` Rust 端仅依赖固定版本的 `tauri` 与 `tauri-build`。
- 其余 Wave 1 基础 crate 保持空边界，没有形成跨层耦合。

### 桌面安全边界

- capability 文件为 `apps/desktop/src-tauri/capabilities/main-window.json`，只绑定 `main` 窗口且 `permissions` 为空。
- 未引入 shell、filesystem 或 opener plugin。
- macOS 已验证桌面进程能够进入事件循环并可控关闭；本轮不将静态测试入口声明为真实桌面端到端测试。

### 契约与生成物

- Rust DTO 与 Schemars JSON Schema 是 TypeScript 契约生成的单一来源。
- `contracts:generate` 与 `contracts:check` 共用生成和格式化路径；生成物被修改后检查会非零退出。
- Adapter V0 使用 Ajv Draft 2020-12，固定 `ajv@8.17.1`、`ajv-formats@3.0.1`。
- 仅关闭与冻结 Schema 合法组合写法冲突的 `strictTypes`、`strictRequired` 风格检查；实例严格性、formats 与未知字段拒绝测试保持启用。

## 3. 最终验证证据

以下总控命令均退出 0：

| 命令 | 结果 |
| ---- | ---- |
| `pnpm run check:toolchain` | PASS |
| `pnpm run fmt:check` | PASS |
| `pnpm run typecheck` | PASS |
| `pnpm run lint` | PASS |
| `pnpm run verify:guards` | PASS |
| `pnpm run contracts:check` | PASS，无生成物漂移 |
| `pnpm run test` | PASS |
| `pnpm run build` | PASS |
| `cargo test --workspace --locked` | PASS |

此前定向复核亦通过：

- `cargo build --workspace --locked`
- `cargo clippy --workspace --all-targets --all-features --locked -- -D warnings`
- `cargo metadata --locked`
- 使用 npm 官方 registry 的 `pnpm install --frozen-lockfile`

测试统计：TypeScript/Vitest 35 个（unit 4、integration 1、contract 21、e2e 1、desktop 4、design-tokens 4）；Rust 13 个（domain 12、contracts generator 1）。其中 e2e 仅为测试入口占位，真实桌面 E2E 属后续节点。

## 4. 整改关闭项与后续边界

- Human 已接受 ADR-0002；DEV-03-01 的 UUIDv7、UTC 三位毫秒、`run.output`、字节预算、自定义错误与 SemVer 缺口均由机器契约和负向测试关闭。
- Wave 1 出口恢复 PASS；本次没有启动 Wave 2，也没有实现任何依赖 DEV-03-01 的后续 Adapter 节点。
- 本结论不等同于 Gate B、C、D、G 整阶段完成；CI、日志脱敏、真实 Adapter、完整 UI、真实桌面 E2E 与视觉/无障碍验收仍依赖后续节点。

## 5. 遗留风险

1. 当前目录不是 Git 仓库，无法用 `git status` / `git diff` 提供基于版本历史的变更证明；本次改用实际文件、manifest、锁文件、生成物检查和全量验证复核。
2. 本机默认 npm 镜像与本地 pnpm store 的键不一致，离线 frozen install 会缺少 tarball；显式使用 npm 官方 registry 的 frozen install 已通过，lockfile 未漂移。
3. pnpm 报告忽略 `esbuild@0.27.7` build script，但 Vite production build 已通过；后续升级依赖时需继续验证构建。
4. Tauri 当前使用临时中性图标，须在后续设计或发布节点替换。
5. macOS 证据覆盖应用进程启动与可控关闭，不覆盖完整窗口交互、多平台运行或真实桌面 E2E。

## 6. 最终结论

Wave 1 的工程、静态门禁、协议正反例与 R3 独立复核均可重放通过。ADR-0002 只补强既有冻结语义，没有新增业务能力或改变技术栈。

结论：**PASS**。DEV-03-01 为 DONE；本次到此停止，未启动 Wave 2。
