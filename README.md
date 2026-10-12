# AI Team Workspace

一个 macOS 桌面应用，让 Claude Code、Codex 等 AI 编程工具以固定岗位组成开发团队。你主要和 Leader 对话，Teamspace 负责建任务、派工、在隔离的 git worktree 里执行、回收结果和留痕。

计划与当前进度见 [docs/PLAN.md](docs/PLAN.md)。

## 本地运行

需要 macOS 14+、Node 24.21.0、pnpm 10.33.0；Rust 版本由 `rust-toolchain.toml` 自动选择。

```bash
pnpm install
pnpm run dev
```

提交前运行全部检查（格式、lint、类型检查、测试）：

```bash
pnpm run check
```

## 目录

| 路径 | 内容 |
|---|---|
| `apps/desktop` | Tauri 桌面应用：React 界面与 Rust 宿主 |
| `crates/` | Rust 内核 `teamspace-core`（M1 建立） |
| `packages/design-tokens` | 来自 Figma 的颜色、字号、间距等设计 Token |
| `archive/` | 重置前的旧代码、旧工具和冻结规格，不参与构建，见 `archive/README.md` |
| `design/` | 界面设计说明与 Figma 节点索引 |
| `docs/PLAN.md` | 计划与进度（唯一权威来源） |
| `docs/DECISIONS.md` | 决策记录 |
| `docs/product/` | PRD 与 V2 愿景（需求来源） |
