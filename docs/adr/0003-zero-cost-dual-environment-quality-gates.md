# ADR-0003：私有阶段使用零费用双环境质量门禁

- 状态：Accepted
- 日期：2026-09-23
- 决策者：Human Owner（2026-09-23 明确批准）
- 适用：私有、零 GitHub Actions 预算阶段的 DEV-01-05 与后续 Wave 2 节点

> 历史决策：本 ADR 的双环境放行要求自 2026-09-28 起由 Human 接受的 [ADR-0005](0005-macos-first-quality-and-release-gates.md) 替代。以下内容保留原有执行与证据标准，不能作为当前节点的 Ubuntu 阻断条件；零新增费用和安全约束仍适用。

## 背景

冻结 Gate B 要求 macOS 与 Ubuntu CI required checks 全绿。当前私有 GitHub 仓库的 Actions 预算为零，GitHub-hosted job 被拒绝启动；Human 明确要求不得产生 GitHub 费用。已安装的 GitHub 集成可以读取私有仓库、PR 与已有 CI，但读取 branch protection 返回 `403`，也没有配置 required checks 的管理接口。

项目已建立不依赖 GitHub-hosted runner 的统一门禁脚本。关键 lockfile、workflow、扫描策略和门禁脚本 SHA-256 相同的工作区快照已经在本机 macOS ARM64 与 Oracle Cloud Ubuntu ARM64 隔离环境中通过格式、Lint、类型、测试、构建、Clippy、生成漂移、秘密、漏洞、许可证与 Rust unsafe 共 21 条门禁命令；本地真实 Git 仓库另以 Gitleaks history 模式扫描全部 4 个提交，无泄露。当前证据尚未包含完整输入快照总哈希、工具来源校验和与完整隔离证明，因此只能作为替代方案的预验证，不能在本 ADR 获批时直接把节点标为 `DONE`。

若继续强制 hosted required checks，则 DEV-01-05 会因费用政策而长期保持 `REVIEW`，Wave 2 无法按依赖继续。直接把本地绿灯称为原 Gate B 通过则会违反冻结基线，因此需要 Human 明确接受“无服务端强制、当前仅覆盖 ARM64”的差异，并批准下列零费用替代门禁及其补强条件。

## 决策

在仓库保持私有且 GitHub Actions 零预算期间：

1. Gate B 中“macOS 与 Ubuntu CI required checks 全绿”临时替换为“同一规范化输入快照在 macOS 与 Ubuntu 两个独立环境的受控门禁全部通过”。Human 同时明确接受：没有 branch protection 服务端强制，且当前平台覆盖为 macOS ARM64 + Ubuntu ARM64，而不是 Ubuntu x86_64。
2. 两个环境必须运行仓库内同一版本的 `scripts/ci/local-gates.mjs`，并使用冻结版本的 Node、pnpm、Rust、Gitleaks、cargo-audit 与 cargo-deny。脚本必须强制校验版本，不得只记录版本，也不得手工删减命令。
3. 门禁运行前必须生成完整、规范化的输入清单和总 SHA-256：覆盖全部受版本控制与待纳入实现的源码、配置、冻结规格和未跟踪输入；排除 `.git`、证据输出、依赖缓存和构建产物。任何文件读取、清单或哈希失败都必须阻断，不得继续产生 `pass`。
4. 两份 JSON 证据必须同时满足：`result=pass`、全部门禁与身份采集命令退出码为 0、完整输入清单总哈希一致、系统和工具版本可识别。证据必须排他创建并保留自身校验和。
5. 工具来源证明必须与证据一起保存：下载型工具记录官方发布 URL、版本和 SHA-256；Rust 工具记录 crate/version/lock；Ubuntu 记录基础镜像 digest、已安装 APT 包版本和镜像 ID。来源或校验失败立即阻断。
6. macOS 负责首发平台实机验证；Ubuntu 在 Oracle ARM 隔离容器中运行，必须使用非 root 用户、资源限制、`cap-drop=ALL`、`no-new-privileges`，且不得挂载 Docker socket、生产目录、生产卷或生产网络。镜像 inspect、mount、network、limits 和 `sing-box` 前后状态必须形成独立可校验文件并纳入证据包。
7. 文件快照扫描之外，真实 Git 仓库必须证明不是 shallow clone，记录 HEAD 与全部 refs，并使用相同 Gitleaks 配置对 `--all` 可达历史执行扫描；命令、范围、配置哈希、退出码与日志哈希必须绑定到同一证据包。任何命中都阻断节点。
8. 总控继续承担 required-check 的治理职责：任一门禁失败、证据缺失、清单/哈希不一致、身份采集失败、工具来源异常或独立审核未通过时，节点保持 `REVIEW/BLOCKED`，不得启动依赖节点。
9. 任何源码、未跟踪输入、锁文件、门禁脚本、workflow、策略、工具版本、工具来源或安全公告库变化都会使旧证据失效；失效后必须重跑受影响平台，不能沿用旧绿灯。
10. 每批仍需按改动风险重跑格式、类型、Lint、测试、构建、安全与生成漂移检查，并在节点审核记录中写明平台、工具版本、证据位置和遗留风险。
11. 本 ADR 只授权使用 Human 已提供、确认零新增费用的本机和 Oracle 实例。最高新增费用为 `$0`；不自动授权购买 GitHub Actions、Oracle 额外资源、镜像服务、网络套餐或其他收费服务。环境或费用条件变化时必须重新获得 Human 明确授权。
12. 本替代不授权提交、推送、部署、修改 `sources/`、访问生产凭据或操作生产数据。

## 适用边界

- 本 ADR 保留全部测试、安全、许可证、生成漂移和节点依赖标准，但由 Human 明确接受两项能力差异：无 branch protection 服务端强制、无 Ubuntu x86_64 证据。
- 当前证据覆盖 macOS ARM64 与 Ubuntu ARM64，不得描述为 hosted runner 或 x86_64 验证。
- 本地脚本的 `--no-git` 文件扫描不能代替历史扫描；必须执行第 7 条的独立全 refs history scan。
- GitHub workflow、Dependabot 与聚合项 `ci / required checks` 继续保留，便于未来恢复自动化。

## 主要后果

### 正向

- 不产生 GitHub Actions 费用，也不需要公开仓库。
- macOS 首发环境与独立 Ubuntu ARM 环境都执行完整门禁，仍保留跨平台故障发现能力。
- JSON 证据、日志与 SHA-256 让结果可复核，失败不能被口头覆盖。

### 代价与风险

- 没有 GitHub branch protection 的服务端强制，依赖总控严格维护节点状态。
- 不是 GitHub-hosted runner，不能证明 GitHub Action 本身、缓存行为或 hosted 镜像完全可用。
- 现有 Oracle 镜像尚未固定全部基础镜像/APT/download digest；在补齐第 5 条并重跑前，现有绿灯只能作为预验证。
- 现有证据只记录关键文件哈希；在补齐第 3、4、7 条并重跑前，不能把当前证据当作本 ADR 的最终验收证据。

## 恢复 hosted required checks

只有以下硬条件全部满足后，才能恢复原 Gate B：

1. Human 确认恢复过程与后续运行新增费用为 `$0`，或另行明确批准具体费用；
2. macOS 与 Ubuntu hosted runner 的全部 workflow job 实际全绿；
3. `ci / required checks` 已绑定为 branch-protection required check，且集成可以读回设置与绿灯；
4. 独立审核确认远端配置、权限和证据有效；
5. 节点审核记录写明恢复日期与远端证据，并保留双环境本地回放作为灾备。

任一条件不满足时，继续使用已批准且证据有效的替代门禁；若替代证据也无效，则节点保持 `REVIEW/BLOCKED`。

## 验证标准

1. macOS 与 Ubuntu 证据均为 `pass`，21 条门禁命令和全部身份采集命令无失败，完整输入清单总 SHA-256 一致。
2. macOS 与 Ubuntu 的格式、Lint、类型、TS/Rust 测试、构建、Clippy、生成漂移、秘密、漏洞、许可证和 Rust unsafe 检查全部通过。
3. 冻结工具版本被脚本强制校验，工具来源与 SHA-256/lock、Ubuntu 镜像 digest 和 APT 包版本均有证明。
4. 非 shallow 的本地 Git 仓库对全部 refs 的 history scan 无秘密命中，范围、HEAD/refs、配置和日志已绑定证据包。
5. Ubuntu 隔离参数、mount/network inspect 和 `sing-box` 前后状态已形成可校验证据。
6. 独立 `gpt-5.6-terra` 审核确认补强项与能力边界，审核记录不把 ARM64、本地环境或文件快照冒充 hosted/x86_64/Git 历史证据。
7. 本 ADR 获批后仍须先实现并重跑第 2 至 7 条；全部通过后 DEV-01-05 才能由 `REVIEW` 改为 `DONE`。

## 补强条件未满足时的行为

本 ADR 虽已获 Human 批准，但在第 2 至 7 条补强条件全部实现并重跑、证据可校验且独立复核通过前，替代门禁尚未完成验收：`DEV-01-05` 保持 `REVIEW`，第一批不视为全部通过，所有依赖节点继续等待。
