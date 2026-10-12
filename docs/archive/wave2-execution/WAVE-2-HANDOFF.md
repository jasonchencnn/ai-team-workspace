# Wave 2 总控交接与后续执行手册

> 2026-09-24 补充：本文件保留 2026-09-23 的历史建议和操作记录。Human 已接受 ADR-0004，改为 Gemini-3.8-Flash 主力开发、MiniMax-M3 边角辅助；下文第 5、7 节的旧模型分工已被取代，不可据此派工。当前安排与长期目标见 `docs/coordination/README.md`、`docs/coordination/GEMINI_WAVE2_LONG_TERM_GOAL.md`。旧双环境结果不能证明当前文档已变更后的输入快照。

- 交接日期：2026-09-23
- 当前阶段：Wave 2，尚未完成，不得进入 Wave 3
- Human 决策：ADR-0003 已批准；允许 macOS ARM64 + Ubuntu ARM64 替代门禁，接受无 GitHub 服务端 required checks；新增费用上限为 `$0`
- 2026-09-23 历史计划模型（已被 2026-09-24 Human 调整取代）：MiniMax-M3 负责边界明确的工程实现；Gemini-3.8-Flash 负责机械性核对、测试执行和独立只读复审
- 禁止事项：不提交、不推送、不部署、不修改 `sources/`，不购买 GitHub Actions 或其他付费资源，不使用浏览器执行工程操作

## 1. 开始前必须知道的结论

Wave 2 第一批已经完成并通过独立审核：

| 节点      | 状态   | 结论                                                                                                                                   |
| --------- | ------ | -------------------------------------------------------------------------------------------------------------------------------------- |
| DEV-01-05 | `DONE` | 零费用 macOS ARM64 + Oracle Ubuntu ARM64 门禁、秘密/依赖/许可证检查与独立审核已通过；ADR-0003 记录了与 hosted required checks 的差异。 |
| DEV-01-08 | `DONE` | 结构化 JSON 日志、correlation ID、统一脱敏与测试已通过。                                                                               |
| DEV-01-09 | `DONE` | 可重复 fixture、时钟/ID/随机源注入与失败路径测试已通过。                                                                               |

当前第二批的实现和专项独立审核也已经通过，但节点仍应保持 `REVIEW`，直到后续执行者完成本手册第 6 节的最终证据收口：

| 节点      | 当前状态 | 已完成                                                                                                                                                       | 尚缺                                                                                      |
| --------- | -------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------ | ----------------------------------------------------------------------------------------- |
| DEV-02-02 | `REVIEW` | Team/Role/Workspace migration、UUIDv7 BLOB 约束、外键、唯一性、版本、停用/归档、重复 migration 测试；原两项审核阻断已关闭；SQLx 依赖收窄复审 PASS。          | 当前源码最终快照的双环境证据包、全 refs 历史秘密扫描、批次独立终审、审核记录最终 `DONE`。 |
| DEV-03-02 | `REVIEW` | Tokio 子进程 supervisor、参数数组、环境清空、有界 stdio、deadline/cancel、直接子进程回收、遗留 pipe 后代有界退出；原两项 S1 已由第二位 Terra reviewer 关闭。 | 与 DEV-02-02 相同的最终证据收口和批次独立终审。                                           |

重要：本交接文件本身是新的仓库输入，因此下文记录的双环境通过证据只证明添加本文件之前的 155 文件快照。按照 ADR-0003，任何源码、文档、未跟踪输入、锁文件、策略或脚本变化都会使旧证据失效。后续执行者不得直接把 DEV-02-02/DEV-03-02 标为 `DONE`；必须针对接手后的最终工作区重新运行 macOS 和 Ubuntu 门禁。

## 2. 当前工作区与主要变更

本地仓库：

```text
/Users/chenzhiqing/Documents/MyVault/协作工作
```

当前存在未提交变更，全部都属于本轮工作。不要执行 `git reset --hard`、`git checkout --`、清理未跟踪文件或覆盖用户改动。

主要文件：

```text
.github/workflows/ci.yml
.gitleaks.toml
AGENTS.override.md
Cargo.lock
crates/adapter-supervisor/Cargo.toml
crates/adapter-supervisor/src/lib.rs
crates/persistence-sqlite/Cargo.toml
crates/persistence-sqlite/src/lib.rs
crates/persistence-sqlite/migrations/0001_team_role_workspace.sql
crates/persistence-sqlite/tests/team_role_workspace_migration.rs
docs/adr/0003-zero-cost-dual-environment-quality-gates.md
docs/development/QUALITY_GATES.md
docs/reviews/DEV-01-05-REVIEW.md
docs/reviews/DEV-02-02-REVIEW.md
docs/reviews/DEV-03-02-REVIEW.md
scripts/ci/local-gates.mjs
scripts/ci/local-gates.test.mjs
scripts/ci/ubuntu-gate.Dockerfile
scripts/ci/verify-workflows.mjs
```

### DEV-02-02 实现摘要

- migration：`0001_team_role_workspace.sql`。
- Team、Role、Workspace 主键使用 UUIDv7 16-byte BLOB。
- Role/Workspace 的 `team_id` 自身也检查 UUIDv7 version nibble 和 RFC 4122 variant，不只依赖外键。
- `version >= 1`；持久化时间使用整数 Unix milliseconds。
- Team 归档状态与 `archived_at` 一致；Role 停用状态与 `deactivated_at` 一致。
- Team 内 Role `display_id` 唯一；一个 Team 最多一个 Workspace。
- 外键使用 `ON DELETE RESTRICT`，保留历史。
- SQLx 真实 SQLite 测试 4/4 PASS；同一 pool 第二次执行 migration 是安全 no-op。
- 为避开未使用的 MySQL/Postgres 依赖与 `RUSTSEC-2023-0071`，不再使用顶层 `sqlx` crate，改用精确固定的：

```toml
sqlx-core = { version = "=0.8.6", default-features = false, features = ["_rt-tokio", "migrate"] }
sqlx-sqlite = { version = "=0.8.6", default-features = false, features = ["bundled", "migrate"] }
```

- migration 通过公开 `MigrationSource`、`Migration::new`、`Migrator::new` 加载，不直接构造 `Migrator` 的 `#[doc(hidden)]` 字段。
- `Cargo.lock` 不应出现 `rsa`、`sqlx-mysql`、`sqlx-postgres`、`sqlx-macros`。
- `_rt-tokio` 是 SQLx 内部 feature；当前接受条件是私有 crate、精确 `=0.8.6`、`Cargo.lock` 和 `--locked`。任何 SQLx 升级或解除精确版本都必须重新复审。

### DEV-03-02 实现摘要

- 使用 Tokio 异步子进程和 stdin/stdout/stderr。
- 使用 `Command::new` + 参数数组，不经过 shell。
- 启动时 `env_clear()`，只注入调用方显式提供的环境变量。
- stdout 在 JSON 解析前按原始字节执行 1 MiB JSON Lines 上限，并使用有界 channel。
- stderr 始终排空，只保留有界诊断前缀和截断标记。
- `send_protocol_line` 必须提供绝对 deadline 和 `CancellationToken`。
- 写超时、取消或 pipe 错误会关闭 stdin、启动 kill，并返回稳定错误。
- 显式 `shutdown` / `wait_for_exit` 会终止并 reap 直接子进程。
- 直接子进程退出后只给 reader task 100 ms 有界排空；若遗留后代仍持有 pipe，则 abort reader，不再无限 join。
- `Drop` 不等待、不阻塞控制面线程；需要确定回收证据时必须显式调用 `shutdown` / `wait_for_exit`。
- 强制终止报告为 `Termination::ForcedShutdown`，不会误报为优雅 EOF。
- 测试 16/16 PASS，并有外层 watchdog 覆盖“后代持 pipe”和“Adapter 不读 1 MiB stdin”两个原 S1 反例。

## 3. 已取得但会被后续改动失效的门禁证据

### macOS ARM64

```text
/private/tmp/ai-team-wave2-batch2-macos-v6.json
```

结果：

```text
result: pass
commands: 21/21
input files: 155
input total SHA-256: 710934ed89c10d7806d80e44e36da47922e347858303b46d96e9750b1a930742
post-run verified SHA-256: 710934ed89c10d7806d80e44e36da47922e347858303b46d96e9750b1a930742
architecture: arm64
platform: darwin
```

### Oracle Ubuntu ARM64

远程证据：

```text
~/ai-team-wave2-batch2-v1/evidence-v2/container/WAVE2-batch2-ubuntu-arm64-v1.json
```

结果：

```text
result: pass
commands: 21/21
input files: 155
input total SHA-256: 710934ed89c10d7806d80e44e36da47922e347858303b46d96e9750b1a930742
post-run verified SHA-256: 710934ed89c10d7806d80e44e36da47922e347858303b46d96e9750b1a930742
container: ai-team-wave2-batch2-v1
container state: exited
container exit code: 0
sing-box: active/running
```

两个平台的输入条目数和总哈希一致，证明当时运行的是同一规范化源码快照。由于本文件随后加入仓库，这些证据只能作为历史诊断证据，不能作为最终 `DONE` 证据。

### 已通过的共同检查

- Gitleaks 当前文件快照扫描：无命中。
- pnpm frozen install。
- Node/pnpm/Rust/Gitleaks/cargo-audit/cargo-deny 固定版本校验。
- workflow 静态契约。
- Prettier/rustfmt。
- ESLint、TypeScript strict。
- TypeScript/Vitest：35/35。
- 前端构建。
- Rust workspace tests：45/45。
- Rust workspace build。
- Clippy `-D warnings`。
- Rust Schema → TypeScript 生成漂移。
- Rust unsafe 策略。
- `pnpm audit --audit-level=high`：只有 1 low + 1 moderate，不触发 high 门禁。
- `cargo audit`：0 vulnerability；保留 7 条既有 allowed warning，不能描述为“零 warning”。
- cargo-deny licenses 与 Node licenses。

## 4. Oracle 远程环境使用说明

### 登录规则

只能使用 SSH 别名 `oracle`，不要使用 IP，不要尝试公网 22 端口：

```bash
ssh -T oracle '<单条非交互命令>'
scp <本地文件> oracle:~/
```

连接经 Cloudflare Tunnel。服务器是 Oracle Cloud ARM64，系统用户是 `ubuntu`，Docker 命令需要 `sudo`。

先做只读检查：

```bash
ssh -T oracle 'uname -m; uname -s; systemctl show sing-box --property=ActiveState,SubState --no-pager; df -h /; sudo docker version --format {{.Server.Version}}'
```

预期：

```text
aarch64
Linux
ActiveState=active
SubState=running
```

### 绝对禁止触碰的对象

- 不得访问、挂载、修改或删除 `~/readest/docker`。
- 不得挂载 Docker socket 到门禁容器。
- 不得加入任何现有生产网络或挂载现有生产 volume。
- 不得读取生产凭据或生产数据。
- `sing-box` 必须在门禁前后均为 `active/running`。
- 不要停止、重启或重建现有容器。

当前可见现有容器包括 `readest-*`、`supabase-*`、`hermes*`、`alist`；全部视为范围外，不要操作。

### 当前专用测试目录

```text
~/ai-team-wave2-batch2-v1/
├── upload/
├── workspace-v2/
├── home-v2/
├── evidence-v2/
│   └── container/
└── workspace/          # 第一次带 macOS 扩展属性旁车文件的废弃快照，不要作为证据输入
```

当前专用 Docker 对象：

```text
container: ai-team-wave2-batch2-v1
network:   ai-team-wave2-batch2-net-v1
image:     ai-team-gate:dev-01-05-final-v3
image ID:  sha256:789c60ceb3d2d91d9a1c0efe3b18fb59bcaf939c9fe24f82ab7f5871a6661699
```

该容器已退出，退出码为 0。不要立即删除；先完成第 6 节的日志、inspect、校验和与独立审核取证。旧的 `workspace/` 目录包含 macOS tar 扩展属性产生的 `._*` 旁车文件，已改由 `COPYFILE_DISABLE=1` 创建的 `workspace-v2/` 替代。

### 当前容器隔离参数

必须在新批次继续保持：

```text
user: 1000:1000
read-only rootfs: true
CPU: 2
memory: 8 GiB
PIDs: 1024
cap-drop: ALL
security-opt: no-new-privileges
tmpfs /tmp: rw,nosuid,nodev,size=2 GiB
独立 bridge network
只挂载专用 workspace、evidence、home
不挂载 docker.sock、生产目录、生产卷或生产网络
```

## 5. 历史模型分工与冲突规则（已被协作入口取代）

> 本节仅用于理解 2026-09-23 的执行设想；2026-09-24 起不可按下列模型角色派工。

Human 计划使用 MiniMax-M3 和 Gemini-3.8-Flash。建议固定角色，避免两个模型同时改同一文件：

### MiniMax-M3：实现者

- 负责单个明确 DEV 节点的 SQL/Rust/测试实现。
- 每次只允许一个 persistence 节点写 `crates/persistence-sqlite/**`。
- 每次只允许一个 Adapter 节点写 `crates/adapter-supervisor/**` 或新增 Adapter crate 文件。
- 开始前必须读取节点关联全部冻结规格，不得自行添加状态、错误码、权限或协议字段。
- 不得改根 `Cargo.toml`、`Cargo.lock`；如确需依赖，先列出需求，由总控串行修改。

### Gemini-3.8-Flash：只读审核者和机械验证者

- 默认只读代码、运行测试、寻找反例、检查文档和证据。
- 只允许在对应 `docs/reviews/DEV-*.md` 末尾追加独立审核小节。
- 不得在审核时顺手修代码，否则失去独立性。
- 对 migration、协议、进程、权限、fencing 等 R3 节点，必须给出至少一个负向反例或明确说明现有测试如何覆盖。
- 如果发现 S0/S1 或任何门禁失败，结论必须是 `BLOCKED`，不能写“基本通过”。

### 总控必须串行处理

- 根 manifest 和 `Cargo.lock`。
- 跨节点冲突整合。
- macOS/Ubuntu 完整门禁与证据归档。
- 节点状态从 `REVIEW` 到 `DONE` 的最终决定。
- 全历史秘密扫描。

每轮最多 3 个 Agent。当前只有两个外部模型时，最多安排一个实现者 + 一个只读审核者；不要为了并发让两者同时写同一 crate。

## 6. 接手后的第一件事：收口 DEV-02-02 与 DEV-03-02

### 6.1 重新读取强制基线

按顺序完整读取：

```text
AGENTS.md
AGENTS.override.md
docs/development/README.md
docs/development/TECH_STACK_AND_STANDARDS.md
docs/development/DEVELOPMENT_NODES.md
docs/development/QUALITY_GATES.md
docs/specs/README.md
docs/specs/REQUIREMENTS_TRACEABILITY.md
docs/specs/DOMAIN_MODEL.md
docs/specs/STATE_MACHINES.md
docs/specs/PERMISSION_MATRIX.md
docs/specs/DATA_CONVENTIONS.md
docs/specs/ERROR_CATALOG.md
docs/specs/adapter/v0/PROTOCOL.md
docs/specs/adapter/v0/adapter-manifest.schema.json
docs/specs/adapter/v0/adapter-message.schema.json
docs/adr/0002-strengthen-adapter-v0-machine-contract.md
docs/adr/0003-zero-cost-dual-environment-quality-gates.md
docs/reviews/WAVE-1-INDEPENDENT-PROTOCOL-AUDIT.md
docs/reviews/DEV-02-02-REVIEW.md
docs/reviews/DEV-03-02-REVIEW.md
```

### 6.2 本地定向复检

```bash
cargo fmt --all -- --check
cargo test -p persistence-sqlite --locked
cargo test -p adapter-supervisor --locked -- --test-threads=1
cargo clippy -p persistence-sqlite -p adapter-supervisor --all-targets --all-features --locked -- -D warnings
git diff --check
```

确认锁文件没有不需要的数据库/RSA 依赖：

```bash
rg -n '^name = "(rsa|sqlx-mysql|sqlx-postgres|sqlx-macros)"$' Cargo.lock
```

预期无输出。

### 6.3 运行新的 macOS ARM64 完整门禁

工具目录：

```text
/private/tmp/ai-team-macos-gate-tools-20260923
```

示例命令；证据文件使用新的、不存在的名字，因为脚本以排他方式创建：

```bash
/usr/bin/env \
  CI=true \
  npm_config_registry=https://registry.npmjs.org \
  PATH=/private/tmp/ai-team-macos-gate-tools-20260923/bin:/private/tmp/ai-team-macos-gate-tools-20260923/node-v24.21.0-darwin-arm64/bin:/Users/chenzhiqing/.cargo/bin:/opt/homebrew/bin:/usr/local/bin:/usr/bin:/bin:/usr/sbin:/sbin \
  node scripts/ci/local-gates.mjs \
  --platform macos \
  --evidence /private/tmp/ai-team-wave2-batch2-macos-v7.json
```

必须检查：

```bash
jq '{result,command_count:(.commands|length),input_count:(.input_manifest.entries|length),input_hash:.input_manifest.total_sha256,verified_after:.input_manifest_verified_after_commands}' /private/tmp/ai-team-wave2-batch2-macos-v7.json
```

必须满足 `result=pass`、`command_count=21`、前后 hash 相等。

### 6.4 创建不含 macOS 旁车文件的输入快照

必须使用刚通过门禁的 JSON 清单，并设置 `COPYFILE_DISABLE=1`：

```bash
jq -r '.input_manifest.entries[].path' /private/tmp/ai-team-wave2-batch2-macos-v7.json \
  | /usr/bin/env COPYFILE_DISABLE=1 tar -czf /private/tmp/ai-team-wave2-batch2-source-v3.tar.gz -T -

tar -tzf /private/tmp/ai-team-wave2-batch2-source-v3.tar.gz | rg '(^|/)\._'
shasum -a 256 /private/tmp/ai-team-wave2-batch2-source-v3.tar.gz
```

`rg` 预期无输出。归档条目数必须等于 macOS evidence 的 `input_manifest.entries` 数量。

不要上传完整 Git bundle。一次尝试因完整历史可能包含敏感内容而被安全策略拒绝；历史扫描应在本机真实仓库完成。Oracle 只接收已由输入清单限定的当前源码快照。

### 6.5 在 Oracle 创建新隔离批次

使用新的目录、容器和网络名称，例如：

```text
~/ai-team-wave2-batch2-v2
ai-team-wave2-batch2-v2
ai-team-wave2-batch2-net-v2
```

上传：

```bash
ssh -T oracle 'mkdir -p "$HOME/ai-team-wave2-batch2-v2/upload"'
scp /private/tmp/ai-team-wave2-batch2-source-v3.tar.gz oracle:~/ai-team-wave2-batch2-v2/upload/
scp /private/tmp/ai-team-wave2-batch2-macos-v7.json oracle:~/ai-team-wave2-batch2-v2/upload/
```

在专用目录解压。因为 `local-gates.mjs` 需要 `HEAD` 和 Git 文件清单，可在该隔离快照内创建一个临时本地 Git commit；这不是用户仓库提交，也不得推送：

```bash
ssh -T oracle 'set -eu; base="$HOME/ai-team-wave2-batch2-v2"; mkdir -p "$base/workspace" "$base/evidence/container" "$base/home"; tar -xzf "$base/upload/ai-team-wave2-batch2-source-v3.tar.gz" -C "$base/workspace"; cd "$base/workspace"; git init -q; git add -A; git -c user.name="ADR-0003 Gate" -c user.email="gate.invalid" commit -qm "ephemeral input snapshot"; git status --short; git rev-parse HEAD'
```

创建容器时沿用第 4 节的隔离参数。镜像 `ai-team-gate:dev-01-05-final-v3` 可复用，但必须重新记录 image inspect，并验证当前 `scripts/ci/ubuntu-gate.Dockerfile` SHA-256 与已验证版本一致。容器命令：

```text
node scripts/ci/local-gates.mjs --platform ubuntu --evidence /evidence/WAVE2-batch2-ubuntu-arm64-v2.json
```

容器必须退出 0，证据必须 21/21 PASS，且 Ubuntu 的输入条目数、总 hash 和 post-run hash 与 macOS v7 完全相同。

### 6.6 补齐 Oracle 运行后证据

在删除任何测试对象前保存：

```bash
ssh -T oracle 'base="$HOME/ai-team-wave2-batch2-v2"; sudo docker logs ai-team-wave2-batch2-v2 > "$base/evidence/ubuntu-gate.log" 2>&1; sudo docker inspect ai-team-wave2-batch2-v2 > "$base/evidence/container-inspect-after.json"; sudo docker network inspect ai-team-wave2-batch2-net-v2 > "$base/evidence/network-inspect-after.json"; systemctl show sing-box --property=ActiveState,SubState --no-pager > "$base/evidence/sing-box-after.txt"; sudo docker image inspect ai-team-gate:dev-01-05-final-v3 > "$base/evidence/gate-image-inspect.json"; sudo docker inspect ai-team-wave2-batch2-v2 --format "{{.State.ExitCode}}" > "$base/evidence/container-exit-code.txt"'
```

另外保存：

- `sing-box-before.txt` 与 `sing-box-after.txt`，必须完全一致且为 active/running。
- `container-inspect-before.json` 与 after。
- network inspect before/after。
- source archive SHA-256。
- Dockerfile SHA-256。
- image ID、architecture、OS。
- APT 包版本、基础镜像 digest、工具版本与官方来源校验和。
- 所有文件的 `SHA256SUMS`。

可以引用旧证据包 `~/ai-team-gate-evidence-dev0105-final-v9` 中与相同 image ID 绑定的 APT、base image 和工具来源材料，但新汇总必须明确记录：复用的是同一个不可变 image ID，且当前 Dockerfile hash 与旧版一致。

### 6.7 在本机真实 Git 仓库运行全 refs 历史秘密扫描

不要把完整 Git 历史上传到 Oracle。应在本机真实仓库执行：

```bash
git rev-parse --is-shallow-repository
git rev-parse HEAD
git show-ref --head
git fsck --full
/private/tmp/ai-team-macos-gate-tools-20260923/bin/gitleaks git \
  --log-opts='--all' \
  --redact \
  --config .gitleaks.toml \
  --exit-code 1 \
  --report-format json \
  --report-path /private/tmp/ai-team-wave2-batch2-gitleaks-history-v2.json \
  .
```

必须满足：不是 shallow clone、`git fsck` 通过、Gitleaks 退出 0、报告无命中。汇总 JSON 必须同时记录当前输入总 hash、`.gitleaks.toml` hash、HEAD、全部 refs 列表、命令和日志 hash。

### 6.8 独立批次终审

2026-09-23 历史建议是让 Gemini-3.8-Flash 做只读终审，不允许改实现；当前 ADR-0003 指定的 Terra 独立证据审核仍不可省略。至少核对：

1. macOS 与 Ubuntu 均为 21/21 PASS。
2. 两个平台输入条目数一致、总 SHA-256 一致。
3. 两个平台运行前后输入 hash 不变。
4. 所有 command 的 `exit_code` 都是 0。
5. Rust 45/45、TypeScript 35/35；生成漂移、unsafe、安全、许可证均通过。
6. `cargo audit` 为 0 vulnerability；7 条 allowed warning 被如实记录。
7. Oracle 容器为 ARM64、非 root、只读根、2 CPU、8 GiB、PID 1024、cap-drop ALL、no-new-privileges。
8. mounts/network 中没有 Docker socket、`readest`、生产 volume 或生产 network。
9. `sing-box` 前后均 active/running。
10. 本地 Git 非 shallow，全 refs Gitleaks history scan 通过。
11. DEV-02-02 与 DEV-03-02 的专项独立审核均已 PASS。

只有终审 PASS 后，才能在：

```text
docs/reviews/DEV-02-02-REVIEW.md
docs/reviews/DEV-03-02-REVIEW.md
```

追加最终门禁证据，并把两个节点结论改为 `DONE`。任何缺证或失败都保持 `REVIEW/BLOCKED`。

## 7. 两个当前节点关闭后的后续执行顺序

不得越过依赖。Persistence migration 必须只有一个写者，并严格串行；Adapter 链也严格串行。

2026-09-23 的推荐批次（模型列已失效；节点依赖仍供参考，实际派工以 `docs/coordination/README.md` 为准）：

| 批次        | MiniMax-M3 实现                                   | Gemini-3.8-Flash                                | 前置条件                                                |
| ----------- | ------------------------------------------------- | ----------------------------------------------- | ------------------------------------------------------- |
| A           | DEV-02-03 Harness/Adapter/Binding migration       | 先审核已完成的 DEV-03-03 方案或在实现后只读审核 | DEV-02-02 `DONE`；DEV-03-02 `DONE`                      |
| A（可并行） | DEV-03-03 initialize/能力/健康检查                | 审核 DEV-02-03                                  | 两个当前节点 `DONE`；写集必须与 persistence 分离        |
| B           | DEV-02-04 Task/Dependency migration               | 审核 DEV-03-04                                  | DEV-02-03 `DONE`                                        |
| B（可并行） | DEV-03-04 Mock Adapter 正常流                     | 审核 DEV-02-04                                  | DEV-03-03 `DONE`                                        |
| C           | DEV-02-05 Run/Lease/Review migration              | 审核 DEV-03-05                                  | DEV-02-04 `DONE`                                        |
| C（可并行） | DEV-03-05 Mock 故障注入                           | 审核 DEV-02-05                                  | DEV-03-04 `DONE`                                        |
| D           | DEV-02-06 Artifact/Reference migration            | 只读审核                                        | DEV-02-04、02-05 `DONE`                                 |
| E           | DEV-02-07 Event/Outbox migration                  | 只读审核                                        | DEV-02-01 `DONE`；前一 persistence 写者结束             |
| F           | DEV-02-08 Thread/Message/Delivery migration       | 只读审核                                        | DEV-02-01 与冻结 Message 规格；DEV-02-07 已整合后再开始 |
| G           | DEV-02-09 Notification/Policy/Attention migration | 只读审核                                        | DEV-02-07、02-08 `DONE`                                 |
| H           | DEV-02-10 Permission/Approval migration           | 只读审核                                        | DEV-02-01 `DONE`；前一 persistence 写者结束             |

为降低冲突，推荐 persistence 的实际串行顺序固定为：

```text
DEV-02-03
→ DEV-02-04
→ DEV-02-05
→ DEV-02-06
→ DEV-02-07
→ DEV-02-08
→ DEV-02-09
→ DEV-02-10
```

Adapter 固定为：

```text
DEV-03-03 → DEV-03-04 → DEV-03-05
```

同一行的 persistence 与 Adapter 工作在文件归属完全分离时可以并行；同一 crate 内绝不并行。

## 8. 后续节点的文件归属和审核重点

### DEV-02-03

- 唯一写者：`crates/persistence-sqlite/migrations/0002_*`、对应 persistence 测试；必要的 crate API。
- 不修改 `0001_team_role_workspace.sql`。
- 重点：一个 Role 最多一个有效主 Binding；历史有效区间不可重叠；Harness/Adapter/Binding 身份不混淆。
- 必须用真实 SQLite 并发/约束反例；不能只做 Rust 内存校验。

### DEV-02-04

- 新增下一条前向 migration，不改既有 migration。
- 重点：Task 状态、owner Role、引用、版本、Team 内显示 ID 唯一、Dependency 外键与环路数据边界。
- 不要在数据库中虚构冻结规格没有定义的 Task 状态。

### DEV-02-05

- 重点：active Run 唯一性、lease、fencing token、attempt 序号、Review 与 Result Artifact 条件。
- 至少覆盖两个 Run 同时 claim、旧 token、过期 lease、重复 attempt 的负向测试。

### DEV-02-06

- 重点：creator Role、Run、版本、URI、commit/checksum、可见范围可追溯。
- 不复制 Workspace 长文本到数据库。

### DEV-02-07

- 重点：业务变更、Event、Outbox 原子提交；Event 追加写；消费去重和可重放。
- 不提供 Event update/delete repository。

### DEV-02-08

- 重点：`to_role_id` 强制存在；Binding snapshot、`reply_to`、TTL、幂等和 delivery attempt 历史。
- Human 不能伪装成 Role；Human 投递属于 Notification。

### DEV-02-09

- 必须等待 DEV-02-07、DEV-02-08。
- 重点：Human recipient、Event、Message、Notification、AttentionItem 的引用边界和去重。

### DEV-02-10

- 重点：资源、动作、路径、有效期、审批人、scope、Human-only；默认拒绝。
- 不能只靠 UI 隐藏按钮表达权限。

### DEV-03-03

- 只实现 initialize、协议版本/capabilities 协商和 health check。
- 不实现完整 Run 状态机或故障注入。
- 不兼容版本、缺能力、未认证、离线必须返回冻结错误分类并留痕。
- 输入先执行 Adapter V0 原始行大小、JSON-RPC 和 Schema 校验。

### DEV-03-04

- Mock Adapter 正常流：`start → heartbeat → output → completed`。
- 顺序和 sequence 可重复；大输出只返回 Artifact reference。
- 不能让 Mock 直写数据库或共享 Workspace。

### DEV-03-05

- 故障注入：超时、重复、乱序、崩溃、晚到回调、拒绝取消、失联。
- 必须保证测试有 deadline/watchdog，不能让故障注入本身挂死 CI。
- 本节点不应自行发明新的协议方法或错误码。

## 9. 每个节点的强制工作流

1. 读取 `AGENTS.md`、`AGENTS.override.md` 和四份开发基线。
2. 读取 `docs/specs/README.md` 及该节点关联的全部冻结规格。
3. 在审核记录中写明节点 ID、实际模型、难度理由、允许文件范围和验收命令。
4. 先写失败测试或明确可复现反例，再做最小实现。
5. 不修改已发布 migration；只新增前向 migration。
6. 实现者完成后，审核者只读复核并追加 review 小节。
7. 节点定向门禁通过后进入 `REVIEW`，不是立即 `DONE`。
8. 本批所有节点审核通过后，运行完整 21 项 macOS + Ubuntu 门禁、历史 Gitleaks 与证据终审。
9. 只有专项标准和通用 G0～G9 都通过，才标记 `DONE` 并启动依赖节点。

每批完整门禁至少包括：格式、TypeScript 类型、ESLint、Rust Clippy、全部测试、全部构建、秘密扫描、npm/Rust 漏洞、许可证、Rust unsafe、生成漂移和双环境输入哈希一致性。

## 10. 费用、GitHub 与外部服务规则

- 新增费用上限严格为 `$0`。
- 不运行 GitHub-hosted Actions，不购买 minutes，不开通付费 runner。
- 仓库保持私有没有问题；无需为了 CI 开源。
- GitHub workflow、Dependabot 和 `ci / required checks` 配置保留，但当前不依赖服务端 branch protection。
- 不使用浏览器执行 GitHub 操作；如需只读仓库信息，优先 CLI/API，但本 Wave 2 不需要推送或 PR。
- 不发送外部消息、不部署、不接触生产数据。

## 11. 完成 Wave 2 的停止条件

只有下列节点全部 `DONE` 才算 Wave 2 完成：

```text
DEV-01-05
DEV-01-08
DEV-01-09
DEV-02-02
DEV-02-03
DEV-02-04
DEV-02-05
DEV-02-06
DEV-02-07
DEV-02-08
DEV-02-09
DEV-02-10
DEV-03-02
DEV-03-03
DEV-03-04
DEV-03-05
```

全部完成后：

1. 生成 Wave 2 总审核报告，列出每个节点状态、实际模型、审核结论、测试数、证据路径、风险和回退方式。
2. 再跑一次最终 macOS ARM64 + Ubuntu ARM64 21 项门禁和全 refs Gitleaks history scan。
3. 由独立审核者核对最终输入 hash、隔离参数、`sing-box` 和所有节点记录。
4. 停止；不得启动 DEV-02-11、DEV-03-06 或任何 Wave 3 节点。
5. 不提交、不推送、不部署，等待 Human 明确决定。

## 12. 当前遗留风险

- 本交接文件使旧的 155 文件输入 hash 失效；必须重跑最终双环境证据。
- Oracle 当前批次虽 21/21 PASS，但运行后 evidence package 尚未补齐 docker logs、after inspect、network after、sing-box after、工具来源复用说明、`SHA256SUMS` 和批次汇总。
- 本地真实 Git 全 refs Gitleaks history scan 尚未针对当前批次重新生成并绑定输入 hash。
- DEV-02-02/DEV-03-02 尚未进行包含双环境证据的最终独立批次终审，因此必须保持 `REVIEW`。
- `cargo audit` 的 7 条既有 warning 主要来自 Tauri/GTK 依赖；当前没有 vulnerability，但升级依赖时需重新评估。
- SQLx `_rt-tokio` 是内部 feature；只在精确锁版本下接受。
- DEV-03-02 保证直接子进程回收和 reader 有界退出，不声称跨平台递归杀死所有 Adapter 后代；Adapter 仍须遵守 EOF/shutdown 后清理自身后代的冻结契约。
