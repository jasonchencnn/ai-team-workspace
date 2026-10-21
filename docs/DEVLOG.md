# 开发日志

按时间倒序追加，每条写：做了什么、谁做的、结果、下一步。细节以提交记录和 `docs/PLAN.md` 为准。

## 2026-10-21

- 迁移检查：仓库迁到 `/Volumes/jasonchen/MyVault/协作工作`（APFS 外置盘）。`git fsck` 无误；三个 worktree（`../协作工作-wt/m5-04`、`m5-05`、`m5-kfix`）链接正常；工作区干净。主线 `pnpm run check` 通过（Rust 984 通过 2 忽略、前端 140 通过）。Rust `target/` 未随迁移，首次冷编译。
- 环境注意：仓库换盘后 pnpm 默认用 `/Volumes/jasonchen/.pnpm-store`（空），离线安装需加 `--store-dir /Users/chenzhiqing/Library/pnpm/store/v10`（非 TTY 时加 `CI=true`）。本机 Node 为 22.23，`engines` 要求 24.21，仅警告。
- 执行者调整（D34）：OpenCode 的 M3.1 不再免费，主力改为 MiniMax Code；OpenCode 只用 `*-free` 模型辅助。
- 派工：MiniMax Code 续做 `team/m5-kfix`、`team/m5-04`；`team/m5-05` 已完成（mcp 测试全过），交 OpenCode deepseek-v4.1-flash-free 交叉审查。
