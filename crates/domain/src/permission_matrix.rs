//! §3 command permission matrix: which actor may attempt which action.
//!
//! Frozen source: `docs/specs/PERMISSION_MATRIX.md` §1 and §3; the matrix is
//! transcribed cell by cell and checked mechanically by
//! `q08-permission-matrix-canonical.py` (parse) and
//! `q09-matrix-vs-spec.py` (Rust-vs-spec diff).
//!
//! # This table is one layer, not the decision
//!
//! §2 makes effective permission an intersection of eight layers, and §3 is only
//! the Role/Binding one. A cell here never grants anything by itself: the
//! application layer resolves every other layer and
//! [`crate::permission_intersection`] intersects them. Reading a `A` out of this
//! table as "allowed" is the mistake this module's doc comment exists to prevent.
//!
//! # Four cells print a symbol §3's own legend does not define
//!
//! | Row | Column | Cell |
//! |---|---|---|
//! | `Task 设置依赖/软依赖` | Owner | `A/H` |
//! | `Task 设置依赖/软依赖` | Leader | `A/S` |
//! | `Review 通过/退回` | QA | `A/S` |
//! | `Workspace 写代码任务区` | QA | `R/S` |
//!
//! The legend defines only `A`, `S`, `R`, `H` and `—`. These four cells print two
//! symbols because **the verdict depends on a fact the table does not carry** —
//! §3's own "Harness 额外条件" column names it: "Human 可越过硬依赖",
//! "Reviewer 不能审批自己未获授权的高风险结果", "Task 授权路径".
//!
//! They are modelled as [`MatrixVerdict::ContextDependent`], which **cannot
//! resolve to Allow on its own**. §2 step 7 is 默认拒绝, so a caller that does not
//! supply the discriminating fact gets a denial, not a guess. Normalising `A/H` to
//! either `A` or `H` would pick a winner the specification deliberately withheld.
//!
//! # Action names are derived; the §3 label is frozen
//!
//! §3 labels each row in prose ("Task claim"), while `permissions.action` in
//! migration `0009` already froze nine stable action codes and eleven
//! `resource_type` codes. **The specification gives no key mapping §3's rows onto
//! that 9×11 vocabulary** — rows 24/25/26 all land on `(UPDATE, WORKSPACE)` yet
//! read `S` / `S` / `H`, because they differ by path, which §2 routes to the
//! WorkspacePathPolicy layer. So the mapping is not invented here; the
//! [`PermissionAction`] variants are **derived** identifiers and
//! [`PermissionAction::spec_label`] returns §3's label verbatim for audit.
//! Registered as a leftover, not filled in.

use std::fmt;

use serde::{Deserialize, Serialize};

/// The subject doing the work, as §3's six columns see it.
///
/// §1 also lists Human Admin and Adapter/Harness. Human Admin is "P0 不开放邀请"
/// and so is not a §3 column. Harness "不作为权限主体持久化" — it is carried as a
/// ROLE_BINDING actor with a `harness_instance_id`, and §3 gives it a separate
/// "Harness 额外条件" column rather than a verdict column. Neither is a seventh
/// matrix column, so neither is here.
/// No `serde` derive: these six are §3's matrix **columns**, and no frozen
/// document gives them a wire spelling. The Event's `actor_kind` field is a
/// different vocabulary — its only frozen value is `ROLE_BINDING` (migration
/// 0015's CHECK constraint) — and §3 deliberately has no column for it, since
/// §1 says Harness "不作为权限主体持久化". Deriving serde here would assert a
/// relationship between the two that no specification makes. The contractual
/// name is [`ActorKind::column_name`], which returns §3's header verbatim.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ActorKind {
    /// §3 `Owner`: Human Owner, the only Human role.
    HumanOwner,
    /// §3 `Leader`: a team-management post represented by a bound Harness.
    RoleLeader,
    RoleProduct,
    RoleDeveloper,
    RoleQa,
    /// §3 `System`: timers, Outbox, recovery, policy execution.
    System,
}

impl ActorKind {
    /// The six §3 columns, in table order.
    pub const ALL: [ActorKind; 6] = [
        ActorKind::HumanOwner,
        ActorKind::RoleLeader,
        ActorKind::RoleProduct,
        ActorKind::RoleDeveloper,
        ActorKind::RoleQa,
        ActorKind::System,
    ];

    /// The §3 column header this variant corresponds to.
    pub const fn column_name(self) -> &'static str {
        match self {
            ActorKind::HumanOwner => "Owner",
            ActorKind::RoleLeader => "Leader",
            ActorKind::RoleProduct => "Product",
            ActorKind::RoleDeveloper => "Developer",
            ActorKind::RoleQa => "QA",
            ActorKind::System => "System",
        }
    }

    /// Column index, so the flat `MATRIX` rows can be indexed by this.
    pub const fn index(self) -> usize {
        match self {
            ActorKind::HumanOwner => 0,
            ActorKind::RoleLeader => 1,
            ActorKind::RoleProduct => 2,
            ActorKind::RoleDeveloper => 3,
            ActorKind::RoleQa => 4,
            ActorKind::System => 5,
        }
    }

    /// `true` for the two Human posts. §5: "'Leader' 是 AI Role 时不视为 Human；
    /// 岗位名不能提升权限" — so `RoleLeader` is **not** Human despite the job title.
    pub const fn is_human(self) -> bool {
        matches!(self, ActorKind::HumanOwner)
    }

    /// `const`-callable equality. A derived `PartialEq` is not `const`, so a
    /// `const fn` in [`crate::permission_intersection`] cannot use `==` on this
    /// type; this is the same pattern the state-machine discriminants use.
    pub const fn same_as(self, other: Self) -> bool {
        self.index() == other.index()
    }
}

impl fmt::Display for ActorKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.column_name())
    }
}

/// A verdict the matrix can state outright.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum SimpleVerdict {
    /// §3 `A` 允许.
    Allow,
    /// §3 `S` 限作用域/状态. §3: its exact scope comes from Role templates, Task
    /// grants, Workspace path policy and command state preconditions, and
    /// "不能由前端自行解释".
    Scoped,
    /// §3 `R` 仅只读.
    ReadOnly,
    /// §3 `H` Human-only. §5 freezes eight categories that can never be delegated.
    HumanOnly,
    /// §3 `—` 默认拒绝.
    Deny,
}

impl SimpleVerdict {
    /// `true` for the three verdicts that permit the attempt at all.
    pub const fn permits_attempt(self) -> bool {
        matches!(
            self,
            SimpleVerdict::Allow | SimpleVerdict::Scoped | SimpleVerdict::ReadOnly
        )
    }

    /// The §3 symbol, for round-tripping against the spec table.
    pub const fn symbol(self) -> &'static str {
        match self {
            SimpleVerdict::Allow => "A",
            SimpleVerdict::Scoped => "S",
            SimpleVerdict::ReadOnly => "R",
            SimpleVerdict::HumanOnly => "H",
            SimpleVerdict::Deny => "—",
        }
    }
}

impl fmt::Display for SimpleVerdict {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.symbol())
    }
}

/// The fact a §3 compound cell depends on, named from that row's own
/// "Harness 额外条件" column.
///
/// `Task 设置依赖/软依赖` carries **two** compound cells and both turn on the same
/// fact, so one variant covers them: §5's frozen list puts "绕过硬依赖" in the
/// Human-only categories, and §3's extra-condition column for that row says
/// "Human 可越过硬依赖".
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum DiscriminatingFact {
    /// `Task 设置依赖/软依赖` + Owner `A/H` and + Leader `A/S`. §3: "软依赖覆盖原因
    /// 必填；Human 可越过硬依赖".
    HardDependencyBypass,
    /// `Review 通过/退回` + QA `A/S`. §3: "Reviewer 不能审批自己未获授权的高风险
    /// 结果".
    ReviewerAuthorizedForHighRiskResult,
    /// `Workspace 写代码任务区` + QA `R/S`. §3: "Task 授权路径 + 有效 fencing；
    /// QA 默认只读代码".
    TaskAuthorizedCodePath,
}

impl DiscriminatingFact {
    /// The stable condition key for the catalogue's `condition_keys` detail.
    pub const fn condition_key(self) -> &'static str {
        match self {
            DiscriminatingFact::HardDependencyBypass => "hard_dependency_bypass",
            DiscriminatingFact::ReviewerAuthorizedForHighRiskResult => {
                "reviewer_authorized_for_high_risk_result"
            }
            DiscriminatingFact::TaskAuthorizedCodePath => "task_authorized_code_path",
        }
    }
}

/// The two readings of a compound cell, **in the order §3 printed them**.
///
/// # The polarity is deliberately not encoded
///
/// Reading `A/H`, `A/S` and `R/S` as "default / exception" is tempting and
/// probably right — §3's extra-condition column names an exception in all four
/// cells, and it is the second symbol. But §3 never says so, and "probably right"
/// is exactly how a derived judgement turns into an invented semantic.
///
/// So the pair stores the two symbols positionally and makes the **caller** state
/// which one the proven fact selects. `a_compound_cell_never_resolves_to_allow`
/// in the test suite holds the line that matters: a caller who cannot prove the
/// fact gets a denial, not a guess.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct VerdictPair {
    /// §3's first symbol, verbatim.
    pub first: SimpleVerdict,
    /// §3's second symbol, verbatim.
    pub second: SimpleVerdict,
    /// Which fact decides, named from §3's own extra-condition column.
    pub decided_by: DiscriminatingFact,
}

impl VerdictPair {
    /// `true` when the caller proved the fact and it selects §3's **first**
    /// symbol; `false` selects the second.
    ///
    /// This is a caller assertion, not a fact the type can check. A caller that
    /// has not checked must not call this — see
    /// [`crate::permission_intersection`]'s default-deny.
    pub const fn resolve(self, first_symbol_applies: bool) -> SimpleVerdict {
        if first_symbol_applies {
            self.first
        } else {
            self.second
        }
    }

    /// The two symbols as §3 prints them, `A/H` style.
    pub fn spec_symbol(self) -> String {
        format!("{}/{}", self.first.symbol(), self.second.symbol())
    }
}

/// A §3 cell.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum MatrixVerdict {
    /// One of the five legend values.
    Simple(SimpleVerdict),
    /// A cell that printed two symbols (`A/H`, `A/S`, `R/S`) and so cannot be
    /// decided from the table alone. Exactly four cells in §3 are like this.
    ContextDependent(VerdictPair),
}

impl MatrixVerdict {
    /// The §3 symbol as printed, including the `/` form for compound cells.
    pub fn spec_symbol(self) -> String {
        match self {
            MatrixVerdict::Simple(v) => v.symbol().to_string(),
            MatrixVerdict::ContextDependent(p) => p.spec_symbol(),
        }
    }

    /// `true` if this cell cannot be decided without a fact from another layer.
    pub const fn is_context_dependent(self) -> bool {
        matches!(self, MatrixVerdict::ContextDependent(_))
    }
}

impl fmt::Display for MatrixVerdict {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.spec_symbol())
    }
}

/// One row of §3. The `PermissionAction` variants are **derived** identifiers;
/// [`PermissionAction::spec_label`] is the frozen §3 text.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum PermissionAction {
    TeamCreateEdit = 0,
    TeamArchiveRestore = 1,
    RoleManage = 2,
    RoleBindingHistoryRead = 3,
    HarnessInstanceManage = 4,
    HarnessHealthCheck = 5,
    BindingManage = 6,
    CredentialReferenceManage = 7,
    WorkspaceBindReplace = 8,
    TaskCreate = 9,
    TaskAssignReassign = 10,
    TaskEditLightweight = 11,
    TaskSetDependencies = 12,
    TaskClaim = 13,
    TaskBlockOrSubmitReview = 14,
    TaskCancelReopen = 15,
    RunStartHeartbeatFinish = 16,
    RunRetry = 17,
    RunCancel = 18,
    HumanTakeover = 19,
    ReviewSubmit = 20,
    ReviewApproveOrRequestChanges = 21,
    WorkspaceReadShared = 22,
    WorkspaceWritePlans = 23,
    WorkspaceWriteCodeArea = 24,
    WorkspaceWriteProduction = 25,
    ArtifactCreateOrLink = 26,
    MessageSend = 27,
    MessageRetry = 28,
    MessageReassign = 29,
    NotificationPolicyEdit = 30,
    ApprovalRequest = 31,
    ApprovalDecide = 32,
    EventActivityRead = 33,
    AuditExport = 34,
    DeployOrProduction = 35,
}

impl PermissionAction {
    /// All 36 §3 rows, in table order. The order is load-bearing: `MATRIX` is
    /// indexed positionally and must stay in step.
    pub const ALL: [PermissionAction; 36] = [
        PermissionAction::TeamCreateEdit,
        PermissionAction::TeamArchiveRestore,
        PermissionAction::RoleManage,
        PermissionAction::RoleBindingHistoryRead,
        PermissionAction::HarnessInstanceManage,
        PermissionAction::HarnessHealthCheck,
        PermissionAction::BindingManage,
        PermissionAction::CredentialReferenceManage,
        PermissionAction::WorkspaceBindReplace,
        PermissionAction::TaskCreate,
        PermissionAction::TaskAssignReassign,
        PermissionAction::TaskEditLightweight,
        PermissionAction::TaskSetDependencies,
        PermissionAction::TaskClaim,
        PermissionAction::TaskBlockOrSubmitReview,
        PermissionAction::TaskCancelReopen,
        PermissionAction::RunStartHeartbeatFinish,
        PermissionAction::RunRetry,
        PermissionAction::RunCancel,
        PermissionAction::HumanTakeover,
        PermissionAction::ReviewSubmit,
        PermissionAction::ReviewApproveOrRequestChanges,
        PermissionAction::WorkspaceReadShared,
        PermissionAction::WorkspaceWritePlans,
        PermissionAction::WorkspaceWriteCodeArea,
        PermissionAction::WorkspaceWriteProduction,
        PermissionAction::ArtifactCreateOrLink,
        PermissionAction::MessageSend,
        PermissionAction::MessageRetry,
        PermissionAction::MessageReassign,
        PermissionAction::NotificationPolicyEdit,
        PermissionAction::ApprovalRequest,
        PermissionAction::ApprovalDecide,
        PermissionAction::EventActivityRead,
        PermissionAction::AuditExport,
        PermissionAction::DeployOrProduction,
    ];

    /// §3's row label, verbatim. Frozen text; the variant name beside it is not.
    pub const fn spec_label(self) -> &'static str {
        match self {
            PermissionAction::TeamCreateEdit => "Team 创建/编辑",
            PermissionAction::TeamArchiveRestore => "Team 归档/恢复",
            PermissionAction::RoleManage => "Role 创建/编辑/停用/恢复",
            PermissionAction::RoleBindingHistoryRead => "Role/Binding/权限历史读取",
            PermissionAction::HarnessInstanceManage => "Harness Instance 注册/编辑/禁用",
            PermissionAction::HarnessHealthCheck => "Harness 健康检查/诊断",
            PermissionAction::BindingManage => "Binding 绑定/解绑/换绑",
            PermissionAction::CredentialReferenceManage => "CredentialReference 创建/选择/删除",
            PermissionAction::WorkspaceBindReplace => "Workspace 绑定/更换",
            PermissionAction::TaskCreate => "Task 创建",
            PermissionAction::TaskAssignReassign => "Task 分配/改派",
            PermissionAction::TaskEditLightweight => "Task 编辑轻量字段",
            PermissionAction::TaskSetDependencies => "Task 设置依赖/软依赖",
            PermissionAction::TaskClaim => "Task claim",
            PermissionAction::TaskBlockOrSubmitReview => "Task block/submit review",
            PermissionAction::TaskCancelReopen => "Task cancel/reopen",
            PermissionAction::RunStartHeartbeatFinish => "Run start/heartbeat/finish",
            PermissionAction::RunRetry => "Run retry",
            PermissionAction::RunCancel => "Run cancel",
            PermissionAction::HumanTakeover => "Human takeover",
            PermissionAction::ReviewSubmit => "Review 提交",
            PermissionAction::ReviewApproveOrRequestChanges => "Review 通过/退回",
            PermissionAction::WorkspaceReadShared => "Workspace 读取共享资料",
            PermissionAction::WorkspaceWritePlans => "Workspace 写 plans/specs/decisions",
            PermissionAction::WorkspaceWriteCodeArea => "Workspace 写代码任务区",
            PermissionAction::WorkspaceWriteProduction => "Workspace 写生产/凭据区",
            PermissionAction::ArtifactCreateOrLink => "Artifact 创建/关联",
            PermissionAction::MessageSend => "Message 发送/回复/确认",
            PermissionAction::MessageRetry => "Message retry",
            PermissionAction::MessageReassign => "Message reassign",
            PermissionAction::NotificationPolicyEdit => "NotificationPolicy 修改",
            PermissionAction::ApprovalRequest => "Approval 请求",
            PermissionAction::ApprovalDecide => "Approval 批准/拒绝",
            PermissionAction::EventActivityRead => "Event/Activity 读取",
            PermissionAction::AuditExport => "审计导出",
            PermissionAction::DeployOrProduction => "部署/付费/生产数据/外发消息",
        }
    }

    /// Row index, so `MATRIX` can be indexed positionally.
    ///
    /// The discriminant **is** the §3 row number, so this cannot drift from the
    /// variant's position in the specification. The remaining hazard — `ALL`
    /// being reordered or incomplete — is caught by
    /// `all_is_indexed_by_discriminant` in the test suite.
    pub const fn index(self) -> usize {
        self as usize
    }
}

impl fmt::Display for PermissionAction {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.spec_label())
    }
}

const fn v(s: SimpleVerdict) -> MatrixVerdict {
    MatrixVerdict::Simple(s)
}

use SimpleVerdict::{Allow as A, Deny as X, HumanOnly as H, ReadOnly as R, Scoped as S};

/// §3's matrix: 36 rows × [`ActorKind::ALL`], in `PermissionAction::ALL` order.
///
/// Flat positional layout rather than one struct per row: 216 cells stay readable
/// as a grid, and `q09-matrix-vs-spec.py` can diff them against a mechanical parse
/// of §3 without a bespoke parser per shape.
pub const MATRIX: &[[MatrixVerdict; 6]; 36] = &[
    // Team 创建/编辑 — Harness: 不允许
    [v(A), v(X), v(X), v(X), v(X), v(X)],
    // Team 归档/恢复 — 不允许
    [v(H), v(X), v(X), v(X), v(X), v(X)],
    // Role 创建/编辑/停用/恢复 — 不允许
    [v(H), v(X), v(X), v(X), v(X), v(X)],
    // Role/Binding/权限历史读取 — 仅本 Team、脱敏
    [v(A), v(R), v(R), v(R), v(R), v(R)],
    // Harness Instance 注册/编辑/禁用 — System 仅执行状态更新
    [v(H), v(X), v(X), v(X), v(X), v(S)],
    // Harness 健康检查/诊断 — 仅当前/授权实例；无凭据值
    [v(A), v(R), v(R), v(R), v(R), v(S)],
    // Binding 绑定/解绑/换绑 — Harness 绝不可自换绑
    [v(H), v(X), v(X), v(X), v(X), v(X)],
    // CredentialReference 创建/选择/删除 — System 只可按 Run 读取引用对应秘密
    [v(H), v(X), v(X), v(X), v(X), v(S)],
    // Workspace 绑定/更换 — 不允许
    [v(H), v(X), v(X), v(X), v(X), v(X)],
    // Task 创建 — Role 模板和 Team 策略允许；owner 必须合法
    [v(A), v(A), v(S), v(S), v(S), v(S)],
    // Task 分配/改派 — System 只按显式策略/依赖执行
    [v(A), v(A), v(S), v(X), v(X), v(S)],
    // Task 编辑轻量字段 — owner Role 只能改自身允许字段；版本匹配
    [v(A), v(A), v(S), v(S), v(S), v(S)],
    // Task 设置依赖/软依赖 — 软依赖覆盖原因必填；Human 可越过硬依赖
    //   【§3 图例未定义 A/H 与 A/S；两格同由「是否越过硬依赖」决定】
    [
        MatrixVerdict::ContextDependent(VerdictPair {
            first: A,
            second: H,
            decided_by: DiscriminatingFact::HardDependencyBypass,
        }),
        MatrixVerdict::ContextDependent(VerdictPair {
            first: A,
            second: S,
            decided_by: DiscriminatingFact::HardDependencyBypass,
        }),
        v(S),
        v(X),
        v(S),
        v(S),
    ],
    // Task claim — 当前 Binding、owner Role、READY、权限、lease/fencing
    [v(A), v(S), v(S), v(S), v(S), v(S)],
    // Task block/submit review — 仅 owner/关联 Run；满足状态前置条件
    [v(A), v(S), v(S), v(S), v(S), v(S)],
    // Task cancel/reopen — 活动 Run 必须先 cancel/fence
    [v(H), v(S), v(X), v(X), v(X), v(X)],
    // Run start/heartbeat/finish — 仅关联 Task/Binding/token
    [v(S), v(S), v(S), v(S), v(S), v(A)],
    // Run retry — 只对有权 Task，且旧 Run 终态/已 fence
    [v(A), v(S), v(S), v(S), v(S), v(S)],
    // Run cancel — Role 仅取消自己拥有 Task 的 Run；强制取消为 Human-only
    [v(H), v(S), v(X), v(S), v(S), v(S)],
    // Human takeover — 必须撤销旧写入权并记 Human actor
    [v(H), v(X), v(X), v(X), v(X), v(X)],
    // Review 提交 — 至少一个 Result Artifact
    [v(A), v(S), v(S), v(S), v(S), v(S)],
    // Review 通过/退回 — Reviewer 不能审批自己未获授权的高风险结果
    [
        v(A),
        v(S),
        v(S),
        v(X),
        MatrixVerdict::ContextDependent(VerdictPair {
            first: A,
            second: S,
            decided_by: DiscriminatingFact::ReviewerAuthorizedForHighRiskResult,
        }),
        v(S),
    ],
    // Workspace 读取共享资料 — 当前 Team、Reference/启动包范围
    [v(A), v(R), v(R), v(R), v(R), v(S)],
    // Workspace 写 plans/specs/decisions — 按 Role path policy
    [v(A), v(S), v(S), v(X), v(S), v(X)],
    // Workspace 写代码任务区 — Task 授权路径 + 有效 fencing；QA 默认只读代码
    [
        v(A),
        v(S),
        v(X),
        v(S),
        MatrixVerdict::ContextDependent(VerdictPair {
            first: R,
            second: S,
            decided_by: DiscriminatingFact::TaskAuthorizedCodePath,
        }),
        v(S),
    ],
    // Workspace 写生产/凭据区 — P0 默认不允许，即使有 capability
    [v(H), v(X), v(X), v(X), v(X), v(X)],
    // Artifact 创建/关联 — creator/Run/路径/版本校验
    [v(A), v(S), v(S), v(S), v(S), v(S)],
    // Message 发送/回复/确认 — to_role_id、Team、Task/Reference 可见性校验
    [v(A), v(S), v(S), v(S), v(S), v(S)],
    // Message retry — sender/target 授权、TTL 和错误可重试
    [v(A), v(S), v(S), v(S), v(S), v(S)],
    // Message reassign — 只有被显式授权的 Leader 可改派；保留历史
    [v(H), v(S), v(X), v(X), v(X), v(X)],
    // NotificationPolicy 修改 — 不能屏蔽强制安全通知
    [v(H), v(S), v(X), v(X), v(X), v(X)],
    // Approval 请求 — 请求不等于授权
    [v(A), v(S), v(S), v(S), v(S), v(S)],
    // Approval 批准/拒绝 — P0 仅 Human；禁止 requester 自批
    [v(H), v(X), v(X), v(X), v(X), v(X)],
    // Event/Activity 读取 — 只显示可见对象，metadata 脱敏
    [v(A), v(R), v(R), v(R), v(R), v(S)],
    // 审计导出 — 后台任务由 System 执行，结果仅 Owner 可取
    [v(H), v(X), v(X), v(X), v(X), v(S)],
    // 部署/付费/生产数据/外发消息 — P0 不实现；即使批准也需未来专门能力
    [v(H), v(X), v(X), v(X), v(X), v(X)],
];

/// The §3 cell for `actor` attempting `action`.
///
/// Returns `None` only if the table is internally inconsistent, which
/// `matrix_is_rectangular_and_complete` in the test suite rules out at compile
/// time by construction: the array is `[MatrixVerdict; 6]` × 36.
pub fn matrix_verdict(action: PermissionAction, actor: ActorKind) -> MatrixVerdict {
    MATRIX[action.index()][actor.index()]
}

/// `true` when `actor` may not attempt `action` **because the action is
/// Human-only**, as opposed to because this actor's Role lacks the capability.
///
/// §3 marks Human-only rows by putting `H` in the **Owner** column, and across
/// all 36 rows `H` never appears in another column — so the Owner cell is the
/// row's Human-only flag. §5 separately freezes actions (Binding 绑定,
/// Approval 批准/拒绝, …) that §3 gives `—` to every Role, and §7 separates the
/// two refusals by code: `AUTH_PERMISSION_DENIED` means "查看权限依据/请求授权"
/// while `AUTH_HUMAN_REQUIRED` means "打开审批/接管入口". Telling a Leader to
/// request permission for `Binding 绑定/解绑/换绑` would point at a grant that
/// §5 item 3 says can never exist; the right remedy is the Human entry point.
///
/// **Derived reading, not mechanical.** §3 and §5 do not cover the same set:
/// `Task cancel/reopen`, `Message reassign` and `NotificationPolicy 修改` carry
/// `H` but are absent from §5's eight categories, and §5 lists 跨 Team 分享 and
/// 删除/清理关键历史 for which §3 has no row. No frozen line states which code a
/// `—` in such a row must produce. The supporting facts are (a) `H` appears only
/// in the Owner column — verified mechanically, 16 occurrences — and (b) the two
/// codes carry different remedies in `ERROR_CATALOG.md`. See
/// [`crate::permission_intersection`]'s module note for why this reading was
/// chosen over the equally under-specified alternative.
///
/// Rows whose Owner cell is `A` are **not** Human-only even when every other
/// column is `—`: `Team 创建/编辑` is `A` for Owner and `—` elsewhere, and §5
/// does not list it. Its non-Owner `—` cells are ordinary permission denials.
pub const fn is_human_only_action(action: PermissionAction) -> bool {
    matches!(
        MATRIX[action.index()][0],
        MatrixVerdict::Simple(SimpleVerdict::HumanOnly)
    )
}

/// `true` when this actor's refusal of `action` is a Human-only refusal.
///
/// `H` never appears outside the Owner column, so a Human actor is never
/// refused on Human-only grounds; a Role/System actor is refused whenever the
/// row is Human-only and its own cell does not permit the attempt.
pub const fn refusal_is_human_only(action: PermissionAction, actor: ActorKind) -> bool {
    is_human_only_action(action) && !MATRIX[action.index()][actor.index()].permits()
}

impl MatrixVerdict {
    /// `true` when the cell permits the attempt outright. A
    /// [`MatrixVerdict::ContextDependent`] cell never does on its own — see the
    /// module note.
    pub const fn permits(self) -> bool {
        match self {
            MatrixVerdict::Simple(s) => s.permits_attempt(),
            MatrixVerdict::ContextDependent(_) => false,
        }
    }
}
