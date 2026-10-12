#![forbid(unsafe_code)]

use std::{fmt, process::ExitStatus, time::Duration};

use tokio::time::Instant;

use crate::{
    CancellationToken, CapturedStderr, StdoutEvent, SupervisedAdapter, SupervisorError,
    json::JsonValue,
    semver::{SemVer, SemVerError},
};

/// Frozen protocol version for Adapter V0.
pub const PROTOCOL_VERSION_V0: &str = "0.1.0";

/// Parameters for sending an `initialize` JSON-RPC request to an Adapter.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InitializeParams {
    pub protocol_version: String,
    pub application_version: String,
    pub harness_instance_id: String,
    pub platform: String,
    pub locale: String,
    pub configuration_generation: Option<u64>,
    pub credential_references: Vec<String>,
    pub correlation_id: String,
}

impl InitializeParams {
    #[must_use]
    pub fn new(
        application_version: impl Into<String>,
        harness_instance_id: impl Into<String>,
        platform: impl Into<String>,
        locale: impl Into<String>,
        correlation_id: impl Into<String>,
    ) -> Self {
        Self {
            protocol_version: PROTOCOL_VERSION_V0.to_string(),
            application_version: application_version.into(),
            harness_instance_id: harness_instance_id.into(),
            platform: platform.into(),
            locale: locale.into(),
            configuration_generation: None,
            credential_references: Vec::new(),
            correlation_id: correlation_id.into(),
        }
    }

    #[must_use]
    pub fn with_protocol_version(mut self, version: impl Into<String>) -> Self {
        self.protocol_version = version.into();
        self
    }

    #[must_use]
    pub fn with_configuration_generation(mut self, generation: u64) -> Self {
        self.configuration_generation = Some(generation);
        self
    }

    #[must_use]
    pub fn with_credential_references(mut self, refs: Vec<String>) -> Self {
        self.credential_references = refs;
        self
    }
}

/// Negotiated authentication state from `initialize`.
///
/// NOTE: Plaintext credentials are strictly prohibited from appearing in protocol messages.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuthStatus {
    NotRequired,
    Ready,
    Missing,
    Invalid,
    Expired,
}

impl AuthStatus {
    #[must_use]
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "not_required" => Some(Self::NotRequired),
            "ready" => Some(Self::Ready),
            "missing" => Some(Self::Missing),
            "invalid" => Some(Self::Invalid),
            "expired" => Some(Self::Expired),
            _ => None,
        }
    }

    #[must_use]
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::NotRequired => "not_required",
            Self::Ready => "ready",
            Self::Missing => "missing",
            Self::Invalid => "invalid",
            Self::Expired => "expired",
        }
    }
}

/// Execution isolation mode reported by the Adapter.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IsolationMode {
    Process,
    ConfigDirectory,
    Container,
    External,
}

impl IsolationMode {
    #[must_use]
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "process" => Some(Self::Process),
            "config_directory" => Some(Self::ConfigDirectory),
            "container" => Some(Self::Container),
            "external" => Some(Self::External),
            _ => None,
        }
    }

    #[must_use]
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Process => "process",
            Self::ConfigDirectory => "config_directory",
            Self::Container => "container",
            Self::External => "external",
        }
    }
}

/// Session recovery mode supported by the Adapter.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecoveryMode {
    None,
    StatusQuery,
    SessionResume,
}

impl RecoveryMode {
    #[must_use]
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "none" => Some(Self::None),
            "status_query" => Some(Self::StatusQuery),
            "session_resume" => Some(Self::SessionResume),
            _ => None,
        }
    }

    #[must_use]
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::None => "none",
            Self::StatusQuery => "status_query",
            Self::SessionResume => "session_resume",
        }
    }
}

/// Validated success response for `initialize`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InitializeResult {
    pub protocol_version: SemVer,
    pub adapter_version: SemVer,
    pub harness_version: Option<String>,
    pub capabilities: Vec<String>,
    pub auth_status: AuthStatus,
    pub isolation_mode: IsolationMode,
    pub recovery_mode: RecoveryMode,
}

impl InitializeResult {
    #[must_use]
    pub fn has_capability(&self, cap: &str) -> bool {
        self.capabilities.iter().any(|c| c == cap)
    }
}

/// Parameters for sending a `health.check` request.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HealthCheckParams {
    pub correlation_id: String,
    pub include_diagnostics: bool,
}

impl HealthCheckParams {
    #[must_use]
    pub fn new(correlation_id: impl Into<String>) -> Self {
        Self {
            correlation_id: correlation_id.into(),
            include_diagnostics: false,
        }
    }

    #[must_use]
    pub fn with_diagnostics(mut self, include: bool) -> Self {
        self.include_diagnostics = include;
        self
    }
}

/// Health status reported by the Adapter.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HealthStatus {
    Healthy,
    Degraded,
    Unavailable,
    AuthFailed,
    Incompatible,
}

impl HealthStatus {
    #[must_use]
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "healthy" => Some(Self::Healthy),
            "degraded" => Some(Self::Degraded),
            "unavailable" => Some(Self::Unavailable),
            "auth_failed" => Some(Self::AuthFailed),
            "incompatible" => Some(Self::Incompatible),
            _ => None,
        }
    }

    #[must_use]
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Healthy => "healthy",
            Self::Degraded => "degraded",
            Self::Unavailable => "unavailable",
            Self::AuthFailed => "auth_failed",
            Self::Incompatible => "incompatible",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArtifactReference {
    pub kind: String,
    pub uri: String,
    pub version: Option<String>,
    pub checksum: Option<String>,
    pub media_type: Option<String>,
}

impl ArtifactReference {
    #[must_use]
    pub fn new(kind: impl Into<String>, uri: impl Into<String>) -> Self {
        Self {
            kind: kind.into(),
            uri: uri.into(),
            version: None,
            checksum: None,
            media_type: None,
        }
    }

    #[must_use]
    pub fn new_artifact(uri: impl Into<String>) -> Self {
        Self::new("artifact", uri)
    }

    #[must_use]
    pub fn with_version(mut self, version: impl Into<String>) -> Self {
        self.version = Some(version.into());
        self
    }

    #[must_use]
    pub fn with_checksum(mut self, checksum: impl Into<String>) -> Self {
        self.checksum = Some(checksum.into());
        self
    }

    #[must_use]
    pub fn with_media_type(mut self, media_type: impl Into<String>) -> Self {
        self.media_type = Some(media_type.into());
        self
    }
}

/// Validated success response for `health.check`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HealthResult {
    pub status: HealthStatus,
    pub checked_at: String,
    pub diagnostic_codes: Vec<String>,
    pub details_reference: Option<ArtifactReference>,
}

impl HealthResult {
    #[must_use]
    pub fn is_healthy(&self) -> bool {
        matches!(self.status, HealthStatus::Healthy)
    }
}

/// Normalized JSON-RPC error returned by an Adapter.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AdapterRpcError {
    pub code: i32,
    pub message: String,
    pub domain_code: Option<String>,
    pub retryable: bool,
    pub correlation_id: Option<String>,
    pub reason_key: Option<String>,
}

impl fmt::Display for AdapterRpcError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "JSON-RPC error {}: {} (domain: {:?})",
            self.code, self.message, self.domain_code
        )
    }
}

impl std::error::Error for AdapterRpcError {}

/// Failure during the `initialize` handshake.
#[derive(Debug)]
pub enum HandshakeError {
    /// Adapter reported an incompatible version or invalid SemVer (-32010 / ADAPTER_INCOMPATIBLE).
    Incompatible {
        code: i32,
        domain_code: &'static str,
        message: String,
        reported_version: String,
    },
    /// Response format or required fields violated protocol specification (-32099 / ADAPTER_PROTOCOL_VIOLATION).
    ProtocolViolation {
        code: i32,
        domain_code: &'static str,
        message: String,
    },
    /// Connection or pipe unavailable (-32017 / ADAPTER_UNAVAILABLE).
    Unavailable {
        code: i32,
        domain_code: &'static str,
        message: String,
    },
    /// JSON-RPC error returned by Adapter.
    RpcError(AdapterRpcError),
    /// Supervisor process / IO error.
    Supervisor(SupervisorError),
    /// Child process terminated unexpectedly before handshake completed.
    ProcessExited {
        code: i32,
        domain_code: &'static str,
        message: String,
        exit_status: Option<ExitStatus>,
        stderr: CapturedStderr,
    },
    /// Handshake exceeded the specified timeout.
    TimedOut,
}

impl HandshakeError {
    #[must_use]
    pub fn jsonrpc_code(&self) -> i32 {
        match self {
            Self::Incompatible { code, .. }
            | Self::ProtocolViolation { code, .. }
            | Self::Unavailable { code, .. }
            | Self::ProcessExited { code, .. } => *code,
            Self::TimedOut => -32017,
            Self::RpcError(err) => err.code,
            Self::Supervisor(_) => -32017,
        }
    }

    #[must_use]
    pub fn domain_code(&self) -> &str {
        match self {
            Self::Incompatible { domain_code, .. }
            | Self::ProtocolViolation { domain_code, .. }
            | Self::Unavailable { domain_code, .. }
            | Self::ProcessExited { domain_code, .. } => domain_code,
            Self::TimedOut => "ADAPTER_UNAVAILABLE",
            Self::RpcError(err) => err
                .domain_code
                .as_deref()
                .unwrap_or("ADAPTER_PROTOCOL_VIOLATION"),
            Self::Supervisor(_) => "ADAPTER_UNAVAILABLE",
        }
    }

    #[must_use]
    pub fn is_retryable(&self) -> bool {
        match self {
            Self::Incompatible { .. } | Self::ProtocolViolation { .. } => false,
            Self::Unavailable { .. } | Self::ProcessExited { .. } | Self::TimedOut => true,
            Self::RpcError(err) => err.retryable,
            Self::Supervisor(_) => true,
        }
    }
}

impl fmt::Display for HandshakeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Incompatible {
                message,
                reported_version,
                ..
            } => {
                write!(
                    f,
                    "handshake incompatible: {message} (reported: {reported_version})"
                )
            }
            Self::ProtocolViolation { message, .. } => {
                write!(f, "handshake protocol violation: {message}")
            }
            Self::Unavailable { message, .. } => {
                write!(f, "handshake unavailable: {message}")
            }
            Self::RpcError(err) => write!(f, "handshake rpc error: {err}"),
            Self::Supervisor(err) => write!(f, "handshake supervisor error: {err}"),
            Self::ProcessExited {
                message,
                exit_status,
                ..
            } => {
                write!(
                    f,
                    "handshake process exited: {message} (status: {exit_status:?})"
                )
            }
            Self::TimedOut => write!(f, "handshake timed out"),
        }
    }
}

impl std::error::Error for HandshakeError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::RpcError(err) => Some(err),
            Self::Supervisor(err) => Some(err),
            _ => None,
        }
    }
}

/// Failure during `health.check`.
#[derive(Debug)]
pub enum HealthCheckError {
    /// Response format or required fields violated protocol specification (-32099 / ADAPTER_PROTOCOL_VIOLATION).
    ProtocolViolation {
        code: i32,
        domain_code: &'static str,
        message: String,
    },
    /// Connection or pipe unavailable (-32017 / ADAPTER_UNAVAILABLE).
    Unavailable {
        code: i32,
        domain_code: &'static str,
        message: String,
    },
    /// JSON-RPC error returned by Adapter.
    RpcError(AdapterRpcError),
    /// Supervisor process / IO error.
    Supervisor(SupervisorError),
    /// Child process terminated unexpectedly before health check completed.
    ProcessExited {
        code: i32,
        domain_code: &'static str,
        message: String,
        exit_status: Option<ExitStatus>,
        stderr: CapturedStderr,
    },
    /// Health check exceeded the specified timeout.
    TimedOut,
}

impl HealthCheckError {
    #[must_use]
    pub fn jsonrpc_code(&self) -> i32 {
        match self {
            Self::ProtocolViolation { code, .. }
            | Self::Unavailable { code, .. }
            | Self::ProcessExited { code, .. } => *code,
            Self::TimedOut => -32017,
            Self::RpcError(err) => err.code,
            Self::Supervisor(_) => -32017,
        }
    }

    #[must_use]
    pub fn domain_code(&self) -> &str {
        match self {
            Self::ProtocolViolation { domain_code, .. }
            | Self::Unavailable { domain_code, .. }
            | Self::ProcessExited { domain_code, .. } => domain_code,
            Self::TimedOut => "ADAPTER_UNAVAILABLE",
            Self::RpcError(err) => err
                .domain_code
                .as_deref()
                .unwrap_or("ADAPTER_PROTOCOL_VIOLATION"),
            Self::Supervisor(_) => "ADAPTER_UNAVAILABLE",
        }
    }

    #[must_use]
    pub fn is_retryable(&self) -> bool {
        match self {
            Self::ProtocolViolation { .. } => false,
            Self::Unavailable { .. } | Self::ProcessExited { .. } | Self::TimedOut => true,
            Self::RpcError(err) => err.retryable,
            Self::Supervisor(_) => true,
        }
    }
}

impl fmt::Display for HealthCheckError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ProtocolViolation { message, .. } => {
                write!(f, "health check protocol violation: {message}")
            }
            Self::Unavailable { message, .. } => {
                write!(f, "health check unavailable: {message}")
            }
            Self::RpcError(err) => write!(f, "health check rpc error: {err}"),
            Self::Supervisor(err) => write!(f, "health check supervisor error: {err}"),
            Self::ProcessExited {
                message,
                exit_status,
                ..
            } => {
                write!(
                    f,
                    "health check process exited: {message} (status: {exit_status:?})"
                )
            }
            Self::TimedOut => write!(f, "health check timed out"),
        }
    }
}

impl std::error::Error for HealthCheckError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::RpcError(err) => Some(err),
            Self::Supervisor(err) => Some(err),
            _ => None,
        }
    }
}

/// Run context carrying immutable routing and fencing identifiers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunContext {
    pub team_id: String,
    pub task_id: String,
    pub run_id: String,
    pub role_id: String,
    pub binding_id: String,
    pub binding_version: u64,
    pub fencing_token: u64,
}

impl RunContext {
    #[must_use]
    pub fn new(
        team_id: impl Into<String>,
        task_id: impl Into<String>,
        run_id: impl Into<String>,
        role_id: impl Into<String>,
        binding_id: impl Into<String>,
        binding_version: u64,
        fencing_token: u64,
    ) -> Self {
        Self {
            team_id: team_id.into(),
            task_id: task_id.into(),
            run_id: run_id.into(),
            role_id: role_id.into(),
            binding_id: binding_id.into(),
            binding_version,
            fencing_token,
        }
    }
}

/// Run event context with strict sequence tracking.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunEventContext {
    pub run_context: RunContext,
    pub sequence: u64,
}

/// Program and arguments for a verification check.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VerificationCommand {
    pub program: String,
    pub args: Vec<String>,
}

impl VerificationCommand {
    #[must_use]
    pub fn new(program: impl Into<String>, args: Vec<String>) -> Self {
        Self {
            program: program.into(),
            args,
        }
    }
}

/// Parameters for initiating a `run.start` request.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunStartParams {
    pub context: RunContext,
    pub idempotency_key: String,
    pub task_title: String,
    pub role_name: String,
    pub role_instructions: String,
    pub references: Vec<ArtifactReference>,
    pub shared_workspace_root: String,
    pub execution_workspace_root: String,
    pub read_scopes: Vec<String>,
    pub write_scopes: Vec<String>,
    pub verification_commands: Vec<VerificationCommand>,
    pub credential_references: Vec<String>,
    pub heartbeat_interval_ms: u64,
    pub timeout_ms: u64,
    pub max_output_bytes: Option<u64>,
    pub correlation_id: String,
}

impl RunStartParams {
    #[must_use]
    pub fn new(
        context: RunContext,
        idempotency_key: impl Into<String>,
        task_title: impl Into<String>,
        correlation_id: impl Into<String>,
    ) -> Self {
        Self {
            context,
            idempotency_key: idempotency_key.into(),
            task_title: task_title.into(),
            role_name: String::new(),
            role_instructions: String::new(),
            references: Vec::new(),
            shared_workspace_root: String::new(),
            execution_workspace_root: String::new(),
            read_scopes: Vec::new(),
            write_scopes: Vec::new(),
            verification_commands: Vec::new(),
            credential_references: Vec::new(),
            heartbeat_interval_ms: 10_000,
            timeout_ms: 60_000,
            max_output_bytes: None,
            correlation_id: correlation_id.into(),
        }
    }

    #[must_use]
    pub fn with_role(mut self, name: impl Into<String>, instructions: impl Into<String>) -> Self {
        self.role_name = name.into();
        self.role_instructions = instructions.into();
        self
    }

    #[must_use]
    pub fn with_workspace_roots(
        mut self,
        shared: impl Into<String>,
        execution: impl Into<String>,
    ) -> Self {
        self.shared_workspace_root = shared.into();
        self.execution_workspace_root = execution.into();
        self
    }

    #[must_use]
    pub fn with_scopes(mut self, read: Vec<String>, write: Vec<String>) -> Self {
        self.read_scopes = read;
        self.write_scopes = write;
        self
    }

    #[must_use]
    pub fn with_timing(mut self, heartbeat_interval_ms: u64, timeout_ms: u64) -> Self {
        self.heartbeat_interval_ms = heartbeat_interval_ms;
        self.timeout_ms = timeout_ms;
        self
    }

    #[must_use]
    pub fn with_references(mut self, references: Vec<ArtifactReference>) -> Self {
        self.references = references;
        self
    }

    #[must_use]
    pub fn with_verification_commands(mut self, commands: Vec<VerificationCommand>) -> Self {
        self.verification_commands = commands;
        self
    }

    #[must_use]
    pub fn with_credential_references(mut self, refs: Vec<String>) -> Self {
        self.credential_references = refs;
        self
    }

    #[must_use]
    pub fn with_max_output_bytes(mut self, max_bytes: u64) -> Self {
        self.max_output_bytes = Some(max_bytes);
        self
    }
}

/// Acknowledged response for `run.start`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunStartResult {
    pub accepted: bool,
    pub session_reference: Option<String>,
    pub observed_at: String,
}

/// Parameters for sending a `run.cancel` request.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunCancelParams {
    pub context: RunContext,
    pub reason: String,
    pub requested_at: String,
    pub force_after_ms: Option<u64>,
    pub correlation_id: String,
}

impl RunCancelParams {
    #[must_use]
    pub fn new(
        context: RunContext,
        reason: impl Into<String>,
        requested_at: impl Into<String>,
        correlation_id: impl Into<String>,
    ) -> Self {
        Self {
            context,
            reason: reason.into(),
            requested_at: requested_at.into(),
            force_after_ms: None,
            correlation_id: correlation_id.into(),
        }
    }

    #[must_use]
    pub fn with_force_after_ms(mut self, force_after_ms: u64) -> Self {
        self.force_after_ms = Some(force_after_ms);
        self
    }
}

/// Acknowledged response for `run.cancel`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunCancelResult {
    pub accepted: bool,
    pub confirmed: bool,
    pub observed_at: String,
}

/// Mutually exclusive output kinds supported by Adapter V0.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutputKind {
    Stdout,
    Stderr,
    Progress,
    ArtifactCandidate,
}

impl OutputKind {
    #[must_use]
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "stdout" => Some(Self::Stdout),
            "stderr" => Some(Self::Stderr),
            "progress" => Some(Self::Progress),
            "artifact_candidate" => Some(Self::ArtifactCandidate),
            _ => None,
        }
    }

    #[must_use]
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Stdout => "stdout",
            Self::Stderr => "stderr",
            Self::Progress => "progress",
            Self::ArtifactCandidate => "artifact_candidate",
        }
    }
}

/// Heartbeat notification received from the Adapter.
#[derive(Debug, Clone, PartialEq)]
pub struct RunHeartbeat {
    pub context: RunEventContext,
    pub observed_at: String,
    pub phase: String,
    pub progress_percent: Option<f64>,
}

/// Bounded output notification received from the Adapter.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunOutput {
    pub context: RunEventContext,
    pub observed_at: String,
    pub kind: OutputKind,
    pub content: Option<String>,
    pub artifact: Option<ArtifactReference>,
    pub truncated: bool,
}

/// Completion notification indicating successful termination.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunCompleted {
    pub context: RunEventContext,
    pub completed_at: String,
    pub result_references: Vec<ArtifactReference>,
    pub summary: Option<String>,
}

/// Failure notification indicating an unrecoverable run error.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunFailed {
    pub context: RunEventContext,
    pub failed_at: String,
    pub error_class: String,
    pub code: String,
    pub retryable: bool,
    pub reason_key: Option<String>,
    pub diagnostic_reference: Option<ArtifactReference>,
}

/// Strongly typed notification emitted during Run execution.
#[derive(Debug, Clone, PartialEq)]
pub enum RunNotification {
    Heartbeat(RunHeartbeat),
    Output(RunOutput),
    Completed(RunCompleted),
    Failed(RunFailed),
}

impl RunNotification {
    #[must_use]
    pub fn sequence(&self) -> u64 {
        match self {
            Self::Heartbeat(h) => h.context.sequence,
            Self::Output(o) => o.context.sequence,
            Self::Completed(c) => c.context.sequence,
            Self::Failed(f) => f.context.sequence,
        }
    }

    #[must_use]
    pub fn context(&self) -> &RunEventContext {
        match self {
            Self::Heartbeat(h) => &h.context,
            Self::Output(o) => &o.context,
            Self::Completed(c) => &c.context,
            Self::Failed(f) => &f.context,
        }
    }
}

/// Complete record of an executed run from start to completion.
#[derive(Debug, Clone, PartialEq)]
pub struct CompletedRunResult {
    pub start_result: RunStartResult,
    pub heartbeats: Vec<RunHeartbeat>,
    pub outputs: Vec<RunOutput>,
    pub completion: RunCompleted,
}

/// Errors occurring during run execution or notification stream validation.
#[derive(Debug)]
pub enum RunExecutionError {
    /// The negotiated Adapter capabilities omit the requested Run operation.
    CapabilityMissing {
        code: i32,
        domain_code: &'static str,
        required_capability: &'static str,
    },
    /// The negotiated Adapter authentication state does not permit Run operations.
    AuthenticationFailed {
        code: i32,
        domain_code: &'static str,
        auth_status: AuthStatus,
    },
    /// Out of order sequence (-32015 / RUN_OUTPUT_OUT_OF_ORDER).
    OutOfOrder {
        code: i32,
        domain_code: &'static str,
        message: String,
        expected_sequence: u64,
        received_sequence: u64,
    },
    /// Protocol violation (-32099 / ADAPTER_PROTOCOL_VIOLATION).
    ProtocolViolation {
        code: i32,
        domain_code: &'static str,
        message: String,
    },
    /// Fencing token mismatch (-32014 / RUN_FENCED).
    Fenced {
        code: i32,
        domain_code: &'static str,
        message: String,
        expected_token: u64,
        received_token: u64,
    },
    /// Connection or pipe unavailable (-32017 / ADAPTER_UNAVAILABLE).
    Unavailable {
        code: i32,
        domain_code: &'static str,
        message: String,
    },
    /// Run failed notification reported by Adapter.
    RunFailed(Box<RunFailed>),
    /// JSON-RPC error returned by Adapter.
    RpcError(AdapterRpcError),
    /// Supervisor process / IO error.
    Supervisor(SupervisorError),
    /// Child process terminated unexpectedly before Run completed.
    ProcessExited {
        code: i32,
        domain_code: &'static str,
        message: String,
        exit_status: Option<ExitStatus>,
        stderr: CapturedStderr,
    },
    /// Run start or stream wait timed out.
    TimedOut,
}

impl RunExecutionError {
    #[must_use]
    pub fn jsonrpc_code(&self) -> i32 {
        match self {
            Self::CapabilityMissing { code, .. }
            | Self::AuthenticationFailed { code, .. }
            | Self::OutOfOrder { code, .. }
            | Self::ProtocolViolation { code, .. }
            | Self::Fenced { code, .. }
            | Self::Unavailable { code, .. }
            | Self::ProcessExited { code, .. } => *code,
            Self::TimedOut => -32017,
            Self::RpcError(err) => err.code,
            Self::Supervisor(_) => -32017,
            Self::RunFailed(_) => -32099,
        }
    }

    #[must_use]
    pub fn domain_code(&self) -> &str {
        match self {
            Self::CapabilityMissing { domain_code, .. }
            | Self::AuthenticationFailed { domain_code, .. }
            | Self::OutOfOrder { domain_code, .. }
            | Self::ProtocolViolation { domain_code, .. }
            | Self::Fenced { domain_code, .. }
            | Self::Unavailable { domain_code, .. }
            | Self::ProcessExited { domain_code, .. } => domain_code,
            Self::TimedOut => "ADAPTER_UNAVAILABLE",
            Self::RpcError(err) => err
                .domain_code
                .as_deref()
                .unwrap_or("ADAPTER_PROTOCOL_VIOLATION"),
            Self::Supervisor(_) => "ADAPTER_UNAVAILABLE",
            Self::RunFailed(_) => "RUN_FAILED",
        }
    }

    #[must_use]
    pub fn is_retryable(&self) -> bool {
        match self {
            Self::CapabilityMissing { .. }
            | Self::AuthenticationFailed { .. }
            | Self::OutOfOrder { .. }
            | Self::ProtocolViolation { .. }
            | Self::Fenced { .. } => false,
            Self::Unavailable { .. } | Self::ProcessExited { .. } | Self::TimedOut => true,
            Self::RpcError(err) => err.retryable,
            Self::Supervisor(_) => true,
            Self::RunFailed(failed) => failed.retryable,
        }
    }
}

impl fmt::Display for RunExecutionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::CapabilityMissing {
                required_capability,
                ..
            } => write!(f, "run execution capability missing: {required_capability}"),
            Self::AuthenticationFailed { auth_status, .. } => write!(
                f,
                "run execution authentication failed: {}",
                auth_status.as_str()
            ),
            Self::OutOfOrder {
                message,
                expected_sequence,
                received_sequence,
                ..
            } => {
                write!(
                    f,
                    "run execution out of order: {message} (expected: {expected_sequence}, received: {received_sequence})"
                )
            }
            Self::ProtocolViolation { message, .. } => {
                write!(f, "run execution protocol violation: {message}")
            }
            Self::Fenced {
                message,
                expected_token,
                received_token,
                ..
            } => {
                write!(
                    f,
                    "run execution fenced: {message} (expected: {expected_token}, received: {received_token})"
                )
            }
            Self::Unavailable { message, .. } => {
                write!(f, "run execution unavailable: {message}")
            }
            Self::RunFailed(failed) => {
                write!(
                    f,
                    "run execution failed: code={}, class={}",
                    failed.code, failed.error_class
                )
            }
            Self::RpcError(err) => write!(f, "run execution rpc error: {err}"),
            Self::Supervisor(err) => write!(f, "run execution supervisor error: {err}"),
            Self::ProcessExited {
                message,
                exit_status,
                ..
            } => {
                write!(
                    f,
                    "run execution process exited: {message} (status: {exit_status:?})"
                )
            }
            Self::TimedOut => write!(f, "run execution timed out"),
        }
    }
}

impl std::error::Error for RunExecutionError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::RpcError(err) => Some(err),
            Self::Supervisor(err) => Some(err),
            _ => None,
        }
    }
}

/// Validates in-order delivery, fencing tokens, and payload constraints for Run notifications.
#[derive(Debug, Clone)]
pub struct RunNotificationValidator {
    expected_context: RunContext,
    last_sequence: u64,
    last_payload: Option<JsonValue>,
    last_was_duplicate: bool,
    duplicate_count: usize,
    terminal: bool,
}

impl RunNotificationValidator {
    #[must_use]
    pub fn new(expected_context: RunContext) -> Self {
        Self {
            expected_context,
            last_sequence: 0,
            last_payload: None,
            last_was_duplicate: false,
            duplicate_count: 0,
            terminal: false,
        }
    }

    #[must_use]
    pub fn expected_context(&self) -> &RunContext {
        &self.expected_context
    }

    #[must_use]
    pub fn last_sequence(&self) -> u64 {
        self.last_sequence
    }

    #[must_use]
    pub fn is_terminal(&self) -> bool {
        self.terminal
    }

    #[must_use]
    pub fn last_was_duplicate(&self) -> bool {
        self.last_was_duplicate
    }

    #[must_use]
    pub fn duplicate_count(&self) -> usize {
        self.duplicate_count
    }

    pub fn validate_line(&mut self, line: &str) -> Result<RunNotification, RunExecutionError> {
        if line.len() > MAX_PROTOCOL_JSON_UTF8_BYTES {
            return Err(protocol_violation(
                "Notification exceeded the 1 MiB UTF-8 protocol message limit",
            ));
        }
        let json = JsonValue::parse(line).map_err(|e| RunExecutionError::ProtocolViolation {
            code: -32099,
            domain_code: "ADAPTER_PROTOCOL_VIOLATION",
            message: format!("Malformed JSON: {e}"),
        })?;
        self.validate_json(&json)
    }

    pub fn validate_json(
        &mut self,
        json: &JsonValue,
    ) -> Result<RunNotification, RunExecutionError> {
        if self.terminal {
            return Err(RunExecutionError::ProtocolViolation {
                code: -32099,
                domain_code: "ADAPTER_PROTOCOL_VIOLATION",
                message: "Received notification after terminal completion or failure".to_string(),
            });
        }

        validate_closed_object(json, &["jsonrpc", "method", "params"], &[], "notification")?;

        // Verify jsonrpc == "2.0"
        match json.get("jsonrpc").and_then(JsonValue::as_str) {
            Some("2.0") => {}
            _ => {
                return Err(RunExecutionError::ProtocolViolation {
                    code: -32099,
                    domain_code: "ADAPTER_PROTOCOL_VIOLATION",
                    message: "Missing or invalid 'jsonrpc' field in notification".to_string(),
                });
            }
        }

        // Verify method
        let method = json
            .get("method")
            .and_then(JsonValue::as_str)
            .ok_or_else(|| RunExecutionError::ProtocolViolation {
                code: -32099,
                domain_code: "ADAPTER_PROTOCOL_VIOLATION",
                message: "Missing 'method' in notification".to_string(),
            })?;

        // Extract params
        let params = json
            .get("params")
            .ok_or_else(|| RunExecutionError::ProtocolViolation {
                code: -32099,
                domain_code: "ADAPTER_PROTOCOL_VIOLATION",
                message: "Missing 'params' in notification".to_string(),
            })?;

        validate_run_event_context(params)?;

        // Verify runContext fields: team_id, task_id, run_id, role_id, binding_id, binding_version
        let team_id = params.get("team_id").and_then(JsonValue::as_str);
        let task_id = params.get("task_id").and_then(JsonValue::as_str);
        let run_id = params.get("run_id").and_then(JsonValue::as_str);
        let role_id = params.get("role_id").and_then(JsonValue::as_str);
        let binding_id = params.get("binding_id").and_then(JsonValue::as_str);
        let binding_version = params.get("binding_version").and_then(json_integer);

        #[allow(clippy::cast_possible_wrap)]
        let expected_binding_version = self.expected_context.binding_version as i64;
        if team_id != Some(&self.expected_context.team_id)
            || task_id != Some(&self.expected_context.task_id)
            || run_id != Some(&self.expected_context.run_id)
            || role_id != Some(&self.expected_context.role_id)
            || binding_id != Some(&self.expected_context.binding_id)
            || binding_version != Some(expected_binding_version)
        {
            return Err(RunExecutionError::ProtocolViolation {
                code: -32099,
                domain_code: "ADAPTER_PROTOCOL_VIOLATION",
                message: "Run context mismatch or missing in notification".to_string(),
            });
        }

        // Verify fencing_token
        let fencing_token_val = params
            .get("fencing_token")
            .and_then(json_integer)
            .ok_or_else(|| RunExecutionError::ProtocolViolation {
                code: -32099,
                domain_code: "ADAPTER_PROTOCOL_VIOLATION",
                message: "Missing 'fencing_token' in notification".to_string(),
            })?;

        let fencing_token = u64::try_from(fencing_token_val).map_err(|_| {
            protocol_violation("'fencing_token' in notification must be a positive integer")
        })?;
        if fencing_token != self.expected_context.fencing_token {
            return Err(RunExecutionError::Fenced {
                code: -32014,
                domain_code: "RUN_FENCED",
                message: format!(
                    "Fencing token mismatch: expected {}, received {}",
                    self.expected_context.fencing_token, fencing_token
                ),
                expected_token: self.expected_context.fencing_token,
                received_token: fencing_token,
            });
        }

        // Verify sequence
        let sequence_val = params
            .get("sequence")
            .and_then(json_integer)
            .ok_or_else(|| RunExecutionError::ProtocolViolation {
                code: -32099,
                domain_code: "ADAPTER_PROTOCOL_VIOLATION",
                message: "Missing 'sequence' in notification".to_string(),
            })?;

        if sequence_val < 1 {
            return Err(RunExecutionError::ProtocolViolation {
                code: -32099,
                domain_code: "ADAPTER_PROTOCOL_VIOLATION",
                message: "Sequence number must be at least 1".to_string(),
            });
        }

        let sequence = u64::try_from(sequence_val).map_err(|_| {
            protocol_violation("'sequence' in notification must be a positive integer")
        })?;

        let is_duplicate = if sequence == self.last_sequence && sequence > 0 {
            if self.last_payload.as_ref() == Some(json) {
                true
            } else {
                return Err(RunExecutionError::ProtocolViolation {
                    code: -32099,
                    domain_code: "ADAPTER_PROTOCOL_VIOLATION",
                    message: format!(
                        "Sequence {sequence} duplicate notification has conflicting payload"
                    ),
                });
            }
        } else {
            let expected_seq = self.last_sequence + 1;
            if sequence != expected_seq {
                return Err(RunExecutionError::OutOfOrder {
                    code: -32015,
                    domain_code: "RUN_OUTPUT_OUT_OF_ORDER",
                    message: format!(
                        "Sequence out of order: expected {expected_seq}, received {sequence}"
                    ),
                    expected_sequence: expected_seq,
                    received_sequence: sequence,
                });
            }
            false
        };

        let event_context = RunEventContext {
            run_context: self.expected_context.clone(),
            sequence,
        };

        let notification = match method {
            "run.heartbeat" => {
                validate_closed_object(
                    params,
                    RUN_EVENT_CONTEXT_FIELDS,
                    &["observed_at", "phase", "progress_percent"],
                    "run.heartbeat params",
                )?;
                let observed_at = params
                    .get("observed_at")
                    .and_then(JsonValue::as_str)
                    .ok_or_else(|| RunExecutionError::ProtocolViolation {
                        code: -32099,
                        domain_code: "ADAPTER_PROTOCOL_VIOLATION",
                        message: "Missing 'observed_at' in run.heartbeat".to_string(),
                    })?;
                if !is_valid_utc_rfc3339_milliseconds(observed_at) {
                    return Err(RunExecutionError::ProtocolViolation {
                        code: -32099,
                        domain_code: "ADAPTER_PROTOCOL_VIOLATION",
                        message: "Invalid 'observed_at' in run.heartbeat; expected UTC RFC 3339 milliseconds".to_string(),
                    });
                }
                let observed_at = observed_at.to_string();

                let phase = params
                    .get("phase")
                    .and_then(JsonValue::as_str)
                    .ok_or_else(|| RunExecutionError::ProtocolViolation {
                        code: -32099,
                        domain_code: "ADAPTER_PROTOCOL_VIOLATION",
                        message: "Missing 'phase' in run.heartbeat".to_string(),
                    })?
                    .to_string();
                if !(1..=120).contains(&phase.chars().count()) {
                    return Err(protocol_violation(
                        "Invalid 'phase' in run.heartbeat; expected 1 to 120 characters",
                    ));
                }

                let progress_percent = match params.get("progress_percent") {
                    Some(JsonValue::Number(n)) => Some(*n),
                    #[allow(clippy::cast_precision_loss)]
                    Some(JsonValue::Integer(i)) => Some(*i as f64),
                    Some(JsonValue::Null) | None => None,
                    _ => {
                        return Err(RunExecutionError::ProtocolViolation {
                            code: -32099,
                            domain_code: "ADAPTER_PROTOCOL_VIOLATION",
                            message: "Invalid 'progress_percent' in run.heartbeat".to_string(),
                        });
                    }
                };
                if progress_percent.is_some_and(|progress| {
                    !progress.is_finite() || !(0.0..=100.0).contains(&progress)
                }) {
                    return Err(RunExecutionError::ProtocolViolation {
                        code: -32099,
                        domain_code: "ADAPTER_PROTOCOL_VIOLATION",
                        message: "Invalid 'progress_percent' in run.heartbeat".to_string(),
                    });
                }

                Ok(RunNotification::Heartbeat(RunHeartbeat {
                    context: event_context,
                    observed_at,
                    phase,
                    progress_percent,
                }))
            }
            "run.output" => {
                validate_closed_object(
                    params,
                    RUN_EVENT_CONTEXT_FIELDS,
                    &["observed_at", "kind", "content", "artifact", "truncated"],
                    "run.output params",
                )?;
                let observed_at = params
                    .get("observed_at")
                    .and_then(JsonValue::as_str)
                    .ok_or_else(|| RunExecutionError::ProtocolViolation {
                        code: -32099,
                        domain_code: "ADAPTER_PROTOCOL_VIOLATION",
                        message: "Missing 'observed_at' in run.output".to_string(),
                    })?;
                if !is_valid_utc_rfc3339_milliseconds(observed_at) {
                    return Err(RunExecutionError::ProtocolViolation {
                        code: -32099,
                        domain_code: "ADAPTER_PROTOCOL_VIOLATION",
                        message: "Invalid 'observed_at' in run.output; expected UTC RFC 3339 milliseconds".to_string(),
                    });
                }
                let observed_at = observed_at.to_string();

                let kind_str = params
                    .get("kind")
                    .and_then(JsonValue::as_str)
                    .ok_or_else(|| RunExecutionError::ProtocolViolation {
                        code: -32099,
                        domain_code: "ADAPTER_PROTOCOL_VIOLATION",
                        message: "Missing 'kind' in run.output".to_string(),
                    })?;

                let kind = OutputKind::parse(kind_str).ok_or_else(|| {
                    RunExecutionError::ProtocolViolation {
                        code: -32099,
                        domain_code: "ADAPTER_PROTOCOL_VIOLATION",
                        message: format!("Unknown output kind '{kind_str}' in run.output"),
                    }
                })?;

                let truncated = match params.get("truncated") {
                    Some(value) => value.as_bool().ok_or_else(|| {
                        protocol_violation("Invalid 'truncated' in run.output; expected boolean")
                    })?,
                    None => false,
                };

                match kind {
                    OutputKind::Stdout | OutputKind::Stderr | OutputKind::Progress => {
                        let content = match params.get("content") {
                            Some(JsonValue::String(s)) if !s.is_empty() => s.clone(),
                            _ => {
                                return Err(RunExecutionError::ProtocolViolation {
                                    code: -32099,
                                    domain_code: "ADAPTER_PROTOCOL_VIOLATION",
                                    message: format!(
                                        "run.output kind '{}' must contain non-empty content string",
                                        kind.as_str()
                                    ),
                                });
                            }
                        };

                        if content.chars().count() > MAX_RUN_OUTPUT_CONTENT_CHARS
                            || content.len() > MAX_RUN_OUTPUT_CONTENT_UTF8_BYTES
                        {
                            return Err(protocol_violation(
                                "run.output content exceeds the 64 KiB UTF-8 limit",
                            ));
                        }

                        if params.get("artifact").is_some() {
                            return Err(RunExecutionError::ProtocolViolation {
                                code: -32099,
                                domain_code: "ADAPTER_PROTOCOL_VIOLATION",
                                message: format!(
                                    "run.output kind '{}' must not contain artifact",
                                    kind.as_str()
                                ),
                            });
                        }

                        Ok(RunNotification::Output(RunOutput {
                            context: event_context,
                            observed_at,
                            kind,
                            content: Some(content),
                            artifact: None,
                            truncated,
                        }))
                    }
                    OutputKind::ArtifactCandidate => {
                        if params.get("content").is_some() {
                            return Err(RunExecutionError::ProtocolViolation {
                                code: -32099,
                                domain_code: "ADAPTER_PROTOCOL_VIOLATION",
                                message:
                                    "run.output kind 'artifact_candidate' must not contain content"
                                        .to_string(),
                            });
                        }

                        let art_val = params.get("artifact").ok_or_else(|| {
                            RunExecutionError::ProtocolViolation {
                                code: -32099,
                                domain_code: "ADAPTER_PROTOCOL_VIOLATION",
                                message: "run.output kind 'artifact_candidate' missing artifact"
                                    .to_string(),
                            }
                        })?;

                        let artifact_ref = parse_reference(art_val).ok_or_else(|| {
                            RunExecutionError::ProtocolViolation {
                                code: -32099,
                                domain_code: "ADAPTER_PROTOCOL_VIOLATION",
                                message: "run.output kind 'artifact_candidate' has invalid artifact reference".to_string(),
                            }
                        })?;

                        if artifact_ref.kind != "artifact" || artifact_ref.uri.is_empty() {
                            return Err(RunExecutionError::ProtocolViolation {
                                code: -32099,
                                domain_code: "ADAPTER_PROTOCOL_VIOLATION",
                                message: "artifact_candidate reference kind must be 'artifact' and uri must not be empty".to_string(),
                            });
                        }

                        Ok(RunNotification::Output(RunOutput {
                            context: event_context,
                            observed_at,
                            kind,
                            content: None,
                            artifact: Some(artifact_ref),
                            truncated,
                        }))
                    }
                }
            }
            "run.completed" => {
                validate_closed_object(
                    params,
                    RUN_EVENT_CONTEXT_FIELDS,
                    &["completed_at", "result_references", "summary"],
                    "run.completed params",
                )?;
                let completed_at = params
                    .get("completed_at")
                    .and_then(JsonValue::as_str)
                    .ok_or_else(|| RunExecutionError::ProtocolViolation {
                        code: -32099,
                        domain_code: "ADAPTER_PROTOCOL_VIOLATION",
                        message: "Missing 'completed_at' in run.completed".to_string(),
                    })?;
                if !is_valid_utc_rfc3339_milliseconds(completed_at) {
                    return Err(RunExecutionError::ProtocolViolation {
                        code: -32099,
                        domain_code: "ADAPTER_PROTOCOL_VIOLATION",
                        message: "Invalid 'completed_at' in run.completed; expected UTC RFC 3339 milliseconds".to_string(),
                    });
                }
                let completed_at = completed_at.to_string();

                let refs_val = params
                    .get("result_references")
                    .and_then(JsonValue::as_array)
                    .ok_or_else(|| RunExecutionError::ProtocolViolation {
                        code: -32099,
                        domain_code: "ADAPTER_PROTOCOL_VIOLATION",
                        message: "Missing 'result_references' in run.completed".to_string(),
                    })?;
                if refs_val.len() > MAX_RESULT_REFERENCES {
                    return Err(protocol_violation(
                        "run.completed result_references exceeds 128 items",
                    ));
                }

                let mut result_references = Vec::with_capacity(refs_val.len());
                for r in refs_val {
                    let parsed =
                        parse_reference(r).ok_or_else(|| RunExecutionError::ProtocolViolation {
                            code: -32099,
                            domain_code: "ADAPTER_PROTOCOL_VIOLATION",
                            message: "Invalid reference in result_references".to_string(),
                        })?;
                    result_references.push(parsed);
                }

                let summary = match params.get("summary") {
                    Some(JsonValue::String(value))
                        if value.chars().count() <= MAX_SUMMARY_CHARS =>
                    {
                        Some(value.clone())
                    }
                    Some(JsonValue::Null) | None => None,
                    _ => {
                        return Err(protocol_violation(
                            "Invalid 'summary' in run.completed; expected null or at most 8000 characters",
                        ));
                    }
                };

                Ok(RunNotification::Completed(RunCompleted {
                    context: event_context,
                    completed_at,
                    result_references,
                    summary,
                }))
            }
            "run.failed" => {
                validate_closed_object(
                    params,
                    RUN_EVENT_CONTEXT_FIELDS,
                    &[
                        "failed_at",
                        "error_class",
                        "code",
                        "retryable",
                        "reason_key",
                        "diagnostic_reference",
                    ],
                    "run.failed params",
                )?;
                let failed_at = params
                    .get("failed_at")
                    .and_then(JsonValue::as_str)
                    .ok_or_else(|| RunExecutionError::ProtocolViolation {
                        code: -32099,
                        domain_code: "ADAPTER_PROTOCOL_VIOLATION",
                        message: "Missing 'failed_at' in run.failed".to_string(),
                    })?;
                if !is_valid_utc_rfc3339_milliseconds(failed_at) {
                    return Err(RunExecutionError::ProtocolViolation {
                        code: -32099,
                        domain_code: "ADAPTER_PROTOCOL_VIOLATION",
                        message:
                            "Invalid 'failed_at' in run.failed; expected UTC RFC 3339 milliseconds"
                                .to_string(),
                    });
                }
                let failed_at = failed_at.to_string();

                let error_class = params
                    .get("error_class")
                    .and_then(JsonValue::as_str)
                    .ok_or_else(|| RunExecutionError::ProtocolViolation {
                        code: -32099,
                        domain_code: "ADAPTER_PROTOCOL_VIOLATION",
                        message: "Missing 'error_class' in run.failed".to_string(),
                    })?
                    .to_string();
                if !RUN_FAILURE_CLASSES.contains(&error_class.as_str()) {
                    return Err(protocol_violation("Invalid 'error_class' in run.failed"));
                }

                let code = params
                    .get("code")
                    .and_then(JsonValue::as_str)
                    .ok_or_else(|| RunExecutionError::ProtocolViolation {
                        code: -32099,
                        domain_code: "ADAPTER_PROTOCOL_VIOLATION",
                        message: "Missing 'code' in run.failed".to_string(),
                    })?
                    .to_string();
                if !is_failure_code(&code) {
                    return Err(protocol_violation("Invalid 'code' in run.failed"));
                }

                let retryable = params
                    .get("retryable")
                    .and_then(JsonValue::as_bool)
                    .ok_or_else(|| {
                        protocol_violation("Missing or invalid 'retryable' in run.failed")
                    })?;

                let reason_key = match params.get("reason_key") {
                    Some(JsonValue::String(value))
                        if value.chars().count() <= MAX_REASON_KEY_CHARS =>
                    {
                        Some(value.clone())
                    }
                    Some(JsonValue::Null) | None => None,
                    _ => return Err(protocol_violation("Invalid 'reason_key' in run.failed")),
                };

                let diagnostic_reference = match params.get("diagnostic_reference") {
                    Some(JsonValue::Null) | None => None,
                    Some(value) => Some(parse_reference(value).ok_or_else(|| {
                        protocol_violation("Invalid 'diagnostic_reference' in run.failed")
                    })?),
                };

                Ok(RunNotification::Failed(RunFailed {
                    context: event_context,
                    failed_at,
                    error_class,
                    code,
                    retryable,
                    reason_key,
                    diagnostic_reference,
                }))
            }
            other => Err(RunExecutionError::ProtocolViolation {
                code: -32099,
                domain_code: "ADAPTER_PROTOCOL_VIOLATION",
                message: format!("Unknown run notification method '{other}'"),
            }),
        };

        if notification.is_ok() {
            if is_duplicate {
                self.last_was_duplicate = true;
                self.duplicate_count += 1;
            } else {
                self.last_sequence = sequence;
                self.last_payload = Some(json.clone());
                self.last_was_duplicate = false;
                self.terminal = matches!(
                    &notification,
                    Ok(RunNotification::Completed(_)) | Ok(RunNotification::Failed(_))
                );
            }
        }

        notification
    }
}

impl SupervisedAdapter {
    fn require_negotiated_run_operation(
        &self,
        required_capability: &'static str,
    ) -> Result<(), RunExecutionError> {
        let negotiated = self.negotiated_initialize.as_ref().ok_or_else(|| {
            RunExecutionError::ProtocolViolation {
                code: -32099,
                domain_code: "ADAPTER_PROTOCOL_VIOLATION",
                message: "Run operation requires a successful initialize handshake".to_string(),
            }
        })?;

        if !matches!(
            negotiated.auth_status,
            AuthStatus::NotRequired | AuthStatus::Ready
        ) {
            return Err(RunExecutionError::AuthenticationFailed {
                code: -32012,
                domain_code: "ADAPTER_AUTHENTICATION_FAILED",
                auth_status: negotiated.auth_status,
            });
        }
        if !negotiated.has_capability(required_capability) {
            return Err(RunExecutionError::CapabilityMissing {
                code: -32011,
                domain_code: "ADAPTER_CAPABILITY_MISSING",
                required_capability,
            });
        }
        Ok(())
    }

    /// Returns the successful handshake retained for this Adapter process, if any.
    #[must_use]
    pub fn negotiated_initialize(&self) -> Option<&InitializeResult> {
        self.negotiated_initialize.as_ref()
    }

    async fn terminate_after_request_timeout(&mut self) -> Result<(), SupervisorError> {
        self.wait_for_exit(Duration::ZERO).await.map(|_| ())
    }

    /// Performs the `initialize` handshake with the supervised Adapter process.
    pub async fn initialize(
        &mut self,
        params: &InitializeParams,
        timeout: Duration,
    ) -> Result<InitializeResult, HandshakeError> {
        self.negotiated_initialize = None;
        validate_initialize_params(params)?;
        let request_id = format!("init-{}", params.correlation_id);
        let request_json = serialize_initialize_request(&request_id, params);

        let deadline = Instant::now() + timeout;
        let cancellation = CancellationToken::new();

        if let Err(error) = self
            .send_protocol_line(request_json.as_bytes(), deadline, &cancellation)
            .await
        {
            return match error {
                SupervisorError::WriteTimedOut => Err(self
                    .terminate_after_request_timeout()
                    .await
                    .map_or_else(HandshakeError::Supervisor, |_| HandshakeError::TimedOut)),
                other => Err(HandshakeError::Supervisor(other)),
            };
        }

        let response_line = match self.read_response_line(timeout).await {
            Err(HandshakeError::TimedOut) => {
                return Err(self
                    .terminate_after_request_timeout()
                    .await
                    .map_or_else(HandshakeError::Supervisor, |_| HandshakeError::TimedOut));
            }
            Err(HandshakeError::Unavailable { .. }) => {
                return Err(HandshakeError::Unavailable {
                    code: -32017,
                    domain_code: "ADAPTER_UNAVAILABLE",
                    message: "Adapter closed stdout without response".to_string(),
                });
            }
            Err(error) => return Err(error),
            Ok(line) => line,
        };

        let result = parse_initialize_response(&response_line, &request_id)?;
        self.negotiated_initialize = Some(result.clone());
        Ok(result)
    }

    /// Performs an active `health.check` on the supervised Adapter process.
    pub async fn health_check(
        &mut self,
        params: &HealthCheckParams,
        timeout: Duration,
    ) -> Result<HealthResult, HealthCheckError> {
        validate_health_check_params(params)?;
        let request_id = format!("health-{}", params.correlation_id);
        let request_json = serialize_health_check_request(&request_id, params);

        let deadline = Instant::now() + timeout;
        let cancellation = CancellationToken::new();

        self.send_protocol_line(request_json.as_bytes(), deadline, &cancellation)
            .await
            .map_err(|e| match e {
                SupervisorError::WriteTimedOut => HealthCheckError::TimedOut,
                other => HealthCheckError::Supervisor(other),
            })?;

        let response_line = self.read_health_response_line(timeout).await?;

        parse_health_check_response(&response_line, &request_id)
    }

    async fn read_response_line(&mut self, timeout: Duration) -> Result<String, HandshakeError> {
        let deadline = Instant::now() + timeout;
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            return Err(HandshakeError::TimedOut);
        }
        match self.next_stdout_event(remaining).await {
            // `PROTOCOL.md` §2: stdout carries one complete JSON-RPC 2.0 object per line and
            // nothing else. No line may be skipped here — blank, non-JSON and diagnostic lines
            // must all fail so the existing `ADAPTER_PROTOCOL_VIOLATION` path rejects them.
            Ok(StdoutEvent::Line(bytes)) => {
                String::from_utf8(bytes).map_err(|_| HandshakeError::ProtocolViolation {
                    code: -32099,
                    domain_code: "ADAPTER_PROTOCOL_VIOLATION",
                    message: "Response line is not valid UTF-8".to_string(),
                })
            }
            Ok(StdoutEvent::LineTooLarge) => Err(HandshakeError::ProtocolViolation {
                code: -32099,
                domain_code: "ADAPTER_PROTOCOL_VIOLATION",
                message: "Response line exceeded 1 MiB limit".to_string(),
            }),
            Ok(StdoutEvent::EndOfStream) => {
                let stderr = self.stderr_capture.lock().await.clone();
                let status = match self.child.as_mut() {
                    Some(child) => child.try_wait().ok().flatten(),
                    None => None,
                };
                Err(HandshakeError::ProcessExited {
                    code: -32017,
                    domain_code: "ADAPTER_UNAVAILABLE",
                    message: "Adapter process exited unexpectedly before response".to_string(),
                    exit_status: status,
                    stderr,
                })
            }
            Ok(StdoutEvent::TimedOut) => Err(HandshakeError::TimedOut),
            Ok(StdoutEvent::ReadFailed) => Err(HandshakeError::Unavailable {
                code: -32017,
                domain_code: "ADAPTER_UNAVAILABLE",
                message: "Pipe read failed".to_string(),
            }),
            Err(e) => Err(HandshakeError::Supervisor(e)),
        }
    }

    async fn read_health_response_line(
        &mut self,
        timeout: Duration,
    ) -> Result<String, HealthCheckError> {
        let deadline = Instant::now() + timeout;
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            return Err(HealthCheckError::TimedOut);
        }
        match self.next_stdout_event(remaining).await {
            // `PROTOCOL.md` §2: stdout carries one complete JSON-RPC 2.0 object per line and
            // nothing else. No line may be skipped here — blank, non-JSON and diagnostic lines
            // must all fail so the existing `ADAPTER_PROTOCOL_VIOLATION` path rejects them.
            Ok(StdoutEvent::Line(bytes)) => {
                String::from_utf8(bytes).map_err(|_| HealthCheckError::ProtocolViolation {
                    code: -32099,
                    domain_code: "ADAPTER_PROTOCOL_VIOLATION",
                    message: "Response line is not valid UTF-8".to_string(),
                })
            }
            Ok(StdoutEvent::LineTooLarge) => Err(HealthCheckError::ProtocolViolation {
                code: -32099,
                domain_code: "ADAPTER_PROTOCOL_VIOLATION",
                message: "Response line exceeded 1 MiB limit".to_string(),
            }),
            Ok(StdoutEvent::EndOfStream) => {
                let stderr = self.stderr_capture.lock().await.clone();
                let status = match self.child.as_mut() {
                    Some(child) => child.try_wait().ok().flatten(),
                    None => None,
                };
                Err(HealthCheckError::ProcessExited {
                    code: -32017,
                    domain_code: "ADAPTER_UNAVAILABLE",
                    message: "Adapter process exited unexpectedly before response".to_string(),
                    exit_status: status,
                    stderr,
                })
            }
            Ok(StdoutEvent::TimedOut) => Err(HealthCheckError::TimedOut),
            Ok(StdoutEvent::ReadFailed) => Err(HealthCheckError::Unavailable {
                code: -32017,
                domain_code: "ADAPTER_UNAVAILABLE",
                message: "Pipe read failed".to_string(),
            }),
            Err(e) => Err(HealthCheckError::Supervisor(e)),
        }
    }

    /// Performs `run.start` on the supervised Adapter process and waits for initial acknowledgment.
    pub async fn run_start(
        &mut self,
        params: &RunStartParams,
        timeout: Duration,
    ) -> Result<RunStartResult, RunExecutionError> {
        self.require_negotiated_run_operation("run.start")?;
        validate_run_start_params(params)?;
        let request_id = format!("run-{}", params.context.run_id);
        let request_json = serialize_run_start_request(&request_id, params);

        let deadline = Instant::now() + timeout;
        let cancellation = CancellationToken::new();

        self.send_protocol_line(request_json.as_bytes(), deadline, &cancellation)
            .await
            .map_err(|e| match e {
                SupervisorError::WriteTimedOut => RunExecutionError::TimedOut,
                other => RunExecutionError::Supervisor(other),
            })?;

        let response_line = self.read_run_start_response_line(timeout).await?;
        parse_run_start_response(&response_line, &request_id)
    }

    async fn read_run_start_response_line(
        &mut self,
        timeout: Duration,
    ) -> Result<String, RunExecutionError> {
        let deadline = Instant::now() + timeout;
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            return Err(RunExecutionError::TimedOut);
        }
        match self.next_stdout_event(remaining).await {
            // `PROTOCOL.md` §2: stdout carries one complete JSON-RPC 2.0 object per line and
            // nothing else. No line may be skipped here — blank, non-JSON and diagnostic lines
            // must all fail so the existing `ADAPTER_PROTOCOL_VIOLATION` path rejects them.
            Ok(StdoutEvent::Line(bytes)) => {
                String::from_utf8(bytes).map_err(|_| RunExecutionError::ProtocolViolation {
                    code: -32099,
                    domain_code: "ADAPTER_PROTOCOL_VIOLATION",
                    message: "Response line is not valid UTF-8".to_string(),
                })
            }
            Ok(StdoutEvent::LineTooLarge) => Err(RunExecutionError::ProtocolViolation {
                code: -32099,
                domain_code: "ADAPTER_PROTOCOL_VIOLATION",
                message: "Response line exceeded 1 MiB limit".to_string(),
            }),
            Ok(StdoutEvent::EndOfStream) => {
                let stderr = self.stderr_capture.lock().await.clone();
                let status = match self.child.as_mut() {
                    Some(child) => child.try_wait().ok().flatten(),
                    None => None,
                };
                Err(RunExecutionError::ProcessExited {
                    code: -32017,
                    domain_code: "ADAPTER_UNAVAILABLE",
                    message: "Adapter process exited unexpectedly before run.start response"
                        .to_string(),
                    exit_status: status,
                    stderr,
                })
            }
            Ok(StdoutEvent::TimedOut) => Err(RunExecutionError::TimedOut),
            Ok(StdoutEvent::ReadFailed) => Err(RunExecutionError::Unavailable {
                code: -32017,
                domain_code: "ADAPTER_UNAVAILABLE",
                message: "Pipe read failed".to_string(),
            }),
            Err(e) => Err(RunExecutionError::Supervisor(e)),
        }
    }

    /// Performs `run.cancel` on the supervised Adapter process and waits for acknowledgment.
    pub async fn run_cancel(
        &mut self,
        params: &RunCancelParams,
        timeout: Duration,
    ) -> Result<RunCancelResult, RunExecutionError> {
        self.require_negotiated_run_operation("run.cancel")?;
        validate_run_cancel_params(params)?;

        let request_id = format!("cancel-{}", params.correlation_id);
        let request_json = serialize_run_cancel_request(&request_id, params);

        let deadline = Instant::now() + timeout;
        let cancellation = CancellationToken::new();

        self.send_protocol_line(request_json.as_bytes(), deadline, &cancellation)
            .await
            .map_err(|e| match e {
                SupervisorError::WriteTimedOut => RunExecutionError::TimedOut,
                other => RunExecutionError::Supervisor(other),
            })?;

        let response_line = self.read_run_cancel_response_line(timeout).await?;
        parse_run_cancel_response(&response_line, &request_id)
    }

    async fn read_run_cancel_response_line(
        &mut self,
        timeout: Duration,
    ) -> Result<String, RunExecutionError> {
        let deadline = Instant::now() + timeout;
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            return Err(RunExecutionError::TimedOut);
        }
        match self.next_stdout_event(remaining).await {
            // `PROTOCOL.md` §2: stdout carries one complete JSON-RPC 2.0 object per line and
            // nothing else. No line may be skipped here — blank, non-JSON and diagnostic lines
            // must all fail so the existing `ADAPTER_PROTOCOL_VIOLATION` path rejects them.
            Ok(StdoutEvent::Line(bytes)) => {
                String::from_utf8(bytes).map_err(|_| RunExecutionError::ProtocolViolation {
                    code: -32099,
                    domain_code: "ADAPTER_PROTOCOL_VIOLATION",
                    message: "Response line is not valid UTF-8".to_string(),
                })
            }
            Ok(StdoutEvent::LineTooLarge) => Err(RunExecutionError::ProtocolViolation {
                code: -32099,
                domain_code: "ADAPTER_PROTOCOL_VIOLATION",
                message: "Response line exceeded 1 MiB limit".to_string(),
            }),
            Ok(StdoutEvent::EndOfStream) => {
                let stderr = self.stderr_capture.lock().await.clone();
                let status = match self.child.as_mut() {
                    Some(child) => child.try_wait().ok().flatten(),
                    None => None,
                };
                Err(RunExecutionError::ProcessExited {
                    code: -32017,
                    domain_code: "ADAPTER_UNAVAILABLE",
                    message: "Adapter process exited unexpectedly before run.cancel response"
                        .to_string(),
                    exit_status: status,
                    stderr,
                })
            }
            Ok(StdoutEvent::TimedOut) => Err(RunExecutionError::TimedOut),
            Ok(StdoutEvent::ReadFailed) => Err(RunExecutionError::Unavailable {
                code: -32017,
                domain_code: "ADAPTER_UNAVAILABLE",
                message: "Pipe read failed".to_string(),
            }),
            Err(e) => Err(RunExecutionError::Supervisor(e)),
        }
    }

    /// Reads and validates the next run notification from the supervised Adapter.
    pub async fn read_run_notification(
        &mut self,
        validator: &mut RunNotificationValidator,
        timeout: Duration,
    ) -> Result<RunNotification, RunExecutionError> {
        let deadline = Instant::now() + timeout;
        loop {
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                return Err(RunExecutionError::TimedOut);
            }
            match self.next_stdout_event(remaining).await {
                // `PROTOCOL.md` §2: stdout carries one complete JSON-RPC 2.0 object per line and
                // nothing else. No line may be skipped here — blank, non-JSON and diagnostic lines
                // must all fail so the existing `ADAPTER_PROTOCOL_VIOLATION` path rejects them.
                Ok(StdoutEvent::Line(bytes)) => {
                    let line = String::from_utf8(bytes).map_err(|_| {
                        RunExecutionError::ProtocolViolation {
                            code: -32099,
                            domain_code: "ADAPTER_PROTOCOL_VIOLATION",
                            message: "Notification line is not valid UTF-8".to_string(),
                        }
                    })?;
                    let notification = validator.validate_line(&line)?;
                    if validator.last_was_duplicate() {
                        continue;
                    }
                    return Ok(notification);
                }
                Ok(StdoutEvent::LineTooLarge) => {
                    return Err(RunExecutionError::ProtocolViolation {
                        code: -32099,
                        domain_code: "ADAPTER_PROTOCOL_VIOLATION",
                        message: "Notification line exceeded 1 MiB limit".to_string(),
                    });
                }
                Ok(StdoutEvent::EndOfStream) => {
                    let stderr = self.stderr_capture.lock().await.clone();
                    let status = match self.child.as_mut() {
                        Some(child) => child.try_wait().ok().flatten(),
                        None => None,
                    };
                    return Err(RunExecutionError::ProcessExited {
                        code: -32017,
                        domain_code: "ADAPTER_UNAVAILABLE",
                        message: "Adapter process exited unexpectedly during run execution"
                            .to_string(),
                        exit_status: status,
                        stderr,
                    });
                }
                Ok(StdoutEvent::TimedOut) => return Err(RunExecutionError::TimedOut),
                Ok(StdoutEvent::ReadFailed) => {
                    return Err(RunExecutionError::Unavailable {
                        code: -32017,
                        domain_code: "ADAPTER_UNAVAILABLE",
                        message: "Pipe read failed".to_string(),
                    });
                }
                Err(e) => return Err(RunExecutionError::Supervisor(e)),
            }
        }
    }

    /// Collects notifications until `run.completed` (or failure/timeout).
    pub async fn collect_run_execution(
        &mut self,
        expected_context: &RunContext,
        timeout: Duration,
    ) -> Result<(Vec<RunHeartbeat>, Vec<RunOutput>, RunCompleted), RunExecutionError> {
        let mut validator = RunNotificationValidator::new(expected_context.clone());
        let mut heartbeats = Vec::new();
        let mut outputs = Vec::new();

        let deadline = Instant::now() + timeout;
        while !validator.is_terminal() {
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                return Err(RunExecutionError::TimedOut);
            }
            let notification = self
                .read_run_notification(&mut validator, remaining)
                .await?;
            match notification {
                RunNotification::Heartbeat(h) => heartbeats.push(h),
                RunNotification::Output(o) => outputs.push(o),
                RunNotification::Completed(c) => return Ok((heartbeats, outputs, c)),
                RunNotification::Failed(f) => {
                    return Err(RunExecutionError::RunFailed(Box::new(f)));
                }
            }
        }

        Err(RunExecutionError::ProtocolViolation {
            code: -32099,
            domain_code: "ADAPTER_PROTOCOL_VIOLATION",
            message: "Stream terminated without completed or failed notification".to_string(),
        })
    }

    /// Starts a run and collects notifications until completion.
    pub async fn execute_run(
        &mut self,
        params: &RunStartParams,
        timeout: Duration,
    ) -> Result<CompletedRunResult, RunExecutionError> {
        let start_result = self.run_start(params, timeout).await?;
        let (heartbeats, outputs, completion) =
            self.collect_run_execution(&params.context, timeout).await?;
        Ok(CompletedRunResult {
            start_result,
            heartbeats,
            outputs,
            completion,
        })
    }
}

fn serialize_initialize_request(request_id: &str, params: &InitializeParams) -> String {
    let mut param_fields = vec![
        (
            "protocol_version".to_string(),
            JsonValue::String(params.protocol_version.clone()),
        ),
        (
            "application_version".to_string(),
            JsonValue::String(params.application_version.clone()),
        ),
        (
            "harness_instance_id".to_string(),
            JsonValue::String(params.harness_instance_id.clone()),
        ),
        (
            "platform".to_string(),
            JsonValue::String(params.platform.clone()),
        ),
        (
            "locale".to_string(),
            JsonValue::String(params.locale.clone()),
        ),
        (
            "correlation_id".to_string(),
            JsonValue::String(params.correlation_id.clone()),
        ),
    ];

    if let Some(generation) = params.configuration_generation {
        #[allow(clippy::cast_possible_wrap)]
        param_fields.push((
            "configuration_generation".to_string(),
            JsonValue::Integer(generation as i64),
        ));
    }

    if !params.credential_references.is_empty() {
        let refs = params
            .credential_references
            .iter()
            .map(|r| JsonValue::String(r.clone()))
            .collect();
        param_fields.push(("credential_references".to_string(), JsonValue::Array(refs)));
    }

    let req = JsonValue::Object(vec![
        ("jsonrpc".to_string(), JsonValue::String("2.0".to_string())),
        ("id".to_string(), JsonValue::String(request_id.to_string())),
        (
            "method".to_string(),
            JsonValue::String("initialize".to_string()),
        ),
        ("params".to_string(), JsonValue::Object(param_fields)),
    ]);

    req.to_json_string()
}

fn serialize_health_check_request(request_id: &str, params: &HealthCheckParams) -> String {
    let mut param_fields = vec![(
        "correlation_id".to_string(),
        JsonValue::String(params.correlation_id.clone()),
    )];
    if params.include_diagnostics {
        param_fields.push(("include_diagnostics".to_string(), JsonValue::Bool(true)));
    }

    let req = JsonValue::Object(vec![
        ("jsonrpc".to_string(), JsonValue::String("2.0".to_string())),
        ("id".to_string(), JsonValue::String(request_id.to_string())),
        (
            "method".to_string(),
            JsonValue::String("health.check".to_string()),
        ),
        ("params".to_string(), JsonValue::Object(param_fields)),
    ]);

    req.to_json_string()
}

fn parse_initialize_response(
    line: &str,
    expected_id: &str,
) -> Result<InitializeResult, HandshakeError> {
    let json = JsonValue::parse(line).map_err(|e| HandshakeError::ProtocolViolation {
        code: -32099,
        domain_code: "ADAPTER_PROTOCOL_VIOLATION",
        message: format!("Malformed JSON: {e}"),
    })?;
    let is_error_response = validate_response_envelope(&json, expected_id).map_err(|message| {
        HandshakeError::ProtocolViolation {
            code: -32099,
            domain_code: "ADAPTER_PROTOCOL_VIOLATION",
            message,
        }
    })?;

    // Verify jsonrpc == "2.0"
    match json.get("jsonrpc").and_then(JsonValue::as_str) {
        Some("2.0") => {}
        _ => {
            return Err(HandshakeError::ProtocolViolation {
                code: -32099,
                domain_code: "ADAPTER_PROTOCOL_VIOLATION",
                message: "Missing or invalid 'jsonrpc' field in response".to_string(),
            });
        }
    }

    // Verify id
    if !is_error_response {
        match json.get("id").and_then(JsonValue::as_str) {
            Some(id) if id == expected_id => {}
            _ => {
                return Err(HandshakeError::ProtocolViolation {
                    code: -32099,
                    domain_code: "ADAPTER_PROTOCOL_VIOLATION",
                    message: "Response id mismatch or missing".to_string(),
                });
            }
        }
    }

    // Check for error response
    if is_error_response {
        let error = parse_adapter_rpc_error(json.get("error").expect("validated error response"))
            .map_err(|message| HandshakeError::ProtocolViolation {
            code: -32099,
            domain_code: "ADAPTER_PROTOCOL_VIOLATION",
            message,
        })?;
        if error.code == -32010 {
            return Err(HandshakeError::Incompatible {
                code: -32010,
                domain_code: "ADAPTER_INCOMPATIBLE",
                message: error.message,
                reported_version: "incompatible".to_string(),
            });
        }
        return Err(HandshakeError::RpcError(error));
    }

    // Must have result
    let result = json
        .get("result")
        .ok_or_else(|| HandshakeError::ProtocolViolation {
            code: -32099,
            domain_code: "ADAPTER_PROTOCOL_VIOLATION",
            message: "Missing 'result' in response".to_string(),
        })?;
    validate_schema_object(
        result,
        &[
            "kind",
            "protocol_version",
            "adapter_version",
            "capabilities",
            "auth_status",
            "isolation_mode",
            "recovery_mode",
        ],
        &["harness_version", "extensions"],
        "initialize result",
    )
    .map_err(|message| HandshakeError::ProtocolViolation {
        code: -32099,
        domain_code: "ADAPTER_PROTOCOL_VIOLATION",
        message,
    })?;
    if result
        .get("extensions")
        .is_some_and(|value| value.as_object().is_none())
    {
        return Err(HandshakeError::ProtocolViolation {
            code: -32099,
            domain_code: "ADAPTER_PROTOCOL_VIOLATION",
            message: "initialize result extensions must be an object".to_string(),
        });
    }

    // Check kind
    match result.get("kind").and_then(JsonValue::as_str) {
        Some("initialize") => {}
        _ => {
            return Err(HandshakeError::ProtocolViolation {
                code: -32099,
                domain_code: "ADAPTER_PROTOCOL_VIOLATION",
                message: "Missing or invalid 'kind' in initialize result".to_string(),
            });
        }
    }

    // Parse protocol_version
    let proto_ver_str = result
        .get("protocol_version")
        .and_then(JsonValue::as_str)
        .ok_or_else(|| HandshakeError::ProtocolViolation {
            code: -32099,
            domain_code: "ADAPTER_PROTOCOL_VIOLATION",
            message: "Missing 'protocol_version' in initialize result".to_string(),
        })?;
    if proto_ver_str.chars().count() > 128 {
        return Err(HandshakeError::ProtocolViolation {
            code: -32099,
            domain_code: "ADAPTER_PROTOCOL_VIOLATION",
            message: "protocol_version exceeds 128 characters".to_string(),
        });
    }

    let protocol_version = match SemVer::parse(proto_ver_str) {
        Ok(v) => v,
        Err(SemVerError::LeadingZero) | Err(SemVerError::InvalidFormat) | Err(_) => {
            return Err(HandshakeError::Incompatible {
                code: -32010,
                domain_code: "ADAPTER_INCOMPATIBLE",
                message: format!("Protocol version '{proto_ver_str}' is invalid SemVer 2.0.0"),
                reported_version: proto_ver_str.to_string(),
            });
        }
    };

    let target_version = SemVer::parse(PROTOCOL_VERSION_V0).expect("valid target SemVer");
    if !protocol_version.is_compatible_with(&target_version) {
        return Err(HandshakeError::Incompatible {
            code: -32010,
            domain_code: "ADAPTER_INCOMPATIBLE",
            message: format!(
                "Reported protocol version '{proto_ver_str}' is incompatible with platform '{PROTOCOL_VERSION_V0}'"
            ),
            reported_version: proto_ver_str.to_string(),
        });
    }

    // Parse adapter_version
    let adapter_ver_str = result
        .get("adapter_version")
        .and_then(JsonValue::as_str)
        .ok_or_else(|| HandshakeError::ProtocolViolation {
            code: -32099,
            domain_code: "ADAPTER_PROTOCOL_VIOLATION",
            message: "Missing 'adapter_version' in initialize result".to_string(),
        })?;
    if adapter_ver_str.chars().count() > 128 {
        return Err(HandshakeError::ProtocolViolation {
            code: -32099,
            domain_code: "ADAPTER_PROTOCOL_VIOLATION",
            message: "adapter_version exceeds 128 characters".to_string(),
        });
    }

    let adapter_version = match SemVer::parse(adapter_ver_str) {
        Ok(v) => v,
        Err(_) => {
            return Err(HandshakeError::Incompatible {
                code: -32010,
                domain_code: "ADAPTER_INCOMPATIBLE",
                message: format!("Adapter version '{adapter_ver_str}' is invalid SemVer 2.0.0"),
                reported_version: adapter_ver_str.to_string(),
            });
        }
    };

    let harness_version = match result.get("harness_version") {
        Some(JsonValue::String(value)) if value.chars().count() <= 128 => Some(value.clone()),
        Some(JsonValue::Null) | None => None,
        _ => {
            return Err(HandshakeError::ProtocolViolation {
                code: -32099,
                domain_code: "ADAPTER_PROTOCOL_VIOLATION",
                message: "Invalid harness_version in initialize result".to_string(),
            });
        }
    };

    // Parse capabilities
    let caps_val = result
        .get("capabilities")
        .and_then(JsonValue::as_array)
        .ok_or_else(|| HandshakeError::ProtocolViolation {
            code: -32099,
            domain_code: "ADAPTER_PROTOCOL_VIOLATION",
            message: "Missing or invalid 'capabilities' in initialize result".to_string(),
        })?;

    let mut capabilities = Vec::with_capacity(caps_val.len());
    for item in caps_val {
        if let Some(s) = item
            .as_str()
            .filter(|value| (1..=128).contains(&value.chars().count()))
        {
            if capabilities.iter().any(|capability| capability == s) {
                return Err(HandshakeError::ProtocolViolation {
                    code: -32099,
                    domain_code: "ADAPTER_PROTOCOL_VIOLATION",
                    message: "Duplicate capability in initialize result".to_string(),
                });
            }
            capabilities.push(s.to_string());
        } else {
            return Err(HandshakeError::ProtocolViolation {
                code: -32099,
                domain_code: "ADAPTER_PROTOCOL_VIOLATION",
                message: "Non-string capability found in initialize result".to_string(),
            });
        }
    }

    // Parse auth_status
    let auth_str = result
        .get("auth_status")
        .and_then(JsonValue::as_str)
        .ok_or_else(|| HandshakeError::ProtocolViolation {
            code: -32099,
            domain_code: "ADAPTER_PROTOCOL_VIOLATION",
            message: "Missing 'auth_status' in initialize result".to_string(),
        })?;
    let auth_status =
        AuthStatus::parse(auth_str).ok_or_else(|| HandshakeError::ProtocolViolation {
            code: -32099,
            domain_code: "ADAPTER_PROTOCOL_VIOLATION",
            message: format!("Unknown auth_status '{auth_str}' in initialize result"),
        })?;

    // Parse isolation_mode
    let iso_str = result
        .get("isolation_mode")
        .and_then(JsonValue::as_str)
        .ok_or_else(|| HandshakeError::ProtocolViolation {
            code: -32099,
            domain_code: "ADAPTER_PROTOCOL_VIOLATION",
            message: "Missing 'isolation_mode' in initialize result".to_string(),
        })?;
    let isolation_mode =
        IsolationMode::parse(iso_str).ok_or_else(|| HandshakeError::ProtocolViolation {
            code: -32099,
            domain_code: "ADAPTER_PROTOCOL_VIOLATION",
            message: format!("Unknown isolation_mode '{iso_str}' in initialize result"),
        })?;

    // Parse recovery_mode
    let rec_str = result
        .get("recovery_mode")
        .and_then(JsonValue::as_str)
        .ok_or_else(|| HandshakeError::ProtocolViolation {
            code: -32099,
            domain_code: "ADAPTER_PROTOCOL_VIOLATION",
            message: "Missing 'recovery_mode' in initialize result".to_string(),
        })?;
    let recovery_mode =
        RecoveryMode::parse(rec_str).ok_or_else(|| HandshakeError::ProtocolViolation {
            code: -32099,
            domain_code: "ADAPTER_PROTOCOL_VIOLATION",
            message: format!("Unknown recovery_mode '{rec_str}' in initialize result"),
        })?;

    Ok(InitializeResult {
        protocol_version,
        adapter_version,
        harness_version,
        capabilities,
        auth_status,
        isolation_mode,
        recovery_mode,
    })
}

fn parse_health_check_response(
    line: &str,
    expected_id: &str,
) -> Result<HealthResult, HealthCheckError> {
    let json = JsonValue::parse(line).map_err(|e| HealthCheckError::ProtocolViolation {
        code: -32099,
        domain_code: "ADAPTER_PROTOCOL_VIOLATION",
        message: format!("Malformed JSON: {e}"),
    })?;
    let is_error_response = validate_response_envelope(&json, expected_id).map_err(|message| {
        HealthCheckError::ProtocolViolation {
            code: -32099,
            domain_code: "ADAPTER_PROTOCOL_VIOLATION",
            message,
        }
    })?;

    // Verify jsonrpc == "2.0"
    match json.get("jsonrpc").and_then(JsonValue::as_str) {
        Some("2.0") => {}
        _ => {
            return Err(HealthCheckError::ProtocolViolation {
                code: -32099,
                domain_code: "ADAPTER_PROTOCOL_VIOLATION",
                message: "Missing or invalid 'jsonrpc' field in health response".to_string(),
            });
        }
    }

    // Verify id
    if !is_error_response {
        match json.get("id").and_then(JsonValue::as_str) {
            Some(id) if id == expected_id => {}
            _ => {
                return Err(HealthCheckError::ProtocolViolation {
                    code: -32099,
                    domain_code: "ADAPTER_PROTOCOL_VIOLATION",
                    message: "Health response id mismatch or missing".to_string(),
                });
            }
        }
    }

    // Check for error response
    if is_error_response {
        let error = parse_adapter_rpc_error(json.get("error").expect("validated error response"))
            .map_err(|message| HealthCheckError::ProtocolViolation {
            code: -32099,
            domain_code: "ADAPTER_PROTOCOL_VIOLATION",
            message,
        })?;
        return Err(HealthCheckError::RpcError(error));
    }

    // Must have result
    let result = json
        .get("result")
        .ok_or_else(|| HealthCheckError::ProtocolViolation {
            code: -32099,
            domain_code: "ADAPTER_PROTOCOL_VIOLATION",
            message: "Missing 'result' in health response".to_string(),
        })?;
    validate_schema_object(
        result,
        &["kind", "status", "checked_at", "diagnostic_codes"],
        &["details_reference"],
        "health result",
    )
    .map_err(|message| HealthCheckError::ProtocolViolation {
        code: -32099,
        domain_code: "ADAPTER_PROTOCOL_VIOLATION",
        message,
    })?;

    // Check kind
    match result.get("kind").and_then(JsonValue::as_str) {
        Some("health") => {}
        _ => {
            return Err(HealthCheckError::ProtocolViolation {
                code: -32099,
                domain_code: "ADAPTER_PROTOCOL_VIOLATION",
                message: "Missing or invalid 'kind' in health result".to_string(),
            });
        }
    }

    // Parse status
    let status_str = result
        .get("status")
        .and_then(JsonValue::as_str)
        .ok_or_else(|| HealthCheckError::ProtocolViolation {
            code: -32099,
            domain_code: "ADAPTER_PROTOCOL_VIOLATION",
            message: "Missing 'status' in health result".to_string(),
        })?;
    let status =
        HealthStatus::parse(status_str).ok_or_else(|| HealthCheckError::ProtocolViolation {
            code: -32099,
            domain_code: "ADAPTER_PROTOCOL_VIOLATION",
            message: format!("Unknown status '{status_str}' in health result"),
        })?;

    // Parse checked_at
    let checked_at = result
        .get("checked_at")
        .and_then(JsonValue::as_str)
        .ok_or_else(|| HealthCheckError::ProtocolViolation {
            code: -32099,
            domain_code: "ADAPTER_PROTOCOL_VIOLATION",
            message: "Missing 'checked_at' in health result".to_string(),
        })?;
    if !is_valid_utc_rfc3339_milliseconds(checked_at) {
        return Err(HealthCheckError::ProtocolViolation {
            code: -32099,
            domain_code: "ADAPTER_PROTOCOL_VIOLATION",
            message: "Invalid 'checked_at' in health result; expected UTC RFC 3339 milliseconds"
                .to_string(),
        });
    }
    let checked_at = checked_at.to_string();

    // Parse diagnostic_codes
    let diag_val = result
        .get("diagnostic_codes")
        .and_then(JsonValue::as_array)
        .ok_or_else(|| HealthCheckError::ProtocolViolation {
            code: -32099,
            domain_code: "ADAPTER_PROTOCOL_VIOLATION",
            message: "Missing 'diagnostic_codes' in health result".to_string(),
        })?;

    let mut diagnostic_codes = Vec::with_capacity(diag_val.len());
    if diag_val.len() > 64 {
        return Err(HealthCheckError::ProtocolViolation {
            code: -32099,
            domain_code: "ADAPTER_PROTOCOL_VIOLATION",
            message: "health diagnostic_codes exceeds 64 items".to_string(),
        });
    }
    for item in diag_val {
        if let Some(code) = item.as_str().filter(|code| is_failure_code(code)) {
            diagnostic_codes.push(code.to_string());
        } else {
            return Err(HealthCheckError::ProtocolViolation {
                code: -32099,
                domain_code: "ADAPTER_PROTOCOL_VIOLATION",
                message: "Non-string diagnostic code in health result".to_string(),
            });
        }
    }

    // Parse details_reference if present
    let details_reference = match result.get("details_reference") {
        Some(JsonValue::Null) | None => None,
        Some(value) => {
            Some(
                parse_reference(value).ok_or_else(|| HealthCheckError::ProtocolViolation {
                    code: -32099,
                    domain_code: "ADAPTER_PROTOCOL_VIOLATION",
                    message: "Invalid details_reference in health result".to_string(),
                })?,
            )
        }
    };

    Ok(HealthResult {
        status,
        checked_at,
        diagnostic_codes,
        details_reference,
    })
}

fn serialize_reference(r: &ArtifactReference) -> JsonValue {
    let mut fields = vec![
        ("kind".to_string(), JsonValue::String(r.kind.clone())),
        ("uri".to_string(), JsonValue::String(r.uri.clone())),
    ];
    if let Some(version) = &r.version {
        fields.push(("version".to_string(), JsonValue::String(version.clone())));
    }
    if let Some(checksum) = &r.checksum {
        fields.push(("checksum".to_string(), JsonValue::String(checksum.clone())));
    }
    if let Some(media_type) = &r.media_type {
        fields.push((
            "media_type".to_string(),
            JsonValue::String(media_type.clone()),
        ));
    }
    JsonValue::Object(fields)
}

fn serialize_run_start_request(request_id: &str, params: &RunStartParams) -> String {
    #[allow(clippy::cast_possible_wrap)]
    let binding_version_i64 = params.context.binding_version as i64;
    #[allow(clippy::cast_possible_wrap)]
    let fencing_token_i64 = params.context.fencing_token as i64;
    #[allow(clippy::cast_possible_wrap)]
    let heartbeat_interval_i64 = params.heartbeat_interval_ms as i64;
    #[allow(clippy::cast_possible_wrap)]
    let timeout_i64 = params.timeout_ms as i64;

    let mut param_fields = vec![
        (
            "team_id".to_string(),
            JsonValue::String(params.context.team_id.clone()),
        ),
        (
            "task_id".to_string(),
            JsonValue::String(params.context.task_id.clone()),
        ),
        (
            "run_id".to_string(),
            JsonValue::String(params.context.run_id.clone()),
        ),
        (
            "role_id".to_string(),
            JsonValue::String(params.context.role_id.clone()),
        ),
        (
            "binding_id".to_string(),
            JsonValue::String(params.context.binding_id.clone()),
        ),
        (
            "binding_version".to_string(),
            JsonValue::Integer(binding_version_i64),
        ),
        (
            "fencing_token".to_string(),
            JsonValue::Integer(fencing_token_i64),
        ),
        (
            "idempotency_key".to_string(),
            JsonValue::String(params.idempotency_key.clone()),
        ),
        (
            "task_title".to_string(),
            JsonValue::String(params.task_title.clone()),
        ),
        (
            "role_name".to_string(),
            JsonValue::String(params.role_name.clone()),
        ),
        (
            "role_instructions".to_string(),
            JsonValue::String(params.role_instructions.clone()),
        ),
        (
            "references".to_string(),
            JsonValue::Array(params.references.iter().map(serialize_reference).collect()),
        ),
        (
            "shared_workspace_root".to_string(),
            JsonValue::String(params.shared_workspace_root.clone()),
        ),
        (
            "execution_workspace_root".to_string(),
            JsonValue::String(params.execution_workspace_root.clone()),
        ),
        (
            "read_scopes".to_string(),
            JsonValue::Array(
                params
                    .read_scopes
                    .iter()
                    .map(|s| JsonValue::String(s.clone()))
                    .collect(),
            ),
        ),
        (
            "write_scopes".to_string(),
            JsonValue::Array(
                params
                    .write_scopes
                    .iter()
                    .map(|s| JsonValue::String(s.clone()))
                    .collect(),
            ),
        ),
        (
            "heartbeat_interval_ms".to_string(),
            JsonValue::Integer(heartbeat_interval_i64),
        ),
        ("timeout_ms".to_string(), JsonValue::Integer(timeout_i64)),
        (
            "correlation_id".to_string(),
            JsonValue::String(params.correlation_id.clone()),
        ),
    ];

    if let Some(max_output) = params.max_output_bytes {
        #[allow(clippy::cast_possible_wrap)]
        param_fields.push((
            "max_output_bytes".to_string(),
            JsonValue::Integer(max_output as i64),
        ));
    }

    if !params.verification_commands.is_empty() {
        let vcmds = params
            .verification_commands
            .iter()
            .map(|vc| {
                JsonValue::Object(vec![
                    ("program".to_string(), JsonValue::String(vc.program.clone())),
                    (
                        "args".to_string(),
                        JsonValue::Array(
                            vc.args
                                .iter()
                                .map(|a| JsonValue::String(a.clone()))
                                .collect(),
                        ),
                    ),
                ])
            })
            .collect();
        param_fields.push(("verification_commands".to_string(), JsonValue::Array(vcmds)));
    }

    if !params.credential_references.is_empty() {
        let creds = params
            .credential_references
            .iter()
            .map(|c| JsonValue::String(c.clone()))
            .collect();
        param_fields.push(("credential_references".to_string(), JsonValue::Array(creds)));
    }

    let req = JsonValue::Object(vec![
        ("jsonrpc".to_string(), JsonValue::String("2.0".to_string())),
        ("id".to_string(), JsonValue::String(request_id.to_string())),
        (
            "method".to_string(),
            JsonValue::String("run.start".to_string()),
        ),
        ("params".to_string(), JsonValue::Object(param_fields)),
    ]);

    req.to_json_string()
}

fn parse_run_start_response(
    line: &str,
    expected_id: &str,
) -> Result<RunStartResult, RunExecutionError> {
    let json = JsonValue::parse(line).map_err(|e| RunExecutionError::ProtocolViolation {
        code: -32099,
        domain_code: "ADAPTER_PROTOCOL_VIOLATION",
        message: format!("Malformed JSON: {e}"),
    })?;
    let is_error_response =
        validate_response_envelope(&json, expected_id).map_err(protocol_violation)?;

    // Verify jsonrpc == "2.0"
    match json.get("jsonrpc").and_then(JsonValue::as_str) {
        Some("2.0") => {}
        _ => {
            return Err(RunExecutionError::ProtocolViolation {
                code: -32099,
                domain_code: "ADAPTER_PROTOCOL_VIOLATION",
                message: "Missing or invalid 'jsonrpc' field in run.start response".to_string(),
            });
        }
    }

    // Verify id
    if !is_error_response {
        match json.get("id").and_then(JsonValue::as_str) {
            Some(id) if id == expected_id => {}
            _ => {
                return Err(RunExecutionError::ProtocolViolation {
                    code: -32099,
                    domain_code: "ADAPTER_PROTOCOL_VIOLATION",
                    message: "run.start response id mismatch or missing".to_string(),
                });
            }
        }
    }

    // Check for error response
    if is_error_response {
        let error = parse_adapter_rpc_error(json.get("error").expect("validated error response"))
            .map_err(protocol_violation)?;
        if error.code == -32014 {
            return Err(RunExecutionError::Fenced {
                code: -32014,
                domain_code: "RUN_FENCED",
                message: error.message,
                expected_token: 0,
                received_token: 0,
            });
        }
        return Err(RunExecutionError::RpcError(error));
    }

    // Must have result
    let result = json
        .get("result")
        .ok_or_else(|| RunExecutionError::ProtocolViolation {
            code: -32099,
            domain_code: "ADAPTER_PROTOCOL_VIOLATION",
            message: "Missing 'result' in run.start response".to_string(),
        })?;
    validate_schema_object(
        result,
        &["kind", "accepted", "observed_at"],
        &["session_reference"],
        "run.start result",
    )
    .map_err(protocol_violation)?;

    // Check kind
    match result.get("kind").and_then(JsonValue::as_str) {
        Some("run_start") => {}
        _ => {
            return Err(RunExecutionError::ProtocolViolation {
                code: -32099,
                domain_code: "ADAPTER_PROTOCOL_VIOLATION",
                message: "Missing or invalid 'kind' in run.start result".to_string(),
            });
        }
    }

    let accepted = result
        .get("accepted")
        .and_then(JsonValue::as_bool)
        .ok_or_else(|| RunExecutionError::ProtocolViolation {
            code: -32099,
            domain_code: "ADAPTER_PROTOCOL_VIOLATION",
            message: "Missing 'accepted' in run.start result".to_string(),
        })?;

    let session_reference = match result.get("session_reference") {
        Some(JsonValue::String(value)) if value.chars().count() <= 2048 => Some(value.clone()),
        Some(JsonValue::Null) | None => None,
        _ => {
            return Err(protocol_violation(
                "Invalid session_reference in run.start result",
            ));
        }
    };

    let observed_at = result
        .get("observed_at")
        .and_then(JsonValue::as_str)
        .ok_or_else(|| RunExecutionError::ProtocolViolation {
            code: -32099,
            domain_code: "ADAPTER_PROTOCOL_VIOLATION",
            message: "Missing 'observed_at' in run.start result".to_string(),
        })?;
    if !is_valid_utc_rfc3339_milliseconds(observed_at) {
        return Err(RunExecutionError::ProtocolViolation {
            code: -32099,
            domain_code: "ADAPTER_PROTOCOL_VIOLATION",
            message:
                "Invalid 'observed_at' in run.start result; expected UTC RFC 3339 milliseconds"
                    .to_string(),
        });
    }
    let observed_at = observed_at.to_string();

    Ok(RunStartResult {
        accepted,
        session_reference,
        observed_at,
    })
}

fn serialize_run_cancel_request(request_id: &str, params: &RunCancelParams) -> String {
    #[allow(clippy::cast_possible_wrap)]
    let binding_version_i64 = params.context.binding_version as i64;
    #[allow(clippy::cast_possible_wrap)]
    let fencing_token_i64 = params.context.fencing_token as i64;

    let mut param_fields = vec![
        (
            "team_id".to_string(),
            JsonValue::String(params.context.team_id.clone()),
        ),
        (
            "task_id".to_string(),
            JsonValue::String(params.context.task_id.clone()),
        ),
        (
            "run_id".to_string(),
            JsonValue::String(params.context.run_id.clone()),
        ),
        (
            "role_id".to_string(),
            JsonValue::String(params.context.role_id.clone()),
        ),
        (
            "binding_id".to_string(),
            JsonValue::String(params.context.binding_id.clone()),
        ),
        (
            "binding_version".to_string(),
            JsonValue::Integer(binding_version_i64),
        ),
        (
            "fencing_token".to_string(),
            JsonValue::Integer(fencing_token_i64),
        ),
        (
            "reason".to_string(),
            JsonValue::String(params.reason.clone()),
        ),
        (
            "requested_at".to_string(),
            JsonValue::String(params.requested_at.clone()),
        ),
        (
            "correlation_id".to_string(),
            JsonValue::String(params.correlation_id.clone()),
        ),
    ];

    if let Some(force) = params.force_after_ms {
        #[allow(clippy::cast_possible_wrap)]
        param_fields.push((
            "force_after_ms".to_string(),
            JsonValue::Integer(force as i64),
        ));
    }

    let req = JsonValue::Object(vec![
        ("jsonrpc".to_string(), JsonValue::String("2.0".to_string())),
        ("id".to_string(), JsonValue::String(request_id.to_string())),
        (
            "method".to_string(),
            JsonValue::String("run.cancel".to_string()),
        ),
        ("params".to_string(), JsonValue::Object(param_fields)),
    ]);

    req.to_json_string()
}

fn parse_run_cancel_response(
    line: &str,
    expected_id: &str,
) -> Result<RunCancelResult, RunExecutionError> {
    let json = JsonValue::parse(line).map_err(|e| RunExecutionError::ProtocolViolation {
        code: -32099,
        domain_code: "ADAPTER_PROTOCOL_VIOLATION",
        message: format!("Malformed JSON: {e}"),
    })?;
    let is_error_response =
        validate_response_envelope(&json, expected_id).map_err(protocol_violation)?;

    // Verify jsonrpc == "2.0"
    match json.get("jsonrpc").and_then(JsonValue::as_str) {
        Some("2.0") => {}
        _ => {
            return Err(RunExecutionError::ProtocolViolation {
                code: -32099,
                domain_code: "ADAPTER_PROTOCOL_VIOLATION",
                message: "Missing or invalid 'jsonrpc' field in run.cancel response".to_string(),
            });
        }
    }

    // Verify id
    if !is_error_response {
        match json.get("id").and_then(JsonValue::as_str) {
            Some(id) if id == expected_id => {}
            _ => {
                return Err(RunExecutionError::ProtocolViolation {
                    code: -32099,
                    domain_code: "ADAPTER_PROTOCOL_VIOLATION",
                    message: "run.cancel response id mismatch or missing".to_string(),
                });
            }
        }
    }

    // Check for error response
    if is_error_response {
        let error = parse_adapter_rpc_error(json.get("error").expect("validated error response"))
            .map_err(protocol_violation)?;
        return Err(RunExecutionError::RpcError(error));
    }

    let result = json
        .get("result")
        .ok_or_else(|| RunExecutionError::ProtocolViolation {
            code: -32099,
            domain_code: "ADAPTER_PROTOCOL_VIOLATION",
            message: "Missing 'result' in run.cancel response".to_string(),
        })?;
    validate_schema_object(
        result,
        &["kind", "accepted", "confirmed", "observed_at"],
        &[],
        "run.cancel result",
    )
    .map_err(protocol_violation)?;

    match result.get("kind").and_then(JsonValue::as_str) {
        Some("run_cancel") => {}
        _ => {
            return Err(RunExecutionError::ProtocolViolation {
                code: -32099,
                domain_code: "ADAPTER_PROTOCOL_VIOLATION",
                message: "Missing or invalid 'kind' in run.cancel result".to_string(),
            });
        }
    }

    let accepted = result
        .get("accepted")
        .and_then(JsonValue::as_bool)
        .ok_or_else(|| RunExecutionError::ProtocolViolation {
            code: -32099,
            domain_code: "ADAPTER_PROTOCOL_VIOLATION",
            message: "Missing 'accepted' in run.cancel result".to_string(),
        })?;

    let confirmed = result
        .get("confirmed")
        .and_then(JsonValue::as_bool)
        .ok_or_else(|| RunExecutionError::ProtocolViolation {
            code: -32099,
            domain_code: "ADAPTER_PROTOCOL_VIOLATION",
            message: "Missing 'confirmed' in run.cancel result".to_string(),
        })?;

    let observed_at = result
        .get("observed_at")
        .and_then(JsonValue::as_str)
        .ok_or_else(|| RunExecutionError::ProtocolViolation {
            code: -32099,
            domain_code: "ADAPTER_PROTOCOL_VIOLATION",
            message: "Missing 'observed_at' in run.cancel result".to_string(),
        })?;
    if !is_valid_utc_rfc3339_milliseconds(observed_at) {
        return Err(RunExecutionError::ProtocolViolation {
            code: -32099,
            domain_code: "ADAPTER_PROTOCOL_VIOLATION",
            message:
                "Invalid 'observed_at' in run.cancel result; expected UTC RFC 3339 milliseconds"
                    .to_string(),
        });
    }
    let observed_at = observed_at.to_string();

    Ok(RunCancelResult {
        accepted,
        confirmed,
        observed_at,
    })
}

const MAX_PROTOCOL_JSON_UTF8_BYTES: usize = 1_048_576;
const MAX_RUN_OUTPUT_CONTENT_CHARS: usize = 65_536;
const MAX_RUN_OUTPUT_CONTENT_UTF8_BYTES: usize = 65_536;
const MAX_RESULT_REFERENCES: usize = 128;
const MAX_SUMMARY_CHARS: usize = 8_000;
const MAX_REASON_KEY_CHARS: usize = 128;
const RUN_EVENT_CONTEXT_FIELDS: &[&str] = &[
    "team_id",
    "task_id",
    "run_id",
    "role_id",
    "binding_id",
    "binding_version",
    "fencing_token",
    "sequence",
];
const RUN_FAILURE_CLASSES: &[&str] = &[
    "transient",
    "authentication",
    "capability",
    "permission",
    "context",
    "execution",
    "timeout",
    "cancelled",
    "protocol",
    "internal",
];
const STANDARD_JSON_RPC_ERROR_CODES: &[i64] = &[-32700, -32600, -32601, -32602, -32603];

fn validate_schema_object(
    value: &JsonValue,
    required: &[&str],
    optional: &[&str],
    object_name: &str,
) -> Result<(), String> {
    let object = value
        .as_object()
        .ok_or_else(|| format!("{object_name} must be an object"))?;
    if let Some(field) = required
        .iter()
        .find(|field| !object.iter().any(|(key, _)| key == **field))
    {
        return Err(format!("{object_name} is missing required field '{field}'"));
    }
    if object.iter().any(|(key, _)| {
        !required
            .iter()
            .chain(optional)
            .any(|allowed| key == *allowed)
    }) {
        return Err(format!("{object_name} contains an unknown field"));
    }
    Ok(())
}

fn validate_response_envelope(json: &JsonValue, expected_id: &str) -> Result<bool, String> {
    validate_schema_object(json, &["jsonrpc", "id"], &["result", "error"], "response")?;
    if json.get("jsonrpc").and_then(JsonValue::as_str) != Some("2.0") {
        return Err("response jsonrpc must equal '2.0'".to_string());
    }
    let has_result = json.get("result").is_some();
    let has_error = json.get("error").is_some();
    if has_result == has_error {
        return Err("response must contain exactly one of result or error".to_string());
    }
    match json.get("id") {
        Some(JsonValue::String(id)) if (1..=128).contains(&id.chars().count()) => {
            if id != expected_id {
                return Err("response id mismatch".to_string());
            }
        }
        _ => return Err("response id cannot be associated with the request".to_string()),
    }
    Ok(has_error)
}

fn parse_adapter_rpc_error(value: &JsonValue) -> Result<AdapterRpcError, String> {
    validate_schema_object(value, &["code", "message"], &["data"], "error")?;
    let code = value
        .get("code")
        .and_then(json_integer)
        .ok_or_else(|| "error code must be an integer".to_string())?;
    let message = value
        .get("message")
        .and_then(JsonValue::as_str)
        .filter(|message| (1..=1000).contains(&message.chars().count()))
        .ok_or_else(|| "error message must contain 1 to 1000 characters".to_string())?
        .to_string();

    if STANDARD_JSON_RPC_ERROR_CODES.contains(&code) {
        return Ok(AdapterRpcError {
            code: i32::try_from(code).map_err(|_| "standard error code out of range")?,
            message,
            domain_code: None,
            retryable: false,
            correlation_id: None,
            reason_key: None,
        });
    }

    let (domain_code, retryable) = custom_error_mapping(code)
        .ok_or_else(|| "error code is not a frozen Adapter custom error".to_string())?;
    let data = value
        .get("data")
        .ok_or_else(|| "custom error requires data".to_string())?;
    validate_schema_object(
        data,
        &["domain_code", "retryable", "correlation_id"],
        &["reason_key"],
        "custom error data",
    )?;
    if data.get("domain_code").and_then(JsonValue::as_str) != Some(domain_code)
        || data.get("retryable").and_then(JsonValue::as_bool) != Some(retryable)
    {
        return Err("custom error code, domain_code, and retryable mapping mismatch".to_string());
    }
    let correlation_id = data
        .get("correlation_id")
        .and_then(JsonValue::as_str)
        .filter(|value| is_valid_uuid_v7(value))
        .ok_or_else(|| "custom error correlation_id must be canonical UUIDv7".to_string())?
        .to_string();
    let reason_key = match data.get("reason_key") {
        Some(JsonValue::String(value)) if value.chars().count() <= MAX_REASON_KEY_CHARS => {
            Some(value.clone())
        }
        Some(JsonValue::Null) | None => None,
        _ => return Err("custom error reason_key is invalid".to_string()),
    };
    Ok(AdapterRpcError {
        code: i32::try_from(code).map_err(|_| "custom error code out of range")?,
        message,
        domain_code: Some(domain_code.to_string()),
        retryable,
        correlation_id: Some(correlation_id),
        reason_key,
    })
}

fn custom_error_mapping(code: i64) -> Option<(&'static str, bool)> {
    match code {
        -32010 => Some(("ADAPTER_INCOMPATIBLE", false)),
        -32011 => Some(("ADAPTER_CAPABILITY_MISSING", false)),
        -32012 => Some(("ADAPTER_AUTHENTICATION_FAILED", false)),
        -32013 => Some(("BINDING_STALE_VERSION", false)),
        -32014 => Some(("RUN_FENCED", false)),
        -32015 => Some(("RUN_OUTPUT_OUT_OF_ORDER", true)),
        -32016 => Some(("WORKSPACE_PATH_OUTSIDE_SCOPE", false)),
        -32017 => Some(("ADAPTER_UNAVAILABLE", true)),
        -32018 => Some(("CONCURRENCY_IDEMPOTENCY_CONFLICT", false)),
        -32099 => Some(("ADAPTER_PROTOCOL_VIOLATION", false)),
        _ => None,
    }
}

fn validate_initialize_params(params: &InitializeParams) -> Result<(), HandshakeError> {
    for (field, value) in [
        ("harness_instance_id", params.harness_instance_id.as_str()),
        ("correlation_id", params.correlation_id.as_str()),
    ] {
        if !is_valid_uuid_v7(value) {
            return Err(HandshakeError::ProtocolViolation {
                code: -32099,
                domain_code: "ADAPTER_PROTOCOL_VIOLATION",
                message: format!("Invalid '{field}' in initialize; expected canonical UUIDv7"),
            });
        }
    }
    Ok(())
}

fn validate_health_check_params(params: &HealthCheckParams) -> Result<(), HealthCheckError> {
    if !is_valid_uuid_v7(&params.correlation_id) {
        return Err(HealthCheckError::ProtocolViolation {
            code: -32099,
            domain_code: "ADAPTER_PROTOCOL_VIOLATION",
            message: "Invalid 'correlation_id' in health.check; expected canonical UUIDv7"
                .to_string(),
        });
    }
    Ok(())
}

fn validate_run_cancel_params(params: &RunCancelParams) -> Result<(), RunExecutionError> {
    validate_run_context(&params.context)?;
    validate_uuid_v7("correlation_id", &params.correlation_id)?;
    if !is_valid_utc_rfc3339_milliseconds(&params.requested_at) {
        return Err(RunExecutionError::ProtocolViolation {
            code: -32099,
            domain_code: "ADAPTER_PROTOCOL_VIOLATION",
            message: "Invalid 'requested_at' in run.cancel; expected UTC RFC 3339 milliseconds"
                .to_string(),
        });
    }
    Ok(())
}

fn validate_run_start_params(params: &RunStartParams) -> Result<(), RunExecutionError> {
    validate_run_context(&params.context)?;
    validate_uuid_v7("correlation_id", &params.correlation_id)
}

fn validate_run_context(context: &RunContext) -> Result<(), RunExecutionError> {
    for (field, value) in [
        ("team_id", context.team_id.as_str()),
        ("task_id", context.task_id.as_str()),
        ("run_id", context.run_id.as_str()),
        ("role_id", context.role_id.as_str()),
        ("binding_id", context.binding_id.as_str()),
    ] {
        validate_uuid_v7(field, value)?;
    }

    if context.binding_version == 0 || context.fencing_token == 0 {
        return Err(protocol_violation(
            "Run context binding_version and fencing_token must be at least 1",
        ));
    }

    Ok(())
}

fn validate_uuid_v7(field: &str, value: &str) -> Result<(), RunExecutionError> {
    if is_valid_uuid_v7(value) {
        Ok(())
    } else {
        Err(protocol_violation(format!(
            "Invalid '{field}'; expected canonical UUIDv7"
        )))
    }
}

fn protocol_violation(message: impl Into<String>) -> RunExecutionError {
    RunExecutionError::ProtocolViolation {
        code: -32099,
        domain_code: "ADAPTER_PROTOCOL_VIOLATION",
        message: message.into(),
    }
}

fn validate_closed_object(
    value: &JsonValue,
    required: &[&str],
    optional: &[&str],
    object_name: &str,
) -> Result<(), RunExecutionError> {
    let object = value
        .as_object()
        .ok_or_else(|| protocol_violation(format!("{object_name} must be an object")))?;

    if let Some(missing) = required
        .iter()
        .find(|field| !object.iter().any(|(key, _)| key == **field))
    {
        return Err(protocol_violation(format!(
            "{object_name} is missing required field '{missing}'"
        )));
    }

    if object.iter().any(|(key, _)| {
        !required
            .iter()
            .chain(optional)
            .any(|allowed| key == *allowed)
    }) {
        return Err(protocol_violation(format!(
            "{object_name} contains an unknown field"
        )));
    }

    Ok(())
}

fn validate_run_event_context(params: &JsonValue) -> Result<(), RunExecutionError> {
    for field in ["team_id", "task_id", "run_id", "role_id", "binding_id"] {
        let id = params
            .get(field)
            .and_then(JsonValue::as_str)
            .ok_or_else(|| {
                protocol_violation(format!("Missing or invalid '{field}' in notification"))
            })?;
        if !is_valid_uuid_v7(id) {
            return Err(protocol_violation(format!(
                "Invalid '{field}' in notification; expected canonical UUIDv7"
            )));
        }
    }

    for field in ["binding_version", "fencing_token", "sequence"] {
        if !params
            .get(field)
            .and_then(json_integer)
            .is_some_and(|value| value >= 1)
        {
            return Err(protocol_violation(format!(
                "Invalid '{field}' in notification; expected integer at least 1"
            )));
        }
    }

    Ok(())
}

fn json_integer(value: &JsonValue) -> Option<i64> {
    match value {
        JsonValue::Integer(value) => Some(*value),
        _ => None,
    }
}

fn is_valid_uuid_v7(value: &str) -> bool {
    let bytes = value.as_bytes();
    if bytes.len() != 36
        || bytes[8] != b'-'
        || bytes[13] != b'-'
        || bytes[18] != b'-'
        || bytes[23] != b'-'
        || bytes[14] != b'7'
        || !matches!(bytes[19], b'8' | b'9' | b'a' | b'b')
    {
        return false;
    }

    bytes.iter().enumerate().all(|(index, byte)| {
        matches!(index, 8 | 13 | 18 | 23) || byte.is_ascii_digit() || matches!(byte, b'a'..=b'f')
    })
}

fn is_failure_code(value: &str) -> bool {
    let bytes = value.as_bytes();
    (3..=64).contains(&bytes.len())
        && matches!(bytes.first(), Some(b'A'..=b'Z'))
        && bytes
            .iter()
            .all(|byte| byte.is_ascii_uppercase() || byte.is_ascii_digit() || *byte == b'_')
}

fn parse_reference(val: &JsonValue) -> Option<ArtifactReference> {
    const REFERENCE_FIELDS: &[&str] = &["kind", "uri", "version", "checksum", "media_type"];
    const REFERENCE_KINDS: &[&str] = &[
        "workspace_file",
        "workspace_anchor",
        "git_commit",
        "git_diff",
        "report",
        "artifact",
        "controlled_url",
    ];

    let obj = val.as_object()?;
    if !obj.iter().any(|(key, _)| key == "kind")
        || !obj.iter().any(|(key, _)| key == "uri")
        || obj
            .iter()
            .any(|(key, _)| !REFERENCE_FIELDS.iter().any(|allowed| key == *allowed))
    {
        return None;
    }

    let kind = val.get("kind")?.as_str()?;
    if !REFERENCE_KINDS.contains(&kind) {
        return None;
    }
    let uri = val.get("uri")?.as_str()?;
    if !(1..=4096).contains(&uri.chars().count()) {
        return None;
    }

    let optional_string = |field: &str, max_length: usize| match val.get(field) {
        Some(JsonValue::String(value)) if value.chars().count() <= max_length => {
            Some(Some(value.clone()))
        }
        Some(JsonValue::Null) | None => Some(None),
        _ => None,
    };

    Some(ArtifactReference {
        kind: kind.to_string(),
        uri: uri.to_string(),
        version: optional_string("version", 256)?,
        checksum: optional_string("checksum", 256)?,
        media_type: optional_string("media_type", 128)?,
    })
}

fn is_valid_utc_rfc3339_milliseconds(value: &str) -> bool {
    let bytes = value.as_bytes();
    if bytes.len() != 24
        || bytes[4] != b'-'
        || bytes[7] != b'-'
        || bytes[10] != b'T'
        || bytes[13] != b':'
        || bytes[16] != b':'
        || bytes[19] != b'.'
        || bytes[23] != b'Z'
    {
        return false;
    }

    if [0, 1, 2, 3, 5, 6, 8, 9, 11, 12, 14, 15, 17, 18, 20, 21, 22]
        .iter()
        .any(|&index| !bytes[index].is_ascii_digit())
    {
        return false;
    }

    let year = decimal_component(bytes, 0, 4);
    let month = decimal_component(bytes, 5, 7);
    let day = decimal_component(bytes, 8, 10);
    let hour = decimal_component(bytes, 11, 13);
    let minute = decimal_component(bytes, 14, 16);
    let second = decimal_component(bytes, 17, 19);

    if !(1..=12).contains(&month) || hour > 23 || minute > 59 || second > 59 {
        return false;
    }

    let days_in_month = match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if year.is_multiple_of(4) && (!year.is_multiple_of(100) || year.is_multiple_of(400)) => {
            29
        }
        2 => 28,
        _ => return false,
    };

    (1..=days_in_month).contains(&day)
}

fn decimal_component(bytes: &[u8], start: usize, end: usize) -> u32 {
    bytes[start..end]
        .iter()
        .fold(0, |value, digit| value * 10 + u32::from(*digit - b'0'))
}

#[cfg(test)]
mod tests {
    use std::{env, ffi::OsString, path::PathBuf, time::Duration};

    use crate::{AdapterCommand, Supervisor, SupervisorConfig, SupervisorError};

    use super::{
        HandshakeError, InitializeParams, RunCancelParams, RunContext, RunExecutionError,
        RunNotificationValidator, parse_health_check_response, parse_initialize_response,
        parse_run_cancel_response, parse_run_start_response,
    };

    const VALID_TIMESTAMP: &str = "2026-09-28T01:02:03.004Z";
    const TEAM_ID: &str = "01992d6d-2f31-7b11-8aa8-c975a632c001";
    const TASK_ID: &str = "01992d6d-2f31-7b11-8aa8-c975a632c002";
    const RUN_ID: &str = "01992d6d-2f31-7b11-8aa8-c975a632c003";
    const ROLE_ID: &str = "01992d6d-2f31-7b11-8aa8-c975a632c004";
    const BINDING_ID: &str = "01992d6d-2f31-7b11-8aa8-c975a632c005";

    fn run_context() -> RunContext {
        RunContext::new(TEAM_ID, TASK_ID, RUN_ID, ROLE_ID, BINDING_ID, 1, 1)
    }

    fn notification(method: &str, timestamp_field: &str, timestamp: &str) -> String {
        let extra = match method {
            "run.heartbeat" => "\"phase\":\"running\"",
            "run.output" => "\"kind\":\"stdout\",\"content\":\"output\"",
            "run.completed" => "\"result_references\":[]",
            "run.failed" => {
                "\"error_class\":\"transient\",\"code\":\"TEMPORARY\",\"retryable\":true"
            }
            _ => unreachable!("test only uses supported notifications"),
        };
        format!(
            concat!(
                "{{\"jsonrpc\":\"2.0\",\"method\":\"{method}\",\"params\":{{",
                "\"team_id\":\"{TEAM_ID}\",\"task_id\":\"{TASK_ID}\",\"run_id\":\"{RUN_ID}\",",
                "\"role_id\":\"{ROLE_ID}\",\"binding_id\":\"{BINDING_ID}\",\"binding_version\":1,",
                "\"fencing_token\":1,\"sequence\":1,\"{timestamp_field}\":\"{timestamp}\",{extra}}}}}"
            ),
            method = method,
            timestamp_field = timestamp_field,
            timestamp = timestamp,
            extra = extra,
            TEAM_ID = TEAM_ID,
            TASK_ID = TASK_ID,
            RUN_ID = RUN_ID,
            ROLE_ID = ROLE_ID,
            BINDING_ID = BINDING_ID,
        )
    }

    #[test]
    fn response_timestamps_require_valid_utc_rfc3339_milliseconds() {
        let invalid_timestamp = "2026-09-28T01:02:03Z";

        let health = format!(
            "{{\"jsonrpc\":\"2.0\",\"id\":\"health\",\"result\":{{\"kind\":\"health\",\"status\":\"healthy\",\"checked_at\":\"{invalid_timestamp}\",\"diagnostic_codes\":[]}}}}"
        );
        assert!(matches!(
            parse_health_check_response(&health, "health"),
            Err(super::HealthCheckError::ProtocolViolation { .. })
        ));

        let start = format!(
            "{{\"jsonrpc\":\"2.0\",\"id\":\"start\",\"result\":{{\"kind\":\"run_start\",\"accepted\":true,\"observed_at\":\"{invalid_timestamp}\"}}}}"
        );
        assert!(matches!(
            parse_run_start_response(&start, "start"),
            Err(RunExecutionError::ProtocolViolation { .. })
        ));

        let cancel = format!(
            "{{\"jsonrpc\":\"2.0\",\"id\":\"cancel\",\"result\":{{\"kind\":\"run_cancel\",\"accepted\":true,\"confirmed\":true,\"observed_at\":\"{invalid_timestamp}\"}}}}"
        );
        assert!(matches!(
            parse_run_cancel_response(&cancel, "cancel"),
            Err(RunExecutionError::ProtocolViolation { .. })
        ));
    }

    #[test]
    fn extra_root_field_initialize_is_rejected() {
        let response = r#"{"jsonrpc":"2.0","id":"init","result":{"kind":"initialize","protocol_version":"0.1.0","adapter_version":"1.0.0","capabilities":[],"auth_status":"ready","isolation_mode":"process","recovery_mode":"none"},"extra_root_field_initialize_accepted":true}"#;
        assert!(matches!(
            parse_initialize_response(response, "init"),
            Err(HandshakeError::ProtocolViolation { .. })
        ));
    }

    #[test]
    fn response_error_envelopes_enforce_custom_mapping_and_keep_standard_compatibility() {
        let standard_error = r#"{"jsonrpc":"2.0","id":"cancel","error":{"code":-32600,"message":"Invalid request"}}"#;
        assert!(matches!(
            parse_run_cancel_response(standard_error, "cancel"),
            Err(RunExecutionError::RpcError(ref error)) if error.code == -32600
        ));

        let mismatched_custom_error = r#"{"jsonrpc":"2.0","id":"cancel","error":{"code":-32010,"message":"bad","data":{"domain_code":"RUN_FENCED","retryable":false,"correlation_id":"01992d6d-2f31-7b11-8aa8-c975a632c007"}}}"#;
        assert!(matches!(
            parse_run_cancel_response(mismatched_custom_error, "cancel"),
            Err(RunExecutionError::ProtocolViolation { .. })
        ));

        let unrelated_integer_id =
            r#"{"jsonrpc":"2.0","id":0,"error":{"code":-32600,"message":"unrelated request"}}"#;
        assert!(matches!(
            parse_run_cancel_response(unrelated_integer_id, "cancel"),
            Err(RunExecutionError::ProtocolViolation { .. })
        ));

        let unrelated_string_id = r#"{"jsonrpc":"2.0","id":"other","error":{"code":-32600,"message":"unrelated request"}}"#;
        assert!(matches!(
            parse_run_cancel_response(unrelated_string_id, "cancel"),
            Err(RunExecutionError::ProtocolViolation { .. })
        ));

        let uncorrelated_standard_error =
            r#"{"jsonrpc":"2.0","id":null,"error":{"code":-32600,"message":"Invalid request"}}"#;
        assert!(matches!(
            parse_run_cancel_response(uncorrelated_standard_error, "cancel"),
            Err(RunExecutionError::ProtocolViolation { .. })
        ));

        let uncorrelated_custom_error = r#"{"jsonrpc":"2.0","id":null,"error":{"code":-32017,"message":"unavailable","data":{"domain_code":"ADAPTER_UNAVAILABLE","retryable":true,"correlation_id":"01992d6d-2f31-7b11-8aa8-c975a632c007"}}}"#;
        assert!(matches!(
            parse_run_cancel_response(uncorrelated_custom_error, "cancel"),
            Err(RunExecutionError::ProtocolViolation { .. })
        ));
    }

    #[test]
    fn run_notification_timestamps_require_valid_utc_rfc3339_milliseconds() {
        for (method, timestamp_field) in [
            ("run.heartbeat", "observed_at"),
            ("run.output", "observed_at"),
            ("run.completed", "completed_at"),
            ("run.failed", "failed_at"),
        ] {
            let mut validator = RunNotificationValidator::new(run_context());
            let invalid = notification(method, timestamp_field, "2026-02-30T01:02:03.004Z");
            assert!(matches!(
                validator.validate_line(&invalid),
                Err(RunExecutionError::ProtocolViolation { .. })
            ));

            let valid = notification(method, timestamp_field, VALID_TIMESTAMP);
            assert!(validator.validate_line(&valid).is_ok());
        }
    }

    #[test]
    fn heartbeat_progress_percent_must_be_between_zero_and_one_hundred() {
        for progress_percent in ["-0.1", "100.1"] {
            let mut validator = RunNotificationValidator::new(run_context());
            let heartbeat = notification("run.heartbeat", "observed_at", VALID_TIMESTAMP).replace(
                "\"phase\":\"running\"",
                &format!("\"phase\":\"running\",\"progress_percent\":{progress_percent}"),
            );
            assert!(matches!(
                validator.validate_line(&heartbeat),
                Err(RunExecutionError::ProtocolViolation { .. })
            ));
        }
    }

    #[test]
    fn run_notification_rejects_noncanonical_uuidv7_and_unknown_fields() {
        let invalid_uuid = notification("run.heartbeat", "observed_at", VALID_TIMESTAMP)
            .replace(TEAM_ID, "01992d6d-2f31-4b11-8aa8-c975a632c001");
        let mut validator = RunNotificationValidator::new(run_context());
        assert!(matches!(
            validator.validate_line(&invalid_uuid),
            Err(RunExecutionError::ProtocolViolation { .. })
        ));

        let unknown_field = notification("run.output", "observed_at", VALID_TIMESTAMP).replace(
            "\"content\":\"output\"",
            "\"content\":\"output\",\"extra\":true",
        );
        let mut validator = RunNotificationValidator::new(run_context());
        assert!(matches!(
            validator.validate_line(&unknown_field),
            Err(RunExecutionError::ProtocolViolation { .. })
        ));
    }

    #[test]
    fn run_output_enforces_utf8_budget_and_forbids_null_mismatch_payloads() {
        let content_at_limit = "é".repeat(32_768);
        let at_limit = notification("run.output", "observed_at", VALID_TIMESTAMP).replace(
            "\"content\":\"output\"",
            &format!("\"content\":\"{content_at_limit}\""),
        );
        let mut validator = RunNotificationValidator::new(run_context());
        assert!(validator.validate_line(&at_limit).is_ok());

        let content_over_limit = "é".repeat(32_769);
        let over_limit = notification("run.output", "observed_at", VALID_TIMESTAMP).replace(
            "\"content\":\"output\"",
            &format!("\"content\":\"{content_over_limit}\""),
        );
        let mut validator = RunNotificationValidator::new(run_context());
        assert!(matches!(
            validator.validate_line(&over_limit),
            Err(RunExecutionError::ProtocolViolation { .. })
        ));

        let stdout_artifact_null = notification("run.output", "observed_at", VALID_TIMESTAMP)
            .replace(
                "\"content\":\"output\"",
                "\"content\":\"output\",\"artifact\":null",
            );
        let artifact_content_null = notification("run.output", "observed_at", VALID_TIMESTAMP)
            .replace(
                "\"kind\":\"stdout\",\"content\":\"output\"",
                "\"kind\":\"artifact_candidate\",\"content\":null,\"artifact\":{\"kind\":\"artifact\",\"uri\":\"artifact://result\"}",
            );

        for invalid in [stdout_artifact_null, artifact_content_null] {
            let mut validator = RunNotificationValidator::new(run_context());
            assert!(matches!(
                validator.validate_line(&invalid),
                Err(RunExecutionError::ProtocolViolation { .. })
            ));
        }
    }

    #[test]
    fn outgoing_protocol_uuidv7_fields_are_validated_before_serialization() {
        let initialize = InitializeParams::new(
            "1.0.0",
            "01992d6d-2f31-4b11-8aa8-c975a632c006",
            "test",
            "en-US",
            "01992d6d-2f31-7b11-8aa8-c975a632c007",
        );
        assert!(matches!(
            super::validate_initialize_params(&initialize),
            Err(HandshakeError::ProtocolViolation { .. })
        ));

        let health = super::HealthCheckParams::new("01992d6d-2f31-7b11-8aa8-c975a632c007");
        assert!(super::validate_health_check_params(&health).is_ok());

        let mut invalid_context = run_context();
        invalid_context.run_id = "01992D6D-2F31-7B11-8AA8-C975A632C003".to_string();
        assert!(matches!(
            super::validate_run_context(&invalid_context),
            Err(RunExecutionError::ProtocolViolation { .. })
        ));
    }

    #[tokio::test]
    async fn initialize_timeout_terminates_and_reaps_the_direct_child() {
        let command = AdapterCommand::new(
            PathBuf::from("/bin/sleep"),
            vec![OsString::from("5")],
            env::current_dir().expect("test working directory is available"),
        );
        let config = SupervisorConfig::new(1_048_576, 1, 0).expect("valid supervisor config");
        let mut adapter = Supervisor::start(command, config).expect("sleeping adapter starts");

        let result = adapter
            .initialize(
                &InitializeParams::new(
                    "1.0.0",
                    "01992d6d-2f31-7b11-8aa8-c975a632c006",
                    "test",
                    "en-US",
                    "01992d6d-2f31-7b11-8aa8-c975a632c007",
                ),
                Duration::from_millis(20),
            )
            .await;
        assert!(matches!(result, Err(HandshakeError::TimedOut)));
        assert!(matches!(
            adapter.wait_for_exit(Duration::ZERO).await,
            Err(SupervisorError::AlreadyReaped)
        ));
    }

    #[tokio::test]
    async fn mock_exposes_negotiation_and_offline_health_diagnostics() {
        let workspace_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .expect("crates parent")
            .parent()
            .expect("repository root")
            .to_path_buf();
        let mock_script = workspace_root.join("adapters/mock/index.mjs");
        let node = env::var_os("PATH")
            .and_then(|paths| {
                env::split_paths(&paths)
                    .map(|directory| directory.join("node"))
                    .find(|candidate| candidate.is_file())
            })
            .expect("Node.js is required for Mock Adapter contract tests");
        let config = SupervisorConfig::new(1_048_576, 16, 4_096).expect("valid config");
        let initialize = || {
            InitializeParams::new(
                "1.0.0",
                "01992d6d-2f31-7b11-8aa8-c975a632c006",
                "macos-aarch64",
                "zh-CN",
                "01992d6d-2f31-7b11-8aa8-c975a632c007",
            )
        };

        let mut incompatible = Supervisor::start(
            AdapterCommand::new(
                node.clone(),
                vec![mock_script.clone().into_os_string()],
                workspace_root.clone(),
            )
            .with_environment("MOCK_FAULT_MODE", "incompatible_protocol"),
            config,
        )
        .expect("mock starts");
        let error = incompatible
            .initialize(&initialize(), Duration::from_secs(2))
            .await
            .expect_err("incompatible protocol must be rejected");
        assert!(matches!(error, HandshakeError::Incompatible { .. }));
        assert_eq!(error.jsonrpc_code(), -32010);
        assert_eq!(error.domain_code(), "ADAPTER_INCOMPATIBLE");
        drop(incompatible);

        let mut missing_capability = Supervisor::start(
            AdapterCommand::new(
                node.clone(),
                vec![mock_script.clone().into_os_string()],
                workspace_root.clone(),
            )
            .with_environment("MOCK_FAULT_MODE", "missing_capability"),
            config,
        )
        .expect("mock starts");
        let result = missing_capability
            .initialize(&initialize(), Duration::from_secs(2))
            .await
            .expect("capability negotiation succeeds");
        assert!(!result.has_capability("run.start"));
        assert!(result.has_capability("health.check"));
        drop(missing_capability);

        let mut unauthenticated = Supervisor::start(
            AdapterCommand::new(
                node.clone(),
                vec![mock_script.clone().into_os_string()],
                workspace_root.clone(),
            )
            .with_environment("MOCK_FAULT_MODE", "auth_missing"),
            config,
        )
        .expect("mock starts");
        let result = unauthenticated
            .initialize(&initialize(), Duration::from_secs(2))
            .await
            .expect("authentication negotiation succeeds");
        assert_eq!(result.auth_status, super::AuthStatus::Missing);
        drop(unauthenticated);

        let mut offline = Supervisor::start(
            AdapterCommand::new(node, vec![mock_script.into_os_string()], workspace_root)
                .with_environment("MOCK_FAULT_MODE", "health_unavailable"),
            config,
        )
        .expect("mock starts");
        let health = offline
            .health_check(
                &super::HealthCheckParams::new("01992d6d-2f31-7b11-8aa8-c975a632c008"),
                Duration::from_secs(2),
            )
            .await
            .expect("offline health status is a valid diagnostic response");
        assert_eq!(health.status, super::HealthStatus::Unavailable);
        assert_eq!(health.diagnostic_codes, ["ADAPTER_OFFLINE"]);
    }

    #[tokio::test]
    async fn run_cancel_validation_rejects_noncanonical_requested_at() {
        let params = RunCancelParams::new(
            run_context(),
            "cancel requested",
            "2026-09-28T01:02:03Z",
            "correlation",
        );

        assert!(matches!(
            super::validate_run_cancel_params(&params),
            Err(RunExecutionError::ProtocolViolation { .. })
        ));
    }
}
