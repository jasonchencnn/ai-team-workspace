# 决策记录

每条决策写清楚「定了什么」和「为什么」。推翻旧决策时追加新条目，不改写旧条目。

## 2026-10-12 项目重置

| # | 决策 | 原因 |
|---|---|---|
| D1 | Claude 担任总指挥，负责规划、实现、审查和整合；取代原来「Codex 主控 + Luna/Terra 子任务 + 外部模型任务包」的路由规则 | Human 指定 |
| D2 | 流程重置：以可用产品为先，取消冻结规格、101 节点账本、逐节点审计和证据目录；进度只记录在 `docs/PLAN.md` | Codex 时期 3 周没有产出任何可用界面，流程开销远大于产出 |
| D3 | 保留技术栈：Tauri 2 + React 19 + TypeScript + Rust + SQLite；macOS 优先 | 选型合理，工具链已就绪 |
| D4 | 新内核 `teamspace-core` 重写；旧 crate 只作参考，保存在存档提交中 | 旧实现与冻结规格强耦合，257 个数据库触发器让每次改动都很重 |
| D5 | Harness 通过直接驱动 CLI 接入（Claude Code、Codex），自定义 JSON-RPC 适配协议搁置 | 最短路径；旧适配层从未接上真实 CLI |
| D6 | Harness 实例 = CLI 类型 + 配置档案；平台不保存密钥，登录由 CLI 自己管理 | MiniMax 等模型通过 Claude Code / Codex 的配置接入；避免平台成为凭据持有者 |
| D7 | 每个 Run 使用独立 git worktree 和分支；验收通过后由 Human 确认合并 | 并行执行互不覆盖，合并权留给 Human |
| D8 | Leader 通过 Teamspace MCP 编排团队（M4）；换绑、权限调整、合并到主分支只能由 Human 执行 | V2 核心体验：Human 主要与 Leader 对话，Leader 不能给自己扩权 |
| D9 | 保留密钥扫描（`.gitleaks.toml`）、依赖审计（`.github/cargo-deny.toml`）和依赖安全更新（`.github/dependabot.yml`）配置 | 安全底线，日常零成本 |

## 沿用的原始产品原则

以下原则来自 `docs/product/PRD-V1.md` 和 `docs/product/V2-愿景与重启基线.md`，继续有效：

- 岗位属于 Team，任务属于岗位，知识属于 Workspace，执行属于 Run，智能属于 Harness，控制权属于 Human。
- 岗位 ≠ Harness ≠ 模型 ≠ 会话；更换 Harness 只改变绑定，不改变岗位、任务和历史。
- 团队通信经 Teamspace 中转并持久化，不以 Harness 之间的点对点通信作为正式渠道。
- local-first 单用户；时间统一使用 UTC 存储，界面按本地时区显示；核心对象只归档、不硬删除。
