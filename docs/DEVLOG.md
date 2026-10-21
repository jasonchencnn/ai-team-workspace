# 开发日志

按时间倒序追加，每条写：做了什么、谁做的、结果、下一步。细节以提交记录和 `docs/PLAN.md` 为准。

## 2026-10-22

- M7-02 完成（分支 `team/m7-02`，MiniMax Code）。L-12 视觉细节三项、6 张画板视觉走查、待我处理页「待答复权限」可就地允许 / 拒绝、看板分页两处缺陷、UI-ROLE-002 死分支清理。`pnpm run check` 通过（Rust 1140 通过 1 忽略、前端 220 通过），全量 e2e 335 通过、4 跳过。
- 顺带修：M7-04b 加的「有执行在跑或有消息在投递时拒绝恢复备份」前置条件是 2026-10-21 22:52 之后才有的，之后没跑过全量 e2e —— 全量 e2e 里「Agent 还没给回复」的消息会一直停在投递中，备份恢复用例被挡住、一直红。测试桥接加测试专用命令 `__clear_pending_messages`，恢复用例先清遗留。
- 顺带修：`apps/desktop/tests/messageBubble.test.ts` 把消息时间写死成 2026-10-21 的 UTC，过了当天零点断言就变成「昨天」而挂掉 —— 改成按本地时区构造时间并固定「现在」。

## 2026-10-22

- M7-02 合入 `6637038`（MiniMax Code：L-12 视觉细节、6 张画板走查、待我处理就地允许 / 拒绝权限、看板分页两缺陷、UI-ROLE-002 确认不可达并删除、清掉最后一个 `test.fixme`；顺带修好写死日期的单测与消息遗留挡住备份恢复的 e2e）。
- M7-05 最终回归（主线 `6637038`）：`pnpm run check` 通过（Rust 1140 通过 1 忽略、前端 225）；`pnpm run e2e` 335 通过、4 跳过（真实额度开关）；`pnpm run package` 产出 `target/release/bundle/dmg/AI Team Workspace_0.2.0_aarch64.dmg`（6.5 MB，未签名）；MiniMax Code 真实冒烟 RUN-SMK-001/002/M3-001 全部通过。Claude Code 真实冒烟消耗 Claude 额度，未跑。
- 全程执行者：MiniMax Code 完成全部开发与修复；OpenCode `deepseek-v4.1-flash-free` 交叉审查、`minimax-m3.1-flash-free` 小修复；agy（Gemini）三轮只读回归共报 24 条，核实后修复严重 6、一般 13，其余轻微暂缓。

### 交 Human 最终校验的建议清单

1. 安装：打开 `.dmg`，拖入应用程序；首次打开右键「打开」（未签名）。
2. 团队与任务：用一个真实仓库建团队、配岗位与 Harness（MiniMax Code），建任务派给 AI，看执行、答复权限、验收看 diff、通过并合并。
3. Leader：在工作空间目录打开 Claude Code 接入 MCP（岗位页一键生成配置），让它拆任务、派工、发消息、查待我处理。
4. 岗位通信与待我处理：给岗位发消息并收到回复；失败 / 受阻 / 待验收 / 待答复权限出现在待我处理并能就地处理；记录决策；通知中心与通知订阅。
5. 换绑：执行进行中更换 Harness，分别试「完成后更换」「取消并重跑」「人工接管」，看待生效与撤销；新 Harness 首次执行的说明包带「岗位交接」。
6. 备份：设置里导出备份、查看摘要、恢复（恢复前会自动再备份一份，需重启应用）。
7. 窄屏与键盘：把窗口拉窄到手机宽度看侧栏抽屉与看板滑动；用 Tab / Enter / Esc 走一遍创建任务与弹窗。

### 暂缓的轻微项（D19，使用中迭代）

- MCP：`wait_reply` 不响应取消、thread 参数无归属校验、`send_message` 不能指定线程。
- 换绑：重复发起直接替换旧待生效、换绑历史无终态记录、启动包预览同步全量生成。
- 分页：翻页间被更新的记录需刷新才出现；列表视图并发加载的 loading 状态；导出备份时快照与文件夹清单非原子。
- 无障碍：未做 VoiceOver 实测；品牌色 / 状态色标签未逐个做 AA 断言。

## 2026-10-21

- M7-03 合入 `d941a98`（MiniMax Code：看板按列游标分页、活动每页 100、6 个索引（迁移 V18）、1 万条基准，debug 下看板 94→24ms；迁移与决策撞号由执行者改为 V18、D38/D39）。
- M7 回归（agy，备份与分页）：严重 2（恢复清单路径穿越可写到工作空间外；覆盖中途失败后连接停在空库）、一般 5。内核 4 项派 MiniMax Code 在 `team/m7-04b` 修复并合入 `1ec6e4f`；看板分页前端 2 项（合并列加载更多按钮丢失、刷新后重复卡片）并入 M7-02；暂缓：翻页间被更新的记录需刷新才出现、列表视图并发加载的 loading 状态、导出时快照与文件夹清单非原子。
- M7-01 合入 `6834086`（MiniMax Code：<768px 抽屉侧栏与看板按列滑动、弹窗焦点统一在 Modal、禁用态改实色 Token 达 AA；全量 e2e 332 通过）。未做：真实读屏器实测。
- 派 MiniMax Code 做 M7-02（L-12 视觉细节、待我处理就地答复权限、fixme / ignore 收尾、看板分页前端两缺陷）。
- M7-03 合入 `9e4aadc`（MiniMax Code：看板分页与「加载更多」、活动页每页 100 条、迁移 V18 查询索引、待我处理聚合批量查、1 万条任务的内核基准与 e2e）。决策号与主线 D37 撞号，改为 D38、D39；索引迁移与主线 V17 撞号，改为 V18。
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
