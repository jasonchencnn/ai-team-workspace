# DEV-02-09 最小 Owner 身份决策材料

状态：**Human 已于 2026-10-10 接受 ADR-0007 第 1 条身份方案；其余六条仍待决**。之前完成的 [限定存储整改与验收](../reviews/DEV-02-09-SCOPE-REVIEW-2026-10-10.md)保留；当前接续见 [Owner 身份计划](DEV-02-09-OWNER-PLAN-2026-10-10.md)，节点在最终验收前仍 BLOCKED。下列待确认表述保留为确认前材料。

## 已确认事实与待决范围

冻结 DOMAIN_MODEL 明确 Human 是自然人、P0 只有 Owner；通知 recipient 必须是 Human。0023 能拒绝 recipient 与 Role ID 碰撞，却接受任意非 Role UUID，不能证明身份。当前 `apps/desktop/src-tauri/src/lib.rs` 只有 Tauri Builder，`crates/security/src/lib.rs` 只有 unsafe 禁令；还没有安装 Owner 的可信来源与通知执行点。

需要 Human 确认的最小方案是 **ADR-0007 第 1 条**：P0 通知与 Policy 均采用同一个可信安装 Owner，由主程序身份端口提供。不能从 IPC/Adapter 的 owner_id、布尔 allow、UUID 形状、岗位名或“不是 Role”推断。完整第 2～7 条继续分别待决，不把本次身份确认扩展成整体接受。

## 确认后的精确执行计划

1. 定位并明确主程序安装身份的真实持久来源和可信调用边界。若不存在，先提交具体来源/初始化/重启稳定性方案；不使用随机 fixture 或系统用户名冒充已验证的自然人身份。
2. 实现封闭的受信身份端口：Notification recipient 与 Policy recipient 都来自它，不允许普通调用者指定另一个收件人，不复制一套可任意设置的 Human 身份集合。
3. 补真实 SQLite/主程序边界正负测试：Owner 正常投递、任意 UUID 与 Role 伪装被拒、身份失效、重启身份稳定；拒绝的业务写入零副作用并记录适用审计。保持角色排除为附加防御。
4. 复核 09 专属身份标准与 G0～G9，按必要变更签新前向迁移/互斥写集；最终完整 Mac21 后再决定是否重签 09。一般通知策略执行仍按 05-08 依赖准入，不借此启动全部后续节点。

限制：P0 仅已有应用内能力，不新增邮件/IM/推送/付费服务，不访问生产凭据或真实账号，不扩大 Harness/Role 权限，不改变状态/错误码/协议集合，新增费用为 0。授权身份方案不替代测试与正式节点验收。

确认依据来自 [AGENTS.override.md](../../AGENTS.override.md) 的“技术栈、领域不变量、协议边界和质量门禁已经冻结。变更必须新增或替代 ADR，并获得 Human 明确确认”，以及 [ADR-0007](../adr/0007-notification-resolution-and-policy-semantics-proposal.md) 当前明确的 Proposed 状态。Human 对 ADR-0009 的接受和一般“制定计划开始执行”不自动接受该身份方案。已冻结规则下的存储修复没有等待此确认，已执行完毕。
