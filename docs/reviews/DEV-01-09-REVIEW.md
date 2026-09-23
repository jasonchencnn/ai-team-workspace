# DEV-01-09 开发 fixture、时钟/ID/随机源注入

- 执行模型：`gpt-5.6-terra`（节点指定模型）；总控完成批次级复核。
- 关联要求：工程 fixture 基础设施；`DATA_CONVENTIONS.md` 的 UUIDv7、UTC 与领域服务时间源注入约束。
- 范围与非目标：在 `domain` 定义时间、随机和 UUIDv7 来源端口，提供无系统时间、用户目录、网络或在线服务依赖的确定性 fixture。不实现生产 OS 适配器，不改变领域状态、权限、协议、migration 或 `sources/`。
- 变更文件：`crates/domain/src/lib.rs`、`crates/domain/src/sources.rs`、`crates/domain/src/test_fixtures.rs`、`crates/domain/tests/injected_sources.rs`。
- 领域与数据影响：不新增聚合、状态、错误码、协议或 migration；UUIDv7 和 UTC 值继续由既有值对象校验。

## TDD 与边界证据

- RED：`cargo test -p domain --test injected_sources --locked` 因 `Clock`、`RandomSource`、`UuidV7Source`、`UuidV7Generator` 和 fixture 尚未定义而编译失败。
- GREEN：同一命令通过 5/5。
- 失败/边界：固定时钟可重置；随机与 UUID 序列自然耗尽和显式耗尽均返回稳定错误；随机源耗尽及 pre-epoch 时间以结构化生成器错误返回，绝不回退系统来源。

## 验证结果

| 门禁                                                                            | 结果                                                                                                                |
| ------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------- |
| `cargo test -p domain --test injected_sources --locked`                         | PASS：5/5。                                                                                                         |
| `cargo test -p domain --locked`                                                 | PASS：17/17。                                                                                                       |
| `cargo clippy -p domain --all-targets --all-features --locked -- -D warnings`   | PASS。                                                                                                              |
| `pnpm run fmt:check`、`pnpm run typecheck`、串行 Lint、`pnpm run verify:guards` | PASS。之前由并发写集产生的临时格式/Lint 阻断已解除。                                                                |
| `pnpm run test`、`pnpm run build`、`pnpm run contracts:check`                   | PASS：TS 35/35，构建与生成漂移检查通过。                                                                            |
| Cargo workspace locked test/build/clippy/metadata                               | PASS：Rust 25/25，零 Clippy warning，锁文件解析成功。                                                               |
| 依赖与许可证扫描                                                                | PASS：npm high/critical 为 0；cargo-audit 无 failure-class vulnerability；Node 280 个传递包和 Rust 许可证检查通过。 |
| `git diff --check`                                                              | PASS。                                                                                                              |

- 视觉/无障碍：不适用，无 UI 变更。
- 迁移与回退：无 migration 或数据写入；移除新增端口/fixture 与导出即可回退。
- 遗留风险：生产系统时钟和密码学安全随机适配器必须由后续 host/infrastructure 节点显式接入；domain 不得自行访问 OS 来源。
- 审核结论：**PASS**；节点可标记 **DONE**。
