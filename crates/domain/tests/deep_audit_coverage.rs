//! DEV-02-11 / MM-R10: boundary, wire-code and rejection tests for the gaps the
//! 2026-10-08 deep audit measured at 1907/2197 domain lines (86.80%).
//!
//! Every assertion here is anchored to a **frozen document**, not to the
//! implementation's own `match`. The rule this file follows is the one the audit
//! enforced: a test that copies the implementation's arms proves nothing, so
//! every expected value below is the literal text from `docs/specs/` or from a
//! frozen migration `CHECK` constraint. If an implementation arm drifted, the
//! assertion here fails.
//!
//! ## What each test can detect
//!
//! | Test | Detects |
//! |---|---|
//! | `every_run_rejection_code_is_a_catalogued_run_code` | a Run rejection code renamed or invented outside `ERROR_CATALOG.md` |
//! | `every_code_the_run_graph_raises_is_listed_in_all` | an error code the §3 graph can raise but an application cannot branch on |
//! | `run_edges_expose_the_table_row_they_were_built_from` | `RunEdge` accessors disagreeing with the §3 rows |
//! | `run_command_all_covers_plain_plus_every_reconcile_outcome` | a reconcile destination reachable by no command |
//! | `each_reconcile_outcome_reaches_exactly_its_own_destination` | the linear-search ambiguity §3's evidence payload exists to prevent |
//! | `run_precondition_keys_are_unique_and_snake_case` | two §3 preconditions collapsing to one `condition_keys` detail |
//! | `task_edges_are_keyed_uniquely_and_match_the_transition_function` | a duplicate `(from, command)` key making `transition` pick a row arbitrarily |
//! | `section_2_2_prohibitions_are_refused_and_have_no_table_row` | a §2.2 "forbidden" edge quietly added to the table |
//! | `permission_matrix_compound_cells_never_default_to_allow` | §2 step 7 默认拒绝 regressing to a guess |
//! | `idempotency_state_spellings_match_the_frozen_migration` | domain/storage drift on migration `0022`'s `CHECK` |
//! | `only_a_missing_record_authorises_a_second_execution` | §4 row 2 duplicating a Run/Event/Artifact on replay |
//! | `cancel_decision_depends_on_the_status_not_only_the_observation` | a Run reported cancellable after its terminal state |
//! | `uuid_and_timestamp_reject_every_noncanonical_protocol_form` | `DATA_CONVENTIONS` §1/§2 canonical-form acceptance regressing |
//!
//! ## Scope discipline
//!
//! This file adds no production behaviour and asserts no new semantics. Where the
//! audit found a coverage gap it recorded a gap, not a defect; nothing here
//! encodes a guess about what an uncovered arm "should" do beyond what a frozen
//! document already fixes.

use teamspace_domain::approval_state_machine::{
    ApprovalCommand, ApprovalErrorCode, ApprovalStatus,
};
use teamspace_domain::attempt_state_machine::AttemptStatus;
use teamspace_domain::idempotency::{
    ActorFingerprint, CommandName, FirstOutcome, IdempotencyBusyCode, IdempotencyErrorCode,
    IdempotencyKey, IdempotencyKeyString, IdempotencyOutcome, IdempotencyRecord,
    IdempotencyReportedCode, IdempotencyState, IdempotentCall, PayloadHash, ResultRef,
    evaluate_replay,
};
use teamspace_domain::permission_intersection::{
    DenialKeys, DenialReason, Layer, LayerVerdict, PermissionDecision, PermissionErrorCode,
    PermissionRequest, evaluate_permission,
};
use teamspace_domain::permission_matrix::{
    ActorKind, DiscriminatingFact, MATRIX, MatrixVerdict, PermissionAction, SimpleVerdict,
    VerdictPair, matrix_verdict,
};
use teamspace_domain::run_state_machine::{
    RUN_EDGES, ReconcileOutcome, RunCommand, RunErrorCode, RunEvent, RunStatus, rejection_code,
    transition,
};
use teamspace_domain::run_termination::{
    CancelDecision, CancelObservation, ExecutionObservation, ReconcileEvidence, decide_cancel,
};
use teamspace_domain::task_state_machine::{TASK_EDGES, TaskCommand, TaskErrorCode, TaskStatus};
use teamspace_domain::{
    Actor, AdapterId, ExpectedVersion, HumanId, RoleId, TeamId, UtcTimestamp, UuidV7,
    ValueObjectError, Version, task_rejection_code, task_transition,
};

// ============================================================== helpers

/// A canonical UUIDv7 for fixtures. The bytes are fixed so failure messages are
/// stable, and the value is a real UUIDv7 (version nibble `7`, variant `10`) so
/// it satisfies `DATA_CONVENTIONS` §1.
fn uuid(byte: u8) -> UuidV7 {
    UuidV7::from_timestamp_and_random(1_700_000_000_000 + u64::from(byte), [byte; 10])
        .expect("fixture timestamp is inside the supported range")
}

/// `2026-10-08T12:34:56.789Z` — the canonical UTC RFC 3339 instant with exactly
/// three fractional digits that `DATA_CONVENTIONS` §2 requires.
const CANONICAL_INSTANT: &str = "2026-10-08T12:34:56.789Z";

fn timestamp() -> UtcTimestamp {
    UtcTimestamp::parse_rfc3339_millis(CANONICAL_INSTANT).expect("canonical instant parses")
}

/// All eight §2 terms permitting outright.
fn all_allow() -> [LayerVerdict; 8] {
    [LayerVerdict::Allow; 8]
}

/// A request whose every term permits, for the given action/actor.
fn permitting_request(action: PermissionAction, actor: ActorKind) -> PermissionRequest {
    PermissionRequest {
        actor,
        action,
        terms: all_allow(),
        facts: Default::default(),
    }
}

/// A fully-formed §4 record key built from a caller-supplied key string.
fn record_key(key: &str, command: &str) -> IdempotencyKey {
    IdempotencyKey::new(
        TeamId::from_uuid(uuid(2)),
        ActorFingerprint::of_actor(Actor::System),
        CommandName::parse(command).expect("command name is in the accepted alphabet"),
        IdempotencyKeyString::parse(key).expect("key is inside the accepted bound"),
    )
}

// ============================================================== §6 Run rejection codes

/// Every `RunErrorCode` must be a code `ERROR_CATALOG.md` actually lists, and it
/// must report the code that list names.
///
/// **Why this distinguishes a wrong implementation:** the expected strings are
/// transcribed from `ERROR_CATALOG.md` (§2, §3, §5, §6, §7, §8), not from
/// `RunErrorCode::as_str`. An implementation that renamed a variant's wire
/// string, invented a code, or dropped one leaves one of these pairs unmatched —
/// including a variant that exists in the enum but no frozen document names,
/// which is the "invented protocol" hazard `ERROR_CATALOG.md` §1 forbids
/// ("改名或改变语义属于破坏性变更").
#[test]
fn every_run_rejection_code_is_a_catalogued_run_code() {
    let catalogued: &[(RunErrorCode, &str)] = &[
        // §2 DOMAIN_INVALID_TRANSITION.
        (RunErrorCode::InvalidTransition, "DOMAIN_INVALID_TRANSITION"),
        // §2 DOMAIN_PRECONDITION_FAILED — the code §3's `—` rows derive.
        (
            RunErrorCode::PreconditionFailed,
            "DOMAIN_PRECONDITION_FAILED",
        ),
        // §5 TASK_ALREADY_CLAIMED.
        (RunErrorCode::AlreadyClaimed, "TASK_ALREADY_CLAIMED"),
        // §7 ADAPTER_UNAVAILABLE.
        (RunErrorCode::AdapterUnavailable, "ADAPTER_UNAVAILABLE"),
        // §3 AUTH_PERMISSION_DENIED.
        (RunErrorCode::PermissionDenied, "AUTH_PERMISSION_DENIED"),
        // §7 BINDING_STALE_VERSION.
        (RunErrorCode::BindingStaleVersion, "BINDING_STALE_VERSION"),
        // §6 RUN_CANCEL_NOT_CONFIRMED.
        (
            RunErrorCode::RunCancelNotConfirmed,
            "RUN_CANCEL_NOT_CONFIRMED",
        ),
        // §6 RUN_FENCED.
        (RunErrorCode::RunFenced, "RUN_FENCED"),
        // §8 ARTIFACT_INVALID.
        (RunErrorCode::ArtifactInvalid, "ARTIFACT_INVALID"),
    ];

    assert_eq!(
        RunErrorCode::ALL.len(),
        catalogued.len(),
        "RunErrorCode::ALL and this catalogue list disagree on how many codes exist; \
         either a code was added without a catalogue entry, or one was dropped"
    );

    for (code, wire) in catalogued {
        assert_eq!(
            code.as_str(),
            *wire,
            "{code:?} reports a wire code that ERROR_CATALOG.md does not name"
        );
    }
}

/// `RunErrorCode::ALL` must be exactly the codes §3's graph can raise. A code
/// `RUN_EDGES` can report but `ALL` omits is one an application cannot branch
/// on; a code in `ALL` that §3 never raises is one the enum carries without a
/// table row to justify it.
///
/// **Why this distinguishes a wrong implementation:** the expected set is built
/// by walking `RUN_EDGES` and `CREATE_QUEUED` (production data) and compared
/// against `ALL` in **both** directions. A duplicated `ALL` entry, or an edge
/// repointed at an unlisted code, breaks one of the two directions.
#[test]
fn every_code_the_run_graph_raises_is_listed_in_all() {
    let mut raised: Vec<&'static str> = Vec::new();
    for edge in RUN_EDGES {
        raised.push(edge.rejection.as_str());
    }
    raised.push(teamspace_domain::CREATE_QUEUED.rejection().as_str());
    // `transition` also reports this one for a command with no §3 row at all, so
    // it is raisable even though no edge row names it.
    raised.push(RunErrorCode::InvalidTransition.as_str());

    for wire in &raised {
        assert!(
            RunErrorCode::ALL.iter().any(|code| code.as_str() == *wire),
            "§3's graph raises {wire} but RunErrorCode::ALL does not list it"
        );
    }

    for code in RunErrorCode::ALL {
        assert!(
            raised.contains(&code.as_str()),
            "{} is in RunErrorCode::ALL but §3's graph never raises it",
            code.as_str()
        );
    }
}

/// `RUN_EDGES` is §3's table. Each row's accessors must return that row, so an
/// application reading `preconditions()`/`events()` gets the table's own content
/// rather than a default.
///
/// **Why this distinguishes a wrong implementation:** the assertion compares each
/// accessor against the struct fields the edge literal carries, and also checks
/// that `rejection_code` finds exactly that edge. An accessor returning a
/// hard-coded or empty slice compiles fine and fails here.
#[test]
fn run_edges_expose_the_table_row_they_were_built_from() {
    for edge in RUN_EDGES {
        let decided = transition(edge.from, edge.command).unwrap_or_else(|_| {
            panic!("RUN_EDGES row {:?} must be decidable", (edge.from, edge.to))
        });
        assert_eq!(decided.from(), edge.from);
        assert_eq!(decided.to(), edge.to);
        assert_eq!(decided.command(), edge.command);
        assert_eq!(
            decided.preconditions(),
            edge.preconditions,
            "{:?} → {:?} reports different preconditions than its §3 row",
            edge.from,
            edge.to
        );
        assert_eq!(
            decided.events(),
            edge.events,
            "{:?} → {:?} reports different events than its §3 row",
            edge.from,
            edge.to
        );
        assert_eq!(
            rejection_code(edge.from, edge.command),
            Some(edge.rejection),
            "{:?} → {:?} reports a different rejection code than its §3 row",
            edge.from,
            edge.to
        );
    }

    // `(from, command)` must key §3's table uniquely: a duplicate would make the
    // linear search in `transition` return whichever row came first, silently
    // choosing between two §3 destinations.
    let mut keys: Vec<(RunStatus, RunCommand)> = RUN_EDGES
        .iter()
        .map(|edge| (edge.from, edge.command))
        .collect();
    keys.sort();
    let before = keys.len();
    keys.dedup();
    assert_eq!(
        keys.len(),
        before,
        "two §3 Run rows share a (from, command) key; transition would pick one arbitrarily"
    );
}

/// `RunCommand::all()` is documented as "7 plain + 5 reconcile". If it lost a
/// reconcile destination, an application iterating `all()` to offer choices would
/// stop offering it while `transition` still accepted it.
///
/// **Why this distinguishes a wrong implementation:** the reconcile commands are
/// counted by matching them structurally against `ReconcileOutcome::ALL`, so a
/// `RunCommand::all()` that dropped or duplicated a variant fails the count *and*
/// the membership checks — it is not a stored-length comparison against a literal.
#[test]
fn run_command_all_covers_plain_plus_every_reconcile_outcome() {
    let all = RunCommand::all();
    let plain = RunCommand::PLAIN;

    assert_eq!(plain.len(), 7, "§3 names seven non-reconcile Run commands");
    assert_eq!(
        all.len(),
        plain.len() + ReconcileOutcome::ALL.len(),
        "RunCommand::all() must be PLAIN plus one command per reconcile outcome"
    );

    for command in plain {
        assert!(
            all.contains(&command),
            "{command:?} is in PLAIN but missing from all()"
        );
        assert!(
            !matches!(command, RunCommand::Reconcile(_)),
            "{command:?} is in PLAIN but carries a reconcile outcome"
        );
    }

    for outcome in ReconcileOutcome::ALL {
        let command = RunCommand::Reconcile(outcome);
        assert!(
            all.contains(&command),
            "the {outcome:?} reconcile destination is unreachable from RunCommand::all()"
        );
    }

    let mut distinct = all.to_vec();
    distinct.sort();
    distinct.dedup();
    assert_eq!(
        distinct.len(),
        all.len(),
        "RunCommand::all() lists a command twice"
    );
}

/// §3 sends the reconcile service down five destinations from `UNKNOWN`, and the
/// only thing distinguishing them is the proven evidence. Each outcome must land
/// on its own §3 destination — if two outcomes resolved alike, a Run the Adapter
/// proved dead could be recorded as still running, which is exactly the failure
/// the outcome payload exists to make unrepresentable.
///
/// **Why this distinguishes a wrong implementation:** the test asserts a
/// one-to-one mapping between `ReconcileOutcome::ALL` and the destination
/// `transition` returns, then requires the five destinations to be pairwise
/// distinct. An implementation that linear-searched the edge table — the bug the
/// docstring warns about — sends every outcome to `RUNNING` and fails.
#[test]
fn each_reconcile_outcome_reaches_exactly_its_own_destination() {
    let mut destinations: Vec<(ReconcileOutcome, RunStatus)> = Vec::new();

    for outcome in ReconcileOutcome::ALL {
        let decided = transition(RunStatus::Unknown, RunCommand::Reconcile(outcome))
            .unwrap_or_else(|_| {
                panic!("§3 gives UNKNOWN a destination for the {outcome:?} outcome")
            });
        destinations.push((outcome, decided.to()));
    }

    for (outcome, destination) in &destinations {
        // The expectations are §3's own UNKNOWN rows.
        let expected = match outcome {
            // "Adapter 证明同 session/token 仍运行；未产生更新世代".
            ReconcileOutcome::StillRunning => RunStatus::Running,
            // "可验证完成结果及 sequence/token".
            ReconcileOutcome::Succeeded => RunStatus::Succeeded,
            // §3 folds FAILED / TIMED_OUT / CANCELLED into one row.
            ReconcileOutcome::Failed => RunStatus::Failed,
            ReconcileOutcome::TimedOut => RunStatus::TimedOut,
            ReconcileOutcome::Cancelled => RunStatus::Cancelled,
        };
        assert_eq!(
            *destination, expected,
            "{outcome:?} must reconcile to {expected}, not {destination}"
        );
    }

    let mut distinct = destinations.clone();
    distinct.sort_by_key(|(_, status)| *status);
    distinct.dedup_by_key(|(_, status)| *status);
    assert_eq!(
        distinct.len(),
        destinations.len(),
        "two reconcile outcomes resolve to the same destination; §3 gives each its own row"
    );
}

/// §3's Run event names are a wire contract (the Event column), so each must be
/// distinct and must stay inside the `RUN_` namespace the Event stream reserves
/// for Runs.
///
/// **Why this distinguishes a wrong implementation:** two events sharing a name
/// would make the audit trail ambiguous, and a missing `RUN_` prefix would let a
/// Run event be filed as a Task event. The expected names are §3's own, listed
/// row by row.
#[test]
fn run_event_names_are_distinct_and_run_scoped() {
    let expected: &[(RunEvent, &str)] = &[
        (RunEvent::RunQueued, "RUN_QUEUED"),
        (RunEvent::RunStarting, "RUN_STARTING"),
        (RunEvent::RunStarted, "RUN_STARTED"),
        (RunEvent::RunSucceeded, "RUN_SUCCEEDED"),
        (RunEvent::RunFailed, "RUN_FAILED"),
        (RunEvent::RunTimedOut, "RUN_TIMED_OUT"),
        (RunEvent::RunCancelled, "RUN_CANCELLED"),
        (RunEvent::RunStatusUnknown, "RUN_STATUS_UNKNOWN"),
        (RunEvent::RunReconciledRunning, "RUN_RECONCILED_RUNNING"),
        (RunEvent::RunReconciledSucceeded, "RUN_RECONCILED_SUCCEEDED"),
        (RunEvent::RunReconciledTerminal, "RUN_RECONCILED_TERMINAL"),
    ];

    let mut names: Vec<&str> = Vec::new();
    for (event, wire) in expected {
        assert_eq!(
            event.as_str(),
            *wire,
            "{event:?} must be recorded as {wire} per §3's Event column"
        );
        assert!(
            event.as_str().starts_with("RUN_"),
            "{event:?} escapes the RUN_ event namespace"
        );
        names.push(event.as_str());
    }
    names.sort();
    let before = names.len();
    names.dedup();
    assert_eq!(names.len(), before, "two §3 Run events share a wire name");
}

/// An illegal Run command must be refused with the code an operator can act on,
/// and the refusal must name both the frozen code and the state that refused it.
///
/// **Why this distinguishes a wrong implementation:** `transition` is driven with
/// genuinely illegal `(status, command)` pairs — pairs §3 has no row for — so the
/// rejection is produced by the real refusal path, not a synthesised value. The
/// rendered form must contain the code and the state, so a `Display` that printed
/// only the command, or the `Debug` spelling of the code, fails.
#[test]
fn run_rejections_name_their_frozen_code_and_the_refusing_state() {
    // Pairs §3 has no row for: §3's STARTING exits are RUNNING/FAILED/
    // TIMED_OUT/CANCELLED/UNKNOWN, and no state accepts a bare Start.
    let illegal: &[(RunStatus, RunCommand)] = &[
        (RunStatus::Starting, RunCommand::Succeed),
        (RunStatus::Queued, RunCommand::ConfirmRunning),
        (RunStatus::Running, RunCommand::Start),
        (RunStatus::Succeeded, RunCommand::Timeout),
        (RunStatus::Failed, RunCommand::Cancel),
        (RunStatus::Cancelled, RunCommand::MarkUnknown),
        (RunStatus::Unknown, RunCommand::Fail),
        // A terminal Run must not be re-queued or started again.
        (RunStatus::Succeeded, RunCommand::Start),
        (RunStatus::TimedOut, RunCommand::ConfirmRunning),
    ];

    for (status, command) in illegal {
        let rejection = transition(*status, *command)
            .err()
            .unwrap_or_else(|| panic!("§3 must have no {status} × {command:?} row"));

        assert_eq!(
            rejection.current(),
            *status,
            "the rejection must record the state it refused"
        );
        assert_eq!(
            rejection.command(),
            *command,
            "the rejection must record the command it refused"
        );
        assert_eq!(
            rejection.code(),
            RunErrorCode::InvalidTransition,
            "§1 requires DOMAIN_INVALID_TRANSITION for a state with no row for the command"
        );
        // `rejection_code` returns the edge's §3 code, which is None precisely
        // because no edge exists for this pair.
        assert_eq!(
            rejection_code(*status, *command),
            None,
            "{status} × {command:?} has no §3 row, so it must report no edge code"
        );

        let rendered = rejection.to_string();
        assert!(
            rendered.contains(rejection.code().as_str()),
            "rejection text {rendered:?} omits the frozen code {}",
            rejection.code().as_str()
        );
        assert!(
            rendered.contains(status.as_str()),
            "rejection text {rendered:?} omits the refusing state {}",
            status.as_str()
        );
    }
}

// ============================================================== §2 Task table

/// `TaskCommand::as_str()` values double as permission-action strings
/// (`task.claim`, `review.approve`, …), so §2.1's command names are frozen. A
/// renamed command would silently stop matching an audited action string.
///
/// **Why this distinguishes a wrong implementation:** the expected names are
/// §2.1's 命令/Actor column verbatim, and the namespace assertion requires the
/// two review commands to live under `review.*` while the rest live under
/// `task.*`. A command moved between namespaces fails even though both spellings
/// "look right", and a renamed command fails the first assertion.
#[test]
fn task_command_names_match_the_section_2_1_command_column() {
    let expected: &[(TaskCommand, &str)] = &[
        (TaskCommand::MarkReady, "task.mark_ready"),
        (TaskCommand::Cancel, "task.cancel"),
        (TaskCommand::Claim, "task.claim"),
        (TaskCommand::Block, "task.block"),
        (TaskCommand::SubmitReview, "task.submit_review"),
        (TaskCommand::Complete, "task.complete"),
        (TaskCommand::ResolveBlock, "task.resolve_block"),
        (TaskCommand::ApproveReview, "review.approve"),
        (TaskCommand::RequestChanges, "review.request_changes"),
        (TaskCommand::Reopen, "task.reopen"),
    ];

    let mut names: Vec<&str> = Vec::new();
    for (command, wire) in expected {
        assert_eq!(
            command.as_str(),
            *wire,
            "{command:?} must be the §2.1 action string {wire}"
        );
        // §2.1 prints exactly two commands under the `review.` namespace.
        let expects_review = matches!(
            command,
            TaskCommand::ApproveReview | TaskCommand::RequestChanges
        );
        assert_eq!(
            command.as_str().starts_with("review."),
            expects_review,
            "{} is in the wrong action namespace for §2.1",
            command.as_str()
        );
        names.push(command.as_str());
    }
    names.sort();
    let before = names.len();
    names.dedup();
    assert_eq!(
        names.len(),
        before,
        "two §2.1 commands share an action string"
    );
}

/// §2.2 states its prohibitions as prose rather than as table rows, so a
/// table-driven test is most likely to miss them. Each must be refused with
/// `DOMAIN_INVALID_TRANSITION` **and** be absent from the table.
///
/// **Why this distinguishes a wrong implementation:** each pair is asserted to
/// have no row *and* to be refused, so an implementation that quietly added the
/// edge to make a happy-path test pass is caught by the first assertion, and one
/// that refuses with the wrong code is caught by the second. The status-set
/// assertion separately enforces "Task 无 FAILED 状态".
#[test]
fn section_2_2_prohibitions_are_refused_and_have_no_table_row() {
    let forbidden: &[(TaskStatus, TaskCommand, &str)] = &[
        // "TODO → DOING：必须先通过 readiness 校验进入 READY."
        (TaskStatus::Todo, TaskCommand::Claim, "TODO → DOING"),
        // "CANCELLED → *：P0 不恢复已取消 Task."
        (
            TaskStatus::Cancelled,
            TaskCommand::Reopen,
            "CANCELLED → READY",
        ),
        (
            TaskStatus::Cancelled,
            TaskCommand::Claim,
            "CANCELLED → DOING",
        ),
        (
            TaskStatus::Cancelled,
            TaskCommand::MarkReady,
            "CANCELLED → READY",
        ),
        // DONE leaves only via §2.1's `task.reopen` row.
        (TaskStatus::Done, TaskCommand::Claim, "DONE → DOING"),
        (TaskStatus::Done, TaskCommand::Cancel, "DONE → CANCELLED"),
        // §2.1 approves only from REVIEW.
        (
            TaskStatus::Doing,
            TaskCommand::ApproveReview,
            "DOING → DONE",
        ),
        (TaskStatus::Todo, TaskCommand::ApproveReview, "TODO → DONE"),
    ];

    for (status, command, label) in forbidden {
        assert!(
            task_transition(*status, *command).is_err(),
            "§2.2 forbids {label}, but the table has a row for it"
        );
        assert_eq!(
            task_rejection_code(*status, *command),
            None,
            "{label} has no §2.1 row, so no edge rejection code may be reported"
        );
        let rejection = task_transition(*status, *command).expect_err("{label} must be refused");
        assert_eq!(
            rejection.code(),
            TaskErrorCode::InvalidTransition,
            "{label} must be refused with DOMAIN_INVALID_TRANSITION"
        );
        let rendered = rejection.to_string();
        assert!(
            rendered.contains(TaskErrorCode::InvalidTransition.as_str()),
            "{label} rejection text {rendered:?} omits its frozen code"
        );
    }

    // §2.2: "任意状态 → FAILED：Task 无 FAILED 状态，失败属于 Run."
    for status in TaskStatus::ALL {
        assert_ne!(
            status.as_str(),
            "FAILED",
            "§2.2 states a Task has no FAILED state; failure belongs to Run"
        );
    }
    // `CANCELLED` is §2's P0 terminal: §2.1 reopens only `DONE → READY`.
    assert!(
        task_transition(TaskStatus::Cancelled, TaskCommand::Reopen).is_err(),
        "§2.2 forbids leaving CANCELLED in P0"
    );
}

/// `TASK_EDGES` is §2.1's table. Every row must be reachable through
/// `task_transition`, report the same row, and key the table uniquely.
///
/// **Why this distinguishes a wrong implementation:** the duplicate-key check is
/// a set comparison over `(from, command)` pairs derived from the table itself,
/// so a duplicated row fails even if both duplicates happen to agree on `to` —
/// and a row the transition function cannot decide fails the first assertion,
/// which is how a table and a decision function drifting apart becomes visible.
#[test]
fn task_edges_are_keyed_uniquely_and_match_the_transition_function() {
    let mut keys: Vec<(TaskStatus, TaskCommand)> = Vec::new();
    for edge in TASK_EDGES {
        let decided = task_transition(edge.from, edge.command)
            .unwrap_or_else(|_| panic!("§2.1 row {:?}→{:?} must be decidable", edge.from, edge.to));
        assert_eq!(decided.from(), edge.from);
        assert_eq!(decided.to(), edge.to);
        assert_eq!(decided.command(), edge.command);
        assert_eq!(
            decided.preconditions(),
            edge.preconditions,
            "{:?}→{:?} reports different preconditions than its §2.1 row",
            edge.from,
            edge.to
        );
        assert_eq!(
            decided.events(),
            edge.events,
            "{:?}→{:?} reports different events than its §2.1 row",
            edge.from,
            edge.to
        );
        keys.push((edge.from, edge.command));
    }

    keys.sort();
    let before = keys.len();
    keys.dedup();
    assert_eq!(
        keys.len(),
        before,
        "two §2.1 rows share a (from, command) key; transition would pick one arbitrarily"
    );
}

// ============================================================== §3 Permission matrix

/// A §3 compound cell must never resolve to `Allow` on its own. The table prints
/// two symbols (`A/H`, `A/S`, `R/S`) precisely because it declines to pick a
/// winner, so an application that has not proven the fact gets a denial —
/// §2 step 7 默认拒绝.
///
/// **Why this distinguishes a wrong implementation:** `resolve_cell` is driven
/// with an absent fact for every compound cell in `MATRIX`. An implementation
/// that defaulted the polarity to "first symbol applies" returns `Some(Allow)` for
/// the `A/H` and `A/S` cells and fails; so does one returning `Some(Deny)`
/// instead of `None`, because the caller needs `None` to tell "unproven" from
/// "refused" and to record the right reason.
#[test]
fn permission_matrix_compound_cells_never_default_to_allow() {
    let mut compound_cells = 0_usize;

    for (row, cells) in MATRIX.iter().enumerate() {
        for (column, cell) in cells.iter().enumerate() {
            let MatrixVerdict::ContextDependent(pair) = cell else {
                assert!(
                    !cell.is_context_dependent(),
                    "row {row} column {column}: a Simple cell must not claim to be \
                     context dependent"
                );
                continue;
            };
            compound_cells += 1;

            assert!(
                cell.is_context_dependent(),
                "row {row} column {column}: a ContextDependent cell must report \
                 is_context_dependent()"
            );

            // No fact supplied: §2 step 7 means the cell cannot be decided.
            let unproven: teamspace_domain::CompoundFacts = Default::default();
            assert_eq!(
                unproven.resolve_cell(*cell),
                None,
                "row {row} column {column}: a §3 compound cell resolved with no fact \
                 proven; absent facts must deny, never guess"
            );

            // Each polarity selects the positional symbol, and a proven `A/H`
            // still resolves to `H` when the bypass fact is false.
            assert_eq!(
                pair.resolve(true),
                pair.first,
                "row {row} column {column}: first_symbol_applies must select §3's first symbol"
            );
            assert_eq!(
                pair.resolve(false),
                pair.second,
                "row {row} column {column}: first_symbol_applies=false must select §3's second symbol"
            );
            assert_ne!(
                pair.first, pair.second,
                "row {row} column {column}: a compound cell printed the same symbol twice"
            );
        }
    }

    assert_eq!(
        compound_cells, 4,
        "§3 has exactly four compound cells (A/H, A/S, A/S, R/S); the matrix has {compound_cells}"
    );
}

/// Each `DiscriminatingFact` must expose the stable `condition_keys` token that
/// names it, and the three must be distinct.
///
/// **Why this distinguishes a wrong implementation:** the three tokens are
/// transcribed from §3's own "Harness 额外条件" column wording, not from
/// `condition_key()`. Two facts collapsing onto one token would make the denial
/// event unable to say which unproven fact caused it — the exact traceability
/// §7 requires of a refusal.
#[test]
fn discriminating_facts_expose_distinct_condition_keys() {
    let expected: &[(DiscriminatingFact, &str)] = &[
        // §3 "Human 可越过硬依赖" → the hard-dependency bypass fact.
        (
            DiscriminatingFact::HardDependencyBypass,
            "hard_dependency_bypass",
        ),
        // §3 "Reviewer 不能审批自己未获授权的高风险结果".
        (
            DiscriminatingFact::ReviewerAuthorizedForHighRiskResult,
            "reviewer_authorized_for_high_risk_result",
        ),
        // §3 "QA 默认只读代码" + "Task 授权路径 + 有效 fencing".
        (
            DiscriminatingFact::TaskAuthorizedCodePath,
            "task_authorized_code_path",
        ),
    ];

    let mut keys: Vec<&str> = Vec::new();
    for (fact, key) in expected {
        assert_eq!(
            fact.condition_key(),
            *key,
            "{fact:?} must report the §3 condition key {key}"
        );
        keys.push(fact.condition_key());
    }
    keys.sort();
    let before = keys.len();
    keys.dedup();
    assert_eq!(keys.len(), before, "two §3 facts share a condition key");
}

/// `VerdictPair::spec_symbol()` is the bridge back to §3's printed cell, so it
/// must reproduce the `A/H` form with the pair's symbols in §3's order.
///
/// **Why this distinguishes a wrong implementation:** the four pairs are built
/// from `SimpleVerdict` values and compared against §3's four literal cells, so a
/// `spec_symbol` that swapped the positional order renders `H/A` and fails.
#[test]
fn compound_cells_render_the_spec_slash_form_in_positional_order() {
    // §3's four compound cells, transcribed from the table.
    let expected: &[(&str, SimpleVerdict, SimpleVerdict, DiscriminatingFact)] = &[
        (
            "A/H",
            SimpleVerdict::Allow,
            SimpleVerdict::HumanOnly,
            DiscriminatingFact::HardDependencyBypass,
        ),
        (
            "A/S",
            SimpleVerdict::Allow,
            SimpleVerdict::Scoped,
            DiscriminatingFact::HardDependencyBypass,
        ),
        (
            "A/S",
            SimpleVerdict::Allow,
            SimpleVerdict::Scoped,
            DiscriminatingFact::ReviewerAuthorizedForHighRiskResult,
        ),
        (
            "R/S",
            SimpleVerdict::ReadOnly,
            SimpleVerdict::Scoped,
            DiscriminatingFact::TaskAuthorizedCodePath,
        ),
    ];

    let mut rendered: Vec<String> = Vec::new();
    for (printed, first, second, fact) in expected {
        let pair = VerdictPair {
            first: *first,
            second: *second,
            decided_by: *fact,
        };
        assert_eq!(
            pair.spec_symbol(),
            *printed,
            "the {first:?}/{second:?} pair must render as §3 printed it"
        );
        assert_eq!(
            MatrixVerdict::ContextDependent(pair).spec_symbol(),
            *printed,
            "MatrixVerdict must render a compound cell in §3's /-form"
        );
        assert!(
            MatrixVerdict::Simple(*first).spec_symbol().len() == 1,
            "a Simple cell renders one symbol, not a /-form"
        );
        rendered.push(pair.spec_symbol());
    }

    rendered.sort();
    rendered.dedup();
    assert_eq!(
        rendered.len(),
        3,
        "§3 prints three distinct compound cell forms (A/H, A/S, R/S)"
    );
}

/// §3's legend is five values, and `SimpleVerdict` must carry exactly those five
/// with their §3 symbols and the §3 polarity: only `A`/`S`/`R` permit an attempt,
/// while `H` and `—` do not.
///
/// **Why this distinguishes a wrong implementation:** the symbols are §3's legend
/// verbatim and `permits_attempt` is checked against §3's rule, so flipping its
/// polarity — treating the Human-only `H` as permitted — fails. That is the
/// §5 closure in miniature.
#[test]
fn simple_verdicts_match_the_section_3_legend() {
    let legend: &[(SimpleVerdict, &str, bool)] = &[
        // §3 图例: `A` 允许.
        (SimpleVerdict::Allow, "A", true),
        // §3: `S` 限作用域/状态 — permitted but narrowed.
        (SimpleVerdict::Scoped, "S", true),
        // §3: `R` 仅只读.
        (SimpleVerdict::ReadOnly, "R", true),
        // §3: `H` Human-only. §5 freezes eight categories that can never be
        // delegated, so it never permits an attempt.
        (SimpleVerdict::HumanOnly, "H", false),
        // §3: `—` 默认拒绝.
        (SimpleVerdict::Deny, "—", false),
    ];

    let mut symbols: Vec<&str> = Vec::new();
    for (verdict, symbol, permits) in legend {
        assert_eq!(
            verdict.symbol(),
            *symbol,
            "{verdict:?} must render as §3's legend symbol {symbol}"
        );
        assert_eq!(
            verdict.permits_attempt(),
            *permits,
            "{verdict:?} has the wrong §3 permits_attempt polarity"
        );
        symbols.push(verdict.symbol());
    }
    symbols.sort();
    let before = symbols.len();
    symbols.dedup();
    assert_eq!(symbols.len(), before, "two §3 legend values share a symbol");
}

/// `ActorKind::is_human` is the single gate in front of §5's Human-only list.
/// Only `HumanOwner` is Human: §5 says 「"Leader" 是 AI Role 时不视为 Human；
/// 岗位名不能提升权限」, so `RoleLeader` must not pass despite its title.
///
/// **Why this distinguishes a wrong implementation:** the assertion is a negative
/// one over the other five actor kinds, driven from `ActorKind::ALL`. An
/// implementation that returned `true` for `RoleLeader` — treating the job title
/// as authority — passes a "HumanOwner is human" test and fails here. That bug
/// would let an AI Role reach every Human-only action in §5.
#[test]
fn only_the_human_owner_counts_as_human() {
    assert_eq!(ActorKind::ALL.len(), 6, "§3's matrix has six columns");
    for actor in ActorKind::ALL {
        let expected = matches!(actor, ActorKind::HumanOwner);
        assert_eq!(
            actor.is_human(),
            expected,
            "{actor:?} has the wrong is_human() answer; §5 says a job title cannot \
             elevate an AI Role to Human"
        );
    }
    assert!(ActorKind::HumanOwner.is_human());
    // §3's column headers must be reproduced verbatim for a spec diff to work.
    let expected_columns = ["Owner", "Leader", "Product", "Developer", "QA", "System"];
    for (actor, column) in ActorKind::ALL.iter().zip(expected_columns) {
        assert_eq!(
            actor.column_name(),
            column,
            "§3's column header for {actor:?} must be {column}"
        );
    }
}

/// `ActorKind::same_as` is the `const`-callable equality the intersection's
/// `const fn`s use. It must agree with the derived `PartialEq`, or the same actor
/// would be two different actors depending on which path compared it.
///
/// **Why this distinguishes a wrong implementation:** every ordered pair in
/// `ActorKind::ALL` is compared (36 pairs). If `index()` disagreed with the
/// derived discriminant for any variant — a reordered `ALL`, say — at least one
/// pair diverges.
#[test]
fn actor_kind_same_as_agrees_with_derived_equality() {
    for left in ActorKind::ALL {
        for right in ActorKind::ALL {
            assert_eq!(
                left.same_as(right),
                left == right,
                "same_as disagreed with == for {left:?} vs {right:?}; the const and \
                 derived comparisons must not see different actors"
            );
        }
    }
}

// ============================================================== §2/§7 Permission intersection

/// §7 fixes the code a refusal reports. Each `DenialReason` family must reach
/// its own §7 code, and the two `AUTH_*` codes must stay distinguishable so the
/// UI can offer the right action (§9's 一一映射).
///
/// **Why this distinguishes a wrong implementation:** the groupings are
/// transcribed from §7's own three sentences — 权限拒绝 `AUTH_PERMISSION_DENIED`,
/// Human-only `AUTH_HUMAN_REQUIRED`, 越界路径 `WORKSPACE_PATH_OUTSIDE_SCOPE`. An
/// implementation that mapped `HumanOnly` to `PermissionDenied` would tell the UI
/// "you lack permission" when the real remedy is "ask a Human", and fails.
#[test]
fn section_7_maps_each_denial_family_to_its_own_code() {
    // §7: Human-only 返回 AUTH_HUMAN_REQUIRED.
    assert_eq!(
        DenialReason::HumanOnly.error_code(),
        PermissionErrorCode::HumanRequired,
        "§7 requires AUTH_HUMAN_REQUIRED for a Human-only refusal"
    );
    // §7: 越界路径返回 WORKSPACE_PATH_OUTSIDE_SCOPE.
    assert_eq!(
        DenialReason::PathOutsideScope.error_code(),
        PermissionErrorCode::PathOutsideScope,
        "§7 requires WORKSPACE_PATH_OUTSIDE_SCOPE for an out-of-scope path"
    );
    // §7: 权限拒绝返回 AUTH_PERMISSION_DENIED. §2 step 7 默认拒绝 is the same
    // refusal from the client's point of view.
    for reason in [
        DenialReason::NotPermitted,
        DenialReason::BindingOrFencingInvalid,
        DenialReason::ApprovalGrantInvalid,
        DenialReason::ApprovalBoundToAnotherBinding,
        DenialReason::CapabilityMissing,
        DenialReason::ObjectNotVisible,
        DenialReason::ScopeNotEstablished,
        DenialReason::DefaultDeny,
    ] {
        assert_eq!(
            reason.error_code(),
            PermissionErrorCode::PermissionDenied,
            "{reason:?} must report §7's AUTH_PERMISSION_DENIED"
        );
    }

    // §7's three codes must remain distinct, or the UI prompt is ambiguous.
    let mut sorted = vec![
        DenialReason::HumanOnly.error_code(),
        DenialReason::PathOutsideScope.error_code(),
        DenialReason::NotPermitted.error_code(),
    ];
    sorted.sort();
    let before = sorted.len();
    sorted.dedup();
    assert_eq!(
        sorted.len(),
        before,
        "§7 names three distinct refusal codes; two reasons collapsed onto one"
    );
}

/// §7 requires the refusal Event to carry a 脱敏原因 and **no** secret, path or
/// unauthorised content. `DenialKeys::of` is the only way to build that detail,
/// so every key it returns must be a fixed identifier taken from the frozen
/// vocabulary — never the caller's resource or path.
///
/// **Why this distinguishes a wrong implementation:** each returned key is
/// checked against `Layer::condition_key` / `DenialReason::condition_key`, and a
/// matrix-cell refusal (no term) must name the matrix namespace explicitly rather
/// than leaving the slot blank or inventing a term name.
#[test]
fn denial_keys_name_only_frozen_identifiers() {
    let reasons = [
        DenialReason::NotPermitted,
        DenialReason::HumanOnly,
        DenialReason::PathOutsideScope,
        DenialReason::ApprovalGrantInvalid,
        DenialReason::DefaultDeny,
    ];

    for reason in reasons {
        for layer in Layer::ALL {
            let keys = DenialKeys::of(reason, Some(layer));
            assert_eq!(
                keys[0],
                layer.condition_key(),
                "the first denial key must name the refusing §2 term"
            );
            assert_eq!(
                keys[1],
                reason.condition_key(),
                "the second denial key must name the reason"
            );
            for key in keys {
                assert!(
                    key.bytes().all(|byte| byte.is_ascii_lowercase()
                        || byte.is_ascii_digit()
                        || byte == b'_'),
                    "{key:?} is not a stable identifier; §7 forbids content in details"
                );
            }
        }

        // A refusal at the §3 matrix cell happened before any term was consulted,
        // so the namespace is recorded explicitly rather than left blank.
        let without_term = DenialKeys::of(reason, None);
        assert_eq!(
            without_term[0], "permission_matrix",
            "a matrix-cell refusal must name the matrix namespace explicitly"
        );
        assert_eq!(without_term[1], reason.condition_key());
    }
}

/// §2's intersection stops at the **first** refusal (任一层拒绝即终止), and a
/// permitted-but-narrowed request must report the term that narrowed it.
///
/// **Why this distinguishes a wrong implementation:** two terms refuse with
/// different reasons and §2's printed order decides which wins — ActorBasePermission
/// (index 1) precedes WorkspacePathPolicy (index 4). A "collect all denials" or
/// "last denial wins" implementation fails on the named term and reason. The
/// narrowing case separately requires `AllowScoped` to name its layer.
#[test]
fn the_first_refusal_wins_and_the_narrowing_term_is_reported() {
    // §3 prints `Workspace 写代码任务区` with Owner `A`, so the matrix cell permits
    // and the refusal must come from a term.
    let action = PermissionAction::WorkspaceWriteCodeArea;

    let mut terms = all_allow();
    terms[Layer::WorkspacePathPolicy.index()] = LayerVerdict::Deny(DenialReason::PathOutsideScope);
    terms[Layer::ActorBasePermission.index()] = LayerVerdict::Deny(DenialReason::NotPermitted);

    let decision = evaluate_permission(PermissionRequest {
        actor: ActorKind::HumanOwner,
        action,
        terms,
        facts: Default::default(),
    });
    assert!(
        !decision.permits(),
        "a request with two refusing terms must be refused"
    );
    match decision {
        PermissionDecision::Deny { layer, reason } => {
            assert_eq!(
                layer,
                Some(Layer::ActorBasePermission),
                "§2 prints ActorBasePermission before WorkspacePathPolicy, so it must \
                 be the refusal that terminates the scan"
            );
            assert_eq!(
                reason,
                DenialReason::NotPermitted,
                "the reported reason must be the first refusing term's reason"
            );
        }
        other => panic!("expected a denial, got {other:?}"),
    }

    // A single `Scoped` term narrows without refusing.
    let mut scoped = all_allow();
    scoped[Layer::WorkspacePathPolicy.index()] = LayerVerdict::Scoped;
    let narrowed = evaluate_permission(PermissionRequest {
        actor: ActorKind::HumanOwner,
        action,
        terms: scoped,
        facts: Default::default(),
    });
    match narrowed {
        PermissionDecision::AllowScoped { limited_by } => {
            assert_eq!(
                limited_by,
                Layer::WorkspacePathPolicy,
                "the narrowing term must be the one named for the audit record"
            );
        }
        other => panic!("expected AllowScoped, got {other:?}"),
    }
}

/// `LayerVerdict`'s two predicates decide how an application reads a term. `Deny`
/// must refuse, `Scoped` must narrow, and `NotApplicable` must do **neither** —
/// it neither permits nor refuses, so treating it as a refusal would deny every
/// action with no applicable term, and treating it as scoped would narrow them all.
///
/// **Why this distinguishes a wrong implementation:** all four variants are
/// checked against both booleans, so an implementation reporting `NotApplicable`
/// as scoped, or as denying, fails on a case a "does it compile" test never sees.
#[test]
fn layer_verdict_predicates_separate_refusal_narrowing_and_inapplicability() {
    let expectations = [
        // A term that refuses terminates the scan.
        (LayerVerdict::Deny(DenialReason::NotPermitted), true, false),
        // A term that narrows permits at a reduced scope.
        (LayerVerdict::Scoped, false, true),
        // "The term does not apply to this request… It neither permits nor
        // refuses."
        (LayerVerdict::NotApplicable, false, false),
        (LayerVerdict::Allow, false, false),
    ];

    for (verdict, denies, is_scoped) in expectations {
        assert_eq!(
            verdict.denies(),
            denies,
            "{verdict:?} has the wrong denies()"
        );
        assert_eq!(
            verdict.is_scoped(),
            is_scoped,
            "{verdict:?} has the wrong is_scoped()"
        );
        // §2: a term either refuses or narrows, never both.
        assert!(
            !(verdict.denies() && verdict.is_scoped()),
            "{verdict:?} cannot both refuse and narrow"
        );
    }
}

/// A Human-only cell must refuse every non-Human actor **before** any term is
/// consulted, and must admit the Human. This is §5's closure: no AI actor can
/// reach a Human-only action however the other terms answer.
///
/// **Why this distinguishes a wrong implementation:** the request supplies terms
/// that all permit, so an `Allow` could only come from the matrix cell being
/// skipped. The assertion also requires `layer: None`, because the refusal
/// happened at the cell and naming a term would misattribute it. The Human
/// control case proves the gate is not "always deny".
#[test]
fn a_human_only_cell_refuses_every_non_human_actor_before_any_term() {
    let mut human_only_cells_checked = 0_usize;

    for (row, cells) in MATRIX.iter().enumerate() {
        for (column, cell) in cells.iter().enumerate() {
            if !matches!(cell, MatrixVerdict::Simple(SimpleVerdict::HumanOnly)) {
                continue;
            }
            let action = PermissionAction::ALL[row];
            let actor = ActorKind::ALL[column];
            human_only_cells_checked += 1;

            assert_eq!(
                matrix_verdict(action, actor),
                *cell,
                "matrix_verdict disagrees with MATRIX at row {row} column {column}"
            );

            if actor.is_human() {
                // §1: the Human Owner is the only Human allowed to act.
                continue;
            }

            let decision = evaluate_permission(permitting_request(action, actor));
            match decision {
                PermissionDecision::Deny { layer, reason } => {
                    assert_eq!(
                        layer, None,
                        "the refusal happened at the §3 cell, so no §2 term refused it"
                    );
                    assert_eq!(reason, DenialReason::HumanOnly);
                    assert_eq!(
                        decision.error_code(),
                        Some(PermissionErrorCode::HumanRequired),
                        "§7 requires AUTH_HUMAN_REQUIRED for a Human-only refusal"
                    );
                }
                other => {
                    panic!("{actor:?} reached the Human-only cell for {action:?} and got {other:?}")
                }
            }
        }
    }

    assert!(
        human_only_cells_checked > 0,
        "§5 freezes a Human-only list, so the matrix must contain such cells"
    );

    // The Human Owner is admitted on a §3 `H` row — the gate must not be a
    // blanket deny.
    let owner_admitted = evaluate_permission(permitting_request(
        PermissionAction::TeamArchiveRestore,
        ActorKind::HumanOwner,
    ));
    assert!(
        owner_admitted.permits(),
        "§3 gives Team 归档/恢复 to Owner as `H`, and the Human Owner must be admitted"
    );
}

/// §2 step 7 默认拒绝: a term that gives no permitting verdict denies the request,
/// and the four terms §2 marks as mandatory cannot be skipped.
///
/// **Why this distinguishes a wrong implementation:** the `NotApplicable` cases
/// are the four terms `evaluate_permission` treats as mandatory — TeamPolicy,
/// ActorBasePermission, RolePolicy for non-Human/non-System actors, and the
/// default-deny path. An implementation that let a missing TeamPolicy verdict
/// pass would let a request through with no team policy consulted at all.
#[test]
fn an_absent_mandatory_term_verdict_is_a_default_deny() {
    let action = PermissionAction::WorkspaceWriteCodeArea;
    let mut terms = all_allow();

    // TeamPolicy is §2's first term and is mandatory for every actor.
    terms[Layer::TeamPolicy.index()] = LayerVerdict::NotApplicable;
    let decision = evaluate_permission(PermissionRequest {
        actor: ActorKind::HumanOwner,
        action,
        terms,
        facts: Default::default(),
    });
    match decision {
        PermissionDecision::Deny { layer, reason } => {
            assert_eq!(layer, Some(Layer::TeamPolicy));
            assert_eq!(
                reason,
                DenialReason::DefaultDeny,
                "§2 step 7 names the absent verdict 默认拒绝"
            );
        }
        other => panic!("a missing TeamPolicy verdict must deny, got {other:?}"),
    }

    // RolePolicy is mandatory for Role actors but not for Human/System.
    let mut terms = all_allow();
    terms[Layer::RolePolicy.index()] = LayerVerdict::NotApplicable;
    for actor in [
        ActorKind::RoleLeader,
        ActorKind::RoleDeveloper,
        ActorKind::RoleQa,
    ] {
        let decision = evaluate_permission(PermissionRequest {
            actor,
            action,
            terms,
            facts: Default::default(),
        });
        assert!(
            !decision.permits(),
            "{actor:?} must supply a RolePolicy verdict; §2 makes it mandatory for Role actors"
        );
    }
    for actor in [ActorKind::HumanOwner, ActorKind::System] {
        let decision = evaluate_permission(PermissionRequest {
            actor,
            action,
            terms,
            facts: Default::default(),
        });
        assert!(
            decision.permits(),
            "§2 prints RolePolicy as 适用时（Role/Binding actor 时）, so {actor:?} \
             need not supply one"
        );
    }
}

/// `condition_keys()` must be `None` for a permitted decision — a permitted
/// action records nothing — and populated for a refusal, which is what §7's
/// auditable-denial requirement and Gate G7 depend on.
///
/// **Why this distinguishes a wrong implementation:** the two permitted variants
/// (`Allow` and `AllowScoped`) are checked separately, so an implementation that
/// attached condition keys to a narrowed-but-permitted decision fails, and so does
/// one that returned `None` on a denial.
#[test]
fn condition_keys_are_recorded_for_refusals_and_omitted_for_permits() {
    let permitted = evaluate_permission(permitting_request(
        PermissionAction::TaskCreate,
        ActorKind::HumanOwner,
    ));
    assert!(permitted.permits());
    assert_eq!(
        permitted.condition_keys(),
        None,
        "a permitted decision must record no denial details"
    );
    assert_eq!(
        permitted.error_code(),
        None,
        "a permitted decision reports no §7 code"
    );

    let mut scoped_terms = all_allow();
    scoped_terms[Layer::WorkspacePathPolicy.index()] = LayerVerdict::Scoped;
    let narrowed = evaluate_permission(PermissionRequest {
        actor: ActorKind::HumanOwner,
        action: PermissionAction::WorkspaceWriteCodeArea,
        terms: scoped_terms,
        facts: Default::default(),
    });
    assert!(narrowed.permits(), "a Scoped term still permits");
    assert_eq!(
        narrowed.condition_keys(),
        None,
        "AllowScoped is a permit, so it records no denial details"
    );

    let mut denied_terms = all_allow();
    denied_terms[Layer::ValidApprovalGrant.index()] =
        LayerVerdict::Deny(DenialReason::ApprovalGrantInvalid);
    let refused = evaluate_permission(PermissionRequest {
        actor: ActorKind::HumanOwner,
        action: PermissionAction::RunCancel,
        terms: denied_terms,
        facts: Default::default(),
    });
    assert!(!refused.permits());
    assert_eq!(
        refused.error_code(),
        Some(PermissionErrorCode::PermissionDenied),
        "§7 requires AUTH_PERMISSION_DENIED for a permission refusal"
    );
    let keys = refused
        .condition_keys()
        .expect("§7 requires a refusal to record condition_keys for diagnosis");
    assert_eq!(keys[0], Layer::ValidApprovalGrant.condition_key());
    assert_eq!(keys[1], DenialReason::ApprovalGrantInvalid.condition_key());
}

// ============================================================== §4 Idempotency

/// `IdempotencyState::as_str()` values are stored in `command_idempotency`,
/// whose `CHECK` constraint migration `0022` freezes as
/// `('IN_PROGRESS', 'SUCCEEDED', 'FAILED')`. A domain spelling the column rejects
/// would make every idempotent command fail at the storage boundary.
///
/// **Why this distinguishes a wrong implementation:** the expected strings are
/// the migration's own `CHECK` list, transcribed from
/// `crates/persistence-sqlite/migrations/0022_command_idempotency.sql`. Renaming
/// `FailedStably` to `FAILED_STABLY`, for instance, produces a value the column
/// rejects — a drift no domain-only unit test would see.
#[test]
fn idempotency_state_spellings_match_the_frozen_migration() {
    // migration 0022_command_idempotency.sql:
    //   state TEXT NOT NULL CHECK (state IN ('IN_PROGRESS','SUCCEEDED','FAILED'))
    let migration_states = ["IN_PROGRESS", "SUCCEEDED", "FAILED"];

    let expected: &[(IdempotencyState, &str)] = &[
        // DATA_CONVENTIONS §4 row 1: 首次仍处理中.
        (IdempotencyState::InProgress, "IN_PROGRESS"),
        // §4 row 2: 首次成功.
        (IdempotencyState::Succeeded, "SUCCEEDED"),
        // §4 row 3: 首次稳定业务失败 — stored as FAILED, not FAILED_STABLY.
        (IdempotencyState::FailedStably, "FAILED"),
    ];

    for (state, spelling) in expected {
        assert_eq!(
            state.as_str(),
            *spelling,
            "{state:?} must use migration 0022's stored spelling {spelling}"
        );
        assert!(
            migration_states.contains(&state.as_str()),
            "{} is not one of migration 0022's CHECK values",
            state.as_str()
        );
    }

    let mut spellings: Vec<&str> = expected.iter().map(|(state, _)| state.as_str()).collect();
    spellings.sort();
    let before = spellings.len();
    spellings.dedup();
    assert_eq!(
        spellings.len(),
        before,
        "two idempotency states share a stored spelling"
    );
}

/// §1 types `idempotency_key` as a stable string/UUID and §4 makes it the fourth
/// part of the record key. An empty or oversized key identifies no logical
/// operation, so it must be refused at the value boundary — otherwise every caller
/// using an empty key shares one record and one replay decision.
///
/// **Why this distinguishes a wrong implementation:** the bound is asserted from
/// both sides — the maximum accepted length is accepted and one byte more is
/// refused. A `>=` instead of `>` (rejecting a legal 128-byte key) or an ignored
/// bound both fail.
#[test]
fn idempotency_keys_and_refs_reject_empty_and_oversized_values() {
    // The domain's own bound is 128 bytes for both the key and the result
    // reference; asserted from the accepted and rejected sides.
    const BOUND: usize = 128;

    let at_bound = "k".repeat(BOUND);
    let over_bound = "k".repeat(BOUND + 1);

    assert!(
        IdempotencyKeyString::parse(&at_bound).is_ok(),
        "a {BOUND}-byte idempotency key is inside the bound and must be accepted"
    );
    assert_eq!(
        IdempotencyKeyString::parse(&over_bound).unwrap_err(),
        ValueObjectError::EmptyField,
        "a {}-byte idempotency key exceeds the bound",
        BOUND + 1
    );
    assert_eq!(
        IdempotencyKeyString::parse("").unwrap_err(),
        ValueObjectError::EmptyField,
        "an empty idempotency key identifies no logical operation"
    );

    assert!(ResultRef::parse(&at_bound).is_ok());
    assert_eq!(
        ResultRef::parse(&over_bound).unwrap_err(),
        ValueObjectError::EmptyField,
        "an oversized result reference must be refused"
    );
    assert_eq!(
        ResultRef::parse("").unwrap_err(),
        ValueObjectError::EmptyField,
        "an empty result reference cannot name a first outcome"
    );

    // `CommandName` carries a stricter alphabet than the key: §4 names commands,
    // and a name carrying a space or a path separator is not a stable identifier.
    assert!(
        CommandName::parse("task.cancel").is_ok(),
        "§2.1's own command names must be accepted"
    );
    for bad in ["", "run retry", "run/retry", "run;retry", "run\tretry"] {
        assert_eq!(
            CommandName::parse(bad).unwrap_err(),
            ValueObjectError::EmptyField,
            "{bad:?} is not a stable command name"
        );
    }
    let at_command_bound = "c".repeat(64);
    assert!(CommandName::parse(&at_command_bound).is_ok());
    assert_eq!(
        CommandName::parse(&"c".repeat(65)).unwrap_err(),
        ValueObjectError::EmptyField,
        "an oversized command name must be refused"
    );
}

/// `ActorFingerprint` is §4's second key part, and its only frozen requirement
/// is **stability** — two requests from the same actor must produce equal
/// fingerprints, or replay detection silently stops working. It must also
/// separate the four actor kinds, so a Role cannot replay as a Human.
///
/// **Why this distinguishes a wrong implementation:** the test asserts
/// determinism *and* cross-kind separation. An implementation that omitted the
/// kind byte, or returned a fresh random value, fails the first; one that
/// collapsed `Actor::Role` and `Actor::Human` fails the second — which is the case
/// that would let a Role replay a Human's command under the same key.
#[test]
fn actor_fingerprints_are_stable_per_actor_and_distinct_per_kind() {
    let human = Actor::Human(HumanId::from_uuid(uuid(1)));
    let role = Actor::Role(RoleId::from_uuid(uuid(1)));

    // Stability: the same actor always fingerprints identically.
    assert_eq!(
        ActorFingerprint::of_actor(human).as_bytes(),
        ActorFingerprint::of_actor(human).as_bytes(),
        "two requests from the same Human must produce equal fingerprints"
    );
    assert_eq!(
        ActorFingerprint::of_actor(role).as_bytes(),
        ActorFingerprint::of_actor(role).as_bytes(),
        "two requests from the same Role must produce equal fingerprints"
    );

    // Separation: the same underlying UUID under a different actor kind is a
    // different actor and must not share a fingerprint.
    let kinds = [
        ActorFingerprint::of_actor(human),
        ActorFingerprint::of_actor(role),
        ActorFingerprint::of_actor(Actor::System),
        ActorFingerprint::of_actor(Actor::Adapter(AdapterId::from_uuid(uuid(1)))),
    ];
    let mut unique: Vec<[u8; 32]> = kinds.iter().map(|f| *f.as_bytes()).collect();
    unique.sort();
    let before = unique.len();
    unique.dedup();
    assert_eq!(
        unique.len(),
        before,
        "two actor kinds share a fingerprint; a Role could replay as a Human"
    );

    // §4 stores the fingerprint as a fixed-width part of the record key.
    assert_eq!(
        ActorFingerprint::of_actor(Actor::System).as_bytes().len(),
        32,
        "§4's fingerprint part is a fixed-width value"
    );

    // A different UUID under the *same* kind must also separate, or two distinct
    // Roles would share one replay decision.
    let other_role = Actor::Role(RoleId::from_uuid(uuid(9)));
    assert_ne!(
        ActorFingerprint::of_actor(role).as_bytes(),
        ActorFingerprint::of_actor(other_role).as_bytes(),
        "two different Roles must not share a fingerprint"
    );
}

/// `evaluate_replay` must answer §4's rows without ever authorising a second
/// execution. `Proceed` is reachable **only** when no record exists, so a
/// finished record carrying no replayable outcome still refuses.
///
/// **Why this distinguishes a wrong implementation:** the second half is the
/// hazard — a `SUCCEEDED` record with a missing `result_ref` must not fall
/// through to `Proceed`, because that re-runs a completed command and duplicates
/// its Event/Run/Artifact (§4 row 2: 不重复 Event/Run/Artifact). The well-formed
/// control then proves the function does replay when it legitimately can.
#[test]
fn only_a_missing_record_authorises_a_second_execution() {
    let key = record_key("key-1", "run.retry");
    let call = IdempotentCall {
        key,
        payload_hash: PayloadHash::from_bytes([7; 32]),
    };

    let outcome = evaluate_replay(None, call);
    assert!(
        outcome.authorises_execution(),
        "§4 permits a first attempt: no record means nothing has run"
    );
    assert_eq!(outcome, IdempotencyOutcome::Proceed);

    // A finished record with no replayable outcome must still refuse.
    for state in [IdempotencyState::Succeeded, IdempotencyState::FailedStably] {
        let stored = IdempotencyRecord {
            key,
            payload_hash: PayloadHash::from_bytes([7; 32]),
            state,
            first_outcome: None,
        };
        let outcome = evaluate_replay(Some(stored), call);
        assert!(
            !outcome.authorises_execution(),
            "a {state:?} record without an outcome must not authorise a second \
             execution; §4 row 2 forbids duplicate Event/Run/Artifact"
        );
        assert!(
            !matches!(outcome, IdempotencyOutcome::Proceed),
            "{state:?} without an outcome fell through to Proceed"
        );
    }

    // A well-formed succeeded record replays the first result and reports no new
    // error code — §4 row 2 returns the first result.
    let stored = IdempotencyRecord {
        key,
        payload_hash: PayloadHash::from_bytes([7; 32]),
        state: IdempotencyState::Succeeded,
        first_outcome: Some(FirstOutcome::Succeeded {
            result_ref: ResultRef::parse("result-1").expect("valid ref"),
        }),
    };
    let replayed = evaluate_replay(Some(stored), call);
    assert!(
        !replayed.authorises_execution(),
        "a succeeded record must replay, not re-execute"
    );
    assert_eq!(
        replayed.error_code(),
        None,
        "§4 row 2 returns the first result, not an error code"
    );
    match replayed {
        IdempotencyOutcome::ReturnFirstResult { result_ref } => {
            assert_eq!(
                result_ref.as_str(),
                "result-1",
                "§4 row 2 must return the *first* result reference verbatim"
            );
        }
        other => panic!("expected the first result to be replayed, got {other:?}"),
    }

    // A record stored under a *different* key is a caller bug, not a replay, and
    // must never answer with this key's decision.
    let other_key = record_key("key-2", "run.retry");
    let crossed = evaluate_replay(
        Some(IdempotencyRecord {
            key: other_key,
            payload_hash: PayloadHash::from_bytes([7; 32]),
            state: IdempotencyState::Succeeded,
            first_outcome: Some(FirstOutcome::Succeeded {
                result_ref: ResultRef::parse("other-result").expect("valid ref"),
            }),
        }),
        call,
    );
    assert!(
        !crossed.authorises_execution(),
        "a record under a different key must not authorise work for this key"
    );
}

/// A replay must report the code the catalogue pairs with its situation, and the
/// three sources must stay distinguishable so an operator can tell a busy
/// resource from an idempotency conflict from a replayed business failure.
///
/// **Why this distinguishes a wrong implementation:** the expected codes are
/// `ERROR_CATALOG.md` §4's `CONCURRENCY_RESOURCE_BUSY`,
/// `CONCURRENCY_IDEMPOTENCY_CONFLICT` and the first attempt's own stable code. An
/// implementation reporting the conflict code for a still-running first attempt
/// would tell the client to mint a **new** key — precisely the double execution
/// §4 exists to prevent — and fails.
#[test]
fn replay_reports_the_catalogued_code_for_each_row_of_section_4() {
    let key = record_key("key-3", "message.retry");
    let hash = PayloadHash::from_bytes([9; 32]);
    let call = IdempotentCall {
        key,
        payload_hash: hash,
    };

    // Row 1: 首次仍处理中 → the retryable busy code, not a conflict.
    let busy = evaluate_replay(
        Some(IdempotencyRecord {
            key,
            payload_hash: hash,
            state: IdempotencyState::InProgress,
            first_outcome: None,
        }),
        call,
    );
    assert_eq!(
        busy.error_code(),
        Some(IdempotencyReportedCode::Busy(
            IdempotencyBusyCode::ResourceBusy
        )),
        "§4 row 1 must report the busy code so the client waits for the same result"
    );
    assert_eq!(
        busy.error_code().map(|code| code.to_string()),
        Some("CONCURRENCY_RESOURCE_BUSY".to_string()),
        "ERROR_CATALOG.md §4 fixes this spelling"
    );

    // Row 3: 首次稳定业务失败 → the first attempt's own stable code, verbatim.
    let failed = evaluate_replay(
        Some(IdempotencyRecord {
            key,
            payload_hash: hash,
            state: IdempotencyState::FailedStably,
            first_outcome: Some(FirstOutcome::FailedStably {
                error_code: "TASK_REVIEW_REQUIRED",
            }),
        }),
        call,
    );
    assert_eq!(
        failed.error_code(),
        Some(IdempotencyReportedCode::FirstFailure(
            "TASK_REVIEW_REQUIRED"
        )),
        "§4 row 3 replays the first stable failure verbatim"
    );
    assert_eq!(
        failed.error_code().map(|code| code.to_string()),
        Some("TASK_REVIEW_REQUIRED".to_string()),
        "§4 row 3 must not substitute a different code for the first failure"
    );

    // Row 4: 同 key + 不同 payload → the conflict code, in every state. The
    // payload comparison precedes the state comparison, so a *succeeded* record
    // still conflicts rather than answering with a stale success.
    for state in [
        IdempotencyState::InProgress,
        IdempotencyState::Succeeded,
        IdempotencyState::FailedStably,
    ] {
        let stored = IdempotencyRecord {
            key,
            payload_hash: hash,
            state,
            first_outcome: match state {
                IdempotencyState::Succeeded => Some(FirstOutcome::Succeeded {
                    result_ref: ResultRef::parse("result-2").expect("valid ref"),
                }),
                IdempotencyState::FailedStably => Some(FirstOutcome::FailedStably {
                    error_code: "TASK_REVIEW_REQUIRED",
                }),
                IdempotencyState::InProgress => None,
            },
        };
        let conflict = evaluate_replay(
            Some(stored),
            IdempotentCall {
                key,
                payload_hash: PayloadHash::from_bytes([0xAA; 32]),
            },
        );
        assert_eq!(
            conflict.error_code(),
            Some(IdempotencyReportedCode::Conflict(
                IdempotencyErrorCode::Conflict
            )),
            "§4 row 4 applies to state {state:?} too"
        );
        assert_eq!(
            conflict.error_code().map(|code| code.to_string()),
            Some("CONCURRENCY_IDEMPOTENCY_CONFLICT".to_string()),
            "ERROR_CATALOG.md §4 fixes this spelling"
        );
        assert!(
            !conflict.authorises_execution(),
            "a payload conflict must never authorise execution"
        );
    }
}

// ============================================================== §6 Run termination

/// §3's cancel preconditions differ per status, so `decide_cancel` must answer
/// differently for the same observation depending on where the Run is. A terminal
/// Run can never be cancelled again, and `UNKNOWN` can only reach `CANCELLED`
/// through a reconcile.
///
/// **Why this distinguishes a wrong implementation:** one observation is held
/// fixed and the status varied, so an implementation ignoring the status — or one
/// collapsing `ReconcileOnly` into `NotCancellable` — fails. Each branch is
/// asserted against §3's own row, not against the decision's Debug spelling.
#[test]
fn cancel_decision_depends_on_the_status_not_only_the_observation() {
    let confirmed = CancelObservation::Confirmed;

    for status in [RunStatus::Queued, RunStatus::Starting, RunStatus::Running] {
        let decision = decide_cancel(status, confirmed);
        assert!(
            matches!(decision, CancelDecision::Confirm { .. }),
            "{status} with a confirmed cancel must apply §3's row, got {decision:?}"
        );
    }

    // §3 closes the four terminal states: SUCCEEDED/FAILED/TIMED_OUT/CANCELLED
    // 为单个 Run 终态, and no `→ CANCELLED` row exists out of any of them.
    for status in [
        RunStatus::Succeeded,
        RunStatus::Failed,
        RunStatus::TimedOut,
        RunStatus::Cancelled,
    ] {
        assert!(
            matches!(
                decide_cancel(status, confirmed),
                CancelDecision::NotCancellable { .. }
            ),
            "{status} is a §3 terminal state and must report NotCancellable"
        );
    }

    // §3's UNKNOWN → CANCELLED row is triggered by the reconcile service, not by
    // a cancel command, so the two answers must stay distinct.
    for status in RunStatus::ALL {
        let decision = decide_cancel(status, confirmed);
        if status == RunStatus::Unknown {
            assert!(
                matches!(decision, CancelDecision::ReconcileOnly { .. }),
                "§3 says UNKNOWN reaches CANCELLED only via reconcile"
            );
        } else {
            assert!(
                !matches!(decision, CancelDecision::ReconcileOnly { .. }),
                "{status} has a plain cancel row and must not be reported reconcile-only"
            );
        }
    }

    // A merely *requested* cancel has not been confirmed, so §3's precondition
    // does not hold and the lease must not be released. The refusal code differs
    // per row and §3's table is the only thing that decides it: `STARTING` names
    // `RUN_CANCEL_NOT_CONFIRMED`, while `QUEUED` and `RUNNING` name
    // `AUTH_PERMISSION_DENIED`.
    for (status, expected_code) in [
        (RunStatus::Starting, RunErrorCode::RunCancelNotConfirmed),
        (RunStatus::Running, RunErrorCode::PermissionDenied),
        (RunStatus::Queued, RunErrorCode::PermissionDenied),
    ] {
        let decision = decide_cancel(status, CancelObservation::Requested);
        match decision {
            CancelDecision::Refuse { error_code } => assert_eq!(
                error_code, expected_code,
                "§3's {status} → CANCELLED row names {expected_code}"
            ),
            other => {
                panic!("{status} with an unconfirmed cancel request must be refused, got {other:?}")
            }
        }
    }

    // §3's QUEUED row accepts 未开始 as well as 已安全停止, so a Run that never
    // started is confirmed on a different observation than a RUNNING Run needs.
    assert!(
        matches!(
            decide_cancel(RunStatus::Queued, CancelObservation::NeverStarted),
            CancelDecision::Confirm { .. }
        ),
        "§3 QUEUED → CANCELLED accepts a Run that never started"
    );
    // A force fence is §3's STARTING row's second alternative and a takeover
    // fence is RUNNING's; neither substitutes for the other.
    assert!(
        matches!(
            decide_cancel(RunStatus::Starting, CancelObservation::ForceFenceReached),
            CancelDecision::Confirm { .. }
        ),
        "§3 STARTING → CANCELLED accepts a reached force-fence condition"
    );
    assert!(
        matches!(
            decide_cancel(RunStatus::Running, CancelObservation::TakeoverFenced),
            CancelDecision::Confirm { .. }
        ),
        "§3 RUNNING → CANCELLED accepts a Human takeover that fenced first"
    );
}

/// `ReconcileEvidence` is what distinguishes "still running" from "cannot prove it
/// running". `blocks_timeout` must be true for both positive findings and false
/// only for `CouldNotProve`, because §3's `RUNNING → TIMED_OUT` row requires
/// 心跳/总时长超限**且对账仍无法证明运行**.
///
/// **Why this distinguishes a wrong implementation:** the assertion pins all
/// three evidence kinds against §3's prose. An implementation blocking the timeout
/// only on `ProvedRunning` would time out a Run whose state is merely 事实不明 —
/// the case §3 says must go to reconcile instead — and fails.
#[test]
fn reconcile_evidence_blocks_timeout_unless_it_could_not_prove_running() {
    assert!(
        ReconcileEvidence::ProvedRunning.blocks_timeout(),
        "a proved-running Run must not be timed out"
    );
    assert!(
        ReconcileEvidence::Indeterminate.blocks_timeout(),
        "an indeterminate Run is 事实不明 and must be reconciled, not timed out"
    );
    assert!(
        !ReconcileEvidence::CouldNotProve.blocks_timeout(),
        "only the 'cannot prove running' finding unblocks §3's TIMED_OUT row"
    );
    assert_eq!(
        ReconcileEvidence::ALL.len(),
        3,
        "§3's RUNNING → TIMED_OUT precondition has exactly three evidence states"
    );
}

/// The five cancel observations must stay distinct, because each corresponds to a
/// different §3 cancellation route. Collapsing two would let one route's evidence
/// satisfy another's precondition.
///
/// **Why this distinguishes a wrong implementation:** the enumeration length and
/// distinctness are both asserted, and the never-started observation is
/// distinguished from a merely requested one on a `QUEUED` Run — the two are
/// different §3 preconditions (未开始 vs cancel 确认).
#[test]
fn cancel_observations_stay_distinct_and_drive_different_decisions() {
    assert_eq!(
        CancelObservation::ALL.len(),
        5,
        "§3 names five distinct cancellation observations"
    );

    let all = CancelObservation::ALL;
    for (index, left) in all.iter().enumerate() {
        for right in &all[index + 1..] {
            assert_ne!(
                left, right,
                "two cancel observations are the same value; one route's evidence \
                 could satisfy another's precondition"
            );
        }
    }

    // A merely requested cancel must not release the lease on a QUEUED Run:
    // §3's row needs 未开始**或**已安全停止.
    assert!(
        matches!(
            decide_cancel(RunStatus::Queued, CancelObservation::Requested),
            CancelDecision::Refuse { .. }
        ),
        "a requested-but-unconfirmed cancel does not satisfy §3's QUEUED precondition"
    );

    // The three Running-only and Starting-only routes must not be interchangeable.
    assert!(
        matches!(
            decide_cancel(RunStatus::Queued, CancelObservation::ForceFenceReached),
            CancelDecision::Refuse { .. }
        ),
        "§3's QUEUED row never mentions fencing; a force fence is not its precondition"
    );
    assert!(
        matches!(
            decide_cancel(RunStatus::Running, CancelObservation::ForceFenceReached),
            CancelDecision::Refuse { .. }
        ),
        "§3's RUNNING row names a takeover fence, not an ordinary force fence"
    );

    // `ExecutionObservation` is the timeout side's evidence type and must
    // enumerate all four Running combinations, so a fourth evidence state is
    // visible as a missing case rather than a silent gap.
    assert_eq!(
        ExecutionObservation::ALL.len(),
        14,
        "§3's timeout rows need 2 Starting cases plus 3 evidence states times 4 \
         Running deadline combinations"
    );
}

// ============================================================== §7 Approval

/// §7 freezes exactly five Approval states and states 全部终态不可重开, so the
/// status set must match and no terminal state may have an outgoing edge.
///
/// **Why this distinguishes a wrong implementation:** the status assertion is set
/// equality against §7's own list, so a `REVOKED` status or a missing `EXPIRED`
/// fails. The edge assertions are driven from §7's four commands over the four
/// terminal states, so a re-decision path added back fails.
#[test]
fn approval_states_are_exactly_section_7_and_terminals_cannot_reopen() {
    // §7: `PENDING → APPROVED / REJECTED / EXPIRED / CANCELLED`.
    let mut expected = vec!["PENDING", "APPROVED", "REJECTED", "EXPIRED", "CANCELLED"];
    expected.sort();
    let mut actual: Vec<&str> = ApprovalStatus::ALL.iter().map(|s| s.as_str()).collect();
    actual.sort();
    assert_eq!(
        actual, expected,
        "§7 freezes exactly five Approval states; a sixth would invent protocol"
    );

    // §7: 全部终态不可重开；新请求必须新建 Approval.
    for status in [
        ApprovalStatus::Approved,
        ApprovalStatus::Rejected,
        ApprovalStatus::Expired,
        ApprovalStatus::Cancelled,
    ] {
        for command in ApprovalCommand::ALL {
            assert!(
                teamspace_domain::approval_transition(status, command).is_err(),
                "§7 makes {status} terminal, so {command:?} must have no outgoing edge"
            );
        }
    }

    // Each PENDING row is decided by exactly one command, and each lands on its
    // own state — §7's four arrows.
    let mut destinations: Vec<ApprovalStatus> = Vec::new();
    for command in ApprovalCommand::ALL {
        let decided = teamspace_domain::approval_transition(ApprovalStatus::Pending, command)
            .unwrap_or_else(|_| panic!("§7 gives PENDING a row for {command:?}"));
        destinations.push(decided.to());
    }
    destinations.sort();
    destinations.dedup();
    assert_eq!(
        destinations.len(),
        4,
        "§7 sends PENDING to four distinct destinations, one per command"
    );
    assert!(destinations.contains(&ApprovalStatus::Approved));
    assert!(destinations.contains(&ApprovalStatus::Rejected));
    assert!(destinations.contains(&ApprovalStatus::Expired));
    assert!(destinations.contains(&ApprovalStatus::Cancelled));
}

/// Approval rejection codes must be codes `ERROR_CATALOG.md` §2/§3/§4 actually
/// list, because §7's approval rows name them as 主要拒绝码.
///
/// **Why this distinguishes a wrong implementation:** the expected strings are
/// transcribed from the catalogue. An invented approval code leaves the UI without
/// an actionable prompt, which §9 forbids, and fails the first assertion.
#[test]
fn approval_rejection_codes_are_catalogued() {
    let codes = [
        ApprovalErrorCode::InvalidTransition,
        ApprovalErrorCode::ScopeInvalid,
        ApprovalErrorCode::HumanRequired,
        ApprovalErrorCode::ConcurrencyVersionConflict,
        ApprovalErrorCode::PreconditionFailed,
    ];
    for code in codes {
        let wire = code.as_str();
        assert!(
            wire.starts_with("APPROVAL_")
                || wire.starts_with("AUTH_")
                || wire.starts_with("CONCURRENCY_")
                || wire.starts_with("DOMAIN_"),
            "{code:?} reports {wire}, which no ERROR_CATALOG.md section names"
        );
        assert!(
            wire.bytes()
                .all(|byte| byte.is_ascii_uppercase() || byte.is_ascii_digit() || byte == b'_'),
            "{wire} is not a stable catalogue code spelling"
        );
        assert_eq!(
            serde_json::to_string(&code).unwrap(),
            format!("\"{wire}\""),
            "serde and as_str disagree for {code:?}; the catalogue spelling is a \
             wire contract"
        );
    }

    // ERROR_CATALOG.md §3 fixes each of these spellings; §7's approval rows name
    // them as 主要拒绝码, so they are wire contract, not debug strings.
    assert_eq!(
        ApprovalErrorCode::ScopeInvalid.as_str(),
        "APPROVAL_SCOPE_INVALID",
        "ERROR_CATALOG.md §3 names this code for an incomplete or over-Team scope"
    );
    assert_eq!(
        ApprovalErrorCode::PreconditionFailed.as_str(),
        "DOMAIN_PRECONDITION_FAILED",
        "ERROR_CATALOG.md §2 fixes this spelling; §7's `—` row derives it"
    );
    assert_eq!(
        ApprovalErrorCode::HumanRequired.as_str(),
        "AUTH_HUMAN_REQUIRED",
        "ERROR_CATALOG.md §3 fixes this spelling"
    );
    assert_eq!(
        ApprovalErrorCode::ConcurrencyVersionConflict.as_str(),
        "CONCURRENCY_VERSION_CONFLICT",
        "ERROR_CATALOG.md §4 fixes this spelling"
    );
    assert_eq!(
        ApprovalErrorCode::InvalidTransition.as_str(),
        "DOMAIN_INVALID_TRANSITION",
        "ERROR_CATALOG.md §2 fixes this spelling"
    );
}

// ============================================================== §1/§2 value objects

/// `DATA_CONVENTIONS` §1 fixes the protocol UUID form: lower-case, hyphenated,
/// canonical. Every non-canonical form must be refused, because two spellings of
/// one id would reach the same object as two rows.
///
/// **Why this distinguishes a wrong implementation:** each case is a distinct
/// malformation — wrong length, wrong hyphen positions, upper case, non-hex. A
/// parser that only checked the length, or only the hyphens, passes a narrower
/// test and fails here. The accepted control uses a real UUIDv7, so the test
/// cannot pass by rejecting everything.
#[test]
fn uuid_rejects_every_noncanonical_protocol_form() {
    let canonical = uuid(0x0A);
    let text = canonical.to_protocol();
    assert_eq!(text.len(), 36, "§1 fixes the canonical UUID length at 36");
    assert_eq!(text, text.to_lowercase(), "§1 requires the lower-case form");
    assert_eq!(
        UuidV7::parse_protocol(&text).expect("canonical form parses"),
        canonical,
        "the canonical form must round-trip"
    );

    let short = &text[..35];
    let long = format!("{text}0");
    let no_hyphens = text.replace('-', "0");
    let upper = text.to_uppercase();
    let non_hex = text.replace('0', "z");
    let rejected = [
        ("", "empty string"),
        ("0199a1b2", "too short"),
        (short, "35 characters"),
        (long.as_str(), "37 characters"),
        (no_hyphens.as_str(), "hyphens in the wrong places"),
        (upper.as_str(), "upper case"),
        (non_hex.as_str(), "non-hexadecimal characters"),
    ];

    for (bad, label) in rejected {
        assert_eq!(
            UuidV7::parse_protocol(bad).unwrap_err(),
            ValueObjectError::InvalidUuidV7,
            "{label} ({bad:?}) is not §1's canonical form and must be refused"
        );
    }
}

/// §2 fixes storage and protocol time as UTC RFC 3339 with exactly three
/// fractional digits. Every other shape must be refused.
///
/// **Why this distinguishes a wrong implementation:** the cases are distinct
/// malformations of one frozen format, and the accepted control proves the parser
/// is not simply rejecting everything. A parser accepting a `+08:00` offset would
/// let a local-time instant into a UTC column, which §2 forbids ("存储与协议一律
/// UTC").
#[test]
fn timestamps_reject_every_noncanonical_rfc3339_form() {
    assert_eq!(timestamp().to_rfc3339_millis(), CANONICAL_INSTANT);

    let rejected = [
        // Not UTC.
        "2026-10-08T12:34:56.789+08:00",
        // No fractional digits.
        "2026-10-08T12:34:56Z",
        // One or two fractional digits.
        "2026-10-08T12:34:56.7Z",
        "2026-10-08T12:34:56.78Z",
        // Four fractional digits.
        "2026-10-08T12:34:56.7890Z",
        // Space instead of the RFC 3339 `T` separator.
        "2026-10-08 12:34:56.789Z",
        // A date that does not exist: 2026 is not a leap year.
        "2026-02-30T00:00:00.000Z",
        // Month 13 and day 0.
        "2026-13-01T00:00:00.000Z",
        "2026-10-00T00:00:00.000Z",
    ];

    for bad in rejected {
        assert_eq!(
            UtcTimestamp::parse_rfc3339_millis(bad).unwrap_err(),
            ValueObjectError::InvalidRfc3339Millis,
            "{bad:?} is not §2's canonical UTC millisecond form"
        );
    }
}

/// A leap day is a legitimate instant and §2's supported range is bounded. Both
/// must be accepted, and every value must round-trip — this exercises the civil
/// calendar arithmetic itself, not the parser's shape checks.
///
/// **Why this distinguishes a wrong implementation:** a civil-calendar bug that
/// mis-handles leap years or the Unix epoch offset shows up as a round-trip
/// failure on one of these instants. The list walks a leap year, a common year and
/// the range boundaries, so an off-by-one in the day-count arithmetic fails.
#[test]
fn leap_days_and_range_boundaries_round_trip_exactly() {
    // A leap day exists in a leap year and not in a common year.
    assert!(
        UtcTimestamp::parse_rfc3339_millis("2024-02-29T23:59:59.999Z").is_ok(),
        "2024 is a leap year, so 2024-02-29 exists"
    );
    assert_eq!(
        UtcTimestamp::parse_rfc3339_millis("2026-02-29T00:00:00.000Z").unwrap_err(),
        ValueObjectError::InvalidRfc3339Millis,
        "2026 is not a leap year, so 2026-02-29 does not exist"
    );

    for instant in [
        "1970-01-01T00:00:00.000Z",
        "2000-02-29T12:00:00.000Z",
        "2024-02-29T00:00:00.000Z",
        "2026-10-08T00:00:00.000Z",
        "2026-10-08T23:59:59.999Z",
    ] {
        let parsed = UtcTimestamp::parse_rfc3339_millis(instant)
            .unwrap_or_else(|error| panic!("{instant} should parse: {error}"));
        assert_eq!(
            parsed.to_rfc3339_millis(),
            instant,
            "{instant} did not round-trip; the civil calendar arithmetic is wrong"
        );
        assert_eq!(
            UtcTimestamp::from_unix_millis(parsed.unix_millis())
                .expect("the same instant re-derives")
                .to_rfc3339_millis(),
            instant,
            "{instant} did not survive the Unix-millisecond storage form §2 names"
        );
    }

    // §2's supported range: RFC 3339's four-digit year bounds.
    for inside in ["0001-01-01T00:00:00.000Z", "9999-12-31T23:59:59.999Z"] {
        assert!(
            UtcTimestamp::parse_rfc3339_millis(inside).is_ok(),
            "{inside} is inside §2's supported range"
        );
    }
}

/// `UuidV7` must carry the RFC 4122 version and variant bits and put the timestamp
/// in its leading 48 bits, because `DATA_CONVENTIONS` §1 requires UUIDv7 so ids
/// are time-ordered.
///
/// **Why this distinguishes a wrong implementation:** the assertions read the raw
/// bytes at the version and variant positions and compare the timestamp prefix of
/// two ids built from different instants. An implementation that dropped `0x70 | …`
/// yields a UUIDv4-shaped value that sorts differently, and one that ignored the
/// timestamp yields two ids sharing a prefix.
#[test]
fn uuid_construction_sets_the_version_variant_and_timestamp_prefix() {
    let id = uuid(0xFF);
    let bytes = id.database_bytes();

    assert_eq!(
        bytes[6] >> 4,
        0x7,
        "UUIDv7 must set the version nibble to 7, got {:#04x}",
        bytes[6] >> 4
    );
    assert_eq!(
        bytes[8] >> 6,
        0b10,
        "UUIDv7 must set the RFC 4122 variant bits, got {:#04x}",
        bytes[8] >> 6
    );

    let earlier = uuid(0x00);
    let later = UuidV7::from_timestamp_and_random(1_700_000_000_001, [0; 10])
        .expect("fixture timestamp is in range");
    assert_ne!(
        earlier.database_bytes()[..6],
        later.database_bytes()[..6],
        "§1 requires UUIDv7 so the leading 48 bits carry the timestamp and ids sort by time"
    );

    // A timestamp outside the supported range is refused, not truncated.
    assert_eq!(
        UuidV7::from_timestamp_and_random(0xFFFF_FFFF_FFFF_FFFF, [0; 10]).unwrap_err(),
        ValueObjectError::TimestampOutOfRange,
        "a timestamp beyond the RFC 3339 range must be refused, not truncated"
    );

    // The database and protocol forms are the same value in §1's two spellings.
    assert_eq!(
        UuidV7::from_database_bytes(id.database_bytes()).expect("round-trip"),
        id,
        "§1 stores 16 bytes and prints the canonical string; the two must be one value"
    );
}

/// `Version` starts at 1 and only increments; §3 fixes 可变聚合初始 `version=1`
/// and 每次成功业务变更加 1, and an overflow must be reported rather than wrapped.
///
/// **Why this distinguishes a wrong implementation:** the zero construction is
/// refused and the maximum version refuses to increment. A `checked_add` replaced
/// by `+ 1` would wrap `u64::MAX` to 0 and turn a live object into a version-0
/// object — the overflow assertion catches it.
#[test]
fn versions_start_at_one_refuse_zero_and_refuse_to_overflow() {
    assert_eq!(
        Version::initial().get(),
        1,
        "DATA_CONVENTIONS §3 fixes the initial aggregate version at 1"
    );
    assert_eq!(
        Version::initial().next().expect("1 increments").get(),
        2,
        "§3 increments by exactly 1 on a successful business change"
    );
    assert_eq!(
        Version::new(0).unwrap_err(),
        ValueObjectError::ZeroVersion,
        "§3 forbids version 0; an object at version 0 has no generation"
    );
    assert_eq!(
        Version::new(u64::MAX)
            .expect("valid version")
            .next()
            .unwrap_err(),
        ValueObjectError::VersionOverflow,
        "an increment past the maximum must be reported, never wrapped to 0"
    );

    // §3: a mismatched `expected_version` returns CONCURRENCY_VERSION_CONFLICT
    // and is never auto-overwritten, so `matches` must be exact equality.
    let version = Version::new(8).expect("valid version");
    assert!(
        ExpectedVersion::new(version).matches(version),
        "a matching expected_version satisfies §3's precondition"
    );
    assert!(
        !ExpectedVersion::new(version).matches(Version::new(7).expect("valid version")),
        "§3 refuses a mismatched expected_version rather than overwriting"
    );
    assert!(
        !ExpectedVersion::new(version).matches(Version::new(9).expect("valid version")),
        "§3 has no tolerance for a *newer* version either"
    );
}

// ============================================================== cross-cutting wire names

/// Every frozen status spelling is uppercase and underscore separated, because
/// the migrations store these values in `CHECK` constraints and §1/§2 forbid
/// inventing a different spelling at the boundary.
///
/// **Why this distinguishes a wrong implementation:** the assertion is a property
/// over all variants of four status enums rather than a table of literals, so a
/// newly added variant with a lowercase or hyphenated spelling fails without the
/// test needing to know the variant exists.
#[test]
fn every_status_spelling_is_the_uppercase_underscore_form_migrations_store() {
    let families: Vec<(&str, Vec<&str>)> = vec![
        (
            "TaskStatus",
            TaskStatus::ALL.iter().map(|s| s.as_str()).collect(),
        ),
        (
            "ApprovalStatus",
            ApprovalStatus::ALL.iter().map(|s| s.as_str()).collect(),
        ),
        (
            "AttemptStatus",
            AttemptStatus::ALL.iter().map(|s| s.as_str()).collect(),
        ),
        (
            "RunStatus",
            RunStatus::ALL.iter().map(|s| s.as_str()).collect(),
        ),
    ];

    for (family, spellings) in families {
        let mut seen: Vec<&str> = Vec::new();
        for spelling in spellings {
            assert!(
                !spelling.is_empty(),
                "{family} has a variant with an empty stored spelling"
            );
            assert!(
                spelling
                    .bytes()
                    .all(|byte| byte.is_ascii_uppercase() || byte.is_ascii_digit() || byte == b'_'),
                "{family} variant {spelling:?} is not the uppercase underscore form the \
                 migrations store"
            );
            seen.push(spelling);
        }
        seen.sort();
        let before = seen.len();
        seen.dedup();
        assert_eq!(
            seen.len(),
            before,
            "{family} has two variants with the same stored spelling"
        );
    }
}

/// The serde wire form of each status must equal its `as_str()` in both
/// directions. A mismatch means one spelling reaches the database and another
/// reaches the audit log, so a grep for a code misses half the record.
///
/// **Why this distinguishes a wrong implementation:** both directions are checked
/// with a round-trip through `from_str`, and an out-of-set wire name must be
/// refused rather than silently mapped onto another variant. An implementation
/// that added a `#[serde(rename)]` to one variant without updating `as_str` — the
/// DEV-02-11 Q-04/Q-05 trap this module documents — fails.
#[test]
fn serde_wire_form_matches_as_str_in_both_directions() {
    for (status, wire) in [
        (TaskStatus::Todo, "TODO"),
        (TaskStatus::Ready, "READY"),
        (TaskStatus::Cancelled, "CANCELLED"),
    ] {
        assert_eq!(status.as_str(), wire);
        assert_eq!(
            serde_json::to_string(&status).unwrap(),
            format!("\"{wire}\""),
            "serde and as_str disagree for {status:?}"
        );
        let parsed: TaskStatus = serde_json::from_str(&format!("\"{wire}\""))
            .unwrap_or_else(|error| panic!("{wire} must deserialize: {error}"));
        assert_eq!(parsed, status, "{wire} did not survive a serde round-trip");
    }

    assert_eq!(ApprovalStatus::Pending.as_str(), "PENDING");
    assert_eq!(
        serde_json::to_string(&ApprovalStatus::Pending).unwrap(),
        "\"PENDING\"",
        "serde and as_str disagree for ApprovalStatus::Pending"
    );
    assert_eq!(
        serde_json::from_str::<ApprovalStatus>("\"PENDING\"").expect("round-trips"),
        ApprovalStatus::Pending
    );

    assert_eq!(AttemptStatus::InFlight.as_str(), "IN_FLIGHT");
    assert_eq!(
        serde_json::to_string(&AttemptStatus::InFlight).unwrap(),
        "\"IN_FLIGHT\"",
        "serde and as_str disagree for AttemptStatus::InFlight"
    );
    assert_eq!(
        serde_json::from_str::<AttemptStatus>("\"IN_FLIGHT\"").expect("round-trips"),
        AttemptStatus::InFlight
    );

    assert_eq!(RunStatus::Unknown.as_str(), "UNKNOWN");
    assert_eq!(
        serde_json::to_string(&RunStatus::Unknown).unwrap(),
        "\"UNKNOWN\"",
        "serde and as_str disagree for RunStatus::Unknown"
    );
    assert_eq!(
        serde_json::from_str::<RunStatus>("\"UNKNOWN\"").expect("round-trips"),
        RunStatus::Unknown
    );

    for (code, wire) in [
        (
            TaskErrorCode::InvalidTransition,
            "DOMAIN_INVALID_TRANSITION",
        ),
        (
            TaskErrorCode::RunCancelNotConfirmed,
            "RUN_CANCEL_NOT_CONFIRMED",
        ),
    ] {
        assert_eq!(code.as_str(), wire);
        assert_eq!(
            serde_json::to_string(&code).unwrap(),
            format!("\"{wire}\""),
            "serde and as_str disagree for {code:?}"
        );
    }

    // A wire name outside the frozen set must be refused rather than silently
    // mapped onto some other variant.
    assert!(
        serde_json::from_str::<TaskStatus>("\"ARCHIVED\"").is_err(),
        "a Task status outside §2's set must not deserialize"
    );
    assert!(
        serde_json::from_str::<RunStatus>("\"PAUSED\"").is_err(),
        "a Run status outside §3's set must not deserialize"
    );
}
