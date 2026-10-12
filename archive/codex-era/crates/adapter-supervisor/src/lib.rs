#![forbid(unsafe_code)]

#[cfg(target_os = "macos")]
use std::os::unix::process::CommandExt;
#[cfg(target_os = "macos")]
use std::process::Command as SystemCommand;
use std::{
    ffi::OsString,
    fmt, io,
    path::PathBuf,
    process::{ExitStatus, Stdio},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};
use tokio::{
    io::{AsyncRead, AsyncReadExt, AsyncWriteExt},
    process::{Child, ChildStdin, Command},
    runtime::Handle,
    sync::{Mutex, Notify, mpsc},
    task::JoinHandle,
    time::{self, Instant},
};

pub mod json;
pub mod protocol;
pub mod semver;

pub use protocol::{
    AdapterRpcError, ArtifactReference, AuthStatus, CompletedRunResult, HandshakeError,
    HealthCheckError, HealthCheckParams, HealthResult, HealthStatus, InitializeParams,
    InitializeResult, IsolationMode, OutputKind, PROTOCOL_VERSION_V0, RecoveryMode,
    RunCancelParams, RunCancelResult, RunCompleted, RunContext, RunEventContext, RunExecutionError,
    RunFailed, RunHeartbeat, RunNotification, RunNotificationValidator, RunOutput, RunStartParams,
    RunStartResult, VerificationCommand,
};
pub use semver::{SemVer, SemVerError};

/// A previously approved Adapter entrypoint and its literal argument vector.
///
/// The supervisor deliberately accepts an argument vector instead of a command string. Manifest
/// validation and workspace authorization belong to their respective boundaries; callers must
/// pass an entrypoint and a working directory that those boundaries have already approved.
#[derive(Debug)]
pub struct AdapterCommand {
    executable: PathBuf,
    arguments: Vec<OsString>,
    working_directory: PathBuf,
    environment: Vec<(OsString, OsString)>,
}

impl AdapterCommand {
    /// Creates a command that will be launched directly, without invoking a shell.
    #[must_use]
    pub fn new(executable: PathBuf, arguments: Vec<OsString>, working_directory: PathBuf) -> Self {
        Self {
            executable,
            arguments,
            working_directory,
            environment: Vec::new(),
        }
    }

    /// Adds one already-approved environment value for the Adapter process.
    ///
    /// Values are never logged by this crate. Credential lookup and injection policy remain the
    /// responsibility of the security boundary.
    #[must_use]
    pub fn with_environment(
        mut self,
        key: impl Into<OsString>,
        value: impl Into<OsString>,
    ) -> Self {
        self.environment.push((key.into(), value.into()));
        self
    }
}

/// Runtime limits for one supervised Adapter process.
#[derive(Debug, Clone, Copy)]
pub struct SupervisorConfig {
    max_protocol_line_bytes: usize,
    stdout_channel_capacity: usize,
    stderr_capture_limit_bytes: usize,
}

impl SupervisorConfig {
    /// Creates explicit bounds for protocol framing, backpressure, and retained diagnostics.
    ///
    /// `max_protocol_line_bytes` cannot exceed the frozen Adapter V0 message limit (1 MiB). The
    /// retained stderr bound does not limit draining: excess stderr is discarded only after it
    /// has been drained from the child pipe, so a noisy child cannot block on a full stderr pipe.
    pub fn new(
        max_protocol_line_bytes: usize,
        stdout_channel_capacity: usize,
        stderr_capture_limit_bytes: usize,
    ) -> Result<Self, SupervisorError> {
        if max_protocol_line_bytes == 0 {
            return Err(SupervisorError::InvalidConfig(
                "max_protocol_line_bytes must be greater than zero",
            ));
        }
        if max_protocol_line_bytes > 1_048_576 {
            return Err(SupervisorError::InvalidConfig(
                "max_protocol_line_bytes cannot exceed the Adapter V0 1 MiB limit",
            ));
        }
        if stdout_channel_capacity == 0 {
            return Err(SupervisorError::InvalidConfig(
                "stdout_channel_capacity must be greater than zero",
            ));
        }

        Ok(Self {
            max_protocol_line_bytes,
            stdout_channel_capacity,
            stderr_capture_limit_bytes,
        })
    }
}

/// A bounded stderr snapshot collected while a child runs.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct CapturedStderr {
    /// The retained diagnostic prefix, capped by `SupervisorConfig`.
    pub bytes: Vec<u8>,
    /// Whether diagnostics exceeded the configured retained bound.
    pub truncated: bool,
}

/// How a supervisor-owned async pipe reader stopped.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReaderTermination {
    /// The pipe reached EOF normally.
    EndOfStream,
    /// The task was cancelled after the direct child exited while another process held the pipe.
    CancelledAfterChildExit,
    /// The operating system reported a pipe read failure.
    ReadFailed,
}

/// Why the supervised direct child stopped.
///
/// These are local process-lifecycle outcomes, not Adapter protocol or Run states.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Termination {
    /// The child exited before the requested wait deadline.
    Exited,
    /// The child exceeded a caller-provided request wait deadline and was terminated.
    TimedOut,
    /// Closing stdin (EOF) ended the child inside the supplied shutdown grace period.
    GracefulShutdown,
    /// The child ignored EOF for the supplied shutdown grace period and was terminated.
    ForcedShutdown,
}

/// The reaped direct child's exit status and bounded diagnostics.
#[derive(Debug)]
pub struct ExitReport {
    /// The OS exit status after the direct child has been reaped.
    pub status: ExitStatus,
    /// The local lifecycle outcome.
    pub termination: Termination,
    /// Bounded stderr diagnostics. Callers must redact before durable logging.
    pub stderr: CapturedStderr,
    /// Final state of the stdout reader task.
    pub stdout_reader: ReaderTermination,
    /// Final state of the stderr reader task.
    pub stderr_reader: ReaderTermination,
}

/// Cooperative cancellation for one bounded protocol write.
#[derive(Clone)]
pub struct CancellationToken {
    inner: Arc<CancellationState>,
}

struct CancellationState {
    cancelled: AtomicBool,
    notify: Notify,
}

impl CancellationToken {
    /// Creates a token that has not been cancelled.
    #[must_use]
    pub fn new() -> Self {
        Self {
            inner: Arc::new(CancellationState {
                cancelled: AtomicBool::new(false),
                notify: Notify::new(),
            }),
        }
    }

    /// Cancels all current and future waits using this token.
    pub fn cancel(&self) {
        self.inner.cancelled.store(true, Ordering::Release);
        self.inner.notify.notify_waiters();
    }

    async fn cancelled(&self) {
        loop {
            let notified = self.inner.notify.notified();
            if self.inner.cancelled.load(Ordering::Acquire) {
                return;
            }
            notified.await;
        }
    }
}

impl Default for CancellationToken {
    fn default() -> Self {
        Self::new()
    }
}

/// Raw events read from the Adapter stdout JSON Lines transport.
#[derive(Debug, PartialEq, Eq)]
pub enum StdoutEvent {
    /// One raw line, without its trailing newline, within the configured byte limit.
    Line(Vec<u8>),
    /// A raw line exceeded the configured byte limit and has been drained without being retained.
    LineTooLarge,
    /// The stdout pipe reached EOF.
    EndOfStream,
    /// No stdout event arrived before the caller's local wait deadline.
    TimedOut,
    /// Reading stdout failed before a complete line was available.
    ReadFailed,
}

/// Failures in the process boundary. These are infrastructure errors, not protocol envelopes.
#[derive(Debug)]
pub enum SupervisorError {
    /// The supplied local resource limits cannot safely supervise a process.
    InvalidConfig(&'static str),
    /// The process could not be created.
    Spawn(io::Error),
    /// An operation on the child process or one of its pipes failed.
    Process(io::Error),
    /// The caller attempted to write after stdin had been closed for shutdown.
    StdinClosed,
    /// The protocol write did not complete before its explicit deadline.
    WriteTimedOut,
    /// The protocol write was cancelled by its caller.
    WriteCancelled,
    /// The process was already reaped by an earlier lifecycle operation.
    AlreadyReaped,
    /// Async process supervision was started outside a Tokio runtime.
    RuntimeUnavailable,
    /// A supervisor-owned pipe reader terminated unexpectedly.
    ReaderPanicked,
    /// A caller attempted to send a non-line-delimited or oversized protocol message.
    InvalidProtocolLine,
}

impl fmt::Display for SupervisorError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidConfig(reason) => {
                write!(formatter, "invalid supervisor configuration: {reason}")
            }
            Self::Spawn(_) => formatter.write_str("could not start adapter process"),
            Self::Process(_) => formatter.write_str("adapter process operation failed"),
            Self::StdinClosed => formatter.write_str("adapter stdin is closed"),
            Self::WriteTimedOut => formatter.write_str("adapter protocol write timed out"),
            Self::WriteCancelled => formatter.write_str("adapter protocol write was cancelled"),
            Self::AlreadyReaped => formatter.write_str("adapter process has already been reaped"),
            Self::RuntimeUnavailable => {
                formatter.write_str("adapter supervisor requires an active Tokio runtime")
            }
            Self::ReaderPanicked => {
                formatter.write_str("adapter pipe reader terminated unexpectedly")
            }
            Self::InvalidProtocolLine => {
                formatter.write_str("protocol line is invalid or exceeds its limit")
            }
        }
    }
}

impl std::error::Error for SupervisorError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Spawn(error) | Self::Process(error) => Some(error),
            Self::InvalidConfig(_)
            | Self::StdinClosed
            | Self::WriteTimedOut
            | Self::WriteCancelled
            | Self::AlreadyReaped
            | Self::RuntimeUnavailable
            | Self::ReaderPanicked
            | Self::InvalidProtocolLine => None,
        }
    }
}

/// Starts and controls one direct Adapter subprocess.
pub struct Supervisor;

impl Supervisor {
    /// Starts an approved Adapter entrypoint with Tokio-managed stdio and no shell interpretation.
    pub fn start(
        command: AdapterCommand,
        config: SupervisorConfig,
    ) -> Result<SupervisedAdapter, SupervisorError> {
        let runtime = Handle::try_current().map_err(|_| SupervisorError::RuntimeUnavailable)?;
        let mut process = Command::new(command.executable);
        process
            .env_clear()
            .args(command.arguments)
            .current_dir(command.working_directory)
            .envs(command.environment)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);
        // Give the Adapter its own process group so ordinary descendants cannot outlive it.
        #[cfg(target_os = "macos")]
        process.as_std_mut().process_group(0);
        let mut child = process.spawn().map_err(SupervisorError::Spawn)?;
        #[cfg(target_os = "macos")]
        let process_group_id = child.id();

        let stdin = child.stdin.take().ok_or(SupervisorError::StdinClosed)?;
        let stdout = child.stdout.take().ok_or(SupervisorError::ReaderPanicked)?;
        let stderr = child.stderr.take().ok_or(SupervisorError::ReaderPanicked)?;
        let (stdout_sender, stdout_receiver) = mpsc::channel(config.stdout_channel_capacity);
        let stderr_capture = Arc::new(Mutex::new(CapturedStderr::default()));
        let stdout_reader = runtime.spawn(read_protocol_lines(
            stdout,
            config.max_protocol_line_bytes,
            stdout_sender,
        ));
        let stderr_reader = runtime.spawn(capture_stderr(
            stderr,
            config.stderr_capture_limit_bytes,
            Arc::clone(&stderr_capture),
        ));

        Ok(SupervisedAdapter {
            child: Some(child),
            stdin: Some(stdin),
            stdout_receiver: Some(stdout_receiver),
            stdout_reader: Some(stdout_reader),
            stderr_reader: Some(stderr_reader),
            stderr_capture,
            max_protocol_line_bytes: config.max_protocol_line_bytes,
            negotiated_initialize: None,
            forced_termination_started: false,
            #[cfg(target_os = "macos")]
            process_group_id,
        })
    }
}

/// A running direct Adapter child. Dropping it terminates and reaps that direct child.
pub struct SupervisedAdapter {
    child: Option<Child>,
    stdin: Option<ChildStdin>,
    stdout_receiver: Option<mpsc::Receiver<StdoutEvent>>,
    stdout_reader: Option<JoinHandle<ReaderTermination>>,
    stderr_reader: Option<JoinHandle<ReaderTermination>>,
    stderr_capture: Arc<Mutex<CapturedStderr>>,
    max_protocol_line_bytes: usize,
    negotiated_initialize: Option<protocol::InitializeResult>,
    forced_termination_started: bool,
    #[cfg(target_os = "macos")]
    process_group_id: Option<u32>,
}

impl SupervisedAdapter {
    /// Writes one raw JSON Lines message before an explicit deadline.
    ///
    /// This crate-internal transport primitive is reserved for the typed protocol methods. It is
    /// not part of the public supervisor API, so callers cannot bypass negotiation checks by
    /// hand-writing Run JSON-RPC messages.
    ///
    /// A timeout, cancellation, or pipe failure makes the partially written protocol stream
    /// unusable. The supervisor therefore closes stdin and starts terminating the direct child;
    /// callers must then use [`Self::wait_for_exit`] or [`Self::shutdown`] to collect its status.
    pub(crate) async fn send_protocol_line(
        &mut self,
        line: &[u8],
        deadline: Instant,
        cancellation: &CancellationToken,
    ) -> Result<(), SupervisorError> {
        if line.len() > self.max_protocol_line_bytes
            || line.contains(&b'\n')
            || line.contains(&b'\r')
        {
            return Err(SupervisorError::InvalidProtocolLine);
        }

        let result = {
            let stdin = self.stdin.as_mut().ok_or(SupervisorError::StdinClosed)?;
            let write = async {
                stdin.write_all(line).await?;
                stdin.write_all(b"\n").await?;
                stdin.flush().await
            };

            tokio::select! {
                biased;
                _ = cancellation.cancelled() => Err(SupervisorError::WriteCancelled),
                result = time::timeout_at(deadline, write) => match result {
                    Ok(Ok(())) => Ok(()),
                    Ok(Err(error)) => Err(SupervisorError::Process(error)),
                    Err(_) => Err(SupervisorError::WriteTimedOut),
                },
            }
        };

        if result.is_err() {
            self.stdin.take();
            self.terminate_process_group();
            if let Some(child) = self.child.as_mut() {
                let _ = child.start_kill();
            }
            self.forced_termination_started = true;
        }
        result
    }

    /// Waits for one raw stdout event for up to `timeout`.
    pub async fn next_stdout_event(
        &mut self,
        timeout: Duration,
    ) -> Result<StdoutEvent, SupervisorError> {
        let receiver = self
            .stdout_receiver
            .as_mut()
            .ok_or(SupervisorError::AlreadyReaped)?;
        match time::timeout(timeout, receiver.recv()).await {
            Ok(Some(event)) => Ok(event),
            Ok(None) => Ok(StdoutEvent::EndOfStream),
            Err(_) => Ok(StdoutEvent::TimedOut),
        }
    }

    /// Waits for a direct child to exit. A timeout forcibly terminates and reaps that child.
    pub async fn wait_for_exit(
        &mut self,
        timeout: Duration,
    ) -> Result<ExitReport, SupervisorError> {
        if let Some(status) = self.wait_until(timeout).await? {
            self.terminate_process_group();
            let termination = if self.forced_termination_started {
                Termination::ForcedShutdown
            } else {
                Termination::Exited
            };
            return self.finish(status, termination).await;
        }

        let status = self.terminate_and_reap().await?;
        let termination = if self.forced_termination_started {
            Termination::ForcedShutdown
        } else {
            Termination::TimedOut
        };
        self.finish(status, termination).await
    }

    /// Closes stdin to request EOF shutdown, then terminates and reaps the direct child if needed.
    pub async fn shutdown(
        &mut self,
        grace_period: Duration,
    ) -> Result<ExitReport, SupervisorError> {
        self.stdin.take();
        if let Some(status) = self.wait_until(grace_period).await? {
            self.terminate_process_group();
            let termination = if self.forced_termination_started {
                Termination::ForcedShutdown
            } else {
                Termination::GracefulShutdown
            };
            return self.finish(status, termination).await;
        }

        let status = self.terminate_and_reap().await?;
        self.finish(status, Termination::ForcedShutdown).await
    }

    async fn wait_until(
        &mut self,
        timeout: Duration,
    ) -> Result<Option<ExitStatus>, SupervisorError> {
        let child = self.child.as_mut().ok_or(SupervisorError::AlreadyReaped)?;
        match time::timeout(timeout, child.wait()).await {
            Ok(result) => result.map(Some).map_err(SupervisorError::Process),
            Err(_) => Ok(None),
        }
    }

    async fn terminate_and_reap(&mut self) -> Result<ExitStatus, SupervisorError> {
        self.terminate_process_group();
        let child = self.child.as_mut().ok_or(SupervisorError::AlreadyReaped)?;
        match child.start_kill() {
            Ok(()) => {}
            Err(error) if error.kind() == io::ErrorKind::InvalidInput => {}
            Err(error) => return Err(SupervisorError::Process(error)),
        }
        child.wait().await.map_err(SupervisorError::Process)
    }

    fn terminate_process_group(&mut self) {
        #[cfg(target_os = "macos")]
        if let Some(group_id) = self.process_group_id.take() {
            // /bin/kill sends the signal to a negative PGID. It is invoked directly,
            // with a literal argument vector and no inherited environment.
            let _ = SystemCommand::new("/bin/kill")
                .env_clear()
                .args(["-s", "KILL", "--", &format!("-{group_id}")])
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status();
        }
    }

    async fn finish(
        &mut self,
        status: ExitStatus,
        termination: Termination,
    ) -> Result<ExitReport, SupervisorError> {
        self.stdin.take();
        self.child.take();
        self.stdout_receiver.take();
        let stdout_reader = match self.stdout_reader.take() {
            Some(reader) => finish_reader(reader).await?,
            None => return Err(SupervisorError::AlreadyReaped),
        };
        let stderr_reader = match self.stderr_reader.take() {
            Some(reader) => finish_reader(reader).await?,
            None => return Err(SupervisorError::AlreadyReaped),
        };
        let stderr = self.stderr_capture.lock().await.clone();

        Ok(ExitReport {
            status,
            termination,
            stderr,
            stdout_reader,
            stderr_reader,
        })
    }
}

impl Drop for SupervisedAdapter {
    fn drop(&mut self) {
        self.stdin.take();
        self.terminate_process_group();
        if let Some(child) = self.child.as_mut() {
            let _ = child.start_kill();
        }
        self.child.take();
        self.stdout_receiver.take();
        if let Some(reader) = self.stdout_reader.take() {
            reader.abort();
        }
        if let Some(reader) = self.stderr_reader.take() {
            reader.abort();
        }
    }
}

const PIPE_DRAIN_GRACE: Duration = Duration::from_millis(100);

async fn finish_reader(
    mut reader: JoinHandle<ReaderTermination>,
) -> Result<ReaderTermination, SupervisorError> {
    match time::timeout(PIPE_DRAIN_GRACE, &mut reader).await {
        Ok(Ok(termination)) => Ok(termination),
        Ok(Err(_)) => Err(SupervisorError::ReaderPanicked),
        Err(_) => {
            reader.abort();
            match reader.await {
                Ok(termination) => Ok(termination),
                Err(error) if error.is_cancelled() => {
                    Ok(ReaderTermination::CancelledAfterChildExit)
                }
                Err(_) => Err(SupervisorError::ReaderPanicked),
            }
        }
    }
}

async fn read_protocol_lines(
    mut stdout: impl AsyncRead + Unpin,
    max_line_bytes: usize,
    sender: mpsc::Sender<StdoutEvent>,
) -> ReaderTermination {
    let mut buffer = [0_u8; 8_192];
    let mut line = Vec::new();
    let mut oversized = false;

    loop {
        let read = match stdout.read(&mut buffer).await {
            Ok(read) => read,
            Err(_) => {
                let _ = sender.send(StdoutEvent::ReadFailed).await;
                return ReaderTermination::ReadFailed;
            }
        };
        if read == 0 {
            if (!line.is_empty() || oversized)
                && !emit_protocol_line(&sender, &mut line, &mut oversized).await
            {
                return ReaderTermination::CancelledAfterChildExit;
            }
            return ReaderTermination::EndOfStream;
        }

        for byte in &buffer[..read] {
            if *byte == b'\n' {
                if !emit_protocol_line(&sender, &mut line, &mut oversized).await {
                    return ReaderTermination::CancelledAfterChildExit;
                }
                continue;
            }
            if line.len() < max_line_bytes {
                line.push(*byte);
            } else {
                oversized = true;
            }
        }
    }
}

async fn emit_protocol_line(
    sender: &mpsc::Sender<StdoutEvent>,
    line: &mut Vec<u8>,
    oversized: &mut bool,
) -> bool {
    let event = if *oversized {
        StdoutEvent::LineTooLarge
    } else {
        StdoutEvent::Line(std::mem::take(line))
    };
    *oversized = false;
    line.clear();
    sender.send(event).await.is_ok()
}

async fn capture_stderr(
    mut stderr: impl AsyncRead + Unpin,
    limit: usize,
    capture: Arc<Mutex<CapturedStderr>>,
) -> ReaderTermination {
    let mut buffer = [0_u8; 8_192];

    loop {
        let read = match stderr.read(&mut buffer).await {
            Ok(read) => read,
            Err(_) => return ReaderTermination::ReadFailed,
        };
        if read == 0 {
            return ReaderTermination::EndOfStream;
        }
        let mut captured = capture.lock().await;
        let remaining = limit.saturating_sub(captured.bytes.len());
        let retained = remaining.min(read);
        captured.bytes.extend_from_slice(&buffer[..retained]);
        if retained < read {
            captured.truncated = true;
        }
    }
}

#[cfg(test)]
mod tests {
    use std::{
        env,
        ffi::OsString,
        fs,
        future::Future,
        path::PathBuf,
        process::{Command, Stdio},
        thread,
        time::{Duration, Instant, SystemTime, UNIX_EPOCH},
    };

    use tokio::time;

    use super::{
        AdapterCommand, CancellationToken, HandshakeError, HealthCheckParams, InitializeParams,
        OutputKind, ReaderTermination, RecoveryMode, RunCancelParams, RunContext,
        RunExecutionError, RunNotificationValidator, RunStartParams, StdoutEvent, Supervisor,
        SupervisorConfig, SupervisorError, Termination,
    };

    fn config(stderr_capture_limit_bytes: usize) -> SupervisorConfig {
        SupervisorConfig::new(1_048_576, 16, stderr_capture_limit_bytes)
            .expect("test configuration is valid")
    }

    /// These four tests exercise `send_protocol_line`, which is crate-private by design so no caller
    /// can hand-write Run JSON-RPC messages. They therefore stay in this unit target instead of
    /// moving to the integration target, where `CARGO_BIN_EXE_fixture_adapter` is the only way to
    /// locate a real fixture Adapter.
    ///
    /// They still must not use the libtest harness binary as their Adapter: libtest writes
    /// `running 1 test` / `test ... ok` / `test result:` to real stdout, so a libtest child can never
    /// satisfy `PROTOCOL.md` §2. Each case instead supervises a plain POSIX child with a literal
    /// argument vector: `/bin/cat` for an echo/EOF child and `/bin/sleep` for a child that never
    /// reads stdin. Both are literal paths, so no PATH lookup and no shell is involved, and a
    /// missing child fails loudly instead of silently passing.
    fn posix_child(executable: &str, arguments: &[&str]) -> AdapterCommand {
        let path = PathBuf::from(executable);
        assert!(
            path.is_file(),
            "required test child {executable} is not present; these cases cannot run without it"
        );
        AdapterCommand::new(
            path,
            arguments.iter().map(OsString::from).collect(),
            env::current_dir().expect("test working directory is available"),
        )
    }

    /// An Adapter child that echoes stdin back and exits 0 at EOF.
    fn echo_child() -> AdapterCommand {
        posix_child("/bin/cat", &[])
    }

    /// An Adapter child that never reads stdin and outlives the test's own deadlines.
    fn non_reading_child() -> AdapterCommand {
        posix_child("/bin/sleep", &["30"])
    }

    #[test]
    fn protocol_line_limit_cannot_exceed_frozen_one_mebibyte_budget() {
        assert!(matches!(
            SupervisorConfig::new(1_048_577, 16, 256),
            Err(SupervisorError::InvalidConfig(_))
        ));
    }

    #[test]
    fn protocol_lines_round_trip_as_literal_json_lines() {
        run_async(async {
            let line = br#"{"jsonrpc":"2.0","id":1,"method":"health.check","params":{}}"#;
            let mut adapter = Supervisor::start(echo_child(), config(256)).expect("fixture starts");

            adapter
                .send_protocol_line(
                    line,
                    tokio::time::Instant::now() + Duration::from_secs(1),
                    &CancellationToken::new(),
                )
                .await
                .expect("literal protocol line is accepted");

            let echoed = receive_until(
                &mut adapter,
                |event| matches!(event, StdoutEvent::Line(value) if value == line),
            )
            .await;
            assert_eq!(echoed, StdoutEvent::Line(line.to_vec()));

            // The echo child reads stdin until EOF, so shutdown is what lets it exit cleanly.
            let report = adapter
                .shutdown(Duration::from_secs(1))
                .await
                .expect("echo fixture exits after EOF");
            assert_eq!(report.termination, Termination::GracefulShutdown);
            assert!(report.status.success());
        });
    }

    #[test]
    fn protocol_input_rejects_cr_lf_and_messages_over_the_frozen_limit() {
        run_async(async {
            let mut adapter = Supervisor::start(echo_child(), config(256)).expect("fixture starts");
            let cancellation = CancellationToken::new();
            let deadline = tokio::time::Instant::now() + Duration::from_secs(1);

            assert!(
                adapter
                    .send_protocol_line(b"{}\n", deadline, &cancellation)
                    .await
                    .is_err()
            );
            assert!(
                adapter
                    .send_protocol_line(b"{}\r", deadline, &cancellation)
                    .await
                    .is_err()
            );
            assert!(
                adapter
                    .send_protocol_line(&vec![b'x'; 1_048_577], deadline, &cancellation)
                    .await
                    .is_err()
            );

            assert!(
                adapter
                    .shutdown(Duration::from_secs(1))
                    .await
                    .expect("fixture exits after EOF")
                    .status
                    .success()
            );
        });
    }

    #[test]
    fn blocked_stdin_write_obeys_deadline_and_still_allows_reap() {
        run_test_with_watchdog(
            "tests::blocked_stdin_write_watchdog_case",
            "blocked_stdin_write",
            Duration::from_millis(1_200),
        );
    }

    #[test]
    fn blocked_stdin_write_watchdog_case() {
        if env::var("ADAPTER_SUPERVISOR_WATCHDOG_CASE").as_deref() != Ok("blocked_stdin_write") {
            return;
        }

        run_async(async {
            let mut adapter =
                Supervisor::start(non_reading_child(), config(256)).expect("fixture starts");
            let result = adapter
                .send_protocol_line(
                    &vec![b'x'; 1_048_576],
                    tokio::time::Instant::now() + Duration::from_millis(75),
                    &CancellationToken::new(),
                )
                .await;
            assert!(matches!(result, Err(SupervisorError::WriteTimedOut)));

            let report = adapter
                .shutdown(Duration::from_millis(100))
                .await
                .expect("shutdown reaps after a timed-out write");
            assert!(!report.status.success());
            assert_eq!(report.termination, Termination::ForcedShutdown);
        });
    }

    #[test]
    fn cancelled_stdin_write_stops_the_unsafe_protocol_stream() {
        run_async(async {
            let mut adapter =
                Supervisor::start(non_reading_child(), config(256)).expect("fixture starts");
            let cancellation = CancellationToken::new();
            cancellation.cancel();

            let result = adapter
                .send_protocol_line(
                    b"{}",
                    tokio::time::Instant::now() + Duration::from_secs(1),
                    &cancellation,
                )
                .await;
            assert!(matches!(result, Err(SupervisorError::WriteCancelled)));

            let report = adapter
                .shutdown(Duration::from_millis(100))
                .await
                .expect("shutdown reaps after a cancelled write");
            assert!(!report.status.success());
            assert_eq!(report.termination, Termination::ForcedShutdown);
        });
    }

    fn sample_run_context() -> RunContext {
        RunContext::new(
            "01992d6d-2f31-7b11-8aa8-c975a632c001",
            "01992d6d-2f31-7b11-8aa8-c975a632c002",
            "01992d6d-2f31-7b11-8aa8-c975a632c003",
            "01992d6d-2f31-7b11-8aa8-c975a632c004",
            "01992d6d-2f31-7b11-8aa8-c975a632c005",
            1,
            1,
        )
    }

    #[test]
    fn run_validator_rejects_skipped_sequence() {
        let mut validator = RunNotificationValidator::new(sample_run_context());

        // First message: sequence 1
        let seq1 = r#"{"jsonrpc":"2.0","method":"run.heartbeat","params":{"team_id":"01992d6d-2f31-7b11-8aa8-c975a632c001","task_id":"01992d6d-2f31-7b11-8aa8-c975a632c002","run_id":"01992d6d-2f31-7b11-8aa8-c975a632c003","role_id":"01992d6d-2f31-7b11-8aa8-c975a632c004","binding_id":"01992d6d-2f31-7b11-8aa8-c975a632c005","binding_version":1,"fencing_token":1,"sequence":1,"observed_at":"2026-09-24T02:30:00.000Z","phase":"executing"}}"#;
        let _ = validator.validate_line(seq1).expect("seq 1 is valid");

        // Second message: sequence 3 (skipping 2!)
        let seq3 = r#"{"jsonrpc":"2.0","method":"run.output","params":{"team_id":"01992d6d-2f31-7b11-8aa8-c975a632c001","task_id":"01992d6d-2f31-7b11-8aa8-c975a632c002","run_id":"01992d6d-2f31-7b11-8aa8-c975a632c003","role_id":"01992d6d-2f31-7b11-8aa8-c975a632c004","binding_id":"01992d6d-2f31-7b11-8aa8-c975a632c005","binding_version":1,"fencing_token":1,"sequence":3,"observed_at":"2026-09-24T02:30:01.000Z","kind":"stdout","content":"test"}}"#;
        match validator.validate_line(seq3) {
            Err(RunExecutionError::OutOfOrder {
                code,
                domain_code,
                expected_sequence,
                received_sequence,
                ..
            }) => {
                assert_eq!(code, -32015);
                assert_eq!(domain_code, "RUN_OUTPUT_OUT_OF_ORDER");
                assert_eq!(expected_sequence, 2);
                assert_eq!(received_sequence, 3);
            }
            other => panic!("expected OutOfOrder error, got: {other:?}"),
        }
    }

    #[test]
    fn run_validator_rejects_out_of_order_sequence() {
        let mut validator = RunNotificationValidator::new(sample_run_context());

        // First message receives sequence 2 directly
        let seq2 = r#"{"jsonrpc":"2.0","method":"run.output","params":{"team_id":"01992d6d-2f31-7b11-8aa8-c975a632c001","task_id":"01992d6d-2f31-7b11-8aa8-c975a632c002","run_id":"01992d6d-2f31-7b11-8aa8-c975a632c003","role_id":"01992d6d-2f31-7b11-8aa8-c975a632c004","binding_id":"01992d6d-2f31-7b11-8aa8-c975a632c005","binding_version":1,"fencing_token":1,"sequence":2,"observed_at":"2026-09-24T02:30:00.000Z","kind":"stdout","content":"test"}}"#;
        match validator.validate_line(seq2) {
            Err(RunExecutionError::OutOfOrder {
                code,
                domain_code,
                expected_sequence,
                received_sequence,
                ..
            }) => {
                assert_eq!(code, -32015);
                assert_eq!(domain_code, "RUN_OUTPUT_OUT_OF_ORDER");
                assert_eq!(expected_sequence, 1);
                assert_eq!(received_sequence, 2);
            }
            other => panic!("expected OutOfOrder error, got: {other:?}"),
        }
    }

    #[test]
    fn run_validator_rejects_null_content_in_stdout_output() {
        let mut validator = RunNotificationValidator::new(sample_run_context());

        let invalid = r#"{"jsonrpc":"2.0","method":"run.output","params":{"team_id":"01992d6d-2f31-7b11-8aa8-c975a632c001","task_id":"01992d6d-2f31-7b11-8aa8-c975a632c002","run_id":"01992d6d-2f31-7b11-8aa8-c975a632c003","role_id":"01992d6d-2f31-7b11-8aa8-c975a632c004","binding_id":"01992d6d-2f31-7b11-8aa8-c975a632c005","binding_version":1,"fencing_token":1,"sequence":1,"observed_at":"2026-09-24T02:30:00.000Z","kind":"stdout","content":null}}"#;
        match validator.validate_line(invalid) {
            Err(RunExecutionError::ProtocolViolation {
                code, domain_code, ..
            }) => {
                assert_eq!(code, -32099);
                assert_eq!(domain_code, "ADAPTER_PROTOCOL_VIOLATION");
            }
            other => panic!("expected ProtocolViolation error, got: {other:?}"),
        }
    }

    #[test]
    fn run_validator_rejects_empty_content_in_stdout_output() {
        let mut validator = RunNotificationValidator::new(sample_run_context());

        let invalid = r#"{"jsonrpc":"2.0","method":"run.output","params":{"team_id":"01992d6d-2f31-7b11-8aa8-c975a632c001","task_id":"01992d6d-2f31-7b11-8aa8-c975a632c002","run_id":"01992d6d-2f31-7b11-8aa8-c975a632c003","role_id":"01992d6d-2f31-7b11-8aa8-c975a632c004","binding_id":"01992d6d-2f31-7b11-8aa8-c975a632c005","binding_version":1,"fencing_token":1,"sequence":1,"observed_at":"2026-09-24T02:30:00.000Z","kind":"stdout","content":""}}"#;
        match validator.validate_line(invalid) {
            Err(RunExecutionError::ProtocolViolation {
                code, domain_code, ..
            }) => {
                assert_eq!(code, -32099);
                assert_eq!(domain_code, "ADAPTER_PROTOCOL_VIOLATION");
            }
            other => panic!("expected ProtocolViolation error, got: {other:?}"),
        }
    }

    #[test]
    fn run_validator_rejects_artifact_candidate_with_missing_artifact() {
        let mut validator = RunNotificationValidator::new(sample_run_context());

        let invalid = r#"{"jsonrpc":"2.0","method":"run.output","params":{"team_id":"01992d6d-2f31-7b11-8aa8-c975a632c001","task_id":"01992d6d-2f31-7b11-8aa8-c975a632c002","run_id":"01992d6d-2f31-7b11-8aa8-c975a632c003","role_id":"01992d6d-2f31-7b11-8aa8-c975a632c004","binding_id":"01992d6d-2f31-7b11-8aa8-c975a632c005","binding_version":1,"fencing_token":1,"sequence":1,"observed_at":"2026-09-24T02:30:00.000Z","kind":"artifact_candidate"}}"#;
        match validator.validate_line(invalid) {
            Err(RunExecutionError::ProtocolViolation {
                code, domain_code, ..
            }) => {
                assert_eq!(code, -32099);
                assert_eq!(domain_code, "ADAPTER_PROTOCOL_VIOLATION");
            }
            other => panic!("expected ProtocolViolation error, got: {other:?}"),
        }
    }

    #[test]
    fn run_validator_rejects_fencing_token_mismatch() {
        let mut validator = RunNotificationValidator::new(sample_run_context());

        // message with fencing_token = 2 while expected is 1
        let invalid = r#"{"jsonrpc":"2.0","method":"run.heartbeat","params":{"team_id":"01992d6d-2f31-7b11-8aa8-c975a632c001","task_id":"01992d6d-2f31-7b11-8aa8-c975a632c002","run_id":"01992d6d-2f31-7b11-8aa8-c975a632c003","role_id":"01992d6d-2f31-7b11-8aa8-c975a632c004","binding_id":"01992d6d-2f31-7b11-8aa8-c975a632c005","binding_version":1,"fencing_token":2,"sequence":1,"observed_at":"2026-09-24T02:30:00.000Z","phase":"executing"}}"#;
        match validator.validate_line(invalid) {
            Err(RunExecutionError::Fenced {
                code,
                domain_code,
                expected_token,
                received_token,
                ..
            }) => {
                assert_eq!(code, -32014);
                assert_eq!(domain_code, "RUN_FENCED");
                assert_eq!(expected_token, 1);
                assert_eq!(received_token, 2);
            }
            other => panic!("expected Fenced error, got: {other:?}"),
        }
    }

    #[test]
    fn run_validator_rejects_stdout_with_artifact() {
        let mut validator = RunNotificationValidator::new(sample_run_context());

        // stdout output cannot contain an artifact
        let invalid = r#"{"jsonrpc":"2.0","method":"run.output","params":{"team_id":"01992d6d-2f31-7b11-8aa8-c975a632c001","task_id":"01992d6d-2f31-7b11-8aa8-c975a632c002","run_id":"01992d6d-2f31-7b11-8aa8-c975a632c003","role_id":"01992d6d-2f31-7b11-8aa8-c975a632c004","binding_id":"01992d6d-2f31-7b11-8aa8-c975a632c005","binding_version":1,"fencing_token":1,"sequence":1,"observed_at":"2026-09-24T02:30:00.000Z","kind":"stdout","content":"valid text","artifact":{"kind":"artifact","uri":"artifact://test/a.txt"}}}"#;
        match validator.validate_line(invalid) {
            Err(RunExecutionError::ProtocolViolation {
                code, domain_code, ..
            }) => {
                assert_eq!(code, -32099);
                assert_eq!(domain_code, "ADAPTER_PROTOCOL_VIOLATION");
            }
            other => panic!("expected ProtocolViolation error, got: {other:?}"),
        }
    }

    #[test]
    fn run_validator_rejects_artifact_candidate_with_content() {
        let mut validator = RunNotificationValidator::new(sample_run_context());

        // artifact_candidate cannot contain content
        let invalid = r#"{"jsonrpc":"2.0","method":"run.output","params":{"team_id":"01992d6d-2f31-7b11-8aa8-c975a632c001","task_id":"01992d6d-2f31-7b11-8aa8-c975a632c002","run_id":"01992d6d-2f31-7b11-8aa8-c975a632c003","role_id":"01992d6d-2f31-7b11-8aa8-c975a632c004","binding_id":"01992d6d-2f31-7b11-8aa8-c975a632c005","binding_version":1,"fencing_token":1,"sequence":1,"observed_at":"2026-09-24T02:30:00.000Z","kind":"artifact_candidate","content":"forbidden text","artifact":{"kind":"artifact","uri":"artifact://test/a.txt"}}}"#;
        match validator.validate_line(invalid) {
            Err(RunExecutionError::ProtocolViolation {
                code, domain_code, ..
            }) => {
                assert_eq!(code, -32099);
                assert_eq!(domain_code, "ADAPTER_PROTOCOL_VIOLATION");
            }
            other => panic!("expected ProtocolViolation error, got: {other:?}"),
        }
    }

    #[test]
    fn run_validator_rejects_notification_after_completed() {
        let mut validator = RunNotificationValidator::new(sample_run_context());

        let completed = r#"{"jsonrpc":"2.0","method":"run.completed","params":{"team_id":"01992d6d-2f31-7b11-8aa8-c975a632c001","task_id":"01992d6d-2f31-7b11-8aa8-c975a632c002","run_id":"01992d6d-2f31-7b11-8aa8-c975a632c003","role_id":"01992d6d-2f31-7b11-8aa8-c975a632c004","binding_id":"01992d6d-2f31-7b11-8aa8-c975a632c005","binding_version":1,"fencing_token":1,"sequence":1,"completed_at":"2026-09-24T02:30:00.000Z","result_references":[]}}"#;
        assert!(validator.validate_line(completed).is_ok());
        assert!(validator.is_terminal());

        // Any subsequent notification must be rejected
        let late = r#"{"jsonrpc":"2.0","method":"run.heartbeat","params":{"team_id":"01992d6d-2f31-7b11-8aa8-c975a632c001","task_id":"01992d6d-2f31-7b11-8aa8-c975a632c002","run_id":"01992d6d-2f31-7b11-8aa8-c975a632c003","role_id":"01992d6d-2f31-7b11-8aa8-c975a632c004","binding_id":"01992d6d-2f31-7b11-8aa8-c975a632c005","binding_version":1,"fencing_token":1,"sequence":2,"observed_at":"2026-09-24T02:30:01.000Z","phase":"late"}}"#;
        match validator.validate_line(late) {
            Err(RunExecutionError::ProtocolViolation {
                code, domain_code, ..
            }) => {
                assert_eq!(code, -32099);
                assert_eq!(domain_code, "ADAPTER_PROTOCOL_VIOLATION");
            }
            other => panic!("expected ProtocolViolation error, got: {other:?}"),
        }
    }

    #[test]
    fn run_validator_accepts_identical_duplicate_notification_idempotently() {
        let mut validator = RunNotificationValidator::new(sample_run_context());

        let seq1 = r#"{"jsonrpc":"2.0","method":"run.heartbeat","params":{"team_id":"01992d6d-2f31-7b11-8aa8-c975a632c001","task_id":"01992d6d-2f31-7b11-8aa8-c975a632c002","run_id":"01992d6d-2f31-7b11-8aa8-c975a632c003","role_id":"01992d6d-2f31-7b11-8aa8-c975a632c004","binding_id":"01992d6d-2f31-7b11-8aa8-c975a632c005","binding_version":1,"fencing_token":1,"sequence":1,"observed_at":"2026-09-24T02:30:00.000Z","phase":"executing"}}"#;
        assert!(validator.validate_line(seq1).is_ok());
        assert_eq!(validator.last_sequence(), 1);
        assert!(!validator.last_was_duplicate());

        // Identical duplicate notification with same sequence and same payload
        assert!(validator.validate_line(seq1).is_ok());
        assert_eq!(validator.last_sequence(), 1);
        assert!(validator.last_was_duplicate());
        assert_eq!(validator.duplicate_count(), 1);
    }

    #[test]
    fn run_validator_rejects_duplicate_sequence_with_conflicting_payload() {
        let mut validator = RunNotificationValidator::new(sample_run_context());

        let seq1 = r#"{"jsonrpc":"2.0","method":"run.heartbeat","params":{"team_id":"01992d6d-2f31-7b11-8aa8-c975a632c001","task_id":"01992d6d-2f31-7b11-8aa8-c975a632c002","run_id":"01992d6d-2f31-7b11-8aa8-c975a632c003","role_id":"01992d6d-2f31-7b11-8aa8-c975a632c004","binding_id":"01992d6d-2f31-7b11-8aa8-c975a632c005","binding_version":1,"fencing_token":1,"sequence":1,"observed_at":"2026-09-24T02:30:00.000Z","phase":"executing"}}"#;
        assert!(validator.validate_line(seq1).is_ok());

        // Conflicting duplicate notification: same sequence 1, but different phase ("blocked" vs "executing")
        let conflicting = r#"{"jsonrpc":"2.0","method":"run.heartbeat","params":{"team_id":"01992d6d-2f31-7b11-8aa8-c975a632c001","task_id":"01992d6d-2f31-7b11-8aa8-c975a632c002","run_id":"01992d6d-2f31-7b11-8aa8-c975a632c003","role_id":"01992d6d-2f31-7b11-8aa8-c975a632c004","binding_id":"01992d6d-2f31-7b11-8aa8-c975a632c005","binding_version":1,"fencing_token":1,"sequence":1,"observed_at":"2026-09-24T02:30:00.000Z","phase":"blocked"}}"#;
        match validator.validate_line(conflicting) {
            Err(RunExecutionError::ProtocolViolation {
                code, domain_code, ..
            }) => {
                assert_eq!(code, -32099);
                assert_eq!(domain_code, "ADAPTER_PROTOCOL_VIOLATION");
            }
            other => panic!("expected ProtocolViolation error, got: {other:?}"),
        }
    }

    fn find_node_executable() -> Option<PathBuf> {
        if let Ok(path_var) = env::var("PATH") {
            for dir in env::split_paths(&path_var) {
                let candidate = dir.join("node");
                if candidate.is_file() {
                    return Some(candidate);
                }
            }
        }
        for candidate in [
            "/opt/homebrew/opt/node@24/bin/node",
            "/opt/homebrew/bin/node",
            "/usr/local/bin/node",
            "/usr/bin/node",
        ] {
            let path = PathBuf::from(candidate);
            if path.is_file() {
                return Some(path);
            }
        }
        None
    }

    #[test]
    fn node_mock_adapter_end_to_end_execution() {
        // Locate adapters/mock/index.mjs relative to the repository workspace
        let workspace_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .expect("crates parent")
            .parent()
            .expect("repo root")
            .to_path_buf();
        let mock_script = workspace_root
            .join("adapters")
            .join("mock")
            .join("index.mjs");

        if !mock_script.exists() {
            panic!(
                "Mock adapter script does not exist at: {}",
                mock_script.display()
            );
        }

        let node_bin = find_node_executable().unwrap_or_else(|| {
            panic!(
                "node executable not found in PATH or a known location; the end-to-end Mock case \
                 cannot be skipped, it must run"
            )
        });

        run_async(async {
            let command =
                AdapterCommand::new(node_bin, vec![mock_script.into_os_string()], workspace_root);
            let mut adapter = Supervisor::start(command, config(256))
                .expect("supervised mock adapter starts with node");

            // 1. Initialize
            let init_params = InitializeParams::new(
                "0.1.0",
                "01992d6d-2f31-7b11-8aa8-c975a632c002",
                "macos-aarch64",
                "zh-CN",
                "01992d6d-2f31-7b11-8aa8-c975a632c003",
            );
            let init_result = adapter
                .initialize(&init_params, Duration::from_secs(2))
                .await
                .expect("mock adapter initialize succeeds");
            assert_eq!(init_result.protocol_version.as_str(), "0.1.0");
            assert!(init_result.has_capability("run.start"));
            assert!(!init_result.has_capability("run.status"));
            assert_eq!(init_result.recovery_mode, RecoveryMode::None);

            // 2. Health check
            let health_params = HealthCheckParams::new("01992d6d-2f31-7b11-8aa8-c975a632c003");
            let health_result = adapter
                .health_check(&health_params, Duration::from_secs(2))
                .await
                .expect("mock adapter health check succeeds");
            assert!(health_result.is_healthy());

            // 3. Execute Run: run.start -> heartbeat -> output (stdout) -> output (artifact_candidate) -> completed
            let run_params = RunStartParams::new(
                sample_run_context(),
                "node-e2e-idempotency-key",
                "Node Mock Adapter End-to-End Test",
                "01992d6d-2f31-7b11-8aa8-c975a632c007",
            )
            .with_role("Mock Role", "Execute task and emit artifact")
            .with_workspace_roots("/tmp/shared", "/tmp/exec")
            .with_scopes(vec![".".to_string()], vec![".".to_string()])
            .with_timing(5000, 60000);

            let run_result = adapter
                .execute_run(&run_params, Duration::from_secs(2))
                .await
                .expect("mock adapter run execution completes successfully in strict order");

            // Verification of sequence progression:
            // run.start: accepted
            assert!(run_result.start_result.accepted);

            // sequence 1: heartbeat
            assert_eq!(run_result.heartbeats.len(), 1);
            assert_eq!(run_result.heartbeats[0].context.sequence, 1);
            assert_eq!(run_result.heartbeats[0].phase, "executing");

            // sequence 2: stdout
            assert_eq!(run_result.outputs.len(), 2);
            assert_eq!(run_result.outputs[0].context.sequence, 2);
            assert_eq!(run_result.outputs[0].kind, OutputKind::Stdout);
            assert!(run_result.outputs[0].content.is_some());
            assert!(run_result.outputs[0].artifact.is_none());

            // sequence 3: artifact_candidate
            assert_eq!(run_result.outputs[1].context.sequence, 3);
            assert_eq!(run_result.outputs[1].kind, OutputKind::ArtifactCandidate);
            assert!(run_result.outputs[1].content.is_none());
            let artifact = run_result.outputs[1]
                .artifact
                .as_ref()
                .expect("artifact candidate reference");
            assert_eq!(artifact.kind, "artifact");
            assert!(!artifact.uri.is_empty());

            // sequence 4: completed
            assert_eq!(run_result.completion.context.sequence, 4);
            assert_eq!(run_result.completion.result_references.len(), 1);
            assert_eq!(run_result.completion.result_references[0].kind, "artifact");

            // 4. Clean shutdown
            let exit_report = adapter
                .shutdown(Duration::from_millis(500))
                .await
                .expect("mock adapter shuts down gracefully");
            assert!(exit_report.status.success());
        });
    }

    #[test]
    fn node_mock_rejects_run_operations_before_trusted_negotiation_allows_them() {
        let (node_bin, mock_script, workspace_root) = node_and_mock_script();

        run_async(async {
            let initialize = || {
                InitializeParams::new(
                    "1.0.0",
                    "01992d6d-2f31-7b11-8aa8-c975a632c002",
                    "macos-aarch64",
                    "zh-CN",
                    "01992d6d-2f31-7b11-8aa8-c975a632c003",
                )
            };
            let run_start = || {
                RunStartParams::new(
                    sample_run_context(),
                    "negotiation-gate-key",
                    "Negotiation gate test",
                    "01992d6d-2f31-7b11-8aa8-c975a632c008",
                )
                .with_role("Mock Role", "Validate negotiation gate")
                .with_workspace_roots("/tmp/shared", "/tmp/exec")
                .with_scopes(vec![".".to_string()], vec![".".to_string()])
            };
            let run_cancel = || {
                RunCancelParams::new(
                    sample_run_context(),
                    "Negotiation gate test",
                    "2026-09-28T01:02:03.004Z",
                    "01992d6d-2f31-7b11-8aa8-c975a632c008",
                )
            };
            let health = || HealthCheckParams::new("01992d6d-2f31-7b11-8aa8-c975a632c009");

            let mut uninitialized = Supervisor::start(
                AdapterCommand::new(
                    node_bin.clone(),
                    vec![mock_script.clone().into_os_string()],
                    workspace_root.clone(),
                ),
                config(256),
            )
            .expect("Mock starts");
            let error = uninitialized
                .run_cancel(&run_cancel(), Duration::from_secs(2))
                .await
                .expect_err("uninitialized run.cancel must not be written to the Adapter");
            assert_eq!(error.jsonrpc_code(), -32099);
            assert_eq!(error.domain_code(), "ADAPTER_PROTOCOL_VIOLATION");
            assert!(
                uninitialized
                    .health_check(&health(), Duration::from_secs(2))
                    .await
                    .is_ok()
            );
            drop(uninitialized);

            let mut missing_capability = Supervisor::start(
                AdapterCommand::new(
                    node_bin.clone(),
                    vec![mock_script.clone().into_os_string()],
                    workspace_root.clone(),
                )
                .with_environment("MOCK_FAULT_MODE", "missing_capability"),
                config(256),
            )
            .expect("Mock starts");
            missing_capability
                .initialize(&initialize(), Duration::from_secs(2))
                .await
                .expect("negotiation succeeds");
            let error = missing_capability
                .run_start(&run_start(), Duration::from_secs(2))
                .await
                .expect_err("missing capability must not be written to the Adapter");
            assert_eq!(error.jsonrpc_code(), -32011);
            assert_eq!(error.domain_code(), "ADAPTER_CAPABILITY_MISSING");
            assert!(
                missing_capability
                    .health_check(&health(), Duration::from_secs(2))
                    .await
                    .is_ok()
            );
            drop(missing_capability);

            let mut unauthenticated = Supervisor::start(
                AdapterCommand::new(node_bin, vec![mock_script.into_os_string()], workspace_root)
                    .with_environment("MOCK_FAULT_MODE", "auth_missing"),
                config(256),
            )
            .expect("Mock starts");
            unauthenticated
                .initialize(&initialize(), Duration::from_secs(2))
                .await
                .expect("negotiation succeeds");
            let error = unauthenticated
                .run_cancel(&run_cancel(), Duration::from_secs(2))
                .await
                .expect_err("unauthenticated run.cancel must not be written to the Adapter");
            assert_eq!(error.jsonrpc_code(), -32012);
            assert_eq!(error.domain_code(), "ADAPTER_AUTHENTICATION_FAILED");
            assert!(
                unauthenticated
                    .health_check(&health(), Duration::from_secs(2))
                    .await
                    .is_ok()
            );
        });
    }

    /// `PROTOCOL.md` §2 requires stdout to carry only complete JSON-RPC lines. These tests inject
    /// the four noise classes the audit named — `running `, `test `, a blank line, and ordinary
    /// non-JSON text — at every response and notification stage, and assert that each is rejected
    /// with the frozen `ADAPTER_PROTOCOL_VIOLATION` instead of being skipped.
    ///
    /// `PROTOCOL.md` §2 (stdout is JSON Lines only) plus the `ERROR_CATALOG.md` §7 entry for
    /// `ADAPTER_PROTOCOL_VIOLATION` (-32099, not retryable).
    const STDOUT_NOISE_CLASSES: [&str; 4] = [
        "running non-protocol diagnostic",
        "test harness chatter",
        "",
        "plain non-json noise",
    ];

    fn noise_label(noise: &str) -> &'static str {
        if noise.is_empty() {
            "<blank line>"
        } else {
            "non-json line"
        }
    }

    fn noisy_mock_command(noise: &str, stage: &str) -> AdapterCommand {
        let (node_bin, mock_script, workspace_root) = node_and_mock_script();
        AdapterCommand::new(node_bin, vec![mock_script.into_os_string()], workspace_root)
            .with_environment("MOCK_STDOUT_NOISE", noise)
            .with_environment("MOCK_STDOUT_NOISE_STAGE", stage)
    }

    /// Asserts the frozen protocol-violation mapping without inventing a new code.
    fn assert_protocol_violation(code: i32, domain: &str) {
        assert_eq!(
            code, -32099,
            "expected ADAPTER_PROTOCOL_VIOLATION json-rpc code"
        );
        assert_eq!(domain, "ADAPTER_PROTOCOL_VIOLATION");
    }

    fn clean_initialize_params() -> InitializeParams {
        InitializeParams::new(
            "0.1.0",
            "01992d6d-2f31-7b11-8aa8-c975a632c002",
            "macos-aarch64",
            "zh-CN",
            "01992d6d-2f31-7b11-8aa8-c975a632c003",
        )
    }

    /// The legal control: with no noise configured, the same Mock still handshakes and runs.
    /// Without this, the rejection tests below could pass for the wrong reason.
    #[test]
    fn node_mock_without_noise_still_completes_the_handshake_and_a_run() {
        let (node_bin, mock_script, workspace_root) = node_and_mock_script();
        run_async(async {
            let command =
                AdapterCommand::new(node_bin, vec![mock_script.into_os_string()], workspace_root);
            let mut adapter = Supervisor::start(command, config(256)).expect("Mock starts");
            let init = adapter
                .initialize(&clean_initialize_params(), Duration::from_secs(2))
                .await
                .expect("legal JSON Lines must still complete the handshake");
            assert_eq!(init.protocol_version.as_str(), "0.1.0");
            let health = adapter
                .health_check(
                    &HealthCheckParams::new("01992d6d-2f31-7b11-8aa8-c975a632c009"),
                    Duration::from_secs(2),
                )
                .await
                .expect("legal JSON Lines must still answer health.check");
            assert!(health.is_healthy());
        });
    }

    /// Stage 1 of 5 — `initialize`. Noise precedes the valid response, so a reader that skipped
    /// non-JSON lines would complete the handshake (this is the audited bypass).
    #[test]
    fn stdout_noise_is_rejected_at_the_initialize_stage() {
        run_async(async {
            for noise in STDOUT_NOISE_CLASSES {
                let command = noisy_mock_command(noise, "initialize");
                let mut adapter =
                    Supervisor::start(command, config(256)).expect("noisy Mock starts");
                let error = adapter
                    .initialize(&clean_initialize_params(), Duration::from_secs(2))
                    .await
                    .expect_err("initialize must reject a polluted stdout line");
                assert_protocol_violation(error.jsonrpc_code(), error.domain_code());
                assert!(
                    adapter.negotiated_initialize().is_none(),
                    "a rejected handshake must not be recorded as negotiated"
                );
                eprintln!("initialize rejected {} noise", noise_label(noise));
            }
        });
    }

    /// Stage 2 of 5 — `health.check`. The handshake is clean; the noise arrives at the response.
    #[test]
    fn stdout_noise_is_rejected_at_the_health_check_stage() {
        run_async(async {
            for noise in STDOUT_NOISE_CLASSES {
                let command = noisy_mock_command(noise, "health.check");
                let mut adapter =
                    Supervisor::start(command, config(256)).expect("noisy Mock starts");
                adapter
                    .initialize(&clean_initialize_params(), Duration::from_secs(2))
                    .await
                    .expect("handshake must succeed when noise is staged at health.check");
                let error = adapter
                    .health_check(
                        &HealthCheckParams::new("01992d6d-2f31-7b11-8aa8-c975a632c009"),
                        Duration::from_secs(2),
                    )
                    .await
                    .expect_err("health.check must reject a polluted stdout line");
                assert_protocol_violation(error.jsonrpc_code(), error.domain_code());
                eprintln!("health.check rejected {} noise", noise_label(noise));
            }
        });
    }

    /// Stage 3 of 5 — `run.start`.
    #[test]
    fn stdout_noise_is_rejected_at_the_run_start_stage() {
        run_async(async {
            for noise in STDOUT_NOISE_CLASSES {
                let command = noisy_mock_command(noise, "run.start");
                let mut adapter =
                    Supervisor::start(command, config(256)).expect("noisy Mock starts");
                adapter
                    .initialize(&clean_initialize_params(), Duration::from_secs(2))
                    .await
                    .expect("handshake must succeed when noise is staged at run.start");
                let params = RunStartParams::new(
                    sample_run_context(),
                    "noise-stage-key",
                    "Stdout noise rejection",
                    "01992d6d-2f31-7b11-8aa8-c975a632c008",
                )
                .with_role("Mock Role", "Reject non-protocol stdout")
                .with_workspace_roots("/tmp/shared", "/tmp/exec")
                .with_scopes(vec![".".to_string()], vec![".".to_string()]);
                let error = adapter
                    .run_start(&params, Duration::from_secs(2))
                    .await
                    .expect_err("run.start must reject a polluted stdout line");
                assert_protocol_violation(error.jsonrpc_code(), error.domain_code());
                eprintln!("run.start rejected {} noise", noise_label(noise));
            }
        });
    }

    /// Stage 4 of 5 — `run.cancel`.
    #[test]
    fn stdout_noise_is_rejected_at_the_run_cancel_stage() {
        run_async(async {
            for noise in STDOUT_NOISE_CLASSES {
                let command = noisy_mock_command(noise, "run.cancel");
                let mut adapter =
                    Supervisor::start(command, config(256)).expect("noisy Mock starts");
                adapter
                    .initialize(&clean_initialize_params(), Duration::from_secs(2))
                    .await
                    .expect("handshake must succeed when noise is staged at run.cancel");
                let params = RunCancelParams::new(
                    sample_run_context(),
                    "Stdout noise rejection",
                    "2026-09-28T01:02:03.004Z",
                    "01992d6d-2f31-7b11-8aa8-c975a632c008",
                );
                let error = adapter
                    .run_cancel(&params, Duration::from_secs(2))
                    .await
                    .expect_err("run.cancel must reject a polluted stdout line");
                assert_protocol_violation(error.jsonrpc_code(), error.domain_code());
                eprintln!("run.cancel rejected {} noise", noise_label(noise));
            }
        });
    }

    /// Stage 5 of 5 — the run notification stream. The noise precedes sequence 1, so a reader
    /// that skipped it would report a successful run; the violation must surface instead.
    #[test]
    fn stdout_noise_is_rejected_at_the_run_notification_stage() {
        run_async(async {
            for noise in STDOUT_NOISE_CLASSES {
                let command = noisy_mock_command(noise, "run notification");
                let mut adapter =
                    Supervisor::start(command, config(256)).expect("noisy Mock starts");
                adapter
                    .initialize(&clean_initialize_params(), Duration::from_secs(2))
                    .await
                    .expect("handshake must succeed when noise is staged at notifications");
                let params = RunStartParams::new(
                    sample_run_context(),
                    "noise-stage-key",
                    "Stdout noise rejection",
                    "01992d6d-2f31-7b11-8aa8-c975a632c008",
                )
                .with_role("Mock Role", "Reject non-protocol stdout")
                .with_workspace_roots("/tmp/shared", "/tmp/exec")
                .with_scopes(vec![".".to_string()], vec![".".to_string()]);
                adapter
                    .run_start(&params, Duration::from_secs(2))
                    .await
                    .expect("run.start must succeed when noise is staged at notifications");
                let context = sample_run_context();
                let error = adapter
                    .collect_run_execution(&context, Duration::from_secs(2))
                    .await
                    .expect_err("a polluted notification stream must not be reported as a run");
                assert_protocol_violation(error.jsonrpc_code(), error.domain_code());
                eprintln!("notification stream rejected {} noise", noise_label(noise));
            }
        });
    }

    /// Resolves the Node executable and the Mock script, failing loudly when either is absent.
    ///
    /// These tests are the only executable proof that the production parser accepts and rejects the
    /// right bytes. A missing Node or Mock used to `return` early, which made the suite report green
    /// while asserting nothing at all, and it made mutation runs indistinguishable from broken
    /// apparatus. A missing dependency is now a hard failure: if these cases cannot run, they must
    /// say so.
    fn node_and_mock_script() -> (PathBuf, PathBuf, PathBuf) {
        let node_bin = find_node_executable().unwrap_or_else(|| {
            panic!(
                "node executable not found in PATH or a known location; the Mock Adapter cases \
                 cannot be skipped, they must run"
            )
        });
        let workspace_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .expect("crates parent")
            .parent()
            .expect("repo root")
            .to_path_buf();
        let mock_script = workspace_root
            .join("adapters")
            .join("mock")
            .join("index.mjs");
        assert!(
            mock_script.is_file(),
            "Mock Adapter script is missing at {}; these cases cannot be skipped, they must run",
            mock_script.display()
        );
        (node_bin, mock_script, workspace_root)
    }

    #[cfg(target_os = "macos")]
    fn read_detached_pid(path: &std::path::Path) -> Option<u32> {
        fs::read_to_string(path).ok()?.trim().parse().ok()
    }

    #[cfg(target_os = "macos")]
    fn process_exists(pid: u32) -> bool {
        Command::new("/bin/kill")
            .args(["-0", &pid.to_string()])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .map(|status| status.success())
            .unwrap_or(false)
    }

    #[cfg(target_os = "macos")]
    async fn wait_for_detached_pid(path: &std::path::Path) -> u32 {
        let deadline = Instant::now() + Duration::from_secs(2);
        loop {
            if let Some(pid) = read_detached_pid(path) {
                return pid;
            }
            assert!(
                Instant::now() < deadline,
                "detached Mock child did not publish its PID"
            );
            time::sleep(Duration::from_millis(10)).await;
        }
    }

    #[cfg(target_os = "macos")]
    async fn assert_detached_pid_exits(pid: u32) {
        let deadline = Instant::now() + Duration::from_secs(2);
        while process_exists(pid) && Instant::now() < deadline {
            time::sleep(Duration::from_millis(10)).await;
        }
        assert!(
            !process_exists(pid),
            "detached Mock child PID {pid} survived after its direct Adapter disappeared"
        );
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn node_mock_detached_child_exits_when_adapter_eof_forced_or_crashes() {
        let (node_bin, mock_script, workspace_root) = node_and_mock_script();

        run_async(async {
            for (mode, graceful) in [("eof", true), ("ignore_eof", false), ("crash", false)] {
                let pid_file = temp_marker_path();
                let command = AdapterCommand::new(
                    node_bin.clone(),
                    vec![mock_script.clone().into_os_string()],
                    workspace_root.clone(),
                )
                .with_environment("MOCK_DETACHED_LIFECYCLE_MODE", mode)
                .with_environment("MOCK_DETACHED_PID_FILE", pid_file.as_os_str());
                let mut adapter =
                    Supervisor::start(command, config(256)).expect("detached Mock adapter starts");

                let pid = wait_for_detached_pid(&pid_file).await;
                assert!(
                    process_exists(pid),
                    "detached Mock child is initially alive"
                );

                let report = if mode == "crash" {
                    adapter
                        .wait_for_exit(Duration::from_secs(1))
                        .await
                        .expect("crashed direct Adapter is reaped")
                } else {
                    adapter
                        .shutdown(Duration::from_millis(80))
                        .await
                        .expect("Adapter shutdown completes")
                };
                assert_eq!(
                    report.status.success(),
                    graceful,
                    "unexpected {mode} exit status"
                );
                assert_detached_pid_exits(pid).await;
                let _ = fs::remove_file(pid_file);
            }
        });
    }

    #[test]
    fn node_mock_adapter_normal_cancel() {
        let (node_bin, mock_script, workspace_root) = node_and_mock_script();

        run_async(async {
            let test_future = async {
                let command = AdapterCommand::new(
                    node_bin,
                    vec![mock_script.into_os_string()],
                    workspace_root,
                );
                let mut adapter = Supervisor::start(command, config(256))
                    .expect("supervised mock adapter starts with node");

                let init_params = InitializeParams::new(
                    "0.1.0",
                    "01992d6d-2f31-7b11-8aa8-c975a632c002",
                    "macos-aarch64",
                    "zh-CN",
                    "01992d6d-2f31-7b11-8aa8-c975a632c003",
                );
                adapter
                    .initialize(&init_params, Duration::from_secs(2))
                    .await
                    .expect("initialize succeeds");

                let cancel_params = RunCancelParams::new(
                    sample_run_context(),
                    "Normal cancellation",
                    "2026-09-24T02:30:00.000Z",
                    "01992d6d-2f31-7b11-8aa8-c975a632c008",
                );

                let cancel_res = adapter
                    .run_cancel(&cancel_params, Duration::from_secs(2))
                    .await
                    .expect("cancel succeeds");

                assert!(cancel_res.accepted);
                assert!(cancel_res.confirmed);

                drop(adapter);
            };

            time::timeout(Duration::from_secs(5), test_future)
                .await
                .expect("watchdog: normal cancel test exceeded bounded deadline");
        });
    }

    #[test]
    fn node_mock_adapter_fault_timeout_handshake() {
        let (node_bin, mock_script, workspace_root) = node_and_mock_script();

        run_async(async {
            let test_future = async {
                let command = AdapterCommand::new(
                    node_bin,
                    vec![mock_script.into_os_string()],
                    workspace_root,
                )
                .with_environment("MOCK_FAULT_MODE", "timeout");
                let mut adapter = Supervisor::start(command, config(256))
                    .expect("supervised mock adapter starts with node");

                let init_params = InitializeParams::new(
                    "0.1.0",
                    "01992d6d-2f31-7b11-8aa8-c975a632c002",
                    "macos-aarch64",
                    "zh-CN",
                    "01992d6d-2f31-7b11-8aa8-c975a632c003",
                );

                let err = adapter
                    .initialize(&init_params, Duration::from_millis(200))
                    .await
                    .expect_err("handshake should time out");

                assert!(matches!(err, HandshakeError::TimedOut));
                assert_eq!(err.jsonrpc_code(), -32017);
                assert_eq!(err.domain_code(), "ADAPTER_UNAVAILABLE");

                drop(adapter);
            };

            time::timeout(Duration::from_secs(5), test_future)
                .await
                .expect("watchdog: timeout_handshake test exceeded bounded deadline");
        });
    }

    #[test]
    fn node_mock_adapter_fault_timeout_run() {
        let (node_bin, mock_script, workspace_root) = node_and_mock_script();

        run_async(async {
            let test_future = async {
                let command = AdapterCommand::new(
                    node_bin,
                    vec![mock_script.into_os_string()],
                    workspace_root,
                )
                .with_environment("MOCK_FAULT_MODE", "timeout_run");
                let mut adapter = Supervisor::start(command, config(256))
                    .expect("supervised mock adapter starts with node");

                let init_params = InitializeParams::new(
                    "0.1.0",
                    "01992d6d-2f31-7b11-8aa8-c975a632c002",
                    "macos-aarch64",
                    "zh-CN",
                    "01992d6d-2f31-7b11-8aa8-c975a632c003",
                );
                adapter
                    .initialize(&init_params, Duration::from_secs(2))
                    .await
                    .expect("initialize succeeds");

                let run_params = RunStartParams::new(
                    sample_run_context(),
                    "timeout-key",
                    "Timeout Run Test",
                    "01992d6d-2f31-7b11-8aa8-c975a632c007",
                );

                let err = adapter
                    .run_start(&run_params, Duration::from_millis(200))
                    .await
                    .expect_err("run.start should time out");

                assert!(matches!(err, RunExecutionError::TimedOut));
                assert_eq!(err.jsonrpc_code(), -32017);
                assert_eq!(err.domain_code(), "ADAPTER_UNAVAILABLE");
                assert!(err.is_retryable());

                drop(adapter);
            };

            time::timeout(Duration::from_secs(5), test_future)
                .await
                .expect("watchdog: timeout_run test exceeded bounded deadline");
        });
    }

    #[test]
    fn node_mock_adapter_fault_timeout_stream() {
        let (node_bin, mock_script, workspace_root) = node_and_mock_script();

        run_async(async {
            let test_future = async {
                let command = AdapterCommand::new(
                    node_bin,
                    vec![mock_script.into_os_string()],
                    workspace_root,
                )
                .with_environment("MOCK_FAULT_MODE", "timeout_stream");
                let mut adapter = Supervisor::start(command, config(256))
                    .expect("supervised mock adapter starts with node");

                adapter
                    .initialize(
                        &InitializeParams::new(
                            "0.1.0",
                            "01992d6d-2f31-7b11-8aa8-c975a632c002",
                            "macos-aarch64",
                            "zh-CN",
                            "01992d6d-2f31-7b11-8aa8-c975a632c003",
                        ),
                        Duration::from_secs(2),
                    )
                    .await
                    .expect("initialize succeeds before the stream stalls");

                let run_params = RunStartParams::new(
                    sample_run_context(),
                    "timeout-stream-key",
                    "Timeout stream test",
                    "01992d6d-2f31-7b11-8aa8-c975a632c007",
                );
                let err = adapter
                    .execute_run(&run_params, Duration::from_millis(200))
                    .await
                    .expect_err("stalled post-start stream must reach execute_run deadline");

                assert!(matches!(err, RunExecutionError::TimedOut));
                assert_eq!(err.jsonrpc_code(), -32017);
                assert_eq!(err.domain_code(), "ADAPTER_UNAVAILABLE");
                assert!(err.is_retryable());

                let report = adapter
                    .shutdown(Duration::from_millis(100))
                    .await
                    .expect("stalled adapter is reaped after deadline");
                assert!(report.status.success());
            };

            time::timeout(Duration::from_secs(5), test_future)
                .await
                .expect("watchdog: timeout_stream test exceeded bounded deadline");
        });
    }

    #[test]
    fn node_mock_adapter_fault_duplicate() {
        let (node_bin, mock_script, workspace_root) = node_and_mock_script();

        run_async(async {
            let test_future = async {
                let command = AdapterCommand::new(
                    node_bin,
                    vec![mock_script.into_os_string()],
                    workspace_root,
                )
                .with_environment("MOCK_FAULT_MODE", "duplicate");
                let mut adapter = Supervisor::start(command, config(256))
                    .expect("supervised mock adapter starts with node");

                let init_params = InitializeParams::new(
                    "0.1.0",
                    "01992d6d-2f31-7b11-8aa8-c975a632c002",
                    "macos-aarch64",
                    "zh-CN",
                    "01992d6d-2f31-7b11-8aa8-c975a632c003",
                );
                adapter
                    .initialize(&init_params, Duration::from_secs(2))
                    .await
                    .expect("initialize succeeds");

                let run_params = RunStartParams::new(
                    sample_run_context(),
                    "dup-key",
                    "Duplicate Test",
                    "01992d6d-2f31-7b11-8aa8-c975a632c007",
                );

                let completed = adapter
                    .execute_run(&run_params, Duration::from_secs(2))
                    .await
                    .expect("run completes with duplicate sequence idempotently handled");

                // Sequence 1 was duplicated, but supervisor idempotently ignored it
                assert_eq!(completed.heartbeats.len(), 1);
                assert_eq!(completed.outputs.len(), 2);
                assert_eq!(completed.completion.context.sequence, 4);

                let exit_report = adapter
                    .shutdown(Duration::from_millis(500))
                    .await
                    .expect("clean shutdown");
                assert!(exit_report.status.success());
            };

            time::timeout(Duration::from_secs(5), test_future)
                .await
                .expect("watchdog: duplicate test exceeded bounded deadline");
        });
    }

    #[test]
    fn node_mock_adapter_fault_duplicate_conflict() {
        let (node_bin, mock_script, workspace_root) = node_and_mock_script();

        run_async(async {
            let test_future = async {
                let command = AdapterCommand::new(
                    node_bin,
                    vec![mock_script.into_os_string()],
                    workspace_root,
                )
                .with_environment("MOCK_FAULT_MODE", "duplicate_conflict");
                let mut adapter = Supervisor::start(command, config(256))
                    .expect("supervised mock adapter starts with node");

                adapter
                    .initialize(
                        &InitializeParams::new(
                            "0.1.0",
                            "01992d6d-2f31-7b11-8aa8-c975a632c002",
                            "macos-aarch64",
                            "zh-CN",
                            "01992d6d-2f31-7b11-8aa8-c975a632c003",
                        ),
                        Duration::from_secs(2),
                    )
                    .await
                    .expect("initialize succeeds");

                let run_params = RunStartParams::new(
                    sample_run_context(),
                    "duplicate-conflict-key",
                    "Duplicate conflict test",
                    "01992d6d-2f31-7b11-8aa8-c975a632c007",
                );
                let err = adapter
                    .execute_run(&run_params, Duration::from_secs(2))
                    .await
                    .expect_err("conflicting duplicate must be rejected");

                assert_eq!(err.jsonrpc_code(), -32099);
                assert_eq!(err.domain_code(), "ADAPTER_PROTOCOL_VIOLATION");
                assert!(!err.is_retryable());

                let _ = adapter.shutdown(Duration::from_millis(100)).await;
            };

            time::timeout(Duration::from_secs(5), test_future)
                .await
                .expect("watchdog: duplicate_conflict test exceeded bounded deadline");
        });
    }

    #[test]
    fn node_mock_adapter_fault_out_of_order() {
        let (node_bin, mock_script, workspace_root) = node_and_mock_script();

        run_async(async {
            let test_future = async {
                let command = AdapterCommand::new(
                    node_bin,
                    vec![mock_script.into_os_string()],
                    workspace_root,
                )
                .with_environment("MOCK_FAULT_MODE", "out_of_order");
                let mut adapter = Supervisor::start(command, config(256))
                    .expect("supervised mock adapter starts with node");

                let init_params = InitializeParams::new(
                    "0.1.0",
                    "01992d6d-2f31-7b11-8aa8-c975a632c002",
                    "macos-aarch64",
                    "zh-CN",
                    "01992d6d-2f31-7b11-8aa8-c975a632c003",
                );
                adapter
                    .initialize(&init_params, Duration::from_secs(2))
                    .await
                    .expect("initialize succeeds");

                let run_params = RunStartParams::new(
                    sample_run_context(),
                    "out-of-order-key",
                    "Out of order Test",
                    "01992d6d-2f31-7b11-8aa8-c975a632c007",
                );

                let err = adapter
                    .execute_run(&run_params, Duration::from_secs(2))
                    .await
                    .expect_err("should fail with out of order error");

                match err {
                    RunExecutionError::OutOfOrder {
                        code,
                        domain_code,
                        expected_sequence,
                        received_sequence,
                        ..
                    } => {
                        assert_eq!(code, -32015);
                        assert_eq!(domain_code, "RUN_OUTPUT_OUT_OF_ORDER");
                        assert_eq!(expected_sequence, 1);
                        assert_eq!(received_sequence, 2);
                    }
                    other => panic!("expected OutOfOrder, got: {other:?}"),
                }

                drop(adapter);
            };

            time::timeout(Duration::from_secs(5), test_future)
                .await
                .expect("watchdog: out_of_order test exceeded bounded deadline");
        });
    }

    #[test]
    fn node_mock_adapter_fault_crash() {
        let (node_bin, mock_script, workspace_root) = node_and_mock_script();

        run_async(async {
            let test_future = async {
                let command = AdapterCommand::new(
                    node_bin,
                    vec![mock_script.into_os_string()],
                    workspace_root,
                )
                .with_environment("MOCK_FAULT_MODE", "crash");
                let mut adapter = Supervisor::start(command, config(256))
                    .expect("supervised mock adapter starts with node");

                let init_params = InitializeParams::new(
                    "0.1.0",
                    "01992d6d-2f31-7b11-8aa8-c975a632c002",
                    "macos-aarch64",
                    "zh-CN",
                    "01992d6d-2f31-7b11-8aa8-c975a632c003",
                );
                adapter
                    .initialize(&init_params, Duration::from_secs(2))
                    .await
                    .expect("initialize succeeds");

                let run_params = RunStartParams::new(
                    sample_run_context(),
                    "crash-key",
                    "Crash Test",
                    "01992d6d-2f31-7b11-8aa8-c975a632c007",
                );

                let err = adapter
                    .execute_run(&run_params, Duration::from_secs(2))
                    .await
                    .expect_err("should fail with crash/process exit");

                assert!(err.is_retryable());
                match err {
                    RunExecutionError::ProcessExited {
                        code,
                        domain_code,
                        exit_status,
                        ..
                    } => {
                        assert_eq!(code, -32017);
                        assert_eq!(domain_code, "ADAPTER_UNAVAILABLE");
                        // Pipe EOF can precede the OS exit notification. Check any available
                        // snapshot, then reap below before asserting the final status/diagnostics.
                        if let Some(status) = exit_status {
                            assert_eq!(status.code(), Some(42));
                        }
                    }
                    other => panic!("expected ProcessExited, got: {other:?}"),
                }

                let report = adapter
                    .wait_for_exit(Duration::from_secs(1))
                    .await
                    .expect("crashed mock adapter is reaped within the existing deadline");
                assert_eq!(report.termination, Termination::Exited);
                assert_eq!(report.status.code(), Some(42));
                assert!(String::from_utf8_lossy(&report.stderr.bytes).contains("crash"));
                assert_eq!(report.stdout_reader, ReaderTermination::EndOfStream);
                assert_eq!(report.stderr_reader, ReaderTermination::EndOfStream);

                drop(adapter);
            };

            time::timeout(Duration::from_secs(5), test_future)
                .await
                .expect("watchdog: crash test exceeded bounded deadline");
        });
    }

    #[test]
    fn node_mock_adapter_fault_late_callback() {
        let (node_bin, mock_script, workspace_root) = node_and_mock_script();

        run_async(async {
            let test_future = async {
                let command = AdapterCommand::new(
                    node_bin,
                    vec![mock_script.into_os_string()],
                    workspace_root,
                )
                .with_environment("MOCK_FAULT_MODE", "late_callback");
                let mut adapter = Supervisor::start(command, config(256))
                    .expect("supervised mock adapter starts with node");

                let init_params = InitializeParams::new(
                    "0.1.0",
                    "01992d6d-2f31-7b11-8aa8-c975a632c002",
                    "macos-aarch64",
                    "zh-CN",
                    "01992d6d-2f31-7b11-8aa8-c975a632c003",
                );
                adapter
                    .initialize(&init_params, Duration::from_secs(2))
                    .await
                    .expect("initialize succeeds");

                let run_params = RunStartParams::new(
                    sample_run_context(),
                    "late-key",
                    "Late Callback Test",
                    "01992d6d-2f31-7b11-8aa8-c975a632c007",
                );

                let _start_res = adapter
                    .run_start(&run_params, Duration::from_secs(2))
                    .await
                    .expect("start succeeds");

                let mut validator = RunNotificationValidator::new(sample_run_context());

                // Read sequence 1 (heartbeat)
                let n1 = adapter
                    .read_run_notification(&mut validator, Duration::from_secs(2))
                    .await
                    .expect("n1");
                assert_eq!(n1.sequence(), 1);

                // Read sequence 2 (stdout)
                let n2 = adapter
                    .read_run_notification(&mut validator, Duration::from_secs(2))
                    .await
                    .expect("n2");
                assert_eq!(n2.sequence(), 2);

                // Read sequence 3 (artifact)
                let n3 = adapter
                    .read_run_notification(&mut validator, Duration::from_secs(2))
                    .await
                    .expect("n3");
                assert_eq!(n3.sequence(), 3);

                // Read sequence 4 (completed)
                let n4 = adapter
                    .read_run_notification(&mut validator, Duration::from_secs(2))
                    .await
                    .expect("n4");
                assert_eq!(n4.sequence(), 4);
                assert!(validator.is_terminal());

                // Read late callback sequence 5 (output after completed)
                let late_err = adapter
                    .read_run_notification(&mut validator, Duration::from_secs(2))
                    .await
                    .expect_err(
                        "late callback after completion must be rejected as protocol violation",
                    );

                match late_err {
                    RunExecutionError::ProtocolViolation {
                        code,
                        domain_code,
                        message,
                    } => {
                        assert_eq!(code, -32099);
                        assert_eq!(domain_code, "ADAPTER_PROTOCOL_VIOLATION");
                        assert!(message.contains("terminal"));
                    }
                    other => panic!("expected ProtocolViolation, got: {other:?}"),
                }

                drop(adapter);
            };

            time::timeout(Duration::from_secs(5), test_future)
                .await
                .expect("watchdog: late_callback test exceeded bounded deadline");
        });
    }

    #[test]
    fn node_mock_adapter_fault_reject_cancel() {
        let (node_bin, mock_script, workspace_root) = node_and_mock_script();

        run_async(async {
            let test_future = async {
                let command = AdapterCommand::new(
                    node_bin,
                    vec![mock_script.into_os_string()],
                    workspace_root,
                )
                .with_environment("MOCK_FAULT_MODE", "reject_cancel");
                let mut adapter = Supervisor::start(command, config(256))
                    .expect("supervised mock adapter starts with node");

                let init_params = InitializeParams::new(
                    "0.1.0",
                    "01992d6d-2f31-7b11-8aa8-c975a632c002",
                    "macos-aarch64",
                    "zh-CN",
                    "01992d6d-2f31-7b11-8aa8-c975a632c003",
                );
                adapter
                    .initialize(&init_params, Duration::from_secs(2))
                    .await
                    .expect("initialize succeeds");

                let cancel_params = RunCancelParams::new(
                    sample_run_context(),
                    "User requested cancellation",
                    "2026-09-24T02:30:00.000Z",
                    "01992d6d-2f31-7b11-8aa8-c975a632c008",
                );

                let cancel_res = adapter
                    .run_cancel(&cancel_params, Duration::from_secs(2))
                    .await
                    .expect("cancel response received");

                assert!(!cancel_res.accepted);
                assert!(!cancel_res.confirmed);

                drop(adapter);
            };

            time::timeout(Duration::from_secs(5), test_future)
                .await
                .expect("watchdog: reject_cancel test exceeded bounded deadline");
        });
    }

    #[test]
    fn node_mock_adapter_fault_reject_cancel_timeout() {
        let (node_bin, mock_script, workspace_root) = node_and_mock_script();

        run_async(async {
            let test_future = async {
                let command = AdapterCommand::new(
                    node_bin,
                    vec![mock_script.into_os_string()],
                    workspace_root,
                )
                .with_environment("MOCK_FAULT_MODE", "reject_cancel_timeout");
                let mut adapter = Supervisor::start(command, config(256))
                    .expect("supervised mock adapter starts with node");
                #[cfg(target_os = "macos")]
                let direct_pid = adapter
                    .child
                    .as_ref()
                    .and_then(|child| child.id())
                    .expect("supervisor exposes a direct Node child PID before cancellation");

                let init_params = InitializeParams::new(
                    "0.1.0",
                    "01992d6d-2f31-7b11-8aa8-c975a632c002",
                    "macos-aarch64",
                    "zh-CN",
                    "01992d6d-2f31-7b11-8aa8-c975a632c003",
                );
                adapter
                    .initialize(&init_params, Duration::from_secs(2))
                    .await
                    .expect("initialize succeeds");

                let cancel_params = RunCancelParams::new(
                    sample_run_context(),
                    "User requested cancellation",
                    "2026-09-24T02:30:00.000Z",
                    "01992d6d-2f31-7b11-8aa8-c975a632c008",
                );

                let err = adapter
                    .run_cancel(&cancel_params, Duration::from_millis(200))
                    .await
                    .expect_err("cancel should time out");

                assert!(matches!(err, RunExecutionError::TimedOut));
                assert_eq!(err.jsonrpc_code(), -32017);
                assert_eq!(err.domain_code(), "ADAPTER_UNAVAILABLE");

                let report = adapter
                    .shutdown(Duration::from_millis(100))
                    .await
                    .expect("cancel-timeout adapter is reaped during cleanup");
                assert!(report.status.success());
                #[cfg(target_os = "macos")]
                assert!(
                    !process_exists(direct_pid),
                    "direct Node child PID {direct_pid} remains after cancel-timeout cleanup"
                );
            };

            time::timeout(Duration::from_secs(5), test_future)
                .await
                .expect("watchdog: reject_cancel_timeout test exceeded bounded deadline");
        });
    }

    #[test]
    fn node_mock_adapter_fault_unresponsive() {
        let (node_bin, mock_script, workspace_root) = node_and_mock_script();

        run_async(async {
            let test_future = async {
                let command = AdapterCommand::new(
                    node_bin,
                    vec![mock_script.into_os_string()],
                    workspace_root,
                )
                .with_environment("MOCK_FAULT_MODE", "unresponsive");
                let mut adapter = Supervisor::start(command, config(256))
                    .expect("supervised mock adapter starts with node");

                let init_params = InitializeParams::new(
                    "0.1.0",
                    "01992d6d-2f31-7b11-8aa8-c975a632c002",
                    "macos-aarch64",
                    "zh-CN",
                    "01992d6d-2f31-7b11-8aa8-c975a632c003",
                );

                let err = adapter
                    .initialize(&init_params, Duration::from_millis(500))
                    .await
                    .expect_err("unresponsive adapter should fail initialization");

                assert_eq!(err.jsonrpc_code(), -32017);
                assert_eq!(err.domain_code(), "ADAPTER_UNAVAILABLE");

                drop(adapter);
            };

            time::timeout(Duration::from_secs(5), test_future)
                .await
                .expect("watchdog: unresponsive test exceeded bounded deadline");
        });
    }

    #[test]
    fn node_mock_adapter_fault_unresponsive_during_run() {
        let (node_bin, mock_script, workspace_root) = node_and_mock_script();

        run_async(async {
            let test_future = async {
                let command = AdapterCommand::new(
                    node_bin,
                    vec![
                        mock_script.into_os_string(),
                        OsString::from("--fault-mode"),
                        OsString::from("unresponsive"),
                        OsString::from("--fault-stage"),
                        OsString::from("run"),
                    ],
                    workspace_root,
                );
                let mut adapter = Supervisor::start(command, config(256))
                    .expect("supervised mock adapter starts with node");

                adapter
                    .initialize(
                        &InitializeParams::new(
                            "0.1.0",
                            "01992d6d-2f31-7b11-8aa8-c975a632c002",
                            "macos-aarch64",
                            "zh-CN",
                            "01992d6d-2f31-7b11-8aa8-c975a632c003",
                        ),
                        Duration::from_secs(2),
                    )
                    .await
                    .expect("initialize succeeds before run-stage disconnect");

                let run_params = RunStartParams::new(
                    sample_run_context(),
                    "unresponsive-run-key",
                    "Unresponsive run-stage test",
                    "01992d6d-2f31-7b11-8aa8-c975a632c007",
                );
                let err = adapter
                    .run_start(&run_params, Duration::from_secs(2))
                    .await
                    .expect_err("run-stage disconnect must fail the run start");

                assert!(matches!(err, RunExecutionError::ProcessExited { .. }));
                assert_eq!(err.jsonrpc_code(), -32017);
                assert_eq!(err.domain_code(), "ADAPTER_UNAVAILABLE");
                assert!(err.is_retryable());

                let _ = adapter.shutdown(Duration::from_millis(100)).await;
            };

            time::timeout(Duration::from_secs(5), test_future)
                .await
                .expect("watchdog: unresponsive run-stage test exceeded bounded deadline");
        });
    }

    fn temp_marker_path() -> PathBuf {
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time is after UNIX_EPOCH")
            .as_nanos();
        env::temp_dir().join(format!("adapter-supervisor-{timestamp}.marker"))
    }

    async fn receive_until(
        adapter: &mut super::SupervisedAdapter,
        predicate: impl Fn(&StdoutEvent) -> bool,
    ) -> StdoutEvent {
        for _ in 0..16 {
            let event = adapter
                .next_stdout_event(Duration::from_secs(1))
                .await
                .expect("stdout reader is available");
            if predicate(&event) {
                return event;
            }
        }
        panic!("expected stdout event was not received");
    }

    fn run_async(future: impl Future<Output = ()>) {
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("test runtime starts")
            .block_on(future);
    }

    fn run_test_with_watchdog(test_name: &str, case: &str, timeout: Duration) {
        let executable = env::current_exe().expect("test binary path is available");
        let mut child = Command::new(executable)
            .env_clear()
            .args(["--exact", test_name, "--nocapture"])
            .env("ADAPTER_SUPERVISOR_WATCHDOG_CASE", case)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("watchdog test process starts");
        let deadline = Instant::now() + timeout;

        loop {
            if let Some(status) = child.try_wait().expect("watchdog polls test process") {
                assert!(status.success(), "watchdog test process failed: {status}");
                return;
            }
            if Instant::now() >= deadline {
                child
                    .kill()
                    .expect("watchdog terminates stuck test process");
                child.wait().expect("watchdog reaps stuck test process");
                panic!("watchdog detected an unbounded supervisor operation");
            }
            thread::sleep(Duration::from_millis(5));
        }
    }
}
