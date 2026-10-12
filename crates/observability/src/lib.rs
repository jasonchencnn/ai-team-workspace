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
/// Registered secrets are removed in plaintext, once-percent-encoded, hexadecimal,
/// and standard/URL-safe base64 forms. Sensitive field names are independently
/// redacted by [`JsonSubscriber`], even when their values were not registered.
///
/// Under a single percent-encoding pass every registered byte may be written
/// literally or as `%XX`, so a partly escaped and a fully escaped spelling both
/// match. Only the hex digits inside an escape are case-insensitive (`%2d` and
/// `%2D` are the same byte) while a byte written literally must equal the
/// registered byte, so `Audit` is not `audit` and `audit%2donly%2faab` stays
/// untouched. Hexadecimal spellings are compared digit by digit, which accepts
/// every per-digit case mix without enumerating spellings and without rewriting
/// text that is not the registered secret.
///
/// Matching is bounded: candidates are the occurrences of at most two anchor bytes
/// and never start past `input.len() - min_len`, so each position is examined a
/// constant number of times. Decoding is one pass only; a `%25` sequence is an
/// ordinary byte here, because a multi-pass decoding policy is not decided.
#[derive(Clone, Default)]
pub struct Redactor {
    dangerous_forms: Arc<Vec<RedactionForm>>,
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
            .filter(|form| !form.is_empty())
            .collect::<Vec<_>>();
        dangerous_forms.sort_by_key(|form| std::cmp::Reverse(form.match_len()));
        dangerous_forms.dedup();
        Self {
            dangerous_forms: Arc::new(dangerous_forms),
        }
    }

    /// Sanitizes arbitrary text using the registered policy.
    pub fn sanitize(&self, input: impl AsRef<str>) -> SafeText {
        let mut output = input.as_ref().to_owned();
        for dangerous in self.dangerous_forms.iter() {
            output = match dangerous {
                RedactionForm::Exact(value) => output.replace(value.as_str(), REDACTED),
                RedactionForm::Percent(form) => form.replace_in(&output),
                RedactionForm::Hex(form) => form.replace_in(&output),
            };
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

/// One registered dangerous form together with how it is matched.
#[derive(Clone, Debug, PartialEq, Eq)]
enum RedactionForm {
    /// An exact substring, replaced like `str::replace` keeps doing.
    Exact(String),
    /// The once-percent-encoded form of one secret, matched by [`PercentForm`].
    Percent(PercentForm),
    /// The hexadecimal form of one secret, matched digit by digit by [`HexForm`].
    Hex(HexForm),
}

impl RedactionForm {
    /// The longest byte length this form can consume; longer forms are tried first.
    fn match_len(&self) -> usize {
        match self {
            Self::Exact(value) => value.len(),
            Self::Percent(form) => form.max_len,
            Self::Hex(form) => form.digits.len(),
        }
    }

    fn is_empty(&self) -> bool {
        match self {
            Self::Exact(value) => value.is_empty(),
            Self::Percent(form) => form.min_len == 0,
            Self::Hex(form) => form.digits.is_empty(),
        }
    }
}

/// Bounded matcher for the once-percent-encoded form of one registered secret.
///
/// Every registered byte may be written literally or as `%XX`, so a partly escaped
/// and a fully escaped spelling both match. Only the two hex digits inside an
/// escape are case-insensitive; a literal byte must equal the registered byte, so
/// `Audit` and `audit` stay distinct while an escaped byte still decodes to the
/// registered case. Decoding is never recursive: `%25` is one ordinary byte here.
///
/// A registered `%` has two legal spellings that start with the *same* input
/// byte, so the matcher cannot commit to one of them greedily; it carries the set
/// of reachable positions instead. The reachable set after `i` matched bytes is
/// indexed by `escaped ∈ 0..=i`, so layer `i` costs `O(i)` and all `n` layers
/// cost `O(secret_len²)` time in `O(secret_len)` space per candidate position.
/// That is the explicit bound this matcher relies on: every reachable state is
/// inspected once, so neither an input full of `%` nor a long secret with many
/// `%` bytes can make the work exponential. No spelling is enumerated, no input
/// byte is folded to another case, and the candidate scan stops at
/// `input.len() - min_len`.
#[derive(Clone, Debug, PartialEq, Eq)]
struct PercentForm {
    /// The registered bytes; each may appear literally or as `%XX`.
    bytes: Vec<u8>,
    /// Bytes a match consumes when no registered byte is escaped.
    min_len: usize,
    /// Bytes a match consumes when every registered byte is escaped.
    max_len: usize,
    /// Bytes that may start a match: the first registered byte, or `%`.
    anchors: Vec<u8>,
}

impl PercentForm {
    fn new(secret: &[u8]) -> Option<Self> {
        let first = *secret.first()?;
        let mut anchors = vec![first];
        if first != b'%' {
            anchors.push(b'%');
        }
        Some(Self {
            bytes: secret.to_vec(),
            min_len: secret.len(),
            max_len: secret.len() * 3,
            anchors,
        })
    }

    /// Returns the matched byte length when the pattern matches at `start`.
    ///
    /// The length depends on how many registered bytes were escaped, so callers
    /// splice with this value and never with `max_len`.
    ///
    /// A registered `%` may be written literally or as `%25`, and both spellings
    /// begin with the same input byte, so neither branch can be tried first and
    /// committed to. Every reachable position is therefore carried forward.
    ///
    /// After the first `i` registered bytes are matched, a spelling that escaped
    /// `e` of them has consumed exactly `i + 2e` input bytes, so the reachable set
    /// at layer `i` is the small index set `0..=i` and holds at most `i + 1`
    /// positions. Layer `i` therefore costs `O(i)` and the `n` layers cost
    /// `O(n²)` time in `O(n)` space — the explicit resource bound of this matcher.
    /// Every state is inspected exactly once per layer, so a secret with many `%`
    /// bytes and an input full of escape anchors cannot grow the work
    /// exponentially, and no backtracking beyond one layer is ever needed.
    ///
    /// Only the two hex digits inside an escape are case-insensitive and only a
    /// literal byte must equal the registered byte, so `Audit` is never `audit`
    /// here. Decoding stays single-pass: `%25` consumes three bytes and yields one.
    ///
    /// The longest reachable match is returned, which is the safe splice point for
    /// a redaction boundary: it never leaves a live spelling of the secret behind.
    fn match_at(&self, input: &[u8], start: usize, reachable: &mut Reachability) -> Option<usize> {
        let total = self.bytes.len();
        reachable.begin();
        for (index, byte) in self.bytes.iter().enumerate() {
            let mut wrote = false;
            for layer_index in 0..reachable.len() {
                let escaped = reachable.at(layer_index);
                let cursor = start + index + (escaped << 1);
                if input.get(cursor) == Some(byte) {
                    reachable.mark(escaped);
                    wrote = true;
                }
                if input.get(cursor) == Some(&b'%')
                    && let Some(high) = input.get(cursor + 1).copied().and_then(hex_digit_value)
                    && let Some(low) = input.get(cursor + 2).copied().and_then(hex_digit_value)
                    && (high << 4) | low == *byte
                {
                    reachable.mark(escaped + 1);
                    wrote = true;
                }
            }
            if !wrote {
                return None;
            }
            reachable.commit();
        }
        reachable.longest(total)
    }

    /// Replaces every non-overlapping match with [`REDACTED`], left to right.
    fn replace_in(&self, input: &str) -> String {
        let mut reachable = Reachability::default();
        replace_bounded_matches(input, &self.anchors, self.min_len, |start| {
            self.match_at(input.as_bytes(), start, &mut reachable)
        })
    }
}

/// The reachable escape counts of one [`PercentForm::match_at`] layer.
///
/// Only states that are actually live are stored, in ascending order, so a layer
/// costs `O(live)` rather than `O(i)`. That keeps the documented `O(n²)` worst
/// case — a layer can never hold more than `i + 1` states — while making the
/// common case, where a `%` byte has exactly one live spelling, linear overall.
///
/// Both buffers are allocated once per scan and reused for every candidate
/// position, so a long input performs no per-candidate allocation.
#[derive(Default)]
struct Reachability {
    current: Vec<usize>,
    next: Vec<usize>,
}

impl Reachability {
    /// Starts one candidate at escape count zero.
    fn begin(&mut self) {
        self.current.clear();
        self.next.clear();
        self.current.push(0);
    }

    fn len(&self) -> usize {
        self.current.len()
    }

    fn at(&self, layer_index: usize) -> usize {
        self.current[layer_index]
    }

    /// Records `escaped` for the next layer.
    ///
    /// Counts only ever arrive as `e` or `e + 1` while `current` is walked in
    /// ascending order, so `next` stays sorted and a duplicate only has to be
    /// dropped against the value just written.
    fn mark(&mut self, escaped: usize) {
        if self.next.last() == Some(&escaped) {
            return;
        }
        self.next.push(escaped);
    }

    /// Promotes the collected counts to the current layer.
    ///
    /// The retired layer must be cleared here: leaving its counts behind would let
    /// a stale escape count re-enter the next layer, which both matches the wrong
    /// bytes and makes the buffer grow without bound.
    fn commit(&mut self) {
        std::mem::swap(&mut self.current, &mut self.next);
        self.next.clear();
    }

    /// The longest spelling, i.e. the largest reachable escape count.
    fn longest(&self, total: usize) -> Option<usize> {
        self.current.last().map(|escaped| total + (escaped << 1))
    }
}

/// Bounded matcher for the hexadecimal form of one registered secret.
///
/// The form is exactly `2 × secret_len` digit positions long and every position is
/// compared by digit value, so each per-digit case mix matches exactly like the
/// all-lower or all-upper spelling without building the `2^n` spellings. A window
/// that is not the registered spelling is left byte-for-byte unchanged.
#[derive(Clone, Debug, PartialEq, Eq)]
struct HexForm {
    /// One nibble per position, from the high nibble of each registered byte.
    digits: Vec<u8>,
    /// Bytes that may start a match: the first digit's byte spellings.
    anchors: Vec<u8>,
}

impl HexForm {
    fn new(secret: &[u8]) -> Option<Self> {
        let mut digits = Vec::with_capacity(secret.len() * 2);
        for byte in secret {
            digits.push(byte >> 4);
            digits.push(byte & 0x0f);
        }
        let first = *digits.first()?;
        Some(Self {
            digits,
            anchors: hex_digit_spellings(first),
        })
    }

    /// Returns the matched byte length when every digit matches at `start`.
    fn match_at(&self, input: &[u8], start: usize) -> Option<usize> {
        let end = start.checked_add(self.digits.len())?;
        let window = input.get(start..end)?;
        let matched = self
            .digits
            .iter()
            .zip(window)
            .all(|(expected, actual)| hex_digit_value(*actual) == Some(*expected));
        matched.then_some(self.digits.len())
    }

    /// Replaces every non-overlapping match with [`REDACTED`], left to right.
    fn replace_in(&self, input: &str) -> String {
        replace_bounded_matches(input, &self.anchors, self.digits.len(), |start| {
            self.match_at(input.as_bytes(), start)
        })
    }
}

/// Replaces every non-overlapping match of a bounded matcher with [`REDACTED`].
///
/// One left-to-right pass: candidates are the occurrences of `anchors` and never
/// start past `input.len() - min_len`, the cursor never moves backwards, and every
/// position is therefore examined a constant number of times.
fn replace_bounded_matches(
    input: &str,
    anchors: &[u8],
    min_len: usize,
    mut match_at: impl FnMut(usize) -> Option<usize>,
) -> String {
    let bytes = input.as_bytes();
    let mut output = String::with_capacity(input.len());
    let mut copied = 0usize;
    let mut search = 0usize;
    while let Some((start, length)) =
        find_bounded_match(bytes, search, anchors, min_len, &mut match_at)
    {
        output.push_str(&input[copied..start]);
        output.push_str(REDACTED);
        copied = start + length;
        search = copied;
    }
    output.push_str(&input[copied..]);
    output
}

/// First match at or after `from`, or `None` once no match can remain.
fn find_bounded_match(
    input: &[u8],
    from: usize,
    anchors: &[u8],
    min_len: usize,
    match_at: &mut impl FnMut(usize) -> Option<usize>,
) -> Option<(usize, usize)> {
    let last_start = input.len().checked_sub(min_len)?;
    let mut search = from;
    while search <= last_start {
        let found = input[search..]
            .iter()
            .position(|byte| anchors.contains(byte))?;
        let candidate = search + found;
        if candidate > last_start {
            return None;
        }
        if is_char_boundary(input, candidate)
            && let Some(length) = match_at(candidate)
        {
            return Some((candidate, length));
        }
        search = candidate + 1;
    }
    None
}

fn hex_digit_value(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

/// The one or two bytes that spell a hex digit value.
fn hex_digit_spellings(value: u8) -> Vec<u8> {
    if value < 10 {
        vec![b'0' + value]
    } else {
        vec![b'a' + value - 10, b'A' + value - 10]
    }
}

fn is_char_boundary(input: &[u8], index: usize) -> bool {
    input.get(index).is_some_and(|byte| (byte & 0xc0) != 0x80)
}

fn derived_forms(secret: &str) -> Vec<RedactionForm> {
    let bytes = secret.as_bytes();
    let (Some(percent), Some(hex)) = (PercentForm::new(bytes), HexForm::new(bytes)) else {
        // An empty secret has no dangerous form: matching it would rewrite every
        // position of every string the redactor sees.
        return Vec::new();
    };
    let standard_base64 = base64_encode(
        bytes,
        b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/",
    );
    let url_base64 = base64_encode(
        bytes,
        b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_",
    );
    vec![
        RedactionForm::Percent(percent),
        RedactionForm::Hex(hex),
        RedactionForm::Exact(secret.to_owned()),
        RedactionForm::Exact(standard_base64.clone()),
        RedactionForm::Exact(standard_base64.trim_end_matches('=').to_owned()),
        RedactionForm::Exact(url_base64.clone()),
        RedactionForm::Exact(url_base64.trim_end_matches('=').to_owned()),
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
