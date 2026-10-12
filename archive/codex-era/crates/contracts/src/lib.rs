#![forbid(unsafe_code)]

//! Rust-owned DTOs exported to TypeScript through JSON Schema.
//!
//! Do not duplicate these wire types in frontend packages. Add a Rust DTO,
//! include its schema in the generator, and regenerate `packages/contracts-ts`.

use std::collections::BTreeMap;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// The stable error response shared by typed UI/IPC boundaries.
///
/// Error-code-specific detail keys remain governed by the frozen error catalog.
/// This transport type deliberately does not invent a second error-code enum.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ErrorEnvelope {
    /// Stable, machine-readable code from the frozen error catalog.
    pub code: String,
    /// Localizable, human-readable message with no secrets or backtraces.
    pub message: String,
    /// Whether the same idempotent intent may be retried after remediation.
    pub retryable: bool,
    /// Stable field path for validation errors; otherwise `null`.
    pub field: Option<String>,
    /// Canonical UUIDv7 used to correlate diagnostics.
    pub correlation_id: String,
    /// Whitelisted, redacted details defined per error code.
    pub details: BTreeMap<String, serde_json::Value>,
}
