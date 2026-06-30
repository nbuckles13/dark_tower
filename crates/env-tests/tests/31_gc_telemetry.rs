//! P1 Tests: GC telemetry proxy (`POST /api/v1/telemetry/v1/{metrics,traces}`)
//!
//! Cluster-level env-tests for the GC OTLP telemetry proxy introduced by task
//! #10. The `#[sqlx::test]` handler tests in `crates/gc-service/tests/` cover
//! handler logic against a postgres sidecar; these verify the DEPLOYED
//! configuration end-to-end: real user-auth (via AC), the real per-user rate
//! limit, the real two-tier size cap, the real Content-Type gate, and the real
//! deny-by-default PII filter forwarding to the real in-cluster OTel collector.
//!
//! # Verification of the PII filter (the crux — see devloop-output main.md §R1)
//! The dev OTel collector's debug exporter runs at `verbosity: normal`
//! (`infra/services/otel-collector/configmap.yaml`), which logs metric/span
//! names+counts only — never attribute keys/values (a deliberate @security
//! control so PII never lands in the collector pod log). So we CANNOT read back
//! the forwarded attributes from the collector. Instead the filter is verified
//! two ways that ARE observable: (1) a **202** is returned only after the
//! handler forwards the filtered bytes and the live collector accepts them
//! (`handlers/telemetry.rs:317-327`) — so 202 ⇔ collector reachable + accepted
//! (this also makes every test fail LOUDLY with a 502 if the collector is down);
//! (2) GC's own `gc_telemetry_pii_attributes_dropped_total{kind}` counter proves
//! the filter actually dropped the planted attribute.
//!
//! # Prerequisites
//! - Kind cluster with AC + GC deployed; AC `devtest` org seeded.
//! - GC telemetry config = compiled defaults (the ConfigMap does NOT override
//!   them): max payload 256 KiB, per-user rate limit 60/min (per-pod, in-memory).
//!   See the reverse cross-ref comment in `infra/services/gc-service/configmap.yaml`.

#![cfg(feature = "flows")]

use env_tests::cluster::ClusterConnection;
use env_tests::fixtures::auth_client::{TokenRequest, UserRegistrationRequest};
use env_tests::fixtures::gc_client::GcClient;
use env_tests::fixtures::AuthClient;
use prost::Message;
use tokio::sync::OnceCell;

use opentelemetry_proto::tonic::collector::metrics::v1::ExportMetricsServiceRequest;
use opentelemetry_proto::tonic::collector::trace::v1::ExportTraceServiceRequest;
use opentelemetry_proto::tonic::common::v1::{any_value::Value, AnyValue, KeyValue, KeyValueList};
use opentelemetry_proto::tonic::metrics::v1::{
    metric::Data, Gauge, Metric, NumberDataPoint, ResourceMetrics, ScopeMetrics,
};
use opentelemetry_proto::tonic::resource::v1::Resource;
use opentelemetry_proto::tonic::trace::v1::{ResourceSpans, ScopeSpans, Span};

// PII-filter verification (C2′) reads GC's drop counter via Prometheus, aggregated
// across the 2 GC replicas — only needed under the `observability` feature
// (flows-only ⇒ the (g)/(h)/(t-c) tests degrade to 202-only).
#[cfg(feature = "observability")]
use env_tests::eventual::{assert_eventually, ConsistencyCategory};
#[cfg(feature = "observability")]
use env_tests::fixtures::PrometheusClient;

// ============================================================================
// Constants
// ============================================================================

/// The only Content-Type the proxy accepts (else 415).
const PROTOBUF_CT: &str = "application/x-protobuf";

/// A body comfortably above the 256 KiB handler cap but below the route's
/// `2×` `DefaultBodyLimit` ceiling (512 KiB), so it reaches the HANDLER's
/// load-bearing 413 gate rather than being dropped by the tower layer.
const OVERSIZE_BYTES: usize = 300 * 1024;

// ============================================================================
// Shared scaffolding (mirrors 24_join_flow.rs)
// ============================================================================

/// Shared cluster connection (initialized once, reused across all tests).
static CLUSTER: OnceCell<ClusterConnection> = OnceCell::const_new();

/// Shared user JWT for cases that just need *a* valid user token. Registered
/// once via AC to keep `auth_events` noise (and registration count) down.
static SHARED_USER: OnceCell<String> = OnceCell::const_new();

/// Dedicated user JWT for the rate-limit test (f), so exhausting its per-`sub`
/// GCRA window can't 429 the shared user's tests (matrix-d isolation).
static RATELIMIT_USER: OnceCell<String> = OnceCell::const_new();

/// Helper to get a cluster connection, verifying AC + GC are available.
async fn cluster() -> &'static ClusterConnection {
    CLUSTER
        .get_or_init(|| async {
            let cluster = ClusterConnection::new()
                .await
                .expect("Failed to connect to cluster - ensure port-forwards are running");
            cluster
                .check_ac_health()
                .await
                .expect("AC service must be running for telemetry proxy tests");
            cluster
                .check_gc_health()
                .await
                .expect("GC service must be running for telemetry proxy tests");
            cluster
        })
        .await
}

/// Register a fresh user via AC and return its JWT access token.
async fn register_user(auth: &AuthClient, display_name: &str) -> String {
    let request = UserRegistrationRequest::unique(display_name);
    auth.register_user(&request)
        .await
        .expect("AC should register test user")
        .access_token
}

/// A shared, valid user JWT (registered once).
async fn shared_user_token(cluster: &ClusterConnection) -> &'static str {
    SHARED_USER
        .get_or_init(|| async {
            let auth = AuthClient::new(&cluster.ac_base_url);
            register_user(&auth, "Telemetry Shared User").await
        })
        .await
        .as_str()
}

/// A dedicated user JWT for the rate-limit test (registered once).
async fn ratelimit_user_token(cluster: &ClusterConnection) -> &'static str {
    RATELIMIT_USER
        .get_or_init(|| async {
            let auth = AuthClient::new(&cluster.ac_base_url);
            register_user(&auth, "Telemetry Rate-Limit User").await
        })
        .await
        .as_str()
}

/// A service (client-credentials) token — NOT a user token. The seeded
/// `test-client` credential (`infra/kind/scripts/setup.sh`) is the same one
/// `24_join_flow.rs` uses for its service-token rejection test.
async fn service_token(cluster: &ClusterConnection) -> String {
    let auth = AuthClient::new(&cluster.ac_base_url);
    let request =
        TokenRequest::client_credentials("test-client", "test-client-secret-dev-999", "test:all");
    auth.issue_token(request)
        .await
        .expect("AC should issue a service token")
        .access_token
}

// ============================================================================
// OTLP payload builders (return ENCODED Vec<u8> bodies — mirrors the builder
// shape in crates/gc-service/tests/telemetry_proxy_tests.rs)
// ============================================================================

/// A `KeyValue` carrying a scalar string value.
fn string_attr(key: &str, val: &str) -> KeyValue {
    KeyValue {
        key: key.to_string(),
        value: Some(AnyValue {
            value: Some(Value::StringValue(val.to_string())),
        }),
    }
}

/// A minimal, valid OTLP metrics export: one resource (allowlisted `org_id`
/// scalar only — survives the filter) with one gauge datapoint. Encodes, decodes,
/// filters to a no-op, and forwards cleanly → 202.
fn minimal_metrics_payload() -> Vec<u8> {
    ExportMetricsServiceRequest {
        resource_metrics: vec![ResourceMetrics {
            resource: Some(Resource {
                attributes: vec![string_attr("org_id", "env-test")],
                dropped_attributes_count: 0,
            }),
            scope_metrics: vec![ScopeMetrics {
                scope: None,
                metrics: vec![Metric {
                    data: Some(Data::Gauge(Gauge {
                        data_points: vec![NumberDataPoint::default()],
                    })),
                    ..Default::default()
                }],
                schema_url: String::new(),
            }],
            schema_url: String::new(),
        }],
    }
    .encode_to_vec()
}

/// A minimal, valid OTLP traces export: one resource span with one clean span.
fn minimal_traces_payload() -> Vec<u8> {
    ExportTraceServiceRequest {
        resource_spans: vec![ResourceSpans {
            resource: Some(Resource {
                attributes: vec![string_attr("org_id", "env-test")],
                dropped_attributes_count: 0,
            }),
            scope_spans: vec![ScopeSpans {
                scope: None,
                spans: vec![Span {
                    trace_id: vec![1u8; 16],
                    span_id: vec![1u8; 8],
                    name: "env-test-span".to_string(),
                    ..Default::default()
                }],
                schema_url: String::new(),
            }],
            schema_url: String::new(),
        }],
    }
    .encode_to_vec()
}

/// (g) builder: a non-allowlisted attribute key at RESOURCE level (synthetic
/// value per R6), beside an allowlisted scalar that survives. Drops exactly one
/// attribute at `kind="resource"` (the datapoint carries no attributes).
fn metrics_payload_pii_resource() -> Vec<u8> {
    ExportMetricsServiceRequest {
        resource_metrics: vec![ResourceMetrics {
            resource: Some(Resource {
                attributes: vec![
                    string_attr("org_id", "env-test"), // allowlisted scalar → kept
                    string_attr("dt_not_allowlisted", "synthetic-pii-marker"), // → dropped
                ],
                dropped_attributes_count: 0,
            }),
            scope_metrics: vec![ScopeMetrics {
                scope: None,
                metrics: vec![Metric {
                    data: Some(Data::Gauge(Gauge {
                        data_points: vec![NumberDataPoint::default()],
                    })),
                    ..Default::default()
                }],
                schema_url: String::new(),
            }],
            schema_url: String::new(),
        }],
    }
    .encode_to_vec()
}

/// (h) builder: an ALLOWLISTED key (`org_id`) at DATAPOINT level whose value is a
/// NON-scalar (`KvlistValue` carrying a synthetic value) — the scalar-only rule
/// drops the whole attribute. Drops exactly one at `kind="datapoint"` (no
/// resource/scope attributes present).
fn metrics_payload_pii_nonscalar_datapoint() -> Vec<u8> {
    let nested = Value::KvlistValue(KeyValueList {
        values: vec![string_attr("smuggled", "synthetic-pii-marker")],
    });
    ExportMetricsServiceRequest {
        resource_metrics: vec![ResourceMetrics {
            resource: None,
            scope_metrics: vec![ScopeMetrics {
                scope: None,
                metrics: vec![Metric {
                    data: Some(Data::Gauge(Gauge {
                        data_points: vec![NumberDataPoint {
                            attributes: vec![KeyValue {
                                key: "org_id".to_string(),
                                value: Some(AnyValue {
                                    value: Some(nested),
                                }),
                            }],
                            ..Default::default()
                        }],
                    })),
                    ..Default::default()
                }],
                schema_url: String::new(),
            }],
            schema_url: String::new(),
        }],
    }
    .encode_to_vec()
}

/// (t-c) builder: a non-allowlisted attribute key at SPAN level (synthetic value
/// per R6), beside an allowlisted scalar that survives. Drops exactly one at
/// `kind="span"`.
fn traces_payload_pii_span() -> Vec<u8> {
    ExportTraceServiceRequest {
        resource_spans: vec![ResourceSpans {
            resource: None,
            scope_spans: vec![ScopeSpans {
                scope: None,
                spans: vec![Span {
                    trace_id: vec![1u8; 16],
                    span_id: vec![1u8; 8],
                    name: "env-test-span".to_string(),
                    attributes: vec![
                        string_attr("org_id", "env-test"), // allowlisted scalar → kept
                        string_attr("dt_not_allowlisted", "synthetic-pii-marker"), // → dropped
                    ],
                    ..Default::default()
                }],
                schema_url: String::new(),
            }],
            schema_url: String::new(),
        }],
    }
    .encode_to_vec()
}

// ============================================================================
// PII-filter verification helper (C2′ — ratified). Reads GC's drop counter via
// PromQL, SUMMED across the 2 GC replicas (each pod is a separate target), so the
// assertion is correct regardless of which pod served the POST. Absent/empty
// vector ⇒ 0 (R2). Only compiled under the `observability` feature.
// ============================================================================

/// Summed value of `gc_telemetry_pii_attributes_dropped_total{kind=…}` across all
/// GC pods. Returns 0.0 when the series does not exist yet (R2 absent ⇒ 0).
#[cfg(feature = "observability")]
async fn pii_dropped_sum(cluster: &ClusterConnection, kind: &str) -> f64 {
    let prom = PrometheusClient::new(&cluster.prometheus_base_url);
    let query = format!("sum(gc_telemetry_pii_attributes_dropped_total{{kind=\"{kind}\"}})");
    let resp = prom
        .query_promql(&query)
        .await
        .expect("Prometheus drop-counter query should succeed");
    resp.data
        .result
        .first()
        .and_then(|r| r.value.as_ref())
        .and_then(|(_, v)| v.parse::<f64>().ok())
        .unwrap_or(0.0)
}

/// Assert the summed `{kind}` drop counter rises by at least `expected` relative
/// to `before`, polling within the `MetricsScrape` budget (scrape lag). `≥`
/// tolerates concurrent increments; R7 keeps one writer per `{kind}` sum-series.
#[cfg(feature = "observability")]
async fn assert_drop_counter_increased(
    cluster: &'static ClusterConnection,
    kind: &'static str,
    before: f64,
    expected: f64,
) {
    assert_eventually(ConsistencyCategory::MetricsScrape, || async move {
        pii_dropped_sum(cluster, kind).await >= before + expected
    })
    .await
    .unwrap_or_else(|_| {
        panic!(
            "sum(gc_telemetry_pii_attributes_dropped_total{{kind=\"{kind}\"}}) did not rise by \
             >= {expected} within the MetricsScrape budget (before = {before}); the deployed PII \
             filter may not have dropped the planted attribute"
        )
    });
}

// ============================================================================
// Metrics: auth + size + content-type
// ============================================================================

/// (a) Authenticated user POST of a minimal valid OTLP-metrics payload → 202.
///
/// Also the collector-liveness detector (verification-matrix b): a 202 is
/// returned only after the filtered bytes are forwarded AND the live collector
/// accepts them, so a down/unreachable collector surfaces here as a 502.
#[tokio::test]
async fn test_metrics_authed_post_returns_202() {
    let cluster = cluster().await;
    let token = shared_user_token(cluster).await;
    let gc = GcClient::new(&cluster.gc_base_url);

    let resp = gc
        .raw_ingest_telemetry(
            "metrics",
            Some(token),
            Some(PROTOBUF_CT),
            minimal_metrics_payload(),
        )
        .await
        .expect("network request should succeed");

    let status = resp.status().as_u16();
    assert_eq!(
        status, 202,
        "authed minimal OTLP-metrics POST should be 202 Accepted; got {status} \
         (a 502 here means the GC→collector forward failed — collector down/unreachable)"
    );
}

/// (b) Unauthenticated POST → 401.
#[tokio::test]
async fn test_metrics_unauthenticated_returns_401() {
    let cluster = cluster().await;
    let gc = GcClient::new(&cluster.gc_base_url);

    let resp = gc
        .raw_ingest_telemetry(
            "metrics",
            None,
            Some(PROTOBUF_CT),
            minimal_metrics_payload(),
        )
        .await
        .expect("network request should succeed");

    assert_eq!(
        resp.status().as_u16(),
        401,
        "unauthenticated telemetry POST should be rejected by require_user_auth (401)"
    );
}

/// (c) Service (client-credentials) token → 401.
///
/// `require_user_auth` validates the token as a USER token (`validate_user`),
/// which rejects a service token — matches `24_join_flow.rs`'s ruling that GC
/// returns 401 (not 403) for a service token on a user-auth route.
#[tokio::test]
async fn test_metrics_service_token_returns_401() {
    let cluster = cluster().await;
    let svc_token = service_token(cluster).await;
    let gc = GcClient::new(&cluster.gc_base_url);

    let resp = gc
        .raw_ingest_telemetry(
            "metrics",
            Some(&svc_token),
            Some(PROTOBUF_CT),
            minimal_metrics_payload(),
        )
        .await
        .expect("network request should succeed");

    assert_eq!(
        resp.status().as_u16(),
        401,
        "a service token must be rejected by require_user_auth (401)"
    );
}

/// (d) Oversize body (> max_bytes) → 413.
#[tokio::test]
async fn test_metrics_oversize_returns_413() {
    let cluster = cluster().await;
    let token = shared_user_token(cluster).await;
    let gc = GcClient::new(&cluster.gc_base_url);

    // 300 KiB: above the 256 KiB handler cap, below the 512 KiB layer ceiling,
    // so it exercises the handler's load-bearing 413 gate (not the tower layer).
    let resp = gc
        .raw_ingest_telemetry(
            "metrics",
            Some(token),
            Some(PROTOBUF_CT),
            vec![0u8; OVERSIZE_BYTES],
        )
        .await
        .expect("network request should succeed");

    assert_eq!(
        resp.status().as_u16(),
        413,
        "a body above the configured max_bytes should be rejected with 413"
    );
}

/// (e) Wrong Content-Type → 415.
#[tokio::test]
async fn test_metrics_wrong_content_type_returns_415() {
    let cluster = cluster().await;
    let token = shared_user_token(cluster).await;
    let gc = GcClient::new(&cluster.gc_base_url);

    let resp = gc
        .raw_ingest_telemetry(
            "metrics",
            Some(token),
            Some("application/json"),
            minimal_metrics_payload(),
        )
        .await
        .expect("network request should succeed");

    assert_eq!(
        resp.status().as_u16(),
        415,
        "a non-protobuf Content-Type should be rejected with 415"
    );
}

// ============================================================================
// Traces: parallel-structure subset (auth)
// ============================================================================

/// (t-a) Authenticated user POST of a minimal valid OTLP-traces payload → 202.
#[tokio::test]
async fn test_traces_authed_post_returns_202() {
    let cluster = cluster().await;
    let token = shared_user_token(cluster).await;
    let gc = GcClient::new(&cluster.gc_base_url);

    let resp = gc
        .raw_ingest_telemetry(
            "traces",
            Some(token),
            Some(PROTOBUF_CT),
            minimal_traces_payload(),
        )
        .await
        .expect("network request should succeed");

    let status = resp.status().as_u16();
    assert_eq!(
        status, 202,
        "authed minimal OTLP-traces POST should be 202 Accepted; got {status} \
         (a 502 here means the GC→collector forward failed — collector down/unreachable)"
    );
}

/// (t-b) Unauthenticated traces POST → 401.
#[tokio::test]
async fn test_traces_unauthenticated_returns_401() {
    let cluster = cluster().await;
    let gc = GcClient::new(&cluster.gc_base_url);

    let resp = gc
        .raw_ingest_telemetry("traces", None, Some(PROTOBUF_CT), minimal_traces_payload())
        .await
        .expect("network request should succeed");

    assert_eq!(
        resp.status().as_u16(),
        401,
        "unauthenticated traces POST should be rejected by require_user_auth (401)"
    );
}

// ============================================================================
// Metrics: per-user rate limit (C1′ — ratified by @test)
// ============================================================================

/// (f) Per-user rate limit → at least one 429 under a concurrent burst.
///
/// The telemetry GCRA limiter is per-pod, in-memory (`handlers/telemetry.rs`),
/// and GC runs 2 replicas behind a load-balancing NodePort, so a single user's
/// requests are split across pods. We fire M concurrent POSTs (wall-clock ≪ the
/// 1/sec replenishment window) on a DEDICATED user `sub` so the burst is
/// attributable and isolated from the shared user's window (matrix-d).
#[tokio::test]
async fn test_metrics_rate_limit_returns_429() {
    let cluster = cluster().await;
    let token = ratelimit_user_token(cluster).await;
    let gc = std::sync::Arc::new(GcClient::new(&cluster.gc_base_url));

    // M=130 > replicas(2) × telemetry burst(60) = 120 ⇒ ≥1 pod exceeds burst ⇒ ≥1 429 guaranteed.
    // COUPLED to: gc-service replicas (deployment.yaml:10) AND TELEMETRY_PROXY_RATE_LIMIT_PER_MINUTE
    // (compiled default 60; NOT set in gc-service/configmap.yaml today — if a ConfigMap later sets
    // it, burst changes too). Recompute M > replicas × burst if either moves (3 replicas ⇒ M ≥ 181).
    const M: usize = 130;

    let mut set = tokio::task::JoinSet::new();
    for _ in 0..M {
        let gc = gc.clone();
        set.spawn(async move {
            gc.raw_ingest_telemetry(
                "metrics",
                Some(token),
                Some(PROTOBUF_CT),
                minimal_metrics_payload(),
            )
            .await
            .map(|r| r.status().as_u16())
        });
    }

    let (mut n_202, mut n_429, mut n_other, mut n_err) = (0u32, 0u32, 0u32, 0u32);
    while let Some(joined) = set.join_next().await {
        match joined.expect("rate-limit request task should not panic") {
            Ok(202) => n_202 += 1,
            Ok(429) => n_429 += 1,
            Ok(_) => n_other += 1,
            Err(_) => n_err += 1, // reqwest network error
        }
    }

    // PRIMARY (collector-independent: the GCRA `check_key` precedes the collector forward).
    assert!(
        n_429 >= 1,
        "expected >=1 of {M} concurrent telemetry POSTs to be rate-limited (429); \
         got n_202={n_202} n_429={n_429} n_other={n_other} n_err={n_err}. \
         M must exceed replicas x burst (2 x 60 = 120) — check gc-service replicas/burst if this drifted."
    );
    // SECONDARY collector-liveness (matrix b): some POSTs were accepted + forwarded.
    // Under a down collector these would be 502 (counted in n_other), failing loudly here.
    assert!(
        n_202 >= 1,
        "expected >=1 accepted (202) among {M} POSTs; got n_202={n_202} n_429={n_429} \
         n_other={n_other} n_err={n_err} (all-non-202 suggests the GC->collector forward is failing)"
    );
}

// ============================================================================
// PII filter (C2′ — ratified). Each test plants at a DISTINCT nesting level so
// its `{kind}` sum-series has exactly one writer in the suite (R7). Under
// `flows`-only these degrade to 202-only (filter strips, not rejects); under
// `observability` they ALSO assert the deployed filter dropped the attribute via
// the summed drop counter.
// ============================================================================

/// (g) Non-allowlisted attribute key (RESOURCE level) → 202 (filtered, not
/// rejected) + the `{kind="resource"}` drop counter rises.
#[tokio::test]
async fn test_metrics_pii_nonallowlisted_key_filtered() {
    let cluster = cluster().await;
    let token = shared_user_token(cluster).await;
    let gc = GcClient::new(&cluster.gc_base_url);

    #[cfg(feature = "observability")]
    let before = pii_dropped_sum(cluster, "resource").await;

    let resp = gc
        .raw_ingest_telemetry(
            "metrics",
            Some(token),
            Some(PROTOBUF_CT),
            metrics_payload_pii_resource(),
        )
        .await
        .expect("network request should succeed");
    assert_eq!(
        resp.status().as_u16(),
        202,
        "a non-allowlisted attribute must be FILTERED (202), not rejected; \
         a 502 here means the GC→collector forward failed (collector down)"
    );

    #[cfg(feature = "observability")]
    assert_drop_counter_increased(cluster, "resource", before, 1.0).await;
}

/// (h) Non-scalar value on an ALLOWLISTED key (DATAPOINT level) → 202 + the
/// `{kind="datapoint"}` drop counter rises (scalar-only rule).
#[tokio::test]
async fn test_metrics_pii_nonscalar_value_dropped() {
    let cluster = cluster().await;
    let token = shared_user_token(cluster).await;
    let gc = GcClient::new(&cluster.gc_base_url);

    #[cfg(feature = "observability")]
    let before = pii_dropped_sum(cluster, "datapoint").await;

    let resp = gc
        .raw_ingest_telemetry(
            "metrics",
            Some(token),
            Some(PROTOBUF_CT),
            metrics_payload_pii_nonscalar_datapoint(),
        )
        .await
        .expect("network request should succeed");
    assert_eq!(
        resp.status().as_u16(),
        202,
        "a non-scalar value on an allowlisted key must be FILTERED (202), not rejected; \
         a 502 here means the GC→collector forward failed (collector down)"
    );

    #[cfg(feature = "observability")]
    assert_drop_counter_increased(cluster, "datapoint", before, 1.0).await;
}

/// (t-c) Non-allowlisted SPAN attribute → 202 + the `{kind="span"}` drop counter
/// rises.
#[tokio::test]
async fn test_traces_pii_nonallowlisted_span_attr_filtered() {
    let cluster = cluster().await;
    let token = shared_user_token(cluster).await;
    let gc = GcClient::new(&cluster.gc_base_url);

    #[cfg(feature = "observability")]
    let before = pii_dropped_sum(cluster, "span").await;

    let resp = gc
        .raw_ingest_telemetry(
            "traces",
            Some(token),
            Some(PROTOBUF_CT),
            traces_payload_pii_span(),
        )
        .await
        .expect("network request should succeed");
    assert_eq!(
        resp.status().as_u16(),
        202,
        "a non-allowlisted span attribute must be FILTERED (202), not rejected; \
         a 502 here means the GC→collector forward failed (collector down)"
    );

    #[cfg(feature = "observability")]
    assert_drop_counter_increased(cluster, "span", before, 1.0).await;
}
