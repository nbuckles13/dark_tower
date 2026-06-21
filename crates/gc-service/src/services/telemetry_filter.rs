//! Deny-by-default PII filter for the telemetry proxy (R-2).
//!
//! This is the security-load-bearing part of the proxy: GC is the trust
//! boundary between a browser (which the user controls) and the in-cluster OTel
//! collector. Every attribute whose key is NOT in the 12-key [`ALLOWLIST`] is
//! dropped at EVERY nesting level of the OTLP message before the payload is
//! re-encoded and forwarded. The filter operates on attribute KEYS only and
//! never descends into attribute VALUES (`AnyValue`), so traversal is strictly
//! fixed-depth over the structural message tree — there is no user-controlled
//! recursion.
//!
//! After filtering, the caller re-encodes the mutated request with `prost` and
//! forwards ONLY the re-encoded bytes — the original body is never proxied.

use crate::observability::metrics;
use opentelemetry_proto::tonic::collector::metrics::v1::ExportMetricsServiceRequest;
use opentelemetry_proto::tonic::collector::trace::v1::ExportTraceServiceRequest;
use opentelemetry_proto::tonic::common::v1::any_value::Value;
use opentelemetry_proto::tonic::common::v1::KeyValue;
use opentelemetry_proto::tonic::metrics::v1::metric::Data;

/// The 12-key attribute allowlist. Any attribute key not in this set is dropped
/// at every level. Deny-by-default: adding a new business attribute requires a
/// change here (intended friction).
pub const ALLOWLIST: [&str; 12] = [
    "client_version",
    "service.name",
    "service.version",
    "meeting_id_hash",
    "org_id",
    "dt.event",
    "dt.duration_ms",
    "dt.failure_stage",
    "dt.close_reason",
    "dt.mh_index_bucket",
    "http.status_code",
    "error.code",
];

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

/// Drop every attribute whose key is not allowlisted, returning the number
/// dropped. Single source of truth for the allowlist check — called at every
/// nesting site.
fn filter_attrs(attrs: &mut Vec<KeyValue>, kind: AttrKind, counts: &mut DropCounts) {
    let before = attrs.len();
    // Deny-by-default on BOTH key and value shape. An attribute survives only if
    // its key is allowlisted AND its value is a scalar (or absent). A non-scalar
    // value — `ArrayValue` / `KvlistValue` (nested attributes) or `BytesValue`
    // (opaque blob) — can smuggle arbitrary PII under an allowlisted key, so the
    // whole attribute is dropped. We do NOT deep-recurse into the value (that
    // would be an unbounded, attacker-controlled-depth traversal); dropping the
    // non-scalar is both bounded and the safer deny-by-default posture.
    attrs.retain(|kv| ALLOWLIST.contains(&kv.key.as_str()) && value_is_scalar(kv));
    let dropped = (before - attrs.len()) as u64;
    counts.add(kind, dropped);
}

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
pub fn filter_metrics(req: &mut ExportMetricsServiceRequest) -> DropCounts {
    let mut counts = DropCounts::default();

    for rm in &mut req.resource_metrics {
        if let Some(resource) = rm.resource.as_mut() {
            filter_attrs(&mut resource.attributes, AttrKind::Resource, &mut counts);
        }
        for sm in &mut rm.scope_metrics {
            if let Some(scope) = sm.scope.as_mut() {
                filter_attrs(&mut scope.attributes, AttrKind::Scope, &mut counts);
            }
            for metric in &mut sm.metrics {
                // Each data variant carries its own datapoint type; only
                // NumberDataPoint/Histogram/ExponentialHistogram have exemplars
                // (SummaryDataPoint does NOT).
                match metric.data.as_mut() {
                    Some(Data::Gauge(g)) => {
                        for dp in &mut g.data_points {
                            filter_attrs(&mut dp.attributes, AttrKind::Datapoint, &mut counts);
                            for ex in &mut dp.exemplars {
                                filter_attrs(
                                    &mut ex.filtered_attributes,
                                    AttrKind::Datapoint,
                                    &mut counts,
                                );
                            }
                        }
                    }
                    Some(Data::Sum(s)) => {
                        for dp in &mut s.data_points {
                            filter_attrs(&mut dp.attributes, AttrKind::Datapoint, &mut counts);
                            for ex in &mut dp.exemplars {
                                filter_attrs(
                                    &mut ex.filtered_attributes,
                                    AttrKind::Datapoint,
                                    &mut counts,
                                );
                            }
                        }
                    }
                    Some(Data::Histogram(h)) => {
                        for dp in &mut h.data_points {
                            filter_attrs(&mut dp.attributes, AttrKind::Datapoint, &mut counts);
                            for ex in &mut dp.exemplars {
                                filter_attrs(
                                    &mut ex.filtered_attributes,
                                    AttrKind::Datapoint,
                                    &mut counts,
                                );
                            }
                        }
                    }
                    Some(Data::ExponentialHistogram(eh)) => {
                        for dp in &mut eh.data_points {
                            filter_attrs(&mut dp.attributes, AttrKind::Datapoint, &mut counts);
                            for ex in &mut dp.exemplars {
                                filter_attrs(
                                    &mut ex.filtered_attributes,
                                    AttrKind::Datapoint,
                                    &mut counts,
                                );
                            }
                        }
                    }
                    Some(Data::Summary(sum)) => {
                        // SummaryDataPoint has no `exemplars` field — only attributes.
                        for dp in &mut sum.data_points {
                            filter_attrs(&mut dp.attributes, AttrKind::Datapoint, &mut counts);
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
pub fn filter_traces(req: &mut ExportTraceServiceRequest) -> DropCounts {
    let mut counts = DropCounts::default();

    for rs in &mut req.resource_spans {
        if let Some(resource) = rs.resource.as_mut() {
            filter_attrs(&mut resource.attributes, AttrKind::Resource, &mut counts);
        }
        for ss in &mut rs.scope_spans {
            if let Some(scope) = ss.scope.as_mut() {
                filter_attrs(&mut scope.attributes, AttrKind::Scope, &mut counts);
            }
            for span in &mut ss.spans {
                filter_attrs(&mut span.attributes, AttrKind::Span, &mut counts);
                // Span.status { message, code } has no attributes — nothing to filter.
                for event in &mut span.events {
                    filter_attrs(&mut event.attributes, AttrKind::SpanEvent, &mut counts);
                }
                for link in &mut span.links {
                    filter_attrs(&mut link.attributes, AttrKind::SpanLink, &mut counts);
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
        filter_attrs(&mut attrs, AttrKind::Resource, &mut counts);

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
        filter_attrs(&mut attrs, AttrKind::Span, &mut counts);

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
        filter_attrs(&mut attrs, AttrKind::Resource, &mut counts);

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
        filter_attrs(&mut attrs, AttrKind::Resource, &mut counts);

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
        filter_attrs(&mut attrs, AttrKind::Span, &mut counts);

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
        filter_attrs(&mut attrs, AttrKind::SpanEvent, &mut counts);

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
        let counts = filter_metrics(&mut req);

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
            let counts = filter_metrics(&mut req);
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

        let counts = filter_traces(&mut req);
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

        let counts = filter_traces(&mut req);
        assert_eq!(counts.span, 0, "clean span attrs must not be counted");
        assert_eq!(
            counts.span_event, 1,
            "event drop must be labeled span_event"
        );
        assert_eq!(counts.span_link, 0);
    }
}
