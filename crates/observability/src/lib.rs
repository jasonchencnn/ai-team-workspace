#![forbid(unsafe_code)]

//! Structured, local tracing with correlation propagation and one redaction boundary.
//!
//! Callers create a [`Correlation`] span at a trusted request boundary and install a
//! [`JsonSubscriber`] at the host boundary. Secret-bearing values should use
//! [`Sensitive`], while arbitrary error and Event text must pass through
//! [`Redactor::redact_error`] or [`Redactor::redact_event`]. The subscriber applies
//! the same redactor to every string field as a final defense in depth.

use std::collections::{BTreeMap, HashMap};
use std::fmt;
use std::io::Write;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::ThreadId;

use serde::Serialize;
use serde_json::{Map, Number, Value};
use time::OffsetDateTime;
use tracing::field::{Field, Visit};
use tracing::span::{Attributes, Id, Record};
use tracing::subscriber::Interest;
use tracing::{Event, Metadata, Subscriber};

/// The only display, debug, serialization, and sanitization replacement for secrets.
pub const REDACTED: &str = "[REDACTED]";

/// A value whose safe `Display`, `Debug`, and serialization forms never reveal its contents.
#[derive(Clone, PartialEq, Eq)]
pub struct Sensitive<T>(T);

impl<T> Sensitive<T> {
    /// Wraps a secret-bearing value.
    pub const fn new(value: T) -> Self {
        Self(value)
    }

    /// Exposes the value only to an explicit, short-lived operation such as process injection.
    ///
    /// The callback must not retain, log, serialize, or include the value in an error.
    pub fn expose_secret<R>(&self, operation: impl FnOnce(&T) -> R) -> R {
        operation(&self.0)
    }
}

impl<T> fmt::Display for Sensitive<T> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(REDACTED)
    }
}

impl<T> fmt::Debug for Sensitive<T> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(REDACTED)
    }
}

impl<T> Serialize for Sensitive<T> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(REDACTED)
    }
}

/// Text that has passed through the shared redaction policy.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SafeText(String);

impl SafeText {
    /// Borrows the sanitized text.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for SafeText {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl Serialize for SafeText {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(&self.0)
    }
}

/// A cloneable redaction policy shared by logs, errors, and Event metadata.
///
/// Registered secrets are removed in plaintext, strict percent-encoded, hexadecimal,
/// and standard/URL-safe base64 forms. Sensitive field names are independently
/// redacted by [`JsonSubscriber`], even when their values were not registered.
#[derive(Clone, Default)]
pub struct Redactor {
    dangerous_forms: Arc<Vec<String>>,
}

impl Redactor {
    /// Builds a policy from secret values available at the trusted host boundary.
    pub fn new<I, S>(secrets: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        let mut dangerous_forms = secrets
            .into_iter()
            .flat_map(|secret| derived_forms(secret.as_ref()))
            .filter(|value| !value.is_empty())
            .collect::<Vec<_>>();
        dangerous_forms.sort_by_key(|value| std::cmp::Reverse(value.len()));
        dangerous_forms.dedup();
        Self {
            dangerous_forms: Arc::new(dangerous_forms),
        }
    }

    /// Sanitizes arbitrary text using the registered policy.
    pub fn sanitize(&self, input: impl AsRef<str>) -> SafeText {
        let mut output = input.as_ref().to_owned();
        for dangerous in self.dangerous_forms.iter() {
            output = output.replace(dangerous, REDACTED);
        }
        SafeText(output)
    }

    /// Produces the only supported free-text representation for error messages.
    pub fn redact_error(&self, input: impl AsRef<str>) -> SafeText {
        self.sanitize(input)
    }

    /// Produces the only supported free-text representation for Event metadata.
    pub fn redact_event(&self, input: impl AsRef<str>) -> SafeText {
        self.sanitize(input)
    }

    fn field_value(&self, field_name: &str, value: &str) -> Value {
        if is_sensitive_field(field_name) {
            Value::String(REDACTED.to_owned())
        } else {
            Value::String(self.sanitize(value).0)
        }
    }
}

/// Correlation identifiers propagated through a request's tracing span tree.
///
/// IDs must already be validated canonical protocol IDs by the caller's domain boundary.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Correlation {
    request_id: String,
    team_id: Option<String>,
    task_id: Option<String>,
    run_id: Option<String>,
}

impl Correlation {
    /// Starts correlation for one external request.
    pub fn new(request_id: impl Into<String>) -> Self {
        Self {
            request_id: request_id.into(),
            team_id: None,
            task_id: None,
            run_id: None,
        }
    }

    /// Adds the owning Team when the request has resolved one.
    pub fn with_team_id(mut self, team_id: impl Into<String>) -> Self {
        self.team_id = Some(team_id.into());
        self
    }

    /// Adds the related Task when the request has resolved one.
    pub fn with_task_id(mut self, task_id: impl Into<String>) -> Self {
        self.task_id = Some(task_id.into());
        self
    }

    /// Adds the related Run when the request has resolved one.
    pub fn with_run_id(mut self, run_id: impl Into<String>) -> Self {
        self.run_id = Some(run_id.into());
        self
    }

    /// Creates the parent span that propagates this correlation through nested work.
    pub fn span(&self) -> tracing::Span {
        let span = tracing::info_span!(
            "request",
            request_id = %self.request_id,
            team_id = tracing::field::Empty,
            task_id = tracing::field::Empty,
            run_id = tracing::field::Empty,
        );
        if let Some(team_id) = self.team_id.as_deref() {
            span.record("team_id", team_id);
        }
        if let Some(task_id) = self.task_id.as_deref() {
            span.record("task_id", task_id);
        }
        if let Some(run_id) = self.run_id.as_deref() {
            span.record("run_id", run_id);
        }
        span
    }
}

/// A controlled `tracing` subscriber that writes one sanitized JSON object per line.
pub struct JsonSubscriber<W> {
    writer: Mutex<W>,
    redactor: Redactor,
    timestamp_provider: Arc<dyn Fn() -> String + Send + Sync>,
    next_span_id: AtomicU64,
    spans: Mutex<HashMap<u64, SpanState>>,
    entered: Mutex<HashMap<ThreadId, Vec<u64>>>,
}

impl<W> JsonSubscriber<W>
where
    W: Write + Send + 'static,
{
    /// Creates a subscriber using UTC RFC 3339 timestamps with millisecond precision.
    pub fn new(writer: W, redactor: Redactor) -> Self {
        Self::with_timestamp_provider(writer, redactor, system_timestamp)
    }

    /// Creates a subscriber with an injected timestamp source for deterministic tests.
    pub fn with_timestamp_provider<F>(writer: W, redactor: Redactor, provider: F) -> Self
    where
        F: Fn() -> String + Send + Sync + 'static,
    {
        Self {
            writer: Mutex::new(writer),
            redactor,
            timestamp_provider: Arc::new(provider),
            next_span_id: AtomicU64::new(1),
            spans: Mutex::new(HashMap::new()),
            entered: Mutex::new(HashMap::new()),
        }
    }

    fn contextual_span_id(&self) -> Option<u64> {
        self.entered.lock().ok().and_then(|entered| {
            entered
                .get(&std::thread::current().id())
                .and_then(|ids| ids.last().copied())
        })
    }

    fn correlation_for(
        &self,
        parent_id: Option<u64>,
        event_fields: &mut BTreeMap<String, Value>,
    ) -> CorrelationValues {
        let mut correlation = CorrelationValues::default();
        if let Ok(spans) = self.spans.lock() {
            let mut cursor = parent_id;
            while let Some(id) = cursor {
                let Some(span) = spans.get(&id) else {
                    break;
                };
                correlation.fill_missing(&span.fields);
                cursor = span.parent;
            }
        }
        correlation.override_from_event(event_fields);
        correlation
    }

    fn write_event(
        &self,
        metadata: &Metadata<'_>,
        mut fields: BTreeMap<String, Value>,
        parent_id: Option<u64>,
    ) {
        let correlation = self.correlation_for(parent_id, &mut fields);
        let mut record = Map::new();
        record.insert(
            "timestamp".to_owned(),
            Value::String((self.timestamp_provider)()),
        );
        record.insert(
            "level".to_owned(),
            Value::String(metadata.level().to_string()),
        );
        record.insert(
            "module".to_owned(),
            Value::String(
                metadata
                    .module_path()
                    .unwrap_or_else(|| metadata.target())
                    .to_owned(),
            ),
        );
        correlation.insert_into(&mut record);
        record.insert(
            "fields".to_owned(),
            Value::Object(fields.into_iter().collect::<Map<String, Value>>()),
        );

        let Ok(mut writer) = self.writer.lock() else {
            return;
        };
        if serde_json::to_writer(&mut *writer, &Value::Object(record)).is_ok() {
            let _write_result = writer.write_all(b"\n");
        }
    }
}

impl<W> Subscriber for JsonSubscriber<W>
where
    W: Write + Send + 'static,
{
    fn enabled(&self, _metadata: &Metadata<'_>) -> bool {
        true
    }

    fn register_callsite(&self, _metadata: &'static Metadata<'static>) -> Interest {
        Interest::always()
    }

    fn new_span(&self, attributes: &Attributes<'_>) -> Id {
        let id = self.next_span_id.fetch_add(1, Ordering::Relaxed);
        let parent = attributes.parent().map(Id::into_u64).or_else(|| {
            attributes
                .is_contextual()
                .then(|| self.contextual_span_id())
                .flatten()
        });
        let mut visitor = FieldVisitor::new(&self.redactor);
        attributes.record(&mut visitor);
        if let Ok(mut spans) = self.spans.lock() {
            if let Some(parent_id) = parent
                && let Some(parent_state) = spans.get_mut(&parent_id)
            {
                parent_state.references = parent_state.references.saturating_add(1);
            }
            spans.insert(
                id,
                SpanState {
                    parent,
                    fields: visitor.fields,
                    references: 1,
                },
            );
        }
        Id::from_u64(id)
    }

    fn record(&self, span: &Id, values: &Record<'_>) {
        let mut visitor = FieldVisitor::new(&self.redactor);
        values.record(&mut visitor);
        if let Ok(mut spans) = self.spans.lock()
            && let Some(state) = spans.get_mut(&span.into_u64())
        {
            state.fields.extend(visitor.fields);
        }
    }

    fn record_follows_from(&self, _span: &Id, _follows: &Id) {}

    fn clone_span(&self, id: &Id) -> Id {
        if let Ok(mut spans) = self.spans.lock()
            && let Some(state) = spans.get_mut(&id.into_u64())
        {
            state.references = state.references.saturating_add(1);
        }
        id.clone()
    }

    fn event(&self, event: &Event<'_>) {
        let mut visitor = FieldVisitor::new(&self.redactor);
        event.record(&mut visitor);
        let parent = event.parent().map(Id::into_u64).or_else(|| {
            event
                .is_contextual()
                .then(|| self.contextual_span_id())
                .flatten()
        });
        self.write_event(event.metadata(), visitor.fields, parent);
    }

    fn enter(&self, span: &Id) {
        if let Ok(mut entered) = self.entered.lock() {
            entered
                .entry(std::thread::current().id())
                .or_default()
                .push(span.into_u64());
        }
    }

    fn exit(&self, span: &Id) {
        if let Ok(mut entered) = self.entered.lock() {
            let thread_id = std::thread::current().id();
            if let Some(ids) = entered.get_mut(&thread_id) {
                if let Some(position) = ids.iter().rposition(|id| *id == span.into_u64()) {
                    ids.remove(position);
                }
                if ids.is_empty() {
                    entered.remove(&thread_id);
                }
            }
        }
    }

    fn try_close(&self, id: Id) -> bool {
        let Ok(mut spans) = self.spans.lock() else {
            return false;
        };
        release_span_reference(&mut spans, id.into_u64())
    }
}

struct SpanState {
    parent: Option<u64>,
    fields: BTreeMap<String, Value>,
    references: usize,
}

fn release_span_reference(spans: &mut HashMap<u64, SpanState>, initial_id: u64) -> bool {
    let mut current_id = initial_id;
    let mut initial_removed = false;
    while let Some((references, parent)) = spans
        .get(&current_id)
        .map(|state| (state.references, state.parent))
    {
        if references > 1 {
            if let Some(state) = spans.get_mut(&current_id) {
                state.references -= 1;
            }
            break;
        }
        spans.remove(&current_id);
        if current_id == initial_id {
            initial_removed = true;
        }
        let Some(parent_id) = parent else {
            break;
        };
        current_id = parent_id;
    }
    initial_removed
}

#[derive(Default)]
struct CorrelationValues {
    request_id: Option<Value>,
    team_id: Option<Value>,
    task_id: Option<Value>,
    run_id: Option<Value>,
}

impl CorrelationValues {
    fn fill_missing(&mut self, fields: &BTreeMap<String, Value>) {
        for (name, destination) in [
            ("request_id", &mut self.request_id),
            ("team_id", &mut self.team_id),
            ("task_id", &mut self.task_id),
            ("run_id", &mut self.run_id),
        ] {
            if destination.is_none()
                && let Some(value) = fields.get(name)
            {
                *destination = Some(value.clone());
            }
        }
    }

    fn override_from_event(&mut self, fields: &mut BTreeMap<String, Value>) {
        for (name, destination) in [
            ("request_id", &mut self.request_id),
            ("team_id", &mut self.team_id),
            ("task_id", &mut self.task_id),
            ("run_id", &mut self.run_id),
        ] {
            if let Some(value) = fields.remove(name) {
                *destination = Some(value);
            }
        }
    }

    fn insert_into(self, record: &mut Map<String, Value>) {
        for (name, value) in [
            ("request_id", self.request_id),
            ("team_id", self.team_id),
            ("task_id", self.task_id),
            ("run_id", self.run_id),
        ] {
            record.insert(name.to_owned(), value.unwrap_or(Value::Null));
        }
    }
}

struct FieldVisitor<'a> {
    redactor: &'a Redactor,
    fields: BTreeMap<String, Value>,
}

impl<'a> FieldVisitor<'a> {
    fn new(redactor: &'a Redactor) -> Self {
        Self {
            redactor,
            fields: BTreeMap::new(),
        }
    }

    fn insert_number(&mut self, field: &Field, value: Number) {
        let output = if is_sensitive_field(field.name()) {
            Value::String(REDACTED.to_owned())
        } else {
            Value::Number(value)
        };
        self.fields.insert(field.name().to_owned(), output);
    }
}

impl Visit for FieldVisitor<'_> {
    fn record_i64(&mut self, field: &Field, value: i64) {
        self.insert_number(field, Number::from(value));
    }

    fn record_u64(&mut self, field: &Field, value: u64) {
        self.insert_number(field, Number::from(value));
    }

    fn record_bool(&mut self, field: &Field, value: bool) {
        let output = if is_sensitive_field(field.name()) {
            Value::String(REDACTED.to_owned())
        } else {
            Value::Bool(value)
        };
        self.fields.insert(field.name().to_owned(), output);
    }

    fn record_str(&mut self, field: &Field, value: &str) {
        self.fields.insert(
            field.name().to_owned(),
            self.redactor.field_value(field.name(), value),
        );
    }

    fn record_debug(&mut self, field: &Field, value: &dyn fmt::Debug) {
        self.fields.insert(
            field.name().to_owned(),
            self.redactor
                .field_value(field.name(), &format!("{value:?}")),
        );
    }
}

fn is_sensitive_field(name: &str) -> bool {
    let normalized = name.to_ascii_lowercase();
    [
        "secret",
        "credential",
        "token",
        "cookie",
        "password",
        "authorization",
        "api_key",
        "apikey",
    ]
    .iter()
    .any(|marker| normalized.contains(marker))
}

fn derived_forms(secret: &str) -> Vec<String> {
    if secret.is_empty() {
        return Vec::new();
    }
    let bytes = secret.as_bytes();
    let hex = bytes
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    let percent = bytes
        .iter()
        .map(|byte| {
            if byte.is_ascii_alphanumeric() {
                char::from(*byte).to_string()
            } else {
                format!("%{byte:02X}")
            }
        })
        .collect::<String>();
    let standard_base64 = base64_encode(
        bytes,
        b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/",
    );
    let url_base64 = base64_encode(
        bytes,
        b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_",
    );
    vec![
        secret.to_owned(),
        hex.clone(),
        hex.to_ascii_uppercase(),
        percent.clone(),
        percent.to_ascii_lowercase(),
        standard_base64.clone(),
        standard_base64.trim_end_matches('=').to_owned(),
        url_base64.clone(),
        url_base64.trim_end_matches('=').to_owned(),
    ]
}

fn base64_encode(input: &[u8], alphabet: &[u8; 64]) -> String {
    let mut output = String::with_capacity(input.len().div_ceil(3) * 4);
    for chunk in input.chunks(3) {
        let first = chunk[0];
        let second = chunk.get(1).copied().unwrap_or(0);
        let third = chunk.get(2).copied().unwrap_or(0);
        output.push(char::from(alphabet[usize::from(first >> 2)]));
        output.push(char::from(
            alphabet[usize::from(((first & 0x03) << 4) | (second >> 4))],
        ));
        if chunk.len() > 1 {
            output.push(char::from(
                alphabet[usize::from(((second & 0x0f) << 2) | (third >> 6))],
            ));
        } else {
            output.push('=');
        }
        if chunk.len() > 2 {
            output.push(char::from(alphabet[usize::from(third & 0x3f)]));
        } else {
            output.push('=');
        }
    }
    output
}

fn system_timestamp() -> String {
    let now = OffsetDateTime::now_utc();
    format!(
        "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}.{:03}Z",
        now.year(),
        u8::from(now.month()),
        now.day(),
        now.hour(),
        now.minute(),
        now.second(),
        now.millisecond(),
    )
}
