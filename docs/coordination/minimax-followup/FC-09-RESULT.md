# FC-09｜执行回传（lint 历史材料范围收口：可执行 replay + 测试 + 精确配置提案）

- 关联任务：**FC-09**（`docs/coordination/MINIMAXCODE-FOLLOWUP-2026-10-08.md` §FC-09 与主控收口）
- 触发依据：`docs/reviews/MINIMAX-RETURN-AUDIT-2026-10-08.md` §2（`lint:ts` exit 1 / 1 error）与 §4 最后一条（历史文件只读与全仓 lint 范围冲突，由主控收口）
- 性质：**配置提案包**。新增可执行脚本与测试，**未修改任何配置文件**，**未改任何历史证据字节**。
- **判定：`SELF_CHECK_PASS`**（本包范围内全部完成；**G2 仍未通过**，因配置提案尚未由 Codex 整合——见 §7「明确未声称」）

---

## 1. 实际模型与环境

| 项 | 值 |
|---|---|
| Harness | MiniMax Code（本地 runtime） |
| 实际模型/版本 | `MiniMax-M3.1-Flash-Preview`（执行端回填，不假定为更高档位） |
| 会话 | Worker `mvs_6414edd33d0848289ef4592de7536cb8`（parent `mvs_71dc1bf32cd54569bc842fdb942a740d`） |
| 平台 | macOS darwin 27.0.0 arm64 |
| 仓库 | `/Users/chenzhiqing/Documents/MyVault/协作工作` |
| HEAD / 分支 | `1a14b8795e76fc373bef326eb5df96f9ac173f71` |
| 共享工作树 | **全部继承成果保留**；未执行 `reset` / `checkout` / `clean` / `stash` |
| Node | v24.21.0 |
| ESLint | 仓库既有 flat config，`@eslint/js` + `typescript-eslint` strictTypeChecked |
| 网络 | **无**。未联网、未下载、未新增依赖 |
| 新增费用 | **$0** |
| 提交/推送/部署 | **未** |
| cargo | **本包未运行 cargo** |

## 2. 输入与输出 SHA

### 2.1 只读输入（本包不得修改，逐字节核对未变）

| 文件 | SHA-256 |
|---|---|
| `eslint.config.mjs` | `c8feea3170463d04e6ccb347c553b5bfe7c07e0769246eb2b6e0d96edc5e0445` |
| `docs/reviews/evidence/deep-audit-2026-10-08/replay_env_manifest.mjs` | `4847fe4cdaf54507177cae9f8d40e1ec982e585b9e8972f3a40f50127f1b3e7f` |
| `scripts/ci/local-gates.mjs` | `3ab5149f8a94f1a7483957e30cc07cf486cdf086e17de87e781e161a9183ee79` |
| `package.json` | `273ee38d1cbb523e44490e1f0796ae2115c3e0d4924f8768e85140bf98f052b4` |

上述 4 项在实验前后各取哈希一次，**完全一致**。

### 2.2 本包新增产出

| 文件 | SHA-256 |
|---|---|
| `scripts/ci/replay-env-inputs.mjs` | `7bbccc32fc2975035151ab9d05e6bbfc85c8697d31e3df5e2f1a1ea34dd516f4` |
| `scripts/ci/replay-env-inputs.test.mjs` | `55bbeafcaea38f83bb481fe4950bc49a3111b8e5f6ed4d00eece301a3a9e2135` |

证据日志清单见 `docs/reviews/evidence/minimax-followup/FC-09/SHA256SUMS.txt`（含每个日志自身的 SHA-256）。

## 3. 逐文件修改

### 新增（写集内）

| 文件 | 说明 |
|---|---|
| `scripts/ci/replay-env-inputs.mjs` | 可执行 replay：在系统临时目录的隔离 Git 仓库中，正放 legacy blind、反放现行 Gate identity；安全清理结构 |
| `scripts/ci/replay-env-inputs.test.mjs` | `node --test` 可直接运行的 8 项测试 |
| `docs/reviews/evidence/minimax-followup/FC-09/` | 9 份证据日志 + `SHA256SUMS.txt` |
| `docs/coordination/minimax-followup/FC-09-RESULT.md` | 本文件 |

### 未修改（禁止清单，逐项核对）

`eslint.config.mjs`、`package.json`、`scripts/ci/local-gates.mjs`、`scripts/ci/verify-workflows.mjs`、`.github/workflows/ci.yml`、`docs/reviews/evidence/deep-audit-2026-10-08/` 下任何文件、任何 Rust crate、任何旧迁移、正式节点状态、ADR 接受状态。

> `scripts/ci/verify-workflows.mjs` 在 `git status` 中显示为 `M`，这是**本包开始前既已存在的继承改动**（本包开工时 `git status` 已列出），**非本包所改**。本包对该文件无任何写操作。

## 4. 新 replay 做了什么

`replay-env-inputs.mjs` 复用 R06 已实现的 `collectInputManifest` / `collectGateInputs`，**不复制、不重写**门禁逻辑。

### 4.1 正反两面

| 断言 | 对象 | 预期 | 依据 |
|---|---|---|---|
| RED | `collectInputManifest`（`git ls-files --others --exclude-standard`） | 空 / 第一份 / 第二份 `.env.production` **三份同 hash** → `blind: true` | gitignored 文件按定义不可见 |
| GREEN | `collectGateInputs`（绑定 ignored dotenv 输入） | 空 / 一 / 二**三者互不相同** → `distinguishes: true` | R06 已实现的 identity 绑定 |

实测（`replay-env-inputs-replay.json`）：

```
legacy_input_manifest: empty = first = second = 9a5bb45fc0dfae03...d9ce16dc2a262   (blind: true)
gate_input_identity:    c2cdcb6f... / 19a5abac... / 415d8617... 三者互异             (distinguishes: true)
```

### 4.2 判据是真判据（方法要求 2 的落实）

「若某断言其实因别的原因通过，必须重写去探测真正的判据」——已用 mutation 探测验证，未靠猜：

```
untracked(--exclude-standard) sees .env.production: false        ← 机制层确认 blind 来源
listIgnoredBuildInputPaths: [".env.production"]                  ← ignored 列表确实看得见
real    gate distinguishes B->C: true
mutated gate distinguishes B->C: false  ← 注入空 ignored 列表后判据立即翻转
```

即：把 `listIgnoredPaths` 打成空桩后，`distinguishes` 立刻变 false，证明断言探测的是 ignored 输入绑定这一真实判据，而非"两次 hash 恰好不同"。

### 4.3 安全清理（历史文件的原缺陷所在）

历史 `replay_env_manifest.mjs` 在 `finally` 里 `throw`（第 24 行），一旦清理阶段抛错就会**覆盖原始错误**。新脚本：

- 清理函数 `removeFixture` **永不抛错**，失败以返回值交给调用方；
- `withIsolatedFixture` 先捕获原始错误，再做清理，二者都失败时抛 `AggregateError([原始错误, 清理错误])`，**原始错误在首位**；
- 「无 commit」不变量改在 body 内断言，不再借 `finally` 抛错。

测试逐条覆盖：replay 失败 → 表面原始错误；清理失败 + replay 失败 → 原始错误仍在首位；清理失败 + replay 成功 → 清理错误照样上抛；真实 `rmSync` 失败不被吞。

### 4.4 无真实 env / 无真实 Git 历史

- fixture 建在 `mkdtempSync(join(tmpdir(), ...))`，跑完即删（有测试断言临时目录前后一致）；
- dotenv 内容是合成标记 `REPLAY_FIXTURE_MARKER=alpha|beta`，非任何真实配置值；测试断言 JSON 输出中不含 `REPLAY_FIXTURE_MARKER` / `alpha` / `beta`；
- fixture `git init` 后**不 commit**，脚本启动即断言无 `HEAD`。

## 5. 配置提案（供 Codex 串行整合；**本包未应用**）

### 5.1 提案正文：仅忽略那一个不可变历史复现件

在 `eslint.config.mjs` 的 `ignores` 数组中，**新增且仅新增**一行：

```diff
--- a/eslint.config.mjs
+++ b/eslint.config.mjs
@@ -9,6 +9,11 @@ export default tseslint.config(
       "node_modules/**",
       "target/**",
       "coverage/**",
       "**/*.d.mts",
       "apps/desktop/dist/**",
       "apps/desktop/src-tauri/gen/**",
+      // FC-09: immutable archived reproduction from the 2026-10-08 audit.
+      // Its `throw` inside `finally` is the historical defect under study and
+      // its bytes must not change. Only this one file is archived material;
+      // every other path, including the rest of this evidence directory, stays
+      // strictly linted.
+      "docs/reviews/evidence/deep-audit-2026-10-08/replay_env_manifest.mjs",
     ],
   },
```

### 5.2 为什么是这个形态

| 约束 | 如何满足 |
|---|---|
| 不得 blanket ignore evidence | 提案是**单个文件精确路径**，非目录、非 glob |
| 不得关掉 `no-unsafe-finally` | 提案**完全不碰 rules**，规则强度不变 |
| 不得删旧文件 / 改历史字节 | 只加 ignore；历史文件哈希不变（§2.1 已证） |
| 不得自行改 `eslint.config.mjs` | 本包**未应用**，diff 仅供 Codex 审阅 |
| active 源码仍严格 | 同一 evidence 目录下的其它文件与 `scripts/ci/` 全部照常受检 |

采用 `ignores` 而非 `rules: {"no-unsafe-finally": "off"}` + `files:` 覆盖块：ignore 是**文件级**表达"归档材料不是 active 源码"，语义与裁决口径一致；rule 覆盖块会让规则本身在该文件上被关掉，弱化了"规则未被削弱"这一可审计事实。

### 5.3 整合后如何证明 active 反例仍能让 lint 失败

这是提案**最容易被滥用的地方**：一条 ignore 一旦写宽，未来 active 脚本里的真实 `no-unsafe-finally` 会被静默吞掉。已实测的三条证据（均已落盘）：

| 证据 | 命令 | 观测结果 | 日志 |
|---|---|---|---|
| A：整合**前**，active 反例必须失败 | `npx eslint scripts/ci/__fc09_active_guard_probe.mjs` | exit 1，`7:5 no-unsafe-finally` | `active-counterexample-current-config.log` |
| B：提案配置下，active 反例**仍然**失败 | `npx eslint -c <提案配置> .` | exit 1，仅剩 `__fc09_active_guard_probe.mjs` 一条 | `proposal-full-repo-lint.log` |
| D：**同目录邻居**新反例仍失败 | 同上 + 在 evidence 目录内放新反例 | exit 1，**2 errors**：evidence 内 + `scripts/ci/` 内各一条 | `proposal-neighbour-guard.log` |

证据 D 是"提案没有扩大成目录级 ignore"的决定性证明：**若提案写成目录 ignore，B/D 都会漏报 evidence 目录内的反例。**

### 5.4 Codex 整合后的验证步骤与预期

```bash
# 1) 应用 §5.1 diff 后，全仓 lint 必须为 0
pnpm run lint:ts
#   预期：exit 0，输出无 error 无 warning
#   已实测等价结果：提案配置下全仓 lint exit 0（proposal-full-repo-lint-clean.log）

# 2) 新 replay 与测试自身必须干净
node --test scripts/ci/replay-env-inputs.test.mjs     # 预期：8 pass / 0 fail，exit 0
npx eslint scripts/ci/replay-env-inputs.mjs scripts/ci/replay-env-inputs.test.mjs
                                                        # 预期：exit 0
npx prettier --check scripts/ci/replay-env-inputs.mjs scripts/ci/replay-env-inputs.test.mjs
                                                        # 预期：All matched files use Prettier code style

# 3) 反例守卫：整合后 active 反例必须仍能被拒绝（一次性探针，验完即删）
printf 'export function p(){try{return 1;}finally{throw new Error("x");}}\n' > /tmp/g.mjs
cp /tmp/g.mjs scripts/ci/__guard.mjs
npx eslint scripts/ci/__guard.mjs ; echo "必须 exit 1 且报 no-unsafe-finally"
rm scripts/ci/__guard.mjs
#   预期：exit 1。若为 0，说明 ignore 写宽了，必须撤回整合。
```

**只有第 1 步 exit 0、第 3 步 exit 1 同时成立，G2 才能判定通过。** 第 3 步是提案的必要组成部分，不是可选加固。

## 6. 正反测试：命令 / exit / 日志路径

| # | 命令 | exit | 日志 |
|---|---|---|---|
| 1 | `node --test scripts/ci/replay-env-inputs.test.mjs` | **0**（8 pass / 0 fail） | `FC-09/replay-env-inputs-test.log` |
| 2 | `node scripts/ci/replay-env-inputs.mjs` | **0**（`verified: true`） | `FC-09/replay-env-inputs-replay.json` |
| 3 | `npx eslint scripts/ci/replay-env-inputs.mjs scripts/ci/replay-env-inputs.test.mjs` | **0** | `FC-09/eslint-new-files.log` |
| 4 | `npx prettier --check scripts/ci/replay-env-inputs.mjs scripts/ci/replay-env-inputs.test.mjs` | **0** | `FC-09/prettier-new-files.log` |
| 5 | `pnpm run lint:ts`（现状基线，**预期失败**） | **1**（1 error / 0 warnings） | 见下 |

测试 8 项全部通过，逐项：

```
✔ replays the legacy blind spot and the current gate identity
✔ the ignored dotenv payload never appears in the replay output
✔ the fixture is removed from the system temp directory
✔ a failing replay surfaces its own error
✔ a failing cleanup cannot mask the original replay error
✔ a failing cleanup is still reported when the replay succeeded
✔ a real cleanup failure still removes the fixture and is not swallowed
✔ the immutable historical fixture still throws from finally
```

第 5 项基线如实登记（复现确认该错误**仍是唯一错误**，与审计 §2 登记一致）：

```
docs/reviews/evidence/deep-audit-2026-10-08/replay_env_manifest.mjs
  24:22  error  Unsafe usage of ThrowStatement  no-unsafe-finally
✖ 1 problem (1 error, 0 warnings)
```

**该错误正是本包提案要解决的目标，本包未修复也不得修复**——修法由 Codex 整合配置。

### 提案预验证（用临时 `-c` 配置，**未写入仓库**）

为避免提交一份"看起来对"的提案，本包把提案 diff 落成一份**临时** `eslint.config.__fc09proposal.mjs`，用 `eslint -c` 在其上跑完整仓库，验证后即删。该临时文件与 3 个探针文件**均已删除并核对无残留**，`git status` 中无 `probe` / `__fc09proposal` 字样。

- 提案配置 + active 探针在场 → exit 1，只剩探针那条（历史错误已被精准消除）
- 提案配置 + 无探针 → **exit 0**（全仓 lint 为 0）
- 提案配置 + 同目录邻居探针 → exit 1，**2 errors**（证明未扩大成目录 ignore）

## 7. 明确未声称

- **未整合配置**：`eslint.config.mjs` 未被修改（哈希前后一致）。提案是提案，**不是已生效事实**。
- **未声称全仓 lint 已 0**：现状 `pnpm run lint:ts` 仍 **exit 1 / 1 error**。§5.4 的"预期 0"是整合后的预期，**不是当前状态**。
- **G2 未通过**：`docs/development/QUALITY_GATES.md` G2 要求 fmt/lint/typecheck/clippy **零错误零告警**。lint 仍有 1 条真实错误，**G2 不通过**，需 Codex 整合 §5.1 并复跑 §5.4 全部三步后才可复判。
- 未运行 cargo / clippy / typecheck / build / 全套门禁（本包不涉及其范围）。
- 未跑 Mac canonical 21 项、未做联网 advisory 校验。
- 未提交、未推送、未部署、未新增费用、未新增依赖。

## 8. 剩余风险与回退

| 风险 | 说明 | 缓解 |
|---|---|---|
| **ignore 被写宽** | 有人把提案改成目录级或 glob，future active 反例被吞 | §5.4 第 3 步反例守卫是硬性回归检查；证据 D 已证明当前形态不漏报 |
| **整合后未复跑 §5.4** | 只跑 `lint:ts` 得 0 就判 G2 通过，会漏掉"规则被掩盖" | §5.4 明示第 3 步为**必要**组成部分，非可选 |
| **新 replay 与历史 fixture 结论漂移** | 两者都基于 `local-gates.mjs`；该文件若被后续包改动，结论需重放 | 两个 SHA 已登记（§2）；门禁逻辑改动后必须重跑本包 8 项测试 |
| **`mkdtemp` 残留** | 极端情况下临时仓库未被清理 | 清理失败不再被吞（会抛）；测试断言临时目录前后一致 |
| 并行包干扰 | 共享 dirty worktree，其他包可能同时改 `local-gates.mjs` | 本包只读该文件；发现漂移须重跑，不得沿用本 RESULT 的结论 |

**回退**：本包全部产出为**新增文件**，删除 `scripts/ci/replay-env-inputs.mjs`、`scripts/ci/replay-env-inputs.test.mjs`、`docs/reviews/evidence/minimax-followup/FC-09/`、`docs/coordination/minimax-followup/FC-09-RESULT.md` 即完全回到执行前状态，**无需回滚任何他人改动**。配置提案未应用，回退成本为零。

## 9. 停止条件核对

| 条件 | 实际情况 |
|---|---|
| 需要改 `eslint.config.mjs` | **否**——仅提交提案，未应用，哈希未变 |
| 需要联网 | **否**——全程离线 |
| 需要更大写集 | **否**——全部产出落在授权的 4 项内 |

**未自行改配置、未 blanket ignore evidence、未关规则、未删改历史文件、未扩大提案范围、未提交推送、未自判 DONE。**