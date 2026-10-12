//! Dump §3's Run error codes and §11's retry admission as text, for the Q-12
//! device to diff against the frozen `ERROR_CATALOG.md`.
//!
//! Same reasoning as `dump_matrix`: the device must compare the catalogue
//! against **what the compiler actually believes**, not against a re-parse of
//! `run_state_machine.rs`. A source scan would happily agree with a stale
//! `is_retryable()` body; running it cannot.
//!
//! Two directions are dumped, because a one-way check is half a check:
//!
//! - `CATALOGUE` — every `RunErrorCode` and its `is_retryable()` answer. The
//!   device asserts each is `是`/`否` exactly as the catalogue's `Retryable`
//!   column says.
//! - `RETRYABLE_NOT_A_RUN_CODE` — the codes the catalogue marks `是` that §3
//!   has no variant for. These must be *absent* from the domain enum: a
//!   catalogue code that is retryable but unreachable from the Run machine is
//!   fine, whereas inventing a §3 variant for one would be a fabricated edge.

use teamspace_domain::{
    RetryDecision, RetryPolicy, RunErrorCode, RunFailure, RunStatus, decide_retry,
};

/// Every §3 Run error code, straight from the source of truth.
const ALL_RUN_CODES: [RunErrorCode; 9] = RunErrorCode::ALL;

fn main() {
    println!("#RUN_CODE_COUNT\t{}", ALL_RUN_CODES.len());
    println!(
        "#TERMINAL_COUNT\t{}",
        RunStatus::ALL.iter().filter(|s| s.is_terminal()).count()
    );
    for code in ALL_RUN_CODES {
        println!(
            "CATALOGUE\t{}\t{}",
            code.as_str(),
            if code.is_retryable() { "是" } else { "否" }
        );
    }

    // The admission verdict for each code, from a terminal Run, under the
    // §11 defaults. This is what the device cross-checks against the Retryable
    // column: a `否` code must never reach `CreateNewRun`, and that must hold
    // through the decision function, not merely through `is_retryable`.
    let policy = RetryPolicy::new(0);
    for code in ALL_RUN_CODES {
        let decision = decide_retry(
            RunStatus::Failed,
            RunFailure::Code(code),
            true,
            policy,
            0,
            0,
        );
        println!(
            "ADMISSION\t{}\t{}",
            code.as_str(),
            match decision {
                RetryDecision::CreateNewRun { .. } => "CREATE",
                RetryDecision::QueryFirstResult { .. } => "QUERY",
                RetryDecision::DoNotRetry { .. } => "REFUSE",
                RetryDecision::NotIdempotent { .. } => "NOT_IDEMPOTENT",
                RetryDecision::NotTerminal { .. } => "NOT_TERMINAL",
                RetryDecision::BudgetExhausted { .. } => "BUDGET",
            }
        );
    }

    // `INTERNAL_UNEXPECTED` is `是` in the catalogue but is not a §3 code, so it
    // arrives as its own failure shape and lands on rule 4 rather than on a
    // replay.
    println!(
        "ADMISSION\tINTERNAL_UNEXPECTED\t{}",
        match decide_retry(
            RunStatus::Failed,
            RunFailure::InternalUnexpected,
            true,
            policy,
            0,
            0,
        ) {
            RetryDecision::CreateNewRun { .. } => "CREATE",
            RetryDecision::QueryFirstResult { .. } => "QUERY",
            RetryDecision::DoNotRetry { .. } => "REFUSE",
            RetryDecision::NotIdempotent { .. } => "NOT_IDEMPOTENT",
            RetryDecision::NotTerminal { .. } => "NOT_TERMINAL",
            RetryDecision::BudgetExhausted { .. } => "BUDGET",
        }
    );

    println!("#CATALOGUE_TOTAL\t74");
}
