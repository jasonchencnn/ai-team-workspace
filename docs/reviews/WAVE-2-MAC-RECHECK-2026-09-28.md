# Wave 2 Mac 门禁复查与安全公告处置（2026-09-28）

> 状态：最终输入待复跑和独立终审；此记录不提前将 DEV-02-02 或 DEV-03-02 标为 `DONE`。首轮失败事实仍以 [WAVE-2-MAC-ACCEPTANCE](WAVE-2-MAC-ACCEPTANCE.md) 为准，前置修复见 [WAVE-2-PREBLOCKER-REPAIR](WAVE-2-PREBLOCKER-REPAIR.md)。

## 只读官方查询与同名 crate 阻断

Human 已明确允许官方安全源的只读查询。`pnpm audit --audit-level=high` 使用 `https://registry.npmjs.org`，官方结果为 high/critical 0、moderate 1、low 1。`cargo audit` 从 `https://github.com/RustSec/advisory-db.git` 更新到 commit `e2111519ba6d14a5da59a7b2e5c8083ae8a37c01`，共 1271 条公告。

首次在线 Rust 审计把本仓库未发布的 `domain 0.0.0` 误匹配到 crates.io 的同名 DNS 库 `RUSTSEC-2026-0310`。本地 package 在 `crates/domain/Cargo.toml` 中 `publish = false`，`Cargo.lock` 的该条无 registry source；公告修复版本 `>=0.12.3` 属于另一项目。总控把本地 package 更名为 `teamspace-domain`，同步两份集成测试导入并串行更新锁文件；没有忽略或豁免该公告。`cargo test -p teamspace-domain --locked --offline` 为 17/17，Clippy 与 fmt 通过；后续 Rust 审计漏洞数为 0。

一次完整脚本记录曾为 21/21 退出零，运行前后输入 SHA-256 均为 `88f20dc60d652d2fe826e273622c5c4099c9dad60a249fd4121a7be8862fd2e7`。独立 Terra 复核发现该次 `cargo audit` 虽返回零，附带 crates.io yanked 状态查询超时，且缺工具来源记录，因此该次**不能签 Gate PASS**。后续采用官方 RustSec Git fetch + `CARGO_NET_OFFLINE=true`：只关闭 Cargo registry 的非门禁 yanked 查询；`cargo audit --format json` 在这一配置下 stderr 为空、漏洞数 0，RustSec 公告库仍更新至上述官方 commit。最终完整 21 项脚本必须在此配置下对新的稳定输入重跑并由独立审核者复核。

## 已知公告与处置期限

RustSec JSON 报告为 0 个已知漏洞，另有 6 条 unmaintained、1 条 unsound informational warning；没有关闭扫描或加入 ignore。`proc-macro-error` 的 `RUSTSEC-2024-0370` 与 `glib` 的 `RUSTSEC-2024-0429` 只由 Linux/GTK 依赖引入，`cargo tree --target aarch64-apple-darwin -i` 对两者均无 Mac 路径。`unic-char-property` (`RUSTSEC-2025-0081`)、`unic-char-range` (`RUSTSEC-2025-0075`)、`unic-common` (`RUSTSEC-2025-0080`)、`unic-ucd-ident` (`RUSTSEC-2025-0100`)、`unic-ucd-version` (`RUSTSEC-2025-0098`) 由 Tauri 的 `urlpattern` 链引入 Mac 构建，属于无人维护告警，当前没有对应漏洞公告。总控负责在 **2026-10-28 前**复查上游替换/升级；若 Gate I 前仍在，需重新评估风险，不得把今日结论当成永久豁免。Linux 不在当前验证范围。

官方 npm 全量审计的 moderate 是开发工具链的 `ajv` `$data` ReDoS（`GHSA-2g4f-4pwh-qvx6`）；low 是 `esbuild` Windows 开发服务器任意读（`GHSA-g7r4-m6w7-qqqr`）。当前 Mac 门禁阈值为 high；这两项不阻断本轮，但总控在 2026-10-28 前复查可兼容升级，Gate I 前重新审计。完整 JSON、锁文件哈希与退出码存于仓库外的本轮受限证据包。

## 工具来源与最终复核条件

门禁固定 Node `24.21.0`、pnpm `10.33.0`、Rust/Cargo `1.98.0`、Gitleaks `8.30.0`、cargo-audit `0.22.0`、cargo-deny `0.18.6`。Node 来自本机 Homebrew `node@24/24.21.0`，pnpm 为 `/opt/homebrew/lib/node_modules/pnpm` 中同版本包，Rust/Cargo 由 `rustup` 固定 toolchain 提供。Gitleaks 现用二进制 SHA-256 为 `095bc4be1624239f52ebb90ff91668d1d3e78e0f8c5286e2d981887905c27561`；从其官方 `v8.30.0` release 下载的 `darwin_arm64` archive SHA-256 与官方 checksums 文件同为 `b251ab2bcd4cd8ba9e56ff37698c033ebf38582b477d21ebd86586d927cf87e7`，archive 内二进制 SHA-256 与现用文件逐字节一致。cargo-audit 与 cargo-deny 现用二进制 SHA-256 分别为 `0be59786f032871e95a1bf1c40f4c24f51b1bfddbfe7dee5a307b1acd422253d`、`c76b6b064ea5235f0a08727159d252f2ddf3e1db5b09a0d17afe7f5da1ebc5b2`；本机 crates.io index 的 `0.22.0`/`0.18.6` checksum 与对应缓存 `.crate` 均一致。最终证据还须保留这些绝对路径、版本和哈希，不以 PATH 名称推定来源。

关闭前须在当前稳定输入上取得：Mac 21/21、输入前后总哈希一致、官方 npm 和 RustSec JSON 结果、无 yanked 查询超时、全 refs Gitleaks 零命中、工具来源与文件哈希、独立 Terra 复核。受限证据保存在仓库外，避免验收输出反向改变被验收输入。当前没有提交、推送、部署或生产数据操作。
