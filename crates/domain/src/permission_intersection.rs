//! §2 effective-permission intersection.
//!
//! Frozen source: `docs/specs/PERMISSION_MATRIX.md` §2, §5, §6 and §7.
//!
//! ```text
//! allow = TeamPolicy(action, resource)
//!       ∩ ActorBasePermission
//!       ∩ RolePolicy（Role/Binding actor 时）
//!       ∩ TaskScope（关联 Task/Run 时）
//!       ∩ WorkspacePathPolicy（文件动作时）
//!       ∩ HarnessCapability（由 Harness 执行时）
//!       ∩ ActiveBindingAndFencing（运行时动作）
//!       ∩ ValidApprovalGrant（高风险动作）
//! ```
//!
//! # No check can be skipped
//!
//! [`PermissionRequest`] carries a **fixed-size** `[LayerVerdict; 8]` — one entry
//! per term above. There is no way to build a request that omits a term, so
//! Team and actor checks cannot be skipped using NotApplicable. Conditional
//! Task/path/capability/approval applicability remains the application
//! resolver's responsibility; the array alone is not proof that it checked them. §2's 默认拒绝 is then a property of the scan: a term only permits
//! when it says so.
//!
//! # The evaluation order is §2's printed order, and first denial wins
//!
//! "任一层拒绝即终止". The decision reports **which** term refused and why,
//! because §7 requires the denial Event to record 脱敏原因, and because
//! `ValidApprovalGrant` is printed last: a Human-only refusal in an earlier term
//! is found before the approval term is even consulted.
//!
//! This is a **dependency, not a coincidence**. §2's printed order is not a
//! statement about causal importance: if some future case needed the approval
//! term consulted *before* deciding whether an action is Human-only, this order
//! would give the wrong answer. No §2/§3/§5 case needs that today.
//!
//! # A `—` in a Human-only row is a Human-only refusal, not a permission denial
//!
//! §3 marks a Human-only row by putting `H` in the **Owner** column, and across
//! all 36 rows `H` never appears anywhere else — so the Owner cell is the row's
//! Human-only flag. §5 freezes actions (Binding 绑定, Approval 批准/拒绝, …) that
//! §3 gives `—` to every Role, and §7 separates the two refusals by code:
//! `AUTH_PERMISSION_DENIED` means "查看权限依据/请求授权", `AUTH_HUMAN_REQUIRED`
//! means "打开审批/接管入口". Telling a Leader to request a grant for a §5-frozen
//! action points at something §5 says can never exist.
//!
//! **This is a derived reading, not a mechanical one.** §3 and §5 are two
//! independent statements that do not cover the same set: `Task cancel/reopen`,
//! `Message reassign` and `NotificationPolicy 修改` carry `H` but are absent from
//! §5's eight categories, while §5 lists 跨 Team 分享 and 删除/清理关键历史 for
//! which §3 has no row at all. No frozen line states which code a `—` in such a
//! row must produce. The two supporting facts are (a) `H` appears only in the
//! Owner column, verified mechanically across all 36 rows, and (b) the two codes
//! carry different remedies in `ERROR_CATALOG.md`.
//!
//! The alternative — every `—` reports `AUTH_PERMISSION_DENIED` — is equally
//! under-specified and would point a Leader at a grant §5 says can never exist.
//! With both readings under-determined, this one is chosen because it agrees with
//! the catalogue's stated remedies, and the dependency is written down here so a
//! different judgement is a one-place change.
//!
//! Rows whose Owner cell is `A` are **not** Human-only even when every other
//! column is `—`: `Team 创建/编辑` is `A`/`—`×5 and §5 does not list it, so its
//! non-Owner cells are ordinary permission denials. See
//! [`crate::permission_matrix::is_human_only_action`].
//!
//! # Approval narrows, never rescues
//!
//! §2: "Approval 只能缩小或临时补充已允许的动作，不能突破 Team 安全上限."
//! That falls out of first-denial-wins with no special case: a term that denied
//! has already terminated the scan, so no later approval term can overturn it.
//! What an approval *can* do is supply a term that would otherwise be
//! [`LayerVerdict::NotApplicable`]. `approval_can_supplement_but_never_rescue`
//! pins both halves.
//!
//! # No derived identifier gets a serde derive
//!
//! [`PermissionErrorCode`] does, because it is transcribed from
//! `ERROR_CATALOG.md`'s table. [`Layer`] and [`crate::permission_matrix::ActorKind`]
//! carry no serde derive: §2 prints its eight terms as prose in a code block and
//! `ActorKind` is §3's matrix columns, not the Event `actor_kind` field whose only
//! frozen value is `ROLE_BINDING` (migration 0015). Deriving serde would invent
//! wire names no specification constrains — the same trap as DEV-02-11's Q-04/Q-05
//! devices, where `rename_all` silently produced `VALID_APPROVAL_GRANT` where the
//! catalogue says `valid_approval_grant`. The stable identifiers are the
//! `condition_key()` methods, pinned literally by the test suite.
//!
//! # A decision cannot change a resource
//!
//! The node's criterion is "拒绝不改变资源". That is guaranteed here by type:
//! [`PermissionDecision`] carries verdicts and strings and holds no handle, no
//! reference and no interior mutability, and [`evaluate_permission`] takes its input by
//! value. There is no path from a denial to a write.

use std::fmt;

use crate::idempotency::PayloadHash;
use crate::{Actor, BindingId, HumanId, RunId, TaskId, TeamId, UuidV7};

use serde::{Deserialize, Serialize};

use crate::permission_matrix::{
    ActorKind, DiscriminatingFact, MatrixVerdict, PermissionAction, SimpleVerdict,
};

/// One term of §2's intersection, in the order §2 prints them.
///
/// The discriminant is the position in that list, so [`Layer::index`] cannot drift.
///
/// No `serde` derive: §2 prints these as prose inside a code block
/// (`TeamPolicy(action, resource)`, `ValidApprovalGrant`, …), not as wire values,
/// and no frozen document gives them a serialised spelling. The stable
/// identifier is [`Layer::condition_key`], which feeds the catalogue's
/// `condition_keys` detail. Deriving `serde` here would invent a wire name no
/// specification constrains — the same trap DEV-02-11's Q-04/Q-05 devices exist
/// to catch, where `#[serde(rename_all)]` silently produced `VALID_APPROVAL_GRANT`
/// where the catalogue and the audit key both say `valid_approval_grant`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Layer {
    /// `TeamPolicy(action, resource)`
    TeamPolicy = 0,
    /// `ActorBasePermission`
    ActorBasePermission = 1,
    /// `RolePolicy`（Role/Binding actor 时）
    RolePolicy = 2,
    /// `TaskScope`（关联 Task/Run 时）
    TaskScope = 3,
    /// `WorkspacePathPolicy`（文件动作时）
    WorkspacePathPolicy = 4,
    /// `HarnessCapability`（由 Harness 执行时）
    HarnessCapability = 5,
    /// `ActiveBindingAndFencing`（运行时动作）
    ActiveBindingAndFencing = 6,
    /// `ValidApprovalGrant`（高风险动作）
    ValidApprovalGrant = 7,
}

impl Layer {
    /// All eight terms, in §2's order.
    pub const ALL: [Layer; 8] = [
        Layer::TeamPolicy,
        Layer::ActorBasePermission,
        Layer::RolePolicy,
        Layer::TaskScope,
        Layer::WorkspacePathPolicy,
        Layer::HarnessCapability,
        Layer::ActiveBindingAndFencing,
        Layer::ValidApprovalGrant,
    ];

    /// Position in §2's list, and therefore scan order.
    pub const fn index(self) -> usize {
        self as usize
    }

    /// The stable condition key for the catalogue's `condition_keys` detail.
    pub const fn condition_key(self) -> &'static str {
        match self {
            Layer::TeamPolicy => "team_policy",
            Layer::ActorBasePermission => "actor_base_permission",
            Layer::RolePolicy => "role_policy",
            Layer::TaskScope => "task_scope",
            Layer::WorkspacePathPolicy => "workspace_path_policy",
            Layer::HarnessCapability => "harness_capability",
            Layer::ActiveBindingAndFencing => "active_binding_and_fencing",
            Layer::ValidApprovalGrant => "valid_approval_grant",
        }
    }
}

impl fmt::Display for Layer {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.condition_key())
    }
}

/// Why a term refused. The variant selects the §7 wire code, so the code is a
/// function of the reason rather than something a caller can get wrong.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum DenialReason {
    /// The term found no policy permitting the attempt.
    NotPermitted,
    /// §3 `H` / §5's frozen Human-only list. §7: `AUTH_HUMAN_REQUIRED`.
    HumanOnly,
    /// §2 step 4: path canonicalised and found outside scope, or behind a
    /// symlink escape. §7: `WORKSPACE_PATH_OUTSIDE_SCOPE`.
    PathOutsideScope,
    /// The Role/Binding, Run or fencing token is not valid.
    BindingOrFencingInvalid,
    /// §6: the grant is expired, already consumed, or does not match this
    /// actor/context.
    ApprovalGrantInvalid,
    /// §6: "Approval 不写回 Role Policy，不在换绑后自动转给新 Harness" — the grant
    /// was issued against a different Binding version.
    ApprovalBoundToAnotherBinding,
    /// The Harness did not declare the capability the action needs.
    CapabilityMissing,
    /// The actor is outside the Team, or the object does not exist / is inactive.
    /// §7: "知道 ID 不等于有读取权".
    ObjectNotVisible,
    /// §3's `S`: permitted only under a narrower scope, and that scope was not
    /// established. Treated as a denial, not a silent widen.
    ScopeNotEstablished,
    /// §2 step 7's default: the term gave no verdict permitting the attempt.
    DefaultDeny,
}

impl DenialReason {
    /// The frozen §7 wire code for this reason.
    pub const fn error_code(self) -> PermissionErrorCode {
        match self {
            DenialReason::HumanOnly => PermissionErrorCode::HumanRequired,
            DenialReason::PathOutsideScope => PermissionErrorCode::PathOutsideScope,
            DenialReason::NotPermitted
            | DenialReason::BindingOrFencingInvalid
            | DenialReason::ApprovalGrantInvalid
            | DenialReason::ApprovalBoundToAnotherBinding
            | DenialReason::CapabilityMissing
            | DenialReason::ObjectNotVisible
            | DenialReason::ScopeNotEstablished
            | DenialReason::DefaultDeny => PermissionErrorCode::PermissionDenied,
        }
    }

    /// The stable condition key for the Event's `condition_keys` detail.
    pub const fn condition_key(self) -> &'static str {
        match self {
            DenialReason::NotPermitted => "not_permitted",
            DenialReason::HumanOnly => "human_only",
            DenialReason::PathOutsideScope => "path_outside_scope",
            DenialReason::BindingOrFencingInvalid => "binding_or_fencing_invalid",
            DenialReason::ApprovalGrantInvalid => "approval_grant_invalid",
            DenialReason::ApprovalBoundToAnotherBinding => "approval_bound_to_another_binding",
            DenialReason::CapabilityMissing => "capability_missing",
            DenialReason::ObjectNotVisible => "object_not_visible",
            DenialReason::ScopeNotEstablished => "scope_not_established",
            DenialReason::DefaultDeny => "default_deny",
        }
    }
}

impl fmt::Display for DenialReason {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.condition_key())
    }
}

/// The §7 rejection codes, anchored to migration `0009`'s frozen vocabulary.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum PermissionErrorCode {
    #[serde(rename = "AUTH_PERMISSION_DENIED")]
    PermissionDenied,
    #[serde(rename = "AUTH_HUMAN_REQUIRED")]
    HumanRequired,
    #[serde(rename = "WORKSPACE_PATH_OUTSIDE_SCOPE")]
    PathOutsideScope,
}

impl PermissionErrorCode {
    /// The frozen wire name.
    pub const fn as_str(self) -> &'static str {
        match self {
            PermissionErrorCode::PermissionDenied => "AUTH_PERMISSION_DENIED",
            PermissionErrorCode::HumanRequired => "AUTH_HUMAN_REQUIRED",
            PermissionErrorCode::PathOutsideScope => "WORKSPACE_PATH_OUTSIDE_SCOPE",
        }
    }

    /// All three are 否 in `ERROR_CATALOG.md`; asserted, not assumed.
    pub const fn is_retryable(self) -> bool {
        false
    }
}

impl fmt::Display for PermissionErrorCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// One term's outcome, as the application layer resolved it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum LayerVerdict {
    /// The term does not apply to this request (e.g. no file path involved). It
    /// neither permits nor refuses.
    NotApplicable,
    /// The term permits the attempt outright.
    Allow,
    /// The term permits it only under a narrower scope, and that scope is
    /// established. §3's `S`.
    Scoped,
    /// The term refuses, with a reason that selects the §7 code.
    Deny(DenialReason),
}

impl LayerVerdict {
    /// `true` if this term refuses and therefore terminates the scan.
    pub const fn denies(self) -> bool {
        matches!(self, LayerVerdict::Deny(_))
    }

    /// `true` if this term narrows rather than permits outright.
    pub const fn is_scoped(self) -> bool {
        matches!(self, LayerVerdict::Scoped)
    }
}

/// The facts §3's four compound cells need, supplied by the application layer.
///
/// Absent facts deny. §2 step 7 is 默认拒绝, and §3's compound cells exist
/// precisely because the table declines to pick a winner, so guessing one here
/// would reintroduce the ambiguity §3 avoided.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct CompoundFacts {
    /// `Task 设置依赖/软依赖` cells: whether a **hard** dependency is being
    /// bypassed.
    pub hard_dependency_bypass: Option<bool>,
    /// `Review 通过/退回` + QA: whether the reviewer is authorised for this
    /// high-risk result.
    pub reviewer_authorized_for_high_risk: Option<bool>,
    /// `Workspace 写代码任务区` + QA: whether a Task authorises this code path.
    pub task_authorized_code_path: Option<bool>,
}

impl CompoundFacts {
    /// The fact a compound cell needs, by name.
    pub const fn get(self, fact: DiscriminatingFact) -> Option<bool> {
        match fact {
            DiscriminatingFact::HardDependencyBypass => self.hard_dependency_bypass,
            DiscriminatingFact::ReviewerAuthorizedForHighRiskResult => {
                self.reviewer_authorized_for_high_risk
            }
            DiscriminatingFact::TaskAuthorizedCodePath => self.task_authorized_code_path,
        }
    }

    /// The result of resolving one compound cell against these facts.
    ///
    /// `None` means the cell could not be resolved and must be treated as a
    /// denial. It is **not** an error: the caller simply has not checked yet.
    pub const fn resolve_cell(self, cell: MatrixVerdict) -> Option<SimpleVerdict> {
        match cell {
            MatrixVerdict::Simple(v) => Some(v),
            MatrixVerdict::ContextDependent(pair) => match self.get(pair.decided_by) {
                Some(first_applies) => Some(pair.resolve(first_applies)),
                None => None,
            },
        }
    }
}

/// A request to attempt one action as one actor, with every §2 term resolved.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PermissionRequest {
    /// Who is attempting it.
    pub actor: ActorKind,
    /// What they are attempting.
    pub action: PermissionAction,
    /// One verdict per §2 term, positionally. All eight must be given: there is
    /// no partial request to construct.
    pub terms: [LayerVerdict; 8],
    /// Facts the §3 compound cells need, if this (action, actor) hits one.
    pub facts: CompoundFacts,
}

impl PermissionRequest {
    /// The verdict recorded for `layer`.
    pub const fn term(&self, layer: Layer) -> LayerVerdict {
        self.terms[layer.index()]
    }
}

/// The outcome of one evaluation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum PermissionDecision {
    /// Every term permitted and none narrowed.
    Allow,
    /// Permitted, but at least one term narrowed the scope. §3's `S`.
    AllowScoped {
        /// The narrowest term, for the caller's audit record.
        limited_by: Layer,
    },
    /// Refused. No resource was touched and nothing was mutated.
    Deny {
        /// Which term or gate refused. `None` when the §3 matrix cell refused
        /// before any term was consulted.
        layer: Option<Layer>,
        /// Why.
        reason: DenialReason,
    },
}

impl PermissionDecision {
    /// `true` for [`PermissionDecision::Allow`] and [`PermissionDecision::AllowScoped`].
    pub const fn permits(self) -> bool {
        matches!(
            self,
            PermissionDecision::Allow | PermissionDecision::AllowScoped { .. }
        )
    }

    /// The §7 code a denial reports, or `None` when permitted.
    pub const fn error_code(self) -> Option<PermissionErrorCode> {
        match self {
            PermissionDecision::Allow | PermissionDecision::AllowScoped { .. } => None,
            PermissionDecision::Deny { reason, .. } => Some(reason.error_code()),
        }
    }

    /// The condition keys to record for a denial, or `None` when permitted.
    ///
    /// §7: the denial Event must carry a 脱敏原因 and must not carry secrets,
    /// full environment variables or unauthorised content. These are stable
    /// identifiers with no content in them, which is what makes them safe to
    /// persist verbatim. A permitted decision records nothing.
    pub const fn condition_keys(self) -> Option<[&'static str; 2]> {
        match self {
            PermissionDecision::Allow | PermissionDecision::AllowScoped { .. } => None,
            PermissionDecision::Deny { layer, reason } => Some(DenialKeys::of(reason, layer)),
        }
    }
}

/// Namespace marker for [`PermissionDecision::condition_keys`], so the key pair
/// is read as "term + reason" at the call site rather than a bare slice.
///
/// Exists because a variable-length `&'static [&'static str]` cannot be built in a
/// `const fn` match arm. A fixed `[&'static str; 2]` can, and a caller cannot
/// invent a key string because neither [`Layer::condition_key`] nor
/// [`DenialReason::condition_key`] takes one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DenialKeys;

impl DenialKeys {
    /// The refusing term's key and the reason's key, in that order. A refusal
    /// that happened at the §3 matrix cell has no term, so the namespace is
    /// recorded explicitly rather than left blank.
    pub const fn of(reason: DenialReason, layer: Option<Layer>) -> [&'static str; 2] {
        match layer {
            Some(l) => [l.condition_key(), reason.condition_key()],
            None => ["permission_matrix", reason.condition_key()],
        }
    }
}

impl fmt::Display for PermissionDecision {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PermissionDecision::Allow => f.write_str("allow"),
            PermissionDecision::AllowScoped { limited_by } => {
                write!(f, "allow-scoped (limited by {limited_by})")
            }
            PermissionDecision::Deny { layer, reason } => match layer {
                Some(l) => write!(f, "deny {reason} at {l}"),
                None => write!(f, "deny {reason} at the §3 matrix"),
            },
        }
    }
}

/// Evaluate one request. Pure: no I/O, no clock, no mutation, input by value.
///
/// The order is §2's printed order. `ValidApprovalGrant` is printed last, so a
/// Human-only refusal in an earlier term is found before the approval term is
/// consulted — which is what makes "Approval 不能突破 Team 安全上限" structural
/// rather than a rule someone has to remember.
pub fn evaluate_permission(request: PermissionRequest) -> PermissionDecision {
    // 0. The §3 matrix cell. This is the Role/Binding layer's static part, and it
    //    runs first because a `Deny` or `HumanOnly` cell is decisive on its own.
    let cell = crate::permission_matrix::matrix_verdict(request.action, request.actor);
    match request.facts.resolve_cell(cell) {
        None => {
            return PermissionDecision::Deny {
                layer: None,
                reason: DenialReason::DefaultDeny,
            };
        }
        Some(SimpleVerdict::Deny) => {
            // A `—` in a Human-only row is a Human-only refusal, not a plain
            // permission denial: §5 freezes the action, so §7 wants
            // AUTH_HUMAN_REQUIRED (whose remedy is the Human entry point), not
            // AUTH_PERMISSION_DENIED (whose remedy is "request a grant" — a grant
            // §5 says can never exist). `is_human_only_action` keys on the Owner
            // cell, which is the only place `H` ever appears in §3.
            let reason =
                if crate::permission_matrix::refusal_is_human_only(request.action, request.actor) {
                    DenialReason::HumanOnly
                } else {
                    DenialReason::NotPermitted
                };
            return PermissionDecision::Deny {
                layer: None,
                reason,
            };
        }
        Some(SimpleVerdict::HumanOnly) if !request.actor.is_human() => {
            return PermissionDecision::Deny {
                layer: None,
                reason: DenialReason::HumanOnly,
            };
        }
        Some(_) => {}
    }

    // 1..8. Terms, in §2's order, first denial wins.
    let mut narrowed_by: Option<Layer> = None;
    for layer in Layer::ALL {
        match request.term(layer) {
            LayerVerdict::Deny(reason) => {
                return PermissionDecision::Deny {
                    layer: Some(layer),
                    reason,
                };
            }
            LayerVerdict::Scoped => {
                if narrowed_by.is_none() {
                    narrowed_by = Some(layer);
                }
            }
            LayerVerdict::NotApplicable
                if matches!(layer, Layer::TeamPolicy | Layer::ActorBasePermission)
                    || (layer == Layer::RolePolicy
                        && !matches!(request.actor, ActorKind::HumanOwner | ActorKind::System)) =>
            {
                return PermissionDecision::Deny {
                    layer: Some(layer),
                    reason: DenialReason::DefaultDeny,
                };
            }
            LayerVerdict::Allow | LayerVerdict::NotApplicable => {}
        }
    }

    match narrowed_by {
        Some(layer) => PermissionDecision::AllowScoped { limited_by: layer },
        None => PermissionDecision::Allow,
    }
}

// ------------------------------------------------------------------ §6 grants

/// Exact execution context bound to a P0 Approval.
///
/// The application resolves identity, ownership and canonical scope from trusted
/// records. The digest represents the canonical scope/parameters, not a policy
/// expression: broader or different parameters must use a different grant.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ApprovalContext {
    /// Team isolation boundary.
    pub team_id: TeamId,
    /// Stable actor identity, never a matrix category.
    pub actor: Actor,
    /// Binding identity and version; equal versions do not identify a Binding.
    pub binding_id: BindingId,
    /// Object version of the authorized Binding.
    pub binding_version: u64,
    /// Requested action from the permission matrix.
    pub action: PermissionAction,
    /// Canonical resource vocabulary, validated by the application.
    pub resource_type: &'static str,
    /// Exact authorized resource.
    pub resource_id: UuidV7,
    /// Optional Task scope.
    pub task_id: Option<TaskId>,
    /// Run scope; retries cannot inherit grants silently.
    pub run_id: Option<RunId>,
    /// Digest of the exact canonical scope and operation parameters.
    pub scope_hash: PayloadHash,
}

/// P0 single-use grant with actual identities and execution scope.
///
/// This check does not consume the grant. Final use and consumption must happen
/// atomically in the application transaction; callers must not cache an allow.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ApprovalGrant {
    /// Requester retained for self-decision prevention.
    pub requested_by: Actor,
    /// Decision attribution loaded from the Approval record.
    pub decided_by: Actor,
    /// The Team's P0 Owner, loaded from trusted installation/Team identity.
    pub owner_id: HumanId,
    /// Exact context this grant authorizes.
    pub context: ApprovalContext,
    /// Whether the single-use grant has already been consumed.
    pub consumed: bool,
    /// UTC Unix millisecond expiry; checked against an injected control-plane clock.
    pub expires_at_ms: i64,
}

/// Validate Human ownership, self-decision, expiry, consumption and exact scope.
///
/// Binding mismatch keeps its existing diagnostic priority. No new wire error
/// codes are introduced. The application must also check its policy ceiling and
/// atomically consume the grant in the same transaction as the business effect.
pub fn approval_grant_is_usable(
    grant: ApprovalGrant,
    use_context: ApprovalContext,
    now_ms: i64,
) -> Option<DenialReason> {
    if grant.context.binding_id != use_context.binding_id
        || grant.context.binding_version != use_context.binding_version
    {
        return Some(DenialReason::ApprovalBoundToAnotherBinding);
    }
    if grant.decided_by != Actor::Human(grant.owner_id)
        || grant.requested_by == grant.decided_by
        || now_ms >= grant.expires_at_ms
        || grant.consumed
        || grant.context != use_context
    {
        return Some(DenialReason::ApprovalGrantInvalid);
    }
    None
}

// -------------------------------------------------------------- §7 cache rules

/// Freshness/version dimensions for an already identity-scoped cache key.
///
/// §7: "缓存的 allow 结果不能跨对象版本、Binding version、Approval expiry 或
/// fencing token 复用." Stated as a struct so the check is one comparison per
/// field. The surrounding cache key must still include Team, actual actor,
/// action/resource identity, policy and grant identity. This value is not
/// authorization and cannot substitute for the final execution-time checks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CacheContext {
    /// The object's version the cached decision was computed against (§2 step 2).
    pub object_version: u64,
    /// The Binding version in force when it was computed (§2 step 3).
    pub binding_version: u64,
    /// The Approval expiry the cached decision depended on, if any (§6).
    pub approval_expiry_tick: Option<u64>,
    /// The fencing token in force when it was computed (§2 step 3).
    pub fencing_token: u64,
}

impl CacheContext {
    /// `true` when an allow cached under `cached` may be reused under `self`.
    ///
    /// Every field must be equal, including the expiry tick's **value** — §7
    /// lists "Approval expiry" as one of the four things a cached allow may not
    /// cross, and `now_tick` must remain strictly before the grant deadline.
    /// Time advancing does not change the expiry value itself.
    ///
    /// Written field-by-field rather than `self == cached` because a derived
    /// `PartialEq` is not `const`. Adding a dimension requires updating this
    /// check and its
    /// negative tests; Rust does not enforce that relationship automatically.
    pub const fn may_reuse(self, cached: Self, now_tick: u64) -> bool {
        let unexpired = match self.approval_expiry_tick {
            Some(expiry) => now_tick < expiry,
            None => true,
        };
        unexpired
            && self.object_version == cached.object_version
            && self.binding_version == cached.binding_version
            && self.approval_expiry_tick.is_some() == cached.approval_expiry_tick.is_some()
            && match (self.approval_expiry_tick, cached.approval_expiry_tick) {
                (Some(a), Some(b)) => a == b,
                (None, None) => true,
                // Unreachable: the `is_some()` equality above already decided
                // this, but a `match` arm has to be spelled out.
                _ => false,
            }
            && self.fencing_token == cached.fencing_token
    }
}
