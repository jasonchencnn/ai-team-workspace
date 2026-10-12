# MM-STARTUP｜Minimaxcode 接续启动回传（2026-10-08）

按 `MINIMAXCODE-TASKS-2026-10-08.md` 的启动指令先回传实际模型、HEAD/输入SHA、任务ID、精确写集及先要复现的失败。本文件只是回传与登记，不改任何产品源码、迁移、正式节点状态或 ADR 接受状态。

## 1. 实际执行端

| 项 | 值 |
|---|---|
| Harness | MiniMax Code（本地 runtime，macOS darwin 27.0.0 arm64） |
| 实际模型/版本 | `MiniMax-M3.1-Flash-Preview`（执行端回填，不假定为 M3） |
| 仓库 | `/Users/chenzhiqing/Documents/MyVault/协作工作` |
| 共享工作树 | 与 Codex 同一未提交工作树，保留全部继承成果 |
| 新增费用 | $0；本轮全部命令 `--offline` / 本地已有依赖，无联网安装、下载、外部服务调用 |
| 总控/正式 DONE | 仍属 Codex；本端不自判 DONE、不解除任何 BLOCKED 依赖 |

会话内并发遵守项目规则上限 3；根 manifest/lockfile 未触碰（审计期即声明由 Codex 串行整合）。

## 2. HEAD 与输入固定

| 项 | 值 |
|---|---|
| HEAD | `1a14b8795e76fc373bef326eb5df96f9ac173f71` |
| 分支 | `codex/wave-2-ci-validation`（未提交工作树继承自上一轮） |
| 审计声明输入 SHA | `8845a053b778b67aeac7a2138ebeaa16432781572cdb2232fa085c30816de37f`（299 个范围文件） |
| 本端重固定的输入清单 | `MM-STARTUP-input-manifest.json`（同样 299 个路径，逐文件 sha256+bytes） |
| 本端口径 line-digest | `e973e019106432c594b8e86aceff3c8f65c680b1c5fae67d628aa40231a9ab23`（按 `path:sha256` 排序后换行拼接再 sha256） |

**方法差异须登记**：审计未在产物中留下 `8845a053…` 的可复算定义（既不是 `input-before.json` 自身字节摘要，也非 `path:sha256` line-digest），因此本端用显式可复算方法重新固定并保留逐文件哈希，不声称两者同口径可比。

**相对审计的输入差异：7 个文件，且全部是文档/元数据，零生产源码、零 migration、零 manifest/lockfile 变化。**

```
AGENTS.override.md
docs/README.md
docs/coordination/GOAL-REMEDIATION-2026-10-07.md
docs/coordination/README.md
docs/coordination/WORKLOG.md
docs/development/DEVELOPMENT_NODES.md
docs/v2/TASK_FREEZE.md
```

结论：DA-01～DA-06 的 RED 输入仍然有效——反例涉及的生产源码与迁移字节未变。

## 3. 迁移编号预检

`crates/persistence-sqlite/migrations/` 现存 0001～**0024**（最高 `0024_outbox_retry_default.sql`）。

- MM-R01 要求 `0025_message_routing_team_integrity.sql` —— 空缺，无冲突。
- MM-R02 要求 `0026_core_history_retention.sql` —— 空缺。
- MM-R03 要求 `0027_run_transition_integrity.sql` —— 空缺。

无需上报编号冲突。

## 4. 待办任务 ID 与精确写集

写集逐字取自各任务包“允许修改”段；每包另可新增自己的 `docs/coordination/minimax-return/MM-Rxx-RESULT.md` 与 `docs/reviews/evidence/minimax-return/MM-Rxx/`。未列文件一律只读。

| 包 | DEV | 允许修改（产品/脚本文件） |
|---|---|---|
| MM-R01 | DEV-02-08 | `migrations/0025_message_routing_team_integrity.sql`、`src/lib.rs`、`tests/message_routing_team_integrity.rs`、`tests/message_delivery_integrity_forward_migration.rs` |
| MM-R02 | DEV-02-05/06/08/10 | `migrations/0026_core_history_retention.sql`、`src/lib.rs`、`tests/core_history_retention.rs`、`tests/permission_approval_integrity_forward_migration.rs` |
| MM-R03 | DEV-02-05 | `migrations/0027_run_transition_integrity.sql`、`src/lib.rs`、`tests/run_transition_integrity.rs`、`tests/run_lease_consistency_forward_migration.rs` |
| MM-R04 | DEV-01-08 | `crates/observability/src/lib.rs`、`crates/observability/tests/structured_logging.rs` |
| MM-R05 | DEV-03-03 | `crates/adapter-supervisor/src/protocol.rs`、`src/lib.rs`、`adapters/mock/index.mjs` |
| MM-R06 | DEV-01-05 | `scripts/ci/local-gates.mjs`、`local-gates.test.mjs`、`verify-workflows.mjs`、`.github/workflows/ci.yml` |
| MM-R07 | DEV-02-08 | 仅 `MM-R07-route-override-proposal.md`（方案） |
| MM-R08 | DEV-02-09 | 仅 `MM-R08-notification-decision-matrix.md`（方案） |
| MM-R09 | DEV-01-05 | `MM-R09-acceptance-index.md`、`crates/domain/src/lib.rs`、`crates/domain/src/task_state_machine.rs`（仅注释订正） |
| MM-R10 | DEV-02-11 | 仅新增 `crates/domain/tests/deep_audit_coverage.rs` |

**写集互斥性已核对**：R01/R02/R03 共享 `persistence-sqlite/src/lib.rs`，必须 R01→R02→R03 串行；R04（observability）、R05（adapter-supervisor+mock）、R06（scripts+workflows）、R10（domain tests）四组文件互斥，可并行。R09 在 R01～R05 与 R10 之后做归档与重测。

## 5. 先要复现的失败（RED）

全部取自审计隔离 workspace 的既有回归，本端须先在正式仓库复现为真红再修。

| 审计 ID | 失败断言 | RED 位置 | 归属 |
|---|---|---|---|
| DA-01 | `draft_recipient_update_cannot_cross_team`：“DRAFT accepted a recipient from another Team” | `storage-boundaries-red-v2.log` | R01 |
| DA-02 | `retained_histories_cannot_be_physically_deleted`：`runs/reviews/artifacts/approvals/messages` 五表 DELETE 均 1→0 | 同上 | R02 |
| DA-03 | `existing_running_attempt_cannot_return_to_queued`：“an existing RUNNING attempt was reset to QUEUED” | 同上 | R03 |
| DA-04 | `lower_hex_digits_keep_secret_letter_case` / `mixed_hex_digits_keep_secret_letter_case`：`Audit%2dOnly%2fAaB`、`Audit%2DOnly%2fAaB` 未脱敏 | `observability-percent.log` | R04 |
| DA-05 | Adapter stdout 非 JSON 前缀（`running `/`test `/空白）被成功握手 | `adapter-main-replay.log`、`main-repro/adapter/` | R05 |
| DA-06 | 两份不同 `.env.production` 得到同一输入 hash | `env-manifest-replay.json`、`replay_env_manifest.mjs` | R06 |

合法对照必须同时成立：R01 的同 Team 草稿编辑/发送、R02 的取消/终结/版本更新、R03 的 `UNKNOWN→RUNNING` 对账、R04 的 plain/大写 hex 脱敏、R05 的合法 JSON Lines 握手。

## 6. 两项须提请 Codex 裁决的登记事项（不自行处置）

1. **`AGENTS.md` 已被 ChatGPT 项目镜像覆盖。** 仓库 HEAD 中的 `AGENTS.md` 自初始化提交起就是 ChatGPT 镜像样板（“This directory is a local mirror of the ChatGPT project…”），并非各任务包要求读取的项目执行规则；真实规则现存于 `AGENTS.override.md`。本端按 `AGENTS.override.md` 执行，不修改 `AGENTS.md`（不在任何写集内，且属同步产物）。
2. **ADR-0007 仍为 Proposed。** 本轮路由不等同接受通知新语义；R08 只交决策表，不实现未决条款。

## 7. 交付与停止

完成一包即产出该包 RESULT 与证据目录，交 Codex 独立复核；不自判 DONE、不自行解除 02-11/02-12/03-02/03-04/03-05 等依赖阻断、不启动 V2、不穿透冻结依赖。遇到规格冲突、需新状态/权限/错误码/协议语义、写集冲突、无法证明合法对照、S0/S1 未闭环或外部权限/费用影响，按任务包停止条件保留证据并回传，不改规格、不改预期、不返回固定 allow、不用假数据保绿。