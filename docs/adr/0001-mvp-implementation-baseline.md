# ADR-0001：MVP 实施基线

- 状态：Accepted
- 日期：2026-09-21
- 决策者：Human Owner（2026-09-21 明确确认）
- 适用：AI Team Workspace local-first MVP

> 平台门禁更新：第 10 项中 Ubuntu 同步验证的发布前要求，已由 Human 接受的 [ADR-0005](0005-macos-first-quality-and-release-gates.md) 替代；macOS 首发目标不变。

## 背景

PRD 要求本地优先、可替换 Harness、并发隔离、权限门禁、重启恢复和完整审计。Figma 已覆盖核心状态，但审计明确要求在开发前冻结进程形态、持久化、安全存储、Adapter 契约、数据语义和平台优先级。

## 决策

1. 使用 **Tauri 2 + React 19 + TypeScript** 构建桌面应用；Rust 负责领域控制面、持久化、工作区与进程监管。
2. 采用**模块化单体**。领域与应用层不依赖 Tauri，使未来可替换为本地服务或远程控制面，但 MVP 不提前拆微服务。
3. 使用 **SQLite + SQLx**；所有业务写入在事务内完成，并同时写入 append-only Event/Outbox。
4. UI 与控制面使用类型化 Tauri IPC；DTO 由 Rust Schema 生成 TypeScript，禁止双份手写协议。
5. Adapter 使用 **JSON-RPC 2.0 over stdio**，manifest 和 payload 用版本化 JSON Schema；适配器不得直写核心数据库。
6. Git Workspace 每个有效 Run 使用独立 branch/worktree；非 Git Workspace 采用单写者锁。
7. 凭据保存在系统 Keychain/Credential Manager/Secret Service，领域数据只保存 `credential_ref`。
8. `Message/Thread` 只面向稳定 Role；Human 接收使用独立 `Notification`，两者均可由 Event 触发并互相引用。
9. 内部 ID 使用 UUIDv7；时间统一 UTC/RFC 3339；核心记录不硬删除；Event、Run、Binding 历史追加写。
10. macOS 14+ 是首个完整验收平台；核心同时在 Ubuntu CI 验证；Windows 适配在不改变领域协议的前提下后续补齐。
11. 先交付 Mock Adapter 契约闭环，再接 Codex；第二真实 Harness 由 Spike 数据决定。
12. `TODO` 和 `READY` 是不同状态；UI 可聚合展示，但不能合并权限和领取规则。

## 主要后果

### 正向

- Rust 同时覆盖本地安全边界、进程监管、文件系统和领域一致性，减少跨进程状态分叉。
- 模块化单体降低 MVP 运维复杂度，同时保留领域层迁移空间。
- 标准化 stdio 协议让 Adapter 与桌面实现解耦，也便于独立契约测试。
- SQLite 事务、Outbox、版本号、租约和 fencing token 能在单机形态下提供足够强的一致性。

### 代价

- 团队需要同时维护 Rust 与 TypeScript 两套工程能力。
- Tauri IPC 和本地文件系统行为必须用真实桌面端 E2E 验证，纯浏览器测试不足。
- macOS 首发意味着 Windows 进程、路径与凭据差异不能在 P0 宣称已验证。

## 被否决方案

- Electron + Node 全栈：进程与文件控制开发较快，但默认攻击面、内存和系统权限边界不符合本项目本地安全优先级。
- 前后端独立 HTTP 微服务：对单机 MVP 增加端口、认证、生命周期和升级复杂度，收益不足。
- 直接让 UI/Adapter 写 SQLite：破坏统一权限、状态机、审计和幂等边界。
- 以 Harness Session 作为上下文：违反 Workspace 是事实源和 Harness 可替换原则。

## 复审触发条件

出现以下任一情况必须复审本 ADR：需要远程多节点控制面、单库规模超过已验证上限、目标 Harness 无法可靠使用 stdio 适配、Windows 成为首发平台、或 Tauri/Rust 无法满足签名与分发要求。

## DEV-00 复核

2026-09-21 已依据需求追踪、领域模型、状态机、权限、数据约定、错误码、Adapter V0 和信息架构完成 Gate A 复核，结论 PASS。记录见 `docs/reviews/DEV-00-FREEZE-REVIEW.md`。
