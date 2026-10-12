# FC-06 RESULT｜门禁证据契约补强（R2 证据契约）

- 节点：DEV-01-05（关联 01-03）；对应审计 §4 第一条的两项 P2 补强
- 执行模型/版本：**MiniMax-M3.1-Flash-Preview**（`ANTHROPIC_MODEL` 当前别名），macOS 27.0.0 arm64，Node v24.21.0，pnpm 10.33.0
- 仓库：`/Users/chenzhiqing/Documents/MyVault/协作工作`，分支 `codex/wave-2-ci-validation`，HEAD `1a14b8795e76fc373bef326eb5df96f9ac173f71` 加继承 dirty worktree
- 未 reset/checkout/clean/stash；未提交、推送、部署；未联网、未安装、未新增依赖、未新增费用
- 结论：**SELF_CHECK_PASS**（自验收，不等于节点 DONE；正式判定由 Codex 独立复核）

## 1. 输入与输出 SHA-256

输入基线取自 `prefix/`（本包改动前的真实字节快照），其 SHA 与 MM-R06-RESULT.md §1 声明的 R06 输出 SHA **逐字节一致**，证明基线就是审计所看的那份代码。

| 文件 | 输入 SHA-256（= MM-R06 输出） | 本包输出 SHA-256 |
|---|---|---|
| `.github/workflows/ci.yml` | `88a3eb5cf951f7064b5e593bd9e2559425c096fc6884e5d9ca4efaa4dbb7d1bf` | `70242c8b08c9f37ec57e047a243fa6a4f43ad666377eed415cbcdc997aaff135` |
| `scripts/ci/verify-workflows.mjs` | `29eacc66a7c90339e465686c1c8050a5de05216a623fb0389f9e550236eb3be6` | `6e7b93031c4e8a00ebe9b829c3cab0b796a6fef14c20c3eccb36cce35f5affd7` |
| `scripts/ci/local-gates.mjs` | `4cfdd3da3e06302cf58d2c0efe6eb3aab05f001104f625553b4f97cff6bbacc2` | `3ab5149f8a94f1a7483957e30cc07cf486cdf086e17de87e781e161a9183ee79` |
| `scripts/ci/local-gates.test.mjs` | `35f04f45740404a9ea18475e47f505293ea683e9e622eab10315f2de50a28ab3` | `0b9df1212561f15749fb57f3a70eb703ba0d94f458417f85d7300a8053a9f294` |

日志：`logs/00-prefix-snapshot-sha256.log`、`logs/09-output-sha256.log`。

## 2. 逐文件修改

### 2.1 `.github/workflows/ci.yml`（+13/-2，只动 `history-secret-scan` job）

把「只打印」的 `- run: git rev-parse --is-shallow-repository` 换成真正 fail-closed 的断言步骤：

```yaml
- name: fail closed unless the clone is complete
  run: |
    shallow="$(git rev-parse --is-shallow-repository)" || {
      echo "::error::could not determine whether this is a shallow repository"
      exit 1
    }
    if [ "$shallow" != "false" ]; then
      echo "::error::full history scan requires a complete clone; is-shallow-repository=${shallow}"
      exit 1
    fi
```

`true`、非零退出、空值/不可解析值一律 `exit 1`；断言位于 gitleaks 全 refs 扫描**之前**。其余 job、`required-checks` 接线、canonical 命令列表均未改动。

### 2.2 `scripts/ci/verify-workflows.mjs`

删除旧的字符串匹配断言：

```js
/history-secret-scan:[\s\S]*?- run: git rev-parse --is-shallow-repository\n\s+- run: gitleaks git/u
```

替换为四类**可执行**判据：

1. 按缩进切出 `history-secret-scan` job，要求其内含 `fetch-depth: 0`（原来完全没有这项要求）；
2. 解析该 job 的每个 `run` 步骤（同时支持 `run: |` 与 `- run: |` 块标量），定位既读 shallow 状态又能让 job 失败的 guard 步骤；
3. 断言 guard 步骤索引 < 扫描步骤索引；
4. **把 guard 脚本抽出来真的执行**：用桩 `git` 跑四种场景——`true`（必须非零）、`false`（必须为零）、命令失败退出 128（必须非零）、空输出（必须非零），任一不符即判失败。

第 4 条是关键：它把「检查这一行是否被打印过」换成「这段 shell 的判定是否正确」，所以只有真正读 shallow 状态并据此 fail-closed 的写法才能通过。

### 2.3 `scripts/ci/local-gates.mjs`

`assertMatchingCompanionEvidence()` 从「取第一条匹配」改为**唯一 + 定义绑定**：

- 同一 id 在 before/after 两侧必须**各恰一条**，多一条即拒（原来是 `find()`，追加的 failed 记录被静默忽略）；
- 每条记录的 `kind` / `command` / `args` 必须与 `companionCheckDefinitions()` 固定定义逐项匹配；
- 原有缺失/失败/摘要漂移/历史范围漂移判定全部保留。

`companionCheckDefinitions()` 由私有改为导出，供计划与测试直接比对定义。日志归一化逻辑**未改动**（仍是 R06 口径：只剥耗时/尺寸/时间戳/颜色，保留 commit 数与 `no leaks found` / `N leaks found` 等判定文本），并新增测试显式锁定该性质。

## 3. canonical 21 仍完整的证明

ADR-0005 未被降低：全 refs 历史扫描仍是**配套证据**，不是第 22 条 canonical 命令。

- 源码守卫未动：`export const canonicalGateCommandCount = 21;`（字面量，非派生值），`const gateCommands = [` 仍**只出现 1 次**；
- `--dry-run` 实际输出 **21** 条 `$ ` canonical 命令，随后单列打印 2 条 companion，并显式声明 `canonical command count stays 21`（`logs/05-accept-dry-run.log`）；
- 新增单测断言 `createGatePlan(...).companion_checks` 等于 `companionCheckDefinitions()`，且 companion 中的全 refs 扫描**不在** `plan.commands` 内；
- `createGatePlan()` 的长度不等于 21 即抛错的守卫未动。

## 4. 正反向 fixture（命令、exit、日志路径）

探针：`docs/reviews/evidence/minimax-followup/FC-06/red-fixtures/probe.mjs`。它在系统临时目录里搭一个假仓库根，把真实 checker 当 CLI 跑（`verify-workflows.mjs` 的 `root` 由自身路径推导，所以能整体搬家），companion 反例则直接调用导出函数。所有 fixture 都只碰 `mkdtemp` 目录，`finally`/`rmSync` 清理。

命令：

```
node docs/reviews/evidence/minimax-followup/FC-06/red-fixtures/probe.mjs baseline   # 对 prefix/ 快照
node docs/reviews/evidence/minimax-followup/FC-06/red-fixtures/probe.mjs current    # 对工作树
```

| fixture | 修复前 baseline | 修复后 current | 当前拒绝理由（current） |
|---|---|---|---|
| `shallow-state-never-asserted`（删断言） | rejected* | **rejected** | 必须断言 clone 非 shallow |
| `shallow-assertion-accepts-true`（断言写成 true） | rejected* | **rejected** | 执行后：shallow 时退出 0 |
| `checkout-without-fetch-depth`（缺 fetch-depth: 0） | **accepted** | **rejected** | 必须 `fetch-depth: 0` |
| `branch-only-history-scan`（缺 `--all`） | rejected | **rejected** | 必须覆盖全 refs |
| `control-r06-print-only-workflow`（审计点名的原始 R06 workflow） | **accepted** | **rejected** | 只打印不算 fail-closed |
| `duplicate-failed-record`（同 ID 多一条 failed） | **accepted** | **rejected** | 声明 id 两侧各恰一条 |
| `swapped-history-scan-command`（换掉扫描命令） | **accepted** | **rejected** | args 与固定定义不符 |
| `kind-does-not-match-definition`（kind 不符） | **accepted** | **rejected** | kind 与固定定义不符 |
| `swapped-guards-command`（换掉 companion 命令） | **accepted** | **rejected** | args 与固定定义不符 |
| `control-unmodified-workflow`（合法对照） | accepted | **accepted** | — |
| `control-complete-companion`（合法对照） | accepted | **accepted** | — |

`*` **重要**：baseline 下前两项被拒**不是因为检查生效**，而是因为旧正则要求那一行字面存在，删掉/改写就失配——这是与 fail-closed 无关的偶然失配，不构成旧检查有效的证据。真正的 RED 是 baseline 下**被接受**的那 5 项（含审计点名的 `control-r06-print-only-workflow`），它们现已全部被拒，且拒绝理由逐条不同、指向真实判据。

日志：`logs/01-red-baseline.log`（RED）、`logs/02-green-current.log`（GREEN）、`logs/01-red-baseline-notes.md`（逐项解读）。两个模式的探针在结果与记录不符时以 exit 1 失败，因此旧日志无法被当作证据重放。

**真实仓库证据（非桩）**：把 ci.yml 里的 guard 抽出来，对本 checkout（非 shallow）执行 exit 0、对 `git clone --depth 1 file://...` 造出的真实 shallow 仓库执行 exit 1 并打出 `is-shallow-repository=true`，见 `logs/08-guard-real-repo.log`。

## 5. 验收命令

| 命令 | exit | 日志 |
|---|---|---|
| `node --test scripts/ci/local-gates.test.mjs` | 0（25 passed / 0 failed） | `logs/03-accept-gate-tests.log` |
| `node scripts/ci/verify-workflows.mjs` | 0 | `logs/04-accept-verify-workflows.log` |
| `node scripts/ci/local-gates.mjs --platform macos --dry-run` | 0（21 canonical + 2 companion 单列） | `logs/05-accept-dry-run.log` |
| `npx prettier --check .github/workflows/ci.yml scripts/ci/local-gates.mjs scripts/ci/local-gates.test.mjs scripts/ci/verify-workflows.mjs` | 0 | `logs/06-accept-format.log` |
| `npx eslint scripts/ci/local-gates.mjs scripts/ci/local-gates.test.mjs scripts/ci/verify-workflows.mjs` | 0 | `logs/07-accept-eslint.log` |

单测由 22 增至 25（新增：companion 唯一性/定义绑定、日志归一化保留判定内容、CI fail-closed 契约）。既有 22 条全部保留且仍绿。

两处 R06 遗留测试的 companion 夹具已改为用真实 definition 构造——新的定义绑定把它们判为不合格，这是新契约按预期生效，不是放宽。

## 6. 明确未声称

- **未跑完整 Mac canonical 21 实跑**（`local-gates.mjs --platform macos` 不带 `--dry-run`）：其中含 `pnpm install --frozen-lockfile`、`pnpm audit`、`cargo audit`，需要联网/advisory，本包无授权；且 Rust 侧存在 FC-01～FC-05 在途失败。
- **未跑 hosted CI**：`.github/workflows/ci.yml` 只做本地静态契约验证 + guard 真机执行，没有在 GitHub Actions 上实跑，也不能据此声称已有 branch protection 服务端强制。
- **未联网**：没有 `pnpm install`、没有 `pnpm audit`/`cargo audit`、没有下载、没有新增依赖。
- **这不是一次真实门禁绕过演示**。以下必须说清楚：
  - companion 反例**只能通过直接传参构造**。`assertMatchingCompanionEvidence()` 的记录由调用方传入，而当前 `main()` 是在进程内用上面的固定 definition 自己构造 before/after，**不读取任何外部证据 JSON**。
  - 因此本项是**独立校验契约补强**：为将来由 Codex 或 CI 串行接入外部证据时定义「什么才算合格的证据」，提前堵住重复 ID 与伪造 command 这两类输入。
  - **不宣称已证明当前 main 受外部 JSON 注入影响**——那条路径今天并不存在，把它说成是门禁绕过属于不实陈述。
- 未修改 `eslint.config.mjs`（FC-09 范围），未 blanket ignore evidence，未关闭任何规则，未改动任何历史字节。

## 7. 剩余风险与回退

1. **YAML 解析是缩进启发式**。为不新增依赖，`extractWorkflowJob()` / `readStepScripts()` 按「job 在 2 空格、step 在 6 空格」解析，与本仓库 workflow 现状一致。风险：把该 job 改成非标准缩进或改用 flow 风格会误判为「缺少断言」——方向是 fail-closed（误拒而非误放），但需在改动 ci.yml 时同步复核。彻底解法是引入 YAML 解析依赖，属新增依赖，本包禁止。
2. **静态检查会执行 workflow 里的 shell 片段**（用桩 `git`、临时目录、`process.env.PATH` 前置桩目录）。片段来自仓库内受评审的 ci.yml，且已拒绝含 `${{ }}` 的片段；但这确实让 verify-workflows.mjs 从「纯静态」变成「执行片段」。若 Codex 认为不可接受，退路是把判据降回静态 token 断言——**代价是重新打开「只匹配打印过这一行」这一原始缺陷**，不建议。
3. **桩 `git` 覆盖四种场景而非真实 git 全部行为**。真实 shallow/full 的行为另由 `logs/08-guard-real-repo.log` 用真仓库补证；两者互补，不互相替代。
4. **归一化口径未改**。`709699 bytes`（数字与单位之间有空格）不在 R06 的替换式覆盖内，两次运行的字节数若不同仍会被判为漂移——这是**偏严**的既有行为，本包按最小改动原则未动，留给 Codex 裁定是否收紧该正则。
5. **回退**：本包只改 4 个文件，均为本包新增逻辑，可逐文件回退（`ci.yml` 与 `verify-workflows.mjs` 用 `git checkout HEAD --`；`local-gates.mjs`/`local-gates.test.mjs` 未跟踪，从 `prefix/` 快照恢复）。回退会重新打开审计 §4 的两条 P2：CI shallow 只打印、companion 缺唯一性/定义绑定。无需迁移，无领域/权限/协议语义变化，canonical 21 不受影响。

## 8. 自检声明

**SELF_CHECK_PASS**。依据：两项 P2 均有先 RED 后 GREEN 的可重放 fixture（修复前 5 个反例被**接受**，修复后 9 个反例**全部被拒**且理由逐条指向真实判据，2 个合法对照始终通过）；静态检查对 fail-closed 的判据由**实际执行 guard** 得出，不依赖字符串出现；guard 另经真实非 shallow 仓库与真实 `--depth 1` shallow 仓库双向验证；canonical 21 的字面量、单一定义列表、dry-run 实输出条数与 companion 单列四重佐证；本包 4 个文件的 test/verify/dry-run/prettier/eslint 全 exit 0；未读取、未输出、未记录任何真实 secret 或环境文件内容。

不自行宣布节点 DONE，不解除任何 BLOCKED，不启动依赖包。正式准入由 Codex 独立逐差异复核后判定。