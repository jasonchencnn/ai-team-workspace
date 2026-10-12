//! Test-only Adapter fixture driven as a real subprocess.
//!
//! `PROTOCOL.md` §2 requires every Adapter stdout line to be a complete JSON-RPC 2.0 object and
//! nothing else. The previous fixture reused the libtest harness binary as its own Adapter, so
//! libtest's own `running 1 test` / `test ... ok` / `test result:` progress lines landed in real
//! stdout and could never be removed (no stable libtest `--format` suppresses them). This binary
//! replaces that: it is a normal executable, it writes protocol JSON Lines and nothing else to
//! stdout, and it routes every diagnostic to stderr.
//!
//! This target exists only for tests. It is not a production Adapter, it is not registered,
//! wrapped, or approved anywhere, and nothing in the runtime path references it. Do not promote
//! it: a real Adapter must satisfy the full frozen Adapter boundary, not just the test assertions.
//!
//! Mode selection is explicit: the mode is the single positional argument. A missing mode, an
//! unknown mode, or a violated fixture precondition is a hard, loud failure on stderr with a
//! non-zero exit status. It is never a silent success, because a silently "successful" fixture
//! would let a broken transport masquerade as a passing test.

#![forbid(unsafe_code)]

use std::{
    env, fs,
    io::{BufRead, Read, Write},
    process::{Command, Stdio},
    thread,
    time::Duration,
};

/// Exit status used for fixture usage errors that are not a simulated Adapter crash.
///
/// Simulated Adapter crashes keep their explicit per-mode statuses (42, 43) because tests assert
/// those exact codes as Adapter behaviour.
const FIXTURE_USAGE_EXIT: i32 = 64;

/// Reports a fixture usage or precondition failure without writing anything to stdout.
fn fail(message: impl std::fmt::Display) -> ! {
    eprintln!("fixture_adapter: {message}");
    std::process::exit(FIXTURE_USAGE_EXIT);
}

/// Reads one newline-terminated protocol request from stdin.
fn read_request() -> String {
    let mut input = String::new();
    std::io::stdin()
        .lock()
        .read_line(&mut input)
        .unwrap_or_else(|error| fail(format!("could not read a protocol request: {error}")));
    input
}

/// Writes one complete protocol line, terminated by `\n`, and flushes it immediately.
fn write_protocol_line(line: &[u8]) {
    let mut stdout = std::io::stdout().lock();
    stdout
        .write_all(line)
        .unwrap_or_else(|error| fail(format!("could not write a protocol line: {error}")));
    stdout
        .write_all(b"\n")
        .unwrap_or_else(|error| fail(format!("could not terminate a protocol line: {error}")));
    stdout
        .flush()
        .unwrap_or_else(|error| fail(format!("could not flush stdout: {error}")));
}

/// Reads stdin to EOF so an EOF-aware shutdown can complete.
fn drain_stdin() {
    let mut input = String::new();
    if let Err(error) = std::io::stdin().read_to_string(&mut input) {
        fail(format!("could not read stdin to EOF: {error}"));
    }
}

/// Reads one request, then waits for stdin EOF the way a well-behaved Adapter would.
fn answer_once_then_wait_for_eof(line: &[u8]) {
    let _request = read_request();
    write_protocol_line(line);
    drain_stdin();
}

/// This binary's own path, used to spawn fixture descendants.
fn own_executable() -> std::path::PathBuf {
    env::current_exe().unwrap_or_else(|error| fail(format!("no own executable path: {error}")))
}

/// Spawns this same binary again in `mode`, without a shell and with a literal argument vector.
fn spawn_descendant(mode: &str, extra: &[(&str, &str)], quiet_output: bool) {
    let mut command = Command::new(own_executable());
    command.env_clear().arg(mode);
    for (key, value) in extra {
        command.env(key, value);
    }
    command.stdin(Stdio::null());
    if quiet_output {
        command.stdout(Stdio::null()).stderr(Stdio::null());
    }
    let child = command
        .spawn()
        .unwrap_or_else(|error| fail(format!("could not spawn a {mode} descendant: {error}")));
    drop(child);
}

fn ready_value(name: &str) -> String {
    env::var(name).unwrap_or_else(|error| fail(format!("{name} is missing: {error}")))
}

/// Publish readiness only after this holder has used both inherited pipe descriptors.
fn announce_pipe_holder_ready() {
    use std::fs::OpenOptions;

    let token = ready_value("ADAPTER_SUPERVISOR_READY_TOKEN");
    if !token
        .bytes()
        .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
    {
        fail("readiness token must be ASCII alphanumeric or hyphen");
    }
    let file = ready_value("ADAPTER_SUPERVISOR_READY_FILE");
    let parent_pid: u32 = ready_value("ADAPTER_SUPERVISOR_READY_PARENT_PID")
        .parse()
        .unwrap_or_else(|error| fail(format!("invalid parent PID: {error}")));
    let pid = std::process::id();
    let message = format!(
        r#"{{"jsonrpc":"2.0","method":"fixture.ready","params":{{"token":"{token}","parent_pid":{parent_pid},"pid":{pid}}}}}"#,
    );
    write_protocol_line(message.as_bytes());
    let mut stderr = std::io::stderr().lock();
    writeln!(stderr, "{message}")
        .unwrap_or_else(|error| fail(format!("could not announce stderr readiness: {error}")));
    stderr
        .flush()
        .unwrap_or_else(|error| fail(format!("could not flush stderr readiness: {error}")));

    // A reader must never see the file between create_new and its final write.
    // hard_link publishes a complete record atomically and refuses to overwrite an old ready file.
    let temporary = format!("{file}.tmp-{pid}");
    let mut ready = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temporary)
        .unwrap_or_else(|error| fail(format!("could not create readiness file: {error}")));
    writeln!(ready, "{token}:{parent_pid}:{pid}")
        .unwrap_or_else(|error| fail(format!("could not announce readiness: {error}")));
    ready
        .flush()
        .unwrap_or_else(|error| fail(format!("could not flush readiness: {error}")));
    drop(ready);
    fs::hard_link(&temporary, &file)
        .unwrap_or_else(|error| fail(format!("could not publish readiness file: {error}")));
    fs::remove_file(&temporary)
        .unwrap_or_else(|error| fail(format!("could not remove readiness staging file: {error}")));
    thread::sleep(Duration::from_secs(10));
}

const INITIALIZE_OK: &[u8] = br#"{"jsonrpc":"2.0","id":"init-01992d6d-2f31-7b11-8aa8-c975a632c003","result":{"kind":"initialize","protocol_version":"0.1.0","adapter_version":"0.1.0","harness_version":"1.0.0","capabilities":["health.check","run.start","run.status","run.cancel"],"auth_status":"ready","isolation_mode":"config_directory","recovery_mode":"status_query"}}"#;
const INITIALIZE_INCOMPATIBLE_VERSION: &[u8] = br#"{"jsonrpc":"2.0","id":"init-01992d6d-2f31-7b11-8aa8-c975a632c003","result":{"kind":"initialize","protocol_version":"0.2.0","adapter_version":"0.1.0","capabilities":["health.check"],"auth_status":"ready","isolation_mode":"process","recovery_mode":"none"}}"#;
const INITIALIZE_ILLEGAL_SEMVER: &[u8] = br#"{"jsonrpc":"2.0","id":"init-01992d6d-2f31-7b11-8aa8-c975a632c003","result":{"kind":"initialize","protocol_version":"00.1.0","adapter_version":"0.1.0","capabilities":["health.check"],"auth_status":"ready","isolation_mode":"process","recovery_mode":"none"}}"#;
const INITIALIZE_MISSING_FIELDS: &[u8] = br#"{"jsonrpc":"2.0","id":"init-01992d6d-2f31-7b11-8aa8-c975a632c003","result":{"kind":"initialize","protocol_version":"0.1.0"}}"#;
const INITIALIZE_RPC_ERROR: &[u8] = br#"{"jsonrpc":"2.0","id":"init-01992d6d-2f31-7b11-8aa8-c975a632c003","error":{"code":-32010,"message":"Adapter incompatible","data":{"domain_code":"ADAPTER_INCOMPATIBLE","retryable":false,"correlation_id":"01992d6d-2f31-7b11-8aa8-c975a632c003"}}}"#;
const HEALTH_OK: &[u8] = br#"{"jsonrpc":"2.0","id":"health-01992d6d-2f31-7b11-8aa8-c975a632c003","result":{"kind":"health","status":"healthy","checked_at":"2026-09-24T02:30:00.000Z","diagnostic_codes":["ALL_SYSTEMS_GO"]}}"#;
const HEALTH_DEGRADED: &[u8] = br#"{"jsonrpc":"2.0","id":"health-01992d6d-2f31-7b11-8aa8-c975a632c003","result":{"kind":"health","status":"degraded","checked_at":"2026-09-24T02:30:00.000Z","diagnostic_codes":["HIGH_LATENCY","MEMORY_PRESSURE"]}}"#;
const HEALTH_MISSING_FIELDS: &[u8] = br#"{"jsonrpc":"2.0","id":"health-01992d6d-2f31-7b11-8aa8-c975a632c003","result":{"kind":"health"}}"#;
const RUN_START_OK: &[u8] = br#"{"jsonrpc":"2.0","id":"run-01992d6d-2f31-7b11-8aa8-c975a632c003","result":{"kind":"run_start","accepted":true,"session_reference":"session-mock-01992d6d-2f31-7b11-8aa8-c975a632c003","observed_at":"2026-09-24T02:30:00.000Z"}}"#;
const RUN_HEARTBEAT: &[u8] = br#"{"jsonrpc":"2.0","method":"run.heartbeat","params":{"team_id":"01992d6d-2f31-7b11-8aa8-c975a632c001","task_id":"01992d6d-2f31-7b11-8aa8-c975a632c002","run_id":"01992d6d-2f31-7b11-8aa8-c975a632c003","role_id":"01992d6d-2f31-7b11-8aa8-c975a632c004","binding_id":"01992d6d-2f31-7b11-8aa8-c975a632c005","binding_version":1,"fencing_token":1,"sequence":1,"observed_at":"2026-09-24T02:30:00.100Z","phase":"executing","progress_percent":25.0}}"#;
const RUN_OUTPUT_STDOUT: &[u8] = br#"{"jsonrpc":"2.0","method":"run.output","params":{"team_id":"01992d6d-2f31-7b11-8aa8-c975a632c001","task_id":"01992d6d-2f31-7b11-8aa8-c975a632c002","run_id":"01992d6d-2f31-7b11-8aa8-c975a632c003","role_id":"01992d6d-2f31-7b11-8aa8-c975a632c004","binding_id":"01992d6d-2f31-7b11-8aa8-c975a632c005","binding_version":1,"fencing_token":1,"sequence":2,"observed_at":"2026-09-24T02:30:00.200Z","kind":"stdout","content":"Mock execution started for task\n","truncated":false}}"#;
const RUN_OUTPUT_ARTIFACT: &[u8] = br#"{"jsonrpc":"2.0","method":"run.output","params":{"team_id":"01992d6d-2f31-7b11-8aa8-c975a632c001","task_id":"01992d6d-2f31-7b11-8aa8-c975a632c002","run_id":"01992d6d-2f31-7b11-8aa8-c975a632c003","role_id":"01992d6d-2f31-7b11-8aa8-c975a632c004","binding_id":"01992d6d-2f31-7b11-8aa8-c975a632c005","binding_version":1,"fencing_token":1,"sequence":3,"observed_at":"2026-09-24T02:30:00.300Z","kind":"artifact_candidate","artifact":{"kind":"artifact","uri":"artifact://01992d6d-2f31-7b11-8aa8-c975a632c003/result.txt","version":"1.0","checksum":"sha256:e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855","media_type":"text/plain"}}}"#;
const RUN_COMPLETED: &[u8] = br#"{"jsonrpc":"2.0","method":"run.completed","params":{"team_id":"01992d6d-2f31-7b11-8aa8-c975a632c001","task_id":"01992d6d-2f31-7b11-8aa8-c975a632c002","run_id":"01992d6d-2f31-7b11-8aa8-c975a632c003","role_id":"01992d6d-2f31-7b11-8aa8-c975a632c004","binding_id":"01992d6d-2f31-7b11-8aa8-c975a632c005","binding_version":1,"fencing_token":1,"sequence":4,"completed_at":"2026-09-24T02:30:00.400Z","result_references":[{"kind":"artifact","uri":"artifact://01992d6d-2f31-7b11-8aa8-c975a632c003/result.txt","version":"1.0","checksum":"sha256:e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855","media_type":"text/plain"}],"summary":"Mock execution completed successfully."}}"#;
const RUN_NORMAL_INITIALIZE: &[u8] = br#"{"jsonrpc":"2.0","id":"init-01992d6d-2f31-7b11-8aa8-c975a632c007","result":{"kind":"initialize","protocol_version":"0.1.0","adapter_version":"0.1.0","capabilities":["run.start","run.cancel"],"auth_status":"ready","isolation_mode":"process","recovery_mode":"none"}}"#;

fn main() {
    let mode = env::args()
        .nth(1)
        .unwrap_or_else(|| fail("a fixture mode argument is required as the first argument"));

    match mode.as_str() {
        "environment" => {
            assert!(
                env::var_os("HOME").is_none(),
                "HOME must not be inherited into the Adapter"
            );
            assert!(
                env::var_os("PATH").is_none(),
                "PATH must not be inherited into the Adapter"
            );
            let explicit = env::var("ADAPTER_SUPERVISOR_EXPLICIT").unwrap_or_else(|error| {
                fail(format!("explicit environment value missing: {error}"))
            });
            if explicit != "allowed" {
                fail(format!(
                    "expected ADAPTER_SUPERVISOR_EXPLICIT=allowed, got {explicit:?}"
                ));
            }
            eprintln!("environment isolated");
        }
        "echo_line" => {
            // `read_line` keeps the terminator, so drop exactly one trailing newline and let
            // `write_protocol_line` re-add it. That returns the request line byte-for-byte.
            let mut request = read_request();
            if request.ends_with('\n') {
                request.pop();
            }
            if request.ends_with('\r') {
                request.pop();
            }
            write_protocol_line(request.as_bytes());
        }
        "literal_argument" => {
            let value = env::var("ADAPTER_SUPERVISOR_LITERAL")
                .unwrap_or_else(|error| fail(format!("literal argument value missing: {error}")));
            eprintln!("{value}");
        }
        "sleep" | "ignore_eof" => thread::sleep(Duration::from_secs(30)),
        "wait_for_eof" => {
            drain_stdin();
            eprintln!("stdin closed");
        }
        "write_marker_after_delay" => {
            thread::sleep(Duration::from_millis(150));
            let marker = env::var("ADAPTER_SUPERVISOR_MARKER")
                .unwrap_or_else(|error| fail(format!("marker path is missing: {error}")));
            if let Err(error) = fs::write(marker, "child survived") {
                fail(format!("could not write the marker file: {error}"));
            }
        }
        "large_stderr" => eprint!("{}", "x".repeat(512)),
        "spawn_pipe_holder_then_exit" => {
            let file = ready_value("ADAPTER_SUPERVISOR_READY_FILE");
            let token = ready_value("ADAPTER_SUPERVISOR_READY_TOKEN");
            spawn_descendant(
                "hold_pipes",
                &[
                    ("ADAPTER_SUPERVISOR_READY_FILE", file.as_str()),
                    ("ADAPTER_SUPERVISOR_READY_TOKEN", token.as_str()),
                    (
                        "ADAPTER_SUPERVISOR_READY_PARENT_PID",
                        &std::process::id().to_string(),
                    ),
                ],
                false,
            );
            // The test has already received the child-owned readiness message before it closes
            // this stdin.  EOF is therefore the normal parent exit path, not a timing guess.
            drain_stdin();
        }
        "spawn_marker_descendant_then_exit" => {
            let marker = env::var("ADAPTER_SUPERVISOR_MARKER")
                .unwrap_or_else(|error| fail(format!("marker path is missing: {error}")));
            spawn_descendant(
                "write_marker_after_delay",
                &[("ADAPTER_SUPERVISOR_MARKER", marker.as_str())],
                true,
            );
        }
        // Long enough that any bounded shutdown/drop assertion in the supervising test can never
        // accidentally pass by outliving this hold.
        "hold_pipes" => announce_pipe_holder_ready(),
        "oversized_then_valid" => {
            let oversized = vec![b'x'; 1_048_577];
            write_protocol_line(&oversized);
            write_protocol_line(br#"{"ok":true}"#);
        }
        "handshake_ok" => answer_once_then_wait_for_eof(INITIALIZE_OK),
        "handshake_incompatible_version" => {
            answer_once_then_wait_for_eof(INITIALIZE_INCOMPATIBLE_VERSION)
        }
        "handshake_illegal_semver" => answer_once_then_wait_for_eof(INITIALIZE_ILLEGAL_SEMVER),
        "handshake_missing_fields" => answer_once_then_wait_for_eof(INITIALIZE_MISSING_FIELDS),
        "handshake_not_json_rpc" => {
            let _request = read_request();
            // Deliberately not JSON-RPC: the supervisor must reject this line rather than skip it.
            write_protocol_line(b"not json-rpc response");
        }
        "handshake_exit_early" => {
            let _request = read_request();
            eprintln!("simulated adapter crash during handshake");
            std::process::exit(42);
        }
        "handshake_rpc_error" => answer_once_then_wait_for_eof(INITIALIZE_RPC_ERROR),
        "health_check_ok" => answer_once_then_wait_for_eof(HEALTH_OK),
        "health_check_degraded" => answer_once_then_wait_for_eof(HEALTH_DEGRADED),
        "health_check_missing_fields" => answer_once_then_wait_for_eof(HEALTH_MISSING_FIELDS),
        "health_check_exit_early" => {
            let _request = read_request();
            eprintln!("simulated health check crash");
            std::process::exit(43);
        }
        "run_normal_ok" => {
            let _initialize_request = read_request();
            write_protocol_line(RUN_NORMAL_INITIALIZE);
            let _run_start_request = read_request();
            write_protocol_line(RUN_START_OK);
            write_protocol_line(RUN_HEARTBEAT);
            write_protocol_line(RUN_OUTPUT_STDOUT);
            write_protocol_line(RUN_OUTPUT_ARTIFACT);
            write_protocol_line(RUN_COMPLETED);
            drain_stdin();
        }
        unexpected => fail(format!("unexpected fixture mode: {unexpected}")),
    }
}
