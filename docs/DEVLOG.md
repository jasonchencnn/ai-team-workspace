# 开发日志

按时间倒序追加，每条写：做了什么、谁做的、结果、下一步。细节以提交记录和 `docs/PLAN.md` 为准。

## 2026-10-21

- M6-03 合入 `30606fa`（MiniMax Code：更换 Harness 弹窗、待生效卡片与撤销、待投递消息、启动包预览）。
- M7-04 合入 `620bf42`（MiniMax Code：未签名 `.dmg` 6.1 MB、版本 0.2.0；`export_backup` / `inspect_backup` / `restore_backup`，恢复前自动备份、失败回滚原库，Leader 被拒；无新依赖）。决策号与 D35 撞号，改为 D36、D37。
- M6 回归（agy）：报 10 条，总指挥核实后按 D35 裁定（人工接管与现有流程一致、进入待验收即生效、新岗位不附交接、手动绑定清待生效、取消并重跑全部重跑且失败不回滚）。
- M6-04 合入：第一部分补 `list_active_runs`、`list_role_messages`、`previousHarnessName`（迁移 V17），去掉前端状态白名单与 N+1；第二部分修复 D35 各项及 3 个严重竞态 / 恢复 / 并发缺陷。全量 e2e 319 通过。M6 完成。
- 暂缓（轻微）：重复发起换绑直接替换旧待生效、换绑历史无终态记录、启动包预览同步全量生成。
- M6-02 合入 `eaf384b`（MiniMax Code：新 Harness 首次接手时说明包末尾附「岗位交接」，上限 12000 字符按 文件→日志→摘要→任务 截断；只读预览 API，不在 Leader 白名单）。
- M6-01 合入 `b3b2fac`（MiniMax Code：`rebind_role` 三种方式、待生效在执行结束后自动生效、撤销、消息绑定快照 `binding_harness_id`、Leader 被拒；迁移 V15）。接受的风险：应用退出时执行被标中断也算结束，待生效换绑随之生效。主线检查通过（Rust 1048、前端 168）。
- M5-06 合入 `b34340b`：5 个缺陷全部修复并各有复现测试；迁移与 M6-01 撞号，由执行者合并主线后改为 V16。M5 完成。
- 已派 MiniMax Code：`team/m6-03`（换绑页面）、`team/m7-04`（未签名 .dmg 打包、备份导出与恢复）。
- 主线 `9ee88f1` 全量通过：Rust 1011、前端 168、e2e 308 通过 5 跳过。派 MiniMax Code 做 `team/m6-01`（换绑三种方式与待生效）、`team/m6-02`（岗位启动包）。
- M5-06 回归（agy 只读审查，从 `/Volumes/jasonchen/MyVault` 启动以便读到 Figma 截图；agy 无 shell 权限，e2e 由总指挥跑）：报 5 个缺陷，总指挥逐条核实属实——严重 1（过期消息重发后立即再过期）、一般 4（总开关关时岗位开关被冲掉、通知中心跨团队、已决策消息再失败不回待我处理、投递引擎权限选项只认 optionId 前缀）。派 MiniMax Code 在 `team/m5-06` 修复。
- 暂缓（M7-02 视觉批）：待我处理页的高风险审批未就地批准 / 拒绝（现为跳转处理）。
- M5 合入：`team/m5-kfix`（MiniMax Code，投递唤醒丢失、发信岗位、线程摘要去 N+1、M4-G-1 socket 握手探测；qa_m4 连跑 10 次通过）→ `ec1a03c`；`team/m5-05`（MCP 消息工具，deepseek-free 交叉审查无越权、无严重问题，2 条一般问题由 minimax-m3.1-flash-free 修复：wait_reply 计时含解析、轮询退避至 2 秒、生效超时回显与测试）→ `43ea58a`；`team/m5-04`（MiniMax Code，待我处理页、决策弹窗、高风险审批样式、通知中心与订阅；与 M5-03 的侧边栏 / 路由 / 导航断言冲突由总指挥按两边合并）→ `9ee88f1`。
- 审查中的轻微问题按 D19 暂缓：wait_reply 不响应取消、thread 参数无归属校验、send_message 不能指定线程、工具清单只做子集断言、inbox 只测了 decision 一类。
- 环境：worktree 里旧的 `target/` 构建产物指向旧路径，会让 Tauri 构建失败，清掉过期产物后正常。
- 已建 `team/m6-01`、`team/m6-02` worktree，等主线全量检查与 e2e 通过后派给 MiniMax Code。
- 迁移检查：仓库迁到 `/Volumes/jasonchen/MyVault/协作工作`（APFS 外置盘）。`git fsck` 无误；三个 worktree（`../协作工作-wt/m5-04`、`m5-05`、`m5-kfix`）链接正常；工作区干净。主线 `pnpm run check` 通过（Rust 984 通过 2 忽略、前端 140 通过）。Rust `target/` 未随迁移，首次冷编译。
- 环境注意：仓库换盘后 pnpm 默认用 `/Volumes/jasonchen/.pnpm-store`（空），离线安装需加 `--store-dir /Users/chenzhiqing/Library/pnpm/store/v10`（非 TTY 时加 `CI=true`）。本机 Node 为 22.23，`engines` 要求 24.21，仅警告。
- 执行者调整（D34）：OpenCode 的 M3.1 不再免费，主力改为 MiniMax Code；OpenCode 只用 `*-free` 模型辅助。
- 派工：MiniMax Code 续做 `team/m5-kfix`、`team/m5-04`；`team/m5-05` 已完成（mcp 测试全过），交 OpenCode deepseek-v4.1-flash-free 交叉审查。
