//! Deny-by-default PII filter for the telemetry proxy (R-2).
//!
//! This is the security-load-bearing part of the proxy: GC is the trust
//! boundary between a browser (which the user controls) and the in-cluster OTel
//! collector.
//!
//! # What this filter bounds
//!
//! Attribute **keys** at every nesting level; value **shape** for every key; and
//! value **content** for exactly two keys:
//!
//! * [`KEY_CUSTODY_LABEL`] — one legal value ([`KEY_CUSTODY_OPERATOR`]), and an
//!   attribute carrying anything else is **dropped**.
//! * `org_id` — never trusted from the payload. It is **stamped** from the
//!   authenticated `UserClaims.org_id` wherever it appears, and inserted on every
//!   metric datapoint.
//!
//! The two rules use different verbs on purpose, and the principle that
//! separates them is: **GC may assert a value it has AUTHENTICATED; it may not
//! synthesize a value it merely ASSUMES.** `org_id` comes from a validated JWT,
//! so stamping it records who authenticated — and dropping it is not an option,
//! because it is the only tenant dimension a client series has. `key_custody` is
//! a deployment property absent from the request, so there is nothing to
//! authenticate; dropping costs nothing because the value is constant.
//!
//! It bounds neither the value domain nor the distinct-value COUNT of any other
//! key. The bounded-cardinality property of `reason`/`outcome`/`action`/`source`
//! comes from the SDK types in
//! `packages/sdk-core/src/media/setup/mediaMetrics.ts` and holds for an HONEST
//! client only: a patched client can inflate their distinct-value count, and the
//! collector's charset regex bounds bytes and charset, not count.
//!
//! Traversal is strictly fixed-depth over the structural message tree. The value
//! checks read a value's discriminant (and, for the two keys above, a short
//! string) but never descend into a nested `AnyValue`, so there is no
//! user-controlled recursion.
//!
//! # Two key tiers
//!
//! The base [`ALLOWLIST`] applies everywhere. Metric **datapoint** attributes
//! additionally admit [`MEDIA_DATAPOINT_EXTRA`] — the five keys the media
//! counters discriminate on. The tier is datapoint-only because those four
//! non-custody keys are GENERIC names: a future free-text `reason` (an error
//! message, say) riding a SPAN would leak, and the trace path has no collector
//! `keep_keys` backstop, while spans deliberately retain `meeting_id_hash`.
//!
//! After filtering, the caller re-encodes the mutated request with `prost` and
//! forwards ONLY the re-encoded bytes — the original body is never proxied.

use crate::observability::metrics;
use common::observability::labels::{KEY_CUSTODY_LABEL, KEY_CUSTODY_OPERATOR};
use opentelemetry_proto::tonic::collector::metrics::v1::ExportMetricsServiceRequest;
use opentelemetry_proto::tonic::collector::trace::v1::ExportTraceServiceRequest;
use opentelemetry_proto::tonic::common::v1::any_value::Value;
use opentelemetry_proto::tonic::common::v1::{AnyValue, KeyValue};
use opentelemetry_proto::tonic::metrics::v1::metric::Data;

/// The 12-key BASE attribute allowlist, applied at every nesting level of both
/// signals. Any attribute key not in this set — and not in
/// [`MEDIA_DATAPOINT_EXTRA`] at a metric datapoint — is dropped.
/// Deny-by-default: adding a new business attribute requires a change here
/// (intended friction).
pub const ALLOWLIST: [&str; 12] = [
    "client_version",
    "service.name",
    "service.version",
    // Retained on the TRACE path on purpose: it is what makes a single session
    // traceable. It never reaches a stored metric series — the collector's
    // metrics-path `keep_keys` omits it
    // (`infra/services/otel-collector/configmap.yaml`). That asymmetry is
    // deliberate; see the three-lists comment there before "unifying" anything.
    "meeting_id_hash",
    // Value is SERVER-STAMPED from the authenticated claims (see
    // [`stamp_org_id`]); the client-sent value — historically the org subdomain —
    // is discarded. The key stays allowlisted because the stamp is a value rule
    // on an allowed key, not an exception to the allowlist.
    "org_id",
    "dt.event",
    "dt.duration_ms",
    "dt.failure_stage",
    "dt.close_reason",
    "dt.mh_index_bucket",
    "http.status_code",
    "error.code",
];

/// The five keys additionally admitted on metric **datapoint** attributes only.
///
/// A DELTA, never a second full list: the union with [`ALLOWLIST`] is computed
/// in [`key_allowed`] and is deliberately never written down. A 17-key array
/// would be a straight copy of the base 12 and would silently diverge the first
/// time someone adds a 13th base key to one array and not the other.
///
/// Without these, `MCMediaMissingKeyMaterial` cannot match: its numerator
/// selects `reason=~"no_kek_for_generation|no_roster_entry"`, and `reason` was
/// being stripped here. Every labelled client media counter was corrupted the
/// same way, even with a single browser.
///
/// Bounded and identity-free on an honest client (see the module header for what
/// that does and does not promise): `key_custody` is the constant `operator`;
/// `reason`, `outcome`, `action` and `source` are bounded enums declared in
/// `packages/sdk-core/src/media/setup/mediaMetrics.ts` and
/// `packages/sdk-core/src/media/frame/rejectReason.ts`. None carries a
/// participant, meeting or stream identity. Deliberately NOT mirrored as a Rust
/// enum here: that would be a fourth, unguarded copy of a TypeScript-owned
/// vocabulary whose own SSoT is a Guarded Shared Area.
pub const MEDIA_DATAPOINT_EXTRA: [&str; 5] =
    ["reason", "outcome", "action", "source", KEY_CUSTODY_LABEL];

/// Which key tier applies at a given call site.
///
/// Never selected from [`AttrKind`]: exemplar `filtered_attributes` are reported
/// as `AttrKind::Datapoint` for metric purposes, so tiering on the kind would
/// silently open the five keys on exemplars.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Tier {
    /// [`ALLOWLIST`] only.
    Base,
    /// [`ALLOWLIST`] plus [`MEDIA_DATAPOINT_EXTRA`].
    MediaDatapoint,
}

/// How `org_id` is applied at a given call site.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Stamp {
    /// Overwrite an existing `org_id`, but never create one. Used everywhere that
    /// is not a metric datapoint: those levels do not become series labels
    /// (`resource_to_telemetry_conversion` is off), so inserting there would be
    /// GC inventing attributes rather than correcting them.
    OverwriteIfPresent,
    /// Overwrite if present, insert if absent. Metric datapoints only, so that
    /// every stored client series carries the authenticated tenant.
    SetOrInsert,
}

/// True if `key` may survive at a call site with this tier.
fn key_allowed(key: &str, tier: Tier) -> bool {
    ALLOWLIST.contains(&key)
        || (tier == Tier::MediaDatapoint && MEDIA_DATAPOINT_EXTRA.contains(&key))
}

/// Structural nesting level at which a drop occurred. Used as the bounded `kind`
/// label on `gc_telemetry_pii_attributes_dropped_total` — NEVER the dropped key
/// or value (which would be unbounded and would re-leak the stripped PII).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum AttrKind {
    Resource,
    Scope,
    Datapoint,
    Span,
    SpanEvent,
    SpanLink,
}

impl AttrKind {
    fn label(self) -> &'static str {
        match self {
            AttrKind::Resource => "resource",
            AttrKind::Scope => "scope",
            AttrKind::Datapoint => "datapoint",
            AttrKind::Span => "span",
            AttrKind::SpanEvent => "span_event",
            AttrKind::SpanLink => "span_link",
        }
    }
}

/// Aggregated drop counts by kind, accumulated across one payload then emitted
/// once per kind (so the metric is not incremented per-attribute).
#[derive(Default, Debug, PartialEq, Eq)]
pub struct DropCounts {
    resource: u64,
    scope: u64,
    datapoint: u64,
    span: u64,
    span_event: u64,
    span_link: u64,
}

impl DropCounts {
    fn add(&mut self, kind: AttrKind, n: u64) {
        match kind {
            AttrKind::Resource => self.resource += n,
            AttrKind::Scope => self.scope += n,
            AttrKind::Datapoint => self.datapoint += n,
            AttrKind::Span => self.span += n,
            AttrKind::SpanEvent => self.span_event += n,
            AttrKind::SpanLink => self.span_link += n,
        }
    }

    /// Total dropped across all levels.
    pub fn total(&self) -> u64 {
        self.resource + self.scope + self.datapoint + self.span + self.span_event + self.span_link
    }

    /// Emit the per-kind drop counters (no-op for any kind with zero drops).
    pub fn emit(&self) {
        metrics::record_telemetry_pii_dropped(AttrKind::Resource.label(), self.resource);
        metrics::record_telemetry_pii_dropped(AttrKind::Scope.label(), self.scope);
        metrics::record_telemetry_pii_dropped(AttrKind::Datapoint.label(), self.datapoint);
        metrics::record_telemetry_pii_dropped(AttrKind::Span.label(), self.span);
        metrics::record_telemetry_pii_dropped(AttrKind::SpanEvent.label(), self.span_event);
        metrics::record_telemetry_pii_dropped(AttrKind::SpanLink.label(), self.span_link);
    }
}

/// Private core. Single source of truth for the allowlist check, the value-shape
/// check, the one value-content drop, and the `org_id` stamp.
///
/// Not called directly from the traversal: every call site goes through one of
/// the two wrappers below, which hard-code their own tier and stamp mode.
fn filter_attrs_with(
    attrs: &mut Vec<KeyValue>,
    kind: AttrKind,
    tier: Tier,
    stamp: Stamp,
    authenticated_org_id: &str,
    counts: &mut DropCounts,
) {
    let before = attrs.len();
    // Deny-by-default on key AND value shape. An attribute survives only if its
    // key is allowlisted for this tier AND its value is a scalar (or absent). A
    // non-scalar value — `ArrayValue` / `KvlistValue` (nested attributes) or
    // `BytesValue` (opaque blob) — can smuggle arbitrary PII under an
    // allowlisted key, so the whole attribute is dropped. We do NOT deep-recurse
    // into the value (that would be an unbounded, attacker-controlled-depth
    // traversal); dropping the non-scalar is both bounded and the safer
    // deny-by-default posture.
    //
    // `custody_value_ok` is the single value-CONTENT drop. It runs here, inside
    // the one retain, so it cannot be forgotten at a call site.
    attrs.retain(|kv| key_allowed(&kv.key, tier) && value_is_scalar(kv) && custody_value_ok(kv));
    let dropped = (before - attrs.len()) as u64;
    counts.add(kind, dropped);

    // AFTER the retain: a non-scalar `org_id` has already been dropped and
    // counted, so the stamp can only ever produce a scalar string.
    stamp_org_id(attrs, stamp, authenticated_org_id, kind, counts);
}

/// Base tier, overwrite-only. Resource, scope, exemplar `filtered_attributes`,
/// and every trace level.
fn filter_base_attrs(
    attrs: &mut Vec<KeyValue>,
    kind: AttrKind,
    authenticated_org_id: &str,
    counts: &mut DropCounts,
) {
    filter_attrs_with(
        attrs,
        kind,
        Tier::Base,
        Stamp::OverwriteIfPresent,
        authenticated_org_id,
        counts,
    );
}

/// Media-datapoint tier, set-or-insert. Metric datapoint `attributes` ONLY.
///
/// The two wrappers exist so that NO call site takes a tier or stamp argument:
/// `filter_metrics` has nine attribute sites, five of them `dp.attributes` and
/// four of them exemplar `filtered_attributes` sitting directly below a
/// near-identical datapoint call in each data-variant arm. A parameter chosen
/// per call site would hold at eight sites and silently not at the ninth, and
/// the ninth is the one review reads past. Making the wrong combination
/// unrepresentable beats getting it right nine times.
fn filter_datapoint_attrs(
    attrs: &mut Vec<KeyValue>,
    authenticated_org_id: &str,
    counts: &mut DropCounts,
) {
    filter_attrs_with(
        attrs,
        AttrKind::Datapoint,
        Tier::MediaDatapoint,
        Stamp::SetOrInsert,
        authenticated_org_id,
        counts,
    );
}

/// True unless this is a [`KEY_CUSTODY_LABEL`] attribute carrying something
/// other than [`KEY_CUSTODY_OPERATOR`].
///
/// ADR-0036 §4 permits exactly one value and §11 bars any end-to-end or
/// zero-trust claim, so a patched client sending `key_custody="end_to_end"`
/// would land a false custody claim in a series that dashboards and the catalog
/// read as fact. DROP rather than overwrite: GC filters, it does not author —
/// and unlike `org_id` there is nothing here GC has authenticated, so correcting
/// the value would be synthesizing one. The collector independently SETS the
/// constant on the metrics path, which is the structural half of the same rule.
fn custody_value_ok(kv: &KeyValue) -> bool {
    if kv.key != KEY_CUSTODY_LABEL {
        return true;
    }
    matches!(
        kv.value.as_ref().and_then(|v| v.value.as_ref()),
        Some(Value::StringValue(s)) if s == KEY_CUSTODY_OPERATOR
    )
}

/// Stamp `org_id` from the authenticated claim.
///
/// The payload's `org_id` is never read. The SDK sends the org SUBDOMAIN and the
/// claim is an org UUID — different identifier spaces — so a naive
/// `payload != claim` comparison would drop the attribute on every honest
/// client, which is why the rule is "stamp", never "compare".
///
/// An EMPTY claim never stamps: any present `org_id` is dropped and nothing is
/// inserted. Stamping an empty string would collapse every tenant into
/// `org_id=""` while the series stayed present — nothing would look broken. It
/// should be unreachable (AC builds the claim from a `Uuid`), which is exactly
/// why it gets a two-line guard and a WARN rather than a comment: the drop
/// counter alone cannot distinguish a server-side invariant violation from
/// ordinary client-driven drops, and masking one behind the other is how a real
/// failure goes silent. The request is NOT failed — a 500 would turn an AC-side
/// defect into a client-facing telemetry outage on a best-effort 202 endpoint.
///
/// Stamping is NOT a drop and is never counted as one: honest clients always
/// "mismatch" (they send the subdomain), so an overwrite counter would read
/// ~100% and signal nothing.
fn stamp_org_id(
    attrs: &mut Vec<KeyValue>,
    stamp: Stamp,
    authenticated_org_id: &str,
    kind: AttrKind,
    counts: &mut DropCounts,
) {
    if authenticated_org_id.is_empty() {
        // NO WARNING HERE. This runs once per attribute SET — every resource,
        // scope, datapoint, exemplar, span, event and link — so a warn at this
        // depth fires hundreds of times per request, exactly during the incident
        // where someone needs to read the logs. The once-per-request warning lives
        // at the entry points (`filter_metrics` / `filter_traces`).
        let before = attrs.len();
        attrs.retain(|kv| kv.key != ORG_ID_KEY);
        let dropped = (before - attrs.len()) as u64;
        if dropped > 0 {
            counts.add(kind, dropped);
        }
        return;
    }

    // REMOVE EVERY COPY, THEN WRITE AT MOST ONE. OTLP permits repeated keys on
    // the wire, and whether the payload repeats `org_id` is client-controlled; an
    // overwrite-in-place loop would forward N stamped copies, and repeated keys are
    // undefined downstream. The payload's value is never read, so how many copies
    // it sent is irrelevant — only WHETHER it sent one decides overwrite-only
    // levels. The removed copies are NOT counted as drops: that counter means
    // "attributes the filter refused", and stamping is not a refusal.
    let had = attrs.iter().any(|kv| kv.key == ORG_ID_KEY);
    attrs.retain(|kv| kv.key != ORG_ID_KEY);
    if had || stamp == Stamp::SetOrInsert {
        attrs.push(KeyValue {
            key: ORG_ID_KEY.to_string(),
            value: Some(AnyValue {
                value: Some(Value::StringValue(authenticated_org_id.to_string())),
            }),
        });
    }
}

/// Warn ONCE per request when the authenticated claim is empty.
///
/// Called at the two entry points only. An empty claim is a server-side
/// invariant violation (AC builds it from a `Uuid`), and the drop counter alone
/// cannot distinguish it from ordinary client-driven drops — so it gets a loud,
/// but bounded, signal. No attribute values are logged.
fn warn_if_empty_claim(authenticated_org_id: &str) {
    if authenticated_org_id.is_empty() {
        tracing::warn!(
            target: "gc.services.telemetry_filter",
            "authenticated org_id is empty; org_id attributes dropped and not stamped"
        );
    }
}

/// The tenant label key. One spelling, used by the allowlist and the stamp.
const ORG_ID_KEY: &str = "org_id";

/// True if the attribute's value is a scalar (`String`/`Bool`/`Int`/`Double`)
/// or absent. `ArrayValue`/`KvlistValue` (nested structures) and `BytesValue`
/// (opaque) are non-scalar and return false — they can carry un-filtered PII.
fn value_is_scalar(kv: &KeyValue) -> bool {
    match kv.value.as_ref().and_then(|v| v.value.as_ref()) {
        // Absent value (proto3 default) — nothing to smuggle.
        None => true,
        Some(Value::StringValue(_))
        | Some(Value::BoolValue(_))
        | Some(Value::IntValue(_))
        | Some(Value::DoubleValue(_)) => true,
        // Non-scalar / opaque — drop the attribute.
        Some(Value::ArrayValue(_)) | Some(Value::KvlistValue(_)) | Some(Value::BytesValue(_)) => {
            false
        }
    }
}

/// Filter a decoded OTLP metrics request in place, returning per-kind drop
/// counts. Walks resource attrs, scope attrs, and every datapoint's attributes
/// across all five metric data variants, plus exemplar `filtered_attributes`.
///
/// `authenticated_org_id` MUST come from validated `UserClaims`, never from the
/// request body. Read it per-request from the same `Extension<UserClaims>` the
/// rate limiter keys on — never cache it in `AppState`, a `OnceCell`, or
/// anything else outliving the request: a cached principal on a stamping path is
/// a cross-tenant bug with a long fuse.
pub fn filter_metrics(
    req: &mut ExportMetricsServiceRequest,
    authenticated_org_id: &str,
) -> DropCounts {
    warn_if_empty_claim(authenticated_org_id);
    let mut counts = DropCounts::default();

    for rm in &mut req.resource_metrics {
        if let Some(resource) = rm.resource.as_mut() {
            filter_base_attrs(
                &mut resource.attributes,
                AttrKind::Resource,
                authenticated_org_id,
                &mut counts,
            );
        }
        for sm in &mut rm.scope_metrics {
            if let Some(scope) = sm.scope.as_mut() {
                filter_base_attrs(
                    &mut scope.attributes,
                    AttrKind::Scope,
                    authenticated_org_id,
                    &mut counts,
                );
            }
            for metric in &mut sm.metrics {
                // Each data variant carries its own datapoint type; only
                // NumberDataPoint/Histogram/ExponentialHistogram have exemplars
                // (SummaryDataPoint does NOT).
                match metric.data.as_mut() {
                    Some(Data::Gauge(g)) => {
                        for dp in &mut g.data_points {
                            filter_datapoint_attrs(
                                &mut dp.attributes,
                                authenticated_org_id,
                                &mut counts,
                            );
                            for ex in &mut dp.exemplars {
                                // Exemplars get the BASE tier, deliberately: this
                                // call sits directly below the datapoint call
                                // above and is the site a skimming review merges
                                // with it. Widening the five generic keys onto
                                // exemplar attributes would put them on a path
                                // with no collector `keep_keys` backstop.
                                filter_base_attrs(
                                    &mut ex.filtered_attributes,
                                    AttrKind::Datapoint,
                                    authenticated_org_id,
                                    &mut counts,
                                );
                            }
                        }
                    }
                    Some(Data::Sum(s)) => {
                        for dp in &mut s.data_points {
                            filter_datapoint_attrs(
                                &mut dp.attributes,
                                authenticated_org_id,
                                &mut counts,
                            );
                            for ex in &mut dp.exemplars {
                                // Exemplars get the BASE tier, deliberately: this
                                // call sits directly below the datapoint call
                                // above and is the site a skimming review merges
                                // with it. Widening the five generic keys onto
                                // exemplar attributes would put them on a path
                                // with no collector `keep_keys` backstop.
                                filter_base_attrs(
                                    &mut ex.filtered_attributes,
                                    AttrKind::Datapoint,
                                    authenticated_org_id,
                                    &mut counts,
                                );
                            }
                        }
                    }
                    Some(Data::Histogram(h)) => {
                        for dp in &mut h.data_points {
                            filter_datapoint_attrs(
                                &mut dp.attributes,
                                authenticated_org_id,
                                &mut counts,
                            );
                            for ex in &mut dp.exemplars {
                                // Exemplars get the BASE tier, deliberately: this
                                // call sits directly below the datapoint call
                                // above and is the site a skimming review merges
                                // with it. Widening the five generic keys onto
                                // exemplar attributes would put them on a path
                                // with no collector `keep_keys` backstop.
                                filter_base_attrs(
                                    &mut ex.filtered_attributes,
                                    AttrKind::Datapoint,
                                    authenticated_org_id,
                                    &mut counts,
                                );
                            }
                        }
                    }
                    Some(Data::ExponentialHistogram(eh)) => {
                        for dp in &mut eh.data_points {
                            filter_datapoint_attrs(
                                &mut dp.attributes,
                                authenticated_org_id,
                                &mut counts,
                            );
                            for ex in &mut dp.exemplars {
                                // Exemplars get the BASE tier, deliberately: this
                                // call sits directly below the datapoint call
                                // above and is the site a skimming review merges
                                // with it. Widening the five generic keys onto
                                // exemplar attributes would put them on a path
                                // with no collector `keep_keys` backstop.
                                filter_base_attrs(
                                    &mut ex.filtered_attributes,
                                    AttrKind::Datapoint,
                                    authenticated_org_id,
                                    &mut counts,
                                );
                            }
                        }
                    }
                    Some(Data::Summary(sum)) => {
                        // SummaryDataPoint has no `exemplars` field — only attributes.
                        for dp in &mut sum.data_points {
                            filter_datapoint_attrs(
                                &mut dp.attributes,
                                authenticated_org_id,
                                &mut counts,
                            );
                        }
                    }
                    None => {}
                }
            }
        }
    }

    counts
}

/// Filter a decoded OTLP traces request in place, returning per-kind drop
/// counts. Walks resource attrs, scope attrs, span attrs, span event attrs, and
/// span link attrs. `Span.status` has no attributes field in OTLP (it is
/// `{ message, code }`), so there is nothing to filter there — that is a
/// documented non-gap, not a skipped level.
///
/// Every level here is BASE tier and overwrite-only: the five media
/// discriminators do not pass on spans (they are generic names with no collector
/// backstop on the trace path), and `org_id` is corrected where present but never
/// invented — a span that carried none still carries none. Same
/// `authenticated_org_id` contract as [`filter_metrics`].
pub fn filter_traces(
    req: &mut ExportTraceServiceRequest,
    authenticated_org_id: &str,
) -> DropCounts {
    warn_if_empty_claim(authenticated_org_id);
    let mut counts = DropCounts::default();

    for rs in &mut req.resource_spans {
        if let Some(resource) = rs.resource.as_mut() {
            filter_base_attrs(
                &mut resource.attributes,
                AttrKind::Resource,
                authenticated_org_id,
                &mut counts,
            );
        }
        for ss in &mut rs.scope_spans {
            if let Some(scope) = ss.scope.as_mut() {
                filter_base_attrs(
                    &mut scope.attributes,
                    AttrKind::Scope,
                    authenticated_org_id,
                    &mut counts,
                );
            }
            for span in &mut ss.spans {
                filter_base_attrs(
                    &mut span.attributes,
                    AttrKind::Span,
                    authenticated_org_id,
                    &mut counts,
                );
                // Span.status { message, code } has no attributes — nothing to filter.
                for event in &mut span.events {
                    filter_base_attrs(
                        &mut event.attributes,
                        AttrKind::SpanEvent,
                        authenticated_org_id,
                        &mut counts,
                    );
                }
                for link in &mut span.links {
                    filter_base_attrs(
                        &mut link.attributes,
                        AttrKind::SpanLink,
                        authenticated_org_id,
                        &mut counts,
                    );
                }
            }
        }
    }

    counts
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use opentelemetry_proto::tonic::common::v1::InstrumentationScope;
    use opentelemetry_proto::tonic::metrics::v1::{
        Exemplar, ExponentialHistogram, ExponentialHistogramDataPoint, Gauge, Histogram,
        HistogramDataPoint, Metric, NumberDataPoint, ResourceMetrics, ScopeMetrics, Sum, Summary,
        SummaryDataPoint,
    };
    use opentelemetry_proto::tonic::resource::v1::Resource;
    use opentelemetry_proto::tonic::trace::v1::{
        span::{Event, Link},
        ResourceSpans, ScopeSpans, Span,
    };

    /// Stands in for a validated `UserClaims.org_id`: an org UUID, which is what
    /// AC actually puts in the claim (`user.org_id.to_string()`). Deliberately
    /// NOT the subdomain the SDK sends — the whole point of the stamp is that
    /// those are different identifier spaces.
    const TEST_ORG: &str = "0f8fad5b-d9cb-469f-a165-70867728950e";

    /// What an honest browser actually sends as `org_id`: the org SUBDOMAIN
    /// (`packages/sdk-core/src/session/MeetingSession.ts`). A test that used the
    /// claim value on both sides would pass for a filter that compared them, and
    /// would therefore prove nothing about the real input.
    const SDK_SENT_ORG: &str = "acme";

    fn string_attr(key: &str, val: &str) -> KeyValue {
        KeyValue {
            key: key.to_string(),
            value: Some(AnyValue {
                value: Some(Value::StringValue(val.to_string())),
            }),
        }
    }

    /// The `(key, value)` pairs of an attribute list. Key-only assertions cannot
    /// see a filter that rewrites or cross-assigns VALUES, which is the hole that
    /// let two rejected `org_id` designs pass the whole existing suite green.
    fn pairs(attrs: &[KeyValue]) -> Vec<(&str, String)> {
        attrs
            .iter()
            .map(|kv| {
                let v = match kv.value.as_ref().and_then(|v| v.value.as_ref()) {
                    Some(Value::StringValue(s)) => s.clone(),
                    other => format!("{other:?}"),
                };
                (kv.key.as_str(), v)
            })
            .collect()
    }

    fn sum_req_with(attrs: Vec<KeyValue>) -> ExportMetricsServiceRequest {
        ExportMetricsServiceRequest {
            resource_metrics: vec![ResourceMetrics {
                resource: None,
                scope_metrics: vec![ScopeMetrics {
                    scope: None,
                    metrics: vec![Metric {
                        data: Some(Data::Sum(Sum {
                            data_points: vec![NumberDataPoint {
                                attributes: attrs,
                                ..Default::default()
                            }],
                            ..Default::default()
                        })),
                        ..Default::default()
                    }],
                    schema_url: String::new(),
                }],
                schema_url: String::new(),
            }],
        }
    }

    fn sum_dp_attrs(req: &ExportMetricsServiceRequest) -> &Vec<KeyValue> {
        let Some(Data::Sum(sum)) = req.resource_metrics[0].scope_metrics[0].metrics[0]
            .data
            .as_ref()
        else {
            unreachable!("payload was built with a Sum")
        };
        &sum.data_points[0].attributes
    }

    /// One allowlisted + one disallowed key.
    fn mixed_attrs() -> Vec<KeyValue> {
        vec![
            KeyValue {
                key: "org_id".to_string(),
                value: None,
            },
            KeyValue {
                key: "user_email".to_string(), // not allowlisted → dropped
                value: None,
            },
        ]
    }

    fn keys(attrs: &[KeyValue]) -> Vec<&str> {
        attrs.iter().map(|kv| kv.key.as_str()).collect()
    }

    #[test]
    fn allowlist_has_exactly_12_keys() {
        assert_eq!(ALLOWLIST.len(), 12);
    }

    #[test]
    fn filter_attrs_drops_only_disallowed_and_counts() {
        let mut attrs = mixed_attrs();
        let mut counts = DropCounts::default();
        filter_base_attrs(&mut attrs, AttrKind::Resource, TEST_ORG, &mut counts);

        assert_eq!(keys(&attrs), vec!["org_id"]);
        assert_eq!(counts.resource, 1);
        assert_eq!(counts.total(), 1);
    }

    #[test]
    fn filter_attrs_preserves_all_allowlisted() {
        let mut attrs: Vec<KeyValue> = ALLOWLIST
            .iter()
            .map(|k| KeyValue {
                key: (*k).to_string(),
                value: None,
            })
            .collect();
        let mut counts = DropCounts::default();
        filter_base_attrs(&mut attrs, AttrKind::Span, TEST_ORG, &mut counts);

        assert_eq!(attrs.len(), 12);
        assert_eq!(counts.total(), 0);
    }

    // ---- Value smuggling defense (CRUX-A) -----------------------------------
    // An allowlisted KEY whose VALUE is a non-scalar (nested kvlist / array /
    // opaque bytes) can hide arbitrary PII. The filter must drop the whole
    // attribute in that case — deny-by-default on value shape, not just key.

    use opentelemetry_proto::tonic::common::v1::{AnyValue, ArrayValue, KeyValueList};

    fn allowlisted(key: &str, value: Option<Value>) -> KeyValue {
        KeyValue {
            key: key.to_string(),
            value: value.map(|v| AnyValue { value: Some(v) }),
        }
    }

    #[test]
    fn scalar_values_on_allowlisted_keys_survive() {
        let mut attrs = vec![
            allowlisted("org_id", Some(Value::StringValue("acme".into()))),
            allowlisted("dt.duration_ms", Some(Value::IntValue(42))),
            allowlisted("http.status_code", Some(Value::IntValue(200))),
            allowlisted("dt.event", Some(Value::BoolValue(true))),
            allowlisted("service.version", Some(Value::DoubleValue(1.5))),
            allowlisted("client_version", None), // absent value also fine
        ];
        let mut counts = DropCounts::default();
        filter_base_attrs(&mut attrs, AttrKind::Resource, TEST_ORG, &mut counts);

        assert_eq!(attrs.len(), 6, "all scalar/none allowlisted attrs survive");
        assert_eq!(counts.total(), 0);
    }

    #[test]
    fn kvlist_value_on_allowlisted_key_is_dropped() {
        // org_id is allowlisted, but its value is a nested kvlist carrying PII.
        let nested = Value::KvlistValue(KeyValueList {
            values: vec![allowlisted_raw("secret_email", "victim@example.com")],
        });
        let mut attrs = vec![
            allowlisted("org_id", Some(nested)),
            allowlisted("client_version", Some(Value::StringValue("1.0".into()))),
        ];
        let mut counts = DropCounts::default();
        filter_base_attrs(&mut attrs, AttrKind::Resource, TEST_ORG, &mut counts);

        assert_eq!(
            keys(&attrs),
            vec!["client_version"],
            "allowlisted key with kvlist value must be dropped (PII smuggling)"
        );
        assert_eq!(counts.resource, 1);
    }

    #[test]
    fn array_value_on_allowlisted_key_is_dropped() {
        let arr = Value::ArrayValue(ArrayValue {
            values: vec![AnyValue {
                value: Some(Value::StringValue("192.168.1.1".into())),
            }],
        });
        let mut attrs = vec![allowlisted("meeting_id_hash", Some(arr))];
        let mut counts = DropCounts::default();
        filter_base_attrs(&mut attrs, AttrKind::Span, TEST_ORG, &mut counts);

        assert!(attrs.is_empty(), "array value must be dropped");
        assert_eq!(counts.span, 1);
    }

    #[test]
    fn bytes_value_on_allowlisted_key_is_dropped() {
        let mut attrs = vec![allowlisted(
            "error.code",
            Some(Value::BytesValue(vec![0xde, 0xad, 0xbe, 0xef])),
        )];
        let mut counts = DropCounts::default();
        filter_base_attrs(&mut attrs, AttrKind::SpanEvent, TEST_ORG, &mut counts);

        assert!(attrs.is_empty(), "opaque bytes value must be dropped");
        assert_eq!(counts.span_event, 1);
    }

    /// Helper: a KeyValue with a string value (for building nested kvlists).
    fn allowlisted_raw(key: &str, val: &str) -> KeyValue {
        KeyValue {
            key: key.to_string(),
            value: Some(AnyValue {
                value: Some(Value::StringValue(val.to_string())),
            }),
        }
    }

    // ---- Metrics: drop at every level, per-kind adjacency --------------------

    fn number_dp() -> NumberDataPoint {
        NumberDataPoint {
            attributes: mixed_attrs(),
            exemplars: vec![Exemplar {
                filtered_attributes: mixed_attrs(),
                ..Default::default()
            }],
            ..Default::default()
        }
    }

    fn metrics_req_with_gauge() -> ExportMetricsServiceRequest {
        ExportMetricsServiceRequest {
            resource_metrics: vec![ResourceMetrics {
                resource: Some(Resource {
                    attributes: mixed_attrs(),
                    dropped_attributes_count: 0,
                }),
                scope_metrics: vec![ScopeMetrics {
                    scope: Some(InstrumentationScope {
                        attributes: mixed_attrs(),
                        ..Default::default()
                    }),
                    metrics: vec![Metric {
                        data: Some(Data::Gauge(Gauge {
                            data_points: vec![number_dp()],
                        })),
                        ..Default::default()
                    }],
                    schema_url: String::new(),
                }],
                schema_url: String::new(),
            }],
        }
    }

    #[test]
    fn filter_metrics_drops_at_resource_scope_datapoint_and_exemplar() {
        let mut req = metrics_req_with_gauge();
        let counts = filter_metrics(&mut req, TEST_ORG);

        // resource (1) + scope (1) + datapoint attrs (1) + exemplar filtered (1, kind=datapoint)
        assert_eq!(counts.resource, 1);
        assert_eq!(counts.scope, 1);
        assert_eq!(counts.datapoint, 2); // datapoint attrs + exemplar filtered
        assert_eq!(counts.total(), 4);

        let rm = &req.resource_metrics[0];
        assert_eq!(
            keys(&rm.resource.as_ref().unwrap().attributes),
            vec!["org_id"]
        );
        let sm = &rm.scope_metrics[0];
        assert_eq!(keys(&sm.scope.as_ref().unwrap().attributes), vec!["org_id"]);
        let Some(Data::Gauge(g)) = sm.metrics[0].data.as_ref() else {
            unreachable!("payload was built with a Gauge")
        };
        assert_eq!(keys(&g.data_points[0].attributes), vec!["org_id"]);
        assert_eq!(
            keys(&g.data_points[0].exemplars[0].filtered_attributes),
            vec!["org_id"]
        );
    }

    #[test]
    fn filter_metrics_covers_all_five_data_variants() {
        // Build one metric of each variant, each with one mixed-attr datapoint.
        let variants: Vec<Data> = vec![
            Data::Gauge(Gauge {
                data_points: vec![number_dp()],
            }),
            Data::Sum(Sum {
                data_points: vec![number_dp()],
                ..Default::default()
            }),
            Data::Histogram(Histogram {
                data_points: vec![HistogramDataPoint {
                    attributes: mixed_attrs(),
                    exemplars: vec![Exemplar {
                        filtered_attributes: mixed_attrs(),
                        ..Default::default()
                    }],
                    ..Default::default()
                }],
                ..Default::default()
            }),
            Data::ExponentialHistogram(ExponentialHistogram {
                data_points: vec![ExponentialHistogramDataPoint {
                    attributes: mixed_attrs(),
                    exemplars: vec![Exemplar {
                        filtered_attributes: mixed_attrs(),
                        ..Default::default()
                    }],
                    ..Default::default()
                }],
                ..Default::default()
            }),
            Data::Summary(Summary {
                data_points: vec![SummaryDataPoint {
                    attributes: mixed_attrs(),
                    ..Default::default()
                }],
            }),
        ];

        for data in variants {
            let mut req = ExportMetricsServiceRequest {
                resource_metrics: vec![ResourceMetrics {
                    resource: None,
                    scope_metrics: vec![ScopeMetrics {
                        scope: None,
                        metrics: vec![Metric {
                            data: Some(data),
                            ..Default::default()
                        }],
                        schema_url: String::new(),
                    }],
                    schema_url: String::new(),
                }],
            };
            let counts = filter_metrics(&mut req, TEST_ORG);
            // Every variant drops at least its datapoint attr; only the four
            // exemplar-bearing variants drop the extra exemplar attr.
            assert!(
                counts.datapoint >= 1,
                "variant did not filter datapoint attrs"
            );
            assert_eq!(counts.resource, 0);
            assert_eq!(counts.scope, 0);
        }
    }

    // ---- Traces: drop at span / event / link --------------------------------

    #[test]
    fn filter_traces_drops_at_resource_scope_span_event_link() {
        let mut req = ExportTraceServiceRequest {
            resource_spans: vec![ResourceSpans {
                resource: Some(Resource {
                    attributes: mixed_attrs(),
                    dropped_attributes_count: 0,
                }),
                scope_spans: vec![ScopeSpans {
                    scope: Some(InstrumentationScope {
                        attributes: mixed_attrs(),
                        ..Default::default()
                    }),
                    spans: vec![Span {
                        attributes: mixed_attrs(),
                        events: vec![Event {
                            attributes: mixed_attrs(),
                            ..Default::default()
                        }],
                        links: vec![Link {
                            attributes: mixed_attrs(),
                            ..Default::default()
                        }],
                        ..Default::default()
                    }],
                    schema_url: String::new(),
                }],
                schema_url: String::new(),
            }],
        };

        let counts = filter_traces(&mut req, TEST_ORG);
        assert_eq!(counts.resource, 1);
        assert_eq!(counts.scope, 1);
        assert_eq!(counts.span, 1);
        assert_eq!(counts.span_event, 1);
        assert_eq!(counts.span_link, 1);
        assert_eq!(counts.total(), 5);

        let span = &req.resource_spans[0].scope_spans[0].spans[0];
        assert_eq!(keys(&span.attributes), vec!["org_id"]);
        assert_eq!(keys(&span.events[0].attributes), vec!["org_id"]);
        assert_eq!(keys(&span.links[0].attributes), vec!["org_id"]);
    }

    // ---- Per-kind adjacency: a drop at exactly one level moves only its label

    #[test]
    fn per_kind_adjacency_span_event_not_mislabeled_as_span() {
        // Only the span event has a disallowed key; span attrs are clean.
        let mut req = ExportTraceServiceRequest {
            resource_spans: vec![ResourceSpans {
                resource: None,
                scope_spans: vec![ScopeSpans {
                    scope: None,
                    spans: vec![Span {
                        attributes: vec![KeyValue {
                            key: "org_id".to_string(),
                            value: None,
                        }],
                        events: vec![Event {
                            attributes: mixed_attrs(),
                            ..Default::default()
                        }],
                        ..Default::default()
                    }],
                    schema_url: String::new(),
                }],
                schema_url: String::new(),
            }],
        };

        let counts = filter_traces(&mut req, TEST_ORG);
        assert_eq!(counts.span, 0, "clean span attrs must not be counted");
        assert_eq!(
            counts.span_event, 1,
            "event drop must be labeled span_event"
        );
        assert_eq!(counts.span_link, 0);
    }

    // ---- Media datapoint tier (blocker 2) ----------------------------------

    #[test]
    fn media_datapoint_extra_has_exactly_5_keys() {
        assert_eq!(MEDIA_DATAPOINT_EXTRA.len(), 5);
    }

    #[test]
    fn five_media_keys_survive_on_a_datapoint_with_values_intact() {
        // Distinct, recognisable values: this pins that the filter neither
        // rewrites a value nor cross-assigns one key's value to another.
        let mut req = sum_req_with(vec![
            string_attr("reason", "no_kek_for_generation"),
            string_attr("outcome", "wrap_key_id_mismatch"),
            string_attr("action", "mute"),
            string_attr("source", "join_response"),
            string_attr(KEY_CUSTODY_LABEL, KEY_CUSTODY_OPERATOR),
            string_attr("client_version", "0.0.0"),
            string_attr("participant_id", "p-42"), // not allowlisted → dropped
        ]);
        let counts = filter_metrics(&mut req, TEST_ORG);

        assert_eq!(counts.datapoint, 1, "only the unlisted key is dropped");
        let got = pairs(sum_dp_attrs(&req));
        for expected in [
            ("reason", "no_kek_for_generation"),
            ("outcome", "wrap_key_id_mismatch"),
            ("action", "mute"),
            ("source", "join_response"),
            (KEY_CUSTODY_LABEL, KEY_CUSTODY_OPERATOR),
            ("client_version", "0.0.0"),
        ] {
            assert!(
                got.contains(&(expected.0, expected.1.to_string())),
                "{} must survive with its value byte-identical; got {got:?}",
                expected.0
            );
        }
        assert!(
            !got.iter().any(|(k, _)| *k == "participant_id"),
            "the allowlist must stay closed: an unlisted key is still dropped"
        );
    }

    #[test]
    fn five_media_keys_are_dropped_at_resource_and_span_level() {
        // Pins the tier as DATAPOINT-ONLY, so a later "just add them to
        // ALLOWLIST" refactor fails here rather than silently widening the
        // trace path (which has no collector keep_keys backstop).
        let five = || {
            vec![
                string_attr("reason", "decrypt_failed"),
                string_attr("outcome", "kek_generation_not_held"),
                string_attr("action", "unmute"),
                string_attr("source", "join_response"),
                string_attr(KEY_CUSTODY_LABEL, KEY_CUSTODY_OPERATOR),
            ]
        };

        let mut m = ExportMetricsServiceRequest {
            resource_metrics: vec![ResourceMetrics {
                resource: Some(Resource {
                    attributes: five(),
                    dropped_attributes_count: 0,
                }),
                scope_metrics: vec![],
                schema_url: String::new(),
            }],
        };
        assert_eq!(filter_metrics(&mut m, TEST_ORG).resource, 5);

        let mut t = ExportTraceServiceRequest {
            resource_spans: vec![ResourceSpans {
                resource: None,
                scope_spans: vec![ScopeSpans {
                    scope: None,
                    spans: vec![Span {
                        attributes: five(),
                        ..Default::default()
                    }],
                    schema_url: String::new(),
                }],
                schema_url: String::new(),
            }],
        };
        assert_eq!(filter_traces(&mut t, TEST_ORG).span, 5);
    }

    #[test]
    fn five_media_keys_are_dropped_on_exemplar_attributes() {
        // The ninth call site. Exemplars report as `AttrKind::Datapoint`, so a
        // tier selected from the kind would silently open these keys here.
        let mut req = ExportMetricsServiceRequest {
            resource_metrics: vec![ResourceMetrics {
                resource: None,
                scope_metrics: vec![ScopeMetrics {
                    scope: None,
                    metrics: vec![Metric {
                        data: Some(Data::Sum(Sum {
                            data_points: vec![NumberDataPoint {
                                attributes: vec![],
                                exemplars: vec![Exemplar {
                                    filtered_attributes: vec![
                                        string_attr("reason", "decrypt_failed"),
                                        string_attr("outcome", "wrap_key_id_mismatch"),
                                        string_attr("action", "mute"),
                                        string_attr("source", "join_response"),
                                        string_attr(KEY_CUSTODY_LABEL, KEY_CUSTODY_OPERATOR),
                                    ],
                                    ..Default::default()
                                }],
                                ..Default::default()
                            }],
                            ..Default::default()
                        })),
                        ..Default::default()
                    }],
                    schema_url: String::new(),
                }],
                schema_url: String::new(),
            }],
        };
        filter_metrics(&mut req, TEST_ORG);

        let Some(Data::Sum(sum)) = req.resource_metrics[0].scope_metrics[0].metrics[0]
            .data
            .as_ref()
        else {
            unreachable!("payload was built with a Sum")
        };
        let ex = &sum.data_points[0].exemplars[0];
        assert!(
            !ex.filtered_attributes
                .iter()
                .any(|kv| MEDIA_DATAPOINT_EXTRA.contains(&kv.key.as_str())),
            "exemplars take the BASE tier; got {:?}",
            pairs(&ex.filtered_attributes)
        );
    }

    #[test]
    fn array_value_under_reason_on_a_datapoint_is_dropped() {
        // Value-shape deny-by-default still applies to the new tier.
        let arr = Value::ArrayValue(ArrayValue {
            values: vec![AnyValue {
                value: Some(Value::StringValue("victim@example.com".into())),
            }],
        });
        let mut req = sum_req_with(vec![allowlisted("reason", Some(arr))]);
        let counts = filter_metrics(&mut req, TEST_ORG);

        assert!(
            !sum_dp_attrs(&req).iter().any(|kv| kv.key == "reason"),
            "a non-scalar value under an admitted key must be dropped"
        );
        assert!(counts.datapoint >= 1);
    }

    // ---- key_custody: one legal value, dropped on mismatch -----------------

    #[test]
    fn key_custody_with_a_forged_value_is_dropped_and_counted() {
        let mut req = sum_req_with(vec![string_attr(KEY_CUSTODY_LABEL, "end_to_end")]);
        let counts = filter_metrics(&mut req, TEST_ORG);

        assert!(
            !sum_dp_attrs(&req)
                .iter()
                .any(|kv| kv.key == KEY_CUSTODY_LABEL),
            "a false custody claim must never reach a stored series"
        );
        assert_eq!(counts.datapoint, 1);
    }

    #[test]
    fn key_custody_with_the_one_legal_value_survives() {
        let mut req = sum_req_with(vec![string_attr(KEY_CUSTODY_LABEL, KEY_CUSTODY_OPERATOR)]);
        let counts = filter_metrics(&mut req, TEST_ORG);

        assert!(pairs(sum_dp_attrs(&req))
            .contains(&(KEY_CUSTODY_LABEL, KEY_CUSTODY_OPERATOR.to_string())));
        assert_eq!(counts.datapoint, 0);
    }

    // ---- org_id: stamped from the authenticated claim ----------------------

    #[test]
    fn spoofed_org_id_on_a_datapoint_is_replaced_by_the_claim() {
        let mut req = sum_req_with(vec![string_attr("org_id", "victim-org")]);
        filter_metrics(&mut req, TEST_ORG);

        let got = pairs(sum_dp_attrs(&req));
        assert_eq!(
            got,
            vec![("org_id", TEST_ORG.to_string())],
            "the payload value must be discarded, not compared"
        );
        assert_eq!(
            sum_dp_attrs(&req)
                .iter()
                .filter(|kv| kv.key == "org_id")
                .count(),
            1,
            "exactly one org_id attribute — a buggy insert would duplicate the key"
        );
    }

    #[test]
    fn honest_subdomain_org_id_is_also_replaced_by_the_claim() {
        // The real-input case: an honest client sends the SUBDOMAIN. A filter
        // that compared payload to claim would drop this, destroying 100% of
        // legitimate telemetry while looking like a fail-closed control.
        let mut req = sum_req_with(vec![
            string_attr("org_id", SDK_SENT_ORG),
            string_attr("client_version", "0.0.0"),
        ]);
        let counts = filter_metrics(&mut req, TEST_ORG);

        let got = pairs(sum_dp_attrs(&req));
        assert!(got.contains(&("org_id", TEST_ORG.to_string())));
        assert!(
            got.contains(&("client_version", "0.0.0".to_string())),
            "nothing else may be disturbed by the stamp"
        );
        assert_eq!(
            counts.total(),
            0,
            "stamping is not a drop and is never counted"
        );
    }

    #[test]
    fn datapoint_without_org_id_gains_the_claim_value() {
        let mut req = sum_req_with(vec![string_attr("client_version", "0.0.0")]);
        filter_metrics(&mut req, TEST_ORG);

        let got = pairs(sum_dp_attrs(&req));
        assert!(
            got.contains(&("org_id", TEST_ORG.to_string())),
            "every stored client series must carry the authenticated tenant; got {got:?}"
        );
    }

    #[test]
    fn org_id_is_overwritten_but_never_inserted_outside_datapoints() {
        // Overwrite-if-present at resource and span level...
        let mut m = ExportMetricsServiceRequest {
            resource_metrics: vec![ResourceMetrics {
                resource: Some(Resource {
                    attributes: vec![string_attr("org_id", "victim-org")],
                    dropped_attributes_count: 0,
                }),
                scope_metrics: vec![],
                schema_url: String::new(),
            }],
        };
        filter_metrics(&mut m, TEST_ORG);
        assert_eq!(
            pairs(&m.resource_metrics[0].resource.as_ref().unwrap().attributes),
            vec![("org_id", TEST_ORG.to_string())]
        );

        // ...but a span that carried none must not GAIN one.
        let mut t = ExportTraceServiceRequest {
            resource_spans: vec![ResourceSpans {
                resource: None,
                scope_spans: vec![ScopeSpans {
                    scope: None,
                    spans: vec![Span {
                        attributes: vec![string_attr("meeting_id_hash", "deadbeef")],
                        ..Default::default()
                    }],
                    schema_url: String::new(),
                }],
                schema_url: String::new(),
            }],
        };
        filter_traces(&mut t, TEST_ORG);
        let span_attrs = &t.resource_spans[0].scope_spans[0].spans[0].attributes;
        assert_eq!(
            pairs(span_attrs),
            vec![("meeting_id_hash", "deadbeef".to_string())],
            "spans keep the hash and gain no org_id"
        );
    }

    #[test]
    fn non_scalar_org_id_is_dropped_then_stamped_once() {
        let nested = Value::KvlistValue(KeyValueList {
            values: vec![allowlisted_raw("secret_email", "victim@example.com")],
        });
        let mut req = sum_req_with(vec![allowlisted("org_id", Some(nested))]);
        let counts = filter_metrics(&mut req, TEST_ORG);

        assert_eq!(
            counts.datapoint, 1,
            "the smuggling attempt is counted as a drop"
        );
        assert_eq!(
            pairs(sum_dp_attrs(&req)),
            vec![("org_id", TEST_ORG.to_string())],
            "exactly one scalar org_id carrying the claim value"
        );
    }

    #[test]
    fn empty_claim_drops_org_id_and_inserts_nothing() {
        // Should be unreachable (AC builds the claim from a Uuid). If it ever
        // happens, collapsing every tenant into org_id="" would look healthy —
        // so the attribute goes away instead.
        let mut req = sum_req_with(vec![
            string_attr("org_id", "victim-org"),
            string_attr("client_version", "0.0.0"),
        ]);
        let counts = filter_metrics(&mut req, "");

        let got = pairs(sum_dp_attrs(&req));
        assert!(
            !got.iter().any(|(k, _)| *k == "org_id"),
            "org_id must be ABSENT, not empty-string; got {got:?}"
        );
        assert!(got.contains(&("client_version", "0.0.0".to_string())));
        assert_eq!(counts.datapoint, 1);
    }

    // ---- duplicate org_id keys (GC Gate-3 F1) -------------------------------

    fn org_id_count(attrs: &[KeyValue]) -> usize {
        attrs.iter().filter(|kv| kv.key == "org_id").count()
    }

    #[test]
    fn duplicate_org_id_on_a_datapoint_collapses_to_exactly_one_claim_value() {
        let mut req = sum_req_with(vec![
            string_attr("org_id", "victim-org"),
            string_attr("client_version", "0.0.0"),
            string_attr("org_id", "other-org"),
        ]);
        let counts = filter_metrics(&mut req, TEST_ORG);

        let attrs = sum_dp_attrs(&req);
        assert_eq!(
            org_id_count(attrs),
            1,
            "repeated keys must not be forwarded"
        );
        assert!(pairs(attrs).contains(&("org_id", TEST_ORG.to_string())));
        assert_eq!(
            counts.datapoint, 0,
            "removing duplicate copies is stamping, not refusing — never counted as a drop"
        );
    }

    #[test]
    fn duplicate_org_id_on_a_span_collapses_to_exactly_one_claim_value() {
        // The overwrite-only path: the span HAD org_id, so it keeps exactly one.
        let mut t = ExportTraceServiceRequest {
            resource_spans: vec![ResourceSpans {
                resource: None,
                scope_spans: vec![ScopeSpans {
                    scope: None,
                    spans: vec![Span {
                        attributes: vec![string_attr("org_id", "a"), string_attr("org_id", "b")],
                        ..Default::default()
                    }],
                    schema_url: String::new(),
                }],
                schema_url: String::new(),
            }],
        };
        let counts = filter_traces(&mut t, TEST_ORG);

        let attrs = &t.resource_spans[0].scope_spans[0].spans[0].attributes;
        assert_eq!(pairs(attrs), vec![("org_id", TEST_ORG.to_string())]);
        assert_eq!(counts.span, 0);
    }
}
