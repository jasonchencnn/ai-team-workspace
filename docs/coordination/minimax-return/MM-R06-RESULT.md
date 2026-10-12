# MM-R06 RESULT｜补齐 Gate 输入与配套证据机制

- 节点：DEV-01-05（关联 01-03）；审计缺陷 DA-06（P1 证据风险）+ 审计 §4 两条机制缺口
- 执行模型/环境：MiniMax-M3.1-Flash-Preview（`ANTHROPIC_MODEL` 当前别名），macOS 27.0.0 arm64，Node v24.21.0，pnpm 10.33.0，gitleaks 8.30.0。仓库 `/Users/chenzhiqing/Documents/MyVault/协作工作`，分支 `codex/wave-2-ci-validation`，HEAD `1a14b8795e76fc373bef326eb5df96f9ac173f71` 加继承未提交工作树（改动前后均 120 项）。未 reset/checkout/clean/stash，未提交、推送、部署，未联网下载或新增依赖，新增费用 $0。
- 结论：**SELF_CHECK_PASS**（自验收，不等于节点 DONE；正式判定由 Codex 独立复核）。

## 1. 输入与输出 SHA

审计输入（`docs/reviews/evidence/deep-audit-2026-10-08/input-before.json`）与本包输出逐文件对照见 `docs/reviews/evidence/minimax-return/MM-R06/input-output-sha256.tsv`。

| 文件 | 审计输入 SHA-256 | 本包输出 SHA-256 |
|---|---|---|
| `scripts/ci/local-gates.mjs` | `84bd09eb364493437ee36574a521c238e6d0aab49d05f2fe6dfa8a540556a15c` | `4cfdd3da3e06302cf58d2c0efe6eb3aab05f001104f625553b4f97cff6bbacc2` |
| `scripts/ci/local-gates.test.mjs` | `17f01e27bf87af1b64630edda3b39bb2344128b8c4b6dee6e7ff419510ddd5fc` | `35f04f45740404a9ea18475e47f505293ea683e9e622eab10315f2de50a28ab3` |
| `scripts/ci/verify-workflows.mjs` | `6f94f05fc456228cde31ae6993e2969c3e695f11260973a0b81ab816f147894d` | `29eacc66a7c90339e465686c1c8050a5de05216a623fb0389f9e550236eb3be6` |
| `.github/workflows/ci.yml` | `e118bee145c1fad4957421c04aa1155a8eb4701150c70e527b61925fe07b5737` | `88a3eb5cf951f7064b5e593bd9e2559425c096fc6884e5d9ca4efaa4dbb7d1bf` |

说明：`local-gates.mjs`、`local-gates.test.mjs`、`ubuntu-gate.Dockerfile` 在 HEAD 中未被跟踪（继承成果），审计记录的 SHA 来自其审计期工作树快照；上表用同一记录作为输入基线。`verify-workflows.mjs` 与 `ci.yml` 为已跟踪文件，可用 `git diff HEAD` 复核差异。

本机实测（companion 探针，写证据前后一致）：输入清单总哈希 `3dd4bd5b…`，ignored build input 总哈希 `fa6f4b3e…`，Gate 输入同一性 `cffffa2e…`（`companion-checks-probe.json`）。

## 2. 逐文件修改说明

### 2.1 `scripts/ci/local-gates.mjs`

**(a) ignored 构建输入被纳入 Gate 输入（DA-06 修复）**

- `listIgnoredBuildInputPaths()` 用 `git ls-files -z --others --ignored --exclude-standard` 补齐原命令按定义排除的 ignored 路径，再按规则筛选：basename 匹配 `/^\.env/`（Vite 的 dotenv 家族）才算构建输入；`.env.example`/`.env.sample`/`.env.template` 是示例模板不是构建输入；`.git`/`node_modules`/`target`/`dist`/`.pnpm-store` 段属于依赖缓存与产物，排除。
- **未使用 `--directory`**：实测该选项会把 untracked 目录折叠成目录条目，从而漏掉审计 fixture 里 `docs/.../env-manifest-fixture/.env.production` 这种「未跟踪目录内的 ignored dotenv」。这是我实测发现的第二层盲区，已改用完整列表 + 应用层过滤，代价是本机 91339 条 ignored 路径（约 10 MiB）。
- `captureGitFileList()` 的 `maxBuffer` 提到 64 MiB：Node `spawnSync` 默认 1 MiB，本机真实 ignored 列表约 10 MiB，不改会 ENOBUFS 失败（实测 `status null / ENOBUFS`）。失败按 fail-closed 抛出，不返回截断列表。
- `collectIgnoredBuildInputs()` 沿用与常规输入完全一致的路径规则：规范化相对路径、不得逃出仓库、拒绝符号链接、拒绝非普通文件、读失败报错。
- `buildIgnoredBuildInputManifest()` 输出格式 `ai-team-ignored-build-inputs/v1`，每条 entry 只有 `mode`、`path`、`sha256`、`size_bytes`，按 UTF-8 字节序排序、拒绝重复路径与非法 mode。
- `collectGateInputs()` 把 `input_manifest` 与 `ignored_build_inputs` 合成 `ai-team-gate-input-identity/v1` 的同一性哈希；`assertMatchingGateInputs()` 用于运行前后比对。

**ignored 输入如何不泄露 env 值**：只读字节做 SHA-256 与 `stat` 取 mode/长度，从不把内容解码、格式化、打印或写入证据；evidence 里只有路径、mode、字节数、摘要。测试 `ignored build inputs record only path, mode, size, and digest; never a value` 直接断言 `JSON.stringify(manifest)` 不含哨兵键名 `VITE_ONE` 与其值片段。`red-da06-reproduction.log` 中 `values_present_in_evidence: false` 是同一断言的机器可读复现。摘要本身仍是内容派生信息，属于「与构建相关的可追溯摘要」范畴；若后续 Human 认为连摘要都过于接近敏感材料，可改为只记录存在性并拒绝一切 ignored dotenv，代价是内容变化不再可分辨——这个取舍留给 Codex 判定。

**(b) 全 refs 历史秘密扫描成为必须通过的配套证据**

- `companionCheckDefinitions()` 定义两项 `kind` 分别为 `history-secret-scan` 与 `command` 的配套检查：`gitleaks git --log-opts=--all --config .gitleaks.toml --redact --exit-code 1 --no-banner .` 和 `pnpm run verify:guards`。两者在 `plan.companion_checks` 中单列，不进 `plan.commands`。
- `collectGitHistoryScope()` 记录 `git rev-parse --is-shallow-repository` 与 `git for-each-ref` 全量 ref，`assertCompleteHistoryScope()` 拒绝 shallow 仓库、拒绝空 ref 列表、拒绝缺失的 64 位 refs 摘要。这正是审计 §4「全 refs 扫描没有随 fixed 21 自动落进 Gate 证据」的执行化：范围、配置、退出码、日志摘要四要素齐备。
- `normalizeCompanionLog()` / `companionLogDigest()`：工具输出含墙钟耗时、字节数与 ANSI 颜色，两次同样的成功扫描字节并不相同，直接比原始字节会把耗时变化误判为漂移。归一化只剥离 ANSI、耗时/尺寸测量值、时间戳与空白；判定性文本（commit 数、`no leaks found`/`N leaks found`）保持不变，因此结论变化仍会改摘要。

**(c) 配套证据缺失、失败、hash 漂移都阻止 PASS**

`assertMatchingCompanionEvidence()` 对每个声明的 id 要求 before/after 记录都存在、两侧 `exit_code` 都为 0、日志摘要一致、`history_scope.refs_sha256` 一致，并拒绝任何未声明的 id。
`assertCompletedGateRun()` 把整体 PASS 的三个出口条件集中到一个可单测的函数：canonical 执行条数必须等于 `plan.canonical_command_count`、初始 Gate 输入同一性必须存在且与运行后一致、配套证据必须完整通过。`main()` 只调用它来产生 PASS，返回的摘要字段写入 evidence（`canonical_commands_executed`、`gate_input_identity_verified_after_commands`、`ignored_build_inputs_verified_after_commands`）。

### 2.2 `scripts/ci/local-gates.test.mjs`

既有 12 个 Gate 测试全部保留且仍绿，新增 10 个（共 22）：ignored 输入摘要字段与「值不入证据」、ignored 列表的过滤与嵌套未跟踪目录、`.env` 内容变化导致 Gate 输入同一性变化（DA-06 主反例）、无 ignored 输入的合法对照、ignored 输入沿用同组路径/链接/失败规则、history scope 拒绝 shallow 与空 refs、companion 日志归一化摘要、companion 缺失/失败/未声明/漂移阻断、整体 PASS 三出口、canonical 21 完整性（含源码级断言 `canonicalGateCommandCount = 21;` 与单一 `const gateCommands = [` 定义）。所有 fixture 都是系统临时目录里的 `git init --template=` 隔离无提交仓库，`finally` 清理，不触碰真实仓库与历史证据。

### 2.3 `scripts/ci/verify-workflows.mjs` 与 `.github/workflows/ci.yml`

- `ci.yml` 新增两个 job 并接入 `required-checks` 的 `needs` 与逐项 `test ... = success`：`history-secret-scan`（`fetch-depth: 0`，先 `git rev-parse --is-shallow-repository` 记录范围，再跑全 refs `gitleaks git`）与 `tooling-guards`（`pnpm install --frozen-lockfile` + `pnpm run verify:guards`）。
- `verify-workflows.mjs` 的 `requiredJobs` 相应增加这两个 job，并把 `pnpm run verify:guards` 加入必须在 workflow 中出现的门禁命令；另加两条断言：历史扫描必须是 `--log-opts=--all` 且带仓库策略，且该 job 必须在扫描前记录 shallow 状态。这关闭审计 §4 的第二条机制缺口（`verify:guards` 未接入 CI）。

## 3. 命令、退出码与日志

全部日志在 `docs/reviews/evidence/minimax-return/MM-R06/`。

| # | 命令 | exit | 日志 |
|---|---|---|---|
| 1 | `node --test scripts/ci/local-gates.test.mjs` | 0（22 passed / 0 failed） | `accept-01-gate-tests.log` |
| 2 | `pnpm run lint:ts` | **1（既有失败，见 §6）** | `accept-02-lint-ts.log`、基线 `baseline-lint-ts.log` |
| 3 | `pnpm run typecheck` | 0 | `accept-03-typecheck.log` |
| 4 | `pnpm run verify:guards` | 0 | `accept-04-verify-guards.log` |
| 5 | `node scripts/ci/local-gates.mjs --platform macos --dry-run` | 0 | `accept-05-dry-run.log` |
| 6 | `node scripts/ci/verify-workflows.mjs` | 0 | `accept-06-verify-workflows.log` |
| 7 | `pnpm exec prettier --check`（本包四个文件） | 0 | `accept-07-format-check.log` |
| 8 | `pnpm exec eslint`（本包三个 mjs） | 0 | `accept-08-eslint-changed-files.log` |
| 9 | companion 探针（两次跑两项配套 + `assertCompletedGateRun`） | 0 | `companion-checks-probe.json` / `.log` |
| 10 | 工作树/全 refs 两次 Gitleaks + shallow 状态 | 0 / 0 | `secret-scan-both-scopes.log` |

`verify:guards` 运行后 `tests/tooling-guard.invalid.ts` 已被自身 `finally` 清理（实测 `ls` 报 No such file），临时目录在 `mkdtemp` 系统目录下自行清理。

## 4. 真实 RED → GREEN 与合法对照

**RED 输入来源**：审计的 `replay_env_manifest.mjs`（只读）。该脚本会往只读历史证据目录写 `env-replay-<时间戳>` 目录，因此**未运行**；改为在系统临时目录抽取同一反例，见 `red-da06-reproduction.log`：`git ls-files --others --exclude-standard` 输出只有 `.gitignore`、`package.json`，两份不同 `.env.production` 的 legacy input manifest 哈希同为 `196ed327…`（与审计 `env-manifest-portable-replay.json` 一致），`input_manifest_still_blind: true`；修复后 Gate 输入同一性三段互不相同（`ad37999c…` / `371549b7…` / `e4966e64…`），`gate_input_identity_distinguishes_env_content: true`，`values_present_in_evidence: false`。

**合法对照**：无 ignored dotenv 的隔离仓库，两次 `collectGateInputs` 的 Gate 输入同一性与 input manifest 哈希都相同且稳定，`.env.example` 仍作为普通未跟踪输入计入 manifest——证明修复不是「一律拒绝」也不是「一律变化」。

**变异台账（10 条，全部被现有测试杀死）**：`red-mutation-summary.tsv` + `R1..R10*.log`。

| 变异 | 退出 | killed |
|---|---|---|
| R1 把 ignored 摘要从 Gate 输入同一性解绑（复现 DA-06） | 1（2 失败） | yes |
| R2 去掉 canonical 执行条数校验 | 1 | yes |
| R3 去掉运行前后输入同一性比对 | 1 | yes |
| R4 接受缺失的初始输入同一性 | 1 | yes |
| R5 配套缺失或失败时 `continue` | 1（2 失败） | yes |
| R6 忽略日志摘要漂移 | 1 | yes |
| R7 接受 shallow / 无 refs 的历史范围 | 1 | yes |
| R8 接受未声明的配套检查 | 1（2 失败） | yes |
| R9 去掉列表长度守卫 + 偷偷加第 22 条命令 | 1 | yes |
| R10 让日志摘要只哈希长度（对判定文本失明） | 1 | yes |

## 5. canonical 21 完整性与「固定 21 + 配套证据」可独立验证

- `canonicalGateCommandCount` 是字面量 `21`（不是由列表长度派生），`createGatePlan()` 在列表长度不等于 21 时直接抛错——R9 变异证明改列表或改计数都会被拦。
- `--dry-run` 输出 21 条 canonical 命令原序列，随后单独打印配套检查并显式声明「canonical command count stays 21」（`accept-05-dry-run.log`）。
- 证据 JSON 同时携带 `canonical_gate_command_count`、`canonical_commands_executed`、`companion_checks[]`、`gate_input_identity`、`input_manifest`、`ignored_build_inputs` 六个字段，因此「只含 21 条命令且全部 exit 0」的 JSON 无法冒充完整证据：`assertCompletedGateRun` 会因缺少配套证据或执行条数不符而拒绝。
- ADR-0005 未被降低：命令数保持 21，历史扫描作为**配套历史证据**而非第 22 条 canonical 命令，与审计 §4 的口径一致。

## 6. 未运行项与理由

- **完整 21 项 canonical 门禁实跑**（`local-gates.mjs --platform macos` 不带 `--dry-run`）：未运行。其中包含 `pnpm install --frozen-lockfile`、`pnpm audit`、`cargo audit`、全 workspace Rust 测试/构建/clippy。前两项与两个 audit 需要联网或访问公开 advisory 服务，本包授权明确禁止未经确认的联网 install/audit；Rust 侧还存在已知与本包无关的失败（`crates/persistence-sqlite` 11 个 adapter-supervisor 相关失败 + 2 处已登记版本计数断言，属 MM-R01～R05 在途），整体必然 FAIL，跑了也只会产出与本包无关的失败噪声。因此 canonical 21 的完整结论仍待 Codex 在统一门禁中运行。
- **`pnpm run lint:ts` exit 1 是既有失败**：唯一原因是只读历史证据文件 `docs/reviews/evidence/deep-audit-2026-10-08/replay_env_manifest.mjs:24:22 no-unsafe-finally`。已在改动前（`baseline-lint-ts.log`）与改动后（`accept-02-lint-ts.log`）各复现一次，错误位置与数量完全一致。该文件不在本包写集内，未修改，也未加排除规则或关闭规则。本包改动的三个 mjs 单独 ESLint exit 0（`accept-08`），Prettier 亦 exit 0。
- **需要在线/需授权才能完成的部分**（留待 Human 决定，本包未执行）：`pnpm audit`、`cargo audit`（需新鲜 advisory 库）、RustSec 在线判定、GitHub Actions 实际运行与 branch protection 服务端强制（私有仓库零 Actions 预算阶段本机证据才是正式门禁，ADR-0005 §4）。
- **`.github/workflows/ci.yml` 未在 GitHub 上实跑**：只做了本地静态契约验证（`verify-workflows.mjs`）与 Prettier 校验。

## 7. 剩余风险与回退

1. **ignored 摘要的敏感度**：SHA-256 是内容派生信息。本包只记录摘要与字节数，不记录任何值；若 Codex 认为应更保守，可改为「检测到 ignored dotenv 直接拒绝」，机制上只需替换 `listIgnoredBuildInputPaths` 的返回与 `assertCompletedGateRun` 的判定。
2. **`--log-opts=--all` 的语义**：扫描所有 ref 的可达提交。若 Human 要求包含不可达对象（reflog、dangling commit），需另加 `git fsck` 范围定义；当前范围已写入 `history_scope.refs_sha256` 可被独立核对。
3. **配套检查双跑**：`main()` 在 21 条命令前后各跑一次配套检查，用于捕捉运行期间的历史范围漂移与输入漂移，代价是 `verify:guards` 与全 refs 扫描各执行两次（本地约 10 秒）。可接受。
4. **maxBuffer 64 MiB**：真实 ignored 列表若继续增长到该量级会 fail-closed 报错，而不是静默截断；这是有意的失败方向。
5. **回退**：本包只改四个文件，全部为本包新增逻辑，可逐文件 `git checkout HEAD -- .github/workflows/ci.yml scripts/ci/verify-workflows.mjs`（两个未跟踪文件需从审计快照 `84bd09eb…`/`17f01e27…` 恢复）。回退会重新打开 DA-06 与两条机制缺口；`ci.yml` 的两个新 job 是纯增量，删除即回到旧状态。无需迁移、无领域/权限/协议语义变化。

## 8. 自检声明

SELF_CHECK_PASS。依据：DA-06 反例已从审计脚本抽取并在临时隔离仓库真红复现、修复后转绿；10 条变异全部被杀死；canonical 21 计数与顺序未被改动且有源码级守卫；配套证据的缺失/失败/漂移三条阻断路径均有测试与变异证据；改动的四个文件 lint、format、typecheck、静态契约全绿；未读取、未输出、未记录任何真实 secret 或真实环境文件内容。

不自行宣布节点 DONE，不解除任何 BLOCKED，不启动依赖包。正式准入由 Codex 独立逐差异复核后判定。