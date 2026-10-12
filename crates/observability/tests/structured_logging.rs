use std::io::{self, Write};
use std::sync::{Arc, Mutex};

use observability::{Correlation, JsonSubscriber, REDACTED, Redactor, Sensitive};
use serde_json::Value;

const REQUEST_ID: &str = "01992d6d-2f31-7b11-8aa8-c975a632c001";
const TEAM_ID: &str = "01992d6d-2f31-7b11-8aa8-c975a632c002";
const TASK_ID: &str = "01992d6d-2f31-7b11-8aa8-c975a632c003";
const RUN_ID: &str = "01992d6d-2f31-7b11-8aa8-c975a632c004";
const TEST_SECRET: &str = "wave2-secret-cookie";
/// Synthetic audit secret only; no real credential is used anywhere in this suite.
const AUDIT_SECRET: &str = "Audit-Only/AaB";

#[derive(Clone, Default)]
struct SharedBuffer(Arc<Mutex<Vec<u8>>>);

impl SharedBuffer {
    fn text(&self) -> String {
        let bytes = self.0.lock().expect("test buffer mutex must be available");
        String::from_utf8(bytes.clone()).expect("subscriber output must be UTF-8")
    }

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

fn fixed_timestamp_subscriber(
    buffer: SharedBuffer,
    redactor: Redactor,
) -> JsonSubscriber<SharedBuffer> {
    JsonSubscriber::with_timestamp_provider(buffer, redactor, || {
        "2026-09-22T00:00:00.000Z".to_owned()
    })
}

fn subscriber(buffer: SharedBuffer) -> JsonSubscriber<SharedBuffer> {
    fixed_timestamp_subscriber(buffer, Redactor::new([TEST_SECRET]))
}

fn audit_subscriber(buffer: SharedBuffer) -> JsonSubscriber<SharedBuffer> {
    fixed_timestamp_subscriber(buffer, Redactor::new([AUDIT_SECRET]))
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

// ---------------------------------------------------------------------------
// MM-R04 / DA-04: percent-escape matching must normalize only the hex digits.
//
// The three cases below are the audit RED inputs migrated verbatim from
// `docs/reviews/evidence/deep-audit-2026-10-08/main-repro/`. Only the dummy
// `AUDIT_SECRET` above is used; no real credential appears in this file.
// ---------------------------------------------------------------------------

#[test]
fn allowed_plain_and_upper_hex_forms_are_sanitized() {
    let redactor = Redactor::new([AUDIT_SECRET]);
    for form in [AUDIT_SECRET, "Audit%2DOnly%2FAaB"] {
        assert_eq!(redactor.sanitize(form).as_str(), REDACTED);
    }
}

#[test]
fn lower_hex_digits_keep_secret_letter_case() {
    let redactor = Redactor::new([AUDIT_SECRET]);
    assert_eq!(
        redactor.sanitize("Audit%2dOnly%2fAaB").as_str(),
        REDACTED,
        "percent escapes are case-insensitive; unescaped letters retain their bytes"
    );
}

#[test]
fn mixed_hex_digits_keep_secret_letter_case() {
    let redactor = Redactor::new([AUDIT_SECRET]);
    assert_eq!(
        redactor.sanitize("Audit%2DOnly%2fAaB").as_str(),
        REDACTED,
        "valid mixed hex escapes must not leak registered secrets"
    );
}

#[test]
fn structured_output_redacts_percent_encoded_secret_with_mixed_hex_case() {
    let buffer = SharedBuffer::default();
    let subscriber = audit_subscriber(buffer.clone());

    tracing::subscriber::with_default(subscriber, || {
        let span = Correlation::new(REQUEST_ID).with_team_id(TEAM_ID).span();
        let _entered = span.enter();
        tracing::error!(
            detail = "url=Audit%2dOnly%2fAaB&again=Audit%2DOnly%2FAaB",
            "percent encoded value in url"
        );
    });

    let raw = buffer.text();
    assert_eq!(raw.lines().count(), 1, "one JSON line per event");
    assert!(
        !raw.contains("Audit") && !raw.contains("Only") && !raw.contains("AaB"),
        "structured output must not contain any decoded or encoded fragment: {raw}"
    );
    assert_eq!(raw.matches(REDACTED).count(), 2, "both occurrences: {raw}");

    let logs = buffer.json_lines();
    assert_eq!(
        logs[0]["fields"]["detail"],
        format!("url={REDACTED}&again={REDACTED}")
    );
    assert_eq!(logs[0]["fields"]["message"], "percent encoded value in url");
    assert_eq!(logs[0]["request_id"], REQUEST_ID);
    assert_eq!(logs[0]["team_id"], TEAM_ID);
    assert_eq!(logs[0]["task_id"], Value::Null);
}

#[test]
fn percent_hex_digit_case_is_the_only_normalization() {
    let redactor = Redactor::new([AUDIT_SECRET]);
    for form in [
        "Audit%2DOnly%2FAaB",
        "Audit%2DOnly%2fAaB",
        "Audit%2dOnly%2FAaB",
        "Audit%2dOnly%2fAaB",
    ] {
        assert_eq!(redactor.sanitize(form).as_str(), REDACTED, "form: {form}");
    }
}

#[test]
fn partially_escaped_and_multi_segment_encodings_are_redacted() {
    let redactor = Redactor::new([AUDIT_SECRET]);
    for form in ["Audit-Only%2FAaB", "Audit%2DOnly/AaB", "Audit%2dOnly%2FAaB"] {
        assert_eq!(redactor.sanitize(form).as_str(), REDACTED, "form: {form}");
    }
    assert_eq!(
        redactor
            .sanitize("a=Audit%2dOnly%2fAaB&b=Audit%2DOnly%2FAaB")
            .as_str(),
        format!("a={REDACTED}&b={REDACTED}")
    );
    assert_eq!(
        redactor
            .sanitize("Audit%2dOnly%2fAaB|Audit%2DOnly%2FAaB")
            .as_str(),
        format!("{REDACTED}|{REDACTED}")
    );
    assert_eq!(
        redactor
            .sanitize("100% of Audit%2dOnly%2fAaB cases")
            .as_str(),
        format!("100% of {REDACTED} cases")
    );
}

#[test]
fn utf8_secret_percent_forms_are_redacted() {
    let secret = "Ünïcode-Sécret";
    let redactor = Redactor::new([secret]);
    for form in [
        "%C3%9Cn%C3%AFcode%2DS%C3%A9cret",
        "%c3%9Cn%c3%AFcode%2dS%c3%a9cret",
        "%C3%9Cn%C3%AFcode-S%C3%A9cret",
        secret,
    ] {
        assert_eq!(redactor.sanitize(form).as_str(), REDACTED, "form: {form}");
    }
    assert_eq!(
        redactor
            .sanitize("值=%C3%9Cn%c3%AFcode%2dS%C3%A9cret。")
            .as_str(),
        format!("值={REDACTED}。")
    );
}

#[test]
fn percent_boundary_conditions_do_not_over_match() {
    let redactor = Redactor::new([AUDIT_SECRET]);
    assert_eq!(redactor.sanitize("").as_str(), "");
    assert_eq!(redactor.sanitize("Audit%2").as_str(), "Audit%2");
    assert_eq!(redactor.sanitize("Audit%").as_str(), "Audit%");
    assert_eq!(
        redactor.sanitize("Audit%2ZOnly%2fAaB").as_str(),
        "Audit%2ZOnly%2fAaB"
    );
    assert_eq!(redactor.sanitize("%2d%2f").as_str(), "%2d%2f");
    assert_eq!(redactor.sanitize("Audit-Only/Aa").as_str(), "Audit-Only/Aa");
    assert_eq!(
        redactor.sanitize("Audit-Only/AaBX").as_str(),
        format!("{REDACTED}X")
    );
    assert_eq!(redactor.sanitize("Audit-Only/Aa").as_str(), "Audit-Only/Aa");
}

#[test]
fn unregistered_text_is_never_rewritten() {
    let redactor = Redactor::new([AUDIT_SECRET]);
    for text in [
        "ordinary log line without any secret",
        "Audit",
        "audit%2donly%2faab",
        "audit%2DOnly%2fAaB",
        // The first literal segment matches here, a later one does not. These decode
        // to a different value than the registered secret, so they must survive
        // byte-for-byte even though an earlier segment already lined up.
        "Audit%2Donly%2fAaB",
        "Audit-Only%2Faab",
        "Audit%2dOnly%2faab",
        "Audit-Only/Aa",
        "Only%2fAaB",
        "Audit%2dOnly%2fAaC",
        "100% complete",
        "%%%",
    ] {
        assert_eq!(redactor.sanitize(text).as_str(), text, "text: {text}");
    }
    let empty_policy = Redactor::default();
    assert_eq!(
        empty_policy.sanitize("Audit%2dOnly%2fAaB").as_str(),
        "Audit%2dOnly%2fAaB"
    );
}

#[test]
fn scan_stays_bounded_on_long_adversarial_input() {
    let redactor = Redactor::new([AUDIT_SECRET]);
    // Every position is a candidate match start here, which is the worst shape the
    // anchor scan can see. A non-linear or unbounded matcher would never return.
    let adversarial = "A".repeat(200_000);
    assert_eq!(redactor.sanitize(&adversarial).as_str(), adversarial);

    let with_secret = format!("{adversarial}Audit%2dOnly%2fAaB{adversarial}");
    let sanitized = redactor.sanitize(&with_secret);
    assert_eq!(
        sanitized.as_str(),
        format!("{adversarial}{REDACTED}{adversarial}")
    );
}

#[test]
fn sensitive_fields_and_plain_hex_base64_forms_keep_their_behaviour() {
    let redactor = Redactor::new([TEST_SECRET]);
    let hex_secret = "77617665322d7365637265742d636f6f6b6965";
    let base64_secret = "d2F2ZTItc2VjcmV0LWNvb2tpZQ==";
    let mixed = format!(
        "plain={TEST_SECRET}; lower={hex_secret}; upper={}; base64={base64_secret}",
        hex_secret.to_ascii_uppercase()
    );

    assert_eq!(
        redactor.sanitize(&mixed).as_str(),
        format!("plain={REDACTED}; lower={REDACTED}; upper={REDACTED}; base64={REDACTED}")
    );

    let buffer = SharedBuffer::default();
    let subscriber = subscriber(buffer.clone());
    tracing::subscriber::with_default(subscriber, || {
        let span = Correlation::new(REQUEST_ID).span();
        let _entered = span.enter();
        tracing::info!(
            api_token = "unregistered api token value",
            percent_secret = "wave2%2dsecret%2dcookie",
            "sensitive field stays masked"
        );
    });

    let logs = buffer.json_lines();
    assert_eq!(logs[0]["fields"]["api_token"], REDACTED);
    assert_eq!(logs[0]["fields"]["percent_secret"], REDACTED);
    assert_eq!(logs[0]["fields"]["message"], "sensitive field stays masked");
    assert_eq!(logs[0]["request_id"], REQUEST_ID);
}

// ---------------------------------------------------------------------------
// FC-04 / MR-02: under a single percent-encoding pass **every** registered byte
// may be written literally or as `%XX`, the two hex digits of an escape are
// case-insensitive, and a literal byte keeps the registered byte's case. A pure
// hex spelling treats every per-digit case mix like the all-lower/all-upper one.
//
// Leak assertions decode the rendered text **once** and require that the decoded
// text no longer contains the registered secret. That probes the harm itself, so
// an assertion cannot pass because the input happened to be rewritten for some
// unrelated reason. Only the synthetic `AUDIT_SECRET`/`TEST_SECRET` constants are
// used; no real credential is read, written or printed by this suite.
// ---------------------------------------------------------------------------

/// The audit's measured misses plus the fully escaped spelling of `AUDIT_SECRET`.
const ENCODED_MISSES: [(&str, &str); 5] = [
    ("%41udit%2dOnly%2fAaB", "%41 is the first byte"),
    ("Audit%2d%4fnly%2fAaB", "%4f is the byte after the dash"),
    ("Audit%2dOnly%2f%41aB", "%41 is the lower-case a"),
    ("Audit%2dOnly%2fAa%42", "%42 is the final byte"),
    (
        "%41%75%64%69%74%2d%4f%6e%6c%79%2f%41%61%42",
        "every registered byte is escaped",
    ),
];

/// Lower-case pure hex spelling of `AUDIT_SECRET` (`Audit` `Only` `/` `AaB`).
const AUDIT_SECRET_HEX_LOWER: &str = "41756469742d4f6e6c792f416142";

fn hex_digit(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

/// One percent-decoding pass. A decoder outside the crate performs exactly this
/// step, so this is the shape in which a leak reaches a log reader.
fn percent_decode_once(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut index = 0usize;
    while index < bytes.len() {
        if bytes[index] == b'%'
            && index + 2 < bytes.len()
            && let Some(high) = hex_digit(bytes[index + 1])
            && let Some(low) = hex_digit(bytes[index + 2])
        {
            decoded.push((high << 4) | low);
            index += 3;
            continue;
        }
        decoded.push(bytes[index]);
        index += 1;
    }
    String::from_utf8_lossy(&decoded).into_owned()
}

/// Requires that neither the rendered text nor its single percent decode contains
/// the registered secret.
fn assert_rendered_hides_secret(rendered: &str, secret: &str, context: &str) {
    assert!(
        !rendered.contains(secret),
        "{context}: plaintext secret survived in {rendered}"
    );
    let decoded = percent_decode_once(rendered);
    assert!(
        !decoded.contains(secret),
        "{context}: one percent decode still yields the secret: {decoded}"
    );
}

/// Writes one real `tracing` error event through a real [`JsonSubscriber`] and
/// returns the raw JSON line together with the parsed `detail` field.
fn log_error_detail(redactor: Redactor, detail: &str) -> (String, String) {
    let buffer = SharedBuffer::default();
    let subscriber = fixed_timestamp_subscriber(buffer.clone(), redactor);
    tracing::subscriber::with_default(subscriber, || {
        let span = Correlation::new(REQUEST_ID).with_team_id(TEAM_ID).span();
        let _entered = span.enter();
        tracing::error!(detail = %detail, "encoded value in detail");
    });

    let raw = buffer.text();
    assert_eq!(raw.lines().count(), 1, "one JSON line per event: {raw}");
    let logs = buffer.json_lines();
    assert_eq!(logs[0]["level"], "ERROR");
    assert_eq!(logs[0]["fields"]["message"], "encoded value in detail");
    assert_eq!(logs[0]["request_id"], REQUEST_ID);
    assert_eq!(logs[0]["team_id"], TEAM_ID);
    let parsed = logs[0]["fields"]["detail"]
        .as_str()
        .expect("detail must stay a string")
        .to_owned();
    (raw, parsed)
}

#[test]
fn every_registered_byte_may_be_literal_or_escaped_in_structured_output() {
    for (encoded, note) in ENCODED_MISSES {
        let detail = format!("prefix url={encoded} suffix");
        let (raw, parsed) = log_error_detail(Redactor::new([AUDIT_SECRET]), &detail);

        assert_eq!(
            parsed,
            format!("prefix url={REDACTED} suffix"),
            "case: {note}"
        );
        assert!(
            !raw.contains(encoded),
            "{note}: the encoded spelling itself survived: {raw}"
        );
        assert_rendered_hides_secret(&raw, AUDIT_SECRET, note);
        assert_rendered_hides_secret(&parsed, AUDIT_SECRET, note);
    }
}

#[test]
fn plain_and_symbol_escaped_spellings_stay_redacted_as_the_positive_control() {
    for spelling in ["Audit-Only/AaB", "Audit%2dOnly%2fAaB"] {
        let (raw, parsed) = log_error_detail(Redactor::new([AUDIT_SECRET]), spelling);
        assert_eq!(parsed, REDACTED, "spelling: {spelling}");
        assert_rendered_hides_secret(&raw, AUDIT_SECRET, spelling);
    }
}

#[test]
fn error_and_event_paths_escape_every_registered_byte() {
    let redactor = Redactor::new([AUDIT_SECRET]);
    for (encoded, note) in ENCODED_MISSES {
        let text = format!("retry failed for url={encoded}");

        let safe_error = redactor.redact_error(&text);
        let safe_event = redactor.redact_event(&text);

        assert_eq!(
            safe_error.as_str(),
            format!("retry failed for url={REDACTED}"),
            "case: {note}"
        );
        assert_eq!(safe_event, safe_error, "one boundary: {note}");
        assert_rendered_hides_secret(safe_error.as_str(), AUDIT_SECRET, note);
        assert_rendered_hides_secret(safe_event.as_str(), AUDIT_SECRET, note);
    }
}

#[test]
fn pure_hex_spelling_treats_every_digit_case_mix_as_one_form() {
    let redactor = Redactor::new([AUDIT_SECRET]);
    let hex = AUDIT_SECRET_HEX_LOWER;
    // Every case spelling is derived from the registered bytes, so a typo in a
    // literal cannot turn this into a case that was never a registered form.
    let mixed = format!(
        "{}{}{}",
        &AUDIT_SECRET_HEX_LOWER[..6],
        AUDIT_SECRET_HEX_LOWER[6..14].to_ascii_uppercase(),
        &AUDIT_SECRET_HEX_LOWER[14..]
    );
    for (hex, note) in [
        (AUDIT_SECRET_HEX_LOWER.to_owned(), "all lower case"),
        (
            AUDIT_SECRET_HEX_LOWER.to_ascii_uppercase(),
            "all upper case",
        ),
        (mixed, "digit-by-digit mix"),
    ] {
        let detail = format!("blob=0x{hex}::ready");
        let (raw, parsed) = log_error_detail(redactor.clone(), &detail);

        assert_eq!(parsed, format!("blob=0x{REDACTED}::ready"), "case: {note}");
        assert!(!raw.contains(&hex), "{note}: hex spelling survived: {raw}");
        assert_rendered_hides_secret(&raw, AUDIT_SECRET, note);
    }

    // Text that still contains the whole registered spelling is removed, exactly
    // like the plaintext and percent forms (`Audit-Only/AaBX` -> `[REDACTED]X`).
    for (text, expected, note) in [
        (
            format!("{hex}42"),
            format!("{REDACTED}42"),
            "one byte pair appended",
        ),
        (
            format!("14{hex}"),
            format!("14{REDACTED}"),
            "shifted by one digit",
        ),
    ] {
        assert_eq!(redactor.sanitize(&text).as_str(), expected, "case: {note}");
    }

    // A window that is not the registered spelling keeps every byte.
    for (text, note) in [
        (format!("{}3", &hex[..hex.len() - 1]), "last digit changed"),
        (hex[..hex.len() - 2].to_owned(), "final byte pair missing"),
        ("0xDEADBEEF".to_owned(), "unrelated hex"),
    ] {
        assert_eq!(redactor.sanitize(&text).as_str(), text, "case: {note}");
    }
}

#[test]
fn surrounding_plain_text_and_repeats_are_never_rewritten() {
    let redactor = Redactor::new([AUDIT_SECRET]);
    let text = format!(
        "GET /v1/task?AUDIT_TOKEN=%41udit%2dOnly%2fAaB&retries=3|Audit-Only/AaB|{AUDIT_SECRET_HEX_LOWER}|Audit%2dOnly%2fAaBAudit%2dOnly%2fAaB"
    );
    let expected = format!(
        "GET /v1/task?AUDIT_TOKEN={REDACTED}&retries=3|{REDACTED}|{REDACTED}|{REDACTED}{REDACTED}"
    );

    assert_eq!(redactor.sanitize(&text).as_str(), expected);
    assert_eq!(redactor.redact_event(&text).as_str(), expected);

    let (raw, parsed) = log_error_detail(redactor, &text);
    assert_eq!(parsed, expected);
    assert_rendered_hides_secret(&raw, AUDIT_SECRET, "repeated spellings");
    assert!(
        raw.contains("GET /v1/task?AUDIT_TOKEN="),
        "prefix lost: {raw}"
    );
    assert!(raw.contains("&retries=3"), "suffix lost: {raw}");
    // Text that is not the secret keeps its case, including upper-case letters that
    // a whole-string case fold would silently rewrite.
    assert!(raw.contains("AUDIT_TOKEN"), "case folded: {raw}");
}

#[test]
fn malformed_and_truncated_encodings_are_left_untouched() {
    let redactor = Redactor::new([AUDIT_SECRET]);
    for text in [
        "%",
        "%2",
        "%4",
        "%zz",
        "%41u",
        "Audit%",
        "Audit%2",
        "Audit%zzOnly%2fAaB",
        "Audit%2dOnly%2fA%41",
        "Audit%2dOnly%2fAa",
        "%2d%2f",
        "Audit%2dOnly%2fAa%4",
        "%%%41%75%64%69%74",
        // Registered bytes line up but one byte differs, so nothing is removed.
        "Audit%2dOnly%2fAaC",
        "Audit%2Donly%2fAaB",
        "audit%2donly%2faab",
    ] {
        assert_eq!(redactor.sanitize(text).as_str(), text, "text: {text}");
    }

    // A complete match followed by a truncated escape keeps the trailing bytes.
    let (raw, parsed) = log_error_detail(Redactor::new([AUDIT_SECRET]), "Audit%2dOnly%2fAaB%4");
    assert_eq!(parsed, format!("{REDACTED}%4"));
    assert_rendered_hides_secret(&raw, AUDIT_SECRET, "truncated tail");
}

#[test]
fn an_empty_registration_redacts_nothing_and_still_honours_real_secrets() {
    let text = format!("prefix {AUDIT_SECRET} url=Audit%2dOnly%2fAaB hex={AUDIT_SECRET_HEX_LOWER}");
    for redactor in [
        Redactor::new([""]),
        Redactor::new(Vec::<String>::new()),
        Redactor::default(),
    ] {
        assert_eq!(redactor.sanitize(&text).as_str(), text);
        assert_eq!(redactor.redact_error(&text).as_str(), text);
        assert_eq!(redactor.redact_event(&text).as_str(), text);
    }

    let mixed = Redactor::new(["", AUDIT_SECRET]);
    assert_eq!(
        mixed.sanitize(text).as_str(),
        format!("prefix {REDACTED} url={REDACTED} hex={REDACTED}")
    );
}

#[test]
fn overlapping_registered_secrets_are_all_removed() {
    let redactor = Redactor::new([AUDIT_SECRET, "Audit-Only/Aa", "Only/AaB", "AaB"]);

    assert_eq!(
        redactor.sanitize(AUDIT_SECRET).as_str(),
        REDACTED,
        "the longest form must win over its own suffixes"
    );
    assert_eq!(redactor.sanitize("Audit-Only/Aa").as_str(), REDACTED);
    assert_eq!(redactor.sanitize("Only/AaB").as_str(), REDACTED);
    assert_eq!(redactor.sanitize("AaB").as_str(), REDACTED);

    let text = "x Audit-Only/AaB y Audit-Only/Aa z Only/AaB w %41udit%2dOnly%2fAaB";
    let expected = format!("x {REDACTED} y {REDACTED} z {REDACTED} w {REDACTED}");
    assert_eq!(redactor.sanitize(text).as_str(), expected);

    let (raw, parsed) = log_error_detail(redactor, text);
    assert_eq!(parsed, expected);
    assert_rendered_hides_secret(&raw, AUDIT_SECRET, "overlapping secrets");
    assert_eq!(
        raw.matches(REDACTED).count(),
        4,
        "each occurrence replaced exactly once: {raw}"
    );
}

#[test]
fn utf8_secret_accepts_full_escaping_and_keeps_registered_case() {
    let secret = "Ünïcode-Sécret";
    let redactor = Redactor::new([secret]);
    let escaped = "%C3%9Cn%C3%AFcode%2dS%C3%A9cret";
    let mixed_case_hex = "%c3%9Cn%C3%afcode%2DS%C3%A9cret";

    for spelling in [escaped, mixed_case_hex, "%C3%9Cn%C3%AFcode-S%C3%A9cret"] {
        let detail = format!("值={spelling}。");
        let (raw, parsed) = log_error_detail(redactor.clone(), &detail);
        assert_eq!(parsed, format!("值={REDACTED}。"), "spelling: {spelling}");
        assert_rendered_hides_secret(&raw, secret, spelling);
    }

    // The hexadecimal spelling of a multi-byte secret starts with a letter, which
    // is the anchor that finds the window.
    let hex_lower = "c39c6ec3af636f64652d53c3a963726574";
    for (hex, note) in [
        (hex_lower.to_owned(), "hex all lower case"),
        (hex_lower.to_ascii_uppercase(), "hex all upper case"),
        (
            format!(
                "{}{}{}",
                &hex_lower[..4],
                hex_lower[4..12].to_ascii_uppercase(),
                &hex_lower[12..]
            ),
            "hex digit-by-digit mix",
        ),
    ] {
        let (raw, parsed) = log_error_detail(redactor.clone(), &hex);
        assert_eq!(parsed, REDACTED, "case: {note}");
        assert!(!raw.contains(&hex), "{note}: hex spelling survived: {raw}");
        assert_rendered_hides_secret(&raw, secret, note);
    }
    let near_miss = format!("{}3", &hex_lower[..hex_lower.len() - 1]);
    assert_eq!(redactor.sanitize(&near_miss).as_str(), near_miss);

    // A same-shaped value with different case is a different value.
    for text in ["ünïcode-Sécret", "Ünïcode-sécret", "Ünïcode-Sécre"] {
        assert_eq!(redactor.sanitize(text).as_str(), text, "text: {text}");
    }
}

#[test]
fn sensitive_field_names_stay_masked_next_to_matching_content() {
    let buffer = SharedBuffer::default();
    let subscriber = audit_subscriber(buffer.clone());

    tracing::subscriber::with_default(subscriber, || {
        let span = Correlation::new(REQUEST_ID).span();
        let _entered = span.enter();
        tracing::error!(
            authorization = "%41udit%2dOnly%2fAaB",
            note = "unregistered value",
            detail = "url=%41udit%2dOnly%2fAaB",
            "sensitive name and encoded content"
        );
    });

    let raw = buffer.text();
    let logs = buffer.json_lines();
    assert_eq!(logs[0]["fields"]["authorization"], REDACTED);
    assert_eq!(logs[0]["fields"]["note"], "unregistered value");
    assert_eq!(logs[0]["fields"]["detail"], format!("url={REDACTED}"));
    assert_rendered_hides_secret(&raw, AUDIT_SECRET, "sensitive field");
}

#[test]
fn percent_scan_is_bounded_on_escaped_and_near_miss_inputs() {
    let redactor = Redactor::new([AUDIT_SECRET]);
    // Long inputs are compared without printing them, so a failure here reports the
    // shape instead of dumping a third of a megabyte of log text.
    let mut near_miss = "Audit-Only/AaX".repeat(20_000);
    assert!(
        redactor.sanitize(&near_miss).as_str() == near_miss,
        "near-miss input must survive unchanged ({} bytes)",
        near_miss.len()
    );
    // Every third byte is an escape anchor that can never start a match.
    let escapes = "%2d".repeat(100_000);
    assert!(
        redactor.sanitize(&escapes).as_str() == escapes,
        "escape-only input must survive unchanged ({} bytes)",
        escapes.len()
    );
    // A fully escaped spelling is still found at the end of a long run.
    let encoded = "%41%75%64%69%74%2d%4f%6e%6c%79%2f%41%61%42";
    near_miss = format!("{escapes}{encoded}{escapes}");
    let sanitized = redactor.sanitize(&near_miss);
    assert!(
        sanitized.as_str() == format!("{escapes}{REDACTED}{escapes}"),
        "the fully escaped spelling must still be removed ({} bytes in)",
        near_miss.len()
    );
}

// ---------------------------------------------------------------------------
// AF-01: a registered `%` has two legal spellings — the literal byte and the
// `%25` escape — and both start with the *same* input byte, so the matcher must
// carry both reachable positions instead of committing to one.
//
// The audit's three counterexamples (`docs/reviews/FC-RETURN-AUDIT-2026-10-09.md`
// §4) are the seeds of this block. Everything here runs through the four real
// production entry points, never through a hand-copied matcher, and every leak
// assertion decodes the rendered text **once** first, which is the shape in
// which an encoded secret reaches a log reader.
//
// Only synthetic constants are registered; no real credential is read, written,
// or printed by this suite.
// ---------------------------------------------------------------------------

/// Writes every registered byte as `%XX`, so a fully escaped spelling is derived
/// from the registered bytes instead of typed out by hand.
fn escape_every_byte(secret: &str) -> String {
    let mut escaped = String::with_capacity(secret.len() * 3);
    for byte in secret.as_bytes() {
        escaped.push_str(&format!("%{byte:02X}"));
    }
    escaped
}

/// The three audit counterexamples: registered secret and its legal once-encoded
/// spelling. The encoded form decodes back to the registered secret in exactly
/// one percent-decoding pass, which is the scope FC-04 promised.
const PERCENT_BYTE_CASES: [(&str, &str, &str); 3] = [
    ("%ABC", "%25ABC", "% is the first byte"),
    ("A%B", "A%25B", "% is a middle byte"),
    ("%%", "%25%25", "% bytes are consecutive"),
];

/// Asserts that all four production entry points remove `encoded` and leave no
/// surrounding text damaged. `expected` is what must survive around the match.
fn assert_all_four_entry_points_redact(secret: &str, encoded: &str, context: &str) {
    let redactor = Redactor::new([secret]);
    let detail = format!("prefix url={encoded} suffix");
    let expected = format!("prefix url={REDACTED} suffix");

    assert_eq!(
        redactor.sanitize(&detail).as_str(),
        expected,
        "{context}: sanitize"
    );
    assert_eq!(
        redactor.redact_error(&detail).as_str(),
        expected,
        "{context}: redact_error"
    );
    assert_eq!(
        redactor.redact_event(&detail).as_str(),
        expected,
        "{context}: redact_event"
    );

    let (raw, parsed) = log_error_detail(redactor, &detail);
    assert_eq!(parsed, expected, "{context}: JsonSubscriber detail field");
    assert!(
        !raw.contains(encoded),
        "{context}: the encoded spelling itself survived in {raw}"
    );
    assert!(raw.contains("prefix url="), "{context}: prefix lost: {raw}");
    assert!(raw.contains(" suffix"), "{context}: suffix lost: {raw}");
    assert_rendered_hides_secret(&raw, secret, context);
}

#[test]
fn registered_percent_at_start_is_redacted_in_all_four_entry_points() {
    let (secret, encoded, note) = PERCENT_BYTE_CASES[0];
    assert_eq!(secret, "%ABC");
    assert_eq!(encoded, "%25ABC");
    assert_all_four_entry_points_redact(secret, encoded, note);

    // The same secret written without escaping must also match, so the fix did
    // not simply prefer the escape branch.
    let redactor = Redactor::new([secret]);
    assert_eq!(
        redactor.sanitize("%ABC").as_str(),
        REDACTED,
        "literal spelling"
    );
    assert_eq!(
        redactor.sanitize("a%ABCb").as_str(),
        format!("a{REDACTED}b")
    );
}

#[test]
fn registered_percent_in_middle_is_redacted_in_all_four_entry_points() {
    let (secret, encoded, note) = PERCENT_BYTE_CASES[1];
    assert_eq!(secret, "A%B");
    assert_eq!(encoded, "A%25B");
    assert_all_four_entry_points_redact(secret, encoded, note);

    // Partly escaped spellings of the same secret are all legal: the `%` escaped
    // and the other bytes literal, or the reverse.
    let redactor = Redactor::new([secret]);
    for spelling in ["A%25B", "A%B", "%41%25%42"] {
        let sanitized = redactor.sanitize(spelling);
        assert_eq!(sanitized.as_str(), REDACTED, "spelling: {spelling}");
        assert_rendered_hides_secret(sanitized.as_str(), secret, spelling);
    }
}

#[test]
fn registered_percent_consecutive_is_redacted_in_all_four_entry_points() {
    let (secret, encoded, note) = PERCENT_BYTE_CASES[2];
    assert_eq!(secret, "%%");
    assert_eq!(encoded, "%25%25");
    assert_all_four_entry_points_redact(secret, encoded, note);

    // One of the two consecutive `%` bytes escaped and the other literal is a
    // legal mixed spelling that must also match.
    let redactor = Redactor::new([secret]);
    for spelling in ["%%", "%25%", "%25%25", "%%%"] {
        let sanitized = redactor.sanitize(spelling);
        assert!(
            !percent_decode_once(sanitized.as_str()).contains(secret),
            "spelling {spelling} leaked: {sanitized}"
        );
    }
}

#[test]
fn registered_percent_position_is_covered_at_start_middle_end_and_repeated() {
    // The four positions a registered `%` can occupy, each with its legal
    // once-encoded spelling and its literal spelling.
    let cases = [
        ("%head", "%25head", "% at the start"),
        ("mid%dle", "mid%25dle", "% in the middle"),
        ("tail%", "tail%25", "% at the end"),
        ("a%%b", "a%25%25b", "two adjacent % bytes"),
        ("%%%", "%25%25%25", "three adjacent % bytes"),
        ("%%%x", "%%25%x", "two escaped, one literal"),
    ];

    for (secret, encoded, note) in cases {
        let redactor = Redactor::new([secret]);
        for spelling in [secret, encoded] {
            let sanitized = redactor.sanitize(spelling);
            assert_eq!(
                sanitized.as_str(),
                REDACTED,
                "{note}: spelling {spelling} rendered as {sanitized}"
            );
            assert_rendered_hides_secret(sanitized.as_str(), secret, note);
        }
        // Surrounding text is preserved exactly.
        let wrapped = format!("[{encoded}]");
        assert_eq!(
            redactor.sanitize(&wrapped).as_str(),
            format!("[{REDACTED}]"),
            "{note}: wrapped"
        );
    }
}

#[test]
fn a_registered_secret_containing_literal_escape_fragments_is_still_redacted() {
    // The registered secret itself looks like an escape: `%25` and `%41`. Its
    // own literal spelling and its once-encoded spelling must both be removed,
    // and the matcher must not read the registered text as an instruction.
    let cases = [
        ("A%25B", "A%2525B", "registered text contains %25"),
        ("A%41B", "A%2541B", "registered text contains %41"),
        ("%25", "%2525", "the registered secret is the escape %25"),
        ("%41", "%2541", "the registered secret is the escape %41"),
    ];

    for (secret, encoded, note) in cases {
        let redactor = Redactor::new([secret]);
        for spelling in [secret, encoded] {
            let sanitized = redactor.sanitize(spelling);
            assert_eq!(
                sanitized.as_str(),
                REDACTED,
                "{note}: spelling {spelling} rendered as {sanitized}"
            );
            assert_rendered_hides_secret(sanitized.as_str(), secret, note);
        }
        // The once-escaped spelling above is the promised scope. Escaping that
        // result again is a *second* decode, which this policy never promised.
        // Control on a secret with no escape-shaped substring of its own: one
        // decode of the doubly-escaped spelling does not yield it either.
        let doubly_escaped = escape_every_byte(encoded);
        assert!(
            !percent_decode_once(&doubly_escaped).contains("Audit-Only"),
            "{note}: control secret must stay absent from a doubly escaped spelling"
        );
    }
}

#[test]
fn fully_and_partly_escaped_percent_secrets_agree_on_all_mixed_spellings() {
    // The registered `%` bytes are escaped independently of the other bytes, so
    // every combination of literal/escaped `%` in the same secret is legal.
    let redactor = Redactor::new(["%%"]);

    // Both escaped: the audit counterexample.
    assert_eq!(
        redactor.sanitize("%25%25").as_str(),
        REDACTED,
        "both escaped"
    );
    // First escaped, second literal.
    assert_eq!(
        redactor.sanitize("%25%").as_str(),
        REDACTED,
        "first escaped"
    );
    // First literal, second escaped.
    assert_eq!(
        redactor.sanitize("%%25").as_str(),
        REDACTED,
        "second escaped"
    );
    // Neither escaped.
    assert_eq!(redactor.sanitize("%%").as_str(), REDACTED, "both literal");
    // Mixed with an unrelated escape that is not a registered byte.
    assert_eq!(
        redactor.sanitize("q=%25%25&r=%%").as_str(),
        format!("q={REDACTED}&r={REDACTED}"),
        "both spellings in one string"
    );
    // The unrelated escape in front survives untouched.
    assert_eq!(
        redactor.sanitize("%41%25%25").as_str(),
        format!("%41{REDACTED}"),
        "unrelated leading escape is kept"
    );
}

#[test]
fn registered_percent_malformed_and_truncated_encodings_are_left_untouched() {
    // Negative controls: none of these is a registered spelling, so the redactor
    // must not rewrite them. Each is checked against a secret whose registered
    // bytes cannot be spelled by the text at all.
    let cases = [
        ("%ABC", "%", "bare percent"),
        ("%ABC", "%2", "truncated after one digit"),
        ("%ABC", "%2G", "non-hex digit in the escape"),
        ("%ABC", "%25A", "truncated registered tail"),
        ("%ABC", "%25A", "truncated before the last two bytes"),
        ("A%B", "A%2", "truncated escape in the middle"),
        ("A%B", "A%", "bare percent in the middle"),
        ("A%B", "A%2B", "escape spells the wrong byte"),
        ("%%", "%25", "only one of two registered bytes"),
        (
            "%%",
            "%252",
            "escaped percent then a literal non-percent byte",
        ),
        ("%%", "%2", "truncated before the first escape"),
        ("%%", "%2%", "truncated escape then an unrelated percent"),
        ("%%", "%2%2", "two truncated escapes with no literal pair"),
        // Note: text that contains `2525` is *not* a negative control, because
        // `2525` is the hexadecimal form of the registered `%%`, which this policy
        // already redacts independently of percent matching.
    ];

    for (secret, text, note) in cases {
        let redactor = Redactor::new([secret]);
        assert_eq!(
            redactor.sanitize(text).as_str(),
            text,
            "{note}: text {text} must survive unchanged"
        );
        assert_eq!(
            redactor.redact_error(text).as_str(),
            text,
            "{note}: redact_error"
        );
        assert_eq!(
            redactor.redact_event(text).as_str(),
            text,
            "{note}: redact_event"
        );
    }

    // A complete match followed by a truncated escape keeps the trailing bytes.
    let (raw, parsed) = log_error_detail(Redactor::new(["%ABC"]), "%25ABC%2");
    assert_eq!(parsed, format!("{REDACTED}%2"));
    assert_rendered_hides_secret(&raw, "%ABC", "complete match plus truncation");
}

#[test]
fn utf8_registered_secret_with_percent_bytes_is_redacted() {
    // A multi-byte secret whose text contains `%`: the literal path and the
    // escape path both have to survive UTF-8 continuation bytes.
    let cases = [
        ("Ünï%code", "%25", "percent at the end of a UTF-8 secret"),
        ("%Ünï", "%25", "percent at the start of a UTF-8 secret"),
        ("Sé%crét", "%25", "percent inside a UTF-8 secret"),
    ];

    for (secret, _, note) in cases {
        let redactor = Redactor::new([secret]);
        // Fully escaped spelling, derived byte by byte from the registered bytes.
        let escaped = escape_every_byte(secret);
        // The literal `%` written as `%25` and everything else escaped.
        let mut half_built = String::new();
        for byte in secret.as_bytes() {
            if *byte == b'%' {
                half_built.push_str("%25");
            } else {
                half_built.push_str(&format!("%{byte:02X}"));
            }
        }

        for spelling in [secret, escaped.as_str(), half_built.as_str()] {
            let detail = format!("值={spelling}。");
            let (raw, parsed) = log_error_detail(redactor.clone(), &detail);
            assert_eq!(
                parsed,
                format!("值={REDACTED}。"),
                "{note}: spelling {spelling} rendered as {parsed}"
            );
            assert_rendered_hides_secret(&raw, secret, note);
        }
    }

    // A same-shaped value with different case is a different value.
    let redactor = Redactor::new(["Ünï%code"]);
    for text in ["ünï%code", "Ünï%Code", "Ünï%cod"] {
        assert_eq!(redactor.sanitize(text).as_str(), text, "text: {text}");
    }
}

#[test]
fn empty_and_overlapping_percent_secrets_are_handled() {
    // An empty registration still redacts nothing on its own.
    let text = "%25ABC|%%|a%25b";
    for redactor in [
        Redactor::new([""]),
        Redactor::new(Vec::<String>::new()),
        Redactor::default(),
    ] {
        assert_eq!(redactor.sanitize(text).as_str(), text);
        assert_eq!(redactor.redact_error(text).as_str(), text);
        assert_eq!(redactor.redact_event(text).as_str(), text);
    }

    // Overlapping registrations, including percent-bearing ones: the longest
    // registered form wins and every occurrence is replaced exactly once.
    let redactor = Redactor::new(["%ABC", "%AB", "AB", "%%", "A%B"]);
    assert_eq!(redactor.sanitize("%ABC").as_str(), REDACTED, "longest wins");
    assert_eq!(redactor.sanitize("%AB").as_str(), REDACTED);
    assert_eq!(redactor.sanitize("AB").as_str(), REDACTED);
    assert_eq!(redactor.sanitize("%%").as_str(), REDACTED);
    assert_eq!(redactor.sanitize("A%B").as_str(), REDACTED);

    let mixed = "x %25ABC y %ABC z %25%25 w A%25B";
    let sanitized = redactor.sanitize(mixed);
    assert_eq!(
        sanitized.as_str(),
        format!("x {REDACTED} y {REDACTED} z {REDACTED} w {REDACTED}")
    );
    assert!(!percent_decode_once(sanitized.as_str()).contains("%ABC"));

    let (raw, parsed) = log_error_detail(redactor, mixed);
    assert_eq!(
        parsed,
        format!("x {REDACTED} y {REDACTED} z {REDACTED} w {REDACTED}")
    );
    assert_eq!(
        raw.matches(REDACTED).count(),
        4,
        "each replaced once: {raw}"
    );
}

#[test]
fn percent_ambiguity_work_stays_inside_its_polynomial_bound() {
    // The matcher's documented bound is O(secret_len^2) per candidate position:
    // after i matched bytes the reachable set is indexed by 0..=i, so layer i
    // costs O(i) and all n layers cost O(n^2). This test exercises the worst
    // shape for that bound — a secret made almost entirely of `%`, where every
    // layer is ambiguous and reachable — against long near-miss inputs, and
    // records the observed cost. An exponential spelling walk would need 2^48
    // paths here and could not finish; the layered walk must.
    let ambiguous_secret = format!("{}tail", "%".repeat(48));
    let redactor = Redactor::new([ambiguous_secret.clone()]);

    // Fully escaped spelling: every `%` takes the three-byte branch, so every
    // layer keeps two reachable states and the longest-match splice must consume
    // the whole run.
    let encoded = escape_every_byte(&ambiguous_secret);

    // A long near miss that ends one byte early, so the last layer fails after
    // every layer before it stayed maximally ambiguous.
    let near_miss = format!("{}{}", "%25".repeat(20_000), &encoded[..encoded.len() - 3]);
    let started = std::time::Instant::now();
    let sanitized = redactor.sanitize(&near_miss);
    let elapsed = started.elapsed();

    // The truncated tail is not the registered secret, so nothing is removed.
    assert_eq!(
        sanitized.as_str(),
        near_miss,
        "near miss must survive unchanged ({} bytes in)",
        near_miss.len()
    );
    assert!(
        elapsed.as_secs() < 30,
        "O(n^2) walk took {elapsed:?} on {} bytes; the bound does not hold",
        near_miss.len()
    );

    // The positive control on the same input shape: the complete spelling at the
    // end of the same run is still removed.
    let with_match = format!("{near_miss}{encoded}");
    let started = std::time::Instant::now();
    let sanitized = redactor.sanitize(&with_match);
    let elapsed = started.elapsed();
    assert_eq!(
        sanitized.as_str(),
        format!("{near_miss}{REDACTED}"),
        "the complete spelling must still be removed ({} bytes in)",
        with_match.len()
    );
    assert!(
        elapsed.as_secs() < 30,
        "O(n^2) walk took {elapsed:?} on {} bytes; the bound does not hold",
        with_match.len()
    );

    // A repeated near miss multiplies the candidate positions, which is the part
    // of the cost that scales with input length.
    let repeated = near_miss.repeat(8);
    let started = std::time::Instant::now();
    let sanitized = redactor.sanitize(&repeated);
    let repeated_elapsed = started.elapsed();
    let elapsed = repeated_elapsed;
    assert_eq!(
        sanitized.as_str(),
        repeated,
        "repeated near miss must survive unchanged ({} bytes in)",
        repeated.len()
    );
    assert!(
        elapsed.as_secs() < 30,
        "O(input_len * n^2) walk took {elapsed:?} on {} bytes",
        repeated.len()
    );
    // The measured numbers, so the bound is evidence in the log rather than a claim.
    println!(
        "AF-01 resource bound: secret_len={} repeated_in={} bytes repeated_elapsed={:?}",
        ambiguous_secret.len(),
        repeated.len(),
        repeated_elapsed
    );

    // The decisive control: doubling the ambiguous secret must roughly quadruple
    // the work, because the bound is O(n^2) per candidate. An exponential
    // spelling walk would instead double its cost 2^n-fold, and a linear one
    // would merely double it. Timing is only used for the shape check here, so
    // the assertion stays on correctness and a generous absolute ceiling.
    let mut scaling = String::new();
    for percent_bytes in [16usize, 32] {
        let scaled_secret = format!("{}tail", "%".repeat(percent_bytes));
        let scaled = Redactor::new([scaled_secret]);
        let input = near_miss.clone();
        let started = std::time::Instant::now();
        let out = scaled.sanitize(&input);
        let step = started.elapsed();
        assert_eq!(out.as_str(), input, "n={percent_bytes} must not match");
        scaling.push_str(&format!(" n={percent_bytes}->{step:?}"));
        assert!(
            step.as_secs() < 30,
            "n={percent_bytes} took {step:?}; the O(n^2) bound does not hold"
        );
    }
    println!("AF-01 scaling (same input, ambiguous secret doubled):{scaling}");

    // Boundary conditions around the ambiguous secret.
    assert_eq!(redactor.sanitize("%").as_str(), "%", "bare percent");
    assert_eq!(redactor.sanitize("").as_str(), "", "empty input");
    assert_eq!(
        redactor.sanitize("%".repeat(47)).as_str(),
        "%".repeat(47),
        "one byte short"
    );
}
