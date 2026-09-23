use std::io::{self, Write};
use std::sync::{Arc, Mutex};

use observability::{Correlation, JsonSubscriber, REDACTED, Redactor, Sensitive};
use serde_json::Value;

const REQUEST_ID: &str = "01992d6d-2f31-7b11-8aa8-c975a632c001";
const TEAM_ID: &str = "01992d6d-2f31-7b11-8aa8-c975a632c002";
const TASK_ID: &str = "01992d6d-2f31-7b11-8aa8-c975a632c003";
const RUN_ID: &str = "01992d6d-2f31-7b11-8aa8-c975a632c004";
const TEST_SECRET: &str = "wave2-secret-cookie";

#[derive(Clone, Default)]
struct SharedBuffer(Arc<Mutex<Vec<u8>>>);

impl SharedBuffer {
    fn json_lines(&self) -> Vec<Value> {
        let bytes = self.0.lock().expect("test buffer mutex must be available");
        String::from_utf8(bytes.clone())
            .expect("subscriber output must be UTF-8")
            .lines()
            .map(|line| serde_json::from_str(line).expect("each log line must be JSON"))
            .collect()
    }
}

impl Write for SharedBuffer {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.0
            .lock()
            .map_err(|_| io::Error::other("test buffer mutex poisoned"))?
            .extend_from_slice(bytes);
        Ok(bytes.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

fn subscriber(buffer: SharedBuffer) -> JsonSubscriber<SharedBuffer> {
    JsonSubscriber::with_timestamp_provider(buffer, Redactor::new([TEST_SECRET]), || {
        "2026-09-22T00:00:00.000Z".to_owned()
    })
}

#[test]
fn writes_structured_json_with_required_and_optional_correlation_fields() {
    let buffer = SharedBuffer::default();
    let subscriber = subscriber(buffer.clone());

    tracing::subscriber::with_default(subscriber, || {
        let correlation = Correlation::new(REQUEST_ID)
            .with_team_id(TEAM_ID)
            .with_task_id(TASK_ID)
            .with_run_id(RUN_ID);
        let span = correlation.span();
        let _entered = span.enter();

        tracing::info!(operation = "task.claim", attempt = 2_u64, "claim accepted");
    });

    let logs = buffer.json_lines();
    assert_eq!(logs.len(), 1);
    let log = &logs[0];
    assert_eq!(log["timestamp"], "2026-09-22T00:00:00.000Z");
    assert_eq!(log["level"], "INFO");
    assert_eq!(log["module"], "structured_logging");
    assert_eq!(log["request_id"], REQUEST_ID);
    assert_eq!(log["team_id"], TEAM_ID);
    assert_eq!(log["task_id"], TASK_ID);
    assert_eq!(log["run_id"], RUN_ID);
    assert_eq!(log["fields"]["message"], "claim accepted");
    assert_eq!(log["fields"]["operation"], "task.claim");
    assert_eq!(log["fields"]["attempt"], 2);
}

#[test]
fn emits_explicit_nulls_when_correlation_is_not_in_scope() {
    let buffer = SharedBuffer::default();
    let subscriber = subscriber(buffer.clone());

    tracing::subscriber::with_default(subscriber, || {
        tracing::warn!(reason = "startup", "no request scope");
    });

    let logs = buffer.json_lines();
    let log = &logs[0];
    assert!(log.get("timestamp").is_some());
    assert!(log.get("level").is_some());
    assert!(log.get("module").is_some());
    assert_eq!(log["request_id"], Value::Null);
    assert_eq!(log["team_id"], Value::Null);
    assert_eq!(log["task_id"], Value::Null);
    assert_eq!(log["run_id"], Value::Null);
}

#[test]
fn propagates_parent_correlation_through_nested_spans() {
    let buffer = SharedBuffer::default();
    let subscriber = subscriber(buffer.clone());

    tracing::subscriber::with_default(subscriber, || {
        let parent = Correlation::new(REQUEST_ID).with_task_id(TASK_ID).span();
        let _parent_entered = parent.enter();
        let child = tracing::info_span!("adapter_call", adapter = "mock");
        let _child_entered = child.enter();

        tracing::info!("nested event");
    });

    let logs = buffer.json_lines();
    assert_eq!(logs[0]["request_id"], REQUEST_ID);
    assert_eq!(logs[0]["task_id"], TASK_ID);
    assert_eq!(logs[0]["team_id"], Value::Null);
    assert_eq!(logs[0]["run_id"], Value::Null);
}

#[test]
fn preserves_correlation_until_the_last_span_handle_is_dropped() {
    let buffer = SharedBuffer::default();
    let subscriber = subscriber(buffer.clone());

    tracing::subscriber::with_default(subscriber, || {
        let original = Correlation::new(REQUEST_ID).with_run_id(RUN_ID).span();
        let remaining_handle = original.clone();
        drop(original);
        let _entered = remaining_handle.enter();

        tracing::info!("event from cloned span handle");
    });

    let logs = buffer.json_lines();
    assert_eq!(logs[0]["request_id"], REQUEST_ID);
    assert_eq!(logs[0]["run_id"], RUN_ID);
}

#[test]
fn child_span_keeps_parent_correlation_after_parent_handle_is_dropped() {
    let buffer = SharedBuffer::default();
    let subscriber = subscriber(buffer.clone());

    tracing::subscriber::with_default(subscriber, || {
        let child = {
            let parent = Correlation::new(REQUEST_ID).with_task_id(TASK_ID).span();
            let _entered = parent.enter();
            tracing::info_span!("deferred_child")
        };
        let _child_entered = child.enter();

        tracing::info!("deferred child event");
    });

    let logs = buffer.json_lines();
    assert_eq!(logs[0]["request_id"], REQUEST_ID);
    assert_eq!(logs[0]["task_id"], TASK_ID);
}

#[test]
fn redacts_registered_secrets_derived_forms_and_sensitive_field_names() {
    let buffer = SharedBuffer::default();
    let subscriber = subscriber(buffer.clone());
    let base64_secret = "d2F2ZTItc2VjcmV0LWNvb2tpZQ==";
    let hex_secret = "77617665322d7365637265742d636f6f6b6965";
    let percent_secret = "wave2%2Dsecret%2Dcookie";

    tracing::subscriber::with_default(subscriber, || {
        let span = Correlation::new(REQUEST_ID).span();
        let _entered = span.enter();
        tracing::error!(
            credential = "unregistered credential value",
            token = "unregistered token value",
            cookie = "unregistered cookie value",
            detail = %format!(
                "raw={TEST_SECRET}; base64={base64_secret}; hex={hex_secret}; percent={percent_secret}"
            ),
            "safe diagnostic"
        );
    });

    let encoded = serde_json::to_string(&buffer.json_lines()).expect("logs serialize");
    for dangerous in [
        TEST_SECRET,
        base64_secret,
        hex_secret,
        percent_secret,
        "unregistered credential value",
        "unregistered token value",
        "unregistered cookie value",
    ] {
        assert!(
            !encoded.contains(dangerous),
            "leaked dangerous value: {dangerous}"
        );
    }
    assert!(encoded.contains(REDACTED));
}

#[test]
fn error_event_and_sensitive_value_share_one_safe_representation() {
    let redactor = Redactor::new([TEST_SECRET]);
    let raw = format!("operation failed with token={TEST_SECRET}");
    let safe_error = redactor.redact_error(&raw);
    let safe_event = redactor.redact_event(&raw);
    let sensitive = Sensitive::new(TEST_SECRET.to_owned());

    assert_eq!(
        safe_error.as_str(),
        format!("operation failed with token={REDACTED}")
    );
    assert_eq!(safe_event, safe_error);
    assert_eq!(sensitive.to_string(), REDACTED);
    assert_eq!(format!("{sensitive:?}"), REDACTED);
    assert_eq!(
        serde_json::to_string(&sensitive).expect("sensitive serializes"),
        format!("\"{REDACTED}\"")
    );
}
